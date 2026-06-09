use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Sense, Stroke, StrokeKind};
use vault::{ItemKind, WorkspaceItem};

use crate::theme;
use super::Action;
use super::atoms::{column, elide};

pub(super) fn body(ui: &mut egui::Ui, ws_id: &str, ws_name: &str, items: &[WorkspaceItem], action: &mut Option<Action>) {
    egui::Panel::top(egui::Id::new(("ws_context", ws_id)))
        .exact_size(30.0)
        .frame(egui::Frame::default().fill(theme::BG_1))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);
                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(dot, 0.0, theme::tint(ws_id));
                ui.add_space(10.0);
                ui.label(egui::RichText::new(ws_name).font(FontId::new(11.5, FontFamily::Monospace)).color(theme::FG_2).extra_letter_spacing(0.8));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    for label in ["···", "SCOPE", "SHARE"] {
                        ui.label(egui::RichText::new(label).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3).extra_letter_spacing(1.6));
                        ui.add_space(12.0);
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE))
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                column(ui, 1100.0, |ui| {
                    ui.add_space(32.0);
                    section_head(ui, ws_id, items.len(), action);
                    ui.add_space(14.0);
                    if items.is_empty() {
                        empty_state(ui, ws_id, action);
                    } else {
                        items_grid(ui, items, action);
                    }
                    ui.add_space(40.0);
                });
            });
        });
}

fn section_head(ui: &mut egui::Ui, ws_id: &str, count: usize, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("items").font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_4).extra_letter_spacing(1.8));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("{count} total")).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui
                .add(egui::Label::new(egui::RichText::new("+ NEW .DOC").font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_3).extra_letter_spacing(1.0)).selectable(false).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.hovered() {
                let r = resp.rect;
                ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(1.0, theme::FG_3));
            }
            if resp.clicked() {
                *action = Some(Action::CreateItem { ws_id: ws_id.to_string(), kind: ItemKind::Doc });
            }
        });
    });
}

fn empty_state(ui: &mut egui::Ui, ws_id: &str, action: &mut Option<Action>) {
    egui::Frame::default()
        .fill(theme::BG_1)
        .stroke(Stroke::new(1.0, theme::BD_1))
        .inner_margin(Margin::symmetric(24, 40))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("no items yet").font(FontId::new(15.0, FontFamily::Proportional)).color(theme::FG_2));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("create a .doc, .table, or .app to get started.").font(FontId::new(12.0, FontFamily::Proportional)).color(theme::FG_3));
                ui.add_space(18.0);
                if ui.add(egui::Button::new(egui::RichText::new("+  new .doc").font(FontId::new(13.0, theme::mono_sb())).color(Color32::WHITE)).fill(theme::ACCENT).min_size(egui::vec2(160.0, 40.0))).clicked() {
                    *action = Some(Action::CreateItem { ws_id: ws_id.to_string(), kind: ItemKind::Doc });
                }
            });
        });
}

const COLS: usize = 4;
const GAP: f32 = 12.0;
const CARD_H: f32 = 100.0;

fn items_grid(ui: &mut egui::Ui, items: &[WorkspaceItem], action: &mut Option<Action>) {
    let card_w = ((ui.available_width() - GAP * (COLS as f32 - 1.0)) / COLS as f32).floor().max(100.0);
    egui::Grid::new("items_grid")
        .num_columns(COLS)
        .min_col_width(card_w)
        .max_col_width(card_w)
        .spacing([GAP, GAP])
        .show(ui, |ui| {
            for (i, item) in items.iter().enumerate() {
                if item_card(ui, card_w, item).clicked() {
                    *action = Some(Action::OpenItem(item.clone()));
                }
                if (i + 1).is_multiple_of(COLS) {
                    ui.end_row();
                }
            }
        });
}

fn item_card(ui: &mut egui::Ui, w: f32, item: &WorkspaceItem) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, CARD_H), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if resp.hovered() { theme::BG_2 } else { theme::BG_1 });
    p.rect_stroke(rect, CornerRadius::same(0), Stroke::new(1.0, if resp.hovered() { theme::BD_2 } else { theme::BD_1 }), StrokeKind::Inside);

    let kind_badge = format!(".{}", item.kind.as_str());
    p.text(egui::pos2(rect.left() + 12.0, rect.top() + 14.0), Align2::LEFT_TOP, &kind_badge, FontId::new(10.0, FontFamily::Monospace), theme::FG_4);
    p.text(egui::pos2(rect.left() + 12.0, rect.top() + 32.0), Align2::LEFT_TOP, elide(&item.name, 22), FontId::new(14.0, FontFamily::Proportional), theme::FG_1);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
