//! The HUD over the overworld, in layout groups (`layout`): the Team bottom left (its list of
//! creatures opening beside it), Settings and Exit top right; the map's name and the keys top
//! left. Esc doesn't leave the world any more: Exit does.
//!
//! The Team's list: the five creatures the player fought with most in their last battles,
//! their main (pinned) first, then "View all": a board of cards, every creature, five a row,
//! the list's five first (or as the player sorts and filters them, the main first all the
//! same), each card its picture, name, types, level, status, HP, MP, Exp and base stats. Clicking a creature
//! pins it as the main, the one sent out first in battle; there's always one (the team's first
//! until another is pinned). The list closes with the Team again, a click anywhere else, or
//! that window opening.
//!
//! The mouse works it all, and so do the game's buttons: Esc (or a controller's Start) puts a
//! ring on the HUD, the arrows (the d-pad) move it, A presses, B goes back.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::Options;
use crate::input::{Button, Input};
use crate::layout::{self, BOTTOM_LEFT, BUTTON, TOP_RIGHT};
use crate::platform;
use crate::team::{Sort, TeamLog, RECENT, SHOWN};
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

/// The board of cards (View all): five a row; a card's size at most, the room between them.
const PER_ROW: usize = 5;
const CARD_W: f32 = 172.0;
const CARD_H: f32 = 334.0;
const CARD_GAP: f32 = 12.0;
/// The board's marks: Close, the sort's dropdown and the types'.
const CLOSE: &str = "uiTeamWindowClose";
const SORT: &str = "uiTeamSort";
const TYPES: &str = "uiTeamTypes";

/// One of the player's creatures, as the Team shows it.
pub struct Member {
    pub id: String,
    /// what it's called: its own name if it was given one, else its kind's
    pub name: String,
    pub level: u8,
    pub icon: Option<egui::TextureId>,
    /// what its card on the board shows
    pub card: Card,
}

/// A creature's card on the board (View all): what it is and how it is.
#[derive(Clone, Debug, Default)]
pub struct Card {
    /// its kind's name (shown under a name of its own) and its number in the creature index
    pub kind: String,
    pub number: u16,
    /// its front picture: the texture, the part with the creature in it (in the texture's 0
    /// to 1) and how many pixels that part is
    pub picture: Option<(egui::TextureId, Rect, Vec2)>,
    /// its types, in their order
    pub types: Vec<TypeTag>,
    /// its status ("BRN", "PAR"...), if it has one
    pub status: Option<String>,
    /// now and at most
    pub hp: (i32, i32),
    pub mp: (i32, i32),
    /// its experience: in all, where its level begins, where the next one does (none at the
    /// top)
    pub exp: (u32, u32, Option<u32>),
    /// its kind's base stats: HP, Attack, Defense, Sp. Atk, Sp. Def, Speed
    pub base: [u16; 6],
}

/// A type as the cards show it.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeTag {
    pub name: String,
    pub color: Color32,
    /// the order types are listed in
    pub order: u16,
}

/// The board of cards' own state: its order and filter, its dropdowns, where the game's
/// buttons are.
#[derive(Default)]
struct Board {
    sort: Sort,
    reverse: bool,
    /// the types not shown, by name (none: all of them; a type new to the team shows)
    hidden: BTreeSet<String>,
    /// the sort's dropdown open, the types' dropdown open
    sort_open: bool,
    types_open: bool,
    /// the place the game's buttons are on (a card's mark, a control's), whether the ring
    /// shows, and the card last scrolled to
    focus: Option<String>,
    ring: bool,
    scrolled: Option<String>,
    /// opened this frame: the button that opened it isn't its own
    fresh: bool,
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
    /// the window of all the player's creatures (View all), its board of cards
    team_window: bool,
    board: Board,
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
        // there's always a main (the story keeps it)
        log.settle(&ids);
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
                // the game's buttons went there: the ring on the first card
                self.board.focus = pressed.is_some().then(|| card_mark(0));
                self.board.ring = pressed.is_some();
                self.board.fresh = true;
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
            self.team_window = team_window(ctx, team, log, input, &mut self.board);
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
    let does = if log.main.as_deref() == Some(m.id.as_str()) { "your main, sent out first".to_string() } else { format!("{how}: pin as your main") };
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

/// A card's mark: its place on the board.
fn card_mark(slot: usize) -> String {
    format!("uiTeamCard_{slot}")
}

/// A mark from words: their letters and digits ("No." to "No").
fn mark_of(prefix: &str, words: &str) -> String {
    format!("{prefix}{}", words.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
}

/// Two creatures in a sort's order: the highest level and the most HP first, names from A,
/// numbers from the first; most used says nothing (the list's order stands).
fn compare(sort: Sort, a: &Member, b: &Member) -> Ordering {
    match sort {
        Sort::MostUsed => Ordering::Equal,
        Sort::Level => b.level.cmp(&a.level),
        Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        Sort::Hp => b.card.hp.0.cmp(&a.card.hp.0),
        Sort::Number => a.card.number.cmp(&b.card.number),
    }
}

/// A number with its thousands apart ("42,875").
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A status's colour, and its words' (KO too).
fn status_colors(label: &str) -> (Color32, Color32) {
    match label {
        "BRN" => (Color32::from_rgb(226, 112, 58), Color32::WHITE),
        "PAR" => (Color32::from_rgb(232, 196, 58), Color32::from_rgb(40, 34, 10)),
        "PSN" | "TOX" => (Color32::from_rgb(168, 104, 208), Color32::WHITE),
        "SLP" => (Color32::from_rgb(132, 150, 176), Color32::WHITE),
        "FRZ" => (Color32::from_rgb(120, 196, 232), Color32::from_rgb(12, 40, 56)),
        _ => (Color32::from_rgb(214, 72, 72), Color32::WHITE),
    }
}

/// The words' colour on a colour: dark on a light one (Electric's yellow), else white.
fn ink_on(fill: Color32) -> Color32 {
    let light = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if light > 165.0 { Color32::from_rgb(28, 30, 38) } else { Color32::WHITE }
}

/// A rounded tag with words in it, at `at` (its left, its top): where it is.
fn tag(p: &egui::Painter, at: Pos2, words: &str, fill: Color32, ink: Color32) -> Rect {
    let g = p.layout_no_wrap(words.to_string(), FontId::proportional(11.0), ink);
    let r = Rect::from_min_size(at, Vec2::new(g.size().x + 12.0, 17.0));
    p.rect_filled(r, 8.5, fill);
    p.galley(r.center() - g.size() / 2.0, g, ink);
    r
}

/// A creature's card: its picture (the main's with a pin, its status or KO on it), its name
/// and level (under them its number, and its kind's name when it has its own), its types, its
/// HP, MP and Exp in bars, its kind's base stats. Greyed out when it's KO.
fn card(ui: &mut egui::Ui, m: &Member, main: bool, w: f32) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(w, CARD_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return r;
    }
    let c = &m.card;
    let ko = c.hp.0 <= 0;
    let hover = ui.ctx().animate_bool_with_time(r.id.with("hover"), r.hovered(), 0.15);
    let p = ui.painter_at(rect);
    let fill = Color32::from_rgb(29, 37, 54).lerp_to_gamma(Color32::from_rgb(40, 51, 74), hover);
    p.rect_filled(rect, 12.0, fill);
    let edge = if main { Stroke::new(1.6, Color32::from_white_alpha(215)) } else { Stroke::new(1.0, Color32::from_white_alpha((28.0 + 50.0 * hover) as u8)) };
    p.rect_stroke(rect, 12.0, edge, egui::StrokeKind::Inside);
    let (x0, x1) = (rect.left() + 10.0, rect.right() - 10.0);
    let dim = |a: u8| Color32::from_white_alpha(if ko { a / 2 } else { a });
    // the picture: its pixels doubled (room for the biggest, 64 by 64), whole pixels at least
    // when there's room for them, smaller when there isn't
    let pic = Rect::from_min_max(Pos2::new(x0, rect.top() + 10.0), Pos2::new(x1, rect.top() + 150.0));
    p.rect_filled(pic, 8.0, Color32::from_black_alpha(45));
    if let Some((tex, uv, px)) = c.picture.filter(|(_, _, px)| px.x > 0.0 && px.y > 0.0) {
        let room = pic.shrink(6.0).size();
        let k = (room.x / px.x).min(room.y / px.y);
        let k = if k >= 1.0 { k.floor().min(2.0) } else { k };
        p.image(tex, Rect::from_center_size(pic.center(), px * k), uv, if ko { Color32::from_gray(80) } else { Color32::WHITE });
    }
    if main {
        let badge = pic.left_top() + Vec2::new(13.0, 13.0);
        p.circle_filled(badge, 9.0, Color32::WHITE);
        egui::Image::new(egui::include_image!("../icons/pin.svg")).fit_to_exact_size(Vec2::splat(32.0)).tint(Color32::from_rgb(20, 24, 32)).paint_at(ui, Rect::from_center_size(badge, Vec2::splat(12.0)));
    }
    if let Some(words) = if ko { Some("KO".to_string()) } else { c.status.clone() } {
        let (fill, ink) = status_colors(&words);
        tag(&p, pic.left_bottom() + Vec2::new(6.0, -23.0), &words, fill, ink);
    }
    // its name; under it its number in the creature index, and its kind's name when it has
    // its own
    let mut y = pic.bottom() + 8.0;
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, &m.name, menu_font(15.0), dim(255));
    p.text(Pos2::new(x1, y + 1.0), Align2::RIGHT_TOP, format!("Lv {}", m.level), menu_font(13.0), dim(220));
    y += 19.0;
    let under = if m.name != c.kind { format!("{} · No. {}", c.kind, c.number) } else { format!("No. {}", c.number) };
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, under, FontId::proportional(11.5), dim(150));
    y += 16.0;
    // its types
    let mut x = x0;
    for t in &c.types {
        let fill = if ko { t.color.gamma_multiply(0.5) } else { t.color };
        x = tag(&p, Pos2::new(x, y), &t.name, fill, ink_on(fill)).right() + 4.0;
    }
    y += 27.0;
    // HP, MP and Exp: a bar each, the numbers beside
    let bar = |y: f32, label: &str, share: f32, color: Color32, value: &str| {
        p.text(Pos2::new(x0, y), Align2::LEFT_CENTER, label, FontId::proportional(11.0), dim(165));
        p.text(Pos2::new(x1, y), Align2::RIGHT_CENTER, value, FontId::proportional(11.0), dim(255));
        let track = Rect::from_min_max(Pos2::new(x0 + 27.0, y - 2.5), Pos2::new(x1 - 54.0, y + 2.5));
        p.rect_filled(track, 2.5, Color32::from_white_alpha(28));
        p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * share.clamp(0.0, 1.0), track.height())), 2.5, if ko { color.gamma_multiply(0.5) } else { color });
    };
    let hp = c.hp.0.max(0) as f32 / c.hp.1.max(1) as f32;
    let hp_color = match hp {
        h if h > 0.5 => Color32::from_rgb(92, 200, 120),
        h if h > 0.2 => Color32::from_rgb(232, 196, 58),
        _ => Color32::from_rgb(214, 72, 72),
    };
    bar(y, "HP", hp, hp_color, &if ko { "KO".to_string() } else { format!("{}/{}", c.hp.0, c.hp.1) });
    y += 16.0;
    bar(y, "MP", c.mp.0.max(0) as f32 / c.mp.1.max(1) as f32, Color32::from_rgb(92, 150, 236), &format!("{}/{}", c.mp.0, c.mp.1));
    y += 16.0;
    let (exp, from, to) = c.exp;
    let share = to.map_or(1.0, |to| exp.saturating_sub(from) as f32 / to.saturating_sub(from).max(1) as f32);
    bar(y, "Exp", share, Color32::from_rgb(170, 132, 236), &thousands(exp));
    y += 17.0;
    // its kind's base stats: HP, Attack, Defense down the left, Sp. Atk, Sp. Def, Speed down the right
    let half = (x1 - x0 - 10.0) / 2.0;
    for (k, (label, v)) in ["HP", "Atk", "Def", "SpA", "SpD", "Spe"].iter().zip(c.base).enumerate() {
        let cx = x0 + (k / 3) as f32 * (half + 10.0);
        let cy = y + (k % 3) as f32 * 17.0;
        p.text(Pos2::new(cx, cy), Align2::LEFT_TOP, *label, FontId::proportional(10.5), dim(150));
        p.text(Pos2::new(cx + half, cy), Align2::RIGHT_TOP, v.to_string(), FontId::proportional(10.5), dim(235));
        let line = Rect::from_min_size(Pos2::new(cx, cy + 13.0), Vec2::new(half, 2.0));
        p.rect_filled(line, 1.0, Color32::from_white_alpha(22));
        p.rect_filled(Rect::from_min_size(line.min, Vec2::new(half * (v as f32 / 200.0).min(1.0), 2.0)), 1.0, dim(170));
    }
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let next = match to {
        Some(to) => format!("{} more to Lv {}", thousands(to.saturating_sub(exp)), m.level as u32 + 1),
        None => "the top level".to_string(),
    };
    let total: u32 = c.base.iter().map(|&b| b as u32).sum();
    let does = if main { "your main, sent out first".to_string() } else { "click: pin as your main".to_string() };
    r.on_hover_text(format!("{} Exp, {next}\nBase stats {total} in all\n{does}", thousands(exp)))
}

/// A small button with an icon on it, as the HUD's are (white on a faint ground, brighter
/// hovered), its words in a tip.
fn icon_button(ui: &mut egui::Ui, icon: egui::ImageSource<'static>, tip: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(r.id.with("hover"), r.hovered(), 0.15);
    ui.painter().rect_filled(rect, 9.0, Color32::from_white_alpha((14.0 + 30.0 * hover) as u8));
    egui::Image::new(icon).fit_to_exact_size(Vec2::splat(36.0)).tint(Color32::WHITE).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.0)));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r.on_hover_text(tip)
}

/// A dropdown's button: what's chosen, a chevron after it (up while it's open).
fn dropdown_button(ui: &mut egui::Ui, words: &str, open: bool) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(format!("{words}   ")).color(Color32::WHITE)).min_size(Vec2::new(96.0, 24.0)).selected(open));
    let at = Pos2::new(r.rect.right() - 11.0, r.rect.center().y);
    let d = if open { -2.0 } else { 2.0 };
    ui.painter().add(egui::Shape::line(vec![at + Vec2::new(-4.0, -d), at + Vec2::new(0.0, d), at + Vec2::new(4.0, -d)], Stroke::new(1.5, Color32::from_white_alpha(220))));
    r
}

/// The dropdowns' look: the dialogs' dark glass.
fn dropdown_frame() -> egui::Frame {
    egui::Frame::new().fill(Color32::from_rgb(26, 33, 49)).corner_radius(10.0).inner_margin(8.0).stroke(Stroke::new(1.0, Color32::from_white_alpha(45)))
}

/// The window of every creature the player has (View all): a board of cards, five a row,
/// the main first, then as the Team's list has them (the first row is the list's five), or as
/// sorted (a dropdown) and filtered by type (a dropdown of checkboxes, All and None at its
/// top), the main first all the same. A card clicked (A on it) pins it as the main. The
/// game's buttons work it all: the arrows or the d-pad move a ring over the cards and the
/// controls above them (and in a dropdown, over its choices), A presses, L and R go through
/// the sorts, B closes a dropdown, else the board. Close (its icon), B, Esc or a click beside
/// it closes it: false then.
fn team_window(ctx: &egui::Context, team: &[Member], log: &mut TeamLog, input: &Input, board: &mut Board) -> bool {
    let mut open = true;
    let ids: Vec<String> = team.iter().map(|m| m.id.clone()).collect();
    // a dropdown open from before: Esc or B closes it, not the board; its button clicked
    // again closes it too
    let (sort_was, types_was) = (board.sort_open, board.types_open);
    let was_open = sort_was || types_was;
    // the game's buttons: the ring shows once they're used (always with a controller in
    // hand), and the mouse moving hides it; the first press only shows where it is
    let pad = crate::pads::on_pad();
    if ctx.input(|i| i.pointer.delta().length() > 2.0) {
        board.ring = false;
    }
    let dirs = [(Button::Left, Vec2::new(-1.0, 0.0)), (Button::Right, Vec2::new(1.0, 0.0)), (Button::Up, Vec2::new(0.0, -1.0)), (Button::Down, Vec2::new(0.0, 1.0))];
    let fresh = std::mem::take(&mut board.fresh);
    let mut moved = dirs.iter().find(|(b, _)| input.pressed(*b)).map(|&(_, d)| d).filter(|_| !fresh);
    let mut press = input.pressed(Button::A) && !fresh;
    let back = input.pressed(Button::B) && !fresh;
    let sorts_by = if fresh { 0 } else { input.pressed(Button::R) as i32 - input.pressed(Button::L) as i32 };
    if moved.is_some() || press || sorts_by != 0 {
        if !(board.ring || pad) {
            (moved, press) = (None, false);
        }
        board.ring = true;
    }
    if sorts_by != 0 {
        let i = Sort::ALL.iter().position(|&s| s == board.sort).unwrap_or(0) as i32;
        board.sort = Sort::ALL[(i + sorts_by).rem_euclid(Sort::ALL.len() as i32) as usize];
    }
    // the types the team has, in their order, for the filter
    let mut kinds: Vec<&TypeTag> = vec![];
    for t in team.iter().flat_map(|m| &m.card.types) {
        if !kinds.iter().any(|k| k.name == t.name) {
            kinds.push(t);
        }
    }
    kinds.sort_by_key(|t| t.order);
    let shown_types: Vec<&str> = kinds.iter().filter(|t| !board.hidden.contains(&t.name)).map(|t| t.name.as_str()).collect();
    // the main first, then those the filter keeps (one of their types shown), in the order
    // asked for
    let order = {
        let (sort, hidden) = (board.sort, &board.hidden);
        let keep = |i: usize| hidden.is_empty() || team[i].card.types.iter().any(|t| !hidden.contains(&t.name));
        log.board(&ids, keep, |a, b| compare(sort, &team[a], &team[b]), board.reverse)
    };
    let main = log.main_of(&ids);
    // the cards as wide as the screen has room for, up to their size
    let screen = ctx.content_rect();
    let card_w = ((screen.width() - 110.0 - CARD_GAP * (PER_ROW - 1) as f32) / PER_ROW as f32).clamp(110.0, CARD_W);
    let width = card_w * PER_ROW as f32 + CARD_GAP * (PER_ROW - 1) as f32;
    // the places for the ring (the board's, and an open dropdown's), what was clicked
    let mut stops: Vec<layout::Stop> = vec![];
    let mut pops: Vec<layout::Stop> = vec![];
    let mut clicked: Option<String> = None;
    let place = |r: Rect| format!("{:.0},{:.0}", r.center().x, r.center().y);
    let stop = |stops: &mut Vec<layout::Stop>, name: String, r: &egui::Response, clicked: &mut Option<String>| {
        platform::set_hook(&name, &place(r.rect));
        if r.clicked() {
            *clicked = Some(name.clone());
        }
        stops.push(layout::Stop { name, rect: r.rect });
    };
    let modal = egui::Modal::new(Id::new("hud-team-window")).frame(dialog_frame()).show(ctx, |ui| {
        ui.set_width(width);
        // the title, how many, Close
        ui.horizontal(|ui| {
            ui.label(RichText::new("Team").font(title_font(26.0)).color(Color32::WHITE));
            ui.add_space(6.0);
            let count = match (team.len(), order.len()) {
                (0, _) => "No creatures yet".to_string(),
                (n, shown) if shown < n => format!("{shown} of {n} shown"),
                (1, _) => "1 creature".to_string(),
                (n, _) => format!("{n} creatures"),
            };
            ui.label(RichText::new(count).color(Color32::from_white_alpha(165)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = icon_button(ui, egui::include_image!("../icons/close.svg"), "Close");
                stop(&mut stops, CLOSE.to_string(), &close, &mut clicked);
            });
        });
        ui.add_space(8.0);
        // how they're sorted and which types show: a dropdown each
        let (sort_button, types_button) = ui
            .horizontal(|ui| {
                ui.label(RichText::new("Sort").color(Color32::from_white_alpha(150)));
                let sort = dropdown_button(ui, board.sort.label(), board.sort_open);
                stop(&mut stops, SORT.to_string(), &sort, &mut clicked);
                let r = ui.selectable_label(board.reverse, "Reversed").on_hover_text("The other way round (the main stays first)");
                stop(&mut stops, "uiTeamReverse".to_string(), &r, &mut clicked);
                ui.add_space(14.0);
                ui.label(RichText::new("Types").color(Color32::from_white_alpha(150)));
                let words = match shown_types.len() {
                    n if n == kinds.len() => "All".to_string(),
                    0 => "None".to_string(),
                    1 | 2 => shown_types.join(", "),
                    n => format!("{n} of {}", kinds.len()),
                };
                let types = dropdown_button(ui, &words, board.types_open);
                stop(&mut stops, TYPES.to_string(), &types, &mut clicked);
                (sort, types)
            })
            .inner;
        // the sort's: its choices, one picked closes it
        egui::Popup::from_response(&sort_button).open_bool(&mut board.sort_open).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).frame(dropdown_frame()).show(|ui| {
            ui.set_min_width(sort_button.rect.width());
            for s in Sort::ALL {
                let r = ui.selectable_label(board.sort == s, s.label());
                stop(&mut pops, mark_of("uiTeamSort_", s.label()), &r, &mut clicked);
            }
        });
        // the types': All and None at its top, then a checkbox each (as many shown at once as
        // checked)
        egui::Popup::from_response(&types_button).open_bool(&mut board.types_open).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).frame(dropdown_frame()).show(|ui| {
            ui.set_min_width(types_button.rect.width().max(150.0));
            ui.horizontal(|ui| {
                for (words, name) in [("All", "uiTeamTypes_All"), ("None", "uiTeamTypes_None")] {
                    let r = ui.add(egui::Button::new(RichText::new(words).size(12.0).color(Color32::WHITE)).small().min_size(Vec2::new(44.0, 20.0)));
                    stop(&mut pops, name.to_string(), &r, &mut clicked);
                }
            });
            ui.separator();
            let mut checkboxes = |ui: &mut egui::Ui| {
                for t in &kinds {
                    let mut on = !board.hidden.contains(&t.name);
                    let r = ui.checkbox(&mut on, RichText::new(&t.name).color(t.color.lerp_to_gamma(Color32::WHITE, 0.4)));
                    stop(&mut pops, mark_of("uiTeamType_", &t.name), &r, &mut clicked);
                }
            };
            // a long list scrolls (All and None stay on top)
            if kinds.len() > 12 {
                egui::ScrollArea::vertical().max_height(280.0).show(ui, checkboxes);
            } else {
                checkboxes(ui);
            }
        });
        ui.add_space(10.0);
        // the cards, five a row
        egui::ScrollArea::vertical().max_height((screen.height() - 190.0).max(CARD_H + 8.0)).auto_shrink([false, true]).show(ui, |ui| {
            for (row, cards) in order.chunks(PER_ROW).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = CARD_GAP;
                    for (k, &i) in cards.iter().enumerate() {
                        let name = card_mark(row * PER_ROW + k);
                        let r = card(ui, &team[i], Some(i) == main, card_w);
                        // the ring brought it here: in view
                        if board.ring && board.focus.as_deref() == Some(name.as_str()) && board.scrolled.as_deref() != Some(name.as_str()) {
                            r.scroll_to_me(None);
                            board.scrolled = Some(name.clone());
                        }
                        stop(&mut stops, name, &r, &mut clicked);
                    }
                });
                ui.add_space(CARD_GAP);
            }
            if order.is_empty() {
                ui.label(RichText::new("Your creatures show here.").color(Color32::from_white_alpha(165)));
            }
        });
        // the game's buttons' words
        let b = |b: Button| button_label(b);
        let words = if was_open {
            format!("{}: pick  ·  {}: back", b(Button::A), b(Button::B))
        } else {
            format!("{}, {}: sort  ·  {}: pin as your main  ·  {}: close", b(Button::L), b(Button::R), b(Button::A), b(Button::B))
        };
        ui.label(RichText::new(words).size(12.0).color(Color32::from_white_alpha(130)));
    });
    // the ring: over an open dropdown's choices, else the board's (onto the first card if
    // it's nowhere yet); moved, pressed
    let popup = board.sort_open || board.types_open;
    if board.ring {
        let places = if popup && !pops.is_empty() { &pops } else { &stops };
        let at = board.focus.as_ref().and_then(|f| places.iter().position(|s| &s.name == f));
        let at = match at {
            Some(at) => moved.map_or(at, |d| layout::step(places, at, d)),
            None if popup => 0,
            None => places.iter().position(|s| s.name == card_mark(0)).unwrap_or(0),
        };
        if let Some(s) = places.get(at) {
            board.focus = Some(s.name.clone());
            layout::ring(ctx, s.rect);
            if press && clicked.is_none() {
                clicked = Some(s.name.clone());
            }
        }
    }
    // what was clicked or pressed
    match clicked.as_deref() {
        Some(CLOSE) => open = false,
        Some(SORT) => {
            board.sort_open = !sort_was;
            board.types_open = false;
            // the ring on what's picked
            board.focus = Some(mark_of("uiTeamSort_", board.sort.label()));
        }
        Some(TYPES) => {
            board.types_open = !types_was;
            board.sort_open = false;
            board.focus = Some("uiTeamTypes_All".to_string());
        }
        Some("uiTeamReverse") => board.reverse = !board.reverse,
        Some("uiTeamTypes_All") => board.hidden.clear(),
        Some("uiTeamTypes_None") => board.hidden = kinds.iter().map(|t| t.name.clone()).collect(),
        Some(name) if name.starts_with("uiTeamSort_") => {
            if let Some(s) = Sort::ALL.into_iter().find(|s| mark_of("uiTeamSort_", s.label()) == name) {
                board.sort = s;
            }
            board.sort_open = false;
            board.focus = Some(SORT.to_string());
        }
        Some(name) if name.starts_with("uiTeamType_") => {
            if let Some(t) = kinds.iter().find(|t| mark_of("uiTeamType_", &t.name) == name) {
                if !board.hidden.remove(&t.name) {
                    board.hidden.insert(t.name.clone());
                }
            }
        }
        Some(name) if name.starts_with("uiTeamCard_") => {
            if let Some(&i) = name.strip_prefix("uiTeamCard_").and_then(|n| n.parse::<usize>().ok()).and_then(|slot| order.get(slot)) {
                log.pin(&team[i].id);
                // the ring follows it to its place: first
                if board.ring {
                    board.focus = Some(card_mark(0));
                }
            }
        }
        _ => {}
    }
    // Esc or B: a dropdown closes (the ring back on its button), else the board
    if was_open && (back || modal.should_close()) {
        if board.focus.as_deref().is_some_and(|f| f.starts_with("uiTeamSort_")) {
            board.focus = Some(SORT.to_string());
        } else if board.focus.as_deref().is_some_and(|f| f.starts_with("uiTeamType")) {
            board.focus = Some(TYPES.to_string());
        }
        (board.sort_open, board.types_open) = (false, false);
    } else if modal.should_close() || back {
        open = false;
    }
    platform::set_hook("hudboard", &order.iter().map(|&i| team[i].id.as_str()).collect::<Vec<_>>().join(","));
    platform::set_hook("hudboardlevels", &order.iter().map(|&i| team[i].level.to_string()).collect::<Vec<_>>().join(","));
    platform::set_hook("hudboardsort", &format!("{}{}", board.sort.label(), if board.reverse { " reversed" } else { "" }));
    platform::set_hook("hudboardtypes", &shown_types.join(","));
    platform::set_hook("hudboardpopup", if board.sort_open { "sort" } else if board.types_open { "types" } else { "" });
    platform::set_hook("hudboardring", if board.ring { board.focus.as_deref().unwrap_or("") } else { "" });
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, level: u8, hp: i32, number: u16) -> Member {
        Member { id: id.into(), name: id.into(), level, icon: None, card: Card { hp: (hp, 100), number, ..Default::default() } }
    }

    #[test]
    fn numbers_have_their_thousands_apart() {
        assert_eq!([0, 999, 1000, 42_875, 1_059_860].map(thousands), ["0", "999", "1,000", "42,875", "1,059,860"]);
    }

    /// A sort's order: the highest level and the most HP first, names from A, numbers from the
    /// first; most used says nothing (the list's order stands). Marks keep letters and digits.
    #[test]
    fn the_sorts_put_them_in_order() {
        let (a, b) = (member("Bram", 30, 10, 7), member("ash", 12, 80, 3));
        assert_eq!(compare(Sort::Level, &a, &b), Ordering::Less, "the higher level first");
        assert_eq!(compare(Sort::Hp, &a, &b), Ordering::Greater, "the more HP first");
        assert_eq!(compare(Sort::Name, &a, &b), Ordering::Greater, "ash before Bram, whatever the case");
        assert_eq!(compare(Sort::Number, &a, &b), Ordering::Greater);
        assert_eq!(compare(Sort::MostUsed, &a, &b), Ordering::Equal);
        assert_eq!((mark_of("uiTeamSort_", "No."), mark_of("uiTeamSort_", "Most used")), ("uiTeamSort_No".to_string(), "uiTeamSort_Mostused".to_string()));
    }
}
