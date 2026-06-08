//! Drag-to-reorder by the gutter grip (`⋮⋮`): dim the source row, follow the cursor with a
//! ghost, show a drop indicator (accent sibling line, or outline over a nestable target for a
//! drop-INTO). On release the block moves in the tree (one `mov`). Painter-safe primitives only.

use egui::{pos2, vec2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Ui};
use loro::TreeID;

use crate::model::Doc;
use crate::theme;

use super::layout;
use super::{DocEditor, Placed};

/// Where a drop would land relative to a target block.
#[derive(Clone, Copy)]
enum DropAt {
    Above(TreeID),
    Below(TreeID),
    /// Last child of the target (nest).
    Into(TreeID),
}

impl DocEditor {
    /// Begin dragging `block` (its grip's drag just started).
    pub(super) fn start_drag(&mut self, block: TreeID) {
        self.drag = Some(block);
    }

    /// Resolve the drop target + intent under `pointer`: a nestable target pushed into its
    /// middle third nests; otherwise the row splits above/below at its midline.
    fn drop_at(&self, placed: &[Placed], rect: Rect, pointer: Pos2) -> Option<DropAt> {
        let bi = layout::block_at_y(placed, rect, pointer.y)?;
        let p = &placed[bi];
        let frac = (pointer.y - rect.top() - p.row_top) / (p.row_bottom - p.row_top).max(1.0);
        let nestable = p.kind.is_list();
        let pushed_in = (pointer.x - rect.left()) >= p.content_x + 16.0;
        Some(if nestable && pushed_in && (0.33..=0.67).contains(&frac) {
            DropAt::Into(p.id)
        } else if frac < 0.5 {
            DropAt::Above(p.id)
        } else {
            DropAt::Below(p.id)
        })
    }

    /// On pointer release, commit the drag: move the block in the tree and clear the drag
    /// state. Returns whether the document changed (a no-op self/descendant drop returns false).
    pub(super) fn commit_drag(&mut self, ui: &Ui, doc: &Doc, rect: Rect, placed: &[Placed]) -> bool {
        let Some(block) = self.drag.take() else { return false };
        let Some(pointer) = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())) else {
            return false;
        };
        match self.drop_at(placed, rect, pointer) {
            Some(DropAt::Above(t)) => doc.move_before(block, t),
            Some(DropAt::Below(t)) => doc.move_after(block, t),
            Some(DropAt::Into(t)) => doc.move_into(block, t),
            None => false,
        }
    }

    /// Paint the live drag (dimmed source + drop indicator + ghost) on a foreground layer.
    pub(super) fn paint_drag(&self, ui: &Ui, doc: &Doc, rect: Rect, viewport: Rect, placed: &[Placed]) {
        let Some(block) = self.drag else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("drag_overlay"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(viewport);

        // Dim the source row by laying the page colour over it at 0.75 alpha.
        if let Some(p) = placed.iter().find(|p| p.id == block) {
            let row = Rect::from_min_max(
                pos2(rect.left() + p.gutter_left, rect.top() + p.row_top),
                pos2(rect.left() + p.content_right, rect.top() + p.row_bottom),
            );
            painter.rect_filled(row, CornerRadius::same(0), Color32::from_rgba_unmultiplied(0x0A, 0x0B, 0x10, 190));
        }

        let Some(pointer) = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())) else {
            return;
        };
        if let Some(at) = self.drop_at(placed, rect, pointer) {
            paint_indicator(&painter, rect, placed, at);
        }
        paint_ghost(&painter, doc, pointer, block);
    }
}

/// The drop indicator: a 3px accent sibling line (with a square end-cap at the indent), or an
/// outline + tint over a nestable target for a drop-INTO.
fn paint_indicator(painter: &Painter, rect: Rect, placed: &[Placed], at: DropAt) {
    let (id, into, below) = match at {
        DropAt::Above(t) => (t, false, false),
        DropAt::Below(t) => (t, false, true),
        DropAt::Into(t) => (t, true, false),
    };
    let Some(p) = placed.iter().find(|p| p.id == id) else { return };
    let x0 = rect.left() + p.spine_x;
    let x1 = rect.left() + p.content_right;
    if into {
        let r = Rect::from_min_max(pos2(x0, rect.top() + p.row_top), pos2(x1, rect.top() + p.row_bottom));
        painter.rect_filled(r, CornerRadius::same(0), theme::ACCENT_BG);
        painter.rect_stroke(r, CornerRadius::same(0), Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
    } else {
        let y = rect.top() + if below { p.row_bottom } else { p.row_top };
        painter.hline(x0..=x1, y, Stroke::new(3.0, theme::ACCENT));
        painter.rect_filled(Rect::from_center_size(pos2(x0, y), vec2(6.0, 6.0)), CornerRadius::same(0), theme::ACCENT);
    }
}

/// A floating preview card following the cursor: the block's first line on a raised square.
fn paint_ghost(painter: &Painter, doc: &Doc, pointer: Pos2, block: TreeID) {
    let text = doc.text(block);
    let line: String = text.lines().next().unwrap_or("").chars().take(64).collect();
    let label = if line.is_empty() { "Empty block".to_owned() } else { line };
    let galley = painter.layout(label, FontId::proportional(14.0), theme::FG_1, 260.0);
    let pad = vec2(12.0, 8.0);
    let origin = pointer + vec2(14.0, 8.0);
    let card = Rect::from_min_size(origin, galley.size() + pad * 2.0);
    painter.rect_filled(card.translate(vec2(4.0, 4.0)), CornerRadius::same(0), Color32::from_black_alpha(110));
    painter.rect_filled(card, CornerRadius::same(0), theme::BG_2);
    painter.rect_stroke(card, CornerRadius::same(0), Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
    painter.galley(origin + pad, galley, theme::FG_1);
}
