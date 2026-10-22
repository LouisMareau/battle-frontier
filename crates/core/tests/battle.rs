//! The prototype's tests/battle.test.js, ported: same scenarios, same assertions.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rules::ai::{choose_action, simulated_timing};
use rules::battle::{Action, Battle, BattleKind, BattleSetup, Event, Item, Side, Timing};
use rules::config::{Config, Period};
use rules::dex::{Dex, Rng};
use rules::mon::{create_mon, Mon, MonSpec};
use rules::moves::{describe_levels, level_from_xp, max_hits, modes, mp_cost, InputKind, Kind, Mode, Track};

mod fixture;

/// The tests' own content (tests/fixture): nothing imported needed.
fn dex() -> Arc<Dex> {
    static DEX: OnceLock<Arc<Dex>> = OnceLock::new();
    DEX.get_or_init(|| Arc::new(Dex::from_content(&fixture::content()).unwrap())).clone()
}

fn mon(species: &str, level: u8) -> Mon {
    mon_with(MonSpec::new(species, level))
}

fn mon_with(mut spec: MonSpec) -> Mon {
    spec.ivs.get_or_insert([20; 6]);
    create_mon(&dex(), &spec, None, &Config::default())
}

fn battle(kind: BattleKind, player: Vec<Mon>, foe: Vec<Mon>, size: usize, seed: u32, config: Option<Config>, pool: Vec<(String, u8)>) -> Battle {
    Battle::new(
        dex(),
        BattleSetup {
            kind, size, seed, config: config.unwrap_or_default(), period: Period::Day,
            player_name: "You".into(), player_party: player,
            bag: BTreeMap::from([(Item::Potion, 3), (Item::Idp, 5)]),
            foe_name: "Leader".into(), foe_party: foe, pool,
        },
    )
}

/// Plays a battle with the AI on both sides. Returns the number of actions taken.
fn autoplay(b: &mut Battle, skill: f64, send_chance: f64) -> usize {
    let mut rng = Rng::new(99);
    b.start();
    let mut actions = 0;
    while b.outcome.is_none() && actions < 3000 {
        let n = b.next();
        if n.replace {
            let m = b.bench(Side::Player)[0];
            b.replace(m);
            continue;
        }
        let Some(actor) = n.actor else { continue };
        let turn = b.begin_turn(actor);
        actions += 1;
        if !turn.can_act {
            b.skip_turn(actor);
            continue;
        }
        let mut action = turn.auto;
        if action.is_none() && b.get(actor).side == Side::Player && b.kind != BattleKind::Trainer && !b.bench(Side::Player).is_empty() && rng.chance(send_chance) {
            action = Some(Action::Send { mon: b.bench(Side::Player)[0] });
        }
        let action = action.unwrap_or_else(|| choose_action(b, actor));
        let plan = b.plan(actor, action);
        let timing = simulated_timing(&plan, &mut rng, skill);
        b.resolve(&plan, timing);
    }
    actions
}

fn mv(c: &str) -> u16 {
    dex().move_id(c)
}

fn damage_total(ev: &[Event]) -> i32 {
    ev.iter().map(|e| if let Event::Damage { amount, .. } = e { *amount } else { 0 }).sum()
}

#[test]
fn the_fixture_loads_with_physical_special_and_status_moves() {
    let d = dex();
    assert_eq!(d.species.len(), 12);
    assert_eq!(d.move_count(), 20 + fixture::mechanic_moves().len());
    use rules::dex::Category::*;
    assert_eq!(d.mv(mv("spark_bolt")).category, Special);
    assert_eq!(d.mv(mv("spark_punch")).category, Physical);
    assert_eq!(d.mv(mv("menace")).category, Status);
}

#[test]
fn tracks_inputs_and_modes_per_level() {
    let d = dex();
    let bolt = d.profile(mv("spark_bolt")).unwrap();
    assert_eq!(bolt.track, Track::Strike);
    assert_eq!(bolt.input, InputKind::Charge);
    assert_eq!(modes(bolt, 1), vec![Mode::Focus]);
    assert_eq!(modes(bolt, 3), vec![Mode::Focus, Mode::Chain]);
    assert_eq!(modes(bolt, 5), vec![Mode::Focus, Mode::Chain, Mode::Storm]);
    let surf = d.profile(mv("tidal_wave")).unwrap();
    assert_eq!(surf.track, Track::Wave);
    assert_eq!(modes(surf, 5), vec![Mode::Storm, Mode::Chain, Mode::Focus]);
    assert!(d.profile(mv("quake")).unwrap().hits_allies);
    let fury = d.profile(mv("flurry")).unwrap();
    assert_eq!((fury.track, fury.input), (Track::Multi, InputKind::Rhythm));
    assert_eq!(max_hits(fury, 5), 5);
    assert!(d.profile(mv("sun_lance")).unwrap().charge_turn);
    assert_eq!(d.profile(mv("bump")).unwrap().input, InputKind::Tap);
    assert_eq!(d.profile(mv("battle_cry")).unwrap().kind, Kind::Boost);
    assert!(d.profile(mv("mimic_form")).is_none());
    assert_eq!(describe_levels(Some(bolt), &Config::default()).len(), 5);
}

#[test]
fn mp_cost_from_pp_status_moves_cheaper_with_levels() {
    let d = dex();
    let c = Config::default();
    let cost = |m: &str, lv| mp_cost(d.mv(mv(m)), d.profile(mv(m)), lv, &c);
    assert_eq!(cost("torrent_blast", 1), 8);
    assert_eq!(cost("spark_bolt", 1), 4);
    assert_eq!(cost("static_wave", 1), 3);
    assert_eq!(cost("static_wave", 4), 1);
    assert_eq!(level_from_xp(0, &c), 1);
    assert_eq!(level_from_xp(70, &c), 5);
}

#[test]
fn every_supported_move_runs_in_real_battles() {
    let d = dex();
    for (id, effect, _, _) in fixture::mechanic_moves() {
        assert!(d.supported(mv(&id)), "the engine doesn't run {effect}");
    }
    let mut ids: Vec<u16> = d.moves().filter(|m| d.supported(m.id)).map(|m| m.id).collect();
    ids.sort();
    let mut rng = Rng::new(5);
    let species: Vec<String> = d.species.iter().filter(|s| !s.learnset.is_empty()).map(|s| s.key.clone()).collect();
    for (k, chunk) in ids.chunks(4).enumerate() {
        let lv = 1 + (k * 4 % 5) as u8;
        let moves: Vec<String> = chunk.iter().map(|&i| d.mv(i).key.clone()).collect();
        let make = |rng: &mut Rng| {
            let sp = rng.pick(&species).clone();
            let spec = MonSpec { species: sp, level: 40, moves: moves.clone(), move_levels: vec![lv; moves.len()], ..Default::default() };
            create_mon(&d, &spec, Some(rng), &Config::default())
        };
        let p = vec![make(&mut rng), make(&mut rng)];
        let f = vec![make(&mut rng), make(&mut rng), make(&mut rng)];
        let mut b = battle(BattleKind::Trainer, p, f, 2, (k * 4) as u32, None, vec![]);
        autoplay(&mut b, 0.7, 0.05);
        assert!(b.outcome.is_some(), "moves {moves:?} never finished");
    }
}

#[test]
fn random_battles_of_every_kind_always_finish() {
    let d = dex();
    let mut rng = Rng::new(11);
    let species: Vec<String> = d.species.iter().filter(|s| !s.learnset.is_empty()).map(|s| s.key.clone()).collect();
    let team = |n: usize, lv: u8, rng: &mut Rng| -> Vec<Mon> {
        (0..n)
            .map(|_| {
                let sp = rng.pick(&species).clone();
                create_mon(&d, &MonSpec::new(&sp, lv), Some(rng), &Config::default())
            })
            .collect()
    };
    for i in 0..60usize {
        let kind = [BattleKind::Wild, BattleKind::Trainer, BattleKind::Gym][i % 3];
        let lv = 5 + ((i * 7) % 90) as u8;
        let pool = vec![(rng.pick(&species).clone(), lv)];
        let p = team(6, lv, &mut rng);
        let f = team(if kind == BattleKind::Wild { 1 } else { 6 }, lv, &mut rng);
        let mut b = battle(kind, p, f, 1 + i % 3, i as u32, None, pool);
        let actions = autoplay(&mut b, 0.7, 0.15);
        assert!(b.outcome.is_some(), "battle {i} ({kind:?}) unfinished after {actions} actions");
        assert!(b.field(Side::Player).len() <= 6 && b.field(Side::Foe).len() <= 6);
    }
}

#[test]
fn the_round_queue_goes_by_speed() {
    let bump = |s, l| mon_with(MonSpec::new(s, l).moves(&["bump"]));
    let mut b = battle(BattleKind::Trainer, vec![bump("drowsbear", 50), bump("zapfox", 50)], vec![bump("voltkit", 50)], 2, 1, None, vec![]);
    b.start();
    let first = b.next();
    assert!(first.events.iter().any(|e| matches!(e, Event::Round { n: 1, .. })));
    let order: Vec<String> = b.queue_preview().now.iter().map(|&u| dex().species[b.mon(u).species].key.clone()).collect();
    assert_eq!(order, vec!["zapfox", "voltkit", "drowsbear"]);
}

#[test]
fn priority_moves_put_the_user_first_in_the_next_round() {
    let quick = mon_with(MonSpec::new("nibbler", 60).moves(&["dash"]));
    let foe = mon_with(MonSpec::new("zapfox", 60).moves(&["bump"]));
    let mut b = battle(BattleKind::Trainer, vec![quick], vec![foe], 1, 1, None, vec![]);
    b.start();
    let mut n = b.next();
    if b.get(n.actor.unwrap()).side == Side::Foe {
        let p = b.plan(n.actor.unwrap(), Action::Attack { target: None });
        b.resolve(&p, Timing::default());
        n = b.next();
    }
    let p = b.plan(n.actor.unwrap(), Action::Move { slot: 0, target: None, mode: None, cast: false });
    b.resolve(&p, Timing::default());
    let first = b.queue_preview().next[0];
    assert_eq!(dex().species[b.mon(first).species].key, "nibbler");
}

#[test]
fn chain_hits_the_target_and_its_neighbours_storm_hits_everyone() {
    let user = mon_with(MonSpec::new("voltkit", 50).moves(&["spark_bolt"]).levels(&[5]));
    let foes: Vec<Mon> = ["nibbler", "sproutling", "cinderpup", "puddlefin"].iter().map(|s| mon(s, 30)).collect();
    let mut b = battle(BattleKind::Trainer, vec![user], foes, 4, 1, None, vec![]);
    b.start();
    let p = b.field(Side::Foe).to_vec();
    assert_eq!(p.len(), 4);
    let me = b.field(Side::Player)[0];
    let d = dex();
    let bolt = d.profile(mv("spark_bolt")).unwrap();
    assert_eq!(b.targets_for(me, bolt, Mode::Chain, Some(p[1]), 5).len(), 3);
    assert_eq!(b.targets_for(me, bolt, Mode::Chain, Some(p[0]), 5).len(), 2);
    assert_eq!(b.targets_for(me, bolt, Mode::Storm, Some(p[0]), 5).len(), 4);
}

#[test]
fn a_timed_hit_deals_more_damage_than_an_untimed_one() {
    let run = |success| {
        let mut b = battle(BattleKind::Trainer, vec![mon_with(MonSpec::new("brawler", 40).moves(&["chop"]))], vec![mon("drowsbear", 60)], 1, 7, None, vec![]);
        b.start();
        let me = b.field(Side::Player)[0];
        let p = b.plan(me, Action::Move { slot: 0, target: None, mode: None, cast: false });
        damage_total(&b.resolve(&p, Timing { success, hits: None }))
    };
    assert!(run(true) > run(false));
}

#[test]
fn a_block_halves_the_damage_taken() {
    let run = |success| {
        let mut b = battle(BattleKind::Trainer, vec![mon("drowsbear", 60)], vec![mon_with(MonSpec::new("brawler", 40).moves(&["chop"]))], 1, 3, None, vec![]);
        b.start();
        let foe = b.field(Side::Foe)[0];
        let p = b.plan(foe, Action::Move { slot: 0, target: None, mode: None, cast: false });
        damage_total(&b.resolve(&p, Timing { success, hits: None }))
    };
    let (open, blocked) = (run(false), run(true));
    assert!((blocked as f64) < open as f64 * 0.7, "{blocked} vs {open}");
}

#[test]
fn locks_break_them_all_and_the_cast_is_cancelled() {
    let caster = mon_with(MonSpec::new("tidewyrm", 40).moves(&["torrent_blast"]));
    let me = mon_with(MonSpec::new("voltkit", 40).moves(&["jolt"]));
    let mut b = battle(BattleKind::Trainer, vec![me], vec![caster], 1, 2, None, vec![]);
    b.start();
    let foe = b.field(Side::Foe)[0];
    let mine = b.field(Side::Player)[0];
    let p = b.plan(foe, Action::Move { slot: 0, target: None, mode: None, cast: true });
    let ev = b.resolve(&p, Timing::default());
    let locks = ev.iter().find_map(|e| if let Event::Cast { locks, .. } = e { Some(locks.clone()) } else { None }).unwrap();
    assert!(locks.len() >= 2);
    assert!(locks.iter().all(|&t| t == dex().ty.electric), "the voltkit can only hit electric locks here");
    for _ in 0..locks.len() {
        let p = b.plan(mine, Action::Move { slot: 0, target: None, mode: None, cast: false });
        b.resolve(&p, Timing { success: true, hits: None });
    }
    if b.mon(foe).hp > 0 {
        let turn = b.begin_turn(foe);
        assert_eq!(turn.auto, Some(Action::Fire));
        let p = b.plan(foe, Action::Fire);
        let fire = b.resolve(&p, Timing::default());
        assert!(fire.iter().any(|e| matches!(e, Event::CastEnd { cancelled: true, .. })));
        assert_eq!(b.get(foe).stunned, 1);
    }
}

#[test]
fn gym_leaders_match_the_number_of_creatures_the_player_sends() {
    let mut b = battle(
        BattleKind::Gym,
        vec![mon("sproutling", 15), mon("cinderpup", 15), mon("puddlefin", 15)],
        vec![mon("pebblet", 14), mon("brawler", 15), mon("flitwing", 14)],
        1, 1, None, vec![],
    );
    b.start();
    assert_eq!(b.field(Side::Foe).len(), 1);
    let me = b.field(Side::Player)[0];
    let m = b.bench(Side::Player)[0];
    let p = b.plan(me, Action::Send { mon: m });
    b.resolve(&p, Timing::default());
    assert_eq!(b.field(Side::Player).len(), 2);
    assert_eq!(b.field(Side::Foe).len(), 2);
}

#[test]
fn wild_creatures_call_for_help_up_to_six() {
    let cfg = Config { wild_call_chance: 1.0, wild_call_below_hp: 2.0, ..Config::default() };
    let mut b = battle(BattleKind::Wild, vec![mon("blazemane", 60)], vec![mon("nibbler", 5)], 1, 1, Some(cfg), vec![("flitwing".into(), 5)]);
    b.start();
    for _ in 0..8 {
        let foe = b.field(Side::Foe)[0];
        let p = b.plan(foe, Action::Call);
        b.resolve(&p, Timing::default());
    }
    assert_eq!(b.field(Side::Foe).len(), 6);
}

#[test]
fn using_a_move_earns_xp_and_levels_it_up() {
    let p = mon_with(MonSpec::new("voltkit", 30).moves(&["jolt"]));
    let mut b = battle(BattleKind::Trainer, vec![p], vec![mon("drowsbear", 80)], 1, 1, None, vec![]);
    b.start();
    let me = b.field(Side::Player)[0];
    let mut leveled = false;
    for _ in 0..3 {
        let p = b.plan(me, Action::Move { slot: 0, target: None, mode: None, cast: false });
        let ev = b.resolve(&p, Timing { success: true, hits: None });
        leveled |= ev.iter().any(|e| matches!(e, Event::MoveLevel { level: 2, .. }));
    }
    assert!(leveled);
}

#[test]
fn basic_attacks_restore_mp_moves_spend_it() {
    let mut spec = MonSpec::new("voltkit", 30).moves(&["spark_bolt"]);
    spec.mp = Some(5);
    let mut b = battle(BattleKind::Trainer, vec![mon_with(spec)], vec![mon("drowsbear", 80)], 1, 1, None, vec![]);
    b.start();
    let me = b.field(Side::Player)[0];
    let p = b.plan(me, Action::Move { slot: 0, target: None, mode: None, cast: false });
    b.resolve(&p, Timing::default());
    assert_eq!(b.mon(me).mp, 1);
    let p = b.plan(me, Action::Attack { target: None });
    b.resolve(&p, Timing { success: true, hits: None });
    assert_eq!(b.mon(me).mp, 5);
}
