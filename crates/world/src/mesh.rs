//! A map's geometry, for the renderer: the cells' tops (coloured by the terrain shader from
//! a texture of the cells, so painting only changes that texture), the cliff faces and
//! tall grass (vertex colours), and the props and trees (their stand-in models, made from
//! the simple shapes their records list).

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
    /// cell tops, drawn by the terrain shader
    pub tops: Mesh,
    /// cliff faces and tall grass
    pub solid: Mesh,
    /// width x height texels: terrain index, level, 0, 255
    pub cells: Vec<u8>,
    /// terrains x 3 rows: colour, second colour (sRGB), [pattern, priority + 128, water, 255]
    pub palette: Vec<u8>,
    pub palette_width: u32,
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
    let mut rows: [Vec<u8>; 3] = [vec![], vec![], vec![]];
    for i in 0..n {
        let t = map.terrains.get(i).and_then(|id| content.terrains.get(id));
        let (c1, c2, params) = match t {
            Some(t) => (
                hex_bytes(&t.color),
                hex_bytes(if t.color2.is_empty() { &t.color } else { &t.color2 }),
                [pattern_id(&t.pattern), (t.priority.clamp(-128, 127) + 128) as u8, (t.walk == "water") as u8 * 255, 255],
            ),
            None => ([255, 0, 255, 255], [128, 0, 128, 255], [0, 128, 0, 255]),
        };
        rows[0].extend_from_slice(&c1);
        rows[1].extend_from_slice(&c2);
        rows[2].extend_from_slice(&params);
    }
    (rows.concat(), n as u32)
}

pub fn ground(map: &WorldMap, content: &Content, level_height: f32) -> Ground {
    let (tops, solid) = ground_region(map, content, level_height, whole(map));
    let (palette, palette_width) = palette(map, content);
    Ground { tops, solid, cells: cells_texture(map), palette, palette_width }
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

/// The cell tops and the cliff faces (and tall grass) of a region's cells; the faces look
/// at the neighbours, so the whole grid is read.
pub fn ground_region(map: &WorldMap, content: &Content, level_height: f32, region: [i32; 4]) -> (Mesh, Mesh) {
    let mut tops = Mesh::default();
    let mut solid = Mesh::default();
    let white = [1.0, 1.0, 1.0, 1.0];
    let terrain = |x: i32, z: i32| map.terrain_at(x, z).and_then(|id| content.terrains.get(id));
    for (x, z) in region_cells(map, region) {
        {
            let level = map.level_at(x, z).unwrap_or(0);
            let y = level as f32 * level_height;
            let (fx, fz) = (x as f32, z as f32);
            let i = tops.vertices.len() as u32;
            for (px, pz) in [(fx, fz), (fx, fz + 1.0), (fx + 1.0, fz + 1.0), (fx + 1.0, fz)] {
                tops.vertices.push(Vertex { pos: [px, y, pz], normal: [0.0, 1.0, 0.0], uv: [fx + 0.5, fz + 0.5], color: white });
            }
            tops.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            let t = terrain(x, z);
            let side = color(t.map(|t| t.side.as_str()).filter(|s| !s.is_empty()).unwrap_or("#8a6a48"));
            let lip = shade(color(t.map(|t| t.color.as_str()).unwrap_or("#6cbb4e")), 0.8);
            // a face on each side whose neighbour is lower (down to it), and around the map
            for (dx, dz, corners) in [
                (0, 1, [(fx, fz + 1.0), (fx + 1.0, fz + 1.0)]),
                (0, -1, [(fx + 1.0, fz), (fx, fz)]),
                (1, 0, [(fx + 1.0, fz + 1.0), (fx + 1.0, fz)]),
                (-1, 0, [(fx, fz), (fx, fz + 1.0)]),
            ] {
                let bottom = match map.level_at(x + dx, z + dz) {
                    Some(l) if l >= level => continue,
                    Some(l) => l as f32 * level_height,
                    None => -0.6,
                };
                let [(ax, az), (bx, bz)] = corners;
                // bands, like layers of rock, with a strip of the top's colour at the lip
                let mut top = y;
                let mut band = 0;
                while top > bottom + 0.001 {
                    let size = if band == 0 { 0.14 } else { 0.5 };
                    let low = (top - size).max(bottom);
                    let k = if band == 0 { 1.0 } else { 0.82 + 0.16 * hash(x * 2 + dx, z * 2 + dz, band) };
                    let base = if band == 0 { lip } else { side };
                    let depth = 1.0 - 0.25 * ((y - low) / (level_height * 1.5)).clamp(0.0, 1.0);
                    solid.quad([Vec3::new(ax, top, az), Vec3::new(ax, low, az), Vec3::new(bx, low, bz), Vec3::new(bx, top, bz)], shade(base, k * depth));
                    top = low;
                    band += 1;
                }
            }
            // tall grass: tufts of blades
            if t.map(|t| t.pattern == "tall").unwrap_or(false) {
                let (c1, c2) = (color(&t.unwrap().color), color(&t.unwrap().color2));
                for k in 0..9 {
                    let px = fx + 0.12 + 0.76 * hash(x, z, k * 3);
                    let pz = fz + 0.12 + 0.76 * hash(x, z, k * 3 + 1);
                    let tall = 0.42 + 0.22 * hash(x, z, k * 3 + 2);
                    let c = if k % 3 == 0 { c2 } else { c1 };
                    let mut blade = Mesh::default();
                    shape_round(&mut blade, Vec3::new(0.2, tall, 0.2), c, 0.0, 4);
                    solid.append(&blade, placement(Vec3::new(px, y, pz), hash(x, z, k) * 90.0, 1.0));
                }
            }
        }
    }
    (tops, solid)
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
        let at = Vec3::new(p.at[0] as f32, p.level as f32 * level_height, p.at[1] as f32);
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
    let white = [1.0, 1.0, 1.0, 1.0];
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
                tops.vertices.push(Vertex { pos: [px as f32, level(px, pz), pz as f32], normal: [0.0, 1.0, 0.0], uv: [px as f32 + 0.01, pz as f32 + 0.01], color: white });
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
