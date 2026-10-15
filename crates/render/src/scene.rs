//! The GPU side: pipelines, textures, the map's meshes, and each frame's sprites and
//! effects. The scene is drawn into its own colour + depth target (with the sun's shadow
//! map first), then blitted into whatever pass the host gives (egui's, in the game).

use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::map::{build_meshes, MapScene, MapVertex, Spot};

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The sun's shadow map, texels a side, unless `Quality` says otherwise.
pub const SHADOW_SIZE: u32 = 2048;
/// World meshes are cut into squares this many cells a side, each drawn only when the
/// camera (or, for the shadow, the sun) can see it.
const CHUNK: f32 = 16.0;

/// How much the GPU draws: the scene's resolution against the screen's, and the sun's
/// shadow map's size (0: no shadows). For machines that can't keep up at full quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    /// 0.25 to 1
    pub scale: f32,
    pub shadow_size: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { scale: 1.0, shadow_size: SHADOW_SIZE }
    }
}

/// What the last frame had drawn: draw calls and triangles, in the scene and in the shadow.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawStats {
    pub draws: u32,
    pub triangles: u32,
    pub shadow_draws: u32,
    pub shadow_triangles: u32,
    /// world chunks left out because they were out of sight (scene + shadow)
    pub culled: u32,
}

/// A view's six planes (from its view-projection; depth 0 to 1, as wgpu has it).
struct Frustum([Vec4; 6]);

impl Frustum {
    fn new(m: &Mat4) -> Frustum {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        Frustum([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2])
    }

    /// Can any of the box be in it?
    fn sees(&self, lo: Vec3, hi: Vec3) -> bool {
        self.0.iter().all(|p| {
            let far = Vec3::new(if p.x >= 0.0 { hi.x } else { lo.x }, if p.y >= 0.0 { hi.y } else { lo.y }, if p.z >= 0.0 { hi.z } else { lo.z });
            p.x * far.x + p.y * far.y + p.z * far.z + p.w >= 0.0
        })
    }
}

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
    shadow: [f32; 4],
    focus: [f32; 4],
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
    /// an editor's focus: the world's cells outside it are darkened (the cells texture's
    /// blue channel says which are in), and what stands outside this box (x0, z0, x1, z1)
    pub focus: Option<[f32; 4]>,
    /// shapes drawn over everything (an editor's cursors), whatever is in front of them
    pub overlays: Vec<Shape>,
}

/// A world map's ground: the cells' texture and the terrains' palette (the meshes come
/// square by square, `set_chunk`).
pub struct WorldGround<'a> {
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
    /// its bounds, in the world
    lo: Vec3,
    hi: Vec3,
}

impl GpuMesh {
    fn new(device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) -> Option<GpuMesh> {
        if indices.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in vertices {
            let p = Vec3::from(v.pos);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        Some(GpuMesh {
            lo,
            hi,
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

    /// A world mesh cut into CHUNK squares (each triangle goes where its middle is), so
    /// what's out of sight can be left out.
    fn chunked(device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) -> Vec<GpuMesh> {
        let mut squares: BTreeMap<(i32, i32), Vec<u32>> = BTreeMap::new();
        for tri in indices.as_chunks::<3>().0 {
            let mid = tri.iter().fold(Vec3::ZERO, |m, &i| m + Vec3::from(vertices[i as usize].pos)) / 3.0;
            squares.entry(((mid.x / CHUNK).floor() as i32, (mid.z / CHUNK).floor() as i32)).or_default().extend_from_slice(tri);
        }
        if squares.len() <= 1 {
            return GpuMesh::new(device, vertices, indices).into_iter().collect();
        }
        // each square's own vertices: `slot` maps the mesh's to the square's (stamped with
        // the square's number, so it's never cleared)
        let mut slot = vec![(usize::MAX, 0u32); vertices.len()];
        squares
            .into_values()
            .enumerate()
            .filter_map(|(n, tris)| {
                let mut own = vec![];
                let idx: Vec<u32> = tris
                    .iter()
                    .map(|&i| {
                        let s = &mut slot[i as usize];
                        if s.0 != n {
                            *s = (n, own.len() as u32);
                            own.push(vertices[i as usize]);
                        }
                        s.1
                    })
                    .collect();
                GpuMesh::new(device, &own, &idx)
            })
            .collect()
    }
}

/// One layer's squares: each by texture (WHITE for the vertex-coloured ones), a texture's
/// pieces one after the other.
type Squares = BTreeMap<(i32, i32), Vec<(String, GpuMesh)>>;

/// The world map on the GPU, square by square (whatever size the editor cuts it in; each
/// square's meshes are cut again into CHUNK squares for the culling).
struct WorldGpu {
    /// the cell tops (the terrain shader), by square
    tops: BTreeMap<(i32, i32), Vec<GpuMesh>>,
    /// everything else by layer ("cliffs", "props", "trees", "buildings"...) and square:
    /// by texture (WHITE for the vertex-coloured ones), a texture's pieces one after the
    /// other
    layers: BTreeMap<String, Squares>,
    /// things that move (the walker, creatures): small, replaced often
    extra: Vec<(String, GpuMesh)>,
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
    shadow_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    /// the shadow map's size now
    shadow_size: u32,
    quality: Quality,
    stats: DrawStats,
    map_pipeline: wgpu::RenderPipeline,
    opaque_pipeline: wgpu::RenderPipeline,
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
    overlay_draws: Vec<DrawCall>,
    overlay_pipeline: wgpu::RenderPipeline,
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
        let (shadow_view, shadow_bind) = Self::make_shadow(device, &shadow_layout, &shadow_sampler, SHADOW_SIZE);

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
            bias: Default::default(),
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
        let opaque_pipeline = pipeline("opaque", &main_layout, "vs_map", "fs_map_opaque", &map_buffers, Some((COLOR, None)), Some(depth(true)));
        let map_shadow_pipeline = pipeline("map shadow", &shadow_pass_layout, "vs_map_shadow", "fs_map_shadow", &map_buffers, None, Some(shadow_depth.clone()));
        let sprite_pipeline = pipeline("sprites", &main_layout, "vs_sprite", "fs_sprite_cutout", &sprite_buffers, Some((COLOR, None)), Some(depth(true)));
        let sprite_shadow_pipeline = pipeline("sprite shadow", &shadow_pass_layout, "vs_sprite_shadow", "fs_sprite_shadow", &sprite_buffers, None, Some(shadow_depth));
        let blend_pipeline = pipeline("blend", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, Some((COLOR, Some(wgpu::BlendState::ALPHA_BLENDING))), Some(depth(false)));
        let on_top = wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() };
        let overlay_pipeline = pipeline("overlay", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, Some((COLOR, Some(wgpu::BlendState::ALPHA_BLENDING))), Some(on_top));
        let add_pipeline = pipeline("additive", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, Some((COLOR, Some(additive))), Some(depth(false)));
        let blit_pipeline = pipeline("blit", &shadow_pass_layout, "vs_blit", "fs_blit", &[], Some((output, None)), None);
        let ground_pipeline = pipeline("ground", &ground_pipeline_layout, "vs_map", "fs_ground", &map_buffers, Some((COLOR, None)), Some(depth(true)));

        let mut r = SceneRenderer {
            globals, globals_bind, texture_layout, nearest, linear, shadow_view, shadow_bind, shadow_layout, shadow_sampler,
            shadow_size: SHADOW_SIZE, quality: Quality::default(), stats: DrawStats::default(),
            map_pipeline, opaque_pipeline, map_shadow_pipeline, sprite_pipeline, sprite_shadow_pipeline, blend_pipeline, add_pipeline, blit_pipeline,
            ground_pipeline, ground_layout, world: None,
            textures: HashMap::new(), meshes: vec![], map_name: None, spot: None, target: None, sprite_buffer: None,
            sprite_draws: vec![], blend_draws: vec![], add_draws: vec![], overlay_draws: vec![], overlay_pipeline, sky: [0.1, 0.1, 0.1],
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
            for p in [&self.map_pipeline, &self.opaque_pipeline, &self.sprite_pipeline, &self.blend_pipeline, &self.add_pipeline, &self.overlay_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
            pass.set_bind_group(1, &ground.1, &[]);
            pass.set_pipeline(&self.ground_pipeline);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }

    /// The sun's shadow map, `size` texels a side.
    fn make_shadow(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, size: u32) -> (wgpu::TextureView, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        (view, bind)
    }

    /// Sets the resolution and the shadows (taking effect on the next frame).
    pub fn set_quality(&mut self, device: &wgpu::Device, q: Quality) {
        let q = Quality { scale: q.scale.clamp(0.25, 1.0), shadow_size: q.shadow_size.min(device.limits().max_texture_dimension_2d) };
        if q.shadow_size > 0 && q.shadow_size != self.shadow_size {
            (self.shadow_view, self.shadow_bind) = Self::make_shadow(device, &self.shadow_layout, &self.shadow_sampler, q.shadow_size);
            self.shadow_size = q.shadow_size;
        }
        if q.scale != self.quality.scale {
            self.target = None;
        }
        self.quality = q;
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    /// What the last frame had drawn.
    pub fn stats(&self) -> DrawStats {
        self.stats
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

    /// Shows a world map's ground: its cells' texture and palette (the one before goes; its
    /// squares stay when the map is the same size, else they go too). Battle maps
    /// (`load_map`) and the world draw together; clear the one you don't want.
    pub fn set_world_ground(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, g: &WorldGround) {
        let (cells, bind) = self.data_bind(device, queue, g.cells, g.size, g.palette, g.palette_width);
        let (tops, layers, extra) = match self.world.take() {
            Some(w) if w.size == g.size => (w.tops, w.layers, w.extra),
            _ => (BTreeMap::new(), BTreeMap::new(), vec![]),
        };
        self.world = Some(WorldGpu { tops, layers, extra, cells, size: g.size, bind, level_height: g.level_height });
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

    /// One square of one layer of the world map (after `set_world_ground`), replacing what
    /// that square held: "tops" is the ground's cell tops (the terrain shader); any other
    /// name ("cliffs", "props", "trees", "buildings") draws like props. No batches: the
    /// square goes. Batches by texture key (`WHITE` for vertex colours alone); a texture
    /// that isn't loaded draws white.
    pub fn set_chunk(&mut self, device: &wgpu::Device, layer: &str, square: (i32, i32), batches: &[(String, Vec<MapVertex>, Vec<u32>)]) {
        let Some(w) = &mut self.world else { return };
        if layer == "tops" {
            let meshes: Vec<GpuMesh> = batches.iter().flat_map(|(_, v, i)| GpuMesh::chunked(device, v, i)).collect();
            if meshes.is_empty() {
                w.tops.remove(&square);
            } else {
                w.tops.insert(square, meshes);
            }
            return;
        }
        let meshes: Vec<(String, GpuMesh)> = batches.iter().flat_map(|(k, v, i)| GpuMesh::chunked(device, v, i).into_iter().map(|m| (k.clone(), m))).collect();
        let squares = w.layers.entry(layer.to_string()).or_default();
        if meshes.is_empty() {
            squares.remove(&square);
        } else {
            squares.insert(square, meshes);
        }
    }

    /// Drops the squares `keep` doesn't want, in every layer (the ones far from the camera).
    pub fn keep_chunks(&mut self, keep: &dyn Fn((i32, i32)) -> bool) {
        let Some(w) = &mut self.world else { return };
        w.tops.retain(|s, _| keep(*s));
        for squares in w.layers.values_mut() {
            squares.retain(|s, _| keep(*s));
        }
    }

    /// What moves on the world map (after `set_world_ground`); empty clears it.
    pub fn set_world_extra(&mut self, device: &wgpu::Device, batches: &[(String, Vec<MapVertex>, Vec<u32>)]) {
        if let Some(w) = &mut self.world {
            w.extra = batches.iter().filter_map(|(k, v, i)| GpuMesh::new(device, v, i).map(|m| (k.clone(), m))).collect();
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
        let scale = self.quality.scale;
        let size = [((size[0] as f32 * scale).round() as u32).max(1), ((size[1] as f32 * scale).round() as u32).max(1)];
        if self.target.as_ref().map(|t| t.size != size).unwrap_or(true) {
            self.target = Some(self.make_target(device, size));
        }
        let l = &frame.light;
        self.sky = l.sky.to_array();
        let shadows = frame.shadows && self.quality.shadow_size > 0;
        let light_view_proj = on_texels(&l.light_view_proj, self.shadow_size);
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            sun_dir: l.sun_dir.normalize().extend(0.0).to_array(),
            sun_color: l.sun.extend(0.0).to_array(),
            ambient: l.ambient.extend(0.0).to_array(),
            hemi_sky: l.hemi_sky.extend(0.0).to_array(),
            hemi_ground: l.hemi_ground.extend(0.0).to_array(),
            fog_color: [l.sky.x, l.sky.y, l.sky.z, l.fog_near],
            params: [l.fog_far, shadows as u32 as f32, (!self.output_srgb) as u32 as f32, 0.0],
            camera: frame.camera_pos.extend(1.0).to_array(),
            world: match &self.world {
                Some(w) => [w.size[0] as f32, w.size[1] as f32, w.level_height, frame.time],
                None => [0.0, 0.0, 2.0, frame.time],
            },
            editor: [frame.grid as u32 as f32, frame.focus.is_some() as u32 as f32, 0.0, 0.0],
            shadow: shadow_params(&light_view_proj, self.shadow_size),
            focus: frame.focus.unwrap_or([0.0; 4]),
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        // this frame's sprites, shapes and glows: one vertex buffer, drawn by texture
        let mut vertices: Vec<SpriteVertex> = vec![];
        self.sprite_draws.clear();
        self.blend_draws.clear();
        self.add_draws.clear();
        self.overlay_draws.clear();
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
        let shapes = frame.shapes.iter().map(|s| (s, false)).chain(frame.overlays.iter().map(|s| (s, true)));
        for (s, over) in shapes {
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
            let list = if over {
                &mut self.overlay_draws
            } else if s.additive {
                &mut self.add_draws
            } else {
                &mut self.blend_draws
            };
            push_draw(list, WHITE, first, count);
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

        let mut stats = DrawStats::default();
        let eye = Frustum::new(&frame.view_proj);
        let sun = Frustum::new(&light_view_proj);

        // the sun's shadow map
        if shadows {
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
                let objects = w.layers.values().flat_map(|l| l.values()).flatten().chain(&w.extra).map(|(k, m)| (k.as_str(), m));
                let mut bound = WHITE;
                for (key, m) in objects {
                    if !sun.sees(m.lo, m.hi) {
                        stats.culled += 1;
                        continue;
                    }
                    if key != bound {
                        pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                        bound = key;
                    }
                    m.draw(&mut pass);
                    stats.shadow_draws += 1;
                    stats.shadow_triangles += m.count / 3;
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
            // what's in sight, nearest first: what's behind is then hidden before it's shaded
            let objects = w.layers.values().flat_map(|l| l.values()).flatten().chain(&w.extra).map(|(k, m)| (k.as_str(), m));
            let mut seen: Vec<(f32, &str, &GpuMesh)> = vec![];
            for (key, m) in objects {
                if eye.sees(m.lo, m.hi) {
                    seen.push((((m.lo + m.hi) * 0.5).distance_squared(frame.camera_pos), key, m));
                } else {
                    stats.culled += 1;
                }
            }
            seen.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut bound: Option<&str> = None;
            let mut cutout: Option<bool> = None;
            for (_, key, m) in seen {
                let cuts = key != WHITE;
                if cutout != Some(cuts) {
                    pass.set_pipeline(if cuts { &self.map_pipeline } else { &self.opaque_pipeline });
                    cutout = Some(cuts);
                }
                if bound != Some(key) {
                    pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                    bound = Some(key);
                }
                m.draw(&mut pass);
                stats.draws += 1;
                stats.triangles += m.count / 3;
            }
            // the ground last: what stands on it hides most of it, and its shader is the
            // costliest
            pass.set_pipeline(&self.ground_pipeline);
            pass.set_bind_group(1, &w.bind, &[]);
            for m in w.tops.values().flatten() {
                if !eye.sees(m.lo, m.hi) {
                    stats.culled += 1;
                    continue;
                }
                m.draw(&mut pass);
                stats.draws += 1;
                stats.triangles += m.count / 3;
            }
        }
        pass.set_vertex_buffer(0, sprite_buffer.slice(..));
        for (pipeline, draws) in [(&self.sprite_pipeline, &self.sprite_draws), (&self.blend_pipeline, &self.blend_draws), (&self.add_pipeline, &self.add_draws), (&self.overlay_pipeline, &self.overlay_draws)] {
            if draws.is_empty() {
                continue;
            }
            pass.set_pipeline(pipeline);
            for d in draws {
                pass.set_bind_group(1, &self.textures[&d.texture].bind, &[]);
                pass.draw(d.first..d.first + d.count, 0..1);
                stats.draws += 1;
                stats.triangles += d.count / 3;
            }
        }
        self.stats = stats;
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
                // at a lower resolution, smoothed on the way up
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if self.quality.scale < 1.0 { &self.linear } else { &self.nearest }) },
            ],
        });
        Target { size, color_view, depth_view: depth.create_view(&Default::default()), blit_bind }
    }
}

/// What the shader needs to read the shadow map without acne and without detaching shadows
/// from what casts them: how far to move a surface along its normal before looking it up
/// (1.5 shadow texels, in the world), a small depth margin (half a texel, in the light's
/// depth), and a texel in uv. Both in world units, so the same at any zoom: a fixed margin
/// in the light's depth was a fraction of its whole range, up to a world unit on big views,
/// and every shadow started that far from its caster.
fn shadow_params(light_view_proj: &Mat4, size: u32) -> [f32; 4] {
    // an orthographic light: its first rows' lengths are its scales (2 / width, 1 / depth)
    let m = light_view_proj;
    let row = |i: usize| m.row(i).truncate().length();
    let texel = 2.0 / row(0).max(1e-6) / size as f32;
    [texel * 1.5, texel * 0.5 * row(2), 1.0 / size as f32, 0.0]
}

/// The sun's view moved by less than a texel so the world lies on the shadow map's texel
/// grid wherever the camera is: the shadows' edges stay still when it moves.
fn on_texels(light_view_proj: &Mat4, size: u32) -> Mat4 {
    let o = *light_view_proj * Vec4::W;
    let texel = 2.0 / size as f32;
    let snap = |v: f32| (v / texel).round() * texel - v;
    Mat4::from_translation(Vec3::new(snap(o.x), snap(o.y), 0.0)) * *light_view_proj
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_sees_what_is_in_front() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 10.0, 10.0), Vec3::ZERO, Vec3::Y);
        let f = Frustum::new(&(Mat4::perspective_rh(1.0, 1.5, 0.1, 200.0) * view));
        assert!(f.sees(Vec3::splat(-1.0), Vec3::splat(1.0)));
        // behind the camera, and far off to the side
        assert!(!f.sees(Vec3::new(-1.0, 9.0, 30.0), Vec3::new(1.0, 11.0, 32.0)));
        assert!(!f.sees(Vec3::new(300.0, 0.0, 0.0), Vec3::new(310.0, 1.0, 1.0)));
        // a box straddling the edge counts
        assert!(f.sees(Vec3::new(-100.0, -1.0, -1.0), Vec3::new(0.0, 1.0, 1.0)));
    }

    #[test]
    fn shadow_texels_hold_still() {
        // two targets a fraction of a texel apart: the world lands on the same texel grid
        let light = |x: f32| {
            let view = Mat4::look_at_rh(Vec3::new(x - 30.0, 60.0, 40.0), Vec3::new(x, 0.0, 0.0), Vec3::Y);
            on_texels(&(Mat4::orthographic_rh(-32.0, 32.0, -32.0, 32.0, 1.0, 200.0) * view), 2048)
        };
        let texel = 2.0 / 2048.0;
        for x in [0.0, 0.013, 0.4, 7.77] {
            let p = light(x) * Vec4::new(5.0, 1.0, 3.0, 1.0);
            let q = light(0.0) * Vec4::new(5.0, 1.0, 3.0, 1.0);
            let frac = |v: f32| (v / texel - (v / texel).round()).abs();
            assert!((frac(p.x) - frac(q.x)).abs() < 1e-2 && (frac(p.y) - frac(q.y)).abs() < 1e-2, "{x}");
        }
    }

    #[test]
    fn shadow_margins_are_world_sized() {
        // the same view at two depth ranges: the normal offset doesn't change, and the depth
        // margin is the same distance in the world
        let view = Mat4::look_at_rh(Vec3::new(-30.0, 60.0, 40.0), Vec3::ZERO, Vec3::Y);
        let near = shadow_params(&(Mat4::orthographic_rh(-20.0, 20.0, -20.0, 20.0, 1.0, 200.0) * view), 2048);
        let far = shadow_params(&(Mat4::orthographic_rh(-20.0, 20.0, -20.0, 20.0, 1.0, 2000.0) * view), 2048);
        assert!((near[0] - far[0]).abs() < 1e-6);
        assert!((near[0] - 40.0 / 2048.0 * 1.5).abs() < 1e-4);
        assert!((near[1] * 199.0 - far[1] * 1999.0).abs() < 1e-5);
    }
}
