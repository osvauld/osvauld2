//! Opt-in bindings supplied by a trusted Rust host, rebuilt for each sandboxed VM.
//! Command bindings must check the VM-local handler context before producing side effects.

use mlua::Lua;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

pub type VmSetup = Rc<dyn Fn(&Lua, HostContext) -> mlua::Result<()>>;

/// Trusted host policy; counts Luau interrupts, not instructions. CPU limits are cooperative;
/// the heap limit covers Lua allocations, not Rust resources referenced by userdata.
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
    budget: Option<Arc<crate::Budget>>,
}

impl HostContext {
    pub(crate) fn new(budget: Arc<crate::Budget>) -> Self {
        Self {
            handling: Rc::new(Cell::new(false)),
            budget: Some(budget),
        }
    }

    pub fn set_limits(&self, vm: &Lua, limits: VmLimits) -> mlua::Result<()> {
        self.budget
            .as_ref()
            .ok_or_else(|| mlua::Error::runtime("host context is not attached to a VM"))?
            .configure(
                vm,
                limits.memory_bytes,
                limits.interrupts,
                limits.time,
                limits.load_time,
            )
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
