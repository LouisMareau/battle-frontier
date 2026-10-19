//! The Ground tool's ramps: a drag along the top of a step in the ground (the cells a level up,
//! where it steps down one level) makes them a ramp down to the ground below, one cell deep
//! (`content::Ramp`, its ground shaped by `world::ramps`); a click makes one of one cell. A
//! click on a ramp picks it, to make it more or less uneven, give it another look, or take it
//! away (Delete). And what lies along ramps (stairs, a mud slide: `content::RampFit`): the
//! Place tool lays it along the ramp under the pointer, Select drags it from one to another,
//! and it goes with its ramp.

use content::{Placed, Prop, Ramp};
use egui::RichText;
use glam::Vec3;
use render::scene::{Shape, ShapeKind};

use super::{hint, mark, rooms, section, MapEditor};

/// The Ground tool's ramps, while it makes them (else it paints).
#[derive(Default)]
pub(super) struct RampTool {
    /// the Ground tool makes ramps, not paint
    pub on: bool,
    /// a drag's first cell, and the last it was over
    pub drag: Option<((i32, i32), (i32, i32))>,
    /// the ramp picked (its index in the map's)
    pub selected: Option<usize>,
}

/// What can't go where the pointer is: its marks' colour.
const REFUSED: [f32; 4] = [1.0, 0.35, 0.3, 0.45];

/// The way a ramp's foot looks, in words (it rises the other way).
fn foot_name(up: &str) -> &'static str {
    match up {
        "n" => "south",
        "e" => "west",
        "s" => "north",
        _ => "east",
    }
}

/// A new ramp's seed, from where it is (the same drag, the same ramp).
fn seed_of(at: [i32; 2], size: [u16; 2]) -> u32 {
    (at[0] as u32).wrapping_mul(73_856_093) ^ (at[1] as u32).wrapping_mul(19_349_663) ^ (size[0] as u32 * 31 + size[1] as u32).wrapping_mul(83_492_791)
}

impl MapEditor {
    /// The Ground panel while it makes ramps: what to do, and the ramp picked.
    pub(super) fn ramp_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Ramps");
        hint(ui, "Drag along the top of a step in the ground (the cells a level up, where it steps down one level; never two): they become a ramp down to the ground below, one cell deep. A click makes one of a cell, down the side you look at where it could go two ways. Click a ramp to pick it; Delete takes it away.");
        let Some(i) = self.ramp.selected.filter(|&i| i < self.map.ramps.len()) else {
            let n = self.map.ramps.len();
            hint(ui, &format!("{n} ramp{} on this map.", if n == 1 { "" } else { "s" }));
            return;
        };
        let r = self.map.ramps[i].clone();
        let top = self.map.level_at(r.at[0], r.at[1]).unwrap_or(0);
        section(ui, "This ramp");
        ui.label(RichText::new(format!("{} cell{} long", r.length(), if r.length() == 1 { "" } else { "s" })).strong());
        hint(ui, &format!("Down from level {top} to {}, its foot to the {}", top.saturating_sub(1), foot_name(&r.up)));
        let mut rough = r.rough;
        let s = ui.add(egui::Slider::new(&mut rough, 0.0..=1.0).text("uneven")).on_hover_text("How bumpy its ground is, and how its slope changes along it: 0, smooth and even");
        mark("uiRampRough", &s);
        if s.drag_started() || s.changed() && !s.dragged() {
            self.checkpoint();
        }
        if rough != r.rough {
            self.map.ramps[i].rough = rough;
            self.ramp_changed();
        }
        ui.horizontal(|ui| {
            let again = ui.button("Another look").on_hover_text("The same ramp, uneven another way");
            mark("uiRampReseed", &again);
            if again.clicked() {
                self.checkpoint();
                let seed = &mut self.map.ramps[i].seed;
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                self.ramp_changed();
            }
            let gone = ui.button("Remove").on_hover_text("Back to a cliff (Delete)");
            mark("uiRampRemove", &gone);
            if gone.clicked() {
                self.remove_ramp(i);
            }
        });
    }

    /// The pointer on the map while the Ground tool makes ramps: a press on a ramp picks it,
    /// elsewhere starts a drag; letting go makes the ramp the drag went along.
    pub(super) fn ramp_input(&mut self, hovered: bool, pressed: bool, released: bool) {
        if pressed && hovered {
            if let Some((x, z)) = self.ground_hit {
                match self.map.ramps.iter().position(|r| r.has(x, z)) {
                    Some(i) => {
                        self.ramp.selected = Some(i);
                        self.ramp.drag = None;
                    }
                    None if self.map.inside(x, z) => {
                        self.ramp.drag = Some(((x, z), (x, z)));
                        self.ramp.selected = None;
                    }
                    None => {}
                }
            }
        }
        if let (Some((a, _)), Some(c)) = (self.ramp.drag, self.ground_hit.filter(|c| self.map.inside(c.0, c.1))) {
            self.ramp.drag = Some((a, c));
        }
        if released {
            if let Some((a, b)) = self.ramp.drag.take() {
                match self.make_ramp(a, b) {
                    Ok(i) => {
                        let r = &self.map.ramps[i];
                        self.status = format!("A ramp {} cell{} long, its foot to the {}", r.length(), if r.length() == 1 { "" } else { "s" }, foot_name(&r.up));
                        self.ramp.selected = Some(i);
                    }
                    Err(why) => self.status = why,
                }
            }
        }
    }

    /// A ramp on the cells from `a` to `b` (a drag's ends; a click's one cell): a row along the
    /// top of a step, down to the side that steps down a level (the side the camera looks from,
    /// where two do); a row that runs on past the step's top (cells lower than the rest at its
    /// ends) is cut back to it. Why not, if it can't be there.
    pub(super) fn make_ramp(&mut self, a: (i32, i32), b: (i32, i32)) -> Result<usize, String> {
        let (mut x0, mut x1) = (a.0.min(b.0), a.0.max(b.0));
        let (mut z0, mut z1) = (a.1.min(b.1), a.1.max(b.1));
        if x1 > x0 && z1 > z0 {
            return Err("A ramp is one cell deep: drag along the top of a step, one row of cells".into());
        }
        let map = &self.map;
        let level = |x: i32, z: i32| map.level_at(x, z);
        let top = (z0..=z1).flat_map(|z| (x0..=x1).map(move |x| (x, z))).filter_map(|(x, z)| level(x, z)).max().ok_or("The ramp goes off the map")?;
        let low = |x: i32, z: i32| level(x, z).is_some_and(|l| l < top);
        while x0 < x1 && low(x0, z0) {
            x0 += 1;
        }
        while x1 > x0 && low(x1, z0) {
            x1 -= 1;
        }
        while z0 < z1 && low(x0, z0) {
            z0 += 1;
        }
        while z1 > z0 && low(x0, z1) {
            z1 -= 1;
        }
        let (at, size) = ([x0, z0], [(x1 - x0 + 1) as u16, (z1 - z0 + 1) as u16]);
        let yaw = self.cam.yaw.to_radians();
        let up = Ramp::rising(at, size, (yaw.sin(), yaw.cos()), &level)?;
        let r = Ramp { at, size, up, seed: seed_of(at, size), ..Default::default() };
        for (x, z) in r.cells() {
            if map.buildings.iter().any(|b| b.covers((x, z))) {
                return Err("No ramp there: a building stands on it".into());
            }
            if map.ramp_at(x, z).is_some() {
                return Err("No ramp there: there's one already".into());
            }
            if map.terrain_at(x, z).and_then(|t| self.content.terrains.get(t)).is_some_and(|t| t.walk == "water") {
                return Err("No ramp there: it's water".into());
            }
        }
        self.checkpoint();
        self.map.ramps.push(r);
        self.ramp_changed();
        Ok(self.map.ramps.len() - 1)
    }

    /// Takes a ramp away: a cliff again (what lay along it goes with it).
    pub(super) fn remove_ramp(&mut self, i: usize) {
        if i >= self.map.ramps.len() {
            return;
        }
        self.checkpoint();
        let gone = self.take_ramp(i);
        self.ramp.selected = None;
        self.ramp_changed();
        self.status = format!("The ramp is gone{}: a cliff again", if gone > 0 { ", and what lay along it" } else { "" });
    }

    /// Takes ramp `i` off the map with what lies along it; how many of those went.
    pub(super) fn take_ramp(&mut self, i: usize) -> usize {
        let r = self.map.ramps.remove(i);
        let content = &self.content;
        let before = self.map.props.len();
        self.map.props.retain(|p| !(r.has(p.at[0].floor() as i32, p.at[1].floor() as i32) && content.props.get(&p.prop).is_some_and(|q| q.ramp.is_some())));
        let gone = before - self.map.props.len();
        if gone > 0 {
            self.selected = None;
            self.need_props = true;
        }
        gone
    }

    /// The ramp (its index) placed prop `i` lies along, if it does.
    pub(super) fn along(&self, i: usize) -> Option<usize> {
        let p = self.map.props.get(i)?;
        let r = self.map.along_ramp(p, self.content.props.get(&p.prop)?)?;
        self.map.ramps.iter().position(|o| std::ptr::eq(o, r))
    }

    /// The ramp what lies along ramps goes on, for the pointer: the one under it, else one
    /// next to the cell it's on (at its foot, past its top or its ends).
    pub(super) fn ramp_under_pointer(&self) -> Option<usize> {
        let (x, z) = self.ground_hit?;
        let on = |x: i32, z: i32| self.map.ramps.iter().position(|r| r.has(x, z));
        on(x, z).or_else(|| [(0, 1), (0, -1), (1, 0), (-1, 0)].into_iter().find_map(|(dx, dz)| on(x + dx, z + dz)))
    }

    /// `id` laid along ramp `i`: on its cells' centre, on their level, turned up its way.
    fn laid_on(&self, id: &str, i: usize) -> Placed {
        let r = &self.map.ramps[i];
        let level = self.map.level_at(r.at[0], r.at[1]).unwrap_or(0);
        let at = [r.at[0] as f64 + r.size[0] as f64 / 2.0, r.at[1] as f64 + r.size[1] as f64 / 2.0];
        Placed { prop: id.into(), at, level, turn: world::ramps::turn_of(&r.up) as f64, scale: 1.0, offset: [0.0, 0.0] }
    }

    /// Why `prop` (made to lie along ramps) can't go along ramp `i` (`except`: a placed prop
    /// that doesn't count, the one moved); None when it can.
    fn along_refused(&self, prop: &Prop, i: usize, except: Option<usize>) -> Option<String> {
        let r = &self.map.ramps[i];
        let level = self.map.level_at(r.at[0], r.at[1])?;
        let name = |id: &str| self.content.props.get(id).map(|q| q.name.clone()).unwrap_or_else(|| id.to_string());
        // one to a ramp
        if let Some(other) = (0..self.map.props.len()).filter(|&k| Some(k) != except).find(|&k| self.along(k) == Some(i)) {
            return Some(format!("That ramp has {} along it already: one to a ramp", name(&self.map.props[other].prop)));
        }
        // nothing that doesn't stack in the way
        let others = rooms::ground_items(self, level, except);
        rooms::clash(self, &prop.id, &r.cells(), &others).map(|other| format!("No room: {other} is on that ramp, and one of them doesn't stack"))
    }

    /// The Place tool's click with what lies along ramps: laid along the ramp under the
    /// pointer (or next to it), turned up its way, from its foot to its top and end to end;
    /// refused anywhere else, saying why.
    pub(super) fn place_along(&mut self, id: &str) {
        let Some(prop) = self.content.props.get(id).cloned() else { return };
        let Some(i) = self.ramp_under_pointer() else {
            self.status = format!("No ramp there to lay {} along: click one (the Ground tool's Ramps tab makes them)", prop.name);
            return;
        };
        if let Some(why) = self.along_refused(&prop, i, None) {
            self.status = why;
            return;
        }
        self.checkpoint();
        let placed = self.laid_on(id, i);
        self.map.props.push(placed);
        self.selected = Some(self.map.props.len() - 1);
        self.along_changed();
        let r = &self.map.ramps[i];
        self.status = format!("{} along the ramp at {}, {}: {} cell{} long, its foot to the {}", prop.name, r.at[0], r.at[1], r.length(), if r.length() == 1 { "" } else { "s" }, foot_name(&r.up));
    }

    /// Select's drag of something along a ramp: onto another ramp under the pointer, when it
    /// can go there.
    pub(super) fn drag_along(&mut self, i: usize) {
        let (Some(from), Some(to)) = (self.along(i), self.ramp_under_pointer()) else { return };
        if from == to {
            return;
        }
        let id = self.map.props[i].prop.clone();
        let Some(prop) = self.content.props.get(&id).cloned() else { return };
        if let Some(why) = self.along_refused(&prop, to, Some(i)) {
            self.status = why;
            return;
        }
        if !self.stroke {
            self.checkpoint();
            self.stroke = true;
        }
        self.map.props[i] = self.laid_on(&id, to);
        self.along_changed();
    }

    /// After something along a ramp came, went or moved: it, and the ramp's ground under it.
    fn along_changed(&mut self) {
        self.dirty = true;
        self.need_props = true;
        self.need_ground = true;
    }

    /// The Place tool's mark for what lies along ramps: the slope of the ramp it'd go on, from
    /// its foot to its top (crimson when it can't go there; over its cells when something lies
    /// along it already), else the cell under the pointer, crimson.
    pub(super) fn along_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        let Some(prop) = self.prop.as_ref().and_then(|id| self.content.props.get(id)) else { return };
        let Some(i) = self.ramp_under_pointer() else {
            if let Some((x, z)) = self.ground_hit {
                let y = self.map.ground_y(x as f32 + 0.5, z as f32 + 0.5, self.map.level_at(x, z).unwrap_or(0), lh);
                shapes.push(Shape { kind: ShapeKind::Rect { width: 0.94, depth: 0.94 }, center: Vec3::new(x as f32 + 0.5, y + 0.04, z as f32 + 0.5), color: REFUSED, additive: false });
            }
            return;
        };
        match self.along_refused(prop, i, None) {
            None => shapes.push(self.slope_mark(i, [1.0, 1.0, 1.0, 0.4])),
            Some(_) if (0..self.map.props.len()).any(|k| self.along(k) == Some(i)) => self.over_ramp(i, REFUSED, shapes),
            Some(_) => shapes.push(self.slope_mark(i, REFUSED)),
        }
    }

    /// A mark along ramp `i`'s slope, from its foot to its top, over the straight line between
    /// them (clear of its ground, as uneven as it is).
    fn slope_mark(&self, i: usize, color: [f32; 4]) -> Shape {
        let lh = self.level_height;
        let r = &self.map.ramps[i];
        let top = self.map.level_at(r.at[0], r.at[1]).unwrap_or(0) as f32 * lh;
        let (x0, z0, x1, z1) = (r.at[0] as f32, r.at[1] as f32, (r.at[0] + r.size[0] as i32) as f32, (r.at[1] + r.size[1] as i32) as f32);
        let height = |px: f32, pz: f32| top - lh + lh * r.along(px as f64, pz as f64).0.clamp(0.0, 1.0) as f32 + 0.2;
        let corners = [(x0, z0), (x0, z1), (x1, z1), (x1, z0)].map(|(px, pz)| Vec3::new(px, height(px, pz), pz));
        Shape { kind: ShapeKind::Quad { corners }, center: Vec3::ZERO, color, additive: false }
    }

    /// Marks over ramp `i`'s cells, on their level: over what lies along it.
    pub(super) fn over_ramp(&self, i: usize, color: [f32; 4], shapes: &mut Vec<Shape>) {
        let r = &self.map.ramps[i];
        let top = self.map.level_at(r.at[0], r.at[1]).unwrap_or(0) as f32 * self.level_height;
        for (x, z) in r.cells() {
            shapes.push(Shape { kind: ShapeKind::Rect { width: 0.94, depth: 0.94 }, center: Vec3::new(x as f32 + 0.5, top + 0.05, z as f32 + 0.5), color, additive: false });
        }
    }

    /// After a ramp changed: the ground built again, and what stands on it.
    fn ramp_changed(&mut self) {
        self.dirty = true;
        self.need_ground = true;
        self.need_props = true;
        self.need_trees = true;
    }

    /// The Ground tool's ramps on the map: the cells a drag goes along, the ramp picked, the
    /// others faintly.
    pub(super) fn ramp_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        // a cell's mark, over the highest of its corners
        let cell = |x: i32, z: i32, color: [f32; 4]| {
            let level = self.map.level_at(x, z).unwrap_or(0);
            let top = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].iter().map(|&(u, v)| self.map.ground_y(x as f32 + u, z as f32 + v, level, lh)).fold(f32::MIN, f32::max);
            Shape { kind: ShapeKind::Rect { width: 0.94, depth: 0.94 }, center: Vec3::new(x as f32 + 0.5, top + 0.04, z as f32 + 0.5), color, additive: false }
        };
        if let Some((a, b)) = self.ramp.drag {
            for z in a.1.min(b.1)..=a.1.max(b.1) {
                for x in a.0.min(b.0)..=a.0.max(b.0) {
                    shapes.push(cell(x, z, [1.0, 1.0, 1.0, 0.35]));
                }
            }
        }
        for (i, r) in self.map.ramps.iter().enumerate() {
            let color = if Some(i) == self.ramp.selected { [0.45, 0.8, 1.0, 0.42] } else { [0.45, 0.8, 1.0, 0.12] };
            for (x, z) in r.cells() {
                shapes.push(cell(x, z, color));
            }
        }
    }

    /// The ramps' marks for the checks: how many; the one picked (x, z; length; way up; how
    /// uneven).
    pub(super) fn ramp_hooks(&mut self) {
        crate::platform::set_hook("ramps", &self.map.ramps.len().to_string());
        crate::platform::set_hook("groundmode", if self.ramp.on { "ramps" } else if self.floorer.on { "floors" } else { "paint" });
        let picked = self.ramp.selected.and_then(|i| self.map.ramps.get(i)).map(|r| format!("{},{};{};{};{:.2}", r.at[0], r.at[1], r.length(), r.up, r.rough));
        crate::platform::set_hook("ramp", &picked.unwrap_or_default());
        // what lies along ramps: the prop, the ramp's corner, how many copies laid on it;
        // looked for again only when the props or the ramps changed (`sync`), among the props
        // standing on a ramp's cells
        if self.along_hook.is_none() {
            let on_ramp = |k: &usize| self.map.props.get(*k).is_some_and(|p| self.map.ramp_at(p.at[0].floor() as i32, p.at[1].floor() as i32).is_some());
            let along: Vec<String> = (0..self.map.props.len())
                .filter(on_ramp)
                .filter_map(|k| {
                    let r = &self.map.ramps[self.along(k)?];
                    let p = &self.map.props[k];
                    let copies = self.content.props.get(&p.prop).map(|q| self.map.puts(p, q, self.level_height).len()).unwrap_or(0);
                    Some(format!("{}@{},{}x{copies}", p.prop, r.at[0], r.at[1]))
                })
                .collect();
            self.along_hook = Some(along.join(";"));
        }
        crate::platform::set_hook("along", self.along_hook.as_deref().unwrap_or(""));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// An editor on a map 24 x 20, seen from the south: a plateau a level up north of z 8 and
    /// west of x 14, the lowland round it (a bay up to z 0 at x 14 to 17), a tower two levels
    /// up beyond (from x 18).
    fn editor() -> MapEditor {
        editor_with(content::Content::default())
    }

    fn editor_with(content: content::Content) -> MapEditor {
        let root = std::env::temp_dir().join("battle-frontier-ramps");
        let mut ed = MapEditor::new(Arc::new(content), None, crate::platform::Files::new(root.to_str().unwrap()));
        let mut map = world::WorldMap::new("hill", "Hill", 24, 20, "grass");
        for z in 0..8 {
            for x in 0..24 {
                map.level[z * 24 + x] = match x {
                    0..14 => 1,
                    14..18 => 0,
                    _ => 2,
                };
            }
        }
        ed.map = map;
        ed.cam.yaw = 0.0;
        ed
    }

    #[test]
    fn a_drag_along_the_top_of_a_step_makes_a_ramp_down_it() {
        let mut ed = editor();
        let i = ed.make_ramp((5, 7), (3, 7)).unwrap();
        let r = &ed.map.ramps[i];
        assert_eq!((r.at, r.size, r.up.as_str(), r.rough), ([3, 7], [3, 1], "n", content::ROUGH));
        // over it again; two rows deep; two levels down; nothing a level down
        assert!(ed.make_ramp((4, 7), (4, 7)).unwrap_err().contains("one already"));
        assert!(ed.make_ramp((9, 6), (10, 7)).unwrap_err().contains("one cell deep"));
        assert!(ed.make_ramp((19, 7), (20, 7)).unwrap_err().contains("one level only"));
        assert!(ed.make_ramp((10, 3), (12, 3)).unwrap_err().contains("don't step down"));
        assert!(ed.make_ramp((3, 9), (6, 9)).unwrap_err().contains("nothing below level 0"));
        assert_eq!(ed.map.ramps.len(), 1);
        // undone, and gone; one of a cell, and taken away
        ed.undo();
        assert!(ed.map.ramps.is_empty());
        let i = ed.make_ramp((8, 7), (8, 7)).unwrap();
        assert_eq!((ed.map.ramps[i].size, ed.map.ramps[i].up.as_str()), ([1, 1], "n"));
        ed.remove_ramp(i);
        assert!(ed.map.ramps.is_empty());
    }

    #[test]
    fn past_the_step_s_end_cut_back_and_at_a_corner_the_side_looked_at() {
        let mut ed = editor();
        // along the plateau's edge, on past its end over the bay: cut back to the plateau
        let i = ed.make_ramp((12, 7), (16, 7)).unwrap();
        assert_eq!((ed.map.ramps[i].at, ed.map.ramps[i].size), ([12, 7], [2, 1]));
        ed.remove_ramp(i);
        // its corner cell steps down south and east: down the side the camera is on
        let i = ed.make_ramp((13, 7), (13, 7)).unwrap();
        assert_eq!(ed.map.ramps[i].up, "n");
        ed.undo();
        ed.cam.yaw = 90.0;
        let i = ed.make_ramp((13, 7), (13, 7)).unwrap();
        assert_eq!(ed.map.ramps[i].up, "w");
    }

    /// The plateau's editor with what lies along ramps (steps, stretched, and a slide, copies
    /// side by side; the steps don't stack) and a crate, and three ramps down its south edge:
    /// 3 to 5, 9 and 10, 12 and 13.
    fn with_ramps() -> MapEditor {
        let mut c = content::Content::default();
        let wedge = content::Part { shape: "wedge".into(), size: [1.0, 2.0, 1.0], color: "#777777".into(), ..Default::default() };
        let along = |id: &str, name: &str, how: &str, unstackable: bool| Prop {
            id: id.into(),
            name: name.into(),
            category: "ramp".into(),
            footprint: [1, 1],
            height: 2.0,
            unstackable,
            parts: vec![wedge.clone()],
            ramp: Some(content::RampFit { along: how.into(), ..Default::default() }),
            ..Default::default()
        };
        c.props.put(along("steps", "Steps", "stretch", true), content::Space::Content);
        c.props.put(along("slide", "Slide", "repeat", false), content::Space::Content);
        c.props.put(Prop { id: "crate".into(), name: "Crate".into(), category: "decoration".into(), footprint: [1, 1], height: 1.0, ..Default::default() }, content::Space::Content);
        let mut ed = editor_with(c);
        for (a, b) in [((3, 7), (5, 7)), ((9, 7), (10, 7)), ((12, 7), (13, 7))] {
            ed.make_ramp(a, b).unwrap();
        }
        ed
    }

    #[test]
    fn what_lies_along_ramps_goes_on_one_fitted_and_nowhere_else() {
        let mut ed = with_ramps();
        // on a ramp's cell: on its centre, its level, turned up its way
        ed.ground_hit = Some((4, 7));
        ed.place_along("steps");
        assert_eq!(ed.map.props.len(), 1);
        let p = &ed.map.props[0];
        assert_eq!((p.prop.as_str(), p.at, p.level, p.turn), ("steps", [4.5, 7.5], 1, 0.0));
        assert_eq!((ed.along(0), ed.selected), (Some(0), Some(0)));
        assert_eq!(ed.status, "Steps along the ramp at 3, 7: 3 cells long, its foot to the south");
        // it covers the ramp's cells, from its foot to its top
        assert_eq!(ed.map.footprint(&ed.map.props[0], &ed.content), [(3, 7), (4, 7), (5, 7)]);
        let (lo, hi) = ed.prop_box(&ed.map.props[0]).unwrap();
        let lh = ed.level_height;
        assert!(lo.distance(Vec3::new(3.0, 0.0, 7.0)) < 1e-5 && hi.distance(Vec3::new(6.0, lh + 0.3, 8.0)) < 1e-5, "{lo} {hi}");
        // one to a ramp
        ed.place_along("slide");
        assert!(ed.status.contains("That ramp has Steps along it already"), "{}", ed.status);
        // nowhere but on a ramp: the plateau's middle, the lowland
        for cell in [(8, 3), (8, 12)] {
            ed.ground_hit = Some(cell);
            ed.place_along("slide");
            assert!(ed.status.starts_with("No ramp there to lay Slide along"), "{}", ed.status);
        }
        assert_eq!(ed.map.props.len(), 1);
        // pointed at the ground past a ramp's foot: that ramp, copies a cell wide side by side
        ed.ground_hit = Some((10, 8));
        ed.place_along("slide");
        assert_eq!((ed.map.props.len(), ed.along(1)), (2, Some(1)));
        assert_eq!(ed.map.props[1].at, [10.0, 7.5]);
        let slide = ed.content.props.get("slide").unwrap().clone();
        assert_eq!(ed.map.puts(&ed.map.props[1], &slide, ed.level_height).len(), 2);
        // something that doesn't stack in the way: no room
        ed.map.props.push(Placed { prop: "crate".into(), at: [12.5, 7.5], level: 1, ..Default::default() });
        ed.ground_hit = Some((12, 7));
        ed.place_along("steps");
        assert!(ed.status.starts_with("No room: Crate is on that ramp"), "{}", ed.status);
        assert_eq!(ed.map.props.len(), 3);
        // the Place tool's mark: up the ramp's slope from its foot to its top, white where it
        // can go (the slide stacks with the crate), crimson where it can't (the steps don't)
        let mark = |ed: &mut MapEditor, id: &str| {
            ed.prop = Some(id.into());
            let mut shapes = vec![];
            ed.along_shapes(&mut shapes);
            match shapes.pop() {
                Some(Shape { kind: ShapeKind::Quad { corners }, color, .. }) => (corners, color),
                other => panic!("{other:?}"),
            }
        };
        let (corners, white) = mark(&mut ed, "slide");
        assert!(corners.iter().all(|c| (c.y - if c.z == 8.0 { 0.2 } else { lh + 0.2 }).abs() < 1e-5), "{corners:?}");
        let (_, crimson) = mark(&mut ed, "steps");
        assert!(white[1] > 0.9 && crimson[1] < 0.5, "{white:?} {crimson:?}");
        // over a ramp something lies along already: its cells, on their level, crimson
        ed.ground_hit = Some((4, 7));
        let mut shapes = vec![];
        ed.along_shapes(&mut shapes);
        assert!(shapes.len() == 3 && shapes.iter().all(|s| matches!(s.kind, ShapeKind::Rect { .. }) && (s.center.y - (lh + 0.05)).abs() < 1e-5 && s.color[1] < 0.5), "{shapes:?}");
        // undone: the slide gone; the ramp removed: what lay along it with it
        ed.undo();
        assert_eq!(ed.map.props.len(), 1);
        ed.remove_ramp(0);
        assert!(ed.map.props.is_empty() && ed.status == "The ramp is gone, and what lay along it: a cliff again", "{}", ed.status);
    }

    #[test]
    fn select_drags_it_from_ramp_to_ramp_never_turns_it() {
        let mut ed = with_ramps();
        ed.ground_hit = Some((4, 7));
        ed.place_along("steps");
        ed.ground_hit = Some((9, 7));
        ed.place_along("slide");
        // onto the free ramp, turned its way; not onto the taken one
        ed.ground_hit = Some((13, 7));
        ed.drag_along(0);
        assert_eq!((ed.along(0), ed.map.props[0].at), (Some(2), [13.0, 7.5]));
        ed.ground_hit = Some((10, 7));
        ed.drag_along(0);
        assert!(ed.status.contains("That ramp has Slide along it already") && ed.along(0) == Some(2), "{}", ed.status);
        // Q and E don't turn it
        ed.selected = Some(0);
        ed.turn_selected(90.0);
        assert!(ed.map.props[0].turn == 0.0 && ed.status.starts_with("It lies along its ramp"));
        // a cell of its ramp painted another level: the ramp gone, and it with it
        (ed.set_level, ed.level, ed.brush, ed.terrain) = (true, 0, 0, "grass".into());
        ed.paint_ground((12, 7));
        assert_eq!(ed.map.props.iter().map(|p| p.prop.as_str()).collect::<Vec<_>>(), ["slide"]);
        assert!(ed.status.contains("with what lay along it"), "{}", ed.status);
    }
}
