use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courier::ConnectionTicket;
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::sync::{SyncLayer, desktop_start_sync};
use courier::token::Scope;
use kunki::push::LiveRegistry;
use loro::LoroDoc;
use tempfile::TempDir;
use vault::Vault;

use super::{
    claim, claim_invite, invite, join_item, listen, next_push, ping, publish, pull_doc, subscribe,
    sync,
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A node with a fresh identity, served on its own socket in a background thread — same
/// process as the test, so a shared env var (what `serve_forever` reads) would race against
/// every other test doing the same thing; `serve` takes the path directly instead.
fn spawn_node() -> (TempDir, PathBuf, ConnectionTicket, Vault) {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("bridge.sock");
    let (vault, _) = kunki::node::open(dir.path().to_path_buf(), "pw").unwrap();
    let ticket = vault
        .with_signer(|node| courier::issue_connection_ticket(node, now(), "kunki"))
        .unwrap()
        .unwrap();
    let bound = socket.clone();
    let served = vault.clone();
    thread::spawn(move || kunki::bridge::serve(&bound, served, LiveRegistry::new()).unwrap());
    wait_for(&socket);
    (dir, socket, ticket, vault)
}

/// The listener binds on its own thread, asynchronously to this one — bounded polling instead
/// of a fixed sleep, so this is as fast as the thread actually is and never flaky under load.
fn wait_for(socket: &Path) {
    for _ in 0..200 {
        if ping(socket) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("kunki bridge never came up at {socket:?}");
}

fn desktop_vault() -> (Vault, TempDir) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("alice", "pw").unwrap();
    (vault, tmp)
}

#[test]
fn claiming_a_freshly_booted_node_records_that_node() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (desktop, _desktop_dir) = desktop_vault();

    let record = claim(&socket, &desktop, &ticket, now()).unwrap();

    assert_eq!(record.node_did, ticket.node_did);
    assert_eq!(record.node_id, ticket.node_id);
}

#[test]
fn a_second_desktop_cannot_claim_a_node_that_already_has_an_admin() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    claim(&socket, &alice, &ticket, now()).unwrap();

    let (bob, _bob_dir) = desktop_vault();
    assert!(claim(&socket, &bob, &ticket, now()).is_err());
}

#[test]
fn publishing_a_workspace_lands_it_in_the_nodes_vault() {
    let (_node_dir, socket, ticket, node_vault) = spawn_node();
    let (desktop, _desktop_dir) = desktop_vault();
    let record = claim(&socket, &desktop, &ticket, now()).unwrap();

    let workspace = PublishedWorkspace {
        id: "a".repeat(32),
        name: "notes".to_string(),
        created: 1,
    };
    let item = PublishedItem {
        id: "b".repeat(32),
        name: "board".to_string(),
        kind: "app".to_string(),
        created: 2,
    };
    publish(
        &socket,
        &desktop,
        record.token,
        workspace.clone(),
        vec![item],
    )
    .unwrap();

    assert_eq!(node_vault.items(&workspace.id).unwrap().len(), 1);
}

#[test]
fn syncing_a_doc_lands_its_content_in_the_nodes_vault() {
    let (_node_dir, socket, ticket, node_vault) = spawn_node();
    let (desktop, _desktop_dir) = desktop_vault();
    let record = claim(&socket, &desktop, &ticket, now()).unwrap();
    let desktop_did = desktop.with_signer(|d| d.did().to_string()).unwrap();

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let hello = desktop_start_sync(
        &desktop_did,
        record.token,
        &ws_id,
        &item_id,
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();

    sync(&socket, hello).unwrap();

    let stored = node_vault
        .get_doc(&ws_id, &item_id, "board")
        .unwrap()
        .unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");
}

#[test]
fn an_invite_lets_a_second_desktop_join_without_claiming() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    let record = claim(&socket, &alice, &ticket, now()).unwrap();

    let invite_ticket = invite(&socket, &alice, record.token, "member", Scope::Node).unwrap();

    let (bob, _bob_dir) = desktop_vault();
    let bob_record = claim_invite(&socket, &bob, invite_ticket, now()).unwrap();

    assert_eq!(bob_record.node_did, record.node_did);
}

#[test]
fn joining_an_item_pulls_its_current_source_and_registers_it_locally() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    let alice_record = claim(&socket, &alice, &ticket, now()).unwrap();
    let alice_did = alice.with_signer(|d| d.did().to_string()).unwrap();

    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let doc = LoroDoc::new();
    doc.get_text("main.lua").insert(0, "-- hello").unwrap();
    let hello = desktop_start_sync(
        &alice_did,
        alice_record.token.clone(),
        &ws_id,
        &item_id,
        SyncLayer::Src,
        &doc,
        None,
    )
    .unwrap();
    sync(&socket, hello).unwrap();

    let invite_ticket = invite(&socket, &alice, alice_record.token, "member", Scope::Node).unwrap();
    let (bob, _bob_dir) = desktop_vault();
    let bob_record = claim_invite(&socket, &bob, invite_ticket, now()).unwrap();

    let ws = vault::WorkspaceMeta {
        id: ws_id.clone(),
        name: "notes".to_string(),
        created: now(),
    };
    let item = vault::WorkspaceItem {
        id: item_id.clone(),
        ws_id: ws_id.clone(),
        name: "board".to_string(),
        kind: vault::ItemKind::App,
        created: now(),
    };
    join_item(&socket, &bob, bob_record.token, ws, item).unwrap();

    let stored = bob.get_src(&ws_id, &item_id).unwrap().unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(landed.get_text("main.lua").to_string(), "-- hello");
    assert_eq!(bob.items(&ws_id).unwrap().len(), 1);
}

#[test]
fn a_subscribed_desktop_receives_a_push_via_listen_when_another_desktop_syncs() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    let alice_record = claim(&socket, &alice, &ticket, now()).unwrap();
    let alice_did = alice.with_signer(|d| d.did().to_string()).unwrap();

    let invite_ticket = invite(
        &socket,
        &alice,
        alice_record.token.clone(),
        "member",
        Scope::Node,
    )
    .unwrap();
    let (bob, _bob_dir) = desktop_vault();
    let bob_record = claim_invite(&socket, &bob, invite_ticket, now()).unwrap();
    let bob_did = bob.with_signer(|d| d.did().to_string()).unwrap();

    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let layer = SyncLayer::Doc("board".to_string());

    subscribe(
        &socket,
        &bob_did,
        bob_record.token.clone(),
        &ws_id,
        &item_id,
        layer.clone(),
    )
    .unwrap();
    let mut conn = listen(&socket, &bob_did, bob_record.token).unwrap();

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let hello = desktop_start_sync(
        &alice_did,
        alice_record.token,
        &ws_id,
        &item_id,
        layer,
        &doc,
        None,
    )
    .unwrap();
    sync(&socket, hello).unwrap();

    let push = next_push(&mut conn).unwrap();
    assert_eq!(push.item_id, item_id);
    let landed = LoroDoc::new();
    landed.import(&push.snapshot).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");
}

#[test]
fn pull_doc_returns_content_another_desktop_already_synced() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    let alice_record = claim(&socket, &alice, &ticket, now()).unwrap();
    let alice_did = alice.with_signer(|d| d.did().to_string()).unwrap();

    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello from alice").unwrap();
    let hello = desktop_start_sync(
        &alice_did,
        alice_record.token.clone(),
        &ws_id,
        &item_id,
        SyncLayer::Doc("scratch".to_string()),
        &doc,
        None,
    )
    .unwrap();
    sync(&socket, hello).unwrap();

    // Bob has never touched this item, but pulls straight from the node — this is what
    // keeps his app from mistaking "not synced to me yet" for "nobody's written this".
    let (bob, _bob_dir) = desktop_vault();
    let bob_did = bob.with_signer(|d| d.did().to_string()).unwrap();
    let invite_ticket = invite(&socket, &alice, alice_record.token, "member", Scope::Node).unwrap();
    let bob_record = claim_invite(&socket, &bob, invite_ticket, now()).unwrap();

    let bytes = pull_doc(
        &socket,
        &bob,
        &bob_did,
        bob_record.token,
        &ws_id,
        &item_id,
        "scratch",
    )
    .expect("the node had content for this layer");

    let landed = LoroDoc::new();
    landed.import(&bytes).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello from alice");
    // Also cached locally, so a second open of the same item doesn't need the node again.
    assert_eq!(
        bob.get_doc(&ws_id, &item_id, "scratch").unwrap().unwrap(),
        bytes
    );
}

#[test]
fn pull_doc_returns_none_when_nobody_has_written_that_layer() {
    let (_node_dir, socket, ticket, _node_vault) = spawn_node();
    let (alice, _alice_dir) = desktop_vault();
    let alice_record = claim(&socket, &alice, &ticket, now()).unwrap();
    let alice_did = alice.with_signer(|d| d.did().to_string()).unwrap();

    let result = pull_doc(
        &socket,
        &alice,
        &alice_did,
        alice_record.token,
        &"a".repeat(32),
        &"b".repeat(32),
        "scratch",
    );
    assert!(result.is_none());
}
