//! Floors laid by wave function collapse: a set's tiles (props of category "floor" whose
//! `floor` names the set: `content::FloorTile`) on the cells given, each cell one of them,
//! turned when it may be, every two side by side agreeing on the edge between them (the same
//! word on both). Every cell starts with every way the set's tiles lie; the cell with the
//! fewest left is decided (by weight, from a seed: the same floor every time) and whatever
//! no longer agrees with the cells next to it goes, until each cell has one. Cells laid
//! already (`fixed`) hold, binding those next to them when they're of the set. Past the cells
//! given (the floor's edge, a hole, another flooring) only what may end there faces out: with
//! a tile whose edge is "rim", rims, and those face nothing else (a border all round); else
//! the words a tile has on all four edges (the joints between plain flagstones: never half a
//! slab that goes on past the floor's edge), or anything when no tile has the same all round.
//! On a dead end it starts again from the next seed; past a few tries, each cell takes what
//! fits best the cells decided before it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use content::{Building, Content};

/// The steps to the cells north, east, south and west (the order a tile's edges go round).
pub const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// The word an edge says to face out of the cells laid, when the set has it.
pub const RIM: &str = "rim";

/// A tile laid on a cell: the prop, its quarter turns clockwise (seen from above), and
/// whether it's mirrored before it's turned (only ever by hand: the laying never mirrors).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Laid {
    pub tile: String,
    pub turn: u8,
    pub flip: bool,
}

/// One way a set's tile lies: the tile, its turn, its edges as it lies (north, east, south,
/// west), how likely.
#[derive(Clone, Debug)]
struct Way {
    tile: String,
    turn: u8,
    edges: [String; 4],
    weight: f64,
}

/// Every way a set's tiles lie: each unturned, and turned the three other ways when it may be
/// (its weight shared among its turns, so turning doesn't make it likelier).
fn ways(content: &Content, set: &str) -> Vec<Way> {
    let mut out = vec![];
    for p in content.floor_sets().remove(set).unwrap_or_default() {
        let Some(f) = &p.floor else { continue };
        let turns: u8 = if f.turns { 4 } else { 1 };
        for t in 0..turns {
            out.push(Way { tile: p.id.clone(), turn: t, edges: std::array::from_fn(|s| f.edge(t, s).to_string()), weight: f.weight / turns as f64 });
        }
    }
    out
}

/// The set a floor tile is of, if it's of one.
pub fn set_of<'a>(content: &'a Content, tile: &str) -> Option<&'a str> {
    content.props.get(tile).and_then(|p| p.floor.as_ref()).map(|f| f.set.as_str())
}

/// A small source of numbers (splitmix64): the same ones from the same seed.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// What binds a cell's side: another cell being laid (its index), the words it may have there
/// (a cell laid already, of the set: its own; past the cells, what may end there), or nothing.
#[derive(Clone, Debug)]
enum Bind {
    Cell(usize),
    Words(Vec<String>),
    Free,
}

/// `cells` laid with `set`'s tiles (each one once, in any order), `fixed` the cells laid
/// already round them; the same seed, the same floor. A set with no tiles lays nothing.
pub fn lay(content: &Content, set: &str, cells: &[(i32, i32)], fixed: &HashMap<(i32, i32), Laid>, seed: u64) -> BTreeMap<(i32, i32), Laid> {
    let ways = ways(content, set);
    let cells: Vec<(i32, i32)> = cells.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
    if ways.is_empty() || cells.is_empty() {
        return BTreeMap::new();
    }
    let index: HashMap<(i32, i32), usize> = cells.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let rims = ways.iter().any(|w| w.edges.iter().any(|e| e == RIM));
    // what may face out: rims; else what some tile has all round; else anything
    let fillers: BTreeSet<String> = ways.iter().filter(|w| w.edges.iter().all(|e| *e == w.edges[0])).map(|w| w.edges[0].clone()).collect();
    let outside = if rims {
        Bind::Words(vec![RIM.to_string()])
    } else if !fillers.is_empty() {
        Bind::Words(fillers.into_iter().collect())
    } else {
        Bind::Free
    };
    // what's on each side of each cell
    let binds: Vec<[Bind; 4]> = cells
        .iter()
        .map(|&(x, z)| {
            std::array::from_fn(|s| {
                let n = (x + SIDES[s].0, z + SIDES[s].1);
                if let Some(&j) = index.get(&n) {
                    return Bind::Cell(j);
                }
                let held = fixed.get(&n).and_then(|l| {
                    let f = content.props.get(&l.tile)?.floor.as_ref().filter(|f| f.set == set)?;
                    Some(f.edge_as(l.turn, l.flip, (s + 2) % 4).to_string())
                });
                match held {
                    Some(word) => Bind::Words(vec![word]),
                    None => outside.clone(),
                }
            })
        })
        .collect();
    // each cell's ways that fit what's fixed round it, rims facing out only (a cell nothing
    // fits keeps them all, and goes by what fits best)
    let start: Vec<Vec<bool>> = binds
        .iter()
        .map(|b| {
            let fits: Vec<bool> = ways
                .iter()
                .map(|w| {
                    (0..4).all(|s| match &b[s] {
                        Bind::Words(words) => words.contains(&w.edges[s]),
                        Bind::Cell(_) => !rims || w.edges[s] != RIM,
                        Bind::Free => true,
                    })
                })
                .collect();
            if fits.iter().any(|&f| f) {
                fits
            } else {
                vec![true; ways.len()]
            }
        })
        .collect();
    let left = |alive: &[bool]| alive.iter().filter(|&&a| a).count();
    'attempt: for attempt in 0..8u64 {
        let mut dice = Dice(seed ^ attempt.wrapping_mul(0x2545_f491_4f6c_dd1d));
        let mut alive = start.clone();
        if !settle(&ways, &binds, &mut alive, (0..cells.len()).collect()) {
            continue;
        }
        while let Some(i) = (0..cells.len()).filter(|&i| left(&alive[i]) > 1).min_by_key(|&i| left(&alive[i])) {
            let open: Vec<usize> = (0..ways.len()).filter(|&w| alive[i][w]).collect();
            let total: f64 = open.iter().map(|&w| ways[w].weight).sum();
            let mut r = dice.next() * total;
            let mut pick = open[open.len() - 1];
            for &w in &open {
                r -= ways[w].weight;
                if r < 0.0 {
                    pick = w;
                    break;
                }
            }
            for (w, a) in alive[i].iter_mut().enumerate() {
                *a = w == pick;
            }
            if !settle(&ways, &binds, &mut alive, vec![i]) {
                continue 'attempt;
            }
        }
        return cells.iter().zip(&alive).map(|(&c, a)| (c, laid(&ways[a.iter().position(|&x| x).unwrap_or(0)]))).collect();
    }
    // past the tries: each cell what agrees most with those decided before it and what's
    // fixed, the likeliest of those
    let mut chosen: Vec<Option<usize>> = vec![None; cells.len()];
    for i in 0..cells.len() {
        let agrees = |w: &Way| {
            (0..4)
                .filter(|&s| match &binds[i][s] {
                    Bind::Cell(j) => chosen[*j].is_some_and(|c| ways[c].edges[(s + 2) % 4] == w.edges[s]),
                    Bind::Words(words) => words.contains(&w.edges[s]),
                    Bind::Free => false,
                })
                .count()
        };
        let best = (0..ways.len()).filter(|&w| start[i][w]).max_by(|&a, &b| agrees(&ways[a]).cmp(&agrees(&ways[b])).then(ways[a].weight.total_cmp(&ways[b].weight)).then(b.cmp(&a)));
        chosen[i] = best.or(Some(0));
    }
    cells.iter().zip(&chosen).map(|(&c, w)| (c, laid(&ways[w.unwrap_or(0)]))).collect()
}

fn laid(w: &Way) -> Laid {
    Laid { tile: w.tile.clone(), turn: w.turn, flip: false }
}

/// Takes away the ways that agree with nothing left on a cell next to them, from the cells in
/// `changed` outwards: false on a dead end (a cell with nothing left).
fn settle(ways: &[Way], binds: &[[Bind; 4]], alive: &mut [Vec<bool>], mut changed: Vec<usize>) -> bool {
    while let Some(i) = changed.pop() {
        for (s, bind) in binds[i].iter().enumerate() {
            let Bind::Cell(j) = *bind else { continue };
            // the words cell i can still show on that side; cell j's ways must show one back
            let words: BTreeSet<&str> = (0..ways.len()).filter(|&w| alive[i][w]).map(|w| ways[w].edges[s].as_str()).collect();
            let mut removed = false;
            for w in 0..ways.len() {
                if alive[j][w] && !words.contains(ways[w].edges[(s + 2) % 4].as_str()) {
                    alive[j][w] = false;
                    removed = true;
                }
            }
            if removed {
                if !alive[j].iter().any(|&a| a) {
                    return false;
                }
                if !changed.contains(&j) {
                    changed.push(j);
                }
            }
        }
    }
    true
}

/// How a design put down by hand is put again round a centre (a mosaic, a church floor's
/// symmetric design): each tile put also where the symmetry takes it, mirrored and turned as
/// it takes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Symmetry {
    #[default]
    None,
    /// mirrored left to right, across the line from north to south through the centre
    MirrorX,
    /// mirrored top to bottom, across the line from east to west through it
    MirrorZ,
    /// mirrored both ways: four of each
    MirrorBoth,
    /// a half turn round it: two of each
    Turn2,
    /// quarter turns round it: four of each
    Turn4,
    /// quarter turns, and mirrored across both lines and both diagonals: eight of each
    All8,
}

impl Symmetry {
    pub const ALL: [Symmetry; 7] = [Symmetry::None, Symmetry::MirrorX, Symmetry::MirrorZ, Symmetry::MirrorBoth, Symmetry::Turn2, Symmetry::Turn4, Symmetry::All8];

    /// Its name for the checks and the panels' marks.
    pub fn key(self) -> &'static str {
        match self {
            Symmetry::None => "none",
            Symmetry::MirrorX => "mirror_x",
            Symmetry::MirrorZ => "mirror_z",
            Symmetry::MirrorBoth => "mirror_both",
            Symmetry::Turn2 => "turn2",
            Symmetry::Turn4 => "turn4",
            Symmetry::All8 => "all8",
        }
    }

    /// What it does to a tile, each way it puts one: quarter turns clockwise round the centre,
    /// mirrored across the line from north to south first or not; the tile as it was put first.
    fn moves(self) -> &'static [(u8, bool)] {
        match self {
            Symmetry::None => &[(0, false)],
            Symmetry::MirrorX => &[(0, false), (0, true)],
            Symmetry::MirrorZ => &[(0, false), (2, true)],
            Symmetry::MirrorBoth => &[(0, false), (0, true), (2, true), (2, false)],
            Symmetry::Turn2 => &[(0, false), (2, false)],
            Symmetry::Turn4 => &[(0, false), (1, false), (2, false), (3, false)],
            Symmetry::All8 => &[(0, false), (1, false), (2, false), (3, false), (0, true), (1, true), (2, true), (3, true)],
        }
    }

    /// Whether it turns a quarter round its centre (which must then be a cell's middle or a
    /// corner: round any other, a quarter turn lands between cells).
    pub fn quarter_turns(self) -> bool {
        matches!(self, Symmetry::Turn4 | Symmetry::All8)
    }

    /// The lines it mirrors across, through the centre: north to south, east to west, and
    /// the diagonals.
    pub fn lines(self) -> (bool, bool, bool) {
        match self {
            Symmetry::MirrorX => (true, false, false),
            Symmetry::MirrorZ => (false, true, false),
            Symmetry::MirrorBoth => (true, true, false),
            Symmetry::All8 => (true, true, true),
            _ => (false, false, false),
        }
    }
}

/// Where a tile put on `cell`, lying `turn` quarter turns (mirrored first or not), is put by
/// `symmetry` round `centre`, in half cells (a cell's middle is odd both ways, a corner even
/// both ways, an edge's middle one of each): each place once, as the tile lies there, the cell
/// itself first (a cell the symmetry takes onto itself keeps the tile as it was put). A quarter
/// turn round the middle of an edge lands between cells: those are left out.
pub fn symmetric(symmetry: Symmetry, centre: (i32, i32), cell: (i32, i32), turn: u8, flip: bool) -> Vec<((i32, i32), u8, bool)> {
    let mut out: Vec<((i32, i32), u8, bool)> = vec![];
    // the cell's middle from the centre, in half cells
    let (dx, dz) = (2 * cell.0 + 1 - centre.0, 2 * cell.1 + 1 - centre.1);
    for &(r, m) in symmetry.moves() {
        let (mut x, mut z) = if m { (-dx, dz) } else { (dx, dz) };
        for _ in 0..r {
            // a quarter turn clockwise seen from above: north to east, east to south
            (x, z) = (-z, x);
        }
        let (px, pz) = (centre.0 + x, centre.1 + z);
        if px.rem_euclid(2) != 1 || pz.rem_euclid(2) != 1 {
            continue;
        }
        let at = ((px - 1).div_euclid(2), (pz - 1).div_euclid(2));
        if out.iter().any(|(c, ..)| *c == at) {
            continue;
        }
        // mirrored, the tile turns the other way round
        let t = if m { (r + 4 - turn % 4) % 4 } else { (r + turn) % 4 };
        out.push((at, t, flip ^ m));
    }
    out
}

/// A building's floor `f` as it's laid on `cells` (those with flooring: not its holes nor the
/// stairs' way up): each cell laid by itself as it was (a tile and its turn), the others with
/// the floor's own; when that's a tile of a set, laid with the set's tiles to fit (with those
/// round them), from a seed of the building's and the floor's.
pub fn floor_laid(b: &Building, f: usize, content: &Content, cells: &[(i32, i32)]) -> HashMap<(i32, i32), Laid> {
    let floor = &b.floors[f];
    let mut out: HashMap<(i32, i32), Laid> = HashMap::new();
    let mut own = vec![];
    for &(x, z) in cells {
        match floor.laid(x, z) {
            Some(t) => {
                out.insert((x, z), Laid { tile: t.tile.clone(), turn: t.turn, flip: t.flip });
            }
            None => own.push((x, z)),
        }
    }
    match set_of(content, &floor.tile) {
        Some(set) => {
            let seed = b.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100_0000_01b3)) ^ (f as u64).wrapping_mul(0x9e37_79b9) ^ 0xf100;
            let laid = lay(content, set, &own, &out, seed);
            out.extend(laid);
        }
        None => {
            out.extend(own.into_iter().map(|c| (c, Laid { tile: floor.tile.clone(), turn: 0, flip: false })));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{FloorTile, Prop, Space};

    fn tile(id: &str, set: &str, edges: [&str; 4], weight: f64, turns: bool) -> Prop {
        Prop { id: id.into(), name: id.into(), category: "floor".into(), footprint: [1, 1], height: 0.05, floor: Some(FloorTile { set: set.into(), edges: edges.map(String::from), weight, turns }), ..Default::default() }
    }

    /// Flagstones: small ones, a slab, and a long slab over two cells (its halves meet along
    /// "long"), which may lie either way.
    fn flags() -> Content {
        let mut c = Content::default();
        c.props.put(tile("small", "flags", ["joint"; 4], 2.0, false), Space::Content);
        c.props.put(tile("slab", "flags", ["joint"; 4], 1.0, false), Space::Content);
        c.props.put(tile("long_a", "flags", ["joint", "long", "joint", "joint"], 3.0, true), Space::Content);
        c.props.put(tile("long_b", "flags", ["joint", "joint", "joint", "long"], 3.0, true), Space::Content);
        c
    }

    fn edge(c: &Content, l: &Laid, side: usize) -> String {
        c.props.get(&l.tile).unwrap().floor.as_ref().unwrap().edge_as(l.turn, l.flip, side).to_string()
    }

    /// Every two cells side by side agree on the edge between them.
    fn agree(c: &Content, laid: &BTreeMap<(i32, i32), Laid>) -> bool {
        laid.iter().all(|(&(x, z), l)| {
            (0..4).all(|s| laid.get(&(x + SIDES[s].0, z + SIDES[s].1)).is_none_or(|n| edge(c, l, s) == edge(c, n, (s + 2) % 4)))
        })
    }

    fn area(w: i32, h: i32) -> Vec<(i32, i32)> {
        (0..h).flat_map(|z| (0..w).map(move |x| (x, z))).collect()
    }

    #[test]
    fn a_turned_tiles_edges_go_round_with_it() {
        let f = FloorTile { set: "s".into(), edges: ["n", "e", "s", "w"].map(String::from), weight: 1.0, turns: true };
        assert_eq!((0..4).map(|s| f.edge(1, s)).collect::<Vec<_>>(), ["w", "n", "e", "s"], "a quarter turn clockwise: its north edge faces east");
        assert_eq!((0..4).map(|s| f.edge(2, s)).collect::<Vec<_>>(), ["s", "w", "n", "e"]);
    }

    #[test]
    fn every_two_tiles_side_by_side_agree_the_same_way_every_time() {
        let c = flags();
        let laid = lay(&c, "flags", &area(9, 7), &HashMap::new(), 7);
        assert_eq!(laid.len(), 63);
        assert!(agree(&c, &laid), "each long slab's halves together");
        let longs = laid.values().filter(|l| l.tile.starts_with("long")).count();
        assert!(longs >= 8 && longs % 2 == 0, "long slabs, both halves each (none half past the edge): {longs}");
        assert!(laid.iter().all(|(&(x, z), l)| (0..4).all(|s| laid.contains_key(&(x + SIDES[s].0, z + SIDES[s].1)) || edge(&c, l, s) == "joint")), "joints all round");
        assert!(laid.values().any(|l| l.tile.starts_with("long") && l.turn % 2 == 1), "some lying the other way");
        assert_eq!(laid, lay(&c, "flags", &area(9, 7), &HashMap::new(), 7), "the same seed, the same floor");
        assert_ne!(laid, lay(&c, "flags", &area(9, 7), &HashMap::new(), 8), "another seed, another");
    }

    #[test]
    fn the_cells_laid_already_bind_those_next_to_them() {
        let c = flags();
        // a long slab's first half laid east of the cells, its long edge facing them (west)
        let fixed = HashMap::from([((3, 0), Laid { tile: "long_a".into(), turn: 2, flip: false })]);
        for seed in 0..12 {
            let laid = lay(&c, "flags", &area(3, 2), &fixed, seed);
            let next = &laid[&(2, 0)];
            assert_eq!(edge(&c, next, 1), "long", "seed {seed}: the other half next to it: {next:?}");
            assert!(agree(&c, &laid));
        }
        // one of another set binds nothing
        let mut other = flags();
        other.props.put(tile("rug", "rugs", ["fringe"; 4], 1.0, false), Space::Content);
        let laid = lay(&other, "flags", &area(3, 2), &HashMap::from([((3, 0), Laid { tile: "rug".into(), turn: 0, flip: false })]), 3);
        assert_eq!(laid.len(), 6);
    }

    #[test]
    fn a_set_with_rims_has_them_all_round() {
        let mut c = Content::default();
        c.props.put(tile("middle", "marble", ["inlay"; 4], 2.0, false), Space::Content);
        c.props.put(tile("side", "marble", ["rim", "border", "inlay", "border"], 1.0, true), Space::Content);
        c.props.put(tile("corner", "marble", ["rim", "rim", "border", "border"], 1.0, true), Space::Content);
        let cells = area(6, 4);
        let laid = lay(&c, "marble", &cells, &HashMap::new(), 2);
        assert!(agree(&c, &laid));
        for (&(x, z), l) in &laid {
            for s in 0..4 {
                let n = (x + SIDES[s].0, z + SIDES[s].1);
                if !laid.contains_key(&n) {
                    assert_eq!(edge(&c, l, s), RIM, "{x},{z} side {s}: a rim facing out ({l:?})");
                }
            }
        }
        assert!(laid.values().any(|l| l.tile == "middle"), "the middle inlaid");
    }

    #[test]
    fn a_set_that_cant_fit_still_lays_every_cell() {
        let mut c = Content::default();
        c.props.put(tile("odd", "odd", ["a", "b", "c", "d"], 1.0, false), Space::Content);
        let laid = lay(&c, "odd", &area(3, 3), &HashMap::new(), 1);
        assert_eq!(laid.len(), 9);
        assert!(lay(&c, "none", &area(3, 3), &HashMap::new(), 1).is_empty(), "no such set: nothing");
    }

    #[test]
    fn a_mirrored_tiles_edges_swap_east_and_west_before_it_turns() {
        let f = FloorTile { set: "s".into(), edges: ["n", "e", "s", "w"].map(String::from), weight: 1.0, turns: true };
        assert_eq!((0..4).map(|s| f.edge_as(0, true, s)).collect::<Vec<_>>(), ["n", "w", "s", "e"]);
        assert_eq!((0..4).map(|s| f.edge_as(1, true, s)).collect::<Vec<_>>(), ["e", "n", "w", "s"], "then turned: its north faces east still");
        assert_eq!((0..4).map(|s| f.edge_as(3, false, s)).collect::<Vec<_>>(), (0..4).map(|s| f.edge(3, s)).collect::<Vec<_>>());
    }

    #[test]
    fn a_tile_laid_mirrored_by_hand_binds_those_next_to_it() {
        let c = flags();
        // a long slab's first half mirrored: its long edge west, facing the cells
        let fixed = HashMap::from([((3, 0), Laid { tile: "long_a".into(), turn: 0, flip: true })]);
        for seed in 0..8 {
            let laid = lay(&c, "flags", &area(3, 2), &fixed, seed);
            assert_eq!(edge(&c, &laid[&(2, 0)], 1), "long", "seed {seed}: the other half against it: {:?}", laid[&(2, 0)]);
        }
    }

    /// The edges of a tile of four different ones, as it lies.
    fn sides(turn: u8, flip: bool) -> Vec<String> {
        let f = FloorTile { set: "s".into(), edges: ["n", "e", "s", "w"].map(String::from), weight: 1.0, turns: true };
        (0..4).map(|s| f.edge_as(turn, flip, s).to_string()).collect()
    }

    #[test]
    fn symmetric_copies_lie_as_the_symmetry_takes_the_tile() {
        // mirrored left to right across x = 2 (a corner's line): the cell west of it east of it,
        // its edges mirrored (what faced east faces west)
        let copies = symmetric(Symmetry::MirrorX, (4, 0), (1, 0), 1, false);
        assert_eq!(copies, vec![((1, 0), 1, false), ((2, 0), 3, true)]);
        let (a, b) = (sides(1, false), sides(3, true));
        assert_eq!((a[1].as_str(), a[3].as_str(), a[0].as_str()), (b[3].as_str(), b[1].as_str(), b[0].as_str()));
        // every symmetry, every way a tile lies: each copy's edges are the tile's, gone round
        // with the cells (what faced a side faces where the symmetry takes that side)
        for symmetry in Symmetry::ALL {
            for (centre, cell) in [((5, 5), (1, 0)), ((4, 6), (0, 1)), ((6, 4), (4, 3))] {
                for turn in 0..4 {
                    for flip in [false, true] {
                        let copies = symmetric(symmetry, centre, cell, turn, flip);
                        assert_eq!(copies[0], (cell, turn, flip), "the tile as it was put first");
                        for (&(r, m), &(at, t, f)) in symmetry.moves().iter().zip(&copies) {
                            let mine = sides(turn, flip);
                            let theirs = sides(t, f);
                            for s in 0..4 {
                                let to = (if m { (4 - s) % 4 } else { s } + r as usize) % 4;
                                assert_eq!(mine[s], theirs[to], "{symmetry:?} round {centre:?}: {cell:?} to {at:?}, side {s}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn symmetric_copies_land_where_the_symmetry_takes_the_cell() {
        let cells = |s: Symmetry, centre: (i32, i32), cell: (i32, i32)| symmetric(s, centre, cell, 0, false).into_iter().map(|(c, ..)| c).collect::<Vec<_>>();
        // quarter turns round cell (2, 2)'s middle
        assert_eq!(cells(Symmetry::Turn4, (5, 5), (2, 0)), [(2, 0), (4, 2), (2, 4), (0, 2)]);
        // round a corner: the four cells round it
        assert_eq!(cells(Symmetry::Turn4, (4, 4), (1, 1)), [(1, 1), (2, 1), (2, 2), (1, 2)]);
        // round an edge's middle only the half turn lands on cells
        assert_eq!(cells(Symmetry::Turn4, (4, 5), (0, 0)), [(0, 0), (3, 4)]);
        // mirrored both ways round a corner
        assert_eq!(cells(Symmetry::MirrorBoth, (6, 4), (0, 0)), [(0, 0), (5, 0), (0, 3), (5, 3)]);
        // a cell on the mirror line is its own copy: put once, as it was
        assert_eq!(symmetric(Symmetry::MirrorX, (5, 0), (2, 3), 1, false), [((2, 3), 1, false)]);
        // eight round a corner; four for a cell on a diagonal
        assert_eq!(cells(Symmetry::All8, (4, 4), (0, 1)).len(), 8);
        assert_eq!(cells(Symmetry::All8, (4, 4), (0, 0)).len(), 4);
        assert_eq!(cells(Symmetry::None, (4, 4), (7, 7)), [(7, 7)]);
    }

    #[test]
    fn a_floor_of_a_set_is_laid_round_what_was_laid_by_hand() {
        let c = flags();
        let mut b = Building { id: "hall".into(), cells: vec!["####".into(), "####".into()], floors: vec![content::Floor { tile: "small".into(), ..Default::default() }], ..Default::default() };
        b.floors[0].set_laid(0, 0, "long_a", 0);
        let cells: Vec<(i32, i32)> = b.cells();
        let laid = floor_laid(&b, 0, &c, &cells);
        assert_eq!(laid.len(), 8);
        assert_eq!(laid[&(0, 0)], Laid { tile: "long_a".into(), turn: 0, flip: false }, "as laid by hand");
        assert_eq!(edge(&c, &laid[&(1, 0)], 3), "long", "its other half next to it: {:?}", laid[&(1, 0)]);
        assert_eq!(laid, floor_laid(&b, 0, &c, &cells), "the same every time");
        // a floor of a tile of no set: that tile everywhere else
        let mut plain = c.clone();
        plain.props.put(Prop { id: "boards".into(), name: "Boards".into(), category: "floor".into(), footprint: [1, 1], height: 0.05, ..Default::default() }, Space::Content);
        b.floors[0].tile = "boards".into();
        let laid = floor_laid(&b, 0, &plain, &cells);
        assert_eq!(laid.values().filter(|l| l.tile == "boards").count(), 7);
    }
}
