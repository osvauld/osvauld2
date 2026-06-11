//! Paint laid-out boxes onto an `egui::Ui` — the whole draw step.
//!
//! [`crate::layout`] already did the thinking (geometry, wrapping, colour-neutral galleys), so
//! this is a flat back-to-front loop: per box pick the `Look` for the pointer state, fill the
//! background, apply a default feedback tint only when the app declared no state styles, and
//! stamp the recoloured galley (plus selection + caret for the focused editor).

use egui::epaint::Shadow;
use egui::{pos2, Color32, CornerRadius, Stroke, StrokeKind};
use text_edit::TextField;

use crate::layout::Placed;
use crate::node::Corners;

/// The selection highlight (the accent at low alpha) painted behind a focused editor's glyphs.
const SELECTION: Color32 = Color32::from_rgba_premultiplied(0x2b, 0x44, 0x73, 0x80);

/// Per-corner radii in egui's u8 form.
fn corner_radius(c: Corners) -> CornerRadius {
    let r = |v: f32| v.clamp(0.0, 255.0) as u8;
    CornerRadius { nw: r(c.tl), ne: r(c.tr), sw: r(c.bl), se: r(c.br) }
}

/// Live pointer state for click feedback: cursor position in app-local coordinates (`None` when
/// not over the cell) and whether the primary button is held.
pub(crate) struct Pointer {
    pub hover: Option<egui::Pos2>,
    pub pressed: bool,
}

/// Paint every box in order (parents first, so children land on top). `focus` is the focused
/// editor's id and retained caret/selection, if any — drawn on the box whose `editor` id matches.
pub(crate) fn paint(ui: &egui::Ui, placed: &[Placed], pointer: &Pointer, focus: Option<(&str, &TextField)>) {
    for node in placed {
        // Clip to the scroll region so scrolled-out content doesn't bleed; non-scrolled boxes
        // carry the cell rect (a no-op clip).
        let painter = ui.painter().with_clip_rect(node.clip);
        let over = pointer.hover.is_some_and(|p| node.rect.contains(p));
        let clickable = node.on_click.is_some();

        // pressed (clickable) → active, hovered → hover, else resting — each falling back to the
        // resting look when the app didn't define it.
        let look = if over && pointer.pressed && clickable {
            node.active.unwrap_or(node.base)
        } else if over {
            node.hover.unwrap_or(node.base)
        } else {
            node.base
        };

        let radius = corner_radius(look.corner_radius);
        let alpha = look.opacity;

        // Shadow → fill → border, back-to-front like CSS paints a box.
        if let Some(s) = look.shadow {
            let shadow = Shadow {
                offset: [s.offset[0].round() as i8, s.offset[1].round() as i8],
                blur: s.blur.clamp(0.0, 255.0) as u8,
                spread: s.spread.clamp(0.0, 255.0) as u8,
                color: s.color.gamma_multiply(alpha),
            };
            painter.add(shadow.as_shape(node.rect, radius));
        }
        if let Some(bg) = look.background {
            painter.rect_filled(node.rect, radius, bg.gamma_multiply(alpha));
        }
        if let Some(b) = look.border {
            painter.rect_stroke(
                node.rect,
                radius,
                Stroke::new(b.width, b.color.gamma_multiply(alpha)),
                StrokeKind::Inside,
            );
        }

        // Default feedback only for clickable boxes that declared no states of their own, so
        // plain buttons still respond without the app spelling out hover/active.
        if over && clickable && node.hover.is_none() && node.active.is_none() {
            let overlay =
                if pointer.pressed { Color32::from_white_alpha(42) } else { Color32::from_white_alpha(16) };
            painter.rect_filled(node.rect, radius, overlay);
        }

        if let Some((origin, galley)) = &node.text {
            // This box's editor field, when it's the focused one.
            let field = focus.filter(|(id, _)| node.editor.as_deref() == Some(*id)).map(|(_, f)| f);
            let shift = origin.to_vec2();

            // Selection highlight goes *behind* the glyphs.
            if let Some(field) = field {
                for r in field.selection_rects(galley) {
                    painter.rect_filled(r.translate(shift), 0.0, SELECTION.gamma_multiply(alpha));
                }
            }
            // The galley is colour-neutral (`PLACEHOLDER`); the state's colour applies here.
            // Caveat: baked per-run colours ignore alpha — only neutral text dims for now.
            painter.galley(*origin, galley.clone(), look.color.gamma_multiply(alpha));
            // The caret goes on top of the glyphs.
            if let Some(field) = field {
                let c = field.caret_rect(galley).translate(shift);
                let x = c.center().x;
                painter.line_segment(
                    [pos2(x, c.top()), pos2(x, c.bottom())],
                    Stroke::new(1.5, look.color.gamma_multiply(alpha)),
                );
            }
        }
    }
}
