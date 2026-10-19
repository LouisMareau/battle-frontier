//! A map's geometry, for the renderer: the cells' tops and the cliff faces (coloured by the
//! terrain shader from a texture of the cells, so painting only changes that texture; the
//! tops bevelled where the ground steps down, the faces and bevels rock), the tall grass
//! (vertex colours), and the props and trees (their stand-in models, made from the simple
//! shapes their records list).

use std::collections::{BTreeMap, HashMap};
use std::f32::consts::TAU;

use content::{Content, Part, Prop};
use glam::{Mat3, Mat4, Vec3};

use crate::trees::{trees, Tree};
use crate::{color, ramps, Rng, WorldMap};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// on cell tops: the cell's centre (x, z), so the shader knows which cell it's on
    pub uv: [f32; 2],
    /// linear
    pub color: [f32; 4],
    /// 1: drawn with the toon look (its prop's or creature's `toon`)
    pub toon: f32,
    /// how it moves in the wind (`WIND`): r sway, g flutter, b phase, a height above its
    /// thing's foot (0 to `WIND_REACH` cells); all 0: it doesn't
    pub wind: [u8; 4],
}

/// The wind's weights as painted (a model's "wind" colour layer, the 0 to 1 a painter sees):
/// R, how far a point sways with the wind (0 rooted: a trunk's or a stem's foot; 1 free: the
/// tips, a crown, hair's ends); G, how much it flutters (leaves, petals, fur's ends, a
/// cloth's edge); B, its phase (different values on different leaves or tufts, so they
/// don't all move together). The game sets the fourth itself: how high above its thing's
/// foot the point is (taller things bend further).
pub const WIND: [&str; 3] = ["sway", "flutter", "phase"];

/// The height the wind's fourth weight reaches, in cells (higher counts as this).
pub const WIND_REACH: f32 = 8.0;

/// A vertex's wind: its painted weights (0 to 1) and its height above its thing's foot.
pub fn wind_of(sway: f32, flutter: f32, phase: f32, height: f32) -> [u8; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(sway), byte(flutter), byte(phase), byte(height / WIND_REACH)]
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn vertex(&mut self, pos: Vec3, normal: Vec3, color: [f32; 4]) -> u32 {
        self.vertices.push(Vertex { pos: pos.to_array(), normal: normal.to_array(), uv: [0.5, 0.5], color, toon: 0.0, wind: [0; 4] });
        self.vertices.len() as u32 - 1
    }

    /// A flat quad, corners counter-clockwise seen from the front.
    pub(crate) fn quad(&mut self, c: [Vec3; 4], color: [f32; 4]) {
        let n = (c[1] - c[0]).cross(c[2] - c[0]).normalize_or(Vec3::Y);
        let i = self.vertices.len() as u32;
        for p in c {
            self.vertex(p, n, color);
        }
        self.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }

    pub(crate) fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let i = self.vertices.len() as u32;
        for p in [a, b, c] {
            self.vertex(p, n, color);
        }
        self.indices.extend_from_slice(&[i, i + 1, i + 2]);
    }

    /// Adds another mesh moved by `m` (rotation, scale, translation); what moves in the wind
    /// keeps its height above its foot, scaled too.
    pub fn append(&mut self, other: &Mesh, m: Mat4) {
        self.append_put(other, &Put::plain(m));
    }

    /// Adds another mesh put down as `put` says: stretched along x (what lies along a ramp),
    /// then moved; its normals turned and leaning as its faces do.
    pub fn append_put(&mut self, other: &Mesh, put: &Put) {
        let base = self.vertices.len() as u32;
        let normals = Mat3::from_mat4(put.m).inverse().transpose();
        let up = put.m.y_axis.truncate().length();
        for v in &other.vertices {
            let p = Vec3::from(v.pos);
            let (x, s) = match put.along {
                Some(a) => (a.x(p.x), a.stretch(p.x)),
                None => (p.x, 1.0),
            };
            let pos = put.m.transform_point3(Vec3::new(x, p.y, p.z));
            let n = Vec3::from(v.normal);
            let normal = (normals * Vec3::new(n.x / s, n.y, n.z)).normalize_or(Vec3::Y);
            let mut wind = v.wind;
            if wind != [0; 4] && (up - 1.0).abs() > 1e-4 {
                wind[3] = (wind[3] as f32 * up).round().min(255.0) as u8;
            }
            self.vertices.push(Vertex { pos: pos.to_array(), normal: normal.to_array(), wind, ..*v });
        }
        // put down mirrored (a floor tile laid flipped), its faces would be wound inside out:
        // each triangle wound back, so it faces the way it did
        if Mat3::from_mat4(put.m).determinant() < 0.0 {
            for [a, b, c] in other.indices.as_chunks::<3>().0 {
                self.indices.extend([a + base, c + base, b + base]);
            }
        } else {
            self.indices.extend(other.indices.iter().map(|i| i + base));
        }
    }
}

/// A model's x stretched from half its width as made (`half`) to half the width it's put
/// down at (`to`), its `ends` either side keeping their size: what lies along a ramp. Where
/// there's no middle left to stretch, all of it is, ends and all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Along {
    pub half: f32,
    pub to: f32,
    pub ends: f32,
}

impl Along {
    /// The ends kept: none when they'd leave no middle.
    fn kept(&self) -> f32 {
        let e = self.ends.clamp(0.0, self.half);
        if self.half - e < 1e-4 || self.to - e < 1e-4 { 0.0 } else { e }
    }

    /// Where a point of the model at `x` goes along x.
    pub fn x(&self, x: f32) -> f32 {
        let e = self.kept();
        let (from, to) = ((self.half - e).max(1e-6), self.to - e);
        if x.abs() <= from { x * to / from } else { x.signum() * (to + x.abs() - from) }
    }

    /// How much the model is stretched along x at `x`.
    pub fn stretch(&self, x: f32) -> f32 {
        let e = self.kept();
        let from = (self.half - e).max(1e-6);
        if x.abs() <= from + 1e-5 { (self.to - e) / from } else { 1.0 }
    }
}

/// Where a placed thing's model goes: stretched along x first (`along`: what lies along a
/// ramp), then moved by `m` (turned, scaled, put at its foot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Put {
    pub m: Mat4,
    pub along: Option<Along>,
}

impl Put {
    pub fn plain(m: Mat4) -> Put {
        Put { m, along: None }
    }

    /// A point of the model, put down.
    pub fn point(&self, p: Vec3) -> Vec3 {
        let x = self.along.map(|a| a.x(p.x)).unwrap_or(p.x);
        self.m.transform_point3(Vec3::new(x, p.y, p.z))
    }

    /// A way in the model (a light's aim), put down: stretched as its middle is.
    pub fn way(&self, v: Vec3) -> Vec3 {
        let s = self.along.map(|a| a.stretch(0.0)).unwrap_or(1.0);
        self.m.transform_vector3(Vec3::new(v.x * s, v.y, v.z))
    }

    /// All of it as one matrix, the stretch even along it (what a rig is posed by).
    pub fn matrix(&self) -> Mat4 {
        let s = self.along.map(|a| a.to / a.half.max(1e-6)).unwrap_or(1.0);
        self.m * Mat4::from_scale(Vec3::new(s, 1.0, 1.0))
    }
}

fn shade(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// Darker at the bottom of a part, as if the ground kept the light off.
fn ao(c: [f32; 4], y: f32, h: f32) -> [f32; 4] {
    shade(c, 0.72 + 0.28 * (y / h.max(0.01)).clamp(0.0, 1.0))
}

// ---------------------------------------------------------------- shapes

fn shape_box(m: &mut Mesh, s: Vec3, c: [f32; 4]) {
    let (x, y, z) = (s.x / 2.0, s.y, s.z / 2.0);
    let p = |px: f32, py: f32, pz: f32| Vec3::new(px, py, pz);
    let faces = [
        [p(-x, y, z), p(-x, 0.0, z), p(x, 0.0, z), p(x, y, z)],     // south
        [p(x, y, -z), p(x, 0.0, -z), p(-x, 0.0, -z), p(-x, y, -z)], // north
        [p(x, y, z), p(x, 0.0, z), p(x, 0.0, -z), p(x, y, -z)],     // east
        [p(-x, y, -z), p(-x, 0.0, -z), p(-x, 0.0, z), p(-x, y, z)], // west
        [p(-x, y, -z), p(-x, y, z), p(x, y, z), p(x, y, -z)],       // top
    ];
    for f in faces {
        let i = m.vertices.len() as u32;
        let n = (f[1] - f[0]).cross(f[2] - f[0]).normalize_or(Vec3::Y);
        for q in f {
            m.vertex(q, n, ao(c, q.y, y));
        }
        m.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
}

/// A wedge: a box whose top slopes from its full height along its back (-z) down to nothing
/// along its front (+z); no bottom face, as a box has none.
fn shape_wedge(m: &mut Mesh, s: Vec3, c: [f32; 4]) {
    let (x, y, z) = (s.x / 2.0, s.y, s.z / 2.0);
    let p = |px: f32, py: f32, pz: f32| Vec3::new(px, py, pz);
    let faces: [&[Vec3]; 4] = [
        &[p(-x, y, -z), p(-x, 0.0, z), p(x, 0.0, z), p(x, y, -z)],   // the slope
        &[p(x, y, -z), p(x, 0.0, -z), p(-x, 0.0, -z), p(-x, y, -z)], // north
        &[p(x, 0.0, z), p(x, 0.0, -z), p(x, y, -z)],                 // east
        &[p(-x, y, -z), p(-x, 0.0, -z), p(-x, 0.0, z)],              // west
    ];
    for f in faces {
        let i = m.vertices.len() as u32;
        let n = (f[1] - f[0]).cross(f[2] - f[0]).normalize_or(Vec3::Y);
        for &q in f {
            m.vertex(q, n, ao(c, q.y, y));
        }
        m.indices.extend_from_slice(&[i, i + 1, i + 2]);
        if f.len() == 4 {
            m.indices.extend_from_slice(&[i, i + 2, i + 3]);
        }
    }
}

/// A cylinder, or a cone when `top` is 0 (the top's radius, as a share of the bottom's).
fn shape_round(m: &mut Mesh, s: Vec3, c: [f32; 4], top: f32, segments: usize) {
    let (rx, rz, h) = (s.x / 2.0, s.z / 2.0, s.y);
    let ring = |k: usize, y: f32, r: f32| {
        let a = k as f32 / segments as f32 * TAU;
        Vec3::new(a.cos() * rx * r, y, a.sin() * rz * r)
    };
    let slope = (rx.max(rz) * (1.0 - top)) / h.max(0.01);
    for k in 0..segments {
        let (a0, a1) = (k as f32 / segments as f32 * TAU, (k + 1) as f32 / segments as f32 * TAU);
        let n0 = Vec3::new(a0.cos(), slope, a0.sin()).normalize();
        let n1 = Vec3::new(a1.cos(), slope, a1.sin()).normalize();
        let (b0, b1, t0, t1) = (ring(k, 0.0, 1.0), ring(k + 1, 0.0, 1.0), ring(k, h, top), ring(k + 1, h, top));
        let i = m.vertices.len() as u32;
        m.vertex(t0, n0, ao(c, h, h));
        m.vertex(b0, n0, ao(c, 0.0, h));
        m.vertex(b1, n1, ao(c, 0.0, h));
        m.vertex(t1, n1, ao(c, h, h));
        // counter-clockwise from outside: t0, t1, b1 / t0, b1, b0 (the ring goes towards +z)
        m.indices.extend_from_slice(&[i, i + 3, i + 2, i, i + 2, i + 1]);
        if top > 0.0 {
            let centre = Vec3::new(0.0, h, 0.0);
            m.tri(centre, t1, t0, c);
        }
    }
}

/// An ellipsoid standing on its base.
fn shape_sphere(m: &mut Mesh, s: Vec3, c: [f32; 4]) {
    let (rings, segments) = (6, 10);
    let r = s * 0.5;
    let point = |i: usize, k: usize| {
        let v = i as f32 / rings as f32 * std::f32::consts::PI;
        let a = k as f32 / segments as f32 * TAU;
        let unit = Vec3::new(v.sin() * a.cos(), -v.cos(), v.sin() * a.sin());
        (Vec3::new(unit.x * r.x, r.y + unit.y * r.y, unit.z * r.z), unit)
    };
    let base = m.vertices.len() as u32;
    for i in 0..=rings {
        for k in 0..=segments {
            let (p, n) = point(i, k);
            let n = Vec3::new(n.x / r.x.max(0.01), n.y / r.y.max(0.01), n.z / r.z.max(0.01)).normalize_or(Vec3::Y);
            m.vertex(p, n, ao(c, p.y, s.y));
        }
    }
    let row = segments as u32 + 1;
    for i in 0..rings as u32 {
        for k in 0..segments as u32 {
            let (a, b, cc, d) = (base + i * row + k, base + (i + 1) * row + k, base + (i + 1) * row + k + 1, base + i * row + k + 1);
            m.indices.extend_from_slice(&[a, b, cc, a, cc, d]);
        }
    }
}

/// A faceted rock: a jittered icosahedron, flat at the bottom.
fn shape_rock(m: &mut Mesh, s: Vec3, c: [f32; 4], seed: u32) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut v: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let faces = [
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    let mut rng = Rng::new(seed);
    for p in v.iter_mut() {
        *p *= rng.range(0.82, 1.08);
        p.y = p.y.max(-0.35);
    }
    let r = s * 0.5;
    let place = |p: Vec3| Vec3::new(p.x * r.x, (p.y + 0.35) / 1.35 * s.y, p.z * r.z);
    for f in faces {
        let (a, b, cc) = (place(v[f[0]]), place(v[f[1]]), place(v[f[2]]));
        let k = 0.9 + 0.1 * rng.next();
        let y = (a.y + b.y + cc.y) / 3.0;
        m.tri(a, b, cc, ao(shade(c, k), y, s.y));
    }
}

pub(crate) fn part_mesh(p: &Part, seed: u32) -> Mesh {
    let mut m = Mesh::default();
    let s = Vec3::new(p.size[0] as f32, p.size[1] as f32, p.size[2] as f32);
    let c = color(&p.color);
    match p.shape.as_str() {
        "box" => shape_box(&mut m, s, c),
        "cylinder" => shape_round(&mut m, s, c, 1.0, 10),
        "cone" => shape_round(&mut m, s, c, 0.0, 10),
        "sphere" => shape_sphere(&mut m, s, c),
        "rock" => shape_rock(&mut m, s, c, seed),
        "wedge" => shape_wedge(&mut m, s, c),
        _ => shape_box(&mut m, s, [1.0, 0.0, 1.0, 1.0]),
    }
    m
}

/// Geometry by texture: `plain` is coloured by its vertices alone (drawn on white), each
/// `textured` mesh by its texture too (the key a model's images are known by).
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub plain: Mesh,
    pub textured: BTreeMap<String, Mesh>,
}

impl Scene {
    /// Adds another scene moved by `m`.
    pub fn append(&mut self, other: &Scene, m: Mat4) {
        self.append_put(other, &Put::plain(m));
    }

    /// Adds another scene put down as `put` says (stretched along a ramp, say).
    pub fn append_put(&mut self, other: &Scene, put: &Put) {
        self.plain.append_put(&other.plain, put);
        for (k, mesh) in &other.textured {
            self.textured.entry(k.clone()).or_default().append_put(mesh, put);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.plain.indices.is_empty() && self.textured.values().all(|m| m.indices.is_empty())
    }

    /// Drawn with the toon look, all of it.
    pub fn make_toon(&mut self) {
        for v in self.plain.vertices.iter_mut().chain(self.textured.values_mut().flat_map(|m| m.vertices.iter_mut())) {
            v.toon = 1.0;
        }
    }
}

impl From<Mesh> for Scene {
    fn from(plain: Mesh) -> Scene {
        Scene { plain, textured: BTreeMap::new() }
    }
}

/// Props' real models, loaded from their files, by prop id.
pub type Models = HashMap<String, crate::gltf_in::Model>;

/// What a prop looks like: its real model when it has one loaded, else its stand-in;
/// toon if it is.
pub fn prop_scene(prop: &Prop, models: &Models) -> Scene {
    let mut s: Scene = match models.get(&prop.id) {
        Some(m) => m.scene.clone(),
        None => prop_model(prop).into(),
    };
    if prop.toon {
        s.make_toon();
    }
    s
}

/// How far a placed prop may stand from its cells' centre, along x and z: as far as its
/// model (turned and scaled) leaves room within its footprint's cells, and always as far as
/// where it was put (0). ([least x, least z], [most x, most z]); none when it fills them.
pub fn offset_room(prop: &Prop, models: &Models, turn: f64, scale: f64) -> ([f64; 2], [f64; 2]) {
    let scene: Scene = match models.get(&prop.id) {
        Some(m) => m.rest_scene(),
        None => prop_model(prop).into(),
    };
    let m = Mat4::from_rotation_y(-(turn as f32).to_radians()) * Mat4::from_scale(Vec3::splat(scale as f32));
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in scene.plain.vertices.iter().chain(scene.textured.values().flat_map(|t| t.vertices.iter())) {
        let p = m.transform_point3(Vec3::from(v.pos));
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if lo.x > hi.x {
        return ([0.0; 2], [0.0; 2]);
    }
    let quarter = ((turn / 90.0).round() as i64).rem_euclid(2) == 1;
    let fp = prop.footprint;
    let (w, d) = if quarter { (fp[1].max(1), fp[0].max(1)) } else { (fp[0].max(1), fp[1].max(1)) };
    let room = |half: f64, a: f32, b: f32| {
        let (least, most) = (-half - a as f64, half - b as f64);
        if least > most { (0.0, 0.0) } else { (least.min(0.0), most.max(0.0)) }
    };
    let (xl, xh) = room(w as f64 / 2.0, lo.x, hi.x);
    let (zl, zh) = room(d as f64 / 2.0, lo.z, hi.z);
    ([xl, zl], [xh, zh])
}

/// A prop's stand-in model, its footprint's centre at the origin, standing on y = 0; moving
/// in the wind as its record says (`breeze`).
pub fn prop_model(prop: &Prop) -> Mesh {
    let mut out = Mesh::default();
    for (i, p) in prop.parts.iter().enumerate() {
        let mut m = part_mesh(p, 7 + i as u32 * 31 + prop.id.len() as u32);
        let at = Vec3::new(p.at[0] as f32, p.at[1] as f32, p.at[2] as f32);
        breeze(prop, i, &mut m, at.y);
        out.append(&m, Mat4::from_translation(at) * Mat4::from_rotation_y(-(p.turn as f32).to_radians()));
    }
    out
}

/// A stand-in's part (`part`: its place in the prop's parts, standing `foot` high) moving in
/// the wind as its prop's record says (`Prop::wind`): swaying more the higher up, rooted at
/// the prop's foot, fluttering, in the part's own time. Nothing when the record asks for no
/// wind.
pub fn breeze(prop: &Prop, part: usize, mesh: &mut Mesh, foot: f32) {
    if prop.wind.still() {
        return;
    }
    let top = (prop.height as f32).max(0.05);
    let phase = hash(part as i32, prop.id.len() as i32, 5);
    for v in mesh.vertices.iter_mut() {
        let y = (v.pos[1] + foot).max(0.0);
        let up = (y / top).clamp(0.0, 1.0);
        v.wind = wind_of(prop.wind.sway as f32 * up.powf(1.4), prop.wind.flutter as f32 * up, phase, y);
    }
}

/// Where a placed thing stands: at its foot, turned clockwise (seen from above), scaled.
pub fn placement(at: Vec3, turn_degrees: f32, scale: f32) -> Mat4 {
    Mat4::from_translation(at) * Mat4::from_rotation_y(-turn_degrees.to_radians()) * Mat4::from_scale(Vec3::splat(scale))
}

/// Where a floor tile lies on its cell: `at` its middle, mirrored first when `flip` (its east
/// and west swapped, as `FloorTile::edge_as` has its edges), then turned `turn` quarter turns
/// clockwise.
pub fn tile_placement(at: Vec3, turn: u8, flip: bool) -> Mat4 {
    let m = placement(at, turn as f32 * 90.0, 1.0);
    if flip {
        m * Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))
    } else {
        m
    }
}

// ---------------------------------------------------------------- the ground

/// How each terrain pattern is drawn by the shader.
pub fn pattern_id(pattern: &str) -> u8 {
    match pattern {
        "grass" => 1,
        "meadow" => 2,
        "tall" => 3,
        "dirt" => 4,
        "sand" => 5,
        "stones" => 6,
        "rock" => 7,
        "water" => 8,
        _ => 0,
    }
}

pub struct Ground {
    /// cell tops, drawn by the terrain shader, bevelled where the ground steps down
    pub tops: Mesh,
    /// cliff faces, drawn by the terrain shader too, as rock
    pub cliffs: Mesh,
    /// tall grass (vertex colours)
    pub grass: Mesh,
    /// see `cells_texture`
    pub cells: Vec<u8>,
    /// see `palette`
    pub palette: Vec<u8>,
    pub palette_width: u32,
}

/// A cliff side's colour when its terrain gives none.
const SIDE: &str = "#8a6a48";

/// The colour of the cliffs under a terrain (linear).
pub fn side_color(t: Option<&content::Terrain>) -> [f32; 4] {
    color(t.map(|t| t.side.as_str()).filter(|s| !s.is_empty()).unwrap_or(SIDE))
}

fn hex_bytes(hex: &str) -> [u8; 4] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0xff00ff);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
}

pub(crate) fn hash(x: i32, z: i32, k: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ (k as u32).wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// The cells past a map's edge, from the maps next to it in the world, in the map's own
/// coordinates: a cell's level, and its terrain (its index in the content's terrains, as
/// the palette has them); none where no map is.
pub trait Beyond {
    fn level(&self, x: i32, z: i32) -> Option<u8>;
    fn terrain(&self, x: i32, z: i32) -> Option<u8>;
}

/// No map past the edge: the map stands alone.
pub struct Alone;

impl Beyond for Alone {
    fn level(&self, _: i32, _: i32) -> Option<u8> {
        None
    }

    fn terrain(&self, _: i32, _: i32) -> Option<u8> {
        None
    }
}

/// A terrain's column in the palette: its place among the content's terrains (one past
/// them, magenta, for a terrain the content hasn't got).
pub fn terrain_index(content: &Content, id: &str) -> u8 {
    content.terrains.iter().position(|t| t.id == id).unwrap_or(content.terrains.iter().count()).min(255) as u8
}

/// The cells' texture (what changes when terrain is painted): (width + 2) x (height + 2)
/// texels, a ring of the cells around the map first (the maps' next to it, else its own
/// edge's again): the terrain's column in the palette, the level, 0, 255.
pub fn cells_texture(map: &WorldMap, content: &Content, beyond: &dyn Beyond) -> Vec<u8> {
    let columns: Vec<u8> = map.terrains.iter().map(|id| terrain_index(content, id)).collect();
    let (w, h) = (map.width as i32, map.height as i32);
    let mut cells = Vec::with_capacity(((w + 2) * (h + 2) * 4) as usize);
    for z in -1..=h {
        for x in -1..=w {
            let (t, l) = texel(map, &columns, beyond, x, z);
            cells.extend_from_slice(&[t, l, 0, 255]);
        }
    }
    cells
}

/// A texel of the cells' texture: the terrain's column in the palette (`columns`, by the
/// map's terrain) and the level of the cell there, on the ring the next map's (else its own
/// edge's again).
fn texel(map: &WorldMap, columns: &[u8], beyond: &dyn Beyond, x: i32, z: i32) -> (u8, u8) {
    let (w, h) = (map.width as i32, map.height as i32);
    let own = |x: i32, z: i32| {
        let i = map.index(x.clamp(0, w - 1), z.clamp(0, h - 1)).unwrap_or(0);
        (columns.get(map.terrain[i] as usize).copied().unwrap_or(255), map.level[i])
    };
    match map.index(x, z) {
        Some(_) => own(x, z),
        None => match (beyond.terrain(x, z), beyond.level(x, z)) {
            (Some(t), Some(l)) => (t, l),
            _ => own(x, z),
        },
    }
}

/// Some of the cells' texture's texels written again (x from -1 to the width, z from -1 to
/// the height: the ring too), as `cells_texture` has them; what marks the focus left as it
/// was.
pub fn cells_texels(map: &WorldMap, content: &Content, beyond: &dyn Beyond, cells: &mut [u8], at: impl IntoIterator<Item = (i32, i32)>) {
    let columns: Vec<u8> = map.terrains.iter().map(|id| terrain_index(content, id)).collect();
    let (w, h) = (map.width as i32, map.height as i32);
    for (x, z) in at.into_iter().filter(|&(x, z)| x >= -1 && z >= -1 && x <= w && z <= h) {
        let i = (((z + 1) * (w + 2) + x + 1) * 4) as usize;
        if let Some(t) = cells.get_mut(i..i + 2) {
            let (c, l) = texel(map, &columns, beyond, x, z);
            t.copy_from_slice(&[c, l]);
        }
    }
}

/// Where a map cell is in `cells_texture`'s texels.
pub fn cell_texel(map: &WorldMap, x: i32, z: i32) -> Option<usize> {
    map.index(x, z).map(|_| ((z + 1) * (map.width as i32 + 2) + x + 1) as usize)
}

/// Every terrain of the content, a column each, then one for a terrain it hasn't got
/// (magenta), in 4 rows: colour, second colour (sRGB), [pattern, priority + 128, water (0
/// none, 1 inland, 2 oceanic: `water_code`), beach (255) or not (0)], the cliff side's colour.
pub fn palette(content: &Content) -> (Vec<u8>, u32) {
    let mut rows: [Vec<u8>; 4] = [vec![], vec![], vec![], vec![]];
    let all: Vec<Option<&content::Terrain>> = content.terrains.iter().map(Some).chain(std::iter::once(None)).collect();
    for t in &all {
        let (c1, c2, params, side) = match t {
            Some(t) => (
                hex_bytes(&t.color),
                hex_bytes(if t.color2.is_empty() { &t.color } else { &t.color2 }),
                [pattern_id(&t.pattern), (t.priority.clamp(-128, 127) + 128) as u8, water_code(t), if t.beach { 255 } else { 0 }],
                hex_bytes(if t.side.is_empty() { SIDE } else { &t.side }),
            ),
            None => ([255, 0, 255, 255], [128, 0, 128, 255], [0, 128, 0, 0], hex_bytes(SIDE)),
        };
        rows[0].extend_from_slice(&c1);
        rows[1].extend_from_slice(&c2);
        rows[2].extend_from_slice(&params);
        rows[3].extend_from_slice(&side);
    }
    (rows.concat(), all.len() as u32)
}

/// The kind of water a terrain is, as the palette (and the renderer's shore field) has it:
/// 0 none, 1 inland, 2 oceanic.
pub fn water_code(t: &content::Terrain) -> u8 {
    match t.water_kind() {
        Some("oceanic") => 2,
        Some(_) => 1,
        None => 0,
    }
}

pub fn ground(map: &WorldMap, content: &Content, level_height: f32) -> Ground {
    let (tops, cliffs, grass) = ground_region(map, content, level_height, whole(map), &Alone);
    let (palette, palette_width) = palette(content);
    Ground { tops, cliffs, grass, cells: cells_texture(map, content, &Alone), palette, palette_width }
}

/// A region holding the whole map: x0, z0 (in) to x1, z1 (out).
pub fn whole(map: &WorldMap) -> [i32; 4] {
    [0, 0, map.width as i32, map.height as i32]
}

/// The cells of a region, clipped to the map.
pub fn region_cells(map: &WorldMap, region: [i32; 4]) -> impl Iterator<Item = (i32, i32)> {
    let (x0, z0) = (region[0].max(0), region[1].max(0));
    let (x1, z1) = (region[2].min(map.width as i32), region[3].min(map.height as i32));
    (z0..z1).flat_map(move |z| (x0..x1).map(move |x| (x, z)))
}

fn in_region(region: [i32; 4], x: i32, z: i32) -> bool {
    x >= region[0] && z >= region[1] && x < region[2] && z < region[3]
}

/// How far a cliff's edges are bevelled, in cells: its top edge (in and down), its foot (the
/// ground below rising into it) and its outer corners (cut off down its height).
pub const CHAMFER: f32 = 0.22;

/// Where the faces around the map go down to.
const SKIRT: f32 = -0.6;

/// The cells across a cell's sides (north, south, west, east).
const SIDES: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
/// A cell's corner: the two sides meeting at it (their indices in SIDES), its u and v, and
/// the cell across it.
type Corner = (usize, usize, f32, f32, (i32, i32));
/// The corners: north-west, north-east, south-west, south-east.
const CORNERS: [Corner; 4] = [(0, 2, 0.0, 0.0, (-1, -1)), (0, 3, 1.0, 0.0, (1, -1)), (1, 2, 0.0, 1.0, (-1, 1)), (1, 3, 1.0, 1.0, (1, 1))];

/// What's around a cell, for the shape of its top: the sides where the ground steps down (or
/// the map ends: `low`) and where it steps up (`high`, the foot of a cliff; water stays
/// flat); the corners where only the cell across the corner is lower (`cuts`: cut down) or
/// higher (`bumps`: raised); and the corners where both sides step down to one level all
/// round (`round`), cut off down the cliff's height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Edges {
    low: [bool; 4],
    high: [bool; 4],
    cuts: [bool; 4],
    bumps: [bool; 4],
    round: [bool; 4],
}

impl Edges {
    fn of(level: &dyn Fn(i32, i32) -> Option<u8>, flat: &dyn Fn(i32, i32) -> bool, x: i32, z: i32) -> Edges {
        let own = level(x, z).unwrap_or(0);
        let at = |(dx, dz): (i32, i32)| level(x + dx, z + dz);
        let low = SIDES.map(|d| at(d).map(|l| l < own).unwrap_or(true));
        let rises = !flat(x, z);
        let high = SIDES.map(|d| rises && at(d).map(|l| l > own).unwrap_or(false));
        let mut e = Edges { low, high, ..Default::default() };
        for (k, &(a, b, _, _, across)) in CORNERS.iter().enumerate() {
            let diag = at(across);
            e.cuts[k] = !low[a] && !low[b] && diag.map(|l| l < own).unwrap_or(true);
            e.bumps[k] = rises && !high[a] && !high[b] && diag.map(|l| l > own).unwrap_or(false);
            let (la, lb) = (at(SIDES[a]), at(SIDES[b]));
            e.round[k] = low[a] && low[b] && la.is_some() && la == lb && lb == diag;
        }
        e
    }

    /// Level with everything around it.
    fn plain(&self) -> bool {
        ![self.low, self.high, self.cuts, self.bumps].iter().flatten().any(|&b| b)
    }

    /// How far below and above the cell's level its top is at a point of it (u, v from its
    /// north-west corner, 0 to 1): CHAMFER down at a side that steps down, back to the level
    /// CHAMFER in, and at a cut corner, across it; CHAMFER up at a side that steps up, and
    /// at a raised corner. Where two sides do the same, they meet in a mitre.
    fn drop_rise(&self, u: f32, v: f32) -> (f32, f32) {
        let c = CHAMFER;
        let (iu, iv) = (1.0 - u, 1.0 - v);
        let sides = [v, iv, u, iu];
        let corners = [u + v, iu + v, u + iv, iu + iv];
        let (mut drop, mut rise) = (0.0f32, 0.0f32);
        for k in 0..4 {
            if self.low[k] {
                drop = drop.max(c - sides[k]);
            }
            if self.high[k] {
                rise = rise.max(c - sides[k]);
            }
            if self.cuts[k] {
                drop = drop.max(c - corners[k]);
            }
            if self.bumps[k] {
                rise = rise.max(c - corners[k]);
            }
        }
        (drop.clamp(0.0, c), rise.clamp(0.0, c))
    }

    /// The top's height at a point of the cell, from its level.
    fn height(&self, u: f32, v: f32) -> f32 {
        let (d, r) = self.drop_rise(u, v);
        r - d
    }

    /// Whether a point of the cell is cut away with a rounded corner.
    fn cut_away(&self, u: f32, v: f32) -> bool {
        CORNERS.iter().enumerate().any(|(k, &(_, _, cu, cv, _))| self.round[k] && (u - cu).abs() + (v - cv).abs() < CHAMFER)
    }
}

/// A flat piece of a cell's top (a triangle or a planar quad), facing up; each vertex's alpha
/// says how much rock it is (down a bevel or up a cliff's foot: 0 on the level, 1 a whole
/// CHAMFER off it).
fn top_piece(m: &mut Mesh, ps: &[(Vec3, f32)], uv: [f32; 2]) {
    let mut n = (ps[1].0 - ps[0].0).cross(ps[2].0 - ps[0].0).normalize_or(Vec3::Y);
    if n.y < 0.0 {
        n = -n;
    }
    let i = m.vertices.len() as u32;
    for &(p, rock) in ps {
        m.vertices.push(Vertex { pos: p.to_array(), normal: n.to_array(), uv, color: [1.0, 1.0, 1.0, rock], toon: 0.0, wind: [0; 4] });
    }
    for k in 1..ps.len() as u32 - 1 {
        m.indices.extend_from_slice(&[i, i + k, i + k + 1]);
    }
}

/// A cell's top: one quad when the ground around it is level with it, else a 3 x 3 grid
/// whose rim is bevelled (down to lower ground, up to higher). A corner piece folds where its
/// planes meet: along the diagonal through the cell's corner where both sides do the same (a
/// mitre), along the other one at a cut or raised corner, along both when both; at a rounded
/// corner only the piece inside the cut is left.
fn cell_top(m: &mut Mesh, fx: f32, fz: f32, y: f32, e: &Edges) {
    let uv = [fx + 0.5, fz + 0.5];
    let c = CHAMFER;
    let p = |u: f32, v: f32| {
        let (d, r) = e.drop_rise(u, v);
        (Vec3::new(fx + u, y + r - d, fz + v), (d.max(r) / c).clamp(0.0, 1.0))
    };
    if e.plain() {
        top_piece(m, &[p(0.0, 0.0), p(0.0, 1.0), p(1.0, 1.0), p(1.0, 0.0)], uv);
        return;
    }
    let at = [0.0, c, 1.0 - c, 1.0];
    for j in 0..3 {
        for i in 0..3 {
            let (u0, u1, v0, v1) = (at[i], at[i + 1], at[j], at[j + 1]);
            let q = [p(u0, v0), p(u0, v1), p(u1, v1), p(u1, v0)];
            // a corner piece: which corner, which of q is the cell's corner, and whether the
            // diagonal through it is q0-q2 (else q1-q3)
            let corner = match (i, j) {
                (0, 0) => Some((0, 0, true)),
                (2, 0) => Some((1, 3, false)),
                (0, 2) => Some((2, 1, false)),
                (2, 2) => Some((3, 2, true)),
                _ => None,
            };
            let Some((k, at_corner, through_02)) = corner else {
                top_piece(m, &q, uv);
                continue;
            };
            if e.round[k] {
                let inside: Vec<(Vec3, f32)> = (0..4).filter(|&n| n != at_corner).map(|n| q[n]).collect();
                top_piece(m, &inside, uv);
                continue;
            }
            let (a, b, _, _, _) = CORNERS[k];
            let mitre = (e.low[a] && e.low[b]) || (e.high[a] && e.high[b]);
            let across = e.cuts[k] || e.bumps[k];
            if mitre && across {
                let mid = p((u0 + u1) / 2.0, (v0 + v1) / 2.0);
                for n in 0..4 {
                    top_piece(m, &[q[n], q[(n + 1) % 4], mid], uv);
                }
            } else if mitre || across {
                if through_02 == mitre {
                    top_piece(m, &[q[0], q[1], q[2]], uv);
                    top_piece(m, &[q[0], q[2], q[3]], uv);
                } else {
                    top_piece(m, &[q[0], q[1], q[3]], uv);
                    top_piece(m, &[q[1], q[2], q[3]], uv);
                }
            } else {
                top_piece(m, &q, uv);
            }
        }
    }
}

/// How far a cliff face bulges out at most, in cells (it dents in a little too).
const RELIEF: f32 = 0.13;

/// Smooth noise in space, 0 to 1: values on a lattice `scale` apart, eased between.
pub(crate) fn noise3(p: Vec3, scale: f32, seed: i32) -> f32 {
    let q = p / scale;
    let i = q.floor();
    let f = q - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let (x, y, z) = (i.x as i32, i.y as i32, i.z as i32);
    let h = |dx: i32, dy: i32, dz: i32| hash(x + dx + seed * 7919, z + dz, y + dy);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |dy: i32| lerp(lerp(h(0, dy, 0), h(1, dy, 0), u.x), lerp(h(0, dy, 1), h(1, dy, 1), u.x), u.z);
    lerp(plane(0), plane(1), u.y)
}

/// How far a cliff face stands out from its cell's edge at a point: lumps of rock.
fn relief(p: Vec3) -> f32 {
    let n = noise3(p, 0.55, 1) * 0.7 + noise3(p, 0.23, 2) * 0.3;
    (n * 1.35 - 0.35) * RELIEF
}

/// A cliff face down a run of points on the ground (x, z, its top, its foot), in order along
/// it: in rows, darker towards the foot, lumpy inside (out along `out`); its edges stay put
/// where they meet the bevel, the ground and other faces, but at an end where the face goes
/// on unchanged into the next cell's (`free`). uv: the cell above's centre; alpha 1, all rock.
fn face(m: &mut Mesh, points: &[(f32, f32, f32, f32)], rows: usize, out: Vec3, free: [bool; 2], uv: [f32; 2]) {
    let base = m.vertices.len() as u32;
    let last = points.len() - 1;
    for (ci, &(px, pz, top, foot)) in points.iter().enumerate() {
        // (a low face, a ramp's wall near its foot, is only as lumpy as it's high)
        let lumpy = ((top - foot) / 0.6).clamp(0.0, 1.0);
        for r in 0..=rows {
            let f = r as f32 / rows as f32;
            let p = Vec3::new(px, foot + (top - foot) * f, pz);
            let pinned = r == 0 || r == rows || (ci == 0 && !free[0]) || (ci == last && !free[1]);
            let p = if pinned { p } else { p + out * relief(p) * lumpy };
            let s = 0.72 + 0.28 * f;
            m.vertices.push(Vertex { pos: p.to_array(), normal: out.to_array(), uv, color: [s, s, s, 1.0], toon: 0.0, wind: [0; 4] });
        }
    }
    let n = rows as u32 + 1;
    for c in 0..last as u32 {
        for r in 0..rows as u32 {
            let (a0, a1, b0, b1) = (base + c * n + r, base + c * n + r + 1, base + (c + 1) * n + r, base + (c + 1) * n + r + 1);
            m.indices.extend_from_slice(&[a1, a0, b0, a1, b0, b1]);
        }
    }
}

/// Where a ramp cell's grid runs across it (and its walls' points along its edges): every
/// quarter, and where the cells next to it bend at their rims (so they meet without cracks).
const RAMP_AT: [f32; 7] = [0.0, CHAMFER, 0.25, 0.5, 0.75, 1.0 - CHAMFER, 1.0];

/// A ramp cell's top: a grid of quads on its ground (`at`: the height at a point of it), lit
/// as it slopes there, its alpha the rock showing there (`rock`).
fn ramp_top(m: &mut Mesh, fx: f32, fz: f32, rock: &dyn Fn(f32, f32) -> f32, at: &dyn Fn(f32, f32) -> f32) {
    let uv = [fx + 0.5, fz + 0.5];
    let base = m.vertices.len() as u32;
    let e = 0.04;
    for v in RAMP_AT {
        for u in RAMP_AT {
            let (px, pz) = (fx + u, fz + v);
            let n = Vec3::new(at(px - e, pz) - at(px + e, pz), 2.0 * e, at(px, pz - e) - at(px, pz + e)).normalize();
            m.vertices.push(Vertex { pos: [px, at(px, pz), pz], normal: n.to_array(), uv, color: [1.0, 1.0, 1.0, rock(px, pz)], toon: 0.0, wind: [0; 4] });
        }
    }
    let row = RAMP_AT.len() as u32;
    for j in 0..row - 1 {
        for i in 0..row - 1 {
            let a = base + j * row + i;
            m.indices.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
        }
    }
}

/// A wall down a run of points (x, z, its top, its foot), as a cliff face, unless it has no
/// height anywhere.
fn wall(m: &mut Mesh, points: &[(f32, f32, f32, f32)], out: Vec3, uv: [f32; 2]) {
    let high = points.iter().map(|&(_, _, top, foot)| top - foot).fold(0.0f32, f32::max);
    if high > 0.005 {
        face(m, points, (high / 0.5).ceil().max(1.0) as usize, out, [false, false], uv);
    }
}

/// Tufts of tall grass on a cell, swaying from their feet: each standing where `foot` says
/// (its height at a point of the cell, from its north-west corner; none where it's cut away).
fn tall_grass(grass: &mut Mesh, t: &content::Terrain, x: i32, z: i32, foot: &dyn Fn(f32, f32) -> Option<f32>) {
    let (c1, c2) = (color(&t.color), color(&t.color2));
    let (fx, fz) = (x as f32, z as f32);
    for k in 0..9 {
        let (u, v) = (0.12 + 0.76 * hash(x, z, k * 3), 0.12 + 0.76 * hash(x, z, k * 3 + 1));
        let Some(y) = foot(u, v) else { continue };
        let tall = 0.42 + 0.22 * hash(x, z, k * 3 + 2);
        let colour = if k % 3 == 0 { c2 } else { c1 };
        let mut blade = Mesh::default();
        shape_round(&mut blade, Vec3::new(0.2, tall, 0.2), colour, 0.0, 4);
        // swaying from its foot, each tuft in its own time
        let phase = hash(x, z, k * 3 + 7);
        for b in blade.vertices.iter_mut() {
            let up = (b.pos[1] / tall).clamp(0.0, 1.0);
            b.wind = wind_of(up.powf(1.5), 0.3 * up, phase, b.pos[1]);
        }
        grass.append(&blade, placement(Vec3::new(fx + u, y, fz + v), hash(x, z, k) * 90.0, 1.0));
    }
}

/// The cell tops, the cliff faces and the tall grass of a region's cells. Every edge of a
/// step in the ground is bevelled: the top edge (the higher cell's rim goes down), the foot
/// (the lower cell's rim comes up, unless it's water), and an outer corner standing on one
/// level all round (cut off down its height, the ground below filling the cut). The faces
/// run from the higher cell's rim down to the lower's, lumpy; they look at the neighbours
/// (two cells out), so the whole grid is read, and past the edge `beyond` (the map next to
/// this one: on its level, no face at all). A ramp's cells follow its ground (`ramps`: down a
/// level from their own; straight under what lies along it, without tall grass), with walls on
/// their sides where the ground next to them is lower,
/// or on their level (the step's top either side, down to the ramp's slope); the ground next
/// to a ramp on its level, or a level down, meets it unbevelled (its top, its foot, the walls
/// between), and higher ground has its cliff down onto it.
pub fn ground_region(map: &WorldMap, content: &Content, level_height: f32, region: [i32; 4], beyond: &dyn Beyond) -> (Mesh, Mesh, Mesh) {
    let mut tops = Mesh::default();
    let mut cliffs = Mesh::default();
    let mut grass = Mesh::default();
    let c = CHAMFER;
    let level = |x: i32, z: i32| map.level_at(x, z).or_else(|| beyond.level(x, z));
    let terrain = |x: i32, z: i32| map.terrain_at(x, z).and_then(|id| content.terrains.get(id));
    let water: Vec<bool> = content.terrains.iter().map(|t| t.walk == "water").collect();
    let flat = |x: i32, z: i32| match map.terrain_at(x, z) {
        Some(id) => content.terrains.get(id).map(|t| t.walk == "water").unwrap_or(false),
        None => beyond.terrain(x, z).and_then(|t| water.get(t as usize).copied()).unwrap_or(false),
    };
    // the ramps here and round about (the cells next to them see them)
    let near: Vec<&content::Ramp> = map
        .ramps
        .iter()
        .filter(|r| r.at[0] - 3 < region[2] && r.at[1] - 3 < region[3] && r.at[0] + r.size[0] as i32 + 3 > region[0] && r.at[1] + r.size[1] as i32 + 3 > region[1])
        .collect();
    let ramp = |x: i32, z: i32| near.iter().copied().find(|r| r.has(x, z) && map.inside(x, z));
    // those something lies along (stairs, a mud slide): their ground straight under it
    let covered: Vec<bool> = near.iter().map(|r| map.ramp_cover(r, content).is_some()).collect();
    let even = |r: &content::Ramp| near.iter().zip(&covered).any(|(o, &c)| c && std::ptr::eq(*o, r));
    let ground = |r: &content::Ramp, top: f32, px: f32, pz: f32| if even(r) { ramps::even(r, top, level_height, px, pz) } else { ramps::height(r, top, level_height, px, pz) };
    // the levels around a cell on `own`, as it sees them: a ramp's cell is level with it
    // when it's on the ramp's level or a level down (the ramp's walls fill in between)
    let seen = |own: Option<u8>| {
        move |x: i32, z: i32| -> Option<u8> {
            let l = level(x, z);
            match (own, l) {
                (Some(o), Some(r)) if (o == r || o + 1 == r) && ramp(x, z).is_some() => Some(o),
                _ => l,
            }
        }
    };
    // a cell's top at a point on it (world x, z); off the map, the skirt's foot
    let surface = |x: i32, z: i32, px: f32, pz: f32| match level(x, z) {
        Some(l) => match ramp(x, z) {
            Some(r) => ground(r, l as f32 * level_height, px, pz),
            None => l as f32 * level_height + Edges::of(&seen(Some(l)), &flat, x, z).height(px - x as f32, pz - z as f32),
        },
        None => SKIRT,
    };
    for (x, z) in region_cells(map, region) {
        let own = level(x, z);
        let y = own.unwrap_or(0) as f32 * level_height;
        let (fx, fz) = (x as f32, z as f32);
        let uv = [fx + 0.5, fz + 0.5];
        // each side's run along the cell's edge, seen from outside, and its corners
        let sides = [
            ([(fx + 1.0, fz), (fx, fz)], [1, 0]),
            ([(fx, fz + 1.0), (fx + 1.0, fz + 1.0)], [2, 3]),
            ([(fx, fz), (fx, fz + 1.0)], [0, 2]),
            ([(fx + 1.0, fz + 1.0), (fx + 1.0, fz)], [3, 1]),
        ];
        if let Some(r) = ramp(x, z) {
            let at = |px: f32, pz: f32| ground(r, y, px, pz);
            // (under what lies along it, its walls down a hair in from its ends: behind that
            // thing's own ends, not through them)
            let tuck = if even(r) { ramps::TUCK } else { 0.0 };
            ramp_top(&mut tops, fx, fz, &|px, pz| ramps::rock(r.along(px as f64, pz as f64).0 as f32), &at);
            for (k, ([(ax, az), (bx, bz)], _)) in sides.into_iter().enumerate() {
                let (dx, dz) = SIDES[k];
                let (nx, nz) = (x + dx, z + dz);
                let next = ramp(nx, nz);
                if next.is_some_and(|o| std::ptr::eq(o, r)) {
                    continue;
                }
                let points: Vec<(f32, f32, f32, f32)> = RAMP_AT
                    .iter()
                    .map(|&t| {
                        let (px, pz) = (ax + (bx - ax) * t, az + (bz - az) * t);
                        (px, pz, at(px, pz), surface(nx, nz, px, pz))
                    })
                    .collect();
                // down to lower ground
                let down: Vec<(f32, f32, f32, f32)> = points.iter().map(|&(px, pz, top, other)| (px - dx as f32 * tuck, pz - dz as f32 * tuck, top, other.min(top))).collect();
                wall(&mut cliffs, &down, Vec3::new(dx as f32, 0.0, dz as f32), uv);
                // up to the ground on its level next to it (the step's top either side, or its
                // foot's level where it dips under); higher ground has its own cliff down to it
                let up_to = level(nx, nz).zip(own).is_some_and(|(l, o)| l <= o);
                if next.is_none() && up_to {
                    let up: Vec<(f32, f32, f32, f32)> = points.iter().rev().map(|&(px, pz, top, other)| (px, pz, other.max(top), top)).collect();
                    wall(&mut cliffs, &up, Vec3::new(-dx as f32, 0.0, -dz as f32), uv);
                }
            }
            // (none through what lies along it)
            if let Some(t) = terrain(x, z).filter(|t| t.pattern == "tall" && !even(r)) {
                tall_grass(&mut grass, t, x, z, &|u, v| Some(at(fx + u, fz + v)));
            }
            continue;
        }
        let e = Edges::of(&seen(own), &flat, x, z);
        cell_top(&mut tops, fx, fz, y, &e);
        // a face on each side that steps down, sampled where the rims either side bend; it
        // stops short of a rounded corner
        for (k, ([(ax, az), (bx, bz)], ends)) in sides.into_iter().enumerate() {
            if !e.low[k] {
                continue;
            }
            let (dx, dz) = SIDES[k];
            let across = level(x + dx, z + dz);
            let foot = across.map(|l| l as f32 * level_height).unwrap_or(SKIRT);
            let rows = ((y - c - foot) / 0.5).ceil().max(1.0) as usize;
            let (ex, ez) = ((bx - ax) as i32, (bz - az) as i32);
            // (not into a ramp: its walls meet the face where it stops, pinned)
            let goes_on = |cx: i32, cz: i32| level(cx, cz) == level(x, z) && level(cx + dx, cz + dz) == across && ramp(cx, cz).is_none() && ramp(cx + dx, cz + dz).is_none();
            let free = [goes_on(x - ex, z - ez), goes_on(x + ex, z + ez)];
            let mut cols = vec![0.0, c, 0.5, 1.0 - c, 1.0];
            if e.round[ends[0]] {
                cols.remove(0);
            }
            if e.round[ends[1]] {
                cols.pop();
            }
            let points: Vec<(f32, f32, f32, f32)> = cols
                .iter()
                .map(|&t| {
                    let (px, pz) = (ax + (bx - ax) * t, az + (bz - az) * t);
                    (px, pz, y + e.height(px - fx, pz - fz), surface(x + dx, z + dz, px, pz))
                })
                .collect();
            face(&mut cliffs, &points, rows, Vec3::new(dx as f32, 0.0, dz as f32), free, uv);
        }
        // a rounded corner: a face down the cut, and the ground across the corner under it
        for (k, &(a, b, cu, cv, (dx, dz))) in CORNERS.iter().enumerate() {
            if !e.round[k] {
                continue;
            }
            // the cut's ends: on side a's edge (along x) and side b's (along z)
            let pa = (fx + cu + if cu == 0.0 { c } else { -c }, fz + cv);
            let pb = (fx + cu, fz + cv + if cv == 0.0 { c } else { -c });
            let (sa, sb) = (SIDES[a], SIDES[b]);
            let foot_a = surface(x + sa.0, z + sa.1, pa.0, pa.1);
            let foot_b = surface(x + sb.0, z + sb.1, pb.0, pb.1);
            let below = level(x + dx, z + dz).unwrap_or(0) as f32 * level_height;
            let rows = ((y - c - below) / 0.5).ceil().max(1.0) as usize;
            let out = Vec3::new(dx as f32, 0.0, dz as f32).normalize();
            face(&mut cliffs, &[(pa.0, pa.1, y - c, foot_a), (pb.0, pb.1, y - c, foot_b)], rows, out, [false, false], uv);
            let corner = surface(x + dx, z + dz, fx + cu, fz + cv);
            let rock = |h: f32| ((h - below) / c).clamp(0.0, 1.0);
            let floor = [
                (Vec3::new(fx + cu, corner, fz + cv), rock(corner)),
                (Vec3::new(pa.0, foot_a, pa.1), rock(foot_a)),
                (Vec3::new(pb.0, foot_b, pb.1), rock(foot_b)),
            ];
            top_piece(&mut tops, &floor, [fx + dx as f32 + 0.5, fz + dz as f32 + 0.5]);
        }
        // tall grass: tufts of blades, standing on the top (on its rim too)
        if let Some(t) = terrain(x, z).filter(|t| t.pattern == "tall") {
            tall_grass(&mut grass, t, x, z, &|u, v| (!e.cut_away(u, v)).then(|| y + e.height(u, v)));
        }
    }
    (tops, cliffs, grass)
}

/// The props placed on the map.
pub fn props(map: &WorldMap, content: &Content, level_height: f32, models: &Models) -> Scene {
    props_region(map, content, level_height, models, whole(map))
}

/// The props whose foot stands in a region.
pub fn props_region(map: &WorldMap, content: &Content, level_height: f32, models: &Models, region: [i32; 4]) -> Scene {
    let mut cache: HashMap<&str, Scene> = HashMap::new();
    let mut out = Scene::default();
    for p in map.props.iter().filter(|p| in_region(region, p.at[0].floor() as i32, p.at[1].floor() as i32)) {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let scene = cache.entry(prop.id.as_str()).or_insert_with(|| prop_scene(prop, models));
        for put in map.puts(p, prop, level_height) {
            out.append_put(scene, &put);
        }
    }
    // the cells laid with flooring out of doors, each on its ground, turned (never on a ramp's
    // slope)
    for t in map.floors.iter().filter(|t| in_region(region, t.cell[0], t.cell[1])) {
        let [x, z] = t.cell;
        let (Some(prop), Some(level)) = (content.props.get(&t.tile), map.level_at(x, z)) else { continue };
        if map.ramp_at(x, z).is_some() {
            continue;
        }
        let scene = cache.entry(prop.id.as_str()).or_insert_with(|| prop_scene(prop, models));
        out.append(scene, tile_placement(Vec3::new(x as f32 + 0.5, level as f32 * level_height + 0.005, z as f32 + 0.5), t.turn, t.flip));
    }
    out
}

/// The tree areas' trees.
pub fn tree_mesh(map: &WorldMap, content: &Content, level_height: f32, models: &Models) -> Scene {
    trees_from(&trees(map, content, level_height), content, models, whole(map))
}

/// The trees (placed by `trees::trees`) whose foot stands in a region.
pub fn trees_from(trees: &[Tree], content: &Content, models: &Models, region: [i32; 4]) -> Scene {
    let mut cache: HashMap<&str, Scene> = HashMap::new();
    let mut out = Scene::default();
    for t in trees.iter().filter(|t| in_region(region, t.pos[0].floor() as i32, t.pos[2].floor() as i32)) {
        let Some(prop) = content.props.get(&t.prop) else { continue };
        let scene = cache.entry(prop.id.as_str()).or_insert_with(|| prop_scene(prop, models));
        out.append(scene, placement(Vec3::from(t.pos), t.turn, t.scale));
    }
    out
}

/// How many cells a far square's blocks are a side.
pub const FAR_STEP: i32 = 4;

/// A region as seen from afar: the ground as one quad per FAR_STEP x FAR_STEP block, its
/// corners at the corner cells' levels (slopes instead of cliffs, and no gaps), drawn by
/// the terrain shader like the cell tops; and, plain, a canopy over the blocks that are
/// mostly tree area and a box per building. Cheap enough for a whole map at once.
pub fn far_region(map: &WorldMap, content: &Content, level_height: f32, region: [i32; 4]) -> (Mesh, Mesh) {
    let (w, h) = (map.width as i32, map.height as i32);
    let (x0, z0) = (region[0].max(0), region[1].max(0));
    let (x1, z1) = (region[2].min(w), region[3].min(h));
    let mut tops = Mesh::default();
    let mut plain = Mesh::default();
    if x1 <= x0 || z1 <= z0 {
        return (tops, plain);
    }
    let level = |x: i32, z: i32| map.level_at(x.min(w - 1), z.min(h - 1)).unwrap_or(0) as f32 * level_height;
    let canopy = color("#2d6a38");
    // the woods' canopy: about two thirds of the way up their tallest trees
    let crowns = map
        .tree_areas
        .iter()
        .flat_map(|a| a.trees.iter().filter_map(|id| content.props.get(id)).map(move |p| p.height * (a.height[0] + a.height[1]) / 2.0))
        .fold(0.0f64, f64::max);
    let above = if crowns > 0.0 { crowns as f32 * 0.65 } else { 1.7 };
    let mut z = z0;
    while z < z1 {
        let zb = (z + FAR_STEP).min(z1);
        let mut x = x0;
        while x < x1 {
            let xb = (x + FAR_STEP).min(x1);
            let i = tops.vertices.len() as u32;
            for (px, pz) in [(x, z), (x, zb), (xb, zb), (xb, z)] {
                tops.vertices.push(Vertex { pos: [px as f32, level(px, pz), pz as f32], normal: [0.0, 1.0, 0.0], uv: [px as f32 + 0.01, pz as f32 + 0.01], color: [1.0, 1.0, 1.0, 0.0], toon: 0.0, wind: [0; 4] });
            }
            tops.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            let (mut n, mut forest, mut sum) = (0, 0, 0.0f32);
            for cz in z..zb {
                for cx in x..xb {
                    if let Some(idx) = map.index(cx, cz) {
                        n += 1;
                        forest += (map.area[idx] > 0) as i32;
                        sum += map.level[idx] as f32;
                    }
                }
            }
            if n > 0 && forest * 2 >= n {
                let y = sum / n as f32 * level_height + above;
                plain.quad([Vec3::new(x as f32, y, z as f32), Vec3::new(x as f32, y, zb as f32), Vec3::new(xb as f32, y, zb as f32), Vec3::new(xb as f32, y, z as f32)], canopy);
            }
            x = xb;
        }
        z = zb;
    }
    for b in map.buildings.iter().filter(|b| b.at[0] >= region[0] && b.at[1] >= region[1] && b.at[0] < region[2] && b.at[1] < region[3]) {
        let wall = content.props.get(&b.wall).map(|p| color(p.color(0))).unwrap_or([0.9, 0.86, 0.8, 1.0]);
        let (bw, bh) = b.size();
        let y0 = b.level as f32 * level_height;
        let y1 = y0 + b.floors.len().max(1) as f32 * crate::floor_height(level_height) + 0.5;
        crate::buildings::aabb(&mut plain, Vec3::new(b.at[0] as f32, y0, b.at[1] as f32), Vec3::new((b.at[0] + bw) as f32, y1, (b.at[1] + bh) as f32), wall);
    }
    (tops, plain)
}

/// Props and trees together.
pub fn objects(map: &WorldMap, content: &Content, level_height: f32, models: &Models) -> Scene {
    let mut out = props(map, content, level_height, models);
    let trees = tree_mesh(map, content, level_height, models);
    out.append(&trees, Mat4::IDENTITY);
    out
}

#[cfg(test)]
mod bevel_tests {
    use super::*;

    /// A little landscape: a plateau with a notch and a hole, a step down to the south, a
    /// lone pillar; '.' is off the map.
    const ROWS: [&str; 7] = ["0000000.", "0111110.", "0110110.", "0111110.", "0001000.", "0000020.", "00000000"];

    fn level(x: i32, z: i32) -> Option<u8> {
        let row = ROWS.get(usize::try_from(z).ok()?)?;
        let c = *row.as_bytes().get(usize::try_from(x).ok()?)?;
        (c != b'.').then(|| c - b'0')
    }

    fn dry(_: i32, _: i32) -> bool {
        false
    }

    #[test]
    fn tops_on_one_level_meet_without_steps() {
        for z in 0..7 {
            for x in 0..8 {
                let Some(l) = level(x, z) else { continue };
                let a = Edges::of(&level, &dry, x, z);
                for (dx, dz) in [(1, 0), (0, 1)] {
                    if level(x + dx, z + dz) != Some(l) {
                        continue;
                    }
                    let b = Edges::of(&level, &dry, x + dx, z + dz);
                    for t in [0.0, CHAMFER * 0.5, CHAMFER, 0.5, 1.0 - CHAMFER, 1.0] {
                        // the shared edge, in each cell's own (u, v)
                        let (ua, va, ub, vb) = if dx == 1 { (1.0, t, 0.0, t) } else { (t, 1.0, t, 0.0) };
                        assert!((a.height(ua, va) - b.height(ub, vb)).abs() < 1e-6, "{x},{z} and the cell {dx},{dz} on at {t}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_ground_comes_up_into_a_cliff_unless_it_is_water() {
        // north of the plateau: its south side steps up
        let e = Edges::of(&level, &dry, 2, 0);
        assert_eq!(e.high, [false, true, false, false]);
        assert!((e.height(0.5, 1.0) - CHAMFER).abs() < 1e-6 && e.height(0.5, 1.0 - CHAMFER).abs() < 1e-6);
        // and where the plateau's corner only touches a cell's corner, that corner comes up
        let e = Edges::of(&level, &dry, 0, 0);
        assert!(e.bumps[3] && (e.height(1.0, 1.0) - CHAMFER).abs() < 1e-6);
        // water stays flat
        let wet = |_: i32, _: i32| true;
        assert!(Edges::of(&level, &wet, 2, 0).height(0.5, 1.0).abs() < 1e-6);
    }

    /// A map past the east edge: level 1 along the plateau's rows, 0 elsewhere.
    struct East;

    impl Beyond for East {
        fn level(&self, x: i32, z: i32) -> Option<u8> {
            (x >= 8 && (0..7).contains(&z)).then_some(if (1..4).contains(&z) { 1 } else { 0 })
        }

        fn terrain(&self, x: i32, z: i32) -> Option<u8> {
            self.level(x, z).map(|_| 2)
        }
    }

    #[test]
    fn a_map_next_door_on_the_same_level_leaves_no_face_at_the_seam() {
        let mut map = WorldMap::new("t", "t", 8, 7, "grass");
        for z in 0..7 {
            for x in 0..8 {
                if let Some(i) = map.index(x, z) {
                    map.level[i] = level(x, z).unwrap_or(0);
                }
            }
        }
        let east_faces = |beyond: &dyn Beyond| {
            let (_, cliffs, _) = ground_region(&map, &Content::default(), 2.0, whole(&map), beyond);
            cliffs.vertices.iter().filter(|v| (v.pos[0] - 8.0).abs() < 1e-4 && v.normal[0] > 0.5).count()
        };
        // alone, the east edge has its skirt; with the map next door (level 0 there, as
        // the edge's cells are), none
        assert!(east_faces(&Alone) > 0);
        assert_eq!(east_faces(&East), 0);
        // and the cells texture's ring holds the next map's cells
        let content = Content::default();
        let cells = cells_texture(&map, &content, &East);
        assert_eq!(cells.len(), 10 * 9 * 4);
        let ring = |x: i32, z: i32| ((z + 1) * 10 + x + 1) as usize * 4;
        assert_eq!(&cells[ring(8, 2)..ring(8, 2) + 2], &[2, 1]);
        // where no map is, the edge's own cell again
        assert_eq!(cells[ring(-1, 3) + 1], level(0, 3).unwrap());
        assert_eq!(cell_texel(&map, 0, 0), Some(11));
    }

    #[test]
    fn a_side_that_steps_down_is_bevelled_all_along() {
        // the plateau's north-west cell: north and west step down, its corner rounded (the
        // ground all round it is on one level)
        let e = Edges::of(&level, &dry, 1, 1);
        assert_eq!(e.low, [true, false, true, false]);
        assert_eq!(e.round, [true, false, false, false]);
        for t in [0.0, 0.3, 1.0] {
            assert!((e.height(t, 0.0) + CHAMFER).abs() < 1e-6 && (e.height(0.0, t) + CHAMFER).abs() < 1e-6);
        }
        assert!(e.height(0.5, 0.5).abs() < 1e-6);
        assert!(e.cut_away(0.05, 0.1) && !e.cut_away(0.2, 0.2));
        // north-west of the hole: the north side steps down, the corner towards the hole is cut
        let e = Edges::of(&level, &dry, 2, 1);
        assert_eq!((e.low, e.cuts), ([true, false, false, false], [false, false, false, true]));
        let e = Edges::of(&level, &dry, 2, 3);
        assert_eq!(e.cuts, [false, true, false, false], "the hole is north-east of 2,3");
        assert!((e.height(1.0, 0.0) + CHAMFER).abs() < 1e-6 && e.height(1.0 - CHAMFER, 0.0).abs() < 1e-6);
        // off the map counts as lower, and a corner off the map isn't rounded
        let e = Edges::of(&level, &dry, 6, 0);
        assert_eq!((e.low, e.round), ([true, false, false, true], [false; 4]));
        // the plateau's tongue to the south: its south corners stand on one level all round
        assert_eq!(Edges::of(&level, &dry, 3, 4).round, [false, false, true, true]);
        // the corners over two levels (north 1, the rest 0) keep their edge
        let steps = |x: i32, z: i32| Some(match (x, z) {
            (0, 0) => 2,
            (0, -1) => 1,
            _ => 0,
        });
        let e = Edges::of(&steps, &dry, 0, 0);
        assert_eq!((e.low, e.round), ([true, true, true, true], [false, false, true, true]));
    }

    #[test]
    fn faces_run_from_under_the_bevel_to_the_ground_across() {
        let mut map = WorldMap::new("t", "t", 8, 7, "grass");
        for z in 0..7 {
            for x in 0..8 {
                if let Some(i) = map.index(x, z) {
                    map.level[i] = level(x, z).unwrap_or(0);
                }
            }
        }
        let (tops, cliffs, _) = ground_region(&map, &Content::default(), 2.0, whole(&map), &Alone);
        assert!(!tops.indices.is_empty() && !cliffs.indices.is_empty());
        // every face vertex lies by a cell edge (no further than the lumps), all rock
        for v in &cliffs.vertices {
            let off = |a: f32| (a - a.round()).abs();
            assert!(off(v.pos[0]).min(off(v.pos[2])) <= RELIEF + 1e-5, "{:?}", v.pos);
            assert_eq!(v.color[3], 1.0);
        }
        // the plateau's north side runs on from cell to cell: the two faces either side of
        // x = 3 share that column, lumps and all (each point twice, some of them lumped)
        let near_north = |v: &&Vertex| v.pos[2] <= 1.0 + 1e-6 && v.pos[2] >= 1.0 - RELIEF - 1e-6;
        let shared: Vec<[f32; 3]> = cliffs.vertices.iter().filter(near_north).filter(|v| v.pos[0] == 3.0).map(|v| v.pos).collect();
        assert!(shared.len() >= 6, "{shared:?}");
        for p in &shared {
            assert_eq!(shared.iter().filter(|q| *q == p).count() % 2, 0, "{p:?} in only one of the faces");
        }
        assert!(shared.iter().any(|p| p[2] != 1.0), "no lumps: {shared:?}");
        // the plateau's corner (1, 1) is rounded: the north face stops CHAMFER short of it,
        // and a face down the cut joins it to the west one
        assert!(cliffs.vertices.iter().filter(near_north).filter(|v| v.normal[2] < -0.99).all(|v| v.pos[0] >= 1.0 + CHAMFER - 1e-4));
        let cut: Vec<[f32; 3]> = cliffs.vertices.iter().filter(|v| v.normal[0] < -0.5 && v.normal[2] < -0.5).map(|v| v.pos).collect();
        assert!(cut.iter().any(|p| (p[0] - 1.0 - CHAMFER).abs() < 1e-4 && p[2] == 1.0), "{cut:?}");
        assert!(cut.iter().any(|p| p[0] == 1.0 && (p[2] - 1.0 - CHAMFER).abs() < 1e-4), "{cut:?}");
        // and its foot stands on the ground come up to it
        assert!(cut.iter().map(|p| p[1]).fold(f32::MAX, f32::min) > CHAMFER - 1e-4);
        // the tops' rock share: 0 inside, 1 a whole bevel off the level
        assert!(tops.vertices.iter().all(|v| (0.0..=1.0).contains(&v.color[3])));
        assert!(tops.vertices.iter().any(|v| v.color[3] == 1.0) && tops.vertices.iter().any(|v| v.color[3] == 0.0));
        // and every top faces up
        assert!(tops.vertices.iter().all(|v| v.normal[1] > 0.5));
    }
}

#[cfg(test)]
mod room_tests {
    use super::*;
    use content::Part;

    #[test]
    fn a_small_prop_moves_within_its_cell_a_big_one_does_not() {
        let part = |size: f64| Part { shape: "box".into(), size: [size, 1.0, size], at: [0.0, 0.0, 0.0], color: "#888888".into(), turn: 0.0 };
        let small = Prop { id: "pebble".into(), footprint: [1, 1], parts: vec![part(0.4)], ..Default::default() };
        let (lo, hi) = offset_room(&small, &Models::new(), 0.0, 1.0);
        assert!((lo[0] + 0.3).abs() < 1e-4 && (hi[0] - 0.3).abs() < 1e-4 && (hi[1] - 0.3).abs() < 1e-4, "{lo:?} {hi:?}");
        // turned 45 degrees it's wider: less room
        let (_, hi45) = offset_room(&small, &Models::new(), 45.0, 1.0);
        assert!(hi45[0] < 0.3 - 0.05, "{hi45:?}");
        // half the size, more room
        let (_, hi_half) = offset_room(&small, &Models::new(), 0.0, 0.5);
        assert!((hi_half[0] - 0.4).abs() < 1e-4, "{hi_half:?}");
        let big = Prop { id: "block".into(), footprint: [1, 1], parts: vec![part(1.0)], ..Default::default() };
        assert_eq!(offset_room(&big, &Models::new(), 0.0, 1.0), ([0.0, 0.0], [0.0, 0.0]));
        // a long prop on two cells has room along its length
        let bench = Prop { id: "bench".into(), footprint: [2, 1], parts: vec![Part { size: [1.2, 0.5, 0.4], ..part(0.0) }], ..Default::default() };
        let (lo, hi) = offset_room(&bench, &Models::new(), 0.0, 1.0);
        assert!((hi[0] - 0.4).abs() < 1e-4 && (lo[1] + 0.3).abs() < 1e-4, "{lo:?} {hi:?}");
        // a quarter turn: its length runs along z now
        let (_, hi90) = offset_room(&bench, &Models::new(), 90.0, 1.0);
        assert!((hi90[1] - 0.4).abs() < 1e-3 && (hi90[0] - 0.3).abs() < 1e-3, "{hi90:?}");
    }

    #[test]
    fn stretched_along_its_faces_lean_as_they_do() {
        // a face on the plane x + y = 1, its normal halfway between x and up
        let mut face = Mesh::default();
        face.tri(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 1.0), [1.0; 4]);
        let n = Vec3::from(face.vertices[0].normal);
        assert!(n.distance(Vec3::new(1.0, 1.0, 0.0).normalize()) < 1e-5, "{n:?}");
        // twice as long along x: the plane x / 2 + y = 1, steeper no more
        let mut out = Mesh::default();
        out.append_put(&face, &Put { m: Mat4::IDENTITY, along: Some(Along { half: 1.0, to: 2.0, ends: 0.0 }) });
        assert_eq!(out.vertices[0].pos, [2.0, 0.0, 0.0]);
        // (past its width as made, a part keeps its size)
        assert_eq!(Along { half: 0.5, to: 1.0, ends: 0.0 }.x(0.75), 1.25);
        let n = Vec3::from(out.vertices[0].normal);
        assert!(n.distance(Vec3::new(0.5, 1.0, 0.0).normalize()) < 1e-5, "{n:?}");
        // turned and scaled evenly, as before: turned only
        let mut turned = Mesh::default();
        turned.append(&face, placement(Vec3::ZERO, 90.0, 2.0));
        let n = Vec3::from(turned.vertices[0].normal);
        assert!(n.distance(Mat4::from_rotation_y(-90f32.to_radians()).transform_vector3(Vec3::new(1.0, 1.0, 0.0).normalize())) < 1e-5, "{n:?}");
        // a wedge: its slope from its back's top to its front's foot
        let w = part_mesh(&Part { shape: "wedge".into(), size: [1.0, 2.0, 1.0], color: "#888888".into(), ..Default::default() }, 1);
        let slope: Vec<&Vertex> = w.vertices.iter().filter(|v| v.normal[1] > 0.1).collect();
        assert_eq!((w.vertices.len(), w.indices.len(), slope.len()), (14, 18, 4));
        assert!(slope.iter().all(|v| (v.pos[1] - (1.0 - 2.0 * v.pos[2])).abs() < 1e-5 && (v.normal[2] - 2.0 / 5f32.sqrt()).abs() < 1e-5));
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;

    /// A floor tile laid mirrored: its east and west swapped (then turned), its faces still
    /// facing up as they did.
    #[test]
    fn a_tile_laid_mirrored_is_mirrored_facing_the_same_way() {
        let mut tile = Mesh::default();
        tile.tri(Vec3::new(0.3, 0.05, -0.4), Vec3::new(-0.4, 0.05, 0.4), Vec3::new(0.4, 0.05, 0.4), [1.0; 4]);
        let face = |m: &Mesh| {
            let p = |i: usize| Vec3::from(m.vertices[m.indices[i] as usize].pos);
            (p(1) - p(0)).cross(p(2) - p(0)).normalize()
        };
        let at = Vec3::new(5.5, 0.0, 2.5);
        let (mut plain, mut flipped) = (Mesh::default(), Mesh::default());
        plain.append(&tile, tile_placement(at, 0, false));
        flipped.append(&tile, tile_placement(at, 0, true));
        assert!((flipped.vertices[0].pos[0] - (5.5 - 0.3)).abs() < 1e-5, "its east now west: {:?}", flipped.vertices[0].pos);
        assert!(face(&plain).dot(face(&flipped)) > 0.99, "facing the same way: {} {}", face(&plain), face(&flipped));
        assert!((Vec3::from(flipped.vertices[0].normal) - Vec3::from(plain.vertices[0].normal)).length() < 1e-5, "its normals as they were (a flat face)");
        // mirrored, then turned a quarter clockwise: what was east (mirrored west) faces north
        let mut turned = Mesh::default();
        turned.append(&tile, tile_placement(at, 1, true));
        let p = Vec3::from(turned.vertices[0].pos) - at;
        assert!((p.x - 0.4).abs() < 1e-5 && (p.z + 0.3).abs() < 1e-5, "{p}");
    }
}

#[cfg(test)]
mod wind_tests {
    use super::*;
    use content::Part;

    /// A stand-in in the wind sways more the higher up (its foot rooted), flutters, each part
    /// in its own time, its height above its foot kept (scaled with it); one that isn't, and
    /// the ground, don't move; the tall grass sways from its foot.
    #[test]
    fn what_moves_in_the_wind_is_weighted_by_height() {
        let mut tree = Prop { id: "tree".into(), footprint: [1, 1], height: 3.0, ..Default::default() };
        tree.parts = vec![
            Part { shape: "cylinder".into(), size: [0.3, 1.2, 0.3], color: "#7a5434".into(), ..Default::default() },
            Part { shape: "sphere".into(), size: [1.6, 1.4, 1.6], at: [0.0, 1.0, 0.0], color: "#4c9a3e".into(), ..Default::default() },
        ];
        assert!(prop_model(&tree).vertices.iter().all(|v| v.wind == [0; 4]), "no wind asked: still");
        tree.wind = content::Breeze { sway: 1.0, flutter: 0.5 };
        let m = prop_model(&tree);
        let foot = m.vertices.iter().filter(|v| v.pos[1] < 0.01).map(|v| v.wind[0]).max().unwrap();
        let top = m.vertices.iter().max_by(|a, b| a.pos[1].total_cmp(&b.pos[1])).unwrap();
        assert_eq!(foot, 0, "rooted");
        // (its top at 2.4 of the prop's 3: swaying (0.8)^1.4, fluttering half of 0.8)
        let up = top.pos[1] / 3.0;
        assert!((top.wind[0] as f32 - up.powf(1.4) * 255.0).abs() <= 1.0 && (top.wind[1] as f32 - 0.5 * up * 255.0).abs() <= 1.0, "the top sways and flutters: {:?}", top.wind);
        assert!((top.wind[3] as f32 / 255.0 * WIND_REACH - top.pos[1]).abs() < 0.05, "its height kept: {:?} at {}", top.wind, top.pos[1]);
        let phases: std::collections::BTreeSet<u8> = m.vertices.iter().map(|v| v.wind[2]).collect();
        assert_eq!(phases.len(), 2, "each part in its own time");
        // placed twice as big: twice as high above its foot
        let mut placed = Mesh::default();
        placed.append(&m, placement(Vec3::new(4.0, 2.0, 4.0), 30.0, 2.0));
        let high = placed.vertices.iter().map(|v| v.wind[3]).max().unwrap();
        assert!((high as f32 - top.wind[3] as f32 * 2.0).abs() <= 1.0, "{high} vs {}", top.wind[3]);
        // the tall grass sways from its foot
        let mut map = WorldMap::new("t", "T", 3, 3, "tall_grass");
        map.terrains = vec!["tall_grass".into()];
        let mut content = Content::default();
        content.terrains.put(content::Terrain { id: "tall_grass".into(), pattern: "tall".into(), color: "#3f9a3e".into(), color2: "#2e7430".into(), ..Default::default() }, content::Space::Content);
        let g = ground(&map, &content, 2.0);
        assert!(!g.grass.vertices.is_empty());
        assert!(g.grass.vertices.iter().any(|v| v.wind[0] > 200) && g.grass.vertices.iter().any(|v| v.wind[0] == 0));
        assert!(g.tops.vertices.iter().all(|v| v.wind == [0; 4]), "the ground stays");
    }

    /// The palette says what's water (and of which kind) and what's a beach.
    #[test]
    fn the_palette_knows_the_waters_and_the_beaches() {
        let mut content = Content::default();
        for (id, pattern, water, beach) in [("grass", "grass", "", false), ("sand", "sand", "", true), ("water", "water", "", false), ("sea", "water", "oceanic", false)] {
            content.terrains.put(content::Terrain { id: id.into(), pattern: pattern.into(), color: "#808080".into(), water: water.into(), beach, ..Default::default() }, content::Space::Content);
        }
        let (palette, width) = palette(&content);
        let at = |id: &str| terrain_index(&content, id) as usize;
        let row2 = |t: usize| &palette[(2 * width as usize + t) * 4..(2 * width as usize + t) * 4 + 4];
        assert_eq!((row2(at("grass"))[2], row2(at("grass"))[3]), (0, 0));
        assert_eq!((row2(at("sand"))[2], row2(at("sand"))[3]), (0, 255), "sand: a beach");
        assert_eq!(row2(at("water"))[2], 1, "water drawn as water: inland");
        assert_eq!(row2(at("sea"))[2], 2, "the sea: oceanic");
    }
}
