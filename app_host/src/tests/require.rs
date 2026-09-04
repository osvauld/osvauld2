//! `require`: multi-file apps.
//!
//! The upload has always stored every file an app folder contained; only the loader was
//! single-file. These tests drive `require` through the app's own VM, because a module's value
//! is not otherwise observable — the view only ever hands back an element tree.

use super::*;

/// An app built from several source files, the way an upload stores them.
fn multi(files: &[(&str, &str)]) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (path, body) in files {
        let t = map.insert_container(*path, LoroText::new()).unwrap();
        t.insert(0, body).unwrap();
    }
    src.commit();
    LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap()
}

/// Evaluate an expression inside the app's own VM — same globals, same module cache.
fn eval<T: FromLua>(app: &LuaApp<LuaMsg>, src: &str) -> mlua::Result<T> {
    app.vm.load(src).eval::<T>()
}

#[test]
fn require_loads_a_sibling_module() {
    let app = multi(&[
        ("main.lua", "return function() return ui.text{ 'x' } end"),
        (
            "lib/util.lua",
            "return { greet = function() return 'hi' end }",
        ),
    ]);
    assert_eq!(app.error, None);
    assert_eq!(
        eval::<String>(&app, "return require('lib/util').greet()").unwrap(),
        "hi"
    );
}

/// The reason a cache exists at all. Without it each requirer re-runs the file and gets its own
/// table, so two modules sharing state would silently hold two copies of it.
#[test]
fn a_module_runs_once_and_is_shared() {
    let app = multi(&[
        ("main.lua", "return function() return ui.text{ 'x' } end"),
        ("state.lua", "return { drag = nil }"),
    ]);
    assert_eq!(app.error, None);

    // Identity, not equality: both requirers must hold the *same* table.
    assert!(eval::<bool>(&app, "return require('state') == require('state')").unwrap());

    app.vm.load("require('state').drag = 'k1'").exec().unwrap();
    assert_eq!(
        eval::<String>(&app, "return require('state').drag").unwrap(),
        "k1",
        "a write through one require was invisible to the next"
    );
}

#[test]
fn a_module_can_require_another_module() {
    let app = multi(&[
        ("main.lua", "return function() return ui.text{ 'x' } end"),
        ("a.lua", "return { n = require('b').n + 1 }"),
        ("b.lua", "return { n = 1 }"),
    ]);
    assert_eq!(app.error, None);
    assert_eq!(eval::<i64>(&app, "return require('a').n").unwrap(), 2);
}

/// `main.lua` is the entry point, not a special case — it can require like anything else, and
/// this is the shape the kanban split takes.
#[test]
fn main_can_require_at_module_scope() {
    let app = multi(&[
        (
            "main.lua",
            "local m = require('model')\nreturn function() return ui.text{ m.title } end",
        ),
        ("model.lua", "return { title = 'Board' }"),
    ]);
    assert_eq!(app.error, None);
    let view_fn = app.view_fn.as_ref().expect("no view closure");
    view_fn.call::<Table>(()).expect("the view failed to build");
}

/// An agent's only feedback loop, so the error carries the answer with it: what this app has.
#[test]
fn a_missing_module_lists_what_exists() {
    let app = multi(&[
        (
            "main.lua",
            "local m = require('modle')\nreturn function() end",
        ),
        ("model.lua", "return {}"),
        ("ui/card.lua", "return {}"),
        ("manifest.osv", "app \"k\""),
    ]);
    let err = app.error.expect("a missing module must fail module scope");
    assert!(err.contains("no modle.lua in this app"), "{err}");
    assert!(err.contains("main.lua, model.lua, ui/card.lua"), "{err}");
    assert!(
        !err.contains("manifest.osv"),
        "only Lua is requirable: {err}"
    );
}

/// Not a nicety. Each `require` is a Rust frame calling back into Lua, so an undetected cycle
/// recurses through the *native* stack: the interrupt budget never runs and the process dies
/// with an uncatchable stack overflow. Disabling the guard aborts this whole test binary with
/// SIGABRT rather than failing one test — which is how it was checked.
#[test]
fn a_require_cycle_is_an_error_not_a_hang() {
    let app = multi(&[
        ("main.lua", "local a = require('a')\nreturn function() end"),
        ("a.lua", "return { b = require('b') }"),
        ("b.lua", "return { a = require('a') }"),
    ]);
    let err = app.error.expect("a cycle must fail module scope");
    assert!(
        err.contains("require cycle: a.lua -> b.lua -> a.lua"),
        "{err}"
    );
}

/// A module that threw must not stay poisoned in the cache — a second require should report the
/// same real error again, not a stale entry or a half-built table.
#[test]
fn a_failed_module_is_not_cached() {
    let app = multi(&[
        ("main.lua", "return function() return ui.text{ 'x' } end"),
        ("bad.lua", "error('boom')"),
    ]);
    assert_eq!(app.error, None, "main itself is fine");

    for attempt in 1..=2 {
        let err = eval::<Value>(&app, "return require('bad')")
            .expect_err("a throwing module must fail")
            .to_string();
        assert!(err.contains("boom"), "attempt {attempt}: {err}");
    }
}

/// The path a file listing or an error message hands you already carries the extension.
#[test]
fn require_accepts_an_explicit_lua_suffix() {
    let app = multi(&[
        ("main.lua", "return function() end"),
        ("lib/util.lua", "return { n = 7 }"),
    ]);
    assert_eq!(
        eval::<i64>(&app, "return require('lib/util.lua').n").unwrap(),
        7
    );
    assert!(
        eval::<bool>(
            &app,
            "return require('lib/util') == require('lib/util.lua')"
        )
        .unwrap(),
        "both spellings must resolve to one cache entry"
    );
}

/// Modules are not second-class: the prelude and the doc layer are installed before any of them
/// runs, so a module can open a doc at its own top level. That is what lets the kanban's model
/// move out of `main.lua` unchanged.
#[test]
fn a_module_sees_the_prelude_and_the_doc_layer() {
    let app = multi(&[
        (
            "main.lua",
            "local m = require('model')\nreturn function() return ui.text{ m.board.meta.title } end",
        ),
        (
            "model.lua",
            r#"
            local board = doc:open("board")
            if not board.meta then board:set({"meta"}, doc.map{ title = "Board" }) end
            return { board = board }
            "#,
        ),
    ]);
    assert_eq!(app.error, None);
    assert_eq!(app.docs.borrow().len(), 1, "the module opened the doc");
    let _ = app.view();
    let view_fn = app.view_fn.as_ref().expect("no view closure");
    view_fn.call::<Table>(()).expect("the view failed to build");
}

/// The sandbox's whole claim: an app sees its own source and nothing else. There is no search
/// path to escape from, so every one of these is just a key the `files` map does not have.
#[test]
fn require_cannot_reach_outside_the_app() {
    let app = multi(&[
        ("main.lua", "return function() end"),
        ("manifest.osv", "app \"k\""),
    ]);
    for path in ["manifest.osv", "../secrets", "/etc/passwd", "os"] {
        let err = eval::<Value>(&app, &format!("return require('{path}')"))
            .expect_err(&format!("{path} must not resolve"))
            .to_string();
        assert!(err.contains("in this app"), "{path}: {err}");
    }
}
