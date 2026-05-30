//! The guest SDK an osvauld app is written against.
//!
//! An app is a wasm module: the host hands it a frame's `RawInput` and gets back
//! a drawn [`app_abi::Surface`]. The app links the whole of egui (no host-curated
//! bindings — that's the point of the wasm model) and writes only its UI:
//!
//! ```ignore
//! use osvauld_app::egui;
//!
//! #[derive(Default)]
//! struct Counter { n: i64 }
//!
//! osvauld_app::app!(Counter::default(), |ui: &mut egui::Ui, state: &mut Counter| {
//!     if ui.button("+").clicked() { state.n += 1; }
//!     ui.label(state.n.to_string());
//! });
//! ```
//!
//! The draw fn gets the root [`egui::Ui`]; reach the rest of the framework via
//! `ui.ctx()`. The [`app!`] macro emits the three wasm exports the host calls:
//! `alloc(len)->ptr` / `dealloc(ptr,len)` (the host's handle on guest memory) and
//! `frame(ptr,len)->u64` (draw, returning a packed `(ptr<<32)|len` surface). The
//! allocator frees — the host frees the input it allocated; the guest's output is
//! freed by the host once read.

// Re-exported so an app depends only on `osvauld-app`, and so the `app!` macro
// can name them without the author importing them.
pub use app_abi;
pub use egui;

/// The type-erased frame loop the [`app!`] macro stores. [`Runtime`] is the only
/// implementor; the trait exists so the macro's `thread_local!` can name a
/// concrete type (`Box<dyn FrameApp>`) without spelling the app's state type.
pub trait FrameApp {
    /// Decode this frame's input, draw, and return the encoded surface bytes.
    fn frame(&mut self, input: &[u8]) -> Vec<u8>;
}

/// Holds an app's persistent egui `Context` and state across frames, and the
/// author's draw function. Generic over the app's state `S`.
pub struct Runtime<S> {
    ctx: egui::Context,
    state: S,
    ui: fn(&mut egui::Ui, &mut S),
}

impl<S> Runtime<S> {
    /// Build a runtime from the app's initial state and its draw function. The
    /// `Context` is created once and reused, so egui keeps its state (animations,
    /// focus, the uploaded font atlas) across frames.
    pub fn new(state: S, ui: fn(&mut egui::Ui, &mut S)) -> Self {
        Runtime { ctx: egui::Context::default(), state, ui }
    }
}

impl<S> FrameApp for Runtime<S> {
    fn frame(&mut self, input: &[u8]) -> Vec<u8> {
        // A malformed/absent input shouldn't crash the app — fall back to an
        // empty frame's worth of input (egui then lays out at default size).
        let raw = app_abi::decode_input(input).unwrap_or_default();

        let Runtime { ctx, state, ui } = self;
        let ui = *ui; // copy the fn pointer so the closure doesn't borrow `self`
        let output = ctx.run_ui(raw, |root| ui(root, state));

        // egui's repaint signal for this frame (ZERO animating, MAX idle); the
        // host redraws the cell only when there's something to show.
        let repaint_after = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(core::time::Duration::MAX, |v| v.repaint_delay);

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let surface = app_abi::Surface::from_tessellated(
            primitives,
            output.textures_delta,
            output.pixels_per_point,
            repaint_after,
        );
        app_abi::encode_surface(&surface).expect("a drawn surface always encodes")
    }
}

// --- Raw ABI helpers (used by the `app!` macro's exported functions) ---------
//
// These manipulate the module's own linear memory by raw offset. They are
// `#[doc(hidden)]` plumbing, not app-facing API.

/// Reserve `len` bytes and hand the host the offset; the host owns it until
/// [`__dealloc`]. Capacity is exactly `len` so `__dealloc(ptr, len)` matches.
#[doc(hidden)]
pub fn __alloc(len: u32) -> u32 {
    let mut buf = Vec::<u8>::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr() as usize as u32;
    core::mem::forget(buf);
    ptr
}

/// Free a buffer previously returned by [`__alloc`] or [`__leak`]. `len` must be
/// the length that was handed out (which equals the allocation's capacity).
///
/// # Safety
/// `ptr`/`len` must name a live buffer from this module's allocator.
#[doc(hidden)]
pub unsafe fn __dealloc(ptr: u32, len: u32) {
    drop(Vec::from_raw_parts(ptr as usize as *mut u8, len as usize, len as usize));
}

/// Borrow `len` bytes at `ptr` as a slice for the duration of one frame call.
///
/// # Safety
/// `ptr`/`len` must name a live buffer the host just wrote input into.
#[doc(hidden)]
pub unsafe fn __input(ptr: u32, len: u32) -> &'static [u8] {
    core::slice::from_raw_parts(ptr as usize as *const u8, len as usize)
}

/// Leak the output buffer, packed as `(ptr << 32) | len` for the ABI return.
/// `into_boxed_slice` forces capacity to equal length so the host's
/// `dealloc(ptr, len)` matches.
#[doc(hidden)]
pub fn __leak(buf: Vec<u8>) -> u64 {
    let boxed: Box<[u8]> = buf.into_boxed_slice();
    let len = boxed.len() as u64;
    let ptr = Box::into_raw(boxed) as *mut u8 as usize as u64;
    (ptr << 32) | len
}

/// Define a wasm app: its initial state and its per-frame draw function.
///
/// Expands to the module's `thread_local!` runtime plus the `alloc`/`dealloc`/
/// `frame` exports the host calls. Use it once, at the crate root of a
/// `crate-type = ["cdylib"]` app built for `wasm32-unknown-unknown`.
#[macro_export]
macro_rules! app {
    ($init:expr, $ui:expr) => {
        thread_local! {
            static __OSV_APP: ::core::cell::RefCell<::std::boxed::Box<dyn $crate::FrameApp>> =
                ::core::cell::RefCell::new(::std::boxed::Box::new($crate::Runtime::new($init, $ui)));
        }

        /// Reserve `len` bytes of guest memory for the host to write into.
        #[no_mangle]
        pub extern "C" fn alloc(len: u32) -> u32 {
            $crate::__alloc(len)
        }

        /// Free a buffer the host is done with.
        #[no_mangle]
        pub extern "C" fn dealloc(ptr: u32, len: u32) {
            unsafe { $crate::__dealloc(ptr, len) }
        }

        /// Draw one frame: read `len` input bytes at `ptr`, return packed
        /// `(out_ptr << 32) | out_len` for the encoded surface.
        #[no_mangle]
        pub extern "C" fn frame(ptr: u32, len: u32) -> u64 {
            let input = unsafe { $crate::__input(ptr, len) };
            let out = __OSV_APP.with(|app| app.borrow_mut().frame(input));
            $crate::__leak(out)
        }
    };
}

#[cfg(test)]
mod tests;
