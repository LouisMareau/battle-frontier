//! Grass props drawn as many copies of one model (the renderer's grass): the model in levels
//! for the copies further away (`thin`), each about half as many of its pieces as the one
//! before (a piece: what's joined together, a blade, a leaf, a stalk with its seeds), those
//! it keeps a little wider so the grass still looks as full: a blade across, about the line
//! down its middle, never longer, taller or leaning further (the grass never swells as the eye
//! draws away). A piece as wide as the cells it covers (a mat of ground cover) is kept on every
//! level, and one resting on another (a seed head on its stalk) with it. The pieces kept are
//! picked by where they stand, the same every time, and each level's are some of the one
//! before's, so a copy going a level further loses blades rather than changing them.

use std::collections::{HashMap, HashSet};
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
    /// the box round all its levels (its foot on y 0, its blades widened)
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
/// stands: the same every time), whether it's a mat (as wide as its cells: on every level),
/// each of its vertices' way across it from the line down its middle (`across`: a blade's
/// only, widened along it), and how many times wider it may be made at most (never wider
/// than it's long).
struct Piece {
    tris: Vec<u32>,
    root: Vec3,
    draw: f32,
    mat: bool,
    across: HashMap<u32, Vec3>,
    most: f32,
}

/// A mesh's pieces: its triangles joined through the points they share (its vertices welded
/// where they stand: a seam or a flat face splits one point in two).
fn pieces(m: &Mesh, cells: [f32; 2]) -> Vec<Piece> {
    let mut at: HashMap<[i32; 3], u32> = HashMap::new();
    let mut points: Vec<Vec3> = vec![];
    let weld: Vec<u32> = m
        .vertices
        .iter()
        .map(|v| {
            let next = at.len() as u32;
            *at.entry(v.pos.map(|c| (c * 1e4).round() as i32)).or_insert_with(|| {
                points.push(Vec3::from(v.pos));
                next
            })
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
            let mat = hi.x - lo.x >= 0.5 * cells[0] && hi.z - lo.z >= 0.5 * cells[1];
            let (across, most) = if mat { (HashMap::new(), 1.0) } else { across(m, &tris, &weld, &points, foot, lo.y) };
            Piece { tris, root: foot, draw: draw((lo + hi) * 0.5), mat, across, most }
        })
        .collect()
}

/// The way across a piece from the line down its middle (its spine: half way between its two
/// sides, from the middle of its foot to the middle of its tip) of each of its vertices, by
/// their indices: a blade's or a leaf's only. Its rim (its edges of one triangle only) goes
/// all round it, its foot along one stretch of it (its lowest points, or for a leaf on a stalk,
/// the rim's points nearest the stalk's foot: the stalk's own rim at its foot left out) and its
/// tip (its point furthest from its foot, and those round it nearly as far) along another,
/// never wider than it's long; what lies past its foot or its tip (the stalk under a leaf) is
/// never widened. With how many times wider it may be made (no wider than it's long: a leaf as
/// round as it's long hardly). Anything else has none: a lump (closed all round, a seed head),
/// a stalk (open at both ends), two blades joined (wider apart than they're long), whose
/// widening would only swell them.
fn across(m: &Mesh, tris: &[u32], weld: &[u32], points: &[Vec3], root: Vec3, low: f32) -> (HashMap<u32, Vec3>, f32) {
    // its rims, each point's two neighbours along its own
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for &f in tris {
        let t = [0, 1, 2].map(|k| weld[m.indices[f as usize + k] as usize]);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a != b {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    if edges.values().any(|&n| n > 2) {
        return (HashMap::new(), 1.0);
    }
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&(a, b), _) in edges.iter().filter(|(_, n)| **n == 1) {
        next.entry(a).or_default().push(b);
        next.entry(b).or_default().push(a);
    }
    if next.is_empty() || next.values().any(|n| n.len() != 2) {
        return (HashMap::new(), 1.0);
    }
    for n in next.values_mut() {
        n.sort_unstable();
    }
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort_unstable();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut rims: Vec<Vec<u32>> = vec![];
    for start in starts {
        if seen.contains(&start) {
            continue;
        }
        let (mut rim, mut before, mut now) = (vec![start], start, next[&start][0]);
        while now != start && rim.len() <= next.len() {
            rim.push(now);
            let n = &next[&now];
            (before, now) = (now, if n[0] == before { n[1] } else { n[0] });
        }
        seen.extend(rim.iter().copied());
        rims.push(rim);
    }
    // the rim round it (any other small: a stalk's at its foot)
    rims.sort_by_key(|r| std::cmp::Reverse(r.len()));
    let rim = &rims[0];
    if rim.len() < 3 || rim.len() * 3 < next.len() * 2 {
        return (HashMap::new(), 1.0);
    }
    let n = rim.len();
    let at = |i: usize| points[rim[i % n] as usize];
    // its foot along one stretch of the rim, its tip along another
    let lowest = (0..n).any(|i| at(i).y <= low + 1e-3);
    let nearest = (0..n).map(|i| at(i).distance(root)).fold(f32::MAX, f32::min);
    let foot: Vec<bool> = (0..n).map(|i| if lowest { at(i).y <= low + 1e-3 } else { at(i).distance(root) <= nearest + 1e-3 }).collect();
    let (sum, count) = (0..n).filter(|&i| foot[i]).fold((Vec3::ZERO, 0.0), |(s, c), i| (s + at(i), c + 1.0));
    let base = sum / count;
    let far: Vec<f32> = (0..n).map(|i| at(i).distance(base)).collect();
    let reach = far.iter().copied().fold(0.0, f32::max);
    let top = (0..n).fold(0, |b, i| if far[i] > far[b] { i } else { b });
    let mut tip = vec![false; n];
    tip[top] = true;
    for step in [1, n - 1] {
        let mut i = (top + step) % n;
        while !tip[i] && far[i] >= reach * 0.97 {
            tip[i] = true;
            i = (i + step) % n;
        }
    }
    let stretch = |on: &[bool]| {
        let firsts: Vec<usize> = (0..n).filter(|&i| on[i] && !on[(i + n - 1) % n]).collect();
        let &[first] = firsts.as_slice() else { return None };
        let mut last = first;
        while on[(last + 1) % n] {
            last = (last + 1) % n;
        }
        Some((first, last))
    };
    let (Some((f0, f1)), Some((t0, t1))) = (stretch(&foot), stretch(&tip)) else { return (HashMap::new(), 1.0) };
    if reach < 1e-4 || (0..n).any(|i| foot[i] && tip[i]) {
        return (HashMap::new(), 1.0);
    }
    // its two sides, foot to tip, and how far along each each of their points is
    let side = |from: usize, to: usize, step: usize| {
        let (mut out, mut i) = (vec![at(from)], from);
        while i != to {
            i = (i + step) % n;
            out.push(at(i));
        }
        let mut gone = vec![0.0f32];
        for w in out.windows(2) {
            gone.push(gone.last().unwrap() + w[0].distance(w[1]));
        }
        (out, gone)
    };
    let (a, b) = (side(f1, t0, 1), side(f0, t1, n - 1));
    // (a side has two points at least: its foot's end and its tip's, unless its foot is one
    // point, then the same both sides)
    let along = |(s, gone): &(Vec<Vec3>, Vec<f32>), share: f32| {
        if s.len() < 2 {
            return s[0];
        }
        let want = share * gone.last().unwrap();
        let k = gone.partition_point(|&g| g < want).clamp(1, s.len() - 1);
        let (g0, g1) = (gone[k - 1], gone[k]);
        s[k - 1].lerp(s[k], if g1 > g0 { ((want - g0) / (g1 - g0)).clamp(0.0, 1.0) } else { 0.0 })
    };
    // (the spine's points as far along as each side's own are: through every corner of both)
    let mut shares: Vec<f32> = [&a, &b].iter().flat_map(|(_, gone)| gone.iter().map(|g| g / gone.last().unwrap().max(1e-9))).collect();
    shares.sort_by(f32::total_cmp);
    shares.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
    let ends: Vec<(Vec3, Vec3)> = shares.iter().map(|&s| (along(&a, s), along(&b, s))).collect();
    let widths: Vec<f32> = ends.iter().map(|(p, q)| p.distance(*q)).collect();
    let widest = widths.iter().copied().fold(0.0, f32::max);
    if ends.len() < 2 || widest > reach || widths.iter().sum::<f32>() / widths.len() as f32 > 0.6 * reach {
        return (HashMap::new(), 1.0);
    }
    let spine: Vec<Vec3> = ends.iter().map(|(p, q)| (*p + *q) * 0.5).collect();
    // how far its foot and its tip spread from the spine's ends: what's further past an end
    // (the stalk under a leaf) isn't the blade's
    let spread = |on: &[bool], end: Vec3| (0..n).filter(|&i| on[i]).map(|i| at(i).distance(end)).fold(0.0, f32::max) * 1.5 + 1e-4;
    let (first, last) = (spread(&foot, spine[0]), spread(&tip, spine[spine.len() - 1]));
    // each vertex's way from the nearest point of the spine, across it
    let mut out = HashMap::new();
    for &f in tris {
        for &i in &m.indices[f as usize..f as usize + 3] {
            out.entry(i).or_insert_with(|| {
                let q = Vec3::from(m.vertices[i as usize].pos);
                let (mut best, mut near, mut way, mut end) = (f32::MAX, spine[0], Vec3::ZERO, None);
                for (k, w) in spine.windows(2).enumerate() {
                    let d = w[1] - w[0];
                    let t = if d.length_squared() > 1e-12 { (q - w[0]).dot(d) / d.length_squared() } else { 0.0 };
                    let c = w[0] + d * t.clamp(0.0, 1.0);
                    if c.distance_squared(q) < best {
                        let past = if k == 0 && t <= 0.0 {
                            Some(first)
                        } else if k + 2 == spine.len() && t >= 1.0 {
                            Some(last)
                        } else {
                            None
                        };
                        (best, near, way, end) = (c.distance_squared(q), c, d.normalize_or_zero(), past);
                    }
                }
                let off = q - near;
                if end.is_some_and(|e| off.length() > e) {
                    return Vec3::ZERO;
                }
                off - way * off.dot(way)
            });
        }
    }
    (out, (reach / widest.max(1e-6)).max(1.0))
}

/// The pieces that don't stand on the model's ground (a seed head on its stalk, a flower on
/// its stem: their foot over 2 cm above its lowest) take the draw of the one they rest on (one
/// of whose points is within a centimetre of their foot, and whose own foot is lower), so
/// they're kept or left with it (lowest first: one resting on one that rests on another, with
/// that too).
fn rest(meshes: &[&Mesh], cut: &mut [Vec<Piece>]) {
    let ground = cut.iter().flatten().map(|p| p.root.y).fold(f32::MAX, f32::min);
    let mut floating: Vec<(usize, usize)> = cut.iter().enumerate().flat_map(|(k, c)| c.iter().enumerate().filter(|(_, p)| p.root.y > ground + 0.02).map(move |(i, _)| (k, i))).collect();
    if floating.is_empty() {
        return;
    }
    floating.sort_by(|a, b| cut[a.0][a.1].root.y.total_cmp(&cut[b.0][b.1].root.y));
    // every piece's points, by the centimetre they're in
    let cell = |p: Vec3| (p * 100.0).floor().as_ivec3().to_array();
    let mut grid: HashMap<[i32; 3], Vec<(Vec3, usize, usize)>> = HashMap::new();
    for (k, (m, c)) in meshes.iter().zip(cut.iter()).enumerate() {
        for (i, p) in c.iter().enumerate() {
            for &f in &p.tris {
                for &v in &m.indices[f as usize..f as usize + 3] {
                    let q = Vec3::from(m.vertices[v as usize].pos);
                    grid.entry(cell(q)).or_default().push((q, k, i));
                }
            }
        }
    }
    for (k, i) in floating {
        let (m, p) = (meshes[k], &cut[k][i]);
        let lowest = p.tris.iter().flat_map(|&f| m.indices[f as usize..f as usize + 3].iter()).map(|&v| Vec3::from(m.vertices[v as usize].pos)).filter(|q| q.y <= p.root.y + 1e-3);
        let mut best: Option<(f32, usize, usize)> = None;
        for q in lowest {
            let c = cell(q);
            for d in [-1, 0, 1].into_iter().flat_map(|x| [-1, 0, 1].into_iter().flat_map(move |y| [-1, 0, 1].map(|z| [x, y, z]))) {
                for &(r, rk, ri) in grid.get(&[c[0] + d[0], c[1] + d[1], c[2] + d[2]]).map(Vec::as_slice).unwrap_or(&[]) {
                    let gap = r.distance(q);
                    if (rk, ri) != (k, i) && gap <= 0.01 && cut[rk][ri].root.y < p.root.y && best.is_none_or(|b| gap < b.0) {
                        best = Some((gap, rk, ri));
                    }
                }
            }
        }
        if let Some((_, rk, ri)) = best {
            cut[k][i].draw = cut[rk][ri].draw;
        }
    }
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

/// The pieces of `m` kept (`keep` says which), as a mesh of their own, each blade widened
/// `wide` times across (`Piece::across`: as long, as tall and leaning as it was, facing the
/// same way; never wider than it's long), anything else as it is.
fn kept(m: &Mesh, cut: &[Piece], keep: &dyn Fn(&Piece) -> bool, wide: f32) -> Mesh {
    let mut out = Mesh::default();
    let mut slot: HashMap<u32, u32> = HashMap::new();
    for p in cut.iter().filter(|p| keep(p)) {
        slot.clear();
        for &first in &p.tris {
            for &i in &m.indices[first as usize..first as usize + 3] {
                let n = *slot.entry(i).or_insert_with(|| {
                    let mut v = m.vertices[i as usize];
                    if let Some(a) = p.across.get(&i).filter(|_| wide.min(p.most) > 1.0) {
                        v.pos = (Vec3::from(v.pos) + *a * (wide.min(p.most) - 1.0)).to_array();
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
/// before (mats on all of them, and what rests on a piece with it), its blades widened a
/// little more across; as many as thin it (up to LEVELS), none after one piece is left. A
/// model of one piece has one level.
pub fn thin(scene: &Scene, footprint: [u8; 2]) -> Levels {
    let cells = [footprint[0].max(1) as f32, footprint[1].max(1) as f32];
    let meshes: Vec<(Option<&String>, &Mesh)> = std::iter::once((None, &scene.plain)).chain(scene.textured.iter().map(|(k, m)| (Some(k), m))).collect();
    let mut cut: Vec<Vec<Piece>> = meshes.iter().map(|(_, m)| pieces(m, cells)).collect();
    if cut.iter().all(|c| c.is_empty()) {
        return Levels::default();
    }
    rest(&meshes.iter().map(|(_, m)| *m).collect::<Vec<_>>(), &mut cut);
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
    fn the_blades_kept_widen_across_their_paint_and_heights_kept() {
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

    /// A blade curving over as it rises: six rows of two points across it (along z), tapering,
    /// its middle bending out along x, to a point; its rows' middles and half widths.
    fn curved(m: &mut Mesh, x: f32, z: f32) -> Vec<(Vec3, f32)> {
        let base = m.vertices.len() as u32;
        let rows: Vec<(Vec3, f32)> = (0..6).map(|r| (Vec3::new(x + 0.3 * (r as f32 / 5.0).powi(2), 0.5 * r as f32 / 5.0, z), 0.015 * (1.0 - r as f32 / 6.0))).collect();
        for &(c, w) in &rows {
            for s in [-1.0, 1.0] {
                m.vertices.push(Vertex { pos: (c + Vec3::Z * w * s).to_array(), normal: [-1.0, 0.0, 0.0], ..Default::default() });
            }
        }
        m.vertices.push(Vertex { pos: [x + 0.45, 0.58, z], normal: [-1.0, 0.0, 0.0], ..Default::default() });
        for r in 0..5u32 {
            let (l, q) = (base + 2 * r, base + 2 * r + 1);
            m.indices.extend([l, q, q + 2, l, q + 2, l + 2]);
        }
        m.indices.extend([base + 10, base + 11, base + 12]);
        rows
    }

    #[test]
    fn a_leaning_blade_widens_across_never_further_out() {
        let mut m = Mesh::default();
        let rows = curved(&mut m, 0.0, 0.0);
        let cut = pieces(&m, [1.0, 1.0]);
        assert_eq!(cut.len(), 1);
        let wide = kept(&m, &cut, &|_| true, 3.0);
        // (each vertex as it was, and as it's kept: the triangles go in the same order)
        let mut to = HashMap::new();
        for (&i, &j) in m.indices.iter().zip(&wide.indices) {
            to.insert(i as usize, Vec3::from(wide.vertices[j as usize].pos));
        }
        // each row three times as wide, about where its middle was, as high, leaning as far
        for (r, &(c, w)) in rows.iter().enumerate() {
            for (k, s) in [-1.0f32, 1.0].into_iter().enumerate() {
                let p = to[&(2 * r + k)];
                assert!(p.distance(c + Vec3::Z * w * 3.0 * s) < 1e-4, "row {r}: {p} for {}", c + Vec3::Z * w * 3.0 * s);
            }
        }
        // its tip where it was, and nothing reaching past it
        assert!(to[&12].distance(Vec3::new(0.45, 0.58, 0.0)) < 1e-4);
        let most = |m: &Mesh| m.vertices.iter().map(|v| v.pos[0]).fold(f32::MIN, f32::max);
        assert!(most(&wide) <= most(&m) + 1e-4, "it reaches no further out: {} for {}", most(&wide), most(&m));
        assert!((0..13).all(|i| (to[&i].y - m.vertices[i].pos[1]).abs() < 1e-6), "as high");
    }

    /// A box's six faces, closed all round: `lo` to `hi`.
    fn lump(m: &mut Mesh, lo: Vec3, hi: Vec3) {
        let base = m.vertices.len() as u32;
        for k in 0..8 {
            let p = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
            m.vertices.push(Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0], ..Default::default() });
        }
        for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
            m.indices.extend([f[0], f[1], f[2], f[0], f[2], f[3]].map(|i| base + i));
        }
    }

    #[test]
    fn a_lump_a_stalk_or_blades_joined_are_never_widened() {
        // a seed head (closed all round)
        let mut m = Mesh::default();
        lump(&mut m, Vec3::new(-0.05, 0.0, -0.05), Vec3::new(0.05, 0.3, 0.05));
        // a stalk: a tube open at both ends
        let base = m.vertices.len() as u32;
        for k in 0..6 {
            let a = k as f32 * std::f32::consts::TAU / 6.0;
            for y in [0.0, 0.6] {
                m.vertices.push(Vertex { pos: [0.4 + 0.01 * a.cos(), y, 0.01 * a.sin()], normal: [a.cos(), 0.0, a.sin()], ..Default::default() });
            }
        }
        for k in 0..6u32 {
            let (a, b) = (base + 2 * k, base + 2 * ((k + 1) % 6));
            m.indices.extend([a, b, b + 1, a, b + 1, a + 1]);
        }
        // two blades joined at their foot, as wide apart as they're long (a V)
        let base = m.vertices.len() as u32;
        for p in [[-0.4, 0.0, 0.3], [-0.38, 0.0, 0.3], [-0.6, 0.4, 0.3], [-0.2, 0.4, 0.3]] {
            m.vertices.push(Vertex { pos: p, normal: [0.0, 0.0, 1.0], ..Default::default() });
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 1, base + 3]);
        let cut = pieces(&m, [1.0, 1.0]);
        assert_eq!(cut.len(), 3);
        assert!(cut.iter().all(|p| p.across.is_empty()), "none of them a blade");
        let wide = kept(&m, &cut, &|_| true, 3.0);
        let mut moved = wide.vertices.iter().map(|v| v.pos).collect::<Vec<_>>();
        let mut was = m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>();
        moved.sort_by(|a, b| a.partial_cmp(b).unwrap());
        was.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(moved, was, "each as it was");
    }

    #[test]
    fn a_seed_head_is_kept_or_left_with_its_stalk() {
        // 200 stalks (curved blades), each with a seed head resting on its tip
        let mut m = Mesh::default();
        for i in 0..200 {
            let (x, z) = (-0.45 + (i % 20) as f32 * 0.045, -0.45 + (i / 20) as f32 * 0.09);
            curved(&mut m, x, z);
            lump(&mut m, Vec3::new(x + 0.445, 0.58, z - 0.005), Vec3::new(x + 0.455, 0.64, z + 0.005));
        }
        let levels = thin(&Scene { plain: m, textured: Default::default() }, [1, 1]);
        assert!(levels.scenes.len() >= 5);
        for s in &levels.scenes {
            let v = &s.plain.vertices;
            // each blade's tip (a point at 0.58 up) with a seed head over it (its foot at 0.58)
            let tips: Vec<[i32; 2]> = v.iter().filter(|q| (q.pos[1] - 0.58).abs() < 1e-5 && q.normal[0] < -0.5).map(|q| [(q.pos[0] * 1e3).round() as i32, (q.pos[2] * 1e3).round() as i32]).collect();
            let heads = v.iter().filter(|q| q.normal[1] > 0.5).count() / 8;
            assert_eq!(tips.len(), heads, "as many heads as stalks");
            for t in &tips {
                assert!(v.iter().any(|q| q.normal[1] > 0.5 && (q.pos[1] - 0.58).abs() < 1e-5 && ((q.pos[0] * 1e3).round() as i32 - t[0]).abs() <= 11 && ((q.pos[2] * 1e3).round() as i32 - t[1]).abs() <= 11), "a head on the stalk at {t:?}");
            }
        }
    }
}
