use app_host::App;
use eframe::egui::{self, FontFamily, FontId};
use vault::WorkspaceItem;

use crate::theme;
use super::atoms::hairline_bottom;

/// Render an engine app inside a shell tab.
/// Returns a CRDT snapshot if the app's state changed this frame.
pub(super) fn body(ui: &mut egui::Ui, item: &WorkspaceItem, engine: &mut App) -> Option<Vec<u8>> {
    egui::Panel::top(egui::Id::new(("app_context", &item.id)))
        .exact_size(30.0)
        .frame(egui::Frame::default().fill(theme::BG_1))
        .show_inside(ui, |ui| {
            hairline_bottom(ui);
            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(".app").font(FontId::new(10.0, FontFamily::Monospace)).color(theme::FG_4));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&item.name).font(FontId::new(11.5, FontFamily::Monospace)).color(theme::FG_2));
            });
        });

    let mut crdt = None;
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE))
        .show_inside(ui, |ui| {
            crdt = engine.show(ui);
        });
    crdt
}
