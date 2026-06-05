//! Headless tests for the selection model: shift-extension, replace-on-type, delete, and
//! plain-arrow collapse — driven by synthetic egui key/text events (no window/GPU).
#![allow(deprecated)] // `Context::run` is the simplest headless driver

use doc_editor::{Doc, DocEditor};
use eframe::egui;

use std::sync::atomic::{AtomicU64, Ordering};

/// Advancing wall-clock for each frame — real frames are ~16 ms apart, and loro's undo
/// grouping is timestamp-sensitive, so a static `time` (the egui default) is unrealistic.
static FRAME_TICK: AtomicU64 = AtomicU64::new(0);

fn frame_mode(ctx: &egui::Context, editor: &mut DocEditor, doc: &Doc, events: Vec<egui::Event>, read_only: bool) {
    let t = FRAME_TICK.fetch_add(1, Ordering::Relaxed) as f64 * 0.05; // 50 ms/frame
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0))),
        focused: true,
        time: Some(t),
        events,
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            editor.show(ui, doc, read_only);
        });
    });
}

fn frame(ctx: &egui::Context, editor: &mut DocEditor, doc: &Doc, events: Vec<egui::Event>) {
    frame_mode(ctx, editor, doc, events, false);
}

fn frame_ro(ctx: &egui::Context, editor: &mut DocEditor, doc: &Doc, events: Vec<egui::Event>) {
    frame_mode(ctx, editor, doc, events, true);
}

fn text(t: &str) -> egui::Event {
    egui::Event::Text(t.to_owned())
}

fn key(k: egui::Key, shift: bool) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers { shift, ..Default::default() },
    }
}

fn paste(t: &str) -> egui::Event {
    egui::Event::Paste(t.to_owned())
}

fn cmd(k: egui::Key, shift: bool) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers { command: true, shift, ..Default::default() },
    }
}

/// Type `word` into a fresh single-block doc, returning the doc, editor, and that block.
fn typed(ctx: &egui::Context, word: &str) -> (Doc, DocEditor, loro::TreeID) {
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    let block = doc.block_ids()[0];
    frame(ctx, &mut editor, &doc, vec![]); // establish focus
    for ch in word.chars() {
        frame(ctx, &mut editor, &doc, vec![text(&ch.to_string())]);
    }
    (doc, editor, block)
}

/// Shift+Left extends a selection; typing over it replaces the selected text.
#[test]
fn shift_left_selects_and_typing_replaces() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello");
    assert_eq!(doc.text(block), "hello");

    // Select the last two chars ("lo"), then type "p".
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![text("p")]);

    assert_eq!(doc.text(block), "help", "typing over a selection replaces it");
}

/// Backspace with a non-empty selection deletes the range, not just one char.
#[test]
fn backspace_deletes_selection() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello");

    // Select the last three chars ("llo"), then Backspace.
    for _ in 0..3 {
        frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    }
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Backspace, false)]);

    assert_eq!(doc.text(block), "he", "Backspace deletes the whole selection");
}

/// A selection spanning two blocks deletes across the boundary and merges them.
#[test]
fn cross_block_selection_deletes_and_merges() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    frame(&ctx, &mut editor, &doc, vec![]); // focus

    for ch in "abc".chars() {
        frame(&ctx, &mut editor, &doc, vec![text(&ch.to_string())]);
    }
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Enter, false)]); // split → 2nd block
    for ch in "def".chars() {
        frame(&ctx, &mut editor, &doc, vec![text(&ch.to_string())]);
    }
    assert_eq!(doc.block_ids().len(), 2);

    // From the end of "def", extend up across the boundary into "abc", then delete.
    for _ in 0..5 {
        frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    }
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Backspace, false)]);

    let ids = doc.block_ids();
    assert_eq!(ids.len(), 1, "the two blocks merge into one");
    assert_eq!(doc.text(ids[0]), "ab", "the spanned text is gone, the remainder joined");
}

/// Single-line paste inserts at the caret.
#[test]
fn paste_single_line_inserts_at_caret() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "ab");
    frame(&ctx, &mut editor, &doc, vec![paste("XY")]);
    assert_eq!(doc.text(block), "abXY");
    assert_eq!(doc.block_ids().len(), 1);
}

/// Multi-line paste splits into one paragraph per line.
#[test]
fn paste_multiline_splits_into_blocks() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    frame(&ctx, &mut editor, &doc, vec![]); // focus
    frame(&ctx, &mut editor, &doc, vec![paste("one\ntwo\nthree")]);

    let ids = doc.block_ids();
    assert_eq!(ids.len(), 3, "three lines → three blocks");
    assert_eq!(doc.text(ids[0]), "one");
    assert_eq!(doc.text(ids[1]), "two");
    assert_eq!(doc.text(ids[2]), "three");
}

/// Paste over a selection replaces it.
#[test]
fn paste_replaces_selection() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "abcd");
    // Select "cd", then paste "X".
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![paste("X")]);
    assert_eq!(doc.text(block), "abX");
}

/// Cut deletes the selection (and would place it on the clipboard).
#[test]
fn cut_deletes_selection() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello");
    for _ in 0..3 {
        frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    }
    frame(&ctx, &mut editor, &doc, vec![egui::Event::Cut]);
    assert_eq!(doc.text(block), "he", "Cut removes the selected text");
}

/// Undo reverts an edit; redo restores it (the seed paragraph itself is not undoable).
#[test]
fn undo_then_redo_round_trips_an_edit() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "a");
    assert_eq!(doc.text(block), "a");

    // Idle frames between actions, as in real use (time advances, no commits in between).
    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::Z, false)]);
    assert_eq!(doc.text(block), "", "Ctrl+Z reverts the typed character");

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::Z, true)]);
    assert_eq!(doc.text(block), "a", "Ctrl+Shift+Z restores it");
}

/// Undo/redo round-trips a structural edit too (multi-line paste creates blocks with text).
#[test]
fn undo_redo_round_trips_a_paste() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    frame(&ctx, &mut editor, &doc, vec![]); // focus
    frame(&ctx, &mut editor, &doc, vec![paste("one\ntwo")]);
    assert_eq!(doc.block_ids().len(), 2);

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::Z, false)]);
    assert_eq!(doc.block_ids().len(), 1, "undo reverts the whole paste");

    frame(&ctx, &mut editor, &doc, vec![]);
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::Z, true)]);
    let ids = doc.block_ids();
    assert_eq!(ids.len(), 2, "redo restores the paste");
    assert_eq!(doc.text(ids[1]), "two");
}

/// Read-only mode suppresses edits (typing, backspace) but keeps the caret/selection live.
#[test]
fn read_only_blocks_edits() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello"); // editable setup

    frame_ro(&ctx, &mut editor, &doc, vec![text("x")]);
    assert_eq!(doc.text(block), "hello", "typing is ignored in read-only");

    frame_ro(&ctx, &mut editor, &doc, vec![key(egui::Key::Backspace, false)]);
    assert_eq!(doc.text(block), "hello", "backspace is ignored in read-only");

    frame_ro(&ctx, &mut editor, &doc, vec![paste("zzz")]);
    assert_eq!(doc.text(block), "hello", "paste is ignored in read-only");

    // Selection still works in a reader (extend left), and the doc is untouched.
    frame_ro(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    assert_eq!(doc.text(block), "hello", "selection does not mutate the document");
}

/// Tab nests a list item under the previous one (structurally); Shift-Tab lifts it back.
#[test]
fn tab_nests_list_item_and_shift_tab_outdents() {
    let ctx = egui::Context::default();
    let doc = Doc::new();
    let mut editor = DocEditor::new();
    frame(&ctx, &mut editor, &doc, vec![]); // focus

    // A bullet, Enter, a second bullet (markdown `- ` converts the first paragraph).
    frame(&ctx, &mut editor, &doc, vec![text("- ")]);
    for ch in "one".chars() {
        frame(&ctx, &mut editor, &doc, vec![text(&ch.to_string())]);
    }
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Enter, false)]);
    for ch in "two".chars() {
        frame(&ctx, &mut editor, &doc, vec![text(&ch.to_string())]);
    }
    let ids = doc.block_ids();
    assert_eq!(ids.len(), 2, "two bullets");

    // Tab on the second bullet nests it under the first.
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Tab, false)]);
    assert_eq!(doc.depth(ids[1]), 1, "Tab nests the second bullet");
    assert_eq!(doc.parent_of(ids[1]), Some(ids[0]));
    assert_eq!(doc.block_ids(), vec![ids[0], ids[1]], "DFS order unchanged: parent then child");

    // Shift-Tab lifts it back to the top level.
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::Tab, true)]);
    assert_eq!(doc.depth(ids[1]), 0, "Shift-Tab outdents");
    assert_eq!(doc.parent_of(ids[1]), None);
}

/// Ctrl+B over a selection bolds it; pressing it again removes the bold.
#[test]
fn ctrl_b_toggles_bold_on_selection() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello");
    // Select all five chars (Shift+Left ×5 from the end).
    for _ in 0..5 {
        frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    }
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::B, false)]);
    assert!(doc.mark_covers(block, 0, 5, "bold"), "Ctrl+B bolds the selection");

    // The selection persists, so a second Ctrl+B toggles it back off.
    frame(&ctx, &mut editor, &doc, vec![cmd(egui::Key::B, false)]);
    assert!(!doc.mark_covers(block, 0, 5, "bold"), "Ctrl+B again removes the bold");
}

/// A plain arrow collapses a selection to its edge without deleting anything.
#[test]
fn plain_arrow_collapses_without_deleting() {
    let ctx = egui::Context::default();
    let (doc, mut editor, block) = typed(&ctx, "hello");

    // Select "lo", then a plain Left collapses to the selection's left edge (index 3).
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, true)]);
    frame(&ctx, &mut editor, &doc, vec![key(egui::Key::ArrowLeft, false)]);
    frame(&ctx, &mut editor, &doc, vec![text("X")]);

    assert_eq!(doc.text(block), "helXlo", "plain arrow collapses, leaving the text intact");
}
