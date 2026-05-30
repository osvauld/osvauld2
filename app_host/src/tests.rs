use super::{App, Inner};

// egui runs without a window or GPU, so we can drive a real frame headlessly:
// load the counter wasm module, hand it one frame of input, and read back the
// surface (tessellated triangles) — no shell `Ui` borrowed and no GPU. This
// proves the module loads in wasmtime, its `alloc`/`dealloc`/`frame` exports
// resolve, and a frame round-trips through the wire ABI into drawable
// primitives. A missing or trapping module flips `App` to `Failed` and draws
// the error inline instead — which this test rejects.
#[test]
fn produces_a_surface_with_primitives() {
    let mut app = App::counter();
    if let Inner::Failed(f) = &app.inner {
        panic!("counter failed to load:\n{}", f.err);
    }

    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 200.0))),
        ..Default::default()
    };
    let surface = app.surface(input, 1.0);

    if let Inner::Failed(f) = &app.inner {
        panic!("frame trapped producing a surface:\n{}", f.err);
    }
    assert!(!surface.primitives.is_empty(), "counter drew nothing");
}
