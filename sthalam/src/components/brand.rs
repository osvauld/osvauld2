// Brand text atoms shared across screens: the wordmark, pixel headings, the tagline, and
// the corner tags. All draw the VT323 pixel face twice for the signature offset shadow.

use eframe::egui::{self, Align2, FontFamily, FontId, Pos2, Rect, RichText};

use crate::theme;

// The "sthalam" wordmark, allocated in-flow, with a pixel-offset shadow scaled to the size
// (96px on signup, 64px on login). Left edge of the allocated box is the glyph origin.
pub fn wordmark(ui: &mut egui::Ui, size: f32) {
    let offset = (size / 24.0).round();
    let font = FontId::new(size, theme::pixel());
    let galley = ui.painter().layout_no_wrap("sthalam".to_owned(), font.clone(), theme::ACCENT);
    let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(offset, offset), egui::Sense::hover());
    pixel_text(ui.painter(), rect.min, "sthalam", font, offset);
}

// A left-aligned pixel heading painted at an absolute position (e.g. "your seed").
pub fn pixel_heading(painter: &egui::Painter, left_top: Pos2, text: &str, size: f32) {
    pixel_text(painter, left_top, text, FontId::new(size, theme::pixel()), 3.0);
}

// Shadow + face — egui has no text-shadow, so the glyphs are drawn twice.
fn pixel_text(painter: &egui::Painter, left_top: Pos2, text: &str, font: FontId, offset: f32) {
    painter.text(left_top + egui::vec2(offset, offset), Align2::LEFT_TOP, text, font.clone(), theme::ACCENT_PRESS);
    painter.text(left_top, Align2::LEFT_TOP, text, font, theme::ACCENT);
}

// Tracked mono caps under the wordmark.
pub fn tagline(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(11.0, FontFamily::Monospace))
            .color(theme::FG_3)
            .extra_letter_spacing(2.0),
    );
}

// The terminal corner tags. Top-left and bottom-right are the fixed brand stamps; the
// caller fills `top_right` (the screen's step, e.g. "02 / login") and `bottom_left` (the
// keyboard hint, e.g. "↑↓ SELECT · ↩ UNLOCK"). Either may be empty to skip it.
pub fn corner_tags(ui: &egui::Ui, rect: Rect, top_right: &str, bottom_left: &str) {
    let font = FontId::new(10.5, FontFamily::Monospace);
    let p = ui.painter();
    p.text(rect.left_top() + egui::vec2(32.0, 16.0), Align2::LEFT_TOP, "OSVAULD · 01 · STHALAM", font.clone(), theme::FG_4);
    p.text(rect.right_bottom() + egui::vec2(-32.0, -18.0), Align2::RIGHT_BOTTOM, "v0.1.0 · local", font.clone(), theme::FG_4);
    if !top_right.is_empty() {
        p.text(rect.right_top() + egui::vec2(-32.0, 16.0), Align2::RIGHT_TOP, top_right, font.clone(), theme::FG_4);
    }
    if !bottom_left.is_empty() {
        p.text(rect.left_bottom() + egui::vec2(32.0, -18.0), Align2::LEFT_BOTTOM, bottom_left, font, theme::FG_4);
    }
}
