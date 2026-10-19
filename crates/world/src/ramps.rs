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
            Put { m, along: Some(Along { half: width / 2.0, to: each / 2.0, ends: fit.ends as f32 }) }
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
            ramp: Some(RampFit { along: along.into(), ..Default::default() }),
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
