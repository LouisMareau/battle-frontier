//! The editor's front door: its Menu (back to the game's main menu, or to the editor's other
//! parts: the content browser and the test battles), the welcome the main menu's Editor
//! shows first (what the editor is, and how what's made here gets into the game: a pull
//! request), and where a new game starts (the world settings' `start`: one for the whole
//! world, picked on the map: a cell and the way the player faces there).

use std::sync::Arc;

use content::Space;
use egui::{Color32, RichText};
use glam::Vec3;
use render::scene::{Shape, ShapeKind};

use super::actors::facing_of;
use super::{hint, mark, section, MapEditor};
use crate::hud::{ACCENT, MUTED, TEXT};

/// The start's mark on the map: where new games start, and the way the player faces.
const START_COLOR: [f32; 4] = [1.0, 0.8, 0.28, 0.9];

/// Picking where new games start on the map (the map panel's "Pick it on the map"): on while
/// it's picked; the cell pressed on (dragging from it faces the player that way); the way they
/// face when it's only clicked.
#[derive(Default)]
pub(super) struct StartPick {
    pub on: bool,
    pub pressed: Option<(i32, i32)>,
    pub facing: f64,
}

/// A facing's name (degrees, as the walker turns: 0 south, 90 west, 180 north, 270 east).
pub(super) fn facing_name(facing: f64) -> &'static str {
    match (facing.rem_euclid(360.0) / 90.0).round() as i32 % 4 {
        1 => "west",
        2 => "north",
        3 => "east",
        _ => "south",
    }
}

/// The way from one cell to another as a facing, along the longer of the two (x on a tie):
/// None when it's the same cell.
pub(super) fn facing_toward(from: (i32, i32), to: (i32, i32)) -> Option<f64> {
    let (dx, dz) = (to.0 - from.0, to.1 - from.1);
    if (dx, dz) == (0, 0) {
        return None;
    }
    let step = if dx.abs() >= dz.abs() { (dx.signum(), 0) } else { (0, dz.signum()) };
    Some(facing_of(step) as f64)
}

/// The name the welcome is turned off under ("Don't show this again"; `platform::store_text`).
pub const WELCOME_NAME: &str = "editor-welcome";

/// How to add to the game, for the welcome's link.
const CONTRIBUTING: &str = "https://github.com/LouisMareau/battle-frontier/blob/main/CONTRIBUTING.md";

/// Where the editor goes when it's left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// the game's main menu
    Menu,
    /// the content browser: every record, both spaces
    Content,
    /// the test battles: any encounter, to try it
    Battles,
}

impl MapEditor {
    /// The top bar's Menu: the main menu, the editor's other parts, the welcome again.
    pub(super) fn leave_menu(&mut self, ui: &mut egui::Ui) -> Option<Leave> {
        let mut to = None;
        let menu = ui.menu_button("Menu", |ui| {
            let item = |ui: &mut egui::Ui, name: &str, mark_as: &str, about: &str| {
                let r = ui.button(name).on_hover_text(about);
                mark(mark_as, &r);
                r.clicked()
            };
            if item(ui, "Main menu", "uiMainMenu", "Back to the game's main menu") {
                to = Some(Leave::Menu);
            }
            ui.separator();
            if item(ui, "Content", "uiContentBrowser", "Every record of the game, both spaces, to read") {
                to = Some(Leave::Content);
            }
            if item(ui, "Test battles", "uiTestBattles", "Any encounter, to try it") {
                to = Some(Leave::Battles);
            }
            ui.separator();
            if item(ui, "About the editor", "uiAboutEditor", "What the editor is, and how what you make gets into the game") {
                self.welcome = true;
            }
        });
        mark("uiEditorMenu", &menu.response);
        menu.response.on_hover_text("The main menu, the content, the test battles");
        to
    }

    /// The welcome, while it's up: what the editor is, how to add to the game, "Don't show
    /// this again".
    pub(super) fn welcome_modal(&mut self, ctx: &egui::Context) {
        crate::platform::set_hook("welcome", if self.welcome { "open" } else { "" });
        if !self.welcome {
            return;
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("editor-welcome")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Welcome to the editor").size(22.0).color(TEXT));
            ui.add_space(8.0);
            let line = |ui: &mut egui::Ui, text: &str| {
                ui.label(RichText::new(text).size(14.5).color(TEXT));
                ui.add_space(6.0);
            };
            line(ui, "This is what Battle Frontier is made with: its maps, props, creatures, abilities and battles.");
            line(ui, "Anyone can add to the game. Make something here, then send it as a pull request on GitHub: once it's reviewed and merged, everyone plays it.");
            let keep = if cfg!(target_arch = "wasm32") {
                "In the browser, saving downloads each file, and Publish, in the top bar, sends what you've saved as a pull request once you've signed in with GitHub."
            } else {
                "What you save goes into the project's folders, and stays on your computer until you publish it: Publish, in the top bar, sends it as a pull request once you've signed in with GitHub."
            };
            ui.label(RichText::new(keep).size(13.0).color(MUTED));
            ui.add_space(4.0);
            ui.hyperlink_to(RichText::new("How to contribute").size(13.5), CONTRIBUTING);
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let hide = ui.checkbox(&mut self.welcome_hide, "Don't show this again");
                mark("uiWelcomeHide", &hide);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let go = ui.add(egui::Button::new(RichText::new("Start editing").strong().color(Color32::from_rgb(34, 34, 34))).fill(ACCENT));
                    mark("uiWelcomeOk", &go);
                    if go.clicked() {
                        close = true;
                    }
                });
            });
        });
        if close || modal.should_close() {
            self.welcome = false;
            if self.welcome_hide {
                if let Err(e) = crate::platform::store_text(WELCOME_NAME, "hidden") {
                    self.status = format!("The welcome will show again: {e}");
                }
            }
        }
    }

    /// The map panel's New game: where a new game starts (one start for the whole world),
    /// picked on the map (a cell, dragged from the way the player faces), or here (where the
    /// walker stands, else in the middle of the view); the way they face turned; shown.
    pub(super) fn start_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "New game");
        let start = self.content.world().start;
        let here = start.map == self.map.id;
        let said = if start.map.is_empty() {
            "New games start on the first map, wherever its walk begins.".to_string()
        } else if here {
            format!("New games start here, on {}, {}, facing {}.", start.at[0], start.at[1], facing_name(start.facing))
        } else {
            let name = self.content.maps.get(&start.map).map(|m| m.name.clone()).unwrap_or_else(|| start.map.clone());
            format!("New games start on {name}, on {}, {}.", start.at[0], start.at[1])
        };
        hint(ui, &said);
        ui.horizontal_wrapped(|ui| {
            let picking = self.start_pick.on;
            let pick = ui.selectable_label(picking, "Pick it on the map").on_hover_text(
                "Then click the cell new games start on, or drag from it the way the player faces. There's one start for the whole world: picked on any map, it moves there. Esc stops",
            );
            mark("uiStartPick", &pick);
            if pick.clicked() {
                self.start_pick = StartPick { on: !picking, pressed: None, facing: if start.map.is_empty() { 0.0 } else { start.facing } };
                if !picking {
                    self.status = "Click the cell new games start on (or drag from it the way the player faces); Esc stops".into();
                }
            }
            let walking = self.walker.is_some();
            let b = ui.button("Start new games here").on_hover_text(if walking {
                "Where the walker stands, facing its way"
            } else {
                "The cell in the middle of the view (Walk puts you on the exact cell first)"
            });
            mark("uiStartHere", &b);
            if b.clicked() {
                let (at, facing) = match &self.walker {
                    Some(w) => ([w.cell.0, w.cell.1], w.facing as f64),
                    None => {
                        let (x, z) = (self.cam.target.x.floor() as i32, self.cam.target.z.floor() as i32);
                        ([x.clamp(0, self.map.width as i32 - 1), z.clamp(0, self.map.height as i32 - 1)], 0.0)
                    }
                };
                self.save_start(content::Start { map: self.map.id.clone(), at, facing });
            }
        });
        if !start.map.is_empty() {
            ui.horizontal_wrapped(|ui| {
                if here {
                    let turn = ui.button(format!("Facing {} ⟳", facing_name(start.facing))).on_hover_text("The way the player faces as a new game starts: a quarter more clockwise each click");
                    mark("uiStartFacing", &turn);
                    if turn.clicked() {
                        self.save_start(content::Start { facing: (start.facing + 90.0).rem_euclid(360.0), ..start.clone() });
                    }
                }
                let go = ui.button(if here { "Show it" } else { "Go to it" }).on_hover_text(if here { "The view on the start" } else { "Opens the map new games start on, the view on the start" });
                mark("uiStartGo", &go);
                if go.clicked() {
                    self.go_to_start(&start);
                }
            });
        }
        crate::platform::set_hook("start", &if start.map.is_empty() { String::new() } else { format!("{}:{},{}", start.map, start.at[0], start.at[1]) });
        crate::platform::set_hook("startfacing", &if start.map.is_empty() { String::new() } else { facing_name(start.facing).to_string() });
    }

    /// The view on where new games start: on this map, or the one it's on opened.
    fn go_to_start(&mut self, start: &content::Start) {
        let cell = (start.at[0], start.at[1]);
        if start.map != self.map.id {
            if self.dirty {
                self.status = format!("Save {} first, then go to the start (on {})", self.map.id, start.map);
                return;
            }
            self.open(&start.map);
        }
        // (a map in chunk files opens a moment later, the view where it always starts)
        if start.map == self.map.id {
            self.cam.target = self.cell_pos(cell);
            self.status = format!("New games start here: {}, {}, facing {}", cell.0, cell.1, facing_name(start.facing));
        }
    }

    /// Picking where new games start, in the view: pressed on a cell, it's there once let go
    /// (facing the way it was dragged, if it was), and kept; nothing else is done with the
    /// pointer.
    pub(super) fn start_input(&mut self, pressed: bool, released: bool) {
        if let Some(start) = self.start_picked(pressed, released) {
            self.save_start(start);
        }
    }

    /// What picking makes of the pointer: the start, once it's let go (picking over).
    fn start_picked(&mut self, pressed: bool, released: bool) -> Option<content::Start> {
        if pressed {
            self.start_pick.pressed = self.ground_hit;
        }
        if !released {
            return None;
        }
        let from = self.start_pick.pressed.take()?;
        let facing = self.ground_hit.and_then(|to| facing_toward(from, to)).unwrap_or(self.start_pick.facing);
        self.start_pick.on = false;
        Some(content::Start { map: self.map.id.clone(), at: [from.0, from.1], facing })
    }

    /// Picking or not, where new games start, on this map: a ring on its cell and an arrow
    /// the way the player faces (and fainter over everything, to be found behind trees);
    /// while picking, where it would go (faded), and the way it's dragged.
    pub(super) fn start_shapes(&self, shapes: &mut Vec<Shape>, overlays: &mut Vec<Shape>) {
        let start = self.content.world().start;
        let mut marks = vec![];
        if start.map == self.map.id && self.map.index(start.at[0], start.at[1]).is_some() {
            marks.push(((start.at[0], start.at[1]), start.facing, START_COLOR));
        }
        if self.start_pick.on {
            let faded = [START_COLOR[0], START_COLOR[1], START_COLOR[2], 0.5];
            match self.start_pick.pressed {
                Some(from) => marks.push((from, self.ground_hit.and_then(|to| facing_toward(from, to)).unwrap_or(self.start_pick.facing), faded)),
                None => marks.extend(self.ground_hit.map(|c| (c, self.start_pick.facing, faded))),
            }
        }
        let mut drawn = vec![];
        for (cell, facing, color) in marks {
            let c = self.cell_pos(cell) + Vec3::Y * 0.05;
            let rad = (facing as f32).to_radians();
            let dir = Vec3::new(-rad.sin(), 0.0, rad.cos());
            let through = [color[0], color[1], color[2], color[3] * 0.4];
            for (out, color) in [(&mut *shapes, color), (&mut *overlays, through)] {
                out.push(Shape { kind: ShapeKind::Ring { inner: 0.34, outer: 0.46 }, center: c, color, additive: false });
                super::zones::arrow(c - dir * 0.22, c + dir * 0.4, 0.14, color, out);
            }
            drawn.push(format!("{},{}:{}", cell.0, cell.1, facing_name(facing)));
        }
        crate::platform::set_hook("startmark", &drawn.join(";"));
        crate::platform::set_hook("startpick", if self.start_pick.on { "1" } else { "" });
    }

    /// The world settings written with a new start (as the Post-processing window writes them).
    fn save_start(&mut self, start: content::Start) {
        let space = self.content.settings.entry("world").map(|e| e.space).unwrap_or(Space::Content);
        let mut record = self.content.world();
        record.start = start.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let saved = {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            content::disk::save(&root, space, &record).map(|path| format!("New games start here ({}, {}): {}", start.at[0], start.at[1], path.display()))
        };
        #[cfg(target_arch = "wasm32")]
        let saved = crate::platform::download("world.json", &content::to_text(&record))
            .map(|()| format!("New games start here ({}, {}): world.json downloaded, for {}/data/settings/", start.at[0], start.at[1], space.dir()));
        match saved {
            Ok(said) => {
                let mut c = (*self.content).clone();
                c.settings.put(record, space);
                self.content = Arc::new(c);
                self.status = said;
            }
            Err(e) => self.status = format!("Where new games start isn't saved: {e}"),
        }
    }

    /// The map open (its id).
    pub fn map_id(&self) -> &str {
        &self.map.id
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn editor(start: Option<content::Start>) -> MapEditor {
        let mut c = content::Content::default();
        if let Some(start) = start {
            c.settings.put(content::Settings { start, ..Default::default() }, Space::Content);
        }
        let root = std::env::temp_dir().join("battle-frontier-start");
        let mut ed = MapEditor::new(Arc::new(c), None, crate::platform::Files::new(root.to_str().unwrap()));
        ed.map = world::WorldMap::new("meadow", "Meadow", 16, 12, "grass");
        ed
    }

    #[test]
    fn a_facing_by_its_name_and_the_way_a_drag_goes() {
        assert_eq!([0.0, 90.0, 180.0, 270.0, 360.0, -90.0].map(facing_name), ["south", "west", "north", "east", "south", "east"]);
        assert_eq!(facing_toward((5, 5), (5, 5)), None, "the same cell: no way");
        assert_eq!(facing_toward((5, 5), (5, 9)), Some(0.0), "south (+z)");
        assert_eq!(facing_toward((5, 5), (2, 6)), Some(90.0), "west, the longer way");
        assert_eq!(facing_toward((5, 5), (6, 1)), Some(180.0), "north");
        assert_eq!(facing_toward((5, 5), (7, 7)), Some(270.0), "east on a tie");
    }

    #[test]
    fn picked_on_the_map_a_click_keeps_the_facing_and_a_drag_turns_it() {
        let mut ed = editor(None);
        // a click: pressed and let go on the same cell, facing as it was
        ed.start_pick = StartPick { on: true, pressed: None, facing: 90.0 };
        ed.ground_hit = Some((4, 6));
        assert_eq!(ed.start_picked(true, false), None, "nothing until it's let go");
        assert_eq!(ed.start_picked(false, true), Some(content::Start { map: "meadow".into(), at: [4, 6], facing: 90.0 }));
        assert!(!ed.start_pick.on, "picked: over");
        // a drag north from a cell: there, facing north
        ed.start_pick = StartPick { on: true, pressed: None, facing: 0.0 };
        ed.ground_hit = Some((9, 8));
        ed.start_picked(true, false);
        ed.ground_hit = Some((9, 3));
        assert_eq!(ed.start_picked(false, true), Some(content::Start { map: "meadow".into(), at: [9, 8], facing: 180.0 }));
        // let go with nothing pressed (pressed off the map): nothing
        ed.start_pick = StartPick { on: true, pressed: None, facing: 0.0 };
        ed.ground_hit = None;
        ed.start_picked(true, false);
        ed.ground_hit = Some((2, 2));
        assert_eq!(ed.start_picked(false, true), None);
    }

    #[test]
    fn the_start_is_marked_on_its_map_and_where_it_would_go_while_picking() {
        let rings = |ed: &MapEditor| {
            let (mut shapes, mut overlays) = (vec![], vec![]);
            ed.start_shapes(&mut shapes, &mut overlays);
            shapes.iter().filter_map(|s| matches!(s.kind, ShapeKind::Ring { .. }).then_some((s.center.x, s.center.z, s.color[3]))).collect::<Vec<_>>()
        };
        // on this map: a ring on its cell (and its arrow), fainter over everything too
        let mut ed = editor(Some(content::Start { map: "meadow".into(), at: [3, 5], facing: 270.0 }));
        assert_eq!(rings(&ed), [(3.5, 5.5, START_COLOR[3])]);
        let (mut shapes, mut overlays) = (vec![], vec![]);
        ed.start_shapes(&mut shapes, &mut overlays);
        assert!(shapes.iter().any(|s| matches!(s.kind, ShapeKind::Quad { .. })), "the arrow");
        assert_eq!(overlays.len(), shapes.len(), "the same, seen through");
        assert!(overlays.iter().all(|o| o.color[3] < START_COLOR[3]), "fainter");
        // picking: where it would go too, under the pointer, faded
        ed.start_pick = StartPick { on: true, pressed: None, facing: 0.0 };
        ed.ground_hit = Some((7, 2));
        assert_eq!(rings(&ed), [(3.5, 5.5, START_COLOR[3]), (7.5, 2.5, 0.5)]);
        // on another map: none here
        let ed = editor(Some(content::Start { map: "elsewhere".into(), at: [3, 5], facing: 0.0 }));
        assert!(rings(&ed).is_empty());
    }
}
