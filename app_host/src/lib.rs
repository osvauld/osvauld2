//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call closure #n" (see [`LuaMsg`]). Its `view()`
//! walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update loop
//! is unchanged — dispatch just calls the closure the index points at.
//! Item discovery is a prefix scan for `meta` keys; `src` and `state` are loaded on demand.
mod crdt;
mod modules;
mod props;
pub use crdt::{Cores, Docs, Resolve, Wake};

use loro::{Container, EventTriggerKind, ExportMode, LoroDoc, Subscription, ValueOrContainer};
use mlua::{Error, Function, IntoLua, Lua, Table, Value};
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

/// The app's source, and the counter that says when it moved.
///
/// Shared rather than owned for the same reason [`Cores`] is: it outlives every VM built from it.
/// The subscription has to outlive them too — it unsubscribes on drop, so a rebuild that owned
/// one would either lose the watch or start a second one and double-count every later edit.
pub struct Source {
    pub doc: LoroDoc,
    /// Bumped on every commit to `doc`, local or imported, and compared against the per-VM
    /// watermark in [`LuaApp::reload_if_stale`].
    version: Arc<AtomicU64>,
    _sub: Subscription,
}

impl Source {
    /// Start watching a source doc.
    ///
    /// The `Import` gate is the same one the data docs use, and for the same reason: a *local*
    /// source write — a code block edited in-app — already happens inside a frame the host asked
    /// for, and the message that carried it runs the staleness check on its way out. An import
    /// has no such frame, so it has to ask for one. The gate decides whether to schedule an extra
    /// frame; it never decides whether the edit counts, which is why the bump sits above it.
    pub fn new(doc: LoroDoc, wake: Wake) -> Self {
        let version = Arc::new(AtomicU64::new(0));
        let v = version.clone();
        let sub = doc.subscribe_root(Arc::new(move |ev| {
            v.fetch_add(1, Ordering::Relaxed);
            if ev.triggered_by == EventTriggerKind::Import {
                wake();
            }
        }));
        Self {
            doc,
            version,
            _sub: sub,
        }
    }
}

pub struct LuaApp<M> {
    vm: Lua, // never read, but every `Function` below borrows from it — dropping it invalidates them
    view_fn: Option<Function>, //the closure the source returns
    handlers: RefCell<Vec<Function>>, // refilled every frame
    error: Option<String>,
    /// The last reload that failed, rendered by `view()` as a banner *above* the still-running
    /// app. `error` is the other kind: a source that never loaded at all, which leaves nothing to
    /// run. Cleared by the next successful reload, since that replaces the whole struct.
    reload_error: Option<String>,
    fires: Arc<AtomicU64>,
    to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    /// Which version of the source this VM was built from. Read *before* the build, so an edit
    /// landing mid-build leaves the VM stale rather than falsely current.
    src_seen: u64,
    /// The four below outlive the VM, and that is the whole reason [`reload`](Self::reload) can
    /// build a replacement beside the running one: `cores` keeps the open docs (and so their
    /// unflushed writes and their single subscription), `src` keeps the watch, and the rest is
    /// what `build` needs to make a VM at all.
    src: Rc<Source>,
    docs: Docs,
    cores: Cores,
    resolve: Resolve,
    wake: Wake,
}

impl<M: 'static> LuaApp<M> {
    pub fn open(
        src: LoroDoc,
        resolve: Resolve,
        wake: Wake,
        to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    ) -> mlua::Result<Self> {
        let cores: Cores = Rc::new(RefCell::new(HashMap::new()));
        let src = Rc::new(Source::new(src, wake.clone()));
        Self::build(src, cores, resolve, wake, to_msg)
    }

    /// Everything that makes a VM, with the doc cores and the source watch handed in rather than
    /// created — so `open` starts with an empty set and `reload` starts with the running app's.
    ///
    /// A source that fails to compile is **not** an error here: it lands in `self.error` and
    /// `view()` renders it. `reload` is the caller that wants the opposite, and it checks.
    fn build(
        src: Rc<Source>,
        cores: Cores,
        resolve: Resolve,
        wake: Wake,
        to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    ) -> mlua::Result<Self> {
        // Before reading a single file, so a write landing mid-build is still counted as unseen.
        // The other order marks this VM current for an edit it never read, and that edit is then
        // lost until the next one happens to arrive.
        let src_seen = src.version.load(Ordering::Relaxed);
        let (vm, fires) = sandboxed_vm()?;
        let map = src.doc.get_map("files");
        let Some(ValueOrContainer::Container(Container::Text(t))) = map.get("main.lua") else {
            return Err(Error::runtime("no main.lua in app source"));
        };
        let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
        crdt::install(
            &vm,
            docs.clone(),
            cores.clone(),
            resolve.clone(),
            wake.clone(),
        )?;
        // Before `main.lua` runs, because its first line will be a `require`.
        modules::install(&vm, &src.doc)?;

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
            reload_error: None,
            fires,
            to_msg,
            src_seen,
            src,
            docs,
            cores,
            resolve,
            wake,
        })
    }

    /// Rebuild the VM from the current source, keeping the docs and as much per-viewer state as
    /// can cross a VM boundary. On any failure **nothing changes** and the running app is
    /// untouched.
    ///
    /// That guarantee is the reason this builds a whole second app rather than re-evaluating in
    /// place. Lua cannot unload a chunk: re-running `main.lua` here would leave every global the
    /// old version set that the new one does not, and every closure already in `handlers` would
    /// still point at the old upvalues. Building beside and swapping is also what makes the
    /// failure path free — the staged `Docs` map is dropped, and the cores it borrowed are still
    /// held by `self.cores`.
    ///
    /// The three stages are load, setup and **first render**, and the third is not optional: a
    /// `main.lua` that compiles and returns a closure which throws on its first call is the
    /// ordinary case, and without the trial frame we would have swapped before finding out.
    pub fn reload(&mut self) -> Result<(), String> {
        let staged = Self::build(
            self.src.clone(),
            self.cores.clone(),
            self.resolve.clone(),
            self.wake.clone(),
            self.to_msg.clone(),
        )
        .map_err(|e| e.to_string())?;
        if let Some(e) = &staged.error {
            return Err(e.clone());
        }
        let dropped = carry_state(&self.vm, &staged.vm).map_err(|e| e.to_string())?;
        let view_fn = staged.view_fn.as_ref().ok_or("no view loaded")?;
        view_fn.call::<Table>(()).map_err(|e| e.to_string())?;
        // A real frame, not half of one. `_sweep` is what drops carried state belonging to an
        // element the new source no longer draws — skip it and that state lingers until whenever
        // the next frame happens to be.
        if let Ok(f) = staged.vm.globals().get::<Function>("_sweep") {
            f.call::<()>(()).map_err(|e| e.to_string())?;
        }
        // The trial frame filled the staged app's handler table with closures nothing will ever
        // dispatch to; clear it so the first real frame starts from an empty one.
        staged.handlers.borrow_mut().clear();
        *self = staged;
        for name in dropped {
            eprintln!("reload: dropped ui.state({name:?}) — it holds a value tied to the old VM");
        }
        Ok(())
    }

    /// Reload if the source has moved since this VM was built; `None` if it had not.
    ///
    /// This is the caller [`reload`](Self::reload) was written for, and it is the one that keeps
    /// the books — which is what lets `reload`'s "on failure nothing changes" stay literally true.
    /// Two entries:
    ///
    /// The **watermark advances either way**. On success it comes from the staged app, which read
    /// it before building. On failure it is advanced here anyway, so a source that does not
    /// compile is retried once per *edit* rather than once per mouse move — and the next edit is
    /// the fix, which is exactly when a retry is worth anything.
    ///
    /// The **error is recorded**, because a failed reload that says nothing is the worst outcome
    /// there is: you change a file, the app keeps running the old code, and nothing anywhere
    /// connects the two.
    pub fn reload_if_stale(&mut self) -> Option<Result<(), String>> {
        let v = self.src.version.load(Ordering::Relaxed);
        if v <= self.src_seen {
            return None;
        }
        let r = self.reload();
        self.src_seen = self.src_seen.max(v);
        self.reload_error = r.as_ref().err().cloned();
        Some(r)
    }

    pub fn view(&self) -> El<M> {
        //reset budget
        self.fires.store(0, Ordering::Relaxed);
        let body = self.body();
        let Some(e) = &self.reload_error else {
            return body;
        };
        // Above the app, not instead of it. What is on screen is still the last version that
        // worked, and it stays interactive — the banner only says that it is not what is on disk.
        col()
            .full()
            .child(err_box(&format!(
                "source changed but does not load — still running the last good version\n{e}"
            )))
            .child(body)
    }

    fn body(&self) -> El<M> {
        if let Some(e) = &self.error {
            return text(format!("reload error\n{e}"));
        }
        let Some(view_fn) = &self.view_fn else {
            return text("no view loaded");
        };
        {
            let mut docs = self.docs.borrow_mut();
            for e in docs.values_mut() {
                let v = e.core.version.load(Ordering::Relaxed);
                if v > e.mirrored {
                    if let Err(err) = patch_into(&self.vm, &e.mirror, &e.core.doc.get_deep_value())
                    {
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
        // Over the *cores*, not the current VM's open docs. A doc the previous source opened and
        // the new one does not still holds unflushed writes, and iterating the mirrors would
        // silently stop saving it the moment a reload dropped it from the view.
        for (name, core) in self.cores.borrow().iter() {
            // Read the counter *before* exporting: a write landing mid-flush then stays dirty
            // rather than being marked saved by a snapshot taken before it.
            let v = core.version.load(Ordering::Relaxed);
            if v <= core.saved.get() {
                continue;
            }
            let bytes = core
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|err| err.to_string())?;
            put(name, &bytes)?;
            core.saved.set(v);
        }
        Ok(())
    }
}

/// A `ui.state` entry reduced to data. What *cannot* be represented here — a function, a thread,
/// userdata — is exactly what makes an entry uncarryable.
enum Plain {
    Nil,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Table(Vec<(Plain, Plain)>),
}

/// How deep a `ui.state` entry may nest before we give up. This is not a size limit — it is the
/// cycle guard. A table that contains itself would otherwise recurse forever, and returning
/// `None` funnels it into the same "dropped, and said so" path as a coroutine.
const MAX_DEPTH: u32 = 16;

/// Carry `ui.state` across a rebuild, returning the names of the entries that could not come.
///
/// A **filter, not a copy**, and the difference is the point. `_state` holds per-viewer scratch —
/// an unsent draft, whether a panel is open — but it is also where retained *execution* state
/// lands once `ui.run` exists, and a suspended coroutine is a live stack of closures belonging to
/// a chunk that no longer exists. No mechanism can move that to another VM: not serialization,
/// not a Rust-side mirror.
///
/// Dropping it is correct rather than a compromise — an animation whose code just changed should
/// restart, which is the same rule that makes `_sweep` kill an animation when its element leaves
/// the tree. What is not acceptable is doing it *silently*, so the names come back to the caller.
///
/// `_live` is deliberately not carried. The trial frame runs after this, marks whatever the new
/// source actually touches, and `_sweep` drops the rest — so state belonging to an element the
/// new code no longer draws is cleaned up on the way in, for free.
fn carry_state(from: &Lua, to: &Lua) -> mlua::Result<Vec<String>> {
    let old: Table = from.globals().get::<Function>("_dump_state")?.call(())?;
    let new = to.create_table()?;
    let mut dropped = Vec::new();
    for pair in old.pairs::<Value, Value>() {
        let (k, v) = pair?;
        match (plain(&k, 0), plain(&v, 0)) {
            (Some(k), Some(v)) => new.set(into_lua(to, &k)?, into_lua(to, &v)?)?,
            _ => dropped.push(match &k {
                Value::String(s) => s.to_string_lossy(),
                other => format!("{other:?}"),
            }),
        }
    }
    to.globals()
        .get::<Function>("_load_state")?
        .call::<()>(new)?;
    Ok(dropped)
}

/// `None` for anything carrying VM identity, and for anything nested past [`MAX_DEPTH`].
fn plain(v: &Value, depth: u32) -> Option<Plain> {
    if depth > MAX_DEPTH {
        return None;
    }
    match v {
        Value::Nil => Some(Plain::Nil),
        Value::Boolean(b) => Some(Plain::Bool(*b)),
        // Luau integers are i32, so this widens rather than truncating.
        Value::Integer(i) => Some(Plain::Int((*i).into())),
        Value::Number(n) => Some(Plain::Num(*n)),
        Value::String(s) => Some(Plain::Str(s.to_str().ok()?.to_owned())),
        Value::Table(t) => {
            // `pairs` is raw, so a metatable does not come along — which is right: the only
            // metatables in reach here are the mirror's and the container tags, and neither
            // belongs to per-viewer scratch.
            let mut out = Vec::new();
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair.ok()?;
                out.push((plain(&k, depth + 1)?, plain(&v, depth + 1)?));
            }
            Some(Plain::Table(out))
        }
        _ => None,
    }
}

fn into_lua(lua: &Lua, p: &Plain) -> mlua::Result<Value> {
    Ok(match p {
        Plain::Nil => Value::Nil,
        Plain::Bool(b) => Value::Boolean(*b),
        Plain::Int(i) => (*i).into_lua(lua)?,
        Plain::Num(n) => Value::Number(*n),
        Plain::Str(s) => Value::String(lua.create_string(s)?),
        Plain::Table(entries) => {
            let t = lua.create_table()?;
            for (k, v) in entries {
                t.set(into_lua(lua, k)?, into_lua(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
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

-- The host's seam onto `ui.state`, used only by `reload` to carry per-viewer scratch across a
-- VM rebuild. `_state` is a local so an app cannot replace the table wholesale; these two are
-- the same deliberate exception `_sweep` already is.
function _dump_state() return _state end
function _load_state(t) _state = t end

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
