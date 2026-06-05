//! Composites isolated app-cells into the shell.
//!
//! Each app runs on its own worker thread (performance isolation) that owns the
//! `app_host::App`; only plain values cross the channels — a translated
//! `RawInput` in, a `Surface` (GPU-ready triangles) out. The shell keeps the GPU
//! to itself: it uploads a worker's latest surface into an offscreen
//! `wgpu::Texture` via the app's *own* `egui_wgpu::Renderer` (so texture ids
//! never collide with the shell's) and composites it into the workspace — a dock
//! of tabs, tiles, and floating windows (egui_dock). The shell never blocks on a
//! worker, so a slow app goes
//! *stale* rather than stalling the others; and drawing is gated on need (input,
//! animation, or a worker wake), so idle cells cost nothing. Moving to a process
//! per app later changes only the channel transport.

// SPIKE (temporary): `CellViewer::ui` draws plain content instead of the cell's
// wgpu texture, leaving the texture/input path unused — silence it here. Revert
// this line together with the spike to restore strict dead-code checking.
#![allow(dead_code)]

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use app_host::Surface;
use egui_dock::{DockArea, DockState, TabViewer};
use egui_wgpu::{RenderState, Renderer, RendererOptions, ScreenDescriptor};

/// A requested repaint interval at or beyond this counts as idle — no redraw is
/// scheduled. egui uses `Duration::MAX` for "no repaint wanted"; the finite cap
/// also absorbs any absurdly-distant request.
const REPAINT_IDLE: Duration = Duration::from_secs(60);

/// A slot the shell fills with its egui `Context` on a cell's first frame, so the
/// worker can wake an idle shell when a surface is ready. `request_repaint` is
/// thread-safe, so no lock is needed.
#[derive(Default)]
struct RepaintTrigger(OnceLock<egui::Context>);

impl RepaintTrigger {
    fn arm(&self, ctx: egui::Context) {
        let _ = self.0.set(ctx); // first set wins
    }
    fn wake(&self) {
        if let Some(ctx) = self.0.get() {
            ctx.request_repaint();
        }
    }
}

/// One app-cell: a handle to its worker thread plus the GPU resources to show the
/// surfaces it produces.
pub struct AppView {
    /// Window title — also egui's tab id and the cell's tab label; unique per cell.
    title: String,
    /// Translated input to the worker (latest-wins; never blocks the shell).
    input_tx: Sender<FrameInput>,
    /// Surfaces back from the worker. Dropping this stops the worker.
    surface_rx: Receiver<Surface>,
    /// Lets the worker wake the shell when a surface lands. Armed on first `show`.
    trigger: Arc<RepaintTrigger>,
    /// The app's own renderer, separate from the shell's so texture ids don't collide.
    renderer: Option<Renderer>,
    tex: Option<AppTex>,
    /// The image's screen rect last frame, to translate input into app space.
    last_rect: egui::Rect,
    /// The newest surface's repaint request — drives whether the cell keeps a
    /// redraw scheduled (animating) or idles until touched.
    repaint_after: Duration,
    /// True until the first surface is composited (cold-start bootstrap).
    awaiting_first: bool,
    /// Whether the pointer was over the cell last frame, to send one `PointerGone`
    /// on leave so hover highlights clear.
    was_over: bool,
}

/// One frame's input for a worker: app-local `RawInput` and the ppp to draw at.
struct FrameInput {
    raw: egui::RawInput,
    pixels_per_point: f32,
}

struct AppTex {
    #[allow(dead_code)] // kept alive so the registered `view` stays valid
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Id of `texture` registered in the shell's renderer, for `ui.image`.
    id: egui::TextureId,
    size_px: [u32; 2],
}

impl AppView {
    /// Create a cell and spawn its worker, which builds the app via `make_app` so
    /// the wasm `Store` is born — and stays — on that thread.
    pub fn new(
        make_app: impl FnOnce() -> app_host::App + Send + 'static,
        title: impl Into<String>,
    ) -> Self {
        let trigger = Arc::new(RepaintTrigger::default());
        let (input_tx, surface_rx) = spawn_worker(make_app, Arc::clone(&trigger));
        Self {
            title: title.into(),
            input_tx,
            surface_rx,
            trigger,
            renderer: None,
            tex: None,
            last_rect: egui::Rect::NOTHING,
            repaint_after: Duration::ZERO, // bootstrap drives the cold start
            awaiting_first: true,
            was_over: false,
        }
    }

    /// The cell's title — its egui tab id, and its tab label in the dock.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// One shell frame for the cell: advance its worker only if it needs it (idle
    /// gating), composite the latest ready surface without blocking, and draw it
    /// into `ui` at `target` logical size (the space the dock allotted this cell).
    fn show(&mut self, ui: &mut egui::Ui, rs: &RenderState, target: egui::Vec2, focused: bool) {
        self.trigger.arm(ui.ctx().clone()); // idempotent

        let ppp = ui.ctx().pixels_per_point();
        let logical = target;
        let size_px =
            [(logical.x * ppp).round().max(1.0) as u32, (logical.y * ppp).round().max(1.0) as u32];

        self.renderer
            .get_or_insert_with(|| Renderer::new(&rs.device, rs.target_format, RendererOptions::default()));
        self.ensure_tex(rs, size_px);

        // Place the texture first, so we know where it landed and whether the
        // pointer is over *this* cell. `contains_pointer` is occlusion-aware, so a
        // cell covered by another window reports "not over" even though the
        // pointer is within its rect — that's what routes input to the front cell
        // only. (Drawing before the render below is fine: `ui.image` just records
        // a paint command, and the texture write still lands before frame paint.)
        let tex_id = self.tex.as_ref().expect("tex set above").id;
        let resp = ui.image(egui::load::SizedTexture::new(tex_id, logical));
        let over = resp.contains_pointer();
        self.last_rect = resp.rect;

        let (raw, activity) = self.gather_input(ui, target, over, focused);

        // Advance the app only when there's a reason to: the user touched it, it's
        // animating, or it hasn't drawn its first surface yet. Otherwise we don't
        // even wake the worker.
        let wants_frame = self.repaint_after < REPAINT_IDLE;
        if activity || wants_frame || self.awaiting_first {
            let _ = self.input_tx.send(FrameInput { raw, pixels_per_point: ppp });
        }

        // Keep a redraw scheduled only while needed: poll during bootstrap, follow
        // the app's cadence while animating. An idle cell schedules nothing — egui
        // repaints on input, and the worker wakes us when a surface arrives.
        if self.awaiting_first {
            ui.ctx().request_repaint();
        } else if wants_frame {
            ui.ctx().request_repaint_after(self.repaint_after);
        }

        // Drain every ready surface. egui's texture deltas are incremental (the
        // font atlas ships only in the first surface), so apply *all* of them;
        // render only the newest geometry.
        let renderer = self.renderer.as_mut().expect("renderer set above");
        let mut newest: Option<(Vec<egui::ClippedPrimitive>, f32)> = None;
        while let Ok(surface) = self.surface_rx.try_recv() {
            for (id, delta) in &surface.textures_delta.set {
                renderer.update_texture(&rs.device, &rs.queue, *id, delta);
            }
            for id in &surface.textures_delta.free {
                renderer.free_texture(id);
            }
            // Distinct fields from `renderer`/`surface_rx`, so the borrow checker
            // allows writing them inside this loop.
            self.repaint_after = surface.repaint_after;
            self.awaiting_first = false;
            newest = Some((surface.primitives, surface.pixels_per_point));
        }
        if let Some((primitives, surf_ppp)) = newest {
            self.render_into_tex(rs, size_px, &primitives, surf_ppp);
        }
    }

    /// Paint the newest primitives into the offscreen texture. Deltas are applied
    /// by the caller during draining, since they must not be skipped.
    fn render_into_tex(
        &mut self,
        rs: &RenderState,
        size_px: [u32; 2],
        primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) {
        let desc = ScreenDescriptor { size_in_pixels: size_px, pixels_per_point };
        let renderer = self.renderer.as_mut().expect("renderer set above");
        let tex = self.tex.as_ref().expect("tex set above");
        let mut encoder =
            rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("app_surface") });
        let user_bufs = renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, primitives, &desc);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("app_surface"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &tex.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, primitives, &desc);
        }
        rs.queue.submit(user_bufs.into_iter().chain(std::iter::once(encoder.finish())));
    }

    /// (Re)create the offscreen texture on a size change, register it with the
    /// shell's renderer (freeing the old one), and clear it so nothing garbage
    /// shows before the first surface.
    fn ensure_tex(&mut self, rs: &RenderState, size_px: [u32; 2]) {
        if self.tex.as_ref().map(|t| t.size_px) == Some(size_px) {
            return;
        }
        if let Some(old) = self.tex.take() {
            rs.renderer.write().free_texture(&old.id);
        }
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("app_surface_tex"),
            size: wgpu::Extent3d { width: size_px[0], height: size_px[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: rs.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Clear once so the first frames (before any surface) aren't garbage.
        let mut encoder =
            rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("clear_app_tex") });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear_app_tex"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        rs.queue.submit(std::iter::once(encoder.finish()));

        let id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear);
        self.tex = Some(AppTex { texture, view, id, size_px });
    }

    /// Build the app's `RawInput` and report whether it carried anything the app
    /// reacts to. `over` (the cell's occlusion-aware `contains_pointer`) gates
    /// pointer input, so events reach a cell only when it's the front one under
    /// the cursor; keyboard reaches the app only when `focused`. Pointer
    /// positions are translated into the app's own (0,0)-based space.
    fn gather_input(&mut self, ui: &egui::Ui, size: egui::Vec2, over: bool, focused: bool) -> (egui::RawInput, bool) {
        let origin = self.last_rect.min.to_vec2();
        let was_over = self.was_over;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size)),
            focused,
            ..Default::default()
        };
        let mut activity = false;
        ui.input(|i| {
            raw.time = Some(i.time);
            raw.modifiers = i.modifiers;
            for ev in &i.raw.events {
                match ev {
                    egui::Event::PointerMoved(p) if over => {
                        raw.events.push(egui::Event::PointerMoved(*p - origin));
                        activity = true;
                    }
                    egui::Event::PointerButton { pos, button, pressed, modifiers } if over => {
                        raw.events.push(egui::Event::PointerButton {
                            pos: *pos - origin,
                            button: *button,
                            pressed: *pressed,
                            modifiers: *modifiers,
                        });
                        activity = true;
                    }
                    egui::Event::MouseWheel { .. } | egui::Event::Zoom(_) if over => {
                        raw.events.push(ev.clone());
                        activity = true;
                    }
                    // Keyboard, text and paste reach only the focused cell.
                    egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Paste(_)
                        if focused =>
                    {
                        raw.events.push(ev.clone());
                        activity = true;
                    }
                    _ => {}
                }
            }
            // Pointer just left the cell: clear hover with one `PointerGone`.
            if was_over && !over {
                raw.events.push(egui::Event::PointerGone);
                activity = true;
            }
        });
        self.was_over = over;
        (raw, activity)
    }
}

// --- Window manager ----------------------------------------------------------
//
// egui_dock owns the whole layout: tabs, splits/tiles, and floating windows, with
// the user dragging cells freely between all three. We supply only each cell's
// content (its app surface) through a `TabViewer`; the dock does the rest.

/// The shell's window manager: a dock of cells the user arranges as tabs, tiles,
/// or floating windows.
pub struct Workspace {
    dock: DockState<AppView>,
}

impl Workspace {
    /// Open a workspace with `tabbed` cells in one tab group. The user can drag a
    /// tab out to split it into a tile or pop it into a floating window. Further
    /// cells open floating via [`Workspace::add_floating`].
    pub fn new(tabbed: Vec<AppView>) -> Self {
        Self { dock: DockState::new(tabbed) }
    }

    /// Open a cell as a floating window; the user can dock it by dragging its tab
    /// into the layout.
    pub fn add_floating(&mut self, app: AppView) {
        self.dock.add_window(vec![app]);
    }

    /// Draw the whole dock for one frame — tabs, tiles, and floating windows.
    /// Keyboard goes only to the focused cell (the dock tracks which that is).
    pub fn ui(&mut self, ui: &mut egui::Ui, rs: &RenderState) {
        // The dock tracks one focused leaf; route keyboard to its active cell.
        // Snapshot its title (owned) first, so that borrow ends before the dock is
        // borrowed again to draw.
        let focused = self.dock.find_active_focused().map(|(_, tab)| tab.title().to_owned());
        let mut viewer = CellViewer { rs, focused };
        DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
    }
}

/// egui_dock adapter: paints each cell's surface as the tab body, routing keyboard
/// only to the focused cell. The dock owns all layout and dragging.
struct CellViewer<'a> {
    rs: &'a RenderState,
    /// Title of the cell that holds focus, so keyboard reaches only it.
    focused: Option<String>,
}

impl TabViewer for CellViewer<'_> {
    type Tab = AppView;

    fn title(&mut self, tab: &mut AppView) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut AppView) {
        // Composite the cell's real app surface (un-spiked): keyboard goes to the
        // focused cell only.
        let focused = self.focused.as_deref() == Some(tab.title());
        tab.show(ui, self.rs, ui.available_size(), focused);
    }

    /// No scroll area around the cell — its surface fills the tab body exactly.
    fn scroll_bars(&self, _tab: &AppView) -> [bool; 2] {
        [false, false]
    }
}

/// Spawn a worker that owns the app and turns input frames into surfaces. It
/// parks on the input channel (no CPU when idle) and exits when the shell drops
/// either channel end.
fn spawn_worker(
    make_app: impl FnOnce() -> app_host::App + Send + 'static,
    trigger: Arc<RepaintTrigger>,
) -> (Sender<FrameInput>, Receiver<Surface>) {
    let (input_tx, input_rx) = mpsc::channel::<FrameInput>();
    let (surface_tx, surface_rx) = mpsc::channel::<Surface>();
    thread::spawn(move || {
        let mut app = make_app();
        while let Some(input) = recv_latest(&input_rx) {
            let surface = app.surface(input.raw, input.pixels_per_point);
            if surface_tx.send(surface).is_err() {
                break; // shell is gone
            }
            trigger.wake(); // composite promptly instead of waiting for input
        }
    });
    (input_tx, surface_rx)
}

/// Block for the next input frame, then collapse any already queued into it —
/// keeping the newest geometry but concatenating every frame's events, so a slow
/// worker drops stale *moves* yet never loses a *click*. `None` once disconnected.
fn recv_latest(rx: &Receiver<FrameInput>) -> Option<FrameInput> {
    let mut input = rx.recv().ok()?;
    while let Ok(mut next) = rx.try_recv() {
        let mut events = std::mem::take(&mut input.raw.events);
        events.append(&mut next.raw.events);
        input = next;
        input.raw.events = events;
    }
    Some(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(n: f32) -> egui::Event {
        egui::Event::PointerMoved(egui::pos2(n, n))
    }

    // A backed-up worker collapses queued frames to the newest geometry/ppp while
    // keeping every event in order — so clicks are never dropped.
    #[test]
    fn recv_latest_merges_events_keeps_newest_ppp() {
        let (tx, rx) = mpsc::channel::<FrameInput>();
        for (n, ppp) in [(1.0_f32, 1.0_f32), (2.0, 1.5), (3.0, 2.0)] {
            let raw = egui::RawInput { events: vec![moved(n)], ..Default::default() };
            tx.send(FrameInput { raw, pixels_per_point: ppp }).unwrap();
        }

        let merged = recv_latest(&rx).expect("three frames were queued");

        assert_eq!(merged.pixels_per_point, 2.0, "newest frame's ppp wins");
        let xs: Vec<f32> = merged
            .raw
            .events
            .iter()
            .map(|e| match e {
                egui::Event::PointerMoved(p) => p.x,
                other => panic!("unexpected event {other:?}"),
            })
            .collect();
        assert_eq!(xs, [1.0, 2.0, 3.0], "all events preserved, in arrival order");
    }

    // When the shell drops its sender, the worker's wait ends instead of hanging.
    #[test]
    fn recv_latest_returns_none_when_shell_disconnects() {
        let (tx, rx) = mpsc::channel::<FrameInput>();
        drop(tx);
        assert!(recv_latest(&rx).is_none());
    }
}
