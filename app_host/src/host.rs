//! Opt-in bindings supplied by a trusted Rust host, rebuilt for each sandboxed VM.
//! Command bindings must check the VM-local handler context before producing side effects.

use mlua::Lua;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub type VmSetup = Rc<dyn Fn(&Lua, HostContext) -> mlua::Result<()>>;

/// Trusted host policy; counts Luau interrupts, not instructions. The deadline is cooperative
/// and the heap limit covers Lua allocations, not Rust resources referenced by userdata.
#[derive(Clone, Copy)]
pub struct VmLimits {
    pub memory_bytes: usize,
    pub interrupts: u64,
    pub time: Duration,
    pub load_time: Duration,
}

#[derive(Clone, Default)]
pub struct HostContext {
    handling: Rc<Cell<bool>>,
    fires: Arc<AtomicU64>,
    time: Rc<Cell<Option<Duration>>>,
    load_time: Rc<Cell<Option<Duration>>>,
    deadline: Rc<Cell<Option<(Instant, Duration)>>>,
}

impl HostContext {
    pub(crate) fn new(fires: Arc<AtomicU64>) -> Self {
        Self {
            fires,
            ..Self::default()
        }
    }

    pub fn set_limits(&self, vm: &Lua, limits: VmLimits) -> mlua::Result<()> {
        if limits.memory_bytes == 0 || limits.interrupts == 0 {
            return Err(mlua::Error::runtime("VM limits must be nonzero"));
        }
        vm.set_memory_limit(limits.memory_bytes)?;
        self.time.set(Some(limits.time));
        self.load_time.set(Some(limits.load_time));
        let fires = self.fires.clone();
        let deadline = self.deadline.clone();
        vm.set_interrupt(move |_| {
            if deadline
                .get()
                .is_some_and(|(start, limit)| start.elapsed() >= limit)
            {
                return Err(mlua::Error::runtime("execution deadline exceeded"));
            }
            if fires.fetch_add(1, Ordering::Relaxed) >= limits.interrupts {
                return Err(mlua::Error::runtime("interrupt budget exceeded"));
            }
            Ok(mlua::VmState::Continue)
        });
        self.reset_budget();
        Ok(())
    }

    pub(crate) fn reset_budget(&self) {
        self.fires.store(0, Ordering::Relaxed);
        self.deadline
            .set(self.time.get().map(|limit| (Instant::now(), limit)));
    }

    pub(crate) fn reset_load_budget(&self) {
        self.fires.store(0, Ordering::Relaxed);
        self.deadline
            .set(self.load_time.get().map(|limit| (Instant::now(), limit)));
    }

    pub fn require_handler(&self) -> mlua::Result<()> {
        if self.handling.get() {
            Ok(())
        } else {
            Err(mlua::Error::runtime(
                "host commands are only allowed in handlers",
            ))
        }
    }

    pub(crate) fn enter(&self) -> HandlerScope {
        HandlerScope {
            context: self.clone(),
            previous: self.handling.replace(true),
        }
    }
}

pub(crate) struct HandlerScope {
    context: HostContext,
    previous: bool,
}

impl Drop for HandlerScope {
    fn drop(&mut self) {
        self.context.handling.set(self.previous);
    }
}
