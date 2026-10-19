//! The trees of tree areas, placed by the engine: trees all along the edge (when the area
//! asks for a solid edge), then the inside, a tree tried at each spot of a fine grid (jittered)
//! over the area. Each tree keeps its room: two trunks never stand closer than their crowns let
//! them (a share of their crowns' radii together, so a wood of real-size trees is a canopy, not
//! a heap of them, and a bush needs little), inside the area not closer than its spacing
//! either. Each spot has its own dice (from the area's seed and where it is), so the same seed
//! gives the same trees, and painting a few cells changes the trees near them only.

use content::{Content, Prop};

use crate::WorldMap;

/// How close two trunks may stand inside an area, as a share of their crowns' radii
/// together: under 1, the crowns meet and overlap a little (a canopy).
const CROWNS: f32 = 0.6;
/// Along a solid edge, closer still, so the edge reads as a wall.
const EDGE_CROWNS: f32 = 0.45;
/// The spots tried inside an area, as a share of how close the two nearest kinds may stand.
const FINE: f32 = 0.7;

#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub prop: String,
    /// its foot, in world units
    pub pos: [f32; 3],
    pub scale: f32,
    /// degrees around the vertical axis
    pub turn: f32,
}

/// How wide a prop's crown is, in cells, at its own size: its stand-in's widest, else (a
/// model) a share of its height; never less than half a cell.
pub fn crown(prop: &Prop) -> f32 {
    let parts = prop.parts.iter().map(|p| 2.0 * (p.at[0].abs() + p.size[0] / 2.0).max(p.at[2].abs() + p.size[2] / 2.0)).fold(0.0f64, f64::max);
    let width = if parts > 0.0 { parts } else { prop.height * 0.45 };
    (width as f32).max(0.5)
}

/// A spot's dice: the same seed and spot, the same throws.
struct Dice(u32);

impl Dice {
    fn at(seed: u32, x: i32, z: i32, salt: u32) -> Dice {
        let mut h = seed ^ (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ salt.wrapping_mul(0xcb1a_b31f);
        h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
        h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
        Dice(h ^ (h >> 16))
    }

    /// 0 to 1 (mulberry32)
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The trees standing so far, in bins `reach` wide over a box of the map: who's near a point.
struct Bins {
    origin: [f32; 2],
    reach: f32,
    w: i32,
    h: i32,
    bins: Vec<Vec<([f32; 2], f32)>>,
}

impl Bins {
    fn new(x0: i32, z0: i32, x1: i32, z1: i32, reach: f32) -> Bins {
        let w = ((x1 - x0) as f32 / reach).ceil() as i32 + 1;
        let h = ((z1 - z0) as f32 / reach).ceil() as i32 + 1;
        Bins { origin: [x0 as f32, z0 as f32], reach, w, h, bins: vec![vec![]; (w * h) as usize] }
    }

    fn of(&self, p: [f32; 2]) -> (i32, i32) {
        (((p[0] - self.origin[0]) / self.reach).floor() as i32, ((p[1] - self.origin[1]) / self.reach).floor() as i32)
    }

    fn add(&mut self, p: [f32; 2], r: f32) {
        let (i, j) = self.of(p);
        if (0..self.w).contains(&i) && (0..self.h).contains(&j) {
            self.bins[(j * self.w + i) as usize].push((p, r));
        }
    }

    /// Room for a tree of crown radius `r` at `p`: no one nearer than their crowns and its
    /// together times `share`, nor than `least`.
    fn room(&self, p: [f32; 2], r: f32, share: f32, least: f32) -> bool {
        let (i, j) = self.of(p);
        for dj in -1..=1 {
            for di in -1..=1 {
                let (bi, bj) = (i + di, j + dj);
                if !(0..self.w).contains(&bi) || !(0..self.h).contains(&bj) {
                    continue;
                }
                for &(q, rq) in &self.bins[(bj * self.w + bi) as usize] {
                    let d = least.max((r + rq) * share);
                    if (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < d * d {
                        return false;
                    }
                }
            }
        }
        true
    }
}

pub fn trees(map: &WorldMap, content: &Content, level_height: f32) -> Vec<Tree> {
    let mut out = vec![];
    // every area's cells, in one look over the map
    let mut cells_of: Vec<Vec<(i32, i32)>> = vec![vec![]; map.tree_areas.len()];
    for z in 0..map.height {
        for x in 0..map.width {
            let a = map.area[z * map.width + x] as usize;
            if let Some(list) = a.checked_sub(1).and_then(|a| cells_of.get_mut(a)) {
                list.push((x as i32, z as i32));
            }
        }
    }
    for (ai, area) in map.tree_areas.iter().enumerate() {
        let cells = &cells_of[ai];
        if area.trees.is_empty() || cells.is_empty() {
            continue;
        }
        let mark = ai as u8 + 1;
        let inside = |x: i32, z: i32| map.index(x, z).map(|i| map.area[i] == mark).unwrap_or(false);
        let seed = area.seed.wrapping_mul(2654435761).wrapping_add(ai as u32);
        let spacing = (area.spacing as f32).max(0.4);
        let (lo, hi) = (area.height[0] as f32, area.height[1].max(area.height[0]) as f32);
        // each kind's crown, its radius at the area's tallest
        let radius: Vec<f32> = area.trees.iter().map(|id| content.props.get(id).map(crown).unwrap_or(1.0) * 0.5 * hi).collect();
        let reach = spacing.max(radius.iter().cloned().fold(0.0, f32::max) * 2.0 * CROWNS);
        let (x0, z0) = cells.iter().fold((i32::MAX, i32::MAX), |(a, b), &(x, z)| (a.min(x), b.min(z)));
        let (x1, z1) = cells.iter().fold((i32::MIN, i32::MIN), |(a, b), &(x, z)| (a.max(x + 1), b.max(z + 1)));
        let mut bins = Bins::new(x0, z0, x1, z1, reach);
        let pick = |t: f32| ((t * area.trees.len() as f32) as usize).min(area.trees.len() - 1);
        let mut put = |bins: &mut Bins, p: [f32; 2], kind: usize, dice: &mut Dice| {
            bins.add(p, radius[kind]);
            let level = map.level_at(p[0].floor() as i32, p[1].floor() as i32).unwrap_or(0);
            let scale = lo + (hi - lo) * dice.next();
            out.push(Tree { prop: area.trees[kind].clone(), pos: [p[0], map.ground_y(p[0], p[1], level, level_height), p[1]], scale, turn: dice.next() * 360.0 });
        };
        if area.solid_edge {
            for &(x, z) in cells {
                if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dz)| inside(x + dx, z + dz)) {
                    continue;
                }
                let mut dice = Dice::at(seed, x, z, 1);
                let p = [x as f32 + 0.5 + (dice.next() - 0.5) * 0.24, z as f32 + 0.5 + (dice.next() - 0.5) * 0.24];
                let kind = pick(dice.next());
                if bins.room(p, radius[kind], EDGE_CROWNS, 0.0) {
                    put(&mut bins, p, kind, &mut dice);
                }
            }
        }
        // the inside: a tree tried at each spot of a grid as fine as the two nearest kinds may
        // stand (a share of it), somewhere in the spot; the grid is the map's (from its corner),
        // so its spots stay where they are whatever the area's cells
        let slot = (radius.iter().map(|&r| spacing.max(2.0 * r * CROWNS)).fold(f32::MAX, f32::min) * FINE).max(0.4);
        let (i0, j0) = ((x0 as f32 / slot).floor() as i32, (z0 as f32 / slot).floor() as i32);
        let (i1, j1) = ((x1 as f32 / slot).ceil() as i32, (z1 as f32 / slot).ceil() as i32);
        for j in j0..j1 {
            for i in i0..i1 {
                let mut dice = Dice::at(seed, i, j, 2);
                let p = [(i as f32 + dice.next()) * slot, (j as f32 + dice.next()) * slot];
                if !inside(p[0].floor() as i32, p[1].floor() as i32) {
                    continue;
                }
                let kind = pick(dice.next());
                if dice.next() < area.density as f32 && bins.room(p, radius[kind], CROWNS, spacing) {
                    put(&mut bins, p, kind, &mut dice);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Part, Space, TreeArea};

    fn area(m: &mut WorldMap, trees: &[&str]) {
        m.tree_areas.push(TreeArea { mark: "a".into(), trees: trees.iter().map(|t| t.to_string()).collect(), spacing: 1.2, ..Default::default() });
        for z in 2..m.height - 2 {
            for x in 2..m.width - 2 {
                m.area[z * m.width + x] = 1;
            }
        }
    }

    fn woods() -> Content {
        let mut content = Content::default();
        let sphere = |w: f64, at: [f64; 3]| Part { shape: "sphere".into(), size: [w, w * 0.8, w], at, color: "#4c9a3e".into(), turn: 0.0 };
        content.props.put(Prop { id: "oak".into(), category: "tree".into(), footprint: [1, 1], height: 10.0, parts: vec![sphere(6.4, [0.0, 3.4, 0.0]), sphere(3.8, [-1.1, 5.4, -0.8])], ..Default::default() }, Space::Content);
        content.props.put(Prop { id: "bush".into(), category: "flora".into(), footprint: [1, 1], height: 0.8, parts: vec![sphere(1.0, [0.0, 0.0, 0.0])], ..Default::default() }, Space::Content);
        content
    }

    #[test]
    fn trees_fill_the_area_the_same_way_every_time() {
        let mut m = WorldMap::new("t", "Test", 12, 10, "grass");
        area(&mut m, &["pine"]);
        let content = Content::default();
        let a = trees(&m, &content, 2.0);
        assert_eq!(a, trees(&m, &content, 2.0));
        // the edge: 2 * 8 + 2 * 4 cells, all with a tree (a crown the content doesn't know of
        // is a cell wide)
        assert!(a.len() >= 24, "{} trees", a.len());
        for t in &a {
            assert!(t.pos[0] >= 2.0 && t.pos[0] <= 10.0 && t.pos[2] >= 2.0 && t.pos[2] <= 8.0);
        }
    }

    #[test]
    fn big_crowns_keep_their_room_and_bushes_fill_between() {
        let content = woods();
        // the oak's crown: its widest part, off its trunk a little
        assert!((crown(content.props.get("oak").unwrap()) - 6.4).abs() < 1e-4);
        let mut m = WorldMap::new("t", "Test", 40, 40, "grass");
        area(&mut m, &["oak", "bush"]);
        let all = trees(&m, &content, 2.0);
        let oaks: Vec<&Tree> = all.iter().filter(|t| t.prop == "oak").collect();
        let bushes = all.len() - oaks.len();
        // no two oaks nearer than their crowns let them (the tallest's: 6.4 x 1.15 x 0.45 on
        // the edge), bushes in between
        for (i, a) in oaks.iter().enumerate() {
            for b in &oaks[i + 1..] {
                let d = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[2] - b.pos[2]).powi(2)).sqrt();
                assert!(d >= 6.4 * 1.15 * EDGE_CROWNS - 1e-3, "two oaks {d} apart");
            }
        }
        assert!(oaks.len() > 10 && oaks.len() < 200 && bushes > oaks.len(), "{} oaks, {bushes} bushes", oaks.len());
    }

    #[test]
    fn a_few_cells_painted_change_the_trees_near_them_only() {
        let content = woods();
        let mut m = WorldMap::new("t", "Test", 60, 60, "grass");
        area(&mut m, &["oak", "bush"]);
        let before = trees(&m, &content, 2.0);
        // a corner of the area taken out
        for z in 50..58 {
            for x in 50..58 {
                m.area[z * 60 + x] = 0;
            }
        }
        let after = trees(&m, &content, 2.0);
        // far from it (twice the reach of the biggest crowns, and more), every tree as it was
        let far = |t: &&Tree| t.pos[0] < 30.0 && t.pos[2] < 30.0;
        let (a, b): (Vec<&Tree>, Vec<&Tree>) = (before.iter().filter(far).collect(), after.iter().filter(far).collect());
        assert!(!a.is_empty() && a == b, "{} trees far from it before, {} after", a.len(), b.len());
        assert_ne!(before, after);
    }
}
