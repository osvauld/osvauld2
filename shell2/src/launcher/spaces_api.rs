//! Account-list capability: copied metadata and handler-only select requests; Rust revalidates.
use super::{Command, context::Spaces};
use app_host::host::VmSetup;
use mlua::Error;
use std::{cell::RefCell, rc::Rc};

pub(super) fn setup(spaces: Spaces, commands: Rc<RefCell<Vec<Command>>>) -> VmSetup {
    Rc::new(move |vm, context| {
        let api = vm.create_table()?;
        let inventory = spaces.clone();
        api.set(
            "snapshot",
            vm.create_function(move |vm, ()| {
                let snapshot = vm.create_table()?;
                let rows = vm.create_table()?;
                for workspace in &inventory.borrow().rows {
                    let row = vm.create_table()?;
                    row.set("id", workspace.id.as_str())?;
                    row.set("name", workspace.name.as_str())?;
                    rows.raw_push(row)?;
                }
                snapshot.set("workspaces", rows)?;
                let state = &inventory.borrow().creation;
                let creation = vm.create_table()?;
                creation.set("pending", state.pending.is_some())?;
                creation.set("generation", state.generation)?;
                creation.set("error", state.error.clone())?;
                snapshot.set("creation", creation)?;
                Ok(snapshot)
            })?,
        )?;
        let queue = commands.clone();
        let create_context = context.clone();
        api.set(
            "create_workspace",
            vm.create_function(move |_, value: String| {
                create_context.require_handler()?;
                let name = super::creation::name(&value).map_err(Error::runtime)?;
                queue.borrow_mut().push(Command::CreateWorkspace(name));
                Ok(())
            })?,
        )?;
        let inventory = spaces.clone();
        let commands = commands.clone();
        api.set(
            "select_workspace",
            vm.create_function(move |_, id: String| {
                context.require_handler()?;
                if !inventory
                    .borrow()
                    .rows
                    .iter()
                    .any(|workspace| workspace.id == id)
                {
                    return Err(Error::runtime("workspace is not in this account"));
                }
                commands.borrow_mut().push(Command::SelectWorkspace(id));
                Ok(())
            })?,
        )?;
        api.set_readonly(true);
        vm.globals().set("shell", api)
    })
}
