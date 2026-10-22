//! The GPU side: pipelines, textures, the map's meshes, and each frame's sprites and
//! effects. The scene is drawn into its own colour + depth target (linear light past white;
//! with the sun's shadow map first, and the indirect light's height pass and cascades, `gi`):
//! what's solid first, writing its look too (toon or not, how far, which way it faces) to a
//! second target and what shines of it to a third, then what's see-through; the bloom of what
//! shines blurred from the third; then blitted into whatever pass the host gives (egui's, in
//! the game), the bloom added, exposed and tone mapped, with the toon look's ink lines found
//! in the looks, and the pixelation and an old handheld's LCD colours if the game asks; a
//! small blurred copy of the picture besides, for frosted glass over it (`frost_view`). The
//! grass grown through tall ground is drawn as copies of its models, culled each frame
//! (`grass`); the meadows' blades and flowers are made on the GPU as they're drawn (`blades`).

use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::blades::{BladeInstance, BladeKind, BladePipelines, Blades};
use crate::gi::{Gi, Region};
use crate::grass::{Grass, GrassCopy, GrassModel};
use crate::map::{build_meshes, MapScene, MapVertex, Spot};

/// The scene's colour: linear, past 1 where a glow or a lamp's hot spot is (the blit
/// exposes and tone maps it to the screen's range).
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// What solid things write as shining (rgb, lit by nothing; a: the ground's), for the bloom
/// and the indirect light; the bloom's chain is of the same.
const EMIT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The bloom's chain: this many levels from half the picture's size, each half the last, at most.
const BLOOM_MIPS: u32 = 5;
/// What solid things write next to their colour, for the ink lines: toon or not, how far,
/// which way they face (see `look` in the shader).
const LOOK: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Uint;
/// The frosted glass's blurred copy of the picture: what egui reads (its values the screen's).
const FROST: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// ... a texel of it for this many pixels of the screen, each way.
const FROST_DIV: u32 = 6;
/// The sun's shadow map, texels a side, unless `Quality` says otherwise.
pub const SHADOW_SIZE: u32 = 2048;
/// World meshes are cut into squares this many cells a side, each drawn only when the
/// camera (or, for the shadow, the sun) can see it.
const CHUNK: f32 = 16.0;
/// Bytes between two world maps' placements in their uniform buffer: a dynamic offset's
/// alignment.
const PLACE_STRIDE: u64 = 256;

/// Where a world map's own space sits in the scene: moved by `offset` (cells; the map the
/// camera is on sits at none), its north-west corner on the world's grid at `grid` (so the
/// ground's patterns run on from one map into the next), dimmed by `dim` (0: not at all).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    pub offset: Vec3,
    pub grid: [f32; 2],
    pub dim: f32,
}

/// A placement as the shader reads it (`Place` there).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PlaceUniform {
    offset: [f32; 4],
    grid: [f32; 4],
}

/// How much the GPU draws: the scene's resolution against the screen's, and the sun's
/// shadow map's size (0: no shadows). For machines that can't keep up at full quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    /// 0.25 to 1
    pub scale: f32,
    pub shadow_size: u32,
    /// the indirect light of what glows (the radiance cascades, each frame; WebGPU only)
    pub indirect: bool,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { scale: 1.0, shadow_size: SHADOW_SIZE, indirect: true }
    }
}

/// The picture's light (the world settings' `light`): how it's exposed and tone mapped to
/// the screen, the bloom of what shines, and the indirect light of what glows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radiance {
    /// the colours times this before the tone map (1: as they are)
    pub exposure: f32,
    /// a filmic tone map (an ACES fit) over the whole range; else a soft shoulder that
    /// keeps the colours as they are up to 0.8
    pub filmic: bool,
    /// how much of the blurred glow is added (0: no bloom)
    pub bloom: f32,
    /// what else blooms: the colour's brightness past this (linear)
    pub threshold: f32,
    /// the indirect light's strength (0: none) and how far it's worked out round the
    /// camera's target (cells; a square twice that wide)
    pub gi: f32,
    pub gi_range: f32,
}

impl Default for Radiance {
    fn default() -> Self {
        Radiance { exposure: 1.0, filmic: false, bloom: 0.35, threshold: 1.0, gi: 1.0, gi_range: 32.0 }
    }
}

/// The post-processing over the game's picture: the pixelation (the scene drawn with fewer
/// pixels, each a square of the screen's, its colours in 15 bits: as dense as asked, growing
/// and shrinking with the camera's zoom) and an old handheld's LCD colours (with the thin
/// gaps between the pixels, where those are big).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Retro {
    /// the pixelation, how dense its pixels are: 0 the fewest (about as many as an old
    /// handheld's screen had), 1 the screen's own (none). None: off.
    pub pixelation: Option<f32>,
    /// how close the camera is against its usual distance (1; half as far: 2): the
    /// pixelation's pixels grow with it, so what's further off keeps its shape
    pub zoom: f32,
    /// whose LCD colours (none: the colours as they are)
    pub lcd: Option<Handheld>,
}

impl Default for Retro {
    fn default() -> Self {
        Retro { pixelation: None, zoom: 1.0, lcd: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handheld {
    /// the GBA: 240 by 160, a reflective screen that muted its colours
    Gba,
    /// the GBC: 160 by 144, its channels mixed by its screen
    Gbc,
}

impl Handheld {
    /// How dark the gaps between its LCD's pixels are.
    fn grid(self) -> f32 {
        match self {
            Handheld::Gba => 0.22,
            Handheld::Gbc => 0.32,
        }
    }

    /// Its LCD's colours, as the shader numbers them.
    fn code(self) -> f32 {
        match self {
            Handheld::Gba => 1.0,
            Handheld::Gbc => 2.0,
        }
    }
}

/// The fewest pixels the pixelation leaves the picture, at its lowest density: about as many
/// as the GBC's screen had (160 by 144).
const FEWEST: f32 = 160.0 * 144.0;

/// How many of the screen's pixels a side the pixelation's pixels are on a `screen`: at
/// `density` 0 the picture has about `FEWEST`, at 1 the screen's own (1: none), and in between
/// evenly in their ratio; times the camera's `zoom` (further out, smaller, so what's far keeps
/// its shape; closer, bigger, but never bigger than the fewest's). A whole number, so they're
/// all the same size.
pub fn pixel_size(screen: [u32; 2], density: f32, zoom: f32) -> u32 {
    let density = density.clamp(0.0, 1.0);
    if density >= 1.0 {
        return 1;
    }
    let fewest = (screen[0] as f32 * screen[1] as f32 / FEWEST).sqrt().max(1.0);
    let k = fewest.powf(1.0 - density) * zoom.max(0.0);
    (k.min(fewest).round() as u32).max(1)
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
    /// how big the pixelation's pixels were, in the screen's a side (1: none)
    pub pixel: u32,
    /// grass copies drawn (as last known: culled on the GPU, they're read back a few frames
    /// late), their vertices at the levels they're drawn at, and whether they're culled on the
    /// GPU (else on the CPU)
    pub grass: u32,
    pub grass_vertices: u64,
    pub grass_on_gpu: bool,
    /// the grass copies drawn at each level (0 the finest: up close)
    pub grass_levels: [u32; crate::grass::MOST_LEVELS],
    /// the meadows' blades drawn (as the grass's copies are known), their vertices, how many
    /// with each template (a blade's finest, middle and simplest, then a flower's), and
    /// whether they're culled on the GPU
    pub blades: u32,
    pub blade_vertices: u64,
    pub blade_templates: [u32; 6],
    pub blades_on_gpu: bool,
}

/// A view's six planes (from its view-projection; depth 0 to 1, as wgpu has it).
pub(crate) struct Frustum([Vec4; 6]);

impl Frustum {
    pub(crate) fn new(m: &Mat4) -> Frustum {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        Frustum([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2])
    }

    /// Can any of the box be in it?
    pub(crate) fn sees(&self, lo: Vec3, hi: Vec3) -> bool {
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
    /// x: how many lights
    light_count: [f32; 4],
    /// the toon look (see the shader)
    toon_a: [f32; 4],
    toon_b: [f32; 4],
    toon_c: [f32; 4],
    toon_d: [f32; 4],
    toon_e: [f32; 4],
    /// the screen back to the world (the post-process finds where a pixel is)
    inv_view_proj: [[f32; 4]; 4],
    /// the clouds' shadows: x how dark (by the sun's strength; 0: none), y 1 / a cloud's
    /// width, z how much of the sky, w how soft their edges; and b: xy their drift (cells
    /// a second), z how high they are
    clouds_a: [f32; 4],
    clouds_b: [f32; 4],
    /// the old handhelds' screens: x their pixel's size in the screen's pixels (0: none), y
    /// the LCD's colours (0 none, 1 GBA, 2 GBC), z how dark its gaps, w a channel's steps (31:
    /// 15-bit colour, 0: all); and b: xy the screen's size in pixels
    retro_a: [f32; 4],
    retro_b: [f32; 4],
    /// the wind: xy the way it blows (x, z), z how strong, w how gusty; b: xy how far its gusts
    /// have gone (cells), z the sway's phase, w the flutter's (radians); c: x the ripples'
    /// phase, y the laps' (turns)
    wind: [f32; 4],
    wind_b: [f32; 4],
    wind_c: [f32; 4],
    /// what's kept in sight through the trees and props in front of it: xyz where, w the
    /// see-through cone's radius there (0: none)
    see_through: [f32; 4],
    /// the picture's light: exposure, bloom, the tone map (0 soft, 1 filmic), what else
    /// blooms (the colour's brightness past this)
    tone: [f32; 4],
    /// the indirect light: x how strong (0: none), y 1 / the region's width, z its width, w
    /// the probes' spacing; b: xy the region's corner, z the lamps' bounce, w the screen's
    /// glow's share
    gi: [f32; 4],
    gi_b: [f32; 4],
    /// who tramples the grass: x how many; each xyz where they stand, w how wide they push
    push: [f32; 4],
    pushers: [[f32; 4]; MAX_PUSHERS],
    /// per light: position + range, direction + cos of the outer angle, colour + cos of
    /// the inner angle
    lights: [[f32; 4]; MAX_LIGHTS * 3],
}

/// How many lights a frame can hold: the nearest ones.
pub const MAX_LIGHTS: usize = 32;

/// The most who trample the grass in a frame (the walker, the actors, the creatures roaming;
/// the nearest the camera's target).
pub const MAX_PUSHERS: usize = 16;

/// The way the clouds drift (cells, a unit long): east, and a little south.
const CLOUD_WIND: glam::Vec2 = glam::Vec2::new(0.928, 0.371);

/// What moves with the wind at the wind's speed, added up frame by frame: where its gusts and
/// the clouds have got to, the phases of the sway, the flutter, the ripples and the laps, how
/// far the clouds have morphed. A change of the wind's way or strength turns them or speeds
/// them up from where they are; were they its speed times the time, they'd jump.
#[derive(Clone, Copy, Debug, Default)]
struct WindClock {
    last: Option<f32>,
    gusts: Vec2,
    clouds: Vec2,
    sway: f64,
    flutter: f64,
    ripple: f64,
    lap: f64,
    morph: f64,
}

impl WindClock {
    /// On to `time` (the frame's seconds) with this wind and these clouds.
    fn step(&mut self, time: f32, wind: Wind, cloud_speed: f32) {
        let dt = match self.last {
            Some(l) if time >= l => (time - l).min(0.25),
            _ => 0.0,
        };
        self.last = Some(time);
        let way = wind.way.normalize_or(CLOUD_WIND);
        let s = wind.strength.clamp(0.0, 1.0);
        self.gusts += way * (4.2 + 10.0 * s) * dt;
        self.clouds += way * cloud_speed.max(0.0) * dt;
        let dt = dt as f64;
        let s = s as f64;
        self.sway = (self.sway + (1.1 + 1.6 * s) * dt) % std::f64::consts::TAU;
        self.flutter = (self.flutter + (7.0 + 9.0 * s) * dt) % std::f64::consts::TAU;
        self.ripple = (self.ripple + (0.7 + 1.6 * s) * dt) % 1.0;
        self.lap = (self.lap + (1.0 + 1.3 * s) * dt) % 1.0;
        // the clouds change their shapes slowly, a little faster in a stronger wind
        self.morph += (0.018 + 0.02 * s) * dt;
    }
}

/// A spot light: a cone of light from `pos` along `dir`, `range` long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotLight {
    pub pos: Vec3,
    pub dir: Vec3,
    /// linear colour, times the intensity
    pub color: Vec3,
    pub range: f32,
    /// the cone's half angles, in radians: full light within `inner`, none past `outer`
    pub inner: f32,
    pub outer: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// 1: drawn with the toon look (ink lines round it)
    pub toon: f32,
}

impl SpriteVertex {
    fn plain(pos: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> SpriteVertex {
        SpriteVertex { pos, uv, color, toon: 0.0 }
    }
}

/// The light of a frame, in linear colours.
#[derive(Clone, Debug)]
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
    /// the lamps lit this frame (the first MAX_LIGHTS count)
    pub lights: Vec<SpotLight>,
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
    /// drawn with the toon look: ink lines round it
    pub toon: bool,
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
/// A vertex of a model that moves with a rig: like a map's, with the joints that place
/// it and their weights.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
    /// 1: drawn with the toon look
    pub toon: f32,
    /// how it moves in the wind (as `MapVertex::wind`)
    pub wind: [u8; 4],
}

/// A skinned model drawn this frame: which (`set_skinned`), where, and its joints' matrices
/// (the pose, see `world::anim::pose`).
#[derive(Clone, Debug)]
pub struct Animated {
    pub key: String,
    pub model: Mat4,
    pub joints: Vec<Mat4>,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub view_proj: Mat4,
    pub camera_pos: Vec3,
    pub light: Lighting,
    pub shadows: bool,
    pub billboards: Vec<Billboard>,
    pub shapes: Vec<Shape>,
    pub glows: Vec<Glow>,
    /// skinned models in their poses
    pub animated: Vec<Animated>,
    /// rain streaks: (top, bottom) points
    pub streaks: Vec<(Vec3, Vec3)>,
    /// seconds, for things that move by themselves (water)
    pub time: f32,
    /// grid lines on the world's ground (the map editor)
    pub grid: bool,
    /// an editor's focus: the world's cells outside it are darkened (the cells texture's
    /// blue channel says which are in), and what stands outside this box (x0, z0, x1, z1)
    pub focus: Option<[f32; 4]>,
    /// how dark the outside of the focus goes, 0 (not at all) to 1
    pub dim: f32,
    /// shapes drawn over everything (an editor's cursors), whatever is in front of them
    pub overlays: Vec<Shape>,
    /// how what's toon looks (vertices and billboards say what is)
    pub toon: Toon,
    /// the old handhelds' pixels and colours (the game's screens ask for them, never an
    /// editor's)
    pub retro: Retro,
    /// a blurred copy of the picture for frosted glass over it (`frost_view`)
    pub frost: bool,
    /// the wind: what's painted for it sways, the water ripples and rolls, the clouds drift
    pub wind: Wind,
    /// what's kept in sight through the trees and props in front of it (the walker): where,
    /// and how wide the see-through cone is round it (cells); none: nothing is
    pub see_through: Option<(Vec3, f32)>,
    /// who tramples the grass: each where they stand (their feet) and how wide they push
    /// (cells); the meadows' blades and what's painted for the wind give way round them (the
    /// first `MAX_PUSHERS` count)
    pub pushers: Vec<(Vec3, f32)>,
    /// the picture's light: its exposure and tone map, the bloom, the indirect light
    pub radiance: Radiance,
}

/// The wind as the frame has it: the way it blows on the ground (x, z, a unit long), how
/// strong (0 calm to 1 a gale) and how much of it comes in gusts (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    pub way: Vec2,
    pub strength: f32,
    pub gusts: f32,
}

impl Default for Wind {
    /// No wind; the clouds drift east, and a little south.
    fn default() -> Wind {
        Wind { way: CLOUD_WIND, strength: 0.0, gusts: 0.0 }
    }
}

/// The toon look, for what's flagged toon (vertices, billboards): the sun's and the lamps'
/// light in steps, the shade tinted, a rim of light round the edge and a glint where the
/// sun catches; and ink lines, drawn over the finished picture, round it where it stands
/// in front of what's behind (outside it) and where its surface folds sharply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Toon {
    /// steps of light from dark to lit, 1 to 5
    pub bands: u32,
    /// how soft a step's edge is, 0 (hard) to 1
    pub softness: f32,
    /// the colour the shade turns towards (linear), and how far, 0 to 1
    pub shadow_color: Vec3,
    pub shadow: f32,
    /// the rim of light: how bright (0: none) and how wide, 0 to 1
    pub rim: f32,
    pub rim_width: f32,
    /// the glint: how bright (0: none) and how big, 0 to 1
    pub glint: f32,
    pub glint_size: f32,
    pub ink: bool,
    /// linear
    pub ink_color: Vec3,
    /// how wide the outline is, in pixels of the screen
    pub ink_width: f32,
    pub ink_opacity: f32,
    /// how far behind what's around it must be for a line, as a share of its distance
    pub depth_step: f32,
    /// a fold sharper than this many degrees gets a line (0: none)
    pub crease: f32,
    /// the whole picture's light in this many steps (0: smooth), and how soft their edges
    /// are (0 to 1): the world's colour steps, whatever is toon
    pub color_steps: u32,
    pub color_softness: f32,
    /// clouds' shadows over the world (0: none, to 1), a cloud's width (cells), their drift
    /// (cells a second), how much of the sky they cover (0 to 1)
    pub cloud_shadow: f32,
    pub cloud_size: f32,
    pub cloud_speed: f32,
    pub cloud_cover: f32,
}

impl Default for Toon {
    fn default() -> Toon {
        Toon {
            bands: 2,
            softness: 0.12,
            shadow_color: Vec3::new(0.18, 0.13, 0.32),
            shadow: 0.35,
            rim: 0.35,
            rim_width: 0.3,
            glint: 0.3,
            glint_size: 0.2,
            ink: true,
            ink_color: Vec3::new(0.025, 0.02, 0.035),
            ink_width: 2.5,
            ink_opacity: 0.9,
            depth_step: 0.04,
            crease: 50.0,
            color_steps: 0,
            color_softness: 0.15,
            cloud_shadow: 0.45,
            cloud_size: 26.0,
            cloud_speed: 0.8,
            cloud_cover: 0.45,
        }
    }
}

/// A world map's ground: the cells' texture and the terrains' palette (the meshes come
/// square by square, `set_chunk`).
pub struct WorldGround<'a> {
    /// (width + 2) x (height + 2) texels, a ring of the cells around the map first: terrain
    /// index, level, in the editor's focus (255) or not, 255
    pub cells: &'a [u8],
    /// the texture's size, the ring included
    pub size: [u32; 2],
    /// terrains x 4 rows, see the shader
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
    /// some of it is toon
    toon: bool,
}

impl GpuMesh {
    fn new(device: &wgpu::Device, vertices: &[MapVertex], indices: &[u32]) -> Option<GpuMesh> {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut toon = false;
        for v in vertices {
            let p = Vec3::from(v.pos);
            lo = lo.min(p);
            hi = hi.max(p);
            toon |= v.toon > 0.5;
        }
        GpuMesh::raw(device, bytemuck::cast_slice(vertices), indices, lo, hi, toon)
    }

    /// From vertices of any layout.
    fn raw(device: &wgpu::Device, vertices: &[u8], indices: &[u32], lo: Vec3, hi: Vec3, toon: bool) -> Option<GpuMesh> {
        if indices.is_empty() {
            return None;
        }
        Some(GpuMesh {
            lo,
            hi,
            toon,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world vertices"),
                contents: vertices,
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

/// A square's pieces, by texture (WHITE for the vertex-coloured ones; a texture's pieces one
/// after the other), with the box round them all and whether any is toon: what's out of
/// sight is left out a square at a time, and a frame doesn't look at every piece for toon.
struct Square {
    lo: Vec3,
    hi: Vec3,
    toon: bool,
    pieces: Vec<(String, GpuMesh)>,
}

impl Square {
    fn new(pieces: Vec<(String, GpuMesh)>) -> Square {
        let lo = pieces.iter().fold(Vec3::splat(f32::MAX), |a, (_, m)| a.min(m.lo));
        let hi = pieces.iter().fold(Vec3::splat(f32::MIN), |a, (_, m)| a.max(m.hi));
        Square { lo, hi, toon: pieces.iter().any(|(_, m)| m.toon), pieces }
    }
}

/// One layer's squares.
type Squares = BTreeMap<(i32, i32), Square>;

/// A world map on the GPU, square by square (whatever size the editor cuts it in; each
/// square's meshes are cut again into CHUNK squares for the culling), in its own space,
/// placed in the scene's by `place`.
struct WorldGpu {
    /// the cell tops (the terrain shader), by square
    tops: Squares,
    /// everything else by layer ("cliffs", "props", "trees", "buildings"...) and square:
    /// by texture (WHITE for the vertex-coloured ones), a texture's pieces one after the
    /// other
    layers: BTreeMap<String, Squares>,
    cells: wgpu::Texture,
    /// how far each cell is from the shore (`shore::shore_texels`), and each terrain's
    /// water (the palette's columns: 0 none, 1 inland, 2 oceanic)
    shore: wgpu::Texture,
    water: Vec<u8>,
    /// the cells the shore's field was last worked out from (`shore::shore_window` finds what
    /// changed)
    shore_cells: Vec<u8>,
    size: [u32; 2],
    bind: wgpu::BindGroup,
    level_height: f32,
    place: Place,
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

/// The frosted glass: the picture small and blurred, in two textures (blurred across from the
/// first into the second, then down back into the first, which egui reads), each bound as
/// the next pass reads it.
struct Frost {
    size: [u32; 2],
    views: [wgpu::TextureView; 2],
    binds: [wgpu::BindGroup; 2],
}

struct Target {
    size: [u32; 2],
    color_view: wgpu::TextureView,
    look_view: wgpu::TextureView,
    emit_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    /// the colour, the looks and the bloom, for the blit; the colour and what shines, for
    /// the bloom's first pass
    post_bind: wgpu::BindGroup,
    bloom_first_bind: wgpu::BindGroup,
    /// the bloom's chain, level by level (half the picture's size, then halved again), each
    /// drawn into, and bound for the next to read
    bloom_views: Vec<wgpu::TextureView>,
    bloom_binds: Vec<wgpu::BindGroup>,
    /// the indirect light's field passes, reading what this target last showed shining
    gi_bind: Option<wgpu::BindGroup>,
}

struct DrawCall {
    texture: String,
    first: u32,
    count: u32,
}

/// The joints' matrices of this frame's skinned models, one buffer, each model's at an
/// offset the draw binds. The binding is a window of `window` bytes from that offset (the
/// same for every model: it's the bind group's), so the buffer runs at least a window past
/// the last model's offset. Read from storage where vertex shaders can (WebGPU); where they
/// can't (WebGL 2), from a uniform buffer, its window BONES_WINDOW matrices (16 KB, the most
/// a uniform block holds there), a model's joints past them left out.
struct Bones {
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
    cap: u64,
    window: u64,
    /// in storage, not a uniform buffer
    stored: bool,
}

/// Matrices per skinned model are padded to this many: a dynamic offset's alignment.
const BONES_ALIGN: usize = 4;
/// The matrices a skinned model's binding holds, at least (a model with more joints widens it).
const BONES_WINDOW: usize = 256;
const MAT4_BYTES: u64 = 64;

/// The bones' buffer and window a frame needs: its models' matrices packed (`packed`
/// bytes, each model's padded to BONES_ALIGN), the widest model `most` joints; never less
/// than what's there (`cap`, `window`), so it's made again only to grow.
fn bones_needed(packed: u64, most: usize, cap: u64, window: u64) -> (u64, u64) {
    let window = ((most.max(BONES_WINDOW).next_multiple_of(BONES_ALIGN)) as u64 * MAT4_BYTES).max(window);
    // the last model starts before `packed`: its window ends before packed + window
    let cap = (packed + window).next_power_of_two().max(cap);
    (cap, window)
}

pub struct SceneRenderer {
    globals: wgpu::Buffer,
    globals_layout: wgpu::BindGroupLayout,
    /// the globals and the world maps' placements (a dynamic offset picks one: 0 is the
    /// scene's own space)
    globals_bind: wgpu::BindGroup,
    places: wgpu::Buffer,
    /// how many placements `places` holds
    places_cap: u64,
    texture_layout: wgpu::BindGroupLayout,
    /// the blit's: the scene's colour, a sampler, the looks, the bloom
    post_layout: wgpu::BindGroupLayout,
    /// the bloom's first pass's: the colour, a smooth sampler, what shines
    bloom_layout: wgpu::BindGroupLayout,
    bones_layout: wgpu::BindGroupLayout,
    bones: Bones,
    /// a stand-in for the shadow map's slot in the shadow pass (which draws into it)
    empty_bind: wgpu::BindGroup,
    skinned_pipeline: wgpu::RenderPipeline,
    skinned_shadow_pipeline: wgpu::RenderPipeline,
    /// a fading mesh: its depth first, then its colour blended
    fade_depth_pipeline: wgpu::RenderPipeline,
    fade_pipeline: wgpu::RenderPipeline,
    /// skinned models by key: their parts by texture
    skinned: HashMap<String, Vec<(String, GpuMesh)>>,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    shadow_view: wgpu::TextureView,
    /// the light's: the sun's shadow map and the indirect light's probes
    shadow_bind: wgpu::BindGroup,
    shadow_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    /// the indirect light's cascades, where the device runs compute shaders
    gi: Option<Gi>,
    /// the globals as the indirect light's height pass has them (its view from above), and
    /// their bind group with the placements
    gi_globals: wgpu::Buffer,
    gi_globals_bind: wgpu::BindGroup,
    /// blank probes, bound where there are no cascades
    blank_gi: (wgpu::TextureView, wgpu::TextureView),
    /// the shadow map's size now
    shadow_size: u32,
    quality: Quality,
    stats: DrawStats,
    map_pipeline: wgpu::RenderPipeline,
    opaque_pipeline: wgpu::RenderPipeline,
    /// the same for trees and props, with the walker kept in sight through them
    through_pipeline: wgpu::RenderPipeline,
    opaque_through_pipeline: wgpu::RenderPipeline,
    map_shadow_pipeline: wgpu::RenderPipeline,
    /// the indirect light's height pass
    height_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    sprite_shadow_pipeline: wgpu::RenderPipeline,
    blend_pipeline: wgpu::RenderPipeline,
    add_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    /// the bloom: what shines at half size, halved down the chain, doubled back up and added
    bloom_first_pipeline: wgpu::RenderPipeline,
    bloom_down_pipeline: wgpu::RenderPipeline,
    bloom_up_pipeline: wgpu::RenderPipeline,
    /// the frosted glass: the picture small (into the first texture), blurred across, down
    frost_pipeline: wgpu::RenderPipeline,
    frost_x_pipeline: wgpu::RenderPipeline,
    frost_y_pipeline: wgpu::RenderPipeline,
    /// a texture and a sampler (the frosted glass's blurs read one)
    frost_layout: wgpu::BindGroupLayout,
    frost: Option<Frost>,
    /// bumped each time the frosted glass's textures are made again (the host's handle on
    /// them is then out of date)
    frost_generation: u64,
    wind_clock: WindClock,
    ground_pipeline: wgpu::RenderPipeline,
    ground_layout: wgpu::BindGroupLayout,
    /// the world maps shown, by id: the one the camera is on and the ones around it
    worlds: BTreeMap<String, WorldGpu>,
    /// things that move on the world (the walker, people): small, replaced often, in the
    /// scene's own space
    extra: Vec<(String, GpuMesh)>,
    /// things fading (a building's upper floors): drawn see-through, by their vertex alpha
    fade: Vec<(String, GpuMesh)>,
    /// the grass grown through tall ground: copies of its models
    grass: Grass,
    /// the meadows' blades and flowers, made on the GPU
    blades: Blades,
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
/// The scene's shaders: its own, and the meadows' blades' (what they are and how fine, then how
/// they're made and drawn). The indirect light's compute shaders are a module of their own
/// (`gi.rs`): a storage texture anywhere in a module keeps WebGL 2 from reading any of it.
const SHADER: &str = concat!(include_str!("shader.wgsl"), include_str!("blade_lod.wgsl"), include_str!("blades_draw.wgsl"));
/// The world layer of cliff faces: drawn by the terrain shader (as rock), like the tops.
pub const CLIFFS: &str = "cliffs";
pub const GLOW: &str = "#glow";

impl SceneRenderer {
    /// `output` is the format of the pass the scene is blitted into.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, output: wgpu::TextureFormat) -> SceneRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("scene"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let make_globals = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = make_globals("globals");
        let gi_globals = make_globals("globals from above");
        // the indirect light's compute shaders read the globals and the ground where there
        // are compute shaders (WebGPU); the layouts say so only then, WebGL 2 knowing no
        // compute stage
        let compute = Gi::can_compute(device);
        let stages = if compute { wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE } else { wgpu::ShaderStages::VERTEX_FRAGMENT };
        let fragment_stages = if compute { wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE } else { wgpu::ShaderStages::FRAGMENT };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: stages,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: stages,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<PlaceUniform>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let (places, places_cap, globals_bind, gi_globals_bind) = Self::make_places(device, &globals_layout, &globals, &gi_globals, 16);
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
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // the bloom, and its smooth sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // the light's: the sun's shadow map, and the indirect light's probes
        let probes = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
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
                probes(2),
                probes(3),
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
        let blank_gi = Gi::blank_views(device);

        let data_texture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: fragment_stages,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let ground_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ground"),
            entries: &[data_texture(2), data_texture(3), data_texture(4)],
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
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&globals_layout), Some(&post_layout)],
            immediate_size: 0,
        });
        // the bloom's first pass: the colour, a smooth sampler, what shines
        let smooth_texture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom first"),
            entries: &[smooth_texture(0), wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }, smooth_texture(5)],
        });
        let bloom_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom first"),
            bind_group_layouts: &[Some(&globals_layout), Some(&bloom_layout)],
            immediate_size: 0,
        });
        let frost_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frost"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let frost_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("frost"),
            bind_group_layouts: &[Some(&globals_layout), Some(&frost_layout)],
            immediate_size: 0,
        });
        // skinned models: their joints' matrices in a storage buffer, each model's at an
        // offset (group 3; in the shadow pass group 2 stands empty, the shadow map being
        // drawn into); where vertex shaders read no storage (WebGL 2), in a uniform buffer
        // the same way, its own binding read by its own entry points
        let stored = device.limits().max_storage_buffers_per_shader_stage > 0;
        let (binding, ty, min_binding_size) = if stored {
            (0, wgpu::BufferBindingType::Storage { read_only: true }, None)
        } else {
            (1, wgpu::BufferBindingType::Uniform, wgpu::BufferSize::new(BONES_WINDOW as u64 * MAT4_BYTES))
        };
        let bones_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bones"),
            entries: &[wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: true, min_binding_size }, count: None }],
        });
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] });
        let empty_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("empty"), layout: &empty_layout, entries: &[] });
        let skinned_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout), Some(&shadow_layout), Some(&bones_layout)],
            immediate_size: 0,
        });
        let skinned_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned shadow"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout), Some(&empty_layout), Some(&bones_layout)],
            immediate_size: 0,
        });
        let (cap, window) = bones_needed(0, 0, 0, 0);
        let bones = Self::make_bones(device, &bones_layout, cap, window, stored);
        let map_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32, 5 => Unorm8x4];
        let sprite_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32];
        let skinned_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Uint32x4, 5 => Float32x4, 6 => Float32, 7 => Unorm8x4];
        let map_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MapVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &map_attrs,
        })];
        let skinned_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SkinnedVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &skinned_attrs,
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
        // the targets: the solid pass's (colour, looks and what shines, all written or
        // none), the see-through pass's (colour), the shadow's (none), the screen's
        let solid = |write: bool| {
            let mask = if write { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() };
            vec![
                Some(wgpu::ColorTargetState { format: COLOR, blend: None, write_mask: mask }),
                Some(wgpu::ColorTargetState { format: LOOK, blend: None, write_mask: mask }),
                Some(wgpu::ColorTargetState { format: EMIT, blend: None, write_mask: mask }),
            ]
        };
        let clear = |blend: Option<wgpu::BlendState>, write: bool| {
            vec![Some(wgpu::ColorTargetState { format: COLOR, blend, write_mask: if write { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() } })]
        };
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        targets: Vec<Option<wgpu::ColorTargetState>>,
                        depth_state: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), buffers, compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
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
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let map_pipeline = pipeline("map", &main_layout, "vs_map", "fs_map", &map_buffers, solid(true), Some(depth(true)));
        let opaque_pipeline = pipeline("opaque", &main_layout, "vs_map", "fs_map_opaque", &map_buffers, solid(true), Some(depth(true)));
        let through_pipeline = pipeline("see-through", &main_layout, "vs_map", "fs_map_through", &map_buffers, solid(true), Some(depth(true)));
        let opaque_through_pipeline = pipeline("opaque see-through", &main_layout, "vs_map", "fs_map_opaque_through", &map_buffers, solid(true), Some(depth(true)));
        let map_shadow_pipeline = pipeline("map shadow", &shadow_pass_layout, "vs_map_shadow", "fs_map_shadow", &map_buffers, vec![], Some(shadow_depth.clone()));
        // the grass's copies: a model's vertices, and a copy an instance
        let copy_attrs = wgpu::vertex_attr_array![6 => Float32x3, 7 => Float32, 8 => Float32, 9 => Float32];
        let grass_buffers = [
            map_buffers[0].clone(),
            Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<GrassCopy>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &copy_attrs }),
        ];
        let grass = Grass::new(
            device,
            pipeline("grass", &main_layout, "vs_grass", "fs_map_opaque", &grass_buffers, solid(true), Some(depth(true))),
            pipeline("grass cut-out", &main_layout, "vs_grass", "fs_map", &grass_buffers, solid(true), Some(depth(true))),
            pipeline("grass shadow", &shadow_pass_layout, "vs_grass_shadow", "fs_map_shadow", &grass_buffers, vec![], Some(shadow_depth.clone())),
        );
        // the meadows' blades: a blade an instance (each square's, the drawing culling them), or
        // with compute shaders from the culling's lists (the blades and a list read from storage)
        let (blade_kinds_layout, blade_store_layout) = Blades::layouts(device);
        let layout = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: groups, immediate_size: 0 });
        let blade_layout = layout("blades", &[Some(&globals_layout), Some(&blade_kinds_layout), Some(&shadow_layout)]);
        let blade_shadow_layout = layout("blades shadow", &[Some(&globals_layout), Some(&blade_kinds_layout)]);
        let blade_attrs = wgpu::vertex_attr_array![6 => Float32x3, 7 => Float32, 8 => Float32, 9 => Float32, 10 => Uint32, 11 => Uint32];
        let blade_buffers = [Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<BladeInstance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &blade_attrs })];
        let listed = blade_store_layout.as_ref().map(|store| {
            let main = layout("blades listed", &[Some(&globals_layout), Some(&blade_kinds_layout), Some(&shadow_layout), Some(store)]);
            let sun = layout("blades listed shadow", &[Some(&globals_layout), Some(&blade_kinds_layout), Some(&empty_layout), Some(store)]);
            (
                pipeline("blades listed", &main, "vs_blade_listed", "fs_blade", &[], solid(true), Some(depth(true))),
                pipeline("blades listed shadow", &sun, "vs_blade_listed_shadow", "fs_blade_shadow", &[], vec![], Some(shadow_depth.clone())),
            )
        });
        let blades = Blades::new(
            device,
            BladePipelines {
                main: pipeline("blades", &blade_layout, "vs_blade", "fs_blade", &blade_buffers, solid(true), Some(depth(true))),
                shadow: pipeline("blades shadow", &blade_shadow_layout, "vs_blade_shadow", "fs_blade_shadow", &blade_buffers, vec![], Some(shadow_depth.clone())),
                listed,
            },
            &blade_kinds_layout,
            blade_store_layout,
        );
        let (skinned_vs, skinned_shadow_vs) = if stored { ("vs_skinned", "vs_skinned_shadow") } else { ("vs_skinned_uniform", "vs_skinned_uniform_shadow") };
        let skinned_pipeline = pipeline("skinned", &skinned_layout, skinned_vs, "fs_map", &skinned_buffers, solid(true), Some(depth(true)));
        let skinned_shadow_pipeline = pipeline("skinned shadow", &skinned_shadow_layout, skinned_shadow_vs, "fs_map_shadow", &skinned_buffers, vec![], Some(shadow_depth.clone()));
        // depth only, in the see-through pass: a colour target it never writes to
        let fade_depth_pipeline = pipeline("fade depth", &main_layout, "vs_map", "fs_map_depth", &map_buffers, clear(None, false), Some(depth(true)));
        let fade_pipeline = pipeline("fade", &main_layout, "vs_map", "fs_map_fade", &map_buffers, clear(alpha, true), Some(depth(false)));
        let sprite_pipeline = pipeline("sprites", &main_layout, "vs_sprite", "fs_sprite_cutout", &sprite_buffers, solid(true), Some(depth(true)));
        // the indirect light's height pass: the height of what the sun's pass draws, from above
        let heights = vec![Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::R32Float, blend: None, write_mask: wgpu::ColorWrites::ALL })];
        let height_pipeline = pipeline("gi heights", &shadow_pass_layout, "vs_map_shadow", "fs_height", &map_buffers, heights, Some(shadow_depth.clone()));
        let sprite_shadow_pipeline = pipeline("sprite shadow", &shadow_pass_layout, "vs_sprite_shadow", "fs_sprite_shadow", &sprite_buffers, vec![], Some(shadow_depth));
        let blend_pipeline = pipeline("blend", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, clear(alpha, true), Some(depth(false)));
        let on_top = wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() };
        let overlay_pipeline = pipeline("overlay", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, clear(alpha, true), Some(on_top));
        let add_pipeline = pipeline("additive", &main_layout, "vs_sprite", "fs_sprite_blend", &sprite_buffers, clear(Some(additive), true), Some(depth(false)));
        let blit_pipeline = pipeline("blit", &post_pipeline_layout, "vs_blit", "fs_blit", &[], vec![Some(wgpu::ColorTargetState { format: output, blend: None, write_mask: wgpu::ColorWrites::ALL })], None);
        let frosted = || vec![Some(wgpu::ColorTargetState { format: FROST, blend: None, write_mask: wgpu::ColorWrites::ALL })];
        let frost_pipeline = pipeline("frost", &post_pipeline_layout, "vs_blit", "fs_frost", &[], frosted(), None);
        let frost_x_pipeline = pipeline("frost across", &frost_pipeline_layout, "vs_blit", "fs_frost_x", &[], frosted(), None);
        let frost_y_pipeline = pipeline("frost down", &frost_pipeline_layout, "vs_blit", "fs_frost_y", &[], frosted(), None);
        // the bloom's chain: halved (the first from what shines and the colour), doubled and added
        let bloomed = |blend: Option<wgpu::BlendState>| vec![Some(wgpu::ColorTargetState { format: EMIT, blend, write_mask: wgpu::ColorWrites::ALL })];
        let add_up = wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent::REPLACE };
        let bloom_first_pipeline = pipeline("bloom first", &bloom_pipeline_layout, "vs_blit", "fs_bloom_first", &[], bloomed(None), None);
        let bloom_down_pipeline = pipeline("bloom down", &frost_pipeline_layout, "vs_blit", "fs_bloom_down", &[], bloomed(None), None);
        let bloom_up_pipeline = pipeline("bloom up", &frost_pipeline_layout, "vs_blit", "fs_bloom_up", &[], bloomed(Some(add_up)), None);
        let ground_pipeline = pipeline("ground", &ground_pipeline_layout, "vs_map", "fs_ground", &map_buffers, solid(true), Some(depth(true)));
        let gi = compute.then(|| Gi::new(device, &globals_layout, &ground_layout, &empty_layout));
        let (shadow_view, shadow_bind) = Self::make_light(device, &shadow_layout, &shadow_sampler, SHADOW_SIZE, gi.as_ref().map_or((&blank_gi.0, &blank_gi.1), Gi::views));

        let mut r = SceneRenderer {
            globals, globals_layout, globals_bind, places, places_cap, texture_layout, post_layout, bloom_layout, bones_layout, bones, empty_bind, skinned_pipeline, skinned_shadow_pipeline, fade_depth_pipeline, fade_pipeline, skinned: HashMap::new(),
            nearest, linear, shadow_view, shadow_bind, shadow_layout, shadow_sampler, gi, gi_globals, gi_globals_bind, blank_gi,
            shadow_size: SHADOW_SIZE, quality: Quality::default(), stats: DrawStats::default(),
            map_pipeline, opaque_pipeline, through_pipeline, opaque_through_pipeline, map_shadow_pipeline, height_pipeline, sprite_pipeline, sprite_shadow_pipeline, blend_pipeline, add_pipeline, blit_pipeline,
            bloom_first_pipeline, bloom_down_pipeline, bloom_up_pipeline,
            frost_pipeline, frost_x_pipeline, frost_y_pipeline, frost_layout, frost: None, frost_generation: 0, wind_clock: WindClock::default(),
            ground_pipeline, ground_layout, worlds: BTreeMap::new(), extra: vec![], fade: vec![], grass, blades,
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

    /// The world maps' placements: a buffer of `cap` of them (the first the scene's own
    /// space), and the globals' bind groups reading it: the frame's, and the indirect
    /// light's height pass's (`gi_globals`).
    fn make_places(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, gi_globals: &wgpu::Buffer, cap: u64) -> (wgpu::Buffer, u64, wgpu::BindGroup, wgpu::BindGroup) {
        let places = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places"),
            size: cap * PLACE_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = |globals: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &places, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<PlaceUniform>() as u64) }),
                    },
                ],
            })
        };
        let (own, above) = (bind(globals), bind(gi_globals));
        (places, cap, own, above)
    }

    /// Draws a degenerate triangle with every pipeline once, offscreen, so that browsers
    /// which compile shaders on first use do it now and not in the middle of a battle.
    fn warm_up(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let target = self.make_target(device, [4, 4]);
        let ground = self.data_bind(device, queue, &WorldGround { cells: &[0, 0, 0, 255], size: [1, 1], palette: &[255; 16], palette_width: 1, level_height: 1.0 }, &[[0.0; 4]]);
        let zeros = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("warm-up"),
            contents: &[0u8; 3 * 96],
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
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_vertex_buffer(0, zeros.slice(..));
            for p in [&self.map_shadow_pipeline, &self.sprite_shadow_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
            pass.set_vertex_buffer(1, zeros.slice(..));
            self.grass.warm_up(&mut pass, true);
            pass.set_bind_group(2, &self.empty_bind, &[]);
            self.blades.warm_up(&mut pass, true, &zeros);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_bind_group(3, &self.bones.bind, &[0]);
            pass.set_pipeline(&self.skinned_shadow_pipeline);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = self.solid_pass(&mut encoder, &target, wgpu::Color::BLACK);
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]);
            pass.set_vertex_buffer(0, zeros.slice(..));
            for p in [&self.map_pipeline, &self.opaque_pipeline, &self.sprite_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
            pass.set_vertex_buffer(1, zeros.slice(..));
            self.grass.warm_up(&mut pass, false);
            self.blades.warm_up(&mut pass, false, &zeros);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_bind_group(3, &self.bones.bind, &[0]);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_bind_group(1, &ground.2, &[]);
            pass.set_pipeline(&self.ground_pipeline);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = Self::clear_pass(&mut encoder, &target);
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]);
            pass.set_vertex_buffer(0, zeros.slice(..));
            for p in [&self.fade_depth_pipeline, &self.fade_pipeline, &self.blend_pipeline, &self.add_pipeline, &self.overlay_pipeline] {
                pass.set_pipeline(p);
                pass.draw(0..3, 0..1);
            }
        }
        // the frosted glass's three passes, and the bloom's three
        let frost = self.make_frost(device, [2, 2]);
        let bloom = &target.bloom_views;
        for (pipeline, bind, into) in [
            (&self.frost_pipeline, &target.post_bind, &frost.views[0]),
            (&self.frost_x_pipeline, &frost.binds[0], &frost.views[1]),
            (&self.frost_y_pipeline, &frost.binds[1], &frost.views[0]),
            (&self.bloom_first_pipeline, &target.bloom_first_bind, &bloom[0]),
            (&self.bloom_down_pipeline, &target.bloom_binds[0], &bloom[1]),
            (&self.bloom_up_pipeline, &target.bloom_binds[1], &bloom[0]),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("warm-up frost"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: into, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            pass.set_bind_group(1, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }

    /// The bones' buffer, `cap` bytes, bound a `window` of bytes at a time: in storage
    /// (`stored`), or a uniform buffer.
    fn make_bones(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, cap: u64, window: u64, stored: bool) -> Bones {
        let (usage, binding) = if stored { (wgpu::BufferUsages::STORAGE, 0) } else { (wgpu::BufferUsages::UNIFORM, 1) };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bones"),
            size: cap,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bones"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(window) }),
            }],
        });
        Bones { buffer, bind, cap, window, stored }
    }

    /// A skinned model's parts, by texture, kept under `key` for `Frame::animated` to draw.
    pub fn set_skinned(&mut self, device: &wgpu::Device, key: &str, parts: &[(String, Vec<SkinnedVertex>, Vec<u32>)]) {
        let meshes = parts
            .iter()
            .filter_map(|(texture, vertices, indices)| {
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for v in vertices {
                    lo = lo.min(Vec3::from(v.pos));
                    hi = hi.max(Vec3::from(v.pos));
                }
                let toon = vertices.iter().any(|v| v.toon > 0.5);
                GpuMesh::raw(device, bytemuck::cast_slice(vertices), indices, lo, hi, toon).map(|m| (texture.clone(), m))
            })
            .collect();
        self.skinned.insert(key.to_string(), meshes);
    }

    pub fn has_skinned(&self, key: &str) -> bool {
        self.skinned.contains_key(key)
    }

    /// The sun's shadow map, `size` texels a side, bound with the indirect light's probes.
    fn make_light(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, size: u32, probes: (&wgpu::TextureView, &wgpu::TextureView)) -> (wgpu::TextureView, wgpu::BindGroup) {
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
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(probes.0) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(probes.1) },
            ],
        });
        (view, bind)
    }

    /// Sets the resolution, the shadows and the indirect light (taking effect on the next
    /// frame).
    pub fn set_quality(&mut self, device: &wgpu::Device, q: Quality) {
        let q = Quality { scale: q.scale.clamp(0.25, 1.0), shadow_size: q.shadow_size.min(device.limits().max_texture_dimension_2d), indirect: q.indirect };
        if q.shadow_size > 0 && q.shadow_size != self.shadow_size {
            (self.shadow_view, self.shadow_bind) = Self::make_light(device, &self.shadow_layout, &self.shadow_sampler, q.shadow_size, self.gi.as_ref().map_or((&self.blank_gi.0, &self.blank_gi.1), Gi::views));
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

    /// Whether the device runs the indirect light's cascades (compute shaders: WebGPU).
    pub fn indirect_on_gpu(&self) -> bool {
        self.gi.is_some()
    }

    /// What the last frame had drawn.
    pub fn stats(&self) -> DrawStats {
        self.stats
    }

    /// The frosted glass of the last frame that asked for it (`Frame::frost`): the picture
    /// small and blurred (Rgba8Unorm, the screen's values, as egui takes a texture), and its
    /// generation: a new one means the texture was made again (the host registers it again).
    pub fn frost_view(&self) -> Option<(&wgpu::TextureView, u64)> {
        self.frost.as_ref().map(|f| (&f.views[0], self.frost_generation))
    }

    /// The frosted glass's textures, `size` texels.
    fn make_frost(&self, device: &wgpu::Device, size: [u32; 2]) -> Frost {
        let view = |label: &str| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FROST,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let views = [view("frost"), view("frost across")];
        let bind = |v: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frost"),
                layout: &self.frost_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.linear) }],
            })
        };
        let binds = [bind(&views[0]), bind(&views[1])];
        Frost { size, views, binds }
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
    fn data_bind(&self, device: &wgpu::Device, queue: &wgpu::Queue, g: &WorldGround, shore: &[[f32; 4]]) -> (wgpu::Texture, wgpu::Texture, wgpu::BindGroup) {
        let cells = Self::data_texture(device, queue, "cells", g.cells, g.size);
        // (as many rows as it has: a terrain's own, then its look's)
        let width = g.palette_width.max(1);
        let palette = Self::data_texture(device, queue, "palette", g.palette, [width, (g.palette.len() as u32 / (4 * width)).max(1)]);
        let shore = Self::field_texture(device, queue, "shore", shore, g.size);
        let (cv, pv, sv) = (cells.create_view(&Default::default()), palette.create_view(&Default::default()), shore.create_view(&Default::default()));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground"),
            layout: &self.ground_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&cv) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&pv) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&sv) },
            ],
        });
        (cells, shore, bind)
    }

    /// A field of four floats a texel as a texture the shaders load from.
    fn field_texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, field: &[[f32; 4]], size: [u32; 2]) -> wgpu::Texture {
        let size = [size[0].max(1), size[1].max(1)];
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self::write_field(queue, &texture, field, size);
        texture
    }

    fn write_field(queue: &wgpu::Queue, texture: &wgpu::Texture, field: &[[f32; 4]], size: [u32; 2]) {
        if field.len() as u32 != size[0] * size[1] {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(field),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 16), rows_per_image: Some(size[1]) },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
    }

    /// Each of the palette's terrains' water (its third row's blue: see the shader).
    fn waters(palette: &[u8], width: u32) -> Vec<u8> {
        let w = width as usize;
        (0..w).map(|t| palette.get((2 * w + t) * 4 + 2).copied().unwrap_or(0)).collect()
    }

    /// How far each of the cells is from the shore, which way the land is, which water it is
    /// or is near (`shore::shore_texels`).
    fn shore_of(cells: &[u8], size: [u32; 2], water: &[u8]) -> Vec<[f32; 4]> {
        crate::shore::shore_texels(cells, size[0] as usize, size[1] as usize, water)
    }

    /// Shows a world map's ground: its cells' texture and palette (the ones it had before
    /// go; its squares stay when it's the same size, else they go too). Battle maps
    /// (`load_map`) and the world draw together; clear the one you don't want.
    pub fn set_world_ground(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, map: &str, g: &WorldGround) {
        let water = Self::waters(g.palette, g.palette_width);
        let shore = Self::shore_of(g.cells, g.size, &water);
        let (cells, shore, bind) = self.data_bind(device, queue, g, &shore);
        let (tops, layers, place) = match self.worlds.remove(map) {
            Some(w) if w.size == g.size => (w.tops, w.layers, w.place),
            Some(w) => {
                self.grass.drop_map(map);
                self.blades.drop_map(map);
                (BTreeMap::new(), BTreeMap::new(), w.place)
            }
            None => (BTreeMap::new(), BTreeMap::new(), Place::default()),
        };
        self.worlds.insert(map.to_string(), WorldGpu { tops, layers, cells, shore, water, shore_cells: g.cells.to_vec(), size: g.size, bind, level_height: g.level_height, place });
    }

    /// Repaints a world map's cells only (the same size): what painting terrain changes; the
    /// shore's field with them round where the water or the levels changed. Only the texels
    /// in `rect` (x0, z0, x1, z1, out) changed: those are sent and looked at.
    pub fn set_world_cells(&mut self, queue: &wgpu::Queue, map: &str, cells: &[u8], rect: [u32; 4]) {
        let Some(w) = self.worlds.get_mut(map) else { return };
        let [x0, z0, x1, z1] = [rect[0].min(w.size[0]), rect[1].min(w.size[1]), rect[2].min(w.size[0]), rect[3].min(w.size[1])];
        if cells.len() as u32 != w.size[0] * w.size[1] * 4 || x1 <= x0 || z1 <= z0 {
            return;
        }
        let row = w.size[0] as usize * 4;
        let (from, to) = (z0 as usize * row + x0 as usize * 4, (z1 as usize - 1) * row + x1 as usize * 4);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &w.cells, mip_level: 0, origin: wgpu::Origin3d { x: x0, y: z0, z: 0 }, aspect: wgpu::TextureAspect::All },
            &cells[from..to],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w.size[0] * 4), rows_per_image: Some(z1 - z0) },
            wgpu::Extent3d { width: x1 - x0, height: z1 - z0, depth_or_array_layers: 1 },
        );
        let look = [x0 as usize, z0 as usize, x1 as usize, z1 as usize];
        if let Some((r, texels)) = crate::shore::shore_window(&w.shore_cells, cells, w.size[0] as usize, w.size[1] as usize, &w.water, look) {
            let (width, height) = ((r[2] - r[0]) as u32, (r[3] - r[1]) as u32);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &w.shore, mip_level: 0, origin: wgpu::Origin3d { x: r[0] as u32, y: r[1] as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(&texels),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 16), rows_per_image: Some(height) },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        }
        if w.shore_cells.len() == cells.len() {
            for z in z0 as usize..z1 as usize {
                let (a, b) = (z * row + x0 as usize * 4, z * row + x1 as usize * 4);
                w.shore_cells[a..b].copy_from_slice(&cells[a..b]);
            }
        } else {
            w.shore_cells.clear();
            w.shore_cells.extend_from_slice(cells);
        }
    }

    /// Where a world map sits in the scene (after `set_world_ground`).
    pub fn set_world_place(&mut self, map: &str, place: Place) {
        if let Some(w) = self.worlds.get_mut(map) {
            w.place = place;
        }
        self.grass.place(map, place.offset, place.dim);
        self.blades.place(map, place.offset, place.dim);
    }

    /// A grass model to draw copies of (`set_grass`), by its key (a grass prop's id), with
    /// what it was made from (`version`: sent again when that changes): its levels, the
    /// first as made, each next thinner (`world::grass::thin`), drawn at the finest a copy's
    /// cells look big enough for.
    pub fn set_grass_model(&mut self, device: &wgpu::Device, key: &str, version: u64, model: &GrassModel) {
        self.grass.set_model(device, key, version, model);
    }

    /// What a grass model held was made from (`set_grass_model`).
    pub fn grass_model_version(&self, key: &str) -> Option<u64> {
        self.grass.version(key)
    }

    /// One square's grass copies on a world map (after `set_world_ground`), by model, in the
    /// map's own space, replacing what that square held (none: they go).
    pub fn set_grass(&mut self, map: &str, square: (i32, i32), copies: Vec<(String, Vec<GrassCopy>)>) {
        self.grass.set_copies(map, square, copies);
    }

    /// Whether the grass's copies are culled on the GPU (else on the CPU: no compute shaders).
    pub fn grass_on_gpu(&self) -> bool {
        self.grass.culled_on_gpu()
    }

    /// Every meadow's kinds of blade and flower (`world::meadow::Kinds`, in its order: a
    /// blade's `kind` is its place in it).
    pub fn set_blade_kinds(&mut self, kinds: &[BladeKind]) {
        self.blades.set_kinds(kinds);
    }

    /// One square's meadows' blades on a world map (after `set_world_ground`), in the map's own
    /// space, replacing what that square held (none: they go).
    pub fn set_blades(&mut self, map: &str, square: (i32, i32), blades: Vec<BladeInstance>) {
        self.blades.set_blades(map, square, blades);
    }

    /// Whether the meadows' blades are culled on the GPU (else on the CPU: no compute shaders).
    pub fn blades_on_gpu(&self) -> bool {
        self.blades.culled_on_gpu()
    }

    /// One square of one layer of a world map (after `set_world_ground`), replacing what
    /// that square held: "tops" is the ground's cell tops and CLIFFS its cliff faces (the
    /// terrain shader); any other name ("grass", "props", "trees", "buildings") draws like
    /// props. No batches: the square goes. Batches by texture key (`WHITE` for vertex
    /// colours alone); a texture that isn't loaded draws white.
    pub fn set_chunk(&mut self, device: &wgpu::Device, map: &str, layer: &str, square: (i32, i32), batches: &[(String, Vec<MapVertex>, Vec<u32>)]) {
        let Some(w) = self.worlds.get_mut(map) else { return };
        let meshes: Vec<(String, GpuMesh)> = batches.iter().flat_map(|(k, v, i)| GpuMesh::chunked(device, v, i).into_iter().map(|m| (k.clone(), m))).collect();
        let squares = if layer == "tops" { &mut w.tops } else { w.layers.entry(layer.to_string()).or_default() };
        if meshes.is_empty() {
            squares.remove(&square);
        } else {
            squares.insert(square, Square::new(meshes));
        }
    }

    /// Drops a world map's squares that `keep` doesn't want, in every layer.
    pub fn keep_chunks(&mut self, map: &str, keep: &dyn Fn((i32, i32)) -> bool) {
        self.grass.keep(map, keep);
        self.blades.keep(map, keep);
        let Some(w) = self.worlds.get_mut(map) else { return };
        w.tops.retain(|s, _| keep(*s));
        for squares in w.layers.values_mut() {
            squares.retain(|s, _| keep(*s));
        }
    }

    /// Forgets the world maps `keep` doesn't name.
    pub fn keep_worlds(&mut self, keep: &dyn Fn(&str) -> bool) {
        self.worlds.retain(|id, _| keep(id));
        self.grass.keep_maps(keep);
        self.blades.keep_maps(keep);
    }

    /// The world maps on the GPU.
    pub fn world_ids(&self) -> Vec<String> {
        self.worlds.keys().cloned().collect()
    }

    /// What moves on the world, in the scene's own space; empty clears it.
    pub fn set_world_extra(&mut self, device: &wgpu::Device, batches: &[(String, Vec<MapVertex>, Vec<u32>)]) {
        self.extra = batches.iter().filter_map(|(k, v, i)| GpuMesh::new(device, v, i).map(|m| (k.clone(), m))).collect();
    }

    /// What fades this frame: drawn after everything opaque, as see-through as its
    /// vertices' alpha, each pixel once (its nearest surface).
    pub fn set_world_fade(&mut self, device: &wgpu::Device, batches: &[(String, Vec<MapVertex>, Vec<u32>)]) {
        self.fade = batches.iter().filter_map(|(k, v, i)| GpuMesh::new(device, v, i).map(|m| (k.clone(), m))).collect();
    }

    pub fn clear_world(&mut self) {
        self.worlds.clear();
        self.extra.clear();
        self.fade.clear();
        self.grass.clear();
        self.blades.clear();
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
        let screen = [size[0].max(1), size[1].max(1)];
        // the pixelation: the scene with fewer pixels, each a square of the screen's (as dense
        // as asked, as big as the camera's zoom makes them); else as much of the screen's
        // resolution as the quality says
        let pixel = frame.retro.pixelation.map(|d| pixel_size(screen, d, frame.retro.zoom)).filter(|&k| k > 1);
        let (size, scale) = match pixel {
            Some(k) => ([screen[0].div_ceil(k), screen[1].div_ceil(k)], 1.0 / k as f32),
            None => {
                let s = self.quality.scale;
                ([((screen[0] as f32 * s).round() as u32).max(1), ((screen[1] as f32 * s).round() as u32).max(1)], s)
            }
        };
        if self.target.as_ref().map(|t| t.size != size).unwrap_or(true) {
            self.target = Some(self.make_target(device, size));
        }
        let frost_size = [screen[0].div_ceil(FROST_DIV), screen[1].div_ceil(FROST_DIV)];
        if frame.frost && self.frost.as_ref().map(|f| f.size != frost_size).unwrap_or(true) {
            self.frost = Some(self.make_frost(device, frost_size));
            self.frost_generation += 1;
        }
        let l = &frame.light;
        let t = &frame.toon;
        self.wind_clock.step(frame.time, frame.wind, t.cloud_speed);
        let clock = self.wind_clock;
        // the ink lines cost a look at every pixel's neighbours: only when something toon
        // is drawn
        let toon_seen = frame.billboards.iter().any(|b| b.toon) || frame.animated.iter().any(|a| self.skinned.get(&a.key).map(|p| p.iter().any(|(_, m)| m.toon)).unwrap_or(false)) || self.worlds.values().flat_map(|w| w.layers.values()).flat_map(|l| l.values()).any(|s| s.toon) || self.extra.iter().chain(&self.fade).any(|(_, m)| m.toon) || self.grass.toon();
        self.sky = l.sky.to_array();
        let shadows = frame.shadows && self.quality.shadow_size > 0;
        let light_view_proj = on_texels(&l.light_view_proj, self.shadow_size);
        // the indirect light: the region round the camera's target, when there are cascades
        // to run and world maps to light
        let gi_region = match &self.gi {
            Some(_) if self.quality.indirect && frame.radiance.gi > 0.0 && !self.worlds.is_empty() => Some(Region::around(&frame.view_proj, frame.camera_pos, 2.0 * frame.radiance.gi_range)),
            _ => None,
        };
        // the clouds' shadows are the sun's: as strong as it is (a day's full, the moon's weak)
        let sun_strength = (l.sun.dot(Vec3::new(0.2126, 0.7152, 0.0722)) / 0.78).clamp(0.2, 1.0);
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
            world: [0.0, 0.0, self.worlds.values().next().map(|w| w.level_height).unwrap_or(2.0), frame.time],
            editor: [frame.grid as u32 as f32, if frame.focus.is_some() { frame.dim.clamp(0.0, 1.0) } else { 0.0 }, 0.0, 0.0],
            shadow: shadow_params(&light_view_proj, self.shadow_size),
            focus: frame.focus.unwrap_or([0.0; 4]),
            light_count: [l.lights.len().min(MAX_LIGHTS) as f32, 0.0, 0.0, 0.0],
            toon_a: [t.bands.clamp(1, 5) as f32, t.softness.clamp(0.0, 1.0), t.shadow.clamp(0.0, 1.0), t.rim.max(0.0)],
            toon_b: [t.shadow_color.x, t.shadow_color.y, t.shadow_color.z, t.rim_width.clamp(0.0, 1.0)],
            toon_c: [t.glint.max(0.0), t.glint_size.clamp(0.0, 1.0), (t.ink_width * scale).max(1.0), t.ink_opacity.clamp(0.0, 1.0)],
            toon_d: [t.ink_color.x, t.ink_color.y, t.ink_color.z, t.depth_step.max(0.0)],
            toon_e: [if t.crease > 0.0 { t.crease.clamp(1.0, 179.0).to_radians().cos() } else { 2.0 }, (t.ink && toon_seen) as u32 as f32, t.color_steps.min(32) as f32, t.color_softness.clamp(0.0, 1.0)],
            inv_view_proj: frame.view_proj.inverse().to_cols_array_2d(),
            clouds_a: [t.cloud_shadow.clamp(0.0, 1.0) * sun_strength, 1.0 / t.cloud_size.max(1.0), t.cloud_cover.clamp(0.0, 1.0), 0.1],
            // the clouds drift with the wind (how far they have), morphing as they go
            clouds_b: [clock.clouds.x, clock.clouds.y, 28.0, clock.morph as f32],
            retro_a: [
                pixel.unwrap_or(0) as f32,
                frame.retro.lcd.map_or(0.0, Handheld::code),
                match (pixel, frame.retro.lcd) {
                    (Some(_), Some(h)) => h.grid(),
                    _ => 0.0,
                },
                // in 15-bit colours while it's on, however big its pixels just now
                if frame.retro.pixelation.is_some_and(|d| d < 1.0) { 31.0 } else { 0.0 },
            ],
            retro_b: [screen[0] as f32, screen[1] as f32, 0.0, 0.0],
            wind: {
                let way = frame.wind.way.normalize_or(CLOUD_WIND);
                [way.x, way.y, frame.wind.strength.clamp(0.0, 1.0), frame.wind.gusts.clamp(0.0, 1.0)]
            },
            wind_b: [clock.gusts.x, clock.gusts.y, clock.sway as f32, clock.flutter as f32],
            wind_c: [clock.ripple as f32, clock.lap as f32, 0.0, 0.0],
            see_through: frame.see_through.map_or([0.0; 4], |(p, r)| [p.x, p.y, p.z, r.max(0.0)]),
            tone: [frame.radiance.exposure.max(0.0), frame.radiance.bloom.max(0.0), frame.radiance.filmic as u32 as f32, frame.radiance.threshold.max(0.0)],
            gi: gi_region.map_or([0.0; 4], |r| [frame.radiance.gi.max(0.0), 1.0 / r.width, r.width, r.spacing()]),
            gi_b: gi_region.map_or([0.0; 4], |r| [r.origin.x, r.origin.y, crate::gi::BOUNCE, crate::gi::SCREEN]),
            push: [frame.pushers.len().min(MAX_PUSHERS) as f32, 0.0, 0.0, 0.0],
            pushers: {
                let mut out = [[0.0f32; 4]; MAX_PUSHERS];
                for (k, (p, r)) in frame.pushers.iter().take(MAX_PUSHERS).enumerate() {
                    out[k] = [p.x, p.y, p.z, r.max(0.01)];
                }
                out
            },
            lights: {
                let mut out = [[0.0f32; 4]; MAX_LIGHTS * 3];
                for (k, s) in l.lights.iter().take(MAX_LIGHTS).enumerate() {
                    let d = s.dir.normalize_or(Vec3::NEG_Y);
                    out[k * 3] = [s.pos.x, s.pos.y, s.pos.z, s.range.max(0.01)];
                    out[k * 3 + 1] = [d.x, d.y, d.z, s.outer.cos()];
                    out[k * 3 + 2] = [s.color.x, s.color.y, s.color.z, s.inner.cos()];
                }
                out
            },
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        // the world maps' placements: the scene's own space first, then each map's
        let need = self.worlds.len() as u64 + 1;
        if need > self.places_cap {
            (self.places, self.places_cap, self.globals_bind, self.gi_globals_bind) = Self::make_places(device, &self.globals_layout, &self.globals, &self.gi_globals, need.next_power_of_two());
        }
        let mut places = vec![0u8; (need * PLACE_STRIDE) as usize];
        for (k, w) in self.worlds.values().enumerate() {
            let u = PlaceUniform { offset: w.place.offset.extend(0.0).to_array(), grid: [w.place.grid[0], w.place.grid[1], w.place.dim, 0.0] };
            let at = (k as u64 + 1) * PLACE_STRIDE;
            places[at as usize..at as usize + std::mem::size_of::<PlaceUniform>()].copy_from_slice(bytemuck::bytes_of(&u));
        }
        queue.write_buffer(&self.places, 0, &places);
        // each map's placement, by its slot's offset
        let slot = |k: usize| ((k as u64 + 1) * PLACE_STRIDE) as u32;

        // the indirect light: the height pass from straight above the region (the ground,
        // its cliffs, the buildings and the props, through the shadow pipelines), then the
        // field and the cascades, reading what the target last showed shining
        if let (Some(gi), Some(region), Some(target)) = (&self.gi, gi_region, &self.target) {
            let top = region.top_view_proj();
            let mut above = globals;
            above.light_view_proj = top.to_cols_array_2d();
            queue.write_buffer(&self.gi_globals, 0, bytemuck::bytes_of(&above));
            gi.prepare(queue, &region);
            let seen = Frustum::new(&top);
            let nothing = wgpu::Color { r: crate::gi::NOTHING, g: 0.0, b: 0.0, a: 1.0 };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gi heights"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &gi.height_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(nothing), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gi.height_depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.height_pipeline);
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            let mut bound = WHITE;
            for (k, w) in self.worlds.values().enumerate() {
                pass.set_bind_group(0, &self.gi_globals_bind, &[slot(k)]);
                let off = w.place.offset;
                let blocking = w.layers.iter().filter(|(name, _)| matches!(name.as_str(), CLIFFS | "buildings" | "props")).flat_map(|(_, l)| l.values());
                for sq in w.tops.values().chain(blocking) {
                    if !seen.sees(sq.lo + off, sq.hi + off) {
                        continue;
                    }
                    for (key, m) in &sq.pieces {
                        if !seen.sees(m.lo + off, m.hi + off) {
                            continue;
                        }
                        if key.as_str() != bound {
                            pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                            bound = key.as_str();
                        }
                        m.draw(&mut pass);
                    }
                }
            }
            drop(pass);
            let grounds: Vec<(u32, &wgpu::BindGroup)> = self.worlds.values().enumerate().map(|(k, w)| (slot(k), &w.bind)).collect();
            if let Some(bind) = &target.gi_bind {
                gi.record(encoder, &self.globals_bind, &self.empty_bind, bind, &grounds);
            }
        }

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
            let toon = b.toon as u32 as f32;
            let v = |pos: [f32; 3], uv: [f32; 2]| SpriteVertex { pos, uv, color: c, toon };
            let quad = [v(p(x0, y1), [u0, v0]), v(p(x0, y0), [u0, v1]), v(p(x1, y0), [u1, v1]), v(p(x0, y1), [u0, v0]), v(p(x1, y0), [u1, v1]), v(p(x1, y1), [u1, v0])];
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
                        vertices.push(SpriteVertex::plain(s.center.to_array(), [0.5, 0.5], c));
                        vertices.push(SpriteVertex::plain(at(radius, a0, squash), [0.5, 0.5], c));
                        vertices.push(SpriteVertex::plain(at(radius, a1, squash), [0.5, 0.5], c));
                    }
                    ShapeKind::Ring { inner, outer } => {
                        let v = |r: f32, a: f32| SpriteVertex::plain(at(r, a, 1.0), [0.5, 0.5], c);
                        vertices.extend_from_slice(&[v(inner, a0), v(outer, a0), v(outer, a1), v(inner, a0), v(outer, a1), v(inner, a1)]);
                    }
                    ShapeKind::Quad { corners } => {
                        if k == 0 {
                            let v = |p: Vec3| SpriteVertex::plain((s.center + p).to_array(), [0.5, 0.5], c);
                            vertices.extend_from_slice(&[v(corners[0]), v(corners[1]), v(corners[2]), v(corners[0]), v(corners[2]), v(corners[3])]);
                        }
                    }
                    ShapeKind::Rect { width, depth } => {
                        if k == 0 {
                            let (w, d) = (width / 2.0, depth / 2.0);
                            let v = |x: f32, z: f32| SpriteVertex::plain((s.center + Vec3::new(x, 0.0, z)).to_array(), [0.5, 0.5], c);
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
            let v = |p: Vec3, uv: [f32; 2]| SpriteVertex::plain(p.to_array(), uv, c);
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
            let v = |p: Vec3| SpriteVertex::plain(p.to_array(), [0.5, 0.5], c);
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
        // the skinned models' joints, each model's placed in the world, in one buffer (a
        // uniform one: a model's first BONES_WINDOW, all its window holds)
        let room = if self.bones.stored { usize::MAX } else { BONES_WINDOW };
        let mut bone_data: Vec<[[f32; 4]; 4]> = vec![];
        let mut animated: Vec<(&str, u32)> = vec![];
        for a in frame.animated.iter().filter(|a| self.skinned.contains_key(&a.key) && !a.joints.is_empty()) {
            animated.push((a.key.as_str(), (bone_data.len() * 64) as u32));
            bone_data.extend(a.joints.iter().take(room).map(|j| (a.model * *j).to_cols_array_2d()));
            while !bone_data.len().is_multiple_of(BONES_ALIGN) {
                bone_data.push(Mat4::IDENTITY.to_cols_array_2d());
            }
        }
        let bone_bytes: &[u8] = bytemuck::cast_slice(&bone_data);
        let most = frame.animated.iter().filter(|a| self.skinned.contains_key(&a.key)).map(|a| a.joints.len().min(room)).max().unwrap_or(0);
        let (cap, window) = bones_needed(bone_bytes.len() as u64, most, self.bones.cap, self.bones.window);
        if (cap, window) != (self.bones.cap, self.bones.window) {
            self.bones = Self::make_bones(device, &self.bones_layout, cap, window, self.bones.stored);
        }
        if !bone_bytes.is_empty() {
            queue.write_buffer(&self.bones.buffer, 0, bone_bytes);
        }

        // the grass's copies culled, each its level picked (on the GPU, a pass before these),
        // by the picture as it is before its pixelation: that only changes how it's drawn, not
        // how fine the grass is (picked by the pixelation's few big pixels, it went sparse)
        let unpixelated = [((screen[0] as f32 * self.quality.scale).round() as u32).max(1), ((screen[1] as f32 * self.quality.scale).round() as u32).max(1)];
        self.grass.prepare(device, queue, encoder, &frame.view_proj, unpixelated);
        self.blades.prepare(device, queue, encoder, &frame.view_proj, unpixelated);
        let (grass, grass_vertices, grass_levels) = self.grass.drawn();
        let (blades, blade_vertices, blade_templates) = self.blades.drawn();
        let mut stats = DrawStats {
            pixel: pixel.unwrap_or(1),
            grass,
            grass_vertices,
            grass_on_gpu: self.grass.culled_on_gpu(),
            grass_levels,
            blades,
            blade_vertices,
            blade_templates,
            blades_on_gpu: self.blades.culled_on_gpu(),
            ..Default::default()
        };
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
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            pass.set_pipeline(&self.map_shadow_pipeline);
            for m in &self.meshes {
                let Some(t) = self.textures.get(&m.sheet) else { continue };
                pass.set_bind_group(1, &t.bind, &[]);
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.count, 0, 0..1);
            }
            pass.set_bind_group(1, &self.textures[WHITE].bind, &[]);
            let mut bound = WHITE;
            // the pieces of each map's squares the sun sees (out of its sight, a square's
            // pieces all left out at once), then the scene's own
            let mut objects = vec![];
            for (k, w) in self.worlds.values().enumerate() {
                let off = w.place.offset;
                for sq in w.layers.values().flat_map(|l| l.values()) {
                    if sun.sees(sq.lo + off, sq.hi + off) {
                        objects.extend(sq.pieces.iter().map(|(key, m)| (slot(k), off, key.as_str(), m)));
                    } else {
                        stats.culled += sq.pieces.len() as u32;
                    }
                }
            }
            let own = self.extra.iter().chain(&self.fade).map(|(key, m)| (0, Vec3::ZERO, key.as_str(), m));
            let mut placed = None;
            for (at, off, key, m) in objects.into_iter().chain(own) {
                if !sun.sees(m.lo + off, m.hi + off) {
                    stats.culled += 1;
                    continue;
                }
                if placed != Some(at) {
                    pass.set_bind_group(0, &self.globals_bind, &[at]);
                    placed = Some(at);
                }
                if key != bound {
                    pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                    bound = key;
                }
                m.draw(&mut pass);
                stats.shadow_draws += 1;
                stats.shadow_triangles += m.count / 3;
            }
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            self.grass.draw(&mut pass, true, &|k| &self.textures.get(k).unwrap_or(&self.textures[WHITE]).bind, &mut stats);
            pass.set_bind_group(2, &self.empty_bind, &[]);
            self.blades.draw(&mut pass, true, &mut stats);
            pass.set_pipeline(&self.sprite_shadow_pipeline);
            pass.set_vertex_buffer(0, sprite_buffer.slice(..));
            for d in &self.sprite_draws {
                pass.set_bind_group(1, &self.textures[&d.texture].bind, &[]);
                pass.draw(d.first..d.first + d.count, 0..1);
            }
            if !animated.is_empty() {
                pass.set_pipeline(&self.skinned_shadow_pipeline);
                pass.set_bind_group(2, &self.empty_bind, &[]);
                for &(key, offset) in &animated {
                    pass.set_bind_group(3, &self.bones.bind, &[offset]);
                    for (texture, m) in &self.skinned[key] {
                        pass.set_bind_group(1, &self.textures.get(texture).unwrap_or(&self.textures[WHITE]).bind, &[]);
                        m.draw(&mut pass);
                        stats.shadow_draws += 1;
                        stats.shadow_triangles += m.count / 3;
                    }
                }
            }
        }

        // the scene: what's solid, writing its look too
        let target = self.target.as_ref().unwrap();
        let sky = l.sky;
        let mut pass = self.solid_pass(encoder, target, wgpu::Color { r: sky.x as f64, g: sky.y as f64, b: sky.z as f64, a: 1.0 });
        pass.set_bind_group(0, &self.globals_bind, &[0]);
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
        if !self.worlds.is_empty() || !self.extra.is_empty() {
            // what's in sight, nearest first: what's behind is then hidden before it's shaded
            let mut seen: Vec<(f32, u32, &str, &GpuMesh, bool)> = vec![];
            let through = frame.see_through.is_some();
            // (a square out of sight: its pieces all left out at once)
            let mut items = vec![];
            for (k, w) in self.worlds.values().enumerate() {
                let (at, off) = (slot(k), w.place.offset);
                for (name, l) in w.layers.iter().filter(|(name, _)| name.as_str() != CLIFFS) {
                    let clear = through && matches!(name.as_str(), "trees" | "props");
                    for sq in l.values() {
                        if eye.sees(sq.lo + off, sq.hi + off) {
                            items.extend(sq.pieces.iter().map(|(key, m)| (at, off, key.as_str(), m, clear)));
                        } else {
                            stats.culled += sq.pieces.len() as u32;
                        }
                    }
                }
            }
            let own = self.extra.iter().map(|(key, m)| (0u32, Vec3::ZERO, key.as_str(), m, false));
            for (at, off, key, m, clear) in items.into_iter().chain(own) {
                if eye.sees(m.lo + off, m.hi + off) {
                    seen.push((((m.lo + m.hi) * 0.5 + off).distance_squared(frame.camera_pos), at, key, m, clear));
                } else {
                    stats.culled += 1;
                }
            }
            seen.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut bound: Option<&str> = None;
            let mut cutout: Option<(bool, bool)> = None;
            let mut placed: Option<u32> = None;
            for (_, at, key, m, clear) in seen {
                let cuts = key != WHITE;
                if cutout != Some((cuts, clear)) {
                    pass.set_pipeline(match (cuts, clear) {
                        (true, false) => &self.map_pipeline,
                        (false, false) => &self.opaque_pipeline,
                        (true, true) => &self.through_pipeline,
                        (false, true) => &self.opaque_through_pipeline,
                    });
                    cutout = Some((cuts, clear));
                }
                if placed != Some(at) {
                    pass.set_bind_group(0, &self.globals_bind, &[at]);
                    placed = Some(at);
                }
                if bound != Some(key) {
                    pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                    bound = Some(key);
                }
                m.draw(&mut pass);
                stats.draws += 1;
                stats.triangles += m.count / 3;
            }
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            // the skinned models, in their poses
            if !animated.is_empty() {
                pass.set_pipeline(&self.skinned_pipeline);
                for &(key, offset) in &animated {
                    pass.set_bind_group(3, &self.bones.bind, &[offset]);
                    for (texture, m) in &self.skinned[key] {
                        pass.set_bind_group(1, &self.textures.get(texture).unwrap_or(&self.textures[WHITE]).bind, &[]);
                        m.draw(&mut pass);
                        stats.draws += 1;
                        stats.triangles += m.count / 3;
                    }
                }
            }
            // the grass's copies and the meadows' blades (in the scene's space: its own
            // placement), over the ground
            pass.set_bind_group(0, &self.globals_bind, &[0]);
            self.grass.draw(&mut pass, false, &|k| &self.textures.get(k).unwrap_or(&self.textures[WHITE]).bind, &mut stats);
            self.blades.draw(&mut pass, false, &mut stats);
            // the ground last, its cliffs too: what stands on it hides most of it, and its
            // shader is the costliest
            pass.set_pipeline(&self.ground_pipeline);
            for (k, w) in self.worlds.values().enumerate() {
                pass.set_bind_group(0, &self.globals_bind, &[slot(k)]);
                pass.set_bind_group(1, &w.bind, &[]);
                let off = w.place.offset;
                let cliffs = w.layers.get(CLIFFS).into_iter().flat_map(|l| l.values());
                for sq in w.tops.values().chain(cliffs) {
                    if !eye.sees(sq.lo + off, sq.hi + off) {
                        stats.culled += sq.pieces.len() as u32;
                        continue;
                    }
                    for (_, m) in &sq.pieces {
                        if !eye.sees(m.lo + off, m.hi + off) {
                            stats.culled += 1;
                            continue;
                        }
                        m.draw(&mut pass);
                        stats.draws += 1;
                        stats.triangles += m.count / 3;
                    }
                }
            }
            pass.set_bind_group(0, &self.globals_bind, &[0]);
        }
        if !self.sprite_draws.is_empty() {
            pass.set_vertex_buffer(0, sprite_buffer.slice(..));
            pass.set_pipeline(&self.sprite_pipeline);
            for d in &self.sprite_draws {
                pass.set_bind_group(1, &self.textures[&d.texture].bind, &[]);
                pass.draw(d.first..d.first + d.count, 0..1);
                stats.draws += 1;
                stats.triangles += d.count / 3;
            }
        }
        drop(pass);
        // then what's see-through, over it
        let mut pass = Self::clear_pass(encoder, target);
        pass.set_bind_group(0, &self.globals_bind, &[0]);
        pass.set_bind_group(2, &self.shadow_bind, &[]);
        // what fades: its nearest surfaces' depth first, then each pixel blended once
        if !self.fade.is_empty() {
            for (blend, pipeline) in [(false, &self.fade_depth_pipeline), (true, &self.fade_pipeline)] {
                pass.set_pipeline(pipeline);
                for (key, m) in &self.fade {
                    pass.set_bind_group(1, &self.textures.get(key).unwrap_or(&self.textures[WHITE]).bind, &[]);
                    m.draw(&mut pass);
                    if blend {
                        stats.draws += 1;
                        stats.triangles += m.count / 3;
                    }
                }
            }
        }
        pass.set_vertex_buffer(0, sprite_buffer.slice(..));
        for (pipeline, draws) in [(&self.blend_pipeline, &self.blend_draws), (&self.add_pipeline, &self.add_draws), (&self.overlay_pipeline, &self.overlay_draws)] {
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
        drop(pass);
        // the bloom: what shines (and what's too bright) at half size, halved down the
        // chain, then doubled back up, each level added onto the finer one
        if frame.radiance.bloom > 0.0 {
            let n = target.bloom_views.len();
            let draw = |encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, bind: &wgpu::BindGroup, into: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>| {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bloom"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: into, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store } })],
                    ..Default::default()
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[0]);
                pass.set_bind_group(1, bind, &[]);
                pass.draw(0..3, 0..1);
            };
            let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
            draw(encoder, &self.bloom_first_pipeline, &target.bloom_first_bind, &target.bloom_views[0], clear);
            for k in 1..n {
                draw(encoder, &self.bloom_down_pipeline, &target.bloom_binds[k - 1], &target.bloom_views[k], clear);
            }
            for k in (0..n - 1).rev() {
                draw(encoder, &self.bloom_up_pipeline, &target.bloom_binds[k + 1], &target.bloom_views[k], wgpu::LoadOp::Load);
            }
        }
        // the frosted glass: the finished picture small, blurred across, then down
        if let (true, Some(frost)) = (frame.frost, &self.frost) {
            let draw = |encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, bind: &wgpu::BindGroup, into: &wgpu::TextureView| {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frost"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: into, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    ..Default::default()
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[0]);
                pass.set_bind_group(1, bind, &[]);
                pass.draw(0..3, 0..1);
            };
            draw(encoder, &self.frost_pipeline, &target.post_bind, &frost.views[0]);
            draw(encoder, &self.frost_x_pipeline, &frost.binds[0], &frost.views[1]);
            draw(encoder, &self.frost_y_pipeline, &frost.binds[1], &frost.views[0]);
        }
        self.stats = stats;
    }

    /// Draws the scene (prepared by `prepare`) into the host's pass, with the ink lines.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(target) = &self.target else { return };
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &self.globals_bind, &[0]);
        pass.set_bind_group(1, &target.post_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The pass for what's solid: colour (cleared to `sky`), looks and what shines (cleared
    /// to nothing), the depth kept for the see-through pass.
    fn solid_pass<'e>(&self, encoder: &'e mut wgpu::CommandEncoder, target: &Target, sky: wgpu::Color) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment { view: &target.color_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(sky), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &target.look_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &target.emit_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }

    /// The pass for what's see-through, over the solid one's colour and depth.
    fn clear_pass<'e>(encoder: &'e mut wgpu::CommandEncoder, target: &Target) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("see-through"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target.color_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
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
        let looks = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene looks"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LOOK,
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
        let emit = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene shining"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EMIT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        // the bloom's chain: half the picture's size, halved again down to a few texels, a
        // texture a level (not one texture's mips: WebGL 2's views of one mip don't read
        // as that mip, and the chain smeared the glow across the whole picture)
        let half = [size[0].div_ceil(2).max(1), size[1].div_ceil(2).max(1)];
        let mips = BLOOM_MIPS.min(half[0].min(half[1]).ilog2() + 1).max(1);
        let bloom_views: Vec<wgpu::TextureView> = (0..mips)
            .map(|k| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("bloom"),
                        size: wgpu::Extent3d { width: (half[0] >> k).max(1), height: (half[1] >> k).max(1), depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: EMIT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            })
            .collect();
        let bloom_binds = bloom_views
            .iter()
            .map(|v| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bloom"),
                    layout: &self.frost_layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.linear) }],
                })
            })
            .collect();
        let color_view = color.create_view(&Default::default());
        let look_view = looks.create_view(&Default::default());
        let emit_view = emit.create_view(&Default::default());
        let post_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &self.post_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color_view) },
                // at a lower resolution, smoothed on the way up
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if self.quality.scale < 1.0 { &self.linear } else { &self.nearest }) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&look_view) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&bloom_views[0]) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&self.linear) },
            ],
        });
        let bloom_first_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom first"),
            layout: &self.bloom_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.linear) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&emit_view) },
            ],
        });
        let gi_bind = self.gi.as_ref().map(|gi| gi.target_bind(device, &emit_view, &look_view));
        Target { size, color_view, look_view, emit_view, depth_view: depth.create_view(&Default::default()), post_bind, bloom_first_bind, bloom_views, bloom_binds, gi_bind }
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

    /// A future that's ready at once (wgpu's are, on the desktop) run to its end.
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        let mut f = std::pin::pin!(f);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::yield_now();
        }
    }

    /// A GPU test's turn, held from its start (taken before its devices, let go after them):
    /// the tests take turns, a device made while another test's were at work or being torn
    /// down having hung the driver (Vulkan's, now and then).
    fn turn() -> std::sync::MutexGuard<'static, ()> {
        static TURNS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        TURNS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A device on the GPU there is, if it does all WebGPU does (vertex shaders reading
    /// storage buffers: the skinned models' joints, the blades from the culling's lists); none
    /// on a machine without (CI's).
    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let adapter = block_on(wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        if !adapter.get_downlevel_capabilities().is_webgpu_compliant() {
            return None;
        }
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// A device on the GPU there is, held to what the game gets in a browser on WebGL 2 (the
    /// limits eframe asks for there): no storage buffers, uniform blocks of 16 KB.
    fn webgl_limits() -> Option<(wgpu::Device, wgpu::Queue)> {
        let adapter = block_on(wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let required_limits = wgpu::Limits { max_texture_dimension_2d: 8192, ..wgpu::Limits::downlevel_webgl2_defaults() };
        block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits, ..Default::default() })).ok()
    }

    /// A device on OpenGL (as WebGL 2 is, through ANGLE or the driver), where this machine has one.
    fn gl() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::GL, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })).ok()
    }

    /// A grey floor round the origin, 8 cells a side, in the scene's own space: a world's to
    /// draw, the skinned models with it.
    fn floor(r: &mut SceneRenderer, device: &wgpu::Device) {
        let corner = |x: f32, z: f32| MapVertex { pos: [x, -0.01, z], normal: [0.0, 1.0, 0.0], uv: [0.5, 0.5], color: [0.5, 0.5, 0.5, 1.0], toon: 0.0, wind: [0; 4] };
        r.set_world_extra(device, &[(WHITE.to_string(), vec![corner(-4.0, -4.0), corner(4.0, -4.0), corner(4.0, 4.0), corner(-4.0, 4.0)], vec![0, 1, 2, 0, 2, 3])]);
    }

    /// A frame of nothing but `animated`, from a camera looking down at the origin.
    fn frame_of(animated: Vec<Animated>) -> Frame {
        let light = Lighting {
            sun_dir: Vec3::new(0.3, 1.0, 0.2),
            sun: Vec3::ONE,
            ambient: Vec3::splat(0.3),
            hemi_sky: Vec3::splat(0.2),
            hemi_ground: Vec3::splat(0.1),
            sky: Vec3::new(0.5, 0.7, 0.9),
            fog_near: 50.0,
            fog_far: 100.0,
            light_view_proj: Mat4::orthographic_rh(-10.0, 10.0, -10.0, 10.0, -20.0, 20.0) * Mat4::look_at_rh(Vec3::new(3.0, 10.0, 2.0), Vec3::ZERO, Vec3::Y),
            lights: vec![],
        };
        Frame {
            view_proj: Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0) * Mat4::look_at_rh(Vec3::new(0.0, 5.0, 5.0), Vec3::ZERO, Vec3::Y),
            camera_pos: Vec3::new(0.0, 5.0, 5.0),
            light,
            shadows: true,
            billboards: vec![],
            shapes: vec![],
            glows: vec![],
            animated,
            streaks: vec![],
            time: 0.0,
            grid: false,
            focus: None,
            dim: 0.0,
            overlays: vec![],
            toon: Toon::default(),
            retro: Retro::default(),
            frost: false,
            wind: Wind { way: Vec2::new(1.0, 0.0), strength: 0.6, gusts: 0.5 },
            see_through: None,
            pushers: vec![],
            radiance: Radiance::default(),
        }
    }

    #[test]
    fn the_bones_buffer_leaves_a_window_past_the_last_model() {
        let window = (BONES_WINDOW as u64) * MAT4_BYTES;
        assert_eq!(bones_needed(0, 0, 0, 0), (window, window), "one model at offset 0");
        // a second model at 256 bytes: its window must fit after it
        let (cap, w) = bones_needed(512, 7, window, window);
        assert!(w == window && cap >= 256 + window && cap.is_power_of_two(), "{cap} {w}");
        // a model of more joints than the window: a wider one, the buffer past it
        let (cap, w) = bones_needed(300 * 64, 300, cap, w);
        assert!(w == 300 * 64 && cap >= 300 * 64 + w, "{cap} {w}");
        // never smaller than what's there
        assert_eq!(bones_needed(64, 1, 1 << 20, w), (1 << 20, w));
    }

    /// Skinned models in one frame, as many as a map has, each with its joints (the model
    /// requests' creatures have 3, 7 and 32; a joint binding takes up to BONES_WINDOW of them,
    /// one of more widens it, or on WebGL 2 keeps its first): drawn with no error from the GPU
    /// (a second one used to overrun the bones' buffer, and wgpu's error ended the game), from
    /// storage as on WebGPU and from a uniform buffer as on WebGL 2.
    #[test]
    fn many_skinned_models_draw_in_one_frame() {
        let _turn = turn();
        let devices: Vec<_> = [gpu(), webgl_limits()].into_iter().flatten().collect();
        if devices.is_empty() {
            eprintln!("no GPU here: the skinned models' draw not checked");
        }
        for (device, queue) in devices {
            let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
            let stored = device.limits().max_storage_buffers_per_shader_stage > 0;
            assert_eq!(r.bones.stored, stored);
            floor(&mut r, &device);
            let vertex = |x: f32, z: f32, joint: u32| SkinnedVertex { pos: [x, 0.0, z], normal: [0.0, 1.0, 0.0], uv: [0.0; 2], color: [1.0; 4], joints: [joint, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0], toon: 0.0, wind: [200, 100, 50, 30] };
            let counts = [3usize, 7, 32, 2, 7, 3, 200, 32, 1, 300];
            for (i, &n) in counts.iter().enumerate() {
                let x = i as f32 * 0.5 - 2.0;
                let last = n as u32 - 1;
                r.set_skinned(&device, &format!("m{i}"), &[(WHITE.to_string(), vec![vertex(x, 0.0, 0), vertex(x + 0.4, 0.0, last), vertex(x, 0.4, last)], vec![0, 1, 2])]);
            }
            for shown in [1, 2, 5, counts.len()] {
                let animated = counts.iter().enumerate().take(shown).map(|(i, &n)| Animated { key: format!("m{i}"), model: Mat4::IDENTITY, joints: vec![Mat4::IDENTITY; n] }).collect();
                let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
                let mut encoder = device.create_command_encoder(&Default::default());
                r.prepare(&device, &queue, &mut encoder, &frame_of(animated), [64, 64]);
                queue.submit([encoder.finish()]);
                let error = block_on(scope.pop());
                assert!(error.is_none(), "{shown} skinned models (stored {stored}): {error:?}");
            }
            assert_eq!(r.bones.window, if stored { 300 * MAT4_BYTES } else { BONES_WINDOW as u64 * MAT4_BYTES });
        }
    }

    /// A skinned model drawn where its joints put it (each vertex half on two of them), the
    /// same from a uniform buffer (WebGL 2's way) as from storage, past other models' joints.
    #[test]
    fn skinned_models_pose_alike_from_storage_and_a_uniform_buffer() {
        let _turn = turn();
        let pictures: Vec<Vec<u8>> = [gpu(), webgl_limits()]
            .into_iter()
            .flatten()
            .map(|(device, queue)| {
                let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
                floor(&mut r, &device);
                // a crimson square at the origin, two cells wide: joints 5 and 6 carry it a cell
                // and a half east between them (another model's 7 joints first)
                let corner = |x: f32, z: f32| SkinnedVertex { pos: [x, 0.0, z], normal: [0.0, 1.0, 0.0], uv: [0.0; 2], color: [1.0, 0.0, 0.0, 1.0], joints: [5, 6, 0, 0], weights: [0.5, 0.5, 0.0, 0.0], toon: 0.0, wind: [0; 4] };
                let square = vec![corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
                r.set_skinned(&device, "square", &[(WHITE.to_string(), square, vec![0, 1, 2, 0, 2, 3])]);
                r.set_skinned(&device, "other", &[(WHITE.to_string(), vec![corner(0.0, 0.0); 3], vec![0, 1, 2])]);
                let mut joints = vec![Mat4::from_translation(Vec3::new(9.0, 0.0, 0.0)); 7];
                joints[5] = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
                joints[6] = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0));
                let other = Animated { key: "other".into(), model: Mat4::IDENTITY, joints: vec![Mat4::from_translation(Vec3::new(0.0, -50.0, 0.0)); 7] };
                let frame = frame_of(vec![other, Animated { key: "square".into(), model: Mat4::IDENTITY, joints }]);
                let size = 64;
                let out = device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let read = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (size * size * 4) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                let mut encoder = device.create_command_encoder(&Default::default());
                r.prepare(&device, &queue, &mut encoder, &frame, [size, size]);
                {
                    let view = out.create_view(&Default::default());
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                        ..Default::default()
                    });
                    r.paint(&mut pass);
                }
                encoder.copy_texture_to_buffer(
                    out.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: None } },
                    wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                );
                queue.submit([encoder.finish()]);
                read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                let rgba = read.slice(..).get_mapped_range().unwrap().to_vec();
                rgba
            })
            .collect();
        if pictures.is_empty() {
            eprintln!("no GPU here: the skinned models' poses not checked");
        }
        // the crimson pixels, and the column they're about: the square seen east of the middle
        let crimson: Vec<(usize, f32)> = pictures
            .iter()
            .map(|rgba| {
                let columns: Vec<usize> = rgba.chunks(4).enumerate().filter(|(_, p)| p[0] > 120 && p[1] < 80 && p[2] < 80).map(|(i, _)| i % 64).collect();
                (columns.len(), columns.iter().sum::<usize>() as f32 / columns.len().max(1) as f32)
            })
            .collect();
        for &(n, middle) in &crimson {
            assert!(n > 100 && middle > 40.0, "{n} crimson pixels about column {middle}");
        }
        if let [(n, middle), (m, uniform)] = crimson[..] {
            assert!(n.abs_diff(m) <= n / 50 && (middle - uniform).abs() < 0.5, "from storage {n} about {middle}, from a uniform buffer {m} about {uniform}");
        }
    }

    /// The pixelation's pixels are whole squares of the screen's: at density 0 the picture
    /// about as many as the GBC's screen had (on 1920 by 1080 nines, about 213 by 120, near its
    /// 23040), at 1 the screen's own, evenly between; smaller with the camera further out,
    /// bigger closer, never bigger than the fewest's; a screen too small keeps its own pixels.
    #[test]
    fn the_pixelations_pixels_are_whole_squares_as_dense_as_asked() {
        assert_eq!(pixel_size([1920, 1080], 0.0, 1.0), 9);
        assert_eq!(pixel_size([1400, 900], 0.0, 1.0), 7);
        assert_eq!(pixel_size([1920, 1080], 0.5, 1.0), 3);
        assert_eq!(pixel_size([1920, 1080], 1.0, 1.0), 1);
        assert_eq!(pixel_size([1920, 1080], 1.0, 3.0), 1, "none, however close");
        // the camera further out: smaller; closer: bigger, up to the fewest's
        assert_eq!(pixel_size([1920, 1080], 0.0, 0.4), 4);
        assert_eq!(pixel_size([1920, 1080], 0.5, 2.0), 6);
        assert_eq!(pixel_size([1920, 1080], 0.0, 3.0), 9);
        assert_eq!(pixel_size([200, 150], 0.0, 1.0), 1);
        assert_eq!(pixel_size([0, 0], 0.0, 1.0), 1);
        // the denser, the smaller (never bigger)
        let sizes: Vec<u32> = (0..=20).map(|i| pixel_size([1920, 1080], i as f32 / 20.0, 1.0)).collect();
        assert!(sizes.windows(2).all(|w| w[1] <= w[0]), "{sizes:?}");
    }

    /// A frame pixelated and in an old handheld's LCD colours, with frosted glass: drawn and
    /// blitted with no error from the GPU, the scene that small, the glass's texture there
    /// (its generation the same while the screen's size is).
    #[test]
    fn the_handhelds_screens_and_the_frost_draw() {
        let _turn = turn();
        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU here: the handhelds' screens not drawn");
            return;
        };
        let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let out = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 640, height: 480, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let out = out.create_view(&Default::default());
        let mut generations = vec![];
        for (pixels, lcd) in [(Some(0.2), Some(Handheld::Gba)), (Some(0.0), Some(Handheld::Gbc)), (None, Some(Handheld::Gba)), (Some(0.5), None)] {
            let mut frame = frame_of(vec![]);
            frame.retro = Retro { pixelation: pixels, lcd, ..Default::default() };
            frame.frost = true;
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut encoder = device.create_command_encoder(&Default::default());
            r.prepare(&device, &queue, &mut encoder, &frame, [640, 480]);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &out, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    ..Default::default()
                });
                r.paint(&mut pass);
            }
            queue.submit([encoder.finish()]);
            let error = block_on(scope.pop());
            assert!(error.is_none(), "{pixels:?} {lcd:?}: {error:?}");
            let k = pixels.map(|d| pixel_size([640, 480], d, 1.0)).unwrap_or(1);
            assert_eq!(r.target.as_ref().map(|t| t.size), Some([640u32.div_ceil(k), 480u32.div_ceil(k)]), "{pixels:?}");
            assert_eq!(r.stats().pixel, k);
            generations.push(r.frost_view().map(|(_, g)| g));
        }
        assert!(generations.iter().all(|g| *g == Some(1)), "{generations:?}");
    }

    /// What goes with the wind turns with it and never jumps: the clouds' drift and the gusts
    /// carry on from where they are when the wind changes its way or its strength; a long
    /// pause doesn't throw them far; the phases stay small.
    #[test]
    fn the_winds_clock_turns_and_never_jumps() {
        let mut c = WindClock::default();
        let east = Wind { way: Vec2::X, strength: 0.5, gusts: 0.5 };
        let north = Wind { way: Vec2::NEG_Y, strength: 1.0, gusts: 0.5 };
        let mut t = 0.0f32;
        for _ in 0..36_000 {
            c.step(t, east, 0.2);
            t += 1.0 / 60.0;
        }
        let (clouds, gusts) = (c.clouds, c.gusts);
        assert!((clouds.x - 120.0).abs() < 0.5 && clouds.y.abs() < 1e-3, "ten minutes at 0.2 cells a second, east: {clouds}");
        c.step(t, north, 0.2);
        assert!((c.clouds - clouds).length() < 0.01 && (c.gusts - gusts).length() < 0.3, "turned, not jumped");
        for _ in 0..60 {
            t += 1.0 / 60.0;
            c.step(t, north, 0.2);
        }
        assert!((c.clouds - clouds - Vec2::new(0.0, -0.2)).length() < 0.02, "a second later, 0.2 cells north: {}", c.clouds - clouds);
        let before = c.clouds;
        c.step(t + 30.0, north, 0.2);
        assert!((c.clouds - before).length() <= 0.05 + 1e-4, "a pause of 30 s counts as a quarter of one");
        assert!(c.sway < std::f64::consts::TAU && c.flutter < std::f64::consts::TAU && c.ripple < 1.0 && c.lap < 1.0);
    }

    /// A grass model in levels of `blades`, each blade a quad a cell's fiftieth wide.
    fn grass_levels(blades: &[usize]) -> Vec<crate::grass::GrassLevel> {
        blades
            .iter()
            .map(|&n| {
                let (mut v, mut i) = (vec![], vec![]);
                for k in 0..n {
                    let x = (k % 20) as f32 * 0.05 - 0.5;
                    let base = v.len() as u32;
                    for (dx, y) in [(0.0, 0.0), (0.02, 0.0), (0.02, 0.6), (0.0, 0.6)] {
                        v.push(MapVertex { pos: [x + dx, y, (k / 20) as f32 * 0.04 - 0.5], normal: [0.0, 0.0, 1.0], uv: [0.5, 0.5], color: [0.3, 0.6, 0.2, 1.0], toon: 1.0, wind: [(y / 0.6 * 255.0) as u8, 40, 90, 2] });
                    }
                    i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                vec![(WHITE.to_string(), v, i)]
            })
            .collect()
    }

    /// A field of grass copies, 96 x 96 cells a copy each, in two squares of a map placed 10
    /// cells east: gathered into the scene's space, culled and their levels picked on the GPU
    /// as on the CPU (`grass::pick`; a float's rounding may tip a copy or two over), drawn with
    /// no error from the GPU; and where there are no compute shaders, on the CPU, the same.
    #[test]
    fn grass_copies_cull_on_the_gpu_as_on_the_cpu() {
        let _turn = turn();
        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU here: the grass's culling not checked");
            return;
        };
        let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let levels = grass_levels(&[500, 250, 125, 62]);
        let (lo, hi) = (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.6, 0.5));
        r.set_grass_model(&device, "tuft", 1, &GrassModel { levels: &levels, cells: 1.0, lo, hi });
        // (one in three a meadow's: fewer further off, shorter at the furthest)
        let copies = |x0: i32| (x0..x0 + 48).flat_map(|x| (0..96).map(move |z| GrassCopy { at: [x as f32 + 0.5, 0.0, z as f32 + 0.5], turn: (x * 7 + z) as f32 * 0.3, scale: 1.0, far: if (x + z) % 3 == 0 { 1.0 } else { 0.0 }, ..Default::default() })).collect::<Vec<_>>();
        r.set_grass("field", (0, 0), vec![("tuft".into(), copies(0))]);
        r.set_grass("field", (1, 0), vec![("tuft".into(), copies(48))]);
        r.set_world_place("field", Place { offset: Vec3::new(10.0, 0.0, 0.0), ..Default::default() });
        let mut frame = frame_of(vec![]);
        let eye = Vec3::new(58.0, 18.0, 110.0);
        frame.view_proj = Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 600.0) * Mat4::look_at_rh(eye, Vec3::new(58.0, 0.0, 48.0), Vec3::Y);
        frame.camera_pos = eye;
        let size = [640, 400];
        let run = |r: &mut SceneRenderer| {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut encoder = device.create_command_encoder(&Default::default());
            r.prepare(&device, &queue, &mut encoder, &frame, size);
            queue.submit([encoder.finish()]);
            let error = block_on(scope.pop());
            assert!(error.is_none(), "{error:?}");
        };
        run(&mut r);
        let gathered = r.grass.copies("tuft");
        assert_eq!(gathered.len(), 96 * 96);
        assert!(gathered.iter().all(|c| c.at[0] >= 10.0), "in the scene's space: their map's placement added");
        // the CPU's picks of the same
        let cull = crate::grass::Cull::new(frame.view_proj, size);
        let shape = crate::grass::Shape::of(lo, hi, 1.0);
        let costs: Vec<f32> = levels.iter().map(|l| l[0].1.len() as f32).collect();
        let mut want = vec![0u32; 4];
        let mut lists: Vec<Vec<[i32; 2]>> = vec![vec![]; 4];
        for (i, c) in gathered.iter().enumerate() {
            if let Some((l, _)) = crate::grass::pick(&cull, &shape, &costs, c, i as u32) {
                want[l] += 1;
                lists[l].push([c.at[0] as i32, c.at[2] as i32]);
            }
        }
        // each level's copies themselves, as the draws read them (in whatever order)
        let cells = |list: &[GrassCopy]| {
            let mut out: Vec<[i32; 2]> = list.iter().map(|c| [c.at[0] as i32, c.at[2] as i32]).collect();
            out.sort();
            out
        };
        for l in &mut lists {
            l.sort();
        }
        let drawn: u32 = want.iter().sum();
        assert!(drawn > 1000 && drawn < 96 * 96 && want.iter().filter(|&&n| n > 0).count() >= 2, "some out of sight, the rest at more than one level: {want:?}");
        assert_eq!(r.grass.first_list("tuft"), Some((0, 4)));
        if r.grass_on_gpu() {
            let got = r.grass.counts_now(&device, &queue);
            let read = r.grass.lists_now(&device, &queue, "tuft");
            for l in 0..4 {
                let (g, w) = (got[l] as i64, want[l] as i64);
                assert!((g - w).abs() <= (w / 100).max(3), "level {l}: the GPU's {got:?}, the CPU's {want:?}");
                let same = cells(&read[l]).iter().filter(|c| lists[l].binary_search(c).is_ok()).count() as i64;
                assert!(same >= w - (w / 100).max(3), "level {l}: {same} of the GPU's {g} copies are the CPU's {w}");
            }
        }
        // on the CPU
        r.grass.on_cpu();
        run(&mut r);
        assert_eq!(r.grass.counts_now(&device, &queue), want);
        // pixelated, the same levels: the pixelation only changes how the picture's drawn
        let mut pixelated = frame.clone();
        pixelated.retro = Retro { pixelation: Some(0.3), zoom: 1.0, lcd: None };
        let mut encoder = device.create_command_encoder(&Default::default());
        r.prepare(&device, &queue, &mut encoder, &pixelated, size);
        queue.submit([encoder.finish()]);
        assert_eq!(r.grass.counts_now(&device, &queue), want, "pixelated, the copies at the levels they're at without it");
        let read = r.grass.lists_now(&device, &queue, "tuft");
        for l in 0..4 {
            assert_eq!(cells(&read[l]), lists[l], "level {l}'s copies as the draws read them");
        }
        assert_eq!(r.stats().grass, drawn);
        assert_eq!(r.stats().grass_vertices, want.iter().zip(&costs).map(|(&n, &v)| n as u64 * v as u64).sum::<u64>(), "each copy its level's vertices");
        assert_eq!(r.stats().grass_levels[..4], want[..], "the copies at each level");
        // the square gone, its copies with it
        r.set_grass("field", (1, 0), vec![]);
        run(&mut r);
        assert_eq!(r.grass.copies("tuft").len(), 48 * 96);
    }

    /// A meadow of blades and flowers, 96 x 96 cells, four to a cell (a flower in twenty), in
    /// two squares of a map placed 10 cells east: gathered into the scene's space, culled and
    /// their templates picked on the GPU as on the CPU (`blades::pick`; a float's rounding may
    /// tip a blade or two over), drawn with no error from the GPU; and where there are no
    /// compute shaders, each square drawn as a whole, at least as many of its blades as the
    /// GPU draws of it.
    #[test]
    fn blades_cull_on_the_gpu_as_on_the_cpu() {
        let _turn = turn();
        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU here: the blades' culling not checked");
            return;
        };
        let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let kinds = [
            BladeKind { form: [0.0, 0.3, 0.6, 0.035], bend: [0.3, 0.4, 1.0, 0.0], foot: [0.1, 0.3, 0.1, 5.0], tip: [0.4, 0.7, 0.3, 0.0], ..Default::default() },
            BladeKind { form: [1.0, 0.3, 0.5, 0.012], bend: [0.1, 0.2, 1.0, 0.1], foot: [0.1, 0.3, 0.1, 5.0], tip: [0.3, 0.5, 0.2, 0.0], petal: [0.9, 0.9, 0.9, 0.0], heart: [0.9, 0.7, 0.1, 0.0] },
        ];
        r.set_blade_kinds(&kinds);
        let blades = |x0: i32| {
            (x0..x0 + 48)
                .flat_map(|x| (0..96).flat_map(move |z| (0..4u32).map(move |k| (x, z, k))))
                .map(|(x, z, k)| {
                    let seed = crate::blades::mix_bits((x as u32) * 7919 + (z as u32) * 104_729 + k);
                    BladeInstance { at: [x as f32 + 0.25 + 0.5 * (k % 2) as f32, 0.0, z as f32 + 0.25 + 0.25 * k as f32], turn: k as f32, size: crate::blades::unit(seed >> 16), kind: u32::from(seed.is_multiple_of(20)), seed, ..Default::default() }
                })
                .collect::<Vec<_>>()
        };
        r.set_blades("field", (0, 0), blades(0));
        r.set_blades("field", (1, 0), blades(48));
        r.set_world_place("field", Place { offset: Vec3::new(10.0, 0.0, 0.0), ..Default::default() });
        let mut frame = frame_of(vec![]);
        let eye = Vec3::new(58.0, 14.0, 106.0);
        frame.view_proj = Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 600.0) * Mat4::look_at_rh(eye, Vec3::new(58.0, 0.0, 60.0), Vec3::Y);
        frame.camera_pos = eye;
        let size = [640, 400];
        let run = |r: &mut SceneRenderer| {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut encoder = device.create_command_encoder(&Default::default());
            r.prepare(&device, &queue, &mut encoder, &frame, size);
            queue.submit([encoder.finish()]);
            let error = block_on(scope.pop());
            assert!(error.is_none(), "{error:?}");
        };
        run(&mut r);
        let gathered = r.blades.gathered();
        assert_eq!(gathered.len(), 96 * 96 * 4);
        assert!(gathered.iter().all(|b| b.at[0] >= 10.0), "in the scene's space: their map's placement added");
        // the CPU's picks of the same
        let sight = crate::blades::Sight::new(frame.view_proj, size);
        let mut want = [0u32; 6];
        for b in &gathered {
            if let Some(l) = crate::blades::pick(&sight, &kinds, b) {
                want[l] += 1;
            }
        }
        let drawn: u32 = want.iter().sum();
        assert!(drawn > 2000 && drawn < 96 * 96 * 4 / 2, "some out of sight, fewer further off: {want:?}");
        assert!(want[..3].iter().filter(|&&n| n > 0).count() >= 2 && want[3] + want[4] + want[5] > 0, "blades with more than one template, and flowers: {want:?}");
        if r.blades_on_gpu() {
            let got = r.blades.counts_now(&device, &queue);
            for l in 0..6 {
                let (g, w) = (got[l] as i64, want[l] as i64);
                assert!((g - w).abs() <= (w / 100).max(3), "list {l}: the GPU's {got:?}, the CPU's {want:?}");
            }
        }
        // on the CPU: each square of each shape whole, as many as its nearest point needs
        r.blades.on_cpu();
        run(&mut r);
        let (n, vertices, templates) = r.blades.drawn();
        assert!(n >= drawn && n < 96 * 96 * 4, "{n} drawn as wholes for {drawn} culled one by one");
        assert_eq!(templates.iter().sum::<u32>(), n);
        assert!(vertices > n as u64 * 3);
        assert_eq!(r.stats().blades, n);
        // the square gone, its blades with it
        r.set_blades("field", (1, 0), vec![]);
        run(&mut r);
        assert_eq!(r.blades.gathered().len(), 48 * 96 * 4);
        assert_eq!(r.blades.kinds().len(), 2);
    }

    /// On OpenGL (as WebGL 2 is), the blades drawn as each patch's instances, the drawing
    /// leaving out what isn't drawn: no error from the GPU, blades and flowers drawn.
    #[test]
    fn blades_draw_on_opengl_as_instances() {
        let _turn = turn();
        let Some((device, queue)) = gl() else {
            eprintln!("no OpenGL here: the blades' WebGL 2 way not checked");
            return;
        };
        let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        r.blades.on_cpu();
        r.set_blade_kinds(&[
            BladeKind { form: [0.0, 0.3, 0.6, 0.035], bend: [0.3, 0.4, 1.0, 0.0], foot: [0.1, 0.3, 0.1, 5.0], tip: [0.4, 0.7, 0.3, 0.0], ..Default::default() },
            BladeKind { form: [1.0, 0.3, 0.5, 0.012], bend: [0.1, 0.2, 1.0, 0.1], foot: [0.1, 0.3, 0.1, 5.0], petal: [0.9, 0.9, 0.9, 0.0], heart: [0.9, 0.7, 0.1, 0.0], ..Default::default() },
        ]);
        let blades: Vec<BladeInstance> = (0..40 * 40 * 8u32)
            .map(|k| {
                let seed = crate::blades::mix_bits(k);
                BladeInstance { at: [(k % 40) as f32 + crate::blades::unit(seed >> 3), 0.0, (k / 40 % 40) as f32 + crate::blades::unit(seed >> 7)], turn: k as f32, size: 0.5, kind: u32::from(k.is_multiple_of(17)), seed, ..Default::default() }
            })
            .collect();
        r.set_blades("field", (0, 0), blades);
        let mut frame = frame_of(vec![]);
        let eye = Vec3::new(20.0, 10.0, 52.0);
        frame.view_proj = Mat4::perspective_rh(30f32.to_radians(), 1.6, 0.1, 400.0) * Mat4::look_at_rh(eye, Vec3::new(20.0, 0.0, 20.0), Vec3::Y);
        frame.camera_pos = eye;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = device.create_command_encoder(&Default::default());
        r.prepare(&device, &queue, &mut encoder, &frame, [640, 400]);
        queue.submit([encoder.finish()]);
        let error = block_on(scope.pop());
        assert!(error.is_none(), "{error:?}");
        let (n, _, templates) = r.blades.drawn();
        assert!(!r.blades_on_gpu() && n > 1000 && templates[3..].iter().sum::<u32>() > 0, "{n} drawn, by template {templates:?}");
    }

    /// The scene drawn and blitted into `size` pixels, read back (RGBA, the screen's values).
    fn pixels_of(r: &mut SceneRenderer, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame, size: u32) -> Vec<u8> {
        let out = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (size * size * 4) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = device.create_command_encoder(&Default::default());
        r.prepare(device, queue, &mut encoder, frame, [size, size]);
        {
            let view = out.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                ..Default::default()
            });
            r.paint(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            out.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: None } },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        let error = block_on(scope.pop());
        assert!(error.is_none(), "{error:?}");
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let rgba = read.slice(..).get_mapped_range().unwrap().to_vec();
        rgba
    }

    /// A 16 x 16 world map at night: a lava band along its west side (a terrain of its own
    /// look, glowing), grey ground elsewhere, a thin wall two cells high from the north edge
    /// down to z 9, three cells east of the lava. With the indirect light on, the ground
    /// beside the lava (east of it, south of the wall's end) is lit by it, the ground behind
    /// the wall is not (as dark as the far side of the map), and nothing is lit by it with
    /// the indirect light off; the lava shines whatever the light. On a device without
    /// compute shaders (WebGL 2's limits) the frame draws all the same, with no indirect light.
    #[test]
    fn a_glow_lights_the_cell_beside_it_and_not_the_one_behind_a_wall() {
        let _turn = turn();
        let devices: Vec<_> = [gpu(), webgl_limits()].into_iter().flatten().collect();
        if devices.is_empty() {
            eprintln!("no GPU here: the indirect light not checked");
        }
        for (device, queue) in devices {
            let mut r = SceneRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
            let (w, h) = (16i32, 16i32);
            // the cells, with the ring round the map: the lava (terrain 1) in the four
            // westmost columns, the grey (terrain 0) elsewhere, all on level 0
            let mut cells = vec![];
            for z in -1..=h {
                for x in -1..=w {
                    cells.extend_from_slice(&[u8::from((0..4).contains(&x) && (0..h).contains(&z)), 0, 255, 255]);
                }
            }
            // the palette: two columns of crate::world's PALETTE_ROWS (19) rows: colour,
            // second colour, [pattern, priority, water, beach], the cliffs' colour, then the
            // look of its own: [size, sharpness, balance, grain], [shine, glow / 4, pulse,
            // flow], [angle, layers, 0, 0], and the layers' rows (none)
            let rows = 19;
            let mut palette = vec![0u8; rows * 2 * 4];
            let mut put = |t: usize, row: usize, v: [u8; 4]| palette[(row * 2 + t) * 4..(row * 2 + t) * 4 + 4].copy_from_slice(&v);
            put(0, 0, [128, 128, 128, 255]);
            put(0, 1, [136, 136, 136, 255]);
            put(0, 2, [1, 128, 0, 0]);
            put(0, 3, [90, 90, 90, 255]);
            put(1, 0, [255, 90, 16, 255]);
            put(1, 1, [255, 176, 48, 255]);
            put(1, 2, [9, 129, 0, 0]);
            put(1, 3, [42, 34, 32, 255]);
            put(1, 4, [153, 102, 128, 26]);
            put(1, 5, [0, 140, 0, 0]);
            put(1, 6, [0, 0, 0, 0]);
            r.set_world_ground(&device, &queue, "lava", &WorldGround { cells: &cells, size: [(w + 2) as u32, (h + 2) as u32], palette: &palette, palette_width: 2, level_height: 1.0 });
            r.set_world_place("lava", Place::default());
            // the cell tops: a quad a cell, each vertex carrying its cell's middle in uv
            let (mut v, mut i) = (vec![], vec![]);
            for z in 0..h {
                for x in 0..w {
                    let base = v.len() as u32;
                    for (dx, dz) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                        v.push(MapVertex { pos: [(x + dx) as f32, 0.0, (z + dz) as f32], normal: [0.0, 1.0, 0.0], uv: [x as f32 + 0.5, z as f32 + 0.5], color: [1.0, 1.0, 1.0, 0.0], toon: 0.0, wind: [0; 4] });
                    }
                    i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            r.set_chunk(&device, "lava", "tops", (0, 0), &[(WHITE.to_string(), v, i)]);
            // the wall: a box x 6 to 6.3, z 0 to 9, two cells high (its faces inward and out)
            let (mut v, mut i) = (vec![], vec![]);
            let corner = |x: f32, y: f32, z: f32, n: [f32; 3]| MapVertex { pos: [x, y, z], normal: n, uv: [0.5, 0.5], color: [0.6, 0.6, 0.6, 1.0], toon: 0.0, wind: [0; 4] };
            let (x0, x1, z0, z1, top) = (6.0, 6.3, 0.0, 9.0, 2.0);
            let faces = [
                [corner(x0, 0.0, z0, [-1.0, 0.0, 0.0]), corner(x0, top, z0, [-1.0, 0.0, 0.0]), corner(x0, top, z1, [-1.0, 0.0, 0.0]), corner(x0, 0.0, z1, [-1.0, 0.0, 0.0])],
                [corner(x1, 0.0, z0, [1.0, 0.0, 0.0]), corner(x1, top, z0, [1.0, 0.0, 0.0]), corner(x1, top, z1, [1.0, 0.0, 0.0]), corner(x1, 0.0, z1, [1.0, 0.0, 0.0])],
                [corner(x0, top, z0, [0.0, 1.0, 0.0]), corner(x1, top, z0, [0.0, 1.0, 0.0]), corner(x1, top, z1, [0.0, 1.0, 0.0]), corner(x0, top, z1, [0.0, 1.0, 0.0])],
                [corner(x0, 0.0, z1, [0.0, 0.0, 1.0]), corner(x1, 0.0, z1, [0.0, 0.0, 1.0]), corner(x1, top, z1, [0.0, 0.0, 1.0]), corner(x0, top, z1, [0.0, 0.0, 1.0])],
            ];
            for f in faces {
                let base = v.len() as u32;
                v.extend(f);
                i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            r.set_chunk(&device, "lava", "buildings", (0, 0), &[(WHITE.to_string(), v, i)]);
            // night: no sun, no sky, no lamps; the camera straight above the map's middle
            let eye = Vec3::new(8.0, 30.0, 8.001);
            let view_proj = Mat4::perspective_rh(0.62, 1.0, 0.1, 100.0) * Mat4::look_at_rh(eye, Vec3::new(8.0, 0.0, 8.0), Vec3::Y);
            let mut frame = frame_of(vec![]);
            frame.view_proj = view_proj;
            frame.camera_pos = eye;
            frame.shadows = false;
            frame.light.sun = Vec3::ZERO;
            frame.light.ambient = Vec3::ZERO;
            frame.light.hemi_sky = Vec3::ZERO;
            frame.light.hemi_ground = Vec3::ZERO;
            frame.light.sky = Vec3::ZERO;
            frame.light.fog_near = 500.0;
            frame.light.fog_far = 1000.0;
            frame.toon.cloud_shadow = 0.0;
            frame.radiance = Radiance { bloom: 0.0, gi: 1.0, gi_range: 32.0, ..Radiance::default() };
            // (a row of pixels copies out in whole multiples of 256 bytes)
            let size = 192u32;
            let compute = r.indirect_on_gpu();
            // (twice: the field also reads what the picture last showed shining)
            pixels_of(&mut r, &device, &queue, &frame, size);
            let lit = pixels_of(&mut r, &device, &queue, &frame, size);
            frame.radiance.gi = 0.0;
            let unlit = pixels_of(&mut r, &device, &queue, &frame, size);
            // the brightness round a point of the ground, linear (the screen's values undone),
            // out of 255
            let bright = |rgba: &[u8], x: f32, z: f32| {
                let c = view_proj * Vec4::new(x, 0.0, z, 1.0);
                let (px, py) = (((c.x / c.w) * 0.5 + 0.5) * size as f32, (0.5 - (c.y / c.w) * 0.5) * size as f32);
                let linear = |v: u8| (v as f32 / 255.0).powf(2.2) * 255.0;
                let mut sum = 0.0;
                for dy in -2..=2i32 {
                    for dx in -2..=2i32 {
                        let i = ((py as i32 + dy) as usize * size as usize + (px as i32 + dx) as usize) * 4;
                        sum += linear(rgba[i]) * 0.2126 + linear(rgba[i + 1]) * 0.7152 + linear(rgba[i + 2]) * 0.0722;
                    }
                }
                sum / 25.0
            };
            let beside = bright(&lit, 7.5, 13.5);
            let behind = bright(&lit, 7.5, 4.5);
            let far = bright(&lit, 15.5, 13.5);
            let lava = bright(&lit, 2.0, 8.0);
            let lava_unlit = bright(&unlit, 2.0, 8.0);
            eprintln!("compute {compute}: beside the lava {beside:.1}, behind the wall {behind:.1}, far {far:.1}, the lava {lava:.1} ({lava_unlit:.1} with the indirect light off), beside with it off {:.1}", bright(&unlit, 7.5, 13.5));
            assert!(lava > 60.0 && lava_unlit > 60.0, "the lava shines of itself: {lava:.1}, {lava_unlit:.1}");
            if compute {
                assert!(beside > 5.0, "the ground beside the lava is lit by it: {beside:.1}");
                assert!(beside > behind * 4.0, "behind the wall it isn't: {behind:.1} against {beside:.1}");
                assert!(behind < far + 4.0, "behind the wall about as dark as far off: {behind:.1}, {far:.1}");
                assert!(bright(&unlit, 7.5, 13.5) < beside / 4.0, "nothing of it with the indirect light off: {:.1}", bright(&unlit, 7.5, 13.5));
            } else {
                assert!(beside < 2.0 && behind < 2.0, "no indirect light without compute shaders: {beside:.1}, {behind:.1}");
            }
        }
    }

    #[test]
    fn the_shader_holds_for_webgpu_and_webgl() {
        let module = naga::front::wgsl::parse_str(SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        assert!(["vs_blade", "vs_blade_shadow", "vs_blade_listed", "vs_blade_listed_shadow", "fs_blade", "fs_blade_shadow", "vs_skinned_uniform", "vs_skinned_uniform_shadow"].iter().all(|n| module.entry_points.iter().any(|e| e.name == *n)));
        // every entry point as WebGL 2 gets it, but the four reading a storage buffer, which it
        // hasn't got: the skinned models' (there their joints are in a uniform buffer, as many
        // as the scene binds) and the blades drawn from the culling's lists (there as instances)
        let stored = |i: usize| module.global_variables.iter().any(|(h, v)| matches!(v.space, naga::AddressSpace::Storage { .. }) && !info.get_entry_point(i)[h].is_empty());
        let mut webgpu_only: Vec<&str> = module.entry_points.iter().enumerate().filter(|&(i, _)| stored(i)).map(|(_, e)| e.name.as_str()).collect();
        webgpu_only.sort_unstable();
        assert_eq!(webgpu_only, ["vs_blade_listed", "vs_blade_listed_shadow", "vs_skinned", "vs_skinned_shadow"]);
        let (_, uniform) = module.global_variables.iter().find(|(_, v)| v.name.as_deref() == Some("bones_uniform")).unwrap();
        assert_eq!(u64::from(module.types[uniform.ty].inner.size(module.to_ctx())), BONES_WINDOW as u64 * MAT4_BYTES);
        // (and nothing of the indirect light's compute shaders, which WebGL 2 never runs: a
        // storage texture anywhere in a module keeps its GLSL from reading any of it)
        assert!(module.entry_points.iter().all(|e| e.stage != naga::ShaderStage::Compute));
        let options = naga::back::glsl::Options { version: naga::back::glsl::Version::Embedded { version: 300, is_webgl: true }, ..Default::default() };
        for (_, ep) in module.entry_points.iter().enumerate().filter(|&(i, _)| !stored(i)) {
            let pipeline = naga::back::glsl::PipelineOptions { shader_stage: ep.stage, entry_point: ep.name.clone(), multiview: None };
            let mut out = String::new();
            let mut w = naga::back::glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default()).unwrap_or_else(|e| panic!("{}: {e}", ep.name));
            w.write().unwrap_or_else(|e| panic!("{}: {e}", ep.name));
        }
    }

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
