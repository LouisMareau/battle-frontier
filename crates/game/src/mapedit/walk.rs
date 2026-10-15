//! Walking the map in the editor: a stand-in character on the grid, moved cell by cell with
//! the arrows (up is away from the camera) at a fixed pace, so 60 and 144 Hz screens play
//! the same. What blocks on foot stops it: tree areas, water, cliffs, blocking props, the
//! walls of buildings; doors let it in, and the building it's in is cut away above it. A
//! script can walk it too (`route`), and holds it still while a dialog is up.

use std::collections::VecDeque;

use glam::Vec3;
use render::map::MapVertex;
use world::mesh::{placement, prop_scene, Scene};

use super::actors::facing_of;
use super::{gpu, models, MapEditor};

/// One cell's walk, in milliseconds.
const STEP_MS: f64 = 210.0;
const CHARACTER: &str = "stand_in_character";
/// A building's upper floors and roof fading, in milliseconds.
const FADE_MS: f64 = 450.0;
/// How dark the outside goes while inside, and how much closer the view comes.
const INSIDE_DIM: f32 = 0.75;
const INSIDE_ZOOM: f32 = 0.8;

/// Meshes for the renderer, by texture.
pub(super) type Batches = Vec<(String, Vec<MapVertex>, Vec<u32>)>;

/// A building's upper floors and roof on their way out (walking in) or back (walking out).
pub(super) struct Fade {
    pub building: usize,
    pub top: usize,
    pub t0: f64,
    pub entering: bool,
}

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
        let inside = self.walker.as_ref().and_then(|w| w.inside);
        self.walker = Some(Walker { cell, from: pos, to: pos, t0: 0.0, facing, inside, route: VecDeque::new() });
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

    /// Puts the walker on a cell, facing south (a check's `walkto:x,z`).
    pub fn walk_to(&mut self, x: i32, z: i32) {
        if self.map.index(x, z).is_none() {
            return;
        }
        self.stand((x, z), 0.0);
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
            self.fade = None;
            self.cam.zoom = 1.0;
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
            // the floors above and the roof fade out on the way in, back on the way out
            self.fade = match (was, inside) {
                (_, Some((i, f))) => Some(Fade { building: i, top: f, t0: now, entering: true }),
                (Some((i, f)), None) => Some(Fade { building: i, top: f, t0: now, entering: false }),
                _ => None,
            };
            self.walker.as_mut().unwrap().inside = inside;
            self.need_buildings = true;
        }
        if self.fade.as_ref().map(|f| now - f.t0 >= FADE_MS).unwrap_or(false) {
            self.fade = None;
            self.need_buildings = true;
        }
        // the view comes a little closer inside
        let k = self.inside_amount(now);
        self.cam.zoom = 1.0 - (1.0 - INSIDE_ZOOM) * k * k * (3.0 - 2.0 * k);
        crate::platform::set_hook("inside", &self.walker.as_ref().and_then(|w| w.inside).map(|(i, _)| self.map.buildings[i].id.clone()).unwrap_or_default());
        crate::platform::set_hook("dim", &format!("{:.2}", self.focus_dim(now)));
        // the camera follows, unless a script holds it somewhere
        match self.story.cam_hold {
            Some(h) => self.cam.target = self.cam.target.lerp(h, 0.08),
            None if self.story.cam_until > now => self.cam.target = self.cam.target.lerp(pos, 0.12),
            None => self.cam.target = pos,
        }
        self.extra_changed = true;
        crate::platform::set_hook("walker", &format!("{},{}", cell.0, cell.1));
    }

    /// How a building shows for the walker: cut above the floor they're on, also while
    /// its upper part fades.
    pub(super) fn building_cut(&self, i: usize) -> Option<usize> {
        if let Some(f) = self.fade.as_ref().filter(|f| f.building == i) {
            return Some(f.top);
        }
        self.walker.as_ref().and_then(|w| w.inside).filter(|(b, _)| *b == i).map(|(_, f)| f)
    }

    /// How far inside a building the view is, 0 to 1: rising on the way in, falling on
    /// the way out.
    pub(super) fn inside_amount(&self, now: f64) -> f32 {
        match &self.fade {
            Some(f) => {
                let k = ((now - f.t0) / FADE_MS).clamp(0.0, 1.0) as f32;
                if f.entering { k } else { 1.0 - k }
            }
            None => self.walker.as_ref().and_then(|w| w.inside).map(|_| 1.0).unwrap_or(0.0),
        }
    }

    /// How dark the rest of the map goes: fully while a building is designed, somewhat
    /// while the walker is inside one.
    pub(super) fn focus_dim(&self, now: f64) -> f32 {
        if self.draft.as_ref().map(|d| d.step == super::build::Step::Design).unwrap_or(false) {
            return 1.0;
        }
        INSIDE_DIM * self.inside_amount(now)
    }

    /// The moving things as the renderer's batches (the walker and the actors), and apart
    /// from them the building part fading, as see-through as the fade says.
    pub(super) fn extra_batches(&self, now: f64) -> (Batches, Batches) {
        let moving = models::batches(&self.extra_mesh(now));
        let mut out = vec![];
        if let Some(f) = &self.fade {
            let Some(b) = self.map.buildings.get(f.building) else { return (moving, out) };
            let k = ((now - f.t0) / FADE_MS).clamp(0.0, 1.0) as f32;
            let alpha = if f.entering { 1.0 - k } else { k };
            let mut part = world::buildings::building_above(b, &self.content, self.level_height, f.top, &self.models);
            for v in part.plain.vertices.iter_mut().chain(part.textured.values_mut().flat_map(|m| m.vertices.iter_mut())) {
                v.color[3] = alpha;
            }
            if !part.plain.indices.is_empty() {
                out.push((render::scene::WHITE.to_string(), gpu(&part.plain.vertices), part.plain.indices));
            }
            for (key, mesh) in part.textured {
                out.push((key, gpu(&mesh.vertices), mesh.indices));
            }
        }
        (moving, out)
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
