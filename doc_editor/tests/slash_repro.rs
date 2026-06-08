//! Headless reproduction of the slash-palette flow: drive `DocEditor::show` with synthetic
//! egui events (no window/GPU) and assert on the resulting document.
#![allow(deprecated)] // `Context::run` is the simplest headless driver for this test

use doc_editor::{BlockKind, Doc, DocEditor};
use eframe::egui;

/// Run one editor frame with the given input events; returns whether the doc changed.
fn frame(ctx: &egui::Context, editor: &mut DocEditor, doc: &Doc, events: Vec<egui::Event>) -> bool {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0))),
        focused: true,
        events,
        ..Default::default()
    };
    let mut changed = false;
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            changed = editor.show(ui, doc, false);
        });
    });
    changed
}

fn text(t: &str) -> egui::Event {
    egui::Event::Text(t.to_owned())
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

/// Sanity: plain typing reaches the document (confirms the headless editor has focus).
#[test]
fn plain_typing_reaches_doc() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    let block = doc.block_ids()[0];

    frame(&ctx, &mut editor, &doc, vec![]); // establish focus
    frame(&ctx, &mut editor, &doc, vec![text("a")]);
    frame(&ctx, &mut editor, &doc, vec![text("b")]);

    assert_eq!(doc.text(block), "ab", "typing should reach the focused editor");
}

/// Typing `/` on an empty block must be consumed (open the palette), not inserted.
#[test]
fn slash_is_consumed_on_empty_block() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    let block = doc.block_ids()[0];

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![text("/")]);

    assert_eq!(doc.text(block), "", "`/` should be consumed by the palette, not inserted");
    assert_eq!(doc.block_ids().len(), 1);
}

/// THE BUG: pressing Enter while the palette is open must apply the highlighted item, not
/// split the block.
#[test]
fn enter_in_palette_applies_not_splits() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    let block = doc.block_ids()[0];

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![text("/")]);
    // Separate frame, like a human: press Enter to pick the highlighted item (Text).
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Enter)]);

    assert_eq!(doc.block_ids().len(), 1, "Enter in the palette must NOT split the block");
    assert_eq!(doc.kind(block), BlockKind::Paragraph, "default item is Text → paragraph");
}

/// Filtering then Enter applies the filtered selection (here `h` → Heading 1).
#[test]
fn filter_then_enter_applies_heading() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    let block = doc.block_ids()[0];

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![text("/")]);
    frame(&ctx, &mut editor, &doc, vec![text("h")]); // filter → Heading 1/2/3
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Enter)]);

    assert_eq!(doc.block_ids().len(), 1, "filtering + Enter must not split");
    assert_eq!(doc.kind(block), BlockKind::H1, "first match of `h` is Heading 1");
    assert_eq!(doc.text(block), "", "the query text never enters the document");
}
