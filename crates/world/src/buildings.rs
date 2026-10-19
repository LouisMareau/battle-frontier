//! Buildings' geometry: on each floor, each cell's flooring on a plain base, so nothing shows
//! between its planks (open where stairs come up and over its holes, the holes' railings put
//! together from their style's pieces: `railings`),
//! walls along the outline (each segment's outside and inside in their styles, patterned,
//! with holes where doors and windows are), the doors, windows and what stands inside;
//! pillars as high as the building; a gabled roof on a rectangle, a flat one with a parapet
//! on any other shape; decorations on the walls. A building can be cut away above a floor,
//! for editing it or walking in it, and there the walls facing the camera cut down to a stub
//! (they'd hide the room behind them).

use content::{segment_ends, side_turn, Anchor, Building, Content};
use glam::{Mat4, Vec3};

use crate::mesh::{placement, prop_scene, Mesh, Models, Scene};
use crate::{floors, railings};
use crate::{color, WorldMap};

/// How thick walls are, inwards from the outline.
pub const WALL: f32 = 0.12;
/// Doors and windows leave this much wall on each side of their span.
const MARGIN: f32 = 0.1;
/// How high a wall cut down stays: its foot, showing where the room ends and where its doors
/// are.
pub const STUB: f32 = 0.2;
/// How far a wall's outside must face the camera (the cosine between its way out and the
/// camera's, on the ground) to be cut down while the walker is inside, and how little for one
/// cut down to rise back: between the two it stays as it is, so it doesn't flicker as the
/// camera turns past.
const FACING_CUT: f32 = 0.35;
const FACING_BACK: f32 = 0.15;

/// Some of the sides of a building's outline (north, south, west, east).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sides(u8);

impl Sides {
    pub const NONE: Sides = Sides(0);
    const NAMES: [&'static str; 4] = ["n", "s", "w", "e"];

    /// One side: "n", "s", "w" or "e".
    pub fn of(side: &str) -> Sides {
        Sides::NAMES.iter().position(|n| *n == side).map_or(Sides::NONE, |i| Sides(1 << i))
    }

    pub fn has(self, side: &str) -> bool {
        self.0 & Sides::of(side).0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn or(self, other: Sides) -> Sides {
        Sides(self.0 | other.0)
    }

    pub fn minus(self, other: Sides) -> Sides {
        Sides(self.0 & !other.0)
    }

    /// Their letters, north first ("se"), for the checks.
    pub fn letters(self) -> String {
        Sides::NAMES.iter().filter(|n| self.has(n)).copied().collect()
    }
}

/// The sides whose walls face a camera turned `yaw` degrees (0: south of what it looks at,
/// the map editor's way) enough to hide the room behind them: those cut down while the walker
/// is inside. `now`: the ones cut down already.
pub fn facing(yaw: f32, now: Sides) -> Sides {
    let (s, c) = yaw.to_radians().sin_cos();
    let mut out = Sides::NONE;
    for (side, dot) in [("n", -c), ("s", c), ("w", -s), ("e", s)] {
        if dot > if now.has(side) { FACING_BACK } else { FACING_CUT } {
            out = out.or(Sides::of(side));
        }
    }
    out
}

fn shade(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

fn hash(a: i32, b: i32, k: i32) -> f32 {
    let mut h = (a as u32).wrapping_mul(374761393) ^ (b as u32).wrapping_mul(668265263) ^ (k as u32).wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// A quad turned to face `out` (whatever order the corners come in).
fn quad_facing(m: &mut Mesh, c: [Vec3; 4], out: Vec3, color: [f32; 4]) {
    let n = (c[1] - c[0]).cross(c[2] - c[0]);
    if n.dot(out) < 0.0 {
        m.quad([c[3], c[2], c[1], c[0]], color);
    } else {
        m.quad(c, color);
    }
}

fn tri_facing(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, out: Vec3, color: [f32; 4]) {
    if (b - a).cross(c - a).dot(out) < 0.0 {
        m.tri(a, c, b, color);
    } else {
        m.tri(a, b, c, color);
    }
}

/// A box between two corners.
pub fn aabb(m: &mut Mesh, lo: Vec3, hi: Vec3, c: [f32; 4]) {
    aabb_by(m, lo, hi, &|_| c);
}

/// A box between two corners, each face the colour `color` gives for its outward normal
/// (the bottom a shade darker).
fn aabb_by(m: &mut Mesh, lo: Vec3, hi: Vec3, color: &dyn Fn(Vec3) -> [f32; 4]) {
    let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    m.quad([p(lo.x, hi.y, lo.z), p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, hi.y, lo.z)], color(Vec3::Y));
    m.quad([p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)], shade(color(Vec3::NEG_Y), 0.7));
    m.quad([p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(hi.x, lo.y, hi.z), p(hi.x, hi.y, hi.z)], color(Vec3::Z));
    m.quad([p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, hi.y, lo.z)], color(Vec3::NEG_Z));
    m.quad([p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)], color(Vec3::X));
    m.quad([p(lo.x, hi.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z), p(lo.x, hi.y, hi.z)], color(Vec3::NEG_X));
}

/// A wall line: which side it faces, where it is, and how to go from (along, height) to
/// the world.
#[derive(Clone, Copy)]
struct Line<'a> {
    side: &'a str,
    /// the outline's coordinate across the wall (world)
    at: f32,
}

impl Line<'_> {
    fn out(&self) -> Vec3 {
        match self.side {
            "n" => Vec3::NEG_Z,
            "s" => Vec3::Z,
            "w" => Vec3::NEG_X,
            _ => Vec3::X,
        }
    }

    /// A point: `along` the wall (world x for north/south walls, z for east/west), `y`,
    /// `inward` from the outline.
    fn point(&self, along: f32, y: f32, inward: f32) -> Vec3 {
        let across = self.at - self.out().x * inward - self.out().z * inward;
        match self.side {
            "n" | "s" => Vec3::new(along, y, across),
            _ => Vec3::new(across, y, along),
        }
    }

    /// A box of wall from a0 to a1 along, y0 to y1 up: its inside face in `inside`, the rest
    /// in `outside`.
    #[allow(clippy::too_many_arguments)]
    fn piece(&self, m: &mut Mesh, a0: f32, a1: f32, y0: f32, y1: f32, outside: [f32; 4], inside: [f32; 4]) {
        let (p, q) = (self.point(a0, y0, 0.0), self.point(a1, y1, WALL));
        let out = self.out();
        aabb_by(m, p.min(q), p.max(q), &|n: Vec3| if n.dot(out) < -0.5 { inside } else { outside });
    }

    /// A flat rectangle on the wall's outside face, a hair out (`inner`: on its inside face,
    /// a hair in).
    #[allow(clippy::too_many_arguments)]
    fn face(&self, m: &mut Mesh, a0: f32, a1: f32, y0: f32, y1: f32, c: [f32; 4], inner: bool) {
        let (e, facing) = if inner { (WALL + 0.004, -self.out()) } else { (-0.004, self.out()) };
        quad_facing(m, [self.point(a0, y1, e), self.point(a0, y0, e), self.point(a1, y0, e), self.point(a1, y1, e)], facing, c);
    }
}

/// One face of a piece of wall (the outside, or with `inner` the inside), in the wall's
/// pattern; (a0, a1) along, (v0, v1) from the floor at `floor_y`, the floor `height` high.
/// Patterns line up from piece to piece (they're laid from fixed points).
#[allow(clippy::too_many_arguments)]
fn pattern(m: &mut Mesh, line: Line, kind: &str, main: [f32; 4], trim: [f32; 4], a0: f32, a1: f32, v0: f32, v1: f32, floor_y: f32, height: f32, inner: bool) {
    let tile = |m: &mut Mesh, b0: f32, b1: f32, w0: f32, w1: f32, c: [f32; 4]| {
        let (b0, b1, w0, w1) = (b0.max(a0), b1.min(a1), w0.max(v0), w1.min(v1));
        if b1 - b0 > 0.02 && w1 - w0 > 0.02 {
            line.face(m, b0, b1, floor_y + w0, floor_y + w1, c, inner);
        }
    };
    let rows = |h: f32| ((v0 / h).floor() as i32, (v1 / h).ceil() as i32);
    match kind {
        "bricks" => {
            let (h, len, gap) = (0.2, 0.5, 0.03);
            let (r0, r1) = rows(h);
            for r in r0..r1 {
                let off = if r % 2 == 0 { 0.0 } else { len / 2.0 };
                let c0 = ((a0 - off) / len).floor() as i32;
                let c1 = ((a1 - off) / len).ceil() as i32;
                for c in c0..c1 {
                    let b = off + c as f32 * len;
                    let k = 0.86 + 0.22 * hash(r, c, 1);
                    tile(m, b + gap / 2.0, b + len - gap / 2.0, r as f32 * h + gap / 2.0, (r + 1) as f32 * h - gap / 2.0, shade(main, k));
                }
            }
        }
        "stone" => {
            let (h, gap) = (0.4, 0.045);
            let (r0, r1) = rows(h);
            for r in r0..r1 {
                for c in a0.floor() as i32..a1.ceil() as i32 {
                    let split = 0.3 + 0.4 * hash(r, c, 7);
                    let cuts: Vec<f32> = if hash(r, c, 3) > 0.45 { vec![0.0, split, 1.0] } else { vec![0.0, 1.0] };
                    for (i, w) in cuts.windows(2).enumerate() {
                        let k = 0.82 + 0.26 * hash(r * 3 + i as i32, c, 5);
                        tile(m, c as f32 + w[0] + gap / 2.0, c as f32 + w[1] - gap / 2.0, r as f32 * h + gap / 2.0, (r + 1) as f32 * h - gap / 2.0, shade(main, k));
                    }
                }
            }
        }
        "planks" => {
            let (h, gap) = (0.25, 0.025);
            let (r0, r1) = rows(h);
            for r in r0..r1 {
                let k = 0.88 + 0.18 * hash(r, 0, 11);
                tile(m, a0, a1, r as f32 * h + gap / 2.0, (r + 1) as f32 * h - gap / 2.0, shade(main, k));
            }
        }
        "logs" => {
            let (h, gap) = (0.32, 0.02);
            let (r0, r1) = rows(h);
            for r in r0..r1 {
                let k = 0.9 + 0.12 * hash(r, 0, 13);
                let (y0, y1) = (r as f32 * h + gap / 2.0, (r + 1) as f32 * h - gap / 2.0);
                let mid = y0 + (y1 - y0) * 0.45;
                tile(m, a0, a1, mid, y1, shade(main, k));
                tile(m, a0, a1, y0, mid, shade(main, k * 0.78));
            }
        }
        _ => {
            // plaster: a base and a band under the next floor, in the trim colour
            tile(m, a0, a1, 0.0, 0.16, trim);
            tile(m, a0, a1, height - 0.08, height, trim);
        }
    }
}

/// A wall prop's look: its colours, its pattern, toon or not.
struct Look {
    main: [f32; 4],
    trim: [f32; 4],
    kind: String,
    /// the wall's own colour (its inside face and edges): the main one for plaster
    body: [f32; 4],
    toon: bool,
}

fn look_of(content: &Content, id: &str) -> Look {
    let wall = content.props.get(id);
    let main = color(wall.map(|w| w.color(0)).unwrap_or("#efe6d2"));
    let trim = color(wall.map(|w| w.color(1)).unwrap_or("#b89f7e"));
    let kind = wall.map(|w| w.pattern.clone()).unwrap_or_else(|| "plaster".into());
    let body = if matches!(kind.as_str(), "plaster" | "") { main } else { trim };
    Look { main, trim, kind, body, toon: wall.map(|w| w.toon).unwrap_or(false) }
}

/// How high a railing round a hole in a floor is.
const RAIL: f32 = 0.9;

/// What a building looks like while it's edited or walked in: floors above `top` cut away, no
/// roof; on floor `top`, the walls on `low`'s sides cut down to a stub (those facing the
/// camera, while the walker is inside).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub top: usize,
    pub low: Sides,
}

pub fn building(b: &Building, content: &Content, level_height: f32, cut: Option<Cut>, models: &Models) -> Scene {
    let shown = match cut {
        Some(c) => (c.top + 1).min(b.floors.len()),
        None => b.floors.len(),
    };
    build(b, content, level_height, 0..shown, cut.is_none(), true, cut.map(|c| (c.top, c.low)), models)
}

/// What a cut takes away: the floors above `top`, the roof, and on floor `top` what's above
/// the stub of the walls on `low`'s sides (what fades out when the player walks in, and back
/// when they leave).
pub fn building_above(b: &Building, content: &Content, level_height: f32, top: usize, low: Sides, models: &Models) -> Scene {
    let mut out = build(b, content, level_height, (top + 1).min(b.floors.len())..b.floors.len(), true, false, None, models);
    out.append(&walls_above_stub(b, content, level_height, top, low, models), Mat4::IDENTITY);
    out
}

/// What's above the stub of floor `floor`'s walls on these sides, with their doors, windows
/// and what hangs on them: what fades as they're cut down, or rise back as the camera turns.
pub fn walls_above_stub(b: &Building, content: &Content, level_height: f32, floor: usize, sides: Sides, models: &Models) -> Scene {
    let mut m = Mesh::default();
    let mut textured = Scene::default();
    if floor < b.floors.len() && !sides.is_empty() {
        floor_walls(&mut m, &mut textured, b, content, level_height, floor, WallPart::Upper(sides), models);
    }
    textured.plain = m;
    textured
}

/// A building's floors in `floors`, its roof when `with_roof`, and when `grounds` what
/// stands outside it and its pillars (as high as the floors shown); `low`: a floor whose walls
/// on these sides are cut down to a stub.
#[allow(clippy::too_many_arguments)]
fn build(b: &Building, content: &Content, level_height: f32, floors: std::ops::Range<usize>, with_roof: bool, grounds: bool, low: Option<(usize, Sides)>, models: &Models) -> Scene {
    let mut m = Mesh::default();
    // (its floors a floor high each, on its level of ground)
    let lh = crate::floor_height(level_height);
    let base = b.level as f32 * level_height;
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    let looks = wall_looks(b, content);
    let (main, trim) = (looks[0].1.main, looks[0].1.trim);
    let wall_toon = looks[0].1.toon;
    let shown = floors.end;
    let model = |id: &str| content.props.get(id).map(|p| prop_scene(p, models));
    let stairs = |id: &str| content.props.get(id).filter(|p| p.category == "stairs").map(|p| p.footprint);
    // a low rim around an opening in a floor or the roof (where stairs come up)
    let rim = |m: &mut Mesh, holes: &[(i32, i32)], y: f32, color: [f32; 4]| {
        for &(x, z) in holes {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if holes.contains(&(x + dx, z + dz)) {
                    continue;
                }
                let (cx, cz) = (ox + x as f32, oz + z as f32);
                let (lo, hi) = match (dx, dz) {
                    (1, 0) => (Vec3::new(cx + 0.96, y, cz), Vec3::new(cx + 1.0, y + 0.12, cz + 1.0)),
                    (-1, 0) => (Vec3::new(cx, y, cz), Vec3::new(cx + 0.04, y + 0.12, cz + 1.0)),
                    (0, 1) => (Vec3::new(cx, y, cz + 0.96), Vec3::new(cx + 1.0, y + 0.12, cz + 1.0)),
                    _ => (Vec3::new(cx, y, cz), Vec3::new(cx + 1.0, y + 0.12, cz + 0.04)),
                };
                aabb(m, lo, hi, color);
            }
        }
    };
    // a plain railing along edges whose style has no pieces: a rail on top, one low,
    // balusters (thick across the edge only; along it the rails end a hair inside the edge's
    // ends: inside the outside wall there, never through it nor flush with its face)
    let plain_railing = |m: &mut Mesh, edges: &[railings::Edge], y: f32, color: [f32; 4]| {
        for e in edges {
            let (a, c) = (Vec3::new(ox + e.a.0 as f32, y, oz + e.a.1 as f32), Vec3::new(ox + e.b.0 as f32, y, oz + e.b.1 as f32));
            let t = if e.a.1 == e.b.1 { Vec3::new(0.0, 0.0, 0.035) } else { Vec3::new(0.035, 0.0, 0.0) };
            let inset = (c - a).normalize() * 0.01;
            let (a, c) = (a + inset, c - inset);
            aabb(m, a.min(c) - t + Vec3::Y * (RAIL - 0.07), a.max(c) + t + Vec3::Y * RAIL, color);
            aabb(m, a.min(c) - t + Vec3::Y * 0.1, a.max(c) + t + Vec3::Y * 0.15, color);
            for k in 0..4 {
                let p = a + (c - a) * (0.125 + 0.25 * k as f32);
                aabb(m, p - Vec3::new(0.022, 0.0, 0.022), p + Vec3::new(0.022, RAIL - 0.07, 0.022), color);
            }
        }
    };
    // a point on the outline: railings that end there end against the outside wall
    let at_wall = |(px, pz): (i32, i32)| {
        let inside = [(px - 1, pz - 1), (px, pz - 1), (px - 1, pz), (px, pz)].iter().filter(|c| b.has(c.0, c.1)).count();
        inside > 0 && inside < 4
    };
    // railing pieces' models, made once
    let mut pieces: Vec<(String, Option<Scene>)> = vec![];
    // the models' textured parts (the plain ones go into m)
    let mut textured = Scene::default();

    for f in floors {
        let floor = &b.floors[f];
        let y0 = base + f as f32 * lh;
        // the floor, each cell its flooring on a plain base (what shows between its planks or
        // slabs, never the ground or the room below); open where stairs come up from below,
        // and over its holes (never the ground floor's), a railing round those
        let holes = b.stairwells(f, &stairs);
        let open: Vec<(i32, i32)> = if f > 0 { floor.holes.iter().map(|h| (h.cell[0], h.cell[1])).filter(|c| !holes.contains(c)).collect() } else { vec![] };
        let foot = y0 + if f == 0 { 0.02 } else { 0.0 };
        // each cell's tile as it lies (a set's laid to fit its neighbours: `floors`), turned
        let floored: Vec<(i32, i32)> = b.cells().into_iter().filter(|c| !holes.contains(c) && !open.contains(c)).collect();
        let laid = floors::floor_laid(b, f, content, &floored);
        let mut tiles: Vec<(String, Option<Flooring>)> = vec![];
        for &(x, z) in &floored {
            let Some(l) = laid.get(&(x, z)) else { continue };
            let k = match tiles.iter().position(|(t, _)| *t == l.tile) {
                Some(k) => k,
                None => {
                    tiles.push((l.tile.clone(), model(&l.tile).map(flooring)));
                    tiles.len() - 1
                }
            };
            if let Some(tile) = &tiles[k].1 {
                let (x0, z0) = (ox + x as f32, oz + z as f32);
                let from = m.vertices.len();
                m.quad([Vec3::new(x0, foot, z0), Vec3::new(x0, foot, z0 + 1.0), Vec3::new(x0 + 1.0, foot, z0 + 1.0), Vec3::new(x0 + 1.0, foot, z0)], tile.under);
                toon_from(&mut m, from, tile.toon);
                put(&mut m, &mut textured, &tile.scene, placement(Vec3::new(x0 + 0.5, foot, z0 + 0.5), l.turn as f32 * 90.0, 1.0));
            }
        }
        // the holes' railings: along their edges where the floor goes on (not where another
        // hole or the stairs' opening is, nor along the outside wall), of each hole's style
        let mut edges = vec![];
        for h in floor.holes.iter().filter(|h| f > 0 && !h.railing.is_empty()) {
            let (x, z) = (h.cell[0], h.cell[1]);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (x + dx, z + dz);
                if open.contains(&n) || holes.contains(&n) || !b.has(n.0, n.1) || holes.contains(&(x, z)) {
                    continue;
                }
                let (a, c) = match (dx, dz) {
                    (1, 0) => ((x + 1, z), (x + 1, z + 1)),
                    (-1, 0) => ((x, z), (x, z + 1)),
                    (0, 1) => ((x, z + 1), (x + 1, z + 1)),
                    _ => ((x, z), (x + 1, z)),
                };
                edges.push(railings::Edge { a, b: c, style: h.railing.clone() });
            }
        }
        let seed = b.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100_0000_01b3)) ^ (f as u64).wrapping_mul(0x9e37_79b9);
        let (puts, plain) = railings::assemble(&edges, content, &at_wall, WALL, seed);
        for r in &puts {
            let k = match pieces.iter().position(|(id, _)| *id == r.prop) {
                Some(k) => k,
                None => {
                    pieces.push((r.prop.clone(), model(&r.prop)));
                    pieces.len() - 1
                }
            };
            if let Some(scene) = &pieces[k].1 {
                put(&mut m, &mut textured, scene, placement(Vec3::new(ox + r.at.x, y0 + r.at.y, oz + r.at.z), r.turn, 1.0));
            }
        }
        let from = m.vertices.len();
        rim(&mut m, &holes, y0, trim);
        plain_railing(&mut m, &plain, y0, trim);
        toon_from(&mut m, from, wall_toon);
        // the walls, with their doors and windows and what hangs on them; on the floor the
        // cut asks, those on its sides cut down to a stub
        let down = low.filter(|&(at, _)| at == f).map_or(Sides::NONE, |(_, sides)| sides);
        floor_walls(&mut m, &mut textured, b, content, level_height, f, WallPart::Whole { low: down }, models);
        // inside
        for p in &floor.props {
            let Some(prop) = content.props.get(&p.prop) else { continue };
            let Some(model) = model(&p.prop) else { continue };
            let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
            let at = Vec3::new(ox + p.pos()[0] as f32, y0 + 0.05, oz + p.pos()[1] as f32);
            put(&mut m, &mut textured, &model, placement(at, p.turn as f32, scale));
        }
    }
    // what stands outside it
    for p in b.outside.iter().filter(|_| grounds) {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let Some(model) = model(&p.prop) else { continue };
        let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
        let at = Vec3::new(ox + p.pos()[0] as f32, base, oz + p.pos()[1] as f32);
        put(&mut m, &mut textured, &model, placement(at, p.turn as f32, scale));
    }
    // pillars, as high as what's shown
    for p in b.pillars.iter().filter(|_| grounds) {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let Some(model) = model(&p.prop) else { continue };
        let stretch = shown as f32 * lh / (prop.height as f32).max(0.1);
        let at = Vec3::new(ox + p.corner[0] as f32, base, oz + p.corner[1] as f32);
        put(&mut m, &mut textured, &model, Mat4::from_translation(at) * Mat4::from_scale(Vec3::new(1.0, stretch, 1.0)));
    }
    if with_roof && !b.floors.is_empty() {
        let top = base + b.floors.len() as f32 * lh;
        let holes = b.stairwells(b.floors.len(), &stairs);
        let from = m.vertices.len();
        roof(&mut m, b, content, top, main, &holes);
        if b.roof_is_flat() {
            rim(&mut m, &holes, top + 0.14, trim);
        }
        toon_from(&mut m, from, content.props.get(&b.roof).map(|p| p.toon).unwrap_or(false));
    }
    textured.plain = m;
    textured
}

/// A model put in: its plain parts into `m`, its textured ones into `textured`.
fn put(m: &mut Mesh, textured: &mut Scene, s: &Scene, t: Mat4) {
    m.append(&s.plain, t);
    let only = Scene { plain: Mesh::default(), textured: s.textured.clone() };
    textured.append(&only, t);
}

/// A flooring's model, and the plain base it lies on, a cell wide at its foot: in its colour
/// seen from above, darker (in the seams between its planks or slabs), toon if it is.
struct Flooring {
    scene: Scene,
    under: [f32; 4],
    toon: bool,
}

/// A flooring whose colours are all in its textures lies on a dark brown.
fn flooring(scene: Scene) -> Flooring {
    let toon = scene.plain.vertices.iter().chain(scene.textured.values().flat_map(|t| t.vertices.iter())).any(|v| v.toon > 0.5);
    let tops: Vec<Vec3> = scene.plain.vertices.iter().filter(|v| v.normal[1] > 0.5).map(|v| Vec3::new(v.color[0], v.color[1], v.color[2])).collect();
    let under = if tops.is_empty() {
        color("#3d3229")
    } else {
        let c = tops.iter().sum::<Vec3>() / tops.len() as f32 * 0.5;
        [c.x, c.y, c.z, 1.0]
    };
    Flooring { scene, under, toon }
}

/// What's been made in `m` from `from` on, toon if `toon` (what's made from the wall and roof
/// props is toon if they are).
fn toon_from(m: &mut Mesh, from: usize, toon: bool) {
    if toon {
        for v in &mut m.vertices[from..] {
            v.toon = 1.0;
        }
    }
}

/// The walls' looks: the building's first, then every other style a face has.
fn wall_looks(b: &Building, content: &Content) -> Vec<(String, Look)> {
    let mut looks = vec![(b.wall.clone(), look_of(content, &b.wall))];
    let others = std::iter::once(&b.inner_wall).chain(b.floors.iter().flat_map(|f| f.walls.iter().chain(&f.inner)).map(|w| &w.wall));
    for id in others.filter(|id| !id.is_empty()) {
        if !looks.iter().any(|(l, _)| l == id) {
            looks.push((id.clone(), look_of(content, id)));
        }
    }
    looks
}

/// The walls' runs: (side, line, first and last cell along) for every straight piece of
/// outline.
fn wall_runs(b: &Building) -> Vec<(&'static str, i32, i32, i32)> {
    let mut lines: Vec<(&str, i32, Vec<i32>)> = vec![];
    for (x, z, side) in b.edges() {
        let (line, along) = match side {
            "n" => (z, x),
            "s" => (z + 1, x),
            "w" => (x, z),
            _ => (x + 1, z),
        };
        match lines.iter_mut().find(|(s, l, _)| *s == side && *l == line) {
            Some((_, _, v)) => v.push(along),
            None => lines.push((side, line, vec![along])),
        }
    }
    let mut runs: Vec<(&str, i32, i32, i32)> = vec![];
    for (side, line, mut v) in lines {
        v.sort();
        let mut start = v[0];
        for i in 1..=v.len() {
            if i == v.len() || v[i] != v[i - 1] + 1 {
                runs.push((side, line, start, v[i - 1]));
                if i < v.len() {
                    start = v[i];
                }
            }
        }
    }
    runs
}

/// What of a floor's walls `floor_walls` makes.
#[derive(Clone, Copy)]
enum WallPart {
    /// all of them, but those on `low`'s sides cut down to a stub (their doors, windows and
    /// what hangs on them gone with the rest)
    Whole { low: Sides },
    /// only what's above the stub of those on these sides, with their doors, windows and what
    /// hangs on them: what fades as they're cut down or rise back
    Upper(Sides),
}

/// Floor `f`'s walls along the outline, with their doors and windows and what hangs on them,
/// as much of them as `part` says.
#[allow(clippy::too_many_arguments)]
fn floor_walls(m: &mut Mesh, textured: &mut Scene, b: &Building, content: &Content, level_height: f32, f: usize, part: WallPart, models: &Models) {
    let floor = &b.floors[f];
    let lh = crate::floor_height(level_height);
    let y0 = b.level as f32 * level_height + f as f32 * lh;
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    let looks = wall_looks(b, content);
    let look_index = |id: &str| looks.iter().position(|(l, _)| l == id).unwrap_or(0);
    let model = |id: &str| content.props.get(id).map(|p| prop_scene(p, models));
    // a side's walls: from how high to how high (from the floor), and whether what's on them
    // is too; none of them at all
    let span = |side: &str| match part {
        WallPart::Whole { low } if low.has(side) => Some((0.0, STUB, false)),
        WallPart::Whole { .. } => Some((0.0, lh, true)),
        WallPart::Upper(sides) if sides.has(side) => Some((STUB, lh, true)),
        WallPart::Upper(_) => None,
    };
    for &(side, l, p0, p1) in &wall_runs(b) {
        let Some((lo, hi, things)) = span(side) else { continue };
        let inside_row = match side {
            "n" | "w" => l,
            _ => l - 1,
        };
        let cell_at = |along: i32| match side {
            "n" | "s" => (along, inside_row),
            _ => (inside_row, along),
        };
        let reflex = |along: i32| {
            let (x, z) = cell_at(along);
            b.has(x, z)
        };
        let line = Line { side, at: if matches!(side, "n" | "s") { oz + l as f32 } else { ox + l as f32 } };
        let origin = if matches!(side, "n" | "s") { ox } else { oz };
        let start = p0 as f32 - if reflex(p0 - 1) { WALL } else { 0.0 };
        let end = (p1 + 1) as f32 + if reflex(p1 + 1) { WALL } else { 0.0 };
        // holes along this run: (from, to, bottom, top), from the floor
        let mut holes: Vec<(f32, f32, f32, f32)> = vec![];
        for o in &floor.openings {
            if o.side != side {
                continue;
            }
            let segs = Building::segments(o);
            let alongs: Vec<i32> = segs.iter().filter(|&&(x, z)| cell_at(if matches!(side, "n" | "s") { x } else { z }) == (x, z)).map(|&(x, z)| if matches!(side, "n" | "s") { x } else { z }).collect();
            if alongs.len() != segs.len() || alongs.iter().any(|&a| a < p0 || a > p1) {
                continue;
            }
            let first = *alongs.iter().min().unwrap() as f32;
            let Some(prop) = content.props.get(&o.prop) else { continue };
            let (bottom, top) = (prop.sill as f32, (prop.sill + prop.height) as f32);
            holes.push((first + MARGIN, first + o.width as f32 - MARGIN, bottom, top.min(lh - 0.05)));
            // the door or window itself, on the outline, facing out
            let centre = first + o.width as f32 / 2.0;
            if let Some(model) = model(&o.prop).filter(|_| things) {
                let at = line.point(origin + centre, y0 + bottom, 0.0);
                put(m, textured, &model, placement(at, side_turn(side) as f32, 1.0));
            }
        }
        holes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cur = start;
        // each segment's styles, outside (the floor's own for it, else the building's) and
        // inside (its own, else the building's inside one, else as its outside), and where
        // along the run either changes
        let styles: Vec<(usize, usize)> = (p0..=p1)
            .map(|k| {
                let (x, z) = cell_at(k);
                (look_index(floor.wall_at(x, z, side, &b.wall)), look_index(b.wall_inside(f, x, z, side)))
            })
            .collect();
        let breaks: Vec<f32> = (p0 + 1..=p1).filter(|&k| styles[(k - p0) as usize] != styles[(k - 1 - p0) as usize]).map(|k| k as f32).collect();
        let solid = |m: &mut Mesh, a0: f32, a1: f32, v0: f32, v1: f32| {
            // no higher (or lower) than this part of the wall goes
            let (v0, v1) = (v0.max(lo), v1.min(hi));
            if a1 - a0 <= 0.001 || v1 - v0 <= 0.001 {
                return;
            }
            let mut cuts = vec![a0];
            cuts.extend(breaks.iter().copied().filter(|&k| k > a0 + 0.001 && k < a1 - 0.001));
            cuts.push(a1);
            for w in cuts.windows(2) {
                let k = ((w[0] + 0.001).floor() as i32).clamp(p0, p1);
                let (outer, inner) = styles[(k - p0) as usize];
                let (outer, inner) = (&looks[outer].1, &looks[inner].1);
                let from = m.vertices.len();
                line.piece(m, origin + w[0], origin + w[1], y0 + v0, y0 + v1, outer.body, inner.body);
                pattern(m, line, &outer.kind, outer.main, outer.trim, origin + w[0], origin + w[1], v0, v1, y0, lh, false);
                toon_from(m, from, outer.toon);
                let from = m.vertices.len();
                pattern(m, line, &inner.kind, inner.main, inner.trim, origin + w[0], origin + w[1], v0, v1, y0, lh, true);
                toon_from(m, from, inner.toon);
            }
        };
        for &(h0, h1, bottom, top) in &holes {
            solid(m, cur, h0, 0.0, lh);
            solid(m, h0, h1, 0.0, bottom);
            solid(m, h0, h1, top, lh);
            cur = h1;
        }
        solid(m, cur, end, 0.0, lh);
    }
    // on the walls: outside facing out, inside (past the wall's thickness) facing in
    for a in b.anchors.iter().filter(|a| a.floor as usize == f && span(&a.side).is_some_and(|(_, _, things)| things)) {
        let Some(model) = model(&a.prop) else { continue };
        put(m, textured, &model, anchor_placement(b, a, level_height));
    }
}

/// Gabled on a rectangle (unless it asks for a flat one), flat with a parapet otherwise;
/// `holes`: cells of a flat roof left open (stairs come up there).
fn roof(m: &mut Mesh, b: &Building, content: &Content, top: f32, wall: [f32; 4], holes: &[(i32, i32)]) {
    let prop = content.props.get(&b.roof);
    let main = color(prop.map(|p| p.color(0)).unwrap_or("#b5473a"));
    let trim = color(prop.map(|p| p.color(1)).unwrap_or("#8a3229"));
    let kind = prop.map(|p| p.pattern.as_str()).unwrap_or("tiles");
    let (w, h) = b.size();
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    if b.roof_is_flat() {
        // flat, with a parapet
        for (x, z) in b.cells().into_iter().filter(|c| !holes.contains(c)) {
            let k = 0.92 + 0.12 * hash(x, z, 17);
            aabb(m, Vec3::new(ox + x as f32, top, oz + z as f32), Vec3::new(ox + x as f32 + 1.0, top + 0.14, oz + z as f32 + 1.0), shade(main, k));
        }
        for (x, z, side) in b.edges() {
            let (p, q) = segment_ends(x, z, side);
            let line = Line { side, at: if matches!(side, "n" | "s") { oz + p[1] as f32 } else { ox + p[0] as f32 } };
            let (a0, a1) = if matches!(side, "n" | "s") { (ox + p[0] as f32, ox + q[0] as f32) } else { (oz + p[1] as f32, oz + q[1] as f32) };
            line.piece(m, a0, a1, top, top + 0.42, trim, trim);
        }
        return;
    }
    // gabled: the ridge along the longer side
    let along_x = w >= h;
    let (len, span) = if along_x { (w as f32, h as f32) } else { (h as f32, w as f32) };
    let o = 0.3;
    let rise = span / 2.0 * 0.8;
    let slope = rise / (span / 2.0);
    // (u along the ridge, v across, y) to the world
    let world = |u: f32, v: f32, y: f32| if along_x { Vec3::new(ox + u, y, oz + v) } else { Vec3::new(ox + v, y, oz + u) };
    let mid = span / 2.0;
    for (sign, out) in [(-1.0f32, if along_x { Vec3::NEG_Z } else { Vec3::NEG_X }), (1.0, if along_x { Vec3::Z } else { Vec3::X })] {
        // from the ridge down to the eave, in strips
        let eave_v = mid + sign * (mid + o);
        let length = ((eave_v - mid).abs().powi(2) + (rise + o * slope).powi(2)).sqrt();
        let strips = (length / if kind == "thatch" { 0.16 } else { 0.26 }).ceil().max(1.0) as i32;
        let point = |t: f32, u: f32| {
            let v = mid + (eave_v - mid) * t;
            let y = top + rise - (rise + o * slope) * t;
            world(u, v, y)
        };
        for s in 0..strips {
            let (t0, t1) = (s as f32 / strips as f32, (s + 1) as f32 / strips as f32);
            let k = match kind {
                "thatch" => 0.9 + 0.14 * hash(s, sign as i32, 21),
                _ => if s % 2 == 0 { 1.0 } else { 0.86 + 0.08 * hash(s, sign as i32, 23) },
            };
            let c = shade(main, k);
            let up = out + Vec3::Y;
            quad_facing(m, [point(t0, -o), point(t1, -o), point(t1, len + o), point(t0, len + o)], up, c);
            // the underside
            let d = Vec3::new(0.0, -0.08, 0.0);
            quad_facing(m, [point(t0, -o) + d, point(t1, -o) + d, point(t1, len + o) + d, point(t0, len + o) + d], -up, shade(trim, 0.7));
        }
        // the eave's edge
        let (e0, e1) = (point(1.0, -o), point(1.0, len + o));
        quad_facing(m, [e0, e0 - Vec3::Y * 0.08, e1 - Vec3::Y * 0.08, e1], out, trim);
    }
    // the ridge cap and the gables
    let (r0, r1) = (world(-o, mid, top + rise), world(len + o, mid, top + rise));
    let cap = Vec3::new(0.06, 0.06, 0.06);
    aabb(m, r0.min(r1) - cap, r0.max(r1) + cap, trim);
    for (u, out) in [(0.0f32, if along_x { Vec3::NEG_X } else { Vec3::NEG_Z }), (len, if along_x { Vec3::X } else { Vec3::Z })] {
        let (a, bb, c) = (world(u, 0.0, top), world(u, span, top), world(u, mid, top + rise));
        tri_facing(m, a, bb, c, out, wall);
        tri_facing(m, a, bb, c, -out, shade(wall, 0.8));
    }
}

/// Where something hung on a wall stands: on the wall's outside face, facing out, or
/// (inside) a wall's thickness in, facing in.
pub fn anchor_placement(b: &Building, a: &Anchor, level_height: f32) -> Mat4 {
    let (p, q) = segment_ends(a.cell[0], a.cell[1], &a.side);
    let y0 = b.level as f32 * level_height + a.floor as f32 * crate::floor_height(level_height);
    let mut mid = Vec3::new(b.at[0] as f32 + (p[0] + q[0]) as f32 / 2.0, y0 + a.height as f32, b.at[1] as f32 + (p[1] + q[1]) as f32 / 2.0);
    let mut turn = side_turn(&a.side) as f32;
    if a.inside {
        mid -= Line { side: &a.side, at: 0.0 }.out() * WALL;
        turn += 180.0;
    }
    placement(mid, turn, 1.0)
}

/// Every building on a map; the one being edited (index, floor) cut away above that floor.
pub fn buildings(map: &WorldMap, content: &Content, level_height: f32, editing: Option<(usize, usize)>, models: &Models) -> Scene {
    let mut out = Scene::default();
    for (i, b) in map.buildings.iter().enumerate() {
        let cut = editing.filter(|(e, _)| *e == i).map(|(_, f)| Cut { top: f, low: Sides::NONE });
        out.append(&building(b, content, level_height, cut, models), Mat4::IDENTITY);
    }
    out
}

/// Where a wall segment is: its middle on the outline at a floor's height, and which way
/// is out.
pub fn segment_point(b: &Building, cell: [i32; 2], side: &str, floor: usize, height: f32, level_height: f32) -> (Vec3, Vec3) {
    let (p, q) = segment_ends(cell[0], cell[1], side);
    let y = b.level as f32 * level_height + floor as f32 * crate::floor_height(level_height) + height;
    let mid = Vec3::new(b.at[0] as f32 + (p[0] + q[0]) as f32 / 2.0, y, b.at[1] as f32 + (p[1] + q[1]) as f32 / 2.0);
    let out = Line { side, at: 0.0 }.out();
    (mid, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Models;
    use content::{Floor, Part, Prop, Space};

    fn content() -> Content {
        let mut c = Content::default();
        let prop = |id: &str, category: &str, pattern: &str, colors: [&str; 2], size: [f64; 3]| Prop {
            id: id.into(),
            name: id.into(),
            category: category.into(),
            footprint: [1, 1],
            height: size[1],
            pattern: pattern.into(),
            parts: colors.iter().map(|&color| Part { shape: "box".into(), size, color: color.into(), ..Default::default() }).collect(),
            ..Default::default()
        };
        c.props.put(prop("wood", "floor", "", ["#c08a55", "#c08a55"], [1.0, 0.05, 1.0]), Space::Content);
        c.props.put(prop("stone", "floor", "", ["#5a7a9a", "#5a7a9a"], [1.0, 0.05, 1.0]), Space::Content);
        c.props.put(prop("plaster", "wall", "plaster", ["#efe6d2", "#b89f7e"], [1.0, 2.0, 0.12]), Space::Content);
        c.props.put(prop("bricks", "wall", "bricks", ["#b5523b", "#d8cbb8"], [1.0, 2.0, 0.12]), Space::Content);
        c
    }

    /// Three cells by two, two floors, plaster and wood; drawn whole but for the roof.
    fn house() -> Building {
        Building { id: "h".into(), cells: vec!["###".into(), "###".into()], wall: "plaster".into(), floors: vec![Floor { tile: "wood".into(), ..Default::default() }; 2], ..Default::default() }
    }

    fn drawn(b: &Building) -> Mesh {
        building(b, &content(), 1.0, Some(Cut { top: 1, low: Sides::NONE }), &Models::new()).plain
    }

    /// The vertices (x, y, z) of about a colour (or of it shaded).
    fn of(m: &Mesh, hex: &str) -> Vec<[f32; 3]> {
        let c = crate::color(hex);
        let near = |v: &[f32; 4]| [1.0, 0.7].iter().any(|k| (0..3).all(|i| (v[i] - c[i] * k).abs() < 0.01));
        m.vertices.iter().filter(|v| near(&v.color)).map(|v| v.pos).collect()
    }

    #[test]
    fn each_cell_its_flooring_and_a_hole_with_a_railing() {
        let mut b = house();
        // another flooring on one cell of the ground floor: there, and only there
        assert!(of(&drawn(&b), "#5a7a9a").is_empty());
        b.floors[0].set_tile(2, 1, "stone");
        let stone = of(&drawn(&b), "#5a7a9a");
        assert!(!stone.is_empty());
        assert!(stone.iter().all(|p| p[0] >= 1.99 && p[0] <= 3.01 && p[2] >= 0.99 && p[2] <= 2.01 && p[1] < 0.2), "{stone:?}");
        // a hole in the floor above: its cell untiled, a railing along its inner edges
        let tiles_up = |m: &Mesh| of(m, "#c08a55").iter().filter(|p| p[1] > 1.9 && p[1] < 2.2).count();
        // the top rail: at its height, round the hole's cell (1, 1)
        let rail_top = |m: &Mesh, y: f32| m.vertices.iter().filter(|v| (v.pos[1] - (y + RAIL)).abs() < 0.01 && (0.95..2.05).contains(&v.pos[0]) && (0.95..2.05).contains(&v.pos[2])).count();
        // balusters along an edge: (x, z) within 0.03 of a point along it, between the rails
        let baluster = |m: &Mesh, x: f32, z: f32| m.vertices.iter().any(|v| (v.pos[0] - x).abs() < 0.03 && (v.pos[2] - z).abs() < 0.03 && v.pos[1] > 2.5 && v.pos[1] < 2.0 + RAIL);
        let before = drawn(&b);
        assert!(tiles_up(&before) > 0 && rail_top(&before, 2.0) == 0);
        b.floors[1].set_hole(1, 1, Some("wood"));
        let after = drawn(&b);
        assert_eq!(tiles_up(&after) * 6, tiles_up(&before) * 5, "five of its six cells floored");
        assert!(rail_top(&after, 2.0) > 0, "a railing round it");
        assert!(baluster(&after, 1.0, 1.125) && baluster(&after, 2.0, 1.875) && baluster(&after, 1.375, 1.0), "on its west, east and north edges");
        // none along the outside wall (its south side), none on the ground floor below
        assert!(!baluster(&after, 1.375, 2.0), "the wall is there");
        assert_eq!(rail_top(&after, 0.0), 0);
    }

    /// Between a flooring's planks, and from cell to cell, nothing below shows: straight down
    /// from anywhere on a floor is the flooring or what it lies on (its colour, darker), never
    /// the ground (nor, upstairs, the room below); a hole stays open.
    #[test]
    fn nothing_shows_between_the_planks() {
        let mut c = content();
        // the wood floor's way: three planks a cell long, a hair apart
        let plank = |z: f64, color: &str| Part { shape: "box".into(), size: [1.0, 0.05, 0.32], at: [0.0, 0.0, z], color: color.into(), ..Default::default() };
        let parts = vec![plank(-0.33, "#c08a55"), plank(0.0, "#b27c49"), plank(0.33, "#c69060")];
        c.props.put(Prop { id: "planks".into(), name: "planks".into(), category: "floor".into(), footprint: [1, 1], height: 0.05, parts, ..Default::default() }, Space::Content);
        let mut b = Building { floors: vec![Floor { tile: "planks".into(), ..Default::default() }; 2], ..house() };
        b.floors[1].set_hole(1, 1, Some(""));
        let m = building(&b, &c, 1.0, Some(Cut { top: 1, low: Sides::NONE }), &Models::new()).plain;
        // the colour of the highest flat triangle over (x, z) between the heights lo and hi
        let seen = |x: f32, z: f32, lo: f32, hi: f32| {
            let mut best: Option<(f32, [f32; 4])> = None;
            for t in m.indices.chunks(3) {
                let v = [t[0], t[1], t[2]].map(|i| m.vertices[i as usize]);
                let p = v.map(|v| Vec3::from(v.pos));
                let n = (p[1] - p[0]).cross(p[2] - p[0]);
                if n.y.abs() < 0.99 * n.length() || p[0].y < lo || p[0].y > hi {
                    continue;
                }
                let side = |a: Vec3, b: Vec3| (b.x - a.x) * (z - a.z) - (b.z - a.z) * (x - a.x);
                let s = [side(p[0], p[1]), side(p[1], p[2]), side(p[2], p[0])];
                if (s.iter().all(|&d| d >= -1e-6) || s.iter().all(|&d| d <= 1e-6)) && best.is_none_or(|(y, _)| p[0].y > y) {
                    best = Some((p[0].y, v[0].color));
                }
            }
            best.map(|(_, c)| c)
        };
        // every 2.5 mm across the house, on both floors (the ground floor's flooring 0.02 up)
        for (lo, hi) in [(0.01, 0.1), (1.999, 2.1)] {
            for x in [0.5, 1.5, 2.5] {
                for k in 0..800 {
                    let z = (k as f32 + 0.5) * 0.0025;
                    let hole = lo > 1.0 && x == 1.5 && z > 1.0;
                    assert!(hole || seen(x, z, lo, hi).is_some(), "the ground or the room below shows at ({x}, {z}) above {lo}");
                }
            }
        }
        // between two planks, darker than they are; the hole open, the floor below it whole
        let gap = seen(1.5, 0.335, 0.01, 0.1).unwrap();
        assert!((0..3).all(|i| gap[i] < crate::color("#b27c49")[i] * 0.8), "{gap:?}");
        assert_eq!(seen(1.5, 0.5, 0.01, 0.1), Some(crate::color("#b27c49")), "the middle plank on top");
        assert!(seen(1.5, 1.5, 1.999, 2.1).is_none() && seen(1.5, 1.5, 0.01, 0.1).is_some());
    }

    #[test]
    fn a_wall_segment_in_its_own_style() {
        let mut b = house();
        let bricks = |m: &Mesh| of(m, "#d8cbb8");
        assert!(bricks(&drawn(&b)).is_empty());
        // the north wall's first segment on the ground floor: bricks there, plaster beside
        b.floors[0].set_wall(0, 0, "n", "bricks", "plaster");
        let m = drawn(&b);
        let brick = bricks(&m);
        assert!(!brick.is_empty());
        assert!(brick.iter().all(|p| p[0] <= 1.001 && p[2] <= 0.13 && p[1] <= 2.001), "only that segment, on that floor: {brick:?}");
        let plaster = of(&m, "#efe6d2");
        assert!(plaster.iter().any(|p| p[0] > 1.5 && p[2] < 0.13 && p[1] < 2.0), "plaster on the rest of that wall");
        // on the floor above too: that floor's segment
        b.floors[1].set_wall(0, 0, "n", "bricks", "plaster");
        assert!(bricks(&drawn(&b)).iter().any(|p| p[1] > 2.01));
    }

    /// The walls facing the camera: the south's at its usual turn (0), the east's too past 45
    /// degrees; a wall cut down stays so until it faces the camera a little less, so it
    /// doesn't flicker as the camera turns past.
    #[test]
    fn the_walls_facing_the_camera_are_the_ones_cut_down() {
        let (s, e) = (Sides::of("s"), Sides::of("e"));
        assert_eq!(facing(0.0, Sides::NONE), s);
        assert_eq!(facing(45.0, Sides::NONE), s.or(e));
        assert_eq!(facing(180.0, Sides::NONE), Sides::of("n"));
        assert_eq!(facing(270.0, Sides::NONE), Sides::of("w"));
        assert_eq!(facing(15.0, s), s, "the east's not yet");
        let both = facing(25.0, s);
        assert_eq!(both, s.or(e));
        assert_eq!(facing(15.0, both), both, "cut down, it stays so a while");
        assert_eq!(facing(5.0, both), s);
        assert_eq!((s.or(e).letters(), s.or(e).minus(s)), ("se".to_string(), e));
    }

    /// On the floor shown last, the walls on the cut's sides go no higher than a stub, and
    /// their doors and windows go; the others stay whole. Above the stub is what fades.
    #[test]
    fn the_walls_cut_down_keep_a_stub() {
        let b = house();
        let cut = |low: Sides| building(&b, &content(), 1.0, Some(Cut { top: 0, low }), &Models::new()).plain;
        // the highest point of a wall's outside face, by its pattern a hair out of the outline
        // (the south's at z 2.004, the north's at -0.004: no other wall's face is there)
        let top = |m: &Mesh, z: f32| m.vertices.iter().filter(|v| (v.pos[2] - z).abs() < 0.002).map(|v| v.pos[1]).fold(0.0f32, f32::max);
        let south = |m: &Mesh| top(m, 2.004);
        let north = |m: &Mesh| top(m, -0.004);
        let whole = cut(Sides::NONE);
        assert!((south(&whole) - 2.0).abs() < 0.01 && (north(&whole) - 2.0).abs() < 0.01);
        let low = cut(Sides::of("s"));
        assert!(south(&low) > 0.1 && south(&low) <= STUB + 0.001, "the south wall a stub: {}", south(&low));
        assert!((north(&low) - 2.0).abs() < 0.01, "the north wall whole");
        // what fades: the south wall above its stub, and nothing else
        let upper = walls_above_stub(&b, &content(), 1.0, 0, Sides::of("s"), &Models::new()).plain;
        assert!(!upper.vertices.is_empty());
        assert!(upper.vertices.iter().all(|v| v.pos[1] > STUB - 0.01 && v.pos[1] < 2.01 && v.pos[2] > 2.0 - WALL - 0.01), "only the south wall above its stub");
        assert!(walls_above_stub(&b, &content(), 1.0, 0, Sides::NONE, &Models::new()).plain.vertices.is_empty());
        // and what a cut takes away has it, on top of the floors above
        let above = building_above(&b, &content(), 1.0, 0, Sides::of("s"), &Models::new()).plain;
        assert!(above.vertices.iter().any(|v| v.pos[1] < 2.0 && v.pos[2] > 2.0 - WALL - 0.01) && above.vertices.iter().any(|v| v.pos[1] > 2.5));
    }

    #[test]
    fn a_hole_without_railing_has_none() {
        let mut b = house();
        b.floors[1].set_hole(1, 1, Some(""));
        let m = drawn(&b);
        assert!(!m.vertices.iter().any(|v| (v.pos[1] - (2.0 + RAIL)).abs() < 0.01), "no rail at its height");
    }

    #[test]
    fn inside_faces_in_their_own_style() {
        // the bricks' joints (trim) colour, on a face: outside a hair out of the north wall
        // (z < 0), inside a hair in (z past the wall's thickness)
        let face = |m: &Mesh, inside: bool| of(m, "#b5523b").iter().chain(of(m, "#d8cbb8").iter()).filter(|p| p[1] < 2.0 && p[0] < 1.0).any(|p| if inside { p[2] > WALL } else { p[2] < 0.0 });
        let mut b = house();
        // by default an inside face is as its outside: bricks both sides
        b.floors[0].set_wall(0, 0, "n", "bricks", "plaster");
        let m = drawn(&b);
        assert!(face(&m, false) && face(&m, true));
        // its own: plaster inside, bricks kept outside; the outside changed: the inside kept
        b.floors[0].set_inner(0, 0, "n", "plaster");
        let m = drawn(&b);
        assert!(face(&m, false) && !face(&m, true));
        b.floors[0].set_inner(0, 0, "n", "bricks");
        b.floors[0].set_wall(0, 0, "n", "plaster", "plaster");
        let m = drawn(&b);
        assert!(!face(&m, false) && face(&m, true), "bricks inside only");
    }

    #[test]
    fn a_floor_is_two_levels_of_ground_on_the_building_s_level() {
        // the world's measures: a level of ground one cell, a floor two (a character's 1.5
        // and room above it)
        let world = content::Settings::default();
        assert_eq!((world.level_height(), world.floor_height()), (1.0, 2.0));
        // a house of two floors on level 3 (its roof left off: its gables are in the walls'
        // colour): its walls from 3 cells up to 7, its upper floor's walls from 5
        let b = Building { level: 3, ..house() };
        let m = drawn(&b);
        let walls: Vec<f32> = of(&m, "#efe6d2").iter().map(|p| p[1]).collect();
        let (low, high) = walls.iter().fold((f32::MAX, f32::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        assert!((low - 3.0).abs() < 0.01 && (high - 7.0).abs() < 0.01, "{low} {high}");
        assert!(walls.iter().any(|&y| (y - 5.0).abs() < 0.01), "the upper floor's foot");
        // on the map: its ground floor's walls stand in the way at levels 3 and 4, its upper
        // floor's at 5 and 6, nothing at 7
        let mut map = crate::WorldMap::new("m", "M", 8, 8, "grass");
        map.buildings.push(Building { at: [2, 2], ..b });
        for (level, wall) in [(2, false), (3, true), (4, true), (5, true), (6, true), (7, false)] {
            assert_eq!(map.wall_between((1, 2), (2, 2), level), wall, "level {level}");
        }
    }
}
