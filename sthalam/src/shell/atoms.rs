use eframe::egui;

use crate::theme;

pub(super) fn column(ui: &mut egui::Ui, max_w: f32, contents: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width().min(max_w);
    let pad = ((ui.available_width() - w) / 2.0).max(0.0) + if w < max_w { 32.0 } else { 0.0 };
    let inner = (ui.available_width() - pad * 2.0).max(120.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.allocate_ui_with_layout(egui::vec2(inner, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(inner);
            contents(ui);
        });
    });
}

pub(super) fn hairline_bottom(ui: &egui::Ui) {
    let r = ui.max_rect();
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, egui::Stroke::new(1.0, theme::BD_2));
}

pub(super) fn right_divider(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().vline(rect.right(), rect.y_range(), egui::Stroke::new(1.0, theme::BD_1));
}

pub(super) fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

pub(super) fn short_id(id: &str) -> String {
    if id.len() <= 10 {
        format!("id {id}")
    } else {
        format!("id {}…{}", &id[..6], &id[id.len() - 2..])
    }
}
