//! Meadows (`content::Meadow`): blades of grass and flowers the engine grows itself through a
//! terrain's cells. Placed here (`grow`: where each stands, which way it leans, how big, which
//! of the meadow's plants), made on the GPU as they're drawn (the renderer's blades); a plant
//! that's a grass prop's model is placed the same way, as a copy of its model (`Tuft`). The
//! blades of a clump (a cell of a Voronoi pattern, `Meadow::clump` across) are alike, as tall,
//! leaning out from its middle; patches a few cells across thin the meadow out; a plant that
//! keeps to clusters grows mostly in its own. The same every time.

use std::collections::HashMap;

use content::{Content, Meadow, Plant, Terrain};
use glam::{Vec2, Vec3};

use crate::mesh::{hash, noise3, Tuft};
use crate::Rng;

/// A blade or a flower placed (`grow`), for the renderer to make: its foot, the way it leans
/// (radians about y as `Mat4::from_rotation_y` turns: 0 leans towards +x), how big within its
/// plant's sizes (0: the shortest, 1: the tallest), its kind (its plant's place among every
/// meadow's, `Kinds`), and its own random bits (its lean, its bend, its shade, its time in
/// the wind).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blade {
    pub at: Vec3,
    pub turn: f32,
    pub size: f32,
    pub kind: u32,
    pub seed: u32,
}

/// The most plants all the meadows may have together (the renderer's kinds; those past it grow
/// nothing).
pub const MOST_KINDS: usize = 64;

/// Every meadow's plants in one list, the renderer's kinds: each terrain's in turn (in the
/// content's order, none on water), its plants in theirs; a plant's kind is its place in it.
pub struct Kinds<'a> {
    pub list: Vec<(&'a Terrain, &'a Plant)>,
    first: HashMap<&'a str, u32>,
}

impl<'a> Kinds<'a> {
    pub fn of(content: &'a Content) -> Kinds<'a> {
        let mut list = vec![];
        let mut first = HashMap::new();
        for t in content.terrains.iter().filter(|t| grows_meadow(t)) {
            first.insert(t.id.as_str(), list.len() as u32);
            for p in &t.growth.meadow.plants {
                if list.len() < MOST_KINDS {
                    list.push((t, p));
                }
            }
        }
        Kinds { list, first }
    }

    /// The first kind of a terrain's meadow, if it grows one.
    pub fn first(&self, terrain: &str) -> Option<u32> {
        self.first.get(terrain).copied()
    }
}

/// Whether a terrain grows a meadow: it has plants, and it isn't water.
pub fn grows_meadow(t: &Terrain) -> bool {
    !t.growth.meadow.is_empty() && t.walk != "water" && t.pattern != "water"
}

/// A plant's colours, linear: at its foot and its tip (its terrain's own where it says none:
/// the darker of its two at the foot, the lighter at the tip), its petals' and its heart's.
pub fn colours(t: &Terrain, p: &Plant) -> [[f32; 4]; 4] {
    let (a, b) = (crate::color(&t.color), crate::color(if t.color2.is_empty() { &t.color } else { &t.color2 }));
    let bright = |c: &[f32; 4]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    let (dark, light) = if bright(&a) <= bright(&b) { (a, b) } else { (b, a) };
    let or = |hex: &str, own: [f32; 4]| if content::is_hex_colour(hex) { crate::color(hex) } else { own };
    [or(&p.color, dark), or(&p.tip, light), or(&p.petal, [0.9, 0.88, 0.82, 1.0]), or(&p.heart, [0.8, 0.5, 0.05, 1.0])]
}

/// A blade's own random bits (`Blade::seed`), from where it is in its cell's list.
fn bits(x: i32, z: i32, k: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (z as u32).wrapping_mul(0x85eb_ca77) ^ (k as u32).wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

/// The clump a point (a map's own cells) is in, of a Voronoi pattern `size` cells across: its
/// middle, and its own 0 to 1.
fn clump(p: Vec2, size: f32) -> (Vec2, f32) {
    let q = p / size;
    let (cx, cz) = (q.x.floor() as i32, q.y.floor() as i32);
    let mut best = (f32::MAX, p, 0.5);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (i, j) = (cx + dx, cz + dz);
            let c = Vec2::new(i as f32 + 0.1 + 0.8 * hash(i, j, 71), j as f32 + 0.1 + 0.8 * hash(i, j, 73)) * size;
            let d = c.distance_squared(p);
            if d < best.0 {
                best = (d, c, hash(i, j, 79));
            }
        }
    }
    (best.1, best.2)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How thick a meadow grows at a point (0 to 1): thinner in its patches a few cells across
/// (bare in the thinnest, all patchy).
fn thick(m: &Meadow, p: Vec2) -> f32 {
    let k = ((noise3(Vec3::new(p.x, 0.0, p.y), 3.5, 41) - 0.3) / 0.4).clamp(0.0, 1.0);
    1.0 - m.patchy.clamp(0.0, 1.0) as f32 * k * k * (3.0 - 2.0 * k)
}

/// How much of its weight a plant (the meadow's `i`th) has at a point: all of it, unless it
/// keeps to clusters (a few cells across, its own), outside them as little as it keeps to them.
fn clustered(p: &Plant, i: usize, at: Vec2) -> f32 {
    let c = p.clusters.clamp(0.0, 1.0) as f32;
    if c <= 0.0 {
        return 1.0;
    }
    let n = noise3(Vec3::new(at.x, 0.0, at.y), 2.0, 101 + i as i32);
    1.0 - c * (1.0 - smoothstep(0.7, 0.8, n))
}

/// A meadow's plants grown through cell x, z (its terrain's kinds from `first`): so many on
/// average, fewer in its patches, spread evenly over it (standing where `foot` says, from its
/// north-west corner: none where it's cut away), each one of its plants by their weights
/// there, sized and leaning as its clump says; the engine's into `blades`, the models into
/// `tufts`. The same every time.
pub fn grow(m: &Meadow, first: u32, x: i32, z: i32, foot: &dyn Fn(f32, f32) -> Option<f32>, blades: &mut Vec<Blade>, tufts: &mut Vec<Tuft>) {
    if m.plants.is_empty() {
        return;
    }
    let density = m.density.clamp(0.0, Meadow::MOST) as f32;
    let n = (density + hash(x, z, 211)).floor() as i32;
    // spread evenly: a sequence that fills the cell however many there are (R2), from a
    // place of the cell's own
    let (ou, ov) = (hash(x, z, 213), hash(x, z, 217));
    let size = m.clump.max(0.0) as f32;
    let alike = m.clumping.clamp(0.0, 1.0) as f32;
    let mut weights: Vec<f32> = Vec::with_capacity(m.plants.len());
    for k in 0..n {
        let mut r = Rng::new(bits(x, z, k));
        let jitter = 0.35 / (n as f32).sqrt().max(1.0);
        let u = (ou + k as f32 * 0.754_877_7 + (r.next() - 0.5) * jitter).rem_euclid(1.0);
        let v = (ov + k as f32 * 0.569_840_3 + (r.next() - 0.5) * jitter).rem_euclid(1.0);
        let at = Vec2::new(x as f32 + u, z as f32 + v);
        let (thin, pick, own, way) = (r.next(), r.next(), r.next(), r.next());
        let Some(y) = foot(u, v) else { continue };
        if thin > thick(m, at) {
            continue;
        }
        // which plant: by their weights here
        weights.clear();
        weights.extend(m.plants.iter().enumerate().map(|(i, p)| p.weight.max(0.0) as f32 * clustered(p, i, at)));
        let total: f32 = weights.iter().sum();
        if total <= 1e-6 {
            continue;
        }
        let mut left = pick * total;
        let i = weights.iter().position(|&w| {
            left -= w;
            left < 0.0
        });
        let i = i.unwrap_or(m.plants.len() - 1);
        // its clump: as big as the clump says, leaning out from its middle
        let mut lean = Vec2::from_angle(way * std::f32::consts::TAU);
        let mut big = own;
        if size > 0.0 && alike > 0.0 {
            let (middle, its) = clump(at, size);
            big = own + (its - own) * alike;
            let out = at - middle;
            if out.length_squared() > 1e-8 {
                lean = (lean + (out.normalize() - lean) * (alike * 0.85)).normalize_or(lean);
            }
        }
        let foot = Vec3::new(at.x, y, at.y);
        let p = &m.plants[i];
        if p.shape == "model" {
            if !p.prop.is_empty() {
                let (lo, hi) = (p.scale[0].min(p.scale[1]), p.scale[0].max(p.scale[1]));
                // (a tuft's turn: degrees, clockwise seen from above)
                let turn = (-lean.y).atan2(lean.x).to_degrees();
                tufts.push(Tuft { prop: p.prop.clone(), at: foot, turn: -turn, scale: (lo + (hi - lo) * big as f64) as f32, far: true });
            }
            continue;
        }
        if (first as usize + i) < MOST_KINDS {
            blades.push(Blade { at: foot, turn: (-lean.y).atan2(lean.x), size: big, kind: first + i as u32, seed: bits(x, z, k ^ 0x5bd1) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Growth, Space};

    fn meadow(plants: Vec<Plant>) -> Meadow {
        Meadow { plants, ..Default::default() }
    }

    fn grown(m: &Meadow, cells: i32) -> (Vec<Blade>, Vec<Tuft>) {
        let (mut blades, mut tufts) = (vec![], vec![]);
        for z in 0..cells {
            for x in 0..cells {
                grow(m, 0, x, z, &|_, _| Some(0.5), &mut blades, &mut tufts);
            }
        }
        (blades, tufts)
    }

    #[test]
    fn a_meadow_grows_as_thick_as_it_says_the_same_every_time_and_on_its_cells() {
        let m = Meadow { patchy: 0.0, ..meadow(vec![Plant::default()]) };
        let (blades, tufts) = grown(&m, 16);
        assert!(tufts.is_empty());
        let per = blades.len() as f64 / 256.0;
        assert!((per - m.density).abs() < 1.0, "{per} a cell for {}", m.density);
        assert_eq!(grown(&m, 16).0, blades, "the same every time");
        for b in &blades {
            assert!((0.0..16.0).contains(&b.at.x) && (0.0..16.0).contains(&b.at.z) && b.at.y == 0.5, "{b:?}");
            assert!((0.0..=1.0).contains(&b.size) && b.kind == 0);
        }
        // spread evenly over a cell: each quarter of it has about a quarter of its blades
        let mut one = vec![];
        grow(&m, 0, 3, 5, &|_, _| Some(0.0), &mut one, &mut vec![]);
        for (qx, qz) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let n = one.iter().filter(|b| ((b.at.x - 3.0) * 2.0) as i32 == qx && ((b.at.z - 5.0) * 2.0) as i32 == qz).count();
            assert!((8..=16).contains(&n), "a quarter of the cell: {n} of {}", one.len());
        }
        // none where its foot is cut away
        let mut half = vec![];
        grow(&m, 0, 0, 0, &|u, _| (u < 0.5).then_some(0.0), &mut half, &mut vec![]);
        assert!(half.iter().all(|b| b.at.x < 0.5) && half.len() > 15 && half.len() < 35);
    }

    #[test]
    fn patches_thin_it_and_clumps_are_alike() {
        let even = Meadow { patchy: 0.0, clumping: 0.0, ..meadow(vec![Plant::default()]) };
        let patchy = Meadow { patchy: 1.0, ..even.clone() };
        let (all, thin) = (grown(&even, 24).0.len(), grown(&patchy, 24).0.len());
        assert!(thin < all * 9 / 10 && thin > all / 4, "patches: {thin} of {all}");
        // clumped: blades near each other are alike in size, more than unclumped
        let clumped = Meadow { clump: 0.8, clumping: 1.0, ..even.clone() };
        let spread = |m: &Meadow| {
            let b = grown(m, 8).0;
            let mut d = 0.0;
            let mut n = 0;
            for (i, a) in b.iter().enumerate() {
                for c in b[i + 1..].iter().take(40).filter(|c| c.at.distance(a.at) < 0.08) {
                    d += (a.size - c.size).abs();
                    n += 1;
                }
            }
            d / n.max(1) as f32
        };
        assert!(spread(&clumped) < spread(&even) * 0.5, "{} against {}", spread(&clumped), spread(&even));
    }

    #[test]
    fn plants_grow_by_their_weights_clustered_ones_in_clusters_and_models_as_tufts() {
        let m = Meadow {
            patchy: 0.0,
            plants: vec![
                Plant { weight: 3.0, ..Default::default() },
                Plant { shape: "flower".into(), weight: 1.0, ..Default::default() },
                Plant { shape: "model".into(), prop: "tuft".into(), weight: 1.0, scale: [0.5, 0.5], ..Default::default() },
            ],
            ..Default::default()
        };
        let (blades, tufts) = grown(&m, 16);
        let (grass, flowers) = (blades.iter().filter(|b| b.kind == 0).count(), blades.iter().filter(|b| b.kind == 1).count());
        let all = (grass + flowers + tufts.len()) as f32;
        assert!((grass as f32 / all - 0.6).abs() < 0.04 && (flowers as f32 / all - 0.2).abs() < 0.04, "{grass} {flowers} {}", tufts.len());
        assert!(tufts.iter().all(|t| t.prop == "tuft" && t.scale == 0.5 && t.far));
        // the flowers keeping to clusters: as many, near one another
        let mut c = m.clone();
        c.plants[1].clusters = 1.0;
        let (blades2, _) = grown(&c, 16);
        let f: Vec<&Blade> = blades2.iter().filter(|b| b.kind == 1).collect();
        assert!(f.len() > flowers / 8 && f.len() < flowers, "clustered: {} of {flowers}", f.len());
        // cells with any of them, against cells with any unclustered
        let cells = |b: &[&Blade]| b.iter().map(|b| (b.at.x as i32, b.at.z as i32)).collect::<std::collections::HashSet<_>>().len();
        let loose: Vec<&Blade> = blades.iter().filter(|b| b.kind == 1).collect();
        assert!(cells(&f) * 2 < cells(&loose), "in clusters: {} cells against {}", cells(&f), cells(&loose));
    }

    #[test]
    fn every_meadow_s_plants_are_one_list_of_kinds() {
        let mut c = Content::default();
        let with = |id: &str, plants: usize, walk: &str| Terrain { id: id.into(), walk: walk.into(), pattern: "grass".into(), growth: Growth { meadow: meadow(vec![Plant::default(); plants]), ..Default::default() }, ..Default::default() };
        c.terrains.put(with("a_lawn", 2, "walk"), Space::Content);
        c.terrains.put(with("b_pond", 3, "water"), Space::Content);
        c.terrains.put(with("c_field", 3, "walk"), Space::Content);
        c.terrains.put(Terrain { id: "d_dirt".into(), walk: "walk".into(), ..Default::default() }, Space::Content);
        let k = Kinds::of(&c);
        assert_eq!(k.list.len(), 5);
        assert_eq!((k.first("a_lawn"), k.first("c_field"), k.first("b_pond"), k.first("d_dirt")), (Some(0), Some(2), None, None));
        // a plant with no colours of its own: its terrain's, the darker at its foot
        let t = Terrain { color: "#80c060".into(), color2: "#204010".into(), ..Default::default() };
        let [foot, tip, _, _] = colours(&t, &Plant::default());
        assert!(foot[1] < tip[1]);
        assert_eq!(colours(&t, &Plant { color: "#ff0000".into(), ..Default::default() })[0], [1.0, 0.0, 0.0, 1.0]);
    }
}
