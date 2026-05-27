// Interactive atoms: the square text field and the brand's pixel-offset CTA button (an
// in-flow full-width form, and a positioned variant for painter-driven screens).

use std::f32::consts::TAU;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Sense, Stroke, StrokeKind};

use crate::theme;

// A square field: mono label above, dark fill, accent focus ring, ✓ when `ok`.
pub fn field(ui: &mut egui::Ui, label: &str, value: &mut String, password: bool, ok: bool) -> egui::Response {
    ui.label(
        egui::RichText::new(label)
            .font(FontId::new(11.0, FontFamily::Monospace))
            .color(theme::FG_3)
            .extra_letter_spacing(0.4),
    );
    ui.add_space(6.0);

    let family = if password { FontFamily::Monospace } else { FontFamily::Proportional };
    let mut edit = egui::TextEdit::singleline(value)
        .desired_width(f32::INFINITY)
        .margin(Margin::symmetric(12, 9))
        .font(FontId::new(14.0, family))
        .background_color(theme::BG_1);
    if password {
        edit = edit.password(true);
    }
    let resp = ui.add(edit);

    if resp.has_focus() {
        let ring = resp.rect.expand(3.0);
        ui.painter()
            .rect_stroke(ring, CornerRadius::same(0), Stroke::new(2.0, alpha(theme::ACCENT, 90)), StrokeKind::Outside);
    }
    if ok {
        ui.painter().text(
            egui::pos2(resp.rect.right() - 16.0, resp.rect.center().y),
            Align2::CENTER_CENTER,
            "✓",
            FontId::new(13.0, FontFamily::Proportional),
            theme::OK,
        );
    }
    resp
}

// Full-width CTA, allocated in-flow (signup). While `loading` it shows a spinner, swallows
// clicks, and keeps itself repainting so the spinner animates even on a static backdrop.
pub fn cta_button(ui: &mut egui::Ui, label: &str, loading: bool) -> egui::Response {
    let sense = if loading { Sense::hover() } else { Sense::click() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), sense);
    let pressed = !loading && resp.is_pointer_button_down_on();
    if loading {
        ui.ctx().request_repaint();
    }
    paint_cta(ui.painter(), rect, label, resp.hovered(), pressed, loading, ui.input(|i| i.time) as f32);
    resp
}

// CTA at an explicit rect (painter-driven screens, e.g. the centered recovery button).
pub fn offset_button(ui: &egui::Ui, rect: Rect, label: &str) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("cta", label)), Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    paint_cta(ui.painter(), rect, label, resp.hovered(), pressed, false, 0.0);
    resp
}

// The accent face with a 3px hard-offset shadow that the face presses onto when held.
fn paint_cta(painter: &egui::Painter, rect: Rect, label: &str, hovered: bool, pressed: bool, loading: bool, t: f32) {
    let shadow = Rect::from_min_size(rect.min + egui::vec2(3.0, 3.0), rect.size());
    let face = if pressed { shadow } else { rect };
    if !pressed {
        painter.rect_filled(shadow, CornerRadius::same(0), theme::ACCENT_PRESS);
    }
    let fill = if !loading && hovered { theme::ACCENT_HOVER } else { theme::ACCENT };
    painter.rect_filled(face, CornerRadius::same(0), fill);
    painter.rect_stroke(face, CornerRadius::same(0), Stroke::new(1.0, theme::ACCENT_PRESS), StrokeKind::Inside);

    if loading {
        spinner(painter, face.center(), 9.0, t);
    } else {
        painter.text(face.center(), Align2::CENTER_CENTER, label, FontId::new(13.0, theme::mono_sb()), Color32::WHITE);
    }
}

fn spinner(painter: &egui::Painter, center: egui::Pos2, radius: f32, t: f32) {
    let spokes = 12;
    let head = t * 7.0;
    for k in 0..spokes {
        let frac = k as f32 / spokes as f32;
        let dir = egui::Vec2::angled(head + frac * TAU);
        let color = Color32::from_rgba_unmultiplied(255, 255, 255, (40.0 + frac * 215.0) as u8);
        painter.line_segment([center + dir * (radius * 0.5), center + dir * radius], Stroke::new(2.0, color));
    }
}

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
