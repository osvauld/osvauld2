use identity::{Identity, peer_of};
use loro::{ExportMode, LoroDoc, LoroMap};
use manifest::Manifest;

use super::*;
use std::collections::BTreeSet;
use crate::CourierError;
use crate::token::issue_root;

/// A test device key, one per identity: the phrase-derived key stands in for the per-install
/// one a vault holds.
fn device_key(desktop: &Identity) -> [u8; 32] {
    desktop.device_public_key()
}

/// A device's own doc, stamped with its peer as every live doc is (`owned-entries.md` §2.1).
fn doc_of(desktop: &Identity) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.set_peer_id(peer_of(&device_key(desktop))).unwrap();
    doc
}

fn author(desktop: &Identity) -> Author {
    Author::Device(peer_of(&device_key(desktop)))
}

/// A device's push on a doc with no rules.
fn writer(desktop: &Identity) -> Writer<'static> {
    Writer {
        author: author(desktop),
        roles: BTreeSet::new(),
        rules: None,
    }
}

/// A host relaying what it judged already.
fn host_writer() -> Writer<'static> {
    Writer {
        author: Author::Host,
        roles: BTreeSet::new(),
        rules: None,
    }
}

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

    let doc = doc_of(&desktop);
    doc.get_text("t").insert(0, "hello").unwrap();
    let hello = hello(&desktop.did(), token, "ws1", &doc);

    let (ack, snapshot) = node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
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
    let desktop_doc = doc_of(&desktop);
    desktop_doc.get_text("t").insert(0, "from-desktop").unwrap();
    let hello = hello(&desktop.did(), token, "ws1", &desktop_doc);

    let (ack, snapshot) = node_accept_sync(
        &hello,
        &node.did(),
        Some(&current_snapshot), Access::OPEN, &writer(&desktop), 2, &HashSet::new())
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &revoked).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap_err(),
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
    let doc = doc_of(&desktop);
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
        node_accept_sync(&first_hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
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
        Some(&snapshot), Access::OPEN, &writer(&desktop), 3, &HashSet::new())
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
    let doc = doc_of(&desktop);
    doc.get_text("t").insert(0, "one").unwrap();
    let first = hello(&desktop.did(), member(&node, &desktop), "ws1", &doc);
    let (ack, snapshot) = node_accept_sync(&first, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
    assert!(!ack.missing);

    doc.get_text("t").insert(3, " two").unwrap();
    let full = doc.export(ExportMode::all_updates()).unwrap();
    let next = hello_since(&desktop, member(&node, &desktop), &doc, &ack.vv);
    assert!(
        next.update.len() < full.len(),
        "since did not shrink the push"
    );

    let (ack, snapshot) =
        node_accept_sync(&next, &node.did(), Some(&snapshot), Access::OPEN, &writer(&desktop), 3, &HashSet::new()).unwrap();
    assert!(!ack.missing);
    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "one two");
}

/// A relayed update the node already holds is acked unchanged: what stops a relay loop.
#[test]
fn a_merge_that_moves_nothing_reports_unchanged() {
    let (node, desktop) = ids();
    let doc = doc_of(&desktop);
    doc.get_text("t").insert(0, "once").unwrap();
    let push = hello(&desktop.did(), member(&node, &desktop), "ws1", &doc);
    let (ack, snapshot) =
        node_accept_sync(&push, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
    assert!(ack.changed);

    let (ack, _) =
        node_accept_sync(&push, &node.did(), Some(&snapshot), Access::OPEN, &writer(&desktop), 3, &HashSet::new()).unwrap();
    assert!(!ack.changed);

    let pull = hello(&desktop.did(), member(&node, &desktop), "ws1", &LoroDoc::new());
    let (ack, _) =
        node_accept_sync(&pull, &node.did(), Some(&snapshot), Access::OPEN, &writer(&desktop), 3, &HashSet::new()).unwrap();
    assert!(!ack.changed);
}

#[test]
fn a_since_the_node_never_reached_is_flagged_missing() {
    let (node, desktop) = ids();
    let doc = doc_of(&desktop);
    doc.get_text("t").insert(0, "one").unwrap();
    let early = doc.oplog_vv().encode();
    doc.get_text("t").insert(3, " two").unwrap();
    // The desktop believes the node holds "one"; this node has never seen anything.
    let hello = hello_since(&desktop, member(&node, &desktop), &doc, &early);
    let (ack, _) = node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
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

    let doc = doc_of(&desktop);
    doc.get_text("t").insert(0, "hi").unwrap();
    let own = hello(&desktop.did(), token.clone(), "ws1", &doc);
    node_accept_sync(&own, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();

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
        node_accept_sync(&other, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap_err(),
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

    let src = doc_of(&owner);
    src.get_text("manifest.osv")
        .insert(0, "app \"x\" {}")
        .unwrap();
    let push = src_hello(&owner, owner_token, &src);
    let (_, stored) = node_accept_sync(&push, &node.did(), None, Access::OPEN, &writer(&owner), 2, &HashSet::new()).unwrap();

    let pull = src_hello(&member, member_token.clone(), &LoroDoc::new());
    node_accept_sync(&pull, &node.did(), Some(&stored), Access::OPEN, &writer(&member), 2, &HashSet::new()).unwrap();

    let forged = doc_of(&member);
    forged.import(&stored).unwrap();
    forged
        .get_text("manifest.osv")
        .insert(0, "-- mine\n")
        .unwrap();
    let forge = src_hello(&member, member_token, &forged);
    assert_eq!(
        node_accept_sync(&forge, &node.did(), Some(&stored), Access::OPEN, &writer(&member), 2, &HashSet::new()).unwrap_err(),
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
        node_accept_sync(&pull, &node.did(), Some(&stored), READ_ONLY, &writer(&desktop), 2, &HashSet::new()).unwrap();
    let mine = doc_of(&desktop);
    mine.import(&ack.update).unwrap();
    assert_eq!(mine.get_text("t").to_string(), "admin's");

    // Re-sending what the node already holds changes nothing, so it needs no write.
    let again = doc_hello(&desktop, member(&node, &desktop), "chat", &mine);
    node_accept_sync(&again, &node.did(), Some(&stored), READ_ONLY, &writer(&desktop), 2, &HashSet::new()).unwrap();

    mine.get_text("t").insert(0, "member's ").unwrap();
    let push = doc_hello(&desktop, member(&node, &desktop), "chat", &mine);
    assert_eq!(
        node_accept_sync(&push, &node.did(), Some(&stored), READ_ONLY, &writer(&desktop), 2, &HashSet::new())
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
        node_accept_sync(&pull, &node.did(), Some(&stored), WRITE_ONLY, &writer(&desktop), 2, &HashSet::new())
            .unwrap_err(),
        CourierError::NoRead
    );

    let fresh = doc_of(&desktop);
    fresh.get_text("t").insert(0, "mine").unwrap();
    let create = doc_hello(&desktop, member(&node, &desktop), "dm", &fresh);
    node_accept_sync(&create, &node.did(), None, WRITE_ONLY, &writer(&desktop), 2, &HashSet::new()).unwrap();
    // Opening a doc syncs it empty before its creator writes: that copy discloses nothing.
    let empty = held(&LoroDoc::new());
    node_accept_sync(&create, &node.did(), Some(&empty), WRITE_ONLY, &writer(&desktop), 2, &HashSet::new()).unwrap();
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
    let doc = doc_of(&guest);
    doc.get_text("t").insert(0, "card").unwrap();
    let own = doc_hello(&guest, token.clone(), "board", &doc);
    node_accept_sync(&own, &node.did(), None, Access::OPEN, &writer(&guest), 2, &HashSet::new()).unwrap();

    let sibling = doc_hello(&guest, token.clone(), "notes", &doc);
    assert_eq!(
        node_accept_sync(&sibling, &node.did(), None, Access::OPEN, &writer(&guest), 2, &HashSet::new())
            .unwrap_err(),
        CourierError::OutOfScope
    );

    let src = src_hello(&guest, token, &LoroDoc::new());
    node_accept_sync(&src, &node.did(), None, Access::OPEN, &writer(&guest), 2, &HashSet::new()).unwrap();
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
            node_accept_sync(&hello, &node.did(), None, Access::OPEN, &writer(&desktop), 2, &HashSet::new())
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

// ── step 6: rejection and rollback ───────────────────────────────────────────

const NO_ACCESS: Access = Access {
    read: false,
    write: false,
};

/// What a node would answer: the refusal `node_accept_sync` raises, turned into a rejection.
fn refuse(
    hello: &SyncHello,
    node: &Identity,
    who: &Identity,
    held: Option<&[u8]>,
    access: Access,
) -> Rejected {
    let why =
        node_accept_sync(hello, &node.did(), held, access, &writer(who), 2, &HashSet::new()).unwrap_err();
    node_reject(hello, &why, held, access)
        .unwrap()
        .unwrap_or_else(|| panic!("{why:?} earned no rejection"))
}

fn text_of(doc: &LoroDoc) -> String {
    doc.get_text("t").to_string()
}

#[test]
fn a_refused_write_rolls_back_to_the_nodes_copy() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "admin's").unwrap();
    let stored = held(&on_node);

    let mine = doc_of(&desktop);
    mine.import(&stored).unwrap();
    let since = mine.oplog_vv().encode();
    mine.get_text("t").insert(0, "member's ").unwrap();
    let push = hello_since(&desktop, member(&node, &desktop), &mine, &since);
    let rejected = refuse(&push, &node, &desktop, Some(&stored), READ_ONLY);

    assert_eq!(rejected.request_id, push.request_id);
    assert!(rejected.reason.contains("may not write"), "{}", rejected.reason);
    let back = desktop_roll_back(&mine, Some(&since), &rejected).unwrap();
    assert_eq!(text_of(&back), "admin's");
    assert_eq!(back.oplog_vv(), on_node.oplog_vv());
}

/// Only the refused batch is lost: what the writer already shared with the node stays, and
/// what others wrote meanwhile arrives.
#[test]
fn rollback_drops_only_the_refused_batch() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "base").unwrap();
    let mine = doc_of(&desktop);
    mine.import(&held(&on_node)).unwrap();

    // A third party's edit reaches both; another reaches only the node.
    let other = LoroDoc::new();
    other.import(&held(&on_node)).unwrap();
    other.get_text("t").insert(4, " seen").unwrap();
    on_node.import(&held(&other)).unwrap();
    mine.import(&held(&other)).unwrap();
    let since = mine.oplog_vv().encode();
    other.get_text("t").insert(9, " unseen").unwrap();
    on_node.import(&held(&other)).unwrap();

    mine.get_text("t").insert(0, "refused ").unwrap();
    let push = hello_since(&desktop, member(&node, &desktop), &mine, &since);
    let rejected = refuse(&push, &node, &desktop, Some(&held(&on_node)), READ_ONLY);
    let back = desktop_roll_back(&mine, Some(&since), &rejected).unwrap();
    assert_eq!(text_of(&back), "base seen unseen");
    assert_eq!(back.oplog_vv(), on_node.oplog_vv());
}

/// The rolled-back doc syncs on as normal: its next push is a diff the node accepts.
#[test]
fn a_rolled_back_doc_syncs_on() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "admin's").unwrap();
    let stored = held(&on_node);
    let mine = doc_of(&desktop);
    mine.import(&stored).unwrap();
    let since = mine.oplog_vv().encode();
    mine.get_text("t").insert(0, "member's ").unwrap();
    let push = hello_since(&desktop, member(&node, &desktop), &mine, &since);
    let rejected = refuse(&push, &node, &desktop, Some(&stored), READ_ONLY);
    let back = desktop_roll_back(&mine, Some(&since), &rejected).unwrap();

    back.get_text("t").insert(7, "!").unwrap();
    let vv = rejected.state.as_ref().unwrap().vv.clone();
    let next = hello_since(&desktop, member(&node, &desktop), &back, &vv);
    let (ack, snapshot) =
        node_accept_sync(&next, &node.did(), Some(&stored), Access::OPEN, &writer(&desktop), 2, &HashSet::new()).unwrap();
    assert!(!ack.missing);
    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(text_of(&landed), "admin's!");
}

/// A writer who may not read gets no state; it rolls back to what the node last acked.
#[test]
fn without_read_a_rejection_discloses_nothing_and_rolls_back_to_the_last_ack() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "alice's").unwrap();
    let stored = held(&on_node);

    let mine = doc_of(&desktop);
    mine.get_text("t").insert(0, "mine").unwrap();
    let since = mine.oplog_vv().encode();
    mine.get_text("t").insert(4, " and mallory's").unwrap();
    let push = hello_since(&desktop, member(&node, &desktop), &mine, &since);
    let rejected = refuse(&push, &node, &desktop, Some(&stored), NO_ACCESS);
    assert_eq!(rejected.state, None);
    assert_eq!(text_of(&desktop_roll_back(&mine, Some(&since), &rejected).unwrap()), "mine");
    // Never acked: nothing of it is the node's.
    assert_eq!(text_of(&desktop_roll_back(&mine, None, &rejected).unwrap()), "");
}

#[test]
fn a_refused_source_change_rolls_back() {
    let (node, owner) = ids();
    let (_, member) = ids();
    let ws = || Scope::Workspace("ws1".into());
    let src = doc_of(&owner);
    src.get_text("manifest.osv").insert(0, "app \"x\" {}").unwrap();
    let push = src_hello(&owner, ws_token(&node, &owner, "owner", ws(), 1), &src);
    let (_, stored) =
        node_accept_sync(&push, &node.did(), None, Access::OPEN, &writer(&owner), 2, &HashSet::new()).unwrap();

    let forged = doc_of(&member);
    forged.import(&stored).unwrap();
    let since = forged.oplog_vv().encode();
    forged.get_text("manifest.osv").insert(0, "-- mine\n").unwrap();
    let forge = src_hello(&member, ws_token(&node, &member, "member", ws(), 1), &forged);
    let rejected = refuse(&forge, &node, &member, Some(&stored), Access::OPEN);
    let back = desktop_roll_back(&forged, Some(&since), &rejected).unwrap();
    assert_eq!(back.get_text("manifest.osv").to_string(), "app \"x\" {}");
}

/// Nothing to roll back: a refused pull, and a refusal of the caller rather than the write.
#[test]
fn a_refused_pull_or_caller_earns_no_rejection() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "secret").unwrap();
    let stored = held(&on_node);

    let pull = doc_hello(&desktop, member(&node, &desktop), "dm", &LoroDoc::new());
    assert_eq!(node_reject(&pull, &CourierError::NoRead, Some(&stored), NO_ACCESS).unwrap(), None);

    let mine = doc_of(&desktop);
    mine.get_text("t").insert(0, "x").unwrap();
    let push = doc_hello(&desktop, member(&node, &desktop), "dm", &mine);
    for why in [
        CourierError::Revoked,
        CourierError::OutOfScope,
        CourierError::Expired,
        CourierError::Decode,
        CourierError::ForeignPeer,
    ] {
        assert_eq!(node_reject(&push, &why, Some(&stored), Access::OPEN).unwrap(), None, "{why:?}");
    }
}

// ── owned-entries step 1: a change carries its device ────────────────────────

/// O5: a push stamped with another device's peer is refused. It earns no rollback — the
/// writer keeps its copy (a rollback would cut an honest writer back after a node restore).
#[test]
fn a_change_in_another_devices_name_is_refused() {
    let (node, alice) = ids();
    let (_, bob) = ids();
    let forged = doc_of(&bob);
    forged.set_peer_id(peer_of(&device_key(&alice))).unwrap();
    forged.get_text("t").insert(0, "as alice").unwrap();
    let push = hello(bob.did(), member(&node, &bob), "ws1", &forged);
    let why = node_accept_sync(&push, &node.did(), None, Access::OPEN, &writer(&bob), 2, &HashSet::new())
        .unwrap_err();
    assert_eq!(why, CourierError::ForeignPeer);
    assert_eq!(node_reject(&push, &why, None, Access::OPEN).unwrap(), None);
}

/// A first push carries the history a member pulled: others' changes the node already holds
/// pass, and only what the node lacks is judged by the pusher's peer.
#[test]
fn a_push_may_carry_what_the_node_already_holds_from_other_devices() {
    let (node, alice) = ids();
    let (_, bob) = ids();
    let alices = doc_of(&alice);
    alices.get_text("t").insert(0, "alice").unwrap();
    let stored = held(&alices);

    let bobs = doc_of(&bob);
    bobs.import(&stored).unwrap();
    bobs.get_text("t").insert(5, " bob").unwrap();
    let push = hello(bob.did(), member(&node, &bob), "ws1", &bobs);
    let (_, snapshot) =
        node_accept_sync(&push, &node.did(), Some(&stored), Access::OPEN, &writer(&bob), 2, &HashSet::new())
            .unwrap();
    let landed = LoroDoc::new();
    landed.import(&snapshot).unwrap();
    assert_eq!(text_of(&landed), "alice bob");
}

/// What the node never held in another device's name is refused even from an honest member
/// (a node restored from backup, a doc passed device to device): without signatures the node
/// cannot tell it from a forgery. A host relaying what it judged already is not checked.
#[test]
fn what_the_node_never_held_from_another_device_is_refused() {
    let (node, alice) = ids();
    let (_, bob) = ids();
    let alices = doc_of(&alice);
    alices.get_text("t").insert(0, "alice").unwrap();
    let bobs = doc_of(&bob);
    bobs.import(&held(&alices)).unwrap();
    bobs.get_text("t").insert(5, " bob").unwrap();
    let push = hello(bob.did(), member(&node, &bob), "ws1", &bobs);
    assert_eq!(
        node_accept_sync(&push, &node.did(), None, Access::OPEN, &writer(&bob), 2, &HashSet::new())
            .unwrap_err(),
        CourierError::ForeignPeer
    );
    node_accept_sync(&push, &node.did(), None, Access::OPEN, &host_writer(), 2, &HashSet::new()).unwrap();
}

/// The doc a rollback hands back writes on under the device's own peer, not the fork's.
#[test]
fn a_rolled_back_doc_keeps_its_devices_peer() {
    let (node, desktop) = ids();
    let on_node = LoroDoc::new();
    on_node.get_text("t").insert(0, "admin's").unwrap();
    let stored = held(&on_node);
    let mine = doc_of(&desktop);
    mine.import(&stored).unwrap();
    let since = mine.oplog_vv().encode();
    mine.get_text("t").insert(0, "member's ").unwrap();
    let push = hello_since(&desktop, member(&node, &desktop), &mine, &since);
    let rejected = refuse(&push, &node, &desktop, Some(&stored), READ_ONLY);
    let back = desktop_roll_back(&mine, Some(&since), &rejected).unwrap();
    assert_eq!(back.peer_id(), mine.peer_id());
}

// ── owned-entries step 3: the rules on the host ──────────────────────────────

const CHAT: &str = r#"
app "chat" {
  roles admin, member
  doc channel/{cid} { read member  write member  own messages/* override admin }
}
"#;

/// A member's push judged by the chat manifest's rules on `channel/general`.
fn ruled<'a>(m: &'a Manifest, desktop: &Identity, roles: &[&str]) -> Writer<'a> {
    let (decl, _) = m.resolve("channel/general").unwrap();
    Writer {
        author: author(desktop),
        roles: roles.iter().map(|r| r.to_string()).collect(),
        rules: Some((m, decl)),
    }
}

fn chat_hello(desktop: &Identity, token: Token, doc: &LoroDoc, since: Option<&[u8]>) -> SyncHello {
    desktop_start_sync(
        desktop.did(),
        token,
        "ws1",
        "item1",
        SyncLayer::Doc("channel/general".to_string()),
        doc,
        since,
    )
    .unwrap()
}

fn post(doc: &LoroDoc, at: usize, text: &str) {
    doc.get_movable_list("messages")
        .insert_container(at, LoroMap::new())
        .unwrap()
        .insert("text", text)
        .unwrap();
    doc.commit();
}

fn texts(doc: &LoroDoc) -> Vec<String> {
    let n = doc.get_movable_list("messages").len();
    (0..n)
        .map(|i| match doc.get_by_str_path(&format!("messages/{i}/text")).unwrap().get_deep_value() {
            loro::LoroValue::String(s) => s.to_string(),
            v => panic!("{v:?}"),
        })
        .collect()
}

/// O3, O4: the host refuses a member deleting or editing another's message, the live doc is
/// untouched, and the rejection rolls the writer back.
#[test]
fn a_member_may_not_delete_or_edit_anothers_message_and_rolls_back() {
    let m = Manifest::parse(CHAT).unwrap();
    let (node, alice) = ids();
    let (_, bob) = ids();
    let alices = doc_of(&alice);
    post(&alices, 0, "alice's");
    let stored = held(&alices);
    let edits: [(&str, Box<dyn Fn(&LoroDoc)>); 2] = [
        ("delete", Box::new(|d| d.get_movable_list("messages").delete(0, 1).unwrap())),
        ("change", Box::new(|d| {
            let msg = d.get_by_str_path("messages/0").unwrap().into_container().unwrap().into_map().unwrap();
            msg.insert("text", "bob's").unwrap();
        })),
    ];
    for (what, edit) in edits {
        let bobs = doc_of(&bob);
        bobs.import(&stored).unwrap();
        let since = bobs.oplog_vv().encode();
        edit(&bobs);
        let push = chat_hello(&bob, member(&node, &bob), &bobs, Some(&since));
        let live = LoroDoc::new();
        live.import(&stored).unwrap();
        let why = accept_into(&live, &push, &node.did(), Access::OPEN, &ruled(&m, &bob, &["member"]), 2, &HashSet::new())
            .unwrap_err();
        assert!(matches!(why, CourierError::Forbidden(_)), "{what}: {why:?}");
        assert_eq!(live.oplog_vv(), alices.oplog_vv(), "{what}: the live doc was touched");
        let rejected = node_reject(&push, &why, Some(&stored), Access::OPEN).unwrap().expect("a rollback");
        let back = desktop_roll_back(&bobs, Some(&since), &rejected).unwrap();
        assert_eq!(texts(&back), ["alice's"], "{what}");
    }
}

/// O6, O7: a member's own message, and an admin over anyone's.
#[test]
fn owners_and_admins_may_delete() {
    let m = Manifest::parse(CHAT).unwrap();
    let (node, alice) = ids();
    let (_, bob) = ids();
    let alices = doc_of(&alice);
    post(&alices, 0, "alice's");
    let stored = held(&alices);

    let bobs = doc_of(&bob);
    bobs.import(&stored).unwrap();
    post(&bobs, 1, "bob's");
    let push = chat_hello(&bob, member(&node, &bob), &bobs, None);
    let (ack, stored) =
        node_accept_sync(&push, &node.did(), Some(&stored), Access::OPEN, &ruled(&m, &bob, &["member"]), 2, &HashSet::new())
            .unwrap();
    bobs.get_movable_list("messages").delete(1, 1).unwrap();
    let push = chat_hello(&bob, member(&node, &bob), &bobs, Some(&ack.vv));
    let (_, stored) =
        node_accept_sync(&push, &node.did(), Some(&stored), Access::OPEN, &ruled(&m, &bob, &["member"]), 3, &HashSet::new())
            .unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(texts(&landed), ["alice's"]);

    let since = bobs.oplog_vv().encode();
    bobs.get_movable_list("messages").delete(0, 1).unwrap();
    let push = chat_hello(&bob, member(&node, &bob), &bobs, Some(&since));
    assert!(matches!(
        node_accept_sync(&push, &node.did(), Some(&stored), Access::OPEN, &ruled(&m, &bob, &["member"]), 4, &HashSet::new())
            .unwrap_err(),
        CourierError::Forbidden(_)
    ));
    let (_, stored) =
        node_accept_sync(&push, &node.did(), Some(&stored), Access::OPEN, &ruled(&m, &bob, &["admin"]), 4, &HashSet::new())
            .unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(texts(&landed), Vec::<String>::new());
}

/// A push whose dependencies the node lacks is merged nowhere and flagged `missing`, so the
/// writer pushes everything and the whole of it is judged.
#[test]
fn a_push_with_missing_dependencies_is_held_out_and_flagged_missing() {
    let m = Manifest::parse(CHAT).unwrap();
    let (node, bob) = ids();
    let bobs = doc_of(&bob);
    post(&bobs, 0, "first");
    let after_first = bobs.oplog_vv().encode();
    post(&bobs, 1, "second");
    let push = chat_hello(&bob, member(&node, &bob), &bobs, Some(&after_first));
    let (ack, stored) =
        node_accept_sync(&push, &node.did(), None, Access::OPEN, &ruled(&m, &bob, &["member"]), 2, &HashSet::new())
            .unwrap();
    assert!(ack.missing);
    assert!(!ack.changed);
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(texts(&landed), Vec::<String>::new());
}
