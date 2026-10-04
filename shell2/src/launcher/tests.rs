use super::*;
use app_host::SourceEdit;
use tempfile::TempDir;

fn fresh() -> (Vault, TempDir, WorkspaceMeta) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("owner", "pw").unwrap();
    let ws = vault.create_workspace("Home").unwrap();
    (vault, tmp, ws)
}

fn title_edit(text: &str) -> SourceEdit {
    SourceEdit {
        old_text: "Your apps".into(),
        new_text: text.into(),
    }
}

#[test]
fn launcher_customization_is_sealed_account_owned_and_survives_reopen() {
    let (mut vault, tmp, ws) = fresh();
    let did = vault.current().unwrap().did.clone();
    let mut launcher = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    let before = launcher.read_file("main.lua").unwrap();
    let revision = launcher
        .edit_file(
            &vault,
            "main.lua",
            &before.revision,
            &[title_edit("Personal launcher marker")],
        )
        .unwrap();
    assert!(
        launcher
            .read_file("main.lua")
            .unwrap()
            .content
            .contains("Personal launcher marker")
    );
    {
        let sealed = vault
            .store()
            .unwrap()
            .get("entry/shell/launcher/v1")
            .unwrap()
            .unwrap();
        assert!(
            !sealed
                .windows(b"Personal launcher marker".len())
                .any(|w| w == b"Personal launcher marker")
        );
    }
    drop(launcher);
    vault.lock();
    drop(vault);
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.login(&did, "pw").unwrap();
    let launcher = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    assert_eq!(launcher.read_file("main.lua").unwrap().revision, revision);
    drop(launcher);
    vault.signup("other", "pw").unwrap();
    let ws = vault.create_workspace("Other").unwrap();
    assert_eq!(
        Launcher::new(&vault, ws, vec![])
            .unwrap()
            .read_file("main.lua")
            .unwrap(),
        before
    );
}

#[test]
fn bad_edits_and_failed_saves_preserve_source_and_saved_snapshot() {
    let (mut vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws, vec![]).unwrap();
    let before = launcher.read_file("main.lua").unwrap();
    for edit in [
        SourceEdit {
            old_text: "local C".into(),
            new_text: "not valid !!!!".into(),
        },
        SourceEdit {
            old_text: "local snapshot = shell.snapshot()".into(),
            new_text: "error('broken view')".into(),
        },
        SourceEdit {
            old_text: "pad = 20".into(),
            new_text: "unknown_prop = true".into(),
        },
    ] {
        assert!(
            launcher
                .edit_file(&vault, "main.lua", &before.revision, &[edit])
                .is_err()
        );
        assert_eq!(launcher.read_file("main.lua").unwrap(), before);
        assert!(vault.get_entry(source::ENTRY).unwrap().is_none());
    }
    assert!(
        launcher
            .edit_file(&vault, "main.lua", "stale", &[title_edit("changed")])
            .is_err()
    );
    vault.lock();
    assert!(
        launcher
            .edit_file(
                &vault,
                "main.lua",
                &before.revision,
                &[title_edit("unsaved")]
            )
            .is_err()
    );
    assert_eq!(launcher.read_file("main.lua").unwrap(), before);
}

#[test]
fn launcher_cursor_coordinates_remain_correct_inside_rust_chrome() {
    struct Mounted(Launcher);
    impl runtime::App for Mounted {
        type Msg = Msg;
        fn view(&self) -> El<Msg> {
            runtime::col().full().pad(40.0).child(self.0.view())
        }
        fn update(&mut self, msg: Msg) {
            if let Msg::Items(ItemsScreenMsg::Launcher(_, instance, msg)) = msg {
                self.0.update(instance, msg);
            }
        }
    }
    let (vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws, vec![]).unwrap();
    let cursor = launcher.read_file("cursor.lua").unwrap();
    // Rects reports input targets only: give the decorative frame a test-only hover probe.
    launcher
        .edit_file(
            &vault,
            "cursor.lua",
            &cursor.revision,
            &[SourceEdit {
                old_text: "id = \"cursor:\" .. name,".into(),
                new_text: "id = \"cursor:\" .. name, on_hover = function() end,".into(),
            }],
        )
        .unwrap();
    let mut driver = runtime::Headless::new(Mounted(launcher), (900.0, 700.0));
    driver.move_to(300.0, 250.0);
    let rects = driver.rects();
    let cursor = rects.iter().find(|r| r.id == "cursor:arrow").unwrap();
    assert!(
        (cursor.x - 299.0).abs() < 0.1 && (cursor.y - 249.0).abs() < 0.1,
        "{cursor:?}"
    );
}

fn action(launcher: &mut Launcher, id: &str, action: runtime::Action<'_>) -> Vec<Command> {
    let msg = launcher.view().trigger(id, action).unwrap();
    let Msg::Items(ItemsScreenMsg::Launcher(_, instance, msg)) = msg else {
        panic!("unscoped launcher action")
    };
    launcher.update(instance, msg)
}

#[test]
fn lua_editor_keeps_failed_drafts_and_closes_after_a_successful_save_or_reset() {
    let (vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws, vec![]).unwrap();
    action(&mut launcher, "launcher:customize", runtime::Action::Click);
    assert!(launcher.app.console(10).is_empty());
    action(
        &mut launcher,
        "launcher:edit:main.lua",
        runtime::Action::Click,
    );
    let before = launcher.read_file("main.lua").unwrap();
    let bad = before
        .content
        .replace("local snapshot = shell.snapshot()", "error('bad draft')");
    action(
        &mut launcher,
        "launcher:editor:source",
        runtime::Action::Type(&bad),
    );
    let commands = action(
        &mut launcher,
        "launcher:editor:save",
        runtime::Action::Click,
    );
    let Command::EditSource {
        path,
        revision,
        edits,
        ..
    } = &commands[0]
    else {
        panic!("not an edit")
    };
    assert!(launcher.edit_file(&vault, path, revision, edits).is_err());
    let tree = crate::info_json(&launcher.view().info()).to_string();
    assert!(tree.contains("bad draft") && tree.contains("launcher:editor:source"));
    assert_eq!(launcher.read_file("main.lua").unwrap(), before);
    let good = before.content.replace("Your apps", "Editor saved title");
    action(
        &mut launcher,
        "launcher:editor:source",
        runtime::Action::Type(&good),
    );
    let commands = action(
        &mut launcher,
        "launcher:editor:save",
        runtime::Action::Click,
    );
    let Command::EditSource {
        path,
        revision,
        edits,
        ..
    } = &commands[0]
    else {
        panic!("not an edit")
    };
    launcher.edit_file(&vault, path, revision, edits).unwrap();
    let tree = crate::info_json(&launcher.view().info()).to_string();
    assert!(tree.contains("Editor saved title") && !tree.contains("launcher:editor:source"));
    action(&mut launcher, "launcher:customize", runtime::Action::Click);
    launcher.reset(&vault).unwrap();
    assert!(
        !crate::info_json(&launcher.view().info())
            .to_string()
            .contains("launcher:editor:source")
    );
}

#[test]
fn recreated_launchers_reject_old_input_and_save_messages_even_in_the_same_workspace() {
    let (vault, _tmp, ws) = fresh();
    let mut old = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    let mut current = Launcher::new(&vault, ws, vec![]).unwrap();
    action(&mut old, "launcher:customize", runtime::Action::Click);
    action(&mut current, "launcher:customize", runtime::Action::Click);
    for (id, input) in [
        (
            "launcher:editor:source",
            runtime::Action::Type("stale draft"),
        ),
        ("launcher:editor:save", runtime::Action::Click),
    ] {
        let msg = old.view().trigger(id, input).unwrap();
        let Msg::Items(ItemsScreenMsg::Launcher(_, instance, msg)) = msg else {
            panic!("unscoped")
        };
        assert!(current.update(instance, msg).is_empty());
    }
    assert!(
        !crate::info_json(&current.view().info())
            .to_string()
            .contains("stale draft")
    );
    assert!(vault.get_entry(source::ENTRY).unwrap().is_none());
}

#[test]
fn workspace_commands_revalidate_home_workspace_and_unlocked_account() {
    let (mut vault, _tmp, ws) = fresh();
    assert!(navigation::validate(&vault, Some(&ws.id), 0, &ws.id).is_ok());
    assert!(navigation::validate(&vault, Some(&ws.id), 1, &ws.id).is_err());
    assert!(navigation::validate(&vault, None, 0, &ws.id).is_err());
    assert!(navigation::validate(&vault, Some(&ws.id), 0, "elsewhere").is_err());
    assert!(navigation::validate(&vault, Some("foreign"), 0, "foreign").is_err());
    vault.lock();
    assert!(navigation::validate(&vault, Some(&ws.id), 0, &ws.id).is_err());
}

#[test]
fn lua_workspace_header_queues_scoped_navigation_and_import() {
    let (vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    assert!(launcher.view().contains_id("launcher:workspace-header"));
    for (id, expected) in [("launcher:back", WorkspaceAction::Back),
                           ("launcher:add-item", WorkspaceAction::Import)] {
        assert_eq!(action(&mut launcher, id, runtime::Action::Click),
            vec![Command::Workspace { ws_id: ws.id.clone(), action: expected }]);
    }
    let before = launcher.read_file("main.lua").unwrap();
    for command in ["shell.back()", "shell.import_item()"] {
        for bad in [format!("{command}; return function() return ui.text({{'bad'}}) end"),
                    format!("return function() {command}; return ui.text({{'bad'}}) end")] {
            assert!(launcher.edit_file(&vault, "main.lua", &before.revision, &[SourceEdit {
                old_text: before.content.clone(), new_text: bad,
            }]).unwrap_err().contains("only allowed in handlers"));
            assert!(launcher.commands.borrow().is_empty());
        }
    }
}

#[test]
fn reset_does_not_call_the_failed_launchers_state_export() {
    let (vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws, vec![]).unwrap();
    let before = launcher.read_file("main.lua").unwrap();
    launcher.edit_file(&vault, "main.lua", &before.revision, &[SourceEdit {
        old_text: before.content.clone(),
        new_text: "return function() return ui.button({id='break', 'Break', on_click=function() _dump_state=function() error('poisoned state export') end end}) end".into(),
    }]).unwrap();
    assert!(action(&mut launcher, "break", runtime::Action::Click).is_empty());
    launcher.reset(&vault).unwrap();
    assert_eq!(launcher.read_file("main.lua").unwrap(), before);
    assert!(vault.get_entry(source::ENTRY).unwrap().is_none());
}

#[test]
fn resource_exhaustion_rejects_edits_and_preserves_durable_launcher() {
    let (vault, _tmp, ws) = fresh();
    let mut launcher = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    let before = launcher.read_file("main.lua").unwrap();
    launcher.edit_file(&vault, "main.lua", &before.revision,
        &[title_edit("Still working")]).unwrap();
    let working = launcher.read_file("main.lua").unwrap();
    let saved = vault.get_entry(source::ENTRY).unwrap().unwrap();
    for bad in [
        "while true do end",
        "return function() while true do end end",
        "local huge=string.rep('x', 64*1024*1024); return function() return ui.text({tostring(#huge)}) end",
    ] {
        let error = launcher.edit_file(&vault, "main.lua", &working.revision,
            &[SourceEdit { old_text: working.content.clone(), new_text: bad.into() }])
            .expect_err("resource-exhausting source must not activate");
        assert!(error.contains("budget") || error.contains("deadline") || error.contains("memory"), "{error}");
        assert_eq!(launcher.read_file("main.lua").unwrap(), working);
        assert_eq!(vault.get_entry(source::ENTRY).unwrap().unwrap(), saved);
        assert!(crate::info_json(&launcher.view().info()).to_string().contains("Still working"));
    }
    launcher.reset(&vault).unwrap();
    assert_eq!(launcher.read_file("main.lua").unwrap(), before);
    assert!(vault.get_entry(source::ENTRY).unwrap().is_none());
    assert!(Launcher::new(&vault, ws, vec![]).unwrap().warning().is_none());
}

#[test]
fn invalid_saved_source_falls_back_without_deleting_and_reset_is_durable() {
    let (vault, _tmp, ws) = fresh();
    vault
        .put_entry(source::ENTRY, b"corrupted snapshot")
        .unwrap();
    let mut launcher = Launcher::new(&vault, ws.clone(), vec![]).unwrap();
    assert!(launcher.warning().is_some());
    assert!(
        launcher
            .read_file("main.lua")
            .unwrap()
            .content
            .contains("Your apps")
    );
    assert_eq!(
        vault.get_entry(source::ENTRY).unwrap().unwrap(),
        b"corrupted snapshot"
    );
    launcher.reset(&vault).unwrap();
    assert!(launcher.warning().is_none());
    assert!(vault.get_entry(source::ENTRY).unwrap().is_none());
    assert!(
        Launcher::new(&vault, ws, vec![])
            .unwrap()
            .warning()
            .is_none()
    );
}
