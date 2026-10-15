//! The trees of tree areas, placed by the engine: a tree in every edge cell (when the area
//! asks for a solid edge), then the inside filled at random, never two trunks closer than
//! the area's spacing. The same seed always gives the same trees.

use std::collections::HashMap;

use crate::{Rng, WorldMap};

#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub prop: String,
    /// its foot, in world units
    pub pos: [f32; 3],
    pub scale: f32,
    /// degrees around the vertical axis
    pub turn: f32,
}

pub fn trees(map: &WorldMap, level_height: f32) -> Vec<Tree> {
    let mut out = vec![];
    for (ai, area) in map.tree_areas.iter().enumerate() {
        if area.trees.is_empty() {
            continue;
        }
        let mark = ai as u8 + 1;
        let inside = |x: i32, z: i32| map.index(x, z).map(|i| map.area[i] == mark).unwrap_or(false);
        let mut cells: Vec<(i32, i32)> = vec![];
        for z in 0..map.height as i32 {
            for x in 0..map.width as i32 {
                if inside(x, z) {
                    cells.push((x, z));
                }
            }
        }
        let mut rng = Rng::new(area.seed.wrapping_mul(2654435761).wrapping_add(ai as u32));
        let spacing = (area.spacing as f32).max(0.4);
        let mut grid: HashMap<(i32, i32), Vec<[f32; 2]>> = HashMap::new();
        let key = |p: [f32; 2]| ((p[0] / spacing).floor() as i32, (p[1] / spacing).floor() as i32);
        let room = |grid: &HashMap<(i32, i32), Vec<[f32; 2]>>, p: [f32; 2], least: f32| {
            let (kx, kz) = key(p);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if let Some(list) = grid.get(&(kx + dx, kz + dz)) {
                        if list.iter().any(|q| (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < least * least) {
                            return false;
                        }
                    }
                }
            }
            true
        };
        let mut add = |grid: &mut HashMap<(i32, i32), Vec<[f32; 2]>>, rng: &mut Rng, p: [f32; 2], cell: (i32, i32)| {
            grid.entry(key(p)).or_default().push(p);
            let level = map.level_at(cell.0, cell.1).unwrap_or(0) as f32;
            let prop = area.trees[((rng.next() * area.trees.len() as f32) as usize).min(area.trees.len() - 1)].clone();
            let scale = rng.range(area.height[0] as f32, area.height[1].max(area.height[0]) as f32);
            out.push(Tree { prop, pos: [p[0], level * level_height, p[1]], scale, turn: rng.next() * 360.0 });
        };
        if area.solid_edge {
            for &(x, z) in &cells {
                let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| !inside(x + dx, z + dz));
                if edge {
                    let p = [x as f32 + 0.5 + rng.range(-0.12, 0.12), z as f32 + 0.5 + rng.range(-0.12, 0.12)];
                    add(&mut grid, &mut rng, p, (x, z));
                }
            }
        }
        // the inside, in a shuffled order
        for i in (1..cells.len()).rev() {
            let j = (rng.next() * (i + 1) as f32) as usize;
            cells.swap(i, j.min(i));
        }
        for &(x, z) in &cells {
            for _ in 0..4 {
                let p = [x as f32 + rng.next(), z as f32 + rng.next()];
                if room(&grid, p, spacing) && rng.next() < area.density as f32 {
                    add(&mut grid, &mut rng, p, (x, z));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::TreeArea;

    #[test]
    fn trees_fill_the_area_the_same_way_every_time() {
        let mut m = WorldMap::new("t", "Test", 12, 10, "grass");
        m.tree_areas.push(TreeArea { mark: "a".into(), trees: vec!["pine".into()], spacing: 1.2, ..Default::default() });
        for z in 2..8 {
            for x in 2..10 {
                m.area[z * 12 + x] = 1;
            }
        }
        let a = trees(&m, 2.0);
        assert_eq!(a, trees(&m, 2.0));
        // the edge: 2 * 8 + 2 * 4 cells, all with a tree
        assert!(a.len() >= 24, "{} trees", a.len());
        for t in &a {
            assert!(t.pos[0] >= 2.0 && t.pos[0] <= 10.0 && t.pos[2] >= 2.0 && t.pos[2] <= 8.0);
        }
    }
}
