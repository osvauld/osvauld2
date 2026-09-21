//! The sovereign node: its identity, the records it keeps of what it granted, and later the
//! protocol it serves. The binary beside this is only a boot sequence over it.
//!
//! Everything the node remembers lives in one `vault` account — the same store shell2 uses —
//! so its own key, its tokens, and its revocations are sealed by the same passphrase.

pub mod admin;
mod error;
pub mod node;

pub use error::NodeError;
