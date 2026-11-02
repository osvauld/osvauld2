//! The member half of a space session (laptop-host §2 Sessions) as the shell runs it: the
//! dial that opens a session to a joined space's host and the loop that carries frames both
//! ways until the connection drops. Frames arrive on the UI thread as `Msg::SessionFrame`;
//! the shell decides what each means for its open apps. What a member keeps and the `Fetch`
//! that redeems a link need no shell and live in `kunki::session`, re-exported here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use courier::invite::InviteClaimHello;
use courier::session::{DocState, Frame, HostEntry};
use iroh::Endpoint;
pub use kunki::session::{Fetched, Joined, endpoint_addr_of, fetch, load_joined, save_joined};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use runtime::EventLoopProxy;
use tokio::runtime::Handle as Rt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use vault::Vault;

use crate::{Msg, clock};

/// Docs an app thread is waiting on from the host, by (space, item, doc): the UI thread
/// hands the bytes over when the session's `Update` arrives. Shared between the shell and
/// every app thread's resolver.
pub type Pulls =
    Arc<Mutex<HashMap<(String, String, String), std::sync::mpsc::SyncSender<Vec<u8>>>>>;

/// What an app thread needs to pull a doc the host holds before seeding its own: the
/// session to ask on and the table the answer lands in.
#[derive(Clone)]
pub struct Pull {
    pub tx: UnboundedSender<Frame>,
    pub waiters: Pulls,
}

/// The shell's end of a session: frames to send go in here. Dropping it ends the session.
pub struct Handle {
    pub tx: UnboundedSender<Frame>,
    pub up: bool,
    /// Which dial this is, so its `SessionDown` is told from a newer session's.
    pub seq: u64,
    /// Which host of `Joined::host_list` it dialled, and who that was — the list may be
    /// replaced by a `Hosts` frame while the session is up.
    pub host_index: usize,
    pub host_did: String,
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Everything a dial needs; `claim` only on the first session after a join.
pub struct Dial {
    pub ws_id: String,
    pub joined: Joined,
    /// The host to dial, `host_index` into `joined.host_list()`.
    pub host: HostEntry,
    pub host_index: usize,
    pub docs: Vec<DocState>,
    pub claim: Option<InviteClaimHello>,
    /// App threads waiting on a doc: answered from the loop itself, since the UI thread may
    /// be waiting on that very app thread to finish opening.
    pub pulls: Pulls,
}

/// Opens a session to the space's host on the net runtime and returns the shell's handle at
/// once; `Msg::SessionUp`/`SessionFrame`/`SessionDown` say what became of it.
pub fn dial(
    rt: &Rt,
    ep: Endpoint,
    vault: Vault,
    proxy: EventLoopProxy<Msg>,
    gen_: u64,
    dial: Dial,
) -> self::Handle {
    let (tx, rx) = unbounded_channel();
    let ws_id = dial.ws_id.clone();
    let (host_index, host_did) = (dial.host_index, dial.host.did.clone());
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    rt.spawn(async move {
        let outcome = run(ep, vault, proxy.clone(), gen_, seq, dial, rx).await;
        let reason = outcome.err().unwrap_or_else(|| "closed".to_string());
        let _ = proxy.send_event(Msg::SessionDown(gen_, ws_id, seq, reason));
    });
    self::Handle {
        tx,
        up: false,
        seq,
        host_index,
        host_did,
    }
}

/// Where a space's next dial goes, kept per space from one drop to the next session that
/// comes up (laptop-host §2 "Failover"): the index to dial and how many whole rounds of the
/// host list have failed since.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Redial {
    pub next: usize,
    pub round: usize,
}

/// What a drop leads to: the host to dial and, only once every host has been tried this
/// round, the ladder's pause before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failover {
    pub index: usize,
    pub pause: Option<Duration>,
}

impl Redial {
    /// The session on host `from` of `hosts` dropped. The next host is dialled at once; when
    /// the list wraps the round is over, and the pause is the ladder's for the rounds so far
    /// — with one host, exactly the old retry ladder. `from` past the end (the list shrank)
    /// wraps too.
    pub fn after_drop(&mut self, from: usize, hosts: usize) -> Failover {
        self.next = from + 1;
        let wrapped = self.next >= hosts.max(1);
        if wrapped {
            self.next = 0;
            self.round += 1;
        }
        Failover {
            index: self.next,
            pause: wrapped.then(|| net::wire::retry_after(self.round - 1)),
        }
    }
}

async fn run(
    ep: Endpoint,
    vault: Vault,
    proxy: EventLoopProxy<Msg>,
    gen_: u64,
    seq: u64,
    dial: Dial,
    mut rx: UnboundedReceiver<Frame>,
) -> Result<(), String> {
    let addr = endpoint_addr_of(&dial.host.addr)?;
    let conn = net::endpoint::dial(&ep, addr, net::SPACE_ALPN, net::wire::DIAL_TIMEOUT)
        .await
        .map_err(|e| e.to_string())?;
    let (mut send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    let mut frames = FrameBuf::new(recv);
    let device = vault.device_public_key().map_err(|e| e.to_string())?;
    let attestation = vault
        .with_signer(|me| courier::attest::attest(me, device, clock::now_secs()))
        .ok_or("account is locked")?
        .map_err(|e| e.to_string())?;
    let hello = Frame::Hello {
        attestation,
        ws_id: dial.ws_id.clone(),
        token: dial.joined.token.clone(),
        claim: dial.claim,
        docs: dial.docs,
    };
    send_frame(&mut send, &hello, FIRST_FRAME)
        .await
        .map_err(|e| e.to_string())?;
    match frames.recv::<Frame>(MAX_FRAME).await.map_err(|e| e.to_string())? {
        Some(Frame::Welcome { token, .. }) => {
            if let Some(token) = token {
                let _ = proxy.send_event(Msg::SessionToken(gen_, dial.ws_id.clone(), seq, token));
            }
        }
        Some(Frame::Refused { reason }) => return Err(reason),
        Some(_) => return Err("the host did not answer the hello".into()),
        None => return Err("the host closed before answering".into()),
    }
    let _ = proxy.send_event(Msg::SessionUp(gen_, dial.ws_id.clone(), seq));
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(frame) => send_frame(&mut send, &frame, MAX_FRAME).await.map_err(|e| e.to_string())?,
                None => return Ok(()),
            },
            incoming = frames.recv::<Frame>(MAX_FRAME) => match incoming {
                Ok(Some(Frame::Update { item_id, layer: courier::sync::SyncLayer::Doc(name), vv, update })) => {
                    let waiter = dial
                        .pulls
                        .lock()
                        .unwrap()
                        .remove(&(dial.ws_id.clone(), item_id.clone(), name.clone()));
                    match waiter {
                        Some(tx) => {
                            let _ = tx.send(update);
                        }
                        None => {
                            let frame = Frame::Update { item_id, layer: courier::sync::SyncLayer::Doc(name), vv, update };
                            let _ = proxy.send_event(Msg::SessionFrame(gen_, dial.ws_id.clone(), seq, frame));
                        }
                    }
                }
                Ok(Some(frame)) => {
                    let _ = proxy.send_event(Msg::SessionFrame(gen_, dial.ws_id.clone(), seq, frame));
                }
                Ok(None) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests;
