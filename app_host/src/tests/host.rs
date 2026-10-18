use super::*;
use crate::host::{HostContext, VmSetup};
use std::cell::Cell;

fn source(main: &str) -> LoroDoc {
    let src = LoroDoc::new();
    let text = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    text.insert(0, main).unwrap();
    src.commit();
    src
}

fn setup(calls: Rc<Cell<usize>>, installs: Rc<Cell<usize>>) -> VmSetup {
    Rc::new(move |vm, context: HostContext| {
        installs.set(installs.get() + 1);
        let calls = calls.clone();
        let api = vm.create_table()?;
        api.set(
            "command",
            vm.create_function(move |_, ()| {
                context.require_handler()?;
                calls.set(calls.get() + 1);
                Ok(())
            })?,
        )?;
        vm.globals().set("test_host", api)
    })
}

const BUTTON: &str = r#"
local api = test_host
return function()
    return ui.button({id = "go", "go", on_click = function() api.command() end})
end
"#;

fn click(app: &mut LuaApp<LuaMsg>) {
    let msg = app.view().trigger("go", runtime::Action::Click).unwrap();
    app.update(msg);
}

#[test]
fn staged_source_is_independent_until_the_host_activates_it() {
    let mut app = LuaApp::open(
        source(
            BUTTON
                .replace(
                    "local api = test_host",
                    "local api = {command = function() end}",
                )
                .as_str(),
        ),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
    )
    .unwrap();
    let original = app.read_source_file_versioned("main.lua").unwrap();
    let candidate = app
        .stage_source(source(
            "return function() return ui.button({id='new', 'new', on_click=function() end}) end",
        ))
        .unwrap();
    assert_eq!(
        app.read_source_file_versioned("main.lua").unwrap(),
        original
    );
    assert!(app.view().trigger("go", runtime::Action::Click).is_ok());
    assert!(
        candidate
            .view()
            .trigger("new", runtime::Action::Click)
            .is_ok()
    );
    app = candidate;
    assert!(
        app.read_source_file("main.lua")
            .unwrap()
            .contains("id='new'")
    );
}

#[test]
fn staged_source_rejects_broken_views_and_nested_props_without_changing_source() {
    let app = LuaApp::open(
        source("return function() return ui.text({'good'}) end"),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
    )
    .unwrap();
    let original = app.read_source_file_versioned("main.lua").unwrap();
    for bad in [
        "not valid Lua !!!",
        "return function() error('broken view') end",
        "return function() return ui.col({ui.text({'bad', imaginary_prop=true})}) end",
    ] {
        assert!(app.stage_source(source(bad)).is_err(), "accepted {bad}");
        assert_eq!(
            app.read_source_file_versioned("main.lua").unwrap(),
            original
        );
    }
}

#[test]
fn staged_source_keeps_the_host_guard_during_trial() {
    let calls = Rc::new(Cell::new(0));
    let app = LuaApp::open_with_setup(
        source(BUTTON),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(setup(calls.clone(), Rc::new(Cell::new(0)))),
    )
    .unwrap();
    for bad in [
        "test_host.command(); return function() return ui.text({'bad'}) end",
        "return function() test_host.command(); return ui.text({'bad'}) end",
    ] {
        assert!(app.stage_source(source(bad)).is_err());
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn staged_source_carries_viewer_state_without_modifying_the_old_vm() {
    let app = LuaApp::open(source("return function() local s=ui.state('draft',{text='kept'}); return ui.text({s.text}) end"),
        Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    app.view();
    let candidate = app.stage_source(source("return function() local s=ui.state('draft',{text='new'}); return ui.text({s.text}) end")).unwrap();
    assert!(
        candidate
            .vm
            .load("return ui.state('draft',{}).text == 'kept'")
            .eval::<bool>()
            .unwrap()
    );
    candidate
        .vm
        .load("ui.state('draft',{}).text = 'candidate only'")
        .exec()
        .unwrap();
    assert!(
        app.vm
            .load("return ui.state('draft',{}).text == 'kept'")
            .eval::<bool>()
            .unwrap()
    );
}

#[test]
fn staged_source_cannot_mutate_running_worlds_during_module_load() {
    let app = LuaApp::open(
        source("return function() return ui.world({id='room', width=100, height=100}) end"),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
    )
    .unwrap();
    app.view();
    app.worlds
        .borrow_mut()
        .get_mut("room")
        .unwrap()
        .after("kept", 10.0)
        .unwrap();
    for bad in [
        "world('room'):cancel('kept'); return function() error('broken') end",
        "_sweep=function() world('room'):cancel('kept') end; return function() return ui.text({'bad sweep'}) end",
    ] {
        assert!(app.stage_source(source(bad)).is_err());
        assert_eq!(
            app.worlds.borrow()["room"].timers().len(),
            1,
            "failed trial changed the running world"
        );
    }
}

#[test]
fn ordinary_apps_have_no_host_binding() {
    let app = LuaApp::open(
        source("assert(test_host == nil); return function() return ui.text({'ordinary'}) end"),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
    )
    .unwrap();
    app.view();
    assert!(app.console(10).is_empty());
}

#[test]
fn setup_precedes_source_and_is_rebuilt_on_reload() {
    let calls = Rc::new(Cell::new(0));
    let installs = Rc::new(Cell::new(0));
    let mut app = LuaApp::open_with_setup(
        source(BUTTON),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(setup(calls.clone(), installs.clone())),
    )
    .unwrap();
    click(&mut app);
    assert_eq!(calls.get(), 1);
    app.reload().unwrap();
    assert_eq!(installs.get(), 2);
    assert_eq!(calls.get(), 1, "trial render must not issue commands");
    click(&mut app);
    assert_eq!(calls.get(), 2);
    assert!(app.console(10).is_empty());
}

#[test]
fn commands_are_refused_in_source_and_view() {
    for main in [
        "test_host.command(); return function() return ui.text({'bad'}) end",
        "return function() test_host.command(); return ui.text({'bad'}) end",
    ] {
        let calls = Rc::new(Cell::new(0));
        let app = LuaApp::open_with_setup(
            source(main),
            Rc::new(|_| Ok(None)),
            noop_wake(),
            identity(),
            Some(setup(calls.clone(), Rc::new(Cell::new(0)))),
        )
        .unwrap();
        app.view();
        assert_eq!(calls.get(), 0);
        assert!(
            app.console(10)
                .iter()
                .any(|e| e.contains("only allowed in handlers"))
        );
    }
}

#[test]
fn failed_reload_keeps_old_bindings_and_trial_commands_are_refused() {
    let calls = Rc::new(Cell::new(0));
    let mut app = LuaApp::open_with_setup(
        source(BUTTON),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(setup(calls.clone(), Rc::new(Cell::new(0)))),
    )
    .unwrap();
    click(&mut app);
    app.write_source_file(
        "main.lua",
        "return function() test_host.command(); return ui.text({'bad'}) end",
    )
    .unwrap();
    assert!(
        app.reload()
            .unwrap_err()
            .contains("only allowed in handlers")
    );
    assert_eq!(calls.get(), 1);
    click(&mut app);
    assert_eq!(calls.get(), 2, "old VM's bindings still work");
}

#[test]
fn setup_failure_preserves_the_running_vm() {
    let calls = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(false));
    let base = setup(calls.clone(), Rc::new(Cell::new(0)));
    let install: VmSetup = {
        let fail = fail.clone();
        Rc::new(move |vm, context| {
            if fail.get() {
                return Err(mlua::Error::runtime("installation failed"));
            }
            base(vm, context)
        })
    };
    let mut app = LuaApp::open_with_setup(
        source(BUTTON),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(install),
    )
    .unwrap();
    fail.set(true);
    assert!(app.reload().unwrap_err().contains("installation failed"));
    click(&mut app);
    assert_eq!(calls.get(), 1);
}

#[test]
fn a_handler_error_does_not_leave_host_commands_enabled() {
    let calls = Rc::new(Cell::new(0));
    let main = r#"
local clicked = false
return function()
    if clicked then test_host.command() end
    return ui.button({id = "go", "go", on_click = function()
        clicked = true
        test_host.command()
        error("handler failed")
    end})
end
"#;
    let mut app = LuaApp::open_with_setup(
        source(main),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(setup(calls.clone(), Rc::new(Cell::new(0)))),
    )
    .unwrap();
    click(&mut app);
    app.view();
    assert_eq!(calls.get(), 1);
    assert!(
        app.console(10)
            .iter()
            .any(|e| e.contains("only allowed in handlers"))
    );
}
