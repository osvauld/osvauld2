//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call closure #n" (see [`LuaMsg`]). Its `view()`
//! walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update loop
//! is unchanged — dispatch just calls the closure the index points at.
//! Item discovery is a prefix scan for `meta` keys; `src` and `state` are loaded on demand.
mod crdt;
mod modules;
mod props;
pub use crdt::{Docs, Resolve, Wake};

use loro::{Container, ExportMode, LoroDoc, ValueOrContainer};
use mlua::{Error, Function, Lua, Table, Value};
use runtime::vello::peniko::Color;
use runtime::{El, col, row, text, text_area, text_input};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crdt::patch_into;
#[derive(Clone, Debug)]
pub enum LuaMsg {
    Call(u32),
    CallStr(u32, String),
    CallPhase(u32, &'static str, f32, f32),
}

pub struct Ctx<'a, M> {
    pub handlers: &'a mut Vec<Function>,
    pub dev: bool,
    pub errors: Vec<String>,
    pub path: String,
    pub to_msg: Rc<dyn Fn(LuaMsg) -> M>,
}
impl<'a, M> Ctx<'a, M> {
    fn new(handlers: &'a mut Vec<Function>, to_msg: Rc<dyn Fn(LuaMsg) -> M>) -> Self {
        Self {
            handlers,
            dev: true,
            errors: Vec::new(),
            path: String::new(),
            to_msg,
        }
    }
}

pub struct LuaApp<M> {
    vm: Lua, // never read, but every `Function` below borrows from it — dropping it invalidates them
    view_fn: Option<Function>, //the closure the source returns
    handlers: RefCell<Vec<Function>>, // refilled every frame
    error: Option<String>,
    fires: Arc<AtomicU64>,
    to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    src: LoroDoc,
    docs: Docs,
}

impl<M: 'static> LuaApp<M> {
    pub fn open(
        src: LoroDoc,
        resolve: Resolve,
        wake: Wake,
        to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    ) -> mlua::Result<Self> {
        let (vm, fires) = sandboxed_vm()?;
        let map = src.get_map("files");
        let Some(ValueOrContainer::Container(Container::Text(t))) = map.get("main.lua") else {
            return Err(Error::runtime("no main.lua in app source"));
        };
        let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
        crdt::install(&vm, docs.clone(), resolve, wake)?;
        // Before `main.lua` runs, because its first line will be a `require`.
        modules::install(&vm, &src)?;

        let main_src = t.to_string();
        let (view_fn, error) = match vm.load(main_src).set_name("main.lua").eval::<Function>() {
            Ok(f) => (Some(f), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(Self {
            vm,
            view_fn,
            handlers: RefCell::new(Vec::new()),
            error,
            fires,
            to_msg,
            src,
            docs,
        })
    }

    pub fn view(&self) -> El<M> {
        //reset budget
        self.fires.store(0, Ordering::Relaxed);
        if let Some(e) = &self.error {
            return text(format!("reload error\n{e}"));
        }
        let Some(view_fn) = &self.view_fn else {
            return text("no view loaded");
        };
        {
            let mut docs = self.docs.borrow_mut();
            for e in docs.values_mut() {
                let v = e.version.load(Ordering::Relaxed);
                if v > e.mirrored {
                    if let Err(err) = patch_into(&self.vm, &e.mirror, &e.doc.get_deep_value()) {
                        return err_box(&format!("mirror: {err}"));
                    }
                    e.mirrored = v;
                }
            }
        }
        let tree = match view_fn.call::<Table>(()) {
            Ok(t) => t,
            Err(e) => return text(format!("View error: {e}")),
        };
        if let Ok(f) = self.vm.globals().get::<Function>("_sweep") {
            let _ = f.call::<()>(());
        }
        let mut handlers = self.handlers.borrow_mut();
        handlers.clear();
        let mut context = Ctx::new(&mut handlers, self.to_msg.clone());
        let el = match walk(tree, &mut context) {
            Ok(el) => el,
            Err(e) => {
                context.errors.push(e.to_string());
                err_box(&e.to_string())
            }
        };
        for e in &context.errors {
            eprintln!("{e}");
        }
        el
    }
    pub fn update(&mut self, msg: LuaMsg) {
        // reset budget
        self.fires.store(0, Ordering::Relaxed);
        let handlers = self.handlers.borrow();

        // A message can outlive the frame that minted its index (queued click, landed animation),
        // so a stale index is expected — drop it rather than panicking.
        let (LuaMsg::Call(i) | LuaMsg::CallStr(i, _) | LuaMsg::CallPhase(i, _, _, _)) = msg;
        let Some(h) = handlers.get(i as usize) else {
            return;
        };
        let result = match msg {
            LuaMsg::Call(_) => h.call::<()>(()),
            LuaMsg::CallStr(_, s) => h.call::<()>(s),
            LuaMsg::CallPhase(_, phase, x, y) => h.call::<()>((phase, x, y)),
        };
        if let Err(e) = result {
            eprintln!("handler error: {e}");
        }
    }

    /// Save every doc whose version has moved since its last successful save. `put` is the
    /// vault write — `(doc name, snapshot bytes)`.
    ///
    /// The shell calls this **after** `update`, not from inside it: MCP and peer writes never
    /// pass through `update` at all, and one call site has to cover all three writers.
    ///
    /// A failed `put` leaves `saved` where it was, so the next flush retries. Advancing it
    /// first would present as "my card vanished after a restart", which is unfindable.
    pub fn flush(
        &mut self,
        mut put: impl FnMut(&str, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        for (name, e) in self.docs.borrow_mut().iter_mut() {
            // Read the counter *before* exporting: a write landing mid-flush then stays dirty
            // rather than being marked saved by a snapshot taken before it.
            let v = e.version.load(Ordering::Relaxed);
            if v <= e.saved {
                continue;
            }
            let bytes = e
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|err| err.to_string())?;
            put(name, &bytes)?;
            e.saved = v;
        }
        Ok(())
    }
}

const PRELUDE: &str = r#"
local _state,_live = {},{}
local function tagger(tag)
    return function(t)
        t.tag = tag
        t.line = debug.info(2, "l")
        return t
    end
end


function _sweep()
    for id in pairs(_state) do 
    if not _live[id] then _state[id] = nil end
    end
    _live={}
end

ui = {
    col = tagger("col"),
    row = tagger("row"),
    text = tagger("text"),
    button = tagger("button"),
    input = tagger("input"),
    text_area = tagger("text_area"),
}

function ui.state(id, init) 
    _live[id] = true
    local s = _state[id]
    if s== nil then
    s = init or {}
    _state[id] = s
    end
    return s
end
"#;

pub fn sandboxed_vm() -> mlua::Result<(Lua, Arc<AtomicU64>)> {
    let vm = Lua::new();
    let now_fn = vm.create_function(|_, ()| {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(mlua::Error::external)?
            .as_secs() as i64;
        Ok(secs)
    })?;
    let uuid_fn = vm.create_function(|_, ()| Ok(uuid::Uuid::new_v4().to_string()))?;
    vm.load(PRELUDE).exec()?;
    vm.globals().set("now", now_fn)?;
    vm.globals().set("uuid", uuid_fn)?;
    let _ = vm.sandbox(true)?;
    let fires = Arc::new(AtomicU64::new(0));
    let f = fires.clone();
    vm.set_interrupt(move |_lua| {
        if f.fetch_add(1, Ordering::Relaxed) > 1_000_000 {
            Err(mlua::Error::runtime("interrupt budget exceeded"))
        } else {
            Ok(mlua::VmState::Continue)
        }
    });
    Ok((vm, fires))
}
fn fail<M>(context: &mut Ctx<M>, msg: String) -> El<M> {
    let msg = format!("{} > {msg}", context.path);
    context.errors.push(msg.clone());
    err_box(&msg)
}
fn children<M: 'static>(
    mut el: El<M>,
    node: &Table,
    context: &mut Ctx<M>,
    tag: &str,
) -> mlua::Result<El<M>> {
    for i in 1..=max_index(node) {
        let mark = context.path.len();
        if context.dev {
            context.path.push_str(&format!("> [{i}]"));
        }
        let child = node.get::<Value>(i)?;
        match child {
            Value::Boolean(false) => {}
            Value::String(s) => {
                el = el.child(text(s.to_str()?.to_owned()));
            }
            Value::Table(t) => {
                if t.contains_key("tag")? {
                    el = el.child(match walk(t, context) {
                        Ok(c) => c,
                        Err(e) => fail(context, e.to_string()),
                    });
                    context.path.truncate(mark);
                } else {
                    if max_index(&t) == 0 && t.pairs::<Value, Value>().count() > 0 {
                        el = el.child(fail(
                            context,
                            format!(
                                "plain table, not an element — missing `ui.col{{...}}`? has: {}",
                                keys(&t)
                            ),
                        ));
                    }
                    el = children(el, &t, context, tag)?;
                }
            }
            Value::Nil => {
                el = el.child(fail(
                    context,
                    "is nil — a helper that forgot to `return`?".into(),
                ))
            }
            Value::Boolean(true) => {
                el = el.child(fail(
                    context,
                    "is `true` — did you write `el and cond` backwards?".into(),
                ))
            }
            other => {
                el = el.child(fail(
                    context,
                    format!("must be an element, string or false, got {}", show(&other)),
                ))
            }
        }
        context.path.truncate(mark);
    }
    Ok(el)
}
fn err_box<M>(msg: &str) -> El<M> {
    let red = Color::from_rgba8(0xEF, 0x44, 0x44, 0xFF);
    let mut el = col()
        .pad(8.0)
        .gap(2.0)
        .radius(4.0)
        .stroke(1.0, red)
        .fill(Color::from_rgba8(0x2A, 0x11, 0x11, 0xFF));
    for chunk in msg.as_bytes().chunks(64) {
        el = el.child(
            text(String::from_utf8_lossy(chunk).into_owned())
                .color(red)
                .font_size(11.0),
        );
    }
    el
}

fn walk<M: 'static>(node: Table, context: &mut Ctx<M>) -> mlua::Result<El<M>> {
    let tag: String = node.get("tag")?;
    // The breadcrumb is dev-only scaffolding, and it isn't cheap: a boundary get for
    // `line`, another for `id`, a `format!`, and a `push_str` — per element, per frame.
    // `walk` is ~80% of a frame's Lua cost (see the `cost_curve` test), so this stays off
    // unless someone is going to read it. With it off, `fail`'s message loses its path.
    let mark = context.path.len();
    if context.dev {
        let line: String = node.get("line")?;
        let seg = match node.get::<Option<String>>("id")? {
            Some(id) => format!("{tag}#{id}:[{line}]"),
            None => format!("{tag}:[{line}]"),
        };
        if !context.path.is_empty() {
            context.path.push_str(" > ");
        }
        context.path.push_str(&seg);
    }

    let r = build(node, context, &tag);
    if r.is_ok() {
        context.path.truncate(mark);
    }
    r
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{:?}", s.to_string_lossy()),
        Value::Table(t) => match t.get::<Option<String>>("tag") {
            Ok(Some(tag)) => format!("<{tag}>"),
            _ => "{...}".into(),
        },

        Value::Function(_) => "function".into(),
        other => other
            .to_string()
            .unwrap_or_else(|_| other.type_name().into()),
    }
}

fn keys(node: &Table) -> String {
    let mut out = Vec::new();
    for pair in node.pairs::<Value, Value>() {
        let Ok((k, v)) = pair else { continue };
        match k {
            Value::Integer(i) => out.push(format!("[{i}] = {}", show(&v))),
            Value::String(s) if s == "tag" || s == "line" => {}
            k => out.push(format!("{}={}", show(&k), show(&v))),
        };
    }
    out.join(",")
}

fn build<M: 'static>(node: Table, context: &mut Ctx<M>, tag: &str) -> mlua::Result<El<M>> {
    let mut el = match tag {
        "col" | "row" | "button" => {
            let el = if tag == "col" { col() } else { row() };
            children(el, &node, context, &tag)?
        }
        "text" => match node.get::<Value>(1)? {
            Value::String(s) => text(s.to_str()?.to_owned()),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "needs its label as child 1, got  {} - has {}",
                    other.type_name(),
                    keys(&node)
                )));
            }
        },
        "input" | "text_area" => {
            if max_index(&node) > 0 {
                return Err(mlua::Error::runtime(format!(
                    "takes no children — put the button beside it, not inside. got: {}",
                    keys(&node)
                )));
            }
            let value: String = node.get("value")?;
            let id: String = node.get("id")?;
            let f: mlua::Function = node.get("on_input")?;
            let idx = context.handlers.len() as u32;
            context.handlers.push(f);
            let to_msg = context.to_msg.clone();
            let map = move |s| to_msg(LuaMsg::CallStr(idx, s));
            if tag == "input" {
                text_input(value, id, map)
            } else {
                text_area(value, id, map)
            }
        }

        other => err_box(&format!("unknown tag{}", other)),
    };
    let id: Option<String> = node.get("id")?;
    if let Some(s) = &id {
        el = el.id(s.as_str());
    }

    // Scroll offset is stored per element id, so a scroller without one silently never scrolls.
    if id.is_none() && (node.contains_key("scroll_x")? || node.contains_key("scroll_y")?) {
        return Err(mlua::Error::runtime(format!("{tag}: scroll needs an id")));
    }

    el = props::apply(el, &node, context)?;
    Ok(el)
}

pub fn parse_color(s: &str) -> mlua::Result<Color> {
    let c = csscolorparser::parse(s).map_err(mlua::Error::external)?;
    let [r, g, b, a] = c.to_rgba8();
    Ok(Color::from_rgba8(r, g, b, a))
}
fn max_index(node: &Table) -> usize {
    let mut max = 0;
    for pair in node.pairs::<Value, Value>() {
        if let Ok((Value::Integer(i), _)) = pair {
            if i > 0 {
                max = max.max(i as usize);
            }
        }
    }
    max
}
#[cfg(test)]
mod tests;
