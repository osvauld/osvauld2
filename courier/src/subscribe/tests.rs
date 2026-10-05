use identity::Identity;

use super::*;
use crate::CourierError;
use crate::token::{Scope, issue_root};

fn ids() -> (Identity, Identity) {
    let (node, _) = identity::generate();
    let (desktop, _) = identity::generate();
    (node, desktop)
}

fn ws_token(node: &Identity, desktop: &Identity, role: &str, now: u64) -> Token {
    issue_root(
        node,
        &desktop.did(),
        role,
        Scope::Workspace("ws1".to_string()),
        true,
        now,
        now + 1000,
    )
    .unwrap()
}

#[test]
fn a_member_subscribes_to_a_layer() {
    let (node, desktop) = ids();
    let token = ws_token(&node, &desktop, "member", 1);
    let hello = desktop_start_subscribe(
        &desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
    );

    assert!(node_accept_subscription(&hello, &node.did(), 2, &HashSet::new()).is_ok());
}

/// The same request, whichever action the caller takes with it — there is nothing here for
/// subscribe and unsubscribe to differ on, so one function proves both.
#[test]
fn the_same_hello_authorizes_an_unsubscribe_too() {
    let (node, desktop) = ids();
    let token = ws_token(&node, &desktop, "guest", 1);
    let hello = desktop_start_subscribe(&desktop.did(), token, "ws1", "item1", SyncLayer::Src);

    assert!(node_accept_subscription(&hello, &node.did(), 2, &HashSet::new()).is_ok());
}

#[test]
fn a_token_scoped_to_another_workspace_cannot_subscribe_to_this_one() {
    let (node, desktop) = ids();
    let token = issue_root(
        &node,
        &desktop.did(),
        "member",
        Scope::Workspace("other".to_string()),
        true,
        1,
        1000,
    )
    .unwrap();
    let hello = desktop_start_subscribe(
        &desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
    );

    assert_eq!(
        node_accept_subscription(&hello, &node.did(), 2, &HashSet::new()).unwrap_err(),
        CourierError::OutOfScope
    );
}

#[test]
fn a_revoked_token_cannot_subscribe() {
    let (node, desktop) = ids();
    let token = ws_token(&node, &desktop, "member", 1);
    let mut revoked = HashSet::new();
    revoked.insert(token.id());
    let hello = desktop_start_subscribe(
        &desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
    );

    assert_eq!(
        node_accept_subscription(&hello, &node.did(), 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn a_hello_that_lies_about_its_holder_is_refused() {
    let (node, desktop) = ids();
    let (someone_else, _) = identity::generate();
    let token = ws_token(&node, &desktop, "member", 1);
    let hello = desktop_start_subscribe(
        &someone_else.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
    );

    assert_eq!(
        node_accept_subscription(&hello, &node.did(), 2, &HashSet::new()).unwrap_err(),
        CourierError::WrongHolder
    );
}

#[test]
fn a_node_scoped_token_may_listen() {
    let (node, desktop) = ids();
    let token = issue_root(&node, &desktop.did(), "member", Scope::Node, true, 1, 1000).unwrap();

    assert!(node_accept_listen(&token, &node.did(), &desktop.did(), 2, &HashSet::new()).is_ok());
}

/// Revised 2026-10-04: a listen carries only pushes for subscriptions that were each
/// authorized, so any token this node rooted is enough — a workspace- or app-scoped member
/// was previously shut out of pushes entirely.
#[test]
fn a_workspace_or_app_scoped_token_may_listen() {
    let (node, desktop) = ids();
    let ws = ws_token(&node, &desktop, "member", 1);
    let app = app_token(&node, &desktop, "item1", 1);
    for token in [ws, app] {
        node_accept_listen(&token, &node.did(), &desktop.did(), 2, &HashSet::new()).unwrap();
    }
}

#[test]
fn a_token_for_someone_else_cannot_listen() {
    let (node, desktop) = ids();
    let (_, other) = ids();
    let token = ws_token(&node, &desktop, "member", 1);
    assert_eq!(
        node_accept_listen(&token, &node.did(), &other.did(), 2, &HashSet::new()).unwrap_err(),
        CourierError::WrongHolder
    );
}

fn app_token(node: &Identity, desktop: &Identity, item: &str, now: u64) -> Token {
    let scope = Scope::App {
        ws: "ws1".to_string(),
        app: item.to_string(),
    };
    issue_root(
        node,
        &desktop.did(),
        "member",
        scope,
        false,
        now,
        now + 1000,
    )
    .unwrap()
}

#[test]
fn an_app_scoped_token_subscribes_to_its_own_item_only() {
    let (node, desktop) = ids();
    let token = app_token(&node, &desktop, "item1", 1);
    let layer = SyncLayer::Doc("board".to_string());
    let own = desktop_start_subscribe(&desktop.did(), token.clone(), "ws1", "item1", layer.clone());
    node_accept_subscription(&own, &node.did(), 2, &HashSet::new()).unwrap();

    let other = desktop_start_subscribe(&desktop.did(), token, "ws1", "item2", layer);
    assert_eq!(
        node_accept_subscription(&other, &node.did(), 2, &HashSet::new()).unwrap_err(),
        CourierError::OutOfScope
    );
}

#[test]
fn a_revoked_token_cannot_listen() {
    let (node, desktop) = ids();
    let token = issue_root(&node, &desktop.did(), "member", Scope::Node, true, 1, 1000).unwrap();
    let mut revoked = HashSet::new();
    revoked.insert(token.id());

    assert_eq!(
        node_accept_listen(&token, &node.did(), &desktop.did(), 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}
