//! Grass props grown through tall ground (`content::Growth`): the Ground tool's "Grows here",
//! for the tall terrain picked to paint (the grass props ticked, each with how often it's
//! picked, and how they grow: tiled, each whole over its cells, or scattered, so many to a
//! cell, their sizes, patches; turned their own ways), and the asset browser's "Grows in" for
//! a grass prop (the tall terrains it grows through). Both change the terrain's record, saved
//! with the rest (Ctrl+S, or by itself); the map's squares show it as it changes (`stream`).
//! The renderer draws a grass prop's copies thinner the further off they are (its model in
//! levels, `world::grass`): one too heavy to grow even at its thinnest
//! (`world::mesh::GROWN_MOST` vertices a cell), or as made (`GROWN_HEAVIEST`), is said so, and
//! the engine's blades grow in its place. Grass props are still placed one by one as any prop.

use std::sync::Arc;

use content::{Growth, Grower, Terrain};
use world::mesh::{grown_cost, GROWN_HEAVIEST, GROWN_MOST};

use super::stream::{grass_version, grass_weight};

use super::{hint, mark, section, warn, MapEditor};

/// The props that can grow through tall ground: the grass ones, by name.
fn grass_props(content: &content::Content) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = content.props.iter().filter(|p| p.category == "grass").map(|p| (p.id.clone(), p.name.clone())).collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(&b.0)));
    out
}

/// A grower ticked or not in a list, its weight kept while it's ticked.
fn tick(list: &mut Vec<Grower>, prop: &str, on: bool) {
    match (list.iter().position(|g| g.prop == prop), on) {
        (None, true) => list.push(Grower { prop: prop.to_string(), weight: 1.0 }),
        (Some(k), false) => {
            list.remove(k);
        }
        _ => {}
    }
}

impl MapEditor {
    /// A terrain changed where it is (its space), to save later.
    pub(super) fn change_terrain(&mut self, id: &str, change: impl FnOnce(&mut Terrain)) {
        let Some(e) = self.content.terrains.entry(id) else { return };
        let (mut t, space) = (e.value.clone(), e.space);
        change(&mut t);
        if self.content.terrains.get(id) == Some(&t) {
            return;
        }
        let mut c = (*self.content).clone();
        c.terrains.put(t, space);
        self.content = Arc::new(c);
        self.terrains_dirty.insert(id.to_string());
    }

    /// What a grass prop costs each cell it grows through as `g` grows it, in vertices, at its
    /// thinnest (what says whether it grows: more than anything when it's too heavy as made);
    /// its real model's once loaded, else its stand-in's.
    fn grown_cost(&self, g: &Growth, id: &str) -> usize {
        self.content.props.get(id).map(|p| grown_cost(g, p, grass_weight(&self.grass_levels, p, &self.models, &self.model_versions))).unwrap_or(0)
    }

    /// The same as made: up close.
    fn near_cost(&self, g: &Growth, id: &str) -> usize {
        self.content
            .props
            .get(id)
            .map(|p| {
                let levels = self.grass_levels.borrow_mut().levels(p, &self.models, grass_version(p, &self.model_versions));
                grown_cost(g, p, levels.vertices.first().copied().unwrap_or(0))
            })
            .unwrap_or(0)
    }

    /// Whether a prop's model fills its footprint's cells, as grass made to tile them is (a
    /// grass request's): within a tenth of them each way.
    fn fills_its_cells(&self, id: &str) -> bool {
        match (self.models.get(id), self.content.props.get(id)) {
            (Some(m), Some(p)) => m.max.x - m.min.x >= 0.9 * p.footprint[0].max(1) as f32 && m.max.z - m.min.z >= 0.9 * p.footprint[1].max(1) as f32,
            _ => false,
        }
    }

    /// The Ground tool's "Grows here", under the terrains, when the one picked to paint is tall
    /// ground: the grass props grown through it in place of the engine's own blades, and how.
    pub(super) fn growth_section(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.content.terrains.get(&self.terrain).filter(|t| t.pattern == "tall").cloned() else { return };
        section(ui, "Grows here");
        let props = grass_props(&self.content);
        if props.is_empty() {
            hint(ui, &format!("{} grows the engine's own blades. A prop in the grass category (the asset browser) can grow through it instead.", t.name));
            return;
        }
        let mut g = t.growth.clone();
        let was_empty = g.is_empty();
        for (id, name) in &props {
            ui.horizontal(|ui| {
                let mut on = g.props.iter().any(|p| &p.prop == id);
                let r = ui.checkbox(&mut on, name.as_str()).on_hover_text("Grows through it (any ticked: the engine's own blades no longer)");
                mark(&format!("uiGrows_{id}"), &r);
                if r.changed() {
                    tick(&mut g.props, id, on);
                }
                if let Some(p) = g.props.iter_mut().find(|p| &p.prop == id) {
                    let w = ui.add(egui::DragValue::new(&mut p.weight).range(0.05..=20.0).speed(0.05).prefix("× ")).on_hover_text("How often it's picked, against the others ticked");
                    mark(&format!("uiGrowsWeight_{id}"), &w);
                }
            });
        }
        // the first ticked made to fill its cells (a grass request's): tiled, as it's made to be
        if was_empty && !g.is_empty() {
            g.tiled = self.fills_its_cells(&g.props[0].prop);
        }
        if g.is_empty() {
            hint(ui, "None ticked: the engine's own blades.");
        } else {
            ui.horizontal(|ui| {
                let tiled = ui.radio_value(&mut g.tiled, true, "Tiled").on_hover_text("Each whole over its cells, lined up with them, so copies side by side join: grass made to fill its cells (a grass request's)");
                mark("uiGrowthTiled", &tiled);
                let scattered = ui.radio_value(&mut g.tiled, false, "Scattered").on_hover_text("So many to a cell, anywhere on it, each sized its own way: small tufts");
                mark("uiGrowthScattered", &scattered);
            });
            if !g.tiled {
                let d = ui.add(egui::Slider::new(&mut g.density, 0.0..=Growth::MOST).step_by(0.5).text("to a cell")).on_hover_text("How many grow on each cell, on average");
                mark("uiGrowthDensity", &d);
                ui.horizontal(|ui| {
                    ui.label("Sizes");
                    let lo = ui.add(egui::DragValue::new(&mut g.scale[0]).range(0.1..=4.0).speed(0.01).prefix("× ")).on_hover_text("The smallest, against its prop as made");
                    mark("uiGrowthSmallest", &lo);
                    ui.label("to");
                    let hi = ui.add(egui::DragValue::new(&mut g.scale[1]).range(0.1..=4.0).speed(0.01).prefix("× ")).on_hover_text("The biggest");
                    mark("uiGrowthBiggest", &hi);
                    if g.scale[0] > g.scale[1] {
                        g.scale.swap(0, 1);
                    }
                });
                let patchy = ui.add(egui::Slider::new(&mut g.patchy, 0.0..=1.0).text("patches")).on_hover_text("How much it thins out in patches a few cells across (0: as thick everywhere, 1: bare in places)");
                mark("uiGrowthPatchy", &patchy);
            }
            let turn = ui.checkbox(&mut g.turn, "Each turned its own way").on_hover_text(if g.tiled { "By quarter turns (half turns when it's longer than wide), so it still lies over its cells" } else { "Any way round" });
            mark("uiGrowthTurn", &turn);
            // what it costs, the vertices a cell: up close (as made) and far off (at its
            // thinnest), the props light enough to grow, as picked
            let costs: Vec<(&Grower, usize)> = g.props.iter().map(|p| (p, self.grown_cost(&g, &p.prop))).collect();
            let light: Vec<&(&Grower, usize)> = costs.iter().filter(|(_, c)| *c <= GROWN_MOST).collect();
            let total: f64 = light.iter().map(|(p, _)| p.weight.max(0.0)).sum();
            let share = |p: &Grower| p.weight.max(0.0) / total.max(1e-9);
            let far: f64 = light.iter().map(|(p, c)| share(p) * *c as f64).sum();
            let near: f64 = light.iter().map(|(p, _)| share(p) * self.near_cost(&g, &p.prop) as f64).sum();
            if !light.is_empty() {
                hint(ui, &format!("About {} vertices a cell up close, {} far off (drawn thinner the further off)", grouped(near.round() as usize), grouped(far.round() as usize)));
            }
            // too heavy to grow: said, with what they weigh (the blades grow in their place)
            let heavy: Vec<String> = costs
                .iter()
                .filter(|(_, c)| *c > GROWN_MOST)
                .map(|(p, c)| {
                    let name = self.content.props.get(&p.prop).map(|q| q.name.as_str()).unwrap_or(&p.prop);
                    if *c == usize::MAX { format!("{name} (more than {} a cell as made)", grouped(GROWN_HEAVIEST)) } else { format!("{name} ({} a cell at its thinnest)", grouped(*c)) }
                })
                .collect();
            if !heavy.is_empty() {
                let instead = if light.is_empty() { "the engine's blades grow instead" } else { "the others grow without them" };
                warn(ui, &format!(
                    "Too heavy to grow: {}; {instead}. Grass is drawn thinner further off by leaving out whole pieces (a blade, a leaf): at its thinnest at most {} vertices a cell, at most {} as made. Make it again in pieces (Model Requests).",
                    heavy.join(", "),
                    grouped(GROWN_MOST),
                    grouped(GROWN_HEAVIEST)
                ));
            }
            crate::platform::set_hook("growthheavy", &heavy.len().to_string());
        }
        if g != t.growth {
            self.change_terrain(&t.id, move |t| t.growth = g);
        }
    }

    /// The asset browser's "Grows in", for a grass prop: the tall terrains it grows through,
    /// each ticked (with how often it's picked there).
    pub(super) fn grows_in(&mut self, ui: &mut egui::Ui, prop: &str) {
        let talls: Vec<Terrain> = self.content.terrains.iter().filter(|t| t.pattern == "tall").cloned().collect();
        section(ui, "Grows in");
        if talls.is_empty() {
            hint(ui, "No tall ground to grow through (a terrain whose pattern is tall).");
            return;
        }
        let mut changes = vec![];
        for t in &talls {
            let mut g = t.growth.clone();
            let was_empty = g.is_empty();
            ui.horizontal(|ui| {
                let mut on = g.props.iter().any(|p| p.prop == prop);
                let r = ui.checkbox(&mut on, t.name.as_str()).on_hover_text(format!("Grows through {}'s cells (the Ground tool's Grows here says how)", t.name));
                mark(&format!("uiGrowsIn_{}", t.id), &r);
                if r.changed() {
                    tick(&mut g.props, prop, on);
                }
                if let Some(p) = g.props.iter_mut().find(|p| p.prop == prop) {
                    ui.add(egui::DragValue::new(&mut p.weight).range(0.05..=20.0).speed(0.05).prefix("× ")).on_hover_text("How often it's picked there, against the others growing there");
                }
            });
            // the first there, made to fill its cells: tiled
            if was_empty && !g.is_empty() {
                g.tiled = self.fills_its_cells(prop);
            }
            let cost = self.grown_cost(&g, prop);
            if g.props.iter().any(|p| p.prop == prop) && cost > GROWN_MOST {
                let weight = if cost == usize::MAX { format!("more than {} vertices a cell as made", grouped(GROWN_HEAVIEST)) } else { format!("{} vertices a cell at its thinnest, at most {}", grouped(cost), grouped(GROWN_MOST)) };
                warn(ui, &format!("Too heavy to grow through {}: {weight}", t.name));
            }
            if g != t.growth {
                changes.push((t.id.clone(), g));
            }
        }
        for (id, g) in changes {
            self.change_terrain(&id, move |t| t.growth = g);
        }
    }

    /// Writes the terrains changed here: into their space on the desktop, kept in the browser
    /// (or downloaded).
    pub(super) fn save_terrains(&mut self) {
        let ids: Vec<String> = self.terrains_dirty.iter().cloned().collect();
        let (mut done, mut errors) = (0, vec![]);
        for id in &ids {
            let Some(e) = self.content.terrains.entry(id) else { continue };
            #[cfg(not(target_arch = "wasm32"))]
            let saved = {
                let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
                content::disk::save(&root, e.space, &e.value).map(|_| ())
            };
            #[cfg(target_arch = "wasm32")]
            let saved = super::saving::browser_save_record(e.space, &e.value, &format!("{id}.json")).map(|_| ());
            match saved {
                Ok(()) => done += 1,
                Err(err) => errors.push(err),
            }
        }
        if errors.is_empty() {
            self.terrains_dirty.clear();
        }
        let plural = if done == 1 { "" } else { "s" };
        self.status = match (done, errors.is_empty()) {
            (_, false) => format!("Terrains not saved: {}", errors.join("; ")),
            (n, true) if super::saving::keeps() => format!("Saved {n} terrain{plural} in this browser"),
            (n, true) if cfg!(target_arch = "wasm32") => format!("Downloaded {n} terrain{plural}: put it in content/data/terrains/"),
            (n, true) => format!("Saved {n} terrain{plural}"),
        };
    }
}

/// A count with its thousands apart: 43,620.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use content::{Content, Grower, Prop, Space, Terrain};

    use super::*;

    fn with_grass() -> Content {
        let mut c = Content::default();
        c.terrains.put(Terrain { id: "grass".into(), name: "Grass".into(), pattern: "grass".into(), walk: "walk".into(), ..Default::default() }, Space::Content);
        c.terrains.put(Terrain { id: "tall_grass".into(), name: "Tall grass".into(), pattern: "tall".into(), walk: "walk".into(), ..Default::default() }, Space::Content);
        for (id, name) in [("tuft", "Tuft"), ("clover", "Clover")] {
            c.props.put(Prop { id: id.into(), name: name.into(), category: "grass".into(), footprint: [1, 1], height: 0.4, ..Default::default() }, Space::Content);
        }
        c.props.put(Prop { id: "bush".into(), name: "Bush".into(), category: "flora".into(), footprint: [1, 1], height: 0.8, ..Default::default() }, Space::Content);
        c
    }

    #[test]
    fn only_grass_props_grow_and_ticking_one_keeps_its_weight() {
        assert_eq!(grass_props(&with_grass()), [("clover".to_string(), "Clover".to_string()), ("tuft".to_string(), "Tuft".to_string())], "by name, not the bush");
        let mut list = vec![];
        tick(&mut list, "tuft", true);
        tick(&mut list, "tuft", true);
        assert_eq!(list, [Grower { prop: "tuft".into(), weight: 1.0 }], "once");
        list[0].weight = 3.0;
        tick(&mut list, "clover", true);
        tick(&mut list, "clover", false);
        assert_eq!(list, [Grower { prop: "tuft".into(), weight: 3.0 }]);
    }

    /// A frame of the Ground tool's "Grows here" for the terrain picked, `events` in it.
    fn grows_here(ctx: &egui::Context, ed: &mut MapEditor, events: Vec<egui::Event>) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 900.0));
        let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| ed.growth_section(ui));
        out.textures_delta.clear();
    }

    /// A click on a marked widget of it, as a pointer makes it: over it and down, then up.
    fn click(ctx: &egui::Context, ed: &mut MapEditor, name: &str) {
        let at = crate::platform::hook(name).unwrap_or_else(|| panic!("{name} isn't drawn"));
        let (x, y) = at.split_once(',').unwrap();
        let pos = egui::pos2(x.parse().unwrap(), y.parse().unwrap());
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        grows_here(ctx, ed, vec![egui::Event::PointerMoved(pos), button(true)]);
        grows_here(ctx, ed, vec![button(false)]);
    }

    /// The tuft's model as a grass request makes it: filling its cell, and `vertices` of them
    /// in one piece (a strip across it: nothing to thin).
    fn made_to_fill(ed: &mut MapEditor, vertices: usize) {
        let mut m = world::gltf_in::Model { min: glam::Vec3::new(-0.5, 0.0, -0.5), max: glam::Vec3::new(0.5, 0.4, 0.5), ..Default::default() };
        m.scene.plain.vertices = (0..vertices).map(|k| world::mesh::Vertex { pos: [k as f32 / vertices as f32 - 0.5, (k % 2) as f32 * 0.4, 0.0], ..Default::default() }).collect();
        m.scene.plain.indices = (0..vertices as u32 - 2).flat_map(|k| [k, k + 1, k + 2]).collect();
        ed.models.insert("tuft".into(), m);
        *ed.model_versions.entry("tuft".into()).or_default() += 1;
    }

    /// Ticked first, a grass prop made to fill its cells tiles them (one that isn't, scattered);
    /// one too heavy to grow is said so, with what it weighs a cell, tiled or scattered.
    #[test]
    fn grass_made_to_fill_its_cells_tiles_and_too_heavy_is_said() {
        let mut ed = MapEditor::new(Arc::new(with_grass()), None, crate::platform::Files::new("."));
        ed.terrain = "tall_grass".into();
        made_to_fill(&mut ed, 43_620);
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiGrows_tuft");
        let growth = |ed: &MapEditor| ed.content.terrains.get("tall_grass").unwrap().growth.clone();
        assert!(growth(&ed).tiled && growth(&ed).props.len() == 1, "made to fill its cell: tiled");
        grows_here(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("growthheavy").as_deref(), Some("1"), "43,620 vertices a cell: too heavy");
        assert_eq!(ed.grown_cost(&growth(&ed), "tuft"), 43_620);
        assert_eq!(ed.grown_cost(&Growth { density: 4.0, ..Default::default() }, "tuft"), 174_480, "scattered four to a cell");
        // made lighter: it grows
        made_to_fill(&mut ed, 900);
        grows_here(&ctx, &mut ed, vec![]);
        assert_eq!(crate::platform::hook("growthheavy").as_deref(), Some("0"));
        // untick it, tick the clover (a stand-in, not made to fill its cell): scattered
        click(&ctx, &mut ed, "uiGrows_tuft");
        assert!(growth(&ed).is_empty());
        click(&ctx, &mut ed, "uiGrows_clover");
        assert!(!growth(&ed).tiled && growth(&ed).props.len() == 1);
        // and switched by hand
        grows_here(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiGrowthTiled");
        assert!(growth(&ed).tiled);
    }

    /// A terrain's growth changed in the editor: the content holds it and it's due to be saved
    /// (the same change again, nothing), its record's text with it; none grows on a terrain
    /// that isn't tall ground.
    #[test]
    fn a_terrain_changed_is_due_to_be_saved_with_its_growth() {
        let mut ed = MapEditor::new(Arc::new(with_grass()), None, crate::platform::Files::new("."));
        assert!(!ed.unsaved());
        ed.change_terrain("tall_grass", |t| {
            t.growth.props.push(Grower { prop: "tuft".into(), weight: 2.0 });
            t.growth.density = 6.0;
        });
        assert!(ed.terrains_dirty.contains("tall_grass") && ed.unsaved());
        ed.terrains_dirty.clear();
        ed.change_terrain("tall_grass", |t| t.growth.density = 6.0);
        assert!(ed.terrains_dirty.is_empty(), "nothing changed");
        let t = ed.content.terrains.get("tall_grass").unwrap();
        let back: Terrain = serde_json::from_str(&content::to_text(t)).unwrap();
        assert_eq!((back.growth.props.clone(), back.growth.density), (vec![Grower { prop: "tuft".into(), weight: 2.0 }], 6.0));
        // without any growing, the record says nothing of it
        assert!(!content::to_text(ed.content.terrains.get("grass").unwrap()).contains("growth"));
    }
}
