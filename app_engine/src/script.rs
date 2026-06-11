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

use std::cell::Cell;
use std::rc::Rc;

use egui::Color32;
use loro::LoroDoc;
use mlua::{Function, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};
use rich_text::{Marks, Run};
use text_edit::TextBuffer;

use crate::node::{Align, Border, BoxShadow, Corners, Direction, Edges, Node, Position, Style, Val};
use crate::PageSpec;

mod crdt;

/// Memory cap per app VM.
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Instruction budget per entry into Lua (one `view()` or one handler), counted in hook fires.
const HOOK_EVERY: u32 = 10_000;
const MAX_HOOK_FIRES: u64 = 5_000; // ≈ 50M instructions

/// Build a sandboxed VM and its per-entry instruction counter (reset before each Lua entry).
///
/// Apps run untrusted (uploaded) code, and this VM is the only sandbox boundary: no `os`/`io`/
/// `package`/`debug`, no filesystem loaders, a memory cap, and an instruction budget per entry
/// into Lua so a hostile loop errors instead of hanging the shell.
fn sandboxed_vm() -> Result<(Lua, Rc<Cell<u64>>), String> {
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
    let lua = Lua::new_with(libs, LuaOptions::default()).map_err(|e| e.to_string())?;
    lua.set_memory_limit(MEMORY_LIMIT).map_err(|e| e.to_string())?;
    // The base lib always loads; scrub its filesystem/codegen doors.
    for global in ["dofile", "loadfile", "load"] {
        lua.globals().set(global, Value::Nil).map_err(|e| e.to_string())?;
    }
    let fires = Rc::new(Cell::new(0_u64));
    let counter = fires.clone();
    lua.set_hook(HookTriggers::new().every_nth_instruction(HOOK_EVERY), move |_, _| {
        let n = counter.get() + 1;
        counter.set(n);
        if n > MAX_HOOK_FIRES {
            Err(mlua::Error::RuntimeError("app exceeded its instruction budget".into()))
        } else {
            Ok(VmState::Continue)
        }
    });
    Ok((lua, fires))
}

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
    /// The app's CRDT — the same `Rc` the engine owns. Resolves an editor id to its backing
    /// LoroText without round-tripping through Lua.
    doc: Rc<LoroDoc>,
    /// This frame's editable fields. Rebuilt by every `view()`; [`Script::with_buffer`] resolves
    /// an id to its buffer.
    editors: Vec<EditorBinding>,
}

/// One editable field in the current view: app-given id, backing text container name, and an
/// optional `on_submit` handler index (fired by Enter while focused).
struct EditorBinding {
    id: String,
    name: String,
    on_submit: Option<u32>,
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

        Script { lua, fires, view, setup_error, handlers: Vec::new(), doc, editors: Vec::new() }
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

            // The sandbox has no `package` lib, so `require` is our own shim over the uploaded
            // files: lazy like `package.preload`, with a loaded-module cache (`false` marks
            // in-progress, catching require cycles).
            let modules = lua.create_table().map_err(|e| e.to_string())?;
            let loaded = lua.create_table().map_err(|e| e.to_string())?;
            let mut entry = None;
            for (path, src) in files {
                if !path.ends_with(".lua") {
                    continue;
                }
                if path == "main.lua" {
                    entry = Some(src.clone());
                    continue;
                }
                let module = path.trim_end_matches(".lua").replace('/', ".");
                let func = lua
                    .load(src)
                    .set_name(&format!("@{path}"))
                    .into_function()
                    .map_err(|e| e.to_string())?;
                modules.set(module, func).map_err(|e| e.to_string())?;
            }
            let require = {
                let (modules, loaded) = (modules.clone(), loaded.clone());
                lua.create_function(move |_, name: String| {
                    match loaded.get::<Value>(name.as_str())? {
                        Value::Nil => {}
                        Value::Boolean(false) => {
                            return Err(mlua::Error::RuntimeError(format!("require cycle on '{name}'")))
                        }
                        cached => return Ok(cached),
                    }
                    let loader: Function = modules.get(name.as_str()).map_err(|_| {
                        mlua::Error::RuntimeError(format!("module '{name}' not found in app"))
                    })?;
                    loaded.set(name.as_str(), false)?;
                    let result: Value = loader.call(())?;
                    // A module that returns nothing still caches as `true`, like Lua's require.
                    let result = if matches!(result, Value::Nil) { Value::Boolean(true) } else { result };
                    loaded.set(name.as_str(), &result)?;
                    Ok(result)
                })
                .map_err(|e| e.to_string())?
            };
            lua.globals().set("require", require).map_err(|e| e.to_string())?;

            let entry = entry.ok_or_else(|| "app has no main.lua".to_string())?;
            match lua.load(&entry).set_name("@main.lua").eval::<Value>().map_err(|e| e.to_string())? {
                Value::Function(f) => Ok(f),
                other => Err(format!("main.lua must `return function() ... end`, got {}", other.type_name())),
            }
        })();

        match result {
            Ok(f) => view = Some(f),
            Err(e) => setup_error = Some(e),
        }

        Script { lua, fires, view, setup_error, handlers: Vec::new(), doc, editors: Vec::new() }
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
            doc: Rc::new(LoroDoc::new()),
            editors: Vec::new(),
        }
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
        let view = self.view.as_ref().expect("view present when setup succeeded");
        self.fires.set(0); // fresh instruction budget for this entry
        let tree: Value = view.call(()).map_err(|e| e.to_string())?;
        let doc = self.doc.clone();
        let mut w = Walk { handlers: Vec::new(), editors: Vec::new(), doc: &doc };
        let node = walk(tree, &mut w)?;
        self.handlers = w.handlers;
        self.editors = w.editors;
        Ok(node)
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

    /// The backing container name for editor `id` in the current view, if it's a known editor.
    fn editor_name(&self, id: &str) -> Option<&str> {
        self.editors.iter().find(|e| e.id == id).map(|e| e.name.as_str())
    }

    /// Run an editing closure against editor `id`'s live buffer (a [`TextBuffer`] over its backing
    /// LoroText), committing once after. Returns whether the content changed; `false` (closure not
    /// run) if `id` isn't an editor in the current view.
    pub fn with_buffer(&self, id: &str, f: impl FnOnce(&mut dyn TextBuffer)) -> bool {
        let Some(name) = self.editor_name(id) else {
            return false;
        };
        let mut buf = crdt::LoroTextBuffer::open(self.doc.clone(), name);
        f(&mut buf);
        buf.commit();
        buf.dirty
    }

    /// Fire editor `id`'s `on_submit` closure (Enter while focused), if it declared one. Returns
    /// whether a handler ran; a handler error is returned to surface.
    pub fn submit(&self, id: &str) -> Result<bool, String> {
        match self.editors.iter().find(|e| e.id == id).and_then(|e| e.on_submit) {
            Some(handler) => self.dispatch(handler).map(|_| true),
            None => Ok(false),
        }
    }
}

/// Collectors threaded through one `view()` walk: click closures, editor bindings, and the doc to
/// read each editor's current content from.
struct Walk<'a> {
    handlers: Vec<Function>,
    editors: Vec<EditorBinding>,
    doc: &'a LoroDoc,
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
                    w.editors.push(EditorBinding { id, name, on_submit });
                    node
                }
                "row" | "col" => {
                    let mut node = if tag == "row" { Node::row() } else { Node::col() };
                    node.style = parse_style(style, node.style);
                    // `scroll = true` (single region, id "") or `scroll = "name"` (a keyed region)
                    // makes this container scroll its overflowing content vertically.
                    node.scroll = match field(&t, "scroll") {
                        Value::Boolean(true) => Some(String::new()),
                        Value::String(s) => Some(lua_str(&s)),
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

/// Overlay an app's `style` table onto a base [`Style`]. Keys follow CSS names (snake_case),
/// with the original short forms kept as aliases. Unknown keys are ignored; missing keys keep
/// the base value.
fn parse_style(style: Value, mut base: Style) -> Style {
    let Value::Table(t) = style else {
        return base;
    };
    // -- flex container -------------------------------------------------------
    if let Some(d) = str_field(&t, "flex_direction").or_else(|| str_field(&t, "direction")) {
        base.direction = if d == "row" { Direction::Row } else { Direction::Column };
    }
    if bool_field(&t, "flex_wrap") || str_field(&t, "flex_wrap").as_deref() == Some("wrap") {
        base.wrap = true;
    }
    if let Some(a) = align_field(&t, "justify_content").or_else(|| align_field(&t, "justify")) {
        base.justify_content = Some(a);
    }
    if let Some(a) = align_field(&t, "align_items") {
        base.align_items = Some(a);
    }
    if let Some(a) = align_field(&t, "align_self") {
        base.align_self = Some(a);
    }
    if let Some(v) = num(&t, "gap") {
        base.gap = v;
    }
    if let Some(v) = num(&t, "flex_grow").or_else(|| num(&t, "grow")) {
        base.flex_grow = v;
    }
    if let Some(v) = num(&t, "flex_shrink") {
        base.flex_shrink = v;
    }
    // -- box ------------------------------------------------------------------
    if let Some(v) = dim(&t, "width") {
        base.width = v;
    }
    if let Some(v) = dim(&t, "height") {
        base.height = v;
    }
    if let Some(v) = dim(&t, "min_width") {
        base.min_width = v;
    }
    if let Some(v) = dim(&t, "min_height") {
        base.min_height = v;
    }
    if let Some(v) = dim(&t, "max_width") {
        base.max_width = v;
    }
    if let Some(v) = dim(&t, "max_height") {
        base.max_height = v;
    }
    if let Some(e) = edges(&t, "padding") {
        base.padding = e;
    }
    if let Some(e) = edges(&t, "margin") {
        base.margin = e;
    }
    // -- position -------------------------------------------------------------
    if let Some(p) = str_field(&t, "position") {
        base.position = if p == "absolute" { Position::Absolute } else { Position::Relative };
    }
    if let Some(e) = edges(&t, "inset") {
        base.inset = e;
    }
    for (key, side) in [("top", 0), ("right", 1), ("bottom", 2), ("left", 3)] {
        if let Some(v) = dim(&t, key) {
            match side {
                0 => base.inset.top = v,
                1 => base.inset.right = v,
                2 => base.inset.bottom = v,
                _ => base.inset.left = v,
            }
        }
    }
    // -- paint ------------------------------------------------------------------
    if let Some(c) = color_field(&t, "background").or_else(|| color_field(&t, "bg")) {
        base.background = Some(c);
    }
    if let Some(c) = corners(&t) {
        base.corner_radius = c;
    }
    if let Some(b) = border_field(&t, "border") {
        base.border = Some(b);
    }
    if let Some(s) = shadow_field(&t, "box_shadow").or_else(|| shadow_field(&t, "shadow")) {
        base.shadow = Some(s);
    }
    if let Some(v) = num(&t, "opacity") {
        base.opacity = v.clamp(0.0, 1.0);
    }
    // -- text -------------------------------------------------------------------
    if let Some(c) = color_field(&t, "color") {
        base.color = c;
    }
    if let Some(v) = num(&t, "font_size").or_else(|| num(&t, "font")) {
        base.font_size = v;
    }
    base
}

/// One CSS alignment keyword (`-` and `_` both accepted, `flex-start`/`start` alike).
fn align_field(t: &Table, key: &str) -> Option<Align> {
    let s = str_field(t, key)?;
    match s.replace('_', "-").as_str() {
        "start" | "flex-start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" | "flex-end" => Some(Align::End),
        "stretch" => Some(Align::Stretch),
        "baseline" => Some(Align::Baseline),
        "space-between" => Some(Align::SpaceBetween),
        "space-around" => Some(Align::SpaceAround),
        "space-evenly" => Some(Align::SpaceEvenly),
        _ => None,
    }
}

/// One length token: `auto`, `50%`, `24`, `24px`, `1.5rem` (1rem = 16px).
fn parse_val(s: &str) -> Option<Val> {
    let s = s.trim();
    if s == "auto" {
        return Some(Val::Auto);
    }
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse().ok().map(Val::Pct);
    }
    if let Some(r) = s.strip_suffix("rem") {
        return r.trim().parse::<f32>().ok().map(|v| Val::Px(v * 16.0));
    }
    let s = s.strip_suffix("px").unwrap_or(s);
    s.trim().parse().ok().map(Val::Px)
}

/// Per-side values: a number applies to all sides; a string is the CSS 1/2/3/4-value shorthand
/// (`"10 20"` = vertical horizontal, …), each token a [`parse_val`] length.
fn edges(t: &Table, key: &str) -> Option<Edges> {
    match field(t, key) {
        Value::Integer(i) => Some(Edges::px(i as f32)),
        Value::Number(n) => Some(Edges::px(n as f32)),
        Value::String(s) => {
            let s = lua_str(&s);
            let v: Vec<Val> = s.split_whitespace().filter_map(parse_val).collect();
            match v.as_slice() {
                [a] => Some(Edges::all(*a)),
                [v, h] => Some(Edges { top: *v, bottom: *v, left: *h, right: *h }),
                [top, h, bottom] => {
                    Some(Edges { top: *top, bottom: *bottom, left: *h, right: *h })
                }
                [top, right, bottom, left] => {
                    Some(Edges { top: *top, right: *right, bottom: *bottom, left: *left })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `border_radius` (alias `corner`): a number rounds all corners; a string is the CSS 4-value
/// form (`"12 12 0 0"`, top-left first, clockwise).
fn corners(t: &Table) -> Option<Corners> {
    let v = field(t, "border_radius");
    let v = if matches!(v, Value::Nil) { field(t, "corner") } else { v };
    match v {
        Value::Integer(i) => Some(Corners::same(i as f32)),
        Value::Number(n) => Some(Corners::same(n as f32)),
        Value::String(s) => {
            let s = lua_str(&s);
            let r: Vec<f32> = s
                .split_whitespace()
                .filter_map(|tok| parse_val(tok).and_then(|v| match v {
                    Val::Px(px) => Some(px),
                    _ => None,
                }))
                .collect();
            match r.as_slice() {
                [a] => Some(Corners::same(*a)),
                [tl, tr, br, bl] => Some(Corners { tl: *tl, tr: *tr, br: *br, bl: *bl }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// CSS-ish `border`: `"1 #3a4151"` / `"2px solid #fff"` — first number is the width, first
/// parsable colour is the colour, `solid` is noise.
fn border_field(t: &Table, key: &str) -> Option<Border> {
    let s = str_field(t, key)?;
    let mut width = None;
    let mut color = None;
    for tok in s.split_whitespace() {
        if width.is_none() {
            if let Some(Val::Px(px)) = parse_val(tok) {
                width = Some(px);
                continue;
            }
        }
        if color.is_none() {
            if let Some(c) = parse_color(tok) {
                color = Some(c);
            }
        }
    }
    Some(Border { width: width?, color: color? })
}

/// CSS-ish `box_shadow`: `"0 4 12 #0008"` (offset-x offset-y blur [spread] colour).
fn shadow_field(t: &Table, key: &str) -> Option<BoxShadow> {
    let s = str_field(t, key)?;
    let mut nums = Vec::new();
    let mut color = None;
    for tok in s.split_whitespace() {
        match parse_val(tok) {
            Some(Val::Px(px)) if nums.len() < 4 => nums.push(px),
            _ => {
                if color.is_none() {
                    color = Some(parse_color(tok)?);
                }
            }
        }
    }
    if nums.len() < 2 {
        return None;
    }
    Some(BoxShadow {
        offset: [nums[0], nums[1]],
        blur: nums.get(2).copied().unwrap_or(0.0),
        spread: nums.get(3).copied().unwrap_or(0.0),
        color: color.unwrap_or(Color32::from_black_alpha(96)),
    })
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
        Value::String(s) => parse_color(&lua_str(&s)),
        _ => None,
    }
}

/// A length: a number is pixels; strings go through [`parse_val`] (`"auto"`, `"50%"`, `"1.5rem"`).
fn dim(t: &Table, key: &str) -> Option<Val> {
    match field(t, key) {
        Value::Integer(i) => Some(Val::Px(i as f32)),
        Value::Number(n) => Some(Val::Px(n as f32)),
        Value::String(s) => parse_val(&lua_str(&s)),
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

fn lua_str(s: &mlua::String) -> String {
    s.to_str().map(|s| s.to_string()).unwrap_or_default()
}

/// Parse any CSS color (hex incl. `#rgb`/`#rgba`, `rgb()`, `hsl()`, `oklch()`, named) into a
/// colour, via `csscolorparser` (CSS Color Level 4).
fn parse_color(s: &str) -> Option<Color32> {
    let [r, g, b, a] = csscolorparser::parse(s.trim()).ok()?.to_rgba8();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

#[cfg(test)]
mod tests;
