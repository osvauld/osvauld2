use app_host::App;
use doc_editor::{Doc, DocEditor};
use eframe::egui;
use vault::{ItemKind, Vault, WorkspaceItem, WorkspaceMeta};

use crate::app::Screen;
use crate::screens::common::back_to_accounts;
use crate::theme;

mod app_view;
mod atoms;
mod doc;
mod home;
mod tab_strip;
mod workspace;

pub struct Shell {
    tabs: Vec<Tab>,
    active: usize,
    // Home screen state.
    workspaces: Vec<WorkspaceMeta>,
    loaded: bool,
    search: String,
    user_menu: bool,
    creating: Option<String>,
}

enum Tab {
    Home,
    Workspace { meta: WorkspaceMeta, items: Vec<WorkspaceItem>, loaded: bool },
    /// Any opened item (.doc, .app, .table, …) — kind lives in `item.kind`, editor state in `body`.
    Open { item: WorkspaceItem, body: TabBody },
}

enum TabBody {
    Doc { doc: Doc, editor: DocEditor },
    App { engine: App },
    // Table { ... }  — future
}

/// Shell-level intents collected during a frame and applied after the immediate-mode pass.
enum Action {
    SelectTab(usize),
    CloseTab(usize),
    NewTab,
    OpenWorkspace(usize),
    StartCreate,
    CancelCreate,
    CommitCreate,
    ToggleUserMenu,
    Logout,
    OpenItem(usize),
    CreateItem(ItemKind),
}

impl Shell {
    pub fn new() -> Self {
        Self {
            tabs: vec![Tab::Home],
            active: 0,
            workspaces: Vec::new(),
            loaded: false,
            search: String::new(),
            user_menu: false,
            creating: None,
        }
    }

    fn reload_workspaces(&mut self, vault: &Vault) {
        self.workspaces = vault.workspaces().unwrap_or_default();
        self.loaded = true;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, vault: &mut Vault) -> Option<Screen> {
        if !self.loaded {
            self.reload_workspaces(vault);
        }

        let mut action = None;

        egui::Panel::top("page_tabs")
            .exact_size(36.0)
            .frame(egui::Frame::default().fill(theme::TAB_STRIP))
            .show_inside(ui, |ui| {
                self.tab_strip(ui, &mut action);
                atoms::hairline_bottom(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme::BG_PAGE))
            .show_inside(ui, |ui| match &mut self.tabs[self.active] {
                Tab::Home => home::body(
                    ui,
                    vault,
                    &mut self.search,
                    &mut self.creating,
                    &self.workspaces,
                    self.user_menu,
                    &mut action,
                ),
                Tab::Workspace { meta, items, loaded } => {
                    if !*loaded {
                        *items = vault.items(&meta.id).unwrap_or_default();
                        *loaded = true;
                    }
                    workspace::body(ui, &meta.id, &meta.name, items, &mut action);
                }
                Tab::Open { item, body } => match body {
                    TabBody::Doc { doc, editor } => {
                        if doc::body(ui, item, doc, editor) {
                            let _ = vault.put_layer(&item.ws_id, &item.id, "text", &doc.export_snapshot());
                        }
                    }
                    TabBody::App { engine } => {
                        if let Some(crdt) = app_view::body(ui, item, engine) {
                            let _ = vault.put_layer(&item.ws_id, &item.id, "crdt", &crdt);
                        }
                    }
                },
            });

        self.apply(action, vault)
    }

    // ── Tab strip ─────────────────────────────────────────────────────────────

    fn tab_strip(&self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            for (i, tab) in self.tabs.iter().enumerate() {
                let active = i == self.active;
                match tab {
                    Tab::Home => {
                        if tab_strip::home_tab(ui, active).clicked() {
                            *action = Some(Action::SelectTab(i));
                        }
                    }
                    Tab::Workspace { meta, .. } => {
                        let (body, close) = tab_strip::workspace_tab(ui, &meta.name, theme::tint(&meta.id), active);
                        if close.clicked() {
                            *action = Some(Action::CloseTab(i));
                        } else if body.clicked() {
                            *action = Some(Action::SelectTab(i));
                        }
                    }
                    Tab::Open { item, .. } => {
                        let (body, close) = tab_strip::item_tab(ui, &item.kind, &item.name, &item.id, active);
                        if close.clicked() {
                            *action = Some(Action::CloseTab(i));
                        } else if body.clicked() {
                            *action = Some(Action::SelectTab(i));
                        }
                    }
                }
            }

            if tab_strip::plus_tab(ui).clicked() {
                *action = Some(Action::NewTab);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                tab_strip::cmdk_hint(ui);
            });
        });
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    fn apply(&mut self, action: Option<Action>, vault: &mut Vault) -> Option<Screen> {
        match action? {
            Action::SelectTab(i) => {
                self.active = i;
                self.user_menu = false;
            }
            Action::CloseTab(i) => {
                self.tabs.remove(i);
                if self.active >= i && self.active > 0 {
                    self.active -= 1;
                }
            }
            Action::NewTab => {
                self.active = 0;
                self.user_menu = false;
            }
            Action::OpenWorkspace(i) => {
                if let Some(meta) = self.workspaces.get(i).cloned() {
                    self.open_workspace(meta);
                }
            }
            Action::StartCreate => self.creating = Some(String::new()),
            Action::CancelCreate => self.creating = None,
            Action::CommitCreate => {
                if let Some(name) = self.creating.take() {
                    let name = name.trim().to_string();
                    if !name.is_empty() {
                        if let Ok(meta) = vault.create_workspace(&name) {
                            self.reload_workspaces(vault);
                            self.open_workspace(meta);
                        }
                    }
                }
            }
            Action::ToggleUserMenu => self.user_menu = !self.user_menu,
            Action::Logout => {
                vault.lock();
                return back_to_accounts(vault);
            }
            Action::OpenItem(i) => {
                let item = if let Tab::Workspace { items, .. } = &self.tabs[self.active] {
                    items.get(i).cloned()
                } else {
                    None
                };
                if let Some(item) = item {
                    self.open_item(item, vault);
                }
            }
            Action::CreateItem(kind) => {
                let ws_id = if let Tab::Workspace { meta, .. } = &self.tabs[self.active] {
                    meta.id.clone()
                } else {
                    return None;
                };
                // For apps, pick the Lua script file first.
                let (name, lua_bytes) = if matches!(kind, ItemKind::App) {
                    match rfd::FileDialog::new()
                        .add_filter("Lua script", &["lua"])
                        .pick_file()
                    {
                        Some(path) => {
                            let name = path.file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "app".to_string());
                            match std::fs::read(&path) {
                                Ok(bytes) => (name, Some(bytes)),
                                Err(_) => return None,
                            }
                        }
                        None => return None,
                    }
                } else {
                    ("untitled".to_string(), None)
                };

                if let Ok(item) = vault.create_item(&ws_id, &name, kind) {
                    // Store the lua script as a blob.
                    if let Some(bytes) = lua_bytes {
                        let _ = vault.put_blob(&item.ws_id, &item.id, "script.lua", &bytes);
                    }
                    if let Tab::Workspace { items, .. } = &mut self.tabs[self.active] {
                        items.push(item.clone());
                    }
                    self.open_item(item, vault);
                }
            }
        }
        None
    }

    // ── Tab management helpers ────────────────────────────────────────────────

    fn open_workspace(&mut self, meta: WorkspaceMeta) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Workspace { meta: m, .. } if m.id == meta.id)) {
            self.active = i;
        } else {
            self.tabs.push(Tab::Workspace { meta, items: Vec::new(), loaded: false });
            self.active = self.tabs.len() - 1;
        }
        self.user_menu = false;
    }

    fn open_item(&mut self, item: WorkspaceItem, vault: &Vault) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Open { item: it, .. } if it.id == item.id)) {
            self.active = i;
            return;
        }
        let body = match item.kind {
            ItemKind::Doc => {
                let doc = vault
                    .get_layer(&item.ws_id, &item.id, "text")
                    .ok()
                    .flatten()
                    .and_then(|b| Doc::from_snapshot(&b).ok())
                    .unwrap_or_else(Doc::new);
                TabBody::Doc { doc, editor: DocEditor::new() }
            }
            ItemKind::App => {
                let lua = vault.get_blob(&item.ws_id, &item.id, "script.lua").ok().flatten()
                    .unwrap_or_default();
                let crdt = vault.get_layer(&item.ws_id, &item.id, "crdt").ok().flatten();
                let engine = App::from_script(&lua, crdt.as_deref());
                TabBody::App { engine }
            }
            _ => return,
        };
        self.tabs.push(Tab::Open { item, body });
        self.active = self.tabs.len() - 1;
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bridge / MCP doc-refresh handler ─────────────────────────────────────────

impl Shell {
    /// Merge a snapshot written by the bridge into an open tab, if the doc is open.
    pub fn apply_doc_refresh(&mut self, ws_id: &str, item_id: &str, snapshot: &[u8]) {
        for tab in &mut self.tabs {
            if let Tab::Open { item, body: TabBody::Doc { doc, .. } } = tab {
                if item.ws_id == ws_id && item.id == item_id {
                    let _ = doc.import(snapshot);
                    return;
                }
            }
        }
    }
}

