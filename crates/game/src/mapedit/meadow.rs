//! A terrain's meadow (`content::Meadow`) in the Ground tool's "Grows here": its plants (blades
//! of grass, flowers, grass props' models of a blade or a few), each a card with how often it
//! grows and how it looks, added from presets (several grasses and flowers) or from the grass
//! props; and how the meadow grows (how many to a cell, its patches, its clumps), lit as the
//! ground or in the toon look. Its plants' looks show at once (the renderer has them as they
//! change: `stream::blade_kinds`); what grows where once its squares are built again. The asset
//! browser's "In meadows", for a grass prop: the meadows it grows in as one of their plants.

use content::{Meadow, Plant, Terrain};
use egui::RichText;

use super::{hint, mark, warn, MapEditor};

/// The plants to start from: grasses (a meadow's, a tall one, a short lawn, reeds, a dry one)
/// and flowers (daisies, poppies, bluebells, buttercups, lavender, clover).
pub(super) fn presets() -> Vec<Plant> {
    let grass = |name: &str, height: [f64; 2], width: f64, lean: f64, bend: f64, sway: f64, color: &str, tip: &str| Plant {
        name: name.into(),
        height,
        width,
        lean,
        bend,
        sway,
        color: color.into(),
        tip: tip.into(),
        ..Default::default()
    };
    let flower = |name: &str, height: [f64; 2], petals: u8, petal: &str, heart: &str, bloom: f64| Plant {
        name: name.into(),
        shape: "flower".into(),
        weight: 0.12,
        height,
        width: 0.012,
        lean: 0.1,
        bend: 0.15,
        sway: 0.8,
        petals,
        petal: petal.into(),
        heart: heart.into(),
        bloom,
        clusters: 0.5,
        ..Default::default()
    };
    vec![
        grass("Meadow grass", [0.3, 0.55], 0.035, 0.25, 0.4, 1.0, "", ""),
        grass("Tall grass", [0.55, 0.95], 0.04, 0.3, 0.55, 1.0, "", ""),
        grass("Short lawn", [0.06, 0.14], 0.03, 0.15, 0.2, 0.5, "", ""),
        grass("Reeds", [0.8, 1.3], 0.03, 0.06, 0.1, 0.6, "#4a6a2a", "#a8b060"),
        grass("Dry grass", [0.3, 0.6], 0.03, 0.35, 0.5, 1.0, "#6a5a2a", "#d9c47a"),
        flower("Daisy", [0.18, 0.32], 8, "#f6f3ea", "#eab92a", 0.09),
        flower("Poppy", [0.3, 0.5], 4, "#d0301e", "#2a1a12", 0.12),
        flower("Bluebell", [0.2, 0.35], 6, "#5b6fd8", "#e8e2a8", 0.06),
        flower("Buttercup", [0.14, 0.28], 5, "#f4cc1c", "#e8a010", 0.055),
        flower("Lavender", [0.3, 0.5], 6, "#9a78d2", "#7a5ab0", 0.05),
        flower("Clover", [0.06, 0.12], 3, "#f2ecf0", "#e0c8d8", 0.05),
    ]
}

/// A plant's name in the editor: its own, else its model's prop's, else its shape's.
fn called(p: &Plant, content: &content::Content) -> String {
    if !p.name.is_empty() {
        return p.name.clone();
    }
    match p.shape.as_str() {
        "model" => content.props.get(&p.prop).map(|q| q.name.clone()).unwrap_or_else(|| "A model".into()),
        "flower" => "A flower".into(),
        _ => "A blade".into(),
    }
}

/// "#rrggbb" edited with a colour picker; `own` unticked: none of its own (its terrain's,
/// `fallback`, shown).
fn colour(ui: &mut egui::Ui, hex: &mut String, fallback: &str, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut own = !hex.is_empty();
        if ui.checkbox(&mut own, "").on_hover_text("Its own colour (else its terrain's)").changed() {
            *hex = if own { fallback.to_string() } else { String::new() };
            changed = true;
        }
        let shown = if hex.is_empty() { fallback } else { hex.as_str() };
        let v = u32::from_str_radix(shown.trim_start_matches('#'), 16).unwrap_or(0x808080);
        let mut rgb = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
        let r = ui.add_enabled_ui(own, |ui| ui.color_edit_button_srgb(&mut rgb)).inner;
        if r.changed() {
            *hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            changed = true;
        }
    });
    changed
}

/// A range of two numbers edited side by side, the least first.
fn range(ui: &mut egui::Ui, label: &str, r: &mut [f64; 2], lo: f64, hi: f64, speed: f64, suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed |= ui.add(egui::DragValue::new(&mut r[0]).range(lo..=hi).speed(speed).suffix(suffix)).changed();
        ui.label("to");
        changed |= ui.add(egui::DragValue::new(&mut r[1]).range(lo..=hi).speed(speed).suffix(suffix)).changed();
    });
    if r[0] > r[1] {
        r.swap(0, 1);
    }
    changed
}

impl MapEditor {
    /// The grass props, by name: what a meadow's model can be.
    fn grass_models(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.content.props.iter().filter(|p| p.category == "grass").map(|p| (p.id.clone(), p.name.clone())).collect();
        out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(&b.0)));
        out
    }

    /// The meadow of the terrain picked to paint, under "Grows here": its plants and how it
    /// grows, changed on its record.
    pub(super) fn meadow_section(&mut self, ui: &mut egui::Ui, t: &Terrain) {
        ui.label(RichText::new("Meadow").strong());
        let mut m = t.growth.meadow.clone();
        if m.is_empty() {
            hint(ui, "Blades of grass and flowers the engine grows itself, made on the GPU as they're drawn: every one up close, further off fewer and wider (as full to the eye), at the furthest shorter. Add a grass or a flower to start one.");
        }
        let (foot, tip) = (t.color.clone(), if t.color2.is_empty() { t.color.clone() } else { t.color2.clone() });
        let models = self.grass_models();
        let mut gone: Option<usize> = None;
        for k in 0..m.plants.len() {
            let id = egui::Id::new(("meadow plant", &t.id, k));
            let name = called(&m.plants[k], &self.content);
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            let p = &mut m.plants[k];
            state
                .show_header(ui, |ui| {
                    let swatch = if p.shape == "flower" { p.petal.clone() } else if p.tip.is_empty() { tip.clone() } else { p.tip.clone() };
                    super::swatch(ui, &swatch);
                    ui.label(&name);
                    let w = ui.add(egui::DragValue::new(&mut p.weight).range(0.01..=20.0).speed(0.01).prefix("× ")).on_hover_text("How often it grows, against the meadow's others");
                    mark(&format!("uiMeadowWeight_{k}"), &w);
                    let x = ui.small_button("✕").on_hover_text("Grows here no longer");
                    mark(&format!("uiMeadowRemove_{k}"), &x);
                    if x.clicked() {
                        gone = Some(k);
                    }
                })
                .body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(140.0).hint_text(name.as_str()));
                    });
                    match p.shape.as_str() {
                        "model" => {
                            let shown = models.iter().find(|(id, _)| id == &p.prop).map(|(_, n)| n.clone()).unwrap_or_else(|| "(none)".into());
                            egui::ComboBox::from_id_salt(("meadow model", &t.id, k)).selected_text(shown).show_ui(ui, |ui| {
                                for (id, n) in &models {
                                    ui.selectable_value(&mut p.prop, id.clone(), n);
                                }
                            });
                            range(ui, "Sizes", &mut p.scale, 0.1, 4.0, 0.01, " ×");
                            hint(ui, "A grass prop's model (a blade or a few, a flower): placed as the meadow's own are, fewer further off and shorter at the furthest.");
                        }
                        shape => {
                            let flower = shape == "flower";
                            range(ui, "Height", &mut p.height, 0.02, 3.0, 0.005, " cells");
                            ui.add(egui::Slider::new(&mut p.width, 0.004..=0.2).logarithmic(true).text(if flower { "stem's width" } else { "width at its foot" }));
                            ui.add(egui::Slider::new(&mut p.lean, 0.0..=1.0).text("lean")).on_hover_text("How far over it leans (0: straight up, 1: lying down)");
                            ui.add(egui::Slider::new(&mut p.bend, 0.0..=1.0).text("curve")).on_hover_text("How much it curves over as it rises");
                            ui.add(egui::Slider::new(&mut p.sway, 0.0..=2.0).text("sway")).on_hover_text("How much the wind moves it");
                            colour(ui, &mut p.color, &foot, if flower { "Stem at its foot" } else { "At its foot" });
                            colour(ui, &mut p.tip, &tip, if flower { "Stem at its top" } else { "At its tip" });
                            if flower {
                                ui.add(egui::Slider::new(&mut p.petals, 3..=Meadow::PETALS).text("petals"));
                                colour(ui, &mut p.petal, "#f4f1e8", "Petals");
                                colour(ui, &mut p.heart, "#e8b923", "Heart");
                                ui.add(egui::Slider::new(&mut p.bloom, 0.02..=0.4).text("head's width"));
                            }
                        }
                    }
                    ui.add(egui::Slider::new(&mut p.clusters, 0.0..=1.0).text("in clusters")).on_hover_text("How much it keeps to clusters of its own a few cells across (0: anywhere, 1: only in them)");
                });
        }
        if let Some(k) = gone {
            m.plants.remove(k);
        }
        // more: a grass or a flower from the presets, or a grass prop's model
        ui.horizontal(|ui| {
            let presets = presets();
            for (label, flowers, name) in [("+ Grass", false, "uiMeadowAddGrass"), ("+ Flower", true, "uiMeadowAddFlower")] {
                let r = ui.menu_button(label, |ui| {
                    for p in presets.iter().filter(|p| (p.shape == "flower") == flowers) {
                        let b = ui.button(&p.name);
                        mark(&format!("uiMeadowPreset_{}", p.name.to_lowercase().replace(' ', "_")), &b);
                        if b.clicked() {
                            m.plants.push(p.clone());
                            ui.close();
                        }
                    }
                });
                mark(name, &r.response);
            }
            let r = ui.add_enabled_ui(!models.is_empty(), |ui| {
                ui.menu_button("+ Model", |ui| {
                    for (id, n) in &models {
                        if ui.button(n).clicked() {
                            m.plants.push(Plant { shape: "model".into(), prop: id.clone(), weight: 0.3, ..Default::default() });
                            ui.close();
                        }
                    }
                })
            });
            r.response.on_disabled_hover_text("No grass props yet (the asset browser's grass category)");
        });
        if !m.is_empty() {
            let d = ui.add(egui::Slider::new(&mut m.density, 0.0..=Meadow::MOST).step_by(1.0).text("to a cell")).on_hover_text("How many grow on each cell, on average (up close: further off fewer are drawn)");
            mark("uiMeadowDensity", &d);
            ui.add(egui::Slider::new(&mut m.patchy, 0.0..=1.0).text("patches")).on_hover_text("How much it thins out in patches a few cells across (0: as thick everywhere, 1: bare in places)");
            ui.horizontal(|ui| {
                ui.label("Clumps");
                ui.add(egui::DragValue::new(&mut m.clump).range(0.0..=2.0).speed(0.01).suffix(" cells")).on_hover_text("How wide its clumps are (0: none)");
                ui.add(egui::Slider::new(&mut m.clumping, 0.0..=1.0).text("alike")).on_hover_text("How alike a clump's blades are: as tall, leaning out from its middle");
            });
            let toon = ui.checkbox(&mut m.toon, "Toon light").on_hover_text("Lit in the toon look's steps, as toon props are (else as the ground under it)");
            mark("uiMeadowToon", &toon);
            let kinds = world::meadow::Kinds::of(&self.content).list.len() - t.growth.meadow.plants.len() + m.plants.len();
            if kinds > world::meadow::MOST_KINDS {
                warn(ui, &format!("{kinds} plants in all the meadows: the first {} grow, the rest don't.", world::meadow::MOST_KINDS));
            }
            if t.pattern == "tall" {
                hint(ui, "It grows in place of the engine's own tufts, with the grass props below or alone.");
            }
        }
        crate::platform::set_hook("meadowplants", &m.plants.len().to_string());
        if m != t.growth.meadow {
            self.change_terrain(&t.id, move |t| t.growth.meadow = m);
        }
    }

    /// The asset browser's "In meadows", for a grass prop: each terrain with a meadow (and each
    /// tall one), ticked when the prop is one of its plants.
    pub(super) fn in_meadows(&mut self, ui: &mut egui::Ui, prop: &str) {
        let terrains: Vec<Terrain> = self.content.terrains.iter().filter(|t| t.walk != "water" && (!t.growth.meadow.is_empty() || t.pattern == "tall")).cloned().collect();
        if terrains.is_empty() {
            return;
        }
        super::section(ui, "In meadows");
        for t in terrains {
            let mut m = t.growth.meadow.clone();
            let mut on = m.plants.iter().any(|p| p.shape == "model" && p.prop == prop);
            let r = ui.checkbox(&mut on, t.name.as_str()).on_hover_text(format!("One of {}'s meadow's plants (the Ground tool's Grows here says how)", t.name));
            mark(&format!("uiInMeadow_{}", t.id), &r);
            if r.changed() {
                if on {
                    m.plants.push(Plant { shape: "model".into(), prop: prop.to_string(), weight: 0.3, ..Default::default() });
                } else {
                    m.plants.retain(|p| !(p.shape == "model" && p.prop == prop));
                }
                self.change_terrain(&t.id, move |t| t.growth.meadow = m);
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use content::{Content, Prop, Space};

    use super::*;

    fn lawn() -> Content {
        let mut c = Content::default();
        c.terrains.put(Terrain { id: "lawn".into(), name: "Lawn".into(), pattern: "grass".into(), walk: "walk".into(), color: "#4a8a3a".into(), color2: "#2e6a28".into(), ..Default::default() }, Space::Content);
        c.terrains.put(Terrain { id: "pond".into(), name: "Pond".into(), pattern: "water".into(), walk: "water".into(), ..Default::default() }, Space::Content);
        c.props.put(Prop { id: "tuft".into(), name: "Tuft".into(), category: "grass".into(), footprint: [1, 1], height: 0.4, ..Default::default() }, Space::Content);
        c
    }

    /// A frame of the Ground tool's "Grows here" for the terrain picked, `events` in it.
    fn grows_here(ctx: &egui::Context, ed: &mut MapEditor, events: Vec<egui::Event>) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 1200.0));
        let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| ed.growth_section(ui));
        out.textures_delta.clear();
    }

    fn click(ctx: &egui::Context, ed: &mut MapEditor, name: &str) {
        let at = crate::platform::hook(name).unwrap_or_else(|| panic!("{name} isn't drawn"));
        let (x, y) = at.split_once(',').unwrap();
        let pos = egui::pos2(x.parse().unwrap(), y.parse().unwrap());
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        grows_here(ctx, ed, vec![egui::Event::PointerMoved(pos), button(true)]);
        grows_here(ctx, ed, vec![button(false)]);
    }

    #[test]
    fn the_presets_are_grasses_and_flowers_each_its_own_and_all_well_made() {
        let all = presets();
        let flowers: Vec<&Plant> = all.iter().filter(|p| p.shape == "flower").collect();
        assert!(flowers.len() >= 5 && all.len() - flowers.len() >= 4);
        let mut names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "each its own name");
        // each checks out on a meadow
        let mut c = lawn();
        let mut t = c.terrains.get("lawn").unwrap().clone();
        t.growth.meadow = Meadow { plants: all, ..Default::default() };
        c.terrains.put(t, Space::Content);
        let said: Vec<String> = c.validate().into_iter().filter(|p| p.contains("meadow")).collect();
        assert!(said.is_empty(), "{said:?}");
    }

    /// A meadow started from the presets on short grass, a grass and a flower added, the first
    /// taken off; the terrain changed and due to be saved; nothing grows on water.
    #[test]
    fn a_meadow_is_made_from_the_presets_in_the_ground_tool() {
        let mut ed = MapEditor::new(Arc::new(lawn()), None, crate::platform::Files::new("."));
        ed.terrain = "lawn".into();
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        grows_here(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("meadowplants").as_deref(), Some("0"));
        // (a menu laid out a frame before it's where it shows)
        click(&ctx, &mut ed, "uiMeadowAddGrass");
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiMeadowPreset_meadow_grass");
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiMeadowAddFlower");
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiMeadowPreset_poppy");
        let meadow = |ed: &MapEditor| ed.content.terrains.get("lawn").unwrap().growth.meadow.clone();
        assert_eq!(meadow(&ed).plants.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Meadow grass", "Poppy"]);
        assert!(ed.terrains_dirty.contains("lawn") && ed.unsaved());
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiMeadowRemove_0");
        assert_eq!(meadow(&ed).plants.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Poppy"]);
        // water: nothing grows there, nothing's offered
        ed.terrain = "pond".into();
        crate::platform::set_hook("meadowplants", "none");
        grows_here(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("meadowplants").as_deref(), Some("none"));
    }

    /// A grass prop ticked in a meadow from the asset browser: one of its plants (a model);
    /// unticked, not.
    #[test]
    fn a_grass_prop_grows_in_a_meadow_ticked_in_the_asset_browser() {
        let mut c = lawn();
        let mut t = c.terrains.get("lawn").unwrap().clone();
        t.growth.meadow = Meadow { plants: vec![Plant::default()], ..Default::default() };
        c.terrains.put(t, Space::Content);
        let mut ed = MapEditor::new(Arc::new(c), None, crate::platform::Files::new("."));
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let frame = |ed: &mut MapEditor, events: Vec<egui::Event>| {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 900.0));
            let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| ed.in_meadows(ui, "tuft"));
            out.textures_delta.clear();
        };
        frame(&mut ed, vec![]);
        let at = crate::platform::hook("uiInMeadow_lawn").unwrap();
        let (x, y) = at.split_once(',').unwrap();
        let pos = egui::pos2(x.parse().unwrap(), y.parse().unwrap());
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(&mut ed, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(&mut ed, vec![button(false)]);
        let plants = ed.content.terrains.get("lawn").unwrap().growth.meadow.plants.clone();
        assert!(plants.len() == 2 && plants[1].shape == "model" && plants[1].prop == "tuft", "{plants:?}");
    }
}
