//! The map's actors brought to life: each stands on its cell, wanders its area or walks its
//! path a cell at a time, turns, hides, and walks where a script sends it. Drawn as its
//! creature's picture, its own picture or its prop, with a shadow.

use std::collections::VecDeque;

use glam::{Vec2, Vec3};
use render::scene::{Billboard, Shape, ShapeKind};
use world::mesh::{placement, prop_scene, Scene};
use world::spawn::find_path;

use super::MapEditor;

/// An actor as it is right now.
pub(super) struct Live {
    pub id: String,
    pub cell: (i32, i32),
    pub from: Vec3,
    pub to: Vec3,
    pub t0: f64,
    /// degrees clockwise from south
    pub facing: f32,
    pub hidden: bool,
    /// the cells it's walking next
    pub route: VecDeque<(i32, i32)>,
    /// wandering: when to set off again; a path: the waypoint it's heading for
    pub next_at: f64,
    pub waypoint: usize,
    /// a script is moving it: its own ways wait
    pub scripted: bool,
    seed: u32,
}

impl Live {
    fn rand(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The facing (degrees clockwise from south) of a step.
pub(super) fn facing_of(d: (i32, i32)) -> f32 {
    match d {
        (0, 1) => 0.0,
        (-1, 0) => 90.0,
        (0, -1) => 180.0,
        (1, 0) => 270.0,
        _ => 0.0,
    }
}

/// The step a facing points along.
pub(super) fn step_of(facing: f32) -> (i32, i32) {
    match (facing.rem_euclid(360.0) / 90.0).round() as i32 % 4 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}

/// The facing that looks from one cell at another.
pub(super) fn facing_toward(from: (i32, i32), to: (i32, i32)) -> f32 {
    let (dx, dz) = (to.0 - from.0, to.1 - from.1);
    if dx.abs() > dz.abs() { facing_of((dx.signum(), 0)) } else { facing_of((0, dz.signum())) }
}

impl MapEditor {
    /// The actors as the map's records have them, standing at home.
    pub(super) fn rebuild_actors(&mut self) {
        self.live.clear();
        for (i, a) in self.map.actors.iter().enumerate() {
            let cell = (a.at[0], a.at[1]);
            let pos = self.cell_pos(cell);
            self.live.push(Live {
                id: a.id.clone(),
                cell,
                from: pos,
                to: pos,
                t0: 0.0,
                facing: a.facing as f32,
                hidden: false,
                route: VecDeque::new(),
                next_at: 0.0,
                waypoint: 0,
                scripted: false,
                seed: 17 + i as u32 * 7919,
            });
        }
        self.extra_changed = true;
    }

    pub(super) fn actor_index(&self, id: &str) -> Option<usize> {
        self.live.iter().position(|l| l.id == id)
    }

    /// The actor standing on (or stepping to) a cell.
    pub(super) fn actor_at(&self, cell: (i32, i32)) -> Option<usize> {
        self.live.iter().position(|l| !l.hidden && (l.cell == cell || l.route.front() == Some(&cell)))
    }

    pub(super) fn actor_pos(&self, i: usize, now: f64) -> Vec3 {
        let l = &self.live[i];
        let speed = self.map.actors.get(i).map(|a| a.speed).unwrap_or(2.0).max(0.1);
        let t = ((now - l.t0) * speed / 1000.0).clamp(0.0, 1.0) as f32;
        l.from.lerp(l.to, t)
    }

    pub(super) fn actor_moving(&self, i: usize, now: f64) -> bool {
        let l = &self.live[i];
        let speed = self.map.actors.get(i).map(|a| a.speed).unwrap_or(2.0).max(0.1);
        !l.route.is_empty() || (now - l.t0) * speed / 1000.0 < 1.0
    }

    /// Whether an actor may step onto a cell: free ground, nobody there.
    fn actor_can_step(&self, i: usize, from: (i32, i32), to: (i32, i32)) -> bool {
        let Some(blocked) = &self.blocked else { return false };
        if !self.map.can_step(blocked, from, to) {
            return false;
        }
        if self.walker.as_ref().map(|w| w.cell == to).unwrap_or(false) {
            return false;
        }
        !self.live.iter().enumerate().any(|(k, l)| k != i && !l.hidden && l.cell == to)
    }

    /// Sends an actor along these cells (the ways between them found on the ground).
    pub(super) fn route_actor(&mut self, i: usize, cells: &[[i32; 2]]) {
        if self.blocked.is_none() {
            self.blocked = Some(self.map.blocked(&self.content));
        }
        let blocked = self.blocked.clone().unwrap();
        let mut route = VecDeque::new();
        let mut from = self.live[i].cell;
        for c in cells {
            let to = (c[0], c[1]);
            let ok = |a: (i32, i32), b: (i32, i32)| self.map.can_step(&blocked, a, b);
            match find_path(&self.map, from, to, &ok) {
                Some(path) => route.extend(path.into_iter().skip_while(|&p| p == from)),
                None => route.push_back(to), // no way: it steps there anyway, a script asked
            }
            from = to;
        }
        let l = &mut self.live[i];
        l.route = route;
        l.scripted = true;
    }

    /// A frame of the actors' lives.
    pub(super) fn tick_actors(&mut self, now: f64) {
        if self.live.len() != self.map.actors.len() {
            self.rebuild_actors();
        }
        if self.blocked.is_none() {
            self.blocked = Some(self.map.blocked(&self.content));
        }
        let scripts_running = self.story.machine.is_some();
        for i in 0..self.live.len() {
            let a = self.map.actors[i].clone();
            let speed = a.speed.max(0.1);
            let stepping = (now - self.live[i].t0) * speed / 1000.0 < 1.0;
            if self.live[i].hidden || stepping {
                continue;
            }
            // where it's going next: its route, else its own ways
            if self.live[i].route.is_empty() {
                if self.live[i].scripted {
                    self.live[i].scripted = false;
                    self.live[i].next_at = now + 1500.0;
                }
                if scripts_running {
                    continue;
                }
                match a.moves.as_str() {
                    "wander" => {
                        if now < self.live[i].next_at {
                            continue;
                        }
                        // a few steps somewhere in its area, then a rest
                        let steps = 1 + (self.live[i].rand() * 3.0) as i32;
                        let mut cell = self.live[i].cell;
                        let mut route = VecDeque::new();
                        for _ in 0..steps {
                            let k = (self.live[i].rand() * 4.0) as i32;
                            let d = [(0, 1), (-1, 0), (0, -1), (1, 0)][k as usize % 4];
                            let next = (cell.0 + d.0, cell.1 + d.1);
                            let inside = next.0 >= a.area[0] && next.0 < a.area[0] + a.area[2] && next.1 >= a.area[1] && next.1 < a.area[1] + a.area[3];
                            if inside && self.actor_can_step(i, cell, next) {
                                route.push_back(next);
                                cell = next;
                            }
                        }
                        let rest = 1500.0 + self.live[i].rand() as f64 * 3000.0;
                        let l = &mut self.live[i];
                        l.route = route;
                        l.next_at = now + rest;
                        if l.route.is_empty() {
                            // a look around
                            l.facing = facing_of([(0, 1), (-1, 0), (0, -1), (1, 0)][(l.rand() * 4.0) as usize % 4]);
                            continue;
                        }
                    }
                    "path" if !a.path.is_empty() => {
                        let k = self.live[i].waypoint % a.path.len();
                        let target = (a.path[k][0], a.path[k][1]);
                        if target == self.live[i].cell {
                            self.live[i].waypoint = k + 1;
                            continue;
                        }
                        let blocked = self.blocked.clone().unwrap();
                        let from = self.live[i].cell;
                        let ok = |p: (i32, i32), q: (i32, i32)| self.map.can_step(&blocked, p, q);
                        match find_path(&self.map, from, target, &ok) {
                            Some(path) => {
                                self.live[i].route = path.into_iter().skip_while(|&p| p == from).collect();
                                self.live[i].waypoint = k + 1;
                            }
                            None => {
                                self.live[i].waypoint = k + 1;
                                self.live[i].next_at = now + 1000.0;
                            }
                        }
                    }
                    _ => continue,
                }
            }
            let Some(next) = self.live[i].route.front().copied() else { continue };
            let cell = self.live[i].cell;
            let d = (next.0 - cell.0, next.1 - cell.1);
            self.live[i].facing = facing_of(d);
            if self.actor_can_step(i, cell, next) || self.live[i].scripted && self.map.index(next.0, next.1).is_some() && !self.live.iter().enumerate().any(|(k, l)| k != i && l.cell == next) {
                let to = self.cell_pos(next);
                let pos = self.actor_pos(i, now);
                let l = &mut self.live[i];
                l.route.pop_front();
                l.from = pos;
                l.to = to;
                l.t0 = now;
                l.cell = next;
            } else if !self.live[i].scripted {
                // in the way: give up this stroll
                self.live[i].route.clear();
                self.live[i].next_at = now + 800.0;
            }
        }
        self.extra_changed = true;
    }

    /// The actors drawn as pictures (creatures and pictures), with their shadows; the ones
    /// drawn as props are in `actor_scene`.
    pub(super) fn actor_billboards(&mut self, tint: [f32; 4], now: f64, billboards: &mut Vec<Billboard>, shapes: &mut Vec<Shape>) {
        let (_, forward) = self.cam.ground_axes();
        let away = Vec2::new(forward.x, forward.z);
        let content = self.content.clone();
        for i in 0..self.live.len() {
            if self.live[i].hidden {
                continue;
            }
            let Some(a) = self.map.actors.get(i).cloned() else { continue };
            let pos = self.actor_pos(i, now);
            let step = step_of(self.live[i].facing);
            let heading = Vec2::new(step.0 as f32, step.1 as f32);
            let (path, uv, h) = match a.look.split_once(':') {
                Some(("creature", id)) => {
                    let Some(e) = content.creatures.entry(id) else { continue };
                    let c = &e.value;
                    let back = heading.dot(away) > 0.35;
                    let (p, b) = if back { (&c.sprites.back, c.sprites.back_box) } else { (&c.sprites.front, c.sprites.front_box) };
                    let Some(p) = p.as_ref().map(|p| e.space.asset(p)) else { continue };
                    let h = ((c.body.height_m as f32) * 1.1).clamp(0.45, 2.4);
                    (p, Some(b), h)
                }
                Some(("sprite", file)) => {
                    let space = content.maps.entry(&self.map.id).map(|e| e.space).unwrap_or(self.space);
                    (space.asset(file), None, 1.6)
                }
                _ => continue,
            };
            shapes.push(Shape { kind: ShapeKind::Disc { radius: 0.3 * h.max(0.6), squash: 0.6 }, center: pos + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.28], additive: false });
            match self.sprites.get(&path) {
                Some(Some((w, ht))) => {
                    let (w, ht) = (*w as f32, *ht as f32);
                    let b = uv.unwrap_or([0, 0, w as i32, ht as i32]);
                    let bw = ((b[2] - b[0]).max(1)) as f32;
                    let bh = ((b[3] - b[1]).max(1)) as f32;
                    billboards.push(Billboard {
                        texture: path,
                        pos,
                        size: Vec2::new(h * bw / bh, h),
                        offset: Vec2::ZERO,
                        uv: [b[0] as f32 / w, b[1] as f32 / ht, b[2] as f32 / w, b[3] as f32 / ht],
                        color: tint,
                        casts_shadow: false,
                    });
                }
                Some(None) => {}
                None => {
                    if !self.sprites_wanted.contains(&path) {
                        self.sprites_wanted.push(path);
                    }
                }
            }
        }
    }

    /// The actors drawn as props, where they stand and turned the way they face.
    pub(super) fn actor_scene(&self, now: f64) -> Scene {
        let mut out = Scene::default();
        for (i, l) in self.live.iter().enumerate() {
            if l.hidden {
                continue;
            }
            let Some(id) = self.map.actors.get(i).and_then(|a| a.look.strip_prefix("prop:")) else { continue };
            let Some(prop) = self.content.props.get(id) else { continue };
            let mut pos = self.actor_pos(i, now);
            let speed = self.map.actors[i].speed.max(0.1);
            let t = ((now - l.t0) * speed / 1000.0).clamp(0.0, 1.0) as f32;
            if t < 1.0 {
                pos.y += (t * std::f32::consts::PI).sin() * 0.06;
            }
            out.append(&prop_scene(prop, &self.models), placement(pos, l.facing, 1.0));
        }
        out
    }
}

