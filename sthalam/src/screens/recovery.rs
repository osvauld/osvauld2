use eframe::egui::{self, Align2, FontFamily, FontId, Rect, Stroke};

use crate::app::Screen;
use crate::components::{brand, controls, Backdrop};
use crate::theme;

// The "your seed" screen: the 24 words in quiet column-major rows, one amber warning, and a
// centered "I've saved it" button. No reveal/copy/checkbox — write them down and continue.
pub fn recovery(ui: &mut egui::Ui, words: &str, backdrop: &mut Backdrop) -> Option<Screen> {
    let rect = ui.max_rect();
    backdrop.show(ui, rect);

    let content = rect.shrink2(egui::vec2(56.0, 32.0));
    topbar(ui, content);
    brand::pixel_heading(ui.painter(), egui::pos2(content.left(), content.top() + 44.0), "your seed", 56.0);
    word_columns(ui, content, content.top() + 128.0, words);

    let button = button_rect(content);
    warning_band(ui, content, button.top() - 18.0);
    controls::offset_button(ui, button, "I'VE SAVED IT ▸")
        .clicked()
        .then(Screen::home)
}

// "OSVAULD · 01 · STHALAM" on the left; step dots + "02 / 03" on the right.
fn topbar(ui: &egui::Ui, content: Rect) {
    let mono = |s| FontId::new(s, FontFamily::Monospace);
    let p = ui.painter();
    p.text(content.left_top(), Align2::LEFT_TOP, "OSVAULD · 01 · STHALAM", mono(10.5), theme::FG_4);

    let label = p.text(content.right_top(), Align2::RIGHT_TOP, "02 / 03", mono(11.0), theme::FG_3);
    let mut x = label.left() - 12.0;
    let y = content.top() + 2.0;
    // Drawn right-to-left: the active (wide) dot is the middle one.
    for (w, fill) in [(6.0, theme::BG_3), (22.0, theme::ACCENT), (6.0, theme::ACCENT)] {
        x -= w;
        p.rect_filled(Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, 6.0)), 0.0, fill);
        x -= 6.0;
    }
}

// 24 words as hairline rows, laid out column-major so each column reads 1→N top-to-bottom
// (how you write them on paper). 3 columns normally, 2 when the window is narrow.
fn word_columns(ui: &egui::Ui, content: Rect, top: f32, words: &str) {
    let words: Vec<&str> = words.split_whitespace().collect();
    if words.is_empty() {
        return;
    }
    let cols = if content.width() > 600.0 { 3 } else { 2 };
    let rows = words.len().div_ceil(cols);
    let block_w = content.width().min(if cols == 3 { 720.0 } else { 520.0 });
    let left = content.center().x - block_w / 2.0;
    let col_gap = 28.0;
    let col_w = (block_w - col_gap * (cols as f32 - 1.0)) / cols as f32;
    const ROW_H: f32 = 36.0;

    let idx_font = FontId::new(11.0, FontFamily::Monospace);
    let word_font = FontId::new(15.0, FontFamily::Monospace);
    let p = ui.painter();
    for (i, word) in words.iter().enumerate() {
        let x = left + (i / rows) as f32 * (col_w + col_gap);
        let y = top + (i % rows) as f32 * ROW_H;
        let cy = y + ROW_H / 2.0;
        p.text(egui::pos2(x + 22.0, cy), Align2::RIGHT_CENTER, format!("{:02}", i + 1), idx_font.clone(), theme::FG_4);
        p.text(egui::pos2(x + 36.0, cy), Align2::LEFT_CENTER, *word, word_font.clone(), theme::FG_1);
        p.line_segment([egui::pos2(x, y + ROW_H), egui::pos2(x + col_w, y + ROW_H)], Stroke::new(1.0, theme::BD_1));
    }
}

// Amber band — the one piece of emphasis now that the checkbox gate is gone. Its bottom sits
// at `bottom`; height grows with the wrapped text so it stays readable on narrow windows.
fn warning_band(ui: &egui::Ui, content: Rect, bottom: f32) {
    const PAD: f32 = 14.0;
    const MARKER: f32 = 22.0;
    let width = content.width().min(720.0);
    let left = content.center().x - width / 2.0;
    let text = "we can't reset your passphrase — write these 24 words down and keep them safe.";
    let font = FontId::new(12.0, FontFamily::Monospace);
    let galley = ui.painter().layout(text.to_owned(), font, theme::WARN, width - PAD * 2.0 - MARKER);
    let height = galley.size().y + PAD * 2.0;
    let band = Rect::from_min_size(egui::pos2(left, bottom - height), egui::vec2(width, height));

    let p = ui.painter();
    p.rect_filled(band, 0.0, theme::WARN_BG);
    p.rect_filled(Rect::from_min_size(band.min, egui::vec2(2.0, height)), 0.0, theme::WARN); // left rule
    p.text(egui::pos2(left + PAD, band.top() + PAD - 1.0), Align2::LEFT_TOP, "!", FontId::new(13.0, theme::mono_sb()), theme::WARN);
    p.galley(egui::pos2(left + PAD + MARKER, band.top() + PAD), galley, theme::WARN);
}

fn button_rect(content: Rect) -> Rect {
    let size = egui::vec2(190.0, 42.0);
    Rect::from_min_size(egui::pos2(content.center().x - size.x / 2.0, content.bottom() - size.y), size)
}
