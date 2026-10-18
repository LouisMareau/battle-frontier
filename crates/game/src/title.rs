//! The game's main menu, the first thing on screen: Continue (the saved story), New game,
//! Options and Exit over the world (the app draws a map behind it, its camera drifting), and
//! the Editor's icon in the bottom-right corner. And the Options, in tabs (Game, Graphics,
//! Controls, Audio), which the editor's Test battles open too; they're kept between runs.
//!
//! The words are Roboto (Bold for the titles, Medium for the items, the tabs and headings,
//! Light for the rest); every move here is a quick tween (0.3 s at most): an item hovered grows a little and
//! steps right; an item picked steps right while the others slide left and fade.

use std::sync::Arc;

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::{Graphics, Options};
use crate::input::{Bindings, Button, Input};
use crate::platform;

/// What the player picked on the main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Continue,
    /// a new game (the saved story, if any, given up: the player said so)
    NewGame,
    Exit,
    Editor,
}

/// The menu's items, in order, and their places' names for the checks.
const ITEMS: [&str; 4] = ["Continue", "New game", "Options", "Exit"];
const HOOKS: [&str; 4] = ["uiContinue", "uiNewGame", "uiOptions", "uiExit"];

/// The tweens: an item hovered, an item picked (and the others leaving), the world fading in.
const HOVER_S: f32 = 0.15;
const PICK_S: f32 = 0.25;
const FADE_MS: f64 = 300.0;

/// The game's title and headings: Roboto Bold (SIL Open Font License: crates/game/fonts/).
pub fn title_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("title".into()))
}

/// The menu's items and tabs: Roboto Medium.
pub fn menu_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("menu".into()))
}

/// The game's fonts, Roboto (SIL Open Font License: crates/game/fonts/): Light for all the
/// text, Medium for headings (`menu_font`, egui's headings), Bold for titles (`title_font`);
/// each falling back on egui's own for what it hasn't got (symbols, emoji).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with_fallback = |name: &str| {
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        list
    };
    for (name, bytes) in [
        ("roboto-light", &include_bytes!("../fonts/Roboto-Light.ttf")[..]),
        ("roboto-medium", &include_bytes!("../fonts/Roboto-Medium.ttf")[..]),
        ("roboto-bold", &include_bytes!("../fonts/Roboto-Bold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    }
    fonts.families.insert(FontFamily::Proportional, with_fallback("roboto-light"));
    fonts.families.insert(FontFamily::Name("menu".into()), with_fallback("roboto-medium"));
    fonts.families.insert(FontFamily::Name("title".into()), with_fallback("roboto-bold"));
    ctx.set_fonts(fonts);
    // egui's headings in Medium
    let mut style = (*ctx.global_style()).clone();
    style.text_styles.insert(egui::TextStyle::Heading, menu_font(19.0));
    ctx.set_global_style(style);
}

/// The Editor's icon.
fn editor_icon() -> egui::ImageSource<'static> {
    egui::include_image!("../icons/editor.svg")
}

#[derive(Default)]
pub struct Title {
    /// the item the keys (or the pointer) are on
    sel: usize,
    /// the item picked, and when: its action follows its tween
    picked: Option<(usize, f64)>,
    /// asking whether to give up the saved story for a new game
    confirm_new: bool,
    pub options_open: bool,
    pub options: OptionsUi,
    /// when Exit was pressed in the browser (a page may not close its tab: a note then)
    exit_tried: Option<f64>,
    /// when the world behind came up (it fades in)
    shown_at: Option<f64>,
}

impl Title {
    /// The menu, over the world the app has drawn (`world_ready`: it's up); what the player
    /// picked. `saved`: the saved story's place (its map's name), when there is one.
    /// `options_changed` is set when an option changed.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut egui::Ui, input: &Input, now: f64, world_ready: bool, saved: Option<&str>, options: &mut Options, options_changed: &mut bool) -> Option<Choice> {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let enabled = [saved.is_some(), true, true, true];
        if !enabled[self.sel] {
            self.sel = 1;
        }
        // a dialog up, or an item on its way: the menu waits
        let busy = self.confirm_new || self.options_open || self.picked.is_some();
        let mut clicked = None;
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let p = ui.painter().clone();
            // the world fades in once it's up; until then the night blue
            if world_ready && self.shown_at.is_none() {
                self.shown_at = Some(now);
            }
            let fade = self.shown_at.map(|at| ((now - at) / FADE_MS).clamp(0.0, 1.0) as f32).unwrap_or(0.0);
            if fade < 1.0 {
                p.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(12, 17, 28, ((1.0 - fade) * 255.0) as u8));
            }
            shade(&p, screen);
            let narrow = screen.width() < 820.0;
            let left = if narrow { screen.center().x } else { screen.left() + (screen.width() * 0.08).max(40.0) };
            let align = if narrow { Align2::CENTER_TOP } else { Align2::LEFT_TOP };
            let top = screen.top() + screen.height() * if narrow { 0.16 } else { 0.2 };
            let title_size = if narrow { 46.0 } else { 76.0 };
            shadowed(&p, Pos2::new(left, top), align, "Battle Frontier", title_font(title_size), Color32::WHITE);
            shadowed(&p, Pos2::new(left + if narrow { 0.0 } else { 3.0 }, top + title_size * 1.12), align, "Creatures, timed battles, and the maps between them.", FontId::proportional(if narrow { 14.0 } else { 18.0 }), Color32::from_white_alpha(220));
            // the items
            let first = screen.top() + screen.height() * if narrow { 0.5 } else { 0.47 };
            let (step, size) = if narrow { (54.0, 30.0) } else { (62.0, 36.0) };
            let pick_t = ctx.animate_value_with_time(ui.id().with("title-pick"), if self.picked.is_some() || self.confirm_new || self.options_open { 1.0 } else { 0.0 }, PICK_S);
            let chosen = self.picked.map(|(i, _)| i).or(if self.confirm_new || self.options_open { Some(self.sel) } else { None });
            for (i, name) in ITEMS.iter().enumerate() {
                let base = Pos2::new(left, first + i as f32 * step);
                // what the pointer can hit: the item's line
                let width = if narrow { 260.0 } else { 300.0 };
                let hit = if narrow { Rect::from_center_size(base + Vec2::new(0.0, size * 0.55), Vec2::new(width, step - 8.0)) } else { Rect::from_min_size(base - Vec2::new(8.0, 4.0), Vec2::new(width, step - 8.0)) };
                let r = ui.interact(hit, ui.id().with(("title-item", i)), if enabled[i] && !busy { Sense::click() } else { Sense::hover() });
                platform::set_hook(HOOKS[i], &format!("{:.0},{:.0}", hit.center().x, hit.center().y));
                let about = match i {
                    0 if saved.is_none() => "No saved story yet: New game starts one".to_string(),
                    0 => format!("Back to your story, where you left it ({})", saved.unwrap_or_default()),
                    1 => "A new story, from the start".into(),
                    2 => "Game, graphics, controls and audio".into(),
                    _ => "Leave the game".into(),
                };
                let r = if busy { r } else { r.on_hover_text(about) };
                if r.hovered() && enabled[i] && !busy {
                    self.sel = i;
                }
                let on = self.sel == i && enabled[i];
                let hover = ctx.animate_value_with_time(ui.id().with(("title-hover", i)), if on { 1.0 } else { 0.0 }, HOVER_S);
                let (shift, alpha) = match chosen {
                    Some(c) if c == i => (16.0 * pick_t, 1.0),
                    Some(_) => (-56.0 * pick_t, 1.0 - pick_t),
                    None => (0.0, 1.0),
                };
                let a = alpha * if enabled[i] { 1.0 } else { 0.38 };
                if a <= 0.01 {
                    continue;
                }
                // grown about its line's middle, not down onto the next
                let at = base + Vec2::new(10.0 * hover + shift, size * 0.6);
                let middle = if narrow { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
                let colour = Color32::from_white_alpha((a * 255.0) as u8);
                let shown = shadowed_alpha(&p, at, middle, name, menu_font(size * (1.0 + 0.08 * hover)), colour, a);
                if i == 0 {
                    if let Some(place) = saved {
                        let (pos, al) = if narrow { (Pos2::new(shown.center().x, shown.bottom() + 2.0), Align2::CENTER_TOP) } else { (Pos2::new(shown.right() + 14.0, shown.center().y + 3.0), Align2::LEFT_CENTER) };
                        shadowed_alpha(&p, pos, al, place, FontId::proportional(14.0), Color32::from_white_alpha((a * 190.0) as u8), a);
                    }
                }
                if r.clicked() {
                    clicked = Some(i);
                }
            }
            if self.exit_tried.is_some_and(|at| now - at > 400.0) {
                let y = first + ITEMS.len() as f32 * step + 6.0;
                shadowed(&p, Pos2::new(left, y), align, "The browser keeps this tab open: close it to leave.", FontId::proportional(14.0), Color32::from_white_alpha(200));
            }
            // the player's keys, bottom left
            let hint = if narrow { String::new() } else { keys_hint(&crate::input::bindings()) };
            platform::set_hook("keyshint", &hint);
            if !narrow {
                shadowed(&p, Pos2::new(left, screen.bottom() - 24.0), Align2::LEFT_BOTTOM, &hint, FontId::proportional(13.5), Color32::from_white_alpha(190));
            }
            // the Editor: its icon, bottom right, a little bigger when hovered
            let base = 34.0;
            let spot = Rect::from_center_size(Pos2::new(screen.right() - 44.0, screen.bottom() - 42.0), Vec2::splat(base + 10.0));
            let r = ui.interact(spot, ui.id().with("title-editor"), if busy { Sense::hover() } else { Sense::click() });
            let r = if busy { r } else { r.on_hover_text("The editor: make maps, props, creatures and abilities, and add them to the game") };
            platform::set_hook("uiEditor", &format!("{:.0},{:.0}", spot.center().x, spot.center().y));
            let grow = ctx.animate_value_with_time(ui.id().with("title-editor-grow"), if r.hovered() && !busy { 1.0 } else { 0.0 }, HOVER_S);
            let icon = Rect::from_center_size(spot.center(), Vec2::splat(base * (1.0 + 0.18 * grow)));
            egui::Image::new(editor_icon()).fit_to_exact_size(Vec2::splat(64.0)).tint(Color32::WHITE).paint_at(ui, icon);
            platform::set_hook("editorgrow", &format!("{grow:.2}"));
            if r.clicked() {
                clicked = Some(ITEMS.len());
            }
        });
        // the keys: up and down through what can be picked, A picks
        if !busy && !ctx.egui_wants_keyboard_input() {
            let step = |from: usize, dir: isize| {
                let mut i = from as isize;
                for _ in 0..ITEMS.len() {
                    i = (i + dir).rem_euclid(ITEMS.len() as isize);
                    if enabled[i as usize] {
                        break;
                    }
                }
                i as usize
            };
            if input.pressed(Button::Down) {
                self.sel = step(self.sel, 1);
            }
            if input.pressed(Button::Up) {
                self.sel = step(self.sel, -1);
            }
            if input.pressed(Button::A) {
                clicked = Some(self.sel);
            }
        }
        // an item picked: its tween, then what it does (the Editor goes at once)
        let mut choice = None;
        match clicked {
            Some(i) if i == ITEMS.len() => choice = Some(Choice::Editor),
            Some(i) if enabled[i] => {
                self.sel = i;
                self.picked = Some((i, now));
            }
            _ => {}
        }
        if let Some((i, at)) = self.picked {
            if now - at >= PICK_S as f64 * 1000.0 + 20.0 {
                self.picked = None;
                match i {
                    0 => choice = Some(Choice::Continue),
                    1 if saved.is_some() => self.confirm_new = true,
                    1 => choice = Some(Choice::NewGame),
                    2 => self.options_open = true,
                    _ => {
                        if cfg!(target_arch = "wasm32") {
                            self.exit_tried = Some(now);
                        }
                        choice = Some(Choice::Exit);
                    }
                }
            }
        }
        platform::set_hook("confirmnew", if self.confirm_new { "open" } else { "" });
        if self.confirm_new {
            match confirm_new(&ctx, saved.unwrap_or_default(), input) {
                Some(true) => {
                    self.confirm_new = false;
                    choice = Some(Choice::NewGame);
                }
                Some(false) => self.confirm_new = false,
                None => {}
            }
        }
        if self.options_open && options_window(&ctx, options, &mut self.options_open, &mut self.options, input) {
            *options_changed = true;
        }
        platform::set_hook("options", if self.options_open { "open" } else { "" });
        platform::set_hook("titlebusy", if self.picked.is_some() { "1" } else { "" });
        choice
    }
}

/// Text with a soft shadow under it, readable over the world.
fn shadowed(p: &egui::Painter, at: Pos2, align: Align2, text: &str, font: FontId, colour: Color32) -> Rect {
    shadowed_alpha(p, at, align, text, font, colour, 1.0)
}

fn shadowed_alpha(p: &egui::Painter, at: Pos2, align: Align2, text: &str, font: FontId, colour: Color32, alpha: f32) -> Rect {
    let shadow = Color32::from_black_alpha((alpha * 110.0) as u8);
    let off = (font.size / 24.0).clamp(1.0, 3.0);
    p.text(at + Vec2::new(0.0, off), align, text, font.clone(), shadow);
    p.text(at, align, text, font, colour)
}

/// A shade over the world where the words are: the left side and the bottom edge darker.
fn shade(p: &egui::Painter, r: Rect) {
    let mut mesh = egui::Mesh::default();
    let mut quad = |a: Pos2, b: Pos2, c: Pos2, d: Pos2, ca: Color32, cb: Color32| {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(a, ca);
        mesh.colored_vertex(b, cb);
        mesh.colored_vertex(c, cb);
        mesh.colored_vertex(d, ca);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    };
    // dark at the left edge, clear past the middle
    let mid = r.left() + r.width() * 0.58;
    quad(r.left_top(), Pos2::new(mid, r.top()), Pos2::new(mid, r.bottom()), r.left_bottom(), Color32::from_black_alpha(150), Color32::TRANSPARENT);
    // and along the bottom
    let band = r.bottom() - 120.0;
    quad(r.left_bottom(), Pos2::new(r.left(), band), Pos2::new(r.right(), band), r.right_bottom(), Color32::from_black_alpha(120), Color32::TRANSPARENT);
    p.add(mesh);
}

/// "Start a new game?" when a story is saved: Some(true) to start over, Some(false) to keep it.
fn confirm_new(ctx: &egui::Context, saved: &str, input: &Input) -> Option<bool> {
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::new("title-new-game")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.label(RichText::new("Start a new game?").font(title_font(26.0)).color(Color32::WHITE));
        ui.add_space(6.0);
        ui.label(RichText::new(format!("The story saved on {saved} is replaced by the new one.")).size(14.0).color(Color32::from_white_alpha(190)));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let yes = ui.add(egui::Button::new(RichText::new("Start a new game").strong().color(Color32::from_rgb(20, 24, 32))).fill(Color32::WHITE));
            platform::set_hook("uiNewGameYes", &format!("{:.0},{:.0}", yes.rect.center().x, yes.rect.center().y));
            if yes.clicked() {
                answer = Some(true);
            }
            let no = ui.button("Keep my story");
            platform::set_hook("uiNewGameNo", &format!("{:.0},{:.0}", no.rect.center().x, no.rect.center().y));
            if no.clicked() {
                answer = Some(false);
            }
        });
    });
    if answer.is_none() && (modal.should_close() || input.pressed(Button::B)) {
        answer = Some(false);
    }
    answer
}

/// A dialog's look: the panels' blue, rounded.
pub fn dialog_frame() -> egui::Frame {
    egui::Frame::new().fill(Color32::from_rgb(20, 27, 41)).corner_radius(14.0).inner_margin(22.0).stroke(Stroke::new(1.0, Color32::from_white_alpha(30)))
}

// ---------------------------------------------------------------- the Options

/// The Options' tabs.
const TABS: [&str; 4] = ["Game", "Graphics", "Controls", "Audio"];

/// The Options' own state: the tab shown, a key being waited for (Controls).
#[derive(Default)]
pub struct OptionsUi {
    pub tab: usize,
    /// a button whose key is being changed (the key it replaces, or a new one): the next key
    /// pressed is it
    waiting: Option<(Button, Option<egui::Key>)>,
}

/// A key's name as the Controls show it.
fn key_label(k: egui::Key) -> String {
    use egui::Key;
    match k {
        Key::ArrowUp => "Up arrow".into(),
        Key::ArrowDown => "Down arrow".into(),
        Key::ArrowLeft => "Left arrow".into(),
        Key::ArrowRight => "Right arrow".into(),
        Key::Escape => "Esc".into(),
        Key::PageUp => "Page Up".into(),
        Key::PageDown => "Page Down".into(),
        other => other.name().to_string(),
    }
}

/// The menu's keys at its foot, from the player's keys: the arrows (else Up's and Down's first
/// keys) choose, A's first two select, B's first two go back.
fn keys_hint(keys: &Bindings) -> String {
    let first = |b: Button, n: usize| keys.keys(b).iter().take(n).map(|&k| key_label(k)).collect::<Vec<_>>().join(" or ");
    let arrows = keys.keys(Button::Up).contains(&egui::Key::ArrowUp) && keys.keys(Button::Down).contains(&egui::Key::ArrowDown);
    let (up, down) = (first(Button::Up, 1), first(Button::Down, 1));
    let choose = if arrows {
        "Arrows".to_string()
    } else if up.is_empty() || down.is_empty() {
        String::new()
    } else {
        format!("{up}, {down}")
    };
    [(choose, "choose"), (first(Button::A, 2), "select"), (first(Button::B, 2), "back")]
        .into_iter()
        .filter(|(keys, _)| !keys.is_empty())
        .map(|(keys, does)| format!("{keys}: {does}"))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// The Options, a dialog over the screen, in tabs: true when an option changed. Done, B (X,
/// Esc) or a click beside it closes it.
pub fn options_window(ctx: &egui::Context, o: &mut Options, open: &mut bool, ui_state: &mut OptionsUi, input: &Input) -> bool {
    let before = o.saved();
    let mut done = false;
    let mut defaults = false;
    // a key being waited for: the next one pressed (Esc gives up)
    let mut caught = false;
    if let Some((button, old)) = ui_state.waiting {
        let key = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                _ => None,
            })
        });
        if let Some(k) = key {
            if k != egui::Key::Escape {
                o.controls.assign(button, old, k);
            }
            ui_state.waiting = None;
            caught = true;
        }
    }
    let modal = egui::Modal::new(egui::Id::new("game-options")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(540.0);
        ui.label(RichText::new("Options").font(title_font(28.0)).color(Color32::WHITE));
        ui.add_space(8.0);
        // the tabs: their names, the one shown underlined
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for (i, name) in TABS.iter().enumerate() {
                let on = ui_state.tab == i;
                let text = RichText::new(*name).font(menu_font(17.0)).color(if on { Color32::WHITE } else { Color32::from_white_alpha(120) });
                let r = ui.add(egui::Label::new(text).sense(Sense::click()));
                platform::set_hook(&format!("uiTab_{name}"), &format!("{:.0},{:.0}", r.rect.center().x, r.rect.center().y));
                let line = ctx.animate_value_with_time(ui.id().with(("options-tab", i)), if on { 1.0 } else { 0.0 }, HOVER_S);
                if line > 0.01 {
                    let w = r.rect.width() * line;
                    let y = r.rect.bottom() + 3.0;
                    ui.painter().line_segment([Pos2::new(r.rect.center().x - w / 2.0, y), Pos2::new(r.rect.center().x + w / 2.0, y)], Stroke::new(2.0, Color32::WHITE));
                }
                if r.clicked() && !on {
                    ui_state.tab = i;
                    ui_state.waiting = None;
                }
            }
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        let rows = |ui: &mut egui::Ui, id: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Grid::new(("options", id)).num_columns(2).spacing([20.0, 12.0]).min_col_width(130.0).show(ui, |ui| add(ui));
        };
        let label = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).color(Color32::WHITE));
        };
        // every tab as tall as the tallest (the controls), so the dialog doesn't jump
        ui.allocate_ui(Vec2::new(ui.available_width(), 0.0), |ui| {
            ui.set_min_height(250.0);
            match ui_state.tab {
                0 => rows(ui, "game", &mut |ui| {
                    label(ui, "Text speed");
                    ui.horizontal(|ui| {
                        for (i, l) in ["Fast", "Normal", "Slow"].iter().enumerate() {
                            choose(ui, &mut o.text, i, l, "uiText");
                        }
                    });
                    ui.end_row();
                    label(ui, "Timing windows");
                    ui.horizontal(|ui| {
                        for (i, l) in ["Easy", "Normal", "Hard"].iter().enumerate() {
                            choose(ui, &mut o.difficulty, i, l, "uiTiming").on_hover_text("How much time a hit, a block or a release gives you");
                        }
                    });
                    ui.end_row();
                    label(ui, "Timing rings");
                    ui.checkbox(&mut o.assist, "Show when to press").on_hover_text("A ring that closes on the moment to press");
                    ui.end_row();
                    label(ui, "Time of day");
                    ui.label(RichText::new(format!("The real one, where you are: now {}", platform::clock_text(platform::local_hours()))).color(Color32::from_white_alpha(150)));
                    ui.end_row();
                }),
                1 => {
                    rows(ui, "graphics", &mut |ui| {
                        label(ui, "Resolution");
                        ui.horizontal(|ui| {
                            for p in [100u8, 75, 50] {
                                choose(ui, &mut o.graphics.scale, p, &format!("{p}%"), "uiResolution");
                            }
                        });
                        ui.end_row();
                        label(ui, "Shadows");
                        ui.horizontal(|ui| {
                            for (i, (name, _)) in Graphics::SHADOWS.iter().enumerate() {
                                choose(ui, &mut o.graphics.shadows, i, name, "uiShadows");
                            }
                        });
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Lower these if the game stutters.").size(12.0).color(Color32::from_white_alpha(140)));
                }
                2 => controls(ui, o, ui_state),
                _ => rows(ui, "audio", &mut |ui| {
                    for (name, value, hook, about) in [
                        ("Master", &mut o.volume, "uiVolume", "Everything"),
                        ("Effects", &mut o.effects, "uiEffects", "Cries, hits, chimes: the sounds that play once"),
                        ("Ambience", &mut o.ambience, "uiAmbience", "Wind, water, crickets, what hums: the sounds that go on"),
                    ] {
                        label(ui, name);
                        let mut v = *value as f32;
                        let s = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).step_by(5.0).fixed_decimals(0).suffix("%")).on_hover_text(about);
                        platform::set_hook(hook, &format!("{:.0},{:.0}", s.rect.center().x, s.rect.center().y));
                        if s.changed() {
                            *value = v.round() as u8;
                        }
                        ui.end_row();
                    }
                }),
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let d = ui.add(egui::Button::new(RichText::new("Done").strong().color(Color32::from_rgb(20, 24, 32))).fill(Color32::WHITE).min_size(Vec2::new(76.0, 28.0)));
            platform::set_hook("uiOptionsDone", &format!("{:.0},{:.0}", d.rect.center().x, d.rect.center().y));
            if d.clicked() {
                done = true;
            }
            let b = ui.add(egui::Button::new("Defaults").min_size(Vec2::new(76.0, 28.0))).on_hover_text(format!("This tab's options as they came ({})", TABS[ui_state.tab]));
            platform::set_hook("uiOptionsDefaults", &format!("{:.0},{:.0}", b.rect.center().x, b.rect.center().y));
            if b.clicked() {
                defaults = true;
            }
        });
    });
    if defaults {
        o.reset_part(ui_state.tab);
        ui_state.waiting = None;
    }
    // while a key is waited for (and as it's caught), the keys are the Controls'
    let listening = ui_state.waiting.is_some() || caught;
    if done || !listening && (modal.should_close() || input.pressed(Button::B) && !ctx.egui_wants_keyboard_input()) {
        *open = false;
        ui_state.waiting = None;
    }
    platform::set_hook("optionstab", TABS[ui_state.tab]);
    platform::set_hook("waitingkey", &ui_state.waiting.map(|(b, _)| b.name().to_string()).unwrap_or_default());
    o.saved() != before
}

/// The Controls tab: each button, what it does, its keys; a key clicked waits for the one to
/// put in its place, "+" for one more, a right-click takes one off.
fn controls(ui: &mut egui::Ui, o: &mut Options, ui_state: &mut OptionsUi) {
    let keys: Bindings = o.controls.clone();
    egui::Grid::new("options-controls").num_columns(3).spacing([14.0, 8.0]).show(ui, |ui| {
        for b in Button::ALL {
            ui.label(RichText::new(b.name()).size(15.0).color(Color32::WHITE));
            ui.label(RichText::new(b.about()).size(12.5).color(Color32::from_white_alpha(150)));
            ui.horizontal(|ui| {
                for (i, &k) in keys.keys(b).iter().enumerate() {
                    let waiting = ui_state.waiting == Some((b, Some(k)));
                    let text = if waiting { "Press a key…".to_string() } else { key_label(k) };
                    let chip = ui.add(egui::Button::new(RichText::new(text).size(12.5)).selected(waiting)).on_hover_text("Click: another key in its place (Esc keeps it). Right-click: take it off");
                    platform::set_hook(&format!("uiKey_{}_{i}", b.name()), &format!("{:.0},{:.0}", chip.rect.center().x, chip.rect.center().y));
                    if chip.clicked() {
                        ui_state.waiting = Some((b, Some(k)));
                    }
                    if chip.secondary_clicked() {
                        o.controls.remove(b, k);
                    }
                }
                let adding = ui_state.waiting == Some((b, None));
                let add = ui.add(egui::Button::new(RichText::new(if adding { "Press a key…" } else { "+" }).size(12.5)).selected(adding)).on_hover_text("One more key for it");
                platform::set_hook(&format!("uiKeyAdd_{}", b.name()), &format!("{:.0},{:.0}", add.rect.center().x, add.rect.center().y));
                if add.clicked() {
                    ui_state.waiting = Some((b, None));
                }
            });
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("A click on the game's view counts as A. A key given to a button leaves the one that had it.").size(12.0).color(Color32::from_white_alpha(140)));
}

/// One of an option's values, picked by a click; its place kept for the checks
/// (`<hook>_<label>`, "uiText_Slow").
fn choose<T: PartialEq>(ui: &mut egui::Ui, current: &mut T, value: T, label: &str, hook: &str) -> egui::Response {
    let r = ui.selectable_value(current, value, label);
    platform::set_hook(&format!("{hook}_{}", label.trim_end_matches('%')), &format!("{:.0},{:.0}", r.rect.center().x, r.rect.center().y));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_names_the_players_keys() {
        let mut keys = Bindings::default();
        assert_eq!(keys_hint(&keys), "Arrows: choose  ·  Z or Space: select  ·  X or Esc: back");
        // A's Z given K: the hint follows
        keys.assign(Button::A, Some(egui::Key::Z), egui::Key::K);
        assert!(keys_hint(&keys).contains("K or Space: select"), "{}", keys_hint(&keys));
        // the arrows taken off: Up's and Down's first keys
        keys.remove(Button::Up, egui::Key::ArrowUp);
        assert!(keys_hint(&keys).starts_with("W, Down arrow: choose"), "{}", keys_hint(&keys));
    }
}
