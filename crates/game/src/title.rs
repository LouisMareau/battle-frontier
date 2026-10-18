//! The game's main menu, the first thing on screen: Continue (the saved story), New game,
//! Options and Exit, the game's emblem beside them (the app icon's: a sunset over a ridge in a
//! gold ring), and the Editor in the bottom-right corner. And the Options window, which the
//! editor's Test battles open too; the options are kept between runs.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::{Graphics, Options, TimeChoice};
use crate::hud::{ACCENT, MUTED, TEXT};
use crate::input::{Button, Input};
use crate::platform;
use rules::config::Period;

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

/// The night blue behind everything, top and bottom (the icon's).
const SKY_TOP: Color32 = Color32::from_rgb(28, 38, 66);
const SKY_BOTTOM: Color32 = Color32::from_rgb(12, 17, 28);

#[derive(Default)]
pub struct Title {
    /// the item the keys are on
    sel: usize,
    /// asking whether to give up the saved story for a new game
    confirm_new: bool,
    pub options_open: bool,
    /// when Exit was pressed in the browser (a page may not close its tab: a note then)
    exit_tried: Option<f64>,
    emblem: Option<egui::TextureHandle>,
}

impl Title {
    /// The menu, full window; what the player picked. `saved`: the saved story's place (its
    /// map's name), when there is one. `options_changed` is set when an option changed.
    pub fn ui(&mut self, ui: &mut egui::Ui, input: &Input, now: f64, saved: Option<&str>, options: &mut Options, options_changed: &mut bool) -> Option<Choice> {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let mut picked = None;
        let enabled = [saved.is_some(), true, true, true];
        if !enabled[self.sel] {
            self.sel = 1;
        }
        let busy = self.confirm_new || self.options_open;
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            let p = ui.painter().clone();
            backdrop(&p, screen);
            // the emblem: to the right, or above everything on a narrow screen
            let narrow = screen.width() < 820.0;
            let size = if narrow { (screen.width() * 0.5).min(screen.height() * 0.28) } else { (screen.height() * 0.62).min(screen.width() * 0.42) };
            let centre = if narrow { Pos2::new(screen.center().x, screen.top() + screen.height() * 0.2) } else { Pos2::new(screen.left() + screen.width() * 0.7, screen.top() + screen.height() * 0.47) };
            let emblem = self.emblem.get_or_insert_with(|| ctx.load_texture("title-emblem", emblem(640), egui::TextureOptions::LINEAR)).id();
            p.image(emblem, Rect::from_center_size(centre, Vec2::splat(size)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            // the title, and the items under it
            let left = if narrow { screen.center().x } else { screen.left() + (screen.width() * 0.09).max(48.0) };
            let align = if narrow { Align2::CENTER_TOP } else { Align2::LEFT_TOP };
            let top = if narrow { screen.top() + screen.height() * 0.38 } else { screen.top() + screen.height() * 0.22 };
            let title_size = if narrow { 40.0 } else { 64.0 };
            p.text(Pos2::new(left, top), align, "Battle Frontier", FontId::proportional(title_size), TEXT);
            let rule_w = if narrow { 120.0 } else { 160.0 };
            let rule_x = if narrow { left - rule_w / 2.0 } else { left + 2.0 };
            let rule_y = top + title_size * 1.3;
            p.line_segment([Pos2::new(rule_x, rule_y), Pos2::new(rule_x + rule_w, rule_y)], Stroke::new(3.0, ACCENT));
            p.text(Pos2::new(left, rule_y + 16.0), align, "Creatures, timed battles, and the maps between them.", FontId::proportional(if narrow { 14.0 } else { 18.0 }), MUTED);
            let first = if narrow { screen.top() + screen.height() * 0.6 } else { screen.top() + screen.height() * 0.48 };
            let (item_h, item_w) = (if narrow { 46.0 } else { 54.0 }, 380.0f32.min(screen.width() - 32.0));
            for (i, name) in ITEMS.iter().enumerate() {
                let x0 = if narrow { left - item_w / 2.0 } else { left - 18.0 };
                let rect = Rect::from_min_size(Pos2::new(x0, first + i as f32 * item_h), Vec2::new(item_w, item_h - 6.0));
                let id = ui.id().with(("title-item", i));
                let r = ui.interact(rect, id, if enabled[i] && !busy { Sense::click() } else { Sense::hover() });
                let about = match i {
                    0 if saved.is_none() => "No saved story yet: New game starts one".to_string(),
                    0 => format!("Back to your story, where you left it ({})", saved.unwrap_or_default()),
                    1 => "A new story, from the start".into(),
                    2 => "Time of day, text, timing, graphics and sound".into(),
                    _ => "Leave the game".into(),
                };
                let r = r.on_hover_text(about);
                platform::set_hook(HOOKS[i], &format!("{:.0},{:.0}", rect.center().x, rect.center().y));
                if r.hovered() && enabled[i] && !busy {
                    self.sel = i;
                }
                let on = self.sel == i && enabled[i] && !busy;
                let colour = if !enabled[i] { Color32::from_white_alpha(70) } else if on { ACCENT } else { TEXT };
                if on {
                    p.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(255, 213, 74, 22));
                    p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())), 2.0, ACCENT);
                }
                let text_x = if narrow { rect.center().x } else { rect.left() + 18.0 };
                let text_align = if narrow { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
                let shown = p.text(Pos2::new(text_x, rect.center().y), text_align, *name, FontId::proportional(if narrow { 24.0 } else { 28.0 }), colour);
                if i == 0 {
                    if let Some(place) = saved {
                        p.text(Pos2::new(shown.right() + 14.0, rect.center().y + 2.0), Align2::LEFT_CENTER, place, FontId::proportional(14.0), MUTED);
                    }
                }
                if r.clicked() {
                    picked = Some(i);
                }
            }
            if let Some(at) = self.exit_tried.filter(|at| now - at > 400.0) {
                let _ = at;
                let y = first + ITEMS.len() as f32 * item_h + 4.0;
                p.text(Pos2::new(left, y), align, "The browser keeps this tab open: close it to leave.", FontId::proportional(14.0), MUTED);
            }
            // the keys, bottom left
            if !narrow {
                p.text(Pos2::new(left, screen.bottom() - 24.0), Align2::LEFT_BOTTOM, "Arrows: choose  ·  Z or Enter: select  ·  X or Esc: back", FontId::proportional(12.5), Color32::from_white_alpha(110));
            }
            // the Editor, in the bottom-right corner
            let editor = Rect::from_min_size(Pos2::new(screen.right() - 116.0, screen.bottom() - 52.0), Vec2::new(96.0, 34.0));
            let r = ui.interact(editor, ui.id().with("title-editor"), if busy { Sense::hover() } else { Sense::click() });
            let r = r.on_hover_text("Make maps, props, creatures and abilities, and add them to the game");
            platform::set_hook("uiEditor", &format!("{:.0},{:.0}", editor.center().x, editor.center().y));
            let hot = r.hovered() && !busy;
            p.rect(editor, 7.0, if hot { Color32::from_rgba_unmultiplied(255, 213, 74, 30) } else { Color32::from_black_alpha(90) }, Stroke::new(1.0, if hot { ACCENT } else { Color32::from_white_alpha(70) }), egui::StrokeKind::Inside);
            p.text(editor.center(), Align2::CENTER_CENTER, "Editor", FontId::proportional(16.0), if hot { ACCENT } else { TEXT });
            if r.clicked() {
                picked = Some(ITEMS.len());
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
                picked = Some(self.sel);
            }
        }
        let mut choice = None;
        match picked {
            Some(0) => choice = Some(Choice::Continue),
            Some(1) if saved.is_some() => self.confirm_new = true,
            Some(1) => choice = Some(Choice::NewGame),
            Some(2) => self.options_open = true,
            Some(3) => {
                if cfg!(target_arch = "wasm32") {
                    self.exit_tried = Some(now);
                }
                choice = Some(Choice::Exit);
            }
            Some(_) => choice = Some(Choice::Editor),
            None => {}
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
        if self.options_open && options_window(&ctx, options, &mut self.options_open, input) {
            *options_changed = true;
        }
        platform::set_hook("options", if self.options_open { "open" } else { "" });
        choice
    }
}

/// "Start a new game?" when a story is saved: Some(true) to start over, Some(false) to keep it.
fn confirm_new(ctx: &egui::Context, saved: &str, input: &Input) -> Option<bool> {
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::new("title-new-game")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.label(RichText::new("Start a new game?").size(22.0).color(TEXT));
        ui.add_space(6.0);
        ui.label(RichText::new(format!("The story saved on {saved} is replaced by the new one.")).size(14.0).color(MUTED));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let yes = ui.add(egui::Button::new(RichText::new("Start a new game").strong().color(Color32::from_rgb(34, 34, 34))).fill(ACCENT));
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

/// The Options, a dialog over the screen: true when an option changed. Done, B (X, Esc) or a
/// click beside it closes it.
pub fn options_window(ctx: &egui::Context, o: &mut Options, open: &mut bool, input: &Input) -> bool {
    let before = o.saved();
    let mut done = false;
    let mut defaults = false;
    let modal = egui::Modal::new(egui::Id::new("game-options")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(RichText::new("Options").size(22.0).color(TEXT));
        // a part's rows: its name, then its choices, in two columns
        let part = |ui: &mut egui::Ui, name: &str, rows: &mut dyn FnMut(&mut egui::Ui)| {
            heading(ui, name);
            egui::Grid::new(("options", name)).num_columns(2).spacing([18.0, 8.0]).min_col_width(120.0).show(ui, |ui| rows(ui));
        };
        part(ui, "Game", &mut |ui| {
            ui.label(RichText::new("Time of day").color(TEXT));
            ui.horizontal(|ui| {
                let now = Period::of_hour(platform::local_hour()).name();
                for (label, t) in [("Clock", TimeChoice::Clock), ("Morning", TimeChoice::Fixed(Period::Morning)), ("Day", TimeChoice::Fixed(Period::Day)), ("Night", TimeChoice::Fixed(Period::Night))] {
                    let r = choose(ui, &mut o.time, t, label, "uiTime");
                    if label == "Clock" {
                        r.on_hover_text(format!("The time where you are (now: {now})"));
                    }
                }
            });
            ui.end_row();
            ui.label(RichText::new("Text").color(TEXT));
            ui.horizontal(|ui| {
                for (i, l) in ["Fast", "Normal", "Slow"].iter().enumerate() {
                    choose(ui, &mut o.text, i, l, "uiText");
                }
            });
            ui.end_row();
        });
        part(ui, "Battles", &mut |ui| {
            ui.label(RichText::new("Timing windows").color(TEXT));
            ui.horizontal(|ui| {
                for (i, l) in ["Easy", "Normal", "Hard"].iter().enumerate() {
                    choose(ui, &mut o.difficulty, i, l, "uiTiming").on_hover_text("How much time a hit, a block or a release gives you");
                }
            });
            ui.end_row();
            ui.label(RichText::new("Timing rings").color(TEXT));
            ui.checkbox(&mut o.assist, "Show when to press").on_hover_text("A ring that closes on the moment to press");
            ui.end_row();
        });
        part(ui, "Graphics", &mut |ui| {
            ui.label(RichText::new("Resolution").color(TEXT));
            ui.horizontal(|ui| {
                for p in [100u8, 75, 50] {
                    choose(ui, &mut o.graphics.scale, p, &format!("{p}%"), "uiResolution");
                }
            });
            ui.end_row();
            ui.label(RichText::new("Shadows").color(TEXT));
            ui.horizontal(|ui| {
                for (i, (name, _)) in Graphics::SHADOWS.iter().enumerate() {
                    choose(ui, &mut o.graphics.shadows, i, name, "uiShadows");
                }
            });
            ui.end_row();
        });
        ui.label(RichText::new("Lower these if the game stutters.").size(12.0).color(MUTED));
        part(ui, "Sound", &mut |ui| {
            ui.label(RichText::new("Volume").color(TEXT));
            let mut v = o.volume as f32;
            let s = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).step_by(5.0).fixed_decimals(0).suffix("%"));
            platform::set_hook("uiVolume", &format!("{:.0},{:.0}", s.rect.center().x, s.rect.center().y));
            if s.changed() {
                o.volume = v.round() as u8;
            }
            ui.end_row();
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            let d = ui.add(egui::Button::new(RichText::new("Done").strong().color(Color32::from_rgb(34, 34, 34))).fill(ACCENT).min_size(Vec2::new(72.0, 26.0)));
            platform::set_hook("uiOptionsDone", &format!("{:.0},{:.0}", d.rect.center().x, d.rect.center().y));
            if d.clicked() {
                done = true;
            }
            let b = ui.add(egui::Button::new("Defaults").min_size(Vec2::new(72.0, 26.0))).on_hover_text("Every option back as it came");
            if b.clicked() {
                defaults = true;
            }
        });
    });
    if defaults {
        o.reset();
    }
    if done || modal.should_close() || input.pressed(Button::B) && !ctx.egui_wants_keyboard_input() {
        *open = false;
    }
    o.saved() != before
}

/// One of an option's values, picked by a click; its place kept for the checks
/// (`<hook>_<label>`, "uiTime_Night").
fn choose<T: PartialEq>(ui: &mut egui::Ui, current: &mut T, value: T, label: &str, hook: &str) -> egui::Response {
    let r = ui.selectable_value(current, value, label);
    platform::set_hook(&format!("{hook}_{}", label.trim_end_matches('%')), &format!("{:.0},{:.0}", r.rect.center().x, r.rect.center().y));
    r
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).strong().color(Color32::from_gray(150)));
    ui.add_space(2.0);
}

/// The night-blue backdrop, lighter at the top.
fn backdrop(p: &egui::Painter, r: Rect) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), SKY_TOP);
    mesh.colored_vertex(r.right_top(), SKY_TOP);
    mesh.colored_vertex(r.right_bottom(), SKY_BOTTOM);
    mesh.colored_vertex(r.left_bottom(), SKY_BOTTOM);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

// ---------------------------------------------------------------- the emblem

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The height of a ridge's line at `x` (a polyline of (x, y) points, x rising).
fn ridge_at(points: &[(f32, f32)], x: f32) -> f32 {
    for w in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 {
            return y0 + (y1 - y0) * ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        }
    }
    points.last().map(|p| p.1).unwrap_or(0.0)
}

/// Whether a point is inside a polygon (even-odd).
fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            odd = !odd;
        }
        j = i;
    }
    odd
}

const FAR: [(f32, f32); 8] = [(-1.0, 0.30), (-0.62, 0.02), (-0.38, 0.22), (-0.05, -0.20), (0.22, 0.12), (0.48, -0.04), (0.78, 0.24), (1.0, 0.16)];
const NEAR: [(f32, f32); 6] = [(-1.0, 0.52), (-0.55, 0.30), (-0.20, 0.46), (0.18, 0.28), (0.55, 0.50), (1.0, 0.38)];

/// The emblem's colour at a point, in the ring's radii from its middle (y down): the sky,
/// the sun and its glow, the two ridges, the star, the gold rings; None outside.
fn emblem_at(x: f32, y: f32, star: &[(f32, f32)]) -> Option<[f32; 4]> {
    let d = (x * x + y * y).sqrt();
    let w = 0.1125;
    if (1.0 + 0.72 * w..=1.0 + 0.9 * w).contains(&d) {
        return Some([150.0, 108.0, 40.0, 1.0]);
    }
    if (1.0 - w / 2.0..=1.0 + w / 2.0).contains(&d) {
        return Some([240.0, 196.0, 92.0, 1.0]);
    }
    if d > 1.0 {
        return None;
    }
    if y > ridge_at(&NEAR, x) {
        return Some([26.0, 18.0, 46.0, 1.0]);
    }
    if y > ridge_at(&FAR, x) {
        return Some([64.0, 34.0, 84.0, 1.0]);
    }
    if inside(star, x, y) {
        return Some([255.0, 252.0, 236.0, 1.0]);
    }
    let sun = (x * x + (y - 0.08) * (y - 0.08)).sqrt();
    if sun < 0.42 {
        return Some([255.0, 246.0, 214.0, 1.0]);
    }
    // the sky, the icon's sunset (over a square 2.5 radii tall), a glow round the sun
    let mut c = lerp([255.0, 196.0, 92.0], [232.0, 74.0, 76.0], (y + 1.25) / 2.5);
    let glow = 0.47 * (1.0 - ((sun - 0.42) / 0.36).clamp(0.0, 1.0)).powf(1.6);
    c = lerp(c, [255.0, 240.0, 180.0], glow);
    Some([c[0], c[1], c[2], 1.0])
}

/// The emblem as a picture `size` pixels across, see-through round it, its edges smoothed
/// (3 x 3 samples a pixel).
pub fn emblem(size: usize) -> egui::ColorImage {
    let star: Vec<(f32, f32)> = (0..8)
        .map(|i| {
            let a = std::f32::consts::FRAC_PI_4 * i as f32 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 0.16 } else { 0.16 * 0.28 };
            (-0.05 + a.cos() * r, -0.55 + a.sin() * r)
        })
        .collect();
    // the ring's outer edge just inside the picture
    let scale = 2.0 * (1.0 + 0.9 * 0.1125) * 1.02 / size as f32;
    let mut pixels = Vec::with_capacity(size * size);
    for py in 0..size {
        for px in 0..size {
            let mut sum = [0.0f32; 4];
            for sy in 0..3 {
                for sx in 0..3 {
                    let x = (px as f32 + (sx as f32 + 0.5) / 3.0 - size as f32 / 2.0) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / 3.0 - size as f32 / 2.0) * scale;
                    if let Some(c) = emblem_at(x, y, &star) {
                        sum[0] += c[0];
                        sum[1] += c[1];
                        sum[2] += c[2];
                        sum[3] += c[3];
                    }
                }
            }
            // premultiplied: colours summed only where something is
            let n = 9.0;
            pixels.push(Color32::from_rgba_premultiplied((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (sum[3] / n * 255.0) as u8));
        }
    }
    egui::ColorImage::new([size, size], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_emblem_is_the_icons() {
        let img = emblem(64);
        let at = |x: usize, y: usize| img.pixels[y * 64 + x];
        assert_eq!(at(0, 0).a(), 0, "see-through round the ring");
        assert_eq!(at(32, 50).to_srgba_unmultiplied()[..3], [26, 18, 46], "the near ridge low in the middle");
        let ring = at(32, 2).to_srgba_unmultiplied();
        assert!(ring[3] > 0 && ring[0] > 140, "the gold ring at the top: {ring:?}");
        let sky = at(14, 22).to_srgba_unmultiplied();
        assert!(sky[0] > 200 && sky[2] < 160, "a sunset sky: {sky:?}");
        assert!(ridge_at(&FAR, -0.05) < ridge_at(&FAR, 0.5) && inside(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)], 0.2, 0.2));
    }
}
