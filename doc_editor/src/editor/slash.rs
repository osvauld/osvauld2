//! The slash command palette controller: open on `/`, filter, navigate, apply the chosen kind,
//! paint on a foreground layer. Palette state lives off the document; drives `overlays`.

use egui::{Event, Key, Modifiers, Rect, Ui};
use loro::TreeID;

use crate::model::{BlockKind, Doc};
use crate::overlays;

use super::layout;
use super::{Caret, DocEditor, Placed, Slash};

impl DocEditor {
    /// Open the palette if `t` is `/` typed at offset 0 of an empty (non-divider) block.
    /// The `/` is consumed — never stored as text. Returns whether it was consumed.
    pub(super) fn try_open_slash(&mut self, doc: &Doc, t: &str) -> bool {
        if t != "/" {
            return false;
        }
        let c = self.caret();
        if c.index == 0 && doc.text_len(c.block) == 0 && doc.kind(c.block) != BlockKind::Divider {
            self.slash = Some(Slash { block: c.block, query: String::new(), selected: 0 });
            true
        } else {
            false
        }
    }

    /// Typing while the palette is open filters it (the query never enters the document).
    fn slash_text(&mut self, t: &str) {
        let Some(s) = self.slash.as_mut() else { return };
        for ch in t.chars().filter(|c| !c.is_control() && *c != '/') {
            s.query.push(ch);
        }
        s.selected = 0;
    }

    /// Keyboard while the palette is open. Returns whether the document changed (an apply).
    fn slash_key(&mut self, doc: &Doc, key: Key) -> bool {
        match key {
            Key::Escape => {
                self.slash = None;
                false
            }
            Key::Enter | Key::Tab => self.apply_slash(doc),
            Key::ArrowDown => {
                let count = overlays::filtered(&self.slash.as_ref().unwrap().query).len();
                if count > 0 {
                    let s = self.slash.as_mut().unwrap();
                    s.selected = (s.selected + 1).min(count - 1);
                }
                false
            }
            Key::ArrowUp => {
                let s = self.slash.as_mut().unwrap();
                s.selected = s.selected.saturating_sub(1);
                false
            }
            Key::Backspace => {
                let s = self.slash.as_mut().unwrap();
                if s.query.is_empty() {
                    self.slash = None;
                } else {
                    s.query.pop();
                    s.selected = 0;
                }
                false
            }
            _ => false,
        }
    }

    /// Apply the highlighted item, then close the palette.
    fn apply_slash(&mut self, doc: &Doc) -> bool {
        let Some(s) = self.slash.as_ref() else { return false };
        let items = overlays::filtered(&s.query);
        if items.is_empty() {
            self.slash = None;
            return false;
        }
        let kind = items[s.selected.min(items.len() - 1)].kind;
        let block = s.block;
        self.slash = None;
        self.apply_slash_kind(doc, block, kind);
        true
    }

    /// Turn `block` into `kind`. Divider spawns a fresh paragraph below for the caret to land
    /// in; every other kind keeps the caret in the now-typed empty block.
    fn apply_slash_kind(&mut self, doc: &Doc, block: TreeID, kind: BlockKind) {
        if kind == BlockKind::Divider {
            doc.set_kind(block, BlockKind::Divider);
            let para = doc.insert_after(block, BlockKind::Paragraph, "");
            self.set_caret(Caret { block: para, index: 0 });
        } else {
            doc.set_kind(block, kind);
            self.set_caret(Caret { block, index: 0 });
        }
        self.desired_x = None;
    }

    /// The palette's screen rect this frame, or `None` when it's closed / its block is gone.
    /// `rect` anchors the caret (content origin); `viewport` is the cell it clamps/flips in.
    pub(super) fn slash_menu_rect(&self, rect: Rect, viewport: Rect, placed: &[Placed]) -> Option<Rect> {
        let s = self.slash.as_ref()?;
        let p = placed.iter().find(|p| p.id == s.block)?;
        let items = overlays::filtered(&s.query);
        Some(overlays::menu_rect(&items, viewport, layout::caret_screen_rect(rect, p)))
    }

    /// All keyboard while the palette is open (typing filters, arrows move, Enter/Tab apply,
    /// Esc/empty-Backspace close). Nothing reaches the editing path.
    pub(super) fn slash_keyboard(&mut self, ui: &Ui, doc: &Doc) -> bool {
        let events = ui.input(|i| i.events.clone());
        // Keep Tab out of egui's focus navigation while the palette is open.
        ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Tab);
            i.consume_key(Modifiers::SHIFT, Key::Tab);
        });
        let mut changed = false;
        for ev in &events {
            match ev {
                Event::Text(t) => self.slash_text(t),
                Event::Key { key, pressed: true, .. } => {
                    if self.slash_key(doc, *key) {
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }

    /// Pointer over the palette: hover moves the highlight (only while the pointer is
    /// actually moving, so it doesn't fight arrow keys); a click applies the item.
    pub(super) fn slash_pointer(&mut self, ui: &Ui, doc: &Doc, rect: Rect, viewport: Rect, placed: &[Placed]) -> bool {
        let Some(s) = self.slash.as_ref() else { return false };
        let block = s.block;
        let Some(p) = placed.iter().find(|p| p.id == block) else {
            self.slash = None;
            return false;
        };
        let items = overlays::filtered(&s.query);
        let menu = overlays::menu_rect(&items, viewport, layout::caret_screen_rect(rect, p));
        let rects = overlays::item_rects(menu, &items);

        let (hover_pos, moving, clicked, click_pos) = ui.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.velocity().length() > 0.0,
                i.pointer.primary_clicked(),
                i.pointer.interact_pos(),
            )
        });
        if moving {
            if let Some(idx) = hover_pos.and_then(|hp| rects.iter().position(|r| r.contains(hp))) {
                if let Some(s) = self.slash.as_mut() {
                    s.selected = idx;
                }
            }
        }
        if clicked {
            if let Some(idx) = click_pos.and_then(|cp| rects.iter().position(|r| r.contains(cp))) {
                let kind = items[idx].kind;
                self.slash = None;
                self.apply_slash_kind(doc, block, kind);
                return true;
            }
        }
        false
    }

    /// Paint the palette on a foreground layer (above the text), clipped to the viewport.
    pub(super) fn paint_slash(&self, ui: &Ui, rect: Rect, viewport: Rect, placed: &[Placed]) {
        let Some(s) = self.slash.as_ref() else { return };
        let Some(p) = placed.iter().find(|p| p.id == s.block) else { return };
        let items = overlays::filtered(&s.query);
        let menu = overlays::menu_rect(&items, viewport, layout::caret_screen_rect(rect, p));
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("slash_overlay"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(viewport);
        let highlight = s.selected.min(items.len().saturating_sub(1));
        overlays::render(&painter, menu, &items, highlight, &s.query);
    }
}
