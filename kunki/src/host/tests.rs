//! The acceptor without a connection (laptop-host step 7c): admission — what the transport
//! proved (`remote`) against what the `Hello` claims, T16's host half — the session table's
//! relay rules, the pusher over it, and which appointed hosts get listed.

use courier::invite::{InviteRequest, InviteTicket, desktop_start_invite_claim};
use courier::token::Scope;
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

#[test]
fn a_claim_by_the_attested_account_on_the_proven_device_is_admitted() {
    let (vault, _tmp, _ws, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, device, attestation) = member();
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let admitted = admit(&admin, device, &attestation, None, Some(claim), NOW).unwrap();
    assert_eq!(admitted.did, bob.did());
    assert!(admitted.minted.is_some());
    // The token it minted admits the next session on its own.
    let again = admit(&admin, device, &attestation, Some(admitted.token), None, NOW).unwrap();
    assert!(again.minted.is_none());
}

#[test]
fn an_attestation_for_another_device_is_refused() {
    let (vault, _tmp, _ws, ticket) = host();
    let admin = Admin::new(vault);
    let (bob, _device, attestation) = member();
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &bob, NOW).unwrap();
    let other = [7u8; 32];
    let err = admit(&admin, other, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another device"), "{err}");
}

#[test]
fn a_claim_by_someone_else_than_the_attested_account_is_refused() {
    let (vault, _tmp, _ws, ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    let carol = identity::generate().0;
    let ticket = InviteTicket::from_text(&ticket).unwrap();
    let claim = desktop_start_invite_claim(ticket, &carol, NOW).unwrap();
    let err = admit(&admin, device, &attestation, None, Some(claim), NOW).unwrap_err();
    assert!(err.contains("another account"), "{err}");
}

#[test]
fn garbage_and_an_empty_hello_are_refused() {
    let (vault, _tmp, _ws, _ticket) = host();
    let admin = Admin::new(vault);
    let (_bob, device, attestation) = member();
    assert!(admit(&admin, device, "nonsense", None, None, NOW).is_err());
    let err = admit(&admin, device, &attestation, None, None, NOW).unwrap_err();
    assert!(err.contains("no token and no claim"), "{err}");
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

fn live(sid: u64, did: &str, host: bool) -> (Live, UnboundedReceiver<Frame>) {
    let (tx, rx) = unbounded_channel();
    let live = Live {
        sid,
        did: did.to_string(),
        tx,
        host,
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
