use super::{App, State};

// egui runs without a window or GPU, so we can drive a real frame headlessly.
// This proves the script compiles, `init()` runs, and the `ui::` bindings
// resolve when `view` calls them — a view error would flip the app to Failed.
#[test]
fn counter_builds_and_runs_a_headless_frame() {
    let mut app = App::counter();
    if let State::Failed(err) = &app.state {
        panic!("counter failed to build:\n{err}");
    }

    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.frame(ui));
    });

    if let State::Failed(err) = &app.state {
        panic!("view() errored on the first frame:\n{err}");
    }
}
