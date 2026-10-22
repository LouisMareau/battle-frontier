//! Meadows' blades and flowers (`world::meadow`), made on the GPU as they're drawn
//! (`blades_draw.wgsl`, Ghost of Tsushima's way): each blade a curve from its foot to its tip,
//! made from nothing but which point of its template it is and the blade as placed (a small
//! record, `BladeInstance`), its kind's look (`BladeKind`, all of them in one uniform) saying
//! how tall, wide, leaning, curved and coloured. How fine each is drawn goes by how big it looks
//! from the eye, by how many pixels a cell spans where it stands: from `FULL_PX` up every blade;
//! under it fewer (the share drawn going with the pixels a cell covers, so the picture holds
//! about as many blades whatever the distance, and costs about as much), each kept one wider
//! across (`WIDEST` times at most) and growing in as it comes; under `SHRINK_PX` each shorter,
//! none under `GONE_PX` (the ground's colour carries the meadow there). A blade drawn `FINE_PX`
//! pixels tall or more has seven rows, `MIDDLE_PX` three, else it's one triangle; a flower its
//! petals in full, simpler, or far off a dot of their colour. Culled each frame on the GPU
//! (`blades.wgsl`: each blade drawn to the list of its shape and rows, the draws indirect), or
//! with no compute shaders (WebGL 2) each square's blades drawn as a whole, as many as its
//! nearest point needs (they're sorted by their draw: a blade's own 0 to 1), the drawing leaving
//! out the rest itself. The sun's shadow draws the same blades a template simpler.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::scene::DrawStats;

/// The pixels a cell spans from which every blade is drawn.
pub const FULL_PX: f32 = 40.0;
/// The pixels under which blades are drawn shorter.
pub const SHRINK_PX: f32 = 10.0;
/// The pixels under which none are.
pub const GONE_PX: f32 = 3.0;
/// The most a blade is drawn wider.
pub const WIDEST: f32 = 3.0;
/// The pixels tall from which a blade has its finest rows (seven), and its middle ones (three).
pub const FINE_PX: f32 = 28.0;
pub const MIDDLE_PX: f32 = 9.0;
/// The share past those drawn over which a blade grows in as more are.
pub const FADE: f32 = 0.25;
/// How far past the eye's sight blades are still drawn (cells): their shadows fall into it.
pub const MARGIN: f32 = 2.0;
/// The most kinds of blade (`world::meadow::MOST_KINDS`).
pub const MOST_KINDS: usize = 64;
/// The most blades all the maps' squares have together (more are left out).
pub const MOST_BLADES: usize = 1 << 20;
/// The most blades a list holds in a frame (each shape and rows its own).
const LIST_ROOM: u32 = 1 << 18;
/// The lists: a blade's and a flower's, in each of their three templates.
const LISTS: usize = 6;
/// The kinds' uniform: its two rows of how fine they're drawn, then each kind's six.
const KINDS_BYTES: u64 = 32 + MOST_KINDS as u64 * 96;
const BLADE_BYTES: u64 = std::mem::size_of::<BladeInstance>() as u64;
/// How many cells a side the CPU's culling draws blades by (each patch's of a shape whole).
const PATCH: f32 = 8.0;
/// The points of each template (for the stats): a blade's in 7, 3 and 1 rows, a flower's.
const POINTS: [u64; LISTS] = [15, 7, 3, 43, 31, 4];

/// A blade or a flower placed (`world::meadow::Blade`) as the GPU has it: its foot (in its
/// map's space as it's sent, the scene's once its map is placed), the way it leans (radians
/// about y, as `Mat4::from_rotation_y` turns), its size within its kind's (0 to 1), how dimmed
/// its map is (0: not at all), its kind, and its own random bits.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BladeInstance {
    pub at: [f32; 3],
    pub turn: f32,
    pub size: f32,
    pub dim: f32,
    pub kind: u32,
    pub seed: u32,
}

/// A kind of blade as the GPU has it (`blade_lod.wgsl`'s `Kind`): its shape (0 a blade, 1 a
/// flower), its shortest and tallest (cells) and its width at its foot; how far it leans, how
/// much it curves, how much the wind moves it, a flower's head's width; its colours (linear)
/// at its foot (with a flower's petals) and its tip (with 1 when it's toon), a flower's petals'
/// (with its terrain's rigidity, 0 to 1) and heart's (with how much it glows of itself, 0 to 4).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BladeKind {
    pub form: [f32; 4],
    pub bend: [f32; 4],
    pub foot: [f32; 4],
    pub tip: [f32; 4],
    pub petal: [f32; 4],
    pub heart: [f32; 4],
}

impl BladeKind {
    /// 0 a blade, 1 a flower.
    fn shape(&self) -> usize {
        usize::from(self.form[0] > 0.5)
    }
}

/// A blade's own random bits mixed (as `blade_lod.wgsl`'s `mix_bits`).
pub fn mix_bits(v: u32) -> u32 {
    let s = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277_803_737);
    (w >> 22) ^ w
}

/// 0 to 1 from the low bits: a blade's draw (`seed`).
pub fn unit(v: u32) -> f32 {
    (v & 0xffff) as f32 / 65535.0
}

/// How many pixels a cell spans at one unit of the eye's depth (its clip w), in a picture
/// `height` pixels tall.
pub fn px_scale(view_proj: &Mat4, height: f32) -> f32 {
    0.5 * height * view_proj.row(1).truncate().length()
}

/// How many pixels a cell spans at a depth `w` from the eye.
pub fn cell_px(scale: f32, w: f32) -> f32 {
    scale / w.max(1e-4)
}

/// The share of the blades drawn where a cell spans `px` pixels.
pub fn keep_at(px: f32) -> f32 {
    let k = (px / FULL_PX).clamp(0.0, 1.0);
    k * k
}

/// Whether a blade (its own random bits `seed`) is drawn where a cell spans `px` pixels.
pub fn kept(px: f32, seed: u32) -> bool {
    px >= GONE_PX && unit(seed) < keep_at(px) * (1.0 + FADE)
}

/// How tall a blade is drawn there against as placed: growing in as it comes, shorter at the
/// furthest.
pub fn tallness(px: f32, seed: u32) -> f32 {
    let keep = keep_at(px);
    let band = keep * FADE;
    let grown = ((keep + band - unit(seed)) / band.max(1e-5)).clamp(0.0, 1.0);
    let t = ((px - GONE_PX) / (SHRINK_PX - GONE_PX)).clamp(0.0, 1.0);
    grown * t * t * (3.0 - 2.0 * t)
}

/// How much wider a blade is drawn there.
pub fn widened(px: f32) -> f32 {
    (1.0 / keep_at(px).max(1e-6).sqrt()).min(WIDEST)
}

/// A blade's template by how many pixels tall it's drawn: 0 its finest, 1 its middle, 2 its
/// simplest.
pub fn tier_of(tall_px: f32) -> usize {
    if tall_px >= FINE_PX {
        0
    } else if tall_px >= MIDDLE_PX {
        1
    } else {
        2
    }
}

/// The eye's view for the culling: its view and projection, its sight's planes (each normal a
/// unit, pointing in, pushed out by `MARGIN`), and how many pixels a cell spans at one unit
/// of its depth.
#[derive(Clone, Copy, Debug)]
pub struct Sight {
    pub view_proj: Mat4,
    pub planes: [Vec4; 6],
    pub scale: f32,
}

impl Sight {
    /// For a picture `size` pixels.
    pub fn new(view_proj: Mat4, size: [u32; 2]) -> Sight {
        let (r0, r1, r2, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| {
            let p = p / p.truncate().length().max(1e-9);
            Vec4::new(p.x, p.y, p.z, p.w + MARGIN)
        });
        Sight { view_proj, planes, scale: px_scale(&view_proj, size[1].max(1) as f32) }
    }
}

/// The list a blade is drawn from this frame (its shape's and its template's: shape × 3 +
/// template), or none when it isn't drawn (out of sight, too far off, one of the share left
/// out there). As `blades.wgsl`'s `cull_blades` picks it.
pub fn pick(sight: &Sight, kinds: &[BladeKind], b: &BladeInstance) -> Option<usize> {
    let k = kinds.get(b.kind as usize).copied().unwrap_or_default();
    let tall = k.form[1] + (k.form[2] - k.form[1]) * b.size;
    let mid = Vec3::from(b.at) + Vec3::Y * tall * 0.5;
    let reach = tall * 0.75 + k.bend[3] + 0.3;
    if sight.planes.iter().any(|p| p.truncate().dot(mid) + p.w < -reach) {
        return None;
    }
    let px = cell_px(sight.scale, (sight.view_proj * mid.extend(1.0)).w);
    if !kept(px, b.seed) {
        return None;
    }
    Some(k.shape() * 3 + tier_of(tall * tallness(px, b.seed) * px))
}

/// The templates (`blades_draw.wgsl`'s `point_of` numbers their points): a blade in 7, 3 and 1
/// rows; a flower in full, simpler, and far off its head alone. Their triangles' points one
/// after another, and where each template's start and how many.
fn templates() -> (Vec<u32>, [(u32, u32); LISTS]) {
    fn strip(out: &mut Vec<u32>, base: u32, rows: u32) {
        for r in 0..rows - 1 {
            let k = base + 2 * r;
            out.extend([k, k + 1, k + 3, k, k + 3, k + 2]);
        }
        let k = base + 2 * (rows - 1);
        out.extend([k, k + 1, base + 2 * rows]);
    }
    fn quad(out: &mut Vec<u32>, b: u32) {
        out.extend([b, b + 1, b + 3, b, b + 3, b + 2]);
    }
    let mut out = vec![];
    let mut spans = [(0, 0); LISTS];
    let mut span = |out: &Vec<u32>, first: usize, k: usize| spans[k] = (first as u32, (out.len() - first) as u32);
    for (k, (base, rows)) in [(0, 7), (16, 3), (24, 1)].into_iter().enumerate() {
        let first = out.len();
        strip(&mut out, base, rows);
        span(&out, first, k);
    }
    let first = out.len();
    strip(&mut out, 32, 3);
    for j in 0..8 {
        quad(&mut out, 40 + 4 * j);
    }
    quad(&mut out, 72);
    span(&out, first, 3);
    let first = out.len();
    strip(&mut out, 80, 1);
    for j in 0..8 {
        let b = 84 + 3 * j;
        out.extend([b, b + 1, b + 2]);
    }
    quad(&mut out, 108);
    span(&out, first, 4);
    let first = out.len();
    quad(&mut out, 112);
    span(&out, first, 5);
    (out, spans)
}

/// The template a list is drawn with: for the eye its own, for the sun a simpler one.
fn template(list: usize, shadow: bool) -> usize {
    if shadow {
        list / 3 * 3 + (list % 3 + 1).min(2)
    } else {
        list
    }
}

/// One patch's blades of one shape (`PATCH` cells a side), where they are in the buffer, sorted
/// by their draw (the CPU's culling draws each as a whole, as many as its nearest point needs):
/// its first, their draws, its shape, the box round them, the tallest.
struct Run {
    first: u32,
    draws: Vec<f32>,
    shape: usize,
    lo: Vec3,
    hi: Vec3,
    tall: f32,
}

/// The culling's settings as the compute shader reads them (`Cull` there).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CullUniform {
    view_proj: [[f32; 4]; 4],
    planes: [[f32; 4]; 6],
    count: [u32; 4],
}

/// The compute shaders and what they read and write, where the GPU has them: the frame's
/// settings, the lists (room for `LIST_ROOM` each) and their counters, the draws (as
/// `DrawIndexedIndirectArgs`, their counts written by `spread_blades`: for the eye and for the
/// sun, each list's) with their lists, and the counters read back now and then (the stats).
struct Compute {
    cull: wgpu::ComputePipeline,
    spread: wgpu::ComputePipeline,
    cull_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    lists: wgpu::Buffer,
    counters: wgpu::Buffer,
    args: wgpu::Buffer,
    cull_bind: Option<wgpu::BindGroup>,
    spread_bind: wgpu::BindGroup,
    readback: wgpu::Buffer,
    reading: Option<Arc<AtomicU8>>,
}

impl Compute {
    fn new(device: &wgpu::Device) -> Compute {
        let source = format!("{}{}", include_str!("blade_lod.wgsl"), include_str!("blades.wgsl"));
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("blades"), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let storage = |read_only: bool| wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None };
        let uniform = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None };
        let cull_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blades cull"),
            entries: &[entry(0, uniform), entry(1, uniform), entry(2, storage(true)), entry(3, storage(false)), entry(4, storage(false))],
        });
        let spread_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blades spread"),
            entries: &[entry(0, uniform), entry(5, storage(true)), entry(6, storage(true)), entry(7, storage(false))],
        });
        let pipeline = |layout: &wgpu::BindGroupLayout, entry_point: &str| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(entry_point), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry_point), layout: Some(&layout), module: &module, entry_point: Some(entry_point), compilation_options: Default::default(), cache: None })
        };
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });
        let params = buffer("blades cull", std::mem::size_of::<CullUniform>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        let lists = buffer("blades lists", LISTS as u64 * LIST_ROOM as u64 * 4, wgpu::BufferUsages::STORAGE);
        let counters = buffer("blades counters", LISTS as u64 * 4, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC);
        // the draws: for the eye each list with its own template, then for the sun with a
        // simpler one
        let (_, spans) = templates();
        let mut args: Vec<u32> = vec![];
        let mut draw_lists: Vec<u32> = vec![];
        for shadow in [false, true] {
            for list in 0..LISTS {
                let (first, count) = spans[template(list, shadow)];
                args.extend([count, 0, first, 0, 0]);
                draw_lists.push(list as u32);
            }
        }
        let args = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blades draws"), contents: bytemuck::cast_slice(&args), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT });
        let draw_lists = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blades draws' lists"), contents: bytemuck::cast_slice(&draw_lists), usage: wgpu::BufferUsages::STORAGE });
        let spread_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blades spread"),
            layout: &spread_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: counters.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: draw_lists.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: args.as_entire_binding() },
            ],
        });
        let readback = buffer("blades counts", LISTS as u64 * 4, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        Compute { cull: pipeline(&cull_layout, "cull_blades"), spread: pipeline(&spread_layout, "spread_blades"), cull_layout, params, lists, counters, args, cull_bind: None, spread_bind, readback, reading: None }
    }

    /// The culling's bind group again, for the blades as they're held now.
    fn bind(&mut self, device: &wgpu::Device, kinds: &wgpu::Buffer, blades: &wgpu::Buffer) {
        self.cull_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blades cull"),
            layout: &self.cull_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: kinds.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: blades.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: self.lists.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: self.counters.as_entire_binding() },
            ],
        }));
    }

    /// The counters read back, once one asked for has come.
    fn collect(&mut self) -> Option<Vec<u32>> {
        let state = self.reading.as_ref()?.load(Ordering::Acquire);
        match state {
            0 => None,
            1 => {
                self.reading = None;
                let counts = self.readback.slice(..).get_mapped_range().ok().map(|view| bytemuck::cast_slice::<u8, u32>(&view).to_vec());
                self.readback.unmap();
                counts
            }
            _ => {
                self.reading = None;
                None
            }
        }
    }

    /// Asks for the counters as this frame leaves them, unless a read-back is on its way.
    fn ask(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.reading.is_some() {
            return;
        }
        encoder.copy_buffer_to_buffer(&self.counters, 0, &self.readback, 0, LISTS as u64 * 4);
        let flag = Arc::new(AtomicU8::new(0));
        let done = flag.clone();
        encoder.map_buffer_on_submit(&self.readback, wgpu::MapMode::Read, .., move |r| done.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release));
        self.reading = Some(flag);
    }
}

/// The pipelines blades are drawn with: as instances (each square's, culled by the drawing
/// itself) for the eye and the sun, and, with compute shaders, from the culling's lists.
pub(crate) struct BladePipelines {
    pub main: wgpu::RenderPipeline,
    pub shadow: wgpu::RenderPipeline,
    pub listed: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

/// The meadows' blades the scene draws: their kinds, every world map's blades square by
/// square, and how they're drawn this frame.
pub(crate) struct Blades {
    pipelines: BladePipelines,
    kinds: Vec<BladeKind>,
    kinds_buffer: wgpu::Buffer,
    kinds_bind: wgpu::BindGroup,
    templates: wgpu::Buffer,
    spans: [(u32, u32); LISTS],
    store_layout: Option<wgpu::BindGroupLayout>,
    store_bind: Option<wgpu::BindGroup>,
    compute: Option<Compute>,
    /// per world map and square, its blades (the map's own space)
    squares: BTreeMap<String, BTreeMap<(i32, i32), Vec<BladeInstance>>>,
    /// each map's placement as last told: where its space sits, how dimmed
    places: BTreeMap<String, (Vec3, f32)>,
    /// the blades to gather again (a square, a placement or a kind's shape changed)
    dirty: bool,
    /// every blade gathered, on the GPU (room for `cap`), and how many
    buffer: wgpu::Buffer,
    cap: usize,
    count: u32,
    runs: Vec<Run>,
    /// on the CPU: each run's blades drawn this frame, and its template
    frame: Vec<(u32, usize)>,
    /// the blades drawn as last known, their vertices, and how many with each template
    drawn: u32,
    drawn_vertices: u64,
    drawn_lists: [u32; LISTS],
}

impl Blades {
    /// Whether the GPU culls the blades: it has compute shaders, and storage buffers the
    /// drawing reads.
    pub(crate) fn can_compute(device: &wgpu::Device) -> bool {
        let l = device.limits();
        l.max_compute_workgroups_per_dimension > 0 && l.max_compute_workgroup_size_x >= 64 && l.max_storage_buffers_per_shader_stage >= 3
    }

    /// The bind group layouts the blades' pipelines need: their kinds (group 1), and where
    /// the GPU culls them (`can_compute`: the drawing reads storage too, which WebGL 2 can't)
    /// the blades and a list's (group 3).
    pub(crate) fn layouts(device: &wgpu::Device) -> (wgpu::BindGroupLayout, Option<wgpu::BindGroupLayout>) {
        let kinds = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blade kinds"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(KINDS_BYTES) },
                count: None,
            }],
        });
        let storage = |binding: u32, dynamic: bool, size: Option<wgpu::BufferSize>| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: dynamic, min_binding_size: size },
            count: None,
        };
        let store = Self::can_compute(device).then(|| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("blades held"), entries: &[storage(1, false, None), storage(2, true, wgpu::BufferSize::new(LIST_ROOM as u64 * 4))] }));
        (kinds, store)
    }

    pub(crate) fn new(device: &wgpu::Device, pipelines: BladePipelines, kinds_layout: &wgpu::BindGroupLayout, store_layout: Option<wgpu::BindGroupLayout>) -> Blades {
        let kinds_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("blade kinds"), size: KINDS_BYTES, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let kinds_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("blade kinds"), layout: kinds_layout, entries: &[wgpu::BindGroupEntry { binding: 6, resource: kinds_buffer.as_entire_binding() }] });
        let (indices, spans) = templates();
        let templates = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blade templates"), contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX });
        let compute = (pipelines.listed.is_some() && store_layout.is_some()).then(|| Compute::new(device));
        let cap = 1024;
        let buffer = Self::make_buffer(device, cap, compute.is_some());
        let mut b = Blades {
            pipelines,
            kinds: vec![],
            kinds_buffer,
            kinds_bind,
            templates,
            spans,
            store_layout,
            store_bind: None,
            compute,
            squares: BTreeMap::new(),
            places: BTreeMap::new(),
            dirty: false,
            buffer,
            cap,
            count: 0,
            runs: vec![],
            frame: vec![],
            drawn: 0,
            drawn_vertices: 0,
            drawn_lists: [0; LISTS],
        };
        b.rebind(device);
        b
    }

    fn make_buffer(device: &wgpu::Device, cap: usize, storage: bool) -> wgpu::Buffer {
        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | if storage { wgpu::BufferUsages::STORAGE } else { wgpu::BufferUsages::empty() };
        device.create_buffer(&wgpu::BufferDescriptor { label: Some("blades"), size: cap as u64 * BLADE_BYTES, usage, mapped_at_creation: false })
    }

    /// The bind groups that hold the blades' buffer, again (it's new).
    fn rebind(&mut self, device: &wgpu::Device) {
        let (Some(c), Some(layout)) = (&mut self.compute, &self.store_layout) else { return };
        c.bind(device, &self.kinds_buffer, &self.buffer);
        self.store_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blades held"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 1, resource: self.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &c.lists, offset: 0, size: wgpu::BufferSize::new(LIST_ROOM as u64 * 4) }) },
            ],
        }));
    }

    pub(crate) fn culled_on_gpu(&self) -> bool {
        self.compute.is_some()
    }

    /// Culled on the CPU from now on (as where there are no compute shaders).
    #[cfg(test)]
    pub(crate) fn on_cpu(&mut self) {
        self.compute = None;
        self.dirty = true;
    }

    /// Every meadow's kinds (`world::meadow::Kinds`, in its order; past `MOST_KINDS` left out).
    pub(crate) fn set_kinds(&mut self, kinds: &[BladeKind]) {
        let kinds = &kinds[..kinds.len().min(MOST_KINDS)];
        if kinds == self.kinds.as_slice() {
            return;
        }
        // (a kind of another shape: its blades in other runs)
        let shapes = |k: &[BladeKind]| k.iter().map(BladeKind::shape).collect::<Vec<_>>();
        self.dirty |= shapes(kinds) != shapes(&self.kinds);
        self.kinds = kinds.to_vec();
    }

    /// One square's blades on a map, replacing what it held (none: it goes).
    pub(crate) fn set_blades(&mut self, map: &str, square: (i32, i32), blades: Vec<BladeInstance>) {
        let squares = self.squares.entry(map.to_string()).or_default();
        if blades.is_empty() {
            self.dirty |= squares.remove(&square).is_some();
        } else if squares.get(&square) != Some(&blades) {
            squares.insert(square, blades);
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

    /// The blades drawn, as last known (on the GPU, read back a few frames late), their
    /// vertices, and how many with each template (a blade's three, then a flower's).
    pub(crate) fn drawn(&self) -> (u32, u64, [u32; LISTS]) {
        (self.drawn, self.drawn_vertices, self.drawn_lists)
    }

    fn tally(&mut self, counts: &[u32]) {
        self.drawn_lists = [0; LISTS];
        for (k, n) in counts.iter().take(LISTS).enumerate() {
            self.drawn_lists[k] = (*n).min(LIST_ROOM);
        }
        self.drawn = self.drawn_lists.iter().sum();
        self.drawn_vertices = self.drawn_lists.iter().zip(POINTS).map(|(&n, p)| n as u64 * p).sum();
    }

    /// Every map's blades again, in the scene's space (each map's placement added, its
    /// dimming given), each square's of one shape a run sorted by their draw; on the GPU.
    fn gather(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let shape = |b: &BladeInstance| self.kinds.get(b.kind as usize).map(BladeKind::shape).unwrap_or(0);
        let tallest = |b: &BladeInstance| self.kinds.get(b.kind as usize).map(|k| k.form[2] + k.bend[3]).unwrap_or(0.0);
        let mut all: Vec<BladeInstance> = vec![];
        let mut runs = vec![];
        for (map, squares) in &self.squares {
            let (offset, dim) = self.places.get(map).copied().unwrap_or((Vec3::ZERO, 0.0));
            for blades in squares.values() {
                // by shape and patch
                let mut patches: BTreeMap<(usize, i32, i32), Vec<BladeInstance>> = BTreeMap::new();
                for b in blades.iter().take(MOST_BLADES.saturating_sub(all.len())) {
                    let key = (shape(b), (b.at[0] / PATCH).floor() as i32, (b.at[2] / PATCH).floor() as i32);
                    patches.entry(key).or_default().push(BladeInstance { at: (Vec3::from(b.at) + offset).to_array(), dim, ..*b });
                }
                for ((s, _, _), mut these) in patches {
                    these.sort_by(|a, b| unit(a.seed).total_cmp(&unit(b.seed)));
                    let (mut lo, mut hi, mut tall) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0.0f32);
                    for b in &these {
                        lo = lo.min(Vec3::from(b.at));
                        hi = hi.max(Vec3::from(b.at));
                        tall = tall.max(tallest(b));
                    }
                    runs.push(Run { first: all.len() as u32, draws: these.iter().map(|b| unit(b.seed)).collect(), shape: s, lo, hi, tall });
                    all.extend(these);
                }
            }
        }
        if all.len() > self.cap {
            self.cap = all.len().next_power_of_two();
            self.buffer = Self::make_buffer(device, self.cap, self.compute.is_some());
            self.rebind(device);
        }
        if !all.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&all));
        }
        self.count = all.len() as u32;
        self.runs = runs;
        self.dirty = false;
    }

    /// The frame's culling: on the GPU (into `encoder`, before the passes that draw), or on
    /// the CPU (each run's count and template). `size`: the picture's, in pixels.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view_proj: &Mat4, size: [u32; 2]) {
        if self.dirty {
            self.gather(device, queue);
        }
        let sight = Sight::new(*view_proj, size);
        // the kinds, and how fine they're drawn this frame
        let mut bytes: Vec<u8> = Vec::with_capacity(KINDS_BYTES as usize);
        bytes.extend_from_slice(bytemuck::cast_slice(&[sight.scale, FULL_PX, SHRINK_PX, GONE_PX, WIDEST, FINE_PX, MIDDLE_PX, FADE]));
        let mut kinds = self.kinds.clone();
        kinds.resize(MOST_KINDS, BladeKind::default());
        bytes.extend_from_slice(bytemuck::cast_slice(&kinds));
        queue.write_buffer(&self.kinds_buffer, 0, &bytes);
        if self.count == 0 {
            (self.drawn, self.drawn_vertices, self.drawn_lists) = (0, 0, [0; LISTS]);
            self.frame.clear();
            return;
        }
        if self.compute.is_some() {
            if let Some(counts) = self.compute.as_mut().and_then(Compute::collect) {
                self.tally(&counts);
            }
            let Some(c) = self.compute.as_mut() else { return };
            let u = CullUniform { view_proj: view_proj.to_cols_array_2d(), planes: sight.planes.map(|p| p.to_array()), count: [self.count, LIST_ROOM, 0, 0] };
            queue.write_buffer(&c.params, 0, bytemuck::bytes_of(&u));
            queue.write_buffer(&c.counters, 0, &[0u8; LISTS * 4]);
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("blades"), timestamp_writes: None });
                if let Some(bind) = &c.cull_bind {
                    pass.set_pipeline(&c.cull);
                    pass.set_bind_group(0, bind, &[]);
                    pass.dispatch_workgroups(self.count.div_ceil(64), 1, 1);
                }
                pass.set_pipeline(&c.spread);
                pass.set_bind_group(0, &c.spread_bind, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            c.ask(encoder);
            return;
        }
        // on the CPU: each run as many of its blades as its nearest point needs, with the
        // finest template a blade there needs
        let eye = crate::scene::Frustum::new(view_proj);
        let mut counts = [0u32; LISTS];
        self.frame.clear();
        for run in &self.runs {
            let pad = Vec3::splat(run.tall + MARGIN);
            if !eye.sees(run.lo - pad, run.hi + pad + Vec3::Y * run.tall) {
                self.frame.push((0, 0));
                continue;
            }
            let near = (0..8).map(|k| Vec3::new([run.lo.x, run.hi.x][k & 1], [run.lo.y, run.hi.y + run.tall][(k >> 1) & 1], [run.lo.z, run.hi.z][k >> 2])).map(|p| (sight.view_proj * p.extend(1.0)).w).fold(f32::MAX, f32::min);
            let px = cell_px(sight.scale, near);
            let keep = if px >= GONE_PX { keep_at(px) * (1.0 + FADE) } else { 0.0 };
            let n = run.draws.partition_point(|&d| d < keep) as u32;
            let tier = tier_of(run.tall * px);
            counts[run.shape * 3 + tier] += n;
            self.frame.push((n, tier));
        }
        self.tally(&counts);
    }

    /// Its pipelines drawn once with nothing to see (their shaders made before they're
    /// needed), the pass's globals bound: the sun's, or the scene's.
    pub(crate) fn warm_up(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool, zeros: &wgpu::Buffer) {
        pass.set_bind_group(1, &self.kinds_bind, &[]);
        pass.set_vertex_buffer(0, zeros.slice(..));
        pass.set_pipeline(if shadow { &self.pipelines.shadow } else { &self.pipelines.main });
        pass.draw(0..3, 0..1);
        if let (Some((main, sun)), Some(bind)) = (&self.pipelines.listed, &self.store_bind) {
            pass.set_bind_group(3, bind, &[0]);
            pass.set_pipeline(if shadow { sun } else { main });
            pass.draw(0..3, 0..1);
        }
    }

    /// The blades into a pass (the sun's, or the scene's solid one), its globals bound at the
    /// scene's own placement (and, in the scene's, the shadow map).
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool, stats: &mut DrawStats) {
        if self.count == 0 {
            return;
        }
        pass.set_bind_group(1, &self.kinds_bind, &[]);
        pass.set_index_buffer(self.templates.slice(..), wgpu::IndexFormat::Uint32);
        match (&self.compute, &self.pipelines.listed, &self.store_bind) {
            (Some(c), Some((main, sun)), Some(bind)) => {
                pass.set_pipeline(if shadow { sun } else { main });
                for list in 0..LISTS {
                    pass.set_bind_group(3, bind, &[list as u32 * LIST_ROOM * 4]);
                    let draw = list + if shadow { LISTS } else { 0 };
                    pass.draw_indexed_indirect(&c.args, draw as u64 * 20);
                    if shadow {
                        stats.shadow_draws += 1;
                    } else {
                        stats.draws += 1;
                    }
                }
            }
            _ => {
                pass.set_pipeline(if shadow { &self.pipelines.shadow } else { &self.pipelines.main });
                for (run, &(n, tier)) in self.runs.iter().zip(&self.frame) {
                    if n == 0 {
                        continue;
                    }
                    let (first, count) = self.spans[template(run.shape * 3 + tier, shadow)];
                    let at = run.first as u64 * BLADE_BYTES;
                    pass.set_vertex_buffer(0, self.buffer.slice(at..at + n as u64 * BLADE_BYTES));
                    pass.draw_indexed(first..first + count, 0, 0..n);
                    if shadow {
                        stats.shadow_draws += 1;
                    } else {
                        stats.draws += 1;
                    }
                }
            }
        }
    }

    /// Each list's blades now (tests): on the GPU read back at once.
    #[cfg(test)]
    pub(crate) fn counts_now(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> [u32; LISTS] {
        let Some(c) = &self.compute else { return self.drawn_lists };
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("blades counts now"), size: LISTS as u64 * 4, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&c.counters, 0, &buf, 0, LISTS as u64 * 4);
        queue.submit([encoder.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let v = bytemuck::cast_slice::<u8, u32>(&buf.slice(..).get_mapped_range().unwrap()).to_vec();
        std::array::from_fn(|k| v[k])
    }

    /// Every blade, as gathered (tests).
    #[cfg(test)]
    pub(crate) fn gathered(&self) -> Vec<BladeInstance> {
        self.squares.iter().flat_map(|(map, s)| {
            let (offset, dim) = self.places.get(map).copied().unwrap_or((Vec3::ZERO, 0.0));
            s.values().flatten().map(move |b| BladeInstance { at: (Vec3::from(b.at) + offset).to_array(), dim, ..*b })
        }).collect()
    }

    /// The kinds as held (tests).
    #[cfg(test)]
    pub(crate) fn kinds(&self) -> &[BladeKind] {
        &self.kinds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An eye 20 cells up and 20 back from a field's south edge (64 x 64, from 0, 0), looking at
    /// its middle.
    fn eye() -> Sight {
        let view = Mat4::look_at_rh(Vec3::new(32.0, 20.0, 84.0), Vec3::new(32.0, 0.0, 32.0), Vec3::Y);
        Sight::new(Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 600.0) * view, [1600, 1000])
    }

    fn grass() -> BladeKind {
        BladeKind { form: [0.0, 0.3, 0.6, 0.035], bend: [0.3, 0.4, 1.0, 0.0], ..Default::default() }
    }

    #[test]
    fn a_cell_s_pixels_are_as_the_picture_has_them() {
        let s = eye();
        // a cell at the field's middle, measured on the picture, against the scale's
        let at = |p: Vec3| {
            let c = s.view_proj * p.extend(1.0);
            glam::Vec2::new(c.x / c.w, c.y / c.w) * glam::Vec2::new(800.0, 500.0)
        };
        let p = Vec3::new(32.0, 0.0, 32.0);
        let up = (at(p + Vec3::Y) - at(p)).length();
        let px = cell_px(s.scale, (s.view_proj * p.extend(1.0)).w);
        assert!((up / px - 1.0).abs() < 0.08, "a cell up: {up} pixels, the scale says {px}");
    }

    #[test]
    fn further_off_fewer_blades_wider_and_shorter_never_any_past_the_last() {
        // the share drawn: all up close, falling with the cell's pixels squared
        assert_eq!(keep_at(FULL_PX * 2.0), 1.0);
        assert!((keep_at(FULL_PX / 2.0) - 0.25).abs() < 1e-6);
        let drawn = |px: f32| (0..20_000u32).map(mix_bits).filter(|&s| kept(px, s)).count() as f32 / 20_000.0;
        assert!((drawn(FULL_PX * 2.0) - 1.0).abs() < 1e-6, "all of them up close");
        let half = drawn(FULL_PX / 2.0);
        assert!((half - 0.25 * (1.0 + FADE)).abs() < 0.02, "a quarter (and those growing in): {half}");
        assert_eq!(drawn(GONE_PX * 0.9), 0.0, "none past the last");
        // as tall as placed up close (every one), wider and shorter as they go
        assert!((0..2000u32).map(mix_bits).all(|s| tallness(FULL_PX * 1.5, s) == 1.0));
        assert_eq!(widened(FULL_PX), 1.0);
        assert!((widened(FULL_PX / 2.0) - 2.0).abs() < 1e-4 && widened(1.0) == WIDEST);
        let mean = |px: f32| (0..4000u32).map(mix_bits).filter(|&s| kept(px, s)).map(|s| tallness(px, s)).sum::<f32>() / 4000.0;
        assert!(mean(SHRINK_PX * 0.6) < mean(SHRINK_PX * 1.2) * 0.75, "shorter at the furthest");
        // those growing in: short when they come, as tall as the share drawn grows
        let s = (0..u32::MAX).map(mix_bits).find(|&s| (unit(s) - 0.3).abs() < 0.001).unwrap();
        let px_at = |share: f32| (share.sqrt() * FULL_PX).max(SHRINK_PX);
        assert!(tallness(px_at(0.3 / (1.0 + FADE)), s) < 0.05 && tallness(px_at(0.3), s) > 0.9);
    }

    /// An eye at `from`, looking at the field's middle.
    fn looking(from: Vec3) -> Sight {
        let view = Mat4::look_at_rh(from, Vec3::new(32.0, 0.0, 32.0), Vec3::Y);
        Sight::new(Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 900.0) * view, [1600, 1000])
    }

    #[test]
    fn the_culling_picks_by_how_big_they_look_and_leaves_out_what_s_out_of_sight() {
        let kinds = [BladeKind { form: [0.0, 0.6, 1.0, 0.035], ..grass() }, BladeKind { form: [1.0, 0.3, 0.5, 0.012], bend: [0.1, 0.2, 1.0, 0.1], ..Default::default() }];
        let at = |x: f32, z: f32, seed: u32| BladeInstance { at: [x, 0.0, z], size: 0.5, seed, ..Default::default() };
        // up close: every one, the finest; from far off (the few kept): the simplest
        let (near, far) = (looking(Vec3::new(32.0, 6.0, 40.0)), looking(Vec3::new(32.0, 160.0, 140.0)));
        let close: Vec<usize> = (0..200u32).filter_map(|k| pick(&near, &kinds, &at(32.0, 32.0, mix_bits(k)))).collect();
        assert!(close.len() == 200 && close.iter().all(|&l| l == 0), "{close:?}");
        let off: Vec<usize> = (0..400u32).filter_map(|k| pick(&far, &kinds, &at(32.0, 32.0, mix_bits(k)))).collect();
        assert!(!off.is_empty() && off.len() < 60 && off.iter().all(|&l| l == 2), "{} kept far off: {off:?}", off.len());
        // a flower: its own lists
        assert_eq!(pick(&near, &kinds, &BladeInstance { kind: 1, ..at(32.0, 32.0, 5) }), Some(3));
        // behind the eye, far off to a side: none
        assert_eq!(pick(&near, &kinds, &at(32.0, 46.0, 1)), None);
        assert_eq!(pick(&near, &kinds, &at(300.0, 32.0, 1)), None);
    }

    #[test]
    fn the_templates_hold_their_points() {
        let (indices, spans) = templates();
        assert_eq!(spans.iter().map(|s| s.1 / 3).collect::<Vec<_>>(), [13, 5, 1, 23, 11, 2], "triangles each");
        // each template's points within its own numbers, as `point_of` has them
        let ranges = [(0, 15), (16, 23), (24, 27), (32, 76), (80, 112), (112, 116)];
        for (k, &(first, count)) in spans.iter().enumerate() {
            let points = &indices[first as usize..(first + count) as usize];
            assert!(points.iter().all(|&p| p >= ranges[k].0 && p < ranges[k].1), "template {k}: {points:?}");
            let mut seen: Vec<u32> = points.to_vec();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len() as u64, POINTS[k], "template {k}'s points");
        }
        // the sun's: a simpler one
        assert_eq!((template(0, true), template(2, true), template(3, true), template(5, true), template(4, false)), (1, 2, 4, 5, 4));
    }

    #[test]
    fn the_shaders_hold() {
        let source = format!("{}{}", include_str!("blade_lod.wgsl"), include_str!("blades.wgsl"));
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        assert!(module.entry_points.iter().any(|e| e.name == "cull_blades") && module.entry_points.iter().any(|e| e.name == "spread_blades"));
    }

    #[test]
    fn the_random_bits_mix_as_the_shader_s_do() {
        let all: Vec<f32> = (0..4096).map(|k| unit(mix_bits(k))).collect();
        let mean = all.iter().sum::<f32>() / all.len() as f32;
        assert!((mean - 0.5).abs() < 0.03, "{mean}");
        // the shader's arithmetic, in its order
        let v: u32 = 12345;
        let s = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277_803_737);
        assert_eq!(mix_bits(v), (w >> 22) ^ w);
    }
}
