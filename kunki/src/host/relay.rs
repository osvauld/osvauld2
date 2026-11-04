//! The host's relay for app channels (`jetpack-mp.md` §6 step 3): which devices are here on
//! which item, the presence frames that follow from that, and the judgement each message passes
//! before it is passed on. The body is never read; the host checks the routing fields, the
//! device's signature over them, the manifest's `send`, `size` and `rate`, and that the `seq` is
//! new. The process's own open app is one more [`Peer`], fed by whoever drains its `tx`.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use courier::net::{Envelope, Rate, Sealed, Seen, may_send, open};
use courier::session::NetFrame;
use manifest::Manifest;
use tokio::sync::mpsc::UnboundedSender;

/// One device's channel stream, here on one item. `sid` names the stream: a device that opens
/// two items is one peer in each list. `rules` are what its messages on this item are judged
/// by — the item's manifest and the account's roles there, read when it opened.
#[derive(Clone)]
pub struct Peer {
    pub sid: u64,
    pub device: [u8; 32],
    pub attestation: String,
    pub tx: UnboundedSender<NetFrame>,
    pub rules: Arc<(Manifest, BTreeSet<String>)>,
}

#[derive(Default)]
struct Inner {
    here: HashMap<(String, String), Vec<Peer>>,
    seen: Seen,
    rate: Rate,
}

#[derive(Clone, Default)]
pub struct Nets(Arc<Mutex<Inner>>);

impl Nets {
    /// `peer` is here on `ws`/`item`: it hears who already is, they hear it. Again is a no-op.
    pub fn open(&self, ws: &str, item: &str, peer: Peer) {
        let mut inner = self.0.lock().unwrap();
        let list = inner.here.entry((ws.to_string(), item.to_string())).or_default();
        if list.iter().any(|p| p.sid == peer.sid) {
            return;
        }
        for p in list.iter() {
            let _ = p.tx.send(here(item, &peer));
            let _ = peer.tx.send(here(item, p));
        }
        list.push(peer);
    }

    pub fn close(&self, ws: &str, item: &str, sid: u64) {
        let mut inner = self.0.lock().unwrap();
        let key = (ws.to_string(), item.to_string());
        let Some(list) = inner.here.get_mut(&key) else {
            return;
        };
        let Some(i) = list.iter().position(|p| p.sid == sid) else {
            return;
        };
        let gone = list.remove(i);
        for p in list.iter() {
            let _ = p.tx.send(NetFrame::Gone {
                item_id: item.to_string(),
                device: gone.device,
            });
        }
        if list.is_empty() {
            inner.here.remove(&key);
        }
    }

    /// Every item `sid` was here on: its stream ended.
    pub fn close_all(&self, sid: u64) {
        let keys: Vec<(String, String)> = {
            let inner = self.0.lock().unwrap();
            inner
                .here
                .iter()
                .filter(|(_, list)| list.iter().any(|p| p.sid == sid))
                .map(|(k, _)| k.clone())
                .collect()
        };
        for (ws, item) in keys {
            self.close(&ws, &item, sid);
        }
    }

    pub fn is_open(&self, ws: &str, item: &str, sid: u64) -> bool {
        let inner = self.0.lock().unwrap();
        inner
            .here
            .get(&(ws.to_string(), item.to_string()))
            .is_some_and(|list| list.iter().any(|p| p.sid == sid))
    }

    /// To every peer here on `ws`/`item` but the stream it came in on.
    pub fn relay(&self, ws: &str, item: &str, from_sid: u64, sealed: &Sealed) {
        let inner = self.0.lock().unwrap();
        if let Some(list) = inner.here.get(&(ws.to_string(), item.to_string())) {
            for p in list.iter().filter(|p| p.sid != from_sid) {
                let _ = p.tx.send(NetFrame::Msg(sealed.clone()));
            }
        }
    }

    /// Whether stream `sid`, whose transport proved `device` on space `ws`, may have `sealed`
    /// relayed, by the rules its peer opened the item with. Rate last, so a refused message
    /// spends nothing. The reason is for the host's log; the sender is not told.
    pub fn judge(
        &self,
        sealed: &Sealed,
        sid: u64,
        device: [u8; 32],
        ws: &str,
        now_ms: u64,
    ) -> Result<Envelope, String> {
        let env = open(sealed).map_err(|e| format!("net: {e}"))?;
        let mut inner = self.0.lock().unwrap();
        let rules = inner
            .here
            .get(&(env.ws.clone(), env.item.clone()))
            .and_then(|list| list.iter().find(|p| p.sid == sid))
            .map(|p| p.rules.clone())
            .ok_or_else(|| format!("net: {} is not open on this stream", env.item))?;
        let (manifest, roles) = &*rules;
        let ch = may_send(&env, device, ws, manifest, roles).map_err(|e| format!("net: {e}"))?;
        if !inner.seen.fresh(&env) {
            return Err(format!("net: replayed or old seq {}", env.seq));
        }
        if !inner.rate.allow(&env, ch.rate_per_sec, now_ms) {
            return Err(format!("net: over rate on {}", env.channel));
        }
        Ok(env)
    }
}

fn here(item: &str, p: &Peer) -> NetFrame {
    NetFrame::Here {
        item_id: item.to_string(),
        attestation: p.attestation.clone(),
    }
}

/// The process's relay: one per endpoint, like [`super::sessions`].
pub fn nets() -> &'static Nets {
    static NETS: OnceLock<Nets> = OnceLock::new();
    NETS.get_or_init(Nets::default)
}

#[cfg(test)]
mod tests;
