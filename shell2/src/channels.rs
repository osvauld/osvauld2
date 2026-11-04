//! App channels as the shell carries them (`jetpack-mp.md` §6 step 4). An app's `net.send`
//! leaves its thread as [`Msg::NetOut`]; here it is signed with the device key and routed: down
//! the space's channel stream when the space is joined, into this process's own relay when this
//! device hosts it. What comes back — messages, who is here, who left — is handed to the app
//! with that item open. Apps in a space with neither route hear only themselves.
//!
//! A member's channel stream is the session's second QUIC stream ([`stream`]), opened after the
//! session's `Welcome` and gone with it. A hosting shell's own app is one more relay peer, its
//! frames carried to the UI thread by a small thread of its own.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use app_host::net::{NetPeer, Outgoing};
use courier::net::{Envelope, seal};
use courier::session::{Frame, NET_BEAT_MS, NetFrame};
use courier::token::Token;
use iroh::endpoint::Connection;
use kunki::admin::Admin;
use kunki::host::relay::{Peer, next_sid, nets};
use net::frames::{FIRST_FRAME, FrameBuf, MAX_FRAME, send_frame};
use runtime::EventLoopProxy;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::{Msg, Route, Shell, clock};

pub fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A `seq` that only rises, across items, channels and restarts: wall-clock microseconds, or
/// one past the last if the clock has not moved. The host's `Seen` refuses anything not newer.
fn next_seq() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = clock::now_ms() * 1000;
    let prev = LAST
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| Some(now.max(last + 1)))
        .unwrap_or_else(|last| last);
    now.max(prev + 1)
}

/// This host's own open apps in its relay: item → (space, stream id).
#[derive(Default)]
pub struct Channels {
    local: HashMap<Arc<str>, (String, u64)>,
}

impl Shell {
    /// An app opened: it learns who it is, and joins its space's channels if there is a route.
    pub(crate) fn net_join(&mut self, id: &Arc<str>, ws: &str) {
        let (Ok(device), Some(did)) = (
            self.vault.device_public_key(),
            self.vault.with_signer(|d| d.did().to_string()),
        ) else {
            return;
        };
        if let Some(app) = self.apps.get(id) {
            let me = NetPeer { id: hex(&device), account: did };
            app.call(move |h| h.app.net().set_me(me));
        }
        match self.route(ws) {
            Route::Session => {
                if let Some(h) = self.sessions.get(ws) {
                    let _ = h.net.send(NetFrame::Open { item_id: id.to_string() });
                }
            }
            Route::Here => self.local_open(id, ws, device),
            Route::Socket => {}
        }
    }

    fn local_open(&mut self, id: &Arc<str>, ws: &str, device: [u8; 32]) {
        let now = clock::now_secs();
        let attestation = match self.vault.with_signer(|me| courier::attest::attest(me, device, now)) {
            Some(Ok(a)) => a,
            _ => return,
        };
        let rules = match Admin::new(self.vault.clone()).own_net_rules(ws, id, now) {
            Ok(rules) => rules,
            Err(e) => {
                eprintln!("net: {id}: {e}");
                return;
            }
        };
        let sid = next_sid();
        let (tx, mut rx) = unbounded_channel::<NetFrame>();
        let (proxy, gen_, space) = (self.proxy.clone(), self.gen_(), ws.to_string());
        // Ends when the relay drops this peer's sender (`local_close`).
        std::thread::spawn(move || {
            while let Some(frame) = rx.blocking_recv() {
                let _ = proxy.send_event(Msg::NetFrame(gen_, space.clone(), 0, frame));
            }
        });
        nets().open(ws, id, Peer { sid, device, attestation, tx, rules: Arc::new(rules) });
        self.channels.local.insert(id.clone(), (ws.to_string(), sid));
    }

    /// An app closed: it leaves its space's channels.
    pub(crate) fn net_leave(&mut self, id: &Arc<str>, ws: &str) {
        if let Some((ws, sid)) = self.channels.local.remove(id) {
            nets().close(&ws, id, sid);
        } else if let Some(h) = self.sessions.get(ws) {
            let _ = h.net.send(NetFrame::Close { item_id: id.to_string() });
        }
    }

    /// Every open app leaves: the account is going away.
    pub(crate) fn net_leave_all(&mut self) {
        for (id, (ws, sid)) in self.channels.local.drain() {
            nets().close(&ws, &id, sid);
        }
    }

    /// A new session's stream hears every app open on its space again.
    pub(crate) fn net_reopen(&self, ws: &str) {
        let Some(h) = self.sessions.get(ws) else {
            return;
        };
        for (id, _) in self.apps.iter().filter(|(_, a)| a.ws_id == ws) {
            let _ = h.net.send(NetFrame::Open { item_id: id.to_string() });
        }
    }

    /// What an app sent, signed and on its way.
    pub(crate) fn net_out(&mut self, id: Arc<str>, out: Vec<Outgoing>) {
        let Some(ws) = self.apps.get(&id).map(|a| a.ws_id.clone()) else {
            return;
        };
        let Ok(secret) = self.vault.device_secret() else {
            return;
        };
        let device = cryptography::signature::public_key(&secret);
        let route = self.route(&ws);
        for m in out {
            let env = Envelope {
                ws: ws.clone(),
                item: id.to_string(),
                channel: m.channel,
                seq: next_seq(),
                from_device: device,
                body: m.body,
            };
            let sealed = match seal(&env, |b| cryptography::signature::sign(&secret, b)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("net: {id}: {e}");
                    continue;
                }
            };
            match route {
                Route::Session => {
                    if let Some(h) = self.sessions.get(&ws) {
                        let _ = h.net.send(NetFrame::Msg(sealed));
                    }
                }
                Route::Here => {
                    let Some((_, sid)) = self.channels.local.get(&id) else {
                        continue;
                    };
                    match nets().judge(&sealed, *sid, device, &ws, clock::now_ms()) {
                        Ok(_) => nets().relay(&ws, &id, *sid, &sealed),
                        Err(e) => eprintln!("host: {e}"),
                    }
                }
                Route::Socket => {}
            }
        }
    }

    /// One frame from the relay, for the app with its item open. `seq` 0 is this host's own
    /// relay; any other must be the space's current session.
    pub(crate) fn net_frame(&mut self, ws: String, seq: u64, frame: NetFrame) {
        if seq != 0 && self.sessions.get(&ws).is_none_or(|h| h.seq != seq) {
            return;
        }
        let app_of = |item: &str| self.apps.get(item).filter(|a| a.ws_id == ws);
        match frame {
            NetFrame::Here { item_id, attestation } => {
                let Ok(a) = courier::attest::verify_attestation(&attestation) else {
                    eprintln!("net {ws}: a bad attestation for {item_id}");
                    return;
                };
                if let Some(app) = app_of(&item_id) {
                    let peer = NetPeer { id: hex(&a.device), account: a.did };
                    app.call(move |h| h.app.net().here(peer));
                }
            }
            NetFrame::Gone { item_id, device } => {
                if let Some(app) = app_of(&item_id) {
                    let id = hex(&device);
                    app.call(move |h| h.app.net().gone(&id));
                }
            }
            NetFrame::Msg(sealed) => {
                let env = match courier::net::open(&sealed) {
                    Ok(env) if env.ws == ws => env,
                    Ok(_) => return,
                    Err(e) => {
                        eprintln!("net {ws}: {e}");
                        return;
                    }
                };
                if let Some(app) = app_of(&env.item) {
                    let from = hex(&env.from_device);
                    app.call(move |h| h.app.net_deliver(&env.channel, &from, &env.body));
                }
            }
            NetFrame::Open { .. } | NetFrame::Close { .. } | NetFrame::Beat => {}
        }
    }

    /// The space's channel stream ended: nobody else is here until a new one is up.
    pub(crate) fn net_down(&mut self, ws: String, seq: u64) {
        if self.sessions.get(&ws).is_some_and(|h| h.seq != seq) {
            return; // an older session's stream; the current one is up or dialling
        }
        for a in self.apps.values().filter(|a| a.ws_id == ws) {
            a.call(|h| h.app.net().alone());
        }
    }
}

/// A member's channel stream (`courier::session` `NetHello`), on the session's connection:
/// what the shell queued goes out, a `Beat` every [`NET_BEAT_MS`], and every frame from the host
/// comes home as `Msg::NetFrame`. `Msg::NetDown` when it ends, however it ends.
#[allow(clippy::too_many_arguments)]
pub async fn stream(
    conn: Connection,
    attestation: String,
    ws_id: String,
    token: Token,
    rx: UnboundedReceiver<NetFrame>,
    proxy: EventLoopProxy<Msg>,
    gen_: u64,
    seq: u64,
) {
    struct Down(EventLoopProxy<Msg>, u64, String, u64);
    impl Drop for Down {
        fn drop(&mut self) {
            let _ = self.0.send_event(Msg::NetDown(self.1, self.2.clone(), self.3));
        }
    }
    let _down = Down(proxy.clone(), gen_, ws_id.clone(), seq);
    // `OSVAULD_NET_DELAY_MS`: this member's link made slow both ways, for the rollback tests (M5).
    let delay = std::env::var("OSVAULD_NET_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()).map(Duration::from_millis);
    let (late_tx, mut late_rx) = unbounded_channel::<(tokio::time::Instant, NetFrame)>();
    let mut rx = match delay {
        None => rx,
        Some(d) => {
            let (tx, slow_rx) = unbounded_channel();
            tokio::spawn(async move {
                let mut rx = rx;
                let (hold_tx, mut hold_rx) = unbounded_channel::<(tokio::time::Instant, NetFrame)>();
                tokio::spawn(async move {
                    while let Some((at, f)) = hold_rx.recv().await {
                        tokio::time::sleep_until(at).await;
                        if tx.send(f).is_err() {
                            return;
                        }
                    }
                });
                while let Some(f) = rx.recv().await {
                    let _ = hold_tx.send((tokio::time::Instant::now() + d, f));
                }
            });
            slow_rx
        }
    };
    {
        let (proxy, ws_id) = (proxy.clone(), ws_id.clone());
        tokio::spawn(async move {
            while let Some((at, frame)) = late_rx.recv().await {
                tokio::time::sleep_until(at).await;
                let _ = proxy.send_event(Msg::NetFrame(gen_, ws_id.clone(), seq, frame));
            }
        });
    }
    let result: Result<(), String> = async {
        let (mut send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
        let mut frames = FrameBuf::new(recv);
        let hello = Frame::NetHello { attestation, ws_id: ws_id.clone(), token };
        send_frame(&mut send, &hello, FIRST_FRAME).await.map_err(|e| e.to_string())?;
        match frames.recv::<Frame>(MAX_FRAME).await.map_err(|e| e.to_string())? {
            Some(Frame::Welcome { .. }) => {}
            Some(Frame::Refused { reason }) => return Err(reason),
            _ => return Err("the host did not answer the channel hello".into()),
        }
        let mut beat = tokio::time::interval(Duration::from_millis(NET_BEAT_MS));
        loop {
            tokio::select! {
                out = rx.recv() => match out {
                    Some(frame) => send_frame(&mut send, &frame, MAX_FRAME).await.map_err(|e| e.to_string())?,
                    None => return Ok(()),
                },
                _ = beat.tick() => send_frame(&mut send, &NetFrame::Beat, MAX_FRAME).await.map_err(|e| e.to_string())?,
                incoming = frames.recv::<NetFrame>(MAX_FRAME) => match incoming.map_err(|e| e.to_string())? {
                    Some(frame) => {
                        let _ = late_tx.send((tokio::time::Instant::now() + delay.unwrap_or_default(), frame));
                    }
                    None => return Ok(()),
                },
            }
        }
    }
    .await;
    if let Err(e) = result {
        eprintln!("net {ws_id}: {e}");
    }
}
