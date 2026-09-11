use crate::item::ItemsScreen;
use crate::{Msg, Screen, theme};
use runtime::{
    Anchor, El, EventLoopProxy, Placement, PlacementAlign, PlacementSide, col, row, text,
    text_input,
};
use vault::{Vault, WorkspaceMeta};

pub struct SpaceScreen {
    spaces: Vec<WorkspaceMeta>,
    space_name: String,
    add_space: bool,
    error: Option<String>,
}
#[derive(Clone)]
pub enum SpaceScreenMsg {
    AddSpace,
    ToggleAddSpace,
    SpaceName(String),
    OpenSpace(String),
    WorkspaceCreation(Result<WorkspaceMeta, String>),
}

impl SpaceScreen {
    /// The workspace metas this screen snapshots — read access for tests and the bridge.
    pub fn spaces(&self) -> &[WorkspaceMeta] {
        &self.spaces
    }

    pub fn new(vault: &Vault) -> Self {
        let (spaces, error) = match vault.workspaces() {
            Ok(s) => (s, None),
            Err(e) => (Vec::<WorkspaceMeta>::new(), Some(e.to_string())),
        };
        SpaceScreen {
            spaces,
            error,
            space_name: String::new(),
            add_space: false,
        }
    }

    fn name_input(&self) -> El<Msg> {
        text_input(&self.space_name, "workspace_name", |s| {
            Msg::Space(SpaceScreenMsg::SpaceName(s))
        })
        .placeholder("name a workspace")
        .on_enter(Msg::Space(SpaceScreenMsg::AddSpace))
        .autofocus()
        .h(40.0)
        .px(12.0)
        .font_size(13.0)
        .fill(theme::bg_1())
        .color(theme::fg_1())
        .radius(6.0)
        .stroke(1.0, theme::bd_2())
        .hover_stroke(1.0, theme::bd_3())
    }

    pub fn view(&self) -> El<Msg> {
        if self.spaces.is_empty() {
            return col()
                .full()
                .center()
                .gap(10.0)
                .child(self.name_input().w(420.0))
                .child(
                    text("press ⏎ to create")
                        .font_size(11.0)
                        .no_wrap()
                        .color(theme::fg_3()),
                );
        }

        let mut add_btn = row()
            .h(36.0)
            .px(14.0)
            .radius(6.0)
            .center()
            // See `item.rs` — a label is not a paragraph, and a squeezed row folds it into
            // stacked words rather than clipping it.
            .no_shrink()
            .id("add_space")
            .fill(theme::accent())
            .hover_fill(theme::accent_hover())
            .press_fill(theme::accent_press())
            .tint(120.0)
            .on_click(Msg::Space(SpaceScreenMsg::ToggleAddSpace))
            .child(
                text("+ new workspace")
                    .font_size(13.0)
                    .no_wrap()
                    .color(theme::bg_page()),
            );
        if self.add_space {
            let panel = col()
                .w(320.0)
                .pad(12.0)
                .gap(8.0)
                .fill(theme::bg_2())
                .stroke(1.0, theme::bd_3())
                .radius(6.0)
                .child(
                    self.name_input()
                        .on_esc(Msg::Space(SpaceScreenMsg::ToggleAddSpace)),
                )
                .child(
                    text("⏎ create · esc cancel")
                        .font_size(11.0)
                        .no_wrap()
                        .color(theme::fg_3()),
                );
            add_btn = add_btn.overlay(
                panel,
                Some(Msg::Space(SpaceScreenMsg::ToggleAddSpace)),
                Placement {
                    side: PlacementSide::Bottom,
                    align: PlacementAlign::End,
                },
                Anchor::Element,
            );
        }
        let header = row().child(col().grow()).child(add_btn);

        let mut grid = row().wrap().gap(20.0);
        for space in &self.spaces {
            let workspace_el: El<Msg> = col()
                .size(240.0, 140.0)
                .pad(16.0)
                .fill(theme::bg_1())
                .stroke(1.0, theme::bd_2())
                .hover_stroke(1.0, theme::bd_3())
                .tint(120.0)
                .id(space.id.clone())
                .on_click(Msg::Space(SpaceScreenMsg::OpenSpace(space.id.clone())))
                .child(
                    row()
                        .align_center()
                        .child(text(&space.name).font_size(15.0).color(theme::fg_1())),
                );
            grid = grid.child(workspace_el);
        }

        col().full().pad(40.0).gap(24.0).child(header).child(grid)
    }

    pub fn update(
        &mut self,
        msg: SpaceScreenMsg,
        vault: &Vault,
        proxy: &EventLoopProxy<Msg>,
    ) -> Option<Screen> {
        match msg {
            SpaceScreenMsg::AddSpace => {
                if self.space_name.trim().is_empty() {
                    return None;
                }
                let vault = vault.clone();
                let proxy = proxy.clone();
                let name = self.space_name.clone();
                std::thread::spawn(move || {
                    let result = vault.create_workspace(&name).map_err(|e| e.to_string());
                    let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::WorkspaceCreation(result)));
                });
                None
            }
            SpaceScreenMsg::ToggleAddSpace => {
                self.add_space = !self.add_space;
                None
            }
            SpaceScreenMsg::SpaceName(s) => {
                self.space_name = s;
                None
            }
            SpaceScreenMsg::OpenSpace(id) => {
                let meta = self.spaces.iter().find(|s| s.id == id).cloned();
                meta.map(|ws| Screen::Items(ItemsScreen::new(vault, ws)))
            }
            SpaceScreenMsg::WorkspaceCreation(result) => {
                match result {
                    Ok(_w) => {
                        self.space_name.clear();
                        self.add_space = false;
                        match vault.workspaces() {
                            Ok(w) => self.spaces = w,
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                };
                None
            }
        }
    }
}
