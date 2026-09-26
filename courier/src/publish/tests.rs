use identity::Identity;

use super::*;
use crate::CourierError;
use crate::token::{self, issue_root};

fn ids() -> (Identity, Identity) {
    let (node, _) = identity::generate();
    let (desktop, _) = identity::generate();
    (node, desktop)
}

/// Stands in for the token a claim or reconnect already left the desktop holding.
fn node_token(node: &Identity, desktop: &Identity, role: &str, now: u64) -> Token {
    issue_root(
        node,
        &desktop.did(),
        role,
        Scope::Node,
        true,
        now,
        now + 1000,
    )
    .unwrap()
}

fn a_workspace() -> PublishedWorkspace {
    PublishedWorkspace {
        id: "ws1".to_string(),
        name: "field notes".to_string(),
        created: 1,
    }
}

#[test]
fn an_owner_creates_a_workspace() {
    let (node, desktop) = ids();
    let hello = desktop_publish(
        &desktop.did(),
        node_token(&node, &desktop, "owner", 1),
        a_workspace(),
        vec![],
    );

    let ack = node_accept_publish(
        &hello,
        &node.did(),
        vec!["known".to_string()],
        2,
        &HashSet::new(),
    )
    .unwrap();
    assert_eq!(ack.request_id, hello.request_id);
    assert_eq!(ack.known_items, vec!["known".to_string()]);
}

#[test]
fn an_admin_creates_a_workspace_too() {
    let (node, desktop) = ids();
    let hello = desktop_publish(
        &desktop.did(),
        node_token(&node, &desktop, "admin", 1),
        a_workspace(),
        vec![],
    );

    assert!(node_accept_publish(&hello, &node.did(), vec![], 2, &HashSet::new()).is_ok());
}

#[test]
fn a_token_already_narrowed_to_the_workspace_cannot_create_it() {
    let (node, desktop) = ids();
    let claim = node_token(&node, &desktop, "owner", 1);
    let narrowed = token::delegate(
        &claim,
        &desktop,
        &desktop.did(),
        Scope::Workspace("ws1".to_string()),
        false,
        1,
        100,
    )
    .unwrap();
    let hello = desktop_publish(&desktop.did(), narrowed, a_workspace(), vec![]);

    // Same role, same workspace — but `WorkspaceCreate` is only ever granted at `Scope::Node`,
    // so narrowing past it drops the capability even though the scope still matches.
    assert_eq!(
        node_accept_publish(&hello, &node.did(), vec![], 2, &HashSet::new()).unwrap_err(),
        CourierError::NotPermitted
    );
}

#[test]
fn a_revoked_claim_cannot_publish() {
    let (node, desktop) = ids();
    let claim = node_token(&node, &desktop, "owner", 1);
    let hello = desktop_publish(&desktop.did(), claim.clone(), a_workspace(), vec![]);

    let mut revoked = HashSet::new();
    revoked.insert(claim.id());
    assert_eq!(
        node_accept_publish(&hello, &node.did(), vec![], 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn a_hello_that_lies_about_its_holder_is_refused() {
    let (node, desktop) = ids();
    let (someone_else, _) = identity::generate();
    let claim = node_token(&node, &desktop, "owner", 1);
    let hello = desktop_publish(&someone_else.did(), claim, a_workspace(), vec![]);

    assert_eq!(
        node_accept_publish(&hello, &node.did(), vec![], 2, &HashSet::new()).unwrap_err(),
        CourierError::WrongHolder
    );
}
