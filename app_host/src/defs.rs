//! Editor type definitions for the sandbox globals, generated from the sandbox itself.
//!
//! gap-log 1.6: an app author writes `ui`, `doc`, `gfx` and gets "undefined global" on every
//! reference — ten of them in a small app, dozens across the kanban corpus. The guide already
//! documents the surface; nothing told the editor.
//!
//! **Generated, not written.** The prop list is registered through a macro that takes names as
//! identifiers (`props.rs`), which is exactly why `six-apps.md` §0 records a contributor grepping
//! for it and concluding a whole feature was missing. A hand-kept definitions file would be that
//! same invisible-registry problem with an extra copy to forget. So: the globals are read off a
//! real `sandboxed_vm`, the props off `Registry` itself, and `generated_defs_are_current` fails
//! when the two disagree.
//!
//! Regenerate with `BLESS=1 cargo test -p app_host generated_defs_are_current`.

use crate::props::Registry;
use crate::{LuaMsg, sandboxed_vm};
use mlua::{Table, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Where the definitions live, relative to the repo root. `.luarc.json` beside it points here.
pub(crate) const DEFS_PATH: &str = "lua-types/osvauld.lua";

/// Sorted key names of a global table, functions and values alike.
fn keys_of(vm: &mlua::Lua, global: &str) -> mlua::Result<Vec<String>> {
    let table: Table = vm.globals().get(global)?;
    let mut names: Vec<String> = table
        .pairs::<Value, Value>()
        .filter_map(|pair| match pair {
            Ok((Value::String(k), _)) => k.to_str().ok().map(|s| s.to_string()),
            _ => None,
        })
        .collect();
    names.sort();
    names
        .iter()
        .any(|_| true)
        .then_some(())
        .ok_or_else(|| mlua::Error::runtime(format!("{global} is empty")))?;
    Ok(names)
}

fn block(title: &str, items: &[String], per_line: usize) -> String {
    let mut out = format!("-- {title}\n");
    for chunk in items.chunks(per_line) {
        out.push_str(&format!("--   {}\n", chunk.join(", ")));
    }
    out
}

/// The definitions file's exact contents.
pub(crate) fn render() -> mlua::Result<String> {
    let (vm, _) = sandboxed_vm()?;
    // `doc` is installed per app, not per VM, so a bare sandbox has `ui` and `gfx` but no
    // document API. Stub plumbing is enough to register the table and read its keys — nothing
    // here opens a doc.
    crate::crdt::install(
        &vm,
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(|_name| Ok(None)),
        Arc::new(|| {}),
    )?;

    let ui = keys_of(&vm, "ui")?;
    let gfx = keys_of(&vm, "gfx")?;
    let doc = keys_of(&vm, "doc")?;

    let mut props: Vec<String> = Registry::<LuaMsg>::PROPS
        .iter()
        .map(|(name, _)| name.to_string())
        .chain(crate::props::STRUCTURAL.iter().map(|s| s.to_string()))
        .collect();
    props.sort();
    props.dedup();

    // Both tables, because they split by *how* a handler is wired, not by what an author types:
    // `on_click` lives in BINDS and `on_enter` in CALLBACKS, and an author cannot tell or care.
    let mut callbacks: Vec<String> = Registry::<LuaMsg>::CALLBACKS
        .iter()
        .map(|(name, _)| name.to_string())
        .chain(Registry::<LuaMsg>::BINDS.iter().map(|(n, _)| n.to_string()))
        .collect();
    callbacks.sort();
    callbacks.dedup();

    // `ui.*` constructors all take one table and return an element. Elements are opaque to the
    // author — they are only ever nested as children — so `El` is a bare class with no fields
    // rather than a guess at a shape that does not exist in Lua.
    let ctors = ui
        .iter()
        .filter(|n| *n != "state")
        .map(|n| {
            format!("--- Element. Children are positional entries; every other key is a prop.\n--- @param spec table\n--- @return El\nfunction ui.{n}(spec) end\n")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let gfx_fns = gfx
        .iter()
        // `...any`, not a guessed shape: `gfx.solid` takes a colour string, `gfx.path` a list of
        // commands, `gfx.frame` a table. A stub that asserts `table` reports correct code as
        // broken, which is worse than saying nothing — see `docs/lua-apps.md` for the real shapes.
        .map(|n| {
            format!("--- @param ... any\n--- @return GfxResource\nfunction gfx.{n}(...) end\n")
        })
        .collect::<Vec<_>>()
        .join("\n");

    // `open` is spelled out below with a real signature, so skip the reflected stub or the file
    // declares it twice and the editor picks whichever it saw last.
    let doc_fns = doc
        .iter()
        .filter(|n| *n != "open")
        .map(|n| format!("--- @param ... any\n--- @return any\nfunction doc.{n}(...) end\n"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        r#"--- @meta
--- OSVAULD SANDBOX GLOBALS — GENERATED, DO NOT EDIT.
---
--- Source of truth is the running sandbox and `app_host/src/props.rs`. Regenerate with
--- `BLESS=1 cargo test -p app_host generated_defs_are_current`; the same test fails when this
--- file drifts, which is the point of generating it (gap-log 1.6).
---
--- This teaches an editor the *names*. `docs/lua-apps.md` is still the contract for what they
--- mean, and unknown props are a hard error at runtime, not a warning here.

--- @class El
El = {{}}

--- @class GfxResource
GfxResource = {{}}

--- @class Doc
Doc = {{}}

--- The element constructors.
--- @class ui
ui = {{}}

{ctors}
--- Per-viewer scratch state, keyed by id and carried across a hot reload. Not in the document:
--- nothing here syncs to a peer.
--- @param id string
--- @param init table|nil
--- @return table
function ui.state(id, init) end

--- Drawing resources for `ui.frame({{ visual = ... }})`.
--- @class gfx
gfx = {{}}

{gfx_fns}
--- The document API — CRDT-backed, shared, persisted.
--- @class doc
doc = {{}}

{doc_fns}
--- This app's document, by name.
--- @param name string
--- @return Doc
function doc.open(name) end

--- Seconds since the Unix epoch. Wall clock: it can step backwards, so never measure a
--- duration with it. For elapsed time use `e.elapsed` in `on_frame` or `e.t` on a pointer event.
--- @return number
function now() end

--- @return string
function uuid() end

{props_block}
{callbacks_block}"#,
        ctors = ctors,
        gfx_fns = gfx_fns,
        doc_fns = doc_fns,
        props_block = block(
            "Props any element accepts (unknown ones are errors):",
            &props,
            6
        ),
        callbacks_block = block("Handler props:", &callbacks, 6),
    ))
}
