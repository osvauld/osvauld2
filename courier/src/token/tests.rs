use std::collections::HashSet;

use super::*;

fn workspace(id: &str) -> Scope {
    Scope::Workspace(id.to_string())
}

fn app(id: &str) -> Scope {
    Scope::App {
        ws: "shop".to_string(),
        app: id.to_string(),
    }
}

fn root(node: &Identity, aud: &Identity, scope: Scope, delegable: bool) -> Token {
    issue_root(node, aud.did(), "maintainer", scope, delegable, 1, 100).unwrap()
}

fn pass(parent: &Token, holder: &Identity, aud: &Identity, scope: Scope) -> Token {
    delegate(parent, holder, aud.did(), scope, false, 2, 90).unwrap()
}

fn none() -> HashSet<[u8; 32]> {
    HashSet::new()
}

#[test]
fn root_is_self_issued_by_the_node() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let claims = root(&node, &admin, workspace("shop"), true)
        .claims()
        .unwrap();

    assert_eq!(claims.iss, node.did());
    assert_eq!(claims.sub, node.did());
    assert_eq!(claims.aud, admin.did());
    assert_eq!(claims.prf, None);
}

#[test]
fn delegation_embeds_parent_and_inherits_role_and_subject() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (member, _) = identity::generate();
    let parent = root(&node, &admin, workspace("shop"), true);
    let claims = pass(&parent, &admin, &member, app("storefront"))
        .claims()
        .unwrap();

    assert_eq!(claims.iss, admin.did());
    assert_eq!(claims.aud, member.did());
    assert_eq!(claims.sub, node.did());
    assert_eq!(claims.role, "maintainer");
    assert_eq!(claims.prf, Some(parent));
}

#[test]
fn signature_covers_domain_and_payload() {
    let (node, _) = identity::generate();
    let token = root(&node, &node, workspace("shop"), true);
    let sig: [u8; 64] = token.sig.as_slice().try_into().unwrap();
    let key = identity::public_key_from_did(node.did()).unwrap();

    assert!(identity::verify(
        &key,
        &[TOKEN_DOMAIN, &token.payload].concat(),
        &sig
    ));
    assert!(!identity::verify(&key, &token.payload, &sig));
}

#[test]
fn id_names_the_payload_not_the_signature() {
    let (node, _) = identity::generate();
    let token = root(&node, &node, workspace("shop"), true);
    let mut resigned = token.clone();
    resigned.sig[0] ^= 1;

    assert_eq!(token.id(), resigned.id());
    assert_ne!(token.id(), root(&node, &node, workspace("shop"), true).id());
}

#[test]
fn a_delegated_chain_verifies_and_returns_the_leaf() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (member, _) = identity::generate();
    let parent = root(&node, &admin, workspace("shop"), true);
    let leaf = pass(&parent, &admin, &member, app("storefront"));

    let claims = verify_chain(&leaf, node.did(), member.did(), 10, &none()).unwrap();
    assert_eq!(claims.aud, member.did());
    assert_eq!(claims.scope, app("storefront"));
    assert!(verify_chain(&parent, node.did(), admin.did(), 10, &none()).is_ok());
}

#[test]
fn rewritten_claims_do_not_keep_the_old_signature() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let token = root(&node, &admin, workspace("shop"), true);
    let mut claims = token.claims().unwrap();
    claims.role = "owner".to_string();
    let forged = Token {
        payload: bincode::serialize(&claims).unwrap(),
        sig: token.sig.clone(),
    };

    assert_eq!(
        verify_chain(&forged, node.did(), admin.did(), 10, &none()).unwrap_err(),
        CourierError::BadSignature
    );
}

#[test]
fn a_token_from_another_node_is_rejected() {
    let (node, _) = identity::generate();
    let (other, _) = identity::generate();
    let (admin, _) = identity::generate();
    let token = root(&other, &admin, workspace("shop"), true);

    assert_eq!(
        verify_chain(&token, node.did(), admin.did(), 10, &none()).unwrap_err(),
        CourierError::NodeMismatch
    );
}

#[test]
fn expiry_and_revocation_end_a_chain() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (member, _) = identity::generate();
    let parent = root(&node, &admin, workspace("shop"), true);
    let leaf = pass(&parent, &admin, &member, app("storefront"));

    assert_eq!(
        verify_chain(&leaf, node.did(), member.did(), 95, &none()).unwrap_err(),
        CourierError::Expired
    );
    // revoking the parent cuts the child, because the child carries it
    let revoked = HashSet::from([parent.id()]);
    assert_eq!(
        verify_chain(&leaf, node.did(), member.did(), 10, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn a_child_may_not_outrank_its_parent() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (member, _) = identity::generate();
    let sealed = root(&node, &admin, workspace("shop"), false);
    let open = root(&node, &admin, app("storefront"), true);

    let from_sealed = pass(&sealed, &admin, &member, app("storefront"));
    assert_eq!(
        verify_chain(&from_sealed, node.did(), member.did(), 10, &none()).unwrap_err(),
        CourierError::NotDelegable
    );

    let widened = pass(&open, &admin, &member, workspace("shop"));
    assert_eq!(
        verify_chain(&widened, node.did(), member.did(), 10, &none()).unwrap_err(),
        CourierError::Escalation
    );
}

#[test]
fn a_chain_must_link_and_reach_the_node() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (member, _) = identity::generate();
    let (stranger, _) = identity::generate();
    let parent = root(&node, &admin, workspace("shop"), true);

    // a token nobody handed over: the stranger issues from a parent addressed to the admin
    let forged = pass(&parent, &stranger, &member, app("storefront"));
    assert_eq!(
        verify_chain(&forged, node.did(), member.did(), 10, &none()).unwrap_err(),
        CourierError::BrokenChain
    );

    // a delegation presented as if it were the root
    let mut orphan = pass(&parent, &admin, &member, app("storefront"))
        .claims()
        .unwrap();
    orphan.prf = None;
    let orphan = sign(&admin, orphan).unwrap();
    assert_eq!(
        verify_chain(&orphan, node.did(), member.did(), 10, &none()).unwrap_err(),
        CourierError::BrokenChain
    );
}

#[test]
fn the_holder_must_be_the_audience() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let (stranger, _) = identity::generate();
    let token = root(&node, &admin, workspace("shop"), true);

    assert_eq!(
        verify_chain(&token, node.did(), stranger.did(), 10, &none()).unwrap_err(),
        CourierError::WrongHolder
    );
}

#[test]
fn chain_depth_is_capped_before_any_crypto() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let mut token = root(&node, &admin, workspace("shop"), true);
    for _ in 0..MAX_CHAIN {
        token = delegate(&token, &admin, admin.did(), workspace("shop"), true, 2, 90).unwrap();
    }

    assert_eq!(
        verify_chain(&token, node.did(), admin.did(), 10, &none()).unwrap_err(),
        CourierError::ChainTooLong
    );
}

#[test]
fn scope_levels_nest_but_never_widen() {
    let orders = Scope::Resource("ws/shop/resource/orders/*".to_string());

    assert!(Scope::Node.contains(&workspace("shop")));
    assert!(workspace("shop").contains(&app("storefront")));
    assert!(workspace("shop").contains(&orders));
    assert!(orders.contains(&Scope::Resource("ws/shop/resource/orders/o-1".to_string())));

    assert!(!workspace("shop").contains(&workspace("cafe")));
    assert!(!workspace("cafe").contains(&orders));
    assert!(!app("storefront").contains(&orders));
    assert!(!app("storefront").contains(&workspace("shop")));
    assert!(!orders.contains(&workspace("shop")));
}
