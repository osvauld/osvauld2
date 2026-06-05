//! Paint laid-out boxes onto an `egui::Ui` — the whole draw step.
//!
//! [`crate::layout`] already did the thinking (geometry, wrapping, galley shaping), so this
//! is a flat back-to-front loop: fill the background, then stamp the galley. It paints with
//! egui's own `Painter`, so the result tessellates into native egui meshes — the same form
//! the compositor already composites from a wasm app, with no texture round-trip.

use egui::CornerRadius;

use crate::layout::Placed;

/// Paint every box in order (parents first, so children land on top).
pub(crate) fn paint(ui: &egui::Ui, placed: &[Placed]) {
    let painter = ui.painter();
    for node in placed {
        if let Some(bg) = node.background {
            let radius = node.corner_radius.clamp(0.0, 255.0) as u8;
            painter.rect_filled(node.rect, CornerRadius::same(radius), bg);
        }
        if let Some((origin, galley)) = &node.text {
            // Colour is baked into the galley by `layout::shape`; the fallback is unused.
            painter.galley(*origin, galley.clone(), egui::Color32::PLACEHOLDER);
        }
    }
}
