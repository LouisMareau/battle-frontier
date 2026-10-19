//! The trees of tree areas, placed by the engine: trees all along the edge (when the area
//! asks for a solid edge), then the inside, a tree tried at each spot of a fine grid (jittered)
//! over the area. Each tree keeps its room: two trunks never stand closer than their crowns let
//! them (a share of their crowns' radii together, so a wood of real-size trees is a canopy, not
//! a heap of them, and a bush needs little), inside the area not closer than its spacing
//! either. Each spot has its own dice (from the area's seed and where it is), so the same seed
//! gives the same trees, and painting a few cells changes the trees near them only.
//!
//! `Woods` keeps a map's trees with what they were placed from, so that after a change only
//! what it can reach is placed again: the spots on the cells that changed are tried again,
//! then, in the order of placing, the spots after them near a tree that came or went (it took
//! their room, or left it), until nothing more changes. The trees come out as placing them all
//! again would put them, at the cost of the change, not of the woods.

use std::collections::BTreeSet;

use content::{Content, Prop, Ramp, TreeArea};

use crate::{same, WorldMap};

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

/// When a spot is tried in its area's placing: along the edge first, cell by cell (`EDGE`, z,
/// x), then the inside, spot by spot of the fine grid (`INSIDE`, j, i).
type Order = (u8, i32, i32);
const EDGE: u8 = 0;
const INSIDE: u8 = 1;

/// The cell a point is on.
fn foot(p: [f32; 2]) -> (i32, i32) {
    (p[0].floor() as i32, p[1].floor() as i32)
}

/// What an area's trees are placed from: its settings, its place in the map's list (its
/// cells' mark, its dice) and its kinds' crowns.
#[derive(Clone, Debug, PartialEq)]
struct Plan {
    mark: u8,
    seed: u32,
    trees: Vec<String>,
    spacing: f32,
    lo: f32,
    hi: f32,
    density: f32,
    solid_edge: bool,
    /// each kind's crown, its radius at the area's tallest
    radius: Vec<f32>,
    /// the farthest a trunk keeps another away: the bins' size
    reach: f32,
    /// the inside's grid: as fine as the two nearest kinds may stand (a share of it)
    slot: f32,
}

impl Plan {
    fn new(ai: usize, area: &TreeArea, content: &Content) -> Plan {
        let spacing = (area.spacing as f32).max(0.4);
        let (lo, hi) = (area.height[0] as f32, area.height[1].max(area.height[0]) as f32);
        let radius: Vec<f32> = area.trees.iter().map(|id| content.props.get(id).map(crown).unwrap_or(1.0) * 0.5 * hi).collect();
        let reach = spacing.max(radius.iter().cloned().fold(0.0, f32::max) * 2.0 * CROWNS);
        let slot = (radius.iter().map(|&r| spacing.max(2.0 * r * CROWNS)).fold(f32::MAX, f32::min) * FINE).max(0.4);
        Plan {
            mark: ai as u8 + 1,
            seed: area.seed.wrapping_mul(2654435761).wrapping_add(ai as u32),
            trees: area.trees.clone(),
            spacing,
            lo,
            hi,
            density: area.density as f32,
            solid_edge: area.solid_edge,
            radius,
            reach,
            slot,
        }
    }

    fn pick(&self, t: f32) -> usize {
        ((t * self.trees.len() as f32) as usize).min(self.trees.len() - 1)
    }

    /// Where a spot's trunk would stand (an edge cell's somewhere near its middle, an inside
    /// spot's anywhere in it), and its dice for the rest.
    fn spot(&self, (pass, a, b): Order) -> ([f32; 2], Dice) {
        if pass == EDGE {
            let mut dice = Dice::at(self.seed, b, a, 1);
            ([b as f32 + 0.5 + (dice.next() - 0.5) * 0.24, a as f32 + 0.5 + (dice.next() - 0.5) * 0.24], dice)
        } else {
            let mut dice = Dice::at(self.seed, b, a, 2);
            ([(b as f32 + dice.next()) * self.slot, (a as f32 + dice.next()) * self.slot], dice)
        }
    }
}

/// A tree standing in an area.
#[derive(Clone, Debug, PartialEq)]
struct Stand {
    order: Order,
    /// its trunk, in cells
    p: [f32; 2],
    /// its crown's radius at the area's tallest
    r: f32,
    kind: usize,
    y: f32,
    scale: f32,
    turn: f32,
}

/// An area's trees as placed, in bins `reach` wide from the corner of its cells' box (who's
/// near a point).
struct Wood {
    plan: Plan,
    /// its cells' box: x0, z0 (in), x1, z1 (out); none without cells. With trees to place,
    /// exactly theirs (its bins' corner and its spots come from it), else one round them.
    bbox: Option<[i32; 4]>,
    cells: usize,
    w: i32,
    h: i32,
    bins: Vec<Vec<Stand>>,
}

/// A box grown to hold a cell.
fn grow(b: Option<[i32; 4]>, (x, z): (i32, i32)) -> [i32; 4] {
    match b {
        None => [x, z, x + 1, z + 1],
        Some(b) => [b[0].min(x), b[1].min(z), b[2].max(x + 1), b[3].max(z + 1)],
    }
}

impl Wood {
    /// Places an area's trees, its cells given row by row.
    fn place(plan: Plan, cells: &[(i32, i32)], map: &WorldMap, level_height: f32) -> Wood {
        let bbox = cells.iter().fold(None, |b, &c| Some(grow(b, c)));
        let mut wood = Wood { plan, bbox, cells: cells.len(), w: 0, h: 0, bins: vec![] };
        let Some([x0, z0, x1, z1]) = bbox else { return wood };
        if wood.plan.trees.is_empty() {
            return wood;
        }
        wood.w = ((x1 - x0) as f32 / wood.plan.reach).ceil() as i32 + 1;
        wood.h = ((z1 - z0) as f32 / wood.plan.reach).ceil() as i32 + 1;
        wood.bins = vec![vec![]; (wood.w * wood.h) as usize];
        if wood.plan.solid_edge {
            for &(x, z) in cells {
                if let Some(s) = wood.stand(map, level_height, (EDGE, z, x)) {
                    wood.add(s);
                }
            }
        }
        // the inside: the grid is the map's (from its corner), so its spots stay where they
        // are whatever the area's cells
        let [i0, j0, i1, j1] = wood.spots();
        for j in j0..j1 {
            for i in i0..i1 {
                if let Some(s) = wood.stand(map, level_height, (INSIDE, j, i)) {
                    wood.add(s);
                }
            }
        }
        wood
    }

    fn stands(&self) -> impl Iterator<Item = &Stand> {
        self.bins.iter().flatten()
    }

    /// Its trees, in the order they were placed.
    fn trees(&self, stands: impl Iterator<Item = Stand>) -> impl Iterator<Item = Tree> + '_ {
        let mut all: Vec<Stand> = stands.collect();
        all.sort_unstable_by_key(|s| s.order);
        all.into_iter().map(|s| Tree { prop: self.plan.trees[s.kind].clone(), pos: [s.p[0], s.y, s.p[1]], scale: s.scale, turn: s.turn })
    }

    fn of(&self, p: [f32; 2]) -> (i32, i32) {
        let [x0, z0, ..] = self.bbox.unwrap_or_default();
        (((p[0] - x0 as f32) / self.plan.reach).floor() as i32, ((p[1] - z0 as f32) / self.plan.reach).floor() as i32)
    }

    fn bin(&self, (i, j): (i32, i32)) -> Option<usize> {
        ((0..self.w).contains(&i) && (0..self.h).contains(&j)).then(|| (j * self.w + i) as usize)
    }

    fn add(&mut self, s: Stand) {
        if let Some(b) = self.bin(self.of(s.p)) {
            self.bins[b].push(s);
        }
    }

    /// The bin and place of the tree a spot holds (its trunk at `p`).
    fn find(&self, order: Order, p: [f32; 2]) -> Option<(usize, usize)> {
        let b = self.bin(self.of(p))?;
        Some((b, self.bins[b].iter().position(|s| s.order == order)?))
    }

    /// Room for a tree of crown radius `r` at `p` among those placed before `order`: no one
    /// nearer than their crowns and its together times `share`, nor than `least`.
    fn room(&self, p: [f32; 2], r: f32, share: f32, least: f32, order: Order) -> bool {
        let (i, j) = self.of(p);
        for dj in -1..=1 {
            for di in -1..=1 {
                let Some(b) = self.bin((i + di, j + dj)) else { continue };
                for s in self.bins[b].iter().filter(|s| s.order < order) {
                    let d = least.max((r + s.r) * share);
                    if (s.p[0] - p[0]).powi(2) + (s.p[1] - p[1]).powi(2) < d * d {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// The inside's spots tried: i0, j0 (in), i1, j1 (out), over its box.
    fn spots(&self) -> [i32; 4] {
        let [x0, z0, x1, z1] = self.bbox.unwrap_or_default();
        let s = self.plan.slot;
        [(x0 as f32 / s).floor() as i32, (z0 as f32 / s).floor() as i32, (x1 as f32 / s).ceil() as i32, (z1 as f32 / s).ceil() as i32]
    }

    /// The tree a spot holds, placed among the trees before it: an edge cell's (the area's
    /// with another beside it), an inside spot's (in the area, when the density lets it),
    /// where it has room.
    fn stand(&self, map: &WorldMap, level_height: f32, order: Order) -> Option<Stand> {
        let plan = &self.plan;
        let inside = |x: i32, z: i32| map.index(x, z).map(|i| map.area[i] == plan.mark).unwrap_or(false);
        let (p, mut dice) = plan.spot(order);
        let (_, a, b) = order;
        let kind;
        if order.0 == EDGE {
            if !plan.solid_edge || !inside(b, a) || [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dz)| inside(b + dx, a + dz)) {
                return None;
            }
            kind = plan.pick(dice.next());
            if !self.room(p, plan.radius[kind], EDGE_CROWNS, 0.0, order) {
                return None;
            }
        } else {
            let [i0, j0, i1, j1] = self.spots();
            if !(i0..i1).contains(&b) || !(j0..j1).contains(&a) || !inside(p[0].floor() as i32, p[1].floor() as i32) {
                return None;
            }
            kind = plan.pick(dice.next());
            let dense = dice.next() < plan.density;
            if !dense || !self.room(p, plan.radius[kind], CROWNS, plan.spacing, order) {
                return None;
            }
        }
        let level = map.level_at(p[0].floor() as i32, p[1].floor() as i32).unwrap_or(0);
        let scale = plan.lo + (plan.hi - plan.lo) * dice.next();
        Some(Stand { order, p, r: plan.radius[kind], kind, y: map.ground_y(p[0], p[1], level, level_height), scale, turn: dice.next() * 360.0 })
    }

    /// Up to date after cells came into the area and left it, its plan now `plan`: its trees
    /// placed again where that can change them, or all of them (its settings changed, its
    /// box, much of it). The cells under the trees that came or went go to `moved`.
    fn change(&mut self, plan: Plan, map: &WorldMap, level_height: f32, came: &[(i32, i32)], left: &[(i32, i32)], moved: &mut Vec<(i32, i32)>) {
        let same = plan == self.plan;
        if same && came.is_empty() && left.is_empty() {
            return;
        }
        let cells = (self.cells + came.len()).saturating_sub(left.len());
        if same && self.bins.is_empty() && (self.plan.trees.is_empty() || cells == 0) {
            // nothing to place: its box kept round its cells
            self.cells = cells;
            self.bbox = if cells == 0 { None } else { came.iter().fold(self.bbox, |b, &c| Some(grow(b, c))) };
            return;
        }
        if same && !self.bins.is_empty() && !self.box_changes(map, came, left) && (came.len() + left.len()) * 4 <= cells {
            self.cells = cells;
            let dirty: Vec<(i32, i32)> = came.iter().chain(left).copied().collect();
            self.replace(map, level_height, &dirty, moved);
            return;
        }
        // from scratch: its cells looked for in its box and round those that came
        let found = cells_in(map, came.iter().fold(self.bbox, |b, &c| Some(grow(b, c))), plan.mark);
        moved.extend(self.stands().map(|s| foot(s.p)));
        *self = Wood::place(plan, &found, map, level_height);
        moved.extend(self.stands().map(|s| foot(s.p)));
    }

    /// Whether cells coming and leaving change its box (now the area's cells on the map).
    fn box_changes(&self, map: &WorldMap, came: &[(i32, i32)], left: &[(i32, i32)]) -> bool {
        let Some([x0, z0, x1, z1]) = self.bbox else { return !came.is_empty() };
        if came.iter().any(|&(x, z)| x < x0 || z < z0 || x >= x1 || z >= z1) {
            return true;
        }
        let has = |x: i32, z: i32| map.area[z as usize * map.width + x as usize] == self.plan.mark;
        // a side of it left with none of the area's cells
        let column = |x: i32| left.iter().any(|c| c.0 == x) && !(z0..z1).any(|z| has(x, z));
        let row = |z: i32| left.iter().any(|c| c.1 == z) && !(x0..x1).any(|x| has(x, z));
        column(x0) || column(x1 - 1) || row(z0) || row(z1 - 1)
    }

    /// After the cells in `dirty` came into the area or left it (its box the same): the spots
    /// on them tried again, then in the order of placing those after a tree that came or
    /// went, near it, as long as trees come or go.
    fn replace(&mut self, map: &WorldMap, level_height: f32, dirty: &[(i32, i32)], moved: &mut Vec<(i32, i32)>) {
        let slot = self.plan.slot;
        let mut queue = BTreeSet::new();
        for &(x, z) in dirty {
            // the cell and those beside it, on the edge now or not any more
            if self.plan.solid_edge {
                for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    queue.insert((EDGE, z + dz, x + dx));
                }
            }
            // the spots whose trunk can stand on it
            for j in (z as f32 / slot).floor() as i32 - 1..=((z + 1) as f32 / slot).floor() as i32 + 1 {
                for i in (x as f32 / slot).floor() as i32 - 1..=((x + 1) as f32 / slot).floor() as i32 + 1 {
                    queue.insert((INSIDE, j, i));
                }
            }
        }
        while let Some(order) = queue.pop_first() {
            let (p, _) = self.plan.spot(order);
            let now = self.stand(map, level_height, order);
            let was = self.find(order, p);
            let flips = was.is_some() != now.is_some();
            match (was, now) {
                (Some((b, k)), None) => {
                    self.bins[b].swap_remove(k);
                }
                (None, Some(s)) => self.add(s),
                (Some((b, k)), Some(s)) => {
                    // the same tree, its ground changed under it
                    if self.bins[b][k] != s {
                        self.bins[b][k] = s;
                        moved.push(foot(p));
                    }
                }
                (None, None) => {}
            }
            if flips {
                moved.push(foot(p));
                self.wake(order, p, &mut queue);
            }
        }
    }

    /// Queues the spots after `order` that see a tree at `p` (their bins next to its): it
    /// took their room or left it.
    fn wake(&self, order: Order, p: [f32; 2], queue: &mut BTreeSet<Order>) {
        let (bi, bj) = self.of(p);
        let near = |q: [f32; 2]| {
            let (i, j) = self.of(q);
            (i - bi).abs() <= 1 && (j - bj).abs() <= 1
        };
        let ([x0, z0, ..], reach) = (self.bbox.unwrap_or_default(), self.plan.reach);
        // the cells of those bins, and one round them
        let xs = ((x0 as f32 + (bi - 1) as f32 * reach).floor() as i32 - 1, (x0 as f32 + (bi + 2) as f32 * reach).ceil() as i32 + 1);
        let zs = ((z0 as f32 + (bj - 1) as f32 * reach).floor() as i32 - 1, (z0 as f32 + (bj + 2) as f32 * reach).ceil() as i32 + 1);
        if order.0 == EDGE {
            for z in zs.0..zs.1 {
                for x in xs.0..xs.1 {
                    let o = (EDGE, z, x);
                    if o > order && near(self.plan.spot(o).0) {
                        queue.insert(o);
                    }
                }
            }
        }
        let s = self.plan.slot;
        let [i0, j0, i1, j1] = self.spots();
        for j in ((zs.0 as f32 / s).floor() as i32 - 1).max(j0)..((zs.1 as f32 / s).ceil() as i32 + 1).min(j1) {
            for i in ((xs.0 as f32 / s).floor() as i32 - 1).max(i0)..((xs.1 as f32 / s).ceil() as i32 + 1).min(i1) {
                let o = (INSIDE, j, i);
                if o > order && near(self.plan.spot(o).0) {
                    queue.insert(o);
                }
            }
        }
    }

    /// The trees on cells whose ground changed (a level, a ramp) stood on it again.
    fn lift(&mut self, map: &WorldMap, level_height: f32, cells: &[(i32, i32)], moved: &mut Vec<(i32, i32)>) {
        if self.bins.is_empty() {
            return;
        }
        for &(x, z) in cells {
            let ((i0, j0), (i1, j1)) = (self.of([x as f32, z as f32]), self.of([(x + 1) as f32, (z + 1) as f32]));
            for j in j0.max(0)..=j1.min(self.h - 1) {
                for i in i0.max(0)..=i1.min(self.w - 1) {
                    for s in self.bins[(j * self.w + i) as usize].iter_mut().filter(|s| foot(s.p) == (x, z)) {
                        let y = map.ground_y(s.p[0], s.p[1], map.level_at(x, z).unwrap_or(0), level_height);
                        if y.to_bits() != s.y.to_bits() {
                            s.y = y;
                            moved.push((x, z));
                        }
                    }
                }
            }
        }
    }
}

/// An area's cells in a region of the map (x0, z0 in, x1, z1 out), row by row.
fn cells_in(map: &WorldMap, region: Option<[i32; 4]>, mark: u8) -> Vec<(i32, i32)> {
    let mut out = vec![];
    if let Some([x0, z0, x1, z1]) = region {
        for z in z0.max(0)..z1.min(map.height as i32) {
            for x in x0.max(0)..x1.min(map.width as i32) {
                if map.area[z as usize * map.width + x as usize] == mark {
                    out.push((x, z));
                }
            }
        }
    }
    out
}

/// Every area's trees placed, its cells found in one look over the map.
fn place_all(map: &WorldMap, content: &Content, level_height: f32) -> Vec<Wood> {
    let mut cells_of: Vec<Vec<(i32, i32)>> = vec![vec![]; map.tree_areas.len()];
    for z in 0..map.height {
        for x in 0..map.width {
            let a = map.area[z * map.width + x] as usize;
            if let Some(list) = a.checked_sub(1).and_then(|a| cells_of.get_mut(a)) {
                list.push((x as i32, z as i32));
            }
        }
    }
    map.tree_areas.iter().enumerate().map(|(ai, area)| Wood::place(Plan::new(ai, area, content), &cells_of[ai], map, level_height)).collect()
}

pub fn trees(map: &WorldMap, content: &Content, level_height: f32) -> Vec<Tree> {
    place_all(map, content, level_height).iter().flat_map(|w| w.trees(w.stands().cloned())).collect()
}

/// A map's trees as placed, with what they were placed from: its cells' areas and levels, its
/// ramps, its areas' settings (`update`).
#[derive(Default)]
pub struct Woods {
    size: (usize, usize),
    level_height: f32,
    area: Vec<u8>,
    level: Vec<u8>,
    ramps: Vec<Ramp>,
    /// how many cells hold each mark (an area's, or one an area gone left)
    marks: Vec<usize>,
    woods: Vec<Wood>,
}

impl Woods {
    /// Every area's trees placed.
    pub fn new(map: &WorldMap, content: &Content, level_height: f32) -> Woods {
        let woods = place_all(map, content, level_height);
        let mut marks = vec![0; 256];
        for &a in &map.area {
            marks[a as usize] += 1;
        }
        Woods { size: (map.width, map.height), level_height, area: map.area.clone(), level: map.level.clone(), ramps: map.ramps.clone(), marks, woods }
    }

    /// Up to date with the map: for each area whose settings, cells or ground changed, the
    /// trees that change can reach placed again (all of an area's when its settings changed).
    /// The cells under the trees that came, went or moved; none when they all may have
    /// (another map, its size, the levels' height).
    pub fn update(&mut self, map: &WorldMap, content: &Content, level_height: f32) -> Option<Vec<(i32, i32)>> {
        if self.size != (map.width, map.height) || self.level_height.to_bits() != level_height.to_bits() || self.area.len() != map.area.len() || self.level.len() != map.level.len() {
            *self = Woods::new(map, content, level_height);
            return None;
        }
        // what changed, row by row: the cells that came into each area or left it, and those
        // whose ground changed (a level, a ramp's)
        let w = map.width;
        let areas = map.tree_areas.len().max(self.woods.len());
        let (mut came, mut left) = (vec![vec![]; areas], vec![vec![]; areas]);
        let mut ground = vec![];
        for z in 0..map.height {
            let row = z * w..(z + 1) * w;
            if !same(&self.area[row.clone()], &map.area[row.clone()]) {
                for (x, (&a, &b)) in self.area[row.clone()].iter().zip(&map.area[row.clone()]).enumerate() {
                    if a != b {
                        let cell = (x as i32, z as i32);
                        self.marks[a as usize] -= 1;
                        self.marks[b as usize] += 1;
                        if let Some(l) = (a as usize).checked_sub(1).and_then(|a| left.get_mut(a)) {
                            l.push(cell);
                        }
                        if let Some(c) = (b as usize).checked_sub(1).and_then(|b| came.get_mut(b)) {
                            c.push(cell);
                        }
                    }
                }
                self.area[row.clone()].copy_from_slice(&map.area[row.clone()]);
            }
            if !same(&self.level[row.clone()], &map.level[row.clone()]) {
                ground.extend(self.level[row.clone()].iter().zip(&map.level[row.clone()]).enumerate().filter(|(_, (a, b))| a != b).map(|(x, _)| (x as i32, z as i32)));
                self.level[row.clone()].copy_from_slice(&map.level[row]);
            }
        }
        if self.ramps != map.ramps {
            for r in self.ramps.iter().chain(&map.ramps) {
                ground.extend(r.cells());
            }
            self.ramps = map.ramps.clone();
        }
        let mut moved = vec![];
        // the areas gone
        for wood in self.woods.drain(map.tree_areas.len().min(self.woods.len())..) {
            moved.extend(wood.stands().map(|s| foot(s.p)));
        }
        for (ai, area) in map.tree_areas.iter().enumerate() {
            let plan = Plan::new(ai, area, content);
            if ai == self.woods.len() {
                // a new area: its cells those that came, or wherever they are when the map
                // held some of its mark already (left by an area gone)
                let region = if self.marks[ai + 1] == came[ai].len() { came[ai].iter().fold(None, |b, &c| Some(grow(b, c))) } else { Some([0, 0, w as i32, map.height as i32]) };
                let wood = Wood::place(plan, &cells_in(map, region, ai as u8 + 1), map, level_height);
                moved.extend(wood.stands().map(|s| foot(s.p)));
                self.woods.push(wood);
                continue;
            }
            self.woods[ai].change(plan, map, level_height, &came[ai], &left[ai], &mut moved);
        }
        if !ground.is_empty() {
            for wood in &mut self.woods {
                wood.lift(map, level_height, &ground, &mut moved);
            }
        }
        Some(moved)
    }

    /// The trees, area by area in the order they were placed (as `trees` gives them).
    pub fn trees(&self) -> Vec<Tree> {
        self.woods.iter().flat_map(|w| w.trees(w.stands().cloned())).collect()
    }

    /// The trees whose feet are in a region of cells (x0, z0 in, x1, z1 out), in that order.
    pub fn trees_in(&self, region: [i32; 4]) -> Vec<Tree> {
        let mut out = vec![];
        for w in self.woods.iter().filter(|w| !w.bins.is_empty()) {
            let ((i0, j0), (i1, j1)) = (w.of([region[0] as f32, region[1] as f32]), w.of([region[2] as f32, region[3] as f32]));
            let mut here = vec![];
            for j in j0.max(0)..=j1.min(w.h - 1) {
                for i in i0.max(0)..=i1.min(w.w - 1) {
                    here.extend(w.bins[(j * w.w + i) as usize].iter().filter(|s| {
                        let (x, z) = foot(s.p);
                        x >= region[0] && z >= region[1] && x < region[2] && z < region[3]
                    }));
                }
            }
            out.extend(w.trees(here.into_iter().cloned()));
        }
        out
    }

    /// How many trees stand.
    pub fn len(&self) -> usize {
        self.woods.iter().map(|w| w.stands().count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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

    /// The placing as it was before `Woods`, word for word: what the trees must still be.
    fn as_before(map: &WorldMap, content: &Content, level_height: f32) -> Vec<Tree> {
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
        let mut out = vec![];
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

    /// The tests' own dice, for their maps and edits.
    struct Rng(u32);

    impl Rng {
        fn below(&mut self, n: i32) -> i32 {
            let mut d = Dice(self.0);
            self.0 = self.0.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
            (d.next() * n as f32) as i32 % n.max(1)
        }
    }

    /// Paints a round blob of cells with a mark (0: none).
    fn blob(m: &mut WorldMap, (cx, cz): (i32, i32), r: i32, mark: u8) {
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                if (x - cx).pow(2) + (z - cz).pow(2) <= r * r {
                    if let Some(i) = m.index(x, z) {
                        m.area[i] = mark;
                    }
                }
            }
        }
    }

    /// Paints a square of cells to a level.
    fn raise(m: &mut WorldMap, (cx, cz): (i32, i32), r: i32, level: u8) {
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                if let Some(i) = m.index(x, z) {
                    m.level[i] = level;
                }
            }
        }
    }

    /// A map of three areas of odd shapes on uneven ground: big crowns and bushes, one without
    /// an edge and sparse, a kind the content doesn't know of; a ramp.
    fn odd_map(seed: u32) -> WorldMap {
        let mut rng = Rng(seed);
        let mut m = WorldMap::new("t", "Test", 70, 60, "grass");
        let kinds: [&[&str]; 3] = [&["oak", "bush"], &["bush"], &["pine", "oak"]];
        for (k, trees) in kinds.iter().enumerate() {
            let a = TreeArea { mark: format!("{k}"), trees: trees.iter().map(|t| t.to_string()).collect(), spacing: 0.8 + k as f64 * 0.3, density: [0.8, 0.5, 1.0][k], solid_edge: k != 1, seed: rng.below(1000) as u32, ..Default::default() };
            m.tree_areas.push(a);
        }
        for _ in 0..14 {
            let c = (rng.below(70), rng.below(60));
            let (r, mark) = (2 + rng.below(9), 1 + rng.below(3) as u8);
            blob(&mut m, c, r, mark);
        }
        for _ in 0..6 {
            let c = (rng.below(70), rng.below(60));
            let r = 1 + rng.below(6);
            raise(&mut m, c, r, 1 + (r % 3) as u8);
        }
        m.ramps.push(Ramp { at: [30, 20], size: [6, 1], up: "n".into(), ..Default::default() });
        m
    }

    #[test]
    fn the_trees_are_placed_as_they_were() {
        let content = woods();
        let mut m = WorldMap::new("t", "Test", 60, 60, "grass");
        area(&mut m, &["oak", "bush"]);
        assert_eq!(trees(&m, &content, 2.0), as_before(&m, &content, 2.0));
        for seed in 1..6 {
            let m = odd_map(seed);
            let all = trees(&m, &content, 2.0);
            assert!(all.len() > 50, "{} trees", all.len());
            assert_eq!(all, as_before(&m, &content, 2.0), "map {seed}");
        }
    }

    /// Every tree that came, went or moved stands on a cell said to have changed.
    fn told(before: &[Tree], after: &[Tree], moved: &Option<Vec<(i32, i32)>>) {
        let Some(cells) = moved else { return };
        let cells: BTreeSet<(i32, i32)> = cells.iter().copied().collect();
        let at = |t: &Tree| (t.pos[0].floor() as i32, t.pos[2].floor() as i32);
        for t in before.iter().filter(|t| !after.contains(t)).chain(after.iter().filter(|t| !before.contains(t))) {
            assert!(cells.contains(&at(t)), "a {} at {:?} changed unsaid", t.prop, at(t));
        }
    }

    #[test]
    fn a_change_places_again_just_what_placing_them_all_would() {
        let content = woods();
        for seed in 1..4 {
            let mut rng = Rng(seed * 77);
            let mut m = odd_map(seed);
            let mut woods = Woods::new(&m, &content, 2.0);
            assert_eq!(woods.trees(), trees(&m, &content, 2.0));
            for step in 0..60 {
                let what = rng.below(12);
                match what {
                    // a stroke of an area's cells, or of none
                    0..=5 => {
                        let mark = rng.below(m.tree_areas.len() as i32 + 1) as u8;
                        let c = (rng.below(70), rng.below(60));
                        let r = rng.below(3);
                        blob(&mut m, c, r, mark);
                    }
                    // a level painted
                    6 => {
                        let c = (rng.below(70), rng.below(60));
                        let (r, level) = (rng.below(3), rng.below(3) as u8);
                        raise(&mut m, c, r, level);
                    }
                    // a new area, painted
                    7 if m.tree_areas.len() < 6 => {
                        m.tree_areas.push(TreeArea { mark: "n".into(), trees: vec!["oak".into(), "bush".into()], seed: rng.below(500) as u32, ..Default::default() });
                        let mark = m.tree_areas.len() as u8;
                        let c = (rng.below(70), rng.below(60));
                        let r = 1 + rng.below(4);
                        blob(&mut m, c, r, mark);
                    }
                    // an area gone
                    8 if m.tree_areas.len() > 1 => {
                        let a = rng.below(m.tree_areas.len() as i32) as usize;
                        m.remove_area(a);
                    }
                    // an area's settings
                    9 => {
                        let a = rng.below(m.tree_areas.len() as i32) as usize;
                        match rng.below(3) {
                            0 => m.tree_areas[a].seed += 1,
                            1 => m.tree_areas[a].solid_edge = !m.tree_areas[a].solid_edge,
                            _ => m.tree_areas[a].name = format!("renamed {step}"),
                        }
                    }
                    // a ramp, or none
                    10 => {
                        if m.ramps.is_empty() {
                            let at = [rng.below(60), rng.below(55)];
                            m.ramps.push(Ramp { at, size: [5, 1], up: "s".into(), ..Default::default() });
                        } else {
                            m.ramps.clear();
                        }
                    }
                    // cells at the map's side, round an area's box
                    _ => {
                        let mark = 1 + rng.below(m.tree_areas.len() as i32) as u8;
                        let c = ([0, 69][rng.below(2) as usize], rng.below(60));
                        blob(&mut m, c, 1, mark);
                    }
                }
                let before = woods.trees();
                let moved = woods.update(&m, &content, 2.0);
                let after = woods.trees();
                assert_eq!(after, trees(&m, &content, 2.0), "map {seed}, step {step} ({what})");
                told(&before, &after, &moved);
            }
        }
    }

    #[test]
    fn a_stroke_in_a_big_wood_moves_the_trees_near_it_only() {
        let content = woods();
        let mut m = WorldMap::new("t", "Test", 400, 300, "grass");
        area(&mut m, &["oak", "bush"]);
        let mut woods = Woods::new(&m, &content, 2.0);
        // eight cells of another wood in the middle of it, and a level up beside them
        m.tree_areas.push(TreeArea { mark: "b".into(), trees: vec!["bush".into()], ..Default::default() });
        for x in 196..204 {
            m.area[150 * 400 + x] = 2;
            m.level[153 * 400 + x] = 1;
        }
        let moved = woods.update(&m, &content, 2.0).expect("the same map");
        assert!(!moved.is_empty());
        for (x, z) in moved {
            assert!((x - 200).abs() < 40 && (z - 150).abs() < 40, "a tree moved at {x}, {z}");
        }
        assert_eq!(woods.trees(), trees(&m, &content, 2.0));
        // and a square's trees, as all of them have them
        let square = woods.trees_in([192, 128, 256, 192]);
        let all: Vec<Tree> = trees(&m, &content, 2.0).into_iter().filter(|t| (192.0..256.0).contains(&t.pos[0]) && (128.0..192.0).contains(&t.pos[2])).collect();
        assert!(!square.is_empty() && square == all);
    }
}
