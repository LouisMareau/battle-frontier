//! The battle rules, ported from the prototype's battle.js. No rendering here: the game
//! asks for the next actor, gets an action from the player's menu (or ai.rs), asks for a
//! plan (what to animate and which timing input to read), plays it, then hands the timing
//! result back to `resolve`, which applies everything and returns events to show.
//!
//! Rounds: everyone on the field acts once per round, fastest first (the order is fixed
//! when the round starts). A move's priority moves its user up or down in the NEXT round.
//! Newcomers act from the next round on.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::config::{Config, Period};
use crate::dex::{Category, Dex, MoveData, Rng, NO_TYPE};
use crate::mon::{create_mon, Mon, MonSpec};
use crate::moves::{
    level_from_xp, max_hits, modes, mp_cost, needs_charge_turn, needs_recharge, power_bonus, Ailment, FieldKind, Fixed,
    Heal, InputKind, Kind, Mode, Profile, Secondary, Stat, Status, Track,
};

pub type Uid = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Player,
    Foe,
}

impl Side {
    pub fn i(self) -> usize {
        self as usize
    }
    pub fn other(self) -> Side {
        match self {
            Side::Player => Side::Foe,
            Side::Foe => Side::Player,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleKind {
    Wild,
    Trainer,
    Gym,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Lose,
    Ran,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Rain,
    Sun,
}

#[derive(Clone, Copy, Debug)]
pub struct Weather {
    pub kind: WeatherKind,
    pub rounds: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    Potion,
    SuperPotion,
    HyperPotion,
    Ether,
    FullHeal,
    Revive,
    CaptureBall,
    GreatBall,
    UltraBall,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItemInfo {
    pub name: &'static str,
    pub text: &'static str,
    pub heal: i32,
    pub mp: i32,
    pub cure: bool,
    pub revive: f32,
    pub ball: f32,
}

impl Item {
    pub const ALL: [Item; 9] = [
        Item::Potion, Item::SuperPotion, Item::HyperPotion, Item::Ether, Item::FullHeal, Item::Revive,
        Item::CaptureBall, Item::GreatBall, Item::UltraBall,
    ];

    pub fn info(self) -> ItemInfo {
        let i = ItemInfo::default();
        match self {
            Item::Potion => ItemInfo { name: "Potion", text: "Restores 20 HP", heal: 20, ..i },
            Item::SuperPotion => ItemInfo { name: "Super Potion", text: "Restores 50 HP", heal: 50, ..i },
            Item::HyperPotion => ItemInfo { name: "Hyper Potion", text: "Restores 200 HP", heal: 200, ..i },
            Item::Ether => ItemInfo { name: "Ether", text: "Restores 10 MP", mp: 10, ..i },
            Item::FullHeal => ItemInfo { name: "Full Heal", text: "Cures any status", cure: true, ..i },
            Item::Revive => ItemInfo { name: "Revive", text: "Revives a fainted creature to half HP", revive: 0.5, ..i },
            Item::CaptureBall => ItemInfo { name: "Capture Ball", text: "Catches a wild creature", ball: 1.0, ..i },
            Item::GreatBall => ItemInfo { name: "Great Ball", text: "Better catch rate", ball: 1.5, ..i },
            Item::UltraBall => ItemInfo { name: "Ultra Ball", text: "Even better catch rate", ball: 2.0, ..i },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lock {
    pub ty: u8,
    pub broken: bool,
}

#[derive(Clone, Debug)]
pub struct Cast {
    pub slot: usize,
    pub level: u8,
    pub mode: Mode,
    pub targets: Vec<Uid>,
    pub locks: Vec<Lock>,
    /// own turns to wait before it fires (1 for big moves)
    pub wait: i32,
}

#[derive(Clone, Debug)]
pub struct Charging {
    pub slot: usize,
    pub level: u8,
    pub mode: Mode,
    pub targets: Vec<Uid>,
}

#[derive(Clone, Debug)]
pub struct Battler {
    pub uid: Uid,
    pub side: Side,
    /// index into its side's party
    pub mon: usize,
    pub stages: [i8; 8],
    pub bracket: i8,
    pub charging: Option<Charging>,
    pub cast: Option<Cast>,
    pub recharge: bool,
    pub protect: bool,
    pub flinch: bool,
    pub confusion: i32,
    pub seeded_by: Option<Uid>,
    pub drowsy: i32,
    pub charged: bool,
    pub fire_boost: bool,
    pub stunned: i32,
    pub loafing: bool,
    pub hit_this_round: bool,
    pub fresh: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SideState {
    pub name: String,
    pub party: Vec<Mon>,
    pub field: Vec<Uid>,
    pub physical_screen: i32,
    pub special_screen: i32,
    pub bag: BTreeMap<Item, u32>,
    /// wild battles: who answers a call for help (creature id, level)
    pub pool: Vec<(String, u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Msg(String),
    Enter { uid: Uid, side: Side, index: usize },
    Leave { uid: Uid },
    Faint { uid: Uid },
    Damage { uid: Uid, amount: i32, hp: i32, eff: f32, crit: bool, blocked: bool, echo: bool, self_hit: bool, from: Option<Uid> },
    Heal { uid: Uid, amount: i32, hp: i32 },
    Mp { uid: Uid, amount: i32, mp: i32 },
    Miss { uid: Uid },
    Status { uid: Uid, status: Option<Status> },
    Stat { uid: Uid, stat: Stat, delta: i8, stage: i8 },
    Cast { uid: Uid, mv: u16, locks: Vec<u8>, targets: Vec<Uid>, rounds: u8 },
    LockBreak { uid: Uid, index: usize },
    CastEnd { uid: Uid, cancelled: bool },
    Charge { uid: Uid, mv: u16 },
    MoveLevel { uid: Uid, mv: u16, level: u8 },
    Round { n: u32, order: Vec<Uid> },
    Weather { kind: Option<WeatherKind> },
    Screen { side: Side, kind: FieldKind, on: bool },
    Protect { uid: Uid },
    Seed { uid: Uid },
    Confused { uid: Uid },
    StagesReset,
    Revive { side: Side, mon: usize },
    Catch { uid: Uid, shakes: u8, caught: bool },
    Outcome(Outcome),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Attack { target: Option<Uid> },
    Move { slot: usize, target: Option<Uid>, mode: Option<Mode>, cast: bool },
    Fire,
    Send { mon: usize },
    Swap { mon: usize },
    Item { item: Item, target: Option<Uid>, mon: Option<usize> },
    Run,
    Call,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anim {
    None,
    Lunge,
    Projectile,
    Wave,
    Hex,
    SelfAnim,
    Gather,
    Call,
    Ball,
    Item,
}

/// What an action will look like: targets, animation, the timing input the player gets.
#[derive(Clone, Debug)]
pub struct Plan {
    pub actor: Uid,
    pub side: Side,
    pub action: Action,
    pub targets: Vec<Uid>,
    pub input: InputKind,
    pub anim: Anim,
    pub ty: Option<u8>,
    pub mv: Option<u16>,
    pub slot: usize,
    pub level: u8,
    pub mode: Mode,
    pub track: Option<Track>,
    pub cast: bool,
    pub charge_start: bool,
    pub beats: u8,
    pub guaranteed: u8,
}

/// What the player did during the animation: `success` for tap/charge/cast/block,
/// `hits` for multi-hit rhythms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timing {
    pub success: bool,
    pub hits: Option<u8>,
}

pub struct Next {
    pub actor: Option<Uid>,
    pub events: Vec<Event>,
    pub replace: bool,
}

pub struct TurnStart {
    pub events: Vec<Event>,
    pub can_act: bool,
    pub auto: Option<Action>,
}

pub struct Preview {
    pub round: u32,
    pub now: Vec<Uid>,
    pub next: Vec<Uid>,
}

pub struct BattleSetup {
    pub kind: BattleKind,
    pub size: usize,
    pub seed: u32,
    pub config: Config,
    pub period: Period,
    pub player_name: String,
    pub player_party: Vec<Mon>,
    pub bag: BTreeMap<Item, u32>,
    pub foe_name: String,
    pub foe_party: Vec<Mon>,
    pub pool: Vec<(String, u8)>,
}

const CRIT_CHANCE: [f64; 5] = [1.0 / 16.0, 1.0 / 8.0, 1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0];

fn stage_mul(s: i8) -> f64 {
    if s >= 0 { (2.0 + s as f64) / 2.0 } else { 2.0 / (2.0 - s as f64) }
}

fn acc_mul(s: i8) -> f64 {
    if s >= 0 { (3.0 + s as f64) / 3.0 } else { 3.0 / (3.0 - s as f64) }
}

fn pinch(dex: &Dex, effect: &str) -> Option<u8> {
    match effect {
        "pinch_grass" => Some(dex.ty.grass),
        "pinch_fire" => Some(dex.ty.fire),
        "pinch_water" => Some(dex.ty.water),
        "pinch_bug" => Some(dex.ty.bug),
        _ => None,
    }
}

/// Abilities (by effect key) that keep a status away.
fn status_immune(effect: &str, a: Ailment) -> bool {
    use Status::*;
    match effect {
        "no_sleep" => matches!(a, Ailment::Status(Slp) | Ailment::Drowsy),
        "no_paralysis" => a == Ailment::Status(Par),
        "no_poison" => matches!(a, Ailment::Status(Psn) | Ailment::Status(Tox)),
        "no_burn" => a == Ailment::Status(Brn),
        "no_freeze" => a == Ailment::Status(Frz),
        "no_confusion" => a == Ailment::Conf,
        _ => false,
    }
}

/// The key types name a status by in their `immune_to` lists.
fn status_key(s: Status) -> &'static str {
    match s {
        Status::Brn => "burn",
        Status::Psn | Status::Tox => "poison",
        Status::Frz => "freeze",
        Status::Par => "paralysis",
        Status::Slp => "sleep",
    }
}

pub fn status_text(s: Status) -> &'static str {
    match s {
        Status::Brn => "was burned",
        Status::Par => "is paralyzed",
        Status::Psn => "was poisoned",
        Status::Tox => "was badly poisoned",
        Status::Slp => "fell asleep",
        Status::Frz => "was frozen solid",
    }
}

#[derive(Clone, Copy, Default)]
struct HitInfo {
    from: Option<Uid>,
    eff: f32,
    crit: bool,
    blocked: bool,
    echo: bool,
    self_hit: bool,
}

impl HitInfo {
    fn own() -> HitInfo {
        HitInfo { eff: 1.0, self_hit: true, ..Default::default() }
    }
}

struct DamageOpts {
    power: f64,
    ty: Option<u8>,
    category: Category,
    fixed: Option<Fixed>,
    high_crit: bool,
    extra_mul: f64,
    no_crit: bool,
}

impl DamageOpts {
    fn new(power: f64, ty: Option<u8>, category: Category) -> DamageOpts {
        DamageOpts { power, ty, category, fixed: None, high_crit: false, extra_mul: 1.0, no_crit: false }
    }
}

pub struct DamageRoll {
    pub damage: i32,
    pub eff: f32,
    pub crit: bool,
}

pub struct Battle {
    pub dex: Arc<Dex>,
    pub config: Config,
    pub rng: Rng,
    pub kind: BattleKind,
    pub size: usize,
    pub period: Period,
    pub round: u32,
    pub queue: VecDeque<Uid>,
    pub current: Option<Uid>,
    pub weather: Option<Weather>,
    pub outcome: Option<Outcome>,
    run_attempts: u32,
    pub caught: Vec<Mon>,
    pub battlers: Vec<Battler>,
    pub sides: [SideState; 2],
}

impl Battle {
    pub fn new(dex: Arc<Dex>, s: BattleSetup) -> Battle {
        Battle {
            dex,
            config: s.config,
            rng: Rng::new(s.seed),
            kind: s.kind,
            size: s.size.max(1),
            period: s.period,
            round: 0,
            queue: VecDeque::new(),
            current: None,
            weather: None,
            outcome: None,
            run_attempts: 0,
            caught: vec![],
            battlers: vec![],
            sides: [
                SideState { name: s.player_name, party: s.player_party, bag: s.bag, ..Default::default() },
                SideState { name: s.foe_name, party: s.foe_party, pool: s.pool, ..Default::default() },
            ],
        }
    }

    // ------------------------------------------------------------ queries

    pub fn get(&self, u: Uid) -> &Battler {
        &self.battlers[u]
    }
    fn bm(&mut self, u: Uid) -> &mut Battler {
        &mut self.battlers[u]
    }
    pub fn mon(&self, u: Uid) -> &Mon {
        let b = &self.battlers[u];
        &self.sides[b.side.i()].party[b.mon]
    }
    pub fn mon_mut(&mut self, u: Uid) -> &mut Mon {
        let (side, mon) = (self.battlers[u].side, self.battlers[u].mon);
        &mut self.sides[side.i()].party[mon]
    }
    pub fn types(&self, u: Uid) -> &[u8] {
        &self.dex.species[self.mon(u).species].types
    }
    pub fn name(&self, u: Uid) -> String {
        self.mon(u).name.clone()
    }
    pub fn side(&self, s: Side) -> &SideState {
        &self.sides[s.i()]
    }
    pub fn field(&self, s: Side) -> &[Uid] {
        &self.sides[s.i()].field
    }
    pub fn on_field(&self, u: Uid) -> bool {
        self.field(self.battlers[u].side).contains(&u)
    }
    fn alive_on_field(&self, u: Uid) -> bool {
        self.on_field(u) && self.mon(u).hp > 0
    }
    /// Party indices of the creatures that can still come out.
    pub fn bench(&self, s: Side) -> Vec<usize> {
        let side = &self.sides[s.i()];
        (0..side.party.len())
            .filter(|&i| side.party[i].hp > 0 && !side.field.iter().any(|&u| self.battlers[u].mon == i))
            .collect()
    }
    pub fn all_on_field(&self) -> Vec<Uid> {
        let mut v = self.sides[0].field.clone();
        v.extend(&self.sides[1].field);
        v
    }
    pub fn neighbours(&self, u: Uid) -> Vec<Uid> {
        let f = self.field(self.battlers[u].side);
        let Some(i) = f.iter().position(|&x| x == u) else { return vec![] };
        let mut out = vec![];
        if i > 0 {
            out.push(f[i - 1]);
        }
        if i + 1 < f.len() {
            out.push(f[i + 1]);
        }
        out
    }
    /// Does its ability have this mechanic (an effect key)?
    pub fn has(&self, u: Uid, effect: &str) -> bool {
        self.effect(u) == effect
    }

    /// Its ability's mechanic ("" for none).
    pub fn effect(&self, u: Uid) -> &str {
        self.dex.ability_effect(&self.mon(u).ability)
    }

    pub fn ability_name(&self, u: Uid) -> String {
        self.dex.ability_name(&self.mon(u).ability)
    }

    pub fn speed(&self, u: Uid) -> f64 {
        let m = self.mon(u);
        let mut s = m.stats.spe as f64 * stage_mul(self.battlers[u].stages[Stat::Spe.idx()]);
        if m.status == Some(Status::Par) {
            s *= 0.25;
        }
        if let Some(w) = self.weather {
            if (w.kind == WeatherKind::Rain && self.has(u, "rain_speed")) || (w.kind == WeatherKind::Sun && self.has(u, "sun_speed")) {
                s *= 2.0;
            }
        }
        s
    }

    /// The rest of this round (actor first) and the order the next one would have now.
    pub fn queue_preview(&self) -> Preview {
        let mut now: Vec<Uid> = self.current.into_iter().chain(self.queue.iter().copied()).filter(|&u| self.on_field(u)).collect();
        now.dedup();
        let mut next = self.all_on_field();
        next.sort_by(|&a, &b| {
            self.battlers[b].bracket.cmp(&self.battlers[a].bracket).then(self.speed(b).partial_cmp(&self.speed(a)).unwrap())
        });
        Preview { round: self.round, now, next }
    }

    pub fn move_level(&self, u: Uid, slot: usize) -> u8 {
        level_from_xp(self.mon(u).moves[slot].xp, &self.config)
    }

    pub fn move_data(&self, u: Uid, slot: usize) -> &MoveData {
        self.dex.mv(self.mon(u).moves[slot].id)
    }

    pub fn move_cost(&self, u: Uid, slot: usize, targets: &[Uid]) -> i32 {
        let id = self.mon(u).moves[slot].id;
        let pressure = targets.iter().any(|&t| self.battlers[t].side != self.battlers[u].side && self.has(t, "heavy_cost")) as i32;
        mp_cost(self.dex.mv(id), self.dex.profile(id), self.move_level(u, slot), &self.config) + pressure
    }

    pub fn basic_category(&self, u: Uid) -> Category {
        let m = self.mon(u);
        if m.stats.atk >= m.stats.spa { Category::Physical } else { Category::Special }
    }

    /// Who a move hits for a mode and a chosen target.
    pub fn targets_for(&self, u: Uid, prof: &Profile, mode: Mode, target: Option<Uid>, level: u8) -> Vec<Uid> {
        let side = self.battlers[u].side;
        let own = self.field(side);
        let opp = self.field(side.other());
        match mode {
            Mode::SelfOnly => return vec![u],
            Mode::Ally => return vec![target.filter(|t| own.contains(t)).unwrap_or(u)],
            _ => {}
        }
        if opp.is_empty() {
            return vec![];
        }
        let t = target.filter(|t| opp.contains(t)).unwrap_or(opp[0]);
        let ti = opp.iter().position(|&x| x == t).unwrap() as i64;
        let mut out: Vec<Uid> = match mode {
            Mode::Focus => vec![t],
            Mode::Chain => opp.iter().enumerate().filter(|(i, _)| (*i as i64 - ti).abs() <= 1).map(|(_, &x)| x).collect(),
            _ => opp.to_vec(),
        };
        if prof.hits_allies && level < 3 && mode != Mode::Focus {
            out.extend(self.neighbours(u));
        }
        out
    }

    // ------------------------------------------------------------ flow

    pub fn start(&mut self) -> Vec<Event> {
        let mut ev = vec![];
        let n = if self.kind == BattleKind::Trainer { self.size } else { 1 };
        let foe_name = self.sides[1].name.clone();
        if self.kind != BattleKind::Wild {
            ev.push(Event::Msg(format!("{foe_name} wants to battle!")));
        }
        for m in self.bench(Side::Foe).into_iter().take(n) {
            let u = self.enter(Side::Foe, m, &mut ev, None);
            let name = self.name(u);
            ev.push(Event::Msg(if self.kind == BattleKind::Wild { format!("A wild {name} appeared!") } else { format!("{foe_name} sent out {name}!") }));
        }
        for m in self.bench(Side::Player).into_iter().take(n) {
            let u = self.enter(Side::Player, m, &mut ev, None);
            ev.push(Event::Msg(format!("Go! {}!", self.name(u))));
        }
        self.entry_abilities(&mut ev);
        ev
    }

    fn enter(&mut self, side: Side, mon: usize, ev: &mut Vec<Event>, index: Option<usize>) -> Uid {
        let uid = self.battlers.len();
        self.battlers.push(Battler {
            uid, side, mon, stages: [0; 8], bracket: 0, charging: None, cast: None, recharge: false, protect: false,
            flinch: false, confusion: 0, seeded_by: None, drowsy: 0, charged: false, fire_boost: false, stunned: 0,
            loafing: false, hit_this_round: false, fresh: true,
        });
        let f = &mut self.sides[side.i()].field;
        match index {
            Some(i) if i <= f.len() => f.insert(i, uid),
            _ => f.push(uid),
        }
        let index = f.iter().position(|&x| x == uid).unwrap();
        ev.push(Event::Enter { uid, side, index });
        uid
    }

    fn entry_abilities(&mut self, ev: &mut Vec<Event>) {
        for u in self.all_on_field() {
            if !self.battlers[u].fresh {
                continue;
            }
            self.bm(u).fresh = false;
            let name = self.name(u);
            if self.has(u, "lower_attack_on_entry") {
                ev.push(Event::Msg(format!("{name}'s {} cuts the foes' Attack!", self.ability_name(u))));
                for t in self.field(self.battlers[u].side.other()).to_vec() {
                    self.change_stat(t, Stat::Atk, -1, ev, Some(u));
                }
            }
            if self.has(u, "summon_rain") {
                self.weather = Some(Weather { kind: WeatherKind::Rain, rounds: 8 });
                ev.push(Event::Weather { kind: Some(WeatherKind::Rain) });
                ev.push(Event::Msg(format!("{name}'s {} made it rain!", self.ability_name(u))));
            }
            if self.has(u, "summon_sun") {
                self.weather = Some(Weather { kind: WeatherKind::Sun, rounds: 8 });
                ev.push(Event::Weather { kind: Some(WeatherKind::Sun) });
                ev.push(Event::Msg(format!("{name}'s {} intensified the sun!", self.ability_name(u))));
            }
        }
    }

    /// The next battler to act, with whatever happened in between (end of round, new round).
    #[allow(clippy::should_implement_trait)] // the battle's next actor, not an iterator
    pub fn next(&mut self) -> Next {
        let mut ev = vec![];
        for _ in 0..4 {
            if self.outcome.is_some() {
                return Next { actor: None, events: ev, replace: false };
            }
            if self.field(Side::Player).is_empty() && self.needs_replacement() {
                return Next { actor: None, events: ev, replace: true };
            }
            while let Some(u) = self.queue.pop_front() {
                if self.on_field(u) {
                    self.current = Some(u);
                    return Next { actor: Some(u), events: ev, replace: false };
                }
            }
            self.current = None;
            if self.round > 0 {
                self.end_round(&mut ev);
            }
            if self.outcome.is_some() {
                return Next { actor: None, events: ev, replace: false };
            }
            if self.needs_replacement() {
                return Next { actor: None, events: ev, replace: true };
            }
            self.begin_round(&mut ev);
        }
        panic!("no one can act");
    }

    fn begin_round(&mut self, ev: &mut Vec<Event>) {
        self.round += 1;
        let mut keyed: Vec<(Uid, i8, f64, f64)> = self
            .all_on_field()
            .into_iter()
            .map(|u| (u, self.battlers[u].bracket, self.speed(u), 0.0))
            .collect();
        for k in keyed.iter_mut() {
            k.3 = self.rng.next();
        }
        keyed.sort_by(|x, y| y.1.cmp(&x.1).then(y.2.partial_cmp(&x.2).unwrap()).then(x.3.partial_cmp(&y.3).unwrap()));
        self.queue = keyed.iter().map(|k| k.0).collect();
        for u in self.all_on_field() {
            let b = self.bm(u);
            b.bracket = 0;
            b.hit_this_round = false;
        }
        ev.push(Event::Round { n: self.round, order: self.queue.iter().copied().collect() });
    }

    /// Start of a battler's turn: what stops it from acting, or what it must do.
    pub fn begin_turn(&mut self, u: Uid) -> TurnStart {
        let mut ev = vec![];
        let name = self.name(u);
        self.bm(u).protect = false;
        let mut can_act = true;
        let status = self.mon(u).status;
        if self.battlers[u].flinch {
            self.bm(u).flinch = false;
            ev.push(Event::Msg(format!("{name} flinched!")));
            can_act = false;
        } else if self.battlers[u].recharge {
            self.bm(u).recharge = false;
            ev.push(Event::Msg(format!("{name} must recharge!")));
            can_act = false;
        } else if self.battlers[u].stunned > 0 {
            self.bm(u).stunned -= 1;
            ev.push(Event::Msg(format!("{name} is staggered!")));
            can_act = false;
        } else if self.battlers[u].loafing {
            self.bm(u).loafing = false;
            ev.push(Event::Msg(format!("{name} is loafing around!")));
            can_act = false;
        } else if status == Some(Status::Slp) {
            self.mon_mut(u).sleep -= 1;
            if self.mon(u).sleep > 0 {
                ev.push(Event::Msg(format!("{name} is fast asleep.")));
                can_act = false;
            } else {
                self.mon_mut(u).status = None;
                ev.push(Event::Status { uid: u, status: None });
                ev.push(Event::Msg(format!("{name} woke up!")));
            }
        } else if status == Some(Status::Frz) {
            if self.rng.chance(0.2) {
                self.mon_mut(u).status = None;
                ev.push(Event::Status { uid: u, status: None });
                ev.push(Event::Msg(format!("{name} thawed out!")));
            } else {
                ev.push(Event::Msg(format!("{name} is frozen solid!")));
                can_act = false;
            }
        } else if status == Some(Status::Par) && self.rng.chance(0.25) {
            ev.push(Event::Msg(format!("{name} is fully paralyzed!")));
            can_act = false;
        }
        if can_act && self.battlers[u].confusion > 0 {
            self.bm(u).confusion -= 1;
            if self.battlers[u].confusion == 0 {
                ev.push(Event::Msg(format!("{name} snapped out of confusion!")));
            } else {
                ev.push(Event::Msg(format!("{name} is confused!")));
                if self.rng.chance(0.5) {
                    let mut o = DamageOpts::new(40.0, None, Category::Physical);
                    o.no_crit = true;
                    let r = self.calc_damage(u, u, &o);
                    self.hurt(u, r.damage, &mut ev, HitInfo::own());
                    ev.push(Event::Msg("It hurt itself in its confusion!".into()));
                    can_act = false;
                }
            }
        }
        if can_act {
            if let Some(c) = self.battlers[u].cast.as_mut() {
                if c.wait > 0 {
                    c.wait -= 1;
                    ev.push(Event::Msg(format!("{name} is still gathering power...")));
                    return TurnStart { events: ev, can_act: false, auto: None };
                }
            }
        }
        let busy = self.battlers[u].cast.is_some() || self.battlers[u].charging.is_some();
        if !can_act && busy {
            ev.push(Event::CastEnd { uid: u, cancelled: true });
            ev.push(Event::Msg(format!("{name} lost its focus!")));
            let b = self.bm(u);
            b.cast = None;
            b.charging = None;
        }
        let auto = if can_act && busy { Some(Action::Fire) } else { None };
        TurnStart { events: ev, can_act, auto }
    }

    /// A turn that did nothing (asleep, flinched...): just clean up.
    pub fn skip_turn(&mut self, u: Uid) -> Vec<Event> {
        let mut ev = vec![];
        self.after_action(u, &mut ev);
        ev
    }

    fn end_round(&mut self, ev: &mut Vec<Event>) {
        let mut order = self.all_on_field();
        order.sort_by(|&a, &b| self.speed(b).partial_cmp(&self.speed(a)).unwrap());
        for u in order {
            if self.mon(u).hp <= 0 {
                continue;
            }
            let max = self.mon(u).stats.hp;
            let name = self.name(u);
            match self.mon(u).status {
                Some(Status::Brn) => {
                    self.hurt(u, (max / 8).max(1), ev, HitInfo::own());
                    ev.push(Event::Msg(format!("{name} is hurt by its burn!")));
                }
                Some(Status::Psn) => {
                    self.hurt(u, (max / 8).max(1), ev, HitInfo::own());
                    ev.push(Event::Msg(format!("{name} is hurt by poison!")));
                }
                Some(Status::Tox) => {
                    let n = (self.mon(u).toxic + 1).min(15);
                    self.mon_mut(u).toxic = n;
                    self.hurt(u, ((max * n) / 16).max(1), ev, HitInfo::own());
                    ev.push(Event::Msg(format!("{name} is hurt by poison!")));
                }
                _ => {}
            }
            if let Some(seeder) = self.battlers[u].seeded_by {
                if self.mon(u).hp > 0 {
                    let amount = self.mon(u).hp.min((max / 8).max(1));
                    self.hurt(u, amount, ev, HitInfo::own());
                    ev.push(Event::Msg(format!("{name}'s health is sapped by the seeds!")));
                    if self.alive_on_field(seeder) {
                        self.heal(seeder, amount, ev);
                    }
                }
            }
            if self.battlers[u].drowsy > 0 {
                self.bm(u).drowsy -= 1;
                if self.battlers[u].drowsy == 0 && self.mon(u).status.is_none() && self.mon(u).hp > 0 {
                    self.inflict(u, Status::Slp, ev, None, true);
                }
            }
            if self.has(u, "speed_up_each_round") && self.mon(u).hp > 0 {
                self.change_stat(u, Stat::Spe, 1, ev, Some(u));
            }
            if self.has(u, "shed_status") && self.mon(u).status.is_some() && self.rng.chance(1.0 / 3.0) {
                self.mon_mut(u).status = None;
                ev.push(Event::Status { uid: u, status: None });
                ev.push(Event::Msg(format!("{name} shed its skin!")));
            }
            self.bm(u).flinch = false;
        }
        for s in [Side::Player, Side::Foe] {
            let who = if s == Side::Player { "Your" } else { "The foes'" };
            for kind in [FieldKind::PhysicalScreen, FieldKind::SpecialScreen] {
                let side = &mut self.sides[s.i()];
                let v = if kind == FieldKind::PhysicalScreen { &mut side.physical_screen } else { &mut side.special_screen };
                if *v <= 0 {
                    continue;
                }
                *v -= 1;
                if *v == 0 {
                    ev.push(Event::Screen { side: s, kind, on: false });
                    ev.push(Event::Msg(format!("{who} {} wore off!", if kind == FieldKind::PhysicalScreen { "physical screen" } else { "special screen" })));
                }
            }
        }
        if let Some(w) = self.weather.as_mut() {
            w.rounds -= 1;
            if w.rounds <= 0 {
                let kind = w.kind;
                self.weather = None;
                ev.push(Event::Weather { kind: None });
                ev.push(Event::Msg(if kind == WeatherKind::Rain { "The rain stopped.".into() } else { "The sunlight faded.".into() }));
            }
        }
        self.cleanup(ev);
    }

    // ------------------------------------------------------------ planning

    pub fn plan(&self, u: Uid, action: Action) -> Plan {
        let side = self.battlers[u].side;
        let mut p = Plan {
            actor: u, side, action: action.clone(), targets: vec![], input: InputKind::None, anim: Anim::None, ty: None,
            mv: None, slot: 0, level: 1, mode: Mode::Focus, track: None, cast: false, charge_start: false, beats: 0, guaranteed: 0,
        };
        let opp = self.field(side.other());
        match action {
            Action::Attack { target } => {
                let t = target.filter(|t| opp.contains(t)).or(opp.first().copied());
                p.targets = t.into_iter().collect();
                p.ty = Some(self.types(u)[0]);
                p.anim = Anim::Lunge;
                p.input = if side == Side::Player { InputKind::Tap } else { InputKind::Block };
            }
            Action::Move { .. } | Action::Fire => {
                let (slot, level, mode, targets) = if let Action::Move { slot, target, mode, .. } = action {
                    let level = self.move_level(u, slot);
                    let prof = self.dex.profile(self.mon(u).moves[slot].id).expect("unsupported move");
                    let allowed = modes(prof, level);
                    let mode = mode.filter(|m| allowed.contains(m)).unwrap_or(allowed[0]);
                    (slot, level, mode, self.targets_for(u, prof, mode, target, level))
                } else {
                    let b = &self.battlers[u];
                    let (slot, level, mode, tg) = match (&b.cast, &b.charging) {
                        (Some(c), _) => (c.slot, c.level, c.mode, c.targets.clone()),
                        (None, Some(c)) => (c.slot, c.level, c.mode, c.targets.clone()),
                        _ => panic!("fire without a cast"),
                    };
                    let prof = self.dex.profile(self.mon(u).moves[slot].id).expect("unsupported move");
                    // targets that left the field are replaced: same shape around a new center
                    let alive: Vec<Uid> = tg.into_iter().filter(|&t| self.alive_on_field(t)).collect();
                    let targets = if alive.is_empty() { self.targets_for(u, prof, mode, None, level) } else { alive };
                    (slot, level, mode, targets)
                };
                let id = self.mon(u).moves[slot].id;
                let mv = self.dex.mv(id);
                let prof = self.dex.profile(id).expect("unsupported move");
                p.mv = Some(id);
                p.slot = slot;
                p.level = level;
                p.mode = mode;
                p.track = Some(prof.track);
                p.ty = Some(self.move_type(u, mv));
                p.cast = matches!(action, Action::Move { cast: true, .. });
                p.charge_start = matches!(action, Action::Move { .. }) && !p.cast && needs_charge_turn(prof, level);
                let hits_player = targets.iter().any(|&t| self.battlers[t].side == Side::Player && t != u);
                p.input = if p.cast || p.charge_start {
                    InputKind::None
                } else if side == Side::Player {
                    prof.input
                } else if hits_player {
                    InputKind::Block
                } else {
                    InputKind::None
                };
                if p.input == InputKind::Rhythm {
                    p.beats = max_hits(prof, level);
                    p.guaranteed = prof.min_hits;
                }
                p.anim = if p.cast || p.charge_start {
                    Anim::Gather
                } else if prof.kind == Kind::Damage {
                    if mv.contact {
                        Anim::Lunge
                    } else if mode == Mode::Storm || prof.track == Track::Wave {
                        Anim::Wave
                    } else {
                        Anim::Projectile
                    }
                } else if targets.iter().any(|&t| self.battlers[t].side != side) {
                    Anim::Hex
                } else {
                    Anim::SelfAnim
                };
                p.targets = targets;
            }
            Action::Call => p.anim = Anim::Call,
            Action::Item { item, target, .. } => {
                p.anim = if item.info().ball > 0.0 { Anim::Ball } else { Anim::Item };
                p.targets = target.into_iter().collect();
            }
            Action::Send { .. } | Action::Swap { .. } | Action::Run => {}
        }
        p
    }

    // ------------------------------------------------------------ resolving

    /// Apply a planned action.
    pub fn resolve(&mut self, plan: &Plan, timing: Timing) -> Vec<Event> {
        let mut ev = vec![];
        let u = plan.actor;
        match plan.action.clone() {
            Action::Attack { .. } => self.do_basic(u, plan, timing, &mut ev),
            Action::Move { .. } => self.do_move(u, plan, timing, &mut ev),
            Action::Fire => self.do_fire(u, plan, timing, &mut ev),
            Action::Send { mon } => self.do_send(u, mon, &mut ev),
            Action::Swap { mon } => self.do_swap(u, mon, &mut ev),
            Action::Item { item, target, mon } => self.do_item(u, item, target, mon, &mut ev),
            Action::Run => self.do_run(u, &mut ev),
            Action::Call => self.do_call(u, &mut ev),
        }
        self.after_action(u, &mut ev);
        ev
    }

    fn after_action(&mut self, u: Uid, ev: &mut Vec<Event>) {
        if self.has(u, "rest_every_other") && self.on_field(u) {
            let b = self.bm(u);
            b.loafing = !b.loafing;
        }
        self.cleanup(ev);
        self.entry_abilities(ev);
    }

    fn do_basic(&mut self, u: Uid, plan: &Plan, timing: Timing, ev: &mut Vec<Event>) {
        let side = self.battlers[u].side;
        let timed = side == Side::Player && timing.success;
        let opp = self.field(side.other()).to_vec();
        let t = plan.targets.first().copied().filter(|&t| self.on_field(t)).or(opp.first().copied());
        ev.push(Event::Msg(format!("{} attacks!", self.name(u))));
        let ty = plan.ty.unwrap_or(self.types(u)[0]);
        if let Some(t) = t {
            let hits = if timed { 2 } else { 1 };
            for h in 0..hits {
                if self.mon(t).hp <= 0 {
                    break;
                }
                if self.battlers[t].protect {
                    ev.push(Event::Msg(format!("{} protected itself!", self.name(t))));
                    break;
                }
                let blocked = self.battlers[t].side == Side::Player && side == Side::Foe && timing.success;
                let mut o = DamageOpts::new(self.config.basic_power as f64, Some(ty), self.basic_category(u));
                o.extra_mul = (if h == 1 { self.config.timed_echo as f64 } else { 1.0 }) * (if blocked { self.config.timed_block as f64 } else { 1.0 });
                let r = self.calc_damage(u, t, &o);
                self.hurt(t, r.damage, ev, HitInfo { from: Some(u), eff: r.eff, crit: r.crit, blocked, echo: h == 1, self_hit: false });
                if r.eff > 0.0 {
                    self.break_lock(t, Some(ty), ev);
                    if self.basic_category(u) == Category::Physical {
                        self.contact_abilities(u, t, ev);
                    }
                }
            }
        }
        let gain = self.config.basic_restore + if timed { self.config.timed_restore } else { 0 };
        self.gain_mp(u, gain, ev);
    }

    fn do_move(&mut self, u: Uid, plan: &Plan, timing: Timing, ev: &mut Vec<Event>) {
        let id = self.mon(u).moves[plan.slot].id;
        let prof = self.dex.profile(id).expect("unsupported move").clone();
        let mv = self.dex.mv(id).clone();
        let cost = self.move_cost(u, plan.slot, &plan.targets);
        if self.mon(u).mp < cost {
            ev.push(Event::Msg(format!("{} doesn't have enough MP!", self.name(u))));
            return;
        }
        self.gain_mp(u, -cost, ev);
        if plan.cast {
            let locks = self.make_locks(u, &mv, &prof);
            let big = mv.power >= self.config.lock_big_power || prof.charge_turn;
            ev.push(Event::Cast { uid: u, mv: id, locks: locks.iter().map(|l| l.ty).collect(), targets: plan.targets.clone(), rounds: if big { 2 } else { 1 } });
            ev.push(Event::Msg(format!("{} is gathering power for {}!", self.name(u), mv.name)));
            self.bm(u).cast = Some(Cast { slot: plan.slot, level: plan.level, mode: plan.mode, targets: plan.targets.clone(), locks, wait: big as i32 });
            return;
        }
        if plan.charge_start {
            self.bm(u).charging = Some(Charging { slot: plan.slot, level: plan.level, mode: plan.mode, targets: plan.targets.clone() });
            ev.push(Event::Charge { uid: u, mv: id });
            ev.push(Event::Msg(format!("{} began charging {}!", self.name(u), mv.name)));
            for &(s, d) in &prof.charge_stats {
                self.change_stat(u, s, d, ev, Some(u));
            }
            if plan.level >= 3 {
                let b = self.bm(u);
                b.bracket = b.bracket.max(1);
            }
            return;
        }
        self.gain_xp(u, plan.slot, timing, ev);
        self.execute(u, &mv, &prof, plan, timing, ev, 1.0, 0);
        if mv.priority != 0 {
            self.bm(u).bracket = mv.priority;
        }
    }

    fn do_fire(&mut self, u: Uid, plan: &Plan, timing: Timing, ev: &mut Vec<Event>) {
        let id = plan.mv.expect("fire without a move");
        let mv = self.dex.mv(id).clone();
        let prof = self.dex.profile(id).expect("unsupported move").clone();
        if let Some(c) = self.bm(u).cast.take() {
            let broken = c.locks.iter().filter(|l| l.broken).count();
            if broken == c.locks.len() {
                ev.push(Event::CastEnd { uid: u, cancelled: true });
                ev.push(Event::Msg(format!("{}'s {} was broken! It's staggered!", self.name(u), mv.name)));
                self.bm(u).stunned = 1;
                return;
            }
            ev.push(Event::CastEnd { uid: u, cancelled: false });
            self.execute(u, &mv, &prof, plan, timing, ev, 1.0 - self.config.lock_cut as f64 * broken as f64, broken);
            return;
        }
        if self.bm(u).charging.take().is_some() {
            ev.push(Event::CastEnd { uid: u, cancelled: false });
            self.gain_xp(u, plan.slot, timing, ev);
            self.execute(u, &mv, &prof, plan, timing, ev, 1.0, 0);
        }
    }

    fn gain_xp(&mut self, u: Uid, slot: usize, timing: Timing, ev: &mut Vec<Event>) {
        if self.battlers[u].side != Side::Player {
            return;
        }
        let before = self.move_level(u, slot);
        let gain = self.config.xp_use + if timing.success { self.config.xp_timed } else { 0 };
        self.mon_mut(u).moves[slot].xp += gain;
        let after = self.move_level(u, slot);
        if after > before {
            let id = self.mon(u).moves[slot].id;
            ev.push(Event::MoveLevel { uid: u, mv: id, level: after });
            ev.push(Event::Msg(format!("{}'s {} grew to level {after}!", self.name(u), self.dex.mv(id).name)));
        }
    }

    pub fn move_type(&self, u: Uid, mv: &MoveData) -> u8 {
        if mv.effect == "WEATHER_POWER" {
            if let Some(w) = self.weather {
                return if w.kind == WeatherKind::Rain { self.dex.ty.water } else { self.dex.ty.fire };
            }
        }
        if mv.effect == "PERSONAL_TYPE" {
            return self.types(u)[0];
        }
        mv.ty
    }

    fn move_power(&mut self, u: Uid, t: Uid, mv: &MoveData) -> f64 {
        let m = self.mon(u);
        let hp = m.hp as f64 / m.stats.hp as f64;
        match mv.effect.as_str() {
            "POWER_BY_FRIENDSHIP" | "POWER_BY_UNFRIENDLINESS" => 102.0,
            "PERSONAL_TYPE" => 70.0,
            "POWER_BY_WEIGHT" => 60.0,
            "POWER_BY_LOW_HP" => {
                if hp < 0.042 { 200.0 } else if hp < 0.104 { 150.0 } else if hp < 0.208 { 100.0 } else if hp < 0.354 { 80.0 } else if hp < 0.688 { 40.0 } else { 20.0 }
            }
            "POWER_BY_HP" => (150.0 * hp).floor().max(1.0),
            "RANDOM_POWER" => *self.rng.pick(&[10.0, 30.0, 50.0, 50.0, 70.0, 70.0, 70.0, 90.0, 90.0, 110.0, 150.0]),
            "RANDOM_HIT_OR_HEAL" => *self.rng.pick(&[40.0, 40.0, 80.0, 120.0]),
            "WEATHER_POWER" => if self.weather.is_some() { 100.0 } else { 50.0 },
            "DOUBLE_IF_STATUSED" => if m.status.is_some() { 140.0 } else { 70.0 },
            "DOUBLE_ON_PARALYZED" => if self.mon(t).status == Some(Status::Par) { 120.0 } else { 60.0 },
            "DOUBLE_IF_HURT" => if self.battlers[u].hit_this_round { 120.0 } else { 60.0 },
            _ => mv.power as f64,
        }
    }

    /// Everything a used move does to its targets.
    #[allow(clippy::too_many_arguments)]
    fn execute(&mut self, u: Uid, mv: &MoveData, prof: &Profile, plan: &Plan, timing: Timing, ev: &mut Vec<Event>, lock_mul: f64, broken: usize) {
        let player = self.battlers[u].side == Side::Player;
        let timed = player && (timing.success || (prof.track == Track::Multi && timing.hits.unwrap_or(0) > prof.min_hits));
        let native = prof.native == plan.mode || prof.native == Mode::SelfOnly;
        let mut text = format!("{} used {}!", self.name(u), mv.name);
        if !native {
            text += &format!(" ({})", plan.mode.name());
        }
        if broken > 0 {
            text += &format!(" ({broken} lock{} broken)", if broken > 1 { "s" } else { "" });
        }
        ev.push(Event::Msg(text));
        let targets: Vec<Uid> = plan.targets.iter().copied().filter(|&t| self.alive_on_field(t)).collect();
        let blocked = |b: &Battle, t: Uid| !player && b.battlers[t].side == Side::Player && timing.success;
        match prof.kind {
            Kind::Damage => self.damage_move(u, mv, prof, plan, &targets, timing, timed, &blocked, lock_mul, ev),
            Kind::Boost => self.boost_move(u, prof, plan.level, timed, ev),
            Kind::Hex | Kind::Ailment => self.status_move(u, mv, prof, plan, &targets, timed, &blocked, ev),
            Kind::Heal => self.heal_move(u, mv, prof, plan, &targets, timed, ev),
            Kind::Field => self.field_move(u, mv, prof, plan, &targets, timed, &blocked, ev),
        }
        if prof.recharge && self.mon(u).hp > 0 && needs_recharge(prof, plan.level, timed) {
            self.bm(u).recharge = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn damage_move(
        &mut self, u: Uid, mv: &MoveData, prof: &Profile, plan: &Plan, targets: &[Uid], timing: Timing, timed: bool,
        blocked_by: &dyn Fn(&Battle, Uid) -> bool, lock_mul: f64, ev: &mut Vec<Event>,
    ) {
        if targets.is_empty() {
            ev.push(Event::Msg("But there was no target...".into()));
            return;
        }
        let level = plan.level;
        let ty = self.move_type(u, mv);
        let multi = prof.track == Track::Multi;
        let spread = if prof.track == Track::Wave { &self.config.spread_wave } else { &self.config.spread_strike };
        // multi-hit moves spread their hits over the targets instead of losing power
        let spread_mul = if multi {
            1.0
        } else {
            match plan.mode {
                Mode::Focus => spread.focus,
                Mode::Chain => spread.chain,
                Mode::Storm => spread.storm,
                _ => 1.0,
            }
        } as f64;
        let power_mul = 1.0 + power_bonus(level, &self.config) as f64;
        let mut hits = 1u8;
        if multi {
            let max = max_hits(prof, level);
            hits = if self.battlers[u].side == Side::Player {
                timing.hits.unwrap_or(prof.min_hits).min(max).max(prof.min_hits)
            } else {
                self.rng.int(prof.min_hits as i32, max as i32) as u8
            };
        }
        let order: Vec<Uid> = if multi { (0..hits as usize).map(|i| targets[i % targets.len()]).collect() } else { targets.to_vec() };
        let mut dealt_total = 0;
        let mut missed: Vec<Uid> = vec![];
        let mut hit_count: Vec<(Uid, u32)> = vec![];
        for t in order {
            if self.mon(t).hp <= 0 || missed.contains(&t) {
                continue;
            }
            let first = !hit_count.iter().any(|&(x, _)| x == t);
            if first {
                if self.battlers[t].protect && t != u {
                    ev.push(Event::Msg(format!("{} protected itself!", self.name(t))));
                    missed.push(t);
                    continue;
                }
                if !self.accuracy_check(u, t, mv, prof, timed, 1.0, 0.0) {
                    ev.push(Event::Miss { uid: t });
                    ev.push(Event::Msg(format!("{}'s attack missed {}!", self.name(u), self.name(t))));
                    missed.push(t);
                    if prof.crash {
                        let crash = (self.mon(u).stats.hp / 8).max(1);
                        self.hurt(u, crash, ev, HitInfo::own());
                        ev.push(Event::Msg(format!("{} kept going and crashed!", self.name(u))));
                    }
                    continue;
                }
                if prof.needs_sleep && self.mon(t).status != Some(Status::Slp) {
                    ev.push(Event::Msg(format!("{} wasn't affected!", self.name(t))));
                    missed.push(t);
                    continue;
                }
                if self.absorbs(t, Some(ty), ev) {
                    missed.push(t);
                    continue;
                }
            }
            let n = match hit_count.iter_mut().find(|(x, _)| *x == t) {
                Some(e) => {
                    e.1 += 1;
                    e.1
                }
                None => {
                    hit_count.push((t, 1));
                    1
                }
            };
            let blocked = blocked_by(self, t);
            let power = self.move_power(u, t, mv) * power_mul * if prof.ramp { n as f64 } else { 1.0 };
            let mut o = DamageOpts::new(power, Some(ty), mv.category);
            o.fixed = prof.fixed;
            o.high_crit = prof.high_crit;
            o.extra_mul = spread_mul
                * (if timed && !multi { self.config.timed_damage as f64 } else { 1.0 })
                * lock_mul
                * (if blocked { self.config.timed_block as f64 } else { 1.0 });
            let r = self.calc_damage(u, t, &o);
            if r.eff == 0.0 {
                ev.push(Event::Msg(format!("It doesn't affect {}...", self.name(t))));
                missed.push(t);
                continue;
            }
            let mut amount = r.damage;
            if prof.leave_one_hp {
                amount = amount.min(self.mon(t).hp - 1);
            }
            dealt_total += self.hurt(t, amount, ev, HitInfo { from: Some(u), eff: r.eff, crit: r.crit, blocked, echo: false, self_hit: false });
            self.break_lock(t, Some(ty), ev);
            if mv.contact {
                self.contact_abilities(u, t, ev);
            }
            self.bm(t).hit_this_round = true;
            if prof.drain > 0.0 && self.mon(u).hp > 0 {
                self.heal(u, ((amount as f32 * prof.drain).floor() as i32).max(1), ev);
            }
        }
        if multi && hits > 1 {
            ev.push(Event::Msg(format!("Hit {hits} times!")));
        }
        // secondary effects, once per target that was hit
        for &(t, _) in &hit_count {
            let Some(s) = prof.secondary.clone() else { continue };
            if self.mon(t).hp <= 0 || blocked_by(self, t) || (self.has(t, "no_side_effects") && self.battlers[t].side != self.battlers[u].side) {
                continue;
            }
            let mut chance = mv.chance as f64;
            if timed && level >= 3 {
                chance *= self.config.timed_secondary_mul;
            }
            if self.has(u, "double_chances") {
                chance *= 2.0;
            }
            if !self.rng.chance(chance.min(100.0) / 100.0) {
                continue;
            }
            match s {
                Secondary::Ailment(Ailment::Conf) => {
                    self.confuse(t, ev, false);
                }
                Secondary::Ailment(Ailment::Status(st)) => {
                    self.inflict(t, st, ev, Some(u), false);
                }
                Secondary::Ailment(Ailment::Drowsy) => {}
                Secondary::Tri => {
                    let st = *self.rng.pick(&[Status::Brn, Status::Par, Status::Frz]);
                    self.inflict(t, st, ev, Some(u), false);
                }
                Secondary::Flinch => {
                    if self.queue.contains(&t) && !self.has(t, "no_flinch") {
                        self.bm(t).flinch = true;
                    }
                }
                Secondary::Stats(list) => {
                    for (st, d) in list {
                        self.change_stat(t, st, d, ev, Some(u));
                    }
                }
                Secondary::SelfStats(list) => {
                    for (st, d) in list {
                        self.change_stat(u, st, d, ev, Some(u));
                    }
                }
            }
        }
        // level 5 + timed: an echo hit on the first target still standing
        let first_hit = hit_count.iter().map(|&(t, _)| t).find(|&t| self.mon(t).hp > 0);
        if timed && level >= 5 && !multi && prof.fixed.is_none() {
            if let Some(t) = first_hit {
                let power = self.move_power(u, t, mv) * power_mul;
                let mut o = DamageOpts::new(power, Some(ty), mv.category);
                o.extra_mul = self.config.timed_echo as f64;
                let r = self.calc_damage(u, t, &o);
                if r.eff > 0.0 {
                    dealt_total += self.hurt(t, r.damage, ev, HitInfo { from: Some(u), eff: r.eff, crit: r.crit, echo: true, ..Default::default() });
                    self.break_lock(t, Some(ty), ev);
                    ev.push(Event::Msg("An echo hit!".into()));
                }
            }
        }
        if !hit_count.is_empty() {
            for &(st, d) in &prof.self_drop {
                self.change_stat(u, st, d, ev, Some(u));
            }
        }
        if prof.recoil > 0.0 && dealt_total > 0 && !self.has(u, "no_recoil") {
            self.hurt(u, ((dealt_total as f32 * prof.recoil).floor() as i32).max(1), ev, HitInfo::own());
            ev.push(Event::Msg(format!("{} is hit with recoil!", self.name(u))));
        }
        if prof.breaks_screens {
            for &(t, _) in &hit_count {
                let s = &mut self.sides[self.battlers[t].side.i()];
                s.physical_screen = 0;
                s.special_screen = 0;
            }
        }
        if prof.self_faint {
            let hp = self.mon(u).hp;
            self.hurt(u, hp, ev, HitInfo::own());
        }
    }

    fn boost_move(&mut self, u: Uid, prof: &Profile, level: u8, timed: bool, ev: &mut Vec<Event>) {
        if prof.max_attack {
            let m = self.mon(u);
            if m.hp <= m.stats.hp / 2 {
                ev.push(Event::Msg("But it failed!".into()));
                return;
            }
            let half = m.stats.hp / 2;
            self.hurt(u, half, ev, HitInfo::own());
            self.change_stat(u, Stat::Atk, 12, ev, Some(u));
            return;
        }
        for (i, &(st, d)) in prof.stats.iter().enumerate() {
            let extra = if i == 0 && timed { d.signum() } else { 0 };
            self.change_stat(u, st, d + extra, ev, Some(u));
        }
        if prof.charge {
            self.bm(u).charged = true;
            ev.push(Event::Msg(format!("{} began charging power!", self.name(u))));
        }
        let first = prof.stats[0].0;
        let allies: Vec<Uid> = if level >= 5 {
            self.field(self.battlers[u].side).iter().copied().filter(|&x| x != u).collect()
        } else if level >= 3 {
            self.neighbours(u)
        } else {
            vec![]
        };
        for a in allies {
            self.change_stat(a, first, 1, ev, Some(u));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn status_move(
        &mut self, u: Uid, mv: &MoveData, prof: &Profile, plan: &Plan, targets: &[Uid], timed: bool,
        blocked_by: &dyn Fn(&Battle, Uid) -> bool, ev: &mut Vec<Event>,
    ) {
        if targets.is_empty() {
            ev.push(Event::Msg("But there was no target...".into()));
            return;
        }
        let wider = if prof.native == Mode::Storm { 1.0 } else { self.status_scale(plan.mode) };
        for &t in targets {
            if self.battlers[t].protect && t != u {
                ev.push(Event::Msg(format!("{} protected itself!", self.name(t))));
                continue;
            }
            let bonus = if prof.kind == Kind::Ailment { 5.0 * (plan.level as f64 - 1.0) } else { 0.0 };
            if !self.accuracy_check(u, t, mv, prof, timed, wider, bonus) {
                ev.push(Event::Miss { uid: t });
                ev.push(Event::Msg(format!("{} avoided it!", self.name(t))));
                continue;
            }
            if blocked_by(self, t) && self.rng.chance(self.config.timed_block_resist) {
                ev.push(Event::Msg(format!("{} shrugged it off!", self.name(t))));
                continue;
            }
            if prof.kind == Kind::Hex {
                let more = timed as i8 + if prof.native == Mode::Storm { (plan.level >= 3) as i8 + (plan.level >= 5) as i8 } else { 0 };
                for (i, &(st, d)) in prof.stats.iter().enumerate() {
                    self.change_stat(t, st, d - if i == 0 { more } else { 0 }, ev, Some(u));
                }
            } else {
                for &(st, d) in &prof.target_stats {
                    self.change_stat(t, st, d, ev, Some(u));
                }
                let status = prof.status.expect("ailment without status");
                if mv.ty == self.dex.ty.electric && status == Ailment::Status(Status::Par) && self.types(t).contains(&self.dex.ty.ground) {
                    ev.push(Event::Msg(format!("It doesn't affect {}...", self.name(t))));
                    continue;
                }
                match status {
                    Ailment::Conf => {
                        self.confuse(t, ev, true);
                    }
                    Ailment::Drowsy => {
                        if self.mon(t).status.is_some() || self.battlers[t].drowsy > 0 || status_immune(self.effect(t), Ailment::Drowsy) {
                            ev.push(Event::Msg("But it failed!".into()));
                        } else {
                            self.bm(t).drowsy = 2;
                            ev.push(Event::Msg(format!("{} grew drowsy!", self.name(t))));
                        }
                    }
                    Ailment::Status(st) => {
                        self.inflict(t, st, ev, Some(u), true);
                    }
                }
            }
        }
    }

    fn status_scale(&self, mode: Mode) -> f64 {
        let s = &self.config.status_accuracy;
        (match mode {
            Mode::Focus => s.focus,
            Mode::Chain => s.chain,
            Mode::Storm => s.storm,
            _ => 1.0,
        }) as f64
    }

    #[allow(clippy::too_many_arguments)]
    fn heal_move(&mut self, u: Uid, mv: &MoveData, prof: &Profile, plan: &Plan, targets: &[Uid], timed: bool, ev: &mut Vec<Event>) {
        if prof.heal == Some(Heal::Rest) {
            let m = self.mon(u);
            if m.hp >= m.stats.hp {
                ev.push(Event::Msg("But it failed!".into()));
                return;
            }
            let max = m.stats.hp;
            let m = self.mon_mut(u);
            m.status = Some(Status::Slp);
            m.sleep = if plan.level >= 3 { 2 } else { 3 };
            ev.push(Event::Status { uid: u, status: Some(Status::Slp) });
            ev.push(Event::Msg(format!("{} slept and became healthy!", self.name(u))));
            self.heal(u, max, ev);
            return;
        }
        let t = targets.first().copied().unwrap_or(u);
        let mut frac = match prof.heal {
            Some(Heal::Frac(f)) => f as f64,
            _ => 0.5,
        };
        let period = match mv.effect.as_str() {
            "HEAL_MORE_IN_MORNING" => Some(Period::Morning),
            "HEAL_MORE_BY_DAY" => Some(Period::Day),
            "HEAL_MORE_AT_NIGHT" => Some(Period::Night),
            _ => None,
        };
        if period == Some(self.period) {
            frac += 0.25;
        }
        if let (Some(_), Some(w)) = (period, self.weather) {
            frac += if w.kind == WeatherKind::Sun { 0.25 } else { -0.25 };
        }
        if timed {
            frac += 0.25;
        }
        let m = self.mon(t);
        if m.hp >= m.stats.hp {
            ev.push(Event::Msg(format!("{}'s HP is full!", self.name(t))));
        } else {
            let amount = ((m.stats.hp as f64 * frac.min(1.0)).floor() as i32).max(1);
            self.heal(t, amount, ev);
        }
        if plan.level >= 5 {
            for n in self.neighbours(t) {
                let m = self.mon(n);
                if m.hp < m.stats.hp {
                    let amount = m.stats.hp / 4;
                    self.heal(n, amount, ev);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn field_move(
        &mut self, u: Uid, mv: &MoveData, prof: &Profile, plan: &Plan, targets: &[Uid], timed: bool,
        blocked_by: &dyn Fn(&Battle, Uid) -> bool, ev: &mut Vec<Event>,
    ) {
        let side = self.battlers[u].side;
        let rounds = |base: i32, c: &Config| base + c.field_per_level * (plan.level as i32 - 1) + if timed { c.field_timed } else { 0 };
        match prof.field.expect("field move without kind") {
            kind @ (FieldKind::PhysicalScreen | FieldKind::SpecialScreen) => {
                let n = rounds(self.config.screen_rounds, &self.config);
                let s = &mut self.sides[side.i()];
                let slot = if kind == FieldKind::PhysicalScreen { &mut s.physical_screen } else { &mut s.special_screen };
                if *slot > 0 {
                    ev.push(Event::Msg("But it failed!".into()));
                    return;
                }
                *slot = n;
                ev.push(Event::Screen { side, kind, on: true });
                ev.push(Event::Msg(format!(
                    "{} raised your team's {}! ({n} rounds)",
                    mv.name,
                    if kind == FieldKind::PhysicalScreen { "Defense" } else { "Sp. Def" }
                )));
            }
            kind @ (FieldKind::Rain | FieldKind::Sun) => {
                let n = rounds(self.config.weather_rounds, &self.config);
                let wk = if kind == FieldKind::Rain { WeatherKind::Rain } else { WeatherKind::Sun };
                self.weather = Some(Weather { kind: wk, rounds: n });
                ev.push(Event::Weather { kind: Some(wk) });
                ev.push(Event::Msg(if wk == WeatherKind::Rain { format!("It started to rain! ({n} rounds)") } else { format!("The sunlight got bright! ({n} rounds)") }));
            }
            FieldKind::ResetStats => {
                for x in self.all_on_field() {
                    self.bm(x).stages = [0; 8];
                }
                ev.push(Event::Msg("All stat changes were eliminated!".into()));
                ev.push(Event::StagesReset);
            }
            FieldKind::Guard => {
                if !timed && !self.rng.chance(0.6) {
                    ev.push(Event::Msg("But it failed!".into()));
                    return;
                }
                let mut covered = vec![u];
                if plan.level >= 5 {
                    covered.extend(self.field(side).iter().copied().filter(|&x| x != u));
                } else if plan.level >= 3 {
                    covered.extend(self.neighbours(u));
                }
                for &x in &covered {
                    self.bm(x).protect = true;
                    ev.push(Event::Protect { uid: x });
                }
                ev.push(Event::Msg(if covered.len() > 1 { format!("{} protected its team!", self.name(u)) } else { format!("{} protected itself!", self.name(u)) }));
            }
            FieldKind::Seed => {
                for &t in targets {
                    if self.battlers[t].protect {
                        ev.push(Event::Msg(format!("{} protected itself!", self.name(t))));
                        continue;
                    }
                    if self.types(t).contains(&self.dex.ty.grass) || self.battlers[t].seeded_by.is_some() {
                        ev.push(Event::Msg(format!("It doesn't affect {}...", self.name(t))));
                        continue;
                    }
                    let wider = self.status_scale(plan.mode);
                    if !self.accuracy_check(u, t, mv, prof, timed, wider, 0.0) {
                        ev.push(Event::Miss { uid: t });
                        ev.push(Event::Msg(format!("{} avoided it!", self.name(t))));
                        continue;
                    }
                    if blocked_by(self, t) && self.rng.chance(self.config.timed_block_resist) {
                        ev.push(Event::Msg(format!("{} shrugged it off!", self.name(t))));
                        continue;
                    }
                    self.bm(t).seeded_by = Some(u);
                    ev.push(Event::Seed { uid: t });
                    ev.push(Event::Msg(format!("{} was seeded!", self.name(t))));
                }
            }
            FieldKind::CureTeam => {
                for m in self.sides[side.i()].party.iter_mut() {
                    m.status = None;
                }
                for x in self.field(side).to_vec() {
                    ev.push(Event::Status { uid: x, status: None });
                }
                ev.push(Event::Msg("A bell chimed! Your team is cured.".into()));
            }
            FieldKind::Nothing => ev.push(Event::Msg("But nothing happened!".into())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn accuracy_check(&mut self, u: Uid, t: Uid, mv: &MoveData, prof: &Profile, timed: bool, scale: f64, bonus: f64) -> bool {
        if timed || prof.always_hit || mv.accuracy == 0 {
            return true;
        }
        let mut acc = (mv.accuracy as f64 + bonus) * scale * acc_mul(self.battlers[u].stages[Stat::Acc.idx()] - self.battlers[t].stages[Stat::Eva.idx()]);
        if self.has(u, "sharp_aim") {
            acc *= 1.3;
        }
        if self.has(u, "power_over_aim") && mv.category == Category::Physical {
            acc *= 0.8;
        }
        if mv.effect == "RAIN_SURE_PARALYZE_HIT" {
            if let Some(w) = self.weather {
                acc = if w.kind == WeatherKind::Rain { 1000.0 } else { acc * 0.5 };
            }
        }
        self.rng.next() * 100.0 < acc
    }

    /// Type immunities from abilities. True when the hit is stopped.
    fn absorbs(&mut self, t: Uid, ty_: Option<u8>, ev: &mut Vec<Event>) -> bool {
        let Some(ty_) = ty_ else { return false };
        let effect = self.effect(t).to_string();
        let (name, ability) = (self.name(t), self.ability_name(t));
        let quarter = self.mon(t).stats.hp / 4;
        let roles = self.dex.ty;
        let type_name = self.dex.types.get(ty_ as usize).cloned().unwrap_or_default();
        if effect == "float" && ty_ == roles.ground {
            ev.push(Event::Msg(format!("{name} makes {type_name} moves miss with {ability}!")));
        } else if (effect == "absorb_electric" && ty_ == roles.electric) || (effect == "absorb_water" && ty_ == roles.water) {
            ev.push(Event::Msg(format!("{name}'s {ability} restored its HP!")));
            self.heal(t, quarter, ev);
        } else if effect == "absorb_fire" && ty_ == roles.fire {
            self.bm(t).fire_boost = true;
            ev.push(Event::Msg(format!("{name}'s {ability} raised its {type_name} power!")));
        } else if effect == "only_super_effective" && self.dex.effectiveness(ty_, self.types(t)) <= 1.0 {
            ev.push(Event::Msg(format!("{name} avoided damage with {ability}!")));
        } else {
            return false;
        }
        true
    }

    fn contact_abilities(&mut self, u: Uid, t: Uid, ev: &mut Vec<Event>) {
        if self.mon(u).hp <= 0 {
            return;
        }
        let effect = self.effect(t).to_string();
        match effect.as_str() {
            "contact_paralysis" if self.rng.chance(0.3) => {
                self.inflict(u, Status::Par, ev, Some(t), false);
            }
            "contact_burn" if self.rng.chance(0.3) => {
                self.inflict(u, Status::Brn, ev, Some(t), false);
            }
            "contact_poison" if self.rng.chance(0.3) => {
                self.inflict(u, Status::Psn, ev, Some(t), false);
            }
            "contact_spores" if self.rng.chance(0.1) => {
                let st = *self.rng.pick(&[Status::Psn, Status::Par, Status::Slp]);
                self.inflict(u, st, ev, Some(t), false);
            }
            "contact_damage" => {
                let n = (self.mon(u).stats.hp / 16).max(1);
                self.hurt(u, n, ev, HitInfo::own());
                ev.push(Event::Msg(format!("{} was hurt by {}!", self.name(u), self.ability_name(t))));
            }
            _ => {}
        }
    }

    /// Gen 3 damage formula + this system's multipliers (extra_mul).
    fn calc_damage(&mut self, u: Uid, t: Uid, o: &DamageOpts) -> DamageRoll {
        let eff = match o.ty {
            Some(x) => self.dex.effectiveness(x, self.types(t)),
            None => 1.0,
        };
        if eff == 0.0 {
            return DamageRoll { damage: 0, eff: 0.0, crit: false };
        }
        if let Some(f) = o.fixed {
            let l = self.mon(u).level as i32;
            let (thp, uhp) = (self.mon(t).hp, self.mon(u).hp);
            let dmg = match f {
                Fixed::Level => l,
                Fixed::Half => (thp / 2).max(1),
                Fixed::RandomLevel => ((l as f64 * (self.rng.int(5, 15) as f64 / 10.0)).floor() as i32).max(1),
                Fixed::MatchHp => (thp - uhp).max(0),
                Fixed::Amount(n) => n,
            };
            return DamageRoll { damage: dmg, eff: 1.0, crit: false };
        }
        let stage = (self.battlers[u].stages[Stat::Crit.idx()] + o.high_crit as i8).clamp(0, 4) as usize;
        let crit = !o.no_crit && self.rng.chance(CRIT_CHANCE[stage]);
        let physical = o.category == Category::Physical;
        let (bu, bt) = (&self.battlers[u], &self.battlers[t]);
        let mut a_s = if physical { bu.stages[Stat::Atk.idx()] } else { bu.stages[Stat::Spa.idx()] };
        let mut d_s = if physical { bt.stages[Stat::Def.idx()] } else { bt.stages[Stat::Spd.idx()] };
        if crit {
            a_s = a_s.max(0);
            d_s = d_s.min(0);
        }
        let (mu, mt) = (self.mon(u), self.mon(t));
        let mut a = (if physical { mu.stats.atk } else { mu.stats.spa }) as f64 * stage_mul(a_s);
        let mut d = (if physical { mt.stats.def } else { mt.stats.spd }) as f64 * stage_mul(d_s);
        let ab = self.dex.ability_effect(&mu.ability);
        if physical && ab == "double_attack" {
            a *= 2.0;
        }
        if physical && ab == "power_over_aim" {
            a *= 1.5;
        }
        if physical && ab == "status_power" && mu.status.is_some() {
            a *= 1.5;
        }
        if physical && self.dex.ability_effect(&mt.ability) == "status_defense" && mt.status.is_some() {
            d *= 1.5;
        }
        if self.dex.ability_effect(&mt.ability) == "resist_fire_ice" && o.ty.is_some_and(|x| x == self.dex.ty.fire || x == self.dex.ty.ice) {
            a *= 0.5;
        }
        let mut power = o.power;
        if o.ty.is_some() && pinch(&self.dex, ab) == o.ty && mu.hp <= mu.stats.hp / 3 {
            power *= 1.5;
        }
        if bu.fire_boost && o.ty == Some(self.dex.ty.fire) {
            power *= 1.5;
        }
        let burned = physical && mu.status == Some(Status::Brn) && ab != "status_power";
        let level = mu.level as f64;
        let stab = o.ty.map(|x| self.types(u).contains(&x)).unwrap_or(false);
        if bu.charged && o.ty == Some(self.dex.ty.electric) {
            power *= 2.0;
            self.bm(u).charged = false;
        }
        let mut dmg = ((((2.0 * level / 5.0 + 2.0).floor() * power * a) / d).floor() / 50.0).floor();
        if burned {
            dmg = (dmg / 2.0).floor();
        }
        let side = &self.sides[self.battlers[t].side.i()];
        if !crit && self.battlers[u].side != self.battlers[t].side && ((physical && side.physical_screen > 0) || (!physical && side.special_screen > 0)) {
            dmg = (dmg / 2.0).floor();
        }
        if let (Some(w), Some(x)) = (self.weather, o.ty) {
            if x == self.dex.ty.water {
                dmg *= if w.kind == WeatherKind::Rain { 1.5 } else { 0.5 };
            }
            if x == self.dex.ty.fire {
                dmg *= if w.kind == WeatherKind::Sun { 1.5 } else { 0.5 };
            }
        }
        dmg = dmg.floor() + 2.0;
        if crit {
            dmg *= 2.0;
        }
        if stab {
            dmg *= 1.5;
        }
        dmg *= eff as f64;
        dmg = ((dmg * self.rng.int(85, 100) as f64) / 100.0).floor();
        dmg *= o.extra_mul;
        DamageRoll { damage: (dmg.floor() as i32).max(1), eff, crit }
    }

    /// Expected damage without luck, for the AI.
    pub fn estimate(&self, u: Uid, t: Uid, mv: &MoveData, mode: Mode) -> f64 {
        let Some(prof) = self.dex.profile(mv.id) else { return 0.0 };
        let ty_ = self.move_type(u, mv);
        let eff = self.dex.effectiveness(ty_, self.types(t)) as f64;
        if eff == 0.0 || (ty_ == self.dex.ty.ground && self.has(t, "float")) {
            return 0.0;
        }
        match prof.fixed {
            Some(Fixed::Level) => return self.mon(u).level as f64,
            Some(Fixed::Amount(n)) => return n as f64,
            Some(Fixed::Half) => return self.mon(t).hp as f64 / 2.0,
            Some(_) => return self.mon(u).level as f64,
            None => {}
        }
        let physical = mv.category == Category::Physical;
        let (bu, bt, mu, mt) = (&self.battlers[u], &self.battlers[t], self.mon(u), self.mon(t));
        let a = (if physical { mu.stats.atk } else { mu.stats.spa }) as f64 * stage_mul(if physical { bu.stages[0] } else { bu.stages[3] });
        let d = (if physical { mt.stats.def } else { mt.stats.spd }) as f64 * stage_mul(if physical { bt.stages[1] } else { bt.stages[4] });
        let mut dmg = ((2.0 * mu.level as f64 / 5.0 + 2.0) * mv.power as f64 * a) / d / 50.0 + 2.0;
        if self.types(u).contains(&ty_) {
            dmg *= 1.5;
        }
        dmg *= eff * 0.925;
        if prof.track == Track::Multi {
            dmg *= (prof.min_hits + prof.max_hits) as f64 / 2.0;
        } else {
            let s = if prof.track == Track::Wave { &self.config.spread_wave } else { &self.config.spread_strike };
            dmg *= match mode {
                Mode::Focus => s.focus,
                Mode::Chain => s.chain,
                Mode::Storm => s.storm,
                _ => 1.0,
            } as f64;
        }
        dmg
    }

    fn hurt(&mut self, t: Uid, amount: i32, ev: &mut Vec<Event>, info: HitInfo) -> i32 {
        let dealt = amount.clamp(0, self.mon(t).hp.max(0));
        self.mon_mut(t).hp -= dealt;
        let hp = self.mon(t).hp;
        ev.push(Event::Damage { uid: t, amount: dealt, hp, eff: info.eff, crit: info.crit, blocked: info.blocked, echo: info.echo, self_hit: info.self_hit, from: info.from });
        if info.crit {
            ev.push(Event::Msg("A critical hit!".into()));
        }
        let name = self.name(t);
        if info.eff > 1.0 {
            ev.push(Event::Msg(format!("It's super effective on {name}!")));
        } else if info.eff > 0.0 && info.eff < 1.0 {
            ev.push(Event::Msg(format!("It's not very effective on {name}...")));
        }
        if info.blocked {
            ev.push(Event::Msg(format!("{name} blocked!")));
        }
        dealt
    }

    fn heal(&mut self, t: Uid, amount: i32, ev: &mut Vec<Event>) -> i32 {
        let m = self.mon(t);
        let healed = amount.clamp(0, (m.stats.hp - m.hp).max(0));
        self.mon_mut(t).hp += healed;
        ev.push(Event::Heal { uid: t, amount: healed, hp: self.mon(t).hp });
        healed
    }

    fn gain_mp(&mut self, u: Uid, amount: i32, ev: &mut Vec<Event>) {
        let m = self.mon_mut(u);
        let before = m.mp;
        m.mp = (m.mp + amount).clamp(0, m.max_mp);
        let (mp, delta) = (m.mp, m.mp - before);
        ev.push(Event::Mp { uid: u, amount: delta, mp });
    }

    fn inflict(&mut self, t: Uid, status: Status, ev: &mut Vec<Event>, _source: Option<Uid>, announce: bool) -> bool {
        let name = self.name(t);
        let fail = |ev: &mut Vec<Event>, why: String| {
            if announce {
                ev.push(Event::Msg(why));
            }
            false
        };
        if self.mon(t).hp <= 0 {
            return false;
        }
        if let Some(s) = self.mon(t).status {
            return fail(ev, format!("{name} is already {}!", if s == Status::Slp { "asleep" } else { "affected" }));
        }
        let immune = self.types(t).iter().any(|&x| self.dex.type_immune(x, status_key(status)));
        if status == Status::Frz && (immune || self.weather.map(|w| w.kind == WeatherKind::Sun).unwrap_or(false)) {
            return false;
        }
        if immune {
            return fail(ev, format!("It doesn't affect {name}..."));
        }
        if status_immune(self.effect(t), Ailment::Status(status)) {
            let ab = self.ability_name(t);
            return fail(ev, format!("{name}'s {ab} prevents it!"));
        }
        let sleep = if status == Status::Slp { if self.has(t, "wake_early") { 2 } else { self.rng.int(2, 5) } } else { 0 };
        let m = self.mon_mut(t);
        m.status = Some(status);
        if status == Status::Slp {
            m.sleep = sleep;
        }
        if status == Status::Tox {
            m.toxic = 0;
        }
        ev.push(Event::Status { uid: t, status: Some(status) });
        ev.push(Event::Msg(format!("{name} {}!", status_text(status))));
        if self.battlers[t].cast.is_some() && matches!(status, Status::Slp | Status::Frz) {
            // falling asleep or freezing breaks a cast
            self.bm(t).cast = None;
            ev.push(Event::CastEnd { uid: t, cancelled: true });
            ev.push(Event::Msg(format!("{name} lost its focus!")));
        }
        true
    }

    fn confuse(&mut self, t: Uid, ev: &mut Vec<Event>, announce: bool) -> bool {
        if self.battlers[t].confusion > 0 || self.has(t, "no_confusion") {
            if announce {
                ev.push(Event::Msg("But it failed!".into()));
            }
            return false;
        }
        let n = self.rng.int(2, 5);
        self.bm(t).confusion = n;
        ev.push(Event::Confused { uid: t });
        ev.push(Event::Msg(format!("{} became confused!", self.name(t))));
        true
    }

    fn change_stat(&mut self, t: Uid, stat: Stat, delta: i8, ev: &mut Vec<Event>, source: Option<Uid>) {
        if self.mon(t).hp <= 0 || delta == 0 {
            return;
        }
        let by_foe = source.map(|s| self.battlers[s].side != self.battlers[t].side).unwrap_or(false);
        if by_foe && delta < 0 {
            let a = self.effect(t).to_string();
            if a == "no_stat_drops" || (a == "keep_attack" && stat == Stat::Atk) || (a == "keep_accuracy" && stat == Stat::Acc) {
                ev.push(Event::Msg(format!("{}'s {} prevents it!", self.name(t), self.ability_name(t))));
                return;
            }
        }
        let (lo, hi) = if stat == Stat::Crit { (0, 4) } else { (-6, 6) };
        let before = self.battlers[t].stages[stat.idx()];
        let after = (before as i32 + delta as i32).clamp(lo, hi) as i8;
        self.bm(t).stages[stat.idx()] = after;
        let d = after - before;
        let name = format!("{}'s {}", self.name(t), stat.name());
        if d == 0 {
            ev.push(Event::Msg(format!("{name} won't go {}!", if delta > 0 { "higher" } else { "lower" })));
            return;
        }
        ev.push(Event::Stat { uid: t, stat, delta: d, stage: after });
        let size = match (d.abs(), d > 0) {
            (3.., true) => "rose drastically",
            (3.., false) => "severely fell",
            (2, true) => "sharply rose",
            (2, false) => "harshly fell",
            (_, true) => "rose",
            (_, false) => "fell",
        };
        ev.push(Event::Msg(format!("{name} {size}!")));
    }

    // ------------------------------------------------------------ locks

    /// The lock types: what the player's side can hit with, super-effective ones first.
    fn make_locks(&mut self, caster: Uid, mv: &MoveData, prof: &Profile) -> Vec<Lock> {
        let players = self.field(Side::Player).to_vec();
        let big = mv.power >= self.config.lock_big_power || prof.charge_turn;
        let n = ((if big { 3 } else { 2 }) + (players.len() >= 3) as usize).min(4);
        let mut avail: Vec<u8> = vec![];
        let add = |t: u8, v: &mut Vec<u8>| {
            if !v.contains(&t) {
                v.push(t);
            }
        };
        for &p in &players {
            add(self.types(p)[0], &mut avail);
            for s in &self.mon(p).moves {
                let m = self.dex.mv(s.id);
                if let Some(pr) = self.dex.profile(s.id) {
                    if pr.kind == Kind::Damage && pr.fixed.is_none() {
                        add(self.move_type(p, m), &mut avail);
                    }
                }
            }
        }
        let caster_types = self.types(caster).to_vec();
        let eff = |t: u8| self.dex.effectiveness(t, &caster_types);
        let usable: Vec<u8> = avail.iter().copied().filter(|&t| eff(t) > 0.0).collect();
        let strong: Vec<u8> = usable.iter().copied().filter(|&t| eff(t) > 1.0).collect();
        let pool: Vec<u8> = if strong.len() >= 2 {
            strong
        } else if !strong.is_empty() {
            let mut p = strong.clone();
            p.extend(&strong);
            p.extend(usable.iter().copied().filter(|&t| eff(t) == 1.0));
            p
        } else {
            usable
        };
        let pool = if pool.is_empty() { vec![if self.dex.ty.normal == NO_TYPE { 0 } else { self.dex.ty.normal }] } else { pool };
        (0..n).map(|_| Lock { ty: *self.rng.pick(&pool), broken: false }).collect()
    }

    fn break_lock(&mut self, t: Uid, ty_: Option<u8>, ev: &mut Vec<Event>) {
        let Some(ty_) = ty_ else { return };
        if self.mon(t).hp <= 0 {
            return;
        }
        let name = self.name(t);
        let Some(c) = self.bm(t).cast.as_mut() else { return };
        let Some(i) = c.locks.iter().position(|l| !l.broken && l.ty == ty_) else { return };
        c.locks[i].broken = true;
        let all = c.locks.iter().all(|l| l.broken);
        ev.push(Event::LockBreak { uid: t, index: i });
        if all {
            ev.push(Event::Msg(format!("Every lock on {name} is broken!")));
        }
    }

    // ------------------------------------------------------------ other actions

    fn do_send(&mut self, u: Uid, mon: usize, ev: &mut Vec<Event>) {
        let side = self.battlers[u].side;
        if self.field(side).len() >= self.config.max_field || !self.bench(side).contains(&mon) {
            ev.push(Event::Msg("But it failed!".into()));
            return;
        }
        let n = self.enter(side, mon, ev, None);
        let name = self.name(n);
        ev.push(Event::Msg(if side == Side::Player { format!("Go! {name}!") } else { format!("{} sent out {name}!", self.sides[side.i()].name) }));
        self.gym_match(ev);
    }

    /// Gym leaders keep as many creatures out as the player (fair fights).
    fn gym_match(&mut self, ev: &mut Vec<Event>) {
        if self.kind != BattleKind::Gym {
            return;
        }
        while self.field(Side::Foe).len() < self.field(Side::Player).len() {
            let Some(&m) = self.bench(Side::Foe).first() else { break };
            let n = self.enter(Side::Foe, m, ev, None);
            ev.push(Event::Msg(format!("{} matches you and sends out {}!", self.sides[1].name, self.name(n))));
        }
    }

    fn do_swap(&mut self, u: Uid, mon: usize, ev: &mut Vec<Event>) {
        let side = self.battlers[u].side;
        if !self.bench(side).contains(&mon) {
            ev.push(Event::Msg("But it failed!".into()));
            return;
        }
        let f = &mut self.sides[side.i()].field;
        let index = f.iter().position(|&x| x == u).unwrap();
        f.remove(index);
        if self.has(u, "cure_on_switch") {
            self.mon_mut(u).status = None;
        }
        ev.push(Event::Leave { uid: u });
        ev.push(Event::Msg(format!("{}, come back!", self.name(u))));
        let n = self.enter(side, mon, ev, Some(index));
        ev.push(Event::Msg(format!("Go! {}!", self.name(n))));
    }

    fn do_item(&mut self, u: Uid, item: Item, target: Option<Uid>, mon: Option<usize>, ev: &mut Vec<Event>) {
        let info = item.info();
        let side = self.battlers[u].side;
        let count = self.sides[side.i()].bag.get(&item).copied().unwrap_or(0);
        if count == 0 {
            ev.push(Event::Msg("But it failed!".into()));
            return;
        }
        self.sides[side.i()].bag.insert(item, count - 1);
        ev.push(Event::Msg(format!("You used a {}!", info.name)));
        if info.ball > 0.0 {
            self.throw_ball(target, info.ball, ev);
            return;
        }
        if info.revive > 0.0 {
            let Some(i) = mon.filter(|&i| self.sides[side.i()].party.get(i).map(|m| m.hp <= 0).unwrap_or(false)) else {
                ev.push(Event::Msg("But it had no effect.".into()));
                return;
            };
            let m = &mut self.sides[side.i()].party[i];
            m.hp = (m.stats.hp as f32 * info.revive).floor() as i32;
            m.status = None;
            let name = m.name.clone();
            ev.push(Event::Revive { side, mon: i });
            ev.push(Event::Msg(format!("{name} was revived!")));
            return;
        }
        let t = target.filter(|&t| t < self.battlers.len()).unwrap_or(u);
        if info.heal > 0 {
            self.heal(t, info.heal, ev);
        }
        if info.mp > 0 {
            self.gain_mp(t, info.mp, ev);
        }
        if info.cure {
            self.mon_mut(t).status = None;
            self.bm(t).confusion = 0;
            ev.push(Event::Status { uid: t, status: None });
            ev.push(Event::Msg(format!("{} was cured!", self.name(t))));
        }
    }

    fn throw_ball(&mut self, target: Option<Uid>, ball: f32, ev: &mut Vec<Event>) {
        if self.kind != BattleKind::Wild {
            ev.push(Event::Msg("You can't catch a trainer's creature!".into()));
            return;
        }
        let foes = self.field(Side::Foe).to_vec();
        let t = target.filter(|t| foes.contains(t)).or(foes.first().copied());
        let Some(t) = t.filter(|_| !(self.config.catch_only_when_alone && foes.len() > 1)) else {
            ev.push(Event::Msg("It can't be caught while others protect it!".into()));
            return;
        };
        let m = self.mon(t);
        let status_mul = match m.status {
            Some(Status::Slp | Status::Frz) => 2.0,
            Some(_) => 1.5,
            None => 1.0,
        };
        let catch_rate = self.dex.species[m.species].catch_rate as f64;
        let x = (((3 * m.stats.hp - 2 * m.hp) as f64 * catch_rate * ball as f64) / (3 * m.stats.hp) as f64) * status_mul;
        let mut shakes = 4u8;
        if x < 255.0 {
            let y = (1048560.0 / (16711680.0 / x.max(1.0)).sqrt().sqrt()).floor();
            shakes = 0;
            while shakes < 4 && (self.rng.int(0, 65535) as f64) < y {
                shakes += 1;
            }
        }
        let caught = shakes == 4;
        ev.push(Event::Catch { uid: t, shakes, caught });
        let name = self.name(t);
        if !caught {
            ev.push(Event::Msg(if shakes > 0 { "Argh! Almost had it!".into() } else { format!("{name} broke free!") }));
            return;
        }
        self.sides[1].field.retain(|&x| x != t);
        self.bm(t).cast = None;
        ev.push(Event::Leave { uid: t });
        ev.push(Event::Msg(format!("Gotcha! {name} was caught!")));
        self.mon_mut(t).status = None;
        let m = self.mon(t).clone();
        if self.sides[0].party.len() < 6 {
            self.sides[0].party.push(m.clone());
        }
        self.caught.push(m);
    }

    fn do_run(&mut self, u: Uid, ev: &mut Vec<Event>) {
        if self.kind != BattleKind::Wild {
            ev.push(Event::Msg("There's no running from a trainer battle!".into()));
            return;
        }
        self.run_attempts += 1;
        let mine = self.field(Side::Player).iter().map(|&x| self.speed(x)).fold(0.0, f64::max);
        let theirs = self.field(Side::Foe).iter().map(|&x| self.speed(x)).fold(0.0, f64::max);
        let ok = self.has(u, "sure_escape") || mine >= theirs || self.rng.chance(0.4 + 0.15 * self.run_attempts as f64);
        if ok {
            ev.push(Event::Msg("Got away safely!".into()));
            self.outcome = Some(Outcome::Ran);
            ev.push(Event::Outcome(Outcome::Ran));
        } else {
            ev.push(Event::Msg("Can't escape!".into()));
        }
    }

    /// Wild creatures call for help: one of the same species, or another from the area.
    fn do_call(&mut self, u: Uid, ev: &mut Vec<Event>) {
        ev.push(Event::Msg(format!("{} called for help!", self.name(u))));
        if self.field(Side::Foe).len() >= self.config.max_field {
            ev.push(Event::Msg("But no one came!".into()));
            return;
        }
        let pool = self.sides[1].pool.clone();
        let (species, level) = if pool.is_empty() || self.rng.chance(0.5) {
            (self.dex.species[self.mon(u).species].key.clone(), self.mon(u).level)
        } else {
            self.rng.pick(&pool).clone()
        };
        let spread = self.config.wild_level_spread;
        let level = (level as i32 + self.rng.int(-spread, spread)).max(2) as u8;
        let dex = self.dex.clone();
        let config = self.config.clone();
        let m = create_mon(&dex, &MonSpec::new(&species, level), Some(&mut self.rng), &config);
        self.sides[1].party.push(m);
        let i = self.sides[1].party.len() - 1;
        let n = self.enter(Side::Foe, i, ev, None);
        ev.push(Event::Msg(format!("A wild {} answered the call!", self.name(n))));
    }

    // ------------------------------------------------------------ bookkeeping

    /// The player must send someone out (empty field, or below a trainer battle's size).
    pub fn needs_replacement(&self) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        let need = if self.kind == BattleKind::Trainer { self.size } else { 1 };
        self.field(Side::Player).len() < need && !self.bench(Side::Player).is_empty()
    }

    pub fn replace(&mut self, mon: usize) -> Vec<Event> {
        let mut ev = vec![];
        let u = self.enter(Side::Player, mon, &mut ev, None);
        ev.push(Event::Msg(format!("Go! {}!", self.name(u))));
        self.gym_match(&mut ev);
        self.entry_abilities(&mut ev);
        ev
    }

    fn cleanup(&mut self, ev: &mut Vec<Event>) {
        for s in [Side::Player, Side::Foe] {
            for u in self.field(s).to_vec() {
                if self.mon(u).hp > 0 {
                    continue;
                }
                self.sides[s.i()].field.retain(|&x| x != u);
                let b = self.bm(u);
                b.cast = None;
                b.charging = None;
                ev.push(Event::Faint { uid: u });
                let prefix = if s == Side::Foe && self.kind == BattleKind::Wild { "The wild " } else { "" };
                ev.push(Event::Msg(format!("{prefix}{} fainted!", self.name(u))));
                self.mon_mut(u).status = None;
                for x in self.all_on_field() {
                    if self.battlers[x].seeded_by == Some(u) {
                        self.bm(x).seeded_by = None;
                    }
                }
            }
        }
        if self.kind != BattleKind::Wild {
            let target = if self.kind == BattleKind::Trainer { self.size } else { self.field(Side::Player).len().max(1) };
            while self.field(Side::Foe).len() < target {
                let Some(&m) = self.bench(Side::Foe).first() else { break };
                let n = self.enter(Side::Foe, m, ev, None);
                ev.push(Event::Msg(format!("{} sent out {}!", self.sides[1].name, self.name(n))));
            }
        }
        if self.outcome.is_some() {
            return;
        }
        if !self.sides[0].party.iter().any(|m| m.hp > 0) {
            self.outcome = Some(Outcome::Lose);
            ev.push(Event::Msg("You have no more creatures that can fight...".into()));
            ev.push(Event::Outcome(Outcome::Lose));
        } else if self.field(Side::Foe).is_empty() && (self.kind == BattleKind::Wild || self.bench(Side::Foe).is_empty()) {
            self.outcome = Some(Outcome::Win);
            let text = if self.kind == BattleKind::Wild {
                if self.caught.is_empty() { "You won!".to_string() } else { "The battle is over!".to_string() }
            } else {
                format!("You defeated {}!", self.sides[1].name)
            };
            ev.push(Event::Msg(text));
            ev.push(Event::Outcome(Outcome::Win));
        }
    }
}
