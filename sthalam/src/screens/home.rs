use doc_editor::{Doc, DocEditor};
use eframe::egui;
use vault::Vault;

use super::common::{back_to_accounts, short_did};
use crate::app::Screen;
use crate::theme;

/// The home screen: a slim identity bar, then the single `.doc` filling the rest. The
/// document is loaded from the vault on the first frame and re-saved (encrypted) on every
/// edit.
pub fn home(
    ui: &mut egui::Ui,
    editor: &mut DocEditor,
    doc: &mut Option<Doc>,
    vault: &mut Vault,
) -> Option<Screen> {
    let mut next = None;
    let current = vault.current();
    let mut lock = false;
    let mut open_apps = false;

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
                        lock = true;
                    }
                    if ui.button("Apps").clicked() {
                        open_apps = true;
                    }
                });
            });
        });

    // Load the home doc on first render — the vault is unlocked on this screen.
    let document = doc.get_or_insert_with(|| crate::home_doc::load_or_create(vault));

    if open_apps {
        crate::home_doc::save(vault, document); // persist before leaving the doc
        next = Some(Screen::Workspace(crate::workspace::engine_workspace()));
    } else if lock {
        crate::home_doc::save(vault, document); // persist before the keys go away
        vault.lock();
        next = back_to_accounts(vault);
    } else if editor.show(ui, document, false) {
        // The editor fills the rest of the window; persist whenever it changed.
        crate::home_doc::save(vault, document);
    }
    next
}
