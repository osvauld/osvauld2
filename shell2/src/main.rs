//! shell2 — the osvauld desktop shell: accounts over `vault` (signup / unlock / mnemonic-once),
//! workspaces and typed items, app upload, and tabs hosting `app_host::LuaApp`s — one running
//! instance per item, ids namespaced per tab. The kanban app in `src/kanban/` is the reference
//! corpus. The window, GPU, event loop, layout and input live in `runtime`; this crate only
//! describes screens and state. The UDS bridge (`src/bridge.rs`) carries `osvauld-rpc` requests
//! to the UI thread — pure transport; handlers land family by family (docs/status.md, item 1).

mod app_src;
mod bridge;
mod item;
mod login;
mod mnemonic;
mod signup;
mod space;
#[cfg(test)]
mod tests;
mod theme;

use std::{
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    item::{ItemsScreen, ItemsScreenMsg},
    login::{LoginMsg, LoginScreen},
    mnemonic::{Mnemonic, MnemonicMsg},
    space::{SpaceScreen, SpaceScreenMsg},
};
use app_host::{LuaApp, Resolve, Wake, write_source_file as write_source_doc_file};
use base64::Engine as _;
use loro::{Container, LoroDoc, ValueOrContainer};
use osvauld_rpc::{AccountSummary, ItemSummary, Request, Response, WorkspaceSummary};
use runtime::{
    Action, App, CapturedImage, El, ElInfo, EventLoopProxy, ScreenshotRequest, col, row, text,
};
use vault::{ItemKind, PreparedAccount, UnlockedAccount, Vault, WorkspaceItem};

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
    /// One bridge request (`bridge.rs` is pure transport). Executed here, on the UI thread —
    /// the single authority — and the sender is how the reply travels back to the socket.
    Rpc(
        osvauld_rpc::Request,
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
    ),
    /// The auth family's completion: `Signup`/`Unlock` ran Argon2 on a worker (mirroring
    /// the login screen — a Vault clone, mutex-shared), and this is how the result and the
    /// bridge's reply channel come home. The arm answers the client and lands the screen
    /// transition, both on the UI thread.
    AuthDone(std::sync::mpsc::Sender<osvauld_rpc::Response>, AuthOutcome),
    /// Runtime completes this only after the requested frame was painted and read back.
    ScreenshotDone(
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        Result<CapturedImage, String>,
    ),
}

type AuthJob<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// The worker's answer to an auth request: prepared data only. Committing it mutates the
/// active account, so that happens in `Shell::update`, not on the worker.
#[derive(Clone)]
enum AuthOutcome {
    Signup(AuthJob<PreparedAccount>),
    Unlock(AuthJob<UnlockedAccount>),
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

/// The auth family's second half, as a free function so tests can drive it without a
/// window: the worker's result becomes both the bridge reply and the screen transition.
/// A script-side signup skips the mnemonic screen — the script is the mnemonic's reader,
/// the same "shown once" contract with a different pair of eyes.
fn finish_auth(vault: &mut Vault, outcome: AuthOutcome) -> (Response, Option<Screen>) {
    match outcome {
        AuthOutcome::Signup(job) => match job.lock().unwrap().take() {
            Some(Ok(prepared)) => match vault.commit_signup(prepared) {
                Ok((_did, mnemonic)) => (
                    Response::ok(serde_json::json!({ "mnemonic": mnemonic.to_string() })),
                    Some(Screen::Spaces(SpaceScreen::new(vault))),
                ),
                Err(e) => (Response::err(e.to_string()), None),
            },
            Some(Err(e)) => (Response::err(e), None),
            None => (Response::err("auth job already resolved"), None),
        },
        AuthOutcome::Unlock(job) => match job.lock().unwrap().take() {
            Some(Ok(unlocked)) => match vault.commit_login(unlocked) {
                Ok(()) => (
                    Response::ok("unlocked"),
                    Some(Screen::Spaces(SpaceScreen::new(vault))),
                ),
                Err(e) => (Response::err(e.to_string()), None),
            },
            Some(Err(e)) => (Response::err(e), None),
            None => (Response::err("auth job already resolved"), None),
        },
    }
}

/// `Unlock`'s `account` is a did or a label; resolve it on the UI thread so a typo costs
/// no Argon2 and spawns no worker.
fn resolve_did(vault: &Vault, account: &str) -> Result<String, String> {
    let accounts = vault.accounts().map_err(|e| e.to_string())?;
    accounts
        .iter()
        .find(|a| a.did == account || a.label == account)
        .map(|a| a.did.clone())
        .ok_or_else(|| format!("no account named {account:?}"))
}

/// The wire's `kind` strings are the item tags; keep the parse honest so a typo is an
/// error, not a silent default.
fn kind_from_str(kind: &str) -> Result<ItemKind, String> {
    match kind {
        "doc" => Ok(ItemKind::Doc),
        "table" => Ok(ItemKind::Table),
        "app" => Ok(ItemKind::App),
        "canvas" => Ok(ItemKind::Canvas),
        other => Err(format!("unknown kind {other:?} (doc, table, app, canvas)")),
    }
}

/// An item id alone must resolve to its item: ids are 128-bit random, so a scan across
/// workspaces is exact. That scan is the price of id-only wire addressing, and it is
/// small — workspaces and items are local reads.
fn find_item(vault: &Vault, item_id: &str) -> Result<WorkspaceItem, String> {
    for ws in vault.workspaces().map_err(|e| e.to_string())? {
        if let Ok(items) = vault.items(&ws.id) {
            if let Some(wi) = items.into_iter().find(|i| i.id == item_id) {
                return Ok(wi);
            }
        }
    }
    Err(format!("no item {item_id:?}"))
}

/// RPC writes must not leave a showing screen stale: screens hold construction-time
/// snapshots, so a workspace created over the bridge rebuilds the Spaces screen — if that
/// is the one showing. Other screens are untouched.
fn refresh_after_workspace(screen: &mut Screen, vault: &Vault) {
    if matches!(screen, Screen::Spaces(_)) {
        *screen = Screen::Spaces(SpaceScreen::new(vault));
    }
}

/// Same rule for items: only the Items screen snapshotting *this* workspace is rebuilt.
fn refresh_after_item(screen: &mut Screen, vault: &Vault, ws_id: &str) {
    if let Screen::Items(i) = screen {
        if i.ws().id == ws_id {
            let ws = i.ws().clone();
            *screen = Screen::Items(ItemsScreen::new(vault, ws));
        }
    }
}

fn valid_source_path(path: &str) -> Result<(), String> {
    let ok = !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if ok {
        Ok(())
    } else {
        Err(format!("invalid source path {path:?}"))
    }
}

fn source_doc(vault: &Vault, ws_id: &str, item_id: &str) -> Result<LoroDoc, String> {
    let doc = LoroDoc::new();
    if let Some(bytes) = vault.get_src(ws_id, item_id).map_err(|e| e.to_string())? {
        doc.import(&bytes).map_err(|e| e.to_string())?;
    }
    Ok(doc)
}

fn source_files(doc: &LoroDoc) -> Vec<String> {
    let mut out: Vec<String> = doc.get_map("files").keys().map(|k| k.to_string()).collect();
    out.sort();
    out
}

fn read_source_file(doc: &LoroDoc, path: &str) -> Result<String, String> {
    valid_source_path(path)?;
    match doc.get_map("files").get(path) {
        Some(ValueOrContainer::Container(Container::Text(t))) => Ok(t.to_string()),
        Some(_) => Err(format!("{path} is not a text source file")),
        None => Err(format!("no source file {path:?}")),
    }
}

/// `ElInfo` → wire JSON. The runtime stays serde-free by design; this is the one place that
/// knows both shapes. Omits empties so a leaf reads `{"kind":"text","id":"lbl","text":"hi"}`.
fn screenshot_spec(
    width: Option<f32>,
    height: Option<f32>,
    scale: Option<f32>,
) -> Result<(Option<(f32, f32)>, Option<f32>), String> {
    let viewport = match (width, height) {
        (None, None) => None,
        (Some(w), Some(h))
            if w.is_finite()
                && h.is_finite()
                && w > 0.0
                && h > 0.0
                && w <= 8192.0
                && h <= 8192.0 =>
        {
            Some((w, h))
        }
        (Some(_), Some(_)) => return Err("screenshot dimensions must be within 0..=8192".into()),
        _ => return Err("screenshot width and height must be supplied together".into()),
    };
    if scale.is_some_and(|s| !s.is_finite() || !(0.5..=4.0).contains(&s)) {
        return Err("screenshot scale must be within 0.5..=4.0".into());
    }
    Ok((viewport, scale))
}

fn info_json(i: &ElInfo) -> serde_json::Value {
    let mut o = serde_json::Map::new();
    o.insert("kind".into(), i.kind.into());
    if let Some(id) = &i.id {
        o.insert("id".into(), serde_json::Value::String(id.clone()));
    }
    if let Some(t) = &i.text {
        o.insert("text".into(), serde_json::Value::String(t.clone()));
    }
    if !i.handlers.is_empty() {
        o.insert(
            "handlers".into(),
            serde_json::Value::Array(i.handlers.iter().map(|h| (*h).into()).collect()),
        );
    }
    if !i.children.is_empty() {
        o.insert(
            "children".into(),
            serde_json::Value::Array(i.children.iter().map(info_json).collect()),
        );
    }
    serde_json::Value::Object(o)
}

/// The bridge's read-only family. A free function over the vault, not a method on `Shell`,
/// so tests drive it without standing up a window. Writable families land slice by slice;
/// until then they answer with an honest "not wired yet" — `Request`'s `Debug` prints
/// passphrases redacted, so that message is safe wherever it lands.
fn answer(vault: &Vault, req: Request) -> Response {
    match req {
        Request::Ping => Response::ok("pong"),
        Request::ListAccounts => match vault.accounts() {
            Ok(list) => Response::ok(
                list.into_iter()
                    .map(|a| AccountSummary {
                        id: a.did,
                        name: a.label,
                    })
                    .collect::<Vec<_>>(),
            ),
            Err(e) => Response::err(e.to_string()),
        },
        Request::ListWorkspaces => match vault.workspaces() {
            Ok(list) => Response::ok(
                list.into_iter()
                    .map(|w| WorkspaceSummary {
                        id: w.id,
                        name: w.name,
                    })
                    .collect::<Vec<_>>(),
            ),
            Err(e) => Response::err(e.to_string()),
        },
        Request::ListItems { ws_id } => match vault.items(&ws_id) {
            Ok(list) => Response::ok(
                list.into_iter()
                    .map(|i| ItemSummary {
                        id: i.id,
                        ws_id: i.ws_id,
                        name: i.name,
                        kind: i.kind.as_str().to_string(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Err(e) => Response::err(e.to_string()),
        },
        req => Response::err(format!("not wired yet: {req:?}")),
    }
}

struct PendingScreenshot {
    reply: std::sync::mpsc::Sender<Response>,
    item_id: String,
    viewport: Option<(f32, f32)>,
    scale: Option<f32>,
}

struct Shell {
    proxy: EventLoopProxy<Msg>,
    screen: Screen,
    vault: Vault,
    tabs: Vec<Tab>,
    apps: HashMap<Arc<str>, OpenApp>,
    focused: usize,
    error: Option<String>,
    screenshot: Option<PendingScreenshot>,
}
fn main() {
    let data_dir = std::env::var_os("OSVAULD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(vault::default_dir);
    let vault = vault::Vault::open(data_dir).expect("failed to open the osvauld data directory");
    runtime::run_with(|proxy| Shell::new(proxy, vault));
}
impl Shell {
    /// Drop every running app and return to a single Home tab. Every auth transition —
    /// lock, unlock, signup — passes through here: open apps belong to the account that
    /// was active, and the post-update flush must never write their docs into another
    /// account's store. Nothing is lost: every prior update already flushed.
    fn reset_tabs(&mut self) {
        self.tabs = vec![Tab::Home];
        self.apps.clear();
        self.focused = 0;
        self.error = None;
    }
    /// The bridge's stateful family: these touch tabs/apps/screens, so they are methods
    /// rather than the windowless `answer`. All are fast — seal plus a local write, no
    /// KDF — so they run inline on the UI thread (the single authority) and rebuild any
    /// screen that shows what they changed.
    fn answer_mut(&mut self, req: Request) -> Response {
        match req {
            Request::CreateWorkspace { name } => {
                if name.trim().is_empty() {
                    return Response::err("name is required");
                }
                match self.vault.create_workspace(&name) {
                    Ok(w) => {
                        refresh_after_workspace(&mut self.screen, &self.vault);
                        Response::ok(WorkspaceSummary {
                            id: w.id,
                            name: w.name,
                        })
                    }
                    Err(e) => Response::err(e.to_string()),
                }
            }
            Request::CreateItem { ws_id, name, kind } => {
                if name.trim().is_empty() {
                    return Response::err("name is required");
                }
                let kind = match kind_from_str(&kind) {
                    Ok(k) => k,
                    Err(e) => return Response::err(e),
                };
                match self.vault.create_item(&ws_id, &name, kind) {
                    Ok(wi) => {
                        refresh_after_item(&mut self.screen, &self.vault, &ws_id);
                        Response::ok(ItemSummary {
                            id: wi.id,
                            ws_id: wi.ws_id,
                            name: wi.name,
                            kind: wi.kind.as_str().to_string(),
                        })
                    }
                    Err(e) => Response::err(e.to_string()),
                }
            }
            Request::ListFiles { item_id } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    if let Some(o) = self.apps.get(wi.id.as_str()) {
                        Response::ok(o.app.source_files())
                    } else {
                        source_doc(&self.vault, &wi.ws_id, &wi.id)
                            .map(|doc| Response::ok(source_files(&doc)))
                            .unwrap_or_else(Response::err)
                    }
                }
            },
            Request::ReadFile { item_id, path } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    let text = self.apps.get(wi.id.as_str()).and_then(|o| {
                        valid_source_path(&path).ok()?;
                        o.app.read_source_file(&path)
                    });
                    match text {
                        Some(s) => Response::ok(s),
                        None => source_doc(&self.vault, &wi.ws_id, &wi.id)
                            .and_then(|doc| read_source_file(&doc, &path))
                            .map(Response::ok)
                            .unwrap_or_else(Response::err),
                    }
                }
            },
            Request::WriteFile {
                item_id,
                path,
                content,
            } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    if let Err(e) = valid_source_path(&path) {
                        return Response::err(e);
                    }
                    let snapshot = if let Some(o) = self.apps.get(wi.id.as_str()) {
                        o.app.write_source_file(&path, &content)
                    } else {
                        source_doc(&self.vault, &wi.ws_id, &wi.id)
                            .and_then(|doc| write_source_doc_file(&doc, &path, &content))
                    };
                    match snapshot.and_then(|bytes| {
                        self.vault
                            .put_src(&wi.ws_id, &wi.id, &bytes)
                            .map_err(|e| e.to_string())
                    }) {
                        Ok(()) => Response::ok("written"),
                        Err(e) => Response::err(e),
                    }
                }
            },
            Request::ReloadItem { item_id } => match self.apps.get_mut(item_id.as_str()) {
                Some(o) => o
                    .app
                    .reload()
                    .map(|()| Response::ok("reloaded"))
                    .unwrap_or_else(Response::err),
                None => Response::err("item is not open"),
            },
            // ── app actions: resolve by element id on a freshly built view, then route the
            // produced message exactly as the `Msg::Tab` arm would — we are already inside
            // `update`, so recursing into it would run the post-update flush twice.
            Request::DumpTree { item_id } => match self.apps.get_mut(item_id.as_str()) {
                None => Response::err("item is not open"),
                Some(o) => {
                    // Same freshness rule as `fire_on_app`: a dump after a WriteFile must
                    // show the source the next frame would run.
                    let _ = o.app.reload_if_stale();
                    Response::ok(info_json(&o.app.view().info()))
                }
            },
            Request::Click { item_id, el_id } => self.fire_on_app(&item_id, &el_id, Action::Click),
            Request::Type {
                item_id,
                el_id,
                content,
            } => self.fire_on_app(&item_id, &el_id, Action::Type(&content)),
            Request::Key {
                item_id,
                el_id,
                key,
            } => {
                let act = match key.as_str() {
                    "enter" => Action::Enter,
                    "esc" => Action::Esc,
                    k => return Response::err(format!("unknown key {k:?} (enter|esc)")),
                };
                self.fire_on_app(&item_id, &el_id, act)
            }
            // ── senses: the app's live data and its console. Open tabs only — a closed
            // item's data is what ReadFile sees, and its console no longer exists.
            Request::ReadConsole { item_id, last } => match self.apps.get(item_id.as_str()) {
                None => Response::err("item is not open"),
                Some(o) => Response::ok(o.app.console(last)),
            },
            // Screenshot is handled by the deferred `Msg::Rpc` arm, never synchronously.
            Request::Screenshot { .. } => Response::err("screenshot was not deferred"),
            Request::AppDataGet { item_id } => match self.apps.get(item_id.as_str()) {
                None => Response::err("item is not open"),
                Some(o) => Response::ok(o.app.docs_json()),
            },
            // Opening an already-open item focuses its tab — never a second VM for one
            // item. A fresh item with no source yet refuses honestly (WriteFile is its
            // other half).
            Request::OpenItem { item_id } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    let id: Arc<str> = wi.id.as_str().into();
                    match self
                        .tabs
                        .iter()
                        .position(|t| matches!(t, Tab::App((tid, _)) if *tid == id))
                    {
                        Some(pos) => {
                            self.focused = pos;
                            Response::ok("open")
                        }
                        None => self
                            .open_tab(wi)
                            .map(|()| Response::ok("open"))
                            .unwrap_or_else(Response::err),
                    }
                }
            },
            req => answer(&self.vault, req),
        }
    }

    /// Build the app's current view — the same fresh handler registration the next frame
    /// uses, since `view()` re-registers per call — fire `act` on the element with `el_id`,
    /// and route the produced message exactly as the `Msg::Tab` arm does. Not by calling
    /// `update` recursively: we are inside it, and the post-update flush must run once.
    fn fire_on_app(&mut self, item_id: &str, el_id: &str, act: Action) -> Response {
        let Some(o) = self.apps.get_mut(item_id) else {
            return Response::err("item is not open");
        };
        let _ = o.app.reload_if_stale();
        let mut tree = o.app.view();
        match tree.trigger(el_id, act) {
            Ok(Msg::Tab(tid, lua)) => match self.apps.get_mut(&tid) {
                Some(o) => {
                    o.app.update(lua);
                    Response::ok("fired")
                }
                None => Response::err("app message routed to a closed tab"),
            },
            // Unreachable in practice: the app's `to_msg` wraps everything in `Msg::Tab`.
            Ok(_) => Response::err("app produced an unexpected message"),
            Err(e) => Response::err(e),
        }
    }

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
            screenshot: None,
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

    fn ready(&mut self) {
        // Publishing the socket before `run_app` begins polling loses rapid startup events on
        // some winit backends. Runner calls this only after the renderer and event loop are live.
        bridge::spawn(self.proxy.clone());
    }

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
            // Auth: Argon2 must not run on the UI thread, so these two do not go through
            // `answer`. The worker sends `AuthDone`, which replies and transitions — here.
            Msg::Rpc(Request::Signup { name, passphrase }, tx) => {
                if name.trim().is_empty() || passphrase.is_empty() {
                    let _ = tx.send(Response::err("name and passphrase are required"));
                } else {
                    let proxy = self.proxy.clone();
                    let job = Arc::new(Mutex::new(None));
                    let done = job.clone();
                    std::thread::spawn(move || {
                        *done.lock().unwrap() = Some(
                            Vault::prepare_signup(&name, &passphrase).map_err(|e| e.to_string()),
                        );
                        let _ = proxy.send_event(Msg::AuthDone(tx, AuthOutcome::Signup(job)));
                    });
                }
                None
            }
            Msg::Rpc(
                Request::Unlock {
                    account,
                    passphrase,
                },
                tx,
            ) => {
                match resolve_did(&self.vault, &account) {
                    Err(e) => {
                        let _ = tx.send(Response::err(e));
                    }
                    Ok(did) => {
                        let vault = self.vault.clone();
                        let proxy = self.proxy.clone();
                        let job = Arc::new(Mutex::new(None));
                        let done = job.clone();
                        std::thread::spawn(move || {
                            *done.lock().unwrap() = Some(
                                vault
                                    .prepare_login(&did, &passphrase)
                                    .map_err(|e| e.to_string()),
                            );
                            let _ = proxy.send_event(Msg::AuthDone(tx, AuthOutcome::Unlock(job)));
                        });
                    }
                }
                None
            }
            // Instant, no worker: drop the key, tear down every running app (their docs
            // were flushed on the previous update), and face a login screen again — or
            // signup, if this device holds no accounts.
            Msg::Rpc(Request::Lock, tx) => {
                self.vault.lock();
                self.reset_tabs();
                self.screen = match self.vault.accounts() {
                    Ok(list) if !list.is_empty() => Screen::Login(LoginScreen::new(&self.vault)),
                    _ => Screen::Signup(SignupForm::default()),
                };
                let _ = tx.send(Response::ok("locked"));
                None
            }
            Msg::Rpc(
                Request::Screenshot {
                    item_id,
                    width,
                    height,
                    scale,
                },
                reply,
            ) => {
                let spec = screenshot_spec(width, height, scale);
                let focused = self
                    .tabs
                    .get(self.focused)
                    .is_some_and(|t| matches!(t, Tab::App((id, _)) if id.as_ref() == item_id));
                let error = if !self.apps.contains_key(item_id.as_str()) {
                    Some("item is not open".to_string())
                } else if !focused {
                    Some("item is not focused; call OpenItem first".to_string())
                } else if self.screenshot.is_some() {
                    Some("a screenshot is already pending".to_string())
                } else {
                    spec.as_ref().err().cloned()
                };
                if let Some(e) = error {
                    let _ = reply.send(Response::err(e));
                } else {
                    let (viewport, scale) = spec.unwrap();
                    self.screenshot = Some(PendingScreenshot {
                        reply,
                        item_id,
                        viewport,
                        scale,
                    });
                }
                None
            }
            Msg::ScreenshotDone(reply, result) => {
                let response = match result {
                    Ok(image) => Response::ok(serde_json::json!({
                        "png_base64": base64::engine::general_purpose::STANDARD.encode(image.png),
                        "width_px": image.width,
                        "height_px": image.height,
                    })),
                    Err(e) => Response::err(e),
                };
                let _ = reply.send(response);
                None
            }
            Msg::AuthDone(reply, outcome) => {
                let (resp, next) = finish_auth(&mut self.vault, outcome);
                let _ = reply.send(resp);
                // A successful auth may have switched accounts: whatever was running
                // belongs to the previous one, and the flush below must not see it.
                if next.is_some() {
                    self.reset_tabs();
                }
                next
            }
            Msg::Rpc(req, tx) => {
                let _ = tx.send(self.answer_mut(req));
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

    fn take_screenshot(&mut self) -> Option<ScreenshotRequest<Msg>> {
        let pending = self.screenshot.take()?;
        let still_focused = self
            .tabs
            .get(self.focused)
            .is_some_and(|t| matches!(t, Tab::App((id, _)) if id.as_ref() == pending.item_id));
        if !still_focused || !self.apps.contains_key(pending.item_id.as_str()) {
            let _ = pending
                .reply
                .send(Response::err("item stopped being focused before capture"));
            return None;
        }
        Some(ScreenshotRequest {
            viewport: pending.viewport,
            scale: pending.scale,
            complete: Box::new(move |result| Msg::ScreenshotDone(pending.reply, result)),
        })
    }
}
