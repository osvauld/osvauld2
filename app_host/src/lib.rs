//! Runs sandboxed wasm apps that draw immediate-mode egui surfaces.
//!
//! An app is a wasm module built against the `osvauld-app` SDK, exporting
//! `alloc`/`dealloc`/`frame`. The host hands it one frame's `RawInput` (encoded
//! via [`app_abi`]) and gets back a [`Surface`] — GPU-ready triangles plus the
//! texture uploads they reference — which the shell composites into the app's
//! cell. egui lives entirely inside the guest; the host exposes none of it. The
//! module's linear memory is the sandbox.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use wasmtime::{Engine, Instance, Memory, Module, Store, TypedFunc};

/// Inline failure-box colour (matches the shell's `theme::ERR`).
const ERR: egui::Color32 = egui::Color32::from_rgb(0xF4, 0x70, 0x68);

/// Where `App::counter` looks for the built counter module. Computed from the
/// crate dir (not the cwd, which differs between `cargo test` and `cargo run`)
/// so it resolves to the workspace `target/`. Override with `OSV_COUNTER_WASM`.
const DEFAULT_COUNTER_WASM: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../target/wasm32-unknown-unknown/release/counter_app.wasm");

/// Where `App::intro` looks for the built intro module (same scheme as counter).
/// Override with `OSV_INTRO_WASM`.
const DEFAULT_INTRO_WASM: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../target/wasm32-unknown-unknown/release/intro_app.wasm");

/// One frame's worth of drawing, ready for the renderer: egui's own
/// `ClippedPrimitive` list plus the texture uploads it references.
///
/// This is the *host-side* surface. It differs from [`app_abi::Surface`], the
/// *wire* form the guest sends (meshes only, since a sandboxed guest emits no
/// GPU paint callbacks); `App::surface` converts the wire form into this.
pub struct Surface {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
    /// When the app wants to be drawn again (egui's repaint signal): `ZERO` while
    /// it animates, `MAX` when idle. The compositor schedules redraws from this.
    pub repaint_after: Duration,
}

/// A running wasm app, or the error that stopped it. `surface` is called once
/// per repaint with the app-local input and returns the picture the app drew.
pub struct App {
    inner: Inner,
}

enum Inner {
    /// A live module instance.
    Wasm(WasmApp),
    /// A homegrown-engine app (Lua-tree + native render), not a wasm sandbox. Its frame is
    /// produced in-process, so there's no ABI round-trip — but it yields the same `Surface`.
    Engine(app_engine::EngineApp),
    /// The app failed to load or trapped; the host renders the message inline.
    Failed(Failed),
}

/// A failed app and the host-side egui context used to draw its error message
/// (the guest can't draw — it's the thing that's broken).
struct Failed {
    err: String,
    ctx: egui::Context,
}

impl App {
    /// Load the counter app. Never fails the caller: a missing module or a load
    /// error is captured and rendered inline by `surface`.
    pub fn counter() -> Self {
        Self::from_module(module(&counter_wasm_path(), "counter"))
    }

    /// Load the intro app (the POC surface). Same contract as `counter`: a missing
    /// or broken module is captured and rendered inline. Override with `OSV_INTRO_WASM`.
    pub fn intro() -> Self {
        Self::from_module(module(&intro_wasm_path(), "intro"))
    }

    /// The homegrown-engine demo app — the render-spine slice (Taffy + egui paint over a
    /// hand-built view tree). Built in-process, so unlike the wasm apps it can never fail to
    /// load.
    pub fn engine_demo() -> Self {
        App { inner: Inner::Engine(app_engine::EngineApp::demo()) }
    }

    /// Build an `App` from a (maybe-failed) module: a load error is captured so
    /// the host renders it inline instead of failing the caller.
    fn from_module(module: Result<Module, String>) -> Self {
        let inner = match WasmApp::load(&module) {
            Ok(app) => Inner::Wasm(app),
            Err(err) => Inner::Failed(Failed { err, ctx: egui::Context::default() }),
        };
        App { inner }
    }

    /// Run one frame against the app and return the surface it drew. `input` is
    /// in the app's local coordinates (its surface spans (0,0)..`screen_rect`);
    /// translating real screen input into that space is the caller's job.
    /// `pixels_per_point` matches rasterization to the display so text stays
    /// crisp. A trap flips the app to `Failed` and renders the error inline.
    pub fn surface(&mut self, input: egui::RawInput, pixels_per_point: f32) -> Surface {
        match &mut self.inner {
            Inner::Failed(f) => render_failure(&f.ctx, &f.err, input, pixels_per_point),
            Inner::Engine(app) => {
                let frame = app.frame(input, pixels_per_point);
                Surface {
                    primitives: frame.primitives,
                    textures_delta: frame.textures_delta,
                    pixels_per_point: frame.pixels_per_point,
                    repaint_after: frame.repaint_after,
                }
            }
            Inner::Wasm(app) => match app.surface(&input, pixels_per_point) {
                Ok(surface) => surface,
                Err(err) => {
                    // The instance trapped — it can't be trusted to draw again.
                    let ctx = egui::Context::default();
                    let surface = render_failure(&ctx, &err, input, pixels_per_point);
                    self.inner = Inner::Failed(Failed { err, ctx });
                    surface
                }
            },
        }
    }
}

/// A loaded module instance: its store, memory, and the three exported funcs.
/// Owned by whichever thread built it (the compositor's per-app worker), so the
/// non-`Send` `Store` never crosses a thread boundary.
struct WasmApp {
    store: Store<()>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    dealloc: TypedFunc<(u32, u32), ()>,
    frame: TypedFunc<(u32, u32), u64>,
}

impl WasmApp {
    /// Instantiate `module` and bind its exports. The module imports nothing, so
    /// instantiation needs no host functions.
    fn load(module: &Result<Module, String>) -> Result<Self, String> {
        let module = module.as_ref().map_err(|e| e.clone())?;
        let mut store = Store::new(engine(), ());
        let instance = Instance::new(&mut store, module, &[]).map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| "module has no `memory` export".to_string())?;
        let alloc = instance
            .get_typed_func::<u32, u32>(&mut store, "alloc")
            .map_err(|e| format!("missing `alloc` export: {e}"))?;
        let dealloc = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "dealloc")
            .map_err(|e| format!("missing `dealloc` export: {e}"))?;
        let frame = instance
            .get_typed_func::<(u32, u32), u64>(&mut store, "frame")
            .map_err(|e| format!("missing `frame` export: {e}"))?;
        Ok(WasmApp { store, memory, alloc, dealloc, frame })
    }

    /// One frame across the wasm boundary: write the encoded input into a guest
    /// buffer, call `frame`, read back the encoded surface, and free both
    /// buffers (the allocator — the host here — frees what it handed in; the
    /// guest's `frame` allocated the output, which the host frees once read).
    fn surface(&mut self, input: &egui::RawInput, pixels_per_point: f32) -> Result<Surface, String> {
        let mut input = input.clone();
        // Pin the app's rasterization to the display so text stays crisp; the
        // guest reads this from the input rather than a separate host call.
        input.viewports.entry(input.viewport_id).or_default().native_pixels_per_point =
            Some(pixels_per_point);

        let bytes = app_abi::encode_input(&input).map_err(|e| format!("encode input: {e}"))?;
        let in_len = bytes.len() as u32;

        // Host allocates the input buffer in guest memory and writes into it.
        let in_ptr =
            self.alloc.call(&mut self.store, in_len).map_err(|e| format!("alloc trapped: {e}"))?;
        self.memory
            .write(&mut self.store, in_ptr as usize, &bytes)
            .map_err(|e| format!("writing input: {e}"))?;

        // Draw. Returns packed (out_ptr << 32) | out_len.
        let packed = self
            .frame
            .call(&mut self.store, (in_ptr, in_len))
            .map_err(|e| format!("frame trapped: {e}"))?;

        // Done with the input buffer — free it (we allocated it).
        self.dealloc
            .call(&mut self.store, (in_ptr, in_len))
            .map_err(|e| format!("dealloc(input) trapped: {e}"))?;

        let out_ptr = (packed >> 32) as u32;
        let out_len = (packed & 0xffff_ffff) as u32;
        let mut out = vec![0u8; out_len as usize];
        self.memory
            .read(&self.store, out_ptr as usize, &mut out)
            .map_err(|e| format!("reading surface: {e}"))?;

        // Free the output buffer the guest allocated for us.
        self.dealloc
            .call(&mut self.store, (out_ptr, out_len))
            .map_err(|e| format!("dealloc(output) trapped: {e}"))?;

        let wire = app_abi::decode_surface(&out).map_err(|e| format!("decode surface: {e}"))?;
        Ok(Surface {
            primitives: wire.to_clipped_primitives(),
            textures_delta: wire.textures_delta,
            pixels_per_point: wire.pixels_per_point,
            repaint_after: wire.repaint_after,
        })
    }
}

/// The process-wide wasmtime engine. Cheap to share; `Engine` is `Sync`.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::default)
}

/// Get a compiled module for `path` — compiled once per path, then reused. Wasm
/// is portable *bytecode*; wasmtime must compile it to native code (`Module::new`,
/// via Cranelift) before it runs, and that dominates a cold start. Two caches
/// avoid paying it twice: an in-process map (cells of one app spawned together
/// share a compile) and a `<path>.cwasm` on disk a *later run* maps instead of
/// recompiling — "precompile once, map at runtime" (effectively the app's
/// *installed* form for this machine). `Module` is ref-counted (cheap clone per
/// cell); a load error is memoized too. `label` names the app in the build-it
/// error message.
fn module(path: &str, label: &str) -> Result<Module, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Module, String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().expect("module cache mutex poisoned");
    map.entry(path.to_string())
        .or_insert_with(|| load_module(path, label))
        .clone()
}

/// Read `path`'s wasm and produce a ready `Module`, preferring a cached native
/// build (`<path>.cwasm`) and falling back to a fresh compile (which it caches).
fn load_module(path: &str, label: &str) -> Result<Module, String> {
    let wasm = std::fs::read(path).map_err(|e| {
        let upper = label.to_uppercase();
        format!(
            "couldn't read {label} wasm at {path}: {e}\nbuild it with:\n  \
             cargo build -p {label}-app --target wasm32-unknown-unknown --release\n\
             or set OSV_{upper}_WASM to its path"
        )
    })?;

    // Fast path: a native artifact we compiled before, still newer than its
    // wasm. `deserialize` is unsafe because it trusts the bytes are a wasmtime
    // artifact for this engine — they're ours, and a version/engine mismatch is
    // an `Err` (not UB), so a miss just falls through to a fresh compile.
    let cache = format!("{path}.cwasm");
    if fresh(&cache, path) {
        if let Ok(bytes) = std::fs::read(&cache) {
            if let Ok(m) = unsafe { Module::deserialize(engine(), &bytes) } {
                return Ok(m);
            }
        }
    }

    // Slow path: compile wasm → native, then cache it for next time.
    let m = Module::new(engine(), &wasm).map_err(|e| format!("compiling {label} wasm: {e}"))?;
    if let Ok(bytes) = m.serialize() {
        let _ = std::fs::write(&cache, bytes); // best-effort: a failure just recompiles next run
    }
    Ok(m)
}

/// Is `cache` present and at least as new as `source`? A wasm rebuild bumps the
/// source's mtime, which invalidates a now-stale native cache.
fn fresh(cache: &str, source: &str) -> bool {
    let mtime = |p: &str| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    matches!((mtime(cache), mtime(source)), (Some(c), Some(s)) if c >= s)
}

/// The path `App::counter` loads from: `OSV_COUNTER_WASM` if set, else the
/// workspace's default release artifact.
fn counter_wasm_path() -> String {
    std::env::var("OSV_COUNTER_WASM").unwrap_or_else(|_| DEFAULT_COUNTER_WASM.to_string())
}

/// The path `App::intro` loads from: `OSV_INTRO_WASM` if set, else the
/// workspace's default release artifact.
fn intro_wasm_path() -> String {
    std::env::var("OSV_INTRO_WASM").unwrap_or_else(|_| DEFAULT_INTRO_WASM.to_string())
}

/// Draw a failed app's error message on a host-side context and return it as a
/// surface, so a broken app still shows *something* in its cell.
fn render_failure(ctx: &egui::Context, err: &str, input: egui::RawInput, pixels_per_point: f32) -> Surface {
    ctx.set_pixels_per_point(pixels_per_point);
    let output = ctx.run_ui(input, |ui| {
        ui.colored_label(ERR, err);
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    Surface {
        primitives,
        textures_delta: output.textures_delta,
        pixels_per_point: output.pixels_per_point,
        // A failure message is static text — draw it once and idle.
        repaint_after: Duration::MAX,
    }
}
