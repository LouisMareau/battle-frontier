//! Battles from the content: its encounters (wild, trainer, gym), regions and the player's
//! party. What's fixed in code here is only the battle bag, until items come from content.

use content::{Encounter, Member};
use rules::battle::{BattleKind, Item};
use rules::config::Period;
use rules::mon::MonSpec;

pub const DEFAULT_BAG: [(Item, u32); 9] = [
    (Item::Potion, 5), (Item::SuperPotion, 3), (Item::HyperPotion, 1), (Item::Ether, 3), (Item::FullHeal, 2),
    (Item::Revive, 2), (Item::Idp, 10), (Item::ReinforcedIdp, 5), (Item::PrimeIdp, 3),
];

pub fn kind_of(e: &Encounter) -> BattleKind {
    match e.kind.as_str() {
        "wild" => BattleKind::Wild,
        "trainer" => BattleKind::Trainer,
        _ => BattleKind::Gym,
    }
}

pub fn kind_label(e: &Encounter) -> String {
    match kind_of(e) {
        BattleKind::Wild => "WILD".into(),
        BattleKind::Gym => "GYM RULES".into(),
        BattleKind::Trainer => format!("TRAINER {}V{}", e.size, e.size),
    }
}

pub fn period_key(p: Period) -> &'static str {
    match p {
        Period::Morning => "morning",
        Period::Day => "day",
        Period::Night => "night",
    }
}

/// Who's around a wild encounter at this time of day (the first list if none for it).
pub fn pool(e: &Encounter, p: Period) -> Vec<(String, u8)> {
    e.pools.get(period_key(p)).or_else(|| e.pools.values().next()).cloned().unwrap_or_default()
}

/// A team member as the rules make creatures; `move_level` when it names no move levels.
pub fn spec(m: &Member, move_level: u8) -> MonSpec {
    MonSpec {
        species: m.creature.clone(),
        name: Some(m.name.trim().to_string()).filter(|n| !n.is_empty()),
        level: m.level,
        moves: m.moves.clone(),
        move_levels: if m.move_levels.is_empty() { vec![move_level; 4] } else { m.move_levels.clone() },
        ..Default::default()
    }
}
