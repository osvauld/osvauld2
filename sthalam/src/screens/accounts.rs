// The login picker: the device's identities as a vertical list (identicon · name · DID),
// the selected row carrying the accent left-rule. ↑↓ move the selection, ↩ or a click opens
// that identity's unlock screen. add-account and recover sit as quiet links underneath.

use eframe::egui::{self, Align2, CornerRadius, FontFamily, FontId, RichText, Sense, Stroke, StrokeKind};
use vault::AccountInfo;

use super::common::{quiet_link, short_did};
use crate::app::{AccountsView, Screen, SignupForm, UnlockForm};
use crate::components::{brand, identicon, Backdrop};
use crate::theme;

const PANEL_W: f32 = 460.0;

pub fn accounts(ui: &mut egui::Ui, view: &mut AccountsView, backdrop: &mut Backdrop) -> Option<Screen> {
    let rect = ui.max_rect();
    backdrop.show(ui, rect);
    brand::corner_tags(ui, rect, "02 / login", "↑↓ SELECT  ·  ↩ UNLOCK");

    let n = view.accounts.len();
    move_selection(ui, view, n);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));

    let mut next = None;
    let top = ((rect.height() - panel_height(n)) * 0.5).max(16.0);
    ui.vertical_centered(|ui| {
        ui.add_space(top);
        ui.allocate_ui_with_layout(
            egui::vec2(PANEL_W, panel_height(n)),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(PANEL_W);
                brand::wordmark(ui, 64.0);
                ui.add_space(26.0);

                for (i, account) in view.accounts.iter().enumerate() {
                    if account_row(ui, account, i == view.selected).clicked() {
                        next = Some(unlock_for(account));
                    }
                    ui.add_space(8.0);
                }

                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        if quiet_link(ui, "+ ADD ACCOUNT").clicked() {
                            next = Some(Screen::Signup(SignupForm { from_accounts: true, ..Default::default() }));
                        }
                        ui.label(RichText::new("·").font(FontId::new(11.0, FontFamily::Monospace)).color(theme::FG_4));
                        // Recover needs the import screen + vault::import (deferred); shown per the design.
                        let _ = quiet_link(ui, "↺ RECOVER WITH PHRASE");
                    });
                });
            },
        );
    });

    if enter && n > 0 && next.is_none() {
        next = Some(unlock_for(&view.accounts[view.selected]));
    }
    next
}

fn move_selection(ui: &egui::Ui, view: &mut AccountsView, n: usize) {
    if n == 0 {
        return;
    }
    ui.input(|i| {
        if i.key_pressed(egui::Key::ArrowDown) {
            view.selected = (view.selected + 1) % n;
        }
        if i.key_pressed(egui::Key::ArrowUp) {
            view.selected = (view.selected + n - 1) % n;
        }
    });
}

fn account_row(ui: &mut egui::Ui, account: &AccountInfo, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), Sense::click());
    let active = selected || resp.hovered();

    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, theme::ACCENT_BG);
    }
    let edge = if active { theme::ACCENT } else { theme::BD_1 };
    p.rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, edge), StrokeKind::Inside);
    if selected {
        p.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())), 0.0, theme::ACCENT);
    }

    let (icon, pad) = (40.0, 16.0);
    identicon(p, egui::pos2(rect.left() + pad, rect.center().y - icon / 2.0), icon, &account.did);
    let tx = rect.left() + pad + icon + 16.0;
    let cy = rect.center().y;
    p.text(egui::pos2(tx, cy - 9.0), Align2::LEFT_CENTER, account.label.as_str(), FontId::new(15.0, FontFamily::Proportional), theme::FG_1);
    p.text(egui::pos2(tx, cy + 10.0), Align2::LEFT_CENTER, short_did(&account.did), FontId::new(11.0, FontFamily::Monospace), theme::FG_4);
    p.text(egui::pos2(rect.right() - 16.0, cy), Align2::RIGHT_CENTER, "▸", FontId::new(14.0, FontFamily::Monospace), if active { theme::ACCENT } else { theme::FG_4 });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn unlock_for(account: &AccountInfo) -> Screen {
    Screen::Unlock(UnlockForm {
        did: account.did.clone(),
        label: account.label.clone(),
        ..Default::default()
    })
}

// Approximate stack height (wordmark + gaps + rows + links), used only to center vertically.
fn panel_height(n: usize) -> f32 {
    100.0 + n as f32 * 72.0
}
