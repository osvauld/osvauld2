//! A reusable single-run text editor: a caret/selection over one editable text run, plus the
//! egui-galley geometry to render and hit-test it. The shared kernel the engine's `ui.input`
//! and doc_editor's blocks build on.
//!
//! Positions are char (code-point) indices, matching egui's `CCursor` — never byte offsets.

use egui::{pos2, text::CCursor, Galley, Rect, Vec2};

/// One editable text run. All positions are char indices; implementors translate to their own
/// addressing (byte offsets for `String`, unicode positions for `LoroText`).
pub trait TextBuffer {
    fn char_len(&self) -> usize;
    fn text(&self) -> String;
    fn insert(&mut self, at: usize, s: &str);
    fn delete(&mut self, at: usize, len: usize);

    /// Whether inline mark `key` is set across the *entire* `[a, b)` range — so a toggle knows to
    /// remove it rather than (re)apply it. Marks are open string keys (`"bold"`, `"link"`, …); a
    /// plain buffer carries none, so the default is `false`.
    fn mark_covers(&self, a: usize, b: usize, key: &str) -> bool {
        let _ = (a, b, key);
        false
    }

    /// Set (`on`) or clear inline mark `key` over `[a, b)`. A plain buffer has no marks, so the
    /// default is a no-op; rich buffers (a `LoroText`, a doc block) override both this and
    /// [`mark_covers`](Self::mark_covers).
    fn set_mark(&mut self, a: usize, b: usize, key: &str, on: bool) {
        let _ = (a, b, key, on);
    }
}

/// A caret + selection over one run. `anchor == head` is a plain caret; otherwise it spans
/// `[min(anchor, head), max(anchor, head))`. Held across frames (retained state immediate-mode
/// would otherwise lose).
#[derive(Clone, Copy, Default)]
pub struct TextField {
    anchor: usize,
    head: usize,
    /// Sticky x for vertical movement in a wrapped run; cleared by any horizontal move.
    desired_x: Option<f32>,
}

impl TextField {
    pub fn new() -> Self {
        Self::default()
    }

    /// The active caret position (the selection head).
    pub fn caret(&self) -> usize {
        self.head
    }

    pub fn collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// The selection as an ordered `(start, end)` range, `start <= end`.
    pub fn selection(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    /// Collapse to a single caret at `i`.
    pub fn set_caret(&mut self, i: usize) {
        self.anchor = i;
        self.head = i;
        self.desired_x = None;
    }

    /// Move the head to `i`; with `extend`, keep the anchor, else collapse to `i`.
    pub fn set_head(&mut self, i: usize, extend: bool) {
        self.head = i;
        if !extend {
            self.anchor = i;
        }
        self.desired_x = None;
    }

    /// Re-clamp both endpoints into `[0, len]` after the buffer changed underneath us (a remote
    /// or MCP edit) — the caret survives instead of pointing past the end. Call each frame
    /// before editing.
    pub fn clamp(&mut self, buf: &dyn TextBuffer) {
        let len = buf.char_len();
        self.anchor = self.anchor.min(len);
        self.head = self.head.min(len);
    }

    // --- editing -------------------------------------------------------------

    /// Insert `s` at the caret (replacing any selection); control chars are dropped. Returns
    /// whether the buffer changed.
    pub fn insert(&mut self, buf: &mut dyn TextBuffer, s: &str) -> bool {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        if s.is_empty() {
            return false;
        }
        if !self.collapsed() {
            self.delete_selection(buf);
        }
        let at = self.head;
        buf.insert(at, &s);
        self.set_caret(at + s.chars().count());
        true
    }

    /// Delete the char before the caret, or the selection if there is one.
    pub fn backspace(&mut self, buf: &mut dyn TextBuffer) -> bool {
        if !self.collapsed() {
            return self.delete_selection(buf);
        }
        if self.head == 0 {
            return false;
        }
        buf.delete(self.head - 1, 1);
        self.set_caret(self.head - 1);
        true
    }

    /// Delete the char after the caret, or the selection if there is one.
    pub fn delete_forward(&mut self, buf: &mut dyn TextBuffer) -> bool {
        if !self.collapsed() {
            return self.delete_selection(buf);
        }
        if self.head >= buf.char_len() {
            return false;
        }
        buf.delete(self.head, 1);
        true
    }

    /// Delete the selected range, collapsing the caret to its start. No-op when collapsed.
    pub fn delete_selection(&mut self, buf: &mut dyn TextBuffer) -> bool {
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        buf.delete(a, b - a);
        self.set_caret(a);
        true
    }

    /// The selected text (empty when collapsed).
    pub fn selected_text(&self, buf: &dyn TextBuffer) -> String {
        let (a, b) = self.selection();
        buf.text().chars().skip(a).take(b - a).collect()
    }

    // --- marks ---------------------------------------------------------------

    /// Toggle inline mark `key` over the selection: removes it when it already covers the whole
    /// selection, else applies it. Leaves the caret and selection where they are (a toolbar/shortcut
    /// keeps editing the same span). No-op returning `false` when there's no selection — a mark
    /// needs a range to land on.
    pub fn toggle_mark(&self, buf: &mut dyn TextBuffer, key: &str) -> bool {
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        let on = !buf.mark_covers(a, b, key);
        buf.set_mark(a, b, key, on);
        true
    }

    // --- horizontal movement -------------------------------------------------

    /// Left one position. Plain: collapse a selection to its left edge, else step left. With
    /// `extend`: grow the selection left.
    pub fn move_left(&mut self, extend: bool) {
        if !extend && !self.collapsed() {
            let (a, _) = self.selection();
            self.set_caret(a);
            return;
        }
        self.set_head(self.head.saturating_sub(1), extend);
    }

    /// Right one position; mirror of [`move_left`].
    pub fn move_right(&mut self, buf: &dyn TextBuffer, extend: bool) {
        if !extend && !self.collapsed() {
            let (_, b) = self.selection();
            self.set_caret(b);
            return;
        }
        self.set_head((self.head + 1).min(buf.char_len()), extend);
    }

    /// To the run start.
    pub fn home(&mut self, extend: bool) {
        self.set_head(0, extend);
    }

    /// To the run end.
    pub fn end(&mut self, buf: &dyn TextBuffer, extend: bool) {
        self.set_head(buf.char_len(), extend);
    }

    /// Select the whole run.
    pub fn select_all(&mut self, buf: &dyn TextBuffer) {
        self.anchor = 0;
        self.head = buf.char_len();
        self.desired_x = None;
    }

    // --- galley geometry (rendering / hit-testing) ---------------------------

    /// Place the caret at the char nearest a galley-local position (a click). With `extend`,
    /// grow the selection to it (a drag).
    pub fn click(&mut self, galley: &Galley, local: Vec2, extend: bool) {
        self.set_head(char_at(galley, local), extend);
    }

    /// The caret's rect in galley-local coordinates (add the text origin to place it).
    pub fn caret_rect(&self, galley: &Galley) -> Rect {
        caret_rect(galley, self.head)
    }

    /// The selection-highlight rects in galley-local coordinates (empty when collapsed).
    pub fn selection_rects(&self, galley: &Galley) -> Vec<Rect> {
        let (a, b) = self.selection();
        selection_rects(galley, a, b)
    }
}

// --- galley helpers (pure egui) ----------------------------------------------

/// The char index nearest a galley-local position (for click-to-place / drag-select).
pub fn char_at(galley: &Galley, local: Vec2) -> usize {
    galley.cursor_from_pos(local).index
}

/// The caret rect for a char index, in galley-local coordinates.
pub fn caret_rect(galley: &Galley, index: usize) -> Rect {
    galley.pos_from_cursor(CCursor::new(index))
}

/// Galley-local rects covering the char range `[a, b)`, one per visual row it spans. X
/// positions come from `pos_from_cursor`; a selection running off the end of a wrapped row
/// extends to that row's right edge, and one that swallows a row's trailing newline extends a
/// few px further so the selected line break reads as selected.
pub fn selection_rects(galley: &Galley, a: usize, b: usize) -> Vec<Rect> {
    let mut rects = Vec::new();
    if a >= b {
        return rects;
    }
    let mut idx = 0usize; // first char index of the current row
    for row in &galley.rows {
        let row_start = idx;
        let row_end = idx + row.char_count_excluding_newline();
        let nl = row.ends_with_newline as usize;
        let sa = a.max(row_start);
        let sb = b.min(row_end + nl);
        if sa < sb {
            let rr = row.rect();
            let x0 = if a <= row_start { rr.left() } else { galley.pos_from_cursor(CCursor::new(sa)).left() };
            let x1 = if sb > row_end {
                rr.right() + 3.0
            } else if b >= row_end {
                rr.right()
            } else {
                galley.pos_from_cursor(CCursor::new(sb)).left()
            };
            rects.push(Rect::from_min_max(pos2(x0, rr.top()), pos2(x1, rr.bottom())));
        }
        idx = row_start + row.char_count_including_newline();
    }
    rects
}

/// Caret blink phase: whether the caret is visible at `now`, with `origin` the last caret
/// move/edit — so the caret snaps solid on input and only blinks once you pause.
pub fn caret_on(now: f64, origin: f64) -> bool {
    ((now - origin) * 1.4).fract() < 0.6
}

/// A ready [`TextBuffer`] over a plain `String` (char-indexed) — for plain fields and tests.
impl TextBuffer for String {
    fn char_len(&self) -> usize {
        self.chars().count()
    }

    fn text(&self) -> String {
        self.clone()
    }

    fn insert(&mut self, at: usize, s: &str) {
        let byte = self.char_indices().nth(at).map_or(self.len(), |(i, _)| i);
        self.insert_str(byte, s);
    }

    fn delete(&mut self, at: usize, len: usize) {
        let start = self.char_indices().nth(at).map_or(self.len(), |(i, _)| i);
        let end = self.char_indices().nth(at + len).map_or(self.len(), |(i, _)| i);
        self.replace_range(start..end, "");
    }
}

#[cfg(test)]
mod tests;
