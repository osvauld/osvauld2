//! One open app on its own thread (`docs/design/app-threads.md` step 3): its VM, docs, search
//! index and [`Tile`] are built there and never leave. The shell holds an [`AppThread`] —
//! a channel in, the latest frame out — and never calls into the app directly.
//!
//! Every app runs, shown or not: a hidden one sleeps on its channel and still answers the
//! bridge, imports node pushes and saves. Only a shown one paints.
//!
//! What crosses is `Send`: input, sizes, and [`Call`]s — closures run against the app on its
//! thread, which reply through whatever channel they captured. `Send` is what keeps an `Rc`,
//! a doc or the VM from crossing by accident.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use app_host::{LuaApp, LuaMsg};
use courier::token::Token;
use loro::LoroDoc;
use runtime::{App, El, EventLoopProxy, Tile, TileEvent, TileFrame};
use vault::Vault;

use crate::{Msg, attach_index, indexer, persist, reindex, resolver, resolver_with_node};

/// A request run on the app's thread.
pub type Call = Box<dyn FnOnce(&mut Hosted) + Send>;

/// How long the shell waits on a thread before treating it as stuck: past the Lua budget, so
/// a killed runaway always answers first.
const STUCK: Duration = Duration::from_secs(3);
/// An animating, shown tile's frame interval with a window.
const FRAME: Duration = Duration::from_millis(16);

/// The app as its thread holds it.
pub struct Hosted {
    pub app: LuaApp<LuaMsg>,
    pub index: Option<indexer::Shared>,
    pub ws_id: String,
    pub item_id: Arc<str>,
    /// Just shown again: a hidden thread does not frame, so its 3D clocks never heard the pause.
    resumed: bool,
}

impl App for Hosted {
    type Msg = LuaMsg;
    /// Inside a full column, as it sat in the shell's page: an app's `grow` root fills the tile.
    fn view(&self) -> El<LuaMsg> {
        runtime::col().full().child(self.app.view())
    }
    fn update(&mut self, msg: LuaMsg) {
        self.app.update(msg);
    }
    /// Only shown tiles frame, so this runs only while shown — except the first frame back,
    /// which pauses instead, so the hidden time is skipped rather than caught up as debt.
    fn advance_simulation(&mut self, elapsed: f64) -> bool {
        if std::mem::take(&mut self.resumed) {
            self.app.advance_simulation(elapsed, false);
            return true;
        }
        self.app.advance_simulation(elapsed, true)
    }
}

/// What opening an app needs; all of it `Send`.
pub struct Open {
    pub item_id: Arc<str>,
    pub ws_id: String,
    pub src: Vec<u8>,
    pub vault: Vault,
    /// Saved to the vault, and resolved through the node when one is claimed. `false` is a
    /// test tab: no vault reads or writes, an in-memory index.
    pub persist: bool,
    pub node: Option<(String, Token)>,
}

enum In {
    Event(TileEvent),
    Resize((f32, f32)),
    Shown(bool),
    /// Offscreen: catch up to the host's clock, and answer with a frame if one is owed.
    Sync(f64, Sender<Option<TileFrame>>),
    Call(Call),
    Wake,
    Close,
}

pub struct AppThread {
    tx: Sender<In>,
    pub ws_id: String,
    /// What the shell paints: the latest frame it has.
    pub frame: TileFrame,
    /// A windowed thread's newest frame, waiting for the shell's next paint.
    latest: Arc<Mutex<Option<TileFrame>>>,
    /// Set once the shell lets go; the thread writes nothing after it sees this.
    closed: Arc<AtomicBool>,
    done: Receiver<()>,
    offscreen: bool,
    shown: bool,
    size: Option<(f32, f32)>,
    /// The docs the app had open at its last save — what sync subscribes to.
    pub doc_names: Vec<String>,
    /// The thread did not answer within [`STUCK`].
    pub stuck: bool,
}

impl AppThread {
    /// Start the thread and wait for the app to load — an open error is the caller's to show,
    /// as before.
    pub fn spawn(
        open: Open,
        proxy: EventLoopProxy<Msg>,
        offscreen: Option<(f32, f32)>,
    ) -> Result<Self, String> {
        let (tx, rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let (done_tx, done) = channel();
        let latest = Arc::new(Mutex::new(None));
        let closed = Arc::new(AtomicBool::new(false));
        let ws_id = open.ws_id.clone();
        let size = offscreen.unwrap_or((800.0, 600.0));
        let ctx = Ctx {
            id: open.item_id.clone(),
            ws_id: open.ws_id.clone(),
            vault: open.vault.clone(),
            persist: open.persist,
            proxy,
            latest: latest.clone(),
            closed: closed.clone(),
            offscreen: offscreen.is_some(),
        };
        let wake_tx = tx.clone();
        std::thread::Builder::new()
            .name(format!("app {}", open.item_id))
            .spawn(move || {
                let wake: app_host::Wake = Arc::new(move || {
                    let _ = wake_tx.send(In::Wake);
                });
                let tile = match build(open, wake) {
                    Ok(hosted) => {
                        let tile = Tile::new(hosted, size);
                        if ctx.offscreen { tile.with_virtual_clock() } else { tile }
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                run(tile, rx, ctx);
                let _ = done_tx.send(());
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv()
            .map_err(|_| "the app thread ended while opening".to_string())??;
        Ok(Self {
            tx,
            ws_id,
            frame: TileFrame::default(),
            latest,
            closed,
            done,
            offscreen: offscreen.is_some(),
            shown: false,
            size: None,
            doc_names: Vec::new(),
            stuck: false,
        })
    }

    pub fn call(&self, f: impl FnOnce(&mut Hosted) + Send + 'static) {
        let _ = self.tx.send(In::Call(Box::new(f)));
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    pub fn input(&self, event: TileEvent) {
        let _ = self.tx.send(In::Event(event));
    }

    pub fn show(&mut self, shown: bool) {
        if self.shown != shown {
            self.shown = shown;
            let _ = self.tx.send(In::Shown(shown));
        }
    }

    /// `true` when the frame now held is at the new size — offscreen, where it was waited for.
    pub fn resize(&mut self, size: (f32, f32), now: f64) -> bool {
        if self.size == Some(size) {
            return false;
        }
        self.size = Some(size);
        let _ = self.tx.send(In::Resize(size));
        self.offscreen && self.sync(now)
    }

    /// Before the shell paints. Offscreen: wait for this tile to reach `now` and take its frame,
    /// which is what keeps a driven run exact. With a window: take whatever frame is newest and
    /// never wait. `true` when the frame changed.
    pub fn sync(&mut self, now: f64) -> bool {
        let asked = self.ask(now);
        self.take(asked)
    }

    /// `sync`'s first half: offscreen, ask for the frame at `now` without waiting for it, so
    /// several tiles can be asked before any is waited on.
    pub fn ask(&self, now: f64) -> Option<Receiver<Option<TileFrame>>> {
        if !self.offscreen {
            return None;
        }
        let (tx, rx) = channel();
        self.tx.send(In::Sync(now, tx)).ok().map(|()| rx)
    }

    /// `sync`'s second half: the frame `ask` asked for, or with a window the newest one.
    pub fn take(&mut self, asked: Option<Receiver<Option<TileFrame>>>) -> bool {
        if !self.offscreen {
            return match self.latest.lock().unwrap().take() {
                Some(f) => {
                    self.frame = f;
                    true
                }
                None => false,
            };
        }
        let Some(rx) = asked else {
            return false;
        };
        match rx.recv_timeout(STUCK) {
            Ok(Some(f)) => {
                self.stuck = false;
                self.frame = f;
                true
            }
            Ok(None) => {
                self.stuck = false;
                false
            }
            Err(_) => {
                self.stuck = true;
                false
            }
        }
    }
}

impl Drop for AppThread {
    /// Close, and wait for the last save to land: whoever closed this may read the vault next,
    /// or switch accounts. A stuck thread is abandoned, and writes nothing after this.
    fn drop(&mut self) {
        let _ = self.tx.send(In::Close);
        if self.done.recv_timeout(STUCK).is_err() {
            eprintln!("app thread did not close in time; abandoning it");
        }
        self.closed.store(true, Ordering::SeqCst);
    }
}

fn build(open: Open, wake: app_host::Wake) -> Result<Hosted, String> {
    let doc = LoroDoc::new();
    doc.import(&open.src).map_err(|e| e.to_string())?;
    let resolve = match (&open.node, open.persist) {
        (_, false) => std::rc::Rc::new(|_: &str| Ok(None)) as app_host::Resolve,
        (Some((did, token)), true) => resolver_with_node(
            &open.vault,
            &open.ws_id,
            &open.item_id,
            did.clone(),
            token.clone(),
        ),
        (None, true) => resolver(&open.vault, &open.ws_id, &open.item_id),
    };
    let mut app = LuaApp::open(doc, resolve, wake, std::rc::Rc::new(|m| m))
        .map_err(|e| e.to_string())?;
    let opened = if open.persist {
        indexer::ItemIndex::open(&open.vault, &open.ws_id, &open.item_id)
    } else {
        // A tab that persists nothing still searches: its own index, in memory, gone with it.
        indexer::ItemIndex::in_memory()
    };
    let index = attach_index(&mut app, opened);
    Ok(Hosted {
        app,
        index,
        ws_id: open.ws_id,
        item_id: open.item_id,
        resumed: false,
    })
}

/// The thread's side of everything but the app itself.
struct Ctx {
    id: Arc<str>,
    ws_id: String,
    vault: Vault,
    persist: bool,
    proxy: EventLoopProxy<Msg>,
    latest: Arc<Mutex<Option<TileFrame>>>,
    closed: Arc<AtomicBool>,
    offscreen: bool,
}

fn run(mut tile: Tile<Hosted>, rx: Receiver<In>, ctx: Ctx) {
    let mut shown = false;
    let mut names = Vec::new();
    loop {
        let animating = shown && !ctx.offscreen && tile.wants_frame();
        let first = if animating {
            match rx.recv_timeout(FRAME) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        };
        // Changed by the app itself (a timer, a doc subscriber) — the shell does not know to
        // paint. Not by a call: the shell paints after delivering whatever sent it, and
        // offscreen an extra paint at the same instant is a zero-length tick the app can see.
        let mut woke = false;
        for msg in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            match msg {
                In::Event(e) => tile.input(e),
                In::Resize(size) => tile.resize(size),
                In::Shown(s) => {
                    shown = s;
                    if s {
                        tile.app_mut().resumed = true;
                        tile.invalidate();
                    }
                }
                In::Call(f) => {
                    f(tile.app_mut());
                    tile.invalidate();
                }
                In::Wake => {
                    tile.invalidate();
                    woke = true;
                }
                In::Sync(now, reply) => {
                    settle(&mut tile, &ctx, &mut names);
                    tile.set_clock(now);
                    let frame = tile.wants_frame().then(|| TileFrame {
                        scene: Arc::new(tile.frame()),
                        rects: tile.rects().into(),
                        view3d: tile.view3d(),
                    });
                    let _ = reply.send(frame);
                }
                In::Close => {
                    settle(&mut tile, &ctx, &mut names);
                    return;
                }
            }
        }
        settle(&mut tile, &ctx, &mut names);
        if shown && !ctx.offscreen && tile.wants_frame() {
            let frame = TileFrame {
                scene: Arc::new(tile.frame()),
                rects: Arc::new([]),
                view3d: tile.view3d(),
            };
            *ctx.latest.lock().unwrap() = Some(frame);
            let _ = ctx.proxy.send_event(Msg::TileDirty(ctx.id.clone()));
        } else if shown && woke {
            // Offscreen the shell's next paint fetches the frame; it only needs telling.
            let _ = ctx.proxy.send_event(Msg::TileDirty(ctx.id.clone()));
        }
    }
}

/// After every batch, as the shell used to after every message: rebuild a stale source, save
/// what changed, index it, and tell the shell what to sync.
fn settle(tile: &mut Tile<Hosted>, ctx: &Ctx, names: &mut Vec<String>) {
    if ctx.closed.load(Ordering::SeqCst) {
        return;
    }
    let h = tile.app_mut();
    if let Some(Err(e)) = h.app.reload_if_stale() {
        eprintln!("reload failed: {e}");
        tile.invalidate();
    }
    let h = tile.app_mut();
    let mut dirtied = Vec::new();
    let mut error = None;
    if ctx.persist {
        let mut put = persist(&ctx.vault, &ctx.ws_id, &ctx.id);
        if let Err(e) = h.app.flush(|name, bytes| {
            dirtied.push(name.to_string());
            put(name, bytes)
        }) {
            error = Some(format!("save failed: {e}"));
        }
    } else {
        // Nothing to save, but the flush still says what changed — the index needs it.
        let _ = h.app.flush(|name, _| {
            dirtied.push(name.to_string());
            Ok(())
        });
    }
    // After the save, so the index never holds a record the vault does not.
    reindex(&h.index, &h.app, &dirtied);
    let now_open = h.app.open_doc_names();
    if dirtied.is_empty() && error.is_none() && now_open == *names {
        return;
    }
    *names = now_open.clone();
    let _ = ctx.proxy.send_event(Msg::AppSaved {
        id: ctx.id.clone(),
        dirtied,
        names: now_open,
        error,
    });
}
