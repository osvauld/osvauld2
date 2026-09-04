//! `reload`: rebuilding the VM without losing the app.
//!
//! The contract these pin, in one line: **on failure nothing changes, and on success only the
//! code does.** Everything that is not the code — the docs and their unflushed writes, the
//! subscription, `ui.state` — has to come through, and the only way to be sure is to make each
//! one observable and break it on purpose.

use super::*;

/// An app whose source can be rewritten afterwards, which is the whole point here.
fn app(files: &[(&str, &str)], resolve: Resolve) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (path, body) in files {
        let t = map.insert_container(*path, LoroText::new()).unwrap();
        t.insert(0, body).unwrap();
    }
    src.commit();
    LuaApp::open(src, resolve, noop_wake(), identity()).unwrap()
}

/// Replace one source file, the way a save or an MCP write would.
fn rewrite(app: &LuaApp<LuaMsg>, path: &str, body: &str) {
    let map = app.src.doc.get_map("files");
    let Some(ValueOrContainer::Container(Container::Text(t))) = map.get(path) else {
        panic!("no {path} in this app");
    };
    t.delete(0, t.len_unicode()).unwrap();
    t.insert(0, body).unwrap();
    app.src.doc.commit();
}

/// The rendered text of a one-`ui.text` app. Enough to say *which* source is running, which is
/// the only question these tests ask of the view.
fn rendered(app: &LuaApp<LuaMsg>) -> String {
    let tree = app
        .view_fn
        .as_ref()
        .expect("no view")
        .call::<Table>(())
        .expect("view failed");
    tree.get::<Table>(1).unwrap().get::<String>(1).unwrap()
}

// ── the happy path ────────────────────────────────────────────────────────────

#[test]
fn a_reload_swaps_the_view() {
    let app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    let mut app = app;
    assert_eq!(rendered(&app), "v1");

    rewrite(
        &app,
        "main.lua",
        "return function() return ui.col{ ui.text{ 'v2' } } end",
    );
    app.reload().unwrap();

    assert_eq!(rendered(&app), "v2");
    assert_eq!(app.error, None);
}

/// The reason `Cores` exists. A reload must not re-read the vault: the doc in memory holds every
/// write since the last flush, and re-importing the snapshot would silently roll them back.
///
/// The resolve here would serve two cards; the live doc has three because the app added one. If
/// the reload re-read, we would be back to two — and nothing would have errored.
#[test]
fn a_reload_keeps_unflushed_writes() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            local b = doc:open("board")
            return function() return ui.col{ ui.text{ "v1" } } end
            "#,
        )],
        serving("board", snapshot_of(&board(&["a", "b"]))),
    );
    let _ = app.view();
    app.vm
        .load(r#"doc:open("board"):insert({ "cards" }, doc.map{ id = "c3", title = "c" })"#)
        .exec()
        .unwrap();
    assert_eq!(cards_len(&app), 2, "stale until the next frame");
    let _ = app.view();
    assert_eq!(cards_len(&app), 3);

    app.reload().unwrap();
    let _ = app.view();
    assert_eq!(
        cards_len(&app),
        3,
        "the reload re-read the vault and lost a write"
    );
}

/// The subscription lives on the core, so a rebuild must not add a second one. If it did, every
/// later edit would bump the version twice — which still *renders* correctly, and would go
/// unnoticed until something depended on the count.
#[test]
fn a_reload_does_not_resubscribe() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            local b = doc:open("board")
            return function() return ui.col{ ui.text{ "v1" } } end
            "#,
        )],
        serving("board", snapshot_of(&board(&["a"]))),
    );
    let _ = app.view();
    app.reload().unwrap();
    let _ = app.view();

    let before = app
        .cores
        .borrow()
        .get("board")
        .unwrap()
        .version
        .load(Ordering::Relaxed);
    let doc = app.cores.borrow().get("board").unwrap().doc.clone();
    doc.get_map("meta").insert("title", "x").unwrap();
    doc.commit();
    let after = app
        .cores
        .borrow()
        .get("board")
        .unwrap()
        .version
        .load(Ordering::Relaxed);

    assert_eq!(
        after - before,
        1,
        "one commit bumped the counter more than once"
    );
}

// ── the failure path ──────────────────────────────────────────────────────────

/// Stage 1: the source does not compile. Nothing ran, so nothing can have changed.
#[test]
fn a_reload_that_does_not_compile_changes_nothing() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    rewrite(&app, "main.lua", "return function( -- unclosed");

    let err = app.reload().unwrap_err();
    assert!(
        err.contains("main.lua"),
        "the error should name the file: {err}"
    );
    assert_eq!(rendered(&app), "v1", "the old view stopped rendering");
    assert_eq!(app.error, None);
}

/// Stage 3, and the reason the trial frame is not optional: this source loads cleanly and
/// returns a perfectly good closure. It only fails when something calls it. Without the trial
/// render we would have swapped, and the app would be broken with no way back.
#[test]
fn a_reload_that_renders_badly_changes_nothing() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    rewrite(
        &app,
        "main.lua",
        "return function() return ui.col{ ui.text{ nope.missing } } end",
    );

    let err = app.reload().unwrap_err();
    // Luau names the field, not the nil it was reached through — and it names the chunk, which
    // is the half that matters for an error card.
    assert!(
        err.contains("main.lua"),
        "the error should name the file: {err}"
    );
    assert!(err.contains("missing"), "unexpected error: {err}");
    assert!(err.contains("stack traceback"), "no traceback: {err}");
    assert_eq!(rendered(&app), "v1");
}

/// A broken *module* is the case an agent hits most, and `require`'s `set_name` is what makes it
/// findable — without it every module reports as `[string "..."]`.
#[test]
fn a_broken_module_names_its_own_file() {
    let mut app = app(
        &[
            (
                "main.lua",
                "local w = require('ui/widgets') return function() return ui.col{ ui.text{ w.label } } end",
            ),
            ("ui/widgets.lua", "return { label = 'v1' }"),
        ],
        Rc::new(|_| Ok(None)),
    );
    assert_eq!(rendered(&app), "v1");

    rewrite(&app, "ui/widgets.lua", "return { label = ");
    let err = app.reload().unwrap_err();
    assert!(
        err.contains("ui/widgets.lua"),
        "the error should name the module, not main.lua: {err}"
    );
    assert_eq!(rendered(&app), "v1");
}

// ── carried state ─────────────────────────────────────────────────────────────

/// The annoyance this exists to prevent: an unsent draft vanishing because someone saved a file.
#[test]
fn a_reload_carries_ui_state() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            return function()
              local s = ui.state("draft", { text = "" })
              return ui.col{ ui.text{ s.text } }
            end
            "#,
        )],
        Rc::new(|_| Ok(None)),
    );
    let _ = app.view();
    app.vm
        .load(r#"_dump_state()["draft"].text = "half typed""#)
        .exec()
        .unwrap();

    rewrite(
        &app,
        "main.lua",
        r#"
        return function()
          local s = ui.state("draft", { text = "" })
          return ui.col{ ui.text{ s.text .. "!" } }
        end
        "#,
    );
    app.reload().unwrap();

    assert_eq!(rendered(&app), "half typed!", "the draft did not survive");
}

/// Clause 4 of the survival contract. A value holding VM identity cannot cross by any mechanism,
/// so the entry is dropped — and the neighbouring entry must still come through, because a
/// dropped one is not a reason to lose the rest.
#[test]
fn a_reload_drops_state_holding_a_function() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            return function()
              local s = ui.state("draft", { text = "kept" })
              local h = ui.state("handler", {})
              return ui.col{ ui.text{ s.text } }
            end
            "#,
        )],
        Rc::new(|_| Ok(None)),
    );
    let _ = app.view();
    app.vm
        .load(r#"_dump_state()["handler"].fn = function() end"#)
        .exec()
        .unwrap();

    app.reload().unwrap();

    assert_eq!(
        rendered(&app),
        "kept",
        "a dropped entry took its neighbour with it"
    );
    // The trial frame re-created it from `init`, so it exists again — but empty, without the
    // function. That is the whole of clause 4: re-created, not carried.
    let has_fn: bool = app
        .vm
        .load(r#"return _dump_state()["handler"].fn ~= nil"#)
        .eval()
        .unwrap();
    assert!(
        !has_fn,
        "a function crossed a VM boundary, which is impossible"
    );
}

/// A self-referential table is the other way `plain` can fail, and it must fail the same way —
/// dropped, not a hang and not a stack overflow.
#[test]
fn a_reload_drops_a_cyclic_state_entry() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            return function()
              local s = ui.state("draft", { text = "kept" })
              local c = ui.state("cycle", {})
              return ui.col{ ui.text{ s.text } }
            end
            "#,
        )],
        Rc::new(|_| Ok(None)),
    );
    let _ = app.view();
    app.vm
        .load(r#"_dump_state()["cycle"].me = _dump_state()["cycle"]"#)
        .exec()
        .unwrap();

    app.reload().unwrap();
    assert_eq!(rendered(&app), "kept");
}

/// `_sweep` runs inside the trial frame, so state belonging to an element the new source no
/// longer draws is cleaned up on the way in rather than lingering forever.
#[test]
fn a_reload_sweeps_state_the_new_source_stopped_using() {
    let mut app = app(
        &[(
            "main.lua",
            r#"
            return function()
              local a = ui.state("kept", { v = 1 })
              local b = ui.state("gone", { v = 2 })
              return ui.col{ ui.text{ "x" } }
            end
            "#,
        )],
        Rc::new(|_| Ok(None)),
    );
    let _ = app.view();
    assert!(
        app.vm
            .load(r#"return _dump_state()["gone"] ~= nil"#)
            .eval::<bool>()
            .unwrap()
    );

    rewrite(
        &app,
        "main.lua",
        r#"
        return function()
          local a = ui.state("kept", { v = 1 })
          return ui.col{ ui.text{ "x" } }
        end
        "#,
    );
    app.reload().unwrap();

    assert!(
        app.vm
            .load(r#"return _dump_state()["kept"] ~= nil"#)
            .eval::<bool>()
            .unwrap()
    );
    assert!(
        app.vm
            .load(r#"return _dump_state()["gone"] == nil"#)
            .eval::<bool>()
            .unwrap(),
        "state for an element the new source never draws stayed alive"
    );
}

// ── the trigger ───────────────────────────────────────────────────────────────
//
// `reload` is the mechanism; these are about *when* it runs. The rule is one line — reload when
// the source moved and not otherwise — and everything below is a way for that to go wrong: not
// noticing, noticing twice, or noticing forever because the new source is broken.

#[test]
fn an_unchanged_source_is_not_stale() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    assert!(
        app.reload_if_stale().is_none(),
        "rebuilt a VM for a source nobody touched"
    );
}

#[test]
fn a_source_edit_makes_the_app_stale() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    rewrite(
        &app,
        "main.lua",
        "return function() return ui.col{ ui.text{ 'v2' } } end",
    );

    app.reload_if_stale()
        .expect("the edit went unnoticed")
        .unwrap();
    assert_eq!(rendered(&app), "v2");
    // And exactly once: the watermark the staged VM carried is now current, so the very next
    // message must not rebuild all over again.
    assert!(
        app.reload_if_stale().is_none(),
        "reloaded twice for one edit"
    );
}

/// A *local* write — the shape an in-app code editor has — moves the source like any other.
/// It must not poke the host, though: the message that carried the edit is already inside a
/// frame, and waking from here would schedule a second one per keystroke.
#[test]
fn a_local_source_edit_counts_but_does_not_wake() {
    let (wake, fired) = counting_wake();
    let src = LoroDoc::new();
    let t = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    t.insert(0, "return function() return ui.col{ ui.text{ 'v1' } } end")
        .unwrap();
    src.commit();
    let mut app = LuaApp::open(src, Rc::new(|_| Ok(None)), wake, identity()).unwrap();

    rewrite(
        &app,
        "main.lua",
        "return function() return ui.col{ ui.text{ 'v2' } } end",
    );
    assert_eq!(
        fired.load(Ordering::Relaxed),
        0,
        "a local edit asked for a frame"
    );
    app.reload_if_stale()
        .expect("the edit went unnoticed")
        .unwrap();
    assert_eq!(rendered(&app), "v2");
}

/// The other half, and the case the gate exists for: the MCP bridge or a peer writes while the
/// window sits idle. Nothing is going to run `update` on its own, so the subscription has to ask.
#[test]
fn an_imported_source_edit_asks_for_a_frame() {
    let (wake, fired) = counting_wake();
    let src = LoroDoc::new();
    let t = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    t.insert(0, "return function() return ui.col{ ui.text{ 'v1' } } end")
        .unwrap();
    src.commit();
    let mut app = LuaApp::open(src, Rc::new(|_| Ok(None)), wake, identity()).unwrap();

    let peer = LoroDoc::new();
    peer.import(&snapshot_of(&app.src.doc)).unwrap();
    let Some(ValueOrContainer::Container(Container::Text(pt))) =
        peer.get_map("files").get("main.lua")
    else {
        panic!("no main.lua in the peer's copy");
    };
    pt.delete(0, pt.len_unicode()).unwrap();
    pt.insert(0, "return function() return ui.col{ ui.text{ 'v2' } } end")
        .unwrap();
    peer.commit();
    app.src.doc.import(&snapshot_of(&peer)).unwrap();

    assert_eq!(
        fired.load(Ordering::Relaxed),
        1,
        "an import landed with no frame to notice it in"
    );
    app.reload_if_stale()
        .expect("the import went unnoticed")
        .unwrap();
    assert_eq!(rendered(&app), "v2");
}

/// `src_seen` is read *before* the build, not after, so an edit landing while the VM is being
/// built still counts as unseen. Reading it afterwards marks this VM current for source it never
/// read, and that edit is then lost until something unrelated happens to move the file again.
///
/// `resolve` runs *inside* `build` — module scope calls `doc:open` — which makes it the one hook
/// a test has into the middle of a rebuild.
#[test]
fn an_edit_landing_mid_build_is_not_marked_seen() {
    let src = LoroDoc::new();
    let t = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    t.insert(
        0,
        r#"
        local b = doc:open("board")
        return function() return ui.col{ ui.text{ "v1" } } end
        "#,
    )
    .unwrap();
    src.commit();

    // Once only: every rebuild re-runs module scope, and a resolve that wrote every time would
    // keep the app permanently stale.
    let armed = Rc::new(std::cell::Cell::new(true));
    let (writer, armed) = (src.clone(), armed.clone());
    let resolve: Resolve = Rc::new(move |_| {
        if armed.replace(false) {
            let Some(ValueOrContainer::Container(Container::Text(t))) =
                writer.get_map("files").get("main.lua")
            else {
                panic!("no main.lua");
            };
            t.insert(0, "-- landed mid-build\n").unwrap();
            writer.commit();
        }
        Ok(None)
    });

    let mut app = LuaApp::open(src, resolve, noop_wake(), identity()).unwrap();
    assert!(
        app.reload_if_stale().is_some(),
        "an edit that landed mid-build was marked as already seen"
    );
}

/// The failure this whole path exists to make visible: you edit a file, it does not compile, and
/// the app keeps running. Without the recorded error nothing anywhere connects the two.
#[test]
fn a_failed_reload_is_recorded_and_the_app_keeps_running() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    rewrite(&app, "main.lua", "return function( -- unclosed");

    app.reload_if_stale()
        .expect("the edit went unnoticed")
        .unwrap_err();
    let err = app.reload_error.as_deref().expect("a failure said nothing");
    assert!(
        err.contains("main.lua"),
        "the banner should name the file: {err}"
    );
    assert_eq!(rendered(&app), "v1", "the old view stopped rendering");
    // `error` is the other kind — a source that never loaded at all. Conflating them would swap
    // a working app for an error page.
    assert_eq!(app.error, None);
    // The banner goes above the app rather than instead of it. `El`'s fields are private to
    // `runtime`, so this only pins that the wrapped tree builds at all.
    let _ = app.view();
}

/// Retried once per *edit*, not once per mouse move. Every message runs the staleness check, so
/// a watermark that stayed put would recompile a broken source on every pointer event — and the
/// next edit, which is the fix, is the only retry worth making.
#[test]
fn a_broken_source_is_retried_only_when_it_changes_again() {
    let mut app = app(
        &[(
            "main.lua",
            "return function() return ui.col{ ui.text{ 'v1' } } end",
        )],
        Rc::new(|_| Ok(None)),
    );
    rewrite(&app, "main.lua", "return function( -- unclosed");
    app.reload_if_stale().unwrap().unwrap_err();

    assert!(
        app.reload_if_stale().is_none(),
        "a broken source is being recompiled on every message"
    );

    rewrite(
        &app,
        "main.lua",
        "return function() return ui.col{ ui.text{ 'v2' } } end",
    );
    app.reload_if_stale()
        .expect("the fix went unnoticed")
        .unwrap();
    assert_eq!(rendered(&app), "v2");
    assert_eq!(app.reload_error, None, "the banner outlived the error");
}

// ── the acceptance test ───────────────────────────────────────────────────────

/// A module edit takes effect, which is the point of the per-VM module cache: nothing clears it,
/// because the VM it belonged to is gone.
#[test]
fn a_reload_picks_up_a_changed_module() {
    let mut app = app(
        &[
            (
                "main.lua",
                "local t = require('theme') return function() return ui.col{ ui.text{ t.name } } end",
            ),
            ("theme.lua", "return { name = 'dark' }"),
        ],
        Rc::new(|_| Ok(None)),
    );
    assert_eq!(rendered(&app), "dark");

    rewrite(&app, "theme.lua", "return { name = 'light' }");
    app.reload().unwrap();

    assert_eq!(rendered(&app), "light", "the module cache outlived its VM");
}
