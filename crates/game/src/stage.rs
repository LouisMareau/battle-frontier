//! The battle staged on the map: creatures as billboards on two lines, the trainers behind
//! them, the animations and effects, the camera and the light of the time of day.
//! Produces the renderer's `Frame` each frame and projects points for the HUD.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};
use render::map::Spot;
use render::scene::{Billboard, Frame, Glow, Lighting, Shape, ShapeKind, SpotLight, MAX_LIGHTS};
use rules::battle::{Side, Uid, WeatherKind};

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
    pub side: Side,
    /// a light it carries
    pub light: Option<content::Light>,
    /// a sound it makes as it stands there
    pub sound: Option<content::Sound>,
    /// drawn with the toon look
    pub toon: bool,
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
    /// taken into an IDP: when it went in, and when it comes back out (none: caught)
    pub pocketed: Option<(f64, Option<f64>)>,
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
    /// an IDP landed on a creature: the net spreading from it as the creature goes into the
    /// pocket, then the pocket's seams lighting up as the creature strains against it
    /// (`struggle`: how hard, 0 to 1), going dark once it gives up (caught) or bursting as it
    /// forces its way out
    Pocket { t0: f64, at: Vec3, struggle: f32, caught: bool },
}

/// An IDP's whole: the net (ms from its landing), when it's known (the light dark or the
/// burst), and when it's over.
const POCKET_NET_MS: f64 = 350.0;
const POCKET_KNOWN_MS: f64 = 1650.0;
const POCKET_MS: f64 = 1950.0;

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
    /// the time of day it plays at (hours), for its light
    pub hours: f32,
    pub weather: Option<WeatherKind>,
    pub targets: Vec<(Uid, bool, [f32; 4])>,
    cam_focus: Vec3,
    cam_goal: Vec3,
    pub view_proj: Mat4,
    pub viewport: egui::Rect,
    rain: Vec<Vec3>,
    seed: u32,
    /// how what's toon looks (the content's world settings)
    pub toon: render::scene::Toon,
    /// an ability's timeline playing out
    pub playing: Option<Playing>,
    /// the timeline's sounds that came due (asset paths, volume): taken by the app
    pub sounds: Vec<(String, f32)>,
    /// the content, for the fx records a timeline names
    pub content: Option<Arc<content::Content>>,
    /// the old handhelds' pixels and colours (the Options')
    pub retro: render::scene::Retro,
    /// the world's wind (the clouds drift with it)
    pub wind: render::scene::Wind,
    /// the picture's light (the content's world settings)
    pub radiance: render::scene::Radiance,
}

/// An ability's timeline playing out on the stage, from `t0` (ms).
pub struct Playing {
    pub key: String,
    pub t0: f64,
    pub timeline: content::Timeline,
    pub user: Uid,
    pub targets: Vec<Uid>,
    /// the sound clips played
    played: BTreeSet<usize>,
}

/// The world settings' wind, for the renderer: the way it blows on the ground, how strong,
/// how gusty.
pub fn wind_look(w: &content::Wind) -> render::scene::Wind {
    let [x, z] = w.way();
    render::scene::Wind { way: glam::Vec2::new(x as f32, z as f32), strength: w.strength.clamp(0.0, 1.0) as f32, gusts: w.gusts.clamp(0.0, 1.0) as f32 }
}

/// The world settings' light, for the renderer.
pub fn radiance_look(l: &content::Radiance) -> render::scene::Radiance {
    render::scene::Radiance {
        exposure: l.exposure.clamp(0.0, 16.0) as f32,
        filmic: l.tone == "filmic",
        bloom: l.bloom.clamp(0.0, 4.0) as f32,
        threshold: l.threshold.clamp(0.0, 64.0) as f32,
        gi: l.gi.clamp(0.0, 8.0) as f32,
        gi_range: l.gi_range.clamp(4.0, 256.0) as f32,
    }
}

/// The world settings' toon look, for the renderer.
pub fn toon_look(t: &content::Toon) -> render::scene::Toon {
    let rgb = |hex: &str| Vec3::from_slice(&world::color(hex)[..3]);
    render::scene::Toon {
        bands: t.bands.clamp(1, 5) as u32,
        softness: t.softness as f32,
        shadow_color: rgb(&t.shadow_color),
        shadow: t.shadow as f32,
        rim: t.rim as f32,
        rim_width: t.rim_width as f32,
        glint: t.glint as f32,
        glint_size: t.glint_size as f32,
        ink: t.ink,
        ink_color: rgb(&t.ink_color),
        ink_width: t.ink_width as f32,
        ink_opacity: t.ink_opacity as f32,
        depth_step: t.depth_step as f32,
        crease: t.crease as f32,
        color_steps: if t.color_steps >= 2 { t.color_steps as u32 } else { 0 },
        color_softness: t.color_softness as f32,
        cloud_shadow: t.cloud_shadow as f32,
        cloud_size: t.cloud_size as f32,
        cloud_speed: t.cloud_speed as f32,
        cloud_cover: t.cloud_cover as f32,
    }
}

/// The light at one moment of the day: the sky, the sun (the moon by night) and where it
/// shines from, the ambient light, the sky's and the ground's bounce, and the pictures' tint.
/// Colours are sRGB.
#[derive(Clone, Copy)]
struct DayKey {
    /// hours, 0 to 24
    at: f32,
    sky: u32,
    sun: u32,
    sun_i: f32,
    amb: u32,
    amb_i: f32,
    tint: u32,
    sun_pos: Vec3,
    hemi: f32,
}

const NIGHT: DayKey = DayKey { at: 0.0, sky: 0x0d1630, sun: 0x8fa8ff, sun_i: 0.9, amb: 0x34406a, amb_i: 1.0, tint: 0x8c9ccc, sun_pos: Vec3::new(30.0, 50.0, -20.0), hemi: 0.2 };

/// The day, key by key: night until dawn, the sun rising in the east, crossing the sky,
/// setting in the west, then the dusk's last light and the night again. In between, blended.
const DAY: [DayKey; 10] = [
    NIGHT,
    DayKey { at: 4.5, ..NIGHT },
    // dawn: pink, the sun low in the east
    DayKey { at: 6.0, sky: 0xe8a890, sun: 0xffb98a, sun_i: 1.6, amb: 0xc0a8b8, amb_i: 1.0, tint: 0xffd8c8, sun_pos: Vec3::new(-50.0, 12.0, 25.0), hemi: 0.4 },
    // morning
    DayKey { at: 8.0, sky: 0xf3c6a0, sun: 0xffd2a0, sun_i: 2.2, amb: 0xd8c0b0, amb_i: 1.2, tint: 0xffe6d0, sun_pos: Vec3::new(-40.0, 25.0, 30.0), hemi: 0.5 },
    // the day, the sun from the east, then from the west
    DayKey { at: 11.0, sky: 0x9fd2ff, sun: 0xfff2dc, sun_i: 2.6, amb: 0xb8c8e0, amb_i: 1.4, tint: 0xffffff, sun_pos: Vec3::new(-20.0, 62.0, 40.0), hemi: 0.5 },
    DayKey { at: 16.0, sky: 0x9fd2ff, sun: 0xfff2dc, sun_i: 2.6, amb: 0xb8c8e0, amb_i: 1.4, tint: 0xffffff, sun_pos: Vec3::new(25.0, 55.0, 40.0), hemi: 0.5 },
    // dusk: orange, the sun low in the west
    DayKey { at: 18.5, sky: 0xf0a070, sun: 0xffa060, sun_i: 1.8, amb: 0xc8a0a0, amb_i: 1.1, tint: 0xffd0b0, sun_pos: Vec3::new(45.0, 14.0, 25.0), hemi: 0.4 },
    // the twilight's blue
    DayKey { at: 20.0, sky: 0x3a3560, sun: 0xb0a0e0, sun_i: 1.0, amb: 0x504a78, amb_i: 1.0, tint: 0xa8a0d0, sun_pos: Vec3::new(40.0, 35.0, 0.0), hemi: 0.3 },
    DayKey { at: 21.5, ..NIGHT },
    DayKey { at: 24.0, ..NIGHT },
];

/// The light at `hours` (0 to 24): between the day's two keys around it, smoothly.
struct DayLight {
    sky: Vec3,
    sun: Vec3,
    sun_i: f32,
    amb: Vec3,
    amb_i: f32,
    tint: Vec3,
    sun_pos: Vec3,
    hemi: f32,
}

fn day_light(hours: f32) -> DayLight {
    let h = hours.rem_euclid(24.0);
    let i = DAY.iter().rposition(|k| k.at <= h).unwrap_or(0).min(DAY.len() - 2);
    let (a, b) = (DAY[i], DAY[i + 1]);
    let t = ((h - a.at) / (b.at - a.at).max(1e-3)).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    let mix = |x: u32, y: u32| srgb_to_linear(x).lerp(srgb_to_linear(y), t);
    DayLight {
        sky: mix(a.sky, b.sky),
        sun: mix(a.sun, b.sun),
        sun_i: a.sun_i + (b.sun_i - a.sun_i) * t,
        amb: mix(a.amb, b.amb),
        amb_i: a.amb_i + (b.amb_i - a.amb_i) * t,
        tint: mix(a.tint, b.tint),
        sun_pos: a.sun_pos.lerp(b.sun_pos, t),
        hemi: a.hemi + (b.hemi - a.hemi) * t,
    }
}



impl Stage {
    pub fn new(spot: Spot, hours: f32) -> Stage {
        let line_gap = 5.0f32.min(spot.depth as f32 - 1.0);
        let focus = Vec3::new(spot.x, 0.7, spot.z - 0.4);
        Stage {
            spot, line_gap, vis: BTreeMap::new(), people: vec![], effects: vec![], hours, weather: None, targets: vec![],
            cam_focus: focus, cam_goal: focus, view_proj: Mat4::IDENTITY, viewport: egui::Rect::NOTHING, rain: vec![], seed: 12345,
            toon: render::scene::Toon::default(),
            playing: None,
            sounds: vec![],
            content: None,
            retro: Default::default(),
            wind: Default::default(),
            radiance: Default::default(),
        }
    }

    // ------------------------------------------------------------ a timeline playing

    /// Plays an ability's timeline from now: the user's and the target's motions, the FX
    /// round them, the sounds, the camera. What was playing stops.
    pub fn start_timeline(&mut self, key: &str, user: Uid, targets: Vec<Uid>, timeline: content::Timeline, now: f64) {
        self.playing = Some(Playing { key: key.to_string(), t0: now, timeline, user, targets, played: BTreeSet::new() });
    }

    /// The sounds that came due since last asked.
    pub fn take_sounds(&mut self) -> Vec<(String, f32)> {
        std::mem::take(&mut self.sounds)
    }

    /// Seconds into the timeline playing, if one is.
    pub fn playing_at(&self, now: f64) -> Option<f64> {
        self.playing.as_ref().map(|p| (now - p.t0) / 1000.0)
    }

    /// Where the timeline's motions put each battler in it at `now`.
    fn timeline_poses(&self, now: f64) -> BTreeMap<Uid, world::timeline::Pose> {
        let mut out = BTreeMap::new();
        let Some(p) = &self.playing else { return out };
        let t = (now - p.t0) / 1000.0;
        let home = |u: Uid| self.vis.get(&u).map(|v| v.home).unwrap_or(Vec3::ZERO);
        let user = home(p.user);
        let target = p.targets.first().map(|&u| home(u)).unwrap_or(user + Vec3::NEG_Z * 5.0);
        for u in std::iter::once(p.user).chain(p.targets.iter().copied()) {
            let (who, other) = if u == p.user { ("user", target) } else { ("target", user) };
            let towards = other - home(u);
            out.insert(u, world::timeline::pose(&p.timeline.clips, who, t, towards, towards.length()));
        }
        out
    }

    /// The way a battler faces, turned so +z points at the other one (degrees, as
    /// `placement` takes them).
    fn facing(&self, uid: Uid) -> f32 {
        let (Some(p), Some(v)) = (&self.playing, self.vis.get(&uid)) else { return 0.0 };
        let other = if uid == p.user { p.targets.first().copied() } else { Some(p.user) };
        let f = other.and_then(|o| self.vis.get(&o)).map(|o| (o.pos - v.pos).normalize_or(Vec3::NEG_Z)).unwrap_or(Vec3::NEG_Z);
        (-f.x).atan2(f.z).to_degrees()
    }

    /// The fx clips showing at `now`: the clip, its record and space, its frame, and who
    /// it's anchored on.
    fn fx_showing(&self, now: f64) -> Vec<(content::Fx, content::Space, u8, Uid)> {
        let (Some(p), Some(content)) = (&self.playing, &self.content) else { return vec![] };
        let t = (now - p.t0) / 1000.0;
        let mut out = vec![];
        for c in p.timeline.clips.iter().filter(|c| c.kind == "fx") {
            let Some(e) = content.fx.entry(&c.what) else { continue };
            let Some(frame) = world::timeline::fx_frame(c, &e.value, t) else { continue };
            let who = if c.who == "target" { p.targets.first().copied() } else { Some(p.user) };
            if let Some(u) = who {
                out.push((e.value.clone(), e.space, frame, u));
            }
        }
        out
    }

    /// The flipbook a mesh draws: its texture key and the size of its frames.
    fn flipbook(fx: &content::Fx, space: content::Space, mesh: &content::FxMesh) -> (String, u16) {
        (space.asset(mesh.drawn()), if mesh.sheet.is_empty() { fx.size } else { fx.export_size })
    }

    /// The flipbooks the timeline playing needs (texture keys, pixel-sharp or not).
    pub fn fx_textures(&self, now: f64) -> Vec<(String, bool)> {
        let mut out = vec![];
        for (fx, space, _, _) in self.fx_showing(now) {
            for mesh in &fx.meshes {
                out.push((Self::flipbook(&fx, space, mesh).0, mesh.pixelated));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The FX meshes that aren't billboards, placed round whoever they're on, as the
    /// renderer's batches (empty when none show).
    pub fn fx_batches(&self, now: f64) -> Vec<(String, Vec<render::map::MapVertex>, Vec<u32>)> {
        let mut scene = world::mesh::Scene::default();
        for (fx, space, frame, uid) in self.fx_showing(now) {
            let Some(v) = self.vis.get(&uid) else { continue };
            let anchor = world::mesh::placement(v.pos, self.facing(uid), 1.0);
            for mesh in fx.meshes.iter().filter(|m| m.shape != "billboard") {
                let (key, size) = Self::flipbook(&fx, space, mesh);
                let mut shape = world::fx::shape(&mesh.shape, Vec3::new(mesh.size[0] as f32, mesh.size[1] as f32, mesh.size[2] as f32));
                world::fx::into_frame(&mut shape, world::fx::frame_uv(frame, fx.frames, mesh.columns, size));
                for vx in &mut shape.vertices {
                    vx.toon = if mesh.lit { 0.0 } else { 2.0 };
                }
                let turn = Mat4::from_rotation_y(-(mesh.turn[1] as f32).to_radians()) * Mat4::from_rotation_x((mesh.turn[0] as f32).to_radians()) * Mat4::from_rotation_z((mesh.turn[2] as f32).to_radians());
                let at = Vec3::new(mesh.at[0] as f32, mesh.at[1] as f32, mesh.at[2] as f32);
                scene.textured.entry(key).or_default().append(&shape, anchor * Mat4::from_translation(at) * turn);
            }
        }
        if scene.is_empty() {
            return vec![];
        }
        crate::mapedit::batches(&scene)
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

    pub fn add_battler(&mut self, uid: Uid, side: Side, sp: &rules::dex::Species, now: f64) {
        let back = side == Side::Player;
        let lift = if back { 0.0 } else { sp.elevation as f32 * PX * 1.6 };
        let phase = self.rand() * 6.0;
        let (species, front_box, back_box) = (&sp.key, sp.front_box, sp.back_box);
        self.vis.insert(uid, Vis {
            side,
            light: sp.light.clone(),
            sound: sp.sound.clone(),
            toon: sp.toon,
            texture: format!("{}/{species}", if back { "back" } else { "front" }),
            bbox: if back { back_box } else { front_box },
            lift, float: lift > 0.0, phase, home: Vec3::ZERO, pos: Vec3::ZERO, placed: false, spawn: Some(now),
            leaving: None, pocketed: None, flash_until: 0.0, flash_power: 4.0, shake_from: -1e9, pulse: None, aura: None, motion: None,
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

    /// An IDP landed on `uid`: the net takes it into the pocket, whose seams light up as it
    /// strains (`struggle`, 0 to 1: how close it came to breaking out), then go dark as it
    /// gives up (`caught`), or flare and burst as it forces its way out, the creature back
    /// where it was. How long the whole takes (ms).
    pub fn pocket(&mut self, uid: Uid, now: f64, struggle: f32, caught: bool) -> f64 {
        let at = self.center_of(uid);
        self.effects.push(Effect::Pocket { t0: now, at, struggle: struggle.clamp(0.0, 1.0), caught });
        if !caught {
            self.burst(at, [0.8, 0.9, 1.0, 1.0], 16, 1.1, now + POCKET_KNOWN_MS);
        }
        if let Some(v) = self.vis.get_mut(&uid) {
            v.pocketed = Some((now, (!caught).then_some(now + POCKET_KNOWN_MS)));
        }
        POCKET_MS + 150.0
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
        // the timeline playing: over when its time is up; its sounds as they come due
        if let Some(p) = &mut self.playing {
            let t = (now - p.t0) / 1000.0;
            if t > p.timeline.seconds.max(0.05) + 0.05 {
                self.playing = None;
            } else {
                for (i, c) in p.timeline.clips.iter().enumerate().filter(|(_, c)| c.kind == "sound") {
                    if t >= c.at && !p.played.contains(&i) {
                        self.sounds.push((c.what.clone(), if c.value > 0.0 { c.value as f32 } else { 1.0 }));
                        p.played.insert(i);
                    }
                }
            }
        }
        let poses = self.timeline_poses(now);
        let mut gone = vec![];
        for (&u, v) in self.vis.iter_mut() {
            if let Some((t0, faint)) = v.leaving {
                if now - t0 > if faint { 450.0 } else { 220.0 } {
                    gone.push(u);
                }
            }
            // out of the pocket and grown back: as it was
            if let Some((_, Some(back))) = v.pocketed {
                if now > back + 400.0 {
                    v.pocketed = None;
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
            v.pos = pos + poses.get(&u).map(|p| p.offset).unwrap_or(Vec3::ZERO);
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
                Effect::Pocket { t0, .. } => (*t0, POCKET_MS),
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
        // the timeline playing shakes it and brings it closer
        let (shake, zoom) = match (&self.playing, self.playing_at(now)) {
            (Some(p), Some(t)) => world::timeline::camera(&p.timeline.clips, t),
            _ => (Vec3::ZERO, 1.0),
        };
        let f = self.cam_focus + shake;
        let sway = ((now / 2400.0).sin() * 0.25) as f32;
        // a three-quarter view from behind the player's right
        let eye = f + Vec3::new(5.2 + sway, 9.6, 15.5) * zoom;
        let view = Mat4::look_at_rh(eye, f, Vec3::Y);
        let proj = Mat4::perspective_rh(30f32.to_radians(), aspect.max(0.1), 0.1, 200.0);
        (proj * view, eye)
    }

    fn lighting(&self) -> (Lighting, Vec3) {
        lighting_at(self.hours, self.weather, Vec3::new(self.spot.x, 0.0, self.spot.z), 22.0)
    }

    pub fn frame(&mut self, now: f64, viewport: egui::Rect) -> Frame {
        self.viewport = viewport;
        let (view_proj, eye) = self.camera(now, viewport.width() / viewport.height().max(1.0));
        self.view_proj = view_proj;
        let (mut light, tint) = self.lighting();
        let mut billboards = vec![];
        let mut shapes = vec![];
        let mut glows = vec![];
        let tint4 = [tint.x, tint.y, tint.z, 1.0];
        let lamps = lamps_at(self.hours);
        let poses = self.timeline_poses(now);
        for person in &self.people {
            billboards.push(Billboard {
                texture: person.texture.clone(), pos: person.pos, size: Vec2::new(1.0, 2.0), offset: Vec2::ZERO,
                uv: [0.0, 0.0, 1.0, 1.0], color: tint4, casts_shadow: true, toon: false,
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
            let mut flash = if now < v.flash_until { v.flash_power } else { 1.0 };
            // into an IDP (gone in a moment), and out again if it forced its way out
            if let Some((t0, back)) = v.pocketed {
                scale *= 1.0 - (((now - t0) / POCKET_NET_MS) as f32).clamp(0.0, 1.0);
                if let Some(b) = back.filter(|&b| now >= b) {
                    scale = (((now - b) / 260.0) as f32).clamp(0.01, 1.0);
                    if now < b + 300.0 {
                        flash = 3.0;
                    }
                }
            }
            if let Some(p) = poses.get(&u) {
                // the timeline's motions: grown, flashing (its moves are in v.pos already)
                scale *= p.scale;
                flash = flash.max(1.0 + p.flash * 3.0);
            }
            let size = Vec2::splat(64.0 * PX * scale);
            let offset = Vec2::new((-((x0 + x1) as f32 / 2.0 - 32.0) * PX) * scale + shake, lift - (64 - y1) as f32 * PX * scale);
            billboards.push(Billboard {
                texture: v.texture.clone(), pos: v.pos, size, offset, uv: [0.0, 0.0, 1.0, 1.0],
                color: [tint.x * flash, tint.y * flash, tint.z * flash, 1.0], casts_shadow: true, toon: v.toon,
            });
            let r = (((x1 - x0) as f32) * PX * 0.45).max(0.35) * scale;
            shapes.push(Shape { kind: ShapeKind::Disc { radius: r, squash: 0.6 }, center: v.pos + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.3 * fade], additive: false });
            // the light it carries: the player's side faces the foes (away from the camera)
            if let Some(l) = v.light.as_ref().filter(|l| (!l.night || lamps > 0.0) && light.lights.len() < MAX_LIGHTS) {
                let forward = if v.side == Side::Player { Vec3::NEG_Z } else { Vec3::Z };
                let placed = world::lights::carried(l, v.pos + Vec3::Y * lift, forward);
                let k = l.intensity as f32 * if l.night { lamps } else { 1.0 } * scale * fade;
                let c = world::color(&l.color);
                light.lights.push(spot_light(l, placed.pos, placed.dir, k));
                glows.push(Glow { pos: placed.pos, size: 0.25 + 0.12 * k, color: [c[0], c[1], c[2], (0.6 * k).min(1.0)] });
            }
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
        // the timeline's FX billboards, round whoever they're on
        for (fx, space, frame, uid) in self.fx_showing(now) {
            let Some(v) = self.vis.get(&uid) else { continue };
            let anchor = world::mesh::placement(v.pos, self.facing(uid), 1.0);
            for mesh in fx.meshes.iter().filter(|m| m.shape == "billboard") {
                let (key, size) = Self::flipbook(&fx, space, mesh);
                let pos = anchor.transform_point3(Vec3::new(mesh.at[0] as f32, mesh.at[1] as f32, mesh.at[2] as f32));
                billboards.push(Billboard {
                    texture: key,
                    pos: pos - Vec3::Y * (mesh.size[1] as f32 / 2.0),
                    size: Vec2::new(mesh.size[0] as f32, mesh.size[1] as f32),
                    offset: Vec2::ZERO,
                    uv: world::fx::frame_uv(frame, fx.frames, mesh.columns, size),
                    color: [1.0, 1.0, 1.0, 1.0],
                    casts_shadow: false,
                    toon: false,
                });
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
                    if now < *t0 {
                        continue; // set off a moment later
                    }
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
                Effect::Pocket { t0, at, struggle, caught } => {
                    let t = now - t0;
                    // the net, spreading from the pocket as the creature goes in
                    let net = ((t / POCKET_NET_MS) as f32).clamp(0.0, 1.0);
                    if net < 1.0 {
                        let s = 0.3 + net * 2.2;
                        shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.7 * s, outer: s }, center: *at + Vec3::Y * 0.08, color: [0.55, 0.75, 1.0, 0.8 * (1.0 - net)], additive: true });
                    }
                    if (POCKET_NET_MS..POCKET_MS).contains(&t) {
                        // the pocket, dark blue; its seams' light rising toward how hard the
                        // creature strains, pulsing with its struggle; then, known: dark as it
                        // gives up, or flaring as it bursts out
                        let u = (((t - POCKET_NET_MS) / (POCKET_KNOWN_MS - POCKET_NET_MS)) as f32).clamp(0.0, 1.0);
                        let peak = struggle.max(0.12);
                        let mut light = 0.12 + (peak - 0.12) * u * u + 0.08 * u * ((u * 9.0) * std::f32::consts::PI).sin().abs();
                        if t >= POCKET_KNOWN_MS {
                            light = if *caught { peak * (1.0 - ((t - POCKET_KNOWN_MS) / 250.0) as f32).clamp(0.0, 1.0) } else { 1.0 };
                        }
                        glows.push(Glow { pos: *at, size: 0.26, color: [0.3, 0.45, 0.9, 0.95] });
                        glows.push(Glow { pos: *at, size: 0.26 + 0.55 * light, color: [0.75, 0.88, 1.0, light] });
                    }
                }
            }
        }
        let streaks = self.rain.iter().map(|&p| (p, p + Vec3::new(-0.05, -0.5, 0.05))).collect();
        Frame { view_proj, camera_pos: eye, light, shadows: true, billboards, shapes, glows, animated: vec![], streaks, time: (now / 1000.0) as f32, grid: false, focus: None, dim: 0.0, overlays: vec![], toon: self.toon, retro: self.retro, frost: false, wind: self.wind, see_through: None, pushers: vec![], radiance: self.radiance }
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

/// A record's light as the renderer's, placed and at `k` times its intensity. A point
/// light is a cone of everything.
pub fn spot_light(l: &content::Light, pos: Vec3, dir: Vec3, k: f32) -> SpotLight {
    let c = world::color(&l.color);
    let point = l.kind == "point";
    let outer = if point { std::f32::consts::PI } else { (l.cone as f32 / 2.0).to_radians() };
    let inner = if point { outer - 0.001 } else { outer * 0.66 };
    SpotLight { pos, dir, color: Vec3::new(c[0], c[1], c[2]) * k, range: l.range as f32, inner, outer }
}

/// How lit the lamps are at a time of day: full by night, a little in the morning, off by
/// day.
/// How lit the lamps are at `hours`: out by day, coming on at dusk (18:30 to 20:00), on all
/// night, going out at dawn (5:30 to 7:00).
pub fn lamps_at(hours: f32) -> f32 {
    let h = hours.rem_euclid(24.0);
    let ramp = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if h < 12.0 {
        1.0 - ramp((h - 5.5) / 1.5)
    } else {
        ramp((h - 18.5) / 1.5)
    }
}

/// The light of a time of day and weather, with the sun's shadow covering `radius` around
/// `target` (and fog starting past it); also the tint for pictures.
pub fn lighting_at(hours: f32, weather: Option<WeatherKind>, target: Vec3, radius: f32) -> (Lighting, Vec3) {
    let p = day_light(hours);
    let pi = std::f32::consts::PI;
    let mut sky = p.sky;
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
    let hemi_i = p.hemi;
    // in steps (so a zoom doesn't change the shadow's texels each frame)
    let radius = 2f32.powf((radius.max(1.0).log2() * 4.0).ceil() / 4.0);
    let far = (radius / 22.0).max(1.0);
    let sun_pos = target + p.sun_pos * far;
    let light_view = Mat4::look_at_rh(sun_pos, target, Vec3::Y);
    let light_proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 1.0, 200.0 * far);
    (
        Lighting {
            sun_dir: p.sun_pos.normalize(),
            sun: p.sun * sun_i / pi,
            ambient: p.amb * amb_i / pi,
            hemi_sky: srgb_to_linear(0xcfe6ff) * hemi_i / pi,
            hemi_ground: srgb_to_linear(0x6b8a5a) * hemi_i / pi,
            sky,
            fog_near: 30.0 * far,
            fog_far: 70.0 * far,
            light_view_proj: light_proj * light_view,
            lights: vec![],
        },
        p.tint,
    )
}

#[cfg(test)]
mod day_tests {
    use super::*;

    #[test]
    fn the_light_follows_the_day() {
        let sun = |h: f32| day_light(h).sun_i;
        let sky = |h: f32| day_light(h).sky.length();
        // night, dawn, morning, noon, dusk, night again: brighter by day, the same at midnight
        assert!(sun(12.0) > sun(8.0) && sun(8.0) > sun(6.0) && sun(6.0) > sun(2.0), "{} {} {} {}", sun(12.0), sun(8.0), sun(6.0), sun(2.0));
        assert!(sky(13.0) > sky(23.0) * 3.0);
        assert!((sky(0.0) - sky(24.0)).abs() < 1e-5 && (sky(23.99) - sky(0.0)).abs() < 1e-3, "midnight meets itself");
        // no jump anywhere: a minute changes little
        for m in 0..(24 * 60) {
            let (a, b) = (day_light(m as f32 / 60.0), day_light((m + 1) as f32 / 60.0));
            assert!((a.sun_i - b.sun_i).abs() < 0.05 && a.sky.distance(b.sky) < 0.05, "{m} min");
        }
        // the sun from the east in the morning, from the west late in the day
        assert!(day_light(8.0).sun_pos.x < 0.0 && day_light(17.0).sun_pos.x > 0.0);
        // the lamps: out by day, on by night, coming on at dusk
        assert_eq!(lamps_at(13.0), 0.0);
        assert_eq!(lamps_at(23.0), 1.0);
        assert_eq!(lamps_at(3.0), 1.0);
        assert!(lamps_at(19.25) > 0.2 && lamps_at(19.25) < 0.8 && lamps_at(6.25) > 0.2 && lamps_at(6.25) < 0.8);
    }
}
