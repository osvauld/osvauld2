use std::os::unix::net::UnixStream;

use courier::invite::{InviteRequest, InviteTicket, desktop_start_invite_claim};
use courier::proof;
use courier::publish::{PublishedItem, PublishedWorkspace, desktop_publish};
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncLayer, desktop_start_sync};
use courier::token::{Scope, Token};
use identity::Identity;
use loro::LoroDoc;
use osvauld_rpc::{read_msg, write_msg};
use tempfile::TempDir;

use super::*;
use crate::admin::Admin;
use crate::push::{LiveRegistry, NoopPusher, Push};

/// The bridge itself reads the real clock (`now_secs` above), so a claim minted here needs a
/// real `now` too — a token from a fixed past timestamp would already read as expired by the
/// time `dispatch` checks it.
fn now() -> u64 {
    now_secs()
}

fn node_vault() -> (TempDir, Vault) {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = crate::node::open(tmp.path().to_path_buf(), "pw").unwrap();
    (tmp, vault)
}

/// Drives a full claim over a real socket — the same wire path a fresh desktop uses, and the
/// front door every other test here has to get through first.
fn claim(vault: &Vault, desktop: &Identity) -> Token {
    let now = now();
    let ticket = vault
        .with_signer(|node| courier::issue_connection_ticket(node, now, "kunki"))
        .unwrap()
        .unwrap();
    let hello = courier::desktop_start_claim(ticket.clone(), desktop, now).unwrap();
    let welcome = match roundtrip(vault, &Request::Claim(hello)) {
        Response::Ok { result } => serde_json::from_value(result).unwrap(),
        other => panic!("{other:?}"),
    };
    courier::desktop_finish_claim(&ticket, welcome, desktop, now)
        .unwrap()
        .token
}

/// One request over a real `UnixStream`, the same pair `handle` sees in `serve_forever` —
/// proves the wire framing and JSON tagging round-trip, not only `Admin::accept_publish`.
/// Unsigned: for the requests that carry no caller (`Ping`, the two claims).
fn roundtrip(vault: &Vault, req: &Request) -> Response {
    send(vault, &mut gate(vault), &Envelope::plain(req).unwrap())
}

/// Signed by `who`, as a desktop calling on its own behalf.
fn roundtrip_as(vault: &Vault, who: &Identity, req: &Request) -> Response {
    send(vault, &mut gate(vault), &sealed(vault, who, req))
}

fn node_did(vault: &Vault) -> String {
    vault.with_signer(|node| node.did().to_string()).unwrap()
}

fn gate(vault: &Vault) -> Gate {
    Gate::new(node_did(vault), now_ms() - 1000)
}

fn sealed(vault: &Vault, who: &Identity, req: &Request) -> Envelope {
    Envelope::signed(req, who, &node_did(vault), now_ms()).unwrap()
}

fn send(vault: &Vault, gate: &mut Gate, env: &Envelope) -> Response {
    let (mut client, server) = UnixStream::pair().unwrap();
    write_msg(&mut client, &serde_json::to_vec(env).unwrap()).unwrap();
    handle(server, vault, gate, &NoopPusher).unwrap();
    serde_json::from_slice(&read_msg(&mut client).unwrap()).unwrap()
}

fn is_err_containing(resp: &Response, needle: &str) -> bool {
    matches!(resp, Response::Err { message } if message.contains(needle))
}

#[test]
fn ping_answers_pong_over_a_real_socket() {
    let (_tmp, vault) = node_vault();
    assert!(matches!(
        roundtrip(&vault, &Request::Ping),
        Response::Ok { result } if result == "pong"
    ));
}

#[test]
fn a_claim_is_answered_over_the_wire() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    claim(&vault, &alice);
}

#[test]
fn a_second_claim_is_refused_once_the_node_already_has_an_admin() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    claim(&vault, &alice);

    let bob = identity::generate().0;
    let now = now();
    let ticket = vault
        .with_signer(|node| courier::issue_connection_ticket(node, now, "kunki"))
        .unwrap()
        .unwrap();
    let hello = courier::desktop_start_claim(ticket, &bob, now).unwrap();
    let resp = roundtrip(&vault, &Request::Claim(hello));
    assert!(matches!(resp, Response::Err { .. }), "{resp:?}");
}

#[test]
fn a_publish_lands_in_the_nodes_vault_over_the_wire() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);

    let ws = PublishedWorkspace {
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
    let hello = desktop_publish(alice.did(), token, ws.clone(), vec![item]);

    let resp = roundtrip_as(&vault, &alice, &Request::Publish(hello));
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
    assert_eq!(vault.items(&ws.id).unwrap().len(), 1);
}

#[test]
fn an_invite_is_minted_and_redeemed_over_the_wire() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let bob = identity::generate().0;

    let request = InviteRequest {
        desktop_did: alice.did().to_string(),
        token,
        role: "member".to_string(),
        scope: Scope::Workspace("a".repeat(32)),
        public: false,
    };
    let ticket: InviteTicket = match roundtrip_as(&vault, &alice, &Request::Invite(request)) {
        Response::Ok { result } => serde_json::from_value(result).unwrap(),
        other => panic!("{other:?}"),
    };

    let hello = desktop_start_invite_claim(ticket, &bob, now()).unwrap();
    let resp = roundtrip(&vault, &Request::ClaimInvite(hello));
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
}

#[test]
fn a_sync_push_lands_in_the_nodes_vault_over_the_wire() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let hello = desktop_start_sync(
        alice.did(),
        token,
        &ws_id,
        &item_id,
        SyncLayer::Doc("board".to_string()),
        &doc,
        None,
    )
    .unwrap();

    let resp = roundtrip_as(&vault, &alice, &Request::Sync(hello));
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");

    let stored = vault.get_doc(&ws_id, &item_id, "board").unwrap().unwrap();
    let landed = LoroDoc::new();
    landed.import(&stored).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");
}

#[test]
fn a_subscribe_and_unsubscribe_round_trip_over_the_wire() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let layer = SyncLayer::Doc("board".to_string());

    let hello = desktop_start_subscribe(alice.did(), token, &ws_id, &item_id, layer.clone());
    let resp = roundtrip_as(&vault, &alice, &Request::Subscribe(hello.clone()));
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
    assert_eq!(
        admin.subscribers_for(&ws_id, &item_id, &layer).unwrap(),
        vec![alice.did().to_string()]
    );

    let resp = roundtrip_as(&vault, &alice, &Request::Unsubscribe(hello));
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
    assert!(
        admin
            .subscribers_for(&ws_id, &item_id, &layer)
            .unwrap()
            .is_empty()
    );
}

/// Waits for a real bound socket to answer `Ping` — `serve`'s accept loop binds on its own
/// thread, asynchronously to this one, so this is bounded polling rather than a guessed sleep.
fn wait_for_bridge(socket: &std::path::Path) {
    for _ in 0..200 {
        if let Ok(mut conn) = UnixStream::connect(socket) {
            let req = serde_json::to_vec(&Envelope::plain(&Request::Ping).unwrap()).unwrap();
            if write_msg(&mut conn, &req).is_ok() {
                if let Ok(bytes) = read_msg(&mut conn) {
                    if matches!(serde_json::from_slice(&bytes), Ok(Response::Ok { .. })) {
                        return;
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("bridge never came up at {socket:?}");
}

#[test]
fn a_listening_desktop_receives_a_push_when_another_desktop_syncs() {
    let (_tmp, vault) = node_vault();
    let admin = Admin::new(vault.clone());
    let alice = identity::generate().0;
    let alice_token = claim(&vault, &alice);

    // Bob's membership and subscription are set up in-process — already covered by their own
    // tests elsewhere. What's new and under test here is the live `Listen` connection.
    let bob = identity::generate().0;
    let invite_ticket = admin
        .issue_invite(
            &InviteRequest {
                desktop_did: alice.did().to_string(),
                token: alice_token.clone(),
                role: "member".to_string(),
                scope: Scope::Node,
                public: false,
            },
            "kunki",
            now(),
        )
        .unwrap();
    let bob_hello = desktop_start_invite_claim(invite_ticket, &bob, now()).unwrap();
    let bob_token = admin.accept_invite(bob_hello, now()).unwrap().token;

    let ws_id = "a".repeat(32);
    let item_id = "b".repeat(32);
    let layer = SyncLayer::Doc("board".to_string());
    let sub_hello = desktop_start_subscribe(
        &bob.did(),
        bob_token.clone(),
        &ws_id,
        &item_id,
        layer.clone(),
    );
    admin.subscribe(&sub_hello, now()).unwrap();

    // A real bound socket this time, not `UnixStream::pair` — `Listen`'s special-casing only
    // happens inside `serve`'s own accept loop.
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("bridge.sock");
    let registry = LiveRegistry::new();
    let (served_vault, served_registry, bound) = (vault.clone(), registry.clone(), socket.clone());
    std::thread::spawn(move || serve(&bound, served_vault, served_registry).unwrap());
    wait_for_bridge(&socket);

    let mut conn = UnixStream::connect(&socket).unwrap();
    let listen = Request::Listen {
        desktop_did: bob.did().to_string(),
        token: bob_token,
    };
    let listen = sealed(&vault, &bob, &listen);
    write_msg(&mut conn, &serde_json::to_vec(&listen).unwrap()).unwrap();
    let ack: Response = serde_json::from_slice(&read_msg(&mut conn).unwrap()).unwrap();
    assert!(matches!(ack, Response::Ok { .. }), "{ack:?}");

    // Alice syncs — in-process against the same registry the socket above is registered on,
    // the same way a real `Sync` request arriving over the wire would reach it.
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    let sync_hello = desktop_start_sync(
        &alice.did(),
        alice_token,
        &ws_id,
        &item_id,
        layer,
        &doc,
        None,
    )
    .unwrap();
    admin.accept_sync(sync_hello, now(), &registry).unwrap();

    let push: Push = serde_json::from_slice(&read_msg(&mut conn).unwrap()).unwrap();
    assert_eq!(push.item_id, item_id);
    let landed = LoroDoc::new();
    landed.import(&push.snapshot).unwrap();
    assert_eq!(landed.get_text("t").to_string(), "hello");
}

fn alice_sync(alice: &Identity, token: Token) -> Request {
    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "hello").unwrap();
    Request::Sync(
        desktop_start_sync(
            alice.did(),
            token,
            &"a".repeat(32),
            &"b".repeat(32),
            SyncLayer::Doc("board".to_string()),
            &doc,
            None,
        )
        .unwrap(),
    )
}

#[test]
fn a_request_naming_alice_but_signed_by_bob_is_refused() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let bob = identity::generate().0;

    let req = alice_sync(&alice, token);
    // Built by hand: `Envelope::signed` itself refuses to sign for someone else.
    let body = serde_json::value::to_raw_value(&req).unwrap();
    let proof = proof::prove(&bob, &node_did(&vault), body.get().as_bytes(), now_ms());
    let forged = Envelope {
        body,
        proof: Some(proof),
    };

    let resp = send(&vault, &mut gate(&vault), &forged);
    assert!(is_err_containing(&resp, "signature"), "{resp:?}");
    assert!(
        vault
            .get_doc(&"a".repeat(32), &"b".repeat(32), "board")
            .unwrap()
            .is_none()
    );
}

#[test]
fn envelope_signed_refuses_to_sign_for_another_desktop() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let bob = identity::generate().0;
    let req = alice_sync(&alice, token);
    assert!(Envelope::signed(&req, &bob, &node_did(&vault), now_ms()).is_err());
}

#[test]
fn an_unsigned_request_on_behalf_of_a_desktop_is_refused() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let req = alice_sync(&alice, token);
    let resp = send(&vault, &mut gate(&vault), &Envelope::plain(&req).unwrap());
    assert!(is_err_containing(&resp, "unsigned"), "{resp:?}");
}

#[test]
fn a_replayed_request_is_refused() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let env = sealed(&vault, &alice, &alice_sync(&alice, token));
    let mut gate = gate(&vault);

    let first = send(&vault, &mut gate, &env);
    assert!(matches!(first, Response::Ok { .. }), "{first:?}");
    let again = send(&vault, &mut gate, &env);
    assert!(is_err_containing(&again, "replay"), "{again:?}");
}

#[test]
fn a_listen_for_alice_signed_by_bob_is_refused() {
    let (_tmp, vault) = node_vault();
    let alice = identity::generate().0;
    let token = claim(&vault, &alice);
    let bob = identity::generate().0;

    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("bridge.sock");
    let (served, bound) = (vault.clone(), socket.clone());
    std::thread::spawn(move || serve(&bound, served, LiveRegistry::new()).unwrap());
    wait_for_bridge(&socket);

    let listen = Request::Listen {
        desktop_did: alice.did().to_string(),
        token,
    };
    let body = serde_json::value::to_raw_value(&listen).unwrap();
    let proof = proof::prove(&bob, &node_did(&vault), body.get().as_bytes(), now_ms());
    let mut conn = UnixStream::connect(&socket).unwrap();
    let forged = Envelope {
        body,
        proof: Some(proof),
    };
    write_msg(&mut conn, &serde_json::to_vec(&forged).unwrap()).unwrap();
    let ack: Response = serde_json::from_slice(&read_msg(&mut conn).unwrap()).unwrap();
    assert!(is_err_containing(&ack, "signature"), "{ack:?}");
}
