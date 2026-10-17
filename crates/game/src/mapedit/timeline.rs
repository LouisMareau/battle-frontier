//! The Timeline panel: an ability playing out on the map. Its clips lie on tracks in time
//! (the user's and the target's animations and motions, FX, sounds, the player's inputs,
//! the camera); a playhead scrubs or plays them, and a stand-in user and target act them
//! out where the view looks (any prop with a model, a rigged one playing its animations,
//! or a creature's picture): motions move them, FX frames play on their meshes round
//! them, sounds sound, input cues shrink onto their mark (Z, the A button, is judged
//! against the window), the camera shakes or comes closer.
//!
//! An fx clip's "Edit the FX" holds the timeline still and opens the FX editor: the FX's
//! meshes (a billboard, a flat quad, a sphere, a dome, a ring) placed round the actor,
//! dragged on the map or set by hand, each with its flipbook of hand-drawn frames; the
//! Drawing window paints the frame shown, the frames before and after ghosted, and "Export
//! the flipbooks" packs every mesh's frames at a smaller size for the game. The FX records
//! and their sheets are saved like abilities: into temporary/ unless told otherwise (in
//! the browser, downloaded).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use content::{Clip, Fx, FxMesh, Space, CAMERAS, CLIP_KINDS, FX_SHAPES, INPUTS, MOTIONS};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use glam::{Mat4, Vec2, Vec3};
use render::scene::{Animated, Billboard, Glow, SceneRenderer, Shape, ShapeKind};
use world::mesh::{placement, prop_scene, Scene};

use super::{hint, mark, section, slug_ok, warn, MapEditor};
use crate::platform::Load;

/// The tracks, top to bottom, and the kind each holds.
const TRACKS: [(&str, &str); 6] = [("User", "animation"), ("Target", "animation"), ("Motion", "motion"), ("FX", "fx"), ("Sound", "sound"), ("Input", "input")];
const TRACK_H: f32 = 22.0;
const RULER_H: f32 = 18.0;
const LABEL_W: f32 = 64.0;
/// How far the target stands from the user, in cells.
const APART: f32 = 3.0;
/// The brush sizes to pick from, in pixels of the frame.
const BRUSHES: [f32; 5] = [2.0, 4.0, 8.0, 14.0, 24.0];

/// A sheet of frames as pixels, drawn on and sent to the GPU as it changes.
pub(super) struct Sheet {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub version: u64,
    uploaded: u64,
    pub dirty: bool,
}

/// An FX being edited, the timeline held still on it.
pub(super) struct FxEdit {
    pub clip: usize,
    pub fx: String,
    pub mesh: usize,
    pub frame: u8,
    pub brush: f32,
    pub color: [u8; 3],
    pub eraser: bool,
    pub onion: bool,
    pub playing: bool,
    /// the frames as they were before each stroke (frame, pixels)
    undo: Vec<(u8, Vec<u8>)>,
    /// the pointer's last place on the frame while painting
    last: Option<(f32, f32)>,
    tex: Option<egui::TextureHandle>,
    /// what the canvas shows: the sheet's version, the frame, the onion skin
    shown: (u64, u8, bool),
}

pub(super) struct Timelines {
    pub open: bool,
    /// the ability shown
    pub of: Option<String>,
    pub playing: bool,
    pub looping: bool,
    /// the playhead, in seconds
    pub t: f64,
    last_tick: f64,
    pub selected: Option<usize>,
    /// where the user stands (its feet), facing +z; the target APART cells on
    pub at: Option<Vec3>,
    pub user: String,
    pub target: String,
    pub fx_edit: Option<FxEdit>,
    /// fx records changed here and not saved yet
    pub fx_dirty: BTreeSet<String>,
    /// the sheets, by asset path
    sheets: HashMap<String, Sheet>,
    /// sounds to play (asset paths, volume): the frame's end plays them
    pub shots: Vec<(String, f32)>,
    /// the sound clips played this pass
    played: BTreeSet<usize>,
    /// how the last input went, and until when it shows
    judged: Option<(String, f64)>,
    /// the camera's shake applied this frame, taken back the next
    shaken: Vec3,
    zoomed: bool,
    /// pixels per second on the tracks
    pub scale: f32,
    pub new_fx: String,
    /// a clip dragged: which, and where it was grabbed from its start (seconds)
    drag: Option<(usize, f64)>,
}

impl Default for Timelines {
    fn default() -> Timelines {
        Timelines {
            open: false,
            of: None,
            playing: false,
            looping: true,
            t: 0.0,
            last_tick: 0.0,
            selected: None,
            at: None,
            user: "prop:stand_in_character".into(),
            target: "prop:stand_in_character".into(),
            fx_edit: None,
            fx_dirty: BTreeSet::new(),
            sheets: HashMap::new(),
            shots: vec![],
            played: BTreeSet::new(),
            judged: None,
            shaken: Vec3::ZERO,
            zoomed: false,
            scale: 160.0,
            new_fx: String::new(),
            drag: None,
        }
    }
}

/// How far along a clip a moment is, 0 to 1 (None outside it).
fn along(c: &Clip, t: f64) -> Option<f32> {
    let len = c.seconds.max(0.05);
    (t >= c.at && t <= c.at + len).then(|| ((t - c.at) / len) as f32)
}

/// Up and back down over a clip: 0 at its ends, 1 in the middle.
fn bump(p: f32) -> f32 {
    (p * std::f32::consts::PI).sin().max(0.0)
}

fn track_of(kind: &str, who: &str) -> usize {
    match kind {
        "animation" if who == "target" => 1,
        "animation" => 0,
        "motion" => 2,
        "fx" => 3,
        "sound" => 4,
        _ => 5,
    }
}

fn clip_color(kind: &str) -> Color32 {
    match kind {
        "animation" => Color32::from_rgb(90, 150, 230),
        "motion" => Color32::from_rgb(120, 200, 120),
        "fx" => Color32::from_rgb(230, 150, 70),
        "sound" => Color32::from_rgb(190, 120, 220),
        "input" => Color32::from_rgb(240, 200, 80),
        _ => Color32::from_rgb(160, 160, 160),
    }
}

/// A PNG of pixels.
fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or("a sheet of the wrong size")?;
    let mut out = std::io::Cursor::new(vec![]);
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// A disc of the brush stamped on a frame of a sheet, at frame pixels (x, y): a colour, or
/// none to erase.
fn stamp(sheet: &mut Sheet, origin: (u32, u32), size: u16, x: f32, y: f32, radius: f32, paint: Option<[u8; 3]>) {
    let r = radius.max(0.5);
    let (x0, x1) = (((x - r).floor() as i32).max(0), ((x + r).ceil() as i32).min(size as i32 - 1));
    let (y0, y1) = (((y - r).floor() as i32).max(0), ((y + r).ceil() as i32).min(size as i32 - 1));
    for py in y0..=y1 {
        for px in x0..=x1 {
            let d = ((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt();
            if d > r {
                continue;
            }
            let i = (((origin.1 + py as u32) * sheet.width + origin.0 + px as u32) * 4) as usize;
            if let Some(p) = sheet.rgba.get_mut(i..i + 4) {
                match paint {
                    Some(c) => p.copy_from_slice(&[c[0], c[1], c[2], 255]),
                    None => p.copy_from_slice(&[0, 0, 0, 0]),
                }
            }
        }
    }
}

impl MapEditor {
    /// Shows an ability's timeline, the actors standing where the view looks.
    pub(super) fn open_timeline(&mut self, id: &str) {
        if self.tl.of.as_deref() != Some(id) {
            self.tl.t = 0.0;
            self.tl.selected = None;
            self.tl.playing = false;
            self.tl.fx_edit = None;
            self.tl.played.clear();
        }
        self.tl.open = true;
        self.tl.of = Some(id.to_string());
        if self.tl.at.is_none() {
            self.stand_here();
        }
    }

    /// Puts the user where the view looks, on the ground there.
    fn stand_here(&mut self) {
        let t = self.cam.target;
        let (x, z) = (t.x.floor() as i32, t.z.floor() as i32);
        let y = self.map.level_at(x, z).unwrap_or(0) as f32 * self.level_height;
        self.tl.at = Some(Vec3::new(x as f32 + 0.5, y, z as f32 + 0.5));
        self.extra_changed = true;
    }

    fn tl_move(&self) -> Option<content::Move> {
        self.tl.of.as_deref().and_then(|id| self.content.moves.get(id)).cloned()
    }

    fn change_clips(&mut self, change: impl FnOnce(&mut content::Timeline)) {
        let Some(id) = self.tl.of.clone() else { return };
        self.change_move(&id, |m| change(&mut m.timeline));
        self.extra_changed = true;
    }

    // ------------------------------------------------------------ the actors

    /// Where someone stands at a moment: their feet, which way they face (degrees clockwise
    /// from south), their scale, and how much they flash; the motions move them.
    fn tl_pose(&self, who: &str, t: f64) -> (Vec3, f32, f32, f32) {
        let base = self.tl.at.unwrap_or(Vec3::ZERO);
        let (mut feet, facing) = if who == "target" { (base + Vec3::new(0.0, 0.0, APART), 180.0) } else { (base, 0.0) };
        let other = if who == "target" { base } else { base + Vec3::new(0.0, 0.0, APART) };
        let (mut scale, mut flash) = (1.0, 0.0);
        if let Some(m) = self.tl_move() {
            for c in m.timeline.clips.iter().filter(|c| c.kind == "motion" && c.who == who) {
                let Some(p) = along(c, t) else { continue };
                let value = c.value as f32;
                match c.what.as_str() {
                    "lunge" => {
                        let reach = if value > 0.0 { value } else { APART - 1.0 };
                        let towards = (other - feet).normalize_or(Vec3::Z);
                        // a wind-back, the dash, a hold, and back
                        let k = if p < 0.2 { -0.2 * (p / 0.2) } else if p < 0.45 { -0.2 + 1.2 * ((p - 0.2) / 0.25) } else if p < 0.7 { 1.0 } else { 1.0 - (p - 0.7) / 0.3 };
                        feet += towards * reach * k;
                    }
                    "hop" => feet.y += (if value > 0.0 { value } else { 0.6 }) * bump(p),
                    "shake" => feet.x += (if value > 0.0 { value } else { 0.15 }) * (p * 50.0).sin() * (1.0 - p),
                    "pulse" => scale += (if value > 0.0 { value } else { 0.2 }) * bump(p),
                    "flash" => flash = (if value > 0.0 { value } else { 1.0 }) * (1.0 - p),
                    _ => {}
                }
            }
        }
        (feet, facing, scale, flash)
    }

    /// What someone looks like: a prop (by id) or a creature's picture.
    fn tl_look(&self, who: &str) -> &str {
        if who == "target" { &self.tl.target } else { &self.tl.user }
    }

    /// The animation playing on someone at a moment, and how far into it (seconds).
    fn tl_animation(&self, who: &str, t: f64) -> Option<(String, f32)> {
        let m = self.tl_move()?;
        m.timeline.clips.iter().filter(|c| c.kind == "animation" && c.who == who).find_map(|c| {
            let len = if c.seconds > 0.0 { c.seconds } else { f64::MAX };
            (t >= c.at && t <= c.at + len).then(|| (c.what.clone(), (t - c.at) as f32))
        })
    }

    /// The actors' meshes (prop models, and a rigged model's still parts) and the FX meshes
    /// on them, for what moves on the map.
    pub(super) fn timeline_scene(&self, now: f64) -> Scene {
        let mut out = Scene::default();
        if !self.tl.open || self.tl.at.is_none() {
            return out;
        }
        let t = self.tl.t;
        let _ = now;
        for who in ["user", "target"] {
            let (feet, facing, scale, _) = self.tl_pose(who, t);
            if let Some(id) = self.tl_look(who).strip_prefix("prop:") {
                if let Some(prop) = self.content.props.get(id) {
                    out.append(&prop_scene(prop, &self.models), placement(feet, facing, scale));
                }
            }
            // the FX anchored on them: every mesh but the billboards (the renderer's)
            let Some(m) = self.tl_move() else { continue };
            for (i, c) in m.timeline.clips.iter().enumerate().filter(|(_, c)| c.kind == "fx" && c.who == who) {
                let Some(fx) = self.content.fx.get(&c.what) else { continue };
                let Some(frame) = self.fx_frame(i, c, fx, t) else { continue };
                let editing = self.tl.fx_edit.as_ref().map(|e| e.fx == fx.id).unwrap_or(false);
                for mesh in fx.meshes.iter().filter(|m| m.shape != "billboard") {
                    let key = self.fx_key(fx, mesh, editing);
                    let mut shape = world::fx::shape(&mesh.shape, Vec3::new(mesh.size[0] as f32, mesh.size[1] as f32, mesh.size[2] as f32));
                    world::fx::into_frame(&mut shape, world::fx::frame_uv(frame, fx.frames, mesh.columns, if editing { fx.size } else { self.sheet_frame_size(&key, fx) }));
                    for v in &mut shape.vertices {
                        v.toon = if mesh.lit { 0.0 } else { 2.0 };
                    }
                    let turn = Mat4::from_rotation_y(-(mesh.turn[1] as f32).to_radians()) * Mat4::from_rotation_x((mesh.turn[0] as f32).to_radians()) * Mat4::from_rotation_z((mesh.turn[2] as f32).to_radians());
                    let at = Vec3::new(mesh.at[0] as f32, mesh.at[1] as f32, mesh.at[2] as f32);
                    let m = placement(feet, facing, 1.0) * Mat4::from_translation(at) * turn;
                    out.textured.entry(key).or_default().append(&shape, m);
                }
            }
        }
        out
    }

    /// The frame an fx clip shows at a moment (looping while the clip lasts; a clip with no
    /// length plays once); the one being edited shows the frame picked.
    fn fx_frame(&self, i: usize, c: &Clip, fx: &Fx, t: f64) -> Option<u8> {
        if let Some(e) = self.tl.fx_edit.as_ref().filter(|e| e.clip == i) {
            return Some(e.frame.min(fx.frames.saturating_sub(1)));
        }
        let since = t - c.at;
        if since < 0.0 {
            return None;
        }
        let total = fx.frames.max(1) as f64 / fx.fps.max(0.1);
        let len = if c.seconds > 0.0 { c.seconds } else { total };
        if since > len {
            return None;
        }
        Some(((since * fx.fps.max(0.1)) as u64 % fx.frames.max(1) as u64) as u8)
    }

    /// The texture key of a mesh's flipbook: its source while it's edited, else what the
    /// game draws.
    fn fx_key(&self, fx: &Fx, mesh: &FxMesh, editing: bool) -> String {
        let space = self.content.fx.entry(&fx.id).map(|e| e.space).unwrap_or(Space::Temporary);
        space.asset(if editing || mesh.sheet.is_empty() { &mesh.source } else { &mesh.sheet })
    }

    /// The frame size of a sheet as loaded (an export is smaller than the source).
    fn sheet_frame_size(&self, key: &str, fx: &Fx) -> u16 {
        match self.tl.sheets.get(key) {
            Some(s) if s.width > 0 => (s.width / fx.meshes.first().map(|m| m.columns.max(1) as u32).unwrap_or(1)).min(u16::MAX as u32) as u16,
            _ => fx.size,
        }
    }

    /// The actors with rigs, posed by their animation clips.
    pub(super) fn timeline_animated(&self, now: f64) -> Vec<Animated> {
        let mut out = vec![];
        if !self.tl.open || self.tl.at.is_none() {
            return out;
        }
        let _ = now;
        let t = self.tl.t;
        for who in ["user", "target"] {
            let Some(id) = self.tl_look(who).strip_prefix("prop:") else { continue };
            let Some(m) = self.models.get(id) else { continue };
            let Some(rig) = &m.rig else { continue };
            let (feet, facing, scale, _) = self.tl_pose(who, t);
            let (anim, at) = match self.tl_animation(who, t) {
                Some((name, since)) => (m.animations.iter().find(|a| a.name == name), since),
                None => (None, 0.0),
            };
            out.push(Animated { key: id.to_string(), model: placement(feet, facing, scale), joints: world::anim::pose(rig, anim, at) });
        }
        out
    }

    /// The actors drawn as pictures, the FX billboards, the input cues and the flashes.
    pub(super) fn timeline_frame(&mut self, now: f64, billboards: &mut Vec<Billboard>, shapes: &mut Vec<Shape>, glows: &mut Vec<Glow>) {
        if !self.tl.open || self.tl.at.is_none() {
            return;
        }
        let t = self.tl.t;
        let Some(m) = self.tl_move() else { return };
        let content = self.content.clone();
        let mut fx_billboards = 0;
        for who in ["user", "target"] {
            let (feet, facing, scale, flash) = self.tl_pose(who, t);
            let look = self.tl_look(who).to_string();
            if let Some(id) = look.strip_prefix("creature:") {
                if let Some(e) = content.creatures.entry(id) {
                    let c = &e.value;
                    let back = who == "user";
                    let (p, b) = if back { (&c.sprites.back, c.sprites.back_box) } else { (&c.sprites.front, c.sprites.front_box) };
                    if let Some(path) = p.as_ref().map(|p| e.space.asset(p)) {
                        let h = ((c.body.height_m as f32) * 1.1).clamp(0.45, 2.4) * scale;
                        match self.sprites.get(&path) {
                            Some(Some((w, ht))) => {
                                let (w, ht) = (*w as f32, *ht as f32);
                                let bw = ((b[2] - b[0]).max(1)) as f32;
                                let bh = ((b[3] - b[1]).max(1)) as f32;
                                let k = 1.0 + flash * 3.0;
                                billboards.push(Billboard {
                                    texture: path,
                                    pos: feet,
                                    size: Vec2::new(h * bw / bh, h),
                                    offset: Vec2::ZERO,
                                    uv: [b[0] as f32 / w, b[1] as f32 / ht, b[2] as f32 / w, b[3] as f32 / ht],
                                    color: [k, k, k, 1.0],
                                    casts_shadow: true,
                                    toon: c.toon,
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
                shapes.push(Shape { kind: ShapeKind::Disc { radius: 0.4, squash: 0.6 }, center: feet + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.28], additive: false });
            }
            if flash > 0.0 {
                glows.push(Glow { pos: feet + Vec3::Y * 0.8, size: 1.6 + flash, color: [1.0, 1.0, 1.0, (0.7 * flash).min(1.0)] });
            }
            // a mark under each: who's who
            let c = if who == "user" { [0.3, 0.6, 1.0, 0.35] } else { [1.0, 0.4, 0.3, 0.35] };
            shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.42, outer: 0.5 }, center: feet + Vec3::Y * 0.02, color: c, additive: false });
            // the FX billboards on them
            for (i, c) in m.timeline.clips.iter().enumerate().filter(|(_, c)| c.kind == "fx" && c.who == who) {
                let Some(fx) = content.fx.get(&c.what) else { continue };
                let Some(frame) = self.fx_frame(i, c, fx, t) else { continue };
                let editing = self.tl.fx_edit.as_ref().map(|e| e.fx == fx.id).unwrap_or(false);
                let turn = placement(feet, facing, 1.0);
                for mesh in fx.meshes.iter().filter(|m| m.shape == "billboard") {
                    let key = self.fx_key(fx, mesh, editing);
                    let size = if editing { fx.size } else { self.sheet_frame_size(&key, fx) };
                    let pos = turn.transform_point3(Vec3::new(mesh.at[0] as f32, mesh.at[1] as f32, mesh.at[2] as f32));
                    fx_billboards += 1;
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
        }
        crate::platform::set_hook("tlbillboards", &fx_billboards.to_string());
        // the input cues: a ring shrinking onto the mark at the beat, as wide as the window
        for c in m.timeline.clips.iter().filter(|c| c.kind == "input") {
            let (feet, ..) = self.tl_pose(if c.what == "block" { "user" } else { "target" }, t);
            let lead = c.seconds.max(0.1);
            let beats: Vec<f64> = match c.what.as_str() {
                "rhythm" => (0..c.count.max(1)).map(|k| c.at + lead * (k + 1) as f64 / c.count.max(1) as f64).collect(),
                _ => vec![c.at + lead],
            };
            for beat in beats {
                let window = (if c.value > 0.0 { c.value } else { 120.0 }) / 1000.0;
                if t < beat - lead || t > beat + window {
                    continue;
                }
                let p = (((t - (beat - lead)) / lead) as f32).clamp(0.0, 1.2);
                let r = 0.45 + 1.1 * (1.0 - p).max(0.0);
                let col = if c.what == "block" { [0.95, 0.35, 0.3, 0.8] } else { [1.0, 0.9, 0.35, 0.8] };
                shapes.push(Shape { kind: ShapeKind::Ring { inner: r - 0.06, outer: r + 0.02, }, center: feet + Vec3::Y * 0.04, color: col, additive: false });
                shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.4, outer: 0.5 }, center: feet + Vec3::Y * 0.03, color: [1.0, 1.0, 1.0, 0.5], additive: false });
            }
        }
        let _ = now;
    }

    // ------------------------------------------------------------ playing

    /// Every frame: the playhead moves, sounds sound as it crosses them, the camera shakes.
    pub(super) fn timeline_tick(&mut self, ctx: &egui::Context, now: f64) {
        // last frame's shake and zoom, taken back
        if self.tl.shaken != Vec3::ZERO {
            self.cam.target -= self.tl.shaken;
            self.tl.shaken = Vec3::ZERO;
        }
        if self.tl.zoomed {
            self.cam.zoom = 1.0;
            self.tl.zoomed = false;
        }
        if !self.tl.open {
            return;
        }
        let Some(m) = self.tl_move() else { return };
        let len = m.timeline.seconds.max(0.1);
        let dt = ((now - self.tl.last_tick) / 1000.0).clamp(0.0, 0.1);
        self.tl.last_tick = now;
        if let Some(e) = &self.tl.fx_edit {
            // held still on the FX's frame
            self.tl.playing = false;
            if let (Some(c), Some(fx)) = (m.timeline.clips.get(e.clip), self.content.fx.get(&e.fx)) {
                self.tl.t = c.at + e.frame as f64 / fx.fps.max(0.1);
            }
        }
        if self.tl.playing {
            let was = self.tl.t;
            self.tl.t += dt;
            if self.tl.t > len {
                if self.tl.looping {
                    self.tl.t -= len;
                    self.tl.played.clear();
                } else {
                    self.tl.t = len;
                    self.tl.playing = false;
                }
            }
            // sounds whose start the playhead crossed
            let space = self.content.moves.entry(&m.id).map(|e| e.space).unwrap_or(Space::Temporary);
            for (i, c) in m.timeline.clips.iter().enumerate().filter(|(_, c)| c.kind == "sound") {
                if self.tl.t >= c.at && (was < c.at || self.tl.t - was > len / 2.0) && !self.tl.played.contains(&i) {
                    self.tl.shots.push((space.asset(&c.what), if c.value > 0.0 { c.value as f32 } else { 1.0 }));
                    self.tl.played.insert(i);
                }
            }
            // Z (the A button) judged against the input windows
            if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                let mut best: Option<(f64, String)> = None;
                for c in m.timeline.clips.iter().filter(|c| c.kind == "input") {
                    let lead = c.seconds.max(0.1);
                    let beats: Vec<f64> = match c.what.as_str() {
                        "rhythm" => (0..c.count.max(1)).map(|k| c.at + lead * (k + 1) as f64 / c.count.max(1) as f64).collect(),
                        _ => vec![c.at + lead],
                    };
                    for beat in beats {
                        let delta = (self.tl.t - beat) * 1000.0;
                        let window = if c.value > 0.0 { c.value } else { 120.0 };
                        if delta.abs() <= window * 3.0 && best.as_ref().map(|b| delta.abs() < b.0).unwrap_or(true) {
                            let word = if delta.abs() <= window * 0.4 { "Perfect" } else if delta.abs() <= window { "Good" } else { "Miss" };
                            best = Some((delta.abs(), format!("{word} ({delta:+.0} ms)")));
                        }
                    }
                }
                self.tl.judged = Some((best.map(|b| b.1).unwrap_or_else(|| "Nothing to hit".into()), now + 1200.0));
            }
        }
        if self.tl.judged.as_ref().map(|j| now > j.1).unwrap_or(false) {
            self.tl.judged = None;
        }
        // the camera
        for c in m.timeline.clips.iter().filter(|c| c.kind == "camera") {
            let Some(p) = along(c, self.tl.t) else { continue };
            match c.what.as_str() {
                "shake" => {
                    let k = (if c.value > 0.0 { c.value as f32 } else { 0.3 }) * (1.0 - p);
                    let (a, b) = ((p * 97.0).sin(), (p * 131.0).cos());
                    self.tl.shaken = Vec3::new(a * k, 0.0, b * k);
                    self.cam.target += self.tl.shaken;
                }
                "zoom" => {
                    self.cam.zoom = 1.0 - (if c.value > 0.0 { c.value as f32 } else { 0.3 }).clamp(0.0, 0.9) * bump(p);
                    self.tl.zoomed = true;
                }
                _ => {}
            }
        }
        self.extra_changed = true;
        crate::platform::set_hook("tlt", &format!("{:.2}", self.tl.t));
        crate::platform::set_hook("tlplaying", if self.tl.playing { "1" } else { "" });
    }

    /// The sheets the FX in sight draw: read from their files (a blank one for a file that
    /// isn't there yet), sent to the GPU, and again when drawn on.
    pub(super) fn load_fx(&mut self, r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState) {
        if !self.tl.open {
            return;
        }
        let Some(m) = self.tl_move() else { return };
        let content = self.content.clone();
        let mut wanted: Vec<(String, bool, u8, u8, u16)> = vec![];
        for c in m.timeline.clips.iter().filter(|c| c.kind == "fx") {
            let Some(e) = content.fx.entry(&c.what) else { continue };
            let fx = &e.value;
            let editing = self.tl.fx_edit.as_ref().map(|x| x.fx == fx.id).unwrap_or(false);
            for mesh in &fx.meshes {
                if mesh.source.is_empty() {
                    continue;
                }
                wanted.push((self.fx_key(fx, mesh, editing), mesh.pixelated, fx.frames, mesh.columns, fx.size));
            }
        }
        for (key, pixelated, frames, columns, size) in wanted {
            if !self.tl.sheets.contains_key(&key) {
                self.files.request(&key);
                let loaded = match self.files.get(&key) {
                    Load::Pending => continue,
                    Load::Ready(bytes) => match image::load_from_memory(&bytes) {
                        Ok(img) => {
                            let img = img.to_rgba8();
                            Some((img.width(), img.height(), img.into_raw(), false))
                        }
                        Err(e) => {
                            self.status = format!("{key}: {e}");
                            None
                        }
                    },
                    Load::Failed(_) => None,
                };
                // no file yet: a blank sheet, to be drawn on and saved
                let (w, h, rgba, dirty) = loaded.unwrap_or_else(|| {
                    let (w, h, rgba) = world::fx::blank(frames, columns, size);
                    (w, h, rgba, true)
                });
                self.tl.sheets.insert(key.clone(), Sheet { width: w, height: h, rgba, version: 1, uploaded: 0, dirty });
            }
            let s = self.tl.sheets.get_mut(&key).unwrap();
            if s.uploaded != s.version {
                r.add_texture(&rs.device, &rs.queue, &key, &s.rgba, s.width, s.height, !pixelated);
                s.uploaded = s.version;
            }
        }
        let sheets: Vec<String> = self.tl.sheets.iter().map(|(k, s)| format!("{k}:{}x{}:v{}:up{}", s.width, s.height, s.version, s.uploaded)).collect();
        crate::platform::set_hook("fxsheets", &sheets.join(";"));
    }

    // ------------------------------------------------------------ the panel

    pub(super) fn timeline_panel(&mut self, ui: &mut egui::Ui) {
        let content = self.content.clone();
        let Some(m) = self.tl.of.as_deref().and_then(|id| content.moves.get(id)).cloned() else {
            hint(ui, "No ability picked: open one in Abilities and press Edit the timeline.");
            if ui.small_button("Close").clicked() {
                self.tl.open = false;
            }
            return;
        };
        crate::platform::set_hook("timeline", &m.id);
        crate::platform::set_hook("clips", &m.timeline.clips.len().to_string());
        crate::platform::set_hook("tlselected", &self.tl.selected.map(|i| i.to_string()).unwrap_or_default());
        let locked = self.tl.fx_edit.is_some();
        let mut seconds = m.timeline.seconds;
        let mut add: Option<&str> = None;
        let mut remove: Option<usize> = None;
        let mut edited: Option<(usize, Clip)> = None;
        let mut edit_fx: Option<usize> = None;
        let mut make_fx: Option<(usize, String)> = None;
        let mut play_sound: Option<String> = None;
        let mut stand = false;
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Timeline · {}", m.name)).strong());
            let play = ui.add_enabled(!locked, egui::Button::new(if self.tl.playing { "Pause" } else { "Play" }));
            mark("uiTimelinePlay", &play);
            if play.clicked() {
                self.tl.playing = !self.tl.playing;
                if self.tl.playing && self.tl.t >= m.timeline.seconds {
                    self.tl.t = 0.0;
                    self.tl.played.clear();
                }
            }
            if ui.add_enabled(!locked, egui::Button::new("Stop")).clicked() {
                self.tl.playing = false;
                self.tl.t = 0.0;
                self.tl.played.clear();
            }
            ui.checkbox(&mut self.tl.looping, "Loop");
            ui.label(RichText::new(format!("{:.2} s", self.tl.t)).monospace());
            ui.label("of");
            let len = ui.add_enabled(!locked, egui::DragValue::new(&mut seconds).speed(0.05).range(0.1..=60.0).suffix(" s"));
            mark("uiTimelineLength", &len);
            ui.separator();
            ui.label("User");
            self.look_picker(ui, "tl-user", "user", &content);
            ui.label("Target");
            self.look_picker(ui, "tl-target", "target", &content);
            let here = ui.button("Stand here").on_hover_text("The user where the view looks, the target 3 cells on");
            mark("uiStandHere", &here);
            stand = here.clicked();
            ui.add(egui::Slider::new(&mut self.tl.scale, 40.0..=400.0).text("zoom").show_value(false));
            if let Some((word, _)) = &self.tl.judged {
                ui.label(RichText::new(word).strong().color(if word.starts_with("Perfect") { Color32::from_rgb(140, 255, 140) } else if word.starts_with("Good") { Color32::from_rgb(255, 230, 120) } else { Color32::from_rgb(255, 140, 120) }));
            } else if self.tl.playing && m.timeline.clips.iter().any(|c| c.kind == "input") {
                hint(ui, "Z: press on the cue");
            }
            if locked {
                warn(ui, "Held still: an FX is being edited");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = ui.small_button("Close");
                mark("uiCloseTimeline", &close);
                if close.clicked() {
                    self.tl.open = false;
                    self.tl.fx_edit = None;
                }
            });
        });
        if seconds != m.timeline.seconds {
            self.change_clips(|t| t.seconds = seconds);
        }
        // the tracks
        let width = ui.available_width();
        let height = RULER_H + TRACK_H * TRACKS.len() as f32 + 4.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, Color32::from_gray(28));
        let left = rect.left() + LABEL_W;
        let scale = self.tl.scale;
        let to_x = |t: f64| left + t as f32 * scale;
        let to_t = |x: f32| ((x - left) / scale).max(0.0) as f64;
        // the ruler
        let ruler = Rect::from_min_max(Pos2::new(left, rect.top()), Pos2::new(rect.right(), rect.top() + RULER_H));
        let step = if scale > 200.0 { 0.1 } else if scale > 90.0 { 0.25 } else { 0.5 };
        let mut t = 0.0f64;
        while to_x(t) < rect.right() {
            let x = to_x(t);
            let major = (t / 1.0).fract().abs() < 1e-6;
            p.line_segment([Pos2::new(x, ruler.bottom() - if major { 8.0 } else { 4.0 }), Pos2::new(x, ruler.bottom())], Stroke::new(1.0, Color32::from_gray(120)));
            if major {
                p.text(Pos2::new(x + 2.0, ruler.top() + 1.0), egui::Align2::LEFT_TOP, format!("{t:.0} s"), egui::FontId::proportional(10.0), Color32::from_gray(150));
            }
            t += step;
        }
        // the tracks' rows and the clips
        let mut hovered_clip: Option<usize> = None;
        let pointer = resp.hover_pos();
        for (k, (name, _)) in TRACKS.iter().enumerate() {
            let top = rect.top() + RULER_H + k as f32 * TRACK_H;
            let row = Rect::from_min_max(Pos2::new(rect.left(), top), Pos2::new(rect.right(), top + TRACK_H));
            if k % 2 == 1 {
                p.rect_filled(row, 0.0, Color32::from_gray(33));
            }
            p.text(Pos2::new(rect.left() + 6.0, top + TRACK_H / 2.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(11.5), Color32::from_gray(170));
        }
        let end_x = to_x(m.timeline.seconds);
        p.rect_filled(Rect::from_min_max(Pos2::new(end_x.min(rect.right()), rect.top() + RULER_H), rect.right_bottom()), 0.0, Color32::from_black_alpha(90));
        for (i, c) in m.timeline.clips.iter().enumerate() {
            let k = track_of(&c.kind, &c.who);
            let top = rect.top() + RULER_H + k as f32 * TRACK_H + 3.0;
            let len = if c.seconds > 0.0 { c.seconds } else { 0.08 };
            let r = Rect::from_min_max(Pos2::new(to_x(c.at), top), Pos2::new(to_x(c.at + len).max(to_x(c.at) + 6.0), top + TRACK_H - 6.0));
            let r = r.intersect(Rect::from_min_max(Pos2::new(left, rect.top()), rect.right_bottom()));
            let col = clip_color(&c.kind);
            let selected = self.tl.selected == Some(i);
            p.rect_filled(r, 3.0, if selected { col } else { col.gamma_multiply(0.6) });
            if selected {
                p.rect_stroke(r, 3.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
            }
            let label = if c.kind == "animation" || c.kind == "fx" || c.kind == "sound" { c.what.rsplit('/').next().unwrap_or(&c.what).to_string() } else { format!("{} {}", c.what, if c.who == "target" { "(target)" } else { "" }) };
            p.text(Pos2::new(r.left() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(11.0), Color32::BLACK);
            if pointer.map(|q| r.contains(q)).unwrap_or(false) {
                hovered_clip = Some(i);
            }
        }
        // the playhead
        let px = to_x(self.tl.t);
        p.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(2.0, Color32::from_rgb(255, 90, 90)));
        crate::platform::set_hook("uiTimelineTracks", &format!("{:.0},{:.0}", rect.left(), rect.top()));
        crate::platform::set_hook("tlscale", &format!("{},{:.0}", scale, left));
        // the pointer: a clip dragged in time, or the playhead set
        if resp.drag_started() {
            match (hovered_clip, locked) {
                (Some(i), false) => {
                    let at = m.timeline.clips[i].at;
                    self.tl.drag = Some((i, pointer.map(|q| to_t(q.x) - at).unwrap_or(0.0)));
                    self.tl.selected = Some(i);
                }
                _ => self.tl.drag = None,
            }
        }
        if resp.dragged() || resp.clicked() {
            if let Some(q) = pointer {
                match self.tl.drag {
                    Some((i, grab)) => {
                        let at = ((to_t(q.x) - grab).max(0.0) * 20.0).round() / 20.0;
                        if let Some(c) = m.timeline.clips.get(i) {
                            if (c.at - at).abs() > 1e-9 {
                                edited = Some((i, Clip { at, ..c.clone() }));
                            }
                        }
                    }
                    None if !locked => {
                        if resp.clicked() {
                            self.tl.selected = hovered_clip;
                        }
                        if q.x >= left {
                            let t = to_t(q.x).min(m.timeline.seconds);
                            if t < self.tl.t {
                                self.tl.played.clear();
                            }
                            self.tl.t = t;
                            self.extra_changed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.drag_stopped() {
            self.tl.drag = None;
        }
        // adding clips
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Add at the playhead:").size(11.5).color(Color32::from_gray(150)));
            for kind in CLIP_KINDS {
                let b = ui.add_enabled(!locked, egui::Button::new(format!("+ {kind}")).small());
                mark(&format!("uiTimelineAdd_{kind}"), &b);
                if b.clicked() {
                    add = Some(kind);
                }
            }
        });
        // the selected clip
        if let Some((i, c)) = self.tl.selected.and_then(|i| m.timeline.clips.get(i).map(|c| (i, c.clone()))) {
            let mut e = c.clone();
            section(ui, &format!("Clip {i}: {}", c.kind));
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(!locked, |ui| {
                    ui.label("on");
                    for who in ["user", "target"] {
                        if ui.selectable_label(e.who == who, who).clicked() {
                            e.who = who.into();
                        }
                    }
                    ui.label("at");
                    ui.add(egui::DragValue::new(&mut e.at).speed(0.02).range(0.0..=60.0).suffix(" s"));
                    ui.label("for");
                    let r = ui.add(egui::DragValue::new(&mut e.seconds).speed(0.02).range(0.0..=60.0).suffix(" s")).on_hover_text("0: an instant, or the file's own length");
                    mark("uiClipSeconds", &r);
                    match c.kind.as_str() {
                        "animation" => {
                            let names: Vec<String> = self.tl_look(&c.who).strip_prefix("prop:").and_then(|id| self.models.get(id)).map(|m| m.animations.iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
                            ui.label("animation");
                            egui::ComboBox::from_id_salt("clip-anim").selected_text(if e.what.is_empty() { "pick one" } else { &e.what }).width(140.0).show_ui(ui, |ui| {
                                for n in &names {
                                    ui.selectable_value(&mut e.what, n.clone(), n);
                                }
                                if names.is_empty() {
                                    ui.label("the actor's model has none");
                                }
                            });
                            ui.add(egui::TextEdit::singleline(&mut e.what).hint_text("or its name").desired_width(90.0));
                        }
                        "motion" => {
                            ui.label("motion");
                            egui::ComboBox::from_id_salt("clip-motion").selected_text(&e.what).width(100.0).show_ui(ui, |ui| {
                                for n in MOTIONS {
                                    ui.selectable_value(&mut e.what, n.to_string(), n);
                                }
                            });
                            ui.label("how much");
                            ui.add(egui::DragValue::new(&mut e.value).speed(0.05).range(0.0..=10.0)).on_hover_text("Cells (a lunge, a hop, a shake) or a share (a pulse, a flash); 0: the usual");
                        }
                        "fx" => {
                            ui.label("FX");
                            let ids: Vec<String> = content.fx.iter().map(|f| f.id.clone()).collect();
                            egui::ComboBox::from_id_salt("clip-fx").selected_text(if e.what.is_empty() { "pick one" } else { &e.what }).width(140.0).show_ui(ui, |ui| {
                                for n in &ids {
                                    ui.selectable_value(&mut e.what, n.clone(), n);
                                }
                            });
                            let f = ui.add(egui::TextEdit::singleline(&mut self.tl.new_fx).hint_text("a new FX's id").desired_width(110.0));
                            mark("uiNewFxId", &f);
                            let b = ui.small_button("Make it");
                            mark("uiMakeFx", &b);
                            if b.clicked() {
                                make_fx = Some((i, self.tl.new_fx.trim().to_string()));
                            }
                        }
                        "sound" => {
                            ui.label("file");
                            let r = ui.add(egui::TextEdit::singleline(&mut e.what).hint_text("sounds/chime.wav").desired_width(160.0));
                            mark("uiClipFile", &r);
                            ui.label("volume");
                            ui.add(egui::DragValue::new(&mut e.value).speed(0.05).range(0.0..=2.0)).on_hover_text("0: as recorded");
                            if ui.small_button("Play").clicked() {
                                play_sound = Some(e.what.clone());
                            }
                        }
                        "input" => {
                            ui.label("input");
                            egui::ComboBox::from_id_salt("clip-input").selected_text(&e.what).width(100.0).show_ui(ui, |ui| {
                                for n in INPUTS {
                                    ui.selectable_value(&mut e.what, n.to_string(), n);
                                }
                            });
                            ui.label("window");
                            ui.add(egui::DragValue::new(&mut e.value).speed(5.0).range(0.0..=1000.0).suffix(" ms")).on_hover_text("Either side of the beat (0: 120 ms); the beat is at the clip's end");
                            if e.what == "rhythm" {
                                ui.label("beats");
                                ui.add(egui::DragValue::new(&mut e.count).range(1..=12));
                            }
                        }
                        "camera" => {
                            ui.label("camera");
                            egui::ComboBox::from_id_salt("clip-camera").selected_text(&e.what).width(100.0).show_ui(ui, |ui| {
                                for n in CAMERAS {
                                    ui.selectable_value(&mut e.what, n.to_string(), n);
                                }
                            });
                            ui.label("how much");
                            ui.add(egui::DragValue::new(&mut e.value).speed(0.02).range(0.0..=2.0));
                        }
                        _ => {}
                    }
                    let rm = ui.small_button("Remove");
                    mark("uiRemoveClip", &rm);
                    if rm.clicked() {
                        remove = Some(i);
                    }
                });
                if c.kind == "fx" && content.fx.contains(&c.what) {
                    let b = ui.button(if locked { "Editing the FX…" } else { "Edit the FX" }).on_hover_text("Hold the timeline still here and place the FX's meshes round the actor, drawing their frames");
                    mark("uiEditFx", &b);
                    if b.clicked() && !locked {
                        edit_fx = Some(i);
                    }
                }
            });
            if e != c {
                edited = Some((i, e));
            }
        } else {
            hint(ui, "Click a clip to change it, drag it along its track; the arrows under the tracks add one at the playhead.");
        }
        // what the panel asked for
        if stand {
            self.stand_here();
        }
        if let Some(kind) = add {
            let at = (self.tl.t * 20.0).round() / 20.0;
            let who = if kind == "input" { "target" } else { "user" };
            let (what, seconds, value) = match kind {
                "motion" => ("lunge", 0.8, 0.0),
                "fx" => ("", 0.0, 0.0),
                "sound" => ("sounds/chime.wav", 0.0, 0.0),
                "input" => ("tap", 0.6, 120.0),
                "camera" => ("shake", 0.4, 0.3),
                _ => ("", 1.0, 0.0),
            };
            let n = m.timeline.clips.len();
            let clip = Clip { kind: kind.into(), who: who.into(), at, seconds, what: what.into(), value, count: if kind == "input" { 3 } else { 0 } };
            self.change_clips(|t| {
                t.clips.push(clip);
                if t.seconds < at + seconds + 0.5 {
                    t.seconds = (at + seconds + 0.5).max(1.0);
                }
            });
            self.tl.selected = Some(n);
        }
        if let Some((i, c)) = edited {
            self.change_clips(move |t| {
                if let Some(x) = t.clips.get_mut(i) {
                    *x = c;
                }
            });
        }
        if let Some(i) = remove {
            self.change_clips(move |t| {
                if i < t.clips.len() {
                    t.clips.remove(i);
                }
            });
            self.tl.selected = None;
        }
        if let Some((i, id)) = make_fx {
            self.make_fx(i, &id);
        }
        if let Some(file) = play_sound {
            let space = content.moves.entry(&m.id).map(|e| e.space).unwrap_or(Space::Temporary);
            self.tl.shots.push((space.asset(&file), 1.0));
        }
        if let Some(i) = edit_fx {
            if let Some(c) = m.timeline.clips.get(i) {
                self.tl.playing = false;
                self.tl.selected = Some(i);
                self.tl.fx_edit = Some(FxEdit { clip: i, fx: c.what.clone(), mesh: 0, frame: 0, brush: 8.0, color: [255, 240, 120], eraser: false, onion: true, playing: false, undo: vec![], last: None, tex: None, shown: (0, 255, false) });
            }
        }
        crate::platform::set_hook("fxedit", &self.tl.fx_edit.as_ref().map(|e| e.fx.clone()).unwrap_or_default());
        crate::platform::set_hook("fxcount", &content.fx.len().to_string());
        crate::platform::set_hook("fxmeshes", &self.tl.fx_edit.as_ref().and_then(|e| content.fx.get(&e.fx)).map(|f| f.meshes.len().to_string()).unwrap_or_default());
    }

    /// A combo of what someone can look like: props with a model (the rigged ones first),
    /// and creatures (their pictures).
    fn look_picker(&mut self, ui: &mut egui::Ui, salt: &str, who: &str, content: &content::Content) {
        let current = self.tl_look(who).to_string();
        let shown = current.split_once(':').map(|(_, id)| id.to_string()).unwrap_or_else(|| current.clone());
        let mut picked: Option<String> = None;
        egui::ComboBox::from_id_salt(salt).selected_text(shown).width(150.0).show_ui(ui, |ui| {
            let mut props: Vec<&content::Prop> = content.props.iter().filter(|p| p.model.is_some() || p.id == "stand_in_character").collect();
            props.sort_by_key(|p| (!self.models.get(&p.id).map(|m| m.rig.is_some()).unwrap_or(false), p.name.clone()));
            for p in props {
                let rigged = self.models.get(&p.id).map(|m| m.rig.is_some()).unwrap_or(false);
                let look = format!("prop:{}", p.id);
                if ui.selectable_label(current == look, format!("{}{}", p.name, if rigged { " (rigged)" } else { "" })).clicked() {
                    picked = Some(look);
                }
            }
            ui.separator();
            for c in content.creatures.iter().take(60) {
                let look = format!("creature:{}", c.id);
                if ui.selectable_label(current == look, &c.name).clicked() {
                    picked = Some(look);
                }
            }
        });
        if let Some(look) = picked {
            if who == "target" {
                self.tl.target = look;
            } else {
                self.tl.user = look;
            }
            self.extra_changed = true;
        }
    }

    // ------------------------------------------------------------ FX records

    fn change_fx(&mut self, id: &str, change: impl FnOnce(&mut Fx)) {
        let Some(e) = self.content.fx.entry(id) else { return };
        let (mut f, space) = (e.value.clone(), e.space);
        change(&mut f);
        if &f == self.content.fx.get(id).unwrap() {
            return;
        }
        let mut c = (*self.content).clone();
        c.fx.put(f, space);
        self.content = Arc::new(c);
        self.tl.fx_dirty.insert(id.to_string());
        self.extra_changed = true;
    }

    /// A new FX record with one billboard, named by an fx clip.
    fn make_fx(&mut self, clip: usize, id: &str) {
        if !slug_ok(id) || self.content.fx.contains(id) {
            self.status = "An FX's id: lowercase letters, digits, _ or -, and not taken".into();
            return;
        }
        let space = if self.abilities.to_temporary { Space::Temporary } else { Space::Content };
        let fx = Fx { id: id.to_string(), name: super::assets::title(id), meshes: vec![FxMesh { source: format!("fx/{id}_0.png"), ..Default::default() }], ..Default::default() };
        let mut c = (*self.content).clone();
        c.fx.put(fx, space);
        self.content = Arc::new(c);
        self.tl.fx_dirty.insert(id.to_string());
        self.tl.new_fx.clear();
        let id = id.to_string();
        self.change_clips(move |t| {
            if let Some(c) = t.clips.get_mut(clip) {
                c.what = id;
            }
        });
        self.status = format!("{}: a new FX with one billboard, for {}", self.tl.new_fx, space.dir());
    }

    /// Writes the FX changed here: records and the sheets drawn on, into their space on
    /// the desktop, as downloads in the browser.
    pub(super) fn save_fx(&mut self) {
        let ids: Vec<String> = self.tl.fx_dirty.iter().cloned().collect();
        let mut done = 0;
        let mut errors = vec![];
        // the sheets of these FX that were drawn on
        let mut sheets: Vec<(String, Space)> = vec![];
        for id in &ids {
            if let Some(e) = self.content.fx.entry(id) {
                for m in &e.value.meshes {
                    for file in [&m.source, &m.sheet] {
                        let key = e.space.asset(file);
                        if !file.is_empty() && self.tl.sheets.get(&key).map(|s| s.dirty).unwrap_or(false) {
                            sheets.push((key, e.space));
                        }
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            for id in &ids {
                let Some(e) = self.content.fx.entry(id) else { continue };
                match content::disk::save(&root, e.space, &e.value) {
                    Ok(_) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            for (key, _) in &sheets {
                let Some(s) = self.tl.sheets.get_mut(key) else { continue };
                let path = root.join(key);
                let written = std::fs::create_dir_all(path.parent().unwrap_or(&root)).map_err(|e| e.to_string()).and_then(|_| png(s.width, s.height, &s.rgba)).and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
                match written {
                    Ok(()) => s.dirty = false,
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                }
            }
            self.status = format!("Saved {done} FX record{} and {} sheet{}", if done == 1 { "" } else { "s" }, sheets.len(), if sheets.len() == 1 { "" } else { "s" });
        }
        #[cfg(target_arch = "wasm32")]
        {
            for id in &ids {
                let Some(e) = self.content.fx.entry(id) else { continue };
                match crate::platform::download(&format!("{id}.json"), &content::to_text(&e.value)) {
                    Ok(()) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            for (key, _) in &sheets {
                let Some(s) = self.tl.sheets.get_mut(key) else { continue };
                let name = key.rsplit('/').next().unwrap_or(key).to_string();
                match png(s.width, s.height, &s.rgba).and_then(|bytes| crate::platform::download_bytes(&name, &bytes, "image/png")) {
                    Ok(()) => s.dirty = false,
                    Err(e) => errors.push(format!("{name}: {e}")),
                }
            }
            self.status = format!("Downloaded {done} FX record{} (into <space>/data/fx/) and {} sheet{} (into <space>/assets/fx/)", if done == 1 { "" } else { "s" }, sheets.len(), if sheets.len() == 1 { "" } else { "s" });
        }
        if errors.is_empty() {
            self.tl.fx_dirty.clear();
        } else {
            self.status = format!("{} · not saved: {}", self.status, errors.join("; "));
        }
    }

    /// Every mesh's frames packed at the export size, as the flipbooks the game draws.
    fn export_fx(&mut self, id: &str) {
        let Some(e) = self.content.fx.entry(id).cloned() else { return };
        let fx = e.value;
        let mut sheets: Vec<(usize, String)> = vec![];
        for (n, m) in fx.meshes.iter().enumerate() {
            let key = e.space.asset(&m.source);
            let Some(s) = self.tl.sheets.get(&key) else { continue };
            let (w, h, rgba) = world::fx::export(&s.rgba, s.width, fx.frames, m.columns, fx.size, fx.export_size);
            let file = format!("fx/{id}_{n}_x{}.png", fx.export_size);
            let out_key = e.space.asset(&file);
            self.tl.sheets.insert(out_key, Sheet { width: w, height: h, rgba, version: 1, uploaded: 0, dirty: true });
            sheets.push((n, file));
        }
        let count = sheets.len();
        self.change_fx(id, move |f| {
            for (n, file) in sheets {
                if let Some(m) = f.meshes.get_mut(n) {
                    m.sheet = file;
                }
            }
        });
        self.tl.fx_dirty.insert(id.to_string());
        self.save_fx();
        self.status = format!("{id}: {count} flipbook{} exported at {} px a frame · {}", if count == 1 { "" } else { "s" }, fx.export_size, self.status);
    }

    // ------------------------------------------------------------ the FX editor

    /// The FX editor and the Drawing window, while an FX is edited.
    pub(super) fn fx_windows(&mut self, ctx: &egui::Context) {
        let Some(edit) = self.tl.fx_edit.as_ref() else { return };
        let content = self.content.clone();
        let Some(e) = content.fx.entry(&edit.fx) else {
            self.tl.fx_edit = None;
            return;
        };
        let fx = e.value.clone();
        let space = e.space;
        let mut after = fx.clone();
        let (mut done, mut export, mut save) = (false, false, false);
        let mut open = true;
        let mesh_i = edit.mesh.min(fx.meshes.len().saturating_sub(1));
        let mut pick_mesh: Option<usize> = None;
        let mut add_shape: Option<String> = None;
        let mut remove_mesh: Option<usize> = None;
        egui::Window::new(format!("FX editor · {}", fx.name)).id(egui::Id::new("fx-editor")).open(&mut open).resizable(false).default_width(330.0).default_pos([258.0, 44.0]).show(ctx, |ui| {
            hint(ui, "The timeline holds still on this FX. Its meshes stand round the actor: drag one on the map to move it along the ground; its height, size and turn are here.");
            egui::Grid::new("fx-grid").num_columns(6).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Frames");
                let r = ui.add(egui::DragValue::new(&mut after.frames).range(1..=64));
                mark("uiFxFrames", &r);
                ui.label("a second");
                ui.add(egui::DragValue::new(&mut after.fps).speed(0.5).range(1.0..=60.0));
                ui.label("Pixels");
                egui::ComboBox::from_id_salt("fx-size").selected_text(after.size.to_string()).width(60.0).show_ui(ui, |ui| {
                    for s in [32u16, 64, 128, 256, 512] {
                        ui.selectable_value(&mut after.size, s, s.to_string());
                    }
                });
                ui.end_row();
            });
            section(ui, "Meshes");
            for (n, m) in fx.meshes.iter().enumerate() {
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(n == mesh_i, format!("{n}: {}", m.shape));
                    mark(&format!("uiFxMesh{n}"), &r);
                    if r.clicked() {
                        pick_mesh = Some(n);
                    }
                    if ui.small_button("×").clicked() {
                        remove_mesh = Some(n);
                    }
                });
            }
            ui.horizontal(|ui| {
                for s in FX_SHAPES {
                    let b = ui.small_button(format!("+ {s}"));
                    mark(&format!("uiFxAdd_{s}"), &b);
                    if b.clicked() {
                        add_shape = Some(s.to_string());
                    }
                }
            });
            if let Some(m) = after.meshes.get_mut(mesh_i) {
                section(ui, &format!("Mesh {mesh_i}: {}", m.shape));
                egui::Grid::new("fx-mesh-grid").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("At");
                    ui.horizontal(|ui| {
                        for (k, v) in m.at.iter_mut().enumerate() {
                            let r = ui.add(egui::DragValue::new(v).speed(0.02).range(-10.0..=10.0));
                            if k == 1 {
                                mark("uiFxHeight", &r);
                            }
                        }
                    });
                    ui.label("Size");
                    ui.horizontal(|ui| {
                        for v in m.size.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(0.02).range(0.05..=20.0));
                        }
                    });
                    ui.end_row();
                    ui.label("Turn");
                    ui.horizontal(|ui| {
                        for v in m.turn.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(1.0).range(-180.0..=180.0).suffix("°"));
                        }
                    });
                    ui.label("Columns");
                    ui.add(egui::DragValue::new(&mut m.columns).range(1..=16)).on_hover_text("Frames across its sheet");
                    ui.end_row();
                    ui.label("");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut m.pixelated, "pixel-sharp");
                        ui.checkbox(&mut m.lit, "lit").on_hover_text("Lit like the world; off: full bright, as drawn");
                    });
                    ui.end_row();
                });
                hint(ui, "x right, y up, z forward from the actor's feet, in cells: a little in front of it, or its body hides it.");
            }
            ui.separator();
            ui.horizontal(|ui| {
                let s = ui.button("Save the FX").on_hover_text("Its record and the sheets drawn on (Ctrl+S does too)");
                mark("uiFxSave", &s);
                save = s.clicked();
                let x = ui.button("Export the flipbooks").on_hover_text(format!("Every mesh's frames packed at {} px a frame, for the game to draw", fx.export_size));
                mark("uiFxExport", &x);
                export = x.clicked();
                ui.label("at");
                egui::ComboBox::from_id_salt("fx-export-size").selected_text(format!("{} px", after.export_size)).width(70.0).show_ui(ui, |ui| {
                    for s in [32u16, 64, 128, 256] {
                        ui.selectable_value(&mut after.export_size, s, format!("{s} px"));
                    }
                });
                let d = ui.button("Done").on_hover_text("Back to the timeline");
                mark("uiFxDone", &d);
                done = d.clicked();
            });
        });
        if let Some(n) = pick_mesh {
            if let Some(e) = &mut self.tl.fx_edit {
                e.mesh = n;
                e.undo.clear();
            }
        }
        if let Some(shape) = add_shape {
            let n = after.meshes.len();
            // in front of the actor (a ring on the ground round it)
            let at = if shape == "ring" { [0.0, 0.05, 0.0] } else { [0.0, 0.8, 0.6] };
            after.meshes.push(FxMesh { shape, at, source: format!("fx/{}_{n}.png", fx.id), ..Default::default() });
            if let Some(e) = &mut self.tl.fx_edit {
                e.mesh = n;
            }
        }
        if let Some(n) = remove_mesh {
            if n < after.meshes.len() {
                after.meshes.remove(n);
            }
        }
        if after != fx {
            // a sheet's layout changing (frames, size, columns) leaves what was drawn behind
            let relaid = after.frames != fx.frames || after.size != fx.size || after.meshes.iter().zip(&fx.meshes).any(|(a, b)| a.columns != b.columns);
            if relaid {
                for m in &fx.meshes {
                    self.tl.sheets.remove(&space.asset(&m.source));
                }
                if let Some(e) = &mut self.tl.fx_edit {
                    e.frame = e.frame.min(after.frames.saturating_sub(1));
                    e.undo.clear();
                }
            }
            let id = fx.id.clone();
            self.change_fx(&id, move |f| *f = after);
        }
        if save {
            self.save_fx();
        }
        if export {
            let id = fx.id.clone();
            self.export_fx(&id);
        }
        if done || !open {
            self.tl.fx_edit = None;
            return;
        }
        self.drawing_window(ctx);
    }

    /// The Drawing window: the mesh's frame, painted with the pointer, the frames before
    /// and after ghosted.
    fn drawing_window(&mut self, ctx: &egui::Context) {
        let content = self.content.clone();
        let Some(edit) = self.tl.fx_edit.as_ref() else { return };
        let Some(e) = content.fx.entry(&edit.fx) else { return };
        let fx = &e.value;
        let Some(mesh) = fx.meshes.get(edit.mesh.min(fx.meshes.len().saturating_sub(1))) else { return };
        let key = e.space.asset(&mesh.source);
        let frames = fx.frames.max(1);
        let size = fx.size;
        let columns = mesh.columns.max(1);
        let mut copy_previous = false;
        let mut clear = false;
        let mut undo = false;
        let mut step: i32 = 0;
        let Timelines { sheets, fx_edit, .. } = &mut self.tl;
        let Some(edit) = fx_edit.as_mut() else { return };
        egui::Window::new("Drawing").id(egui::Id::new("fx-drawing")).resizable(false).default_pos([980.0, 44.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let prev = ui.small_button("◀");
                mark("uiFramePrev", &prev);
                if prev.clicked() {
                    step = -1;
                }
                ui.label(RichText::new(format!("frame {} / {frames}", edit.frame + 1)).monospace());
                let next = ui.small_button("▶");
                mark("uiFrameNext", &next);
                if next.clicked() {
                    step = 1;
                }
                ui.checkbox(&mut edit.onion, "Ghosts").on_hover_text("The frame before (blue) and after (crimson), faintly");
                if ui.selectable_label(edit.playing, "Play").on_hover_text("The frames one after the other, here and on the map").clicked() {
                    edit.playing = !edit.playing;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Brush");
                for b in BRUSHES {
                    if ui.selectable_label((edit.brush - b).abs() < 0.1, format!("{b:.0}")).clicked() {
                        edit.brush = b;
                    }
                }
                let eraser = ui.selectable_label(edit.eraser, "Eraser");
                mark("uiEraser", &eraser);
                if eraser.clicked() {
                    edit.eraser = !edit.eraser;
                }
                ui.color_edit_button_srgb(&mut edit.color);
                for c in [[255u8, 240, 120], [255, 120, 60], [255, 255, 255], [120, 200, 255], [60, 40, 70]] {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::click());
                    ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                    if resp.clicked() {
                        edit.color = c;
                        edit.eraser = false;
                    }
                }
            });
            ui.horizontal(|ui| {
                let u = ui.add_enabled(!edit.undo.is_empty(), egui::Button::new("Undo").small());
                mark("uiDrawUndo", &u);
                undo = u.clicked();
                clear = ui.small_button("Clear").clicked();
                let cp = ui.small_button("Copy the frame before").on_hover_text("This frame becomes a copy of the one before it");
                mark("uiCopyPrevious", &cp);
                copy_previous = cp.clicked();
            });
            // the canvas
            let zoom = (384.0 / size as f32).max(1.0);
            let side = size as f32 * zoom;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click_and_drag());
            crate::platform::set_hook("fxcanvas", &format!("{:.0},{:.0},{:.0},{:.0}", rect.left(), rect.top(), rect.width(), rect.height()));
            let Some(sheet) = sheets.get_mut(&key) else {
                ui.painter().rect_filled(rect, 4.0, Color32::from_gray(40));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "the sheet is on its way", egui::FontId::proportional(13.0), Color32::GRAY);
                return;
            };
            let frame = edit.frame.min(frames - 1);
            let origin = world::fx::frame_at(frame, columns, size);
            // painting
            let to_frame = |p: Pos2| ((p.x - rect.left()) / zoom, (p.y - rect.top()) / zoom);
            if resp.drag_started() || (resp.clicked() && resp.hover_pos().is_some()) {
                edit.undo.push((frame, frame_pixels(sheet, origin, size)));
                if edit.undo.len() > 40 {
                    edit.undo.remove(0);
                }
                edit.last = None;
            }
            if (resp.dragged() || resp.clicked()) && resp.hover_pos().is_some() {
                let (x, y) = to_frame(resp.hover_pos().unwrap());
                let (lx, ly) = edit.last.unwrap_or((x, y));
                let steps = (((x - lx).powi(2) + (y - ly).powi(2)).sqrt() / (edit.brush * 0.25).max(0.5)).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let f = k as f32 / steps as f32;
                    stamp(sheet, origin, size, lx + (x - lx) * f, ly + (y - ly) * f, edit.brush / 2.0, if edit.eraser { None } else { Some(edit.color) });
                }
                edit.last = Some((x, y));
                sheet.version += 1;
                sheet.dirty = true;
            }
            if resp.drag_stopped() {
                edit.last = None;
            }
            // what's shown: a checkerboard, the ghosts, the frame
            if edit.shown != (sheet.version, frame, edit.onion) || edit.tex.is_none() {
                let mut img = vec![0u8; (size as usize) * (size as usize) * 4];
                let cell = |k: u8| world::fx::frame_at(k, columns, size);
                let ghosts: Vec<((u32, u32), [u8; 3])> = if edit.onion {
                    let mut g = vec![];
                    if frame > 0 {
                        g.push((cell(frame - 1), [110u8, 150, 255]));
                    }
                    if frame + 1 < frames {
                        g.push((cell(frame + 1), [255u8, 120, 110]));
                    }
                    g
                } else {
                    vec![]
                };
                for y in 0..size as u32 {
                    for x in 0..size as u32 {
                        let o = ((y * size as u32 + x) * 4) as usize;
                        let check = if ((x / 8) + (y / 8)) % 2 == 0 { 70 } else { 58 };
                        let mut px = [check, check, check, 255u8];
                        for (c, tint) in &ghosts {
                            let i = (((c.1 + y) * sheet.width + c.0 + x) * 4) as usize;
                            if let Some(s) = sheet.rgba.get(i..i + 4) {
                                if s[3] > 0 {
                                    for k in 0..3 {
                                        px[k] = ((px[k] as u32 * 2 + tint[k] as u32) / 3) as u8;
                                    }
                                }
                            }
                        }
                        let i = (((origin.1 + y) * sheet.width + origin.0 + x) * 4) as usize;
                        if let Some(s) = sheet.rgba.get(i..i + 4) {
                            if s[3] > 0 {
                                let a = s[3] as u32;
                                for k in 0..3 {
                                    px[k] = ((s[k] as u32 * a + px[k] as u32 * (255 - a)) / 255) as u8;
                                }
                            }
                        }
                        img[o..o + 4].copy_from_slice(&px);
                    }
                }
                let image = egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &img);
                match &mut edit.tex {
                    Some(t) => t.set(image, egui::TextureOptions::NEAREST),
                    None => edit.tex = Some(ctx.load_texture("fx-canvas", image, egui::TextureOptions::NEAREST)),
                }
                edit.shown = (sheet.version, frame, edit.onion);
            }
            if let Some(t) = &edit.tex {
                ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Outside);
            // the brush's reach, under the pointer
            if let Some(p) = resp.hover_pos() {
                ui.painter().circle_stroke(p, edit.brush / 2.0 * zoom, Stroke::new(1.0, if edit.eraser { Color32::from_rgb(255, 120, 120) } else { Color32::WHITE }));
            }
            crate::platform::set_hook("fxpainted", &painted(sheet, origin, size).to_string());
            crate::platform::set_hook("fxframe", &frame.to_string());
            // the asks, on the sheet
            if undo {
                if let Some((f, pixels)) = edit.undo.pop() {
                    let o = world::fx::frame_at(f, columns, size);
                    put_frame(sheet, o, size, &pixels);
                    sheet.version += 1;
                    sheet.dirty = true;
                }
            }
            if clear {
                edit.undo.push((frame, frame_pixels(sheet, origin, size)));
                put_frame(sheet, origin, size, &vec![0; (size as usize) * (size as usize) * 4]);
                sheet.version += 1;
                sheet.dirty = true;
            }
            if copy_previous && frame > 0 {
                edit.undo.push((frame, frame_pixels(sheet, origin, size)));
                let before = frame_pixels(sheet, world::fx::frame_at(frame - 1, columns, size), size);
                put_frame(sheet, origin, size, &before);
                sheet.version += 1;
                sheet.dirty = true;
            }
            if step != 0 {
                edit.frame = ((edit.frame as i32 + step).rem_euclid(frames as i32)) as u8;
                edit.last = None;
            }
        });
        // the frames playing, in the window and on the map
        if let Some(edit) = self.tl.fx_edit.as_mut() {
            if edit.playing {
                let k = ((ctx.input(|i| i.time) * fx.fps.max(0.1)) as u64 % frames as u64) as u8;
                if k != edit.frame {
                    edit.frame = k;
                    edit.last = None;
                }
                ctx.request_repaint();
            }
        }
        self.extra_changed = true;
    }

    /// A drag on the map while an FX is edited: the picked mesh moves along the ground, its
    /// height kept.
    pub(super) fn fx_view_input(&mut self, origin: Vec3, dir: Vec3, primary_down: bool) {
        if !primary_down {
            return;
        }
        let Some(edit) = self.tl.fx_edit.as_ref() else { return };
        let (feet, facing, ..) = self.tl_pose(&self.content.moves.get(self.tl.of.as_deref().unwrap_or("")).and_then(|m| m.timeline.clips.get(edit.clip)).map(|c| c.who.clone()).unwrap_or_else(|| "user".into()), self.tl.t);
        let Some(fx) = self.content.fx.get(&edit.fx) else { return };
        let Some(mesh) = fx.meshes.get(edit.mesh) else { return };
        let y = feet.y + mesh.at[1] as f32;
        let Some(hit) = Self::plane(origin, dir, y) else { return };
        // back into the actor's frame: z forward, x right
        let back = placement(feet, facing, 1.0).inverse().transform_point3(Vec3::new(hit.x, y, hit.z));
        let at = [(back.x as f64 * 50.0).round() / 50.0, mesh.at[1], (back.z as f64 * 50.0).round() / 50.0];
        if at != mesh.at {
            let (id, n) = (edit.fx.clone(), edit.mesh);
            self.change_fx(&id, move |f| {
                if let Some(m) = f.meshes.get_mut(n) {
                    m.at = at;
                }
            });
        }
    }
}

/// A frame's pixels copied out of a sheet.
fn frame_pixels(sheet: &Sheet, origin: (u32, u32), size: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity((size as usize) * (size as usize) * 4);
    for y in 0..size as u32 {
        let i = (((origin.1 + y) * sheet.width + origin.0) * 4) as usize;
        out.extend_from_slice(sheet.rgba.get(i..i + size as usize * 4).unwrap_or(&[]));
    }
    out.resize((size as usize) * (size as usize) * 4, 0);
    out
}

/// Pixels put back into a frame of a sheet.
fn put_frame(sheet: &mut Sheet, origin: (u32, u32), size: u16, pixels: &[u8]) {
    for y in 0..size as u32 {
        let i = (((origin.1 + y) * sheet.width + origin.0) * 4) as usize;
        let row = (y as usize) * size as usize * 4;
        if let (Some(dst), Some(src)) = (sheet.rgba.get_mut(i..i + size as usize * 4), pixels.get(row..row + size as usize * 4)) {
            dst.copy_from_slice(src);
        }
    }
}

/// How many pixels of a frame are drawn on.
fn painted(sheet: &Sheet, origin: (u32, u32), size: u16) -> usize {
    frame_pixels(sheet, origin, size).chunks(4).filter(|p| p[3] > 0).count()
}
