use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, StrokeKind};
use vault::ItemKind;

use crate::theme;
use super::atoms::{elide, right_divider};

pub(super) fn home_tab(ui: &mut egui::Ui, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(38.0, 36.0), Sense::click());
    paint_tab_bg(ui, rect, active, resp.hovered());
    let color = if active { theme::ACCENT } else { theme::FG_3 };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "⌂", FontId::new(14.0, FontFamily::Monospace), color);
    right_divider(ui, rect);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn workspace_tab(ui: &mut egui::Ui, name: &str, tint: Color32, active: bool) -> (egui::Response, egui::Response) {
    let font = FontId::new(12.5, FontFamily::Proportional);
    let label_w = ui.painter().layout_no_wrap(name.to_owned(), font.clone(), theme::FG_1).size().x.min(140.0);
    let width = 12.0 + 8.0 + 8.0 + label_w + 8.0 + 11.0 + 12.0;

    let (rect, body) = ui.allocate_exact_size(egui::vec2(width, 36.0), Sense::click());
    paint_tab_bg(ui, rect, active, body.hovered());

    let p = ui.painter();
    let cy = rect.center().y;
    p.rect_filled(Rect::from_center_size(egui::pos2(rect.left() + 16.0, cy), egui::vec2(8.0, 8.0)), 0.0, tint);
    let text_color = if active { theme::FG_1 } else { theme::FG_3 };
    p.text(egui::pos2(rect.left() + 28.0, cy), Align2::LEFT_CENTER, elide(name, 18), font, text_color);

    let (close_rect, close) = close_btn(ui, rect, ("ws_close", name));
    paint_close(ui, close_rect, close.hovered());
    right_divider(ui, rect);
    (body.on_hover_cursor(egui::CursorIcon::PointingHand), close.on_hover_cursor(egui::CursorIcon::PointingHand))
}

/// Generic tab for every `Tab::Open` item — kind badge + name + ×.
/// Widget key uses `item_id` (unique hex) so two items with the same name never collide.
pub(super) fn item_tab(ui: &mut egui::Ui, kind: &ItemKind, name: &str, item_id: &str, active: bool) -> (egui::Response, egui::Response) {
    let badge = format!(".{}", kind.as_str());
    let badge_font = FontId::new(9.5, FontFamily::Monospace);
    let name_font = FontId::new(12.5, FontFamily::Proportional);
    let p = ui.painter();
    let badge_w = p.layout_no_wrap(badge.clone(), badge_font.clone(), theme::FG_4).size().x;
    let label_w = p.layout_no_wrap(name.to_owned(), name_font.clone(), theme::FG_1).size().x.min(140.0);
    let width = 12.0 + badge_w + 6.0 + label_w + 8.0 + 11.0 + 12.0;

    let (rect, body) = ui.allocate_exact_size(egui::vec2(width, 36.0), Sense::click());
    paint_tab_bg(ui, rect, active, body.hovered());

    let p = ui.painter();
    let cy = rect.center().y;
    p.text(egui::pos2(rect.left() + 12.0, cy), Align2::LEFT_CENTER, &badge, badge_font, theme::FG_4);
    let text_color = if active { theme::FG_1 } else { theme::FG_3 };
    p.text(egui::pos2(rect.left() + 12.0 + badge_w + 6.0, cy), Align2::LEFT_CENTER, elide(name, 18), name_font, text_color);

    let (close_rect, close) = close_btn(ui, rect, ("item_close", item_id));
    paint_close(ui, close_rect, close.hovered());
    right_divider(ui, rect);
    (body.on_hover_cursor(egui::CursorIcon::PointingHand), close.on_hover_cursor(egui::CursorIcon::PointingHand))
}

pub(super) fn plus_tab(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(32.0, 36.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, Color32::from_white_alpha(6));
    }
    let color = if resp.hovered() { theme::FG_2 } else { theme::FG_4 };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", FontId::new(15.0, FontFamily::Monospace), color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn cmdk_hint(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 22.0), Sense::hover());
    ui.painter().rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, theme::BD_2), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "⌘K", FontId::new(10.5, FontFamily::Monospace), theme::FG_4);
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn paint_tab_bg(ui: &egui::Ui, rect: Rect, active: bool, hovered: bool) {
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 0.0, theme::BG_PAGE);
        p.rect_filled(Rect::from_min_size(rect.min, egui::vec2(rect.width(), 2.0)), 0.0, theme::ACCENT);
    } else if hovered {
        p.rect_filled(rect, 0.0, Color32::from_white_alpha(6));
    }
}

fn close_btn(ui: &mut egui::Ui, tab_rect: Rect, key: impl std::hash::Hash) -> (Rect, egui::Response) {
    let r = Rect::from_center_size(
        egui::pos2(tab_rect.right() - 13.0, tab_rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    let resp = ui.interact(r, ui.id().with(key), Sense::click());
    (r, resp)
}

fn paint_close(ui: &egui::Ui, rect: Rect, hovered: bool) {
    let color = if hovered { theme::FG_1 } else { theme::FG_4 };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "×", FontId::new(12.0, FontFamily::Monospace), color);
}
