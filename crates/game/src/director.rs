//! Runs a battle: asks the rules for the next actor, opens the menu or asks the AI,
//! animates the plan while reading the timing input, hands the result back, and plays
//! the events one after the other (each holds the screen for a moment).

use std::collections::VecDeque;

use egui::Color32;
use glam::Vec3;
use rules::ai::{choose_action, simulated_timing};
use rules::battle::{Action, Anim, Battle, Event, Outcome, Plan, Side, Timing, Uid};
use rules::dex::Rng;
use rules::moves::InputKind;

use crate::hud::{self, Cue, Hud};
use crate::input::{Beat, ChargeRead, Input, RhythmRead, TapRead};
use crate::menu::Menu;
use crate::platform;
use crate::stage::{type_color, Stage};

enum Step {
    NeedActor,
    BeginTurn(Uid),
    Decide(Uid, Option<Action>),
    Choose(Uid),
    Replace,
    Perform(Box<Perform>),
    Finished,
}

pub struct Director {
    pub battle: Battle,
    pub stage: Stage,
    pub hud: Hud,
    pub menu: Menu,
    events: VecDeque<Event>,
    wait_until: f64,
    step: Step,
    pub auto: bool,
    pub text_delay: f64,
    pub level_ups: Vec<String>,
    /// cries to play, taken by the app each frame
    pub cries: Vec<String>,
    last_damage: f64,
    sim_rng: Rng,
    pending_hook: String,
}

impl Director {
    pub fn new(mut battle: Battle, stage: Stage, hud: Hud, auto: bool, text_delay: f64) -> Director {
        let events: VecDeque<Event> = battle.start().into();
        Director {
            battle, stage, hud, menu: Menu::new(), events, wait_until: 0.0, step: Step::NeedActor, auto, text_delay,
            level_ups: vec![],
            cries: vec![], last_damage: 0.0, sim_rng: Rng::new(4242), pending_hook: String::new(),
        }
    }

    /// The cries since last asked.
    pub fn take_cries(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cries)
    }

    pub fn finished(&self) -> bool {
        matches!(self.step, Step::Finished) && self.events.is_empty()
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.battle.outcome
    }

    pub fn menu_open(&self) -> bool {
        matches!(self.step, Step::Choose(_) | Step::Replace) && !self.auto
    }

    pub fn update(&mut self, ctx: &egui::Context, input: &Input, now: f64, dt: f32) {
        self.stage.update(now, dt);
        for _ in 0..64 {
            if now < self.wait_until {
                break;
            }
            if let Some(e) = self.events.pop_front() {
                let hold = self.apply(e, now);
                self.wait_until = now + hold;
                continue;
            }
            match std::mem::replace(&mut self.step, Step::Finished) {
                Step::NeedActor => {
                    if self.battle.outcome.is_some() {
                        self.step = Step::Finished;
                        break;
                    }
                    let n = self.battle.next();
                    self.events.extend(n.events);
                    self.step = if n.replace {
                        Step::Replace
                    } else if let Some(a) = n.actor {
                        Step::BeginTurn(a)
                    } else if self.battle.outcome.is_some() {
                        Step::Finished
                    } else {
                        Step::NeedActor
                    };
                }
                Step::BeginTurn(a) => {
                    self.hud.active = Some(a);
                    let t = self.battle.begin_turn(a);
                    self.events.extend(t.events);
                    if !t.can_act {
                        let ev = self.battle.skip_turn(a);
                        self.events.extend(ev);
                        self.step = Step::NeedActor;
                    } else {
                        self.step = Step::Decide(a, t.auto);
                    }
                }
                Step::Decide(a, auto_action) => {
                    let human = self.battle.get(a).side == Side::Player && !self.auto;
                    if let Some(action) = auto_action {
                        self.start_perform(a, action, now);
                    } else if human {
                        self.menu.new_turn();
                        self.stage.focus_on(&[a]);
                        self.stage.targets = vec![(a, true, [0.43, 0.72, 1.0, 0.95])];
                        self.step = Step::Choose(a);
                    } else {
                        let action = choose_action(&mut self.battle, a);
                        self.wait_until = now + 250.0;
                        self.start_perform(a, action, now + 250.0);
                    }
                }
                Step::Choose(a) => {
                    if let Some(action) = self.menu.update(ctx, input, &self.battle, a, &mut self.stage, &mut self.hud) {
                        self.stage.targets.clear();
                        self.hud.pointer = None;
                        self.start_perform(a, action, now);
                    } else {
                        if self.stage.targets.is_empty() {
                            self.stage.targets = vec![(a, true, [0.43, 0.72, 1.0, 0.95])];
                        }
                        self.step = Step::Choose(a);
                        break;
                    }
                }
                Step::Replace => {
                    let pick = if self.auto { self.battle.bench(Side::Player).first().copied() } else { self.menu.replacement(ctx, input, &self.battle) };
                    match pick {
                        Some(mon) => {
                            let ev = self.battle.replace(mon);
                            self.events.extend(ev);
                            self.step = Step::NeedActor;
                        }
                        None => {
                            self.step = Step::Replace;
                            break;
                        }
                    }
                }
                Step::Perform(mut p) => match p.update(input, now, &mut self.stage, &mut self.hud, &self.battle) {
                    Some(timing) => {
                        let timing = if p.simulated { simulated_timing(&p.sim_plan(), &mut self.sim_rng, 0.75) } else { timing };
                        let ev = self.battle.resolve(&p.plan, timing);
                        self.events.extend(ev);
                        self.stage.focus_on(&[]);
                        self.hud.prompt = None;
                        self.step = Step::NeedActor;
                    }
                    None => {
                        self.step = Step::Perform(p);
                        break;
                    }
                },
                Step::Finished => {
                    self.step = Step::Finished;
                    break;
                }
            }
        }
        let fields = [self.battle.field(Side::Player).to_vec(), self.battle.field(Side::Foe).to_vec()];
        self.stage.layout([&fields[0], &fields[1]], now);
        let hook = match &self.step {
            Step::Perform(p) => p.pending_hook(),
            _ => String::new(),
        };
        if hook != self.pending_hook {
            platform::set_hook("pending", &hook);
            self.pending_hook = hook;
        }
    }

    fn start_perform(&mut self, a: Uid, action: Action, now: f64) {
        let plan = self.battle.plan(a, action);
        let p = Perform::start(plan, now, &mut self.stage, &mut self.hud, &self.battle, self.auto);
        self.step = Step::Perform(Box::new(p));
    }

    /// Shows one event; returns how long it holds the screen (ms).
    fn apply(&mut self, e: Event, now: f64) -> f64 {
        let b = &self.battle;
        match e {
            Event::Msg(text) => {
                self.hud.message(&text);
                self.text_delay
            }
            Event::Enter { uid, side, .. } => {
                let m = b.mon(uid);
                let sp = &b.dex.species[m.species];
                self.stage.add_battler(uid, side, &sp.key, sp.front_box, sp.back_box, sp.elevation, sp.light.clone(), sp.sound.clone(), now);
                if let Some(c) = &sp.cry {
                    self.cries.push(c.clone());
                }
                self.hud.shown.insert(uid, (m.hp, m.mp));
                let fields = [b.field(Side::Player).to_vec(), b.field(Side::Foe).to_vec()];
                self.stage.layout([&fields[0], &fields[1]], now);
                260.0
            }
            Event::Leave { uid } => {
                self.stage.remove_battler(uid, false, now);
                220.0
            }
            Event::Faint { uid } => {
                self.stage.remove_battler(uid, true, now);
                450.0
            }
            Event::Damage { uid, amount, hp, eff, crit, blocked, echo, self_hit, .. } => {
                if let Some(s) = self.hud.shown.get_mut(&uid) {
                    s.0 = hp;
                }
                let (color, size) = if blocked {
                    (hud::BLOCK, 20.0)
                } else if self_hit {
                    (Color32::from_rgb(255, 176, 166), 15.0)
                } else if echo {
                    (Color32::from_rgb(226, 179, 255), 17.0)
                } else if eff > 1.0 {
                    (Color32::from_rgb(255, 207, 74), 24.0)
                } else if eff < 1.0 {
                    (Color32::from_rgb(185, 194, 207), 16.0)
                } else if crit {
                    (Color32::from_rgb(255, 138, 61), 20.0)
                } else {
                    (Color32::WHITE, 20.0)
                };
                self.hud.pop(uid, format!("{amount}{}", if crit { "!" } else { "" }), color, size, now);
                if self_hit || echo {
                    self.stage.flash(uid, now, 4.0);
                }
                let gap = now - self.last_damage;
                self.last_damage = now;
                if gap < 200.0 { 110.0 } else { 160.0 }
            }
            Event::Heal { uid, amount, hp } => {
                if let Some(s) = self.hud.shown.get_mut(&uid) {
                    s.0 = hp;
                }
                if amount > 0 {
                    self.hud.pop(uid, format!("+{amount}"), Color32::from_rgb(143, 240, 160), 20.0, now);
                }
                120.0
            }
            Event::Mp { uid, amount, mp } => {
                if let Some(s) = self.hud.shown.get_mut(&uid) {
                    s.1 = mp;
                }
                if amount > 0 && b.get(uid).side == Side::Player {
                    self.hud.pop(uid, format!("+{amount} MP"), Color32::from_rgb(156, 196, 255), 13.0, now);
                }
                0.0
            }
            Event::Miss { uid } => {
                self.hud.pop(uid, "Miss", Color32::from_rgb(200, 200, 200), 16.0, now);
                0.0
            }
            Event::Stat { uid, stat, delta, .. } => {
                let up = delta > 0;
                self.hud.pop(uid, format!("{} {}", if up { "▲" } else { "▼" }, stat.short()), if up { Color32::from_rgb(159, 240, 176) } else { Color32::from_rgb(255, 176, 166) }, 13.0, now);
                0.0
            }
            Event::Cast { uid, mv, locks, .. } => {
                let color = type_color(&b.dex, b.dex.mv(mv).ty, 1.0);
                self.stage.aura(uid, Some(color));
                self.hud.pop(uid, format!("{} locks!", locks.len()), hud::ACCENT, 16.0, now);
                350.0
            }
            Event::Charge { uid, mv } => {
                let color = type_color(&b.dex, b.dex.mv(mv).ty, 1.0);
                self.stage.aura(uid, Some(color));
                0.0
            }
            Event::LockBreak { uid, .. } => {
                self.hud.pop(uid, "BREAK!", Color32::from_rgb(255, 243, 107), 22.0, now);
                self.stage.flash(uid, now, 6.0);
                160.0
            }
            Event::CastEnd { uid, .. } => {
                self.stage.aura(uid, None);
                0.0
            }
            Event::MoveLevel { uid, mv, level } => {
                let m = b.dex.mv(mv);
                let perk = rules::moves::describe_levels(b.dex.profile(mv), &b.config)[level as usize - 1].clone();
                self.level_ups.push(format!("{}'s {} reached Lv {level}: {perk}", b.mon(uid).name, m.name));
                self.hud.banner(format!("{} reached Lv {level}! {perk}", m.name), hud::ACCENT, now + 2200.0);
                0.0
            }
            Event::Weather { kind } => {
                self.stage.weather = kind;
                0.0
            }
            Event::Catch { uid, shakes, caught } => {
                for i in 0..shakes.min(3) {
                    self.hud.pop(uid, "•".repeat(i as usize + 1), Color32::WHITE, 18.0, now + i as f64 * 420.0);
                }
                if !caught {
                    self.stage.flash(uid, now, 4.0);
                }
                420.0 * shakes.min(3) as f64
            }
            Event::Outcome(o) => {
                let (text, color) = match o {
                    Outcome::Win => ("Victory!", hud::GOOD),
                    Outcome::Lose => ("Defeat...", hud::LOW),
                    Outcome::Ran => ("Got away!", hud::TEXT),
                };
                self.hud.banner(text, color, now + 1e9);
                900.0
            }
            _ => 0.0,
        }
    }
}

enum Phase {
    /// gathers, calls, balls, items: an animation, then done
    Wait { until: f64, end_burst: Option<(Vec3, [f32; 4])>, flash: Option<Uid> },
    /// beams: hold A first, the attack starts on release
    Charge(ChargeRead),
    Anim { impact: f64, read: Option<TapRead>, cue: Option<Uid>, block: bool, bursts: bool },
    Rhythm(RhythmRead),
}

/// An action being animated, and the timing input read during it.
pub struct Perform {
    pub plan: Plan,
    phase: Phase,
    pre: Option<Beat>,
    pub simulated: bool,
    targets: Vec<Uid>,
    color: [f32; 4],
}

impl Perform {
    fn start(plan: Plan, now: f64, stage: &mut Stage, hud: &mut Hud, battle: &Battle, auto: bool) -> Perform {
        let actor = plan.actor;
        let targets: Vec<Uid> = plan.targets.iter().copied().filter(|t| stage.vis.contains_key(t)).collect();
        let mut focus = vec![actor];
        focus.extend(&targets);
        stage.focus_on(&focus);
        let color = type_color(&battle.dex, plan.ty.unwrap_or(0), 1.0);
        let simulated = auto && plan.input != InputKind::None;
        let mut p = Perform { plan, phase: Phase::Wait { until: now, end_burst: None, flash: None }, pre: None, simulated, targets, color };
        let input = if simulated { InputKind::None } else { p.plan.input };
        match p.plan.anim {
            Anim::Gather => {
                stage.aura(actor, Some(color));
                stage.gather(actor, color, now, 700.0);
                p.phase = Phase::Wait { until: now + 700.0, end_burst: Some((stage.center_of(actor), color)), flash: None };
            }
            Anim::Call => {
                stage.hop(actor, now);
                p.phase = Phase::Wait { until: now + 400.0, end_burst: None, flash: None };
            }
            Anim::Ball => {
                if let Some(&t) = p.targets.first() {
                    stage.orb(stage.player_spot(), stage.center_of(t), [1.0, 0.31, 0.31, 1.0], now, 550.0, 0.5, 2.0);
                    p.phase = Phase::Wait { until: now + 550.0, end_burst: None, flash: Some(t) };
                }
            }
            Anim::Item => {
                let t = p.targets.first().copied().unwrap_or(actor);
                stage.burst(stage.center_of(t), [0.6, 1.0, 0.63, 1.0], 12, 0.9, now);
                p.phase = Phase::Wait { until: now + 300.0, end_burst: None, flash: None };
            }
            Anim::None => {}
            _ if input == InputKind::Rhythm => {
                let gap = battle.config.timing.rhythm_gap;
                let beats: Vec<f64> = (0..p.plan.beats).map(|i| now + 650.0 + i as f64 * gap).collect();
                let dest = p.targets.first().and_then(|t| stage.vis.get(t)).map(|v| v.home).unwrap_or(Vec3::ZERO);
                stage.stay_near(actor, dest, beats.last().copied().unwrap_or(now) + 200.0, now);
                hud.prompt = Some((format!("Press A on every hit (up to {})", p.plan.beats), hud::ACCENT));
                let assist = battle.config.timing.assist;
                for (i, &b) in beats.iter().enumerate() {
                    if let Some(&t) = p.targets.get(i % p.targets.len().max(1)) {
                        hud.cues.push(Cue { uid: t, beat: b, window: battle.config.timing.tap, block: false, assist });
                    }
                }
                p.phase = Phase::Rhythm(RhythmRead::new(beats, battle.config.timing.tap));
            }
            _ if input == InputKind::Charge => {
                hud.prompt = Some(("Hold A, release when the ring fills".into(), Color32::from_rgb(181, 255, 154)));
                p.phase = Phase::Charge(ChargeRead::new(now, battle.config.timing.charge_fill, battle.config.timing.charge_window));
            }
            _ => p.start_anim(now, stage, hud, battle, input),
        }
        p
    }

    fn start_anim(&mut self, now: f64, stage: &mut Stage, hud: &mut Hud, battle: &Battle, input: InputKind) {
        let actor = self.plan.actor;
        let t = &battle.config.timing;
        let impact_in = match self.plan.anim {
            Anim::Lunge => {
                let mid = self.targets.get(self.targets.len() / 2).copied();
                let dest = mid.and_then(|u| stage.vis.get(&u)).map(|v| v.home).unwrap_or_else(|| stage.vis.get(&actor).map(|v| v.home).unwrap_or(Vec3::ZERO));
                stage.lunge(actor, dest, 620.0, now);
                620.0
            }
            Anim::Projectile | Anim::Hex => {
                let (delay, total) = if self.plan.anim == Anim::Projectile { (120.0, 520.0) } else { (150.0, 650.0) };
                let from = stage.center_of(actor);
                for &u in &self.targets {
                    let to = stage.center_of(u);
                    stage.orb(from, to, self.color, now + delay, total - delay, 0.8, 0.8);
                }
                total
            }
            Anim::Wave => {
                stage.wave(actor, self.color, now, 650.0);
                650.0
            }
            _ => {
                stage.pulse(actor, now, 650.0);
                650.0
            }
        };
        let impact = now + impact_in;
        let (read, cue, block) = match input {
            InputKind::Tap | InputKind::Cast => {
                let window = if input == InputKind::Tap { t.tap } else { t.cast };
                let cue = if input == InputKind::Cast && self.plan.anim == Anim::SelfAnim { actor } else { self.targets.first().copied().unwrap_or(actor) };
                hud.prompt = Some((if input == InputKind::Tap { "Press A at impact" } else { "Press A at the flash" }.into(), hud::ACCENT));
                hud.cues.push(Cue { uid: cue, beat: impact, window, block: false, assist: t.assist });
                (Some(TapRead::new(impact, window)), Some(cue), false)
            }
            InputKind::Block => {
                let mine: Vec<Uid> = self.targets.iter().copied().filter(|&u| battle.get(u).side == Side::Player).collect();
                hud.prompt = Some(("Block! Press A at impact".into(), hud::BLOCK));
                for &u in &mine {
                    hud.cues.push(Cue { uid: u, beat: impact, window: t.block, block: true, assist: t.assist });
                }
                (Some(TapRead::new(impact, t.block)), mine.first().copied(), true)
            }
            _ => (None, None, false),
        };
        self.phase = Phase::Anim { impact, read, cue, block, bursts: matches!(self.plan.anim, Anim::Projectile | Anim::Hex | Anim::Wave) };
    }

    /// The timing result, once the animation reaches its impact.
    fn update(&mut self, input: &Input, now: f64, stage: &mut Stage, hud: &mut Hud, battle: &Battle) -> Option<Timing> {
        match &mut self.phase {
            Phase::Wait { until, end_burst, flash } => {
                if now < *until {
                    return None;
                }
                if let Some((at, color)) = end_burst.take() {
                    stage.burst(at, color, 8, 0.6, now);
                }
                if let Some(u) = flash.take() {
                    stage.flash(u, now, 3.0);
                }
                Some(Timing::default())
            }
            Phase::Charge(read) => {
                hud.charge = read.fraction(now).map(|f| (self.plan.actor, f, battle.config.timing.charge_window / battle.config.timing.charge_fill)).or(Some((self.plan.actor, 0.0, 0.19)));
                let r = read.update(input, now)?;
                hud.charge = None;
                hud.feedback(self.plan.actor, r.success, r.delta, r.overheld, false, now);
                self.pre = Some(r);
                let input_kind = InputKind::None;
                self.start_anim(now, stage, hud, battle, input_kind);
                None
            }
            Phase::Anim { impact, read, cue, block, bursts } => {
                let result = match read {
                    Some(r) => {
                        let b = r.update(input, now)?;
                        if let Some(u) = cue.take() {
                            hud.feedback(u, b.success, b.delta, b.overheld, *block, now);
                        }
                        Some(b)
                    },
                    None => None,
                };
                if now < *impact {
                    if let Some(r) = read {
                        // keep the result: the read is done, wait for the impact
                        r.result = result;
                    }
                    return None;
                }
                for &u in &self.targets {
                    stage.flash(u, now, 4.0);
                    if *bursts {
                        stage.burst(stage.center_of(u), self.color, 6, 0.9, now);
                    }
                }
                hud.prompt = None;
                let success = match (self.pre, result) {
                    (Some(pre), _) => pre.success,
                    (None, Some(b)) => b.success,
                    _ => false,
                };
                Some(Timing { success, hits: None })
            }
            Phase::Rhythm(read) => {
                for (i, r, performed) in read.update(input, now, self.plan.guaranteed) {
                    let u = self.targets.get(i % self.targets.len().max(1)).copied();
                    if let Some(u) = u {
                        if performed {
                            stage.flash(u, now, 4.0);
                            stage.burst(stage.center_of(u), self.color, 5, 0.5, now);
                        }
                        hud.feedback(u, r.success, r.delta, false, false, now);
                    }
                    if !read.going && i + 1 >= self.plan.guaranteed as usize {
                        hud.cues.clear();
                    }
                }
                if !read.done() {
                    return None;
                }
                hud.cues.clear();
                hud.prompt = None;
                let hits = self.plan.beats.min(self.plan.guaranteed.max(read.streak + 1));
                Some(Timing { success: read.streak > 0, hits: Some(hits) })
            }
        }
    }

    /// For the AI-plays-your-side option: what a 75% player would have done.
    fn sim_plan(&self) -> Plan {
        let mut p = self.plan.clone();
        if p.input == InputKind::Block {
            p.input = InputKind::Tap;
        }
        p
    }

    /// For the browser test hook: the timing read in progress, in page-clock ms.
    fn pending_hook(&self) -> String {
        if self.simulated {
            return String::new();
        }
        match &self.phase {
            Phase::Charge(r) if r.start.is_none() => format!("charge:{}:{}:{}", r.armed, r.fill, r.window),
            Phase::Anim { read: Some(r), .. } if r.result.is_none() => format!("tap:{}:{}", r.beat, r.window),
            Phase::Rhythm(r) if !r.done() => format!("rhythm:{}:{}", r.beats.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","), r.window),
            _ => String::new(),
        }
    }
}
