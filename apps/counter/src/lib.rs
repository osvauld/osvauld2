//! The counter — the first osvauld app written as a wasm module.
//!
//! This is the whole app: state, and a draw function over the root `egui::Ui`.
//! The `app!` macro supplies the wasm ABI (`alloc`/`dealloc`/`frame`); the host
//! never sees egui — the sandboxed module links the full framework with zero
//! host bindings.

use osvauld_app::egui;

#[derive(Default)]
struct Counter {
    n: i64,
}

osvauld_app::app!(Counter::default(), |ui: &mut egui::Ui, state: &mut Counter| {
    ui.heading("Counter");
    ui.horizontal(|ui| {
        if ui.button("−").clicked() {
            state.n -= 1;
        }
        ui.label(state.n.to_string());
        if ui.button("+").clicked() {
            state.n += 1;
        }
    });
    if ui.button("reset").clicked() {
        state.n = 0;
    }
});
