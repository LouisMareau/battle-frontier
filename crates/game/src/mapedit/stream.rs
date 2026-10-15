//! The world map on the GPU, square by square (CHUNK cells a side): the squares within
//! reach of the camera are built and sent, nearest first and a few per frame, the far ones
//! dropped; and an edit rebuilds the squares it touched and no more, since every layer
//! keeps a digest of what each square was built from. A map of any size costs what's in
//! reach, not its size.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use content::CHUNK;
use render::scene::{SceneRenderer, WorldGround, WHITE};
use world::buildings::{building, Cut};
use world::mesh::{self, Scene};
use world::trees::{trees, Tree};

use super::{build, gpu, models, MapEditor};

/// The layers built square by square.
const LAYERS: [&str; 4] = ["ground", "props", "trees", "buildings"];
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

    /// Takes a layer's digests: the squares whose digest changed lose their built mesh, so
    /// they're built again when wanted.
    fn refresh(&mut self, layer: &'static str, digests: BTreeMap<(i32, i32), u64>) {
        let squares = self.squares();
        let old = self.gpu.sources.entry(layer).or_default();
        let built = self.gpu.built.entry(layer).or_default();
        for s in squares {
            let d = digests.get(&s).copied().unwrap_or(0);
            if old.get(&s) != Some(&d) {
                built.remove(&s);
                old.insert(s, d);
            }
        }
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
            (p.at[0].to_bits(), p.at[1].to_bits(), p.level, p.turn.to_bits(), p.scale.to_bits()).hash(h);
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
        let inside = self.walker.as_ref().and_then(|w| w.inside);
        let mut by: BTreeMap<(i32, i32), DefaultHasher> = BTreeMap::new();
        for (i, b) in self.map.buildings.iter().enumerate() {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            if Some(i) == editing {
                "editing".hash(h);
                continue;
            }
            let cut = inside.filter(|(w, _)| *w == i).map(|(_, f)| f);
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
                let (tops, solid) = mesh::ground_region(&self.map, &self.content, lh, reg);
                r.set_chunk(&rs.device, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(&rs.device, "cliffs", s, &models::batches(&Scene::from(solid)));
            }
            "props" => {
                let scene = mesh::props_region(&self.map, &self.content, lh, &self.models, reg);
                r.set_chunk(&rs.device, "props", s, &models::batches(&scene));
            }
            "trees" => {
                let scene = mesh::trees_from(&self.gpu.trees, &self.content, &self.models, reg);
                r.set_chunk(&rs.device, "trees", s, &models::batches(&scene));
            }
            _ => {
                let editing = self.draft.as_ref().and_then(|d| d.editing);
                let inside = self.walker.as_ref().and_then(|w| w.inside);
                let mut m = Scene::default();
                for (i, b) in self.map.buildings.iter().enumerate().filter(|(_, b)| square(b.at[0], b.at[1]) == s) {
                    if Some(i) != editing {
                        let cut = inside.filter(|(w, _)| *w == i).map(|(_, f)| Cut { top: f });
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
    /// (a few per frame), the far ones dropped, and what moves.
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
        // the squares in reach, nearest first, as many as the frame's budget allows
        let (wanted, keep) = self.wanted();
        let start = crate::platform::now_ms();
        'build: for &s in &wanted {
            for layer in LAYERS {
                if self.gpu.built.get(layer).map(|b| b.contains(&s)).unwrap_or(false) {
                    continue;
                }
                self.build_square(r, rs, layer, s);
                self.gpu.built.entry(layer).or_default().insert(s);
                if crate::platform::now_ms() - start > BUDGET_MS {
                    break 'build;
                }
            }
        }
        // the far squares go
        let t = self.cam.target;
        let near = move |s: (i32, i32)| distance(s, t.x, t.z) <= keep;
        let mut dropped = false;
        for b in self.gpu.built.values_mut() {
            let before = b.len();
            b.retain(|&s| near(s));
            dropped |= b.len() != before;
        }
        if dropped {
            r.keep_chunks(&near);
        }
        if self.extra_changed {
            let extra = self.extra_mesh(now);
            r.set_world_extra(&rs.device, &models::batches(&extra));
            self.extra_changed = false;
        }
        let held = self.gpu.built.get("ground").map(|b| b.len()).unwrap_or(0);
        crate::platform::set_hook("squares", &format!("{held}/{}", self.squares().len()));
    }
}
