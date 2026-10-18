//! The HUD over the overworld, in layout groups (`layout`): the Team bottom left (its list of
//! creatures opening beside it), Settings and Exit top right; the map's name and the keys top
//! left. Esc doesn't leave the world any more: Exit does.
//!
//! The Team's list: the five creatures the player fought with most in their last battles,
//! their main (pinned) first, then "View all" (a window of all of them). Clicking a creature
//! pins it as the main (clicking the main unpins it). The list closes with the Team again, a
//! click anywhere else, or that window opening.
//!
//! The mouse works it all, and so do the game's buttons: Esc (or a controller's Start) puts a
//! ring on the HUD, the arrows (the d-pad) move it, A presses, B goes back.

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, RichText, Sense, Vec2};

use crate::app::Options;
use crate::input::{Button, Input};
use crate::layout::{self, BOTTOM_LEFT, BUTTON, TOP_RIGHT};
use crate::platform;
use crate::team::{TeamLog, RECENT, SHOWN};
use crate::title::{button_label, dialog_frame, menu_font, title_font, OptionsUi};

/// The places' names (the checks' marks).
pub const TEAM: &str = "uiHudTeam";
pub const SETTINGS: &str = "uiHudSettings";
pub const EXIT: &str = "uiHudExit";
pub const VIEW_ALL: &str = "uiTeamAll";

/// The Team's list opening or closing; a creature's tile in it, the space round and between
/// them.
const OPEN_S: f32 = 0.2;
const TILE: f32 = 44.0;
const PAD: f32 = 4.0;
const TILE_GAP: f32 = 6.0;

/// One of the player's creatures, as the Team shows it.
pub struct Member {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub icon: Option<egui::TextureId>,
}

/// What the player did on the HUD that the game answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Stay,
    /// Exit: back to the main menu
    Exit,
}

#[derive(Default)]
pub struct PlayHud {
    /// the Team's list is open
    team_open: bool,
    /// the window of all the player's creatures (View all)
    team_window: bool,
    /// the Options (Settings), and their own state
    options_open: bool,
    options_ui: OptionsUi,
    /// the game's buttons are on the HUD: the place the ring is on
    focus: Option<String>,
}

/// A creature's place in the Team's list (its slot there).
fn slot_mark(slot: usize) -> String {
    format!("uiTeam_{slot}")
}

/// One of the Team list's places (a creature's, View all).
fn in_list(name: &str) -> bool {
    name.starts_with("uiTeam_") || name == VIEW_ALL
}

impl PlayHud {
    /// Something of the HUD's has the game's buttons (the ring, the Options, the window): the
    /// overworld waits for it (no walking, no talking).
    pub fn holds(&self) -> bool {
        self.focus.is_some() || self.options_open || self.team_window
    }

    /// The HUD for a frame. `map`: the map's name; `story_busy`: a script or a dialog has the
    /// floor (the HUD waits, dimmed); `team`: the player's creatures in the team's order;
    /// `log`: their last battles and the main (pinned here). `options_changed` is set when the
    /// Options changed one.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ctx: &egui::Context, map: &str, story_busy: bool, team: &[Member], log: &mut TeamLog, options: &mut Options, input: &Input, options_changed: &mut bool) -> Out {
        let enabled = !story_busy && !self.options_open && !self.team_window;
        if story_busy {
            self.focus = None;
            self.team_open = false;
        }
        // Esc (or Start) puts the ring on the HUD, or takes it off; with the list open it
        // closes the list first. B goes back the same way while the ring is on.
        let esc = enabled && (ctx.input(|i| i.key_pressed(egui::Key::Escape)) || crate::pads::start_pressed());
        let back = esc || (enabled && self.focus.is_some() && input.pressed(Button::B));
        let mut moved = None;
        let mut press = false;
        if back && self.team_open {
            self.team_open = false;
        } else if back && self.focus.is_some() {
            self.focus = None;
        } else if esc {
            self.focus = Some(SETTINGS.into());
        } else if enabled && self.focus.is_some() {
            for (b, d) in [(Button::Left, Vec2::new(-1.0, 0.0)), (Button::Right, Vec2::new(1.0, 0.0)), (Button::Up, Vec2::new(0.0, -1.0)), (Button::Down, Vec2::new(0.0, 1.0))] {
                if input.pressed(b) {
                    moved = Some(d);
                }
            }
            press = input.pressed(Button::A);
        }

        // the map's name and the keys, top left
        let keys = crate::input::bindings();
        let pad = crate::title::pad_shown();
        let walk = crate::title::walk_hint(&keys, pad);
        platform::set_hook("walkhint", &walk);
        let hint = if self.focus.is_some() { crate::title::hud_hint(&keys, pad) } else { walk };
        platform::set_hook("hudhint", &hint);
        let words = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, Id::new("hud-words")));
        let top = ctx.content_rect().left_top() + Vec2::new(layout::MARGIN + 2.0, layout::MARGIN);
        crate::title::shadowed(&words, top, Align2::LEFT_TOP, map, menu_font(18.0), Color32::WHITE);
        crate::title::shadowed(&words, top + Vec2::new(0.0, 26.0), Align2::LEFT_TOP, &hint, FontId::proportional(13.0), Color32::from_white_alpha(200));

        // the groups: Settings and Exit top right, the Team (and its list) bottom left
        let ((settings, exit), mut stops) = TOP_RIGHT.show(ctx, enabled, |row| {
            let settings = row.button(egui::include_image!("../icons/settings.svg"), "Settings", SETTINGS);
            let exit = row.button(egui::include_image!("../icons/exit.svg"), "Exit to the main menu (your story is kept)", EXIT);
            (settings.clicked(), exit.clicked())
        });
        let open = ctx.animate_bool_with_time(Id::new("hud-team-open"), self.team_open, OPEN_S);
        let ids: Vec<String> = team.iter().map(|m| m.id.clone()).collect();
        let shown = log.most_used(&ids, SHOWN);
        let ((team_clicked, team_rect, list), bottom) = BOTTOM_LEFT.show(ctx, enabled, |row| {
            let r = row.button(egui::include_image!("../icons/team.svg"), "Team: your creatures", TEAM);
            let list = (open > 0.0).then(|| team_list(row, open, team, &shown, log));
            (r.clicked(), r.rect, list)
        });
        stops.extend(bottom);

        // the game's buttons: the ring moves, A presses what it's on (a place not there, as
        // the list opens: the ring waits for it)
        let mut pressed: Option<String> = None;
        if let Some(name) = self.focus.clone() {
            match stops.iter().position(|s| s.name == name) {
                Some(at) => {
                    let to = moved.map_or(at, |d| layout::step(&stops, at, d));
                    self.focus = Some(stops[to].name.clone());
                    if press {
                        pressed = Some(stops[to].name.clone());
                    }
                }
                None if moved.is_some() || press => self.focus = stops.first().map(|s| s.name.clone()),
                None => {}
            }
        }
        let hit = |name: &str| pressed.as_deref() == Some(name);

        // what was clicked or pressed: a click anywhere but on the Team or its list closes the
        // list (A on another place too)
        let in_team = |p: Pos2| team_rect.contains(p) || list.as_ref().is_some_and(|l| l.rect.contains(p));
        let pressed_outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !in_team(p)));
        if self.team_open && (pressed_outside || pressed.as_deref().is_some_and(|n| n != TEAM && !in_list(n))) {
            self.team_open = false;
        }
        if team_clicked || hit(TEAM) {
            self.team_open = !self.team_open;
            // the ring goes into the list as it opens
            if self.team_open && hit(TEAM) && !shown.is_empty() {
                self.focus = Some(slot_mark(0));
            }
        }
        let picked = list.as_ref().and_then(|l| l.clicked.clone()).or(pressed.clone().filter(|n| in_list(n)));
        if let Some(name) = picked.as_deref() {
            if name == VIEW_ALL {
                self.team_window = true;
                self.team_open = false;
            } else if let Some(m) = name.strip_prefix("uiTeam_").and_then(|s| s.parse::<usize>().ok()).and_then(|s| shown.get(s)).and_then(|&i| team.get(i)) {
                log.pin(&m.id);
                // the ring follows it to its new place (first, as the main)
                if pressed.is_some() {
                    if let Some(slot) = log.most_used(&ids, SHOWN).iter().position(|&i| team[i].id == m.id) {
                        self.focus = Some(slot_mark(slot));
                    }
                }
            }
        }
        // the list closed with the ring in it: the ring back on the Team
        if !self.team_open && self.focus.as_deref().is_some_and(in_list) {
            self.focus = Some(TEAM.into());
        }
        // the Options open from the next frame (the A that opened them isn't theirs)
        let opening = !self.options_open && (settings || hit(SETTINGS));
        let out = if exit || hit(EXIT) { Out::Exit } else { Out::Stay };

        // the ring, and the caption of the creature it's on (or the pointer)
        if let Some(s) = self.focus.as_ref().and_then(|name| stops.iter().find(|s| &s.name == name)) {
            layout::ring(ctx, s.rect);
        }
        if let Some(l) = &list {
            let ringed = self.focus.as_deref().filter(|n| in_list(n));
            let on = l.hovered.clone().or(ringed.map(String::from));
            if let Some(on) = on.filter(|_| open > 0.9) {
                let text = if on == VIEW_ALL {
                    ("View all".to_string(), "every creature of yours".to_string())
                } else {
                    let slot = on.strip_prefix("uiTeam_").and_then(|s| s.parse::<usize>().ok());
                    match slot.and_then(|s| shown.get(s)).and_then(|&i| team.get(i)) {
                        Some(m) => member_caption(m, log, ringed.is_some() && l.hovered.is_none()),
                        None => (String::new(), String::new()),
                    }
                };
                platform::set_hook("hudcaption", &format!("{} · {}", text.0, text.1));
                caption(ctx, l.rect, &text);
            }
        }

        if self.options_open && crate::title::options_window(ctx, options, &mut self.options_open, &mut self.options_ui, input) {
            *options_changed = true;
        }
        if opening {
            self.options_open = true;
        }
        if self.team_window {
            self.team_window = team_window(ctx, team, input);
        }
        platform::set_hook("hudring", self.focus.as_deref().unwrap_or(""));
        platform::set_hook("hudteam", if self.team_open { "open" } else { "" });
        platform::set_hook("hudteamshown", &shown.iter().map(|&i| team[i].id.as_str()).collect::<Vec<_>>().join(","));
        platform::set_hook("hudteamall", &ids.join(","));
        platform::set_hook("hudmain", log.main.as_deref().unwrap_or(""));
        platform::set_hook("hudoptions", if self.options_open { "open" } else { "" });
        platform::set_hook("hudteamwindow", if self.team_window { "open" } else { "" });
        out
    }
}

/// What the Team's list did this frame: where it is, what was clicked in it, what the pointer
/// is on.
struct List {
    rect: Rect,
    clicked: Option<String>,
    hovered: Option<String>,
}

/// The Team's list, opening (`open`, 0 to 1) beside the Team: a tile for each creature shown
/// (`shown`: their places in `team`), the main's with a pin, then View all. Its places join the
/// row's once it's open.
fn team_list(row: &mut layout::Row, open: f32, team: &[Member], shown: &[usize], log: &TeamLog) -> List {
    let ease = 1.0 - (1.0 - open).powi(3);
    let empty = if shown.is_empty() { 130.0 } else { 0.0 };
    let tiles = shown.len() + 1;
    let full = PAD * 2.0 + empty + tiles as f32 * TILE + (tiles - 1) as f32 * TILE_GAP;
    let enabled = row.enabled && open > 0.9;
    let ui = &mut *row.ui;
    let (rect, _) = ui.allocate_exact_size(Vec2::new((full * ease).max(1.0), BUTTON), Sense::hover());
    let p = ui.painter_at(rect);
    layout::frosted(&p, rect, layout::RADIUS, 0.0);
    let fade = |c: Color32| c.gamma_multiply(open);
    let (mut clicked, mut hovered) = (None, None);
    let mut places = vec![];
    let y = rect.center().y;
    let mut x = rect.left() + PAD;
    if shown.is_empty() {
        p.text(Pos2::new(x + 12.0, y), Align2::LEFT_CENTER, "No creatures yet", FontId::proportional(13.0), fade(Color32::from_white_alpha(210)));
        x += empty;
    }
    let mut tile = |ui: &mut egui::Ui, name: String, x: f32, paint: &dyn Fn(&egui::Ui, Rect)| {
        let r = Rect::from_min_size(Pos2::new(x, y - TILE / 2.0), Vec2::splat(TILE));
        let id = Id::new(("hud-team-tile", name.as_str()));
        let response = ui.interact(r.intersect(rect), id, if enabled { Sense::click() } else { Sense::hover() });
        let hover = ui.ctx().animate_bool_with_time(id, enabled && response.hovered(), 0.15);
        p.rect_filled(r, 11.0, fade(Color32::from_white_alpha((16.0 + 30.0 * hover) as u8)));
        paint(ui, r);
        if enabled && response.hovered() {
            hovered = Some(name.clone());
        }
        if response.clicked() {
            clicked = Some(name.clone());
        }
        places.push((name, r));
    };
    for (slot, &i) in shown.iter().enumerate() {
        let m = &team[i];
        let main = log.main.as_deref() == Some(m.id.as_str());
        tile(ui, slot_mark(slot), x, &|ui, r| {
            match m.icon {
                Some(t) => {
                    p.image(t, r.shrink(3.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), fade(Color32::WHITE));
                }
                None => {
                    let initial = m.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                    p.text(r.center(), Align2::CENTER_CENTER, initial, menu_font(18.0), fade(Color32::WHITE));
                }
            }
            // the main's pin, on its corner
            if main {
                let badge = r.right_top() + Vec2::new(-5.0, 5.0);
                p.circle_filled(badge, 8.5, fade(Color32::WHITE));
                egui::Image::new(egui::include_image!("../icons/pin.svg")).fit_to_exact_size(Vec2::splat(32.0)).tint(fade(Color32::from_rgb(20, 24, 32))).paint_at(ui, Rect::from_center_size(badge, Vec2::splat(12.0)));
            }
        });
        x += TILE + TILE_GAP;
    }
    tile(ui, VIEW_ALL.to_string(), x, &|ui, r| {
        egui::Image::new(egui::include_image!("../icons/all.svg")).fit_to_exact_size(Vec2::splat(64.0)).tint(fade(Color32::WHITE)).paint_at(ui, Rect::from_center_size(r.center(), Vec2::splat(TILE * 0.55)));
    });
    if open > 0.9 {
        for (name, r) in places {
            row.stop(&name, r);
        }
    }
    List { rect, clicked, hovered }
}

/// A creature's caption: its name and level; how much it fought lately and what pressing it
/// does (`ring`: the game's buttons are on it, so A does).
fn member_caption(m: &Member, log: &TeamLog, ring: bool) -> (String, String) {
    let fought = match log.uses(&m.id) {
        0 => "not in your last battles".to_string(),
        n => format!("in {n} of your last {RECENT} battles"),
    };
    let how = if ring { button_label(Button::A) } else { "click".to_string() };
    let does = if log.main.as_deref() == Some(m.id.as_str()) { format!("your main · {how}: unpin") } else { format!("{how}: pin as your main") };
    (format!("{} · Lv {}", m.name, m.level), format!("{fought} · {does}"))
}

/// A caption over the Team's list (`list`): a line in Medium, one in Light under it, on glass.
fn caption(ctx: &egui::Context, list: Rect, text: &(String, String)) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("hud-caption")));
    let name = p.layout_no_wrap(text.0.clone(), menu_font(14.0), Color32::WHITE);
    let what = p.layout_no_wrap(text.1.clone(), FontId::proportional(12.5), Color32::from_white_alpha(215));
    let size = Vec2::new(name.size().x.max(what.size().x) + 24.0, name.size().y + what.size().y + 16.0);
    let r = Rect::from_min_size(Pos2::new(list.left(), list.top() - 10.0 - size.y), size);
    layout::frosted(&p, r, 12.0, 0.0);
    let name_h = name.size().y;
    p.galley(r.min + Vec2::new(12.0, 7.0), name, Color32::WHITE);
    p.galley(r.min + Vec2::new(12.0, 9.0 + name_h), what, Color32::WHITE);
}

/// The window of every creature the player has (View all): their board of cards comes next;
/// for now, how many they are. Close, B, Esc or a click beside it closes it: false then.
fn team_window(ctx: &egui::Context, team: &[Member], input: &Input) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(Id::new("hud-team-window")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Team").font(title_font(26.0)).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = ui.add(egui::Button::new(RichText::new("Close").color(Color32::WHITE)).min_size(Vec2::new(72.0, 28.0)));
                platform::set_hook("uiTeamWindowClose", &format!("{:.0},{:.0}", close.rect.center().x, close.rect.center().y));
                if close.clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(6.0);
        let count = match team.len() {
            0 => "No creatures yet.".to_string(),
            1 => "1 creature.".to_string(),
            n => format!("{n} creatures."),
        };
        ui.label(RichText::new(count).color(Color32::from_white_alpha(210)));
        ui.add_space(4.0);
        ui.label(RichText::new(format!("{}: close", button_label(Button::B))).size(12.0).color(Color32::from_white_alpha(130)));
    });
    if modal.should_close() || input.pressed(Button::B) {
        open = false;
    }
    open
}
