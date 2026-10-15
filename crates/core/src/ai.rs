//! What a computer-controlled creature does on its turn: score every option, keep
//! some randomness, telegraph big moves with locks.

use crate::battle::{Action, Battle, BattleKind, Plan, Side, Timing, Uid};
use crate::dex::{Category, MoveData, Rng};
use crate::moves::{modes, Ailment, FieldKind, InputKind, Kind, Mode, Profile, Status};

pub fn choose_action(battle: &mut Battle, u: Uid) -> Action {
    let side = battle.get(u).side;
    let foes = battle.field(side.other()).to_vec();
    if foes.is_empty() {
        return Action::Attack { target: None };
    }
    let cfg = battle.config.clone();

    // wild creatures call for help, more often when hurt
    if battle.kind == BattleKind::Wild && side == Side::Foe && battle.field(Side::Foe).len() < cfg.max_field {
        let m = battle.mon(u);
        let hurt = (m.hp as f32 / m.stats.hp as f32) < cfg.wild_call_below_hp;
        let p = if hurt { cfg.wild_call_chance } else { cfg.wild_call_chance / 4.0 };
        if battle.rng.chance(p) {
            return Action::Call;
        }
    }

    let mut options: Vec<(f64, Action)> = vec![];
    // the basic attack: weak but free, worth more when MP is low
    let basic_ty = battle.types(u)[0];
    let basic_cat = battle.basic_category(u);
    for &t in &foes {
        let dmg = estimate_basic(battle, u, t, basic_ty, basic_cat, cfg.basic_power);
        let m = battle.mon(u);
        let low_mp = if (m.mp as f32) < m.max_mp as f32 * 0.3 { 0.35 } else { 0.0 };
        options.push((value(battle, dmg, t) + low_mp, Action::Attack { target: Some(t) }));
    }

    let slots = battle.mon(u).moves.len();
    for i in 0..slots {
        let id = battle.mon(u).moves[i].id;
        let Some(prof) = battle.dex.profile(id).cloned() else { continue };
        if battle.mon(u).mp < battle.move_cost(u, i, &[]) {
            continue;
        }
        let mv = battle.dex.mv(id).clone();
        let level = battle.move_level(u, i);
        for mode in modes(&prof, level) {
            let centers: Vec<Uid> = match mode {
                Mode::SelfOnly => vec![u],
                Mode::Ally => battle.field(side).to_vec(),
                _ => foes.clone(),
            };
            for c in centers {
                let targets = battle.targets_for(u, &prof, mode, Some(c), level);
                let score = score_move(battle, u, &mv, &prof, mode, &targets, level);
                if score > 0.0 {
                    options.push((score, Action::Move { slot: i, mode: Some(mode), target: Some(c), cast: false }));
                }
            }
        }
    }

    for o in options.iter_mut() {
        o.0 *= 0.8 + battle.rng.next() * 0.4;
    }
    options.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let mut best = options.swap_remove(0).1;

    // big moves are telegraphed: locks the player can break
    if let Action::Move { slot, ref mut cast, .. } = best {
        if side == Side::Foe {
            let id = battle.mon(u).moves[slot].id;
            let prof = battle.dex.profile(id).unwrap();
            let big = prof.kind == Kind::Damage && (battle.dex.mv(id).power >= cfg.lock_min_power || prof.charge_turn);
            let charge_turn = prof.charge_turn;
            if big && (charge_turn || battle.rng.chance(cfg.lock_foe_use)) {
                *cast = true;
            }
        }
    }
    best
}

fn value(battle: &Battle, dmg: f64, t: Uid) -> f64 {
    let m = battle.mon(t);
    let frac = dmg.min(m.hp as f64) / m.stats.hp as f64;
    frac + if dmg >= m.hp as f64 { 0.4 } else { 0.0 }
}

fn estimate_basic(battle: &Battle, u: Uid, t: Uid, ty_: u8, cat: Category, power: u16) -> f64 {
    let eff = battle.dex.effectiveness(ty_, battle.types(t)) as f64;
    let (mu, mt) = (battle.mon(u), battle.mon(t));
    let physical = cat == Category::Physical;
    let a = if physical { mu.stats.atk } else { mu.stats.spa } as f64;
    let d = if physical { mt.stats.def } else { mt.stats.spd } as f64;
    ((((2.0 * mu.level as f64) / 5.0 + 2.0) * power as f64 * a) / d / 50.0 + 2.0) * 1.5 * eff * 0.925
}

/// Would this status move do anything to t? (already statused, type or ability immunity)
fn can_afflict(battle: &Battle, mv: &MoveData, prof: &Profile, t: Uid) -> bool {
    let b = battle.get(t);
    let m = battle.mon(t);
    let types = battle.types(t);
    match prof.status {
        Some(Ailment::Conf) => b.confusion == 0 && battle.dex.ability_effect(&m.ability) != "no_confusion",
        Some(Ailment::Drowsy) => m.status.is_none() && b.drowsy == 0,
        Some(Ailment::Status(s)) => {
            if m.status.is_some() {
                return false;
            }
            let key = match s {
                Status::Brn => "burn",
                Status::Psn | Status::Tox => "poison",
                Status::Frz => "freeze",
                Status::Par => "paralysis",
                Status::Slp => "sleep",
            };
            if types.iter().any(|&x| battle.dex.type_immune(x, key)) {
                return false;
            }
            let roles = battle.dex.ty;
            if s == Status::Par && mv.ty == roles.electric && types.contains(&roles.ground) {
                return false;
            }
            battle.dex.effectiveness(mv.ty, types) > 0.0 || mv.ty == roles.normal
        }
        None => false,
    }
}

fn score_move(battle: &Battle, u: Uid, mv: &MoveData, prof: &Profile, mode: Mode, targets: &[Uid], level: u8) -> f64 {
    let side = battle.get(u).side;
    let enemies: Vec<Uid> = targets.iter().copied().filter(|&t| battle.get(t).side != side).collect();
    let allies: Vec<Uid> = targets.iter().copied().filter(|&t| battle.get(t).side == side && t != u).collect();
    match prof.kind {
        Kind::Damage => {
            let mut s = 0.0;
            for &t in &enemies {
                s += value(battle, battle.estimate(u, t, mv, mode) * (1.0 + 0.1 * (level as f64 - 1.0)), t);
            }
            for &t in &allies {
                s -= value(battle, battle.estimate(u, t, mv, mode), t) * 1.5;
            }
            if prof.recoil > 0.0 {
                s *= 0.85;
            }
            if prof.self_faint {
                let m = battle.mon(u);
                s *= if (m.hp as f64 / m.stats.hp as f64) < 0.3 { 1.0 } else { 0.2 };
            }
            if prof.needs_sleep && !enemies.iter().any(|&t| battle.mon(t).status == Some(Status::Slp)) {
                s = 0.0;
            }
            s
        }
        Kind::Boost => {
            let stat = prof.stats[0].0;
            let m = battle.mon(u);
            if battle.get(u).stages[stat.idx()] >= 2 || (m.hp as f64 / m.stats.hp as f64) < 0.5 { 0.05 } else { 0.3 }
        }
        Kind::Hex => {
            let stat = prof.stats[0].0;
            enemies.iter().filter(|&&t| battle.get(t).stages[stat.idx()] > -2).count() as f64 * 0.12
        }
        Kind::Ailment => enemies.iter().filter(|&&t| can_afflict(battle, mv, prof, t)).count() as f64 * 0.3,
        Kind::Heal => {
            let t = targets.first().copied().unwrap_or(u);
            let m = battle.mon(t);
            let missing = 1.0 - m.hp as f64 / m.stats.hp as f64;
            if missing > 0.45 { missing * 0.9 } else { 0.02 }
        }
        Kind::Field => {
            let s = battle.side(side);
            match prof.field {
                Some(FieldKind::PhysicalScreen) => if s.physical_screen > 0 { 0.0 } else { 0.25 },
                Some(FieldKind::SpecialScreen) => if s.special_screen > 0 { 0.0 } else { 0.25 },
                Some(FieldKind::Rain) => if battle.weather.map(|w| w.kind == crate::battle::WeatherKind::Rain).unwrap_or(false) { 0.0 } else { 0.2 },
                Some(FieldKind::Sun) => if battle.weather.map(|w| w.kind == crate::battle::WeatherKind::Sun).unwrap_or(false) { 0.0 } else { 0.2 },
                Some(FieldKind::Seed) => {
                    enemies.iter().filter(|&&t| battle.get(t).seeded_by.is_none() && !battle.types(t).contains(&battle.dex.ty.grass)).count() as f64 * 0.25
                }
                Some(FieldKind::Guard) => {
                    let m = battle.mon(u);
                    if (m.hp as f64 / m.stats.hp as f64) < 0.4 { 0.2 } else { 0.05 }
                }
                Some(FieldKind::ResetStats) => {
                    if battle.field(side.other()).iter().any(|&t| battle.get(t).stages.iter().any(|&v| v >= 2)) { 0.4 } else { 0.0 }
                }
                _ => 0.01,
            }
        }
    }
}

/// Timing results for a simulated player who hits the window with probability `skill`.
pub fn simulated_timing(plan: &Plan, rng: &mut Rng, skill: f64) -> Timing {
    match plan.input {
        InputKind::Rhythm => {
            let mut hits = 1u8;
            while hits < plan.beats && rng.chance(skill) {
                hits += 1;
            }
            Timing { success: false, hits: Some(hits.max(plan.guaranteed.max(1))) }
        }
        InputKind::None => Timing::default(),
        _ => Timing { success: rng.chance(skill), hits: None },
    }
}
