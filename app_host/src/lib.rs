use std::time::Duration;

pub use app_engine::{FontBytes, PageSpec};

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
    /// Load an app from a vault-stored source tree (`(path, source)` pairs) and an optional
    /// runtime CRDT snapshot.
    pub fn from_files(files: &[(String, String)], crdt_snapshot: Option<&[u8]>) -> Self {
        App { inner: app_engine::EngineApp::from_files(files, crdt_snapshot) }
    }

    /// The built-in static demo — used by the compositor's demo cell.
    pub fn demo() -> Self {
        App { inner: app_engine::EngineApp::demo() }
    }

    /// Rebuild from edited source, keeping the live runtime state (so the run preview reflects an
    /// in-editor edit without resetting the app).
    pub fn reload_source(&mut self, files: &[(String, String)]) {
        self.inner.reload_source(files);
    }

    /// The app's print-page declaration, if it made one (see [`PageSpec`]).
    pub fn page(&self) -> Option<PageSpec> {
        self.inner.page()
    }

    /// Export a page-declaring app to PDF bytes (see [`app_engine::EngineApp::export_pdf`]).
    pub fn export_pdf(&mut self, fonts: FontBytes) -> Result<Vec<u8>, String> {
        self.inner.export_pdf(fonts)
    }

    /// Screenshot the app off-screen as PNG bytes (see [`app_engine::EngineApp::screenshot`]).
    pub fn screenshot(
        &mut self,
        width: f32,
        height: f32,
        scale: f32,
        clear: egui::Color32,
        fonts: FontBytes,
    ) -> Result<Vec<u8>, String> {
        self.inner.screenshot(width, height, scale, clear, fonts)
    }

    /// Merge an external runtime-CRDT write (MCP app-data) into the live doc.
    pub fn import_state(&self, snapshot: &[u8]) -> Result<(), String> {
        self.inner.import_state(snapshot)
    }

    /// The runtime CRDT as a snapshot (for persisting a merged union).
    pub fn export_state(&self) -> Option<Vec<u8>> {
        self.inner.export_state()
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
