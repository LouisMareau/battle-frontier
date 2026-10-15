//! Templates: what stands on an area of a map, kept to be put down again anywhere as it is
//! (`content::Template`). `capture` takes it from a map: positions from the area's
//! north-west corner, levels from its lowest ground, the scripts' cells along with them.
//! `stamp` puts a copy down with that corner on a cell: ids made unique on the map, levels
//! from the ground there, the scripts' cells and actor names moved along.

use std::collections::HashSet;

use content::{script, Actor, Building, Content, Event, Placed, Template};

use crate::WorldMap;

/// What a capture takes, by index in the map's lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picks {
    pub props: Vec<usize>,
    pub buildings: Vec<usize>,
    pub actors: Vec<usize>,
    pub events: Vec<usize>,
}

fn within(area: [i32; 4], x: i32, z: i32) -> bool {
    x >= area[0] && z >= area[1] && x < area[0] + area[2] && z < area[1] + area[3]
}

impl Picks {
    /// Everything wholly inside an area (x, z, width, height): props whose cells are all in
    /// it, buildings likewise, actors standing in it, events whose cells are in it.
    pub fn inside(map: &WorldMap, content: &Content, area: [i32; 4]) -> Picks {
        let all_in = |cells: &[(i32, i32)]| !cells.is_empty() && cells.iter().all(|&(x, z)| within(area, x, z));
        Picks {
            props: (0..map.props.len()).filter(|&i| all_in(&map.footprint(&map.props[i], content))).collect(),
            buildings: (0..map.buildings.len()).filter(|&i| all_in(&map.buildings[i].map_cells())).collect(),
            actors: (0..map.actors.len()).filter(|&i| within(area, map.actors[i].at[0], map.actors[i].at[1])).collect(),
            events: (0..map.events.len())
                .filter(|&i| {
                    let a = map.events[i].area;
                    a[2] > 0 && a[3] > 0 && within(area, a[0], a[1]) && within(area, a[0] + a[2] - 1, a[1] + a[3] - 1)
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.props.is_empty() && self.buildings.is_empty() && self.actors.is_empty() && self.events.is_empty()
    }
}

/// The lowest ground under an area (0 when none of it is on the map).
pub fn base_level(map: &WorldMap, area: [i32; 4]) -> u8 {
    let mut lowest: Option<u8> = None;
    for z in area[1]..area[1] + area[3] {
        for x in area[0]..area[0] + area[2] {
            if let Some(l) = map.level_at(x, z) {
                lowest = Some(lowest.map_or(l, |m| m.min(l)));
            }
        }
    }
    lowest.unwrap_or(0)
}

fn moved_area(a: [i32; 4], by: [i32; 2]) -> [i32; 4] {
    if a[2] > 0 && a[3] > 0 { [a[0] + by[0], a[1] + by[1], a[2], a[3]] } else { a }
}

fn moved_event(e: &Event, by: [i32; 2], renames: &[(String, String)], id: String) -> Event {
    Event { id, area: moved_area(e.area, by), script: script::shift(&e.script, by, renames), ..e.clone() }
}

fn moved_actor(a: &Actor, by: [i32; 2], level: u8, renames: &[(String, String)], id: String, event_id: &mut dyn FnMut(&str) -> String) -> Actor {
    Actor {
        id,
        at: [a.at[0] + by[0], a.at[1] + by[1]],
        level,
        area: moved_area(a.area, by),
        path: a.path.iter().map(|c| [c[0] + by[0], c[1] + by[1]]).collect(),
        events: a.events.iter().map(|e| moved_event(e, by, renames, event_id(&e.id))).collect(),
        ..a.clone()
    }
}

/// A template of what `picks` names on the map, from the area's corner.
pub fn capture(map: &WorldMap, id: &str, name: &str, area: [i32; 4], picks: &Picks) -> Template {
    let by = [-area[0], -area[1]];
    let base = base_level(map, area);
    let rel = |l: u8| l.saturating_sub(base);
    let none: Vec<(String, String)> = vec![];
    Template {
        id: id.to_string(),
        name: name.to_string(),
        size: [area[2].max(1) as u16, area[3].max(1) as u16],
        props: picks.props.iter().filter_map(|&i| map.props.get(i)).map(|p| Placed { at: [p.at[0] + by[0] as f64, p.at[1] + by[1] as f64], level: rel(p.level), ..p.clone() }).collect(),
        buildings: picks.buildings.iter().filter_map(|&i| map.buildings.get(i)).map(|b| Building { at: [b.at[0] + by[0], b.at[1] + by[1]], level: rel(b.level), ..b.clone() }).collect(),
        actors: picks.actors.iter().filter_map(|&i| map.actors.get(i)).map(|a| moved_actor(a, by, rel(a.level), &none, a.id.clone(), &mut |e: &str| e.to_string())).collect(),
        events: picks.events.iter().filter_map(|&i| map.events.get(i)).map(|e| moved_event(e, by, &none, e.id.clone())).collect(),
    }
}

/// What a stamp put down.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stamped {
    pub props: usize,
    /// the new buildings' indices on the map (their ground is to be settled)
    pub buildings: Vec<usize>,
    pub actors: usize,
    pub events: usize,
    /// actors and buildings that took another id: (in the template, on the map)
    pub renamed: Vec<(String, String)>,
}

/// `id`, or `id_2`, `id_3`... the first one not taken (which is then taken).
fn unique(id: &str, taken: &mut HashSet<String>) -> String {
    let mut out = id.to_string();
    let mut n = 2;
    while taken.contains(&out) {
        out = format!("{id}_{n}");
        n += 1;
    }
    taken.insert(out.clone());
    out
}

/// Puts a copy of a template down with its north-west corner on cell `at`. Refused when
/// part of it would go off the map, or a building of it onto another building's cells.
pub fn stamp(map: &mut WorldMap, t: &Template, at: [i32; 2], max_level: u8) -> Result<Stamped, String> {
    let (w, h) = (t.size[0].max(1) as i32, t.size[1].max(1) as i32);
    if at[0] < 0 || at[1] < 0 || at[0] + w > map.width as i32 || at[1] + h > map.height as i32 {
        return Err(format!("{} doesn't fit there: part of it would go off the map", t.name));
    }
    let built: HashSet<(i32, i32)> = map.buildings.iter().flat_map(|b| b.map_cells()).collect();
    for b in &t.buildings {
        if b.map_cells().iter().any(|&(x, z)| built.contains(&(x + at[0], z + at[1]))) {
            return Err(format!("{}: its {} would stand on another building", t.name, b.name));
        }
    }
    let base = base_level(map, [at[0], at[1], w, h]);
    let level = |l: u8| base.saturating_add(l).min(max_level);
    let mut out = Stamped::default();
    // the ids it brings, made unique on this map
    let mut actor_ids: HashSet<String> = map.actors.iter().map(|a| a.id.clone()).collect();
    let mut event_ids: HashSet<String> = map.events.iter().chain(map.actors.iter().flat_map(|a| a.events.iter())).map(|e| e.id.clone()).collect();
    let mut building_ids: HashSet<String> = map.buildings.iter().map(|b| b.id.clone()).collect();
    let renames: Vec<(String, String)> = t.actors.iter().map(|a| (a.id.clone(), unique(&a.id, &mut actor_ids))).collect();
    for p in &t.props {
        map.props.push(Placed { at: [p.at[0] + at[0] as f64, p.at[1] + at[1] as f64], level: level(p.level), ..p.clone() });
        out.props += 1;
    }
    for b in &t.buildings {
        let id = unique(&b.id, &mut building_ids);
        if id != b.id {
            out.renamed.push((b.id.clone(), id.clone()));
        }
        map.buildings.push(Building { id, at: [b.at[0] + at[0], b.at[1] + at[1]], level: level(b.level), ..b.clone() });
        out.buildings.push(map.buildings.len() - 1);
    }
    for (a, (old, new)) in t.actors.iter().zip(&renames) {
        if old != new {
            out.renamed.push((old.clone(), new.clone()));
        }
        let mut event_id = |e: &str| unique(e, &mut event_ids);
        map.actors.push(moved_actor(a, at, level(a.level), &renames, new.clone(), &mut event_id));
        out.actors += 1;
    }
    for e in &t.events {
        let id = unique(&e.id, &mut event_ids);
        map.events.push(moved_event(e, at, &renames, id));
        out.events += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::Floor;

    fn map() -> (WorldMap, Content) {
        let mut content = Content::default();
        content.props.put(content::Prop { id: "lamp".into(), category: "decoration".into(), footprint: [1, 1], ..Default::default() }, content::Space::Content);
        content.props.put(content::Prop { id: "bench".into(), category: "furniture".into(), footprint: [2, 1], ..Default::default() }, content::Space::Content);
        let mut m = WorldMap::new("m", "M", 30, 20, "grass");
        // a hill under the square: its lowest ground is level 1
        for z in 2..8 {
            for x in 2..10 {
                let i = m.index(x, z).unwrap();
                m.level[i] = if x < 4 { 2 } else { 1 };
            }
        }
        m.props.push(Placed { prop: "lamp".into(), at: [4.5, 3.5], level: 1, offset: [0.2, -0.1], ..Default::default() });
        m.props.push(Placed { prop: "bench".into(), at: [6.0, 5.5], level: 1, turn: 180.0, ..Default::default() });
        // half outside the area: left behind
        m.props.push(Placed { prop: "bench".into(), at: [10.0, 4.5], level: 0, ..Default::default() });
        m.buildings.push(Building { id: "hut".into(), name: "Hut".into(), at: [2, 2], level: 2, cells: vec!["##".into(), "##".into()], floors: vec![Floor::default()], ..Default::default() });
        m.actors.push(Actor { id: "bo".into(), name: "Bo".into(), look: "prop:lamp".into(), at: [7, 6], level: 1, moves: "wander".into(), area: [5, 5, 3, 2], path: vec![[8, 6]], ..Default::default() });
        m.actors[0].events.push(Event { id: "bo_talk".into(), script: vec!["say Bo: Hi.".into(), "move bo 8,7".into(), "camera 5,5".into()], ..Default::default() });
        m.events.push(Event { id: "step".into(), trigger: "enter".into(), area: [3, 6, 2, 1], script: vec!["face bo player".into()], ..Default::default() });
        // outside: not taken
        m.events.push(Event { id: "far".into(), trigger: "enter".into(), area: [20, 10, 1, 1], ..Default::default() });
        (m, content)
    }

    #[test]
    fn a_capture_takes_what_is_wholly_inside() {
        let (m, content) = map();
        let picks = Picks::inside(&m, &content, [2, 2, 8, 6]);
        assert_eq!(picks, Picks { props: vec![0, 1], buildings: vec![0], actors: vec![0], events: vec![0] });
        let t = capture(&m, "square", "Square", [2, 2, 8, 6], &picks);
        assert_eq!(t.size, [8, 6]);
        assert_eq!((t.props[0].at, t.props[0].level, t.props[0].offset), ([2.5, 1.5], 0, [0.2, -0.1]));
        assert_eq!((t.buildings[0].at, t.buildings[0].level), ([0, 0], 1));
        let bo = &t.actors[0];
        assert_eq!((bo.at, bo.area, bo.path.clone()), ([5, 4], [3, 3, 3, 2], vec![[6, 4]]));
        assert_eq!(bo.events[0].script, vec!["say Bo: Hi.", "move bo 6,5", "camera 3,3"]);
        assert_eq!(t.events[0].area, [1, 4, 2, 1]);
        assert_eq!(t.summary(), "2 props, 1 building, 1 actor, 1 event");
    }

    #[test]
    fn stamps_keep_everything_and_take_new_ids() {
        let (mut m, content) = map();
        let t = capture(&m, "square", "Square", [2, 2, 8, 6], &Picks::inside(&m, &content, [2, 2, 8, 6]));
        // on flat ground at level 0, twenty cells east
        let s = stamp(&mut m, &t, [20, 12], 9).expect("room there");
        assert_eq!((s.props, s.buildings.len(), s.actors, s.events), (2, 1, 1, 1));
        let lamp = &m.props[3];
        assert_eq!((lamp.at, lamp.level, lamp.offset), ([22.5, 13.5], 0, [0.2, -0.1]));
        let bench = &m.props[4];
        assert_eq!((bench.turn, bench.at), (180.0, [24.0, 15.5]));
        let hut = m.buildings.last().unwrap();
        assert_eq!((hut.id.as_str(), hut.at, hut.level), ("hut_2", [20, 12], 1));
        let bo = m.actors.last().unwrap();
        assert_eq!((bo.id.as_str(), bo.at, bo.area), ("bo_2", [25, 16], [23, 15, 3, 2]));
        assert_eq!(bo.events[0].id, "bo_talk_2");
        assert_eq!(bo.events[0].script, vec!["say Bo: Hi.", "move bo_2 26,17", "camera 23,15"]);
        let step = m.events.last().unwrap();
        assert_eq!((step.id.as_str(), step.area, step.script.clone()), ("step_2", [21, 16, 2, 1], vec!["face bo_2 player".to_string()]));
        assert!(s.renamed.contains(&("bo".into(), "bo_2".into())) && s.renamed.contains(&("hut".into(), "hut_2".into())));
        // a second copy: ids go on counting
        stamp(&mut m, &t, [20, 3], 9).unwrap();
        assert_eq!(m.actors.last().unwrap().id, "bo_3");
        // off the map, or on a building: refused, the map unchanged
        let before = (m.props.len(), m.buildings.len());
        assert!(stamp(&mut m, &t, [25, 15], 9).unwrap_err().contains("off the map"));
        assert!(stamp(&mut m, &t, [21, 13], 9).unwrap_err().contains("another building"));
        assert_eq!((m.props.len(), m.buildings.len()), before);
    }
}
