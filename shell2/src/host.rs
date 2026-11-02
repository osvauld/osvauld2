//! The host half of a space session on this device's endpoint (laptop-host §2 Sessions).
//! Accepts connections, answers `Fetch` for a share code, and runs one session per (member,
//! space). Every update a member sends is merged by the shell's UI thread (`Msg::HostSync`):
//! into the live doc when the owner has the app open, into the vault when not — one copy per
//! doc. Accepted updates fan out to the other sessions on that space through [`pusher`].
//!
//! Step 7 lifts the acceptor, [`admit`] and the loop out of the shell for the headless host;
//! what ties them here is `Msg`/the proxy, nothing else.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use courier::invite::InviteClaimHello;
use courier::link::HostAddr;
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::session::{DocState, Frame, HostEntry};
use courier::subscribe::SubscribeHello;
use courier::sync::{SyncHello, SyncLayer, SyncReply};
use courier::token::Token;
use iroh::endpoint::{Connection, SendStream};
use iroh::{Endpoint, EndpointAddr};
use kunki::admin::Admin;
use kunki::push::{Push, Pusher};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use net::guard::{Codes, Verdict};
use runtime::EventLoopProxy;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use vault::Vault;

use crate::{Msg, Slot, clock, node};

/// One live session's end, by id: the relay target.
#[derive(Clone)]
struct Live {
    sid: u64,
    tx: UnboundedSender<Frame>,
}

/// Live sessions by (member did, space), for relaying an accepted update to everyone else on
/// that space. One member may hold several (a phone and a laptop), each its own session.
#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<HashMap<(String, String), Vec<Live>>>>);

impl Sessions {
    fn add(&self, did: &str, ws: &str, live: Live) {
        self.0
            .lock()
            .unwrap()
            .entry((did.to_string(), ws.to_string()))
            .or_default()
            .push(live);
    }

    fn remove(&self, did: &str, ws: &str, sid: u64) {
        let mut all = self.0.lock().unwrap();
        let key = (did.to_string(), ws.to_string());
        if let Some(list) = all.get_mut(&key) {
            list.retain(|l| l.sid != sid);
            if list.is_empty() {
                all.remove(&key);
            }
        }
    }

    /// To every session of `did` on `ws` but `except` — the one the update came in on.
    fn send(&self, did: &str, ws: &str, frame: &Frame, except: Option<u64>) {
        if let Some(list) = self
            .0
            .lock()
            .unwrap()
            .get(&(did.to_string(), ws.to_string()))
        {
            for live in list.iter().filter(|l| Some(l.sid) != except) {
                let _ = live.tx.send(frame.clone());
            }
        }
    }
}

/// One table per process, as the host is.
pub fn sessions() -> &'static Sessions {
    static SESSIONS: OnceLock<Sessions> = OnceLock::new();
    SESSIONS.get_or_init(Sessions::default)
}

/// The origin an owner's own fan-out carries: nothing to skip but the owner's own listener.
pub const HERE: &str = "here";

/// Where an accepted update goes: every member session on the space except the one it came
/// in on, and the owner's own in-process listener (`node::registry`) unless the owner wrote
/// it — its open app merged that already.
pub struct HostPusher;

impl Pusher for HostPusher {
    fn push(&self, subscriber_did: &str, update: &Push) {
        if !(update.origin == HERE && subscriber_did == update.author) {
            node::registry().push(subscriber_did, update);
        }
        let frame = Frame::Update {
            item_id: update.item_id.clone(),
            layer: update.layer.clone(),
            vv: Vec::new(),
            update: update.snapshot.clone(),
        };
        let except = session_of(&update.origin).filter(|_| subscriber_did == update.author);
        sessions().send(subscriber_did, &update.ws_id, &frame, except);
    }

    fn excludes_author(&self) -> bool {
        false
    }
}

/// The session a request id names: `<session>/<n>`.
fn session_of(request_id: &str) -> Option<u64> {
    request_id.split('/').next()?.parse().ok()
}

pub fn pusher() -> &'static HostPusher {
    static PUSHER: HostPusher = HostPusher;
    &PUSHER
}

/// What the acceptor needs of the shell. `gen_` tags every message it sends: a session that
/// outlives a lock talks to nobody.
#[derive(Clone)]
pub struct HostCtx {
    pub vault: Vault,
    pub proxy: EventLoopProxy<Msg>,
    pub gen_: u64,
    pub ep: Endpoint,
}

/// Where this host can be dialled, as its endpoint reports it right now.
pub fn host_addr_of(addr: &EndpointAddr) -> HostAddr {
    // A link is typed: loopback and LAN addresses first, four at most, not every interface.
    let rank = |a: &std::net::SocketAddr| match a.ip() {
        std::net::IpAddr::V4(ip) if ip.is_loopback() => 0,
        std::net::IpAddr::V4(ip) if ip.is_private() => 1,
        std::net::IpAddr::V6(ip) if ip.is_loopback() => 2,
        std::net::IpAddr::V4(_) => 3,
        std::net::IpAddr::V6(_) => 4,
    };
    let mut addrs: Vec<std::net::SocketAddr> = addr.ip_addrs().copied().collect();
    addrs.sort_by_key(rank);
    addrs.truncate(4);
    HostAddr {
        device: *addr.id.as_bytes(),
        relay: addr.relay_urls().next().map(|u| u.to_string()),
        addrs,
    }
}

/// Accepts on the endpoint until it closes; every stream runs on the net runtime.
pub fn serve(net: &net::Net, ctx: HostCtx) {
    let ep = net.endpoint().clone();
    let codes = Arc::new(Mutex::new(Codes::default()));
    net.handle().spawn(async move {
        while let Some(incoming) = ep.accept().await {
            let (ctx, codes) = (ctx.clone(), codes.clone());
            tokio::spawn(async move {
                let conn = match incoming.accept() {
                    Ok(accepting) => match accepting.await {
                        Ok(conn) => conn,
                        Err(e) => return eprintln!("host: handshake: {e}"),
                    },
                    Err(e) => return eprintln!("host: accept: {e}"),
                };
                serve_conn(conn, ctx, codes).await;
            });
        }
    });
}

static CONN_SEQ: AtomicU64 = AtomicU64::new(1);
static SESSION_SEQ: AtomicU64 = AtomicU64::new(1);

async fn serve_conn(conn: Connection, ctx: HostCtx, codes: Arc<Mutex<Codes>>) {
    let remote = *conn.remote_id().as_bytes();
    let conn_id = CONN_SEQ.fetch_add(1, Ordering::Relaxed);
    loop {
        let (send, recv) = match conn.accept_bi().await {
            Ok(streams) => streams,
            Err(_) => break,
        };
        let (ctx, codes, conn) = (ctx.clone(), codes.clone(), conn.clone());
        tokio::spawn(async move {
            let mut frames = FrameBuf::new(recv);
            let first = match frames.recv::<Frame>(FIRST_FRAME).await {
                Ok(Some(frame)) => frame,
                Ok(None) => return,
                Err(e) => return eprintln!("host: first frame: {e}"),
            };
            let result = match first {
                Frame::Fetch { code } => fetch(&ctx, &codes, conn_id, &conn, code, send).await,
                Frame::Hello {
                    attestation,
                    ws_id,
                    token,
                    claim,
                    docs,
                } => {
                    let admin = Admin::new(ctx.vault.clone());
                    match admit(&admin, remote, &attestation, token, claim, clock::now_secs()) {
                        Ok(admitted) => session(&ctx, admin, admitted, ws_id, docs, send, frames).await,
                        Err(reason) => refuse(send, &reason).await,
                    }
                }
                _ => refuse(send, "a stream starts with Hello or Fetch").await,
            };
            if let Err(e) = result {
                eprintln!("host: stream ended: {e}");
            }
        });
    }
    codes.lock().unwrap().forget(conn_id);
}

async fn refuse(mut send: SendStream, reason: &str) -> Result<(), String> {
    send_frame(
        &mut send,
        &Frame::Refused {
            reason: reason.to_string(),
        },
        MAX_FRAME,
    )
    .await
    .map_err(|e| e.to_string())?;
    let _ = send.finish();
    Ok(())
}

/// A share code for its invite and the space's headers — rate-limited by the guard, since
/// this is the one thing a stranger may ask.
async fn fetch(
    ctx: &HostCtx,
    codes: &Arc<Mutex<Codes>>,
    conn_id: u64,
    conn: &Connection,
    code: [u8; courier::link::CODE_LEN],
    mut send: SendStream,
) -> Result<(), String> {
    if !codes.lock().unwrap().allowed(clock::now_ms()) {
        return refuse(send, "refused: too many wrong codes, try later").await;
    }
    let admin = Admin::new(ctx.vault.clone());
    let found = admin
        .fetch_code(&code, clock::now_secs())
        .map_err(|e| e.to_string())?;
    let Some((invite, ws_id)) = found else {
        let verdict = codes.lock().unwrap().wrong(conn_id, clock::now_ms());
        return match verdict {
            Verdict::Delay(pause) => {
                tokio::time::sleep(pause).await;
                refuse(send, "refused: no such link, or expired").await
            }
            Verdict::Close => {
                conn.close(1u32.into(), b"too many wrong codes");
                Ok(())
            }
        };
    };
    let ws = ctx
        .vault
        .workspaces()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|w| w.id == ws_id)
        .ok_or("the link's space is gone")?;
    let items = ctx
        .vault
        .items(&ws_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| PublishedItem {
            id: i.id,
            name: i.name,
            kind: i.kind.as_str().to_string(),
            created: i.created,
        })
        .collect();
    let owner = kunki::node::did(&ctx.vault).map_err(|e| e.to_string())?;
    let fetched = Frame::Fetched {
        invite,
        hosts: vec![HostEntry {
            did: owner,
            addr: host_addr_of(&ctx.ep.addr()),
        }],
        ws: PublishedWorkspace {
            id: ws.id,
            name: ws.name,
            created: ws.created,
        },
        items,
    };
    send_frame(&mut send, &fetched, MAX_FRAME)
        .await
        .map_err(|e| e.to_string())?;
    let _ = send.finish();
    Ok(())
}

/// Who a `Hello` admitted, and with what.
#[derive(Debug)]
pub struct Admitted {
    pub did: String,
    pub token: Token,
    /// Minted now for a claim, to be handed back in `Welcome`.
    pub minted: Option<Token>,
}

/// The whole of a session's admission, so it can be tested without a connection: the
/// transport proved `remote` holds the device key; the attestation must be that device's and
/// name the account; then a token held, or an invite claimed by that same account.
pub fn admit(
    admin: &Admin,
    remote: [u8; 32],
    attestation: &str,
    token: Option<Token>,
    claim: Option<InviteClaimHello>,
    now: u64,
) -> Result<Admitted, String> {
    let did = match courier::attest::verify_attestation(attestation) {
        Ok(a) if a.device == remote => a.did,
        Ok(_) => return Err("refused: the attestation names another device".into()),
        Err(e) => return Err(format!("refused: attestation: {e}")),
    };
    let (token, minted) = match (token, claim) {
        (Some(token), _) => (token, None),
        (None, Some(claim)) => {
            if claim.desktop_did != did {
                return Err("refused: the claim names another account".into());
            }
            let welcome = admin
                .accept_invite(claim, now)
                .map_err(|e| format!("refused: {e}"))?;
            (welcome.token.clone(), Some(welcome.token))
        }
        (None, None) => return Err("refused: no token and no claim".into()),
    };
    Ok(Admitted { did, token, minted })
}

/// An admitted member's session: updates both ways until either side closes.
async fn session(
    ctx: &HostCtx,
    admin: Admin,
    admitted: Admitted,
    ws_id: String,
    docs: Vec<DocState>,
    mut send: SendStream,
    mut frames: FrameBuf,
) -> Result<(), String> {
    let Admitted { did, token, minted } = admitted;
    let now = clock::now_secs();
    let subscribe = |item_id: &str, layer: &SyncLayer| {
        admin.subscribe(
            &SubscribeHello {
                desktop_did: did.clone(),
                token: token.clone(),
                ws_id: ws_id.clone(),
                item_id: item_id.to_string(),
                layer: layer.clone(),
            },
            now,
        )
    };
    for doc in &docs {
        if let Err(e) = subscribe(&doc.item_id, &doc.layer) {
            return refuse(send, &format!("refused: {e}")).await;
        }
    }
    let sid = SESSION_SEQ.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = unbounded_channel::<Frame>();
    sessions().add(&did, &ws_id, Live { sid, tx });
    let welcome = Frame::Welcome {
        token: minted,
        docs: Vec::new(),
    };
    let mut seq = 0u64;
    let result = async {
        send_frame(&mut send, &welcome, MAX_FRAME)
            .await
            .map_err(|e| e.to_string())?;
        loop {
            tokio::select! {
                incoming = frames.recv::<Frame>(MAX_FRAME) => match incoming {
                    Ok(Some(Frame::Update { item_id, layer, vv, update })) => {
                        let out = match subscribe(&item_id, &layer) {
                            // The empty `Update` first, as for every refusal: a pull waiting
                            // on this doc is answered, not timed out.
                            Err(e) => vec![
                                Frame::Update { item_id, layer, vv: Vec::new(), update: Vec::new() },
                                Frame::Refused { reason: e.to_string() },
                            ],
                            Ok(()) => {
                                seq += 1;
                                let hello = SyncHello {
                                    request_id: format!("{sid}/{seq}"),
                                    desktop_did: did.clone(),
                                    token: token.clone(),
                                    ws_id: ws_id.clone(),
                                    item_id: item_id.clone(),
                                    layer: layer.clone(),
                                    vv,
                                    update,
                                };
                                let (reply_tx, reply_rx) = oneshot::channel();
                                ctx.proxy
                                    .send_event(Msg::HostSync(ctx.gen_, hello, Slot::new(reply_tx)))
                                    .map_err(|_| "shell gone".to_string())?;
                                match reply_rx.await {
                                    // The update first, always — a member pulling a doc before
                                    // it seeds waits for exactly one `Update`, empty when there
                                    // is nothing — then the ack that moves its `since`.
                                    Ok(Ok(SyncReply::Ack(ack))) => vec![
                                        Frame::Update { item_id: item_id.clone(), layer: layer.clone(), vv: ack.vv.clone(), update: ack.update },
                                        Frame::Ack { item_id, layer, vv: ack.vv, missing: ack.missing },
                                    ],
                                    Ok(Ok(SyncReply::Rejected(rejected))) => vec![Frame::Rejected { item_id, layer, rejected }],
                                    Ok(Err(reason)) => vec![
                                        Frame::Update { item_id, layer, vv: Vec::new(), update: Vec::new() },
                                        Frame::Refused { reason },
                                    ],
                                    Err(_) => return Err("reply dropped".to_string()),
                                }
                            }
                        };
                        for frame in out {
                            send_frame(&mut send, &frame, MAX_FRAME).await.map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(Some(Frame::Role { item_id, to, role, revoke })) => {
                        let request = courier::role::RoleRequest {
                            desktop_did: did.clone(),
                            token: token.clone(),
                            to,
                            role,
                            scope: courier::sync::item_scope(&ws_id, &item_id),
                        };
                        let outcome = if revoke {
                            admin.revoke_role(request, clock::now_secs()).map(|_| ())
                        } else {
                            admin.assign_role(request, clock::now_secs()).map(|_| ())
                        };
                        let done = match outcome {
                            Ok(()) => Frame::RoleDone { ok: true, reason: String::new() },
                            Err(e) => Frame::RoleDone { ok: false, reason: e.to_string() },
                        };
                        send_frame(&mut send, &done, MAX_FRAME).await.map_err(|e| e.to_string())?;
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => return Ok(()),
                    Err(e) => return Err(e.to_string()),
                },
                relay = rx.recv() => match relay {
                    Some(frame) => send_frame(&mut send, &frame, MAX_FRAME).await.map_err(|e| e.to_string())?,
                    None => return Ok(()),
                },
            }
        }
    }
    .await;
    sessions().remove(&did, &ws_id, sid);
    result
}

#[cfg(test)]
mod tests;
