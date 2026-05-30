// Unlock the selected identity: its chip (with a switch-back link), a passphrase field, the
// pixel-offset unlock button, and quiet recover/forget links. The Argon2 decrypt runs on a
// worker thread so the window never freezes; the button shows a spinner while it's in flight.

use eframe::egui::{self, Align2, CornerRadius, FontFamily, FontId, Margin, RichText, Sense, Stroke, StrokeKind};
use vault::{Vault, VaultError};
use zeroize::Zeroize;

use super::common::{back_to_accounts, quiet_link, short_did};
use crate::app::{LoginResult, Screen, UnlockForm};
use crate::components::{brand, controls, identicon, Backdrop};
use crate::theme;

const PANEL_W: f32 = 420.0;

pub fn unlock(ui: &mut egui::Ui, vault: &mut Vault, form: &mut UnlockForm, backdrop: &mut Backdrop) -> Option<Screen> {
    let rect = ui.max_rect();
    backdrop.show(ui, rect);
    brand::corner_tags(ui, rect, "02 / login · unlock", "↩ UNLOCK  ·  ESC BACK");

    if let Some(result) = poll_login(form) {
        match result {
            Ok(unlocked) => match vault.commit_login(unlocked) {
                Ok(()) => {
                    return Some(Screen::Home { workspace: Box::new(crate::workspace::demo_workspace()) })
                }
                Err(error) => form.error = Some(error.to_string()),
            },
            Err(VaultError::WrongPassphrase) => {
                form.passphrase.zeroize();
                form.error = Some("passphrase didn't decrypt this identity. try again or recover.".into());
            }
            Err(error) => form.error = Some(error.to_string()),
        }
    }
    let loading = form.pending.is_some();

    if !loading && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return back_to_accounts(vault);
    }

    let mut next = None;
    let top = ((rect.height() - 430.0) * 0.5).max(16.0);
    ui.vertical_centered(|ui| {
        ui.add_space(top);
        brand::wordmark(ui, 64.0);
        ui.add_space(26.0);
        ui.allocate_ui_with_layout(
            egui::vec2(PANEL_W, 360.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(PANEL_W);

                if !loading && account_chip(ui, form).clicked() {
                    next = back_to_accounts(vault);
                }
                ui.add_space(20.0);

                passphrase_field(ui, form, loading);
                ui.add_space(16.0);

                let clicked = controls::cta_button(ui, "UNLOCK  ▸", loading).clicked();
                if !loading && (clicked || ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    start_login(vault, form);
                }
                ui.add_space(14.0);

                footer_links(ui);
            },
        );
    });
    next
}

// Identicon · name · DID for the chosen identity, with a "switch ↺" link to return to the
// picker. The chip body is inert; only the switch zone is clickable.
fn account_chip(ui: &mut egui::Ui, form: &UnlockForm) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::ACCENT_BG);
    p.rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
    p.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())), 0.0, theme::ACCENT);

    let (icon, pad) = (36.0, 14.0);
    identicon(p, egui::pos2(rect.left() + pad, rect.center().y - icon / 2.0), icon, &form.did);
    let tx = rect.left() + pad + icon + 14.0;
    let cy = rect.center().y;
    p.text(egui::pos2(tx, cy - 8.0), Align2::LEFT_CENTER, form.label.as_str(), FontId::new(14.0, FontFamily::Proportional), theme::FG_1);
    p.text(egui::pos2(tx, cy + 9.0), Align2::LEFT_CENTER, short_did(&form.did), FontId::new(10.5, FontFamily::Monospace), theme::FG_4);

    let switch_rect = egui::Rect::from_min_max(egui::pos2(rect.right() - 96.0, rect.top()), rect.right_bottom());
    let switch = ui.interact(switch_rect, ui.id().with("switch"), Sense::click());
    let color = if switch.hovered() { theme::ACCENT } else { theme::FG_3 };
    ui.painter().text(egui::pos2(rect.right() - 14.0, cy), Align2::RIGHT_CENTER, "SWITCH ↺", FontId::new(10.5, FontFamily::Monospace), color);
    switch.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn passphrase_field(ui: &mut egui::Ui, form: &mut UnlockForm, loading: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("passphrase").font(FontId::new(11.0, FontFamily::Monospace)).color(theme::FG_3));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let toggle = if form.show { "HIDE" } else { "SHOW" };
            if quiet_link(ui, toggle).clicked() {
                form.show = !form.show;
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("{} chars", form.passphrase.chars().count()))
                    .font(FontId::new(10.0, FontFamily::Monospace))
                    .color(theme::FG_4),
            );
        });
    });
    ui.add_space(6.0);

    let edit = egui::TextEdit::singleline(&mut form.passphrase)
        .password(!form.show)
        .desired_width(f32::INFINITY)
        .margin(Margin::symmetric(12, 10))
        .font(FontId::new(15.0, FontFamily::Monospace))
        .background_color(theme::BG_1);
    let resp = ui.add_enabled(!loading, edit);
    if !loading && ui.memory(|m| m.focused().is_none()) {
        resp.request_focus();
    }
    if resp.changed() {
        form.error = None; // typing clears the failed-attempt state
    }

    // Accent ring normally; red border + ring after a failed attempt.
    let (border, ring) = match form.error {
        Some(_) => (theme::ERR, theme::ERR_BG),
        None => (theme::ACCENT, theme::ACCENT_BG),
    };
    let p = ui.painter();
    p.rect_stroke(resp.rect.expand(3.0), CornerRadius::same(0), Stroke::new(3.0, ring), StrokeKind::Outside);
    p.rect_stroke(resp.rect, CornerRadius::same(0), Stroke::new(1.0, border), StrokeKind::Inside);

    ui.add_space(8.0);
    let (text, color) = match &form.error {
        Some(error) => (format!("! {error}"), theme::ERR),
        None => ("decryption happens on this device — never sent over the wire.".to_owned(), theme::FG_4),
    };
    ui.label(RichText::new(text).font(FontId::new(11.0, FontFamily::Monospace)).color(color));
}

// recover (import) and forget (delete keystore) need backend that isn't built yet; rendered
// per the design, wired in a later slice.
fn footer_links(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let _ = quiet_link(ui, "↺ RECOVER WITH PHRASE");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let _ = quiet_link(ui, "FORGET IDENTITY");
        });
    });
}

fn start_login(vault: &Vault, form: &mut UnlockForm) {
    if form.passphrase.is_empty() {
        form.error = Some("enter your passphrase.".into());
        return;
    }
    form.error = None;

    let dir = vault.dir().to_path_buf();
    let did = form.did.clone();
    let passphrase = form.passphrase.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut passphrase = passphrase;
        let result = Vault::prepare_login(&dir, &did, &passphrase);
        passphrase.zeroize();
        let _ = tx.send(result);
    });
    form.pending = Some(rx);
}

fn poll_login(form: &mut UnlockForm) -> Option<LoginResult> {
    match form.pending.as_ref()?.try_recv() {
        Ok(result) => {
            form.pending = None;
            Some(result)
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            form.pending = None;
            form.error = Some("unlock failed unexpectedly.".into());
            None
        }
    }
}
