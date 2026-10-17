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
use crate::{color, Rng, WorldMap};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// on cell tops: the cell's centre (x, z), so the shader knows which cell it's on
    pub uv: [f32; 2],
    /// linear
    pub color: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn vertex(&mut self, pos: Vec3, normal: Vec3, color: [f32; 4]) -> u32 {
        self.vertices.push(Vertex { pos: pos.to_array(), normal: normal.to_array(), uv: [0.5, 0.5], color });
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

    /// Adds another mesh moved by `m` (rotation, uniform scale, translation).
    pub fn append(&mut self, other: &Mesh, m: Mat4) {
        let base = self.vertices.len() as u32;
        let rot = Mat3::from_mat4(m);
        for v in &other.vertices {
            let pos = m.transform_point3(Vec3::from(v.pos));
            let normal = (rot * Vec3::from(v.normal)).normalize_or(Vec3::Y);
            self.vertices.push(Vertex { pos: pos.to_array(), normal: normal.to_array(), ..*v });
        }
        self.indices.extend(other.indices.iter().map(|i| i + base));
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
        self.plain.append(&other.plain, m);
        for (k, mesh) in &other.textured {
            self.textured.entry(k.clone()).or_default().append(mesh, m);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.plain.indices.is_empty() && self.textured.values().all(|m| m.indices.is_empty())
    }
}

impl From<Mesh> for Scene {
    fn from(plain: Mesh) -> Scene {
        Scene { plain, textured: BTreeMap::new() }
    }
}

/// Props' real models, loaded from their files, by prop id.
pub type Models = HashMap<String, crate::gltf_in::Model>;

/// What a prop looks like: its real model when it has one loaded, else its stand-in.
pub fn prop_scene(prop: &Prop, models: &Models) -> Scene {
    match models.get(&prop.id) {
        Some(m) => m.scene.clone(),
        None => prop_model(prop).into(),
    }
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

/// A prop's stand-in model, its footprint's centre at the origin, standing on y = 0.
pub fn prop_model(prop: &Prop) -> Mesh {
    let mut out = Mesh::default();
    for (i, p) in prop.parts.iter().enumerate() {
        let m = part_mesh(p, 7 + i as u32 * 31 + prop.id.len() as u32);
        let at = Vec3::new(p.at[0] as f32, p.at[1] as f32, p.at[2] as f32);
        out.append(&m, Mat4::from_translation(at) * Mat4::from_rotation_y(-(p.turn as f32).to_radians()));
    }
    out
}

/// Where a placed thing stands: at its foot, turned clockwise (seen from above), scaled.
pub fn placement(at: Vec3, turn_degrees: f32, scale: f32) -> Mat4 {
    Mat4::from_translation(at) * Mat4::from_rotation_y(-turn_degrees.to_radians()) * Mat4::from_scale(Vec3::splat(scale))
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
    /// width x height texels: terrain index, level, 0, 255
    pub cells: Vec<u8>,
    /// terrains x 4 rows: colour, second colour (sRGB), [pattern, priority + 128, water,
    /// 255], the cliff side's colour (sRGB)
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

fn hash(x: i32, z: i32, k: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ (k as u32).wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// The cells' texture (what changes when terrain is painted).
pub fn cells_texture(map: &WorldMap) -> Vec<u8> {
    let mut cells = Vec::with_capacity(map.width * map.height * 4);
    for i in 0..map.width * map.height {
        cells.extend_from_slice(&[map.terrain[i].min(255) as u8, map.level[i], 0, 255]);
    }
    cells
}

pub fn palette(map: &WorldMap, content: &Content) -> (Vec<u8>, u32) {
    let n = map.terrains.len().max(1);
    let mut rows: [Vec<u8>; 4] = [vec![], vec![], vec![], vec![]];
    for i in 0..n {
        let t = map.terrains.get(i).and_then(|id| content.terrains.get(id));
        let (c1, c2, params, side) = match t {
            Some(t) => (
                hex_bytes(&t.color),
                hex_bytes(if t.color2.is_empty() { &t.color } else { &t.color2 }),
                [pattern_id(&t.pattern), (t.priority.clamp(-128, 127) + 128) as u8, (t.walk == "water") as u8 * 255, 255],
                hex_bytes(if t.side.is_empty() { SIDE } else { &t.side }),
            ),
            None => ([255, 0, 255, 255], [128, 0, 128, 255], [0, 128, 0, 255], hex_bytes(SIDE)),
        };
        rows[0].extend_from_slice(&c1);
        rows[1].extend_from_slice(&c2);
        rows[2].extend_from_slice(&params);
        rows[3].extend_from_slice(&side);
    }
    (rows.concat(), n as u32)
}

pub fn ground(map: &WorldMap, content: &Content, level_height: f32) -> Ground {
    let (tops, cliffs, grass) = ground_region(map, content, level_height, whole(map));
    let (palette, palette_width) = palette(map, content);
    Ground { tops, cliffs, grass, cells: cells_texture(map), palette, palette_width }
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

/// How far a cliff's top edge is bevelled, in and down, in cells.
pub const CHAMFER: f32 = 0.22;

/// Where the faces around the map go down to.
const SKIRT: f32 = -0.6;

/// What's lower around a cell: its sides (north, south, west, east) where the ground steps
/// down or the map ends, and its corners (north-west, north-east, south-west, south-east)
/// where only the cell across the corner is lower, which are cut off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Drops {
    sides: [bool; 4],
    cuts: [bool; 4],
}

impl Drops {
    fn of(level: &dyn Fn(i32, i32) -> Option<u8>, x: i32, z: i32) -> Drops {
        let own = level(x, z).unwrap_or(0);
        let low = |dx: i32, dz: i32| level(x + dx, z + dz).map(|l| l < own).unwrap_or(true);
        let sides = [low(0, -1), low(0, 1), low(-1, 0), low(1, 0)];
        let [n, s, w, e] = sides;
        let cuts = [!n && !w && low(-1, -1), !n && !e && low(1, -1), !s && !w && low(-1, 1), !s && !e && low(1, 1)];
        Drops { sides, cuts }
    }

    fn none(&self) -> bool {
        !self.sides.iter().chain(&self.cuts).any(|&b| b)
    }

    /// How far below the cell's level its top is at a point of it (u, v from its
    /// north-west corner, 0 to 1): CHAMFER at a side that steps down, rising to nothing
    /// CHAMFER in; at a cut corner, across it. Where two sides step down, they meet in a
    /// mitre.
    fn bevel(&self, u: f32, v: f32) -> f32 {
        let c = CHAMFER;
        let [n, s, w, e] = self.sides;
        let [nw, ne, sw, se] = self.cuts;
        let (iu, iv) = (1.0 - u, 1.0 - v);
        let mut k = 0.0f32;
        for (on, d) in [(n, v), (s, iv), (w, u), (e, iu), (nw, u + v), (ne, iu + v), (sw, u + iv), (se, iu + iv)] {
            if on {
                k = k.max(c - d);
            }
        }
        k.clamp(0.0, c)
    }
}

/// A flat piece of a cell's top (a triangle or a planar quad), facing up; its vertices'
/// alpha says how far down the bevel they are (0 on top, 1 at the bottom), for the rock.
fn top_piece(m: &mut Mesh, ps: &[Vec3], uv: [f32; 2], y: f32) {
    let mut n = (ps[1] - ps[0]).cross(ps[2] - ps[0]).normalize_or(Vec3::Y);
    if n.y < 0.0 {
        n = -n;
    }
    let i = m.vertices.len() as u32;
    for p in ps {
        let rock = ((y - p.y) / CHAMFER).clamp(0.0, 1.0);
        m.vertices.push(Vertex { pos: p.to_array(), normal: n.to_array(), uv, color: [1.0, 1.0, 1.0, rock] });
    }
    for k in 1..ps.len() as u32 - 1 {
        m.indices.extend_from_slice(&[i, i + k, i + k + 1]);
    }
}

/// A cell's top: one quad when nothing around it is lower, else a 3 x 3 grid whose rim is
/// bevelled. A corner piece where two bevels meet folds along the diagonal through the
/// cell's corner (the mitre), a cut corner's along the other one.
fn cell_top(m: &mut Mesh, fx: f32, fz: f32, y: f32, d: &Drops) {
    let uv = [fx + 0.5, fz + 0.5];
    if d.none() {
        let q = [Vec3::new(fx, y, fz), Vec3::new(fx, y, fz + 1.0), Vec3::new(fx + 1.0, y, fz + 1.0), Vec3::new(fx + 1.0, y, fz)];
        top_piece(m, &q, uv, y);
        return;
    }
    let c = CHAMFER;
    let at = [0.0, c, 1.0 - c, 1.0];
    let p = |u: f32, v: f32| Vec3::new(fx + u, y - d.bevel(u, v), fz + v);
    for j in 0..3 {
        for i in 0..3 {
            let (u0, u1, v0, v1) = (at[i], at[i + 1], at[j], at[j + 1]);
            let q = [p(u0, v0), p(u0, v1), p(u1, v1), p(u1, v0)];
            // a corner piece: the corner's index in Drops, the two sides meeting there,
            // and whether the diagonal through the cell's corner is q0-q2 (else q1-q3)
            let corner = match (i, j) {
                (0, 0) => Some((0, 0, 2, true)),
                (2, 0) => Some((1, 0, 3, false)),
                (0, 2) => Some((2, 1, 2, false)),
                (2, 2) => Some((3, 1, 3, true)),
                _ => None,
            };
            let fold = corner.and_then(|(k, a, b, through_02)| {
                if d.sides[a] && d.sides[b] {
                    Some(through_02)
                } else if d.cuts[k] {
                    Some(!through_02)
                } else {
                    None
                }
            });
            match fold {
                Some(true) => {
                    top_piece(m, &[q[0], q[1], q[2]], uv, y);
                    top_piece(m, &[q[0], q[2], q[3]], uv, y);
                }
                Some(false) => {
                    top_piece(m, &[q[0], q[1], q[3]], uv, y);
                    top_piece(m, &[q[1], q[2], q[3]], uv, y);
                }
                None => top_piece(m, &q, uv, y),
            }
        }
    }
}

/// How far a cliff face bulges out at most, in cells (it dents in a little too).
const RELIEF: f32 = 0.13;

/// Smooth noise in space, 0 to 1: values on a lattice `scale` apart, eased between.
fn noise3(p: Vec3, scale: f32, seed: i32) -> f32 {
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

/// The cell tops, the cliff faces and the tall grass of a region's cells. A side that steps
/// down (or ends the map) is bevelled at the top and has a face from under the bevel down
/// to the ground across, which may be bevelled too; the faces look at the neighbours, so
/// the whole grid is read.
pub fn ground_region(map: &WorldMap, content: &Content, level_height: f32, region: [i32; 4]) -> (Mesh, Mesh, Mesh) {
    let mut tops = Mesh::default();
    let mut cliffs = Mesh::default();
    let mut grass = Mesh::default();
    let level = |x: i32, z: i32| map.level_at(x, z);
    let terrain = |x: i32, z: i32| map.terrain_at(x, z).and_then(|id| content.terrains.get(id));
    // a cell's top at a point on it (world x, z); off the map, the skirt's foot
    let surface = |x: i32, z: i32, px: f32, pz: f32| match level(x, z) {
        Some(l) => l as f32 * level_height - Drops::of(&level, x, z).bevel(px - x as f32, pz - z as f32),
        None => SKIRT,
    };
    for (x, z) in region_cells(map, region) {
        let y = level(x, z).unwrap_or(0) as f32 * level_height;
        let (fx, fz) = (x as f32, z as f32);
        let d = Drops::of(&level, x, z);
        cell_top(&mut tops, fx, fz, y, &d);
        let uv = [fx + 0.5, fz + 0.5];
        // a face on each side that steps down, seen from outside: from under the bevel
        // down to the top across (sampled where the bevels over there bend), in rows half a
        // cell high, lumpy inside; its edges stay put where it meets the bevel, the ground
        // and other faces, except where the face goes on unchanged into the next cell's
        let sides = [
            (0, -1, [(fx + 1.0, fz), (fx, fz)]),
            (0, 1, [(fx, fz + 1.0), (fx + 1.0, fz + 1.0)]),
            (-1, 0, [(fx, fz), (fx, fz + 1.0)]),
            (1, 0, [(fx + 1.0, fz + 1.0), (fx + 1.0, fz)]),
        ];
        for (k, (dx, dz, [(ax, az), (bx, bz)])) in sides.into_iter().enumerate() {
            if !d.sides[k] {
                continue;
            }
            let across = level(x + dx, z + dz);
            let top = y - CHAMFER;
            let foot = across.map(|l| l as f32 * level_height).unwrap_or(SKIRT);
            let rows = ((top - foot) / 0.5).ceil().max(1.0) as usize;
            let (ex, ez) = ((bx - ax) as i32, (bz - az) as i32);
            let goes_on = |cx: i32, cz: i32| level(cx, cz) == level(x, z) && level(cx + dx, cz + dz) == across;
            let free = [goes_on(x - ex, z - ez), goes_on(x + ex, z + ez)];
            let out = Vec3::new(dx as f32, 0.0, dz as f32);
            let cols = [0.0, CHAMFER, 0.5, 1.0 - CHAMFER, 1.0];
            let base = cliffs.vertices.len() as u32;
            for (ci, &t) in cols.iter().enumerate() {
                let (px, pz) = (ax + (bx - ax) * t, az + (bz - az) * t);
                let b = surface(x + dx, z + dz, px, pz);
                for r in 0..=rows {
                    let f = r as f32 / rows as f32;
                    let p = Vec3::new(px, b + (top - b) * f, pz);
                    let pinned = r == 0 || r == rows || (ci == 0 && !free[0]) || (ci == cols.len() - 1 && !free[1]);
                    let p = if pinned { p } else { p + out * relief(p) };
                    // darker towards the foot
                    let s = 0.72 + 0.28 * f;
                    cliffs.vertices.push(Vertex { pos: p.to_array(), normal: out.to_array(), uv, color: [s, s, s, 1.0] });
                }
            }
            let n = rows as u32 + 1;
            for c in 0..cols.len() as u32 - 1 {
                for r in 0..rows as u32 {
                    let (a0, a1, b0, b1) = (base + c * n + r, base + c * n + r + 1, base + (c + 1) * n + r, base + (c + 1) * n + r + 1);
                    cliffs.indices.extend_from_slice(&[a1, a0, b0, a1, b0, b1]);
                }
            }
        }
        // tall grass: tufts of blades, standing on the top (on the bevel too)
        if let Some(t) = terrain(x, z).filter(|t| t.pattern == "tall") {
            let (c1, c2) = (color(&t.color), color(&t.color2));
            for k in 0..9 {
                let (u, v) = (0.12 + 0.76 * hash(x, z, k * 3), 0.12 + 0.76 * hash(x, z, k * 3 + 1));
                let tall = 0.42 + 0.22 * hash(x, z, k * 3 + 2);
                let c = if k % 3 == 0 { c2 } else { c1 };
                let mut blade = Mesh::default();
                shape_round(&mut blade, Vec3::new(0.2, tall, 0.2), c, 0.0, 4);
                grass.append(&blade, placement(Vec3::new(fx + u, y - d.bevel(u, v), fz + v), hash(x, z, k) * 90.0, 1.0));
            }
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
        let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
        let pos = p.pos();
        let at = Vec3::new(pos[0] as f32, p.level as f32 * level_height, pos[1] as f32);
        let scene = cache.entry(prop.id.as_str()).or_insert_with(|| prop_scene(prop, models));
        out.append(scene, placement(at, p.turn as f32, scale));
    }
    out
}

/// The tree areas' trees.
pub fn tree_mesh(map: &WorldMap, content: &Content, level_height: f32, models: &Models) -> Scene {
    trees_from(&trees(map, level_height), content, models, whole(map))
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
    let mut z = z0;
    while z < z1 {
        let zb = (z + FAR_STEP).min(z1);
        let mut x = x0;
        while x < x1 {
            let xb = (x + FAR_STEP).min(x1);
            let i = tops.vertices.len() as u32;
            for (px, pz) in [(x, z), (x, zb), (xb, zb), (xb, z)] {
                tops.vertices.push(Vertex { pos: [px as f32, level(px, pz), pz as f32], normal: [0.0, 1.0, 0.0], uv: [px as f32 + 0.01, pz as f32 + 0.01], color: [1.0, 1.0, 1.0, 0.0] });
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
                let y = sum / n as f32 * level_height + 1.7;
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
        let y1 = y0 + b.floors.len().max(1) as f32 * level_height + 0.5;
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

    #[test]
    fn tops_on_one_level_meet_without_steps() {
        for z in 0..7 {
            for x in 0..8 {
                let Some(l) = level(x, z) else { continue };
                let a = Drops::of(&level, x, z);
                for (dx, dz) in [(1, 0), (0, 1)] {
                    if level(x + dx, z + dz) != Some(l) {
                        continue;
                    }
                    let b = Drops::of(&level, x + dx, z + dz);
                    for t in [0.0, CHAMFER * 0.5, CHAMFER, 0.5, 1.0 - CHAMFER, 1.0] {
                        // the shared edge, in each cell's own (u, v)
                        let (ua, va, ub, vb) = if dx == 1 { (1.0, t, 0.0, t) } else { (t, 1.0, t, 0.0) };
                        assert!((a.bevel(ua, va) - b.bevel(ub, vb)).abs() < 1e-6, "{x},{z} and the cell {dx},{dz} on at {t}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_side_that_steps_down_is_bevelled_all_along() {
        // the plateau's north-west cell: north and west step down, a mitre at the corner
        let d = Drops::of(&level, 1, 1);
        assert_eq!(d.sides, [true, false, true, false]);
        for t in [0.0, 0.3, 1.0] {
            assert!((d.bevel(t, 0.0) - CHAMFER).abs() < 1e-6 && (d.bevel(0.0, t) - CHAMFER).abs() < 1e-6);
        }
        assert!(d.bevel(0.5, 0.5).abs() < 1e-6);
        // north-west of the hole: the north side steps down, the corner towards the hole is cut
        let d = Drops::of(&level, 2, 1);
        assert_eq!((d.sides, d.cuts), ([true, false, false, false], [false, false, false, true]));
        let d = Drops::of(&level, 2, 3);
        assert_eq!(d.cuts, [false, true, false, false], "the hole is north-east of 2,3");
        assert!((d.bevel(1.0, 0.0) - CHAMFER).abs() < 1e-6 && d.bevel(1.0 - CHAMFER, 0.0).abs() < 1e-6);
        // off the map counts as lower
        assert_eq!(Drops::of(&level, 6, 0).sides, [true, false, false, true]);
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
        let (tops, cliffs, _) = ground_region(&map, &Content::default(), 2.0, whole(&map));
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
        // at the plateau's corner (x = 1) the column stays on the edge, where the west face
        // turns away
        let corner: Vec<[f32; 3]> = cliffs.vertices.iter().filter(near_north).filter(|v| v.pos[0] == 1.0).map(|v| v.pos).collect();
        assert!(!corner.is_empty() && corner.iter().all(|p| p[2] == 1.0), "{corner:?}");
        // the tops' rock share: 0 inside, 1 at a bevel's foot
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
}
