//! `require` for app sources.
//!
//! An app is a folder, and the upload already stores every file it found under its relative
//! path — `files["main.lua"]`, `files["ui/card.lua"]`. Only the *loader* was single-file:
//! [`LuaApp::open`](crate::LuaApp::open) read `main.lua` and ignored everything beside it.
//! This module is the other half.
//!
//! There is no filesystem here and there must not be one — the point of the sandbox is that an
//! app sees its own source doc and nothing else. So `require` resolves against the `files` map
//! alone: no search path, no `package.path`, no way to reach out of the app.

use std::cell::RefCell;
use std::rc::Rc;

use loro::{Container, LoroDoc, ValueOrContainer};
use mlua::{Error, Lua, Table, Value};

/// Registry slot for the module cache: resolved path → whatever that chunk returned.
///
/// The registry is a table only Rust can name, which buys two things. The app cannot reach the
/// host's bookkeeping; and the cached values stay reachable from Lua's own GC roots, which a
/// Rust-side `HashMap<String, Value>` would not — the collector cannot see into a Rust struct.
const LOADED: &str = "osv.modules.loaded";

/// Install the `require` global, reading modules out of `src`'s `files` map.
///
/// Modules are cached per VM, so a reload — which builds a fresh VM against the same docs — is
/// what invalidates them. There is deliberately no way for an app to clear the cache itself: a
/// module that can be re-run mid-frame is a module whose top-level state changes under closures
/// that already captured it.
pub fn install(lua: &Lua, src: &LoroDoc) -> mlua::Result<()> {
    lua.set_named_registry_value(LOADED, lua.create_table()?)?;
    let src = src.clone(); // reference clone: the same doc, not a fork
    // The modules currently being evaluated, innermost last — only non-empty *during* a require,
    // so it costs nothing at rest. Its whole job is to make a cycle nameable.
    let chain: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let require = lua.create_function(move |lua, path: mlua::String| {
        let asked = path.to_str()?.to_string();
        let key = resolve(&asked);

        // Run once, share the result. Without this each requirer would re-run the file and get
        // its own copy, so two modules sharing a state table would silently hold two tables.
        let loaded: Table = lua.named_registry_value(LOADED)?;
        match loaded.get::<Value>(key.as_str())? {
            Value::Nil => {}
            hit => return Ok(hit),
        }
        if chain.borrow().contains(&key) {
            return Err(cycle(&chain.borrow(), &key));
        }
        let source = read(&src, &key).ok_or_else(|| missing(&src, &asked, &key))?;

        chain.borrow_mut().push(key.clone());
        // `set_name` is what puts the module's own path into its runtime errors. Without it
        // every module reports as `[string "..."]` and a stack trace cannot tell you which file
        // it came from.
        let result = lua.load(source).set_name(&key).eval::<Value>();
        chain.borrow_mut().pop();

        // A module that failed stays *out* of the cache on purpose: requiring it again should
        // report the real error again, not hand back a half-built table.
        let value = match result? {
            // Lua's own convention — a module that returns nothing still counts as loaded.
            Value::Nil => Value::Boolean(true),
            v => v,
        };
        loaded.set(key.as_str(), value.clone())?;
        Ok(value)
    })?;
    lua.globals().set("require", require)?;
    Ok(())
}

/// `ui/card` → `ui/card.lua`. An explicit `.lua` is accepted so a path copied out of an error
/// message or a file listing works, but it is the only extension: `.osv` files are data.
///
/// The separator is `/`, matching the stored key. Lua's own convention is `.`, which cannot
/// work here — it collides with the extension the key already carries.
fn resolve(asked: &str) -> String {
    match asked.ends_with(".lua") {
        true => asked.to_string(),
        false => format!("{asked}.lua"),
    }
}

fn read(src: &LoroDoc, key: &str) -> Option<String> {
    match src.get_map("files").get(key) {
        Some(ValueOrContainer::Container(Container::Text(t))) => Some(t.to_string()),
        _ => None,
    }
}

/// The error an agent will hit most often, so it carries the answer with it: every module this
/// app actually has. Without the listing, finding a typo costs another round trip.
fn missing(src: &LoroDoc, asked: &str, key: &str) -> Error {
    let mut names: Vec<String> = src
        .get_map("files")
        .keys()
        .map(|k| k.to_string())
        .filter(|k| k.ends_with(".lua"))
        .collect();
    names.sort();
    Error::runtime(format!(
        "require(\"{asked}\"): no {key} in this app. It has: {}",
        names.join(", ")
    ))
}

/// Naming the whole chain, not just the module that repeats: a cycle is only readable from the
/// path that closed it.
///
/// This guard is not a nicety. Each `require` is a Rust frame calling back into Lua, so a cycle
/// recurses through the *native* stack — the interrupt budget never gets a chance and the
/// process dies with a stack overflow it cannot catch. Verified by disabling this check: the
/// test aborts the whole test binary with SIGABRT rather than failing.
fn cycle(chain: &[String], key: &str) -> Error {
    let mut path = chain.to_vec();
    path.push(key.to_string());
    Error::runtime(format!("require cycle: {}", path.join(" -> ")))
}
