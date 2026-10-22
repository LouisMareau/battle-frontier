//! Saving by itself, and where the browser's saves go. A moment after the last press (a click,
//! a key, a drag let go), whatever's changed is saved, new things too, as each has its place in
//! the project: in the browser unless saving downloads, on the desktop once "Auto-save" is
//! ticked there (it writes into the project's folders). In the browser each file is kept there
//! at its place (`crate::keep`): "Saved in this browser", in the Menu, lists what's kept,
//! downloads it as one zip, or forgets it.

#[cfg(target_arch = "wasm32")]
use egui::RichText;

use super::{mark, MapEditor};

/// How long after the last press what's changed is saved by itself (seconds).
const AUTOSAVE_SECS: f64 = 1.5;

/// The name the choice is kept under (`platform::store_text`).
const AUTOSAVE_NAME: &str = "editor-autosave";

/// Saving by itself: on or off (kept between runs), and when it's due.
pub(super) struct Autosave {
    pub on: bool,
    pub due: Option<f64>,
}

impl Default for Autosave {
    fn default() -> Self {
        // the browser saves by itself unless told not to; the desktop once it's been ticked
        let on = match crate::platform::stored_text(AUTOSAVE_NAME).as_deref().map(str::trim) {
            Some("on") => true,
            Some("off") => false,
            _ => cfg!(target_arch = "wasm32"),
        };
        Autosave { on, due: None }
    }
}

/// "Saved in this browser": open; forgetting it all, to be sure first. (The browser's only.)
#[derive(Default)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) struct KeptWindow {
    pub open: bool,
    pub forgetting: bool,
}

/// Whether saving keeps the files in the browser (else they're written into the project's
/// folders, on the desktop, or downloaded).
pub(super) fn keeps() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        crate::keep::keeping() == crate::keep::Keeping::Ready
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Whether saving by itself can be on: not while the browser downloads each save (each would
/// ask where).
pub(super) fn can_autosave() -> bool {
    !cfg!(target_arch = "wasm32") || keeps()
}

/// Saving in the browser: the file kept there at its place in the project, or downloaded (as
/// `name`) when the browser keeps nothing; noted for Publish either way. True when it was kept.
#[cfg(target_arch = "wasm32")]
pub(super) fn browser_save(path: &str, name: &str, bytes: Vec<u8>, mime: &str) -> Result<bool, String> {
    if keeps() {
        crate::keep::keep(path, bytes);
        return Ok(true);
    }
    crate::platform::download_bytes(name, &bytes, mime)?;
    crate::keep::note(path, bytes);
    Ok(false)
}

/// A record saved in the browser (`browser_save`), at `<space>/data/<collection>/<id>.json`;
/// `name` when it's downloaded.
#[cfg(target_arch = "wasm32")]
pub(super) fn browser_save_record<T: content::Record>(space: content::Space, record: &T, name: &str) -> Result<bool, String> {
    browser_save(&format!("{}/data/{}/{}.json", space.dir(), T::COLLECTION, record.id()), name, content::to_text(record).into_bytes(), "application/json")
}

impl MapEditor {
    /// Whether anything's changed and not saved yet: the map (its creatures with it), the
    /// world, the props, the world settings, the abilities, the FX, the templates, the
    /// terrains.
    pub(super) fn unsaved(&self) -> bool {
        self.dirty || self.worlds.dirty || self.assets.changed() > 0 || self.post_changed() || !self.abilities.dirty.is_empty() || !self.tl.fx_dirty.is_empty() || !self.tpl.dirty.is_empty() || !self.terrains_dirty.is_empty()
    }

    /// Saving by itself, each frame: what's changed, a moment after the last press, with
    /// nothing held and nothing being typed into (a name half typed isn't a new file).
    pub(super) fn autosave(&mut self, ctx: &egui::Context) {
        if self.review.is_some() {
            return;
        }
        crate::platform::set_hook("autosave", if self.autosave.on && can_autosave() { "on" } else { "off" });
        if !(self.autosave.on && can_autosave()) || !self.unsaved() || self.welcome {
            self.autosave.due = None;
            return;
        }
        let now = ctx.input(|i| i.time);
        let pressed = ctx.input(|i| i.pointer.any_down() || i.events.iter().any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::PointerButton { .. })));
        if pressed || ctx.egui_wants_keyboard_input() || self.autosave.due.is_none() {
            self.autosave.due = Some(now + AUTOSAVE_SECS);
        }
        let due = self.autosave.due.unwrap_or(now);
        if now < due {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
            return;
        }
        self.autosave.due = None;
        self.save_some(self.dirty);
        if self.worlds.dirty {
            let saved = std::mem::take(&mut self.status);
            self.save_world();
            if !saved.is_empty() {
                self.status = format!("{saved} · {}", self.status);
            }
        }
    }

    /// The top bar's Auto-save, next to Save: on or off, kept between runs.
    pub(super) fn autosave_toggle(&mut self, ui: &mut egui::Ui) {
        let can = can_autosave();
        let about = if cfg!(target_arch = "wasm32") {
            "Saves what you change a moment after you stop, new things too, kept in this browser (Publish sends it)"
        } else {
            "Saves what you change a moment after you stop, new things too, into the project's folders"
        };
        let t = ui.add_enabled(can, egui::Button::selectable(self.autosave.on && can, "Auto-save")).on_hover_text(about).on_disabled_hover_text("Saving downloads each file here: saving by itself would download them over and over");
        mark("uiAutosave", &t);
        if t.clicked() {
            self.autosave.on = !self.autosave.on;
            self.autosave.due = None;
            if let Err(e) = crate::platform::store_text(AUTOSAVE_NAME, if self.autosave.on { "on" } else { "off" }) {
                log::warn!("the Auto-save choice isn't kept: {e}");
            }
        }
    }

    /// "Saved in this browser" (the Menu, in the browser), when it's open: what's kept, each at
    /// its place in the project; one zip of it; or it all forgotten (the page opens again on
    /// the site's own content). What went wrong keeping a file said in the status line.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn kept_window(&mut self, ctx: &egui::Context) {
        use crate::keep::{Keeping, Kept};
        if let Some(trouble) = crate::keep::take_trouble() {
            self.status = trouble;
        }
        let kept = crate::keep::files();
        let keeping = crate::keep::keeping();
        crate::platform::set_hook(
            "keeping",
            &match &keeping {
                Keeping::Ready => "ready".to_string(),
                Keeping::Loading => "loading".to_string(),
                Keeping::No(why) => format!("no: {why}"),
            },
        );
        crate::platform::set_hook("kept", &kept.keys().cloned().collect::<Vec<_>>().join(";"));
        crate::platform::set_hook("keptwindow", if self.kept_window.open { "open" } else { "" });
        if !self.kept_window.open {
            return;
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("editor-kept")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new("Saved in this browser").size(20.0).color(crate::hud::TEXT));
            ui.add_space(4.0);
            match &keeping {
                Keeping::Ready => super::hint(ui, "What you save is kept here, each file at its place in the project: laid over the site's content whenever the editor opens, until the game has the same (then it's dropped) or you forget it. Publish sends the game's own (content/) as a pull request; the model requests' folders (private/) and the placeholders' (temporary/) stay here."),
                Keeping::Loading => super::hint(ui, "Reading what's kept…"),
                Keeping::No(why) => super::warn(ui, &format!("Nothing's kept here ({why}): saving downloads each file.")),
            }
            ui.add_space(6.0);
            if kept.is_empty() {
                ui.label(RichText::new("Nothing saved here yet.").color(crate::hud::TEXT));
            } else {
                super::hint(ui, &format!("{} file{}:", kept.len(), if kept.len() == 1 { "" } else { "s" }));
                egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (path, k) in &kept {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(path.trim_start_matches("content/")).monospace().size(11.5).color(crate::hud::TEXT));
                            super::hint(
                                ui,
                                &match k {
                                    Kept::File(b) => super::publish::size(b.len() as u64),
                                    Kept::Removed => "removed".to_string(),
                                },
                            );
                        });
                    }
                });
            }
            ui.add_space(8.0);
            if self.kept_window.forgetting {
                super::warn(ui, "Forget everything saved here? What you haven't published is lost, and the editor opens again on the site's own content.");
                ui.horizontal(|ui| {
                    let yes = ui.button("Forget them");
                    mark("uiKeptForgetYes", &yes);
                    if yes.clicked() {
                        crate::keep::forget_all(|| {
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().reload();
                            }
                        });
                    }
                    let no = ui.button("Keep them");
                    mark("uiKeptForgetNo", &no);
                    if no.clicked() {
                        self.kept_window.forgetting = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    let files = kept.values().any(|k| matches!(k, Kept::File(_)));
                    let zip = ui.add_enabled(files, egui::Button::new("Download them (.zip)")).on_hover_text("Every file kept here in one zip, each at its place in the project");
                    mark("uiKeptZip", &zip);
                    if zip.clicked() {
                        self.status = match crate::platform::download_bytes("battle-frontier-saved.zip", &crate::keep::zip(&kept), "application/zip") {
                            Ok(()) => "Downloaded battle-frontier-saved.zip: the files at their places in the project".to_string(),
                            Err(e) => format!("Not downloaded: {e}"),
                        };
                    }
                    let forget = ui.add_enabled(!kept.is_empty(), egui::Button::new("Forget them…"));
                    mark("uiKeptForget", &forget);
                    if forget.clicked() {
                        self.kept_window.forgetting = true;
                    }
                    let c = ui.button("Close");
                    mark("uiKeptClose", &c);
                    close = c.clicked();
                });
            }
        });
        if close || modal.should_close() {
            self.kept_window = KeptWindow::default();
        }
    }
}
