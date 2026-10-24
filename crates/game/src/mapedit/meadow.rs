//! A terrain's meadow (`content::Meadow`) in the Ground tool's "Grows here": its plants (blades
//! of grass, flowers, grass props' models of a blade or a few), each a card with how often it
//! grows and how it looks, added from presets (several grasses and flowers) or from the grass
//! props; and how the meadow grows (how many to a cell, its patches, its clumps), lit as the
//! ground or in the toon look. Its plants' looks show at once (the renderer has them as they
//! change: `stream::blade_kinds`); what grows where once its squares are built again. The asset
//! browser's "In meadows", for a grass prop: the meadows it grows in as one of their plants.

use content::{Meadow, Plant, Terrain};

use super::{card, fieldset, fieldset_with, form, hint, mark, sign_button, warn, MapEditor, Sign};

/// The plants to start from: grasses (a meadow's, a tall one, a short lawn, reeds, a dry one)
/// and flowers (daisies, poppies, bluebells, buttercups, lavender, clover); how much any of
/// them sways is the wind's and the terrain's rigidity's to say.
pub(super) fn presets() -> Vec<Plant> {
    let grass = |name: &str, height: [f64; 2], width: f64, lean: f64, bend: f64, color: &str, tip: &str| Plant {
        name: name.into(),
        height,
        width,
        lean,
        bend,
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
        petals,
        petal: petal.into(),
        heart: heart.into(),
        bloom,
        clusters: 0.5,
        ..Default::default()
    };
    vec![
        grass("Meadow grass", [0.3, 0.55], 0.035, 0.25, 0.4, "", ""),
        grass("Tall grass", [0.55, 0.95], 0.04, 0.3, 0.55, "", ""),
        grass("Short lawn", [0.06, 0.14], 0.03, 0.15, 0.2, "", ""),
        grass("Reeds", [0.8, 1.3], 0.03, 0.06, 0.1, "#4a6a2a", "#a8b060"),
        grass("Dry grass", [0.3, 0.6], 0.03, 0.35, 0.5, "#6a5a2a", "#d9c47a"),
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
fn colour(ui: &mut egui::Ui, hex: &mut String, fallback: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
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
fn range(ui: &mut egui::Ui, r: &mut [f64; 2], lo: f64, hi: f64, speed: f64, suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(egui::DragValue::new(&mut r[0]).range(lo..=hi).speed(speed).max_decimals(2)).changed();
        ui.label("to");
        changed |= ui.add(egui::DragValue::new(&mut r[1]).range(lo..=hi).speed(speed).max_decimals(2).suffix(suffix)).changed();
    });
    if r[0] > r[1] {
        r.swap(0, 1);
    }
    changed
}

/// A plant's rigidity row in its card: its terrain's (the default, `terrain`), or one of its
/// own, so a meadow has soft grass and stiff reeds, flowers that barely nod.
fn rigidity_row(ui: &mut egui::Ui, p: &mut content::Plant, k: usize, terrain: f64) {
    ui.label("Rigidity");
    ui.horizontal(|ui| {
        let inherited = ui.selectable_label(p.rigidity.is_none(), "the terrain's").on_hover_text(format!("As stiff as its terrain says ({terrain:.2}: the Terrain fieldset's Rigidity)"));
        mark(&format!("uiMeadowRigidityTerrain_{k}"), &inherited);
        if inherited.clicked() {
            p.rigidity = None;
        }
        let mut value = p.rigidity.unwrap_or(terrain);
        let r = ui.add(egui::Slider::new(&mut value, 0.0..=1.0).fixed_decimals(2)).on_hover_text("Its own stiffness: 0 bends from its foot, swaying with the wind and giving way to whoever walks through it as far as they push; more and it's still that far up from the ground, its tip giving that much less; 1 never moves");
        mark(&format!("uiMeadowRigidity_{k}"), &r);
        if r.changed() {
            p.rigidity = Some(value);
        }
    });
    ui.end_row();
}

impl MapEditor {
    /// The grass props, by name: what a meadow's model can be.
    fn grass_models(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.content.props.iter().filter(|p| p.category == "grass").map(|p| (p.id.clone(), p.name.clone())).collect();
        out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(&b.0)));
        out
    }

    /// The meadow of the terrain picked to paint, under "Grows here": its plants, each a card
    /// (its colour, name, how often it grows and taking it off in its header; under it its
    /// shape, its colours or its head, and where it grows, a box each), more from the presets;
    /// how the meadow grows, a box; changed on its record.
    pub(super) fn meadow_section(&mut self, ui: &mut egui::Ui, t: &Terrain) {
        let mut m = t.growth.meadow.clone();
        if m.is_empty() {
            hint(ui, "A meadow: blades of grass and flowers the engine grows itself, made on the GPU as they're drawn: every one up close, further off fewer and wider (as full to the eye), at the furthest shorter. Add a grass or a flower to start one.");
        }
        let (foot, tip) = (t.color.clone(), if t.color2.is_empty() { t.color.clone() } else { t.color2.clone() });
        let models = self.grass_models();
        let mut gone: Option<usize> = None;
        for k in 0..m.plants.len() {
            let name = called(&m.plants[k], &self.content);
            // (in the pull request's eyes: by its name, else its place)
            let key = if m.plants[k].name.is_empty() { k.to_string() } else { m.plants[k].name.clone() };
            let changed = self.review_changes("terrains", &t.id, &format!("growth.meadow.plants[{key}]"));
            card(
                ui,
                egui::Id::new(("meadow plant", &t.id, k)),
                &format!("uiMeadowPlant_{k}"),
                &name,
                &mut m.plants[k],
                |ui, p| {
                    super::swatch(ui, if p.shape == "flower" { &p.petal } else if p.tip.is_empty() { &tip } else { &p.tip });
                    if let Some((kind, about)) = &changed {
                        super::review::dot(ui, *kind, about);
                    }
                },
                |ui, p| {
                    let x = sign_button(ui, Sign::Cross, true).on_hover_text("Grows here no longer");
                    mark(&format!("uiMeadowRemove_{k}"), &x);
                    if x.clicked() {
                        gone = Some(k);
                    }
                    let w = ui.add(egui::DragValue::new(&mut p.weight).range(0.01..=20.0).speed(0.01).prefix("× ")).on_hover_text("How often it grows, against the meadow's others");
                    mark(&format!("uiMeadowWeight_{k}"), &w);
                },
                |ui, p| {
                    form(ui, ("meadow plant's name", k), |ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(f32::INFINITY).hint_text(name.as_str()));
                        ui.end_row();
                    });
                    match p.shape.as_str() {
                        "model" => {
                            fieldset(ui, "Model", |ui| {
                                form(ui, ("meadow plant's model", k), |ui| {
                                    ui.label("Prop");
                                    let shown = models.iter().find(|(id, _)| id == &p.prop).map(|(_, n)| n.clone()).unwrap_or_else(|| "(none)".into());
                                    egui::ComboBox::from_id_salt(("meadow model", &t.id, k)).selected_text(shown).show_ui(ui, |ui| {
                                        for (id, n) in &models {
                                            ui.selectable_value(&mut p.prop, id.clone(), n);
                                        }
                                    });
                                    ui.end_row();
                                    ui.label("Sizes");
                                    range(ui, &mut p.scale, 0.1, 4.0, 0.01, " ×");
                                    ui.end_row();
                                    rigidity_row(ui, p, k, t.rigidity);
                                });
                                hint(ui, "A grass prop's model (a blade or a few, a flower): placed as the meadow's own are, fewer further off and shorter at the furthest.");
                            });
                        }
                        shape => {
                            let flower = shape == "flower";
                            fieldset(ui, if flower { "Stem" } else { "Shape" }, |ui| {
                                form(ui, ("meadow plant's shape", k), |ui| {
                                    ui.label("Height");
                                    range(ui, &mut p.height, 0.02, 3.0, 0.005, " cells");
                                    ui.end_row();
                                    ui.label("Width");
                                    ui.add(egui::Slider::new(&mut p.width, 0.004..=0.2).logarithmic(true)).on_hover_text(if flower { "The stem's" } else { "At its foot" });
                                    ui.end_row();
                                    ui.label("Lean");
                                    ui.add(egui::Slider::new(&mut p.lean, 0.0..=1.0)).on_hover_text("How far over it leans (0: straight up, 1: lying down)");
                                    ui.end_row();
                                    ui.label("Curve");
                                    ui.add(egui::Slider::new(&mut p.bend, 0.0..=1.0)).on_hover_text("How much it curves over as it rises");
                                    ui.end_row();
                                    rigidity_row(ui, p, k, t.rigidity);
                                    if flower {
                                        ui.label("At its foot");
                                        colour(ui, &mut p.color, &foot);
                                        ui.end_row();
                                        ui.label("At its top");
                                        colour(ui, &mut p.tip, &tip);
                                        ui.end_row();
                                    }
                                });
                            });
                            fieldset(ui, if flower { "Head" } else { "Colour" }, |ui| {
                                form(ui, ("meadow plant's colours", k), |ui| {
                                    if flower {
                                        ui.label("Petals");
                                        ui.add(egui::Slider::new(&mut p.petals, 3..=Meadow::PETALS)).on_hover_text("How many");
                                        ui.end_row();
                                        ui.label("Petals' colour");
                                        colour(ui, &mut p.petal, "#f4f1e8");
                                        ui.end_row();
                                        ui.label("Heart");
                                        colour(ui, &mut p.heart, "#e8b923");
                                        ui.end_row();
                                        ui.label("Width");
                                        ui.add(egui::Slider::new(&mut p.bloom, 0.02..=0.4)).on_hover_text("The head's, across its petals");
                                        ui.end_row();
                                    } else {
                                        ui.label("At its foot");
                                        colour(ui, &mut p.color, &foot);
                                        ui.end_row();
                                        ui.label("At its tip");
                                        colour(ui, &mut p.tip, &tip);
                                        ui.end_row();
                                    }
                                    ui.label("Glow");
                                    let g = ui.add(egui::Slider::new(&mut p.glow, 0.0..=4.0)).on_hover_text(if flower { "How much its petals and heart glow of themselves, lit by nothing: at night as by day" } else { "How much it glows of itself towards its tip, lit by nothing" });
                                    mark(&format!("uiMeadowGlow_{k}"), &g);
                                    ui.end_row();
                                });
                            });
                        }
                    }
                    fieldset(ui, "Where", |ui| {
                        form(ui, ("meadow plant's place", k), |ui| {
                            ui.label("Clusters");
                            ui.add(egui::Slider::new(&mut p.clusters, 0.0..=1.0)).on_hover_text("How much it keeps to clusters of its own a few cells across (0: anywhere, 1: only in them)");
                            ui.end_row();
                        });
                    });
                },
            );
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
            fieldset_with(ui, "Meadow", self.review_changes_in("terrains", &t.id, &["growth.meadow.density", "growth.meadow.patchy", "growth.meadow.clump", "growth.meadow.clumping", "growth.meadow.toon"]), |ui| {
                form(ui, "meadow", |ui| {
                    ui.label("To a cell");
                    let d = ui.add(egui::Slider::new(&mut m.density, 0.0..=Meadow::MOST).step_by(1.0)).on_hover_text("How many grow on each cell, on average (up close: further off fewer are drawn)");
                    mark("uiMeadowDensity", &d);
                    ui.end_row();
                    ui.label("Patches");
                    ui.add(egui::Slider::new(&mut m.patchy, 0.0..=1.0)).on_hover_text("How much it thins out in patches a few cells across (0: as thick everywhere, 1: bare in places)");
                    ui.end_row();
                    ui.label("Clumps");
                    ui.add(egui::DragValue::new(&mut m.clump).range(0.0..=2.0).speed(0.01).suffix(" cells")).on_hover_text("How wide its clumps are (0: none)");
                    ui.end_row();
                    ui.label("Alike");
                    ui.add(egui::Slider::new(&mut m.clumping, 0.0..=1.0)).on_hover_text("How alike a clump's blades are: as tall, leaning out from its middle");
                    ui.end_row();
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
            });
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
