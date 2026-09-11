//! Render — the per-window host runtime. Owns the winit window, the wgpu device/surface, the vello
//! renderer, and the text engine; drives one frame (reset scene → active screen builds it →
//! rasterize offscreen → blit to surface → present). Knows *how* to paint, not *what* — that's the
//! `Screen`.

use std::sync::Arc;

use vello::kurbo::Affine;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::window::Window;

use wgpu::CurrentSurfaceTexture::*;

/// Supersample factor (vello renders into a target this many times larger than the surface, blit
/// downsamples). Left at 1 = native res: 2× linear-downsampled blurred edges more than it smoothed.
/// Kept as a tunable knob; real fix is vello's analytic (Area) AA once it's past alpha.
const SUPERSAMPLE: u32 = 1;
const MAX_CAPTURE_PIXELS: u64 = 16 * 1024 * 1024;

/// Per-window GPU state, created once the event loop is `resumed` (a surface needs a live window).
/// PNG read back from the live Vello target. Dimensions are physical pixels.
#[derive(Clone)]
pub struct CapturedImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct Render {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    scale: f64,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
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
            | wgpu::TextureUsages::COPY_SRC,
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
            scale,
            target,
            target_view,
            blitter,
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
    pub fn present(&mut self, clear: Color, scene: &Scene) -> bool {
        // Acquire before submitting Vello work. Some backends can stall acquiring a surface after
        // offscreen work is already queued; this was the original and reliable frame ordering.
        let frame = match self.surface.get_current_texture() {
            Success(f) | Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
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
        true
    }

    /// Read the most recently rendered live target and encode it as PNG. Queue submissions
    /// are ordered, so a copy submitted after [`Self::present`] observes that frame even if
    /// its GPU work was still in flight.
    pub fn capture(&self) -> Result<CapturedImage, String> {
        self.read_texture(
            &self.target,
            self.config.width * SUPERSAMPLE,
            self.config.height * SUPERSAMPLE,
        )
    }

    /// Render an already-built scene to a temporary target. This reuses the live renderer and
    /// device but never acquires or presents a surface frame.
    pub fn capture_scene(
        &mut self,
        clear: Color,
        scene: &Scene,
        viewport: (f32, f32),
        scale: f32,
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
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &view,
                &RenderParams {
                    base_color: clear,
                    width,
                    height,
                    antialiasing_method: AaConfig::Msaa16,
                },
            )
            .map_err(|e| format!("vello render: {e}"))?;
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
