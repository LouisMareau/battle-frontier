//! Abilities' FX: a few simple meshes (a flat quad, a sphere, a dome, a ring; billboards are
//! the renderer's) with UVs over 0..1, so a flipbook's frame maps onto them; and the sheets
//! the frames are drawn on: a blank one, where a frame lies, an export at a smaller size.

use std::f32::consts::{PI, TAU};

use glam::Vec3;

use crate::mesh::{Mesh, Vertex};

fn vertex(m: &mut Mesh, pos: Vec3, normal: Vec3, uv: [f32; 2]) -> u32 {
    m.vertices.push(Vertex { pos: pos.to_array(), normal: normal.normalize_or(Vec3::Y).to_array(), uv, color: [1.0; 4], toon: 0.0, wind: [0; 4] });
    m.vertices.len() as u32 - 1
}

/// A flat quad `w` by `h`, centred, facing +z (u to the right, v down).
pub fn quad(w: f32, h: f32) -> Mesh {
    let mut m = Mesh::default();
    let (x, y) = (w / 2.0, h / 2.0);
    let i = vertex(&mut m, Vec3::new(-x, y, 0.0), Vec3::Z, [0.0, 0.0]);
    vertex(&mut m, Vec3::new(-x, -y, 0.0), Vec3::Z, [0.0, 1.0]);
    vertex(&mut m, Vec3::new(x, -y, 0.0), Vec3::Z, [1.0, 1.0]);
    vertex(&mut m, Vec3::new(x, y, 0.0), Vec3::Z, [1.0, 0.0]);
    m.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    m
}

/// An ellipsoid centred on the origin (`size` is its full extent), the frame wrapped round
/// it: u round the axis, v from the top (0) to the bottom (1). `top_only`: the upper half,
/// open underneath (a dome), v from the top to the rim.
fn ball(size: Vec3, top_only: bool) -> Mesh {
    let (rings, segments) = (8usize, 16usize);
    let r = size * 0.5;
    let rows = if top_only { rings / 2 } else { rings };
    let mut m = Mesh::default();
    let base = m.vertices.len() as u32;
    for i in 0..=rows {
        let v = i as f32 / rings as f32 * PI;
        for k in 0..=segments {
            let a = k as f32 / segments as f32 * TAU;
            let unit = Vec3::new(v.sin() * a.sin(), v.cos(), v.sin() * a.cos());
            let n = Vec3::new(unit.x / r.x.max(0.01), unit.y / r.y.max(0.01), unit.z / r.z.max(0.01));
            vertex(&mut m, unit * r, n, [k as f32 / segments as f32, i as f32 / rows as f32]);
        }
    }
    let row = segments as u32 + 1;
    for i in 0..rows as u32 {
        for k in 0..segments as u32 {
            let (a, b, c, d) = (base + i * row + k, base + (i + 1) * row + k, base + (i + 1) * row + k + 1, base + i * row + k + 1);
            m.indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    m
}

pub fn sphere(size: Vec3) -> Mesh {
    ball(size, false)
}

pub fn dome(size: Vec3) -> Mesh {
    ball(size, true)
}

/// A flat ring on the ground (facing up), `size.x` by `size.z` across, its hole `inner` of
/// that: u round it, v from the hole (0) to the rim (1).
pub fn ring(size: Vec3, inner: f32) -> Mesh {
    let segments = 24usize;
    let (rx, rz) = (size.x / 2.0, size.z / 2.0);
    let mut m = Mesh::default();
    let base = m.vertices.len() as u32;
    for k in 0..=segments {
        let a = k as f32 / segments as f32 * TAU;
        let u = k as f32 / segments as f32;
        vertex(&mut m, Vec3::new(a.cos() * rx * inner, 0.0, a.sin() * rz * inner), Vec3::Y, [u, 0.0]);
        vertex(&mut m, Vec3::new(a.cos() * rx, 0.0, a.sin() * rz), Vec3::Y, [u, 1.0]);
    }
    for k in 0..segments as u32 {
        let (a, b, c, d) = (base + k * 2, base + k * 2 + 1, base + k * 2 + 3, base + k * 2 + 2);
        m.indices.extend_from_slice(&[a, c, b, a, d, c]);
    }
    m
}

/// A shape by its name (a billboard is drawn by the renderer: a quad here).
pub fn shape(kind: &str, size: Vec3) -> Mesh {
    match kind {
        "sphere" => sphere(size),
        "dome" => dome(size),
        "ring" => ring(size, 0.6),
        _ => quad(size.x, size.y),
    }
}

/// A sheet's size in pixels: `frames` frames `size` px square, `columns` across.
pub fn sheet_size(frames: u8, columns: u8, size: u16) -> (u32, u32) {
    let columns = columns.max(1) as u32;
    let rows = (frames.max(1) as u32).div_ceil(columns);
    (columns * size as u32, rows * size as u32)
}

/// Where frame `k` lies on the sheet, in pixels (x, y).
pub fn frame_at(k: u8, columns: u8, size: u16) -> (u32, u32) {
    let columns = columns.max(1) as u32;
    ((k as u32 % columns) * size as u32, (k as u32 / columns) * size as u32)
}

/// Frame `k`'s box on the sheet, in UVs: u0, v0, u1, v1.
pub fn frame_uv(k: u8, frames: u8, columns: u8, size: u16) -> [f32; 4] {
    let (w, h) = sheet_size(frames, columns, size);
    let (x, y) = frame_at(k, columns, size);
    let s = size as f32;
    [x as f32 / w as f32, y as f32 / h as f32, (x as f32 + s) / w as f32, (y as f32 + s) / h as f32]
}

/// A mesh's UVs (0..1) squeezed into a frame's box.
pub fn into_frame(mesh: &mut Mesh, uv: [f32; 4]) {
    for v in &mut mesh.vertices {
        v.uv = [uv[0] + (uv[2] - uv[0]) * v.uv[0], uv[1] + (uv[3] - uv[1]) * v.uv[1]];
    }
}

/// A blank sheet: transparent.
pub fn blank(frames: u8, columns: u8, size: u16) -> (u32, u32, Vec<u8>) {
    let (w, h) = sheet_size(frames, columns, size);
    (w, h, vec![0; (w * h * 4) as usize])
}

/// A sheet's frames at another size (each pixel the mean of the box it stands for), packed
/// the same way.
pub fn export(rgba: &[u8], width: u32, frames: u8, columns: u8, size: u16, out: u16) -> (u32, u32, Vec<u8>) {
    let (ow, oh) = sheet_size(frames, columns, out);
    let mut data = vec![0u8; (ow * oh * 4) as usize];
    let scale = size as f32 / out as f32;
    for k in 0..frames {
        let (sx, sy) = frame_at(k, columns, size);
        let (dx, dy) = frame_at(k, columns, out);
        for y in 0..out as u32 {
            for x in 0..out as u32 {
                // the box of source pixels this one stands for
                let x0 = (x as f32 * scale) as u32;
                let x1 = (((x + 1) as f32 * scale).ceil() as u32).max(x0 + 1);
                let y0 = (y as f32 * scale) as u32;
                let y1 = (((y + 1) as f32 * scale).ceil() as u32).max(y0 + 1);
                let (mut sum, mut n) = ([0u32; 4], 0u32);
                for yy in y0..y1.min(size as u32) {
                    for xx in x0..x1.min(size as u32) {
                        let i = (((sy + yy) * width + sx + xx) * 4) as usize;
                        if let Some(px) = rgba.get(i..i + 4) {
                            // colours weighted by alpha, so clear pixels don't darken edges
                            let a = px[3] as u32;
                            sum[0] += px[0] as u32 * a;
                            sum[1] += px[1] as u32 * a;
                            sum[2] += px[2] as u32 * a;
                            sum[3] += a;
                            n += 1;
                        }
                    }
                }
                let o = (((dy + y) * ow + dx + x) * 4) as usize;
                if n > 0 && sum[3] > 0 {
                    data[o] = (sum[0] / sum[3]) as u8;
                    data[o + 1] = (sum[1] / sum[3]) as u8;
                    data[o + 2] = (sum[2] / sum[3]) as u8;
                    data[o + 3] = (sum[3] / n) as u8;
                }
            }
        }
    }
    (ow, oh, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_lie_on_the_sheet_in_rows() {
        assert_eq!(sheet_size(8, 4, 128), (512, 256));
        assert_eq!(frame_at(5, 4, 128), (128, 128));
        let uv = frame_uv(5, 8, 4, 128);
        assert!((uv[0] - 0.25).abs() < 1e-6 && (uv[1] - 0.5).abs() < 1e-6 && (uv[2] - 0.5).abs() < 1e-6 && (uv[3] - 1.0).abs() < 1e-6, "{uv:?}");
        let mut q = quad(1.0, 1.0);
        into_frame(&mut q, uv);
        assert_eq!(q.vertices[2].uv, [0.5, 1.0]);
    }

    #[test]
    fn an_export_keeps_what_was_drawn_smaller() {
        let (w, h, mut rgba) = blank(2, 2, 4);
        assert_eq!((w, h), (8, 4));
        // a solid 2 x 2 square at the top left of frame 1
        for y in 0..2 {
            for x in 4..6 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[200, 100, 50, 255]);
            }
        }
        let (ow, oh, out) = export(&rgba, w, 2, 2, 4, 2);
        assert_eq!((ow, oh), (4, 2));
        // frame 1 is at x 2..4: its top-left pixel is the square, the rest clear
        let px = |x: u32, y: u32| &out[((y * ow + x) * 4) as usize..((y * ow + x) * 4 + 4) as usize];
        assert_eq!(px(2, 0), &[200, 100, 50, 255]);
        assert_eq!(px(3, 1)[3], 0);
        assert_eq!(px(0, 0)[3], 0);
    }

    #[test]
    fn shapes_have_uvs_within_the_frame() {
        for kind in ["quad", "sphere", "dome", "ring"] {
            let m = shape(kind, Vec3::new(1.0, 1.0, 1.0));
            assert!(!m.indices.is_empty(), "{kind}");
            assert!(m.vertices.iter().all(|v| (0.0..=1.0).contains(&v.uv[0]) && (0.0..=1.0).contains(&v.uv[1])), "{kind}");
        }
        // the dome stands on the ground: nothing below it
        assert!(dome(Vec3::ONE).vertices.iter().all(|v| v.pos[1] >= -1e-6));
    }
}
