use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use super::text_mesh::layout_text;
use super::{MAX_OBJECTS, MAX_TEXT_SURFACES, Scene3d};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

const fn vertex(position: [f32; 3], normal: [f32; 3]) -> Vertex {
    Vertex { position, normal }
}

const VERTICES: [Vertex; 24] = [
    vertex([-0.5, -0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, -0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([-0.5, 0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, -0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([-0.5, -0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([-0.5, 0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([0.5, 0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([0.5, -0.5, 0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, -0.5, -0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, 0.5, -0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, 0.5, 0.5], [1.0, 0.0, 0.0]),
    vertex([-0.5, -0.5, -0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, -0.5, 0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, 0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, 0.5], [0.0, 1.0, 0.0]),
    vertex([0.5, 0.5, 0.5], [0.0, 1.0, 0.0]),
    vertex([0.5, 0.5, -0.5], [0.0, 1.0, 0.0]),
    vertex([-0.5, 0.5, -0.5], [0.0, 1.0, 0.0]),
    vertex([-0.5, -0.5, -0.5], [0.0, -1.0, 0.0]),
    vertex([0.5, -0.5, -0.5], [0.0, -1.0, 0.0]),
    vertex([0.5, -0.5, 0.5], [0.0, -1.0, 0.0]),
    vertex([-0.5, -0.5, 0.5], [0.0, -1.0, 0.0]),
];
const INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18,
    16, 18, 19, 20, 21, 22, 20, 22, 23,
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// (curve_offset, curve_count, 0, 0) — this object's slice of the shared `glyph_curves`
    /// storage buffer. Zero count means no text.
    text_curves: [f32; 4],
    /// This object's shaped text, in em-space: (min.x, min.y, max.x, max.y).
    text_bounds: [f32; 4],
}

/// `Curve`'s exact layout for the GPU.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuCurve {
    p0: [f32; 2],
    control: [f32; 2],
    p1: [f32; 2],
}

/// Per-text-object curve budget in the shared `glyph_curves` buffer below. Longer shaped text is
/// silently clipped to this many curves — plenty for short cube-face labels; worth revisiting if
/// labels grow much past a couple of words.
const MAX_CURVES_PER_SURFACE: usize = 512;

/// Bundled directly here rather than reusing `text::MONO_FONT` (private to that module) — same
/// font JetBrains Mono renders everywhere. The only font 3D text supports; `TextSurface` has no
/// font-family prop yet.
const GLYPH_FONT: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

pub(crate) struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    glyph_curves: wgpu::Buffer,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d cube vertices"),
            contents: bytemuck::cast_slice(&VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d cube indices"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3d camera"),
            size: size_of::<Mat4>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3d instances"),
            size: (size_of::<Instance>() * MAX_OBJECTS) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_curves = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3d glyph curves"),
            size: (size_of::<GpuCurve>() * MAX_TEXT_SURFACES * MAX_CURVES_PER_SURFACE) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3d camera layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3d camera and glyph curves"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: glyph_curves.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3d pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let instance_attrs = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
            7 => Float32x4, 8 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3d pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance_attrs,
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            camera,
            camera_group,
            vertices,
            indices,
            instances,
            glyph_curves,
        }
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: &wgpu::TextureView,
        resolve: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        scene: &Scene3d,
        viewport: [u32; 4],
    ) {
        let [x, y, width, height] = viewport;
        if width == 0 || height == 0 || scene.objects.is_empty() {
            return;
        }
        let camera = &scene.camera;
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up.normalize());
        let projection = Mat4::perspective_rh(
            camera.fov_y_radians,
            width as f32 / height as f32,
            camera.near,
            camera.far,
        );
        queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&(projection * view)));
        let mut surface_slot = 0usize;
        let instances: Vec<_> = scene
            .objects
            .iter()
            .map(|object| {
                let (text_curves, text_bounds) = if let Some(text) = &object.surface {
                    let slot = surface_slot;
                    surface_slot += 1;
                    let mesh = layout_text(GLYPH_FONT, &text.text);
                    let count = mesh.curves.len().min(MAX_CURVES_PER_SURFACE);
                    let gpu_curves: Vec<GpuCurve> = mesh.curves[..count]
                        .iter()
                        .map(|c| GpuCurve {
                            p0: c.p0,
                            control: c.control,
                            p1: c.p1,
                        })
                        .collect();
                    let byte_offset = (slot * MAX_CURVES_PER_SURFACE * size_of::<GpuCurve>()) as u64;
                    queue.write_buffer(&self.glyph_curves, byte_offset, bytemuck::cast_slice(&gpu_curves));
                    let text_curves = [(slot * MAX_CURVES_PER_SURFACE) as f32, count as f32, 0.0, 0.0];
                    let text_bounds = [mesh.min[0], mesh.min[1], mesh.max[0], mesh.max[1]];
                    (text_curves, text_bounds)
                } else {
                    ([0.0; 4], [0.0; 4])
                };
                Instance {
                    model: Mat4::from_scale_rotation_translation(
                        object.scale,
                        object.rotation,
                        object.position,
                    )
                    .to_cols_array_2d(),
                    color: object.color,
                    text_curves,
                    text_bounds,
                }
            })
            .collect();
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("3d compositor"),
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d mesh pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: Some(resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, width, height);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..INDICES.len() as u32, 0, 0..instances.len() as u32);
        drop(pass);
        queue.submit([encoder.finish()]);
    }
}
