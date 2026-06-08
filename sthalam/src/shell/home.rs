use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Sense, Stroke, StrokeKind};
use vault::{Vault, WorkspaceMeta};

use crate::components::identicon;
use crate::screens::common::short_did;
use crate::theme;
use super::Action;
use super::atoms::{column, elide, short_id};

pub(super) fn body(
    ui: &mut egui::Ui,
    vault: &Vault,
    search: &mut String,
    creating: &mut Option<String>,
    workspaces: &[WorkspaceMeta],
    user_menu: bool,
    action: &mut Option<Action>,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        column(ui, 1100.0, |ui| {
            ui.add_space(40.0);

            ui.horizontal(|ui| {
                let search_w = ui.available_width() - 220.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(search_w.max(200.0), 38.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| search_field(ui, search),
                );
                ui.add_space(14.0);
                if user_chip(ui, vault, user_menu, action).clicked() {
                    *action = Some(Action::ToggleUserMenu);
                }
            });

            ui.add_space(28.0);

            section_head(ui, "workspaces", &format!("{} total", workspaces.len()), "+ NEW WORKSPACE", action);
            ui.add_space(14.0);

            if workspaces.is_empty() && creating.is_none() {
                empty_state(ui, action);
            } else {
                workspaces_grid(ui, creating, workspaces, action);
            }
            ui.add_space(56.0);
        });
    });
}

fn workspaces_grid(ui: &mut egui::Ui, creating: &mut Option<String>, workspaces: &[WorkspaceMeta], action: &mut Option<Action>) {
    const COLS: usize = 4;
    const GAP: f32 = 12.0;
    let card_w = ((ui.available_width() - GAP * (COLS as f32 - 1.0)) / COLS as f32).floor().max(120.0);

    egui::Grid::new("workspaces_grid")
        .num_columns(COLS)
        .min_col_width(card_w)
        .max_col_width(card_w)
        .spacing([GAP, GAP])
        .show(ui, |ui| {
            let mut placed = 0usize;

            if let Some(buf) = creating {
                create_card(ui, card_w, buf, action);
                placed += 1;
                if placed.is_multiple_of(COLS) { ui.end_row(); }
            }

            for (i, w) in workspaces.iter().enumerate() {
                if workspace_card(ui, card_w, w).clicked() {
                    *action = Some(Action::OpenWorkspace(i));
                }
                placed += 1;
                if placed.is_multiple_of(COLS) { ui.end_row(); }
            }
        });
}

fn search_field(ui: &mut egui::Ui, search: &mut String) {
    let resp = ui.add(
        egui::TextEdit::singleline(search)
            .hint_text("go to a workspace, page, app, doc, or peer…")
            .desired_width(f32::INFINITY)
            .margin(Margin::symmetric(14, 10))
            .font(FontId::new(13.0, FontFamily::Proportional))
            .background_color(theme::BG_1),
    );
    ui.painter().rect_stroke(resp.rect, CornerRadius::same(0), Stroke::new(1.0, theme::BD_2), StrokeKind::Inside);
}

fn user_chip(ui: &mut egui::Ui, vault: &Vault, open: bool, action: &mut Option<Action>) -> egui::Response {
    let account = vault.current();
    let did = account.as_ref().map(|a| a.did.clone()).unwrap_or_default();
    let label = account.as_ref().map(|a| a.label.clone()).unwrap_or_else(|| "identity".into());

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(206.0, 38.0), Sense::click());
    let p = ui.painter();
    if open || resp.hovered() { p.rect_filled(rect, 0.0, theme::BG_2); }
    p.rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, if open { theme::BD_3 } else { theme::BD_2 }), StrokeKind::Inside);
    let icon = 20.0;
    identicon(p, egui::pos2(rect.left() + 9.0, rect.center().y - icon / 2.0), icon, &did);
    p.text(egui::pos2(rect.left() + 9.0 + icon + 10.0, rect.center().y), Align2::LEFT_CENTER, &label, FontId::new(13.0, FontFamily::Proportional), theme::FG_1);
    p.text(egui::pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, if open { "▴" } else { "▾" }, FontId::new(10.0, FontFamily::Monospace), theme::FG_4);

    if open {
        let menu_rect = user_menu(ui, rect, &did, action);
        if action.is_none() {
            let outside = ui.input(|i| i.pointer.any_pressed())
                && ui.input(|i| i.pointer.interact_pos()).is_none_or(|p| !rect.contains(p) && !menu_rect.contains(p));
            if outside { *action = Some(Action::ToggleUserMenu); }
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn user_menu(ui: &mut egui::Ui, chip: Rect, did: &str, action: &mut Option<Action>) -> Rect {
    let area = egui::Area::new(ui.id().with("user_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(chip.right() - 206.0, chip.bottom() + 4.0));
    let inner = area.show(ui.ctx(), |ui| {
        egui::Frame::default()
            .fill(theme::BG_2)
            .stroke(Stroke::new(1.0, theme::BD_2))
            .inner_margin(Margin::symmetric(0, 6))
            .show(ui, |ui| {
                ui.set_width(206.0);
                ui.add_space(2.0);
                menu_label(ui, &short_did(did));
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                if menu_item(ui, "settings", "⌘,", false).clicked() {}
                if menu_item(ui, "switch identity", "⌘⇧I", false).clicked() {
                    *action = Some(Action::Logout);
                }
                if menu_item(ui, "log out", "", true).clicked() {
                    *action = Some(Action::Logout);
                }
            });
    });
    inner.response.rect
}

fn menu_label(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        ui.label(egui::RichText::new(text).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_4));
    });
}

fn menu_item(ui: &mut egui::Ui, label: &str, key: &str, danger: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), Sense::click());
    if resp.hovered() { ui.painter().rect_filled(rect, 0.0, theme::BG_3); }
    let color = if danger { theme::ERR } else { theme::FG_1 };
    ui.painter().text(egui::pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::new(13.0, FontFamily::Proportional), color);
    if !key.is_empty() {
        ui.painter().text(egui::pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, key, FontId::new(10.5, FontFamily::Monospace), theme::FG_4);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn section_head(ui: &mut egui::Ui, title: &str, hint: &str, action_label: &str, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_4).extra_letter_spacing(1.8));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(hint).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui
                .add(egui::Label::new(egui::RichText::new(action_label).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3).extra_letter_spacing(1.0)).selectable(false).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.hovered() {
                let r = resp.rect;
                ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(1.0, theme::FG_3));
            }
            if resp.clicked() { *action = Some(Action::StartCreate); }
        });
    });
}

const CARD_H: f32 = 124.0;

fn workspace_card(ui: &mut egui::Ui, w: f32, meta: &WorkspaceMeta) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, CARD_H), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if resp.hovered() { theme::BG_2 } else { theme::BG_1 });
    p.rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, if resp.hovered() { theme::BD_2 } else { theme::BD_1 }), StrokeKind::Inside);

    let tint = theme::tint(&meta.id);
    let block = Rect::from_min_size(rect.min + egui::vec2(16.0, 16.0), egui::vec2(28.0, 28.0));
    p.rect_filled(block.translate(egui::vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(110));
    p.rect_filled(block, 0.0, tint);

    p.text(egui::pos2(rect.left() + 16.0, rect.top() + 60.0), Align2::LEFT_TOP, elide(&meta.name, 22), FontId::new(16.0, FontFamily::Proportional), theme::FG_1);
    p.text(egui::pos2(rect.left() + 16.0, rect.bottom() - 18.0), Align2::LEFT_CENTER, short_id(&meta.id), FontId::new(10.5, FontFamily::Monospace), theme::FG_4);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn create_card(ui: &mut egui::Ui, w: f32, buf: &mut String, action: &mut Option<Action>) {
    egui::Frame::default()
        .fill(theme::BG_2)
        .stroke(Stroke::new(1.0, theme::ACCENT))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(w - 28.0);
            ui.label(egui::RichText::new("NEW WORKSPACE").font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3).extra_letter_spacing(1.4));
            ui.add_space(8.0);
            let edit = ui.add(
                egui::TextEdit::singleline(buf)
                    .hint_text("name")
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8))
                    .font(FontId::new(13.0, FontFamily::Proportional))
                    .background_color(theme::BG_1),
            );
            if ui.memory(|m| m.focused().is_none()) { edit.request_focus(); }
            let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(egui::RichText::new("create").font(FontId::new(12.0, FontFamily::Proportional)).color(Color32::WHITE)).fill(theme::ACCENT)).clicked() || submit {
                    *action = Some(Action::CommitCreate);
                }
                if ui.add(egui::Button::new(egui::RichText::new("cancel").font(FontId::new(12.0, FontFamily::Proportional)).color(theme::FG_3)).fill(theme::BG_3)).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    *action = Some(Action::CancelCreate);
                }
            });
        });
}

fn empty_state(ui: &mut egui::Ui, action: &mut Option<Action>) {
    egui::Frame::default()
        .fill(theme::BG_1)
        .stroke(Stroke::new(1.0, theme::BD_1))
        .inner_margin(Margin::symmetric(24, 40))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("no workspaces yet").font(FontId::new(15.0, FontFamily::Proportional)).color(theme::FG_2));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("a workspace is a space you create, fill with pages, and share.").font(FontId::new(12.0, FontFamily::Proportional)).color(theme::FG_3));
                ui.add_space(18.0);
                if ui.add(egui::Button::new(egui::RichText::new("+  new workspace").font(FontId::new(13.0, theme::mono_sb())).color(Color32::WHITE)).fill(theme::ACCENT).min_size(egui::vec2(190.0, 40.0))).clicked() {
                    *action = Some(Action::StartCreate);
                }
            });
        });
}
