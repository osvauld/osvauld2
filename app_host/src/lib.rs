//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call closure #n" (see [`LuaMsg`]). Its `view()`
//! walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update loop
//! is unchanged — dispatch just calls the closure the index points at.

use mlua::{Function, Lua, Table};
use runtime::{col, row, text, El};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug)]
pub enum LuaMsg {
    Call(u32),
    CallStr(u32, String),
    CallPos(u32, f32, f32),
}

pub struct LuaApp {
    vm: Lua,
    view_fn: Function,                //the closure the source returns
    handlers: RefCell<Vec<Function>>, // refilled every frame
}

impl LuaApp {
    pub fn new(source: &str) -> mlua::Result<Self> {
        let vm = sandboxed_vm()?;
        let view_fn = vm.load(source).eval::<Function>()?;
        Ok(Self {
            vm,
            view_fn,
            handlers: RefCell::new(Vec::new()),
        })
    }
}
impl runtime::App for LuaApp {
    type Msg = LuaMsg;
    fn view(&self) -> El<LuaMsg> {
        let tree = match self.view_fn.call::<Table>(()) {
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

        let result = match msg {
            LuaMsg::Call(i) => handlers[i as usize].call::<()>(()),
            LuaMsg::CallStr(i, s) => handlers[i as usize].call::<()>(s),
            LuaMsg::CallPos(i, x, y) => handlers[i as usize].call::<()>((x, y)),
        };
        if let Err(e) = result {
            eprintln!("handler error: {e}");
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
        "col" => {
            let mut el = col();
            for i in 1..=node.raw_len() {
                let child: Table = node.get(i)?;
                el = el.child(walk(child, handlers)?);
            }
            el
        }
        "row" => {
            let mut el = row();
            for i in 1..=node.raw_len() {
                let child: Table = node.get(i)?;
                el = el.child(walk(child, handlers)?);
            }
            el
        }
        "text" | "button" => {
            let label: String = node.get(1)?;
            text(label)
        }

        other => text(format!("unknown tag{}", other)),
    };
    if let Some(f) = node.get::<Option<Function>>("on_click")? {
        let idx = handlers.len() as u32;
        handlers.push(f);
        el = el.on_click(LuaMsg::Call(idx))
    }
    Ok(el)
}

#[cfg(test)]
mod tests;
