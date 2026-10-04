//! The per-VM guard (`docs/design/app-threads.md` step 1): an interrupt count, a CPU-time
//! limit, and a memory cap. The count catches a pure-Lua loop deterministically; the clock
//! catches a loop whose time goes into native calls, which fire few interrupts.
//!
//! The clock is the thread's own CPU time, not the wall: a VM waiting for a core while a build
//! runs, or asleep in a native call, has done no work. Each app has its own thread, so its
//! thread's time is its own. A call that never returns is the shell's stuck watchdog's.
//!
//! The clock only runs while *armed* — from an entry into app Lua (`view`, `update`, module
//! load, a test, an index call) until it returns. Unarmed, a stale start would kill the next
//! entry the moment it began.

use mlua::Lua;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_FIRES: u64 = 1_000_000;
/// Read the clock every this many interrupts: reading it on each would cost ~25 ms at the
/// count ceiling.
const CLOCK_EVERY: u64 = 16;

/// What one VM may spend. Apps take `Policy::app()`; a host with stricter needs (the launcher)
/// passes its own.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// CPU milliseconds per entry.
    pub cpu_ms: u64,
    pub memory_mb: u64,
}

impl Policy {
    /// 1 s of CPU and 512 MB; `OSVAULD_APP_BUDGET_MS` / `OSVAULD_APP_MEMORY_MB` override — the
    /// e2e tests set them small so a breach is quick and cheap.
    pub fn app() -> Self {
        Self {
            cpu_ms: knob("OSVAULD_APP_BUDGET_MS", 1_000),
            memory_mb: knob("OSVAULD_APP_MEMORY_MB", 512),
        }
    }
}

pub struct Budget {
    fires: AtomicU64,
    /// CPU nanoseconds at the arm, plus one; 0 = unarmed.
    armed_at: AtomicU64,
    cpu_ms: u64,
}

impl Budget {
    /// Interrupts spent since the last arm — what `cost_curve` reports.
    pub fn spent(&self) -> u64 {
        self.fires.load(Ordering::Relaxed)
    }

    /// Refill the count and start the clock; it stops when the guard drops.
    pub fn arm(self: &Arc<Self>) -> Armed {
        self.fires.store(0, Ordering::Relaxed);
        self.armed_at.store(cpu_now(), Ordering::Relaxed);
        Armed(self.clone())
    }

    /// Stop the clock until the guard drops — for a native call whose time is not this VM's,
    /// such as a test stepping the app it tests. The interrupt count still runs.
    pub fn pause(self: &Arc<Self>) -> Paused {
        Paused(self.clone(), cpu_now())
    }

    fn check(&self) -> mlua::Result<mlua::VmState> {
        let n = self.fires.fetch_add(1, Ordering::Relaxed);
        if n > MAX_FIRES {
            return Err(mlua::Error::runtime("interrupt budget exceeded"));
        }
        if n % CLOCK_EVERY == 0 {
            let at = self.armed_at.load(Ordering::Relaxed);
            if at != 0 && cpu_now().saturating_sub(at) > self.cpu_ms * 1_000_000 {
                return Err(mlua::Error::runtime(format!(
                    "CPU-time budget exceeded: ran over {} ms without returning",
                    self.cpu_ms
                )));
            }
        }
        Ok(mlua::VmState::Continue)
    }
}

/// This thread's CPU time in nanoseconds, plus one so 0 can mean unarmed. A VM runs on one
/// thread, so arm, check and pause all read the same clock.
#[cfg(unix)]
fn cpu_now() -> u64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `t` is a valid out-pointer; this clock id exists on Linux, Android and macOS.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64 + 1
}

/// No thread clock wired up here yet: the wall, as before.
#[cfg(not(unix))]
fn cpu_now() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

pub struct Armed(Arc<Budget>);

impl Drop for Armed {
    fn drop(&mut self) {
        self.0.armed_at.store(0, Ordering::Relaxed);
    }
}

pub struct Paused(Arc<Budget>, u64);

impl Drop for Paused {
    /// Move the arm forward by the pause, so the clock resumes where it stopped.
    fn drop(&mut self) {
        let away = cpu_now().saturating_sub(self.1);
        let _ = self
            .0
            .armed_at
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |at| {
                (at != 0).then_some(at + away)
            });
    }
}

fn knob(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn install(vm: &Lua, policy: Policy) -> mlua::Result<Arc<Budget>> {
    let budget = Arc::new(Budget {
        fires: AtomicU64::new(0),
        armed_at: AtomicU64::new(0),
        cpu_ms: policy.cpu_ms,
    });
    let b = budget.clone();
    vm.set_interrupt(move |_| b.check());
    vm.set_memory_limit((policy.memory_mb * 1024 * 1024) as usize)?;
    Ok(budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn spin(ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            std::hint::spin_loop();
        }
    }

    fn policy(cpu_ms: u64) -> Policy {
        Policy {
            cpu_ms,
            memory_mb: 64,
        }
    }

    /// A VM with `work()` (5 ms on the CPU) and `nap()` (5 ms asleep): native calls, ~3
    /// interrupts each, so the count alone would let 1000 of them run.
    fn native_vm(cpu_ms: u64) -> (Lua, Arc<Budget>) {
        let vm = Lua::new();
        let work = vm.create_function(|_, ()| Ok(spin(5))).unwrap();
        let nap = vm
            .create_function(|_, ()| Ok(std::thread::sleep(Duration::from_millis(5))))
            .unwrap();
        vm.globals().set("work", work).unwrap();
        vm.globals().set("nap", nap).unwrap();
        let budget = install(&vm, policy(cpu_ms)).unwrap();
        (vm, budget)
    }

    #[test]
    fn armed_clock_stops_a_native_heavy_loop() {
        let (vm, budget) = native_vm(50);
        let _armed = budget.arm();
        let t0 = Instant::now();
        let err = vm
            .load("for _ = 1, 1000 do work() end")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(err.contains("CPU-time budget"), "{err}");
        assert!(t0.elapsed() < Duration::from_millis(500), "{:?}", t0.elapsed());
    }

    #[test]
    fn a_stale_arm_does_not_kill_the_next_entry() {
        let (vm, budget) = native_vm(50);
        drop(budget.arm());
        spin(80);
        // Unarmed: nothing is timing this call, so a short loop well past the old arm finishes.
        vm.load("for _ = 1, 20 do work() end").exec().unwrap();
    }

    /// Time the thread spends off the CPU — asleep here, or waiting for a core while a build
    /// runs — is not the app's work and spends nothing.
    #[test]
    fn time_off_the_cpu_is_not_spent() {
        let (vm, budget) = native_vm(50);
        let _armed = budget.arm();
        vm.load("for _ = 1, 20 do nap() end").exec().unwrap();
    }

    /// A Lua test drives the app through native calls (`t.step`); those frames are the app's
    /// time, not the test's, so they must not spend the test's clock.
    #[test]
    fn a_paused_call_does_not_spend_the_clock() {
        let (vm, budget) = native_vm(50);
        let b = budget.clone();
        let drive = vm
            .create_function(move |_, ()| {
                let _paused = b.pause();
                Ok(spin(30))
            })
            .unwrap();
        vm.globals().set("drive", drive).unwrap();
        let _armed = budget.arm();
        vm.load("for _ = 1, 5 do drive() end").exec().unwrap();
    }

    #[test]
    fn memory_cap_is_an_error_not_an_abort() {
        let vm = Lua::new();
        install(&vm, Policy { cpu_ms: 1_000, memory_mb: 8 }).unwrap();
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
