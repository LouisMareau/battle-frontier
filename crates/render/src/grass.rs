//! Grass grown through tall ground drawn as copies of its models (`SceneRenderer::set_grass`):
//! each model on the GPU once, in levels each thinner than the one before
//! (`world::grass::thin`: about half the blades, those kept wider), and each copy of it a
//! small record (`GrassCopy`). Each frame a copy out of the eye's sight is left out, one just
//! outside it (casting its shadow in) drawn at its thinnest, and every other one at the finest
//! level that costs no more than it looks big from the eye (about `DENSITY` vertices to a pixel
//! of the ground it covers, and of a quarter of it standing up: the rest of that covers the
//! ground of the copies behind it): a field costs about as much as the picture it fills,
//! however finely its grass is made. Culled and picked on the GPU
//! (`grass.wgsl`, the draws indirect), or, with no compute shaders (WebGL 2), the same on the
//! CPU (`pick`). The sun's shadow draws the same copies a couple of levels thinner.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::map::MapVertex;
use crate::scene::{DrawStats, WHITE};

/// How many vertices a pixel a copy covers may cost at the level it's drawn at.
pub const DENSITY: f32 = 1.5;
/// The fewest pixels a copy may cover (its ground and it standing) to be drawn.
pub const FEWEST_PIXELS: f32 = 4.0;
/// The sun's shadow draws each copy this many levels thinner.
pub const SHADOW_THINNER: usize = 2;
/// How far past the eye's sight copies are still drawn (cells): their shadows fall into it.
pub const MARGIN: f32 = 6.0;
/// The most copies of one model (more are left out).
pub const MOST_COPIES: usize = 1 << 18;
/// The most levels of a model (as `world::grass::LEVELS`).
pub const MOST_LEVELS: usize = 12;
/// How far the wind can bend a model past its box (cells).
pub(crate) const SWAY: f32 = 0.6;
/// Bytes between two models' records for the compute shader: a dynamic offset's alignment.
const INFO_STRIDE: u64 = 256;
const COPY_BYTES: u64 = std::mem::size_of::<GrassCopy>() as u64;

/// A square's copies, by model.
type Copies = Vec<(String, Vec<GrassCopy>)>;

/// A copy of a grass model: where its foot stands (in its map's own space as it's sent; the
/// scene's once its map is placed), its turn (radians about y, as `Mat4::from_rotation_y`
/// turns), its size against the model as made, and how dimmed its map is (0: not at all).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GrassCopy {
    pub at: [f32; 3],
    pub turn: f32,
    pub scale: f32,
    pub dim: f32,
    pub pad: [f32; 2],
}

/// A level of a grass model: its batches by texture (`WHITE` for vertex colours alone).
pub type GrassLevel = Vec<(String, Vec<MapVertex>, Vec<u32>)>;

/// A grass model as it's sent: its levels (the first as made, each next thinner); the cells it
/// covers; the box round all its levels (its own space, its foot on y 0).
pub struct GrassModel<'a> {
    pub levels: &'a [GrassLevel],
    pub cells: f32,
    pub lo: Vec3,
    pub hi: Vec3,
}

/// The culling's settings for a frame: the eye's view and projection, its sight's planes
/// (each normal a unit, pointing in, pushed out by `MARGIN`), and the picture's size in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Cull {
    pub view_proj: Mat4,
    pub planes: [Vec4; 6],
    pub screen: Vec2,
}

impl Cull {
    pub fn new(view_proj: Mat4, size: [u32; 2]) -> Cull {
        let (r0, r1, r2, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| {
            let p = p / p.truncate().length().max(1e-9);
            Vec4::new(p.x, p.y, p.z, p.w + MARGIN)
        });
        Cull { view_proj, planes, screen: Vec2::new(size[0].max(1) as f32, size[1].max(1) as f32) }
    }

    fn uniform(&self) -> CullUniform {
        CullUniform {
            view_proj: self.view_proj.to_cols_array_2d(),
            planes: self.planes.map(|p| p.to_array()),
            margin: [MARGIN, 0.0, 0.0, 0.0],
            screen: [self.screen.x, self.screen.y, FEWEST_PIXELS, DENSITY],
        }
    }
}

/// 0 to 1, copy `i`'s own: where one level gives way to the next differs copy to copy (as
/// `grass.wgsl`'s `jitter`).
fn jitter(i: u32) -> f32 {
    let mut h = i.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277_803_737);
    h ^= h >> 22;
    (h & 0xffff) as f32 / 65535.0
}

/// A grass model's shape, as the culling sees it: the middle of its box, how far round that
/// it reaches (the wind's bending in), the cells it covers and how tall it stands.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub middle: Vec3,
    pub reach: f32,
    pub cells: f32,
    pub tall: f32,
}

impl Shape {
    /// The shape of a model whose levels are all in the box `lo` to `hi`, covering `cells`.
    pub fn of(lo: Vec3, hi: Vec3, cells: f32) -> Shape {
        Shape { middle: (lo + hi) * 0.5, reach: (hi - lo).length() * 0.5 + SWAY, cells: cells.max(1.0), tall: (hi.y - lo.y).max(0.0) }
    }
}

/// The level copy `i` of a model is drawn at (its shape, its levels' vertices `costs`), or none
/// when it isn't: out of sight, or too small to see. Just outside the eye's sight (casting its
/// shadow in): its thinnest. As `grass.wgsl` picks it.
pub fn pick(cull: &Cull, shape: &Shape, costs: &[f32], c: &GrassCopy, i: u32) -> Option<usize> {
    let Shape { middle, reach, cells, tall } = *shape;
    let (s, k) = c.turn.sin_cos();
    let m = middle * c.scale;
    let mid = Vec3::from(c.at) + Vec3::new(k * m.x + s * m.z, m.y, k * m.z - s * m.x);
    let r = reach * c.scale;
    let mut seen = true;
    for p in &cull.planes {
        let d = p.truncate().dot(mid) + p.w;
        if d < -r {
            return None;
        }
        seen &= d >= MARGIN - r;
    }
    let last = costs.len().saturating_sub(1);
    if !seen {
        return Some(last);
    }
    // the pixels its ground covers, and it standing up
    let at = |p: Vec3| cull.view_proj * p.extend(1.0);
    let (a, x, y, z) = (at(mid), at(mid + Vec3::X), at(mid + Vec3::Y * tall * c.scale), at(mid + Vec3::Z));
    let area = if a.w.min(x.w).min(y.w).min(z.w) > 1e-4 {
        let half = cull.screen * 0.5;
        let o = a.truncate().truncate() / a.w * half;
        let [dx, dy, dz] = [x, y, z].map(|q| q.truncate().truncate() / q.w * half - o);
        dx.perp_dot(dz).abs() + 0.25 * dx.perp_dot(dy).abs().max(dz.perp_dot(dy).abs())
    } else {
        1e12
    };
    if area < FEWEST_PIXELS {
        return None;
    }
    let want = DENSITY * area * (0.75 + 0.5 * jitter(i));
    Some(costs.iter().position(|&v| v / cells <= want).unwrap_or(last))
}

/// The culling's settings as the compute shader reads them (`Cull` there).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CullUniform {
    view_proj: [[f32; 4]; 4],
    planes: [[f32; 4]; 6],
    margin: [f32; 4],
    screen: [f32; 4],
}

/// A model as the compute shader reads it (`Model` there).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InfoUniform {
    count: [u32; 4],
    middle: [f32; 4],
    cells: [f32; 4],
    cost: [[f32; 4]; 3],
}

/// A level's draw of one texture: its indices, where they start, and its vertices'.
struct Batch {
    texture: String,
    first: u32,
    count: u32,
    base: i32,
}

struct Level {
    batches: Vec<Batch>,
    vertices: f32,
}

/// A model's copies on the GPU: room for `cap` at every level, each level's list one after
/// another (the compute shader writes them; on the CPU, `prepare`); the copies themselves
/// for the compute shader, and its bind group.
struct Held {
    cap: usize,
    copies: Option<wgpu::Buffer>,
    out: wgpu::Buffer,
    bind: Option<wgpu::BindGroup>,
}

impl Held {
    fn new(device: &wgpu::Device, cap: usize, levels: usize, compute: bool) -> Held {
        let storage = if compute { wgpu::BufferUsages::STORAGE } else { wgpu::BufferUsages::COPY_DST };
        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_SRC | storage;
        let out = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grass lists"), size: cap as u64 * levels as u64 * COPY_BYTES, usage, mapped_at_creation: false });
        let copies = compute.then(|| device.create_buffer(&wgpu::BufferDescriptor { label: Some("grass copies"), size: cap as u64 * COPY_BYTES, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        Held { cap, copies, out, bind: None }
    }
}

struct Model {
    version: u64,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    levels: Vec<Level>,
    shape: Shape,
    toon: bool,
    /// its copies in the scene's space, from every map's squares (`gather`)
    copies: Vec<GrassCopy>,
    held: Option<Held>,
    /// its first list's counter (a level each)
    first: usize,
}

impl Model {
    fn info(&self) -> InfoUniform {
        let mut cost = [[0.0f32; 4]; 3];
        for (k, l) in self.levels.iter().enumerate() {
            cost[k / 4][k % 4] = l.vertices;
        }
        // (each list's room as its buffer has it: as the draws read it)
        let room = self.held.as_ref().map(|h| h.cap).unwrap_or(0) as u32;
        let s = self.shape;
        InfoUniform { count: [self.copies.len() as u32, self.levels.len() as u32, self.first as u32, room], middle: s.middle.extend(s.reach).to_array(), cells: [s.cells, s.tall, 0.0, 0.0], cost }
    }
}

/// A draw: copies of a model from one level's list (`list`), drawn with one level's batch
/// (`mesh`: the list's own, or for the sun a thinner one), with its list's counter.
struct Draw {
    model: usize,
    list: usize,
    mesh: usize,
    batch: usize,
    shadow: bool,
    counter: usize,
}

/// The compute shaders and what they read and write, where the GPU has them: the frame's
/// settings, the models' records (`INFO_STRIDE` apart), the lists' counters, the draws (as
/// `DrawIndexedIndirectArgs`, their counts written by `spread`) with their lists, and the
/// counters read back now and then (for the stats).
struct Compute {
    cull: wgpu::ComputePipeline,
    spread: wgpu::ComputePipeline,
    cull_layout: wgpu::BindGroupLayout,
    spread_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    infos: wgpu::Buffer,
    infos_cap: usize,
    counters: wgpu::Buffer,
    counters_cap: usize,
    args: wgpu::Buffer,
    lists: wgpu::Buffer,
    draws_cap: usize,
    spread_bind: Option<wgpu::BindGroup>,
    readback: Option<wgpu::Buffer>,
    /// a read-back asked for: done (1), failed (2) or not yet (0), and how many counters
    reading: Option<(Arc<AtomicU8>, usize)>,
}

impl Compute {
    fn new(device: &wgpu::Device) -> Compute {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("grass"), source: wgpu::ShaderSource::Wgsl(include_str!("grass.wgsl").into()) });
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let storage = |read_only: bool| wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None };
        let cull_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grass cull"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<InfoUniform>() as u64) }),
                entry(2, storage(true)),
                entry(3, storage(false)),
                entry(4, storage(false)),
            ],
        });
        let spread_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("grass spread"), entries: &[entry(5, storage(true)), entry(6, storage(true)), entry(7, storage(false))] });
        let pipeline = |layout: &wgpu::BindGroupLayout, entry_point: &str| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(entry_point), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry_point), layout: Some(&layout), module: &module, entry_point: Some(entry_point), compilation_options: Default::default(), cache: None })
        };
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });
        Compute {
            cull: pipeline(&cull_layout, "cull_copies"),
            spread: pipeline(&spread_layout, "spread"),
            params: buffer("grass cull", std::mem::size_of::<CullUniform>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
            infos: buffer("grass models", INFO_STRIDE * 4, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
            infos_cap: 4,
            counters: buffer("grass counters", 4 * 64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC),
            counters_cap: 64,
            args: buffer("grass draws", 20 * 64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST),
            lists: buffer("grass draws' lists", 4 * 64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST),
            draws_cap: 64,
            cull_layout,
            spread_layout,
            spread_bind: None,
            readback: None,
            reading: None,
        }
    }

    /// Room for the models' records, the counters and the draws; the draws' fixed numbers;
    /// the models' records; every bind group again (what they bind may be new).
    fn hold(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, models: &mut [Model], draws: &[Draw], counters: usize) {
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });
        if models.len() > self.infos_cap {
            self.infos_cap = models.len().next_power_of_two();
            self.infos = buffer("grass models", INFO_STRIDE * self.infos_cap as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        }
        if counters > self.counters_cap {
            self.counters_cap = counters.next_power_of_two();
            self.counters = buffer("grass counters", 4 * self.counters_cap as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC);
        }
        if draws.len() > self.draws_cap {
            self.draws_cap = draws.len().next_power_of_two();
            self.args = buffer("grass draws", 20 * self.draws_cap as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST);
            self.lists = buffer("grass draws' lists", 4 * self.draws_cap as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        }
        let mut args: Vec<u32> = Vec::with_capacity(draws.len() * 5);
        let mut lists: Vec<u32> = Vec::with_capacity(draws.len());
        for d in draws {
            let b = &models[d.model].levels[d.mesh].batches[d.batch];
            args.extend([b.count, 0, b.first, b.base as u32, 0]);
            lists.push(d.counter as u32);
        }
        if !draws.is_empty() {
            queue.write_buffer(&self.args, 0, bytemuck::cast_slice(&args));
            queue.write_buffer(&self.lists, 0, bytemuck::cast_slice(&lists));
        }
        let mut infos = vec![0u8; INFO_STRIDE as usize * models.len().max(1)];
        for (k, m) in models.iter().enumerate() {
            let at = k * INFO_STRIDE as usize;
            infos[at..at + std::mem::size_of::<InfoUniform>()].copy_from_slice(bytemuck::bytes_of(&m.info()));
        }
        queue.write_buffer(&self.infos, 0, &infos);
        for m in models.iter_mut() {
            let Some(h) = &mut m.held else { continue };
            let Some(copies) = &h.copies else { continue };
            h.bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("grass cull"),
                layout: &self.cull_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.infos, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<InfoUniform>() as u64) }) },
                    wgpu::BindGroupEntry { binding: 2, resource: copies.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: h.out.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: self.counters.as_entire_binding() },
                ],
            }));
        }
        // (the draws' lists bound as many as there are: the shader counts them)
        self.spread_bind = (!draws.is_empty()).then(|| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("grass spread"),
                layout: &self.spread_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 5, resource: self.counters.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.lists, offset: 0, size: wgpu::BufferSize::new(4 * draws.len() as u64) }) },
                    wgpu::BindGroupEntry { binding: 7, resource: self.args.as_entire_binding() },
                ],
            })
        });
    }

    /// The counters read back, once one asked for has come.
    fn collect(&mut self) -> Option<Vec<u32>> {
        let (state, n) = {
            let (flag, n) = self.reading.as_ref()?;
            (flag.load(Ordering::Acquire), *n)
        };
        match state {
            0 => None,
            1 => {
                self.reading = None;
                let buf = self.readback.as_ref()?;
                let counts = buf.slice(..n as u64 * 4).get_mapped_range().ok().map(|view| bytemuck::cast_slice::<u8, u32>(&view).to_vec());
                buf.unmap();
                counts
            }
            _ => {
                self.reading = None;
                None
            }
        }
    }

    /// Asks for the counters as this frame leaves them, unless a read-back is on its way.
    fn ask(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, counters: usize) {
        if self.reading.is_some() || counters == 0 {
            return;
        }
        let bytes = counters as u64 * 4;
        if self.readback.as_ref().is_none_or(|b| b.size() < bytes) {
            self.readback = Some(device.create_buffer(&wgpu::BufferDescriptor { label: Some("grass counts"), size: bytes.next_power_of_two(), usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        let buf = self.readback.as_ref().unwrap();
        encoder.copy_buffer_to_buffer(&self.counters, 0, buf, 0, bytes);
        let flag = Arc::new(AtomicU8::new(0));
        let done = flag.clone();
        encoder.map_buffer_on_submit(buf, wgpu::MapMode::Read, ..bytes, move |r| done.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release));
        self.reading = Some((flag, counters));
    }
}

/// The grass the scene draws as copies: its models, every world map's copies square by
/// square, and how they're drawn this frame.
pub(crate) struct Grass {
    opaque: wgpu::RenderPipeline,
    cutout: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    compute: Option<Compute>,
    models: Vec<Model>,
    index: HashMap<String, usize>,
    /// per world map and square, the copies by model (the map's own space)
    squares: BTreeMap<String, BTreeMap<(i32, i32), Copies>>,
    /// each map's placement as last told: where its space sits, how dimmed
    places: BTreeMap<String, (Vec3, f32)>,
    /// the copies to gather again (a square, a model or a placement changed)
    dirty: bool,
    draws: Vec<Draw>,
    counters: usize,
    /// on the CPU: each list's copies this frame
    counts: Vec<u32>,
    /// the copies drawn, as last known, their vertices, and how many at each level
    drawn: u32,
    drawn_vertices: u64,
    drawn_levels: [u32; MOST_LEVELS],
}

impl Grass {
    /// With the pipelines its copies are drawn with (vertex colours, cut-outs, the sun's);
    /// culled on the GPU when it has compute shaders.
    pub(crate) fn new(device: &wgpu::Device, opaque: wgpu::RenderPipeline, cutout: wgpu::RenderPipeline, shadow: wgpu::RenderPipeline) -> Grass {
        let l = device.limits();
        let compute = (l.max_compute_workgroups_per_dimension > 0 && l.max_compute_workgroup_size_x >= 64 && l.max_storage_buffers_per_shader_stage >= 3).then(|| Compute::new(device));
        Grass { opaque, cutout, shadow, compute, models: vec![], index: HashMap::new(), squares: BTreeMap::new(), places: BTreeMap::new(), dirty: false, draws: vec![], counters: 0, counts: vec![], drawn: 0, drawn_vertices: 0, drawn_levels: [0; MOST_LEVELS] }
    }

    /// Culled and picked on the CPU from now on (as where there are no compute shaders).
    #[cfg(test)]
    pub(crate) fn on_cpu(&mut self) {
        self.compute = None;
        for m in &mut self.models {
            m.held = None;
        }
        self.dirty = true;
    }

    pub(crate) fn culled_on_gpu(&self) -> bool {
        self.compute.is_some()
    }

    pub(crate) fn version(&self, key: &str) -> Option<u64> {
        self.index.get(key).map(|&k| self.models[k].version)
    }

    /// A model, replacing the one of that key: every level's vertices and indices one after
    /// another in a buffer of each, each level's batches by where they start in them. With
    /// nothing in its first level, none.
    pub(crate) fn set_model(&mut self, device: &wgpu::Device, key: &str, version: u64, g: &GrassModel) {
        let (mut vertices, mut indices): (Vec<MapVertex>, Vec<u32>) = (vec![], vec![]);
        let mut levels = vec![];
        let mut toon = false;
        for level in g.levels.iter().take(MOST_LEVELS) {
            let mut batches = vec![];
            let mut n = 0;
            for (texture, v, i) in level.iter().filter(|(_, _, i)| !i.is_empty()) {
                batches.push(Batch { texture: texture.clone(), first: indices.len() as u32, count: i.len() as u32, base: vertices.len() as i32 });
                toon |= v.iter().any(|v| v.toon > 0.5 && v.toon < 1.5);
                n += v.len();
                vertices.extend_from_slice(v);
                indices.extend_from_slice(i);
            }
            if batches.is_empty() {
                break;
            }
            levels.push(Level { batches, vertices: n as f32 });
        }
        if levels.is_empty() {
            return;
        }
        let model = Model {
            version,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("grass model"), contents: bytemuck::cast_slice(&vertices), usage: wgpu::BufferUsages::VERTEX }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("grass model"), contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX }),
            levels,
            shape: Shape::of(g.lo, g.hi, g.cells),
            toon,
            copies: vec![],
            held: None,
            first: 0,
        };
        match self.index.get(key) {
            Some(&k) => self.models[k] = model,
            None => {
                self.index.insert(key.to_string(), self.models.len());
                self.models.push(model);
            }
        }
        self.dirty = true;
    }

    /// One square's copies on a map, by model, replacing what it held (none: it goes).
    pub(crate) fn set_copies(&mut self, map: &str, square: (i32, i32), copies: Vec<(String, Vec<GrassCopy>)>) {
        let squares = self.squares.entry(map.to_string()).or_default();
        if copies.iter().all(|(_, c)| c.is_empty()) {
            self.dirty |= squares.remove(&square).is_some();
        } else {
            squares.insert(square, copies);
            self.dirty = true;
        }
    }

    /// Where a map's space sits in the scene and how dimmed it is.
    pub(crate) fn place(&mut self, map: &str, offset: Vec3, dim: f32) {
        if self.places.get(map) != Some(&(offset, dim)) {
            self.places.insert(map.to_string(), (offset, dim));
            self.dirty |= self.squares.contains_key(map);
        }
    }

    /// Keeps a map's squares that `keep` wants.
    pub(crate) fn keep(&mut self, map: &str, keep: &dyn Fn((i32, i32)) -> bool) {
        if let Some(squares) = self.squares.get_mut(map) {
            let n = squares.len();
            squares.retain(|s, _| keep(*s));
            self.dirty |= squares.len() != n;
        }
    }

    /// Keeps the maps that `keep` names.
    pub(crate) fn keep_maps(&mut self, keep: &dyn Fn(&str) -> bool) {
        let n = self.squares.len();
        self.squares.retain(|id, _| keep(id));
        self.places.retain(|id, _| keep(id));
        self.dirty |= self.squares.len() != n;
    }

    pub(crate) fn drop_map(&mut self, map: &str) {
        self.dirty |= self.squares.remove(map).is_some();
    }

    pub(crate) fn clear(&mut self) {
        self.dirty |= !self.squares.is_empty();
        self.squares.clear();
        self.places.clear();
    }

    /// Whether any copy drawn is toon (the ink lines look for it).
    pub(crate) fn toon(&self) -> bool {
        self.models.iter().any(|m| m.toon && !m.copies.is_empty())
    }

    /// The copies drawn, as last known (on the GPU, read back a few frames late), what their
    /// levels cost in vertices, and how many at each level (0 the finest).
    pub(crate) fn drawn(&self) -> (u32, u64, [u32; MOST_LEVELS]) {
        (self.drawn, self.drawn_vertices, self.drawn_levels)
    }

    /// The copies drawn, their vertices and their levels, from each list's count.
    fn tally(&mut self, counts: &[u32]) {
        (self.drawn, self.drawn_vertices, self.drawn_levels) = (0, 0, [0; MOST_LEVELS]);
        for m in &self.models {
            for (l, level) in m.levels.iter().enumerate() {
                let n = counts.get(m.first + l).copied().unwrap_or(0);
                self.drawn += n;
                self.drawn_vertices += n as u64 * level.vertices as u64;
                self.drawn_levels[l] += n;
            }
        }
    }

    /// The copies of each model again from every map's squares, in the scene's space (each
    /// map's placement added, its dimming given); room for them on the GPU at every level;
    /// the draws again.
    fn gather(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for m in &mut self.models {
            m.copies.clear();
        }
        for (map, squares) in &self.squares {
            let (offset, dim) = self.places.get(map).copied().unwrap_or((Vec3::ZERO, 0.0));
            for list in squares.values() {
                for (key, copies) in list {
                    let Some(&k) = self.index.get(key) else { continue };
                    let m = &mut self.models[k];
                    let room = MOST_COPIES.saturating_sub(m.copies.len());
                    m.copies.extend(copies.iter().take(room).map(|c| GrassCopy { at: (Vec3::from(c.at) + offset).to_array(), dim, ..*c }));
                }
            }
        }
        let mut first = 0;
        for m in &mut self.models {
            m.first = first;
            first += m.levels.len();
        }
        self.counters = first;
        let compute = self.compute.is_some();
        for m in &mut self.models {
            if m.copies.is_empty() {
                continue;
            }
            let need = m.copies.len().next_power_of_two().max(64);
            if m.held.as_ref().is_none_or(|h| h.cap < need) {
                m.held = Some(Held::new(device, need, m.levels.len(), compute));
            }
            if let Some(copies) = m.held.as_ref().and_then(|h| h.copies.as_ref()) {
                queue.write_buffer(copies, 0, bytemuck::cast_slice(&m.copies));
            }
        }
        // each level's copies drawn with its own batches; for the sun, with a thinner level's
        self.draws.clear();
        for (k, m) in self.models.iter().enumerate() {
            if m.copies.is_empty() {
                continue;
            }
            let last = m.levels.len() - 1;
            for list in 0..m.levels.len() {
                for (shadow, mesh) in [(false, list), (true, (list + SHADOW_THINNER).min(last))] {
                    for batch in 0..m.levels[mesh].batches.len() {
                        self.draws.push(Draw { model: k, list, mesh, batch, shadow, counter: m.first + list });
                    }
                }
            }
        }
        if let Some(c) = &mut self.compute {
            c.hold(device, queue, &mut self.models, &self.draws, self.counters);
        }
        self.dirty = false;
    }

    /// The frame's culling: on the GPU (into `encoder`, before the passes that draw), or on
    /// the CPU (each list written for the draws). `size`: the picture's, in pixels.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view_proj: &Mat4, size: [u32; 2]) {
        if self.dirty {
            self.gather(device, queue);
        }
        if self.draws.is_empty() {
            (self.drawn, self.drawn_vertices, self.drawn_levels) = (0, 0, [0; MOST_LEVELS]);
            return;
        }
        let cull = Cull::new(*view_proj, size);
        // a read-back come in (counted before a model came or went: none of it)
        if let Some(counts) = self.compute.as_mut().and_then(|c| c.collect()).filter(|c| c.len() == self.counters) {
            self.tally(&counts);
        }
        if let Some(c) = &mut self.compute {
            queue.write_buffer(&c.params, 0, bytemuck::bytes_of(&cull.uniform()));
            queue.write_buffer(&c.counters, 0, &vec![0u8; self.counters * 4]);
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("grass"), timestamp_writes: None });
                pass.set_pipeline(&c.cull);
                for (k, m) in self.models.iter().enumerate() {
                    let Some(bind) = m.held.as_ref().and_then(|h| h.bind.as_ref()).filter(|_| !m.copies.is_empty()) else { continue };
                    pass.set_bind_group(0, bind, &[(k as u64 * INFO_STRIDE) as u32]);
                    pass.dispatch_workgroups((m.copies.len() as u32).div_ceil(64), 1, 1);
                }
                if let Some(bind) = &c.spread_bind {
                    pass.set_pipeline(&c.spread);
                    pass.set_bind_group(0, bind, &[]);
                    pass.dispatch_workgroups((self.draws.len() as u32).div_ceil(64), 1, 1);
                }
            }
            c.ask(device, encoder, self.counters);
        } else {
            self.counts = vec![0; self.counters];
            let mut lists: Vec<Vec<GrassCopy>> = vec![];
            for m in &self.models {
                let Some(h) = m.held.as_ref().filter(|_| !m.copies.is_empty()) else { continue };
                lists.clear();
                lists.resize(m.levels.len(), vec![]);
                let costs: Vec<f32> = m.levels.iter().map(|l| l.vertices).collect();
                for (i, c) in m.copies.iter().enumerate() {
                    if let Some(l) = pick(&cull, &m.shape, &costs, c, i as u32) {
                        lists[l].push(*c);
                    }
                }
                for (l, list) in lists.iter().enumerate() {
                    self.counts[m.first + l] = list.len() as u32;
                    if !list.is_empty() {
                        queue.write_buffer(&h.out, l as u64 * h.cap as u64 * COPY_BYTES, bytemuck::cast_slice(list));
                    }
                }
            }
            let counts = std::mem::take(&mut self.counts);
            self.tally(&counts);
            self.counts = counts;
        }
    }

    /// Its pipelines drawn once with nothing to see (their shaders made before they're
    /// needed), the pass's buffers bound: the sun's, or the scene's two.
    pub(crate) fn warm_up(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool) {
        let pipelines: &[&wgpu::RenderPipeline] = if shadow { &[&self.shadow] } else { &[&self.opaque, &self.cutout] };
        for p in pipelines {
            pass.set_pipeline(p);
            pass.draw(0..3, 0..1);
        }
    }

    /// The copies into a pass (the sun's, or the scene's solid one), its globals bound at the
    /// scene's own placement (and, in the scene's, the shadow map): each list with its batches.
    pub(crate) fn draw<'t>(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool, texture: &dyn Fn(&str) -> &'t wgpu::BindGroup, stats: &mut DrawStats) {
        // which pipeline is set: the sun's (0), vertex colours (1), cut-outs (2)
        let mut set: Option<u8> = None;
        for (k, d) in self.draws.iter().enumerate().filter(|(_, d)| d.shadow == shadow) {
            let m = &self.models[d.model];
            let Some(h) = &m.held else { continue };
            let n = if self.compute.is_some() { 1 } else { self.counts.get(d.counter).copied().unwrap_or(0) };
            if n == 0 {
                continue;
            }
            let b = &m.levels[d.mesh].batches[d.batch];
            let which = if shadow { 0 } else if b.texture == WHITE { 1 } else { 2 };
            if set != Some(which) {
                pass.set_pipeline([&self.shadow, &self.opaque, &self.cutout][which as usize]);
                set = Some(which);
            }
            pass.set_bind_group(1, texture(&b.texture), &[]);
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            let region = d.list as u64 * h.cap as u64 * COPY_BYTES;
            pass.set_vertex_buffer(1, h.out.slice(region..region + h.cap as u64 * COPY_BYTES));
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            match &self.compute {
                Some(c) => pass.draw_indexed_indirect(&c.args, k as u64 * 20),
                None => pass.draw_indexed(b.first..b.first + b.count, b.base, 0..n),
            }
            if shadow {
                stats.shadow_draws += 1;
            } else {
                stats.draws += 1;
            }
        }
    }

    /// Each list's copies now (tests): on the GPU read back at once.
    #[cfg(test)]
    pub(crate) fn counts_now(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u32> {
        let Some(c) = &self.compute else { return self.counts.clone() };
        if self.counters == 0 {
            return vec![];
        }
        let bytes = self.counters as u64 * 4;
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grass counts now"), size: bytes, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&c.counters, 0, &buf, 0, bytes);
        queue.submit([encoder.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let out = bytemuck::cast_slice::<u8, u32>(&buf.slice(..).get_mapped_range().unwrap()).to_vec();
        out
    }

    /// A model's lists now, each level's copies as the draws read them (tests).
    #[cfg(test)]
    pub(crate) fn lists_now(&self, device: &wgpu::Device, queue: &wgpu::Queue, key: &str) -> Vec<Vec<GrassCopy>> {
        let counts = self.counts_now(device, queue);
        let Some(m) = self.index.get(key).map(|&k| &self.models[k]) else { return vec![] };
        let Some(h) = &m.held else { return vec![] };
        let bytes = h.out.size();
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grass lists now"), size: bytes, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&h.out, 0, &buf, 0, bytes);
        queue.submit([encoder.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let all: Vec<GrassCopy> = bytemuck::cast_slice::<u8, GrassCopy>(&buf.slice(..).get_mapped_range().unwrap()).to_vec();
        (0..m.levels.len()).map(|l| all[l * h.cap..l * h.cap + counts[m.first + l] as usize].to_vec()).collect()
    }

    /// Each model's lists' counters: where its levels' lists start (tests).
    #[cfg(test)]
    pub(crate) fn first_list(&self, key: &str) -> Option<(usize, usize)> {
        self.index.get(key).map(|&k| (self.models[k].first, self.models[k].levels.len()))
    }

    /// Every model's copies, as gathered (tests).
    #[cfg(test)]
    pub(crate) fn copies(&self, key: &str) -> Vec<GrassCopy> {
        self.index.get(key).map(|&k| self.models[k].copies.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An eye 20 cells up and 20 back from a field's south edge (64 x 64, from 0, 0), looking at its middle.
    fn eye() -> Cull {
        let view = Mat4::look_at_rh(Vec3::new(32.0, 20.0, 84.0), Vec3::new(32.0, 0.0, 32.0), Vec3::Y);
        Cull::new(Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 600.0) * view, [1600, 1000])
    }

    const COSTS: [f32; 6] = [4000.0, 2000.0, 1000.0, 500.0, 250.0, 125.0];

    fn level(cull: &Cull, x: f32, z: f32, i: u32) -> Option<usize> {
        let c = GrassCopy { at: [x, 0.0, z], scale: 1.0, ..Default::default() };
        pick(cull, &Shape { middle: Vec3::new(0.0, 0.3, 0.0), reach: 1.0, cells: 1.0, tall: 0.6 }, &COSTS, &c, i)
    }

    #[test]
    fn copies_out_of_sight_are_left_out_and_further_ones_drawn_thinner() {
        let cull = eye();
        // in front of the eye (the bottom of its sight), finer; further along the field, thinner
        let near = level(&cull, 32.0, 52.0, 1).unwrap();
        let mid = level(&cull, 32.0, 32.0, 1).unwrap();
        let far = level(&cull, 32.0, -40.0, 1).unwrap();
        assert!(near < mid && mid < far, "{near} {mid} {far}");
        // just under the bottom of its sight (casting its shadow in): its thinnest
        assert_eq!(level(&cull, 32.0, 62.0, 1), Some(COSTS.len() - 1));
        // behind the eye, past the margin; far off to a side: none
        assert_eq!(level(&cull, 32.0, 84.0 + MARGIN + 3.0, 1), None, "behind the eye");
        assert_eq!(level(&cull, 200.0, 32.0, 1), None, "off to the side");
        // just past the side of the sight, within the margin (a shadow falls in): drawn, at its
        // thinnest
        let edge = (0..400).map(|k| 32.0 + k as f32 * 0.25).find(|&x| level(&cull, x, 50.0, 1).is_none()).unwrap();
        let inside = (0..400).map(|k| 32.0 + k as f32 * 0.25).find(|&x| level(&cull, x, 50.0, 1) == Some(COSTS.len() - 1)).unwrap();
        assert!(edge > inside + MARGIN * 0.8 && inside > 40.0, "seen up to {inside}, drawn up to {edge}");
        // so far a cell is a pixel or two: none
        assert_eq!(level(&cull, 32.0, -2000.0, 1), None);
        // the same copy, the same level every time; copies side by side, a level or the next
        assert_eq!(level(&cull, 32.0, 32.0, 7), level(&cull, 32.0, 32.0, 7));
        let around: Vec<usize> = (0..200).filter_map(|i| level(&cull, 32.0, 20.0, i)).collect();
        let (lo, hi) = (around.iter().min().unwrap(), around.iter().max().unwrap());
        assert!(hi - lo == 1, "where one level gives way to the next differs copy to copy: {lo} to {hi}");
    }

    #[test]
    fn a_copy_never_goes_past_its_last_level_and_a_light_one_stays_at_its_first() {
        let cull = eye();
        let c = GrassCopy { at: [32.0, 0.0, -100.0], scale: 1.0, ..Default::default() };
        let shape = Shape { middle: Vec3::ZERO, reach: 1.0, cells: 1.0, tall: 0.6 };
        assert_eq!(pick(&cull, &shape, &COSTS, &c, 3), Some(COSTS.len() - 1), "its thinnest, however far");
        assert_eq!(pick(&cull, &shape, &[10.0, 5.0], &c, 3), Some(0), "as made, when that's light enough");
        // its middle turned with it: a copy turned half round, its box's middle off to one side
        let off = GrassCopy { at: [70.0, 0.0, 50.0], turn: std::f32::consts::PI, scale: 1.0, ..Default::default() };
        let seen = |middle: Vec3| pick(&cull, &Shape { middle, reach: 0.5, cells: 1.0, tall: 0.6 }, &COSTS, &off, 3).is_some();
        assert!(seen(Vec3::new(30.0, 0.0, 0.0)) && !seen(Vec3::new(-30.0, 0.0, 0.0)), "turned, +30 east is 30 west");
    }

    /// The culling's shader as WebGPU reads it (CI has no GPU to make it on).
    #[test]
    fn the_culling_shader_holds() {
        let src = include_str!("grass.wgsl");
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        assert!(module.entry_points.iter().any(|e| e.name == "cull_copies") && module.entry_points.iter().any(|e| e.name == "spread"));
    }

    #[test]
    fn the_jitter_runs_zero_to_one_as_the_shader_s_does() {
        let all: Vec<f32> = (0..4096).map(jitter).collect();
        assert!(all.iter().all(|j| (0.0..=1.0).contains(j)));
        let mean = all.iter().sum::<f32>() / all.len() as f32;
        assert!((mean - 0.5).abs() < 0.03, "{mean}");
        // the shader's arithmetic, in its order
        let i: u32 = 12345;
        let mut h = i.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277_803_737);
        h ^= h >> 22;
        assert_eq!(jitter(i), (h & 0xffff) as f32 / 65535.0);
    }
}
