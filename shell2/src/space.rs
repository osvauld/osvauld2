use crate::item::ItemsScreen;
use crate::node;
use crate::{Msg, Screen, theme};
use crate::launcher::{Launcher, context::Spaces};
use std::{cell::RefCell, rc::Rc};
use courier::DesktopNodeRecord;
use courier::publish::{PublishedItem, PublishedWorkspace};
use courier::token::Scope;
use runtime::{
    Anchor, El, EventLoopProxy, Placement, PlacementAlign, PlacementSide, col, row, text,
    text_input,
};
use vault::{Vault, WorkspaceMeta};

pub struct SpaceScreen {
    spaces: Vec<WorkspaceMeta>,
    inventory: Spaces,
    space_name: String,
    add_space: bool,
    error: Option<String>,
    node: Option<DesktopNodeRecord>,
    /// The record names a node other than this account. An account that hosts itself
    /// (every unlock since laptop-host step 4) holds its own record and still joins by link.
    foreign_node: bool,
    claim_ticket: String,
    show_claim: bool,
    claim_error: Option<String>,
    /// A share link went to the shell's `JoinSpace`; `JoinResult` ends it.
    joining: bool,
    publish_status: Option<Result<String, String>>,
    invite_status: Option<Result<String, String>>,
}
#[derive(Clone)]
pub enum SpaceScreenMsg {
    AddSpace,
    ToggleAddSpace,
    SpaceName(String),
    OpenSpace(String),
    ToggleClaim,
    ClaimTicket(String),
    ClaimNode,
    ClaimResult(Result<DesktopNodeRecord, String>),
    /// The shell answered a share link from the join box (`Msg::JoinLink`).
    JoinResult(Result<(), String>),
    PublishAll,
    PublishResult(Result<String, String>),
    InviteMint,
    InviteResult(Result<String, String>),
}

/// Whether the join box holds a space's share link (laptop-host §2 "Sharing = a link") rather
/// than a node ticket or an invite for the socket route.
pub(crate) fn is_share_link(text: &str) -> bool {
    text.trim().starts_with(courier::link::PREFIX)
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
        let foreign_node = node.as_ref().is_some_and(|r| !node::is_me(vault, r));
        let inventory = Rc::new(RefCell::new(crate::launcher::context::List {
            rows: spaces.clone(), ..Default::default()
        }));
        SpaceScreen {
            spaces,
            inventory,
            error,
            space_name: String::new(),
            add_space: false,
            node,
            foreign_node,
            claim_ticket: String::new(),
            show_claim: false,
            claim_error: None,
            joining: false,
            publish_status: None,
            invite_status: None,
        }
    }

    pub(crate) fn inventory(&self) -> Spaces { self.inventory.clone() }

    /// The shell's answer to a share link from the join box. A success already rebuilt this
    /// screen with the space listed (`finish_join`), so it lands on a fresh one with nothing
    /// to close; a failure keeps the box open with the reason.
    fn join_answered(&mut self, result: Result<(), String>) {
        self.joining = false;
        match result {
            Ok(()) => {
                self.show_claim = false;
                self.claim_ticket.clear();
            }
            Err(e) => {
                self.show_claim = true;
                self.claim_error = Some(e);
            }
        }
    }

    pub(crate) fn create_workspace(&mut self, vault: &Vault, proxy: &EventLoopProxy<Msg>, value: &str) -> Result<(), String> {
        let name = crate::launcher::creation::name(value)?;
        let account = vault.current().ok_or("vault is locked")?.did;
        let Some(job) = self.inventory.borrow_mut().creation.begin() else { return Ok(()); };
        self.error = None;
        let vault = vault.clone();
        let proxy = proxy.clone();
        if let Err(error) = std::thread::Builder::new().name("workspace-create".into()).spawn(move || {
            let result = vault.create_workspace_for(&account, &name).map_err(|e| e.to_string());
            let _ = proxy.send_event(Msg::WorkspaceCreated { account, job, result });
        }) {
            let error = error.to_string();
            self.inventory.borrow_mut().creation.finish(job, Some(error.clone()));
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn finish_creation(&mut self, vault: &Vault, account: &str, job: u64, result: Result<WorkspaceMeta, String>) {
        if !vault.current().is_some_and(|current| current.did == account) { return; }
        if !self.inventory.borrow_mut().creation.finish(job, result.as_ref().err().cloned()) {
            // An accepted write survives navigation, but its feedback must not touch a new draft/job.
            if let Ok(workspace) = result {
                if !self.spaces.iter().any(|row| row.id == workspace.id) {
                    self.spaces.insert(0, workspace);
                    self.inventory.borrow_mut().rows = self.spaces.clone();
                }
            }
            return;
        }
        match result {
            Ok(workspace) => {
                self.spaces.insert(0, workspace);
                self.inventory.borrow_mut().rows = self.spaces.clone();
                self.space_name.clear();
                self.add_space = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
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
        if let Some(record) = self.node.as_ref().filter(|_| self.foreign_node) {
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
            .child(text("join").font_size(13.0).color(theme::fg_1()));
        if self.show_claim {
            // A phone has no node socket (laptop-host step 8): the link is all it joins by.
            const PLACEHOLDER: &str = if cfg!(target_os = "android") {
                "type a share link"
            } else {
                "paste a share link, a node ticket or an invite"
            };
            let idle = if self.joining { "joining…" } else { "⏎ join · esc cancel" };
            let caption = self.claim_error.as_deref().unwrap_or(idle);
            let caption_color = if self.claim_error.is_some() {
                theme::error()
            } else {
                theme::fg_3()
            };
            let panel = col()
                .w_pct(0.9)
                .max_w(360.0)
                .pad(12.0)
                .gap(8.0)
                .fill(theme::bg_2())
                .stroke(1.0, theme::bd_3())
                .radius(6.0)
                .child(
                    text_input(&self.claim_ticket, "claim_ticket", |s| {
                        Msg::Space(SpaceScreenMsg::ClaimTicket(s))
                    })
                    .placeholder(PLACEHOLDER)
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

    pub fn view(&self, launcher: Option<&Launcher>, warning: Option<&str>) -> El<Msg> {
        let body = launcher.map(Launcher::view)
            .filter(|body| body.contains_id("launcher:workspace-list"));
        if self.spaces.is_empty() && body.is_none() {
            return col()
                .full()
                .center()
                .px(16.0)
                .gap(10.0)
                .child(self.name_input().w_full().max_w(420.0))
                .child(
                    text(self.error.as_deref().unwrap_or("press ⏎ to create"))
                        .font_size(11.0)
                        .no_wrap()
                        .color(theme::fg_3()),
                );
        }

        let lua_creation = body.as_ref().is_some_and(|body| body.contains_id("launcher:workspace-create"));
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
                .w_pct(0.9)
                .max_w(320.0)
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
        let mut header = row().wrap().gap(12.0).child(col().grow());
        if self.node.is_some() {
            header = header
                .child(self.invite_widget())
                .child(self.publish_widget());
        }
        header = header.child(self.claim_widget());
        if !lua_creation { header = header.child(add_btn); }

        let page = col().full().pad(40.0).gap(24.0).child(header)
            .children(self.error.as_deref().or(warning).map(|message| {
                text(message).font_size(12.0).color(theme::error())
            }));
        if let Some(body) = body { return page.child(body); }
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

        page.child(grid)
    }

    pub fn update(
        &mut self,
        msg: SpaceScreenMsg,
        vault: &Vault,
        proxy: &EventLoopProxy<Msg>,
    ) -> Option<Screen> {
        match msg {
            SpaceScreenMsg::AddSpace => {
                self.error = self.create_workspace(vault, proxy, &self.space_name.clone()).err();
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
                self.claim_error = None;
                if is_share_link(&text) {
                    // The shell's own `JoinSpace`; `JoinResult` closes the box or says why not.
                    self.joining = true;
                    let _ = proxy.send_event(Msg::JoinLink(text));
                    return None;
                }
                let vault = vault.clone();
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let link = node::Link::socket();
                    let result = node::join(&link, &vault, &text, node::now_secs());
                    let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::ClaimResult(result)));
                });
                None
            }
            SpaceScreenMsg::ClaimResult(result) => {
                match result {
                    Ok(record) => {
                        self.node = Some(record);
                        self.foreign_node = true;
                        self.show_claim = false;
                        self.claim_ticket.clear();
                        self.claim_error = None;
                    }
                    Err(e) => self.claim_error = Some(e),
                }
                None
            }
            SpaceScreenMsg::JoinResult(result) => {
                self.join_answered(result);
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
                    let link = node::Link::current(&vault);
                    let mut published = 0usize;
                    for meta in &spaces {
                        let result = publish_one(&link, &vault, record.token.clone(), meta);
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
                    let link = node::Link::current(&vault);
                    let result = node::invite(&link, &vault, record.token, "member", Scope::Node)
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
    link: &node::Link,
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
    node::publish(link, vault, token, workspace, published_items).map(|_ack| ())
}

#[cfg(test)]
mod tests;
