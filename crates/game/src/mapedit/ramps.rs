//! The Ground tool's ramps: a drag along the top of a step in the ground (the cells a level up,
//! where it steps down one level) makes them a ramp down to the ground below, one cell deep
//! (`content::Ramp`, its ground shaped by `world::ramps`); a click makes one of one cell. A
//! click on a ramp picks it, to make it more or less uneven, give it another look, or take it
//! away (Delete).

use content::Ramp;
use egui::RichText;
use glam::Vec3;
use render::scene::{Shape, ShapeKind};

use super::{hint, mark, section, MapEditor};

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
            if map.buildings.iter().any(|b| b.map_cells().contains(&(x, z))) {
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

    /// Takes a ramp away: a cliff again.
    pub(super) fn remove_ramp(&mut self, i: usize) {
        if i >= self.map.ramps.len() {
            return;
        }
        self.checkpoint();
        self.map.ramps.remove(i);
        self.ramp.selected = None;
        self.ramp_changed();
        self.status = "The ramp is gone: a cliff again".into();
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
    pub(super) fn ramp_hooks(&self) {
        crate::platform::set_hook("ramps", &self.map.ramps.len().to_string());
        crate::platform::set_hook("groundmode", if self.ramp.on { "ramps" } else { "paint" });
        let picked = self.ramp.selected.and_then(|i| self.map.ramps.get(i)).map(|r| format!("{},{};{};{};{:.2}", r.at[0], r.at[1], r.length(), r.up, r.rough));
        crate::platform::set_hook("ramp", &picked.unwrap_or_default());
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
        let root = std::env::temp_dir().join("battle-frontier-ramps");
        let mut ed = MapEditor::new(Arc::new(content::Content::default()), None, crate::platform::Files::new(root.to_str().unwrap()));
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
}
