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

/// Build a multi-file app with a fresh empty CRDT.
fn load_app(files: &[(&str, &str)]) -> Script {
    let owned: Vec<(String, String)> = files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
    Script::load_app(&owned, Rc::new(LoroDoc::new()))
}

#[test]
fn multi_file_app_requires_modules() {
    // main.lua pulls a value out of lib/state.lua via `require`; a nested path maps to a dotted
    // module name (lib/state.lua -> "lib.state").
    let mut s = load_app(&[
        ("lib/state.lua", r#"return { title = "from module" }"#),
        (
            "main.lua",
            r#"local state = require("lib.state")
               return function()
                 return ui.text{ state.title }
               end"#,
        ),
    ]);
    let node = s.view().expect("view ok");
    assert_eq!(node.plain_text().as_deref(), Some("from module"));
}

#[test]
fn app_without_main_is_an_error() {
    let mut s = load_app(&[("lib/util.lua", "return {}")]);
    assert!(s.view().is_err(), "an app with no main.lua reports a setup error");
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
        ("gallery.lua", include_str!("../../examples/gallery.lua")),
        ("dashboard.lua", include_str!("../../examples/dashboard.lua")),
        ("settings.lua", include_str!("../../examples/settings.lua")),
        ("orders.lua", include_str!("../../examples/orders.lua")),
        ("standup.lua", include_str!("../../examples/standup.lua")),
    ] {
        let mut s = Script::load(src, Rc::new(LoroDoc::new()));
        let root = s.view().unwrap_or_else(|e| panic!("{name} failed to render: {e}"));
        assert!(!root.children.is_empty(), "{name} rendered an empty tree");
    }
    // Multi-file showcase apps load through the require shim.
    for (name, files) in [
        (
            "deck",
            vec![
                ("main.lua".to_string(), include_str!("../../examples/deck/main.lua").to_string()),
                ("slides.lua".to_string(), include_str!("../../examples/deck/slides.lua").to_string()),
            ],
        ),
        (
            "kit",
            vec![
                ("main.lua".to_string(), include_str!("../../examples/kit/main.lua").to_string()),
                ("lib/kit.lua".to_string(), include_str!("../../examples/kit/lib/kit.lua").to_string()),
            ],
        ),
    ] {
        let mut s = Script::load_app(&files, Rc::new(LoroDoc::new()));
        let root = s.view().unwrap_or_else(|e| panic!("{name} failed to render: {e}"));
        assert!(!root.children.is_empty(), "{name} rendered an empty tree");
    }
}

#[test]
fn deck_example_navigates_through_crdt_state() {
    // The deck's slide index is CRDT state: clicking "›" advances it for every viewer.
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load_app(
        &[
            ("main.lua".into(), include_str!("../../examples/deck/main.lua").into()),
            ("slides.lua".into(), include_str!("../../examples/deck/slides.lua").into()),
        ],
        doc.clone(),
    );

    // Find the "›" nav button by its label.
    fn find_next(n: &crate::node::Node) -> Option<u32> {
        if n.on_click.is_some() && n.plain_text().as_deref() == Some("›") {
            return n.on_click;
        }
        n.children.iter().find_map(find_next)
    }
    let root = s.view().expect("deck renders slide 1");
    let next = find_next(&root).expect("a next button exists");
    s.dispatch(next).expect("nav handler runs");

    let i = doc.get_map("nav").get("i").and_then(|v| match v {
        loro::ValueOrContainer::Value(loro::LoroValue::I64(n)) => Some(n),
        _ => None,
    });
    assert_eq!(i, Some(2), "the slide index advanced in the CRDT");
    // And the rendered slide changed: slide 2's title appears in the new tree.
    fn has_text(n: &crate::node::Node, t: &str) -> bool {
        n.plain_text().is_some_and(|s| s.contains(t)) || n.children.iter().any(|c| has_text(c, t))
    }
    let root = s.view().expect("deck renders slide 2");
    assert!(has_text(&root, "Apps are Lua over CRDTs"), "slide 2 is on screen");
}

#[test]
fn board_example_renders_and_toggles_a_task() {
    // The multi-file board exercises the full new-style surface plus require + CRDT interaction.
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load_app(
        &[
            ("main.lua".into(), include_str!("../../examples/board/main.lua").into()),
            ("lib/theme.lua".into(), include_str!("../../examples/board/lib/theme.lua").into()),
        ],
        doc.clone(),
    );
    s.view().expect("empty board renders");

    // Type a task into the draft and submit it.
    doc.get_text("draft").insert(0, "ship the style engine").unwrap();
    doc.commit();
    s.view().expect("re-render with draft");
    assert!(s.submit("draft").expect("submit runs"), "draft declared on_submit");
    assert_eq!(doc.get_movable_list("tasks").len(), 1, "task added");
    assert_eq!(doc.get_text("draft").to_string(), "", "draft cleared");

    // The task row toggles done via its on_click (found by its "○" marker — the Add button is
    // also clickable and comes first in pre-order).
    let root = s.view().expect("board with one task renders");
    fn row_click(n: &crate::node::Node) -> Option<u32> {
        let is_row = n.on_click.is_some()
            && n.children.iter().any(|c| c.plain_text().is_some_and(|t| t.starts_with('○')));
        if is_row {
            n.on_click
        } else {
            n.children.iter().find_map(row_click)
        }
    }
    let row = row_click(&root).expect("a clickable task row exists");
    s.dispatch(row).expect("toggle runs");
    let toggled = doc
        .get_movable_list("tasks")
        .get(0)
        .and_then(|v| match v {
            loro::ValueOrContainer::Container(loro::Container::Map(m)) => m.get("done"),
            _ => None,
        })
        .and_then(|v| match v {
            loro::ValueOrContainer::Value(loro::LoroValue::Bool(b)) => Some(b),
            _ => None,
        });
    assert_eq!(toggled, Some(true), "click marked the task done");
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

// --- CSS-named style keys, shorthands, colors -------------------------------------------------

#[test]
fn parses_css_named_style_keys() {
    let mut s = load(
        r##"return function()
              return ui.col{ style = {
                justify_content = "center", align_items = "center",
                padding = "10 20", margin = 8,
                position = "absolute", top = 5, left = 12,
                border = "2 #ff0000", box_shadow = "0 4 12 #00000080",
                border_radius = "8 8 0 0", opacity = 0.5,
                font_size = 22, flex_grow = 2, max_width = 300,
              } }
            end"##,
    );
    let n = s.view().expect("view ok");
    let st = &n.style;
    assert_eq!(st.justify_content, Some(crate::node::Align::Center));
    assert_eq!(st.align_items, Some(crate::node::Align::Center));
    assert_eq!(st.padding.top, crate::node::Val::Px(10.0));
    assert_eq!(st.padding.left, crate::node::Val::Px(20.0));
    assert_eq!(st.margin.top, crate::node::Val::Px(8.0));
    assert_eq!(st.position, crate::node::Position::Absolute);
    assert_eq!(st.inset.top, crate::node::Val::Px(5.0));
    assert_eq!(st.inset.left, crate::node::Val::Px(12.0));
    let b = st.border.expect("border parsed");
    assert!((b.width - 2.0).abs() < 0.01);
    assert_eq!(b.color, Color32::from_rgb(0xff, 0, 0));
    let sh = st.shadow.expect("shadow parsed");
    assert_eq!(sh.offset, [0.0, 4.0]);
    assert!((sh.blur - 12.0).abs() < 0.01);
    assert_eq!(st.corner_radius.tl, 8.0);
    assert_eq!(st.corner_radius.br, 0.0);
    assert!((st.opacity - 0.5).abs() < 0.01);
    assert!((st.font_size - 22.0).abs() < 0.01);
    assert!((st.flex_grow - 2.0).abs() < 0.01);
    assert_eq!(st.max_width, crate::node::Val::Px(300.0));
}

#[test]
fn old_short_keys_still_work_as_aliases() {
    let mut s = load(
        r##"return function()
              return ui.col{ style = { font = 18, corner = 6, grow = 1, direction = "row", padding = 12 } }
            end"##,
    );
    let st = s.view().expect("view ok").style;
    assert!((st.font_size - 18.0).abs() < 0.01);
    assert_eq!(st.corner_radius.tl, 6.0);
    assert!((st.flex_grow - 1.0).abs() < 0.01);
    assert!(matches!(st.direction, crate::node::Direction::Row));
    assert_eq!(st.padding.left, crate::node::Val::Px(12.0));
}

#[test]
fn parses_full_css_colors_and_units() {
    let mut s = load(
        r##"return function()
              return ui.col{ style = {
                background = "hsl(220, 50%, 20%)", color = "rebeccapurple",
                width = "1.5rem", height = "50%",
              } }
            end"##,
    );
    let st = s.view().expect("view ok").style;
    assert!(st.background.is_some(), "hsl() parses");
    assert_eq!(st.color, Color32::from_rgb(0x66, 0x33, 0x99), "named CSS color parses");
    assert_eq!(st.width, crate::node::Val::Px(24.0), "rem = 16px");
    assert_eq!(st.height, crate::node::Val::Pct(50.0));
}

// --- Sandbox ----------------------------------------------------------------------------------

#[test]
fn sandbox_has_no_os_io_or_loaders() {
    let mut s = load(
        r#"return function()
             local leaks = {}
             for _, k in ipairs({ "os", "io", "package", "debug", "dofile", "loadfile", "load" }) do
               if _G[k] ~= nil then leaks[#leaks + 1] = k end
             end
             return ui.text{ table.concat(leaks, ",") }
           end"#,
    );
    let n = s.view().expect("view ok");
    assert_eq!(n.plain_text().as_deref(), Some(""), "no sandbox-hostile global is visible");
}

#[test]
fn runaway_loop_errors_instead_of_hanging() {
    let mut s = load("return function() while true do end end");
    let err = s.view().expect_err("budget exhausted");
    assert!(err.contains("instruction budget"), "got: {err}");
    // …and the VM recovers: the next entry gets a fresh budget.
    let mut ok = load(r#"return function() return ui.text{ "fine" } end"#);
    assert!(ok.view().is_ok());
}

#[test]
fn memory_bomb_errors_instead_of_oom() {
    let mut s = load(
        r#"return function()
             local t = {}
             for i = 1, 1e9 do t[i] = string.rep("x", 1024) end
             return ui.text{ "unreachable" }
           end"#,
    );
    assert!(s.view().is_err(), "allocation past the cap errors");
}

// --- ui.table ----------------------------------------------------------------

#[test]
fn ui_table_renders_header_divider_and_rows() {
    let mut s = load(
        r##"
        local t = doc:list("orders")
        if #t == 0 then
          t:add{ ref = "A-1", qty = 2, paid = true,  status = "open" }
          t:add{ ref = "A-2", qty = 5, paid = false, status = "done" }
        end
        return function()
          return ui.table{
            rows = doc:list("orders"),
            columns = {
              { key = "ref", label = "Ref", width = 100 },
              { key = "qty", type = "number", width = 60 },
              { key = "paid", type = "check", width = 50 },
              { key = "status", type = "select" },
            },
          }
        end"##,
    );
    let root = s.view().expect("view ok");
    assert_eq!(root.children.len(), 3, "header + divider + body");
    assert!(root.scroll.as_ref().is_some_and(|s| s.x && !s.y), "outer region scrolls x");
    let header = &root.children[0];
    assert_eq!(header.children[0].children[0].plain_text().as_deref(), Some("Ref"));
    assert_eq!(header.children[1].children[0].plain_text().as_deref(), Some("qty"), "label defaults to key");
    let body = &root.children[2];
    assert!(body.scroll.as_ref().is_some_and(|s| s.y && !s.x), "body region scrolls y");
    assert_eq!(body.children.len(), 2, "two data rows");
    let row1 = &body.children[0];
    assert_eq!(row1.children[0].children[0].plain_text().as_deref(), Some("A-1"));
    assert_eq!(row1.children[1].children[0].plain_text().as_deref(), Some("2"), "whole numbers drop the .0");
    assert!(row1.style.background.is_none(), "first row unbanded");
    assert!(body.children[1].style.background.is_some(), "second row banded");
}

#[test]
fn table_where_filters_and_order_by_sorts() {
    let mut s = load(
        r##"
        local t = doc:list("tasks")
        t:add{ name = "a", pts = 1, open = true }
        t:add{ name = "b", pts = 9, open = true }
        t:add{ name = "c", pts = 5, open = false }
        t:add{ name = "d", pts = 5, open = true }
        return function()
          return ui.table{
            rows = doc:list("tasks"),
            where = { open = true },
            order_by = { "pts", desc = true },
            columns = { { key = "name" } },
          }
        end"##,
    );
    let root = s.view().expect("view ok");
    let names: Vec<String> = root.children[2]
        .children
        .iter()
        .filter_map(|r| r.children[0].children[0].plain_text())
        .collect();
    assert_eq!(names, ["b", "d", "a"], "closed row filtered out; descending by pts");
}

#[test]
fn list_add_stamps_a_stable_row_id() {
    let doc = Rc::new(LoroDoc::new());
    let _s = Script::load(
        r#"doc:list("t"):add{ a = 1 }
           doc:list("t"):add{ id = "mine", a = 2 }
           return function() return ui.col{} end"#,
        doc.clone(),
    );
    use loro::{Container, LoroValue, ValueOrContainer};
    let get_id = |i: usize| match doc.get_movable_list("t").get(i) {
        Some(ValueOrContainer::Container(Container::Map(m))) => match m.get("id") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    };
    assert!(get_id(0).is_some_and(|id| !id.is_empty()), "auto id stamped at birth");
    assert_eq!(get_id(1).as_deref(), Some("mine"), "an app-supplied id wins");
}

// Edits address "first row with this id", so add rejects colliding / non-string / '#'-ids.
#[test]
fn list_add_rejects_bad_row_ids() {
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(
        r##"local t = doc:list("t")
           t:add{ id = "dup", a = 1 }
           local dup = select(2, pcall(function() t:add{ id = "dup", a = 2 } end))
           local num = select(2, pcall(function() t:add{ id = 42 } end))
           local hash = select(2, pcall(function() t:add{ id = "#1" } end))
           doc:map("out").dup = tostring(dup)
           doc:map("out").num = tostring(num)
           doc:map("out").hash = tostring(hash)
           return function() return ui.col{} end"##,
        doc.clone(),
    );
    assert!(s.view().is_ok(), "guarded adds fail inside pcall, not the script");
    assert_eq!(doc.get_movable_list("t").len(), 1, "only the first dup row landed");
    let out = |k: &str| match doc.get_map("out").get(k) {
        Some(loro::ValueOrContainer::Value(loro::LoroValue::String(s))) => s.to_string(),
        _ => String::new(),
    };
    assert!(out("dup").contains("duplicate row id 'dup'"), "got: {}", out("dup"));
    assert!(out("num").contains("row id must be a string"), "got: {}", out("num"));
    assert!(out("hash").contains("may not start with '#'"), "got: {}", out("hash"));
}

#[test]
fn check_cell_click_flips_the_right_row_under_sort() {
    let doc = Rc::new(LoroDoc::new());
    let mut s = Script::load(
        r##"
        local t = doc:list("tasks")
        t:add{ name = "low",  pts = 1, done = false }
        t:add{ name = "high", pts = 9, done = false }
        return function()
          return ui.table{
            rows = doc:list("tasks"),
            order_by = { "pts", desc = true },
            columns = { { key = "name" }, { key = "done", type = "check" } },
          }
        end"##,
        doc.clone(),
    );
    let root = s.view().expect("view ok");
    // Display row 0 is "high" (sorted desc); its check cell carries the toggle handler.
    let first = &root.children[2].children[0];
    assert_eq!(first.children[0].children[0].plain_text().as_deref(), Some("high"));
    let handler = first.children[1].on_click.expect("check cell is clickable");
    s.dispatch(handler).expect("toggle runs");

    use loro::{Container, LoroValue, ValueOrContainer};
    let done = |i: usize| match doc.get_movable_list("tasks").get(i) {
        Some(ValueOrContainer::Container(Container::Map(m))) => {
            matches!(m.get("done"), Some(ValueOrContainer::Value(LoroValue::Bool(true))))
        }
        _ => false,
    };
    assert!(!done(0), "'low' (source index 0) untouched");
    assert!(done(1), "'high' (source index 1) flipped despite sitting first in display order");
}
