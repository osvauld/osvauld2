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

    let (ack, snapshot) = node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();
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
        Some(&current_snapshot), Access::OPEN,
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &revoked).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&first_hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();
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
        Some(&snapshot), Access::OPEN,
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

fn member(node: &Identity, desktop: &Identity) -> Token {
    ws_token(
        node,
        desktop,
        "member",
        Scope::Workspace("ws1".to_string()),
        1,
    )
}

fn hello_since(desktop: &Identity, token: Token, doc: &LoroDoc, since: &[u8]) -> SyncHello {
    desktop_start_sync(
        &desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("board".to_string()),
        doc,
        Some(since),
    )
    .unwrap()
}

#[test]
fn a_diff_since_the_acked_vv_carries_only_new_edits_and_converges() {
    let (node, desktop) = ids();
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "one").unwrap();
    let first = hello(&desktop.did(), member(&node, &desktop), "ws1", &doc);
    let (ack, snapshot) = node_accept_sync(&first, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();
    assert!(!ack.missing);

    doc.get_text("t").insert(3, " two").unwrap();
    let full = doc.export(ExportMode::all_updates()).unwrap();
    let next = hello_since(&desktop, member(&node, &desktop), &doc, &ack.vv);
    assert!(
        next.update.len() < full.len(),
        "since did not shrink the push"
    );

    let (ack, snapshot) =
        node_accept_sync(&next, &node.did(), Some(&snapshot), Access::OPEN, 3, &HashSet::new()).unwrap();
    assert!(!ack.missing);
    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "one two");
}

#[test]
fn a_since_the_node_never_reached_is_flagged_missing() {
    let (node, desktop) = ids();
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "one").unwrap();
    let early = doc.oplog_vv().encode();
    doc.get_text("t").insert(3, " two").unwrap();
    // The desktop believes the node holds "one"; this node has never seen anything.
    let hello = hello_since(&desktop, member(&node, &desktop), &doc, &early);
    let (ack, _) = node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();
    assert!(ack.missing);
}

/// An app-role grant (`role.assign`, or an invite at app scope) reaches its own item's docs
/// and no other: until 2026-10-04 sync checked `Scope::Workspace`, which no app scope contains.
#[test]
fn an_app_scoped_token_syncs_its_own_item_only() {
    let (node, desktop) = ids();
    let app = |item: &str| Scope::App {
        ws: "ws1".to_string(),
        app: item.to_string(),
    };
    let token = issue_root(
        &node,
        &desktop.did(),
        "moderator",
        app("item1"),
        false,
        1,
        1000,
    )
    .unwrap();

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hi").unwrap();
    let own = hello(&desktop.did(), token.clone(), "ws1", &doc);
    node_accept_sync(&own, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();

    let other = desktop_start_sync(
        &desktop.did(),
        token,
        "ws1",
        "item2",
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();
    assert_eq!(
        node_accept_sync(&other, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap_err(),
        CourierError::OutOfScope
    );
}

fn src_hello(desktop: &Identity, token: Token, doc: &LoroDoc) -> SyncHello {
    desktop_start_sync(
        desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Src,
        doc,
        None,
    )
    .unwrap()
}

/// The source holds `manifest.osv`; a member who could rewrite it could grant itself roles.
#[test]
fn only_an_installer_may_change_an_apps_source_but_any_member_may_pull_it() {
    let (node, owner) = ids();
    let (_, member) = ids();
    let ws = || Scope::Workspace("ws1".into());
    let owner_token = ws_token(&node, &owner, "owner", ws(), 1);
    let member_token = ws_token(&node, &member, "member", ws(), 1);

    let src = LoroDoc::new();
    src.get_text("manifest.osv")
        .insert(0, "app \"x\" {}")
        .unwrap();
    let push = src_hello(&owner, owner_token, &src);
    let (_, stored) = node_accept_sync(&push, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();

    let pull = src_hello(&member, member_token.clone(), &LoroDoc::new());
    node_accept_sync(&pull, &node.did(), Some(&stored), Access::OPEN, 2, &HashSet::new()).unwrap();

    let forged = LoroDoc::new();
    forged.import(&stored).unwrap();
    forged
        .get_text("manifest.osv")
        .insert(0, "-- mine\n")
        .unwrap();
    let forge = src_hello(&member, member_token, &forged);
    assert_eq!(
        node_accept_sync(&forge, &node.did(), Some(&stored), Access::OPEN, 2, &HashSet::new()).unwrap_err(),
        CourierError::NotPermitted
    );
}

fn doc_hello(desktop: &Identity, token: Token, name: &str, doc: &LoroDoc) -> SyncHello {
    desktop_start_sync(
        desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc(name.to_string()),
        doc,
        None,
    )
    .unwrap()
}

fn held(doc: &LoroDoc) -> Vec<u8> {
    doc.export(ExportMode::Snapshot).unwrap()
}

const READ_ONLY: Access = Access {
    read: true,
    write: false,
};
const WRITE_ONLY: Access = Access {
    read: false,
    write: true,
};

#[test]
fn a_reader_may_pull_but_not_change_the_nodes_copy() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "admin's").unwrap();
    let stored = held(&on_node);

    let pull = doc_hello(&desktop, member(&node, &desktop), "chat", &LoroDoc::new());
    let (ack, _) =
        node_accept_sync(&pull, &node.did(), Some(&stored), READ_ONLY, 2, &HashSet::new()).unwrap();
    let mine = LoroDoc::new();
    mine.import(&ack.update).unwrap();
    assert_eq!(mine.get_text("t").to_string(), "admin's");

    // Re-sending what the node already holds changes nothing, so it needs no write.
    let again = doc_hello(&desktop, member(&node, &desktop), "chat", &mine);
    node_accept_sync(&again, &node.did(), Some(&stored), READ_ONLY, 2, &HashSet::new()).unwrap();

    mine.get_text("t").insert(0, "member's ").unwrap();
    let push = doc_hello(&desktop, member(&node, &desktop), "chat", &mine);
    assert_eq!(
        node_accept_sync(&push, &node.did(), Some(&stored), READ_ONLY, 2, &HashSet::new())
            .unwrap_err(),
        CourierError::NoWrite
    );
}

#[test]
fn without_read_a_held_doc_is_refused_but_a_new_one_may_be_created() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "secret").unwrap();
    let stored = held(&on_node);

    let pull = doc_hello(&desktop, member(&node, &desktop), "dm", &LoroDoc::new());
    assert_eq!(
        node_accept_sync(&pull, &node.did(), Some(&stored), WRITE_ONLY, 2, &HashSet::new())
            .unwrap_err(),
        CourierError::NoRead
    );

    let fresh = LoroDoc::new();
    fresh.get_text("t").insert(0, "mine").unwrap();
    let create = doc_hello(&desktop, member(&node, &desktop), "dm", &fresh);
    node_accept_sync(&create, &node.did(), None, WRITE_ONLY, 2, &HashSet::new()).unwrap();
    // Opening a doc syncs it empty before its creator writes: that copy discloses nothing.
    let empty = held(&LoroDoc::new());
    node_accept_sync(&create, &node.did(), Some(&empty), WRITE_ONLY, 2, &HashSet::new()).unwrap();
}

/// A one-doc grant syncs that doc and pulls the app's source; its siblings are out of scope.
#[test]
fn a_resource_token_reaches_its_own_doc_and_the_source_only() {
    let (node, guest) = ids();
    let token = ws_token(
        &node,
        &guest,
        "member",
        Scope::Resource("ws/ws1/item1/board".to_string()),
        1,
    );
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "card").unwrap();
    let own = doc_hello(&guest, token.clone(), "board", &doc);
    node_accept_sync(&own, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();

    let sibling = doc_hello(&guest, token.clone(), "notes", &doc);
    assert_eq!(
        node_accept_sync(&sibling, &node.did(), None, Access::OPEN, 2, &HashSet::new())
            .unwrap_err(),
        CourierError::OutOfScope
    );

    let src = src_hello(&guest, token, &LoroDoc::new());
    node_accept_sync(&src, &node.did(), None, Access::OPEN, 2, &HashSet::new()).unwrap();
}

#[test]
fn reaches_follows_doc_addresses() {
    let item = item_scope("ws1", "item1");
    let board = SyncLayer::Doc("board".to_string());
    let one = Scope::Resource("ws/ws1/item1/board".to_string());
    let other = Scope::Resource("ws/ws1/item2/board".to_string());
    assert!(reaches(&item, "ws1", "item1", &board));
    assert!(reaches(&one, "ws1", "item1", &board));
    assert!(reaches(&one, "ws1", "item1", &SyncLayer::Src));
    assert!(!reaches(&one, "ws1", "item1", &SyncLayer::Doc("notes".to_string())));
    assert!(!reaches(&other, "ws1", "item1", &board));
    assert!(!reaches(&other, "ws1", "item1", &SyncLayer::Src));
}

/// A name with no address is refused outright, never judged against the whole item.
#[test]
fn a_doc_name_with_no_address_is_refused_not_widened_to_the_item() {
    let (node, desktop) = ids();
    let long = vec!["c".repeat(128); 8].join("/");
    for name in ["a.b", long.as_str()] {
        let hello = doc_hello(&desktop, member(&node, &desktop), name, &LoroDoc::new());
        assert_eq!(
            node_accept_sync(&hello, &node.did(), None, Access::OPEN, 2, &HashSet::new())
                .unwrap_err(),
            CourierError::BadScope,
            "{name}"
        );
        assert!(!reaches(
            &Scope::Node,
            "ws1",
            "item1",
            &SyncLayer::Doc(name.to_string())
        ));
    }
}
