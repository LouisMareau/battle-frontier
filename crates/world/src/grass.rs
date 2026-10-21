//! Grass props drawn as many copies of one model (the renderer's grass): the model in levels
//! for the copies further away (`thin`), each about half as many of its pieces as the one
//! before (a piece: what's joined together, a blade, a leaf, a stalk with its seeds), those
//! it keeps a little wider so the grass still looks as full; a piece as wide as the cells it
//! covers (a mat of ground cover) is kept on every level. The pieces kept are picked by where
//! they stand, the same every time, and each level's are some of the one before's, so a copy
//! going a level further loses blades rather than changing them.

use std::collections::HashMap;
use std::sync::Arc;

use content::Prop;
use glam::Vec3;

use crate::mesh::{prop_scene, Mesh, Models, Scene};

/// The most levels a model is thinned into (its last about 1 / 2^11 of its pieces).
pub const LEVELS: usize = 12;

/// How much wider a piece kept at the last levels is, at most (each level a little more:
/// as many times as half its pieces are gone, square-rooted).
pub const WIDEST: f32 = 3.0;

/// A grass model in levels (`thin`).
#[derive(Clone, Debug, Default)]
pub struct Levels {
    /// the first the model as made, each next with about half the pieces of the one before
    pub scenes: Vec<Scene>,
    /// each level's vertices
    pub vertices: Vec<usize>,
    /// the box round all its levels (its foot on y 0, its pieces widened)
    pub lo: Vec3,
    pub hi: Vec3,
}

impl Levels {
    /// The vertices of its thinnest level.
    pub fn thinnest(&self) -> usize {
        self.vertices.last().copied().unwrap_or(0)
    }
}

/// Grass props' models in levels (`thin`), each kept while its prop and its model stay as
/// they were (the `version` it was made from).
#[derive(Default)]
pub struct Cache {
    held: HashMap<String, (u64, Arc<Levels>)>,
}

impl Cache {
    /// A grass prop's model in levels: its real one's once loaded, else its stand-in's.
    pub fn levels(&mut self, prop: &Prop, models: &Models, version: u64) -> Arc<Levels> {
        if let Some((_, l)) = self.held.get(&prop.id).filter(|(v, _)| *v == version) {
            return l.clone();
        }
        let l = Arc::new(thin(&prop_scene(prop, models), prop.footprint));
        self.held.insert(prop.id.clone(), (version, l.clone()));
        l
    }
}

/// One piece of a mesh: its triangles (their first indices), where it stands (the middle of
/// its foot: its points within a millimetre of its lowest), its draw (0 to 1, by where it
/// stands: the same every time) and whether it's a mat (as wide as its cells: on every level).
struct Piece {
    tris: Vec<u32>,
    root: Vec3,
    draw: f32,
    mat: bool,
}

/// A mesh's pieces: its triangles joined through the points they share (its vertices welded
/// where they stand: a seam or a flat face splits one point in two).
fn pieces(m: &Mesh, cells: [f32; 2]) -> Vec<Piece> {
    let mut at: HashMap<[i32; 3], u32> = HashMap::new();
    let weld: Vec<u32> = m
        .vertices
        .iter()
        .map(|v| {
            let next = at.len() as u32;
            *at.entry(v.pos.map(|c| (c * 1e4).round() as i32)).or_insert(next)
        })
        .collect();
    let mut parent: Vec<u32> = (0..at.len() as u32).collect();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    for t in m.indices.as_chunks::<3>().0 {
        let a = root(&mut parent, weld[t[0] as usize]);
        for &k in &t[1..] {
            let b = root(&mut parent, weld[k as usize]);
            if b != a {
                parent[b as usize] = a;
            }
        }
    }
    let mut by: HashMap<u32, usize> = HashMap::new();
    let mut out: Vec<(Vec<u32>, Vec3, Vec3)> = vec![];
    for (n, t) in m.indices.as_chunks::<3>().0.iter().enumerate() {
        let r = root(&mut parent, weld[t[0] as usize]);
        let k = *by.entry(r).or_insert_with(|| {
            out.push((vec![], Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
            out.len() - 1
        });
        let piece = &mut out[k];
        piece.0.push(n as u32 * 3);
        for &i in t {
            let p = Vec3::from(m.vertices[i as usize].pos);
            piece.1 = piece.1.min(p);
            piece.2 = piece.2.max(p);
        }
    }
    out.into_iter()
        .map(|(tris, lo, hi)| {
            // its foot: the middle of its points near its lowest (each once)
            let mut near: Vec<u32> = tris.iter().flat_map(|&f| m.indices[f as usize..f as usize + 3].iter().copied()).filter(|&i| m.vertices[i as usize].pos[1] <= lo.y + 1e-3).collect();
            near.sort_unstable();
            near.dedup();
            let (sum, n) = near.iter().fold((Vec3::ZERO, 0.0), |(s, n), &i| (s + Vec3::from(m.vertices[i as usize].pos), n + 1.0));
            let foot = Vec3::new(sum.x / n, lo.y, sum.z / n);
            Piece { tris, root: foot, draw: draw((lo + hi) * 0.5), mat: hi.x - lo.x >= 0.5 * cells[0] && hi.z - lo.z >= 0.5 * cells[1] }
        })
        .collect()
}

/// A piece's draw, 0 to 1, by where its middle is (to a tenth of a millimetre).
fn draw(p: Vec3) -> f32 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    for c in p.to_array() {
        h ^= ((c * 1e4).round() as i64) as u64;
        h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h ^= h >> 31;
    }
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// The pieces of `m` kept (`keep` says which), as a mesh of their own, each widened `wide`
/// times across (x and z) about where it stands, mats as they are.
fn kept(m: &Mesh, cut: &[Piece], keep: &dyn Fn(&Piece) -> bool, wide: f32) -> Mesh {
    let mut out = Mesh::default();
    let mut slot: HashMap<u32, u32> = HashMap::new();
    for p in cut.iter().filter(|p| keep(p)) {
        slot.clear();
        let s = if p.mat { 1.0 } else { wide };
        for &first in &p.tris {
            for &i in &m.indices[first as usize..first as usize + 3] {
                let n = *slot.entry(i).or_insert_with(|| {
                    let mut v = m.vertices[i as usize];
                    if s != 1.0 {
                        let q = Vec3::from(v.pos);
                        let w = Vec3::new(p.root.x + (q.x - p.root.x) * s, q.y, p.root.z + (q.z - p.root.z) * s);
                        let n = Vec3::new(v.normal[0] / s, v.normal[1], v.normal[2] / s).normalize_or(Vec3::Y);
                        (v.pos, v.normal) = (w.to_array(), n.to_array());
                    }
                    out.vertices.push(v);
                    out.vertices.len() as u32 - 1
                });
                out.indices.push(n);
            }
        }
    }
    out
}

/// A grass prop's model (its `prop_scene`, covering `footprint` cells) in levels for copies
/// further away: the first as made, each next one keeping about half the pieces of the one
/// before (mats on all of them), widened a little more; as many as thin it (up to LEVELS),
/// none after one piece is left. A model of one piece has one level.
pub fn thin(scene: &Scene, footprint: [u8; 2]) -> Levels {
    let cells = [footprint[0].max(1) as f32, footprint[1].max(1) as f32];
    let meshes: Vec<(Option<&String>, &Mesh)> = std::iter::once((None, &scene.plain)).chain(scene.textured.iter().map(|(k, m)| (Some(k), m))).collect();
    let cut: Vec<Vec<Piece>> = meshes.iter().map(|(_, m)| pieces(m, cells)).collect();
    if cut.iter().all(|c| c.is_empty()) {
        return Levels::default();
    }
    let mut out = Levels { lo: Vec3::splat(f32::MAX), hi: Vec3::splat(f32::MIN), ..Default::default() };
    let mut before = usize::MAX;
    for k in 0..LEVELS {
        let keep = 0.5f32.powi(k as i32);
        let free = |p: &Piece| !p.mat && p.draw < keep;
        let count: usize = cut.iter().flatten().filter(|p| free(p)).count();
        if k > 0 && (count == 0 || count >= before) {
            if count == 0 {
                break;
            }
            continue;
        }
        before = count;
        let wide = 2f32.sqrt().powi(k as i32).min(WIDEST);
        let mut level = Scene::default();
        for ((key, m), cut) in meshes.iter().zip(&cut) {
            let mesh = kept(m, cut, &|p| p.mat || free(p), wide);
            if mesh.indices.is_empty() {
                continue;
            }
            match key {
                None => level.plain = mesh,
                Some(k) => {
                    level.textured.insert((*k).clone(), mesh);
                }
            }
        }
        let vertices = level.plain.vertices.len() + level.textured.values().map(|m| m.vertices.len()).sum::<usize>();
        for v in level.plain.vertices.iter().chain(level.textured.values().flat_map(|m| m.vertices.iter())) {
            out.lo = out.lo.min(Vec3::from(v.pos));
            out.hi = out.hi.max(Vec3::from(v.pos));
        }
        out.scenes.push(level);
        out.vertices.push(vertices);
        if count <= 1 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Vertex;

    /// A blade: a quad 2 cm wide standing at x, z, `tall` high, painted for the wind.
    fn blade(m: &mut Mesh, x: f32, z: f32, tall: f32) {
        let base = m.vertices.len() as u32;
        for (dx, y) in [(-0.01, 0.0), (0.01, 0.0), (0.01, tall), (-0.01, tall)] {
            m.vertices.push(Vertex { pos: [x + dx, y, z], normal: [0.0, 0.0, 1.0], uv: [0.5, 0.5], color: [0.2, 0.6, 0.2, 1.0], toon: 1.0, wind: [(y / tall * 255.0) as u8, 0, 9, 0] });
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// 400 blades over a cell (a 20 x 20 grid), and under them a mat as wide as it.
    fn tuft() -> Scene {
        let mut m = Mesh::default();
        for i in 0..20 {
            for j in 0..20 {
                blade(&mut m, -0.475 + i as f32 * 0.05, -0.475 + j as f32 * 0.05, 0.5 + (i * j % 7) as f32 * 0.03);
            }
        }
        let base = m.vertices.len() as u32;
        for (x, z) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            m.vertices.push(Vertex { pos: [x, 0.001, z], normal: [0.0, 1.0, 0.0], color: [0.1, 0.3, 0.1, 1.0], ..Default::default() });
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        Scene { plain: m, textured: Default::default() }
    }

    /// The blades' feet in a level (x, z, to a tenth of a millimetre), the mat left out.
    fn feet(s: &Scene) -> Vec<(i32, i32)> {
        let mut out: Vec<(i32, i32)> = s.plain.vertices.chunks(4).filter(|q| q[0].pos[1] == 0.0).map(|q| (((q[0].pos[0] + q[1].pos[0]) * 5e3).round() as i32, (q[0].pos[2] * 1e4).round() as i32)).collect();
        out.sort();
        out
    }

    #[test]
    fn each_level_keeps_about_half_the_blades_of_the_one_before_and_the_mat() {
        let levels = thin(&tuft(), [1, 1]);
        let n = levels.scenes.len();
        assert!((8..=LEVELS).contains(&n), "{n} levels for 400 blades");
        assert_eq!(levels.vertices[0], 401 * 4, "the first: the model as made");
        let blades: Vec<usize> = levels.scenes.iter().map(|s| feet(s).len()).collect();
        for k in 1..n {
            assert!(blades[k] < blades[k - 1], "fewer each level: {blades:?}");
            assert!(levels.vertices[k] < levels.vertices[k - 1]);
        }
        // about half, the first few levels (by chance, a little more or less)
        for k in 1..4 {
            let share = blades[k] as f32 / blades[k - 1] as f32;
            assert!((0.35..0.65).contains(&share), "level {k}: {blades:?}");
        }
        assert!(levels.thinnest() >= 4 && *blades.last().unwrap() <= 2, "the last a blade or two: {blades:?}");
        // the mat on every one (four vertices up at 1 mm)
        for s in &levels.scenes {
            assert_eq!(s.plain.vertices.iter().filter(|v| (v.pos[1] - 0.001).abs() < 1e-6).count(), 4);
        }
        // each level's blades some of the one before's
        for k in 1..n {
            let (before, now) = (feet(&levels.scenes[k - 1]), feet(&levels.scenes[k]));
            assert!(now.iter().all(|f| before.contains(f)), "level {k}'s blades were all at level {}", k - 1);
        }
        // the same every time
        let again = thin(&tuft(), [1, 1]);
        assert_eq!(again.vertices, levels.vertices);
        assert_eq!(feet(&again.scenes[3]), feet(&levels.scenes[3]));
    }

    #[test]
    fn the_blades_kept_widen_about_their_feet_their_paint_and_heights_kept() {
        let levels = thin(&tuft(), [1, 1]);
        let width = |s: &Scene| {
            let q = &s.plain.vertices[..4];
            q[1].pos[0] - q[0].pos[0]
        };
        assert!((width(&levels.scenes[0]) - 0.02).abs() < 1e-6);
        assert!((width(&levels.scenes[2]) - 0.04).abs() < 1e-5, "two levels on: twice as wide");
        let last = levels.scenes.last().unwrap();
        assert!((width(last) - 0.02 * WIDEST).abs() < 1e-5, "never wider than WIDEST times");
        // standing where they stood, as tall, painted as they were
        let (a, b) = (&levels.scenes[0].plain.vertices, &levels.scenes[2].plain.vertices);
        let foot = |q: &[Vertex]| (q[0].pos[0] + q[1].pos[0]) * 0.5;
        let same = a.chunks(4).find(|q| (foot(q) - foot(&b[..4])).abs() < 1e-6 && q[0].pos[2] == b[0].pos[2]).expect("the blade at level 0");
        assert_eq!((same[2].pos[1], same[2].wind, same[2].toon), (b[2].pos[1], b[2].wind, b[2].toon));
        // the box round them all
        assert!(levels.lo.x <= -0.5 && levels.hi.x >= 0.5 && levels.hi.y >= 0.5);
    }

    #[test]
    fn a_model_of_one_piece_is_one_level_and_a_seam_is_no_gap() {
        // a mat alone: nothing to thin
        let mut m = Mesh::default();
        blade(&mut m, 0.0, 0.0, 0.5);
        let one = thin(&Scene { plain: m.clone(), textured: Default::default() }, [1, 1]);
        assert_eq!((one.scenes.len(), one.vertices.clone()), (1, vec![4]));
        // a blade whose two halves don't share vertices (a seam) but meet where they stand:
        // one piece still
        let base = m.vertices.len() as u32;
        for (x, y) in [(0.3, 0.0), (0.32, 0.0), (0.32, 0.25), (0.3, 0.25), (0.3, 0.25), (0.32, 0.25), (0.32, 0.5), (0.3, 0.5)] {
            m.vertices.push(Vertex { pos: [x, y, 0.2], normal: [0.0, 0.0, 1.0], ..Default::default() });
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3, base + 4, base + 5, base + 6, base + 4, base + 6, base + 7]);
        assert_eq!(pieces(&m, [1.0, 1.0]).len(), 2, "the first blade and the seamed one");
        // nothing at all: no levels
        assert!(thin(&Scene::default(), [1, 1]).scenes.is_empty());
    }
}
