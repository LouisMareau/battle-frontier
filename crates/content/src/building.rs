//! Buildings on maps: a footprint of cells (one connected piece, any shape), floors one
//! level each, walls along the footprint's edge with doors and windows in them, pillars on
//! the corners, a roof, and decorations anchored on the outside walls. Everything is
//! relative to the building's `at`, so moving it changes one line.

use serde::{Deserialize, Serialize};

use crate::model::Placed;

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Building {
    /// unique on its map
    pub id: String,
    pub name: String,
    /// the map cell of its rows' north-west corner
    pub at: [i32; 2],
    /// the level its ground floor stands on
    pub level: u8,
    /// its cells: rows of '#' (in) and '.' (out), north first
    pub cells: Vec<String>,
    /// the outside walls' style (a "wall" prop)
    pub wall: String,
    /// a "roof" prop
    pub roof: String,
    /// the ground floor first, one level each; never empty
    pub floors: Vec<Floor>,
    /// on grid points of the outline, as high as the building
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pillars: Vec<Pillar>,
    /// decorations on the outside walls
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floor {
    /// laid on every cell (a "floor" prop)
    pub tile: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<Opening>,
    /// what stands inside: `at` from the building's `at`, `level` unused
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Placed>,
}

/// A door or a window: in the wall on `side` of `cell`, and of the next cells along that
/// wall (east along a north or south wall, south along an east or west wall).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Opening {
    /// a "door" or "window" prop as wide (in cells) as the opening
    pub prop: String,
    pub cell: [i32; 2],
    /// "n", "e", "s" or "w"
    pub side: String,
    pub width: u8,
}

/// A pillar on a grid point of the outline (from `at`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pillar {
    pub prop: String,
    pub corner: [i32; 2],
}

/// A decoration on an outside wall: centred on the wall on `side` of `cell`, on `floor`,
/// `height` cells above that floor, facing out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anchor {
    pub prop: String,
    pub cell: [i32; 2],
    pub side: String,
    #[serde(skip_serializing_if = "is_default")]
    pub floor: u8,
    pub height: f64,
}

pub const SIDES: [&str; 4] = ["n", "e", "s", "w"];

/// The neighbour across a side.
pub fn side_step(side: &str) -> Option<(i32, i32)> {
    match side {
        "n" => Some((0, -1)),
        "e" => Some((1, 0)),
        "s" => Some((0, 1)),
        "w" => Some((-1, 0)),
        _ => None,
    }
}

/// Which way an opening runs along a wall: east along north and south walls, south along
/// east and west ones.
pub fn side_along(side: &str) -> (i32, i32) {
    if matches!(side, "e" | "w") { (0, 1) } else { (1, 0) }
}

/// Facing out of a wall on that side, as a turn (degrees clockwise from south).
pub fn side_turn(side: &str) -> f64 {
    match side {
        "w" => 90.0,
        "n" => 180.0,
        "e" => 270.0,
        _ => 0.0,
    }
}

/// The wall segment's two ends (grid points), on `side` of a cell.
pub fn segment_ends(x: i32, z: i32, side: &str) -> ([i32; 2], [i32; 2]) {
    match side {
        "n" => ([x, z], [x + 1, z]),
        "s" => ([x, z + 1], [x + 1, z + 1]),
        "w" => ([x, z], [x, z + 1]),
        _ => ([x + 1, z], [x + 1, z + 1]),
    }
}

fn turned_side(side: &str) -> &'static str {
    match side {
        "n" => "e",
        "e" => "s",
        "s" => "w",
        _ => "n",
    }
}

impl Building {
    /// Its rows' size: (along x, along z).
    pub fn size(&self) -> (i32, i32) {
        (self.cells.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32, self.cells.len() as i32)
    }

    /// Is this cell (from `at`) inside?
    pub fn has(&self, x: i32, z: i32) -> bool {
        x >= 0 && z >= 0 && self.cells.get(z as usize).and_then(|r| r.chars().nth(x as usize)).map(|c| c == '#').unwrap_or(false)
    }

    /// Its cells, from `at`.
    pub fn cells(&self) -> Vec<(i32, i32)> {
        let mut out = vec![];
        for (z, row) in self.cells.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    out.push((x as i32, z as i32));
                }
            }
        }
        out
    }

    /// Its cells on the map.
    pub fn map_cells(&self) -> Vec<(i32, i32)> {
        self.cells().into_iter().map(|(x, z)| (x + self.at[0], z + self.at[1])).collect()
    }

    /// Is the side of this cell on the outline (a wall)?
    pub fn is_edge(&self, x: i32, z: i32, side: &str) -> bool {
        match side_step(side) {
            Some((dx, dz)) => self.has(x, z) && !self.has(x + dx, z + dz),
            None => false,
        }
    }

    /// Every wall segment: (cell from `at`, side).
    pub fn edges(&self) -> Vec<(i32, i32, &'static str)> {
        let mut out = vec![];
        for (x, z) in self.cells() {
            for side in SIDES {
                if self.is_edge(x, z, side) {
                    out.push((x, z, side));
                }
            }
        }
        out
    }

    /// The grid points of the outline (from `at`), where pillars can stand.
    pub fn corners(&self) -> Vec<[i32; 2]> {
        let mut out: Vec<[i32; 2]> = vec![];
        for (x, z, side) in self.edges() {
            let (a, b) = segment_ends(x, z, side);
            for p in [a, b] {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// One piece: every cell reaches every other through sides.
    pub fn connected(&self) -> bool {
        let cells = self.cells();
        let Some(&first) = cells.first() else { return false };
        let mut seen = vec![first];
        let mut todo = vec![first];
        while let Some((x, z)) = todo.pop() {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (x + dx, z + dz);
                if self.has(n.0, n.1) && !seen.contains(&n) {
                    seen.push(n);
                    todo.push(n);
                }
            }
        }
        seen.len() == cells.len()
    }

    /// The wall segments an opening covers: the cells whose `side` it's on.
    pub fn segments(o: &Opening) -> Vec<(i32, i32)> {
        let (ax, az) = side_along(&o.side);
        (0..o.width.max(1) as i32).map(|k| (o.cell[0] + ax * k, o.cell[1] + az * k)).collect()
    }

    /// Is the opening on the outline, all along one wall?
    pub fn fits(&self, o: &Opening) -> bool {
        Building::segments(o).iter().all(|&(x, z)| self.is_edge(x, z, &o.side))
    }

    /// Footprint rows and their corner for a set of map cells.
    pub fn rows_for(cells: &[(i32, i32)]) -> ([i32; 2], Vec<String>) {
        if cells.is_empty() {
            return ([0, 0], vec![]);
        }
        let (x0, z0) = (cells.iter().map(|c| c.0).min().unwrap(), cells.iter().map(|c| c.1).min().unwrap());
        let (x1, z1) = (cells.iter().map(|c| c.0).max().unwrap(), cells.iter().map(|c| c.1).max().unwrap());
        let rows = (z0..=z1).map(|z| (x0..=x1).map(|x| if cells.contains(&(x, z)) { '#' } else { '.' }).collect()).collect();
        ([x0, z0], rows)
    }

    /// Turned a quarter clockwise (seen from above) inside the same corner: the footprint,
    /// openings, pillars, decorations and what's inside all turn with it.
    pub fn turned(&self) -> Building {
        let (w, h) = self.size();
        let cell = |x: i32, z: i32| (h - 1 - z, x);
        let point = |x: f64, z: f64| (h as f64 - z, x);
        let mut b = self.clone();
        b.cells = (0..w).map(|z| (0..h).map(|x| if self.has(z, h - 1 - x) { '#' } else { '.' }).collect()).collect();
        for (f, floor) in b.floors.iter_mut().enumerate() {
            for (o, old) in floor.openings.iter_mut().zip(&self.floors[f].openings) {
                let side = turned_side(&old.side);
                let turned: Vec<(i32, i32)> = Building::segments(old).into_iter().map(|(x, z)| cell(x, z)).collect();
                let first = turned.iter().copied().min_by_key(|&(x, z)| (x, z)).unwrap_or((0, 0));
                *o = Opening { prop: old.prop.clone(), cell: [first.0, first.1], side: side.into(), width: old.width };
            }
            for p in floor.props.iter_mut() {
                let (x, z) = point(p.at[0], p.at[1]);
                p.at = [x, z];
                p.turn = (p.turn + 90.0).rem_euclid(360.0);
            }
        }
        for p in b.pillars.iter_mut() {
            let (x, z) = (h - p.corner[1], p.corner[0]);
            p.corner = [x, z];
        }
        for a in b.anchors.iter_mut() {
            let (x, z) = cell(a.cell[0], a.cell[1]);
            a.cell = [x, z];
            a.side = turned_side(&a.side).into();
        }
        b
    }

    /// What's wrong with it (empty when nothing), checked against the props' categories.
    pub fn problems(&self, prop: &dyn Fn(&str) -> Option<(String, u8)>) -> Vec<String> {
        let mut out = vec![];
        let at = format!("building \"{}\"", self.id);
        if self.cells.iter().flat_map(|r| r.chars()).any(|c| c != '#' && c != '.') {
            out.push(format!("{at}: cells are '#' and '.'"));
        }
        if self.cells().is_empty() {
            out.push(format!("{at}: no cells"));
        } else if !self.connected() {
            out.push(format!("{at}: its cells must be one piece"));
        }
        if self.floors.is_empty() {
            out.push(format!("{at}: no floor"));
        }
        let want = |id: &str, kinds: &[&str], what: &str, out: &mut Vec<String>| match prop(id) {
            Some((cat, _)) if kinds.contains(&cat.as_str()) => {}
            Some((cat, _)) => out.push(format!("{at}: {what} \"{id}\" is a {cat}")),
            None => out.push(format!("{at}: {what} \"{id}\", no such prop")),
        };
        want(&self.wall, &["wall"], "wall", &mut out);
        want(&self.roof, &["roof"], "roof", &mut out);
        for (f, floor) in self.floors.iter().enumerate() {
            want(&floor.tile, &["floor"], &format!("floor {} tile", f + 1), &mut out);
            let mut used: Vec<(i32, i32, String)> = vec![];
            for o in &floor.openings {
                want(&o.prop, &["door", "window"], &format!("floor {} opening", f + 1), &mut out);
                if let Some((_, w)) = prop(&o.prop) {
                    if w != o.width {
                        out.push(format!("{at}: \"{}\" is {w} cells wide, its opening {}", o.prop, o.width));
                    }
                }
                if !self.fits(o) {
                    out.push(format!("{at}: floor {}: the opening at {:?} {} isn't along one outside wall", f + 1, o.cell, o.side));
                }
                for (x, z) in Building::segments(o) {
                    if used.contains(&(x, z, o.side.clone())) {
                        out.push(format!("{at}: floor {}: two openings on {x}, {z} {}", f + 1, o.side));
                    }
                    used.push((x, z, o.side.clone()));
                }
            }
            for p in &floor.props {
                if prop(&p.prop).is_none() {
                    out.push(format!("{at}: floor {}: \"{}\", no such prop", f + 1, p.prop));
                }
            }
        }
        let corners = self.corners();
        for p in &self.pillars {
            want(&p.prop, &["pillar"], "pillar", &mut out);
            if !corners.contains(&p.corner) {
                out.push(format!("{at}: a pillar at {:?}, not on the outline", p.corner));
            }
        }
        for a in &self.anchors {
            want(&a.prop, &["wall_decoration"], "decoration", &mut out);
            if !self.is_edge(a.cell[0], a.cell[1], &a.side) {
                out.push(format!("{at}: a decoration at {:?} {}, not on an outside wall", a.cell, a.side));
            }
            if a.floor as usize >= self.floors.len() {
                out.push(format!("{at}: a decoration on floor {}, it has {}", a.floor + 1, self.floors.len()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l_shape() -> Building {
        Building {
            id: "b".into(),
            cells: vec!["#.".into(), "##".into()],
            floors: vec![Floor { tile: "wood".into(), openings: vec![Opening { prop: "door".into(), cell: [0, 1], side: "s".into(), width: 2 }], props: vec![] }],
            ..Default::default()
        }
    }

    #[test]
    fn outline_and_openings() {
        let b = l_shape();
        assert!(b.connected());
        assert_eq!(b.edges().len(), 8);
        assert_eq!(b.corners().len(), 8);
        assert!(b.fits(&b.floors[0].openings[0]));
        let bad = Opening { prop: "door".into(), cell: [0, 0], side: "e".into(), width: 2 };
        assert!(!b.fits(&bad), "the east side of (0, 1) is inside");
        let apart = Building { cells: vec!["#.#".into()], ..Default::default() };
        assert!(!apart.connected());
    }

    #[test]
    fn four_quarter_turns_come_back() {
        let b = l_shape();
        let t = b.turned();
        assert_eq!(t.cells, vec!["##".to_string(), "#.".to_string()]);
        assert!(t.fits(&t.floors[0].openings[0]), "{:?}", t.floors[0].openings[0]);
        assert_eq!(t.floors[0].openings[0].side, "w");
        assert_eq!(t.turned().turned().turned(), b);
    }
}
