use identity::Identity;
use loro::{ExportMode, LoroDoc};

use super::*;
use crate::CourierError;
use crate::token::issue_root;

fn ids() -> (Identity, Identity) {
    let (node, _) = identity::generate();
    let (desktop, _) = identity::generate();
    (node, desktop)
}

fn ws_token(node: &Identity, desktop: &Identity, role: &str, scope: Scope, now: u64) -> Token {
    issue_root(node, &desktop.did(), role, scope, true, now, now + 1000).unwrap()
}

/// Every test below goes through the real desktop-side constructor rather than assembling a
/// `SyncHello` by hand — the authorization/rejection tests get `desktop_start_sync`'s
/// commit-before-export handling for free, same as the happy-path ones.
fn hello(desktop_did: &str, token: Token, ws_id: &str, doc: &LoroDoc) -> SyncHello {
    desktop_start_sync(
        desktop_did,
        token,
        ws_id,
        "item1",
        SyncLayer::Doc("board".to_string()),
        doc,
        None,
    )
    .unwrap()
}

#[test]
fn a_member_pushes_into_a_fresh_layer() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "guest",
        Scope::Workspace("ws1".to_string()),
        1,
    );

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let hello = hello(&desktop.did(), token, "ws1", &doc);

    let (ack, snapshot) = node_accept_sync(&hello, &node.did(), None, 2, &HashSet::new()).unwrap();
    assert_eq!(ack.request_id, hello.request_id);

    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");

    // The node had nothing beyond what this push already covers, so importing the ack back
    // into the desktop's own doc is a no-op — an "empty" update is a small valid blob, not a
    // zero-length one, so the exact byte count isn't the thing to pin.
    doc.import(&ack.update).unwrap();
    assert_eq!(doc.get_text("t").to_string(), "hello");
}

#[test]
fn a_push_merges_with_what_the_node_already_held_and_the_diff_comes_back() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    );

    // Someone else's edit, already on the node.
    let existing = LoroDoc::new();
    existing.get_text("t").insert(0, "from-node").unwrap();
    let current_snapshot = existing.export(ExportMode::Snapshot).unwrap();

    // The desktop's own doc started independently and knows nothing of it.
    let desktop_doc = LoroDoc::new();
    desktop_doc.get_text("t").insert(0, "from-desktop").unwrap();
    let hello = hello(&desktop.did(), token, "ws1", &desktop_doc);

    let (ack, snapshot) = node_accept_sync(
        &hello,
        &node.did(),
        Some(&current_snapshot),
        2,
        &HashSet::new(),
    )
    .unwrap();
    // The node's pre-existing edit is exactly what the desktop's vv didn't cover.
    assert!(!ack.update.is_empty());

    let merged = LoroDoc::new();
    merged.import(&snapshot).unwrap();
    let text = merged.get_text("t").to_string();
    assert!(text.contains("from-node") && text.contains("from-desktop"));
}

#[test]
fn a_revoked_token_cannot_sync() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    );
    let mut revoked = HashSet::new();
    revoked.insert(token.id());
    let hello = hello(&desktop.did(), token, "ws1", &LoroDoc::new());

    assert_eq!(
        node_accept_sync(&hello, &node.did(), None, 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn a_token_scoped_to_another_workspace_cannot_sync_this_one() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("other".to_string()),
        1,
    );
    let hello = hello(&desktop.did(), token, "ws1", &LoroDoc::new());

    assert_eq!(
        node_accept_sync(&hello, &node.did(), None, 2, &HashSet::new()).unwrap_err(),
        CourierError::OutOfScope
    );
}

#[test]
fn a_hello_that_lies_about_its_holder_is_refused() {
    let (node, desktop) = ids();
    let (someone_else, _) = identity::generate();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    );
    let hello = hello(&someone_else.did(), token, "ws1", &LoroDoc::new());

    assert_eq!(
        node_accept_sync(&hello, &node.did(), None, 2, &HashSet::new()).unwrap_err(),
        CourierError::WrongHolder
    );
}

#[test]
fn a_malformed_update_is_rejected_not_imported() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    );
    let mut hello = hello(&desktop.did(), token, "ws1", &LoroDoc::new());
    hello.update = b"not a loro update".to_vec();

    assert_eq!(
        node_accept_sync(&hello, &node.did(), None, 2, &HashSet::new()).unwrap_err(),
        CourierError::Decode
    );
}

#[test]
fn a_second_sync_sends_only_what_changed_since_the_first() {
    let (node, desktop) = ids();
    let token = ws_token(
        &node,
        &desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    );

    // A long first edit and a tiny second one: envelope overhead is roughly fixed per update,
    // so making the *content* gap large is what makes a byte-length comparison mean something
    // (a bug caught in B1a's own tests: two tiny ops can differ in envelope noise alone).
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, &"a".repeat(500)).unwrap();
    let first_hello = desktop_start_sync(
        &desktop.did(),
        token.clone(),
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();
    let (first_ack, snapshot) =
        node_accept_sync(&first_hello, &node.did(), None, 2, &HashSet::new()).unwrap();
    doc.import(&first_ack.update).unwrap();

    // A further local edit, synced against what the first round already covered — only the
    // new character should cross, not the 500-character body again.
    doc.get_text("t").insert(500, "!").unwrap();
    let second_hello = desktop_start_sync(
        &desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
        &doc,
        Some(&first_hello.vv),
    )
    .unwrap();
    assert!(second_hello.update.len() < first_hello.update.len() / 2);

    let (_, snapshot) = node_accept_sync(
        &second_hello,
        &node.did(),
        Some(&snapshot),
        3,
        &HashSet::new(),
    )
    .unwrap();
    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(
        landed.get_text("t").to_string(),
        format!("{}!", "a".repeat(500))
    );
}
