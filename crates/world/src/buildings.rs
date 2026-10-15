//! Buildings' geometry: on each floor, the floor's tiles on every cell, walls along the
//! outline (patterned on the outside, with holes where doors and windows are), the doors,
//! windows and what stands inside; pillars as high as the building; a gabled roof on a
//! rectangle, a flat one with a parapet on any other shape; decorations on the walls.
//! A building can be cut away above a floor, for editing it.

use content::{segment_ends, side_turn, Building, Content};
use glam::{Mat4, Vec3};

use crate::mesh::{placement, prop_scene, Mesh, Models, Scene};
use crate::{color, WorldMap};

/// How thick walls are, inwards from the outline.
pub const WALL: f32 = 0.12;
/// Doors and windows leave this much wall on each side of their span.
const MARGIN: f32 = 0.1;

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
    let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    m.quad([p(lo.x, hi.y, lo.z), p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, hi.y, lo.z)], c);
    m.quad([p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)], shade(c, 0.7));
    m.quad([p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(hi.x, lo.y, hi.z), p(hi.x, hi.y, hi.z)], c);
    m.quad([p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, hi.y, lo.z)], c);
    m.quad([p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)], c);
    m.quad([p(lo.x, hi.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z), p(lo.x, hi.y, hi.z)], c);
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

    /// A box of wall from a0 to a1 along, y0 to y1 up.
    fn piece(&self, m: &mut Mesh, a0: f32, a1: f32, y0: f32, y1: f32, c: [f32; 4]) {
        let (p, q) = (self.point(a0, y0, 0.0), self.point(a1, y1, WALL));
        aabb(m, p.min(q), p.max(q), c);
    }

    /// A flat rectangle on the wall's outside face, a hair out.
    fn face(&self, m: &mut Mesh, a0: f32, a1: f32, y0: f32, y1: f32, c: [f32; 4]) {
        let e = -0.004;
        quad_facing(m, [self.point(a0, y1, e), self.point(a0, y0, e), self.point(a1, y0, e), self.point(a1, y1, e)], self.out(), c);
    }
}

/// The outside of a piece of wall, in the wall's pattern; (a0, a1) along, (v0, v1) from
/// the floor at `floor_y`. Patterns line up from piece to piece (they're laid from fixed
/// points).
#[allow(clippy::too_many_arguments)]
fn pattern(m: &mut Mesh, line: Line, kind: &str, main: [f32; 4], trim: [f32; 4], a0: f32, a1: f32, v0: f32, v1: f32, floor_y: f32, level_height: f32) {
    let tile = |m: &mut Mesh, b0: f32, b1: f32, w0: f32, w1: f32, c: [f32; 4]| {
        let (b0, b1, w0, w1) = (b0.max(a0), b1.min(a1), w0.max(v0), w1.min(v1));
        if b1 - b0 > 0.02 && w1 - w0 > 0.02 {
            line.face(m, b0, b1, floor_y + w0, floor_y + w1, c);
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
            tile(m, a0, a1, level_height - 0.08, level_height, trim);
        }
    }
}

/// What a building looks like while it's edited: floors above `top` cut away, no roof.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub top: usize,
}

pub fn building(b: &Building, content: &Content, level_height: f32, cut: Option<Cut>, models: &Models) -> Scene {
    let mut m = Mesh::default();
    let lh = level_height;
    let base = b.level as f32 * lh;
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    let wall = content.props.get(&b.wall);
    let main = color(wall.map(|w| w.color(0)).unwrap_or("#efe6d2"));
    let trim = color(wall.map(|w| w.color(1)).unwrap_or("#b89f7e"));
    let kind = wall.map(|w| w.pattern.as_str()).unwrap_or("plaster");
    let body = if matches!(kind, "plaster" | "") { main } else { trim };
    let shown = match cut {
        Some(c) => (c.top + 1).min(b.floors.len()),
        None => b.floors.len(),
    };
    let model = |id: &str| content.props.get(id).map(|p| prop_scene(p, models));
    // the models' textured parts (the plain ones go into m)
    let mut textured = Scene::default();
    let put = |m: &mut Mesh, textured: &mut Scene, s: &Scene, t: Mat4| {
        m.append(&s.plain, t);
        let only = Scene { plain: Mesh::default(), textured: s.textured.clone() };
        textured.append(&only, t);
    };

    // the walls' runs: (side, line, cells along) for every straight piece of outline
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

    for f in 0..shown {
        let floor = &b.floors[f];
        let y0 = base + f as f32 * lh;
        // the floor
        if let Some(tile) = model(&floor.tile) {
            for (x, z) in b.cells() {
                let at = Vec3::new(ox + x as f32 + 0.5, y0 + if f == 0 { 0.02 } else { 0.0 }, oz + z as f32 + 0.5);
                put(&mut m, &mut textured, &tile, Mat4::from_translation(at));
            }
        }
        // the walls, with their doors and windows
        for &(side, l, p0, p1) in &runs {
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
                if let Some(model) = model(&o.prop) {
                    let at = line.point(origin + centre, y0 + bottom, 0.0);
                    put(&mut m, &mut textured, &model, placement(at, side_turn(side) as f32, 1.0));
                }
            }
            holes.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut cur = start;
            let solid = |m: &mut Mesh, a0: f32, a1: f32, v0: f32, v1: f32| {
                if a1 - a0 > 0.001 && v1 - v0 > 0.001 {
                    line.piece(m, origin + a0, origin + a1, y0 + v0, y0 + v1, body);
                    pattern(m, line, kind, main, trim, origin + a0, origin + a1, v0, v1, y0, lh);
                }
            };
            for &(h0, h1, bottom, top) in &holes {
                solid(&mut m, cur, h0, 0.0, lh);
                solid(&mut m, h0, h1, 0.0, bottom);
                solid(&mut m, h0, h1, top, lh);
                cur = h1;
            }
            solid(&mut m, cur, end, 0.0, lh);
        }
        // inside
        for p in &floor.props {
            let Some(prop) = content.props.get(&p.prop) else { continue };
            let Some(model) = model(&p.prop) else { continue };
            let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
            let at = Vec3::new(ox + p.at[0] as f32, y0 + 0.05, oz + p.at[1] as f32);
            put(&mut m, &mut textured, &model, placement(at, p.turn as f32, scale));
        }
        // on the walls: outside facing out, inside (past the wall's thickness) facing in
        for a in b.anchors.iter().filter(|a| a.floor as usize == f) {
            let Some(model) = model(&a.prop) else { continue };
            let (p, q) = segment_ends(a.cell[0], a.cell[1], &a.side);
            let mut mid = Vec3::new(ox + (p[0] + q[0]) as f32 / 2.0, y0 + a.height as f32, oz + (p[1] + q[1]) as f32 / 2.0);
            let mut turn = side_turn(&a.side) as f32;
            if a.inside {
                let out = Line { side: &a.side, at: 0.0 }.out();
                mid -= out * WALL;
                turn += 180.0;
            }
            put(&mut m, &mut textured, &model, placement(mid, turn, 1.0));
        }
    }
    // what stands outside it
    for p in &b.outside {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let Some(model) = model(&p.prop) else { continue };
        let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
        let at = Vec3::new(ox + p.at[0] as f32, base, oz + p.at[1] as f32);
        put(&mut m, &mut textured, &model, placement(at, p.turn as f32, scale));
    }
    // pillars, as high as what's shown
    for p in &b.pillars {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let Some(model) = model(&p.prop) else { continue };
        let stretch = shown as f32 * lh / (prop.height as f32).max(0.1);
        let at = Vec3::new(ox + p.corner[0] as f32, base, oz + p.corner[1] as f32);
        put(&mut m, &mut textured, &model, Mat4::from_translation(at) * Mat4::from_scale(Vec3::new(1.0, stretch, 1.0)));
    }
    if cut.is_none() && !b.floors.is_empty() {
        roof(&mut m, b, content, base + b.floors.len() as f32 * lh, main);
    }
    textured.plain = m;
    textured
}

fn roof(m: &mut Mesh, b: &Building, content: &Content, top: f32, wall: [f32; 4]) {
    let prop = content.props.get(&b.roof);
    let main = color(prop.map(|p| p.color(0)).unwrap_or("#b5473a"));
    let trim = color(prop.map(|p| p.color(1)).unwrap_or("#8a3229"));
    let kind = prop.map(|p| p.pattern.as_str()).unwrap_or("tiles");
    let (w, h) = b.size();
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    let rectangle = b.cells().len() as i32 == w * h;
    if !rectangle {
        // flat, with a parapet
        for (x, z) in b.cells() {
            let k = 0.92 + 0.12 * hash(x, z, 17);
            aabb(m, Vec3::new(ox + x as f32, top, oz + z as f32), Vec3::new(ox + x as f32 + 1.0, top + 0.14, oz + z as f32 + 1.0), shade(main, k));
        }
        for (x, z, side) in b.edges() {
            let (p, q) = segment_ends(x, z, side);
            let line = Line { side, at: if matches!(side, "n" | "s") { oz + p[1] as f32 } else { ox + p[0] as f32 } };
            let (a0, a1) = if matches!(side, "n" | "s") { (ox + p[0] as f32, ox + q[0] as f32) } else { (oz + p[1] as f32, oz + q[1] as f32) };
            line.piece(m, a0, a1, top, top + 0.42, trim);
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

/// Every building on a map; the one being edited (index, floor) cut away above that floor.
pub fn buildings(map: &WorldMap, content: &Content, level_height: f32, editing: Option<(usize, usize)>, models: &Models) -> Scene {
    let mut out = Scene::default();
    for (i, b) in map.buildings.iter().enumerate() {
        let cut = editing.filter(|(e, _)| *e == i).map(|(_, f)| Cut { top: f });
        out.append(&building(b, content, level_height, cut, models), Mat4::IDENTITY);
    }
    out
}

/// Where a wall segment is: its middle on the outline at a floor's height, and which way
/// is out.
pub fn segment_point(b: &Building, cell: [i32; 2], side: &str, floor: usize, height: f32, level_height: f32) -> (Vec3, Vec3) {
    let (p, q) = segment_ends(cell[0], cell[1], side);
    let y = (b.level as usize + floor) as f32 * level_height + height;
    let mid = Vec3::new(b.at[0] as f32 + (p[0] + q[0]) as f32 / 2.0, y, b.at[1] as f32 + (p[1] + q[1]) as f32 / 2.0);
    let out = Line { side, at: 0.0 }.out();
    (mid, out)
}
