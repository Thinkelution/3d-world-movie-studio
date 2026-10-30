//! Our renderer.
//!
//! M0 renders headless (no window) into an offscreen target and reads the
//! pixels back, which is all a film needs. It draws lit meshes with a sun and
//! hemisphere ambient; shadows, sky and the real material system come in M1.

use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use glam::camera::rh as camera;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wms_core::Rgb;
use wms_geo::Mesh;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLE_COUNT: u32 = 4;
/// Uniform buffer offsets must be 256-byte aligned on every backend.
const OBJECT_STRIDE: u64 = 256;
pub const MAX_DRAW_ITEMS: usize = 2048;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no GPU adapter found ({0})")]
    NoAdapter(String),
    #[error("could not open the GPU device: {0}")]
    Device(String),
    #[error("frame has {0} draw items, the limit is {MAX_DRAW_ITEMS}")]
    TooManyItems(usize),
    #[error("reading the frame back from the GPU failed: {0}")]
    Readback(String),
}

#[derive(Debug, Clone, Copy)]
pub struct RendererOptions {
    pub width: u32,
    pub height: u32,
    /// Use the platform's software rasteriser (WARP, lavapipe) instead of a GPU.
    pub software: bool,
}

/// A mesh uploaded to the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshId(usize);

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Vertical field of view in degrees.
    pub fov_y: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Lighting {
    /// Direction towards the sun.
    pub sun_dir: Vec3,
    pub sun_color: Rgb,
    pub sky_color: Rgb,
    pub ground_color: Rgb,
}

#[derive(Debug, Clone, Copy)]
pub struct DrawItem {
    pub mesh: MeshId,
    /// Rotation, translation and uniform scale only.
    pub transform: Mat4,
    pub color: Rgb,
}

/// Everything needed to draw one frame.
#[derive(Debug, Clone)]
pub struct FrameDesc {
    pub camera: Camera,
    pub lighting: Lighting,
    pub background: Rgb,
    pub items: Vec<DrawItem>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalsUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniform {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    width: u32,
    height: u32,
    padded_row_bytes: u32,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    objects: wgpu::Buffer,
    objects_group: wgpu::BindGroup,
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    meshes: Vec<GpuMesh>,
}

impl Renderer {
    pub fn new(options: RendererOptions) -> Result<Self, RenderError> {
        pollster::block_on(Self::new_async(options))
    }

    async fn new_async(options: RendererOptions) -> Result<Self, RenderError> {
        let RendererOptions {
            width,
            height,
            software,
        } = options;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: software,
                compatible_surface: None,
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("wms"),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::Device(e.to_string()))?;

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<GlobalsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let objects = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("objects"),
            size: OBJECT_STRIDE * MAX_DRAW_ITEMS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_layout = |label, dynamic, size: u64| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: dynamic,
                        min_binding_size: wgpu::BufferSize::new(size),
                    },
                    count: None,
                }],
            })
        };
        let globals_layout = uniform_layout("globals", false, size_of::<GlobalsUniform>() as u64);
        let objects_layout = uniform_layout("objects", true, size_of::<ObjectUniform>() as u64);

        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let objects_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("objects"),
            layout: &objects_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &objects,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<ObjectUniform>() as u64),
                }),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("basic"),
            source: wgpu::ShaderSource::Wgsl(include_str!("basic.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("basic"),
            bind_group_layouts: &[Some(&globals_layout), Some(&objects_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("basic"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let target = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
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
        let msaa = target("msaa", COLOR_FORMAT, SAMPLE_COUNT, attachment);
        let depth = target("depth", DEPTH_FORMAT, SAMPLE_COUNT, attachment);
        let resolve = target(
            "resolve",
            COLOR_FORMAT,
            1,
            attachment | wgpu::TextureUsages::COPY_SRC,
        );

        let padded_row_bytes = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            adapter_info: adapter.get_info(),
            width,
            height,
            padded_row_bytes,
            pipeline,
            globals,
            globals_group,
            objects,
            objects_group,
            msaa_view: msaa.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            resolve_view: resolve.create_view(&Default::default()),
            resolve,
            readback,
            meshes: Vec::new(),
            device,
            queue,
        })
    }

    /// The GPU in use, e.g. "NVIDIA GeForce RTX 4070 (Dx12)".
    pub fn adapter_summary(&self) -> String {
        let info = &self.adapter_info;
        format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
    }

    pub fn upload_mesh(&mut self, mesh: &Mesh) -> MeshId {
        let vertices: Vec<Vertex> = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .map(|(&position, &normal)| Vertex { position, normal })
            .collect();
        let create = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        let gpu = GpuMesh {
            vertices: create(
                "vertices",
                bytemuck::cast_slice(&vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: create(
                "indices",
                bytemuck::cast_slice(&mesh.indices),
                wgpu::BufferUsages::INDEX,
            ),
            index_count: mesh.indices.len() as u32,
        };
        self.meshes.push(gpu);
        MeshId(self.meshes.len() - 1)
    }

    /// Renders one frame and returns tightly packed RGBA8 pixels (sRGB), top row first.
    pub fn render(&mut self, frame: &FrameDesc) -> Result<Vec<u8>, RenderError> {
        if frame.items.len() > MAX_DRAW_ITEMS {
            return Err(RenderError::TooManyItems(frame.items.len()));
        }
        self.write_uniforms(frame);

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let bg = frame.background;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&self.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(bg.r),
                            g: f64::from(bg.g),
                            b: f64::from(bg.b),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_group, &[]);
            for (i, item) in frame.items.iter().enumerate() {
                let mesh = &self.meshes[item.mesh.0];
                pass.set_bind_group(1, &self.objects_group, &[(i as u64 * OBJECT_STRIDE) as u32]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row_bytes),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.read_pixels()
    }

    fn write_uniforms(&self, frame: &FrameDesc) {
        let cam = frame.camera;
        let aspect = self.width as f32 / self.height as f32;
        let view = camera::view::look_at_mat4(cam.eye, cam.target, Vec3::Y);
        // WebGPU clip space matches DirectX: depth 0..1, Y up.
        let proj = camera::proj::directx::perspective(cam.fov_y.to_radians(), aspect, 0.1, 200.0);
        let light = frame.lighting;
        let rgb = |c: Rgb| [c.r, c.g, c.b, 1.0];
        let globals = GlobalsUniform {
            view_proj: (proj * view).to_cols_array_2d(),
            eye: cam.eye.extend(1.0).to_array(),
            sun_dir: light.sun_dir.normalize().extend(0.0).to_array(),
            sun_color: rgb(light.sun_color),
            sky_color: rgb(light.sky_color),
            ground_color: rgb(light.ground_color),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        if frame.items.is_empty() {
            return;
        }
        let mut staging = vec![0u8; frame.items.len() * OBJECT_STRIDE as usize];
        for (chunk, item) in staging
            .chunks_exact_mut(OBJECT_STRIDE as usize)
            .zip(&frame.items)
        {
            let object = ObjectUniform {
                model: item.transform.to_cols_array_2d(),
                color: rgb(item.color),
            };
            chunk[..size_of::<ObjectUniform>()].copy_from_slice(bytemuck::bytes_of(&object));
        }
        self.queue.write_buffer(&self.objects, 0, &staging);
    }

    fn read_pixels(&self) -> Result<Vec<u8>, RenderError> {
        let (tx, rx) = mpsc::channel();
        self.readback
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Readback(e.to_string()))?
            .map_err(|e| RenderError::Readback(e.to_string()))?;

        let row_bytes = (self.width * 4) as usize;
        let mut pixels = Vec::with_capacity(row_bytes * self.height as usize);
        {
            let mapped = self
                .readback
                .get_mapped_range(..)
                .map_err(|e| RenderError::Readback(e.to_string()))?;
            for row in mapped.chunks_exact(self.padded_row_bytes as usize) {
                pixels.extend_from_slice(&row[..row_bytes]);
            }
        }
        self.readback.unmap();
        Ok(pixels)
    }
}
