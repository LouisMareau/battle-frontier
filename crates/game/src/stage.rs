//! The battle staged on the map: creatures as billboards on two lines, the trainers behind
//! them, the animations and effects, the camera and the light of the time of day.
//! Produces the renderer's `Frame` each frame and projects points for the HUD.

use std::collections::BTreeMap;

use glam::{Mat4, Vec2, Vec3, Vec4};
use render::map::Spot;
use render::scene::{Billboard, Frame, Glow, Lighting, Shape, ShapeKind};
use rules::battle::{Side, Uid, WeatherKind};
use rules::config::Period;

/// world units per sprite pixel
pub const PX: f32 = 1.0 / 27.0;

pub fn srgb_to_linear(hex: u32) -> Vec3 {
    let c = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    Vec3::new(c((hex >> 16) & 255), c((hex >> 8) & 255), c(hex & 255))
}

/// A type's colour (from the content, sRGB hex) as a linear colour for the renderer.
pub fn type_color(dex: &rules::dex::Dex, ty: u8, alpha: f32) -> [f32; 4] {
    let c = srgb_to_linear(dex.type_color(ty));
    [c.x, c.y, c.z, alpha]
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A creature on the field.
pub struct Vis {
    pub texture: String,
    pub bbox: [i32; 4],
    pub lift: f32,
    pub float: bool,
    pub phase: f32,
    pub home: Vec3,
    pub pos: Vec3,
    pub placed: bool,
    pub spawn: Option<f64>,
    pub leaving: Option<(f64, bool)>,
    pub flash_until: f64,
    pub flash_power: f32,
    pub shake_from: f64,
    pub pulse: Option<(f64, f64)>,
    pub aura: Option<[f32; 4]>,
    pub motion: Option<Motion>,
}

pub enum Motion {
    /// wind back, dash to `reach`, hold, walk back home
    Lunge { t0: f64, from: Vec3, reach: Vec3, ms: f64 },
    /// a slide (layout changes)
    Slide { t0: f64, from: Vec3, ms: f64 },
    /// the call for help: a hop
    Hop { t0: f64, ms: f64 },
    /// multi-hit: stay near the target until `until`, then walk back
    Stay { t0: f64, near: Vec3, until: f64 },
}

pub enum Effect {
    Orb { t0: f64, ms: f64, from: Vec3, to: Vec3, color: [f32; 4], size: f32, arc: f32 },
    Burst { t0: f64, ms: f64, origin: Vec3, vel: Vec<Vec3>, color: [f32; 4] },
    Gather { t0: f64, ms: f64, center: Vec3, from: Vec<Vec3>, color: [f32; 4] },
    Wave { t0: f64, ms: f64, center: Vec3, color: [f32; 4] },
}

pub struct Person {
    pub texture: String,
    pub pos: Vec3,
}

pub struct Stage {
    pub spot: Spot,
    pub line_gap: f32,
    pub vis: BTreeMap<Uid, Vis>,
    pub people: Vec<Person>,
    pub effects: Vec<Effect>,
    pub period: Period,
    pub weather: Option<WeatherKind>,
    pub targets: Vec<(Uid, bool, [f32; 4])>,
    cam_focus: Vec3,
    cam_goal: Vec3,
    pub view_proj: Mat4,
    pub viewport: egui::Rect,
    rain: Vec<Vec3>,
    seed: u32,
}

struct PeriodLight {
    sky: u32,
    sun: u32,
    sun_i: f32,
    amb: u32,
    amb_i: f32,
    tint: u32,
    sun_pos: Vec3,
}

fn period_light(p: Period) -> PeriodLight {
    match p {
        Period::Morning => PeriodLight { sky: 0xf3c6a0, sun: 0xffd2a0, sun_i: 2.2, amb: 0xd8c0b0, amb_i: 1.2, tint: 0xffe6d0, sun_pos: Vec3::new(-40.0, 25.0, 30.0) },
        Period::Day => PeriodLight { sky: 0x9fd2ff, sun: 0xfff2dc, sun_i: 2.6, amb: 0xb8c8e0, amb_i: 1.4, tint: 0xffffff, sun_pos: Vec3::new(-30.0, 60.0, 40.0) },
        Period::Night => PeriodLight { sky: 0x0d1630, sun: 0x8fa8ff, sun_i: 0.9, amb: 0x34406a, amb_i: 1.0, tint: 0x8c9ccc, sun_pos: Vec3::new(30.0, 50.0, -20.0) },
    }
}

impl Stage {
    pub fn new(spot: Spot, period: Period) -> Stage {
        let line_gap = 5.0f32.min(spot.depth as f32 - 1.0);
        let focus = Vec3::new(spot.x, 0.7, spot.z - 0.4);
        Stage {
            spot, line_gap, vis: BTreeMap::new(), people: vec![], effects: vec![], period, weather: None, targets: vec![],
            cam_focus: focus, cam_goal: focus, view_proj: Mat4::IDENTITY, viewport: egui::Rect::NOTHING, rain: vec![], seed: 12345,
        }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10000) as f32 / 10000.0
    }

    /// How far behind their line the trainers stand.
    fn behind(&self) -> f32 {
        ((self.spot.depth as f32) / 2.0 - self.line_gap / 2.0 + 0.8).clamp(1.2, 2.4)
    }

    /// The trainers (texture keys): the player behind their creatures, the opponent behind theirs.
    pub fn add_people(&mut self, player: Option<String>, foe: Option<String>) {
        let b = self.behind();
        if let Some(t) = player {
            self.people.push(Person { texture: t, pos: Vec3::new(self.spot.x + 0.6, 0.0, self.spot.z + self.line_gap / 2.0 + b) });
        }
        if let Some(t) = foe {
            self.people.push(Person { texture: t, pos: Vec3::new(self.spot.x, 0.0, self.spot.z - self.line_gap / 2.0 - b - 0.2) });
        }
    }

    pub fn player_spot(&self) -> Vec3 {
        Vec3::new(self.spot.x + 0.6, 1.2, self.spot.z + self.line_gap / 2.0 + 2.0)
    }

    pub fn slot_position(&self, side: Side, index: usize, count: usize) -> Vec3 {
        let gap = if count <= 3 { 2.2 } else { 1.75 };
        let x = self.spot.x + (index as f32 - (count as f32 - 1.0) / 2.0) * gap;
        let z = self.spot.z + if side == Side::Player { self.line_gap / 2.0 } else { -self.line_gap / 2.0 };
        Vec3::new(x, 0.0, z)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_battler(&mut self, uid: Uid, side: Side, species: &str, front_box: [i32; 4], back_box: [i32; 4], elevation: u16, now: f64) {
        let back = side == Side::Player;
        let lift = if back { 0.0 } else { elevation as f32 * PX * 1.6 };
        let phase = self.rand() * 6.0;
        self.vis.insert(uid, Vis {
            texture: format!("{}/{species}", if back { "back" } else { "front" }),
            bbox: if back { back_box } else { front_box },
            lift, float: lift > 0.0, phase, home: Vec3::ZERO, pos: Vec3::ZERO, placed: false, spawn: Some(now),
            leaving: None, flash_until: 0.0, flash_power: 4.0, shake_from: -1e9, pulse: None, aura: None, motion: None,
        });
    }

    pub fn remove_battler(&mut self, uid: Uid, faint: bool, now: f64) {
        if let Some(v) = self.vis.get_mut(&uid) {
            v.leaving = Some((now, faint));
        }
    }

    /// Where each battler stands, from the battle's fields (slides when it changes).
    pub fn layout(&mut self, fields: [&[Uid]; 2], now: f64) {
        for (si, side) in [Side::Player, Side::Foe].into_iter().enumerate() {
            let f = fields[si];
            for (i, &u) in f.iter().enumerate() {
                let goal = self.slot_position(side, i, f.len());
                let Some(v) = self.vis.get_mut(&u) else { continue };
                if v.leaving.is_some() {
                    continue;
                }
                if !v.placed {
                    v.home = goal;
                    v.pos = goal;
                    v.placed = true;
                } else if v.home.distance(goal) > 0.01 {
                    let from = v.pos;
                    v.home = goal;
                    if v.motion.is_none() {
                        v.motion = Some(Motion::Slide { t0: now, from, ms: 300.0 });
                    }
                }
            }
        }
    }

    pub fn height(&self, uid: Uid) -> f32 {
        self.vis.get(&uid).map(|v| ((v.bbox[3] - v.bbox[1]) as f32 * PX).max(1.2) + v.lift).unwrap_or(1.5)
    }

    pub fn center_of(&self, uid: Uid) -> Vec3 {
        match self.vis.get(&uid) {
            Some(v) => v.pos + Vec3::Y * (v.lift + self.height(uid) * 0.45),
            None => Vec3::new(self.spot.x, 1.0, self.spot.z),
        }
    }

    pub fn focus_on(&mut self, uids: &[Uid]) {
        let xs: Vec<f32> = uids.iter().filter_map(|u| self.vis.get(u)).map(|v| v.home.x).collect();
        let x = if xs.is_empty() { self.spot.x } else { xs.iter().sum::<f32>() / xs.len() as f32 };
        self.cam_goal = Vec3::new(self.spot.x + (x - self.spot.x) * 0.35, 0.7, self.spot.z - 0.4);
    }

    pub fn flash(&mut self, uid: Uid, now: f64, power: f32) {
        if let Some(v) = self.vis.get_mut(&uid) {
            v.flash_until = now + 90.0;
            v.flash_power = power;
            v.shake_from = now;
        }
    }

    pub fn aura(&mut self, uid: Uid, color: Option<[f32; 4]>) {
        if let Some(v) = self.vis.get_mut(&uid) {
            v.aura = color;
        }
    }

    pub fn lunge(&mut self, uid: Uid, dest: Vec3, impact_in: f64, now: f64) {
        let Some(v) = self.vis.get_mut(&uid) else { return };
        let toward = dest - v.home;
        let reach = if toward.length() > 0.5 { v.home + toward * 0.72 } else { v.home };
        v.motion = Some(Motion::Lunge { t0: now, from: v.home, reach, ms: impact_in });
    }

    pub fn stay_near(&mut self, uid: Uid, dest: Vec3, until: f64, now: f64) {
        let Some(v) = self.vis.get_mut(&uid) else { return };
        let near = v.home.lerp(dest, 0.7);
        v.motion = Some(Motion::Stay { t0: now, near, until });
    }

    pub fn hop(&mut self, uid: Uid, now: f64) {
        if let Some(v) = self.vis.get_mut(&uid) {
            v.motion = Some(Motion::Hop { t0: now, ms: 400.0 });
        }
    }

    pub fn pulse(&mut self, uid: Uid, now: f64, ms: f64) {
        if let Some(v) = self.vis.get_mut(&uid) {
            v.pulse = Some((now, ms));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn orb(&mut self, from: Vec3, to: Vec3, color: [f32; 4], now: f64, ms: f64, size: f32, arc: f32) {
        self.effects.push(Effect::Orb { t0: now, ms, from, to, color, size, arc });
    }

    pub fn burst(&mut self, at: Vec3, color: [f32; 4], n: usize, spread: f32, now: f64) {
        let vel = (0..n).map(|_| Vec3::new((self.rand() - 0.5) * spread, self.rand() * spread, (self.rand() - 0.5) * spread)).collect();
        self.effects.push(Effect::Burst { t0: now, ms: 380.0, origin: at, vel, color });
    }

    pub fn gather(&mut self, uid: Uid, color: [f32; 4], now: f64, ms: f64) {
        let c = self.center_of(uid);
        let from = (0..14)
            .map(|_| {
                let a = self.rand() * std::f32::consts::TAU;
                Vec3::new(c.x + a.cos() * 1.8, c.y + (self.rand() - 0.3) * 1.6, c.z + a.sin() * 1.8)
            })
            .collect();
        self.effects.push(Effect::Gather { t0: now, ms, center: c, from, color });
    }

    pub fn wave(&mut self, uid: Uid, color: [f32; 4], now: f64, ms: f64) {
        let center = self.vis.get(&uid).map(|v| v.home).unwrap_or(Vec3::new(self.spot.x, 0.0, self.spot.z));
        self.effects.push(Effect::Wave { t0: now, ms, center, color });
    }

    /// Advances motions and effects; drops what has finished.
    pub fn update(&mut self, now: f64, dt: f32) {
        self.cam_focus = self.cam_focus.lerp(self.cam_goal, (dt * 3.0).min(1.0));
        let mut gone = vec![];
        for (&u, v) in self.vis.iter_mut() {
            if let Some((t0, faint)) = v.leaving {
                if now - t0 > if faint { 450.0 } else { 220.0 } {
                    gone.push(u);
                }
            }
            let home = v.home;
            let (pos, done) = match &v.motion {
                None => (home, false),
                Some(Motion::Slide { t0, from, ms }) => {
                    let t = ((now - t0) / ms).clamp(0.0, 1.0) as f32;
                    (from.lerp(home, t), t >= 1.0)
                }
                Some(Motion::Lunge { t0, from, reach, ms }) => {
                    let e = now - t0;
                    let wind = from.lerp(*reach, -0.08 / 0.72);
                    let (a, b) = (ms * 0.35, ms * 0.65);
                    if e < a {
                        (from.lerp(wind, (e / a) as f32), false)
                    } else if e < a + b {
                        let t = ((e - a) / b) as f32;
                        (wind.lerp(*reach, t * t), false)
                    } else if e < ms + 120.0 {
                        (*reach, false)
                    } else {
                        let t = ((e - ms - 120.0) / 300.0).min(1.0) as f32;
                        (reach.lerp(home, t), t >= 1.0)
                    }
                }
                Some(Motion::Hop { t0, ms }) => {
                    let t = ((now - t0) / ms).min(1.0) as f32;
                    (home + Vec3::Y * (t * std::f32::consts::TAU).sin().abs() * 0.3, t >= 1.0)
                }
                Some(Motion::Stay { t0, near, until }) => {
                    let e = now - t0;
                    if e < 450.0 {
                        (home.lerp(*near, (e / 450.0) as f32), false)
                    } else if now < *until {
                        (*near, false)
                    } else {
                        let t = ((now - until) / 300.0).min(1.0) as f32;
                        (near.lerp(home, t), t >= 1.0)
                    }
                }
            };
            v.pos = pos;
            if done {
                v.motion = None;
            }
        }
        for u in gone {
            self.vis.remove(&u);
        }
        self.effects.retain(|e| {
            let (t0, ms) = match e {
                Effect::Orb { t0, ms, .. } | Effect::Burst { t0, ms, .. } | Effect::Gather { t0, ms, .. } | Effect::Wave { t0, ms, .. } => (*t0, *ms),
            };
            now - t0 < ms
        });
        if self.weather == Some(WeatherKind::Rain) {
            if self.rain.is_empty() {
                for _ in 0..700 {
                    let p = Vec3::new(self.spot.x + (self.rand() - 0.5) * 30.0, self.rand() * 14.0, self.spot.z + (self.rand() - 0.5) * 24.0);
                    self.rain.push(p);
                }
            }
            for p in self.rain.iter_mut() {
                p.y -= dt * 22.0;
                if p.y < 0.0 {
                    p.y += 14.0;
                }
            }
        } else {
            self.rain.clear();
        }
    }

    pub fn camera(&self, now: f64, aspect: f32) -> (Mat4, Vec3) {
        let f = self.cam_focus;
        let sway = ((now / 2400.0).sin() * 0.25) as f32;
        // a three-quarter view from behind the player's right
        let eye = Vec3::new(f.x + 5.2 + sway, 9.6, f.z + 15.5);
        let view = Mat4::look_at_rh(eye, f, Vec3::Y);
        let proj = Mat4::perspective_rh(30f32.to_radians(), aspect.max(0.1), 0.1, 200.0);
        (proj * view, eye)
    }

    fn lighting(&self) -> (Lighting, Vec3) {
        period_lighting(self.period, self.weather, Vec3::new(self.spot.x, 0.0, self.spot.z), 22.0)
    }

    pub fn frame(&mut self, now: f64, viewport: egui::Rect) -> Frame {
        self.viewport = viewport;
        let (view_proj, eye) = self.camera(now, viewport.width() / viewport.height().max(1.0));
        self.view_proj = view_proj;
        let (light, tint) = self.lighting();
        let mut billboards = vec![];
        let mut shapes = vec![];
        let mut glows = vec![];
        let tint4 = [tint.x, tint.y, tint.z, 1.0];
        for person in &self.people {
            billboards.push(Billboard {
                texture: person.texture.clone(), pos: person.pos, size: Vec2::new(1.0, 2.0), offset: Vec2::ZERO,
                uv: [0.0, 0.0, 1.0, 1.0], color: tint4, casts_shadow: true,
            });
            shapes.push(Shape { kind: ShapeKind::Disc { radius: 0.45, squash: 0.6 }, center: person.pos + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.3], additive: false });
        }
        for (&u, v) in &self.vis {
            let [x0, _, x1, y1] = v.bbox;
            let mut scale = 1.0f32;
            if let Some(s) = v.spawn {
                scale = (((now - s) / 260.0) as f32).clamp(0.01, 1.0);
            }
            let mut lift = v.lift;
            if v.float {
                lift += ((now / 420.0) as f32 + v.phase).sin() * 0.08;
            }
            let mut fade = 1.0;
            if let Some((t0, faint)) = v.leaving {
                let t = ((now - t0) / if faint { 450.0 } else { 220.0 }).min(1.0) as f32;
                if faint {
                    lift -= t * 1.4;
                    fade = 1.0 - t;
                } else {
                    scale *= 1.0 - t * 0.99;
                }
            }
            if let Some((t0, ms)) = v.pulse {
                let t = ((now - t0) / ms) as f32;
                if (0.0..=1.0).contains(&t) {
                    scale *= 1.0 + (t * std::f32::consts::PI).sin() * 0.12;
                }
            }
            let shake = {
                let t = ((now - v.shake_from) / 260.0) as f32;
                if (0.0..1.0).contains(&t) { (t * std::f32::consts::PI * 6.0).sin() * 0.12 * (1.0 - t) } else { 0.0 }
            };
            let flash = if now < v.flash_until { v.flash_power } else { 1.0 };
            let size = Vec2::splat(64.0 * PX * scale);
            let offset = Vec2::new((-((x0 + x1) as f32 / 2.0 - 32.0) * PX) * scale + shake, lift - (64 - y1) as f32 * PX * scale);
            billboards.push(Billboard {
                texture: v.texture.clone(), pos: v.pos, size, offset, uv: [0.0, 0.0, 1.0, 1.0],
                color: [tint.x * flash, tint.y * flash, tint.z * flash, 1.0], casts_shadow: true,
            });
            let r = (((x1 - x0) as f32) * PX * 0.45).max(0.35) * scale;
            shapes.push(Shape { kind: ShapeKind::Disc { radius: r, squash: 0.6 }, center: v.pos + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.3 * fade], additive: false });
            if let Some(mut color) = v.aura {
                let k = 1.0 + ((now / 180.0) as f32).sin() * 0.12;
                color[3] = 0.7;
                shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.55 * k, outer: 0.85 * k }, center: v.pos + Vec3::Y * 0.06, color, additive: true });
            }
            let _ = u;
        }
        for &(u, center, color) in &self.targets {
            if let Some(v) = self.vis.get(&u) {
                let c = if center { color } else { [1.0, 1.0, 1.0, 0.6] };
                shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.62, outer: 0.8 }, center: v.pos + Vec3::Y * 0.05, color: c, additive: false });
            }
        }
        for e in &self.effects {
            match e {
                Effect::Orb { t0, ms, from, to, color, size, arc } => {
                    if now < *t0 {
                        continue; // launched a moment later
                    }
                    let t = (((now - t0) / ms) as f32).clamp(0.0, 1.0);
                    let p = from.lerp(*to, t) + Vec3::Y * (t * std::f32::consts::PI).sin() * arc;
                    glows.push(Glow { pos: p, size: *size, color: *color });
                }
                Effect::Burst { t0, ms, origin, vel, color } => {
                    let t = (((now - t0) / ms) as f32).clamp(0.0, 1.0);
                    for v in vel {
                        let mut c = *color;
                        c[3] *= 1.0 - t;
                        glows.push(Glow { pos: *origin + *v * t * 1.37, size: 0.35, color: c });
                    }
                }
                Effect::Gather { t0, ms, center, from, color } => {
                    let t = (((now - t0) / ms) as f32).clamp(0.0, 1.0);
                    for f in from {
                        glows.push(Glow { pos: f.lerp(*center, smoothstep(t)), size: 0.4, color: *color });
                    }
                }
                Effect::Wave { t0, ms, center, color } => {
                    let t = (((now - t0) / ms) as f32).clamp(0.0, 1.0);
                    let s = 1.0 + t * 5.5;
                    let mut c = *color;
                    c[3] = 0.85 * (1.0 - t * 0.6);
                    shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.8 * s, outer: 1.2 * s }, center: *center + Vec3::Y * 0.08, color: c, additive: true });
                }
            }
        }
        let streaks = self.rain.iter().map(|&p| (p, p + Vec3::new(-0.05, -0.5, 0.05))).collect();
        Frame { view_proj, camera_pos: eye, light, shadows: true, billboards, shapes, glows, streaks, time: (now / 1000.0) as f32, grid: false }
    }

    /// Screen position (egui points) of a point in the world; None behind the camera.
    pub fn project(&self, p: Vec3) -> Option<egui::Pos2> {
        let c = self.view_proj * Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 {
            return None;
        }
        let n = c.truncate() / c.w;
        let r = self.viewport;
        Some(egui::pos2(r.left() + (n.x * 0.5 + 0.5) * r.width(), r.top() + (0.5 - n.y * 0.5) * r.height()))
    }

    /// Over a battler's head (dy: more or less), or at its feet.
    pub fn head(&self, uid: Uid, dy: f32) -> Option<egui::Pos2> {
        let v = self.vis.get(&uid)?;
        self.project(v.pos + Vec3::Y * (self.height(uid) + 0.25 + dy))
    }

    pub fn feet(&self, uid: Uid) -> Option<egui::Pos2> {
        let v = self.vis.get(&uid)?;
        self.project(v.pos + Vec3::Z * 0.55)
    }
}

/// The light of a time of day and weather, with the sun's shadow covering `radius` around
/// `target` (and fog starting past it); also the tint for pictures.
pub fn period_lighting(period: Period, weather: Option<WeatherKind>, target: Vec3, radius: f32) -> (Lighting, Vec3) {
    let p = period_light(period);
    let pi = std::f32::consts::PI;
    let mut sky = srgb_to_linear(p.sky);
    let (mut sun_i, mut amb_i) = (p.sun_i, p.amb_i);
    match weather {
        Some(WeatherKind::Rain) => {
            sky = sky.lerp(srgb_to_linear(0x56606e), 0.6);
            sun_i *= 0.45;
            amb_i *= 0.85;
        }
        Some(WeatherKind::Sun) => sun_i *= 1.35,
        None => {}
    }
    let hemi_i = if period == Period::Night { 0.2 } else { 0.5 };
    let far = (radius / 22.0).max(1.0);
    let sun_pos = target + p.sun_pos * far;
    let light_view = Mat4::look_at_rh(sun_pos, target, Vec3::Y);
    let light_proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 1.0, 200.0 * far);
    (
        Lighting {
            sun_dir: p.sun_pos.normalize(),
            sun: srgb_to_linear(p.sun) * sun_i / pi,
            ambient: srgb_to_linear(p.amb) * amb_i / pi,
            hemi_sky: srgb_to_linear(0xcfe6ff) * hemi_i / pi,
            hemi_ground: srgb_to_linear(0x6b8a5a) * hemi_i / pi,
            sky,
            fog_near: 30.0 * far,
            fog_far: 70.0 * far,
            light_view_proj: light_proj * light_view,
        },
        srgb_to_linear(p.tint),
    )
}
