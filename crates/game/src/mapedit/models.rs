//! Props' real models (glTF files, from Blender or anything else) in the editor: loaded
//! from the props' `model` paths, their textures handed to the renderer, used wherever the
//! prop stands. And the way back to Blender: the map, a prop, every prop or a building as
//! a `.glb` (written to build/exports/ on the desktop, downloaded in the browser).

use render::map::MapVertex;
use render::scene::{Animated, SceneRenderer, SkinnedVertex, WHITE};
use world::gltf_in::load_model;
use world::gltf_out::{building_glb, map_glb, prop_glb, props_glb};
use glam::Vec3;
use world::mesh::{placement, Scene};

use super::{gpu, MapEditor};
use crate::platform::Load;

/// A scene as the renderer's batches: by texture, WHITE for the vertex-coloured part.
pub(crate) fn batches(scene: &Scene) -> Vec<(String, Vec<MapVertex>, Vec<u32>)> {
    let mut out = vec![(WHITE.to_string(), gpu(&scene.plain.vertices), scene.plain.indices.clone())];
    for (key, mesh) in &scene.textured {
        out.push((key.clone(), gpu(&mesh.vertices), mesh.indices.clone()));
    }
    out
}

impl MapEditor {
    /// Takes in the model files that arrived: textures to the renderer, everything that
    /// shows props rebuilt.
    pub(super) fn load_models(&mut self, r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState) {
        let content = self.content.clone();
        for e in content.props.entries() {
            let id = &e.value.id;
            let Some(model) = &e.value.model else { continue };
            if self.models.contains_key(id) || self.model_failed.contains(id) {
                continue;
            }
            let path = e.space.asset(model);
            self.files.request(&path);
            match self.files.get(&path) {
                Load::Pending => {}
                Load::Failed(err) => {
                    self.status = format!("{id}'s model: {err}");
                    self.model_failed.insert(id.clone());
                }
                Load::Ready(bytes) => match load_model(&bytes, &path) {
                    Ok(m) => {
                        for (key, img) in &m.images {
                            r.add_texture(&rs.device, &rs.queue, key, &img.rgba, img.width, img.height, true);
                        }
                        // its moving parts (if rigged) go to the renderer below
                        self.skinned_sent.remove(id);
                        self.models.insert(id.clone(), m);
                        *self.model_versions.entry(id.clone()).or_default() += 1;
                        self.need_props = true;
                        self.need_trees = true;
                        self.need_buildings = true;
                        self.extra_changed = true;
                    }
                    Err(err) => {
                        self.status = format!("{id}'s model ({path}): {err}");
                        self.model_failed.insert(id.clone());
                    }
                },
            }
        }
        // the parts of rigged models that move with their rig: the renderer keeps them, the
        // frames say the pose; sent again when the prop is made toon or not
        for (id, m) in self.models.iter().filter(|(_, m)| !m.skinned.is_empty()) {
            let toon = content.props.get(id).map(|p| p.toon).unwrap_or(false);
            if self.skinned_sent.get(id) == Some(&toon) {
                continue;
            }
            let parts: Vec<(String, Vec<SkinnedVertex>, Vec<u32>)> = m
                .skinned
                .iter()
                .map(|s| {
                    let vertices = s
                        .mesh
                        .vertices
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            let j = s.joints.get(i).copied().unwrap_or([0; 4]);
                            let weights = s.weights.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
                            SkinnedVertex { pos: v.pos, normal: v.normal, uv: v.uv, color: v.color, joints: j.map(|x| x as u32), weights, toon: toon as u32 as f32 }
                        })
                        .collect();
                    (s.texture.clone().unwrap_or_else(|| WHITE.to_string()), vertices, s.mesh.indices.clone())
                })
                .collect();
            r.set_skinned(&rs.device, id, &parts);
            self.skinned_sent.insert(id.clone(), toon);
        }
        crate::platform::set_hook("models", &self.models.len().to_string());
    }

    /// Hands a .glb over: a file in build/exports/ on the desktop, a download in the browser.
    fn deliver(&mut self, name: &str, bytes: Vec<u8>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            let dir = root.join("build").join("exports");
            let path = dir.join(name);
            self.status = match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
                Ok(()) => format!("For Blender: {} (File > Import > glTF, or the Battle Frontier add-on)", path.display()),
                Err(e) => format!("Not exported: {e}"),
            };
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = match crate::platform::download_bytes(name, &bytes, "model/gltf-binary") {
                Ok(()) => format!("Downloaded {name}: open it in Blender with File > Import > glTF"),
                Err(e) => format!("Not exported: {e}"),
            };
        }
        crate::platform::set_hook("exported", name);
    }

    pub(super) fn export_map(&mut self) {
        let bytes = map_glb(&self.map, &self.content, self.level_height, &self.models);
        let name = format!("{}.glb", self.map.id);
        self.deliver(&name, bytes);
    }

    pub(super) fn export_prop(&mut self, id: &str) {
        let Some(p) = self.content.props.get(id).cloned() else { return };
        let bytes = prop_glb(&p, &self.models);
        self.deliver(&format!("{id}.glb"), bytes);
    }

    /// Every prop in one file, laid out by category.
    pub(super) fn export_props(&mut self) {
        let bytes = props_glb(&self.content, &self.models);
        self.deliver("props.glb", bytes);
    }

    /// Opens a prop in Blender to edit it: on the desktop, Blender itself with the prop
    /// imported (the add-on's "Add to the project" sends it back, and the editor picks up
    /// the change within a second); in the browser, its .glb downloaded instead.
    pub(super) fn edit_in_blender(&mut self, id: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            self.status = match open_in_blender(&root, id) {
                Ok(msg) => msg,
                Err(e) => {
                    self.export_prop(id);
                    format!("{e} · {}", self.status)
                }
            };
        }
        #[cfg(target_arch = "wasm32")]
        {
            // the desktop game answers battlefrontier: links (it sets them up when it starts)
            self.status = match crate::platform::open_link(&format!("{LINK}edit/{id}")) {
                Ok(()) => format!("Opening Blender on {id} through the battlefrontier: link (run the desktop game once to set it up); it saves the prop into your project by itself"),
                Err(e) => format!("Blender not opened: {e}"),
            };
        }
        crate::platform::set_hook("blender", id);
    }


    pub(super) fn export_building(&mut self, i: usize) {
        let Some(b) = self.map.buildings.get(i).cloned() else { return };
        let bytes = building_glb(&b, &self.content, self.level_height, &self.models);
        let name = format!("{}-{}.glb", self.map.id, b.id);
        self.deliver(&name, bytes);
    }

    /// The rigged props on the map in their poses this frame: each plays its idle
    /// animation (or its first), out of step with the others.
    pub(super) fn animated_draws(&self, now: f64) -> Vec<Animated> {
        let mut out = vec![];
        let lh = self.level_height;
        let t = (now / 1000.0) as f32;
        for (i, p) in self.map.props.iter().enumerate() {
            let Some(m) = self.models.get(&p.prop) else { continue };
            let Some(rig) = &m.rig else { continue };
            // still, at rest, when Animate is off
            let anim = if self.animate { m.idle() } else { None };
            let Some(prop) = self.content.props.get(&p.prop) else { continue };
            // out of reach: not worth posing
            let pos = p.pos();
            if Vec3::new(pos[0] as f32, 0.0, pos[1] as f32).distance(Vec3::new(self.cam.target.x, 0.0, self.cam.target.z)) > self.cam.distance * 1.5 + 24.0 {
                continue;
            }
            let scale = if prop.scalable() { p.scale as f32 } else { 1.0 };
            let at = Vec3::new(pos[0] as f32, p.level as f32 * lh, pos[1] as f32);
            let phase = (i as f32 * 0.37) % 1.0 * anim.map(|a| a.duration).unwrap_or(1.0).max(0.1);
            out.push(Animated { key: p.prop.clone(), model: placement(at, p.turn as f32, scale), joints: world::anim::pose(rig, anim, t + phase) });
        }
        out
    }
}

/// The links the browser's "Edit in Blender" opens: `battlefrontier://edit/<prop id>`.
pub const LINK: &str = "battlefrontier://";

/// The prop a link names, if it's one of ours (the desktop game answers the links).
#[cfg(not(target_arch = "wasm32"))]
pub fn link_target(url: &str) -> Option<String> {
    let url = url.trim().trim_matches('"');
    let rest = url.strip_prefix(LINK).or_else(|| url.strip_prefix("battlefrontier:"))?.trim_start_matches('/');
    let id = rest.strip_prefix("edit/")?.trim_end_matches('/');
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')).then(|| id.to_string())
}

/// A path as people and other programs write it: Windows' canonical paths come as
/// `\\?\F:\...` (or `\\?\UNC\server\...`), which Blender takes for a relative path (it puts
/// its own folder in front, and finds nothing).
#[cfg(not(target_arch = "wasm32"))]
pub fn plain_path(p: &std::path::Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}").into();
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) => rest.into(),
        None => p.to_path_buf(),
    }
}

/// Opens Blender on a prop of the project at `root`: the add-on is set up, the prop
/// imported, and it's written back to the project by itself as it's edited
/// (`tools/blender/open_prop.py`).
#[cfg(not(target_arch = "wasm32"))]
pub fn open_in_blender(root: &std::path::Path, id: &str) -> Result<String, String> {
    let root = plain_path(&std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
    let blender = find_blender().ok_or("Blender not found: set BLENDER to blender.exe, or put it on the PATH")?;
    let script = root.join("tools").join("blender").join("open_prop.py");
    if !script.is_file() {
        return Err(format!("{} isn't there: is {} the project?", script.display(), root.display()));
    }
    // from the project's folder (a link from the browser starts this program anywhere)
    std::process::Command::new(&blender)
        .arg("--python")
        .arg(&script)
        .arg("--")
        .arg(&root)
        .arg(id)
        .current_dir(&root)
        .spawn()
        .map_err(|e| format!("Blender ({}) wouldn't start: {e}", blender.display()))?;
    Ok(format!("Blender is opening {id}: what you change there is saved into the project by itself (and with Ctrl+S), and shows here within a second"))
}

/// Has the system send battlefrontier: links to this program (Windows: the user's own
/// registry, no admin), with the project at `root`.
#[cfg(windows)]
pub fn register_links(root: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let (Ok(exe), Ok(root)) = (std::env::current_exe(), std::fs::canonicalize(root)) else { return };
    let command = format!("\"{}\" --root \"{}\" --link \"%1\"", plain_path(&exe).display(), plain_path(&root).display());
    let key = r"HKCU\Software\Classes\battlefrontier";
    let open = format!(r"{key}\shell\open\command");
    for args in [vec!["add", key, "/ve", "/d", "URL:Battle Frontier", "/f"], vec!["add", key, "/v", "URL Protocol", "/d", "", "/f"], vec!["add", open.as_str(), "/ve", "/d", command.as_str(), "/f"]] {
        // no console window flashing
        let _ = std::process::Command::new("reg").args(&args).creation_flags(0x0800_0000).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

/// Where Blender is: `BLENDER` (the program's path), `blender` on the PATH, or the newest
/// one under Program Files (its launcher, which opens no console).
#[cfg(not(target_arch = "wasm32"))]
fn find_blender() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("BLENDER") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).find_map(|dir| {
        ["blender-launcher.exe", "blender.exe", "blender"].iter().map(|n| dir.join(n)).find(|p| p.is_file())
    });
    if on_path.is_some() {
        return on_path;
    }
    let mut found: Vec<std::path::PathBuf> = vec![];
    for base in ["C:\\Program Files\\Blender Foundation", "C:\\Program Files (x86)\\Blender Foundation", "/Applications", "/usr/bin", "/snap/bin"] {
        let Ok(entries) = std::fs::read_dir(base) else { continue };
        for e in entries.flatten() {
            let dir = e.path();
            for name in ["blender-launcher.exe", "blender.exe", "Blender.app/Contents/MacOS/Blender", "blender"] {
                let p = if base == "/usr/bin" || base == "/snap/bin" { dir.clone() } else { dir.join(name) };
                if p.is_file() && p.file_name().map(|f| f.to_string_lossy().to_lowercase().contains("blender")).unwrap_or(false) {
                    found.push(p);
                    break;
                }
            }
        }
    }
    found.sort();
    found.pop()
}

#[cfg(test)]
mod tests {
    use super::{link_target, plain_path};
    use std::path::{Path, PathBuf};

    #[test]
    fn blender_gets_plain_paths() {
        assert_eq!(plain_path(Path::new(r"\\?\F:\Emulation\BattleFrontier")), PathBuf::from(r"F:\Emulation\BattleFrontier"));
        assert_eq!(plain_path(Path::new(r"\\?\UNC\server\share\game")), PathBuf::from(r"\\server\share\game"));
        assert_eq!(plain_path(Path::new(r"F:\Emulation\BattleFrontier")), PathBuf::from(r"F:\Emulation\BattleFrontier"));
        assert_eq!(plain_path(Path::new("/home/someone/game")), PathBuf::from("/home/someone/game"));
    }

    #[test]
    fn links_name_a_prop() {
        assert_eq!(link_target("battlefrontier://edit/lamp_post").as_deref(), Some("lamp_post"));
        assert_eq!(link_target("\"battlefrontier://edit/stone_well/\"").as_deref(), Some("stone_well"));
        assert_eq!(link_target("battlefrontier:edit/bench").as_deref(), Some("bench"));
        assert_eq!(link_target("battlefrontier://edit/../secret"), None);
        assert_eq!(link_target("https://edit/lamp_post"), None);
        assert_eq!(link_target("battlefrontier://delete/lamp_post"), None);
    }
}
