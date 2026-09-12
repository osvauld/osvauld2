use super::*;
use mlua::{FromLua, Table};
use std::rc::Rc;

mod reload;
mod require;
mod round_trip;
mod scratch;

// Phase 1's to_msg is the identity — these tests only care that walk builds a tree.
fn identity() -> Rc<dyn Fn(LuaMsg) -> LuaMsg> {
    Rc::new(|m| m)
}

/// No window to repaint. Tests that care about the poke build their own counting waker.
/// A fresh, empty core set. Only `reload` ever hands a populated one in — every other test
/// opens its docs for the first time.
fn no_cores() -> Cores {
    Rc::new(RefCell::new(HashMap::new()))
}

fn noop_wake() -> Wake {
    std::sync::Arc::new(|| {})
}

#[test]
fn vm_runs_lua() {
    let (lua, _) = sandboxed_vm().unwrap();
    let res = lua.load("return 1+2").eval::<i64>();
    assert_eq!(res.unwrap(), 3);
}

#[test]
fn vm_interrupt() {
    let (lua, _) = sandboxed_vm().unwrap();
    let res = lua.load("while true do end").exec();
    assert!(res.is_err(), "inifinite loop should have been killed")
}
#[test]
fn now() {
    let (lua, _) = sandboxed_vm().unwrap();
    let res = lua.load("return now()").eval::<i64>();
    assert!(res.unwrap() > 1);
}

#[test]
fn prelude_tags_tables() {
    let (lua, _) = sandboxed_vm().unwrap();
    let node: Table = lua.load(r#"return ui.col{ui.text{"hi"}}"#).eval().unwrap();
    let tag: String = node.get("tag").unwrap();
    assert_eq!(tag, "col");
    let child: Table = node.get(1).unwrap();
    let ctag: String = child.get("tag").unwrap();
    assert_eq!(ctag, "text");
    let label: String = child.get(1).unwrap();
    assert_eq!(label, "hi");
}

#[test]
fn walk_builds_el() {
    let (lua, _) = sandboxed_vm().unwrap();
    let node: Table = lua
        .load(r#"return ui.col{ui.text{"a"}, ui.row{ui.text{"b"}, ui.text{"c"}}}"#)
        .eval()
        .unwrap();
    let mut handlers: Vec<Function> = Vec::new();
    let mut ctx = Ctx::new(&mut handlers, identity());
    assert!(walk(node, &mut ctx).is_ok());
}
/// Walk one node written in Lua, and report what `props::apply` made of it.
fn walk_props(src: &str) -> mlua::Result<()> {
    let (lua, _) = sandboxed_vm().unwrap();
    let node: Table = lua.load(src).eval().unwrap();
    let mut handlers: Vec<Function> = Vec::new();
    let mut ctx = Ctx::new(&mut handlers, identity());
    walk(node, &mut ctx).map(|_| ())
}

/// `grow` is the one prop that takes two Lua types, so it is the one that cannot go through the
/// `prop!` macro — its arms are keyed on a single type each. The ratio is what a drag between two
/// elastic siblings has to write (docs/design/code-as-tree.md §11); the bool is what every app
/// already says and must keep meaning 1.0. The layout consequences are asserted where the layout
/// is, in `runtime::layout` — this is about the decode.
#[test]
fn grow_takes_a_bool_or_a_ratio() {
    for src in [
        r#"return ui.col{ grow = true }"#,
        r#"return ui.col{ grow = false }"#,
        r#"return ui.col{ grow = 2 }"#,
        r#"return ui.col{ grow = 0.5 }"#,
    ] {
        assert!(walk_props(src).is_ok(), "{src}");
    }
    // Still a decode, not a shrug: a string is neither spelling.
    let err = walk_props(r#"return ui.col{ grow = "wide" }"#)
        .expect_err("a string grow must not be accepted")
        .to_string();
    assert!(err.contains("expected a number"), "{err}");
}

#[test]
fn size_bounds_are_props() {
    assert!(walk_props(r#"return ui.col{ min_w = 100, max_w = 200 }"#).is_ok());
    assert!(walk_props(r#"return ui.col{ min_h = 10, max_h = 20 }"#).is_ok());
}

#[test]
fn walk_collects_handlers() {
    let (lua, _) = sandboxed_vm().unwrap();
    let node: Table = lua
        .load(r#"return ui.button{"add", on_click = function() end }"#)
        .eval()
        .unwrap();

    let mut handlers: Vec<Function> = Vec::new();
    let mut ctx = Ctx::new(&mut handlers, identity());
    let _el = walk(node, &mut ctx).unwrap();
    assert_eq!(handlers.len(), 1);
    assert!(handlers[0].call::<()>(()).is_ok());
}

// ── crdt::patch_into ──────────────────────────────────────────────────────────
// The mirror is rebuilt into the *same* Lua table every time, so every test here
// patches twice: once to fill, once to prove the second pass converges.

use crate::crdt::patch_into;
use loro::{LoroDoc, LoroMap, LoroMovableList, LoroValue};

/// A doc with one map root and one list-of-maps root — the kanban shape in miniature.
fn board(titles: &[&str]) -> LoroDoc {
    let doc = LoroDoc::new();
    let meta = doc.get_map("meta");
    meta.insert("title", "Todo").unwrap();
    meta.insert("count", titles.len() as i64).unwrap();
    let cards = doc.get_movable_list("cards");
    for (i, t) in titles.iter().enumerate() {
        let m = cards.insert_container(i, LoroMap::new()).unwrap();
        m.insert("title", *t).unwrap();
    }
    doc.commit();
    doc
}

#[test]
fn patch_builds_the_tree() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    patch_into(&lua, &t, &board(&["a", "b"]).get_deep_value()).unwrap();
    lua.globals().set("m", &t).unwrap();

    assert_eq!(
        lua.load("return m.meta.title").eval::<String>().unwrap(),
        "Todo"
    );
    // i64 lands as a Lua number, not an integer — Luau has no integer subtype.
    assert_eq!(lua.load("return m.meta.count").eval::<f64>().unwrap(), 2.0);
    assert_eq!(lua.load("return #m.cards").eval::<usize>().unwrap(), 2);
    assert_eq!(
        lua.load("return m.cards[2].title")
            .eval::<String>()
            .unwrap(),
        "b"
    );
}

#[test]
fn patch_shrinks_a_list() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    patch_into(&lua, &t, &board(&["a", "b", "c"]).get_deep_value()).unwrap();

    // Delete from the *middle*: survivors shift left, so it is always the tail that goes stale.
    let doc = board(&["a", "c"]);
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();
    lua.globals().set("m", &t).unwrap();

    assert_eq!(lua.load("return #m.cards").eval::<usize>().unwrap(), 2);
    assert_eq!(
        lua.load("return m.cards[2].title")
            .eval::<String>()
            .unwrap(),
        "c"
    );
    assert_eq!(
        lua.load("return m.cards[3]").eval::<Value>().unwrap(),
        Value::Nil
    );
}

#[test]
fn patch_shrinks_a_map() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    let doc = board(&[]);
    doc.get_map("meta").insert("temp", "x").unwrap();
    doc.commit();
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    doc.get_map("meta").delete("temp").unwrap();
    doc.commit();
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();
    lua.globals().set("m", &t).unwrap();

    assert_eq!(
        lua.load("return m.meta.temp").eval::<Value>().unwrap(),
        Value::Nil
    );
    assert_eq!(
        lua.load("return m.meta.title").eval::<String>().unwrap(),
        "Todo"
    );
}

/// The whole reason `patch_into` exists: a reference Lua captured before the patch must still
/// see the new data afterwards. Rebuilding the table instead would leave `cards` pointing at
/// a detached copy that silently never updates again.
#[test]
fn patch_keeps_table_identity() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    patch_into(&lua, &t, &board(&["a"]).get_deep_value()).unwrap();
    lua.globals().set("m", &t).unwrap();
    lua.load("held = m.cards").exec().unwrap();

    patch_into(&lua, &t, &board(&["a", "b"]).get_deep_value()).unwrap();

    assert!(
        lua.load("return rawequal(held, m.cards)")
            .eval::<bool>()
            .unwrap()
    );
    assert_eq!(
        lua.load("return held[2].title").eval::<String>().unwrap(),
        "b"
    );
}

/// A doc with one nested container under `data.items`, built by `f`.
fn nested(f: impl FnOnce(&LoroMap)) -> LoroDoc {
    let doc = LoroDoc::new();
    let data = doc.get_map("data");
    f(&data);
    doc.commit();
    doc
}

/// Loro cannot change a container's type in place, but deleting a key and inserting a
/// different container under the same name is an ordinary edit — and one a peer can make
/// without this app ever seeing the intermediate state. `patch_into` then finds a Lua table
/// already sitting at that key and reuses it, because `child` asks only "is there a table
/// here?" and never "of the right kind".
#[test]
fn a_map_replaced_by_a_list_drops_the_old_keys() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    let doc = nested(|d| {
        let m = d.insert_container("items", LoroMap::new()).unwrap();
        m.insert("a", 1).unwrap();
    });
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    doc.get_map("data").delete("items").unwrap();
    let l = doc
        .get_map("data")
        .insert_container("items", LoroMovableList::new())
        .unwrap();
    l.insert(0, 10).unwrap();
    l.insert(1, 20).unwrap();
    doc.commit();
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    lua.globals().set("m", &t).unwrap();
    assert_eq!(lua.load("return #m.data.items").eval::<usize>().unwrap(), 2);
    assert_eq!(
        lua.load("return m.data.items.a").eval::<Value>().unwrap(),
        Value::Nil,
        "the map's key survived into a list"
    );
}

/// The mirror image. The map arm's stale sweep iterates `pairs::<mlua::String, _>` and
/// `filter_map`s the failures away, so an integer key is dropped from consideration *before*
/// it is ever judged stale.
#[test]
fn a_list_replaced_by_a_map_drops_the_old_indices() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    let doc = nested(|d| {
        let l = d.insert_container("items", LoroMovableList::new()).unwrap();
        l.insert(0, 10).unwrap();
        l.insert(1, 20).unwrap();
    });
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    doc.get_map("data").delete("items").unwrap();
    let m = doc
        .get_map("data")
        .insert_container("items", LoroMap::new())
        .unwrap();
    m.insert("a", 1).unwrap();
    doc.commit();
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    lua.globals().set("m", &t).unwrap();
    assert_eq!(lua.load("return m.data.items.a").eval::<i64>().unwrap(), 1);
    assert_eq!(
        lua.load("return m.data.items[1]").eval::<Value>().unwrap(),
        Value::Nil,
        "the list's elements survived into a map"
    );
}

/// A shorter list of maps must not leave the tail's *tables* behind either — the truncation
/// loop keyed on `raw_len()` only ever removed a contiguous tail, which is correct for a list
/// but says nothing about a table that also picked up string keys along the way.
#[test]
fn a_list_sweep_removes_a_stray_string_key() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    patch_into(&lua, &t, &board(&["a"]).get_deep_value()).unwrap();

    // Something a Map→List flip leaves behind, planted directly.
    lua.globals().set("m", &t).unwrap();
    lua.load("m.cards.leftover = true").exec().unwrap();

    patch_into(&lua, &t, &board(&["a", "b"]).get_deep_value()).unwrap();
    assert_eq!(
        lua.load("return m.cards.leftover").eval::<Value>().unwrap(),
        Value::Nil
    );
}

/// Lua has no `null`, so a Loro null and an absent key mirror to the same thing: nothing.
/// Fine in a map — `m.meta.temp` is nil either way — but in a *list* it punches a hole, and a
/// Lua array with a hole has no defined length. Pinned rather than fixed: the only honest
/// alternatives are a sentinel value leaking into app code or an error on import, and neither
/// is obviously right until something actually writes one.
#[test]
fn a_null_in_a_list_leaves_a_hole() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    let doc = nested(|d| {
        let l = d.insert_container("items", LoroMovableList::new()).unwrap();
        l.insert(0, 10).unwrap();
        l.insert(1, LoroValue::Null).unwrap();
        l.insert(2, 30).unwrap();
    });
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    lua.globals().set("m", &t).unwrap();
    assert_eq!(
        lua.load("return m.data.items[1]").eval::<i64>().unwrap(),
        10
    );
    assert_eq!(
        lua.load("return m.data.items[2]").eval::<Value>().unwrap(),
        Value::Nil,
        "the null itself"
    );
    assert_eq!(
        lua.load("return m.data.items[3]").eval::<i64>().unwrap(),
        30
    );

    // The survivor of the sweep: index 3 is inside `1..=len`, so it is kept even though the
    // hole at 2 makes `#` unable to reach it.
    let n = lua.load("return #m.data.items").eval::<usize>().unwrap();
    assert!(
        n == 1 || n == 3,
        "a hole makes the border arbitrary, got {n}"
    );
}

/// The map half of the same thing, and the one that matters today: null and absent are
/// indistinguishable, which is what lets `patch_shrinks_a_map` and this share one code path.
#[test]
fn a_null_in_a_map_reads_as_absent() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    let doc = nested(|d| {
        let m = d.insert_container("items", LoroMap::new()).unwrap();
        m.insert("a", LoroValue::Null).unwrap();
        m.insert("b", 2).unwrap();
    });
    patch_into(&lua, &t, &doc.get_deep_value()).unwrap();

    lua.globals().set("m", &t).unwrap();
    assert_eq!(
        lua.load("return m.data.items.a").eval::<Value>().unwrap(),
        Value::Nil
    );
    assert_eq!(lua.load("return m.data.items.b").eval::<i64>().unwrap(), 2);
}

#[test]
fn patch_rejects_a_scalar_root() {
    let (lua, _) = sandboxed_vm().unwrap();
    let t = lua.create_table().unwrap();
    assert!(patch_into(&lua, &t, &LoroValue::from("nope")).is_err());
}

// ── crdt::install / doc:open ──────────────────────────────────────────────────

use crate::crdt::install;
use loro::{ExportMode, LoroText};

/// A resolver that serves `snapshot` under one name and nothing else — the shape shell2
/// builds around `Vault::get_doc`, minus the vault.
fn serving(name: &'static str, snapshot: Vec<u8>) -> Resolve {
    Rc::new(move |n: &str| Ok((n == name).then(|| snapshot.clone())))
}

fn snapshot_of(doc: &LoroDoc) -> Vec<u8> {
    doc.export(ExportMode::Snapshot).unwrap()
}

#[test]
fn open_imports_and_mirrors() {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        serving("board", snapshot_of(&board(&["a", "b"]))),
        noop_wake(),
    )
    .unwrap();

    let got: String = lua
        .load(r#"local b = doc:open("board") return b.cards[2].title .. "/" .. b.meta.title"#)
        .eval()
        .unwrap();
    assert_eq!(got, "b/Todo");
    assert_eq!(docs.borrow().len(), 1);
}

/// Apps call `open` every frame, so the warm path must hand back the *same* table — a fresh
/// one each time would detach anything Lua captured and silently stop updating.
#[test]
fn open_is_idempotent() {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        serving("board", snapshot_of(&board(&["a"]))),
        noop_wake(),
    )
    .unwrap();

    let same: bool = lua
        .load(r#"return rawequal(doc:open("board"), doc:open("board"))"#)
        .eval()
        .unwrap();
    assert!(same);
    assert_eq!(docs.borrow().len(), 1);
}

/// Every app's first run resolves nothing, so a miss is an empty doc, not an error.
#[test]
fn open_missing_doc_is_empty_not_an_error() {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        Rc::new(|_| Ok(None)),
        noop_wake(),
    )
    .unwrap();

    let n: usize = lua
        .load(r#"local b = doc:open("fresh") local n = 0 for _ in pairs(b) do n = n + 1 end return n"#)
        .eval()
        .unwrap();
    assert_eq!(n, 0);
    assert_eq!(docs.borrow().len(), 1);
}

#[test]
fn open_reports_a_corrupt_snapshot() {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        serving("board", b"not a snapshot".to_vec()),
        noop_wake(),
    )
    .unwrap();

    assert!(
        lua.load(r#"return doc:open("board")"#)
            .eval::<Table>()
            .is_err()
    );
    assert_eq!(
        docs.borrow().len(),
        0,
        "a failed import must not leave an entry"
    );
}

/// A vault *read failure* is not "no doc yet". It has to surface, because the alternative is
/// an empty board that the next flush then writes over the real one.
#[test]
fn open_reports_a_resolver_failure() {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        Rc::new(|_| Err("vault locked".to_string())),
        noop_wake(),
    )
    .unwrap();

    assert!(
        lua.load(r#"return doc:open("board")"#)
            .eval::<Table>()
            .is_err()
    );
    assert_eq!(
        docs.borrow().len(),
        0,
        "a failed resolve must not leave an entry"
    );
}

/// End to end through `LuaApp`: `install` has to run before `eval`, because apps open docs at
/// module scope. The Lua `assert` is the real check — a wrong mirror surfaces as `app.error`.
#[test]
fn app_opens_a_doc_at_module_scope() {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(
        0,
        r#"
        local b = doc:open("board")
        assert(b.cards[2].title == "b", "mirror is wrong")
        return function() return ui.text{ b.cards[2].title } end
        "#,
    )
    .unwrap();
    src.commit();

    let app = LuaApp::open(
        src,
        serving("board", snapshot_of(&board(&["a", "b"]))),
        noop_wake(),
        identity(),
    )
    .unwrap();
    assert_eq!(app.error, None);
    assert_eq!(app.docs.borrow().len(), 1);
}

// ── change detection ──────────────────────────────────────────────────────────

/// An app that opens `board` at module scope and renders one card.
fn board_app_waking(resolve: Resolve, wake: Wake) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(
        0,
        r#"
        local b = doc:open("board")
        return function() return ui.text{ b.cards[1].title } end
        "#,
    )
    .unwrap();
    src.commit();
    LuaApp::open(src, resolve, wake, identity()).unwrap()
}

fn board_app(resolve: Resolve) -> LuaApp<LuaMsg> {
    board_app_waking(resolve, noop_wake())
}

fn cards_len(app: &LuaApp<LuaMsg>) -> usize {
    let docs = app.docs.borrow();
    let mirror = &docs.get("board").unwrap().mirror;
    mirror.get::<Table>("cards").unwrap().raw_len()
}

/// The half of the design that fails invisibly: a write that did not come from Lua still has
/// to reach the mirror. Here the writer is this test holding a reference clone of the doc —
/// exactly what the MCP bridge and peer sync will be.
#[test]
fn external_write_reaches_the_mirror_on_next_view() {
    let app = board_app(serving("board", snapshot_of(&board(&["a", "b"]))));
    assert_eq!(cards_len(&app), 2);

    // `LoroDoc::clone` is a reference clone, so this is the app's own doc, not a fork.
    let doc = app.docs.borrow().get("board").unwrap().core.doc.clone();
    let extra = doc
        .get_movable_list("cards")
        .insert_container(2, LoroMap::new())
        .unwrap();
    extra.insert("title", "c").unwrap();
    doc.commit();

    // The subscriber has fired, but nothing has patched yet — the mirror is deliberately stale.
    assert_eq!(cards_len(&app), 2);
    assert_eq!(
        app.docs
            .borrow()
            .get("board")
            .unwrap()
            .core
            .version
            .load(Ordering::Relaxed),
        1
    );

    let _ = app.view();
    assert_eq!(cards_len(&app), 3);
}

/// The gate is `version > mirrored`, so a view with no writes behind it must not re-patch.
#[test]
fn view_without_a_write_does_not_repatch() {
    let app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    let _ = app.view();
    let e = app.docs.borrow();
    let e = e.get("board").unwrap();
    assert_eq!(e.core.version.load(Ordering::Relaxed), 0);
    assert_eq!(e.mirrored, 0);
}

/// Several commits between two frames collapse into a single patch — the counter coalesces.
#[test]
fn many_writes_are_one_patch() {
    let app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    let doc = app.docs.borrow().get("board").unwrap().core.doc.clone();
    for t in ["b", "c", "d"] {
        let m = doc
            .get_movable_list("cards")
            .push_container(LoroMap::new())
            .unwrap();
        m.insert("title", t).unwrap();
        doc.commit();
    }
    assert_eq!(
        app.docs
            .borrow()
            .get("board")
            .unwrap()
            .core
            .version
            .load(Ordering::Relaxed),
        3
    );

    let _ = app.view();
    let docs = app.docs.borrow();
    let e = docs.get("board").unwrap();
    assert_eq!(e.mirrored, 3, "one patch, catching up all three commits");
    drop(docs);
    assert_eq!(cards_len(&app), 4);
}

// ── the repaint poke ──────────────────────────────────────────────────────────
// The version counter says the mirror is stale; it does not make anyone look. A click already
// carries a frame with it, so the only writes at risk are the ones nobody clicked for.

/// A waker that counts instead of repainting.
fn counting_wake() -> (Wake, std::sync::Arc<AtomicU64>) {
    let hits = std::sync::Arc::new(AtomicU64::new(0));
    let n = hits.clone();
    (
        std::sync::Arc::new(move || {
            n.fetch_add(1, Ordering::Relaxed);
        }),
        hits,
    )
}

/// A peer's update, as an import would deliver it.
fn update_adding(title: &str, base: &LoroDoc) -> Vec<u8> {
    let peer = LoroDoc::new();
    peer.import(&snapshot_of(base)).unwrap();
    let m = peer
        .get_movable_list("cards")
        .push_container(LoroMap::new())
        .unwrap();
    m.insert("title", title).unwrap();
    peer.commit();
    peer.export(ExportMode::Snapshot).unwrap()
}

/// The whole point: a write that arrives while the window sits idle has to ask for a frame.
/// Without this the doc changes, the counter moves, and the screen keeps showing the old board
/// until the user happens to move the mouse.
#[test]
fn an_import_asks_for_a_frame() {
    let base = board(&["a"]);
    let (wake, hits) = counting_wake();
    let app = board_app_waking(serving("board", snapshot_of(&base)), wake);
    assert_eq!(hits.load(Ordering::Relaxed), 0, "opening is not a change");

    let doc = app.docs.borrow().get("board").unwrap().core.doc.clone();
    doc.import(&update_adding("b", &base)).unwrap();

    assert_eq!(hits.load(Ordering::Relaxed), 1);
    assert_eq!(
        cards_len(&app),
        1,
        "still stale — the poke asks, view() patches"
    );
    let _ = app.view();
    assert_eq!(cards_len(&app), 2);
}

/// The gate, and the reason there is one. A local write happens *inside* a frame the host
/// already scheduled, so poking from here would queue a second frame for every keystroke — a
/// repaint loop that never settles while anyone is typing.
#[test]
fn a_local_write_does_not_ask_for_a_frame() {
    let (wake, hits) = counting_wake();
    let app = board_app_waking(serving("board", snapshot_of(&board(&["a"]))), wake);

    let doc = app.docs.borrow().get("board").unwrap().core.doc.clone();
    let m = doc
        .get_movable_list("cards")
        .push_container(LoroMap::new())
        .unwrap();
    m.insert("title", "b").unwrap();
    doc.commit();

    assert_eq!(
        app.docs
            .borrow()
            .get("board")
            .unwrap()
            .core
            .version
            .load(Ordering::Relaxed),
        1,
        "the counter still moves — only the poke is gated"
    );
    assert_eq!(hits.load(Ordering::Relaxed), 0);
}

/// One poke per import, not one per changed container: `subscribe_root` fires once for the
/// whole batch, so a peer landing twenty cards costs one frame, not twenty.
#[test]
fn a_batch_of_changes_is_one_poke() {
    let base = board(&["a"]);
    let (wake, hits) = counting_wake();
    let app = board_app_waking(serving("board", snapshot_of(&base)), wake);

    let peer = LoroDoc::new();
    peer.import(&snapshot_of(&base)).unwrap();
    for t in ["b", "c", "d"] {
        let m = peer
            .get_movable_list("cards")
            .push_container(LoroMap::new())
            .unwrap();
        m.insert("title", t).unwrap();
        peer.commit();
    }
    let doc = app.docs.borrow().get("board").unwrap().core.doc.clone();
    doc.import(&peer.export(ExportMode::Snapshot).unwrap())
        .unwrap();

    assert_eq!(hits.load(Ordering::Relaxed), 1);
    let _ = app.view();
    assert_eq!(cards_len(&app), 4);
}

// ── cost curve ────────────────────────────────────────────────────────────────
// How many elements can a frame afford? Times the two halves of the Lua path
// *separately* — building node tables in Luau, then `walk`ing them into `El`s —
// because they have different fixes: the first is helped by fewer/flatter tables,
// the second by `props::apply`'s linear scan or by retaining the tree. A combined
// number can't tell you which to reach for.
//
// Headless: no window, no vello, no taffy. This measures the boundary only.
//
//   cargo test -p app_host --release cost_curve -- --nocapture --ignored
//
// Debug numbers are meaningless here (mlua and Luau are both ~10x slower unopt),
// so the test says so rather than letting you read a fake ceiling.

use std::time::{Duration, Instant};

/// A flat list: N siblings, one prop-light leaf each. Isolates per-element cost.
const FLAT_APP: &str = r##"
local C = { bg = "#0d1117", text = "#e6edf3" }

return function()
	local kids = { pad = 16, gap = 2, fill = C.bg }
	for i = 1, N do
		kids[#kids + 1] = ui.text({ "item " .. i, color = C.text, font_size = 13 })
	end
	return ui.col(kids)
end
"##;

/// A table: ROWS x COLS cells, each a styled container wrapping a text leaf.
/// This is the shape that actually matters — six props per cell, one level of
/// nesting per row, and two `El`s per cell.
const TABLE_APP: &str = r##"
local C = { bg = "#0d1117", cell = "#161b22", line = "#30363d", text = "#e6edf3" }

local function cell(r, c)
	return ui.col({
		w = 120, h = 28, px = 8, py = 6, radius = 4,
		fill = C.cell, hover_fill = C.line,
		ui.text({ "r" .. r .. "c" .. c, color = C.text, font_size = 13 }),
	})
end

return function()
	local rows = { pad = 16, gap = 4, fill = C.bg }
	for r = 1, ROWS do
		local cells = { gap = 4 }
		for c = 1, COLS do
			cells[#cells + 1] = cell(r, c)
		end
		rows[#rows + 1] = ui.row(cells)
	end
	return ui.col(rows)
end
"##;

/// One measured frame. Returns nothing — this prints a row of the curve.
///
/// `els` is what the app *intends* to build; if the interrupt budget kills the
/// frame first, that's the finding, so it's reported rather than unwrapped.
/// The probe body: identical shape, N leaves, only the prop list varying. Used by the colour
/// probe (same prop *count*, differing types) and by the id probe (one prop more).
fn probe_app(props: &str) -> String {
    const T: &str = r##"
local C = { bg = "#0d1117", cell = "#161b22", text = "#e6edf3" }

return function()
	local kids = { pad = 16, gap = 2, fill = C.bg }
	for i = 1, N do
		kids[#kids + 1] = ui.text({ "item " .. i, @PROPS@ })
	end
	return ui.col(kids)
end
"##;
    T.replace("@PROPS@", props)
}

/// One point on the curve, measured `REPS` times.
///
/// Reports the *minimum*, not the mean: every source of error here is additive (scheduler
/// preemption, thermal drift, another process touching the cache), so the fastest run is
/// the least-contaminated estimate of the real cost. An earlier single-shot version of this
/// test drifted ~20% between runs — enough to swamp the changes being measured, and enough
/// to read a regression that wasn't there.
///
/// The `lua` column is the control: no change to `walk` or `props` can affect it, so if it
/// moves between two runs, the machine moved and the comparison is void.
fn frame(label: &str, header: &str, app: &str, els: usize, dev: bool) {
    const REPS: usize = 9;

    let (lua, fires) = sandboxed_vm().unwrap();
    let view: Function = match lua.load(format!("{header}\n{app}")).eval() {
        Ok(f) => f,
        Err(e) => return eprintln!("{label:<14} {els:>7}  setup failed: {e}"),
    };

    // Warm frame: the first call pays for string interning and table growth a steady-state
    // frame doesn't. Discarded, along with its budget spend.
    if let Err(e) = view.call::<Table>(()) {
        return eprintln!("{label:<14} {els:>7}  {e}");
    }

    let (mut lua_t, mut walk_t) = (Duration::MAX, Duration::MAX);
    let (mut spent, mut ok) = (0u64, true);

    for _ in 0..REPS {
        fires.store(0, Ordering::Relaxed); // also refills the budget, exactly as view() does
        let t0 = Instant::now();
        let node: Table = match view.call(()) {
            Ok(n) => n,
            Err(e) => return eprintln!("{label:<14} {els:>7}  {e}"),
        };
        lua_t = lua_t.min(t0.elapsed());
        spent = fires.load(Ordering::Relaxed);

        // Fresh handlers per rep — the vec grows as callbacks are collected.
        let mut handlers: Vec<Function> = Vec::new();
        let mut ctx = Ctx::new(&mut handlers, identity());
        ctx.dev = dev;
        let t1 = Instant::now();
        let built = walk(node, &mut ctx);
        walk_t = walk_t.min(t1.elapsed());
        ok &= built.is_ok();
    }

    let total = lua_t + walk_t;
    eprintln!(
        "{label:<14} {els:>7}  lua {:>8.2?}  walk {:>8.2?}  total {:>8.2?}  {:>6.0}ns/el  \
         budget {:>5.1}%  {}",
        lua_t,
        walk_t,
        total,
        total.as_nanos() as f64 / els as f64,
        spent as f64 / 10_000.0, // as a % of the 1M ceiling
        if ok { "ok" } else { "walk failed" },
    );
}

#[test]
#[ignore = "measurement, not an assertion — needs --release --nocapture"]
fn cost_curve() {
    if cfg!(debug_assertions) {
        eprintln!("\n!! debug build — these numbers are ~10x pessimistic. Use --release.\n");
    }
    eprintln!("\n16.7ms is one frame at 60fps; budget% is of the 1M interrupt ceiling.\n");

    for dev in [true, false] {
        eprintln!("\n================ dev = {dev} ================");

        eprintln!("-- flat: N text leaves --------------------------------------");
        for n in [100usize, 500, 1_000, 5_000, 10_000, 20_000, 50_000] {
            frame("flat", &format!("local N = {n}"), FLAT_APP, n + 1, dev);
        }

        eprintln!("\n-- table: ROWS x 8 cols, 2 els per cell ---------------------");
        for rows in [10usize, 50, 100, 500, 1_000, 2_000] {
            let cols = 8;
            frame(
                "table",
                &format!("local ROWS, COLS = {rows}, {cols}"),
                TABLE_APP,
                rows * cols * 2 + rows + 1,
                dev,
            );
        }
    }

    // Both probes run once, at dev = false, so the breadcrumb doesn't dilute the slope.
    eprintln!("\n================ colour probe (dev = false) ================");
    eprintln!("same element count, same 2 props each — only the prop *types* differ\n");
    for n in [1_000usize, 10_000] {
        for (name, props) in [
            ("0 colours", "font_size = 13, opacity = 1.0"),
            ("1 colour", "font_size = 13, color = C.text"),
            ("2 colours", "fill = C.cell, color = C.text"),
        ] {
            frame(
                name,
                &format!("local N = {n}"),
                &probe_app(props),
                n + 1,
                false,
            );
        }
        eprintln!();
    }

    // Id probe: what one more identity string per element costs. `id` stands in for `_nid`
    // (docs/design/nid-channel.md §8.1) because `walk` already reads it the way `_nid` would be
    // read — `node.get::<Option<String>>` at lib.rs:702, then an `Arc<str>` on the `El`.
    //
    // The two `id` rows differ in where the *Lua* string comes from, and that is the point. A
    // nid is a literal the printer wrote into the chunk, so Luau interns it once at load and
    // hands back the same object every frame; only the Rust side allocates. An author's
    // `"k" .. i` is built fresh per element per frame. `const` is the row that bounds `_nid`;
    // `unique` is there to show how much of the cost is the concatenation rather than the
    // boundary, so the two are not confused for each other.
    eprintln!("\n================ id probe (dev = false) ================");
    eprintln!("one *more* prop, not a swapped one — the marginal cost of an identity string\n");
    for n in [1_000usize, 10_000] {
        for (name, props) in [
            ("no id", "font_size = 13, opacity = 1.0"),
            ("+ const id", "font_size = 13, opacity = 1.0, id = \"k3f9\""),
            (
                "+ unique id",
                "font_size = 13, opacity = 1.0, id = \"k\" .. i",
            ),
        ] {
            frame(
                name,
                &format!("local N = {n}"),
                &probe_app(props),
                n + 1,
                false,
            );
        }
        eprintln!();
    }
}

// ── scan_for_id ───────────────────────────────────────────────────────────────
// Needs `scan_for_id` to be `pub(crate)` (it is a bare `fn` today).

use crate::crdt::scan_for_id;

/// A doc holding `cards` as a MovableList of `{id = ...}` maps.
fn id_list(ids: &[&str]) -> LoroDoc {
    let doc = LoroDoc::new();
    let list = doc.get_movable_list("cards");
    for (i, id) in ids.iter().enumerate() {
        let m = list.insert_container(i, LoroMap::new()).unwrap();
        m.insert("id", *id).unwrap();
    }
    doc.commit();
    doc
}

#[test]
fn scan_for_id_returns_the_position() {
    let doc = id_list(&["k1", "k2", "k3"]);
    let list = doc.get_movable_list("cards");
    assert_eq!(scan_for_id(&list, "k1"), Some(0));
    assert_eq!(scan_for_id(&list, "k3"), Some(2));
}

/// The whole reason the function exists. An id is a stable name; an index is only where the
/// element happens to sit *now*. A peer inserting at the front shifts every position and no
/// id — resolving a click by remembered index instead would edit the neighbouring card.
#[test]
fn scan_for_id_tracks_the_element_not_the_slot() {
    let doc = id_list(&["k1", "k2"]);
    assert_eq!(scan_for_id(&doc.get_movable_list("cards"), "k2"), Some(1));

    let list = doc.get_movable_list("cards");
    let m = list.insert_container(0, LoroMap::new()).unwrap();
    m.insert("id", "k0").unwrap();
    doc.commit();

    assert_eq!(scan_for_id(&doc.get_movable_list("cards"), "k2"), Some(2));
}

#[test]
fn scan_for_id_misses_are_none_not_errors() {
    let doc = id_list(&["k1"]);
    assert_eq!(scan_for_id(&doc.get_movable_list("cards"), "nope"), None);

    let empty = LoroDoc::new();
    assert_eq!(scan_for_id(&empty.get_movable_list("cards"), "k1"), None);
}

/// A list may legally hold scalars, maps with no id, and maps whose id is not a string.
/// None of those is an error — they simply cannot match, so the scan steps over them.
#[test]
fn scan_for_id_skips_elements_that_cannot_match() {
    let doc = LoroDoc::new();
    let list = doc.get_movable_list("cards");
    list.insert(0, "a bare string").unwrap();
    list.insert_container(1, LoroMap::new()).unwrap();
    let wrong_type = list.insert_container(2, LoroMap::new()).unwrap();
    wrong_type.insert("id", 42i64).unwrap();
    let wanted = list.insert_container(3, LoroMap::new()).unwrap();
    wanted.insert("id", "k1").unwrap();
    doc.commit();

    assert_eq!(scan_for_id(&doc.get_movable_list("cards"), "k1"), Some(3));
}

// ── persistence: LuaApp::flush ────────────────────────────────────────────────
// Assumes:
//   pub fn flush(&mut self, put: impl FnMut(&str, &[u8]) -> Result<(), String>)
//       -> Result<(), String>
// and a `saved: u64` watermark on `DocEntry`, beside `mirrored`.

/// Writes straight to the doc rather than through `:set`, so these stay valid while the Lua
/// write API is still being designed. This is also what MCP and peer writes will look like.
fn add_card(app: &LuaApp<LuaMsg>, title: &str) {
    let docs = app.docs.borrow();
    let doc = &docs.get("board").unwrap().core.doc;
    let cards = doc.get_movable_list("cards");
    let m = cards.insert_container(cards.len(), LoroMap::new()).unwrap();
    m.insert("title", title).unwrap();
    doc.commit();
}

fn card_title(app: &LuaApp<LuaMsg>, i: usize) -> String {
    let docs = app.docs.borrow();
    let mirror = &docs.get("board").unwrap().mirror;
    let cards: Table = mirror.get("cards").unwrap();
    cards.get::<Table>(i).unwrap().get("title").unwrap()
}

/// Everything `flush` handed to the vault.
#[derive(Default)]
struct Puts(RefCell<Vec<(String, Vec<u8>)>>);

impl Puts {
    fn take(&self) -> Vec<(String, Vec<u8>)> {
        self.0.borrow_mut().drain(..).collect()
    }
    fn recorder(&self) -> impl FnMut(&str, &[u8]) -> Result<(), String> + '_ {
        move |name, bytes| {
            self.0.borrow_mut().push((name.to_string(), bytes.to_vec()));
            Ok(())
        }
    }
}

#[test]
fn flush_writes_a_dirty_doc() {
    let mut app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    add_card(&app, "b");

    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();

    let got = puts.take();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "board", "the doc's name is the storage key");
    assert!(!got[0].1.is_empty());
}

/// The watermark is the entire point — a flush with no write behind it must not touch the
/// vault. Without it, every frame rewrites the whole snapshot.
#[test]
fn flush_without_a_write_does_nothing() {
    let mut app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    assert_eq!(puts.take().len(), 0);
}

#[test]
fn flush_is_idempotent() {
    let mut app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    add_card(&app, "b");

    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    assert_eq!(puts.take().len(), 1);

    app.flush(puts.recorder()).unwrap();
    assert_eq!(puts.take().len(), 0, "nothing written since the last flush");
}

/// A vault failure must leave the doc dirty. Advancing `saved` before the write succeeds loses
/// the change permanently, and it presents as "my card vanished after a restart".
#[test]
fn a_failed_put_does_not_advance_the_watermark() {
    let mut app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    add_card(&app, "b");

    assert!(app.flush(|_, _| Err("disk full".to_string())).is_err());

    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    assert_eq!(
        puts.take().len(),
        1,
        "a failed write must be retried, not dropped"
    );
}

/// "Can we save a card?", end to end, minus the vault: write it, flush it, and hand the bytes
/// back to a fresh app exactly as the resolver would on the next launch.
#[test]
fn a_card_survives_a_restart() {
    let mut app = board_app(serving("board", snapshot_of(&board(&["a"]))));
    add_card(&app, "b");

    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    let bytes = puts.take().pop().unwrap().1;
    drop(app);

    let restarted = board_app(serving("board", bytes));
    assert_eq!(cards_len(&restarted), 2);
    assert_eq!(card_title(&restarted, 2), "b");
}

// ── resolve_path ──────────────────────────────────────────────────────────────

use crate::crdt::{Seg, resolve_path};
use loro::Index;

/// `meta` (a map) and `cards` (a MovableList of `{id, title}` maps) — enough to exercise both
/// segment rules from one doc.
fn pathed() -> LoroDoc {
    let doc = LoroDoc::new();
    doc.get_map("meta").insert("title", "Todo").unwrap();
    let cards = doc.get_movable_list("cards");
    for (i, id) in ["k1", "k2"].iter().enumerate() {
        let m = cards.insert_container(i, LoroMap::new()).unwrap();
        m.insert("id", *id).unwrap();
        let t = format!("card {id}");
        m.insert("title", t.as_str()).unwrap();
    }
    doc.commit();
    doc
}

fn name(s: &str) -> Seg {
    Seg::Name(s.to_string())
}

#[test]
fn resolve_path_finds_a_map_key() {
    let doc = pathed();
    let (parent, i) = resolve_path(&doc, &[name("meta"), name("title")]).unwrap();
    assert!(matches!(parent, Container::Map(_)));
    assert_eq!(i, Index::Key("title".into()));
}

/// A string means "key" on a map and "element id" on a list. Same segment, different rule,
/// decided by what it is being looked up in.
#[test]
fn resolve_path_reads_a_string_as_an_id_inside_a_list() {
    let doc = pathed();
    let (parent, i) = resolve_path(&doc, &[name("cards"), name("k2")]).unwrap();
    assert!(matches!(parent, Container::MovableList(_)));
    assert_eq!(i, Index::Seq(1));

    // One level deeper: the parent is now k2's own map.
    let (parent, i) = resolve_path(&doc, &[name("cards"), name("k2"), name("title")]).unwrap();
    assert!(matches!(parent, Container::Map(_)));
    assert_eq!(i, Index::Key("title".into()));
}

/// Lua counts from 1, Loro from 0. An off-by-one here edits the neighbouring card and reports
/// nothing, so it gets its own test.
#[test]
fn resolve_path_converts_lua_positions_to_loro_ones() {
    let doc = pathed();
    let (_, i) = resolve_path(&doc, &[name("cards"), Seg::Pos(1)]).unwrap();
    assert_eq!(i, Index::Seq(0));
    let (_, i) = resolve_path(&doc, &[name("cards"), Seg::Pos(2)]).unwrap();
    assert_eq!(i, Index::Seq(1));

    // 0 is not a Lua index — without the guard it would underflow to usize::MAX.
    assert!(resolve_path(&doc, &[name("cards"), Seg::Pos(0)]).is_err());
}

/// The point of leaving the final segment unresolved: `:set` has to be able to create a key
/// that isn't there yet.
#[test]
fn resolve_path_does_not_require_the_last_segment_to_exist() {
    let doc = pathed();
    let (parent, i) = resolve_path(&doc, &[name("meta"), name("brand_new")]).unwrap();
    assert!(matches!(parent, Container::Map(_)));
    assert_eq!(i, Index::Key("brand_new".into()));
}

/// Everything before the last segment must exist — no auto-vivification, so a typo is an error
/// rather than a silently invented subtree.
#[test]
fn resolve_path_requires_intermediates_to_exist() {
    let doc = pathed();
    let err = resolve_path(&doc, &[name("cards"), name("nope"), name("title")]).unwrap_err();
    assert!(err.to_string().contains("no element with id"), "{err}");

    let err = resolve_path(&doc, &[name("absent"), name("x")]).unwrap_err();
    assert!(err.to_string().contains("does not exist"), "{err}");
}

/// "Missing" and "there, but a scalar" are different mistakes and say so.
#[test]
fn resolve_path_rejects_a_scalar_as_a_parent() {
    let doc = pathed();
    let err = resolve_path(&doc, &[name("meta"), name("title"), name("x")]).unwrap_err();
    assert!(
        err.to_string().contains("is a value, not a container"),
        "{err}"
    );
}

#[test]
fn resolve_path_rejects_paths_too_short_to_address_anything() {
    let doc = pathed();
    assert!(resolve_path(&doc, &[]).is_err());
    // A root is always a container, so there is nothing a one-segment path could set.
    assert!(resolve_path(&doc, &[name("meta")]).is_err());
    // Roots are named, never positional.
    assert!(resolve_path(&doc, &[Seg::Pos(1), name("x")]).is_err());
}

#[test]
fn resolve_path_rejects_a_position_inside_a_map() {
    let doc = pathed();
    let err = resolve_path(&doc, &[name("meta"), Seg::Pos(1)]).unwrap_err();
    assert!(err.to_string().contains("not by position"), "{err}");
}

// ── the write path ────────────────────────────────────────────────────────────

/// A VM with `doc` installed over one empty doc named `board`, plus the doc itself so a test
/// can check what Lua actually produced rather than what the mirror says it produced.
fn writing() -> (Lua, LoroDoc) {
    let (lua, _) = sandboxed_vm().unwrap();
    let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
    install(
        &lua,
        docs.clone(),
        no_cores(),
        Rc::new(|_| Ok(None)),
        noop_wake(),
    )
    .unwrap();
    // Roots have to be declared: nothing owns them, so `:set` cannot invent one on the way
    // down a path. Every test below writes under `meta`, so seed it once here.
    lua.load(r#"board = doc:open("board") board:set({"meta"}, doc.map{})"#)
        .exec()
        .unwrap();
    let doc = docs.borrow().get("board").unwrap().core.doc.clone();
    (lua, doc)
}

/// Run Lua against a fresh board and hand back the doc's deep value.
fn wrote(src: &str) -> LoroValue {
    let (lua, doc) = writing();
    lua.load(src).exec().unwrap();
    doc.get_deep_value()
}

fn err_from(src: &str) -> String {
    let (lua, _doc) = writing();
    lua.load(src).exec().unwrap_err().to_string()
}

/// Follow a path of map keys / list positions through a deep value.
fn at<'a>(v: &'a LoroValue, path: &[&str]) -> Option<&'a LoroValue> {
    let mut cur = v;
    for seg in path {
        cur = match (cur, seg.parse::<usize>()) {
            (LoroValue::List(items), Ok(i)) => items.get(i)?,
            (LoroValue::Map(m), _) => m.get(*seg)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn text_at(v: &LoroValue, path: &[&str]) -> Option<String> {
    match at(v, path)? {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

#[test]
fn set_writes_a_scalar_into_a_root_map() {
    let v = wrote(r#"board:set({"meta", "title"}, "Todo")"#);
    assert_eq!(text_at(&v, &["meta", "title"]).as_deref(), Some("Todo"));
}

/// The whole point of `resolve_path` returning the last segment unresolved: `title` does not
/// exist until this call creates it.
#[test]
fn set_creates_a_key_that_did_not_exist() {
    let v = wrote(
        r#"
        board:set({"meta", "a"}, 1)
        board:set({"meta", "b"}, 2)
        "#,
    );
    assert!(at(&v, &["meta", "a"]).is_some() && at(&v, &["meta", "b"]).is_some());
}

/// A tagged table becomes a real container, not a `LoroValue::Map`. The difference is the whole
/// game: a container merges per key, a value is one LWW register over the entire subtree.
#[test]
fn a_tagged_map_becomes_a_container() {
    let (lua, doc) = writing();
    lua.load(r#"board:set({"meta", "card"}, doc.map{ title = "a" })"#)
        .exec()
        .unwrap();

    let ValueOrContainer::Container(Container::Map(_)) = doc.get_map("meta").get("card").unwrap()
    else {
        panic!("doc.map must produce a container, not a value");
    };
}

#[test]
fn a_tagged_list_becomes_a_movable_list() {
    let (lua, doc) = writing();
    lua.load(r#"board:set({"meta", "cards"}, doc.list{})"#)
        .exec()
        .unwrap();

    let ValueOrContainer::Container(Container::MovableList(_)) =
        doc.get_map("meta").get("cards").unwrap()
    else {
        panic!("doc.list must produce a MovableList — a plain List has no per-element `pos`");
    };
}

/// `doc.text` is not cosmetic: it picks character-level merging over whole-value LWW. Nothing
/// about the Lua value can imply that choice, which is why the tag is mandatory.
#[test]
fn a_tagged_string_becomes_a_text_container() {
    let (lua, doc) = writing();
    lua.load(r#"board:set({"meta", "body"}, doc.text("hello"))"#)
        .exec()
        .unwrap();

    let ValueOrContainer::Container(Container::Text(t)) = doc.get_map("meta").get("body").unwrap()
    else {
        panic!("doc.text must produce a Text container");
    };
    assert_eq!(t.to_string(), "hello");
}

/// A plain string stays an LWW register. The pair of this test and the one above is the entire
/// String-vs-Text distinction.
#[test]
fn an_untagged_string_stays_a_value() {
    let v = wrote(r#"board:set({"meta", "title"}, "hello")"#);
    assert!(matches!(
        at(&v, &["meta", "title"]),
        Some(LoroValue::String(_))
    ));
}

#[test]
fn nesting_recurses_all_the_way_down() {
    let v = wrote(
        r#"
        board:set({"cards"}, doc.list{})
        "#,
    );
    // A root created by set is reachable, and empty.
    assert!(matches!(at(&v, &["cards"]), Some(LoroValue::List(_))));

    let v = wrote(
        r#"
        board:set({"meta", "board"}, doc.map{
            title = "Todo",
            cards = doc.list{
                doc.map{ id = "c1", title = "first", tags = doc.list{"red", "urgent"} },
                doc.map{ id = "c2", title = "second" },
            },
        })
        "#,
    );
    assert_eq!(
        text_at(&v, &["meta", "board", "title"]).as_deref(),
        Some("Todo")
    );
    assert_eq!(
        text_at(&v, &["meta", "board", "cards", "1", "title"]).as_deref(),
        Some("second")
    );
    assert_eq!(
        text_at(&v, &["meta", "board", "cards", "0", "tags", "1"]).as_deref(),
        Some("urgent")
    );
}

/// Lua counts from 1 and Loro from 0. `resolve_path` converts; this proves the conversion
/// survives an actual write, because getting it wrong edits the neighbour and says nothing.
#[test]
fn a_list_position_addresses_the_element_lua_named() {
    let v = wrote(
        r#"
        board:set({"meta", "xs"}, doc.list{"a", "b", "c"})
        board:set({"meta", "xs", 2}, "B")
        "#,
    );
    assert_eq!(text_at(&v, &["meta", "xs", "0"]).as_deref(), Some("a"));
    assert_eq!(text_at(&v, &["meta", "xs", "1"]).as_deref(), Some("B"));
    assert_eq!(text_at(&v, &["meta", "xs", "2"]).as_deref(), Some("c"));
}

/// The payoff for `scan_for_id`: a card is addressed by its id, and keeps being addressed
/// correctly after something is inserted ahead of it.
#[test]
fn an_element_is_addressable_by_its_id() {
    let v = wrote(
        r#"
        board:set({"cards"}, doc.list{
            doc.map{ id = "c1", title = "first" },
            doc.map{ id = "c2", title = "second" },
        })
        board:set({"cards", "c2", "title"}, "renamed")
        "#,
    );
    assert_eq!(
        text_at(&v, &["cards", "0", "title"]).as_deref(),
        Some("first")
    );
    assert_eq!(
        text_at(&v, &["cards", "1", "title"]).as_deref(),
        Some("renamed")
    );
}

/// Writing a container over an existing one **replaces** it — a new ContainerID, the old
/// subtree orphaned. Documented here because it is the decision most likely to surprise:
/// a concurrent edit inside the old subtree is discarded, not merged.
#[test]
fn setting_a_container_replaces_the_whole_subtree() {
    let v = wrote(
        r#"
        board:set({"meta", "card"}, doc.map{ title = "a", note = "keep me" })
        board:set({"meta", "card"}, doc.map{ title = "b" })
        "#,
    );
    assert_eq!(
        text_at(&v, &["meta", "card", "title"]).as_deref(),
        Some("b")
    );
    assert!(
        at(&v, &["meta", "card", "note"]).is_none(),
        "replace, not merge — `note` must be gone"
    );
}

// ── what the write path refuses ───────────────────────────────────────────────

/// The rule that makes explicit tagging work at all. An untagged table is an authoring mistake,
/// and guessing at it is exactly what tagging exists to avoid.
#[test]
fn an_untagged_table_is_rejected() {
    let err = err_from(r#"board:set({"meta", "card"}, { title = "a" })"#);
    assert!(err.contains("doc.map"), "{err}");
}

/// Nested tables need tagging too — the rule is uniform, which is the point of it.
#[test]
fn an_untagged_nested_table_is_rejected() {
    let err = err_from(r#"board:set({"meta", "c"}, doc.map{ inner = { x = 1 } })"#);
    assert!(err.contains("doc.map"), "{err}");
}

/// A lookalike cannot forge the tag: the marker is metatable *identity*, not content.
#[test]
fn a_forged_tag_is_rejected() {
    let err = err_from(
        r#"
        local fake = setmetatable({ title = "a" }, {})
        board:set({"meta", "card"}, fake)
        "#,
    );
    assert!(err.contains("doc.map"), "{err}");
}

/// `set(path, x)` where `x` turned out nil is the commonest Lua accident there is. Storing a
/// Null would also punch a hole in the mirror's array part that `raw_len` then reads wrong.
#[test]
fn writing_nil_is_rejected() {
    let err = err_from(r#"board:set({"meta", "title"}, nil)"#);
    assert!(err.contains("nil"), "{err}");
}

#[test]
fn writing_a_function_is_rejected() {
    let err = err_from(r#"board:set({"meta", "f"}, function() end)"#);
    assert!(err.contains("cannot write a function"), "{err}");
}

/// Only the array part of a `doc.list` is written, so a named field would vanish silently.
#[test]
fn a_named_field_in_a_list_is_rejected() {
    let err = err_from(r#"board:set({"meta", "xs"}, doc.list{ 1, 2, why = "here" })"#);
    assert!(err.contains("positional entries only"), "{err}");
}

#[test]
fn a_positional_entry_in_a_map_is_rejected() {
    let err = err_from(r#"board:set({"meta", "m"}, doc.map{ "positional" })"#);
    assert!(err.contains("keys must be strings"), "{err}");
}

#[test]
fn an_empty_path_is_rejected() {
    let err = err_from(r#"board:set({}, "x")"#);
    assert!(err.contains("list of segments"), "{err}");
}

/// A failed write is **not** atomic, and there is no public API to make it so — `LoroDoc` has
/// `commit`, but no abort (loro/src/lib.rs:593). Ops already applied sit in the pending
/// transaction and are sealed by the *next* successful commit.
///
/// So this records the real behaviour rather than a rollback that does not exist: the container
/// is created, whatever landed before the bad value stays, and `:set` reports the error.
#[test]
fn a_failed_write_is_not_rolled_back() {
    let (lua, doc) = writing();
    let err = lua
        .load(r#"board:set({"meta", "c"}, doc.map{ good = 1, bad = function() end })"#)
        .exec();
    assert!(err.is_err(), "a function is not writable");

    // `place` runs before `fill`, so the container exists regardless of what fill did.
    assert!(
        doc.get_map("meta").get("c").is_some(),
        "the container is created before its contents are written"
    );
    // And the partial op is not discarded — the next commit seals it.
    lua.load(r#"board:set({"meta", "ok"}, 1)"#).exec().unwrap();
    assert!(doc.get_map("meta").get("c").is_some());
}

// ── write reaches the mirror ──────────────────────────────────────────────────

/// End to end: Lua writes, the subscriber ticks, `view()` repatches, and Lua reads its own
/// write back through the mirror on the next frame. This is the loop the whole file exists for.
#[test]
fn a_write_comes_back_through_the_mirror() {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(
        0,
        r#"
        local b = doc:open("board")
        b:set({"cards"}, doc.list{ doc.map{ id = "c1", title = "first" } })
        return function() return ui.text{ b.cards[1].title } end
        "#,
    )
    .unwrap();
    src.commit();

    let app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    assert_eq!(app.error, None);
    // view() is what repatches the mirror, so the card is only visible after a frame.
    let _ = app.view();
    assert_eq!(cards_len(&app), 1);
}

/// And it survives the round trip to storage, which is the first time a card — rather than a
/// scalar poked into a root map — has ever made that trip.
#[test]
fn a_card_written_from_lua_survives_a_restart() {
    let writer =
        board_app_writing(r#"b:set({"cards"}, doc.list{ doc.map{ id = "c1", title = "first" } })"#);
    let _ = writer.view();
    let mut writer = writer;
    let puts = Puts::default();
    writer.flush(puts.recorder()).unwrap();

    let saved = puts.take();
    let (name, bytes) = saved.first().expect("the board should have been saved");
    assert_eq!(name, "board");

    let reopened = board_app(serving("board", bytes.clone()));
    assert_eq!(cards_len(&reopened), 1);
}

/// `board_app`, plus a line of Lua run at module scope before the view closure is returned.
fn board_app_writing(write: &str) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(
        0,
        &format!(
            r#"
            local b = doc:open("board")
            {write}
            return function() return ui.text{{ b.cards[1].title }} end
            "#
        ),
    )
    .unwrap();
    src.commit();
    LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap()
}

// ── :insert / :delete / :move ─────────────────────────────────────────────────

/// A board with three cards, as a root list — the kanban shape.
fn carded() -> (Lua, LoroDoc) {
    let (lua, doc) = writing();
    lua.load(
        r#"
        board:set({"cards"}, doc.list{
            doc.map{ id = "k1", col = "todo", text = "one" },
            doc.map{ id = "k2", col = "todo", text = "two" },
            doc.map{ id = "k3", col = "done", text = "three" },
        })
        "#,
    )
    .exec()
    .unwrap();
    (lua, doc)
}

/// The ids in `cards`, in order. Every assertion below is about order or membership, and an id
/// list says both at once.
fn ids(doc: &LoroDoc) -> Vec<String> {
    let v = doc.get_deep_value();
    let Some(LoroValue::List(items)) = at(&v, &["cards"]).cloned() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|c| match c {
            LoroValue::Map(m) => match m.get("id") {
                Some(LoroValue::String(s)) => Some(s.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn run(lua: &Lua, src: &str) {
    lua.load(src).exec().unwrap();
}

/// Appending reads the way Lua already writes it — `#list + 1` — because `resolve_path` does not
/// bounds-check a position. That is the whole reason `:insert` needs no separate `push`.
#[test]
fn insert_appends_at_one_past_the_end() {
    let (lua, doc) = carded();
    run(&lua, r#"board:insert({"cards"}, doc.map{ id = "k4" })"#);
    assert_eq!(ids(&doc), ["k1", "k2", "k3", "k4"]);
}

#[test]
fn insert_shifts_the_elements_after_it() {
    let (lua, doc) = carded();
    run(&lua, r#"board:insert({"cards"}, 1, doc.map{ id = "k0" })"#);
    assert_eq!(ids(&doc), ["k0", "k1", "k2", "k3"]);
}

/// `:insert` grows the list, `:set` replaces an element. Same path shape, different verb — and
/// picking the wrong one silently loses a card, so the pair is tested together.
#[test]
fn insert_grows_where_set_replaces() {
    let (lua, doc) = carded();
    run(
        &lua,
        r#"board:set({"cards", 1}, doc.map{ id = "replaced" })"#,
    );
    assert_eq!(ids(&doc), ["replaced", "k2", "k3"]);
}

#[test]
fn insert_past_the_end_is_rejected() {
    let (lua, _doc) = carded();
    let err = lua
        .load(r#"board:insert({"cards"}, 99, doc.map{ id = "x" })"#)
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("the list holds 3"), "{err}");
}

#[test]
fn insert_into_a_map_points_at_set() {
    let (lua, _doc) = carded();
    let err = lua
        .load(r#"board:insert({"meta"}, doc.map{})"#)
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains(":set"), "{err}");
}

#[test]
fn delete_removes_a_card_by_id() {
    let (lua, doc) = carded();
    run(&lua, r#"board:delete({"cards", "k2"})"#);
    assert_eq!(ids(&doc), ["k1", "k3"]);
}

#[test]
fn delete_removes_a_card_by_position() {
    let (lua, doc) = carded();
    run(&lua, r#"board:delete({"cards", 1})"#);
    assert_eq!(ids(&doc), ["k2", "k3"]);
}

#[test]
fn delete_removes_a_map_key() {
    let (lua, doc) = carded();
    run(
        &lua,
        r#"
        board:set({"meta", "title"}, "Todo")
        board:delete({"meta", "title"})
        "#,
    );
    assert!(at(&doc.get_deep_value(), &["meta", "title"]).is_none());
}

/// Deleting an id that is not there is an error, not a silent no-op: the caller thought it was
/// removing something, and quietly doing nothing hides a stale id.
#[test]
fn deleting_an_absent_id_is_an_error() {
    let (lua, _doc) = carded();
    let err = lua
        .load(r#"board:delete({"cards", "nope"})"#)
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("no element with id"), "{err}");
}

#[test]
fn move_reorders_by_id() {
    let (lua, doc) = carded();
    run(&lua, r#"board:move({"cards", "k3"}, 1)"#);
    assert_eq!(ids(&doc), ["k3", "k1", "k2"]);
}

#[test]
fn move_reorders_by_position() {
    let (lua, doc) = carded();
    run(&lua, r#"board:move({"cards", 1}, 3)"#);
    assert_eq!(ids(&doc), ["k2", "k3", "k1"]);
}

#[test]
fn move_out_of_range_is_rejected() {
    let (lua, _doc) = carded();
    for src in [
        r#"board:move({"cards", "k1"}, 0)"#,
        r#"board:move({"cards", "k1"}, 9)"#,
    ] {
        assert!(lua.load(src).exec().is_err(), "{src} should be rejected");
    }
}

/// The cross-column drag, which is the whole reason the board is a MovableList rather than a
/// plain List: `:move` changes the element's `pos` register and `:set` changes a field inside
/// it, and the two are independent registers. Delete-then-insert would mint a new element and
/// duplicate the card under a concurrent edit.
#[test]
fn a_card_moves_across_columns_without_being_recreated() {
    let (lua, doc) = carded();
    run(
        &lua,
        r#"
        board:move({"cards", "k3"}, 1)
        board:set({"cards", "k3", "col"}, "todo")
        "#,
    );
    assert_eq!(ids(&doc), ["k3", "k1", "k2"]);
    let v = doc.get_deep_value();
    assert_eq!(text_at(&v, &["cards", "0", "col"]).as_deref(), Some("todo"));
    // The field it did not touch survived — the card was moved, not rebuilt.
    assert_eq!(
        text_at(&v, &["cards", "0", "text"]).as_deref(),
        Some("three")
    );
}

/// Add, move across a column, delete — then restart. The w3.md "done when" for §3, minus the
/// window.
#[test]
fn a_board_survives_a_restart_after_every_operation() {
    let app = board_app_writing(
        r#"
        if not b.cards then
            b:set({"cards"}, doc.list{
                doc.map{ id = "k1", col = "todo", text = "one" },
                doc.map{ id = "k2", col = "todo", text = "two" },
            })
            b:insert({"cards"}, doc.map{ id = "k3", col = "todo", text = "three" })
            b:move({"cards", "k3"}, 1)
            b:set({"cards", "k3", "col"}, "done")
            b:delete({"cards", "k2"})
        end
        "#,
    );
    let _ = app.view();
    let mut app = app;
    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    let saved = puts.take();
    let (_, bytes) = saved.first().expect("the board should have been saved");

    let doc = LoroDoc::new();
    doc.import(bytes).unwrap();
    assert_eq!(ids(&doc), ["k3", "k1"]);
    let v = doc.get_deep_value();
    assert_eq!(text_at(&v, &["cards", "0", "col"]).as_deref(), Some("done"));
}

// ── the ported kanban ─────────────────────────────────────────────────────────

/// The real app source. The path reaches into shell2 deliberately: a fixture copy would drift
/// away from the app that ships, and the whole value of these tests is that the write API works
/// for its actual first caller. It is an `include_str!`, not a crate dependency.
/// The real app, all four files of it — `include_str!` rather than a fixture so these tests
/// fail when the app changes. It is also the only end-to-end check that `require` resolves a
/// subdirectory (`ui/widgets`) the way an upload stores one.
const KANBAN: [(&str, &str); 4] = [
    ("main.lua", include_str!("../../shell2/src/kanban/main.lua")),
    (
        "model.lua",
        include_str!("../../shell2/src/kanban/model.lua"),
    ),
    (
        "theme.lua",
        include_str!("../../shell2/src/kanban/theme.lua"),
    ),
    (
        "ui/widgets.lua",
        include_str!("../../shell2/src/kanban/ui/widgets.lua"),
    ),
];

fn kanban_app(resolve: Resolve) -> LuaApp<LuaMsg> {
    app_from(KANBAN.iter().map(|(p, b)| (*p, *b)), resolve)
}

/// `kanban_app` with the bodies handed in, so `round_trip` can run the same app from source that
/// went through the printer.
fn app_from<'a>(
    files: impl Iterator<Item = (&'a str, &'a str)>,
    resolve: Resolve,
) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (path, body) in files {
        let t = map.insert_container(path, LoroText::new()).unwrap();
        t.insert(0, body).unwrap();
    }
    src.commit();
    LuaApp::open(src, resolve, noop_wake(), Rc::new(|m| m)).unwrap()
}

/// Ids in a root list of maps, in order.
fn ids_in(snapshot: &[u8], root: &str) -> Vec<String> {
    let doc = LoroDoc::new();
    doc.import(snapshot).unwrap();
    let LoroValue::Map(top) = doc.get_deep_value() else {
        return Vec::new();
    };
    let Some(LoroValue::List(items)) = top.get(root).cloned() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|c| match c {
            LoroValue::Map(m) => match m.get("id") {
                Some(LoroValue::String(s)) => Some(s.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The port's real risk is that it does not run at all: module scope now calls `doc:open` and
/// four write methods before the view closure is ever returned, and a mistake there is a
/// startup failure, not a wrong pixel.
#[test]
fn the_kanban_loads_and_seeds_its_board() {
    let mut app = kanban_app(Rc::new(|_| Ok(None)));
    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();

    let saved = puts.take();
    let (name, bytes) = saved
        .first()
        .expect("seeding should have made the doc dirty");
    assert_eq!(name, "board");
    assert_eq!(ids_in(bytes, "columns"), ["c-todo", "c-doing", "c-done"]);
    assert_eq!(ids_in(bytes, "cards"), ["k1", "k2", "k3", "k4"]);
}

/// Module scope runs on every open. Without the `if not board.columns` guard, `:set` on a root
/// clears and refills it — so reopening would silently discard the real board and write the
/// seed over it on the next flush. This is the test that guard exists for.
#[test]
fn reopening_the_kanban_does_not_reseed_it() {
    let mut first = kanban_app(Rc::new(|_| Ok(None)));
    let puts = Puts::default();
    first.flush(puts.recorder()).unwrap();
    let seeded = puts.take().pop().expect("seeded").1;

    // Stand in for a user having edited the board between sessions.
    let doc = LoroDoc::new();
    doc.import(&seeded).unwrap();
    doc.get_movable_list("cards").delete(0, 3).unwrap();
    doc.commit();
    let edited = doc.export(loro::ExportMode::Snapshot).unwrap();
    assert_eq!(ids_in(&edited, "cards"), ["k4"]);

    let mut second = kanban_app(serving("board", edited));
    let puts = Puts::default();
    second.flush(puts.recorder()).unwrap();

    // Nothing was written, because nothing changed — the seed did not fire.
    assert!(
        puts.take().is_empty(),
        "reopening re-seeded the board, wiping the user's edits"
    );
}

/// And the view still builds against the mirror. `view()` repatches before calling the closure,
/// so the cards seeded at module scope are visible on the very first frame despite the mirror
/// being a frame behind every *other* write.
///
/// The view closure is called *directly* rather than trusting `LuaApp::view`, which swallows a
/// Lua error into a `text("View error: …")` element. An earlier version of this test did
/// `let _ = app.view();` and passed against a view that referenced a variable the port had
/// deleted — green, and broken on screen. A discarded view result asserts nothing.
#[test]
fn the_kanban_renders_its_seeded_board() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None, "module scope failed");

    // Repatches the mirror, which the closure below reads.
    let _ = app.view();
    let docs = app.docs.borrow();
    let mirror = &docs.get("board").unwrap().mirror;
    assert_eq!(mirror.get::<Table>("cards").unwrap().raw_len(), 4);
    assert_eq!(mirror.get::<Table>("columns").unwrap().raw_len(), 3);
    drop(docs);

    let view_fn = app.view_fn.as_ref().expect("no view closure");
    if let Err(e) = view_fn.call::<Table>(()) {
        panic!("the view failed to build: {e}");
    }
}

/// What the split actually introduced. `drag`, `placement` and `col_modal` used to be file-scoped
/// locals; now handlers in `model.lua` write them and the view in `main.lua` reads them, which
/// only works because both hold the *same* table. Get that wrong and nothing fails to load — the
/// board renders perfectly and simply never responds to a pointer.
#[test]
fn the_kanban_shares_pointer_state_across_modules() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None);
    let _ = app.view();

    app.vm
        .load("require('model').update({ kind = 'open_col' })")
        .exec()
        .unwrap();
    assert!(
        app.vm
            .load("return require('model').state.col_modal")
            .eval::<bool>()
            .unwrap(),
        "the handler's write did not land in the shared table"
    );

    // The read half, and the only assertion that can catch the failure that matters. Building
    // without error proves nothing: `main.lua` holding its *own* `{ col_modal = false }` would
    // still render a perfectly good board — just one that ignores every click forever. So look
    // for the thing the modal draws.
    let view_fn = app.view_fn.as_ref().expect("no view closure");
    let tree = view_fn
        .call::<Table>(())
        .expect("the view failed to build with the modal open");
    assert!(
        has_text(&tree, "New column"),
        "the view did not see the handler's write"
    );
}

/// Depth-first search for a string anywhere in a `ui.*` tree — the cheapest way to ask "did this
/// actually render?" without teaching the tests the shape of every node.
fn has_text(node: &Table, needle: &str) -> bool {
    for pair in node.pairs::<Value, Value>() {
        let Ok((_, v)) = pair else { continue };
        let found = match v {
            Value::String(s) => s.to_str().is_ok_and(|s| &*s == needle),
            Value::Table(t) => has_text(&t, needle),
            _ => false,
        };
        if found {
            return true;
        }
    }
    false
}

/// The whole loop across all four files: a handler in `model.lua` reads pointer state, writes
/// the doc, and the mirror in `main.lua` shows the result on the next frame. Dropping a card on
/// another column's background restamps `col` and leaves the ordering alone.
#[test]
fn the_kanban_moves_a_card_across_modules() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None);
    let _ = app.view();
    assert_eq!(card_col(&app, "k1"), "c-todo", "seeded position");

    app.vm
        .load(
            r#"
            local m = require('model')
            m.update({ kind = 'drag', what = 'card', id = 'k1', phase = 'start', x = 0, y = 0 })
            m.update({ kind = 'drop_col', col = 'c-done', phase = 'over', x = 0.5 })
            m.update({ kind = 'drop_col', col = 'c-done', phase = 'end', x = 0.5 })
            "#,
        )
        .exec()
        .unwrap();

    // The write reached Loro immediately; the mirror only catches up in view().
    assert_eq!(card_col(&app, "k1"), "c-todo", "stale until the next frame");
    let _ = app.view();
    assert_eq!(card_col(&app, "k1"), "c-done");
}

/// A card's column, read out of the mirror by id.
fn card_col(app: &LuaApp<LuaMsg>, id: &str) -> String {
    let docs = app.docs.borrow();
    let cards: Table = docs.get("board").unwrap().mirror.get("cards").unwrap();
    for i in 1..=cards.raw_len() {
        let card: Table = cards.get(i).unwrap();
        if card.get::<String>("id").unwrap() == id {
            return card.get::<String>("col").unwrap();
        }
    }
    panic!("no card {id}");
}

/// A column's stored width, read out of the mirror by id.
fn col_w(app: &LuaApp<LuaMsg>, id: &str) -> f64 {
    let docs = app.docs.borrow();
    let cols: Table = docs.get("board").unwrap().mirror.get("columns").unwrap();
    for i in 1..=cols.raw_len() {
        let c: Table = cols.get(i).unwrap();
        if c.get::<String>("id").unwrap() == id {
            return c.get::<f64>("w").unwrap();
        }
    }
    panic!("no column {id}");
}

/// Every `id` in a `ui.*` tree that starts with `prefix`, depth-first.
fn ids_with(node: &Table, prefix: &str, out: &mut Vec<String>) {
    if let Ok(Some(id)) = node.get::<Option<String>>("id")
        && id.starts_with(prefix)
    {
        out.push(id);
    }
    for pair in node.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            ids_with(&t, prefix, out);
        }
    }
}

/// The four tests below drive `model.lua` directly, which means every one of them would still pass
/// if the grip were deleted from `main.lua` — a board that resizes perfectly and offers nothing to
/// grab. So look for the control itself, one per column and keyed to it.
#[test]
fn the_kanban_draws_a_grip_for_every_column() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None);
    let _ = app.view();

    let tree = app
        .view_fn
        .as_ref()
        .expect("no view closure")
        .call::<Table>(())
        .expect("the view failed to build");

    let mut grips = Vec::new();
    ids_with(&tree, "grip:", &mut grips);
    grips.sort();
    assert_eq!(
        grips,
        ["grip:c-doing", "grip:c-done", "grip:c-todo"],
        "a grip is missing, or is not keyed to its column"
    );
}

/// Drive a resize gesture: grab at `from`, release at `to`, both in the same coordinates the
/// runtime hands a drag handler (`pos - grab`, so their difference is the pointer's travel).
fn resize(app: &LuaApp<LuaMsg>, id: &str, from: f64, to: f64) {
    app.vm
        .load(format!(
            r#"
            local m = require('model')
            m.update({{ kind = 'resize', id = '{id}', phase = 'start', x = {from} }})
            m.update({{ kind = 'resize', id = '{id}', phase = 'move', x = {to} }})
            m.update({{ kind = 'resize', id = '{id}', phase = 'end', x = {to} }})
            "#
        ))
        .exec()
        .unwrap();
}

/// Column width is board data, not view state and not source, so a resize takes the same route a
/// card move does: pointer state while the gesture runs, one write at the end, mirror next frame.
/// Nothing here is checkable by looking — the app cannot be launched from a test — so the
/// assertion is on what the doc holds.
#[test]
fn the_kanban_resizes_a_column() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None);
    let _ = app.view();
    assert_eq!(col_w(&app, "c-todo"), 300.0, "seeded width");

    resize(&app, "c-todo", 100.0, 180.0);
    assert_eq!(col_w(&app, "c-todo"), 300.0, "stale until the next frame");
    let _ = app.view();
    assert_eq!(col_w(&app, "c-todo"), 380.0, "300 plus the 80pt travelled");

    // Only the one column moved. A splitter that quietly rewrites its neighbour is the failure
    // the fixed-width layout is supposed to make impossible, so say so.
    assert_eq!(col_w(&app, "c-doing"), 300.0);
    assert_eq!(col_w(&app, "c-done"), 300.0);
}

/// The doc is untouched *during* the gesture. Sixty frames of dragging is one op, not sixty —
/// otherwise every resize floods the history and every peer replays the whole sweep.
#[test]
fn a_resize_in_flight_writes_nothing_to_the_doc() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    let _ = app.view();

    app.vm
        .load(
            r#"
            local m = require('model')
            m.update({ kind = 'resize', id = 'c-todo', phase = 'start', x = 0 })
            for i = 1, 60 do
                m.update({ kind = 'resize', id = 'c-todo', phase = 'move', x = i })
            end
            "#,
        )
        .exec()
        .unwrap();

    let _ = app.view();
    assert_eq!(col_w(&app, "c-todo"), 300.0, "a live drag reached the doc");
    assert_eq!(
        app.vm
            .load("return require('model').state.resize.w")
            .eval::<f64>()
            .unwrap(),
        360.0,
        "the width the frame should be drawing is not in pointer state"
    );
}

/// The floor is not decoration. A column dragged to nothing has no grip left to drag it back by,
/// and there is no undo for it — so the clamp has to run on every move, not once at release.
#[test]
fn a_resize_cannot_drag_a_column_away() {
    let app = kanban_app(Rc::new(|_| Ok(None)));
    let _ = app.view();

    resize(&app, "c-todo", 0.0, -5000.0);
    let _ = app.view();
    assert_eq!(col_w(&app, "c-todo"), 180.0, "theme's col_w_min");

    resize(&app, "c-todo", 0.0, 5000.0);
    let _ = app.view();
    assert_eq!(col_w(&app, "c-todo"), 620.0, "theme's col_w_max");
}

/// A brush against the grip is not an edit: a press with no drag must leave the doc alone, so an
/// accidental touch does not put an op in every peer's history.
///
/// Both halves, because the negative one alone is worthless — a `flush` that never produces
/// anything would pass it while the whole feature was dead.
#[test]
fn a_resize_that_never_moved_writes_nothing() {
    let mut app = kanban_app(Rc::new(|_| Ok(None)));
    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    puts.take(); // drop the seed
    let _ = app.view();

    app.vm
        .load(
            r#"
            local m = require('model')
            m.update({ kind = 'resize', id = 'c-todo', phase = 'start', x = 42 })
            m.update({ kind = 'resize', id = 'c-todo', phase = 'end', x = 42 })
            "#,
        )
        .exec()
        .unwrap();
    app.flush(puts.recorder()).unwrap();
    assert!(puts.take().is_empty(), "a press with no drag wrote to Loro");

    // The same path, moved. This is what makes the assertion above mean something.
    resize(&app, "c-todo", 0.0, 40.0);
    app.flush(puts.recorder()).unwrap();
    assert!(!puts.take().is_empty(), "a real resize wrote nothing");
}

// ── the tally demo app, from disk ─────────────────────────────────────────────

/// The app a fresh demo seeds (`demo_apps/tally/`), loaded the way the shell loads every app:
/// from the files map of a source doc. Reads the real file from disk so the test cannot drift
/// from what ships.
fn tally_app() -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main_text =
        std::fs::read_to_string("../demo_apps/tally/main.lua").expect("tally/main.lua on disk");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(0, &main_text).unwrap();
    src.commit();
    LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap()
}

#[test]
fn tally_loads_views_and_clicks() {
    let mut app = tally_app();
    assert_eq!(app.error, None, "the app must load: {:?}", app.error);

    let _ = app.view();

    // Handlers are registered in document order: pill("−") is 0, pill("+") is 1, reset is 2.
    // A frame between each click, the way the real loop delivers them: the mirror is a frame
    // behind the doc, so clicks with no frame between them all read the same stale count.
    for _ in 0..3 {
        app.update(LuaMsg::Call(1));
        let _ = app.view();
    }

    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    let saved = puts.take();
    let (name, bytes) = saved.first().expect("the tally doc should have been saved");
    assert_eq!(name, "tally");

    let back = LoroDoc::new();
    back.import(bytes).unwrap();
    let v = back.get_deep_value();
    let Some(LoroValue::Map(m)) = at(&v, &["count"]).cloned() else {
        panic!("no count map in the saved doc: {v:?}");
    };
    match m.get("n") {
        Some(LoroValue::Double(n)) => assert_eq!(*n, 3.0, "three clicks of +"),
        other => panic!("count.n is not a number: {other:?}"),
    }
}

#[test]
fn console_captures_view_and_handler_errors() {
    let src = LoroDoc::new();
    write_source_file(
        &src,
        "main.lua",
        r#"
return function()
	return ui.col{
		ui.text({ id = "t1", bogus_prop = true, "hi" }),
		ui.button({ id = "boom", on_click = function() error("kaboom") end, ui.text{ "go" } }),
	}
end
"#,
    )
    .unwrap();
    let mut app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    let _ = app.view();
    let once = app.console(100);
    assert!(
        once.iter().any(|l| l.contains("bogus_prop")),
        "unknown prop must land in the console: {once:?}"
    );
    let _ = app.view();
    assert_eq!(
        app.console(100),
        once,
        "a persistent per-frame error must not repeat per frame"
    );

    let mut tree = app.view();
    let msg = tree.trigger("boom", runtime::Action::Click).unwrap();
    app.update(msg);
    let c = app.console(100);
    assert!(
        c.last()
            .is_some_and(|l| l.contains("handler error") && l.contains("kaboom")),
        "handler error must land in the console: {c:?}"
    );
}

// ── error cards ─────────────────────────────────────────────────────────────

/// The card text is authored, not leaked: the boundary used to render mlua's plumbing
/// (`runtime error: …`) into every card, and the breadcrumb separators alternated
/// (`col:[3]> [1]`). Both are pinned here in one console line.
#[test]
fn error_cards_show_the_reason_not_the_plumbing() {
    let src = LoroDoc::new();
    write_source_file(
        &src,
        "main.lua",
        r#"
return function()
	return ui.col{
		ui.text({ id = "t1", bogus_prop = true, "hi" }),
		ui.text{ "sibling stays alive" },
	}
end
"#,
    )
    .unwrap();
    let app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    let _ = app.view();
    let lines = app.console(100);
    let line = lines
        .iter()
        .find(|l| l.contains("bogus_prop"))
        .expect("the error must land in the console");
    assert_eq!(
        line.as_str(),
        "col:[3] > [1] > text#t1:[4] > unknown prop bogus_prop"
    );
}

/// With breadcrumbs off (`dev = false`, the production setting) the path is empty and the
/// message used to arrive with a leading ` > ` — an arrow pointing at nothing.
#[test]
fn a_breadcrumb_less_card_has_no_leading_separator() {
    let (lua, _fires) = sandboxed_vm().unwrap();
    let node: Table = lua
        .load(r#"return ui.col{ ui.text{} }"#)
        .eval()
        .unwrap();
    let mut handlers = Vec::new();
    let mut ctx = Ctx::new(&mut handlers, identity());
    ctx.dev = false;
    assert!(walk(node, &mut ctx).is_ok(), "siblings stay alive");
    assert_eq!(
        ctx.errors,
        vec!["needs its label as child 1, got  nil - has ".to_string()]
    );
}

#[test]
fn docs_json_reads_live_core_state() {
    let src = LoroDoc::new();
    write_source_file(
        &src,
        "main.lua",
        r#"
local s = doc:open("state")
if not s.map then s:set({ "map" }, doc.map({ count = 0 })) end
return function()
	local m = s.map
	return ui.col{
		ui.button({ id = "inc", on_click = function() s:set({ "map", "count" }, m.count + 1) end, ui.text{ "+" } }),
	}
end
"#,
    )
    .unwrap();
    let mut app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    let _ = app.view();
    // The core is read directly, not through the one-frame-behind mirror. Lua numbers are
    // doubles all the way down — the JSON carries 0.0, not 0.
    assert_eq!(app.docs_json()["state"]["map"]["count"].as_f64(), Some(0.0));
    let mut tree = app.view();
    app.update(tree.trigger("inc", runtime::Action::Click).unwrap());
    assert_eq!(app.docs_json()["state"]["map"]["count"].as_f64(), Some(1.0));
}
