//! Crate-level tests: the render spine end-to-end and the Lua app path. All headless — no GPU.

use super::*;

// The whole spine end-to-end: build → Taffy layout → egui font shaping → flatten.
#[test]
fn lays_out_demo_tree_to_fill_the_cell() {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    };

    let mut placed = Vec::new();
    let offsets = std::collections::HashMap::new();
    let _ = ctx.run_ui(raw, |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &demo_tree(), &offsets);
    });

    // root + 3 children + (card title + body + tag-row) + 3 tags = 10 boxes.
    assert_eq!(placed.len(), 10, "every node is placed");
    let root = placed[0].rect;
    assert!((root.width() - 800.0).abs() < 1.0, "root fills width, got {}", root.width());
    assert!((root.height() - 600.0).abs() < 1.0, "root fills height, got {}", root.height());
    // The card (last top-level child) is inset by the root's 28pt padding.
    let card = placed.iter().find(|p| p.rect.width() == 360.0).expect("card present");
    assert!((card.rect.min.x - 28.0).abs() < 0.5, "card sits at the page margin");
}

// The Lua demo app runs through frame() and draws geometry (script → walk → render).
#[test]
fn lua_demo_app_produces_a_frame() {
    let mut app = EngineApp::demo_script();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    };
    let frame = app.frame(raw, 1.0);
    assert!(!frame.primitives.is_empty(), "the Lua app drew something");
}

// A runtime error inside the view surfaces as an inline error card, not a panic.
#[test]
fn broken_lua_renders_error_card_not_panic() {
    let mut app = EngineApp::script("return function() return ui.text(nil.x) end");
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
        ..Default::default()
    };
    let frame = app.frame(raw, 1.0);
    assert!(!frame.primitives.is_empty(), "the error card drew something");
}

fn cell(w: f32, h: f32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))),
        ..Default::default()
    }
}

// An app is loaded from a file at run time (not compiled in) and draws.
#[test]
fn loads_lua_from_a_file_at_runtime() {
    let path = std::env::temp_dir().join(format!("osv_app_engine_{}.lua", std::process::id()));
    std::fs::write(&path, r#"return function() return ui.text{ "from a file" } end"#).unwrap();

    let mut app = EngineApp::from_file(&path);
    let frame = app.frame(cell(400.0, 300.0), 1.0);
    assert!(!frame.primitives.is_empty(), "file-loaded app drew something");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}.snapshot", path.display()));
}

// A file-loaded app persists its CRDT to a snapshot beside the source, so reopening it (a fresh
// EngineApp) restores the data instead of starting empty.
#[test]
fn from_file_restores_persisted_data() {
    let path = std::env::temp_dir().join(format!("osv_persist_{}.lua", std::process::id()));
    let snap = std::env::temp_dir().join(format!("osv_persist_{}.lua.snapshot", std::process::id()));
    std::fs::write(&path, r#"local t = doc:list("todos") return function() return ui.text{ "n" } end"#).unwrap();
    let _ = std::fs::remove_file(&snap); // start clean

    // A prior session saved one todo.
    let prior = loro::LoroDoc::new();
    prior.get_movable_list("todos").push_container(loro::LoroMap::new()).unwrap();
    prior.commit();
    std::fs::write(&snap, prior.export(loro::ExportMode::Snapshot).unwrap()).unwrap();

    // Opening the app (a brand-new EngineApp, as reopening the cell does) restores it.
    let app = EngineApp::from_file(&path);
    assert_eq!(app.doc().get_movable_list("todos").len(), 1, "persisted data restored on open");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&snap);
}

// --- Editor: the focus + keyboard loop end-to-end -----------------------------------------

/// A cell input with events (a click, a keystroke) at 1x.
fn input_with(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
        events,
        ..Default::default()
    }
}

fn press(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    }
}

fn release(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    }
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::default() }
}

// Click an editor, type, and the text lands in its LoroText — the whole loop: hit-test → focus →
// keyboard routing → shared editing kernel → CRDT.
#[test]
fn typing_into_a_focused_editor_writes_to_the_doc() {
    let mut app = EngineApp::script(
        r##"return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "note", style = { width = 300, height = 30, background = "#222" } },
             }
           end"##,
    );

    app.frame(input_with(vec![]), 1.0); // frame 1: lay the editor out (registers its binding)
    app.frame(input_with(vec![press(egui::pos2(6.0, 6.0))]), 1.0); // frame 2: click to focus
    app.frame(input_with(vec![egui::Event::Text("hi".into())]), 1.0); // frame 3: type

    assert_eq!(app.doc().get_text("note").to_string(), "hi", "typing reached the editor's LoroText");
}

// With a selection (Ctrl/Cmd+A), typing replaces the whole content — select-all + delete-selection
// against externally-seeded content.
#[test]
fn select_all_then_typing_replaces_editor_content() {
    let mut app = EngineApp::script(
        r#"return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "f", style = { width = 300, height = 30 } },
             }
           end"#,
    );
    app.doc().get_text("f").insert(0, "world").unwrap(); // a peer/MCP seeded it
    app.doc().commit();

    app.frame(input_with(vec![]), 1.0);
    app.frame(input_with(vec![press(egui::pos2(5.0, 5.0))]), 1.0); // focus

    let cmd = egui::Modifiers { command: true, ..Default::default() };
    app.frame(
        input_with(vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: cmd,
            },
            egui::Event::Text("hi".into()),
        ]),
        1.0,
    );

    assert_eq!(app.doc().get_text("f").to_string(), "hi", "select-all + type replaced the content");
}

// Enter fires the editor's `on_submit`: the todo-app flow — type, press Enter, the task is added,
// the draft clears, and focus stays.
#[test]
fn pressing_enter_fires_on_submit_and_clears_the_draft() {
    let mut app = EngineApp::script(
        r#"local items = doc:list("items")
           local draft = doc:text("draft")
           return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "draft", style = { width = 300, height = 26 },
                 on_submit = function()
                   if draft:get() ~= "" then items:add{ text = draft:get() }; draft:set("") end
                 end },
             }
           end"#,
    );

    app.frame(input_with(vec![]), 1.0); // lay out (registers the binding)
    app.frame(input_with(vec![press(egui::pos2(5.0, 5.0))]), 1.0); // focus
    app.frame(input_with(vec![egui::Event::Text("milk".into())]), 1.0); // type
    app.frame(input_with(vec![key(egui::Key::Enter)]), 1.0); // submit

    assert_eq!(app.doc().get_movable_list("items").len(), 1, "Enter added the typed task");
    assert_eq!(app.doc().get_text("draft").to_string(), "", "submit cleared the draft");

    // Focus is retained: typing again goes into the (now empty) draft, not nowhere.
    app.frame(input_with(vec![egui::Event::Text("eggs".into())]), 1.0);
    assert_eq!(app.doc().get_text("draft").to_string(), "eggs", "focus stayed on the input after submit");
}

// Drag-to-select: press to set the anchor, drag past the end to grow the selection, then type —
// the whole dragged range is replaced.
#[test]
fn drag_selecting_then_typing_replaces_the_range() {
    let mut app = EngineApp::script(
        r#"return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "f", style = { width = 300, height = 30 } },
             }
           end"#,
    );
    app.doc().get_text("f").insert(0, "hello").unwrap();
    app.doc().commit();

    app.frame(input_with(vec![]), 1.0); // lay out
    app.frame(input_with(vec![press(egui::pos2(2.0, 8.0))]), 1.0); // press near start → anchor, focus
    // Drag far past the end while the button stays held → selection grows over the whole word.
    app.frame(input_with(vec![egui::Event::PointerMoved(egui::pos2(280.0, 8.0))]), 1.0);
    app.frame(input_with(vec![egui::Event::Text("x".into())]), 1.0); // type replaces the selection

    assert_eq!(app.doc().get_text("f").to_string(), "x", "drag-select then type replaced the word");
}

// A scroll region taller than its box reports a positive scroll range and clips its children to
// its own rect.
#[test]
fn scroll_region_reports_range_and_clips_children() {
    use crate::node::{Node, Val};
    // A 60pt scroll column of five 30pt rows (~150pt of content) inside a 300pt page.
    let mut list = Node::col().width(Val::Px(200.0)).height(Val::Px(60.0)).gap(0.0);
    list.scroll = Some(crate::node::ScrollSpec::y("s"));
    list.children =
        (0..5).map(|i| Node::text(format!("row {i}")).height(Val::Px(30.0))).collect();
    let root = Node::col().width(Val::Px(200.0)).height(Val::Px(300.0)).children(vec![list]);

    let ctx = egui::Context::default();
    let mut placed = Vec::new();
    let mut offsets = std::collections::HashMap::new();
    offsets.insert("s".to_string(), egui::vec2(0.0, 40.0)); // scrolled down 40pt
    let _ = ctx.run_ui(cell(200.0, 300.0), |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
    });

    let region = placed.iter().find(|p| p.scroll.is_some()).expect("scroll region placed");
    let (_, max) = region.scroll.clone().unwrap();
    assert!(max.y > 50.0, "content overflows the 60pt box, so max_scroll ({max:?}) is large");
    // A child row is clipped to the ~60pt region, not the 300pt cell.
    let child = placed.iter().find(|p| p.text.is_some()).expect("a row placed");
    assert!(child.clip.height() <= region.rect.height() + 0.5, "rows clip to the scroll region");
}

// A non-page app gets a synthetic root scroll region: a natural-height root taller than the host
// reports a scrollable range, so the run pane scrolls like a web page.
#[test]
fn non_page_app_scrolls_when_content_overflows() {
    use crate::node::{Node, Val};
    let tall = Node::col()
        .width(Val::Pct(100.0))
        .children((0..40).map(|i| Node::text(format!("line {i}")).height(Val::Px(30.0))).collect());
    let root = crate::scroll_root(tall);

    let ctx = egui::Context::default();
    let mut placed = Vec::new();
    let offsets = std::collections::HashMap::new();
    let _ = ctx.run_ui(cell(400.0, 300.0), |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
    });

    let (id, max) = placed.iter().find_map(|p| p.scroll.clone()).expect("root scroll region");
    assert_eq!(id, "__root");
    assert!(max.y > 800.0, "1200pt of content in a 300pt host leaves ~900pt of range, got {max:?}");
}

// Shift+wheel through the real input pipeline scrolls a wide region on x: egui swaps the axis
// at input level, wheel_targets routes it, and the offset survives smoothing across frames.
#[test]
fn shift_wheel_scrolls_a_wide_region_horizontally() {
    use crate::node::{Node, Val};
    let mut wide = Node::row().children(vec![Node::col().width(Val::Px(2000.0)).height(Val::Px(50.0))]);
    wide.scroll = Some(crate::node::ScrollSpec::x("x"));
    let root = Node::col().width(Val::Pct(100.0)).children(vec![wide]);
    let mut app = EngineApp::new(root);

    let at = |t: f64, events: Vec<egui::Event>| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
        time: Some(t),
        events,
        ..Default::default()
    };
    let hover = egui::Event::PointerMoved(egui::pos2(200.0, 25.0));
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -3.0),
        modifiers: egui::Modifiers::SHIFT,
        phase: egui::TouchPhase::Move,
    };
    app.frame(at(0.0, vec![hover.clone(), wheel]), 1.0);
    for i in 1..=20 {
        app.frame(at(i as f64 * 0.016, vec![hover.clone()]), 1.0);
    }
    let off = app.scroll.get("x").copied().unwrap_or_default();
    assert!(off.x > 0.0, "shift+wheel moved the x offset, got {off:?}");
    assert_eq!(off.y, 0.0, "no y movement on an x-only region");
}

// Dragging the bottom scrollbar thumb scrolls the region on x, and the press never reaches the
// app's click handler underneath.
#[test]
fn dragging_the_bottom_scrollbar_scrolls_x_and_swallows_the_click() {
    let mut app = EngineApp::script(
        r##"local out = doc:map("out")
            return function()
              return ui.row{ scroll = "x", style = { width = "100%" },
                ui.col{ style = { width = 2000, height = 100, background = "#222" },
                        on_click = function() out.hit = true end },
              }
            end"##,
    );

    app.frame(input_with(vec![]), 1.0); // lay out: region (800x100) overflows by 1200
    app.frame(input_with(vec![press(egui::pos2(10.0, 95.0))]), 1.0); // press the bottom thumb
    app.frame(input_with(vec![egui::Event::PointerMoved(egui::pos2(400.0, 95.0))]), 1.0);

    let off = app.scroll.get("").copied().unwrap_or_default();
    assert!(off.x > 100.0, "thumb drag scrolled x, got {off:?}");
    assert!(
        app.doc().get_map("out").get("hit").is_none(),
        "the press on the bar never reached the app's on_click"
    );
}

// A table whose fixed columns outgrow the host overflows its own scroll region horizontally
// (rows are min-width = the fixed sum), instead of clipping columns away.
#[test]
fn wide_table_scrolls_horizontally() {
    let doc = std::rc::Rc::new(loro::LoroDoc::new());
    let mut s = script::Script::load(
        r#"doc:list("r"):add{ a = "x", b = "y", c = "z" }
           return function()
             return ui.table{ rows = doc:list("r"), columns = {
               { key = "a", width = 300 }, { key = "b", width = 300 }, { key = "c", width = 300 },
             } }
           end"#,
        doc,
    );
    let root = s.view().expect("view ok");

    let ctx = egui::Context::default();
    let mut placed = Vec::new();
    let offsets = std::collections::HashMap::new();
    let _ = ctx.run_ui(cell(400.0, 300.0), |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
    });

    let (id, max) = placed.iter().find_map(|p| p.scroll.clone()).expect("table scroll region");
    assert_eq!(id, "__table:r");
    assert!(max.x > 400.0, "900pt of columns in a 400pt host scrolls x, got {max:?}");
    assert_eq!(max.y, 0.0, "the grid never overflows vertically — y falls through to the page");
}

// A height-constrained table is a data grid: the body scrolls vertically while the header row
// stays pinned above it (the y-region is the body, not the whole grid).
#[test]
fn fixed_height_table_scrolls_its_body_under_a_pinned_header() {
    let doc = std::rc::Rc::new(loro::LoroDoc::new());
    let mut s = script::Script::load(
        r#"local t = doc:list("r")
           for i = 1, 30 do t:add{ n = i } end
           return function()
             return ui.table{ rows = doc:list("r"), style = { height = 200 },
                              columns = { { key = "n", type = "number", width = 60 } } }
           end"#,
        doc,
    );
    let root = s.view().expect("view ok");

    let ctx = egui::Context::default();
    let mut placed = Vec::new();
    let offsets = std::collections::HashMap::new();
    let _ = ctx.run_ui(cell(600.0, 400.0), |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
    });

    let region = |id: &str| {
        placed
            .iter()
            .find_map(|p| p.scroll.clone().filter(|(rid, _)| rid == id))
            .unwrap_or_else(|| panic!("region {id} placed"))
    };
    assert_eq!(region("__table:r").1.y, 0.0, "outer region never scrolls y");
    assert!(region("__tbody:r").1.y > 100.0, "30 rows overflow the body: {:?}", region("__tbody:r").1);
    // The header row sits above the body box (pinned, outside the y-region).
    let body = placed.iter().find(|p| p.scroll.as_ref().is_some_and(|(id, _)| id == "__tbody:r")).unwrap();
    let header_text = placed.iter().find(|p| p.text.is_some()).expect("header label placed");
    assert!(header_text.rect.bottom() <= body.rect.top() + 1.0, "header pinned above the body");
}

/// The engine's own layout of the app's current view (same ctx/fonts/content rect as `frame()`),
/// so a test can aim a click at a placed node.
fn placed_rect(app: &mut EngineApp, find: impl Fn(&layout::Placed) -> bool) -> egui::Rect {
    placed_find(app, find).expect("node placed")
}

fn placed_find(app: &mut EngineApp, find: impl Fn(&layout::Placed) -> bool) -> Option<egui::Rect> {
    let root = match &mut app.view {
        ViewSource::Script { script, .. } => scroll_root(script.view().expect("view ok")),
        ViewSource::Static(n) => scroll_root(n.clone()),
    };
    let offsets = app.scroll.clone();
    let mut found = None;
    let _ = app.ctx.run_ui(input_with(vec![]), |ui| {
        let placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
        found = placed.iter().find(|p| find(p)).map(|p| p.rect);
    });
    found
}

// The stable ids of the row nodes actually built when the table is viewed at `viewport_h` with the
// body scrolled to `offset_y` — i.e. the visible window. Drives the live (windowed) view() path.
fn windowed_row_ids(app: &mut EngineApp, viewport_h: f32, offset_y: f32) -> Vec<String> {
    let mut offsets = app.scroll.clone();
    offsets.insert("__tbody:r".to_string(), egui::vec2(0.0, offset_y));
    let root = match &mut app.view {
        ViewSource::Script { script, .. } => {
            script.set_viewport(viewport_h, &offsets);
            scroll_root(script.view().expect("view ok"))
        }
        ViewSource::Static(_) => unreachable!(),
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, viewport_h));
    let mut ids = Vec::new();
    let _ = app.ctx.run_ui(cell(600.0, viewport_h), |ui| {
        let placed = layout::layout(ui.ctx(), rect, &root, &offsets);
        ids = placed
            .iter()
            .filter_map(|p| match &p.resize {
                Some(node::Resize::Row { row, .. }) => Some(row.clone()),
                _ => None,
            })
            .collect();
    });
    ids
}

fn map_i64(doc: &loro::LoroDoc, map: &str, key: &str) -> Option<i64> {
    match doc.get_map(map).get(key) {
        Some(loro::ValueOrContainer::Value(loro::LoroValue::I64(n))) => Some(n),
        Some(loro::ValueOrContainer::Value(loro::LoroValue::Double(d))) => Some(d as i64),
        _ => None,
    }
}

// Click a text cell, select-all, type — the keystrokes land in THAT row's field, addressed by
// stable id: display order is sorted, so the row shown first is the *last* list row.
#[test]
fn typing_into_a_table_cell_edits_that_row_under_sort() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "banana", n = 2 }; r:add{ name = "apple", n = 1 } end
           return function()
             return ui.table{ rows = doc:list("r"), order_by = "n",
               columns = { { key = "name" }, { key = "n", type = "number", width = 60 } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0); // lay out (seeds rows, registers cell bindings)

    let rows = table::read_rows(app.doc(), "r");
    let apple = rows[1].id.clone(); // list order: banana, apple — apple displays first
    let cell_id = format!("__cell:r:{apple}:name");
    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some(cell_id.as_str()));

    app.frame(input_with(vec![press(rect.center())]), 1.0); // click the cell → focus + caret
    let cmd = egui::Modifiers { command: true, ..Default::default() };
    app.frame(
        input_with(vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: cmd,
            },
            egui::Event::Text("grape".into()),
        ]),
        1.0,
    );

    let rows = table::read_rows(app.doc(), "r");
    let name = |id: &str| {
        rows.iter().find(|r| r.id == id).and_then(|r| r.cells.get("name")).cloned().unwrap()
    };
    assert_eq!(name(&apple), table::CellValue::Text("grape".into()), "the clicked row changed");
    assert_eq!(
        name(&rows[0].id),
        table::CellValue::Text("banana".into()),
        "the other row is untouched — the edit addressed by id, not display index"
    );
}

// `on_edit(row, key)` is the cell's commit hook: Enter fires it; blur fires it only after edits
// (and not again when Enter already committed).
#[test]
fn cell_on_edit_fires_on_enter_and_on_blur_after_edits() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ id = "a", name = "x" } end
           local out = doc:map("out")
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name" } },
               on_edit = function(row, key)
                 out.row = row; out.key = key; out.n = (out.n or 0) + 1
               end }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some("__cell:r:a:name"));
    app.frame(input_with(vec![press(rect.center())]), 1.0); // focus the cell
    app.frame(input_with(vec![egui::Event::Text("!".into())]), 1.0); // edit
    app.frame(input_with(vec![key(egui::Key::Enter)]), 1.0); // Enter commits
    assert_eq!(map_i64(app.doc(), "out", "n"), Some(1), "Enter fired on_edit once");

    app.frame(input_with(vec![press(egui::pos2(700.0, 550.0))]), 1.0); // blur, nothing typed since
    assert_eq!(map_i64(app.doc(), "out", "n"), Some(1), "a clean blur after Enter doesn't re-fire");

    app.frame(input_with(vec![press(rect.center())]), 1.0); // back in
    app.frame(input_with(vec![egui::Event::Text("?".into())]), 1.0); // edit
    app.frame(input_with(vec![press(egui::pos2(700.0, 550.0))]), 1.0); // blur commits
    assert_eq!(map_i64(app.doc(), "out", "n"), Some(2), "blur-after-edit fired on_edit");
    let row = app.doc().get_map("out").get("row");
    assert!(
        matches!(row, Some(loro::ValueOrContainer::Value(loro::LoroValue::String(ref s))) if **s == *"a"),
        "the handler saw the row id, got {row:?}"
    );
}

// `row_height` fixes each data row's box, so consecutive rows sit exactly that far apart.
#[test]
fn row_height_spaces_table_rows() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "banana" }; r:add{ name = "apple" } end
           return function()
             return ui.table{ rows = doc:list("r"), row_height = 40,
               columns = { { key = "name", locked = true } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let by_text = |t: &'static str| {
        move |p: &layout::Placed| p.text.as_ref().is_some_and(|(_, g)| g.job.text == t)
    };
    let banana = placed_rect(&mut app, by_text("banana"));
    let apple = placed_rect(&mut app, by_text("apple"));
    assert_eq!(apple.top() - banana.top(), 40.0, "rows stack at the fixed height");
}

// Dragging a header cell's right edge resizes that column: press the grab band, move, release.
// The dragged width overrides the declared one and sticks for later frames.
#[test]
fn dragging_a_column_edge_resizes_it() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "a", n = 1 } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name", width = 100 }, { key = "n" } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let name_col = |p: &layout::Placed| {
        matches!(&p.resize, Some(node::Resize::Col { key, .. }) if key == "name")
    };
    let rect = placed_rect(&mut app, name_col);
    assert_eq!(rect.width(), 100.0);

    let edge = egui::pos2(rect.right(), rect.center().y);
    let moved = edge + egui::vec2(40.0, 0.0);
    app.frame(input_with(vec![press(edge)]), 1.0);
    app.frame(input_with(vec![egui::Event::PointerMoved(moved)]), 1.0);
    app.frame(input_with(vec![release(moved)]), 1.0);

    assert_eq!(placed_rect(&mut app, name_col).width(), 140.0, "the column followed the drag");
}

// Dragging a data row's bottom edge resizes THAT row (by stable id); its neighbours keep theirs.
#[test]
fn dragging_a_row_edge_resizes_that_row_only() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "a" }; r:add{ name = "b" } end
           return function()
             return ui.table{ rows = doc:list("r"), columns = { { key = "name" } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let ids: Vec<String> = table::read_rows(app.doc(), "r").into_iter().map(|r| r.id).collect();
    let row = |id: String| {
        move |p: &layout::Placed| {
            matches!(&p.resize, Some(node::Resize::Row { row, .. }) if *row == id)
        }
    };
    let rect = placed_rect(&mut app, row(ids[0].clone()));
    let h0 = rect.height();

    let edge = egui::pos2(rect.center().x, rect.bottom());
    let moved = edge + egui::vec2(0.0, 12.0);
    app.frame(input_with(vec![press(edge)]), 1.0);
    app.frame(input_with(vec![egui::Event::PointerMoved(moved)]), 1.0);
    app.frame(input_with(vec![release(moved)]), 1.0);

    assert_eq!(placed_rect(&mut app, row(ids[0].clone())).height(), h0 + 12.0, "dragged row grew");
    assert_eq!(placed_rect(&mut app, row(ids[1].clone())).height(), h0, "the other row didn't");
}

// A tall table builds only the rows in (and around) the viewport, and the window tracks the body
// scroll offset — so per-frame cost is O(visible), not O(rows). The off-screen rows are reserved by
// spacers, so the scrollbar still spans the whole list.
#[test]
fn table_body_windows_to_the_scrolled_viewport() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then for i = 1, 120 do r:add{ name = "row" .. i } end end
           return function()
             return ui.table{ rows = doc:list("r"), style = { height = "100%" },
               columns = { { key = "name" } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0); // seed the rows + register the table
    let ids: Vec<String> = table::read_rows(app.doc(), "r").into_iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), 120);

    // At the top: a small window including the first row, but not a far one.
    let top = windowed_row_ids(&mut app, 240.0, 0.0);
    assert!(!top.is_empty() && top.len() < 30, "windowed at top, built {} rows", top.len());
    assert!(top.contains(&ids[0]), "first row is built at offset 0");
    assert!(!top.contains(&ids[100]), "a far row is not built at offset 0");

    // Scrolled down ~50 rows: still a small window, now around the middle — the first row is gone.
    let mid = windowed_row_ids(&mut app, 240.0, 1600.0);
    assert!(mid.len() < 30, "still windowed after scrolling, built {} rows", mid.len());
    assert!(!mid.contains(&ids[0]), "scrolled past the first row");
    assert!(mid.contains(&ids[50]), "a mid-list row is now built");
}

// Flex columns carry a per-kind min width, so a wide table's rows exceed a narrow host and the
// x-scroll region gains scrollable range (instead of crushing every column to fit).
#[test]
fn many_columns_overflow_into_horizontal_scroll() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ a="1", b="2", c="3", d="4", e="5", f="6" } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { {key="a"},{key="b"},{key="c"},{key="d"},{key="e"},{key="f"} } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);
    let root = match &mut app.view {
        ViewSource::Script { script, .. } => scroll_root(script.view().expect("view ok")),
        ViewSource::Static(_) => unreachable!(),
    };
    let offsets = app.scroll.clone();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
    let mut max_x = 0.0_f32;
    let _ = app.ctx.run_ui(cell(400.0, 300.0), |ui| {
        let placed = layout::layout(ui.ctx(), rect, &root, &offsets);
        max_x = placed
            .iter()
            .filter_map(|p| p.scroll.as_ref())
            .find(|(id, _)| id == "__table:r")
            .map_or(0.0, |(_, max)| max.x);
    });
    // 6 text columns × 140 min = 840, well past the 400px host.
    assert!(max_x > 250.0, "wide table overflows into x-scroll, got max_x {max_x}");
}

// The scene cache: a pointer-only repaint (hover) reuses the laid-out scene — no view()+Taffy
// rebuild — but a real change (here a scroll) rebuilds. This is what keeps mouse-move off the CPU.
#[test]
fn hover_reuses_the_scene_a_scroll_rebuilds() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then for i = 1, 40 do r:add{ name = "row" .. i } end end
           return function()
             return ui.table{ rows = doc:list("r"), style = { height = "100%" },
               columns = { { key = "name" } } }
           end"#,
    );
    let ctx = egui::Context::default();
    let show = |app: &mut EngineApp, events: Vec<egui::Event>| {
        let _ = ctx.run_ui(input_with(events), |ui| {
            app.show(ui);
        });
    };
    // Two priming frames: the first seeds rows inside view() (which bumps the doc version), the
    // second settles on the now-stable doc.
    show(&mut app, vec![]);
    show(&mut app, vec![]);
    let baseline = app.scene_rebuilds;

    // Pure hover moves: same doc/rect/scroll/focus → reuse, no rebuild.
    show(&mut app, vec![egui::Event::PointerMoved(egui::pos2(40.0, 40.0))]);
    show(&mut app, vec![egui::Event::PointerMoved(egui::pos2(80.0, 120.0))]);
    assert_eq!(app.scene_rebuilds, baseline, "hover moves reuse the cached scene");

    // A scroll changes the key → exactly one rebuild.
    app.scroll.insert("__tbody:r".to_string(), egui::vec2(0.0, 90.0));
    show(&mut app, vec![]);
    assert_eq!(app.scene_rebuilds, baseline + 1, "a scroll change rebuilds once");
}

// The same column drag through `show()` — the path the shell embeds apps with.
#[test]
fn column_drag_works_through_show() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "a", n = 1 } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name", width = 100 }, { key = "n" } } }
           end"#,
    );
    let ctx = egui::Context::default();
    let run = |app: &mut EngineApp, events: Vec<egui::Event>| {
        let _ = ctx.run_ui(input_with(events), |ui| {
            app.show(ui);
        });
    };
    run(&mut app, vec![]);

    let name_col = |p: &layout::Placed| {
        matches!(&p.resize, Some(node::Resize::Col { key, .. }) if key == "name")
    };
    let rect = placed_rect(&mut app, name_col);
    assert_eq!(rect.width(), 100.0);

    let edge = egui::pos2(rect.right(), rect.center().y);
    let moved = edge + egui::vec2(40.0, 0.0);
    run(&mut app, vec![press(edge)]);
    run(&mut app, vec![egui::Event::PointerMoved(moved)]);
    run(&mut app, vec![release(moved)]);

    assert_eq!(placed_rect(&mut app, name_col).width(), 140.0, "show() applied the drag");
}

// A number cell parses its committed text into a real number on Enter (the CRDT holds a
// number, not the typed string); unparseable text stays as typed.
#[test]
fn number_cell_commits_a_real_number() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "a", n = 1 } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name" }, { key = "n", type = "number", width = 80 } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let id = table::read_rows(app.doc(), "r")[0].id.clone();
    let cell = format!("__cell:r:{id}:n");
    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some(cell.as_str()));
    app.frame(input_with(vec![press(rect.center())]), 1.0); // focus
    let cmd = egui::Modifiers { command: true, ..Default::default() };
    let select_all = egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: cmd,
    };
    app.frame(input_with(vec![select_all, egui::Event::Text("42.5".into()), key(egui::Key::Enter)]), 1.0);

    let rows = table::read_rows(app.doc(), "r");
    assert_eq!(
        rows[0].cells.get("n"),
        Some(&table::CellValue::Number(42.5)),
        "Enter parsed the typed text into a number"
    );
}

// A date cell normalizes a loosely-typed date to canonical YYYY-MM-DD on blur.
#[test]
fn date_cell_normalizes_on_blur() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "a", due = "" } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name" }, { key = "due", type = "date", width = 110 } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let id = table::read_rows(app.doc(), "r")[0].id.clone();
    let cell = format!("__cell:r:{id}:due");
    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some(cell.as_str()));
    app.frame(input_with(vec![press(rect.center())]), 1.0); // focus
    app.frame(input_with(vec![egui::Event::Text("2026-6-1".into())]), 1.0); // type loosely
    app.frame(input_with(vec![press(egui::pos2(700.0, 550.0))]), 1.0); // blur commits

    let rows = table::read_rows(app.doc(), "r");
    assert_eq!(
        rows[0].cells.get("due"),
        Some(&table::CellValue::Text("2026-06-01".into())),
        "blur normalized the date"
    );
}

// The select combobox: focusing the cell opens its dropdown, transitions gate which options it
// offers, clicking one commits the value and closes the box.
#[test]
fn select_cell_dropdown_respects_transitions_and_commits() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ task = "ship", status = "open" } end
           local out = doc:map("out")
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "task" },
                 { key = "status", type = "select", width = 120,
                   options = { "open", "doing", "done" },
                   transitions = { open = { "doing" }, doing = { "done", "open" } } } },
               on_edit = function(row, key) out.n = (out.n or 0) + 1; out.key = key end }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let id = table::read_rows(app.doc(), "r")[0].id.clone();
    let cell = format!("__cell:r:{id}:status");
    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some(cell.as_str()));
    app.frame(input_with(vec![press(rect.center())]), 1.0); // focus the cell
    app.frame(input_with(vec![]), 1.0); // re-view: dropdown open

    // The option's label (its clickable row is the parent; a click falls through to it).
    let option = |t: &'static str| {
        move |p: &layout::Placed| p.text.as_ref().is_some_and(|(_, g)| g.job.text == t)
    };
    let doing = placed_rect(&mut app, option("doing"));
    assert!(
        placed_find(&mut app, option("done")).is_none(),
        "open → done is not a legal transition, so the dropdown must not offer it"
    );

    app.frame(input_with(vec![press(doing.center())]), 1.0); // pick "doing"
    let rows = table::read_rows(app.doc(), "r");
    assert_eq!(rows[0].cells.get("status"), Some(&table::CellValue::Text("doing".into())));
    assert_eq!(map_i64(app.doc(), "out", "n"), Some(1), "the commit fired on_edit");

    app.frame(input_with(vec![]), 1.0);
    assert!(
        placed_find(&mut app, option("done")).is_none() && placed_find(&mut app, option("open")).is_none(),
        "committing closed the dropdown"
    );
}

// Typing into a focused select filters the candidates; Enter commits the first match.
#[test]
fn select_typeahead_filters_and_enter_commits() {
    let mut app = EngineApp::script(
        r#"local r = doc:list("r")
           if #r == 0 then r:add{ name = "x", fruit = "" } end
           return function()
             return ui.table{ rows = doc:list("r"),
               columns = { { key = "name" },
                 { key = "fruit", type = "select", width = 120,
                   options = { "apple", "banana", "grape" } } } }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);

    let id = table::read_rows(app.doc(), "r")[0].id.clone();
    let cell = format!("__cell:r:{id}:fruit");
    let rect = placed_rect(&mut app, |p| p.editor.as_deref() == Some(cell.as_str()));
    app.frame(input_with(vec![press(rect.center())]), 1.0); // focus
    app.frame(input_with(vec![egui::Event::Text("gr".into())]), 1.0); // filter
    app.frame(input_with(vec![key(egui::Key::Enter)]), 1.0); // resolve to "grape"

    let rows = table::read_rows(app.doc(), "r");
    assert_eq!(
        rows[0].cells.get("fruit"),
        Some(&table::CellValue::Text("grape".into())),
        "Enter committed the typed query's match"
    );

    // The typed filter was scratch state, never row data, and the box closed.
    app.frame(input_with(vec![egui::Event::Text("zzz".into())]), 1.0);
    let rows = table::read_rows(app.doc(), "r");
    assert_eq!(rows[0].cells.get("fruit"), Some(&table::CellValue::Text("grape".into())));
}

// Clicking empty space (no editor, no handler) blurs, so later keystrokes are ignored.
#[test]
fn clicking_empty_space_blurs_the_editor() {
    let mut app = EngineApp::script(
        r#"return function()
             return ui.col{ style = { padding = 0, width = "100%", height = "100%" },
               ui.editor{ id = "f", style = { width = 200, height = 24 } },
             }
           end"#,
    );
    app.frame(input_with(vec![]), 1.0);
    app.frame(input_with(vec![press(egui::pos2(5.0, 5.0))]), 1.0); // focus the editor
    app.frame(input_with(vec![press(egui::pos2(500.0, 400.0))]), 1.0); // click far away → blur
    app.frame(input_with(vec![egui::Event::Text("x".into())]), 1.0); // ignored — nothing focused

    assert_eq!(app.doc().get_text("f").to_string(), "", "a blurred editor ignores keystrokes");
}

/// The count of code points carrying a `true` boolean mark `key` in `name`'s rich-text delta.
fn marked_len(doc: &loro::LoroDoc, name: &str, key: &str) -> usize {
    doc.get_text(name)
        .to_delta()
        .into_iter()
        .map(|d| match d {
            loro::TextDelta::Insert { insert, attributes } => {
                let on = attributes
                    .is_some_and(|a| matches!(a.get(key), Some(loro::LoroValue::Bool(true))));
                if on { insert.chars().count() } else { 0 }
            }
            _ => 0,
        })
        .sum()
}

// Cmd+B over a selection marks the editor's text bold in its LoroText, and a second Cmd+B clears
// it — the focus → shortcut → shared toggle → CRDT-mark loop, with the selection persisting across
// frames so the second toggle hits the same span.
#[test]
fn cmd_b_toggles_bold_on_the_selection() {
    let mut app = EngineApp::script(
        r#"return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "f", style = { width = 300, height = 30 } },
             }
           end"#,
    );
    app.doc().get_text("f").insert(0, "hello").unwrap(); // seeded (unmarked) content
    app.doc().commit();

    let cmd = egui::Modifiers { command: true, ..Default::default() };
    let cmd_key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: cmd };

    app.frame(input_with(vec![]), 1.0); // lay out (registers the binding)
    // Focus with a full click (press *and* release) so the button isn't left held — a phantom
    // hold would read as a drag and collapse the selection between frames.
    app.frame(input_with(vec![press(egui::pos2(5.0, 5.0)), release(egui::pos2(5.0, 5.0))]), 1.0);
    // Select all, then bold the whole selection.
    app.frame(input_with(vec![cmd_key(egui::Key::A), cmd_key(egui::Key::B)]), 1.0);
    assert_eq!(marked_len(app.doc(), "f", "bold"), 5, "Cmd+B bolded the whole selection");

    // The selection survives the toggle, so a second Cmd+B removes the mark over the same span.
    app.frame(input_with(vec![cmd_key(egui::Key::B)]), 1.0);
    assert_eq!(marked_len(app.doc(), "f", "bold"), 0, "a second Cmd+B cleared the bold");
}

// Bolding a selection must not "leak" into later typing: after marking a span bold, moving the
// caret to the end and typing leaves the new text unbold (the mark expands to neither edge).
#[test]
fn typing_after_a_bold_selection_is_not_bold() {
    let mut app = EngineApp::script(
        r#"return function()
             return ui.col{ style = { padding = 0 },
               ui.editor{ id = "f", style = { width = 300, height = 30 } },
             }
           end"#,
    );
    app.doc().get_text("f").insert(0, "hello").unwrap();
    app.doc().commit();

    let cmd = egui::Modifiers { command: true, ..Default::default() };
    let cmd_key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: cmd };

    app.frame(input_with(vec![]), 1.0); // lay out (registers the binding)
    app.frame(input_with(vec![press(egui::pos2(5.0, 5.0)), release(egui::pos2(5.0, 5.0))]), 1.0); // focus
    // Bold the whole word, then collapse the selection to the end (End key) and type past it.
    app.frame(input_with(vec![cmd_key(egui::Key::A), cmd_key(egui::Key::B)]), 1.0);
    app.frame(input_with(vec![key(egui::Key::End), egui::Event::Text("!".into())]), 1.0);

    assert_eq!(app.doc().get_text("f").to_string(), "hello!", "the char was appended at the end");
    assert_eq!(marked_len(app.doc(), "f", "bold"), 5, "only the original 'hello' stays bold — the new '!' is not");
}

// A missing source file renders an inline error card instead of failing the caller.
#[test]
fn missing_file_renders_error_card_not_panic() {
    let mut app = EngineApp::from_file("/no/such/osv-engine-missing.lua");
    let frame = app.frame(cell(400.0, 300.0), 1.0);
    assert!(!frame.primitives.is_empty(), "missing-file error card drew something");
}

// --- New layout capabilities: alignment, margin, absolute, min/max, border ------------------

/// Lay a tree out in a headless 400×300 cell and return the placed boxes.
fn place(root: &Node) -> Vec<layout::Placed> {
    let ctx = egui::Context::default();
    let raw = cell(400.0, 300.0);
    let mut placed = Vec::new();
    let offsets = std::collections::HashMap::new();
    let _ = ctx.run_ui(raw, |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), root, &offsets);
    });
    placed
}

#[test]
fn justify_and_align_center_a_child() {
    let root = Node::col()
        .width(Val::Px(400.0))
        .height(Val::Px(300.0))
        .justify(node::Align::Center)
        .align(node::Align::Center)
        .children(vec![Node::col().width(Val::Px(100.0)).height(Val::Px(50.0))]);
    let placed = place(&root);
    let child = placed[1].rect;
    assert!((child.min.x - 150.0).abs() < 0.5, "centered horizontally, got {}", child.min.x);
    assert!((child.min.y - 125.0).abs() < 0.5, "centered vertically, got {}", child.min.y);
}

#[test]
fn margin_offsets_a_child() {
    let root = Node::col().width(Val::Px(400.0)).height(Val::Px(300.0)).children(vec![
        Node::col().width(Val::Px(50.0)).height(Val::Px(50.0)).margin(20.0),
    ]);
    let placed = place(&root);
    assert!((placed[1].rect.min.x - 20.0).abs() < 0.5, "margin moved the child in");
    assert!((placed[1].rect.min.y - 20.0).abs() < 0.5);
}

#[test]
fn absolute_child_places_by_inset() {
    let mut badge = Node::col().width(Val::Px(40.0)).height(Val::Px(20.0));
    badge.style.position = node::Position::Absolute;
    badge.style.inset.top = Val::Px(10.0);
    badge.style.inset.left = Val::Px(30.0);
    let root = Node::col()
        .width(Val::Px(400.0))
        .height(Val::Px(300.0))
        .children(vec![Node::col().grow(1.0), badge]);
    let placed = place(&root);
    let b = placed.last().unwrap().rect;
    assert!((b.min.x - 30.0).abs() < 0.5, "absolute left, got {}", b.min.x);
    assert!((b.min.y - 10.0).abs() < 0.5, "absolute top, got {}", b.min.y);
    // …and it didn't consume flex space: the grower still fills the column.
    assert!((placed[1].rect.height() - 300.0).abs() < 1.0, "absolute child is out of flow");
}

#[test]
fn max_width_caps_a_child() {
    let mut child = Node::col().width(Val::Px(500.0)).height(Val::Px(20.0));
    child.style.max_width = Val::Px(120.0);
    let root = Node::col().width(Val::Px(400.0)).children(vec![child]);
    let placed = place(&root);
    assert!((placed[1].rect.width() - 120.0).abs() < 0.5, "max_width wins over width");
}

#[test]
fn border_participates_in_layout() {
    // A bordered box's child starts inside the border (border-box), not under it.
    let root = Node::col()
        .width(Val::Px(200.0))
        .height(Val::Px(100.0))
        .border(5.0, egui::Color32::WHITE)
        .children(vec![Node::col().grow(1.0)]);
    let placed = place(&root);
    assert!((placed[1].rect.min.x - 5.0).abs() < 0.5, "child clears the border");
    assert!((placed[1].rect.width() - 190.0).abs() < 0.5, "content shrinks by both borders");
}

#[test]
fn opacity_multiplies_down_the_subtree() {
    let mut parent = Node::col().opacity(0.5).children(vec![Node::col().opacity(0.5)]);
    parent.style.width = Val::Px(100.0);
    let placed = place(&parent);
    assert!((placed[0].base.opacity - 0.5).abs() < 0.01);
    assert!((placed[1].base.opacity - 0.25).abs() < 0.01, "child folds in the ancestor product");
}

#[test]
fn centered_text_keeps_its_single_line_width() {
    // Repro of the gallery hero: a fixed-height band, justify+align center, a large title and a
    // smaller subtitle. The title must lay out at its full unwrapped width (no overlap below).
    let root = Node::col().width(Val::Pct(100.0)).height(Val::Pct(100.0)).padding(24.0).gap(16.0).children(vec![
        Node::col()
            .height(Val::Px(110.0))
            .justify(crate::node::Align::Center)
            .align(crate::node::Align::Center)
            .gap(6.0)
            .children(vec![
                Node::text("Style Gallery").font(28.0),
                Node::text("alignment · borders · shadows · opacity · absolute · colors").font(13.0),
            ]),
    ]);
    let placed = place(&root);
    let texts: Vec<_> = placed.iter().filter(|p| p.text.is_some()).collect();
    assert_eq!(texts.len(), 2);
    for t in &texts {
        let (_, galley) = t.text.as_ref().unwrap();
        eprintln!("rect={:?} galley={:?} rows={}", t.rect, galley.size(), galley.rows.len());
        assert!(
            galley.size().y <= t.rect.height() + 0.5,
            "painted galley ({}) must fit the laid-out box ({})",
            galley.size().y,
            t.rect.height()
        );
    }
}

// `show()` hosts the app at the Ui's rect, which in the shell sits below a tab strip — clicks
// arrive in screen coordinates and must hit the screen-space layout (regression: input was
// translated to app-local coords while rects stayed screen-space, so every click missed).
#[test]
fn show_dispatches_clicks_when_hosted_at_an_offset() {
    let mut app = EngineApp::script(
        r#"local n = doc:list("clicks")
           return function()
             return ui.col{ style = { padding = 0 },
               ui.button{ "hit me", style = { width = 100, height = 30 },
                 on_click = function() n:add{ at = 1 } end },
             }
           end"#,
    );

    let ctx = egui::Context::default();
    let host = egui::Rect::from_min_size(egui::pos2(200.0, 150.0), egui::vec2(400.0, 300.0));
    let mut run = |events: Vec<egui::Event>| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(raw, |ui| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(host));
            app.show(&mut child);
        });
    };

    run(vec![]); // frame 1: lay out
    run(vec![press(egui::pos2(210.0, 160.0))]); // click inside the button, in screen coords
    assert_eq!(app.doc().get_movable_list("clicks").len(), 1, "the offset-hosted click dispatched");
}

// A page-declaring app exports valid single-page PDF bytes (written to /tmp for inspection).
#[test]
fn exports_page_app_pdf() {
    const SANS: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf");
    const SANS_SB: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf");
    const MONO: &[u8] = include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf");
    let src = r##"
page = { size = "A4" }
return function()
  return ui.col{ style = { padding = 48, gap = 10 },
    ui.text{ "ACME Corporation", style = { font_size = 26, color = "#1a2b4c" } },
    ui.text{ "12 Foundry Lane, Kochi", style = { font_size = 11, color = "#666666" } },
    ui.col{ style = { height = 2, background = "#1a2b4c" } },
    ui.text{ "Dear reader, this letter was laid out by Taffy and printed by printpdf.",
             style = { font_size = 13, color = "#222222" } },
  }
end
"##;
    let mut app = EngineApp::script(src);
    let page = app.page().expect("page declared");
    assert!((page.width - 595.28).abs() < 0.1, "A4 portrait width in pt, got {}", page.width);
    let bytes =
        app.export_pdf(FontBytes { regular: SANS, bold: SANS_SB, mono: MONO, fallback: &[] }).expect("export");
    assert!(bytes.starts_with(b"%PDF"), "not a PDF");
    assert!(bytes.len() > 5_000, "suspiciously small: {} bytes", bytes.len());
    std::fs::write("/tmp/app_engine_page.pdf", &bytes).ok();
}

// Landscape swaps the page dimensions.
#[test]
fn page_orientation_landscape() {
    let app = EngineApp::script("page = { size = \"A4\", orientation = \"landscape\" }\nreturn function() return ui.col{} end");
    let page = app.page().expect("page declared");
    assert!(page.width > page.height);
}

// Headless screenshot: the demo app rendered off-screen comes back as a plausible PNG.
// Needs a GPU adapter; skips (with a note) where none exists.
#[test]
fn screenshots_app_png() {
    const SANS: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf");
    const SANS_SB: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf");
    const MONO: &[u8] = include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf");
    let mut app = EngineApp::demo_script();
    let clear = egui::Color32::from_rgb(0x0a, 0x0b, 0x10);
    match app.screenshot(800.0, 600.0, 2.0, clear, FontBytes { regular: SANS, bold: SANS_SB, mono: MONO, fallback: &[] }) {
        Ok(bytes) => {
            assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "not a PNG");
            assert!(bytes.len() > 10_000, "suspiciously small: {} bytes", bytes.len());
            std::fs::write("/tmp/app_engine_shot.png", &bytes).ok();
        }
        Err(e) if e.contains("adapter") => eprintln!("skipped: {e}"),
        Err(e) => panic!("{e}"),
    }
}


// --- World B: the `data` binding over a stub host (no Polars) -------------------

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use table_core::{CellValue, Column, ColKind, Row, RowOp, TableSpec};

/// A fixed host data plane for tests: a 3-row result, a recorded last write, a togglable
/// `Pending`, and a bumpable version. Exercises the engine→`DataAccess` seam without `table_query`.
#[derive(Default)]
struct StubData {
    version: Cell<u64>,
    pending: Cell<bool>,
    sql_calls: Cell<u32>,
    last_mutate: RefCell<Option<(String, RowOp)>>,
}

fn stub_rows() -> Vec<Row> {
    let row = |id: &str, seg: &str, total: f64| Row {
        id: id.into(),
        cells: HashMap::from([
            ("segment".to_string(), CellValue::Text(seg.into())),
            ("total".to_string(), CellValue::Number(total)),
        ]),
    };
    vec![row("a", "ent", 1000.0), row("b", "smb", 100.0), row("c", "mid", 50.0)]
}

impl crate::data::DataAccess for StubData {
    fn use_source(&self, _alias: &str) {}
    fn sql(&self, _query: &str, _params: &[(String, CellValue)]) -> crate::data::QueryState {
        self.sql_calls.set(self.sql_calls.get() + 1);
        if self.pending.get() {
            crate::data::QueryState::Pending
        } else {
            crate::data::QueryState::Ready(1)
        }
    }
    fn op(&self, _handle: crate::data::Handle, _op: &crate::data::NamedOp) -> crate::data::QueryState {
        crate::data::QueryState::Ready(2)
    }
    fn spec(&self, _handle: crate::data::Handle) -> TableSpec {
        let col = |k: &str, kind| Column {
            key: k.into(),
            label: k.into(),
            kind,
            width: None,
            locked: true,
            options: vec![],
            transitions: HashMap::new(),
        };
        TableSpec {
            columns: vec![col("segment", ColKind::Text), col("total", ColKind::Number)],
            filter: vec![],
            order: None,
            row_height: None,
            row_heights: HashMap::new(),
        }
    }
    fn len(&self, handle: crate::data::Handle) -> usize {
        if handle == 2 { 2 } else { stub_rows().len() }
    }
    fn window(&self, _handle: crate::data::Handle, offset: usize, count: usize) -> Vec<Row> {
        let all = stub_rows();
        let end = (offset + count).min(all.len());
        all[offset.min(all.len())..end].to_vec()
    }
    fn value(&self, _handle: crate::data::Handle, col: &str) -> CellValue {
        stub_rows()[0].cells.get(col).cloned().unwrap_or(CellValue::Empty)
    }
    fn mutate(&self, alias: &str, op: RowOp) -> Result<String, String> {
        *self.last_mutate.borrow_mut() = Some((alias.to_string(), op));
        Ok("newid".into())
    }
    fn version(&self) -> u64 {
        self.version.get()
    }
}

#[test]
fn data_value_scalar_crosses_into_lua() {
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(
        r#"return function()
            local q = data.sql("select 1")
            doc:text("kpi"):set(tostring(q:value("total")))
            return ui.text{ "ok" }
        end"#,
    )
    .with_data_access(stub);
    app.frame(cell(400.0, 300.0), 1.0);
    // Crosses as a Lua number; `tostring` of a float keeps the `.0`.
    assert_eq!(app.doc().get_text("kpi").to_string(), "1000.0", "q:value crossed as a scalar");
}

#[test]
fn data_table_source_renders_rows() {
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(
        r#"return function() return ui.table{ source = data.sql("select *") } end"#,
    )
    .with_data_access(stub);
    let frame = app.frame(cell(500.0, 300.0), 1.0);
    assert!(!frame.primitives.is_empty(), "the query-backed grid drew something");
}

#[test]
fn data_pending_result_renders_placeholder_not_panic() {
    let stub = Rc::new(StubData::default());
    stub.pending.set(true);
    let mut app = EngineApp::script(
        r#"return function() return ui.table{ source = data.sql("select *") } end"#,
    )
    .with_data_access(stub);
    let frame = app.frame(cell(500.0, 300.0), 1.0);
    assert!(!frame.primitives.is_empty(), "a Pending result still draws (placeholder)");
}

#[test]
fn data_pivot_returns_a_result_handle() {
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(
        r#"return function()
            local c = data.sql("select *"):pivot{ on = "m", index = "r", values = "total" }
            doc:text("n"):set(tostring(c:len()))
            return ui.text{ "ok" }
        end"#,
    )
    .with_data_access(stub);
    app.frame(cell(400.0, 300.0), 1.0);
    assert_eq!(app.doc().get_text("n").to_string(), "2", "pivot resolved to its own result");
}

#[test]
fn data_chart_resolves_without_panic() {
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(
        r##"return function()
            return ui.chart{ data = data.sql("select *"), type = "bar", x = "segment", y = "total",
                             style = { background = "#222222" } }
        end"##,
    )
    .with_data_access(stub);
    // frame() doesn't run the chart paint pass (that's show()), but the walk must resolve the
    // ChartSpec from the result without error; the box's background still paints.
    let frame = app.frame(cell(500.0, 300.0), 1.0);
    assert!(!frame.primitives.is_empty(), "the chart leaf laid out and its box painted");
}

#[test]
fn data_mutate_routes_to_source_alias() {
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(
        r#"return function()
            local orders = data.table("orders")
            return ui.button{ "add", on_click = function() orders:add{ x = 1 } end }
        end"#,
    )
    .with_data_access(stub.clone());
    app.frame(cell(200.0, 80.0), 1.0); // lay out (registers the handler)
    app.frame(input_with(vec![press(egui::pos2(10.0, 10.0)), release(egui::pos2(10.0, 10.0))]), 1.0);
    let m = stub.last_mutate.borrow();
    let (alias, op) = m.as_ref().expect("a write reached the host");
    assert_eq!(alias, "orders");
    assert!(matches!(op, RowOp::Add { .. }), "an add op");
}

#[test]
fn analytics_example_loads_and_runs() {
    // The shipped dashboard fixture must parse as Lua and run its view against the data binding —
    // a setup/syntax error would render the error card and never reach `data.sql`.
    let stub = Rc::new(StubData::default());
    let mut app = EngineApp::script(include_str!("../examples/analytics.lua"))
        .with_data_access(stub.clone());
    let frame = app.frame(cell(960.0, 720.0), 1.0);
    assert!(!frame.primitives.is_empty(), "the dashboard drew something");
    assert!(stub.sql_calls.get() > 0, "the view ran its data.sql queries (no setup/syntax error)");
}
