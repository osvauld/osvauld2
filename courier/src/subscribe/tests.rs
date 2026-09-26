use identity::Identity;

use super::*;
use crate::CourierError;
use crate::token::issue_root;

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

#[test]
fn a_workspace_scoped_token_cannot_listen() {
    let (node, desktop) = ids();
    let token = ws_token(&node, &desktop, "member", 1);

    assert_eq!(
        node_accept_listen(&token, &node.did(), &desktop.did(), 2, &HashSet::new()).unwrap_err(),
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
