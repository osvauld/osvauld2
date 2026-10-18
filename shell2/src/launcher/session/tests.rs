use super::*;
use crate::launcher::tests::fresh;
use runtime::Action;

fn inventory(ws: vault::WorkspaceMeta) -> Inventory {
    std::rc::Rc::new(std::cell::RefCell::new(crate::shell_api::Snapshot {
        workspace: ws,
        items: vec![],
    }))
}

#[test]
fn account_list_selects_only_known_workspaces_and_rejects_trial_commands() {
    let (vault, _tmp, ws) = fresh();
    let spaces = Rc::new(RefCell::new(crate::launcher::context::List {
        rows: vec![ws.clone()],
        ..Default::default()
    }));
    let mut session = Session::default();
    session.bind_context(&vault, Some(Context::Spaces(spaces.clone())));
    let launcher = session.get_mut().unwrap();
    let event = launcher
        .view()
        .trigger(&format!("launcher:space:{}", ws.id), Action::Click)
        .unwrap();
    let crate::Msg::Launcher(instance, message) = event else {
        panic!("unscoped list event")
    };
    assert_eq!(
        launcher.update(instance, message),
        vec![crate::launcher::Command::SelectWorkspace(ws.id.clone())]
    );
    let source = launcher.read_file("spaces_view.lua").unwrap();
    for bad in [
        format!(
            "shell.select_workspace('{}'); return function() return ui.text({{'bad'}}) end",
            ws.id
        ),
        format!(
            "return function() shell.select_workspace('{}'); return ui.text({{'bad'}}) end",
            ws.id
        ),
    ] {
        assert!(
            launcher
                .edit_file(
                    &vault,
                    "spaces_view.lua",
                    &source.revision,
                    &[app_host::SourceEdit {
                        old_text: source.content.clone(),
                        new_text: bad
                    },]
                )
                .is_err()
        );
        assert_eq!(launcher.read_file("spaces_view.lua").unwrap(), source);
        assert!(launcher.commands.borrow().is_empty());
    }
    let forged = "return function() return ui.button({id='forged', 'go', on_click=function() shell.select_workspace('absent') end}) end";
    launcher
        .edit_file(
            &vault,
            "spaces_view.lua",
            &source.revision,
            &[app_host::SourceEdit {
                old_text: source.content,
                new_text: forged.into(),
            }],
        )
        .unwrap();
    let event = launcher.view().trigger("forged", Action::Click).unwrap();
    let crate::Msg::Launcher(instance, message) = event else {
        panic!()
    };
    assert!(launcher.update(instance, message).is_empty());
    assert!(
        launcher
            .console(10)
            .iter()
            .any(|line| line.contains("workspace is not in this account"))
    );
    assert_eq!(spaces.borrow().rows[0].id, ws.id);
}

#[test]
fn old_workspace_overrides_keep_working_and_are_not_rewritten_by_listing() {
    let (vault, _tmp, ws) = fresh();
    let legacy = source::bundled().unwrap();
    legacy.get_map("files").delete("spaces.lua").unwrap();
    legacy.get_map("files").delete("spaces_view.lua").unwrap();
    legacy
        .get_map("files")
        .insert_container("main.lua", loro::LoroText::new())
        .unwrap()
        .insert(
            0,
            "return function() return ui.text({'Legacy workspace marker'}) end",
        )
        .unwrap();
    let bytes = legacy.export(loro::ExportMode::Snapshot).unwrap();
    vault.put_entry(source::ENTRY, &bytes).unwrap();
    let mut session = Session::default();
    session.bind_context(
        &vault,
        Some(Context::Spaces(Rc::new(RefCell::new(
            crate::launcher::context::List {
                rows: vec![ws.clone()],
                ..Default::default()
            },
        )))),
    );
    assert!(
        session
            .get()
            .unwrap()
            .view()
            .contains_id("launcher:workspace-list")
    );
    assert_eq!(vault.get_entry(source::ENTRY).unwrap().unwrap(), bytes);
    session.bind(&vault, Some(inventory(ws)));
    assert!(
        crate::info_json(&session.get().unwrap().view().info())
            .to_string()
            .contains("Legacy workspace marker")
    );
    assert_eq!(vault.get_entry(source::ENTRY).unwrap().unwrap(), bytes);
}

#[test]
fn inventory_refresh_keeps_the_session_vm_and_live_snapshot() {
    let (vault, _tmp, ws) = fresh();
    let inventory = inventory(ws.clone());
    let mut session = Session::default();
    session.bind(&vault, Some(inventory.clone()));
    let instance = session.get().unwrap().instance;
    let item = vault
        .create_item(&ws.id, "Live inventory marker", vault::ItemKind::App)
        .unwrap();
    inventory.borrow_mut().items.push(item);
    session.bind(&vault, Some(inventory));
    assert_eq!(session.get().unwrap().instance, instance);
    assert!(
        crate::info_json(&session.get().unwrap().view().info())
            .to_string()
            .contains("Live inventory marker")
    );
}

#[test]
fn leaving_and_reentering_a_workspace_rejects_old_events() {
    let (vault, _tmp, ws) = fresh();
    let mut session = Session::default();
    session.bind(&vault, Some(inventory(ws.clone())));
    let old = session.get().unwrap();
    let event = old.view().trigger("launcher:back", Action::Click).unwrap();
    let crate::Msg::Items(crate::item::ItemsScreenMsg::Launcher(_, instance, message)) = event
    else {
        panic!("unscoped launcher event");
    };
    session.bind(&vault, None);
    assert!(session.get().is_err());
    session.bind(&vault, Some(inventory(ws)));
    assert_ne!(session.get().unwrap().instance, instance);
    assert!(
        session
            .get_mut()
            .unwrap()
            .update(instance, message)
            .is_empty()
    );
}

#[test]
fn resetting_session_keeps_metadata_but_replaces_the_instance() {
    let (vault, _tmp, ws) = fresh();
    let inventory = inventory(ws);
    let mut session = Session::default();
    session.bind(&vault, Some(inventory.clone()));
    let old = session.get().unwrap().instance;
    // Reset must bypass the old VM's state export, including when it is poisoned.
    let file = session.get().unwrap().read_file("main.lua").unwrap();
    session.get_mut().unwrap().edit_file(&vault, "main.lua", &file.revision, &[
        app_host::SourceEdit {
            old_text: file.content,
            new_text: "return function() return ui.button({id='break', 'Break', on_click=function() _dump_state=function() error('poisoned') end end}) end".into(),
        },
    ]).unwrap();
    session.get_mut().unwrap().update(
        old,
        app_host::LuaMsg::Call(app_host::Key::new("break", "on_click")),
    );
    session.reset(&vault).unwrap();
    let fresh = session.get().unwrap().instance;
    assert_ne!(fresh, old);
    session.bind(&vault, Some(inventory));
    assert_eq!(session.get().unwrap().instance, fresh);
}

#[test]
fn locking_and_switching_accounts_discard_the_session() {
    let (mut vault, _tmp, ws) = fresh();
    let inventory = inventory(ws);
    let mut session = Session::default();
    session.bind(&vault, Some(inventory.clone()));
    let old = session.get().unwrap().instance;
    // Even a caller reusing its metadata handle must not reuse the old account's VM.
    vault.signup("other", "pw").unwrap();
    session.bind(&vault, Some(inventory.clone()));
    assert_ne!(session.get().unwrap().instance, old);
    vault.lock();
    session.bind(&vault, Some(inventory));
    assert!(session.get().is_err());
}
