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

use std::rc::Rc;

use egui::Color32;
use loro::LoroDoc;
use mlua::{Function, Lua, Table, Value};
use rich_text::{Marks, Run};
use text_edit::TextBuffer;

use crate::node::{Direction, Node, Style, Val};

mod crdt;

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
        let lua = Lua::new();
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

        Script { lua, view, setup_error, handlers: Vec::new(), doc, editors: Vec::new() }
    }

    /// Load a multi-file app: every `*.lua` file is registered as a `require`-able module
    /// (`lib/state.lua` → `require("lib.state")`) via `package.preload`, then `main.lua` runs as
    /// the entry chunk and must `return function() ... end`. Non-`.lua` files (manifest, assets)
    /// are ignored here. Never panics: any failure is captured and reported by every `view()`.
    pub fn load_app(files: &[(String, String)], doc: Rc<LoroDoc>) -> Self {
        let lua = Lua::new();
        let mut view = None;
        let mut setup_error = None;

        // Run the whole setup as one fallible block so the first error wins and is reported.
        let result = (|| -> Result<Function, String> {
            lua.load(UI_PRELUDE).set_name("ui").exec().map_err(|e| format!("engine prelude failed: {e}"))?;
            crdt::install(&lua, doc.clone()).map_err(|e| format!("doc binding failed: {e}"))?;

            // Preload every module except the entry point, so `require` resolves them lazily
            // against this VM (and the chunk only runs the first time it's required).
            let package: Table = lua.globals().get("package").map_err(|e| e.to_string())?;
            let preload: Table = package.get("preload").map_err(|e| e.to_string())?;
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
                preload.set(module, func).map_err(|e| e.to_string())?;
            }

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

        Script { lua, view, setup_error, handlers: Vec::new(), doc, editors: Vec::new() }
    }

    /// A script that failed before any Lua ran (e.g. its source couldn't be read). Every `view()`
    /// reports `error`.
    pub fn failed(error: String) -> Self {
        Script {
            lua: Lua::new(),
            view: None,
            setup_error: Some(error),
            handlers: Vec::new(),
            doc: Rc::new(LoroDoc::new()),
            editors: Vec::new(),
        }
    }

    /// Run the view for one frame and walk its tree into a [`Node`], capturing this frame's
    /// closures. Returns the setup error if the app never loaded, or the view/walk error.
    pub fn view(&mut self) -> Result<Node, String> {
        if let Some(err) = &self.setup_error {
            return Err(err.clone());
        }
        let view = self.view.as_ref().expect("view present when setup succeeded");
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
            Some(f) => f.call::<()>(()).map_err(|e| e.to_string()),
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

/// Overlay an app's `style` table onto a base [`Style`]. Unknown keys are ignored; missing keys
/// keep the base value.
fn parse_style(style: Value, mut base: Style) -> Style {
    let Value::Table(t) = style else {
        return base;
    };
    if let Some(v) = num(&t, "padding") {
        base.padding = v;
    }
    if let Some(v) = num(&t, "gap") {
        base.gap = v;
    }
    if let Some(v) = num(&t, "font") {
        base.font_size = v;
    }
    if let Some(v) = num(&t, "corner") {
        base.corner_radius = v;
    }
    if let Some(v) = num(&t, "grow") {
        base.flex_grow = v;
    }
    if let Some(c) = color_field(&t, "background") {
        base.background = Some(c);
    }
    if let Some(c) = color_field(&t, "color") {
        base.color = c;
    }
    if let Some(d) = str_field(&t, "direction") {
        base.direction = if d == "row" { Direction::Row } else { Direction::Column };
    }
    if let Some(v) = dim(&t, "width") {
        base.width = v;
    }
    if let Some(v) = dim(&t, "height") {
        base.height = v;
    }
    base
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

/// A length: a number is pixels; `"auto"`, `"50%"`, or `"120"` (px) are accepted as strings.
fn dim(t: &Table, key: &str) -> Option<Val> {
    match field(t, key) {
        Value::Integer(i) => Some(Val::Px(i as f32)),
        Value::Number(n) => Some(Val::Px(n as f32)),
        Value::String(s) => {
            let s = lua_str(&s);
            let s = s.trim();
            if s == "auto" {
                Some(Val::Auto)
            } else if let Some(p) = s.strip_suffix('%') {
                p.trim().parse().ok().map(Val::Pct)
            } else {
                s.parse().ok().map(Val::Px)
            }
        }
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

/// Parse `#rrggbb` / `#rrggbbaa` (and a few names) into a colour.
fn parse_color(s: &str) -> Option<Color32> {
    match s.trim() {
        "white" => return Some(Color32::WHITE),
        "black" => return Some(Color32::BLACK),
        "transparent" => return Some(Color32::TRANSPARENT),
        _ => {}
    }
    let hex = s.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
