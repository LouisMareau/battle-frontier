//! The Ground tool's terrains (`content::Terrain`): listed by kind, a group folding wherever
//! two or more share one (`Terrain::category_name`: its category, else its pattern's name); the
//! one picked to paint duplicated (a variant of it, say to grow something else), made anew (a
//! kind of its own, from a preset), or deleted (one no map uses); and the Terrain window
//! (Edit…), where one is named, put in a group, walked or not, coloured and drawn: by one of
//! the engine's patterns, or by a look of its own (`content::Look`: its two colours in
//! blotches, layers over them, each glowing, drifting, pulsing, raised or sunken). The presets
//! are kinds of their own: volcanic rock, a lava lake, arcane moss (its meadow glowing too),
//! a crystal field, snow, ice, a swamp, autumn leaves, dunes, ash, mossy flagstones and
//! glowing moss. What it changes shows on the map at once (the palette goes again:
//! `stream`); it's saved with the rest (Ctrl+S, or by itself).

use std::sync::Arc;

use content::{Layer, Look, Meadow, Plant, Space, Terrain, LAYER_SHAPES, PATTERNS};
use egui::{RichText, Sense};

use super::{card, fieldset, fieldset_with, fold_row, fold_section, fold_section_with, form, hint, mark, row_icon, sign_button, swatch, warn, MapEditor, Sign, ARROW, ROW, SWATCH};

/// The Terrain window: open for which terrain, and whether its deletion is being asked.
#[derive(Default)]
pub(super) struct TerrainWindow {
    pub id: Option<String>,
    pub deleting: bool,
}

/// What a layer's shape is called in the window, by `LAYER_SHAPES`.
const SHAPE_NAMES: [&str; 7] = ["Patches", "Stones", "Cracks", "Dots", "Streaks", "Veins", "Ripples"];

/// A record id from a name: its letters and digits in lower case, the rest `_`, free among
/// those `taken` says are (`_2`, `_3`... after it when it isn't).
pub(super) fn free_id(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut base: String = name.trim().chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    let base = base.trim_matches('_').to_string();
    let base = if base.is_empty() { "terrain".to_string() } else { base };
    if !taken(&base) {
        return base;
    }
    (2..).map(|k| format!("{base}_{k}")).find(|id| !taken(id)).unwrap()
}

/// A name free among those `taken` says are: `name` itself, else `name 2`, `name 3`...
fn free_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
    if !taken(name) {
        return name.to_string();
    }
    (2..).map(|k| format!("{stem} {k}")).find(|n| !taken(n)).unwrap()
}

/// The maps whose records name a terrain (in their legend, or their chunks' list).
pub(super) fn maps_using(content: &content::Content, id: &str) -> Vec<String> {
    content.maps.iter().filter(|m| m.terrains.iter().any(|t| t == id) || m.legend.values().any(|t| t == id)).map(|m| m.id.clone()).collect()
}

/// A layer of a look, quickly.
#[allow(clippy::too_many_arguments)]
fn layer(shape: &str, color: &str, scale: f64, amount: f64, softness: f64, glow: f64, pulse: f64, relief: f64) -> Layer {
    Layer { shape: shape.into(), color: color.into(), scale, amount, softness, glow, pulse, relief, ..Default::default() }
}

/// The kinds of terrain to start a new one from: each its name, colours, cliff, how it's walked
/// and its look (arcane moss with its glowing meadow).
pub(super) fn presets() -> Vec<Terrain> {
    let kind = |name: &str, walk: &str, colors: [&str; 3], look: Look| Terrain {
        name: name.into(),
        color: colors[0].into(),
        color2: colors[1].into(),
        side: colors[2].into(),
        pattern: "custom".into(),
        walk: walk.into(),
        priority: 1,
        look,
        ..Default::default()
    };
    let look = |scale: f64, sharpness: f64, grain: f64, shine: f64, layers: Vec<Layer>| Look { scale, sharpness, grain, shine, layers, ..Default::default() };
    let mut arcane = kind(
        "Arcane moss",
        "walk",
        ["#241a3e", "#34245c", "#2a2236"],
        look(2.0, 0.3, 0.25, 0.0, vec![layer("patches", "#2c4a3e", 1.5, 0.35, 0.4, 0.0, 0.0, 0.0), layer("veins", "#5ef2ff", 3.0, 0.35, 0.3, 2.2, 0.5, -0.2), layer("dots", "#d8a8ff", 0.45, 0.1, 0.2, 2.0, 1.2, 0.0)]),
    );
    arcane.growth.meadow = Meadow {
        plants: vec![
            Plant { name: "Dusk grass".into(), height: [0.12, 0.25], width: 0.03, color: "#2a1f45".into(), tip: "#6a50c0".into(), glow: 0.4, ..Default::default() },
            Plant {
                name: "Glowbell".into(),
                shape: "flower".into(),
                weight: 0.06,
                height: [0.18, 0.3],
                width: 0.012,
                lean: 0.1,
                bend: 0.15,
                petals: 5,
                petal: "#8af0ff".into(),
                heart: "#ffffff".into(),
                bloom: 0.08,
                clusters: 0.6,
                glow: 2.5,
                ..Default::default()
            },
        ],
        density: 36.0,
        ..Default::default()
    };
    let lava = Look {
        scale: 1.5,
        sharpness: 0.4,
        grain: 0.1,
        glow: 2.2,
        pulse: 0.25,
        flow: 0.08,
        angle: 30.0,
        layers: vec![
            Layer { flow: 0.08, angle: 30.0, ..layer("stones", "#2b2320", 1.1, 0.62, 0.25, 0.0, 0.0, 0.7) },
            Layer { flow: 0.08, angle: 30.0, ..layer("dots", "#ff8a20", 0.25, 0.05, 0.2, 3.0, 1.5, 0.0) },
        ],
        ..Default::default()
    };
    vec![
        kind("Volcanic rock", "walk", ["#2a2422", "#3a302c", "#2a2220"], look(2.5, 0.55, 0.35, 0.05, vec![layer("cracks", "#ff5a14", 1.6, 0.3, 0.35, 2.6, 0.35, -0.6), layer("dots", "#5e5650", 0.3, 0.3, 0.2, 0.0, 0.0, 0.3)])),
        kind("Lava lake", "block", ["#ff5a10", "#ffb030", "#2a2220"], lava),
        arcane,
        kind("Crystal field", "walk", ["#c6d4e6", "#9cb0ca", "#6e7a8e"], look(2.0, 0.5, 0.2, 0.7, vec![layer("stones", "#e6f2ff", 0.8, 0.55, 0.15, 0.25, 0.0, 0.6), layer("dots", "#7fe4ff", 0.35, 0.15, 0.1, 2.6, 0.7, 0.0)])),
        kind("Snow", "walk", ["#f2f6fb", "#d4dfec", "#8a94a6"], Look { balance: 0.4, ..look(3.5, 0.12, 0.1, 0.35, vec![layer("patches", "#c6d4e6", 2.5, 0.2, 0.6, 0.0, 0.0, -0.2), layer("dots", "#ffffff", 0.08, 0.06, 0.0, 0.8, 2.0, 0.0)]) }),
        kind("Ice", "walk", ["#bfe4f0", "#92c6dc", "#6a8ea0"], look(4.0, 0.2, 0.15, 0.95, vec![layer("cracks", "#eafcff", 1.4, 0.15, 0.2, 0.0, 0.0, -0.3), Layer { angle: 35.0, ..layer("streaks", "#a6d8ea", 2.0, 0.25, 0.5, 0.0, 0.0, 0.0) }])),
        kind("Swamp", "walk", ["#3a3422", "#2a2a1c", "#3a3020"], look(2.5, 0.3, 0.3, 0.45, vec![layer("patches", "#1e2a1c", 1.8, 0.35, 0.25, 0.0, 0.0, -0.5), layer("dots", "#4e6a2a", 0.25, 0.25, 0.3, 0.0, 0.0, 0.2)])),
        kind("Autumn leaves", "walk", ["#5a3e24", "#6e4a28", "#5a4a38"], look(2.0, 0.3, 0.3, 0.0, vec![layer("dots", "#c4501c", 0.16, 0.55, 0.15, 0.0, 0.0, 0.2), layer("dots", "#e2a02e", 0.15, 0.45, 0.15, 0.0, 0.0, 0.2), layer("dots", "#8e2c18", 0.18, 0.3, 0.15, 0.0, 0.0, 0.2)])),
        kind("Dunes", "walk", ["#e0c088", "#cca664", "#b08850"], look(4.0, 0.2, 0.15, 0.0, vec![Layer { angle: 20.0, ..layer("ripples", "#b89452", 0.45, 0.35, 0.5, 0.0, 0.0, 0.4) }, Layer { angle: 20.0, ..layer("streaks", "#f0d8a2", 3.0, 0.25, 0.6, 0.0, 0.0, 0.0) }])),
        kind("Ash waste", "walk", ["#575350", "#45413e", "#3a3634"], look(2.5, 0.35, 0.4, 0.0, vec![layer("patches", "#2c2a28", 1.8, 0.3, 0.4, 0.0, 0.0, -0.2), layer("dots", "#8c867f", 0.12, 0.2, 0.1, 0.0, 0.0, 0.2)])),
        kind("Mossy flagstones", "walk", ["#6c7660", "#59634e", "#5a5a50"], look(3.0, 0.3, 0.2, 0.0, vec![layer("stones", "#8c9282", 1.0, 0.82, 0.2, 0.0, 0.0, 0.5), layer("patches", "#4c7a36", 0.8, 0.35, 0.4, 0.0, 0.0, 0.1)])),
        kind("Glowing moss", "walk", ["#1e2a20", "#283a2a", "#2a2a22"], look(2.0, 0.3, 0.3, 0.0, vec![layer("patches", "#2e4a2e", 1.2, 0.45, 0.4, 0.0, 0.0, 0.1), layer("dots", "#7affb0", 0.22, 0.08, 0.2, 3.0, 0.5, 0.1)])),
    ]
}

/// "#rrggbb" edited with a colour picker.
fn colour(ui: &mut egui::Ui, hex: &mut String) -> bool {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x808080);
    let mut rgb = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
    let changed = ui.color_edit_button_srgb(&mut rgb).changed();
    if changed {
        *hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    }
    changed
}

impl MapEditor {
    /// The Ground tool's terrains, by kind (a group folding where two or more share one), each a
    /// row as wide as the panel: its colour and name, picked by a click (a double-click edits
    /// it), and at its right end Duplicate and Edit; `+` by the section's title makes a new kind
    /// from a preset. Folded, the section shows the one picked alone (under its group's name,
    /// when it's in one); a group folded, the one picked in it.
    pub(super) fn terrain_list(&mut self, ui: &mut egui::Ui) {
        crate::platform::set_hook("terrain", &self.terrain);
        let open = fold_section_with(ui, "Terrain", |ui| {
            let plus = egui::Image::new(egui::include_image!("../../icons/plus.svg")).fit_to_exact_size(egui::vec2(14.0, 14.0)).tint(ui.visuals().text_color());
            let (r, _) = egui::containers::menu::MenuButton::from_button(egui::Button::image(plus).frame(false)).ui(ui, |ui| {
                hint(ui, "A terrain of its own look, from:");
                for p in presets() {
                    let b = ui.button(&p.name);
                    mark(&format!("uiTerrainPreset_{}", free_id(&p.name, &|_| false)), &b);
                    if b.clicked() {
                        let id = self.new_terrain(p);
                        self.terrain = id.clone();
                        self.terrain_window = TerrainWindow { id: Some(id), deleting: false };
                        ui.close();
                    }
                }
            });
            let r = r.on_hover_text("New kind: a terrain of its own look (a base and layers, glowing, drifting...), from a preset to change as you like");
            mark("uiTerrainNew", &r);
        });
        let mut all: Vec<Terrain> = self.content.terrains.iter().cloned().collect();
        all.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
        // the groups, each where its first one is
        let mut groups: Vec<(String, Vec<Terrain>)> = vec![];
        for t in all {
            let name = t.category_name();
            match groups.iter_mut().find(|(n, _)| *n == name) {
                Some((_, members)) => members.push(t),
                None => groups.push((name, vec![t])),
            }
        }
        crate::platform::set_hook("terraingroups", &groups.iter().filter(|(_, m)| m.len() > 1).map(|(n, m)| format!("{n}:{}", m.len())).collect::<Vec<_>>().join(";"));
        let picked = self.terrain.clone();
        let mut shown = vec![];
        for (name, members) in &groups {
            let has_picked = members.iter().any(|t| t.id == picked);
            if !open && !has_picked {
                continue;
            }
            if members.len() == 1 {
                self.terrain_row(ui, &members[0], false);
                shown.push(members[0].id.clone());
                continue;
            }
            // a group: the colour of its first (the others are its variants), its name
            let head = |ui: &mut egui::Ui| {
                swatch(ui, &members[0].color);
                ui.add(egui::Label::new(RichText::new(format!("{name} ({})", members.len())).strong()).selectable(false));
            };
            let left = ui.cursor().left();
            // (open until folded; the section folded, its name alone over the one picked)
            let (all_of_it, top) = if open {
                let (unfolded, r) = fold_row(ui, egui::Id::new(("terrain group", name)), true, head);
                mark(&format!("uiTerrainGroup_{}", free_id(name, &|_| false)), &r);
                (unfolded, r.rect.bottom())
            } else {
                let r = ui.horizontal(|ui| {
                    ui.set_height(ROW);
                    ui.add_space(ARROW);
                    head(ui);
                });
                (false, r.response.rect.bottom())
            };
            // its terrains under its name, a line down to them from its colour
            let mut last = top;
            for t in members.iter().filter(|t| all_of_it || t.id == picked) {
                last = self.terrain_row(ui, t, true).center().y;
                shown.push(t.id.clone());
            }
            let x = (left + ARROW + SWATCH / 2.0).round() + 0.5;
            ui.painter().vline(x, top - 3.0..=last, egui::Stroke::new(1.0, egui::Color32::from_gray(80)));
        }
        crate::platform::set_hook("terrainsshown", &shown.join(";"));
    }

    /// A terrain's row in the list, as wide as the panel: its colour (in a group, none: the
    /// group's is over it) and its name, picked by a click, edited by a double-click; at its
    /// right end Duplicate and Edit (marked `uiTerrainDuplicate_<id>`, `uiTerrainEdit_<id>`).
    /// Where it is.
    fn terrain_row(&mut self, ui: &mut egui::Ui, t: &Terrain, in_group: bool) -> egui::Rect {
        // (its own id: a row keeps its focus as others come and go)
        ui.push_id(&t.id, |ui| {
            let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), ROW));
            // the row first: its buttons, drawn after, take their own clicks
            let r = ui.interact(rect, ui.id().with("row"), Sense::click());
            let picked = self.terrain == t.id;
            let lit = picked || r.contains_pointer();
            let v = ui.visuals().clone();
            if picked {
                ui.painter().rect_filled(rect, 3.0, v.selection.bg_fill);
            } else if r.contains_pointer() {
                ui.painter().rect_filled(rect, 3.0, v.widgets.hovered.weak_bg_fill);
            }
            let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
            inside.add_space(ARROW);
            if in_group {
                inside.add_space(SWATCH + inside.spacing().item_spacing.x);
            } else {
                swatch(&mut inside, &t.color);
            }
            let extra = if t.walk == "walk" || t.name.to_lowercase().contains(&t.walk) { String::new() } else { format!(" · {}", t.walk) };
            let (dup, edit) = inside
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(2.0);
                    let edit = row_icon(ui, egui::include_image!("../../icons/edit.svg"), lit, "Edit: its name, group, colours, how it's walked and drawn (double-click it too)", &format!("uiTerrainEdit_{}", t.id));
                    let dup = row_icon(ui, egui::include_image!("../../icons/duplicate.svg"), lit, "Duplicate: a variant of it, in its group (the same ground growing other grass, flowers or models)", &format!("uiTerrainDuplicate_{}", t.id));
                    let text = RichText::new(format!("{}{extra}", t.name)).color(if picked { v.selection.stroke.color } else { v.text_color() });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(text).truncate().selectable(false));
                        self.change_mark(ui, "terrains", &t.id, "");
                    });
                    (dup.clicked(), edit.clicked())
                })
                .inner;
            mark(&format!("uiTerrain_{}", t.id), &r);
            if r.clicked() {
                self.terrain = t.id.clone();
            }
            if r.double_clicked() || edit {
                self.terrain = t.id.clone();
                self.terrain_window = TerrainWindow { id: Some(t.id.clone()), deleting: false };
            }
            if dup {
                if let Some(new) = self.duplicate_terrain(&t.id) {
                    self.terrain = new;
                }
            }
            rect
        })
        .inner
    }

    /// A copy of a terrain: its own id and name, in its group (next to it: as early in the
    /// list), saved with the rest. Its id.
    pub(super) fn duplicate_terrain(&mut self, id: &str) -> Option<String> {
        let t = self.content.terrains.get(id)?.clone();
        let names: Vec<String> = self.content.terrains.iter().map(|t| t.name.clone()).collect();
        let name = free_name(&format!("{} 2", t.name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end()), &|n| names.iter().any(|m| m == n));
        let content = self.content.clone();
        let new_id = free_id(&name, &|i| content.terrains.contains(i));
        let copy = Terrain { id: new_id.clone(), name, category: t.category_name(), ..t };
        let mut c = (*self.content).clone();
        c.terrains.put(copy, Space::Content);
        self.content = Arc::new(c);
        self.terrains_dirty.insert(new_id.clone());
        self.status = format!("Duplicated {}: {new_id}", id);
        Some(new_id)
    }

    /// A terrain of a kind of its own (a preset's), its id free, last in the list, saved with
    /// the rest. Its id.
    fn new_terrain(&mut self, preset: Terrain) -> String {
        let names: Vec<String> = self.content.terrains.iter().map(|t| t.name.clone()).collect();
        let name = free_name(&preset.name, &|n| names.iter().any(|m| m == n));
        let content = self.content.clone();
        let id = free_id(&name, &|i| content.terrains.contains(i));
        let order = self.content.terrains.iter().map(|t| t.order).max().unwrap_or(0).saturating_add(1);
        let mut c = (*self.content).clone();
        c.terrains.put(Terrain { id: id.clone(), name, order, ..preset }, Space::Content);
        self.content = Arc::new(c);
        self.terrains_dirty.insert(id.clone());
        self.status = format!("A new kind of terrain: {id}");
        id
    }

    /// A terrain taken out of the project (its file too, on the desktop; in the browser, kept
    /// as removed), when no map uses it.
    fn delete_terrain(&mut self, id: &str) {
        let Some(e) = self.content.terrains.entry(id).cloned() else { return };
        if !maps_using(&self.content, id).is_empty() || self.map.terrains.iter().any(|t| t == id) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            let _ = std::fs::remove_file(root.join(e.space.dir()).join("data").join("terrains").join(format!("{id}.json")));
        }
        #[cfg(target_arch = "wasm32")]
        if super::saving::keeps() {
            crate::keep::keep_removed(&format!("{}/data/terrains/{id}.json", e.space.dir()));
        }
        let mut c = (*self.content).clone();
        c.terrains.remove(id);
        self.content = Arc::new(c);
        self.terrains_dirty.remove(id);
        if self.terrain == id {
            self.terrain = self.content.terrains.iter().next().map(|t| t.id.clone()).unwrap_or_default();
        }
        self.status = format!("Terrain {} deleted", e.value.name);
    }

    /// The Terrain window (Edit…, New kind…), when it's open: the terrain's name, group, how
    /// it's walked; its colours, how far it reaches and how it's drawn (a look of its own: its
    /// base, its glow and its layers); under them, where they scroll, Close and deleting it, when
    /// no map uses it.
    pub(super) fn terrain_window(&mut self, ctx: &egui::Context) {
        crate::platform::set_hook("terrainwindow", self.terrain_window.id.as_deref().unwrap_or(""));
        let Some(id) = self.terrain_window.id.clone() else { return };
        let Some(before) = self.content.terrains.get(&id).cloned() else {
            self.terrain_window = TerrainWindow::default();
            return;
        };
        let mut t = before.clone();
        let mut open = true;
        let (mut delete, mut closed) = (false, false);
        let used = maps_using(&self.content, &id);
        let painted = self.map.terrains.contains(&id);
        // as tall as the screen leaves it: what's in it scrolls, its buttons stay under it
        let room = (ctx.content_rect().height() - 210.0).clamp(160.0, 600.0);
        egui::Window::new(format!("Terrain: {}", before.name)).id(egui::Id::new("terrain-edit")).open(&mut open).collapsible(false).resizable(false).default_pos([258.0, 60.0]).show(ctx, |ui| {
            // (clear of the scroll bar, which floats over what's under it)
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 8, top: 0, bottom: 0 }).show(ui, |ui| {
                ui.set_width(330.0);
                fieldset_with(ui, "Terrain", self.review_changes_in("terrains", &id, &["name", "category", "walk", "rigidity"]), |ui| {
                    form(ui, "terrain-what", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            self.change_mark(ui, "terrains", &id, "name");
                        });
                        let r = ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(f32::INFINITY));
                        mark("uiTerrainName", &r);
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.label("Group");
                            self.change_mark(ui, "terrains", &id, "category");
                        });
                        let derived = Terrain { category: String::new(), ..t.clone() }.category_name();
                        let r = ui.add(egui::TextEdit::singleline(&mut t.category).desired_width(f32::INFINITY).hint_text(derived)).on_hover_text("The group it's listed in with the terrains like it (none: its pattern's)");
                        mark("uiTerrainGroup", &r);
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.label("Walked");
                            self.change_mark(ui, "terrains", &id, "walk");
                        });
                        ui.horizontal(|ui| {
                            for (walk, about) in [("walk", "Walked on"), ("water", "Swum: needs surfing"), ("block", "Never walked on")] {
                                if ui.selectable_label(t.walk == walk, walk).on_hover_text(about).clicked() {
                                    t.walk = walk.into();
                                }
                            }
                        });
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.label("Rigidity");
                            self.change_mark(ui, "terrains", &id, "rigidity");
                        });
                        let r = ui.add(egui::Slider::new(&mut t.rigidity, 0.0..=1.0).fixed_decimals(2)).on_hover_text("How stiff what grows on it is (its meadow, its grass): 0 bends from its foot, swaying with the wind and giving way to whoever walks through it as far as they push; more and it's still that far up from the ground, its tip giving that much less; 1 never moves. The default for its meadow's plants: each may have its own (Grows here, a plant's Rigidity)");
                        mark("uiTerrainRigidity", &r);
                        ui.end_row();
                    });
                });
                fieldset_with(ui, "Drawn", self.review_changes_in("terrains", &id, &["color", "color2", "side", "priority", "pattern", "water", "beach"]), |ui| {
                    form(ui, "terrain-drawn", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Colours");
                            self.change_mark(ui, "terrains", &id, "color");
                            self.change_mark(ui, "terrains", &id, "color2");
                            self.change_mark(ui, "terrains", &id, "side");
                        });
                        ui.horizontal(|ui| {
                            colour(ui, &mut t.color);
                            if t.color2.is_empty() {
                                t.color2 = t.color.clone();
                            }
                            colour(ui, &mut t.color2);
                            ui.label("cliffs");
                            if t.side.is_empty() {
                                t.side = "#8a6a48".into();
                            }
                            colour(ui, &mut t.side);
                        });
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.label("Reaches");
                            self.change_mark(ui, "terrains", &id, "priority");
                        });
                        ui.add(egui::Slider::new(&mut t.priority, -5..=5)).on_hover_text("Where two terrains meet, the one with the higher reaches further");
                        ui.end_row();
                        ui.horizontal(|ui| {
                            ui.label("Drawn as");
                            self.change_mark(ui, "terrains", &id, "pattern");
                        });
                        let shown = PATTERNS.iter().find(|(p, _)| *p == t.pattern).map(|(_, n)| *n).unwrap_or("Plain");
                        let combo = egui::ComboBox::from_id_salt("terrain-pattern").selected_text(shown).show_ui(ui, |ui| {
                            for (p, n) in PATTERNS {
                                let r = ui.selectable_value(&mut t.pattern, p.to_string(), n);
                                mark(&format!("uiTerrainPattern_{p}"), &r);
                            }
                        });
                        mark("uiTerrainPattern", &combo.response);
                        ui.end_row();
                        if t.water_kind().is_some() || t.pattern == "water" {
                            ui.label("Water");
                            ui.horizontal(|ui| {
                                for (w, about) in [("inland", "Lakes, ponds, rivers: ripples with the wind"), ("oceanic", "The sea: waves always rolling in")] {
                                    let on = t.water_kind() == Some(w);
                                    if ui.selectable_label(on, w).on_hover_text(about).clicked() {
                                        t.water = w.into();
                                    }
                                }
                            });
                            ui.end_row();
                        } else {
                            ui.label("");
                            ui.checkbox(&mut t.beach, "Water washes up onto it").on_hover_text("Where water meets it, the water washes up onto it and back, as onto sand");
                            ui.end_row();
                        }
                    });
                    if t.pattern != "custom" {
                        hint(ui, "Drawn by one of the engine's patterns in its two colours. Its own look draws it as you design it: a base, its glow and layers.");
                    }
                });
                if t.pattern == "custom" {
                    self.look_section(ui, &mut t);
                }
            }));
            // under what scrolls: always there
            ui.separator();
            let deletable = used.is_empty() && !painted;
            ui.horizontal(|ui| {
                let close = ui.button("Close");
                mark("uiTerrainClose", &close);
                closed = close.clicked();
                if deletable && !self.terrain_window.deleting {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let r = ui.button("Delete…").on_hover_text("No map uses it: it can go");
                        mark("uiTerrainDelete", &r);
                        if r.clicked() {
                            self.terrain_window.deleting = true;
                        }
                    });
                }
            });
            if deletable && self.terrain_window.deleting {
                warn(ui, &format!("Delete {}? No map uses it.", t.name));
                ui.horizontal(|ui| {
                    let yes = ui.button("Delete it");
                    mark("uiTerrainDeleteYes", &yes);
                    delete = yes.clicked();
                    if ui.button("Keep it").clicked() {
                        self.terrain_window.deleting = false;
                    }
                });
            } else if !deletable {
                let maps = if painted && !used.contains(&self.map.id) { [vec![self.map.id.clone()], used.clone()].concat() } else { used.clone() };
                hint(ui, &format!("Painted on {}: it stays while a map uses it.", maps.join(", ")));
            }
        });
        if delete {
            self.delete_terrain(&id);
            self.terrain_window = TerrainWindow::default();
            return;
        }
        if !open || closed {
            self.terrain_window = TerrainWindow::default();
        }
        if t != before {
            self.change_terrain(&id, move |x| *x = t);
        }
    }

    /// A look of its own in the Terrain window, a section that folds: from a preset; its base
    /// (its two colours in blotches, grain, shine) and its glow (pulsing, drifting) each a box;
    /// its layers, each a card (its shape: what it is, colour, size, how much, softness, relief;
    /// its glow: pulse, drift and its way), moved up or taken off; one more.
    fn look_section(&mut self, ui: &mut egui::Ui, t: &mut Terrain) {
        if !fold_section(ui, "Its look") {
            return;
        }
        let id = t.id.clone();
        if let Some((kind, about)) = self.review_changes("terrains", &id, "look") {
            ui.horizontal(|ui| {
                super::review::dot(ui, kind, &about);
                hint(ui, "changed in the pull request (hover the dot)");
            });
        }
        ui.horizontal(|ui| {
            let r = ui.menu_button("From a preset…", |ui| {
                for p in presets() {
                    if ui.button(&p.name).clicked() {
                        t.look = p.look.clone();
                        (t.color, t.color2, t.side) = (p.color.clone(), p.color2.clone(), p.side.clone());
                        ui.close();
                    }
                }
            });
            r.response.on_hover_text("Its colours, cliffs and look as a preset has them (its name, group and what grows here stay)");
        });
        let l = &mut t.look;
        fieldset_with(ui, "Base", self.review_changes_in("terrains", &id, &["look.scale", "look.sharpness", "look.balance", "look.grain", "look.shine"]), |ui| {
            form(ui, "look-base", |ui| {
                ui.label("Blotches");
                ui.add(egui::DragValue::new(&mut l.scale).range(0.1..=32.0).speed(0.05).suffix(" cells")).on_hover_text("How big the blotches of its two colours are");
                ui.end_row();
                ui.label("Sharpness");
                ui.add(egui::Slider::new(&mut l.sharpness, 0.0..=1.0)).on_hover_text("How sharp their edges are");
                ui.end_row();
                ui.label("Second colour");
                ui.add(egui::Slider::new(&mut l.balance, 0.0..=1.0)).on_hover_text("How much of it is its second colour");
                ui.end_row();
                ui.label("Grain");
                ui.add(egui::Slider::new(&mut l.grain, 0.0..=1.0)).on_hover_text("How much its small specks vary in brightness");
                ui.end_row();
                ui.label("Shine");
                ui.add(egui::Slider::new(&mut l.shine, 0.0..=1.0)).on_hover_text("How glossy: wet, icy, glassy (it catches the sun)");
                ui.end_row();
            });
        });
        fieldset_with(ui, "Glow", self.review_changes_in("terrains", &id, &["look.glow", "look.pulse", "look.flow", "look.angle"]), |ui| {
            form(ui, "look-glow", |ui| {
                ui.label("Glow");
                let r = ui.add(egui::Slider::new(&mut l.glow, 0.0..=4.0)).on_hover_text("How much its colours glow of themselves, lit by nothing (lava): at night as by day");
                mark("uiLookGlow", &r);
                ui.end_row();
                ui.label("Pulse");
                ui.add(egui::Slider::new(&mut l.pulse, 0.0..=2.0)).on_hover_text("How fast its glow pulses (0: steady)");
                ui.end_row();
                ui.label("Drift");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut l.flow).range(0.0..=2.0).speed(0.01).suffix(" cells/s")).on_hover_text("How fast it drifts (flowing lava, mist)");
                    ui.add(egui::DragValue::new(&mut l.angle).range(-360.0..=360.0).speed(1.0).suffix("°")).on_hover_text("Its way: clockwise from east, seen from above");
                });
                ui.end_row();
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new("Layers").strong());
        let mut gone = None;
        let mut up = None;
        let n = l.layers.len();
        for k in 0..n {
            let name = SHAPE_NAMES.get(LAYER_SHAPES.iter().position(|s| *s == l.layers[k].shape).unwrap_or(0)).copied().unwrap_or("Layer");
            let title = format!("{}. {name}{}", k + 1, if l.layers[k].glow > 0.0 { " · glowing" } else { "" });
            let changed = self.review_changes("terrains", &id, &format!("look.layers[{k}]"));
            card(
                ui,
                egui::Id::new(("look layer", &t.id, k)),
                &format!("uiLookLayer_{k}"),
                &title,
                &mut l.layers[k],
                |ui, y| {
                    swatch(ui, &y.color);
                    if let Some((kind, about)) = &changed {
                        super::review::dot(ui, *kind, about);
                    }
                },
                |ui, _| {
                    let x = sign_button(ui, Sign::Cross, true).on_hover_text("Take it off");
                    mark(&format!("uiLookLayerRemove_{k}"), &x);
                    if x.clicked() {
                        gone = Some(k);
                    }
                    if sign_button(ui, Sign::Up, k > 0).on_hover_text("Drawn before the one above").clicked() {
                        up = Some(k);
                    }
                },
                |ui, y| {
                    fieldset(ui, "Shape", |ui| {
                        form(ui, ("look-layer-shape", k), |ui| {
                            ui.label("Shape");
                            egui::ComboBox::from_id_salt(("look-shape", k)).selected_text(name).show_ui(ui, |ui| {
                                for (s, n) in LAYER_SHAPES.iter().zip(SHAPE_NAMES) {
                                    ui.selectable_value(&mut y.shape, s.to_string(), n);
                                }
                            });
                            ui.end_row();
                            ui.label("Colour");
                            colour(ui, &mut y.color);
                            ui.end_row();
                            ui.label("Size");
                            ui.add(egui::DragValue::new(&mut y.scale).range(0.02..=32.0).speed(0.01).suffix(" cells")).on_hover_text("How big what it's made of is: a stone, a dot, a crack's cells");
                            ui.end_row();
                            ui.label("How much");
                            ui.add(egui::Slider::new(&mut y.amount, 0.0..=1.0)).on_hover_text("How much of the ground it covers (dots: how many; cracks and veins: how wide)");
                            ui.end_row();
                            ui.label("Softness");
                            ui.add(egui::Slider::new(&mut y.softness, 0.0..=1.0)).on_hover_text("How soft its edges are");
                            ui.end_row();
                            ui.label("Relief");
                            ui.add(egui::Slider::new(&mut y.relief, -1.0..=1.0)).on_hover_text("Sunken (below 0) or raised (above): how the light falls on it");
                            ui.end_row();
                        });
                    });
                    fieldset(ui, "Glow", |ui| {
                        form(ui, ("look-layer-glow", k), |ui| {
                            ui.label("Glow");
                            ui.add(egui::Slider::new(&mut y.glow, 0.0..=4.0)).on_hover_text("How much it glows of itself, lit by nothing");
                            ui.end_row();
                            ui.label("Pulse");
                            ui.add(egui::Slider::new(&mut y.pulse, 0.0..=2.0)).on_hover_text("How fast its glow pulses (each thing of it its own time)");
                            ui.end_row();
                            ui.label("Drift");
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut y.flow).range(0.0..=2.0).speed(0.01).suffix(" cells/s"));
                                ui.add(egui::DragValue::new(&mut y.angle).range(-360.0..=360.0).speed(1.0).suffix("°")).on_hover_text("Its way (and streaks' and ripples'): clockwise from east");
                            });
                            ui.end_row();
                        });
                    });
                },
            );
        }
        if let Some(k) = gone {
            l.layers.remove(k);
        }
        if let Some(k) = up {
            l.layers.swap(k - 1, k);
        }
        ui.horizontal(|ui| {
            let r = ui.add_enabled(n < Look::LAYERS, egui::Button::new("+ Layer")).on_hover_text(format!("One more over it (at most {})", Look::LAYERS));
            mark("uiLookAddLayer", &r);
            if r.clicked() {
                l.layers.push(Layer { color: t.color2.clone(), ..Default::default() });
            }
            hint(ui, &format!("{n} of {} layers", Look::LAYERS));
        });
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use content::{Content, Map};

    use super::*;

    fn ground() -> Content {
        let mut c = Content::default();
        for (id, name, pattern, order) in [("grass", "Grass", "grass", 0), ("tall_grass", "Tall grass", "tall", 3), ("water", "Water", "water", 7), ("sea", "Sea", "water", 8)] {
            c.terrains.put(Terrain { id: id.into(), name: name.into(), pattern: pattern.into(), walk: if pattern == "water" { "water" } else { "walk" }.into(), order, color: "#3f9a3e".into(), color2: "#2e7430".into(), ..Default::default() }, Space::Content);
        }
        c.maps.put(Map { id: "field".into(), name: "Field".into(), width: 4, height: 4, legend: [("g".to_string(), "grass".to_string()), ("t".to_string(), "tall_grass".to_string())].into_iter().collect(), ..Default::default() }, Space::Content);
        c
    }

    /// A frame of the Ground tool's terrains, `events` in it.
    fn list(ctx: &egui::Context, ed: &mut MapEditor, events: Vec<egui::Event>) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 900.0));
        let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| ed.terrain_list(ui));
        out.textures_delta.clear();
    }

    /// A click on a marked widget of it, as a pointer makes it: over it and down, then up.
    fn click(ctx: &egui::Context, ed: &mut MapEditor, name: &str) {
        let at = crate::platform::hook(name).unwrap_or_else(|| panic!("{name} isn't drawn"));
        let (x, y) = at.split_once(',').unwrap();
        let pos = egui::pos2(x.parse().unwrap(), y.parse().unwrap());
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        list(ctx, ed, vec![egui::Event::PointerMoved(pos), button(true)]);
        list(ctx, ed, vec![button(false)]);
    }

    #[test]
    fn ids_and_names_come_free() {
        let taken = |s: &str| ["tall_grass", "tall_grass_2"].contains(&s);
        assert_eq!(free_id("Tall grass", &taken), "tall_grass_3");
        assert_eq!(free_id("Lava lake!", &taken), "lava_lake");
        assert_eq!(free_id("  ", &taken), "terrain");
        assert_eq!(free_name("Tall grass 2", &|n| n == "Tall grass 2"), "Tall grass 3");
    }

    #[test]
    fn a_terrain_duplicated_goes_in_its_group_the_list_folds_and_one_no_map_uses_can_go() {
        let mut ed = MapEditor::new(Arc::new(ground()), None, crate::platform::Files::new("."));
        let copy = ed.duplicate_terrain("tall_grass").unwrap();
        let t = ed.content.terrains.get(&copy).unwrap();
        assert_eq!((copy.as_str(), t.name.as_str(), t.pattern.as_str(), t.order), ("tall_grass_2", "Tall grass 2", "tall", 3));
        assert_eq!(t.category_name(), ed.content.terrains.get("tall_grass").unwrap().category_name(), "in its group");
        assert!(ed.terrains_dirty.contains(&copy) && ed.unsaved());
        // the list: Tall grass and Water in groups that fold, Grass alone
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        ed.terrain = copy.clone();
        list(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("terraingroups").as_deref(), Some("Tall grass:2;Water:2"));
        let shown = || crate::platform::hook("terrainsshown").unwrap_or_default();
        assert_eq!(shown(), "grass;tall_grass;tall_grass_2;water;sea");
        // folded, the section shows the one picked alone (under its group's name); open again,
        // its group folded, the one picked in it
        click(&ctx, &mut ed, "uiSection_Terrain");
        list(&ctx, &mut ed, vec![]);
        assert_eq!(shown(), "tall_grass_2");
        click(&ctx, &mut ed, "uiSection_Terrain");
        list(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiTerrainGroup_tall_grass");
        list(&ctx, &mut ed, vec![]);
        assert_eq!(shown(), "grass;tall_grass_2;water;sea");
        // a row's Edit opens the Terrain window on it; its Duplicate makes a variant, picked, in
        // its group; New kind is by the section's title
        click(&ctx, &mut ed, "uiTerrainEdit_water");
        assert_eq!((ed.terrain.as_str(), ed.terrain_window.id.as_deref()), ("water", Some("water")));
        click(&ctx, &mut ed, "uiTerrainDuplicate_grass");
        assert!(ed.content.terrains.contains("grass_2") && ed.terrain == "grass_2");
        list(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("terraingroups").as_deref(), Some("Grass:2;Tall grass:2;Water:2"));
        assert!(crate::platform::hook("uiTerrainNew").is_some());
        // the copy: no map uses it, it goes; the original: a map does, it stays
        assert_eq!(maps_using(&ed.content, "tall_grass"), ["field"]);
        ed.delete_terrain("tall_grass");
        ed.delete_terrain(&copy);
        assert!(ed.content.terrains.contains("tall_grass") && !ed.content.terrains.contains(&copy) && !ed.terrains_dirty.contains(&copy));
    }

    #[test]
    fn a_new_kind_comes_from_a_preset_its_own_group_and_every_preset_checks_out() {
        let mut ed = MapEditor::new(Arc::new(ground()), None, crate::platform::Files::new("."));
        let all = presets();
        assert!(all.len() >= 10 && all.iter().any(|p| p.look.glow > 0.0 || p.look.layers.iter().any(|y| y.glow > 0.0)));
        let id = ed.new_terrain(all[0].clone());
        let t = ed.content.terrains.get(&id).unwrap().clone();
        assert_eq!((t.pattern.as_str(), t.category_name()), ("custom", t.name.clone()), "a kind of its own: its own group");
        assert!(t.order > 8, "last in the list");
        // each preset a terrain the content check takes
        let mut c = ground();
        for p in presets() {
            let id = free_id(&p.name, &|_| false);
            c.terrains.put(Terrain { id, ..p }, Space::Content);
        }
        let said: Vec<String> = c.validate().into_iter().filter(|p| p.contains("terrains/")).collect();
        assert!(said.is_empty(), "{said:?}");
    }
}
