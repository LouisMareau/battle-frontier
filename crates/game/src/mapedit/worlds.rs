//! The World view: maps stitched into a world on one grid of cells. A panel across the
//! window shows the world's pieces as rectangles (the map being edited lit), the seams
//! between them coloured by whether the levels match (walking crosses only where they do):
//! pieces are dragged into place (snapping to cells), maps dragged in from a list, a piece
//! opened for editing with a double click. Walking off a map's edge goes on in the piece
//! there (`walk.rs`).

use std::collections::BTreeMap;
use std::sync::Arc;

use content::{Chunk, Levels, Piece, Space, World};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};

use super::{hint, mark, section, warn, MapEditor};
use crate::platform::Load;

#[derive(Default)]
pub(super) struct Worlds {
    pub open: bool,
    /// the world shown, and the one walking crosses into
    pub world: Option<String>,
    /// the world as edited here, saved or not
    draft: Option<World>,
    pub dirty: bool,
    /// a piece being dragged: its index, and the pointer's grab from its corner (cells)
    drag: Option<(usize, [f32; 2])>,
    /// screen points per cell, and the world cell at the canvas's top-left corner
    zoom: f32,
    pan: [f32; 2],
    fitted: bool,
    selected: Option<usize>,
    new_id: String,
    /// other maps' sizes and levels, for the seams
    pub levels: BTreeMap<String, Levels>,
}

fn overlaps(a: (&Piece, (u16, u16)), b: (&Piece, (u16, u16))) -> bool {
    let (ax1, az1) = (a.0.at[0] + a.1 .0 as i32, a.0.at[1] + a.1 .1 as i32);
    let (bx1, bz1) = (b.0.at[0] + b.1 .0 as i32, b.0.at[1] + b.1 .1 as i32);
    a.0.at[0] < bx1 && b.0.at[0] < ax1 && a.0.at[1] < bz1 && b.0.at[1] < az1
}

impl MapEditor {
    /// The world this map is part of: the one shown, else the first that holds the map.
    pub(super) fn world_record(&self) -> Option<World> {
        if let Some(d) = &self.worlds.draft {
            return Some(d.clone());
        }
        let id = self.worlds.world.clone().or_else(|| self.content.worlds.iter().find(|w| w.piece_of(&self.map.id).is_some()).map(|w| w.id.clone()))?;
        self.content.worlds.get(&id).cloned()
    }

    /// A map's size and levels for the seams: this map's as they are now, another's from
    /// its record (a chunked one once its files are here).
    fn levels_of(&mut self, id: &str) -> Option<Levels> {
        if id == self.map.id {
            return Some((self.map.width as u16, self.map.height as u16, self.map.level.clone()));
        }
        if let Some(l) = self.worlds.levels.get(id) {
            return Some(l.clone());
        }
        let e = self.content.maps.entry(id)?;
        let m = &e.value;
        let mut chunks = vec![];
        if m.chunked() {
            let (gx, gz) = m.chunk_grid();
            for z in 0..gz {
                for x in 0..gx {
                    let path = format!("{}/{}", e.space.dir(), Chunk::path(&m.id, x, z));
                    match self.files.get(&path) {
                        Load::Pending => return None,
                        Load::Ready(bytes) => {
                            if let Ok(c) = serde_json::from_slice::<Chunk>(&bytes) {
                                chunks.push(c);
                            }
                        }
                        Load::Failed(_) => {}
                    }
                }
            }
        }
        let l = (m.width, m.height, m.cells(&chunks).level);
        self.worlds.levels.insert(id.to_string(), l.clone());
        Some(l)
    }

    pub(super) fn worlds_panel(&mut self, ui: &mut egui::Ui) {
        // the world shown: the one picked, else the first holding this map, else the first
        if self.worlds.world.is_none() {
            self.worlds.world = self.content.worlds.iter().find(|w| w.piece_of(&self.map.id).is_some()).or_else(|| self.content.worlds.iter().next()).map(|w| w.id.clone());
        }
        let stale = match (&self.worlds.draft, &self.worlds.world) {
            (Some(d), Some(w)) => &d.id != w,
            (None, Some(_)) => true,
            _ => false,
        };
        if stale {
            self.worlds.draft = self.worlds.world.as_ref().and_then(|id| self.content.worlds.get(id).cloned());
            self.worlds.dirty = false;
            self.worlds.fitted = false;
            self.worlds.selected = None;
        }
        let content = self.content.clone();
        let size = |id: &str| content.maps.get(id).map(|m| (m.width, m.height));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("World").strong());
            let ids: Vec<(String, String)> = content.worlds.iter().map(|w| (w.id.clone(), w.name.clone())).collect();
            let shown = self.worlds.draft.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "none yet".into());
            egui::ComboBox::from_id_salt("world-pick").selected_text(shown).show_ui(ui, |ui| {
                for (id, name) in &ids {
                    let r = ui.selectable_label(self.worlds.world.as_deref() == Some(id), format!("{name} ({id})"));
                    mark(&format!("uiWorldPick_{id}"), &r);
                    if r.clicked() {
                        self.worlds.world = Some(id.clone());
                    }
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.worlds.new_id).hint_text("a new world's id").desired_width(120.0));
            let new = ui.button("New world").on_hover_text("A world with the map you're editing as its first piece");
            mark("uiNewWorld", &new);
            if new.clicked() {
                let id = self.worlds.new_id.trim().to_string();
                if super::slug_ok(&id) && !content.worlds.contains(&id) {
                    self.worlds.draft = Some(World { id: id.clone(), name: id.replace(['_', '-'], " "), pieces: vec![Piece { map: self.map.id.clone(), at: [0, 0] }] });
                    self.worlds.world = Some(id);
                    self.worlds.dirty = true;
                    self.worlds.fitted = false;
                    self.worlds.selected = Some(0);
                    self.worlds.new_id.clear();
                } else {
                    self.status = "A world's id: lowercase letters, digits, _ or -, and not taken".into();
                }
            }
            let save = ui.add_enabled(self.worlds.dirty, egui::Button::new(if self.worlds.dirty { "Save world •" } else { "Save world" }));
            mark("uiSaveWorld", &save);
            if save.clicked() {
                self.save_world();
            }
            let fit = ui.button("Fit").on_hover_text("Every piece in view");
            mark("uiFit", &fit);
            if fit.clicked() {
                self.worlds.fitted = false;
            }
            // the selected piece
            let picked = self.worlds.selected.and_then(|i| self.worlds.draft.as_ref().and_then(|d| d.pieces.get(i)).map(|p| (i, p.map.clone())));
            if let Some((i, map)) = picked {
                ui.separator();
                let name = content.maps.get(&map).map(|m| m.name.clone()).unwrap_or_else(|| map.clone());
                ui.label(RichText::new(&name).strong());
                let open = ui.add_enabled(map != self.map.id, egui::Button::new("Open")).on_hover_text("Edit this map (double-click the piece does too)");
                mark("uiOpenPiece", &open);
                if open.clicked() {
                    self.open(&map);
                }
                let remove = ui.button("Remove from the world");
                mark("uiRemovePiece", &remove);
                if remove.clicked() {
                    if let Some(d) = &mut self.worlds.draft {
                        d.pieces.remove(i);
                        self.worlds.dirty = true;
                        self.worlds.selected = None;
                    }
                }
            }
        });
        hint(ui, "Drag a map in from the list, drag pieces into place. Green seams: the levels match, walking crosses; crimson: they don't. Double-click a piece to edit it.");
        ui.separator();
        let Some(world) = self.worlds.draft.clone() else {
            hint(ui, "No world yet: give one an id and make it, with the map you're editing as its first piece.");
            crate::platform::set_hook("world", "");
            return;
        };
        for p in world.problems(&size) {
            warn(ui, &p);
        }
        // the other maps' levels, for the seams
        let levels: BTreeMap<String, Levels> = world.pieces.iter().filter_map(|p| self.levels_of(&p.map).map(|l| (p.map.clone(), l))).collect();
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(170.0);
                section(ui, "Maps");
                let review = self.review.clone();
                egui::ScrollArea::vertical().id_salt("world-maps").max_height((height - 40.0).max(40.0)).show(ui, |ui| {
                    for m in content.maps.iter() {
                        let placed = world.piece_of(&m.id).is_some();
                        let text = RichText::new(format!("{}{}", m.name, if placed { " ·" } else { "" })).color(if placed { Color32::from_gray(140) } else { Color32::from_gray(220) });
                        ui.horizontal(|ui| {
                            let r = ui.selectable_label(false, text).interact(Sense::drag()).on_hover_text(if placed { "In the world already" } else { "Drag it onto the grid" });
                            r.dnd_set_drag_payload(format!("map:{}", m.id));
                            mark(&format!("uiMapRow_{}", m.id), &r);
                            super::review::record_mark(ui, review.as_deref(), "maps", &m.id, "");
                        });
                    }
                });
            });
            ui.separator();
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click_and_drag());
            self.world_canvas(ui, rect, resp, world, &levels);
        });
    }

    /// The grid with the pieces on it.
    fn world_canvas(&mut self, ui: &mut egui::Ui, rect: Rect, resp: egui::Response, mut world: World, levels: &BTreeMap<String, Levels>) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 14, 22));
        let content = self.content.clone();
        let size = |id: &str| content.maps.get(id).map(|m| (m.width, m.height));
        if !self.worlds.fitted {
            let (mut lo, mut hi) = ([i32::MAX; 2], [i32::MIN; 2]);
            for p in &world.pieces {
                if let Some((w, h)) = size(&p.map) {
                    lo = [lo[0].min(p.at[0]), lo[1].min(p.at[1])];
                    hi = [hi[0].max(p.at[0] + w as i32), hi[1].max(p.at[1] + h as i32)];
                }
            }
            if lo[0] <= hi[0] {
                let (w, h) = ((hi[0] - lo[0]) as f32 + 16.0, (hi[1] - lo[1]) as f32 + 16.0);
                self.worlds.zoom = (rect.width() / w).min(rect.height() / h).clamp(0.25, 24.0);
                self.worlds.pan = [lo[0] as f32 - 8.0, lo[1] as f32 - 8.0];
            } else {
                self.worlds.zoom = 4.0;
                self.worlds.pan = [-8.0, -8.0];
            }
            self.worlds.fitted = true;
        }
        // the wheel zooms around the pointer; the right or middle button pans
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(p) = resp.hover_pos() {
                    let (zoom, pan) = (self.worlds.zoom, self.worlds.pan);
                    let before = [(p.x - rect.left()) / zoom + pan[0], (p.y - rect.top()) / zoom + pan[1]];
                    let z = (zoom * (1.0 + scroll * 0.002)).clamp(0.25, 24.0);
                    self.worlds.zoom = z;
                    self.worlds.pan = [before[0] - (p.x - rect.left()) / z, before[1] - (p.y - rect.top()) / z];
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            let d = resp.drag_delta();
            self.worlds.pan = [self.worlds.pan[0] - d.x / self.worlds.zoom, self.worlds.pan[1] - d.y / self.worlds.zoom];
        }
        let (zoom, pan) = (self.worlds.zoom, self.worlds.pan);
        let to_screen = |x: f32, z: f32| Pos2::new(rect.left() + (x - pan[0]) * zoom, rect.top() + (z - pan[1]) * zoom);
        let to_world = |p: Pos2| [(p.x - rect.left()) / zoom + pan[0], (p.y - rect.top()) / zoom + pan[1]];
        // the grid, every 16 cells (every cell when close)
        let step = if zoom >= 8.0 { 1.0 } else { 16.0 };
        let grid = Stroke::new(1.0, Color32::from_gray(if step == 1.0 { 30 } else { 45 }));
        let mut x = (pan[0] / step).floor() * step;
        while to_screen(x, 0.0).x <= rect.right() {
            let sx = to_screen(x, 0.0).x;
            painter.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], grid);
            x += step;
        }
        let mut z = (pan[1] / step).floor() * step;
        while to_screen(0.0, z).y <= rect.bottom() {
            let sz = to_screen(0.0, z).y;
            painter.line_segment([Pos2::new(rect.left(), sz), Pos2::new(rect.right(), sz)], grid);
            z += step;
        }
        // the pieces
        let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
        let sized: Vec<Option<(u16, u16)>> = world.pieces.iter().map(|p| size(&p.map)).collect();
        let mut rects: Vec<Option<Rect>> = vec![None; world.pieces.len()];
        // where new games start: one for the whole world, marked on its map
        let start = content.world().start;
        let mut start_at = None;
        for (i, p) in world.pieces.iter().enumerate() {
            let Some((w, h)) = sized[i] else { continue };
            let r = Rect::from_min_max(to_screen(p.at[0] as f32, p.at[1] as f32), to_screen((p.at[0] + w as i32) as f32, (p.at[1] + h as i32) as f32));
            rects[i] = Some(r);
            let current = p.map == self.map.id;
            let overlapping = world.pieces.iter().enumerate().any(|(j, o)| j != i && sized[j].map(|s| overlaps((p, (w, h)), (o, s))).unwrap_or(false));
            painter.rect_filled(r, 2.0, if current { Color32::from_rgb(38, 78, 118) } else { Color32::from_rgb(40, 50, 66) });
            let selected = self.worlds.selected == Some(i);
            let edge = if overlapping { Color32::from_rgb(230, 80, 70) } else if selected { Color32::from_rgb(255, 200, 90) } else { Color32::from_gray(120) };
            painter.rect_stroke(r, 2.0, Stroke::new(if selected { 2.0 } else { 1.0 }, edge), StrokeKind::Inside);
            let name = content.maps.get(&p.map).map(|m| m.name.clone()).unwrap_or_else(|| p.map.clone());
            if r.width() > 40.0 {
                painter.text(r.left_top() + egui::vec2(4.0, 2.0), Align2::LEFT_TOP, format!("{name} · {w}×{h}"), FontId::proportional(11.5), Color32::from_gray(225));
            }
            let c = r.center();
            crate::platform::set_hook(&format!("uiPiece_{}", p.map), &format!("{:.0},{:.0}", c.x, c.y));
            if p.map == start.map {
                start_at = Some(to_screen((p.at[0] + start.at[0]) as f32 + 0.5, (p.at[1] + start.at[1]) as f32 + 0.5));
            }
        }
        if let Some(at) = start_at {
            painter.circle(at, 4.5, Color32::from_rgb(255, 204, 72), Stroke::new(1.5, Color32::from_rgb(40, 30, 10)));
            painter.text(at + egui::vec2(7.0, 0.0), Align2::LEFT_CENTER, "Start", FontId::proportional(11.0), Color32::from_rgb(255, 214, 110));
        }
        crate::platform::set_hook("worldstart", &start_at.map(|at| format!("{:.0},{:.0}", at.x, at.y)).unwrap_or_default());
        // the piece under a point: the last drawn is on top
        let piece_at = |q: Pos2| rects.iter().rposition(|r| r.map(|r| r.contains(q)).unwrap_or(false));
        let hit = pointer.and_then(piece_at);
        // the seams: green where the levels match, crimson where they don't
        let seams = world.seams(&|id: &str| levels.get(id).cloned());
        let mut mismatches = 0;
        for s in &seams {
            mismatches += s.mismatches.len();
            let east = s.side == "e";
            let (a, b) = if east { (to_screen(s.from[0] as f32 + 1.0, s.from[1] as f32), to_screen(s.from[0] as f32 + 1.0, s.from[1] as f32 + s.cells as f32)) } else { (to_screen(s.from[0] as f32, s.from[1] as f32 + 1.0), to_screen(s.from[0] as f32 + s.cells as f32, s.from[1] as f32 + 1.0)) };
            painter.line_segment([a, b], Stroke::new(3.0, Color32::from_rgb(90, 210, 110)));
            for m in &s.mismatches {
                let (a, b) = if east { (to_screen(m[0] as f32 + 1.0, m[1] as f32), to_screen(m[0] as f32 + 1.0, m[1] as f32 + 1.0)) } else { (to_screen(m[0] as f32, m[1] as f32 + 1.0), to_screen(m[0] as f32 + 1.0, m[1] as f32 + 1.0)) };
                painter.line_segment([a, b], Stroke::new(3.0, Color32::from_rgb(235, 80, 70)));
            }
        }
        // pieces picked and dragged (grabbed where the button went down, not where the
        // drag was first noticed); a map dropped in; a piece opened
        let mut changed = false;
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
            if let Some((i, q)) = origin.and_then(|q| piece_at(q).map(|i| (i, q))) {
                let p = to_world(q);
                let at = world.pieces[i].at;
                self.worlds.drag = Some((i, [p[0] - at[0] as f32, p[1] - at[1] as f32]));
                self.worlds.selected = Some(i);
            }
        }
        if resp.clicked() {
            self.worlds.selected = hit;
        }
        // the last move counts too, on the frame the button is let go
        let dragging = resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped_by(egui::PointerButton::Primary);
        if let (Some((i, grab)), true, Some(q)) = (self.worlds.drag, dragging, pointer) {
            let p = to_world(q);
            let at = [(p[0] - grab[0]).round() as i32, (p[1] - grab[1]).round() as i32];
            if i < world.pieces.len() && world.pieces[i].at != at {
                world.pieces[i].at = at;
                changed = true;
            }
        }
        if resp.drag_stopped() {
            self.worlds.drag = None;
        }
        let mut open = None;
        if resp.double_clicked() {
            if let Some(i) = hit {
                open = Some(world.pieces[i].map.clone());
            }
        }
        if let (Some(payload), Some(q)) = (resp.dnd_release_payload::<String>(), pointer) {
            if let Some(id) = payload.strip_prefix("map:") {
                if world.piece_of(id).is_none() && size(id).is_some() {
                    let p = to_world(q);
                    world.pieces.push(Piece { map: id.to_string(), at: [p[0].round() as i32, p[1].round() as i32] });
                    self.worlds.selected = Some(world.pieces.len() - 1);
                    changed = true;
                }
            }
        }
        crate::platform::set_hook("world", &format!("{};pieces:{};seams:{};mismatches:{mismatches}", world.id, world.pieces.len(), seams.len()));
        crate::platform::set_hook("pieces", &world.pieces.iter().map(|p| format!("{}@{},{}", p.map, p.at[0], p.at[1])).collect::<Vec<_>>().join(";"));
        crate::platform::set_hook("worldzoom", &format!("{zoom:.3}"));
        if changed {
            self.worlds.draft = Some(world);
            self.worlds.dirty = true;
        }
        if let Some(id) = open.filter(|id| *id != self.map.id) {
            self.open(&id);
        }
    }

    /// Writes the world: into content/ on the desktop, kept in the browser (or downloaded).
    pub(super) fn save_world(&mut self) {
        let Some(w) = self.worlds.draft.clone() else { return };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            match content::disk::save(&root, Space::Content, &w) {
                Ok(path) => self.status = format!("Saved {}", path.display()),
                Err(e) => {
                    self.status = format!("Not saved: {e}");
                    return;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let name = format!("{}.json", w.id);
            match super::saving::browser_save_record(Space::Content, &w, &name) {
                Ok(true) => self.status = format!("Saved the world {} in this browser", w.id),
                Ok(false) => self.status = format!("Downloaded {name}: put it in content/data/worlds/"),
                Err(e) => {
                    self.status = format!("Not saved: {e}");
                    return;
                }
            }
        }
        let mut c = (*self.content).clone();
        c.worlds.put(w, Space::Content);
        c.sort();
        self.content = Arc::new(c);
        self.worlds.dirty = false;
    }

    /// Walking off this map at `next` (a cell past its edge): the piece next to it in the
    /// world, and the cell there.
    pub(super) fn across(&self, next: (i32, i32)) -> Option<(String, (i32, i32))> {
        let world = self.world_record()?;
        let here = world.piece_of(&self.map.id)?;
        let cell = [here.at[0] + next.0, here.at[1] + next.1];
        let content = self.content.clone();
        let size = |id: &str| content.maps.get(id).map(|m| (m.width, m.height));
        let (i, there) = world.at(&size, cell)?;
        let id = world.pieces[i].map.clone();
        (id != self.map.id).then_some((id, (there[0], there[1])))
    }
}
