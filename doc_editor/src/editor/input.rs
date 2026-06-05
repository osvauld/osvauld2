//! **Input / editing**: text insertion, the markdown input rules, the keyboard map, and the
//! block-editing operations (split / merge / indent) plus caret movement. Every method here
//! turns a raw key/text event into Loro ops on the document and moves the caret.

use egui::{text::CCursor, Key, Modifiers, Vec2};

use crate::block;
use crate::model::{BlockKind, Doc};

use super::{Caret, DocEditor, Placed};

impl DocEditor {
    pub(super) fn insert_text(&mut self, doc: &Doc, t: &str) -> bool {
        let s: String = t.chars().filter(|c| !c.is_control()).collect();
        if s.is_empty() {
            return false;
        }
        if !self.collapsed() {
            self.delete_selection(doc);
        }
        let c = self.caret();
        doc.insert_text(c.block, c.index, &s);
        self.set_caret(Caret { block: c.block, index: c.index + s.chars().count() });
        self.desired_x = None;
        true
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

        // Simple conversions, sourced from each kind's `md_prefixes` in `block::SPECS`: strip
        // the prefix and set the kind, keeping the caret. (Prefixes are mutually exclusive —
        // each carries its exact `#`-count / trailing space — so table order is irrelevant.)
        // `Code`'s ``` folds in cleanly: stripping its 3 chars and setting Code is identical
        // to the old special case.
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

    pub(super) fn handle_key(&mut self, doc: &Doc, key: Key, mods: Modifiers, placed: &[Placed], read_only: bool) -> bool {
        match key {
            // --- Mutations: suppressed in a reader (fall through to no-op) ---
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
            // --- Inline marks: toggle over the selection (Cmd/Ctrl + key) ---
            Key::B if mods.command && !read_only => self.toggle_mark(doc, "bold"),
            Key::I if mods.command && !read_only => self.toggle_mark(doc, "italic"),
            Key::E if mods.command && !read_only => self.toggle_mark(doc, "code"),
            Key::S if mods.command && mods.shift && !read_only => self.toggle_mark(doc, "strike"),
            // --- Navigation / selection: always live (read-only safe) ---
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
            // Enter on an empty *nested* item lifts it one level (staying the same kind); at
            // the top level there's nowhere to lift, so it falls back to a plain paragraph.
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
        // The tail rides into a sibling right after this block — same parent, so it inherits
        // the nesting depth automatically. A list continues its kind; anything else splits
        // into a paragraph.
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
            doc.delete_text(c.block, c.index - 1, 1);
            self.set_caret(Caret { block: c.block, index: c.index - 1 });
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
            doc.delete_text(c.block, c.index, 1);
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
    /// Driven by galley geometry + a sticky x, so it works inside wrapped paragraphs and
    /// across blocks the same way. `extend` grows the selection instead of collapsing.
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
