use super::*;

const SCOPE: &str = "ws/shop/orders/*";

#[test]
fn root_is_self_issued_by_the_node() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let token = issue_root(&node, admin.did(), "writer", SCOPE, true, 1, 100).unwrap();
    let claims = token.claims().unwrap();

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
    let root = issue_root(&node, admin.did(), "writer", SCOPE, true, 1, 100).unwrap();
    let child = delegate(&root, &admin, member.did(), SCOPE, false, 2, 50).unwrap();
    let claims = child.claims().unwrap();

    assert_eq!(claims.iss, admin.did());
    assert_eq!(claims.aud, member.did());
    assert_eq!(claims.sub, node.did());
    assert_eq!(claims.role, "writer");
    assert_eq!(claims.prf, Some(root));
}

#[test]
fn signature_covers_domain_and_payload() {
    let (node, _) = identity::generate();
    let token = issue_root(&node, node.did(), "writer", SCOPE, true, 1, 100).unwrap();
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
    let token = issue_root(&node, node.did(), "writer", SCOPE, true, 1, 100).unwrap();
    let mut resigned = token.clone();
    resigned.sig[0] ^= 1;
    let reissued = issue_root(&node, node.did(), "writer", SCOPE, true, 1, 100).unwrap();

    assert_eq!(token.id(), resigned.id());
    assert_ne!(token.id(), reissued.id());
}
