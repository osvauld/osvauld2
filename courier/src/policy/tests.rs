use std::collections::HashSet;

use identity::Identity;

use super::*;
use crate::token::{delegate, issue_root};

fn workspace(id: &str) -> Scope {
    Scope::Workspace(id.to_string())
}

fn app(id: &str) -> Scope {
    Scope::App {
        ws: "shop".to_string(),
        app: id.to_string(),
    }
}

fn held(node: &Identity, aud: &Identity, role: &str, scope: Scope) -> Token {
    issue_root(node, aud.did(), role, scope, true, 1, 100).unwrap()
}

fn none() -> HashSet<[u8; 32]> {
    HashSet::new()
}

fn may(token: &Token, node: &Identity, holder: &Identity, cap: Capability, target: &Scope) -> bool {
    authorize(token, node.did(), holder.did(), cap, target, 10, &none()).is_ok()
}

#[test]
fn a_node_admin_creates_workspaces_but_does_not_set_policy() {
    let (node, _) = identity::generate();
    let (admin, _) = identity::generate();
    let token = held(&node, &admin, "admin", Scope::Node);

    assert!(may(&token, &node, &admin, WorkspaceCreate, &Scope::Node));
    assert!(may(&token, &node, &admin, RoleAssign, &Scope::Node));
    assert_eq!(
        authorize(
            &token,
            node.did(),
            admin.did(),
            PolicyPublish,
            &workspace("shop"),
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::NotPermitted
    );
}

#[test]
fn a_node_role_reaches_the_workspaces_under_it() {
    let (node, _) = identity::generate();
    let (owner, _) = identity::generate();
    let token = held(&node, &owner, "owner", Scope::Node);

    assert!(may(&token, &node, &owner, AppInstall, &workspace("shop")));
    assert!(may(&token, &node, &owner, AppInstall, &app("storefront")));
}

#[test]
fn a_maintainer_installs_apps_but_cannot_delete_the_workspace() {
    let (node, _) = identity::generate();
    let (keeper, _) = identity::generate();
    let token = held(&node, &keeper, "maintainer", workspace("shop"));

    assert!(may(&token, &node, &keeper, AppInstall, &workspace("shop")));
    assert!(may(
        &token,
        &node,
        &keeper,
        NamespaceDeclare,
        &app("orders")
    ));
    assert_eq!(
        authorize(
            &token,
            node.did(),
            keeper.did(),
            WorkspaceDelete,
            &workspace("shop"),
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::NotPermitted
    );
}

#[test]
fn a_workspace_role_does_not_reach_another_workspace() {
    let (node, _) = identity::generate();
    let (keeper, _) = identity::generate();
    let token = held(&node, &keeper, "owner", workspace("shop"));

    assert_eq!(
        authorize(
            &token,
            node.did(),
            keeper.did(),
            AppInstall,
            &workspace("cafe"),
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::OutOfScope
    );
    // Reaching upward is the same refusal: the workspace does not contain the node.
    assert_eq!(
        authorize(
            &token,
            node.did(),
            keeper.did(),
            WorkspaceCreate,
            &Scope::Node,
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::OutOfScope
    );
}

#[test]
fn the_capability_matrix_is_exactly_this() {
    let node_owner: &[Capability] = &[
        WorkspaceCreate,
        WorkspaceDelete,
        AppInstall,
        AppRemove,
        NamespaceDeclare,
        PolicyPublish,
        MemberInvite,
        RoleAssign,
    ];
    let ws_owner: &[Capability] = &[
        WorkspaceDelete,
        AppInstall,
        AppRemove,
        NamespaceDeclare,
        PolicyPublish,
        MemberInvite,
        RoleAssign,
    ];
    let maintainer: &[Capability] = &[
        AppInstall,
        AppRemove,
        NamespaceDeclare,
        PolicyPublish,
        MemberInvite,
        RoleAssign,
    ];
    let orders = Scope::Resource("ws/shop/resource/orders".to_string());
    let nothing: &[Capability] = &[];

    // Every cell, not a sample: this table is the security contract, so a capability added
    // anywhere has to be added here too.
    let matrix: &[(&Scope, &str, &[Capability])] = &[
        (&Scope::Node, "owner", node_owner),
        (&Scope::Node, "admin", &[WorkspaceCreate, RoleAssign]),
        (&Scope::Node, "maintainer", nothing),
        (&Scope::Node, "member", nothing),
        (&Scope::Node, "guest", nothing),
        (&workspace("shop"), "owner", ws_owner),
        (&workspace("shop"), "admin", nothing),
        (&workspace("shop"), "maintainer", maintainer),
        (&workspace("shop"), "member", nothing),
        (&workspace("shop"), "guest", nothing),
        (&app("storefront"), "owner", nothing),
        (&app("storefront"), "admin", nothing),
        (&app("storefront"), "maintainer", nothing),
        (&app("storefront"), "member", nothing),
        (&app("storefront"), "guest", nothing),
        (&orders, "owner", nothing),
        (&orders, "admin", nothing),
        (&orders, "maintainer", nothing),
        (&orders, "member", nothing),
        (&orders, "guest", nothing),
    ];

    for (scope, role, expected) in matrix {
        assert_eq!(
            platform_capabilities(role, scope),
            *expected,
            "{role} at {scope:?}"
        );
    }
    // A name nobody put in the table grants nothing, whatever it looks like.
    for role in ["reviewer", "Owner", "owner ", ""] {
        assert!(platform_capabilities(role, &Scope::Node).is_empty());
    }
}

#[test]
fn a_malformed_target_is_refused_before_the_chain_is_walked() {
    let (node, _) = identity::generate();
    let (owner, _) = identity::generate();
    let token = held(&node, &owner, "owner", Scope::Node);

    // Scope::Node contains every variant, so a node role would otherwise act on targets a
    // workspace role could not even parse.
    for target in [
        Scope::Resource("not-an-address".to_string()),
        Scope::Resource("ws/shop/resource/../secrets".to_string()),
        Scope::Workspace(String::new()),
        Scope::App {
            ws: "shop".to_string(),
            app: "a/b".to_string(),
        },
    ] {
        assert_eq!(
            authorize(
                &token,
                node.did(),
                owner.did(),
                AppInstall,
                &target,
                10,
                &none()
            )
            .unwrap_err(),
            CourierError::BadScope,
            "{target:?}"
        );
    }
}

#[test]
fn authorize_still_enforces_the_chain() {
    let (node, _) = identity::generate();
    let (owner, _) = identity::generate();
    let (stranger, _) = identity::generate();
    let token = held(&node, &owner, "owner", workspace("shop"));
    let target = workspace("shop");

    assert_eq!(
        authorize(
            &token,
            node.did(),
            stranger.did(),
            AppInstall,
            &target,
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::WrongHolder
    );
    assert_eq!(
        authorize(
            &token,
            node.did(),
            owner.did(),
            AppInstall,
            &target,
            10,
            &HashSet::from([token.id()])
        )
        .unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn delegating_sideways_keeps_the_role_and_its_capabilities() {
    let (node, _) = identity::generate();
    let (keeper, _) = identity::generate();
    let (deputy, _) = identity::generate();
    let parent = held(&node, &keeper, "maintainer", workspace("shop"));
    let passed = delegate(
        &parent,
        &keeper,
        deputy.did(),
        workspace("shop"),
        false,
        2,
        90,
    )
    .unwrap();

    assert!(may(&passed, &node, &deputy, AppInstall, &workspace("shop")));
}

#[test]
fn narrowing_to_app_level_drops_platform_capabilities() {
    let (node, _) = identity::generate();
    let (keeper, _) = identity::generate();
    let (deputy, _) = identity::generate();
    let parent = held(&node, &keeper, "maintainer", workspace("shop"));
    let narrowed = delegate(&parent, &keeper, deputy.did(), app("orders"), false, 2, 90).unwrap();

    // Scope names the level a role is read against, so "maintainer" one level down is a
    // manifest name. Narrowing cannot smuggle platform power into an app, even though the
    // holder above still has it over the same target.
    assert!(may(&parent, &node, &keeper, AppRemove, &app("orders")));
    assert_eq!(
        authorize(
            &narrowed,
            node.did(),
            deputy.did(),
            AppRemove,
            &app("orders"),
            10,
            &none()
        )
        .unwrap_err(),
        CourierError::NotPermitted
    );
}
