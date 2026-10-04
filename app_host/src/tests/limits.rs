use super::*;
use crate::host::{VmLimits, VmSetup};
use std::time::Duration;

fn source(main: &str) -> LoroDoc {
    let doc = LoroDoc::new();
    doc.get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap()
        .insert(0, main)
        .unwrap();
    doc
}

fn open(main: &str, limits: VmLimits) -> LuaApp<LuaMsg> {
    let setup: VmSetup = Rc::new(move |vm, context| context.set_limits(vm, limits));
    LuaApp::open_with_setup(
        source(main),
        Rc::new(|_| Ok(None)),
        noop_wake(),
        identity(),
        Some(setup),
    )
    .unwrap()
}

fn limits() -> VmLimits {
    VmLimits {
        memory_bytes: 8 * 1024 * 1024,
        interrupts: 100,
        time: Duration::from_secs(10),
        load_time: Duration::from_secs(10),
    }
}

#[test]
fn limited_load_view_and_staging_stop_runaway_lua() {
    let load = open("while true do end", limits());
    assert!(
        load.console(512)
            .iter()
            .any(|s| s.contains("interrupt budget exceeded"))
    );
    let app = open(
        "return function() return ui.text({'working'}) end",
        limits(),
    );
    let before = app.read_source_file_versioned("main.lua").unwrap();
    for bad in [
        "while true do end",
        "return function() while true do end end",
    ] {
        assert!(
            app.stage_source(source(bad))
                .err()
                .unwrap()
                .contains("interrupt budget exceeded")
        );
        assert_eq!(app.read_source_file_versioned("main.lua").unwrap(), before);
        assert_eq!(app.view().info().text.as_deref(), Some("working"));
    }
}

#[test]
fn limited_handler_recovers_a_fresh_budget_and_reload_keeps_policy() {
    let main = "local n=0; return function() return ui.button({id='go', tostring(n), on_click=function() if n==0 then n=1; while true do end else n=n+1 end end}) end";
    let mut app = open(main, limits());
    let msg = app.view().trigger("go", runtime::Action::Click).unwrap();
    app.update(msg);
    assert!(
        app.console(512)
            .iter()
            .any(|s| s.contains("interrupt budget exceeded"))
    );
    let msg = app.view().trigger("go", runtime::Action::Click).unwrap();
    app.update(msg);
    assert_eq!(app.view().info().children[0].text.as_deref(), Some("2"));
    app.reload().unwrap();
    let msg = app.view().trigger("go", runtime::Action::Click).unwrap();
    app.update(msg);
    assert!(
        app.console(512)
            .last()
            .unwrap()
            .contains("interrupt budget exceeded")
    );
}

#[test]
fn heap_limit_rejects_candidate_without_changing_working_vm() {
    let app = open(
        "return function() return ui.text({'working'}) end",
        limits(),
    );
    let error = app.stage_source(source("local huge=string.rep('x', 16*1024*1024); return function() return ui.text({huge}) end")).err().unwrap();
    assert!(error.contains("memory"), "{error}");
    assert_eq!(app.view().info().text.as_deref(), Some("working"));
}

#[test]
fn view_and_handler_deadlines_start_fresh_after_load() {
    let app = open(
        "return function() return ui.button({id='go', 'go', on_click=function() while true do end end}) end",
        VmLimits {
            time: Duration::ZERO,
            interrupts: u64::MAX,
            ..limits()
        },
    );
    assert!(app.error.is_none());
    app.view();
    assert!(
        app.console(512)
            .iter()
            .any(|s| s.contains("execution deadline exceeded"))
    );
    let mut app = open(
        "return function() return ui.button({id='go', 'go', on_click=function() while true do end end}) end",
        limits(),
    );
    let msg = app.view().trigger("go", runtime::Action::Click).unwrap();
    // End the view's budget, then give the next handler an independently expired one.
    app.host_context
        .set_limits(
            &app.vm,
            VmLimits {
                time: Duration::ZERO,
                interrupts: u64::MAX,
                ..limits()
            },
        )
        .unwrap();
    app.update(msg);
    assert!(
        app.console(512)
            .last()
            .unwrap()
            .contains("execution deadline exceeded")
    );
}

#[test]
fn ordinary_apps_keep_their_existing_heap_policy() {
    let app = LuaApp::open(
        source("local huge=string.rep('x', 16*1024*1024); return function() return ui.text({tostring(#huge)}) end"),
        Rc::new(|_| Ok(None)), noop_wake(), identity(),
    ).unwrap();
    assert_eq!(app.view().info().text.as_deref(), Some("16777216"));
}

#[test]
fn deadline_is_separate_from_interrupt_ceiling() {
    let app = open(
        "while true do end",
        VmLimits {
            time: Duration::ZERO,
            load_time: Duration::ZERO,
            interrupts: u64::MAX,
            ..limits()
        },
    );
    assert!(
        app.console(512)
            .iter()
            .any(|s| s.contains("execution deadline exceeded"))
    );
}
