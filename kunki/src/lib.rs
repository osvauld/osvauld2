//! The sovereign node: its identity, the records it keeps of what it granted and what it
//! holds from others, and later the protocol it serves. The binary beside this is only a
//! boot sequence over it.
//!
//! Everything the node remembers lives in one `vault` account — the same store shell2 uses —
//! so its own key, its tokens, and its revocations are sealed by the same passphrase.
//!
//! Since laptop-host step 7 the host acceptor every serving process runs is here too
//! (`host`, behind a `Sink` the process supplies), with the member-side pieces that need no
//! shell (`session`).

pub mod admin;
pub mod bridge;
mod error;
pub mod host;
pub mod node;
pub mod peer;
pub mod push;
pub mod session;

pub use error::NodeError;
