//! Props' real models: glTF files (`.glb`, as Blender exports them) turned into the
//! engine's meshes. One unit is one cell (a metre in Blender), Y is up and the front faces
//! +Z (Blender's front view), the origin is the centre of the footprint on the ground.
//! Colours come from the materials' base colour times the vertex colours; textured parts
//! are kept apart by texture, named "<key>#<image>". The file's lights
//! (`KHR_lights_punctual`, Blender's spot and point lights) come out as a prop's `light`
//! would hold them. Cameras are dropped (the game has one camera of its own); skins,
//! animations and the rest of the materials stay in the file, unread for now.

use std::collections::BTreeMap;

use content::Light;
use glam::{Mat3, Mat4, Vec3};

use crate::mesh::{Mesh, Scene, Vertex};

/// An image a model uses, decoded (RGBA8, sRGB).
#[derive(Clone, Debug)]
pub struct Image {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub scene: Scene,
    /// texture key -> image
    pub images: BTreeMap<String, Image>,
    pub min: Vec3,
    pub max: Vec3,
    /// its lights (spots and points), where they sit and point in the model's frame
    pub lights: Vec<Light>,
    /// what else the file holds: cameras (dropped), skins and animations (kept in the
    /// file, not played yet)
    pub cameras: usize,
    pub skins: usize,
    pub animations: usize,
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

/// Reads a `.glb` (or a `.gltf` with everything embedded). `key` names its textures.
pub fn load_model(bytes: &[u8], key: &str) -> Result<Model, String> {
    let (doc, buffers, images) = gltf::import_slice(bytes).map_err(|e| e.to_string())?;
    let mut model = Model { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN), ..Default::default() };
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("the file has no scene")?;
    model.cameras = doc.cameras().count();
    model.skins = doc.skins().count();
    model.animations = doc.animations().count();
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
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(positions) = reader.read_positions() else { continue };
                let positions: Vec<Vec3> = positions.map(|p| m.transform_point3(Vec3::from(p))).collect();
                let normals: Option<Vec<Vec3>> = reader.read_normals().map(|n| n.map(|n| (normal_m * Vec3::from(n)).normalize_or(Vec3::Y)).collect());
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
                    });
                    model.min = model.min.min(*p);
                    model.max = model.max.max(*p);
                }
                out.indices = indices.into_iter().filter(|&i| (i as usize) < positions.len()).collect();
                if normals.is_none() {
                    // flat normals from the triangles
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
                match texture.and_then(|i| images.get(i).map(|d| (i, d))) {
                    Some((i, data)) => {
                        let k = format!("{key}#{i}");
                        if !model.images.contains_key(&k) {
                            if let Some(img) = rgba8(data) {
                                model.images.insert(k.clone(), img);
                            }
                        }
                        model.scene.textured.entry(k).or_default().append(&out, Mat4::IDENTITY);
                    }
                    None => model.scene.plain.append(&out, Mat4::IDENTITY),
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
    if m.skins > 0 || m.animations > 0 {
        let mut kept = vec![];
        if m.skins > 0 {
            kept.push(count(m.skins, "rig", "rigs"));
        }
        if m.animations > 0 {
            kept.push(count(m.animations, "animation", "animations"));
        }
        notes.push(format!("{} kept in the file, not played yet", kept.join(" and ")));
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
