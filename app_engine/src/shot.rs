//! Off-screen screenshot: render one frame exactly as [`EngineApp::frame`] would paint it, but
//! into a headless wgpu texture instead of a window, and encode it as PNG. The viewport is the
//! caller's (a page-declaring app uses its page; anything else takes whatever size the caller
//! picks), so no shell window or open tab is involved at all.

use egui::{pos2, vec2, Rect};

use crate::pdf::install_fonts;
use crate::{EngineApp, FontBytes};

impl EngineApp {
    /// Render the app at `width`×`height` logical px (× `scale` px/pt) onto `clear` and return
    /// PNG bytes. `fonts` are the host's faces, installed so the shot shapes like the shell.
    pub fn screenshot(
        &mut self,
        width: f32,
        height: f32,
        scale: f32,
        clear: egui::Color32,
        fonts: FontBytes,
    ) -> Result<Vec<u8>, String> {
        install_fonts(&self.ctx, &fonts);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height));
        let input = || egui::RawInput { screen_rect: Some(rect), ..Default::default() };

        // Two frames: the first carries the rebuilt font atlas (and any first-frame settling),
        // the second is the steady image. Both deltas upload in order.
        let first = self.frame(input(), scale);
        let second = self.frame(input(), scale);
        let w = (width * scale).round() as u32;
        let h = (height * scale).round() as u32;
        rasterize(&[first.textures_delta, second.textures_delta], &second.primitives, w, h, scale, clear)
    }
}

/// Draw tessellated egui primitives on a one-shot headless wgpu device and read back as PNG.
fn rasterize(
    deltas: &[egui::TexturesDelta],
    primitives: &[egui::ClippedPrimitive],
    w: u32,
    h: u32,
    pixels_per_point: f32,
    clear: egui::Color32,
) -> Result<Vec<u8>, String> {
    let instance = wgpu::Instance::default(); // headless: no display handle
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| format!("no gpu adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("gpu device: {e}"))?;

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    let mut renderer = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
    for delta in deltas {
        for (id, image_delta) in &delta.set {
            renderer.update_texture(&device, &queue, *id, image_delta);
        }
    }

    let desc = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point };
    let mut encoder = device.create_command_encoder(&Default::default());
    let user_cmds = renderer.update_buffers(&device, &queue, &mut encoder, primitives, &desc);
    {
        // The clear is the "behind the app" colour; convert sRGB→linear for the sRGB target.
        let bg = egui::Rgba::from(clear);
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screenshot"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg.r() as f64,
                            g: bg.g() as f64,
                            b: bg.b() as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, primitives, &desc);
    }

    // Read back: rows pad to wgpu's 256-byte alignment, stripped below.
    let padded = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
        },
        extent,
    );
    queue.submit(user_cmds.into_iter().chain([encoder.finish()]));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("gpu poll: {e}"))?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("map readback: {e}"))?;

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        let mut chunk = data[start..start + (w * 4) as usize].to_vec();
        for px in chunk.chunks_exact_mut(4) {
            px[3] = 0xff; // composite is over an opaque clear — drop blend residue in alpha
        }
        pixels.extend_from_slice(&chunk);
    }
    drop(data);

    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&pixels).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}
