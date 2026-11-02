//! The numbers every wire shares. Lifted from Tinline (`node-runtime/src/wire.rs`), where
//! they have carried calls and chats on phones since 2026-10.

use std::time::Duration;

/// How long a dial may take before the attempt counts as failed.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// How long one side waits for the other's reply to a frame it sent.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
/// Pause before each further attempt, in seconds; the last repeats.
pub const RETRY_SECS: [u64; 5] = [2, 5, 15, 30, 60];
/// A session with no traffic either way for this long closes; whoever needs the other
/// redials. Peers do not hold connections open.
pub const IDLE_CLOSE: Duration = Duration::from_secs(60);

/// The pause before attempt `n` (0 = the first retry).
pub fn retry_after(n: usize) -> Duration {
    Duration::from_secs(RETRY_SECS[n.min(RETRY_SECS.len() - 1)])
}

/// [`IDLE_CLOSE`], or `OSVAULD_IDLE_CLOSE_MS` — a test knob, so an offline scenario does not
/// wait a minute for a session to notice.
pub fn idle_close() -> Duration {
    std::env::var("OSVAULD_IDLE_CLOSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(IDLE_CLOSE)
}

/// How often a session checks whether it has been idle long enough to close.
pub fn idle_check() -> Duration {
    (idle_close() / 6).clamp(Duration::from_millis(50), Duration::from_secs(10))
}
