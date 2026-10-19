//! The sounds of a map, where they come from: the props' sounds (`Sound` on a prop), placed
//! like their lights, and the ambience (`Ambience` on the map): everywhere, or within an
//! area and fading past its edge.

use content::{Ambience, Content, Prop, Sound};
use glam::{Mat4, Vec3};

use crate::buildings::anchor_placement;
use crate::mesh::placement;
use crate::{QuickMap, WorldMap};

/// A sound where its prop stands.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedSound {
    pub pos: Vec3,
    pub sound: Sound,
    /// which prop placement it is on the map (a key that lasts between frames)
    pub key: String,
    /// the prop's id (its space says where the file is)
    pub prop: String,
}

fn put(out: &mut Vec<PlacedSound>, key: String, prop: &str, sound: &Sound, m: Mat4) {
    let at = Vec3::new(sound.at[0] as f32, sound.at[1] as f32, sound.at[2] as f32);
    out.push(PlacedSound { pos: m.transform_point3(at), sound: sound.clone(), key, prop: prop.to_string() });
}

/// Every prop's sound on the map, where its prop stands.
pub fn sounds(map: &WorldMap, content: &Content, level_height: f32) -> Vec<PlacedSound> {
    let mut out = vec![];
    // the props giving off a sound, looked up quickly for each of a big map's props
    let heard: QuickMap<&str, (&Sound, &Prop)> = content.props.iter().filter_map(|p| p.sound.as_ref().map(|s| (p.id.as_str(), (s, p)))).collect();
    let sound = |id: &str| heard.get(id).map(|(s, p)| (*s, p.scalable()));
    // (along a ramp, from the middle of what's laid on it, once)
    for (i, p) in map.props.iter().enumerate() {
        let Some(&(s, prop)) = heard.get(p.prop.as_str()) else { continue };
        let puts = map.puts(p, prop, level_height);
        let Some(at) = puts.get(puts.len() / 2) else { continue };
        let pos = at.point(Vec3::new(s.at[0] as f32, s.at[1] as f32, s.at[2] as f32));
        out.push(PlacedSound { pos, sound: s.clone(), key: format!("prop:{i}"), prop: p.prop.clone() });
    }
    for (bi, b) in map.buildings.iter().enumerate() {
        let base = b.level as f32 * level_height;
        let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
        for (f, floor) in b.floors.iter().enumerate() {
            let y0 = base + f as f32 * crate::floor_height(level_height);
            for (i, p) in floor.props.iter().enumerate() {
                let Some((s, scalable)) = sound(&p.prop) else { continue };
                let scale = if scalable { p.scale as f32 } else { 1.0 };
                put(&mut out, format!("building:{bi}:floor:{f}:{i}"), &p.prop, s, placement(Vec3::new(ox + p.pos()[0] as f32, y0 + 0.05, oz + p.pos()[1] as f32), p.turn as f32, scale));
            }
        }
        for (i, a) in b.anchors.iter().enumerate() {
            if let Some((s, _)) = sound(&a.prop) {
                put(&mut out, format!("building:{bi}:anchor:{i}"), &a.prop, s, anchor_placement(b, a, level_height));
            }
        }
        for (i, p) in b.outside.iter().enumerate() {
            let Some((s, scalable)) = sound(&p.prop) else { continue };
            let scale = if scalable { p.scale as f32 } else { 1.0 };
            put(&mut out, format!("building:{bi}:outside:{i}"), &p.prop, s, placement(Vec3::new(ox + p.pos()[0] as f32, base, oz + p.pos()[1] as f32), p.turn as f32, scale));
        }
    }
    out
}

/// How loud an ambience is at a point on the ground (x, z in cells): its volume inside
/// its area (or anywhere, with no area), fading to nothing `fade` cells past the edge.
pub fn ambience_level(a: &Ambience, x: f32, z: f32) -> f32 {
    if a.area == [0, 0, 0, 0] {
        return a.volume as f32;
    }
    let (x0, z0) = (a.area[0] as f32, a.area[1] as f32);
    let (x1, z1) = (x0 + a.area[2] as f32, z0 + a.area[3] as f32);
    let dx = (x0 - x).max(0.0).max(x - x1);
    let dz = (z0 - z).max(0.0).max(z - z1);
    let d = (dx * dx + dz * dz).sqrt();
    if d <= 0.0 {
        return a.volume as f32;
    }
    if a.fade <= 0.0 {
        return 0.0;
    }
    (a.volume as f32 * (1.0 - d / a.fade as f32)).max(0.0)
}

/// Whether an ambience plays at a time of day ("morning", "day" or "night").
pub fn ambience_on(a: &Ambience, period: &str) -> bool {
    a.when == "always" || a.when == period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambience_fades_past_its_area() {
        let stream = Ambience { file: "sounds/stream.wav".into(), volume: 0.8, when: "always".into(), area: [10, 10, 4, 2], fade: 4.0 };
        assert_eq!(ambience_level(&stream, 12.0, 11.0), 0.8);
        assert!((ambience_level(&stream, 16.0, 11.0) - 0.4).abs() < 1e-6, "two cells out of four");
        assert_eq!(ambience_level(&stream, 20.0, 11.0), 0.0);
        let wind = Ambience { file: "sounds/wind.wav".into(), volume: 0.3, when: "night".into(), ..Default::default() };
        assert_eq!(ambience_level(&wind, 99.0, -5.0), 0.3);
        assert!(ambience_on(&wind, "night") && !ambience_on(&wind, "day") && ambience_on(&stream, "day"));
    }
}
