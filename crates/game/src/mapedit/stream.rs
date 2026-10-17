//! The world map on the GPU, square by square (CHUNK cells a side): the squares within
//! reach of the camera are built in full and sent, nearest first and a few per frame; the
//! rest get a cheap stand-in (the ground in blocks, a canopy over the woods, a box per
//! building) so a map of any size shows whole from afar, and a square swaps between the
//! two as the camera moves. An edit rebuilds the squares it touched and no more, since
//! every layer keeps a digest of what each square was built from.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use content::CHUNK;
use render::scene::{SceneRenderer, WorldGround, CLIFFS, WHITE};
use world::buildings::{building, Cut};
use world::mesh::{self, Scene};
use world::trees::{trees, Tree};

use super::{build, gpu, models, MapEditor};

/// The layers built square by square, in full.
const LAYERS: [&str; 4] = ["ground", "props", "trees", "buildings"];
/// The stand-in of a square out of reach: `mesh::far_region`.
const FAR: &str = "far";
/// How much of a frame building squares may take, in milliseconds.
const BUDGET_MS: f64 = 6.0;

#[derive(Default)]
pub(super) struct GpuState {
    /// what the ground's texture and palette were sent for: width, height, terrains
    ground: Option<(usize, usize, Vec<String>)>,
    /// the squares each layer holds on the GPU
    built: BTreeMap<&'static str, BTreeSet<(i32, i32)>>,
    /// per layer and square, a digest of what it's made from
    sources: BTreeMap<&'static str, BTreeMap<(i32, i32), u64>>,
    /// the trees, as last laid out
    trees: Vec<Tree>,
    /// how many models were loaded when the digests were taken (one more changes meshes)
    models: usize,
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

impl MapEditor {
    /// The map's squares.
    fn squares(&self) -> Vec<(i32, i32)> {
        let n = CHUNK as i32;
        let (gx, gz) = ((self.map.width as i32 + n - 1) / n, (self.map.height as i32 + n - 1) / n);
        (0..gz).flat_map(|z| (0..gx).map(move |x| (x, z))).collect()
    }

    /// The squares to hold, nearest the camera's target first, and the reach past which
    /// held squares are dropped.
    fn wanted(&self) -> (Vec<(i32, i32)>, f32) {
        let reach = self.cam.distance * 1.1 + 48.0;
        let t = self.cam.target;
        let mut out: Vec<(f32, (i32, i32))> = self.squares().into_iter().map(|s| (distance(s, t.x, t.z), s)).filter(|(d, _)| *d <= reach).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        (out.into_iter().map(|(_, s)| s).collect(), reach + CHUNK as f32)
    }

    /// Takes a layer's digests: the squares whose digest changed lose their built mesh (and
    /// their far stand-in, which shows the ground, the woods and the buildings), so they're
    /// built again when wanted.
    fn refresh(&mut self, layer: &'static str, digests: BTreeMap<(i32, i32), u64>) {
        let squares = self.squares();
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

    /// The ground: each square's levels and terrains, and its neighbours' (the cliff faces
    /// look across).
    fn ground_digests(&self) -> BTreeMap<(i32, i32), u64> {
        let m = &self.map;
        self.squares()
            .into_iter()
            .map(|s| {
                let r = region(s);
                let mut h = DefaultHasher::new();
                for z in (r[1] - 1).max(0)..(r[3] + 1).min(m.height as i32) {
                    for x in (r[0] - 1).max(0)..(r[2] + 1).min(m.width as i32) {
                        let i = z as usize * m.width + x as usize;
                        m.level[i].hash(&mut h);
                        m.terrain[i].hash(&mut h);
                    }
                }
                (s, h.finish())
            })
            .collect()
    }

    fn prop_digests(&self) -> BTreeMap<(i32, i32), u64> {
        let mut by: BTreeMap<(i32, i32), DefaultHasher> = BTreeMap::new();
        for p in &self.map.props {
            let h = by.entry(square(p.at[0].floor() as i32, p.at[1].floor() as i32)).or_default();
            p.prop.hash(h);
            (p.at[0].to_bits(), p.at[1].to_bits(), p.level, p.turn.to_bits(), p.scale.to_bits(), p.offset[0].to_bits(), p.offset[1].to_bits()).hash(h);
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    fn tree_digests(&self) -> BTreeMap<(i32, i32), u64> {
        let mut by: BTreeMap<(i32, i32), DefaultHasher> = BTreeMap::new();
        for t in &self.gpu.trees {
            let h = by.entry(square(t.pos[0].floor() as i32, t.pos[2].floor() as i32)).or_default();
            t.prop.hash(h);
            (t.pos[0].to_bits(), t.pos[1].to_bits(), t.pos[2].to_bits(), t.scale.to_bits(), t.turn.to_bits()).hash(h);
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    /// The buildings by the square of their corner, with how each is shown: cut above the
    /// walker's floor, left out while it's edited (the draft takes its place).
    fn building_digests(&self) -> BTreeMap<(i32, i32), u64> {
        let editing = self.draft.as_ref().and_then(|d| d.editing);
        let mut by: BTreeMap<(i32, i32), DefaultHasher> = BTreeMap::new();
        for (i, b) in self.map.buildings.iter().enumerate() {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            if Some(i) == editing {
                "editing".hash(h);
                continue;
            }
            let cut = self.building_cut(i);
            format!("{b:?} {cut:?}").hash(h);
        }
        if let Some(d) = self.draft.as_ref().filter(|d| d.step == build::Step::Design) {
            let h = by.entry(square(d.b.at[0], d.b.at[1])).or_default();
            format!("draft {:?} floor {}", d.b, d.floor).hash(h);
        }
        by.into_iter().map(|(s, h)| (s, h.finish())).collect()
    }

    /// Builds one square of one layer and sends it.
    fn build_square(&self, r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState, layer: &str, s: (i32, i32)) {
        let reg = region(s);
        let lh = self.level_height;
        match layer {
            "ground" => {
                let (tops, cliffs, grass) = mesh::ground_region(&self.map, &self.content, lh, reg);
                r.set_chunk(&rs.device, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(&rs.device, CLIFFS, s, &[(WHITE.to_string(), gpu(&cliffs.vertices), cliffs.indices)]);
                r.set_chunk(&rs.device, "grass", s, &models::batches(&Scene::from(grass)));
            }
            "props" => {
                let scene = mesh::props_region(&self.map, &self.content, lh, &self.models, reg);
                r.set_chunk(&rs.device, "props", s, &models::batches(&scene));
            }
            "trees" => {
                let scene = mesh::trees_from(&self.gpu.trees, &self.content, &self.models, reg);
                r.set_chunk(&rs.device, "trees", s, &models::batches(&scene));
            }
            "far" => {
                let (tops, plain) = mesh::far_region(&self.map, &self.content, lh, reg);
                r.set_chunk(&rs.device, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(&rs.device, FAR, s, &models::batches(&Scene::from(plain)));
            }
            _ => {
                let editing = self.draft.as_ref().and_then(|d| d.editing);
                let mut m = Scene::default();
                for (i, b) in self.map.buildings.iter().enumerate().filter(|(_, b)| square(b.at[0], b.at[1]) == s) {
                    if Some(i) != editing {
                        let cut = self.building_cut(i).map(|top| Cut { top });
                        m.append(&building(b, &self.content, lh, cut, &self.models), glam::Mat4::IDENTITY);
                    }
                }
                if let Some(d) = self.draft.as_ref().filter(|d| d.step == build::Step::Design && square(d.b.at[0], d.b.at[1]) == s) {
                    m.append(&building(&d.b, &self.content, lh, Some(Cut { top: d.floor }), &self.models), glam::Mat4::IDENTITY);
                }
                r.set_chunk(&rs.device, "buildings", s, &models::batches(&m));
            }
        }
    }

    /// Every frame: the ground's texture, the squares that changed or came within reach
    /// (a few per frame, in full), the stand-ins of the rest, and what moves.
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
        // the ground's texture and palette: a new map, a resized one, or one terrain more
        let key = (self.map.width, self.map.height, self.map.terrains.clone());
        if self.gpu.ground.as_ref() != Some(&key) {
            r.clear_map();
            let (palette, palette_width) = mesh::palette(&self.map, &self.content);
            let mut cells = mesh::cells_texture(&self.map);
            self.mark_focus(&mut cells, focus.as_deref());
            let size = [self.map.width as u32, self.map.height as u32];
            r.set_world_ground(&rs.device, &rs.queue, &WorldGround { cells: &cells, size, palette: &palette, palette_width, level_height: self.level_height });
            if self.gpu.ground.as_ref().map(|g| (g.0, g.1) != (key.0, key.1)).unwrap_or(true) {
                // another size: nothing held is right any more
                self.gpu.built.clear();
                self.gpu.sources.clear();
            }
            self.gpu.ground = Some(key);
            self.need_cells = false;
        } else if self.need_cells {
            let mut cells = mesh::cells_texture(&self.map);
            self.mark_focus(&mut cells, focus.as_deref());
            r.set_world_cells(&rs.queue, &cells);
            self.need_cells = false;
        }
        self.focus_sent = focus;
        // a model that arrived changes every mesh it's in
        if self.models.len() != self.gpu.models {
            self.gpu.models = self.models.len();
            self.need_props = true;
            self.need_trees = true;
            self.need_buildings = true;
        }
        // what changed, square by square
        if self.need_ground {
            let d = self.ground_digests();
            self.refresh("ground", d);
            self.need_ground = false;
        }
        if self.need_trees {
            self.gpu.trees = trees(&self.map, self.level_height);
            let d = self.tree_digests();
            self.refresh("trees", d);
            self.need_trees = false;
        }
        if self.need_props {
            let d = self.prop_digests();
            self.refresh("props", d);
            self.need_props = false;
        }
        if self.need_buildings {
            let d = self.building_digests();
            self.refresh("buildings", d);
            self.need_buildings = false;
        }
        let (wanted, keep) = self.wanted();
        let near: BTreeSet<(i32, i32)> = wanted.iter().copied().collect();
        let t = self.cam.target;
        let all = self.squares();
        // the squares that left the camera's reach: their full layers go, a stand-in follows
        let demote: Vec<(i32, i32)> = all.iter().copied().filter(|&s| distance(s, t.x, t.z) > keep && self.built("ground", s)).collect();
        for s in demote {
            for layer in [CLIFFS, "grass", "props", "trees", "buildings"] {
                r.set_chunk(&rs.device, layer, s, &[]);
            }
            for layer in LAYERS {
                if let Some(b) = self.gpu.built.get_mut(layer) {
                    b.remove(&s);
                }
            }
        }
        // the squares in reach, nearest first and in full, as many as the frame's budget allows
        let start = crate::platform::now_ms();
        let over = || crate::platform::now_ms() - start > BUDGET_MS;
        'build: for &s in &wanted {
            for layer in LAYERS {
                if self.built(layer, s) {
                    continue;
                }
                self.build_square(r, rs, layer, s);
                self.gpu.built.entry(layer).or_default().insert(s);
                if layer == "ground" {
                    // the full ground took the stand-in's place
                    r.set_chunk(&rs.device, FAR, s, &[]);
                    if let Some(f) = self.gpu.built.get_mut(FAR) {
                        f.remove(&s);
                    }
                }
                if over() {
                    break 'build;
                }
            }
        }
        // the rest as stand-ins, nearest first, with what's left of the budget
        if !over() {
            let mut far: Vec<(f32, (i32, i32))> = all.iter().copied().filter(|&s| !near.contains(&s) && !self.built("ground", s) && !self.built(FAR, s)).map(|s| (distance(s, t.x, t.z), s)).collect();
            far.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, s) in far {
                self.build_square(r, rs, FAR, s);
                self.gpu.built.entry(FAR).or_default().insert(s);
                if over() {
                    break;
                }
            }
        }
        if self.extra_changed {
            let (extra, fade) = self.extra_batches(now);
            r.set_world_extra(&rs.device, &extra);
            r.set_world_fade(&rs.device, &fade);
            self.extra_changed = false;
        }
        let held = self.gpu.built.get("ground").map(|b| b.len()).unwrap_or(0);
        crate::platform::set_hook("squares", &format!("{held}/{}", all.len()));
        crate::platform::set_hook("far", &self.gpu.built.get(FAR).map(|b| b.len()).unwrap_or(0).to_string());
    }
}
