//! The headless appointed host — `kunki --data DIR host --join LINK --accept WS` (laptop-host
//! §2 "Appointed hosts"). One `vault` account, unlocked from `OSVAULD_KUNKI_PASSPHRASE` as
//! the node's is, that joins a space by its link as any member does, keeps one session to
//! the owner — the uplink — and serves the space to members on the device endpoint from the
//! owner's replayed ledger (`Admin::hosted`) and the owner's docs. Nothing it decides is its
//! own authority: it signs nothing for the owner, answers no `Fetch` (links are the owner's),
//! refuses a `Hello` on a space it has not joined, and sends up only docs, never a source
//! (only the owner installs an app).
//!
//! stdout carries exactly one line, `did: <this account's DID>`, once the owner's first
//! `Welcome` is in — the harness reads it as the boot wait — and nothing else, ever; the rest
//! goes to stderr behind `kunki: `. The process exits non-zero when the link cannot be
//! fetched or names another space than `--accept`, or when the first claim is refused; once
//! joined it runs until killed, redialling an offline owner forever and sending its docs up
//! again after every `Welcome`. A restart with a token in `joined/<ws>` skips the link: it
//! may have expired, and the token is what admits.

use std::collections::HashMap;
use std::future::Future;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courier::invite::{InviteTicket, desktop_start_invite_claim};
use courier::link::Link;
use courier::session::{Frame, HostEntry};
use courier::sync::SyncHello;
use iroh::Endpoint;
use iroh::endpoint::SendStream;
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use vault::{ItemKind, Vault, WorkspaceItem, WorkspaceMeta};
use zeroize::Zeroizing;

use crate::admin::Admin;
use crate::host::{
    self, HostCtx, HostPusher, Sink, SyncFuture, doc_frames, host_addr_of, sessions,
};
use crate::push::Push;
use crate::session::{Joined, endpoint_addr_of, fetch, load_joined, save_joined};
use crate::{NodeError, node};

pub const USAGE: &str = "usage: kunki [--data DIR] [host --join LINK --accept WS]";

#[derive(Debug, PartialEq, Eq)]
pub struct Args {
    pub data: PathBuf,
    pub command: Command,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// No subcommand: the pre-pivot node — a boot ticket and the socket bridge.
    Node,
    Host {
        link: String,
        accept: String,
    },
}

/// `argv` without the program name; `env_data` is `OSVAULD_KUNKI_DIR`, which `--data` beats.
pub fn parse_args(
    argv: impl IntoIterator<Item = String>,
    env_data: Option<PathBuf>,
) -> Result<Args, String> {
    let mut argv = argv.into_iter();
    let (mut data, mut host, mut link, mut accept) = (None, false, None, None);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--data" => data = Some(PathBuf::from(value("--data", &mut argv)?)),
            "host" if !host => host = true,
            "--join" if host => link = Some(value("--join", &mut argv)?),
            "--accept" if host => accept = Some(value("--accept", &mut argv)?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let data = data.or(env_data).unwrap_or_else(|| PathBuf::from(".kunki"));
    let command = if host {
        Command::Host {
            link: link.ok_or("host needs --join LINK")?,
            accept: accept.ok_or("host needs --accept WS")?,
        }
    } else {
        Command::Node
    };
    Ok(Args { data, command })
}

fn value(flag: &str, argv: &mut impl Iterator<Item = String>) -> Result<String, String> {
    argv.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn log(msg: impl std::fmt::Display) {
    eprintln!("kunki: {msg}");
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}

pub fn now_secs() -> u64 {
    now_ms() / 1000
}

/// Opens the node's account; a first boot says the new identity and its recovery phrase on
/// stderr, once.
pub fn open(data: PathBuf, passphrase: &str) -> Result<Vault, NodeError> {
    let (vault, mnemonic) = node::open(data, passphrase)?;
    if let Some(mnemonic) = mnemonic {
        let phrase = Zeroizing::new(mnemonic.to_string());
        log(format!("created identity {}", node::did(&vault)?));
        eprintln!("kunki: recovery phrase: {}", &*phrase);
    }
    Ok(vault)
}

/// What the headless host supplies the acceptor: the owner's replayed records for each
/// joined space, the system clock, merges inline into the vault, no listener of its own,
/// and the hosts the owner last announced.
#[derive(Clone)]
pub struct HeadlessSink {
    vault: Vault,
    /// The last `Hosts` per space, owner first; seeded from `joined/<ws>` at boot.
    hosts: Arc<Mutex<HashMap<String, Vec<HostEntry>>>>,
    /// One writer per doc, as the shell's UI thread is: the vault's doc copies are
    /// read-modify-write, and two members' merges of one doc on two blocking threads would
    /// each store over the other's.
    merge: Arc<Mutex<()>>,
}

impl HeadlessSink {
    pub fn new(vault: Vault) -> Self {
        let hosts = load_joined(&vault)
            .into_iter()
            .map(|(ws, joined)| (ws, joined.hosts))
            .collect();
        Self {
            vault,
            hosts: Arc::new(Mutex::new(hosts)),
            merge: Arc::default(),
        }
    }

    /// The owner of a joined space: `joined/<ws>`'s `host_did`.
    fn owner_of(&self, ws_id: &str) -> Option<String> {
        load_joined(&self.vault).remove(ws_id).map(|j| j.host_did)
    }

    pub fn set_hosts(&self, ws_id: &str, hosts: Vec<HostEntry>) {
        self.hosts.lock().unwrap().insert(ws_id.to_string(), hosts);
    }
}

/// `f` on a blocking thread, after every merge before it (`HeadlessSink::merge`).
fn serialized<T: Send + 'static>(
    one: Arc<Mutex<()>>,
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = Result<T, String>> + Send {
    async move {
        tokio::task::spawn_blocking(move || {
            let _held = one.lock().unwrap();
            f()
        })
        .await
        .map_err(|e| e.to_string())
    }
}

impl Sink for HeadlessSink {
    fn admin_for(&self, ws_id: &str) -> Option<Admin> {
        Some(Admin::hosted(self.vault.clone(), &self.owner_of(ws_id)?))
    }

    fn fetch_admin(&self) -> Option<Admin> {
        None
    }

    fn now_secs(&self) -> u64 {
        now_secs()
    }

    fn now_ms(&self) -> u64 {
        now_ms()
    }

    fn sync(&self, hello: SyncHello) -> SyncFuture {
        let Some(admin) = self.admin_for(&hello.ws_id) else {
            return Box::pin(async { Err("not hosting this space".to_string()) });
        };
        let pusher = HostPusher::new(self.clone());
        let now = now_secs();
        let merged = serialized(self.merge.clone(), move || {
            admin
                .accept_sync(hello, now, &pusher)
                .map_err(|e| e.to_string())
        });
        Box::pin(async move { merged.await? })
    }

    fn local_push(&self, _subscriber_did: &str, _update: &Push) {}

    fn hosts(&self, ws_id: &str) -> Vec<HostEntry> {
        self.hosts
            .lock()
            .unwrap()
            .get(ws_id)
            .cloned()
            .unwrap_or_default()
    }
}

/// Joins (or resumes) `accept_ws` by `link` and serves it until killed; `Err` is why it
/// could not.
pub fn run(data: PathBuf, passphrase: &str, link: &str, accept_ws: &str) -> Result<(), NodeError> {
    let vault = open(data, passphrase)?;
    let did = node::did(&vault)?;
    let net = net::Net::bind(
        &*vault.device_secret()?,
        net::Mode::from_env(),
        vec![net::SPACE_ALPN.to_vec()],
        None,
    )
    .map_err(|e| NodeError::Net(e.to_string()))?;
    // The direct addresses, before anything is announced: a local-mode `HostHere` without
    // them is a host nobody can dial.
    let _ = net.addr(Duration::from_millis(500));
    let sink = HeadlessSink::new(vault.clone());
    host::serve(
        &net,
        HostCtx {
            vault: vault.clone(),
            ep: net.endpoint().clone(),
            sink: sink.clone(),
        },
    );
    let (joined, ticket) = match load_joined(&vault).remove(accept_ws) {
        Some(joined) if joined.token.is_some() => (joined, None),
        _ => join(&vault, &net, link, accept_ws)?,
    };
    sink.set_hosts(accept_ws, joined.hosts.clone());
    let (fatal, stopped) = mpsc::channel();
    let up = Uplink {
        ep: net.endpoint().clone(),
        vault,
        sink,
        did,
        ws: accept_ws.to_string(),
        fatal,
    };
    let state = State {
        joined,
        ticket,
        announced: false,
    };
    net.handle().spawn(uplink(up, state));
    // The runtime lives in `net`; this thread only holds it, until the uplink says stop.
    Err(stopped
        .recv()
        .unwrap_or_else(|_| NodeError::Net("the uplink stopped".into())))
}

/// What a shell's join does, minus the screens: fetch the link, adopt the space's headers,
/// keep `joined/<ws>`. The ticket comes back for the first session's claim.
fn join(
    vault: &Vault,
    net: &net::Net,
    link: &str,
    accept_ws: &str,
) -> Result<(Joined, Option<InviteTicket>), NodeError> {
    let link = Link::from_text(link)?;
    let fetched = fetch(net.handle(), net.endpoint().clone(), link).map_err(NodeError::Net)?;
    if fetched.ws.id != accept_ws {
        return Err(NodeError::Refused(format!(
            "refused: the link is for space {}, not {accept_ws}",
            fetched.ws.id
        )));
    }
    vault.adopt_workspace(&WorkspaceMeta {
        id: fetched.ws.id.clone(),
        name: fetched.ws.name.clone(),
        created: fetched.ws.created,
    })?;
    for item in &fetched.items {
        let kind =
            ItemKind::parse(&item.kind).ok_or_else(|| NodeError::BadItemKind(item.kind.clone()))?;
        vault.adopt_item(&WorkspaceItem {
            id: item.id.clone(),
            ws_id: fetched.ws.id.clone(),
            name: item.name.clone(),
            kind,
            created: item.created,
        })?;
    }
    let host = fetched
        .hosts
        .first()
        .map(|h| h.addr.clone())
        .unwrap_or_else(|| fetched.link.host.clone());
    let joined = Joined {
        host_did: fetched.invite.node_did.clone(),
        host,
        token: None,
        hosts: fetched.hosts,
    };
    save_joined(vault, accept_ws, &joined)
        .map_err(|e| NodeError::Keep(format!("joined/{accept_ws}: {e}")))?;
    Ok((joined, Some(fetched.invite)))
}

fn keep(vault: &Vault, ws: &str, joined: &Joined) {
    if let Err(e) = save_joined(vault, ws, joined) {
        log(format!("could not keep joined/{ws}: {e}"));
    }
}

/// What the uplink task holds for the life of the process.
struct Uplink {
    ep: Endpoint,
    vault: Vault,
    sink: HeadlessSink,
    did: String,
    ws: String,
    /// Ends the process: the first claim refused, or nothing to sign with.
    fatal: mpsc::Sender<NodeError>,
}

/// What outlives one dial.
struct State {
    joined: Joined,
    /// The invite to claim, until the first `Welcome` mints a token.
    ticket: Option<InviteTicket>,
    /// `did:` has been printed.
    announced: bool,
}

/// Why a dial is not followed by another.
enum Stop {
    Fatal(NodeError),
    Retry(String),
}

/// The session to the owner, redialled forever: an owner offline is normal.
async fn uplink(u: Uplink, mut state: State) {
    let mut attempt = 0;
    loop {
        let pause = match session(&u, &mut state).await {
            Ok(how) => {
                log(format!("session to the owner ended: {how}"));
                attempt = 0;
                net::wire::retry_after(0)
            }
            Err(Stop::Retry(why)) => {
                log(format!("no session to the owner: {why}"));
                let pause = net::wire::retry_after(attempt);
                attempt += 1;
                pause
            }
            Err(Stop::Fatal(err)) => {
                let _ = u.fatal.send(err);
                return;
            }
        };
        tokio::time::sleep(pause).await;
    }
}

/// One dial of the owner. `Ok` is how an established session ended; `Retry` never got one.
async fn session(u: &Uplink, state: &mut State) -> Result<String, Stop> {
    let retry = |e: String| Stop::Retry(e);
    // A host dials only the owner, at whichever address it last announced.
    let owner = state
        .joined
        .hosts
        .iter()
        .find(|h| h.did == state.joined.host_did)
        .map(|h| h.addr.clone())
        .unwrap_or_else(|| state.joined.host.clone());
    let addr = endpoint_addr_of(&owner).map_err(retry)?;
    let conn = net::endpoint::dial(&u.ep, addr, net::SPACE_ALPN, net::wire::DIAL_TIMEOUT)
        .await
        .map_err(|e| retry(e.to_string()))?;
    let (mut send, recv) = conn.open_bi().await.map_err(|e| retry(e.to_string()))?;
    let mut frames = FrameBuf::new(recv);
    let now = now_secs();
    let device = u
        .vault
        .device_public_key()
        .map_err(|e| retry(e.to_string()))?;
    // A fresh claim per dial: one signed before the retry ladder would age out on it.
    let ticket = state
        .ticket
        .as_ref()
        .filter(|_| state.joined.token.is_none());
    let first = ticket.is_some();
    let signed = u
        .vault
        .with_signer(|me| -> Result<_, courier::CourierError> {
            let attestation = courier::attest::attest(me, device, now)?;
            let claim = ticket
                .map(|t| desktop_start_invite_claim(t.clone(), me, now))
                .transpose()?;
            Ok((attestation, claim))
        });
    let (attestation, claim) = match signed {
        Some(Ok(signed)) => signed,
        Some(Err(e)) => return Err(Stop::Fatal(e.into())),
        None => return Err(Stop::Fatal(NodeError::Locked)),
    };
    let hello = Frame::Hello {
        attestation,
        ws_id: u.ws.clone(),
        token: state.joined.token.clone(),
        claim,
        docs: Vec::new(),
    };
    send_frame(&mut send, &hello, FIRST_FRAME)
        .await
        .map_err(|e| retry(e.to_string()))?;
    let reply = tokio::time::timeout(net::wire::REPLY_TIMEOUT, frames.recv::<Frame>(MAX_FRAME))
        .await
        .map_err(|_| retry("the owner did not answer the hello".into()))?
        .map_err(|e| retry(e.to_string()))?;
    match reply {
        Some(Frame::Welcome { token, .. }) => {
            if let Some(token) = token {
                state.joined.token = Some(token);
                state.ticket = None;
                keep(&u.vault, &u.ws, &state.joined);
            }
        }
        Some(Frame::Refused { reason }) if first => {
            return Err(Stop::Fatal(NodeError::Refused(reason)));
        }
        Some(Frame::Refused { reason }) => return Err(retry(reason)),
        Some(_) => return Err(retry("the owner did not answer the hello".into())),
        None => return Err(retry("the owner closed before answering".into())),
    }
    if !state.announced {
        // The one stdout line: the harness's boot wait.
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "did: {}", u.did).and_then(|()| out.flush());
        state.announced = true;
    }
    let (tx, mut rx) = unbounded_channel();
    let sid = sessions().register_uplink(&u.ws, &state.joined.host_did, tx);
    let admin = Admin::hosted(u.vault.clone(), &state.joined.host_did);
    let announced_host = admin.is_host(&u.did, &u.ws, now_secs()).unwrap_or(false);
    let mut up = Up {
        u,
        state,
        admin,
        origin: sid.to_string(),
        announced_host,
    };
    let ended = serve_uplink(&mut up, &mut send, &mut frames, &mut rx).await;
    sessions().unregister(&u.ws, sid);
    Ok(ended.err().unwrap_or_else(|| "the owner closed".into()))
}

/// This side of one established session, from the `Welcome` on.
struct Up<'a> {
    u: &'a Uplink,
    state: &'a mut State,
    admin: Admin,
    /// The uplink's session id, the origin its merges carry: `Pusher::broadcast` leaves out
    /// the session an update came in on, so nothing the owner sent goes back up.
    origin: String,
    /// `HostHere` went up while this host's ledger copy already named it; when a replayed
    /// ledger first does, it goes up again — the owner dropped the earlier one.
    announced_host: bool,
}

impl Up<'_> {
    fn here(&self) -> Frame {
        Frame::HostHere {
            addr: host_addr_of(&self.u.ep.addr()),
        }
    }

    /// One frame from the owner; `Some` is the answer to send up.
    async fn handle(&mut self, frame: Frame) -> Result<Option<Frame>, String> {
        let u = self.u;
        match frame {
            Frame::Ledger { entries } => {
                let (admin, ws) = (self.admin.clone(), u.ws.clone());
                // Whole or not at all, and a frame with a forged entry ends the session:
                // the ladder redials, this host serving on from the ledger it had.
                serialized(u.sink.merge.clone(), move || {
                    admin
                        .replay_all(&entries)
                        .map_err(|e| format!("ledger of {ws}: {e}"))
                })
                .await??;
                // The ledger is how this host learns it was appointed; the owner lists it
                // from the `HostHere` that follows.
                let host = self
                    .admin
                    .is_host(&u.did, &u.ws, now_secs())
                    .map_err(|e| e.to_string())?;
                if host && !self.announced_host {
                    self.announced_host = true;
                    log(format!("appointed host of {}", u.ws));
                    return Ok(Some(self.here()));
                }
            }
            Frame::Hosts { hosts } => {
                u.sink.set_hosts(&u.ws, hosts.clone());
                self.state.joined.hosts = hosts;
                keep(&u.vault, &u.ws, &self.state.joined);
            }
            Frame::Update {
                item_id,
                layer,
                update,
                ..
            } => {
                let (admin, origin, sink) =
                    (self.admin.clone(), self.origin.clone(), u.sink.clone());
                let (ws, now) = (u.ws.clone(), now_secs());
                let merged = serialized(u.sink.merge.clone(), move || {
                    admin
                        .accept_trusted(
                            &ws,
                            &item_id,
                            &layer,
                            &update,
                            &origin,
                            now,
                            &HostPusher::new(sink),
                        )
                        .map_err(|e| format!("{item_id}/{layer:?}: {e}"))
                })
                .await?;
                if let Err(e) = merged {
                    log(format!("update from the owner: {e}"));
                }
            }
            Frame::Refused { reason } => log(format!("the owner refused: {reason}")),
            Frame::Rejected {
                item_id,
                layer,
                rejected,
            } => log(format!(
                "the owner rejected {item_id}/{layer:?}: {}",
                rejected.reason
            )),
            Frame::Ack {
                item_id,
                layer,
                missing: true,
                ..
            } => log(format!(
                "the owner lacks history this host has of {item_id}/{layer:?}"
            )),
            Frame::RoleDone { ok: false, reason } => log(format!("role: {reason}")),
            // `Fetch`, `Fetched`, `Hello`, `Welcome`, `Role`, `HostHere`: a member's to send,
            // not to hear; a clean `Ack` or `RoleDone` needs nothing.
            _ => {}
        }
        Ok(None)
    }
}

/// From the `Welcome` on: where this host is, its docs, then frames both ways until the
/// stream ends. `Err` is how it broke.
async fn serve_uplink(
    up: &mut Up<'_>,
    send: &mut SendStream,
    frames: &mut FrameBuf,
    rx: &mut UnboundedReceiver<Frame>,
) -> Result<(), String> {
    let wire = |e: net::Error| e.to_string();
    // Sent before appointment too: the owner ignores it until the ledger names this DID.
    send_frame(send, &up.here(), MAX_FRAME)
        .await
        .map_err(wire)?;
    for frame in doc_frames(&up.u.vault, &up.u.ws)? {
        send_frame(send, &frame, MAX_FRAME).await.map_err(wire)?;
    }
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(frame) => send_frame(send, &frame, MAX_FRAME).await.map_err(wire)?,
                None => return Ok(()),
            },
            incoming = frames.recv::<Frame>(MAX_FRAME) => match incoming {
                Ok(Some(frame)) => {
                    if let Some(reply) = up.handle(frame).await? {
                        send_frame(send, &reply, MAX_FRAME).await.map_err(wire)?;
                    }
                }
                Ok(None) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests;
