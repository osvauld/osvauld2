//! Items screen: inside one workspace — the apps it holds, drawn as the same card grid as
//! the workspaces themselves. List-only for now: app upload/creation is the next design
//! discussion, so an empty workspace just says so.

use crate::app_src::{app_name, read_app_folder, snapshot};
use crate::space::SpaceScreen;
use crate::{Msg, Screen, theme};
use runtime::{El, EventLoopProxy, col, row, text};
use vault::{Vault, WorkspaceItem, WorkspaceMeta};

pub struct ItemsScreen {
    ws: WorkspaceMeta,
    items: Vec<WorkspaceItem>,
    error: Option<String>,
}

#[derive(Clone)]
pub enum ItemsScreenMsg {
    Back,
    Upload,
    Open(WorkspaceItem),
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
        ItemsScreen { ws, items, error }
    }

    pub fn view(&self) -> El<Msg> {
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
            .child(col().grow())
            .child(upload_btn);

        let body = if self.items.is_empty() {
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
            grid
        };

        let mut page = col().full().pad(40.0).gap(24.0).child(header);
        // Upload failures land here (no main.lua, non-utf8 file, locked vault) — without this
        // the button is a silent no-op.
        if let Some(error) = &self.error {
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
            ItemsScreenMsg::Open(item) => None, //intercepted before its reaced here
        }
    }

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
        Ok(())
    }
}
