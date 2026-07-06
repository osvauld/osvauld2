//! Render — the per-window host runtime. Owns the winit window, the wgpu device/surface, the vello
//! renderer, and the text engine; drives one frame (reset scene → active screen builds it →
//! rasterize offscreen → blit to surface → present). Knows *how* to paint, not *what* — that's the
//! `Screen`.

use std::sync::Arc;
use std::time::Instant;

use vello::kurbo::Affine;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::window::Window;

use crate::text::TextEngine;

/// Supersample factor (vello renders into a target this many times larger than the surface, blit
/// downsamples). Left at 1 = native res: 2× linear-downsampled blurred edges more than it smoothed.
/// Kept as a tunable knob; real fix is vello's analytic (Area) AA once it's past alpha.
const SUPERSAMPLE: u32 = 1;

/// Per-window GPU state, created once the event loop is `resumed` (a surface needs a live window).
pub struct Render {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    scene: Scene,
    scale: f64,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    blitter: wgpu::util::TextureBlitter,
    /// Startup instant; `now` (seconds since) is the clock passed to screens for time-driven motion.
    start: Instant,
}

/// vello's compute output target: an Rgba8Unorm texture that's both storage-writable (vello) and
/// sampleable (the blit). Recreated on resize.
fn create_targets(
    width: u32,
    height: u32,
    device: &wgpu::Device,
) -> (wgpu::Texture, wgpu::TextureView) {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vello target"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        format: wgpu::TextureFormat::Rgba8Unorm,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    (target, view)
}

impl Render {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("shell2 device"),
                // vello's optional features, only if present — NOT raw `adapter.features()`, which
                // sweeps in experimental features that need a separate opt-in and fail to enable.
                required_features: adapter.features()
                    & (wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE),
                ..Default::default()
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        // 8-bit non-sRGB blit target so vello's already-gamma-encoded pixels pass through unaltered.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .expect("surface supports Rgba8Unorm or Bgra8Unorm");
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let renderer = Renderer::new(
            &device,
            RendererOptions {
                antialiasing_support: AaSupport::all(),
                ..Default::default()
            },
        )
        .expect("create vello renderer");

        let (target, target_view) = create_targets(
            config.width * SUPERSAMPLE,
            config.height * SUPERSAMPLE,
            &device,
        );
        // Nearest is exact at 1:1 (crisp). If SUPERSAMPLE > 1, switch to Linear to downsample.
        let blitter = wgpu::util::TextureBlitter::new(&device, format);
        let scale = window.scale_factor();
        eprintln!(
            "shell2: scale_factor = {scale}, surface = {}x{}",
            config.width, config.height
        );

        Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            scene: Scene::new(),
            scale,
            target,
            target_view,
            blitter,
            start: Instant::now(),
        }
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        self.window.set_ime_allowed(allowed);
    }

    pub fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        let (target, target_view) = create_targets(
            size.width * SUPERSAMPLE,
            size.height * SUPERSAMPLE,
            &self.device,
        );
        self.target = target;
        self.target_view = target_view;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Physical pixels per logical point — used to convert pointer events to logical coords.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Ask winit for the next frame — call after rendering to keep the loop alive.
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn set_cursor(&self, icon: winit::window::CursorIcon) {
        self.window.set_cursor(icon);
    }

    /// Draw one frame: clear to `clear`, let `build` populate the scene (it gets the scene, text
    /// engine, the logical→physical transform, the logical viewport, and the elapsed clock), then
    /// rasterize offscreen and present. Knows *how* to paint, not *what* — that's `build`.
    pub fn paint<F>(&mut self, clear: Color, text: &mut TextEngine, build: F)
    where
        F: FnOnce(&mut Scene, &mut TextEngine, Affine, (f32, f32), f64),
    {
        // wgpu 29 returns a status enum (not Result). Use the texture on success/suboptimal;
        // anything else means reconfigure and skip this frame (winit will send another).
        use wgpu::CurrentSurfaceTexture::*;
        let frame = match self.surface.get_current_texture() {
            Success(f) | Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.scene.reset();
        let t = Affine::scale(self.scale * SUPERSAMPLE as f64);
        // Viewport in logical points = physical / scale (supersample cancels out). Screens lay out
        // within this; `t` scales their logical coords to the physical target.
        let viewport = (
            (self.config.width as f64 / self.scale) as f32,
            (self.config.height as f64 / self.scale) as f32,
        );
        let now = self.start.elapsed().as_secs_f64();
        build(&mut self.scene, text, t, viewport, now);

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &self.target_view,
                &RenderParams {
                    base_color: clear,
                    width: self.config.width * SUPERSAMPLE,
                    height: self.config.height * SUPERSAMPLE,
                    // Area AA (analytic) has conflation jaggies at vello's alpha; MSAA16 is clean.
                    antialiasing_method: AaConfig::Msaa16,
                },
            )
            .expect("vello render");

        // Blit the compute-rendered target onto the (non-storage) surface texture, then present.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit to surface"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, &self.target_view, &view);
        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
