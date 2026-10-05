use std::collections::HashSet;

use identity::Identity;
use manifest::Manifest;

use super::*;
use crate::CourierError;
use crate::token::issue_root;

const CHAT: &str = r#"app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }
}"#;

fn chat() -> Manifest {
    Manifest::parse(CHAT).unwrap()
}

fn app(item: &str) -> Scope {
    Scope::App {
        ws: "ws1".to_string(),
        app: item.to_string(),
    }
}

fn grant(role: &str, scope: Scope, byte: u8) -> Grant {
    Grant {
        id: [byte; 32],
        role: role.to_string(),
        scope,
    }
}

#[test]
fn a_node_owner_assigns_any_declared_app_role() {
    let grants = [grant("owner", Scope::Node, 1)];
    for role in ["admin", "moderator", "member"] {
        let g = assigning_grant(&grants, role, &app("item1"), &chat()).unwrap();
        assert_eq!(g.id, [1; 32]);
    }
}

#[test]
fn a_workspace_maintainer_assigns_app_roles_in_that_workspace_only() {
    let grants = [grant("maintainer", Scope::Workspace("ws1".into()), 1)];
    assert!(assigning_grant(&grants, "admin", &app("item1"), &chat()).is_some());
    let elsewhere = Scope::App {
        ws: "ws2".into(),
        app: "item1".into(),
    };
    assert!(assigning_grant(&grants, "admin", &elsewhere, &chat()).is_none());
}

#[test]
fn a_platform_member_cannot_assign() {
    let grants = [
        grant("member", Scope::Node, 1),
        grant("member", Scope::Workspace("ws1".into()), 2),
    ];
    assert!(assigning_grant(&grants, "member", &app("item1"), &chat()).is_none());
}

#[test]
fn an_app_admin_assigns_inside_its_cone_only() {
    let grants = [grant("admin", app("item1"), 7)];
    assert_eq!(
        assigning_grant(&grants, "moderator", &app("item1"), &chat())
            .unwrap()
            .id,
        [7; 32]
    );
    assert!(assigning_grant(&grants, "member", &app("item1"), &chat()).is_some());
    assert!(assigning_grant(&grants, "admin", &app("item1"), &chat()).is_none());
}

#[test]
fn an_app_role_does_not_reach_another_app() {
    let grants = [grant("admin", app("item1"), 7)];
    assert!(assigning_grant(&grants, "moderator", &app("item2"), &chat()).is_none());
}

#[test]
fn an_app_member_cannot_assign() {
    let grants = [grant("member", app("item1"), 7)];
    assert!(assigning_grant(&grants, "member", &app("item1"), &chat()).is_none());
}

#[test]
fn an_undeclared_role_cannot_be_assigned_even_by_the_owner() {
    let grants = [grant("owner", Scope::Node, 1)];
    for role in ["owner", "maintainer", "ghost"] {
        assert!(
            assigning_grant(&grants, role, &app("item1"), &chat()).is_none(),
            "{role}"
        );
    }
}

#[test]
fn several_grants_add_up() {
    let grants = [
        grant("member", Scope::Node, 1),
        grant("admin", app("item1"), 2),
    ];
    assert_eq!(
        assigning_grant(&grants, "moderator", &app("item1"), &chat())
            .unwrap()
            .id,
        [2; 32]
    );
}

fn ids() -> (Identity, Identity, Identity) {
    (
        identity::generate().0,
        identity::generate().0,
        identity::generate().0,
    )
}

fn request(caller: &Identity, token: Token, to: &str, role: &str, scope: Scope) -> RoleRequest {
    RoleRequest {
        desktop_did: caller.did().to_string(),
        token,
        to: to.to_string(),
        role: role.to_string(),
        scope,
    }
}

#[test]
fn an_accepted_assignment_mints_a_flat_app_token_for_the_assignee() {
    let (node, alice, bob) = ids();
    let token = issue_root(&node, alice.did(), "owner", Scope::Node, true, 1, 1000).unwrap();
    let grants = [Grant::of(&token).unwrap()];
    let req = request(&alice, token.clone(), bob.did(), "admin", app("item1"));

    let (minted, cause) =
        node_accept_assign(&req, &node, &grants, &chat(), 2, &HashSet::new()).unwrap();
    assert_eq!(cause, token.id());
    let claims = minted.claims().unwrap();
    assert_eq!(claims.aud, bob.did());
    assert_eq!(claims.role, "admin");
    assert_eq!(claims.scope, app("item1"));
    assert!(!claims.delegable);
    assert!(
        claims.prf.is_none(),
        "flat: lineage lives in the node's records"
    );
}

#[test]
fn an_assignment_outside_the_callers_grants_is_refused() {
    let (node, alice, bob) = ids();
    let token = issue_root(&node, alice.did(), "member", Scope::Node, true, 1, 1000).unwrap();
    let grants = [Grant::of(&token).unwrap()];
    let req = request(&alice, token, bob.did(), "moderator", app("item1"));
    assert_eq!(
        node_accept_assign(&req, &node, &grants, &chat(), 2, &HashSet::new()).unwrap_err(),
        CourierError::NotPermitted
    );
}

#[test]
fn an_assignment_presented_with_a_revoked_token_is_refused() {
    let (node, alice, bob) = ids();
    let token = issue_root(&node, alice.did(), "owner", Scope::Node, true, 1, 1000).unwrap();
    let grants = [Grant::of(&token).unwrap()];
    let revoked = HashSet::from([token.id()]);
    let req = request(&alice, token, bob.did(), "admin", app("item1"));
    assert_eq!(
        node_accept_assign(&req, &node, &grants, &chat(), 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn an_assignment_must_target_one_app_and_name_a_real_did() {
    let (node, alice, bob) = ids();
    let token = issue_root(&node, alice.did(), "owner", Scope::Node, true, 1, 1000).unwrap();
    let grants = [Grant::of(&token).unwrap()];
    let ws = request(
        &alice,
        token.clone(),
        bob.did(),
        "admin",
        Scope::Workspace("ws1".into()),
    );
    assert_eq!(
        node_accept_assign(&ws, &node, &grants, &chat(), 2, &HashSet::new()).unwrap_err(),
        CourierError::BadScope
    );
    let nobody = request(&alice, token, "did:key:nope", "admin", app("item1"));
    assert_eq!(
        node_accept_assign(&nobody, &node, &grants, &chat(), 2, &HashSet::new()).unwrap_err(),
        CourierError::Decode
    );
}

#[test]
fn a_revocation_is_authorized_like_the_assignment_it_undoes() {
    let (node, alice, bob) = ids();
    let token = issue_root(&node, alice.did(), "member", Scope::Node, true, 1, 1000).unwrap();
    let grants = [Grant::of(&token).unwrap()];
    let req = request(&alice, token, bob.did(), "moderator", app("item1"));
    assert_eq!(
        node_accept_revoke(&req, node.did(), &grants, &chat(), 2, &HashSet::new()).unwrap_err(),
        CourierError::NotPermitted
    );
}
