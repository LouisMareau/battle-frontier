//! Ramps on maps: a row of cells along the top of a step in the ground (one cell deep, as long
//! as the step lets it be) whose ground goes down from their level to the level below, flush
//! with the ground at their foot. One level only: 2 down to 1, never 2 down to 0. The cells keep
//! their level (the upper one) and their terrain; the engine shapes their ground
//! (`world::ramps`), a little uneven, each ramp its own way.

use serde::{Deserialize, Serialize};

use crate::building::side_step;

/// How uneven a new ramp's ground is (`Ramp::rough`).
pub const ROUGH: f64 = 0.5;

/// The ways a prop lies along the length of a ramp (`RampFit::along`).
pub const ALONG: [&str; 2] = ["stretch", "repeat"];

/// How high a prop made to lie along ramps rises when its record doesn't say (a level, as
/// the world's settings have it by default: a character's 1.5 cells and 0.5 more).
pub const RISE: f64 = 2.0;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ramp {
    /// its north-west cell
    pub at: [i32; 2],
    /// the cells it covers, along x and z: one deep up its way, as long as it is along the step
    pub size: [u16; 2],
    /// the way it rises, from its foot to its top: "n", "e", "s" or "w"
    pub up: String,
    /// how uneven its ground is: 0 (smooth) to 1
    pub rough: f64,
    /// its unevenness's own: the same seed, the same bumps
    pub seed: u32,
}

impl Default for Ramp {
    fn default() -> Ramp {
        Ramp { at: [0, 0], size: [1, 1], up: "n".into(), rough: ROUGH, seed: 0 }
    }
}

impl Ramp {
    /// Whether a map cell is one of its cells.
    pub fn has(&self, x: i32, z: i32) -> bool {
        x >= self.at[0] && z >= self.at[1] && x < self.at[0] + self.size[0] as i32 && z < self.at[1] + self.size[1] as i32
    }

    /// Its cells, row by row (north first).
    pub fn cells(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::with_capacity(self.size[0] as usize * self.size[1] as usize);
        for z in 0..self.size[1] as i32 {
            for x in 0..self.size[0] as i32 {
                out.push((self.at[0] + x, self.at[1] + z));
            }
        }
        out
    }

    /// The way up, as a step from one cell to the next (north: 0, -1).
    pub fn way(&self) -> (i32, i32) {
        side_step(&self.up).unwrap_or((0, -1))
    }

    /// How many cells deep it is, up its way (one, as a ramp must be).
    pub fn depth(&self) -> u16 {
        if self.way().0 != 0 { self.size[0] } else { self.size[1] }
    }

    /// How many cells long it is, along the step.
    pub fn length(&self) -> u16 {
        if self.way().0 != 0 { self.size[1] } else { self.size[0] }
    }

    /// The cells just past its foot: a level down.
    pub fn past_foot(&self) -> Vec<(i32, i32)> {
        let (dx, dz) = self.way();
        self.cells().into_iter().map(|(x, z)| (x - dx, z - dz)).filter(|&(x, z)| !self.has(x, z)).collect()
    }

    /// Where a point of the map (in cells) is on it: how far up (0 at its foot, 1 at its
    /// top) and along it (0 to 1); past 0 to 1 outside it.
    pub fn along(&self, px: f64, pz: f64) -> (f64, f64) {
        let u = (px - self.at[0] as f64) / self.size[0].max(1) as f64;
        let v = (pz - self.at[1] as f64) / self.size[1].max(1) as f64;
        match self.up.as_str() {
            "s" => (v, u),
            "e" => (u, v),
            "w" => (1.0 - u, v),
            _ => (1.0 - v, u),
        }
    }

    /// The way a ramp on these cells (a row along the top of a step, `level` giving the cells'
    /// levels, None off the map) would rise: from the side whose ground past it is all a level
    /// down. Where two sides are (a ridge, a corner), the one whose foot looks most `towards`
    /// (x, z: the camera's side, say). Why not, when it can't be.
    pub fn rising(at: [i32; 2], size: [u16; 2], towards: (f32, f32), level: &dyn Fn(i32, i32) -> Option<u8>) -> Result<String, String> {
        let probe = Ramp { at, size, ..Default::default() };
        let cells = probe.cells();
        let Some(&first) = cells.first() else { return Err("A ramp needs a cell at least".into()) };
        let Some(top) = level(first.0, first.1) else { return Err("The ramp goes off the map".into()) };
        if let Some(other) = cells.iter().filter_map(|&(x, z)| level(x, z)).find(|&l| l != top) {
            return Err(format!("A ramp's cells are on one level: these are on {top} and {other}"));
        }
        if size[0] > 1 && size[1] > 1 {
            return Err("A ramp is one cell deep: a row of cells along the top of a step".into());
        }
        let mut ways = vec![];
        let mut lows = vec![];
        for up in ["n", "e", "s", "w"] {
            let r = Ramp { up: up.into(), ..probe.clone() };
            if r.depth() != 1 {
                continue;
            }
            let past: Vec<Option<u8>> = r.past_foot().into_iter().map(|(x, z)| level(x, z)).collect();
            if !past.is_empty() && top > 0 && past.iter().all(|&l| l == Some(top - 1)) {
                ways.push(r);
            } else {
                lows.extend(past.into_iter().flatten().filter(|&l| l + 1 < top));
            }
        }
        let pick = match ways.len() {
            0 => {
                let why = match lows.iter().min() {
                    Some(&low) => format!("the ground below is on {low}: a ramp goes down one level only ({top} to {})", top - 1),
                    None if top == 0 => "there's nothing below level 0 to go down to".to_string(),
                    None => format!("these cells don't step down to ground a level below ({})", top - 1),
                };
                return Err(format!("No ramp there: {why}"));
            }
            1 => ways.remove(0),
            _ => {
                // the foot that looks most the way asked
                let facing = |r: &Ramp| {
                    let (dx, dz) = r.way();
                    -(dx as f32 * towards.0 + dz as f32 * towards.1)
                };
                ways.into_iter().max_by(|a, b| facing(a).total_cmp(&facing(b))).unwrap()
            }
        };
        Ok(pick.up)
    }

    /// What's wrong with it on a map whose cells' levels `level` gives (None off the map).
    pub fn problems(&self, level: &dyn Fn(i32, i32) -> Option<u8>) -> Vec<String> {
        let at = format!("the ramp at {}, {}", self.at[0], self.at[1]);
        let mut out = vec![];
        if side_step(&self.up).is_none() {
            out.push(format!("{at} rises \"{}\": n, e, s or w", self.up));
            return out;
        }
        if self.size[0] == 0 || self.size[1] == 0 {
            out.push(format!("{at} has no cells (size {:?})", self.size));
            return out;
        }
        if self.depth() != 1 {
            out.push(format!("{at} is {} cells deep up its way: a ramp is one (a row along the top of a step)", self.depth()));
        }
        if !(0.0..=1.0).contains(&self.rough) {
            out.push(format!("{at}: rough {}, 0 to 1", self.rough));
        }
        let levels: Vec<Option<u8>> = self.cells().into_iter().map(|(x, z)| level(x, z)).collect();
        let Some(top) = levels.iter().copied().collect::<Option<Vec<u8>>>().and_then(|l| l.first().copied()) else {
            out.push(format!("{at} goes off the map"));
            return out;
        };
        if let Some(other) = levels.iter().flatten().find(|&&l| l != top) {
            out.push(format!("{at}: its cells must be on one level (they're on {top} and {other})"));
            return out;
        }
        if top == 0 {
            out.push(format!("{at} is on level 0: there's nothing below it to go down to"));
            return out;
        }
        for (x, z) in self.past_foot() {
            match level(x, z) {
                None => {
                    out.push(format!("{at}: its foot is past the map's edge"));
                    break;
                }
                Some(l) if l + 1 != top => {
                    out.push(format!("{at}: a ramp goes down one level, {top} to {}, but past its foot the ground is on {l} (at {x}, {z})", top - 1));
                    break;
                }
                _ => {}
            }
        }
        out
    }
}

/// How a prop lies along ramps (`Prop::ramp`): stairs, a mud slide. Its model is made one
/// cell deep and `rise` high, going up along -z: its foot along z 0.5 on the ground (y 0),
/// its top along z -0.5, `rise` up, as wide along x as its footprint's first number. Put on
/// a ramp, it's turned up the ramp's way and made a level high (the world's), its foot on
/// the ground at the ramp's foot and its top on the ground at the ramp's top, and it covers
/// the ramp from end to end: "stretch", one stretched along it, or "repeat", copies side by
/// side, one per its width (each stretched a little where the ramp's length isn't a whole
/// number of them); either way, `ends` of it at either end keep their size (a parapet, a
/// post). One goes on a ramp; the ground under it goes straight from its foot to its top,
/// and it's walked on, never in the way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RampFit {
    /// along the ramp's length: "stretch" or "repeat" (`ALONG`)
    pub along: String,
    /// how high its model rises from its foot to its top, in cells: a level when it was made
    pub rise: f64,
    /// how much of each of its ends (along x, in cells) keeps its size when it's stretched
    #[serde(skip_serializing_if = "is_zero")]
    pub ends: f64,
}

impl Default for RampFit {
    fn default() -> RampFit {
        RampFit { along: "stretch".into(), rise: RISE, ends: 0.0 }
    }
}

impl RampFit {
    /// Copies side by side along a ramp, rather than one stretched along it.
    pub fn repeats(&self) -> bool {
        self.along == "repeat"
    }

    /// What's wrong with it, on a prop of that footprint.
    pub fn problems(&self, footprint: [u8; 2]) -> Vec<String> {
        let mut out = vec![];
        if !ALONG.contains(&self.along.as_str()) {
            out.push(format!("it lies along a ramp \"{}\": {}", self.along, ALONG.join(" or ")));
        }
        if self.rise.is_nan() || self.rise <= 0.0 {
            out.push(format!("along a ramp, it rises {}: more than 0 cells (a level when it was made)", self.rise));
        }
        let width = footprint[0].max(1) as f64;
        if self.ends.is_nan() || self.ends < 0.0 || self.ends * 2.0 >= width {
            out.push(format!("along a ramp, it keeps {} of each end: 0, up to under half its width ({width})", self.ends));
        }
        if footprint[1] != 1 {
            out.push(format!("it's {} cells deep: a prop along a ramp is made one cell deep, as a ramp is", footprint[1]));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map 8 wide, 8 deep: a plateau on 2 north of z 4 (a tower on 3 at its east end, x 6
    /// and 7), the lowland on 1 south of it.
    fn level(x: i32, z: i32) -> Option<u8> {
        if !(0..8).contains(&x) || !(0..8).contains(&z) {
            return None;
        }
        Some(if z >= 4 { 1 } else if x >= 6 { 3 } else { 2 })
    }

    #[test]
    fn a_ramp_goes_down_from_the_top_of_a_step() {
        // two long, on the plateau's edge, its foot on the lowland south of it
        assert_eq!(Ramp::rising([2, 3], [2, 1], (0.0, 1.0), &level).as_deref(), Ok("n"));
        let r = Ramp { at: [2, 3], size: [2, 1], up: "n".into(), ..Default::default() };
        assert_eq!((r.depth(), r.length()), (1, 2));
        assert_eq!(r.past_foot(), [(2, 4), (3, 4)]);
        assert!(r.problems(&level).is_empty(), "{:?}", r.problems(&level));
        // its foot at the south edge, its top at the north one, along it from west to east
        assert_eq!(r.along(3.0, 4.0), (0.0, 0.5));
        assert_eq!(r.along(3.0, 3.0), (1.0, 0.5));
        assert!(r.has(3, 3) && !r.has(4, 3) && !r.has(3, 4));
    }

    #[test]
    fn one_cell_deep_one_level_down() {
        // the tower's edge: 3 down to 1
        let why = Ramp::rising([6, 3], [2, 1], (0.0, 1.0), &level).unwrap_err();
        assert!(why.contains("on 1") && why.contains("one level only (3 to 2)"), "{why}");
        let r = Ramp { at: [6, 3], size: [2, 1], up: "n".into(), ..Default::default() };
        assert!(r.problems(&level)[0].contains("past its foot the ground is on 1"), "{:?}", r.problems(&level));
        // the middle of the plateau: nothing steps down
        assert!(Ramp::rising([2, 1], [2, 1], (0.0, 1.0), &level).unwrap_err().contains("don't step down"));
        // two rows deep
        assert!(Ramp::rising([2, 2], [2, 2], (0.0, 1.0), &level).unwrap_err().contains("one cell deep"));
        assert!(Ramp { at: [2, 2], size: [2, 2], ..Default::default() }.problems(&level)[0].contains("2 cells deep"));
        // cells on two levels
        assert!(Ramp::rising([2, 3], [1, 2], (0.0, 1.0), &level).unwrap_err().contains("on one level"));
        // nothing below level 0; off the map; a way that isn't one
        let flat = |x: i32, z: i32| ((0..8).contains(&x) && (0..8).contains(&z)).then_some(0);
        assert!(Ramp::rising([2, 2], [1, 1], (0.0, 1.0), &flat).unwrap_err().contains("nothing below level 0"));
        assert!(Ramp { at: [7, 3], size: [2, 1], ..Default::default() }.problems(&level)[0].contains("off the map"));
        assert!(Ramp { up: "up".into(), ..Default::default() }.problems(&level)[0].contains("n, e, s or w"));
    }

    #[test]
    fn at_a_corner_the_side_looked_at() {
        // a plateau's south-east corner cell (3, 3) steps down south and east
        let corner = |x: i32, z: i32| ((0..8).contains(&x) && (0..8).contains(&z)).then_some(if x < 4 && z < 4 { 2 } else { 1 });
        // seen from the south, its foot south: it rises north
        assert_eq!(Ramp::rising([3, 3], [1, 1], (0.0, 1.0), &corner).as_deref(), Ok("n"));
        // seen from the east, its foot east: it rises west
        assert_eq!(Ramp::rising([3, 3], [1, 1], (1.0, 0.0), &corner).as_deref(), Ok("w"));
    }
}
