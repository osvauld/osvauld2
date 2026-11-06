//! Bundled workspace launcher VM. Owns no app instances or documents; commands leave as data.

use crate::{Msg, shell_api};
#[cfg(test)]
use crate::item::ItemsScreenMsg;
use app_host::{LuaApp, LuaMsg};
use runtime::El;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
const VM_LIMITS: app_host::host::VmLimits = app_host::host::VmLimits {
    memory_bytes: 32 * 1024 * 1024,
    interrupts: 20_000,
    time: std::time::Duration::from_millis(10),
    load_time: std::time::Duration::from_millis(100),
};
use vault::{Vault, WorkspaceItem, WorkspaceMeta};

pub(crate) mod context;
pub(crate) mod creation;
mod customization;
mod spaces_api;
pub(crate) mod navigation;
mod rpc;
pub(crate) mod session;
mod source;
mod source_api;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod spaces_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceAction {
    Back,
    Import,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Workspace {
        ws_id: String,
        action: WorkspaceAction,
    },
    OpenItem(shell_api::OpenItem),
    SelectWorkspace(String),
    CreateWorkspace(String),
    EditSource {
        ws_id: String,
        path: String,
        revision: String,
        edits: Vec<app_host::SourceEdit>,
    },
}

pub struct Launcher {
    instance: u64,
    app: LuaApp<Msg>,
    source: Rc<RefCell<source::State>>,
    context: context::Context,
    commands: Rc<RefCell<Vec<Command>>>,
    warning: Option<String>,
}

impl Launcher {
    pub fn new(
        vault: &Vault,
        workspace: WorkspaceMeta,
        items: Vec<WorkspaceItem>,
    ) -> Result<Self, String> {
        Self::new_with_inventory(vault, Rc::new(RefCell::new(shell_api::Snapshot { workspace, items })))
    }

    pub(crate) fn new_with_inventory(vault: &Vault, inventory: session::Inventory) -> Result<Self, String> {
        Self::new_in_context(vault, context::Context::Workspace(inventory))
    }

    pub(crate) fn new_in_context(vault: &Vault, context: context::Context) -> Result<Self, String> {
        let saved = vault.get_entry(source::ENTRY).map_err(|e| e.to_string())?;
        Self::open(context, saved)
    }

    fn open(context: context::Context, saved: Option<Vec<u8>>) -> Result<Self, String> {
        let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let src = source::bundled()?;
        let source = Rc::new(RefCell::new(source::State {
            doc: src.clone(),
            generation: 0,
        }));
        let commands = Rc::new(RefCell::new(Vec::new()));
        let inventory_setup = context.setup(commands.clone());
        let source_setup = match &context {
            context::Context::Workspace(inventory) => Some(source_api::setup(
                source.clone(), inventory.borrow().workspace.id.clone(), commands.clone(),
            )),
            context::Context::Spaces(_) => None,
        };
        let mut app = LuaApp::open_with_entry(
            src,
            Rc::new(|_| Err("launcher documents are not configured".into())),
            std::sync::Arc::new(|| {}),
            context.identity(instance),
            Some(Rc::new(move |vm, context| {
                context.set_limits(vm, VM_LIMITS)?;
                inventory_setup(vm, context.clone())?;
                if let Some(source_setup) = &source_setup { source_setup(vm, context)?; }
                Ok(())
            })),
            context.entry(),
            // Launcher documents are not configured: nothing of its is ever pushed.
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut warning = None;
        if let Some(bytes) = saved {
            match source::import(&bytes)
                .and_then(|src| if matches!(context, context::Context::Spaces(_)) {
                    source::with_spaces(src)
                } else { Ok(src) })
                .and_then(|src| app.stage_source(src.clone()).map(|app| (src, app)))
            {
                Ok((src, candidate)) => {
                    source.borrow_mut().replace(src);
                    app = candidate;
                }
                Err(error) => {
                    warning = Some(format!(
                        "Saved launcher failed; using bundled default: {error}"
                    ));
                }
            }
        }
        Ok(Self {
            instance,
            app,
            source,
            context,
            commands,
            warning,
        })
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn view(&self) -> El<Msg> {
        self.app.view()
    }

    pub fn refresh(&self, items: Vec<WorkspaceItem>) {
        if let context::Context::Workspace(inventory) = &self.context {
            inventory.borrow_mut().items = items;
        }
    }

    pub fn update(&mut self, instance: u64, msg: LuaMsg) -> Vec<Command> {
        if instance != self.instance {
            return Vec::new();
        }
        self.app.update(msg);
        self.commands.borrow_mut().drain(..).collect()
    }
}
