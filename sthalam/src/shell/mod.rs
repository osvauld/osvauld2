use doc_editor::{Doc, DocEditor};
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke};
use egui_dock::{DockArea, DockState, OverlayType, Style, TabViewer};
use vault::{ItemKind, Vault, WorkspaceItem, WorkspaceMeta};

use crate::app::Screen;
use crate::bridge::Refresh;
use crate::screens::common::back_to_accounts;
use crate::theme;

use app_view::AppTab;

mod app_view;
mod atoms;
mod doc;
mod home;
mod workspace;

pub struct Shell {
    dock: DockState<Tab>,
    workspaces: Vec<WorkspaceMeta>,
    loaded: bool,
    search: String,
    user_menu: bool,
    creating: Option<String>,
}

enum Tab {
    Home,
    Workspace { meta: WorkspaceMeta, items: Vec<WorkspaceItem>, loaded: bool },
    /// Any opened item (.doc, .app, …)
    Open { item: WorkspaceItem, body: TabBody },
}

enum TabBody {
    Doc { doc: Doc, editor: DocEditor },
    App { tab: AppTab },
}

enum Action {
    OpenWorkspace(usize),
    StartCreate,
    CancelCreate,
    CommitCreate,
    ToggleUserMenu,
    Logout,
    OpenItem(WorkspaceItem),
    CreateItem { ws_id: String, kind: ItemKind },
}

// Returns a unique stable string key for a tab — used to identify the active one.
fn tab_key(tab: &Tab) -> &str {
    match tab {
        Tab::Home => "home",
        Tab::Workspace { meta, .. } => &meta.id,
        Tab::Open { item, .. } => &item.id,
    }
}

// ── egui_dock viewer ──────────────────────────────────────────────────────────

struct ShellViewer<'a> {
    vault: &'a mut Vault,
    action: &'a mut Option<Action>,
    workspaces: &'a [WorkspaceMeta],
    search: &'a mut String,
    creating: &'a mut Option<String>,
    user_menu: bool,
    /// Key of the currently active (focused) tab, used for styling.
    active_key: Option<String>,
}

impl TabViewer for ShellViewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let is_active = self.active_key.as_deref() == Some(tab_key(tab));
        let fg = if is_active { theme::FG_1 } else { theme::FG_3 };

        match tab {
            Tab::Home => {
                let color = if is_active { theme::ACCENT } else { theme::FG_3 };
                egui::RichText::new("⌂")
                    .font(FontId::new(14.0, FontFamily::Monospace))
                    .color(color)
                    .into()
            }
            Tab::Workspace { meta, .. } => {
                use egui::text::{LayoutJob, TextFormat};
                let mut job = LayoutJob::default();
                job.append("■ ", 0.0, TextFormat {
                    font_id: FontId::new(9.0, FontFamily::Monospace),
                    color: theme::tint(&meta.id),
                    ..Default::default()
                });
                job.append(&atoms::elide(&meta.name, 18), 0.0, TextFormat {
                    font_id: FontId::new(12.5, FontFamily::Proportional),
                    color: fg,
                    ..Default::default()
                });
                egui::WidgetText::LayoutJob(job.into())
            }
            Tab::Open { item, .. } => {
                use egui::text::{LayoutJob, TextFormat};
                let badge = format!(".{} ", item.kind.as_str());
                let mut job = LayoutJob::default();
                job.append(&badge, 0.0, TextFormat {
                    font_id: FontId::new(9.5, FontFamily::Monospace),
                    color: theme::FG_4,
                    ..Default::default()
                });
                job.append(&atoms::elide(&item.name, 18), 0.0, TextFormat {
                    font_id: FontId::new(12.5, FontFamily::Proportional),
                    color: fg,
                    ..Default::default()
                });
                egui::WidgetText::LayoutJob(job.into())
            }
        }
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        match tab {
            Tab::Home => egui::Id::new("shell_home"),
            Tab::Workspace { meta, .. } => egui::Id::new(("shell_ws", &meta.id)),
            Tab::Open { item, .. } => egui::Id::new(("shell_item", &item.id)),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Home => home::body(
                ui,
                self.vault,
                self.search,
                self.creating,
                self.workspaces,
                self.user_menu,
                self.action,
            ),
            Tab::Workspace { meta, items, loaded } => {
                if !*loaded {
                    *items = self.vault.items(&meta.id).unwrap_or_default();
                    *loaded = true;
                }
                workspace::body(ui, &meta.id, &meta.name, items, self.action);
            }
            Tab::Open { item, body } => match body {
                TabBody::Doc { doc, editor } => {
                    if doc::body(ui, item, doc, editor) {
                        let _ = self.vault.put_state(&item.ws_id, &item.id, &doc.export_snapshot());
                    }
                }
                TabBody::App { tab } => {
                    if let Some(crdt) = app_view::body(ui, item, tab) {
                        let _ = self.vault.put_state(&item.ws_id, &item.id, &crdt);
                    }
                }
            },
        }
    }

    /// Paint a 2px accent bar at the top of the active tab button.
    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        if self.active_key.as_deref() != Some(tab_key(tab)) { return; }
        response.ctx.layer_painter(
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("tab_accent_line"))
        ).hline(
            response.rect.x_range(),
            response.rect.top() + 1.0,
            Stroke::new(2.0, theme::ACCENT),
        );
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        !matches!(tab, Tab::Home)
    }

    fn clear_background(&self, _tab: &Tab) -> bool {
        false
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }
}

// ── Shell ─────────────────────────────────────────────────────────────────────

impl Shell {
    pub fn new() -> Self {
        Self {
            dock: DockState::new(vec![Tab::Home]),
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

        // Snapshot the active tab key before DockArea takes a mutable borrow.
        let active_key = self.dock.find_active_focused()
            .map(|(_, tab)| tab_key(tab).to_owned());

        let mut action = None;
        {
            let style = dock_style(ui);
            let mut viewer = ShellViewer {
                vault,
                action: &mut action,
                workspaces: &self.workspaces,
                search: &mut self.search,
                creating: &mut self.creating,
                user_menu: self.user_menu,
                active_key,
            };
            DockArea::new(&mut self.dock).style(style).show_inside(ui, &mut viewer);
        }
        self.apply(action, vault)
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    fn apply(&mut self, action: Option<Action>, vault: &mut Vault) -> Option<Screen> {
        match action? {
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
            Action::OpenItem(item) => {
                self.open_item(item, vault);
            }
            Action::CreateItem { ws_id, kind } => {
                // Apps are seeded with a starter source tree by the vault; docs get a starter CRDT
                // snapshot here so MCP can read/write them before they're ever opened. An agent
                // fills in the real content afterwards.
                if let Ok(item) = vault.create_item(&ws_id, "untitled", kind) {
                    crate::bridge::seed_item_state(vault, &item);
                    for (_, tab) in self.dock.iter_all_tabs_mut() {
                        if let Tab::Workspace { meta, items, .. } = tab {
                            if meta.id == ws_id {
                                items.push(item.clone());
                                break;
                            }
                        }
                    }
                    self.open_item(item, vault);
                }
            }
        }
        None
    }

    // ── Tab management ────────────────────────────────────────────────────────

    /// Focus an already-open tab matching `pred`; returns true if one was found.
    fn focus_existing(&mut self, pred: impl Fn(&Tab) -> bool) -> bool {
        let path = self.dock.iter_all_tabs()
            .find_map(|(path, t)| pred(t).then_some(path));
        match path {
            Some(path) => { let _ = self.dock.set_active_tab(path); true }
            None => false,
        }
    }

    fn open_workspace(&mut self, meta: WorkspaceMeta) {
        if !self.focus_existing(|t| matches!(t, Tab::Workspace { meta: m, .. } if m.id == meta.id)) {
            self.dock.push_to_focused_leaf(Tab::Workspace { meta, items: Vec::new(), loaded: false });
        }
        self.user_menu = false;
    }

    fn open_item(&mut self, item: WorkspaceItem, vault: &Vault) {
        if self.focus_existing(|t| matches!(t, Tab::Open { item: i, .. } if i.id == item.id)) {
            return;
        }
        let body = match item.kind {
            ItemKind::Doc => {
                let doc = vault
                    .get_state(&item.ws_id, &item.id)
                    .ok()
                    .flatten()
                    .and_then(|b| Doc::from_snapshot(&b).ok())
                    .unwrap_or_else(Doc::new);
                TabBody::Doc { doc, editor: DocEditor::new() }
            }
            ItemKind::App => TabBody::App { tab: AppTab::load(vault, &item) },
            _ => return,
        };
        self.dock.push_to_focused_leaf(Tab::Open { item, body });
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bridge / MCP refresh handler ──────────────────────────────────────────────

impl Shell {
    /// Merge a write made by an external client (MCP) into any matching open tab.
    pub fn apply_refresh(&mut self, vault: &Vault, refresh: Refresh) {
        match refresh {
            Refresh::Doc { ws_id, item_id, snapshot } => self.refresh_doc(vault, &ws_id, &item_id, &snapshot),
            Refresh::App { ws_id, item_id } => self.refresh_app(vault, &ws_id, &item_id),
            Refresh::Workspace { ws_id } => self.refresh_workspace(&ws_id),
        }
    }

    fn refresh_doc(&mut self, vault: &Vault, ws_id: &str, item_id: &str, snapshot: &[u8]) {
        for (_, tab) in self.dock.iter_all_tabs_mut() {
            if let Tab::Open { item, body: TabBody::Doc { doc, .. } } = tab {
                if item.ws_id == ws_id && item.id == item_id {
                    let _ = doc.import(snapshot);
                    // The bridge already persisted its own snapshot, but the live tab may
                    // hold local edits the bridge's read-modify-write didn't see. Re-export
                    // the merged doc so the durable state is the union of both, not whichever
                    // writer landed last.
                    let _ = vault.put_state(ws_id, item_id, &doc.export_snapshot());
                    return;
                }
            }
        }
    }

    fn refresh_app(&mut self, vault: &Vault, ws_id: &str, item_id: &str) {
        for (_, tab) in self.dock.iter_all_tabs_mut() {
            if let Tab::Open { item, body: TabBody::App { tab: app } } = tab {
                if item.ws_id == ws_id && item.id == item_id {
                    app.reload(vault, item);
                    return;
                }
            }
        }
    }

    /// Mark an open workspace tab stale so it re-reads its item list next frame, surfacing an
    /// item the bridge just created. (Re-reading the whole list also picks up any other external
    /// change to the workspace, and is cheaper to reason about than splicing one item in.)
    fn refresh_workspace(&mut self, ws_id: &str) {
        for (_, tab) in self.dock.iter_all_tabs_mut() {
            if let Tab::Workspace { meta, loaded, .. } = tab {
                if meta.id == ws_id {
                    *loaded = false;
                }
            }
        }
    }
}

fn dock_style(ui: &egui::Ui) -> Style {
    let mut style = Style::from_egui(ui.style().as_ref());

    // Drag-to-split overlay
    style.overlay.overlay_type = OverlayType::HighlightedAreas;
    style.overlay.selection_color = theme::ACCENT;

    // Tab bar strip
    style.tab_bar.bg_fill = theme::TAB_STRIP;
    style.tab_bar.height = 36.0;
    style.tab_bar.hline_color = theme::BD_2;
    style.tab_bar.inner_margin = Margin::ZERO;
    style.tab_bar.corner_radius = CornerRadius::ZERO;

    // Tab button sizing
    style.tab.spacing = 0.0;

    // Active / focused tab — matches BG_PAGE so content area appears merged
    let active_style = |s: &mut egui_dock::TabInteractionStyle| {
        s.bg_fill = theme::BG_PAGE;
        s.outline_color = Color32::TRANSPARENT;
        s.text_color = theme::FG_1;
        s.corner_radius = CornerRadius::ZERO;
    };
    active_style(&mut style.tab.active);
    active_style(&mut style.tab.focused);
    active_style(&mut style.tab.active_with_kb_focus);
    active_style(&mut style.tab.focused_with_kb_focus);

    // Inactive tab — transparent so TAB_STRIP shows through
    let inactive_style = |s: &mut egui_dock::TabInteractionStyle| {
        s.bg_fill = Color32::TRANSPARENT;
        s.outline_color = Color32::TRANSPARENT;
        s.text_color = theme::FG_3;
        s.corner_radius = CornerRadius::ZERO;
    };
    inactive_style(&mut style.tab.inactive);
    inactive_style(&mut style.tab.inactive_with_kb_focus);

    // Hovered tab — slight white tint
    style.tab.hovered.bg_fill = Color32::from_white_alpha(8);
    style.tab.hovered.outline_color = Color32::TRANSPARENT;
    style.tab.hovered.text_color = theme::FG_2;
    style.tab.hovered.corner_radius = CornerRadius::ZERO;

    // Tab body — no extra padding or border (each tab manages its own layout)
    style.tab.tab_body.inner_margin = Margin::ZERO;
    style.tab.tab_body.stroke = Stroke::NONE;

    // Close button colors
    style.buttons.close_tab_color = theme::FG_4;
    style.buttons.close_tab_active_color = theme::FG_1;
    style.buttons.close_tab_bg_fill = Color32::TRANSPARENT;

    style
}
