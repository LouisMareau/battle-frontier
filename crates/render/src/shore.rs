//! Where the water meets the land: a field over a world map's cells (its cells' texture's
//! own, the ring of cells around the map included) of how far each cell's centre is from the
//! shore, in cells, negative on the water, and which way the land is from there (the field's
//! slope, smoothed, so it turns gently from cell to cell), and for the land near water, which
//! water that is (on the land's own level). The ground's shader takes the water's waves and
//! foam from it, and the beaches' swash.

/// How far the field reaches either way, in cells: past it (or with no shore at all) all is
/// this far.
pub const SHORE_FAR: f32 = 64.0;

/// The field for `water` (a flag a cell, row by row, `width` a row): from each cell's centre
/// the straight distance to the nearest cell of the other kind's centre, less half a cell
/// (the shore runs halfway between them), negative on the water.
pub fn shore_field(water: &[bool], width: usize, height: usize) -> Vec<f32> {
    let n = width * height;
    if water.len() != n || n == 0 {
        return vec![SHORE_FAR; n];
    }
    if water.iter().all(|&w| !w) {
        return vec![SHORE_FAR; n];
    }
    if water.iter().all(|&w| w) {
        return vec![-SHORE_FAR; n];
    }
    let to_land = squared_distances(&water.iter().map(|&w| !w).collect::<Vec<_>>(), width, height);
    let to_water = squared_distances(water, width, height);
    (0..n)
        .map(|i| {
            if water[i] {
                -(to_land[i].sqrt() as f32 - 0.5).clamp(0.0, SHORE_FAR)
            } else {
                (to_water[i].sqrt() as f32 - 0.5).clamp(0.0, SHORE_FAR)
            }
        })
        .collect()
}

/// How far from the water a land cell may be and still know which water it's near (past the
/// furthest a swash goes, from a cell's corner).
const NEAR: i64 = 3;

/// The field for a world map's cells (`cells` as their texture has them: terrain, level, 0,
/// 255 a texel, `width` a row; `kinds` each terrain's water: 0 none, 1 inland, 2 oceanic):
/// each cell's [distance, way x, way z, water]. The way to the land is a unit long, or none
/// where there's no telling (far from any shore, or where two shores are as near): the
/// distance's slope, smoothed over the cells around (twice over three by three). `water` is
/// one more than the terrain of the water a cell is (the water's own), or of the nearest
/// water on its level within three cells (the land's), else 0.
pub fn shore_texels(cells: &[u8], width: usize, height: usize, kinds: &[u8]) -> Vec<[f32; 4]> {
    let n = width * height;
    if cells.len() != n * 4 || n == 0 {
        return vec![[SHORE_FAR, 0.0, 0.0, 0.0]; n];
    }
    let terrain = |i: usize| cells[i * 4] as usize;
    let level = |i: usize| cells[i * 4 + 1];
    let wet = |i: usize| kinds.get(terrain(i)).is_some_and(|&k| k > 0);
    let water: Vec<bool> = (0..n).map(wet).collect();
    let d = shore_field(&water, width, height);
    let at = |x: i64, z: i64| d[(z.clamp(0, height as i64 - 1) as usize) * width + x.clamp(0, width as i64 - 1) as usize];
    let mut way: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let (x, z) = ((i % width) as i64, (i / width) as i64);
            [(at(x + 1, z) - at(x - 1, z)) * 0.5, (at(x, z + 1) - at(x, z - 1)) * 0.5]
        })
        .collect();
    for _ in 0..2 {
        let before = way.clone();
        for (i, w) in way.iter_mut().enumerate() {
            let (x, z) = ((i % width) as i64, (i / width) as i64);
            let mut sum = [0.0f32; 2];
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (xx, zz) = ((x + dx).clamp(0, width as i64 - 1) as usize, (z + dz).clamp(0, height as i64 - 1) as usize);
                    let v = before[zz * width + xx];
                    sum = [sum[0] + v[0], sum[1] + v[1]];
                }
            }
            *w = sum;
        }
    }
    // which water each cell is, or is near on its level
    let near = |i: usize| -> f32 {
        if water[i] {
            return (terrain(i) + 1) as f32;
        }
        if d[i] > NEAR as f32 {
            return 0.0;
        }
        let (x, z) = ((i % width) as i64, (i / width) as i64);
        let mut best: Option<(i64, usize)> = None;
        for dz in -NEAR..=NEAR {
            for dx in -NEAR..=NEAR {
                let (xx, zz) = (x + dx, z + dz);
                if xx < 0 || zz < 0 || xx >= width as i64 || zz >= height as i64 {
                    continue;
                }
                let j = zz as usize * width + xx as usize;
                if water[j] && level(j) == level(i) && best.is_none_or(|(b, _)| dx * dx + dz * dz < b) {
                    best = Some((dx * dx + dz * dz, j));
                }
            }
        }
        best.map_or(0.0, |(_, j)| (terrain(j) + 1) as f32)
    };
    d.iter()
        .zip(way)
        .enumerate()
        .map(|(i, (&d, [x, z]))| {
            let len = (x * x + z * z).sqrt();
            let (wx, wz) = if len > 1e-3 { (x / len, z / len) } else { (0.0, 0.0) };
            [d, wx, wz, near(i)]
        })
        .collect()
}

/// How far round a change in the cells the field can change: as far as it reaches, and the
/// way's smoothing and the land's look round for its water.
const WINDOW: usize = SHORE_FAR as usize + 6;

/// What changed in a world map's shore field from one of its cells' textures to the next
/// (`old`, `new`; as `shore_texels` has them): the rectangle round the cells whose water or
/// level changed, as far as the field can change round them (x0, z0, x1, z1, out), and the
/// field's texels there, worked out from the cells round it as far again (as the whole map's
/// would be). None when nothing the field is made from changed; the whole map when `old`
/// isn't the same size. Only the cells in `look` (x0, z0, x1, z1, out) can have changed.
pub fn shore_window(old: &[u8], new: &[u8], width: usize, height: usize, kinds: &[u8], look: [usize; 4]) -> Option<([usize; 4], Vec<[f32; 4]>)> {
    if old.len() != new.len() || new.len() != width * height * 4 {
        return Some(([0, 0, width, height], shore_texels(new, width, height, kinds)));
    }
    let wet = |c: &[u8]| kinds.get(c[0] as usize).is_some_and(|&k| k > 0);
    let mut changed: Option<[usize; 4]> = None;
    for z in look[1]..look[3].min(height) {
        let row = z * width;
        let (from, to) = ((row + look[0]) * 4, (row + look[2].min(width)) * 4);
        for (k, (a, b)) in old[from..to].as_chunks::<4>().0.iter().zip(new[from..to].as_chunks::<4>().0).enumerate() {
            let (wa, wb) = (wet(a), wet(b));
            if wa != wb || a[1] != b[1] || (wb && a[0] != b[0]) {
                let x = look[0] + k;
                changed = Some(match changed {
                    None => [x, z, x + 1, z + 1],
                    Some(r) => [r[0].min(x), r[1].min(z), r[2].max(x + 1), r[3].max(z + 1)],
                });
            }
        }
    }
    let grow = |r: [usize; 4], by: usize| [r[0].saturating_sub(by), r[1].saturating_sub(by), (r[2] + by).min(width), (r[3] + by).min(height)];
    let write = grow(changed?, WINDOW);
    let read = grow(write, SHORE_FAR as usize + 8);
    let (rw, rh) = (read[2] - read[0], read[3] - read[1]);
    let mut cells = Vec::with_capacity(rw * rh * 4);
    for z in read[1]..read[3] {
        cells.extend_from_slice(&new[(z * width + read[0]) * 4..(z * width + read[2]) * 4]);
    }
    let field = shore_texels(&cells, rw, rh, kinds);
    let ww = write[2] - write[0];
    let mut out = Vec::with_capacity(ww * (write[3] - write[1]));
    for z in write[1]..write[3] {
        let row = (z - read[1]) * rw + (write[0] - read[0]);
        out.extend_from_slice(&field[row..row + ww]);
    }
    Some((write, out))
}

/// Every cell's squared distance (in cells) to the nearest cell where `on` is, exactly: the
/// distance transform done along the rows, then down the columns (Felzenszwalb and
/// Huttenlocher's lower envelope of parabolas).
fn squared_distances(on: &[bool], width: usize, height: usize) -> Vec<f64> {
    let mut grid: Vec<f64> = on.iter().map(|&o| if o { 0.0 } else { f64::INFINITY }).collect();
    let mut line = vec![0.0; width.max(height)];
    for z in 0..height {
        let row = &grid[z * width..(z + 1) * width];
        line[..width].copy_from_slice(row);
        let done = along(&line[..width]);
        grid[z * width..(z + 1) * width].copy_from_slice(&done);
    }
    for x in 0..width {
        for z in 0..height {
            line[z] = grid[z * width + x];
        }
        let done = along(&line[..height]);
        for z in 0..height {
            grid[z * width + x] = done[z];
        }
    }
    grid
}

/// One line's squared distances: `f` is each place's own (0 where it is, infinite where not,
/// or what the other direction found).
fn along(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    // the parabolas of the lower envelope (their places) and where each begins
    let mut v: Vec<usize> = Vec::with_capacity(n);
    let mut z: Vec<f64> = Vec::with_capacity(n);
    for q in 0..n {
        if !f[q].is_finite() {
            continue;
        }
        loop {
            match v.last() {
                None => {
                    v.push(q);
                    z.push(f64::NEG_INFINITY);
                    break;
                }
                Some(&p) => {
                    let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
                    if s <= *z.last().unwrap() {
                        v.pop();
                        z.pop();
                    } else {
                        v.push(q);
                        z.push(s);
                        break;
                    }
                }
            }
        }
    }
    if v.is_empty() {
        return vec![f64::INFINITY; n];
    }
    let mut out = vec![0.0; n];
    let mut k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while k + 1 < v.len() && z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *o = (q as f64 - p as f64).powi(2) + f[p];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distances against the slow way (every cell to every cell of the other kind).
    #[test]
    fn the_field_is_the_distance_to_the_shore() {
        let (w, h) = (9, 7);
        // a pond, a bay off it, and a lone puddle
        let rows = ["........~", "..~~~....", ".~~~~~...", ".~~~~~~..", "..~~~~...", "...~~....", "........."];
        let water: Vec<bool> = rows.iter().flat_map(|r| r.chars().map(|c| c == '~')).collect();
        let field = shore_field(&water, w, h);
        for z in 0..h {
            for x in 0..w {
                let me = water[z * w + x];
                let mut best = f64::INFINITY;
                for zz in 0..h {
                    for xx in 0..w {
                        if water[zz * w + xx] != me {
                            best = best.min(((x as f64 - xx as f64).powi(2) + (z as f64 - zz as f64).powi(2)).sqrt());
                        }
                    }
                }
                let want = (best as f32 - 0.5) * if me { -1.0 } else { 1.0 };
                assert!((field[z * w + x] - want).abs() < 1e-4, "{x},{z}: {} for {want}", field[z * w + x]);
            }
        }
        // next to the shore: half a cell either side; the pond's middle deeper than its edge
        assert_eq!(field[w + 2], -0.5);
        assert_eq!(field[w + 1], 0.5);
        assert!(field[3 * w + 3] < -1.0);
    }

    /// Cells as their texture has them: a terrain and a level each.
    fn cells_of(terrain: &[u8], level: &[u8]) -> Vec<u8> {
        terrain.iter().zip(level).flat_map(|(&t, &l)| [t, l, 0, 255]).collect()
    }

    /// The way to the land points to it, and turns gently: next to a straight coast it's
    /// straight across it, and nowhere does it turn round from one cell to the next.
    #[test]
    fn the_way_to_the_land_points_to_it_and_turns_gently() {
        let (w, h) = (12, 10);
        // the sea (terrain 1) on the west, the coast bending; grass is 0
        let terrain: Vec<u8> = (0..w * h).map(|i| ((i % w) < 4 + (i / w) / 3) as u8).collect();
        let t = shore_texels(&cells_of(&terrain, &vec![0; w * h]), w, h, &[0, 2]);
        for z in 0..h {
            let coast = 4 + z / 3;
            let i = z * w + coast - 1;
            assert!(t[i][0] < 0.0 && t[i][1] > 0.7, "{z}: the sea's last cell looks east to the land: {:?}", t[i]);
            let j = z * w + coast;
            assert!(t[j][0] > 0.0 && t[j][1] > 0.7, "{z}: the land's first cell looks east, away from the sea: {:?}", t[j]);
        }
        for z in 0..h - 1 {
            for x in 0..w - 1 {
                let (a, b, c) = (t[z * w + x], t[z * w + x + 1], t[(z + 1) * w + x]);
                for o in [b, c] {
                    if (a[1] != 0.0 || a[2] != 0.0) && (o[1] != 0.0 || o[2] != 0.0) {
                        assert!(a[1] * o[1] + a[2] * o[2] > 0.5, "{x},{z}: {a:?} next to {o:?}");
                    }
                }
            }
        }
    }

    /// Which water a cell is, or is near on its own level: the sea's own, the beach's next
    /// to it, nothing too far off, nor on a cliff over it.
    #[test]
    fn the_land_knows_the_water_it_is_near() {
        let (w, h) = (8, 3);
        // the sea (2: oceanic, terrain 1) on the west, sand (0) then a step up (level 1)
        let terrain = [1, 1, 0, 0, 0, 0, 0, 0].repeat(h);
        let level = [0, 0, 0, 0, 1, 0, 0, 0].repeat(h);
        let t = shore_texels(&cells_of(&terrain, &level), w, h, &[0, 2]);
        let water = |x: usize| t[w + x][3];
        assert_eq!(water(0), 2.0, "the sea is itself (terrain 1, plus one)");
        assert_eq!((water(2), water(3)), (2.0, 2.0), "the beach next to it knows it");
        assert_eq!(water(4), 0.0, "a step up: not on its level");
        assert_eq!(water(6), 0.0, "too far off");
    }

    /// A change worked out round it alone comes out as the whole map's would, and the field
    /// past the window doesn't change.
    #[test]
    fn a_change_is_worked_out_round_it_as_the_whole_map_would_be() {
        let (w, h) = (230, 170);
        // a lake, a river and a step up (level 1) east of x 150; a pond dug in the middle later
        let lake = |x: usize, z: usize| ((x as f32 - 60.0).powi(2) + (z as f32 - 50.0).powi(2)).sqrt() < 25.0 || (x > 100 && x < 106);
        let terrain: Vec<u8> = (0..w * h).map(|i| lake(i % w, i / w) as u8).collect();
        let level: Vec<u8> = (0..w * h).map(|i| (i % w > 150) as u8).collect();
        let old = cells_of(&terrain, &level);
        let mut dug = terrain.clone();
        for z in 120..124 {
            for x in 40..47 {
                dug[z * w + x] = 1;
            }
        }
        let new = cells_of(&dug, &level);
        let kinds = [0, 1];
        let (r, texels) = shore_window(&old, &new, w, h, &kinds, [0, 0, w, h]).expect("a change");
        assert_eq!(r, [40usize.saturating_sub(WINDOW), 120 - WINDOW, (47 + WINDOW).min(w), (124 + WINDOW).min(h)]);
        let (whole, before) = (shore_texels(&new, w, h, &kinds), shore_texels(&old, w, h, &kinds));
        for z in 0..h {
            for x in 0..w {
                let i = z * w + x;
                if x >= r[0] && x < r[2] && z >= r[1] && z < r[3] {
                    let t = texels[(z - r[1]) * (r[2] - r[0]) + x - r[0]];
                    assert!(t.iter().zip(whole[i]).all(|(a, b)| (a - b).abs() < 1e-4), "{x},{z}: {t:?} for {:?}", whole[i]);
                } else {
                    assert_eq!(whole[i], before[i], "{x},{z} changed past the window");
                }
            }
        }
        // grass painted on grass far from water: nothing
        let mut grass = new.clone();
        grass[(10 * w + 200) * 4] = 0;
        assert!(shore_window(&new, &grass, w, h, &kinds, [0, 0, w, h]).is_none());
        // and looked for only where the cells changed: the same
        assert_eq!(shore_window(&old, &new, w, h, &kinds, [40, 120, 47, 124]), Some((r, texels)));
    }

    #[test]
    fn with_no_shore_everything_is_far() {
        assert!(shore_field(&[false; 12], 4, 3).iter().all(|&d| d == SHORE_FAR));
        assert!(shore_field(&[true; 12], 4, 3).iter().all(|&d| d == -SHORE_FAR));
        assert!(shore_field(&[true; 5], 4, 3).iter().all(|&d| d == SHORE_FAR), "a field that doesn't fit: nothing known");
    }
}
