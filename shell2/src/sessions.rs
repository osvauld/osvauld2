//! The member half of a space session (laptop-host §2 Sessions): what this account keeps
//! about a space it joined from another host, the dial that opens a session to that host,
//! and the loop that carries frames both ways until the connection drops. Frames arrive on
//! the UI thread as `Msg::SessionFrame`; the shell decides what each means for its open apps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use courier::invite::InviteClaimHello;
use courier::link::{HostAddr, Link};
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::session::{DocState, Frame};
use courier::token::Token;
use iroh::{Endpoint, EndpointAddr, TransportAddr};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use runtime::EventLoopProxy;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle as Rt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use vault::Vault;

use crate::{Msg, clock};

/// A space this account joined from another host, as the vault keeps it (`joined/<ws>`,
/// sealed): who hosts it, where, and the token the host minted for this account once the
/// first session's claim was accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Joined {
    pub host_did: String,
    pub host: HostAddr,
    pub token: Option<Token>,
}

fn joined_key(ws_id: &str) -> String {
    format!("joined/{ws_id}")
}

pub fn save_joined(vault: &Vault, ws_id: &str, joined: &Joined) -> Result<(), String> {
    let bytes = serde_json::to_vec(joined).map_err(|e| e.to_string())?;
    vault
        .put_entry(&joined_key(ws_id), &bytes)
        .map_err(|e| e.to_string())
}

pub fn load_joined(vault: &Vault) -> HashMap<String, Joined> {
    let mut out = HashMap::new();
    let Ok(names) = vault.list_entries("joined/") else {
        return out;
    };
    for name in names {
        let Some(ws_id) = name.strip_prefix("joined/") else {
            continue;
        };
        if let Ok(Some(bytes)) = vault.get_entry(&name) {
            if let Ok(joined) = serde_json::from_slice::<Joined>(&bytes) {
                out.insert(ws_id.to_string(), joined);
            }
        }
    }
    out
}

pub fn endpoint_addr_of(host: &HostAddr) -> Result<EndpointAddr, String> {
    let id = iroh::EndpointId::from_bytes(&host.device).map_err(|e| e.to_string())?;
    let mut addrs = Vec::new();
    if let Some(relay) = &host.relay {
        let url = relay.parse().map_err(|e| format!("relay url: {e}"))?;
        addrs.push(TransportAddr::Relay(url));
    }
    addrs.extend(host.addrs.iter().map(|a| TransportAddr::Ip(*a)));
    Ok(EndpointAddr::from_parts(id, addrs))
}

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
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Everything a dial needs; `claim` only on the first session after a join.
pub struct Dial {
    pub ws_id: String,
    pub joined: Joined,
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
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    rt.spawn(async move {
        let outcome = run(ep, vault, proxy.clone(), gen_, dial, rx).await;
        let reason = outcome.err().unwrap_or_else(|| "closed".to_string());
        let _ = proxy.send_event(Msg::SessionDown(gen_, ws_id, seq, reason));
    });
    self::Handle { tx, up: false, seq }
}

async fn run(
    ep: Endpoint,
    vault: Vault,
    proxy: EventLoopProxy<Msg>,
    gen_: u64,
    dial: Dial,
    mut rx: UnboundedReceiver<Frame>,
) -> Result<(), String> {
    let addr = endpoint_addr_of(&dial.joined.host)?;
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
                let _ = proxy.send_event(Msg::SessionToken(gen_, dial.ws_id.clone(), token));
            }
        }
        Some(Frame::Refused { reason }) => return Err(reason),
        Some(_) => return Err("the host did not answer the hello".into()),
        None => return Err("the host closed before answering".into()),
    }
    let _ = proxy.send_event(Msg::SessionUp(gen_, dial.ws_id.clone()));
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
                            let _ = proxy.send_event(Msg::SessionFrame(gen_, dial.ws_id.clone(), frame));
                        }
                    }
                }
                Ok(Some(frame)) => {
                    let _ = proxy.send_event(Msg::SessionFrame(gen_, dial.ws_id.clone(), frame));
                }
                Ok(None) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            },
        }
    }
}

/// What a `Fetch` brought back, ready for the shell to adopt and claim.
pub struct Fetched {
    pub link: Link,
    pub invite: courier::invite::InviteTicket,
    pub hosts: Vec<HostAddr>,
    pub ws: PublishedWorkspace,
    pub items: Vec<PublishedItem>,
}

/// Dials the link's host and redeems its code. Blocking; run it off the UI thread.
pub fn fetch(rt: &Rt, ep: Endpoint, link: Link) -> Result<Fetched, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let code = link.code;
    let host = link.host.clone();
    rt.spawn(async move {
        let result = async {
            let addr = endpoint_addr_of(&host)?;
            let conn = net::endpoint::dial(&ep, addr, net::SPACE_ALPN, net::wire::DIAL_TIMEOUT)
                .await
                .map_err(|e| e.to_string())?;
            let (mut send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
            send_frame(&mut send, &Frame::Fetch { code }, FIRST_FRAME)
                .await
                .map_err(|e| e.to_string())?;
            let _ = send.finish();
            let mut frames = FrameBuf::new(recv);
            let reply = tokio::time::timeout(net::wire::REPLY_TIMEOUT, frames.recv::<Frame>(MAX_FRAME))
                .await
                .map_err(|_| "the host did not answer".to_string())?
                .map_err(|e| e.to_string())?;
            match reply {
                Some(Frame::Fetched { invite, hosts, ws, items }) => Ok((invite, hosts, ws, items)),
                Some(Frame::Refused { reason }) => Err(reason),
                Some(_) => Err("the host answered with the wrong frame".into()),
                None => Err("the host closed without answering".into()),
            }
        }
        .await;
        let _ = tx.send(result);
    });
    let (invite, hosts, ws, items) = rx.recv().map_err(|_| "net runtime stopped".to_string())??;
    Ok(Fetched {
        link,
        invite,
        hosts,
        ws,
        items,
    })
}
