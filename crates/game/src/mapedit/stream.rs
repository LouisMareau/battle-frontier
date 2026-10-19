//! The world on the GPU, map by map and square by square (CHUNK cells a side): the open map
//! and the maps around it in its world (`around.rs`), each in its own space, placed where it
//! lies on the world's grid. The squares within reach of the camera are built in full and
//! sent, nearest first and a few per frame; the rest get a cheap stand-in (the ground in
//! blocks, a canopy over the woods, a box per building) so a map of any size shows whole
//! from afar, and a square swaps between the two as the camera moves. An edit rebuilds the
//! squares it touched and no more, since every layer keeps a digest of what each square was
//! built from; a prop's model arriving (or read again) counts as a change to every square
//! that uses the prop, so a square built before the file came is built again with it. A
//! map's edges look past it into the maps next to it: no cliff at a seam on one level,
//! terrain borders running across.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use content::{Building, Content, CHUNK};
use glam::{Mat4, Vec3};
use render::scene::{Place, SceneRenderer, WorldGround, CLIFFS, WHITE};
use world::buildings::{building, Cut, Sides};
use world::mesh::{self, placement, prop_scene, Beyond, Models, Scene};
use world::trees::{trees, Tree};
use world::WorldMap;

use super::{build, gpu, models, MapEditor};

/// The layers built square by square, in full.
const LAYERS: [&str; 4] = ["ground", "props", "trees", "buildings"];
/// The stand-in of a square out of reach: `mesh::far_region`.
const FAR: &str = "far";
/// How much of a frame building squares may take, in milliseconds.
const BUDGET_MS: f64 = 6.0;

/// What a map has on the GPU.
#[derive(Default)]
pub(super) struct GpuState {
    /// what the ground's texture and palette were sent for: the map, its width, height and
    /// terrains
    pub(super) ground: Option<(String, usize, usize, Vec<String>)>,
    /// the squares each layer holds on the GPU
    built: BTreeMap<&'static str, BTreeSet<(i32, i32)>>,
    /// per layer and square, a digest of what it's made from
    sources: BTreeMap<&'static str, BTreeMap<(i32, i32), u64>>,
    /// the trees, as last laid out
    trees: Vec<Tree>,
    /// the props layer's vertices, by square (a hook for the checks: it grows when a model
    /// comes late)
    props_vertices: BTreeMap<(i32, i32), usize>,
    /// the maps around it when its edges were last built (`Around::version`)
    around: u64,
}

impl GpuState {
    /// How many of its squares are held in full, and how many it has.
    pub fn held(&self, map: &WorldMap) -> (usize, usize) {
        (self.built.get("ground").map(|b| b.len()).unwrap_or(0), squares_of(map).len())
    }
}

/// What changed on a map since its squares were last looked at.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Needs {
    pub ground: bool,
    pub cells: bool,
    pub props: bool,
    pub trees: bool,
    pub buildings: bool,
}

impl Needs {
    pub fn all() -> Needs {
        Needs { ground: true, cells: true, props: true, trees: true, buildings: true }
    }
}

/// One map to stream: the open one, or one around it.
pub(super) struct Feed<'a> {
    pub id: &'a str,
    pub map: &'a WorldMap,
    pub gpu: &'a mut GpuState,
    pub content: &'a Content,
    pub models: &'a Models,
    /// the models' generations (`MapEditor::model_versions`)
    pub model_versions: &'a HashMap<String, u64>,
    pub level_height: f32,
    /// the maps next to it
    pub beyond: &'a dyn Beyond,
    /// the camera's target and distance, in the map's own space
    pub target: Vec3,
    pub distance: f32,
    /// how each building shows: cut above that floor (the walker's), its walls facing the
    /// camera there cut down, or whole
    pub cuts: &'a [Option<Cut>],
    /// a building being designed: the one left out (being edited), and the draft with the
    /// floor shown
    pub editing: Option<usize>,
    pub draft: Option<(&'a Building, usize)>,
    /// its actors stand where they were put (a map around the open one, where nobody moves)
    pub actors: bool,
    /// the editor's focus (the cells texture's blue channel)
    pub focus: Option<&'a [(i32, i32)]>,
    pub around_version: u64,
}

/// The square a cell is in.
pub(super) fn square(x: i32, z: i32) -> (i32, i32) {
    (x.div_euclid(CHUNK as i32), z.div_euclid(CHUNK as i32))
}

/// A square's cells: x0, z0 (in) to x1, z1 (out).
fn region(s: (i32, i32)) -> [i32; 4] {
    let n = CHUNK as i32;
    [s.0 * n, s.1 * n, (s.0 + 1) * n, (s.1 + 1) * n]
}

/// How far a point is from a square's nearest cell, in cells.
fn distance(s: (i32, i32), x: f32, z: f32) -> f32 {
    let n = CHUNK as f32;
    let (cx, cz) = ((s.0 as f32 + 0.5) * n, (s.1 as f32 + 0.5) * n);
    let (dx, dz) = ((x - cx).abs() - n / 2.0, (z - cz).abs() - n / 2.0);
    (dx.max(0.0).powi(2) + dz.max(0.0).powi(2)).sqrt()
}

/// A map's squares.
fn squares_of(map: &WorldMap) -> Vec<(i32, i32)> {
    let n = CHUNK as i32;
    let (gx, gz) = ((map.width as i32 + n - 1) / n, (map.height as i32 + n - 1) / n);
    (0..gz).flat_map(|z| (0..gx).map(move |x| (x, z))).collect()
}

/// Each prop record's digest, its model's generation in it: a square whose props, trees or
/// buildings use one that changed (made toon, given another model, its model's file come or
/// read again) is built again.
/// A quick hasher for the squares' digests, which read every cell of the map: SipHash's care
/// (against keys picked to collide) isn't needed here. FxHash's mix.
#[derive(Default)]
struct Quick(u64);

impl Quick {
    fn mix(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Quick {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..chunk.len()].copy_from_slice(chunk);
            self.mix(u64::from_le_bytes(b));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.mix(n as u64);
    }

    fn write_u16(&mut self, n: u16) {
        self.mix(n as u64);
    }

    fn write_u32(&mut self, n: u32) {
        self.mix(n as u64);
    }

    fn write_u64(&mut self, n: u64) {
        self.mix(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.mix(n as u64);
    }
}

fn record_digests<'a>(content: &'a Content, versions: &HashMap<String, u64>) -> HashMap<&'a str, u64> {
    content
        .props
        .iter()
        .map(|p| {
            let mut h = Quick::default();
            format!("{p:?}").hash(&mut h);
            versions.get(&p.id).hash(&mut h);
            (p.id.as_str(), h.finish())
        })
        .collect()
}

/// Whether a ramp covers any of a region's cells, or comes within `pad` cells of them.
fn overlaps(r: &content::Ramp, region: [i32; 4], pad: i32) -> bool {
    r.at[0] - pad < region[2] && r.at[1] - pad < region[3] && r.at[0] + r.size[0] as i32 + pad > region[0] && r.at[1] + r.size[1] as i32 + pad > region[1]
}

/// The cells texture's blue channel: 255 on the cells in the focus.
fn mark_focus(map: &WorldMap, cells: &mut [u8], focus: Option<&[(i32, i32)]>) {
    for &(x, z) in focus.unwrap_or(&[]) {
        if let Some(b) = mesh::cell_texel(map, x, z).and_then(|t| cells.get_mut(t * 4 + 2)) {
            *b = 255;
        }
    }
}

impl Feed<'_> {
    /// The squares to hold, nearest the camera's target first, and the reach past which
    /// held squares are dropped.
    fn wanted(&self) -> (Vec<(i32, i32)>, f32) {
        let reach = self.distance * 1.1 + 48.0;
        let t = self.target;
        let mut out: Vec<(f32, (i32, i32))> = squares_of(self.map).into_iter().map(|s| (distance(s, t.x, t.z), s)).filter(|(d, _)| *d <= reach).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        (out.into_iter().map(|(_, s)| s).collect(), reach + CHUNK as f32)
    }

    /// Takes a layer's digests: the squares whose digest changed lose their built mesh (and
    /// their far stand-in, which shows the ground, the woods and the buildings), so they're
    /// built again when wanted.
    fn refresh(&mut self, layer: &'static str, digests: BTreeMap<(i32, i32), u64>) {
        let squares = squares_of(self.map);
        let old = self.gpu.sources.entry(layer).or_default();
        let changed: Vec<(i32, i32)> = squares.into_iter().filter(|s| old.get(s) != Some(&digests.get(s).copied().unwrap_or(0))).collect();
        for s in &changed {
            old.insert(*s, digests.get(s).copied().unwrap_or(0));
        }
        let built = self.gpu.built.entry(layer).or_default();
        for s in &changed {
            built.remove(s);
        }
        if layer != "props" {
            let far = self.gpu.built.entry(FAR).or_default();
            for s in &changed {
                far.remove(s);
            }
        }
    }

    /// Is a square's layer on the GPU?
    fn built(&self, layer: &str, s: (i32, i32)) -> bool {
        self.gpu.built.get(layer).map(|b| b.contains(&s)).unwrap_or(false)
    }

    /// The ground: each square's levels and terrains, and two cells around it (the cliff
    /// faces come down onto the ground across, which comes up to them), past the edge the
    /// next maps'.
    fn ground_digests(&self) -> BTreeMap<(i32, i32), u64> {
        let m = self.map;
        squares_of(m)
            .into_iter()
            .map(|s| {
                let r = region(s);
                let mut h = Quick::default();
                for z in r[1] - 2..(r[3] + 2).min(m.height as i32 + 2) {
                    for x in r[0] - 2..(r[2] + 2).min(m.width as i32 + 2) {
                        match m.index(x, z) {
                            Some(i) => (m.level[i], m.terrain[i]).hash(&mut h),
                            None => (self.beyond.level(x, z), self.beyond.terrain(x, z)).hash(&mut h),
                        }
                    }
                }
                // the ramps on it and round it (their walls come down onto the ground across),
                // and whether something lies along each (its ground straight under it)
                for ramp in m.ramps.iter().filter(|q| overlaps(q, r, 3)) {
                    format!("{ramp:?}").hash(&mut h);
                    m.ramp_cover(ramp, self.content).is_some().hash(&mut h);
                }
                (s, h.finish())
            })
            .collect()
    }

    /// The props and the actors standing on each square (on a ramp's slope: the ramps there
    /// too).
    fn prop_digests(&self, records: &HashMap<&str, u64>) -> BTreeMap<(i32, i32), u64> {
        let mut by: BTreeMap<(i32, i32), Quick> = BTreeMap::new();
        for p in &self.map.props {
            let h = by.entry(square(p.at[0].floor() as i32, p.at[1].floor() as i32)).or_default();
            p.prop.hash(h);
            records.get(p.prop.as_str()).hash(h);
            (p.at[0].to_bits(), p.at[1].to_bits(), p.level, p.turn.to_bits(), p.scale.to_bits(), p.offset[0].to_bits(), p.offset[1].to_bits()).hash(h);
        }
        if self.actors {
            for a in &self.map.actors {
                let h = by.entry(square(a.at[0], a.at[1])).or_default();
                (&a.look, a.at, a.level, a.facing.to_bits()).hash(h);
                a.look.strip_prefix("prop:").and_then(|id| records.get(id)).hash(h);
            }
        }
        for (s, h) in by.iter_mut() {
            for ramp in self.map.ramps.iter().filter(|q| overlaps(q, region(*s), 0)) {
                format!("{ramp:?}").hash(h);
            }
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    fn tree_digests(&self, records: &HashMap<&str, u64>) -> BTreeMap<(i32, i32), u64> {
        let mut by: BTreeMap<(i32, i32), Quick> = BTreeMap::new();
        for t in &self.gpu.trees {
            let h = by.entry(square(t.pos[0].floor() as i32, t.pos[2].floor() as i32)).or_default();
            t.prop.hash(h);
            records.get(t.prop.as_str()).hash(h);
            (t.pos[0].to_bits(), t.pos[1].to_bits(), t.pos[2].to_bits(), t.scale.to_bits(), t.turn.to_bits()).hash(h);
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    /// The buildings by the square of their corner, with how each is shown: cut above the
    /// walker's floor, left out while it's edited (the draft takes its place).
    fn building_digests(&self, records: &HashMap<&str, u64>) -> BTreeMap<(i32, i32), u64> {
        let mut by: BTreeMap<(i32, i32), Quick> = BTreeMap::new();
        for (i, b) in self.map.buildings.iter().enumerate() {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            if Some(i) == self.editing {
                "editing".hash(h);
                continue;
            }
            let cut = self.cuts.get(i).copied().flatten();
            format!("{b:?} {cut:?}").hash(h);
            // the props it's made of
            let floors = b.floors.iter().flat_map(|f| std::iter::once(&f.tile).chain(f.openings.iter().map(|o| &o.prop)).chain(f.props.iter().map(|p| &p.prop)));
            let rest = b.pillars.iter().map(|p| &p.prop).chain(b.anchors.iter().map(|a| &a.prop)).chain(b.outside.iter().map(|p| &p.prop));
            for id in [&b.wall, &b.roof].into_iter().chain(floors).chain(rest) {
                records.get(id.as_str()).hash(h);
            }
        }
        if let Some((b, floor)) = self.draft {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            format!("draft {b:?} floor {floor}").hash(h);
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    /// Builds one square of one layer and sends it.
    fn build_square(&mut self, r: &mut SceneRenderer, device: &eframe::wgpu::Device, layer: &str, s: (i32, i32)) {
        let (reg, lh, id) = (region(s), self.level_height, self.id);
        match layer {
            "ground" => {
                let (tops, cliffs, grass) = mesh::ground_region(self.map, self.content, lh, reg, self.beyond);
                r.set_chunk(device, id, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(device, id, CLIFFS, s, &[(WHITE.to_string(), gpu(&cliffs.vertices), cliffs.indices)]);
                r.set_chunk(device, id, "grass", s, &models::batches(&Scene::from(grass)));
            }
            "props" => {
                let mut scene = mesh::props_region(self.map, self.content, lh, self.models, reg);
                if self.actors {
                    // the people standing where they were put
                    for a in self.map.actors.iter().filter(|a| square(a.at[0], a.at[1]) == s) {
                        let Some(prop) = a.look.strip_prefix("prop:").and_then(|id| self.content.props.get(id)) else { continue };
                        let (x, z) = (a.at[0] as f32 + 0.5, a.at[1] as f32 + 0.5);
                        let at = Vec3::new(x, self.map.ground_y(x, z, a.level, lh), z);
                        scene.append(&prop_scene(prop, self.models), placement(at, a.facing as f32, 1.0));
                    }
                }
                r.set_chunk(device, id, "props", s, &models::batches(&scene));
                self.gpu.props_vertices.insert(s, scene.plain.vertices.len() + scene.textured.values().map(|m| m.vertices.len()).sum::<usize>());
            }
            "trees" => {
                let scene = mesh::trees_from(&self.gpu.trees, self.content, self.models, reg);
                r.set_chunk(device, id, "trees", s, &models::batches(&scene));
            }
            "far" => {
                let (tops, plain) = mesh::far_region(self.map, self.content, lh, reg);
                r.set_chunk(device, id, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(device, id, FAR, s, &models::batches(&Scene::from(plain)));
            }
            _ => {
                let mut m = Scene::default();
                for (i, b) in self.map.buildings.iter().enumerate().filter(|(_, b)| square(b.at[0], b.at[1]) == s) {
                    if Some(i) != self.editing {
                        let cut = self.cuts.get(i).copied().flatten();
                        m.append(&building(b, self.content, lh, cut, self.models), Mat4::IDENTITY);
                    }
                }
                if let Some((b, floor)) = self.draft.filter(|(b, _)| square(b.at[0], b.at[1]) == s) {
                    m.append(&building(b, self.content, lh, Some(Cut { top: floor, low: Sides::NONE }), self.models), Mat4::IDENTITY);
                }
                r.set_chunk(device, id, "buildings", s, &models::batches(&m));
            }
        }
    }
}

/// Streams one map: its ground's texture, the squares that changed or came within reach (in
/// full, nearest first, while the frame's budget lasts from `start`), the stand-ins of the
/// rest.
pub(super) fn stream(r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState, f: &mut Feed, mut need: Needs, start: f64) {
    let (w, h) = (f.map.width, f.map.height);
    // the ground's texture and palette: a new map, a resized one, one terrain more; the
    // cells again when painted, or when the maps around it changed (the ring)
    let edges = f.gpu.around != f.around_version;
    // what each step of a change costs (the `streamcost` hook: how big maps hold up)
    let mut costs: Vec<(&str, f64)> = vec![];
    let mut tick = crate::platform::now_ms();
    let mut cost = |what: &'static str, costs: &mut Vec<(&str, f64)>| {
        let now = crate::platform::now_ms();
        costs.push((what, now - tick));
        tick = now;
    };
    let key = (f.id.to_string(), w, h, f.map.terrains.clone());
    if f.gpu.ground.as_ref() != Some(&key) {
        let (palette, palette_width) = mesh::palette(f.content);
        let mut cells = mesh::cells_texture(f.map, f.content, f.beyond);
        mark_focus(f.map, &mut cells, f.focus);
        let size = [w as u32 + 2, h as u32 + 2];
        r.set_world_ground(&rs.device, &rs.queue, f.id, &WorldGround { cells: &cells, size, palette: &palette, palette_width, level_height: f.level_height });
        if f.gpu.ground.as_ref().map(|g| (g.0.as_str(), g.1, g.2) != (f.id, w, h)).unwrap_or(true) {
            // another map, or another size: nothing held is right any more
            f.gpu.built.clear();
            f.gpu.sources.clear();
            need = Needs::all();
        }
        f.gpu.ground = Some(key);
    } else if need.cells || edges {
        let mut cells = mesh::cells_texture(f.map, f.content, f.beyond);
        mark_focus(f.map, &mut cells, f.focus);
        cost("cells", &mut costs);
        r.set_world_cells(&rs.queue, f.id, &cells);
        cost("upload", &mut costs);
    }
    f.gpu.around = f.around_version;
    // what changed, square by square (props too can change the ground: what lies along a
    // ramp makes it straight)
    if need.ground || edges || (need.props && !f.map.ramps.is_empty()) {
        let d = f.ground_digests();
        f.refresh("ground", d);
        cost("ground", &mut costs);
    }
    if need.props || need.trees || need.buildings {
        let records = record_digests(f.content, f.model_versions);
        if need.trees {
            f.gpu.trees = trees(f.map, f.content, f.level_height);
            cost("trees", &mut costs);
            let d = f.tree_digests(&records);
            f.refresh("trees", d);
            cost("treedigests", &mut costs);
        }
        if need.props {
            let d = f.prop_digests(&records);
            f.refresh("props", d);
            cost("props", &mut costs);
        }
        if need.buildings {
            let d = f.building_digests(&records);
            f.refresh("buildings", d);
            cost("buildings", &mut costs);
        }
    }
    if costs.iter().any(|c| c.1 > 1.0) {
        crate::platform::set_hook("streamcost", &costs.iter().map(|(w, t)| format!("{w} {t:.1}")).collect::<Vec<_>>().join(", "));
    }
    let (wanted, keep) = f.wanted();
    let near: BTreeSet<(i32, i32)> = wanted.iter().copied().collect();
    let t = f.target;
    let all = squares_of(f.map);
    // the squares that left the camera's reach: their full layers go, a stand-in follows
    let demote: Vec<(i32, i32)> = all.iter().copied().filter(|&s| distance(s, t.x, t.z) > keep && f.built("ground", s)).collect();
    for s in demote {
        for layer in [CLIFFS, "grass", "props", "trees", "buildings"] {
            r.set_chunk(&rs.device, f.id, layer, s, &[]);
        }
        for layer in LAYERS {
            if let Some(b) = f.gpu.built.get_mut(layer) {
                b.remove(&s);
            }
        }
        f.gpu.props_vertices.remove(&s);
    }
    // the squares in reach, nearest first and in full, as many as the frame's budget allows
    let over = || crate::platform::now_ms() - start > BUDGET_MS;
    'build: for &s in &wanted {
        for layer in LAYERS {
            if f.built(layer, s) {
                continue;
            }
            f.build_square(r, &rs.device, layer, s);
            f.gpu.built.entry(layer).or_default().insert(s);
            if layer == "ground" {
                // the full ground took the stand-in's place
                r.set_chunk(&rs.device, f.id, FAR, s, &[]);
                if let Some(b) = f.gpu.built.get_mut(FAR) {
                    b.remove(&s);
                }
            }
            if over() {
                break 'build;
            }
        }
    }
    // the rest as stand-ins, nearest first, with what's left of the budget
    if !over() {
        let mut far: Vec<(f32, (i32, i32))> = all.iter().copied().filter(|&s| !near.contains(&s) && !f.built("ground", s) && !f.built(FAR, s)).map(|s| (distance(s, t.x, t.z), s)).collect();
        far.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, s) in far {
            f.build_square(r, &rs.device, FAR, s);
            f.gpu.built.entry(FAR).or_default().insert(s);
            if over() {
                break;
            }
        }
    }
}

impl MapEditor {
    /// Every frame: the open map's ground and squares (what changed, what came within
    /// reach), then the maps around it, then what moves.
    pub(super) fn sync(&mut self, rs: &eframe::egui_wgpu::RenderState, now: f64) {
        let focus = self.focus_cells();
        if focus != self.focus_sent {
            self.need_cells = true;
        }
        if self.need_ground || self.need_cells || self.need_props || self.need_trees || self.need_buildings {
            self.blocked = None;
        }
        let mut guard = rs.renderer.write();
        let Some(r) = guard.callback_resources.get_mut::<SceneRenderer>() else { return };
        self.load_sprites(r, rs);
        self.load_models(r, rs);
        self.load_fx(r, rs);
        self.hold_around();
        let start = crate::platform::now_ms();
        // the open map
        let need = Needs { ground: self.need_ground, cells: self.need_cells, props: self.need_props, trees: self.need_trees, buildings: self.need_buildings };
        (self.need_ground, self.need_cells, self.need_props, self.need_trees, self.need_buildings) = (false, false, false, false, false);
        // the props' and buildings' lights and sounds again, when they (or the ground under
        // them) changed
        if need.props || need.buildings || need.ground {
            self.placed_lights = None;
            self.placed_sounds = None;
        }
        if self.placed_sounds.is_none() {
            self.placed_sounds = Some(world::sounds::sounds(&self.map, &self.content, self.level_height));
        }
        if need.ground || need.cells {
            // its edge, as the maps around see it
            self.note_edge();
        }
        if self.gpu.ground.as_ref().map(|g| g.0 != self.map.id).unwrap_or(true) {
            // a map other than the one before (or the first): no battle map under it
            r.clear_map();
        }
        let cuts: Vec<Option<Cut>> = (0..self.map.buildings.len()).map(|i| self.building_cut(i)).collect();
        let origin = self.around.origin.unwrap_or([0, 0]);
        let next = self.around.next(&self.content, origin, None);
        let draft = self.draft.as_ref().filter(|d| d.step == build::Step::Design).map(|d| (&d.b, d.floor));
        let mut feed = Feed {
            id: &self.map.id,
            map: &self.map,
            gpu: &mut self.gpu,
            content: &self.content,
            models: &self.models,
            model_versions: &self.model_versions,
            level_height: self.level_height,
            beyond: &next,
            target: self.cam.target,
            distance: self.cam.distance,
            cuts: &cuts,
            editing: self.draft.as_ref().and_then(|d| d.editing),
            draft,
            actors: false,
            focus: focus.as_deref(),
            around_version: self.around.version,
        };
        stream(r, rs, &mut feed, need, start);
        r.set_world_place(&self.map.id, Place { offset: Vec3::ZERO, grid: [origin[0] as f32, origin[1] as f32], dim: 0.0 });
        self.focus_sent = focus;
        // the maps around it, with what's left of the frame's budget
        self.stream_around(r, rs, start);
        let open = self.map.id.clone();
        let around = &self.around.maps;
        r.keep_worlds(&|id| id == open || around.contains_key(id));
        if self.extra_changed {
            let (extra, fade) = self.extra_batches(now);
            r.set_world_extra(&rs.device, &extra);
            r.set_world_fade(&rs.device, &fade);
            self.extra_changed = false;
        }
        let (held, total) = self.gpu.held(&self.map);
        crate::platform::set_hook("squares", &format!("{held}/{total}"));
        crate::platform::set_hook("far", &self.gpu.built.get(FAR).map(|b| b.len()).unwrap_or(0).to_string());
        crate::platform::set_hook("propsverts", &self.gpu.props_vertices.values().sum::<usize>().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Prop, Space};

    #[test]
    fn a_model_arriving_changes_its_props_digest() {
        let mut c = Content::default();
        c.props.put(Prop { id: "pole".into(), model: Some("models/pole.glb".into()), ..Default::default() }, Space::Content);
        let before = record_digests(&c, &HashMap::new())["pole"];
        let mut loaded = HashMap::new();
        loaded.insert("pole".to_string(), 1u64);
        let after = record_digests(&c, &loaded)["pole"];
        assert_ne!(before, after, "the file came: the squares using the prop are built again");
        loaded.insert("pole".to_string(), 2);
        assert_ne!(after, record_digests(&c, &loaded)["pole"], "read again: again");
    }
}
