//! Tests for the Lua layer: the table → `Node` walk, style parsing, error capture, and the `doc`
//! CRDT binding. Most need no egui context.

use std::rc::Rc;

use egui::Color32;
use loro::LoroDoc;

use super::Script;

/// Load a script with a fresh, empty CRDT (most tests don't touch `doc`).
fn load(source: &str) -> Script {
    Script::load(source, Rc::new(LoroDoc::new()))
}

#[test]
fn walks_nested_tree_and_styles() {
    let mut s = load(
        r##"return function()
              return ui.col{ style = { gap = 8, background = "#1e2228" },
                ui.text{ "hi" },
                ui.row{ ui.text{ "a" }, ui.text{ "b" } },
              }
            end"##,
    );
    let root = s.view().expect("view ok");
    assert!((root.style.gap - 8.0).abs() < 0.01);
    assert_eq!(root.style.background, Some(Color32::from_rgb(0x1e, 0x22, 0x28)));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].plain_text().as_deref(), Some("hi"));
    assert_eq!(root.children[1].children.len(), 2, "the row has two text leaves");
}

#[test]
fn parses_text_into_styled_runs() {
    // A text node's array items become runs: strings plain, tables styled.
    let mut s = load(
        r##"return function()
              return ui.text{ "plain ", { "bold", bold = true }, { "x", code = true, color = "#ff0000" } }
            end"##,
    );
    let node = s.view().expect("view ok");
    let runs = node.text.expect("a text leaf");
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].text, "plain ");
    assert!(runs[0].marks.is_empty(), "a bare string is an unmarked run");
    assert!(runs[1].marks.has("bold"));
    assert!(runs[2].marks.has("code"));
    assert_eq!(runs[2].color, Some(Color32::from_rgb(0xff, 0x00, 0x00)), "explicit run colour parsed");
}

#[test]
fn setup_error_is_captured_not_panicked() {
    // Not a function — setup should fail cleanly rather than panic.
    let mut s = load("return 42");
    assert!(s.view().is_err());
}

#[test]
fn runtime_error_keeps_chunk_name_and_line() {
    // Fails when the view *runs* (undefined global) — the per-frame error path, not load-time.
    let mut s = load("return function() return missing_fn() end");
    let err = s.view().expect_err("calling a nil value must error");
    assert!(err.contains("app:1"), "error keeps the chunk name + line, got: {err}");
}

#[test]
fn on_click_dispatch_mutates_lua_state() {
    // Dispatching the captured on_click mutates an upvalue the next view() reflects — the event
    // loop minus geometry/hit-test.
    let mut s = load(
        r#"local n = 0
           return function()
             return ui.col{
               ui.text{ "n=" .. n },
               ui.button{ "+", on_click = function() n = n + 1 end },
             }
           end"#,
    );

    let tree = s.view().expect("view ok");
    assert_eq!(tree.children[0].plain_text().as_deref(), Some("n=0"));
    let id = tree.children[1].on_click.expect("button registered a handler");

    s.dispatch(id).expect("handler runs");
    let tree = s.view().expect("view ok after dispatch");
    assert_eq!(tree.children[0].plain_text().as_deref(), Some("n=1"), "click mutated state");
}

#[test]
fn list_and_map_crdt_ops() {
    // add / field-set / remove / field-get / #len / :get — the whole Map + MovableList surface.
    let mut s = load(
        r#"local todos = doc:list("todos")
           todos:add{ text = "a", done = false }
           todos:add{ text = "b", done = true }
           todos:get(1).done = true   -- toggle the first
           todos:remove(2)            -- drop the second
           return function()
             local t = todos:get(1)
             return ui.text{ #todos .. ":" .. t.text .. ":" .. tostring(t.done) }
           end"#,
    );
    assert_eq!(s.view().unwrap().plain_text().as_deref(), Some("1:a:true"));
}

#[test]
fn dispatched_handler_writes_to_doc() {
    // A click handler (not setup-time code) that calls `todos:add` reaches the same doc.
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(
        r#"local todos = doc:list("todos")
           return function()
             return ui.button{ "add", on_click = function() todos:add{ text = "x", done = false } end }
           end"#,
        doc.clone(),
    );
    let tree = s.view().unwrap();
    let id = tree.on_click.expect("button registered a handler");
    s.dispatch(id).expect("handler runs");
    assert_eq!(doc.get_movable_list("todos").len(), 1, "the dispatched add wrote to loro");
}

#[test]
fn bundled_example_apps_load_and_render() {
    // The shipped apps must parse, run setup, and produce a non-empty view — a regression guard.
    for (name, src) in [
        ("demo.lua", include_str!("../../examples/demo.lua")),
        ("todo.lua", include_str!("../../examples/todo.lua")),
        ("kanban.lua", include_str!("../../examples/kanban.lua")),
        ("notes.lua", include_str!("../../examples/notes.lua")),
    ] {
        let mut s = Script::load(src, Rc::new(LoroDoc::new()));
        let root = s.view().unwrap_or_else(|e| panic!("{name} failed to render: {e}"));
        assert!(!root.children.is_empty(), "{name} rendered an empty tree");
    }
}

#[test]
fn editor_reads_loro_text_and_binds_its_id() {
    // An `ui.editor{ id }` is an editable leaf whose content is the backing LoroText (named by
    // the id), read fresh each view.
    let doc = Rc::new(LoroDoc::new());
    doc.get_text("title").insert(0, "hello").unwrap();
    doc.commit();
    let mut s = Script::load(r#"return function() return ui.editor{ id = "title" } end"#, doc.clone());
    let node = s.view().expect("view ok");
    assert_eq!(node.editor.as_deref(), Some("title"), "the leaf is an editor bound to its id");
    assert_eq!(node.plain_text().as_deref(), Some("hello"), "it shows the LoroText content");
}

#[test]
fn with_buffer_edits_the_backing_loro_text() {
    // `with_buffer` hands a TextBuffer over the editor's LoroText; the keystroke reaches the same
    // CRDT and reports the content changed.
    use text_edit::TextField;
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(r#"return function() return ui.editor{ id = "note" } end"#, doc.clone());
    s.view().expect("view ok"); // populates the editor binding this frame
    let changed = s.with_buffer("note", |buf| {
        let mut f = TextField::new();
        f.insert(buf, "abc");
    });
    assert!(changed, "with_buffer reports the content changed");
    assert_eq!(doc.get_text("note").to_string(), "abc", "the edit reached the CRDT");
}

#[test]
fn with_buffer_unknown_id_is_a_noop() {
    // An id that isn't an editor: the closure must not run, and nothing changes.
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(r#"return function() return ui.text{ "x" } end"#, doc.clone());
    s.view().expect("view ok");
    let changed = s.with_buffer("nope", |_| panic!("closure must not run for an unknown editor"));
    assert!(!changed);
}

#[test]
fn editor_value_binds_to_a_doc_text_handle() {
    // `value = doc:text(name)` binds the field to that container even when the `id` differs.
    use text_edit::TextField;
    let doc = Rc::new(LoroDoc::new());
    doc.get_text("body").insert(0, "hi").unwrap();
    doc.commit();
    let mut s = Script::load(
        r#"return function() return ui.editor{ id = "f1", value = doc:text("body") } end"#,
        doc.clone(),
    );
    let node = s.view().expect("view ok");
    assert_eq!(node.editor.as_deref(), Some("f1"), "the field keeps its own id");
    assert_eq!(node.plain_text().as_deref(), Some("hi"), "but reads the bound container");
    let changed = s.with_buffer("f1", |buf| {
        let mut f = TextField::new();
        f.end(buf, false);
        f.insert(buf, "!");
    });
    assert!(changed);
    assert_eq!(doc.get_text("body").to_string(), "hi!", "editing f1 wrote to the bound 'body'");
}

#[test]
fn external_crdt_write_shows_in_view() {
    // MCP-symmetry proof: a writer OUTSIDE the script mutates the same LoroDoc, and the view
    // reflects it next render — no special integration, it's the same CRDT.
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(
        r#"local todos = doc:list("todos")
           return function() return ui.text{ "n=" .. #todos } end"#,
        doc.clone(),
    );
    assert_eq!(s.view().unwrap().plain_text().as_deref(), Some("n=0"));

    // External (peer / MCP) write to the very same doc:
    let map = doc.get_movable_list("todos").push_container(loro::LoroMap::new()).unwrap();
    map.insert("text", "from outside").unwrap();
    doc.commit();

    assert_eq!(s.view().unwrap().plain_text().as_deref(), Some("n=1"), "external CRDT write shows in the view");
}
