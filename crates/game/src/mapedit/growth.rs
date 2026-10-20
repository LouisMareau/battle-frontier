//! Grass props grown through tall ground (`content::Growth`): the Ground tool's "Grows here",
//! for the tall terrain picked to paint (the grass props ticked, each with how often it's
//! picked, and how they grow: so many to a cell, their sizes, patches, turned any way), and
//! the asset browser's "Grows in" for a grass prop (the tall terrains it grows through). Both
//! change the terrain's record, saved with the rest (Ctrl+S, or by itself); the map's squares
//! show it as it changes (`stream`). Grass props are still placed one by one as any prop.

use std::sync::Arc;

use content::{Grower, Terrain};

use super::{hint, mark, section, MapEditor};

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

    /// How many vertices a prop's model has (its real one once loaded, else its stand-in).
    fn prop_vertices(&self, id: &str) -> usize {
        match (self.models.get(id), self.content.props.get(id)) {
            (Some(m), _) => m.scene.plain.vertices.len() + m.scene.textured.values().map(|t| t.vertices.len()).sum::<usize>(),
            (None, Some(p)) => world::mesh::prop_model(p).vertices.len(),
            _ => 0,
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
        if g.is_empty() {
            hint(ui, "None ticked: the engine's own blades.");
        } else {
            let d = ui.add(egui::Slider::new(&mut g.density, 0.0..=content::Growth::MOST).step_by(0.5).text("to a cell")).on_hover_text("How many grow on each cell, on average");
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
            let turn = ui.checkbox(&mut g.turn, "Each turned its own way");
            mark("uiGrowthTurn", &turn);
            // what it costs: the vertices a cell, the props' average as picked
            let total: f64 = g.props.iter().map(|p| p.weight.max(0.0)).sum();
            let each: f64 = g.props.iter().map(|p| p.weight.max(0.0) / total.max(1e-9) * self.prop_vertices(&p.prop) as f64).sum();
            hint(ui, &format!("About {} vertices a cell", (each * g.density).round() as u64));
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
