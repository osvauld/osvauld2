//! Per-frame chrome painting — the screen-only affordances around the content (selection
//! highlight, hover tint, indent guides, spine, focus type-tag, gutter, caret + placeholder).
//! Block content is composed into a backend-neutral [`Scene`] that this file and the PDF export
//! both render, so paper and glass can't drift; chrome stays out of the scene as it's edit-only.

use std::time::Duration;

use egui::{
    pos2, text::CCursor, Align2, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response,
    Stroke, Ui,
};

use crate::model::{BlockKind, Doc};
use crate::{block, theme};

use super::layout;
use super::{DocEditor, Placed};

impl DocEditor {
    pub(super) fn paint(&self, ui: &Ui, response: &Response, rect: Rect, doc: &Doc, placed: &[Placed], hovered: Option<usize>) {
        let painter = ui.painter_at(rect);
        let focused = response.has_focus();
        let narrow = rect.width() < 480.0;
        let caret_idx = placed.iter().position(|p| p.id == self.caret().block);
        let hovered = hovered.filter(|&h| h < placed.len());

        // Selection highlight first, behind everything, so the text paints on top of it.
        self.paint_selection(&painter, rect, doc, placed);

        // Per-block chrome — everything that sits behind the content.
        for (i, p) in placed.iter().enumerate() {
            let is_caret = caret_idx == Some(i);
            let gutter_visible = hovered == Some(i) || (focused && is_caret);
            let is_focus_block = focused && is_caret;
            self.paint_block_chrome(&painter, rect, p, gutter_visible, is_focus_block, hovered == Some(i), narrow);
        }

        // The content as one display list, composed every frame so screen and PDF can't drift.
        let scene = super::compose::build(doc, placed);
        scene.paint(&painter, rect.min.to_vec2());

        // Code-block language-tag dropdown affordance (screen-only chrome, so it's not in the
        // PDF): a ▾ chevron just left of the tag text the scene drew, brightening on hover or
        // while the dropdown is open. Painted after the scene so it sits cleanly on the code box.
        let pointer = ui.input(|i| i.pointer.hover_pos());
        for p in placed.iter().filter(|p| p.kind == BlockKind::Code) {
            let open = self.lang_pick.as_ref().is_some_and(|l| l.block == p.id);
            let hot = pointer.is_some_and(|hp| layout::lang_tag_rect(p, rect).contains(hp));
            paint_lang_chevron(&painter, rect, doc, p, hot || open);
        }

        // Caret + placeholder sit on top of the content.
        if focused {
            if let Some(i) = caret_idx {
                self.paint_caret(ui, &painter, rect, doc, &placed[i]);
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    /// The screen-only affordances behind a block's content.
    #[allow(clippy::too_many_arguments)]
    fn paint_block_chrome(
        &self,
        painter: &Painter,
        rect: Rect,
        p: &Placed,
        gutter_visible: bool,
        is_focus_block: bool,
        is_hover: bool,
        narrow: bool,
    ) {
        let ox = rect.left();
        let oy = rect.top();
        let abs = |x: f32, y: f32| Pos2::new(ox + x, oy + y);

        // Hover tint spanning the pinned gutter to the text right edge, so the row reads as one
        // unit with its affordances. (Resting / focused show none.)
        if is_hover && !is_focus_block {
            let row = Rect::from_min_max(abs(p.gutter_left, p.row_top), abs(p.content_right, p.row_bottom));
            painter.rect_filled(row, CornerRadius::same(0), theme::HOVER_BG);
        }

        // Indent guides — one hairline per nesting level in the column an ancestor's spine
        // would occupy; the deepest (the caret's own branch) is tinted accent.
        for level in 0..p.depth {
            let gx = ox + theme::OUTER_LEFT + theme::GUTTER + level as f32 * theme::INDENT;
            let color = if level == p.depth - 1 { theme::GUIDE_ACTIVE } else { theme::HAIR };
            painter.vline(gx, (oy + p.row_top)..=(oy + p.row_bottom), Stroke::new(1.0, color));
        }

        // The persistent spine at the text-column left — indents with depth; darker on focus.
        let spine_x = ox + p.spine_x;
        let spine_color = if is_focus_block { theme::BD } else { theme::HAIR };
        painter.vline(spine_x, (oy + p.row_top)..=(oy + p.row_bottom), Stroke::new(theme::SPINE, spine_color));

        // Focus type-tag in the outer margin, beside the pinned gutter (hidden on narrow tiles).
        if is_focus_block && !narrow {
            painter.text(
                abs(p.gutter_left - 6.0, p.content_top),
                Align2::RIGHT_TOP,
                block::type_tag(p.kind),
                FontId::new(9.5, FontFamily::Monospace),
                theme::FAINT,
            );
        }

        if gutter_visible {
            paint_gutter(painter, rect, p);
        }
    }

    fn paint_caret(&self, ui: &Ui, painter: &Painter, rect: Rect, doc: &Doc, p: &Placed) {
        let caret = self.caret();
        if p.kind == BlockKind::Divider {
            return;
        }
        let origin = Pos2::new(rect.left() + p.text_x, rect.top() + p.content_top);

        // Placeholder inside an empty, focused block — hidden while the palette covers it.
        let palette_here = self.slash.as_ref().is_some_and(|s| s.block == caret.block);
        if doc.text_len(caret.block) == 0 && !palette_here {
            let wrap = (rect.left() + p.content_right - origin.x).max(80.0);
            let mut faint = block::block_style(p.kind);
            faint.color = theme::FAINT;
            faint.italics = false;
            let ph = layout::plain_galley(ui, block::placeholder(p.kind), wrap, &faint);
            painter.galley(origin, ph, theme::FAINT);
        }

        // Blink is measured from `blink_origin` (last edit/caret-move), so the caret is solid
        // the instant you type and only blinks once you pause.
        let solid = ((ui.input(|i| i.time) - self.blink_origin) * 1.4).fract() < 0.6;
        if solid {
            let cr = p.galley.pos_from_cursor(CCursor::new(caret.index));
            // A text-height bar centred on the glyph (egui centres glyphs in the line box, so
            // `cr.center().y` is the glyph centre; full line-box height looked oversized).
            let half = block::block_style(p.kind).font.size * 0.62;
            let x = origin.x + cr.center().x;
            let cy = origin.y + cr.center().y;
            painter.vline(x, (cy - half)..=(cy + half), Stroke::new(2.0, theme::ACCENT));
        }
    }
}

/// The code-block language dropdown chevron — a glyph-free `▾` just left of the tag text. The
/// tag text (e.g. `RUST`) is drawn by the scene; this only adds the dropdown hint. Width of the
/// tag is approximated (mono ~5.7px/char) to place the chevron — exactness isn't critical.
fn paint_lang_chevron(painter: &Painter, rect: Rect, doc: &Doc, p: &Placed, active: bool) {
    let n = doc.lang(p.id).map_or(4, |l| l.chars().count().max(1)); // "text" / unset → "TEXT"
    let right = rect.left() + p.content_right - 7.0;
    let cx = right - n as f32 * 5.7 - 7.0;
    let cy = rect.top() + p.row_top + 12.5;
    let col = if active { theme::FG_2 } else { theme::MUTED };
    let (w, h) = (3.0, 1.9);
    painter.line_segment([pos2(cx - w, cy - h), pos2(cx, cy + h)], Stroke::new(1.2, col));
    painter.line_segment([pos2(cx + w, cy - h), pos2(cx, cy + h)], Stroke::new(1.2, col));
}

/// The gutter `+` (insert) and `⋮⋮` (grip) drawn as primitives (no glyph dependency).
fn paint_gutter(painter: &Painter, rect: Rect, p: &Placed) {
    let plus = layout::plus_rect(p, rect).center();
    painter.line_segment([pos2(plus.x - 5.0, plus.y), pos2(plus.x + 5.0, plus.y)], Stroke::new(1.5, theme::FAINT));
    painter.line_segment([pos2(plus.x, plus.y - 5.0), pos2(plus.x, plus.y + 5.0)], Stroke::new(1.5, theme::FAINT));

    let grip = layout::grip_rect(p, rect).center();
    for &dy in &[-5.0_f32, 0.0, 5.0] {
        for &dx in &[-2.5_f32, 2.5] {
            painter.circle_filled(pos2(grip.x + dx, grip.y + dy), 1.1, theme::FAINT);
        }
    }
}
