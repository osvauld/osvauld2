//! The shell's end of `kunki::host` (laptop-host §2 Sessions): the acceptor, the session
//! table and the loop live there; what ties them to this process is [`ShellSink`]. Every
//! update a member sends is merged by the UI thread (`Msg::HostSync`): into the live doc when
//! the owner has the app open, into the vault when not — one copy per doc. The clock is
//! `clock`, the in-process listener `node::registry`, and the hosts to advertise are this
//! device first, then the appointed ones that have said where they are.

use courier::session::HostEntry;
use courier::sync::SyncHello;
use iroh::Endpoint;
use kunki::admin::Admin;
use kunki::host::{HERE, Sink, SyncFuture, appointed_hosts, host_addr_of};
use kunki::push::{Push, Pusher as _};
use runtime::EventLoopProxy;
use tokio::sync::oneshot;
use vault::Vault;

use crate::{Msg, Slot, clock, node};

/// `gen_` tags every merge it sends: a session that outlives a lock talks to nobody. `ep` is
/// `None` before the bind — this device is then no host to advertise, and nothing is asking.
#[derive(Clone)]
pub struct ShellSink {
    pub vault: Vault,
    pub proxy: EventLoopProxy<Msg>,
    pub gen_: u64,
    pub ep: Option<Endpoint>,
}

impl Sink for ShellSink {
    fn admin_for(&self, ws_id: &str) -> Option<Admin> {
        node::hosting(&self.vault)
            .ok()?
            .iter()
            .any(|w| w == ws_id)
            .then(|| Admin::new(self.vault.clone()))
    }

    fn fetch_admin(&self) -> Option<Admin> {
        Some(Admin::new(self.vault.clone()))
    }

    fn now_secs(&self) -> u64 {
        clock::now_secs()
    }

    fn now_ms(&self) -> u64 {
        clock::now_ms()
    }

    fn sync(&self, hello: SyncHello, device: [u8; 32]) -> SyncFuture {
        let (tx, rx) = oneshot::channel();
        let sent = self
            .proxy
            .send_event(Msg::HostSync(self.gen_, hello, device, Slot::new(tx)))
            .map_err(|_| "shell gone".to_string());
        Box::pin(async move {
            sent?;
            rx.await.map_err(|_| "reply dropped".to_string())?
        })
    }

    /// The owner's own listener, unless the owner wrote it — its open app merged that already.
    fn local_push(&self, subscriber_did: &str, update: &Push) {
        if !(update.origin == HERE && subscriber_did == update.author) {
            node::registry().push(subscriber_did, update);
        }
    }

    fn hosts(&self, ws_id: &str) -> Vec<HostEntry> {
        let (Some(ep), Ok(did)) = (&self.ep, kunki::node::did(&self.vault)) else {
            return Vec::new();
        };
        let mut hosts = vec![HostEntry {
            did,
            addr: host_addr_of(&ep.addr()),
        }];
        match appointed_hosts(&Admin::new(self.vault.clone()), ws_id, clock::now_secs()) {
            Ok(more) => hosts.extend(more),
            Err(e) => eprintln!("host: hosts of {ws_id}: {e}"),
        }
        hosts
    }
}
