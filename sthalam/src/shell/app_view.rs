//! The .app tab: two views over one item.
//!
//! - **run** — the engine renders the app's Lua UI; the user interacts with it.
//! - **code** — a file-tree sidebar + a syntax-highlighted view of the selected source file.
//!
//! Source files live in the vault's path-addressed file tree (written by an agent over MCP);
//! the runtime state is the item's separate CRDT, persisted back whenever `run` mutates it.

use app_host::App;
use code_highlight::HlKind;
use eframe::egui::{self, Color32, FontFamily, FontId};
use vault::{Vault, WorkspaceItem};

use crate::theme;
use super::atoms::hairline_bottom;

/// Which of the .app tab's two views is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewMode {
    Run,
    Code,
}

/// The retained state of one open .app tab.
pub(super) struct AppTab {
    engine: App,
    /// The app's source tree as `(path, source)` pairs, sorted by path — the engine's input and
    /// the code view's content.
    files: Vec<(String, String)>,
    mode: ViewMode,
    /// Index into `files` of the file shown in the code view.
    selected: usize,
}

impl AppTab {
    /// Load an app from the vault: read its whole source tree and runtime state, then build the
    /// engine. Defaults to the run view.
    pub(super) fn load(vault: &Vault, item: &WorkspaceItem) -> Self {
        let files = read_files(vault, item);
        let state = vault.get_state(&item.ws_id, &item.id).ok().flatten();
        let engine = App::from_files(&files, state.as_deref());
        AppTab { engine, files, mode: ViewMode::Run, selected: 0 }
    }

    /// Re-read the source tree and rebuild the engine (after an external write, e.g. MCP). The
    /// runtime state carries over from the vault; the current view and selection are preserved.
    pub(super) fn reload(&mut self, vault: &Vault, item: &WorkspaceItem) {
        self.files = read_files(vault, item);
        let state = vault.get_state(&item.ws_id, &item.id).ok().flatten();
        self.engine = App::from_files(&self.files, state.as_deref());
        self.selected = self.selected.min(self.files.len().saturating_sub(1));
    }
}

/// Read every source file of `item` into `(path, source)` pairs, sorted by path.
fn read_files(vault: &Vault, item: &WorkspaceItem) -> Vec<(String, String)> {
    vault
        .list_files(&item.ws_id, &item.id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| {
            let bytes = vault.get_file(&item.ws_id, &item.id, &path).ok().flatten()?;
            Some((path, String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect()
}

/// Render the .app tab. Returns a runtime-CRDT snapshot if the app's state changed this frame
/// (run view only), so the caller can persist it to the vault.
pub(super) fn body(ui: &mut egui::Ui, item: &WorkspaceItem, tab: &mut AppTab) -> Option<Vec<u8>> {
    egui::Panel::top(egui::Id::new(("app_context", &item.id)))
        .exact_size(30.0)
        .frame(egui::Frame::default().fill(theme::BG_1))
        .show_inside(ui, |ui| {
            hairline_bottom(ui);
            ui.horizontal_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(".app").font(FontId::new(10.0, FontFamily::Monospace)).color(theme::FG_4));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&item.name).font(FontId::new(11.5, FontFamily::Monospace)).color(theme::FG_2));

                // Right-aligned run / code toggle.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    mode_toggle(ui, "code", ViewMode::Code, tab);
                    ui.add_space(4.0);
                    mode_toggle(ui, "run", ViewMode::Run, tab);
                });
            });
        });

    let mut crdt = None;
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE))
        .show_inside(ui, |ui| match tab.mode {
            ViewMode::Run => crdt = tab.engine.show(ui),
            ViewMode::Code => code_view(ui, item, tab),
        });
    crdt
}

/// One pill in the run/code toggle; switches the tab's view when clicked.
fn mode_toggle(ui: &mut egui::Ui, label: &str, mode: ViewMode, tab: &mut AppTab) {
    let on = tab.mode == mode;
    let color = if on { theme::ACCENT_SOFT } else { theme::FG_3 };
    let text = egui::RichText::new(label).font(FontId::new(10.5, FontFamily::Monospace)).color(color);
    if ui.add(egui::Label::new(text).sense(egui::Sense::click())).clicked() {
        tab.mode = mode;
    }
}

/// The code view: a file-tree sidebar + the selected file's highlighted source.
fn code_view(ui: &mut egui::Ui, item: &WorkspaceItem, tab: &mut AppTab) {
    if tab.files.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("no source files").font(FontId::new(12.0, FontFamily::Monospace)).color(theme::FG_4));
        });
        return;
    }

    egui::Panel::left(egui::Id::new(("app_files", &item.id)))
        .exact_size(180.0)
        .frame(egui::Frame::default().fill(theme::BG_1).inner_margin(egui::Margin::symmetric(8, 8)))
        .show_inside(ui, |ui| {
            hairline_right(ui);
            for (i, (path, _)) in tab.files.iter().enumerate() {
                let on = i == tab.selected;
                let color = if on { theme::FG_1 } else { theme::FG_3 };
                let text = egui::RichText::new(path).font(FontId::new(11.5, FontFamily::Monospace)).color(color);
                let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()).truncate());
                if resp.clicked() {
                    tab.selected = i;
                }
            }
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE).inner_margin(egui::Margin::same(16)))
        .show_inside(ui, |ui| {
            let (path, source) = &tab.files[tab.selected];
            let lang = lang_for(path);
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                ui.add(egui::Label::new(code_job(ui, source, lang)).selectable(true));
            });
        });
}

/// Build a monospace, syntax-highlighted [`egui::text::LayoutJob`] for `source`. Falls back to
/// one plain run when the language is unknown or highlighting fails.
fn code_job(ui: &egui::Ui, source: &str, lang: Option<&str>) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let font = FontId::new(12.5, FontFamily::Monospace);
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY; // code scrolls horizontally, never wraps
    let _ = ui;

    let spans = lang.and_then(|l| code_highlight::highlight(l, source));
    match spans {
        Some(spans) if !spans.is_empty() => {
            for span in spans {
                let fmt = TextFormat { font_id: font.clone(), color: code_color(span.kind), ..Default::default() };
                job.append(&source[span.range], 0.0, fmt);
            }
        }
        _ => {
            let fmt = TextFormat { font_id: font, color: theme::FG_2, ..Default::default() };
            job.append(source, 0.0, fmt);
        }
    }
    job
}

/// Map a highlight kind to a colour on the dark page (mirrors doc_editor's code palette).
fn code_color(kind: HlKind) -> Color32 {
    match kind {
        HlKind::Keyword => Color32::from_rgb(0xC4, 0xA7, 0xF7),
        HlKind::Function => Color32::from_rgb(0x82, 0xAA, 0xFF),
        HlKind::Type => Color32::from_rgb(0x7F, 0xD1, 0xC0),
        HlKind::Constant | HlKind::Attribute => Color32::from_rgb(0xFF, 0xCB, 0x6B),
        HlKind::Number => Color32::from_rgb(0xFF, 0x9E, 0x64),
        HlKind::String => Color32::from_rgb(0x9E, 0xCE, 0x6A),
        HlKind::Comment => Color32::from_rgb(0x6B, 0x6D, 0x7E),
        HlKind::Property => Color32::from_rgb(0x89, 0xDD, 0xFF),
        HlKind::Operator => Color32::from_rgb(0xC0, 0xCA, 0xF5),
        HlKind::Tag | HlKind::Escape => Color32::from_rgb(0xF7, 0x76, 0x8E),
        HlKind::Variable | HlKind::Text => theme::FG_2,
        HlKind::Punctuation => theme::FG_3,
    }
}

/// The highlighter token for a file path, by extension (`main.lua` → `"lua"`). `None` for files
/// with no bundled grammar (rendered plain).
fn lang_for(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    let token = match ext {
        "lua" => "lua",
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "go" => "go",
        "c" | "h" => "c",
        "json" => "json",
        "toml" => "toml",
        "sh" | "bash" => "bash",
        _ => return None,
    };
    Some(token)
}

/// A 1px hairline down the right edge of the current panel (the sidebar's divider).
fn hairline_right(ui: &egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().vline(rect.right(), rect.y_range(), egui::Stroke::new(1.0, theme::BD_2));
}
