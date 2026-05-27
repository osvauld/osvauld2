use eframe::egui;
use vault::Vault;
use zeroize::Zeroize;

use super::common::{back_to_accounts, show_error};
use crate::app::{Screen, SignupForm, SignupResult};
use crate::components::{brand, controls, Backdrop};

pub fn signup(ui: &mut egui::Ui, vault: &mut Vault, form: &mut SignupForm, backdrop: &mut Backdrop) -> Option<Screen> {
    let rect = ui.max_rect();
    backdrop.show(ui, rect);
    brand::corner_tags(ui, rect, "", "");

    // Argon2 runs on a worker thread; poll it each frame.
    if let Some(result) = poll_signup(form) {
        match result.and_then(|prepared| vault.commit_signup(prepared)) {
            Ok((_did, mnemonic)) => return Some(Screen::Mnemonic(mnemonic.to_string())),
            Err(error) => form.error = Some(error.to_string()),
        }
    }
    let loading = form.pending.is_some();

    let mut next = None;
    let top = ((rect.height() - 470.0) * 0.5).max(16.0);
    ui.vertical_centered(|ui| {
        ui.add_space(top);
        brand::wordmark(ui, 96.0);
        ui.add_space(12.0);
        brand::tagline(ui, "BROWSER FOR THE EXTENDED INTERNET");
        ui.add_space(34.0);

        ui.allocate_ui_with_layout(
            egui::vec2(360.0, 300.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(360.0);

                let name = controls::field(ui, "name", &mut form.label, false, false);
                if !loading && ui.memory(|m| m.focused().is_none()) {
                    name.request_focus();
                }
                ui.add_space(14.0);
                controls::field(ui, "passphrase", &mut form.passphrase, true, false);
                ui.add_space(14.0);
                let matched = !form.passphrase.is_empty() && form.passphrase == form.confirm;
                controls::field(ui, "confirm", &mut form.confirm, true, matched);

                ui.add_space(20.0);
                let clicked = controls::cta_button(ui, "OPEN YOUR STHALAM ▸", loading).clicked();
                if !loading && (clicked || ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    start_signup(form);
                }
                show_error(ui, form.error.as_deref());

                if form.from_accounts && !loading {
                    ui.add_space(10.0);
                    if ui.button("← back").clicked() {
                        next = back_to_accounts(vault);
                    }
                }
            },
        );
    });
    next
}

fn poll_signup(form: &mut SignupForm) -> Option<SignupResult> {
    match form.pending.as_ref()?.try_recv() {
        Ok(result) => {
            form.pending = None;
            Some(result)
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            form.pending = None;
            form.error = Some("Signup failed unexpectedly.".into());
            None
        }
    }
}

fn start_signup(form: &mut SignupForm) {
    let label = form.label.trim().to_string();
    if label.is_empty() {
        form.error = Some("Account name can't be empty.".into());
        return;
    }
    if form.passphrase.is_empty() {
        form.error = Some("Passphrase can't be empty.".into());
        return;
    }
    if form.passphrase != form.confirm {
        form.error = Some("Passphrases don't match.".into());
        return;
    }
    form.error = None;

    let passphrase = form.passphrase.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut passphrase = passphrase;
        let result = Vault::prepare_signup(&label, &passphrase);
        passphrase.zeroize();
        let _ = tx.send(result);
    });
    form.pending = Some(rx);
}
