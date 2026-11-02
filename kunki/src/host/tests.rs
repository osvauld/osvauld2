//! The acceptor without a connection (laptop-host step 7c): admission — what the transport
//! proved (`remote`) against what the `Hello` claims, T16's host half, the token judged at the
//! door — the session table's relay rules, a host session re-judged before each broadcast,
//! the pusher over it, and which appointed hosts get listed.

use courier::invite::{InviteRequest, InviteTicket, desktop_start_invite_claim};
use courier::token::{Scope, issue_root};
use identity::Identity;
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedReceiver;

use super::*;
use crate::admin::Cause;
use crate::node;

const NOW: u64 = 1_700_000_000;
const DEVICE: [u8; 32] = [9u8; 32];

/// Alice hosts `team` for herself and has a share link out for it.
fn host() -> (Vault, TempDir, String, String) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("alice", "pw").unwrap();
    let admin = Admin::new(vault.clone());
    let token = vault
        .with_signer(|me| courier::self_claim(me, NOW))
        .unwrap()
        .unwrap();
    admin.record(&token, Cause::Node, NOW).unwrap();
    let ws = vault.create_workspace("team").unwrap();
    let request = InviteRequest {
        desktop_did: node::did(&vault).unwrap(),
        token,
        role: "member".into(),
        scope: Scope::Workspace(ws.id.clone()),
        public: true,
    };
    let code = admin.issue_code(&request, "alice", NOW).unwrap();
    let (ticket, _) = admin.fetch_code(&code, NOW).unwrap().unwrap();
    (vault, tmp, ws.id, ticket.to_text().unwrap())
}

fn member() -> (Identity, [u8; 32], String) {
    let me = identity::generate().0;
    let attestation = courier::attest::attest(&me, DEVICE, NOW).unwrap();
    (me, DEVICE, attestation)
}

/// A token alice's vault signs `aud` directly: a member's grant at `scope`.
fn token_for(vault: &Vault, aud: &str, scope: Scope, exp: u64) -> Token {
    vault
        .with_signer(|me| issue_root(me, aud, "member", scope, false, NOW, exp))
        .unwrap()
        .unwrap()
}

/// `token` with one bit of its signature flipped: the same claims, nobody's signature.
fn forged(token: &Token) -> Token {
    let mut value = serde_json::to_value(token).unwrap();
    let first = value["sig"][0].as_u64().unwrap();
    value["sig"][0] = serde_json::Value::from(first ^ 1);
    serde_json::from_value(value).unwrap()
}

#[test]
fn a_claim_by_the_attested_account_on_the_proven_device_is_admitted() {
    let (vault, _tmp, ws, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, device, attestation) = member();
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let admitted = admit(&admin, &ws, device, &attestation, None, Some(claim), NOW).unwrap();
    assert_eq!(admitted.did, bob.did());
    assert!(admitted.minted.is_some());
    // The token it minted admits the next session on its own.
    let again = admit(&admin, &ws, device, &attestation, Some(admitted.token), None, NOW).unwrap();
    assert!(again.minted.is_none());
}

#[test]
fn an_attestation_for_another_device_is_refused() {
    let (vault, _tmp, ws, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, _device, attestation) = member();
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let other = [7u8; 32];
    let err = admit(&admin, &ws, other, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another device"), "{err}");
}

#[test]
fn a_claim_by_someone_else_than_the_attested_account_is_refused() {
    let (vault, _tmp, ws, ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    let carol = identity::generate().0;
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &carol, NOW).unwrap();
    let err = admit(&admin, &ws, device, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another account"), "{err}");
}

#[test]
fn garbage_and_an_empty_hello_are_refused() {
    let (vault, _tmp, ws, _ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    assert!(admit(&admin, &ws, device, "nonsense", None, None, NOW).is_err());
    let err = admit(&admin, &ws, device, &attestation, None, None, NOW).unwrap_err();
    assert!(err.contains("no token and no claim"), "{err}");
}

#[test]
fn a_presented_token_is_verified_at_the_door() {
    let (vault, _tmp, ws, _ticket) = host();
    let admin = Admin::new(vault.clone());
    let (bob, device, attestation) = member();
    let space = Scope::Workspace(ws.clone());
    let good = token_for(&vault, bob.did(), space.clone(), NOW + 10);
    let present = |token: Token, now: u64| {
        admit(&admin, &ws, device, &attestation, Some(token), None, now).map(|_| ())
    };
    present(good.clone(), NOW).unwrap();

    let stranger = identity::generate().0;
    let refusals = [
        (forged(&good), NOW, "signature invalid"),
        (good.clone(), NOW + 10, "token expired"),
        (
            issue_root(&stranger, bob.did(), "member", space.clone(), false, NOW, NOW + 10)
                .unwrap(),
            NOW,
            "identity did not match",
        ),
        (
            token_for(&vault, identity::generate().0.did(), space, NOW + 10),
            NOW,
            "issued to someone else",
        ),
    ];
    for (token, now, why) in refusals {
        let err = present(token, now).unwrap_err();
        assert!(err.contains(why), "{err}");
    }

    // Revoked by the owner: the door reads the set the docs are judged on.
    admin.record(&good, Cause::Node, NOW).unwrap();
    admin.revoke(&good.id(), NOW).unwrap();
    let err = present(good, NOW).unwrap_err();
    assert!(err.contains("token revoked"), "{err}");
}

#[test]
fn a_session_is_on_a_space_its_grant_reaches() {
    let (vault, _tmp, ws, ticket) = host();
    let admin = Admin::new(vault.clone());
    let (bob, device, attestation) = member();
    let other = "other-space";
    let on = |scope: Scope, space: &str| {
        let token = token_for(&vault, bob.did(), scope, NOW + 10);
        admit(&admin, space, device, &attestation, Some(token), None, NOW).map(|_| ())
    };
    let app = Scope::App {
        ws: ws.clone(),
        app: "item".to_string(),
    };
    on(app.clone(), &ws).unwrap();
    on(Scope::Node, &ws).unwrap();
    let err = on(Scope::Workspace(other.to_string()), &ws).unwrap_err();
    assert!(err.contains("another space"), "{err}");
    let err = on(app, other).unwrap_err();
    assert!(err.contains("another space"), "{err}");

    // A claim passes the same door: the link's space and the hello's must agree.
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let err = admit(&admin, other, device, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another space"), "{err}");
}

#[test]
fn a_host_is_listed_once_it_has_said_where_it_is_and_while_it_holds_host() {
    let (vault, _tmp, ws, _ticket) = host();
    let admin = Admin::new(vault);
    let bob = identity::generate().0;
    let addr = HostAddr {
        device: [3u8; 32],
        relay: None,
        addrs: vec!["127.0.0.1:4433".parse().unwrap()],
    };
    let listed = || appointed_hosts(&admin, &ws, NOW).unwrap();
    assert!(listed().is_empty());
    let token = admin.appoint_host(&ws, bob.did(), NOW).unwrap();
    assert!(listed().is_empty(), "appointed, but nowhere to dial yet");
    admin.set_host_addr(&ws, bob.did(), &addr).unwrap();
    assert_eq!(
        listed(),
        vec![HostEntry {
            did: bob.did().to_string(),
            addr: addr.clone()
        }]
    );
    admin.revoke(&token.id(), NOW).unwrap();
    assert!(listed().is_empty(), "revoked");
    assert_eq!(
        admin.host_addr(&ws, bob.did()).unwrap(),
        Some(addr),
        "the record outlives the grant; the listing is what checks it"
    );
}

/// A session nothing re-judges: the relay rules alone.
fn live(sid: u64, did: &str, host: bool) -> (Live, UnboundedReceiver<Frame>) {
    let (tx, rx) = unbounded_channel();
    let live = Live {
        sid,
        did: did.to_string(),
        tx,
        host,
        judge: None,
    };
    (live, rx)
}

fn frame() -> Frame {
    Frame::Hosts { hosts: Vec::new() }
}

/// How many frames arrived since last asked.
fn got(rx: &mut UnboundedReceiver<Frame>) -> usize {
    let mut n = 0;
    while rx.try_recv().is_ok() {
        n += 1;
    }
    n
}

#[test]
fn broadcast_reaches_host_sessions_only_and_skips_the_origin() {
    let table = Sessions::default();
    let (a, mut a_rx) = live(1, "a", true);
    let (b, mut b_rx) = live(2, "b", false);
    let (c, mut c_rx) = live(3, "c", true);
    for l in [a, b, c] {
        table.add("ws", l);
    }
    table.broadcast("ws", &frame(), Some(1));
    assert_eq!(got(&mut a_rx), 0, "the session it came in on");
    assert_eq!(got(&mut b_rx), 0, "a member session");
    assert_eq!(got(&mut c_rx), 1);
    table.broadcast("other", &frame(), None);
    assert_eq!(got(&mut c_rx), 0, "another space");
    table.send_all("ws", &frame());
    assert_eq!(got(&mut a_rx) + got(&mut b_rx) + got(&mut c_rx), 3);
}

#[test]
fn promote_flips_every_session_of_the_did() {
    let table = Sessions::default();
    let (a1, mut a1_rx) = live(1, "a", false);
    let (a2, mut a2_rx) = live(2, "a", false);
    let (b, mut b_rx) = live(3, "b", false);
    for l in [a1, a2, b] {
        table.add("ws", l);
    }
    assert!(!table.has_host("a", "ws"));
    assert!(table.promote("a", "elsewhere").is_empty());
    assert_eq!(table.promote("a", "ws").len(), 2);
    assert!(table.has_host("a", "ws"));
    assert!(!table.has_host("b", "ws"));
    table.broadcast("ws", &frame(), None);
    assert_eq!(got(&mut a1_rx), 1);
    assert_eq!(got(&mut a2_rx), 1);
    assert_eq!(got(&mut b_rx), 0);
    // Its sessions are a host's now: `send` leaves them to `broadcast`.
    table.send("a", "ws", &frame(), None);
    assert_eq!(got(&mut a1_rx) + got(&mut a2_rx), 0);
    table.remove("ws", 1);
    table.remove("ws", 2);
    assert!(!table.has_host("a", "ws"));
}

#[test]
fn an_uplink_is_a_host_session_until_unregistered() {
    let ws = "uplink";
    let (tx, mut rx) = unbounded_channel();
    let sid = sessions().register_uplink(ws, "owner", tx);
    assert!(sessions().has_host("owner", ws));
    sessions().broadcast(ws, &frame(), Some(sid));
    assert_eq!(got(&mut rx), 0);
    sessions().broadcast(ws, &frame(), None);
    assert_eq!(got(&mut rx), 1);
    sessions().unregister(ws, sid);
    assert!(!sessions().has_host("owner", ws));
}

#[derive(Clone)]
struct Stub;

impl Sink for Stub {
    fn admin_for(&self, _: &str) -> Option<Admin> {
        None
    }
    fn fetch_admin(&self) -> Option<Admin> {
        None
    }
    fn now_secs(&self) -> u64 {
        NOW
    }
    fn now_ms(&self) -> u64 {
        NOW * 1000
    }
    fn sync(&self, _: SyncHello) -> SyncFuture {
        Box::pin(async { Err("stub".to_string()) })
    }
    fn local_push(&self, _: &str, _: &Push) {}
    fn hosts(&self, _: &str) -> Vec<HostEntry> {
        Vec::new()
    }
}

#[test]
fn a_host_session_is_demoted_once_its_appointment_ends_but_an_uplink_never() {
    let (vault, _tmp, _ws, _ticket) = host();
    let admin = Admin::new(vault);
    // The table is the process's: this test's space is its own.
    let ws = "demoted-host";
    let bob = identity::generate().0;
    let appointment = admin.appoint_host(ws, bob.did(), NOW).unwrap();
    let (tx, mut bob_rx) = unbounded_channel();
    sessions().add(
        ws,
        Live {
            sid: 201,
            did: bob.did().to_string(),
            tx,
            host: true,
            judge: Some(judge_of(&Stub, &admin, bob.did(), ws)),
        },
    );
    let (tx, mut up_rx) = unbounded_channel();
    let up = sessions().register_uplink(ws, "owner", tx);
    ledger_changed(&admin, ws).unwrap();
    assert_eq!((got(&mut bob_rx), got(&mut up_rx)), (1, 1));

    admin.revoke(&appointment.id(), NOW).unwrap();
    ledger_changed(&admin, ws).unwrap();
    assert_eq!(got(&mut bob_rx), 0, "revoked: nothing more, the ledger included");
    assert_eq!(got(&mut up_rx), 1, "the owner's own session is never re-judged");
    assert!(!sessions().has_host(bob.did(), ws), "offline over the bridge");
    sessions().broadcast(ws, &frame(), None);
    assert_eq!(got(&mut bob_rx), 0, "a member session again");

    admin.appoint_host(ws, bob.did(), NOW).unwrap();
    sessions().promote(bob.did(), ws);
    sessions().broadcast(ws, &frame(), None);
    assert_eq!(got(&mut bob_rx), 1, "re-appointed, re-promoted");
    sessions().unregister(ws, 201);
    sessions().unregister(ws, up);
}

#[test]
fn the_pusher_broadcasts_to_host_sessions_only() {
    // The table is the process's: this test's space is its own.
    let ws = "pusher-broadcast";
    let (h, mut h_rx) = live(101, "h", true);
    let (m, mut m_rx) = live(102, "m", false);
    let (origin, mut origin_rx) = live(103, "o", true);
    for l in [h, m, origin] {
        sessions().add(ws, l);
    }
    let pusher = HostPusher::new(Stub);
    let update = Push {
        ws_id: ws.to_string(),
        item_id: "item".to_string(),
        layer: SyncLayer::Src,
        snapshot: vec![1],
        author: "o".to_string(),
        origin: "103/1".to_string(),
    };
    pusher.broadcast(&update);
    assert_eq!(got(&mut h_rx), 1);
    assert_eq!(got(&mut m_rx), 0, "not a host");
    assert_eq!(got(&mut origin_rx), 0, "the session it came in on");
    // `push` is the subscriber path: a member session hears it, a host's waits for broadcast.
    pusher.push("m", &update);
    pusher.push("h", &update);
    assert_eq!(got(&mut m_rx), 1);
    assert_eq!(got(&mut h_rx), 0);
    for sid in [101, 102, 103] {
        sessions().unregister(ws, sid);
    }
}
