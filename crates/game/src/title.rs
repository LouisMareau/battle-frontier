//! The game's main menu, the first thing on screen: Continue (the saved story), New game,
//! Options and Exit over the world (the app draws a map behind it, its camera drifting), and
//! the Editor's icon in the bottom-right corner. And the Options, in tabs (Game, Graphics,
//! Controls, Audio), which the editor's Test battles open too; they're kept between runs.
//!
//! The words are Roboto (Bold for the titles, Medium for the items, the tabs and headings,
//! Light for the rest); every move here is a quick tween (0.3 s at most): an item hovered grows a little and
//! steps right; an item picked steps right while the others slide left and fade.
//!
//! The mouse works all of it, and so do the game's buttons, on the keyboard or a controller
//! (whichever is in use: `pads`): the hints name that device's keys or buttons.

use std::sync::Arc;

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::{Graphics, Options};
use crate::input::{Bindings, Button, Input};
use crate::pads::{Family, PadButton};
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
    /// the answer the buttons are on (0 start over, 1 keep the story)
    confirm_sel: usize,
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
            let moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
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
                // the pointer moving onto an item picks it out (resting there, it leaves the
                // buttons be)
                if r.hovered() && moved && enabled[i] && !busy {
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
            let hint = if narrow { String::new() } else { keys_hint(&crate::input::bindings(), pad_shown()) };
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
        platform::set_hook("titlesel", ITEMS[self.sel]);
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
                    1 if saved.is_some() => (self.confirm_new, self.confirm_sel) = (true, 1),
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
            match confirm_new(&ctx, saved.unwrap_or_default(), input, &mut self.confirm_sel) {
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
pub(crate) fn shadowed(p: &egui::Painter, at: Pos2, align: Align2, text: &str, font: FontId, colour: Color32) -> Rect {
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
/// The mouse answers, or the buttons: Left and Right (Up and Down) go between the answers, a
/// ring round the one they're on ("Keep my story" first), A gives it, B keeps the story.
fn confirm_new(ctx: &egui::Context, saved: &str, input: &Input, sel: &mut usize) -> Option<bool> {
    let mut answer = None;
    if input.pressed(Button::Left) || input.pressed(Button::Up) {
        *sel = 0;
    }
    if input.pressed(Button::Right) || input.pressed(Button::Down) {
        *sel = 1;
    }
    let moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
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
            // the pointer moving onto an answer puts the buttons there too
            if moved && yes.hovered() {
                *sel = 0;
            }
            if moved && no.hovered() {
                *sel = 1;
            }
            ring(ui, if *sel == 0 { yes.rect } else { no.rect });
        });
    });
    if answer.is_none() && input.pressed(Button::A) {
        answer = Some(*sel == 0);
    }
    if answer.is_none() && (modal.should_close() || input.pressed(Button::B)) {
        answer = Some(false);
    }
    platform::set_hook("confirmsel", if *sel == 0 { "yes" } else { "no" });
    answer
}

/// A dialog's look: the panels' blue, rounded.
pub fn dialog_frame() -> egui::Frame {
    egui::Frame::new().fill(Color32::from_rgb(20, 27, 41)).corner_radius(14.0).inner_margin(22.0).stroke(Stroke::new(1.0, Color32::from_white_alpha(30)))
}

// ---------------------------------------------------------------- the Options

/// The Options' tabs.
const TABS: [&str; 4] = ["Game", "Graphics", "Controls", "Audio"];

/// The Options' own state: the tab shown, a key or a controller's button being waited for
/// (Controls), where the game's buttons are.
#[derive(Default)]
pub struct OptionsUi {
    pub tab: usize,
    /// a button whose key is being changed (the key it replaces, or a new one): the next key
    /// pressed is it
    waiting: Option<(Button, Option<egui::Key>)>,
    /// the same for its controller buttons: the next one pressed on the controller
    waiting_pad: Option<(Button, Option<PadButton>)>,
    /// where the game's buttons are: a row of the tab (the last: Done and Defaults), a place
    /// along it
    row: usize,
    col: usize,
    /// the ring round that place shown (the buttons were used; the mouse moving hides it)
    ring: bool,
    /// the frame the window was last drawn (opened again, the buttons start at the top)
    last_frame: u64,
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

/// A button's first `n` keys, or its controller buttons as a controller of `pad`'s kind has
/// them printed, joined ("Z or Space").
fn names(keys: &Bindings, pad: Option<Family>, b: Button, n: usize, join: &str) -> String {
    match pad {
        Some(f) => keys.pads(b).iter().take(n).map(|p| p.label(f).to_string()).collect::<Vec<_>>().join(join),
        None => keys.keys(b).iter().take(n).map(|&k| key_label(k)).collect::<Vec<_>>().join(join),
    }
}

/// "Arrows" (or "D-pad") when those buttons have them, else their first keys ("W, S").
fn along(keys: &Bindings, pad: Option<Family>, buttons: &[Button]) -> String {
    let on_them = buttons.iter().all(|&b| match (pad, b) {
        (Some(_), Button::Up) => keys.pads(b).contains(&PadButton::DPadUp),
        (Some(_), Button::Down) => keys.pads(b).contains(&PadButton::DPadDown),
        (Some(_), Button::Left) => keys.pads(b).contains(&PadButton::DPadLeft),
        (Some(_), Button::Right) => keys.pads(b).contains(&PadButton::DPadRight),
        (None, Button::Up) => keys.keys(b).contains(&egui::Key::ArrowUp),
        (None, Button::Down) => keys.keys(b).contains(&egui::Key::ArrowDown),
        (None, Button::Left) => keys.keys(b).contains(&egui::Key::ArrowLeft),
        (None, Button::Right) => keys.keys(b).contains(&egui::Key::ArrowRight),
        _ => false,
    });
    if on_them {
        return if pad.is_some() { "D-pad" } else { "Arrows" }.to_string();
    }
    let firsts: Vec<String> = buttons.iter().map(|&b| names(keys, pad, b, 1, "")).collect();
    if firsts.iter().any(String::is_empty) {
        String::new()
    } else {
        firsts.join(", ")
    }
}

/// The menu's buttons at its foot, from the player's keys or, with `pad`, the controller's
/// buttons: the arrows or the d-pad (else Up's and Down's first) choose, A's first two
/// select, B's first two go back.
fn keys_hint(keys: &Bindings, pad: Option<Family>) -> String {
    [(along(keys, pad, &[Button::Up, Button::Down]), "choose"), (names(keys, pad, Button::A, 2, " or "), "select"), (names(keys, pad, Button::B, 2, " or "), "back")]
        .into_iter()
        .filter(|(keys, _)| !keys.is_empty())
        .map(|(keys, does)| format!("{keys}: {does}"))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// Each of the game's buttons and what it is on the device listened to ("A: Z / Space /
/// Enter · B: X / Esc / Backspace · d-pad: arrows · L / R: Q / E").
pub fn buttons_text(keys: &Bindings, pad: Option<Family>) -> String {
    let dirs = along(keys, pad, &[Button::Up, Button::Down, Button::Left, Button::Right]).to_lowercase();
    let dirs = if pad.is_some() && dirs == "d-pad" { "d-pad, left stick".to_string() } else { dirs };
    format!(
        "A: {} · B: {} · d-pad: {dirs} · L / R: {} / {}",
        names(keys, pad, Button::A, 9, " / "),
        names(keys, pad, Button::B, 9, " / "),
        names(keys, pad, Button::L, 1, ""),
        names(keys, pad, Button::R, 1, "")
    )
}

/// Two buttons' first keys (or controller buttons), "Q / E" or "LB / RB".
pub fn pair(keys: &Bindings, pad: Option<Family>, a: Button, b: Button) -> String {
    format!("{} / {}", names(keys, pad, a, 1, ""), names(keys, pad, b, 1, ""))
}

/// What the player presses for one of the game's buttons on the device in use: its first key
/// ("Z", "Esc") or, with a controller in hand, its first button as printed on that controller
/// ("A", "Cross"). The prompts name it: "Press Z at impact".
pub fn button_label(b: Button) -> String {
    label(&crate::input::bindings(), pad_shown(), b)
}

/// `b`'s first key, or its first controller button as a controller of `pad`'s kind has it
/// printed; the game's own name for it when nothing is on it.
fn label(keys: &Bindings, pad: Option<Family>, b: Button) -> String {
    let first = names(keys, pad, b, 1, "");
    if first.is_empty() {
        b.name().to_string()
    } else {
        first
    }
}

/// The overworld's buttons, top left under the map's name: what moves, what speaks, what
/// puts the ring on the HUD's buttons (Esc, or the controller's Start).
pub fn walk_hint(keys: &Bindings, pad: Option<Family>) -> String {
    let moves = along(keys, pad, &[Button::Up, Button::Down, Button::Left, Button::Right]);
    let menu = pad.map_or("Esc", |f| PadButton::Start.label(f));
    let verb = if moves == "D-pad" { "moves" } else { "move" };
    format!("{moves} {verb}  ·  {} speaks  ·  {menu}: the buttons", names(keys, pad, Button::A, 1, ""))
}

/// The HUD's buttons while the ring is on them: what moves it, what presses, what goes back.
pub fn hud_hint(keys: &Bindings, pad: Option<Family>) -> String {
    let one = |b: Button| names(keys, pad, b, 1, "");
    format!("{}: choose  ·  {}: press  ·  {}: back", along(keys, pad, &[Button::Up, Button::Down, Button::Left, Button::Right]), one(Button::A), one(Button::B))
}

/// The Options' buttons, as the device listened to has them.
fn options_hint(keys: &Bindings, pad: Option<Family>) -> String {
    let one = |b: Button| names(keys, pad, b, 1, "");
    format!("{}, {}: tabs  ·  {}: move  ·  {}: press  ·  {}: close", one(Button::L), one(Button::R), along(keys, pad, &[Button::Up, Button::Down, Button::Left, Button::Right]), one(Button::A), one(Button::B))
}

/// The controller's kind when one is listened to (its labels name the buttons), else None.
pub fn pad_shown() -> Option<Family> {
    crate::pads::on_pad().then(crate::pads::family)
}

/// What a row of the Options does with the game's buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Row {
    /// one of a few values: Left and Right go through them
    Pick,
    /// on or off: A, Left or Right turns it
    Toggle,
    /// a level: Left and Right by 5
    Level,
    /// a button's keys and controller buttons: Left and Right go along them, A changes one
    Keys(Button),
    /// Done and Defaults
    End,
}

/// A tab's rows, top to bottom.
fn tab_rows(tab: usize) -> Vec<Row> {
    let mut rows = match tab {
        0 => vec![Row::Pick, Row::Pick, Row::Toggle],
        1 => vec![Row::Pick, Row::Pick, Row::Pick, Row::Pick],
        2 => Button::ALL.iter().map(|&b| Row::Keys(b)).collect(),
        _ => vec![Row::Level, Row::Level, Row::Level],
    };
    rows.push(Row::End);
    rows
}

/// The places along a row: a button's keys and "+" (its controller buttons and "+" in the
/// controller's view); Done and Defaults; else the row as one.
fn row_places(row: Row, keys: &Bindings, pad_view: bool) -> usize {
    match row {
        Row::Keys(b) if pad_view => keys.pads(b).len() + 1,
        Row::Keys(b) => keys.keys(b).len() + 1,
        Row::End => 2,
        _ => 1,
    }
}

/// Left (-1) or Right (+1) on an option's row: its next value that way (a toggle turns).
fn nudge(o: &mut Options, tab: usize, row: usize, by: i32) {
    let step = |v: usize, n: usize| (v as i32 + by).clamp(0, n as i32 - 1) as usize;
    match (tab, row) {
        (0, 0) => o.text = step(o.text, 3),
        (0, 1) => o.difficulty = step(o.difficulty, 3),
        (0, 2) => o.assist = !o.assist,
        (1, 0) => {
            let scales = [100u8, 75, 50];
            let i = scales.iter().position(|&s| s == o.graphics.scale).unwrap_or(0);
            o.graphics.scale = scales[step(i, scales.len())];
        }
        (1, 1) => o.graphics.shadows = step(o.graphics.shadows, Graphics::SHADOWS.len()),
        (1, 2) => o.graphics.pixels = step(o.graphics.pixels, Graphics::HANDHELDS.len()),
        (1, 3) => o.graphics.lcd = step(o.graphics.lcd, Graphics::HANDHELDS.len()),
        (3, r) => {
            let level = match r {
                0 => &mut o.volume,
                1 => &mut o.effects,
                _ => &mut o.ambience,
            };
            *level = (*level as i32 + by * 5).clamp(0, 100) as u8;
        }
        _ => {}
    }
}

/// The game's buttons in the Options: L and R go through the tabs, Up and Down through the
/// rows (Done and Defaults last), Left and Right change a row's value or go along it, A
/// presses what they're on. `pad_view`: a controller is in use (the Controls show its
/// buttons); without one, the first press only shows where they are.
fn buttons_in_options(o: &mut Options, ui_state: &mut OptionsUi, input: &Input, pad_view: bool, done: &mut bool, defaults: &mut bool) {
    let tabs_by = input.pressed(Button::R) as i32 - input.pressed(Button::L) as i32;
    if tabs_by != 0 {
        ui_state.tab = (ui_state.tab as i32 + tabs_by).rem_euclid(TABS.len() as i32) as usize;
        (ui_state.row, ui_state.col, ui_state.ring) = (0, 0, true);
        return;
    }
    if ![Button::Up, Button::Down, Button::Left, Button::Right, Button::A].iter().any(|&b| input.pressed(b)) {
        return;
    }
    if !(ui_state.ring || pad_view) {
        ui_state.ring = true;
        return;
    }
    ui_state.ring = true;
    let rows = tab_rows(ui_state.tab);
    let n = rows.len();
    let mut row = ui_state.row.min(n - 1);
    if input.pressed(Button::Up) {
        row = (row + n - 1) % n;
    }
    if input.pressed(Button::Down) {
        row = (row + 1) % n;
    }
    ui_state.row = row;
    let places = row_places(rows[row], &o.controls, pad_view);
    ui_state.col = ui_state.col.min(places - 1);
    let by = input.pressed(Button::Right) as i32 - input.pressed(Button::Left) as i32;
    if by != 0 {
        match rows[row] {
            Row::Keys(_) | Row::End => ui_state.col = (ui_state.col as i32 + by).clamp(0, places as i32 - 1) as usize,
            _ => nudge(o, ui_state.tab, row, by),
        }
    }
    if input.pressed(Button::A) {
        match rows[row] {
            Row::Toggle => nudge(o, ui_state.tab, row, 1),
            // the one it's on waits for its new key or button ("+": one more)
            Row::Keys(b) if pad_view => ui_state.waiting_pad = Some((b, o.controls.pads(b).get(ui_state.col).copied())),
            Row::Keys(b) => ui_state.waiting = Some((b, o.controls.keys(b).get(ui_state.col).copied())),
            Row::End if ui_state.col == 0 => *done = true,
            Row::End => *defaults = true,
            _ => {}
        }
    }
}

/// A ring round what the buttons are on.
fn ring(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_stroke(rect.expand(4.0), 7.0, Stroke::new(1.5, Color32::from_white_alpha(230)), egui::StrokeKind::Outside);
}

/// The Options, a dialog over the screen, in tabs: true when an option changed. Done, B or a
/// click beside it closes it. The mouse works it, and so do the game's buttons, on the
/// keyboard or a controller (`buttons_in_options`), a ring round what they're on.
pub fn options_window(ctx: &egui::Context, o: &mut Options, open: &mut bool, ui_state: &mut OptionsUi, input: &Input) -> bool {
    let before = o.saved();
    let mut done = false;
    let mut defaults = false;
    // opened again: the buttons start at the top
    let frame_nr = ctx.cumulative_frame_nr();
    if frame_nr > ui_state.last_frame + 1 {
        (ui_state.row, ui_state.col, ui_state.ring) = (0, 0, false);
    }
    ui_state.last_frame = frame_nr;
    // the device in use: the Controls show its keys or buttons
    let pad_view = crate::pads::on_pad();
    // a key or a controller's button being waited for: the next one pressed (Delete or the
    // controller's Select takes the one waited on off, Esc or its Start keeps it). The other
    // device used meanwhile: the wait is over (its chip isn't shown any more).
    let mut caught = false;
    if (pad_view && ui_state.waiting.take().is_some()) || (!pad_view && ui_state.waiting_pad.take().is_some()) {
        caught = true;
    }
    if let Some((button, old)) = ui_state.waiting {
        let key = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                _ => None,
            })
        });
        if let Some(k) = key {
            match (k, old) {
                (egui::Key::Escape, _) | (egui::Key::Delete, None) => {}
                (egui::Key::Delete, Some(old)) => o.controls.remove(button, old),
                (k, old) => o.controls.assign(button, old, k),
            }
            ui_state.waiting = None;
            caught = true;
        }
    }
    if let Some((button, old)) = ui_state.waiting_pad {
        if let Some(&p) = crate::pads::raw_pressed().first() {
            match (p, old) {
                (PadButton::Start, _) | (PadButton::Select, None) => {}
                (PadButton::Select, Some(old)) => o.controls.remove_pad(button, old),
                (p, old) => o.controls.assign_pad(button, old, p),
            }
            ui_state.waiting_pad = None;
            caught = true;
        }
    }
    // while one is waited for (and as it's caught), the keys and buttons are the Controls'
    let listening = ui_state.waiting.is_some() || ui_state.waiting_pad.is_some() || caught;
    // the ring: always with a controller in hand, else once the buttons are used, until the
    // mouse moves
    if ctx.input(|i| i.pointer.delta().length() > 2.0) {
        ui_state.ring = false;
    }
    if !listening {
        buttons_in_options(o, ui_state, input, pad_view, &mut done, &mut defaults);
    }
    let ring_on = ui_state.ring || pad_view;
    // where each row's places are, for the ring
    let mut spots: Vec<Vec<Rect>> = vec![];
    let modal = egui::Modal::new(egui::Id::new("game-options")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(600.0);
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
                    (ui_state.row, ui_state.col) = (0, 0);
                    ui_state.waiting = None;
                    ui_state.waiting_pad = None;
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
            ui.set_min_height(300.0);
            match ui_state.tab {
                0 => rows(ui, "game", &mut |ui| {
                    label(ui, "Text speed");
                    let r = ui.horizontal(|ui| {
                        for (i, l) in ["Fast", "Normal", "Slow"].iter().enumerate() {
                            choose(ui, &mut o.text, i, l, "uiText");
                        }
                    });
                    spots.push(vec![r.response.rect]);
                    ui.end_row();
                    label(ui, "Timing windows");
                    let r = ui.horizontal(|ui| {
                        for (i, l) in ["Easy", "Normal", "Hard"].iter().enumerate() {
                            choose(ui, &mut o.difficulty, i, l, "uiTiming").on_hover_text("How much time a hit, a block or a release gives you");
                        }
                    });
                    spots.push(vec![r.response.rect]);
                    ui.end_row();
                    label(ui, "Timing rings");
                    let r = ui.checkbox(&mut o.assist, "Show when to press").on_hover_text("A ring that closes on the moment to press");
                    spots.push(vec![r.rect]);
                    ui.end_row();
                    label(ui, "Time of day");
                    ui.label(RichText::new(format!("The real one, where you are: now {}", platform::clock_text(platform::local_hours()))).color(Color32::from_white_alpha(150)));
                    ui.end_row();
                }),
                1 => {
                    rows(ui, "graphics", &mut |ui| {
                        label(ui, "Resolution");
                        // the old handhelds' pixels set it themselves
                        let own = o.graphics.pixels == 0;
                        let r = ui.add_enabled_ui(own, |ui| {
                            ui.horizontal(|ui| {
                                for p in [100u8, 75, 50] {
                                    choose(ui, &mut o.graphics.scale, p, &format!("{p}%"), "uiResolution");
                                }
                            })
                        });
                        let r = if own { r.response } else { r.response.on_disabled_hover_text("The pixels set it") };
                        spots.push(vec![r.rect]);
                        ui.end_row();
                        label(ui, "Shadows");
                        let r = ui.horizontal(|ui| {
                            for (i, (name, _)) in Graphics::SHADOWS.iter().enumerate() {
                                choose(ui, &mut o.graphics.shadows, i, name, "uiShadows");
                            }
                        });
                        spots.push(vec![r.response.rect]);
                        ui.end_row();
                        label(ui, "Pixels");
                        let r = ui.horizontal(|ui| {
                            for (i, name) in Graphics::HANDHELDS.iter().enumerate() {
                                let about = match i {
                                    0 => "The world at the screen's own resolution",
                                    1 => "The world in big pixels, about as many as the GBA's screen had (240 by 160), in its 15-bit colours",
                                    _ => "The world in bigger pixels, about as many as the GBC's screen had (160 by 144), in its 15-bit colours",
                                };
                                choose(ui, &mut o.graphics.pixels, i, name, "uiPixels").on_hover_text(about);
                            }
                        });
                        spots.push(vec![r.response.rect]);
                        ui.end_row();
                        label(ui, "LCD screen");
                        let r = ui.horizontal(|ui| {
                            for (i, name) in Graphics::HANDHELDS.iter().enumerate() {
                                let about = match i {
                                    0 => "The colours as they are",
                                    1 => "The colours as the GBA's screen showed them: dimmer, muted, mixed (with Pixels on, the thin gaps between its pixels too)",
                                    _ => "The colours as the GBC's screen mixed them (with Pixels on, the gaps between its pixels too)",
                                };
                                choose(ui, &mut o.graphics.lcd, i, name, "uiLcd").on_hover_text(about);
                            }
                        });
                        spots.push(vec![r.response.rect]);
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Lower the resolution or the shadows if the game stutters. Pixels and LCD screen are the old handhelds' looks.").size(12.0).color(Color32::from_white_alpha(140)));
                }
                2 => spots.extend(controls(ui, o, ui_state, pad_view)),
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
                        spots.push(vec![s.rect]);
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
            spots.push(vec![d.rect, b.rect]);
            // the buttons' words, while they're the ones in use
            if ring_on {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(options_hint(&crate::input::bindings(), pad_shown())).size(12.0).color(Color32::from_white_alpha(140)));
                });
            }
        });
        if ring_on {
            if let Some(r) = spots.get(ui_state.row).and_then(|s| s.get(ui_state.col).or(s.first())) {
                ring(ui, *r);
            }
        }
    });
    if defaults {
        o.reset_part(ui_state.tab);
        ui_state.waiting = None;
        ui_state.waiting_pad = None;
    }
    if done || !listening && (modal.should_close() || input.pressed(Button::B) && !ctx.egui_wants_keyboard_input()) {
        *open = false;
        ui_state.waiting = None;
        ui_state.waiting_pad = None;
    }
    platform::set_hook("optionstab", TABS[ui_state.tab]);
    platform::set_hook("waitingkey", &ui_state.waiting.map(|(b, _)| b.name().to_string()).unwrap_or_default());
    platform::set_hook("waitingpad", &ui_state.waiting_pad.map(|(b, _)| b.name().to_string()).unwrap_or_default());
    platform::set_hook("optionsring", &if ring_on { format!("{},{}", ui_state.row, ui_state.col) } else { String::new() });
    o.saved() != before
}

/// The Controls tab, for the device in use: a controller's buttons while one is, else the
/// keyboard's keys. Each of the game's buttons, what it does and what's on it: one clicked (or
/// A on it) waits for the one to put in its place, "+" for one more; while one waits, a
/// controller's Select (the Delete key) takes it off and its Start (Esc) keeps it; a
/// right-click takes one off too. The places of each row's chips, for the ring.
fn controls(ui: &mut egui::Ui, o: &mut Options, ui_state: &mut OptionsUi, pad_view: bool) -> Vec<Vec<Rect>> {
    let keys: Bindings = o.controls.clone();
    let family = crate::pads::family();
    let mut spots = vec![];
    // whose buttons these are; the other device's show while it's the one in use
    ui.horizontal(|ui| {
        let (device, other) = match (pad_view, crate::pads::count()) {
            (true, _) => (crate::pads::name(), "the keyboard's keys show while it's the one in use"),
            (false, 0) => ("Keyboard".to_string(), "connect a controller to see and change its buttons"),
            (false, _) => ("Keyboard".to_string(), "the controller's buttons show once one of them is pressed"),
        };
        ui.label(RichText::new(device).font(menu_font(15.0)).color(Color32::WHITE));
        ui.label(RichText::new(other).size(12.0).color(Color32::from_white_alpha(120)));
    });
    platform::set_hook("controlsview", if pad_view { "controller" } else { "keyboard" });
    ui.add_space(8.0);
    let chip = |ui: &mut egui::Ui, text: String, waiting: bool| ui.add(egui::Button::new(RichText::new(text).size(13.0)).selected(waiting).min_size(Vec2::new(26.0, 0.0)));
    let place = |r: Rect| format!("{:.0},{:.0}", r.center().x, r.center().y);
    egui::Grid::new("options-controls").num_columns(2).spacing([28.0, 8.0]).show(ui, |ui| {
        for b in Button::ALL {
            let mut row = vec![];
            ui.horizontal(|ui| {
                ui.add_sized(Vec2::new(44.0, 18.0), egui::Label::new(RichText::new(b.name()).size(15.0).color(Color32::WHITE)));
                // what it does, unless its name says it (the d-pad's)
                if b.about() != b.name() {
                    ui.label(RichText::new(b.about()).size(12.0).color(Color32::from_white_alpha(140)));
                }
            });
            ui.horizontal(|ui| {
                if pad_view {
                    for (i, &p) in keys.pads(b).iter().enumerate() {
                        let waiting = ui_state.waiting_pad == Some((b, Some(p)));
                        let c = chip(ui, if waiting { "Press a button…".to_string() } else { p.label(family).to_string() }, waiting);
                        platform::set_hook(&format!("uiPad_{}_{i}", b.name()), &place(c.rect));
                        if c.clicked() {
                            ui_state.waiting_pad = Some((b, Some(p)));
                        }
                        if c.secondary_clicked() {
                            o.controls.remove_pad(b, p);
                        }
                        row.push(c.rect);
                    }
                    let adding = ui_state.waiting_pad == Some((b, None));
                    let add = chip(ui, if adding { "Press a button…" } else { "+" }.to_string(), adding);
                    platform::set_hook(&format!("uiPadAdd_{}", b.name()), &place(add.rect));
                    if add.clicked() {
                        ui_state.waiting_pad = Some((b, None));
                    }
                    row.push(add.rect);
                } else {
                    for (i, &k) in keys.keys(b).iter().enumerate() {
                        let waiting = ui_state.waiting == Some((b, Some(k)));
                        let c = chip(ui, if waiting { "Press a key…".to_string() } else { key_label(k) }, waiting);
                        platform::set_hook(&format!("uiKey_{}_{i}", b.name()), &place(c.rect));
                        if c.clicked() {
                            ui_state.waiting = Some((b, Some(k)));
                        }
                        if c.secondary_clicked() {
                            o.controls.remove(b, k);
                        }
                        row.push(c.rect);
                    }
                    let adding = ui_state.waiting == Some((b, None));
                    let add = chip(ui, if adding { "Press a key…" } else { "+" }.to_string(), adding);
                    platform::set_hook(&format!("uiKeyAdd_{}", b.name()), &place(add.rect));
                    if add.clicked() {
                        ui_state.waiting = Some((b, None));
                    }
                    row.push(add.rect);
                }
            });
            ui.end_row();
            spots.push(row);
        }
    });
    ui.add_space(8.0);
    // what to do here; while one waits, how to take it off or keep it
    let (select, start) = (PadButton::Select.label(family), PadButton::Start.label(family));
    let note = match (pad_view, ui_state.waiting_pad, ui_state.waiting) {
        (true, Some((_, Some(_))), _) => format!("Press the new button  ·  {select}: take it off  ·  {start}: keep it"),
        (true, Some((_, None)), _) => format!("Press the button to add  ·  {start}: none"),
        (false, _, Some((_, Some(_)))) => "Press the new key  ·  Delete: take it off  ·  Esc: keep it".to_string(),
        (false, _, Some((_, None))) => "Press the key to add  ·  Esc: none".to_string(),
        (true, ..) => format!("{} on a button, then the new one. The left stick moves like the d-pad.", label(&keys, Some(family), Button::A)),
        (false, ..) => format!("Click a key (or {} on it), then the new one. A click on the game's view counts as A.", label(&keys, None, Button::A)),
    };
    ui.label(RichText::new(note).size(12.0).color(Color32::from_white_alpha(150)));
    spots
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

    use crate::input::Press;

    #[test]
    fn the_hint_names_the_players_keys() {
        let mut keys = Bindings::default();
        assert_eq!(keys_hint(&keys, None), "Arrows: choose  ·  Z or Space: select  ·  X or Esc: back");
        // a controller in hand: its buttons, as printed on it
        assert_eq!(keys_hint(&keys, Some(Family::Xbox)), "D-pad: choose  ·  A: select  ·  B: back");
        assert_eq!(keys_hint(&keys, Some(Family::PlayStation)), "D-pad: choose  ·  Cross: select  ·  Circle: back");
        // A's Z given K: the hint follows
        keys.assign(Button::A, Some(egui::Key::Z), egui::Key::K);
        assert!(keys_hint(&keys, None).contains("K or Space: select"), "{}", keys_hint(&keys, None));
        // the arrows taken off: Up's and Down's first keys
        keys.remove(Button::Up, egui::Key::ArrowUp);
        assert!(keys_hint(&keys, None).starts_with("W, Down arrow: choose"), "{}", keys_hint(&keys, None));
        // the controller's A on North: Y selects
        keys.assign_pad(Button::A, Some(PadButton::South), PadButton::North);
        assert!(keys_hint(&keys, Some(Family::Xbox)).contains("Y: select"), "{}", keys_hint(&keys, Some(Family::Xbox)));
        // the Test battles' line and the overworld's
        let d = Bindings::default();
        assert_eq!(buttons_text(&d, None), "A: Z / Space / Enter · B: X / Esc / Backspace · d-pad: arrows · L / R: Q / E");
        assert_eq!(buttons_text(&d, Some(Family::Xbox)), "A: A · B: B · d-pad: d-pad, left stick · L / R: LB / RB");
        assert_eq!(walk_hint(&d, None), "Arrows move  ·  Z speaks  ·  Esc: the buttons");
        assert_eq!(walk_hint(&d, Some(Family::PlayStation)), "D-pad moves  ·  Cross speaks  ·  Options: the buttons");
        // the ring on the HUD's buttons
        assert_eq!(hud_hint(&d, None), "Arrows: choose  ·  Z: press  ·  X: back");
        assert_eq!(hud_hint(&d, Some(Family::Xbox)), "D-pad: choose  ·  A: press  ·  B: back");
    }

    #[test]
    fn a_button_is_named_as_the_player_presses_it() {
        let mut keys = Bindings::default();
        // the keyboard: each button's first key
        assert_eq!(label(&keys, None, Button::A), "Z");
        assert_eq!(label(&keys, None, Button::B), "X");
        assert_eq!(label(&keys, None, Button::L), "Q");
        assert_eq!(label(&keys, None, Button::R), "E");
        // A's Z given K: K; K taken off: Space, the next one; B's X given Esc (B's own): Esc
        keys.assign(Button::A, Some(egui::Key::Z), egui::Key::K);
        assert_eq!(label(&keys, None, Button::A), "K");
        keys.remove(Button::A, egui::Key::K);
        assert_eq!(label(&keys, None, Button::A), "Space");
        keys.assign(Button::B, Some(egui::Key::X), egui::Key::Escape);
        assert_eq!(label(&keys, None, Button::B), "Esc");
        // a controller: its button as printed on it
        let d = Bindings::default();
        assert_eq!(label(&d, Some(Family::Xbox), Button::A), "A");
        assert_eq!(label(&d, Some(Family::Xbox), Button::B), "B");
        assert_eq!(label(&d, Some(Family::Xbox), Button::L), "LB");
        assert_eq!(label(&d, Some(Family::PlayStation), Button::A), "Cross");
        assert_eq!(label(&d, Some(Family::PlayStation), Button::B), "Circle");
        assert_eq!(label(&d, Some(Family::PlayStation), Button::R), "R1");
        assert_eq!(label(&d, Some(Family::RightA), Button::A), "B", "A on the right: the game's A (South) is printed B");
        // A given North on the controller: Y, Triangle; East given to A too, B has nothing on
        // the controller: its own name
        let mut pads = Bindings::default();
        pads.assign_pad(Button::A, Some(PadButton::South), PadButton::North);
        assert_eq!(label(&pads, Some(Family::Xbox), Button::A), "Y");
        assert_eq!(label(&pads, Some(Family::PlayStation), Button::A), "Triangle");
        pads.assign_pad(Button::A, None, PadButton::East);
        assert_eq!(label(&pads, Some(Family::Xbox), Button::B), "B");
        assert_eq!(label(&pads, None, Button::A), "Z", "the keyboard's keys stay as they were");
        // in use: the player's keys (no test sets other ones), no controller listened to
        assert_eq!(button_label(Button::A), "Z");
        assert_eq!(button_label(Button::B), "X");
    }

    #[test]
    fn the_options_answer_the_buttons() {
        let press = |b: Button| Input { presses: vec![Press { button: b, down: true, time: 0.0 }], a_held: false };
        let mut o = Options::default();
        let mut s = OptionsUi::default();
        let (mut done, mut defaults) = (false, false);
        let mut go = |o: &mut Options, s: &mut OptionsUi, b: Button, pad: bool| buttons_in_options(o, s, &press(b), pad, &mut done, &mut defaults);
        // the keyboard: the first press only shows where the buttons are
        go(&mut o, &mut s, Button::Down, false);
        assert!(s.ring);
        assert_eq!((s.row, o.text), (0, 1));
        // Right: the text slower; Down twice, A: the rings off
        go(&mut o, &mut s, Button::Right, false);
        assert_eq!(o.text, 2);
        go(&mut o, &mut s, Button::Down, false);
        go(&mut o, &mut s, Button::Down, false);
        go(&mut o, &mut s, Button::A, false);
        assert_eq!(s.row, 2);
        assert!(!o.assist);
        // R: the Graphics tab, from its top; Down, Left: the shadows a step lower
        go(&mut o, &mut s, Button::R, false);
        assert_eq!((s.tab, s.row), (1, 0));
        let shadows = o.graphics.shadows;
        go(&mut o, &mut s, Button::Down, false);
        go(&mut o, &mut s, Button::Left, false);
        assert_eq!(o.graphics.shadows, shadows.saturating_sub(1));
        // Down, Right: the GBA's pixels; Down, Right twice: the GBC's LCD (no further)
        go(&mut o, &mut s, Button::Down, false);
        go(&mut o, &mut s, Button::Right, false);
        assert_eq!(o.graphics.pixels, 1);
        go(&mut o, &mut s, Button::Down, false);
        for _ in 0..3 {
            go(&mut o, &mut s, Button::Right, false);
        }
        assert_eq!((s.row, o.graphics.lcd), (3, 2));
        assert_eq!(o.graphics.retro(), render::scene::Retro { pixels: Some(render::scene::Handheld::Gba), lcd: Some(render::scene::Handheld::Gbc) });
        // L twice: round to Audio; Left: the master volume 5 lower
        go(&mut o, &mut s, Button::L, false);
        go(&mut o, &mut s, Button::L, false);
        assert_eq!(s.tab, 3);
        go(&mut o, &mut s, Button::Left, false);
        assert_eq!(o.volume, 95);
        // L: the Controls, the keyboard's: along A's keys (Z, Space, Enter, "+"), A on Space
        go(&mut o, &mut s, Button::L, false);
        assert_eq!(s.tab, 2);
        for _ in 0..5 {
            go(&mut o, &mut s, Button::Right, false);
        }
        assert_eq!(s.col, 3, "no further than the \"+\"");
        go(&mut o, &mut s, Button::A, false);
        assert_eq!(s.waiting, Some((Button::A, None)), "\"+\": one more key");
        s.waiting = None;
        go(&mut o, &mut s, Button::Left, false);
        go(&mut o, &mut s, Button::Left, false);
        go(&mut o, &mut s, Button::A, false);
        assert_eq!(s.waiting, Some((Button::A, Some(egui::Key::Space))));
        s.waiting = None;
        // a controller in use: its buttons (South, "+"); the place kept within them
        go(&mut o, &mut s, Button::Right, true);
        assert_eq!(s.col, 1, "South, then the \"+\"");
        go(&mut o, &mut s, Button::Left, true);
        go(&mut o, &mut s, Button::A, true);
        assert_eq!(s.waiting_pad, Some((Button::A, Some(PadButton::South))));
        assert_eq!(s.waiting, None);
        s.waiting_pad = None;
        // up from the top row: the last (Done, Defaults); Right: Defaults; Left, A: Done
        go(&mut o, &mut s, Button::Up, true);
        assert_eq!(s.row, Button::ALL.len());
        go(&mut o, &mut s, Button::Right, true);
        assert_eq!(s.col, 1);
        go(&mut o, &mut s, Button::Left, true);
        go(&mut o, &mut s, Button::A, true);
        assert!(done);
        assert!(!defaults);
    }
}

