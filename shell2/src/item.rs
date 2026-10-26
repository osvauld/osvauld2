//! Workspace items: Lua navigation/import presentation, with Rust authority and fallback chrome.
//! Keeps the Rust grid as a fallback if the launcher cannot initialize.

#[cfg(all(feature = "folder-picker", not(target_os = "android")))]
use crate::app_src::{app_name, read_app_folder, snapshot};
use crate::launcher::{Launcher, session::Inventory};
use crate::space::SpaceScreen;
use crate::{Msg, Screen, theme};
use app_host::LuaMsg;
use runtime::{El, EventLoopProxy, col, row, text};
use std::{cell::RefCell, rc::Rc};
use vault::{Vault, WorkspaceItem, WorkspaceMeta};

pub struct ItemsScreen {
    ws: WorkspaceMeta,
    items: Vec<WorkspaceItem>,
    error: Option<String>,
    inventory: Inventory,
}

#[derive(Clone)]
pub enum ItemsScreenMsg {
    Back,
    Upload,
    Open(WorkspaceItem),
    Launcher(String, u64, LuaMsg),
}

impl ItemsScreen {
    /// The workspace whose items this screen snapshots — the bridge's refresh needs it to
    /// decide whether a write concerns the screen that is showing.
    pub fn ws(&self) -> &WorkspaceMeta {
        &self.ws
    }

    pub fn new(vault: &Vault, ws: WorkspaceMeta) -> Self {
        let (items, error) = match vault.items(&ws.id) {
            Ok(i) => (i, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        let inventory = Rc::new(RefCell::new(crate::shell_api::Snapshot {
            workspace: ws.clone(),
            items: items.clone(),
        }));
        ItemsScreen {
            ws,
            items,
            error,
            inventory,
        }
    }

    pub fn inventory(&self) -> Inventory {
        self.inventory.clone()
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn refresh(&mut self, vault: &Vault) {
        match vault.items(&self.ws.id) {
            Ok(items) => {
                self.items = items;
                self.inventory.borrow_mut().items = self.items.clone();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn view(&self, launcher: Option<&Launcher>, warning: Option<&str>) -> El<Msg> {
        #[cfg(all(feature = "folder-picker", not(target_os = "android")))]
        let upload_btn = row()
            .h(36.0)
            .px(14.0)
            .radius(6.0)
            .center()
            // Without this a narrow window folds the label into three stacked words rather than
            // letting the title beside it give way — the spacer collapses first, then everything
            // shrinks to min-content together.
            .no_shrink()
            .id("add_item")
            .fill(theme::accent())
            .hover_fill(theme::accent_hover())
            .press_fill(theme::accent_press())
            .tint(120.0)
            .on_click(Msg::Items(ItemsScreenMsg::Upload))
            .child(
                text("+ Add item")
                    .font_size(13.0)
                    .no_wrap()
                    .color(theme::bg_page()),
            );
        let back = row()
            .h(36.0)
            .px(10.0)
            .radius(6.0)
            .center()
            .no_shrink()
            .id("back")
            .hover_fill(theme::bg_1())
            .tint(120.0)
            .on_click(Msg::Items(ItemsScreenMsg::Back))
            .child(text("←").font_size(15.0).no_wrap().color(theme::fg_3()));
        let header = row()
            .gap(12.0)
            .align_center()
            .child(back)
            .child(text(&self.ws.name).font_size(15.0).color(theme::fg_1()))
            .child(col().grow());
        #[cfg(all(feature = "folder-picker", not(target_os = "android")))]
        let header = header.child(upload_btn);

        let body = if let Some(launcher) = launcher {
            launcher.view()
        } else if self.items.is_empty() {
            col().grow().center().child(
                text("no apps in this workspace yet")
                    .font_size(13.0)
                    .color(theme::fg_3()),
            )
        } else {
            let mut grid = row().wrap().gap(20.0);
            for item in &self.items {
                grid = grid.child(
                    col()
                        .size(240.0, 140.0)
                        .pad(16.0)
                        .fill(theme::bg_1())
                        .stroke(1.0, theme::bd_2())
                        .hover_stroke(1.0, theme::bd_3())
                        .tint(120.0)
                        .id(item.id.clone())
                        .on_click(Msg::Items(ItemsScreenMsg::Open(item.clone())))
                        .child(
                            row()
                                .align_center()
                                .child(text(&item.name).font_size(15.0).color(theme::fg_1())),
                        ),
                );
            }
            // A growing scroller takes the page's remaining height; without it a long app list
            // runs off the bottom of the window.
            col().id("items").grow().scroll_y().child(grid)
        };

        // The reserved marker opts into Lua presentation. Old overrides and failed views keep
        // the Rust controls; query the Rust description, never execute a handler to probe it.
        let mut page = col().full().pad(40.0).gap(24.0);
        if !body.contains_id("launcher:workspace-header") {
            page = page.child(header);
        }
        // Upload failures land here (no main.lua, non-utf8 file, locked vault) — without this
        // the button is a silent no-op.
        if let Some(error) = self.error.as_deref().or(warning) {
            page = page.child(text(error).font_size(12.0).color(theme::error()));
        }
        page.child(body)
    }

    pub fn update(
        &mut self,
        msg: ItemsScreenMsg,
        vault: &Vault,
        _proxy: &EventLoopProxy<Msg>,
    ) -> Option<Screen> {
        match msg {
            ItemsScreenMsg::Back => Some(Screen::Spaces(SpaceScreen::new(vault))),
            ItemsScreenMsg::Upload => {
                self.error = self.upload(vault).err();
                None
            }
            ItemsScreenMsg::Open(_) | ItemsScreenMsg::Launcher(..) => None, // intercepted by Shell
        }
    }

    #[cfg(not(all(feature = "folder-picker", not(target_os = "android"))))]
    pub fn upload(&mut self, _vault: &Vault) -> Result<(), String> {
        Err("folder import is unavailable in this build".into())
    }

    #[cfg(all(feature = "folder-picker", not(target_os = "android")))]
    pub fn upload(&mut self, vault: &Vault) -> Result<(), String> {
        let Some(root) = rfd::FileDialog::new().pick_folder() else {
            return Ok(());
        };
        let files = read_app_folder(&root)?;
        let name = app_name(&files, &root);
        let snapshot_bytes = snapshot(&files)?;
        let item = vault
            .create_item(&self.ws.id, &name, vault::ItemKind::App)
            .map_err(|e| e.to_string())?;
        vault
            .put_src(&self.ws.id, &item.id, &snapshot_bytes)
            .map_err(|e| e.to_string())?;
        self.items = vault.items(&self.ws.id).map_err(|e| e.to_string())?;
        self.inventory.borrow_mut().items = self.items.clone();
        Ok(())
    }
}
