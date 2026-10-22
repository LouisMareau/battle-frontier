//! Ramps' ground: down a level in one cell, from the ramp's cells' level at its top (the top of
//! a step) to the ground at its foot, easing a little into the ground at both edges, and a
//! little uneven: bumps, and its slope rounder in places and straighter in others as it runs
//! along the step; never at its foot and top, which meet the ground flush. Each ramp is uneven
//! its own way (its seed) and as much as it says (`rough`). And what lies along a ramp (stairs,
//! a mud slide: `content::RampFit`), laid on it from its foot to its top and end to end
//! (`lay`), its ground straight under it (`even`).

use std::f32::consts::PI;

use content::{Ramp, RampFit};
use glam::{Mat4, Vec3};

use crate::mesh::{noise3, Along, Put};

/// How much a ramp's slope eases into the ground at its edges (0: straight).
const EASE: f32 = 0.35;

/// How far under the straight line from its foot to its top the ground of a ramp something
/// lies along is (in cells), soon after its foot and until just before its top: clear of that
/// thing's underside, never through it.
pub const SINK: f32 = 0.06;

/// How far in from its ends the walls down from a ramp something lies along stand (in
/// cells): behind that thing's own ends rather than through them.
pub const TUCK: f32 = 0.02;

/// How far up a ramp's ground is, in levels from its foot, `t` of the way up (0 at its foot,
/// 1 at its top), eased `ease` (0 straight; under 1, it only ever rises).
fn eased(t: f32, ease: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    t + (smooth - t) * ease
}

/// A smooth ramp's ground, in levels from its foot, `t` of the way up.
pub fn profile(t: f32) -> f32 {
    eased(t, EASE)
}

/// A ramp's ground at a point of its cells (in cells), its cells' level at `top` (their
/// height): a level (`level_height`) lower at its foot.
pub fn height(r: &Ramp, top: f32, level_height: f32, px: f32, pz: f32) -> f32 {
    let t = r.along(px as f64, pz as f64).0 as f32;
    let k = r.rough.clamp(0.0, 1.0) as f32;
    let seed = (r.seed % 10_007) as i32;
    let p = Vec3::new(px, 0.0, pz);
    // rounder here, straighter there, as it runs along the step
    let ease = EASE + k * 0.8 * (noise3(p, 2.5, seed + 2) - 0.5);
    // and bumps, none at its edges
    let bumps = k * (PI * t.clamp(0.0, 1.0)).sin() * ((noise3(p, 0.6, seed) - 0.5) * 0.08 + (noise3(p, 0.27, seed + 1) - 0.5) * 0.04);
    top - level_height + level_height * eased(t, ease) + bumps
}

/// How much rock shows on a ramp's ground (0 to 1) `t` of the way up: in ragged patches down
/// its middle, its steepest; none at its foot and top.
pub fn rock(t: f32) -> f32 {
    0.45 * (PI * t.clamp(0.0, 1.0)).sin()
}

/// The ground of a ramp something lies along (stairs, a mud slide), at a point of its cells:
/// straight from its foot (a level under `top`, its cells' height) to its top, all along it,
/// and a little under that line between them (`SINK`), so what's made on the line covers it.
pub fn even(r: &Ramp, top: f32, level_height: f32, px: f32, pz: f32) -> f32 {
    let t = (r.along(px as f64, pz as f64).0 as f32).clamp(0.0, 1.0);
    top - level_height + level_height * t - SINK * (6.0 * (PI * t).sin()).min(1.0)
}

/// The way up a ramp as a prop's turn (degrees, clockwise seen from above): what's made
/// rising along -z, turned so, rises up it.
pub fn turn_of(up: &str) -> f32 {
    match up {
        "e" => 90.0,
        "s" => 180.0,
        "w" => 270.0,
        _ => 0.0,
    }
}

/// Where the model of a prop that lies along ramps goes on one (`content::RampFit`), its
/// cells at `top` (their height): turned up the ramp's way, its foot on the ground along the
/// ramp's foot (a level down), its rise made a level, and from one end of the ramp to the
/// other, as one copy stretched along it or copies side by side, one per `width` cells
/// (stretched a little to make up the length), their ends keeping their size.
pub fn lay(r: &Ramp, fit: &RampFit, width: u8, top: f32, level_height: f32) -> Vec<Put> {
    let length = r.length().max(1) as f32;
    let width = width.max(1) as f32;
    let copies = if fit.repeats() { (length / width).round().max(1.0) } else { 1.0 };
    let each = length / copies;
    let centre = Vec3::new(r.at[0] as f32 + r.size[0] as f32 / 2.0, top - level_height, r.at[1] as f32 + r.size[1] as f32 / 2.0);
    let turn = Mat4::from_rotation_y(-turn_of(&r.up).to_radians());
    let rise = level_height / (fit.rise as f32).max(0.01);
    let along = turn.transform_vector3(Vec3::X);
    (0..copies as usize)
        .map(|k| {
            let at = centre + along * (-length / 2.0 + (k as f32 + 0.5) * each);
            let m = Mat4::from_translation(at) * turn * Mat4::from_scale(Vec3::new(1.0, rise, 1.0));
            Put { m, along: Some(Along { half: width / 2.0, to: each / 2.0, ends: fit.ends as f32 }), rigidity: 0.0 }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(rough: f64) -> Ramp {
        Ramp { at: [4, 10], size: [3, 1], up: "n".into(), rough, seed: 7 }
    }

    #[test]
    fn a_ramp_meets_the_ground_flush_at_both_edges() {
        let r = ramp(1.0);
        for x in [4.0, 4.6, 5.5, 6.9, 7.0] {
            // its foot (the south edge) a level down, its top (north) on its level
            assert!((height(&r, 4.0, 2.0, x, 11.0) - 2.0).abs() < 1e-4, "foot at {x}");
            assert!((height(&r, 4.0, 2.0, x, 10.0) - 4.0).abs() < 1e-4, "top at {x}");
        }
        // up all the way, a quarter of a cell after a quarter, as uneven as it gets
        for x in [4.2, 5.5, 6.8] {
            let mut last = height(&r, 4.0, 2.0, x, 11.0);
            for k in 1..=4 {
                let h = height(&r, 4.0, 2.0, x, 11.0 - k as f32 * 0.25);
                assert!(h > last, "down at {x}, {k}: {last} -> {h}");
                last = h;
            }
        }
    }

    #[test]
    fn smooth_is_even_and_rough_is_not() {
        let (smooth, rough) = (ramp(0.0), ramp(1.0));
        // along it, halfway up: smooth, all one height (half a level down); rough, not
        let along = |r: &Ramp| (0..=12).map(|k| height(r, 4.0, 2.0, 4.0 + k as f32 * 0.25, 10.5)).collect::<Vec<f32>>();
        let (a, b) = (along(&smooth), along(&rough));
        assert!(a.iter().all(|&h| (h - 3.0).abs() < 1e-5), "{a:?}");
        let spread = b.iter().cloned().fold(f32::MIN, f32::max) - b.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.01 && spread < 0.5, "{spread}");
        // another seed, another ramp
        assert_ne!(along(&Ramp { seed: 8, ..rough.clone() }), b);
        // rock down its middle only
        assert!(rock(0.0) == 0.0 && rock(1.0).abs() < 1e-6 && rock(0.5) > 0.4);
    }

    /// Where the ground lies open: the stretches of its triangles' edges (the tops and the
    /// cliffs together) that only one triangle has and that no other such edge runs along,
    /// cracks to see through (or fins standing off the ground); the map's own rim, the skirt's
    /// foot, left out. Each as a point of it.
    fn open_seams(map: &crate::WorldMap, level_height: f32) -> Vec<[f32; 3]> {
        use std::collections::HashMap;
        let (tops, cliffs, _) = crate::mesh::ground_region(map, &content::Content::default(), level_height, crate::mesh::whole(map), &crate::mesh::Alone);
        let mut tris = vec![];
        for m in [&tops, &cliffs] {
            for t in m.indices.chunks(3) {
                let p = |i: u32| Vec3::from(m.vertices[i as usize].pos);
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                if (b - a).cross(c - a).length() > 1e-7 {
                    tris.push([a, b, c]);
                }
            }
        }
        // each edge by its ends, and how many triangles have it
        type Key = (i64, i64, i64);
        let key = |p: Vec3| -> Key { ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64) };
        let mut count: HashMap<(Key, Key), (Vec3, Vec3, usize)> = HashMap::new();
        for t in &tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(a), key(b));
                count.entry(if ka < kb { (ka, kb) } else { (kb, ka) }).or_insert((a, b, 0)).2 += 1;
            }
        }
        let open: Vec<(Vec3, Vec3)> = count.values().filter(|e| e.2 == 1).map(|e| (e.0, e.1)).collect();
        // the open edges by the half cells they pass over
        let mut near: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, (a, b)) in open.iter().enumerate() {
            let (lo, hi) = (a.min(*b), a.max(*b));
            for gx in (lo.x * 2.0 - 0.01).floor() as i32..=(hi.x * 2.0 + 0.01).floor() as i32 {
                for gz in (lo.z * 2.0 - 0.01).floor() as i32..=(hi.z * 2.0 + 0.01).floor() as i32 {
                    near.entry((gx, gz)).or_default().push(i);
                }
            }
        }
        let on = |p: Vec3, (a, b): (Vec3, Vec3)| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            (a + ab * t - p).length() < 2e-3
        };
        let mut out = vec![];
        for (i, &(a, b)) in open.iter().enumerate() {
            if a.y <= -0.6 + 1e-3 && b.y <= -0.6 + 1e-3 {
                continue;
            }
            for s in [0.1, 0.3, 0.5, 0.7, 0.9] {
                let p = a + (b - a) * s;
                let others = near.get(&((p.x * 2.0).floor() as i32, (p.z * 2.0).floor() as i32));
                if !others.is_some_and(|o| o.iter().any(|&j| j != i && on(p, open[j]))) {
                    out.push(p.to_array());
                    break;
                }
            }
        }
        out
    }

    /// Where the sky shows through the ground, looked at from above along `d` (down): the
    /// ground's triangles (tops and cliffs: both their sides are drawn) laid flat across `d`
    /// onto a grid `step` fine; a point of the grid none covers whose line of sight comes out
    /// under the ground inside the map (a cell or more in from its edges) is a hole. Each as
    /// where the line of sight first goes into the ground.
    fn sky_through(map: &crate::WorldMap, level_height: f32, d: Vec3, step: f32) -> Vec<[f32; 3]> {
        let (tops, cliffs, _) = crate::mesh::ground_region(map, &content::Content::default(), level_height, crate::mesh::whole(map), &crate::mesh::Alone);
        let d = d.normalize();
        let e1 = d.cross(Vec3::Y).normalize();
        let e2 = e1.cross(d).normalize();
        let flat = |p: Vec3| (p.dot(e1), p.dot(e2));
        let mut tris = vec![];
        for m in [&tops, &cliffs] {
            for t in m.indices.chunks(3) {
                let p = |i: u32| flat(Vec3::from(m.vertices[i as usize].pos));
                tris.push([p(t[0]), p(t[1]), p(t[2])]);
            }
        }
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for &(a, b) in tris.iter().flatten() {
            lo = (lo.0.min(a), lo.1.min(b));
            hi = (hi.0.max(a), hi.1.max(b));
        }
        let (w, h) = (((hi.0 - lo.0) / step) as usize + 2, ((hi.1 - lo.1) / step) as usize + 2);
        let mut covered = vec![false; w * h];
        let eps = step * 0.15;
        for t in &tris {
            let [(ax, ay), (bx, by), (cx, cy)] = *t;
            let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
            if area.abs() < 1e-9 {
                continue;
            }
            // in: on the inner side of each edge, or within eps of it
            let side = |(px, py): (f32, f32), (qx, qy): (f32, f32), (x, y): (f32, f32)| {
                let (ex, ey) = (qx - px, qy - py);
                (ex * (y - py) - ey * (x - px)) * area.signum() / (ex * ex + ey * ey).sqrt().max(1e-9)
            };
            let i0 = ((ax.min(bx).min(cx) - lo.0) / step).floor().max(0.0) as usize;
            let i1 = (((ax.max(bx).max(cx) - lo.0) / step).ceil() as usize).min(w - 1);
            let j0 = ((ay.min(by).min(cy) - lo.1) / step).floor().max(0.0) as usize;
            let j1 = (((ay.max(by).max(cy) - lo.1) / step).ceil() as usize).min(h - 1);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let q = (lo.0 + i as f32 * step, lo.1 + j as f32 * step);
                    if side(t[0], t[1], q) > -eps && side(t[1], t[2], q) > -eps && side(t[2], t[0], q) > -eps {
                        covered[j * w + i] = true;
                    }
                }
            }
        }
        // the ground a line of sight goes into: the cells' levels, a ramp's own
        let ideal = |p: Vec3| {
            let (cx, cz) = (p.x.floor() as i32, p.z.floor() as i32);
            let l = map.level_at(cx, cz)? as f32 * level_height;
            Some(map.ramps.iter().find(|r| r.has(cx, cz)).map(|r| height(r, l, level_height, p.x, p.z)).unwrap_or(l))
        };
        let mut out = vec![];
        for j in 0..h {
            for i in 0..w {
                if covered[j * w + i] {
                    continue;
                }
                let base = e1 * (lo.0 + i as f32 * step) + e2 * (lo.1 + j as f32 * step);
                let s = (-0.5 - base.y) / d.y;
                let q = base + d * s;
                if q.x > 1.0 && q.z > 1.0 && q.x < map.width as f32 - 1.0 && q.z < map.height as f32 - 1.0 {
                    let mut k = (12.0 - base.y) / d.y;
                    let mut into = q;
                    while k < s {
                        let p = base + d * k;
                        if ideal(p).is_some_and(|g| p.y < g - 0.3) {
                            into = p;
                            break;
                        }
                        k += 0.01;
                    }
                    out.push(into.to_array());
                }
            }
        }
        out
    }

    /// The ways looked from, at 50 degrees down: every eighth of the way round (`ways` 8),
    /// or every quarter.
    fn views(ways: usize) -> Vec<Vec3> {
        (0..ways)
            .map(|k| {
                let (yaw, pitch) = (k as f32 * 2.0 * PI / ways as f32 + 0.3, 50f32.to_radians());
                Vec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos())
            })
            .collect()
    }

    /// A map `n` cells square of blocks raised on blocks (levels 0 to 3), with up to four
    /// ramps where they can go: 1 to 3 long, smooth to rough, side by side or not.
    fn random_ground(seed: u64, n: i32) -> crate::WorldMap {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut next = move |k: u64| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) % k) as i32
        };
        let mut map = crate::WorldMap::new("fuzz", "Fuzz", n as usize, n as usize, "grass");
        for _ in 0..(4 + next(5)) {
            let (x0, z0, w, h) = (next(n as u64), next(n as u64), 1 + next(3), 1 + next(3));
            for z in z0..(z0 + h).min(n) {
                for x in x0..(x0 + w).min(n) {
                    let i = (z * n + x) as usize;
                    map.level[i] = (map.level[i] + 1).min(3);
                }
            }
        }
        for _ in 0..(1 + next(4)) {
            for _ in 0..60 {
                let (x, z) = (1 + next(n as u64 - 2), 1 + next(n as u64 - 2));
                let up = ["n", "e", "s", "w"][next(4) as usize];
                let long = 1 + next(3) as u16;
                let size = if matches!(up, "n" | "s") { [long, 1] } else { [1, long] };
                let r = Ramp { at: [x, z], size, up: up.into(), rough: [0.0, 0.5, 1.0][next(3) as usize], seed: next(1000) as u32 };
                let ((dx, dz), cells) = (r.way(), r.cells());
                let Some(top) = map.level_at(x, z).filter(|&l| l >= 1) else { continue };
                let inside = |cx: i32, cz: i32| cx > 0 && cz > 0 && cx < n - 1 && cz < n - 1;
                let fits = cells.iter().all(|&(cx, cz)| inside(cx, cz) && map.level_at(cx, cz) == Some(top) && map.level_at(cx - dx, cz - dz) == Some(top - 1))
                    && map.ramps.iter().all(|o| cells.iter().all(|&(cx, cz)| !o.has(cx, cz) && !o.has(cx - dx, cz - dz)));
                if fits {
                    map.ramps.push(r);
                    break;
                }
            }
        }
        map
    }

    /// A map's levels row by row, each ramp's cells with its way up (for a failure's message).
    fn drawn(map: &crate::WorldMap) -> String {
        let mut out = String::new();
        for z in 0..map.height as i32 {
            for x in 0..map.width as i32 {
                let way = map.ramps.iter().find(|r| r.has(x, z)).map(|r| r.up.as_str()).unwrap_or(" ");
                out.push_str(&format!("{}{way} ", map.level_at(x, z).unwrap_or(0)));
            }
            out.push('\n');
        }
        out
    }

    /// The ground round ramps closes up whatever is round them: random ground, ramps 1 to 3
    /// long, smooth or rough, side by side, at both level heights; no seam open anywhere, and
    /// (looked at every way round, now and then) no sky through it.
    #[test]
    fn no_seam_opens_round_ramps_on_any_ground() {
        let ways = views(4);
        let mut tried = 0;
        for seed in 0..300u64 {
            let map = random_ground(seed, 9);
            if map.ramps.is_empty() {
                continue;
            }
            tried += 1;
            for level_height in [2.0, 1.0] {
                let open = open_seams(&map, level_height);
                assert!(open.is_empty(), "seed {seed}, level height {level_height}, ramps {:?}\n{}open at {:?}", map.ramps, drawn(&map), &open[..open.len().min(4)]);
                if seed % 25 == 0 {
                    for &d in &ways {
                        let sky = sky_through(&map, level_height, d, 0.03);
                        assert!(sky.is_empty(), "seed {seed}, level height {level_height}: the sky through at {:?}\n{}", &sky[..sky.len().min(4)], drawn(&map));
                    }
                }
            }
        }
        assert!(tried > 250, "{tried} maps with ramps");
    }

    /// The ground as `rows` says (a digit per cell: its level), and ramps.
    fn ground(rows: &[&str], ramps: Vec<Ramp>) -> crate::WorldMap {
        let mut map = crate::WorldMap::new("seams", "Seams", rows[0].len(), rows.len(), "grass");
        for (z, row) in rows.iter().enumerate() {
            for (x, l) in row.bytes().enumerate() {
                map.level[z * rows[0].len() + x] = l - b'0';
            }
        }
        map.ramps = ramps;
        map
    }

    /// A ramp down beside a cliff a level higher than it, the cliff running on past it (a
    /// ramp's left side on the user's map, a slit two levels high along the cliff's corner):
    /// the cliff's face down onto the ramp and its neighbour's on past it are both pinned
    /// where they meet, nothing between them; and the sky doesn't show through anywhere.
    #[test]
    fn a_cliff_s_face_onto_a_ramp_meets_the_next_face_without_a_slit() {
        let map = ground(&["0000000", "0222100", "0222100", "0233100", "0011100", "0000000"], vec![Ramp { at: [4, 2], size: [1, 1], up: "w".into(), rough: 0.0, seed: 1 }]);
        let (_, cliffs, _) = crate::mesh::ground_region(&map, &content::Content::default(), 2.0, crate::mesh::whole(&map), &crate::mesh::Alone);
        // the faces' points on the line where they meet (x 4, z 2), from below the cliff's
        // top: all on it, none pushed off it by the rock's lumps
        let at_line: Vec<[f32; 3]> = cliffs.vertices.iter().map(|v| v.pos).filter(|p| (p[0] - 4.0).abs() < 0.2 && (p[2] - 2.0).abs() < 0.2 && p[1] > 2.05 && p[1] < 3.7).collect();
        assert!(!at_line.is_empty());
        let off: Vec<&[f32; 3]> = at_line.iter().filter(|p| (p[2] - 2.0).abs() < 1e-4 && (p[0] - 4.0).abs() > 1e-4).collect();
        assert!(off.is_empty(), "lumps off the corner: {off:?}");
        assert!(open_seams(&map, 2.0).is_empty(), "{:?}", open_seams(&map, 2.0));
        for d in views(8) {
            assert!(sky_through(&map, 2.0, d, 0.02).is_empty(), "the sky through, looking along {d}");
        }
    }

    /// A higher cell's outer corner over a ramp's top (a ramp's top corner on the user's map,
    /// a hole beside it): it isn't cut round as it would be over level ground, the faces either
    /// side meet in it down onto the ramp; nothing open, no sky through.
    #[test]
    fn a_corner_over_a_ramp_is_not_cut_round() {
        let map = ground(&["000000000", "000022100", "000022100", "000011100", "000000000", "000000000"], vec![Ramp { at: [5, 3], size: [1, 1], up: "n".into(), rough: 0.0, seed: 1 }]);
        let mut level_ground = map.clone();
        level_ground.ramps.clear();
        let (_, cliffs, _) = crate::mesh::ground_region(&map, &content::Content::default(), 2.0, crate::mesh::whole(&map), &crate::mesh::Alone);
        let (_, plain, _) = crate::mesh::ground_region(&level_ground, &content::Content::default(), 2.0, crate::mesh::whole(&level_ground), &crate::mesh::Alone);
        // the cell (5, 2) a level up over the ramp's top: its south-east corner (6, 3) is cut
        // round over level ground (a slanted face across it), never over the ramp
        let slanted = |m: &crate::mesh::Mesh| m.vertices.iter().any(|v| v.normal[0] > 0.5 && v.normal[2] > 0.5 && (v.pos[0] - 5.89).abs() < 0.12 && (v.pos[2] - 2.89).abs() < 0.12);
        assert!(slanted(&plain) && !slanted(&cliffs));
        assert!(open_seams(&map, 2.0).is_empty(), "{:?}", open_seams(&map, 2.0));
        for d in views(8) {
            assert!(sky_through(&map, 2.0, d, 0.02).is_empty(), "the sky through, looking along {d}");
        }
    }

    /// 10 x 10: a plateau on level 1 north of z 4, the lowland on 0, and a ramp two long on
    /// the plateau's edge (x 3, 4; z 3) down to it.
    fn hill() -> crate::WorldMap {
        let mut map = crate::WorldMap::new("hill", "Hill", 10, 10, "grass");
        for z in 0..4 {
            for x in 0..10 {
                map.level[z * 10 + x] = 1;
            }
        }
        map.ramps.push(Ramp { at: [3, 3], size: [2, 1], up: "n".into(), rough: 0.5, seed: 3 });
        map
    }

    #[test]
    fn up_and_down_a_ramp_on_foot_never_off_its_sides() {
        let map = hill();
        // from the lowland, onto it and up onto the plateau, and back
        let path = [(3, 5), (3, 4), (3, 3), (3, 2)];
        for w in path.windows(2) {
            assert!(map.ground_steps(w[0], w[1]) && map.ground_steps(w[1], w[0]), "{:?}", w);
        }
        // along it
        assert!(map.ground_steps((3, 3), (4, 3)));
        // off its sides, onto the plateau's edge either side: no (half a level)
        assert!(!map.ground_steps((3, 3), (2, 3)) && !map.ground_steps((4, 3), (5, 3)));
        // the cliff beside it is still a cliff
        assert!(!map.ground_steps((2, 3), (2, 4)));
        // standing on it: as high as its ground, its cells' level at its top
        assert!(map.ground_y(3.5, 4.0 - 1e-4, 1, 2.0).abs() < 1e-2);
        assert!((map.ground_y(3.5, 3.0 + 1e-4, 1, 2.0) - 2.0).abs() < 1e-2);
        let mid = map.ground_y(3.5, 3.5, 1, 2.0);
        assert!((mid - 1.0).abs() < 0.3, "{mid}");
        // on the plateau, and on another level than the ramp's, the level's
        assert_eq!(map.ground_y(3.5, 2.5, 1, 2.0), 2.0);
        assert_eq!(map.ground_y(3.5, 3.5, 0, 2.0), 0.0);
    }

    #[test]
    fn a_ramp_takes_the_cliff_s_place_walls_up_its_sides() {
        let map = hill();
        let (tops, cliffs, _) = crate::mesh::ground_region(&map, &content::Content::default(), 2.0, crate::mesh::whole(&map), &crate::mesh::Alone);
        assert!(tops.vertices.iter().chain(&cliffs.vertices).all(|v| v.pos.iter().all(|c| c.is_finite())));
        // the plateau's south face along z = 4, but where the ramp goes down
        let south = |v: &&crate::mesh::Vertex| v.normal[2] > 0.99 && (v.pos[2] - 4.0).abs() < 0.2;
        assert!(cliffs.vertices.iter().filter(south).any(|v| v.pos[0] < 3.0) && cliffs.vertices.iter().filter(south).any(|v| v.pos[0] > 5.0));
        assert!(!cliffs.vertices.iter().filter(south).any(|v| v.pos[0] > 3.01 && v.pos[0] < 4.99), "a cliff where the ramp goes down");
        // the plateau behind it keeps its rim on the level (no bevel down to it)
        let rim: Vec<f32> = tops.vertices.iter().filter(|v| (v.pos[2] - 3.0).abs() < 1e-4 && v.pos[0] > 3.2 && v.pos[0] < 4.8).map(|v| v.pos[1]).collect();
        assert!(!rim.is_empty() && rim.iter().all(|&y| (y - 2.0).abs() < 1e-3), "{rim:?}");
        // walls up its sides to the plateau's edge either side: none at its top, a level at
        // its foot (less the plateau's bevel there)
        let west: Vec<[f32; 3]> = cliffs.vertices.iter().filter(|v| v.normal[0] > 0.99 && (v.pos[0] - 3.0).abs() < 0.2).map(|v| v.pos).collect();
        let high = |z0: f32, z1: f32| west.iter().filter(|p| p[2] >= z0 && p[2] <= z1).map(|p| p[1]).fold(0.0f32, f32::max);
        assert!(high(3.75, 4.0) > 1.5 && high(3.0, 3.05) > 1.9, "{} {}", high(3.75, 4.0), high(3.0, 3.05));
        let low = |z0: f32, z1: f32| west.iter().filter(|p| p[2] >= z0 && p[2] <= z1).map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!(low(3.75, 4.0) < 0.3, "{}", low(3.75, 4.0));
        // the plateau's face beside it stops where the ramp's wall starts: its end there
        // pinned to the corner, lumps and all (no gap to see into the ground through)
        let corner: Vec<[f32; 3]> = cliffs.vertices.iter().filter(|v| v.normal[2] > 0.99 && (v.pos[0] - 3.0).abs() < 1e-4 && v.pos[2] < 5.0).map(|v| v.pos).collect();
        assert!(!corner.is_empty() && corner.iter().all(|p| (p[2] - 4.0).abs() < 1e-4), "{corner:?}");
        // its own top, flush with the lowland at its foot and the plateau at its top
        let ramp_top: Vec<[f32; 3]> = tops.vertices.iter().filter(|v| v.uv[0] > 3.0 && v.uv[0] < 5.0 && v.uv[1] > 3.0 && v.uv[1] < 4.0).map(|v| v.pos).collect();
        assert_eq!(ramp_top.len(), 2 * 49);
        assert!(ramp_top.iter().filter(|p| p[2] == 4.0).all(|p| p[1].abs() < 1e-4));
        assert!(ramp_top.iter().filter(|p| p[2] == 3.0).all(|p| (p[1] - 2.0).abs() < 1e-4));
    }

    /// A slide: a wedge from its foot (z 0.5, on the ground) up to its top (z -0.5, 2 up), a
    /// cell wide, made to lie along ramps as `along` says.
    fn slide(along: &str) -> content::Prop {
        content::Prop {
            id: "slide".into(),
            name: "Slide".into(),
            category: "ramp".into(),
            footprint: [1, 1],
            height: 2.0,
            blocks: true,
            parts: vec![content::Part { shape: "wedge".into(), size: [1.0, 2.0, 1.0], color: "#6b4a2f".into(), ..Default::default() }],
            ramp: Some(RampFit { along: along.into(), rise: 2.0, ..Default::default() }),
            ..Default::default()
        }
    }

    fn with(props: &[content::Prop]) -> content::Content {
        let mut c = content::Content::default();
        for p in props {
            c.props.put(p.clone(), content::Space::Content);
        }
        c
    }

    /// Placed on a ramp's centre, on its level.
    fn on(r: &Ramp, id: &str, level: u8) -> content::Placed {
        content::Placed { prop: id.into(), at: [r.at[0] as f64 + r.size[0] as f64 / 2.0, r.at[1] as f64 + r.size[1] as f64 / 2.0], level, ..Default::default() }
    }

    #[test]
    fn what_lies_along_a_ramp_fits_it_foot_to_top_end_to_end_whichever_way_it_rises() {
        // 12 x 12: a plateau a level up (x 3 to 8, z 3 to 8), a ramp three long down each side
        let mut map = crate::WorldMap::new("mesa", "Mesa", 12, 12, "grass");
        for z in 3..=8 {
            for x in 3..=8 {
                map.level[z * 12 + x] = 1;
            }
        }
        let ramps = [
            Ramp { at: [4, 8], size: [3, 1], up: "n".into(), ..Default::default() },
            Ramp { at: [4, 3], size: [3, 1], up: "s".into(), ..Default::default() },
            Ramp { at: [8, 4], size: [1, 3], up: "w".into(), ..Default::default() },
            Ramp { at: [3, 4], size: [1, 3], up: "e".into(), ..Default::default() },
        ];
        let content = with(&[slide("stretch")]);
        for r in &ramps {
            assert!(r.problems(&|x, z| map.level_at(x, z)).is_empty(), "{r:?}");
            let mut m = map.clone();
            m.ramps = vec![r.clone()];
            m.props = vec![on(r, "slide", 1)];
            let scene = crate::mesh::props_region(&m, &content, 2.0, &crate::mesh::Models::new(), crate::mesh::whole(&m));
            let v = &scene.plain.vertices;
            assert!(!v.is_empty());
            // on its cells, a level from its foot to its top, from one end to the other
            let (mut lo, mut hi) = ((f64::MAX, f64::MAX, f32::MAX), (f64::MIN, f64::MIN, f32::MIN));
            for p in v.iter().map(|v| v.pos) {
                let (t, across) = r.along(p[0] as f64, p[2] as f64);
                lo = (lo.0.min(t), lo.1.min(across), lo.2.min(p[1]));
                hi = (hi.0.max(t), hi.1.max(across), hi.2.max(p[1]));
            }
            let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
            assert!(near(lo.0, 0.0) && near(hi.0, 1.0) && near(lo.1, 0.0) && near(hi.1, 1.0), "{} {lo:?} {hi:?}", r.up);
            assert!(lo.2.abs() < 1e-4 && (hi.2 - 2.0).abs() < 1e-4, "{} {lo:?} {hi:?}", r.up);
            // its slope on the line from the ramp's foot to its top, facing down it
            let (dx, dz) = r.way();
            let slope: Vec<&crate::mesh::Vertex> = v.iter().filter(|v| v.normal[1] > 0.3).collect();
            assert_eq!(slope.len(), 4, "{}", r.up);
            for s in slope {
                let t = r.along(s.pos[0] as f64, s.pos[2] as f64).0 as f32;
                assert!((s.pos[1] - 2.0 * t).abs() < 1e-4, "{}: {:?} at {t}", r.up, s.pos);
                let down = -(s.normal[0] * dx as f32 + s.normal[2] * dz as f32);
                assert!((down - 2.0 / 5f32.sqrt()).abs() < 1e-4 && (s.normal[1] - 1.0 / 5f32.sqrt()).abs() < 1e-4, "{}: {:?}", r.up, s.normal);
            }
        }
    }

    #[test]
    fn copies_side_by_side_or_one_stretched_its_ends_kept() {
        let r = Ramp { at: [0, 0], size: [3, 1], up: "n".into(), ..Default::default() };
        let fit = |along: &str, ends: f64| RampFit { along: along.into(), ends, ..Default::default() };
        let middle = |p: &Put| p.m.transform_point3(Vec3::ZERO);
        // one a cell wide, one per cell; two wide, two a cell and a half each
        let one = lay(&r, &fit("repeat", 0.0), 1, 2.0, 2.0);
        assert_eq!(one.iter().map(|p| middle(p).x).collect::<Vec<f32>>(), [0.5, 1.5, 2.5]);
        assert!(one.iter().all(|p| p.along == Some(Along { half: 0.5, to: 0.5, ends: 0.0 })));
        let two = lay(&r, &fit("repeat", 0.0), 2, 2.0, 2.0);
        assert_eq!(two.iter().map(|p| middle(p).x).collect::<Vec<f32>>(), [0.75, 2.25]);
        assert!(two.iter().all(|p| p.along.is_some_and(|a| a.half == 1.0 && a.to == 0.75)));
        // one stretched three cells long, its ends kept
        let long = lay(&r, &fit("stretch", 0.1), 1, 2.0, 2.0);
        assert_eq!(long.len(), 1);
        let a = long[0].along.unwrap();
        for (x, to) in [(0.5, 1.5), (0.45, 1.45), (0.4, 1.4), (0.2, 0.7), (0.0, 0.0), (-0.45, -1.45)] {
            assert!((a.x(x) - to).abs() < 1e-5, "{x} -> {} (wanted {to})", a.x(x));
        }
        assert!((a.stretch(0.0) - 3.5).abs() < 1e-5 && a.stretch(0.45) == 1.0);
        // its foot a level under its cells, along their foot; made rising a cell, made a level
        let low = lay(&r, &RampFit { rise: 1.0, ..Default::default() }, 1, 4.0, 2.0);
        assert_eq!(low[0].point(Vec3::new(0.0, 0.0, 0.5)), Vec3::new(1.5, 2.0, 1.0));
        assert_eq!(low[0].point(Vec3::new(0.0, 1.0, -0.5)), Vec3::new(1.5, 4.0, 0.0));
        // rising east: laid along z, its foot along the west edge
        let east = Ramp { at: [5, 2], size: [1, 3], up: "e".into(), ..Default::default() };
        let put = lay(&east, &fit("stretch", 0.0), 1, 2.0, 2.0)[0];
        assert!(put.point(Vec3::new(0.5, 0.0, 0.0)).distance(Vec3::new(5.5, 0.0, 5.0)) < 1e-5);
        assert!(put.point(Vec3::new(0.0, 0.0, 0.5)).distance(Vec3::new(5.0, 0.0, 3.5)) < 1e-5);
    }

    #[test]
    fn under_what_lies_along_a_ramp_the_ground_goes_straight() {
        let mut content = with(&[slide("stretch")]);
        content.terrains.put(content::Terrain { id: "tall".into(), pattern: "tall".into(), walk: "walk".into(), color: "#4f8a3a".into(), color2: "#6aa04a".into(), ..Default::default() }, content::Space::Content);
        // the plateau's south-east corner: the ramp's east end open to the lowland; tall grass
        let mut map = crate::WorldMap::new("corner", "Corner", 10, 10, "grass");
        for z in 0..4 {
            for x in 0..5 {
                map.level[z * 10 + x] = 1;
            }
        }
        let tall = map.terrain_index("tall");
        for x in 3..5 {
            map.terrain[3 * 10 + x] = tall;
        }
        map.ramps.push(Ramp { at: [3, 3], size: [2, 1], up: "n".into(), rough: 1.0, seed: 5 });
        let ground = |m: &crate::WorldMap| crate::mesh::ground_region(m, &content, 2.0, crate::mesh::whole(m), &crate::mesh::Alone);
        let halfway = |tops: &crate::mesh::Mesh| tops.vertices.iter().filter(|v| v.uv[0] > 3.0 && v.uv[0] < 5.0 && v.uv[1] > 3.0 && v.uv[1] < 4.0 && (v.pos[2] - 3.5).abs() < 1e-5).map(|v| v.pos[1]).collect::<Vec<f32>>();
        let on_it = |grass: &crate::mesh::Mesh| grass.vertices.iter().filter(|v| v.pos[0] > 3.0 && v.pos[0] < 5.0 && v.pos[2] > 3.0 && v.pos[2] < 4.0).count();
        // the east wall's foot, down to the lowland
        let foot = |cliffs: &crate::mesh::Mesh| cliffs.vertices.iter().filter(|v| v.normal[0] > 0.99 && v.pos[1].abs() < 1e-5 && v.pos[2] > 3.0 && v.pos[2] < 4.0).map(|v| v.pos[0]).collect::<Vec<f32>>();
        // bare, uneven, grass on it, its wall along its end
        let (tops, cliffs, grass) = ground(&map);
        let bare = halfway(&tops);
        assert!(bare.iter().any(|&y| (y - 0.94).abs() > 0.01), "{bare:?}");
        assert!(on_it(&grass) > 0);
        assert!(!foot(&cliffs).is_empty() && foot(&cliffs).iter().all(|&x| (x - 5.0).abs() < 1e-5), "{:?}", foot(&cliffs));
        // a slide along it: straight from its foot to its top, a hair under the line; no grass
        // through it, its wall a hair in from the slide's end
        map.props.push(on(&map.ramps[0].clone(), "slide", 1));
        let (tops, cliffs, grass) = ground(&map);
        let straight = halfway(&tops);
        assert!(!straight.is_empty() && straight.iter().all(|&y| (y - (1.0 - SINK)).abs() < 1e-5), "{straight:?}");
        let edges: Vec<[f32; 3]> = tops.vertices.iter().filter(|v| v.uv[1] > 3.0 && v.uv[1] < 4.0 && v.uv[0] > 3.0 && v.uv[0] < 5.0).map(|v| v.pos).collect();
        assert!(edges.iter().filter(|p| p[2] == 4.0).all(|p| p[1].abs() < 1e-5) && edges.iter().filter(|p| p[2] == 3.0).all(|p| (p[1] - 2.0).abs() < 1e-5));
        assert_eq!(on_it(&grass), 0);
        assert!(!foot(&cliffs).is_empty() && foot(&cliffs).iter().all(|&x| (x - (5.0 - TUCK)).abs() < 1e-5), "{:?}", foot(&cliffs));
    }

    #[test]
    fn what_lies_along_a_ramp_covers_its_cells_and_is_walked_on() {
        let content = with(&[slide("stretch")]);
        let mut map = hill();
        let r = map.ramps[0].clone();
        map.props.push(on(&r, "slide", 1));
        // on the ramp: its cells, walked on though it blocks elsewhere, the ramp's cover
        assert_eq!(map.footprint(&map.props[0], &content), [(3, 3), (4, 3)]);
        let blocked = map.blocked(&content);
        assert!(!blocked[3 * 10 + 3] && !blocked[3 * 10 + 4]);
        assert_eq!(map.ramp_cover(&r, &content), Some(&map.props[0]));
        // a level down there, or off the ramp: a prop like any other, standing on its cell
        for at in [[4.5, 3.5], [7.5, 6.5]] {
            let p = content::Placed { prop: "slide".into(), at, level: 0, ..Default::default() };
            let mut m = hill();
            m.props.push(p.clone());
            let cell = (at[0].floor() as i32, at[1].floor() as i32);
            assert_eq!(m.footprint(&p, &content), [cell]);
            assert!(m.ramp_cover(&r, &content).is_none());
            let puts = m.puts(&p, content.props.get("slide").unwrap(), 2.0);
            assert!(puts.len() == 1 && puts[0].along.is_none());
        }
        let mut m = hill();
        m.props.push(content::Placed { prop: "slide".into(), at: [7.5, 6.5], ..Default::default() });
        assert!(m.blocked(&content)[6 * 10 + 7]);
    }
}
