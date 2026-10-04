use super::{Command, source::State};
use app_host::{SourceEdit, host::VmSetup, read_source_file_versioned};
use mlua::{Error, Table, Value};
use std::{cell::RefCell, rc::Rc};

pub(super) fn setup(
    source: Rc<RefCell<State>>,
    ws_id: String,
    commands: Rc<RefCell<Vec<Command>>>,
) -> VmSetup {
    Rc::new(move |vm, context| {
        // Trusted installers extend the frozen capability before any app source can run.
        let api: Table = vm.globals().get("shell")?;
        api.set_readonly(false);
        let source = source.clone();
        api.set(
            "read_launcher_file",
            vm.create_function(move |vm, path: String| {
                crate::valid_source_path(&path).map_err(Error::runtime)?;
                let source = source.borrow();
                let file =
                    read_source_file_versioned(&source.doc, &path).map_err(Error::runtime)?;
                let result = vm.create_table()?;
                result.set("content", file.content)?;
                result.set("revision", file.revision)?;
                result.set("generation", source.generation)?;
                Ok(result)
            })?,
        )?;
        let ws_id = ws_id.clone();
        let commands = commands.clone();
        api.set(
            "edit_launcher_file",
            vm.create_function(move |_, (path, revision, edits): (String, String, Table)| {
                context.require_handler()?;
                crate::valid_source_path(&path).map_err(Error::runtime)?;
                let edits = replacements(edits)?;
                commands.borrow_mut().push(Command::EditSource {
                    ws_id: ws_id.clone(),
                    path,
                    revision,
                    edits,
                });
                Ok(())
            })?,
        )?;
        api.set_readonly(true);
        Ok(())
    })
}

fn replacements(list: Table) -> mlua::Result<Vec<SourceEdit>> {
    let len = list.raw_len();
    if len > 128 {
        return Err(Error::runtime("source edit batch is too large"));
    }
    for pair in list.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        if !matches!(key, Value::Integer(i) if i > 0 && (i as usize) <= len) {
            return Err(Error::runtime("source edits must be a contiguous list"));
        }
    }
    let mut edits = Vec::with_capacity(len);
    let mut bytes = 0usize;
    for i in 1..=len {
        let entry: Table = list.raw_get(i)?;
        for pair in entry.clone().pairs::<String, Value>() {
            let (key, _) = pair?;
            if key != "old_text" && key != "new_text" {
                return Err(Error::runtime(format!("unknown source edit field: {key}")));
            }
        }
        let edit = SourceEdit {
            old_text: entry.get("old_text")?,
            new_text: entry.get("new_text")?,
        };
        bytes = bytes
            .saturating_add(edit.old_text.len())
            .saturating_add(edit.new_text.len());
        if bytes > 1024 * 1024 {
            return Err(Error::runtime("source edit batch is too large"));
        }
        edits.push(edit);
    }
    Ok(edits)
}

#[cfg(test)]
mod tests;
