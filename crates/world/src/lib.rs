//! Maps as the engine works on them: the content's map record decoded into a grid (a
//! terrain and a level per cell, tree areas), what stands on it, the trees the engine
//! places, which cells block, and the geometry to draw it all (`mesh`).

pub mod anim;
pub mod buildings;
pub mod events;
pub mod fx;
pub mod timeline;
pub mod gltf_in;
pub mod gltf_out;
pub mod lights;
pub mod mesh;
pub mod railings;
pub mod ramps;
pub mod sounds;
pub mod spawn;
pub mod templates;
pub mod trees;

use std::collections::BTreeMap;

use content::{side_step, Actor, Ambience, Building, Cells, Chunk, Content, Event, Map, Placed, Ramp, SpawnLink, TreeArea, Zone};

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
    /// rows of cells along the top of a step, going down a level (`ramps`)
    pub ramps: Vec<Ramp>,
    /// 0: none, else 1 + the index in `zones`
    pub zone: Vec<u8>,
    pub zones: Vec<Zone>,
    pub links: Vec<SpawnLink>,
    pub actors: Vec<Actor>,
    pub events: Vec<Event>,
    pub ambience: Vec<Ambience>,
    /// the props that are doors (openings you can walk through), from `know_doors`
    pub door_props: Vec<String>,
    /// the characters the record used, so a saved map keeps its legend
    legend: BTreeMap<String, String>,
    /// stored as chunk files this many cells a side (0: as rows in the record)
    pub chunk: u16,
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
            ramps: vec![],
            zone: vec![0; width * height],
            zones: vec![],
            links: vec![],
            actors: vec![],
            events: vec![],
            ambience: vec![],
            door_props: vec![],
            legend: BTreeMap::new(),
            chunk: 0,
        }
    }

    pub fn from_record(m: &Map) -> WorldMap {
        WorldMap::from_chunks(m, &[])
    }

    /// From the record and, for a chunked map, its chunk files (a square missing is blank).
    pub fn from_chunks(m: &Map, chunks: &[Chunk]) -> WorldMap {
        let cells = m.cells(chunks);
        WorldMap {
            id: m.id.clone(),
            name: m.name.clone(),
            region: m.region.clone(),
            width: m.width as usize,
            height: m.height as usize,
            terrains: cells.terrains,
            terrain: cells.terrain,
            level: cells.level,
            area: cells.area,
            tree_areas: m.tree_areas.clone(),
            props: m.props.clone(),
            buildings: m.buildings.clone(),
            ramps: m.ramps.clone(),
            zone: cells.zone,
            zones: m.zones.clone(),
            links: m.links.clone(),
            actors: m.actors.clone(),
            events: m.events.clone(),
            ambience: m.ambience.clone(),
            door_props: vec![],
            legend: m.legend.clone(),
            chunk: m.chunk,
        }
    }

    /// The record: rows of characters, or for a chunked map the header alone (its cells
    /// are `to_chunks`).
    pub fn to_record(&self) -> Map {
        self.records().0
    }

    /// A chunked map's chunk files (blank squares left out); none for a map kept as rows.
    pub fn to_chunks(&self) -> Vec<Chunk> {
        self.records().1
    }

    /// The record and the chunk files, as the map is stored.
    pub fn records(&self) -> (Map, Vec<Chunk>) {
        let header = Map {
            id: self.id.clone(),
            name: self.name.clone(),
            region: self.region.clone(),
            width: self.width as u16,
            height: self.height as u16,
            chunk: self.chunk,
            legend: self.legend.clone(),
            tree_areas: self.tree_areas.clone(),
            props: self.props.clone(),
            buildings: self.buildings.clone(),
            ramps: self.ramps.clone(),
            zones: self.zones.clone(),
            links: self.links.clone(),
            actors: self.actors.clone(),
            events: self.events.clone(),
            ambience: self.ambience.clone(),
            ..Default::default()
        };
        let cells = Cells { terrains: self.terrains.clone(), terrain: self.terrain.clone(), level: self.level.clone(), area: self.area.clone(), zone: self.zone.clone() };
        if self.chunk > 0 {
            header.with_chunks(&cells)
        } else {
            (header.with_rows(&cells), vec![])
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
        // a ramp the map no longer holds goes
        self.ramps.retain(|r| r.at[0] + r.size[0] as i32 <= width as i32 && r.at[1] + r.size[1] as i32 <= height as i32);
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
        let s = if prop.scalable() { p.scale.clamp(0.5, 1.0) } else { 1.0 };
        content::footprint_cells(p.at, p.turn, prop.footprint, s)
    }

    /// Which cells the player can't walk on (on foot): tree areas, blocking terrain and
    /// water, blocking props. Cliffs are edges between cells, see `can_step`.
    pub fn blocked(&self, content: &Content) -> Vec<bool> {
        let mut out = vec![false; self.width * self.height];
        for (i, b) in out.iter_mut().enumerate() {
            let walk = content.terrains.get(&self.terrains[self.terrain[i] as usize]).map(|t| t.walk.as_str()).unwrap_or("walk");
            *b = self.area[i] > 0 || walk != "walk";
        }
        // what stands on the ground floor of buildings, and outside them
        for b in &self.buildings {
            let Some(floor) = b.floors.first() else { continue };
            for p in floor.props.iter().chain(&b.outside) {
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

    /// Can the player step from one cell to the next (the ground lets them, nothing in the
    /// way)?
    pub fn can_step(&self, blocked: &[bool], from: (i32, i32), to: (i32, i32)) -> bool {
        match (self.index(from.0, from.1), self.index(to.0, to.1)) {
            (Some(a), Some(b)) => !blocked[b] && self.ground_steps(from, to) && !self.wall_between(from, to, self.level[a]),
            _ => false,
        }
    }

    /// Where a placed prop stands: its place, on the ground of its level there (a ramp's
    /// slope too).
    pub fn prop_foot(&self, p: &Placed, level_height: f32) -> glam::Vec3 {
        let pos = p.pos();
        let (x, z) = (pos[0] as f32, pos[1] as f32);
        glam::Vec3::new(x, self.ground_y(x, z, p.level, level_height), z)
    }

    /// The ramp a cell is on, if it's on one.
    pub fn ramp_at(&self, x: i32, z: i32) -> Option<&Ramp> {
        self.ramps.iter().find(|r| r.has(x, z))
    }

    /// The ground's height at a point (in cells) of a cell on `level`: the level's, or where a
    /// ramp goes down from that level, the ramp's there.
    pub fn ground_y(&self, px: f32, pz: f32, level: u8, level_height: f32) -> f32 {
        let (x, z) = (px.floor() as i32, pz.floor() as i32);
        let base = level as f32 * level_height;
        match self.ramp_at(x, z) {
            Some(r) if self.level_at(x, z) == Some(level) => ramps::height(r, base, level_height, px, pz),
            _ => base,
        }
    }

    /// How high a cell's ground is, in levels, halfway along its side towards (dx, dz): its
    /// level, or on a ramp the ramp's (smooth) height there, a level down at its foot.
    pub fn edge_level(&self, x: i32, z: i32, (dx, dz): (i32, i32)) -> Option<f32> {
        let l = self.level_at(x, z)? as f32;
        Some(match self.ramp_at(x, z) {
            Some(r) => l - 1.0 + ramps::profile(r.along(x as f64 + 0.5 + dx as f64 * 0.5, z as f64 + 0.5 + dz as f64 * 0.5).0 as f32),
            None => l,
        })
    }

    /// Does the ground let one step from a cell to the next: level with it, or up and down a
    /// ramp, from its foot to its top and along it (never off its sides, half a level from the
    /// ground beside)?
    pub fn ground_steps(&self, from: (i32, i32), to: (i32, i32)) -> bool {
        let d = (to.0 - from.0, to.1 - from.1);
        match (self.edge_level(from.0, from.1, d), self.edge_level(to.0, to.1, (-d.0, -d.1))) {
            (Some(a), Some(b)) => (a - b).abs() < 0.5,
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

/// A linear colour as the records write it: "#rrggbb" (sRGB).
pub fn hex_color(c: [f32; 3]) -> String {
    let s = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let v = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round() as u8
    };
    format!("#{:02x}{:02x}{:02x}", s(c[0]), s(c[1]), s(c[2]))
}

/// A map from the content, its chunk files read from disk under `root` when it's chunked.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_map(root: &std::path::Path, content: &Content, id: &str) -> Result<WorldMap, String> {
    let e = content.maps.entry(id).ok_or_else(|| format!("no map {id}"))?;
    let chunks = if e.value.chunked() { content::disk::read_chunks(root, e.space, &e.value)? } else { vec![] };
    let mut m = WorldMap::from_chunks(&e.value, &chunks);
    m.know_doors(content);
    Ok(m)
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
            floors: vec![Floor { tile: "wood".into(), openings: vec![Opening { prop: "door".into(), cell: [1, 1], side: "s".into(), width: 1 }], ..Default::default() }],
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

    #[test]
    fn a_chunked_map_round_trips() {
        let mut m = WorldMap::new("big", "Big", 150, 100, "grass");
        let sand = m.terrain_index("sand");
        let rock = m.terrain_index("rock");
        for z in 0..100 {
            for x in 0..150 {
                let i = z * 150 + x;
                if (x * 7 + z * 3) % 11 == 0 {
                    m.terrain[i] = sand;
                }
                if x > 120 && z > 70 {
                    m.terrain[i] = rock;
                    m.level[i] = 12;
                }
            }
        }
        m.tree_areas.push(TreeArea { mark: "a".into(), ..Default::default() });
        m.area[0] = 1;
        m.zones.push(Zone { mark: "z".into(), ..Default::default() });
        m.zone[150 * 99 + 149] = 1;
        m.chunk = content::CHUNK;
        let (header, chunks) = m.records();
        assert_eq!(header.chunk, 64);
        assert!(header.terrain.is_empty() && header.levels.is_empty() && header.legend.is_empty());
        assert_eq!(header.terrains, vec!["grass", "sand", "rock"]);
        assert_eq!(header.chunk_grid(), (3, 2));
        assert_eq!(chunks.len(), 6);
        let back = WorldMap::from_chunks(&header, &chunks);
        let grids = [
            ("terrain", back.terrain.iter().map(|&v| v as u32).collect::<Vec<u32>>(), m.terrain.iter().map(|&v| v as u32).collect::<Vec<u32>>()),
            ("level", back.level.iter().map(|&v| v as u32).collect(), m.level.iter().map(|&v| v as u32).collect()),
            ("area", back.area.iter().map(|&v| v as u32).collect(), m.area.iter().map(|&v| v as u32).collect()),
            ("zone", back.zone.iter().map(|&v| v as u32).collect(), m.zone.iter().map(|&v| v as u32).collect()),
        ];
        for (name, got, want) in &grids {
            let first = got.iter().zip(want).position(|(a, b)| a != b);
            assert!(first.is_none() && got.len() == want.len(), "{name} differs first at cell {first:?} (x {}, z {}): {:?} instead of {:?}", first.map(|i| i % 150).unwrap_or(0), first.map(|i| i / 150).unwrap_or(0), first.map(|i| got[i]), first.map(|i| want[i]));
        }
        assert_eq!(back.level_at(149, 99), Some(12), "levels past 9 need no letters");
        assert_eq!(back.chunk, 64);
        // a square left out is blank
        let partial: Vec<Chunk> = chunks.iter().filter(|c| c.chunk != [0, 0]).cloned().collect();
        let sparse = WorldMap::from_chunks(&header, &partial);
        assert_eq!(sparse.terrain_at(0, 0), Some("grass"));
        assert_eq!(sparse.terrain_at(149, 99), Some("rock"));
        // back to rows: the same cells
        let mut rows = back.clone();
        rows.chunk = 0;
        let r = rows.to_record();
        assert_eq!(r.terrain.len(), 100);
        let again = WorldMap::from_record(&r);
        let same = (0..150 * 100).all(|i| again.terrains[again.terrain[i] as usize] == m.terrains[m.terrain[i] as usize]);
        assert!(same, "the rows hold the same terrains (their numbers may differ: the legend orders them)");
    }
}
