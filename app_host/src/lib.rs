use std::time::Duration;

/// One frame's worth of drawing, ready for the renderer.
pub struct Surface {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
    pub repaint_after: Duration,
}

/// A running engine app.
pub struct App {
    inner: app_engine::EngineApp,
}

impl App {
    /// Load an app from a vault-stored Lua script (UTF-8 bytes) and an optional CRDT snapshot.
    pub fn from_script(lua_bytes: &[u8], crdt_snapshot: Option<&[u8]>) -> Self {
        let lua = String::from_utf8_lossy(lua_bytes);
        App { inner: app_engine::EngineApp::from_source(&lua, crdt_snapshot) }
    }

    /// The built-in static demo — used by the compositor's demo cell.
    pub fn demo() -> Self {
        App { inner: app_engine::EngineApp::demo() }
    }

    /// Render into a `Ui` directly (shell tab path — no offscreen texture needed).
    /// Returns a CRDT snapshot if state changed, so the caller can persist it to vault.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        self.inner.show(ui)
    }

    /// Run one isolated frame for the compositor (wgpu offscreen texture path).
    pub fn surface(&mut self, input: egui::RawInput, pixels_per_point: f32) -> Surface {
        let frame = self.inner.frame(input, pixels_per_point);
        Surface {
            primitives: frame.primitives,
            textures_delta: frame.textures_delta,
            pixels_per_point: frame.pixels_per_point,
            repaint_after: frame.repaint_after,
        }
    }
}
