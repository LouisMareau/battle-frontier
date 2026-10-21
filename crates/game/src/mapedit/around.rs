//! The maps around the open one: the other pieces of its world, held while they're within
//! the camera's reach, each streamed square by square like the open map (`stream.rs`) and
//! placed where it lies on the world's grid, so the world reads as one place (dimmed a
//! little in the editor, where they aren't the map being edited: a double-click opens one).
//! Walking off the open map's edge, or opening a map that's around, moves into it without
//! loading anything: it becomes the open map, the one before a map around it, and the
//! camera (and the walker) move into its space, so nothing on the screen moves.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use content::{Chunk, Content, Space, CHUNK};
use glam::Vec3;
use render::scene::{Place, SceneRenderer};
use world::lights::PlacedLight;
use world::mesh::{self, Beyond};
use world::WorldMap;

use super::stream::{self, Feed, GpuState, Needs};
use super::MapEditor;
use crate::platform::Load;

/// How much darker the maps around the open one are in the editor.
const DIM: f32 = 0.45;

pub(super) struct Neighbour {
    pub map: WorldMap,
    pub space: Space,
    /// its north-west cell on the world's grid
    pub origin: [i32; 2],
    pub gpu: GpuState,
    need: Needs,
    /// its lamps (in its own space) and which of its cells block, worked out when it comes
    /// and when the content changes, not every frame or step: a big map has thousands
    lights: Vec<PlacedLight>,
    blocked: Vec<bool>,
}

impl Neighbour {
    fn new(map: WorldMap, space: Space, origin: [i32; 2], content: &Content, level_height: f32) -> Neighbour {
        let (lights, blocked) = (world::lights::lights(&map, content, level_height), map.blocked(content));
        Neighbour { map, space, origin, gpu: GpuState::default(), need: Needs::all(), lights, blocked }
    }
}

/// A map on its way: its chunk files.
struct Coming {
    record: content::Map,
    space: Space,
    origin: [i32; 2],
    paths: Vec<String>,
}

#[derive(Default)]
pub(super) struct Around {
    /// the maps held, by id
    pub maps: BTreeMap<String, Neighbour>,
    coming: BTreeMap<String, Coming>,
    /// the open map's north-west cell on its world's grid (none: it's in no world)
    pub origin: Option<[i32; 2]>,
    /// goes up whenever a map comes, goes or moves, or the open map's edge changes: the
    /// edges between them are built again
    pub version: u64,
    /// the open map's edge, as last seen (a digest)
    edge: u64,
    /// the content the maps were last built from (another one: their props again)
    content: usize,
    /// the models' generations when they were last built (a sum): a model come since
    /// builds their props again
    models: u64,
}

/// The maps next to one being built, in its own coordinates.
pub(super) struct Next<'a> {
    /// a map, its corner from the one being built's, its terrains' palette columns
    maps: Vec<(&'a WorldMap, [i32; 2], Vec<u8>)>,
}

impl Beyond for Next<'_> {
    fn level(&self, x: i32, z: i32) -> Option<u8> {
        self.maps.iter().find_map(|(m, o, _)| m.level_at(x - o[0], z - o[1]))
    }

    fn terrain(&self, x: i32, z: i32) -> Option<u8> {
        self.maps.iter().find_map(|(m, o, columns)| m.index(x - o[0], z - o[1]).map(|i| columns.get(m.terrain[i] as usize).copied().unwrap_or(255)))
    }
}

impl Around {
    /// The maps next to one whose corner is at `at` on the world's grid: the ones held, and
    /// the open map (with its corner) when given.
    pub fn next<'a>(&'a self, content: &Content, at: [i32; 2], open: Option<(&'a WorldMap, [i32; 2])>) -> Next<'a> {
        let maps = self.maps.values().map(|n| (&n.map, n.origin)).chain(open);
        Next { maps: maps.map(|(m, o)| (m, [o[0] - at[0], o[1] - at[1]], m.terrains.iter().map(|t| mesh::terrain_index(content, t)).collect())).collect() }
    }

    /// Where a held map's space is in the open map's.
    fn offset(&self, n: &Neighbour) -> Vec3 {
        let o = self.origin.unwrap_or(n.origin);
        Vec3::new((n.origin[0] - o[0]) as f32, 0.0, (n.origin[1] - o[1]) as f32)
    }
}

/// How far a point is from a rectangle (x0, z0, x1, z1), on the ground.
fn off_rect(x0: f32, z0: f32, x1: f32, z1: f32, p: Vec3) -> f32 {
    let dx = (x0 - p.x).max(p.x - x1).max(0.0);
    let dz = (z0 - p.z).max(p.z - z1).max(0.0);
    (dx * dx + dz * dz).sqrt()
}

impl MapEditor {
    /// Which maps around the open one to hold: the pieces of its world within the camera's
    /// reach come (their chunk files fetched), the ones well out of it go.
    pub(super) fn hold_around(&mut self) {
        // another content (a prop's record changed): their props, trees and buildings again
        let c = std::sync::Arc::as_ptr(&self.content) as usize;
        let epoch: u64 = self.model_versions.values().sum();
        if c != self.around.content || epoch != self.around.models {
            // (their lamps and what blocks come from the records, not the models)
            let records = c != self.around.content;
            self.around.content = c;
            self.around.models = epoch;
            let lh = self.level_height;
            for n in self.around.maps.values_mut() {
                (n.need.props, n.need.trees, n.need.buildings) = (true, true, true);
                if records {
                    (n.lights, n.blocked) = (world::lights::lights(&n.map, &self.content, lh), n.map.blocked(&self.content));
                }
            }
        }
        let world = self.world_record();
        let here = world.as_ref().and_then(|w| w.piece_of(&self.map.id)).map(|p| p.at);
        if here != self.around.origin {
            self.around.origin = here;
            self.around.version += 1;
        }
        let (Some(world), Some(origin)) = (world, here) else {
            if !self.around.maps.is_empty() || !self.around.coming.is_empty() {
                self.around.maps.clear();
                self.around.coming.clear();
                self.around.version += 1;
            }
            crate::platform::set_hook("around", "");
            return;
        };
        let t = self.cam.target;
        let reach = self.cam.distance * 1.1 + 48.0 + CHUNK as f32;
        let content = self.content.clone();
        let mut wanted: BTreeMap<String, [i32; 2]> = BTreeMap::new();
        let mut keep: BTreeSet<String> = BTreeSet::new();
        for p in world.pieces.iter().filter(|p| p.map != self.map.id) {
            let Some(m) = content.maps.get(&p.map) else { continue };
            let (x0, z0) = ((p.at[0] - origin[0]) as f32, (p.at[1] - origin[1]) as f32);
            let d = off_rect(x0, z0, x0 + m.width as f32, z0 + m.height as f32, t);
            if d <= reach {
                wanted.insert(p.map.clone(), p.at);
            }
            if d <= reach * 1.5 {
                keep.insert(p.map.clone());
            }
        }
        // well out of reach, out of the world, or the open map now: gone
        let before = self.around.maps.len();
        let open = self.map.id.clone();
        self.around.maps.retain(|id, _| keep.contains(id) && *id != open);
        self.around.coming.retain(|id, _| wanted.contains_key(id));
        let mut changed = self.around.maps.len() != before;
        // moved in the World view: where they are now
        for (id, n) in self.around.maps.iter_mut() {
            if let Some(p) = world.piece_of(id).filter(|p| p.at != n.origin) {
                n.origin = p.at;
                changed = true;
            }
        }
        // wanted and not here: on their way
        for (id, at) in &wanted {
            if self.around.maps.contains_key(id) || self.around.coming.contains_key(id) {
                continue;
            }
            let Some(e) = content.maps.entry(id) else { continue };
            let mut paths = vec![];
            if e.value.chunked() {
                let (gx, gz) = e.value.chunk_grid();
                paths = (0..gz).flat_map(|z| (0..gx).map(move |x| (x, z))).map(|(x, z)| format!("{}/{}", e.space.dir(), Chunk::path(&e.value.id, x, z))).collect();
                for p in &paths {
                    self.files.request(p);
                }
            }
            self.around.coming.insert(id.clone(), Coming { record: e.value.clone(), space: e.space, origin: *at, paths });
        }
        // the ones whose files are all here
        let ready: Vec<String> = self.around.coming.iter().filter(|(_, c)| c.paths.iter().all(|p| !matches!(self.files.get(p), Load::Pending))).map(|(id, _)| id.clone()).collect();
        for id in ready {
            let c = self.around.coming.remove(&id).unwrap();
            let mut map = if c.record.chunked() {
                let chunks: Vec<Chunk> = c.paths.iter().filter_map(|p| if let Load::Ready(b) = self.files.get(p) { serde_json::from_slice(&b).ok() } else { None }).collect();
                WorldMap::from_chunks(&c.record, &chunks)
            } else {
                WorldMap::from_record(&c.record)
            };
            map.know_doors(&content);
            self.around.maps.insert(id, Neighbour::new(map, c.space, c.origin, &content, self.level_height));
            changed = true;
        }
        if changed {
            self.around.version += 1;
        }
        crate::platform::set_hook("around", &self.around.maps.keys().cloned().collect::<Vec<_>>().join(","));
    }

    /// The open map's edge (its outer cells' levels and terrains): when it changes, the maps
    /// next to it build their seams again.
    pub(super) fn note_edge(&mut self) {
        let m = &self.map;
        let (w, h) = (m.width as i32, m.height as i32);
        let mut d = DefaultHasher::new();
        (w, h).hash(&mut d);
        let rim = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|z| [(0, z), (w - 1, z)]));
        for (x, z) in rim {
            if let Some(i) = m.index(x, z) {
                (m.level[i], m.terrains.get(m.terrain[i] as usize)).hash(&mut d);
            }
        }
        let d = d.finish();
        if d != self.around.edge {
            self.around.edge = d;
            self.around.version += 1;
        }
    }

    /// The maps around, square by square like the open one, nearest first, with what's
    /// left of the frame's budget; each placed where it lies.
    pub(super) fn stream_around(&mut self, r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState, start: f64) {
        let Some(origin) = self.around.origin else { return };
        let t = self.cam.target;
        let mut ids: Vec<(f32, String)> = self
            .around
            .maps
            .iter()
            .map(|(id, n)| {
                let o = self.around.offset(n);
                (off_rect(o.x, o.z, o.x + n.map.width as f32, o.z + n.map.height as f32, t), id.clone())
            })
            .collect();
        ids.sort_by(|a, b| a.0.total_cmp(&b.0));
        let dim = if self.play { 0.0 } else { DIM };
        let mut held = (0, 0);
        for (_, id) in ids {
            let Some(mut n) = self.around.maps.remove(&id) else { continue };
            let offset = Vec3::new((n.origin[0] - origin[0]) as f32, 0.0, (n.origin[1] - origin[1]) as f32);
            {
                let next = self.around.next(&self.content, n.origin, Some((&self.map, origin)));
                let need = std::mem::take(&mut n.need);
                let mut feed = Feed {
                    id: &id,
                    map: &n.map,
                    gpu: &mut n.gpu,
                    content: &self.content,
                    models: &self.models,
                    model_versions: &self.model_versions,
                    grass: &self.grass_levels,
                    level_height: self.level_height,
                    beyond: &next,
                    target: t - offset,
                    distance: self.cam.distance,
                    cuts: &[],
                    editing: None,
                    draft: None,
                    actors: true,
                    focus: None,
                    around_version: self.around.version,
                };
                stream::stream(r, rs, &mut feed, need, start);
            }
            r.set_world_place(&id, Place { offset, grid: [n.origin[0] as f32, n.origin[1] as f32], dim });
            let (a, b) = n.gpu.held(&n.map);
            held = (held.0 + a, held.1 + b);
            self.around.maps.insert(id, n);
        }
        crate::platform::set_hook("aroundsquares", &format!("{}/{}", held.0, held.1));
    }

    /// The held map under a point of the open map's space, if one is there.
    pub(super) fn around_at(&self, p: Vec3) -> Option<&str> {
        self.around.maps.iter().find(|(_, n)| {
            let o = self.around.offset(n);
            n.map.index((p.x - o.x).floor() as i32, (p.z - o.z).floor() as i32).is_some()
        }).map(|(id, _)| id.as_str())
    }

    /// The lamps of the maps around, where they are in the open map's space.
    pub(super) fn around_lights(&self) -> Vec<PlacedLight> {
        let mut out = vec![];
        for n in self.around.maps.values() {
            let o = self.around.offset(n);
            out.extend(n.lights.iter().map(|l| PlacedLight { pos: l.pos + o, ..l.clone() }));
        }
        out
    }

    /// Whether the walker can step onto a cell of a map around this one: on the same level,
    /// nothing blocking there. None: that map isn't here (yet).
    pub(super) fn can_cross(&self, id: &str, cell: (i32, i32), level: u8) -> Option<bool> {
        let n = self.around.maps.get(id)?;
        let i = n.map.index(cell.0, cell.1)?;
        Some(n.map.level[i] == level && !n.blocked.get(i).copied().unwrap_or(false))
    }

    /// Makes a map around this one the open one, keeping all that's on the GPU: the one
    /// before becomes a map around it, and the camera (and the walker) move into the new
    /// one's space, so nothing on the screen moves. Returns how far that space is from the
    /// old one's.
    pub(super) fn step_into(&mut self, id: &str) -> Option<Vec3> {
        let origin = self.around.origin?;
        let n = self.around.maps.remove(id)?;
        let delta = Vec3::new((n.origin[0] - origin[0]) as f32, 0.0, (n.origin[1] - origin[1]) as f32);
        let (map, space) = (std::mem::replace(&mut self.map, n.map), std::mem::replace(&mut self.space, n.space));
        let mut old = Neighbour::new(map, space, origin, &self.content, self.level_height);
        old.gpu = std::mem::replace(&mut self.gpu, n.gpu);
        self.around.maps.insert(old.map.id.clone(), old);
        self.around.origin = Some(n.origin);
        self.around.version += 1;
        self.cam.target -= delta;
        if let Some(w) = &mut self.walker {
            w.shift(delta);
        }
        self.after_switch();
        Some(delta)
    }
}
