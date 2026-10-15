//! The Zones tool: areas creatures roam, who lives in each by time of day (the map's
//! `spawns` record, saved in `temporary/` while it names placeholders), and links that send
//! creatures out of a treeline to a zone (drag from the forest's edge onto the zone). The
//! creatures roam the map while "Creatures" is on; "Debug" shows every area, link and path.

use content::{Space, SpawnLink, Zone};
use egui::RichText;
use glam::{Vec2, Vec3};
use render::scene::{Billboard, SceneRenderer, Shape, ShapeKind};
use rules::config::Period;
use world::spawn::{Phase, Pools};

use super::{hint, mark, section, MapEditor};
use crate::platform::Load;

const ZONE_COLORS: [[f32; 4]; 6] = [
    [0.95, 0.75, 0.2, 0.35],
    [0.85, 0.35, 0.75, 0.35],
    [0.3, 0.8, 0.95, 0.35],
    [0.95, 0.45, 0.3, 0.35],
    [0.55, 0.9, 0.35, 0.35],
    [0.6, 0.55, 0.95, 0.35],
];
const LINK: [f32; 4] = [1.0, 0.55, 0.1, 0.85];

pub(super) fn period_key(p: Period) -> &'static str {
    match p {
        Period::Morning => "morning",
        Period::Day => "day",
        Period::Night => "night",
    }
}

fn quad(corners: [Vec3; 4], color: [f32; 4]) -> Shape {
    Shape { kind: ShapeKind::Quad { corners }, center: Vec3::ZERO, color, additive: false }
}

/// A flat arrow on the ground, from one point to another.
pub(super) fn arrow(from: Vec3, to: Vec3, width: f32, color: [f32; 4], out: &mut Vec<Shape>) {
    let d = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let len = d.length();
    if len < 0.2 {
        return;
    }
    let dir = d / len;
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let head = (len * 0.3).clamp(0.3, 0.9);
    let neck = to - dir * head;
    let (a, b) = (from + side * width / 2.0, from - side * width / 2.0);
    let (c, e) = (neck - side * width / 2.0, neck + side * width / 2.0);
    out.push(quad([a, b, c, e], color));
    let w = width * 2.2;
    out.push(quad([neck + side * w / 2.0, neck - side * w / 2.0, to, to], color));
}

impl MapEditor {
    /// The map's spawns record (or a new, empty one).
    pub(super) fn load_spawns(&mut self) {
        self.spawns = match self.content.spawns.entry(&self.map.id) {
            Some(e) => e.value.clone(),
            None => content::Spawns { id: self.map.id.clone(), ..Default::default() },
        };
        self.spawner.reset();
    }

    fn pools(&self) -> Pools {
        let period = period_key(self.period);
        self.map.zones.iter().map(|z| self.spawns.zones.get(&z.mark).and_then(|p| p.get(period)).cloned().unwrap_or_default()).collect()
    }

    /// A frame of the creatures' lives.
    pub(super) fn tick(&mut self, now: f64) {
        let dt = ((now - self.last_tick) / 1000.0).clamp(0.0, 0.1);
        self.last_tick = now;
        if !self.creatures_on || self.map.zones.is_empty() {
            return;
        }
        if self.blocked.is_none() {
            self.blocked = Some(self.map.blocked(&self.content));
        }
        let pools = self.pools();
        self.spawner.step(&self.map, self.blocked.as_ref().unwrap(), &pools, dt);
        crate::platform::set_hook("roamers", &self.spawner.roamers.len().to_string());
        let roaming: Vec<String> = self.spawner.roamers.iter().take(8).map(|r| format!("{}:{:.2},{:.2}:{:?}", r.creature, r.pos.x, r.pos.y, r.phase)).collect();
        crate::platform::set_hook("roaming", &roaming.join(";"));
    }

    /// Who's in the zone list for this time of day: the selected zone's pool.
    fn pool_mut(&mut self, zone: usize) -> &mut Vec<(String, u8)> {
        let mark = self.map.zones[zone].mark.clone();
        let period = self.pool_period.clone();
        self.spawns.zones.entry(mark).or_default().entry(period).or_default()
    }

    // ------------------------------------------------------------ the panel

    pub(super) fn zones_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Zones");
        ui.horizontal(|ui| {
            mark("uiPaintZones", &ui.radio_value(&mut self.linking, false, "Paint zones"));
            mark("uiLinkMode", &ui.radio_value(&mut self.linking, true, "Link treelines"));
        });
        if self.linking {
            hint(ui, "Drag from the edge of a tree area onto a zone: creatures come out of those trees, roam the zone, then go back.");
        } else {
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.remove_cells, false, "Add cells");
                ui.radio_value(&mut self.remove_cells, true, "Remove");
            });
            ui.add(egui::Slider::new(&mut self.brush, 0..=4).custom_formatter(|v, _| format!("{0}×{0}", v as i32 * 2 + 1)));
            hint(ui, "Alt: the other way. Painting with no zone picked makes one.");
        }
        ui.add_space(4.0);
        for i in 0..self.map.zones.len() {
            let z = &self.map.zones[i];
            let cells = self.map.zone.iter().filter(|&&c| c as usize == i + 1).count();
            if ui.selectable_label(self.zone_sel == Some(i), format!("{} ({}) · {cells} cells", z.name, z.mark)).clicked() {
                self.zone_sel = Some(i);
            }
        }
        if ui.small_button("New zone").clicked() {
            self.checkpoint();
            self.new_zone();
        }
        if let Some(i) = self.zone_sel.filter(|&i| i < self.map.zones.len()) {
            section(ui, "Zone");
            let before = self.map.zones[i].clone();
            let mut remove = false;
            {
                let z = &mut self.map.zones[i];
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut z.name).desired_width(f32::INFINITY));
                });
                ui.add(egui::Slider::new(&mut z.count, 1..=12).text("at once"));
                ui.checkbox(&mut z.appear_inside, "Creatures appear inside").on_hover_text("Off: they only come from linked treelines");
                let (mut lo, mut hi) = (z.stay[0], z.stay[1]);
                ui.add(egui::Slider::new(&mut lo, 5.0..=120.0).text("stay at least").suffix(" s"));
                ui.add(egui::Slider::new(&mut hi, 5.0..=120.0).text("at most").suffix(" s"));
                z.stay = [lo.min(hi), hi.max(lo)];
                if ui.small_button("Remove this zone").clicked() {
                    remove = true;
                }
            }
            if self.map.zones[i] != before {
                self.dirty = true;
            }
            if remove {
                self.checkpoint();
                let mark = self.map.zones[i].mark.clone();
                self.map.remove_zone(i);
                self.spawns.zones.remove(&mark);
                self.zone_sel = None;
                self.spawner.reset();
                return;
            }
            self.pool_panel(ui, i);
        }
        section(ui, "Links");
        if self.map.links.is_empty() {
            hint(ui, "None yet: in Link treelines, drag a forest's edge onto a zone.");
        }
        let mut remove = None;
        let mut changed = false;
        for (k, l) in self.map.links.iter_mut().enumerate() {
            let from = self.map.tree_areas.iter().find(|a| a.mark == l.area).map(|a| a.name.clone()).unwrap_or_else(|| l.area.clone());
            let to = self.map.zones.iter().find(|z| z.mark == l.zone).map(|z| z.name.clone()).unwrap_or_else(|| l.zone.clone());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{from} ({} cells) to {to}", l.treeline.len())).size(12.5));
                if ui.small_button("remove").clicked() {
                    remove = Some(k);
                }
            });
            changed |= ui.add(egui::Slider::new(&mut l.every, 1.0..=60.0).text("every").suffix(" s")).changed();
            let (mut lo, mut hi) = (l.stay[0], l.stay[1]);
            changed |= ui.add(egui::Slider::new(&mut lo, 5.0..=120.0).text("stay at least").suffix(" s")).changed();
            changed |= ui.add(egui::Slider::new(&mut hi, 5.0..=120.0).text("at most").suffix(" s")).changed();
            l.stay = [lo.min(hi), hi.max(lo)];
            ui.add_space(4.0);
        }
        if changed {
            self.dirty = true;
        }
        if let Some(k) = remove {
            self.checkpoint();
            self.map.links.remove(k);
            self.spawner.reset();
        }
    }

    fn pool_panel(&mut self, ui: &mut egui::Ui, zone: usize) {
        section(ui, "Who lives here");
        ui.horizontal(|ui| {
            for p in ["morning", "day", "night"] {
                if ui.selectable_label(self.pool_period == p, p).clicked() {
                    self.pool_period = p.into();
                }
            }
        });
        let content = self.content.clone();
        let mut remove = None;
        let mut changed = false;
        let pool = self.pool_mut(zone).clone();
        for (k, (c, lv)) in pool.iter().enumerate() {
            ui.horizontal(|ui| {
                let e = content.creatures.entry(c);
                let name = e.map(|e| e.value.name.clone()).unwrap_or_else(|| c.clone());
                ui.label(name);
                if e.map(|e| e.space == Space::Temporary).unwrap_or(false) {
                    ui.label(RichText::new("temporary").size(10.0).color(super::TEMP));
                }
                let mut l = *lv;
                if ui.add(egui::DragValue::new(&mut l).range(1..=100).prefix("Lv ")).changed() {
                    self.pool_mut(zone)[k].1 = l;
                    changed = true;
                }
                if ui.small_button("remove").clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            self.pool_mut(zone).remove(k);
            changed = true;
        }
        let filter = ui.add(egui::TextEdit::singleline(&mut self.creature_filter).hint_text("Add a creature: type a name").desired_width(f32::INFINITY));
        mark("uiCreatureFilter", &filter);
        let f = self.creature_filter.to_lowercase();
        if !f.is_empty() {
            let found: Vec<(String, String)> = content
                .creatures
                .iter()
                .filter(|c| c.id.contains(&f) || c.name.to_lowercase().contains(&f))
                .take(8)
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect();
            for (k, (id, name)) in found.into_iter().enumerate() {
                let b = ui.small_button(format!("+ {name}"));
                if k == 0 {
                    mark("uiAddFirst", &b);
                }
                if b.clicked() {
                    self.pool_mut(zone).push((id, 5));
                    changed = true;
                }
            }
        }
        if changed {
            self.dirty = true;
        }
        let n = self.pool_mut(zone).len();
        crate::platform::set_hook("pool", &n.to_string());
    }

    fn new_zone(&mut self) {
        let n = self.map.zones.len() + 1;
        self.map.zones.push(Zone { mark: self.map.free_zone_mark(), name: format!("Zone {n}"), ..Default::default() });
        self.zone_sel = Some(self.map.zones.len() - 1);
    }

    // ------------------------------------------------------------ the view

    #[allow(clippy::too_many_arguments)]
    pub(super) fn zones_view(&mut self, alt: bool, pressed: bool, down: bool, released: bool, over_view: bool) {
        let cell = self.ground_hit;
        if self.linking {
            self.hover_treeline = match cell {
                Some((x, z)) if self.link_drag.is_none() && self.map.area_edge(x, z) => self.map.treeline(x, z, 5),
                _ => vec![],
            };
            if pressed && over_view && !self.hover_treeline.is_empty() {
                self.link_drag = Some(self.hover_treeline.clone());
            }
            if released {
                if let (Some(line), Some((x, z))) = (self.link_drag.take(), cell) {
                    let zi = self.map.index(x, z).map(|i| self.map.zone[i] as usize).unwrap_or(0);
                    let area = self.map.index(line[0].0, line[0].1).map(|i| self.map.area[i] as usize).unwrap_or(0);
                    if zi > 0 && area > 0 {
                        self.checkpoint();
                        let link = SpawnLink {
                            area: self.map.tree_areas[area - 1].mark.clone(),
                            treeline: line.iter().map(|c| [c.0, c.1]).collect(),
                            zone: self.map.zones[zi - 1].mark.clone(),
                            ..Default::default()
                        };
                        self.map.links.push(link);
                        self.zone_sel = Some(zi - 1);
                        self.spawner.reset();
                        self.status = "Linked: creatures will come out of those trees".into();
                    } else {
                        self.status = "Drop the link on a zone".into();
                    }
                }
            }
            return;
        }
        if pressed && over_view {
            self.checkpoint();
            self.stroke = true;
            if self.zone_sel.is_none() && !(self.remove_cells != alt) {
                self.new_zone();
            }
        }
        if self.stroke && down {
            if let Some(c) = cell {
                let remove = self.remove_cells != alt;
                let mark = if remove { 0 } else { self.zone_sel.map(|z| z as u8 + 1).unwrap_or(0) };
                for c in self.line_to(c) {
                    for (x, z) in self.brush_cells(c) {
                        let i = self.map.index(x, z).unwrap();
                        if self.map.zone[i] != mark && (remove || mark > 0) {
                            self.map.zone[i] = mark;
                            self.zones_changed = true;
                        }
                    }
                }
            }
        }
        if released && self.zones_changed {
            self.zones_changed = false;
            self.spawner.reset();
        }
    }

    pub(super) fn cell_y(&self, x: i32, z: i32) -> f32 {
        self.map.level_at(x, z).unwrap_or(0) as f32 * self.level_height
    }

    fn treeline_centre(&self, line: &[(i32, i32)]) -> Vec3 {
        let n = line.len().max(1) as f32;
        let (sx, sz) = line.iter().fold((0.0, 0.0), |(a, b), c| (a + c.0 as f32 + 0.5, b + c.1 as f32 + 0.5));
        let (x, z) = (sx / n, sz / n);
        Vec3::new(x, self.cell_y(x.floor() as i32, z.floor() as i32) + 0.1, z)
    }

    fn zone_centre(&self, zone: usize) -> Option<Vec3> {
        let cells: Vec<(i32, i32)> = (0..self.map.height as i32)
            .flat_map(|z| (0..self.map.width as i32).map(move |x| (x, z)))
            .filter(|&(x, z)| self.map.index(x, z).map(|i| self.map.zone[i] as usize == zone + 1).unwrap_or(false))
            .collect();
        if cells.is_empty() {
            return None;
        }
        Some(self.treeline_centre(&cells))
    }

    pub(super) fn zones_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        let rect = |x: f32, y: f32, z: f32, color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: 1.0, depth: 1.0 }, center: Vec3::new(x, y, z), color, additive: false };
        let in_tool = self.tool == super::Tool::Zones;
        if in_tool || self.debug {
            for z in 0..self.map.height as i32 {
                for x in 0..self.map.width as i32 {
                    let i = self.map.index(x, z).unwrap();
                    let y = self.map.level[i] as f32 * lh + 0.06;
                    let zi = self.map.zone[i];
                    if zi > 0 {
                        let mut c = ZONE_COLORS[(zi as usize - 1) % ZONE_COLORS.len()];
                        if Some(zi as usize - 1) == self.zone_sel {
                            c[3] = 0.55;
                        }
                        shapes.push(rect(x as f32 + 0.5, y, z as f32 + 0.5, c));
                    } else if self.debug && self.map.area[i] > 0 {
                        shapes.push(rect(x as f32 + 0.5, y, z as f32 + 0.5, [0.1, 0.45, 0.15, 0.3]));
                    }
                }
            }
        }
        if in_tool || self.debug {
            for l in &self.map.links {
                let line: Vec<(i32, i32)> = l.treeline.iter().map(|c| (c[0], c[1])).collect();
                for &(x, z) in &line {
                    shapes.push(rect(x as f32 + 0.5, self.cell_y(x, z) + 0.07, z as f32 + 0.5, [1.0, 0.55, 0.1, 0.45]));
                }
                if let Some(zi) = self.map.zones.iter().position(|z| z.mark == l.zone) {
                    if let Some(to) = self.zone_centre(zi) {
                        arrow(self.treeline_centre(&line), to, 0.28, LINK, shapes);
                    }
                }
            }
        }
        if in_tool {
            for &(x, z) in &self.hover_treeline {
                shapes.push(rect(x as f32 + 0.5, self.cell_y(x, z) + 0.08, z as f32 + 0.5, [1.0, 1.0, 1.0, 0.45]));
            }
            if let (Some(line), Some(p)) = (&self.link_drag, self.ground_point()) {
                arrow(self.treeline_centre(line), p, 0.3, [1.0, 1.0, 1.0, 0.8], shapes);
            }
            if !self.linking {
                if let Some((x, z)) = self.ground_hit {
                    let s = (self.brush * 2 + 1) as f32;
                    shapes.push(Shape { kind: ShapeKind::Rect { width: s, depth: s }, center: Vec3::new(x as f32 + 0.5, self.cell_y(x, z) + 0.09, z as f32 + 0.5), color: [1.0, 1.0, 1.0, 0.3], additive: false });
                }
            }
        }
        if self.debug {
            // where each creature is going
            for r in &self.spawner.roamers {
                let mut from = Vec3::new(r.pos.x, self.cell_y(r.pos.x.floor() as i32, r.pos.y.floor() as i32) + 0.1, r.pos.y);
                for p in &r.path {
                    let to = Vec3::new(p.x, self.cell_y(p.x.floor() as i32, p.y.floor() as i32) + 0.1, p.y);
                    let d = Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero();
                    let side = Vec3::new(-d.z, 0.0, d.x) * 0.04;
                    shapes.push(quad([from + side, from - side, to - side, to + side], [1.0, 1.0, 1.0, 0.6]));
                    from = to;
                }
            }
        }
    }

    /// The ground point under the pointer (on its cell's level).
    fn ground_point(&self) -> Option<Vec3> {
        let (x, z) = self.ground_hit?;
        let y = self.cell_y(x, z);
        let (o, d) = self.last_ray?;
        Self::plane(o, d, y).map(|p| p + Vec3::Y * 0.1)
    }

    // ------------------------------------------------------------ the creatures

    /// The lights the roaming creatures carry, where they are and the way they face (as
    /// much of each as there is of its creature).
    pub(super) fn creature_lights(&self) -> Vec<world::lights::PlacedLight> {
        let mut out = vec![];
        for r in &self.spawner.roamers {
            let Some(l) = self.content.creatures.get(&r.creature).and_then(|c| c.light.as_ref()) else { continue };
            let feet = Vec3::new(r.pos.x, self.cell_y(r.pos.x.floor() as i32, r.pos.y.floor() as i32), r.pos.y);
            let mut placed = world::lights::carried(l, feet, Vec3::new(r.heading.x, 0.0, r.heading.y));
            placed.light.intensity *= r.presence.clamp(0.0, 1.0) as f64;
            out.push(placed);
        }
        out
    }

    /// The roaming creatures as pictures standing on the map (and their shadows).
    pub(super) fn creature_billboards(&mut self, tint: [f32; 4], billboards: &mut Vec<Billboard>, shapes: &mut Vec<Shape>) {
        if !self.creatures_on {
            return;
        }
        let (_, forward) = self.cam.ground_axes();
        let away = Vec2::new(forward.x, forward.z);
        let content = self.content.clone();
        for r in &self.spawner.roamers {
            let Some(e) = content.creatures.entry(&r.creature) else { continue };
            let c = &e.value;
            let back = r.moving && r.heading.dot(away) > 0.35;
            let (path, b) = if back { (&c.sprites.back, c.sprites.back_box) } else { (&c.sprites.front, c.sprites.front_box) };
            let y = self.cell_y(r.pos.x.floor() as i32, r.pos.y.floor() as i32);
            let pos = Vec3::new(r.pos.x, y, r.pos.y);
            let grow = 0.25 + 0.75 * r.presence.clamp(0.0, 1.0);
            let h = ((c.body.height_m as f32) * 1.1).clamp(0.45, 2.4) * grow;
            shapes.push(Shape { kind: ShapeKind::Disc { radius: 0.3 * h.max(0.6), squash: 0.6 }, center: pos + Vec3::Y * 0.03, color: [0.0, 0.0, 0.0, 0.28 * grow], additive: false });
            let Some(path) = path.as_ref().map(|p| e.space.asset(p)) else { continue };
            match self.sprites.get(&path) {
                Some(Some((w, ht))) => {
                    let (w, ht) = (*w as f32, *ht as f32);
                    let bw = ((b[2] - b[0]).max(1)) as f32;
                    let bh = ((b[3] - b[1]).max(1)) as f32;
                    billboards.push(Billboard {
                        texture: path,
                        pos,
                        size: Vec2::new(h * bw / bh, h),
                        offset: Vec2::ZERO,
                        uv: [b[0] as f32 / w, b[1] as f32 / ht, b[2] as f32 / w, b[3] as f32 / ht],
                        color: tint,
                        casts_shadow: false,
                    });
                }
                Some(None) => {}
                None => {
                    if !self.sprites_wanted.contains(&path) {
                        self.sprites_wanted.push(path);
                    }
                }
            }
        }
    }

    /// Loads the creatures' pictures the frames asked for.
    pub(super) fn load_sprites(&mut self, r: &mut SceneRenderer, rs: &eframe::egui_wgpu::RenderState) {
        let wanted = std::mem::take(&mut self.sprites_wanted);
        for path in wanted {
            if r.has_texture(&path) {
                let size = r.texture_size(&path);
                self.sprites.insert(path, size);
                continue;
            }
            self.files.request(&path);
            match self.files.get(&path) {
                Load::Pending => self.sprites_wanted.push(path),
                Load::Failed(_) => {
                    self.sprites.insert(path, None);
                }
                Load::Ready(bytes) => match image::load_from_memory(&bytes) {
                    Ok(img) => {
                        let img = img.to_rgba8();
                        let (w, h) = img.dimensions();
                        r.add_texture(&rs.device, &rs.queue, &path, &img, w, h, false);
                        self.sprites.insert(path, Some((w, h)));
                    }
                    Err(_) => {
                        self.sprites.insert(path, None);
                    }
                },
            }
        }
    }

    /// Saves the spawns record where its creatures allow: temporary/ while it names a
    /// placeholder, content/ when it only names the game's own creatures.
    pub(super) fn save_spawns(&mut self) -> Option<String> {
        self.spawns.id = self.map.id.clone();
        self.spawns.zones.retain(|mark, periods| {
            periods.retain(|_, pool| !pool.is_empty());
            !periods.is_empty() && self.map.zones.iter().any(|z| &z.mark == mark)
        });
        if self.spawns.zones.is_empty() {
            return None;
        }
        let placeholder = self.spawns.zones.values().flat_map(|p| p.values()).flatten().any(|(c, _)| self.content.creatures.entry(c).map(|e| e.space == Space::Temporary).unwrap_or(true));
        let space = if placeholder { Space::Temporary } else { Space::Content };
        #[cfg(not(target_arch = "wasm32"))]
        let saved = {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            content::disk::save(&root, space, &self.spawns).map(|p| p.display().to_string())
        };
        #[cfg(target_arch = "wasm32")]
        let saved = {
            let name = format!("{}.spawns.json", self.spawns.id);
            crate::platform::download(&name, &content::to_text(&self.spawns)).map(|_| format!("{name} (put it in {}/data/spawns/{}.json)", space.dir(), self.spawns.id))
        };
        let mut c = (*self.content).clone();
        c.spawns.put(self.spawns.clone(), space);
        self.content = std::sync::Arc::new(c);
        Some(match saved {
            Ok(p) => format!("creatures in {p}"),
            Err(e) => format!("creatures not saved: {e}"),
        })
    }

    /// For the debug panel: what's roaming.
    pub(super) fn roamer_summary(&self) -> String {
        let coming = self.spawner.roamers.iter().filter(|r| r.phase == Phase::Coming).count();
        let leaving = self.spawner.roamers.iter().filter(|r| r.phase == Phase::Leaving).count();
        format!("{} creatures · {coming} coming · {leaving} going back", self.spawner.roamers.len())
    }
}
