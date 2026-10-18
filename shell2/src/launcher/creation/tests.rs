use super::*;
use crate::launcher::{
    Command, Launcher,
    context::{Context, List},
};
use runtime::Action;
use std::{cell::RefCell, rc::Rc};

fn send(launcher: &mut Launcher, id: &str, action: Action<'_>) -> Vec<Command> {
    let crate::Msg::Launcher(instance, message) = launcher.view().trigger(id, action).unwrap()
    else {
        panic!()
    };
    launcher.update(instance, message)
}

#[test]
fn lua_form_retains_failed_drafts_blocks_pending_submit_and_clears_after_success() {
    let (vault, _tmp, _) = crate::launcher::tests::fresh();
    let list = Rc::new(RefCell::new(List::default()));
    let mut launcher = Launcher::new_in_context(&vault, Context::Spaces(list.clone())).unwrap();
    send(&mut launcher, "launcher:new-workspace", Action::Click);
    send(&mut launcher, "launcher:create:submit", Action::Click);
    assert!(
        crate::info_json(&launcher.view().info())
            .to_string()
            .contains("enter a workspace name")
    );
    assert!(launcher.commands.borrow().is_empty());
    send(
        &mut launcher,
        "launcher:create:name",
        Action::Type("  Draft workspace  "),
    );
    assert_eq!(
        send(&mut launcher, "launcher:create:submit", Action::Click),
        vec![Command::CreateWorkspace("Draft workspace".into())]
    );
    let job = list.borrow_mut().creation.begin().unwrap();
    assert!(
        launcher
            .view()
            .trigger("launcher:create:submit", Action::Click)
            .is_err()
    );
    list.borrow_mut()
        .creation
        .finish(job, Some("disk failure".into()));
    let tree = crate::info_json(&launcher.view().info()).to_string();
    assert!(tree.contains("disk failure") && tree.contains("Draft workspace"));
    assert_eq!(
        send(&mut launcher, "launcher:create:submit", Action::Click).len(),
        1
    );
    let job = list.borrow_mut().creation.begin().unwrap();
    list.borrow_mut().creation.finish(job, None);
    assert!(!launcher.view().contains_id("launcher:create:name"));
    send(&mut launcher, "launcher:new-workspace", Action::Click);
    assert!(
        !crate::info_json(&launcher.view().info())
            .to_string()
            .contains("Draft workspace")
    );
    send(&mut launcher, "launcher:create:cancel", Action::Click);
    assert!(!launcher.view().contains_id("launcher:create:name"));
    assert!(launcher.console(10).is_empty());
}

#[test]
fn creation_capability_rejects_load_view_trials_and_invalid_names() {
    let (vault, _tmp, _) = crate::launcher::tests::fresh();
    let mut launcher = Launcher::new_in_context(
        &vault,
        Context::Spaces(Rc::new(RefCell::new(List::default()))),
    )
    .unwrap();
    let entry = launcher.read_file("spaces_view.lua").unwrap();
    for source in [
        "shell.create_workspace('bad'); return function() return ui.text({'bad'}) end",
        "return function() shell.create_workspace('bad'); return ui.text({'bad'}) end",
    ] {
        assert!(
            launcher
                .edit_file(
                    &vault,
                    "spaces_view.lua",
                    &entry.revision,
                    &[app_host::SourceEdit {
                        old_text: entry.content.clone(),
                        new_text: source.into()
                    },]
                )
                .is_err()
        );
        assert!(launcher.commands.borrow().is_empty());
        assert_eq!(launcher.read_file("spaces_view.lua").unwrap(), entry);
    }
    let invalid = "return function() return ui.button({id='invalid', 'bad', on_click=function() shell.create_workspace(string.rep('x', 257)) end}) end";
    launcher
        .edit_file(
            &vault,
            "spaces_view.lua",
            &entry.revision,
            &[app_host::SourceEdit {
                old_text: entry.content,
                new_text: invalid.into(),
            }],
        )
        .unwrap();
    assert!(send(&mut launcher, "invalid", Action::Click).is_empty());
    assert!(
        launcher
            .console(10)
            .iter()
            .any(|line| line.contains("at most 256 bytes"))
    );
    assert_eq!(vault.workspaces().unwrap().len(), 1);
}

#[test]
fn completions_only_refresh_the_submitting_account_and_list_job() {
    let (vault, _tmp, workspace) = crate::launcher::tests::fresh();
    let account = vault.current().unwrap().did;
    let mut screen = crate::space::SpaceScreen::new(&vault);
    let job = screen.inventory().borrow_mut().creation.begin().unwrap();
    screen.finish_creation(&vault, "another account", job, Ok(workspace.clone()));
    screen.finish_creation(&vault, &account, job + 1, Ok(workspace.clone()));
    assert_eq!(screen.spaces().len(), 1);
    assert_eq!(screen.inventory().borrow().creation.pending, Some(job));
    let mut reentered = crate::space::SpaceScreen::new(&vault);
    reentered.finish_creation(&vault, &account, job, Ok(workspace.clone()));
    assert_eq!(reentered.spaces().len(), 1);
    screen.finish_creation(&vault, &account, job, Err("disk failure".into()));
    assert_eq!(
        screen.inventory().borrow().creation.error.as_deref(),
        Some("disk failure")
    );
    let job = screen.inventory().borrow_mut().creation.begin().unwrap();
    let created = vault.create_workspace_for(&account, "Created").unwrap();
    screen.finish_creation(&vault, &account, job, Ok(created.clone()));
    assert_eq!(screen.spaces()[0], created);
    assert_eq!(screen.inventory().borrow().rows, screen.spaces());
}

#[test]
fn accepted_old_jobs_refresh_metadata_without_overwriting_new_job_feedback() {
    let (vault, _tmp, _) = crate::launcher::tests::fresh();
    let account = vault.current().unwrap().did;
    let mut original = crate::space::SpaceScreen::new(&vault);
    let old_job = original.inventory().borrow_mut().creation.begin().unwrap();
    let mut reentered = crate::space::SpaceScreen::new(&vault);
    let new_job = reentered.inventory().borrow_mut().creation.begin().unwrap();
    let created = vault
        .create_workspace_for(&account, "Finished after navigation")
        .unwrap();
    reentered.finish_creation(&vault, &account, old_job, Ok(created.clone()));
    assert_eq!(reentered.spaces()[0], created);
    assert_eq!(
        reentered.inventory().borrow().creation.pending,
        Some(new_job)
    );
    assert_eq!(reentered.inventory().borrow().creation.generation, 0);
    reentered.finish_creation(&vault, &account, old_job, Ok(created));
    assert_eq!(reentered.spaces().len(), 2);
    reentered.finish_creation(&vault, &account, old_job, Err("old failure".into()));
    assert!(reentered.inventory().borrow().creation.error.is_none());
    assert_eq!(
        reentered.inventory().borrow().creation.pending,
        Some(new_job)
    );
}

#[test]
fn creation_validates_bounds_and_normalizes_the_name() {
    assert_eq!(name("  Work ✨  ").unwrap(), "Work ✨");
    for invalid in ["".to_string(), " \n\t".into(), "x".repeat(257)] {
        assert!(name(&invalid).is_err());
    }
}

#[test]
fn pending_creation_rejects_duplicates_and_stale_completions() {
    let mut state = Creation::default();
    let job = state.begin().unwrap();
    assert!(state.begin().is_none());
    assert!(!state.finish(job + 1, Some("stale".into())));
    assert_eq!(state.pending, Some(job));
    assert!(state.finish(job, Some("disk failure".into())));
    assert_eq!(state.error.as_deref(), Some("disk failure"));
    assert_eq!(state.generation, 1);
    let retry = state.begin().unwrap();
    assert_ne!(retry, job);
    assert!(state.error.is_none());
    assert!(!state.finish(job, None));
    assert!(state.finish(retry, None));
    assert_eq!(state.generation, 2);
    assert!(state.pending.is_none());
    assert!(state.error.is_none());
    assert!(!Creation::default().finish(retry, None));
}
