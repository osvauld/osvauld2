//! Editing-kernel tests over a `String` buffer — headless, no egui galley needed.

use super::{TextBuffer, TextField};

#[test]
fn insert_and_backspace() {
    let mut buf = String::new();
    let mut f = TextField::new();

    assert!(f.insert(&mut buf, "hello"));
    assert_eq!(buf, "hello");
    assert_eq!(f.caret(), 5);

    assert!(f.backspace(&mut buf));
    assert_eq!(buf, "hell");
    assert_eq!(f.caret(), 4);

    // control chars dropped, empty insert is a no-op
    assert!(!f.insert(&mut buf, "\n"));
    assert_eq!(buf, "hell");
}

#[test]
fn selection_replace_and_delete() {
    let mut buf = String::from("hello world");
    let mut f = TextField::new();
    f.set_caret(0);
    f.set_head(5, true);

    assert_eq!(f.selected_text(&buf), "hello");
    assert!(f.insert(&mut buf, "hi")); // typing replaces the selection
    assert_eq!(buf, "hi world");
    assert_eq!(f.caret(), 2);

    f.set_caret(2);
    f.set_head(8, true);
    assert!(f.backspace(&mut buf)); // backspace deletes a selection
    assert_eq!(buf, "hi");
}

#[test]
fn movement_select_all_and_forward_delete() {
    let mut buf = String::from("abc");
    let mut f = TextField::new();

    f.end(&buf, false);
    assert_eq!(f.caret(), 3);
    f.move_left(false);
    assert_eq!(f.caret(), 2);
    f.home(false);
    assert_eq!(f.caret(), 0);
    f.move_right(&buf, false);
    assert_eq!(f.caret(), 1);

    f.delete_forward(&mut buf);
    assert_eq!(buf, "ac");

    f.select_all(&buf);
    assert_eq!(f.selection(), (0, 2));
    assert!(f.insert(&mut buf, "x"));
    assert_eq!(buf, "x");
}

#[test]
fn clamp_survives_external_shrink() {
    // A remote/MCP edit shortened the buffer under the caret — it re-clamps, doesn't panic.
    let mut f = TextField::new();
    f.set_caret(10);
    let buf = String::from("abc");
    f.clamp(&buf);
    assert_eq!(f.caret(), 3);
}

#[test]
fn unicode_is_char_indexed_not_byte() {
    // Multi-byte chars: positions are code points, so deletes land on char boundaries.
    let mut buf = String::from("áé"); // 2 chars, 4 bytes
    let mut f = TextField::new();
    f.end(&buf, false);
    assert_eq!(f.caret(), 2);
    f.backspace(&mut buf);
    assert_eq!(buf, "á");
}

/// A buffer that records the last `set_mark` and answers `mark_covers` from a canned flag — just
/// enough to pin the field's toggle decision without a real rich-text store (Loro is exercised
/// end-to-end in app_engine).
#[derive(Default)]
struct Recorder {
    covers: bool,
    last: Option<(usize, usize, String, bool)>,
}

impl TextBuffer for Recorder {
    fn char_len(&self) -> usize {
        100
    }
    fn text(&self) -> String {
        String::new()
    }
    fn insert(&mut self, _: usize, _: &str) {}
    fn delete(&mut self, _: usize, _: usize) {}
    fn mark_covers(&self, _: usize, _: usize, _: &str) -> bool {
        self.covers
    }
    fn set_mark(&mut self, a: usize, b: usize, key: &str, on: bool) {
        self.last = Some((a, b, key.to_string(), on));
    }
}

#[test]
fn toggle_mark_applies_then_removes_over_the_selection() {
    let mut f = TextField::new();
    f.set_caret(2);
    f.set_head(7, true); // selection [2, 7)

    // Not yet covered → the toggle applies the mark over the selection.
    let mut buf = Recorder { covers: false, last: None };
    assert!(f.toggle_mark(&mut buf, "bold"));
    assert_eq!(buf.last, Some((2, 7, "bold".to_string(), true)));
    assert_eq!(f.selection(), (2, 7), "a toggle leaves the selection in place");

    // Already covering the whole selection → the toggle removes it.
    let mut buf = Recorder { covers: true, last: None };
    assert!(f.toggle_mark(&mut buf, "bold"));
    assert_eq!(buf.last, Some((2, 7, "bold".to_string(), false)));
}

#[test]
fn toggle_mark_is_a_noop_without_a_selection() {
    let mut f = TextField::new();
    f.set_caret(3); // collapsed caret — nothing to mark
    let mut buf = Recorder::default();
    assert!(!f.toggle_mark(&mut buf, "bold"));
    assert!(buf.last.is_none(), "no mark op when there's no selection");
}
