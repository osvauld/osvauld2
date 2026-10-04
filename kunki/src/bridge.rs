//! The node's bridge: same shape as shell2's — one connection, one request, one reply — but
//! with nowhere to hand off to. Kunki has no UI thread to serialize through, so the accept
//! loop dispatches directly; staying single-threaded is what keeps `vault`'s single-handle
//! rule satisfied without a second mechanism.
//!
//! The socket is `$OSVAULD_KUNKI_SOCKET` (default `/tmp/osvauld-kunki.sock`), hardened to
//! `0600` by `osvauld_rpc::bind_uds` — shared with shell2's bridge, since a claim token
//! crossing this socket is as sensitive as the passphrases crossing that one.
//!
//! `Request` is its own small vocabulary, not shell2's: a different protocol for a different
//! trust boundary (this one is headed toward real peers over a transport, not a single local
//! automation caller).

use std::io;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use courier::ClaimHello;
use courier::invite::{InviteClaimHello, InviteRequest};
use courier::proof::{self, Proof, Replay};
use courier::publish::PublishHello;
use courier::subscribe::SubscribeHello;
use courier::sync::SyncHello;
use courier::token::Token;
use identity::Signer;
use osvauld_rpc::{Response, read_msg, write_msg};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use vault::Vault;

use crate::admin::Admin;
use crate::push::{LiveRegistry, Pusher};

/// See `shell2/src/bridge.rs`'s `READ_TIMEOUT`: a stalled client must not wedge the loop.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Current wall-clock time. A real clock, not virtual: nothing dispatched here yet has a TTL
/// worth fast-forwarding past in a test. That changes once claim/reconnect verbs land, at
/// which point this is what grows a `Frame`/`Advance` pair, not this file's shape.
fn now_secs() -> u64 {
    now_ms() / 1000
}

/// The clock request proofs are stamped and checked against (`courier::proof`). Public so the
/// desktop stamps with the same one.
pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Request {
    /// Liveness probe — also how a test harness waits for the socket.
    Ping,
    /// A brand-new node's first admin, redeeming the ticket it printed at boot. Every later
    /// member arrives through `Invite`/`ClaimInvite` instead — this is only ever answered once.
    Claim(ClaimHello),
    Publish(PublishHello),
    /// An existing member asking the node to mint an invite.
    Invite(InviteRequest),
    /// A claimant redeeming a ticket an `Invite` call produced.
    ClaimInvite(InviteClaimHello),
    /// A desktop pushing local changes for one item's layer and pulling back what it lacks.
    Sync(SyncHello),
    /// A desktop declaring interest in an item's layer, for a future push.
    Subscribe(SubscribeHello),
    Unsubscribe(SubscribeHello),
    /// Open the channel pushes actually travel on. Unlike every other request here, this one
    /// does not get one reply and close — after the ack, the connection is held open and every
    /// `Push` the desktop is subscribed to is relayed down it until it disconnects. See
    /// `serve`'s accept loop, which is what gives this request its different treatment; nothing
    /// else about the wire format sets it apart.
    Listen {
        desktop_did: String,
        token: Token,
    },
}

impl Request {
    /// The desktop a request acts for — the DID its proof must be signed by. `None` for the
    /// requests that prove themselves another way (claims carry an attestation) or need no
    /// caller at all.
    pub fn caller(&self) -> Option<&str> {
        match self {
            Request::Ping | Request::Claim(_) | Request::ClaimInvite(_) => None,
            Request::Publish(h) => Some(&h.desktop_did),
            Request::Invite(r) => Some(&r.desktop_did),
            Request::Sync(h) => Some(&h.desktop_did),
            Request::Subscribe(h) | Request::Unsubscribe(h) => Some(&h.desktop_did),
            Request::Listen { desktop_did, .. } => Some(desktop_did),
        }
    }
}

/// What crosses the socket: a request's exact bytes, and the caller's [`Proof`] over them.
/// `body` stays raw so the node hashes the bytes that were signed, not a re-encoding.
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub body: Box<RawValue>,
    #[serde(default)]
    pub proof: Option<Proof>,
}

impl Envelope {
    pub fn plain(req: &Request) -> Result<Self, String> {
        let body = serde_json::value::to_raw_value(req).map_err(|e| e.to_string())?;
        Ok(Self { body, proof: None })
    }

    /// Refuses a request that names someone other than `signer` — the node would.
    pub fn signed(
        req: &Request,
        signer: &(impl Signer + ?Sized),
        node_did: &str,
        now_ms: u64,
    ) -> Result<Self, String> {
        if req.caller().is_some_and(|did| did != signer.did()) {
            return Err("request names a different desktop than the signer".into());
        }
        let mut env = Self::plain(req)?;
        env.proof = Some(proof::prove(
            signer,
            node_did,
            env.body.get().as_bytes(),
            now_ms,
        ));
        Ok(env)
    }
}

/// The node's check on every envelope before anything acts on it.
pub struct Gate {
    node_did: String,
    replay: Replay,
}

impl Gate {
    /// `since_ms` is when this gate started — the node's boot.
    pub fn new(node_did: String, since_ms: u64) -> Self {
        Self {
            node_did,
            replay: Replay::new(since_ms),
        }
    }

    pub fn open(&mut self, env: &Envelope, now_ms: u64) -> Result<Request, String> {
        let body = env.body.get().as_bytes();
        let req: Request = serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
        if let Some(caller) = req.caller() {
            let proof = env.proof.as_ref().ok_or("unsigned request")?;
            proof::verify(proof, caller, &self.node_did, body, now_ms)
                .map_err(|e| e.to_string())?;
            self.replay
                .admit(proof, now_ms)
                .map_err(|e| e.to_string())?;
        }
        Ok(req)
    }
}

/// Public so a desktop-side client (`shell2::node`) dials the same path by the same
/// convention, rather than a second copy of the env var name and default drifting from this
/// one.
pub fn socket_path() -> PathBuf {
    std::env::var_os("OSVAULD_KUNKI_SOCKET")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/osvauld-kunki.sock"))
}

/// Bind and serve forever at the env's socket path. The node's account is already open by the
/// time this is called, so a bind failure is returned rather than swallowed — starting with no
/// way to be reached is not a node worth running.
pub fn serve_forever(vault: Vault, registry: LiveRegistry) -> io::Result<()> {
    serve(&socket_path(), vault, registry)
}

/// Same as [`serve_forever`], at an explicit path rather than the env's. A caller in the same
/// process as another bridge (a test standing up a node next to a desktop) cannot share one
/// env var between them — this is what lets it bind its own path instead.
///
/// Every request but `Listen` stays on this thread, sequential, same as before — that is what
/// keeps `vault`'s single-handle rule satisfied without a second mechanism. A `Listen` gets its
/// own thread because it is the one request that does not return; nothing else here needs one.
/// `registry` (not a generic `Pusher`) because this loop has to register/unregister a `Listen`
/// connection, not only push through one — `dispatch`/`handle` stay generic for tests that want
/// a `NoopPusher`/`MockPusher` instead.
pub fn serve(socket: &std::path::Path, vault: Vault, registry: LiveRegistry) -> io::Result<()> {
    let node_did = vault
        .with_signer(|node| node.did().to_string())
        .ok_or_else(|| io::Error::other("node account is locked"))?;
    let mut gate = Gate::new(node_did, now_ms());
    let listener = osvauld_rpc::bind_uds(socket)?;
    for conn in listener.incoming() {
        match conn {
            Ok(mut conn) => {
                let req = match read_request(&mut conn, &mut gate) {
                    Ok(Ok(req)) => req,
                    Ok(Err(refused)) => {
                        let _ = reply(&mut conn, &Response::err(refused));
                        continue;
                    }
                    Err(e) => {
                        eprintln!("kunki: bad request: {e}");
                        continue;
                    }
                };
                if let Request::Listen { desktop_did, token } = req {
                    let vault = vault.clone();
                    let registry = registry.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = handle_listen(conn, &vault, &registry, desktop_did, token) {
                            eprintln!("kunki: listen ended: {e}");
                        }
                    });
                } else if let Err(e) = respond(conn, req, &vault, &registry) {
                    eprintln!("kunki: request failed: {e}");
                }
            }
            Err(e) => eprintln!("kunki: accept failed: {e}"),
        }
    }
    Ok(())
}

/// Outer `Err` is the connection failing; inner is the gate refusing, which gets an answer.
fn read_request(conn: &mut UnixStream, gate: &mut Gate) -> io::Result<Result<Request, String>> {
    conn.set_read_timeout(Some(READ_TIMEOUT))?;
    let env: Envelope = serde_json::from_slice(&read_msg(conn)?)
        .map_err(|e| io::Error::other(format!("bad envelope: {e}")))?;
    Ok(gate.open(&env, now_ms()))
}

fn reply(conn: &mut UnixStream, resp: &Response) -> io::Result<()> {
    let payload =
        serde_json::to_vec(resp).map_err(|e| io::Error::other(format!("response encode: {e}")))?;
    write_msg(conn, &payload)
}

fn respond(
    mut conn: UnixStream,
    req: Request,
    vault: &Vault,
    pusher: &impl Pusher,
) -> io::Result<()> {
    reply(&mut conn, &dispatch(req, vault, pusher))
}

/// One connection, one request, one reply — the shape every request but `Listen` keeps, which
/// tests reach for against a `NoopPusher`/`MockPusher`.
#[cfg(test)]
fn handle(
    mut conn: UnixStream,
    vault: &Vault,
    gate: &mut Gate,
    pusher: &impl Pusher,
) -> io::Result<()> {
    match read_request(&mut conn, gate)? {
        Ok(req) => respond(conn, req, vault, pusher),
        Err(refused) => reply(&mut conn, &Response::err(refused)),
    }
}

/// Authorize, ack, then hold the connection open — relaying every `Push` [`LiveRegistry`]
/// registers for `desktop_did` until the connection drops or a write to it fails. This is the
/// one bridge request that does not return after one reply.
fn handle_listen(
    mut conn: UnixStream,
    vault: &Vault,
    registry: &LiveRegistry,
    desktop_did: String,
    token: Token,
) -> io::Result<()> {
    let ack = match Admin::new(vault.clone()).authorize_listen(&desktop_did, &token, now_secs()) {
        Ok(()) => Response::ok("listening"),
        Err(e) => Response::err(e.to_string()),
    };
    let refused = matches!(ack, Response::Err { .. });
    let payload =
        serde_json::to_vec(&ack).map_err(|e| io::Error::other(format!("response encode: {e}")))?;
    write_msg(&mut conn, &payload)?;
    if refused {
        return Ok(());
    }

    let rx = registry.register(&desktop_did);
    let result = (|| -> io::Result<()> {
        for push in rx {
            let payload = serde_json::to_vec(&push)
                .map_err(|e| io::Error::other(format!("push encode: {e}")))?;
            write_msg(&mut conn, &payload)?;
        }
        Ok(())
    })();
    registry.unregister(&desktop_did);
    result
}

fn dispatch(req: Request, vault: &Vault, pusher: &impl Pusher) -> Response {
    match req {
        Request::Ping => Response::ok("pong"),
        Request::Claim(hello) => match Admin::new(vault.clone()).accept_claim(hello, now_secs()) {
            Ok(welcome) => Response::ok(welcome),
            Err(e) => Response::err(e.to_string()),
        },
        Request::Publish(hello) => {
            match Admin::new(vault.clone()).accept_publish(hello, now_secs()) {
                Ok(ack) => Response::ok(ack),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Invite(request) => {
            match Admin::new(vault.clone()).issue_invite(&request, "kunki", now_secs()) {
                Ok(ticket) => Response::ok(ticket),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::ClaimInvite(hello) => {
            match Admin::new(vault.clone()).accept_invite(hello, now_secs()) {
                Ok(welcome) => Response::ok(welcome),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Sync(hello) => {
            match Admin::new(vault.clone()).accept_sync(hello, now_secs(), pusher) {
                Ok(ack) => Response::ok(ack),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Subscribe(hello) => {
            match Admin::new(vault.clone()).subscribe(&hello, now_secs()) {
                Ok(()) => Response::ok(()),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::Unsubscribe(hello) => {
            match Admin::new(vault.clone()).unsubscribe(&hello, now_secs()) {
                Ok(()) => Response::ok(()),
                Err(e) => Response::err(e.to_string()),
            }
        }
        // `serve`'s accept loop intercepts this before it ever reaches `dispatch` — reachable
        // here only if something calls `handle`/`respond` directly with one, which is a caller
        // bug, not a request this function itself knows how to answer.
        Request::Listen { .. } => {
            Response::err("Listen must be answered by the accept loop, not dispatched directly")
        }
    }
}

#[cfg(test)]
mod tests;
