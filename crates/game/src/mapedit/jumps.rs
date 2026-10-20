//! The Ground tool's jumps: a click on a cell, then one on another two cells from it in a line
//! (one cell between them) on its level, makes a jump between them (`content::Jump`): the
//! player jumps from one to the other over the cell between, both ways unless it's made one
//! way. A click on a jump's end picks it, to make it one way (or turn it round) or take it away
//! (Delete). Its ends keep their level while it's there: the brush paints their ground, never
//! raises or sinks them.

use content::Jump;
use egui::RichText;
use glam::Vec3;
use render::scene::{Shape, ShapeKind};

use super::{hint, mark, section, MapEditor};

/// The Ground tool's jumps, while it makes them (else it paints).
#[derive(Default)]
pub(super) struct JumpTool {
    /// the Ground tool makes jumps, not paint
    pub on: bool,
    /// the end clicked first, its other end to come
    pub first: Option<(i32, i32)>,
    /// the jump picked (its index in the map's)
    pub selected: Option<usize>,
}

/// The jumps' marks (the one picked brighter).
const JUMP: [f32; 3] = [1.0, 0.72, 0.28];

/// What can't go where the pointer is: its marks' colour.
const REFUSED: [f32; 4] = [1.0, 0.35, 0.3, 0.45];

/// A jump in words: from one end to the other, and back unless it's one way.
fn about(j: &Jump) -> String {
    let way = if j.one_way { "one way" } else { "and back" };
    format!("from {}, {} to {}, {} ({way})", j.from[0], j.from[1], j.to[0], j.to[1])
}

impl MapEditor {
    /// The Ground panel while it makes jumps: what to do, and the jump picked.
    pub(super) fn jump_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Jumps");
        hint(ui, "Click a cell, then another two cells from it in a line (one cell between them) on the same level: the player jumps from one to the other over the cell between, both ways. Click a jump's end to pick it; Delete takes it away. Its ends keep their level (their ground still paints).");
        if let Some(a) = self.jump.first {
            hint(ui, &format!("From {}, {}: now its other end (Escape: never mind).", a.0, a.1));
        }
        let Some(i) = self.jump.selected.filter(|&i| i < self.map.jumps.len()) else {
            let n = self.map.jumps.len();
            hint(ui, &format!("{n} jump{} on this map.", if n == 1 { "" } else { "s" }));
            return;
        };
        let j = self.map.jumps[i].clone();
        section(ui, "This jump");
        ui.label(RichText::new(format!("From {}, {} to {}, {}", j.from[0], j.from[1], j.to[0], j.to[1])).strong());
        hint(ui, if j.one_way { "Jumped from its first end only, never back" } else { "Jumped both ways" });
        let mut one_way = j.one_way;
        let r = ui.checkbox(&mut one_way, "One way").on_hover_text("Jumped from its first end to the other only (its little ramp at that end alone)");
        mark("uiJumpOneWay", &r);
        if one_way != j.one_way {
            self.checkpoint();
            self.map.jumps[i].one_way = one_way;
            self.jump_changed();
        }
        ui.horizontal(|ui| {
            if j.one_way {
                let round = ui.button("Turn round").on_hover_text("Jumped the other way");
                mark("uiJumpTurn", &round);
                if round.clicked() {
                    self.checkpoint();
                    let j = &mut self.map.jumps[i];
                    (j.from, j.to) = (j.to, j.from);
                    self.jump_changed();
                }
            }
            let gone = ui.button("Remove").on_hover_text("No jump there (Delete)");
            mark("uiJumpRemove", &gone);
            if gone.clicked() {
                self.remove_jump(i);
            }
        });
    }

    /// The pointer on the map while the Ground tool makes jumps: a press after a first end on
    /// a cell two from it in a line makes the jump; anywhere else it's a first end (and picks
    /// the jump there, if one's there); on the first end again, it lets go of it.
    pub(super) fn jump_input(&mut self, hovered: bool, pressed: bool) {
        if !(pressed && hovered) {
            return;
        }
        let Some(c) = self.ground_hit.filter(|c| self.map.inside(c.0, c.1)) else { return };
        if let Some(a) = self.jump.first {
            if a == c {
                self.jump.first = None;
                self.status = "No jump from there after all".into();
                return;
            }
            if Jump::spans([a.0, a.1], [c.0, c.1]) {
                match self.make_jump(a, c) {
                    Ok(i) => {
                        self.status = format!("A jump {}", about(&self.map.jumps[i]));
                        self.jump.selected = Some(i);
                        self.jump.first = None;
                    }
                    Err(why) => self.status = why,
                }
                return;
            }
        }
        // a first end (the jump there picked, if one's there)
        self.jump.selected = self.map.jumps.iter().position(|j| j.has([c.0, c.1]));
        match self.jump_end_refused(c) {
            None => {
                self.jump.first = Some(c);
                self.status = match self.jump.selected {
                    Some(i) => format!("The jump {}; or click two cells away in a line for another from {}, {}", about(&self.map.jumps[i]), c.0, c.1),
                    None => format!("A jump from {}, {}: click its other end, two cells away in a line", c.0, c.1),
                };
            }
            Some(why) => {
                self.jump.first = None;
                self.status = why;
            }
        }
    }

    /// Why a jump can't have an end on a cell (None when it can): on a ramp, under a building,
    /// on water.
    fn jump_end_refused(&self, (x, z): (i32, i32)) -> Option<String> {
        if !self.map.inside(x, z) {
            return Some("No jump there: it's off the map".into());
        }
        if self.map.ramp_at(x, z).is_some() {
            return Some(format!("No jump there: {x}, {z} is on a ramp"));
        }
        if self.map.buildings.iter().any(|b| b.covers((x, z))) {
            return Some(format!("No jump there: a building stands on {x}, {z}"));
        }
        if self.map.terrain_at(x, z).and_then(|t| self.content.terrains.get(t)).is_some_and(|t| t.walk == "water") {
            return Some(format!("No jump there: {x}, {z} is water"));
        }
        None
    }

    /// Why there can't be a jump from `a` to `b` (None when there can): its ends two cells
    /// apart in a line, each where an end can be, on one level, the two not joined already.
    fn jump_refused(&self, a: (i32, i32), b: (i32, i32)) -> Option<String> {
        if !Jump::spans([a.0, a.1], [b.0, b.1]) {
            return Some("A jump's ends are two cells apart in a line: one cell between them, along x or z".into());
        }
        if let Some(why) = self.jump_end_refused(a).or_else(|| self.jump_end_refused(b)) {
            return Some(why);
        }
        let (la, lb) = (self.map.level_at(a.0, a.1).unwrap_or(0), self.map.level_at(b.0, b.1).unwrap_or(0));
        if la != lb {
            return Some(format!("No jump there: its ends are on levels {la} and {lb} (a jump's are on one level)"));
        }
        if self.map.jumps.iter().any(|j| j.joins([a.0, a.1], [b.0, b.1])) {
            return Some("There's a jump there already".into());
        }
        None
    }

    /// A jump from `a` to `b`, both ways; why not, if it can't be there.
    pub(super) fn make_jump(&mut self, a: (i32, i32), b: (i32, i32)) -> Result<usize, String> {
        if let Some(why) = self.jump_refused(a, b) {
            return Err(why);
        }
        self.checkpoint();
        self.map.jumps.push(Jump { from: [a.0, a.1], to: [b.0, b.1], one_way: false });
        self.jump_changed();
        Ok(self.map.jumps.len() - 1)
    }

    /// Takes a jump away (its ends' level free to change again).
    pub(super) fn remove_jump(&mut self, i: usize) {
        if i >= self.map.jumps.len() {
            return;
        }
        self.checkpoint();
        let j = self.map.jumps.remove(i);
        self.jump.selected = None;
        self.jump_changed();
        self.status = format!("The jump from {}, {} to {}, {} is gone", j.from[0], j.from[1], j.to[0], j.to[1]);
    }

    /// The jumps with an end on cells a building now stands on go (it's no place to jump to or
    /// from); how many went.
    pub(super) fn jumps_under(&mut self, cells: &[(i32, i32)]) -> usize {
        let before = self.map.jumps.len();
        self.map.jumps.retain(|j| ![j.from, j.to].iter().any(|c| cells.contains(&(c[0], c[1]))));
        let gone = before - self.map.jumps.len();
        if gone > 0 {
            self.jump.selected = None;
            self.jump_changed();
        }
        gone
    }

    /// After a jump changed: its ends' little ramps built again.
    fn jump_changed(&mut self) {
        self.dirty = true;
        self.need_ground = true;
    }

    /// The Ground tool's jumps on the map: each one's ends and the way it goes (an arrowhead
    /// at each end it lands on), the one picked brighter; the first end clicked, and where its
    /// other end could go (crimson where it can't); the cell under the pointer.
    pub(super) fn jump_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        let centre = |c: (i32, i32), up: f32| Vec3::new(c.0 as f32 + 0.5, self.map.level_at(c.0, c.1).unwrap_or(0) as f32 * lh + up, c.1 as f32 + 0.5);
        let cell = |c: (i32, i32), color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: 0.94, depth: 0.94 }, center: centre(c, 0.04), color, additive: false };
        let quad = |corners: [Vec3; 4], color: [f32; 4]| Shape { kind: ShapeKind::Quad { corners }, center: Vec3::ZERO, color, additive: false };
        for (i, j) in self.map.jumps.iter().enumerate() {
            let picked = Some(i) == self.jump.selected;
            let color = |a: f32| [JUMP[0], JUMP[1], JUMP[2], if picked { a * 2.5 } else { a }];
            let (f, t) = ((j.from[0], j.from[1]), (j.to[0], j.to[1]));
            shapes.push(cell(f, color(0.16)));
            shapes.push(cell(t, color(0.16)));
            // a line from one end's middle to the other's, over the cell between
            let (a, b) = (centre(f, 0.07), centre(t, 0.07));
            let d = (b - a).normalize_or_zero();
            let s = Vec3::new(-d.z, 0.0, d.x);
            shapes.push(quad([a - s * 0.05, a + s * 0.05, b + s * 0.05, b - s * 0.05], color(0.3)));
            // an arrowhead into each end it lands on (one way: its other end only)
            for (land, way) in [(t, d), (f, -d)] {
                if land == f && j.one_way {
                    continue;
                }
                let tip = centre(land, 0.08) - way * 0.12;
                let back = tip - way * 0.3;
                shapes.push(quad([tip, back + s * 0.2, back - s * 0.2, back - s * 0.2], color(0.36)));
            }
        }
        if let Some(a) = self.jump.first {
            shapes.push(cell(a, [1.0, 1.0, 1.0, 0.45]));
            for (dx, dz) in [(2, 0), (-2, 0), (0, 2), (0, -2)] {
                let c = (a.0 + dx, a.1 + dz);
                if self.map.inside(c.0, c.1) {
                    shapes.push(cell(c, if self.jump_refused(a, c).is_none() { [1.0, 1.0, 1.0, 0.22] } else { [REFUSED[0], REFUSED[1], REFUSED[2], 0.25] }));
                }
            }
        }
        if let Some(c) = self.ground_hit.filter(|c| self.map.inside(c.0, c.1)) {
            shapes.push(cell(c, [1.0, 1.0, 1.0, 0.18]));
        }
    }

    /// The jumps' marks for the checks: how many; the one picked (from > to; both or one
    /// way); the first end clicked.
    pub(super) fn jump_hooks(&self) {
        crate::platform::set_hook("jumps", &self.map.jumps.len().to_string());
        let picked = self.jump.selected.and_then(|i| self.map.jumps.get(i)).map(|j| format!("{},{}>{},{};{}", j.from[0], j.from[1], j.to[0], j.to[1], if j.one_way { "one" } else { "both" }));
        crate::platform::set_hook("jump", &picked.unwrap_or_default());
        crate::platform::set_hook("jumpfirst", &self.jump.first.map(|a| format!("{},{}", a.0, a.1)).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::walk::{JUMP_HIGH, JUMP_MS};
    use super::*;

    /// An editor on a field 12 x 10 on level 1, seen from the south: a ditch a level down along
    /// x 5 (what jumps go over), a pond at 8, 2, a rise a level up at 6, 6, a hut at 2, 8, a
    /// ramp down to the ditch at 4, 9.
    fn editor() -> MapEditor {
        let mut c = content::Content::default();
        for (id, walk) in [("grass", "walk"), ("dirt", "walk"), ("pond", "water")] {
            c.terrains.put(content::Terrain { id: id.into(), name: id.into(), walk: walk.into(), ..Default::default() }, content::Space::Content);
        }
        let root = std::env::temp_dir().join("battle-frontier-jumps");
        let mut ed = MapEditor::new(Arc::new(c), None, crate::platform::Files::new(root.to_str().unwrap()));
        let mut map = world::WorldMap::new("field", "Field", 12, 10, "grass");
        map.level.iter_mut().for_each(|l| *l = 1);
        for z in 0..10 {
            map.level[z * 12 + 5] = 0;
        }
        map.level[6 * 12 + 6] = 2;
        let pond = map.terrain_index("pond");
        map.terrain[2 * 12 + 8] = pond;
        map.buildings.push(content::Building { id: "hut".into(), name: "Hut".into(), at: [2, 8], level: 1, cells: vec!["#".into()], floors: vec![content::Floor::default()], ..Default::default() });
        map.ramps.push(content::Ramp { at: [4, 9], size: [1, 1], up: "w".into(), ..Default::default() });
        ed.map = map;
        ed.cam.yaw = 0.0;
        ed
    }

    /// A click on the map, on a cell.
    fn click(ed: &mut MapEditor, c: (i32, i32)) {
        ed.ground_hit = Some(c);
        ed.jump_input(true, true);
    }

    #[test]
    fn two_clicks_two_cells_apart_make_a_jump_both_ways() {
        let mut ed = editor();
        ed.jump.on = true;
        click(&mut ed, (4, 3));
        assert_eq!((ed.jump.first, ed.status.as_str()), (Some((4, 3)), "A jump from 4, 3: click its other end, two cells away in a line"));
        // a cell three away: a first end again, there; clicked again, let go of
        click(&mut ed, (7, 3));
        assert_eq!(ed.jump.first, Some((7, 3)));
        click(&mut ed, (7, 3));
        assert_eq!(ed.jump.first, None);
        click(&mut ed, (4, 3));
        click(&mut ed, (6, 3));
        assert_eq!(ed.map.jumps, [Jump { from: [4, 3], to: [6, 3], one_way: false }]);
        assert_eq!((ed.jump.first, ed.jump.selected, ed.dirty), (None, Some(0), true));
        assert_eq!(ed.status, "A jump from 4, 3 to 6, 3 (and back)");
        // its end clicked: picked (and a first end, for another from there)
        ed.jump.selected = None;
        click(&mut ed, (6, 3));
        assert_eq!((ed.jump.selected, ed.jump.first), (Some(0), Some((6, 3))));
        ed.jump_hooks();
        assert_eq!(crate::platform::hook("jumps").as_deref(), Some("1"));
        assert_eq!(crate::platform::hook("jump").as_deref(), Some("4,3>6,3;both"));
        assert_eq!(crate::platform::hook("jumpfirst").as_deref(), Some("6,3"));
        // undone: gone; made again, then taken away
        ed.undo();
        assert!(ed.map.jumps.is_empty());
        let i = ed.make_jump((4, 3), (6, 3)).unwrap();
        ed.remove_jump(i);
        assert!(ed.map.jumps.is_empty() && ed.status == "The jump from 4, 3 to 6, 3 is gone", "{}", ed.status);
    }

    #[test]
    fn a_jump_goes_only_between_two_cells_it_can_join() {
        let mut ed = editor();
        ed.make_jump((4, 3), (6, 3)).unwrap();
        let refused = |ed: &mut MapEditor, a: (i32, i32), b: (i32, i32)| ed.make_jump(a, b).unwrap_err();
        assert!(refused(&mut ed, (4, 4), (7, 4)).contains("two cells apart in a line"));
        assert!(refused(&mut ed, (4, 4), (6, 6)).contains("two cells apart in a line"));
        assert_eq!(refused(&mut ed, (4, 6), (6, 6)), "No jump there: its ends are on levels 1 and 2 (a jump's are on one level)");
        assert_eq!(refused(&mut ed, (6, 2), (8, 2)), "No jump there: 8, 2 is water");
        assert_eq!(refused(&mut ed, (2, 8), (4, 8)), "No jump there: a building stands on 2, 8");
        assert_eq!(refused(&mut ed, (4, 9), (6, 9)), "No jump there: 4, 9 is on a ramp");
        assert_eq!(refused(&mut ed, (6, 3), (4, 3)), "There's a jump there already");
        assert_eq!(refused(&mut ed, (11, 3), (13, 3)), "No jump there: it's off the map");
        assert_eq!(ed.map.jumps.len(), 1);
        // the second click on a cell it can't join: said why, the first end kept
        ed.jump.on = true;
        click(&mut ed, (6, 2));
        click(&mut ed, (8, 2));
        assert_eq!((ed.jump.first, ed.status.as_str()), (Some((6, 2)), "No jump there: 8, 2 is water"));
        // where its other end could go: white, crimson where it can't
        let mut shapes = vec![];
        ed.jump_shapes(&mut shapes);
        let at = |x: f32, z: f32| shapes.iter().filter(|s| s.center.x == x && s.center.z == z && matches!(s.kind, ShapeKind::Rect { .. })).map(|s| s.color).collect::<Vec<_>>();
        assert!(at(8.5, 2.5).iter().any(|c| c[1] < 0.5), "the pond: crimson");
        assert!(at(4.5, 2.5).contains(&[1.0, 1.0, 1.0, 0.22]), "over the ditch: white");
    }

    #[test]
    fn a_jump_s_ends_keep_their_level_their_ground_still_paints() {
        let mut ed = editor();
        ed.make_jump((4, 3), (6, 3)).unwrap();
        (ed.set_level, ed.level, ed.brush, ed.terrain) = (true, 0, 1, "dirt".into());
        ed.paint_ground((4, 3));
        let dirt = ed.map.terrain_index("dirt");
        let at = |ed: &MapEditor, x: i32, z: i32| (ed.map.level_at(x, z).unwrap(), ed.map.terrain[ed.map.index(x, z).unwrap()] == dirt);
        assert_eq!(at(&ed, 4, 3), (1, true), "its level kept, its ground painted");
        assert_eq!((at(&ed, 3, 3), at(&ed, 4, 2)), ((0, true), (0, true)), "round it, both");
        assert!(ed.status.starts_with("A jump's ends keep their level"), "{}", ed.status);
        // the jump taken away: its end's level free again
        ed.remove_jump(0);
        ed.paint_ground((4, 3));
        assert_eq!(at(&ed, 4, 3), (0, true));
    }

    #[test]
    fn a_building_put_on_a_jump_s_end_takes_the_jump_away() {
        let mut ed = editor();
        ed.make_jump((7, 3), (9, 3)).unwrap();
        ed.make_jump((7, 5), (9, 5)).unwrap();
        ed.map.buildings.push(content::Building { id: "shed".into(), name: "Shed".into(), at: [8, 2], level: 1, cells: vec!["##".into(), "##".into()], floors: vec![content::Floor::default()], ..Default::default() });
        ed.settle(1);
        assert_eq!(ed.map.jumps, [Jump { from: [7, 5], to: [9, 5], one_way: false }]);
        assert_eq!(ed.status, "Shed: the jump with an end under it is gone");
    }

    /// One frame of walking with `key` pressed (let go of the frame after).
    fn walk_pressing(ed: &mut MapEditor, ctx: &egui::Context, key: egui::Key, now: f64) {
        for pressed in [true, false] {
            let events = vec![egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }];
            let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                if pressed {
                    ed.walk(ui.ctx(), now);
                }
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn the_player_jumps_over_the_cell_between_and_back_unless_it_s_one_way() {
        let mut ed = editor();
        ed.make_jump((4, 3), (6, 3)).unwrap();
        ed.walk_to(4, 3);
        let ctx = egui::Context::default();
        let lh = ed.level_height;
        let cell = |ed: &MapEditor| ed.walker.as_ref().unwrap().cell;
        // east (the right arrow, seen from the south): over the ditch onto its other end, in an
        // arc half a cell high or more, landing on its middle
        walk_pressing(&mut ed, &ctx, egui::Key::ArrowRight, 1000.0);
        assert_eq!(cell(&ed), (6, 3));
        let w = ed.walker.as_ref().unwrap();
        let top = w.pos(1000.0 + JUMP_MS / 2.0);
        assert!(top.y >= lh + JUMP_HIGH - 1e-4 && (top.x - 5.5).abs() < 1e-4, "{top}");
        assert!((w.pos(1000.0 + JUMP_MS) - ed.cell_pos((6, 3))).length() < 1e-4);
        // and back
        walk_pressing(&mut ed, &ctx, egui::Key::ArrowLeft, 2000.0);
        assert_eq!(cell(&ed), (4, 3));
        // one way: there, never back (the ditch a level down: no step either)
        ed.map.jumps[0].one_way = true;
        walk_pressing(&mut ed, &ctx, egui::Key::ArrowRight, 3000.0);
        assert_eq!(cell(&ed), (6, 3));
        walk_pressing(&mut ed, &ctx, egui::Key::ArrowLeft, 4000.0);
        assert_eq!(cell(&ed), (6, 3));
        // on along the ground from its end: a step, no jump
        walk_pressing(&mut ed, &ctx, egui::Key::ArrowRight, 5000.0);
        assert_eq!(cell(&ed), (7, 3));
    }
}
