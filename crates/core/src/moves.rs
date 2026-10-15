//! What each Gen 3 move does in this battle system: its upgrade track, the shapes
//! (modes) its level unlocks, its timing input and its MP cost.

use crate::config::Config;
use crate::dex::MoveData;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stat {
    Atk,
    Def,
    Spe,
    Spa,
    Spd,
    Acc,
    Eva,
    Crit,
}

impl Stat {
    pub const ALL: [Stat; 8] = [Stat::Atk, Stat::Def, Stat::Spe, Stat::Spa, Stat::Spd, Stat::Acc, Stat::Eva, Stat::Crit];

    pub fn idx(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Stat::Atk => "Attack",
            Stat::Def => "Defense",
            Stat::Spe => "Speed",
            Stat::Spa => "Sp. Atk",
            Stat::Spd => "Sp. Def",
            Stat::Acc => "Accuracy",
            Stat::Eva => "Evasion",
            Stat::Crit => "Crit rate",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Stat::Atk => "ATK",
            Stat::Def => "DEF",
            Stat::Spe => "SPE",
            Stat::Spa => "SPA",
            Stat::Spd => "SPD",
            Stat::Acc => "ACC",
            Stat::Eva => "EVA",
            Stat::Crit => "CRIT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Brn,
    Par,
    Psn,
    Tox,
    Slp,
    Frz,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Brn => "BRN",
            Status::Par => "PAR",
            Status::Psn => "PSN",
            Status::Tox => "TOX",
            Status::Slp => "SLP",
            Status::Frz => "FRZ",
        }
    }
}

/// What a status move or secondary effect inflicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ailment {
    Status(Status),
    Conf,
    Drowsy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Damage,
    Boost,
    Hex,
    Ailment,
    Heal,
    Field,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    Strike,
    Wave,
    Multi,
    Charge,
    Boost,
    Hex,
    Ailment,
    Heal,
    Field,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Focus,
    Chain,
    Storm,
    SelfOnly,
    Ally,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Focus => "Focus",
            Mode::Chain => "Chain",
            Mode::Storm => "Storm",
            Mode::SelfOnly => "Self",
            Mode::Ally => "Ally",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    None,
    Tap,
    Charge,
    Rhythm,
    Cast,
    Block,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Secondary {
    Ailment(Ailment),
    Tri,
    Flinch,
    Stats(Vec<(Stat, i8)>),
    SelfStats(Vec<(Stat, i8)>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fixed {
    Level,
    Amount(i32),
    Half,
    RandomLevel,
    MatchHp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Heal {
    Frac(f32),
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    PhysicalScreen,
    SpecialScreen,
    ResetStats,
    Guard,
    Rain,
    Sun,
    Seed,
    CureTeam,
    Nothing,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: u16,
    pub kind: Kind,
    pub track: Track,
    pub native: Mode,
    pub input: InputKind,
    pub hits_allies: bool,
    pub min_hits: u8,
    pub max_hits: u8,
    pub ramp: bool,
    pub charge_turn: bool,
    pub recharge: bool,
    pub secondary: Option<Secondary>,
    pub charge_stats: Vec<(Stat, i8)>,
    pub high_crit: bool,
    pub always_hit: bool,
    pub recoil: f32,
    pub crash: bool,
    pub drain: f32,
    pub needs_sleep: bool,
    pub self_faint: bool,
    pub self_drop: Vec<(Stat, i8)>,
    pub leave_one_hp: bool,
    pub breaks_screens: bool,
    pub fixed: Option<Fixed>,
    pub stats: Vec<(Stat, i8)>,
    pub max_attack: bool,
    pub charge: bool,
    pub status: Option<Ailment>,
    pub target_stats: Vec<(Stat, i8)>,
    pub heal: Option<Heal>,
    pub field: Option<FieldKind>,
}

impl Profile {
    fn new(id: u16, kind: Kind, track: Track, native: Mode, input: InputKind) -> Profile {
        Profile {
            id, kind, track, native, input,
            hits_allies: false, min_hits: 1, max_hits: 1, ramp: false, charge_turn: false, recharge: false,
            secondary: None, charge_stats: vec![], high_crit: false, always_hit: false, recoil: 0.0, crash: false,
            drain: 0.0, needs_sleep: false, self_faint: false, self_drop: vec![], leave_one_hp: false,
            breaks_screens: false, fixed: None, stats: vec![], max_attack: false, charge: false, status: None,
            target_stats: vec![], heal: None, field: None,
        }
    }
}

/// The effects of moves that deal damage (`power` > 0) the engine knows.
pub const PLAIN_HITS: &[&str] = &[
    "HIT", "HIGH_CRITICAL", "ALWAYS_HIT", "PRIORITY_HIT", "LAST_SURE_HIT", "FIRST_TURN_FLINCH", "LEAVE_ONE_HP",
    "CLEAR_TRAPS_HIT", "HIT_SWITCHING", "BREAK_SCREENS", "KNOCK_ITEM", "STEAL_ITEM", "SCATTER_COINS", "TERRAIN_HIT", "ANGER_HIT",
    "DOUBLING_STREAK", "ROLLING_STREAK", "NOISY_RAMPAGE", "RAMPAGE", "TRAP", "POWER_BY_FRIENDSHIP", "POWER_BY_UNFRIENDLINESS", "PERSONAL_TYPE",
    "DOUBLE_IF_STATUSED", "DOUBLE_ON_PARALYZED", "DOUBLE_IF_HURT", "POWER_BY_HP", "POWER_BY_LOW_HP", "POWER_BY_WEIGHT", "RANDOM_POWER", "RANDOM_HIT_OR_HEAL",
    "WEATHER_POWER", "HITS_UNDERGROUND", "HITS_AIRBORNE", "FLINCH_HITS_AIRBORNE", "REACHES_AIRBORNE", "RAIN_SURE_PARALYZE_HIT", "SELF_FAINT_HIT",
    "RECOIL", "HEAVY_RECOIL", "RECOIL_IF_MISS", "DRAIN_HIT", "DRAIN_SLEEPING", "HIT_LOWER_OWN_ATTACK_DEFENSE", "HIT_LOWER_OWN_SP_ATTACK",
    "RECHARGE", "FLINCH_HIT", "FLINCH_HIT_SHRUNK", "CONFUSE_HIT", "POISON_HIT", "BURN_HIT",
    "FREEZE_HIT", "PARALYZE_HIT", "THAW_BURN_HIT", "HIGH_CRIT_BURN_HIT", "BAD_POISON_HIT", "HIGH_CRIT_POISON_HIT", "RANDOM_STATUS_HIT",
    "ATTACK_DOWN_HIT", "DEFENSE_DOWN_HIT", "SPEED_DOWN_HIT", "SPECIAL_ATTACK_DOWN_HIT",
    "SPECIAL_DEFENSE_DOWN_HIT", "ACCURACY_DOWN_HIT", "EVASION_DOWN_HIT", "ATTACK_UP_HIT",
    "DEFENSE_UP_HIT", "ALL_STATS_UP_HIT", "LEVEL_DAMAGE", "FIXED_40", "FIXED_20", "HALVE_HP",
    "RANDOM_LEVEL_DAMAGE", "MATCH_TARGET_HP", "CHARGE_UNLESS_SUN", "CHARGE_HIGH_CRIT", "CHARGE_FLINCH_CRIT", "CHARGE_DEFENSE_UP", "CHARGE_HIDDEN",
    "CHARGE_FAILS_IF_HIT", "MULTI_HIT", "DOUBLE_HIT", "DOUBLE_HIT_POISON", "RISING_TRIPLE_HIT",
];

/// The effects of moves that don't deal damage the engine knows, besides stat changes
/// (`ATTACK_UP`, `SPEED_DOWN_2`... any stat of `stat_of`, up or down, by 1 or 2).
pub const OTHER_EFFECTS: &[&str] = &[
    "ATTACK_DEFENSE_UP", "SPECIALS_UP", "ATTACK_SPEED_UP", "DEFENSES_UP", "CURL_UP", "SHRINK", "CRIT_UP",
    "MAX_ATTACK_HALF_HP", "POWER_UP_NEXT", "ATTACK_DEFENSE_DOWN", "SLEEP", "POISON", "BAD_POISON", "PARALYZE",
    "BURN", "CONFUSE", "CONFUSE_ATTACK_UP", "CONFUSE_SP_ATTACK_UP", "CONFUSE_ALL", "DROWSY", "RESTORE_HP",
    "HEAL_HALF", "HEAL_MORE_IN_MORNING", "HEAL_MORE_BY_DAY", "HEAL_MORE_AT_NIGHT", "SLEEP_FULL_HEAL",
    "PHYSICAL_SCREEN", "SPECIAL_SCREEN", "RESET_STATS", "GUARD", "RAIN", "SUN", "SEED_DRAIN", "CURE_TEAM", "NOTHING",
];

/// The stats moves raise and lower, by name.
pub const STAT_NAMES: &[&str] = &["ATTACK", "DEFENSE", "SPEED", "SPECIAL_ATTACK", "SPECIAL_DEFENSE", "ACCURACY", "EVASION"];

fn secondary_status(e: &str) -> Option<Ailment> {
    use Status::*;
    Some(match e {
        "POISON_HIT" | "HIGH_CRIT_POISON_HIT" | "DOUBLE_HIT_POISON" => Ailment::Status(Psn),
        "BURN_HIT" | "THAW_BURN_HIT" | "HIGH_CRIT_BURN_HIT" => Ailment::Status(Brn),
        "FREEZE_HIT" => Ailment::Status(Frz),
        "PARALYZE_HIT" | "TERRAIN_HIT" | "RAIN_SURE_PARALYZE_HIT" => Ailment::Status(Par),
        "BAD_POISON_HIT" => Ailment::Status(Tox),
        "CONFUSE_HIT" => Ailment::Conf,
        _ => return None,
    })
}

fn stat_of(name: &str) -> Option<Stat> {
    Some(match name {
        "ATTACK" => Stat::Atk,
        "DEFENSE" => Stat::Def,
        "SPEED" => Stat::Spe,
        "SPECIAL_ATTACK" => Stat::Spa,
        "SPECIAL_DEFENSE" => Stat::Spd,
        "ACCURACY" => Stat::Acc,
        "EVASION" => Stat::Eva,
        _ => return None,
    })
}

/// ATTACK_UP, DEFENSE_DOWN_2, SPEED_DOWN_HIT...: (stat, stages, is a hit's side effect)
fn stat_change(e: &str) -> Option<(Stat, i8, bool)> {
    let (rest, hit) = match e.strip_suffix("_HIT") {
        Some(r) => (r, true),
        None => (e, false),
    };
    let (rest, two) = match rest.strip_suffix("_2") {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let (name, sign) = if let Some(n) = rest.strip_suffix("_UP") {
        (n, 1)
    } else {
        let n = rest.strip_suffix("_DOWN")?;
        (n, -1)
    };
    let stat = stat_of(name)?;
    Some((stat, sign * if two { 2 } else { 1 }, hit))
}

fn spread_target(t: &str) -> bool {
    matches!(t, "BOTH" | "FOES_AND_ALLY" | "OPPONENTS_FIELD")
}

/// Everything the battle needs to know about a move, or None when the prototype can't run it.
pub fn build_profile(m: &MoveData) -> Option<Profile> {
    use Stat::*;
    let e = m.effect.as_str();
    if m.power > 0 && PLAIN_HITS.contains(&e) {
        let mut p = Profile::new(m.id, Kind::Damage, Track::Strike, Mode::Focus, InputKind::Tap);
        p.hits_allies = m.target == "FOES_AND_ALLY";
        match e {
            "MULTI_HIT" => (p.track, p.min_hits, p.max_hits) = (Track::Multi, 2, 3),
            "DOUBLE_HIT" | "DOUBLE_HIT_POISON" => (p.track, p.min_hits, p.max_hits) = (Track::Multi, 2, 2),
            "RISING_TRIPLE_HIT" => (p.track, p.min_hits, p.max_hits, p.ramp) = (Track::Multi, 1, 3, true),
            "CHARGE_UNLESS_SUN" | "CHARGE_HIGH_CRIT" | "CHARGE_FLINCH_CRIT" | "CHARGE_DEFENSE_UP" | "CHARGE_HIDDEN" | "CHARGE_FAILS_IF_HIT" => {
                p.track = Track::Charge;
                p.charge_turn = true;
            }
            "RECHARGE" => {
                p.track = Track::Charge;
                p.recharge = true;
            }
            _ => p.track = if spread_target(&m.target) { Track::Wave } else { Track::Strike },
        }
        p.native = if p.track == Track::Wave { Mode::Storm } else { Mode::Focus };
        p.input = if p.track == Track::Multi { InputKind::Rhythm } else if m.contact { InputKind::Tap } else { InputKind::Charge };
        if let Some(a) = secondary_status(e) {
            p.secondary = Some(Secondary::Ailment(a));
        }
        if e == "RANDOM_STATUS_HIT" {
            p.secondary = Some(Secondary::Tri);
        }
        if matches!(e, "FLINCH_HIT" | "FLINCH_HIT_SHRUNK" | "FLINCH_HITS_AIRBORNE" | "FIRST_TURN_FLINCH" | "CHARGE_FLINCH_CRIT") {
            p.secondary = Some(Secondary::Flinch);
        }
        if let Some((stat, n, true)) = stat_change(e) {
            p.secondary = Some(if n > 0 { Secondary::SelfStats(vec![(stat, n)]) } else { Secondary::Stats(vec![(stat, n)]) });
        }
        if e == "ALL_STATS_UP_HIT" {
            p.secondary = Some(Secondary::SelfStats(vec![(Atk, 1), (Def, 1), (Spe, 1), (Spa, 1), (Spd, 1)]));
        }
        if e == "CHARGE_DEFENSE_UP" {
            p.charge_stats = vec![(Def, 1)];
        }
        p.high_crit = matches!(e, "HIGH_CRITICAL" | "HIGH_CRIT_BURN_HIT" | "HIGH_CRIT_POISON_HIT" | "CHARGE_FLINCH_CRIT");
        p.always_hit = matches!(e, "ALWAYS_HIT" | "LAST_SURE_HIT") || m.accuracy == 0;
        p.recoil = match e {
            "RECOIL" => 0.25,
            "HEAVY_RECOIL" => 1.0 / 3.0,
            _ => 0.0,
        };
        p.crash = e == "RECOIL_IF_MISS";
        if e == "DRAIN_HIT" || e == "DRAIN_SLEEPING" {
            p.drain = 0.5;
        }
        p.needs_sleep = e == "DRAIN_SLEEPING";
        p.self_faint = e == "SELF_FAINT_HIT";
        p.self_drop = match e {
            "HIT_LOWER_OWN_ATTACK_DEFENSE" => vec![(Atk, -1), (Def, -1)],
            "HIT_LOWER_OWN_SP_ATTACK" => vec![(Spa, -2)],
            _ => vec![],
        };
        p.leave_one_hp = e == "LEAVE_ONE_HP";
        p.breaks_screens = e == "BREAK_SCREENS";
        p.fixed = match e {
            "LEVEL_DAMAGE" => Some(Fixed::Level),
            "FIXED_40" => Some(Fixed::Amount(40)),
            "FIXED_20" => Some(Fixed::Amount(20)),
            "HALVE_HP" => Some(Fixed::Half),
            "RANDOM_LEVEL_DAMAGE" => Some(Fixed::RandomLevel),
            "MATCH_TARGET_HP" => Some(Fixed::MatchHp),
            _ => None,
        };
        return Some(p);
    }

    let sc = stat_change(e);
    let boosts: Option<Vec<(Stat, i8)>> = match e {
        "ATTACK_DEFENSE_UP" => Some(vec![(Atk, 1), (Def, 1)]),
        "SPECIALS_UP" => Some(vec![(Spa, 1), (Spd, 1)]),
        "ATTACK_SPEED_UP" => Some(vec![(Atk, 1), (Spe, 1)]),
        "DEFENSES_UP" => Some(vec![(Def, 1), (Spd, 1)]),
        "CURL_UP" => Some(vec![(Def, 1)]),
        "SHRINK" => Some(vec![(Eva, 1)]),
        "CRIT_UP" => Some(vec![(Crit, 2)]),
        "MAX_ATTACK_HALF_HP" => Some(vec![(Atk, 12)]),
        "POWER_UP_NEXT" => Some(vec![(Spd, 1)]),
        _ => None,
    };
    if matches!(sc, Some((_, n, false)) if n > 0) || boosts.is_some() {
        let mut p = Profile::new(m.id, Kind::Boost, Track::Boost, Mode::SelfOnly, InputKind::Cast);
        p.stats = boosts.unwrap_or_else(|| {
            let (s, n, _) = sc.unwrap();
            vec![(s, n)]
        });
        p.max_attack = e == "MAX_ATTACK_HALF_HP";
        p.charge = e == "POWER_UP_NEXT";
        return Some(p);
    }
    if matches!(sc, Some((_, n, false)) if n < 0) || e == "ATTACK_DEFENSE_DOWN" {
        let native = if spread_target(&m.target) { Mode::Storm } else { Mode::Focus };
        let mut p = Profile::new(m.id, Kind::Hex, Track::Hex, native, InputKind::Cast);
        p.stats = if e == "ATTACK_DEFENSE_DOWN" {
            vec![(Atk, -1), (Def, -1)]
        } else {
            let (s, n, _) = sc.unwrap();
            vec![(s, n)]
        };
        return Some(p);
    }
    let ailment = match e {
        "SLEEP" => Some(Ailment::Status(Status::Slp)),
        "POISON" => Some(Ailment::Status(Status::Psn)),
        "BAD_POISON" => Some(Ailment::Status(Status::Tox)),
        "PARALYZE" => Some(Ailment::Status(Status::Par)),
        "BURN" => Some(Ailment::Status(Status::Brn)),
        "CONFUSE" | "CONFUSE_ATTACK_UP" | "CONFUSE_SP_ATTACK_UP" | "CONFUSE_ALL" => Some(Ailment::Conf),
        "DROWSY" => Some(Ailment::Drowsy),
        _ => None,
    };
    if let Some(a) = ailment {
        let native = if spread_target(&m.target) { Mode::Storm } else { Mode::Focus };
        let mut p = Profile::new(m.id, Kind::Ailment, Track::Ailment, native, InputKind::Cast);
        p.status = Some(a);
        p.target_stats = match e {
            "CONFUSE_ATTACK_UP" => vec![(Atk, 2)],
            "CONFUSE_SP_ATTACK_UP" => vec![(Spa, 1)],
            _ => vec![],
        };
        return Some(p);
    }
    let heal = match e {
        "RESTORE_HP" | "HEAL_HALF" | "HEAL_MORE_IN_MORNING" | "HEAL_MORE_BY_DAY" | "HEAL_MORE_AT_NIGHT" => Some(Heal::Frac(0.5)),
        "SLEEP_FULL_HEAL" => Some(Heal::Rest),
        _ => None,
    };
    if let Some(h) = heal {
        let mut p = Profile::new(m.id, Kind::Heal, Track::Heal, Mode::SelfOnly, InputKind::Cast);
        p.heal = Some(h);
        return Some(p);
    }
    let field = match e {
        "PHYSICAL_SCREEN" => Some(FieldKind::PhysicalScreen),
        "SPECIAL_SCREEN" => Some(FieldKind::SpecialScreen),
        "RESET_STATS" => Some(FieldKind::ResetStats),
        "GUARD" => Some(FieldKind::Guard),
        "RAIN" => Some(FieldKind::Rain),
        "SUN" => Some(FieldKind::Sun),
        "SEED_DRAIN" => Some(FieldKind::Seed),
        "CURE_TEAM" => Some(FieldKind::CureTeam),
        "NOTHING" => Some(FieldKind::Nothing),
        _ => None,
    };
    if let Some(f) = field {
        let native = if f == FieldKind::Seed { Mode::Focus } else { Mode::SelfOnly };
        let mut p = Profile::new(m.id, Kind::Field, Track::Field, native, InputKind::Cast);
        p.field = Some(f);
        return Some(p);
    }
    None
}

/// Shapes the move can take at this level, native first.
pub fn modes(p: &Profile, level: u8) -> Vec<Mode> {
    let n = if level >= 5 { 3 } else if level >= 3 { 2 } else { 1 };
    if p.native == Mode::SelfOnly {
        if p.kind == Kind::Heal && p.heal != Some(Heal::Rest) && level >= 3 {
            return vec![Mode::SelfOnly, Mode::Ally];
        }
        return vec![Mode::SelfOnly];
    }
    if p.field == Some(FieldKind::Seed)
        || matches!(p.track, Track::Strike | Track::Multi | Track::Charge)
        || (p.native == Mode::Focus && matches!(p.kind, Kind::Hex | Kind::Ailment))
    {
        return [Mode::Focus, Mode::Chain, Mode::Storm][..n].to_vec();
    }
    if p.track == Track::Wave {
        return [Mode::Storm, Mode::Chain, Mode::Focus][..n].to_vec();
    }
    vec![Mode::Storm]
}

pub fn power_bonus(level: u8, cfg: &Config) -> f32 {
    let mut bonus = 0.0;
    for &(lv, b) in &cfg.power_per_level {
        if level >= lv {
            bonus = b;
        }
    }
    bonus
}

pub fn mp_cost(m: &MoveData, p: Option<&Profile>, level: u8, cfg: &Config) -> i32 {
    let mut cost = cfg.cost_by_pp.iter().find(|(pp, _)| m.pp <= *pp).map(|c| c.1).unwrap_or(1);
    if let Some(p) = p {
        if p.kind != Kind::Damage {
            cost -= (level >= 2) as i32 + (level >= 4) as i32;
        }
    }
    cost.max(1)
}

pub fn level_from_xp(xp: u32, cfg: &Config) -> u8 {
    let mut lv = 1;
    for (i, &need) in cfg.xp_levels.iter().enumerate() {
        if xp >= need {
            lv = i as u8 + 1;
        }
    }
    lv
}

pub fn xp_for_level(level: u8, cfg: &Config) -> u32 {
    cfg.xp_levels[(level.clamp(1, 5) - 1) as usize]
}

/// Multi-hit: how many hits this level allows at most.
pub fn max_hits(p: &Profile, level: u8) -> u8 {
    p.max_hits + (level >= 3) as u8 + (level >= 5) as u8
}

/// Charge-track moves: does this use spend its turn charging (and fire next round)?
pub fn needs_charge_turn(p: &Profile, level: u8) -> bool {
    p.charge_turn && level < 5
}

/// ...and does the user lose its next turn recharging?
pub fn needs_recharge(p: &Profile, level: u8, timed: bool) -> bool {
    if !p.recharge || level >= 5 {
        return false;
    }
    if level >= 3 {
        return !timed;
    }
    true
}

fn mode_text(m: Mode) -> &'static str {
    match m {
        Mode::Chain => "Chain mode (target + its neighbours)",
        Mode::Storm => "Storm mode (every foe)",
        Mode::Focus => "Focus mode (one target, full power)",
        _ => "Can target an ally",
    }
}

pub fn timed_text(p: &Profile) -> &'static str {
    match p.kind {
        Kind::Damage if p.track == Track::Multi => "Timed: each beat you hit adds a hit",
        Kind::Damage => "Timed: x1.25 damage",
        Kind::Boost => "Timed: +1 extra stage",
        Kind::Hex => "Timed: -1 extra stage",
        Kind::Ailment => "Timed: never misses",
        Kind::Heal => "Timed: heals 25% more",
        Kind::Field => match p.field {
            Some(FieldKind::Guard) => "Timed: always works",
            Some(FieldKind::Seed) => "Timed: never misses",
            _ => "Timed: lasts 2 rounds longer",
        },
    }
}

pub fn input_text(i: InputKind) -> &'static str {
    match i {
        InputKind::Tap => "Press A at impact",
        InputKind::Charge => "Hold A, release when the ring fills",
        InputKind::Rhythm => "Press A on every hit",
        InputKind::Cast => "Press A at the flash",
        InputKind::Block => "Press A at impact to block",
        InputKind::None => "",
    }
}

/// One line per level for the move info panel.
pub fn describe_levels(p: Option<&Profile>, cfg: &Config) -> Vec<String> {
    let Some(p) = p else { return vec!["Not in the prototype yet".into()] };
    let mut out = vec![];
    for lv in 1..=5u8 {
        let mut parts: Vec<String> = vec![];
        if lv == 1 {
            parts.push(timed_text(p).into());
        }
        let prev = modes(p, lv - 1);
        if lv > 1 {
            for m in modes(p, lv) {
                if !prev.contains(&m) {
                    parts.push(mode_text(m).into());
                }
            }
        }
        if p.kind == Kind::Damage {
            let (b, pb) = (power_bonus(lv, cfg), power_bonus(lv - 1, cfg));
            if lv > 1 && b > pb {
                parts.push(format!("+{}% power", (b * 100.0).round()));
            }
            if p.track == Track::Multi && lv > 1 && max_hits(p, lv) > max_hits(p, lv - 1) {
                parts.push(format!("up to {} hits", max_hits(p, lv)));
            }
            if p.charge_turn && lv == 3 {
                parts.push("goes first in the round it fires".into());
            }
            if p.charge_turn && lv == 5 {
                parts.push("no charge turn".into());
            }
            if p.recharge && lv == 3 {
                parts.push("timed: no recharge".into());
            }
            if p.recharge && lv == 5 {
                parts.push("never recharges".into());
            }
            if lv == 3 && p.secondary.is_some() {
                parts.push("timed: double effect chance".into());
            }
            if lv == 5 {
                parts.push("timed: echo hit (50%)".into());
            }
        } else {
            if lv == 2 || lv == 4 {
                parts.push("-1 MP".into());
            }
            if p.kind == Kind::Boost && lv == 3 {
                parts.push("also +1 to adjacent allies".into());
            }
            if p.kind == Kind::Boost && lv == 5 {
                parts.push("also +1 to every ally".into());
            }
            if p.kind == Kind::Hex && p.native == Mode::Storm && (lv == 3 || lv == 5) {
                parts.push("one more stage".into());
            }
            if p.kind == Kind::Ailment && lv > 1 {
                parts.push("+5 accuracy".into());
            }
            if p.kind == Kind::Heal && lv == 5 && p.heal != Some(Heal::Rest) {
                parts.push("also heals the target's neighbours 25%".into());
            }
            if matches!(p.field, Some(FieldKind::PhysicalScreen | FieldKind::SpecialScreen | FieldKind::Rain | FieldKind::Sun)) && lv > 1 {
                parts.push(format!("+{} round", cfg.field_per_level));
            }
            if p.field == Some(FieldKind::Guard) && lv == 3 {
                parts.push("covers adjacent allies".into());
            }
            if p.field == Some(FieldKind::Guard) && lv == 5 {
                parts.push("covers every ally".into());
            }
        }
        out.push(if parts.is_empty() { "—".into() } else { parts.join(" · ") });
    }
    out
}
