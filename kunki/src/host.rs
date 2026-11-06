//! The host half of a space session on this process's endpoint (laptop-host §2 Sessions,
//! "Appointed hosts"). Accepts connections, answers `Fetch` for a share code, and runs one
//! session per (member, space). The owner's shell and the headless host run this same code;
//! what differs between them — how an update is merged, whose clock, which admin judges a
//! space, which hosts to advertise, the in-process listener to poke — is the [`Sink`] the
//! process supplies. Accepted updates fan out to the other sessions on that space through
//! [`HostPusher`]. A session whose DID holds `host` on the space is a host session: it gets
//! the ledger and a full copy on entry and every accepted update after, subscribed or not.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use courier::invite::InviteClaimHello;
use courier::link::HostAddr;
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::session::{DocState, Frame, HostEntry};
use courier::subscribe::SubscribeHello;
use courier::sync::{SyncHello, SyncLayer, SyncReply};
use courier::token::{Scope, Token};
use iroh::endpoint::{Connection, SendStream};
use iroh::{Endpoint, EndpointAddr};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use net::guard::{Codes, Verdict};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vault::Vault;

use crate::NodeError;
use crate::admin::Admin;
use crate::push::{Push, Pusher};

/// One merge, answered wherever the process keeps the doc.
pub type SyncFuture = Pin<Box<dyn Future<Output = Result<SyncReply, String>> + Send>>;

/// What the acceptor needs of its process. Called from the net runtime's threads.
pub trait Sink: Clone + Send + Sync + 'static {
    /// The admin that judges `ws_id`; `None` refuses the `Hello` — not a space this process
    /// hosts. The shell: its own records for a space it owns; the headless host: the
    /// owner's, replayed (`Admin::hosted`).
    fn admin_for(&self, ws_id: &str) -> Option<Admin>;
    /// Who answers `Fetch { code }`; `None` when links are the owner's to answer.
    fn fetch_admin(&self) -> Option<Admin>;
    fn now_secs(&self) -> u64;
    fn now_ms(&self) -> u64;
    /// Merge one update: into the live doc on the shell's UI thread, into the vault inline
    /// on the headless host. `Err` is a refusal the member hears. `device` is the key the
    /// connection proved the member holds, which the merge binds its changes to.
    fn sync(&self, hello: SyncHello, device: [u8; 32]) -> SyncFuture;
    /// The in-process listener, if the process has one.
    fn local_push(&self, subscriber_did: &str, update: &Push);
    /// Every host of `ws_id` to advertise, owner first.
    fn hosts(&self, ws_id: &str) -> Vec<HostEntry>;
}

/// What a serving process hands the acceptor, cloned into every stream.
#[derive(Clone)]
pub struct HostCtx<S: Sink> {
    pub vault: Vault,
    pub ep: Endpoint,
    pub sink: S,
}

/// Whether a session's DID still holds `host` on its space, as the admin that admitted it
/// reads it on the process's clock. Asked before every broadcast, since an appointment ends
/// mid-session; `None` on the uplink, the owner's own session, a host by the link that named
/// it. Carried by the session rather than passed by the caller because [`ledger_changed`]
/// has an admin and no clock.
type Judge = Arc<dyn Fn() -> bool + Send + Sync>;

/// One live session's end, by id: the relay target. `host` marks a session whose DID holds
/// `host` on the space — it hears every accepted update, not only the docs it subscribed.
#[derive(Clone)]
struct Live {
    sid: u64,
    did: String,
    tx: UnboundedSender<Frame>,
    host: bool,
    judge: Option<Judge>,
}

/// The re-check a session carries: `did` still holds `host` on `ws` by `admin`, now. A
/// record that cannot be read demotes rather than vouches.
fn judge_of<S: Sink>(sink: &S, admin: &Admin, did: &str, ws: &str) -> Judge {
    let (sink, admin) = (sink.clone(), admin.clone());
    let (did, ws) = (did.to_string(), ws.to_string());
    Arc::new(move || admin.is_host(&did, &ws, sink.now_secs()).unwrap_or(false))
}

/// Live sessions by space, for relaying an accepted update to everyone else on it. One
/// member may hold several (a phone and a laptop), each its own session.
#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<HashMap<String, Vec<Live>>>>);

impl Sessions {
    fn add(&self, ws: &str, live: Live) {
        self.0
            .lock()
            .unwrap()
            .entry(ws.to_string())
            .or_default()
            .push(live);
    }

    fn remove(&self, ws: &str, sid: u64) {
        let mut all = self.0.lock().unwrap();
        if let Some(list) = all.get_mut(ws) {
            list.retain(|l| l.sid != sid);
            if list.is_empty() {
                all.remove(ws);
            }
        }
    }

    /// To every member session of `did` on `ws` but `except` — the one the update came in
    /// on. A host session is left to [`broadcast`](Self::broadcast), which every fan-out
    /// ends with: it would hear each update twice otherwise.
    pub fn send(&self, did: &str, ws: &str, frame: &Frame, except: Option<u64>) {
        if let Some(list) = self.0.lock().unwrap().get(ws) {
            for live in list
                .iter()
                .filter(|l| l.did == did && !l.host && Some(l.sid) != except)
            {
                let _ = live.tx.send(frame.clone());
            }
        }
    }

    /// To every host session of `ws` but `except_sid`, each re-judged first: one whose
    /// appointment was revoked or ran out is demoted — a member session again, hearing
    /// nothing more until re-promoted. Judged outside the lock: a judge reads the vault.
    pub fn broadcast(&self, ws: &str, frame: &Frame, except_sid: Option<u64>) {
        let hosts: Vec<(u64, UnboundedSender<Frame>, Option<Judge>)> = {
            let all = self.0.lock().unwrap();
            let Some(list) = all.get(ws) else {
                return;
            };
            list.iter()
                .filter(|l| l.host && Some(l.sid) != except_sid)
                .map(|l| (l.sid, l.tx.clone(), l.judge.clone()))
                .collect()
        };
        let mut demoted = Vec::new();
        for (sid, tx, judge) in hosts {
            if judge.is_some_and(|still_host| !still_host()) {
                demoted.push(sid);
            } else {
                let _ = tx.send(frame.clone());
            }
        }
        if !demoted.is_empty() {
            self.demote(ws, &demoted);
        }
    }

    fn demote(&self, ws: &str, sids: &[u64]) {
        if let Some(list) = self.0.lock().unwrap().get_mut(ws) {
            for live in list.iter_mut().filter(|l| sids.contains(&l.sid)) {
                live.host = false;
            }
        }
    }

    /// To every session of `ws`: a `Hosts` announcement.
    pub fn send_all(&self, ws: &str, frame: &Frame) {
        if let Some(list) = self.0.lock().unwrap().get(ws) {
            for live in list {
                let _ = live.tx.send(frame.clone());
            }
        }
    }

    /// Marks every session of `did` on `ws` a host session and returns their senders, for
    /// the ledger and the copy a new host is owed.
    pub fn promote(&self, did: &str, ws: &str) -> Vec<UnboundedSender<Frame>> {
        let mut all = self.0.lock().unwrap();
        let Some(list) = all.get_mut(ws) else {
            return Vec::new();
        };
        list.iter_mut()
            .filter(|l| l.did == did)
            .map(|l| {
                l.host = true;
                l.tx.clone()
            })
            .collect()
    }

    /// Whether `did` has a live host session on `ws` — the `online` of `Hosts` over the bridge.
    pub fn has_host(&self, did: &str, ws: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(ws)
            .is_some_and(|list| list.iter().any(|l| l.did == did && l.host))
    }

    fn any_host(&self, ws: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(ws)
            .is_some_and(|list| list.iter().any(|l| l.host))
    }

    /// A headless host's own session to the owner, as a host session in its table: what it
    /// accepts from members goes up through the same [`broadcast`](Self::broadcast). Never
    /// re-judged — the owner is the root, not a holder of `host`. Returns the session id,
    /// which the uplink's merges name as their origin ([`session_of`]).
    pub fn register_uplink(&self, ws: &str, did: &str, tx: UnboundedSender<Frame>) -> u64 {
        let sid = SESSION_SEQ.fetch_add(1, Ordering::Relaxed);
        self.add(
            ws,
            Live {
                sid,
                did: did.to_string(),
                tx,
                host: true,
                judge: None,
            },
        );
        sid
    }

    pub fn unregister(&self, ws: &str, sid: u64) {
        self.remove(ws, sid);
    }
}

/// One table per process, as the host is.
pub fn sessions() -> &'static Sessions {
    static SESSIONS: OnceLock<Sessions> = OnceLock::new();
    SESSIONS.get_or_init(Sessions::default)
}

/// The origin an owner's own fan-out carries: nothing to skip but the owner's own listener.
pub const HERE: &str = "here";

/// The session a request id names: `<session>/<n>`, or a bare session id.
pub fn session_of(request_id: &str) -> Option<u64> {
    request_id.split('/').next()?.parse().ok()
}

/// Where an accepted update goes: every member session subscribed on the space except the
/// one it came in on, the process's own listener, and — once per fan-out — every host
/// session but the one it arrived on.
pub struct HostPusher<S: Sink> {
    sink: S,
}

impl<S: Sink> HostPusher<S> {
    pub fn new(sink: S) -> Self {
        Self { sink }
    }
}

fn update_frame(update: &Push) -> Frame {
    Frame::Update {
        item_id: update.item_id.clone(),
        layer: update.layer.clone(),
        vv: Vec::new(),
        update: update.snapshot.clone(),
    }
}

impl<S: Sink> Pusher for HostPusher<S> {
    fn push(&self, subscriber_did: &str, update: &Push) {
        self.sink.local_push(subscriber_did, update);
        let except = session_of(&update.origin).filter(|_| subscriber_did == update.author);
        sessions().send(subscriber_did, &update.ws_id, &update_frame(update), except);
    }

    fn excludes_author(&self) -> bool {
        false
    }

    fn broadcast(&self, update: &Push) {
        sessions().broadcast(&update.ws_id, &update_frame(update), session_of(&update.origin));
    }
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

/// The appointed hosts of `ws` that have said where they are: a DID holding `host` with no
/// `HostHere` yet is a host nobody can dial, so it is not listed.
pub fn appointed_hosts(admin: &Admin, ws: &str, now: u64) -> Result<Vec<HostEntry>, NodeError> {
    let mut out = Vec::new();
    for did in admin.hosts_of(ws, now)? {
        if let Some(addr) = admin.host_addr(ws, &did)? {
            out.push(HostEntry { did, addr });
        }
    }
    Ok(out)
}

/// Accepts on the endpoint until it closes; every stream runs on the net runtime.
pub fn serve<S: Sink>(net: &net::Net, ctx: HostCtx<S>) {
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

async fn serve_conn<S: Sink>(conn: Connection, ctx: HostCtx<S>, codes: Arc<Mutex<Codes>>) {
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
                } => match ctx.sink.admin_for(&ws_id) {
                    None => refuse(send, "refused: not hosting this space").await,
                    Some(admin) => {
                        let now = ctx.sink.now_secs();
                        match admit(&admin, &ws_id, remote, &attestation, token, claim, now) {
                            Ok(admitted) => {
                                session(&ctx, admin, admitted, ws_id, docs, send, frames).await
                            }
                            Err(reason) => refuse(send, &reason).await,
                        }
                    }
                },
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
async fn fetch<S: Sink>(
    ctx: &HostCtx<S>,
    codes: &Arc<Mutex<Codes>>,
    conn_id: u64,
    conn: &Connection,
    code: [u8; courier::link::CODE_LEN],
    mut send: SendStream,
) -> Result<(), String> {
    let Some(admin) = ctx.sink.fetch_admin() else {
        return refuse(send, "refused: links are answered by the space's owner").await;
    };
    if !codes.lock().unwrap().allowed(ctx.sink.now_ms()) {
        return refuse(send, "refused: too many wrong codes, try later").await;
    }
    let found = admin
        .fetch_code(&code, ctx.sink.now_secs())
        .map_err(|e| e.to_string())?;
    let Some((invite, ws_id)) = found else {
        let verdict = codes.lock().unwrap().wrong(conn_id, ctx.sink.now_ms());
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
    let fetched = Frame::Fetched {
        invite,
        hosts: ctx.sink.hosts(&ws_id),
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
    /// The device key the connection proved and the attestation named.
    pub device: [u8; 32],
}

/// The whole of a session's admission, so it can be tested without a connection: the
/// transport proved `remote` holds the device key; the attestation must be that device's and
/// name the account; then a token held — verified here, at the door, not on the first doc —
/// or an invite claimed by that same account; and the grant must reach `ws_id`.
pub fn admit(
    admin: &Admin,
    ws_id: &str,
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
    let records = |e: NodeError| format!("refused: {e}");
    let (token, minted) = match (token, claim) {
        (Some(token), _) => {
            let root = admin.root().map_err(records)?;
            let revoked = admin.revoked().map_err(records)?;
            courier::token::verify_chain(&token, &root, &did, now, &revoked)
                .map_err(|e| format!("refused: token: {e}"))?;
            (token, None)
        }
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
    // A session is on one space. A grant on it, on an app in it or on the whole node is a
    // member's here; one on another space is a stranger's, whoever signed it.
    let scope = token.claims().map_err(|e| format!("refused: {e}"))?.scope;
    let space = Scope::Workspace(ws_id.to_string());
    if !(space.contains(&scope) || scope.contains(&space)) {
        return Err("refused: the token is for another space".into());
    }
    Ok(Admitted {
        did,
        token,
        minted,
        device: remote,
    })
}

/// The whole ledger of `ws`, for a host session.
fn ledger_frame(admin: &Admin, ws: &str) -> Result<Frame, String> {
    let entries = admin.signed_ledger(ws).map_err(|e| e.to_string())?;
    Ok(Frame::Ledger { entries })
}

/// A full copy of `ws` straight from the vault: one `Update` per item source and per doc.
/// A doc the owner has open is one flush behind at most; the contract accepts that.
fn copy_frames(vault: &Vault, ws: &str) -> Result<Vec<Frame>, String> {
    let err = |e: vault::VaultError| e.to_string();
    let mut out = Vec::new();
    for item in vault.items(ws).map_err(err)? {
        if let Some(src) = vault.get_src(ws, &item.id).map_err(err)? {
            out.push(Frame::Update {
                item_id: item.id.clone(),
                layer: SyncLayer::Src,
                vv: Vec::new(),
                update: src,
            });
        }
        doc_frames_of(vault, ws, &item.id, &mut out)?;
    }
    Ok(out)
}

/// The docs of `ws` alone, each with the version vector of its stored snapshot — what an
/// appointed host sends up its uplink: only the owner installs a source, so a host's copy of
/// one is never ahead and the owner would refuse it anyway.
pub fn doc_frames(vault: &Vault, ws: &str) -> Result<Vec<Frame>, String> {
    let mut out = Vec::new();
    for item in vault.items(ws).map_err(|e| e.to_string())? {
        doc_frames_of(vault, ws, &item.id, &mut out)?;
    }
    Ok(out)
}

fn doc_frames_of(
    vault: &Vault,
    ws: &str,
    item_id: &str,
    out: &mut Vec<Frame>,
) -> Result<(), String> {
    let err = |e: vault::VaultError| e.to_string();
    for name in vault.doc_names(ws, item_id).map_err(err)? {
        let Some(bytes) = vault.get_doc(ws, item_id, &name).map_err(err)? else {
            continue;
        };
        let doc = loro::LoroDoc::new();
        doc.import(&bytes)
            .map_err(|e| format!("doc {item_id}/{name}: {e}"))?;
        out.push(Frame::Update {
            item_id: item_id.to_string(),
            layer: SyncLayer::Doc(name),
            vv: doc.oplog_vv().encode(),
            update: bytes,
        });
    }
    Ok(())
}

/// The ledger of `ws` changed — a member joined, a role moved, a host was appointed: every
/// host session of it gets the whole ledger again. Nothing to sign when no host is listening.
pub fn ledger_changed(admin: &Admin, ws: &str) -> Result<(), String> {
    if !sessions().any_host(ws) {
        return Ok(());
    }
    sessions().broadcast(ws, &ledger_frame(admin, ws)?, None);
    Ok(())
}

/// `did` was appointed a host of `ws` while its sessions were up: they become host sessions
/// and get what a host is owed on entry — the ledger (which every other host gets too, the
/// appointment being a ledger fact) and the full copy — then everyone hears the hosts.
pub fn appointed<S: Sink>(ctx: &HostCtx<S>, ws: &str, did: &str) -> Result<(), String> {
    let admin = ctx.sink.admin_for(ws).ok_or("not hosting this space")?;
    let promoted = sessions().promote(did, ws);
    ledger_changed(&admin, ws)?;
    if !promoted.is_empty() {
        let copy = copy_frames(&ctx.vault, ws)?;
        for tx in &promoted {
            for frame in &copy {
                let _ = tx.send(frame.clone());
            }
        }
    }
    sessions().send_all(
        ws,
        &Frame::Hosts {
            hosts: ctx.sink.hosts(ws),
        },
    );
    Ok(())
}

/// An admitted member's session: updates both ways until either side closes.
async fn session<S: Sink>(
    ctx: &HostCtx<S>,
    admin: Admin,
    admitted: Admitted,
    ws_id: String,
    docs: Vec<DocState>,
    mut send: SendStream,
    mut frames: FrameBuf,
) -> Result<(), String> {
    let Admitted {
        did,
        token,
        minted,
        device,
    } = admitted;
    let now = ctx.sink.now_secs();
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
    let host = admin
        .is_host(&did, &ws_id, now)
        .map_err(|e| e.to_string())?;
    let sid = SESSION_SEQ.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = unbounded_channel::<Frame>();
    sessions().add(
        &ws_id,
        Live {
            sid,
            did: did.clone(),
            tx,
            host,
            judge: Some(judge_of(&ctx.sink, &admin, &did, &ws_id)),
        },
    );
    let mut seq = 0u64;
    let result = async {
        let mut intro = vec![
            Frame::Welcome {
                token: minted.clone(),
                docs: Vec::new(),
            },
            Frame::Hosts {
                hosts: ctx.sink.hosts(&ws_id),
            },
        ];
        if host {
            intro.push(ledger_frame(&admin, &ws_id)?);
            intro.extend(copy_frames(&ctx.vault, &ws_id)?);
        }
        for frame in &intro {
            send_frame(&mut send, frame, MAX_FRAME).await.map_err(|e| e.to_string())?;
        }
        // A new member is a ledger fact.
        if minted.is_some() {
            ledger_changed(&admin, &ws_id)?;
        }
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
                                match ctx.sink.sync(hello, device).await {
                                    // The update first, always — a member pulling a doc before
                                    // it seeds waits for exactly one `Update`, empty when there
                                    // is nothing — then the ack that moves its `since`.
                                    Ok(SyncReply::Ack(ack)) => vec![
                                        Frame::Update { item_id: item_id.clone(), layer: layer.clone(), vv: ack.vv.clone(), update: ack.update },
                                        Frame::Ack { item_id, layer, vv: ack.vv, missing: ack.missing },
                                    ],
                                    Ok(SyncReply::Rejected(rejected)) => vec![Frame::Rejected { item_id, layer, rejected }],
                                    Err(reason) => vec![
                                        Frame::Update { item_id, layer, vv: Vec::new(), update: Vec::new() },
                                        Frame::Refused { reason },
                                    ],
                                }
                            }
                        };
                        for frame in out {
                            send_frame(&mut send, &frame, MAX_FRAME).await.map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(Some(Frame::Role { item_id, to, role, revoke })) => {
                        let now = ctx.sink.now_secs();
                        let request = courier::role::RoleRequest {
                            desktop_did: did.clone(),
                            token: token.clone(),
                            to,
                            role,
                            scope: courier::sync::item_scope(&ws_id, &item_id),
                        };
                        let outcome = if revoke {
                            admin.revoke_role(request, now).map(|_| ())
                        } else {
                            admin.assign_role(request, now).map(|_| ())
                        };
                        let done = match outcome {
                            Ok(()) => Frame::RoleDone { ok: true, reason: String::new() },
                            Err(e) => Frame::RoleDone { ok: false, reason: e.to_string() },
                        };
                        send_frame(&mut send, &done, MAX_FRAME).await.map_err(|e| e.to_string())?;
                        if let Frame::RoleDone { ok: true, .. } = done {
                            if let Err(e) = ledger_changed(&admin, &ws_id) {
                                eprintln!("host: ledger of {ws_id}: {e}");
                            }
                        }
                    }
                    // Only a current host is listed; appointment may have come mid-session.
                    Ok(Some(Frame::HostHere { addr })) => {
                        let now = ctx.sink.now_secs();
                        if admin.is_host(&did, &ws_id, now).map_err(|e| e.to_string())? {
                            admin.set_host_addr(&ws_id, &did, &addr).map_err(|e| e.to_string())?;
                            sessions().send_all(&ws_id, &Frame::Hosts { hosts: ctx.sink.hosts(&ws_id) });
                        }
                    }
                    // `Ledger`/`Hosts` are an uplink's to receive, not an acceptor's.
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
    sessions().remove(&ws_id, sid);
    result
}

#[cfg(test)]
mod tests;
