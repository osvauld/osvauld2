//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call closure #n" (see [`LuaMsg`]). Its `view()`
//! walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update loop
//! is unchanged — dispatch just calls the closure the index points at.
