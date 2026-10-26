//! `ui.group` in the UI tree (`docs/design/ui-reuse.md` U2–U4): a group runs again only when a
//! signal it read changed, and a group that did not run hands back what it built — handlers
//! included. Every group here counts its runs in the global `runs`.

use super::*;
use runtime::{Action, ElInfo};

fn texts(info: &ElInfo, out: &mut Vec<String>) {
    out.extend(info.text.clone());
    info.children.iter().for_each(|c| texts(c, out));
}

fn shown(app: &LuaApp<LuaMsg>) -> String {
    let mut out = Vec::new();
    texts(&app.view().info(), &mut out);
    out.join(" ")
}

fn runs(app: &LuaApp<LuaMsg>, name: &str) -> String {
    said(app, &format!("return runs.{name}"))
}

const TWO: &str = "a = signal(1, 'a') b = signal(1, 'b') runs = { a = 0, b = 0 } \
    return function() return ui.col({ \
      ui.group('ga', function() runs.a += 1 return ui.text({ 'a' .. a() }) end), \
      ui.group('gb', function() runs.b += 1 return ui.text({ 'b' .. b() }) end) }) end";

/// U2.
#[test]
fn only_the_group_whose_signal_changed_runs_again() {
    let app = app_of(TWO);
    assert_eq!(shown(&app), "a1 b1");
    let _ = app.view();
    assert_eq!((runs(&app, "a"), runs(&app, "b")), ("1".into(), "1".into()), "nothing changed");

    said(&app, "a:set(2)");
    assert_eq!(shown(&app), "a2 b1", "the changed group shows its new value, the kept one its old");
    assert_eq!((runs(&app, "a"), runs(&app, "b")), ("2".into(), "1".into()));
}

/// U3: what a kept group handed back still takes clicks, from the same handler.
#[test]
fn a_kept_group_still_takes_clicks() {
    let mut app = app_of(
        "a = signal(1, 'a') b = signal(1, 'b') runs = { a = 0, b = 0 } clicks = 0 \
         return function() return ui.col({ \
           ui.group('ga', function() runs.a += 1 \
             return ui.button({ id = 'inc', on_click = function() clicks += 1 end, 'a' .. a() }) end), \
           ui.group('gb', function() runs.b += 1 return ui.text({ 'b' .. b() }) end) }) end",
    );
    let _ = app.view();
    said(&app, "b:set(2)");
    let mut view = app.view();
    assert_eq!(runs(&app, "a"), "1", "the button's group was kept");
    let click = view.trigger("inc", Action::Click).expect("a kept group keeps its button");
    app.update(click);
    assert_eq!(said(&app, "return clicks"), "1");
    assert!(app.console(10).is_empty(), "{:?}", app.console(10));
}

/// U4: a group that read no signal follows plain state, which cannot say when it changed.
#[test]
fn a_group_that_reads_no_signal_runs_every_view() {
    let app = app_of(
        "n = 0 runs = { g = 0 } \
         return function() return ui.group('g', function() runs.g += 1 return ui.text({ 'n' .. n }) end) end",
    );
    assert_eq!(shown(&app), "n0");
    said(&app, "n = 5");
    assert_eq!(shown(&app), "n5");
    assert_eq!(runs(&app, "g"), "3");
}

/// A group's output is everything under it, so what a group inside it read is read by it too.
#[test]
fn a_changed_inner_group_re_runs_the_group_around_it_and_not_its_neighbours() {
    let app = app_of(
        "x = signal(1, 'x') y = signal(1, 'y') z = signal(1, 'z') runs = { outer = 0, inner = 0, side = 0 } \
         return function() return ui.col({ \
           ui.group('outer', function() runs.outer += 1 local seen = x() \
             return ui.col({ ui.text({ 'x' .. seen }), \
               ui.group('inner', function() runs.inner += 1 return ui.text({ 'y' .. y() }) end) }) end), \
           ui.group('side', function() runs.side += 1 return ui.text({ 'z' .. z() }) end) }) end",
    );
    assert_eq!(shown(&app), "x1 y1 z1");
    said(&app, "x:set(2)");
    assert_eq!(shown(&app), "x2 y1 z1");
    assert_eq!((runs(&app, "outer"), runs(&app, "inner"), runs(&app, "side")), ("2".into(), "1".into(), "1".into()),
        "the outer group changed, the inner one was kept");
    said(&app, "y:set(2)");
    assert_eq!(shown(&app), "x2 y2 z1", "a kept outer group would have shown the old y");
    assert_eq!((runs(&app, "outer"), runs(&app, "inner"), runs(&app, "side")), ("3".into(), "2".into(), "1".into()));
}

/// A group holding something that cannot say when it changed cannot be kept.
#[test]
fn a_group_holding_a_world_or_a_plain_group_is_never_kept() {
    let app = app_of(
        "a = signal(1, 'a') n = 0 runs = { world = 0, plain = 0, outer = 0 } \
         return function() return ui.col({ \
           ui.group('world', function() runs.world += 1 local seen = a() \
             return ui.world({ id = 'map', width = 40, height = 40 }) end), \
           ui.group('outer', function() runs.outer += 1 local seen = a() \
             return ui.group('plain', function() runs.plain += 1 return ui.text({ 'n' .. n }) end) end) }) end",
    );
    let _ = app.view();
    said(&app, "n = 3");
    let _ = app.view();
    assert_eq!(runs(&app, "world"), "3", "a world's frame moves without a signal");
    assert_eq!(runs(&app, "plain"), "3");
    assert_eq!(runs(&app, "outer"), "3", "a group holding one that runs every view runs every view");
}

#[test]
fn a_group_that_leaves_the_view_runs_again_when_it_returns() {
    let app = app_of(
        "show = signal(true, 'show') s = signal(1, 's') runs = { g = 0 } \
         return function() return ui.col({ show() and ui.group('g', function() \
           runs.g += 1 return ui.text({ 's' .. s() }) end) or false }) end",
    );
    let _ = app.view();
    said(&app, "show:set(false)");
    let _ = app.view();
    said(&app, "show:set(true)");
    assert_eq!(shown(&app), "s1");
    assert_eq!(runs(&app, "g"), "2", "a group no view described is forgotten");
}

#[test]
fn two_groups_with_one_id_are_refused() {
    let app = app_of(
        "return function() return ui.col({ \
           ui.group('g', function() return ui.text({ 'one' }) end), \
           ui.group('g', function() return ui.text({ 'two' }) end) }) end",
    );
    let _ = app.view();
    let console = app.console(10).join("\n");
    assert!(console.contains("ui.group") && console.contains("\"g\"") && console.contains("twice"), "{console}");
}

#[test]
fn a_group_writing_a_signal_while_it_describes_is_refused() {
    let app = app_of(
        "a = signal(1, 'a') \
         return function() return ui.group('g', function() a:set(2) return ui.text({ 'x' }) end) end",
    );
    let _ = app.view();
    let console = app.console(10).join("\n");
    assert!(console.contains("only in a handler"), "{console}");
}

/// A chat-shaped list: 300 messages, one signal each, one `ui.group` each or none.
fn chat(grouped: bool) -> LuaApp<LuaMsg> {
    app_of(&format!(
        "msgs = {{}} for i = 1, 300 do msgs[i] = signal({{ who = 'user' .. i, body = 'a message with a few words in it, number ' .. i }}, 'm' .. i) end \
         local function row(i) local m = msgs[i]() \
           return ui.row({{ gap = 8, ui.text({{ m.who, color = '#8899aa' }}), ui.text({{ m.body }}), \
             ui.button({{ id = 'like' .. i, on_click = function() end, 'like' }}) }}) end \
         return function() local rows = {{}} for i = 1, 300 do \
           rows[i] = {grouped} and ui.group('m' .. i, function() return row(i) end) or row(i) end \
           return ui.col({{ gap = 4, rows }}) end"
    ))
}

/// One message edited among 300: the view after it, with and without groups.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_app_ui_groups() {
    for (name, grouped) in [("app_chat_300_edit_grouped", true), ("app_chat_300_edit_plain", false)] {
        let app = chat(grouped);
        let mut n = 0;
        bench(name, 30, || {
            n += 1;
            said(&app, &format!("msgs[150]:set({{ who = 'user150', body = 'edited {n}' }})"));
            let _ = app.view();
        });
    }
}
