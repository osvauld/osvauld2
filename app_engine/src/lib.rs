//! Renders uploadable apps as a homegrown declarative UI engine, composited natively into the
//! egui shell: a Lua script returns a [`Node`] tree, laid out with Taffy and painted with egui
//! into a [`Frame`] of native meshes (the shell adds no GPU glue beyond egui's own).
//!
//! Text shapes via egui's fonts for now (no complex scripts); that swaps to parley/cosmic-text
//! behind the `layout::shape` seam when it matters.

pub mod data;
mod layout;
mod node;
mod paint;
mod pdf;
mod script;
mod shot;
mod table;

pub use data::{DataAccess, DataAccessRef, Handle, NamedOp, QueryState};
pub use node::{ChartKind, ChartSeries, ChartSpec};
pub use pdf::FontBytes;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use egui::Color32;
use loro::{ExportMode, LoroDoc};
use text_edit::TextField;

pub use node::{Direction, Node, Style, Val};

/// A print-page declaration from the app (`page = { size = "A4", orientation = "landscape" }`):
/// fixed host-rect dimensions in logical px, sized so 1 px = 1 PDF pt.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PageSpec {
    pub width: f32,
    pub height: f32,
}

/// One frame's drawing from an app: GPU-ready triangles plus their texture uploads and a
/// repaint signal.
pub struct Frame {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
    /// When to redraw (egui's repaint delay): `ZERO` while animating, `MAX` when idle.
    pub repaint_after: Duration,
}

/// Where an app's Lua comes from at run time (never compiled in): an in-memory string or a
/// hot-reloaded file. The canonical store later becomes a CRDT text container behind this seam.
enum Source {
    /// A fixed in-memory string (tests, the built-in fallback) — never reloads.
    Inline,
    /// A file read at startup and re-read whenever its mtime advances (hot-reload).
    File { path: PathBuf, loaded: Option<SystemTime> },
}

impl Source {
    /// Read the current source text, or a message to show in the cell on failure.
    fn read(&mut self) -> Result<String, String> {
        match self {
            Source::Inline => Err("inline source cannot be reloaded".to_string()),
            Source::File { path, loaded } => {
                let text = std::fs::read_to_string(&*path)
                    .map_err(|e| format!("can't read app source {}: {e}", path.display()))?;
                *loaded = mtime(path);
                Ok(text)
            }
        }
    }

    /// Has the file changed (or appeared) since the last read? Always false for `Inline`.
    fn changed(&self) -> bool {
        match self {
            Source::Inline => false,
            Source::File { path, loaded } => {
                let now = mtime(path);
                now.is_some() && now != *loaded
            }
        }
    }

    /// How often to poll for edits while idle (dev convenience until a file watcher exists);
    /// `None` = no polling.
    fn poll_interval(&self) -> Option<Duration> {
        match self {
            Source::Inline => None,
            Source::File { .. } => Some(Duration::from_millis(400)),
        }
    }
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// A fresh engine context with the rich-text fonts (the bold face) installed, so `ui.text`
/// marks render.
fn engine_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    rich_text::install_fonts(&ctx);
    // Match the shell: Gamma(0.7) keeps the AA ramp on curves (dark default hardens it).
    ctx.global_style_mut(|s| {
        s.visuals.text_options.alpha_from_coverage = egui::epaint::AlphaFromCoverage::Gamma(0.7);
    });
    ctx
}

pub use table_core::row_id;

/// The doc snapshot file for a source path — the source with `.snapshot` appended.
fn snapshot_path(source: &Path) -> PathBuf {
    let mut p = source.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p)
}

/// Where a frame's view tree comes from: a fixed Rust tree, or a Lua script (plus its runtime
/// source) re-run each frame (`view = f(state)`).
enum ViewSource {
    Static(Node),
    Script { source: Source, script: script::Script },
}

/// A running engine app: its persistent egui `Context`, the source of the view tree it draws,
/// and the CRDT its data lives in. One `frame` call per repaint.
///
/// The `doc` is owned here, *outside* the script, so it survives hot-reloads — editing the code
/// never wipes the data — and is the seam where a peer or MCP writes the same CRDT.
pub struct EngineApp {
    ctx: egui::Context,
    view: ViewSource,
    doc: Rc<LoroDoc>,
    /// Where the doc is persisted between runs (a Loro snapshot beside the source); `None` for
    /// in-memory/static apps. Dev-grade — production home is the account's encrypted Store.
    data_path: Option<PathBuf>,
    /// Retained caret/selection per editable field, keyed by `ui.editor{ id }`. The view tree is
    /// rebuilt every frame, so this is the one widget state the engine keeps across frames.
    fields: HashMap<String, TextField>,
    /// The focused editor's id, if any — where keyboard input is routed.
    focus: Option<String>,
    /// Retained scroll position (points, per axis) per scroll region, keyed by `ui.col{ scroll }`.
    scroll: HashMap<String, egui::Vec2>,
    /// An in-progress scrollbar thumb drag: (region id, horizontal axis, grab offset in the thumb).
    drag_bar: Option<(String, bool, f32)>,
    /// An in-progress table resize drag: (target, size at press, pointer coord at press).
    drag_size: Option<(node::Resize, f32, f32)>,
    /// Whether the focused field has uncommitted-feeling edits this focus session — blur then
    /// fires a cell's `on_edit` once. (The CRDT already holds the keystrokes; this gates the hook.)
    focus_dirty: bool,
    /// Last frame's laid-out scene, reused when only the pointer moved (hover). egui repaints on
    /// every pointer-move, but an unchanged [`SceneKey`] with no edit or live drag means the layout
    /// is identical, so `show()` skips view() + Taffy and just repaints.
    scene: Option<SceneCache>,
    /// Monotonic count of scene rebuilds; a reused (cached) hover frame does not bump it. Telemetry,
    /// and lets a test assert that a pointer-only repaint skips the rebuild.
    scene_rebuilds: u64,
    /// Live embedded block editors, keyed by `ui.doc{ id }`. Each drives a `doc_editor::Doc` over a
    /// `uidoc:<id>` tree inside `doc`, so its blocks persist + sync with the rest of the app's CRDT.
    /// Retained across frames (caret/scroll) and across hot-reloads (the data outlives the code).
    docs: HashMap<String, DocCell>,
    /// The host's World-B data plane (`data` Lua binding), if this app was given one. Imported
    /// `.table` sources resolve through it; its composite version folds into the scene key so an
    /// external source write triggers a rebuild. `None` = no imported-table access (tests, `.table`,
    /// static).
    data: Option<data::DataAccessRef>,
}

/// A live embedded block document: the native editor's retained UI state plus the `Doc` view over
/// its tree in the app's shared CRDT.
struct DocCell {
    editor: doc_editor::DocEditor,
    doc: doc_editor::Doc,
}

/// A cached frame: the laid-out boxes and the key they were built under.
struct SceneCache {
    key: SceneKey,
    placed: Vec<layout::Placed>,
}

/// Everything that changes the laid-out scene. Equal key (plus no edit/drag in flight) ⇒ last
/// frame's `placed` still holds; hover and caret are applied at paint time, so they track the
/// pointer without a rebuild.
#[derive(PartialEq)]
struct SceneKey {
    frontiers: loro::Frontiers,
    rect: egui::Rect,
    scroll: HashMap<String, egui::Vec2>,
    focus: Option<String>,
    /// Composite version of the app's imported `.table` sources (0 with no data plane). A bump
    /// (an external source write) differs the key, forcing a rebuild + recompute of `data.sql`.
    data_version: u64,
}

impl EngineApp {
    /// Build an engine app around a fixed view tree (no scripting).
    pub fn new(root: Node) -> Self {
        EngineApp {
            ctx: engine_ctx(),
            view: ViewSource::Static(root),
            doc: Rc::new(LoroDoc::new()),
            data_path: None,
            fields: HashMap::new(),
            focus: None,
            scroll: HashMap::new(),
            drag_bar: None,
            drag_size: None,
            focus_dirty: false,
            scene: None,
            scene_rebuilds: 0,
            docs: HashMap::new(),
            data: None,
        }
    }

    /// The built-in static demo tree — a titled card with tag pills, built in Rust.
    pub fn demo() -> Self {
        EngineApp::new(demo_tree())
    }

    /// Build an engine app from an in-memory Lua string (tests / fallback). No persistence.
    pub fn script(source: &str) -> Self {
        let doc = Rc::new(LoroDoc::new());
        let script = script::Script::load(source, doc.clone());
        EngineApp {
            ctx: engine_ctx(),
            view: ViewSource::Script { source: Source::Inline, script },
            doc,
            data_path: None,
            fields: HashMap::new(),
            focus: None,
            scroll: HashMap::new(),
            drag_bar: None,
            drag_size: None,
            focus_dirty: false,
            scene: None,
            scene_rebuilds: 0,
            docs: HashMap::new(),
            data: None,
        }
    }

    /// Build an engine app from a vault-stored source tree (`(path, source)` pairs, e.g.
    /// `main.lua` + `lib/*.lua`) and an optional runtime CRDT snapshot. No hot-reload — the
    /// source is fixed at load time; the caller persists the CRDT back to the vault.
    pub fn from_files(files: &[(String, String)], crdt_snapshot: Option<&[u8]>) -> Self {
        let doc = Rc::new(LoroDoc::new());
        if let Some(bytes) = crdt_snapshot {
            let _ = doc.import(bytes);
        }
        let script = script::Script::load_app(files, doc.clone());
        EngineApp {
            ctx: engine_ctx(),
            view: ViewSource::Script { source: Source::Inline, script },
            doc,
            data_path: None,
            fields: HashMap::new(),
            focus: None,
            scroll: HashMap::new(),
            drag_bar: None,
            drag_size: None,
            focus_dirty: false,
            scene: None,
            scene_rebuilds: 0,
            docs: HashMap::new(),
            data: None,
        }
    }

    /// Build a `.table` view over a stored CRDT snapshot (schema + `rows`): a grid rendered
    /// natively from the doc's stored schema, no Lua app. `color` is the host text colour the grid
    /// tints its chrome from. Writes (cell edits) flow back to the doc like any app's runtime.
    pub fn table(crdt_snapshot: Option<&[u8]>, color: egui::Color32, font: f32) -> Self {
        let doc = Rc::new(LoroDoc::new());
        if let Some(bytes) = crdt_snapshot {
            let _ = doc.import(bytes);
        }
        let script = script::Script::table(doc.clone(), color, font);
        EngineApp {
            ctx: engine_ctx(),
            view: ViewSource::Script { source: Source::Inline, script },
            doc,
            data_path: None,
            fields: HashMap::new(),
            focus: None,
            scroll: HashMap::new(),
            drag_bar: None,
            drag_size: None,
            focus_dirty: false,
            scene: None,
            scene_rebuilds: 0,
            docs: HashMap::new(),
            data: None,
        }
    }

    /// Give this app a host data plane (the `data` Lua binding) for reading imported `.table`
    /// sources. Builder form, used by the host when it opens an app tab. Re-installed across
    /// hot-reloads so the binding survives a code edit.
    pub fn with_data_access(mut self, access: data::DataAccessRef) -> Self {
        self.data = Some(access);
        self.install_data();
        self
    }

    /// Install the held data plane into the current script's VM, if both exist.
    fn install_data(&self) {
        if let (Some(access), ViewSource::Script { script, .. }) = (&self.data, &self.view) {
            if let Err(e) = script.install_data(access.clone()) {
                eprintln!("app_engine: data binding failed: {e}");
            }
        }
    }

    /// Rebuild the script from edited source, keeping the live runtime CRDT so state survives the
    /// edit. Ephemeral UI state (focus/caret/scroll) resets — the node tree may have changed.
    pub fn reload_source(&mut self, files: &[(String, String)]) {
        let script = script::Script::load_app(files, self.doc.clone());
        self.view = ViewSource::Script { source: Source::Inline, script };
        self.install_data();
        self.fields.clear();
        self.focus = None;
        self.focus_dirty = false;
        self.scroll.clear();
        self.scene = None;
    }

    /// The app's print-page declaration, if it made one. The host renders such an app inside a
    /// fixed page-sized rect (print preview) and may export it to PDF.
    pub fn page(&self) -> Option<PageSpec> {
        match &self.view {
            ViewSource::Script { script, .. } => script.page(),
            ViewSource::Static(_) => None,
        }
    }

    /// Hot-reload the script if a file backs it and its source changed; the same `doc` carries
    /// over, so editing the code keeps the data. Returns the idle poll interval for file sources.
    fn hot_reload(&mut self) -> Option<Duration> {
        let doc = self.doc.clone();
        let ViewSource::Script { source, script } = &mut self.view else { return None };
        let reloaded = source.changed();
        if reloaded {
            *script = match source.read() {
                Ok(text) => script::Script::load(&text, doc),
                Err(err) => script::Script::failed(err),
            };
        }
        let interval = source.poll_interval();
        if reloaded {
            self.install_data(); // the new script's VM needs the `data` binding re-installed
            self.scene = None; // the tree changed though the doc version didn't — drop stale scene
        }
        interval
    }

    /// Route this frame's keyboard to the focused editor *before* the view resolves, so the view
    /// re-reads the post-edit content this same frame. The field re-clamps against the live
    /// buffer so the caret survives an external/remote edit (peer, MCP, or a clearing
    /// `on_submit`). Enter fires `on_submit` after the keys, so a batched `Text`+`Enter` submits
    /// the just-typed value. Returns (edited, submitted).
    fn apply_edits(&mut self, edits: &[Edit], submit: bool) -> (bool, bool) {
        let Some(id) = self.focus.clone() else { return (false, false) };
        let ViewSource::Script { script, .. } = &self.view else { return (false, false) };
        let mut field = self.fields.get(&id).copied().unwrap_or_default();
        let edited = script.with_buffer(&id, |buf| {
            field.clamp(&*buf);
            for e in edits {
                apply_edit(&mut field, &mut *buf, e);
            }
        });
        self.fields.insert(id.clone(), field);
        self.focus_dirty |= edited;
        let mut submitted = false;
        if submit {
            match script.submit(&id) {
                Ok(fired) => submitted = fired,
                Err(err) => eprintln!("app_engine: on_submit error: {err}"),
            }
            // Enter already committed; the eventual blur shouldn't re-fire.
            self.focus_dirty = false;
        }
        (edited, submitted)
    }

    /// Resolve this frame's view tree (the script runs knowing the focused id — a focused select
    /// cell opens its dropdown). A non-page app gets the web-page scroll root: a natural-height
    /// root overflows the synthetic region; a `height = "100%"` root fills the host (dashboard
    /// semantics) and never does.
    fn resolve_root(&mut self, viewport_h: f32, scroll: &HashMap<String, egui::Vec2>) -> Node {
        let root = match &mut self.view {
            ViewSource::Static(node) => node.clone(),
            ViewSource::Script { script, .. } => {
                script.set_focus(self.focus.as_deref());
                script.set_viewport(viewport_h, scroll);
                script.view().unwrap_or_else(|err| error_card(&err))
            }
        };
        if self.page().is_none() { scroll_root(root) } else { root }
    }

    /// Apply one frame's sensed interactions to the engine state: drag starts and live updates,
    /// the click (editor focus / handler dispatch / empty-space blur), the blur commit, selection
    /// drag, wheel + thumb scrolling, and drag release.
    fn apply(&mut self, prev_focus: Option<String>, ix: Interactions) -> Applied {
        if let Some(press) = ix.bar_press {
            self.drag_bar = Some(press);
        }
        if let Some(press) = ix.size_press {
            self.drag_size = Some(press);
        }
        // A held resize handle tracks the pointer (live — the next view() applies it); the
        // declared sizes become the initial values.
        if let (Some((target, start, origin)), Some(p)) = (self.drag_size.clone(), ix.pointer) {
            if let ViewSource::Script { script, .. } = &self.view {
                match &target {
                    node::Resize::Col { table, key } => {
                        script.set_col_width(table, key, (start + p.x - origin).max(40.0))
                    }
                    node::Resize::Row { table, row } => {
                        script.set_row_height(table, row, (start + p.y - origin).max(22.0))
                    }
                }
            }
        }

        // Apply the click: focus + caret on an editor; dispatch a handler (keeping focus); empty
        // space blurs. A handler error keeps the current view (logged).
        let mut dispatched = false;
        if let Some((id, idx, extend)) = ix.editor_click {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, extend);
            self.fields.insert(id.clone(), field);
            self.focus = Some(id);
        } else if let (Some(id), ViewSource::Script { script, .. }) = (ix.hit, &mut self.view) {
            if let Err(err) = script.dispatch(id) {
                eprintln!("app_engine: on_click error: {err}");
            }
            dispatched = true;
        } else if ix.miss {
            self.focus = None;
        }
        // A select commit asked to close its dropdown: drop focus (the blur below clears the query).
        if let ViewSource::Script { script, .. } = &self.view {
            if script.take_defocus() {
                self.focus = None;
            }
        }

        // Leaving an edited field commits it: a cell's `on_edit` fires once on blur.
        let mut blurred = false;
        if prev_focus != self.focus {
            if let (Some(old), true, ViewSource::Script { script, .. }) =
                (&prev_focus, self.focus_dirty, &self.view)
            {
                match script.blur(old) {
                    Ok(fired) => blurred = fired,
                    Err(err) => eprintln!("app_engine: on_edit error: {err}"),
                }
            }
            self.focus_dirty = false;
        }

        // A drag extends the focused field's selection (the click above set its anchor).
        let dragging = ix.drag_to.is_some();
        if let Some((id, idx)) = ix.drag_to {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, true);
            self.fields.insert(id, field);
        }

        // Apply the wheel to the hovered scroll regions (egui's convention: offset -= delta).
        let mut scrolled = false;
        for (id, max, d) in ix.wheel {
            let off = self.scroll.entry(id).or_default();
            let new = (*off - d).clamp(egui::Vec2::ZERO, max);
            scrolled |= new != *off;
            *off = new;
        }

        // A held scrollbar thumb tracks the pointer; release ends both drags.
        if let (Some((id, horizontal, grab)), Some(p)) = (self.drag_bar.clone(), ix.pointer) {
            if let Some(bar) = ix.bars.iter().find(|b| b.id == id && b.horizontal == horizontal) {
                let v = bar_drag_offset(bar, p, grab);
                let off = self.scroll.entry(id).or_default();
                let new = if horizontal { egui::vec2(v, off.y) } else { egui::vec2(off.x, v) };
                scrolled |= new != *off;
                *off = new;
            }
        }
        if !ix.down {
            self.drag_bar = None;
            self.drag_size = None;
        }

        Applied { dispatched, blurred, dragging, scrolled }
    }

    /// Render one frame into a `Ui`, handling input and returning the CRDT snapshot if
    /// state changed (so the caller can persist it to vault). Consumes the available rect.
    /// Pointer positions stay in screen coordinates — layout and hit-testing both run in
    /// screen space (host-rect origin).
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        let rect = ui.available_rect_before_wrap();
        let input = ui.ctx().input(|i| i.raw.clone());
        let click = click_pos(&input);
        let edits = collect_edits(&input);
        let submit = wants_submit(&input);
        // Arrow keys are app-level navigation only when no editor is focused (otherwise they move
        // the caret) — neither an engine `ui.editor` (self.focus) nor an embedded `ui.doc` (which
        // takes egui's own keyboard focus). Resolved against the frame-start focus.
        let doc_focused = ui.ctx().memory(|m| m.focused().is_some());
        let nav = (self.focus.is_none() && !doc_focused).then(|| nav_key(&input)).flatten();

        self.hot_reload();
        let prev_focus = self.focus.clone();
        let (edited, submitted) = self.apply_edits(&edits, submit);
        // Window the table to the host height; the body's retained y offset drives which rows
        // build. Cloned up front (before the `&mut self` view resolve) and reused for layout.
        let scroll = self.scroll.clone();

        // Rebuild the scene only when something that affects the layout changed. egui repaints on
        // every pointer-move (for hover), but if the doc version, host rect, scroll, and focus are
        // unchanged — and no edit or live drag is in flight — last frame's layout is identical, so
        // we skip view() + Taffy entirely. Hover/caret are applied below at paint time, so they
        // still track the pointer on a reused scene.
        let key = SceneKey {
            frontiers: self.doc.oplog_frontiers(),
            rect,
            scroll: scroll.clone(),
            focus: self.focus.clone(),
            data_version: self.data.as_ref().map_or(0, |d| d.version()),
        };
        let live = edited || submitted || self.drag_bar.is_some() || self.drag_size.is_some();
        let reuse = !live && self.scene.as_ref().is_some_and(|s| s.key == key);
        if !reuse {
            let root = self.resolve_root(rect.height(), &scroll);
            let placed = layout::layout(ui.ctx(), rect, &root, &scroll);
            self.scene = Some(SceneCache { key, placed });
            self.scene_rebuilds += 1;
        }

        let focus_id = self.focus.clone();
        let focus_field = focus_id.as_ref().and_then(|id| self.fields.get(id)).copied();
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let pointer = paint::Pointer { hover, pressed: ui.input(|i| i.pointer.primary_down()) };
        let focus = focus_id.as_deref().zip(focus_field.as_ref());

        // Allocate the full rect so egui knows we used it, then paint + sense the (possibly reused)
        // scene. The `placed` borrow ends with `sense`, before `apply` takes `&mut self`.
        let (_, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
        let placed = &self.scene.as_ref().expect("scene built this frame").placed;
        paint::paint(ui, placed, &pointer, focus);
        let ix =
            sense(ui, placed, &scroll, click, hover, focus_id.as_deref(), &self.drag_bar, &self.drag_size);
        // Embedded block docs render in their own child Ui *after* the painted scene (so their
        // content lands on top of the node's box). Collect (id, rect) before taking `&mut self`.
        let doc_nodes: Vec<(String, egui::Rect)> =
            placed.iter().filter_map(|p| p.doc.clone().map(|id| (id, p.rect))).collect();
        // Charts paint in their own child Ui (egui_plot needs `&mut Ui`), after the scene, like docs.
        let chart_nodes: Vec<(node::ChartSpec, egui::Rect)> =
            placed.iter().filter_map(|p| p.chart.clone().map(|c| (c, p.rect))).collect();
        let acted = self.apply(prev_focus, ix);
        // App-level arrow navigation: dispatch the `on_key` handler (mutates the doc, e.g. the
        // slide index), counting as a state change so we persist + the next frame rebuilds.
        let key_acted = match nav {
            Some(k) => self.dispatch_nav_key(k),
            None => false,
        };
        // Drive each embedded block editor for the frame; a mutation persists like any edit (and
        // bumps the doc's frontiers, so next frame's SceneKey differs and the scene rebuilds).
        let mut doc_edited = false;
        for (id, doc_rect) in doc_nodes {
            doc_edited |= self.render_doc(ui, &id, doc_rect);
        }
        for (i, (spec, rect)) in chart_nodes.into_iter().enumerate() {
            paint::paint_chart(ui, i, &spec, rect);
        }

        // Return a CRDT snapshot if the state changed so the caller can persist it.
        if acted.dispatched || edited || submitted || acted.blurred || key_acted || doc_edited {
            self.doc.export(ExportMode::Snapshot).ok()
        } else {
            None
        }
    }

    /// Render + edit one embedded `ui.doc` into `rect` for this frame, returning whether it
    /// mutated. The editor and its `Doc` (a tree inside the app's shared CRDT) are created on first
    /// sight and retained by `id`, so caret/scroll and undo history survive across frames.
    fn render_doc(&mut self, ui: &mut egui::Ui, id: &str, rect: egui::Rect) -> bool {
        let shared = (*self.doc).clone(); // reference clone — shares the app's underlying doc
        let cell = self.docs.entry(id.to_string()).or_insert_with(|| DocCell {
            editor: doc_editor::DocEditor::new(),
            doc: doc_editor::Doc::on_tree(shared, &format!("uidoc:{id}")),
        });
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        // doc_editor fills its own page background over `ui.clip_rect()` and scrolls within it, so
        // confine both to the node's box (else the dark page bleeds across the whole slide).
        child.set_clip_rect(rect);
        let changed = cell.editor.show(&mut child, &cell.doc, false);
        if changed {
            cell.doc.commit();
        }
        changed
    }

    /// Build an engine app from a Lua file loaded at run time (the uploadable-app path),
    /// hot-reloaded on change; a missing/broken file renders inline instead of failing the caller.
    ///
    /// The CRDT is loaded from / saved to a snapshot beside the source, *before* the script runs
    /// so the script's seeds see existing data instead of duplicating it.
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data_path = snapshot_path(&path);

        let doc = Rc::new(LoroDoc::new());
        if let Ok(bytes) = std::fs::read(&data_path) {
            let _ = doc.import(&bytes); // a corrupt/old snapshot just starts empty
        }

        let mut source = Source::File { path, loaded: None };
        let script = match source.read() {
            Ok(text) => script::Script::load(&text, doc.clone()),
            Err(err) => script::Script::failed(err),
        };

        let app = EngineApp {
            ctx: engine_ctx(),
            view: ViewSource::Script { source, script },
            doc,
            data_path: Some(data_path),
            fields: HashMap::new(),
            focus: None,
            scroll: HashMap::new(),
            drag_bar: None,
            drag_size: None,
            focus_dirty: false,
            scene: None,
            scene_rebuilds: 0,
            docs: HashMap::new(),
            data: None,
        };
        app.save(); // persist any first-run seed the setup wrote
        app
    }

    /// Dispatch an app-level arrow key ("left"/"right") to the script's `on_key` handler. Returns
    /// whether a handler ran (the key was consumed and state may have changed).
    fn dispatch_nav_key(&mut self, key: &str) -> bool {
        if let ViewSource::Script { script, .. } = &self.view {
            match script.dispatch_key(key) {
                Ok(acted) => return acted,
                Err(err) => eprintln!("on_key handler error: {err}"),
            }
        }
        false
    }

    /// Persist the doc to its snapshot file, if this app has one. Best-effort.
    fn save(&self) {
        if let Some(path) = &self.data_path {
            if let Ok(bytes) = self.doc.export(ExportMode::Snapshot) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }

    /// The built-in demo as an in-memory script — used by tests and as a guaranteed fallback.
    pub fn demo_script() -> Self {
        EngineApp::script(DEMO_LUA)
    }

    /// The app's live CRDT. A peer or MCP writes *this* doc; mutations show on the next frame.
    /// Merge an external runtime-CRDT write (a peer or MCP) into the live doc — the same CRDT
    /// op the UI makes, so the next frame re-reads the merged state.
    pub fn import_state(&self, snapshot: &[u8]) -> Result<(), String> {
        self.doc.import(snapshot).map(|_| ()).map_err(|e| e.to_string())
    }

    /// The runtime CRDT as a snapshot (for persisting a merged union).
    pub fn export_state(&self) -> Option<Vec<u8>> {
        self.doc.export(ExportMode::Snapshot).ok()
    }

    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// Draw one frame: resolve the root tree (hot-reloading and running the script if any), lay
    /// it out, paint it, and tessellate to a [`Frame`]. `input` is in the app's own (0,0)-based
    /// coordinates. Any failure (file read or script) renders an inline error card, never a crash.
    pub fn frame(&mut self, input: egui::RawInput, pixels_per_point: f32) -> Frame {
        self.ctx.set_pixels_per_point(pixels_per_point);

        // Hot-reload first, so this frame's keyboard edits and the view both run against the
        // new script.
        let poll = self.hot_reload();

        // Captured before `input` is moved into `run_ui`.
        let click = click_pos(&input);
        let edits = collect_edits(&input);
        let submit = wants_submit(&input);

        let prev_focus = self.focus.clone();
        let (edited, submitted) = self.apply_edits(&edits, submit);
        // Headless render: no viewport, so every row builds (windowing is a live-view optimization).
        let root = self.resolve_root(f32::INFINITY, &HashMap::new());

        // Snapshot the focused field and scroll offsets for the closure below.
        let focus_id = self.focus.clone();
        let focus_field = focus_id.as_ref().and_then(|id| self.fields.get(id)).copied();
        let scroll = self.scroll.clone();

        // Lay out, paint, and sense the pointer against the geometry. `run_ui` borrows the
        // engine's ctx, so the sensed interactions apply after the closure returns.
        let mut ix = None;
        let drag_bar = self.drag_bar.clone();
        let drag_size = self.drag_size.clone();
        let output = self.ctx.run_ui(input, |ui| {
            let placed = layout::layout(ui.ctx(), ui.ctx().content_rect(), &root, &scroll);
            let hover = ui.input(|i| i.pointer.hover_pos());
            let pointer = paint::Pointer { hover, pressed: ui.input(|i| i.pointer.primary_down()) };
            let focus = focus_id.as_deref().zip(focus_field.as_ref());
            paint::paint(ui, &placed, &pointer, focus);
            // Charts paint in their own child Ui after the scene (so screenshots/PDF include them).
            for (i, p) in placed.iter().enumerate() {
                if let Some(spec) = &p.chart {
                    paint::paint_chart(ui, i, spec, p.rect);
                }
            }
            ix = Some(sense(ui, &placed, &scroll, click, hover, focus_id.as_deref(), &drag_bar, &drag_size));
        });
        let acted = ix.map(|ix| self.apply(prev_focus, ix)).unwrap_or_default();

        // A handler, a keystroke, or a submit may have mutated the CRDT — persist it.
        if acted.dispatched || edited || submitted || acted.blurred {
            self.save();
        }

        // Idle for free by default; repaint now if anything happened this frame; otherwise poll
        // for hot-reload edits if a file backs the app.
        let mut repaint_after = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        if acted.dispatched
            || edited
            || submitted
            || acted.blurred
            || acted.dragging
            || acted.scrolled
            || click.is_some()
            || !edits.is_empty()
            || self.drag_bar.is_some()
            || self.drag_size.is_some()
        {
            repaint_after = Duration::ZERO;
        } else if let Some(interval) = poll {
            repaint_after = repaint_after.min(interval);
        }

        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        Frame {
            primitives,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
            repaint_after,
        }
    }
}

/// The position of the latest primary-button *press* this frame, if any (press, not release —
/// fine for buttons for now).
fn click_pos(input: &egui::RawInput) -> Option<egui::Pos2> {
    input.events.iter().rev().find_map(|e| match e {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => {
            Some(*pos)
        }
        _ => None,
    })
}

/// Whether this frame's input asks to submit the focused editor (the Enter key). egui sends
/// Enter as a `Key`, never `Text`, so it never inserts a newline in a single-line field.
fn wants_submit(input: &egui::RawInput) -> bool {
    input
        .events
        .iter()
        .any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, .. }))
}

/// An app-level navigation key this frame ("left"/"right"), if any — used only when no editor is
/// focused, so it never steals arrow keys from caret movement.
fn nav_key(input: &egui::RawInput) -> Option<&'static str> {
    input.events.iter().find_map(|e| match e {
        egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. } => Some("left"),
        egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. } => Some("right"),
        _ => None,
    })
}

/// One keyboard action routed to the focused editor (single-line field for now).
enum Edit {
    Insert(String),
    Backspace,
    DeleteForward,
    Left { extend: bool },
    Right { extend: bool },
    Home { extend: bool },
    End { extend: bool },
    SelectAll,
    /// Toggle an inline mark (`"bold"`/`"italic"`/`"code"`) over the selection.
    ToggleMark(&'static str),
}

/// Pull editor-relevant key/text events out of a frame's raw input, in order. egui sends typed
/// characters as `Text` (shortcut combos don't), and editing/navigation as `Key`.
fn collect_edits(input: &egui::RawInput) -> Vec<Edit> {
    use egui::{Event, Key};
    let mut edits = Vec::new();
    for e in &input.events {
        match e {
            Event::Text(t) | Event::Paste(t) => edits.push(Edit::Insert(t.clone())),
            Event::Key { key, pressed: true, modifiers, .. } => match key {
                Key::Backspace => edits.push(Edit::Backspace),
                Key::Delete => edits.push(Edit::DeleteForward),
                Key::ArrowLeft => edits.push(Edit::Left { extend: modifiers.shift }),
                Key::ArrowRight => edits.push(Edit::Right { extend: modifiers.shift }),
                Key::Home => edits.push(Edit::Home { extend: modifiers.shift }),
                Key::End => edits.push(Edit::End { extend: modifiers.shift }),
                Key::A if modifiers.command => edits.push(Edit::SelectAll),
                // Mark shortcuts: egui sends a `Key` (never `Text`) for a command combo, so these
                // never insert their letter. A toggle over no selection is a harmless no-op.
                Key::B if modifiers.command => edits.push(Edit::ToggleMark("bold")),
                Key::I if modifiers.command => edits.push(Edit::ToggleMark("italic")),
                Key::E if modifiers.command => edits.push(Edit::ToggleMark("code")),
                _ => {}
            },
            _ => {}
        }
    }
    edits
}

/// Apply one [`Edit`] to a field + its buffer. Movement leaves the buffer unchanged (so it never
/// triggers a save); insert/backspace/delete go through the buffer.
fn apply_edit(field: &mut TextField, buf: &mut dyn text_edit::TextBuffer, edit: &Edit) {
    match edit {
        Edit::Insert(s) => {
            field.insert(buf, s);
        }
        Edit::Backspace => {
            field.backspace(buf);
        }
        Edit::DeleteForward => {
            field.delete_forward(buf);
        }
        Edit::Left { extend } => field.move_left(*extend),
        Edit::Right { extend } => field.move_right(buf, *extend),
        Edit::Home { extend } => field.home(*extend),
        Edit::End { extend } => field.end(buf, *extend),
        Edit::SelectAll => field.select_all(buf),
        Edit::ToggleMark(key) => {
            field.toggle_mark(buf, key);
        }
    }
}

/// One frame's pointer input resolved against the laid-out boxes — computed by [`sense`] inside
/// a `Ui`, applied to engine state by [`EngineApp::apply`] (split so `frame` can apply outside
/// its `run_ui` borrow).
struct Interactions {
    bars: Vec<ScrollBar>,
    /// A scrollbar press this frame: (region id, horizontal, grab offset in the thumb).
    bar_press: Option<(String, bool, f32)>,
    /// A resize-band press this frame: (target, size at press, pointer coord at press).
    size_press: Option<(node::Resize, f32, f32)>,
    /// A click on an editor: (id, caret char index, shift-extend).
    editor_click: Option<(String, usize, bool)>,
    /// A click on an `on_click` handler.
    hit: Option<u32>,
    /// The click landed on nothing interactive — blurs the focused field.
    miss: bool,
    /// A held drag over the focused editor: (id, selection head char index).
    drag_to: Option<(String, usize)>,
    /// Wheel deltas routed per scroll region: (id, max offset, delta).
    wheel: Vec<(String, egui::Vec2, egui::Vec2)>,
    down: bool,
    pointer: Option<egui::Pos2>,
}

/// What [`EngineApp::apply`] did, for the persistence and repaint decisions upstream.
#[derive(Default)]
struct Applied {
    dispatched: bool,
    blurred: bool,
    dragging: bool,
    scrolled: bool,
}

/// Resolve this frame's pointer against the laid-out boxes, painting the interaction chrome
/// (scrollbars, resize feedback) while a `Ui` is at hand. Reads engine state, never writes it.
fn sense(
    ui: &egui::Ui,
    placed: &[layout::Placed],
    offsets: &HashMap<String, egui::Vec2>,
    click: Option<egui::Pos2>,
    hover: Option<egui::Pos2>,
    focus_id: Option<&str>,
    drag_bar: &Option<(String, bool, f32)>,
    drag_size: &Option<(node::Resize, f32, f32)>,
) -> Interactions {
    let down = ui.input(|i| i.pointer.primary_down());
    // Drag tracking uses the unclamped pointer so it survives overshoot.
    let pointer = ui.input(|i| i.pointer.latest_pos());

    // Scrollbars: a press on a thumb (or its track) starts a drag and never reaches the app;
    // a track press centres the thumb on the pointer. Painted last so they sit on top.
    let bars = scroll_bars(placed, offsets);
    let mut bar_press = None;
    if let Some(p) = click {
        if let Some(bar) = bars.iter().find(|b| b.thumb.expand(2.0).contains(p) || b.track.contains(p)) {
            let (thumb_min, half) = if bar.horizontal {
                (bar.thumb.left(), bar.thumb.width() / 2.0)
            } else {
                (bar.thumb.top(), bar.thumb.height() / 2.0)
            };
            let on_thumb = bar.thumb.expand(2.0).contains(p);
            let grab = if on_thumb { (if bar.horizontal { p.x } else { p.y }) - thumb_min } else { half };
            bar_press = Some((bar.id.clone(), bar.horizontal, grab));
        }
    }
    paint_bars(ui, &bars, hover, if bar_press.is_some() { &bar_press } else { drag_bar });

    // Table resizing: a press in a grab band starts a drag and never reaches the app (it must
    // not focus the cell underneath); the cursor flips over the band and stays while dragging.
    let mut size_press = None;
    if let Some(p) = click.filter(|_| bar_press.is_none()) {
        if let Some((target, size)) = size_hit(placed, p) {
            let origin = if matches!(target, node::Resize::Col { .. }) { p.x } else { p.y };
            size_press = Some((target, size, origin));
        }
    }
    let band = drag_size
        .as_ref()
        .or(size_press.as_ref())
        .map(|(t, _, _)| t.clone())
        .or_else(|| hover.and_then(|h| size_hit(placed, h)).map(|(t, _)| t));
    if let Some(t) = band {
        ui.ctx().set_cursor_icon(match t {
            node::Resize::Col { .. } => egui::CursorIcon::ResizeColumn,
            node::Resize::Row { .. } => egui::CursorIcon::ResizeRow,
        });
        paint_resize_band(ui, placed, &t);
    }

    // Hit-test the click: one topmost-wins pass over editors and handlers together, so a popup
    // (painted last) shades the editor cells beneath it.
    let mut editor_click = None;
    let mut hit = None;
    let mut miss = false;
    if let Some(p) = click.filter(|_| bar_press.is_none() && size_press.is_none()) {
        let extend = ui.input(|i| i.modifiers.shift);
        match placed
            .iter()
            .rev()
            .find(|n| (n.editor.is_some() || n.on_click.is_some()) && n.rect.contains(p))
        {
            Some(node) if node.editor.is_some() => {
                // Click-to-place: the char nearest the click.
                let idx =
                    node.text.as_ref().map_or(0, |(origin, galley)| text_edit::char_at(galley, p - *origin));
                editor_click = Some((node.editor.clone().unwrap(), idx, extend));
            }
            Some(node) => hit = node.on_click,
            None => miss = true,
        }
    }

    // Drag-to-select: while held and an editor is focused, extend its selection head to the
    // char under the pointer (even past the box, clamped by the galley). The press already set
    // the anchor; this grows the range.
    let mut drag_to = None;
    if drag_bar.is_none() && bar_press.is_none() {
        if let (Some(fid), Some(h), true) = (focus_id, hover, down) {
            if let Some(node) = placed.iter().find(|n| n.editor.as_deref() == Some(fid)) {
                if let Some((origin, galley)) = &node.text {
                    drag_to = Some((fid.to_string(), text_edit::char_at(galley, h - *origin)));
                }
            }
        }
    }

    // Mouse wheel scrolls the scroll regions under the pointer (routed per axis).
    let delta = ui.input(|i| i.smooth_scroll_delta);
    let wheel = hover.map(|h| wheel_targets(placed, h, delta)).unwrap_or_default();

    Interactions { bars, bar_press, size_press, editor_click, hit, miss, drag_to, wheel, down, pointer }
}

/// The table-resize handle under `p`, if any: a marked header cell's right edge (column width)
/// or a marked data row's bottom edge (row height), each a ±4 px grab band. Returns the target
/// plus the box's current size on the drag axis.
fn size_hit(placed: &[layout::Placed], p: egui::Pos2) -> Option<(node::Resize, f32)> {
    placed.iter().rev().find_map(|n| {
        let target = n.resize.clone()?;
        if !n.clip.contains(p) {
            return None;
        }
        let (zone, size) = match &target {
            node::Resize::Col { .. } => (
                egui::Rect::from_x_y_ranges(n.rect.right() - 4.0..=n.rect.right() + 4.0, n.rect.y_range()),
                n.rect.width(),
            ),
            node::Resize::Row { .. } => (
                egui::Rect::from_x_y_ranges(n.rect.x_range(), n.rect.bottom() - 4.0..=n.rect.bottom() + 4.0),
                n.rect.height(),
            ),
        };
        zone.contains(p).then_some((target, size))
    })
}

/// Visible feedback for a hovered/dragged resize handle (the grab bands are invisible chrome
/// otherwise): a line along the column boundary down the whole table, or under the row.
fn paint_resize_band(ui: &egui::Ui, placed: &[layout::Placed], target: &node::Resize) {
    let Some(n) = placed.iter().find(|n| n.resize.as_ref() == Some(target)) else { return };
    let stroke = egui::Stroke::new(1.0, n.base.color.gamma_multiply(0.6));
    let painter = ui.painter().with_clip_rect(n.clip);
    match target {
        node::Resize::Col { table, .. } => {
            let bottom = placed
                .iter()
                .filter(|p| matches!(&p.resize, Some(node::Resize::Row { table: t, .. }) if t == table))
                .last()
                .map_or(n.rect.bottom(), |p| p.rect.bottom());
            let x = n.rect.right();
            painter.line_segment([egui::pos2(x, n.rect.top()), egui::pos2(x, bottom)], stroke);
        }
        node::Resize::Row { .. } => {
            let y = n.rect.bottom();
            painter.line_segment([egui::pos2(n.rect.left(), y), egui::pos2(n.rect.right(), y)], stroke);
        }
    }
}

/// Route a wheel delta to the scroll regions under the pointer, per axis: each axis goes to the
/// innermost region that can actually scroll that way — so a wide table consumes the x while the
/// page behind it keeps the y.
fn wheel_targets(
    placed: &[layout::Placed],
    p: egui::Pos2,
    delta: egui::Vec2,
) -> Vec<(String, egui::Vec2, egui::Vec2)> {
    let mut out: Vec<(String, egui::Vec2, egui::Vec2)> = Vec::new();
    let mut route = |d: egui::Vec2, can: fn(&egui::Vec2) -> bool| {
        if d == egui::Vec2::ZERO {
            return;
        }
        let hit = placed
            .iter()
            .rev()
            .filter(|n| n.rect.contains(p))
            .filter_map(|n| n.scroll.clone())
            .find(|(_, max)| can(max));
        if let Some((id, max)) = hit {
            match out.iter_mut().find(|(eid, _, _)| *eid == id) {
                Some(e) => e.2 += d,
                None => out.push((id, max, d)),
            }
        }
    };
    route(egui::vec2(delta.x, 0.0), |m| m.x > 0.0);
    route(egui::vec2(0.0, delta.y), |m| m.y > 0.0);
    out
}

/// One visible scrollbar: thumb-drag geometry for a scroll region's overflowing axis.
struct ScrollBar {
    id: String,
    horizontal: bool,
    track: egui::Rect,
    thumb: egui::Rect,
    clip: egui::Rect,
    max: f32,
}

const BAR_W: f32 = 6.0;
const BAR_PAD: f32 = 2.0;
const THUMB_MIN: f32 = 24.0;

/// Scrollbar geometry for every placed region that overflows: a y bar on the right edge, an
/// x bar on the bottom edge, thumb sized by the visible fraction and placed by the offset.
fn scroll_bars(placed: &[layout::Placed], offsets: &HashMap<String, egui::Vec2>) -> Vec<ScrollBar> {
    let mut out = Vec::new();
    for n in placed {
        let Some((id, max)) = &n.scroll else { continue };
        let off = offsets.get(id).copied().unwrap_or_default();
        let r = n.rect;
        if max.y > 0.0 {
            let track = egui::Rect::from_min_max(
                egui::pos2(r.right() - BAR_W - BAR_PAD, r.top() + BAR_PAD),
                egui::pos2(r.right() - BAR_PAD, r.bottom() - BAR_PAD),
            );
            let len = (track.height() * r.height() / (r.height() + max.y))
                .clamp(THUMB_MIN.min(track.height()), track.height());
            let top = track.top() + (track.height() - len) * (off.y / max.y).clamp(0.0, 1.0);
            out.push(ScrollBar {
                id: id.clone(),
                horizontal: false,
                track,
                thumb: egui::Rect::from_min_size(egui::pos2(track.left(), top), egui::vec2(BAR_W, len)),
                clip: n.clip,
                max: max.y,
            });
        }
        if max.x > 0.0 {
            let track = egui::Rect::from_min_max(
                egui::pos2(r.left() + BAR_PAD, r.bottom() - BAR_W - BAR_PAD),
                egui::pos2(r.right() - BAR_PAD, r.bottom() - BAR_PAD),
            );
            let len = (track.width() * r.width() / (r.width() + max.x))
                .clamp(THUMB_MIN.min(track.width()), track.width());
            let left = track.left() + (track.width() - len) * (off.x / max.x).clamp(0.0, 1.0);
            out.push(ScrollBar {
                id: id.clone(),
                horizontal: true,
                track,
                thumb: egui::Rect::from_min_size(egui::pos2(left, track.top()), egui::vec2(len, BAR_W)),
                clip: n.clip,
                max: max.x,
            });
        }
    }
    out
}

/// The offset a thumb drag asks for: map the pointer (minus the grab point) across the track.
fn bar_drag_offset(bar: &ScrollBar, p: egui::Pos2, grab: f32) -> f32 {
    let (track_min, track_len, thumb_len, pos) = if bar.horizontal {
        (bar.track.left(), bar.track.width(), bar.thumb.width(), p.x)
    } else {
        (bar.track.top(), bar.track.height(), bar.thumb.height(), p.y)
    };
    let span = (track_len - thumb_len).max(1.0);
    ((pos - grab - track_min) / span * bar.max).clamp(0.0, bar.max)
}

/// Paint the thumbs (the track stays invisible): the always-on affordance that a region scrolls.
fn paint_bars(ui: &egui::Ui, bars: &[ScrollBar], hover: Option<egui::Pos2>, drag: &Option<(String, bool, f32)>) {
    for bar in bars {
        let active = drag.as_ref().is_some_and(|(id, h, _)| *id == bar.id && *h == bar.horizontal)
            || hover.is_some_and(|p| bar.thumb.expand(2.0).contains(p));
        let color = if active {
            egui::Color32::from_white_alpha(90)
        } else {
            egui::Color32::from_white_alpha(36)
        };
        ui.painter().with_clip_rect(bar.clip).rect_filled(bar.thumb, BAR_W / 2.0, color);
    }
}

/// Wrap a non-page app's root in a host-sized scroll region (reserved id, so an app's own
/// `scroll = true` region keeps its "" key). Web-page semantics: a natural-height root overflows
/// and scrolls; an explicit `height = "100%"` root fills the host and never does.
fn scroll_root(inner: Node) -> Node {
    let mut wrap = Node::col().width(Val::Pct(100.0)).height(Val::Pct(100.0)).children(vec![inner]);
    wrap.scroll = Some(crate::node::ScrollSpec::y("__root"));
    wrap
}

/// Render a script error as a full-cell card, so a broken app shows *why* instead of nothing.
fn error_card(message: &str) -> Node {
    const ERR_BG: Color32 = Color32::from_rgb(0x2a, 0x15, 0x17);
    const ERR_FG: Color32 = Color32::from_rgb(0xf4, 0x70, 0x68);
    Node::col()
        .width(Val::Pct(100.0))
        .height(Val::Pct(100.0))
        .padding(20.0)
        .gap(8.0)
        .bg(ERR_BG)
        .children(vec![
            Node::text("app error").font(15.0).color(ERR_FG),
            Node::text(message).font(13.0).color(Color32::from_gray(0xcc)).width(Val::Pct(100.0)),
        ])
}

// --- Demo tree: a hand-built tree exercising the whole spine ------------------

const BG: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
const CARD: Color32 = Color32::from_rgb(0x1e, 0x22, 0x28);
const FG: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xea);
const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xab);
const ACCENT: Color32 = Color32::from_rgb(0x4c, 0x8b, 0xf5);

fn demo_tree() -> Node {
    Node::col()
        .width(Val::Pct(100.0))
        .height(Val::Pct(100.0))
        .padding(28.0)
        .gap(16.0)
        .bg(BG)
        .children(vec![
            Node::text("app_engine").font(24.0).color(FG),
            Node::text("A homegrown declarative UI engine — Taffy layout, egui paint. Lua + CRDT come next.")
                .font(15.0)
                .color(MUTED)
                .width(Val::Px(360.0)),
            card(),
        ])
}

fn card() -> Node {
    Node::col()
        .width(Val::Px(360.0))
        .padding(20.0)
        .gap(12.0)
        .bg(CARD)
        .radius(12.0)
        .children(vec![
            Node::text("Note").font(18.0).color(FG),
            Node::text("Tag-able notes are the first real app on this engine. These pills are placeholders:")
                .font(14.0)
                .color(MUTED)
                .width(Val::Px(320.0)),
            Node::row().gap(8.0).children(vec![tag("crdt"), tag("lua"), tag("taffy")]),
        ])
}

fn tag(label: &str) -> Node {
    Node::text(label).font(13.0).color(Color32::WHITE).padding(7.0).bg(ACCENT).radius(7.0)
}

/// The Lua source for [`EngineApp::demo_script`] — the demo card as a script. `r##"…"##` so the
/// `"#hex"` colours don't close the raw string early.
const DEMO_LUA: &str = r##"
local S = {
  page  = { padding = 28, gap = 16, background = "#14161a", width = "100%", height = "100%" },
  title = { font = 24, color = "#e6e6ea" },
  body  = { font = 15, color = "#9aa0ab", width = 360 },
  card  = { padding = 20, gap = 12, background = "#1e2228", corner = 12, width = 360 },
  note  = { font = 18, color = "#e6e6ea" },
  desc  = { font = 14, color = "#9aa0ab", width = 320 },
  pill  = { padding = 7, font = 13, color = "white", background = "#4c8bf5", corner = 7 },
}

return function()
  return ui.col{ style = S.page,
    ui.text{ "app_engine — via Lua", style = S.title },
    ui.text{ "This whole tree came from a Lua script: ui.col / ui.row / ui.text, walked into nodes.", style = S.body },
    ui.col{ style = S.card,
      ui.text{ "Note", style = S.note },
      ui.text{ "Tag-able notes are the first real app. These pills came from Lua:", style = S.desc },
      ui.row{ style = { gap = 8 },
        ui.text{ "crdt",  style = S.pill },
        ui.text{ "lua",   style = S.pill },
        ui.text{ "taffy", style = S.pill },
      },
    },
  }
end
"##;

#[cfg(test)]
mod tests;
