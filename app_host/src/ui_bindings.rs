//! The `ui::` module exposed to Rune apps: immediate-mode egui calls acting on
//! the active `egui::Ui`. The runtime sets a thread-local pointer to that `Ui`
//! for the synchronous span of each `view` call (Rune runs inline on this
//! thread), and the bindings deref it. This is the whole UI capability surface
//! an app gets in step 1.

use std::cell::Cell;

use egui::Ui;
use rune::{ContextError, Module};

thread_local! {
    static UI_PTR: Cell<*mut Ui> = const { Cell::new(std::ptr::null_mut()) };
}

/// Point `UI_PTR` at `ui` for the duration of `f`, then clear it. Only `ui::`
/// bindings read it, and they run synchronously inside `f`.
pub(crate) fn with_ui_scope<R>(ui: &mut Ui, f: impl FnOnce() -> R) -> R {
    UI_PTR.with(|c| c.set(ui as *mut Ui));
    let out = f();
    UI_PTR.with(|c| c.set(std::ptr::null_mut()));
    out
}

fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    let ptr = UI_PTR.with(|c| c.get());
    debug_assert!(!ptr.is_null(), "a ui:: binding ran outside view()");
    // Safety: the runtime sets the pointer to a live `Ui` for the synchronous
    // span of `view`, and Rune calls these bindings inline on the same thread,
    // so the reference cannot outlive or alias the borrow.
    unsafe { f(&mut *ptr) }
}

/// Build the `ui` module installed into every app's VM.
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::with_crate("ui")?;
    m.function("heading", |text: &str| with_ui(|ui| { ui.heading(text); })).build()?;
    m.function("label", |text: &str| with_ui(|ui| { ui.label(text); })).build()?;
    m.function("button", |text: &str| with_ui(|ui| ui.button(text).clicked())).build()?;
    m.function("separator", || with_ui(|ui| { ui.separator(); })).build()?;
    m.function("space", |amount: f64| with_ui(|ui| { ui.add_space(amount as f32); })).build()?;
    Ok(m)
}
