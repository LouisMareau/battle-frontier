//! The Abilities window: every ability (a move record) of both spaces, one picked to edit,
//! or a new one. Every field is here: what it does in battle (type, category, power,
//! accuracy, PP, priority, target, the engine's effect and its chance, contact, flags, the
//! description), what it gains at each move level, the stronger abilities it becomes
//! (made separately, named here with what brings the change about), and the way to its
//! timeline (`timeline.rs`). Changes show at once and wait to be saved (Save, or Ctrl+S
//! with the map): a new ability goes to temporary/ unless told otherwise, an old one back
//! where it came from; in the browser the record is downloaded.

use std::collections::BTreeMap;
use std::sync::Arc;

use content::{Move, MoveEvolution, MoveLevel, Space};
use egui::{Color32, RichText};

use super::{badge, hint, mark, section_with, slug_ok, warn, MapEditor};

const TARGETS: [&str; 8] = ["selected", "both", "foes_and_ally", "user", "random", "depends", "user_or_selected", "opponents_field"];
const CATEGORIES: [&str; 3] = ["physical", "special", "status"];

pub(super) struct Abilities {
    pub open: bool,
    pub filter: String,
    /// the ability being edited
    pub picked: Option<String>,
    /// changed here and not saved yet, each as it was (None: made here)
    pub dirty: BTreeMap<String, Option<Move>>,
    /// where new abilities go: temporary/ (placeholders, never committed), else content/
    pub to_temporary: bool,
    pub new_id: String,
    /// the engine's effects, as the records spell them
    effects: Vec<String>,
}

impl Default for Abilities {
    fn default() -> Abilities {
        Abilities { open: false, filter: String::new(), picked: None, dirty: BTreeMap::new(), to_temporary: true, new_id: String::new(), effects: vec![] }
    }
}

/// The effects the battle rules know, as the records spell them (lowercase): the hits,
/// the others, and every stat up or down by one or two stages.
fn effects() -> Vec<String> {
    let mut out: Vec<String> = rules::moves::PLAIN_HITS.iter().chain(rules::moves::OTHER_EFFECTS).map(|e| e.to_lowercase()).collect();
    for stat in rules::moves::STAT_NAMES {
        for way in ["up", "down"] {
            out.push(format!("{}_{way}", stat.to_lowercase()));
            out.push(format!("{}_{way}_2", stat.to_lowercase()));
        }
    }
    out.sort();
    out.dedup();
    out
}

impl MapEditor {
    /// Changes an ability where it is (its space), to save later.
    pub(super) fn change_move(&mut self, id: &str, change: impl FnOnce(&mut Move)) {
        let Some(e) = self.content.moves.entry(id) else { return };
        let (mut m, space) = (e.value.clone(), e.space);
        change(&mut m);
        let was = self.content.moves.get(id).unwrap();
        if &m == was {
            return;
        }
        let was = was.clone();
        let mut c = (*self.content).clone();
        c.moves.put(m, space);
        self.content = Arc::new(c);
        self.abilities.dirty.entry(id.to_string()).or_insert(Some(was));
    }

    /// A new ability, plain, in the chosen space; false when the id won't do.
    pub(super) fn new_move(&mut self, id: &str) -> bool {
        if !slug_ok(id) || self.content.moves.contains(id) {
            self.status = "An ability's id: lowercase letters, digits, _ or -, and not taken".into();
            return false;
        }
        let ty = self.content.types.iter().next().map(|t| t.id.clone()).unwrap_or_default();
        let number = self.content.moves.iter().map(|m| m.number).max().unwrap_or(0) + 1;
        let m = Move {
            id: id.to_string(),
            name: super::assets::title(id),
            number,
            ty,
            category: "physical".into(),
            power: 40,
            accuracy: 100,
            pp: 20,
            target: "selected".into(),
            effect: "hit".into(),
            contact: true,
            ..Default::default()
        };
        let space = if self.abilities.to_temporary { Space::Temporary } else { Space::Content };
        let mut c = (*self.content).clone();
        c.moves.put(m, space);
        self.content = Arc::new(c);
        self.abilities.dirty.insert(id.to_string(), None);
        self.abilities.picked = Some(id.to_string());
        self.status = format!("{id}: a new ability, for {}", space.dir());
        true
    }

    /// Back to the ability as it was (a new one goes).
    fn revert_move(&mut self, id: &str) {
        let Some(was) = self.abilities.dirty.remove(id) else { return };
        let mut c = (*self.content).clone();
        match was {
            Some(m) => {
                let space = c.moves.entry(id).map(|e| e.space).unwrap_or(Space::Temporary);
                c.moves.put(m, space);
            }
            None => {
                c.moves.remove(id);
                if self.abilities.picked.as_deref() == Some(id) {
                    self.abilities.picked = None;
                }
            }
        }
        self.content = Arc::new(c);
    }

    /// Writes the abilities changed here: into their space on the desktop, kept in the
    /// browser (or downloaded).
    pub(super) fn save_abilities(&mut self) {
        let ids: Vec<String> = self.abilities.dirty.keys().cloned().collect();
        let mut done = 0;
        let mut errors = vec![];
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            for id in &ids {
                let Some(e) = self.content.moves.entry(id) else { continue };
                match content::disk::save(&root, e.space, &e.value) {
                    Ok(_) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            self.status = format!("Saved {done} abilit{}", if done == 1 { "y" } else { "ies" });
        }
        #[cfg(target_arch = "wasm32")]
        {
            for id in &ids {
                let Some(e) = self.content.moves.entry(id) else { continue };
                match super::saving::browser_save_record(e.space, &e.value, &format!("{id}.json")) {
                    Ok(_) => done += 1,
                    Err(err) => errors.push(err),
                }
            }
            let what = format!("{done} abilit{}", if done == 1 { "y" } else { "ies" });
            self.status = if super::saving::keeps() { format!("Saved {what} in this browser") } else { format!("Downloaded {what} (into <space>/data/moves/)") };
        }
        if errors.is_empty() {
            self.abilities.dirty.clear();
        } else {
            self.status = format!("{} · not saved: {}", self.status, errors.join("; "));
        }
    }

    /// The Abilities panel (docked or floating, as the layout has it), when it's open; its
    /// marks for the checks whether or not.
    pub(super) fn abilities_window(&mut self, ui: &mut egui::Ui) {
        crate::platform::set_hook("abilities", &self.content.moves.len().to_string());
        crate::platform::set_hook("abilitiesdirty", &self.abilities.dirty.len().to_string());
        crate::platform::set_hook("ability", self.abilities.picked.as_deref().unwrap_or(""));
        if !self.abilities.open {
            return;
        }
        if self.abilities.effects.is_empty() {
            self.abilities.effects = effects();
        }
        let mut pick: Option<String> = None;
        let mut make: Option<String> = None;
        let mut save = false;
        let mut revert: Option<String> = None;
        let mut edit_timeline: Option<String> = None;
        let mut after: Option<Move> = None;
        let content = self.content.clone();
        let picked = self.abilities.picked.clone();
        let mut st = self.dock.get(super::dock::Pane::Abilities);
        super::dock::show(ui, &mut st, super::dock::Pane::Abilities, "Abilities", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.abilities.filter).hint_text("Filter: name, id, type").desired_width(180.0));
                let field = ui.add(egui::TextEdit::singleline(&mut self.abilities.new_id).hint_text("a new ability's id").desired_width(140.0));
                mark("uiNewAbilityId", &field);
                let new = ui.button("New ability");
                mark("uiNewAbility", &new);
                if new.clicked() {
                    make = Some(self.abilities.new_id.trim().to_string());
                }
                ui.checkbox(&mut self.abilities.to_temporary, "New ones go to temporary/").on_hover_text("Placeholders, never committed. Off: the game's own content/");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.abilities.dirty.len();
                    let s = ui.add_enabled(n > 0, egui::Button::new(format!("Save {n} changed")));
                    mark("uiSaveAbilities", &s);
                    save = s.clicked();
                });
            });
            ui.separator();
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(240.0);
                    egui::ScrollArea::vertical().id_salt("ability-list").max_height(height).show(ui, |ui| {
                        let filter = self.abilities.filter.to_lowercase();
                        let rows: Vec<_> = content
                            .moves
                            .entries()
                            .iter()
                            .filter(|e| filter.is_empty() || e.value.id.contains(&filter) || e.value.name.to_lowercase().contains(&filter) || e.value.ty.contains(&filter))
                            .collect();
                        crate::platform::set_hook("abilitiesshown", &rows.len().to_string());
                        let changes = self.changes();
                        for e in rows {
                            let m = &e.value;
                            let dot = if self.abilities.dirty.contains_key(&m.id) { " •" } else { "" };
                            ui.horizontal(|ui| {
                                let r = ui.selectable_label(picked.as_deref() == Some(m.id.as_str()), format!("{}{dot}", m.name)).on_hover_text(format!("{} · {} · {}", m.id, m.ty, m.category));
                                mark(&format!("uiAbility_{}", m.id), &r);
                                if r.clicked() {
                                    pick = Some(m.id.clone());
                                }
                                super::review::record_mark(ui, changes.as_ref(), "moves", &m.id, "");
                            });
                        }
                        if content.moves.is_empty() {
                            hint(ui, "No abilities yet: give one an id and press New ability.");
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().id_salt("ability-form").max_height(height).show(ui, |ui| {
                        let Some(e) = picked.as_deref().and_then(|id| content.moves.entry(id)) else {
                            hint(ui, "Pick an ability on the left, or make a new one.");
                            return;
                        };
                        let m = &e.value;
                        let mut a = m.clone();
                        let changes = self.changes();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&m.name).size(16.0).strong());
                            badge(ui, e.space);
                            super::review::record_mark(ui, changes.as_ref(), "moves", &m.id, "");
                            ui.label(RichText::new(&m.id).monospace().size(11.0).color(Color32::GRAY));
                            if self.abilities.dirty.contains_key(&m.id) && ui.small_button("Revert").on_hover_text("Back to how it was saved").clicked() {
                                revert = Some(m.id.clone());
                            }
                        });
                        let changed = |prefixes: &[&str]| {
                            let found: Vec<(content::diff::Kind, String)> = prefixes.iter().filter_map(|p| super::review::changes_of(changes.as_ref(), "moves", &m.id, p)).collect();
                            (!found.is_empty()).then(|| (if found.iter().all(|(k, _)| *k == found[0].0) { found[0].0 } else { content::diff::Kind::Modified }, found.iter().map(|(_, a)| a.clone()).collect::<Vec<_>>().join("\n")))
                        };
                        section_with(ui, "In battle", changed(&["name", "type", "category", "power", "accuracy", "pp", "priority", "effects", "description", "target"]));
                        egui::Grid::new("ability-grid").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                            ui.label("Name");
                            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(180.0));
                            ui.label("Number");
                            ui.add(egui::DragValue::new(&mut a.number));
                            ui.end_row();
                            ui.label("Type");
                            let types: Vec<(String, String, String)> = content.types.iter().map(|t| (t.id.clone(), t.name.clone(), t.color.clone())).collect();
                            super::pick(ui, "ability-type", &mut a.ty, &types);
                            ui.label("Category");
                            egui::ComboBox::from_id_salt("ability-category").selected_text(&a.category).width(110.0).show_ui(ui, |ui| {
                                for c in CATEGORIES {
                                    ui.selectable_value(&mut a.category, c.to_string(), c);
                                }
                            });
                            ui.end_row();
                            ui.label("Power");
                            let r = ui.add(egui::DragValue::new(&mut a.power).range(0..=300)).on_hover_text("0: no damage of its own");
                            mark("uiAbilityPower", &r);
                            ui.label("Accuracy");
                            ui.add(egui::DragValue::new(&mut a.accuracy).range(0..=100).suffix("%")).on_hover_text("0: never misses");
                            ui.end_row();
                            ui.label("PP");
                            ui.add(egui::DragValue::new(&mut a.pp).range(1..=64));
                            ui.label("Priority");
                            ui.add(egui::DragValue::new(&mut a.priority).range(-7..=7)).on_hover_text("Higher goes first in a round");
                            ui.end_row();
                            ui.label("Target");
                            egui::ComboBox::from_id_salt("ability-target").selected_text(&a.target).width(150.0).show_ui(ui, |ui| {
                                for t in TARGETS {
                                    ui.selectable_value(&mut a.target, t.to_string(), t);
                                }
                            });
                            ui.label("Contact");
                            ui.checkbox(&mut a.contact, "touches the target").on_hover_text("The player presses A at impact; a ranged ability is held and released instead");
                            ui.end_row();
                            ui.label("Effect");
                            let known = self.abilities.effects.contains(&a.effect);
                            egui::ComboBox::from_id_salt("ability-effect").selected_text(if a.effect.is_empty() { "none" } else { &a.effect }).width(220.0).show_ui(ui, |ui| {
                                for ef in &self.abilities.effects {
                                    ui.selectable_value(&mut a.effect, ef.clone(), ef);
                                }
                            });
                            ui.label("Chance");
                            ui.add(egui::DragValue::new(&mut a.effect_chance).range(0..=100).suffix("%")).on_hover_text("Of the effect happening (0: always, for effects that always do)");
                            ui.end_row();
                            if !known {
                                ui.label("");
                                warn(ui, "The battle rules don't know this effect: the ability can't be used yet.");
                                ui.end_row();
                            }
                            ui.label("Flags");
                            let mut flags = a.flags.join(", ");
                            if ui.add(egui::TextEdit::singleline(&mut flags).desired_width(300.0)).on_hover_text("Comma-separated, as the rules read them (protect, sound, contact...)").changed() {
                                a.flags = flags.split(',').map(str::trim).filter(|f| !f.is_empty()).map(String::from).collect();
                            }
                            ui.end_row();
                        });
                        ui.label("Description");
                        ui.add(egui::TextEdit::multiline(&mut a.description).desired_rows(2).desired_width(f32::INFINITY));

                        section_with(ui, "Level-ups", changed(&["levels"]));
                        hint(ui, "What it gains at a move level, on top of the world's own growth (a field at 0 keeps the level before's).");
                        let mut gone: Option<usize> = None;
                        for (i, l) in a.levels.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Level {}", l.level)).strong());
                                ui.label("power");
                                ui.add(egui::DragValue::new(&mut l.power).range(0..=300));
                                ui.label("accuracy");
                                ui.add(egui::DragValue::new(&mut l.accuracy).range(0..=100));
                                ui.label("chance");
                                ui.add(egui::DragValue::new(&mut l.effect_chance).range(0..=100));
                                ui.label("hits");
                                ui.add(egui::DragValue::new(&mut l.hits).range(0..=10));
                                ui.add(egui::TextEdit::singleline(&mut l.note).hint_text("a note").desired_width(120.0));
                                if ui.small_button("×").on_hover_text("No change at this level").clicked() {
                                    gone = Some(i);
                                }
                            });
                        }
                        if let Some(i) = gone {
                            a.levels.remove(i);
                        }
                        ui.horizontal(|ui| {
                            for lv in 2..=5u8 {
                                if a.levels.iter().any(|l| l.level == lv) {
                                    continue;
                                }
                                let r = ui.small_button(format!("Add level {lv}"));
                                mark(&format!("uiAddLevel{lv}"), &r);
                                if r.clicked() {
                                    a.levels.push(MoveLevel { level: lv, ..Default::default() });
                                    a.levels.sort_by_key(|l| l.level);
                                }
                            }
                        });

                        section_with(ui, "Becomes", changed(&["evolutions"]));
                        hint(ui, "The stronger abilities it turns into, each its own record: at a move level, with an item, or both.");
                        let others: Vec<(String, String)> = content.moves.iter().filter(|o| o.id != m.id).map(|o| (o.id.clone(), o.name.clone())).collect();
                        let items: Vec<(String, String)> = content.items.iter().map(|i| (i.id.clone(), i.name.clone())).collect();
                        let mut gone: Option<usize> = None;
                        for (i, ev) in a.evolutions.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let shown = others.iter().find(|(id, _)| *id == ev.into).map(|(_, n)| n.clone()).unwrap_or_else(|| if ev.into.is_empty() { "pick one".into() } else { ev.into.clone() });
                                egui::ComboBox::from_id_salt(("ability-into", i)).selected_text(shown).width(160.0).show_ui(ui, |ui| {
                                    for (id, name) in &others {
                                        ui.selectable_value(&mut ev.into, id.clone(), format!("{name} ({id})"));
                                    }
                                });
                                ui.label("at level");
                                ui.add(egui::DragValue::new(&mut ev.at_level).range(0..=5)).on_hover_text("0: an item alone brings it about");
                                ui.label("with");
                                let item = ev.item.clone().unwrap_or_default();
                                let shown = items.iter().find(|(id, _)| *id == item).map(|(_, n)| n.clone()).unwrap_or_else(|| "no item".into());
                                egui::ComboBox::from_id_salt(("ability-item", i)).selected_text(shown).width(140.0).show_ui(ui, |ui| {
                                    if ui.selectable_label(ev.item.is_none(), "no item").clicked() {
                                        ev.item = None;
                                    }
                                    for (id, name) in &items {
                                        if ui.selectable_label(ev.item.as_deref() == Some(id), name).clicked() {
                                            ev.item = Some(id.clone());
                                        }
                                    }
                                });
                                ui.add(egui::TextEdit::singleline(&mut ev.note).hint_text("a note").desired_width(120.0));
                                if ui.small_button("×").clicked() {
                                    gone = Some(i);
                                }
                            });
                        }
                        if let Some(i) = gone {
                            a.evolutions.remove(i);
                        }
                        let add = ui.small_button("Add what it becomes");
                        mark("uiAddEvolution", &add);
                        if add.clicked() {
                            a.evolutions.push(MoveEvolution { at_level: 3, ..Default::default() });
                        }

                        section_with(ui, "Timeline", changed(&["timeline"]));
                        let t = &a.timeline;
                        ui.horizontal(|ui| {
                            hint(ui, &format!("{} clip{} over {:.1} s: animations, motions, FX, sounds, the player's inputs, the camera.", t.clips.len(), if t.clips.len() == 1 { "" } else { "s" }, t.seconds));
                            let r = ui.button("Edit the timeline").on_hover_text("The Timeline panel: the ability playing out on the map");
                            mark("uiEditTimeline", &r);
                            if r.clicked() {
                                edit_timeline = Some(m.id.clone());
                            }
                        });
                        if a != *m {
                            after = Some(a);
                        }
                    });
                });
            });
        });
        if let Some(a) = after {
            let id = a.id.clone();
            self.change_move(&id, move |q| *q = a);
        }
        if let Some(id) = pick {
            self.abilities.picked = Some(id);
        }
        if let Some(id) = make {
            if self.new_move(&id) {
                self.abilities.new_id.clear();
            }
        }
        if let Some(id) = revert {
            self.revert_move(&id);
        }
        if save {
            self.save_abilities();
        }
        if let Some(id) = edit_timeline {
            self.open_timeline(&id);
        }
        if !st.open {
            self.abilities.open = false;
        }
        self.dock.set(super::dock::Pane::Abilities, st);
    }
}
