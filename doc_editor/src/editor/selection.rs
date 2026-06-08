//! The selection model: an anchor/head range over caret positions, the range ops (ordering,
//! text extraction, deletion), and the highlight render. `anchor == head` is a plain caret;
//! otherwise it spans document order. Reads are read-only safe; deletion mutates.

use std::cmp::Ordering;

use egui::{CornerRadius, Painter, Pos2, Rect, Ui};
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
            // Trim the start tail and end head, splice the end's remainder onto the start, then
            // drop every block from the next through the end.
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

    /// Toggle an inline mark over the selection, across every spanned block: removed if it
    /// already covers the whole selection, else applied. No-op when collapsed. Returns changed.
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

    /// Whether `key` covers the entire selection (so the toolbar shows it active). False when
    /// collapsed.
    pub(super) fn selection_has_mark(&self, doc: &Doc, key: &str) -> bool {
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
        (si..=ei).all(|i| {
            let a = if i == si { start.index } else { 0 };
            let b = if i == ei { end.index } else { doc.text_len(ids[i]) };
            a >= b || doc.mark_covers(ids[i], a, b, key)
        })
    }

    /// Put the selected text on the system clipboard. A read — read-only safe; no-op when empty.
    pub(super) fn copy_to_clipboard(&self, ui: &Ui, doc: &Doc) {
        let s = self.selected_text(doc);
        if !s.is_empty() {
            ui.ctx().copy_text(s);
        }
    }

    /// Cut: copy the selection, then delete it. Returns whether the document changed. Mutates.
    pub(super) fn cut(&mut self, ui: &Ui, doc: &Doc) -> bool {
        if self.collapsed() {
            return false;
        }
        self.copy_to_clipboard(ui, doc);
        self.delete_selection(doc);
        true
    }

    /// Paste `text` at the caret, first deleting any selection. Newlines split into paragraphs;
    /// the caret block's original tail rides to the end of the last pasted line. Mutates.
    pub(super) fn paste(&mut self, doc: &Doc, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if !self.collapsed() {
            self.delete_selection(doc);
        }
        let c = self.caret();

        // Code is a single multi-line block: paste verbatim, keeping newlines (and tabs) literal
        // instead of splitting on '\n' into paragraphs. Normalise CRLF; drop other control chars.
        if doc.kind(c.block) == BlockKind::Code {
            let body: String = text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .chars()
                .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
                .collect();
            doc.insert_text(c.block, c.index, &body);
            self.set_caret(Caret { block: c.block, index: c.index + body.chars().count() });
            return true;
        }

        // Prose: paragraphs are separated by BLANK lines; single newlines inside a paragraph are
        // soft wraps, joined with a space — so a hard-wrapped paragraph pastes as ONE block, not
        // one block per wrapped line. (A blank line is a real paragraph break, and splits.)
        let paras = split_paragraphs(text);
        let Some(first) = paras.first() else {
            return false; // only blank lines / control chars — nothing to paste
        };

        if paras.len() == 1 {
            doc.insert_text(c.block, c.index, first);
            self.set_caret(Caret { block: c.block, index: c.index + first.chars().count() });
            return true;
        }

        // Multi-paragraph: truncate the caret block at the caret (saving its tail), append the
        // first paragraph, then emit one new paragraph block per remaining chunk; the last
        // carries the saved tail.
        let block_len = doc.text_len(c.block);
        let tail: String = doc.text(c.block).chars().skip(c.index).collect();
        doc.delete_text(c.block, c.index, block_len - c.index);
        doc.insert_text(c.block, c.index, first);

        let mut after = c.block;
        let mut caret = Caret { block: c.block, index: c.index + first.chars().count() };
        let last = paras.len() - 1;
        for (i, para) in paras.iter().enumerate().skip(1) {
            let content = if i == last { format!("{para}{tail}") } else { para.clone() };
            let nb = doc.insert_after(after, BlockKind::Paragraph, &content);
            after = nb;
            caret = Caret { block: nb, index: para.chars().count() };
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
            for r in text_edit::selection_rects(&p.galley, a, b) {
                painter.rect_filled(r.translate(origin.to_vec2()), CornerRadius::same(0), theme::SEL_BG);
            }
        }
    }
}

/// Split pasted plain text into prose paragraphs. A **blank line** (empty or whitespace-only)
/// is a paragraph boundary; consecutive non-blank lines are **soft wraps** joined with a single
/// space — so a hard-wrapped paragraph becomes one block, not one block per wrapped line. CR/CRLF
/// is normalised; tabs and stray control chars collapse to spaces; runs of whitespace collapse.
/// Returns the cleaned, single-line paragraph strings (no empties).
fn split_paragraphs(text: &str) -> Vec<String> {
    let normalized = text.replace('\r', "\n");
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in normalized.split('\n') {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                chunks.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        chunks.push(cur.join(" "));
    }
    chunks
        .into_iter()
        .map(|p| {
            // Control chars (incl. tabs) → spaces, then collapse whitespace runs to one space.
            let spaced: String = p.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
            spaced.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect()
}
