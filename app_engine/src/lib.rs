//! Renders uploadable apps as a homegrown declarative UI engine, composited natively into the
//! egui shell: a Lua script returns a [`Node`] tree, laid out with Taffy and painted with egui
//! into a [`Frame`] of native meshes (the compositor adds no GPU glue beyond egui's own).
//!
//! Text shapes via egui's fonts for now (no complex scripts); that swaps to parley/cosmic-text
//! behind the `layout::shape` seam when it matters.

mod layout;
mod node;
mod paint;
mod script;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use egui::Color32;
use loro::{ExportMode, LoroDoc};
use text_edit::TextField;

pub use node::{Direction, Node, Style, Val};

/// One frame's drawing from an app: GPU-ready triangles plus their texture uploads and a
/// repaint signal. Field-for-field the host-side `app_host::Surface`, kept separate so the
/// engine doesn't depend on the host (the dependency runs the other way).
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
    ctx
}

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
    /// Retained vertical scroll position (points) per scroll region, keyed by `ui.col{ scroll }`.
    scroll: HashMap<String, f32>,
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
        }
    }

    /// Render one frame into a `Ui`, handling input and returning the CRDT snapshot if
    /// state changed (so the caller can persist it to vault). Consumes the available rect.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        let rect = ui.available_rect_before_wrap();
        let input = Self::collect_input(ui, rect);
        let click = click_pos(&input);
        let edits = collect_edits(&input);
        let submit = wants_submit(&input);

        // Apply edits to the focused field before resolving the view.
        let mut edited = false;
        let mut submitted = false;
        if let Some(id) = self.focus.clone() {
            if let ViewSource::Script { script, .. } = &self.view {
                let mut field = self.fields.get(&id).copied().unwrap_or_default();
                edited = script.with_buffer(&id, |buf| {
                    field.clamp(&*buf);
                    for e in &edits {
                        apply_edit(&mut field, &mut *buf, e);
                    }
                });
                self.fields.insert(id.clone(), field);
                if submit {
                    match script.submit(&id) {
                        Ok(fired) => submitted = fired,
                        Err(err) => eprintln!("app_engine: on_submit error: {err}"),
                    }
                }
            }
        }

        // Hot-reload if this app has a file source.
        let doc = self.doc.clone();
        if let ViewSource::Script { source, script } = &mut self.view {
            if source.changed() {
                *script = match source.read() {
                    Ok(text) => script::Script::load(&text, doc.clone()),
                    Err(err) => script::Script::failed(err),
                };
            }
        }

        let root = match &mut self.view {
            ViewSource::Static(node) => node.clone(),
            ViewSource::Script { script, .. } => script.view().unwrap_or_else(|err| error_card(&err)),
        };

        let focus_id = self.focus.clone();
        let focus_field = focus_id.as_ref().and_then(|id| self.fields.get(id)).copied();
        let scroll = self.scroll.clone();

        let placed = layout::layout(ui.ctx(), &root, &scroll);
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let pointer = paint::Pointer { hover, pressed: ui.input(|i| i.pointer.primary_down()) };
        let focus = focus_id.as_deref().zip(focus_field.as_ref());

        // Allocate the full rect so egui knows we used it, then paint.
        let (_, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
        paint::paint(ui, &placed, &pointer, focus);

        // Hit-test clicks.
        let mut dispatched = false;
        let mut editor_click = None;
        let mut drag_to: Option<(String, usize)> = None;
        let mut wheel: Option<(String, f32, f32)> = None;

        if let Some(p) = click {
            let extend = ui.input(|i| i.modifiers.shift);
            if let Some(node) = placed.iter().rev().find(|n| n.editor.is_some() && n.rect.contains(p)) {
                let idx = node.text.as_ref().map_or(0, |(origin, galley)| text_edit::char_at(galley, p - *origin));
                editor_click = Some((node.editor.clone().unwrap(), idx, extend));
            } else if let Some(id) = hit_test(&placed, p) {
                if let ViewSource::Script { script, .. } = &mut self.view {
                    if let Err(err) = script.dispatch(id) {
                        eprintln!("app_engine: on_click error: {err}");
                    }
                    dispatched = true;
                }
            } else {
                self.focus = None;
            }
        }

        if let Some((id, idx, extend)) = editor_click {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, extend);
            self.fields.insert(id.clone(), field);
            self.focus = Some(id);
        }

        if let (Some(fid), Some(h), true) = (&focus_id, hover, ui.input(|i| i.pointer.primary_down())) {
            if let Some(node) = placed.iter().find(|n| n.editor.as_deref() == Some(fid.as_str())) {
                if let Some((origin, galley)) = &node.text {
                    drag_to = Some((fid.clone(), text_edit::char_at(galley, h - *origin)));
                }
            }
        }
        if let Some((id, idx)) = drag_to {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, true);
            self.fields.insert(id, field);
        }

        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy != 0.0 {
            if let Some(node) = hover.and_then(|h| placed.iter().rev().find(|n| n.scroll.is_some() && n.rect.contains(h))) {
                let (id, max) = node.scroll.clone().unwrap();
                wheel = Some((id, max, dy));
            }
        }
        if let Some((id, max, dy)) = wheel {
            let off = self.scroll.entry(id).or_default();
            *off = (*off - dy).clamp(0.0, max);
        }

        // Return a CRDT snapshot if the state changed so the caller can persist it.
        if dispatched || edited || submitted {
            self.doc.export(ExportMode::Snapshot).ok()
        } else {
            None
        }
    }

    /// Build a minimal `RawInput` for an app embedded in a `Ui` cell (pointer in app-local
    /// coords, keyboard only when focused).
    fn collect_input(ui: &egui::Ui, rect: egui::Rect) -> egui::RawInput {
        ui.ctx().input(|i| {
            let mut raw = i.raw.clone();
            // Translate pointer events into app-local coordinates (origin at rect.min).
            for event in &mut raw.events {
                match event {
                    egui::Event::PointerMoved(p) => *p -= rect.min.to_vec2(),
                    egui::Event::PointerButton { pos, .. } => *pos -= rect.min.to_vec2(),
                    _ => {}
                }
            }
            raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size()));
            raw
        })
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
        };
        app.save(); // persist any first-run seed the setup wrote
        app
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
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// Draw one frame: resolve the root tree (hot-reloading and running the script if any), lay
    /// it out, paint it, and tessellate to a [`Frame`]. `input` is in the app's own (0,0)-based
    /// coordinates. Any failure (file read or script) renders an inline error card, never a crash.
    pub fn frame(&mut self, input: egui::RawInput, pixels_per_point: f32) -> Frame {
        self.ctx.set_pixels_per_point(pixels_per_point);

        // Hot-reload the source first, so this frame's keyboard edits and the view both run
        // against the new script. The same `doc` carries over, so editing the code keeps the data.
        let doc = self.doc.clone();
        let mut poll = None;
        if let ViewSource::Script { source, script } = &mut self.view {
            if source.changed() {
                *script = match source.read() {
                    Ok(text) => script::Script::load(&text, doc.clone()),
                    Err(err) => script::Script::failed(err),
                };
            }
            poll = source.poll_interval();
        }

        // Captured before `input` is moved into `run_ui`.
        let click = click_pos(&input);
        let edits = collect_edits(&input);
        let submit = wants_submit(&input);

        // Apply the focused editor's keyboard *before* resolving the view, so the view re-reads
        // the post-edit content this same frame. Re-clamp the field against the live buffer each
        // focused frame, so the caret survives an external/remote edit (peer, MCP, or a clearing
        // `on_submit`).
        let mut edited = false;
        let mut submitted = false;
        if let Some(id) = self.focus.clone() {
            if let ViewSource::Script { script, .. } = &self.view {
                let mut field = self.fields.get(&id).copied().unwrap_or_default();
                edited = script.with_buffer(&id, |buf| {
                    field.clamp(&*buf);
                    for e in &edits {
                        apply_edit(&mut field, &mut *buf, e);
                    }
                });
                self.fields.insert(id.clone(), field);
                // Enter fires `on_submit` after the keys, so a batched `Text`+`Enter` submits the
                // just-typed value.
                if submit {
                    match script.submit(&id) {
                        Ok(fired) => submitted = fired,
                        Err(err) => eprintln!("app_engine: on_submit error: {err}"),
                    }
                }
            }
        }

        // Resolve the view tree — now reflecting the edit above.
        let root = match &mut self.view {
            ViewSource::Static(node) => node.clone(),
            ViewSource::Script { script, .. } => script.view().unwrap_or_else(|err| error_card(&err)),
        };

        // Snapshot the focused field and scroll offsets for the closure below.
        let focus_id = self.focus.clone();
        let focus_field = focus_id.as_ref().and_then(|id| self.fields.get(id)).copied();
        let scroll = self.scroll.clone();

        // Lay out and paint; while we have the geometry, resolve what the pointer hit this frame:
        // a click (focus / handler), a drag (extend selection), and a wheel (scroll).
        let mut hit = None;
        let mut editor_click = None;
        let mut drag_to: Option<(String, usize)> = None;
        let mut wheel: Option<(String, f32, f32)> = None;
        let output = self.ctx.run_ui(input, |ui| {
            let placed = layout::layout(ui.ctx(), &root, &scroll);
            let hover = ui.input(|i| i.pointer.hover_pos());
            let pointer = paint::Pointer { hover, pressed: ui.input(|i| i.pointer.primary_down()) };
            let focus = focus_id.as_deref().zip(focus_field.as_ref());
            paint::paint(ui, &placed, &pointer, focus);

            if let Some(p) = click {
                let extend = ui.input(|i| i.modifiers.shift);
                if let Some(node) = placed.iter().rev().find(|n| n.editor.is_some() && n.rect.contains(p)) {
                    // Click-to-place: the char nearest the click.
                    let idx =
                        node.text.as_ref().map_or(0, |(origin, galley)| text_edit::char_at(galley, p - *origin));
                    editor_click = Some((node.editor.clone().unwrap(), idx, extend));
                } else {
                    hit = hit_test(&placed, p);
                }
            }

            // Drag-to-select: while held and an editor is focused, extend its selection head to
            // the char under the pointer (even past the box, clamped by the galley). The press
            // already set the anchor; this grows the range.
            if let (Some(fid), Some(h), true) = (&focus_id, hover, ui.input(|i| i.pointer.primary_down())) {
                if let Some(node) = placed.iter().find(|n| n.editor.as_deref() == Some(fid.as_str())) {
                    if let Some((origin, galley)) = &node.text {
                        drag_to = Some((fid.clone(), text_edit::char_at(galley, h - *origin)));
                    }
                }
            }

            // Mouse wheel scrolls the innermost scroll region under the pointer.
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            if dy != 0.0 {
                if let Some(node) =
                    hover.and_then(|h| placed.iter().rev().find(|n| n.scroll.is_some() && n.rect.contains(h)))
                {
                    let (id, max) = node.scroll.clone().unwrap();
                    wheel = Some((id, max, dy));
                }
            }
        });

        // Apply the click: focus + caret on an editor; dispatch a handler (keeping focus); empty
        // space blurs. A handler error keeps the current view (logged).
        let mut dispatched = false;
        if let Some((id, idx, extend)) = editor_click {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, extend);
            self.fields.insert(id.clone(), field);
            self.focus = Some(id);
        } else if let (Some(id), ViewSource::Script { script, .. }) = (hit, &mut self.view) {
            if let Err(err) = script.dispatch(id) {
                eprintln!("app_engine: on_click error: {err}");
            }
            dispatched = true;
        } else if click.is_some() {
            self.focus = None;
        }

        // A drag extends the focused field's selection (the click above set its anchor).
        let dragging = drag_to.is_some();
        if let Some((id, idx)) = drag_to {
            let mut field = self.fields.get(&id).copied().unwrap_or_default();
            field.set_head(idx, true);
            self.fields.insert(id, field);
        }

        // Apply the wheel to the hovered scroll region (egui's convention: offset -= delta).
        let mut scrolled = false;
        if let Some((id, max, dy)) = wheel {
            let off = self.scroll.entry(id).or_default();
            let new = (*off - dy).clamp(0.0, max);
            scrolled = (new - *off).abs() > f32::EPSILON;
            *off = new;
        }

        // A handler, a keystroke, or a submit may have mutated the CRDT — persist it.
        if dispatched || edited || submitted {
            self.save();
        }

        // Idle for free by default; repaint now if anything happened this frame; otherwise poll
        // for hot-reload edits if a file backs the app.
        let mut repaint_after = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        if dispatched || edited || submitted || dragging || scrolled || click.is_some() || !edits.is_empty() {
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

/// The handler id of the top-most box under `p` with an `on_click`. Boxes are in paint order
/// (parents first), so reverse iteration finds the front-most and lets a handler-less child fall
/// through to a clickable parent.
fn hit_test(placed: &[layout::Placed], p: egui::Pos2) -> Option<u32> {
    placed.iter().rev().find(|n| n.on_click.is_some() && n.rect.contains(p)).and_then(|n| n.on_click)
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
