//! The Post-processing window: the toon look (the world settings record's `toon`), how
//! what's toon is lit and its ink lines, the colour steps and the clouds' shadows; the
//! picture's light (its `light`: exposure, tone map, bloom, the indirect light of what
//! glows); and the world's wind (its `wind`). Changes show on the map at once and wait to be
//! saved into the record (into its file on the desktop, as a download in the browser);
//! Ctrl+S saves them with the map. What's toon is ticked per asset: a prop in the asset
//! browser or a placed prop's panel, a creature in the asset browser's Creatures.

use std::sync::Arc;

use content::{Radiance, Space, Toon, Wind};

use super::{hint, mark, section, MapEditor};

#[derive(Default)]
pub(super) struct Post {
    pub open: bool,
    /// the look as changed here, not saved yet
    draft: Option<Toon>,
    /// the wind as changed here, not saved yet
    wind: Option<Wind>,
    /// the light as changed here, not saved yet
    light: Option<Radiance>,
}

/// A way on the map, in words: north (up the map, -z), north-east...
fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    POINTS[((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

/// A little compass, north up: an arrow the way the wind blows, as long as it's strong.
fn wind_rose(ui: &mut egui::Ui, w: &Wind) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    p.circle_stroke(c, 19.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)));
    p.text(c - egui::vec2(0.0, 15.0), egui::Align2::CENTER_CENTER, "N", egui::FontId::proportional(9.0), egui::Color32::from_gray(150));
    let [x, z] = w.way();
    let way = egui::vec2(x as f32, z as f32);
    let reach = 6.0 + 11.0 * w.strength as f32;
    let tip = c + way * reach;
    let colour = egui::Color32::from_rgb(140, 200, 255);
    p.line_segment([c - way * reach * 0.6, tip], egui::Stroke::new(2.0, colour));
    let side = egui::vec2(-way.y, way.x);
    p.line_segment([tip, tip - way * 5.0 + side * 3.5], egui::Stroke::new(2.0, colour));
    p.line_segment([tip, tip - way * 5.0 - side * 3.5], egui::Stroke::new(2.0, colour));
    r
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

    /// The wind as it stands: as changed in the window, else the content's.
    pub(super) fn wind_settings(&self) -> Wind {
        self.post.wind.clone().unwrap_or_else(|| self.content.world().wind)
    }

    /// The picture's light as it stands: as changed in the window, else the content's.
    pub(super) fn light_settings(&self) -> Radiance {
        self.post.light.clone().unwrap_or_else(|| self.content.world().light)
    }

    pub(super) fn post_changed(&self) -> bool {
        self.post.draft.is_some() || self.post.wind.is_some() || self.post.light.is_some()
    }

    /// The Post-processing panel (docked or floating, as the layout has it), when it's open;
    /// its marks for the checks whether or not.
    pub(super) fn post_window(&mut self, ui: &mut egui::Ui) {
        let mut t = self.toon_settings();
        let mut w = self.wind_settings();
        let mut l = self.light_settings();
        crate::platform::set_hook("wind", &format!("{:.0} {:.2} {:.2}", w.direction, w.strength, w.gusts));
        crate::platform::set_hook("light", &format!("{:.2} {} {:.2} {:.2} {:.0}", l.exposure, l.tone, l.bloom, l.gi, l.gi_range));
        crate::platform::set_hook("post", if self.post.open { "open" } else { "" });
        crate::platform::set_hook("toon", &format!("{} bands, ink {}, {} colour steps", t.bands, if t.ink { format!("{:.1}px", t.ink_width) } else { "off".into() }, t.color_steps));
        crate::platform::set_hook("postdirty", if self.post_changed() { "1" } else { "" });
        crate::platform::set_hook("clouds", &format!("{:.2}", t.cloud_shadow));
        if !self.post.open {
            return;
        }
        let before = t.clone();
        let wind_before = w.clone();
        let light_before = l.clone();
        let (mut save, mut revert, mut defaults) = (false, false, false);
        let props = self.content.props.iter().filter(|p| p.toon).count();
        let creatures = self.content.creatures.iter().filter(|c| c.toon).count();
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let mut st = self.dock.get(super::dock::Pane::Post);
        // floating, in two columns (the toon look and its lines; the clouds, the light and the
        // wind), so it fits the screen; docked, one; what doesn't fit scrolls, the buttons
        // staying under it
        let wide = !st.docked;
        let room = (ui.ctx().content_rect().height() - 150.0).clamp(200.0, 1200.0);
        super::dock::show(ui, &mut st, super::dock::Pane::Post, "Post-processing", |ui| {
            if wide {
                ui.set_min_width(700.0);
            }
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
            let mut part = |ui: &mut egui::Ui, k: usize| {
            if k == 0 {
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
            return;
            }
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
            section(ui, "Light");
            hint(ui, "The picture exposed and tone mapped to the screen, what glows blooming, and the indirect light: what glows (lava, arcane moss, the lamps bounced off the ground) lighting what's round it, in the shadow of walls and cliffs (WebGPU; the Options can turn it off).");
            let off = |v: f64, _: std::ops::RangeInclusive<usize>| if v < 0.005 { "off".to_string() } else { format!("{v:.2}") };
            egui::Grid::new("post-radiance").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Exposure");
                let r = ui.add(egui::Slider::new(&mut l.exposure, 0.25..=4.0).logarithmic(true)).on_hover_text("The colours times this before the tone map (1: as they are)");
                mark("uiPostExposure", &r);
                ui.end_row();
                ui.label("Tone map");
                ui.horizontal(|ui| {
                    let r = ui.selectable_value(&mut l.tone, "soft".to_string(), "Soft").on_hover_text("The colours as they are up to 0.8, what's brighter bent under 1: a glow keeps its hue");
                    mark("uiPostToneSoft", &r);
                    let r = ui.selectable_value(&mut l.tone, "filmic".to_string(), "Filmic").on_hover_text("A film's contrast and shoulder over the whole range (an ACES fit)");
                    mark("uiPostToneFilmic", &r);
                });
                ui.end_row();
                ui.label("Bloom");
                let r = ui.add(egui::Slider::new(&mut l.bloom, 0.0..=1.0).custom_formatter(off)).on_hover_text("How much of the blurred glow bleeds into what's round it (0: none)");
                mark("uiPostBloom", &r);
                ui.end_row();
                ui.label("Blooms past");
                ui.add_enabled(l.bloom > 0.0, egui::Slider::new(&mut l.threshold, 0.5..=4.0)).on_hover_text("What else blooms: the colour's brightness past this (1: only what's brighter than white)");
                ui.end_row();
                ui.label("Indirect light");
                let r = ui.add(egui::Slider::new(&mut l.gi, 0.0..=2.0).custom_formatter(off)).on_hover_text("How much what glows lights what's round it (0: none)");
                mark("uiPostGi", &r);
                crate::platform::set_hook("uiPostGiRect", &format!("{:.0},{:.0},{:.0},{:.0}", r.rect.left(), r.rect.top(), r.rect.width(), r.rect.height()));
                ui.end_row();
                ui.label("Reach");
                ui.add_enabled(l.gi > 0.0, egui::Slider::new(&mut l.gi_range, 8.0..=128.0).logarithmic(true).suffix(" cells")).on_hover_text("How far round the camera's target it's worked out: further, coarser");
                ui.end_row();
            });
            section(ui, "Wind");
            hint(ui, "Sways what's painted for it (a model's wind paint, a stand-in's Sway and Flutter) and the tall grass; ripples inland water; the sea's waves grow when it blows to land; the clouds drift with it.");
            ui.horizontal(|ui| {
                wind_rose(ui, &w).on_hover_text("North is up the map");
                egui::Grid::new("post-wind").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("Blowing to");
                    let r = ui.add(egui::Slider::new(&mut w.direction, 0.0..=359.0).custom_formatter(|v, _| format!("{v:.0}° {}", compass(v)))).on_hover_text("The way it blows, in degrees clockwise from north (up the map)");
                    mark("uiPostWindWay", &r);
                    ui.end_row();
                    ui.label("Strength");
                    let r = ui.add(egui::Slider::new(&mut w.strength, 0.0..=1.0).custom_formatter(|v, _| if v < 0.005 { "calm".into() } else { format!("{v:.2}") })).on_hover_text("0: calm, 1: a gale");
                    mark("uiPostWindStrength", &r);
                    crate::platform::set_hook("uiPostWindStrengthRect", &format!("{:.0},{:.0},{:.0},{:.0}", r.rect.left(), r.rect.top(), r.rect.width(), r.rect.height()));
                    ui.end_row();
                    ui.label("Gusts");
                    let r = ui.add(egui::Slider::new(&mut w.gusts, 0.0..=1.0)).on_hover_text("0: steady, 1: all in gusts");
                    mark("uiPostWindGusts", &r);
                    ui.end_row();
                });
            });
            };
            if wide {
                ui.columns(2, |cols| {
                    part(&mut cols[0], 0);
                    part(&mut cols[1], 1);
                });
            } else {
                part(ui, 0);
                part(ui, 1);
            }
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
            w = Wind::default();
            l = Radiance::default();
        }
        if t != before {
            self.post.draft = Some(t);
        }
        if w != wind_before {
            self.post.wind = Some(w);
        }
        if l != light_before {
            self.post.light = Some(l);
        }
        if revert {
            self.post.draft = None;
            self.post.wind = None;
            self.post.light = None;
        }
        if save {
            self.save_post();
        }
        if !st.open {
            self.post.open = false;
        }
        self.dock.set(super::dock::Pane::Post, st);
    }

    /// Writes the world settings record with the toon look, the light and the wind as
    /// changed here.
    pub(super) fn save_post(&mut self) {
        if !self.post_changed() {
            return;
        }
        let space = self.content.settings.entry("world").map(|e| e.space).unwrap_or(Space::Content);
        let mut record = self.content.world();
        record.toon = self.toon_settings();
        record.wind = self.wind_settings();
        record.light = self.light_settings();
        #[cfg(not(target_arch = "wasm32"))]
        let saved = {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            content::disk::save(&root, space, &record).map(|path| format!("Saved the toon look and the wind into {}", path.display()))
        };
        #[cfg(target_arch = "wasm32")]
        let saved = super::saving::browser_save_record(space, &record, "world.json").map(|kept| {
            if kept {
                "Saved the toon look and the wind in this browser".to_string()
            } else {
                format!("Downloaded world.json: the toon look and the wind, for {}/data/settings/", space.dir())
            }
        });
        match saved {
            Ok(said) => {
                let mut c = (*self.content).clone();
                c.settings.put(record, space);
                self.content = Arc::new(c);
                self.post.draft = None;
                self.post.wind = None;
                self.post.light = None;
                self.status = said;
            }
            Err(e) => self.status = format!("The toon look and the wind aren't saved: {e}"),
        }
    }
}
