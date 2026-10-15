//! Maps as the engine works on them: the content's map record decoded into a grid (a
//! terrain and a level per cell, tree areas), what stands on it, the trees the engine
//! places, which cells block, and the geometry to draw it all (`mesh`).

pub mod buildings;
pub mod gltf_in;
pub mod gltf_out;
pub mod mesh;
pub mod spawn;
pub mod trees;

use std::collections::BTreeMap;

use content::{char_level, level_char, side_step, Building, Content, Map, Placed, SpawnLink, TreeArea, Zone};

/// A map, decoded. Cells are row by row, north first: index = z * width + x.
#[derive(Clone, Debug)]
pub struct WorldMap {
    pub id: String,
    pub name: String,
    pub region: Option<String>,
    pub width: usize,
    pub height: usize,
    /// terrain ids, by index (what `terrain` holds)
    pub terrains: Vec<String>,
    pub terrain: Vec<u16>,
    pub level: Vec<u8>,
    /// 0: none, else 1 + the index in `tree_areas`
    pub area: Vec<u8>,
    pub tree_areas: Vec<TreeArea>,
    pub props: Vec<Placed>,
    pub buildings: Vec<Building>,
    /// 0: none, else 1 + the index in `zones`
    pub zone: Vec<u8>,
    pub zones: Vec<Zone>,
    pub links: Vec<SpawnLink>,
    /// the props that are doors (openings you can walk through), from `know_doors`
    pub door_props: Vec<String>,
    /// the characters the record used, so a saved map keeps its legend
    legend: BTreeMap<String, String>,
}

impl WorldMap {
    /// A new map, all of one terrain at level 0.
    pub fn new(id: &str, name: &str, width: usize, height: usize, terrain: &str) -> WorldMap {
        WorldMap {
            id: id.into(),
            name: name.into(),
            region: None,
            width,
            height,
            terrains: vec![terrain.into()],
            terrain: vec![0; width * height],
            level: vec![0; width * height],
            area: vec![0; width * height],
            tree_areas: vec![],
            props: vec![],
            buildings: vec![],
            zone: vec![0; width * height],
            zones: vec![],
            links: vec![],
            door_props: vec![],
            legend: BTreeMap::new(),
        }
    }

    pub fn from_record(m: &Map) -> WorldMap {
        let (w, h) = (m.width as usize, m.height as usize);
        let mut out = WorldMap::new(&m.id, &m.name, w, h, "");
        out.terrains.clear();
        out.region = m.region.clone();
        out.legend = m.legend.clone();
        let mut ids: BTreeMap<char, u16> = BTreeMap::new();
        for (c, t) in &m.legend {
            let Some(c) = c.chars().next() else { continue };
            let i = out.terrain_index(t);
            ids.insert(c, i);
        }
        if out.terrains.is_empty() {
            out.terrains.push("grass".into());
        }
        let rows = |rows: &[String], out: &mut Vec<u8>, f: &dyn Fn(char) -> u8| {
            for (z, row) in rows.iter().take(h).enumerate() {
                for (x, c) in row.chars().take(w).enumerate() {
                    out[z * w + x] = f(c);
                }
            }
        };
        for (z, row) in m.terrain.iter().take(h).enumerate() {
            for (x, c) in row.chars().take(w).enumerate() {
                out.terrain[z * w + x] = ids.get(&c).copied().unwrap_or(0);
            }
        }
        rows(&m.levels, &mut out.level, &|c| char_level(c).unwrap_or(0));
        let marks: BTreeMap<char, u8> = m.tree_areas.iter().enumerate().filter_map(|(i, a)| a.mark.chars().next().map(|c| (c, i as u8 + 1))).collect();
        rows(&m.areas, &mut out.area, &|c| marks.get(&c).copied().unwrap_or(0));
        out.tree_areas = m.tree_areas.clone();
        out.props = m.props.clone();
        out.buildings = m.buildings.clone();
        let zmarks: BTreeMap<char, u8> = m.zones.iter().enumerate().filter_map(|(i, z)| z.mark.chars().next().map(|c| (c, i as u8 + 1))).collect();
        rows(&m.zone_cells, &mut out.zone, &|c| zmarks.get(&c).copied().unwrap_or(0));
        out.zones = m.zones.clone();
        out.links = m.links.clone();
        out
    }

    /// Back to the record (rows of characters), keeping the characters it had.
    pub fn to_record(&self) -> Map {
        let (w, h) = (self.width, self.height);
        // a character per terrain in use: the one it had, else its first letter, else any free one
        let used: std::collections::BTreeSet<u16> = self.terrain.iter().copied().collect();
        let mut legend: BTreeMap<String, String> = BTreeMap::new();
        let mut chars: BTreeMap<u16, char> = BTreeMap::new();
        for &t in &used {
            let id = &self.terrains[t as usize];
            if let Some((c, _)) = self.legend.iter().find(|(c, v)| *v == id && !legend.contains_key(*c)) {
                legend.insert(c.clone(), id.clone());
                chars.insert(t, c.chars().next().unwrap_or('?'));
            }
        }
        for &t in &used {
            if chars.contains_key(&t) {
                continue;
            }
            let id = &self.terrains[t as usize];
            let first = id.chars().next().into_iter();
            let pool = first.chain('a'..='z').chain('A'..='Z').chain('0'..='9');
            for c in pool {
                if c != '.' && !legend.contains_key(&c.to_string()) {
                    legend.insert(c.to_string(), id.clone());
                    chars.insert(t, c);
                    break;
                }
            }
        }
        let rows = |f: &dyn Fn(usize) -> char| -> Vec<String> { (0..h).map(|z| (0..w).map(|x| f(z * w + x)).collect()).collect() };
        let marks: Vec<char> = self.tree_areas.iter().map(|a| a.mark.chars().next().unwrap_or('a')).collect();
        Map {
            id: self.id.clone(),
            name: self.name.clone(),
            region: self.region.clone(),
            width: w as u16,
            height: h as u16,
            legend,
            terrain: rows(&|i| chars.get(&self.terrain[i]).copied().unwrap_or('?')),
            levels: rows(&|i| level_char(self.level[i])),
            areas: if self.area.iter().any(|&a| a > 0) { rows(&|i| if self.area[i] == 0 { '.' } else { marks[self.area[i] as usize - 1] }) } else { vec![] },
            tree_areas: self.tree_areas.clone(),
            props: self.props.clone(),
            buildings: self.buildings.clone(),
            zone_cells: if self.zone.iter().any(|&z| z > 0) {
                let zm: Vec<char> = self.zones.iter().map(|z| z.mark.chars().next().unwrap_or('a')).collect();
                rows(&|i| if self.zone[i] == 0 { '.' } else { zm[self.zone[i] as usize - 1] })
            } else {
                vec![]
            },
            zones: self.zones.clone(),
            links: self.links.clone(),
        }
    }

    /// Learns which props are doors (for `wall_between`).
    pub fn know_doors(&mut self, content: &Content) {
        self.door_props = content.props.iter().filter(|p| p.category == "door").map(|p| p.id.clone()).collect();
    }

    /// The index of a terrain in this map, added if it's new.
    pub fn terrain_index(&mut self, id: &str) -> u16 {
        match self.terrains.iter().position(|t| t == id) {
            Some(i) => i as u16,
            None => {
                self.terrains.push(id.into());
                (self.terrains.len() - 1) as u16
            }
        }
    }

    pub fn inside(&self, x: i32, z: i32) -> bool {
        x >= 0 && z >= 0 && (x as usize) < self.width && (z as usize) < self.height
    }

    pub fn index(&self, x: i32, z: i32) -> Option<usize> {
        self.inside(x, z).then(|| z as usize * self.width + x as usize)
    }

    pub fn level_at(&self, x: i32, z: i32) -> Option<u8> {
        self.index(x, z).map(|i| self.level[i])
    }

    pub fn terrain_at(&self, x: i32, z: i32) -> Option<&str> {
        self.index(x, z).map(|i| self.terrains[self.terrain[i] as usize].as_str())
    }

    /// Changes the size, keeping what's in the north-west corner; new cells get `fill`.
    pub fn resize(&mut self, width: usize, height: usize, fill: &str) {
        let t = self.terrain_index(fill);
        let mut terrain = vec![t; width * height];
        let mut level = vec![0; width * height];
        let mut area = vec![0; width * height];
        for z in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                terrain[z * width + x] = self.terrain[z * self.width + x];
                level[z * width + x] = self.level[z * self.width + x];
                area[z * width + x] = self.area[z * self.width + x];
            }
        }
        let mut zone = vec![0; width * height];
        for z in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                zone[z * width + x] = self.zone[z * self.width + x];
            }
        }
        (self.width, self.height, self.terrain, self.level, self.area, self.zone) = (width, height, terrain, level, area, zone);
        self.props.retain(|p| p.at[0] < width as f64 && p.at[1] < height as f64);
    }

    /// A tree area's letter that's free.
    pub fn free_mark(&self) -> String {
        ('a'..='z').chain('A'..='Z').find(|c| !self.tree_areas.iter().any(|a| a.mark.starts_with(*c))).unwrap_or('#').to_string()
    }

    /// A zone's letter that's free.
    pub fn free_zone_mark(&self) -> String {
        ('a'..='z').chain('A'..='Z').find(|c| !self.zones.iter().any(|z| z.mark.starts_with(*c))).unwrap_or('#').to_string()
    }

    /// Removes a zone, its cells and its links.
    pub fn remove_zone(&mut self, index: usize) {
        if index >= self.zones.len() {
            return;
        }
        let mark = self.zones.remove(index).mark;
        self.links.retain(|l| l.zone != mark);
        let gone = index as u8 + 1;
        for z in self.zone.iter_mut() {
            if *z == gone {
                *z = 0;
            } else if *z > gone {
                *z -= 1;
            }
        }
    }

    /// Is the cell on the edge of its tree area (an area cell next to one outside it)?
    pub fn area_edge(&self, x: i32, z: i32) -> bool {
        let Some(i) = self.index(x, z) else { return false };
        let a = self.area[i];
        a > 0 && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| self.index(x + dx, z + dz).map(|j| self.area[j] != a).unwrap_or(false))
    }

    /// The treeline around an edge cell of a tree area: the edge cells of the same area
    /// that face open ground the same way, connected to it, at most `reach` cells away.
    pub fn treeline(&self, x: i32, z: i32, reach: i32) -> Vec<(i32, i32)> {
        let Some(i) = self.index(x, z) else { return vec![] };
        let a = self.area[i];
        if a == 0 {
            return vec![];
        }
        let open = |x: i32, z: i32, d: (i32, i32)| self.index(x + d.0, z + d.1).map(|j| self.area[j] != a).unwrap_or(false);
        let Some(&side) = [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().find(|&&d| open(x, z, d)) else { return vec![] };
        let mut out = vec![(x, z)];
        let mut todo = vec![(x, z)];
        while let Some((cx, cz)) = todo.pop() {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (cx + dx, cz + dz);
                let inside = self.index(n.0, n.1).map(|j| self.area[j] == a).unwrap_or(false);
                if inside && open(n.0, n.1, side) && !out.contains(&n) && (n.0 - x).abs().max((n.1 - z).abs()) <= reach {
                    out.push(n);
                    todo.push(n);
                }
            }
        }
        out.sort();
        out
    }

    /// Removes a tree area and its cells.
    pub fn remove_area(&mut self, index: usize) {
        if index >= self.tree_areas.len() {
            return;
        }
        let mark = self.tree_areas.remove(index).mark;
        self.links.retain(|l| l.area != mark);
        let gone = index as u8 + 1;
        for a in self.area.iter_mut() {
            if *a == gone {
                *a = 0;
            } else if *a > gone {
                *a -= 1;
            }
        }
    }

    /// The cells a placed prop covers (its footprint, turned by quarter turns).
    pub fn footprint(&self, p: &Placed, content: &Content) -> Vec<(i32, i32)> {
        let Some(prop) = content.props.get(&p.prop) else { return vec![] };
        let quarter = ((p.turn / 90.0).round() as i64).rem_euclid(2) == 1;
        let (fw, fd) = if quarter { (prop.footprint[1], prop.footprint[0]) } else { (prop.footprint[0], prop.footprint[1]) };
        let s = if prop.scalable() { p.scale.clamp(0.5, 1.0) } else { 1.0 };
        let (fw, fd) = (fw.max(1) as f64 * s, fd.max(1) as f64 * s);
        let x0 = (p.at[0] - fw / 2.0 + 0.01).floor() as i32;
        let z0 = (p.at[1] - fd / 2.0 + 0.01).floor() as i32;
        let x1 = (p.at[0] + fw / 2.0 - 0.01).floor() as i32;
        let z1 = (p.at[1] + fd / 2.0 - 0.01).floor() as i32;
        let mut out = vec![];
        for z in z0..=z1 {
            for x in x0..=x1 {
                out.push((x, z));
            }
        }
        out
    }

    /// Which cells the player can't walk on (on foot): tree areas, blocking terrain and
    /// water, blocking props. Cliffs are edges between cells, see `can_step`.
    pub fn blocked(&self, content: &Content) -> Vec<bool> {
        let mut out = vec![false; self.width * self.height];
        for (i, b) in out.iter_mut().enumerate() {
            let walk = content.terrains.get(&self.terrains[self.terrain[i] as usize]).map(|t| t.walk.as_str()).unwrap_or("walk");
            *b = self.area[i] > 0 || walk != "walk";
        }
        // what stands on the ground floor of buildings
        for b in &self.buildings {
            let Some(floor) = b.floors.first() else { continue };
            for p in &floor.props {
                if content.props.get(&p.prop).map(|q| q.blocks).unwrap_or(false) {
                    let placed = Placed { at: [p.at[0] + b.at[0] as f64, p.at[1] + b.at[1] as f64], level: b.level, ..p.clone() };
                    for (x, z) in self.footprint(&placed, content) {
                        if let Some(i) = self.index(x, z) {
                            out[i] = true;
                        }
                    }
                }
            }
        }
        for p in &self.props {
            if content.props.get(&p.prop).map(|q| q.blocks).unwrap_or(false) {
                for (x, z) in self.footprint(p, content) {
                    if let Some(i) = self.index(x, z) {
                        if self.level[i] == p.level {
                            out[i] = true;
                        }
                    }
                }
            }
        }
        out
    }

    /// Can the player step from one cell to the next (same level, nothing in the way)?
    pub fn can_step(&self, blocked: &[bool], from: (i32, i32), to: (i32, i32)) -> bool {
        match (self.index(from.0, from.1), self.index(to.0, to.1)) {
            (Some(a), Some(b)) => !blocked[b] && self.level[a] == self.level[b] && !self.wall_between(from, to, self.level[a]),
            _ => false,
        }
    }

    /// Is there a building's wall (and no door) between two side-by-side cells, at a level?
    pub fn wall_between(&self, a: (i32, i32), b: (i32, i32), level: u8) -> bool {
        let dir = (b.0 - a.0, b.1 - a.1);
        let side_of = |d: (i32, i32)| ["n", "e", "s", "w"].into_iter().find(|s| side_step(s) == Some(d));
        for bd in &self.buildings {
            if level < bd.level || level as usize >= bd.level as usize + bd.floors.len() {
                continue;
            }
            let floor = &bd.floors[(level - bd.level) as usize];
            let (ra, rb) = ((a.0 - bd.at[0], a.1 - bd.at[1]), (b.0 - bd.at[0], b.1 - bd.at[1]));
            // the wall segment crossed: on the inside cell, facing the outside one
            let (cell, side) = match (bd.has(ra.0, ra.1), bd.has(rb.0, rb.1)) {
                (true, false) => (ra, side_of(dir)),
                (false, true) => (rb, side_of((-dir.0, -dir.1))),
                _ => continue,
            };
            let Some(side) = side else { continue };
            let door = floor.openings.iter().any(|o| {
                o.side == side && Building::segments(o).contains(&cell) && self.door_props.contains(&o.prop)
            });
            if !door {
                return true;
            }
        }
        false
    }
}

/// A small deterministic generator (mulberry32), so procedural things come out the same.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed)
    }

    #[allow(clippy::should_implement_trait)] // a generator, not an iterator
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// "#rrggbb" to linear colour.
pub fn color(hex: &str) -> [f32; 4] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0xff00ff);
    let c = |s: u32| {
        let s = (s & 255) as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    [c(v >> 16), c(v >> 8), c(v), 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Floor, Opening};

    #[test]
    fn walls_block_and_doors_let_you_in() {
        let mut m = WorldMap::new("t", "Test", 8, 8, "grass");
        m.door_props = vec!["door".into()];
        m.buildings.push(Building {
            id: "house".into(),
            at: [2, 2],
            cells: vec!["###".into(), "###".into()],
            floors: vec![Floor { tile: "wood".into(), openings: vec![Opening { prop: "door".into(), cell: [1, 1], side: "s".into(), width: 1 }], props: vec![] }],
            ..Default::default()
        });
        let blocked = vec![false; 64];
        assert!(m.can_step(&blocked, (3, 4), (3, 3)), "in through the door");
        assert!(m.can_step(&blocked, (3, 3), (3, 4)), "and out");
        assert!(!m.can_step(&blocked, (2, 4), (2, 3)), "the wall beside it");
        assert!(!m.can_step(&blocked, (5, 2), (4, 2)), "the east wall");
        assert!(m.can_step(&blocked, (3, 3), (2, 3)), "inside");
        assert!(m.can_step(&blocked, (6, 2), (5, 2)), "outside");
    }

    #[test]
    fn a_map_round_trips() {
        let mut m = WorldMap::new("t", "Test", 5, 4, "grass");
        let sand = m.terrain_index("sand");
        m.terrain[7] = sand;
        m.level[3] = 2;
        m.tree_areas.push(TreeArea { mark: "a".into(), ..Default::default() });
        m.area[0] = 1;
        let r = m.to_record();
        assert_eq!(r.terrain[1], "ggsgg");
        assert_eq!(r.levels[0], "00020");
        assert_eq!(r.areas[0], "a....");
        let back = WorldMap::from_record(&r);
        assert_eq!(back.terrain_at(2, 1), Some("sand"));
        assert_eq!(back.level_at(3, 0), Some(2));
        assert_eq!(back.area[0], 1);
        assert_eq!(back.to_record(), r);
    }
}
