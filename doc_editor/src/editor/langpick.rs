//! The code-block **language dropdown** controller: open it from a code block's language tag,
//! pick a language (or "Plain text"), paint it on a foreground layer. State ([`LangPick`]) lives
//! off the document; the rows + menu geometry come from [`crate::overlays`]. Mirrors the slash
//! palette's no-focus-steal pattern — paint-only overlay, the editor hit-tests the rects itself.

use egui::{Rect, Ui};

use crate::model::Doc;
use crate::overlays;

use super::layout;
use super::{DocEditor, Placed};

impl DocEditor {
    /// The dropdown's screen rect this frame, or `None` when it's closed / its block is gone.
    /// The row count is fixed (Plain + the bundled languages), so this needs no `doc`.
    pub(super) fn lang_menu_rect(&self, rect: Rect, viewport: Rect, placed: &[Placed]) -> Option<Rect> {
        let lp = self.lang_pick.as_ref()?;
        let p = placed.iter().find(|p| p.id == lp.block)?;
        let n = overlays::lang_rows(None).len();
        Some(overlays::lang_menu_rect(viewport, layout::lang_tag_rect(p, rect), n))
    }

    /// Pointer over the dropdown: hover highlights a row (only while moving, so it doesn't fight
    /// arrow keys), a click applies it and closes. Returns whether the document changed.
    pub(super) fn lang_pointer(&mut self, ui: &Ui, doc: &Doc, rect: Rect, viewport: Rect, placed: &[Placed]) -> bool {
        let Some(lp) = self.lang_pick.as_ref() else { return false };
        let block = lp.block;
        let Some(p) = placed.iter().find(|p| p.id == block) else {
            self.lang_pick = None;
            return false;
        };
        let rows = overlays::lang_rows(doc.lang(block).as_deref());
        let menu = overlays::lang_menu_rect(viewport, layout::lang_tag_rect(p, rect), rows.len());
        let rects = overlays::lang_item_rects(menu, rows.len());

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
                if let Some(lp) = self.lang_pick.as_mut() {
                    lp.selected = idx;
                }
            }
        }
        if clicked {
            if let Some(idx) = click_pos.and_then(|cp| rects.iter().position(|r| r.contains(cp))) {
                let token = rows[idx].token;
                self.lang_pick = None;
                doc.set_lang(block, token); // `"text"` (Plain) round-trips to no highlighting
                return true;
            }
        }
        false
    }

    /// Keyboard while the dropdown is open: ↑/↓ move the highlight, Enter applies, Esc closes.
    /// Returns whether the document changed (an apply).
    pub(super) fn lang_key(&mut self, doc: &Doc, key: egui::Key) -> bool {
        let Some(lp) = self.lang_pick.as_ref() else { return false };
        let block = lp.block;
        let rows = overlays::lang_rows(doc.lang(block).as_deref());
        match key {
            egui::Key::Escape => {
                self.lang_pick = None;
                false
            }
            egui::Key::ArrowDown => {
                if let Some(lp) = self.lang_pick.as_mut() {
                    lp.selected = (lp.selected + 1).min(rows.len().saturating_sub(1));
                }
                false
            }
            egui::Key::ArrowUp => {
                if let Some(lp) = self.lang_pick.as_mut() {
                    lp.selected = lp.selected.saturating_sub(1);
                }
                false
            }
            egui::Key::Enter | egui::Key::Tab => {
                let idx = lp.selected.min(rows.len().saturating_sub(1));
                let token = rows[idx].token;
                self.lang_pick = None;
                doc.set_lang(block, token);
                true
            }
            _ => false,
        }
    }

    /// All keyboard while the dropdown is open (arrows move, Enter/Tab apply, Esc closes).
    /// Nothing reaches the editing path.
    pub(super) fn lang_keyboard(&mut self, ui: &Ui, doc: &Doc) -> bool {
        let events = ui.input(|i| i.events.clone());
        // Keep Tab out of egui's focus navigation while the dropdown is open.
        ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
        });
        let mut changed = false;
        for ev in &events {
            if let egui::Event::Key { key, pressed: true, .. } = ev {
                if self.lang_key(doc, *key) {
                    changed = true;
                }
            }
        }
        changed
    }

    /// Paint the dropdown on a foreground layer (above the text), clipped to the viewport.
    pub(super) fn paint_lang(&self, ui: &Ui, doc: &Doc, rect: Rect, viewport: Rect, placed: &[Placed]) {
        let Some(lp) = self.lang_pick.as_ref() else { return };
        let Some(p) = placed.iter().find(|p| p.id == lp.block) else { return };
        let rows = overlays::lang_rows(doc.lang(lp.block).as_deref());
        let menu = overlays::lang_menu_rect(viewport, layout::lang_tag_rect(p, rect), rows.len());
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("lang_overlay"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(viewport);
        let sel = lp.selected.min(rows.len().saturating_sub(1));
        overlays::render_lang(&painter, menu, &rows, sel);
    }
}
