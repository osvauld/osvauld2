//! The Lua layer: load an app's source, run it, and turn the view tree it returns into engine
//! [`Node`]s. All mlua use is contained here so the render core stays Lua-free and testable
//! without a VM.
//!
//! The contract mirrors the immediate-mode model: the chunk runs once (setup) and must
//! `return function() ... end` — the view, which runs every frame and returns a tree of `ui.*`
//! tables that we walk into `Node`s. `ui` is an injected prelude (below).
//!
//! Every Lua entry point is fallible and converted to `Result<_, String>`; the engine renders the
//! message inline rather than crashing the cell.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use egui::{Color32, Vec2};
use loro::{Frontiers, LoroDoc};
use mlua::{Function, Lua, Table, Value};
use rich_text::{Marks, Run};
use text_edit::TextBuffer;

use crate::data::QueryState;
use crate::node::{ChartKind, ChartSeries, ChartSpec, Node, ScrollSpec, Style, Val};
use crate::table::{self, CellValue, ColKind, Column, TableSpec};
use crate::PageSpec;

mod crdt;
mod data;
mod style;
mod vm;

use style::parse_style;
use vm::sandboxed_vm;

/// Injected before every app: the `ui.*` builders. Each tags a table with its node kind so an
/// app's view is plain Lua data. A string/number arg is sugar for a one-element table.
const UI_PRELUDE: &str = r#"
ui = {}
local function tagged(tag, t)
  local ty = type(t)
  if ty == "string" or ty == "number" then t = { t } end
  t.tag = tag
  return t
end
function ui.col(t)    return tagged("col", t)    end
function ui.row(t)    return tagged("row", t)    end
function ui.text(t)   return tagged("text", t)   end
function ui.button(t) return tagged("button", t) end
function ui.editor(t) return tagged("editor", t) end
function ui.doc(t)    return tagged("doc", t)    end
function ui.table(t)  return tagged("table", t)  end
function ui.chart(t)  return tagged("chart", t)  end
function ui.code(t)   return tagged("code", t)   end
"#;

/// A loaded app: its Lua VM and the view function setup returned. If setup (or the prelude)
/// failed, `view` is `None` and `setup_error` holds the message every `view()` then reports.
pub struct Script {
    // Kept alive to own the VM (and the `doc` binding injected into it).
    #[allow(dead_code)]
    lua: Lua,
    /// Hook-fire count for the current Lua entry; reset to 0 before every `view()`/`dispatch()`
    /// so the instruction budget is per-entry, not cumulative.
    fires: Rc<Cell<u64>>,
    view: Option<Function>,
    setup_error: Option<String>,
    /// This frame's `on_click` closures, indexed by the id on each `Node`. Rebuilt by every
    /// `view()`; a hit-test resolves to an index that [`Script::dispatch`] calls.
    handlers: Vec<Function>,
    /// This frame's app-level key handler (`on_key` on any node, last wins), called with a key
    /// name ("left"/"right") by [`Script::dispatch_key`] when no editor is focused.
    key_handler: Option<Function>,
    /// The app's CRDT — the same `Rc` the engine owns. Resolves an editor id to its backing
    /// LoroText without round-tripping through Lua.
    doc: Rc<LoroDoc>,
    /// This frame's editable fields. Rebuilt by every `view()`; [`Script::with_buffer`] resolves
    /// an id to its buffer.
    editors: Vec<EditorBinding>,
    /// User-dragged table sizes, keyed by list name — view state (like scroll), not doc data.
    /// Overrides the declared `width`/`row_height` on every walk.
    sizes: RefCell<HashMap<String, TableSizes>>,
    /// The engine's focused editor id, set before each `view()` — a focused select cell renders
    /// its dropdown open.
    focus: RefCell<Option<String>>,
    /// The open select combobox's UI state, shared with its commit closures.
    select: Rc<SelectUi>,
    /// Set for a `.table` view: `view()` builds the grid natively from the doc's stored schema
    /// instead of running user Lua, reusing the whole dispatch/buffer/finalize/resize machinery.
    table: Option<TableMode>,
    /// Per-list cache of filtered+sorted rows, so the O(rows) read→coerce→query pass runs only when
    /// the doc version or the query changes — not every repaint. The window slices it each frame.
    row_cache: RefCell<HashMap<String, CachedRows>>,
    /// Ambient view geometry set before each `view()` (like [`Script::set_focus`]): body viewport
    /// height + scroll offsets, so the table builder can window to the visible rows.
    viewport: RefCell<Viewport>,
}

/// A table body's filtered+sorted rows, tagged with the doc version and query they were computed
/// at; reused while both are unchanged. See [`emit_table`].
struct CachedRows {
    frontiers: Frontiers,
    query: u64,
    rows: Vec<table::Row>,
}

/// Body viewport height + retained scroll offsets, fed to the row window. Defaults to an infinite
/// viewport (render everything), so any render that never sets it — headless, PDF, tests — is
/// unwindowed.
struct Viewport {
    height: f32,
    scroll: HashMap<String, Vec2>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { height: f32::INFINITY, scroll: HashMap::new() }
    }
}

/// Hash the spec parts that change the cached rows — column identity+kind (coercion), the equality
/// filter, the order. Row *data* is versioned separately by the doc's frontiers.
fn query_hash(spec: &TableSpec) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in &spec.columns {
        c.key.hash(&mut h);
        c.kind.as_str().hash(&mut h);
    }
    for (k, v) in &spec.filter {
        k.hash(&mut h);
        v.display().hash(&mut h);
    }
    if let Some((k, desc)) = &spec.order {
        k.hash(&mut h);
        desc.hash(&mut h);
    }
    h.finish()
}

/// A `.table` view's render params: the host text colour (the grid tints its chrome from it) and
/// base font size. The schema + rows come from the doc itself.
struct TableMode {
    color: Color32,
    font: f32,
}

/// A select combobox's transient state: the typed filter (scratch — never written to the CRDT)
/// and a flag the commit closures raise so the engine drops focus (closing the dropdown).
#[derive(Default)]
pub(crate) struct SelectUi {
    query: RefCell<String>,
    defocus: Cell<bool>,
}

/// One table's dragged sizes: column widths by key, row heights by stable row id.
#[derive(Default)]
pub(crate) struct TableSizes {
    cols: HashMap<String, f32>,
    rows: HashMap<String, f32>,
}

/// One editable field in the current view: app-given id, what backs it, and an optional
/// `on_submit` handler index (fired by Enter while focused; a cell also fires it on
/// blur-after-edit — its commit hook).
struct EditorBinding {
    id: String,
    target: Target,
    on_submit: Option<u32>,
}

/// What an editable field writes to: a LoroText container, one scalar cell in a row map
/// (typed — number/date cells finalize their text on commit), or a select combobox whose
/// typing edits the scratch filter query, never the CRDT.
enum Target {
    Text(String),
    Cell { list: String, row: String, key: String, kind: ColKind },
    Select,
}

impl Script {
    /// Load and run an app's source once, with `doc` bound to its CRDT. Never panics: a setup
    /// failure is captured and reported by every `view()`.
    pub fn load(source: &str, doc: Rc<LoroDoc>) -> Self {
        let (lua, fires) = match sandboxed_vm() {
            Ok(v) => v,
            Err(e) => return Script::failed(format!("vm setup failed: {e}")),
        };
        let mut view = None;
        let mut setup_error = None;

        if let Err(e) = lua.load(UI_PRELUDE).set_name("ui").exec() {
            setup_error = Some(format!("engine prelude failed: {e}"));
        } else if let Err(e) = crdt::install(&lua, doc.clone()) {
            setup_error = Some(format!("doc binding failed: {e}"));
        } else {
            // `@`-prefix marks the chunk filename-style, so errors read `app:LINE: …` rather than
            // `[string "app"]:LINE: …`.
            match lua.load(source).set_name("@app").eval::<Value>() {
                Ok(Value::Function(f)) => view = Some(f),
                Ok(other) => {
                    setup_error =
                        Some(format!("app must `return function() ... end`, got {}", other.type_name()));
                }
                Err(e) => setup_error = Some(e.to_string()),
            }
        }

        Script { lua, fires, view, setup_error, handlers: Vec::new(), key_handler: None, doc, editors: Vec::new(), sizes: RefCell::default(), focus: RefCell::new(None), select: Rc::default(), table: None, row_cache: RefCell::default(), viewport: RefCell::default() }
    }

    /// Load a multi-file app: every `*.lua` file is registered as a `require`-able module
    /// (`lib/state.lua` → `require("lib.state")`) via `package.preload`, then `main.lua` runs as
    /// the entry chunk and must `return function() ... end`. Non-`.lua` files (manifest, assets)
    /// are ignored here. Never panics: any failure is captured and reported by every `view()`.
    pub fn load_app(files: &[(String, String)], doc: Rc<LoroDoc>) -> Self {
        let (lua, fires) = match sandboxed_vm() {
            Ok(v) => v,
            Err(e) => return Script::failed(format!("vm setup failed: {e}")),
        };
        let mut view = None;
        let mut setup_error = None;

        // Run the whole setup as one fallible block so the first error wins and is reported.
        let result = (|| -> Result<Function, String> {
            lua.load(UI_PRELUDE).set_name("ui").exec().map_err(|e| format!("engine prelude failed: {e}"))?;
            crdt::install(&lua, doc.clone()).map_err(|e| format!("doc binding failed: {e}"))?;
            let entry =
                vm::install_require(&lua, files)?.ok_or_else(|| "app has no main.lua".to_string())?;
            match lua.load(&entry).set_name("@main.lua").eval::<Value>().map_err(|e| e.to_string())? {
                Value::Function(f) => Ok(f),
                other => Err(format!("main.lua must `return function() ... end`, got {}", other.type_name())),
            }
        })();

        match result {
            Ok(f) => view = Some(f),
            Err(e) => setup_error = Some(e),
        }

        Script { lua, fires, view, setup_error, handlers: Vec::new(), key_handler: None, doc, editors: Vec::new(), sizes: RefCell::default(), focus: RefCell::new(None), select: Rc::default(), table: None, row_cache: RefCell::default(), viewport: RefCell::default() }
    }

    /// A script that failed before any Lua ran (e.g. its source couldn't be read). Every `view()`
    /// reports `error`.
    pub fn failed(error: String) -> Self {
        Script {
            lua: Lua::new(),
            fires: Rc::new(Cell::new(0)),
            view: None,
            setup_error: Some(error),
            handlers: Vec::new(),
            key_handler: None,
            doc: Rc::new(LoroDoc::new()),
            editors: Vec::new(),
            sizes: RefCell::default(),
            focus: RefCell::new(None),
            select: Rc::default(),
            table: None,
            row_cache: RefCell::default(),
            viewport: RefCell::default(),
        }
    }

    /// A native `.table` view over `doc`: no user Lua, the grid is built each frame from the doc's
    /// stored schema. Carries a VM only so the grid's synthesized check/select handlers (which are
    /// engine-created Lua closures) work exactly as in `ui.table`. `color` is the host text colour.
    pub fn table(doc: Rc<LoroDoc>, color: Color32, font: f32) -> Self {
        let mut s = match sandboxed_vm() {
            Ok((lua, fires)) => Script {
                lua,
                fires,
                view: None,
                setup_error: None,
                handlers: Vec::new(),
                key_handler: None,
                doc,
                editors: Vec::new(),
                sizes: RefCell::default(),
                focus: RefCell::new(None),
                select: Rc::default(),
                table: None,
                row_cache: RefCell::default(),
                viewport: RefCell::default(),
            },
            Err(e) => Script::failed(format!("vm setup failed: {e}")),
        };
        s.table = Some(TableMode { color, font });
        s
    }

    /// Run the view for one frame and walk its tree into a [`Node`], capturing this frame's
    /// closures. Returns the setup error if the app never loaded, or the view/walk error.
    /// The app's print-page declaration, read from the script's globals: `page = { size = "A4",
    /// orientation = "landscape" }`, or explicit `width`/`height` in mm. `None` = a normal
    /// pane-filling app.
    pub fn page(&self) -> Option<PageSpec> {
        const PT_PER_MM: f32 = 72.0 / 25.4;
        let t: Table = self.lua.globals().get("page").ok()?;
        let size: Option<String> = t.get("size").ok();
        let (mut w_mm, mut h_mm) = match size.map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("a3") => (297.0, 420.0),
            Some("a5") => (148.0, 210.0),
            Some("letter") => (215.9, 279.4),
            Some("legal") => (215.9, 355.6),
            _ => (210.0, 297.0), // A4, the default
        };
        if let (Ok(w), Ok(h)) = (t.get::<f32>("width"), t.get::<f32>("height")) {
            (w_mm, h_mm) = (w, h);
        }
        if matches!(t.get::<String>("orientation").ok().as_deref(), Some("landscape")) {
            std::mem::swap(&mut w_mm, &mut h_mm);
        }
        Some(PageSpec { width: w_mm * PT_PER_MM, height: h_mm * PT_PER_MM })
    }

    pub fn view(&mut self) -> Result<Node, String> {
        if let Some(err) = &self.setup_error {
            return Err(err.clone());
        }
        if let Some(mode) = &self.table {
            return Ok(self.table_view(mode.color, mode.font));
        }
        let view = self.view.as_ref().expect("view present when setup succeeded");
        self.fires.set(0); // fresh instruction budget for this entry
        let tree: Value = view.call(()).map_err(|e| e.to_string())?;
        let doc = self.doc.clone();
        let sizes = self.sizes.borrow();
        let focus = self.focus.borrow().clone();
        let vp = self.viewport.borrow();
        let mut w = Walk {
            handlers: Vec::new(),
            key_handler: None,
            editors: Vec::new(),
            doc: &doc,
            lua: &self.lua,
            sizes: &sizes,
            focus: focus.as_deref(),
            select: &self.select,
            viewport_h: vp.height,
            scroll: &vp.scroll,
            cache: &self.row_cache,
        };
        let node = walk(tree, &mut w)?;
        self.handlers = w.handlers;
        self.key_handler = w.key_handler;
        self.editors = w.editors;
        Ok(node)
    }

    /// Build the `.table` grid from the doc's stored schema — the native counterpart of running a
    /// Lua `view`, sharing [`emit_table`] with `ui.table`. An absent schema renders an empty grid;
    /// rows live in the `"rows"` list.
    fn table_view(&mut self, color: Color32, font: f32) -> Node {
        let doc = self.doc.clone();
        let sizes = self.sizes.borrow();
        let focus = self.focus.borrow().clone();
        let vp = self.viewport.borrow();
        let mut w = Walk {
            handlers: Vec::new(),
            key_handler: None,
            editors: Vec::new(),
            doc: &doc,
            lua: &self.lua,
            sizes: &sizes,
            focus: focus.as_deref(),
            select: &self.select,
            viewport_h: vp.height,
            scroll: &vp.scroll,
            cache: &self.row_cache,
        };
        let spec = table::read_schema(&doc).unwrap_or_else(|| TableSpec {
            columns: Vec::new(),
            filter: Vec::new(),
            order: None,
            row_height: None,
            row_heights: HashMap::new(),
        });
        // Fill the host so the body scrolls internally (header pinned) and its `__tbody` offset —
        // not a page scroll — drives the row window. A natural-height table would scroll the whole
        // page instead, stranding the window at the top.
        let base = Style {
            width: Val::Pct(100.0),
            height: Val::Pct(100.0),
            color,
            font_size: font,
            ..Style::default()
        };
        let node = emit_table(&mut w, spec, Some("rows".to_string()), base, None);
        self.handlers = w.handlers;
        self.editors = w.editors;
        node
    }

    /// Install (or replace) the `data` global — the World-B host data plane — into this script's
    /// VM. Called by the engine after load/reload; harmless on a failed script. The binding only
    /// carries handles, so re-installing it on the live VM is safe.
    pub fn install_data(&self, access: crate::data::DataAccessRef) -> Result<(), String> {
        data::install(&self.lua, access).map_err(|e| e.to_string())
    }

    /// Tell the walk which editor the engine has focused (a focused select cell opens its
    /// dropdown). Call before `view()`.
    pub fn set_focus(&self, id: Option<&str>) {
        *self.focus.borrow_mut() = id.map(str::to_string);
    }

    /// Set the ambient view geometry for the next `view()` (call before it, like `set_focus`): the
    /// body viewport height and current scroll offsets, so the table builder windows to the visible
    /// rows.
    pub fn set_viewport(&self, height: f32, scroll: &HashMap<String, Vec2>) {
        let mut vp = self.viewport.borrow_mut();
        vp.height = height;
        vp.scroll.clear();
        vp.scroll.extend(scroll.iter().map(|(k, v)| (k.clone(), *v)));
    }

    /// Render every row in the next `view()` (headless / PDF / screenshot): an infinite viewport
    /// disables windowing.
    pub fn full_viewport(&self) {
        let mut vp = self.viewport.borrow_mut();
        vp.height = f32::INFINITY;
        vp.scroll.clear();
    }

    /// Whether a select commit asked the engine to drop focus (closing the dropdown); reading
    /// clears the flag.
    pub fn take_defocus(&self) -> bool {
        self.select.defocus.replace(false)
    }

    /// Record a user drag: table `table`'s column `key` is now `w` px wide (next walk applies it).
    pub(crate) fn set_col_width(&self, table: &str, key: &str, w: f32) {
        self.sizes.borrow_mut().entry(table.to_string()).or_default().cols.insert(key.to_string(), w);
    }

    /// Record a user drag: table `table`'s row `row` is now `h` px tall.
    pub(crate) fn set_row_height(&self, table: &str, row: &str, h: f32) {
        self.sizes.borrow_mut().entry(table.to_string()).or_default().rows.insert(row.to_string(), h);
    }

    /// Call the `on_click` closure with handler id `id`. A handler error is returned to surface; it
    /// does *not* replace the running view.
    pub fn dispatch(&self, id: u32) -> Result<(), String> {
        match self.handlers.get(id as usize) {
            Some(f) => {
                self.fires.set(0); // fresh instruction budget for this entry
                f.call::<()>(()).map_err(|e| e.to_string())
            }
            None => Ok(()),
        }
    }

    /// Call the app's `on_key` handler with a key name (e.g. "left"/"right"). Returns whether a
    /// handler was registered (so the engine knows the key was consumed and state may have changed).
    pub fn dispatch_key(&self, key: &str) -> Result<bool, String> {
        match &self.key_handler {
            Some(f) => {
                self.fires.set(0); // fresh instruction budget for this entry
                f.call::<()>(key).map_err(|e| e.to_string())?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Run an editing closure against editor `id`'s live buffer (a [`TextBuffer`] over its
    /// backing LoroText or table cell), committing once after. Returns whether the content
    /// changed; `false` (closure not run) if `id` isn't an editor in the current view.
    pub fn with_buffer(&self, id: &str, f: impl FnOnce(&mut dyn TextBuffer)) -> bool {
        match self.editors.iter().find(|e| e.id == id).map(|e| &e.target) {
            Some(Target::Text(name)) => {
                let mut buf = crdt::LoroTextBuffer::open(self.doc.clone(), name);
                f(&mut buf);
                buf.commit();
                buf.dirty
            }
            Some(Target::Cell { list, row, key, .. }) => {
                let mut buf = crdt::CellBuffer::open(self.doc.clone(), list, row, key);
                f(&mut buf);
                buf.commit()
            }
            Some(Target::Select) => {
                let mut buf = QueryBuffer { s: self.select.query.borrow_mut(), dirty: false };
                f(&mut buf);
                buf.dirty
            }
            None => false,
        }
    }

    /// Enter on editor `id`: a typed cell finalizes its text first (number parse / date
    /// normalize), then any `on_submit` fires — for a select cell that's the query resolver.
    /// Returns whether anything happened; a handler error is returned to surface.
    pub fn submit(&self, id: &str) -> Result<bool, String> {
        let Some(b) = self.editors.iter().find(|e| e.id == id) else { return Ok(false) };
        let mut acted = false;
        if let Target::Cell { list, row, key, kind } = &b.target {
            acted |= self.finalize_cell(list, row, key, *kind);
        }
        if let Some(h) = b.on_submit {
            self.dispatch(h)?;
            acted = true;
        }
        Ok(acted)
    }

    /// Field `id` lost focus after edits. A cell treats that as its commit — finalize, then fire
    /// `on_edit`; a select discards its filter query; a plain editor's `on_submit` stays
    /// Enter-only (its buffer is the document, not a form).
    pub fn blur(&self, id: &str) -> Result<bool, String> {
        match self.editors.iter().find(|e| e.id == id) {
            Some(EditorBinding { target: Target::Cell { list, row, key, kind }, on_submit, .. }) => {
                let mut acted = self.finalize_cell(list, row, key, *kind);
                if let Some(h) = on_submit {
                    self.dispatch(*h)?;
                    acted = true;
                }
                Ok(acted)
            }
            Some(EditorBinding { target: Target::Select, .. }) => {
                self.select.query.borrow_mut().clear();
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Convert a typed cell's committed text to its column type: a number cell's parseable text
    /// becomes a real number, a date cell's becomes canonical `YYYY-MM-DD`. Unparseable text is
    /// left as typed (visible and honest, not silently dropped). Returns whether it wrote.
    fn finalize_cell(&self, list: &str, row: &str, key: &str, kind: ColKind) -> bool {
        let text = match crdt::cell_string(&self.doc, list, row, key) {
            Some(t) => t,
            None => return false,
        };
        let trimmed = text.trim();
        match kind {
            ColKind::Number => match trimmed.parse::<f64>() {
                Ok(n) => crdt::set_cell_number(&self.doc, list, row, key, n),
                Err(_) => false,
            },
            ColKind::Date => match table::parse_date(trimmed) {
                Some(d) => {
                    let canon = table::format_date(d);
                    canon != text && crdt::set_cell_text(&self.doc, list, row, key, &canon)
                }
                None => false,
            },
            // Stored as its canonical string (exact); never as a float.
            ColKind::Decimal => match table::parse_decimal(trimmed) {
                Some(canon) => canon != text && crdt::set_cell_text(&self.doc, list, row, key, &canon),
                None => false,
            },
            _ => false,
        }
    }
}

/// [`TextBuffer`] over the select filter query (a plain string — scratch view state).
struct QueryBuffer<'a> {
    s: std::cell::RefMut<'a, String>,
    dirty: bool,
}

impl text_edit::TextBuffer for QueryBuffer<'_> {
    fn char_len(&self) -> usize {
        self.s.chars().count()
    }

    fn text(&self) -> String {
        self.s.clone()
    }

    fn insert(&mut self, at: usize, t: &str) {
        let b = crdt::byte_at(&self.s, at);
        self.s.insert_str(b, t);
        self.dirty = true;
    }

    fn delete(&mut self, at: usize, len: usize) {
        let a = crdt::byte_at(&self.s, at);
        let b = crdt::byte_at(&self.s, at + len);
        self.s.replace_range(a..b, "");
        self.dirty = true;
    }
}

/// Collectors threaded through one `view()` walk: click closures, editor bindings, the doc to
/// read content from, and the VM (tables synthesize cell-edit handlers).
struct Walk<'a> {
    handlers: Vec<Function>,
    /// App-level key handler (`on_key`), the last one the walk sees.
    key_handler: Option<Function>,
    editors: Vec<EditorBinding>,
    doc: &'a Rc<LoroDoc>,
    lua: &'a Lua,
    sizes: &'a HashMap<String, TableSizes>,
    /// The engine-focused editor id — the select cell matching it renders its dropdown.
    focus: Option<&'a str>,
    select: &'a Rc<SelectUi>,
    /// Body viewport height (for windowing); `INFINITY` ⇒ render every row.
    viewport_h: f32,
    /// Retained scroll offsets by region id — the table body's `y` offset drives its window.
    scroll: &'a HashMap<String, Vec2>,
    /// Per-list filtered+sorted row cache, sliced to the visible window each frame.
    cache: &'a RefCell<HashMap<String, CachedRows>>,
}

/// Turn one `ui.*` table (or a bare string) into a [`Node`], registering any `on_click`
/// closure and any editor binding into `w` and recording the resulting ids on the node.
fn walk(value: Value, w: &mut Walk) -> Result<Node, String> {
    match value {
        Value::String(s) => Ok(Node::text(lua_str(&s))),
        Value::Table(t) => {
            let tag = str_field(&t, "tag").unwrap_or_else(|| "col".to_string());
            let style = field(&t, "style");
            let mut node = match tag.as_str() {
                // A leaf whose array elements are styled runs (a `button` is a text leaf that
                // typically carries an `on_click`).
                "text" | "button" => {
                    let mut node = Node::runs(parse_runs(&t));
                    node.style = parse_style(style, Style::default());
                    node
                }
                // A syntax-highlighted code block: `source` highlighted by `lang` (default
                // "rust") via the same `code_highlight` leaf doc_editor uses, emitted as monospace
                // runs with per-token colours. Whitespace is preserved (galley keeps it verbatim).
                "code" => {
                    let source = str_field(&t, "source").unwrap_or_default();
                    let lang = str_field(&t, "lang").unwrap_or_else(|| "rust".to_string());
                    let mut node = Node::runs(highlight_runs(&lang, &source));
                    node.style = parse_style(style, Style::default());
                    node
                }
                // An editable field: its content is the backing LoroText, read now as styled runs
                // (so existing marks render); the id (or bound container name) keys its caret.
                "editor" => {
                    let bound = crdt::text_name(&field(&t, "value"));
                    let id = str_field(&t, "id")
                        .or_else(|| bound.clone())
                        .unwrap_or_else(|| "editor".to_string());
                    let name = bound.unwrap_or_else(|| id.clone());
                    // The backing text's styled runs, so the field renders its existing marks.
                    let content = crdt::text_runs(w.doc, name.as_str());
                    let mut node = Node::editor(id.clone(), content);
                    node.style = parse_style(style, Style::default());
                    // `on_submit` (Enter) registers like `on_click`, but fires for the *focused*
                    // field rather than on a hit-test, so its index rides on the binding.
                    let on_submit = match field(&t, "on_submit") {
                        Value::Function(f) => {
                            let idx = w.handlers.len() as u32;
                            w.handlers.push(f);
                            Some(idx)
                        }
                        _ => None,
                    };
                    w.editors.push(EditorBinding { id, target: Target::Text(name), on_submit });
                    node
                }
                // An embedded block document (`ui.doc{ id }`): a native doc_editor over a tree in
                // the app's CRDT. Only the id + box matter here; the engine finds it in the placed
                // scene by `doc` id and renders/edits it in its own child Ui.
                "doc" => {
                    let id = str_field(&t, "id").unwrap_or_else(|| "doc".to_string());
                    let mut node = Node::doc(id);
                    node.style = parse_style(style, Style::default());
                    node
                }
                // A typed grid. Two row sources: `rows = doc:list(name)` — the app's own editable
                // CRDT list (World A) — or `source = data.sql(...)` / `data.table(...)` — a
                // read-only host-computed result over imported tables (World B), whose rows never
                // enter Lua. The `source` form wins when present.
                "table" => {
                    let base = parse_style(style, Style { width: Val::Pct(100.0), ..Style::default() });
                    match data::query_ref(&field(&t, "source")) {
                        Some(qref) => {
                            let region =
                                str_field(&t, "id").or_else(|| qref.source.clone()).unwrap_or_else(|| "query".to_string());
                            emit_query_table(w, qref, &region, base)
                        }
                        None => {
                            let spec = parse_table_spec(&t)?;
                            let list = crdt::list_name(&field(&t, "rows"));
                            let on_edit = match field(&t, "on_edit") {
                                Value::Function(f) => Some(f),
                                _ => None,
                            };
                            emit_table(w, spec, list, base, on_edit)
                        }
                    }
                }
                // A chart over a host-computed result (`data = data.sql(...)`): `type` (line/bar/
                // scatter), `x` (the category-label column key), `y` (one key or a list of keys, the
                // numeric series). Data is resolved host-side into a `ChartSpec`; the engine paints
                // it with egui_plot. Rows never enter Lua.
                "chart" => {
                    let base = parse_style(
                        style,
                        Style { width: Val::Pct(100.0), height: Val::Px(240.0), ..Style::default() },
                    );
                    match data::query_ref(&field(&t, "data")) {
                        Some(qref) => emit_chart(&t, qref, base),
                        None => {
                            let mut n = Node::text("ui.chart needs data = data.sql(...)");
                            n.style = base;
                            n
                        }
                    }
                }
                "row" | "col" => {
                    let mut node = if tag == "row" { Node::row() } else { Node::col() };
                    node.style = parse_style(style, node.style);
                    // `scroll = true` (vertical, id "") / `"x"` / `"y"` / `"both"` pick the axes;
                    // any other string is a keyed vertical region (`scroll = "name"`).
                    node.scroll = match field(&t, "scroll") {
                        Value::Boolean(true) => Some(ScrollSpec::y("")),
                        Value::String(s) => Some(match lua_str(&s).as_str() {
                            "x" => ScrollSpec::x(""),
                            "y" => ScrollSpec::y(""),
                            "both" | "xy" => ScrollSpec::both(""),
                            id => ScrollSpec::y(id),
                        }),
                        _ => None,
                    };
                    let len = t.raw_len();
                    let mut children = Vec::new();
                    for i in 1..=len {
                        let child = field_i(&t, i);
                        if !matches!(child, Value::Nil) {
                            children.push(walk(child, w)?);
                        }
                    }
                    node.children = children;
                    node
                }
                other => return Err(format!("unknown ui tag '{other}'")),
            };
            // Any node may carry an `on_click`.
            if let Value::Function(f) = field(&t, "on_click") {
                node.on_click = Some(w.handlers.len() as u32);
                w.handlers.push(f);
            }
            // Any node may carry an `on_key`: an app-level key handler called with a key name
            // ("left"/"right") when no editor is focused. The last one the walk sees wins.
            if let Value::Function(f) = field(&t, "on_key") {
                w.key_handler = Some(f);
            }
            // Optional state styles overlaid on the base. Only paint props take effect (layout
            // uses the base).
            let hover = field(&t, "hover");
            if matches!(hover, Value::Table(_)) {
                node.hover = Some(parse_style(hover, node.style.clone()));
            }
            let active = field(&t, "active");
            if matches!(active, Value::Table(_)) {
                node.active = Some(parse_style(active, node.style.clone()));
            }
            Ok(node)
        }
        Value::Nil => Ok(Node::col()),
        other => Err(format!("a ui node must be a table or string, got {}", other.type_name())),
    }
}

/// The focused select cell's dropdown: legal next values (`transitions`-gated against the cell's
/// current value), filtered by the typed query; each option's click writes the value, fires
/// `on_edit`, and asks the engine to drop focus. `None` if the VM refuses a closure.
fn select_popup(
    w: &mut Walk,
    list: &str,
    row: &table::Row,
    col: &Column,
    on_edit: &Option<Function>,
) -> Option<table::SelectPopup> {
    let current =
        row.cells.get(&col.key).map(|v| v.display()).unwrap_or_default();
    let query = w.select.query.borrow().clone();
    let q = query.to_lowercase();
    let mut options = Vec::new();
    for opt in table::legal_options(col, &current) {
        if !q.is_empty() && !opt.to_lowercase().contains(&q) {
            continue;
        }
        let edit = on_edit
            .clone()
            .and_then(|f| crdt::cell_edit_fn(w.lua, f, &row.id, &col.key).ok());
        let (doc, ui) = (w.doc.clone(), w.select.clone());
        let (l, r, k, v) = (list.to_string(), row.id.clone(), col.key.clone(), opt.to_string());
        let commit = w
            .lua
            .create_function(move |_, ()| {
                if crdt::set_cell_text(&doc, &l, &r, &k, &v) {
                    if let Some(f) = &edit {
                        f.call::<()>(())?;
                    }
                }
                // Close the box: clear the filter (blur won't always run, e.g. Enter) and
                // ask the engine to drop focus.
                ui.query.borrow_mut().clear();
                ui.defocus.set(true);
                Ok(())
            })
            .ok()?;
        let idx = w.handlers.len() as u32;
        w.handlers.push(commit);
        options.push((opt.to_string(), idx));
    }
    // The guard swallows clicks on popup chrome (padding, "no match") so they can't blur
    // the field or hit what's underneath.
    let guard = w.lua.create_function(|_, ()| Ok(())).ok()?;
    let guard_idx = w.handlers.len() as u32;
    w.handlers.push(guard);
    Some(table::SelectPopup { query, options, guard: guard_idx })
}

/// Emit a grid subtree + register its live cell bindings — shared by the Lua `ui.table` widget
/// (declared `spec`) and the native `.table` view (stored `spec`). `list` names the rows
/// container; `base` carries the resolved style (the grid tints its chrome from `base.color`);
/// `on_edit` is the optional user commit hook (always `None` for a bare `.table`). User-dragged
/// sizes override the declared widths; rows are read, type-coerced (so `Decimal` cells sort
/// exactly), then filtered + sorted.
fn emit_table(
    w: &mut Walk,
    mut spec: TableSpec,
    list: Option<String>,
    base: Style,
    on_edit: Option<Function>,
) -> Node {
    if let Some(sz) = list.as_ref().and_then(|n| w.sizes.get(n)) {
        for c in &mut spec.columns {
            if let Some(&px) = sz.cols.get(&c.key) {
                c.width = Some(px);
            }
        }
        spec.row_heights.extend(sz.rows.iter().map(|(k, v)| (k.clone(), *v)));
    }
    // Default row height when none is declared: one line + padding. Windowing and the pinned row
    // height share these values, so the rendered extent matches the reserved (spacer) extent.
    let default_h = (base.font_size * 1.4 + 12.0).ceil();
    // Filtered+sorted rows come from the per-list cache (recomputed only when the doc version or the
    // query changes), then we slice the visible window; off-screen rows become spacer heights. Each
    // arm yields the visible rows, their pinned heights, the absolute start index, and the spacers.
    let (rows, heights, first, lead, trail) = match list.as_ref() {
        Some(name) => {
            let frontiers = w.doc.oplog_frontiers();
            let query = query_hash(&spec);
            {
                let mut cache = w.cache.borrow_mut();
                let stale = cache
                    .get(name)
                    .map_or(true, |c| c.frontiers != frontiers || c.query != query);
                if stale {
                    let mut rows = table::read_rows(w.doc, name);
                    table::coerce(&mut rows, &spec);
                    let rows = table::apply(&spec, rows);
                    cache.insert(name.clone(), CachedRows { frontiers, query, rows });
                }
            }
            let cache = w.cache.borrow();
            let all = &cache[name].rows;
            let offset_y = w.scroll.get(&format!("__tbody:{name}")).map_or(0.0, |v| v.y);
            // Clone just the visible window (~tens of rows) out of the cache, so the borrow drops
            // before the `bind` closure below takes `&mut w`.
            if spec.row_heights.is_empty() {
                // Uniform fast path: O(1) window, every row the same height.
                let row_h = spec.row_height.unwrap_or(default_h);
                let (range, lead, trail) =
                    table::row_window(all.len(), row_h, offset_y, w.viewport_h);
                let first = range.start;
                let vis = all[range].to_vec();
                let hs = vec![row_h; vis.len()];
                (vis, hs, first, lead, trail)
            } else {
                // Some rows were drag-resized: per-row heights + a cumulative window (O(rows)).
                let eff: Vec<f32> = all
                    .iter()
                    .map(|r| {
                        spec.row_heights.get(&r.id).copied().or(spec.row_height).unwrap_or(default_h)
                    })
                    .collect();
                let (range, lead, trail) =
                    table::row_window_variable(&eff, offset_y, w.viewport_h);
                let first = range.start;
                let vis = all[range.clone()].to_vec();
                let hs = eff[range].to_vec();
                (vis, hs, first, lead, trail)
            }
        }
        None => (Vec::new(), Vec::new(), 0, 0.0, 0.0),
    };
    // Cells are live: a check registers a synthesized flip-by-row-id handler (dispatched like any
    // on_click); a text/number/decimal/date cell registers an editor binding to its scalar field;
    // a select cell is a combobox whose dropdown lists the legal options as commit handlers.
    let mut bind = |row: &table::Row, col: &Column| -> table::CellBind {
        let Some(name) = list.clone() else { return table::CellBind::None };
        let row_id = row.id.as_str();
        let key = col.key.as_str();
        match col.kind {
            ColKind::Check if !col.locked => {
                let f = crdt::toggle_cell_fn(
                    w.lua,
                    w.doc.clone(),
                    name,
                    row_id.to_string(),
                    key.to_string(),
                );
                match f {
                    Ok(f) => {
                        let idx = w.handlers.len() as u32;
                        w.handlers.push(f);
                        table::CellBind::Click(idx)
                    }
                    Err(_) => table::CellBind::None,
                }
            }
            ColKind::Text | ColKind::Number | ColKind::Decimal | ColKind::Date if !col.locked => {
                let id = format!("__cell:{name}:{row_id}:{key}");
                let on_submit = on_edit.clone().and_then(|f| {
                    let f = crdt::cell_edit_fn(w.lua, f, row_id, key).ok()?;
                    let idx = w.handlers.len() as u32;
                    w.handlers.push(f);
                    Some(idx)
                });
                w.editors.push(EditorBinding {
                    id: id.clone(),
                    target: Target::Cell {
                        list: name,
                        row: row_id.to_string(),
                        key: key.to_string(),
                        kind: col.kind,
                    },
                    on_submit,
                });
                table::CellBind::Edit(id)
            }
            ColKind::Select if !col.locked => {
                let id = format!("__cell:{name}:{row_id}:{key}");
                let focused = w.focus == Some(id.as_str());
                let popup = if focused { select_popup(w, &name, row, col, &on_edit) } else { None };
                // Enter resolves a typed query to its first match; without one it's a no-op (no
                // surprise commit of the first option).
                let on_submit = popup
                    .as_ref()
                    .filter(|p| !p.query.is_empty())
                    .and_then(|p| p.options.first())
                    .map(|(_, h)| *h);
                w.editors.push(EditorBinding { id: id.clone(), target: Target::Select, on_submit });
                table::CellBind::Select { id, popup }
            }
            _ => table::CellBind::None,
        }
    };
    let region = list.clone().unwrap_or_default();
    let mut node = table::grid(
        &spec,
        &rows,
        &heights,
        first,
        lead,
        trail,
        &region,
        base.color,
        base.font_size,
        &mut bind,
    );
    node.style = base;
    node
}

/// Emit a read-only grid over a host-computed query result (World B). The result's schema +
/// windowed rows come from [`crate::data::DataAccess`] (all columns read-only); rows are sliced to
/// the visible window each frame (the host owns the result cache, so windowing is a cheap copy and
/// needs no per-list `row_cache`). A `Pending` result renders a loading placeholder.
fn emit_query_table(w: &mut Walk, qref: data::QueryRef, region: &str, base: Style) -> Node {
    let handle = match qref.state {
        QueryState::Ready(h) => h,
        QueryState::Pending => return placeholder("computing…", base),
    };
    let access = &qref.access;
    let spec = access.spec(handle);
    let total = access.len(handle);
    let row_h = spec.row_height.unwrap_or((base.font_size * 1.4 + 12.0).ceil());
    let offset_y = w.scroll.get(&format!("__tbody:{region}")).map_or(0.0, |v| v.y);
    let (range, lead, trail) = table::row_window(total, row_h, offset_y, w.viewport_h);
    let rows = access.window(handle, range.start, range.len());
    let heights = vec![row_h; rows.len()];
    let mut bind = |_: &table::Row, _: &Column| table::CellBind::None; // results are read-only
    let mut node = table::grid(
        &spec, &rows, &heights, range.start, lead, trail, region, base.color, base.font_size, &mut bind,
    );
    node.style = base;
    node
}

/// Resolve a chart leaf from a query result: read the whole result (an aggregate — small) and shape
/// it into a [`ChartSpec`]. `x` names the category-label column; `y` names one series column or a
/// list of them. A `Pending` result renders a loading placeholder.
fn emit_chart(t: &Table, qref: data::QueryRef, base: Style) -> Node {
    let handle = match qref.state {
        QueryState::Ready(h) => h,
        QueryState::Pending => return placeholder("computing…", base),
    };
    let access = &qref.access;
    let total = access.len(handle);
    let rows = access.window(handle, 0, total);
    let kind = match str_field(t, "type").as_deref() {
        Some("bar") => ChartKind::Bar,
        Some("scatter") => ChartKind::Scatter,
        _ => ChartKind::Line,
    };
    let x = str_field(t, "x");
    let ys: Vec<String> = match field(t, "y") {
        Value::String(s) => vec![lua_str(&s)],
        Value::Table(l) => (1..=l.raw_len())
            .filter_map(|i| match field_i(&l, i) {
                Value::String(s) => Some(lua_str(&s)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let x_labels: Vec<String> = match &x {
        Some(xk) => rows.iter().map(|r| r.cells.get(xk).map(|v| v.display()).unwrap_or_default()).collect(),
        None => (0..rows.len()).map(|i| i.to_string()).collect(),
    };
    let series = ys
        .iter()
        .map(|yk| ChartSeries {
            name: yk.clone(),
            values: rows.iter().map(|r| cell_f64(r.cells.get(yk))).collect(),
        })
        .collect();
    let mut node = Node::chart(ChartSpec { kind, x_labels, series, color: base.color });
    node.style = base;
    node
}

/// A centred message box in the table/chart's slot (used while a result is `Pending`).
fn placeholder(msg: &str, base: Style) -> Node {
    let color = base.color;
    let font = base.font_size;
    let mut n = Node::col()
        .children(vec![Node::text(msg).font(font).color(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            120,
        ))])
        .justify(crate::node::Align::Center)
        .align(crate::node::Align::Center);
    n.style = base;
    n
}

/// A cell's numeric value as `f64` (for chart series); non-numeric / missing cells → 0.
fn cell_f64(v: Option<&CellValue>) -> f64 {
    match v {
        Some(CellValue::Number(n)) => *n,
        Some(CellValue::Decimal(d)) => d.to_string().parse().unwrap_or(0.0),
        Some(CellValue::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(CellValue::Text(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Parse a `ui.table` spec: `columns = { { key, label, type, width }, … }` plus the declarative
/// query (`where` = equality map, `order_by` = key or `{ key, desc = true }`).
fn parse_table_spec(t: &Table) -> Result<TableSpec, String> {
    let mut columns = Vec::new();
    if let Value::Table(cols) = field(t, "columns") {
        for i in 1..=cols.raw_len() {
            if let Value::Table(c) = field_i(&cols, i) {
                let key = str_field(&c, "key").ok_or("a table column needs a key")?;
                let kind = match str_field(&c, "type").as_deref() {
                    Some("number") => ColKind::Number,
                    Some("decimal") => ColKind::Decimal,
                    Some("check") => ColKind::Check,
                    Some("select") => ColKind::Select,
                    Some("date") => ColKind::Date,
                    _ => ColKind::Text,
                };
                let label = str_field(&c, "label").unwrap_or_else(|| key.clone());
                let locked = bool_field(&c, "locked");
                // `options = {"open", …}` (select); `transitions = { open = {"doing"} }` gates
                // each value's successors — a value with no entry may move anywhere.
                let mut options = Vec::new();
                if let Value::Table(o) = field(&c, "options") {
                    for i in 1..=o.raw_len() {
                        if let Value::String(s) = field_i(&o, i) {
                            options.push(lua_str(&s));
                        }
                    }
                }
                let mut transitions = HashMap::new();
                if let Value::Table(tr) = field(&c, "transitions") {
                    for pair in tr.pairs::<String, Table>() {
                        let (from, to) = pair.map_err(|e| e.to_string())?;
                        let mut next = Vec::new();
                        for i in 1..=to.raw_len() {
                            if let Value::String(s) = field_i(&to, i) {
                                next.push(lua_str(&s));
                            }
                        }
                        transitions.insert(from, next);
                    }
                }
                columns.push(Column {
                    key,
                    label,
                    kind,
                    width: num(&c, "width"),
                    locked,
                    options,
                    transitions,
                });
            }
        }
    }
    if columns.is_empty() {
        return Err("ui.table needs columns".to_string());
    }
    let mut filter = Vec::new();
    if let Value::Table(wh) = field(t, "where") {
        for pair in wh.pairs::<String, Value>() {
            let (k, v) = pair.map_err(|e| e.to_string())?;
            if let Some(cv) = scalar_cell(&v) {
                filter.push((k, cv));
            }
        }
    }
    let order = match field(t, "order_by") {
        Value::String(s) => Some((lua_str(&s), false)),
        Value::Table(o) => match field_i(&o, 1) {
            Value::String(s) => Some((lua_str(&s), bool_field(&o, "desc"))),
            _ => None,
        },
        _ => None,
    };
    Ok(TableSpec { columns, filter, order, row_height: num(t, "row_height"), row_heights: HashMap::new() })
}

/// A Lua scalar as a [`CellValue`] (for `where` comparisons); non-scalars are `None`.
fn scalar_cell(v: &Value) -> Option<CellValue> {
    match v {
        Value::Boolean(b) => Some(CellValue::Bool(*b)),
        Value::Integer(i) => Some(CellValue::Number(*i as f64)),
        Value::Number(n) => Some(CellValue::Number(*n)),
        Value::String(s) => Some(CellValue::Text(lua_str(s))),
        _ => None,
    }
}

// --- small typed field readers (Lua value → Rust) ---------------------------

fn field(t: &Table, key: &str) -> Value {
    t.get(key).unwrap_or(Value::Nil)
}

fn field_i(t: &Table, i: usize) -> Value {
    t.get(i).unwrap_or(Value::Nil)
}

fn num(t: &Table, key: &str) -> Option<f32> {
    match field(t, key) {
        Value::Integer(i) => Some(i as f32),
        Value::Number(n) => Some(n as f32),
        _ => None,
    }
}

fn str_field(t: &Table, key: &str) -> Option<String> {
    match field(t, key) {
        Value::String(s) => Some(lua_str(&s)),
        _ => None,
    }
}

fn color_field(t: &Table, key: &str) -> Option<Color32> {
    match field(t, key) {
        Value::String(s) => style::parse_color(&lua_str(&s)),
        _ => None,
    }
}

/// The first array element of a leaf table, as a string (its text content).
fn first_string(t: &Table) -> String {
    match field_i(t, 1) {
        Value::String(s) => lua_str(&s),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn bool_field(t: &Table, key: &str) -> bool {
    matches!(field(t, key), Value::Boolean(true))
}

/// Parse a text node's array items into styled runs: a string/number item is a plain run; a table
/// item is a styled run, e.g. `{ "world", bold = true, color = "#f47068" }`.
fn parse_runs(t: &Table) -> Vec<Run> {
    let mut runs = Vec::new();
    for i in 1..=t.raw_len() {
        match field_i(t, i) {
            Value::String(s) => runs.push(Run::plain(lua_str(&s))),
            Value::Integer(n) => runs.push(Run::plain(n.to_string())),
            Value::Number(n) => runs.push(Run::plain(n.to_string())),
            Value::Table(run) => runs.push(parse_run(&run)),
            _ => {}
        }
    }
    if runs.is_empty() {
        runs.push(Run::plain("")); // an empty leaf still needs a run for its caret row
    }
    runs
}

/// One styled run from a `{ "text", bold = true, link = "url", color = "#hex" }` table. Marks are
/// open string keys (unknown ones don't apply); `color` is an explicit foreground.
fn parse_run(t: &Table) -> Run {
    let mut marks = Marks::new();
    for key in ["bold", "italic", "strike", "code"] {
        if bool_field(t, key) {
            marks = marks.flag(key);
        }
    }
    if let Some(url) = str_field(t, "link") {
        marks = marks.with("link", url);
    }
    Run { text: first_string(t), marks, color: color_field(t, "color") }
}

/// Highlight `source` as `lang` into monospace [`Run`]s, one per token, each coloured by its
/// [`code_highlight::HlKind`]. An unknown language falls back to a single uncoloured mono run.
fn highlight_runs(lang: &str, source: &str) -> Vec<Run> {
    match code_highlight::highlight(lang, source) {
        Some(spans) => spans
            .iter()
            .filter_map(|s| {
                let text = source.get(s.range.clone())?.to_string();
                Some(Run { text, marks: Marks::new().flag("code"), color: Some(hl_color(s.kind)) })
            })
            .collect(),
        None => vec![Run { text: source.to_string(), marks: Marks::new().flag("code"), color: None }],
    }
}

/// A One-Dark-ish palette for syntax tokens, legible on a dark (~#1b1e24) code box.
fn hl_color(kind: code_highlight::HlKind) -> Color32 {
    use code_highlight::HlKind::*;
    match kind {
        Keyword => Color32::from_rgb(0xc6, 0x78, 0xdd),
        Function => Color32::from_rgb(0x61, 0xaf, 0xef),
        Type => Color32::from_rgb(0xe5, 0xc0, 0x7b),
        Constant | Number => Color32::from_rgb(0xd1, 0x9a, 0x66),
        String => Color32::from_rgb(0x98, 0xc3, 0x79),
        Comment => Color32::from_rgb(0x7f, 0x84, 0x8e),
        Property | Operator | Escape => Color32::from_rgb(0x56, 0xb6, 0xc2),
        Attribute => Color32::from_rgb(0xe5, 0xc0, 0x7b),
        Tag => Color32::from_rgb(0xe0, 0x6c, 0x75),
        Variable | Punctuation | Text => Color32::from_rgb(0xab, 0xb2, 0xbf),
    }
}

fn lua_str(s: &mlua::String) -> String {
    s.to_str().map(|s| s.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
