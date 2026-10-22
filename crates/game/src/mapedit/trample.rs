//! Who tramples the grass, and the grass springing back. The walker, the actors and the
//! creatures roaming push the meadows' blades and what's painted for the wind aside where
//! they stand (`Frame.pushers`: each where it is and how wide it pushes); behind each, where
//! it has just been, the grass is still bent and springs back over a time that goes by its
//! weight (`recovery`: a small creature's trail is gone in a moment, a person's in about a
//! second, the heaviest creatures' in nearly two). Each keeps a trail of where it was
//! (`Trails`), and the frame gets ghosts of it along the trail at even ages, each pushing as
//! much of a bend as is left (`left`).

use std::collections::{HashMap, VecDeque};

use glam::Vec3;
use render::scene::{Pusher, MAX_PUSHERS};

use super::MapEditor;

/// How many ghosts follow each along its trail.
pub const GHOSTS: usize = 4;
/// How long a trail is kept (seconds): past the longest recovery.
const KEPT_S: f64 = 2.2;
/// A person's weight (kg): the walker, and the actors that are people or props.
const PERSON_KG: f64 = 65.0;
/// A creature of no known weight (kg).
const CREATURE_KG: f64 = 20.0;

/// Who leaves a trail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Who {
    Walker,
    Actor(usize),
    Roamer(u32),
}

/// Where each of them was over the last moments: (when, where), the oldest first.
#[derive(Default)]
pub(super) struct Trails {
    paths: HashMap<Who, VecDeque<(f64, Vec3)>>,
    /// who was seen this frame
    seen: Vec<Who>,
}

impl Trails {
    /// Where `who` is now; its trail's oldest moments forgotten.
    fn record(&mut self, who: Who, now: f64, at: Vec3) {
        let path = self.paths.entry(who).or_default();
        path.push_back((now, at));
        while path.front().is_some_and(|(t, _)| now - t > KEPT_S) {
            path.pop_front();
        }
        self.seen.push(who);
    }

    /// Whoever wasn't seen this frame forgotten (gone, hidden, off the map).
    fn settle(&mut self) {
        let seen = std::mem::take(&mut self.seen);
        self.paths.retain(|who, _| seen.contains(who));
    }

    /// Where `who` was `age` seconds ago (between the moments kept); none before its trail
    /// began.
    fn back(&self, who: Who, now: f64, age: f64) -> Option<Vec3> {
        let path = self.paths.get(&who)?;
        let when = now - age;
        if when < path.front()?.0 {
            return None;
        }
        // the first moment after `when`; the one before it is at or before `when`
        let i = path.partition_point(|(t, _)| *t <= when);
        let (t0, p0) = path.get(i.checked_sub(1)?).copied()?;
        match path.get(i) {
            Some(&(t1, p1)) if t1 > t0 => Some(p0.lerp(p1, ((when - t0) / (t1 - t0)) as f32)),
            _ => Some(p0),
        }
    }
}

/// How long the grass takes to spring back from under someone of `kg` (seconds): under a
/// small creature well under half a second, under a person about one, under the heaviest
/// creatures nearly two.
pub fn recovery(kg: f64) -> f64 {
    0.3 + 1.5 * (kg / 150.0).clamp(0.0, 1.0)
}

/// How much of a bend is left `age` seconds after whoever made it has gone, springing back
/// over `recovery` seconds (quick at first, then easing).
pub fn left(age: f64, recovery: f64) -> f32 {
    (1.0 - age / recovery.max(0.01)).clamp(0.0, 1.0).powf(1.5) as f32
}

impl MapEditor {
    /// What an actor weighs (kg): a creature's from its record; a person's for a picture or
    /// a prop.
    fn weight_of(&self, look: &str) -> f64 {
        match look.split_once(':') {
            Some(("creature", id)) => self.content.creatures.get(id).map(|c| c.body.weight_kg).filter(|kg| *kg > 0.0).unwrap_or(CREATURE_KG),
            _ => PERSON_KG,
        }
    }

    /// Who tramples the grass this frame (`Frame.pushers`): the walker, the actors where they
    /// stand or step, the creatures roaming as much as they're there, each followed by the
    /// ghosts of where it has just been (`GHOSTS` of them along its trail at even ages over
    /// its recovery, each pushing what's left of its bend); the nearest the camera's target
    /// first, at most `MAX_PUSHERS` in all; none while the checks say `trample:off`.
    pub(super) fn pushers(&mut self, now: f64) -> Vec<Pusher> {
        if !self.trample {
            self.trails.settle();
            return vec![];
        }
        // who's where, how wide they push, how heavy they are
        let mut who: Vec<(Who, Vec3, f32, f64)> = vec![];
        if let Some(w) = &self.walker {
            who.push((Who::Walker, w.pos(now), 0.9, PERSON_KG));
        }
        for i in 0..self.live.len() {
            if !self.live[i].hidden {
                let kg = self.map.actors.get(i).map_or(PERSON_KG, |a| self.weight_of(&a.look));
                who.push((Who::Actor(i), self.actor_pos(i, now), 0.8, kg));
            }
        }
        for r in &self.spawner.roamers {
            if r.presence > 0.05 {
                let kg = self.content.creatures.get(&r.creature).map(|c| c.body.weight_kg).filter(|kg| *kg > 0.0).unwrap_or(CREATURE_KG);
                who.push((Who::Roamer(r.id), Vec3::new(r.pos.x, self.ground_at(r.pos.x, r.pos.y), r.pos.y), 0.7 * r.presence, kg));
            }
        }
        for &(w, at, _, _) in &who {
            self.trails.record(w, now, at);
        }
        self.trails.settle();
        let target = self.cam.target;
        who.sort_by(|a, b| a.1.distance_squared(target).total_cmp(&b.1.distance_squared(target)));
        let mut out: Vec<Pusher> = vec![];
        for (w, at, reach, kg) in who {
            if out.len() >= MAX_PUSHERS {
                break;
            }
            out.push(Pusher { at, reach, strength: 1.0 });
            let recovery = recovery(kg);
            for k in 1..=GHOSTS {
                let age = recovery * k as f64 / (GHOSTS + 1) as f64;
                let strength = left(age, recovery);
                if let Some(back) = self.trails.back(w, now, age).filter(|_| strength > 0.02 && out.len() < MAX_PUSHERS) {
                    out.push(Pusher { at: back, reach, strength });
                }
            }
        }
        out
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// The grass springs back faster from under the light, slower from under the heavy; a
    /// trail gives where someone was a moment ago, between the moments kept, nothing from
    /// before it began, and is forgotten once they're gone. The walker standing a while:
    /// itself and its ghosts, all where it stands, each bending the grass less the older;
    /// nothing with the trampling off.
    #[test]
    fn the_grass_springs_back_behind_whoever_walked_through_it() {
        assert!(recovery(5.0) < 0.4 && (0.9..1.05).contains(&recovery(65.0)) && recovery(300.0) == 1.8, "{} {} {}", recovery(5.0), recovery(65.0), recovery(300.0));
        assert!((left(0.0, 1.0) - 1.0).abs() < 1e-6 && left(0.5, 1.0) < 0.4 && left(1.0, 1.0) == 0.0 && left(2.0, 1.0) == 0.0);
        let mut t = Trails::default();
        for i in 0..=10 {
            t.record(Who::Walker, i as f64 * 0.1, Vec3::new(i as f32 * 0.5, 0.0, 0.0));
        }
        assert_eq!(t.back(Who::Walker, 1.0, 0.0), Some(Vec3::new(5.0, 0.0, 0.0)));
        let between = t.back(Who::Walker, 1.0, 0.25).unwrap();
        assert!((between.x - 3.75).abs() < 1e-4, "between two moments: {between}");
        assert_eq!(t.back(Who::Walker, 1.0, 1.5), None, "before its trail began");
        t.settle();
        assert!(t.paths.contains_key(&Who::Walker), "seen this frame: kept");
        t.settle();
        assert!(t.paths.is_empty(), "not seen since: forgotten");
        let mut ed = MapEditor::new(Arc::new(content::Content::default()), None, crate::platform::Files::new("."));
        assert!(ed.pushers(0.0).is_empty(), "nobody about");
        ed.walk_to(3, 3);
        let mut p = vec![];
        for i in 0..=12 {
            p = ed.pushers(1000.0 + i as f64 * 0.1);
        }
        assert_eq!(p.len(), 1 + GHOSTS, "standing: itself and its ghosts: {p:?}");
        assert!(p.iter().all(|q| (3.0..4.0).contains(&q.at.x) && (3.0..4.0).contains(&q.at.z) && q.reach > 0.3 && q.reach < 1.0), "{p:?}");
        assert!(p[0].strength == 1.0 && p[1].strength < 1.0 && p[GHOSTS].strength < p[1].strength && p[GHOSTS].strength > 0.0, "{p:?}");
        ed.trample = false;
        assert!(ed.pushers(1002.0).is_empty(), "turned off for the checks");
    }
}
