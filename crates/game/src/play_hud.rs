//! The HUD over the overworld, in layout groups (`layout`): the Team bottom left (its list of
//! creatures opening beside it), Settings and Exit top right; the map's name and the keys top
//! left. Esc doesn't leave the world any more: Exit does.
//!
//! The Team's list: the five creatures the player fought with most in their last battles,
//! their main (pinned) first, then "View all": a board of cards, every creature, five a row,
//! the list's five first (or as the player sorts and filters them, the main first all the
//! same), each card its picture, name, types, level, status, HP, MP, Exp and base stats, its
//! pin while it's hovered; a card clicked turns the cards into thumbnails and shows that
//! creature in full: its lifeforce, its moves and their levels too. Clicking a creature
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
use crate::title::{button_label, menu_font, title_font, OptionsUi};

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
/// The details (a card clicked): the cards turned into thumbnails, three a row, on the left;
/// the details at least this tall; the turning, in seconds.
const THUMB_W: f32 = 76.0;
const THUMB_H: f32 = 92.0;
const THUMB_GAP: f32 = 8.0;
const THUMBS_PER_ROW: usize = 3;
const DETAIL_H: f32 = 600.0;
const MORPH_S: f32 = 0.3;
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
    /// its lifeforce: left, at most (one lost each time it's knocked out); none left: lost for
    /// good
    pub lifeforce: (u8, u8),
    /// its moves, in their order
    pub moves: Vec<MoveInfo>,
}

impl Card {
    /// Lost for good: no lifeforce left.
    pub fn lost(&self) -> bool {
        self.lifeforce.0 == 0
    }
}

/// One of a creature's moves, as its details show it.
#[derive(Clone, Debug, Default)]
pub struct MoveInfo {
    pub name: String,
    pub ty: Option<TypeTag>,
    /// "Physical", "Special" or "Status"
    pub category: &'static str,
    /// 0: none (a status move); accuracy 0: never misses
    pub power: u16,
    pub accuracy: u16,
    /// what using it costs, at its level
    pub mp: i32,
    /// its level (1 to 5), its XP, the XP its next level needs (none at the top)
    pub level: u8,
    pub xp: u32,
    pub next: Option<u32>,
    pub description: String,
    /// what each of its levels brings, 1 to 5
    pub levels: Vec<String>,
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
    /// the creature whose details show (a card clicked), by its id, and its move open there
    detail: Option<String>,
    move_open: usize,
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
    /// the Bag's window, and its own state
    bag_window: bool,
    bag_ui: crate::bag_ui::BagUi,
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
        self.focus.is_some() || self.options_open || self.team_window || self.bag_window
    }

    /// The HUD for a frame. `map`: the map's name; `story_busy`: a script or a dialog has the
    /// floor (the HUD waits, dimmed); `team`: the player's creatures in the team's order;
    /// `log`: their last battles and the main (pinned here). `options_changed` is set when the
    /// Options changed one.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ctx: &egui::Context, map: &str, story_busy: bool, team: &[Member], log: &mut TeamLog, bag: &mut crate::bag::Bag, bag_of: (&crate::bag::Catalogue, &content::Content), options: &mut Options, input: &Input, options_changed: &mut bool) -> Out {
        let enabled = !story_busy && !self.options_open && !self.team_window && !self.bag_window;
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
        let ((team_clicked, team_rect, list, bag_clicked), bottom) = BOTTOM_LEFT.show(ctx, enabled, |row| {
            let r = row.button(egui::include_image!("../icons/team.svg"), "Team: your creatures", TEAM);
            let list = (open > 0.0).then(|| team_list(row, open, team, &shown, log));
            let b = row.button(egui::include_image!("../icons/bag.svg"), "Bag: what you carry", crate::bag_ui::BAG);
            (r.clicked(), r.rect, list, b.clicked())
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
        // the Bag opens (the list closes), the A that opened it not its own
        if (bag_clicked || hit(crate::bag_ui::BAG)) && !self.bag_window {
            self.bag_window = true;
            self.bag_ui.fresh = true;
            self.team_open = false;
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
            // opened again, it shows the board
            if !self.team_window {
                self.board.detail = None;
            }
        }
        if self.bag_window {
            self.bag_window = crate::bag_ui::bag_window(ctx, bag, bag_of.0, bag_of.1, input, &mut self.bag_ui);
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
        let tint = if m.card.lost() { Color32::from_gray(80) } else { Color32::WHITE };
        tile(ui, slot_mark(slot), x, &|ui, r| {
            match m.icon {
                Some(t) => {
                    p.image(t, r.shrink(3.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), fade(tint));
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
    let does = if m.card.lost() {
        "lost for good: it fights no more".to_string()
    } else if log.main.as_deref() == Some(m.id.as_str()) {
        "your main, sent out first".to_string()
    } else {
        format!("{how}: pin as your main")
    };
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
        "LOST" => (Color32::from_rgb(64, 68, 80), Color32::from_white_alpha(230)),
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

/// A rect on its way from `a` to `b` (`t`: 0 to 1).
fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

/// Where a card's picture is in it: the top, 140 tall (room for the biggest, 64 by 64,
/// doubled).
fn card_pic(card: Rect) -> Rect {
    Rect::from_min_max(card.min + Vec2::splat(10.0), Pos2::new(card.right() - 10.0, card.top() + 150.0))
}

/// Where a thumbnail's picture is in it: all of it but its foot (its level there).
fn thumb_pic(thumb: Rect) -> Rect {
    Rect::from_min_max(thumb.min + Vec2::splat(5.0), Pos2::new(thumb.right() - 5.0, thumb.bottom() - 20.0))
}

/// A creature's picture in `room`: as big as fits, up to `most` times its pixels, whole ones
/// when it's settled (`whole`; on its way it scales smoothly), greyed when it can't fight.
fn paint_picture(p: &egui::Painter, c: &Card, room: Rect, most: f32, whole: bool, grey: bool) {
    let Some((tex, uv, px)) = c.picture.filter(|(_, _, px)| px.x > 0.0 && px.y > 0.0) else { return };
    let fit = room.shrink(6.0).size();
    let k = (fit.x / px.x).min(fit.y / px.y).min(most);
    let k = if whole && k >= 1.0 { k.floor() } else { k };
    p.image(tex, Rect::from_center_size(room.center(), px * k), uv, if grey { Color32::from_gray(80) } else { Color32::WHITE });
}

/// The pin on a creature's picture at `at`: white on the main, grey on another (brighter
/// while it's hovered, `hover` 0 to 1), where pressing it pins that one.
fn pin_badge(ui: &egui::Ui, at: Pos2, main: bool, hover: f32) {
    let (ground, ink) = if main { (Color32::WHITE, Color32::from_rgb(20, 24, 32)) } else { (Color32::from_gray(105).lerp_to_gamma(Color32::from_gray(170), hover), Color32::from_rgb(34, 38, 46)) };
    ui.painter().circle_filled(at, 9.0, ground);
    egui::Image::new(egui::include_image!("../icons/pin.svg")).fit_to_exact_size(Vec2::splat(32.0)).tint(ink).paint_at(ui, Rect::from_center_size(at, Vec2::splat(12.0)));
}

/// What a creature's status tag says: LOST (no lifeforce left), KO, its status, or nothing.
fn status_words(c: &Card) -> Option<String> {
    if c.lost() {
        Some("LOST".into())
    } else if c.hp.0 <= 0 {
        Some("KO".into())
    } else {
        c.status.clone()
    }
}

/// A creature on the board, `t` of the way from its card (`card`, 0) to its thumbnail
/// (`thumb`, 1): the frame and the picture go from one to the other, the card's words fading
/// out and the thumbnail's (its level) in. `hover`: how hovered, 0 to 1; `pin`: its pin shown
/// (the main's always, another's while it's hovered) and how hovered that is. Greyed when it
/// can't fight (KO, or lost for good).
#[allow(clippy::too_many_arguments)]
fn paint_item(ui: &egui::Ui, m: &Member, main: bool, card: Rect, thumb: Rect, t: f32, hover: f32, pin: Option<f32>) {
    let rect = lerp_rect(card, thumb, t);
    if !ui.is_rect_visible(rect) {
        return;
    }
    let c = &m.card;
    let down = c.hp.0 <= 0 || c.lost();
    let p = ui.painter_at(rect);
    let radius = 12.0 - 3.0 * t;
    p.rect_filled(rect, radius, Color32::from_rgb(29, 37, 54).lerp_to_gamma(Color32::from_rgb(40, 51, 74), hover));
    let edge = if main { Stroke::new(1.6, Color32::from_white_alpha(215)) } else { Stroke::new(1.0, Color32::from_white_alpha((28.0 + 50.0 * hover) as u8)) };
    p.rect_stroke(rect, radius, edge, egui::StrokeKind::Inside);
    let pic = lerp_rect(card_pic(card), thumb_pic(thumb), t);
    p.rect_filled(pic, 8.0 - 2.0 * t, Color32::from_black_alpha(45));
    paint_picture(&p, c, pic, 2.0, t <= 0.0 || t >= 1.0, down);
    let words = (1.0 - 2.0 * t).clamp(0.0, 1.0);
    if words > 0.0 {
        card_words(&p, m, card, words);
    }
    let level = (2.0 * t - 1.0).clamp(0.0, 1.0);
    if level > 0.0 {
        let ink = Color32::from_white_alpha(if down { 110 } else { 235 }).gamma_multiply(level);
        p.text(Pos2::new(rect.center().x, rect.bottom() - 10.0), Align2::CENTER_CENTER, format!("Lv {}", m.level), menu_font(12.0), ink);
    }
    if let Some(h) = pin {
        pin_badge(ui, pic.left_top() + Vec2::splat(12.0 - 3.0 * t), main, h);
    }
}

/// A card's words, `alpha` of them (they fade as it becomes a thumbnail): its status, KO or
/// LOST on its picture, its name and level (under them its number, and its kind's name when
/// it has its own), its types, HP, MP and Exp in bars, its kind's base stats.
fn card_words(p: &egui::Painter, m: &Member, card: Rect, alpha: f32) {
    let c = &m.card;
    let down = c.hp.0 <= 0 || c.lost();
    let (x0, x1) = (card.left() + 10.0, card.right() - 10.0);
    let dim = |a: u8| Color32::from_white_alpha(if down { a / 2 } else { a }).gamma_multiply(alpha);
    let pic = card_pic(card);
    if let Some(words) = status_words(c) {
        let (fill, ink) = status_colors(&words);
        tag(p, pic.left_bottom() + Vec2::new(6.0, -23.0), &words, fill.gamma_multiply(alpha), ink.gamma_multiply(alpha));
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
        let fill = if down { t.color.gamma_multiply(0.5) } else { t.color };
        x = tag(p, Pos2::new(x, y), &t.name, fill.gamma_multiply(alpha), ink_on(fill).gamma_multiply(alpha)).right() + 4.0;
    }
    y += 27.0;
    // HP, MP and Exp: a bar each, the numbers beside
    let bar = |y: f32, label: &str, share: f32, color: Color32, value: &str| {
        p.text(Pos2::new(x0, y), Align2::LEFT_CENTER, label, FontId::proportional(11.0), dim(165));
        p.text(Pos2::new(x1, y), Align2::RIGHT_CENTER, value, FontId::proportional(11.0), dim(255));
        let track = Rect::from_min_max(Pos2::new(x0 + 27.0, y - 2.5), Pos2::new(x1 - 54.0, y + 2.5));
        p.rect_filled(track, 2.5, Color32::from_white_alpha(28).gamma_multiply(alpha));
        let color = if down { color.gamma_multiply(0.5) } else { color };
        p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * share.clamp(0.0, 1.0), track.height())), 2.5, color.gamma_multiply(alpha));
    };
    bar(y, "HP", c.hp.0.max(0) as f32 / c.hp.1.max(1) as f32, hp_color(c), &if c.hp.0 <= 0 { "KO".to_string() } else { format!("{}/{}", c.hp.0, c.hp.1) });
    y += 16.0;
    bar(y, "MP", c.mp.0.max(0) as f32 / c.mp.1.max(1) as f32, MP_COLOR, &format!("{}/{}", c.mp.0, c.mp.1));
    y += 16.0;
    bar(y, "Exp", level_progress(c), EXP_COLOR, &thousands(c.exp.0));
    y += 17.0;
    // its kind's base stats: HP, Attack, Defense down the left, Sp. Atk, Sp. Def, Speed down the right
    let half = (x1 - x0 - 10.0) / 2.0;
    for (k, (label, v)) in STATS.iter().zip(c.base).enumerate() {
        let cx = x0 + (k / 3) as f32 * (half + 10.0);
        let cy = y + (k % 3) as f32 * 17.0;
        p.text(Pos2::new(cx, cy), Align2::LEFT_TOP, *label, FontId::proportional(10.5), dim(150));
        p.text(Pos2::new(cx + half, cy), Align2::RIGHT_TOP, v.to_string(), FontId::proportional(10.5), dim(235));
        let line = Rect::from_min_size(Pos2::new(cx, cy + 13.0), Vec2::new(half, 2.0));
        p.rect_filled(line, 1.0, Color32::from_white_alpha(22).gamma_multiply(alpha));
        p.rect_filled(Rect::from_min_size(line.min, Vec2::new(half * (v as f32 / 200.0).min(1.0), 2.0)), 1.0, dim(170));
    }
}

/// The base stats' short names, in the cards' order.
const STATS: [&str; 6] = ["HP", "Atk", "Def", "SpA", "SpD", "Spe"];
/// The MP's and the Exp's bars' colours.
const MP_COLOR: Color32 = Color32::from_rgb(92, 150, 236);
const EXP_COLOR: Color32 = Color32::from_rgb(170, 132, 236);
/// Lifeforce's colour.
const LIFE_COLOR: Color32 = Color32::from_rgb(112, 218, 196);

/// The HP bar's colour: green, yellow under half, scarlet under a fifth.
fn hp_color(c: &Card) -> Color32 {
    match c.hp.0.max(0) as f32 / c.hp.1.max(1) as f32 {
        h if h > 0.5 => Color32::from_rgb(92, 200, 120),
        h if h > 0.2 => Color32::from_rgb(232, 196, 58),
        _ => Color32::from_rgb(214, 72, 72),
    }
}

/// How far into its level a creature's Exp is, 0 to 1 (full at the top level).
fn level_progress(c: &Card) -> f32 {
    let (exp, from, to) = c.exp;
    to.map_or(1.0, |to| exp.saturating_sub(from) as f32 / to.saturating_sub(from).max(1) as f32)
}

/// A creature's place on the board (its card, or its thumbnail, at `rect`): clickable, and
/// its pin at `badge` while shown (the main's always; another's while the pointer is on it;
/// one lost for good can't be pressed). Its response, and its pin's: how hovered, whether it
/// was pressed, where it is.
fn interact_item(ui: &egui::Ui, m: &Member, main: bool, rect: Rect, badge: Pos2) -> (egui::Response, Option<(f32, bool, Rect)>) {
    let id = Id::new(("hud-team-item", m.id.as_str()));
    let r = ui.interact(rect, id, Sense::click());
    let lost = m.card.lost();
    let pin = (main || r.contains_pointer()).then(|| {
        let at = Rect::from_center_size(badge, Vec2::splat(22.0));
        let pr = ui.interact(at, id.with("pin"), if main || lost { Sense::hover() } else { Sense::click() });
        let tip = if main {
            "Your main, sent out first"
        } else if lost {
            "Lost for good: it can't be your main"
        } else {
            "Pin as your main"
        };
        let pr = pr.on_hover_text(tip);
        if pr.hovered() && !main && !lost {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        (ui.ctx().animate_bool_with_time(id.with("pin-hover"), pr.hovered(), 0.12), pr.clicked(), at)
    });
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (r, pin)
}

/// A bar across `w`: its label, how full (`share`, 0 to 1) in `color`, its value at its end;
/// where it is (for a tip).
fn bar_line(ui: &mut egui::Ui, w: f32, label: &str, share: f32, color: Color32, value: &str) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
    let p = ui.painter();
    p.text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(12.5), Color32::from_white_alpha(170));
    p.text(Pos2::new(r.right(), r.center().y), Align2::RIGHT_CENTER, value, FontId::proportional(12.5), Color32::WHITE);
    let track = Rect::from_min_max(Pos2::new(r.left() + 64.0, r.center().y - 3.5), Pos2::new(r.right() - 132.0, r.center().y + 3.5));
    p.rect_filled(track, 3.5, Color32::from_white_alpha(28));
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * share.clamp(0.0, 1.0), track.height())), 3.5, color);
    response
}

/// A title over a part of the details.
fn part_title(ui: &mut egui::Ui, words: &str) {
    ui.label(RichText::new(words).font(menu_font(14.0)).color(Color32::WHITE));
}

/// A creature's details, right of the thumbnails: its picture (three times its pixels; its
/// model, idle, once creatures have models), name, level, number, kind, types, status, HP,
/// MP and Exp, lifeforce and a button to pin it as the main; its kind's base stats; its moves
/// (a tab each, `open_move`'s details and levels shown). Its buttons, by their marks.
fn detail_panel(ui: &mut egui::Ui, m: &Member, main: bool, open_move: usize) -> Vec<(String, egui::Response)> {
    let mut out = vec![];
    let c = &m.card;
    let down = c.hp.0 <= 0 || c.lost();
    let soft = Color32::from_white_alpha(165);
    ui.horizontal_top(|ui| {
        // its picture, its status on it
        let (pic, _) = ui.allocate_exact_size(Vec2::splat(200.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(pic, 12.0, Color32::from_black_alpha(55));
        paint_picture(p, c, pic, 3.0, true, down);
        if let Some(words) = status_words(c) {
            let (fill, ink) = status_colors(&words);
            tag(p, pic.left_bottom() + Vec2::new(8.0, -25.0), &words, fill, ink);
        }
        ui.add_space(14.0);
        ui.vertical(|ui| {
            // its name and level; back to all the cards
            ui.horizontal(|ui| {
                ui.label(RichText::new(&m.name).font(title_font(24.0)).color(Color32::WHITE));
                ui.label(RichText::new(format!("Lv {}", m.level)).font(menu_font(16.0)).color(soft));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let back = icon_button(ui, egui::include_image!("../icons/all.svg"), "All the cards");
                    out.push(("uiTeamBack".to_string(), back));
                });
            });
            let under = if m.name != c.kind { format!("{} · No. {}", c.kind, c.number) } else { format!("No. {}", c.number) };
            ui.label(RichText::new(under).size(12.5).color(soft));
            ui.add_space(4.0);
            // its types
            let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
            let mut x = row.left();
            for t in &c.types {
                x = tag(ui.painter(), Pos2::new(x, row.top()), &t.name, t.color, ink_on(t.color)).right() + 5.0;
            }
            ui.add_space(8.0);
            // HP, MP and Exp
            let w = ui.available_width();
            bar_line(ui, w, "HP", c.hp.0.max(0) as f32 / c.hp.1.max(1) as f32, hp_color(c), &if c.hp.0 <= 0 { "KO".to_string() } else { format!("{} / {}", c.hp.0, c.hp.1) });
            bar_line(ui, w, "MP", c.mp.0.max(0) as f32 / c.mp.1.max(1) as f32, MP_COLOR, &format!("{} / {}", c.mp.0, c.mp.1));
            let exp = match c.exp.2 {
                Some(to) => format!("{} / {}", thousands(c.exp.0), thousands(to)),
                None => format!("{}: the top", thousands(c.exp.0)),
            };
            bar_line(ui, w, "Exp", level_progress(c), EXP_COLOR, &exp);
            ui.add_space(6.0);
            // its lifeforce, a gauge: some lost to each knock-out, none left lost for good
            let (left, max) = c.lifeforce;
            let words = if left == 0 { "0: lost for good".to_string() } else { format!("{left} / {max}") };
            let r = bar_line(ui, w, "Lifeforce", left as f32 / max.max(1) as f32, LIFE_COLOR, &words);
            r.on_hover_text("Some is lost each time it's knocked out in a battle; with none left it's lost for good");
            if c.lost() {
                ui.label(RichText::new("It can't be brought back but by a very rare consumable.").size(12.0).color(soft));
            }
            ui.add_space(6.0);
            // pinned as the main, or a button to (not one lost for good)
            if !c.lost() {
                let words = if main { "Your main, sent out first" } else { "Pin as your main" };
                let icon = egui::Image::new(egui::include_image!("../icons/pin.svg")).fit_to_exact_size(Vec2::splat(14.0)).tint(Color32::WHITE);
                let b = ui.add_enabled(!main, egui::Button::image_and_text(icon, RichText::new(words).color(Color32::WHITE)).min_size(Vec2::new(0.0, 26.0)));
                out.push(("uiTeamDetailPin".to_string(), b));
            }
        });
    });
    ui.add_space(12.0);
    // its kind's base stats, in a row
    let total: u32 = c.base.iter().map(|&b| b as u32).sum();
    ui.horizontal(|ui| {
        part_title(ui, "Base stats");
        ui.label(RichText::new(format!("{total} in all")).size(12.0).color(soft));
    });
    let w = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::hover());
    let col = (w - 5.0 * 12.0) / 6.0;
    for (k, (label, v)) in STATS.iter().zip(c.base).enumerate() {
        let x = row.left() + k as f32 * (col + 12.0);
        let p = ui.painter();
        p.text(Pos2::new(x, row.top()), Align2::LEFT_TOP, *label, FontId::proportional(12.0), soft);
        p.text(Pos2::new(x + col, row.top()), Align2::RIGHT_TOP, v.to_string(), FontId::proportional(12.0), Color32::WHITE);
        let line = Rect::from_min_size(Pos2::new(x, row.top() + 19.0), Vec2::new(col, 3.0));
        p.rect_filled(line, 1.5, Color32::from_white_alpha(25));
        p.rect_filled(Rect::from_min_size(line.min, Vec2::new(col * (v as f32 / 200.0).min(1.0), 3.0)), 1.5, Color32::from_white_alpha(190));
    }
    ui.add_space(8.0);
    // its moves: a tab each, the one open shown with its levels
    part_title(ui, "Moves");
    if c.moves.is_empty() {
        ui.label(RichText::new("No moves known.").size(12.5).color(soft));
    }
    ui.horizontal_wrapped(|ui| {
        for (k, mv) in c.moves.iter().enumerate() {
            // a dot of its type's colour before its name (room left for it)
            let r = ui.add(egui::Button::new(RichText::new(format!("        {}  ·  Lv {}", mv.name, mv.level)).color(Color32::WHITE)).selected(k == open_move).min_size(Vec2::new(0.0, 26.0)));
            ui.painter().circle_filled(Pos2::new(r.rect.left() + 12.0, r.rect.center().y), 4.5, mv.ty.as_ref().map_or(Color32::GRAY, |t| t.color));
            out.push((format!("uiTeamMove_{k}"), r));
        }
    });
    if let Some(mv) = c.moves.get(open_move) {
        ui.add_space(4.0);
        egui::Frame::new().fill(Color32::from_black_alpha(60)).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // what it is: its type, kind, power, accuracy, cost
            ui.horizontal(|ui| {
                if let Some(t) = &mv.ty {
                    let g = ui.painter().layout_no_wrap(t.name.clone(), FontId::proportional(11.0), Color32::WHITE).size().x;
                    let (at, _) = ui.allocate_exact_size(Vec2::new(g + 12.0, 18.0), Sense::hover());
                    tag(ui.painter(), at.min, &t.name, t.color, ink_on(t.color));
                }
                let facts = [
                    mv.category.to_string(),
                    if mv.power > 0 { format!("Power {}", mv.power) } else { "No power".to_string() },
                    if mv.accuracy > 0 { format!("Accuracy {}%", mv.accuracy) } else { "Never misses".to_string() },
                    format!("MP {}", mv.mp),
                ];
                ui.label(RichText::new(facts.join("  ·  ")).size(12.5).color(Color32::WHITE));
            });
            if !mv.description.is_empty() {
                ui.label(RichText::new(&mv.description).size(12.5).color(soft));
            }
            ui.add_space(6.0);
            // its levels: those reached lit, the one it's at marked
            egui::Grid::new(("hud-team-move-levels", m.id.as_str(), open_move)).num_columns(3).spacing([12.0, 5.0]).show(ui, |ui| {
                for (k, line) in mv.levels.iter().enumerate() {
                    let lv = k as u8 + 1;
                    let reached = lv <= mv.level;
                    let ink = if reached { Color32::WHITE } else { Color32::from_white_alpha(105) };
                    ui.label(RichText::new(format!("Lv {lv}")).font(menu_font(13.0)).color(ink));
                    ui.label(RichText::new(line).size(12.5).color(if reached { Color32::from_white_alpha(225) } else { Color32::from_white_alpha(105) }));
                    if lv == mv.level {
                        ui.label(RichText::new("now").size(11.5).color(LIFE_COLOR));
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            let words = match mv.next {
                Some(n) => format!("XP {} of {} for Lv {}", mv.xp, n, mv.level + 1),
                None => format!("XP {}: the top level", mv.xp),
            };
            ui.label(RichText::new(words).size(12.0).color(soft));
        });
    }
    out
}

/// A field's title in the toolbar: white, in Medium, as tall as the controls beside it.
fn field_label(ui: &mut egui::Ui, words: &str) {
    let w = ui.painter().layout_no_wrap(words.to_string(), menu_font(14.0), Color32::WHITE).size().x;
    ui.add_sized(Vec2::new(w, 24.0), egui::Label::new(RichText::new(words).font(menu_font(14.0)).color(Color32::WHITE)));
}

/// A thumbnail's mark, and a pin's: their places on the board.
fn thumb_mark(slot: usize) -> String {
    format!("uiTeamThumb_{slot}")
}

fn pin_mark(slot: usize) -> String {
    format!("uiTeamPin_{slot}")
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

/// The window of every creature the player has (View all), on frosted glass: a board of
/// cards, five a row, the main first, then as the Team's list has them (the first row is the
/// list's five), or as sorted (a dropdown) and filtered by type (a dropdown of checkboxes,
/// All and None at its top), the main first all the same. A card's pin (shown while it's
/// hovered) pins it as the main. A card clicked opens its details: the cards turn into
/// thumbnails, three a row on the left (one clicked shows its own), and the creature shows on
/// the right with its lifeforce, its moves (one picked shows its levels) and a button to pin
/// it. The game's buttons work it all: the arrows or the d-pad move a ring over the cards (or
/// the thumbnails), the controls and an open dropdown's choices, A presses, L and R go
/// through the sorts, B closes a dropdown, then the details, then the board; the info icon
/// says so. Close (its icon), Esc or a click beside it closes it: false then.
fn team_window(ctx: &egui::Context, team: &[Member], log: &mut TeamLog, input: &Input, board: &mut Board) -> bool {
    let mut open = true;
    let ids: Vec<String> = team.iter().map(|m| m.id.clone()).collect();
    // the details of one still in the team
    if board.detail.as_ref().is_some_and(|d| !ids.contains(d)) {
        board.detail = None;
    }
    // a dropdown open from before: Esc or B closes it, not the board; its button clicked
    // again closes it too
    let (sort_was, types_was) = (board.sort_open, board.types_open);
    let was_open = sort_was || types_was;
    let detail_was = board.detail.clone();
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
    // the details opening (the cards turning into thumbnails) or closing, 0 to 1
    let t = ctx.animate_bool_with_time(Id::new("hud-team-detail"), board.detail.is_some(), MORPH_S);
    let settled = t <= 0.0 || t >= 1.0;
    // the cards as wide as the screen has room for, up to their size; the board as tall as
    // its rows (scrolling past the room), the details at least as tall as theirs
    let screen = ctx.content_rect();
    let card_w = ((screen.width() - 110.0 - CARD_GAP * (PER_ROW - 1) as f32) / PER_ROW as f32).clamp(110.0, CARD_W);
    let width = card_w * PER_ROW as f32 + CARD_GAP * (PER_ROW - 1) as f32;
    let rows = order.len().div_ceil(PER_ROW).max(1);
    let grid_h = rows as f32 * CARD_H + (rows - 1) as f32 * CARD_GAP;
    let room = (screen.height() - 190.0).max(CARD_H + 8.0);
    let board_h = grid_h.min(room);
    let detail_h = grid_h.max(DETAIL_H).min(room.max(DETAIL_H));
    let area_h = board_h + (detail_h - board_h) * t;
    let left_w = THUMBS_PER_ROW as f32 * THUMB_W + (THUMBS_PER_ROW - 1) as f32 * THUMB_GAP;
    let thumbs_h = order.len().div_ceil(THUMBS_PER_ROW).max(1) as f32 * (THUMB_H + THUMB_GAP);
    let card_at = |origin: Pos2, slot: usize| Rect::from_min_size(origin + Vec2::new((slot % PER_ROW) as f32 * (card_w + CARD_GAP), (slot / PER_ROW) as f32 * (CARD_H + CARD_GAP)), Vec2::new(card_w, CARD_H));
    let thumb_at = |origin: Pos2, slot: usize| Rect::from_min_size(origin + Vec2::new((slot % THUMBS_PER_ROW) as f32 * (THUMB_W + THUMB_GAP), (slot / THUMBS_PER_ROW) as f32 * (THUMB_H + THUMB_GAP)), Vec2::new(THUMB_W, THUMB_H));
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
    // a creature's pin: its mark where it is, pressed
    let pinned = |slot: usize, pin: Option<(f32, bool, Rect)>, clicked: &mut Option<String>| {
        if let Some((_, pressed, at)) = pin {
            platform::set_hook(&pin_mark(slot), &place(at));
            if pressed {
                *clicked = Some(pin_mark(slot));
            }
        }
    };
    // the game's buttons, in words, for the info icon
    let b = |b: Button| button_label(b);
    let controls = format!(
        "The arrows or the d-pad: from one to the next\n{}: open a creature's details, or press\n{}, {}: change the sort\n{}: back (from a dropdown, from the details), then close\nA card's pin (it shows on the card): make it your main",
        b(Button::A),
        b(Button::L),
        b(Button::R),
        b(Button::B)
    );
    let modal = egui::Modal::new(Id::new("hud-team-window")).frame(egui::Frame::new().inner_margin(22.0)).show(ctx, |ui| {
        // the frosted glass behind it all, drawn once its size is known
        let glass = ui.painter().add(egui::Shape::Noop);
        ui.set_width(width);
        // the title, how many, the controls' words, Close
        ui.horizontal(|ui| {
            ui.label(RichText::new("Team").font(title_font(26.0)).color(Color32::WHITE));
            ui.add_space(6.0);
            let count = match (team.len(), order.len()) {
                (0, _) => "No creatures yet".to_string(),
                (n, shown) if shown < n => format!("{shown} of {n} shown"),
                (1, _) => "1 creature".to_string(),
                (n, _) => format!("{n} creatures"),
            };
            ui.label(RichText::new(count).color(Color32::from_white_alpha(185)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = icon_button(ui, egui::include_image!("../icons/close.svg"), "Close");
                stop(&mut stops, CLOSE.to_string(), &close, &mut clicked);
                ui.add_space(2.0);
                let info = icon_button(ui, egui::include_image!("../icons/info.svg"), &controls);
                platform::set_hook("uiTeamInfo", &place(info.rect));
            });
        });
        ui.add_space(8.0);
        // how they're sorted and which types show, in a box: a dropdown each
        let (sort_button, types_button) = egui::Frame::new()
            .stroke(Stroke::new(1.0, Color32::from_white_alpha(70)))
            .corner_radius(10.0)
            .inner_margin(egui::Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    field_label(ui, "Sort");
                    let sort = dropdown_button(ui, board.sort.label(), board.sort_open);
                    stop(&mut stops, SORT.to_string(), &sort, &mut clicked);
                    let r = ui.add(egui::Button::new(RichText::new("Reversed").color(Color32::WHITE)).selected(board.reverse).min_size(Vec2::new(0.0, 24.0))).on_hover_text("The other way round (the main stays first)");
                    stop(&mut stops, "uiTeamReverse".to_string(), &r, &mut clicked);
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                    field_label(ui, "Types");
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
                .inner
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
        // the cards, or the details (the thumbnails left, the creature right), or on the way
        let (area, _) = ui.allocate_exact_size(Vec2::new(width, area_h), Sense::hover());
        // the ring brought it here: in view
        let follow = |r: &egui::Response, name: &str, scrolled: &mut Option<String>| {
            if board.ring && board.focus.as_deref() == Some(name) && scrolled.as_deref() != Some(name) {
                r.scroll_to_me(None);
                *scrolled = Some(name.to_string());
            }
        };
        let mut scrolled = board.scrolled.clone();
        if t <= 0.0 {
            // the board: the cards, five a row
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            egui::ScrollArea::vertical().id_salt("hud-team-cards").max_height(area.height()).auto_shrink([false, false]).show(&mut child, |ui| {
                let (content, _) = ui.allocate_exact_size(Vec2::new(width, grid_h), Sense::hover());
                for (slot, &i) in order.iter().enumerate() {
                    let (m, is_main) = (&team[i], Some(i) == main);
                    let rect = card_at(content.min, slot);
                    let (r, pin) = interact_item(ui, m, is_main, rect, card_pic(rect).left_top() + Vec2::splat(12.0));
                    let hover = ui.ctx().animate_bool_with_time(r.id.with("hover"), r.contains_pointer(), 0.15);
                    paint_item(ui, m, is_main, rect, rect, 0.0, hover, pin.map(|p| p.0));
                    let name = card_mark(slot);
                    follow(&r, &name, &mut scrolled);
                    stop(&mut stops, name, &r, &mut clicked);
                    pinned(slot, pin, &mut clicked);
                }
                if order.is_empty() {
                    ui.put(content, egui::Label::new(RichText::new("Your creatures show here.").color(Color32::from_white_alpha(185))));
                }
            });
        } else if t >= 1.0 {
            // the details: the thumbnails, three a row, on the left
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(area.min, Vec2::new(left_w + 8.0, area.height()))));
            egui::ScrollArea::vertical().id_salt("hud-team-thumbs").max_height(area.height()).auto_shrink([false, false]).show(&mut left, |ui| {
                let (content, _) = ui.allocate_exact_size(Vec2::new(left_w, thumbs_h), Sense::hover());
                for (slot, &i) in order.iter().enumerate() {
                    let (m, is_main) = (&team[i], Some(i) == main);
                    let rect = thumb_at(content.min, slot);
                    let (r, pin) = interact_item(ui, m, is_main, rect, thumb_pic(rect).left_top() + Vec2::splat(9.0));
                    let shown = board.detail.as_deref() == Some(m.id.as_str());
                    let hover = ui.ctx().animate_bool_with_time(r.id.with("hover"), r.contains_pointer() || shown, 0.15);
                    paint_item(ui, m, is_main, rect, rect, 1.0, hover, pin.map(|p| p.0));
                    if shown {
                        ui.painter().rect_stroke(rect.expand(2.0), 11.0, Stroke::new(2.0, Color32::from_white_alpha(220)), egui::StrokeKind::Outside);
                    }
                    let name = thumb_mark(slot);
                    follow(&r, &name, &mut scrolled);
                    stop(&mut stops, name, &r, &mut clicked);
                    pinned(slot, pin, &mut clicked);
                }
            });
            // and the creature, on the right
            let panel = Rect::from_min_max(Pos2::new(area.left() + left_w + 18.0, area.top()), area.max);
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(panel));
            if let Some(m) = board.detail.as_ref().and_then(|d| team.iter().find(|m| &m.id == d)) {
                let is_main = main.is_some_and(|i| team[i].id == m.id);
                egui::Frame::new().fill(Color32::from_rgba_unmultiplied(18, 24, 38, 215)).corner_radius(12.0).inner_margin(14.0).stroke(Stroke::new(1.0, Color32::from_white_alpha(35))).show(&mut right, |ui| {
                    ui.set_min_size(panel.size() - Vec2::splat(28.0));
                    egui::ScrollArea::vertical().id_salt("hud-team-detail").auto_shrink([false, false]).show(ui, |ui| {
                        for (name, r) in detail_panel(ui, m, is_main, board.move_open) {
                            stop(&mut stops, name, &r, &mut clicked);
                        }
                    });
                });
            }
        } else {
            // on the way: the cards slide into their thumbnails' places (and back), the
            // details' panel fading in (or out); nothing to press meanwhile
            for (slot, &i) in order.iter().enumerate() {
                paint_item(ui, &team[i], Some(i) == main, card_at(area.min, slot), thumb_at(area.min, slot), t, 0.0, (Some(i) == main).then_some(0.0));
            }
            let panel = Rect::from_min_max(Pos2::new(area.left() + left_w + 18.0, area.top()), area.max);
            let fade = (2.0 * t - 1.0).clamp(0.0, 1.0);
            if fade > 0.0 {
                ui.painter().rect_filled(panel, 12.0, Color32::from_rgba_unmultiplied(18, 24, 38, (215.0 * fade) as u8));
            }
        }
        board.scrolled = scrolled;
        // the glass: frosted as the HUD's buttons, a little darker for the words on it
        let r = ui.min_rect().expand(22.0);
        let mut shapes = layout::frosted_shapes(r, 18.0, 0.0);
        let edge = shapes.len() - 1;
        shapes.insert(edge, egui::epaint::RectShape::filled(r, 18.0, Color32::from_black_alpha(95)).into());
        ui.painter().set(glass, egui::Shape::Vec(shapes));
    });
    // the ring (not while the cards are on their way): over an open dropdown's choices, else
    // the board's (onto the first card, or the thumbnail shown, if it's nowhere yet); moved,
    // pressed
    let popup = board.sort_open || board.types_open;
    if board.ring && settled {
        let places = if popup && !pops.is_empty() { &pops } else { &stops };
        let at = board.focus.as_ref().and_then(|f| places.iter().position(|s| &s.name == f));
        let first = if board.detail.is_some() { thumb_mark(0) } else { card_mark(0) };
        let at = match at {
            Some(at) => moved.map_or(at, |d| layout::step(places, at, d)),
            None if popup => 0,
            None => places.iter().position(|s| s.name == first).unwrap_or(0),
        };
        if let Some(s) = places.get(at) {
            board.focus = Some(s.name.clone());
            layout::ring(ctx, s.rect);
            if press && clicked.is_none() {
                clicked = Some(s.name.clone());
            }
        }
    }
    // a creature's slot on the board, by its id; the one at a mark's slot
    let slot_of = |id: &str| order.iter().position(|&i| team[i].id == id);
    let at_slot = |name: &str, prefix: &str| name.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok()).and_then(|slot| order.get(slot)).map(|&i| &team[i]);
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
        Some("uiTeamBack") => {
            board.focus = board.detail.as_deref().and_then(slot_of).map(card_mark);
            board.detail = None;
        }
        Some("uiTeamDetailPin") => {
            if let Some(d) = board.detail.clone() {
                log.pin(&d);
            }
        }
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
        Some(name) if name.starts_with("uiTeamPin_") => {
            if let Some(m) = at_slot(name, "uiTeamPin_") {
                log.pin(&m.id);
            }
        }
        Some(name) if name.starts_with("uiTeamMove_") => {
            if let Some(k) = name.strip_prefix("uiTeamMove_").and_then(|n| n.parse::<usize>().ok()) {
                board.move_open = k;
            }
        }
        Some(name) if name.starts_with("uiTeamCard_") || name.starts_with("uiTeamThumb_") => {
            let card = name.starts_with("uiTeamCard_");
            if let Some(m) = at_slot(name, if card { "uiTeamCard_" } else { "uiTeamThumb_" }) {
                if board.detail.as_deref() != Some(m.id.as_str()) {
                    board.move_open = 0;
                }
                board.detail = Some(m.id.clone());
                // the ring onto its thumbnail
                if card && board.ring {
                    board.focus = slot_of(&m.id).map(thumb_mark);
                }
            }
        }
        _ => {}
    }
    // Esc or B: a dropdown closes (the ring back on its button), else the details (the ring
    // back on its card), else the board; a click beside it closes it
    if was_open && (back || modal.should_close()) {
        if board.focus.as_deref().is_some_and(|f| f.starts_with("uiTeamSort_")) {
            board.focus = Some(SORT.to_string());
        } else if board.focus.as_deref().is_some_and(|f| f.starts_with("uiTeamType")) {
            board.focus = Some(TYPES.to_string());
        }
        (board.sort_open, board.types_open) = (false, false);
    } else if back && detail_was.is_some() {
        board.focus = detail_was.as_deref().and_then(slot_of).map(card_mark);
        board.detail = None;
    } else if modal.should_close() || back {
        open = false;
    }
    let shown = board.detail.as_ref().and_then(|d| team.iter().find(|m| &m.id == d));
    platform::set_hook("hudboard", &order.iter().map(|&i| team[i].id.as_str()).collect::<Vec<_>>().join(","));
    platform::set_hook("hudboardlevels", &order.iter().map(|&i| team[i].level.to_string()).collect::<Vec<_>>().join(","));
    platform::set_hook("hudboardsort", &format!("{}{}", board.sort.label(), if board.reverse { " reversed" } else { "" }));
    platform::set_hook("hudboardtypes", &shown_types.join(","));
    platform::set_hook("hudboardpopup", if board.sort_open { "sort" } else if board.types_open { "types" } else { "" });
    platform::set_hook("hudboardring", if board.ring { board.focus.as_deref().unwrap_or("") } else { "" });
    platform::set_hook("hudboarddetail", shown.map_or("", |m| m.id.as_str()));
    platform::set_hook("hudboardmove", &shown.and_then(|m| m.card.moves.get(board.move_open)).map(|mv| format!("{}:{}:{}", mv.name, mv.level, mv.levels.len())).unwrap_or_default());
    platform::set_hook("hudboardlost", &team.iter().filter(|m| m.card.lost()).map(|m| m.id.as_str()).collect::<Vec<_>>().join(","));
    platform::set_hook("hudboardlife", &shown.map(|m| format!("{}/{}", m.card.lifeforce.0, m.card.lifeforce.1)).unwrap_or_default());
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
