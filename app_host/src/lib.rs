//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call closure #n" (see [`LuaMsg`]). Its `view()`
//! walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update loop
//! is unchanged — dispatch just calls the closure the index points at.

mod props;
use mlua::{Function, Lua, Table};
use runtime::vello::peniko::Color;
use runtime::{El, col, row, text, text_input};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug)]
pub enum LuaMsg {
    Call(u32),
    CallStr(u32, String),
    CallPos(u32, f32, f32),
}

pub struct LuaApp {
    vm: Lua, // never read, but every `Function` below borrows from it — dropping it invalidates them

    view_fn: Option<Function>,        //the closure the source returns
    handlers: RefCell<Vec<Function>>, // refilled every frame
    path: Option<PathBuf>,
    error: Option<String>,
}

impl LuaApp {
    pub fn new(source: &str) -> mlua::Result<Self> {
        let vm = sandboxed_vm()?;
        let view_fn = Some(vm.load(source).eval::<Function>()?);
        Ok(Self {
            vm,
            view_fn,
            handlers: RefCell::new(Vec::new()),
            path: None,
            error: None,
        })
    }
    pub fn from_file(path: impl Into<PathBuf>) -> mlua::Result<Self> {
        let vm = sandboxed_vm()?;
        let path = path.into();
        let content = fs::read_to_string(&path);
        match content {
            Ok(c) => {
                let (view_fn, error) = match vm
                    .load(c)
                    .set_name(path.display().to_string())
                    .eval::<Function>()
                {
                    Ok(f) => (Some(f), None),
                    Err(e) => (None, Some(e.to_string())),
                };

                Ok(Self {
                    vm,
                    view_fn,
                    handlers: RefCell::new(Vec::new()),
                    path: Some(path.into()),
                    error: error,
                })
            }
            Err(e) => Err(mlua::Error::runtime(format!(
                "failed to read {}: {e}",
                path.display()
            ))),
        }
    }
}
impl runtime::App for LuaApp {
    type Msg = LuaMsg;
    fn view(&self) -> El<LuaMsg> {
        if let Some(e) = &self.error {
            return text(format!("reload error\n{e}"));
        }
        let Some(view_fn) = &self.view_fn else {
            return text("no view loaded");
        };
        let tree = match view_fn.call::<Table>(()) {
            Ok(t) => t,
            Err(e) => return text(format!("View error: {e}")),
        };
        let mut handlers = self.handlers.borrow_mut();
        handlers.clear();
        match walk(tree, &mut handlers) {
            Ok(el) => el,
            Err(e) => text(format!("walk error: {e}")),
        }
    }
    fn update(&mut self, msg: LuaMsg) {
        let handlers = self.handlers.borrow();

        // A message can outlive the frame that minted its index (queued click, landed animation),
        // so a stale index is expected — drop it rather than panicking.
        let (LuaMsg::Call(i) | LuaMsg::CallStr(i, _) | LuaMsg::CallPos(i, _, _)) = msg;
        let Some(h) = handlers.get(i as usize) else {
            return;
        };
        let result = match msg {
            LuaMsg::Call(_) => h.call::<()>(()),
            LuaMsg::CallStr(_, s) => h.call::<()>(s),
            LuaMsg::CallPos(_, x, y) => h.call::<()>((x, y)),
        };
        if let Err(e) = result {
            eprintln!("handler error: {e}");
        }
    }
    fn reload(&mut self) {
        if let Some(path) = self.path.clone() {
            match Self::from_file(path) {
                Ok(app) => *self = app,
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }
}

const PRELUDE: &str = r#"local function tagged(tag,t)
                            t.tag = tag
                            return t
                        end 
                        ui = {
                            col = function(t) return tagged("col", t) end,
                            row = function(t) return tagged("row", t) end,
                            text = function(t) return tagged("text", t) end,
                            button = function(t) return tagged("button", t) end,
                            input = function(t) return  tagged("input", t) end,
                        }
                    "#;

pub fn sandboxed_vm() -> mlua::Result<Lua> {
    let vm = Lua::new();
    let now_fn = vm.create_function(|_, ()| {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(mlua::Error::external)?
            .as_secs() as i64;
        Ok(secs)
    })?;
    vm.load(PRELUDE).exec()?;
    vm.globals().set("now", now_fn)?;
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
    Ok(vm)
}

fn walk(node: Table, handlers: &mut Vec<Function>) -> mlua::Result<El<LuaMsg>> {
    let tag: String = node.get("tag")?;
    let mut el = match tag.as_str() {
        "col" | "row" | "button" => {
            let mut el = if tag == "col" { col() } else { row() };
            for i in 1..=node.raw_len() {
                let child: Table = node.get(i)?;
                el = el.child(walk(child, handlers)?);
            }
            el
        }
        "text" => {
            let label: String = node.get(1)?;
            text(label)
        }
        "input" => {
            let value: String = node.get("value")?;
            let id: String = node.get("id")?;
            let f: mlua::Function = node.get("on_input")?;
            let idx = handlers.len() as u32;
            handlers.push(f);
            text_input(value, id, move |s| LuaMsg::CallStr(idx, s))
        }

        other => text(format!("unknown tag{}", other)),
    };
    if let Some(f) = node.get::<Option<Function>>("on_drag")? {
        let id: String = node.get("id")?;
        let idx = handlers.len() as u32;
        handlers.push(f);
        el = el.on_drag(id, move |e| LuaMsg::CallPos(idx, e.pos.0, e.pos.1));
    }

    if let Some(f) = node.get::<Option<Function>>("on_drop")? {
        let id: String = node.get("id")?;
        let idx = handlers.len() as u32;
        handlers.push(f);
        el = el.on_drop(id, move |_e| LuaMsg::Call(idx));
    }
    let id: Option<String> = node.get("id")?;
    if let Some(s) = &id {
        el = el.id(s.as_str());
    }

    el = props::apply(el, &node, handlers)?;

    if let Some(scroll) = node.get::<Option<String>>("scroll")? {
        if id.is_none() {
            return Err(mlua::Error::runtime(format!("{tag}: scroll needs an id")));
        };
        match scroll.as_str() {
            "x" => el = el.scroll_x(),
            "y" => el = el.scroll_y(),
            other => return Err(mlua::Error::runtime(format!("bad scroll: {other}"))),
        }
    }
    Ok(el)
}

pub fn parse_color(s: &str) -> mlua::Result<Color> {
    let c = csscolorparser::parse(s).map_err(mlua::Error::external)?;
    let [r, g, b, a] = c.to_rgba8();
    Ok(Color::from_rgba8(r, g, b, a))
}

#[cfg(test)]
mod tests;
