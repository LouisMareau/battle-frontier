//! Templates in the editor: what stands on an area of the map, captured (Select, "Capture
//! an area") or one thing made into one (the inspector's "Make a template"), listed in the
//! Select panel and dropped onto any map from there: a copy of everything in it, as it was
//! (props on their cells and within them, buildings, actors with their events, events with
//! their scripts). Kept as `content/data/templates/<id>.json` (`temporary/` while it names
//! placeholders).

use std::collections::BTreeSet;
use std::sync::Arc;

use content::{Space, Template};
use egui::{RichText, Sense};
use glam::Vec3;
use render::scene::{Shape, ShapeKind};
use world::templates::{capture, stamp, Picks};

use super::{hint, mark, section, MapEditor};

/// A template being dragged: the payload's prefix (a plain id is a prop).
pub(super) const PAYLOAD: &str = "template:";

#[derive(Default)]
pub(super) struct Templates {
    /// a capture: the area is dragged on the map
    pub capturing: bool,
    pub drag: Option<((i32, i32), (i32, i32))>,
    /// the name the next one takes
    pub name: String,
    pub selected: Option<String>,
    /// changed and not saved yet
    pub dirty: BTreeSet<String>,
    /// what's dragged over the map: a template's corner (or a prop's centre), and whether it
    /// can go there
    pub ghost: Option<Ghost>,
}

#[derive(Clone, Debug)]
pub(super) enum Ghost {
    Template { id: String, corner: [i32; 2], fits: bool },
    Prop { id: String, at: [f64; 2], level: u8 },
}

fn slug(name: &str) -> String {
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

fn area_of(a: (i32, i32), b: (i32, i32)) -> [i32; 4] {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
    [x0, z0, x1 - x0 + 1, z1 - z0 + 1]
}

/// The smallest area holding these cells.
fn bounds(cells: &[(i32, i32)]) -> Option<[i32; 4]> {
    let (x0, z0) = (cells.iter().map(|c| c.0).min()?, cells.iter().map(|c| c.1).min()?);
    let (x1, z1) = (cells.iter().map(|c| c.0).max()?, cells.iter().map(|c| c.1).max()?);
    Some([x0, z0, x1 - x0 + 1, z1 - z0 + 1])
}

impl MapEditor {
    /// A new template of what `picks` names, from `area`, saved at once.
    pub(super) fn make_template(&mut self, area: [i32; 4], picks: &Picks) {
        if picks.is_empty() {
            self.status = "Nothing stands wholly inside that area: no template".into();
            return;
        }
        let n = self.content.templates.iter().count() + 1;
        let name = match self.tpl.name.trim() {
            "" => format!("Template {n}"),
            s => s.to_string(),
        };
        let base = match slug(&name) {
            s if s.is_empty() => "template".to_string(),
            s => s,
        };
        let mut id = base.clone();
        let mut k = 2;
        while self.content.templates.contains(&id) {
            id = format!("{base}_{k}");
            k += 1;
        }
        let t = capture(&self.map, &id, &name, area, picks);
        // the game's own, unless it names placeholders
        let placeholder = |space: Option<Space>| space == Some(Space::Temporary);
        let temporary = t.props.iter().any(|p| placeholder(self.content.props.entry(&p.prop).map(|e| e.space)))
            || t.actors.iter().any(|a| a.look.strip_prefix("creature:").map(|c| placeholder(self.content.creatures.entry(c).map(|e| e.space))).unwrap_or(false));
        let space = if temporary { Space::Temporary } else { Space::Content };
        let summary = t.summary();
        let mut c = (*self.content).clone();
        c.templates.put(t, space);
        c.sort();
        self.content = Arc::new(c);
        self.tpl.dirty.insert(id.clone());
        self.tpl.selected = Some(id.clone());
        self.tpl.name.clear();
        self.save_templates();
        self.status = format!("Template {name}: {summary}, {}×{} cells · {}", area[2], area[3], self.status);
        crate::platform::set_hook("tplnew", &id);
    }

    /// A template of one placed prop.
    pub(super) fn template_of_prop(&mut self, i: usize) {
        let Some(p) = self.map.props.get(i) else { return };
        let Some(area) = bounds(&self.map.footprint(p, &self.content)) else { return };
        self.make_template(area, &Picks { props: vec![i], ..Default::default() });
    }

    /// A template of one building, with what's inside and outside it.
    pub(super) fn template_of_building(&mut self, i: usize) {
        let Some(b) = self.map.buildings.get(i) else { return };
        let Some(area) = bounds(&b.map_cells()) else { return };
        self.make_template(area, &Picks { buildings: vec![i], ..Default::default() });
    }

    /// A template of one actor, with its events, its area and its path.
    pub(super) fn template_of_actor(&mut self, i: usize) {
        let Some(a) = self.map.actors.get(i) else { return };
        let mut cells = vec![(a.at[0], a.at[1])];
        cells.extend(a.path.iter().map(|c| (c[0], c[1])));
        if a.area[2] > 0 && a.area[3] > 0 {
            cells.push((a.area[0], a.area[1]));
            cells.push((a.area[0] + a.area[2] - 1, a.area[1] + a.area[3] - 1));
        }
        let Some(area) = bounds(&cells) else { return };
        let area = [area[0].max(0), area[1].max(0), area[2], area[3]];
        self.make_template(area, &Picks { actors: vec![i], ..Default::default() });
    }

    /// Writes the changed templates: the files on the desktop, downloads in the browser.
    pub(super) fn save_templates(&mut self) {
        let ids: Vec<String> = self.tpl.dirty.iter().cloned().collect();
        let mut errors = vec![];
        let mut done = 0;
        for id in &ids {
            let Some(e) = self.content.templates.entry(id) else { continue };
            #[cfg(not(target_arch = "wasm32"))]
            let saved = {
                let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
                content::disk::save(&root, e.space, &e.value).map(|_| ())
            };
            #[cfg(target_arch = "wasm32")]
            let saved = crate::platform::download(&format!("{id}.json"), &content::to_text(&e.value));
            match saved {
                Ok(()) => done += 1,
                Err(err) => errors.push(err),
            }
        }
        self.tpl.dirty.clear();
        self.status = match (done, errors.is_empty()) {
            (_, false) => format!("Templates not saved: {}", errors.join("; ")),
            (0, true) => String::new(),
            (n, true) if cfg!(target_arch = "wasm32") => format!("Downloaded {n} template{}: put it in content/data/templates/", if n == 1 { "" } else { "s" }),
            (n, true) => format!("Saved {n} template{}", if n == 1 { "" } else { "s" }),
        };
    }

    /// Takes a template out of the project (its file too, on the desktop).
    fn delete_template(&mut self, id: &str) {
        let Some(e) = self.content.templates.entry(id).cloned() else { return };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            let _ = std::fs::remove_file(root.join(e.space.dir()).join("data").join("templates").join(format!("{id}.json")));
        }
        let mut c = (*self.content).clone();
        c.templates.remove(id);
        self.content = Arc::new(c);
        self.tpl.dirty.remove(id);
        self.tpl.selected = None;
        self.status = format!("Template {} deleted", e.value.name);
    }

    // ------------------------------------------------------------ the panel

    pub(super) fn templates_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Templates");
        hint(ui, "Things on the map kept to put down again anywhere, as they are: drag one from here onto the map.");
        ui.horizontal(|ui| {
            ui.label("Name");
            let r = ui.add(egui::TextEdit::singleline(&mut self.tpl.name).hint_text("the next one's").desired_width(150.0));
            mark("uiTemplateName", &r);
        });
        let capture = ui.selectable_label(self.tpl.capturing, "Capture an area (drag cells)").on_hover_text("Everything wholly inside the cells you drag: props, buildings, actors, events. Esc stops.");
        mark("uiCapture", &capture);
        if capture.clicked() {
            self.tpl.capturing = !self.tpl.capturing;
            self.tpl.drag = None;
        }
        let list: Vec<(String, String, String, Space)> = self.content.templates.entries().iter().map(|e| (e.value.id.clone(), e.value.name.clone(), format!("{}×{} · {}", e.value.size[0], e.value.size[1], e.value.summary()), e.space)).collect();
        if list.is_empty() {
            hint(ui, "None yet: capture an area, or use \"Make a template\" on what's selected.");
        }
        let mut delete = None;
        for (id, name, about, space) in list {
            let chosen = self.tpl.selected.as_deref() == Some(id.as_str());
            let dirty = if self.tpl.dirty.contains(&id) { " •" } else { "" };
            let r = ui
                .add(egui::Button::selectable(chosen, RichText::new(format!("{name}{dirty}"))).sense(Sense::click_and_drag()))
                .on_hover_text(format!("{about}{} · drag onto the map", if space == Space::Temporary { " · temporary" } else { "" }));
            r.dnd_set_drag_payload(format!("{PAYLOAD}{id}"));
            mark(&format!("uiTemplate_{id}"), &r);
            if r.clicked() {
                self.tpl.selected = if chosen { None } else { Some(id.clone()) };
            }
            if chosen {
                hint(ui, &about);
                let mut name2 = name.clone();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut name2).desired_width(140.0));
                });
                if name2 != name {
                    if let Some(e) = self.content.templates.entry(&id).cloned() {
                        let mut c = (*self.content).clone();
                        c.templates.put(Template { name: name2, ..e.value }, e.space);
                        self.content = Arc::new(c);
                        self.tpl.dirty.insert(id.clone());
                    }
                }
                ui.horizontal(|ui| {
                    if !self.tpl.dirty.is_empty() && ui.button("Save").on_hover_text("Ctrl+S saves it too").clicked() {
                        self.save_templates();
                    }
                    if ui.button("Delete").clicked() {
                        delete = Some(id.clone());
                    }
                });
            }
        }
        if let Some(id) = delete {
            self.delete_template(&id);
        }
        crate::platform::set_hook("templates", &self.content.templates.iter().count().to_string());
        crate::platform::set_hook("capturing", if self.tpl.capturing { "1" } else { "0" });
    }

    // ------------------------------------------------------------ on the map

    /// A capture: the cells dragged; on release, the template.
    pub(super) fn capture_view(&mut self, pressed: bool, down: bool, released: bool) {
        let cell = self.ground_hit;
        if let (true, Some(c)) = (pressed, cell) {
            self.tpl.drag = Some((c, c));
        }
        if let (true, Some(c), Some(d)) = (down, cell, self.tpl.drag.as_mut()) {
            d.1 = c;
        }
        if let (true, Some((a, b))) = (released, self.tpl.drag) {
            self.tpl.drag = None;
            self.tpl.capturing = false;
            let area = area_of(a, b);
            let picks = Picks::inside(&self.map, &self.content, area);
            self.make_template(area, &picks);
        }
    }

    /// Something dragged over the map from a list: where it would land.
    pub(super) fn drag_over(&mut self, payload: &str) {
        let Some((x, z)) = self.ground_hit else {
            self.tpl.ghost = None;
            return;
        };
        self.tpl.ghost = match payload.strip_prefix(PAYLOAD) {
            Some(id) => self.content.templates.get(id).map(|t| {
                let corner = [x - t.size[0] as i32 / 2, z - t.size[1] as i32 / 2];
                let mut probe = self.map.clone();
                let fits = stamp(&mut probe, t, corner, self.max_level).is_ok();
                Ghost::Template { id: id.to_string(), corner, fits }
            }),
            None => self.content.props.get(payload).map(|_| {
                let at = self.snap(payload, self.turn, Vec3::new(x as f32 + 0.5, 0.0, z as f32 + 0.5));
                Ghost::Prop { id: payload.to_string(), at, level: self.map.level_at(x, z).unwrap_or(0) }
            }),
        };
    }

    /// Let go over the map: the template's copy (or the prop) put down there.
    pub(super) fn drop_on_map(&mut self, payload: &str) {
        self.drag_over(payload);
        let ghost = self.tpl.ghost.take();
        match ghost {
            Some(Ghost::Template { id, corner, .. }) => {
                let Some(t) = self.content.templates.get(&id).cloned() else { return };
                super::make_room(&self.map);
                let before = self.map.clone();
                match stamp(&mut self.map, &t, corner, self.max_level) {
                    Ok(s) => {
                        self.undo.push(before);
                        self.redo.clear();
                        self.dirty = true;
                        for &i in &s.buildings {
                            self.settle(i);
                        }
                        self.map.know_doors(&self.content);
                        self.rebuild_actors();
                        self.need_props = true;
                        self.need_buildings = true;
                        self.blocked = None;
                        let renamed = if s.renamed.is_empty() { String::new() } else { format!(" ({})", s.renamed.iter().map(|(a, b)| format!("{a} is {b} here")).collect::<Vec<_>>().join(", ")) };
                        self.status = format!("Put down {}: {}{renamed}", t.name, t.summary());
                        crate::platform::set_hook("stamped", &format!("{id}@{},{}", corner[0], corner[1]));
                    }
                    Err(e) => self.status = e,
                }
            }
            Some(Ghost::Prop { id, at, level }) => {
                let placed = content::Placed { prop: id.clone(), at, level, turn: self.turn as f64, ..Default::default() };
                let cells = super::rooms::cells_of(self, &placed);
                match super::rooms::clash(self, &id, &cells, &super::rooms::ground_items(self, level, None)) {
                    Some(other) => self.status = format!("No room: {other} is there, and one of them doesn't stack"),
                    None => {
                        self.checkpoint();
                        self.map.props.push(placed);
                        self.selected = Some(self.map.props.len() - 1);
                        self.need_props = true;
                    }
                }
            }
            None => {}
        }
    }

    /// The capture's cells, and what's dragged over the map.
    pub(super) fn template_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        let y_at = |x: i32, z: i32| self.map.level_at(x, z).unwrap_or(0) as f32 * lh + 0.07;
        let rect = |x: f32, z: f32, w: f32, d: f32, y: f32, color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: w, depth: d }, center: Vec3::new(x + w / 2.0, y, z + d / 2.0), color, additive: false };
        if let Some((a, b)) = self.tpl.drag {
            let r = area_of(a, b);
            shapes.push(rect(r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32, y_at(r[0], r[1]), [1.0, 1.0, 1.0, 0.3]));
        }
        match &self.tpl.ghost {
            Some(Ghost::Template { id, corner, fits }) => {
                let Some(t) = self.content.templates.get(id) else { return };
                let (x0, z0) = (corner[0], corner[1]);
                let tone = if *fits { [0.45, 0.95, 0.55, 0.22] } else { [1.0, 0.35, 0.3, 0.3] };
                shapes.push(rect(x0 as f32, z0 as f32, t.size[0] as f32, t.size[1] as f32, y_at(x0, z0), tone));
                for p in &t.props {
                    let fp = self.content.props.get(&p.prop).map(|q| p.turned(q.footprint)).unwrap_or((1, 1));
                    let (w, d) = (fp.0 as f32, fp.1 as f32);
                    let (x, z) = (x0 as f32 + p.at[0] as f32 - w / 2.0, z0 as f32 + p.at[1] as f32 - d / 2.0);
                    shapes.push(rect(x, z, w, d, y_at(x.floor() as i32, z.floor() as i32) + 0.01, [1.0, 1.0, 1.0, 0.35]));
                }
                for b in &t.buildings {
                    for (x, z) in b.map_cells() {
                        shapes.push(rect((x + x0) as f32, (z + z0) as f32, 1.0, 1.0, y_at(x + x0, z + z0) + 0.01, [1.0, 0.85, 0.2, 0.35]));
                    }
                }
                for a in &t.actors {
                    let (x, z) = (a.at[0] + x0, a.at[1] + z0);
                    shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.32, outer: 0.45 }, center: Vec3::new(x as f32 + 0.5, y_at(x, z) + 0.02, z as f32 + 0.5), color: [0.75, 0.45, 0.95, 0.8], additive: false });
                }
            }
            Some(Ghost::Prop { id, at, level }) => {
                let fp = self.content.props.get(id).map(|q| q.footprint).unwrap_or([1, 1]);
                let quarter = ((self.turn / 90.0).round() as i64).rem_euclid(2) == 1;
                let (w, d) = if quarter { (fp[1] as f32, fp[0] as f32) } else { (fp[0] as f32, fp[1] as f32) };
                shapes.push(rect(at[0] as f32 - w / 2.0, at[1] as f32 - d / 2.0, w, d, *level as f32 * lh + 0.07, [1.0, 1.0, 1.0, 0.4]));
            }
            None => {}
        }
    }
}
