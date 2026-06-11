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
    list.scroll = Some("s".to_string());
    list.children =
        (0..5).map(|i| Node::text(format!("row {i}")).height(Val::Px(30.0))).collect();
    let root = Node::col().width(Val::Px(200.0)).height(Val::Px(300.0)).children(vec![list]);

    let ctx = egui::Context::default();
    let mut placed = Vec::new();
    let mut offsets = std::collections::HashMap::new();
    offsets.insert("s".to_string(), 40.0); // scrolled down 40pt
    let _ = ctx.run_ui(cell(200.0, 300.0), |ui| {
        placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &offsets);
    });

    let region = placed.iter().find(|p| p.scroll.is_some()).expect("scroll region placed");
    let (_, max) = region.scroll.clone().unwrap();
    assert!(max > 50.0, "content overflows the 60pt box, so max_scroll ({max}) is large");
    // A child row is clipped to the ~60pt region, not the 300pt cell.
    let child = placed.iter().find(|p| p.text.is_some()).expect("a row placed");
    assert!(child.clip.height() <= region.rect.height() + 0.5, "rows clip to the scroll region");
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
        app.export_pdf(FontBytes { regular: SANS, bold: SANS_SB, mono: MONO }).expect("export");
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
    match app.screenshot(800.0, 600.0, 2.0, clear, FontBytes { regular: SANS, bold: SANS_SB, mono: MONO }) {
        Ok(bytes) => {
            assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "not a PNG");
            assert!(bytes.len() > 10_000, "suspiciously small: {} bytes", bytes.len());
            std::fs::write("/tmp/app_engine_shot.png", &bytes).ok();
        }
        Err(e) if e.contains("adapter") => eprintln!("skipped: {e}"),
        Err(e) => panic!("{e}"),
    }
}
