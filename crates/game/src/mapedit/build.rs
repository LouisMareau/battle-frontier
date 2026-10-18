//! The Build tool: a building is drawn as a footprint of cells (any shape, one piece), then
//! designed floor by floor, then confirmed into the map, where it blocks and its doors let
//! the player through. Designing has three modes: Structure (flooring, doors and windows
//! along the outside walls, pillars, the walls' and the roof's styles, and cell by cell: a
//! flooring for each cell of the floor, a hole down to the floor below on the floors above
//! the ground, a style for each segment of wall), Inside (what stands on the floor being
//! designed and hangs on its inner walls) and Outside (a wall first, then what hangs on it
//! and stands in front of it): see `rooms.rs`. While it's designed the rest of the map is
//! darkened and nothing lands outside it. Buildings already on the map are moved, turned,
//! edited and deleted from the Select tool.

use content::{segment_ends, Building, Floor, Opening, Pillar, INSIDE, OUTSIDE};
use egui::RichText;
use glam::Vec3;
use render::scene::{Shape, ShapeKind};

use super::{hint, pick, section, warn, MapEditor};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Step {
    Footprint,
    Design,
}

/// What a click does while designing: the first four are the Structure mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Clicks {
    Openings,
    Pillars,
    /// a cell of the floor takes the flooring picked (or opens: a hole)
    Tiles,
    /// a segment of the floor's walls takes the style picked
    Walls,
    Inside,
    Outside,
}

impl Clicks {
    fn structure(self) -> bool {
        matches!(self, Clicks::Openings | Clicks::Pillars | Clicks::Tiles | Clicks::Walls)
    }
}

/// The flooring brush that opens a cell: a hole down to the floor below.
const HOLE: &str = "#hole";

/// Something placed in the building being designed: on the floor being designed, hung on
/// one of its walls (inside or out: `Building::anchors`), or standing outside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Sel {
    Floor(usize),
    Wall(usize),
    Outside(usize),
}

pub(super) struct Draft {
    pub b: Building,
    /// the building on the map it replaces, when one is edited
    pub editing: Option<usize>,
    pub step: Step,
    /// the footprint while it's drawn (map cells)
    pub cells: Vec<(i32, i32)>,
    /// the floor being designed (the ones above are cut away)
    pub floor: usize,
    pub clicks: Clicks,
    /// doors, or windows
    pub doors: bool,
    /// a span of wall chosen for a door or window (its prop still to pick)
    pub span: Option<Opening>,
    pub drag_from: Option<(i32, i32, &'static str)>,
    pub hover_edge: Option<(i32, i32, &'static str)>,
    pub hover_point: Option<[i32; 2]>,
    pub inside_prop: Option<String>,
    pub outside_prop: Option<String>,
    pub pillar_prop: Option<String>,
    /// Outside: the wall picked (one of its edges; the straight run it's on counts)
    pub wall: Option<(i32, i32, &'static str)>,
    /// Inside / Outside: where the pointer is on a wall, or on the ground (from `b.at`)
    pub hover_face: Option<super::rooms::WallHit>,
    pub hover_at: Option<[f64; 2]>,
    /// what's selected (Q/E turn it, Delete removes it)
    pub sel: Option<Sel>,
    pub problems: Vec<String>,
    /// Flooring: what a click lays on a cell (a floor prop; "": the floor's own; `HOLE`)
    pub tile_brush: String,
    /// Walls: the style a click gives a segment (a wall prop; "": the building's)
    pub wall_brush: String,
    /// Walls: given on every floor at once, not just the one being designed
    pub every_floor: bool,
    /// Walls: the brush paints the segments' inside faces (else their outside)
    pub inner_face: bool,
    /// Flooring: the railing a hole painted gets (a style; "": none)
    pub railing_brush: String,
    /// Flooring: the cell under the pointer (from `at`)
    pub hover_cell: Option<(i32, i32)>,
    /// a stroke of flooring or walls going on (the button held down)
    pub painting: bool,
}

fn first_of(ed: &MapEditor, category: &str) -> Option<String> {
    ed.content.props.iter().find(|p| p.category == category).map(|p| p.id.clone())
}

/// A new building's style: these if the content has them, else the first of each kind.
fn default_of(ed: &MapEditor, category: &str, wanted: &str) -> String {
    if ed.content.props.get(wanted).map(|p| p.category == category).unwrap_or(false) {
        return wanted.into();
    }
    first_of(ed, category).unwrap_or_default()
}

/// The props of some categories: (id, name, footprint, colour).
fn ids_of(ed: &MapEditor, categories: &[&str]) -> Vec<(String, String, [u8; 2], String)> {
    ed.content
        .props
        .iter()
        .filter(|p| categories.contains(&p.category.as_str()))
        .map(|p| (p.id.clone(), p.name.clone(), p.footprint, p.color(0).to_string()))
        .collect()
}

/// The props of a category as a picker's choices: (id, name, colour).
fn choices(ed: &MapEditor, category: &str) -> Vec<(String, String, String)> {
    ids_of(ed, &[category]).into_iter().map(|(id, name, _, color)| (id, name, color)).collect()
}

/// The railing styles (those with a span among the railing props) as choices: (style, its
/// name, its span's colour).
fn railing_styles(ed: &MapEditor) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = vec![];
    for p in ed.content.props.iter() {
        let Some(r) = p.railing.as_ref().filter(|r| r.is_span()) else { continue };
        if out.iter().any(|s| s.0 == r.style) {
            continue;
        }
        let words = r.style.replace('_', " ");
        let name = words.get(..1).map(|c| c.to_uppercase() + &words[1..]).unwrap_or_default();
        out.push((r.style.clone(), name, p.color(0).to_string()));
    }
    out.sort();
    out
}

/// Shifts everything a building holds when its corner moves from `old` to its `at`, and
/// drops what no longer fits the footprint.
fn rebase(b: &mut Building, old: [i32; 2]) -> usize {
    let (dx, dz) = (old[0] - b.at[0], old[1] - b.at[1]);
    let mut dropped = 0;
    for f in b.floors.iter_mut() {
        for o in f.openings.iter_mut() {
            o.cell = [o.cell[0] + dx, o.cell[1] + dz];
        }
        for p in f.props.iter_mut() {
            p.at = [p.at[0] + dx as f64, p.at[1] + dz as f64];
        }
        for t in f.tiles.iter_mut() {
            t.cell = [t.cell[0] + dx, t.cell[1] + dz];
        }
        for h in f.holes.iter_mut() {
            h.cell = [h.cell[0] + dx, h.cell[1] + dz];
        }
        for w in f.walls.iter_mut().chain(f.inner.iter_mut()) {
            w.cell = [w.cell[0] + dx, w.cell[1] + dz];
        }
    }
    for p in b.pillars.iter_mut() {
        p.corner = [p.corner[0] + dx, p.corner[1] + dz];
    }
    for a in b.anchors.iter_mut() {
        a.cell = [a.cell[0] + dx, a.cell[1] + dz];
    }
    for p in b.outside.iter_mut() {
        p.at = [p.at[0] + dx as f64, p.at[1] + dz as f64];
    }
    let copy = b.clone();
    for f in b.floors.iter_mut() {
        let before = f.openings.len() + f.props.len();
        f.openings.retain(|o| copy.fits(o));
        f.props.retain(|p| copy.has(p.at[0].floor() as i32, p.at[1].floor() as i32));
        dropped += before - f.openings.len() - f.props.len();
        // a cell's flooring, a hole, a wall's style: gone with their cell or wall
        f.tiles.retain(|t| copy.has(t.cell[0], t.cell[1]));
        f.holes.retain(|h| copy.has(h.cell[0], h.cell[1]));
        f.walls.retain(|w| copy.is_edge(w.cell[0], w.cell[1], &w.side));
        f.inner.retain(|w| copy.is_edge(w.cell[0], w.cell[1], &w.side));
    }
    let corners = copy.corners();
    let before = b.pillars.len() + b.anchors.len() + b.outside.len();
    b.pillars.retain(|p| corners.contains(&p.corner));
    b.anchors.retain(|a| copy.is_edge(a.cell[0], a.cell[1], &a.side));
    b.outside.retain(|p| !copy.has(p.at[0].floor() as i32, p.at[1].floor() as i32));
    dropped + before - b.pillars.len() - b.anchors.len() - b.outside.len()
}

impl MapEditor {
    /// The map cells every building but `except` covers.
    fn taken(&self, except: Option<usize>) -> Vec<(i32, i32)> {
        self.map.buildings.iter().enumerate().filter(|(i, _)| Some(*i) != except).flat_map(|(_, b)| b.map_cells()).collect()
    }

    /// The first prop of a folder that stands on the ground (or, `walls`, hangs).
    fn first_in(&self, folder: &str, walls: bool) -> Option<String> {
        self.folder_props(folder).into_iter().find(|p| p.4 == walls).or_else(|| self.folder_props(folder).into_iter().next()).map(|p| p.0)
    }

    pub(super) fn start_building(&mut self, editing: Option<usize>) {
        let b = match editing.and_then(|i| self.map.buildings.get(i)) {
            Some(b) => b.clone(),
            None => {
                let n = (1..).find(|n| !self.map.buildings.iter().any(|b| b.id == format!("building_{n}"))).unwrap_or(1);
                Building {
                    id: format!("building_{n}"),
                    name: format!("Building {n}"),
                    level: self.level,
                    wall: default_of(self, "wall", "plaster_wall"),
                    roof: default_of(self, "roof", "red_tile_roof"),
                    floors: vec![Floor { tile: default_of(self, "floor", "wood_floor"), ..Default::default() }],
                    ..Default::default()
                }
            }
        };
        let cells = b.map_cells();
        // the brushes: another flooring and another style than the building's to start with
        let tile_brush = choices(self, "floor").into_iter().map(|c| c.0).find(|id| b.floors.first().map(|f| &f.tile) != Some(id)).unwrap_or_default();
        let wall_brush = choices(self, "wall").into_iter().map(|c| c.0).find(|id| *id != b.wall).unwrap_or_default();
        let styles = railing_styles(self);
        let railing_brush = styles.iter().find(|s| s.0 == content::DEFAULT_RAILING).or(styles.first()).map(|s| s.0.clone()).unwrap_or_default();
        self.draft = Some(Draft {
            tile_brush,
            wall_brush,
            every_floor: false,
            inner_face: false,
            railing_brush,
            hover_cell: None,
            painting: false,
            step: if editing.is_some() { Step::Design } else { Step::Footprint },
            editing,
            cells,
            floor: 0,
            clicks: Clicks::Openings,
            doors: true,
            span: None,
            drag_from: None,
            hover_edge: None,
            hover_point: None,
            inside_prop: self.first_in(INSIDE, false),
            outside_prop: self.first_in(OUTSIDE, true),
            pillar_prop: first_of(self, "pillar"),
            wall: None,
            hover_face: None,
            hover_at: None,
            sel: None,
            problems: vec![],
            b,
        });
        if let Some(i) = editing {
            self.level = self.map.buildings[i].level;
        }
        self.tool = super::Tool::Build;
        self.selected = None;
        self.selected_building = None;
        self.need_buildings = true;
    }

    fn open_design(&mut self) {
        let taken = self.taken(self.draft.as_ref().and_then(|d| d.editing));
        let Some(d) = &mut self.draft else { return };
        if d.cells.is_empty() {
            self.status = "Draw the building's cells first".into();
            return;
        }
        let (at, rows) = Building::rows_for(&d.cells);
        let old = d.b.at;
        d.b.at = at;
        d.b.cells = rows;
        if !d.b.connected() {
            self.status = "The cells must be one piece: every cell next to another".into();
            return;
        }
        if d.cells.iter().any(|c| taken.contains(c)) {
            self.status = "Another building is already there".into();
            return;
        }
        let dropped = rebase(&mut d.b, old);
        if dropped > 0 {
            self.status = format!("{dropped} doors, windows or items didn't fit the new outline and were dropped");
        }
        d.step = Step::Design;
        d.span = None;
        d.wall = None;
        d.sel = None;
        self.need_buildings = true;
    }

    fn confirm_building(&mut self) {
        let taken = self.taken(self.draft.as_ref().and_then(|d| d.editing));
        let Some(d) = &mut self.draft else { return };
        let content = self.content.clone();
        let prop = |id: &str| content.props.get(id).map(|p| (p.category.clone(), p.footprint));
        let mut problems = d.b.problems(&prop);
        let cells = d.b.map_cells();
        if cells.iter().any(|&(x, z)| !self.map.inside(x, z)) {
            problems.push("it goes off the map".into());
        }
        if cells.iter().any(|c| taken.contains(c)) {
            problems.push("another building is already there".into());
        }
        if !problems.is_empty() {
            self.status = format!("Not confirmed: {}", problems.join("; "));
            d.problems = problems;
            return;
        }
        let b = d.b.clone();
        let editing = d.editing;
        self.draft = None;
        self.checkpoint();
        let i = match editing {
            Some(i) => {
                self.map.buildings[i] = b;
                i
            }
            None => {
                self.map.buildings.push(b);
                self.map.buildings.len() - 1
            }
        };
        self.settle(i);
        // on the map and selected: move it, turn it or edit it again from Select
        self.selected_building = Some(i);
        self.tool = super::Tool::Select;
        self.status = format!("{} is on the map: walls block, doors let you in", self.map.buildings[i].name);
    }

    /// Drops the draft: an edit leaves the building as it was, selected.
    fn cancel_building(&mut self) {
        if let Some(i) = self.draft.take().and_then(|d| d.editing) {
            self.selected_building = Some(i);
            self.tool = super::Tool::Select;
        }
        self.need_buildings = true;
    }

    /// Moves a building to other cells (its corner to `at`): only onto the map, and not onto
    /// another building.
    pub(super) fn move_building(&mut self, i: usize, at: [i32; 2]) -> bool {
        let mut b = self.map.buildings[i].clone();
        b.at = at;
        let taken = self.taken(Some(i));
        if b.map_cells().iter().any(|&(x, z)| !self.map.inside(x, z) || taken.contains(&(x, z))) {
            return false;
        }
        self.map.buildings[i] = b;
        self.need_buildings = true;
        true
    }

    /// Levels the ground under a building to its floor and clears tree areas from it.
    pub(super) fn settle(&mut self, i: usize) {
        let b = &self.map.buildings[i];
        let level = b.level;
        for (x, z) in b.map_cells() {
            if let Some(c) = self.map.index(x, z) {
                if self.map.level[c] != level || self.map.area[c] != 0 {
                    self.map.level[c] = level;
                    self.map.area[c] = 0;
                    self.need_ground = true;
                    self.need_trees = true;
                }
            }
        }
        self.need_buildings = true;
    }

    // ------------------------------------------------------------ picking walls

    /// The outline edge nearest a point on the floor's plane (within 0.6 cells).
    pub(super) fn nearest_edge(b: &Building, p: Vec3) -> Option<(i32, i32, &'static str)> {
        let (px, pz) = (p.x - b.at[0] as f32, p.z - b.at[1] as f32);
        let mut best: Option<(f32, (i32, i32, &'static str))> = None;
        for (x, z, side) in b.edges() {
            let (a, q) = segment_ends(x, z, side);
            let (ax, az, qx, qz) = (a[0] as f32, a[1] as f32, q[0] as f32, q[1] as f32);
            let t = (((px - ax) * (qx - ax) + (pz - az) * (qz - az)) / 1.0).clamp(0.0, 1.0);
            let (cx, cz) = (ax + (qx - ax) * t, az + (qz - az) * t);
            let d = ((px - cx).powi(2) + (pz - cz).powi(2)).sqrt();
            if d < 0.6 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, (x, z, side)));
            }
        }
        best.map(|(_, e)| e)
    }

    /// The opening from one edge to another along the same wall, as far as the wall goes.
    fn span(b: &Building, from: (i32, i32, &'static str), to: (i32, i32, &'static str)) -> Opening {
        let side = from.2;
        let along_x = matches!(side, "n" | "s");
        let (a, c) = if along_x { (from.0, to.0) } else { (from.1, to.1) };
        let same_line = to.2 == side && if along_x { to.1 == from.1 } else { to.0 == from.0 };
        let (lo, hi) = if same_line { (a.min(c), a.max(c)) } else { (a, a) };
        let cell = |k: i32| if along_x { (k, from.1) } else { (from.0, k) };
        // only as far as the wall goes without a break, from where the drag started
        let mut s = a;
        while s > lo && b.is_edge(cell(s - 1).0, cell(s - 1).1, side) {
            s -= 1;
        }
        let mut e = a;
        while e < hi && b.is_edge(cell(e + 1).0, cell(e + 1).1, side) {
            e += 1;
        }
        let start = cell(s);
        Opening { prop: String::new(), cell: [start.0, start.1], side: side.into(), width: (e - s + 1) as u8 }
    }

    /// The nearest building along a ray (by its cells' columns).
    pub(super) fn pick_building(&self, origin: Vec3, dir: Vec3) -> Option<(f32, usize)> {
        let lh = self.level_height;
        let mut best: Option<(f32, usize)> = None;
        for (i, b) in self.map.buildings.iter().enumerate() {
            let y0 = b.level as f32 * lh;
            let y1 = y0 + b.floors.len() as f32 * lh + 1.0;
            for (x, z) in b.map_cells() {
                let (lo, hi) = (Vec3::new(x as f32, y0, z as f32), Vec3::new(x as f32 + 1.0, y1, z as f32 + 1.0));
                let inv = dir.recip();
                let (t1, t2) = ((lo - origin) * inv, (hi - origin) * inv);
                let (tmin, tmax) = (t1.min(t2).max_element(), t1.max(t2).min_element());
                if tmax >= tmin.max(0.0) && best.map(|(t, _)| tmin < t).unwrap_or(true) {
                    best = Some((tmin, i));
                }
            }
        }
        best
    }

    // ------------------------------------------------------------ the view

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_view(&mut self, origin: Vec3, dir: Vec3, alt: bool, pressed: bool, down: bool, released: bool, over_view: bool) {
        let lh = self.level_height;
        if self.draft.is_none() {
            if !(pressed && over_view) {
                return;
            }
            self.start_building(None);
        }
        let step = self.draft.as_ref().map(|d| d.step).unwrap_or(Step::Footprint);
        match step {
            Step::Footprint => {
                if let Some(d) = &mut self.draft {
                    d.b.level = self.level;
                }
                if pressed && over_view {
                    self.stroke = true;
                }
                if self.stroke && down {
                    let taken = self.taken(self.draft.as_ref().and_then(|d| d.editing));
                    if let Some(p) = Self::plane(origin, dir, self.level as f32 * lh) {
                        let centre = (p.x.floor() as i32, p.z.floor() as i32);
                        for c in self.line_to(centre) {
                            for cell in self.brush_cells(c) {
                                let remove = self.remove_cells != alt;
                                let d = self.draft.as_mut().unwrap();
                                if remove {
                                    d.cells.retain(|&x| x != cell);
                                } else if !d.cells.contains(&cell) && !taken.contains(&cell) {
                                    d.cells.push(cell);
                                }
                            }
                        }
                    }
                }
            }
            Step::Design => {
                let Some(d) = &self.draft else { return };
                let clicks = d.clicks;
                if !clicks.structure() {
                    self.rooms_view(origin, dir, pressed && over_view);
                    return;
                }
                let y = (d.b.level as usize + d.floor) as f32 * lh;
                let Some(p) = Self::plane(origin, dir, y) else { return };
                match clicks {
                    Clicks::Openings => {
                        let d = self.draft.as_mut().unwrap();
                        d.hover_edge = Self::nearest_edge(&d.b, p);
                        if pressed && over_view {
                            d.drag_from = d.hover_edge;
                            d.span = None;
                        }
                        if let (Some(from), true) = (d.drag_from, down) {
                            let to = d.hover_edge.unwrap_or(from);
                            let mut s = Self::span(&d.b, from, to);
                            // the asset picked before stays if it's as wide
                            if let Some(old) = &d.span {
                                if old.width == s.width {
                                    s.prop = old.prop.clone();
                                }
                            }
                            d.span = Some(s);
                        }
                        if released {
                            d.drag_from = None;
                        }
                    }
                    Clicks::Pillars => {
                        let d = self.draft.as_mut().unwrap();
                        let rel = (p.x - d.b.at[0] as f32, p.z - d.b.at[1] as f32);
                        d.hover_point = d.b.corners().into_iter().find(|c| (c[0] as f32 - rel.0).powi(2) + (c[1] as f32 - rel.1).powi(2) < 0.2);
                        if pressed && over_view {
                            if let (Some(c), Some(prop)) = (d.hover_point, d.pillar_prop.clone()) {
                                match d.b.pillars.iter().position(|q| q.corner == c) {
                                    Some(i) => {
                                        d.b.pillars.remove(i);
                                    }
                                    None => d.b.pillars.push(Pillar { prop, corner: c }),
                                }
                                self.need_buildings = true;
                            }
                        }
                    }
                    Clicks::Tiles => {
                        let d = self.draft.as_mut().unwrap();
                        let rel = ((p.x - d.b.at[0] as f32).floor() as i32, (p.z - d.b.at[1] as f32).floor() as i32);
                        d.hover_cell = d.b.has(rel.0, rel.1).then_some(rel);
                        if pressed && over_view {
                            d.painting = true;
                        }
                        if let (true, true, Some((x, z))) = (d.painting, down, d.hover_cell) {
                            match self.lay(x, z) {
                                Ok(true) => self.need_buildings = true,
                                Ok(false) => {}
                                Err(why) => self.status = why,
                            }
                        }
                        if released {
                            if let Some(d) = &mut self.draft {
                                d.painting = false;
                            }
                        }
                    }
                    Clicks::Walls => {
                        let d = self.draft.as_mut().unwrap();
                        d.hover_edge = Self::nearest_edge(&d.b, p);
                        if pressed && over_view {
                            d.painting = true;
                        }
                        if let (true, true, Some((x, z, side))) = (d.painting, down, d.hover_edge) {
                            let wall = d.b.wall.clone();
                            let floors: Vec<usize> = if d.every_floor { (0..d.b.floors.len()).collect() } else { vec![d.floor] };
                            let mut changed = false;
                            for f in floors {
                                // the inside face keeps its own style whatever the outside becomes
                                if d.inner_face {
                                    let before = d.b.wall_inside(f, x, z, side).to_string();
                                    d.b.floors[f].set_inner(x, z, side, &d.wall_brush);
                                    changed |= d.b.wall_inside(f, x, z, side) != before;
                                } else {
                                    let before = d.b.floors[f].wall_at(x, z, side, &wall).to_string();
                                    d.b.floors[f].set_wall(x, z, side, &d.wall_brush, &wall);
                                    changed |= d.b.floors[f].wall_at(x, z, side, &wall) != before;
                                }
                            }
                            if changed {
                                self.need_buildings = true;
                            }
                        }
                        if released {
                            if let Some(d) = &mut self.draft {
                                d.painting = false;
                            }
                        }
                    }
                    Clicks::Inside | Clicks::Outside => {}
                }
            }
        }
    }

    /// The flooring brush laid on a cell (from `at`) of the floor being designed: Ok(true)
    /// when it changed. A hole only above the ground floor and where nothing stands; the
    /// stairs' way up stays open.
    fn lay(&mut self, x: i32, z: i32) -> Result<bool, String> {
        let content = self.content.clone();
        let stairs = |id: &str| content.props.get(id).filter(|p| p.category == "stairs").map(|p| p.footprint);
        let standing: Vec<String> = {
            let Some(d) = &self.draft else { return Ok(false) };
            d.b.floors[d.floor].props.iter().filter(|p| super::rooms::cells_of(self, p).contains(&(x, z))).map(|p| content.props.get(&p.prop).map(|q| q.name.clone()).unwrap_or_else(|| p.prop.clone())).collect()
        };
        let Some(d) = &mut self.draft else { return Ok(false) };
        let f = d.floor;
        if d.b.stairwells(f, &stairs).contains(&(x, z)) {
            return Ok(false);
        }
        let floor = &mut d.b.floors[f];
        let state = |floor: &content::Floor| (floor.tile_at(x, z).to_string(), floor.hole(x, z).map(|h| h.railing.clone()));
        let before = state(floor);
        if d.tile_brush == HOLE {
            if f == 0 {
                return Err("The ground floor has nothing under it: holes go in the floors above".into());
            }
            if let Some(name) = standing.first() {
                return Err(format!("{name} stands there: move it first (Inside)"));
            }
            // a hole painted again takes the railing picked
            floor.set_hole(x, z, Some(&d.railing_brush));
        } else {
            floor.set_tile(x, z, &d.tile_brush);
        }
        Ok(before != state(floor))
    }

    /// The Build tool's marks: on the ground while the footprint is drawn; over everything
    /// while designing (`overlays`), so floors and walls never hide where things go.
    pub(super) fn build_shapes(&self, shapes: &mut Vec<Shape>, overlays: &mut Vec<Shape>) {
        let lh = self.level_height;
        let rect = |x: f32, y: f32, z: f32, w: f32, d: f32, color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: w, depth: d }, center: Vec3::new(x, y, z), color, additive: false };
        let Some(d) = &self.draft else {
            if let Some(p) = self.plane_hit {
                shapes.push(rect(p.x.floor() + 0.5, p.y + 0.04, p.z.floor() + 0.5, 1.0, 1.0, [0.4, 0.7, 1.0, 0.4]));
            }
            return;
        };
        match d.step {
            Step::Footprint => {
                let y = d.b.level as f32 * lh + 0.05;
                let (at, rows) = Building::rows_for(&d.cells);
                let probe = Building { at, cells: rows, ..Default::default() };
                let whole = probe.connected();
                for &(x, z) in &d.cells {
                    let c = if whole { [0.3, 0.55, 1.0, 0.5] } else { [1.0, 0.35, 0.3, 0.5] };
                    shapes.push(rect(x as f32 + 0.5, y, z as f32 + 0.5, 0.96, 0.96, c));
                }
                if let Some(p) = self.plane_hit {
                    let size = (self.brush * 2 + 1) as f32;
                    shapes.push(rect(p.x.floor() + 0.5, y + 0.01, p.z.floor() + 0.5, size, size, [1.0, 1.0, 1.0, 0.3]));
                }
            }
            Step::Design => {
                let y = (d.b.level as usize + d.floor) as f32 * lh + 0.1;
                let (ox, oz) = (d.b.at[0] as f32, d.b.at[1] as f32);
                let edge = |x: i32, z: i32, side: &str, color: [f32; 4], out: &mut Vec<Shape>| {
                    let (p, q) = segment_ends(x, z, side);
                    let (mx, mz) = (ox + (p[0] + q[0]) as f32 / 2.0, oz + (p[1] + q[1]) as f32 / 2.0);
                    let (dx, dz) = content::side_step(side).unwrap_or((0, 0));
                    let (w, dd) = if matches!(side, "n" | "s") { (1.0, 0.3) } else { (0.3, 1.0) };
                    out.push(rect(mx + dx as f32 * 0.2, y, mz + dz as f32 * 0.2, w, dd, color));
                };
                match d.clicks {
                    Clicks::Openings => {
                        for o in &d.b.floors[d.floor].openings {
                            for (x, z) in Building::segments(o) {
                                edge(x, z, &o.side, [0.3, 0.85, 0.4, 0.55], overlays);
                            }
                        }
                        if let Some((x, z, side)) = d.hover_edge {
                            edge(x, z, side, [1.0, 1.0, 1.0, 0.5], overlays);
                        }
                        if let Some(s) = &d.span {
                            for (x, z) in Building::segments(s) {
                                edge(x, z, &s.side, [1.0, 0.6, 0.15, 0.75], overlays);
                            }
                        }
                    }
                    Clicks::Pillars => {
                        for c in d.b.corners() {
                            let on = d.b.pillars.iter().any(|p| p.corner == c);
                            let hover = d.hover_point == Some(c);
                            let color = if hover { [1.0, 1.0, 1.0, 0.8] } else if on { [0.3, 0.85, 0.4, 0.6] } else { [1.0, 1.0, 1.0, 0.25] };
                            overlays.push(rect(ox + c[0] as f32, y + 0.01, oz + c[1] as f32, 0.3, 0.3, color));
                        }
                    }
                    Clicks::Tiles => {
                        // the floor's holes, and the cell under the pointer
                        for h in d.b.floors[d.floor].holes.iter().filter(|_| d.floor > 0) {
                            overlays.push(rect(ox + h.cell[0] as f32 + 0.5, y - 0.02, oz + h.cell[1] as f32 + 0.5, 0.9, 0.9, [0.1, 0.12, 0.3, 0.35]));
                        }
                        if let Some((x, z)) = d.hover_cell {
                            let c = if d.tile_brush == HOLE { [1.0, 0.55, 0.3, 0.5] } else { [1.0, 1.0, 1.0, 0.45] };
                            overlays.push(rect(ox + x as f32 + 0.5, y, oz + z as f32 + 0.5, 0.96, 0.96, c));
                        }
                    }
                    Clicks::Walls => {
                        if let Some((x, z, side)) = d.hover_edge {
                            edge(x, z, side, [1.0, 1.0, 1.0, 0.6], overlays);
                        }
                    }
                    Clicks::Inside | Clicks::Outside => self.rooms_overlays(overlays),
                }
            }
        }
    }

    // ------------------------------------------------------------ the panel

    pub(super) fn build_panel(&mut self, ui: &mut egui::Ui) {
        let Some(step) = self.draft.as_ref().map(|d| d.step) else {
            section(ui, "Build");
            hint(ui, "Click the map to start a building and draw its cells.");
            if ui.button("New building").clicked() {
                self.start_building(None);
            }
            return;
        };
        match step {
            Step::Footprint => self.footprint_panel(ui),
            Step::Design => self.design_panel(ui),
        }
    }

    fn footprint_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Footprint");
        hint(ui, &format!("Paint the cells it stands on, at level {}: any shape, in one piece.", self.level));
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.remove_cells, false, "Add cells");
            ui.radio_value(&mut self.remove_cells, true, "Remove");
        });
        ui.add(egui::Slider::new(&mut self.brush, 0..=2).custom_formatter(|v, _| format!("{0}×{0}", v as i32 * 2 + 1)));
        let d = self.draft.as_ref().unwrap();
        let (at, rows) = Building::rows_for(&d.cells);
        let whole = Building { at, cells: rows, ..Default::default() }.connected();
        ui.label(format!("{} cells", d.cells.len()));
        if !d.cells.is_empty() && !whole {
            warn(ui, "Not one piece yet: every cell must touch another.");
        }
        let ok = !d.cells.is_empty() && whole;
        ui.horizontal(|ui| {
            let next = ui.add_enabled(ok, egui::Button::new(RichText::new("Next: the building").strong()));
            super::mark("uiNext", &next);
            if next.clicked() {
                self.open_design();
            }
            if ui.button("Cancel").clicked() {
                self.cancel_building();
            }
        });
    }

    fn design_panel(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let (mut confirm, mut cancel, mut reshape) = (false, false, false);
        {
            let d = self.draft.as_mut().unwrap();
            section(ui, if d.editing.is_some() { "Building" } else { "New building" });
            ui.horizontal(|ui| {
                let c = ui.button(RichText::new("Confirm").strong()).on_hover_text("Put it on the map");
                super::mark("uiConfirm", &c);
                confirm = c.clicked();
                reshape = ui.button("Footprint").on_hover_text("Change the cells it stands on").clicked();
                let c = ui.button("Cancel").on_hover_text("Drop the changes");
                super::mark("uiCancelBuilding", &c);
                cancel = c.clicked();
            });
            for p in &d.problems {
                warn(ui, p);
            }
            ui.horizontal(|ui| {
                ui.label("Name");
                changed |= ui.add(egui::TextEdit::singleline(&mut d.b.name).desired_width(f32::INFINITY)).changed();
            });
            section(ui, "Floor");
            ui.horizontal_wrapped(|ui| {
                for f in 0..d.b.floors.len() {
                    let about = if f == 0 { "The ground floor".to_string() } else { format!("Floor {}: the floors above are cut away while you work on it", f + 1) };
                    let r = ui.selectable_label(d.floor == f, (f + 1).to_string()).on_hover_text(about);
                    super::mark(&format!("uiFloor{}", f + 1), &r);
                    if r.clicked() && d.floor != f {
                        d.floor = f;
                        d.span = None;
                        if matches!(d.sel, Some(Sel::Floor(_))) {
                            d.sel = None;
                        }
                        changed = true;
                    }
                }
                let add = ui.small_button("+ floor").on_hover_text("A floor on top, with the same flooring");
                super::mark("uiAddFloor", &add);
                if add.clicked() {
                    let tile = d.b.floors.last().map(|f| f.tile.clone()).unwrap_or_default();
                    d.b.floors.push(Floor { tile, ..Default::default() });
                    d.floor = d.b.floors.len() - 1;
                    d.sel = None;
                    changed = true;
                }
                if ui.add_enabled(d.b.floors.len() > 1, egui::Button::new("− top").small()).on_hover_text("Remove the top floor").clicked() {
                    d.b.floors.pop();
                    d.floor = d.floor.min(d.b.floors.len() - 1);
                    let floors = d.b.floors.len();
                    d.b.anchors.retain(|a| (a.floor as usize) < floors);
                    d.sel = None;
                    changed = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let s = ui.selectable_label(d.clicks.structure(), "Structure").on_hover_text("Flooring, doors and windows, pillars, walls and roof");
                super::mark("uiStructure", &s);
                if s.clicked() && !d.clicks.structure() {
                    d.clicks = Clicks::Openings;
                }
                let i = ui.selectable_label(d.clicks == Clicks::Inside, "Inside").on_hover_text("What stands on this floor and hangs on its walls");
                super::mark("uiInside", &i);
                if i.clicked() {
                    d.clicks = Clicks::Inside;
                }
                let o = ui.selectable_label(d.clicks == Clicks::Outside, "Outside").on_hover_text("What hangs on a wall outside, or stands in front of it");
                super::mark("uiOutside", &o);
                if o.clicked() && d.clicks != Clicks::Outside {
                    d.clicks = Clicks::Outside;
                    d.wall = None;
                }
            });
            if d.sel.is_some() && d.clicks.structure() {
                d.sel = None;
            }
        }
        let clicks = self.draft.as_ref().unwrap().clicks;
        match clicks {
            Clicks::Inside => self.inside_panel(ui),
            Clicks::Outside => self.outside_panel(ui),
            Clicks::Openings | Clicks::Pillars | Clicks::Tiles | Clicks::Walls => changed |= self.structure_panel(ui),
        }
        if changed {
            self.need_buildings = true;
            if let Some(d) = &mut self.draft {
                d.problems.clear();
            }
        }
        if reshape {
            if let Some(d) = &mut self.draft {
                d.cells = d.b.map_cells();
                d.step = Step::Footprint;
                self.level = d.b.level;
            }
        }
        if confirm {
            self.confirm_building();
        }
        if cancel {
            self.cancel_building();
        }
    }

    /// Flooring, walls and roof; then doors and windows, or pillars.
    fn structure_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let (tiles, walls, roofs, pillars) = (choices(self, "floor"), choices(self, "wall"), choices(self, "roof"), choices(self, "pillar"));
        let rails = railing_styles(self);
        let doors = ids_of(self, &["door"]);
        let windows = ids_of(self, &["window"]);
        let content = self.content.clone();
        let mut changed = false;
        let d = self.draft.as_mut().unwrap();
        section(ui, "Style");
        egui::Grid::new("building-style").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Floor");
            changed |= pick(ui, "tile", &mut d.b.floors[d.floor].tile, &tiles);
            ui.end_row();
            ui.label("Walls");
            changed |= pick(ui, "wall", &mut d.b.wall, &walls);
            ui.end_row();
            // the inside faces: as each one's outside, or all of one style (a face painted
            // on its own keeps its own)
            ui.label("Inside");
            let outside_color = walls.iter().find(|w| w.0 == d.b.wall).map(|w| w.2.clone()).unwrap_or_else(|| "#888888".into());
            let mut inner = vec![(String::new(), "As the outside".to_string(), outside_color)];
            inner.extend(walls.iter().cloned());
            changed |= pick(ui, "inner_wall", &mut d.b.inner_wall, &inner);
            ui.end_row();
            ui.label("Roof");
            changed |= pick(ui, "roof", &mut d.b.roof, &roofs);
            ui.end_row();
        });
        if d.b.is_rectangle() {
            let flat = ui.checkbox(&mut d.b.flat_roof, "Flat roof").on_hover_text("A terrace with a parapet instead of a gabled roof: stairs on the top floor come up to it");
            super::mark("uiFlatRoof", &flat);
            changed |= flat.changed();
        } else {
            hint(ui, "A flat roof (the outline isn't a rectangle): stairs can come up to it.");
        }
        section(ui, "Clicking places");
        ui.horizontal_wrapped(|ui| {
            super::mark("uiOpenings", &ui.radio_value(&mut d.clicks, Clicks::Openings, "Doors & windows"));
            super::mark("uiPillars", &ui.radio_value(&mut d.clicks, Clicks::Pillars, "Pillars"));
            super::mark("uiTiles", &ui.radio_value(&mut d.clicks, Clicks::Tiles, "Flooring").on_hover_text("Each cell of this floor its flooring; above the ground floor, holes"));
            super::mark("uiWallStyles", &ui.radio_value(&mut d.clicks, Clicks::Walls, "Walls").on_hover_text("Each segment of this floor's walls its style"));
        });
        // a brush: its swatch and name, picked by a click
        let brush = |ui: &mut egui::Ui, current: &mut String, id: &str, name: &str, color: Option<&str>, mark: &str| {
            let r = ui
                .horizontal(|ui| {
                    if let Some(c) = color {
                        super::swatch(ui, c);
                    }
                    ui.selectable_label(current == id, name)
                })
                .inner;
            super::mark(mark, &r);
            if r.clicked() {
                *current = id.to_string();
            }
            r
        };
        match d.clicks {
            Clicks::Openings => {
                ui.horizontal(|ui| {
                    if ui.radio(d.doors, "Doors").clicked() {
                        d.doors = true;
                        if let Some(s) = &mut d.span {
                            s.prop.clear();
                        }
                    }
                    let windows = ui.radio(!d.doors, "Windows");
                    super::mark("uiWindows", &windows);
                    if windows.clicked() {
                        d.doors = false;
                        if let Some(s) = &mut d.span {
                            s.prop.clear();
                        }
                    }
                });
                hint(ui, "Drag along an outside wall: the span is the width.");
                if let Some(span) = d.span.clone() {
                    let wall = match span.side.as_str() { "n" => "north", "s" => "south", "e" => "east", _ => "west" };
                    ui.label(format!("{} cell{} of the {wall} wall", span.width, if span.width == 1 { "" } else { "s" }));
                    let list = if d.doors { &doors } else { &windows };
                    let fitting: Vec<&(String, String, [u8; 2], String)> = list.iter().filter(|(_, _, fp, _)| fp[0] == span.width).collect();
                    if fitting.is_empty() {
                        warn(ui, &format!("No {} {} cells wide: drag a different span.", if d.doors { "door" } else { "window" }, span.width));
                    }
                    for (k, (id, name, _, _)) in fitting.into_iter().enumerate() {
                        let put = ui.button(format!("Put a {name} here"));
                        if k == 0 {
                            super::mark("uiPut", &put);
                        }
                        if put.clicked() {
                            let floor = &mut d.b.floors[d.floor];
                            let segs = Building::segments(&span);
                            // what was in the way goes
                            floor.openings.retain(|o| o.side != span.side || !Building::segments(o).iter().any(|s| segs.contains(s)));
                            floor.openings.push(Opening { prop: id.clone(), ..span.clone() });
                            d.span = None;
                            changed = true;
                        }
                    }
                }
                let mut remove = None;
                for (i, o) in d.b.floors[d.floor].openings.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let name = content.props.get(&o.prop).map(|p| p.name.clone()).unwrap_or_else(|| o.prop.clone());
                        let wall = match o.side.as_str() { "n" => "north", "s" => "south", "e" => "east", _ => "west" };
                        ui.label(format!("{name} · {wall} wall"));
                        if ui.small_button("remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    d.b.floors[d.floor].openings.remove(i);
                    changed = true;
                }
            }
            Clicks::Pillars => {
                hint(ui, "Click a point of the outline to put or take away a pillar.");
                let mut current = d.pillar_prop.clone().unwrap_or_default();
                if pick(ui, "pillar", &mut current, &pillars) {
                    d.pillar_prop = Some(current);
                }
            }
            Clicks::Tiles => {
                hint(ui, "Click or drag over this floor's cells: each takes the flooring picked.");
                if d.floor == 0 && d.tile_brush == HOLE {
                    d.tile_brush.clear();
                }
                // one a line (the panel is narrow): every flooring, the floor's own, a hole
                for (id, name, color) in &tiles {
                    brush(ui, &mut d.tile_brush, id, name, Some(color.as_str()), &format!("uiTileBrush_{id}"));
                }
                let own = d.b.floors[d.floor].tile.clone();
                let (own_name, own_color) = tiles.iter().find(|t| t.0 == own).map(|t| (t.1.clone(), t.2.clone())).unwrap_or((own, "#888888".into()));
                brush(ui, &mut d.tile_brush, "", &format!("As the floor ({own_name})"), Some(own_color.as_str()), "uiTileOwn").on_hover_text("Back to this floor's own flooring (Style: Floor)");
                if d.floor > 0 {
                    brush(ui, &mut d.tile_brush, HOLE, "Empty: a hole", Some("#1c2130"), "uiTileHole").on_hover_text("No floor there: open down to the floor below (an inner balcony), with the railing picked below or none");
                } else {
                    hint(ui, "Holes go in the floors above: there's nothing under the ground floor.");
                }
                // a hole's railing: a style (its pieces put together along the edge) or none
                if d.tile_brush == HOLE {
                    ui.indent("hole-railing", |ui| {
                        ui.label(RichText::new("Its railing").size(12.0));
                        for (id, name, color) in &rails {
                            brush(ui, &mut d.railing_brush, id, name, Some(color.as_str()), &format!("uiRailing_{id}"));
                        }
                        brush(ui, &mut d.railing_brush, "", "No railing", Some("#1c2130"), "uiRailingNone");
                        hint(ui, "A hole painted again takes the railing picked.");
                    });
                }
                let fl = &d.b.floors[d.floor];
                let (n, h) = (fl.tiles.len(), fl.holes.len());
                ui.label(format!("{n} cell{} of another flooring, {h} hole{}", if n == 1 { "" } else { "s" }, if h == 1 { "" } else { "s" }));
                let all = ui.add_enabled(n + h > 0, egui::Button::new("All as the floor").small()).on_hover_text("Every cell of this floor back to its own flooring, its holes closed");
                if all.clicked() {
                    let fl = &mut d.b.floors[d.floor];
                    fl.tiles.clear();
                    fl.holes.clear();
                    changed = true;
                }
            }
            Clicks::Walls => {
                // which face: the outside, or the inside (kept whatever the outside becomes)
                ui.horizontal(|ui| {
                    super::mark("uiWallFaceOutside", &ui.radio_value(&mut d.inner_face, false, "Outside"));
                    super::mark("uiWallFaceInside", &ui.radio_value(&mut d.inner_face, true, "Inside"));
                });
                hint(ui, if d.inner_face { "Click or drag along this floor's walls: each segment's inside takes the style picked, and keeps it whatever its outside becomes." } else { "Click or drag along this floor's walls: each segment's outside takes the style picked." });
                for (id, name, color) in &walls {
                    brush(ui, &mut d.wall_brush, id, name, Some(color.as_str()), &format!("uiWallBrush_{id}"));
                }
                let find = |id: &str| walls.iter().find(|w| w.0 == id).map(|w| (w.1.clone(), w.2.clone())).unwrap_or((id.to_string(), "#888888".into()));
                let (own, about) = match (d.inner_face, d.b.inner_wall.is_empty()) {
                    (false, _) => (format!("As the building ({})", find(&d.b.wall).0), "Back to the building's own walls (Style: Walls)"),
                    (true, true) => ("As the outside".to_string(), "Back to as its outside is (Style: Inside)"),
                    (true, false) => (format!("As the building's inside ({})", find(&d.b.inner_wall).0), "Back to the building's inside walls (Style: Inside)"),
                };
                let own_color = find(if d.inner_face && !d.b.inner_wall.is_empty() { &d.b.inner_wall } else { &d.b.wall }).1;
                brush(ui, &mut d.wall_brush, "", &own, Some(own_color.as_str()), "uiWallOwn").on_hover_text(about);
                let every = ui.checkbox(&mut d.every_floor, "On every floor").on_hover_text("A segment takes the style all the way up, not just on this floor");
                super::mark("uiWallEveryFloor", &every);
                let fl = &d.b.floors[d.floor];
                let n = if d.inner_face { fl.inner.len() } else { fl.walls.len() };
                let face = if d.inner_face { "inside" } else { "outside" };
                ui.label(format!("{n} segment{} with an {face} of its own on this floor", if n == 1 { "" } else { "s" }));
                if ui.add_enabled(n > 0, egui::Button::new(if d.inner_face { "All insides as the building" } else { "All as the building" }).small()).clicked() {
                    let fl = &mut d.b.floors[d.floor];
                    if d.inner_face {
                        fl.inner.clear();
                    } else {
                        fl.walls.clear();
                    }
                    changed = true;
                }
            }
            Clicks::Inside | Clicks::Outside => {}
        }
        // for the checks: this floor's own cells and walls, the brushes, the cell pointed at
        let fl = &d.b.floors[d.floor];
        crate::platform::set_hook("buildtiles", &fl.tiles.len().to_string());
        crate::platform::set_hook("buildholes", &fl.holes.len().to_string());
        crate::platform::set_hook("buildwalls", &fl.walls.len().to_string());
        crate::platform::set_hook("buildinner", &fl.inner.len().to_string());
        crate::platform::set_hook("buildrailed", &fl.holes.iter().filter(|h| !h.railing.is_empty()).map(|h| h.railing.as_str()).collect::<Vec<_>>().join(","));
        crate::platform::set_hook("railingbrush", &d.railing_brush);
        crate::platform::set_hook("wallface", if d.inner_face { "inside" } else { "outside" });
        crate::platform::set_hook("tilebrush", &d.tile_brush);
        crate::platform::set_hook("wallbrush", &d.wall_brush);
        crate::platform::set_hook("hovercell", &d.hover_cell.map(|(x, z)| format!("{x},{z}")).unwrap_or_default());
        changed
    }

    // ------------------------------------------------------------ buildings on the map

    /// The selected building's inspector.
    pub(super) fn building_inspector(&mut self, ui: &mut egui::Ui, i: usize) {
        let b = self.map.buildings[i].clone();
        ui.label(RichText::new(&b.name).size(16.0).strong());
        hint(ui, &format!("{} · {} cells · {} floor{}", b.id, b.cells().len(), b.floors.len(), if b.floors.len() == 1 { "" } else { "s" }));
        let inside: usize = b.floors.iter().map(|f| f.props.len()).sum::<usize>() + b.anchors.iter().filter(|a| a.inside).count();
        let outside = b.outside.len() + b.anchors.iter().filter(|a| !a.inside).count();
        hint(ui, &format!("{inside} thing{} inside, {outside} outside", if inside == 1 { "" } else { "s" }));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let edit = ui.button("Edit").on_hover_text("Its structure, what's inside, what's outside");
            super::mark("uiEditBuilding", &edit);
            if edit.clicked() {
                self.start_building(Some(i));
            }
            if ui.button("Turn").on_hover_text("A quarter turn (Q / E)").clicked() {
                self.turn_building(i);
            }
            let export = ui.button("To Blender").on_hover_text("This building as a .glb");
            super::mark("uiExportBuilding", &export);
            if export.clicked() {
                self.export_building(i);
            }
            if ui.button("Delete").on_hover_text("Delete").clicked() {
                self.checkpoint();
                self.map.buildings.remove(i);
                self.selected_building = None;
                self.need_buildings = true;
            }
            let t = ui.button("Make a template").on_hover_text("This building with what's inside and outside it, to put down again anywhere (Select's Templates)");
            super::mark("uiTemplateBuilding", &t);
            if t.clicked() {
                self.template_of_building(i);
            }
        });
        hint(ui, "It fills its cells: it stays where it was built. Edit changes it, Turn turns it on its cells.");
    }

    pub(super) fn turn_building(&mut self, i: usize) {
        let turned = self.map.buildings[i].turned();
        let taken = self.taken(Some(i));
        if turned.map_cells().iter().any(|&(x, z)| !self.map.inside(x, z) || taken.contains(&(x, z))) {
            self.status = "No room to turn it there".into();
            return;
        }
        self.checkpoint();
        self.map.buildings[i] = turned;
        self.settle(i);
    }

    /// Q/E and Delete while designing: on what's selected inside or outside; with nothing
    /// selected, Q/E turn what's about to be placed.
    pub(super) fn build_keys(&mut self, turn: f32, delete: bool) -> bool {
        let Some(d) = &self.draft else { return false };
        if d.step != Step::Design || d.clicks.structure() {
            return false;
        }
        if d.sel.is_some() {
            return self.rooms_keys(turn, delete);
        }
        if turn != 0.0 {
            self.turn = (self.turn + turn).rem_euclid(360.0);
            return true;
        }
        false
    }
}
