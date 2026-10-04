//! Launcher-only Lua capability: copied workspace inventory and a scoped open-item command.
//! Rust's receiver must revalidate account/workspace access when executing the command.

use app_host::host::VmSetup;
use std::{cell::RefCell, rc::Rc};
use vault::{WorkspaceItem, WorkspaceMeta};

pub struct Snapshot {
    pub workspace: WorkspaceMeta,
    pub items: Vec<WorkspaceItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpenItem {
    pub ws_id: String,
    pub item_id: String,
}

pub fn setup(snapshot: Rc<RefCell<Snapshot>>, send: Rc<dyn Fn(OpenItem)>) -> VmSetup {
    Rc::new(move |vm, context| {
        let api = vm.create_table()?;
        let inventory = snapshot.clone();
        api.set(
            "snapshot",
            vm.create_function(move |vm, ()| {
                let snapshot = inventory.borrow();
                let out = vm.create_table()?;
                let workspace = vm.create_table()?;
                workspace.set("id", snapshot.workspace.id.as_str())?;
                workspace.set("name", snapshot.workspace.name.as_str())?;
                out.set("workspace", workspace)?;
                let items = vm.create_table()?;
                for item in snapshot
                    .items
                    .iter()
                    .filter(|i| i.ws_id == snapshot.workspace.id)
                {
                    let row = vm.create_table()?;
                    row.set("id", item.id.as_str())?;
                    row.set("name", item.name.as_str())?;
                    row.set("kind", item.kind.as_str())?;
                    items.raw_push(row)?;
                }
                out.set("items", items)?;
                Ok(out)
            })?,
        )?;
        let inventory = snapshot.clone();
        let send = send.clone();
        api.set(
            "open_item",
            vm.create_function(move |_, item_id: String| {
                context.require_handler()?;
                let snapshot = inventory.borrow();
                if !snapshot
                    .items
                    .iter()
                    .any(|i| i.id == item_id && i.ws_id == snapshot.workspace.id)
                {
                    return Err(mlua::Error::runtime("item is not in this workspace"));
                }
                let command = OpenItem {
                    ws_id: snapshot.workspace.id.clone(),
                    item_id,
                };
                drop(snapshot);
                send(command);
                Ok(())
            })?,
        )?;
        api.set_readonly(true);
        vm.globals().set("shell", api)
    })
}

#[cfg(test)]
mod tests;
