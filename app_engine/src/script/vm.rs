//! The sandboxed Lua VM apps run in. Apps are untrusted (uploaded) code and this is the only
//! sandbox boundary: a stdlib subset (no `os`/`io`/`package`/`debug`), no filesystem/codegen
//! loaders, a memory cap, an instruction budget per entry, and an owned `require` over the
//! uploaded files.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Function, HookTriggers, Lua, LuaOptions, StdLib, Value, VmState};

/// Memory cap per app VM.
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Instruction budget per entry into Lua (one `view()` or one handler), counted in hook fires.
const HOOK_EVERY: u32 = 10_000;
const MAX_HOOK_FIRES: u64 = 5_000; // ≈ 50M instructions

/// Build a sandboxed VM and its per-entry instruction counter (reset before each Lua entry),
/// so a hostile loop errors instead of hanging the shell.
pub(super) fn sandboxed_vm() -> Result<(Lua, Rc<Cell<u64>>), String> {
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
    let lua = Lua::new_with(libs, LuaOptions::default()).map_err(|e| e.to_string())?;
    lua.set_memory_limit(MEMORY_LIMIT).map_err(|e| e.to_string())?;
    // The base lib always loads; scrub its filesystem/codegen doors.
    for global in ["dofile", "loadfile", "load"] {
        lua.globals().set(global, Value::Nil).map_err(|e| e.to_string())?;
    }
    // `now()` — epoch seconds. The one ambient capability apps get (no `os` lib): ops records
    // want stamps (`on_edit` audit trails) and the clock leaks nothing.
    let now = lua
        .create_function(|_, ()| {
            use std::time::{SystemTime, UNIX_EPOCH};
            Ok(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()))
        })
        .map_err(|e| e.to_string())?;
    lua.globals().set("now", now).map_err(|e| e.to_string())?;
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

/// Register `require` over the uploaded files — the sandbox has no `package` lib, so this is our
/// own shim: every non-entry `*.lua` file becomes a module (`lib/state.lua` →
/// `require("lib.state")`), lazy like `package.preload`, with a loaded-module cache (`false`
/// marks in-progress, catching require cycles). Non-`.lua` files (manifest, assets) are ignored.
/// Returns `main.lua`'s source, if present.
pub(super) fn install_require(lua: &Lua, files: &[(String, String)]) -> Result<Option<String>, String> {
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
            let loader: Function = modules
                .get(name.as_str())
                .map_err(|_| mlua::Error::RuntimeError(format!("module '{name}' not found in app")))?;
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
    Ok(entry)
}
