use doc_editor::{Doc, DocEditor};
use eframe::egui::{self, FontFamily, FontId};
use vault::WorkspaceItem;

use crate::theme;

/// Render the doc editor surface. Returns `true` if the doc was edited this frame (autosave trigger).
pub(super) fn body(ui: &mut egui::Ui, item: &WorkspaceItem, doc: &Doc, editor: &mut DocEditor) -> bool {
    egui::Panel::top(egui::Id::new(("doc_context", &item.id)))
        .exact_size(30.0)
        .frame(egui::Frame::default().fill(theme::BG_1))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(format!(".{}", item.kind.as_str())).font(FontId::new(10.0, FontFamily::Monospace)).color(theme::FG_4));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&item.name).font(FontId::new(11.5, FontFamily::Monospace)).color(theme::FG_2));
            });
        });

    let mut dirty = false;
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE))
        .show_inside(ui, |ui| {
            // editor.show owns its own ScrollArea — no outer scroll wrapper needed.
            // Centre by allocating a sub-UI of the right width at full available height.
            let avail = ui.available_size();
            let max_w = 720.0_f32.min(avail.x);
            let h_pad = ((avail.x - max_w) / 2.0).max(32.0);
            ui.horizontal(|ui| {
                ui.add_space(h_pad);
                ui.allocate_ui_with_layout(
                    egui::vec2(avail.x - 2.0 * h_pad, avail.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        dirty = editor.show(ui, doc, false);
                    },
                );
            });
        });
    dirty
}
