// Cross-screen helpers: where to return when an account screen is dismissed, the inline
// error line (signup), and DID shortening.

use eframe::egui;
use vault::Vault;

use crate::app::{AccountsView, Screen, SignupForm};
use crate::theme;

// Where to go when an account screen is dismissed: the picker if any account exists, else
// straight to signup.
pub(crate) fn back_to_accounts(vault: &Vault) -> Option<Screen> {
    match vault.accounts() {
        Ok(accounts) if !accounts.is_empty() => Some(Screen::Accounts(AccountsView { accounts, selected: 0 })),
        _ => Some(Screen::Signup(SignupForm::default())),
    }
}

// A quiet mono caps text link (add account / recover / forget). Returns its click Response.
pub(crate) fn quiet_link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
                .color(theme::FG_3)
                .extra_letter_spacing(1.4),
        )
        .selectable(false) // it's a link, not selectable text — otherwise hover shows the I-beam
        .sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn show_error(ui: &mut egui::Ui, error: Option<&str>) {
    if let Some(error) = error {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(error).color(theme::ERR).size(14.0));
    }
}

// did:key:z6Mk…r9 — DIDs are ASCII, so byte slicing is safe.
pub(crate) fn short_did(did: &str) -> String {
    if did.len() <= 20 {
        return did.to_string();
    }
    format!("{}…{}", &did[..16], &did[did.len() - 4..])
}
