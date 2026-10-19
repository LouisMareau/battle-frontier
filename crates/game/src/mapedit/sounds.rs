//! What the map sounds like from where the player stands (or the camera looks): the
//! props' sounds where they are, the creatures' as they roam, the actors', and the
//! ambience of the place by time of day; and the one-shots scripts ask for.

use glam::Vec3;

use super::zones::period_key;
use super::MapEditor;
use crate::audio::Want;

impl MapEditor {
    /// Where sounds are heard from: the walker, else the middle of the view.
    pub(super) fn listener(&self, now: f64) -> Vec3 {
        match &self.walker {
            Some(w) => w.pos(now),
            None => self.cam.target,
        }
    }

    /// Everything that should be playing now.
    pub(super) fn sound_wants(&self, now: f64) -> Vec<Want> {
        if !self.sounds_on {
            return vec![];
        }
        let listener = self.listener(now);
        let mut out = vec![];
        let lh = self.level_height;
        for s in world::sounds::sounds(&self.map, &self.content, lh) {
            let Some(e) = self.content.props.entry(&s.prop) else { continue };
            out.push(Want { key: s.key, file: e.space.asset(&s.sound.file), pos: Some(s.pos), volume: s.sound.volume as f32, range: s.sound.range as f32 });
        }
        if self.creatures_on {
            for r in &self.spawner.roamers {
                let Some(e) = self.content.creatures.entry(&r.creature) else { continue };
                let Some(s) = &e.value.sound else { continue };
                let feet = Vec3::new(r.pos.x, self.ground_at(r.pos.x, r.pos.y), r.pos.y);
                out.push(Want { key: format!("roamer:{}", r.id), file: e.space.asset(&s.file), pos: Some(feet), volume: s.volume as f32 * r.presence.clamp(0.0, 1.0), range: s.range as f32 });
            }
        }
        for (i, l) in self.live.iter().enumerate() {
            if l.hidden {
                continue;
            }
            let Some(id) = self.map.actors.get(i).and_then(|a| a.look.strip_prefix("creature:")) else { continue };
            let Some(e) = self.content.creatures.entry(id) else { continue };
            let Some(s) = &e.value.sound else { continue };
            out.push(Want { key: format!("actor:{}", l.id), file: e.space.asset(&s.file), pos: Some(self.actor_pos(i, now)), volume: s.volume as f32, range: s.range as f32 });
        }
        let space = self.content.maps.entry(&self.map.id).map(|e| e.space).unwrap_or(self.space);
        let period = period_key(self.period());
        for (i, a) in self.map.ambience.iter().enumerate() {
            if !world::sounds::ambience_on(a, period) {
                continue;
            }
            let level = world::sounds::ambience_level(a, listener.x, listener.z);
            if level > 0.0 {
                out.push(Want { key: format!("ambience:{i}"), file: space.asset(&a.file), pos: None, volume: level, range: 0.0 });
            }
        }
        // the nearest first: the mixer plays what's in reach
        let mut keyed: Vec<(f32, Want)> = out.into_iter().map(|w| (crate::audio::level(listener, w.pos, w.volume, w.range), w)).filter(|(l, _)| *l > 0.0).collect();
        keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
        keyed.into_iter().take(16).map(|(_, w)| w).collect()
    }
}
