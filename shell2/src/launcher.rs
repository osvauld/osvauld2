//! Bundled workspace launcher VM. Owns no app instances or documents; commands leave as data.

use crate::{Msg, item::ItemsScreenMsg, shell_api};
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

mod customization;
pub(crate) mod navigation;
mod rpc;
mod source;
mod source_api;
#[cfg(test)]
mod tests;

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
    inventory: Rc<RefCell<shell_api::Snapshot>>,
    commands: Rc<RefCell<Vec<Command>>>,
    warning: Option<String>,
}

impl Launcher {
    pub fn new(
        vault: &Vault,
        workspace: WorkspaceMeta,
        items: Vec<WorkspaceItem>,
    ) -> Result<Self, String> {
        let saved = vault.get_entry(source::ENTRY).map_err(|e| e.to_string())?;
        Self::open(workspace, items, saved)
    }

    fn open(
        workspace: WorkspaceMeta,
        items: Vec<WorkspaceItem>,
        saved: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let src = source::bundled()?;
        let source = Rc::new(RefCell::new(source::State {
            doc: src.clone(),
            generation: 0,
        }));
        let ws_id = workspace.id.clone();
        let inventory = Rc::new(RefCell::new(shell_api::Snapshot { workspace, items }));
        let commands = Rc::new(RefCell::new(Vec::new()));
        let queue = commands.clone();
        let inventory_setup = shell_api::setup(
            inventory.clone(),
            Rc::new(move |item| queue.borrow_mut().push(Command::OpenItem(item))),
        );
        let source_setup = source_api::setup(source.clone(), ws_id.clone(), commands.clone());
        let navigation_setup = navigation::setup(ws_id.clone(), commands.clone());
        let mut app = LuaApp::open_with_setup(
            src,
            Rc::new(|_| Err("launcher documents are not configured".into())),
            std::sync::Arc::new(|| {}),
            Rc::new(move |msg| Msg::Items(ItemsScreenMsg::Launcher(ws_id.clone(), instance, msg))),
            Some(Rc::new(move |vm, context| {
                context.set_limits(vm, VM_LIMITS)?;
                inventory_setup(vm, context.clone())?;
                navigation_setup(vm, context.clone())?;
                source_setup(vm, context)
            })),
        )
        .map_err(|e| e.to_string())?;
        let mut warning = None;
        if let Some(bytes) = saved {
            match source::import(&bytes)
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
            inventory,
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
        self.inventory.borrow_mut().items = items;
    }

    pub fn update(&mut self, instance: u64, msg: LuaMsg) -> Vec<Command> {
        if instance != self.instance {
            return Vec::new();
        }
        self.app.update(msg);
        self.commands.borrow_mut().drain(..).collect()
    }
}
