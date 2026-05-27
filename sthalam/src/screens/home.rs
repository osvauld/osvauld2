use eframe::egui;
use vault::Vault;

use super::common::{back_to_accounts, short_did};
use crate::app::Screen;
use crate::theme;

pub fn home(ui: &mut egui::Ui, app: &mut app_host::App, vault: &mut Vault) -> Option<Screen> {
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
                });
            });
        });

    ui.add_space(24.0);
    app.frame(ui);
    next
}
