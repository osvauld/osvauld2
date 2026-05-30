use eframe::egui;
use vault::Vault;

use super::common::{back_to_accounts, short_did};
use crate::app::Screen;
use crate::theme;
use compositor::Workspace;

pub fn home(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    vault: &mut Vault,
    frame: &mut eframe::Frame,
) -> Option<Screen> {
    let mut next = None;
    let current = vault.current();

    egui::Frame::default()
        .fill(theme::CARD)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(account) = &current {
                    ui.strong(account.label.as_str());
                    ui.add_space(6.0);
                    ui.weak(short_did(&account.did));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Lock").clicked() {
                        vault.lock();
                        next = back_to_accounts(vault);
                    }
                    if ui.button("+ New counter").clicked() {
                        // Spawn an app-cell mid-session — its own thread, renderer,
                        // texture, and context — as a floating window.
                        workspace.add_floating(crate::workspace::new_counter());
                    }
                });
            });
        });

    ui.add_space(24.0);
    if let Some(rs) = frame.wgpu_render_state() {
        // The workspace draws both layers: the tiled tree fills the area, the
        // floating windows draw on top.
        workspace.ui(ui, rs);
    }
    next
}
