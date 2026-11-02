//! The shell's wall clock for anything that expires — share codes, tokens. Real time plus an
//! offset the bridge's `Advance` moves (offscreen, where the runtime's own clock is virtual
//! too), so a test can age a link past its week without waiting one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static OFFSET_MS: AtomicU64 = AtomicU64::new(0);

pub fn now_ms() -> u64 {
    let real = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64;
    real + OFFSET_MS.load(Ordering::Relaxed)
}

pub fn now_secs() -> u64 {
    now_ms() / 1000
}

/// Moves this process's clock forward by `secs`, for good.
pub fn advance(secs: f64) {
    if secs > 0.0 {
        OFFSET_MS.fetch_add((secs * 1000.0) as u64, Ordering::Relaxed);
    }
}
