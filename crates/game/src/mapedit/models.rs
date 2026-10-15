//! Props' real models (glTF files, from Blender or anything else) in the editor: loaded
//! from the props' `model` paths, their textures handed to the renderer, used wherever the
//! prop stands. And the way back to Blender: the map, a prop, every prop or a building as
//! a `.glb` (written to build/exports/ on the desktop, downloaded in the browser).

use render::map::MapVertex;
use render::scene::{SceneRenderer, WHITE};
use world::gltf_in::load_model;
use world::gltf_out::{building_glb, map_glb, prop_glb, props_glb};
use world::mesh::Scene;

use super::{gpu, MapEditor};
use crate::platform::Load;

/// A scene as the renderer's batches: by texture, WHITE for the vertex-coloured part.
pub(super) fn batches(scene: &Scene) -> Vec<(String, Vec<MapVertex>, Vec<u32>)> {
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
                        self.models.insert(id.clone(), m);
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

    pub(super) fn export_building(&mut self, i: usize) {
        let Some(b) = self.map.buildings.get(i).cloned() else { return };
        let bytes = building_glb(&b, &self.content, self.level_height, &self.models);
        let name = format!("{}-{}.glb", self.map.id, b.id);
        self.deliver(&name, bytes);
    }
}
