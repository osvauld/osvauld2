//! The inline format toolbar: a floating row of mark toggles (B / I / S / `</>`) above a
//! non-empty selection. Painter-only + self-hit-tested on a foreground layer (like the slash
//! palette), so it never steals keyboard focus. Clicking a button toggles its mark.

use egui::{
    pos2, text::CCursor, vec2, Align2, CornerRadius, FontFamily, FontId, Rect, Stroke, StrokeKind, Ui,
};
use loro::TreeID;

use crate::model::Doc;
use crate::theme;

use super::{DocEditor, Placed};

/// The toggles, in order: (mark key, button label). Link / comment / math are deferred.
const BUTTONS: &[(&str, &str)] = &[("bold", "B"), ("italic", "I"), ("strike", "S"), ("code", "</>")];

const BTN_W: f32 = 30.0;
const BTN_H: f32 = 28.0;
const PAD: f32 = 4.0;

impl DocEditor {
    /// The toolbar's screen rect this frame, or `None` when the selection is collapsed (no
    /// range to format). Anchored just above the selection's start, clamped into the cell;
    /// flips below the line when it would clip the cell top.
    pub(super) fn toolbar_rect(&self, rect: Rect, viewport: Rect, placed: &[Placed]) -> Option<Rect> {
        if self.collapsed() {
            return None;
        }
        let ids: Vec<TreeID> = placed.iter().map(|p| p.id).collect();
        let (start, _end) = self.ordered(&ids);
        let p = placed.iter().find(|p| p.id == start.block)?;
        let cr = p.galley.pos_from_cursor(CCursor::new(start.index));
        let anchor_x = rect.left() + p.text_x + cr.center().x;
        let line_top = rect.top() + p.content_top + cr.top();
        let line_bottom = rect.top() + p.content_top + cr.bottom();

        let w = BUTTONS.len() as f32 * BTN_W + 2.0 * PAD;
        let h = BTN_H + 2.0 * PAD;
        let mut x = anchor_x - w * 0.5;
        let mut y = line_top - h - 8.0;
        if y < viewport.top() + 4.0 {
            y = line_bottom + 8.0; // not enough room above — drop below the line
        }
        x = x.clamp(viewport.left() + 4.0, (viewport.right() - w - 4.0).max(viewport.left() + 4.0));
        Some(Rect::from_min_size(pos2(x, y), vec2(w, h)))
    }

    /// A primary click on a button toggles its mark over the selection. Returns whether the
    /// document changed. `toolbar` is the precomputed rect (the caller already gated on focus).
    pub(super) fn toolbar_pointer(&mut self, ui: &Ui, doc: &Doc, toolbar: Rect) -> bool {
        let rects = button_rects(toolbar);
        let (clicked, pos) = ui.input(|i| (i.pointer.primary_clicked(), i.pointer.interact_pos()));
        if clicked {
            if let Some(idx) = pos.and_then(|p| rects.iter().position(|r| r.contains(p))) {
                return self.toggle_mark(doc, BUTTONS[idx].0);
            }
        }
        false
    }

    /// Paint the toolbar on a foreground layer; a button is highlighted when its mark already
    /// covers the whole selection.
    pub(super) fn paint_toolbar(&self, ui: &Ui, doc: &Doc, toolbar: Rect, viewport: Rect) {
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("inline_toolbar"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(viewport);
        painter.rect_filled(toolbar, CornerRadius::same(0), theme::BG_3);
        painter.rect_stroke(toolbar, CornerRadius::same(0), Stroke::new(1.0, theme::BD), StrokeKind::Inside);

        let bold_fam = rich_text::bold_or_fallback(ui.ctx());
        for (i, r) in button_rects(toolbar).into_iter().enumerate() {
            let (key, label) = BUTTONS[i];
            let active = self.selection_has_mark(doc, key);
            if active {
                painter.rect_filled(r, CornerRadius::same(0), theme::ACCENT_BG);
            }
            let color = if active { theme::ACCENT_HI } else { theme::FG_2 };
            let font = match key {
                "bold" => FontId::new(14.0, bold_fam.clone()),
                "code" => FontId::new(11.0, FontFamily::Monospace),
                _ => FontId::new(14.0, FontFamily::Proportional),
            };
            let center = r.center();
            painter.text(center, Align2::CENTER_CENTER, label, font, color);
            // Preview the effect where the glyph alone doesn't: a line through the "S".
            if key == "strike" {
                painter.hline((center.x - 5.0)..=(center.x + 5.0), center.y, Stroke::new(1.2, color));
            }
        }
    }
}

/// The button rects laid left-to-right inside `toolbar`.
fn button_rects(toolbar: Rect) -> Vec<Rect> {
    (0..BUTTONS.len())
        .map(|i| {
            Rect::from_min_size(
                pos2(toolbar.left() + PAD + i as f32 * BTN_W, toolbar.top() + PAD),
                vec2(BTN_W, BTN_H),
            )
        })
        .collect()
}
