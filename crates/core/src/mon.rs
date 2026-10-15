//! A creature outside of battle: its stats, MP, and its moves with their XP
//! (a move's level comes from its XP).

use crate::config::Config;
use crate::dex::{Dex, Rng};
use crate::moves::{level_from_xp, Status};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub hp: i32,
    pub atk: i32,
    pub def: i32,
    pub spe: i32,
    pub spa: i32,
    pub spd: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveSlot {
    pub id: u16,
    pub xp: u32,
}

#[derive(Clone, Debug)]
pub struct Mon {
    /// index into `Dex::species`
    pub species: usize,
    pub name: String,
    pub level: u8,
    pub ivs: [u8; 6],
    pub stats: Stats,
    pub hp: i32,
    pub max_mp: i32,
    pub mp: i32,
    pub status: Option<Status>,
    pub sleep: i32,
    pub toxic: i32,
    pub ability: String,
    pub moves: Vec<MoveSlot>,
}

/// How to make a creature: what encounters and parties say.
#[derive(Clone, Debug, Default)]
pub struct MonSpec {
    pub species: String,
    pub level: u8,
    /// move ids; empty = the learnset's last four
    pub moves: Vec<String>,
    pub move_xp: Vec<u32>,
    /// 1-5 per move (the XP for that level at least)
    pub move_levels: Vec<u8>,
    pub ivs: Option<[u8; 6]>,
    pub ability: Option<String>,
    pub mp: Option<i32>,
}

impl MonSpec {
    pub fn new(species: &str, level: u8) -> MonSpec {
        MonSpec { species: species.into(), level, ..Default::default() }
    }

    pub fn moves(mut self, moves: &[&str]) -> MonSpec {
        self.moves = moves.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn levels(mut self, levels: &[u8]) -> MonSpec {
        self.move_levels = levels.to_vec();
        self
    }
}

/// `one_hp`: the creature always has 1 HP (a base HP of 1 says so).
pub fn calc_stats(base: [u16; 6], level: u8, ivs: [u8; 6], one_hp: bool) -> Stats {
    let l = level as i32;
    let other = |b: u16, iv: u8| ((2 * b as i32 + iv as i32) * l) / 100 + 5;
    Stats {
        hp: if one_hp { 1 } else { ((2 * base[0] as i32 + ivs[0] as i32) * l) / 100 + l + 10 },
        atk: other(base[1], ivs[1]),
        def: other(base[2], ivs[2]),
        spe: other(base[3], ivs[3]),
        spa: other(base[4], ivs[4]),
        spd: other(base[5], ivs[5]),
    }
}

pub fn max_mp(level: u8, cfg: &Config) -> i32 {
    (cfg.mp_base + cfg.mp_per_level * level as f32).round() as i32
}

/// The last four distinct moves the learnset gives at this level that the prototype supports.
pub fn default_moves(dex: &Dex, species: usize, level: u8) -> Vec<u16> {
    let mut out: Vec<u16> = vec![];
    for &(lv, id) in &dex.species[species].learnset {
        if lv > level || !dex.supported(id) {
            continue;
        }
        out.retain(|&m| m != id);
        out.push(id);
    }
    let n = out.len();
    out[n.saturating_sub(4)..].to_vec()
}

pub fn create_mon(dex: &Dex, spec: &MonSpec, rng: Option<&mut Rng>, cfg: &Config) -> Mon {
    let species = dex.sp(&spec.species);
    let sp = &dex.species[species];
    let mut rng = rng;
    let ivs = spec.ivs.unwrap_or_else(|| match rng.as_deref_mut() {
        Some(r) => std::array::from_fn(|_| r.int(0, 31) as u8),
        None => [20; 6],
    });
    let ids: Vec<u16> = if spec.moves.is_empty() {
        default_moves(dex, species, spec.level)
    } else {
        spec.moves.iter().map(|m| dex.move_id(m)).collect()
    };
    let moves = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let mut xp = spec.move_xp.get(i).copied().unwrap_or(0);
            if let Some(&lv) = spec.move_levels.get(i) {
                if lv >= 1 {
                    xp = xp.max(cfg.xp_levels[(lv.min(5) - 1) as usize]);
                }
            }
            MoveSlot { id, xp }
        })
        .collect();
    let stats = calc_stats(sp.base, spec.level, ivs, sp.base[0] == 1);
    let ability = spec.ability.clone().unwrap_or_else(|| {
        let second = sp.abilities.len() > 1 && rng.map(|r| r.chance(0.5)).unwrap_or(false);
        sp.abilities.get(if second { 1 } else { 0 }).cloned().unwrap_or_else(|| "None".into())
    });
    let mp = max_mp(spec.level, cfg);
    Mon {
        species,
        name: sp.name.clone(),
        level: spec.level,
        ivs,
        stats,
        hp: stats.hp,
        max_mp: mp,
        mp: spec.mp.unwrap_or(mp),
        status: None,
        sleep: 0,
        toxic: 0,
        ability,
        moves,
    }
}

impl Mon {
    pub fn move_level(&self, slot: usize, cfg: &Config) -> u8 {
        level_from_xp(self.moves[slot].xp, cfg)
    }

    pub fn heal_full(&mut self) {
        self.hp = self.stats.hp;
        self.mp = self.max_mp;
        self.status = None;
        self.sleep = 0;
        self.toxic = 0;
    }
}
