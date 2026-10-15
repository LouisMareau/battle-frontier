//! Walking the map in the editor: a stand-in character on the grid, moved cell by cell with
//! the arrows (up is away from the camera) at a fixed pace, so 60 and 144 Hz screens play
//! the same. What blocks on foot stops it: tree areas, water, cliffs, blocking props, the
//! walls of buildings; doors let it in, and the building it's in is cut away above it.

use glam::Vec3;
use world::mesh::{placement, prop_scene, Scene};

use super::MapEditor;

/// One cell's walk, in milliseconds.
const STEP_MS: f64 = 210.0;
const CHARACTER: &str = "stand_in_character";

pub(super) struct Walker {
    pub cell: (i32, i32),
    from: Vec3,
    to: Vec3,
    t0: f64,
    /// degrees clockwise from south
    facing: f32,
    /// the building it's in, and the floor
    pub inside: Option<(usize, usize)>,
}

impl MapEditor {
    fn cell_pos(&self, c: (i32, i32)) -> Vec3 {
        let level = self.map.level_at(c.0, c.1).unwrap_or(0) as f32;
        Vec3::new(c.0 as f32 + 0.5, level * self.level_height, c.1 as f32 + 0.5)
    }

    /// Starts walking from the free cell nearest the middle of the view.
    pub(super) fn start_walk(&mut self) {
        let blocked = self.map.blocked(&self.content);
        let c = (self.cam.target.x.floor() as i32, self.cam.target.z.floor() as i32);
        let mut found = None;
        'search: for r in 0..64i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    if let Some(i) = self.map.index(c.0 + dx, c.1 + dz) {
                        if !blocked[i] {
                            found = Some((c.0 + dx, c.1 + dz));
                            break 'search;
                        }
                    }
                }
            }
        }
        let Some(cell) = found else {
            self.status = "Nowhere to stand on this map".into();
            return;
        };
        let pos = self.cell_pos(cell);
        self.walker = Some(Walker { cell, from: pos, to: pos, t0: 0.0, facing: 180.0, inside: None });
        self.blocked = Some(blocked);
        self.status = "Walking: the arrows move, Esc stops".into();
    }

    pub(super) fn stop_walk(&mut self) {
        if self.walker.take().is_some() {
            self.need_buildings = true;
            self.extra_changed = true;
            self.status.clear();
        }
    }

    /// A frame of walking: reads the arrows, steps, follows with the camera.
    pub(super) fn walk(&mut self, ctx: &egui::Context, now: f64) {
        if self.walker.is_none() {
            return;
        }
        if self.blocked.is_none() {
            self.blocked = Some(self.map.blocked(&self.content));
        }
        let typing = ctx.egui_wants_keyboard_input();
        // held, or tapped this frame (a tap can start and end between two frames)
        let held = |k: egui::Key| !typing && ctx.input(|i| i.key_down(k) || i.key_pressed(k));
        let (right, forward) = self.cam.ground_axes();
        let snap = |v: Vec3| if v.x.abs() > v.z.abs() { (v.x.signum() as i32, 0) } else { (0, v.z.signum() as i32) };
        let dir = if held(egui::Key::ArrowUp) {
            Some(snap(forward))
        } else if held(egui::Key::ArrowDown) {
            Some(snap(-forward))
        } else if held(egui::Key::ArrowLeft) {
            Some(snap(-right))
        } else if held(egui::Key::ArrowRight) {
            Some(snap(right))
        } else {
            None
        };
        let w = self.walker.as_ref().unwrap();
        let t = ((now - w.t0) / STEP_MS).clamp(0.0, 1.0) as f32;
        let pos = w.from.lerp(w.to, t);
        if let (Some(d), true) = (dir, t >= 1.0) {
            let cell = w.cell;
            let next = (cell.0 + d.0, cell.1 + d.1);
            let facing = match d {
                (0, 1) => 0.0,
                (-1, 0) => 90.0,
                (0, -1) => 180.0,
                _ => 270.0,
            };
            let free = self.map.can_step(self.blocked.as_ref().unwrap(), cell, next);
            let to = self.cell_pos(next);
            let w = self.walker.as_mut().unwrap();
            w.facing = facing;
            if free {
                w.from = pos;
                w.to = to;
                w.t0 = now;
                w.cell = next;
            }
        }
        let (cell, was) = { let w = self.walker.as_ref().unwrap(); (w.cell, w.inside) };
        let level = self.map.level_at(cell.0, cell.1).unwrap_or(0);
        let inside = self.map.buildings.iter().position(|b| b.map_cells().contains(&cell)).map(|i| (i, level.saturating_sub(self.map.buildings[i].level) as usize));
        if inside != was {
            self.walker.as_mut().unwrap().inside = inside;
            self.need_buildings = true;
        }
        self.cam.target = pos;
        self.extra_changed = true;
        crate::platform::set_hook("walker", &format!("{},{}", cell.0, cell.1));
    }

    /// The things that move: the walker.
    pub(super) fn extra_mesh(&self, now: f64) -> Scene {
        let mut out = Scene::default();
        let Some(w) = &self.walker else { return out };
        let Some(prop) = self.content.props.get(CHARACTER) else { return out };
        let t = ((now - w.t0) / STEP_MS).clamp(0.0, 1.0) as f32;
        let mut pos = w.from.lerp(w.to, t);
        // a little bob while walking
        if t < 1.0 {
            pos.y += (t * std::f32::consts::PI).sin() * 0.06;
        }
        out.append(&prop_scene(prop, &self.models), placement(pos, w.facing, 1.0));
        out
    }
}
