//! A placeholder battle map (scene.json, from the private map prototype): flat ground
//! cells, buildings as boxes, trees and signs as upright cutouts. Turned into meshes here, as the prototype's
//! viewer and the battle prototype's world.js do.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Deserialize)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub sheet: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoxCell {
    pub row: i32,
    pub col: i32,
    pub sheet: String,
    pub rect: [f32; 4],
    #[serde(default)]
    pub z0: Option<f32>,
    #[serde(default)]
    pub d: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Face {
    pub sheet: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Deserialize)]
pub struct MapBox {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub d: i32,
    pub h: f32,
    #[serde(default)]
    pub inset: f32,
    pub top: Vec<BoxCell>,
    pub front: Vec<BoxCell>,
    pub side: Face,
    pub back: Face,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Cutout {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub sheet: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Deserialize)]
pub struct MapScene {
    pub map: String,
    pub width: i32,
    pub height: i32,
    pub sheets: HashMap<String, String>,
    pub ground: Vec<Cell>,
    pub boxes: Vec<MapBox>,
    pub cutouts: Vec<Cutout>,
}

impl MapScene {
    pub fn parse(text: &str) -> Result<MapScene, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// Where the battle happens, in tiles: the middle of an open patch, and its depth in rows.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub x: f32,
    pub z: f32,
    pub depth: i32,
}

/// The open patch (no trees, buildings or people) nearest the middle of the map: 9x9 tiles
/// with a tile of room around buildings when the map has one; narrow maps (routes that
/// are corridors between forests) get the biggest smaller patch, right against the walls.
pub fn find_spot(m: &MapScene) -> Spot {
    let blocked = |ring: i32| {
        let mut set = HashSet::new();
        for c in &m.cutouts {
            for dx in 0..c.w {
                set.insert((c.x + dx, c.y));
                set.insert((c.x + dx, c.y - 1));
            }
        }
        for b in &m.boxes {
            for dx in -ring..b.w + ring {
                for dy in -ring..b.d + ring {
                    set.insert((b.x + dx, b.y + dy));
                }
            }
        }
        set
    };
    let sets = [blocked(1), blocked(0)];
    for (w, d) in [(9, 9), (9, 7), (7, 7), (9, 5), (7, 5), (5, 5)] {
        for set in &sets {
            let mut best: Option<(f32, Spot)> = None;
            for y in 0..=(m.height - d) {
                for x in 0..=(m.width - w) {
                    let free = (0..d).all(|dy| (0..w).all(|dx| !set.contains(&(x + dx, y + dy))));
                    if !free {
                        continue;
                    }
                    let dist = ((x as f32 + w as f32 / 2.0 - m.width as f32 / 2.0).powi(2) + (y as f32 + d as f32 / 2.0 - m.height as f32 / 2.0).powi(2)).sqrt();
                    if best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                        best = Some((dist, Spot { x: x as f32 + w as f32 / 2.0, z: y as f32 + d as f32 / 2.0, depth: d }));
                    }
                }
            }
            if let Some((_, s)) = best {
                return s;
            }
        }
    }
    Spot { x: m.width as f32 / 2.0, z: m.height as f32 / 2.0, depth: 9 }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MapVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// multiplies the texture (linear); world meshes are vertex-coloured on a white texture
    pub color: [f32; 4],
}

/// The quads that share one texture sheet.
#[derive(Default)]
pub struct MeshData {
    pub sheet: String,
    pub cutout: bool,
    pub vertices: Vec<MapVertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// A quad from its corners as seen from its front: top-left, bottom-left, bottom-right,
    /// top-right; `rect` is the pixel rectangle of the sheet drawn on it.
    fn quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], rect: [f32; 4], size: (u32, u32)) {
        let i = self.vertices.len() as u32;
        let (w, h) = (size.0 as f32, size.1 as f32);
        let e = 0.03; // a hair inside the rectangle, so an edge never samples the next cell
        let (u0, u1, v0, v1) = ((rect[0] + e) / w, (rect[2] - e) / w, (rect[1] + e) / h, (rect[3] - e) / h);
        let uvs = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]];
        for k in 0..4 {
            self.vertices.push(MapVertex { pos: corners[k], normal, uv: uvs[k], color: [1.0; 4] });
        }
        self.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
}

const UP: [f32; 3] = [0.0, 1.0, 0.0];
const SOUTH: [f32; 3] = [0.0, 0.0, 1.0];
const NORTH: [f32; 3] = [0.0, 0.0, -1.0];
const EAST: [f32; 3] = [1.0, 0.0, 0.0];
const WEST: [f32; 3] = [-1.0, 0.0, 0.0];

/// The map's meshes, one per (sheet, cutout). Boxes between the battle spot and the camera
/// (south of the spot, a bit to the right, where the camera stands) are drawn flat.
pub fn build_meshes(m: &MapScene, sizes: &HashMap<String, (u32, u32)>, spot: Spot) -> Vec<MeshData> {
    let mut batches: Vec<MeshData> = vec![];
    let mut index: HashMap<(String, bool), usize> = HashMap::new();
    let mut batch = |sheet: &str, cutout: bool, batches: &mut Vec<MeshData>| -> usize {
        *index.entry((sheet.to_string(), cutout)).or_insert_with(|| {
            batches.push(MeshData { sheet: sheet.to_string(), cutout, ..Default::default() });
            batches.len() - 1
        })
    };
    let size = |sheet: &str| sizes.get(sheet).copied().unwrap_or((512, 512));
    let flat = |b: &mut MeshData, x: f32, z: f32, w: f32, d: f32, y: f32, rect: [f32; 4], s: (u32, u32)| {
        b.quad([[x, y, z], [x, y, z + d], [x + w, y, z + d], [x + w, y, z]], UP, rect, s);
    };
    for g in &m.ground {
        let i = batch(&g.sheet, false, &mut batches);
        flat(&mut batches[i], g.x as f32, g.y as f32, 1.0, 1.0, 0.0, g.rect, size(&g.sheet));
    }
    let in_the_way = |b: &MapBox| {
        (b.y + b.d) as f32 > spot.z + 1.0 && (b.y as f32) < spot.z + 9.0 && (b.x + b.w) as f32 > spot.x - 8.0 && (b.x as f32) < spot.x + 10.0
    };
    for b in &m.boxes {
        let (x, y, w, d, h) = (b.x as f32, b.y as f32, b.w as f32, b.d as f32, b.h);
        if in_the_way(b) {
            for c in &b.top {
                let i = batch(&c.sheet, false, &mut batches);
                flat(&mut batches[i], x + c.col as f32, y + c.row as f32 + c.z0.unwrap_or(0.0), 1.0, c.d.unwrap_or(1.0), 0.02, c.rect, size(&c.sheet));
            }
            continue;
        }
        let (zs, zn) = (y + d, y + b.inset);
        for c in &b.top {
            let i = batch(&c.sheet, false, &mut batches);
            flat(&mut batches[i], x + c.col as f32, y + c.row as f32 + c.z0.unwrap_or(0.0), 1.0, c.d.unwrap_or(1.0), h, c.rect, size(&c.sheet));
        }
        for c in &b.front {
            let top = h - c.row as f32;
            let cx = x + c.col as f32;
            let i = batch(&c.sheet, false, &mut batches);
            batches[i].quad([[cx, top, zs], [cx, top - 1.0, zs], [cx + 1.0, top - 1.0, zs], [cx + 1.0, top, zs]], SOUTH, c.rect, size(&c.sheet));
        }
        let i = batch(&b.side.sheet, false, &mut batches);
        let s = size(&b.side.sheet);
        batches[i].quad([[x + w, h, zs], [x + w, 0.0, zs], [x + w, 0.0, zn], [x + w, h, zn]], EAST, b.side.rect, s);
        batches[i].quad([[x, h, zn], [x, 0.0, zn], [x, 0.0, zs], [x, h, zs]], WEST, b.side.rect, s);
        let i = batch(&b.back.sheet, false, &mut batches);
        let s = size(&b.back.sheet);
        for col in 0..b.w {
            let (x1, x0) = (x + w - col as f32, x + w - col as f32 - 1.0);
            batches[i].quad([[x1, h, zn], [x1, 0.0, zn], [x0, 0.0, zn], [x0, h, zn]], NORTH, b.back.rect, s);
        }
    }
    for c in &m.cutouts {
        let i = batch(&c.sheet, true, &mut batches);
        let (x, z, w, h) = (c.x as f32, c.y as f32 + 0.5, c.w as f32, c.h as f32);
        batches[i].quad([[x, h, z], [x, 0.0, z], [x + w, 0.0, z], [x + w, h, z]], SOUTH, c.rect, size(&c.sheet));
    }
    batches
}
