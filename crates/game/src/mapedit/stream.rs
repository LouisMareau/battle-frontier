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
//! terrain borders running across. The trees are placed again where a change can move them
//! (`world::trees::Woods`), the buildings' digests worked out again for those that changed.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use content::{Building, Content, Placed, Prop, CHUNK};
use glam::{Mat4, Vec3};
use render::blades::{BladeInstance, BladeKind};
use render::grass::{GrassCopy, GrassModel};
use render::scene::{Place, SceneRenderer, WorldGround, CLIFFS, WHITE};
use world::buildings::{building, Cut, Sides};
use world::mesh::{self, placement, prop_scene, Beyond, Models, Scene};
use world::trees::{Tree, Woods};
use world::{same, Quick, QuickMap, WorldMap};

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
    /// terrains, the palette's digest (a terrain's colours or look changed: sent again)
    pub(super) ground: Option<(String, usize, usize, Vec<String>, u64)>,
    /// the squares each layer holds on the GPU
    built: BTreeMap<&'static str, BTreeSet<(i32, i32)>>,
    /// per layer and square, a digest of what it's made from
    sources: BTreeMap<&'static str, BTreeMap<(i32, i32), u64>>,
    /// the cells' terrains and levels as last looked at for what changed (`cells_changed`),
    /// the cells changed since that the texture hasn't caught up with, and the squares whose
    /// ground digest hasn't
    seen_terrain: Vec<u16>,
    seen_level: Vec<u8>,
    texels_due: Vec<(i32, i32)>,
    ground_due: BTreeSet<(i32, i32)>,
    /// the cells' texture as sent, and the focus marked on it
    texture: Vec<u8>,
    focus: Vec<(i32, i32)>,
    /// each square's digest of its cells and the two rows round them (its ramps left out)
    ground_cells: BTreeMap<(i32, i32), u64>,
    /// the props as last looked at for what changed, and each square's digest of its props
    /// (their records left out) with the props they are
    props_seen: Vec<Placed>,
    prop_squares: BTreeMap<(i32, i32), (u64, Vec<String>)>,
    /// the trees as placed, and by square
    woods: Woods,
    groves: BTreeMap<(i32, i32), Grove>,
    /// each building's digest by its id, with the building it's for, and the props' records'
    /// digests they were worked out with
    building_digests: HashMap<String, (Building, u64)>,
    records: HashMap<String, u64>,
    /// what the props' lights and sounds were last placed from (`stream` says when they're
    /// to be placed again): the props' records, the buildings, the ramps and the ground on
    /// them
    lit_records: u64,
    lit_buildings: u64,
    lit_ramps: Vec<content::Ramp>,
    ramp_ground_changed: bool,
    /// what the grass props grown through tall ground were last built from (`growth_digest`)
    lit_growth: u64,
    /// what the terrains were when the ground was last built (`terrains_digest`: how each is
    /// drawn and walked; every square's ground holds it)
    lit_terrains: u64,
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

/// A square's trees, with a digest of them (their props' records left out) and the props
/// they are.
struct Grove {
    trees: Vec<Tree>,
    digest: u64,
    props: Vec<String>,
}

impl Grove {
    fn new(trees: Vec<Tree>) -> Grove {
        let mut h = Quick::default();
        let mut props: Vec<String> = vec![];
        for t in &trees {
            t.prop.hash(&mut h);
            (t.pos[0].to_bits(), t.pos[1].to_bits(), t.pos[2].to_bits(), t.scale.to_bits(), t.turn.to_bits()).hash(&mut h);
            if !props.contains(&t.prop) {
                props.push(t.prop.clone());
            }
        }
        Grove { trees, digest: h.finish(), props }
    }
}

/// What the steps of the map editor's frame cost (the `streamcost` hook).
struct Costs {
    /// the steps so far, with what each cost (in milliseconds)
    steps: Vec<(&'static str, f64)>,
    /// when the frame began
    began: f64,
    /// the costliest frame lately: what it cost, and when
    worst: f64,
    when: f64,
}

thread_local! {
    static COSTS: RefCell<Costs> = const { RefCell::new(Costs { steps: vec![], began: 0.0, worst: 0.0, when: 0.0 }) };
}

/// The map editor's frame begins (what its steps cost: `cost`).
pub(super) fn frame_begins() {
    COSTS.with(|c| {
        let mut c = c.borrow_mut();
        c.steps.clear();
        c.began = crate::platform::now_ms();
    });
}

/// A step of the frame done, begun at `since`: what it cost. The time it ended.
pub(super) fn cost(what: &'static str, since: f64) -> f64 {
    let now = crate::platform::now_ms();
    COSTS.with(|c| c.borrow_mut().steps.push((what, now - since)));
    now
}

/// The frame done: its steps (and the rest of it) kept in the `streamcost` hook when it's
/// the costliest of the last two seconds, what a change cost at its worst.
pub(super) fn frame_ends() {
    let now = crate::platform::now_ms();
    COSTS.with(|c| {
        let c = &mut *c.borrow_mut();
        let total = now - c.began;
        if total > 1.0 && (total >= c.worst || now - c.when > 2000.0) && c.steps.iter().any(|s| s.1 > 1.0) {
            (c.worst, c.when) = (total, now);
            let rest = total - c.steps.iter().map(|s| s.1).sum::<f64>();
            let text: Vec<String> = c.steps.iter().chain([&("rest", rest)]).filter(|s| s.1 >= 0.1).map(|(w, t)| format!("{w} {t:.1}")).collect();
            crate::platform::set_hook("streamcost", &text.join(", "));
        }
        c.steps.clear();
    });
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

/// A placed container taken (the story's flag "took:MAP:X:Z", the container's cells' middle
/// floored), and its prop naming an emptied look the content has: that look's id.
pub(super) fn emptied_look<'a>(map: &WorldMap, content: &'a Content, flags: &BTreeMap<String, i64>, p: &content::Placed) -> Option<&'a str> {
    let prop = content.props.get(&p.prop)?;
    let look = prop.emptied.as_deref().filter(|e| content.props.contains(e))?;
    flags.contains_key(&format!("took:{}:{}:{}", map.id, p.at[0].floor() as i64, p.at[1].floor() as i64)).then_some(look)
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
    /// the grass props' models in levels (`MapEditor::grass_levels`)
    pub grass: &'a RefCell<world::grass::Cache>,
    pub level_height: f32,
    /// the story's flags: a container taken ("took:MAP:X:Z") draws as its emptied look
    pub flags: &'a BTreeMap<String, i64>,
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
fn record_digests<'a>(content: &'a Content, versions: &HashMap<String, u64>) -> QuickMap<&'a str, u64> {
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

/// What a grass prop's model in levels is made from: its record, its model's generation.
pub(super) fn grass_version(prop: &Prop, versions: &HashMap<String, u64>) -> u64 {
    let mut h = Quick::default();
    format!("{prop:?}").hash(&mut h);
    versions.get(&prop.id).hash(&mut h);
    h.finish()
}

/// A grass prop's weight for growing (`mesh::GROWN_MOST`): its vertices at its thinnest, or
/// more than anything when its model is heavier than `mesh::GROWN_HEAVIEST` a cell as made.
pub(super) fn grass_weight(cache: &RefCell<world::grass::Cache>, prop: &Prop, models: &Models, versions: &HashMap<String, u64>) -> usize {
    let levels = cache.borrow_mut().levels(prop, models, grass_version(prop, versions));
    let cells = prop.footprint[0].max(1) as usize * prop.footprint[1].max(1) as usize;
    match levels.vertices.first() {
        Some(&made) if made <= mesh::GROWN_HEAVIEST * cells => levels.thinnest(),
        Some(_) => usize::MAX,
        None => 0,
    }
}

/// What the grass grown through the ground is made from: each tall terrain's grass props, with
/// their records and their models' generations (a model arriving shows on them), and each
/// meadow's placing (how many, which plants, the clumps and patches, the models it grows; not
/// its plants' looks, which the renderer has as they change: `blade_kinds`).
fn growth_digest(content: &content::Content, versions: &HashMap<String, u64>) -> u64 {
    let mut h = Quick::default();
    for t in content.terrains.iter().filter(|t| t.pattern == "tall" && t.growth.grows_props()) {
        let g = content::Growth { meadow: Default::default(), ..t.growth.clone() };
        (&t.id, format!("{g:?}")).hash(&mut h);
        for g in &t.growth.props {
            content.props.get(&g.prop).map(|p| format!("{p:?}")).hash(&mut h);
            versions.get(&g.prop).hash(&mut h);
        }
    }
    for t in content.terrains.iter().filter(|t| world::meadow::grows_meadow(t)) {
        let m = &t.growth.meadow;
        (&t.id, m.density.to_bits(), m.patchy.to_bits(), m.clump.to_bits(), m.clumping.to_bits()).hash(&mut h);
        for p in &m.plants {
            (&p.shape, &p.prop, p.weight.to_bits(), p.scale.map(f64::to_bits), p.clusters.to_bits()).hash(&mut h);
            if p.shape == "model" {
                content.props.get(&p.prop).map(|q| format!("{q:?}")).hash(&mut h);
                versions.get(&p.prop).hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Every meadow's kinds of blade and flower as the renderer has them (`world::meadow::Kinds`,
/// in its order): their shapes, heights, widths, how they lean and curve, their
/// colours (their terrain's where they have none), whether they're toon, how much they glow,
/// how stiff they are (their own, else their terrain's rigidity).
pub(super) fn blade_kinds(content: &content::Content) -> Vec<BladeKind> {
    world::meadow::Kinds::of(content)
        .list
        .iter()
        .map(|(t, p)| {
            let [foot, tip, petal, heart] = world::meadow::colours(t, p);
            let flower = p.shape == "flower";
            let (lo, hi) = (p.height[0].min(p.height[1]) as f32, p.height[0].max(p.height[1]) as f32);
            BladeKind {
                form: [if flower { 1.0 } else { 0.0 }, lo, hi, p.width as f32],
                bend: [p.lean as f32, p.bend as f32, 1.0, if flower { p.bloom as f32 } else { 0.0 }],
                foot: [foot[0], foot[1], foot[2], p.petals as f32],
                tip: [tip[0], tip[1], tip[2], if t.growth.meadow.toon { 1.0 } else { 0.0 }],
                petal: [petal[0], petal[1], petal[2], p.rigidity.unwrap_or(t.rigidity).clamp(0.0, 1.0) as f32],
                heart: [heart[0], heart[1], heart[2], p.glow as f32],
            }
        })
        .collect()
}

/// What the ground's squares are built from in the terrains' records: how each is drawn and
/// walked (water lies flat, tall ground grows its tufts), its water.
fn terrains_digest(content: &content::Content) -> u64 {
    let mut h = Quick::default();
    for t in content.terrains.iter() {
        (&t.id, &t.pattern, &t.walk, &t.water).hash(&mut h);
    }
    h.finish()
}

/// The palette's digest (`mesh::palette`: every terrain's colours and look).
fn palette_digest(content: &content::Content) -> u64 {
    let mut h = Quick::default();
    mesh::palette(content).0.hash(&mut h);
    h.finish()
}

/// A building's digest: its record and the records of the props it's made of.
fn building_digest(b: &Building, records: &QuickMap<&str, u64>) -> u64 {
    let mut h = Quick::default();
    format!("{b:?}").hash(&mut h);
    let floors = b.floors.iter().flat_map(|f| std::iter::once(&f.tile).chain(f.openings.iter().map(|o| &o.prop)).chain(f.props.iter().map(|p| &p.prop)));
    let rest = b.pillars.iter().map(|p| &p.prop).chain(b.anchors.iter().map(|a| &a.prop)).chain(b.outside.iter().map(|p| &p.prop));
    for id in [&b.wall, &b.roof].into_iter().chain(floors).chain(rest) {
        records.get(id.as_str()).hash(&mut h);
    }
    h.finish()
}

/// Whether a ramp covers any of a region's cells, or comes within `pad` cells of them.
fn overlaps(r: &content::Ramp, region: [i32; 4], pad: i32) -> bool {
    r.at[0] - pad < region[2] && r.at[1] - pad < region[3] && r.at[0] + r.size[0] as i32 + pad > region[0] && r.at[1] + r.size[1] as i32 + pad > region[1]
}

/// The cells texture's blue channel: 255 on the cells in the focus (0 to take it off).
fn mark_focus(map: &WorldMap, cells: &mut [u8], focus: &[(i32, i32)], on: u8) {
    for &(x, z) in focus {
        if let Some(b) = mesh::cell_texel(map, x, z).and_then(|t| cells.get_mut(t * 4 + 2)) {
            *b = on;
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

    /// Looks over the cells for those whose terrain or level changed since: they're due to
    /// the texture (`texels_due`) and to the ground digests of the squares that read them
    /// (`ground_due`). Another map, or another size: every square.
    fn cells_changed(&mut self) {
        let (m, gpu) = (self.map, &mut *self.gpu);
        if gpu.seen_terrain.len() != m.terrain.len() || gpu.seen_level.len() != m.level.len() {
            gpu.seen_terrain = m.terrain.clone();
            gpu.seen_level = m.level.clone();
            gpu.ground_due.extend(squares_of(m));
            return;
        }
        let w = m.width;
        for z in 0..m.height {
            let row = z * w..(z + 1) * w;
            if same(&gpu.seen_terrain[row.clone()], &m.terrain[row.clone()]) && same(&gpu.seen_level[row.clone()], &m.level[row.clone()]) {
                continue;
            }
            for i in row.clone().filter(|&i| gpu.seen_terrain[i] != m.terrain[i] || gpu.seen_level[i] != m.level[i]) {
                let (x, z) = ((i % w) as i32, z as i32);
                gpu.texels_due.push((x, z));
                gpu.ramp_ground_changed |= m.ramps.iter().any(|r| r.has(x, z));
                // the squares whose digests read it: its own and those two cells round it
                for sz in square(x, z - 2).1..=square(x, z + 2).1 {
                    for sx in square(x - 2, z).0..=square(x + 2, z).0 {
                        gpu.ground_due.insert((sx, sz));
                    }
                }
            }
            gpu.seen_terrain[row.clone()].copy_from_slice(&m.terrain[row.clone()]);
            gpu.seen_level[row.clone()].copy_from_slice(&m.level[row]);
        }
    }

    /// The ground: each square's levels and terrains, and two cells around it (the cliff
    /// faces come down onto the ground across, which comes up to them), past the edge the
    /// next maps'. Worked out again for the squares whose cells changed (`cells_changed`),
    /// or at the edge when the maps around did.
    fn ground_digests(&mut self, edges: bool) -> BTreeMap<(i32, i32), u64> {
        let (m, gpu) = (self.map, &mut *self.gpu);
        let squares = squares_of(m);
        let n = CHUNK as i32;
        let (gx, gz) = ((m.width as i32 + n - 1) / n, (m.height as i32 + n - 1) / n);
        let mut due = std::mem::take(&mut gpu.ground_due);
        if edges {
            due.extend(squares.iter().filter(|s| s.0 == 0 || s.1 == 0 || s.0 == gx - 1 || s.1 == gz - 1));
        }
        // the terrains grass props or a meadow grow through (by their place in the map's list)
        let growing: Vec<u16> = (0..m.terrains.len() as u16).filter(|&k| self.content.terrains.get(&m.terrains[k as usize]).is_some_and(|t| (t.pattern == "tall" && t.growth.grows_props()) || world::meadow::grows_meadow(t))).collect();
        for s in due.into_iter().filter(|s| (0..gx).contains(&s.0) && (0..gz).contains(&s.1)) {
            let r = region(s);
            let mut h = Quick::default();
            let mut grows = false;
            for z in r[1] - 2..(r[3] + 2).min(m.height as i32 + 2) {
                for x in r[0] - 2..(r[2] + 2).min(m.width as i32 + 2) {
                    match m.index(x, z) {
                        Some(i) => {
                            (m.level[i], m.terrain[i]).hash(&mut h);
                            grows |= x >= r[0] && z >= r[1] && x < r[2] && z < r[3] && growing.contains(&m.terrain[i]);
                        }
                        None => (self.beyond.level(x, z), self.beyond.terrain(x, z)).hash(&mut h),
                    }
                }
            }
            // grass props or a meadow grow through its cells: built again when they change
            if grows {
                gpu.lit_growth.hash(&mut h);
            }
            // (and every square when a terrain is drawn or walked otherwise)
            gpu.lit_terrains.hash(&mut h);
            gpu.ground_cells.insert(s, h.finish());
        }
        // the ramps on it and round it (their walls come down onto the ground across), and
        // whether something lies along each (its ground straight under it)
        let ramps: Vec<(&content::Ramp, u64)> = m
            .ramps
            .iter()
            .map(|ramp| {
                let mut h = Quick::default();
                format!("{ramp:?}").hash(&mut h);
                m.ramp_cover(ramp, self.content).is_some().hash(&mut h);
                (ramp, h.finish())
            })
            .collect();
        squares
            .into_iter()
            .map(|s| {
                let mut h = Quick(gpu.ground_cells.get(&s).copied().unwrap_or(0));
                for (_, d) in ramps.iter().filter(|(q, _)| overlaps(q, region(s), 3)) {
                    d.hash(&mut h);
                }
                // the jumps with an end on it (their little ramps)
                let r = region(s);
                for j in m.jumps.iter().filter(|j| [j.from, j.to].iter().any(|c| c[0] >= r[0] && c[1] >= r[1] && c[0] < r[2] && c[1] < r[3])) {
                    (j.from, j.to, j.one_way).hash(&mut h);
                }
                (s, h.finish())
            })
            .collect()
    }

    /// The props and the actors standing on each square (on a ramp's slope: the ramps there
    /// too). A square's props are hashed again only when they changed: the list is looked
    /// over for what stayed the same at its start and at its end since, and the squares of
    /// the props between (as they were and as they are) are hashed again.
    fn prop_digests(&mut self, records: &QuickMap<&str, u64>) -> (BTreeMap<(i32, i32), u64>, bool) {
        let (m, gpu) = (self.map, &mut *self.gpu);
        let at = |p: &Placed| square(p.at[0].floor() as i32, p.at[1].floor() as i32);
        let (old, new) = (&gpu.props_seen, &m.props);
        let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
        let (gone, came) = (head..old.len() - tail, head..new.len() - tail);
        let due: BTreeSet<(i32, i32)> = old[gone.clone()].iter().chain(&new[came.clone()]).map(at).collect();
        let changed = !due.is_empty();
        if changed {
            gpu.props_seen.splice(gone, new[came].iter().cloned());
            let mut by: BTreeMap<(i32, i32), (Quick, Vec<String>)> = due.into_iter().map(|s| (s, (Quick::default(), vec![]))).collect();
            for p in &m.props {
                let Some((h, ids)) = by.get_mut(&at(p)) else { continue };
                p.prop.hash(h);
                (p.at[0].to_bits(), p.at[1].to_bits(), p.level, p.turn.to_bits(), p.scale.to_bits(), p.offset[0].to_bits(), p.offset[1].to_bits()).hash(h);
                if !ids.contains(&p.prop) {
                    ids.push(p.prop.clone());
                }
            }
            for (s, (h, ids)) in by {
                if ids.is_empty() {
                    gpu.prop_squares.remove(&s);
                } else {
                    gpu.prop_squares.insert(s, (h.finish(), ids));
                }
            }
        }
        // with their records
        let mut by: BTreeMap<(i32, i32), Quick> = gpu
            .prop_squares
            .iter()
            .map(|(s, (d, ids))| {
                let mut h = Quick(*d);
                for id in ids {
                    records.get(id.as_str()).hash(&mut h);
                }
                (*s, h)
            })
            .collect();
        if self.actors {
            for a in &self.map.actors {
                let h = by.entry(square(a.at[0], a.at[1])).or_default();
                (&a.look, a.at, a.level, a.facing.to_bits()).hash(h);
                a.look.strip_prefix("prop:").and_then(|id| records.get(id)).hash(h);
            }
        }
        // the flooring out of doors, on its cells' ground, with its records
        for t in &self.map.floors {
            let h = by.entry(square(t.cell[0], t.cell[1])).or_default();
            (t.cell, &t.tile, t.turn, t.flip, self.map.level_at(t.cell[0], t.cell[1]), self.map.ramp_at(t.cell[0], t.cell[1]).is_some()).hash(h);
            records.get(t.tile.as_str()).hash(h);
        }
        for (s, h) in by.iter_mut() {
            for ramp in self.map.ramps.iter().filter(|q| overlaps(q, region(*s), 0)) {
                format!("{ramp:?}").hash(h);
            }
        }
        (by.into_iter().map(|(s, h)| (s, h.finish())).collect(), changed)
    }

    /// The trees of each square (`groves`), with their props' records.
    fn tree_digests(&self, records: &QuickMap<&str, u64>) -> BTreeMap<(i32, i32), u64> {
        let digest = |g: &Grove| {
            let mut h = Quick(g.digest);
            for p in &g.props {
                records.get(p.as_str()).hash(&mut h);
            }
            h.finish()
        };
        self.gpu.groves.iter().map(|(s, g)| (*s, digest(g))).collect()
    }

    /// The buildings by the square of their corner, with how each is shown: cut above the
    /// walker's floor, left out while it's edited (the draft takes its place). A building's
    /// own digest is kept while it and the props' records stay the same.
    fn building_digests(&mut self, records: &QuickMap<&str, u64>) -> (BTreeMap<(i32, i32), u64>, bool) {
        let gpu = &mut *self.gpu;
        let mut all = Quick::default();
        if gpu.records.len() != records.len() || records.iter().any(|(id, d)| gpu.records.get(*id) != Some(d)) {
            gpu.records = records.iter().map(|(id, d)| (id.to_string(), *d)).collect();
            gpu.building_digests.clear();
        }
        let mut by: BTreeMap<(i32, i32), Quick> = BTreeMap::new();
        for (i, b) in self.map.buildings.iter().enumerate() {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            if Some(i) == self.editing {
                "editing".hash(h);
                continue;
            }
            let own = match gpu.building_digests.get(&b.id) {
                Some((kept, d)) if kept == b => *d,
                _ => {
                    let d = building_digest(b, records);
                    gpu.building_digests.insert(b.id.clone(), (b.clone(), d));
                    d
                }
            };
            own.hash(h);
            own.hash(&mut all);
            self.cuts.get(i).copied().flatten().map(|c| (c.top, c.low)).hash(h);
        }
        let changed = std::mem::replace(&mut gpu.lit_buildings, all.finish()) != all.finish();
        // (the buildings gone: forgotten)
        if gpu.building_digests.len() > 2 * self.map.buildings.len() + 16 {
            let ids: BTreeSet<&str> = self.map.buildings.iter().map(|b| b.id.as_str()).collect();
            gpu.building_digests.retain(|id, _| ids.contains(id.as_str()));
        }
        if let Some((b, floor)) = self.draft {
            let h = by.entry(square(b.at[0], b.at[1])).or_default();
            format!("draft {b:?} floor {floor}").hash(h);
        }
        (by.into_iter().map(|(s, h)| (s, h.finish())).collect(), changed)
    }

    /// Builds one square of one layer and sends it.
    fn build_square(&mut self, r: &mut SceneRenderer, device: &eframe::wgpu::Device, layer: &str, s: (i32, i32)) {
        let (reg, lh, id) = (region(s), self.level_height, self.id);
        match layer {
            "ground" => {
                let weigh = |p: &Prop| grass_weight(self.grass, p, self.models, self.model_versions);
                let mesh::Grown { tops, cliffs, grass, tufts, blades } = mesh::ground_region_all(self.map, self.content, lh, reg, self.beyond, &weigh);
                r.set_chunk(device, id, "tops", s, &[(WHITE.to_string(), gpu(&tops.vertices), tops.indices)]);
                r.set_chunk(device, id, CLIFFS, s, &[(WHITE.to_string(), gpu(&cliffs.vertices), cliffs.indices)]);
                // the engine's blades; the grass props grown through tall ground as copies of
                // their models, each model sent in levels when it's new or changed
                r.set_chunk(device, id, "grass", s, &models::batches(&Scene::from(grass)));
                let mut copies: BTreeMap<String, Vec<GrassCopy>> = BTreeMap::new();
                for t in &tufts {
                    copies.entry(t.prop.clone()).or_default().push(GrassCopy { at: t.at.to_array(), turn: -t.turn.to_radians(), scale: t.scale, far: if t.far { 1.0 } else { 0.0 }, ..Default::default() });
                }
                // the meadows' blades and flowers, for the GPU to make
                r.set_blades(id, s, blades.iter().map(|b| BladeInstance { at: b.at.to_array(), turn: b.turn, size: b.size, kind: b.kind, seed: b.seed, ..Default::default() }).collect());
                for key in copies.keys() {
                    let Some(prop) = self.content.props.get(key) else { continue };
                    let version = grass_version(prop, self.model_versions);
                    if r.grass_model_version(key) != Some(version) {
                        let levels = self.grass.borrow_mut().levels(prop, self.models, version);
                        let batches: Vec<_> = levels.scenes.iter().map(models::batches).collect();
                        let cells = prop.footprint[0].max(1) as f32 * prop.footprint[1].max(1) as f32;
                        r.set_grass_model(device, key, version, &GrassModel { levels: &batches, cells, lo: levels.lo, hi: levels.hi });
                    }
                }
                r.set_grass(id, s, copies.into_iter().collect());
            }
            "props" => {
                let (map, content, flags) = (self.map, self.content, self.flags);
                let mut scene = mesh::props_region_as(map, content, lh, self.models, reg, &|p| emptied_look(map, content, flags, p));
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
                let scene = self.gpu.groves.get(&s).map(|g| mesh::trees_from(&g.trees, self.content, self.models, reg)).unwrap_or_default();
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
/// rest. Whether what gives off light and sound changed (the props, their records, the
/// buildings, the ramps and the ground on them).
pub(super) fn stream(r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState, f: &mut Feed, mut need: Needs, start: f64) -> bool {
    let (w, h) = (f.map.width, f.map.height);
    // the ground's texture and palette: a new map, a resized one, one terrain more; the
    // cells again when painted, or when the maps around it changed (the ring)
    let edges = f.gpu.around != f.around_version;
    // what each step of a change costs (the `streamcost` hook: how big maps hold up)
    let mut tick = crate::platform::now_ms();
    let key = (f.id.to_string(), w, h, f.map.terrains.clone(), palette_digest(f.content));
    let focus = f.focus.unwrap_or(&[]).to_vec();
    let whole = f.gpu.ground.as_ref() != Some(&key);
    if f.gpu.ground.as_ref().map(|g| (g.0.as_str(), g.1, g.2) != (f.id, w, h)).unwrap_or(true) {
        // another map, or another size: nothing held is right any more
        *f.gpu = GpuState { around: f.gpu.around, ..Default::default() };
        need = Needs::all();
    }
    // the meadows' kinds as they look now (the renderer has them as they change)
    r.set_blade_kinds(&blade_kinds(f.content));
    // the grass props grown through tall ground and the meadows: changed (their settings, a
    // record, a model come, what grows where), every square's ground looked at again (those
    // they grow on are built again)
    let growth = growth_digest(f.content, f.model_versions);
    let grew = std::mem::replace(&mut f.gpu.lit_growth, growth) != growth;
    // a terrain drawn or walked otherwise (the Terrain window): every square's ground again
    let terrains = terrains_digest(f.content);
    let redrawn = std::mem::replace(&mut f.gpu.lit_terrains, terrains) != terrains;
    if grew || redrawn {
        f.gpu.ground_due.extend(squares_of(f.map));
    }
    // the ground's squares again (props too can change the ground: what lies along a ramp
    // makes it straight); what changed in the cells looked for once, for them and the texture
    let ground = need.ground || edges || (need.props && !f.map.ramps.is_empty()) || grew || redrawn;
    if whole || need.cells || edges || ground {
        f.cells_changed();
        tick = cost("cellschanged", tick);
    }
    if whole {
        let (palette, palette_width) = mesh::palette(f.content);
        let mut cells = mesh::cells_texture(f.map, f.content, f.beyond);
        mark_focus(f.map, &mut cells, &focus, 255);
        let size = [w as u32 + 2, h as u32 + 2];
        r.set_world_ground(&rs.device, &rs.queue, f.id, &WorldGround { cells: &cells, size, palette: &palette, palette_width, level_height: f.level_height });
        (f.gpu.texture, f.gpu.focus) = (cells, focus);
        f.gpu.texels_due.clear();
        f.gpu.ground = Some(key);
        tick = cost("texture", tick);
    } else if need.cells || edges || !f.gpu.texels_due.is_empty() {
        // the texels of the cells that changed (and on the map's edge, of the ring round it
        // showing them again), of the ring when the maps around changed, and where the focus
        // was and is; a level painted with the terrain as it was too (left out, the shader
        // read the cells' old levels: a cliff's foot shaded where ground raised to meet it
        // now runs on, a seam)
        let (wi, hi) = (w as i32, h as i32);
        let mut texels: Vec<(i32, i32)> = vec![];
        for (x, z) in std::mem::take(&mut f.gpu.texels_due) {
            if x == 0 || z == 0 || x == wi - 1 || z == hi - 1 {
                texels.extend((-1..=1).flat_map(|dz| (-1..=1).map(move |dx| (x + dx, z + dz))));
            } else {
                texels.push((x, z));
            }
        }
        if edges {
            texels.extend((-1..=wi).flat_map(|x| [(x, -1), (x, hi)]).chain((0..hi).flat_map(|z| [(-1, z), (wi, z)])));
        }
        mesh::cells_texels(f.map, f.content, f.beyond, &mut f.gpu.texture, texels.iter().copied());
        if focus != f.gpu.focus {
            mark_focus(f.map, &mut f.gpu.texture, &f.gpu.focus, 0);
            mark_focus(f.map, &mut f.gpu.texture, &focus, 255);
            texels.extend(f.gpu.focus.iter().chain(&focus));
            f.gpu.focus = focus;
        }
        tick = cost("cells", tick);
        // sent: the box round them, in the texture's texels (the ring's first)
        if let Some([x0, z0, x1, z1]) = texels.iter().fold(None, |b: Option<[i32; 4]>, &(x, z)| {
            let (x, z) = ((x + 1).clamp(0, wi + 1), (z + 1).clamp(0, hi + 1));
            Some(b.map_or([x, z, x + 1, z + 1], |b| [b[0].min(x), b[1].min(z), b[2].max(x + 1), b[3].max(z + 1)]))
        }) {
            r.set_world_cells(&rs.queue, f.id, &f.gpu.texture, [x0 as u32, z0 as u32, x1 as u32, z1 as u32]);
        }
        tick = cost("upload", tick);
    }
    f.gpu.around = f.around_version;
    // what changed, square by square
    let mut lit = false;
    if ground {
        let d = f.ground_digests(edges);
        f.refresh("ground", d);
        if f.gpu.lit_ramps != f.map.ramps || std::mem::take(&mut f.gpu.ramp_ground_changed) {
            f.gpu.lit_ramps = f.map.ramps.clone();
            lit = true;
        }
        tick = cost("ground", tick);
    }
    if need.props || need.trees || need.buildings {
        let records = record_digests(f.content, f.model_versions);
        let all = records.iter().fold(0, |a, e| {
            let mut h = Quick::default();
            e.hash(&mut h);
            a ^ h.finish()
        });
        lit |= std::mem::replace(&mut f.gpu.lit_records, all) != all;
        if need.trees {
            // the trees placed again where they can have moved, and those squares' again
            match f.gpu.woods.update(f.map, f.content, f.level_height) {
                None => {
                    let mut by: BTreeMap<(i32, i32), Vec<Tree>> = BTreeMap::new();
                    for t in f.gpu.woods.trees() {
                        by.entry(square(t.pos[0].floor() as i32, t.pos[2].floor() as i32)).or_default().push(t);
                    }
                    f.gpu.groves = by.into_iter().map(|(s, trees)| (s, Grove::new(trees))).collect();
                }
                Some(cells) => {
                    for s in cells.into_iter().map(|(x, z)| square(x, z)).collect::<BTreeSet<_>>() {
                        let trees = f.gpu.woods.trees_in(region(s));
                        if trees.is_empty() {
                            f.gpu.groves.remove(&s);
                        } else {
                            f.gpu.groves.insert(s, Grove::new(trees));
                        }
                    }
                }
            }
            tick = cost("trees", tick);
            let d = f.tree_digests(&records);
            f.refresh("trees", d);
            tick = cost("treedigests", tick);
        }
        if need.props {
            let (d, changed) = f.prop_digests(&records);
            f.refresh("props", d);
            lit |= changed;
            tick = cost("props", tick);
        }
        if need.buildings {
            let (d, changed) = f.building_digests(&records);
            f.refresh("buildings", d);
            lit |= changed;
            cost("buildings", tick);
        }
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
        r.set_grass(f.id, s, vec![]);
        r.set_blades(f.id, s, vec![]);
        for layer in LAYERS {
            if let Some(b) = f.gpu.built.get_mut(layer) {
                b.remove(&s);
            }
        }
        f.gpu.props_vertices.remove(&s);
    }
    // the squares in reach, nearest first and in full, as many as the frame's budget allows
    let over = || crate::platform::now_ms() - start > BUDGET_MS;
    let began = crate::platform::now_ms();
    let mut built = false;
    'build: for &s in &wanted {
        for layer in LAYERS {
            if f.built(layer, s) {
                continue;
            }
            f.build_square(r, &rs.device, layer, s);
            f.gpu.built.entry(layer).or_default().insert(s);
            built = true;
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
            built = true;
            if over() {
                break;
            }
        }
    }
    if built {
        cost("squares", began);
    }
    lit
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
            grass: &self.grass_levels,
            level_height: self.level_height,
            flags: &self.story.flags,
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
        // the props' and buildings' lights and sounds again, when they (or the ground under
        // them on a ramp) changed
        if stream(r, rs, &mut feed, need, start) {
            self.placed_lights = None;
            self.placed_sounds = None;
            self.along_hook = None;
        }
        if self.placed_sounds.is_none() {
            let t = crate::platform::now_ms();
            self.placed_sounds = Some(world::sounds::sounds(&self.map, &self.content, self.level_height));
            cost("sounds", t);
        }
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
