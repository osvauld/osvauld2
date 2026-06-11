//! The .app tab: a VS-Code-like host over one item.
//!
//! A file explorer on the left; a dock of open panes in the centre. A pane is either a **file**
//! (`.lua` → its own block editor with its own caret; anything else → read-only highlighted source)
//! or the **run** preview (the engine renders the app's Lua UI, which the user interacts with).
//!
//! Source files live in the vault's path-addressed file tree (written by an agent over MCP, or
//! edited here in the block editor — flushed back to storage on blur/close, preserving block
//! identity); the runtime state is the item's separate CRDT, persisted whenever `run` mutates it.

use std::collections::HashMap;

use app_host::App;
use code_highlight::HlKind;
use eframe::egui::{self, Color32, FontFamily, FontId};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, TabViewer};
use vault::{Vault, WorkspaceItem};

use crate::theme;
use super::atoms::hairline_bottom;

/// What the .app tab wants persisted this frame.
#[derive(Default)]
pub(super) struct AppOutput {
    /// Runtime-CRDT snapshot to `put_state` (the run pane mutated the app's state).
    pub runtime: Option<Vec<u8>>,
    /// Source files to `put_file` as `(path, block-snapshot bytes)` — the editors flushed this frame
    /// (blur / close).
    pub files: Vec<(String, Vec<u8>)>,
}

/// An open pane in the dock: a source file, or the run preview.
enum Pane {
    File(FileTab),
    Run,
}

/// One open file. `.lua` files carry a live block doc + editor; any other file has `doc: None` and
/// renders read-only (its source comes from the tab's `files` cache).
struct FileTab {
    path: String,
    doc: Option<code_editor::BlockDoc>,
    editor: code_editor::Editor,
}

/// The retained state of one open .app tab.
pub(super) struct AppTab {
    engine: App,
    /// The app's source tree as `(path, source)` pairs, sorted by path — the engine's input and the
    /// read-only view's content. Kept in sync with the editors' flushes.
    files: Vec<(String, String)>,
    /// Raw stored bytes per path (the on-disk form `get_file` returned). For a `.lua` file this is a
    /// block snapshot, loaded *directly* into the editor's doc so block identity is preserved
    /// (re-splitting the emitted source would mint fresh IDs).
    stored: HashMap<String, Vec<u8>>,
    /// Open panes (files + at most one run preview). Only the active pane runs each frame.
    dock: DockState<Pane>,
}

impl AppTab {
    /// Load an app: read its whole source tree and runtime state, build the engine, and open the
    /// entry file as the first tab.
    pub(super) fn load(vault: &Vault, item: &WorkspaceItem) -> Self {
        let (files, stored) = read_tree(vault, item);
        let state = vault.get_state(&item.ws_id, &item.id).ok().flatten();
        let engine = App::from_files(&files, state.as_deref());
        let dock = build_dock(&default_paths(&files), &files, &stored, false);
        AppTab { engine, files, stored, dock }
    }

    /// Re-read the source tree and rebuild the engine (after a whole-file external write, e.g. MCP
    /// `write_file`). Re-opens whichever panes were open so the user's tab strip survives; carets are
    /// lost (this is a hard reload, unlike `merge_lua`).
    pub(super) fn reload(&mut self, vault: &Vault, item: &WorkspaceItem) {
        let open: Vec<String> = file_paths(&self.dock);
        let run = self.dock.iter_all_tabs().any(|(_, p)| matches!(p, Pane::Run));
        let (files, stored) = read_tree(vault, item);
        self.files = files;
        self.stored = stored;
        let state = vault.get_state(&item.ws_id, &item.id).ok().flatten();
        self.engine = App::from_files(&self.files, state.as_deref());
        self.dock = build_dock(&open, &self.files, &self.stored, run);
    }

    /// Merge a per-block MCP edit (`snapshot`) to `path` into this tab, surgically — *not* a reload,
    /// so a human's live edits + caret survive. If a tab for `path` is open, the agent's delta is
    /// imported into that live doc (both descend from the same stored snapshot, so the merge is
    /// clean); otherwise only the caches update and it loads fresh on the next open. Returns the
    /// union snapshot to persist when a live doc absorbed the delta, else `None`.
    pub(super) fn merge_lua(&mut self, path: &str, snapshot: &[u8]) -> Option<Vec<u8>> {
        let live = self.dock.iter_all_tabs_mut().find_map(|(_, p)| match p {
            Pane::File(ft) if ft.path == path => ft.doc.as_ref(),
            _ => None,
        });
        let (source, union, persist) = match live {
            Some(doc) => {
                let _ = doc.import(snapshot);
                let union = code_editor::store::snapshot_from_doc(doc);
                (code_editor::store::source_from_doc(doc), union.clone(), Some(union))
            }
            None => (code_editor::store::decode(path, snapshot), snapshot.to_vec(), None),
        };
        self.stored.insert(path.to_string(), union);
        if let Some(f) = self.files.iter_mut().find(|(p, _)| p == path) {
            f.1 = source;
        }
        self.engine.reload_source(&self.files); // run preview reflects the agent's edit
        persist
    }

    /// Merge an external runtime-CRDT write (an MCP app-data edit) into the live engine, returning
    /// the union snapshot to persist (the live doc may hold edits the writer didn't see).
    pub(super) fn import_runtime(&mut self, snapshot: &[u8]) -> Option<Vec<u8>> {
        if self.engine.import_state(snapshot).is_err() {
            return None;
        }
        self.engine.export_state()
    }

    /// Flush every open editor, rebuild the engine from the edited source, and show the run pane.
    /// The explicit "refresh and run" action — returns the `(path, snapshot)`s to persist.
    fn refresh(&mut self) -> Vec<(String, Vec<u8>)> {
        let mut saved = Vec::new();
        for (_, pane) in self.dock.iter_all_tabs_mut() {
            if let Pane::File(ft) = pane {
                if let Some(f) = flush_tab(ft, &mut self.files, &mut self.stored) {
                    saved.push(f);
                }
            }
        }
        self.engine.reload_source(&self.files);
        open_run(&mut self.dock);
        saved
    }
}

/// Read every source file of `item` into sorted `(path, source)` pairs plus a `path → raw bytes`
/// map. The source feeds the engine and the read-only view; the raw bytes feed the editor (which
/// needs the snapshot, not re-split source, to keep block identity).
fn read_tree(vault: &Vault, item: &WorkspaceItem) -> (Vec<(String, String)>, HashMap<String, Vec<u8>>) {
    let mut files = Vec::new();
    let mut stored = HashMap::new();
    for path in vault.list_files(&item.ws_id, &item.id).unwrap_or_default() {
        let Some(bytes) = vault.get_file(&item.ws_id, &item.id, &path).ok().flatten() else { continue };
        // `.lua` files are block snapshots; `decode` emits them back to source for the engine.
        let source = code_editor::store::decode(&path, &bytes);
        stored.insert(path.clone(), bytes);
        files.push((path, source));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    (files, stored)
}

/// The file(s) to open on a fresh load: the entry file (`*main.lua`) if present, else the first.
fn default_paths(files: &[(String, String)]) -> Vec<String> {
    let entry = files.iter().find(|(p, _)| p.ends_with("main.lua")).or_else(|| files.first());
    entry.map(|(p, _)| p.clone()).into_iter().collect()
}

/// The paths of the open file panes, in order.
fn file_paths(dock: &DockState<Pane>) -> Vec<String> {
    dock.iter_all_tabs()
        .filter_map(|(_, p)| match p {
            Pane::File(ft) => Some(ft.path.clone()),
            Pane::Run => None,
        })
        .collect()
}

/// Build a dock opening `paths` (those that still exist, in order) as file panes, plus the run
/// preview if `run`.
fn build_dock(paths: &[String], files: &[(String, String)], stored: &HashMap<String, Vec<u8>>, run: bool) -> DockState<Pane> {
    let mut panes: Vec<Pane> = paths
        .iter()
        .filter(|p| files.iter().any(|(fp, _)| fp == *p))
        .map(|p| Pane::File(make_file_tab(p, files, stored)))
        .collect();
    if run {
        panes.push(Pane::Run);
    }
    DockState::new(panes)
}

/// Build a [`FileTab`] for `path`. `.lua` files get a live block doc loaded from their snapshot
/// (identity-preserving; a re-split of the emitted source would mint fresh IDs).
fn make_file_tab(path: &str, files: &[(String, String)], stored: &HashMap<String, Vec<u8>>) -> FileTab {
    let doc = path.ends_with(".lua").then(|| {
        stored
            .get(path)
            .and_then(|bytes| code_editor::store::doc_from_bytes(path, bytes))
            .or_else(|| files.iter().find(|(p, _)| p == path).map(|(_, s)| code_editor::store::doc_from_source(s)))
    });
    FileTab { path: path.to_string(), doc: doc.flatten(), editor: code_editor::Editor::new() }
}

/// Open `path` as a file pane (or focus it if already open) and make it active.
fn open_file(dock: &mut DockState<Pane>, files: &[(String, String)], stored: &HashMap<String, Vec<u8>>, path: &str) {
    let exists = dock.iter_all_tabs().any(|(_, p)| matches!(p, Pane::File(ft) if ft.path == path));
    if !exists {
        dock.push_to_focused_leaf(Pane::File(make_file_tab(path, files, stored)));
    }
    let tp = dock.iter_all_tabs().find_map(|(tp, p)| matches!(p, Pane::File(ft) if ft.path == path).then_some(tp));
    if let Some(tp) = tp {
        let _ = dock.set_active_tab(tp);
    }
}

/// Open the run preview pane (or focus it if already open) and make it active.
fn open_run(dock: &mut DockState<Pane>) {
    if !dock.iter_all_tabs().any(|(_, p)| matches!(p, Pane::Run)) {
        dock.push_to_focused_leaf(Pane::Run);
    }
    let tp = dock.iter_all_tabs().find_map(|(tp, p)| matches!(p, Pane::Run).then_some(tp));
    if let Some(tp) = tp {
        let _ = dock.set_active_tab(tp);
    }
}

/// Persist `ft`'s live doc back if it has unsaved edits, preserving block identity (we save the
/// *live* snapshot, never a re-split). Updates the source + raw caches so the run view and a later
/// re-open reflect the edit. `None` when nothing's editable or there are no unsaved edits.
fn flush_tab(
    ft: &mut FileTab,
    files: &mut [(String, String)],
    stored: &mut HashMap<String, Vec<u8>>,
) -> Option<(String, Vec<u8>)> {
    let doc = ft.doc.as_ref()?;
    if !ft.editor.take_dirty() {
        return None;
    }
    let snapshot = code_editor::store::snapshot_from_doc(doc);
    if let Some(f) = files.iter_mut().find(|(p, _)| *p == ft.path) {
        f.1 = code_editor::store::source_from_doc(doc);
    }
    stored.insert(ft.path.clone(), snapshot.clone());
    Some((ft.path.clone(), snapshot))
}

/// Render the .app tab. Returns what to persist this frame (runtime CRDT and/or edited source
/// files), so the caller can write it to the vault.
pub(super) fn body(ui: &mut egui::Ui, item: &WorkspaceItem, tab: &mut AppTab) -> AppOutput {
    let mut refreshed = None;
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

                // Right-aligned: run = flush every editor, rebuild the engine, show the run pane.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    let run = egui::RichText::new("▶ run").font(FontId::new(10.5, FontFamily::Monospace)).color(theme::ACCENT_SOFT);
                    if ui.add(egui::Label::new(run).sense(egui::Sense::click())).clicked() {
                        refreshed = Some(tab.refresh());
                    }
                });
            });
        });

    let mut out = AppOutput::default();
    if let Some(files) = refreshed {
        out.files = files;
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_PAGE))
        .show_inside(ui, |ui| {
            let (files, runtime) = code_view(ui, item, tab);
            out.files.extend(files);
            out.runtime = runtime;
        });
    out
}

/// The code view: a file explorer + a nested dock of open panes. Returns any `(path, snapshot)`
/// flushed this frame (a pane blurred or was closed) and a runtime-CRDT snapshot if the run pane
/// mutated state.
fn code_view(ui: &mut egui::Ui, item: &WorkspaceItem, tab: &mut AppTab) -> (Vec<(String, Vec<u8>)>, Option<Vec<u8>>) {
    if tab.files.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("no source files").font(FontId::new(12.0, FontFamily::Monospace)).color(theme::FG_4));
        });
        return (Vec::new(), None);
    }

    let AppTab { engine, files, stored, dock } = tab;
    let mut saved = Vec::new();
    let mut runtime = None;

    // The set of currently-open file paths, to highlight them in the explorer (borrow ends before
    // the explorer mutates `dock`).
    let open = file_paths(dock);

    let mut to_open = None;
    egui::Panel::left(egui::Id::new(("app_files", &item.id)))
        .exact_size(180.0)
        .frame(egui::Frame::default().fill(theme::BG_1).inner_margin(egui::Margin::symmetric(8, 8)))
        .show_inside(ui, |ui| {
            hairline_right(ui);
            for (path, _) in files.iter() {
                let is_open = open.iter().any(|p| p == path);
                let color = if is_open { theme::FG_1 } else { theme::FG_3 };
                let text = egui::RichText::new(path).font(FontId::new(11.5, FontFamily::Monospace)).color(color);
                let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()).truncate());
                if resp.clicked() {
                    to_open = Some(path.clone());
                }
            }
        });
    if let Some(path) = to_open {
        open_file(dock, files, stored, &path);
    }

    let ctheme = code_editor::Theme {
        bg: theme::BG_PAGE,
        gutter_bg: theme::BG_1,
        gutter_fg: theme::FG_4,
        fg: theme::FG_2,
        punct: theme::FG_3,
        rule: theme::BD_2,
        selection: theme::ACCENT_BG,
        caret: theme::ACCENT,
    };
    let mut viewer = CodeViewer {
        engine,
        item_name: &item.name,
        files,
        stored,
        saved: &mut saved,
        runtime: &mut runtime,
        theme: ctheme,
    };
    let style = super::dock_style(ui);
    DockArea::new(dock)
        .id(egui::Id::new(("app_dock", &item.id)))
        .style(style)
        .show_inside(ui, &mut viewer);

    (saved, runtime)
}

/// Draws the open panes. Borrows the tab's engine + source caches so it can render the run preview,
/// read-only files, and flush edited ones (on blur or close), pushing flushes into `saved` and the
/// run pane's mutated state into `runtime` for the host to persist.
struct CodeViewer<'a> {
    engine: &'a mut App,
    item_name: &'a str,
    files: &'a mut [(String, String)],
    stored: &'a mut HashMap<String, Vec<u8>>,
    saved: &'a mut Vec<(String, Vec<u8>)>,
    runtime: &'a mut Option<Vec<u8>>,
    theme: code_editor::Theme,
}

impl TabViewer for CodeViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, pane: &mut Pane) -> egui::WidgetText {
        let name = match pane {
            Pane::File(ft) => ft.path.rsplit('/').next().unwrap_or(&ft.path),
            Pane::Run => "▶ run",
        };
        egui::RichText::new(name).font(FontId::new(11.5, FontFamily::Monospace)).into()
    }

    fn id(&mut self, pane: &mut Pane) -> egui::Id {
        match pane {
            Pane::File(ft) => egui::Id::new(("app_file", &ft.path)),
            Pane::Run => egui::Id::new("app_run"),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane) {
        match pane {
            Pane::Run => {
                let out = match self.engine.page() {
                    Some(page) => {
                        export_bar(ui, self.engine, self.item_name);
                        page_preview(ui, page, self.engine)
                    }
                    None => self.engine.show(ui),
                };
                if let Some(crdt) = out {
                    *self.runtime = Some(crdt);
                }
            }
            // `.lua`: live block editor (own gutter + scroll, no margin), flushed on blur.
            Pane::File(ft) if ft.doc.is_some() => {
                let doc = ft.doc.as_ref().unwrap();
                let lost_focus = ft.editor.show(ui, doc, &self.theme);
                if lost_focus {
                    if let Some(f) = flush_tab(ft, self.files, self.stored) {
                        self.engine.reload_source(self.files); // run preview reflects the edit
                        self.saved.push(f);
                    }
                }
            }
            // Anything else: read-only highlighted source.
            Pane::File(ft) => {
                let source = self.files.iter().find(|(p, _)| *p == ft.path).map(|(_, s)| s.as_str()).unwrap_or("");
                let lang = lang_for(&ft.path);
                let job = code_job(ui, source, lang);
                egui::Frame::default().inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                    ui.add(egui::Label::new(job).selectable(true));
                });
            }
        }
    }

    fn on_close(&mut self, pane: &mut Pane) -> OnCloseResponse {
        if let Pane::File(ft) = pane {
            if let Some(f) = flush_tab(ft, self.files, self.stored) {
                self.engine.reload_source(self.files);
                self.saved.push(f);
            }
        }
        OnCloseResponse::Close
    }

    fn clear_background(&self, _tab: &Pane) -> bool {
        false
    }

    fn scroll_bars(&self, tab: &Pane) -> [bool; 2] {
        // File panes scroll through the dock body (the editor/label render straight into it); the run
        // pane manages its own layout, so don't wrap it in a scroll viewport.
        match tab {
            Pane::File(_) => [true, true],
            Pane::Run => [false, false],
        }
    }
}

/// Toolbar above the page preview: export the sheet as PDF (to ~/Downloads, else the temp dir),
/// showing the written path — or the error — beside the button.
fn export_bar(ui: &mut egui::Ui, engine: &mut App, name: &str) {
    let store_id = egui::Id::new(("pdf_export_msg", name));
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let label = egui::RichText::new("⤓ export pdf")
            .font(FontId::new(10.5, FontFamily::Monospace))
            .color(theme::ACCENT_SOFT);
        if ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked() {
            let fonts = app_host::FontBytes {
                regular: theme::FONT_SANS,
                bold: theme::FONT_SANS_SB,
                mono: theme::FONT_MONO,
            };
            let msg = match engine.export_pdf(fonts) {
                Ok(bytes) => {
                    let path = crate::bridge::pdf_path(name);
                    match std::fs::write(&path, &bytes) {
                        Ok(()) => path.display().to_string(),
                        Err(e) => format!("write failed: {e}"),
                    }
                }
                Err(e) => format!("export failed: {e}"),
            };
            ui.ctx().data_mut(|d| d.insert_temp(store_id, msg));
        }
        if let Some(msg) = ui.ctx().data(|d| d.get_temp::<String>(store_id)) {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(msg).font(FontId::new(10.5, FontFamily::Monospace)).color(theme::FG_4));
        }
    });
}

/// Print preview: the app laid out inside a fixed page-sized white sheet (1 px = 1 pt), centered
/// in a scrollable backdrop instead of filling the pane. The sheet is the print default surface;
/// the app paints on top of it.
fn page_preview(ui: &mut egui::Ui, page: app_host::PageSpec, engine: &mut App) -> Option<Vec<u8>> {
    const MARGIN: f32 = 28.0;
    let mut out = None;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let size = egui::vec2(page.width, page.height);
        let x_pad = ((ui.available_width() - size.x) * 0.5).max(MARGIN);
        ui.add_space(MARGIN);
        ui.horizontal(|ui| {
            ui.add_space(x_pad);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let shadow = egui::epaint::Shadow {
                offset: [0, 2],
                blur: 18,
                spread: 0,
                color: Color32::from_black_alpha(110),
            };
            ui.painter().add(shadow.as_shape(rect, egui::CornerRadius::same(0)));
            ui.painter().rect_filled(rect, egui::CornerRadius::same(0), Color32::WHITE);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            out = engine.show(&mut child);
        });
        ui.add_space(MARGIN);
    });
    out
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
