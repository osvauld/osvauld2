//! One endpoint per process, bound on the install's device key, and the runtime it runs on.
//! `Net` is what shell2 and kunki hold; its async surface is for the wires built on it
//! (`Net::handle`), its blocking one (`Net::block_on`) for the plain threads a GUI already has.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, QuicTransportConfig, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use tokio::runtime::{Handle, Runtime};

use crate::{Error, Result, wire};

/// Where this endpoint may be reached from. Read from `OSVAULD_NET`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// n0's relays and address lookup: reachable from the internet. The default.
    Auto,
    /// No relay, no lookup: reachable by its direct addresses only — the same machine or
    /// LAN. What the e2e tests use; also "same wifi, no internet".
    Local,
    /// Every packet through the relay: the path a dial takes when hole punching fails.
    RelayOnly,
}

impl Mode {
    pub fn from_env() -> Mode {
        match std::env::var("OSVAULD_NET").as_deref() {
            Ok("local") => Mode::Local,
            Ok("relay-only") => Mode::RelayOnly,
            Ok("") | Ok("auto") | Err(_) => Mode::Auto,
            Ok(other) => {
                eprintln!("net: OSVAULD_NET={other:?} is not auto|local|relay-only; using auto");
                Mode::Auto
            }
        }
    }
}

pub struct Net {
    rt: Runtime,
    ep: Endpoint,
    mode: Mode,
}

impl Net {
    /// Binds on `secret` — the device key, so the endpoint id *is* the device — serving
    /// `alpns`. Blocks for the bind; makes the runtime.
    pub fn bind(secret: &[u8; 32], mode: Mode, alpns: Vec<Vec<u8>>) -> Result<Net> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("osvauld-net")
            .enable_all()
            .build()
            .map_err(Error::net)?;
        let ep = rt.block_on(bind(secret, mode, alpns))?;
        Ok(Net { rt, ep, mode })
    }

    pub fn id(&self) -> EndpointId {
        self.ep.id()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.ep
    }

    pub fn handle(&self) -> &Handle {
        self.rt.handle()
    }

    /// Where to tell a peer to dial: the id, the relay once there is one, and in `Local`
    /// mode the direct addresses (they are all a local peer has). Waits up to `wait` for
    /// the endpoint to learn them; a bare id comes back if it never does.
    pub fn addr(&self, wait: Duration) -> EndpointAddr {
        let started = Instant::now();
        loop {
            let addr = self.ep.addr();
            let ready = match self.mode {
                Mode::Local => addr.ip_addrs().next().is_some(),
                Mode::Auto | Mode::RelayOnly => addr.relay_urls().next().is_some(),
            };
            if ready || started.elapsed() >= wait {
                return addr;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Runs `fut` on the runtime and waits for it, from any thread that is not one of the
    /// runtime's own. A channel rather than `Handle::block_on` so a caller on some other
    /// runtime's thread (a future test harness) is not refused.
    pub fn block_on<T: Send + 'static>(
        &self,
        fut: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = mpsc::channel();
        self.rt.spawn(async move {
            let _ = tx.send(fut.await);
        });
        rx.recv().map_err(|_| Error::Closed)
    }

    /// Dials `addr` for `alpn`, giving up after [`wire::DIAL_TIMEOUT`].
    pub async fn dial(&self, addr: EndpointAddr, alpn: &[u8]) -> Result<Connection> {
        dial(&self.ep, addr, alpn, wire::DIAL_TIMEOUT).await
    }

    /// Closes the endpoint (peers see a clean close) and stops the runtime.
    pub fn close(self) {
        let ep = self.ep.clone();
        let _ = self.block_on(async move { ep.close().await });
        self.rt.shutdown_timeout(Duration::from_secs(2));
    }
}

/// The bind itself, for a caller that already has a runtime (tests, a future async host).
pub async fn bind(secret: &[u8; 32], mode: Mode, alpns: Vec<Vec<u8>>) -> Result<Endpoint> {
    let transport = QuicTransportConfig::builder()
        // Keeps NAT bindings alive between frames; the relay link has its own pings.
        .keep_alive_interval(Duration::from_secs(5))
        .max_idle_timeout(Some(Duration::from_secs(20).try_into().expect("small timeout")))
        .build();
    let key = SecretKey::from_bytes(secret);
    let ep = match mode {
        Mode::Local => Endpoint::builder(presets::Minimal),
        Mode::Auto => Endpoint::builder(presets::N0),
        // No UDP of our own: every packet goes through the relay.
        Mode::RelayOnly => Endpoint::builder(presets::N0).clear_ip_transports(),
    }
    .secret_key(key)
    .alpns(alpns)
    .transport_config(transport)
    .bind()
    .await
    .map_err(Error::net)?;
    Ok(ep)
}

pub async fn dial(
    ep: &Endpoint,
    addr: EndpointAddr,
    alpn: &[u8],
    timeout: Duration,
) -> Result<Connection> {
    match tokio::time::timeout(timeout, ep.connect(addr, alpn)).await {
        Ok(conn) => conn.map_err(Error::net),
        Err(_) => Err(Error::Timeout),
    }
}
