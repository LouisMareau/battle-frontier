//! Walking the map in the editor: a stand-in character on the grid, moved cell by cell with
//! the arrows (up is away from the camera) at a fixed pace, so 60 and 144 Hz screens play
//! the same. What blocks on foot stops it: tree areas, water, cliffs, blocking props, the
//! walls of buildings; doors let it in, and the building it's in is cut away above it. A
//! script can walk it too (`route`), and holds it still while a dialog is up.

use std::collections::VecDeque;

use glam::Vec3;
use world::mesh::{placement, prop_scene, Scene};

use super::actors::facing_of;
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
    pub facing: f32,
    /// the building it's in, and the floor
    pub inside: Option<(usize, usize)>,
    /// cells a script sends it along
    pub route: VecDeque<(i32, i32)>,
}

impl Walker {
    pub fn pos(&self, now: f64) -> Vec3 {
        let t = ((now - self.t0) / STEP_MS).clamp(0.0, 1.0) as f32;
        self.from.lerp(self.to, t)
    }

    pub fn stepping(&self, now: f64) -> bool {
        now - self.t0 < STEP_MS
    }
}

impl MapEditor {
    pub(super) fn cell_pos(&self, c: (i32, i32)) -> Vec3 {
        let level = self.map.level_at(c.0, c.1).unwrap_or(0) as f32;
        Vec3::new(c.0 as f32 + 0.5, level * self.level_height, c.1 as f32 + 0.5)
    }

    fn stand(&mut self, cell: (i32, i32), facing: f32) {
        let pos = self.cell_pos(cell);
        self.walker = Some(Walker { cell, from: pos, to: pos, t0: 0.0, facing, inside: None, route: VecDeque::new() });
        self.blocked = Some(self.map.blocked(&self.content));
        self.story.started = false;
        // standing on cells counts as entering them
        self.story.last_cell = None;
        self.extra_changed = true;
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
        self.stand(cell, 180.0);
        self.status = "Walking: the arrows move, A speaks to what's in front, Esc stops".into();
    }

    /// Puts the walker on a cell (a check's `walkto:x,z`, a warp's landing).
    pub fn walk_to(&mut self, x: i32, z: i32) {
        if self.map.index(x, z).is_none() {
            return;
        }
        let facing = self.walker.as_ref().map(|w| w.facing).unwrap_or(0.0);
        self.stand((x, z), facing);
        self.story.started = true;
        self.cam.target = self.cell_pos((x, z));
    }

    /// A map opened for a walker coming from the map next to it: it stands where it
    /// arrived.
    pub(super) fn finish_arrival(&mut self) {
        let Some((cell, facing)) = self.arrive.take() else { return };
        self.stand(cell, facing);
        self.cam.target = self.cell_pos(cell);
        self.status = format!("Walking on {}: the arrows move, Esc stops", self.map.name);
    }

    pub(super) fn stop_walk(&mut self) {
        if self.walker.take().is_some() {
            self.need_buildings = true;
            self.extra_changed = true;
            self.story.machine = None;
            self.story.dialog = None;
            self.story.running.clear();
            self.story.cam_hold = None;
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
        // a script has the floor: the arrows do nothing
        let frozen = self.story.machine.is_some() || self.story.dialog.is_some();
        // held, or tapped this frame (a tap can start and end between two frames)
        let held = |k: egui::Key| !typing && !frozen && ctx.input(|i| i.key_down(k) || i.key_pressed(k));
        let (right, forward) = self.cam.ground_axes();
        let snap = |v: Vec3| if v.x.abs() > v.z.abs() { (v.x.signum() as i32, 0) } else { (0, v.z.signum() as i32) };
        let w = self.walker.as_ref().unwrap();
        let t = ((now - w.t0) / STEP_MS).clamp(0.0, 1.0) as f32;
        let pos = w.from.lerp(w.to, t);
        let routed = w.route.front().map(|&n| (n.0 - w.cell.0, n.1 - w.cell.1));
        let dir = if let Some(d) = routed {
            if t >= 1.0 { Some(d) } else { None }
        } else if held(egui::Key::ArrowUp) {
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
        if let (Some(d), true) = (dir, t >= 1.0) {
            let cell = w.cell;
            let next = (cell.0 + d.0, cell.1 + d.1);
            let facing = facing_of(d);
            if self.map.index(next.0, next.1).is_none() {
                // off the map: on into the piece next to it in the world, if there's one
                self.walker.as_mut().unwrap().facing = facing;
                if let Some((id, there)) = self.across(next) {
                    if self.dirty {
                        self.status = format!("Save {} first, then walk on into {id}", self.map.id);
                    } else {
                        self.arrive = Some((there, facing));
                        self.open(&id);
                        return;
                    }
                }
            }
            let someone = self.actor_at(next).is_some();
            let free = (self.map.can_step(self.blocked.as_ref().unwrap(), cell, next) || routed.is_some() && self.map.index(next.0, next.1).is_some()) && !someone;
            let to = self.cell_pos(next);
            let w = self.walker.as_mut().unwrap();
            w.facing = facing;
            if routed.is_some() {
                if free {
                    w.route.pop_front();
                } else if someone {
                    // someone in the way: wait for them
                } else {
                    w.route.pop_front();
                }
            }
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
        // the camera follows, unless a script holds it somewhere
        match self.story.cam_hold {
            Some(h) => self.cam.target = self.cam.target.lerp(h, 0.08),
            None if self.story.cam_until > now => self.cam.target = self.cam.target.lerp(pos, 0.12),
            None => self.cam.target = pos,
        }
        self.extra_changed = true;
        crate::platform::set_hook("walker", &format!("{},{}", cell.0, cell.1));
    }

    /// The things that move: the walker and the actors drawn as props.
    pub(super) fn extra_mesh(&self, now: f64) -> Scene {
        let mut out = self.actor_scene(now);
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
