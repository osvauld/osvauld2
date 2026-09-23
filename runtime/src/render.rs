//! Render — the per-window host runtime. Owns the wgpu device, the vello renderer, and (when there
//! is a window) its surface; drives one frame (reset scene → active screen builds it → rasterize
//! offscreen → blit to surface → present). Knows *how* to paint, not *what* — that's the `Screen`.
//!
//! The window is optional. Rasterizing was never the part that needed one — only presenting is —
//! so an offscreen `Render` runs the same device, renderer and target, and differs in exactly one
//! observable way: [`Render::present`] returns false because there is nowhere to present to.

use std::sync::Arc;

use crate::scene3d::{Scene3d, SceneRenderer};
use vello::kurbo::{Affine, Rect};
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::window::Window;

use wgpu::CurrentSurfaceTexture::*;

/// Supersample factor (vello renders into a target this many times larger than the surface, blit
/// downsamples). Left at 1 = native res: 2× linear-downsampled blurred edges more than it smoothed.
/// Kept as a tunable knob; real fix is vello's analytic (Area) AA once it's past alpha.
const SUPERSAMPLE: u32 = 1;
const SCENE_SAMPLES: u32 = 4;
const MAX_CAPTURE_PIXELS: u64 = 16 * 1024 * 1024;

/// PNG read back from the live Vello target. Dimensions are physical pixels.
#[derive(Clone)]
pub struct CapturedImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A window and the surface drawn to it. One `Option` rather than two because the surface borrows
/// its window for `'static`: neither can be present without the other.
struct Presenter {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
}

pub struct Render {
    /// `None` offscreen. The device, renderer and target are still real there — what is missing is
    /// only somewhere to put the finished frame.
    presenter: Option<Presenter>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    scale: f64,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    scene_targets: SceneTargets,
    scene_renderer: SceneRenderer,
    scene_blitter: wgpu::util::TextureBlitter,
    blitter: wgpu::util::TextureBlitter,
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
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8Unorm,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    (target, view)
}

/// The adapter and device, with or without a surface to be compatible with. Shared by both
/// constructors so the offscreen path cannot quietly drift into a different feature set than the
/// windowed one — the whole value of offscreen mode is that it is the same renderer.
async fn open_device(
    instance: &wgpu::Instance,
    compatible: Option<&wgpu::Surface<'static>>,
) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: compatible,
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
    (adapter, device, queue)
}

fn surface_config(
    format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}

struct SceneTargets {
    color: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
}

fn create_scene_targets(width: u32, height: u32, device: &wgpu::Device) -> SceneTargets {
    let texture = |label, format, samples, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
    SceneTargets {
        color: texture(
            "3d msaa color",
            wgpu::TextureFormat::Rgba8Unorm,
            SCENE_SAMPLES,
            attachment,
        )
        .create_view(&Default::default()),
        resolve: texture(
            "3d resolved color",
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default()),
        depth: texture(
            "3d msaa depth",
            wgpu::TextureFormat::Depth32Float,
            SCENE_SAMPLES,
            attachment,
        )
        .create_view(&Default::default()),
    }
}

fn render_vello(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    target: &wgpu::TextureView,
    clear: Color,
    width: u32,
    height: u32,
) -> Result<(), String> {
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            target,
            &RenderParams {
                base_color: clear,
                width,
                height,
                // Area AA (analytic) has conflation jaggies at vello's alpha; MSAA16 is clean.
                antialiasing_method: AaConfig::Msaa16,
            },
        )
        .map_err(|e| format!("vello render: {e}"))
}

pub(crate) struct SceneView3d {
    pub scene: Arc<Scene3d>,
    pub rect: Rect,
}

fn physical_viewport(rect: Rect, scale: f32, width: u32, height: u32) -> [u32; 4] {
    let x0 = (rect.x0 * scale as f64).floor().clamp(0.0, width as f64) as u32;
    let y0 = (rect.y0 * scale as f64).floor().clamp(0.0, height as f64) as u32;
    let x1 = (rect.x1 * scale as f64)
        .ceil()
        .clamp(x0 as f64, width as f64) as u32;
    let y1 = (rect.y1 * scale as f64)
        .ceil()
        .clamp(y0 as f64, height as f64) as u32;
    [x0, y0, x1 - x0, y1 - y0]
}

impl Render {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let (adapter, device, queue) = open_device(&instance, Some(&surface)).await;

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
        let config = surface_config(format, caps.alpha_modes[0], size.width, size.height);
        surface.configure(&device, &config);

        let scale = window.scale_factor();
        eprintln!(
            "shell2: scale_factor = {scale}, surface = {}x{}",
            config.width, config.height
        );
        Self::assemble(
            Some(Presenter { window, surface }),
            device,
            queue,
            config,
            scale,
        )
    }

    /// The same device and renderer with nowhere to present. `width` and `height` are physical
    /// pixels, which offscreen are also logical points because the scale is 1.0 — so the numbers a
    /// driver passes to a pointer method are the ones an element's rect is measured in. Use
    /// [`Self::set_scale`] to test a hidpi layout.
    ///
    /// Frames still rasterize, so [`Self::capture_scene`] reads back real pixels.
    pub async fn offscreen(width: u32, height: u32) -> Self {
        let instance = wgpu::Instance::default();
        let (_, device, queue) = open_device(&instance, None).await;
        // With no surface the format is chosen rather than negotiated. Rgba8Unorm is what the vello
        // target already is, and nothing blits offscreen, so there is nothing left to disagree.
        let config = surface_config(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::CompositeAlphaMode::Auto,
            width,
            height,
        );
        Self::assemble(None, device, queue, config, 1.0)
    }

    /// Everything downstream of the device: the renderer, the vello target, and the blit pipeline.
    /// Identical either way — a frame is built the same with and without a window.
    fn assemble(
        presenter: Option<Presenter>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        scale: f64,
    ) -> Self {
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                antialiasing_support: AaSupport::all(),
                ..Default::default()
            },
        )
        .expect("create vello renderer");
        let target_width = config.width * SUPERSAMPLE;
        let target_height = config.height * SUPERSAMPLE;
        let (target, target_view) = create_targets(target_width, target_height, &device);
        let scene_targets = create_scene_targets(target_width, target_height, &device);
        let scene_renderer = SceneRenderer::new(&device);
        let scene_blitter =
            wgpu::util::TextureBlitterBuilder::new(&device, wgpu::TextureFormat::Rgba8Unorm)
                .blend_state(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                .build();
        // Nearest is exact at 1:1 (crisp). If SUPERSAMPLE > 1, switch to Linear to downsample.
        let blitter = wgpu::util::TextureBlitter::new(&device, config.format);
        Self {
            presenter,
            device,
            queue,
            config,
            renderer,
            scale,
            target,
            target_view,
            scene_targets,
            scene_renderer,
            scene_blitter,
            blitter,
        }
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        if let Some(p) = &self.presenter {
            p.window.set_ime_allowed(allowed);
        }
    }

    pub fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        if let Some(p) = &self.presenter {
            p.surface.configure(&self.device, &self.config);
        }
        let (target, target_view) = create_targets(
            size.width * SUPERSAMPLE,
            size.height * SUPERSAMPLE,
            &self.device,
        );
        self.target = target;
        self.target_view = target_view;
        self.scene_targets = create_scene_targets(
            size.width * SUPERSAMPLE,
            size.height * SUPERSAMPLE,
            &self.device,
        );
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Physical pixels per logical point — used to convert pointer events to logical coords.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Ask winit for the next frame — call after rendering to keep the loop alive. Offscreen this
    /// is deliberately nothing: no window means no `RedrawRequested`, so frames happen when the
    /// driver says and an animating app cannot free-run.
    pub fn request_redraw(&self) {
        if let Some(p) = &self.presenter {
            p.window.request_redraw();
        }
    }

    pub fn set_cursor(&self, icon: winit::window::CursorIcon) {
        if let Some(p) = &self.presenter {
            p.window.set_cursor(icon);
        }
    }

    pub fn viewport(&self) -> (f32, f32) {
        (
            (self.config.width as f64 / self.scale) as f32,
            (self.config.height as f64 / self.scale) as f32,
        )
    }

    pub fn transform(&self) -> Affine {
        Affine::scale(self.scale * SUPERSAMPLE as f64)
    }

    /// Draw one frame: clear to `clear`, let `build` populate the scene (it gets the scene, text
    /// engine, the logical→physical transform, the logical viewport, and the elapsed clock), then
    /// rasterize offscreen and present. Knows *how* to paint, not *what* — that's `build`.
    pub(crate) fn present(
        &mut self,
        clear: Color,
        scene: &Scene,
        scene3d: Option<&SceneView3d>,
    ) -> bool {
        // Acquire before submitting Vello work. Some backends can stall acquiring a surface after
        // offscreen work is already queued; this was the original and reliable frame ordering.
        // Scoped so the surface borrow ends before the renderer is taken mutably below.
        let frame = {
            // Offscreen this is the whole difference. Reported as a failed present rather than
            // hidden, because the caller's fallback for one — capture from the scene instead of
            // from the live target — is exactly what an offscreen screenshot needs.
            let Some(presenter) = &self.presenter else {
                return false;
            };
            match presenter.surface.get_current_texture() {
                Success(f) | Suboptimal(f) => f,
                _ => {
                    presenter.surface.configure(&self.device, &self.config);
                    return false;
                }
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        render_vello(
            &mut self.renderer,
            &self.device,
            &self.queue,
            scene,
            &self.target_view,
            clear,
            self.config.width * SUPERSAMPLE,
            self.config.height * SUPERSAMPLE,
        )
        .expect("vello render");
        if let Some(view) = scene3d {
            self.scene_renderer.render(
                &self.device,
                &self.queue,
                &self.scene_targets.color,
                &self.scene_targets.resolve,
                &self.scene_targets.depth,
                &view.scene,
                physical_viewport(
                    view.rect,
                    (self.scale * SUPERSAMPLE as f64) as f32,
                    self.config.width * SUPERSAMPLE,
                    self.config.height * SUPERSAMPLE,
                ),
            );
        }

        // Blit the compute-rendered target onto the (non-storage) surface texture, then present.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit to surface"),
            });
        if scene3d.is_some() {
            self.scene_blitter.copy(
                &self.device,
                &mut encoder,
                &self.scene_targets.resolve,
                &self.target_view,
            );
        }
        self.blitter
            .copy(&self.device, &mut encoder, &self.target_view, &view);
        self.queue.submit([encoder.finish()]);
        frame.present();
        true
    }

    /// Read the most recently rendered live target and encode it as PNG. Queue submissions
    /// are ordered, so a copy submitted after [`Self::present`] observes that frame even if
    /// its GPU work was still in flight.
    pub fn capture(&self) -> Result<CapturedImage, String> {
        if self.presenter.is_none() {
            // Nothing is ever presented into the live target offscreen, so reading it back would
            // hand out an uninitialized texture. `capture_scene` is the offscreen path, and a
            // false `present` already routes callers there.
            return Err("no live frame offscreen: capture the scene instead".into());
        }
        self.read_texture(
            &self.target,
            self.config.width * SUPERSAMPLE,
            self.config.height * SUPERSAMPLE,
        )
    }

    /// Render an already-built scene to a temporary target. This reuses the live renderer and
    /// device but never acquires or presents a surface frame.
    pub(crate) fn capture_scene(
        &mut self,
        clear: Color,
        scene: &Scene,
        viewport: (f32, f32),
        scale: f32,
        scene3d: Option<&SceneView3d>,
    ) -> Result<CapturedImage, String> {
        let dimension = |logical: f32| -> Result<u32, String> {
            let px = (logical * scale).round();
            if !px.is_finite() || px < 1.0 || px > 8192.0 {
                return Err(format!("screenshot dimension is out of range: {px}"));
            }
            Ok(px as u32)
        };
        let width = dimension(viewport.0)?;
        let height = dimension(viewport.1)?;
        if u64::from(width) * u64::from(height) > MAX_CAPTURE_PIXELS {
            return Err("screenshot exceeds the 16 megapixel limit".into());
        }
        let (target, view) = create_targets(width, height, &self.device);
        let scene_targets = create_scene_targets(width, height, &self.device);
        render_vello(
            &mut self.renderer,
            &self.device,
            &self.queue,
            scene,
            &view,
            clear,
            width,
            height,
        )?;
        if let Some(scene_view) = scene3d {
            self.scene_renderer.render(
                &self.device,
                &self.queue,
                &scene_targets.color,
                &scene_targets.resolve,
                &scene_targets.depth,
                &scene_view.scene,
                physical_viewport(scene_view.rect, scale, width, height),
            );
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("3d capture composite"),
                });
            self.scene_blitter
                .copy(&self.device, &mut encoder, &scene_targets.resolve, &view);
            self.queue.submit([encoder.finish()]);
        }
        self.read_texture(&target, width, height)
    }

    /// Rows are padded for wgpu, then packed for PNG.
    fn read_texture(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<CapturedImage, String> {
        let row = width.checked_mul(4).ok_or("screenshot row is too wide")?;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("screenshot copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("gpu poll: {e}"))?;
        rx.recv()
            .map_err(|e| format!("map callback: {e}"))?
            .map_err(|e| format!("map readback: {e}"))?;

        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((u64::from(row) * u64::from(height)) as usize);
        for y in 0..height {
            let start = (y * padded) as usize;
            rgba.extend_from_slice(&mapped[start..start + row as usize]);
        }
        drop(mapped);
        buffer.unmap();

        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
        Ok(CapturedImage { png, width, height })
    }
}
