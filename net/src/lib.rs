//! The device's one iroh endpoint and what every wire over it shares: binding on the device
//! key, length-framed messages with a cap the peer cannot raise, the timeouts and retry
//! ladder, and the pure guard that rate-limits code guessing. Transport only — no accounts,
//! no docs, no authority: `courier` decides, `net` carries ([`docs/design/laptop-host.md`]
//! §2 Transport). A GUI calls in from plain threads through [`Net::block_on`]; the tokio
//! runtime lives in here and never on the UI thread.

pub mod endpoint;
pub mod frames;
pub mod guard;
pub mod wire;

pub use endpoint::{Mode, Net};
pub use iroh::{EndpointAddr, EndpointId};

/// The wire a space's sessions run on (laptop-host step 5). Versioned in the name: a peer on
/// another version fails at connect, not mid-session.
pub const SPACE_ALPN: &[u8] = b"osvauld/space/1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The transport refused or dropped: a bind, dial, stream or read/write failure, as iroh
    /// reports it. One variant, because no caller branches on which quinn error it was.
    #[error("net: {0}")]
    Net(String),
    /// The peer broke the wire's rules: a frame over the cap, bytes that do not decode.
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("timed out")]
    Timeout,
    /// The endpoint or runtime is gone.
    #[error("closed")]
    Closed,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn net(e: impl std::fmt::Display) -> Self {
        Error::Net(e.to_string())
    }
}

#[cfg(test)]
mod tests;
