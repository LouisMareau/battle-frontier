//! An ability's timeline played out: where a motion clip puts whoever it's on at a moment,
//! which frame an fx clip shows, where an input clip's beats fall, how the camera moves,
//! and when the ability lands. The map editor's preview and the battle's stage both read
//! it, so an ability looks the same in both.

use content::{Clip, Fx, Timeline};
use glam::Vec3;

/// How far along a clip a moment is, 0 to 1 (None outside it); a clip with no length lasts
/// a twentieth of a second.
pub fn along(c: &Clip, t: f64) -> Option<f32> {
    let len = c.seconds.max(0.05);
    (t >= c.at && t <= c.at + len).then(|| ((t - c.at) / len) as f32)
}

/// Up and back down over a clip: 0 at its ends, 1 in the middle.
pub fn bump(p: f32) -> f32 {
    (p * std::f32::consts::PI).sin().max(0.0)
}

/// Where the motions put whoever they're on: moved by `offset`, scaled, flashing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub offset: Vec3,
    pub scale: f32,
    pub flash: f32,
}

impl Default for Pose {
    fn default() -> Pose {
        Pose { offset: Vec3::ZERO, scale: 1.0, flash: 0.0 }
    }
}

/// The motions on `who` ("user" or "target") at `t`, added up. `towards` is the way to the
/// other one, `apart` how far it stands: a lunge with no reach of its own stops a cell
/// short of it.
pub fn pose(clips: &[Clip], who: &str, t: f64, towards: Vec3, apart: f32) -> Pose {
    let mut pose = Pose::default();
    let towards = towards.normalize_or(Vec3::Z);
    for c in clips.iter().filter(|c| c.kind == "motion" && c.who == who) {
        let Some(p) = along(c, t) else { continue };
        let value = c.value as f32;
        match c.what.as_str() {
            "lunge" => {
                let reach = if value > 0.0 { value } else { (apart - 1.0).max(0.5) };
                // a wind-back, the dash, a hold, and back
                let k = if p < 0.2 {
                    -0.2 * (p / 0.2)
                } else if p < 0.45 {
                    -0.2 + 1.2 * ((p - 0.2) / 0.25)
                } else if p < 0.7 {
                    1.0
                } else {
                    1.0 - (p - 0.7) / 0.3
                };
                pose.offset += towards * reach * k;
            }
            "hop" => pose.offset.y += (if value > 0.0 { value } else { 0.6 }) * bump(p),
            "shake" => pose.offset += towards.cross(Vec3::Y).normalize_or(Vec3::X) * (if value > 0.0 { value } else { 0.15 }) * (p * 50.0).sin() * (1.0 - p),
            "pulse" => pose.scale += (if value > 0.0 { value } else { 0.2 }) * bump(p),
            "flash" => pose.flash = pose.flash.max((if value > 0.0 { value } else { 1.0 }) * (1.0 - p)),
            _ => {}
        }
    }
    pose
}

/// The frame an fx clip shows at `t`: looping while the clip lasts (a clip with no length
/// plays the frames once); None before and after.
pub fn fx_frame(c: &Clip, fx: &Fx, t: f64) -> Option<u8> {
    let since = t - c.at;
    if since < 0.0 {
        return None;
    }
    let fps = fx.fps.max(0.1);
    let frames = fx.frames.max(1);
    let len = if c.seconds > 0.0 { c.seconds } else { frames as f64 / fps };
    if since > len {
        return None;
    }
    Some(((since * fps) as u64 % frames as u64) as u8)
}

/// An input clip's beats, in seconds from the start: at its end, or `count` of them spread
/// over it for a rhythm.
pub fn beats(c: &Clip) -> Vec<f64> {
    let lead = c.seconds.max(0.1);
    match c.what.as_str() {
        "rhythm" => (0..c.count.max(1)).map(|k| c.at + lead * (k + 1) as f64 / c.count.max(1) as f64).collect(),
        _ => vec![c.at + lead],
    }
}

/// An input clip's window, in milliseconds: its own, or `usual`.
pub fn window(c: &Clip, usual: f64) -> f64 {
    if c.value > 0.0 { c.value } else { usual }
}

/// The camera at `t`: shaken by an offset, and zoomed (times its distance; 1 is none).
pub fn camera(clips: &[Clip], t: f64) -> (Vec3, f32) {
    let (mut shake, mut zoom) = (Vec3::ZERO, 1.0f32);
    for c in clips.iter().filter(|c| c.kind == "camera") {
        let Some(p) = along(c, t) else { continue };
        match c.what.as_str() {
            "shake" => {
                let k = (if c.value > 0.0 { c.value as f32 } else { 0.3 }) * (1.0 - p);
                shake += Vec3::new((p * 97.0).sin() * k, 0.0, (p * 131.0).cos() * k);
            }
            "zoom" => zoom *= 1.0 - (if c.value > 0.0 { c.value as f32 } else { 0.3 }).clamp(0.0, 0.9) * bump(p),
            _ => {}
        }
    }
    (shake, zoom)
}

/// When the ability lands, in seconds from the start: its first input's first beat, else
/// the first flash or shake on the target, else its end.
pub fn impact(timeline: &Timeline) -> f64 {
    let clips = &timeline.clips;
    if let Some(c) = clips.iter().find(|c| c.kind == "input") {
        return beats(c)[0];
    }
    clips
        .iter()
        .filter(|c| c.kind == "motion" && c.who == "target" && matches!(c.what.as_str(), "flash" | "shake"))
        .map(|c| c.at)
        .fold(None, |m: Option<f64>, a| Some(m.map_or(a, |m| m.min(a))))
        .unwrap_or(timeline.seconds.max(0.05))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(kind: &str, who: &str, at: f64, seconds: f64, what: &str, value: f64, count: u8) -> Clip {
        Clip { kind: kind.into(), who: who.into(), at, seconds, what: what.into(), value, count }
    }

    #[test]
    fn a_lunge_goes_towards_the_other_and_back() {
        let clips = [clip("motion", "user", 0.0, 1.0, "lunge", 0.0, 0)];
        let at = |t: f64| pose(&clips, "user", t, Vec3::Z, 5.0).offset;
        assert_eq!(at(-0.1), Vec3::ZERO, "not yet");
        assert!(at(0.1).z < 0.0, "a wind-back first");
        assert!((at(0.55).z - 4.0).abs() < 1e-4, "a cell short of the other, held");
        assert!(at(1.0).length() < 1e-4, "back at the end");
        assert_eq!(pose(&clips, "target", 0.5, Vec3::NEG_Z, 5.0).offset, Vec3::ZERO, "the target stays");
        // a hop lifts, a pulse grows, a flash fades
        let more = [clip("motion", "user", 0.0, 1.0, "hop", 0.0, 0), clip("motion", "user", 0.0, 1.0, "pulse", 0.5, 0), clip("motion", "user", 0.0, 1.0, "flash", 0.0, 0)];
        let p = pose(&more, "user", 0.5, Vec3::Z, 5.0);
        assert!((p.offset.y - 0.6).abs() < 1e-4 && (p.scale - 1.5).abs() < 1e-4 && (p.flash - 0.5).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn fx_frames_loop_while_the_clip_lasts() {
        let fx = Fx { fps: 10.0, frames: 4, ..Default::default() };
        let looping = clip("fx", "user", 1.0, 1.0, "zap", 0.0, 0);
        assert_eq!(fx_frame(&looping, &fx, 0.9), None);
        assert_eq!(fx_frame(&looping, &fx, 1.0), Some(0));
        assert_eq!(fx_frame(&looping, &fx, 1.25), Some(2));
        assert_eq!(fx_frame(&looping, &fx, 1.65), Some(2), "round again");
        assert_eq!(fx_frame(&looping, &fx, 2.1), None);
        // no length: the frames once
        let once = clip("fx", "user", 0.0, 0.0, "zap", 0.0, 0);
        assert_eq!(fx_frame(&once, &fx, 0.35), Some(3));
        assert_eq!(fx_frame(&once, &fx, 0.5), None);
    }

    #[test]
    fn beats_windows_and_the_impact() {
        let tap = clip("input", "target", 0.3, 0.5, "tap", 150.0, 0);
        assert_eq!(beats(&tap), vec![0.8]);
        assert_eq!(window(&tap, 110.0), 150.0);
        let rhythm = clip("input", "target", 1.0, 0.9, "rhythm", 0.0, 3);
        let b = beats(&rhythm);
        assert_eq!(b.len(), 3);
        assert!((b[0] - 1.3).abs() < 1e-9 && (b[2] - 1.9).abs() < 1e-9, "{b:?}");
        assert_eq!(window(&rhythm, 110.0), 110.0);
        let tl = Timeline { seconds: 2.0, clips: vec![clip("motion", "user", 0.0, 0.8, "lunge", 0.0, 0), tap.clone()] };
        assert!((impact(&tl) - 0.8).abs() < 1e-9);
        let silent = Timeline { seconds: 2.0, clips: vec![clip("motion", "target", 1.1, 0.3, "flash", 0.0, 0)] };
        assert!((impact(&silent) - 1.1).abs() < 1e-9);
        assert!((impact(&Timeline { seconds: 1.5, clips: vec![] }) - 1.5).abs() < 1e-9);
        // the camera shakes and zooms
        let cam = [clip("camera", "user", 0.0, 1.0, "shake", 0.5, 0), clip("camera", "user", 0.0, 1.0, "zoom", 0.4, 0)];
        let (shake, zoom) = camera(&cam, 0.5);
        assert!(shake.length() > 0.0 && (zoom - 0.6).abs() < 1e-4, "{shake:?} {zoom}");
        assert_eq!(camera(&cam, 2.0), (Vec3::ZERO, 1.0));
    }
}
