//! Writing glTF (`.glb`) for Blender: a prop (its stand-in's parts, one material per
//! colour, or its own model with its textures; its light as a light object), every prop
//! laid out in one file, a building, or a whole map (the ground, the props and trees as
//! linked copies, the buildings). Same frame as the engine and as Blender's glTF files: one
//! unit is a cell (a metre), Y up, the front facing +Z. Never a camera: the game has one.

use std::collections::{BTreeMap, HashMap};

use content::{Building, Content, Light, Prop};
use glam::{Mat4, Quat, Vec3};
use serde_json::{json, Value};

use crate::buildings::building;
use crate::gltf_in::Image;
use crate::mesh::{ground, part_mesh, placement, side_color, Mesh, Models, Scene, Vertex};
use crate::trees::trees;
use crate::{color, hex_color, WorldMap};

#[derive(Default)]
pub struct Glb {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    nodes: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    /// KHR_lights_punctual
    lights: Vec<Value>,
    material_by: HashMap<String, usize>,
}

fn hex(c: [f32; 4]) -> String {
    hex_color([c[0], c[1], c[2]])
}

impl Glb {
    pub fn new() -> Glb {
        Glb::default()
    }

    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut v = json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.views.push(v);
        self.views.len() - 1
    }

    fn floats(&mut self, data: &[f32], kind: &str, size: usize, bounds: bool) -> usize {
        let view = self.view(bytemuck::cast_slice(data), Some(34962));
        let mut a = json!({ "bufferView": view, "componentType": 5126, "count": data.len() / size, "type": kind });
        if bounds {
            let (mut lo, mut hi) = (vec![f32::MAX; size], vec![f32::MIN; size]);
            for c in data.chunks(size) {
                for k in 0..size {
                    lo[k] = lo[k].min(c[k]);
                    hi[k] = hi[k].max(c[k]);
                }
            }
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn indices(&mut self, data: &[u32]) -> usize {
        let view = self.view(bytemuck::cast_slice(data), Some(34963));
        self.accessors.push(json!({ "bufferView": view, "componentType": 5125, "count": data.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }

    /// A flat material of one colour (linear), shared by colour.
    pub fn color_material(&mut self, c: [f32; 4]) -> usize {
        let name = hex(c);
        if let Some(&i) = self.material_by.get(&name) {
            return i;
        }
        self.materials.push(json!({ "name": name, "pbrMetallicRoughness": { "baseColorFactor": [c[0], c[1], c[2], c[3]], "metallicFactor": 0.0, "roughnessFactor": 0.9 } }));
        self.material_by.insert(name, self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// White: the vertex colours carry the colour.
    pub fn vertex_material(&mut self) -> usize {
        if let Some(&i) = self.material_by.get("vertex colours") {
            return i;
        }
        self.materials.push(json!({ "name": "vertex colours", "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.9 } }));
        self.material_by.insert("vertex colours".into(), self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// A material with a texture (the image written as PNG), shared by key.
    pub fn texture_material(&mut self, key: &str, img: &Image) -> usize {
        if let Some(&i) = self.material_by.get(key) {
            return i;
        }
        let mut png = vec![];
        {
            use image::ImageEncoder;
            let enc = image::codecs::png::PngEncoder::new(&mut png);
            let _ = enc.write_image(&img.rgba, img.width, img.height, image::ExtendedColorType::Rgba8);
        }
        let view = self.view(&png, None);
        self.images.push(json!({ "bufferView": view, "mimeType": "image/png", "name": key }));
        self.textures.push(json!({ "source": self.images.len() - 1 }));
        let name = key.rsplit('/').next().unwrap_or(key);
        self.materials.push(json!({ "name": name, "pbrMetallicRoughness": { "baseColorTexture": { "index": self.textures.len() - 1 }, "metallicFactor": 0.0, "roughnessFactor": 0.9 }, "alphaMode": "MASK" }));
        self.material_by.insert(key.to_string(), self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// A mesh from primitives (mesh, material, write vertex colours, write uvs); None if empty.
    pub fn mesh(&mut self, name: &str, prims: &[(&Mesh, usize, bool, bool)]) -> Option<usize> {
        let mut out = vec![];
        for &(m, material, colors, uvs) in prims {
            if m.indices.is_empty() {
                continue;
            }
            let pos: Vec<f32> = m.vertices.iter().flat_map(|v| v.pos).collect();
            let nor: Vec<f32> = m.vertices.iter().flat_map(|v| Vec3::from(v.normal).normalize_or(Vec3::Y).to_array()).collect();
            let mut attributes = json!({ "POSITION": self.floats(&pos, "VEC3", 3, true), "NORMAL": self.floats(&nor, "VEC3", 3, false) });
            if colors {
                let col: Vec<f32> = m.vertices.iter().flat_map(|v| v.color).collect();
                attributes["COLOR_0"] = json!(self.floats(&col, "VEC4", 4, false));
            }
            if uvs {
                let uv: Vec<f32> = m.vertices.iter().flat_map(|v| v.uv).collect();
                attributes["TEXCOORD_0"] = json!(self.floats(&uv, "VEC2", 2, false));
            }
            let indices = self.indices(&m.indices);
            out.push(json!({ "attributes": attributes, "indices": indices, "material": material }));
        }
        if out.is_empty() {
            return None;
        }
        self.meshes.push(json!({ "name": name, "primitives": out }));
        Some(self.meshes.len() - 1)
    }

    /// A scene's geometry as one mesh: vertex colours, plus textures where it has some.
    pub fn scene_mesh(&mut self, name: &str, scene: &Scene, images: &BTreeMap<String, Image>) -> Option<usize> {
        let white = self.vertex_material();
        let mut prims: Vec<(&Mesh, usize, bool, bool)> = vec![(&scene.plain, white, true, false)];
        for (key, mesh) in &scene.textured {
            let material = match images.get(key) {
                Some(img) => self.texture_material(key, img),
                None => white,
            };
            prims.push((mesh, material, true, true));
        }
        self.mesh(name, &prims)
    }

    /// A mesh of lines (only seen, never part of a model: a model's loader skips them).
    pub fn lines(&mut self, name: &str, segments: &[(Vec3, Vec3)]) -> Option<usize> {
        if segments.is_empty() {
            return None;
        }
        let pos: Vec<f32> = segments.iter().flat_map(|(a, b)| [a.x, a.y, a.z, b.x, b.y, b.z]).collect();
        let position = self.floats(&pos, "VEC3", 3, true);
        let idx: Vec<u32> = (0..segments.len() as u32 * 2).collect();
        let indices = self.indices(&idx);
        self.meshes.push(json!({ "name": name, "primitives": [{ "attributes": { "POSITION": position }, "indices": indices, "mode": 1 }] }));
        Some(self.meshes.len() - 1)
    }

    /// A node marked as an asset's bounds (Blender reads `extras` as custom properties).
    pub fn bounds_node(&mut self, name: &str, segments: &[(Vec3, Vec3)]) -> usize {
        let mesh = self.lines(name, segments);
        let i = self.node(name, mesh, Mat4::IDENTITY, vec![]);
        self.nodes[i]["extras"] = json!({ "bf_bounds": true });
        i
    }

    /// A light where a record puts it (`KHR_lights_punctual`, which Blender reads as a
    /// light object): a spot or a point, pointing down its node's -z. Blender's units are
    /// the record's backwards: 1000 W per intensity, written as they are (Blender's "Raw"
    /// lighting mode, what Blender 3.1 to 3.5 always does).
    pub fn light_node(&mut self, name: &str, light: &Light) -> usize {
        let c = color(&light.color);
        let candela = light.intensity as f32 * 1000.0;
        let mut l = json!({ "name": name, "type": light.kind, "color": [c[0], c[1], c[2]], "intensity": candela, "range": light.range });
        if light.kind == "spot" {
            let outer = (light.cone as f32 / 2.0).to_radians().clamp(0.01, std::f32::consts::FRAC_PI_2);
            l["spot"] = json!({ "innerConeAngle": outer * 0.66, "outerConeAngle": outer });
        }
        self.lights.push(l);
        let at = Vec3::new(light.at[0] as f32, light.at[1] as f32, light.at[2] as f32);
        let aim = Vec3::new(light.aim[0] as f32, light.aim[1] as f32, light.aim[2] as f32).normalize_or(Vec3::NEG_Y);
        let turn = Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::NEG_Z, aim), at);
        let i = self.node(name, None, turn, vec![]);
        self.nodes[i]["extensions"] = json!({ "KHR_lights_punctual": { "light": self.lights.len() - 1 } });
        i
    }

    pub fn node(&mut self, name: &str, mesh: Option<usize>, transform: Mat4, children: Vec<usize>) -> usize {
        let (s, r, t) = transform.to_scale_rotation_translation();
        let mut n = json!({ "name": name });
        if let Some(m) = mesh {
            n["mesh"] = json!(m);
        }
        if t != Vec3::ZERO {
            n["translation"] = json!(t.to_array());
        }
        if r != glam::Quat::IDENTITY {
            n["rotation"] = json!(r.to_array());
        }
        if s != Vec3::ONE {
            n["scale"] = json!(s.to_array());
        }
        if !children.is_empty() {
            n["children"] = json!(children);
        }
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// The file: these nodes as the scene.
    pub fn finish(mut self, roots: &[usize]) -> Vec<u8> {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut doc = json!({
            "asset": { "version": "2.0", "generator": "Battle Frontier" },
            "scene": 0,
            "scenes": [{ "nodes": roots }],
            "nodes": self.nodes,
            "meshes": self.meshes,
            "materials": self.materials,
            "accessors": self.accessors,
            "bufferViews": self.views,
            "buffers": [{ "byteLength": self.bin.len() }],
        });
        if !self.images.is_empty() {
            doc["images"] = json!(self.images);
            doc["textures"] = json!(self.textures);
        }
        if !self.lights.is_empty() {
            doc["extensionsUsed"] = json!(["KHR_lights_punctual"]);
            doc["extensions"] = json!({ "KHR_lights_punctual": { "lights": self.lights } });
        }
        let mut text = serde_json::to_vec(&doc).unwrap_or_default();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let total = 12 + 8 + text.len() + 8 + self.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&text);
        out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.bin);
        out
    }
}

/// A prop's mesh in the file: its own model if it has one loaded, else its stand-in's
/// parts, one primitive per part in the part's colour.
pub fn prop_mesh(glb: &mut Glb, prop: &Prop, models: &Models) -> Option<usize> {
    if let Some(m) = models.get(&prop.id) {
        // its skinned parts baked at rest: a rig doesn't go back out yet
        return glb.scene_mesh(&prop.id, &m.rest_scene(), &m.images);
    }
    let mut parts: Vec<(Mesh, usize)> = vec![];
    for (i, p) in prop.parts.iter().enumerate() {
        let mesh = part_mesh(p, 7 + i as u32 * 31 + prop.id.len() as u32);
        let at = Vec3::new(p.at[0] as f32, p.at[1] as f32, p.at[2] as f32);
        let mut placed = Mesh::default();
        placed.append(&mesh, Mat4::from_translation(at) * Mat4::from_rotation_y(-(p.turn as f32).to_radians()));
        let material = glb.color_material(color(&p.color));
        parts.push((placed, material));
    }
    let prims: Vec<(&Mesh, usize, bool, bool)> = parts.iter().map(|(m, mat)| (m, *mat, false, false)).collect();
    glb.mesh(&prop.id, &prims)
}

/// The cells a prop may cover and its height: its footprint's cells on the ground, the box
/// above them.
pub fn prop_bounds(prop: &Prop) -> Vec<(Vec3, Vec3)> {
    let (w, d) = (prop.footprint[0].max(1) as f32, prop.footprint[1].max(1) as f32);
    let h = (prop.height as f32).max(0.05);
    let (x0, z0) = (-w / 2.0, -d / 2.0);
    let mut out = vec![];
    for k in 0..=w as i32 {
        out.push((Vec3::new(x0 + k as f32, 0.0, z0), Vec3::new(x0 + k as f32, 0.0, z0 + d)));
    }
    for k in 0..=d as i32 {
        out.push((Vec3::new(x0, 0.0, z0 + k as f32), Vec3::new(x0 + w, 0.0, z0 + k as f32)));
    }
    let corners = [(x0, z0), (x0 + w, z0), (x0 + w, z0 + d), (x0, z0 + d)];
    for (i, &(x, z)) in corners.iter().enumerate() {
        let (nx, nz) = corners[(i + 1) % 4];
        out.push((Vec3::new(x, 0.0, z), Vec3::new(x, h, z)));
        out.push((Vec3::new(x, h, z), Vec3::new(nx, h, nz)));
    }
    out
}

/// A building's outline on the ground and at every floor, and its corners.
pub fn building_bounds(b: &Building, level_height: f32) -> Vec<(Vec3, Vec3)> {
    let base = b.level as f32 * level_height;
    let top = base + b.floors.len() as f32 * level_height;
    let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
    let mut out = vec![];
    for (x, z, side) in b.edges() {
        let (p, q) = content::segment_ends(x, z, side);
        for f in 0..=b.floors.len() {
            let y = base + f as f32 * level_height;
            out.push((Vec3::new(ox + p[0] as f32, y, oz + p[1] as f32), Vec3::new(ox + q[0] as f32, y, oz + q[1] as f32)));
        }
    }
    for c in b.corners() {
        out.push((Vec3::new(ox + c[0] as f32, base, oz + c[1] as f32), Vec3::new(ox + c[0] as f32, top, oz + c[1] as f32)));
    }
    out
}

/// A prop as a node: its mesh, its light under it, and what the add-on's "Add to the
/// project" fills in from it (`extras`, custom properties in Blender).
pub fn prop_node(glb: &mut Glb, prop: &Prop, models: &Models) -> usize {
    let mesh = prop_mesh(glb, prop, models);
    let children = prop.light.as_ref().map(|l| vec![glb.light_node(&format!("{} light", prop.id), l)]).unwrap_or_default();
    let i = glb.node(&prop.id, mesh, Mat4::IDENTITY, children);
    glb.nodes[i]["extras"] = json!({ "bf_prop": prop.id, "bf_name": prop.name, "bf_category": prop.category, "bf_blocks": prop.blocks, "bf_unstackable": prop.unstackable, "bf_sill": prop.sill });
    i
}

/// One prop, standing at the origin, with its bounds.
pub fn prop_glb(prop: &Prop, models: &Models) -> Vec<u8> {
    let mut glb = Glb::new();
    let root = prop_node(&mut glb, prop, models);
    let bounds = glb.bounds_node(&format!("{} bounds", prop.id), &prop_bounds(prop));
    glb.finish(&[root, bounds])
}

/// Every prop in one file: a row per category, the props along it a cell apart, each
/// standing on its own bounds (under a node named after the category).
pub fn props_glb(content: &Content, models: &Models) -> Vec<u8> {
    let mut glb = Glb::new();
    let mut by_category: BTreeMap<&str, Vec<&Prop>> = BTreeMap::new();
    for p in content.props.iter() {
        by_category.entry(p.category.as_str()).or_default().push(p);
    }
    let mut rows = vec![];
    let mut z = 0.0f32;
    for (category, props) in by_category {
        let depth = props.iter().map(|p| p.footprint[1].max(1) as f32).fold(1.0, f32::max);
        let mut x = 0.0f32;
        let mut nodes = vec![];
        for p in props {
            let (w, d) = (p.footprint[0].max(1) as f32, p.footprint[1].max(1) as f32);
            let at = Mat4::from_translation(Vec3::new(x + w / 2.0, 0.0, z + d / 2.0));
            let prop = prop_node(&mut glb, p, models);
            let bounds = glb.bounds_node(&format!("{} bounds", p.id), &prop_bounds(p));
            nodes.push(glb.node(&format!("{} ", p.id), None, at, vec![prop, bounds]));
            x += w + 1.0;
        }
        rows.push(glb.node(category, None, Mat4::IDENTITY, nodes));
        z += depth + 2.0;
    }
    let root = glb.node("Props", None, Mat4::IDENTITY, rows);
    glb.finish(&[root])
}

fn all_images(models: &Models) -> BTreeMap<String, Image> {
    models.values().flat_map(|m| m.images.iter().map(|(k, v)| (k.clone(), v.clone()))).collect()
}

/// One building, where it stands on its map.
pub fn building_glb(b: &Building, content: &Content, level_height: f32, models: &Models) -> Vec<u8> {
    let mut glb = Glb::new();
    let scene = building(b, content, level_height, None, models);
    let mesh = glb.scene_mesh(&b.id, &scene, &all_images(models));
    let root = glb.node(&b.name, mesh, Mat4::IDENTITY, vec![]);
    let bounds = glb.bounds_node(&format!("{} bounds", b.name), &building_bounds(b, level_height));
    glb.finish(&[root, bounds])
}

/// A whole map: the ground (cell tops in their terrain's colour, cliffs, tall grass), the
/// props and trees (linked copies of one mesh per prop), the buildings.
pub fn map_glb(map: &WorldMap, content: &Content, level_height: f32, models: &Models) -> Vec<u8> {
    let mut glb = Glb::new();
    let images = all_images(models);
    let g = ground(map, content, level_height);
    // the terrain shader's colours, as vertex colours: the top's down to the bevels' foot,
    // where the rock takes over; the faces the rock's, darker at the foot
    let terrain = |v: &Vertex| map.terrain_at(v.uv[0].floor() as i32, v.uv[1].floor() as i32).and_then(|id| content.terrains.get(id));
    let mut tops = g.tops.clone();
    for v in tops.vertices.iter_mut() {
        let t = terrain(v);
        let (top, side, k) = (color(t.map(|t| t.color.as_str()).unwrap_or("#6cbb4e")), side_color(t), v.color[3]);
        v.color = [0, 1, 2, 3].map(|i| if i == 3 { 1.0 } else { top[i] + (side[i] - top[i]) * k });
    }
    let mut cliffs = g.cliffs.clone();
    for v in cliffs.vertices.iter_mut() {
        let side = side_color(terrain(v));
        v.color = [side[0] * v.color[0], side[1] * v.color[1], side[2] * v.color[2], 1.0];
    }
    let white = glb.vertex_material();
    let ground_mesh = glb.mesh("ground", &[(&tops, white, true, false), (&cliffs, white, true, false), (&g.grass, white, true, false)]);
    let ground_node = glb.node("Ground", ground_mesh, Mat4::IDENTITY, vec![]);
    let mut prop_meshes: HashMap<String, Option<usize>> = HashMap::new();
    let mut mesh_of = |glb: &mut Glb, id: &str| -> Option<usize> {
        if let Some(m) = prop_meshes.get(id) {
            return *m;
        }
        let m = content.props.get(id).and_then(|p| prop_mesh(glb, p, models));
        prop_meshes.insert(id.to_string(), m);
        m
    };
    let mut props = vec![];
    for (i, p) in map.props.iter().enumerate() {
        let Some(prop) = content.props.get(&p.prop) else { continue };
        let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
        let pos = p.pos();
        let at = Vec3::new(pos[0] as f32, p.level as f32 * level_height, pos[1] as f32);
        let mesh = mesh_of(&mut glb, &p.prop);
        props.push(glb.node(&format!("{} {}", p.prop, i + 1), mesh, placement(at, p.turn as f32, scale), vec![]));
    }
    let props_node = glb.node("Props", None, Mat4::IDENTITY, props);
    let mut tree_nodes = vec![];
    for (i, t) in trees(map, level_height).iter().enumerate() {
        let mesh = mesh_of(&mut glb, &t.prop);
        tree_nodes.push(glb.node(&format!("{} {}", t.prop, i + 1), mesh, placement(Vec3::from(t.pos), t.turn, t.scale), vec![]));
    }
    let trees_node = glb.node("Trees", None, Mat4::IDENTITY, tree_nodes);
    let mut houses = vec![];
    for b in &map.buildings {
        let scene = building(b, content, level_height, None, models);
        let mesh = glb.scene_mesh(&b.id, &scene, &images);
        houses.push(glb.node(&b.name, mesh, Mat4::IDENTITY, vec![]));
    }
    let houses_node = glb.node("Buildings", None, Mat4::IDENTITY, houses);
    let root = glb.node(&map.name, None, Mat4::IDENTITY, vec![ground_node, props_node, trees_node, houses_node]);
    glb.finish(&[root])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_in::{light_record, load_model};
    use content::Part;

    fn prop() -> Prop {
        let part = |shape: &str, size: [f64; 3], at: [f64; 3], color: &str| Part { shape: shape.into(), size, at, color: color.into(), turn: 0.0 };
        Prop {
            id: "lamp".into(),
            name: "Lamp".into(),
            category: "decoration".into(),
            footprint: [1, 1],
            height: 2.4,
            parts: vec![part("cylinder", [0.2, 2.0, 0.2], [0.0, 0.0, 0.0], "#3c3f46"), part("sphere", [0.4, 0.4, 0.4], [0.0, 2.0, 0.0], "#ffe9a8")],
            light: Some(Light { color: "#ffe9a8".into(), intensity: 1.5, range: 7.0, cone: 120.0, at: [0.0, 2.0, 0.3], aim: [0.0, -1.0, 1.0], ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn a_prop_goes_out_and_comes_back() {
        let bytes = prop_glb(&prop(), &Models::new());
        let (doc, _, _) = gltf::import_slice(&bytes).expect("a valid glb");
        let mesh = doc.meshes().next().unwrap();
        assert_eq!(mesh.primitives().count(), 2, "a primitive per part");
        let bounds = doc.nodes().find(|n| n.name() == Some("lamp bounds")).expect("its bounds");
        assert_eq!(bounds.mesh().unwrap().primitives().next().unwrap().mode(), gltf::mesh::Mode::Lines);
        let colours: Vec<String> = doc.materials().filter_map(|m| m.name().map(String::from)).collect();
        assert_eq!(colours, vec!["#3c3f46", "#ffe9a8"]);
        assert_eq!(doc.cameras().count(), 0, "never a camera");
        let model = load_model(&bytes, "lamp.glb").unwrap();
        assert!((model.max.y - 2.4).abs() < 0.02, "as tall as the prop: {}", model.max.y);
        assert!(model.min.y.abs() < 0.01, "standing on the ground: {}", model.min.y);
        assert!(model.scene.textured.is_empty());
        // the sphere comes back in its colour
        let lit = color("#ffe9a8");
        assert!(model.scene.plain.vertices.iter().any(|v| (v.color[0] - lit[0]).abs() < 1e-4 && (v.color[2] - lit[2]).abs() < 1e-4));
        // and its light, as the record had it: where it sits, where it points, how bright
        assert_eq!(model.lights.len(), 1);
        let l = &model.lights[0];
        assert_eq!((l.kind.as_str(), l.color.as_str(), l.intensity, l.range, l.cone), ("spot", "#ffe9a8", 1.5, 7.0, 120.0));
        assert_eq!(l.at, [0.0, 2.0, 0.3]);
        assert_eq!(l.aim, [0.0, -0.71, 0.71]);
        assert!(l.night, "lit by night, as a prop's lights are unless told otherwise");
        assert_eq!(model.cameras, 0);
    }

    #[test]
    fn blenders_physical_units_come_back_as_well() {
        // Blender 3.6+ writes physical units unless told otherwise: a 1000 W lamp is 54,351
        // candela there (683 lm/W over 4 pi steradians)
        let physical = light_record("point", [1.0, 1.0, 1.0], 1000.0 * 683.0 / (4.0 * std::f32::consts::PI), None, None, Vec3::Y, Vec3::NEG_Y);
        assert_eq!((physical.intensity, physical.kind.as_str(), physical.cone, physical.range), (1.0, "point", 180.0, 6.0));
        // Blender 3.1 (and "Raw" later): the watts as they are
        let raw = light_record("spot", [1.0, 0.5, 0.0], 500.0, Some(9.0), Some(0.5), Vec3::ZERO, Vec3::X);
        assert_eq!((raw.intensity, raw.range, raw.cone, raw.aim), (0.5, 9.0, 57.3, [1.0, 0.0, 0.0]));
    }

    #[test]
    fn every_prop_in_one_file() {
        let mut content = Content::default();
        content.props.put(prop(), content::Space::Content);
        content.props.put(Prop { id: "bench".into(), name: "Bench".into(), category: "furniture".into(), footprint: [2, 1], height: 0.9, ..Default::default() }, content::Space::Content);
        content.props.put(Prop { id: "rock".into(), name: "Rock".into(), category: "rock".into(), footprint: [1, 1], height: 0.6, ..Default::default() }, content::Space::Content);
        let bytes = props_glb(&content, &Models::new());
        let (doc, _, _) = gltf::import_slice(&bytes).expect("a valid glb");
        let names: Vec<&str> = doc.nodes().filter_map(|n| n.name()).collect();
        for wanted in ["Props", "decoration", "furniture", "rock", "lamp", "bench", "rock bounds", "lamp light"] {
            assert!(names.contains(&wanted), "{wanted} in {names:?}");
        }
        // a row per category, the props along it a cell apart
        let row_of = |name: &str| {
            let m = doc.nodes().find(|n| n.name() == Some(name)).unwrap().transform().matrix();
            [m[3][0], m[3][1], m[3][2]]
        };
        let (lamp, bench, rock) = (row_of("lamp "), row_of("bench "), row_of("rock "));
        assert_eq!(lamp, [0.5, 0.0, 0.5]);
        assert_eq!(bench, [1.0, 0.0, 3.5]);
        assert_eq!(rock, [0.5, 0.0, 6.5]);
        // Blender's "Add to the project" finds the prop's facts on its node
        let lamp_node = doc.nodes().find(|n| n.name() == Some("lamp")).unwrap();
        assert!(lamp_node.extras().as_ref().map(|e| e.get().contains("\"bf_prop\":\"lamp\"")).unwrap_or(false));
        assert_eq!(doc.lights().map(|l| l.count()).unwrap_or(0), 1);
    }

    #[test]
    fn textures_go_out_and_come_back() {
        let mut model = crate::gltf_in::Model::default();
        let mut quad = Mesh::default();
        for (p, uv) in [([0.0, 0.0, 0.0], [0.0, 1.0]), ([1.0, 0.0, 0.0], [1.0, 1.0]), ([1.0, 1.0, 0.0], [1.0, 0.0])] {
            quad.vertices.push(crate::mesh::Vertex { pos: p, normal: [0.0, 0.0, 1.0], uv, color: [1.0; 4], toon: 0.0 });
        }
        quad.indices = vec![0, 1, 2];
        model.scene.textured.insert("sign.glb#0".into(), quad);
        model.images.insert("sign.glb#0".into(), Image { rgba: vec![200, 30, 30, 255, 30, 200, 30, 255, 30, 30, 200, 255, 250, 250, 250, 255], width: 2, height: 2 });
        let mut models = Models::new();
        models.insert("sign".into(), model);
        let sign = Prop { id: "sign".into(), footprint: [1, 1], ..Default::default() };
        let bytes = prop_glb(&sign, &models);
        let back = load_model(&bytes, "again.glb").unwrap();
        assert_eq!(back.scene.textured.len(), 1);
        let img = back.images.values().next().unwrap();
        assert_eq!((img.width, img.height, &img.rgba[..4]), (2, 2, &[200, 30, 30, 255][..]));
    }
}
