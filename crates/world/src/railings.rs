//! Railings put together from a style's pieces (props of category "railing": see
//! `content::RailingPiece`): a span along each edge given railing, and a post on each point
//! where spans meet or end. The pieces are picked by wave function collapse: every span
//! starts with all its style's spans (either way round, when their ends differ), every point
//! with all its style's posts that stand on that kind of point; the slot with the fewest
//! choices left is decided (by weight, from a seed: the same railing every time), and
//! whatever no longer agrees with what's next to it goes (the word a span's end says must be
//! one the post there takes), until each slot has one. On a dead end it starts again from
//! the next seed; past a few tries, the first choices that fit are taken.

use std::collections::BTreeMap;

use content::{Content, RailingPiece};
use glam::Vec3;

/// A unit edge given railing, between two grid points (from the building's `at`), and the
/// railing's style.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub style: String,
}

/// A piece where it goes: its prop, its foot (from the building's `at`, on the floor) and
/// its turn (degrees, as `mesh::placement` takes it): a span's +x along its edge, a post's
/// +x away from its point along one of its spans.
#[derive(Clone, Debug, PartialEq)]
pub struct Put {
    pub prop: String,
    pub at: Vec3,
    pub turn: f32,
}

/// The turn that points a piece's +x along a step.
fn turn_of(d: (i32, i32)) -> f32 {
    match d {
        (1, 0) => 0.0,
        (0, 1) => 90.0,
        (-1, 0) => 180.0,
        _ => 270.0,
    }
}

/// What kind of point it is (`content::JUNCTIONS`), from the steps away from it along its
/// spans and whether it's on the outside wall.
pub fn junction(steps: &[(i32, i32)], at_wall: bool) -> &'static str {
    match steps.len() {
        0 | 1 if at_wall => "wall",
        0 | 1 => "end",
        2 if steps[0].0 == -steps[1].0 && steps[0].1 == -steps[1].1 => "line",
        2 => "corner",
        3 => "tee",
        _ => "cross",
    }
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

/// A point of a railing: where it is, its slot (none: no post stands on such points), the
/// steps away from it along its spans, its kind.
struct Point {
    at: (i32, i32),
    slot: Option<usize>,
    steps: Vec<(i32, i32)>,
    kind: &'static str,
}

/// A span's or a point's choices: (piece, the other way round), and how likely each is.
struct Slot {
    choices: Vec<(usize, bool)>,
    weights: Vec<f64>,
}

/// The railing for these edges, each style's from its own pieces. `at_wall`: is a point on
/// the outside wall; `inset`: how far a post that ends a railing against that wall stands
/// off it (the wall's thickness). Edges of a style no span piece has come back as they are,
/// for the caller to draw plainly.
pub fn assemble(edges: &[Edge], content: &Content, at_wall: &dyn Fn((i32, i32)) -> bool, inset: f32, seed: u64) -> (Vec<Put>, Vec<Edge>) {
    let mut by_style: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        by_style.entry(e.style.as_str()).or_default().push(e);
    }
    let mut puts = vec![];
    let mut unknown = vec![];
    for (k, (style, mine)) in by_style.into_iter().enumerate() {
        let pieces: Vec<(&str, &RailingPiece)> = content.props.iter().filter_map(|p| p.railing.as_ref().filter(|r| r.style == style).map(|r| (p.id.as_str(), r))).collect();
        let spans: Vec<(&str, &RailingPiece)> = pieces.iter().copied().filter(|(_, r)| r.is_span()).collect();
        let posts: Vec<(&str, &RailingPiece)> = pieces.iter().copied().filter(|(_, r)| !r.is_span()).collect();
        if spans.is_empty() {
            unknown.extend(mine.into_iter().cloned());
            continue;
        }
        puts.extend(one_style(&mine, &spans, &posts, at_wall, inset, seed.wrapping_add(k as u64 * 7919)));
    }
    (puts, unknown)
}

fn one_style(edges: &[&Edge], spans: &[(&str, &RailingPiece)], posts: &[(&str, &RailingPiece)], at_wall: &dyn Fn((i32, i32)) -> bool, inset: f32, seed: u64) -> Vec<Put> {
    // each edge from its lower point to its higher one
    let edges: Vec<((i32, i32), (i32, i32))> = edges.iter().map(|e| if e.a <= e.b { (e.a, e.b) } else { (e.b, e.a) }).collect();
    // each point: its edges, and whether it's their first point
    let mut points: BTreeMap<(i32, i32), Vec<(usize, bool)>> = BTreeMap::new();
    for (i, &(a, b)) in edges.iter().enumerate() {
        points.entry(a).or_default().push((i, true));
        points.entry(b).or_default().push((i, false));
    }
    // the word a span says at one of its ends: its -x end's when unturned at its first point
    let end_word = |piece: usize, flipped: bool, first: bool| -> &str {
        let ends = &spans[piece].1.ends;
        ends.get(if first != flipped { 0 } else { 1 }).map(String::as_str).unwrap_or("rail")
    };
    // the slots: the spans first, then the points with posts to choose from
    let mut slots: Vec<Slot> = vec![];
    for _ in &edges {
        let mut s = Slot { choices: vec![], weights: vec![] };
        for (k, (_, r)) in spans.iter().enumerate() {
            s.choices.push((k, false));
            s.weights.push(r.weight);
            if r.ends.first() != r.ends.get(1) {
                s.choices.push((k, true));
                s.weights.push(r.weight);
            }
        }
        slots.push(s);
    }
    let mut at_points: Vec<Point> = vec![];
    // the ties: (a point's slot, a span's slot, whether the point is the span's first)
    let mut ties: Vec<(usize, usize, bool)> = vec![];
    for (&p, here) in &points {
        let steps: Vec<(i32, i32)> = here
            .iter()
            .map(|&(i, first)| {
                let (a, b) = edges[i];
                let (from, to) = if first { (a, b) } else { (b, a) };
                ((to.0 - from.0).signum(), (to.1 - from.1).signum())
            })
            .collect();
        let kind = junction(&steps, at_wall(p));
        let mut s = Slot { choices: vec![], weights: vec![] };
        for (k, (_, r)) in posts.iter().enumerate() {
            if r.at.iter().any(|a| a == kind) {
                s.choices.push((k, false));
                s.weights.push(r.weight);
            }
        }
        // no post stands on such a point: none there, nothing to agree with
        let slot = if s.choices.is_empty() {
            None
        } else {
            slots.push(s);
            let id = slots.len() - 1;
            ties.extend(here.iter().map(|&(i, first)| (id, i, first)));
            Some(id)
        };
        at_points.push(Point { at: p, slot, steps, kind });
    }
    let agree = |post: (usize, bool), span: (usize, bool), first: bool| posts[post.0].1.takes.iter().any(|t| t == end_word(span.0, span.1, first));
    let chosen = collapse(&slots, &ties, &agree, seed);
    let mut out = vec![];
    for (i, &(a, b)) in edges.iter().enumerate() {
        let (k, flipped) = slots[i].choices[chosen[i]];
        let at = Vec3::new((a.0 + b.0) as f32 / 2.0, 0.0, (a.1 + b.1) as f32 / 2.0);
        let turn = turn_of(((b.0 - a.0).signum(), (b.1 - a.1).signum())) + if flipped { 180.0 } else { 0.0 };
        out.push(Put { prop: spans[k].0.to_string(), at, turn });
    }
    for Point { at: p, slot, steps, kind } in at_points {
        let Some(slot) = slot else { continue };
        let (k, _) = slots[slot].choices[chosen[slot]];
        let away = steps[0];
        let mut at = Vec3::new(p.0 as f32, 0.0, p.1 as f32);
        if kind == "wall" {
            at += Vec3::new(away.0 as f32, 0.0, away.1 as f32) * inset;
        }
        out.push(Put { prop: posts[k].0.to_string(), at, turn: turn_of(away) });
    }
    out
}

/// Wave function collapse over slots tied in pairs (a point's slot, a span's slot, which of
/// the span's ends is at the point): each slot's choice, an index into its choices.
fn collapse(slots: &[Slot], ties: &[(usize, usize, bool)], agree: &dyn Fn((usize, bool), (usize, bool), bool) -> bool, seed: u64) -> Vec<usize> {
    let mut touching: Vec<Vec<usize>> = vec![vec![]; slots.len()];
    for (t, &(p, s, _)) in ties.iter().enumerate() {
        touching[p].push(t);
        touching[s].push(t);
    }
    let left = |alive: &[bool]| alive.iter().filter(|&&a| a).count();
    for attempt in 0..8u64 {
        let mut dice = Dice(seed ^ attempt.wrapping_mul(0x2545_f491_4f6c_dd1d));
        let mut alive: Vec<Vec<bool>> = slots.iter().map(|s| vec![true; s.choices.len()]).collect();
        if !settle(slots, ties, &touching, agree, &mut alive, (0..slots.len()).collect()) {
            continue;
        }
        let mut stuck = false;
        // the undecided slot with the fewest choices left (the first of those)
        while let Some(i) = (0..slots.len()).filter(|&i| left(&alive[i]) > 1).min_by_key(|&i| left(&alive[i])) {
            let open: Vec<usize> = (0..alive[i].len()).filter(|&c| alive[i][c]).collect();
            let total: f64 = open.iter().map(|&c| slots[i].weights[c]).sum();
            let mut r = dice.next() * total;
            let mut pick = open[open.len() - 1];
            for &c in &open {
                r -= slots[i].weights[c];
                if r < 0.0 {
                    pick = c;
                    break;
                }
            }
            for (c, a) in alive[i].iter_mut().enumerate() {
                *a = c == pick;
            }
            if !settle(slots, ties, &touching, agree, &mut alive, vec![i]) {
                stuck = true;
                break;
            }
        }
        if !stuck {
            return alive.iter().map(|a| a.iter().position(|&x| x).unwrap_or(0)).collect();
        }
    }
    vec![0; slots.len()]
}

/// Takes away the choices that agree with nothing left in a slot they're tied to, from the
/// slots in `changed` outwards: false on a dead end (a slot with nothing left).
fn settle(slots: &[Slot], ties: &[(usize, usize, bool)], touching: &[Vec<usize>], agree: &dyn Fn((usize, bool), (usize, bool), bool) -> bool, alive: &mut [Vec<bool>], mut changed: Vec<usize>) -> bool {
    while let Some(i) = changed.pop() {
        for &t in &touching[i] {
            let (p, s, first) = ties[t];
            let other = if p == i { s } else { p };
            let mut removed = false;
            for c in 0..alive[other].len() {
                if !alive[other][c] {
                    continue;
                }
                let held = (0..alive[i].len()).filter(|&d| alive[i][d]).any(|d| {
                    let (post, span) = if other == p { (slots[p].choices[c], slots[s].choices[d]) } else { (slots[p].choices[d], slots[s].choices[c]) };
                    agree(post, span, first)
                });
                if !held {
                    alive[other][c] = false;
                    removed = true;
                }
            }
            if removed {
                if !alive[other].iter().any(|&a| a) {
                    return false;
                }
                if !changed.contains(&other) {
                    changed.push(other);
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Prop, Space};

    fn piece(id: &str, r: RailingPiece) -> Prop {
        Prop { id: id.into(), name: id.into(), category: "railing".into(), footprint: [1, 1], height: 0.9, railing: Some(r), ..Default::default() }
    }

    /// A style "t": a plain span, a panel that needs posts at both ends, a post for any
    /// point, a joint (no post) only where two spans are in a line, a bracket against walls.
    fn style() -> Content {
        let mut c = Content::default();
        let span = |ends: [&str; 2], weight: f64| RailingPiece { style: "t".into(), part: "span".into(), ends: ends.iter().map(|e| e.to_string()).collect(), weight, ..Default::default() };
        let post = |takes: &[&str], at: &[&str], weight: f64| RailingPiece { style: "t".into(), part: "post".into(), ends: vec![], takes: takes.iter().map(|e| e.to_string()).collect(), at: at.iter().map(|e| e.to_string()).collect(), weight };
        c.props.put(piece("plain", span(["rail", "rail"], 2.0)), Space::Content);
        c.props.put(piece("panel", span(["post", "post"], 2.0)), Space::Content);
        c.props.put(piece("post", post(&["rail", "post"], &["end", "line", "corner", "tee", "cross"], 1.0)), Space::Content);
        c.props.put(piece("joint", post(&["rail"], &["line"], 3.0)), Space::Content);
        c.props.put(piece("bracket", post(&["rail", "post"], &["wall"], 1.0)), Space::Content);
        c
    }

    /// The edges round cells x 1-2, z 1 (a hole two cells long), and one from a wall.
    fn edges() -> Vec<Edge> {
        let e = |a: (i32, i32), b: (i32, i32)| Edge { a, b, style: "t".into() };
        vec![e((1, 1), (2, 1)), e((2, 1), (3, 1)), e((1, 2), (2, 2)), e((2, 2), (3, 2)), e((1, 1), (1, 2)), e((3, 1), (3, 2)), e((5, 0), (5, 1))]
    }

    #[test]
    fn the_pieces_agree_at_every_point() {
        let content = style();
        let wall = |p: (i32, i32)| p.1 == 0;
        for seed in 0..40 {
            let (puts, unknown) = assemble(&edges(), &content, &wall, 0.12, seed);
            assert!(unknown.is_empty());
            let at = |x: f32, z: f32| puts.iter().find(|p| (p.at.x - x).abs() < 1e-4 && (p.at.z - z).abs() < 1e-4).map(|p| p.prop.as_str());
            // a span on each edge; corners and the free end posts; the wall end a bracket
            assert_eq!(puts.len(), 7 + 4 + 2 + 2, "{puts:?}");
            for corner in [(1.0, 1.0), (3.0, 1.0), (1.0, 2.0), (3.0, 2.0)] {
                assert_eq!(at(corner.0, corner.1), Some("post"), "seed {seed}: {corner:?}");
            }
            assert_eq!(at(5.0, 1.0), Some("post"), "a free end");
            assert_eq!(at(5.0, 0.12), Some("bracket"), "off the wall by its thickness");
            // a panel never meets a joint: posts at both its ends
            for p in puts.iter().filter(|p| p.prop == "panel") {
                let (dx, dz) = if p.turn % 180.0 == 0.0 { (0.5, 0.0) } else { (0.0, 0.5) };
                for end in [(p.at.x - dx, p.at.z - dz), (p.at.x + dx, p.at.z + dz)] {
                    assert_ne!(at(end.0, end.1), Some("joint"), "seed {seed}: a panel at {:?} ends at a joint", p.at);
                }
            }
        }
        // the same railing from the same seed; others vary
        let one = assemble(&edges(), &content, &wall, 0.12, 7).0;
        assert_eq!(one, assemble(&edges(), &content, &wall, 0.12, 7).0);
        assert!((0..40).any(|s| assemble(&edges(), &content, &wall, 0.12, s).0 != one), "the seed changes it");
        // both line points see joints and posts over the seeds, and both kinds of span
        let all: Vec<Put> = (0..40).flat_map(|s| assemble(&edges(), &content, &wall, 0.12, s).0).collect();
        for kind in ["joint", "panel", "plain"] {
            assert!(all.iter().any(|p| p.prop == kind), "{kind} somewhere");
        }
    }

    #[test]
    fn a_style_without_pieces_comes_back() {
        let mut odd = edges();
        odd[0].style = "nothing".into();
        let (puts, unknown) = assemble(&odd, &style(), &|_| false, 0.12, 1);
        assert_eq!(unknown.len(), 1);
        assert_eq!(puts.iter().filter(|p| p.prop == "plain" || p.prop == "panel").count(), 6);
    }
}
