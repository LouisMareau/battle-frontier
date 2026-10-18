//! Buildings on maps: a footprint of cells (one connected piece, any shape), floors one
//! level each, walls along the footprint's edge with doors and windows in them, pillars on
//! the corners, a roof, and decorations anchored on the outside walls. Each floor has its
//! flooring, and may lay another on any cell, leave a cell open down to the floor below (a
//! hole: an inner balcony's edge; not on the ground floor; a railing of a style along it, or
//! none) and give any of its wall segments another style than the building's, outside and
//! inside apart (an inside face follows its outside unless it has a style of its own, or the
//! building one for all its inside faces). Everything is relative to the building's `at`, so
//! moving it changes one line.

use serde::{Deserialize, Serialize};

use crate::model::{footprint_cells, Placed};

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
    /// the inside walls' style (a "wall" prop); none: each inside face as its outside
    #[serde(skip_serializing_if = "String::is_empty")]
    pub inner_wall: String,
    /// a "roof" prop
    pub roof: String,
    /// flat with a parapet whatever the outline (else a rectangle gets a gabled roof): a
    /// terrace, which stairs on the top floor come up to
    #[serde(skip_serializing_if = "is_default")]
    pub flat_roof: bool,
    /// the ground floor first, one level each; never empty
    pub floors: Vec<Floor>,
    /// on grid points of the outline, as high as the building
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pillars: Vec<Pillar>,
    /// things hung on its walls, outside (or inside: `Anchor::inside`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    /// what stands outside it, next to its walls (benches, planters...): `at` from the
    /// building's `at`, on the ground floor's level; never on its cells
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outside: Vec<Placed>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floor {
    /// laid on every cell (a "floor" prop) but those in `tiles` and `holes`
    pub tile: String,
    /// cells laid with another flooring
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tiles: Vec<CellTile>,
    /// cells left open, down to the floor below (never on the ground floor)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Hole>,
    /// wall segments of this floor whose outside is in another style than the building's
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub walls: Vec<WallStyle>,
    /// wall segments of this floor whose inside has a style of its own (kept whatever the
    /// outside becomes)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inner: Vec<WallStyle>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<Opening>,
    /// what stands inside: `at` from the building's `at`, `level` unused
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Placed>,
}

/// Another flooring than its floor's, on one cell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellTile {
    pub cell: [i32; 2],
    /// a "floor" prop
    pub tile: String,
}

/// The railing style a hole had before holes chose theirs.
pub const DEFAULT_RAILING: &str = "wood";

/// A cell left open down to the floor below, a railing along its edges where the floor goes
/// on (its pieces those of a style: `RailingPiece`), or none.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Hole {
    pub cell: [i32; 2],
    /// the railing's style; none: no railing
    #[serde(skip_serializing_if = "String::is_empty")]
    pub railing: String,
}

impl<'de> Deserialize<'de> for Hole {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Kept {
            /// as first kept, a cell alone: its railing the plain one
            Cell([i32; 2]),
            Full {
                cell: [i32; 2],
                #[serde(default)]
                railing: String,
            },
        }
        Ok(match Kept::deserialize(d)? {
            Kept::Cell(cell) => Hole { cell, railing: DEFAULT_RAILING.into() },
            Kept::Full { cell, railing } => Hole { cell, railing },
        })
    }
}

/// Another style than the building's for one wall segment of a floor: the wall on `side` of
/// `cell`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallStyle {
    pub cell: [i32; 2],
    /// "n", "e", "s" or "w"
    pub side: String,
    /// a "wall" prop
    pub wall: String,
}

impl Floor {
    /// The flooring on a cell (a hole has none to walk on, but keeps what it would be).
    pub fn tile_at(&self, x: i32, z: i32) -> &str {
        self.tiles.iter().find(|t| t.cell == [x, z]).map_or(self.tile.as_str(), |t| t.tile.as_str())
    }

    /// `tile` laid on a cell (the floor's own, or "": back to it); a hole there is closed.
    pub fn set_tile(&mut self, x: i32, z: i32, tile: &str) {
        self.holes.retain(|h| h.cell != [x, z]);
        self.tiles.retain(|t| t.cell != [x, z]);
        if !tile.is_empty() && tile != self.tile {
            self.tiles.push(CellTile { cell: [x, z], tile: tile.into() });
        }
    }

    /// Is this cell left open down to the floor below?
    pub fn is_hole(&self, x: i32, z: i32) -> bool {
        self.hole(x, z).is_some()
    }

    pub fn hole(&self, x: i32, z: i32) -> Option<&Hole> {
        self.holes.iter().find(|h| h.cell == [x, z])
    }

    /// A cell opened with a railing of that style ("": none) — its flooring goes — or, with
    /// none, closed again (the floor's own flooring).
    pub fn set_hole(&mut self, x: i32, z: i32, railing: Option<&str>) {
        self.tiles.retain(|t| t.cell != [x, z]);
        self.holes.retain(|h| h.cell != [x, z]);
        if let Some(r) = railing {
            self.holes.push(Hole { cell: [x, z], railing: r.into() });
        }
    }

    /// The inside face's own style on `side` of a cell, if it has one.
    pub fn inner_at(&self, x: i32, z: i32, side: &str) -> Option<&str> {
        self.inner.iter().find(|w| w.cell == [x, z] && w.side == side).map(|w| w.wall.as_str())
    }

    /// An inside face given its own style ("": none, as the building has it again).
    pub fn set_inner(&mut self, x: i32, z: i32, side: &str, style: &str) {
        self.inner.retain(|w| !(w.cell == [x, z] && w.side == side));
        if !style.is_empty() {
            self.inner.push(WallStyle { cell: [x, z], side: side.into(), wall: style.into() });
        }
    }

    /// The style of the wall on `side` of a cell, `wall` (the building's) unless it has its own.
    pub fn wall_at<'a>(&'a self, x: i32, z: i32, side: &str, wall: &'a str) -> &'a str {
        self.walls.iter().find(|w| w.cell == [x, z] && w.side == side).map_or(wall, |w| w.wall.as_str())
    }

    /// A wall segment given a style (`wall`, the building's, or "": back to it).
    pub fn set_wall(&mut self, x: i32, z: i32, side: &str, style: &str, wall: &str) {
        self.walls.retain(|w| !(w.cell == [x, z] && w.side == side));
        if !style.is_empty() && style != wall {
            self.walls.push(WallStyle { cell: [x, z], side: side.into(), wall: style.into() });
        }
    }
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

/// Something hung on a wall: centred on the wall on `side` of `cell`, on `floor`, `height`
/// cells above that floor, on the outside face facing out (or the inside face facing in).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anchor {
    pub prop: String,
    pub cell: [i32; 2],
    pub side: String,
    #[serde(skip_serializing_if = "is_default")]
    pub floor: u8,
    pub height: f64,
    /// on the inside face of the wall
    #[serde(skip_serializing_if = "is_default")]
    pub inside: bool,
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

/// What a check knows of a prop: its category and footprint (nothing: no such prop).
pub type PropFacts = Option<(String, [u8; 2])>;

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

    /// Its outline fills its rows (a gabled roof fits it).
    pub fn is_rectangle(&self) -> bool {
        let (w, h) = self.size();
        self.cells().len() as i32 == w * h
    }

    /// Is its roof flat (a terrace)? By choice, or because the outline isn't a rectangle.
    pub fn roof_is_flat(&self) -> bool {
        self.flat_roof || !self.is_rectangle()
    }

    /// The cells of `floor` left open because stairs stand under them, on the floor below
    /// (`stairs`: a stairs prop's footprint, nothing for other props); `floor` past the top
    /// floor gives the roof's openings. From `at`.
    pub fn stairwells(&self, floor: usize, stairs: &dyn Fn(&str) -> Option<[u8; 2]>) -> Vec<(i32, i32)> {
        let Some(below) = floor.checked_sub(1).and_then(|f| self.floors.get(f)) else { return vec![] };
        below.props.iter().filter_map(|p| stairs(&p.prop).map(|fp| footprint_cells(p.at, p.turn, fp, 1.0))).flatten().collect()
    }

    /// The cells of `floor` open down to the floor below: under the stairs' way up
    /// (`stairwells`) and its holes. From `at`.
    pub fn open_cells(&self, floor: usize, stairs: &dyn Fn(&str) -> Option<[u8; 2]>) -> Vec<(i32, i32)> {
        let mut out = self.stairwells(floor, stairs);
        if let Some(f) = self.floors.get(floor).filter(|_| floor > 0) {
            for h in &f.holes {
                if !out.contains(&(h.cell[0], h.cell[1])) {
                    out.push((h.cell[0], h.cell[1]));
                }
            }
        }
        out
    }

    /// The style of a wall segment's inside face on `floor`: its own, else the building's
    /// for all inside faces, else its outside's.
    pub fn wall_inside(&self, floor: usize, x: i32, z: i32, side: &str) -> &str {
        let Some(f) = self.floors.get(floor) else { return &self.wall };
        match f.inner_at(x, z, side) {
            Some(own) => own,
            None if !self.inner_wall.is_empty() => &self.inner_wall,
            None => f.wall_at(x, z, side, &self.wall),
        }
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
            for t in floor.tiles.iter_mut() {
                let (x, z) = cell(t.cell[0], t.cell[1]);
                t.cell = [x, z];
            }
            for h in floor.holes.iter_mut() {
                let (x, z) = cell(h.cell[0], h.cell[1]);
                h.cell = [x, z];
            }
            for w in floor.walls.iter_mut().chain(floor.inner.iter_mut()) {
                let (x, z) = cell(w.cell[0], w.cell[1]);
                w.cell = [x, z];
                w.side = turned_side(&w.side).into();
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
        for p in b.outside.iter_mut() {
            let (x, z) = point(p.at[0], p.at[1]);
            p.at = [x, z];
            p.turn = (p.turn + 90.0).rem_euclid(360.0);
        }
        b
    }

    /// What's wrong with it (empty when nothing), checked against the props' categories
    /// and footprints.
    pub fn problems(&self, prop: &dyn Fn(&str) -> PropFacts) -> Vec<String> {
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
        if !self.inner_wall.is_empty() {
            want(&self.inner_wall, &["wall"], "inside walls", &mut out);
        }
        want(&self.roof, &["roof"], "roof", &mut out);
        let stairs = |id: &str| match prop(id) {
            Some((cat, fp)) if cat == "stairs" => Some(fp),
            _ => None,
        };
        for (f, floor) in self.floors.iter().enumerate() {
            want(&floor.tile, &["floor"], &format!("floor {} tile", f + 1), &mut out);
            for t in &floor.tiles {
                want(&t.tile, &["floor"], &format!("floor {} tile at {:?}", f + 1, t.cell), &mut out);
                if !self.has(t.cell[0], t.cell[1]) {
                    out.push(format!("{at}: floor {}: a tile at {:?}, not on its cells", f + 1, t.cell));
                }
            }
            if f == 0 && !floor.holes.is_empty() {
                out.push(format!("{at}: the ground floor has holes: there's nothing below it"));
            }
            for h in &floor.holes {
                if !self.has(h.cell[0], h.cell[1]) {
                    out.push(format!("{at}: floor {}: a hole at {:?}, not on its cells", f + 1, h.cell));
                }
            }
            for (w, face) in floor.walls.iter().map(|w| (w, "wall")).chain(floor.inner.iter().map(|w| (w, "inside wall"))) {
                want(&w.wall, &["wall"], &format!("floor {} {face} at {:?} {}", f + 1, w.cell, w.side), &mut out);
                if !self.is_edge(w.cell[0], w.cell[1], &w.side) {
                    out.push(format!("{at}: floor {}: a {face} style at {:?} {}, not on one of its walls", f + 1, w.cell, w.side));
                }
            }
            let mut used: Vec<(i32, i32, String)> = vec![];
            for o in &floor.openings {
                want(&o.prop, &["door", "window"], &format!("floor {} opening", f + 1), &mut out);
                if let Some((_, w)) = prop(&o.prop) {
                    if w[0] != o.width {
                        out.push(format!("{at}: \"{}\" is {} cells wide, its opening {}", o.prop, w[0], o.width));
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
            // nothing over the stairs from below or over a hole, and stairs with somewhere to go
            let holes = self.stairwells(f, &stairs);
            for p in &floor.props {
                match prop(&p.prop) {
                    None => out.push(format!("{at}: floor {}: \"{}\", no such prop", f + 1, p.prop)),
                    Some((_, fp)) => {
                        let cells = footprint_cells(p.at, p.turn, fp, 1.0);
                        if cells.iter().any(|c| holes.contains(c)) {
                            out.push(format!("{at}: floor {}: \"{}\" stands over the stairs from below", f + 1, p.prop));
                        }
                        if f > 0 && cells.iter().any(|&(x, z)| floor.is_hole(x, z)) {
                            out.push(format!("{at}: floor {}: \"{}\" stands over a hole", f + 1, p.prop));
                        }
                    }
                }
            }
            if f + 1 == self.floors.len() && !self.roof_is_flat() && !self.stairwells(f + 1, &stairs).is_empty() {
                out.push(format!("{at}: stairs on the top floor need a flat roof (or a floor above)"));
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
            if prop(&a.prop).is_none() {
                out.push(format!("{at}: a wall item \"{}\", no such prop", a.prop));
            }
            if !self.is_edge(a.cell[0], a.cell[1], &a.side) {
                out.push(format!("{at}: a wall item at {:?} {}, not on one of its walls", a.cell, a.side));
            }
            if a.floor as usize >= self.floors.len() {
                out.push(format!("{at}: a decoration on floor {}, it has {}", a.floor + 1, self.floors.len()));
            }
        }
        for p in &self.outside {
            if prop(&p.prop).is_none() {
                out.push(format!("{at}: outside, \"{}\", no such prop", p.prop));
            }
            if self.has(p.at[0].floor() as i32, p.at[1].floor() as i32) {
                out.push(format!("{at}: \"{}\" stands outside but on its cells, at {:?}", p.prop, p.at));
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
            floors: vec![Floor { tile: "wood".into(), openings: vec![Opening { prop: "door".into(), cell: [0, 1], side: "s".into(), width: 2 }], ..Default::default() }],
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
    fn stairs_open_the_floor_above_or_a_flat_roof() {
        let stairs = |id: &str| (id == "stairs").then_some([1, 2]);
        let prop = |id: &str| match id {
            "stairs" => Some(("stairs".to_string(), [1, 2])),
            "table" => Some(("furniture".to_string(), [1, 1])),
            _ => None,
        };
        let about_stairs = |b: &Building| b.problems(&prop).into_iter().filter(|p| p.contains("stairs")).collect::<Vec<_>>();
        let place = |id: &str, x: f64, z: f64| Placed { prop: id.into(), at: [x, z], level: 0, turn: 0.0, scale: 1.0, ..Default::default() };
        // an L: a flat roof, so stairs on its only floor come up to it
        let mut b = l_shape();
        b.floors[0].props.push(place("stairs", 0.5, 1.0));
        assert!(b.roof_is_flat());
        assert!(b.stairwells(0, &stairs).is_empty(), "nothing under the ground floor");
        assert_eq!(b.stairwells(1, &stairs), vec![(0, 0), (0, 1)], "the roof's opening");
        assert!(about_stairs(&b).is_empty(), "{:?}", about_stairs(&b));
        // a floor above: open over the stairs, and nothing may stand there
        b.floors.push(Floor { tile: "wood".into(), props: vec![place("table", 0.5, 0.5)], ..Default::default() });
        assert_eq!(b.stairwells(1, &stairs), vec![(0, 0), (0, 1)]);
        assert_eq!(about_stairs(&b).len(), 1, "{:?}", about_stairs(&b));
        b.floors[1].props[0].at = [1.5, 1.5];
        assert!(about_stairs(&b).is_empty());
        // a rectangle has a gabled roof: no stairs on its top floor until it's made flat
        let mut square = Building { id: "s".into(), cells: vec!["##".into(), "##".into()], floors: vec![Floor { tile: "wood".into(), ..Default::default() }], ..Default::default() };
        square.floors[0].props.push(place("stairs", 0.5, 1.0));
        assert!(!square.roof_is_flat());
        assert!(about_stairs(&square).iter().any(|p| p.contains("flat roof")), "{:?}", about_stairs(&square));
        square.flat_roof = true;
        assert!(square.roof_is_flat());
        assert!(about_stairs(&square).is_empty());
        assert!(square.turned().flat_roof);
    }

    #[test]
    fn four_quarter_turns_come_back() {
        let mut b = l_shape();
        b.floors[0].set_tile(1, 1, "stone");
        b.floors[0].set_wall(1, 1, "e", "bricks", "plaster");
        b.floors.push(Floor { tile: "wood".into(), ..Default::default() });
        b.floors[1].set_hole(0, 0, Some("iron"));
        b.floors[0].set_inner(0, 1, "w", "bricks");
        let t = b.turned();
        assert_eq!(t.cells, vec!["##".to_string(), "#.".to_string()]);
        assert!(t.fits(&t.floors[0].openings[0]), "{:?}", t.floors[0].openings[0]);
        assert_eq!(t.floors[0].openings[0].side, "w");
        // the tile, the wall's style and the hole turn with it, onto its cells and walls
        assert_eq!(t.floors[0].tiles[0].cell, [0, 1]);
        assert_eq!((t.floors[0].walls[0].cell, t.floors[0].walls[0].side.as_str()), ([0, 1], "s"));
        assert!(t.is_edge(0, 1, "s"));
        assert!(t.floors[1].is_hole(1, 0));
        assert_eq!(t.floors[1].hole(1, 0).map(|h| h.railing.as_str()), Some("iron"));
        assert_eq!((t.floors[0].inner[0].cell, t.floors[0].inner[0].side.as_str()), ([0, 0], "n"));
        assert_eq!(t.turned().turned().turned(), b);
    }

    #[test]
    fn cells_and_walls_of_their_own() {
        let prop = |id: &str| match id {
            "wood" | "stone" => Some(("floor".to_string(), [1, 1])),
            "plaster" | "bricks" => Some(("wall".to_string(), [1, 1])),
            "table" => Some(("furniture".to_string(), [1, 1])),
            _ => None,
        };
        let mut b = Building { id: "b".into(), cells: vec!["##".into(), "##".into()], wall: "plaster".into(), roof: "plaster".into(), floors: vec![Floor { tile: "wood".into(), ..Default::default() }; 2], ..Default::default() };
        let mine = |b: &Building| b.problems(&prop).into_iter().filter(|p| !p.contains("roof")).collect::<Vec<_>>();
        // a cell of its own flooring; laying the floor's own on it takes it back
        b.floors[0].set_tile(1, 0, "stone");
        assert_eq!((b.floors[0].tile_at(1, 0), b.floors[0].tile_at(0, 0)), ("stone", "wood"));
        b.floors[0].set_tile(1, 0, "wood");
        assert!(b.floors[0].tiles.is_empty());
        // a wall segment of its own style, the others the building's
        b.floors[1].set_wall(0, 0, "n", "bricks", &b.wall.clone());
        assert_eq!(b.floors[1].wall_at(0, 0, "n", &b.wall), "bricks");
        assert_eq!(b.floors[1].wall_at(1, 0, "n", &b.wall), "plaster");
        assert_eq!(b.floors[0].wall_at(0, 0, "n", &b.wall), "plaster", "another floor's wall");
        assert!(mine(&b).is_empty(), "{:?}", mine(&b));
        // a hole upstairs: open, its flooring gone; a tile laid closes it again
        b.floors[1].set_tile(1, 1, "stone");
        b.floors[1].set_hole(1, 1, Some("wood"));
        assert!(b.floors[1].is_hole(1, 1) && b.floors[1].tiles.is_empty());
        let none = |_: &str| -> Option<[u8; 2]> { None };
        assert_eq!(b.open_cells(1, &none), vec![(1, 1)]);
        assert!(b.open_cells(0, &none).is_empty());
        // nothing stands over it; none on the ground floor; none off the building
        b.floors[1].props.push(Placed { prop: "table".into(), at: [1.5, 1.5], scale: 1.0, ..Default::default() });
        assert!(mine(&b).iter().any(|p| p.contains("over a hole")), "{:?}", mine(&b));
        b.floors[1].props.clear();
        b.floors[0].holes.push(Hole { cell: [0, 0], railing: String::new() });
        b.floors[1].holes.push(Hole { cell: [5, 5], railing: String::new() });
        b.floors[1].walls.push(WallStyle { cell: [0, 0], side: "s".into(), wall: "bricks".into() });
        b.floors[1].tiles.push(CellTile { cell: [0, 1], tile: "table".into() });
        let wrong = mine(&b);
        assert!(wrong.iter().any(|p| p.contains("ground floor has holes")), "{wrong:?}");
        assert!(wrong.iter().any(|p| p.contains("a hole at [5, 5]")), "{wrong:?}");
        assert!(wrong.iter().any(|p| p.contains("not on one of its walls")), "{wrong:?}");
        assert!(wrong.iter().any(|p| p.contains("is a furniture")), "{wrong:?}");
        b.floors[1].set_tile(1, 1, "");
        assert!(!b.floors[1].is_hole(1, 1), "laying the floor's own closes it");
    }

    #[test]
    fn holes_say_their_railing() {
        // as first kept (a cell alone): the plain railing; as kept now: theirs, or none
        let f: Floor = serde_json::from_str(r#"{"tile": "wood", "holes": [[1, 0], {"cell": [2, 0], "railing": "iron"}, {"cell": [3, 0]}]}"#).unwrap();
        let railings: Vec<&str> = f.holes.iter().map(|h| h.railing.as_str()).collect();
        assert_eq!(railings, [DEFAULT_RAILING, "iron", ""]);
        let mut again = f.clone();
        again.set_hole(2, 0, Some(""));
        assert_eq!(serde_json::to_string(&again.holes).unwrap(), r#"[{"cell":[1,0],"railing":"wood"},{"cell":[3,0]},{"cell":[2,0]}]"#);
        let back: Floor = serde_json::from_str(&serde_json::to_string(&again).unwrap()).unwrap();
        assert_eq!(back, again);
        again.set_hole(1, 0, None);
        assert!(!again.is_hole(1, 0));
    }

    #[test]
    fn inside_faces_keep_their_own() {
        let mut b = Building { id: "b".into(), cells: vec!["##".into()], wall: "plaster".into(), floors: vec![Floor { tile: "wood".into(), ..Default::default() }], ..Default::default() };
        // an inside face follows its outside until it has its own
        assert_eq!(b.wall_inside(0, 0, 0, "n"), "plaster");
        b.floors[0].set_wall(0, 0, "n", "bricks", "plaster");
        assert_eq!(b.wall_inside(0, 0, 0, "n"), "bricks", "as the outside");
        b.floors[0].set_inner(0, 0, "n", "planks");
        assert_eq!(b.wall_inside(0, 0, 0, "n"), "planks");
        // its own kept whatever the outside becomes, the building's or the segment's
        b.floors[0].set_wall(0, 0, "n", "stone", &b.wall.clone());
        b.wall = "logs".into();
        assert_eq!(b.wall_inside(0, 0, 0, "n"), "planks");
        // the building's inside walls: every inside face without its own
        b.inner_wall = "plaster".into();
        assert_eq!((b.wall_inside(0, 0, 0, "n"), b.wall_inside(0, 1, 0, "e")), ("planks", "plaster"));
        // and the outside never follows the inside
        assert_eq!(b.floors[0].wall_at(0, 0, "n", &b.wall), "stone");
        b.floors[0].set_inner(0, 0, "n", "");
        assert_eq!(b.wall_inside(0, 0, 0, "n"), "plaster");
    }
}
