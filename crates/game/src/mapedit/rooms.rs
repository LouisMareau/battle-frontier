//! Inside and outside a building being edited. Inside: furniture on the floor being edited
//! (all of it within the footprint) and things hung on the inner faces of its walls.
//! Outside: first a wall (one of the outline's straight runs; around a court too), then
//! things hung on that wall's outer face or standing on the ground in front of it. The two
//! lists are the asset folders "Buildings/Inside" and "Buildings/Outside". Nothing lands
//! anywhere else, unstackable things never overlap anything, and whatever is placed can be
//! selected (a click), turned (Q/E) and removed (Delete, or its button).

use content::{segment_ends, side_step, Anchor, Building, Placed, INSIDE, OUTSIDE};
use egui::RichText;
use glam::{Vec2, Vec3};
use render::scene::{Shape, ShapeKind};
use world::buildings::WALL;

use super::build::{Clicks, Sel};
use super::{hint, section, warn, MapEditor};

/// How far in front of a wall outside things may stand, in cells.
const FRONT: i32 = 2;

/// A point on a wall: (cell, side, floor, height above the floor).
pub(super) type WallHit = ([i32; 2], &'static str, usize, f64);

/// The cells a placed prop covers (map cells).
pub(super) fn cells_of(ed: &MapEditor, p: &Placed) -> Vec<(i32, i32)> {
    ed.map.footprint(p, &ed.content)
}

/// Would `new` (covering `cells`) clash with one of `others`? Unstackable things share no
/// cell with anything. Returns what's in the way.
pub(super) fn clash(ed: &MapEditor, new: &str, cells: &[(i32, i32)], others: &[Placed]) -> Option<String> {
    let solid = |id: &str| ed.content.props.get(id).map(|p| p.unstackable).unwrap_or(false);
    for o in others {
        if !(solid(new) || solid(&o.prop)) {
            continue;
        }
        let theirs = cells_of(ed, o);
        if cells.iter().any(|c| theirs.contains(c)) {
            return Some(ed.content.props.get(&o.prop).map(|p| p.name.clone()).unwrap_or_else(|| o.prop.clone()));
        }
    }
    None
}

/// What stands on the map's ground at a level: its props and what stands outside its
/// buildings (in map coordinates), but `except` (a prop of the map).
pub(super) fn ground_items(ed: &MapEditor, level: u8, except: Option<usize>) -> Vec<Placed> {
    let mut out: Vec<Placed> = ed.map.props.iter().enumerate().filter(|(i, p)| p.level == level && Some(*i) != except).map(|(_, p)| p.clone()).collect();
    for b in ed.map.buildings.iter().filter(|b| b.level == level) {
        out.extend(on_map(b, &b.outside, b.level));
    }
    out
}

/// A building's items as map placements (from relative to map coordinates).
fn on_map(b: &Building, items: &[Placed], level: u8) -> Vec<Placed> {
    items.iter().map(|p| Placed { at: [p.at[0] + b.at[0] as f64, p.at[1] + b.at[1] as f64], level, ..p.clone() }).collect()
}

/// The straight run of wall a segment belongs to (cells from the building's corner).
pub(super) fn run_of(b: &Building, seg: (i32, i32, &str)) -> Vec<(i32, i32)> {
    let (x, z, side) = seg;
    let along = if matches!(side, "n" | "s") { (1, 0) } else { (0, 1) };
    let mut out = vec![(x, z)];
    for dir in [-1, 1] {
        let mut c = (x + along.0 * dir, z + along.1 * dir);
        while b.is_edge(c.0, c.1, side) {
            out.push(c);
            c = (c.0 + along.0 * dir, c.1 + along.1 * dir);
        }
    }
    out.sort();
    out
}

/// The ground in front of a run of wall: up to FRONT cells out, off the building, on the map
/// (map cells).
pub(super) fn front_of(ed: &MapEditor, b: &Building, run: &[(i32, i32)], side: &str) -> Vec<(i32, i32)> {
    let (dx, dz) = side_step(side).unwrap_or((0, 0));
    let taken: Vec<(i32, i32)> = ed.map.buildings.iter().filter(|o| o.id != b.id).flat_map(|o| o.map_cells()).collect();
    let mut out = vec![];
    for &(x, z) in run {
        for k in 1..=FRONT {
            let (cx, cz) = (x + dx * k, z + dz * k);
            if b.has(cx, cz) {
                break;
            }
            let m = (cx + b.at[0], cz + b.at[1]);
            if ed.map.inside(m.0, m.1) && !taken.contains(&m) && !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

impl MapEditor {
    /// The wall face under a ray: inner faces of a floor, or outer faces (`only` those of a
    /// run of wall: its cells and side).
    pub(super) fn pick_face(&self, b: &Building, origin: Vec3, dir: Vec3, inside: bool, floor: Option<usize>, only: Option<(&[(i32, i32)], &str)>) -> Option<WallHit> {
        let lh = self.level_height;
        let mut best: Option<(f32, WallHit)> = None;
        for (x, z, side) in b.edges() {
            if let Some((run, run_side)) = only {
                if run_side != side || !run.contains(&(x, z)) {
                    continue;
                }
            }
            let (p, q) = segment_ends(x, z, side);
            let (p, q) = ([p[0] + b.at[0], p[1] + b.at[1]], [q[0] + b.at[0], q[1] + b.at[1]]);
            let along_x = matches!(side, "n" | "s");
            let (dx, dz) = side_step(side).unwrap_or((0, 0));
            // only faces turned to the camera: an inner face from inside, an outer one from out
            let facing = dir.x * dx as f32 + dir.z * dz as f32;
            if (inside && facing <= 0.0) || (!inside && facing >= 0.0) {
                continue;
            }
            // the inner face is a wall's thickness in from the outline
            let inward = if inside { WALL } else { 0.0 };
            let (plane, d, o) = if along_x { (p[1] as f32 - dz as f32 * inward, dir.z, origin.z) } else { (p[0] as f32 - dx as f32 * inward, dir.x, origin.x) };
            if d.abs() < 1e-5 {
                continue;
            }
            let t = (plane - o) / d;
            if t <= 0.0 {
                continue;
            }
            let hit = origin + dir * t;
            let a = if along_x { hit.x } else { hit.z };
            let (a0, a1) = if along_x { (p[0] as f32, q[0] as f32) } else { (p[1] as f32, q[1] as f32) };
            let base = b.level as f32 * lh;
            let f = (((hit.y - base) / lh).floor() as i32).clamp(0, b.floors.len() as i32 - 1) as usize;
            if a < a0 || a > a1 || hit.y < base || hit.y > base + b.floors.len() as f32 * lh {
                continue;
            }
            if floor.map(|want| want != f).unwrap_or(false) {
                continue;
            }
            if best.map(|bb| t < bb.0).unwrap_or(true) {
                let height = ((hit.y - base - f as f32 * lh) as f64 * 20.0).round() / 20.0;
                best = Some((t, ([x, z], side, f, height.clamp(0.0, lh as f64 - 0.1))));
            }
        }
        best.map(|(_, h)| h)
    }

    /// The props of an asset folder.
    pub(super) fn folder_props(&self, folder: &str) -> Vec<(String, String, [u8; 2], bool, bool)> {
        self.content.props.iter().filter(|p| p.folders().iter().any(|f| f == folder)).map(|p| (p.id.clone(), p.name.clone(), p.footprint, p.unstackable, p.on_walls())).collect()
    }

    // ------------------------------------------------------------ the view

    pub(super) fn rooms_view(&mut self, origin: Vec3, dir: Vec3, pressed: bool) {
        let lh = self.level_height;
        let Some(d) = &self.draft else { return };
        let (clicks, floor, b) = (d.clicks, d.floor, d.b.clone());
        let y = (b.level as usize + floor) as f32 * lh;
        let rel = |p: Vec3| Vec2::new(p.x - b.at[0] as f32, p.z - b.at[1] as f32);
        match clicks {
            Clicks::Inside => {
                let prop = d.inside_prop.clone();
                let on_walls = prop.as_ref().and_then(|p| self.content.props.get(p)).map(|p| p.on_walls()).unwrap_or(false);
                let face = self.pick_face(&b, origin, dir, true, Some(floor), None);
                let ground = Self::plane(origin, dir, y);
                let d = self.draft.as_mut().unwrap();
                d.hover_face = if on_walls { face } else { None };
                d.hover_at = if on_walls { None } else { ground.map(|g| { let r = rel(g); [r.x as f64, r.y as f64] }) };
                if !pressed {
                    return;
                }
                // a click on something placed selects it
                if let Some(g) = ground {
                    let r = rel(g);
                    if let Some(i) = b.floors[floor].props.iter().position(|q| (Vec2::new(q.at[0] as f32, q.at[1] as f32) - r).length() < 0.5) {
                        self.draft.as_mut().unwrap().sel = Some(Sel::Floor(i));
                        return;
                    }
                }
                if let Some((cell, side, f, h)) = face {
                    if let Some(i) = b.anchors.iter().position(|a| a.inside && a.floor as usize == f && a.cell == cell && a.side == side && (a.height - h).abs() < 0.45) {
                        self.draft.as_mut().unwrap().sel = Some(Sel::Wall(i));
                        return;
                    }
                }
                let Some(prop) = prop else { return };
                if on_walls {
                    let Some((cell, side, f, height)) = face else {
                        self.status = "Hang it on an inside wall of this floor".into();
                        return;
                    };
                    self.status = format!("Hung {} on the wall", self.prop_name(&prop));
                    let d = self.draft.as_mut().unwrap();
                    d.b.anchors.push(Anchor { prop, cell, side: side.into(), floor: f as u8, height, inside: true });
                    d.sel = Some(Sel::Wall(d.b.anchors.len() - 1));
                    self.need_buildings = true;
                    return;
                }
                let Some(g) = ground else { return };
                let r = rel(g);
                let turn = self.turn as f64;
                let at = self.snap(&prop, turn as f32, Vec3::new(r.x, 0.0, r.y));
                let placed = Placed { prop: prop.clone(), at, level: 0, turn, scale: 1.0, ..Default::default() };
                if let Err(why) = self.inside_ok(&b, floor, &placed, None) {
                    self.status = why;
                    return;
                }
                self.status = format!("Placed {} on floor {}", self.prop_name(&prop), floor + 1);
                let d = self.draft.as_mut().unwrap();
                d.b.floors[floor].props.push(placed);
                d.sel = Some(Sel::Floor(d.b.floors[floor].props.len() - 1));
                self.need_buildings = true;
            }
            Clicks::Outside => {
                let Some(wall) = d.wall else {
                    // first, which wall
                    let ground = Self::plane(origin, dir, b.level as f32 * lh);
                    let edge = ground.and_then(|g| Self::nearest_edge(&b, g));
                    let d = self.draft.as_mut().unwrap();
                    d.hover_edge = edge;
                    if pressed {
                        if let Some(e) = edge {
                            d.wall = Some(e);
                            d.hover_edge = None;
                        }
                    }
                    return;
                };
                let run = run_of(&b, wall);
                let front = front_of(self, &b, &run, wall.2);
                let prop = d.outside_prop.clone();
                let on_walls = prop.as_ref().and_then(|p| self.content.props.get(p)).map(|p| p.on_walls()).unwrap_or(false);
                let face = self.pick_face(&b, origin, dir, false, None, Some((&run, wall.2)));
                let ground = Self::plane(origin, dir, b.level as f32 * lh);
                let d = self.draft.as_mut().unwrap();
                d.hover_face = if on_walls { face } else { None };
                d.hover_at = if on_walls { None } else { ground.map(|g| { let r = rel(g); [r.x as f64, r.y as f64] }) };
                if !pressed {
                    return;
                }
                if let Some(g) = ground {
                    let r = rel(g);
                    if let Some(i) = b.outside.iter().position(|q| (Vec2::new(q.at[0] as f32, q.at[1] as f32) - r).length() < 0.6) {
                        self.draft.as_mut().unwrap().sel = Some(Sel::Outside(i));
                        return;
                    }
                }
                if let Some((cell, side, f, h)) = face {
                    if let Some(i) = b.anchors.iter().position(|a| !a.inside && a.floor as usize == f && a.cell == cell && a.side == side && (a.height - h).abs() < 0.45) {
                        self.draft.as_mut().unwrap().sel = Some(Sel::Wall(i));
                        return;
                    }
                }
                let Some(prop) = prop else { return };
                if on_walls {
                    let Some((cell, side, f, height)) = face else {
                        self.status = "Hang it on the wall you picked".into();
                        return;
                    };
                    self.status = format!("Hung {} outside", self.prop_name(&prop));
                    let d = self.draft.as_mut().unwrap();
                    d.b.anchors.push(Anchor { prop, cell, side: side.into(), floor: f as u8, height, inside: false });
                    d.sel = Some(Sel::Wall(d.b.anchors.len() - 1));
                    self.need_buildings = true;
                    return;
                }
                let Some(g) = ground else { return };
                let turn = self.turn as f64;
                let at = self.snap(&prop, turn as f32, g);
                let placed = Placed { prop: prop.clone(), at, level: b.level, turn, scale: 1.0, ..Default::default() };
                let cells = cells_of(self, &placed);
                if cells.iter().any(|c| !front.contains(c)) {
                    self.status = format!("Outside, in front of the wall you picked (up to {FRONT} cells out)");
                    return;
                }
                let mut others = on_map(&b, &b.outside, b.level);
                others.extend(self.map.props.iter().filter(|p| p.level == b.level).cloned());
                for o in self.map.buildings.iter().filter(|o| o.id != b.id && o.level == b.level) {
                    others.extend(on_map(o, &o.outside, o.level));
                }
                if let Some(other) = clash(self, &prop, &cells, &others) {
                    self.status = format!("No room: {other} is there, and one of them doesn't stack");
                    return;
                }
                let r = [at[0] - b.at[0] as f64, at[1] - b.at[1] as f64];
                self.status = format!("Placed {} outside", self.prop_name(&prop));
                let d = self.draft.as_mut().unwrap();
                d.b.outside.push(Placed { at: r, level: 0, ..placed });
                d.sel = Some(Sel::Outside(d.b.outside.len() - 1));
                self.need_buildings = true;
            }
            _ => {}
        }
    }

    /// The cells an Inside/Outside placement would take now, and whether it may.
    fn hover_cells(&self) -> Option<(Vec<(i32, i32)>, bool)> {
        let d = self.draft.as_ref()?;
        let b = &d.b;
        let at = d.hover_at?;
        let (prop, outside) = match d.clicks {
            Clicks::Inside => (d.inside_prop.clone()?, false),
            Clicks::Outside if d.wall.is_some() => (d.outside_prop.clone()?, true),
            _ => return None,
        };
        let snapped = self.snap(&prop, self.turn, Vec3::new(at[0] as f32 + b.at[0] as f32, 0.0, at[1] as f32 + b.at[1] as f32));
        let placed = Placed { prop: prop.clone(), at: snapped, level: b.level, turn: self.turn as f64, scale: 1.0, ..Default::default() };
        let cells = cells_of(self, &placed);
        let ok = if outside {
            let wall = d.wall?;
            let front = front_of(self, b, &run_of(b, wall), wall.2);
            let mut others = on_map(b, &b.outside, b.level);
            others.extend(self.map.props.iter().filter(|p| p.level == b.level).cloned());
            for o in self.map.buildings.iter().filter(|o| o.id != b.id && o.level == b.level) {
                others.extend(on_map(o, &o.outside, o.level));
            }
            cells.iter().all(|c| front.contains(c)) && clash(self, &prop, &cells, &others).is_none()
        } else {
            let rel = Placed { at: [snapped[0] - b.at[0] as f64, snapped[1] - b.at[1] as f64], level: 0, ..placed.clone() };
            self.inside_ok(b, d.floor, &rel, None).is_ok()
        };
        Some((cells, ok))
    }

    /// A prop's name, or its id.
    fn prop_name(&self, id: &str) -> String {
        self.content.props.get(id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_string())
    }

    /// A stairs prop's footprint (nothing for other props).
    pub(super) fn stairs_footprint(&self, id: &str) -> Option<[u8; 2]> {
        self.content.props.get(id).filter(|p| p.category == "stairs").map(|p| p.footprint)
    }

    /// May this stand on `floor` of the building (`placed` from the building's `at`)? Inside
    /// the outline; over no stairwell (the floor is open there); overlapping nothing on the
    /// floor when either is unstackable (but `except`, the item being moved); and stairs
    /// only with the floor above clear over them, or a flat roof when there's no floor above.
    pub(super) fn inside_ok(&self, b: &Building, floor: usize, placed: &Placed, except: Option<usize>) -> Result<(), String> {
        let cells = cells_of(self, placed);
        if cells.iter().any(|&(x, z)| !b.has(x, z)) {
            return Err("Inside the building only: it would stick out".into());
        }
        let stairs = |id: &str| self.stairs_footprint(id);
        if cells.iter().any(|c| b.stairwells(floor, &stairs).contains(c)) {
            return Err("That's open: the stairs from below come up there".into());
        }
        let others: Vec<Placed> = b.floors[floor].props.iter().enumerate().filter(|(i, _)| Some(*i) != except).map(|(_, p)| p.clone()).collect();
        if let Some(other) = clash(self, &placed.prop, &cells, &others) {
            return Err(format!("No room: {other} is there, and one of them doesn't stack"));
        }
        if stairs(&placed.prop).is_some() {
            match b.floors.get(floor + 1) {
                Some(above) => {
                    let name = |id: &str| self.content.props.get(id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_string());
                    if let Some(p) = above.props.iter().find(|p| cells_of(self, p).iter().any(|c| cells.contains(c))) {
                        return Err(format!("Stairs need the floor above clear over them: {} is there", name(&p.prop)));
                    }
                }
                None if !b.roof_is_flat() => return Err("Stairs on the top floor need a flat roof (Structure: Flat roof), or a floor above".into()),
                None => {}
            }
        }
        Ok(())
    }

    /// Highlights, drawn over everything so floors and walls never hide them.
    pub(super) fn rooms_overlays(&self, out: &mut Vec<Shape>) {
        let lh = self.level_height;
        let Some(d) = &self.draft else { return };
        let b = &d.b;
        let rect = |x: f32, y: f32, z: f32, w: f32, dd: f32, color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: w, depth: dd }, center: Vec3::new(x, y, z), color, additive: false };
        let floor_y = (b.level as usize + d.floor) as f32 * lh + 0.1;
        let base_y = b.level as f32 * lh + 0.06;
        let face_mark = |hit: WallHit, color: [f32; 4], out: &mut Vec<Shape>| {
            let (cell, side, f, h) = hit;
            let (p, q) = segment_ends(cell[0], cell[1], side);
            let mid = Vec3::new(b.at[0] as f32 + (p[0] + q[0]) as f32 / 2.0, (b.level as usize + f) as f32 * lh + h as f32, b.at[1] as f32 + (p[1] + q[1]) as f32 / 2.0);
            let (dx, dz) = side_step(side).unwrap_or((0, 0));
            let (w, dd) = if matches!(side, "n" | "s") { (0.8, 0.2) } else { (0.2, 0.8) };
            out.push(rect(mid.x + dx as f32 * 0.12, mid.y, mid.z + dz as f32 * 0.12, w, dd, color));
        };
        match d.clicks {
            Clicks::Inside => {
                // open to the floor below: the stairs come up there
                let stairs = |id: &str| self.stairs_footprint(id);
                for (x, z) in b.stairwells(d.floor, &stairs) {
                    out.push(rect((b.at[0] + x) as f32 + 0.5, floor_y - 0.02, (b.at[1] + z) as f32 + 0.5, 0.9, 0.9, [0.05, 0.05, 0.1, 0.55]));
                }
                if let Some((cells, ok)) = self.hover_cells() {
                    let c = if ok { [0.4, 1.0, 0.5, 0.55] } else { [1.0, 0.3, 0.25, 0.6] };
                    for (x, z) in cells {
                        out.push(rect(x as f32 + 0.5, floor_y, z as f32 + 0.5, 0.94, 0.94, c));
                    }
                }
                if let Some(h) = d.hover_face {
                    face_mark(h, [1.0, 1.0, 1.0, 0.8], out);
                }
            }
            Clicks::Outside => match d.wall {
                None => {
                    if let Some((x, z, side)) = d.hover_edge {
                        for (rx, rz) in run_of(b, (x, z, side)) {
                            let (p, q) = segment_ends(rx, rz, side);
                            let (dx, dz) = side_step(side).unwrap_or((0, 0));
                            let (w, dd) = if matches!(side, "n" | "s") { (1.0, 0.3) } else { (0.3, 1.0) };
                            out.push(rect(b.at[0] as f32 + (p[0] + q[0]) as f32 / 2.0 + dx as f32 * 0.2, base_y, b.at[1] as f32 + (p[1] + q[1]) as f32 / 2.0 + dz as f32 * 0.2, w, dd, [1.0, 0.6, 0.15, 0.8]));
                        }
                    }
                }
                Some(wall) => {
                    let run = run_of(b, wall);
                    for (x, z) in front_of(self, b, &run, wall.2) {
                        out.push(rect(x as f32 + 0.5, base_y, z as f32 + 0.5, 0.96, 0.96, [1.0, 0.6, 0.15, 0.18]));
                    }
                    if let Some((cells, ok)) = self.hover_cells() {
                        let c = if ok { [0.4, 1.0, 0.5, 0.55] } else { [1.0, 0.3, 0.25, 0.6] };
                        for (x, z) in cells {
                            out.push(rect(x as f32 + 0.5, base_y + 0.02, z as f32 + 0.5, 0.94, 0.94, c));
                        }
                    }
                    if let Some(h) = d.hover_face {
                        face_mark(h, [1.0, 1.0, 1.0, 0.8], out);
                    }
                }
            },
            _ => {}
        }
        // what's selected
        match d.sel {
            Some(Sel::Floor(i)) => {
                if let Some(p) = b.floors[d.floor].props.get(i) {
                    let placed = Placed { at: [p.at[0] + b.at[0] as f64, p.at[1] + b.at[1] as f64], ..p.clone() };
                    for (x, z) in cells_of(self, &placed) {
                        out.push(rect(x as f32 + 0.5, floor_y + 0.01, z as f32 + 0.5, 0.98, 0.98, [1.0, 0.85, 0.2, 0.5]));
                    }
                }
            }
            Some(Sel::Outside(i)) => {
                if let Some(p) = b.outside.get(i) {
                    let placed = Placed { at: [p.at[0] + b.at[0] as f64, p.at[1] + b.at[1] as f64], level: b.level, ..p.clone() };
                    for (x, z) in cells_of(self, &placed) {
                        out.push(rect(x as f32 + 0.5, base_y + 0.03, z as f32 + 0.5, 0.98, 0.98, [1.0, 0.85, 0.2, 0.5]));
                    }
                }
            }
            Some(Sel::Wall(i)) => {
                if let Some(a) = b.anchors.get(i) {
                    let side: &'static str = content::SIDES.iter().find(|s| **s == a.side).copied().unwrap_or("s");
                    face_mark((a.cell, side, a.floor as usize, a.height), [1.0, 0.85, 0.2, 0.9], out);
                }
            }
            None => {}
        }
    }

    // ------------------------------------------------------------ the panel

    fn placed_list(&mut self, ui: &mut egui::Ui, inside: bool) {
        let Some(d) = &self.draft else { return };
        let b = d.b.clone();
        let floor = d.floor;
        let sel = d.sel;
        let wall = d.wall;
        let name = |id: &str| self.content.props.get(id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_string());
        let mut rows: Vec<(Sel, String)> = vec![];
        if inside {
            for (i, p) in b.floors[floor].props.iter().enumerate() {
                rows.push((Sel::Floor(i), format!("{} · on the floor", name(&p.prop))));
            }
            for (i, a) in b.anchors.iter().enumerate().filter(|(_, a)| a.inside && a.floor as usize == floor) {
                rows.push((Sel::Wall(i), format!("{} · on a wall, {:.2} up", name(&a.prop), a.height)));
            }
        } else {
            let run = wall.map(|w| run_of(&b, w)).unwrap_or_default();
            for (i, a) in b.anchors.iter().enumerate().filter(|(_, a)| !a.inside) {
                let here = wall.map(|w| a.side == w.2 && run.contains(&(a.cell[0], a.cell[1]))).unwrap_or(true);
                if here {
                    rows.push((Sel::Wall(i), format!("{} · on the wall, floor {}, {:.2} up", name(&a.prop), a.floor + 1, a.height)));
                }
            }
            for (i, p) in b.outside.iter().enumerate() {
                rows.push((Sel::Outside(i), format!("{} · on the ground", name(&p.prop))));
            }
        }
        section(ui, &if inside { format!("On floor {}", floor + 1) } else { "Outside".to_string() });
        if rows.is_empty() {
            hint(ui, "Nothing yet.");
        }
        let mut remove = None;
        let mut pick = None;
        for (k, (s, label)) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.selectable_label(sel == Some(*s), label).clicked() {
                    pick = Some(*s);
                }
                let r = ui.small_button("remove");
                if k == 0 {
                    super::mark("uiRemoveFirst", &r);
                }
                if r.clicked() {
                    remove = Some(*s);
                }
            });
        }
        if let Some(s) = pick {
            self.draft.as_mut().unwrap().sel = Some(s);
        }
        if let Some(s) = remove {
            self.remove_item(s);
        }
    }

    pub(super) fn remove_item(&mut self, s: Sel) {
        let Some(d) = &mut self.draft else { return };
        match s {
            Sel::Floor(i) if i < d.b.floors[d.floor].props.len() => {
                d.b.floors[d.floor].props.remove(i);
            }
            Sel::Wall(i) if i < d.b.anchors.len() => {
                d.b.anchors.remove(i);
            }
            Sel::Outside(i) if i < d.b.outside.len() => {
                d.b.outside.remove(i);
            }
            _ => return,
        }
        d.sel = None;
        self.need_buildings = true;
    }

    pub(super) fn inside_panel(&mut self, ui: &mut egui::Ui) {
        hint(ui, "Click the floor to place it, or an inside wall for things that hang. Click a thing to select it: Q/E turn it, Delete removes it.");
        self.prop_picker(ui, INSIDE, true);
        self.placed_list(ui, true);
    }

    pub(super) fn outside_panel(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &self.draft else { return };
        match d.wall {
            None => {
                warn(ui, "Click one of its walls first (a court's walls count): things go on it, or on the ground in front of it.");
            }
            Some(w) => {
                let n = run_of(&d.b, w).len();
                let facing = match w.2 { "n" => "north", "s" => "south", "e" => "east", _ => "west" };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("The {facing} wall · {n} cell{}", if n == 1 { "" } else { "s" })).strong());
                    let other = ui.small_button("Another wall");
                    super::mark("uiOtherWall", &other);
                    if other.clicked() {
                        let d = self.draft.as_mut().unwrap();
                        d.wall = None;
                        d.sel = None;
                    }
                });
                hint(ui, "Things hang on it, or stand on the ground up to two cells in front of it.");
                self.prop_picker(ui, OUTSIDE, false);
            }
        }
        self.placed_list(ui, false);
    }

    /// The props of a folder to pick from.
    fn prop_picker(&mut self, ui: &mut egui::Ui, folder: &str, inside: bool) {
        let props = self.folder_props(folder);
        section(ui, folder);
        if props.is_empty() {
            hint(ui, "Empty: put props in this folder from Assets.");
        }
        for (k, (id, name, fp, solid, walls)) in props.into_iter().enumerate() {
            let d = self.draft.as_mut().unwrap();
            let current = if inside { &mut d.inside_prop } else { &mut d.outside_prop };
            let mut tags = format!("{}×{}", fp[0], fp[1]);
            if walls {
                tags += " · on walls";
            }
            if solid {
                tags += " · unstackable";
            }
            let r = ui.selectable_label(current.as_deref() == Some(id.as_str()), format!("{name}   {tags}"));
            if k == 0 {
                super::mark(if inside { "uiInsideFirst" } else { "uiOutsideFirst" }, &r);
            }
            super::mark(&format!("uiPick_{id}"), &r);
            if r.clicked() {
                *current = Some(id);
            }
        }
    }

    /// Q/E turns the selected item (if it still fits turned), Delete removes it.
    pub(super) fn rooms_keys(&mut self, turn: f32, delete: bool) -> bool {
        let Some(s) = self.draft.as_ref().and_then(|d| d.sel) else { return false };
        if delete {
            self.remove_item(s);
            return true;
        }
        if turn == 0.0 {
            return false;
        }
        let d = self.draft.as_ref().unwrap();
        let b = &d.b;
        let (list, i, outside) = match s {
            Sel::Floor(i) => (&b.floors[d.floor].props, i, false),
            Sel::Outside(i) => (&b.outside, i, true),
            Sel::Wall(_) => return false,
        };
        let Some(p) = list.get(i) else { return false };
        let turned = Placed { turn: ((p.turn as f32 + turn).rem_euclid(360.0)) as f64, ..p.clone() };
        if !outside {
            if let Err(why) = self.inside_ok(b, d.floor, &turned, Some(i)) {
                self.status = format!("No room to turn it: {why}");
                return true;
            }
        } else {
            let level = b.level;
            let cells = cells_of(self, &on_map(b, std::slice::from_ref(&turned), level)[0]);
            let mut others = on_map(b, list, level);
            others.remove(i);
            for o in self.map.buildings.iter().filter(|o| o.id != b.id && o.level == level) {
                others.extend(on_map(o, &o.outside, o.level));
            }
            others.extend(self.map.props.iter().filter(|q| q.level == level).cloned());
            let taken: Vec<(i32, i32)> = self.map.buildings.iter().filter(|o| o.id != b.id).flat_map(|o| o.map_cells()).collect();
            if !cells.iter().all(|&(x, z)| self.map.inside(x, z) && !b.has(x - b.at[0], z - b.at[1]) && !taken.contains(&(x, z))) {
                self.status = "No room to turn it there".into();
                return true;
            }
            if let Some(other) = clash(self, &turned.prop, &cells, &others) {
                self.status = format!("No room to turn it: {other} is in the way, and one of them doesn't stack");
                return true;
            }
        }
        let d = self.draft.as_mut().unwrap();
        let list = if outside { &mut d.b.outside } else { &mut d.b.floors[d.floor].props };
        list[i] = turned;
        self.need_buildings = true;
        true
    }

    /// The cells the edited building and what's in front of the picked wall cover: the
    /// part of the map that stays lit while it's edited.
    pub(super) fn focus_cells(&self) -> Option<Vec<(i32, i32)>> {
        let Some(d) = self.draft.as_ref().filter(|d| d.step == super::build::Step::Design) else {
            // the walker inside a building (or on the threshold, fading): the building
            let i = self.fade.as_ref().map(|f| f.building).or_else(|| self.walker.as_ref().and_then(|w| w.inside).map(|(i, _)| i))?;
            return self.map.buildings.get(i).map(|b| b.map_cells());
        };
        let mut cells = d.b.map_cells();
        if let (Clicks::Outside, Some(w)) = (d.clicks, d.wall) {
            cells.extend(front_of(self, &d.b, &run_of(&d.b, w), w.2));
        }
        Some(cells)
    }

    /// The box around them (x0, z0, x1, z1): what stands outside it is darkened too.
    pub(super) fn focus_box(&self) -> Option<[f32; 4]> {
        let cells = self.focus_cells()?;
        let (mut x0, mut z0, mut x1, mut z1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (x, z) in cells {
            (x0, z0, x1, z1) = (x0.min(x), z0.min(z), x1.max(x + 1), z1.max(z + 1));
        }
        Some([x0 as f32 - 0.05, z0 as f32 - 0.05, x1 as f32 + 0.05, z1 as f32 + 0.05])
    }
}
