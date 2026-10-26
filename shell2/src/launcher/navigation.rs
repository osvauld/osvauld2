use super::{Command, WorkspaceAction};
use app_host::host::VmSetup;
use std::{cell::RefCell, rc::Rc};
use vault::Vault;

pub(super) fn setup(ws_id: String, commands: Rc<RefCell<Vec<Command>>>) -> VmSetup {
    Rc::new(move |vm, context| {
        let api: mlua::Table = vm.globals().get("shell")?;
        api.set_readonly(false);
        for (name, action) in [
            ("back", WorkspaceAction::Back),
            ("import_item", WorkspaceAction::Import),
        ] {
            let context = context.clone();
            let commands = commands.clone();
            let ws_id = ws_id.clone();
            api.set(
                name,
                vm.create_function(move |_, ()| {
                    context.require_handler()?;
                    commands.borrow_mut().push(Command::Workspace {
                        ws_id: ws_id.clone(),
                        action,
                    });
                    Ok(())
                })?,
            )?;
        }
        api.set_readonly(true);
        Ok(())
    })
}

pub(crate) fn validate(
    vault: &Vault,
    home_ws: Option<&str>,
    focused: usize,
    ws_id: &str,
) -> Result<(), String> {
    let workspaces = vault.workspaces().map_err(|e| e.to_string())?;
    if focused != 0 || home_ws != Some(ws_id) {
        return Err("launcher workspace changed or Home is not focused".into());
    }
    if !workspaces.iter().any(|ws| ws.id == ws_id) {
        return Err("workspace is not in this account".into());
    }
    Ok(())
}
