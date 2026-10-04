//! The per-VM guard (`docs/design/app-threads.md` step 1): an interrupt count, a wall-clock
//! limit, and a memory cap. The count catches a pure-Lua loop deterministically; the clock
//! catches a loop whose time goes into native calls, which fire few interrupts.
//!
//! The clock only runs while *armed* — from an entry into app Lua (`view`, `update`, module
//! load, a test, an index call) until it returns. Unarmed, a stale start would kill the next
//! entry the moment it began.

use mlua::Lua;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const MAX_FIRES: u64 = 1_000_000;
/// Read the clock every this many interrupts: `Instant::now` on each would cost ~25 ms at the
/// count ceiling.
const CLOCK_EVERY: u64 = 16;
const WALL_MS: u64 = 1_000;
const MEMORY_MB: u64 = 512;

pub struct Budget {
    fires: AtomicU64,
    /// Nanoseconds since `base`, plus one; 0 = unarmed.
    armed_at: AtomicU64,
    base: Instant,
    wall_ms: u64,
}

impl Budget {
    /// Interrupts spent since the last arm — what `cost_curve` reports.
    pub fn spent(&self) -> u64 {
        self.fires.load(Ordering::Relaxed)
    }

    /// Refill the count and start the clock; it stops when the guard drops.
    pub fn arm(self: &Arc<Self>) -> Armed {
        self.fires.store(0, Ordering::Relaxed);
        self.armed_at.store(self.now(), Ordering::Relaxed);
        Armed(self.clone())
    }

    fn now(&self) -> u64 {
        self.base.elapsed().as_nanos() as u64 + 1
    }

    fn check(&self) -> mlua::Result<mlua::VmState> {
        let n = self.fires.fetch_add(1, Ordering::Relaxed);
        if n > MAX_FIRES {
            return Err(mlua::Error::runtime("interrupt budget exceeded"));
        }
        if n % CLOCK_EVERY == 0 {
            let at = self.armed_at.load(Ordering::Relaxed);
            if at != 0 && self.now() - at > self.wall_ms * 1_000_000 {
                return Err(mlua::Error::runtime(format!(
                    "wall-time budget exceeded: ran over {} ms without returning",
                    self.wall_ms
                )));
            }
        }
        Ok(mlua::VmState::Continue)
    }
}

pub struct Armed(Arc<Budget>);

impl Drop for Armed {
    fn drop(&mut self) {
        self.0.armed_at.store(0, Ordering::Relaxed);
    }
}

/// `OSVAULD_APP_BUDGET_MS` / `OSVAULD_APP_MEMORY_MB` override the defaults — the e2e tests set
/// them small so a breach is quick and cheap.
fn knob(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn install(vm: &Lua) -> mlua::Result<Arc<Budget>> {
    install_with(
        vm,
        knob("OSVAULD_APP_BUDGET_MS", WALL_MS),
        knob("OSVAULD_APP_MEMORY_MB", MEMORY_MB),
    )
}

fn install_with(vm: &Lua, wall_ms: u64, memory_mb: u64) -> mlua::Result<Arc<Budget>> {
    let budget = Arc::new(Budget {
        fires: AtomicU64::new(0),
        armed_at: AtomicU64::new(0),
        base: Instant::now(),
        wall_ms,
    });
    let b = budget.clone();
    vm.set_interrupt(move |_| b.check());
    vm.set_memory_limit((memory_mb * 1024 * 1024) as usize)?;
    Ok(budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A loop whose time is all in a native call: ~3 interrupts per 5 ms, so the count alone
    /// would let it run 1000 × 5 ms.
    fn native_loop(wall_ms: u64) -> (Lua, Arc<Budget>, mlua::Function) {
        let vm = Lua::new();
        let nap = vm
            .create_function(|_, ()| Ok(std::thread::sleep(Duration::from_millis(5))))
            .unwrap();
        vm.globals().set("nap", nap).unwrap();
        let budget = install_with(&vm, wall_ms, 64).unwrap();
        let f = vm
            .load("return function() for _ = 1, 1000 do nap() end end")
            .eval()
            .unwrap();
        (vm, budget, f)
    }

    #[test]
    fn armed_clock_stops_a_native_heavy_loop() {
        let (_vm, budget, f) = native_loop(50);
        let _armed = budget.arm();
        let t0 = Instant::now();
        let err = f.call::<()>(()).unwrap_err().to_string();
        assert!(err.contains("wall-time budget"), "{err}");
        assert!(t0.elapsed() < Duration::from_millis(500), "{:?}", t0.elapsed());
    }

    #[test]
    fn a_stale_arm_does_not_kill_the_next_entry() {
        let (vm, budget, _) = native_loop(50);
        drop(budget.arm());
        std::thread::sleep(Duration::from_millis(80));
        // Unarmed: nothing is timing this call, so a short loop well past the old arm finishes.
        vm.load("for _ = 1, 20 do nap() end").exec().unwrap();
    }

    #[test]
    fn memory_cap_is_an_error_not_an_abort() {
        let vm = Lua::new();
        install_with(&vm, 1_000, 8).unwrap();
        let err = vm
            .load("local t = {} for i = 1, 64 do t[i] = string.rep('x', 1024 * 1024) .. i end")
            .exec()
            .unwrap_err();
        assert!(matches!(err, mlua::Error::MemoryError(_)), "{err}");
        // The VM is still usable once the hoard is gone.
        vm.load("collectgarbage()").exec().ok();
        vm.load("local s = string.rep('y', 1024)").exec().unwrap();
    }
}
