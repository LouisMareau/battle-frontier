//! Walking the map in the editor: a stand-in character on the grid, moved cell by cell with
//! the arrows (up is away from the camera) at a fixed pace, so 60 and 144 Hz screens play
//! the same. What blocks on foot stops it: tree areas, water, cliffs, blocking props, the
//! walls of buildings; doors let it in, and the building it's in is cut away above it, the
//! walls facing the camera on its floor cut down to a stub (fading, as the camera turns too).
//! Stairs take it up: from in front of a flight's bottom step, walking the way it rises,
//! it climbs to the floor above (or onto a flat roof), and back down the same way. A script
//! can walk it too (`route`), and holds it still while a dialog is up.

use std::collections::VecDeque;

use glam::Vec3;
use render::map::MapVertex;
use world::buildings::{Cut, Sides};
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

/// A flight of stairs, for the walker: the cell in front of its bottom step, the cell past
/// its top (on the floor above), and the way up.
pub(super) type Flight = ((i32, i32), (i32, i32), (i32, i32));

/// A building's upper floors and roof on their way out (walking in) or back (walking out),
/// and the walls facing the camera on the walker's floor (`low`'s sides) above their stubs.
pub(super) struct Fade {
    pub building: usize,
    pub top: usize,
    pub t0: f64,
    pub entering: bool,
    pub low: Sides,
}

/// Walls of the floor the walker's on, as the camera turned: those it faced (`from`'s sides)
/// rising back, those it faces now (`to`'s) going down to their stubs.
pub(super) struct WallFade {
    pub building: usize,
    pub floor: usize,
    pub from: Sides,
    pub to: Sides,
    pub t0: f64,
}

pub(super) struct Walker {
    pub cell: (i32, i32),
    from: Vec3,
    to: Vec3,
    t0: f64,
    /// how long the move from `from` to `to` takes (a step, or a flight of stairs)
    dur: f64,
    /// on a building's upper floor (or its roof): the building and the floor
    pub upstairs: Option<(usize, usize)>,
    /// degrees clockwise from south
    pub facing: f32,
    /// the building it's in, and the floor
    pub inside: Option<(usize, usize)>,
    /// cells a script sends it along
    pub route: VecDeque<(i32, i32)>,
}

impl Walker {
    pub fn pos(&self, now: f64) -> Vec3 {
        let t = ((now - self.t0) / self.dur).clamp(0.0, 1.0) as f32;
        self.from.lerp(self.to, t)
    }

    pub fn stepping(&self, now: f64) -> bool {
        now - self.t0 < self.dur
    }

    /// Into the space of a map `by` away (walking on into it): the step it's taking goes on.
    pub fn shift(&mut self, by: glam::Vec3) {
        self.from -= by;
        self.to -= by;
        self.cell = (self.cell.0 - by.x.round() as i32, self.cell.1 - by.z.round() as i32);
        self.inside = None;
        self.upstairs = None;
        self.route.clear();
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
        self.walker = Some(Walker { cell, from: pos, to: pos, t0: 0.0, dur: STEP_MS, upstairs: None, facing, inside, route: VecDeque::new() });
        self.blocked = Some(self.map.blocked(&self.content));
        self.story.started = false;
        // standing on cells counts as entering them
        self.story.last_cell = None;
        self.extra_changed = true;
    }

    /// Where a cell's floor is: floor `f` of building `b`.
    fn floor_pos(&self, c: (i32, i32), b: usize, f: usize) -> Vec3 {
        let level = self.map.buildings.get(b).map(|x| x.level as usize + f).unwrap_or(0) as f32;
        Vec3::new(c.0 as f32 + 0.5, level * self.level_height, c.1 as f32 + 0.5)
    }

    /// A building's flights of stairs on a floor: for each column of steps, the cell in
    /// front of its bottom step, the cell past its top (on the floor above), and the way up.
    pub(super) fn flights(&self, bi: usize, floor: usize) -> Vec<Flight> {
        let Some(b) = self.map.buildings.get(bi) else { return vec![] };
        let Some(fl) = b.floors.get(floor) else { return vec![] };
        let mut out = vec![];
        for p in &fl.props {
            let Some(prop) = self.content.props.get(&p.prop).filter(|q| q.category == "stairs") else { continue };
            let cells: Vec<(i32, i32)> = content::footprint_cells(p.at, p.turn, prop.footprint, 1.0).into_iter().map(|(x, z)| (x + b.at[0], z + b.at[1])).collect();
            // the steps rise toward the stairs' back (north), turned with them
            let up = match ((p.turn / 90.0).round() as i64).rem_euclid(4) {
                0 => (0, -1),
                1 => (1, 0),
                2 => (0, 1),
                _ => (-1, 0),
            };
            for &c in &cells {
                let front = (c.0 - up.0, c.1 - up.1);
                if cells.contains(&front) {
                    continue;
                }
                let mut top = c;
                while cells.contains(&(top.0 + up.0, top.1 + up.1)) {
                    top = (top.0 + up.0, top.1 + up.1);
                }
                out.push((front, (top.0 + up.0, top.1 + up.1), up));
            }
        }
        out
    }

    /// Whether the walker can stand on a cell of a building's floor above the ground (or
    /// its flat roof): on the building, off the stairwells and the floor's holes, clear of
    /// what blocks there.
    fn floor_free(&self, bi: usize, f: usize, c: (i32, i32)) -> bool {
        let Some(b) = self.map.buildings.get(bi) else { return false };
        let rel = (c.0 - b.at[0], c.1 - b.at[1]);
        if !b.has(rel.0, rel.1) || f > b.floors.len() || (f == b.floors.len() && !b.roof_is_flat()) {
            return false;
        }
        let stairs = |id: &str| self.content.props.get(id).filter(|p| p.category == "stairs").map(|p| p.footprint);
        if b.open_cells(f, &stairs).contains(&rel) {
            return false;
        }
        !self.stairs_or_blocking(bi, f, rel)
    }

    /// A blocking prop, or stairs (taken from their bottom, not walked over), on a cell
    /// (from the building's corner) of a building's floor.
    fn stairs_or_blocking(&self, bi: usize, f: usize, rel: (i32, i32)) -> bool {
        let Some(fl) = self.map.buildings.get(bi).and_then(|b| b.floors.get(f)) else { return false };
        fl.props.iter().any(|p| {
            self.content.props.get(&p.prop).map(|q| (q.blocks || q.category == "stairs") && content::footprint_cells(p.at, p.turn, q.footprint, 1.0).contains(&rel)).unwrap_or(false)
        })
    }

    /// Taking the stairs: from in front of a flight's bottom, the way it rises, up to the
    /// floor above; from past its top, the other way, back down. (where, on which floor,
    /// how many steps)
    #[allow(clippy::type_complexity)]
    fn stairs_step(&self, cell: (i32, i32), up: Option<(usize, usize)>, d: (i32, i32)) -> Option<((i32, i32), Option<(usize, usize)>, usize)> {
        let (bi, f) = match up {
            Some(x) => x,
            None => (self.map.buildings.iter().position(|b| b.map_cells().contains(&cell))?, 0),
        };
        let steps = |a: (i32, i32), b: (i32, i32)| ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as usize;
        for (front, exit, u) in self.flights(bi, f) {
            if cell == front && d == u && self.floor_free(bi, f + 1, exit) {
                return Some((exit, Some((bi, f + 1)), steps(front, exit)));
            }
        }
        if f >= 1 {
            for (front, exit, u) in self.flights(bi, f - 1) {
                if cell == exit && d == (-u.0, -u.1) {
                    let free = if f == 1 {
                        let b = &self.map.buildings[bi];
                        let blocked = self.map.index(front.0, front.1).map(|i| self.blocked.as_ref().map(|bl| bl[i]).unwrap_or(false)).unwrap_or(true);
                        !blocked && !self.stairs_or_blocking(bi, 0, (front.0 - b.at[0], front.1 - b.at[1]))
                    } else {
                        self.floor_free(bi, f - 1, front)
                    };
                    if free {
                        return Some((front, if f == 1 { None } else { Some((bi, f - 1)) }, steps(front, exit)));
                    }
                }
            }
        }
        None
    }

    /// The floor the walker is on, in the building it's in (0: the ground).
    pub(super) fn walker_floor(&self) -> usize {
        self.walker.as_ref().and_then(|w| w.upstairs).map(|(_, f)| f).unwrap_or(0)
    }

    /// Puts the walker on a floor of the building it stands in (a save loaded again).
    pub(super) fn set_walker_floor(&mut self, f: usize) {
        let Some(cell) = self.walker.as_ref().map(|w| w.cell) else { return };
        let Some(bi) = self.map.buildings.iter().position(|b| b.map_cells().contains(&cell)) else { return };
        if f == 0 || !self.floor_free(bi, f, cell) {
            return;
        }
        let pos = self.floor_pos(cell, bi, f);
        if let Some(w) = &mut self.walker {
            w.upstairs = Some((bi, f));
            w.from = pos;
            w.to = pos;
        }
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
        // a script has the floor, or the HUD has the game's buttons: the arrows do nothing
        let frozen = self.story.machine.is_some() || self.story.dialog.is_some() || self.hud_busy;
        // held, or tapped this frame (a tap can start and end between two frames)
        let held = |b: crate::input::Button| !typing && !frozen && crate::input::held(ctx, b);
        let (right, forward) = self.cam.ground_axes();
        let snap = |v: Vec3| if v.x.abs() > v.z.abs() { (v.x.signum() as i32, 0) } else { (0, v.z.signum() as i32) };
        let w = self.walker.as_ref().unwrap();
        let t = ((now - w.t0) / w.dur).clamp(0.0, 1.0) as f32;
        let pos = w.from.lerp(w.to, t);
        let upstairs = w.upstairs;
        let routed = w.route.front().map(|&n| (n.0 - w.cell.0, n.1 - w.cell.1));
        let dir = if let Some(d) = routed {
            if t >= 1.0 { Some(d) } else { None }
        } else if held(crate::input::Button::Up) {
            Some(snap(forward))
        } else if held(crate::input::Button::Down) {
            Some(snap(-forward))
        } else if held(crate::input::Button::Left) {
            Some(snap(-right))
        } else if held(crate::input::Button::Right) {
            Some(snap(right))
        } else {
            None
        };
        let stairs = match (dir, t >= 1.0) {
            (Some(d), true) => self.stairs_step(w.cell, upstairs, d).map(|s| (d, s)),
            _ => None,
        };
        if let Some((d, (to_cell, to_floor, steps))) = stairs {
            // up or down a flight of stairs
            let to = match to_floor {
                Some((b, f)) => self.floor_pos(to_cell, b, f),
                None => self.cell_pos(to_cell),
            };
            let w = self.walker.as_mut().unwrap();
            w.facing = facing_of(d);
            w.from = pos;
            w.to = to;
            w.t0 = now;
            w.dur = STEP_MS * steps as f64;
            w.cell = to_cell;
            w.upstairs = to_floor;
            w.route.pop_front();
        } else if let (Some(d), true, Some((bi, f))) = (dir, t >= 1.0, upstairs) {
            // on an upper floor: within the building
            let next = (w.cell.0 + d.0, w.cell.1 + d.1);
            let free = self.floor_free(bi, f, next) && self.actor_at(next).is_none();
            let to = self.floor_pos(next, bi, f);
            let w = self.walker.as_mut().unwrap();
            w.facing = facing_of(d);
            w.route.pop_front();
            if free {
                w.from = pos;
                w.to = to;
                w.t0 = now;
                w.dur = STEP_MS;
                w.cell = next;
            }
        } else if let (Some(d), true) = (dir, t >= 1.0) {
            let cell = w.cell;
            let next = (cell.0 + d.0, cell.1 + d.1);
            let facing = facing_of(d);
            if self.map.index(next.0, next.1).is_none() {
                // off the map: on into the piece next to it in the world, if there's one
                self.walker.as_mut().unwrap().facing = facing;
                if let Some((id, there)) = self.across(next) {
                    let level = self.map.level_at(cell.0, cell.1).unwrap_or(0);
                    match self.can_cross(&id, there, level) {
                        _ if self.dirty => self.status = format!("Save {} first, then walk on into {id}", self.map.id),
                        // it's here already: the open map now, the step going on into it
                        Some(true) => {
                            if let Some(delta) = self.step_into(&id) {
                                let to = self.cell_pos(there);
                                let w = self.walker.as_mut().unwrap();
                                w.from = pos - delta;
                                w.to = to;
                                w.t0 = now;
                                w.dur = STEP_MS;
                                w.cell = there;
                                self.cam.target = w.from;
                                self.extra_changed = true;
                                return;
                            }
                        }
                        // another level there, or something in the way
                        Some(false) => {}
                        // not here yet: opened (a pause)
                        None => {
                            self.arrive = Some((there, facing));
                            self.open(&id);
                            return;
                        }
                    }
                }
            }
            let someone = self.actor_at(next).is_some();
            // stairs on a building's ground floor are taken from their bottom, not walked over
            let on_stairs = self.map.buildings.iter().position(|b| b.map_cells().contains(&next)).map(|bi| {
                let b = &self.map.buildings[bi];
                self.stairs_or_blocking(bi, 0, (next.0 - b.at[0], next.1 - b.at[1]))
            });
            let free = (self.map.can_step(self.blocked.as_ref().unwrap(), cell, next) || routed.is_some() && self.map.index(next.0, next.1).is_some()) && !someone && !on_stairs.unwrap_or(false);
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
                w.dur = STEP_MS;
                w.cell = next;
            }
        }
        let (cell, was, upstairs) = { let w = self.walker.as_ref().unwrap(); (w.cell, w.inside, w.upstairs) };
        let level = self.map.level_at(cell.0, cell.1).unwrap_or(0);
        let inside = match upstairs {
            // on an upper floor; on the roof, outside again
            Some((b, f)) => (f < self.map.buildings.get(b).map(|x| x.floors.len()).unwrap_or(0)).then_some((b, f)),
            None => self.map.buildings.iter().position(|b| b.map_cells().contains(&cell)).map(|i| (i, level.saturating_sub(self.map.buildings[i].level) as usize)),
        };
        // the walls facing the camera: cut down while inside (they'd hide the room)
        let front = world::buildings::facing(self.cam.yaw, self.front);
        if inside != was {
            // the floors above, the roof and the walls facing the camera fade out on the way
            // in, back on the way out; up and down the stairs the cut just moves
            self.fade = match (was, inside) {
                (Some((a, _)), Some((b, _))) if a == b => None,
                (_, Some((i, f))) => Some(Fade { building: i, top: f, t0: now, entering: true, low: front }),
                (Some((i, f)), None) => Some(Fade { building: i, top: f, t0: now, entering: false, low: self.front }),
                _ => None,
            };
            self.front = front;
            self.wall_fade = None;
            self.walker.as_mut().unwrap().inside = inside;
            self.need_buildings = true;
        } else if front != self.front && self.fade.is_none() {
            // the camera turned: the walls it faces now go down, the others rise back (once
            // the building has faded in or out)
            if let Some((i, f)) = inside {
                self.wall_fade = Some(WallFade { building: i, floor: f, from: self.front, to: front, t0: now });
                self.need_buildings = true;
            }
            self.front = front;
        }
        if self.fade.as_ref().map(|f| now - f.t0 >= FADE_MS).unwrap_or(false) {
            self.fade = None;
            self.need_buildings = true;
        }
        if self.wall_fade.as_ref().map(|w| now - w.t0 >= FADE_MS).unwrap_or(false) {
            self.wall_fade = None;
            self.need_buildings = true;
        }
        crate::platform::set_hook("cutaway", &inside.map(|_| self.front.letters()).unwrap_or_default());
        crate::platform::set_hook("wallfade", if self.wall_fade.is_some() || self.fade.is_some() { "1" } else { "" });
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
        crate::platform::set_hook("floor", &upstairs.and_then(|(b, f)| self.map.buildings.get(b).map(|x| format!("{}:{f}", x.id))).unwrap_or_default());
    }

    /// How a building shows for the walker: cut above the floor they're on, the walls facing
    /// the camera there cut down; also while its upper part fades.
    pub(super) fn building_cut(&self, i: usize) -> Option<Cut> {
        if let Some(f) = self.fade.as_ref().filter(|f| f.building == i) {
            return Some(Cut { top: f.top, low: f.low });
        }
        let (_, floor) = self.walker.as_ref().and_then(|w| w.inside).filter(|(b, _)| *b == i)?;
        // while walls turn with the camera, those going and those coming back are both cut
        // down here: the fade has the rest of them
        let low = match &self.wall_fade {
            Some(w) if w.building == i && w.floor == floor => w.from.or(w.to),
            _ => self.front,
        };
        Some(Cut { top: floor, low })
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
    /// from them the building parts fading, as see-through as their fades say.
    pub(super) fn extra_batches(&self, now: f64) -> (Batches, Batches) {
        let moving = models::batches(&self.extra_mesh(now));
        let mut out = vec![];
        let mut faded = |mut part: Scene, alpha: f32| {
            for v in part.plain.vertices.iter_mut().chain(part.textured.values_mut().flat_map(|m| m.vertices.iter_mut())) {
                v.color[3] = alpha;
            }
            if !part.plain.indices.is_empty() {
                out.push((render::scene::WHITE.to_string(), gpu(&part.plain.vertices), part.plain.indices));
            }
            for (key, mesh) in part.textured {
                out.push((key, gpu(&mesh.vertices), mesh.indices));
            }
        };
        if let Some((f, b)) = self.fade.as_ref().and_then(|f| self.map.buildings.get(f.building).map(|b| (f, b))) {
            let k = ((now - f.t0) / FADE_MS).clamp(0.0, 1.0) as f32;
            faded(world::buildings::building_above(b, &self.content, self.level_height, f.top, f.low, &self.models), if f.entering { 1.0 - k } else { k });
        }
        if let Some((w, b)) = self.wall_fade.as_ref().and_then(|w| self.map.buildings.get(w.building).map(|b| (w, b))) {
            let k = ((now - w.t0) / FADE_MS).clamp(0.0, 1.0) as f32;
            let walls = |sides: Sides| world::buildings::walls_above_stub(b, &self.content, self.level_height, w.floor, sides, &self.models);
            // those the camera faces no more rise back; those it faces now go
            faded(walls(w.from.minus(w.to)), k);
            faded(walls(w.to.minus(w.from)), 1.0 - k);
        }
        (moving, out)
    }

    /// The things that move: the walker and the actors drawn as props.
    pub(super) fn extra_mesh(&self, now: f64) -> Scene {
        let mut out = self.actor_scene(now);
        out.append(&self.timeline_scene(now), glam::Mat4::IDENTITY);
        let Some(w) = &self.walker else { return out };
        let Some(prop) = self.content.props.get(CHARACTER) else { return out };
        let t = ((now - w.t0) / w.dur).clamp(0.0, 1.0) as f32;
        let mut pos = w.from.lerp(w.to, t);
        // a little bob while walking
        if t < 1.0 {
            pos.y += (t * std::f32::consts::PI).sin() * 0.06;
        }
        out.append(&prop_scene(prop, &self.models), placement(pos, w.facing, 1.0));
        out
    }
}
