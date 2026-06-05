//! The **selection model**: an anchor/head range over caret positions, the range operations
//! (ordering, text extraction, deletion), and the highlight render.
//!
//! A [`Selection`] is two [`super::Caret`] positions. When `anchor == head` it's a plain
//! caret; otherwise it spans the text from the earlier to the later position in document
//! order. Reads (extent, [`selected_text`](DocEditor::selected_text)) are allowed in a
//! read-only document; [`delete_selection`](DocEditor::delete_selection) mutates.

use std::cmp::Ordering;

use egui::{pos2, text::CCursor, CornerRadius, Galley, Painter, Pos2, Rect, Ui};
use loro::TreeID;

use crate::model::{BlockKind, Doc};
use crate::theme;

use super::{Caret, DocEditor, Placed};

/// A text/block range: `anchor` is where the selection began, `head` is the moving caret.
#[derive(Clone, Copy)]
pub(super) struct Selection {
    pub anchor: Caret,
    pub head: Caret,
}

impl Selection {
    pub(super) fn caret(c: Caret) -> Self {
        Selection { anchor: c, head: c }
    }

    fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// Document order of two positions: by block index in `ids`, then by offset.
fn order(ids: &[TreeID], a: Caret, b: Caret) -> Ordering {
    let ai = ids.iter().position(|&x| x == a.block);
    let bi = ids.iter().position(|&x| x == b.block);
    ai.cmp(&bi).then(a.index.cmp(&b.index))
}

impl DocEditor {
    /// The active caret position (the selection head). Valid after `resolve_caret`.
    pub(super) fn caret(&self) -> Caret {
        self.sel.expect("selection resolved").head
    }

    /// Whether the selection is empty (a plain caret).
    pub(super) fn collapsed(&self) -> bool {
        self.sel.map(|s| s.is_collapsed()).unwrap_or(true)
    }

    /// Collapse the selection to a single caret at `c`.
    pub(super) fn set_caret(&mut self, c: Caret) {
        self.sel = Some(Selection::caret(c));
    }

    /// Move the head to `c`. With `extend`, keep the anchor (grow/shrink the range);
    /// otherwise collapse to `c`.
    pub(super) fn set_head(&mut self, c: Caret, extend: bool) {
        if extend {
            let anchor = self.sel.map(|s| s.anchor).unwrap_or(c);
            self.sel = Some(Selection { anchor, head: c });
        } else {
            self.set_caret(c);
        }
    }

    /// The selection endpoints in document order: `(start, end)` with `start <= end`.
    pub(super) fn ordered(&self, ids: &[TreeID]) -> (Caret, Caret) {
        let s = self.sel.expect("selection resolved");
        if order(ids, s.anchor, s.head) == Ordering::Greater {
            (s.head, s.anchor)
        } else {
            (s.anchor, s.head)
        }
    }

    /// Delete the selected range (single- or cross-block), collapsing the caret to its start.
    /// No-op when collapsed.
    pub(super) fn delete_selection(&mut self, doc: &Doc) {
        let ids = doc.block_ids();
        let (start, end) = self.ordered(&ids);
        if start == end {
            return;
        }
        let si = ids.iter().position(|&x| x == start.block).expect("start live");
        let ei = ids.iter().position(|&x| x == end.block).expect("end live");
        if si == ei {
            doc.delete_text(start.block, start.index, end.index - start.index);
        } else {
            // Trim the start block's tail and the end block's head, splice the end's
            // remainder onto the start, then drop every block from the next through the end.
            let start_len = doc.text_len(start.block);
            doc.delete_text(start.block, start.index, start_len - start.index);
            doc.delete_text(end.block, 0, end.index);
            let end_text = doc.text(end.block);
            if !end_text.is_empty() {
                let at = doc.text_len(start.block);
                doc.insert_text(start.block, at, &end_text);
            }
            for &mid in &ids[si + 1..=ei] {
                doc.delete_block(mid);
            }
        }
        self.set_caret(start);
        self.desired_x = None;
    }

    /// The selected text, blocks joined by newlines. Empty when collapsed. (Read-only safe.)
    pub(super) fn selected_text(&self, doc: &Doc) -> String {
        let ids = doc.block_ids();
        let (start, end) = self.ordered(&ids);
        if start == end {
            return String::new();
        }
        let si = ids.iter().position(|&x| x == start.block).expect("start live");
        let ei = ids.iter().position(|&x| x == end.block).expect("end live");
        if si == ei {
            return doc.text(start.block).chars().skip(start.index).take(end.index - start.index).collect();
        }
        let mut out: String = doc.text(start.block).chars().skip(start.index).collect();
        for &mid in &ids[si + 1..ei] {
            out.push('\n');
            out.push_str(&doc.text(mid));
        }
        out.push('\n');
        out.extend(doc.text(end.block).chars().take(end.index));
        out
    }

    /// Toggle an inline mark (`"bold"`/`"italic"`/`"strike"`/`"code"`) over the selection. If
    /// the mark already covers the *whole* selection it's removed, else it's applied — across
    /// every block the selection spans (each block's local sub-range). No-op when the
    /// selection is collapsed (a caret has no range to mark). Returns whether it changed.
    pub(super) fn toggle_mark(&mut self, doc: &Doc, key: &str) -> bool {
        if self.collapsed() {
            return false;
        }
        let ids = doc.block_ids();
        let (start, end) = self.ordered(&ids);
        let (Some(si), Some(ei)) = (
            ids.iter().position(|&x| x == start.block),
            ids.iter().position(|&x| x == end.block),
        ) else {
            return false;
        };
        // The local [a, b) sub-range of the selection within spanned block index `i`.
        let span = |i: usize| {
            let a = if i == si { start.index } else { 0 };
            let b = if i == ei { end.index } else { doc.text_len(ids[i]) };
            (a, b)
        };
        // Toggle direction: remove only if the mark already covers every spanned sub-range.
        let covered = (si..=ei).all(|i| {
            let (a, b) = span(i);
            doc.mark_covers(ids[i], a, b, key)
        });
        for i in si..=ei {
            let (a, b) = span(i);
            if covered {
                doc.unmark(ids[i], a, b, key);
            } else {
                doc.mark(ids[i], a, b, key);
            }
        }
        true
    }

    /// Put the selected text on the system clipboard. A read operation — allowed even in a
    /// read-only document. No-op when nothing is selected.
    pub(super) fn copy_to_clipboard(&self, ui: &Ui, doc: &Doc) {
        let s = self.selected_text(doc);
        if !s.is_empty() {
            ui.ctx().copy_text(s);
        }
    }

    /// Cut: copy the selection to the clipboard, then delete it. Returns whether the
    /// document changed (false when nothing is selected). Mutates — gate on edit mode.
    pub(super) fn cut(&mut self, ui: &Ui, doc: &Doc) -> bool {
        if self.collapsed() {
            return false;
        }
        self.copy_to_clipboard(ui, doc);
        self.delete_selection(doc);
        true
    }

    /// Paste `text` at the caret, first deleting any selection. Newlines split into new
    /// paragraphs: the first line joins the caret block, the block's original tail rides to
    /// the end of the last pasted line, and interior lines become their own blocks. Mutates.
    pub(super) fn paste(&mut self, doc: &Doc, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if !self.collapsed() {
            self.delete_selection(doc);
        }
        let c = self.caret();

        // Normalise: drop carriage returns, split on '\n', strip other control chars per line.
        let lines: Vec<String> = text
            .replace('\r', "")
            .split('\n')
            .map(|l| l.chars().filter(|ch| !ch.is_control()).collect())
            .collect();

        if lines.len() == 1 {
            doc.insert_text(c.block, c.index, &lines[0]);
            self.set_caret(Caret { block: c.block, index: c.index + lines[0].chars().count() });
            return true;
        }

        // Multi-line: truncate the caret block at the caret (saving its tail), append the
        // first line, then emit one new paragraph per remaining line; the last carries the
        // saved tail.
        let block_len = doc.text_len(c.block);
        let tail: String = doc.text(c.block).chars().skip(c.index).collect();
        doc.delete_text(c.block, c.index, block_len - c.index);
        doc.insert_text(c.block, c.index, &lines[0]);

        let mut after = c.block;
        let mut caret = Caret { block: c.block, index: c.index + lines[0].chars().count() };
        let last = lines.len() - 1;
        for (i, line) in lines.iter().enumerate().skip(1) {
            let content = if i == last { format!("{line}{tail}") } else { line.clone() };
            let nb = doc.insert_after(after, BlockKind::Paragraph, &content);
            after = nb;
            caret = Caret { block: nb, index: line.chars().count() };
        }
        self.set_caret(caret);
        true
    }

    /// Paint the selection highlight behind the text. No-op when collapsed.
    pub(super) fn paint_selection(&self, painter: &Painter, rect: Rect, doc: &Doc, placed: &[Placed]) {
        if self.collapsed() {
            return;
        }
        let ids = doc.block_ids();
        let (start, end) = self.ordered(&ids);
        let (Some(si), Some(ei)) = (
            ids.iter().position(|&x| x == start.block),
            ids.iter().position(|&x| x == end.block),
        ) else {
            return;
        };

        for (i, p) in placed.iter().enumerate() {
            if i < si || i > ei {
                continue;
            }
            let a = if i == si { start.index } else { 0 };
            let b = if i == ei { end.index } else { doc.text_len(p.id) };
            let origin = Pos2::new(rect.left() + p.text_x, rect.top() + p.content_top);
            for r in selection_rects(&p.galley, a, b) {
                painter.rect_filled(r.translate(origin.to_vec2()), CornerRadius::same(0), theme::SEL_BG);
            }
        }
    }
}

/// Galley-local rects covering the codepoint range `[a, b)`, one per visual row it spans.
/// X positions come from `pos_from_cursor` (galley space); a selection that runs off the
/// end of a wrapped row extends to that row's right edge instead.
fn selection_rects(galley: &Galley, a: usize, b: usize) -> Vec<Rect> {
    let mut rects = Vec::new();
    if a >= b {
        return rects;
    }
    let mut idx = 0usize; // first codepoint index of the current row
    for row in &galley.rows {
        let row_start = idx;
        let row_end = idx + row.char_count_excluding_newline();
        let sa = a.max(row_start);
        let sb = b.min(row_end);
        if sa < sb {
            let rr = row.rect();
            let x0 = if a <= row_start { rr.left() } else { galley.pos_from_cursor(CCursor::new(sa)).left() };
            let x1 = if b >= row_end { rr.right() } else { galley.pos_from_cursor(CCursor::new(sb)).left() };
            rects.push(Rect::from_min_max(pos2(x0, rr.top()), pos2(x1, rr.bottom())));
        }
        idx = row_start + row.char_count_including_newline();
    }
    rects
}
