//! Input / editing: text insertion, markdown input rules, the keyboard map, block-editing
//! ops (split / merge / indent), and caret movement. Each turns a key/text event into Loro ops.

use egui::{text::CCursor, Key, Modifiers, Vec2};

use crate::block;
use crate::model::{BlockKind, Doc};

use super::{Caret, DocEditor, Placed};

/// Inline markdown delimiters → the mark they apply, in match order (double before single, so
/// `**` beats `*`). Each is its own opener *and* closer.
const INLINE_RULES: &[(&str, &str)] = &[
    ("**", "bold"),
    ("~~", "strike"),
    ("`", "code"),
    ("*", "italic"),
    ("_", "italic"),
];

impl DocEditor {
    pub(super) fn insert_text(&mut self, doc: &Doc, t: &str) -> bool {
        if t.chars().all(|c| c.is_control()) {
            return false; // nothing will insert — leave any selection alone
        }
        // A cross-block selection needs the structural delete; an in-block one TextField replaces.
        if !self.collapsed() {
            let (start, end) = self.ordered(&doc.block_ids());
            if start.block != end.block {
                self.delete_selection(doc);
            }
        }
        let changed = self.in_block(doc, |tf, buf| tf.insert(buf, t));
        self.desired_x = None;
        changed
    }

    /// Markdown input rules at the start of a paragraph: `# `/`## `/`### ` → heading,
    /// `- `/`* ` → bullet, `1. ` → numbered, `[] ` → to-do, `> ` → quote, ` ``` ` → code,
    /// `---` → divider. The trigger prefix is consumed, never stored.
    pub(super) fn apply_markdown(&mut self, doc: &Doc) {
        let c = self.caret();
        if doc.kind(c.block) != BlockKind::Paragraph {
            return;
        }
        let text = doc.text(c.block);

        // Strip the prefix and set the kind, keeping the caret. Prefixes are mutually exclusive
        // (each carries its exact `#`-count / trailing space), so table order is irrelevant.
        for spec in block::SPECS {
            for prefix in spec.md_prefixes {
                if text.starts_with(*prefix) {
                    let n = prefix.chars().count();
                    doc.delete_text(c.block, 0, n);
                    doc.set_kind(c.block, spec.kind);
                    self.set_caret(Caret { block: c.block, index: c.index.saturating_sub(n) });
                    return;
                }
            }
        }

        // Code fence is special (like Divider): ```␣ → plain code, ```lang␣ → code with that
        // language. The trailing space is the trigger (so you can type the language between the
        // fence and the space); the whole `` ```lang `` prefix is consumed. An unknown language
        // is still stored (it just renders un-highlighted), so new grammars work retroactively.
        if let Some(after) = text.strip_prefix("```") {
            if let Some(lang) = after.strip_suffix(' ') {
                if !lang.contains(char::is_whitespace) {
                    doc.delete_text(c.block, 0, text.chars().count());
                    doc.set_kind(c.block, BlockKind::Code);
                    if !lang.is_empty() {
                        doc.set_lang(c.block, &lang.to_ascii_lowercase());
                    }
                    self.set_caret(Caret { block: c.block, index: 0 });
                    return;
                }
            }
        }

        // Divider is special: it clears the line and spawns a fresh paragraph below.
        if text == "---" || text.starts_with("--- ") {
            doc.delete_text(c.block, 0, doc.text_len(c.block));
            doc.set_kind(c.block, BlockKind::Divider);
            let ids = doc.block_ids();
            let idx = ids.iter().position(|&x| x == c.block).expect("caret block live");
            let para = doc.create_block(idx + 1, BlockKind::Paragraph, "");
            self.set_caret(Caret { block: para, index: 0 });
        }
    }

    /// If the just-typed char closed `**bold**`, `*italic*`/`_italic_`, `~~strike~~`, or
    /// `` `code` ``, strip both delimiters and mark the inner text. Double delimiters win over
    /// single (checked in `INLINE_RULES` order). Runs after each insertion.
    pub(super) fn apply_inline_markdown(&mut self, doc: &Doc) {
        let c = self.caret();
        if matches!(doc.kind(c.block), BlockKind::Code | BlockKind::Divider) {
            return; // code is literal; a divider has no text
        }
        let chars: Vec<char> = doc.text(c.block).chars().collect();
        let caret = c.index;
        for (delim, key) in INLINE_RULES {
            let d: Vec<char> = delim.chars().collect();
            let dl = d.len();
            // Need at least open + 1 char of content + close.
            if caret < 2 * dl + 1 || chars[caret - dl..caret] != d[..] {
                continue;
            }
            // The content's last char (just before the closing delim) must be real, not a
            // space or another delimiter char (avoids `a * b *` and `** **`).
            let last = chars[caret - dl - 1];
            if last.is_whitespace() || last == d[0] {
                continue;
            }
            // Walk left for the nearest opening delimiter whose content is well-formed.
            let dc = d[0];
            let mut found = None;
            let mut o = caret as isize - 2 * dl as isize - 1;
            while o >= 0 {
                let oi = o as usize;
                if chars[oi..oi + dl] == d[..] {
                    let after = chars[oi + dl]; // content start (always in range here)
                    let standalone = oi == 0 || chars[oi - 1] != dc;
                    if after != dc && !after.is_whitespace() && standalone {
                        found = Some(oi);
                        break;
                    }
                }
                o -= 1;
            }
            let Some(open) = found else { continue };
            // Strip the closing delimiter, then the opening (later index first so the earlier
            // stays valid). Content collapses left by `dl`; mark the result.
            doc.delete_text(c.block, caret - dl, dl);
            doc.delete_text(c.block, open, dl);
            let end = caret - 2 * dl;
            doc.mark(c.block, open, end, key);
            self.set_caret(Caret { block: c.block, index: end });
            self.desired_x = None;
            return;
        }
    }

    pub(super) fn handle_key(&mut self, doc: &Doc, key: Key, mods: Modifiers, placed: &[Placed], read_only: bool) -> bool {
        match key {
            // Mutations: suppressed in a reader (fall through to no-op).
            Key::Enter if !read_only => {
                self.enter(doc, mods.shift);
                true
            }
            Key::Backspace if !read_only => {
                self.backspace(doc);
                true
            }
            Key::Delete if !read_only => {
                self.delete_forward(doc);
                true
            }
            Key::Tab if !read_only => self.reindent(doc, !mods.shift),
            // Inline marks: toggle over the selection.
            Key::B if mods.command && !read_only => self.toggle_mark(doc, "bold"),
            Key::I if mods.command && !read_only => self.toggle_mark(doc, "italic"),
            Key::E if mods.command && !read_only => self.toggle_mark(doc, "code"),
            Key::S if mods.command && mods.shift && !read_only => self.toggle_mark(doc, "strike"),
            // Navigation / selection: always live (read-only safe).
            Key::ArrowLeft => {
                self.move_left(doc, mods.shift);
                false
            }
            Key::ArrowRight => {
                self.move_right(doc, mods.shift);
                false
            }
            Key::ArrowUp => {
                self.move_vertical(doc, placed, -1, mods.shift);
                false
            }
            Key::ArrowDown => {
                self.move_vertical(doc, placed, 1, mods.shift);
                false
            }
            Key::Home => {
                let c = self.caret();
                self.set_head(Caret { block: c.block, index: 0 }, mods.shift);
                self.desired_x = None;
                false
            }
            Key::End => {
                let c = self.caret();
                self.set_head(Caret { block: c.block, index: doc.text_len(c.block) }, mods.shift);
                self.desired_x = None;
                false
            }
            _ => false,
        }
    }

    /// Enter. In code: a newline (code is one multi-line block). Shift+Enter: soft line
    /// break anywhere. In an empty list item / quote: exit to a plain paragraph.
    /// Otherwise: split the block, continuing the list kind if it is one.
    fn enter(&mut self, doc: &Doc, shift: bool) {
        if !self.collapsed() {
            self.delete_selection(doc);
        }
        let c = self.caret();
        let kind = doc.kind(c.block);
        if shift || kind == BlockKind::Code {
            self.soft_break(doc);
            return;
        }
        if (kind.is_list() || kind == BlockKind::Quote) && doc.text_len(c.block) == 0 {
            // Empty nested item lifts one level; at the top level there's nowhere to lift,
            // so it falls back to a plain paragraph.
            if !doc.outdent(c.block) {
                doc.set_kind(c.block, BlockKind::Paragraph);
            }
            self.desired_x = None;
            return;
        }
        self.split(doc);
    }

    fn soft_break(&mut self, doc: &Doc) {
        if !self.collapsed() {
            self.delete_selection(doc);
        }
        let c = self.caret();
        doc.insert_text(c.block, c.index, "\n");
        self.set_caret(Caret { block: c.block, index: c.index + 1 });
        self.desired_x = None;
    }

    /// Split the block at the caret; the tail becomes a new block below — same kind and
    /// indent if it's a list item, else a paragraph.
    fn split(&mut self, doc: &Doc) {
        let c = self.caret();
        let kind = doc.kind(c.block);
        let total = doc.text_len(c.block);
        let tail: String = doc.text(c.block).chars().skip(c.index).collect();
        doc.delete_text(c.block, c.index, total - c.index);
        // The tail becomes a sibling right after this block (same parent), inheriting its depth.
        let new_kind = if kind.is_list() { kind } else { BlockKind::Paragraph };
        let new = doc.insert_after(c.block, new_kind, &tail);
        self.set_caret(Caret { block: new, index: 0 });
        self.desired_x = None;
    }

    /// Backspace: delete the char before the caret. At the start of a block: outdent a
    /// nested list item, else demote a typed block to a paragraph, else merge upward.
    fn backspace(&mut self, doc: &Doc) {
        if !self.collapsed() {
            self.delete_selection(doc);
            return;
        }
        let c = self.caret();
        if c.index > 0 {
            self.in_block(doc, |tf, buf| tf.backspace(buf));
        } else {
            let kind = doc.kind(c.block);
            if doc.outdent(c.block) {
                // A nested block un-nests one level first (whatever its kind).
            } else if kind != BlockKind::Paragraph {
                doc.set_kind(c.block, BlockKind::Paragraph);
            } else {
                let ids = doc.block_ids();
                let idx = ids.iter().position(|&x| x == c.block).expect("caret block live");
                if idx > 0 {
                    let prev = ids[idx - 1];
                    let prev_len = doc.text_len(prev);
                    let cur_text = doc.text(c.block);
                    if !cur_text.is_empty() {
                        doc.insert_text(prev, prev_len, &cur_text);
                    }
                    doc.delete_block(c.block);
                    self.set_caret(Caret { block: prev, index: prev_len });
                }
            }
        }
        self.desired_x = None;
    }

    /// Delete: remove the char after the caret; at the end of a block, pull the next block
    /// up into this one.
    fn delete_forward(&mut self, doc: &Doc) {
        if !self.collapsed() {
            self.delete_selection(doc);
            return;
        }
        let c = self.caret();
        let len = doc.text_len(c.block);
        if c.index < len {
            self.in_block(doc, |tf, buf| tf.delete_forward(buf));
        } else {
            let ids = doc.block_ids();
            let idx = ids.iter().position(|&x| x == c.block).expect("caret block live");
            if idx + 1 < ids.len() {
                let next = ids[idx + 1];
                let next_text = doc.text(next);
                if !next_text.is_empty() {
                    doc.insert_text(c.block, len, &next_text);
                }
                doc.delete_block(next);
            }
        }
        self.desired_x = None;
    }

    /// Tab / Shift-Tab on a list item: nest it under the previous sibling / lift it a level.
    /// No-op on non-list kinds, or when there's nowhere to go (returns whether it moved).
    fn reindent(&mut self, doc: &Doc, deeper: bool) -> bool {
        let c = self.caret();
        if !doc.kind(c.block).is_list() {
            return false;
        }
        if deeper {
            doc.indent(c.block)
        } else {
            doc.outdent(c.block)
        }
    }

    /// Left by one position. Plain: collapse a selection to its left edge, else step the
    /// caret left (across blocks). With `extend`: move the head left, growing the selection.
    fn move_left(&mut self, doc: &Doc, extend: bool) {
        if !extend && !self.collapsed() {
            let (start, _) = self.ordered(&doc.block_ids());
            self.set_caret(start);
            self.desired_x = None;
            return;
        }
        let c = self.caret();
        let new = if c.index > 0 {
            Caret { block: c.block, index: c.index - 1 }
        } else {
            let ids = doc.block_ids();
            let idx = ids.iter().position(|&x| x == c.block).expect("caret block live");
            if idx > 0 {
                Caret { block: ids[idx - 1], index: doc.text_len(ids[idx - 1]) }
            } else {
                c
            }
        };
        self.set_head(new, extend);
        self.desired_x = None;
    }

    /// Right by one position; mirror of [`move_left`] (plain collapses to the right edge).
    fn move_right(&mut self, doc: &Doc, extend: bool) {
        if !extend && !self.collapsed() {
            let (_, end) = self.ordered(&doc.block_ids());
            self.set_caret(end);
            self.desired_x = None;
            return;
        }
        let c = self.caret();
        let len = doc.text_len(c.block);
        let new = if c.index < len {
            Caret { block: c.block, index: c.index + 1 }
        } else {
            let ids = doc.block_ids();
            let idx = ids.iter().position(|&x| x == c.block).expect("caret block live");
            if idx + 1 < ids.len() {
                Caret { block: ids[idx + 1], index: 0 }
            } else {
                c
            }
        };
        self.set_head(new, extend);
        self.desired_x = None;
    }

    /// Up/Down by one visual line, crossing into the adjacent block at the first/last row.
    /// Driven by galley geometry + a sticky x, so it works inside wrapped paragraphs too.
    fn move_vertical(&mut self, doc: &Doc, placed: &[Placed], dir: i32, extend: bool) {
        let c = self.caret();
        let ids = doc.block_ids();
        let Some(bi) = ids.iter().position(|&x| x == c.block) else { return };
        if bi >= placed.len() {
            return;
        }
        let p = &placed[bi];
        let cr = p.galley.pos_from_cursor(CCursor::new(c.index));
        let local_x = self.desired_x.unwrap_or_else(|| cr.center().x);
        self.desired_x = Some(local_x);
        let line_h = cr.height().max(1.0);

        let new = if dir < 0 {
            let y = cr.center().y - line_h;
            if y >= 0.0 {
                Some(Caret { block: c.block, index: p.galley.cursor_from_pos(Vec2::new(local_x, y)).index })
            } else if bi > 0 {
                let prev = &placed[bi - 1];
                let bottom = (prev.galley.size().y - 1.0).max(0.0);
                Some(Caret { block: ids[bi - 1], index: prev.galley.cursor_from_pos(Vec2::new(local_x, bottom)).index })
            } else {
                None
            }
        } else {
            let y = cr.center().y + line_h;
            if y <= p.galley.size().y {
                Some(Caret { block: c.block, index: p.galley.cursor_from_pos(Vec2::new(local_x, y)).index })
            } else if bi + 1 < placed.len() && bi + 1 < ids.len() {
                Some(Caret { block: ids[bi + 1], index: placed[bi + 1].galley.cursor_from_pos(Vec2::new(local_x, 1.0)).index })
            } else {
                None
            }
        };
        if let Some(new) = new {
            self.set_head(new, extend);
        }
    }
}
