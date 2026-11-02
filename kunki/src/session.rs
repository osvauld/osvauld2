//! The member half of a space session that has no shell in it (laptop-host §2 Sessions):
//! what an account keeps about a space it joined from another host (`joined/<ws>`), the
//! address a [`HostAddr`] dials, and the `Fetch` that redeems a share link. The dial and its
//! loop stay with whoever owns the frames' destination — the shell's `Msg`, the headless
//! host's uplink.

use std::collections::HashMap;

use courier::invite::InviteTicket;
use courier::link::{HostAddr, Link};
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::session::{Frame, HostEntry};
use courier::token::Token;
use iroh::{Endpoint, EndpointAddr, TransportAddr};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle as Rt;
use vault::Vault;

/// A space this account joined from another host, as the vault keeps it (`joined/<ws>`,
/// sealed): who owns it, where it was first dialled, the token the host minted for this
/// account once the first session's claim was accepted, and every host it has been told of
/// since (`Frame::Hosts`, owner first — the order to fail over in).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Joined {
    pub host_did: String,
    pub host: HostAddr,
    pub token: Option<Token>,
    #[serde(default)]
    pub hosts: Vec<HostEntry>,
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

/// What a `Fetch` brought back, ready to adopt and claim.
pub struct Fetched {
    pub link: Link,
    pub invite: InviteTicket,
    pub hosts: Vec<HostEntry>,
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
