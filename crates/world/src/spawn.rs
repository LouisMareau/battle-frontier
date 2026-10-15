//! Creatures roaming the map. Each zone keeps up to its `count` creatures: ones that
//! appear inside it, and ones that come out of a linked treeline, walk to the zone, roam
//! there for a while, then walk back into the trees. They move freely (not cell by cell),
//! from point to point, along paths that go around what blocks (trees, water, cliffs,
//! props, walls), and stay inside their zone while roaming. The same seed gives the same
//! creatures doing the same things.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use glam::Vec2;

use crate::{Rng, WorldMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// showing up inside its zone
    Appearing,
    /// walking from its treeline to its zone
    Coming,
    Roaming,
    Resting,
    /// walking back to its treeline
    Leaving,
    Vanishing,
}

#[derive(Clone, Debug)]
pub struct Roamer {
    pub id: u32,
    pub creature: String,
    pub level: u8,
    /// on the ground, in cells (x, z)
    pub pos: Vec2,
    pub zone: usize,
    pub link: Option<usize>,
    pub phase: Phase,
    /// 0-1: how much of it is there (it grows in and shrinks away)
    pub presence: f32,
    /// the way it last moved
    pub heading: Vec2,
    pub moving: bool,
    /// where it's going next (the debug view draws it)
    pub path: Vec<Vec2>,
    until: f64,
    leave_at: f64,
    home: Option<(i32, i32)>,
    speed: f32,
}

/// Who can appear in each zone right now (by zone index): (creature, level).
pub type Pools = Vec<Vec<(String, u8)>>;

pub struct Spawner {
    pub roamers: Vec<Roamer>,
    pub time: f64,
    seed: u32,
    rng: Rng,
    next_zone: Vec<f64>,
    next_link: Vec<f64>,
    next_id: u32,
}

#[derive(PartialEq)]
struct Node(f32, usize);

impl Eq for Node {}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}

/// A path of cells from `from` to `to` (both included), through cells `ok` lets it step
/// into (`ok(from_cell, to_cell)`), diagonals only where both sides are open.
pub fn find_path(map: &WorldMap, from: (i32, i32), to: (i32, i32), ok: &dyn Fn((i32, i32), (i32, i32)) -> bool) -> Option<Vec<(i32, i32)>> {
    let w = map.width as i32;
    let idx = |c: (i32, i32)| (c.1 * w + c.0) as usize;
    if !map.inside(from.0, from.1) || !map.inside(to.0, to.1) {
        return None;
    }
    let h = |c: (i32, i32)| {
        let (dx, dz) = ((c.0 - to.0).abs() as f32, (c.1 - to.1).abs() as f32);
        dx.max(dz) + 0.414 * dx.min(dz)
    };
    let mut open = BinaryHeap::new();
    let mut came: HashMap<usize, (i32, i32)> = HashMap::new();
    let mut cost: HashMap<usize, f32> = HashMap::new();
    cost.insert(idx(from), 0.0);
    open.push(Node(h(from), idx(from)));
    let cell_of = |i: usize| ((i as i32) % w, (i as i32) / w);
    let mut steps = 0;
    while let Some(Node(_, i)) = open.pop() {
        let c = cell_of(i);
        if c == to {
            let mut path = vec![c];
            let mut cur = c;
            while let Some(&p) = came.get(&idx(cur)) {
                path.push(p);
                cur = p;
            }
            path.reverse();
            return Some(path);
        }
        steps += 1;
        if steps > 20_000 {
            return None;
        }
        let g = cost[&i];
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let n = (c.0 + dx, c.1 + dz);
            if !map.inside(n.0, n.1) || !ok(c, n) {
                continue;
            }
            if dx != 0 && dz != 0 && !(ok(c, (c.0 + dx, c.1)) && ok(c, (c.0, c.1 + dz))) {
                continue;
            }
            let step = if dx != 0 && dz != 0 { 1.414 } else { 1.0 };
            let ng = g + step;
            if cost.get(&idx(n)).map(|&old| ng < old).unwrap_or(true) {
                cost.insert(idx(n), ng);
                came.insert(idx(n), c);
                open.push(Node(ng + h(n), idx(n)));
            }
        }
    }
    None
}

fn centre(c: (i32, i32)) -> Vec2 {
    Vec2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5)
}

impl Spawner {
    pub fn new(seed: u32) -> Spawner {
        Spawner { roamers: vec![], time: 0.0, seed, rng: Rng::new(seed), next_zone: vec![], next_link: vec![], next_id: 1 }
    }

    /// Starts over (the map's zones or links changed).
    pub fn reset(&mut self) {
        *self = Spawner::new(self.seed);
    }

    fn in_zone(&self, zone: usize) -> usize {
        self.roamers.iter().filter(|r| r.zone == zone && !matches!(r.phase, Phase::Leaving | Phase::Vanishing)).count()
    }

    fn pick<'a>(&mut self, pool: &'a [(String, u8)]) -> &'a (String, u8) {
        &pool[((self.rng.next() * pool.len() as f32) as usize).min(pool.len() - 1)]
    }

    /// Path points from a cell path: cell centres a little off, the last one exact.
    fn waypoints(&mut self, cells: &[(i32, i32)], last: Vec2) -> Vec<Vec2> {
        let mut out: Vec<Vec2> = cells.iter().skip(1).map(|&c| centre(c) + Vec2::new(self.rng.range(-0.22, 0.22), self.rng.range(-0.22, 0.22))).collect();
        if let Some(l) = out.last_mut() {
            *l = last;
        }
        out
    }

    fn zone_cells(map: &WorldMap, blocked: &[bool], zone: usize) -> Vec<(i32, i32)> {
        let mut out = vec![];
        for z in 0..map.height as i32 {
            for x in 0..map.width as i32 {
                let i = map.index(x, z).unwrap();
                if map.zone[i] as usize == zone + 1 && !blocked[i] {
                    out.push((x, z));
                }
            }
        }
        out
    }

    /// A step of `dt` seconds.
    pub fn step(&mut self, map: &WorldMap, blocked: &[bool], pools: &Pools, dt: f64) {
        self.time += dt;
        let now = self.time;
        self.next_zone.resize(map.zones.len(), 1.0);
        self.next_link.resize(map.links.len(), 0.5);
        let walk = |a: (i32, i32), b: (i32, i32), goal: (i32, i32)| -> bool {
            let (Some(ia), Some(ib)) = (map.index(a.0, a.1), map.index(b.0, b.1)) else { return false };
            (b == goal || !blocked[ib]) && map.level[ia] == map.level[ib] && !map.wall_between(a, b, map.level[ia])
        };
        // creatures showing up inside their zones
        for (zi, zone) in map.zones.iter().enumerate() {
            let pool = pools.get(zi).map(|p| p.as_slice()).unwrap_or(&[]);
            if !zone.appear_inside || pool.is_empty() || now < self.next_zone[zi] || self.in_zone(zi) >= zone.count as usize {
                continue;
            }
            self.next_zone[zi] = now + self.rng.range(2.0, 5.0) as f64;
            let cells = Self::zone_cells(map, blocked, zi);
            if cells.is_empty() {
                continue;
            }
            let c = cells[((self.rng.next() * cells.len() as f32) as usize).min(cells.len() - 1)];
            let (creature, level) = self.pick(pool).clone();
            let stay = self.rng.range(zone.stay[0] as f32, zone.stay[1].max(zone.stay[0]) as f32) as f64;
            let pos = centre(c) + Vec2::new(self.rng.range(-0.3, 0.3), self.rng.range(-0.3, 0.3));
            self.roamers.push(Roamer {
                id: self.next_id, creature, level, pos, zone: zi, link: None, phase: Phase::Appearing, presence: 0.0,
                heading: Vec2::Y, moving: false, path: vec![], until: 0.0, leave_at: now + stay, home: None, speed: self.rng.range(1.1, 2.0),
            });
            self.next_id += 1;
        }
        // creatures coming out of treelines
        for (li, link) in map.links.iter().enumerate() {
            let Some(zi) = map.zones.iter().position(|z| z.mark == link.zone) else { continue };
            let pool = pools.get(zi).map(|p| p.as_slice()).unwrap_or(&[]);
            if pool.is_empty() || link.treeline.is_empty() || now < self.next_link[li] || self.in_zone(zi) >= map.zones[zi].count as usize {
                continue;
            }
            self.next_link[li] = now + link.every.max(0.5) * self.rng.range(0.7, 1.3) as f64;
            let t = link.treeline[((self.rng.next() * link.treeline.len() as f32) as usize).min(link.treeline.len() - 1)];
            let home = (t[0], t[1]);
            let cells = Self::zone_cells(map, blocked, zi);
            if cells.is_empty() {
                continue;
            }
            let goal = cells[((self.rng.next() * cells.len() as f32) as usize).min(cells.len() - 1)];
            let Some(path) = find_path(map, home, goal, &|a, b| walk(a, b, goal)) else { continue };
            let (creature, level) = self.pick(pool).clone();
            let stay = self.rng.range(link.stay[0] as f32, link.stay[1].max(link.stay[0]) as f32) as f64;
            let last = centre(goal) + Vec2::new(self.rng.range(-0.3, 0.3), self.rng.range(-0.3, 0.3));
            let path = self.waypoints(&path, last);
            self.roamers.push(Roamer {
                id: self.next_id, creature, level, pos: centre(home), zone: zi, link: Some(li), phase: Phase::Coming, presence: 0.0,
                heading: Vec2::Y, moving: true, path, until: 0.0, leave_at: stay, home: Some(home), speed: self.rng.range(1.6, 2.4),
            });
            self.next_id += 1;
        }
        // everyone moves
        for k in 0..self.roamers.len() {
            let mut r = self.roamers[k].clone();
            match r.phase {
                Phase::Appearing => {
                    r.presence = (r.presence + dt as f32 / 0.6).min(1.0);
                    if r.presence >= 1.0 {
                        r.phase = Phase::Resting;
                        r.until = now + self.rng.range(0.4, 1.5) as f64;
                    }
                }
                Phase::Vanishing => r.presence -= dt as f32 / 0.6,
                Phase::Resting => {
                    r.moving = false;
                    if now >= r.until {
                        let cell = (r.pos.x.floor() as i32, r.pos.y.floor() as i32);
                        if now >= r.leave_at {
                            match r.home {
                                Some(home) => match find_path(map, cell, home, &|a, b| walk(a, b, home)) {
                                    Some(path) => {
                                        r.path = self.waypoints(&path, centre(home));
                                        r.phase = Phase::Leaving;
                                    }
                                    None => r.phase = Phase::Vanishing,
                                },
                                None => r.phase = Phase::Vanishing,
                            }
                        } else {
                            // somewhere else in the zone, not too far
                            let cells: Vec<(i32, i32)> = Self::zone_cells(map, blocked, r.zone).into_iter().filter(|c| (c.0 - cell.0).abs().max((c.1 - cell.1).abs()) <= 5).collect();
                            if let Some(&goal) = cells.get(((self.rng.next() * cells.len() as f32) as usize).min(cells.len().saturating_sub(1))) {
                                let zone = r.zone as u8 + 1;
                                let inside = |a: (i32, i32), b: (i32, i32)| walk(a, b, (-1, -1)) && map.index(b.0, b.1).map(|i| map.zone[i] == zone).unwrap_or(false);
                                if let Some(path) = find_path(map, cell, goal, &inside) {
                                    let last = centre(goal) + Vec2::new(self.rng.range(-0.35, 0.35), self.rng.range(-0.35, 0.35));
                                    r.path = self.waypoints(&path, last);
                                    r.phase = Phase::Roaming;
                                }
                            }
                            if r.phase == Phase::Resting {
                                r.until = now + self.rng.range(0.5, 1.5) as f64;
                            }
                        }
                    }
                }
                Phase::Coming | Phase::Roaming | Phase::Leaving => {
                    if r.phase == Phase::Coming {
                        r.presence = (r.presence + dt as f32 / 0.4).min(1.0);
                    }
                    let mut budget = r.speed * dt as f32;
                    while budget > 0.0 {
                        let Some(&target) = r.path.first() else { break };
                        let d = target - r.pos;
                        let len = d.length();
                        if len <= budget {
                            r.pos = target;
                            budget -= len;
                            r.path.remove(0);
                        } else {
                            r.heading = d / len;
                            r.pos += r.heading * budget;
                            budget = 0.0;
                        }
                    }
                    r.moving = !r.path.is_empty();
                    if r.path.is_empty() {
                        match r.phase {
                            Phase::Coming => {
                                r.phase = Phase::Resting;
                                // a linked creature's stay starts when it arrives
                                r.leave_at += now;
                                r.until = now + self.rng.range(0.5, 2.0) as f64;
                            }
                            Phase::Roaming => {
                                r.phase = Phase::Resting;
                                r.until = now + self.rng.range(0.8, 3.5) as f64;
                            }
                            _ => r.phase = Phase::Vanishing,
                        }
                    }
                }
            }
            self.roamers[k] = r;
        }
        self.roamers.retain(|r| r.presence > 0.0 || r.phase != Phase::Vanishing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{SpawnLink, TreeArea, Zone};

    #[test]
    fn creatures_come_out_of_the_treeline_and_roam_their_zone() {
        let mut m = WorldMap::new("t", "Test", 20, 12, "grass");
        m.tree_areas.push(TreeArea { mark: "a".into(), trees: vec!["pine".into()], ..Default::default() });
        for z in 0..12 {
            for x in 0..3 {
                m.area[z * 20 + x] = 1;
            }
        }
        m.zones.push(Zone { mark: "a".into(), count: 3, appear_inside: false, stay: [5.0, 6.0], ..Default::default() });
        for z in 3..9 {
            for x in 12..18 {
                m.zone[z * 20 + x] = 1;
            }
        }
        let treeline = m.treeline(2, 5, 3);
        assert!(treeline.len() >= 5 && treeline.iter().all(|c| c.0 == 2), "{treeline:?}");
        m.links.push(SpawnLink { area: "a".into(), treeline: treeline.iter().map(|c| [c.0, c.1]).collect(), zone: "a".into(), every: 1.0, stay: [5.0, 6.0] });
        let blocked: Vec<bool> = m.area.iter().map(|&a| a > 0).collect();
        let pools: Pools = vec![vec![("critter".into(), 5)]];
        let mut s = Spawner::new(3);
        let mut seen_in_zone = false;
        let mut went_home = false;
        for _ in 0..(60 * 40) {
            s.step(&m, &blocked, &pools, 1.0 / 60.0);
            assert!(s.roamers.iter().filter(|r| !matches!(r.phase, Phase::Leaving | Phase::Vanishing)).count() <= 3);
            for r in &s.roamers {
                let cell = (r.pos.x.floor() as i32, r.pos.y.floor() as i32);
                if r.phase == Phase::Roaming || r.phase == Phase::Resting {
                    let i = m.index(cell.0, cell.1).unwrap();
                    assert_eq!(m.zone[i], 1, "roaming outside its zone at {:?}", r.pos);
                    seen_in_zone = true;
                }
                went_home |= r.phase == Phase::Leaving;
            }
        }
        assert!(seen_in_zone, "nobody reached the zone");
        assert!(went_home, "nobody went back to the trees");
        // they move freely: not on cell centres
        assert!(s.roamers.iter().any(|r| (r.pos.x.fract() - 0.5).abs() > 0.01));
    }
}
