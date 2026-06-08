//! Composites isolated app-cells into the shell.
//!
//! Each app runs on its own worker thread owning the `app_host::App`; only plain
//! values cross the channels (`RawInput` in, GPU-ready `Surface` out). The shell
//! keeps the GPU to itself, uploading each worker's latest surface into an
//! offscreen texture via the app's *own* renderer (so texture ids never collide)
//! and compositing it into an egui_dock of tabs/tiles/floating windows. It never
//! blocks on a worker — a slow app goes stale rather than stalling the others —
//! and draws only on need (input, animation, or a worker wake).

// SPIKE: `CellViewer::ui` draws plain content instead of the wgpu texture,
// leaving the texture/input path unused. Revert with the spike.
#![allow(dead_code)]

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use app_host::Surface;
use egui_dock::{DockArea, DockState, OverlayType, Style, TabViewer};
use egui_wgpu::{RenderState, Renderer, RendererOptions, ScreenDescriptor};

/// A requested repaint interval at or beyond this counts as idle (no redraw
/// scheduled); the finite cap also absorbs egui's `Duration::MAX` "no repaint".
const REPAINT_IDLE: Duration = Duration::from_secs(60);

/// Holds the shell's egui `Context` so an idle worker can wake the shell when a
/// surface is ready. `request_repaint` is thread-safe, so no lock is needed.
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
    /// Input to the worker (latest-wins; never blocks the shell).
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
    /// Newest surface's repaint request: drives whether the cell keeps a redraw
    /// scheduled (animating) or idles until touched.
    repaint_after: Duration,
    /// True until the first surface is composited (cold-start bootstrap).
    awaiting_first: bool,
    /// Pointer was over the cell last frame; on leave, send one `PointerGone` so
    /// hover highlights clear.
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
    /// Create a cell and spawn its worker. `make_app` runs on that worker so the
    /// app's non-`Send` state (Lua VM, egui context) is born and stays there.
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

    /// The cell's title — its egui tab id and tab label in the dock.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// One shell frame for the cell: advance its worker only if needed (idle
    /// gating), composite the latest ready surface without blocking, and draw it
    /// into `ui` at `target` logical size. Returns whether a primary press landed
    /// inside the cell this frame (the shell uses it to move keyboard focus here).
    fn show(&mut self, ui: &mut egui::Ui, rs: &RenderState, target: egui::Vec2, focused: bool) -> bool {
        self.trigger.arm(ui.ctx().clone()); // idempotent

        let ppp = ui.ctx().pixels_per_point();
        let logical = target;
        let size_px =
            [(logical.x * ppp).round().max(1.0) as u32, (logical.y * ppp).round().max(1.0) as u32];

        self.renderer
            .get_or_insert_with(|| Renderer::new(&rs.device, rs.target_format, RendererOptions::default()));
        self.ensure_tex(rs, size_px);

        // Place the texture first to learn where it landed and whether the pointer
        // is over *this* cell. `contains_pointer` is occlusion-aware, so a covered
        // cell reports "not over" — that's what routes input to the front cell only.
        // (Drawing before the render below is fine: `ui.image` only records a paint
        // command, and the texture write still lands before frame paint.)
        let tex_id = self.tex.as_ref().expect("tex set above").id;
        let resp = ui.image(egui::load::SizedTexture::new(tex_id, logical));
        let over = resp.contains_pointer();
        self.last_rect = resp.rect;

        let (raw, activity, pressed_over) = self.gather_input(ui, target, over, focused);

        // Advance the app only on a reason: touched, animating, or pre-first-surface.
        let wants_frame = self.repaint_after < REPAINT_IDLE;
        if activity || wants_frame || self.awaiting_first {
            let _ = self.input_tx.send(FrameInput { raw, pixels_per_point: ppp });
        }

        // Schedule a redraw only while needed: poll during bootstrap, follow the
        // app's cadence while animating. An idle cell schedules nothing — egui
        // repaints on input and the worker wakes us when a surface arrives.
        if self.awaiting_first {
            ui.ctx().request_repaint();
        } else if wants_frame {
            ui.ctx().request_repaint_after(self.repaint_after);
        }

        // Drain every ready surface: egui's texture deltas are incremental (font
        // atlas ships only in the first), so apply *all* of them but render only
        // the newest geometry.
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

        pressed_over
    }

    /// Paint the newest primitives into the offscreen texture. The caller applies
    /// texture deltas during draining, since those must not be skipped.
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
    /// shell's renderer (freeing the old), and clear it so no garbage shows before
    /// the first surface.
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
    /// reacts to. `over` gates pointer input (front cell under the cursor only),
    /// `focused` gates keyboard; pointer positions are translated into the app's
    /// own (0,0)-based space.
    fn gather_input(
        &mut self,
        ui: &egui::Ui,
        size: egui::Vec2,
        over: bool,
        focused: bool,
    ) -> (egui::RawInput, bool, bool) {
        let origin = self.last_rect.min.to_vec2();
        let was_over = self.was_over;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size)),
            focused,
            ..Default::default()
        };
        let mut activity = false;
        // A primary press inside the cell this frame: the shell uses it to grant
        // keyboard focus (egui_dock's body-click focus is unreliable for image cells).
        let mut pressed_over = false;
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
                        if *pressed && *button == egui::PointerButton::Primary {
                            pressed_over = true;
                        }
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
        (raw, activity, pressed_over)
    }
}

// --- Window manager ----------------------------------------------------------
//
// egui_dock owns the whole layout (tabs, tiles, floating windows); we supply only
// each cell's content (its app surface) through a `TabViewer`.

/// The shell's window manager: a dock of cells the user arranges as tabs, tiles,
/// or floating windows.
pub struct Workspace {
    dock: DockState<AppView>,
    /// Which cell owns the keyboard, by title. We track it ourselves because
    /// egui_dock's body-click focus is unreliable for our wgpu-texture cells.
    kbd_focus: Option<String>,
    /// Last frame's egui_dock focus, to detect a tab switch (which moves keyboard).
    prev_dock_focus: Option<String>,
}

impl Workspace {
    /// Open a workspace with `tabbed` cells in one tab group. Further cells open
    /// floating via [`Workspace::add_floating`].
    pub fn new(tabbed: Vec<AppView>) -> Self {
        Self { dock: DockState::new(tabbed), kbd_focus: None, prev_dock_focus: None }
    }

    /// Open a cell as a floating window; the user can dock it by dragging its tab
    /// into the layout.
    pub fn add_floating(&mut self, app: AppView) {
        self.dock.add_window(vec![app]);
    }

    /// Draw the whole dock for one frame. Keyboard goes only to the focused cell.
    pub fn ui(&mut self, ui: &mut egui::Ui, rs: &RenderState) {
        // egui_dock's focused leaf catches tab-header clicks reliably but body
        // clicks only unreliably (the leaf focus hinges on a layer test our image
        // cells fail), so we track body presses ourselves and treat either signal
        // as "focus this cell". Snapshot the owned title so the borrow ends first.
        let dock_focus = self.dock.find_active_focused().map(|(_, tab)| tab.title().to_owned());
        if dock_focus.is_some() && dock_focus != self.prev_dock_focus {
            self.kbd_focus = dock_focus.clone(); // a tab switch moves keyboard
        }
        self.prev_dock_focus = dock_focus;

        let style = dock_style(ui);
        let mut viewer = CellViewer { rs, focused: self.kbd_focus.clone(), claimed: None };
        DockArea::new(&mut self.dock).style(style).show_inside(ui, &mut viewer);

        // A primary press inside a cell body this frame claims keyboard from the next frame on.
        if let Some(title) = viewer.claimed {
            self.kbd_focus = Some(title);
        }
    }
}

/// The dock style for one frame, derived from the shell's egui theme (tab bar,
/// separators, buttons) with two overrides:
///
/// - **Position-based drop overlay** (`HighlightedAreas`) instead of egui_dock's
///   default center button-cluster. The cluster forces you to aim the pointer at a
///   widget in the *center* of the target — but the dragged cell follows the pointer
///   and (egui_dock keeps one shared "hovered leaf" slot, last-drawn wins) shadows
///   the target as you move onto it, so the cluster vanishes mid-aim. Position-based
///   docking reads the drop zone from where the pointer already is (center → tabify,
///   edges → split), so there's nothing to chase.
/// - **Drop highlight in the shell accent** rather than egui_dock's hardcoded cyan.
fn dock_style(ui: &egui::Ui) -> Style {
    let mut style = Style::from_egui(ui.style().as_ref());
    style.overlay.overlay_type = OverlayType::HighlightedAreas;
    style.overlay.selection_color = ui.visuals().selection.bg_fill;
    style
}

/// egui_dock adapter: paints each cell's surface as the tab body, routing keyboard
/// only to the focused cell.
struct CellViewer<'a> {
    rs: &'a RenderState,
    /// Title of the cell that holds focus, so keyboard reaches only it.
    focused: Option<String>,
    /// Set to a cell's title when it takes a primary press this frame; the shell
    /// promotes it to keyboard focus next frame.
    claimed: Option<String>,
}

impl TabViewer for CellViewer<'_> {
    type Tab = AppView;

    fn title(&mut self, tab: &mut AppView) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut AppView) {
        // A press inside the cell claims focus for next frame (our reliable
        // body-click focus); keyboard reaches only the focused cell.
        let focused = self.focused.as_deref() == Some(tab.title());
        if tab.show(ui, self.rs, ui.available_size(), focused) {
            self.claimed = Some(tab.title().to_owned());
        }
    }

    /// No scroll area: the cell's surface fills the tab body exactly.
    fn scroll_bars(&self, _tab: &AppView) -> [bool; 2] {
        [false, false]
    }
}

/// Spawn a worker that owns the app and turns input frames into surfaces. It
/// parks on the input channel (no CPU when idle) and exits when the shell drops
/// a channel end.
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

/// Block for the next input frame, then collapse any already queued into it:
/// newest geometry wins but every frame's events are concatenated, so a slow
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

    #[test]
    fn recv_latest_returns_none_when_shell_disconnects() {
        let (tx, rx) = mpsc::channel::<FrameInput>();
        drop(tx);
        assert!(recv_latest(&rx).is_none());
    }
}
