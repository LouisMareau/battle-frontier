//! The asset browser: every prop of both spaces as a detailed list (name, id, GAME or
//! TEMPORARY, category, size, model, what it does, folders), by folder ("Buildings/Inside",
//! "Nature"...), and the creatures under Creatures (whether they're toon). A row dragged onto a folder puts the prop in it too (a prop can be in
//! several); a folder chip's × takes it out. A row clicked is what the Place tool puts down,
//! or, while a building is designed, what goes inside or outside it (if it's in that
//! folder). Files dropped on the window (.glb or .gltf, from Blender or anywhere) become
//! props in the folder they're dropped on: on the desktop written to content/ (or
//! temporary/) at once, in the browser kept until saved (downloaded); pictures dropped while
//! the Requests window is open go to it (`requests.rs`). On the desktop the
//! props' records and models on disk are watched: what changes there (the Blender add-on's
//! "Add to project", an edit) shows at once.

use std::collections::BTreeSet;
use std::sync::Arc;

use content::{Creature, Prop, Space, INSIDE, OUTSIDE};
use egui::{Color32, RichText, Sense};
use world::gltf_in::load_model;

use super::build::Clicks;
use super::{badge, hint, mark, requests, section, swatch, MapEditor};
use crate::platform::{self, Dropped};

/// What a prop can be (the category picker).
const CATEGORIES: [&str; 19] = [
    "decoration", "furniture", "wall_decoration", "stairs", "flora", "tree", "rock", "fence", "character", "door", "window", "wall", "pillar", "floor", "roof",
    "railing", "sign", "light", "other",
];

/// "#rrggbb" as bytes (magenta when it isn't one).
fn hex_rgb(hex: &str) -> [u8; 3] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0xff00ff);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// A change to a prop, made once the list is drawn.
type Change = Box<dyn FnOnce(&mut Prop)>;

/// Where dropped files go when no folder is shown.
const IMPORTED: &str = "Imported";

/// The folder row that shows the creatures instead of props.
const CREATURES: &str = "#creatures";

#[derive(Default)]
pub(super) struct Assets {
    pub open: bool,
    /// the folder shown (None: all of them)
    pub folder: Option<String>,
    pub filter: String,
    /// folders made here that nothing is in yet
    pub made: BTreeSet<String>,
    pub new_folder: String,
    /// props changed here and not saved yet
    pub dirty: BTreeSet<String>,
    /// creatures changed here and not saved yet
    pub creatures_dirty: BTreeSet<String>,
    /// where dropped files go: temporary/ (placeholders, never committed), else content/
    pub to_temporary: bool,
    /// files dropped, read (in the browser, when the page has read them)
    dropped: Dropped,
    /// the folder they were dropped on
    drop_to: String,
    /// the folder rows on screen (to drop on)
    rows: Vec<(String, egui::Rect)>,
    /// files hovering over the window
    hovering: bool,
    /// the prop whose light is being edited
    light_edit: Option<String>,
    /// models dropped in the browser (file name, bytes): downloaded with the records on save
    #[cfg(target_arch = "wasm32")]
    new_models: Vec<(String, Vec<u8>)>,
    /// the desktop: the props' records and models on disk, as last seen
    #[cfg(not(target_arch = "wasm32"))]
    seen: Vec<(std::path::PathBuf, std::time::SystemTime)>,
    #[cfg(not(target_arch = "wasm32"))]
    next_look: f64,
}

impl Assets {
    /// How many records wait to be saved.
    pub fn changed(&self) -> usize {
        self.dirty.len() + self.creatures_dirty.len()
    }
}

/// In the folder, or one inside it.
fn in_folder(p: &Prop, folder: &str) -> bool {
    p.folders().iter().any(|f| f == folder || f.strip_prefix(folder).map(|r| r.starts_with('/')).unwrap_or(false))
}

/// A file name made into an id: lowercase letters, digits and _.
pub(super) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// A file name made into a name: "stone_well-2" -> "Stone well 2".
pub(super) fn title(stem: &str) -> String {
    let words = stem.replace(['_', '-', '.'], " ");
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut c = words.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "Model".into(),
    }
}

fn category_for(folder: &str) -> &'static str {
    if folder.starts_with(INSIDE) {
        "furniture"
    } else if folder.starts_with("Nature") {
        "flora"
    } else if folder.starts_with("Characters") {
        "character"
    } else {
        "decoration"
    }
}

impl MapEditor {
    /// Every folder: the props', the ones made here and the building ones, with their
    /// parents ("Buildings" for "Buildings/Inside").
    fn asset_folders(&self) -> Vec<String> {
        let mut leaves: BTreeSet<String> = self.assets.made.clone();
        leaves.insert(INSIDE.into());
        leaves.insert(OUTSIDE.into());
        for p in self.content.props.iter() {
            leaves.extend(p.folders());
        }
        let mut all = BTreeSet::new();
        for f in leaves {
            let mut path = String::new();
            for part in f.split('/') {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                all.insert(path.clone());
            }
        }
        all.into_iter().collect()
    }

    /// Changes a prop where it is (its space), to save later.
    fn change_prop(&mut self, id: &str, change: impl FnOnce(&mut Prop)) {
        let Some(e) = self.content.props.entry(id) else { return };
        let (mut p, space) = (e.value.clone(), e.space);
        change(&mut p);
        let was = self.content.props.get(id).unwrap();
        if &p == was {
            return;
        }
        if p.toon != was.toon {
            // drawn another way: wherever it stands
            self.need_props = true;
            self.need_trees = true;
            self.need_buildings = true;
            self.extra_changed = true;
        }
        let mut c = (*self.content).clone();
        c.props.put(p, space);
        self.content = Arc::new(c);
        self.assets.dirty.insert(id.to_string());
        self.blocked = None;
    }

    /// Changes a creature where it is (its space), to save later.
    fn change_creature(&mut self, id: &str, change: impl FnOnce(&mut Creature)) {
        let Some(e) = self.content.creatures.entry(id) else { return };
        let (mut cr, space) = (e.value.clone(), e.space);
        change(&mut cr);
        if &cr == self.content.creatures.get(id).unwrap() {
            return;
        }
        let mut c = (*self.content).clone();
        c.creatures.put(cr, space);
        self.content = Arc::new(c);
        self.assets.creatures_dirty.insert(id.to_string());
    }

    pub(super) fn add_to_folder(&mut self, id: &str, folder: &str) {
        if !self.content.props.contains(id) {
            return;
        }
        self.change_prop(id, |p| {
            let mut f = p.folders();
            if !f.iter().any(|x| x == folder) {
                f.push(folder.to_string());
            }
            p.folders = f;
        });
        self.assets.made.remove(folder);
        self.status = format!("{id} is in {folder} too");
    }

    fn take_from_folder(&mut self, id: &str, folder: &str) {
        self.change_prop(id, |p| {
            let mut f = p.folders();
            f.retain(|x| x != folder);
            if !f.is_empty() {
                p.folders = f;
            }
        });
    }

    /// A prop picked in the browser: what the Place tool puts down, and while a building is
    /// designed, what goes inside or outside it (if it's in that folder).
    pub(super) fn use_prop(&mut self, id: &str) {
        self.prop = Some(id.to_string());
        let Some(p) = self.content.props.get(id).cloned() else { return };
        let Some(d) = self.draft.as_mut() else { return };
        let folders = p.folders();
        match d.clicks {
            Clicks::Inside if folders.iter().any(|f| f == INSIDE) => d.inside_prop = Some(id.to_string()),
            Clicks::Outside if folders.iter().any(|f| f == OUTSIDE) => d.outside_prop = Some(id.to_string()),
            Clicks::Inside => self.status = format!("{} isn't in {INSIDE}: drag its row onto that folder first", p.name),
            Clicks::Outside => self.status = format!("{} isn't in {OUTSIDE}: drag its row onto that folder first", p.name),
            _ => {}
        }
    }

    // ------------------------------------------------------------ the panel

    pub(super) fn assets_panel(&mut self, ui: &mut egui::Ui) {
        let folders = self.asset_folders();
        let content = self.content.clone();
        let count = |f: &str| content.props.iter().filter(|p| in_folder(p, f)).count();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Assets").strong());
            ui.add(egui::TextEdit::singleline(&mut self.assets.filter).hint_text("Filter: name, id, category").desired_width(180.0));
            // what files hovering would do: in this line, so nothing moves under them
            if self.assets.hovering {
                let to = self.assets.folder.clone().unwrap_or_else(|| IMPORTED.into());
                ui.label(RichText::new(format!("Drop them on a folder (elsewhere: into {to})")).strong().color(Color32::from_rgb(255, 200, 90)));
            } else {
                ui.label(RichText::new("Drag rows onto folders · drop .glb files on a folder").color(Color32::GRAY));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = self.assets.changed();
                let save = ui.add_enabled(n > 0, egui::Button::new(format!("Save {n} changed")));
                mark("uiSaveAssets", &save);
                if save.clicked() {
                    self.save_assets();
                }
                let all = ui.button("All props to Blender").on_hover_text("One .glb with every prop laid out by category, each with its cells and its light");
                mark("uiExportProps", &all);
                if all.clicked() {
                    self.export_props();
                }
                ui.checkbox(&mut self.assets.to_temporary, "Drops go to temporary/")
                    .on_hover_text("Placeholders, never committed. Off: the game's own content/ (only what you made or may share)");
            });
        });
        ui.separator();
        self.assets.rows.clear();
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                egui::ScrollArea::vertical().id_salt("asset-folders").max_height(height).show(ui, |ui| {
                    let all = ui.selectable_label(self.assets.folder.is_none(), format!("All assets ({})", content.props.len()));
                    if all.clicked() {
                        self.assets.folder = None;
                    }
                    for f in &folders {
                        let depth = f.matches('/').count();
                        let name = f.rsplit('/').next().unwrap_or(f);
                        let r = ui
                            .horizontal(|ui| {
                                ui.add_space(14.0 * depth as f32);
                                ui.selectable_label(self.assets.folder.as_deref() == Some(f.as_str()), format!("{name} ({})", count(f)))
                            })
                            .inner;
                        mark(&format!("uiFolder_{}", f.replace(['/', ' '], "_")), &r);
                        if r.clicked() {
                            self.assets.folder = Some(f.clone());
                        }
                        // a row dragged here: the prop goes in this folder too
                        if r.dnd_hover_payload::<String>().is_some() || (self.assets.hovering && r.contains_pointer()) {
                            ui.painter().rect_stroke(r.rect.expand(2.0), 3.0, (1.5, Color32::from_rgb(255, 200, 90)), egui::StrokeKind::Outside);
                        }
                        if let Some(id) = r.dnd_release_payload::<String>() {
                            self.add_to_folder(&id, f);
                        }
                        self.assets.rows.push((f.clone(), r.rect));
                    }
                    ui.separator();
                    let creatures = ui.selectable_label(self.assets.folder.as_deref() == Some(CREATURES), format!("Creatures ({})", content.creatures.len())).on_hover_text("Every creature: whether it's drawn toon");
                    mark("uiFolder_Creatures", &creatures);
                    if creatures.clicked() {
                        self.assets.folder = Some(CREATURES.into());
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.assets.new_folder).hint_text("New folder (a/b)").desired_width(130.0));
                        if ui.button("Add").clicked() {
                            let name: String = self.assets.new_folder.split('/').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/");
                            if !name.is_empty() {
                                self.assets.made.insert(name.clone());
                                self.assets.folder = Some(name);
                                self.assets.new_folder.clear();
                            }
                        }
                    });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::both().id_salt("asset-list").max_height(height).show(ui, |ui| self.asset_table(ui));
            });
        });
        // what's being dragged, by the pointer
        let ctx = ui.ctx().clone();
        if let (Some(id), Some(pos)) = (egui::DragAndDrop::payload::<String>(&ctx), ctx.pointer_latest_pos()) {
            egui::Area::new(egui::Id::new("asset-drag")).order(egui::Order::Tooltip).fixed_pos(pos + egui::vec2(14.0, 10.0)).interactable(false).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(format!("{id}: onto a folder")));
            });
        }
        crate::platform::set_hook("assets", &content.props.len().to_string());
        crate::platform::set_hook("toonprops", &content.props.iter().filter(|p| p.toon).map(|p| p.id.as_str()).collect::<Vec<_>>().join(","));
        crate::platform::set_hook("assetsdirty", &self.assets.dirty.len().to_string());
        crate::platform::set_hook("creaturesdirty", &self.assets.creatures_dirty.len().to_string());
        crate::platform::set_hook("folder", self.assets.folder.as_deref().unwrap_or(""));
    }

    fn asset_table(&mut self, ui: &mut egui::Ui) {
        if self.assets.folder.as_deref() == Some(CREATURES) {
            self.creature_table(ui);
            return;
        }
        let content = self.content.clone();
        let filter = self.assets.filter.to_lowercase();
        let folder = self.assets.folder.clone();
        let rows: Vec<_> = content
            .props
            .entries()
            .iter()
            .filter(|e| folder.as_deref().map(|f| in_folder(&e.value, f)).unwrap_or(true))
            .filter(|e| filter.is_empty() || e.value.id.contains(&filter) || e.value.name.to_lowercase().contains(&filter) || e.value.category.contains(&filter))
            .collect();
        crate::platform::set_hook("shown", &rows.iter().map(|e| e.value.id.as_str()).collect::<Vec<_>>().join(","));
        if rows.is_empty() {
            hint(ui, "Nothing here yet: drag rows from another folder, or drop .glb files on this one.");
            return;
        }
        let mut picked = None;
        let mut light_edit = None;
        let mut changes: Vec<(String, Change)> = vec![];
        let mut out_of: Option<(String, String)> = None;
        egui::Grid::new("asset-grid").striped(true).num_columns(8).spacing([14.0, 5.0]).show(ui, |ui| {
            for h in ["Name", "Id", "", "Category", "Size", "Model", "Does", "Folders"] {
                ui.label(RichText::new(h).strong().size(11.5));
            }
            ui.end_row();
            for e in rows {
                let p = &e.value;
                let chosen = self.prop.as_deref() == Some(p.id.as_str());
                let dirty = if self.assets.dirty.contains(&p.id) { " •" } else { "" };
                let r = ui.selectable_label(chosen, format!("{}{dirty}", p.name)).interact(Sense::drag()).on_hover_text("Click: use it. Drag: onto a folder");
                r.dnd_set_drag_payload(p.id.clone());
                mark(&format!("uiAsset_{}", p.id), &r);
                if r.clicked() {
                    picked = Some(p.id.clone());
                }
                ui.label(RichText::new(&p.id).monospace().size(11.0).color(Color32::GRAY));
                badge(ui, e.space);
                let mut category = p.category.clone();
                egui::ComboBox::from_id_salt(("asset-category", &p.id)).selected_text(&category).width(104.0).show_ui(ui, |ui| {
                    for c in CATEGORIES {
                        ui.selectable_value(&mut category, c.to_string(), c);
                    }
                });
                if category != p.category {
                    changes.push((p.id.clone(), Box::new(move |q: &mut Prop| q.category = category)));
                }
                ui.label(format!("{}×{} · {:.2}", p.footprint[0], p.footprint[1], p.height)).on_hover_text("Footprint in cells, height");
                match &p.model {
                    Some(m) => {
                        let state = if self.models.contains_key(&p.id) {
                            ""
                        } else if self.model_failed.contains(&p.id) {
                            " (won't load)"
                        } else {
                            " (loading)"
                        };
                        ui.label(format!("{}{state}", m.rsplit('/').next().unwrap_or(m)));
                    }
                    None => {
                        ui.label(RichText::new(format!("stand-in, {} part{}", p.parts.len(), if p.parts.len() == 1 { "" } else { "s" })).color(Color32::GRAY));
                    }
                }
                ui.horizontal(|ui| {
                    // toggles: lit when on
                    let blocks = !p.blocks;
                    if ui.selectable_label(p.blocks, "blocks").on_hover_text("The player can't walk through it").clicked() {
                        changes.push((p.id.clone(), Box::new(move |q: &mut Prop| q.blocks = blocks)));
                    }
                    let solid = !p.unstackable;
                    let r = ui.selectable_label(p.unstackable, "unstackable").on_hover_text("Nothing goes on it or overlaps it, and it goes on nothing");
                    mark(&format!("uiUnstackable_{}", p.id), &r);
                    if r.clicked() {
                        changes.push((p.id.clone(), Box::new(move |q: &mut Prop| q.unstackable = solid)));
                    }
                    if p.on_walls() {
                        ui.label(RichText::new("on walls").color(Color32::GRAY));
                    }
                    let lit = ui.selectable_label(p.light.is_some(), "light").on_hover_text(if p.light.is_some() { "The light it gives off: click to change it" } else { "Give it a light (a lamp)" });
                    mark(&format!("uiLight_{}", p.id), &lit);
                    if lit.clicked() {
                        light_edit = Some(p.id.clone());
                    }
                    let toon = !p.toon;
                    let r = ui.selectable_label(p.toon, "toon").on_hover_text("Drawn with the toon look: light in steps, ink lines (Post-processing sets how)");
                    mark(&format!("uiToon_{}", p.id), &r);
                    if r.clicked() {
                        changes.push((p.id.clone(), Box::new(move |q: &mut Prop| q.toon = toon)));
                    }
                });
                ui.horizontal(|ui| {
                    let own = !p.folders.is_empty();
                    let folders = p.folders();
                    for f in &folders {
                        let chip = egui::Button::new(RichText::new(if folders.len() > 1 { format!("{f} ×") } else { f.clone() }).size(11.0)).small();
                        let r = ui.add_enabled(folders.len() > 1, chip).on_hover_text(if own { "Take it out of this folder" } else { "Its category's folder" });
                        if r.clicked() {
                            out_of = Some((p.id.clone(), f.clone()));
                        }
                    }
                });
                ui.end_row();
            }
        });
        for (id, change) in changes {
            self.change_prop(&id, change);
        }
        if let Some((id, f)) = out_of {
            self.take_from_folder(&id, &f);
        }
        if let Some(id) = picked {
            self.use_prop(&id);
        }
        if light_edit.is_some() {
            self.assets.light_edit = light_edit;
        }
    }

    /// The creatures, under the folders' Creatures: whether each is toon.
    fn creature_table(&mut self, ui: &mut egui::Ui) {
        let content = self.content.clone();
        let filter = self.assets.filter.to_lowercase();
        let rows: Vec<_> = content.creatures.entries().iter().filter(|e| filter.is_empty() || e.value.id.contains(&filter) || e.value.name.to_lowercase().contains(&filter)).collect();
        crate::platform::set_hook("shown", &rows.iter().map(|e| e.value.id.as_str()).collect::<Vec<_>>().join(","));
        if rows.is_empty() {
            hint(ui, "No creatures here.");
            return;
        }
        let mut changes: Vec<(String, bool)> = vec![];
        egui::Grid::new("creature-grid").striped(true).num_columns(5).spacing([14.0, 5.0]).show(ui, |ui| {
            for h in ["Name", "Id", "", "Types", "Does"] {
                ui.label(RichText::new(h).strong().size(11.5));
            }
            ui.end_row();
            for e in rows {
                let c = &e.value;
                let dirty = if self.assets.creatures_dirty.contains(&c.id) { " •" } else { "" };
                ui.label(format!("{}{dirty}", c.name));
                ui.label(RichText::new(&c.id).monospace().size(11.0).color(Color32::GRAY));
                badge(ui, e.space);
                ui.label(c.types.join(", "));
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(c.toon, "toon").on_hover_text("Drawn with the toon look: ink lines round its picture (Post-processing sets how)");
                    mark(&format!("uiToon_{}", c.id), &r);
                    if r.clicked() {
                        changes.push((c.id.clone(), !c.toon));
                    }
                    if c.light.is_some() {
                        ui.label(RichText::new("light").color(Color32::GRAY));
                    }
                    if c.sound.is_some() {
                        ui.label(RichText::new("sound").color(Color32::GRAY));
                    }
                });
                ui.end_row();
            }
        });
        for (id, toon) in changes {
            self.change_creature(&id, |c| c.toon = toon);
        }
    }

    /// A prop's record, in the inspector of a placed prop: what it is, what it does, its
    /// light and its sound, and the way to Blender. Changes wait in the asset browser's
    /// "Save changed" (Ctrl+S saves them with the map).
    pub(super) fn prop_settings(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(e) = self.content.props.entry(id).cloned() else { return };
        let p = e.value;
        let mut after = p.clone();
        section(ui, "Prop");
        ui.horizontal(|ui| {
            badge(ui, e.space);
            ui.label(RichText::new(&p.id).monospace().size(11.0).color(Color32::GRAY));
            if self.assets.dirty.contains(id) {
                let save = ui.small_button("Save prop").on_hover_text("Writes its record (Ctrl+S does too)");
                mark("uiSaveProp", &save);
                if save.clicked() {
                    self.save_assets();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut after.name).desired_width(150.0));
        });
        ui.horizontal(|ui| {
            ui.label("Category");
            egui::ComboBox::from_id_salt(("prop-category", id)).selected_text(&after.category).width(130.0).show_ui(ui, |ui| {
                for c in CATEGORIES {
                    ui.selectable_value(&mut after.category, c.to_string(), c);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Cells");
            ui.add(egui::DragValue::new(&mut after.footprint[0]).range(1..=32)).on_hover_text("Along x");
            ui.add(egui::DragValue::new(&mut after.footprint[1]).range(1..=32)).on_hover_text("Along z");
            ui.label("high");
            ui.add(egui::DragValue::new(&mut after.height).speed(0.05).range(0.05..=40.0));
        });
        if matches!(after.category.as_str(), "window") {
            ui.horizontal(|ui| {
                ui.label("Sill");
                ui.add(egui::DragValue::new(&mut after.sill).speed(0.05).range(0.0..=4.0)).on_hover_text("How high above the floor a window starts");
            });
        }
        ui.horizontal(|ui| {
            let blocks = ui.selectable_label(after.blocks, "blocks").on_hover_text("The player can't walk through it");
            mark("uiPropBlocks", &blocks);
            if blocks.clicked() {
                after.blocks = !after.blocks;
            }
            let solid = ui.selectable_label(after.unstackable, "unstackable").on_hover_text("Nothing goes on it or overlaps it, and it goes on nothing");
            if solid.clicked() {
                after.unstackable = !after.unstackable;
            }
            let toon = ui.selectable_label(after.toon, "toon").on_hover_text("Drawn with the toon look: light in steps, ink lines (Post-processing sets how)");
            mark("uiPropToon", &toon);
            if toon.clicked() {
                after.toon = !after.toon;
            }
        });
        // in the wind: a model by its paint, a stand-in by its record
        match self.models.get(id) {
            Some(m) if m.wind > 0 => hint(ui, &format!("Moves in the wind by its model's paint ({} vertices).", m.wind)),
            Some(_) => hint(ui, "Still in the wind: its model isn't painted for it (in Blender, a \"wind\" colour layer: R sway, G flutter, B phase)."),
            None => {
                ui.horizontal(|ui| {
                    ui.label("Wind").on_hover_text("How its stand-in moves in the wind (a model is painted for it instead)");
                    let r = ui.add(egui::Slider::new(&mut after.wind.sway, 0.0..=1.0).text("sway")).on_hover_text("Bending with the wind, more the higher up (0: not at all)");
                    mark("uiPropSway", &r);
                });
                ui.horizontal(|ui| {
                    ui.add_space(34.0);
                    let r = ui.add(egui::Slider::new(&mut after.wind.flutter, 0.0..=1.0).text("flutter")).on_hover_text("A leaves' quick shiver");
                    mark("uiPropFlutter", &r);
                });
            }
        }
        // a railing's piece: its style, a span or a post, what its ends say or what it takes
        // and where it stands, how likely it's picked (`world::railings` puts them together)
        if after.category == "railing" {
            let r = after.railing.get_or_insert_with(|| content::RailingPiece { style: content::DEFAULT_RAILING.into(), ..Default::default() });
            section(ui, "Railing piece");
            ui.horizontal(|ui| {
                ui.label("Style");
                ui.add(egui::TextEdit::singleline(&mut r.style).desired_width(110.0)).on_hover_text("The pieces of one style are put together into a railing");
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut r.part, "span".to_string(), "Span").on_hover_text("Along one cell edge: modelled from x -0.5 to 0.5, z across");
                ui.radio_value(&mut r.part, "post".to_string(), "Post").on_hover_text("On a point where spans meet or end: at the origin, +x along a span");
            });
            let words = |ui: &mut egui::Ui, list: &mut Vec<String>, about: &str| {
                let mut text = list.join(", ");
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0)).on_hover_text(about).changed() {
                    *list = text.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
                }
            };
            if r.part == "span" {
                if r.ends.len() != 2 {
                    r.ends = vec!["rail".into(), "rail".into()];
                }
                ui.horizontal(|ui| {
                    ui.label("Ends");
                    ui.add(egui::TextEdit::singleline(&mut r.ends[0]).desired_width(60.0)).on_hover_text("What its -x end needs of the point there (a word a post takes: \"rail\", \"post\"...)");
                    ui.add(egui::TextEdit::singleline(&mut r.ends[1]).desired_width(60.0)).on_hover_text("What its +x end needs");
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("Takes");
                    words(ui, &mut r.takes, "The span ends it stands between, comma-separated");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Stands at");
                    for j in content::JUNCTIONS {
                        let mut on = r.at.iter().any(|a| a == j);
                        if ui.checkbox(&mut on, j).changed() {
                            r.at.retain(|a| a != j);
                            if on {
                                r.at.push(j.to_string());
                            }
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Weight");
                ui.add(egui::DragValue::new(&mut r.weight).speed(0.1).range(0.1..=100.0)).on_hover_text("How likely it's picked among the pieces that fit");
            });
        }
        // its light: the window
        ui.horizontal(|ui| {
            let light = ui.button(if p.light.is_some() { "Light…" } else { "Give it a light…" });
            mark("uiPropLight", &light);
            if light.clicked() {
                self.assets.light_edit = Some(id.to_string());
            }
            if let Some(l) = &p.light {
                swatch(ui, &l.color);
                hint(ui, &format!("{} ×{:.1}, {:.0} cells{}", l.kind, l.intensity, l.range, if l.night { ", at night" } else { "" }));
            }
        });
        // its sound: where its prop stands, looping
        match &mut after.sound {
            Some(s) => {
                ui.horizontal(|ui| {
                    ui.label("Sound");
                    ui.add(egui::TextEdit::singleline(&mut s.file).desired_width(140.0)).on_hover_text("A file under assets/ (sounds/hum.wav)");
                });
                ui.add(egui::Slider::new(&mut s.volume, 0.0..=2.0).text("volume"));
                ui.add(egui::Slider::new(&mut s.range, 1.0..=24.0).text("range").suffix(" cells"));
                if ui.small_button("No sound").clicked() {
                    after.sound = None;
                }
            }
            None => {
                let give = ui.button("Give it a sound…").on_hover_text("A loop heard where it stands");
                mark("uiPropSound", &give);
                if give.clicked() {
                    after.sound = Some(content::Sound { file: "sounds/hum.wav".into(), ..Default::default() });
                }
            }
        }
        // the asset browser's folders it's in, and moving it to another
        let folders = after.folders();
        ui.horizontal_wrapped(|ui| {
            ui.label("In");
            for f in &folders {
                ui.label(RichText::new(f).size(11.0).color(Color32::from_gray(190)));
            }
        });
        let mut move_to = None;
        let all = self.asset_folders();
        let combo = egui::ComboBox::from_id_salt(("prop-move", id)).selected_text("Move to a folder…").width(170.0).show_ui(ui, |ui| {
            for f in &all {
                let r = ui.selectable_label(folders.contains(f), f);
                mark(&format!("uiMoveTo_{}", f.replace(['/', ' '], "_")), &r);
                if r.clicked() {
                    move_to = Some(f.clone());
                }
            }
        });
        mark("uiPropMoveTo", &combo.response);
        if let Some(f) = move_to {
            after.folders = vec![f];
        }
        // its model, and Blender
        ui.horizontal(|ui| {
            match &p.model {
                Some(m) => hint(ui, &format!("Model {m}")),
                None => hint(ui, &format!("A stand-in of {} shape{}", p.parts.len(), if p.parts.len() == 1 { "" } else { "s" })),
            }
        });
        ui.horizontal(|ui| {
            let edit = ui.button("Edit in Blender").on_hover_text(if cfg!(target_arch = "wasm32") {
                "Downloads it as a .glb for Blender; the add-on's Add to the project writes it back"
            } else {
                "Opens Blender on it (BLENDER names the program, else the PATH or Program Files); Add to the project with Replace sends it back, and it shows here within a second"
            });
            mark("uiEditBlender", &edit);
            if edit.clicked() {
                self.edit_in_blender(id);
            }
            let export = ui.small_button("To Blender").on_hover_text("A .glb of it, standing at the origin");
            if export.clicked() {
                self.export_prop(id);
            }
        });
        if after != p {
            self.change_prop(id, move |q| *q = after);
            self.need_props = true;
        }
    }

    /// The window where a prop's light is set.
    pub(super) fn light_window(&mut self, ui: &mut egui::Ui) {
        crate::platform::set_hook("lightedit", self.assets.light_edit.as_deref().unwrap_or(""));
        let Some(id) = self.assets.light_edit.clone() else { return };
        let Some(p) = self.content.props.get(&id).cloned() else {
            self.assets.light_edit = None;
            return;
        };
        let mut light = p.light.clone().unwrap_or_default();
        let (mut changed, mut remove, mut open) = (false, false, true);
        egui::Window::new(format!("Light of {}", p.name)).id(egui::Id::new("light-edit")).open(&mut open).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
            hint(ui, "A spot is a cone of light from where it sits, the way it points; a point lights all around it. Neither casts a shadow.");
            egui::Grid::new("light-grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Kind");
                ui.horizontal(|ui| {
                    for kind in ["spot", "point"] {
                        if ui.selectable_label(light.kind == kind, kind).clicked() && light.kind != kind {
                            light.kind = kind.into();
                            changed = true;
                        }
                    }
                });
                ui.end_row();
                ui.label("Colour");
                let mut rgb = hex_rgb(&light.color);
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    light.color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
                    changed = true;
                }
                ui.end_row();
                ui.label("Intensity");
                changed |= ui.add(egui::Slider::new(&mut light.intensity, 0.0..=4.0)).changed();
                ui.end_row();
                ui.label("Range");
                changed |= ui.add(egui::Slider::new(&mut light.range, 1.0..=24.0).suffix(" cells")).changed();
                ui.end_row();
                if light.kind == "spot" {
                    ui.label("Cone");
                    changed |= ui.add(egui::Slider::new(&mut light.cone, 10.0..=170.0).suffix("°")).changed();
                    ui.end_row();
                }
                ui.label("Sits at");
                ui.horizontal(|ui| {
                    for v in light.at.iter_mut() {
                        changed |= ui.add(egui::DragValue::new(v).speed(0.05)).changed();
                    }
                });
                ui.end_row();
                if light.kind == "spot" {
                    ui.label("Points");
                    ui.horizontal(|ui| {
                        for v in light.aim.iter_mut() {
                            changed |= ui.add(egui::DragValue::new(v).speed(0.05)).changed();
                        }
                    });
                    ui.end_row();
                }
                ui.label("");
                changed |= ui.checkbox(&mut light.night, "At night only").changed();
                ui.end_row();
            });
            hint(ui, "x, y (up) and z from the prop's origin, before it's turned. Lamps light the Night preview, or always with Lamps on.");
            ui.horizontal(|ui| {
                if p.light.is_none() {
                    let give = ui.button("Give it this light");
                    mark("uiLightGive", &give);
                    changed |= give.clicked();
                } else if ui.button("Remove the light").clicked() {
                    remove = true;
                }
            });
        });
        if remove {
            self.change_prop(&id, |p| p.light = None);
            self.assets.light_edit = None;
        } else if changed {
            self.change_prop(&id, move |p| p.light = Some(light));
        }
        if !open {
            self.assets.light_edit = None;
        }
    }

    // ------------------------------------------------------------ saving

    /// Writes the props and creatures changed here: into their space on the desktop, as
    /// downloads in the browser (with the models dropped there).
    pub(super) fn save_assets(&mut self) {
        let ids: Vec<String> = self.assets.dirty.iter().cloned().collect();
        let creatures: Vec<String> = self.assets.creatures_dirty.iter().cloned().collect();
        let (mut done, mut done_creatures) = (0, 0);
        let mut errors = vec![];
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            for id in &ids {
                let Some(e) = self.content.props.entry(id) else { continue };
                match content::disk::save(&root, e.space, &e.value) {
                    Ok(_) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            for id in &creatures {
                let Some(e) = self.content.creatures.entry(id) else { continue };
                match content::disk::save(&root, e.space, &e.value) {
                    Ok(_) => done_creatures += 1,
                    Err(err) => errors.push(err),
                }
            }
            self.assets.seen = snapshot(&root);
            self.status = format!("Saved {done} prop{}", plural(done));
            if done_creatures > 0 {
                self.status = format!("{} and {done_creatures} creature{}", self.status, plural(done_creatures));
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            for id in &ids {
                let Some(e) = self.content.props.entry(id) else { continue };
                match platform::download(&format!("{id}.json"), &content::to_text(&e.value)) {
                    Ok(()) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            for id in &creatures {
                let Some(e) = self.content.creatures.entry(id) else { continue };
                match platform::download(&format!("{id}.json"), &content::to_text(&e.value)) {
                    Ok(()) => done_creatures += 1,
                    Err(err) => errors.push(err),
                }
            }
            let models = std::mem::take(&mut self.assets.new_models);
            for (name, bytes) in &models {
                if let Err(err) = platform::download_bytes(name, bytes, "model/gltf-binary") {
                    errors.push(err);
                }
            }
            self.status = format!("Downloaded {done} prop record{} (into <space>/data/props/) and {} model{} (into <space>/assets/models/)", plural(done), models.len(), plural(models.len()));
            if done_creatures > 0 {
                self.status = format!("{}, {done_creatures} creature record{} (into <space>/data/creatures/)", self.status, plural(done_creatures));
            }
        }
        if errors.is_empty() {
            self.assets.dirty.clear();
            self.assets.creatures_dirty.clear();
        } else {
            self.status = format!("{} · not saved: {}", self.status, errors.join("; "));
        }
    }

    // ------------------------------------------------------------ files dropped in

    /// Files hovering or dropped on the window: the browser opens, models become props.
    /// While the Requests window is open, pictures go to it instead.
    pub(super) fn take_drops(&mut self, ctx: &egui::Context) {
        let (hovered, files, pos) = ctx.input(|i| (i.raw.hovered_files.clone(), i.raw.dropped_files.clone(), i.pointer.latest_pos()));
        let to_requests = self.requests.open;
        let (pictures, others): (Vec<_>, Vec<_>) = hovered.iter().partition(|h| to_requests && requests::hovered_picture(h));
        self.requests.hovering = !pictures.is_empty();
        let hovering = !others.is_empty();
        if hovering {
            self.assets.open = true;
        }
        self.assets.hovering = hovering;
        let (pictures, files): (Vec<_>, Vec<_>) = files.into_iter().partition(|f| to_requests && requests::is_picture(&f.path().to_string_lossy()));
        if !pictures.is_empty() {
            self.requests.take(pictures);
        }
        if !files.is_empty() {
            self.assets.open = true;
            // the folder under the pointer, else the one shown
            let under = pos.and_then(|p| self.assets.rows.iter().find(|(_, r)| r.contains(p)).map(|(f, _)| f.clone()));
            self.assets.drop_to = under.or_else(|| self.assets.folder.clone()).unwrap_or_else(|| IMPORTED.into());
            platform::read_dropped(files, &self.assets.dropped);
        }
        let arrived = std::mem::take(&mut *self.assets.dropped.lock().unwrap());
        for (name, bytes) in arrived {
            match bytes {
                Ok(b) => self.add_model(&name, b),
                Err(e) => self.status = format!("{name}: {e}"),
            }
        }
    }

    /// A model file made into a prop: its size measured, in the folder it was dropped on.
    pub(super) fn add_model(&mut self, name: &str, bytes: Vec<u8>) {
        let lower = name.to_lowercase();
        let ext = if lower.ends_with(".glb") {
            "glb"
        } else if lower.ends_with(".gltf") {
            "gltf"
        } else if requests::is_picture(name) {
            self.status = format!("{name}: only glTF models (.glb, .gltf) become props; for a model made from a picture, drop it on the Requests window");
            return;
        } else {
            self.status = format!("{name}: only glTF models (.glb, .gltf) become props");
            return;
        };
        let stem = &name[..name.len() - ext.len() - 1];
        let base = match slug(stem) {
            s if s.is_empty() => "model".to_string(),
            s => s,
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.content.props.contains(&id) {
            id = format!("{base}_{n}");
            n += 1;
        }
        let model = match load_model(&bytes, &format!("dropped/{id}")) {
            Ok(m) => m,
            Err(e) => {
                self.status = format!("{name}: not a model I can read ({e})");
                return;
            }
        };
        let size = model.max - model.min;
        let folder = self.assets.drop_to.clone();
        let space = if self.assets.to_temporary { Space::Temporary } else { Space::Content };
        let file = format!("models/{id}.{ext}");
        let prop = Prop {
            id: id.clone(),
            name: title(stem),
            category: category_for(&folder).into(),
            footprint: [size.x.ceil().clamp(1.0, 32.0) as u8, size.z.ceil().clamp(1.0, 32.0) as u8],
            height: ((size.y as f64 * 20.0).round() / 20.0).max(0.05),
            blocks: true,
            model: Some(file.clone()),
            // its first light (a lamp made in Blender lights up here too)
            light: model.lights.first().cloned(),
            folders: vec![folder.clone()],
            ..Default::default()
        };
        let note = world::gltf_in::extras_note(&model);
        let path = space.asset(&file);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            let full = root.join(&path);
            let written = full.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| std::fs::write(&full, &bytes)).map_err(|e| e.to_string());
            let saved = written.and_then(|_| content::disk::save(&root, space, &prop));
            if let Err(e) = saved {
                self.status = format!("{name}: not added ({e})");
                return;
            }
            self.assets.seen = snapshot(&root);
            self.status = format!("Added {}: {path} and {}/data/props/{id}.json, in {folder}", prop.name, space.dir());
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.assets.new_models.push((format!("{id}.{ext}"), bytes.clone()));
            self.assets.dirty.insert(id.clone());
            self.status = format!("Added {} to {folder}: save to download its record and model", prop.name);
        }
        self.files.insert(&path, bytes);
        if !note.is_empty() {
            self.status = format!("{} · {note}", self.status);
        }
        let mid = (model.min + model.max) * 0.5;
        if mid.x.abs() > 0.25 || mid.z.abs() > 0.25 || model.min.y.abs() > 0.1 {
            self.status = format!(
                "{} · its middle is {:.2}, {:.2} off the origin and its bottom at {:.2}: centre it in Blender (the add-on's Add to project does), or it stands off its cells",
                self.status, mid.x, mid.z, model.min.y
            );
        }
        let mut c = (*self.content).clone();
        c.props.put(prop, space);
        c.sort();
        self.content = Arc::new(c);
        self.assets.made.remove(&folder);
        self.assets.folder = Some(folder);
        self.prop = Some(id);
        crate::platform::set_hook("dropped", name);
    }

    // ------------------------------------------------------------ watching the disk

    /// The desktop: what changed on disk among the props' records and models shows at once.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn watch_assets(&mut self, now: f64) {
        if now < self.assets.next_look {
            return;
        }
        self.assets.next_look = now + 1000.0;
        let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
        let seen = snapshot(&root);
        if self.assets.seen.is_empty() || seen == self.assets.seen {
            self.assets.seen = seen;
            return;
        }
        let before: std::collections::HashMap<_, _> = self.assets.seen.iter().cloned().collect();
        let now_set: std::collections::HashSet<_> = seen.iter().map(|(p, _)| p.clone()).collect();
        let mut changed: Vec<std::path::PathBuf> = seen.iter().filter(|(p, t)| before.get(p) != Some(t)).map(|(p, _)| p.clone()).collect();
        changed.extend(before.keys().filter(|p| !now_set.contains(*p)).cloned());
        self.assets.seen = seen;
        if changed.iter().any(|p| p.extension().map(|e| e == "json").unwrap_or(false)) {
            // the records again, but those changed here and not saved yet
            let disk = content::disk::load(&root);
            let mut c = (*self.content).clone();
            let keep: Vec<_> = self.assets.dirty.iter().filter_map(|id| c.props.entry(id).cloned()).collect();
            c.props = disk.props;
            for e in keep {
                c.props.put(e.value, e.space);
            }
            c.sort();
            self.content = Arc::new(c);
        }
        // the models whose files changed load again
        let content = self.content.clone();
        for e in content.props.entries() {
            let Some(m) = &e.value.model else { continue };
            let path = e.space.asset(m);
            if changed.iter().any(|c| c == &root.join(&path)) || !self.models.contains_key(&e.value.id) {
                self.models.remove(&e.value.id);
                self.model_failed.remove(&e.value.id);
                self.files.forget(&path);
            }
        }
        self.need_props = true;
        self.need_trees = true;
        self.need_buildings = true;
        self.extra_changed = true;
        self.blocked = None;
        let names: Vec<String> = changed.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect();
        self.status = format!("Changed on disk, shown: {}", names.join(", "));
    }
}

/// The props' records and models on disk, with when they last changed.
#[cfg(not(target_arch = "wasm32"))]
fn snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, std::time::SystemTime)> {
    let mut out = vec![];
    for space in Space::ALL {
        for sub in ["data/props", "assets/models"] {
            let Ok(dir) = std::fs::read_dir(root.join(space.dir()).join(sub)) else { continue };
            for e in dir.flatten() {
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    out.push((e.path(), t));
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_files() {
        assert_eq!(slug("Garden Lamp"), "garden_lamp");
        assert_eq!(slug("  Old--Tree.v2 "), "old_tree_v2");
        assert_eq!(slug("***"), "");
        assert_eq!(title("stone_well-2"), "Stone well 2");
        assert_eq!(title(""), "Model");
    }

    #[test]
    fn folders_hold_their_subfolders() {
        let p = Prop { folders: vec!["Buildings/Inside".into()], ..Default::default() };
        assert!(in_folder(&p, "Buildings/Inside"));
        assert!(in_folder(&p, "Buildings"));
        assert!(!in_folder(&p, "Buildings/In"));
        assert!(!in_folder(&p, "Nature"));
        // no folders of its own: its category's
        let tree = Prop { category: "tree".into(), ..Default::default() };
        assert!(in_folder(&tree, "Nature"));
    }

    #[test]
    fn the_watch_sees_a_change() {
        let root = std::env::temp_dir().join(format!("bf-assets-{}", std::process::id()));
        let props = root.join("content").join("data").join("props");
        std::fs::create_dir_all(&props).unwrap();
        std::fs::write(props.join("a.json"), "{}").unwrap();
        let before = snapshot(&root);
        assert_eq!(before.len(), 1);
        std::fs::write(props.join("b.json"), "{}").unwrap();
        let after = snapshot(&root);
        assert_eq!(after.len(), 2);
        assert_ne!(before, after);
        std::fs::remove_dir_all(&root).ok();
    }
}
