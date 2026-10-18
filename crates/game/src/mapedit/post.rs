//! The Post-processing window: the toon look (the world settings record's `toon`), how
//! what's toon is lit and its ink lines, the colour steps and the clouds' shadows. Changes show on the map at once and wait to be
//! saved into the record (into its file on the desktop, as a download in the browser);
//! Ctrl+S saves them with the map. What's toon is ticked per asset: a prop in the asset
//! browser or a placed prop's panel, a creature in the asset browser's Creatures.

use std::sync::Arc;

use content::{Space, Toon};

use super::{hint, mark, section, MapEditor};

#[derive(Default)]
pub(super) struct Post {
    pub open: bool,
    /// the look as changed here, not saved yet
    draft: Option<Toon>,
}

/// "#rrggbb" edited with a colour picker.
fn colour(ui: &mut egui::Ui, hex: &mut String) -> egui::Response {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0xff00ff);
    let mut rgb = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
    let r = ui.color_edit_button_srgb(&mut rgb);
    if r.changed() {
        *hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    }
    r
}

impl MapEditor {
    /// The toon look as it stands: as changed in the window, else the content's.
    pub(super) fn toon_settings(&self) -> Toon {
        self.post.draft.clone().unwrap_or_else(|| self.content.world().toon)
    }

    pub(super) fn post_changed(&self) -> bool {
        self.post.draft.is_some()
    }

    /// The top bar's button.
    pub(super) fn post_toggle(&mut self, ui: &mut egui::Ui) {
        let r = ui.selectable_label(self.post.open, "Post-processing").on_hover_text("The toon look: how what's toon is lit, and its ink lines");
        mark("uiPost", &r);
        if r.clicked() {
            self.post.open = !self.post.open;
        }
    }

    pub(super) fn post_window(&mut self, ctx: &egui::Context) {
        let mut t = self.toon_settings();
        crate::platform::set_hook("post", if self.post.open { "open" } else { "" });
        crate::platform::set_hook("toon", &format!("{} bands, ink {}, {} colour steps", t.bands, if t.ink { format!("{:.1}px", t.ink_width) } else { "off".into() }, t.color_steps));
        crate::platform::set_hook("postdirty", if self.post_changed() { "1" } else { "" });
        crate::platform::set_hook("clouds", &format!("{:.2}", t.cloud_shadow));
        if !self.post.open {
            return;
        }
        let before = t.clone();
        let (mut open, mut save, mut revert, mut defaults) = (true, false, false, false);
        let props = self.content.props.iter().filter(|p| p.toon).count();
        let creatures = self.content.creatures.iter().filter(|c| c.toon).count();
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        egui::Window::new("Post-processing").id(egui::Id::new("post-window")).open(&mut open).collapsible(false).resizable(false).default_width(340.0).show(ctx, |ui| {
            section(ui, "Toon look");
            hint(ui, &format!("For what's ticked toon: {props} prop{}, {creatures} creature{}. Tick them in the asset browser (creatures under Creatures) or a placed prop's panel.", plural(props), plural(creatures)));
            ui.add_space(4.0);
            egui::Grid::new("post-light").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Light steps");
                let r = ui.add(egui::Slider::new(&mut t.bands, 1..=5)).on_hover_text("How many steps the light takes from dark to lit");
                mark("uiPostBands", &r);
                ui.end_row();
                ui.label("Step edges");
                ui.add(egui::Slider::new(&mut t.softness, 0.0..=1.0)).on_hover_text("0: hard, 1: soft");
                ui.end_row();
                ui.label("Shade colour");
                colour(ui, &mut t.shadow_color).on_hover_text("What the shaded side turns towards");
                ui.end_row();
                ui.label("Shade tint");
                ui.add(egui::Slider::new(&mut t.shadow, 0.0..=1.0)).on_hover_text("How far the shade turns that colour");
                ui.end_row();
                ui.label("Rim light");
                ui.add(egui::Slider::new(&mut t.rim, 0.0..=2.0)).on_hover_text("A rim of light round the edge (0: none)");
                ui.end_row();
                ui.label("Rim width");
                ui.add(egui::Slider::new(&mut t.rim_width, 0.0..=1.0));
                ui.end_row();
                ui.label("Glint");
                ui.add(egui::Slider::new(&mut t.glint, 0.0..=2.0)).on_hover_text("A hard highlight where the sun catches (0: none)");
                ui.end_row();
                ui.label("Glint size");
                ui.add(egui::Slider::new(&mut t.glint_size, 0.0..=1.0));
                ui.end_row();
            });
            section(ui, "Ink lines");
            let ink = ui.checkbox(&mut t.ink, "Outline what's toon");
            mark("uiPostInk", &ink);
            ui.add_enabled_ui(t.ink, |ui| {
                egui::Grid::new("post-ink").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("Colour");
                    colour(ui, &mut t.ink_color);
                    ui.end_row();
                    ui.label("Width");
                    let r = ui.add(egui::Slider::new(&mut t.ink_width, 0.5..=6.0).suffix(" px")).on_hover_text("Pixels of the screen");
                    mark("uiPostWidth", &r);
                    ui.end_row();
                    ui.label("Opacity");
                    ui.add(egui::Slider::new(&mut t.ink_opacity, 0.0..=1.0));
                    ui.end_row();
                    ui.label("Depth step");
                    ui.add(egui::Slider::new(&mut t.depth_step, 0.005..=0.5).logarithmic(true)).on_hover_text("How far behind what's around it must be for a line, as a share of its distance: smaller, more lines");
                    ui.end_row();
                    ui.label("Creases");
                    ui.add(egui::Slider::new(&mut t.crease, 0.0..=179.0).suffix("°")).on_hover_text("Folds sharper than this get a line too (0: none)");
                    ui.end_row();
                });
            });
            hint(ui, "A line round it where it stands in front of what's behind (drawn outside it), and along its sharp folds.");
            section(ui, "Colour steps");
            hint(ui, "The whole picture's light in bands, toon or not (the world, not the interface): no smooth gradients.");
            egui::Grid::new("post-steps").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Steps");
                let mut steps = t.color_steps.max(1) as u32;
                let r = ui.add(egui::Slider::new(&mut steps, 1..=16).custom_formatter(|v, _| if v < 1.5 { "off".into() } else { format!("{v:.0}") })).on_hover_text("How many bands of light, dark to bright (off: smooth)");
                mark("uiPostSteps", &r);
                t.color_steps = if steps < 2 { 0 } else { steps as u8 };
                ui.end_row();
                ui.label("Step edges");
                ui.add_enabled(t.color_steps >= 2, egui::Slider::new(&mut t.color_softness, 0.0..=1.0)).on_hover_text("0: hard, 1: soft");
                ui.end_row();
            });
            section(ui, "Cloud shadows");
            hint(ui, "Clouds' shadows drifting over the world, in the sun's direction: darker where it's lit, fainter by moonlight.");
            egui::Grid::new("post-clouds").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Shadow");
                let r = ui.add(egui::Slider::new(&mut t.cloud_shadow, 0.0..=1.0).custom_formatter(|v, _| if v < 0.005 { "off".into() } else { format!("{v:.2}") })).on_hover_text("How dark a cloud makes what's in the sun (0: no clouds)");
                mark("uiPostClouds", &r);
                crate::platform::set_hook("uiPostCloudsRect", &format!("{:.0},{:.0},{:.0},{:.0}", r.rect.left(), r.rect.top(), r.rect.width(), r.rect.height()));
                ui.end_row();
                ui.label("Size");
                ui.add_enabled(t.cloud_shadow > 0.0, egui::Slider::new(&mut t.cloud_size, 6.0..=120.0).logarithmic(true).suffix(" cells")).on_hover_text("How wide a cloud is");
                ui.end_row();
                ui.label("Speed");
                ui.add_enabled(t.cloud_shadow > 0.0, egui::Slider::new(&mut t.cloud_speed, 0.0..=6.0).suffix(" cells/s")).on_hover_text("How fast they drift with the wind");
                ui.end_row();
                ui.label("Cover");
                ui.add_enabled(t.cloud_shadow > 0.0, egui::Slider::new(&mut t.cloud_cover, 0.0..=1.0)).on_hover_text("How much of the sky they cover");
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                let s = ui.add_enabled(self.post_changed(), egui::Button::new(if self.post_changed() { "Save •" } else { "Save" })).on_hover_text("Into the world settings record (Ctrl+S saves it with the map)");
                mark("uiPostSave", &s);
                save = s.clicked();
                revert = ui.add_enabled(self.post_changed(), egui::Button::new("Revert")).on_hover_text("Back to what's saved").clicked();
                let d = ui.button("Defaults");
                mark("uiPostDefaults", &d);
                defaults = d.clicked();
            });
        });
        if defaults {
            t = Toon::default();
        }
        if t != before {
            self.post.draft = Some(t);
        }
        if revert {
            self.post.draft = None;
        }
        if save {
            self.save_post();
        }
        if !open {
            self.post.open = false;
        }
    }

    /// Writes the world settings record with the toon look as changed here.
    pub(super) fn save_post(&mut self) {
        let Some(t) = self.post.draft.clone() else { return };
        let space = self.content.settings.entry("world").map(|e| e.space).unwrap_or(Space::Content);
        let mut record = self.content.world();
        record.toon = t;
        #[cfg(not(target_arch = "wasm32"))]
        let saved = {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            content::disk::save(&root, space, &record).map(|path| format!("Saved the toon look into {}", path.display()))
        };
        #[cfg(target_arch = "wasm32")]
        let saved = crate::platform::download("world.json", &content::to_text(&record)).map(|()| format!("Downloaded world.json: the toon look, for {}/data/settings/", space.dir()));
        match saved {
            Ok(said) => {
                let mut c = (*self.content).clone();
                c.settings.put(record, space);
                self.content = Arc::new(c);
                self.post.draft = None;
                self.status = said;
            }
            Err(e) => self.status = format!("The toon look isn't saved: {e}"),
        }
    }
}
