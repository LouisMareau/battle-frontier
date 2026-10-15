//! The start of the editor: a browser of all the content, both spaces. Temporary records
//! (placeholders imported locally, never committed) are flagged on every row
//! and page, so it's always clear what still has to be replaced by the game's own.

use std::collections::HashMap;
use std::sync::Arc;

use content::{Content, Record, Space};
use egui::{Color32, RichText};

use crate::platform::{Files, Load};

const TEMP: Color32 = Color32::from_rgb(240, 170, 60);
const OWN: Color32 = Color32::from_rgb(110, 210, 130);

struct Row {
    id: String,
    name: String,
    number: Option<i64>,
    space: Space,
    json: serde_json::Value,
}

fn rows_of<T: Record>(c: &content::Collection<T>) -> Vec<Row> {
    c.entries()
        .iter()
        .map(|e| {
            let json = serde_json::to_value(&e.value).unwrap_or_default();
            Row {
                id: e.value.id().to_string(),
                name: json.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                number: json.get("number").and_then(|v| v.as_i64()),
                space: e.space,
                json,
            }
        })
        .collect()
}

fn rows(content: &Content, collection: &str) -> Vec<Row> {
    match collection {
        "types" => rows_of(&content.types),
        "creatures" => rows_of(&content.creatures),
        "moves" => rows_of(&content.moves),
        "abilities" => rows_of(&content.abilities),
        "items" => rows_of(&content.items),
        "natures" => rows_of(&content.natures),
        "growth_rates" => rows_of(&content.growth_rates),
        "egg_groups" => rows_of(&content.egg_groups),
        "regions" => rows_of(&content.regions),
        "encounters" => rows_of(&content.encounters),
        "parties" => rows_of(&content.parties),
        "settings" => rows_of(&content.settings),
        "terrains" => rows_of(&content.terrains),
        "props" => rows_of(&content.props),
        "maps" => rows_of(&content.maps),
        "spawns" => rows_of(&content.spawns),
        "worlds" => rows_of(&content.worlds),
        "templates" => rows_of(&content.templates),
        _ => vec![],
    }
}

fn badge(ui: &mut egui::Ui, space: Space) {
    let (text, color) = match space {
        Space::Temporary => ("TEMPORARY", TEMP),
        Space::Content => ("GAME", OWN),
    };
    ui.label(RichText::new(text).size(10.0).strong().color(Color32::BLACK).background_color(color));
}

pub struct Browser {
    content: Arc<Content>,
    collection: &'static str,
    rows: Vec<Row>,
    filter: String,
    selected: usize,
    pictures: HashMap<String, Option<egui::TextureHandle>>,
}

impl Browser {
    pub fn new(content: Arc<Content>) -> Browser {
        let rows = rows(&content, "creatures");
        Browser { content, collection: "creatures", rows, filter: String::new(), selected: 0, pictures: HashMap::new() }
    }

    fn picture(&mut self, ctx: &egui::Context, files: &Files, path: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.pictures.get(path) {
            return t.clone();
        }
        match files.get(path) {
            Load::Pending => None,
            Load::Failed(_) => {
                self.pictures.insert(path.to_string(), None);
                None
            }
            Load::Ready(bytes) => {
                let t = image::load_from_memory(&bytes).ok().map(|img| {
                    let img = img.to_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    ctx.load_texture(path, egui::ColorImage::from_rgba_unmultiplied(size, &img), egui::TextureOptions::NEAREST)
                });
                self.pictures.insert(path.to_string(), t.clone());
                t
            }
        }
    }

    /// Draws the browser; true when the player asks to go back to the game.
    pub fn ui(&mut self, ui: &mut egui::Ui, files: &Files) -> bool {
        let ctx = ui.ctx().clone();
        let mut back = false;
        egui::Panel::top("editor-top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Content").strong());
                ui.add_space(12.0);
                if ui.button("Back to the game").clicked() {
                    back = true;
                }
                ui.add_space(12.0);
                badge(ui, Space::Temporary);
                ui.label(RichText::new("imported placeholders, never committed").color(Color32::GRAY));
                badge(ui, Space::Content);
                ui.label(RichText::new("the game's own content (content/)").color(Color32::GRAY));
            });
            ui.add_space(6.0);
        });
        egui::Panel::left("editor-collections").default_size(210.0).show(ui, |ui| {
            ui.add_space(6.0);
            let counts = self.content.counts();
            for name in content::COLLECTIONS {
                let temp = counts.get(&(name, Space::Temporary)).copied().unwrap_or(0);
                let own = counts.get(&(name, Space::Content)).copied().unwrap_or(0);
                let label = format!("{}  {}", name.replace('_', " "), temp + own);
                if ui.selectable_label(self.collection == name, label).clicked() && self.collection != name {
                    self.collection = name;
                    self.rows = rows(&self.content, name);
                    self.selected = 0;
                    self.filter.clear();
                }
                if temp > 0 {
                    ui.label(RichText::new(format!("   {temp} temporary · {own} game")).size(10.5).color(if own == 0 { TEMP } else { Color32::GRAY }));
                }
            }
            let problems = self.content.validate();
            ui.add_space(10.0);
            ui.label(RichText::new(if problems.is_empty() { "Every reference resolves.".to_string() } else { format!("{} problems", problems.len()) }).color(if problems.is_empty() { OWN } else { Color32::from_rgb(255, 120, 100) }));
        });
        egui::Panel::left("editor-list").default_size(300.0).show(ui, |ui| {
            ui.add_space(6.0);
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter by name or id"));
            ui.add_space(4.0);
            let f = self.filter.to_lowercase();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, row) in self.rows.iter().enumerate() {
                    if !f.is_empty() && !row.id.contains(&f) && !row.name.to_lowercase().contains(&f) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        badge(ui, row.space);
                        let n = row.number.map(|n| format!("#{n:03} ")).unwrap_or_default();
                        let text = if row.name.is_empty() { format!("{n}{}", row.id) } else { format!("{n}{}  ({})", row.name, row.id) };
                        if ui.selectable_label(i == self.selected, text).clicked() {
                            self.selected = i;
                        }
                    });
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(row) = self.rows.get(self.selected) else {
                ui.label("Nothing in this collection yet.");
                return;
            };
            let (space, id, name, json) = (row.space, row.id.clone(), row.name.clone(), row.json.clone());
            ui.horizontal(|ui| {
                badge(ui, space);
                ui.heading(if name.is_empty() { id.clone() } else { name });
                ui.label(RichText::new(format!("{}/data/{}/{id}.json", space.dir(), self.collection)).monospace().color(Color32::GRAY));
            });
            if space == Space::Temporary {
                ui.label(RichText::new("A placeholder, imported locally. It stays out of the repository; the game's own record with the same id will replace it.").color(TEMP));
            }
            ui.add_space(6.0);
            // pictures named by the record (creature sprites, item icons...)
            let mut paths = vec![];
            if let Some(sprites) = json.get("sprites") {
                for k in ["front", "back", "front_shiny", "back_shiny", "icon"] {
                    if let Some(p) = sprites.get(k).and_then(|v| v.as_str()) {
                        paths.push((k, space.asset(p)));
                    }
                }
            }
            for k in ["icon", "trainer_sprite"] {
                if let Some(p) = json.get(k).and_then(|v| v.as_str()) {
                    paths.push((k, space.asset(p)));
                }
            }
            if !paths.is_empty() {
                ui.horizontal(|ui| {
                    for (label, path) in &paths {
                        if let Some(t) = self.picture(&ctx, files, path) {
                            ui.vertical(|ui| {
                                let size = t.size_vec2() * 2.0;
                                ui.image((t.id(), size));
                                ui.label(RichText::new(*label).size(10.0).color(Color32::GRAY));
                            });
                        }
                    }
                });
            }
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let text = serde_json::to_string_pretty(&json).unwrap_or_default();
                ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0)).wrap());
            });
        });
        back
    }
}
