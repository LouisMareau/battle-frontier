//! Props' real models: glTF files (`.glb`, as Blender exports them) turned into the
//! engine's meshes. One unit is one cell (a metre in Blender), Y is up and the front faces
//! +Z (Blender's front view), the origin is the centre of the footprint on the ground.
//! Colours come from the materials' base colour times the vertex colours; textured parts
//! are kept apart by texture, named "<key>#<image>". The file's lights
//! (`KHR_lights_punctual`, Blender's spot and point lights) come out as a prop's `light`
//! would hold them. A rig (a skin) and the animations come out as `anim::Rig` and
//! `anim::Animation`, and the parts skinned to it apart from the rest (`skinned`), with
//! their joints and weights, for the renderer to move. Cameras are dropped (the game has
//! one camera of its own); the rest of the materials stays in the file, unread for now.

use std::collections::BTreeMap;

use content::Light;
use glam::{Mat3, Mat4, Vec3};

use crate::anim::{Animation, Channel, Node, Path, Rig};
use crate::mesh::{Mesh, Scene, Vertex};

/// An image a model uses, decoded (RGBA8, sRGB).
#[derive(Clone, Debug)]
pub struct Image {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A part of a model skinned to its rig: its vertices as the file has them (the joints'
/// matrices place them), each with up to four joints and their weights.
#[derive(Clone, Debug, Default)]
pub struct Skinned {
    pub mesh: Mesh,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// its texture key, when it has one
    pub texture: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    /// the parts that don't move
    pub scene: Scene,
    /// the parts that move with the rig
    pub skinned: Vec<Skinned>,
    pub rig: Option<Rig>,
    pub animations: Vec<Animation>,
    /// texture key -> image
    pub images: BTreeMap<String, Image>,
    pub min: Vec3,
    pub max: Vec3,
    /// its lights (spots and points), where they sit and point in the model's frame
    pub lights: Vec<Light>,
    /// cameras in the file, dropped
    pub cameras: usize,
}

impl Model {
    /// Everything, standing still: the skinned parts baked at rest with the rest.
    pub fn rest_scene(&self) -> Scene {
        let mut out = self.scene.clone();
        let Some(rig) = &self.rig else { return out };
        let pose = crate::anim::pose(rig, None, 0.0);
        for part in &self.skinned {
            let mut mesh = part.mesh.clone();
            for (i, v) in mesh.vertices.iter_mut().enumerate() {
                let (j, w) = (part.joints.get(i).copied().unwrap_or([0; 4]), part.weights.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]));
                v.pos = crate::anim::skin_point(Vec3::from(v.pos), j, w, &pose).to_array();
                let p = Vec3::from(v.pos);
                let n = crate::anim::skin_point(p + Vec3::from(v.normal), j, w, &pose) - crate::anim::skin_point(p, j, w, &pose);
                v.normal = n.normalize_or(Vec3::Y).to_array();
            }
            match &part.texture {
                Some(k) => out.textured.entry(k.clone()).or_default().append(&mesh, Mat4::IDENTITY),
                None => out.plain.append(&mesh, Mat4::IDENTITY),
            }
        }
        out
    }

    /// The animation to play when nothing says which: one called "idle", else the first.
    pub fn idle(&self) -> Option<&Animation> {
        self.animations.iter().find(|a| a.name.eq_ignore_ascii_case("idle")).or(self.animations.first())
    }
}

/// Blender's own units through glTF: a light's watts in Blender go into the file as they
/// are (its "Raw" lighting mode, and all Blender 3.1 to 3.5 does), and 1000 W is an
/// intensity of 1 here.
const WATTS_PER_INTENSITY: f32 = 1000.0;
/// Blender 3.6 and newer export physical units unless told otherwise: candela = watts x
/// 683 lumens per watt / 4 pi steradians, 54 times as much. An intensity past 10 (ten
/// thousand watts) can only be that, and is brought back.
const PHYSICAL_CANDELA_PER_WATT: f32 = 683.0 / (4.0 * std::f32::consts::PI);

/// A glTF light as a record: `candela` as the file has it, `cone` a spot's outer cone (its
/// half angle, radians), none for a point light.
pub fn light_record(kind: &str, color: [f32; 3], candela: f32, range: Option<f32>, cone: Option<f32>, at: Vec3, aim: Vec3) -> Light {
    let mut intensity = candela / WATTS_PER_INTENSITY;
    if intensity > 10.0 {
        intensity /= PHYSICAL_CANDELA_PER_WATT;
    }
    let round = |v: f32| (v as f64 * 100.0).round() / 100.0;
    Light {
        kind: kind.into(),
        color: crate::hex_color(color),
        intensity: round(intensity),
        range: round(range.filter(|r| *r > 0.0).unwrap_or(Light::default().range as f32)),
        cone: cone.map(|c| round((c * 2.0).to_degrees().clamp(1.0, 180.0))).unwrap_or(180.0),
        at: at.to_array().map(round),
        aim: aim.normalize_or(Vec3::NEG_Y).to_array().map(round),
        ..Default::default()
    }
}

fn rgba8(data: &gltf::image::Data) -> Option<Image> {
    use gltf::image::Format;
    let px = &data.pixels;
    let rgba: Vec<u8> = match data.format {
        Format::R8G8B8A8 => px.clone(),
        Format::R8G8B8 => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        Format::R8G8 => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        Format::R8 => px.iter().flat_map(|&c| [c, c, c, 255]).collect(),
        _ => return None,
    };
    Some(Image { rgba, width: data.width, height: data.height })
}

/// Flat normals from the triangles, for a mesh that came without.
fn flat_normals(out: &mut Mesh) {
    for t in out.indices.chunks(3) {
        if t.len() < 3 {
            break;
        }
        let (a, b, c) = (Vec3::from(out.vertices[t[0] as usize].pos), Vec3::from(out.vertices[t[1] as usize].pos), Vec3::from(out.vertices[t[2] as usize].pos));
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y).to_array();
        for &i in t {
            out.vertices[i as usize].normal = n;
        }
    }
}

/// The file's nodes as a rig's, and the order parents come before children.
fn rig_nodes(doc: &gltf::Document) -> (Vec<Node>, Vec<usize>) {
    let mut nodes: Vec<Node> = doc
        .nodes()
        .map(|n| {
            let (s, r, t) = Mat4::from_cols_array_2d(&n.transform().matrix()).to_scale_rotation_translation();
            Node { parent: None, translation: t, rotation: r, scale: s }
        })
        .collect();
    for n in doc.nodes() {
        for c in n.children() {
            if let Some(child) = nodes.get_mut(c.index()) {
                child.parent = Some(n.index());
            }
        }
    }
    let mut order = vec![];
    let mut stack: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        let mut kids: Vec<usize> = doc.nodes().nth(i).map(|n| n.children().map(|c| c.index()).collect()).unwrap_or_default();
        kids.reverse();
        stack.extend(kids);
    }
    (nodes, order)
}

/// Reads a `.glb` (or a `.gltf` with everything embedded). `key` names its textures.
pub fn load_model(bytes: &[u8], key: &str) -> Result<Model, String> {
    let (doc, buffers, images) = gltf::import_slice(bytes).map_err(|e| e.to_string())?;
    let mut model = Model { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN), ..Default::default() };
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("the file has no scene")?;
    model.cameras = doc.cameras().count();
    // the rig: the first skin's joints over the file's nodes
    if let Some(skin) = doc.skins().next() {
        let (nodes, order) = rig_nodes(&doc);
        let reader = skin.reader(|b| Some(&buffers[b.index()]));
        let ibms: Vec<Mat4> = reader.read_inverse_bind_matrices().map(|it| it.map(|m| Mat4::from_cols_array_2d(&m)).collect()).unwrap_or_default();
        let joints: Vec<(usize, Mat4)> = skin.joints().enumerate().map(|(k, j)| (j.index(), ibms.get(k).copied().unwrap_or(Mat4::IDENTITY))).collect();
        model.rig = Some(Rig { nodes, joints, order });
    }
    for a in doc.animations() {
        let mut out = Animation { name: a.name().unwrap_or("").to_string(), duration: 0.0, channels: vec![] };
        for ch in a.channels() {
            let reader = ch.reader(|b| Some(&buffers[b.index()]));
            let Some(times) = reader.read_inputs() else { continue };
            let times: Vec<f32> = times.collect();
            let cubic = ch.sampler().interpolation() == gltf::animation::Interpolation::CubicSpline;
            let step = ch.sampler().interpolation() == gltf::animation::Interpolation::Step;
            let (path, mut values): (Path, Vec<[f32; 4]>) = match reader.read_outputs() {
                Some(gltf::animation::util::ReadOutputs::Translations(it)) => (Path::Translation, it.map(|v| [v[0], v[1], v[2], 0.0]).collect()),
                Some(gltf::animation::util::ReadOutputs::Rotations(it)) => (Path::Rotation, it.into_f32().collect()),
                Some(gltf::animation::util::ReadOutputs::Scales(it)) => (Path::Scale, it.map(|v| [v[0], v[1], v[2], 0.0]).collect()),
                _ => continue, // morph targets: not yet
            };
            if cubic {
                // in-tangent, value, out-tangent per key: the values alone
                values = values.chunks(3).filter_map(|c| c.get(1).copied()).collect();
            }
            if let Some(&last) = times.last() {
                out.duration = out.duration.max(last);
            }
            out.channels.push(Channel { node: ch.target().node().index(), path, times, values, step });
        }
        model.animations.push(out);
    }
    let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();
    while let Some((node, parent)) = stack.pop() {
        let m = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        let normal_m = Mat3::from_mat4(m).inverse().transpose();
        if let Some(light) = node.light() {
            use gltf::khr_lights_punctual::Kind;
            // a light points down its node's -z
            let (at, aim) = (m.transform_point3(Vec3::ZERO), m.transform_vector3(Vec3::NEG_Z));
            match light.kind() {
                Kind::Spot { outer_cone_angle, .. } => model.lights.push(light_record("spot", light.color(), light.intensity(), light.range(), Some(outer_cone_angle), at, aim)),
                Kind::Point => model.lights.push(light_record("point", light.color(), light.intensity(), light.range(), None, at, aim)),
                Kind::Directional => {} // the sun is the world's
            }
        }
        // a skinned mesh's vertices are placed by its joints, not by its node
        let skinned = node.skin().is_some() && model.rig.is_some();
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(positions) = reader.read_positions() else { continue };
                let place = if skinned { Mat4::IDENTITY } else { m };
                let positions: Vec<Vec3> = positions.map(|p| place.transform_point3(Vec3::from(p))).collect();
                let normals: Option<Vec<Vec3>> = reader.read_normals().map(|n| n.map(|n| (if skinned { Vec3::from(n) } else { normal_m * Vec3::from(n) }).normalize_or(Vec3::Y)).collect());
                let uvs: Option<Vec<[f32; 2]>> = reader.read_tex_coords(0).map(|t| t.into_f32().collect());
                let colors: Option<Vec<[f32; 4]>> = reader.read_colors(0).map(|c| c.into_rgba_f32().collect());
                let indices: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..positions.len() as u32).collect(),
                };
                let pbr = prim.material().pbr_metallic_roughness();
                let factor = pbr.base_color_factor();
                let texture = pbr.base_color_texture().map(|t| t.texture().source().index());
                let mut out = Mesh::default();
                for (i, p) in positions.iter().enumerate() {
                    let c = colors.as_ref().and_then(|c| c.get(i)).copied().unwrap_or([1.0; 4]);
                    out.vertices.push(Vertex {
                        pos: p.to_array(),
                        normal: normals.as_ref().and_then(|n| n.get(i)).copied().unwrap_or(Vec3::Y).to_array(),
                        uv: uvs.as_ref().and_then(|u| u.get(i)).copied().unwrap_or([0.5, 0.5]),
                        color: [factor[0] * c[0], factor[1] * c[1], factor[2] * c[2], factor[3] * c[3]],
                        toon: 0.0,
                    });
                }
                out.indices = indices.into_iter().filter(|&i| (i as usize) < positions.len()).collect();
                if normals.is_none() {
                    flat_normals(&mut out);
                }
                let texture_key = texture.and_then(|i| images.get(i).map(|d| (i, d))).map(|(i, data)| {
                    let k = format!("{key}#{i}");
                    if !model.images.contains_key(&k) {
                        if let Some(img) = rgba8(data) {
                            model.images.insert(k.clone(), img);
                        }
                    }
                    k
                });
                let joints: Option<Vec<[u16; 4]>> = if skinned { reader.read_joints(0).map(|j| j.into_u16().collect()) } else { None };
                let weights: Option<Vec<[f32; 4]>> = if skinned { reader.read_weights(0).map(|w| w.into_f32().collect()) } else { None };
                match (joints, weights) {
                    (Some(joints), Some(weights)) => {
                        // its bounds at rest
                        let pose = crate::anim::pose(model.rig.as_ref().unwrap(), None, 0.0);
                        for (i, v) in out.vertices.iter().enumerate() {
                            let p = crate::anim::skin_point(Vec3::from(v.pos), joints.get(i).copied().unwrap_or([0; 4]), weights.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]), &pose);
                            model.min = model.min.min(p);
                            model.max = model.max.max(p);
                        }
                        model.skinned.push(Skinned { mesh: out, joints, weights, texture: texture_key });
                    }
                    _ => {
                        for p in &positions {
                            model.min = model.min.min(*p);
                            model.max = model.max.max(*p);
                        }
                        match texture_key {
                            Some(k) => model.scene.textured.entry(k).or_default().append(&out, Mat4::IDENTITY),
                            None => model.scene.plain.append(&out, Mat4::IDENTITY),
                        }
                    }
                }
            }
        }
        for child in node.children() {
            stack.push((child, m));
        }
    }
    if model.min.x > model.max.x {
        return Err("the file has no triangles".into());
    }
    Ok(model)
}

/// What a model holds besides its triangles, in a few words (nothing when nothing).
pub fn extras_note(m: &Model) -> String {
    let mut notes = vec![];
    let count = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    if !m.lights.is_empty() {
        notes.push(count(m.lights.len(), "light", "lights"));
    }
    if m.cameras > 0 {
        notes.push(format!("{} dropped (the game has its own)", count(m.cameras, "camera", "cameras")));
    }
    if let Some(rig) = &m.rig {
        notes.push(format!("a rig of {} ({} skinned)", count(rig.joints.len(), "joint", "joints"), count(m.skinned.len(), "part", "parts")));
    }
    if !m.animations.is_empty() {
        let names: Vec<String> = m.animations.iter().map(|a| format!("{} ({:.1} s)", if a.name.is_empty() { "unnamed" } else { &a.name }, a.duration)).collect();
        notes.push(format!("{}: {}", count(m.animations.len(), "animation", "animations"), names.join(", ")));
    }
    notes.join(", ")
}

/// Every prop's model file on disk under `root` (the project): the models that load, and
/// what went wrong with the others.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_disk_models(root: &std::path::Path, content: &content::Content) -> (crate::mesh::Models, Vec<String>) {
    let mut models = crate::mesh::Models::new();
    let mut problems = vec![];
    for e in content.props.entries() {
        let Some(path) = &e.value.model else { continue };
        let rel = e.space.asset(path);
        match std::fs::read(root.join(&rel)) {
            Ok(bytes) => match load_model(&bytes, &rel) {
                Ok(m) => {
                    models.insert(e.value.id.clone(), m);
                }
                Err(err) => problems.push(format!("{rel}: {err}")),
            },
            Err(err) => problems.push(format!("{rel}: {err}")),
        }
    }
    (models, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;
    use serde_json::json;

    /// A glb written by hand: a two-joint arm (the root at the origin, the elbow a unit up)
    /// with a quad skinned to it, and a one-second animation bending the elbow a quarter
    /// turn and back.
    fn rigged_glb() -> Vec<u8> {
        let mut bin: Vec<u8> = vec![];
        let mut views = vec![];
        let mut accessors = vec![];
        let mut push = |bytes: &[u8], count: usize, kind: &str, ty: u32, extra: serde_json::Value| -> usize {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            views.push(json!({ "buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len() }));
            bin.extend_from_slice(bytes);
            let mut a = json!({ "bufferView": views.len() - 1, "componentType": ty, "count": count, "type": kind });
            if let Some(o) = extra.as_object() {
                for (k, v) in o {
                    a[k] = v.clone();
                }
            }
            accessors.push(a);
            accessors.len() - 1
        };
        // a strip up the arm: y = 0, 1, 2, each a pair of vertices
        let pos: Vec<f32> = vec![-0.1, 0.0, 0.0, 0.1, 0.0, 0.0, -0.1, 1.0, 0.0, 0.1, 1.0, 0.0, -0.1, 2.0, 0.0, 0.1, 2.0, 0.0];
        let a_pos = push(bytemuck::cast_slice(&pos), 6, "VEC3", 5126, json!({ "min": [-0.1, 0.0, 0.0], "max": [0.1, 2.0, 0.0] }));
        let joints: Vec<u16> = vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
        let a_joints = push(bytemuck::cast_slice(&joints), 6, "VEC4", 5123, json!({}));
        let weights: Vec<f32> = vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let a_weights = push(bytemuck::cast_slice(&weights), 6, "VEC4", 5126, json!({}));
        let idx: Vec<u32> = vec![0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4];
        let a_idx = push(bytemuck::cast_slice(&idx), 12, "SCALAR", 5125, json!({}));
        let ibm: Vec<f32> = [Mat4::IDENTITY, Mat4::from_translation(-Vec3::Y)].iter().flat_map(|m| m.to_cols_array()).collect();
        let a_ibm = push(bytemuck::cast_slice(&ibm), 2, "MAT4", 5126, json!({}));
        let times: Vec<f32> = vec![0.0, 0.5, 1.0];
        let a_times = push(bytemuck::cast_slice(&times), 3, "SCALAR", 5126, json!({ "min": [0.0], "max": [1.0] }));
        let quarter = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let rots: Vec<f32> = [[0.0, 0.0, 0.0, 1.0], quarter, [0.0, 0.0, 0.0, 1.0]].concat();
        let a_rots = push(bytemuck::cast_slice(&rots), 3, "VEC4", 5126, json!({}));
        let doc = json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0, 2] }],
            "nodes": [
                { "name": "root", "children": [1] },
                { "name": "elbow", "translation": [0.0, 1.0, 0.0] },
                { "name": "arm", "mesh": 0, "skin": 0 }
            ],
            "skins": [{ "joints": [0, 1], "inverseBindMatrices": a_ibm }],
            "meshes": [{ "primitives": [{ "attributes": { "POSITION": a_pos, "JOINTS_0": a_joints, "WEIGHTS_0": a_weights }, "indices": a_idx }] }],
            "animations": [{
                "name": "bend",
                "samplers": [{ "input": a_times, "output": a_rots, "interpolation": "LINEAR" }],
                "channels": [{ "sampler": 0, "target": { "node": 1, "path": "rotation" } }]
            }],
            "accessors": accessors,
            "bufferViews": views,
            "buffers": [{ "byteLength": bin.len() }]
        });
        let mut text = serde_json::to_vec(&doc).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + text.len() + 8 + bin.len();
        let mut out = vec![];
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&text);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }

    #[test]
    fn a_rig_and_its_animation_come_in() {
        let m = load_model(&rigged_glb(), "arm.glb").expect("a readable glb");
        let rig = m.rig.as_ref().expect("a rig");
        assert_eq!(rig.joints.len(), 2);
        assert_eq!(m.skinned.len(), 1, "the arm is skinned, not static");
        assert!(m.scene.plain.vertices.is_empty());
        assert_eq!(m.animations.len(), 1);
        assert_eq!((m.animations[0].name.as_str(), m.animations[0].duration), ("bend", 1.0));
        assert!((m.max.y - 2.0).abs() < 1e-5 && m.min.y.abs() < 1e-5, "its size at rest: {:?} {:?}", m.min, m.max);
        assert!(extras_note(&m).contains("a rig of 2 joints (1 part skinned)") && extras_note(&m).contains("bend (1.0 s)"), "{}", extras_note(&m));
        // at rest the strip stands straight; half way through the bend, the tip swings left
        let rest = m.rest_scene();
        assert_eq!(rest.plain.vertices.len(), 6);
        assert!((rest.plain.vertices[4].pos[1] - 2.0).abs() < 1e-5);
        let bent = crate::anim::pose(rig, m.idle(), 0.5);
        let part = &m.skinned[0];
        let tip = crate::anim::skin_point(Vec3::from(part.mesh.vertices[4].pos), part.joints[4], part.weights[4], &bent);
        assert!((tip - Vec3::new(-1.0, 0.9, 0.0)).length() < 1e-4, "{tip:?}");
    }
}
