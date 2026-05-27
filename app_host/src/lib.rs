//! Runs Rune-scripted apps that draw immediate-mode egui surfaces.
//!
//! Step 1: a single app, in-process, one egui Context. The host compiles the
//! app's Rune script, keeps its state across frames, and calls `view` with the
//! live `egui::Ui` exposed through a thread-local pointer (see `ui_bindings`).

mod ui_bindings;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use rune::runtime::Value;
use rune::{Context, Diagnostics, Source, Sources, Vm};

/// Inline failure-box colour (matches the shell's `theme::ERR`).
const ERR: egui::Color32 = egui::Color32::from_rgb(0xF4, 0x70, 0x68);

/// The embedded counter app — step 1 ships exactly one app, baked into the binary.
const COUNTER_SRC: &str = include_str!("../apps/counter.rn");

/// A running Rune app, or the error that stopped it. `frame` is called once per
/// egui repaint with the panel's `Ui`.
pub struct App {
    state: State,
}

enum State {
    Running { vm: Vm, app_state: Value },
    Failed(String),
}

impl App {
    /// Build the embedded counter app. Never fails the caller: a compile or
    /// init error is captured and rendered inline by `frame`.
    pub fn counter() -> Self {
        let state = match build(COUNTER_SRC) {
            Ok((vm, app_state)) => State::Running { vm, app_state },
            Err(err) => State::Failed(err),
        };
        App { state }
    }

    /// Draw one frame: run the app's `view(state)` against `ui`, threading the
    /// returned state back so the script keeps it across frames.
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let failure = match &mut self.state {
            State::Failed(err) => {
                ui.colored_label(ERR, err.as_str());
                return;
            }
            State::Running { vm, app_state } => {
                let call = ui_bindings::with_ui_scope(ui, || vm.call(["view"], (app_state.clone(),)));
                match call {
                    Ok(next) => {
                        *app_state = next;
                        return;
                    }
                    Err(err) => format!("view() failed: {err}"),
                }
            }
        };
        self.state = State::Failed(failure);
    }
}

/// Compile `src`, install the `ui` module, and run `init()` for the starting
/// state. Returns the VM plus that state, ready to drive frames.
fn build(src: &str) -> Result<(Vm, Value), String> {
    let mut context = Context::with_default_modules().map_err(|e| e.to_string())?;
    context
        .install(ui_bindings::module().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let runtime = Arc::new(context.runtime().map_err(|e| e.to_string())?);

    let mut sources = Sources::new();
    sources
        .insert(Source::memory(src).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let mut diagnostics = Diagnostics::new();
    let built = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    let unit = match built {
        Ok(unit) => unit,
        Err(_) => {
            let mut buf = rune::termcolor::Buffer::no_color();
            let _ = diagnostics.emit(&mut buf, &sources);
            return Err(String::from_utf8_lossy(buf.as_slice()).into_owned());
        }
    };

    let mut vm = Vm::new(runtime, Arc::new(unit));
    let app_state = vm.call(["init"], ()).map_err(|e| format!("init() failed: {e}"))?;
    Ok((vm, app_state))
}
