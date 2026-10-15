//! The GPU side: pipelines, textures, the map's meshes, and each frame's sprites and
//! effects. The scene is drawn into its own colour + depth target (with the sun's shadow
//! map first), then blitted into whatever pass the host gives (egui's, in the game).

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};
use wgpu::util::DeviceExt;

use crate::map::{build_meshes, MapScene, MapVertex, Spot};

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADOW_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    ambient: [f32; 4],
    hemi_sky: [f32; 4],
    hemi_ground: [f32; 4],
    fog_color: [f32; 4],
    params: [f32; 4],
    camera: [f32; 4],
    world: [f32; 4],
    editor: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// The light of a frame, in linear colours.
#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    /// towards the sun
    pub sun_dir: Vec3,
    pub sun: Vec3,
    pub ambient: Vec3,
    pub hemi_sky: Vec3,
    pub hemi_ground: Vec3,
    pub sky: Vec3,
    pub fog_near: f32,
    pub fog_far: f32,
    pub light_view_proj: Mat4,
}

/// An upright picture: a creature, a person. Turns with the camera around the vertical axis.
#[derive(Clone, Debug)]
pub struct Billboard {
    pub texture: String,
    /// its feet, in the world
    pub pos: Vec3,
    /// width and height in world units
    pub size: Vec2,
    /// x: sideways shift (in the picture's plane), y: lift off the ground
    pub offset: Vec2,
    /// u0, v0, u1, v1
    pub uv: [f32; 4],
    /// multiplies the texture; over 1 flashes it white
    pub color: [f32; 4],
    pub casts_shadow: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeKind {
    /// a soft disc on the ground (r, squash along z)
    Disc { radius: f32, squash: f32 },
    /// a flat ring on the ground
    Ring { inner: f32, outer: f32 },
    /// a flat rectangle (x by z), centred
    Rect { width: f32, depth: f32 },
    /// any four corners, from `center` (two of them the same for a triangle)
    Quad { corners: [Vec3; 4] },
}

#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub kind: ShapeKind,
    pub center: Vec3,
    pub color: [f32; 4],
    pub additive: bool,
}

/// A glowing orb facing the camera (projectiles, particles), drawn additively.
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub pos: Vec3,
    pub size: f32,
    pub color: [f32; 4],
}

/// Everything the game wants drawn this frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub view_proj: Mat4,
    pub camera_pos: Vec3,
    pub light: Lighting,
    pub shadows: bool,
    pub billboards: Vec<Billboard>,
    pub shapes: Vec<Shape>,
    pub glows: Vec<Glow>,
    /// rain streaks: (top, bottom) points
    pub streaks: Vec<(Vec3, Vec3)>,
    /// seconds, for things that move by themselves (water)
    pub time: f32,
    /// grid lines on the world's ground (the map editor)
    pub grid: bool,
}

/// A world map's ground, as `world::mesh::ground` makes it.
pub struct WorldGround<'a> {
    pub tops: (&'a [MapVertex], &'a [u32]),
    pub solid: (&'a [MapVertex], &'a [u32]),
    /// width x height texels: terrain index, level, 0, 255
    pub cells: &'a [u8],
    pub size: [u32; 2],
    /// terrains x 3 rows, see the shader
    pub palette: &'a [u8],
    pub palette_width: u32,
    pub level_height: f32,
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

impl GpuMesh {
    fn new(device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) -> Option<GpuMesh> {
        if indices.is_empty() {
            return None;
        }
        Some(GpuMesh {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: indices.len() as u32,
        })
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}

/// The world map on the GPU: cell tops (terrain shader), cliffs, props and trees.
struct WorldGpu {
    tops: Option<GpuMesh>,
    solid: Option<GpuMesh>,
    objects: Option<GpuMesh>,
    /// things that move (the walker, creatures): small, replaced often
    extra: Option<GpuMesh>,
    cells: wgpu::Texture,
    size: [u32; 2],
    bind: wgpu::BindGroup,
    level_height: f32,
}

struct Texture {
    bind: wgpu::BindGroup,
    size: (u32, u32),
}

struct Mesh {
    sheet: String,
    cutout: bool,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

struct Target {
    size: [u32; 2],
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
}

struct DrawCall {
    texture: String,
    first: u32,
    count: u32,
}

pub struct SceneRenderer {
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    shadow_view: wgpu::TextureView,
    shadow_bind: wgpu::BindGroup,
    map_pipeline: wgpu::RenderPipeline,
    map_shadow_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    sprite_shadow_pipeline: wgpu::RenderPipeline,
    blend_pipeline: wgpu::RenderPipeline,
    add_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    ground_layout: wgpu::BindGroupLayout,
    world: Option<WorldGpu>,
    textures: HashMap<String, Texture>,
    meshes: Vec<Mesh>,
    pub map_name: Option<String>,
    pub spot: Option<Spot>,
    target: Option<Target>,
    sprite_buffer: Option<(wgpu::Buffer, u64)>,
    sprite_draws: Vec<DrawCall>,
    blend_draws: Vec<DrawCall>,
    add_draws: Vec<DrawCall>,
    sky: [f32; 3],
    output_srgb: bool,
}

pub const WHITE: &str = "#white";
pub const GLOW: &str = "#glow";

impl SceneRenderer {
    /// `output` is the format of the pass the scene is blitted into.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, output: wgpu::TextureFormat) -> SceneRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&Default::default());
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
            ],
        });

        let data_texture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let ground_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ground"),
            entries: &[data_texture(2), data_texture(3)],
        });
        let ground_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ground"),
            bind_group_layouts: &[Some(&globals_layout), Some(&ground_layout), Some(&shadow_layout)],
            immediate_size: 0,
        });
        let main_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("main"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout), Some(&shadow_layout)],
            immediate_size: 0,
        });
        let shadow_pass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pass"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let map_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
        let sprite_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
        let map_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MapVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &map_attrs,
        })];
        let sprite_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &sprite_attrs,
        })];
        let depth = |write: bool| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let shadow_depth = wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
        };
        let primitive = wgpu::PrimitiveState { cull_mode: None, ..Default::default() };
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        target: Option<(wgpu::TextureFormat, Option<wgpu::BlendState>)>,
                        depth_state: Option<wgpu::DepthStencilState>| {
            let targets = target.map(|(format, blend)| [Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })]);
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), buffers, compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: match &targets {
                        Some(t) => t,
                        None => &[],
                    },
                    compilation_options: Default::default(),
                }),
                primitive,
                depth_stencil: depth_state,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let map_pipeline = pipeline("map", &main_layout, "vs_map", "fs_map", &map_buffers, Some((COLOR, None)), Some(depth(true)));
        let map_shadow_pipeline = pipeline("map shadow", &shadow_pass_layout, "vs_map_shadow", "fs_map_shadow", &map_buffers, None, Some(shadow_depth.clone()));
        let sprite_pipeline = pipeline("sprites", &main_layout, "vs_sprite", "fs_sprite_cutout", &sprite_buffers, Some((COLOR, None)), Some(depth(true)));
        let sprite_shadow_pipeline = pipeline("sprite shadow", &shadow_pass_layout, "vs_sprite_shadow", "fs_sprite_shadow", &sprite_buffers, None, Some(shadow_depth));
        let blend_pipeline = pipeline("blend", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, Some((COLOR, Some(wgpu::BlendState::ALPHA_BLENDING))), Some(depth(false)));
        let add_pipeline = pipeline("additive", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, Some((COLOR, Some(additive))), Some(depth(false)));
        let blit_pipeline = pipeline("blit", &shadow_pass_layout, "vs_blit", "fs_blit", &[], Some((output, None)), None);
        let ground_pipeline = pipeline("ground", &ground_pipeline_layout, "vs_map", "fs_ground", &map_buffers, Some((COLOR, None)), Some(depth(true)));

        let mut r = SceneRenderer {
            globals, globals_bind, texture_layout, nearest, linear, shadow_view, shadow_bind,
            map_pipeline, map_shadow_pipeline, sprite_pipeline, sprite_shadow_pipeline, blend_pipeline, add_pipeline, blit_pipeline,
            ground_pipeline, ground_layout, world: None,
            textures: HashMap::new(), meshes: vec![], map_name: None, spot: None, target: None, sprite_buffer: None,
            sprite_draws: vec![], blend_draws: vec![], add_draws: vec![], sky: [0.1, 0.1, 0.1],
            output_srgb: output.is_srgb(),
        };
        r.add_texture(device, queue, WHITE, &[255, 255, 255, 255], 1, 1, false);
        let mut glow = vec![0u8; 64 * 64 * 4];
        for y in 0..64 {
            for x in 0..64 {
                let d = (((x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2)).sqrt() / 32.0).min(1.0);
                // white, opaque in the middle, gone at the edge
                let a = if d < 0.35 { 1.0 - d / 0.35 * 0.2 } else { 0.8 * (1.0 - (d - 0.35) / 0.65) };
                let i = (y * 64 + x) * 4;
                glow[i..i + 4].copy_from_slice(&[255, 255, 255, (a.clamp(0.0, 1.0) * 255.0) as u8]);
            }
        }
        r.add_texture(device, queue, GLOW, &glow, 64, 64, true);
        r.warm_up(device, queue);
        r
    }

    /// Draws a degenerate triangle with every pipeline once, offscreen, so that browsers
    /// which compile shaders on first use do it now and not in the middle of a battle.
    fn warm_up(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let target = self.make_target(device, [4, 4]);
        let ground = self.data_bind(device, queue, &[0, 0, 0, 255], [1, 1], &[255; 12], 1);
        let zeros = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("warm-up"),
            contents: &[0u8; 3 * 64],
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("warm-up shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_vertex_buffer(0, zeros.slice(..));
            for p in [&self.map_shadow_pipeline, &self.sprite_shadow_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("warm-up"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]);
            pass.set_vertex_buffer(0, zeros.slice(..));
            for p in [&self.map_pipeline, &self.sprite_pipeline, &self.blend_pipeline, &self.add_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
            pass.set_bind_group(1, &ground.1, &[]);
            pass.set_pipeline(&self.ground_pipeline);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }

    pub fn has_texture(&self, key: &str) -> bool {
        self.textures.contains_key(key)
    }

    pub fn texture_size(&self, key: &str) -> Option<(u32, u32)> {
        self.textures.get(key).map(|t| t.size)
    }

    /// RGBA8 pixels, sRGB. `smooth`: linear filtering (else nearest, for pixel art).
    #[allow(clippy::too_many_arguments)]
    pub fn add_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: &str, rgba: &[u8], width: u32, height: u32, smooth: bool) {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(key),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(key),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if smooth { &self.linear } else { &self.nearest }) },
            ],
        });
        self.textures.insert(key.to_string(), Texture { bind, size: (width, height) });
    }

    fn data_texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, rgba: &[u8], size: [u32; 2]) -> wgpu::Texture {
        device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        )
    }

    /// The cells' and the palette's textures, bound for the ground shader.
    fn data_bind(&self, device: &wgpu::Device, queue: &wgpu::Queue, cells: &[u8], size: [u32; 2], palette: &[u8], palette_width: u32) -> (wgpu::Texture, wgpu::BindGroup) {
        let cells = Self::data_texture(device, queue, "cells", cells, size);
        let palette = Self::data_texture(device, queue, "palette", palette, [palette_width.max(1), 3]);
        let (cv, pv) = (cells.create_view(&Default::default()), palette.create_view(&Default::default()));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground"),
            layout: &self.ground_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&cv) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&pv) },
            ],
        });
        (cells, bind)
    }

    /// Shows a world map's ground (replacing the one before; props and trees stay until
    /// `set_world_objects`). Battle maps (`load_map`) and the world draw together; clear
    /// the one you don't want.
    pub fn set_world_ground(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, g: &WorldGround) {
        let (cells, bind) = self.data_bind(device, queue, g.cells, g.size, g.palette, g.palette_width);
        let (objects, extra) = match self.world.take() {
            Some(w) => (w.objects, w.extra),
            None => (None, None),
        };
        self.world = Some(WorldGpu {
            tops: GpuMesh::new(device, g.tops.0, g.tops.1),
            solid: GpuMesh::new(device, g.solid.0, g.solid.1),
            objects,
            extra,
            cells,
            size: g.size,
            bind,
            level_height: g.level_height,
        });
    }

    /// Repaints the cells only (same size): what painting terrain changes.
    pub fn set_world_cells(&mut self, queue: &wgpu::Queue, cells: &[u8]) {
        let Some(w) = &self.world else { return };
        if cells.len() as u32 != w.size[0] * w.size[1] * 4 {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &w.cells, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            cells,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w.size[0] * 4), rows_per_image: Some(w.size[1]) },
            wgpu::Extent3d { width: w.size[0], height: w.size[1], depth_or_array_layers: 1 },
        );
    }

    /// The props and trees (after `set_world_ground`).
    pub fn set_world_objects(&mut self, device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) {
        if let Some(w) = &mut self.world {
            w.objects = GpuMesh::new(device, vertices, indices);
        }
    }

    /// What moves on the world map (after `set_world_ground`); empty clears it.
    pub fn set_world_extra(&mut self, device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) {
        if let Some(w) = &mut self.world {
            w.extra = GpuMesh::new(device, vertices, indices);
        }
    }

    pub fn clear_world(&mut self) {
        self.world = None;
    }

    /// Forgets the battle map.
    pub fn clear_map(&mut self) {
        self.meshes.clear();
        self.map_name = None;
        self.spot = None;
    }

    /// The map's sheets must already be textures, named "<map>/<sheet>".
    pub fn load_map(&mut self, device: &wgpu::Device, scene: &MapScene, spot: Spot) {
        let sizes: HashMap<String, (u32, u32)> = scene
            .sheets
            .keys()
            .filter_map(|k| self.texture_size(&format!("{}/{}", scene.map, k)).map(|s| (k.clone(), s)))
            .collect();
        self.meshes = build_meshes(scene, &sizes, spot)
            .into_iter()
            .filter(|m| !m.indices.is_empty())
            .map(|m| Mesh {
                sheet: format!("{}/{}", scene.map, m.sheet),
                cutout: m.cutout,
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("map vertices"),
                    contents: bytemuck::cast_slice(&m.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("map indices"),
                    contents: bytemuck::cast_slice(&m.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: m.indices.len() as u32,
            })
            .collect();
        self.map_name = Some(scene.map.clone());
        self.spot = Some(spot);
    }

    /// Uploads the frame and records the shadow and scene passes into `encoder`.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, frame: &Frame, size: [u32; 2]) {
        let size = [size[0].max(1), size[1].max(1)];
        if self.target.as_ref().map(|t| t.size != size).unwrap_or(true) {
            self.target = Some(self.make_target(device, size));
        }
        let l = &frame.light;
        self.sky = l.sky.to_array();
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_view_proj: l.light_view_proj.to_cols_array_2d(),
            sun_dir: l.sun_dir.normalize().extend(0.0).to_array(),
            sun_color: l.sun.extend(0.0).to_array(),
            ambient: l.ambient.extend(0.0).to_array(),
            hemi_sky: l.hemi_sky.extend(0.0).to_array(),
            hemi_ground: l.hemi_ground.extend(0.0).to_array(),
            fog_color: [l.sky.x, l.sky.y, l.sky.z, l.fog_near],
            params: [l.fog_far, frame.shadows as u32 as f32, (!self.output_srgb) as u32 as f32, 0.0],
            camera: frame.camera_pos.extend(1.0).to_array(),
            world: match &self.world {
                Some(w) => [w.size[0] as f32, w.size[1] as f32, w.level_height, frame.time],
                None => [0.0, 0.0, 2.0, frame.time],
            },
            editor: [frame.grid as u32 as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        // this frame's sprites, shapes and glows: one vertex buffer, drawn by texture
        let mut vertices: Vec<SpriteVertex> = vec![];
        self.sprite_draws.clear();
        self.blend_draws.clear();
        self.add_draws.clear();
        let cam = frame.camera_pos;
        let mut sorted: Vec<&Billboard> = frame.billboards.iter().filter(|b| self.textures.contains_key(&b.texture)).collect();
        sorted.sort_by(|a, b| a.texture.cmp(&b.texture));
        for b in sorted {
            let first = vertices.len() as u32;
            let to_cam = Vec3::new(cam.x - b.pos.x, 0.0, cam.z - b.pos.z).normalize_or(Vec3::Z);
            let right = Vec3::Y.cross(to_cam).normalize();
            let x0 = b.offset.x - b.size.x / 2.0;
            let x1 = b.offset.x + b.size.x / 2.0;
            let y0 = b.offset.y;
            let y1 = b.offset.y + b.size.y;
            let p = |x: f32, y: f32| (b.pos + right * x + Vec3::Y * y).to_array();
            let [u0, v0, u1, v1] = b.uv;
            let c = b.color;
            let quad = [
                SpriteVertex { pos: p(x0, y1), uv: [u0, v0], color: c },
                SpriteVertex { pos: p(x0, y0), uv: [u0, v1], color: c },
                SpriteVertex { pos: p(x1, y0), uv: [u1, v1], color: c },
                SpriteVertex { pos: p(x0, y1), uv: [u0, v0], color: c },
                SpriteVertex { pos: p(x1, y0), uv: [u1, v1], color: c },
                SpriteVertex { pos: p(x1, y1), uv: [u1, v0], color: c },
            ];
            vertices.extend_from_slice(&quad);
            push_draw(&mut self.sprite_draws, &b.texture, first, 6);
        }
        for s in &frame.shapes {
            let first = vertices.len() as u32;
            let c = s.color;
            let n = 40;
            let at = |r: f32, a: f32, squash: f32| (s.center + Vec3::new(r * a.cos(), 0.0, r * a.sin() * squash)).to_array();
            for k in 0..n {
                let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
                match s.kind {
                    ShapeKind::Disc { radius, squash } => {
                        let mid = SpriteVertex { pos: s.center.to_array(), uv: [0.5, 0.5], color: c };
                        vertices.push(mid);
                        vertices.push(SpriteVertex { pos: at(radius, a0, squash), uv: [0.5, 0.5], color: c });
                        vertices.push(SpriteVertex { pos: at(radius, a1, squash), uv: [0.5, 0.5], color: c });
                    }
                    ShapeKind::Ring { inner, outer } => {
                        let v = |r: f32, a: f32| SpriteVertex { pos: at(r, a, 1.0), uv: [0.5, 0.5], color: c };
                        vertices.extend_from_slice(&[v(inner, a0), v(outer, a0), v(outer, a1), v(inner, a0), v(outer, a1), v(inner, a1)]);
                    }
                    ShapeKind::Quad { corners } => {
                        if k == 0 {
                            let v = |p: Vec3| SpriteVertex { pos: (s.center + p).to_array(), uv: [0.5, 0.5], color: c };
                            vertices.extend_from_slice(&[v(corners[0]), v(corners[1]), v(corners[2]), v(corners[0]), v(corners[2]), v(corners[3])]);
                        }
                    }
                    ShapeKind::Rect { width, depth } => {
                        if k == 0 {
                            let (w, d) = (width / 2.0, depth / 2.0);
                            let v = |x: f32, z: f32| SpriteVertex { pos: (s.center + Vec3::new(x, 0.0, z)).to_array(), uv: [0.5, 0.5], color: c };
                            vertices.extend_from_slice(&[v(-w, -d), v(-w, d), v(w, d), v(-w, -d), v(w, d), v(w, -d)]);
                        }
                    }
                }
            }
            let count = vertices.len() as u32 - first;
            push_draw(if s.additive { &mut self.add_draws } else { &mut self.blend_draws }, WHITE, first, count);
        }
        // glows face the camera fully
        let view = frame.view_proj;
        let inv = view.inverse();
        let cam_right = (inv * glam::Vec4::new(1.0, 0.0, 0.0, 0.0)).truncate().normalize_or(Vec3::X);
        let cam_up = (inv * glam::Vec4::new(0.0, 1.0, 0.0, 0.0)).truncate().normalize_or(Vec3::Y);
        for gl in &frame.glows {
            let first = vertices.len() as u32;
            let (r, u) = (cam_right * gl.size / 2.0, cam_up * gl.size / 2.0);
            let c = gl.color;
            let v = |p: Vec3, uv: [f32; 2]| SpriteVertex { pos: p.to_array(), uv, color: c };
            vertices.extend_from_slice(&[
                v(gl.pos - r + u, [0.0, 0.0]),
                v(gl.pos - r - u, [0.0, 1.0]),
                v(gl.pos + r - u, [1.0, 1.0]),
                v(gl.pos - r + u, [0.0, 0.0]),
                v(gl.pos + r - u, [1.0, 1.0]),
                v(gl.pos + r + u, [1.0, 0.0]),
            ]);
            push_draw(&mut self.add_draws, GLOW, first, 6);
        }
        for (top, bottom) in &frame.streaks {
            let first = vertices.len() as u32;
            let w = cam_right * 0.012;
            let c = [0.55, 0.65, 0.9, 0.55];
            let v = |p: Vec3| SpriteVertex { pos: p.to_array(), uv: [0.5, 0.5], color: c };
            vertices.extend_from_slice(&[v(*top - w), v(*bottom - w), v(*bottom + w), v(*top - w), v(*bottom + w), v(*top + w)]);
            push_draw(&mut self.blend_draws, WHITE, first, 6);
        }
        let bytes: &[u8] = bytemuck::cast_slice(&vertices);
        let needed = (bytes.len() as u64).max(64);
        if self.sprite_buffer.as_ref().map(|(_, cap)| *cap < needed).unwrap_or(true) {
            let cap = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprites"),
                size: cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.sprite_buffer = Some((buffer, cap));
        }
        let (sprite_buffer, _) = self.sprite_buffer.as_ref().unwrap();
        if !bytes.is_empty() {
            queue.write_buffer(sprite_buffer, 0, bytes);
        }

        // the sun's shadow map
        if frame.shadows {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_pipeline(&self.map_shadow_pipeline);
            for m in &self.meshes {
                let Some(t) = self.textures.get(&m.sheet) else { continue };
                pass.set_bind_group(1, &t.bind, &[]);
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.count, 0, 0..1);
            }
            if let Some(w) = &self.world {
                pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
                for m in [&w.solid, &w.objects, &w.extra].into_iter().flatten() {
                    m.draw(&mut pass);
                }
            }
            pass.set_pipeline(&self.sprite_shadow_pipeline);
            pass.set_vertex_buffer(0, sprite_buffer.slice(..));
            for d in &self.sprite_draws {
                pass.set_bind_group(1, &self.textures[&d.texture].bind, &[]);
                pass.draw(d.first..d.first + d.count, 0..1);
            }
        }

        // the scene
        let target = self.target.as_ref().unwrap();
        let sky = l.sky;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: sky.x as f64, g: sky.y as f64, b: sky.z as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.globals_bind, &[]);
        pass.set_bind_group(2, &self.shadow_bind, &[]);
        pass.set_pipeline(&self.map_pipeline);
        for m in &self.meshes {
            let Some(t) = self.textures.get(&m.sheet) else { continue };
            let _ = m.cutout;
            pass.set_bind_group(1, &t.bind, &[]);
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.count, 0, 0..1);
        }
        if let Some(w) = &self.world {
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            for m in [&w.solid, &w.objects, &w.extra].into_iter().flatten() {
                m.draw(&mut pass);
            }
            if let Some(tops) = &w.tops {
                pass.set_pipeline(&self.ground_pipeline);
                pass.set_bind_group(1, &w.bind, &[]);
                tops.draw(&mut pass);
            }
        }
        pass.set_vertex_buffer(0, sprite_buffer.slice(..));
        for (pipeline, draws) in [(&self.sprite_pipeline, &self.sprite_draws), (&self.blend_pipeline, &self.blend_draws), (&self.add_pipeline, &self.add_draws)] {
            if draws.is_empty() {
                continue;
            }
            pass.set_pipeline(pipeline);
            for d in draws {
                pass.set_bind_group(1, &self.textures[&d.texture].bind, &[]);
                pass.draw(d.first..d.first + d.count, 0..1);
            }
        }
    }

    /// Draws the scene (prepared by `prepare`) into the host's pass.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(target) = &self.target else { return };
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &self.globals_bind, &[]);
        pass.set_bind_group(1, &target.blit_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn make_target(&self, device: &wgpu::Device, size: [u32; 2]) -> Target {
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene colour"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.nearest) },
            ],
        });
        Target { size, color_view, depth_view: depth.create_view(&Default::default()), blit_bind }
    }
}

fn push_draw(draws: &mut Vec<DrawCall>, texture: &str, first: u32, count: u32) {
    if let Some(last) = draws.last_mut() {
        if last.texture == texture && last.first + last.count == first {
            last.count += count;
            return;
        }
    }
    draws.push(DrawCall { texture: texture.to_string(), first, count });
}
