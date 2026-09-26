use crate::item::ItemsScreen;
use crate::node;
use crate::{Msg, Screen, theme};
use courier::invite::InviteTicket;
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::token::Scope;
use courier::{ConnectionTicket, DesktopNodeRecord};
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
    node: Option<DesktopNodeRecord>,
    claim_ticket: String,
    show_claim: bool,
    claim_error: Option<String>,
    publish_status: Option<Result<String, String>>,
    invite_status: Option<Result<String, String>>,
}
#[derive(Clone)]
pub enum SpaceScreenMsg {
    AddSpace,
    ToggleAddSpace,
    SpaceName(String),
    OpenSpace(String),
    WorkspaceCreation(Result<WorkspaceMeta, String>),
    ToggleClaim,
    ClaimTicket(String),
    ClaimNode,
    ClaimResult(Result<DesktopNodeRecord, String>),
    PublishAll,
    PublishResult(Result<String, String>),
    InviteMint,
    InviteResult(Result<String, String>),
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
        let node = node::load_relationship(vault).ok().flatten();
        SpaceScreen {
            spaces,
            error,
            space_name: String::new(),
            add_space: false,
            node,
            claim_ticket: String::new(),
            show_claim: false,
            claim_error: None,
            publish_status: None,
            invite_status: None,
        }
    }

    fn invite_widget(&self) -> El<Msg> {
        let mut widget = row().gap(8.0).center();
        if let Some(status) = &self.invite_status {
            let (ok, msg) = match status {
                Ok(m) => (true, m.clone()),
                Err(e) => (false, e.clone()),
            };
            widget = widget.child(text(msg).font_size(11.0).no_wrap().color(if ok {
                theme::fg_3()
            } else {
                theme::error()
            }));
        }
        widget.child(
            row()
                .h(36.0)
                .px(14.0)
                .radius(6.0)
                .center()
                .no_shrink()
                .id("invite")
                .fill(theme::bg_1())
                .stroke(1.0, theme::bd_2())
                .hover_stroke(1.0, theme::bd_3())
                .tint(120.0)
                .on_click(Msg::Space(SpaceScreenMsg::InviteMint))
                .child(text("invite").font_size(13.0).color(theme::fg_1())),
        )
    }

    fn publish_widget(&self) -> El<Msg> {
        let mut widget = row().gap(8.0).center();
        if let Some(status) = &self.publish_status {
            let (ok, msg) = match status {
                Ok(m) => (true, m.clone()),
                Err(e) => (false, e.clone()),
            };
            widget = widget.child(text(msg).font_size(11.0).no_wrap().color(if ok {
                theme::fg_3()
            } else {
                theme::error()
            }));
        }
        widget.child(
            row()
                .h(36.0)
                .px(14.0)
                .radius(6.0)
                .center()
                .no_shrink()
                .id("publish_all")
                .fill(theme::bg_1())
                .stroke(1.0, theme::bd_2())
                .hover_stroke(1.0, theme::bd_3())
                .tint(120.0)
                .on_click(Msg::Space(SpaceScreenMsg::PublishAll))
                .child(text("publish").font_size(13.0).color(theme::fg_1())),
        )
    }

    fn claim_widget(&self) -> El<Msg> {
        if let Some(record) = &self.node {
            let did = record.node_did.chars().take(8).collect::<String>();
            return row().h(36.0).px(14.0).center().child(
                text(format!("node {did}…"))
                    .font_size(12.0)
                    .color(theme::fg_3()),
            );
        }
        let mut btn = row()
            .h(36.0)
            .px(14.0)
            .radius(6.0)
            .center()
            .no_shrink()
            .id("claim_node")
            .fill(theme::bg_1())
            .stroke(1.0, theme::bd_2())
            .hover_stroke(1.0, theme::bd_3())
            .tint(120.0)
            .on_click(Msg::Space(SpaceScreenMsg::ToggleClaim))
            .child(text("join a node").font_size(13.0).color(theme::fg_1()));
        if self.show_claim {
            let caption = self.claim_error.as_deref().unwrap_or("⏎ join · esc cancel");
            let caption_color = if self.claim_error.is_some() {
                theme::error()
            } else {
                theme::fg_3()
            };
            let panel = col()
                .w(360.0)
                .pad(12.0)
                .gap(8.0)
                .fill(theme::bg_2())
                .stroke(1.0, theme::bd_3())
                .radius(6.0)
                .child(
                    text_input(&self.claim_ticket, "claim_ticket", |s| {
                        Msg::Space(SpaceScreenMsg::ClaimTicket(s))
                    })
                    .placeholder("paste a node ticket or an invite")
                    .on_enter(Msg::Space(SpaceScreenMsg::ClaimNode))
                    .on_esc(Msg::Space(SpaceScreenMsg::ToggleClaim))
                    .autofocus()
                    .h(36.0)
                    .px(10.0)
                    .font_size(12.0)
                    .fill(theme::bg_1())
                    .color(theme::fg_1())
                    .radius(6.0)
                    .stroke(1.0, theme::bd_2()),
                )
                .child(text(caption).font_size(11.0).color(caption_color));
            btn = btn.overlay(
                panel,
                Some(Msg::Space(SpaceScreenMsg::ToggleClaim)),
                Placement {
                    side: PlacementSide::Bottom,
                    align: PlacementAlign::End,
                },
                Anchor::Element,
            );
        }
        btn
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
        let mut header = row().gap(12.0).child(col().grow());
        if self.node.is_some() {
            header = header
                .child(self.invite_widget())
                .child(self.publish_widget());
        }
        header = header.child(self.claim_widget()).child(add_btn);

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
            SpaceScreenMsg::ToggleClaim => {
                self.show_claim = !self.show_claim;
                self.claim_error = None;
                None
            }
            SpaceScreenMsg::ClaimTicket(s) => {
                self.claim_ticket = s;
                None
            }
            SpaceScreenMsg::ClaimNode => {
                let text = self.claim_ticket.trim().to_string();
                let vault = vault.clone();
                let proxy = proxy.clone();
                // Two ticket kinds share one box: `ConnectionTicket` (`osv1.`) claims a node
                // that has no admin yet, `InviteTicket` (`osvi1.`) joins one that already
                // does. Their prefixes are how `from_text` tells them apart on sight.
                if let Ok(ticket) = ConnectionTicket::from_text(&text) {
                    std::thread::spawn(move || {
                        let socket = kunki::bridge::socket_path();
                        let now = node::now_secs();
                        let result = node::claim(&socket, &vault, &ticket, now).and_then(|r| {
                            node::save_relationship(&vault, &r)?;
                            Ok(r)
                        });
                        let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::ClaimResult(result)));
                    });
                } else if let Ok(ticket) = InviteTicket::from_text(&text) {
                    std::thread::spawn(move || {
                        let socket = kunki::bridge::socket_path();
                        let now = node::now_secs();
                        let result =
                            node::claim_invite(&socket, &vault, ticket, now).and_then(|r| {
                                node::save_relationship(&vault, &r)?;
                                Ok(r)
                            });
                        let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::ClaimResult(result)));
                    });
                } else {
                    self.claim_error = Some("not a recognized node ticket or invite".to_string());
                }
                None
            }
            SpaceScreenMsg::ClaimResult(result) => {
                match result {
                    Ok(record) => {
                        self.node = Some(record);
                        self.show_claim = false;
                        self.claim_ticket.clear();
                        self.claim_error = None;
                    }
                    Err(e) => self.claim_error = Some(e),
                }
                None
            }
            SpaceScreenMsg::PublishAll => {
                let Some(record) = self.node.clone() else {
                    return None;
                };
                let spaces = self.spaces.clone();
                let vault = vault.clone();
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let socket = kunki::bridge::socket_path();
                    let mut published = 0usize;
                    for meta in &spaces {
                        let result = publish_one(&socket, &vault, record.token.clone(), meta);
                        if let Err(e) = result {
                            let _ =
                                proxy.send_event(Msg::Space(SpaceScreenMsg::PublishResult(Err(e))));
                            return;
                        }
                        published += 1;
                    }
                    let msg = format!("published {published} workspace(s)");
                    let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::PublishResult(Ok(msg))));
                });
                None
            }
            SpaceScreenMsg::PublishResult(result) => {
                self.publish_status = Some(result);
                None
            }
            SpaceScreenMsg::InviteMint => {
                let Some(record) = self.node.clone() else {
                    return None;
                };
                let vault = vault.clone();
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let socket = kunki::bridge::socket_path();
                    let result = node::invite(&socket, &vault, record.token, "member", Scope::Node)
                        .and_then(|ticket| ticket.to_text().map_err(|e| e.to_string()));
                    // No clipboard support in this codebase — printed for the same reason
                    // kunki's own boot ticket is: a terminal is the thing to copy it from.
                    let status = match &result {
                        Ok(text) => {
                            println!("invite ticket (paste into another desktop):\n{text}");
                            Ok("invite minted — see terminal output".to_string())
                        }
                        Err(e) => Err(e.clone()),
                    };
                    let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::InviteResult(status)));
                });
                None
            }
            SpaceScreenMsg::InviteResult(result) => {
                self.invite_status = Some(result);
                None
            }
        }
    }
}

/// One workspace's headers and item headers, over the wire to the claimed node. `courier`
/// never touches storage, so the item list is read here rather than inside `node::publish`.
pub(crate) fn publish_one(
    socket: &std::path::Path,
    vault: &Vault,
    token: courier::token::Token,
    meta: &WorkspaceMeta,
) -> Result<(), String> {
    let items = vault.items(&meta.id).map_err(|e| e.to_string())?;
    let workspace = PublishedWorkspace {
        id: meta.id.clone(),
        name: meta.name.clone(),
        created: meta.created,
    };
    let published_items = items
        .iter()
        .map(|i| PublishedItem {
            id: i.id.clone(),
            name: i.name.clone(),
            kind: i.kind.as_str().to_string(),
            created: i.created,
        })
        .collect();
    node::publish(socket, vault, token, workspace, published_items).map(|_ack| ())
}
