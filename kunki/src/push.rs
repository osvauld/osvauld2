//! Who delivers a pushed update once the node has one to send, kept as its own trait so F4b's
//! fanout can be tested against an in-memory stand-in instead of a real transport — the same
//! split `sync`/`bridge` already draws between deciding and doing, one level further out. Lives
//! here, not in courier: courier stays decide (pure), kunki stays do (I/O), same division sync
//! itself already keeps.
//!
//! Fire-and-forget: a push that fails to land is not retried and nothing is queued for it. The
//! subscriber's own next `Sync` call is what catches it up if one is ever lost — the same
//! correction path a missed message already had, not a new one built for this.

use courier::sync::SyncLayer;
use serde::{Deserialize, Serialize};

/// Not `SyncAck` itself: that struct's `request_id` exists to let a desktop correlate a reply
/// with its own request, and a push has no request behind it to correlate with. Everything else
/// here is the same shape sync already proved — `ws_id`/`item_id`/`layer` to address it, bytes
/// Loro's own `import` already knows how to consume regardless of which export mode produced
/// them. `Serialize`/`Deserialize` because a `Listen` connection now carries these over the
/// wire, not only in-process to a `MockPusher`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Push {
    pub ws_id: String,
    pub item_id: String,
    pub layer: SyncLayer,
    pub snapshot: Vec<u8>,
}

pub trait Pusher {
    /// Best-effort delivery to one subscriber. No return value: a push that can't be delivered
    /// (subscriber offline, connection refused) is not an error the caller should branch on —
    /// fanout finishes the same way whether every push lands or none do.
    fn push(&self, subscriber_did: &str, update: &Push);
}

/// Delivers nothing. Kept for tests and any bridge path that doesn't care about push — the
/// node itself now boots with [`LiveRegistry`] instead.
pub struct NoopPusher;

impl Pusher for NoopPusher {
    fn push(&self, _subscriber_did: &str, _update: &Push) {}
}

/// One entry per desktop currently holding a `Listen` connection open — the live half
/// `NoopPusher` had nothing to hold. A `SyncSender`, not the unbounded kind: `push` runs
/// inline on whichever thread called it (`Admin::accept_sync`'s `fan_out`, possibly the main
/// accept loop's own thread for a request that arrived over the sequential path), so a full or
/// abandoned receiver must never block it — `try_send` on a bounded channel is the only shape
/// that can't.
#[derive(Clone, Default)]
pub struct LiveRegistry {
    inner: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::mpsc::SyncSender<Push>>>,
    >,
}

/// How many pushes a desktop can be behind before new ones are dropped rather than queued — a
/// slow consumer costs itself missed pushes, never the node's own throughput. The subscriber's
/// own next `Sync` (or the reconciliation tick) is still what catches it up.
const CHANNEL_CAPACITY: usize = 32;

impl LiveRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `desktop_did` as reachable and returns the receiving half for its `Listen`
    /// connection to relay. A second registration for the same did replaces the first — the
    /// old connection finds out it's stale on its own next failed write, not from here.
    pub fn register(&self, desktop_did: &str) -> std::sync::mpsc::Receiver<Push> {
        let (tx, rx) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        self.inner
            .lock()
            .unwrap()
            .insert(desktop_did.to_string(), tx);
        rx
    }

    pub fn unregister(&self, desktop_did: &str) {
        self.inner.lock().unwrap().remove(desktop_did);
    }
}

impl Pusher for LiveRegistry {
    fn push(&self, subscriber_did: &str, update: &Push) {
        if let Some(tx) = self.inner.lock().unwrap().get(subscriber_did) {
            let _ = tx.try_send(update.clone());
        }
    }
}

/// Records what was sent to whom instead of sending it anywhere, so a test can assert fanout
/// without a real transport. `Mutex`, not `RefCell`: `push` takes `&self`, and nothing about this
/// trait says its implementors are single-threaded.
#[cfg(test)]
#[derive(Default)]
pub struct MockPusher {
    received: std::sync::Mutex<Vec<(String, Push)>>,
}

#[cfg(test)]
impl MockPusher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn received_by(&self, subscriber_did: &str) -> Vec<Push> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter(|(did, _)| did == subscriber_did)
            .map(|(_, update)| update.clone())
            .collect()
    }
}

#[cfg(test)]
impl Pusher for MockPusher {
    fn push(&self, subscriber_did: &str, update: &Push) {
        self.received
            .lock()
            .unwrap()
            .push((subscriber_did.to_string(), update.clone()));
    }
}

#[cfg(test)]
mod tests;
