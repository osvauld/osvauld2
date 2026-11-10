//! shell2 — the shared osvauld shell: accounts over `vault` (signup / unlock / mnemonic-once),
//! workspaces and typed items, app upload, and tabs hosting `app_host::LuaApp`s — one running
//! instance per item, ids namespaced per tab. The kanban app in `demo_apps/kanban/` is the reference
//! corpus. The window, GPU, event loop, layout and input live in `runtime`; this crate only
//! describes screens and state. The UDS bridge (`src/bridge.rs`) carries `osvauld-rpc` requests
//! to the UI thread — pure transport; handlers land family by family (docs/status.md, item 1).

mod app_src;
mod app_thread;
mod channels;
mod native_rects;
mod binary_files;
mod bridge;
mod bundled;
mod indexer;
mod item;
mod launcher;
mod login;
mod mnemonic;
mod names;
mod clock;
mod host;
mod node;
mod sessions;
mod performance;
mod shell_api;
mod signup;
mod sound;
mod space;
#[cfg(test)]
mod tests;
mod theme;

use std::{
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::{
    app_thread::{AppThread, Open},
    item::{ItemsScreen, ItemsScreenMsg},
    login::{LoginMsg, LoginScreen},
    mnemonic::{Mnemonic, MnemonicMsg},
    space::{SpaceScreen, SpaceScreenMsg},
};
use app_host::{
    DocKey, LuaApp, LuaTest, LuaTestApi, Resolve, SourceEdit,
    edit_source_file as edit_source_doc_file,
    read_source_file_versioned as read_source_doc_file_versioned, run_lua_tests,
    run_lua_tests_with, write_source_file as write_source_doc_file,
};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use courier::DesktopNodeRecord;
use courier::invite::{InviteClaimHello, InviteRequest, desktop_start_invite_claim};
use courier::session::{DocState, Frame, HostEntry};
use courier::sync::{
    Rejected, SyncHello, SyncLayer, SyncReply, desktop_roll_back, desktop_start_sync,
};
use courier::token::{Scope, Token};
use kunki::admin::Admin;
use kunki::push::Push;
use loro::{Container, ExportMode, LoroDoc, ValueOrContainer, VersionVector};
use osvauld_rpc::{
    AccountSummary, EditFileResult, ItemSummary, LuaTestResult, Request, Response,
    SourceActivation, VersionedFile, WorkspaceSummary,
};
use runtime::{
    Action, App, CapturedImage, DriverOp, DriverReport, DriverRequest, El, ElInfo, EventLoopProxy,
    KeyInput, Mods, ScreenshotRequest, TileEvent, WindowFrame, WindowIn, col, row, text, tile,
};
use vault::{
    ASSET_DOC, ItemKind, PreparedAccount, UnlockedAccount, Vault, VaultError, WorkspaceItem,
    WorkspaceMeta,
};

use crate::signup::{SignupForm, SignupMsg};

#[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
static ANDROID_DEBUG_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Enable ADB endpoints before constructing the shell. `root` must be the trusted app-private
/// directory returned by `AndroidApp::internal_data_path`, never a caller-selected import path.
#[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
pub fn configure_android_debug(root: PathBuf) {
    assert!(root.is_absolute());
    let configured = ANDROID_DEBUG_DIR.get_or_init(|| root.clone());
    assert_eq!(configured, &root, "Android debug directory changed within one process");
}

#[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
fn android_debug_dir() -> &'static std::path::Path {
    ANDROID_DEBUG_DIR.get().expect("configure Android debug sockets before starting shell")
}

#[derive(Clone)]
pub enum Msg {
    Signup(SignupMsg),
    Mnemonic(MnemonicMsg),
    Login(LoginMsg),
    Space(SpaceScreenMsg),
    Items(ItemsScreenMsg),
    Launcher(u64, app_host::LuaMsg),
    WorkspaceCreated {
        account: String,
        job: u64,
        result: Result<WorkspaceMeta, String>,
    },
    /// Pointer, wheel or keys for an app's tile, in its coordinates — forwarded to its thread.
    TileInput(Arc<str>, TileEvent),
    /// An app's thread changed what it shows on its own (a bridge call, a wake, a push): paint.
    TileDirty(Arc<str>),
    /// Bring an app back from its own window into its tab.
    Dock(Arc<str>),
    /// An app's thread saved: what changed (to sync), the docs it has open (to subscribe), and
    /// whether the save failed.
    AppSaved {
        id: Arc<str>,
        dirtied: Vec<String>,
        names: Vec<String>,
        error: Option<String>,
    },
    Focus(usize),
    /// Keyed by item id, not index: closing is destructive, and a stale index would tear down
    /// the wrong app's VM. `Focus` can stay positional because being wrong there is harmless.
    Close(Arc<str>),
    ResetLauncher,
    CancelLauncherReset,
    SetFullscreen(bool),
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
    /// The device endpoint bound off-thread after an unlock (laptop-host step 4), tagged with
    /// the sync generation it was asked for: a bind that lands after a lock is closed again.
    NetBound(u64, Result<Slot<net::Net>, String>),
    /// A member's update reached this host (`host.rs`); the UI thread merges it — into the
    /// live doc when the app is open, into the vault when not — and answers through the slot.
    /// A member's update over a session, with the device key the connection proved.
    HostSync(
        u64,
        SyncHello,
        [u8; 32],
        Slot<tokio::sync::oneshot::Sender<Result<SyncReply, String>>>,
    ),
    /// A session to a joined space's host came up, dropped (with why), carried a frame, or
    /// brought the token the host minted for this account's claim.
    /// Each carries `(generation, space, the dial's sequence number, …)`: a message from a
    /// session the shell has since replaced is dropped, whatever it says.
    SessionUp(u64, String, u64),
    SessionDown(u64, String, u64, String),
    SessionFrame(u64, String, u64, Frame),
    SessionToken(u64, String, u64, Token),
    /// App channels (`channels.rs`): a frame from the relay for `(generation, space, dial)` —
    /// dial 0 is this host's own relay — the space's channel stream ending, and what an app's
    /// thread sent.
    NetFrame(u64, String, u64, courier::session::NetFrame),
    NetDown(u64, String, u64),
    NetOut(Arc<str>, Vec<app_host::net::Outgoing>),
    /// The retry ladder says it is time to dial again.
    SessionRedial(u64, String),
    /// `JoinSpace` fetched the space from the link's host; the UI thread adopts it and opens
    /// the claiming session. Answered when the sources have arrived (`PendingJoin`).
    JoinFetched(
        u64,
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        Result<Slot<sessions::Fetched>, String>,
    ),
    /// A share link typed into the Spaces screen's join box: `JoinSpace` without a bridge
    /// caller, answered back to the screen as `SpaceScreenMsg::JoinResult`.
    JoinLink(String),
    /// Runtime completes this only after the driver op actually ran — see `App::take_driver`.
    DriverDone(DriverReply, Result<runtime::DriverReport, String>),
    NativeRects(
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        u64,
        Result<Vec<native_rects::Snapshot>, String>,
    ),
    TestDriver(
        DriverOp,
        std::sync::mpsc::Sender<Result<runtime::DriverReport, String>>,
    ),
    TestWorld(
        Arc<str>,
        std::sync::mpsc::Sender<Result<serde_json::Value, String>>,
    ),
    TestText(
        Arc<str>,
        String,
        std::sync::mpsc::Sender<Result<Option<String>, String>>,
    ),
    TestType(
        Arc<str>,
        String,
        String,
        std::sync::mpsc::Sender<Result<(), String>>,
    ),
    TestRunDone(
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        Vec<osvauld_rpc::LuaTestResult>,
        Arc<str>,
        usize,
    ),
    /// Runtime completes this only after the requested frame was painted and read back.
    ScreenshotDone(
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        Result<CapturedImage, String>,
    ),
    /// Self-rescheduling: syncs every open doc against the claimed node, then arms its own
    /// next tick regardless of what that sync finds — a node that never answers should not
    /// stop later ticks from trying. A no-op with nothing to sync if no node is claimed.
    /// Reads `Shell.node`, not `SpaceScreen`'s own copy: that one is dropped the moment the
    /// screen navigates away from Spaces, and sync has to keep running regardless of screen.
    SyncTick,
    /// One item's sync round trip finished — `(generation, item id, doc name, ack or error)`.
    /// Every node-worker message carries the [`Shell::sync_gen`] it started under; one from an
    /// earlier generation (another account, a lock, a re-claim) is dropped. The import
    /// happens here, on the UI thread, because the doc it targets is not `Send`.
    SyncDone(u64, Arc<str>, String, Result<SyncReply, String>),
    /// A node RPC (`ClaimNode`/`Invite`/`PublishAll`) finished on its worker thread — same
    /// shape as `AuthDone`, and for the same reason: these do socket I/O, so `answer_mut`'s
    /// inline-on-the-UI-thread family is the wrong place for them.
    NodeRpcDone(
        std::sync::mpsc::Sender<osvauld_rpc::Response>,
        NodeRpcOutcome,
    ),
    /// One `Push` arrived on the standing `Listen` connection ([`spawn_push_listener`]) —
    /// the primary delivery path now; `SyncTick` is the backstop.
    PushReceived(u64, Push),
    /// The `Listen` connection (re)opened. Anything pushed while it was down is lost, and a
    /// restarted node may have missed subscribes, so every open doc is re-subscribed and
    /// re-synced as if seen for the first time.
    ListenUp(u64),
    /// A subscribe never reached the node; `SyncTick` retries it.
    SubscribeFailed(u64, Arc<str>, String),
}

#[derive(Clone)]
enum DriverReply {
    Rpc(std::sync::mpsc::Sender<Response>),
    NativeRects(
        std::sync::mpsc::Sender<Response>,
        u64,
        Vec<native_rects::Snapshot>,
    ),
    Test(std::sync::mpsc::Sender<Result<DriverReport, String>>),
}

/// What a node RPC's worker thread hands back. `Claimed` is its own variant rather than
/// folded into a generic `Result<Value, String>` because only it needs `Shell.node` updated —
/// `Invited`/`Published` are pure pass-through replies.
#[derive(Clone)]
enum NodeRpcOutcome {
    Claimed(Result<DesktopNodeRecord, String>),
    Invited(Result<String, String>),
    Published(Result<usize, String>),
    Joined(Result<(), String>),
    Pushed(Result<(), String>),
}

type AuthJob<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// The worker's answer to an auth request: prepared data only. Committing it mutates the
/// active account, so that happens in `Shell::update`, not on the worker.
#[derive(Clone)]
enum AuthOutcome {
    Signup(AuthJob<PreparedAccount>),
    Unlock(AuthJob<UnlockedAccount>),
}

impl AuthOutcome {
    fn succeeded(&self) -> bool {
        match self {
            Self::Signup(job) => matches!(*job.lock().unwrap(), Some(Ok(_))),
            Self::Unlock(job) => matches!(*job.lock().unwrap(), Some(Ok(_))),
        }
    }
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
    Rc::new(move |key| {
        let found = match key {
            DocKey::Own(name) => v.get_doc(&ws, &it, name),
            // An asset not created yet reads as never saved; its first save creates it.
            DocKey::Asset(name) => match v.item_named(&ws, name, ItemKind::Asset) {
                Ok(Some(asset)) => v.get_doc(&ws, &asset.id, ASSET_DOC),
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            },
        };
        found.map_err(|e| e.to_string())
    })
}

/// [`resolver`] for a joined space: a doc the vault lacks is asked of the host over the
/// session before the app may seed it — the same reason as [`resolver_with_node`], over the
/// session's frames. Blocks the app thread for the round trip (a few ms on a LAN, ten seconds
/// at most); the frames queue until the session is up, so an item opened while the dial is
/// still in flight waits rather than seeds.
fn resolver_with_session(vault: &Vault, ws_id: &str, item_id: &str, pull: sessions::Pull) -> Resolve {
    let local = resolver(vault, ws_id, item_id);
    let (v, ws, it) = (vault.clone(), ws_id.to_string(), item_id.to_string());
    Rc::new(move |key| {
        if let Some(bytes) = local(key)? {
            return Ok(Some(bytes));
        }
        let DocKey::Own(name) = key else {
            return Ok(None);
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let slot = (ws.clone(), it.clone(), name.to_string());
        pull.waiters.lock().unwrap().insert(slot.clone(), tx);
        let asked = pull.tx.send(Frame::Update {
            item_id: it.clone(),
            layer: SyncLayer::Doc(name.to_string()),
            vv: VersionVector::new().encode(),
            update: Vec::new(),
        });
        let bytes = match asked {
            Ok(()) => rx.recv_timeout(net::wire::REPLY_TIMEOUT).ok(),
            Err(_) => None,
        };
        pull.waiters.lock().unwrap().remove(&slot);
        // Unreachable is not "nothing there": seeding now would collide with what the host
        // holds. The item opens once the host answers.
        let Some(bytes) = bytes else {
            return Err("the space's host is unreachable; open this item again when it is back".into());
        };
        let doc = LoroDoc::new();
        if doc.import(&bytes).is_err() || doc.oplog_vv().is_empty() {
            return Ok(None);
        }
        let snapshot = doc.export(ExportMode::Snapshot).map_err(|e| e.to_string())?;
        let _ = v.put_doc(&ws, &it, &snapshot, name);
        Ok(Some(snapshot))
    })
}

/// [`resolver`], plus one more thing to check first: if the local vault has nothing yet *and*
/// a node is claimed, pull whatever the node already has before answering `None` — otherwise
/// a doc that looks empty only because nobody has synced it to *this* desktop yet gets treated
/// as never-written, and the app's own first-run seeding logic re-creates default content that
/// someone else already put there (union-merged on the next sync into visible duplicates).
/// A doc genuinely never written by anyone still resolves to `None`, same as `resolver` alone.
fn resolver_with_node(
    vault: &Vault,
    ws_id: &str,
    item_id: &str,
    desktop_did: String,
    token: Token,
) -> Resolve {
    let local = resolver(vault, ws_id, item_id);
    let (v, ws, it) = (vault.clone(), ws_id.to_string(), item_id.to_string());
    Rc::new(move |key| {
        if let Some(bytes) = local(key)? {
            return Ok(Some(bytes));
        }
        let DocKey::Own(name) = key else {
            return Ok(None); // pulling an asset from the node is slice 2c
        };
        let link = node::Link::current(&v);
        Ok(node::pull_doc(
            &link,
            &v,
            &desktop_did,
            token.clone(),
            &ws,
            &it,
            name,
        ))
    })
}

/// A reconciliation backstop now that push ([`spawn_push_listener`]) is the primary delivery
/// path, not the poll itself — seconds-scale, the way `kunki::push`'s own doc comment already
/// says a missed push should be caught, not the tight loop this was before push existed.
const SYNC_INTERVAL: Duration = Duration::from_secs(20);

/// Arms one `SyncTick`, off-thread so a sleeping timer never blocks the UI thread it wakes.
/// Called from `App::ready` for the first tick, and again by `Msg::SyncTick` itself for every
/// tick after — the loop has no other driver.
fn arm_sync_tick(proxy: &EventLoopProxy<Msg>) {
    let proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SYNC_INTERVAL);
        let _ = proxy.send_event(Msg::SyncTick);
    });
}

/// Temporary diagnostic: wall-clock milliseconds, comparable across threads (unlike
/// `Instant`, which is only meaningful within the thread that created it) — this is what lets
/// a "thread started" print and a "proxy.send_event" print on different threads be subtracted
/// against each other after the fact.
fn debug_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// How long to wait before a dropped `Listen` connection tries again. Short and fixed rather
/// than backing off: a node that is down for longer than that is no worse off getting a few
/// wasted connection attempts than it is leaving a desktop silently unlistened forever.
const LISTEN_RETRY: Duration = Duration::from_secs(2);

/// Starts (or restarts) the connection pushes arrive on — one per claimed relationship, held
/// for as long as the process runs. Reconnects on its own after any drop; the caller does not
/// need to notice a disconnect and call this again.
fn spawn_push_listener(
    proxy: &EventLoopProxy<Msg>,
    desktop_did: String,
    token: Token,
    current: Arc<AtomicU64>,
    vault: Vault,
) {
    let proxy = proxy.clone();
    let gen_ = current.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        while current.load(Ordering::SeqCst) == gen_ {
            let link = node::Link::current(&vault);
            match node::listen(&link, &vault, &desktop_did, token.clone()) {
                Ok(mut conn) => {
                    // Registered on the node by now, so nothing synced after this is missed.
                    let _ = proxy.send_event(Msg::ListenUp(gen_));
                    loop {
                        match node::next_push(&mut conn) {
                            Ok(push) => {
                                let _ = proxy.send_event(Msg::PushReceived(gen_, push));
                            }
                            Err(e) => {
                                eprintln!("kunki listen: connection lost: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("kunki listen: could not connect: {e}"),
            }
            std::thread::sleep(LISTEN_RETRY);
        }
    });
}

/// Builds a `SyncHello` against one open doc and, if that succeeds, sends it off-thread —
/// the one piece `SyncTick` and an immediate post-flush push (a local edit reaching the node
/// without waiting for the next tick) both need, so it exists once rather than twice.
fn sync_doc<M: Clone + 'static>(
    proxy: &EventLoopProxy<Msg>,
    vault: &Vault,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &Arc<str>,
    name: &str,
    app: &LuaApp<M>,
    since: Option<&[u8]>,
    gen_: u64,
) {
    let hello = app.with_doc(name, |doc| {
        desktop_start_sync(
            desktop_did,
            token,
            ws_id,
            item_id,
            SyncLayer::Doc(name.to_string()),
            doc,
            since,
        )
    });
    match hello {
        Some(Ok(hello)) => {
            let proxy = proxy.clone();
            let id = item_id.clone();
            let doc_name = name.to_string();
            let vault = vault.clone();
            std::thread::spawn(move || {
                let link = node::Link::current(&vault);
                let result = node::sync(&link, &vault, hello);
                let _ = proxy.send_event(Msg::SyncDone(gen_, id, doc_name, result));
            });
        }
        Some(Err(e)) => eprintln!("sync: could not build hello for {item_id}/{name}: {e}"),
        None => {} // doc closed between the caller finding it and this call
    }
}

/// Whether `new` should replace `old` as the node's known version: an ack that arrives after a
/// later one must not move `since` backwards.
fn newer_vv(old: Option<&Vec<u8>>, new: &[u8]) -> bool {
    let Some(old) = old else { return true };
    match (VersionVector::decode(old), VersionVector::decode(new)) {
        (Ok(old), Ok(new)) => new.includes_vv(&old),
        _ => true,
    }
}

/// Subscribes to one doc's layer the first time it's seen for this item, so a future push
/// delivers its changes — best-effort, off-thread, same as every other node call here. A
/// no-op if this (item, name) pair is already subscribed. True on a first sighting, which the
/// caller answers with a sync: a doc opened from local storage may be behind the node.
fn subscribe_if_new(
    proxy: &EventLoopProxy<Msg>,
    vault: &Vault,
    gen_: u64,
    subscribed: &mut HashMap<Arc<str>, std::collections::HashSet<String>>,
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &Arc<str>,
    name: &str,
) -> bool {
    let first_sighting = subscribed
        .entry(item_id.clone())
        .or_default()
        .insert(name.to_string());
    if !first_sighting {
        return false;
    }
    let proxy = proxy.clone();
    let link = node::Link::current(&vault);
    let did = desktop_did.to_string();
    let ws_id = ws_id.to_string();
    let id = item_id.clone();
    let layer = SyncLayer::Doc(name.to_string());
    let log_name = name.to_string();
    let vault = vault.clone();
    std::thread::spawn(move || {
        if let Err(e) = node::subscribe(&link, &vault, &did, token, &ws_id, &id, layer) {
            eprintln!("subscribe: {id}/{log_name}: {e}");
            let _ = proxy.send_event(Msg::SubscribeFailed(gen_, id, log_name));
        }
    });
    true
}

/// Open an app's index and wire it in. A failure is the app's to see — in its console, and in
/// `search.query`'s error — never a reason not to open the app.
fn attach_index<M: Clone + 'static>(
    app: &mut LuaApp<M>,
    opened: Result<indexer::ItemIndex, String>,
) -> Option<indexer::Shared> {
    let shared = match opened {
        Ok(ix) => Rc::new(std::cell::RefCell::new(ix)),
        Err(e) => {
            app.note(format!("search: the index did not open: {e}"));
            return None;
        }
    };
    match indexer::attach(&shared, app) {
        Ok(()) => Some(shared),
        Err(e) => {
            app.note(format!("search: {e}"));
            None
        }
    }
}

fn reindex<M: Clone + 'static>(
    index: &Option<indexer::Shared>,
    app: &LuaApp<M>,
    dirtied: &[String],
) {
    if let Some(ix) = index {
        if let Err(e) = indexer::index_dirty(ix, app, dirtied) {
            app.note(format!("search: {e}"));
        }
    }
}

/// A bridge request against an open app, on its thread.
fn serve(h: &mut app_thread::Hosted, req: Request, vault: &Vault) -> Response {
    let (ws, item) = (h.ws_id.clone(), h.item_id.to_string());
    match req {
        Request::ListFiles { .. } => Response::ok(h.app.source_files()),
        Request::ReadFile { path, .. } => {
            let live = valid_source_path(&path)
                .ok()
                .and_then(|()| h.app.read_source_file(&path));
            match live {
                Some(s) => Response::ok(s),
                None => source_doc(vault, &ws, &item)
                    .and_then(|doc| read_source_file(&doc, &path))
                    .map(Response::ok)
                    .unwrap_or_else(Response::err),
            }
        }
        Request::ReadFileVersioned { path, .. } => {
            if let Err(e) = valid_source_path(&path) {
                return Response::err(e);
            }
            h.app
                .read_source_file_versioned(&path)
                .map(|f| {
                    Response::ok(VersionedFile {
                        content: f.content,
                        revision: f.revision,
                    })
                })
                .unwrap_or_else(Response::err)
        }
        Request::EditFile {
            path,
            expected_revision,
            edits,
            ..
        } => {
            if let Err(e) = valid_source_path(&path) {
                return Response::err(e);
            }
            let edits = match source_edits(edits) {
                Ok(e) => e,
                Err(e) => return Response::err(e),
            };
            let saved = h
                .app
                .edit_source_file(&path, &expected_revision, &edits)
                .and_then(|edited| {
                    vault
                        .put_src(&ws, &item, &edited.snapshot)
                        .map_err(|e| e.to_string())?;
                    Ok(edited.revision)
                });
            match saved {
                Err(e) => Response::err(e),
                Ok(revision) => Response::ok(EditFileResult {
                    revision,
                    persisted: true,
                    activation: match h.app.reload_if_stale() {
                        Some(Err(error)) => SourceActivation::Failed { error },
                        Some(Ok(())) | None => SourceActivation::Activated,
                    },
                }),
            }
        }
        Request::WriteFile { path, content, .. } => {
            if let Err(e) = valid_source_path(&path) {
                return Response::err(e);
            }
            match h
                .app
                .write_source_file(&path, &content)
                .and_then(|bytes| vault.put_src(&ws, &item, &bytes).map_err(|e| e.to_string()))
            {
                Ok(()) => Response::ok("written"),
                Err(e) => Response::err(e),
            }
        }
        Request::WriteBinaryFile { path, base64, .. } => {
            let bytes = match valid_source_path(&path).and_then(|()| binary_files::decode(&base64)) {
                Ok(b) => b,
                Err(e) => return Response::err(e),
            };
            match h
                .app
                .bind_binary_file(&path, &bytes, binary_files::store(vault, &ws, &item))
            {
                Ok(bound) => binary_files::finish(vault, &ws, &item, bound),
                Err(e) => Response::err(e),
            }
        }
        Request::ListBinaryFiles { .. } => binary_files::list(h.app.binary_files()),
        Request::ReadBinaryFile { path, .. } => {
            let file = h.app.binary_files().into_iter().find(|f| f.path == path);
            binary_files::read(vault, &ws, &item, file)
        }
        Request::ReloadItem { .. } => h
            .app
            .reload()
            .map(|()| Response::ok("reloaded"))
            .unwrap_or_else(Response::err),
        Request::DumpTree { .. } => {
            // A dump after a WriteFile must show the source the next frame would run.
            let _ = h.app.reload_if_stale();
            let mut tree = info_json(&h.app.view().info());
            let worlds = h.app.inspect_worlds();
            let worlds = worlds
                .iter()
                .map(|(id, w)| (id.as_str(), world_json(w)))
                .collect();
            add_worlds(&mut tree, &worlds);
            let worlds3d = h.app.inspect_worlds3d();
            if !worlds3d.is_empty() {
                tree["worlds3d"] = worlds3d
                    .iter()
                    .map(|(id, w)| (id.clone(), world3d_json(w)))
                    .collect();
            }
            Response::ok(tree)
        }
        Request::Click { el_id, .. } => fire(h, &el_id, Action::Click),
        Request::Type { el_id, content, .. } => fire(h, &el_id, Action::Type(&content)),
        Request::Key { el_id, key, .. } => match key.as_str() {
            "enter" => fire(h, &el_id, Action::Enter),
            "esc" => fire(h, &el_id, Action::Esc),
            k => Response::err(format!("unknown key {k:?} (enter|esc)")),
        },
        Request::ReadConsole { last, .. } => Response::ok(h.app.console(last)),
        Request::AppDataGet { .. } => Response::ok(h.app.docs_json()),
        Request::Search { query, limit, .. } => match &h.index {
            None => Response::err("this item has no search index"),
            Some(ix) => {
                let ix = ix.borrow();
                match ix.query(&query, limit.unwrap_or(20).clamp(1, 500)) {
                    Err(e) => Response::err(e),
                    Ok(hits) => Response::ok(serde_json::json!({
                        "hits": hits.iter().map(|h| serde_json::json!({
                            "doc": h.doc, "id": h.id, "score": h.score, "snippet": h.snippet,
                        })).collect::<Vec<_>>(),
                        "fields_runs": ix.fields_runs,
                    })),
                }
            }
        },
        _ => Response::err("not an app request"),
    }
}

/// Build the app's current view — the same fresh handler registration the next frame uses,
/// since `view()` re-registers per call — fire `act` on the element with `el_id`, and apply
/// the message it produces.
fn fire(h: &mut app_thread::Hosted, el_id: &str, act: Action) -> Response {
    let _ = h.app.reload_if_stale();
    let mut tree = h.app.view();
    match tree.trigger(el_id, act) {
        Ok(msg) => {
            h.app.update(msg);
            Response::ok("fired")
        }
        Err(e) => Response::err(e),
    }
}

/// The bridge's edits, bounded.
fn source_edits(edits: Vec<osvauld_rpc::SourceTextEdit>) -> Result<Vec<SourceEdit>, String> {
    let edit_bytes = edits.iter().fold(0usize, |total, e| {
        total
            .saturating_add(e.old_text.len())
            .saturating_add(e.new_text.len())
    });
    if edits.len() > 128 || edit_bytes > 1024 * 1024 {
        return Err("source edit batch is too large".into());
    }
    Ok(edits
        .into_iter()
        .map(|e| SourceEdit {
            old_text: e.old_text,
            new_text: e.new_text,
        })
        .collect())
}

fn persist(
    vault: &Vault,
    ws_id: &str,
    item_id: &str,
) -> impl FnMut(&DocKey, &[u8]) -> Result<(), String> {
    let (v, ws, it) = (vault.clone(), ws_id.to_string(), item_id.to_string());
    move |key, bytes| {
        let saved = match key {
            DocKey::Own(name) => v.put_doc(&ws, &it, bytes, name),
            DocKey::Asset(name) => asset_item(&v, &ws, name)
                .and_then(|asset| v.put_doc(&ws, &asset.id, bytes, ASSET_DOC)),
        };
        saved.map_err(|e| e.to_string())
    }
}

/// The asset named `name` in `ws_id`, created on its first save — an app makes an asset by
/// writing to it. Two peers doing so at once each make one; [`Vault::item_named`]'s
/// oldest-wins picks the same one everywhere.
fn asset_item(vault: &Vault, ws_id: &str, name: &str) -> Result<WorkspaceItem, VaultError> {
    match vault.item_named(ws_id, name, ItemKind::Asset)? {
        Some(asset) => Ok(asset),
        None => vault.create_item(ws_id, name, ItemKind::Asset),
    }
}

/// The auth family's second half, as a free function so tests can drive it without a
/// window: the worker's result becomes both the bridge reply and the screen transition.
/// A script-side signup skips the mnemonic screen — the script is the mnemonic's reader,
/// the same "shown once" contract with a different pair of eyes.
/// Whether a screen's move from `before` to `next` is an unlock: the lock screen's login or
/// a signup's "I've saved this phrase" landing on Spaces with the vault open. The bridge's
/// unlock goes through `AuthDone` instead; both start the same node, listener and endpoint,
/// or a device unlocked by hand (a phone has no bridge) never comes online.
fn unlocks(before: &Screen, next: Option<&Screen>) -> bool {
    matches!(before, Screen::Login(_) | Screen::Mnemonic(_))
        && matches!(next, Some(Screen::Spaces(_)))
}

fn finish_auth(vault: &mut Vault, outcome: AuthOutcome) -> (Response, Option<Screen>) {
    match outcome {
        AuthOutcome::Signup(job) => match job.lock().unwrap().take() {
            Some(Ok(prepared)) => match vault.commit_signup(prepared) {
                Ok((_did, mnemonic)) => {
                    // A failed install leaves a working account; the signup still succeeded.
                    if let Some(dir) = bundled::dir()
                        && let Err(e) = bundled::install(vault, &dir)
                    {
                        eprintln!("bundled apps from {}: {e}", dir.display());
                    }
                    (
                        Response::ok(serde_json::json!({ "mnemonic": mnemonic.to_string() })),
                        Some(Screen::Spaces(SpaceScreen::new(vault))),
                    )
                }
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
        "asset" => Ok(ItemKind::Asset),
        other => Err(format!(
            "unknown kind {other:?} (doc, table, app, canvas, asset)"
        )),
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
            i.refresh(vault);
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

/// This device's Loro peer (`owned-entries.md` §2.1): every doc it writes is stamped with it,
/// so a host can tell its changes from anyone else's. From the per-install device key, never
/// the account's — every install of one account shares that, and Loro allows one writer per
/// peer.
/// The manifest an app's source carries, if it has one that parses.
fn app_manifest(src: &[u8]) -> Option<manifest::Manifest> {
    let doc = LoroDoc::new();
    doc.import(src).ok()?;
    let Some(loro::ValueOrContainer::Container(loro::Container::Text(text))) =
        doc.get_map("files").get("manifest.osv")
    else {
        return None;
    };
    manifest::Manifest::parse(&text.to_string())
        .map_err(|e| eprintln!("manifest: line {}: {}", e.line, e.message))
        .ok()
}

pub(crate) fn device_peer(vault: &Vault) -> Result<loro::PeerID, String> {
    let key = vault.device_public_key().map_err(|e| e.to_string())?;
    Ok(identity::peer_of(&key))
}

/// An item's stored source, as this device's own copy: what the bridge edits and writes
/// back while the item is closed, pushed as this device's changes.
fn source_doc(vault: &Vault, ws_id: &str, item_id: &str) -> Result<LoroDoc, String> {
    let doc = LoroDoc::new();
    if let Some(bytes) = vault.get_src(ws_id, item_id).map_err(|e| e.to_string())? {
        doc.import(&bytes).map_err(|e| e.to_string())?;
    }
    doc.set_peer_id(device_peer(vault)?).map_err(|e| e.to_string())?;
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

fn source_tests(doc: &LoroDoc, filter: Option<&str>) -> Result<Vec<LuaTest>, String> {
    source_files(doc)
        .into_iter()
        .filter(|p| p.starts_with("tests/") && p.ends_with(".lua"))
        .filter(|p| filter.map_or(true, |f| p.contains(f)))
        .map(|p| read_source_file(doc, &p).map(|source| LuaTest { name: p, source }))
        .collect()
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

fn scene3d_json(scene: &runtime::scene3d::SceneInspection) -> serde_json::Value {
    let objects = scene
        .objects
        .iter()
        .map(|object| {
            serde_json::json!({
                "id": object.id,
                "mesh": match object.mesh {
                    runtime::scene3d::MeshKind::Cube => "cube",
                    runtime::scene3d::MeshKind::Triangles => "triangles",
                },
                "mesh_resource": object.mesh_resource,
                "vertices": object.vertex_count,
                "triangles": object.triangle_count,
                "mesh_bytes": object.mesh_bytes,
                "local_bounds": { "min": object.local_bounds.0, "max": object.local_bounds.1 },
                "position": object.position,
                "rotation": object.rotation,
                "scale": object.scale,
                "color": object.color,
                "texture": object.texture.map(|(resource, width, height)| serde_json::json!({
                    "resource": resource, "width": width, "height": height,
                })),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "camera": {
            "eye": scene.eye,
            "target": scene.target,
            "up": scene.up,
            "fov_y_radians": scene.fov_y_radians,
            "near": scene.near,
            "far": scene.far,
        },
        "objects": objects,
    })
}

fn rects_json(report: DriverReport) -> serde_json::Value {
    serde_json::Value::Array(
        report
            .rects
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id, "x": r.x, "y": r.y, "w": r.w, "h": r.h, "hits": r.hits,
                })
            })
            .collect(),
    )
}

fn worlds_json(
    worlds: &std::collections::HashMap<String, world::WorldInspection>,
) -> serde_json::Value {
    serde_json::Value::Object(
        worlds
            .iter()
            .map(|(id, w)| (id.clone(), world_json(w)))
            .collect(),
    )
}

fn world3d_json(world: &world::world3d::WorldInspection3d) -> serde_json::Value {
    use world::world3d::Shape3d;
    let entities: Vec<_> = world.entities.iter().map(|e| {
        let shape = match &e.authored.shape {
            Shape3d::Sphere(radius) => serde_json::json!({ "sphere": radius }),
            Shape3d::Box(size) => serde_json::json!({ "box": size }),
            Shape3d::Mesh(mesh) => serde_json::json!({ "mesh": {
                "resource": mesh.resource_id(), "vertices": mesh.vertex_count(),
                "triangles": mesh.triangle_count(),
            } }),
        };
        serde_json::json!({
            "authored": { "id": e.authored.id, "shape": shape,
                "position": e.authored.position, "rotation": e.authored.rotation,
                "dynamic": e.authored.dynamic, "sensor": e.authored.sensor },
            "zones": e.zones,
            "mass":e.mass, "properties":{"mass":e.properties.mass,"center":e.properties.center,
                "friction":e.properties.friction,"restitution":e.properties.restitution},
            "resolved": { "position": e.resolved.position, "rotation": e.resolved.rotation,
                "velocity": e.resolved.velocity, "angular_velocity": e.resolved.angular_velocity,
                "sleeping": e.resolved.sleeping },
        })
    }).collect();
    let wheels:Vec<_>=world.wheels.iter().map(|w| {
        let motor=w.input.motor.map(|m|serde_json::json!({"velocity":m.velocity,"max_torque":m.max_torque}));
        serde_json::json!({"id":w.spec.id,"body":w.spec.body,"position":w.position,"rotation":w.rotation,
            "contact":w.contact,"suspension_length":w.suspension_length,"force":w.force,
            "steering":w.steering,"angle":w.angle,
            "input":{"torque":w.input.torque,"brake":w.input.brake,"steering":w.input.steering,"motor":motor},
            "authored":{"at":w.spec.at,"radius":w.spec.radius,"width":w.spec.width,"rest":w.spec.rest,
                "travel":w.spec.travel,"stiffness":w.spec.stiffness,"damping":w.spec.damping,"grip":w.spec.grip}})
    }).collect();
    serde_json::json!({"tick":world.tick,"gravity":world.gravity,"dropped_seconds":world.dropped_seconds,
        "dropped_zone_events":world.dropped_zone_events,"entities":entities,"wheels":wheels})
}

fn world_json(world: &world::WorldInspection) -> serde_json::Value {
    let entities = world.entities.iter().map(|e| {
        let clip = e
            .clip
            .as_ref()
            .map(|c| serde_json::json!({ "time": c.time, "length": c.length, "looped": c.looped }));
        let attached = e
            .attached
            .as_ref()
            .map(|(to, part)| serde_json::json!({ "to": to, "part": part }));
        serde_json::json!({
            "id": e.id, "pos": [e.pos.0, e.pos.1], "body": e.body,
            "velocity": [e.velocity.0, e.velocity.1], "rot": e.rot, "spin": e.spin,
            "attached": attached, "zones": e.zones, "clip": clip, "particles": e.particles,
        })
    });
    let timers = world
        .timers
        .iter()
        .map(|t| serde_json::json!({ "name": t.name, "left": t.left }));
    let camera = world.camera.as_ref().map(|c| {
        serde_json::json!({
            "at": [c.at.0, c.at.1], "follow": c.follow, "lost": c.lost, "ease": c.ease,
            "bounds": c.bounds.map(|(x, y, w, h)| [x, y, w, h]), "view": [c.view.0, c.view.1],
            "drawn": c.drawn,
        })
    });
    serde_json::json!({ "entities": entities.collect::<Vec<_>>(), "timers": timers.collect::<Vec<_>>(),
        "tick": world.tick, "dropped": world.dropped, "camera": camera })
}

/// A world draws as one frame element; its entities go on that element, found by the world's id.
fn add_worlds(tree: &mut serde_json::Value, worlds: &HashMap<&str, serde_json::Value>) {
    let Some(node) = tree.as_object_mut() else {
        return;
    };
    let id = node.get("id").and_then(|id| id.as_str());
    if let Some(world) = id.and_then(|id| worlds.get(id)) {
        node.insert("world".into(), world.clone());
    }
    if let Some(children) = node.get_mut("children").and_then(|c| c.as_array_mut()) {
        children
            .iter_mut()
            .for_each(|child| add_worlds(child, worlds));
    }
}

fn text_by_id(i: &ElInfo, id: &str) -> Option<String> {
    if i.id.as_deref() == Some(id) {
        return i.text.clone();
    }
    i.children.iter().find_map(|c| text_by_id(c, id))
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
    if let Some(scene) = &i.scene3d {
        o.insert("scene3d".into(), scene3d_json(scene));
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

/// A driver op waiting for the Runner to run it. Same deferral as `PendingScreenshot`: the reply
/// travels with the op because the answer is not known until the Runner has finished.
struct PendingDriver {
    reply: DriverReply,
    op: DriverOp,
}

struct PendingScreenshot {
    reply: std::sync::mpsc::Sender<Response>,
    item_id: Option<String>,
    viewport: Option<(f32, f32)>,
    scale: Option<f32>,
}

pub struct Shell {
    proxy: EventLoopProxy<Msg>,
    screen: Screen,
    vault: Vault,
    tabs: Vec<Tab>,
    /// Every open app, each on its own thread — shown or not.
    apps: HashMap<Arc<str>, AppThread>,
    focused: usize,
    /// The open app shown beside the focused one, if any (`SplitWith`).
    split: Option<Arc<str>>,
    /// Open apps shown in windows of their own (`PopOut`), in the order they left.
    popped: Vec<Arc<str>>,
    /// Offscreen, tiles run on the shell's virtual clock and every paint waits for them.
    offscreen: Option<(f32, f32)>,
    /// The time of the last paint, for a resize that has to wait for a tile's frame.
    clock: f64,
    /// The one mixer every app plays into (`sound.rs`).
    sound: sound::SoundHost,
    performance: Option<runtime::FramePerformance>,
    error: Option<String>,
    reset_launcher_pending: bool,
    fullscreen: bool,
    launcher: launcher::session::Session,
    screenshot: Option<PendingScreenshot>,
    driver: Option<PendingDriver>,
    native_rects_pending: bool,
    /// Mirrors whatever `SpaceScreen` last claimed — kept here too because `SyncTick` has to
    /// read it regardless of which screen is currently showing, and `SpaceScreen` itself is
    /// dropped the moment the user navigates away from Spaces.
    node: Option<DesktopNodeRecord>,
    /// This device's iroh endpoint, bound on its device key while an account is unlocked
    /// (laptop-host step 4). Nothing is served on it yet; step 5 brings the sessions.
    net: Option<net::Net>,
    /// The DID the current push listener registered under, so the next generation can
    /// unregister it and let that thread end.
    listener_did: Option<String>,
    /// Spaces joined from another host, by workspace id (`sessions::load_joined`).
    joined: HashMap<String, sessions::Joined>,
    /// Open sessions to those hosts, by workspace id.
    sessions: HashMap<String, sessions::Handle>,
    /// Where a joined space's next dial goes, from a drop until a session comes up.
    redial: HashMap<String, sessions::Redial>,
    pending_join: HashMap<String, PendingJoin>,
    /// `JoinSpace` requests that arrived before the endpoint was bound.
    join_waiting: Vec<(String, std::sync::mpsc::Sender<Response>)>,
    /// Docs app threads are waiting on from a host (`resolver_with_session`).
    pulls: sessions::Pulls,
    /// `AssignRole`/`RevokeRole` sent down a session, awaiting the host's `RoleDone`.
    pending_role: HashMap<String, PendingRole>,
    /// Doc names already subscribed, per open item — `SyncTick` inserts into this the first
    /// time it sees a name from `open_doc_names()`, so a doc is subscribed once, not every
    /// tick; `Msg::Close` drains an item's entry to know what to unsubscribe.
    subscribed: HashMap<Arc<str>, std::collections::HashSet<String>>,
    /// Subscribes that never reached the node, retried by the next `SyncTick`. Not retried at
    /// once: a node that is down would turn every failure into the next attempt.
    failed_subs: Vec<(Arc<str>, String)>,
    /// The node's version vector from the last ack, per (item, doc): the node provably holds
    /// everything up to it, so the next push is a diff against it. In memory only — the first
    /// sync after a restart pushes full history, which merges as a no-op.
    node_vv: HashMap<(Arc<str>, String), Vec<u8>>,
    /// Bumped by every auth transition and every (re)claim. Node workers and the push listener
    /// tag their messages with the value they started under; stale ones are dropped, and a
    /// listener whose generation has passed stops reconnecting.
    sync_gen: Arc<AtomicU64>,
    channels: channels::Channels,
}
/// A value on its way to the UI thread that `Msg`'s `Clone` cannot carry — a bound endpoint
/// (it owns a runtime), a one-shot reply sender. The handler takes it out of the slot once.
pub struct Slot<T>(Arc<Mutex<Option<T>>>);

// By hand: a derive would ask `T: Clone`, and not being `Clone` is the point of the slot.
impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Slot<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(Mutex::new(Some(value))))
    }
    pub fn take(&self) -> Option<T> {
        self.0.lock().unwrap().take()
    }
}

/// A `JoinSpace` in flight: answered once every item's source has arrived over the first
/// session, so the caller can open an item the moment the reply lands.
struct PendingJoin {
    reply: std::sync::mpsc::Sender<Response>,
    remaining: std::collections::HashSet<String>,
    ws: courier::publish::PublishedWorkspace,
    items: Vec<courier::publish::PublishedItem>,
    /// Carried by every dial until the join is done: a failover to another host claims again.
    claim: InviteClaimHello,
}

/// A role change down a session, awaiting `RoleDone`. `sent_on` is the dial it went down:
/// a failover resends it on the next session, and no session hears it twice.
struct PendingRole {
    reply: std::sync::mpsc::Sender<Response>,
    frame: Frame,
    sent_on: u64,
}

/// The UDP port this install's endpoint binds, remembered per account (a sealed entry).
const PORT_KEY: &str = "net/port";

/// Where a space's sync goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// This account hosts it: straight into its own admin.
    Here,
    /// A kunki node over its unix socket (pre-pivot; the chat tests' node mode).
    Socket,
    /// Joined from another host: a session over iroh.
    Session,
}

/// `--offscreen WxH` — run with no window, for a bridge client driving the shell. The size is in
/// logical points, which offscreen are also pixels. A flag rather than an env var (the other two
/// knobs are env vars) so that `ps` answers "is this the windowless one?".
fn offscreen_viewport() -> Option<(f32, f32)> {
    let mut args = std::env::args().skip(1);
    let size = loop {
        match args.next() {
            Some(a) if a == "--offscreen" => break args.next(),
            Some(a) => match a.strip_prefix("--offscreen=") {
                Some(rest) => break Some(rest.to_string()),
                None => continue,
            },
            None => return None,
        }
    };
    let size =
        size.unwrap_or_else(|| panic!("--offscreen needs a size, e.g. --offscreen 1280x800"));
    let (w, h) = size
        .split_once(['x', 'X'])
        .unwrap_or_else(|| panic!("--offscreen wants WxH, got {size:?}"));
    let parse = |s: &str, axis| {
        s.trim()
            .parse::<f32>()
            .unwrap_or_else(|e| panic!("--offscreen {axis} {s:?}: {e}"))
    };
    Some((parse(w, "width"), parse(h, "height")))
}

pub fn run_desktop() {
    let data_dir = std::env::var_os("OSVAULD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(vault::default_dir);
    let offscreen = offscreen_viewport();
    let vault = vault::Vault::open(data_dir).expect("failed to open the osvauld data directory");
    match offscreen {
        Some(v) => runtime::run_offscreen(v, |proxy| Shell::with_viewport(proxy, vault, offscreen)),
        None => runtime::run_with(|proxy| Shell::new(proxy, vault)),
    }
}
impl Shell {
    /// Drop every running app and return to a single Home tab. Every auth transition —
    /// lock, unlock, signup — passes through here: open apps belong to the account that
    /// was active, and the post-update flush must never write their docs into another
    /// account's store. Nothing is lost: every prior update already flushed.
    fn sync_one(&mut self, item_id: &Arc<str>, name: &str) {
        let Some(ws) = self.apps.get(item_id).map(|a| a.ws_id.clone()) else {
            return;
        };
        match self.route(&ws) {
            Route::Session => return self.session_sync_one(&ws, item_id, name),
            Route::Here => return self.here_fan_out(item_id, name),
            Route::Socket => {}
        }
        let (Some(record), Some(a)) = (&self.node, self.apps.get(item_id)) else {
            return;
        };
        let Some(did) = self.vault.with_signer(|d| d.did().to_string()) else {
            return;
        };
        let since = self.node_vv.get(&(item_id.clone(), name.to_string())).cloned();
        let (proxy, vault, token, ws, id, name, gen_) = (
            self.proxy.clone(),
            self.vault.clone(),
            record.token.clone(),
            a.ws_id.clone(),
            item_id.clone(),
            name.to_string(),
            self.gen_(),
        );
        a.call(move |h| {
            sync_doc(
                &proxy,
                &vault,
                &did,
                token,
                &ws,
                &id,
                &name,
                &h.app,
                since.as_deref(),
                gen_,
            )
        });
    }

    /// The backstop pass: retry any subscribe that failed, and sync every open doc.
    fn reconcile(&mut self) {
        for (id, name) in self.failed_subs.drain(..) {
            if let Some(names) = self.subscribed.get_mut(&id) {
                names.remove(&name);
            }
        }
        let open: Vec<(Arc<str>, Vec<String>)> = self
            .apps
            .iter()
            .map(|(id, a)| (id.clone(), a.doc_names.clone()))
            .collect();
        for (id, names) in open {
            self.sync_open(&id, &names);
        }
    }

    /// Tear down an item's tab and unsubscribe its docs. False if it was not open.
    fn close_item(&mut self, id: Arc<str>) -> bool {
        let found = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::App((tid, _)) if *tid == id));
        let Some(pos) = found else {
            return false;
        };
        if self.focused == pos { self.fullscreen = false; }
        self.node_vv.retain(|(item, _), _| *item != id);
        self.tabs.remove(pos);
        self.split = self.split.take().filter(|s| *s != id);
        self.popped.retain(|p| *p != id);
        let closed = self.apps.remove(&id); // tear down: VM and doc handle both dropped
        if let Some(o) = &closed {
            let ws = o.ws_id.clone();
            self.net_leave(&id, &ws);
        }
        if let (Some(record), Some(names), Some(o)) = (
            self.node.clone(),
            self.subscribed.remove(&id),
            closed.as_ref(),
        ) {
            if self.joined.contains_key(&o.ws_id) {
                // A session carries no subscriptions to undo; it closes with the last open item.
                let ws = o.ws_id.clone();
                if !self.apps.values().any(|a| a.ws_id == ws) {
                    self.sessions.remove(&ws);
                }
            } else if let Some(desktop_did) = self.vault.with_signer(|d| d.did().to_string()) {
                let link = node::Link::current(&self.vault);
                let (did, ws_id, item_id) = (desktop_did, o.ws_id.clone(), id.to_string());
                let vault = self.vault.clone();
                std::thread::spawn(move || {
                    for name in names {
                        let layer = SyncLayer::Doc(name.clone());
                        if let Err(e) = node::unsubscribe(
            &link,
                            &vault,
                            &did,
                            record.token.clone(),
                            &ws_id,
                            &item_id,
                            layer,
                        ) {
                            eprintln!("unsubscribe: {item_id}/{name}: {e}");
                        }
                    }
                });
            }
        }
        // Everything after `pos` shifts down one, so a focus at or past it must
        // follow. Closing the focused tab therefore lands on its left neighbour —
        // always valid, since Home holds index 0 and can never be the one removed.
        if self.focused >= pos {
            self.focused -= 1;
        }
        true
    }

    /// A member's update, judged by this host's records and merged into the live doc when
    /// the app is open, into the vault when not. Answered through `reply`. `device` is the
    /// key the session proved the member holds: its changes must be in that device's name.
    fn host_sync(
        &mut self,
        gen_: u64,
        hello: SyncHello,
        device: [u8; 32],
        reply: tokio::sync::oneshot::Sender<Result<SyncReply, String>>,
    ) {
        if gen_ != self.gen_() {
            let _ = reply.send(Err("account changed".into()));
            return;
        }
        let admin = Admin::new(self.vault.clone());
        let now = clock::now_secs();
        let pusher = self.pusher();
        let id: Arc<str> = hello.item_id.as_str().into();
        // One copy per doc: an open app's live doc is the host's copy of it.
        let live = match (&hello.layer, self.apps.get(&id)) {
            (SyncLayer::Doc(name), Some(a))
                if a.ws_id == hello.ws_id && a.doc_names.contains(name) =>
            {
                Some((a, name.clone()))
            }
            _ => None,
        };
        match live {
            Some((a, name)) => a.call(move |h| {
                let judged = match admin.judge(&hello, Some(device), now) {
                    Ok(judged) => judged,
                    Err(e) => {
                        let _ = reply.send(Err(e.to_string()));
                        return;
                    }
                };
                let result = h.app.with_doc(&name, |doc| {
                    admin.accept_live(doc, &hello, &judged, now, &pusher)
                });
                let _ = reply.send(match result {
                    Some(Ok(outcome)) => Ok(outcome),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Err("doc closed".into()),
                });
            }),
            None => {
                // Inline, on this thread: one merge at a time, and never while an app for this
                // doc is opening (also here) — the vault keeps one writer per doc.
                let outcome = admin
                    .accept_sync(hello, Some(device), now, &pusher)
                    .map_err(|e| e.to_string());
                let _ = reply.send(outcome);
            }
        }
    }

    /// A session ended: the next host is dialled at once; once the whole list has failed,
    /// its apps hear why, what waited on it is answered, and the retry ladder paces the next
    /// round. With one host that is every drop, as it always was.
    fn session_down(&mut self, ws: String, seq: u64, reason: String) {
        let from = match self.sessions.get(&ws) {
            Some(h) if h.seq != seq => return, // an earlier session's ending; a newer one is up or dialling
            Some(h) => h.host_index,
            None => 0,
        };
        self.sessions.remove(&ws);
        let hosts = self.joined.get(&ws).map_or(1, |j| j.host_list().len());
        let next = self
            .redial
            .entry(ws.clone())
            .or_default()
            .after_drop(from, hosts);
        if next.pause.is_some() {
            if let Some(p) = self.pending_join.remove(&ws) {
                let _ = p
                    .reply
                    .send(Response::err(format!("could not reach the host: {reason}")));
            }
            if let Some(p) = self.pending_role.remove(&ws) {
                let _ = p
                    .reply
                    .send(Response::err(format!("could not reach the host: {reason}")));
            }
            self.note_offline(&ws, &reason);
        }
        if !self.session_wanted(&ws) {
            return;
        }
        match next.pause {
            None => self.open_session(&ws),
            Some(pause) => {
                let (proxy, gen_) = (self.proxy.clone(), self.gen_());
                std::thread::spawn(move || {
                    std::thread::sleep(pause);
                    let _ = proxy.send_event(Msg::SessionRedial(gen_, ws));
                });
            }
        }
    }

    /// Every session, host list and request-in-flight of the account that is going away,
    /// answered with `why`: a pending join or role change must never be replayed down the
    /// next account's session with its credentials.
    fn drop_pending(&mut self, why: &str) {
        self.sessions.clear();
        self.joined.clear();
        self.redial.clear();
        for (_, pending) in self.pending_join.drain() {
            let _ = pending.reply.send(Response::err(why));
        }
        for (_, pending) in self.pending_role.drain() {
            let _ = pending.reply.send(Response::err(why));
        }
        for (_, tx) in self.join_waiting.drain(..) {
            let _ = tx.send(Response::err(why));
        }
    }

    /// Whether anything is waiting on a session to `ws`: an open app, or a join or role
    /// change still failing over.
    fn session_wanted(&self, ws: &str) -> bool {
        self.apps.values().any(|a| a.ws_id == ws)
            || self.pending_join.contains_key(ws)
            || self.pending_role.contains_key(ws)
    }

    /// `JoinSpace`'s second half: adopt what the host listed, remember the host, and open
    /// the first session with the invite claim and a pull of every item's source.
    fn join_fetched(
        &mut self,
        gen_: u64,
        tx: std::sync::mpsc::Sender<Response>,
        result: Result<Slot<sessions::Fetched>, String>,
    ) {
        if gen_ != self.gen_() {
            let _ = tx.send(Response::err("account changed"));
            return;
        }
        let fetched = match result.map(|slot| slot.take()) {
            Ok(Some(fetched)) => fetched,
            Ok(None) => return,
            Err(e) => {
                let _ = tx.send(Response::err(e));
                return;
            }
        };
        let now = clock::now_secs();
        let ws = fetched.ws.id.clone();
        let adopted = (|| -> Result<(), String> {
            self.vault
                .adopt_workspace(&WorkspaceMeta {
                    id: ws.clone(),
                    name: fetched.ws.name.clone(),
                    created: fetched.ws.created,
                })
                .map_err(|e| e.to_string())?;
            for item in &fetched.items {
                self.vault
                    .adopt_item(&WorkspaceItem {
                        id: item.id.clone(),
                        ws_id: ws.clone(),
                        name: item.name.clone(),
                        kind: kind_from_str(&item.kind)?,
                        created: item.created,
                    })
                    .map_err(|e| e.to_string())?;
            }
            let joined = sessions::Joined {
                host_did: fetched.invite.node_did.clone(),
                host: fetched
                    .hosts
                    .first()
                    .map(|h| h.addr.clone())
                    .unwrap_or_else(|| fetched.link.host.clone()),
                token: None,
                hosts: fetched.hosts.clone(),
            };
            sessions::save_joined(&self.vault, &ws, &joined)?;
            self.joined.insert(ws.clone(), joined);
            Ok(())
        })();
        if let Err(e) = adopted {
            let _ = tx.send(Response::err(e));
            return;
        }
        let claim = self
            .vault
            .with_signer(|me| desktop_start_invite_claim(fetched.invite.clone(), me, now));
        let claim = match claim {
            Some(Ok(claim)) => claim,
            Some(Err(e)) => {
                let _ = tx.send(Response::err(e.to_string()));
                return;
            }
            None => {
                let _ = tx.send(Response::err("locked"));
                return;
            }
        };
        self.pending_join.insert(
            ws.clone(),
            PendingJoin {
                reply: tx,
                remaining: fetched.items.iter().map(|i| i.id.clone()).collect(),
                ws: fetched.ws.clone(),
                items: fetched.items.clone(),
                claim,
            },
        );
        self.sessions.remove(&ws);
        self.redial.remove(&ws);
        self.open_session(&ws);
    }

    /// `JoinSpace`'s first half: fetch the space from the link's host, off-thread. Asked
    /// before the endpoint is up (right after a signup), it waits for `NetBound`.
    fn join_space(&mut self, link: String, tx: std::sync::mpsc::Sender<Response>) {
        let Some(net) = &self.net else {
            self.join_waiting.push((link, tx));
            return;
        };
        let link = match courier::link::Link::from_text(&link) {
            Ok(link) => link,
            Err(_) => {
                let _ = tx.send(Response::err("not a share link"));
                return;
            }
        };
        let (rt, ep, proxy, gen_) = (
            net.handle().clone(),
            net.endpoint().clone(),
            self.proxy.clone(),
            self.gen_(),
        );
        std::thread::spawn(move || {
            let fetched = sessions::fetch(&rt, ep, link).map(Slot::new);
            let _ = proxy.send_event(Msg::JoinFetched(gen_, tx, fetched));
        });
    }

    /// The join box's `JoinSpace`: the bridge's path with the reply carried back to the
    /// Spaces screen instead of a socket. Waited on from a thread of its own, since the
    /// reply is sent by this very UI thread (`finish_join`) once the sources have arrived.
    fn join_typed(&mut self, link: String) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.join_space(link, tx);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = match rx.recv() {
                Ok(Response::Ok { .. }) => Ok(()),
                Ok(Response::Err { message }) => Err(message),
                Err(_) => Err("the join was dropped".to_string()),
            };
            let _ = proxy.send_event(Msg::Space(SpaceScreenMsg::JoinResult(result)));
        });
    }

    fn session_up(&mut self, ws: String, seq: u64) {
        if self.sessions.get(&ws).is_none_or(|h| h.seq != seq) {
            return; // a replaced session's hello
        }
        if let Some(h) = self.sessions.get_mut(&ws) {
            h.up = true;
        }
        self.redial.remove(&ws);
        // A role change that failed over goes down this session; one queued on it already
        // (the channel buffers before the hello) is not repeated.
        if let (Some(p), Some(h)) = (self.pending_role.get_mut(&ws), self.sessions.get(&ws)) {
            if p.sent_on != seq {
                let _ = h.tx.send(p.frame.clone());
                p.sent_on = seq;
            }
        }
        // A pull of every source a join waits on; then whatever is open re-syncs.
        if let (Some(p), Some(h)) = (self.pending_join.get(&ws), self.sessions.get(&ws)) {
            for item_id in &p.remaining {
                let _ = h.tx.send(Frame::Update {
                    item_id: item_id.clone(),
                    layer: SyncLayer::Src,
                    vv: VersionVector::new().encode(),
                    update: Vec::new(),
                });
            }
            if p.remaining.is_empty() {
                self.finish_join(&ws);
            }
        }
        let open: Vec<(Arc<str>, Vec<String>)> = self
            .apps
            .iter()
            .filter(|(_, a)| a.ws_id == ws)
            .map(|(id, a)| (id.clone(), a.doc_names.clone()))
            .collect();
        for (id, names) in open {
            // A fresh session knows nothing of what the host acked before.
            self.node_vv.retain(|(i, _), _| *i != id);
            self.subscribed.remove(&id);
            self.sync_open(&id, &names);
        }
    }

    fn session_token(&mut self, ws: String, seq: u64, token: Token) {
        if self.sessions.get(&ws).is_none_or(|h| h.seq != seq) {
            return;
        }
        if let Some(joined) = self.joined.get_mut(&ws) {
            joined.token = Some(token);
            let joined = joined.clone();
            if let Err(e) = sessions::save_joined(&self.vault, &ws, &joined) {
                eprintln!("session {ws}: could not keep the token: {e}");
            }
        }
    }

    fn session_frame(&mut self, ws: String, seq: u64, frame: Frame) {
        if self.sessions.get(&ws).is_none_or(|h| h.seq != seq) {
            return;
        }
        match frame {
            Frame::Ack {
                item_id,
                layer: SyncLayer::Doc(name),
                vv,
                missing,
            } => {
                let id: Arc<str> = item_id.as_str().into();
                let key = (id.clone(), name.clone());
                if self.apps.contains_key(&id) {
                    if missing {
                        self.node_vv.remove(&key);
                        self.sync_one(&id, &name);
                    } else if newer_vv(self.node_vv.get(&key), &vv) {
                        self.node_vv.insert(key, vv);
                    }
                }
            }
            Frame::Update {
                item_id,
                layer: SyncLayer::Doc(name),
                update,
                ..
            } => {
                // An app thread waiting on this doc before it opens gets it first.
                let waiter = self
                    .pulls
                    .lock()
                    .unwrap()
                    .remove(&(ws.clone(), item_id.clone(), name.clone()));
                match waiter {
                    Some(tx) => {
                        let _ = tx.send(update);
                    }
                    // Empty: the host's answer to a pull of a doc it holds nothing of.
                    None if update.is_empty() => {}
                    None => {
                        let id: Arc<str> = item_id.as_str().into();
                        self.import(&id, Some(&ws), name, update);
                    }
                }
            }
            Frame::RoleDone { ok, reason } => {
                if let Some(p) = self.pending_role.remove(&ws) {
                    let _ = p.reply.send(if ok {
                        Response::ok("done")
                    } else {
                        Response::err(reason)
                    });
                }
            }
            Frame::Hosts { hosts } => self.hosts_learned(&ws, hosts),
            Frame::Update {
                item_id,
                layer: SyncLayer::Src,
                update,
                ..
            } => self.src_arrived(&ws, &item_id, update),
            Frame::Rejected {
                item_id,
                layer: SyncLayer::Doc(name),
                rejected,
            } => {
                let id: Arc<str> = item_id.as_str().into();
                self.roll_back(&id, name, rejected);
            }
            Frame::Refused { reason } => eprintln!("session {ws}: {reason}"),
            _ => {}
        }
    }

    fn route(&self, ws: &str) -> Route {
        if self.joined.contains_key(ws) {
            return Route::Session;
        }
        match node::Link::current(&self.vault) {
            node::Link::Here(_) => Route::Here,
            node::Link::Socket(_) => Route::Socket,
        }
    }

    fn ensure_session(&mut self, ws: &str) {
        if !self.sessions.contains_key(ws) {
            self.open_session(ws);
        }
    }

    /// The host the space's last drop pointed at (owner first), and whatever a join still
    /// pending needs said again — the claim and a pull of each source it waits on. Silently
    /// nothing until the endpoint is up — `NetBound` dials what is wanted then.
    fn open_session(&mut self, ws: &str) {
        let (Some(net), Some(joined)) = (&self.net, self.joined.get(ws)) else {
            return;
        };
        let list = joined.host_list();
        let host_index = self
            .redial
            .get(ws)
            .map_or(0, |r| r.next)
            .min(list.len() - 1);
        let (claim, docs) = match self.pending_join.get(ws) {
            Some(p) => (
                Some(p.claim.clone()),
                p.remaining
                    .iter()
                    .map(|id| DocState {
                        item_id: id.clone(),
                        layer: SyncLayer::Src,
                        vv: VersionVector::new().encode(),
                    })
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        let handle = sessions::dial(
            net.handle(),
            net.endpoint().clone(),
            self.vault.clone(),
            self.proxy.clone(),
            self.gen_(),
            sessions::Dial {
                ws_id: ws.to_string(),
                joined: joined.clone(),
                host: list[host_index].clone(),
                host_index,
                docs,
                claim,
                pulls: self.pulls.clone(),
            },
        );
        self.sessions.insert(ws.to_string(), handle);
        self.net_reopen(ws);
    }

    /// The host told this member who serves the space (`Frame::Hosts`, owner first): kept,
    /// and the live session re-finds its place in the new order.
    fn hosts_learned(&mut self, ws: &str, hosts: Vec<HostEntry>) {
        let Some(joined) = self.joined.get_mut(ws) else {
            return;
        };
        joined.hosts = hosts;
        if let Some(h) = self.sessions.get_mut(ws) {
            if let Some(i) = joined.host_list().iter().position(|e| e.did == h.host_did) {
                h.host_index = i;
            }
        }
        if let Err(e) = sessions::save_joined(&self.vault, ws, joined) {
            eprintln!("session {ws}: could not keep the hosts: {e}");
        }
    }

    /// `sync_one` for a joined space: the update goes down the session instead of to a node.
    fn session_sync_one(&mut self, ws: &str, item_id: &Arc<str>, name: &str) {
        self.ensure_session(ws);
        let Some(token) = self.joined.get(ws).and_then(|j| j.token.clone()) else {
            return; // no token until the first Welcome; `SessionUp` re-syncs
        };
        let Some(tx) = self.sessions.get(ws).map(|h| h.tx.clone()) else {
            return;
        };
        let Some(a) = self.apps.get(item_id) else { return };
        let Some(did) = self.vault.with_signer(|d| d.did().to_string()) else {
            return;
        };
        let since = self
            .node_vv
            .get(&(item_id.clone(), name.to_string()))
            .cloned();
        let (ws, id, name) = (ws.to_string(), item_id.clone(), name.to_string());
        a.call(move |h| {
            let hello = h.app.with_doc(&name, |doc| {
                desktop_start_sync(
                    &did,
                    token,
                    &ws,
                    &id,
                    SyncLayer::Doc(name.clone()),
                    doc,
                    since.as_deref(),
                )
            });
            match hello {
                Some(Ok(hello)) => {
                    let _ = tx.send(Frame::Update {
                        item_id: hello.item_id,
                        layer: hello.layer,
                        vv: hello.vv,
                        update: hello.update,
                    });
                }
                Some(Err(e)) => eprintln!("session: no update for {id}/{name}: {e}"),
                None => {}
            }
        });
    }

    /// `sync_one` for a space this account hosts: the open doc *is* the host's copy, so there
    /// is nothing to sync it with — only members to tell. Fans the live doc out to every
    /// session subscribed to it, from the app's thread; the app's own flush persists it.
    fn here_fan_out(&mut self, item_id: &Arc<str>, name: &str) {
        let (Some(record), Some(a)) = (&self.node, self.apps.get(item_id)) else {
            return;
        };
        let Some(did) = self.vault.with_signer(|d| d.did().to_string()) else {
            return;
        };
        let admin = Admin::new(self.vault.clone());
        let now = clock::now_secs();
        let pusher = self.pusher();
        let (ws, id, name, token) = (
            a.ws_id.clone(),
            item_id.clone(),
            name.to_string(),
            record.token.clone(),
        );
        a.call(move |h| {
            let result = h.app.with_doc(&name, |doc| -> Result<(), String> {
                let hello = SyncHello {
                    request_id: kunki::host::HERE.to_string(),
                    desktop_did: did,
                    token,
                    ws_id: ws,
                    item_id: id.to_string(),
                    layer: SyncLayer::Doc(name.clone()),
                    vv: doc.oplog_vv().encode(),
                    update: Vec::new(),
                };
                // A hello the host builds for itself: nothing is pushed, nothing to bind.
                let judged = admin.judge(&hello, None, now).map_err(|e| e.to_string())?;
                let snapshot = doc
                    .export(ExportMode::Snapshot)
                    .map_err(|e| e.to_string())?;
                admin
                    .fan_out(&hello, &snapshot, judged.rules.as_ref(), now, &pusher)
                    .map_err(|e| e.to_string())
            });
            if let Some(Err(e)) = result {
                eprintln!("host: fan-out of {id}/{name}: {e}");
            }
        });
    }

    /// An item's source arrived over a session: stored, and if a join was waiting on it, one
    /// fewer to wait for.
    fn src_arrived(&mut self, ws: &str, item_id: &str, update: Vec<u8>) {
        let doc = LoroDoc::new();
        let stored = doc
            .import(&update)
            .map_err(|e| e.to_string())
            .and_then(|_| doc.export(ExportMode::Snapshot).map_err(|e| e.to_string()))
            .and_then(|bytes| {
                self.vault
                    .put_src(ws, item_id, &bytes)
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = stored {
            eprintln!("session {ws}: source for {item_id}: {e}");
            return;
        }
        let done = match self.pending_join.get_mut(ws) {
            Some(p) => {
                p.remaining.remove(item_id);
                p.remaining.is_empty()
            }
            None => false,
        };
        if done {
            self.finish_join(ws);
        }
    }

    fn finish_join(&mut self, ws: &str) {
        let Some(p) = self.pending_join.remove(ws) else { return };
        refresh_after_workspace(&mut self.screen, &self.vault);
        let _ = p
            .reply
            .send(Response::ok(serde_json::json!({ "ws": p.ws, "items": p.items })));
    }

    /// Tells every open app of a joined space that its host is unreachable, in its console.
    fn note_offline(&self, ws: &str, reason: &str) {
        for a in self.apps.values().filter(|a| a.ws_id == ws) {
            let line = format!("host offline: {reason}");
            a.call(move |h| h.app.note(line));
        }
    }

    fn share_space(&self, ws_id: &str) -> Response {
        let Some(net) = &self.net else {
            return Response::err("this device's endpoint is not up yet");
        };
        let (Some(record), Some(did)) = (
            &self.node,
            self.vault.with_signer(|d| d.did().to_string()),
        ) else {
            return Response::err("locked");
        };
        if !node::hosting(&self.vault)
            .unwrap_or_default()
            .iter()
            .any(|w| w == ws_id)
        {
            return Response::err("not a space this account hosts");
        }
        let request = InviteRequest {
            desktop_did: did,
            token: record.token.clone(),
            role: "member".into(),
            scope: Scope::Workspace(ws_id.to_string()),
            public: true,
        };
        let label = self.vault.current().map(|a| a.label).unwrap_or_default();
        let code = match Admin::new(self.vault.clone()).issue_code(&request, &label, clock::now_secs()) {
            Ok(code) => code,
            Err(e) => return Response::err(e.to_string()),
        };
        let host = kunki::host::host_addr_of(&net.addr(Duration::from_millis(500)));
        match (courier::link::Link { host, code }).to_text() {
            Ok(text) => Response::ok(text),
            Err(e) => Response::err(e.to_string()),
        }
    }

    /// `[{did, device, online}]`, owner first. For a joined space, `online` is the host the
    /// live session is on; for one this account hosts, whether each appointed host holds a
    /// host session right now.
    fn hosts(&self, ws_id: &str) -> Response {
        if let Some(joined) = self.joined.get(ws_id) {
            let on = self
                .sessions
                .get(ws_id)
                .filter(|h| h.up)
                .map(|h| h.host_did.clone());
            let list: Vec<serde_json::Value> = joined
                .host_list()
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "did": h.did,
                        "device": URL_SAFE_NO_PAD.encode(h.addr.device),
                        "online": on.as_deref() == Some(h.did.as_str()),
                    })
                })
                .collect();
            return Response::ok(list);
        }
        let live = |did: &str| kunki::host::sessions().has_host(did, ws_id);
        match node::hosts(&self.vault, ws_id, self.net.is_some(), clock::now_secs(), &live) {
            Ok(hosts) => Response::ok(hosts),
            Err(e) => Response::err(e),
        }
    }

    /// `AppointHost`: a `host` token for `did` on a space this account hosts, then whatever
    /// `did` has live becomes a host session and everyone on the space hears the new list.
    fn appoint_host(&self, ws_id: String, did: String, tx: std::sync::mpsc::Sender<Response>) {
        let Some(net) = &self.net else {
            let _ = tx.send(Response::err("this device's endpoint is not up yet"));
            return;
        };
        if !node::hosting(&self.vault)
            .unwrap_or_default()
            .iter()
            .any(|w| *w == ws_id)
        {
            let _ = tx.send(Response::err("not a space this account hosts"));
            return;
        }
        let admin = Admin::new(self.vault.clone());
        if let Err(e) = admin.appoint_host(&ws_id, &did, clock::now_secs()) {
            let _ = tx.send(Response::err(e.to_string()));
            return;
        }
        let ctx = kunki::host::HostCtx {
            vault: self.vault.clone(),
            ep: net.endpoint().clone(),
            sink: self.sink(),
        };
        // Promoting a live session signs the whole ledger and reads every doc of the space:
        // off the UI thread. The mint above is one signature and stays here.
        std::thread::spawn(move || {
            let _ = tx.send(match kunki::host::appointed(&ctx, &ws_id, &did) {
                Ok(()) => Response::ok("appointed"),
                Err(e) => Response::err(e),
            });
        });
    }

    /// Binds this device's endpoint on the unlocked account's device key — off the UI thread,
    /// once per unlock, replacing any earlier one (`Msg::NetBound` lands it). A failure is
    /// logged, not fatal: the shell still runs, it just cannot be dialled.
    fn bind_net(&mut self) {
        self.close_net();
        let secret = match self.vault.device_secret() {
            Ok(secret) => secret,
            Err(e) => {
                eprintln!("net: no device key: {e}");
                return;
            }
        };
        let proxy = self.proxy.clone();
        let gen_ = self.gen_();
        let vault = self.vault.clone();
        std::thread::spawn(move || {
            // The port this install bound last time, so a local link survives a restart; a
            // busy port falls back to any, and whatever was bound is what gets remembered.
            let wanted = vault
                .get_entry(PORT_KEY)
                .ok()
                .flatten()
                .and_then(|b| b.try_into().ok().map(u16::from_be_bytes));
            let alpns = vec![net::SPACE_ALPN.to_vec()];
            let mode = net::Mode::from_env();
            let bound = net::Net::bind(&secret, mode, alpns.clone(), wanted)
                .or_else(|e| {
                    eprintln!("net: port {wanted:?} unavailable ({e}); binding any");
                    net::Net::bind(&secret, mode, alpns, None)
                })
                .map_err(|e| e.to_string());
            if let Ok(bound) = &bound {
                let port = net::endpoint::bound_port(bound.endpoint());
                eprintln!("net: bound {mode:?} on port {port:?}");
                if let Some(port) = port {
                    if wanted != Some(port) {
                        let _ = vault.put_entry(PORT_KEY, &port.to_be_bytes());
                    }
                }
            }
            let _ = proxy.send_event(Msg::NetBound(gen_, bound.map(Slot::new)));
        });
    }

    /// What an unlock starts, whoever did it — the bridge's `AuthDone`, the lock screen's
    /// login, a signup's last step. `Shell::new`'s own load ran while the vault was still
    /// locked and found nothing, so the relationship is read now. An account with no node,
    /// or one that is its own, hosts for itself: its owner grant is reissued every unlock,
    /// like a reconnect would.
    fn after_unlock(&mut self) {
        self.node = match node::load_relationship(&self.vault).ok().flatten() {
            Some(record) if !node::is_me(&self.vault, &record) => Some(record),
            _ => node::self_claim(&self.vault, node::now_secs())
                .map_err(|e| eprintln!("host: could not self-claim: {e}"))
                .ok(),
        };
        // Whatever the previous account had in flight is answered, not replayed on this
        // one's sessions.
        self.drop_pending("account changed");
        self.joined = sessions::load_joined(&self.vault);
        // The listener first: it bumps the generation the bind is tagged with.
        self.start_listener();
        self.bind_net();
    }

    /// Closing waits for the runtime, so it happens on its own thread.
    fn close_net(&mut self) {
        if let Some(net) = self.net.take() {
            std::thread::spawn(move || net.close());
        }
    }

    /// A new sync generation: drop every piece of per-node bookkeeping and, if a node is
    /// claimed and the vault unlocked, start this generation's listener.
    fn start_listener(&mut self) {
        self.sync_gen.fetch_add(1, Ordering::SeqCst);
        self.subscribed.clear();
        self.failed_subs.clear();
        self.node_vv.clear();
        // The in-process registry's channel is the only thing an old `Here` listener blocks
        // on; dropping its sender is what lets that thread see the generation has moved.
        if let Some(did) = self.listener_did.take() {
            node::registry().unregister(&did);
        }
        if let (Some(record), Some(did)) = (
            &self.node,
            self.vault.with_signer(|d| d.did().to_string()),
        ) {
            self.listener_did = Some(did.clone());
            spawn_push_listener(
                &self.proxy,
                did,
                record.token.clone(),
                self.sync_gen.clone(),
                self.vault.clone(),
            );
        }
    }

    /// `AssignRole`/`RevokeRole`: off-thread, answered through `NodeRpcDone` — or, for a
    /// joined space, down the session and answered by the host's `RoleDone`.
    fn change_role(
        &mut self,
        item_id: &str,
        did: String,
        role: String,
        revoke: bool,
        tx: std::sync::mpsc::Sender<Response>,
    ) {
        let (record, wi) = match (self.node.clone(), find_item(&self.vault, item_id)) {
            (Some(record), Ok(wi)) => (record, wi),
            (None, _) => {
                let _ = tx.send(Response::err("no node claimed"));
                return;
            }
            (_, Err(e)) => {
                let _ = tx.send(Response::err(e));
                return;
            }
        };
        if self.joined.contains_key(&wi.ws_id) {
            self.ensure_session(&wi.ws_id);
            let Some(handle) = self.sessions.get(&wi.ws_id) else {
                let _ = tx.send(Response::err("not connected to the host"));
                return;
            };
            if self.pending_role.contains_key(&wi.ws_id) {
                let _ = tx.send(Response::err("a role change is already in flight"));
                return;
            }
            let frame = Frame::Role {
                item_id: wi.id.clone(),
                to: did,
                role,
                revoke,
            };
            let _ = handle.tx.send(frame.clone());
            self.pending_role.insert(
                wi.ws_id.clone(),
                PendingRole {
                    reply: tx,
                    frame,
                    sent_on: handle.seq,
                },
            );
            return;
        }
        let vault = self.vault.clone();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let link = node::Link::current(&vault);
            let result = node::change_role(
            &link,
                &vault,
                record.token,
                &wi.ws_id,
                &wi.id,
                &did,
                &role,
                revoke,
            );
            // A role moved in this account's own ledger: the hosts it appointed hear it.
            if result.is_ok() && matches!(link, node::Link::Here(_)) {
                if let Err(e) = kunki::host::ledger_changed(&Admin::new(vault.clone()), &wi.ws_id) {
                    eprintln!("host: ledger of {}: {e}", wi.ws_id);
                }
            }
            let _ = proxy.send_event(Msg::NodeRpcDone(tx, NodeRpcOutcome::Pushed(result)));
        });
    }

    fn gen_(&self) -> u64 {
        self.sync_gen.load(Ordering::SeqCst)
    }

    /// This process as `kunki::host` sees it, at this generation.
    fn sink(&self) -> host::ShellSink {
        host::ShellSink {
            vault: self.vault.clone(),
            proxy: self.proxy.clone(),
            gen_: self.gen_(),
            ep: self.net.as_ref().map(|n| n.endpoint().clone()),
        }
    }

    fn pusher(&self) -> kunki::host::HostPusher<host::ShellSink> {
        kunki::host::HostPusher::new(self.sink())
    }

    fn validate_native_rects(&self, generation: u64, snapshots: &[native_rects::Snapshot]) -> Result<(), String> {
        native_rects::validate(generation, self.gen_(), snapshots, |id| {
            self.apps.get(id).filter(|a| a.is_shown()).map(AppThread::rect_scope)
        })
    }

    fn reset_tabs(&mut self) {
        self.fullscreen = false;
        self.tabs = vec![Tab::Home];
        self.net_leave_all();
        self.apps.clear();
        self.launcher = Default::default();
        self.reset_launcher_pending = false;
        self.focused = 0;
        self.split = None;
        self.popped.clear();
        self.error = None;
    }
    fn sync_launcher(&mut self) {
        match &self.screen {
            Screen::Items(screen) => self.launcher.bind(&self.vault, Some(screen.inventory())),
            Screen::Spaces(screen) => self.launcher.bind_context(
                &self.vault,
                Some(launcher::context::Context::Spaces(screen.inventory())),
            ),
            _ => self.launcher.bind(&self.vault, None),
        }
    }

    /// The bridge's stateful family: these touch tabs/apps/screens, so they are methods
    /// rather than the windowless `answer`. All are fast — seal plus a local write, no
    /// KDF — so they run inline on the UI thread (the single authority) and rebuild any
    /// screen that shows what they changed.
    fn answer_mut(&mut self, req: Request) -> Response {
        let response = match req {
            // Answered here, not in `answer`: whether this device is reachable is shell state.
            Request::Whoami => match self.vault.current() {
                Some(a) => {
                    let device = self
                        .vault
                        .device_public_key()
                        .map(|k| URL_SAFE_NO_PAD.encode(k))
                        .unwrap_or_default();
                    let hosting = node::hosting(&self.vault).unwrap_or_default();
                    Response::ok(serde_json::json!({
                        "did": a.did, "name": a.label, "device": device,
                        "online": self.net.is_some(), "hosting": hosting,
                    }))
                }
                None => Response::err("locked"),
            },
            Request::Hosts { ws_id } => self.hosts(&ws_id),
            Request::ShareSpace { ws_id } => self.share_space(&ws_id),
            Request::SoundLog { item_id } => match self.apps.get(item_id.as_str()) {
                Some(a) => Response::ok(self.sound.log_json(a.instance())),
                None => Response::err("item is not open"),
            },
            Request::SoundCapture { item_id, seconds } => match self.apps.get(item_id.as_str()) {
                Some(a) => {
                    let (wav, clock) = self.sound.capture_wav(a.instance(), seconds);
                    Response::ok(serde_json::json!({
                        "wav_base64": base64::engine::general_purpose::STANDARD.encode(wav),
                        "clock": clock,
                    }))
                }
                None => Response::err("item is not open"),
            },
            Request::Performance => Response::ok(performance::json(self.performance.as_ref())),
            Request::SelectWorkspace { ws_id } => {
                let workspace =
                    self.vault
                        .workspaces()
                        .map_err(|e| e.to_string())
                        .and_then(|workspaces| {
                            workspaces
                                .into_iter()
                                .find(|w| w.id == ws_id)
                                .ok_or_else(|| "workspace is not in this account".to_string())
                        });
                match workspace {
                    Ok(ws) => {
                        self.screen = Screen::Items(ItemsScreen::new(&self.vault, ws));
                        self.focused = 0;
                        self.error = None;
                        self.reset_launcher_pending = false;
                        Response::ok("selected")
                    }
                    Err(error) => Response::err(error),
                }
            }
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
                Ok(wi) => source_doc(&self.vault, &wi.ws_id, &wi.id)
                    .map(|doc| Response::ok(source_files(&doc)))
                    .unwrap_or_else(Response::err),
            },
            Request::ReadFile { item_id, path } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => source_doc(&self.vault, &wi.ws_id, &wi.id)
                    .and_then(|doc| read_source_file(&doc, &path))
                    .map(Response::ok)
                    .unwrap_or_else(Response::err),
            },
            req @ (Request::ReadLauncherFile { .. }
            | Request::EditLauncherFile { .. }
            | Request::ResetLauncher {}
            | Request::DumpLauncherTree {}
            | Request::ClickLauncher { .. }
            | Request::TypeLauncher { .. }
            | Request::ReadLauncherConsole { .. }) => self.answer_launcher(req),
            Request::ReadFileVersioned { item_id, path } => {
                match find_item(&self.vault, &item_id) {
                    Err(e) => Response::err(e),
                    Ok(wi) => {
                        if let Err(e) = valid_source_path(&path) {
                            return Response::err(e);
                        }
                        let file = source_doc(&self.vault, &wi.ws_id, &wi.id)
                            .and_then(|doc| read_source_doc_file_versioned(&doc, &path));
                        file.map(|f| {
                            Response::ok(VersionedFile {
                                content: f.content,
                                revision: f.revision,
                            })
                        })
                        .unwrap_or_else(Response::err)
                    }
                }
            }
            Request::EditFile {
                item_id,
                path,
                expected_revision,
                edits,
            } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    if let Err(e) = valid_source_path(&path) {
                        return Response::err(e);
                    }
                    let edits = match source_edits(edits) {
                        Ok(e) => e,
                        Err(e) => return Response::err(e),
                    };
                    let edited = source_doc(&self.vault, &wi.ws_id, &wi.id).and_then(|doc| {
                        edit_source_doc_file(&doc, &path, &expected_revision, &edits)
                    });
                    match edited.and_then(|edited| {
                        self.vault
                            .put_src(&wi.ws_id, &wi.id, &edited.snapshot)
                            .map_err(|e| e.to_string())?;
                        Ok(edited.revision)
                    }) {
                        Err(e) => Response::err(e),
                        Ok(revision) => Response::ok(EditFileResult {
                            revision,
                            persisted: true,
                            activation: SourceActivation::Closed,
                        }),
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
                    let snapshot = source_doc(&self.vault, &wi.ws_id, &wi.id)
                        .and_then(|doc| write_source_doc_file(&doc, &path, &content));
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
            Request::WriteBinaryFile {
                item_id,
                path,
                base64,
            } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => {
                    let (ws, item) = (wi.ws_id.as_str(), wi.id.as_str());
                    let bound = valid_source_path(&path)
                        .and_then(|()| binary_files::decode(&base64))
                        .and_then(|bytes| {
                            let doc = source_doc(&self.vault, ws, item)?;
                            app_host::bind_binary_file(
                                &doc,
                                &path,
                                &bytes,
                                binary_files::store(&self.vault, ws, item),
                            )
                        });
                    match bound {
                        Ok(bound) => binary_files::finish(&self.vault, ws, item, bound),
                        Err(e) => Response::err(e),
                    }
                }
            },
            Request::ListBinaryFiles { item_id } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => source_doc(&self.vault, &wi.ws_id, &wi.id)
                    .map(|doc| binary_files::list(app_host::binary_files(&doc)))
                    .unwrap_or_else(Response::err),
            },
            Request::ReadBinaryFile { item_id, path } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => match source_doc(&self.vault, &wi.ws_id, &wi.id) {
                    Err(e) => Response::err(e),
                    Ok(doc) => {
                        let file = app_host::binary_file(&doc, &path);
                        binary_files::read(&self.vault, &wi.ws_id, &wi.id, file)
                    }
                },
            },
            Request::ReloadItem { .. } => Response::err("item is not open"),
            // Slice 1 runs only source-only `t.expect` tests, but it still opens a non-persisting
            // test tab so the runner path already has a distinct retained-id/doc namespace.
            Request::RunTests { item_id, filter } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => match self
                    .vault
                    .get_src(&wi.ws_id, &wi.id)
                    .map_err(|e| e.to_string())
                {
                    Err(e) => Response::err(e),
                    Ok(None) => Response::err(format!("{} has no source", wi.name)),
                    Ok(Some(src)) => {
                        let old_focus = self.focused;
                        let response = match self.open_test_tab(&wi, src.clone(), "run") {
                            Err(e) => Err(e),
                            Ok(test_id) => {
                                let doc = LoroDoc::new();
                                let result = (|| {
                                    doc.import(&src).map_err(|e| e.to_string())?;
                                    let tests = source_tests(&doc, filter.as_deref())?;
                                    let bytes: usize = tests.iter().map(|t| t.source.len()).sum();
                                    if tests.len() > 32 || bytes > 256 * 1024 {
                                        return Err("test batch is too large".into());
                                    }
                                    Ok(Response::ok(
                                        run_lua_tests(tests)
                                            .into_iter()
                                            .map(|r| LuaTestResult {
                                                name: r.name,
                                                ok: r.ok,
                                                frames: r.frames,
                                                failure: r.failure,
                                            })
                                            .collect::<Vec<_>>(),
                                    ))
                                })();
                                self.close_test_tab(&test_id, old_focus);
                                result
                            }
                        };
                        response.unwrap_or_else(Response::err)
                    }
                },
            },
            // ── app actions: resolve by element id on a freshly built view, then route the
            // produced message exactly as the `Msg::Tab` arm would — we are already inside
            // `update`, so recursing into it would run the post-update flush twice.
            Request::DumpTree { .. }
            | Request::Click { .. }
            | Request::Type { .. }
            | Request::Key { .. } => Response::err("item is not open"),
            // ── senses: the app's live data and its console. Open tabs only — a closed
            // item's data is what ReadFile sees, and its console no longer exists.
            Request::ReadConsole { .. } => Response::err("item is not open"),
            // Screenshot is handled by the deferred `Msg::Rpc` arm, never synchronously.
            Request::Screenshot { .. } | Request::ScreenshotShell { .. } => {
                Response::err("screenshot was not deferred")
            }
            // Same: the Runner owns the clock and the frame, so these cannot be answered here.
            Request::Frame { .. }
            | Request::Advance { .. }
            | Request::Rects { .. }
            | Request::Touch { .. }
            | Request::PointerMove { .. }
            | Request::PointerPress { .. }
            | Request::PointerRelease { .. }
            | Request::Drag { .. }
            | Request::Wheel { .. } => Response::err("driver op was not deferred"),
            Request::AppDataGet { .. } | Request::Search { .. } => {
                Response::err("item is not open")
            }
            // Opening an already-open item focuses its tab — never a second VM for one
            // item. A fresh item with no source yet refuses honestly (WriteFile is its
            // other half).
            Request::OpenItem { item_id } => match find_item(&self.vault, &item_id) {
                Err(e) => Response::err(e),
                Ok(wi) => self
                    .open_tab(wi)
                    .map(|()| Response::ok("open"))
                    .unwrap_or_else(Response::err),
            },
            Request::SplitWith { item_id } => match self.apps.get_key_value(item_id.as_str()) {
                Some((id, _)) => {
                    self.split = Some(id.clone());
                    Response::ok("split")
                }
                None => Response::err("item is not open; call OpenItem first"),
            },
            Request::Unsplit => {
                self.split = None;
                Response::ok("unsplit")
            }
            Request::PopOut { item_id } => match self.apps.get_key_value(item_id.as_str()) {
                Some((id, _)) => {
                    let id = id.clone();
                    if !self.popped.contains(&id) {
                        self.split = self.split.take().filter(|s| *s != id);
                        self.popped.push(id);
                    }
                    Response::ok("popped out")
                }
                None => Response::err("item is not open; call OpenItem first"),
            },
            Request::DockIn { item_id } => {
                if self.dock(item_id.as_str()) {
                    Response::ok("docked")
                } else {
                    Response::err("item is not in a window of its own")
                }
            }
            Request::ListTabs => Response::ok(
                self.tabs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, t)| match t {
                        Tab::App((id, name)) => Some((idx, id, name)),
                        Tab::Home => None,
                    })
                    .map(|(idx, id, name)| {
                        let app = self.apps.get(id);
                        serde_json::json!({
                            "item_id": id.as_ref(),
                            "name": name,
                            "focused": idx == self.focused,
                            "shown": app.is_some_and(|a| a.is_shown()),
                            "window": self.popped.contains(id),
                            "responding": app.is_some_and(|a| a.responding()),
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
            req => answer(&self.vault, req),
        };
        self.sync_launcher();
        response
    }

    /// The apps on screen: the focused tab's, and the one split beside it. Home shows alone.
    /// The apps in the main window: the focused one and the split one, unless in a window of
    /// their own.
    fn on_screen(&self) -> Vec<Arc<str>> {
        let Some(Tab::App((id, _))) = self.tabs.get(self.focused) else {
            return Vec::new();
        };
        let mut ids = vec![id.clone()];
        ids.extend(self.split.clone().filter(|s| s != id));
        ids.retain(|id| !self.popped.contains(id));
        ids
    }

    /// Back into its tab, focused. False if it was not in a window of its own.
    fn dock(&mut self, id: &str) -> bool {
        let Some(pos) = self.popped.iter().position(|p| p.as_ref() == id) else {
            return false;
        };
        self.popped.remove(pos);
        if let Some(idx) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::App((tid, _)) if tid.as_ref() == id))
        {
            self.focused = idx;
        }
        true
    }

    /// Only apps on screen paint — in the main window or their own; every other one keeps
    /// running unseen.
    fn show_on_screen(&mut self) {
        let mut shown = self.on_screen();
        shown.extend(self.popped.iter().cloned());
        for (id, a) in self.apps.iter_mut() {
            a.show(shown.contains(id));
        }
    }

    /// Subscribe to an open app's docs, and send `names` to the node — plus any doc seen for the
    /// first time, since one opened from local storage may be behind the node. A no-op
    /// unclaimed.
    fn sync_open(&mut self, id: &Arc<str>, names: &[String]) {
        let (Some(record), Some(did)) = (
            self.node.clone(),
            self.vault.with_signer(|d| d.did().to_string()),
        ) else {
            return;
        };
        let Some(a) = self.apps.get(id) else { return };
        let ws = a.ws_id.clone();
        let session = self.joined.contains_key(&ws);
        let mut send = names.to_vec();
        for name in &a.doc_names {
            // A session needs no subscribe: the host subscribes each doc an update names.
            let first = if session {
                self.subscribed
                    .entry(id.clone())
                    .or_default()
                    .insert(name.clone())
            } else {
                subscribe_if_new(
                    &self.proxy,
                    &self.vault,
                    self.gen_(),
                    &mut self.subscribed,
                    &did,
                    record.token.clone(),
                    &ws,
                    id,
                    name,
                )
            };
            if first && !send.contains(name) {
                send.push(name.clone());
            }
        }
        for name in send {
            self.sync_one(id, &name);
        }
    }

    /// The node refused a write: put the doc back to the node's copy on the app's thread, and
    /// say why in the app's console. Everything the node lacked goes, including edits made
    /// after the refused push was sent; the next flush saves the result and syncs it.
    fn roll_back(&mut self, item_id: &Arc<str>, name: String, rejected: Rejected) {
        let key = (item_id.clone(), name.clone());
        let since = self.node_vv.get(&key).cloned();
        if let Some(state) = &rejected.state {
            self.node_vv.insert(key, state.vv.clone());
        }
        let Some(a) = self.apps.get(item_id) else {
            return;
        };
        let id = item_id.clone();
        a.call(move |h| {
            let back = h.app.with_doc(&name, |doc| {
                desktop_roll_back(doc, since.as_deref(), &rejected)
            });
            match back {
                Some(Ok(doc)) => {
                    h.app.replace_doc(&name, doc);
                    h.app.note(format!("sync: {name} rejected: {}", rejected.reason));
                }
                Some(Err(e)) => eprintln!("sync: rolling back {id}/{name} failed: {e}"),
                None => {}
            }
        });
    }

    /// Bytes from the node into an open app's doc, on its thread. `ws` is checked when given.
    fn import(&self, id: &Arc<str>, ws: Option<&str>, name: String, bytes: Vec<u8>) {
        let Some(a) = self.apps.get(id) else { return };
        if ws.is_some_and(|ws| ws != a.ws_id) {
            return;
        }
        let id = id.clone();
        a.call(move |h| {
            h.app.with_doc(&name, |doc| {
                if let Err(e) = doc.import(&bytes) {
                    eprintln!("import failed for {id}/{name}: {e}");
                }
            });
        });
    }

    /// A request for an open app goes to its thread, which replies itself; anything else comes
    /// back for `answer_mut`, which serves a closed item from the vault.
    fn forward(
        &mut self,
        req: Request,
        tx: std::sync::mpsc::Sender<Response>,
    ) -> Option<(Request, std::sync::mpsc::Sender<Response>)> {
        let id = match &req {
            Request::ListFiles { item_id }
            | Request::ReadFile { item_id, .. }
            | Request::ReadFileVersioned { item_id, .. }
            | Request::EditFile { item_id, .. }
            | Request::WriteFile { item_id, .. }
            | Request::WriteBinaryFile { item_id, .. }
            | Request::ListBinaryFiles { item_id }
            | Request::ReadBinaryFile { item_id, .. }
            | Request::ReloadItem { item_id }
            | Request::DumpTree { item_id }
            | Request::Click { item_id, .. }
            | Request::Type { item_id, .. }
            | Request::Key { item_id, .. }
            | Request::ReadConsole { item_id, .. }
            | Request::AppDataGet { item_id }
            | Request::Search { item_id, .. } => item_id.clone(),
            _ => return Some((req, tx)),
        };
        let Some(a) = self.apps.get(id.as_str()) else {
            return Some((req, tx));
        };
        // Queued behind a hang, it would wait out the bridge's timeout.
        if !a.responding() {
            let _ = tx.send(Response::err("the app is not responding"));
            return None;
        }
        let vault = self.vault.clone();
        a.call(move |h| {
            let _ = tx.send(serve(h, req, &vault));
        });
        None
    }

    pub fn new(proxy: EventLoopProxy<Msg>, vault: Vault) -> Self {
        Self::with_viewport(proxy, vault, None)
    }

    fn with_viewport(
        proxy: EventLoopProxy<Msg>,
        vault: Vault,
        offscreen: Option<(f32, f32)>,
    ) -> Self {
        let screen = if vault.is_empty() {
            Screen::Signup(SignupForm::default())
        } else {
            Screen::Login(LoginScreen::new(&vault))
        };
        let tabs = vec![Tab::Home];
        let node = node::load_relationship(&vault).ok().flatten();
        let mut shell = Shell {
            vault,
            proxy,
            screen,
            tabs,
            apps: HashMap::new(),
            focused: 0,
            split: None,
            popped: Vec::new(),
            sound: sound::SoundHost::new(offscreen.is_some()),
            offscreen,
            clock: 0.0,
            performance: None,
            error: None,
            reset_launcher_pending: false,
            fullscreen: false,
            launcher: Default::default(),
            screenshot: None,
            driver: None,
            native_rects_pending: false,
            node: node.clone(),
            net: None,
            listener_did: None,
            joined: HashMap::new(),
            sessions: HashMap::new(),
            redial: HashMap::new(),
            pending_join: HashMap::new(),
            join_waiting: Vec::new(),
            pulls: Default::default(),
            pending_role: HashMap::new(),
            subscribed: HashMap::new(),
            failed_subs: Vec::new(),
            node_vv: HashMap::new(),
            sync_gen: Arc::new(AtomicU64::new(0)),
            channels: Default::default(),
        };
        shell.start_listener();
        shell
    }
    /// Open-or-focus, the one path every opener takes: an open item's tab is focused, never
    /// given a second thread.
    fn open_tab(&mut self, wi: WorkspaceItem) -> Result<(), String> {
        self.fullscreen = false;
        if let Some(pos) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::App((id, _)) if id.as_ref() == wi.id.as_str()))
        {
            self.focused = pos;
            return Ok(());
        }
        let src = self
            .vault
            .get_src(&wi.ws_id, &wi.id)
            .map_err(|e| e.to_string())?;
        let Some(src) = src else {
            return Err(format!("{} has no source", wi.name));
        };

        let id: Arc<str> = wi.id.as_str().into();
        let node = self.node.as_ref().and_then(|record| {
            self.vault
                .with_signer(|d| d.did().to_string())
                .map(|did| (did, record.token.clone()))
        });
        // A joined space's docs come from its host, over the session, before the app seeds.
        let pull = if self.joined.contains_key(&wi.ws_id) {
            self.ensure_session(&wi.ws_id);
            self.sessions.get(&wi.ws_id).map(|h| sessions::Pull {
                tx: h.tx.clone(),
                waiters: self.pulls.clone(),
            })
        } else {
            None
        };
        let me = self.me_in(&wi.ws_id, &src, true)?;
        let app = AppThread::spawn(
            Open {
                item_id: id.clone(),
                ws_id: wi.ws_id.clone(),
                src,
                vault: self.vault.clone(),
                persist: true,
                node,
                pull,
                me,
                sound: Some(self.sound.sender()),
            },
            self.proxy.clone(),
            self.offscreen,
        )?;
        self.apps.insert(id.clone(), app);
        self.net_join(&id, &wi.ws_id);
        self.focused = self.tabs.len();
        self.tabs.push(Tab::App((id, wi.name)));
        Ok(())
    }

    /// Who an app in `ws_id` runs for: this account, this device, and the manifest's rules
    /// with the account's roles in the space (`owned-entries.md` §2.4). The owner of a space it
    /// hosts holds `owner`; a member, the role its token carries — never `owner` by default,
    /// which would satisfy every override. A source whose manifest does not parse checks
    /// nothing here; the host refuses its writes anyway.
    fn me_in(&self, ws_id: &str, src: &[u8], stamp: bool) -> Result<app_host::Me, String> {
        let did = self
            .vault
            .with_signer(|d| d.did().to_string())
            .ok_or("no account is unlocked")?;
        let name = self.vault.current().map(|a| a.label).unwrap_or_default();
        let roles = match self.joined.get(ws_id) {
            Some(joined) => joined
                .token
                .as_ref()
                .and_then(|t| courier::role::Grant::of(t).ok())
                .map(|g| g.role)
                .into_iter()
                .collect(),
            None => [manifest::OWNER.to_string()].into(),
        };
        let rules = app_manifest(src).map(|manifest| app_host::Rules { manifest, roles });
        Ok(app_host::Me {
            did,
            name,
            peer: device_peer(&self.vault)?,
            stamp,
            rules,
        })
    }

    fn open_test_tab(
        &mut self,
        wi: &WorkspaceItem,
        src: Vec<u8>,
        run: &str,
    ) -> Result<Arc<str>, String> {
        let id: Arc<str> = format!("test:{}:{run}", wi.id).into();
        self.close_test_tab(&id, self.focused);
        let me = self.me_in(&wi.ws_id, &src, false)?;
        let app = AppThread::spawn(
            Open {
                item_id: id.clone(),
                ws_id: wi.ws_id.clone(),
                src,
                vault: self.vault.clone(),
                persist: false,
                node: None,
                pull: None,
                me,
                sound: None,
            },
            self.proxy.clone(),
            self.offscreen,
        )?;
        self.apps.insert(id.clone(), app);
        self.focused = self.tabs.len();
        self.tabs
            .push(Tab::App((id.clone(), format!("test: {}", wi.name))));
        Ok(id)
    }

    /// An app's share of the screen: its latest frame, or why there is none.
    fn app_slot(&self, id: &Arc<str>) -> El<Msg> {
        match self.apps.get(id) {
            Some(a) if !a.responding() => text("this app is not responding")
                .color(theme::error())
                .grow(),
            Some(a) => {
                let to = id.clone();
                tile(a.frame.clone())
                    .h_full()
                    .grow()
                    .on_tile(id.to_string(), move |e| Msg::TileInput(to.clone(), e))
            }
            None => text("app not found").color(theme::error()).grow(),
        }
    }

    fn close_test_tab(&mut self, id: &Arc<str>, restore_focus: usize) {
        self.apps.remove(id);
        self.split = self.split.take().filter(|s| s != id);
        self.popped.retain(|p| p != id);
        if let Some(pos) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::App((tid, _)) if tid == id))
        {
            self.tabs.remove(pos);
        }
        self.focused = restore_focus.min(self.tabs.len().saturating_sub(1));
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
            let stuck = close_id
                .as_ref()
                .and_then(|id| self.apps.get(id))
                .is_some_and(|a| !a.responding());

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
            if stuck {
                el = el.child(text("not responding").font_size(11.0).color(theme::error()));
            }

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
            .child(col().grow())
            .children((!self.on_screen().is_empty()).then(|| {
                row().h(28.0).px(8.0).radius(6.0).align_center().no_shrink()
                    .id("shell:fullscreen").hover_fill(theme::bg_2())
                    .on_click(Msg::SetFullscreen(true))
                    .child(
                        text("Full screen")
                            .font_size(11.0)
                            .no_wrap()
                            .color(theme::fg_3()),
                    )
            }))
            .child(
                row()
                    .h(28.0)
                    .px(8.0)
                    .radius(6.0)
                    .no_shrink()
                    .align_center()
                    .id("shell:reset-launcher")
                    .hover_fill(theme::bg_2())
                    .on_click(Msg::ResetLauncher)
                    .child(
                        text(if self.reset_launcher_pending {
                            "Confirm reset"
                        } else {
                            "Reset launcher"
                        })
                        .font_size(11.0)
                        .no_wrap()
                        .color(theme::fg_3()),
                    ),
            )
            .children(self.reset_launcher_pending.then(|| {
                row()
                    .h(28.0)
                    .px(8.0)
                    .align_center()
                    .id("shell:cancel-launcher-reset")
                    .on_click(Msg::CancelLauncherReset)
                    .child(
                        text("Cancel")
                            .font_size(11.0)
                            .no_wrap()
                            .color(theme::fg_3()),
                    )
            }))
    }
}

impl App for Shell {
    fn frame_performance(&mut self, snapshot: runtime::FramePerformance) {
        self.performance = Some(snapshot);
    }
    type Msg = Msg;

    fn fullscreen(&self) -> bool {
        self.fullscreen && !self.on_screen().is_empty()
    }

    fn ready(&mut self) {
        // Publishing the socket before `run_app` begins polling loses rapid startup events on
        // some winit backends. Runner calls this only after the renderer and event loop are live.
        #[cfg(not(target_os = "android"))]
        {
            bridge::spawn(self.proxy.clone());
        }
        #[cfg(all(target_os = "android", debug_assertions, feature = "adb-debug"))]
        bridge::spawn_at(self.proxy.clone(), android_debug_dir().join("shell.sock"));
        #[cfg(any(not(target_os = "android"), all(debug_assertions, feature = "adb-debug")))]
        arm_sync_tick(&self.proxy);
    }

    fn view(&self) -> El<Msg> {
        let content = match &self.tabs[self.focused] {
            Tab::Home => match &self.screen {
                Screen::Signup(f) => f.view(),
                Screen::Mnemonic(m) => m.view(),
                Screen::Login(a) => a.view(),
                Screen::Spaces(s) => s.view(self.launcher.get().ok(), self.launcher.warning()),
                Screen::Items(i) => i.view(self.launcher.get().ok(), self.launcher.warning()),
            },
            Tab::App((id, _)) if self.popped.contains(id) => col()
                .w_full()
                .grow()
                .center()
                .gap(12.0)
                .child(text("open in its own window").color(theme::fg_3()))
                .child(
                    row()
                        .h(28.0)
                        .px(12.0)
                        .radius(6.0)
                        .center()
                        .id(format!("dock:{id}"))
                        .fill(theme::bg_2())
                        .hover_fill(theme::bg_3())
                        .on_click(Msg::Dock(id.clone()))
                        .child(text("bring back").font_size(13.0)),
                ),
            Tab::App(_) => {
                let mut tiles = row().w_full().grow();
                for (i, id) in self.on_screen().into_iter().enumerate() {
                    if i > 0 {
                        tiles = tiles.child(col().w(1.0).h_full().fill(theme::bd_1()));
                    }
                    tiles = tiles.child(self.app_slot(&id));
                }
                tiles
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
                    // Full launcher validation diagnostics remain in its console.
                    .child(
                        text(e.lines().next().unwrap_or(e))
                            .font_size(12.0)
                            .color(theme::error()),
                    ),
            );
        }

        if self.fullscreen() {
            return page.child(content).overlay(
                row().h(32.0).px(8.0).center().radius(6.0).fill(theme::bg_2())
                    .id("shell:exit-fullscreen").on_click(Msg::SetFullscreen(false))
                    .child(text("Exit").font_size(11.0).no_wrap().color(theme::fg_2())),
                None,
                runtime::Placement {
                    side: runtime::PlacementSide::Bottom,
                    align: runtime::PlacementAlign::Start,
                },
                runtime::Anchor::Point(8.0, 80.0),
            );
        }
        if matches!(self.screen, Screen::Spaces(_) | Screen::Items(_)) {
            page = page.child(self.strip());
        }
        page.child(content)
    }
    fn update(&mut self, msg: Msg) {
        // `Shell.node` mirrors `SpaceScreen`'s own copy — see the field's doc comment. A borrow,
        // not a move, so the match below still owns `msg` and dispatches it to the screen too.
        if let Msg::Space(SpaceScreenMsg::ClaimResult(Ok(record))) = &msg {
            self.node = Some(record.clone());
            self.start_listener();
        }
        let next = match msg {
            Msg::SetFullscreen(enabled) => {
                self.fullscreen = enabled && !self.on_screen().is_empty();
                if self.fullscreen { self.split = None; }
                None
            }
            Msg::ResetLauncher => {
                if self.reset_launcher_pending {
                    self.error = self.reset_launcher().err();
                    self.reset_launcher_pending = false;
                } else if self.vault.current().is_some() {
                    self.reset_launcher_pending = true;
                }
                None
            }
            Msg::CancelLauncherReset => {
                self.reset_launcher_pending = false;
                None
            }
            Msg::Items(ItemsScreenMsg::Open(wi)) => {
                self.error = self.open_tab(wi).err();
                None
            }
            Msg::WorkspaceCreated {
                account,
                job,
                result,
            } => {
                if let Screen::Spaces(screen) = &mut self.screen {
                    screen.finish_creation(&self.vault, &account, job, result);
                }
                None
            }
            Msg::Launcher(instance, msg) => {
                let commands = if self.focused == 0 && self.vault.current().is_some()
                    && matches!(self.screen, Screen::Spaces(_)) {
                    self.launcher.get_mut().map(|launcher| launcher.update(instance, msg))
                        .unwrap_or_default()
                } else { Vec::new() };
                for command in commands {
                    match command {
                        launcher::Command::SelectWorkspace(ws_id) => {
                            self.error = match self.answer_mut(Request::SelectWorkspace { ws_id }) {
                                Response::Ok { .. } => None,
                                Response::Err { message } => Some(message),
                            };
                            break;
                        }
                        launcher::Command::CreateWorkspace(name) => {
                            if let Screen::Spaces(screen) = &mut self.screen {
                                self.error = screen.create_workspace(&self.vault, &self.proxy, &name).err();
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            Msg::Items(ItemsScreenMsg::Launcher(ws_id, instance, msg)) => {
                let commands = match &mut self.screen {
                    Screen::Items(i) if self.focused == 0 && i.ws().id == ws_id => self
                        .launcher
                        .get_mut()
                        .map(|launcher| launcher.update(instance, msg))
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let mut next = None;
                for command in commands {
                    self.error = match command {
                        launcher::Command::SelectWorkspace(_)
                        | launcher::Command::CreateWorkspace(_) => {
                            Err("focus the workspace list to select or create".into())
                        }
                        launcher::Command::Workspace { ws_id, action } => {
                            let home_ws = match &self.screen {
                                Screen::Items(i) => Some(i.ws().id.as_str()),
                                _ => None,
                            };
                            launcher::navigation::validate(
                                &self.vault,
                                home_ws,
                                self.focused,
                                &ws_id,
                            )
                            .and_then(|_| {
                                let Screen::Items(i) = &mut self.screen else {
                                    return Err("launcher workspace changed".into());
                                };
                                let msg = match action {
                                    launcher::WorkspaceAction::Back => ItemsScreenMsg::Back,
                                    launcher::WorkspaceAction::Import => ItemsScreenMsg::Upload,
                                };
                                next = i.update(msg, &self.vault, &self.proxy);
                                Ok(())
                            })
                        }
                        launcher::Command::OpenItem(command) => {
                            let item = self
                                .vault
                                .items(&ws_id)
                                .map_err(|e| e.to_string())
                                .and_then(|items| {
                                    items
                                        .into_iter()
                                        .find(|i| command.ws_id == ws_id && i.id == command.item_id)
                                        .ok_or_else(|| "item is not in this workspace".to_string())
                                });
                            item.and_then(|wi| self.open_tab(wi))
                        }
                        launcher::Command::EditSource {
                            ws_id,
                            path,
                            revision,
                            edits,
                        } => self
                            .edit_launcher(Some(&ws_id), &path, &revision, &edits)
                            .map(|_| ()),
                    }
                    .err();
                    if next.is_some() {
                        self.reset_launcher_pending = false;
                        break;
                    }
                }
                next
            }
            Msg::TileInput(id, e) => {
                if let Some(a) = self.apps.get(&id) {
                    a.input(e);
                }
                None
            }
            // The paint that follows delivering this is the point; `before_frame` fetches.
            Msg::TileDirty(_) => None,
            Msg::AppSaved {
                id,
                dirtied,
                names,
                error,
            } => {
                if error.is_some() {
                    self.error = error;
                }
                if let Some(a) = self.apps.get_mut(&id) {
                    a.doc_names = names;
                }
                self.sync_open(&id, &dirtied);
                None
            }
            Msg::Focus(idx) => {
                self.fullscreen = false;
                if idx < self.tabs.len() {
                    self.focused = idx;
                }
                None
            }
            Msg::Dock(id) => {
                self.dock(&id);
                None
            }
            Msg::Close(id) => {
                self.close_item(id);
                None
            }
            Msg::Rpc(Request::CloseItem { item_id }, tx) => {
                let closed = self.close_item(item_id.as_str().into());
                let _ = tx.send(Response::ok(if closed { "closed" } else { "not open" }));
                None
            }
            Msg::Rpc(Request::SyncNow, tx) => {
                self.reconcile();
                let _ = tx.send(Response::ok("syncing"));
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
                // Apps first: closing waits for their last save, which must land in this account.
                self.reset_tabs();
                self.vault.lock();
                self.node = None;
                self.close_net();
                self.drop_pending("locked");
                self.start_listener();
                self.screen = match self.vault.accounts() {
                    Ok(list) if !list.is_empty() => Screen::Login(LoginScreen::new(&self.vault)),
                    _ => Screen::Signup(SignupForm::default()),
                };
                let _ = tx.send(Response::ok("locked"));
                None
            }
            // Node RPCs: socket I/O to kunki, so a worker thread and the same
            // AuthDone-shaped completion path Signup/Unlock already use — not `answer_mut`,
            // whose whole point is that its family never leaves the UI thread.
            Msg::Rpc(Request::ClaimNode { ticket }, tx) => {
                let vault = self.vault.clone();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    eprintln!("DBG timing: ClaimNode thread start {}", debug_now_ms());
                    let link = node::Link::socket();
                    let result = node::join(&link, &vault, &ticket, node::now_secs());
                    eprintln!("DBG timing: ClaimNode send_event {}", debug_now_ms());
                    let _ = proxy.send_event(Msg::NodeRpcDone(tx, NodeRpcOutcome::Claimed(result)));
                });
                None
            }
            Msg::Rpc(Request::Invite, tx) => {
                match self.node.clone() {
                    Some(record) => {
                        let vault = self.vault.clone();
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            eprintln!("DBG timing: Invite thread start {}", debug_now_ms());
                            let link = node::Link::current(&vault);
                            let result =
                                node::invite(&link, &vault, record.token, "member", Scope::Node)
                                    .and_then(|t| t.to_text().map_err(|e| e.to_string()));
                            eprintln!("DBG timing: Invite send_event {}", debug_now_ms());
                            let _ = proxy
                                .send_event(Msg::NodeRpcDone(tx, NodeRpcOutcome::Invited(result)));
                        });
                    }
                    None => {
                        let _ = tx.send(Response::err("no node claimed"));
                    }
                }
                None
            }
            Msg::Rpc(Request::AssignRole { item_id, did, role }, tx) => {
                self.change_role(&item_id, did, role, false, tx);
                None
            }
            Msg::Rpc(Request::RevokeRole { item_id, did, role }, tx) => {
                self.change_role(&item_id, did, role, true, tx);
                None
            }
            Msg::Rpc(Request::PushSrc { item_id }, tx) => {
                match (self.node.clone(), find_item(&self.vault, &item_id)) {
                    (Some(record), Ok(wi)) => {
                        let vault = self.vault.clone();
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            eprintln!("DBG timing: PushSrc thread start {}", debug_now_ms());
                            let link = node::Link::current(&vault);
                            let result =
                                node::push_src(&link, &vault, record.token, &wi.ws_id, &wi.id);
                            eprintln!("DBG timing: PushSrc send_event {}", debug_now_ms());
                            let _ = proxy
                                .send_event(Msg::NodeRpcDone(tx, NodeRpcOutcome::Pushed(result)));
                        });
                    }
                    (None, _) => {
                        let _ = tx.send(Response::err("no node claimed"));
                    }
                    (_, Err(e)) => {
                        let _ = tx.send(Response::err(e));
                    }
                }
                None
            }
            Msg::Rpc(Request::PublishAll, tx) => {
                match (self.node.clone(), self.vault.workspaces()) {
                    (Some(record), Ok(spaces)) => {
                        let vault = self.vault.clone();
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            eprintln!("DBG timing: PublishAll thread start {}", debug_now_ms());
                            let link = node::Link::current(&vault);
                            let mut published = 0usize;
                            let mut failed = None;
                            for meta in &spaces {
                                let r =
                                    space::publish_one(&link, &vault, record.token.clone(), meta);
                                if let Err(e) = r {
                                    failed = Some(e);
                                    break;
                                }
                                published += 1;
                            }
                            let result = match failed {
                                Some(e) => Err(e),
                                None => Ok(published),
                            };
                            eprintln!("DBG timing: PublishAll send_event {}", debug_now_ms());
                            let _ = proxy.send_event(Msg::NodeRpcDone(
                                tx,
                                NodeRpcOutcome::Published(result),
                            ));
                        });
                    }
                    (None, _) => {
                        let _ = tx.send(Response::err("no node claimed"));
                    }
                    (_, Err(e)) => {
                        let _ = tx.send(Response::err(e.to_string()));
                    }
                }
                None
            }
            Msg::Rpc(Request::JoinSpace { link }, tx) => {
                self.join_space(link, tx);
                None
            }
            Msg::JoinLink(link) => {
                self.join_typed(link);
                None
            }
            Msg::Rpc(Request::AppointHost { ws_id, did }, tx) => {
                self.appoint_host(ws_id, did, tx);
                None
            }
            Msg::Rpc(
                Request::JoinItem {
                    ws_id,
                    ws_name,
                    item_id,
                    item_name,
                    item_kind,
                },
                tx,
            ) => {
                match (self.node.clone(), kind_from_str(&item_kind)) {
                    (Some(record), Ok(kind)) => {
                        let vault = self.vault.clone();
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            eprintln!("DBG timing: JoinItem thread start {}", debug_now_ms());
                            let link = node::Link::current(&vault);
                            let now = node::now_secs();
                            let ws = WorkspaceMeta {
                                id: ws_id.clone(),
                                name: ws_name,
                                created: now,
                            };
                            let item = WorkspaceItem {
                                id: item_id,
                                ws_id,
                                name: item_name,
                                kind,
                                created: now,
                            };
                            let result = node::join_item(&link, &vault, record.token, ws, item);
                            eprintln!("DBG timing: JoinItem send_event {}", debug_now_ms());
                            let _ = proxy
                                .send_event(Msg::NodeRpcDone(tx, NodeRpcOutcome::Joined(result)));
                        });
                    }
                    (None, _) => {
                        let _ = tx.send(Response::err("no node claimed"));
                    }
                    (_, Err(e)) => {
                        let _ = tx.send(Response::err(e));
                    }
                }
                None
            }
            Msg::NodeRpcDone(tx, outcome) => {
                let kind = match &outcome {
                    NodeRpcOutcome::Claimed(_) => "Claimed",
                    NodeRpcOutcome::Invited(_) => "Invited",
                    NodeRpcOutcome::Published(_) => "Published",
                    NodeRpcOutcome::Joined(_) => "Joined",
                    NodeRpcOutcome::Pushed(_) => "Pushed",
                };
                eprintln!(
                    "DBG timing: NodeRpcDone({kind}) received {}",
                    debug_now_ms()
                );
                let resp = match outcome {
                    NodeRpcOutcome::Claimed(Ok(record)) => {
                        let did = record.node_did.clone();
                        self.node = Some(record);
                        self.start_listener();
                        Response::ok(did)
                    }
                    NodeRpcOutcome::Claimed(Err(e)) => Response::err(e),
                    NodeRpcOutcome::Invited(Ok(text)) => Response::ok(text),
                    NodeRpcOutcome::Invited(Err(e)) => Response::err(e),
                    NodeRpcOutcome::Published(Ok(n)) => Response::ok(n),
                    NodeRpcOutcome::Published(Err(e)) => Response::err(e),
                    NodeRpcOutcome::Joined(Ok(())) => {
                        // Same rule `CreateWorkspace`/`CreateItem` already follow: a write
                        // that lands off-thread must not leave a showing Spaces screen
                        // holding its construction-time (possibly empty) snapshot — this is
                        // exactly how a joinee with no workspace of their own got stuck on
                        // "press ⏎ to create" even after `join_item` had already adopted one.
                        refresh_after_workspace(&mut self.screen, &self.vault);
                        Response::ok("joined")
                    }
                    NodeRpcOutcome::Joined(Err(e)) => Response::err(e),
                    NodeRpcOutcome::Pushed(Ok(())) => Response::ok("pushed"),
                    NodeRpcOutcome::Pushed(Err(e)) => Response::err(e),
                };
                let _ = tx.send(resp);
                None
            }
            Msg::Rpc(Request::RunTests { item_id, filter }, reply) => {
                let result = find_item(&self.vault, &item_id).and_then(|wi| {
                    let src = self
                        .vault
                        .get_src(&wi.ws_id, &wi.id)
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("{} has no source", wi.name))?;
                    let doc = LoroDoc::new();
                    doc.import(&src).map_err(|e| e.to_string())?;
                    let tests = source_tests(&doc, filter.as_deref())?;
                    let bytes: usize = tests.iter().map(|t| t.source.len()).sum();
                    if tests.len() > 32 || bytes > 256 * 1024 {
                        return Err("test batch is too large".into());
                    }
                    let old_focus = self.focused;
                    let test_id = self.open_test_tab(&wi, src, "run")?;
                    Ok((tests, test_id, old_focus))
                });
                match result {
                    Err(e) => {
                        let _ = reply.send(Response::err(e));
                    }
                    Ok((tests, test_id, old_focus)) => {
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            let driver_proxy = proxy.clone();
                            let world_proxy = proxy.clone();
                            let done_proxy = proxy.clone();
                            let send_driver = Arc::new(move |op: DriverOp| {
                                let (tx, rx) = std::sync::mpsc::channel();
                                driver_proxy
                                    .send_event(Msg::TestDriver(op, tx))
                                    .map_err(|e| format!("shell event loop closed: {e}"))?;
                                rx.recv()
                                    .map_err(|_| "shell closed before driver reply".to_string())?
                            });
                            let for_step = send_driver.clone();
                            let for_rects = send_driver.clone();
                            let for_click = send_driver.clone();
                            let text_proxy = world_proxy.clone();
                            let world_id = test_id.clone();
                            let text_id = test_id.clone();
                            let type_id = test_id.clone();
                            let type_proxy = text_proxy.clone();
                            let api = LuaTestApi {
                                step: Some(Arc::new(move |frames| {
                                    for_step(DriverOp::Frame(frames)).map(|r| r.frames)
                                })),
                                world: Some(Arc::new(move || {
                                    let (tx, rx) = std::sync::mpsc::channel();
                                    world_proxy
                                        .send_event(Msg::TestWorld(world_id.clone(), tx))
                                        .map_err(|e| format!("shell event loop closed: {e}"))?;
                                    rx.recv().map_err(|_| {
                                        "shell closed before world reply".to_string()
                                    })?
                                })),
                                rects: Some(Arc::new(move || {
                                    for_rects(DriverOp::Rects).map(rects_json)
                                })),
                                click_at: Some(Arc::new(move |x, y| {
                                    for_click(DriverOp::PointerMove((x, y)))?;
                                    for_click(DriverOp::PointerPress)?;
                                    for_click(DriverOp::PointerRelease)?;
                                    Ok(())
                                })),
                                text: Some(Arc::new(move |el_id| {
                                    let (tx, rx) = std::sync::mpsc::channel();
                                    text_proxy
                                        .send_event(Msg::TestText(text_id.clone(), el_id, tx))
                                        .map_err(|e| format!("shell event loop closed: {e}"))?;
                                    rx.recv()
                                        .map_err(|_| "shell closed before text reply".to_string())?
                                })),
                                type_text: Some(Arc::new(move |el_id, text| {
                                    let (tx, rx) = std::sync::mpsc::channel();
                                    type_proxy
                                        .send_event(Msg::TestType(type_id.clone(), el_id, text, tx))
                                        .map_err(|e| format!("shell event loop closed: {e}"))?;
                                    rx.recv()
                                        .map_err(|_| "shell closed before type reply".to_string())?
                                })),
                            };
                            let out = run_lua_tests_with(tests, api)
                                .into_iter()
                                .map(|r| LuaTestResult {
                                    name: r.name,
                                    ok: r.ok,
                                    frames: r.frames,
                                    failure: r.failure,
                                })
                                .collect();
                            let _ = done_proxy
                                .send_event(Msg::TestRunDone(reply, out, test_id, old_focus));
                        });
                    }
                }
                None
            }
            Msg::Rpc(
                req @ (Request::Screenshot { .. } | Request::ScreenshotShell { .. }),
                reply,
            ) => {
                let (item_id, width, height, scale) = match req {
                    Request::Screenshot {
                        item_id,
                        width,
                        height,
                        scale,
                    } => (Some(item_id), width, height, scale),
                    Request::ScreenshotShell {
                        width,
                        height,
                        scale,
                    } => (None, width, height, scale),
                    _ => unreachable!(),
                };
                let spec = screenshot_spec(width, height, scale);
                let item_error = item_id.as_ref().and_then(|item_id| {
                    if !self.apps.contains_key(item_id.as_str()) {
                        return Some("item is not open".to_string());
                    }
                    let focused = self
                        .tabs
                        .get(self.focused)
                        .is_some_and(|t| matches!(t, Tab::App((id, _)) if id.as_ref() == item_id));
                    (!focused).then(|| "item is not focused; call OpenItem first".to_string())
                });
                let error = if item_error.is_some() {
                    item_error
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
            Msg::Rpc(Request::Rects {}, reply) if self.offscreen.is_none() => {
                if self.native_rects_pending || self.driver.is_some() {
                    let _ = reply.send(Response::err("a geometry inspection is already pending"));
                    return;
                }
                self.native_rects_pending = true;
                let requests: Vec<_> = self
                    .apps
                    .iter()
                    .filter(|(_, a)| a.is_shown())
                    .map(|(id, a)| native_rects::Request {
                        id: id.clone(),
                        scope: a.rect_scope(),
                        reply: a.rects(),
                    })
                    .collect();
                let proxy = self.proxy.clone();
                let generation = self.gen_();
                std::thread::spawn(move || {
                    let result = native_rects::collect(requests, std::time::Duration::from_secs(3));
                    let _ = proxy.send_event(Msg::NativeRects(reply, generation, result));
                });
                None
            }
            Msg::NativeRects(reply, generation, result) => {
                self.native_rects_pending = false;
                let result = result.and_then(|snapshots| {
                    self.validate_native_rects(generation, &snapshots)?;
                    Ok(snapshots)
                });
                match result {
                    _ if self.driver.is_some() => { let _ = reply.send(Response::err("a driver op is already pending")); }
                    Err(e) => { let _ = reply.send(Response::err(e)); }
                    Ok(snapshots) => {
                        for snapshot in &snapshots {
                            if let Some(a) = self.apps.get_mut(&snapshot.id) { a.frame.rects = snapshot.rects.clone(); }
                        }
                        self.driver = Some(PendingDriver {
                            reply: DriverReply::NativeRects(reply, generation, snapshots),
                            op: DriverOp::Rects,
                        });
                    }
                }
                None
            }
            Msg::Rpc(
                req @ (Request::Frame { .. }
                | Request::Advance { .. }
                | Request::Rects { .. }
                | Request::Touch { .. }
                | Request::PointerMove { .. }
                | Request::PointerPress { .. }
                | Request::PointerRelease { .. }
                | Request::Keyboard { .. }
                | Request::Drag { .. }
                | Request::Wheel { .. }),
                reply,
            ) => {
                let op = match req {
                    Request::Frame { count } => DriverOp::Frame(count),
                    Request::Advance { secs } => {
                        // The shell's own clock too, so a share code ages with the frames.
                        clock::advance(secs);
                        DriverOp::Advance(secs)
                    }
                    Request::Rects {} => DriverOp::Rects,
                    Request::Touch { id, phase, x, y } => DriverOp::Touch {
                        id,
                        phase,
                        at: (x, y),
                    },
                    Request::PointerMove { x, y } => DriverOp::PointerMove((x, y)),
                    Request::PointerPress {} => DriverOp::PointerPress,
                    Request::PointerRelease {} => DriverOp::PointerRelease,
                    Request::Keyboard {
                        code,
                        key,
                        down,
                        repeat,
                        shift,
                        ctrl,
                        alt,
                        super_,
                    } => DriverOp::Keyboard(KeyInput {
                        code,
                        key,
                        down,
                        repeat,
                        cancelled: false,
                        mods: Mods {
                            shift,
                            ctrl,
                            alt,
                            super_,
                        },
                    }),
                    Request::Drag { from, to, steps } => DriverOp::Drag { from, to, steps },
                    Request::Wheel { x, y, dx, dy, ctrl } => DriverOp::Wheel {
                        at: (x, y),
                        delta: (dx, dy),
                        ctrl,
                    },
                    _ => unreachable!("matched above"),
                };
                if self.driver.is_some() {
                    let _ = reply.send(Response::err("a driver op is already pending"));
                } else {
                    self.driver = Some(PendingDriver {
                        reply: DriverReply::Rpc(reply),
                        op,
                    });
                }
                None
            }
            Msg::TestDriver(op, reply) => {
                if self.driver.is_some() {
                    let _ = reply.send(Err("a driver op is already pending".into()));
                } else {
                    self.driver = Some(PendingDriver {
                        reply: DriverReply::Test(reply),
                        op,
                    });
                }
                None
            }
            Msg::TestWorld(id, reply) => {
                match self.apps.get(&id) {
                    Some(a) => a.call(move |h| {
                        let _ = reply.send(Ok(worlds_json(&h.app.inspect_worlds())));
                    }),
                    None => {
                        let _ = reply.send(Err("test app is not open".into()));
                    }
                }
                None
            }
            Msg::TestText(id, el_id, reply) => {
                match self.apps.get(&id) {
                    Some(a) => a.call(move |h| {
                        let _ = reply.send(Ok(text_by_id(&h.app.view().info(), &el_id)));
                    }),
                    None => {
                        let _ = reply.send(Err("test app is not open".into()));
                    }
                }
                None
            }
            Msg::TestType(id, el_id, text, reply) => {
                // The thread saves — and indexes — what the input's handler wrote right after
                // this call, before it handles the test's next step.
                match self.apps.get(&id) {
                    Some(a) => a.call(move |h| {
                        let _ = reply.send(match fire(h, &el_id, Action::Type(&text)) {
                            Response::Ok { .. } => Ok(()),
                            Response::Err { message } => Err(message),
                        });
                    }),
                    None => {
                        let _ = reply.send(Err("test app is not open".into()));
                    }
                }
                None
            }
            Msg::TestRunDone(reply, results, test_id, old_focus) => {
                self.close_test_tab(&test_id, old_focus);
                let _ = reply.send(Response::ok(results));
                None
            }
            Msg::DriverDone(reply, result) => {
                // Frame paints before advancing its clock. Settle shown tiles at the reported
                // instant before replying, not during the caller's next observation.
                if let Ok(report) = &result {
                    self.before_frame(report.clock);
                }
                let (reply, result) = match reply {
                    DriverReply::NativeRects(tx, generation, snapshots) => {
                        let result = result.and_then(|report| {
                            self.validate_native_rects(generation, &snapshots)?;
                            Ok(report)
                        });
                        (DriverReply::Rpc(tx), result)
                    }
                    other => (other, result),
                };
                match reply {
                    DriverReply::NativeRects(..) => unreachable!("normalized above"),
                    DriverReply::Test(tx) => {
                        let _ = tx.send(result);
                    }
                    DriverReply::Rpc(tx) => {
                        let response = match result {
                            Ok(DriverReport {
                                clock,
                                frames,
                                rects,
                            }) => Response::ok(serde_json::json!({
                                "clock": clock,
                                "frames": frames,
                                "rects": rects.map(|rs| rs.into_iter().map(|r| serde_json::json!({
                                    "id": r.id, "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                    "hits": r.hits,
                                })).collect::<Vec<_>>()),
                            })),
                            Err(e) => Response::err(e),
                        };
                        let _ = tx.send(response);
                    }
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
                // An account switch closes every app; close them before it, so their last saves
                // land in the account they belong to.
                if outcome.succeeded() {
                    self.reset_tabs();
                }
                let (resp, next) = finish_auth(&mut self.vault, outcome);
                let _ = reply.send(resp);
                // A successful auth may have switched accounts: whatever was running
                // belongs to the previous one, and the flush below must not see it.
                if next.is_some() {
                    self.reset_tabs();
                    self.after_unlock();
                }
                next
            }
            Msg::NetBound(gen_, bound) => {
                match bound.map(|b| b.take()) {
                    Ok(Some(bound)) if gen_ == self.gen_() => {
                        self.close_net();
                        kunki::host::serve(
                            &bound,
                            kunki::host::HostCtx {
                                vault: self.vault.clone(),
                                ep: bound.endpoint().clone(),
                                sink: host::ShellSink {
                                    vault: self.vault.clone(),
                                    proxy: self.proxy.clone(),
                                    gen_,
                                    ep: Some(bound.endpoint().clone()),
                                },
                            },
                        );
                        self.net = Some(bound);
                        // Sessions wanted before the endpoint was up can dial now.
                        let wanted: Vec<String> = self
                            .apps
                            .values()
                            .map(|a| a.ws_id.clone())
                            .filter(|ws| self.joined.contains_key(ws))
                            .collect();
                        for ws in wanted {
                            self.ensure_session(&ws);
                        }
                        for (link, tx) in std::mem::take(&mut self.join_waiting) {
                            self.join_space(link, tx);
                        }
                    }
                    Ok(Some(stale)) => {
                        std::thread::spawn(move || stale.close());
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("net: bind failed: {e}"),
                }
                None
            }
            Msg::Rpc(req, tx) => {
                if let Some((req, tx)) = self.forward(req, tx) {
                    let _ = tx.send(self.answer_mut(req));
                }
                None
            }

            Msg::SyncTick => {
                // Re-armed unconditionally: a node that is slow or unreachable this tick must
                // not stop the next one from trying.
                arm_sync_tick(&self.proxy);
                self.reconcile();
                None
            }
            Msg::SyncDone(gen_, _, _, _)
            | Msg::ListenUp(gen_)
            | Msg::PushReceived(gen_, _)
            | Msg::SubscribeFailed(gen_, _, _)
            | Msg::SessionUp(gen_, _, _)
            | Msg::SessionDown(gen_, _, _, _)
            | Msg::SessionFrame(gen_, _, _, _)
            | Msg::SessionToken(gen_, _, _, _)
            | Msg::SessionRedial(gen_, _)
            | Msg::NetFrame(gen_, _, _, _)
            | Msg::NetDown(gen_, _, _)
                if gen_ != self.gen_() =>
            {
                None
            }
            Msg::SyncDone(_, item_id, name, result) => {
                let key = (item_id.clone(), name.clone());
                match result {
                    Ok(SyncReply::Rejected(r)) => self.roll_back(&item_id, name, r),
                    Ok(SyncReply::Ack(ack)) => {
                        if self.apps.contains_key(&item_id) {
                            self.import(&item_id, None, name.clone(), ack.update);
                            if ack.missing {
                                // Our `since` was ahead of the node: push everything.
                                self.node_vv.remove(&key);
                                self.sync_one(&item_id, &name);
                            } else if newer_vv(self.node_vv.get(&key), &ack.vv) {
                                self.node_vv.insert(key, ack.vv);
                            }
                        }
                    }
                    Err(e) => eprintln!("sync: {item_id}/{name} failed: {e}"),
                }
                None
            }
            Msg::ListenUp(_) => {
                self.subscribed.clear();
                self.node_vv.clear();
                // At once: subscribing rides on an app's save, and a quiet app may not save
                // before the next tick.
                self.reconcile();
                None
            }
            Msg::SubscribeFailed(_, item_id, name) => {
                self.failed_subs.push((item_id, name));
                None
            }
            Msg::PushReceived(_, push) => {
                if let SyncLayer::Doc(name) = &push.layer {
                    let id: Arc<str> = push.item_id.as_str().into();
                    self.import(&id, Some(&push.ws_id), name.clone(), push.snapshot);
                }
                None
            }

            Msg::HostSync(gen_, hello, device, reply) => {
                if let Some(reply) = reply.take() {
                    self.host_sync(gen_, hello, device, reply);
                }
                None
            }
            Msg::SessionDown(_, ws, seq, reason) => {
                self.session_down(ws, seq, reason);
                None
            }
            Msg::JoinFetched(gen_, tx, result) => {
                self.join_fetched(gen_, tx, result);
                None
            }
            Msg::SessionUp(_, ws, seq) => {
                self.session_up(ws, seq);
                None
            }
            Msg::SessionRedial(_, ws) => {
                if self.session_wanted(&ws) {
                    self.ensure_session(&ws);
                }
                None
            }
            Msg::SessionToken(_, ws, seq, token) => {
                self.session_token(ws, seq, token);
                None
            }
            Msg::SessionFrame(_, ws, seq, frame) => {
                self.session_frame(ws, seq, frame);
                None
            }
            Msg::NetFrame(_, ws, seq, frame) => {
                self.net_frame(ws, seq, frame);
                None
            }
            Msg::NetDown(_, ws, seq) => {
                self.net_down(ws, seq);
                None
            }
            Msg::NetOut(id, out) => {
                self.net_out(id, out);
                None
            }
            msg => {
                let next = match (&mut self.screen, msg) {
                    (Screen::Signup(f), Msg::Signup(m)) => f.update(m, &mut self.vault, &self.proxy),
                    (Screen::Mnemonic(f), Msg::Mnemonic(m)) => f.update(m, &self.vault),
                    (Screen::Login(f), Msg::Login(m)) => f.update(m, &mut self.vault, &self.proxy),
                    (Screen::Spaces(s), Msg::Space(m)) => s.update(m, &mut self.vault, &self.proxy),
                    (Screen::Items(i), Msg::Items(m)) => i.update(m, &mut self.vault, &self.proxy),
                    _ => None,
                };
                if unlocks(&self.screen, next.as_ref()) {
                    self.after_unlock();
                }
                next
            }
        };
        // Saving, indexing and reloading happen on each app's own thread now, after every
        // batch it handles (`app_thread::settle`); the shell only keeps tiles shown or hidden.
        self.show_on_screen();

        if let Some(next) = next {
            self.screen = next;
        }
        self.sync_launcher();
    }

    /// Wakes on wall-clock time or from a subscriber, never a request: offscreen they must not move
    /// the clock, or a 20s sync timer shifts whatever the driver is timing by a frame.
    fn is_ambient(&self, msg: &Msg) -> bool {
        matches!(
            msg,
            Msg::SyncTick
                | Msg::SyncDone(..)
                | Msg::NetBound(..)
                | Msg::HostSync(..)
                | Msg::SessionUp(..)
                | Msg::SessionDown(..)
                | Msg::SessionFrame(..)
                | Msg::SessionToken(..)
                | Msg::SessionRedial(..)
                | Msg::NetFrame(..)
                | Msg::NetDown(..)
                | Msg::NetOut(..)
                | Msg::JoinFetched(..)
                | Msg::PushReceived(..)
                | Msg::ListenUp(_)
                | Msg::SubscribeFailed(..)
                | Msg::TileDirty(_)
                | Msg::AppSaved { .. }
        )
    }

    fn before_frame(&mut self, now: f64) {
        self.clock = now;
        // What apps played since the last frame starts at the last frame's time: exact offscreen.
        self.sound.advance_to(now);
        // Ask every shown tile first, then collect: they frame in parallel, not in turn.
        let asked: Vec<_> = self
            .apps
            .iter()
            .filter(|(_, a)| a.is_shown())
            .map(|(id, a)| (id.clone(), a.ask(now)))
            .collect();
        for (id, asked) in asked {
            if let Some(a) = self.apps.get_mut(&id) {
                a.take(asked);
            }
        }
    }

    fn windows(&self) -> Vec<WindowFrame> {
        self.popped
            .iter()
            .filter_map(|id| {
                let a = self.apps.get(id)?;
                let title = self.tabs.iter().find_map(|t| match t {
                    Tab::App((tid, name)) if tid == id => Some(name.clone()),
                    _ => None,
                })?;
                Some(WindowFrame {
                    key: id.to_string(),
                    title,
                    frame: a.frame.clone(),
                })
            })
            .collect()
    }

    fn window_event(&mut self, key: &str, event: WindowIn) {
        match event {
            WindowIn::Input(e) => {
                if let Some(a) = self.apps.get(key) {
                    a.input(e);
                }
            }
            WindowIn::Resized(size) => {
                if let Some(a) = self.apps.get_mut(key) {
                    a.resize(size, self.clock);
                }
            }
            // Docks rather than closes: closing a window must not lose the app.
            WindowIn::Closed => {
                self.dock(key);
                self.show_on_screen();
            }
        }
    }

    fn tiles_sized(&mut self, sizes: &[(String, (f32, f32))]) -> bool {
        let mut again = false;
        for (id, size) in sizes {
            if let Some(a) = self.apps.get_mut(id.as_str()) {
                again |= a.resize(*size, self.clock);
            }
        }
        again
    }

    fn take_driver(&mut self) -> Option<DriverRequest<Msg>> {
        let pending = self.driver.take()?;
        Some(DriverRequest {
            op: pending.op,
            complete: Box::new(move |result| Msg::DriverDone(pending.reply, result)),
        })
    }

    fn take_screenshot(&mut self) -> Option<ScreenshotRequest<Msg>> {
        let pending = self.screenshot.take()?;
        let item_unavailable = pending.item_id.as_ref().is_some_and(|item_id| {
            let focused = self
                .tabs
                .get(self.focused)
                .is_some_and(|t| matches!(t, Tab::App((id, _)) if id.as_ref() == item_id));
            !focused || !self.apps.contains_key(item_id.as_str())
        });
        if item_unavailable {
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
