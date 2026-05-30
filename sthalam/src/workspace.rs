//! Which app-cells the shell opens. A placeholder until apps are loaded from a
//! node — for now every cell is the bundled counter.

use std::sync::atomic::{AtomicUsize, Ordering};

use compositor::{AppView, Workspace};

/// The workspace to open on login: two independent counters in a tab group (each
/// its own thread, state, and surface). Drag a tab out to tile or float them; the
/// "+ New counter" button opens more as floating windows.
pub fn demo_workspace() -> Workspace {
    Workspace::new(vec![
        AppView::new(app_host::App::counter, "Counter 1"),
        AppView::new(app_host::App::counter, "Counter 2"),
    ])
}

/// Open one more counter cell at runtime, with a unique title — the "+ New
/// counter" button, which shows cells can start mid-session.
pub fn new_counter() -> AppView {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    AppView::new(app_host::App::counter, format!("New Counter {n}"))
}
