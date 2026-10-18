//! The battle menu, classic RPG style: Attack / Moves / Party / Bag / Run, then target
//! selection with the move's shapes (Focus / Chain / Storm) on L/R. Keyboard first (the
//! GBA buttons), the mouse works too.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use rules::battle::{Action, Battle, BattleKind, Item, Side, Uid};
use rules::moves::{describe_levels, input_text, level_from_xp, modes, timed_text, xp_for_level, Mode, Track};

use crate::hud::{self, Hud, ACCENT, MUTED, PANEL, PANEL_2, TEXT};
use crate::input::{Button, Input};
use crate::stage::Stage;

#[derive(Clone, Copy, PartialEq)]
enum Purpose {
    Attack,
    Move(usize),
    Item(Item),
}

#[derive(Clone)]
struct Pick {
    side: Side,
    modes: Vec<Mode>,
    mode: usize,
    i: usize,
    purpose: Purpose,
}

#[derive(Clone)]
enum Page {
    Main,
    Moves,
    Target(Pick),
    Party,
    PartyAction(usize),
    Bag,
    Revive(Item),
}

pub struct Menu {
    page: Page,
    sel: usize,
    main_sel: usize,
    move_sel: usize,
    menu_height: f32,
}

struct Row {
    label: String,
    disabled: bool,
    kind: RowKind,
}

enum RowKind {
    Text,
    Move { color: u32, label: String, level: u8, cost: i32, short: bool },
    Mon { hp: String, note: &'static str },
}

impl Menu {
    pub fn new() -> Menu {
        Menu { page: Page::Main, sel: 0, main_sel: 0, move_sel: 0, menu_height: 200.0 }
    }

    /// Back to the first page, on the entry last chosen.
    pub fn reset(&mut self) {
        self.page = Page::Main;
        self.sel = self.main_sel;
    }

    /// A new turn starts on Attack.
    pub fn new_turn(&mut self) {
        self.main_sel = 0;
        self.reset();
    }

    /// The player's choice for `actor`, once made.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, ctx: &egui::Context, input: &Input, battle: &Battle, actor: Uid, stage: &mut Stage, hud: &mut Hud) -> Option<Action> {
        let page = self.page.clone();
        match page {
            Page::Main => {
                let mut rows = vec!["Attack", "Moves", "Party", "Bag"];
                if battle.kind == BattleKind::Wild {
                    rows.push("Run");
                }
                let tips = [
                    format!(
                        "Basic attack ({}). Free, gives back {} MP (+{} timed). Timed: a second hit. Breaks matching locks.",
                        battle.dex.types[battle.types(actor)[0] as usize], battle.config.basic_restore, battle.config.timed_restore
                    ),
                    "Use a move. Costs MP. Moves level up as you use them (more with timed hits).".into(),
                    if battle.kind == BattleKind::Trainer { "Swap a creature in.".into() } else { "Send another creature onto the field (up to 6), or swap one in.".into() },
                    "Potions, Ethers, Capture Balls...".into(),
                    "Try to get away.".into(),
                ];
                let title = format!("What will {} do?", battle.mon(actor).name);
                let items: Vec<Row> = rows.iter().map(|r| Row { label: r.to_string(), disabled: false, kind: RowKind::Text }).collect();
                let tip = tips[self.sel.min(tips.len() - 1)].clone();
                self.info_text(ctx, &tip);
                match self.list(ctx, input, &title, &items) {
                    Some(Ok(i)) => {
                        self.main_sel = i;
                        self.sel = 0;
                        match rows[i] {
                            "Attack" => self.page = Page::Target(Pick { side: Side::Foe, modes: vec![Mode::Focus], mode: 0, i: usize::MAX, purpose: Purpose::Attack }),
                            "Moves" => {
                                self.page = Page::Moves;
                                self.sel = self.move_sel;
                            }
                            "Party" => self.page = Page::Party,
                            "Bag" => self.page = Page::Bag,
                            _ => return Some(Action::Run),
                        }
                    }
                    Some(Err(())) => {}
                    None => {}
                }
            }
            Page::Moves => {
                let m = battle.mon(actor);
                let items: Vec<Row> = (0..m.moves.len())
                    .map(|i| {
                        let mv = battle.move_data(actor, i);
                        let cost = battle.move_cost(actor, i, &[]);
                        let short = m.mp < cost;
                        Row {
                            label: mv.name.clone(),
                            disabled: short || !battle.dex.supported(mv.id),
                            kind: RowKind::Move { color: battle.dex.type_color(mv.ty), label: battle.dex.type_short(mv.ty).to_string(), level: battle.move_level(actor, i), cost, short },
                        }
                    })
                    .collect();
                self.move_info(ctx, battle, actor, self.sel.min(items.len().saturating_sub(1)));
                match self.list(ctx, input, &format!("{}'s moves (MP {}/{})", m.name, m.mp, m.max_mp), &items) {
                    Some(Ok(i)) => {
                        self.move_sel = i;
                        let id = m.moves[i].id;
                        let prof = battle.dex.profile(id).unwrap();
                        let level = battle.move_level(actor, i);
                        let ms = modes(prof, level);
                        if ms == [Mode::SelfOnly] {
                            self.reset();
                            return Some(Action::Move { slot: i, target: None, mode: Some(Mode::SelfOnly), cast: false });
                        }
                        let side = if ms.contains(&Mode::Ally) || ms[0] == Mode::SelfOnly { Side::Player } else { Side::Foe };
                        self.page = Page::Target(Pick { side, modes: ms, mode: 0, i: usize::MAX, purpose: Purpose::Move(i) });
                    }
                    Some(Err(())) => self.reset(),
                    None => {}
                }
            }
            Page::Target(mut pick) => {
                let field = battle.field(pick.side).to_vec();
                if field.is_empty() {
                    self.reset();
                    return None;
                }
                if pick.i == usize::MAX {
                    pick.i = if pick.side == Side::Foe { (field.len() - 1) / 2 } else { field.iter().position(|&u| u == actor).unwrap_or(0) };
                }
                let n = field.len();
                pick.i = pick.i.min(n - 1);
                if input.pressed(Button::Left) {
                    pick.i = (pick.i + n - 1) % n;
                }
                if input.pressed(Button::Right) {
                    pick.i = (pick.i + 1) % n;
                }
                let nm = pick.modes.len();
                if input.pressed(Button::L) || input.pressed(Button::Up) {
                    pick.mode = (pick.mode + nm - 1) % nm;
                }
                if input.pressed(Button::R) || input.pressed(Button::Down) {
                    pick.mode = (pick.mode + 1) % nm;
                }
                let center = field[pick.i];
                let mode = pick.modes[pick.mode];
                let (area, describe) = match pick.purpose {
                    Purpose::Move(slot) => {
                        let id = battle.mon(actor).moves[slot].id;
                        let prof = battle.dex.profile(id).unwrap();
                        let level = battle.move_level(actor, slot);
                        let hit = battle.targets_for(actor, prof, mode, Some(center), level);
                        let spread = if prof.track == Track::Wave { &battle.config.spread_wave } else { &battle.config.spread_strike };
                        let mul = match mode {
                            Mode::Chain => spread.chain,
                            Mode::Storm => spread.storm,
                            _ => spread.focus,
                        };
                        let damage = prof.kind == rules::moves::Kind::Damage && prof.track != Track::Multi;
                        let allies = hit.iter().filter(|&&t| battle.get(t).side == battle.get(actor).side && t != actor).count();
                        let mut text = format!("{}: {} target{}", mode.name(), hit.len(), if hit.len() > 1 { "s" } else { "" });
                        if damage && mul != 1.0 {
                            text += &format!(" at {}%", (mul * 100.0).round());
                        }
                        if allies > 0 {
                            text += &format!(" (hits {allies} of yours!)");
                        }
                        (hit, text)
                    }
                    Purpose::Attack => {
                        let ty = battle.types(actor)[0];
                        let eff = battle.dex.effectiveness(ty, battle.types(center));
                        let what = if eff == 0.0 { " · no effect" } else if eff > 1.0 { " · super effective" } else if eff < 1.0 { " · not very effective" } else { "" };
                        (vec![center], format!("{} basic attack{what} · {}", battle.dex.types[ty as usize], input_text(rules::moves::InputKind::Tap)))
                    }
                    Purpose::Item(_) => {
                        let m = battle.mon(center);
                        (vec![center], format!("{}: {}/{} HP, {}/{} MP", m.name, m.hp, m.stats.hp, m.mp, m.max_mp))
                    }
                };
                let color = if pick.side == Side::Foe { [1.0, 0.83, 0.29, 0.95] } else { [0.5, 0.88, 0.54, 0.95] };
                stage.targets = area.iter().map(|&u| (u, u == center, color)).collect();
                hud.pointer = Some(center);
                // the panel
                let mut clicked_mode = None;
                let shown = Rect::from_min_size(Pos2::ZERO, Vec2::new(330.0, 0.0));
                let _ = shown;
                egui::Area::new(egui::Id::new("menu"))
                    .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
                    .show(ctx, |ui| {
                        egui::Frame::new().fill(PANEL).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                            ui.set_width(310.0);
                            ui.label(egui::RichText::new(format!("Target: {}", battle.mon(center).name)).color(MUTED).size(12.0));
                            if pick.modes.len() > 1 || pick.modes[0] != Mode::Focus {
                                ui.horizontal(|ui| {
                                    for (k, m) in pick.modes.iter().enumerate() {
                                        let sel = k == pick.mode;
                                        let b = egui::Button::new(egui::RichText::new(m.name()).color(if sel { Color32::from_rgb(34, 34, 34) } else { TEXT }).strong())
                                            .fill(if sel { ACCENT } else { Color32::from_white_alpha(20) })
                                            .corner_radius(10.0);
                                        if ui.add(b).clicked() {
                                            clicked_mode = Some(k);
                                        }
                                    }
                                });
                                if pick.modes.len() > 1 {
                                    let keys = crate::title::pair(&crate::input::bindings(), crate::title::pad_shown(), Button::L, Button::R);
                                    ui.label(egui::RichText::new(format!("L / R ({keys}): change shape")).color(MUTED).size(12.0));
                                }
                            }
                            ui.label(egui::RichText::new(&describe).color(MUTED).size(12.0));
                            ui.label(egui::RichText::new("◀ ▶ choose · A confirm · B back").color(MUTED).size(12.0));
                        });
                    });
                if let Some(k) = clicked_mode {
                    pick.mode = k;
                }
                let confirm = input.pressed(Button::A);
                let back = input.pressed(Button::B);
                let mode = pick.modes[pick.mode];
                let purpose = pick.purpose;
                self.page = Page::Target(pick);
                if back {
                    stage.targets.clear();
                    hud.pointer = None;
                    self.page = match purpose {
                        Purpose::Move(_) => Page::Moves,
                        Purpose::Item(_) => Page::Bag,
                        Purpose::Attack => Page::Main,
                    };
                    self.sel = match purpose {
                        Purpose::Move(i) => i,
                        _ => self.main_sel,
                    };
                } else if confirm {
                    stage.targets.clear();
                    hud.pointer = None;
                    self.reset();
                    return Some(match purpose {
                        Purpose::Attack => Action::Attack { target: Some(center) },
                        Purpose::Move(slot) => Action::Move { slot, target: Some(center), mode: Some(mode), cast: false },
                        Purpose::Item(item) => Action::Item { item, target: Some(center), mon: None },
                    });
                }
            }
            Page::Party => {
                let party = &battle.side(Side::Player).party;
                let on_field = |i: usize| battle.field(Side::Player).iter().any(|&u| battle.get(u).mon == i);
                let items: Vec<Row> = party
                    .iter()
                    .enumerate()
                    .map(|(i, m)| Row {
                        label: format!("{} Lv{}", m.name, m.level),
                        disabled: m.hp <= 0 || on_field(i),
                        kind: RowKind::Mon {
                            hp: if m.hp > 0 { format!("{}/{} HP", m.hp, m.stats.hp) } else { "Fainted".into() },
                            note: if on_field(i) { "On the field" } else { "" },
                        },
                    })
                    .collect();
                self.info_text(ctx, "");
                match self.list(ctx, input, "Party", &items) {
                    Some(Ok(i)) => {
                        self.page = Page::PartyAction(i);
                        self.sel = 0;
                    }
                    Some(Err(())) => self.reset(),
                    None => {}
                }
            }
            Page::PartyAction(mon) => {
                let can_send = battle.kind != BattleKind::Trainer && battle.field(Side::Player).len() < battle.config.max_field;
                let name = battle.side(Side::Player).party[mon].name.clone();
                let n = battle.field(Side::Player).len();
                let mut rows: Vec<(String, bool)> = vec![];
                if can_send {
                    rows.push((format!("Send {name} out (your field: {n} to {})", n + 1), true));
                }
                rows.push((format!("Swap {} for {name}", battle.mon(actor).name), false));
                let tip = match battle.kind {
                    BattleKind::Gym => "Gym rule: the leader sends one more too.",
                    BattleKind::Wild => "More of yours on the field = more hits per round, but wild creatures call for help too.",
                    BattleKind::Trainer => "",
                };
                self.info_text(ctx, tip);
                let items: Vec<Row> = rows.iter().map(|(l, _)| Row { label: l.clone(), disabled: false, kind: RowKind::Text }).collect();
                match self.list(ctx, input, &name, &items) {
                    Some(Ok(i)) => {
                        self.reset();
                        return Some(if rows[i].1 { Action::Send { mon } } else { Action::Swap { mon } });
                    }
                    Some(Err(())) => {
                        self.page = Page::Party;
                        self.sel = 0;
                    }
                    None => {}
                }
            }
            Page::Bag => {
                let bag = &battle.side(Side::Player).bag;
                let keys: Vec<Item> = Item::ALL.iter().copied().filter(|k| bag.get(k).copied().unwrap_or(0) > 0).collect();
                if keys.is_empty() {
                    self.reset();
                    return None;
                }
                let items: Vec<Row> = keys
                    .iter()
                    .map(|k| Row { label: format!("{} ×{}", k.info().name, bag[k]), disabled: k.info().ball > 0.0 && battle.kind != BattleKind::Wild, kind: RowKind::Text })
                    .collect();
                let tip = keys[self.sel.min(keys.len() - 1)].info().text;
                self.info_text(ctx, tip);
                match self.list(ctx, input, "Bag", &items) {
                    Some(Ok(i)) => {
                        let item = keys[i];
                        let info = item.info();
                        if info.ball > 0.0 {
                            self.page = Page::Target(Pick { side: Side::Foe, modes: vec![Mode::Focus], mode: 0, i: usize::MAX, purpose: Purpose::Item(item) });
                        } else if info.revive > 0.0 {
                            self.page = Page::Revive(item);
                            self.sel = 0;
                        } else {
                            self.page = Page::Target(Pick { side: Side::Player, modes: vec![Mode::Focus], mode: 0, i: usize::MAX, purpose: Purpose::Item(item) });
                        }
                    }
                    Some(Err(())) => self.reset(),
                    None => {}
                }
            }
            Page::Revive(item) => {
                let party = &battle.side(Side::Player).party;
                let fainted: Vec<usize> = (0..party.len()).filter(|&i| party[i].hp <= 0).collect();
                if fainted.is_empty() {
                    self.reset();
                    return None;
                }
                let items: Vec<Row> = fainted.iter().map(|&i| Row { label: format!("{} Lv{}", party[i].name, party[i].level), disabled: false, kind: RowKind::Text }).collect();
                self.info_text(ctx, "");
                match self.list(ctx, input, "Revive who?", &items) {
                    Some(Ok(i)) => {
                        self.reset();
                        return Some(Action::Item { item, target: None, mon: Some(fainted[i]) });
                    }
                    Some(Err(())) => {
                        self.page = Page::Bag;
                        self.sel = 0;
                    }
                    None => {}
                }
            }
        }
        None
    }

    /// Choosing who replaces a fainted creature.
    pub fn replacement(&mut self, ctx: &egui::Context, input: &Input, battle: &Battle) -> Option<usize> {
        let bench = battle.bench(Side::Player);
        let party = &battle.side(Side::Player).party;
        let items: Vec<Row> = bench.iter().map(|&i| Row { label: format!("{} Lv{} · {}/{} HP", party[i].name, party[i].level, party[i].hp, party[i].stats.hp), disabled: false, kind: RowKind::Text }).collect();
        self.info_text(ctx, "");
        match self.list(ctx, input, "Send out which creature?", &items) {
            Some(Ok(i)) => {
                self.sel = 0;
                Some(bench[i])
            }
            _ => None,
        }
    }

    /// A list: Some(Ok(index)) when chosen, Some(Err) for B.
    fn list(&mut self, ctx: &egui::Context, input: &Input, title: &str, items: &[Row]) -> Option<Result<usize, ()>> {
        let n = items.len().max(1);
        self.sel = self.sel.min(n - 1);
        if input.pressed(Button::Up) {
            self.sel = (self.sel + n - 1) % n;
        }
        if input.pressed(Button::Down) {
            self.sel = (self.sel + 1) % n;
        }
        let mut chosen = None;
        if input.pressed(Button::A) && !items.is_empty() && !items[self.sel].disabled {
            chosen = Some(Ok(self.sel));
        }
        if input.pressed(Button::B) {
            chosen = Some(Err(()));
        }
        let response = egui::Area::new(egui::Id::new("menu")).anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0]).show(ctx, |ui| {
            egui::Frame::new().fill(PANEL).corner_radius(12.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(314.0);
                ui.label(egui::RichText::new(title).color(MUTED).size(12.0));
                ui.add_space(2.0);
                for (i, row) in items.iter().enumerate() {
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(314.0, 30.0), Sense::click());
                    if r.hovered() && ui.ctx().input(|x| x.pointer.delta() != Vec2::ZERO) {
                        self.sel = i;
                    }
                    if r.clicked() && !row.disabled {
                        self.sel = i;
                        chosen = Some(Ok(i));
                    }
                    let p = ui.painter();
                    if i == self.sel {
                        p.rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(255, 213, 74, 46));
                        p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 1.0, ACCENT);
                    }
                    let alpha = if row.disabled { 0.4 } else { 1.0 };
                    let y = rect.center().y;
                    match &row.kind {
                        RowKind::Text => {
                            p.text(Pos2::new(rect.left() + 10.0, y), Align2::LEFT_CENTER, &row.label, FontId::proportional(14.0), TEXT.gamma_multiply(alpha));
                        }
                        RowKind::Move { color, label, level, cost, short } => {
                            let w = hud::type_pill(p, Pos2::new(rect.left() + 8.0, y - 7.0), *color, label, 9.0, false);
                            p.text(Pos2::new(rect.left() + 14.0 + w, y), Align2::LEFT_CENTER, &row.label, FontId::proportional(14.0), TEXT.gamma_multiply(alpha));
                            for k in 0..5 {
                                let c = Pos2::new(rect.right() - 96.0 + k as f32 * 9.0, y);
                                if k < *level as usize {
                                    p.circle_filled(c, 3.2, ACCENT);
                                } else {
                                    p.circle_stroke(c, 3.0, Stroke::new(1.0, ACCENT.gamma_multiply(0.6)));
                                }
                            }
                            let cost_color = if *short { hud::LOW } else { Color32::from_rgb(156, 196, 255) };
                            p.text(Pos2::new(rect.right() - 8.0, y), Align2::RIGHT_CENTER, format!("{cost} MP"), FontId::proportional(11.0), cost_color);
                        }
                        RowKind::Mon { hp, note } => {
                            p.text(Pos2::new(rect.left() + 10.0, y), Align2::LEFT_CENTER, &row.label, FontId::proportional(14.0), TEXT.gamma_multiply(alpha));
                            p.text(Pos2::new(rect.right() - 8.0, y), Align2::RIGHT_CENTER, format!("{hp}  {note}"), FontId::proportional(11.0), MUTED);
                        }
                    }
                }
            });
        });
        self.menu_height = response.response.rect.height();
        chosen
    }

    fn info_frame(&self, ctx: &egui::Context, add: impl FnOnce(&mut egui::Ui)) {
        egui::Area::new(egui::Id::new("menu-info")).anchor(Align2::RIGHT_BOTTOM, [-16.0, -(self.menu_height + 24.0)]).show(ctx, |ui| {
            egui::Frame::new().fill(PANEL_2).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(310.0);
                add(ui);
            });
        });
    }

    fn info_text(&self, ctx: &egui::Context, text: &str) {
        if text.is_empty() {
            return;
        }
        self.info_frame(ctx, |ui| {
            ui.label(egui::RichText::new(text).color(TEXT).size(12.0));
        });
    }

    fn move_info(&self, ctx: &egui::Context, battle: &Battle, actor: Uid, slot: usize) {
        let m = battle.mon(actor);
        let Some(s) = m.moves.get(slot) else { return };
        let mv = battle.dex.mv(s.id);
        let prof = battle.dex.profile(s.id);
        let level = level_from_xp(s.xp, &battle.config);
        let cost = battle.move_cost(actor, slot, &[]);
        self.info_frame(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(310.0, 18.0), Sense::hover());
            let p = ui.painter();
            let mut x = rect.left();
            x += hud::type_pill(p, Pos2::new(x, rect.top() + 1.0), battle.dex.type_color(mv.ty), &battle.dex.types[mv.ty as usize], 10.0, false) + 6.0;
            let (cat, color) = match mv.category {
                rules::dex::Category::Physical => ("Physical", Color32::from_rgb(200, 73, 47)),
                rules::dex::Category::Special => ("Special", Color32::from_rgb(74, 98, 201)),
                rules::dex::Category::Status => ("Status", Color32::from_rgb(119, 119, 119)),
            };
            let g = p.layout_no_wrap(cat.to_string(), FontId::proportional(10.0), Color32::WHITE);
            let r = Rect::from_min_size(Pos2::new(x, rect.top() + 1.0), g.size() + Vec2::new(8.0, 2.0));
            p.rect_filled(r, 3.0, color);
            p.galley(r.min + Vec2::new(4.0, 1.0), g, Color32::WHITE);
            x = r.right() + 6.0;
            let pw = if mv.power > 0 { mv.power.to_string() } else { "—".into() };
            let ac = if mv.accuracy > 0 { mv.accuracy.to_string() } else { "—".into() };
            p.text(Pos2::new(x, rect.top() + 2.0), Align2::LEFT_TOP, format!("Power {pw} · Acc {ac} · {cost} MP"), FontId::proportional(11.0), MUTED);
            if let Some(prof) = prof {
                ui.label(egui::RichText::new(format!("{} · {}", input_text(prof.input), timed_text(prof))).color(Color32::from_rgb(181, 224, 255)).size(12.0));
            }
            for (i, line) in describe_levels(prof, &battle.config).iter().enumerate() {
                let lv = i as u8 + 1;
                let color = if lv <= level { TEXT } else if lv == level + 1 { ACCENT } else { MUTED };
                ui.label(egui::RichText::new(format!("Lv {lv}: {line}")).color(color).size(12.0));
            }
            if level < 5 {
                let need = xp_for_level(level + 1, &battle.config);
                let from = xp_for_level(level, &battle.config);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(310.0, 12.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 6.0, Color32::from_white_alpha(26));
                let f = (s.xp - from) as f32 / (need - from) as f32;
                p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, 12.0)), 6.0, ACCENT);
                p.text(rect.center(), Align2::CENTER_CENTER, format!("{} / {} XP to Lv {}", s.xp - from, need - from, level + 1), FontId::proportional(9.0), Color32::WHITE);
                p.rect_stroke(rect, 6.0, Stroke::new(0.5, Color32::from_white_alpha(40)), StrokeKind::Inside);
            }
        });
    }
}
