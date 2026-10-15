//! The rule tests' own content: a few original types, creatures and moves made for the
//! tests (a fast one, a slow heavy one, a caster weak to one type...), and a move for every
//! mechanic the engine knows, so the tests need nothing imported.

use content::{Content, Space};
use rules::moves::{OTHER_EFFECTS, PLAIN_HITS, STAT_NAMES};
use serde_json::{json, Value};

/// type -> what it hits hard (2x), softly (0.5x), not at all (0x)
const TYPES: &[(&str, &[&str], &[&str], &[&str])] = &[
    ("normal", &[], &["rock"], &[]),
    ("fire", &["grass", "ice", "bug"], &["water", "rock", "fire"], &[]),
    ("water", &["fire", "ground", "rock"], &["grass", "water"], &[]),
    ("grass", &["water", "ground", "rock"], &["fire", "grass", "bug", "flying"], &[]),
    ("electric", &["water", "flying"], &["grass", "electric"], &["ground"]),
    ("ice", &["grass", "ground", "flying"], &["fire", "water", "ice"], &[]),
    ("ground", &["fire", "electric", "rock"], &["grass", "bug"], &["flying"]),
    ("rock", &["fire", "ice", "flying", "bug"], &["ground", "fighting"], &[]),
    ("flying", &["grass", "bug", "fighting"], &["electric", "rock"], &[]),
    ("bug", &["grass"], &["fire", "flying", "fighting"], &[]),
    ("fighting", &["normal", "rock", "ice"], &["flying", "bug"], &[]),
];

/// id, types, hp atk def spe spa spd, ability, learnset
#[allow(clippy::type_complexity)]
const CREATURES: &[(&str, &[&str], [u16; 6], &str, &[(u8, &str)])] = &[
    ("voltkit", &["electric"], [35, 55, 40, 90, 50, 50], "contact_paralysis", &[(1, "jolt"), (1, "bump"), (5, "static_wave"), (20, "spark_bolt"), (25, "spark_punch")]),
    ("zapfox", &["electric"], [65, 65, 60, 130, 110, 95], "absorb_electric", &[(1, "jolt"), (1, "bump"), (30, "spark_bolt")]),
    ("drowsbear", &["normal"], [160, 110, 65, 30, 65, 110], "status_defense", &[(1, "bump"), (1, "battle_cry"), (20, "menace")]),
    ("nibbler", &["normal"], [30, 56, 35, 72, 25, 35], "sure_escape", &[(1, "bump"), (1, "dash"), (10, "flurry")]),
    ("brawler", &["fighting"], [80, 100, 70, 45, 50, 60], "status_power", &[(1, "chop"), (1, "bump"), (15, "battle_cry")]),
    ("tidewyrm", &["water", "flying"], [95, 125, 79, 81, 60, 100], "lower_attack_on_entry", &[(1, "torrent_blast"), (1, "tidal_wave"), (1, "bump")]),
    ("sproutling", &["grass"], [40, 49, 49, 45, 65, 65], "pinch_grass", &[(1, "vine_lash"), (1, "bump"), (20, "sun_lance")]),
    ("cinderpup", &["fire"], [39, 52, 43, 65, 60, 50], "pinch_fire", &[(1, "ember_bite"), (1, "bump"), (25, "flame_burst")]),
    ("puddlefin", &["water"], [44, 48, 65, 43, 50, 64], "pinch_water", &[(1, "bubble_jet"), (1, "bump"), (25, "tidal_wave")]),
    ("pebblet", &["rock", "ground"], [40, 80, 100, 20, 30, 30], "no_recoil", &[(1, "rock_toss"), (1, "bump"), (30, "quake")]),
    ("flitwing", &["bug", "flying"], [40, 45, 35, 60, 40, 40], "float", &[(1, "flurry"), (1, "bump"), (1, "mimic_form")]),
    ("blazemane", &["fire", "fighting"], [80, 120, 70, 80, 110, 70], "pinch_fire", &[(1, "ember_bite"), (1, "chop"), (1, "flame_burst"), (1, "battle_cry")]),
];

/// id, type, category, power, accuracy, pp, target, effect, chance, contact, priority
#[allow(clippy::type_complexity)]
const MOVES: &[(&str, &str, &str, u16, u16, u8, &str, &str, u16, bool, i8)] = &[
    ("bump", "normal", "physical", 40, 100, 35, "selected", "hit", 0, true, 0),
    ("dash", "normal", "physical", 40, 100, 30, "selected", "priority_hit", 0, true, 1),
    ("flurry", "normal", "physical", 15, 85, 20, "selected", "multi_hit", 0, true, 0),
    ("battle_cry", "normal", "status", 0, 0, 30, "user", "attack_up_2", 0, false, 0),
    ("menace", "normal", "status", 0, 100, 40, "both", "attack_down", 0, false, 0),
    ("mimic_form", "normal", "status", 0, 0, 10, "selected", "transform", 0, false, 0),
    ("jolt", "electric", "special", 40, 100, 30, "selected", "paralyze_hit", 10, false, 0),
    ("spark_bolt", "electric", "special", 95, 100, 15, "selected", "paralyze_hit", 10, false, 0),
    ("spark_punch", "electric", "physical", 75, 100, 15, "selected", "paralyze_hit", 10, true, 0),
    ("static_wave", "electric", "status", 0, 100, 20, "selected", "paralyze", 0, false, 0),
    ("torrent_blast", "water", "special", 120, 80, 5, "selected", "hit", 0, false, 0),
    ("tidal_wave", "water", "special", 95, 100, 15, "both", "hit", 0, false, 0),
    ("bubble_jet", "water", "special", 40, 100, 30, "selected", "speed_down_hit", 10, false, 0),
    ("quake", "ground", "physical", 100, 100, 10, "foes_and_ally", "hits_underground", 0, false, 0),
    ("sun_lance", "grass", "special", 120, 100, 10, "selected", "charge_unless_sun", 0, false, 0),
    ("vine_lash", "grass", "physical", 45, 100, 25, "selected", "hit", 0, true, 0),
    ("chop", "fighting", "physical", 50, 100, 25, "selected", "high_critical", 0, true, 0),
    ("ember_bite", "fire", "physical", 60, 100, 25, "selected", "burn_hit", 10, true, 0),
    ("flame_burst", "fire", "special", 90, 100, 15, "selected", "burn_hit", 10, false, 0),
    ("rock_toss", "rock", "physical", 50, 90, 15, "selected", "hit", 0, false, 0),
];

/// One move per mechanic: (id, effect, power, target).
pub fn mechanic_moves() -> Vec<(String, String, u16, &'static str)> {
    let mut out = vec![];
    for e in PLAIN_HITS {
        out.push((format!("fx_{}", e.to_lowercase()), e.to_lowercase(), 60, "selected"));
    }
    for e in OTHER_EFFECTS {
        let target = if e.ends_with("_UP") || e.contains("HEAL") || matches!(*e, "RESTORE_HP" | "SLEEP_FULL_HEAL" | "SHRINK" | "PHYSICAL_SCREEN" | "SPECIAL_SCREEN" | "RESET_STATS" | "GUARD" | "RAIN" | "SUN" | "CURE_TEAM" | "NOTHING" | "MAX_ATTACK_HALF_HP" | "POWER_UP_NEXT" | "CRIT_UP") { "user" } else { "selected" };
        out.push((format!("fx_{}", e.to_lowercase()), e.to_lowercase(), 0, target));
    }
    for stat in STAT_NAMES {
        for (way, target) in [("UP", "user"), ("UP_2", "user"), ("DOWN", "selected"), ("DOWN_2", "selected")] {
            let e = format!("{stat}_{way}");
            out.push((format!("fx_{}", e.to_lowercase()), e.to_lowercase(), 0, target));
        }
    }
    out
}

fn add(c: &mut Content, collection: &str, value: Value) {
    let path = format!("{collection}/{}.json", value["id"].as_str().unwrap());
    c.add(Space::Content, &path, value).unwrap();
}

pub fn content() -> Content {
    let mut c = Content::default();
    for (i, (id, strong, weak, none)) in TYPES.iter().enumerate() {
        let mut against = serde_json::Map::new();
        for t in *strong {
            against.insert(t.to_string(), json!(2.0));
        }
        for t in *weak {
            against.insert(t.to_string(), json!(0.5));
        }
        for t in *none {
            against.insert(t.to_string(), json!(0.0));
        }
        let immune: Vec<&str> = match *id {
            "fire" => vec!["burn"],
            "ice" => vec!["freeze"],
            _ => vec![],
        };
        add(&mut c, "types", json!({ "id": id, "name": id, "short": &id[..3], "color": "#888888", "order": i, "against": against, "immune_to": immune }));
    }
    let mut abilities: Vec<&str> = CREATURES.iter().map(|c| c.3).collect();
    abilities.sort();
    abilities.dedup();
    for (i, a) in abilities.iter().enumerate() {
        add(&mut c, "abilities", json!({ "id": a, "name": a, "number": i + 1, "effect": a }));
    }
    for (i, (id, ty, cat, power, acc, pp, target, effect, chance, contact, priority)) in MOVES.iter().enumerate() {
        add(&mut c, "moves", json!({ "id": id, "name": id, "number": i + 1, "type": ty, "category": cat, "power": power, "accuracy": acc, "pp": pp,
            "target": target, "effect": effect, "effect_chance": chance, "contact": contact, "priority": priority }));
    }
    for (i, (id, effect, power, target)) in mechanic_moves().into_iter().enumerate() {
        let category = if power > 0 { "physical" } else { "status" };
        add(&mut c, "moves", json!({ "id": id, "name": id, "number": 1000 + i, "type": "normal", "category": category, "power": power,
            "accuracy": 100, "pp": 15, "target": target, "effect": effect, "contact": power > 0 }));
    }
    let exp: Vec<u32> = (0..=100u32).map(|l| l * l * l).collect();
    add(&mut c, "growth_rates", json!({ "id": "steady", "name": "Steady", "exp": exp }));
    for (i, (id, types, s, ability, learnset)) in CREATURES.iter().enumerate() {
        add(&mut c, "creatures", json!({
            "id": id, "name": id, "number": i + 1, "types": types,
            "base_stats": { "hp": s[0], "attack": s[1], "defense": s[2], "speed": s[3], "sp_attack": s[4], "sp_defense": s[5] },
            "abilities": [ability], "catch_rate": 120, "exp_yield": 60, "growth_rate": "steady",
            "learnset": { "level": learnset.iter().map(|(l, m)| json!([l, m])).collect::<Vec<_>>() },
        }));
    }
    c.sort();
    c
}
