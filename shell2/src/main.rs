//! shell2 — the osvauld UI. It defines the screens (login, …) and their logic; the window, GPU,
//! event loop, layout, and input all live in the `runtime` crate. `main` just picks the first app.

mod app_src;
mod item;
mod login;
mod mnemonic;
mod signup;
mod space;
#[cfg(test)]
mod tests;
mod theme;

use std::{collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

use crate::{
    item::{ItemsScreen, ItemsScreenMsg},
    login::{LoginMsg, LoginScreen},
    mnemonic::{Mnemonic, MnemonicMsg},
    space::{SpaceScreen, SpaceScreenMsg},
};
use app_host::{LuaApp, Resolve, Wake};
use loro::LoroDoc;
use runtime::{App, El, EventLoopProxy, col, row, text};
use vault::{Vault, WorkspaceItem};

use crate::signup::{SignupForm, SignupMsg};

#[derive(Clone)]
pub enum Msg {
    Signup(SignupMsg),
    Mnemonic(MnemonicMsg),
    Login(LoginMsg),
    Space(SpaceScreenMsg),
    Items(ItemsScreenMsg),
    Tab(Arc<str>, app_host::LuaMsg),
    Focus(usize),
    /// Keyed by item id, not index: closing is destructive, and a stale index would tear down
    /// the wrong app's VM. `Focus` can stay positional because being wrong there is harmless.
    Close(Arc<str>),
    /// A doc changed from outside this window — a peer, the MCP bridge. Carries nothing and
    /// updates nothing: delivering *any* user event runs `update` and then repaints, and the
    /// repaint is the entire point. `view()` compares each doc's version counter against its
    /// own watermark, so the mirror catches up on its own.
    DocChanged,
}
pub enum Screen {
    Signup(SignupForm),
    Mnemonic(Mnemonic),
    Login(LoginScreen),
    Spaces(SpaceScreen),
    Items(ItemsScreen),
}

#[derive(PartialEq)]
enum Tab {
    Home,
    App((Arc<str>, String)),
}
/// A running app plus the workspace it came from. The item id is the map key; `ws_id` has to
/// be kept because `put_doc` is scoped by both and nothing else remembers it once `open_tab`
/// has returned.
struct OpenApp {
    ws_id: String,
    app: LuaApp<Msg>,
}

/// The two halves of doc persistence, as free functions rather than closures built inline.
///
/// This is the only seam between `app_host`, which knows a doc by its name, and the vault,
/// which knows it by `(workspace, item, name)`. `app_host`'s tests stub both sides and the
/// vault's tests exercise the store directly, so the *scoping* — that the pair agree on which
/// two ids a name hangs under — is only ever checked here. Extracting them is what lets a
/// test check it without standing up a window.
fn resolver(vault: &Vault, ws_id: &str, item_id: &str) -> Resolve {
    let (v, ws, it) = (vault.clone(), ws_id.to_string(), item_id.to_string());
    // Not `.ok().flatten()`: a vault read failure must not masquerade as "no doc yet", or the
    // app opens empty and the first flush writes that emptiness over the real board.
    Rc::new(move |name| v.get_doc(&ws, &it, name).map_err(|e| e.to_string()))
}

/// A change from outside the window has to ask for a frame; a click already has one.
///
/// The proxy is the only part of the runtime that is `Send`, which is what makes this the seam:
/// Loro's subscriber demands `Send + Sync` and so cannot hold the VM, the mirror, or anything
/// else in the app. An integer bump and a wake-up are all that can cross, and all that needs to.
fn waker(proxy: &EventLoopProxy<Msg>) -> Wake {
    let proxy = proxy.clone();
    Arc::new(move || {
        let _ = proxy.send_event(Msg::DocChanged);
    })
}

fn persist(
    vault: &Vault,
    ws_id: &str,
    item_id: &str,
) -> impl FnMut(&str, &[u8]) -> Result<(), String> {
    let (v, ws, it) = (vault.clone(), ws_id.to_string(), item_id.to_string());
    move |name, bytes| v.put_doc(&ws, &it, bytes, name).map_err(|e| e.to_string())
}

struct Shell {
    proxy: EventLoopProxy<Msg>,
    screen: Screen,
    vault: Vault,
    tabs: Vec<Tab>,
    apps: HashMap<Arc<str>, OpenApp>,
    focused: usize,
    error: Option<String>,
}
fn main() {
    let data_dir = std::env::var_os("OSVAULD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(vault::default_dir);
    let vault = vault::Vault::open(data_dir).expect("failed to open the osvauld data directory");
    runtime::run_with(|proxy| Shell::new(proxy, vault));
}
impl Shell {
    fn new(proxy: EventLoopProxy<Msg>, vault: Vault) -> Self {
        let screen = if vault.is_empty() {
            Screen::Signup(SignupForm::default())
        } else {
            Screen::Login(LoginScreen::new(&vault))
        };
        let tabs = vec![Tab::Home];
        let shell = Shell {
            vault,
            proxy,
            screen,
            tabs,
            apps: HashMap::new(),
            focused: 0,
            error: None,
        };
        shell
    }
    fn open_tab(&mut self, wi: WorkspaceItem) -> Result<(), String> {
        let src = self
            .vault
            .get_src(&wi.ws_id, &wi.id)
            .map_err(|e| e.to_string())?;
        let Some(src) = src else {
            return Err(format!("{} has no source", wi.name));
        };

        let doc = LoroDoc::new();
        doc.import(&src).map_err(|e| e.to_string())?;
        let id: Arc<str> = wi.id.as_str().into();
        let to_msg_id = id.clone();

        let app = LuaApp::open(
            doc,
            resolver(&self.vault, &wi.ws_id, &wi.id),
            waker(&self.proxy),
            Rc::new(move |msg| Msg::Tab(to_msg_id.clone(), msg)),
        )
        .map_err(|e| e.to_string())?;
        self.apps.insert(
            id.clone(),
            OpenApp {
                ws_id: wi.ws_id.clone(),
                app,
            },
        );
        self.focused = self.tabs.len();
        self.tabs.push(Tab::App((id, wi.name)));
        Ok(())
    }
    /// Tab ids are prefixed (`tab:*`) because the retained store is keyed by `Id` alone — an app
    /// naming an element "Home" would otherwise share this strip's hover/press state.
    fn strip(&self) -> El<Msg> {
        let mut tabs: Vec<El<Msg>> = Vec::new();
        for (idx, tab) in self.tabs.iter().enumerate() {
            let focused = idx == self.focused;
            // Home is pinned and unclosable, so it yields no close target — that `Option` is the
            // only thing distinguishing the two arms downstream.
            let (label, id, close_id) = match tab {
                Tab::Home => ("⌂".to_string(), "tab:home".to_string(), None),
                Tab::App((id, name)) => (name.clone(), format!("tab:{id}"), Some(id.clone())),
            };

            let mut el = row()
                .h(28.0)
                .px(12.0)
                .gap(8.0)
                .radius(6.0)
                .align_center()
                .id(id)
                // `tint` binds the Hover driver, so hover_fill fades in over 120ms instead of
                // snapping — same value the item cards use.
                .tint(120.0)
                .on_click(Msg::Focus(idx));

            el = if focused {
                el.fill(theme::accent_bg())
                    .press_fill(theme::accent_press())
            } else {
                el.hover_fill(theme::bg_2()).press_fill(theme::bg_3())
            };

            let fg = if focused {
                theme::fg_1()
            } else {
                theme::fg_3()
            };
            el = el.child(text(label).font_size(13.0).color(fg));

            // Nested click: the runtime's hit test takes the innermost match (`lib.rs:462` walks
            // the hit list in reverse), so pressing × closes without also focusing.
            if let Some(cid) = close_id {
                el = el.child(
                    row()
                        .size(16.0, 16.0)
                        .radius(4.0)
                        .center()
                        .id(format!("tabclose:{cid}"))
                        .hover_fill(theme::bd_2())
                        .tint(120.0)
                        .on_click(Msg::Close(cid))
                        .child(text("×").font_size(13.0).no_wrap().color(theme::fg_3())),
                );
            }
            tabs.push(el);
        }

        row()
            .w_full()
            .h(38.0)
            .px(8.0)
            .gap(4.0)
            .align_center()
            .fill(theme::bg_1())
            .children(tabs)
    }
}

impl App for Shell {
    type Msg = Msg;

    fn view(&self) -> El<Msg> {
        let content = match &self.tabs[self.focused] {
            Tab::Home => match &self.screen {
                Screen::Signup(f) => f.view(),
                Screen::Mnemonic(m) => m.view(),
                Screen::Login(a) => a.view(),
                Screen::Spaces(s) => s.view(),
                Screen::Items(i) => i.view(),
            },
            Tab::App((id, _name)) => {
                if let Some(o) = self.apps.get(id) {
                    o.app.view()
                } else {
                    text("app not found").color(theme::error())
                }
            }
        };

        // Shell-level chrome, above whatever tab is focused: open failures surface here rather
        // than in a screen, because `open_tab` also fires from MCP where no screen is involved.
        // The tab strip lands in this same wrapper.
        let mut page = col().full();
        if let Some(e) = &self.error {
            page = page.child(
                row()
                    .w_full()
                    .px(40.0)
                    .py(10.0)
                    .fill(theme::bg_1())
                    .align_center()
                    .child(text(e).font_size(12.0).color(theme::error())),
            );
        }

        if matches!(self.screen, Screen::Spaces(_) | Screen::Items(_)) {
            page = page.child(self.strip());
        }
        page.child(content)
    }
    fn update(&mut self, msg: Msg) {
        let next = match msg {
            Msg::Items(ItemsScreenMsg::Open(wi)) => {
                self.error = self.open_tab(wi).err();
                None
            }
            Msg::Tab(id, msg) => {
                if let Some(o) = self.apps.get_mut(&id) {
                    o.app.update(msg);
                }
                None
            }
            Msg::Focus(idx) => {
                if idx < self.tabs.len() {
                    self.focused = idx;
                }
                None
            }
            Msg::Close(id) => {
                let found = self
                    .tabs
                    .iter()
                    .position(|t| matches!(t, Tab::App((tid, _)) if *tid == id));
                if let Some(pos) = found {
                    self.tabs.remove(pos);
                    self.apps.remove(&id); // tear down: VM and doc handle both dropped
                    // Everything after `pos` shifts down one, so a focus at or past it must
                    // follow. Closing the focused tab therefore lands on its left neighbour —
                    // always valid, since Home holds index 0 and can never be the one removed.
                    if self.focused >= pos {
                        self.focused -= 1;
                    }
                }
                None
            }
            // Deliberately empty. The state it announces is already in the doc; what was missing
            // was a frame, and delivering this message is what produced one. The reload check and
            // the flush below then act on the imported change like any other.
            Msg::DocChanged => None,

            msg => match (&mut self.screen, msg) {
                (Screen::Signup(f), Msg::Signup(m)) => f.update(m, &mut self.vault, &self.proxy),
                (Screen::Mnemonic(f), Msg::Mnemonic(m)) => f.update(m),
                (Screen::Login(f), Msg::Login(m)) => f.update(m, &mut self.vault, &self.proxy),
                (Screen::Spaces(s), Msg::Space(m)) => s.update(m, &mut self.vault, &self.proxy),
                (Screen::Items(i), Msg::Items(m)) => i.update(m, &mut self.vault, &self.proxy),
                _ => None,
            },
        };
        // Rebuild any app whose source moved. Here rather than in `view` because reloading needs
        // `&mut self` and `App::view` takes `&self` — but `update` is also the better place on its
        // own terms, since every writer reaches it: an MCP write and a peer arrive as `DocChanged`,
        // and a code block edited in-app moved the source during the message just dispatched.
        //
        // After the match for that last case, and before the flush so a reload that opens a new
        // doc gets it saved in the same pass. The error is reported by the app's own banner; this
        // line is only so a terminal is watching too.
        for o in self.apps.values_mut() {
            if let Some(Err(e)) = o.app.reload_if_stale() {
                eprintln!("reload failed: {e}");
            }
        }

        // Persist after every message, not only Lua ones: MCP and peer writes reach the docs
        // without ever passing through `LuaApp::update`, so this is the single place that sees
        // all three writers. `flush` is a no-op for any doc whose version hasn't moved.
        let vault = self.vault.clone();
        let mut failed = None;
        for (item_id, o) in self.apps.iter_mut() {
            let ws = o.ws_id.clone();
            if let Err(e) = o.app.flush(persist(&vault, &ws, item_id)) {
                failed = Some(format!("save failed: {e}"));
            }
        }
        if failed.is_some() {
            self.error = failed;
        }

        if let Some(next) = next {
            self.screen = next;
        }
    }
}
