//! The editor's front door: its Menu (back to the game's main menu, or to the editor's other
//! parts: the content browser and the test battles), the welcome the main menu's Editor
//! shows first (what the editor is, and how what's made here gets into the game: a pull
//! request), and where a new game starts (the world settings' `start`).

use std::sync::Arc;

use content::Space;
use egui::{Color32, RichText};

use super::{hint, mark, section, MapEditor};
use crate::hud::{ACCENT, MUTED, TEXT};

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
                "In the browser, saving downloads each file: put it in the project before you send it."
            } else {
                "What you save goes into the project's folders, and stays on your computer until you send it."
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

    /// The map panel's New game: where a new game starts, and a button to start new games
    /// here (where the walker stands, else in the middle of the view).
    pub(super) fn start_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "New game");
        let start = self.content.world().start;
        let here = start.map == self.map.id;
        let said = if start.map.is_empty() {
            "New games start on the first map, wherever its walk begins.".to_string()
        } else if here {
            format!("New games start here, on {}, {}.", start.at[0], start.at[1])
        } else {
            let name = self.content.maps.get(&start.map).map(|m| m.name.clone()).unwrap_or_else(|| start.map.clone());
            format!("New games start on {name}.")
        };
        hint(ui, &said);
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
        crate::platform::set_hook("start", &if start.map.is_empty() { String::new() } else { format!("{}:{},{}", start.map, start.at[0], start.at[1]) });
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
