//! Per-frame **painting**: the block rows (hover tint, indent guides, spine, focus type-tag,
//! lead markers, text body) and the caret + placeholder, plus the gutter / checkbox glyph
//! primitives. Painter-safe: solid fills, 1px hairlines, square corners.

use std::time::Duration;

use egui::{
    pos2, text::CCursor, Align2, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response,
    Shape, Stroke, StrokeKind, Ui,
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

        // Selection highlight first, so the text paints on top of it.
        self.paint_selection(&painter, rect, doc, placed);

        for (i, p) in placed.iter().enumerate() {
            let is_caret = caret_idx == Some(i);
            let gutter_visible = hovered == Some(i) || (focused && is_caret);
            let is_focus_block = focused && is_caret;
            self.paint_block(&painter, rect, doc, p, gutter_visible, is_focus_block, hovered == Some(i), narrow);
        }

        if focused {
            if let Some(i) = caret_idx {
                self.paint_caret(ui, &painter, rect, doc, &placed[i]);
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_block(
        &self,
        painter: &Painter,
        rect: Rect,
        doc: &Doc,
        p: &Placed,
        gutter_visible: bool,
        is_focus_block: bool,
        is_hover: bool,
        narrow: bool,
    ) {
        let ox = rect.left();
        let oy = rect.top();
        let abs = |x: f32, y: f32| Pos2::new(ox + x, oy + y);
        let st = block::block_style(p.kind);

        // Hover tint over the row — from the pinned gutter to the text right edge, so the
        // hovered row reads as one unit with its affordances. (Resting / focused show none.)
        if is_hover && !is_focus_block {
            let row = Rect::from_min_max(abs(p.gutter_left, p.row_top), abs(p.content_right, p.row_bottom));
            painter.rect_filled(row, CornerRadius::same(0), theme::HOVER_BG);
        }

        // Indent guides — one hairline per nesting level, sitting in the column an ancestor's
        // spine would occupy (so a guide reads as that ancestor's spine continued down). The
        // deepest (the caret's own branch) is tinted accent.
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

        // Lead markers, block decorations, and the text body.
        match p.kind {
            BlockKind::Divider => {
                let y = oy + (p.row_top + p.row_bottom) * 0.5;
                painter.hline((ox + p.content_x)..=(ox + p.content_right), y, Stroke::new(1.0, theme::BD));
            }
            BlockKind::Code => {
                let box_rect = Rect::from_min_max(
                    abs(p.content_x, p.row_top + 4.0),
                    abs(p.content_right, p.row_bottom - 4.0),
                );
                painter.rect_filled(box_rect, CornerRadius::same(0), theme::CODE_BG);
                painter.rect_stroke(box_rect, CornerRadius::same(0), Stroke::new(1.0, theme::HAIR), StrokeKind::Inside);
                let lang = doc.lang(p.id).unwrap_or_else(|| "text".into()).to_uppercase();
                painter.text(
                    pos2(box_rect.right() - 7.0, box_rect.top() + 4.0),
                    Align2::RIGHT_TOP,
                    lang,
                    FontId::new(9.5, FontFamily::Monospace),
                    theme::MUTED,
                );
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_2);
            }
            BlockKind::Quote => {
                let h = p.galley.size().y;
                painter.vline(
                    ox + p.content_x,
                    (oy + p.content_top)..=(oy + p.content_top + h),
                    Stroke::new(2.0, theme::ACCENT),
                );
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_2);
            }
            BlockKind::BulletList => {
                let (glyph, size) = match p.depth % 3 {
                    0 => ("•", 16.0),
                    1 => ("◦", 13.0),
                    _ => ("▪", 9.0),
                };
                painter.text(
                    abs(p.content_x + 7.0, p.content_top + st.line_height * 0.5),
                    Align2::CENTER_CENTER,
                    glyph,
                    FontId::new(size, FontFamily::Proportional),
                    theme::FG_2,
                );
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_1);
            }
            BlockKind::NumberedList => {
                painter.text(
                    abs(p.content_x, p.content_top),
                    Align2::LEFT_TOP,
                    format!("{}.", p.ordinal.unwrap_or(1)),
                    FontId::new(16.0, FontFamily::Proportional),
                    theme::FG_2,
                );
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_1);
            }
            BlockKind::Todo => {
                paint_checkbox(painter, layout::checkbox_rect(p, rect), p.done);
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_1);
            }
            _ => {
                painter.galley(abs(p.text_x, p.content_top), p.galley.clone(), theme::FG_1);
            }
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
            let ph = layout::layout_run(ui, block::placeholder(p.kind), wrap, &faint, false);
            painter.galley(origin, ph, theme::FAINT);
        }

        // Blink is measured from `blink_origin` (set on the last edit/caret-move), so the
        // caret is solid the instant you type and only blinks once you pause.
        let solid = ((ui.input(|i| i.time) - self.blink_origin) * 1.4).fract() < 0.6;
        if solid {
            let cr = p.galley.pos_from_cursor(CCursor::new(caret.index));
            // A natural text-height bar centred on the line — egui centres glyphs within the
            // line box, so `cr.center().y` is the glyph centre. (Full line-box height looked
            // oversized.)
            let half = block::block_style(p.kind).font.size * 0.62;
            let x = origin.x + cr.center().x;
            let cy = origin.y + cr.center().y;
            painter.vline(x, (cy - half)..=(cy + half), Stroke::new(2.0, theme::ACCENT));
        }
    }
}

/// The gutter `+` (insert) and `⋮⋮` (grip) drawn as primitives, so we don't depend on
/// glyph availability.
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

/// A square to-do checkbox; filled accent with a check when done.
fn paint_checkbox(painter: &Painter, r: Rect, done: bool) {
    let border = if done { theme::ACCENT } else { theme::BD_HI };
    if done {
        painter.rect_filled(r, CornerRadius::same(0), theme::ACCENT);
    }
    painter.rect_stroke(r, CornerRadius::same(0), Stroke::new(1.5, border), StrokeKind::Inside);
    if done {
        let check = vec![
            pos2(r.left() + 4.0, r.center().y),
            pos2(r.left() + 6.5, r.bottom() - 4.0),
            pos2(r.right() - 3.5, r.top() + 4.5),
        ];
        painter.add(Shape::line(check, Stroke::new(2.0, theme::BG_PAGE)));
    }
}
