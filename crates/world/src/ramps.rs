//! Ramps' ground: down a level in one cell, from the ramp's cells' level at its top (the top of
//! a step) to the ground at its foot, easing a little into the ground at both edges, and a
//! little uneven: bumps, and its slope rounder in places and straighter in others as it runs
//! along the step; never at its foot and top, which meet the ground flush. Each ramp is uneven
//! its own way (its seed) and as much as it says (`rough`).

use std::f32::consts::PI;

use content::Ramp;
use glam::Vec3;

use crate::mesh::noise3;

/// How much a ramp's slope eases into the ground at its edges (0: straight).
const EASE: f32 = 0.35;

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
}
