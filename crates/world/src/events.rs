//! Running an event's script: the machine walks the actions, keeps the flags, and asks its
//! host (the map editor, later the game) to do what takes time or a screen: showing a
//! line, offering a choice, moving someone, turning them, waiting, a sound, the camera, a
//! warp, a battle. The host says when a wait is over; the machine goes on. Flags live in
//! `Flags` for as long as the host keeps them (a save, one day).

use std::collections::BTreeMap;

use content::script::{Action, Target};

/// What a script is waiting for.
#[derive(Clone, Debug, PartialEq)]
pub enum Wait {
    Nothing,
    /// a line shown: A goes on
    Line,
    /// a choice offered: the pick's label comes back through `choose`
    Choice,
    /// someone walking to the end of a `move`
    Moving(String),
    /// seconds
    Timer(f64),
    /// the view gliding somewhere
    Camera,
    /// a battle on another screen
    Battle,
    /// another map: this script is over
    Warped,
}

/// Where a script's doing happens.
pub trait Host {
    /// A line of dialog (`who` empty: nobody in particular).
    fn say(&mut self, who: &str, text: &str);
    /// A choice to offer (what's offered); the pick comes back as its index.
    fn ask(&mut self, options: &[String]);
    /// Someone walks these cells, one after the other.
    fn walk(&mut self, who: &str, cells: &[[i32; 2]]);
    /// Someone turns: to n, s, e, w, "player" or an actor.
    fn face(&mut self, who: &str, to: &str);
    fn show(&mut self, who: &str, shown: bool);
    fn sound(&mut self, file: &str, volume: f64);
    fn camera(&mut self, to: &Target);
    fn warp(&mut self, map: &str, at: [i32; 2]);
    fn battle(&mut self, encounter: &str);
}

pub type Flags = BTreeMap<String, i64>;

/// A script being run.
#[derive(Clone, Debug)]
pub struct Machine {
    actions: Vec<Action>,
    pc: usize,
    pub wait: Wait,
    /// the choice's labels, in the order offered
    choices: Vec<String>,
    /// a flag to set when the script ends (an event run once)
    pub done_flag: Option<String>,
}

impl Machine {
    pub fn new(actions: Vec<Action>, done_flag: Option<String>) -> Machine {
        Machine { actions, pc: 0, wait: Wait::Nothing, choices: vec![], done_flag }
    }

    pub fn finished(&self) -> bool {
        self.pc >= self.actions.len() && self.wait == Wait::Nothing || self.wait == Wait::Warped
    }

    /// The host says what it was waiting for is over.
    pub fn resume(&mut self) {
        if !matches!(self.wait, Wait::Choice | Wait::Warped) {
            self.wait = Wait::Nothing;
        }
    }

    /// The pick of a choice, by index: the script goes on at its label.
    pub fn choose(&mut self, pick: usize) {
        if self.wait != Wait::Choice {
            return;
        }
        if let Some(label) = self.choices.get(pick).cloned() {
            self.jump(&label);
        }
        self.wait = Wait::Nothing;
    }

    fn jump(&mut self, label: &str) {
        match self.actions.iter().position(|a| matches!(a, Action::Label(l) if l == label)) {
            Some(i) => self.pc = i + 1,
            None => self.pc = self.actions.len(),
        }
    }

    /// Runs on until something takes time (or the end). `now` is in seconds.
    pub fn run(&mut self, host: &mut dyn Host, flags: &mut Flags, now: f64) {
        if let Wait::Timer(until) = self.wait {
            if now >= until {
                self.wait = Wait::Nothing;
            }
        }
        while self.wait == Wait::Nothing && self.pc < self.actions.len() {
            let action = self.actions[self.pc].clone();
            self.pc += 1;
            match action {
                Action::Say { who, text } => {
                    host.say(&who, &text);
                    self.wait = Wait::Line;
                }
                Action::Ask(options) => {
                    self.choices = options.iter().map(|(_, l)| l.clone()).collect();
                    host.ask(&options.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>());
                    self.wait = Wait::Choice;
                }
                Action::Label(_) => {}
                Action::Goto(l) => self.jump(&l),
                Action::If { flag, label, set } => {
                    if (flags.get(&flag).copied().unwrap_or(0) != 0) == set {
                        self.jump(&label);
                    }
                }
                Action::Set { flag, value } => {
                    flags.insert(flag, value);
                }
                Action::Add { flag, value } => {
                    *flags.entry(flag).or_insert(0) += value;
                }
                Action::Clear(flag) => {
                    flags.remove(&flag);
                }
                Action::Move { who, cells } => {
                    host.walk(&who, &cells);
                    self.wait = Wait::Moving(who);
                }
                Action::Face { who, to } => host.face(&who, &to),
                Action::Wait(s) => self.wait = Wait::Timer(now + s),
                Action::Show(who) => host.show(&who, true),
                Action::Hide(who) => host.show(&who, false),
                Action::Sound { file, volume } => host.sound(&file, volume),
                Action::Camera(t) => {
                    host.camera(&t);
                    self.wait = Wait::Camera;
                }
                Action::Warp { map, at } => {
                    host.warp(&map, at);
                    self.wait = Wait::Warped;
                }
                Action::Battle(e) => {
                    host.battle(&e);
                    self.wait = Wait::Battle;
                }
                Action::End => self.pc = self.actions.len(),
            }
        }
        if self.finished() {
            if let Some(f) = self.done_flag.take() {
                flags.insert(f, 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::script::parse;

    #[derive(Default)]
    struct Log(Vec<String>);

    impl Host for Log {
        fn say(&mut self, who: &str, text: &str) {
            self.0.push(format!("say {who}: {text}"));
        }
        fn ask(&mut self, options: &[String]) {
            self.0.push(format!("ask {}", options.join("|")));
        }
        fn walk(&mut self, who: &str, cells: &[[i32; 2]]) {
            self.0.push(format!("walk {who} {cells:?}"));
        }
        fn face(&mut self, who: &str, to: &str) {
            self.0.push(format!("face {who} {to}"));
        }
        fn show(&mut self, who: &str, shown: bool) {
            self.0.push(format!("show {who} {shown}"));
        }
        fn sound(&mut self, file: &str, _volume: f64) {
            self.0.push(format!("sound {file}"));
        }
        fn camera(&mut self, to: &Target) {
            self.0.push(format!("camera {to:?}"));
        }
        fn warp(&mut self, map: &str, at: [i32; 2]) {
            self.0.push(format!("warp {map} {at:?}"));
        }
        fn battle(&mut self, encounter: &str) {
            self.0.push(format!("battle {encounter}"));
        }
    }

    fn script(s: &str) -> Vec<Action> {
        parse(&s.lines().map(String::from).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_talk_with_a_choice_branches_and_remembers() {
        let s = script("say Bo: Hello.\nask Hi -> hi | Bye -> bye\nlabel hi\nset met_bo\nmove bo 1,1 1,2\nwait 0.5\ngoto end\nlabel bye\nsay Bo: Fine.\nlabel end\nface bo player");
        let mut host = Log::default();
        let mut flags = Flags::new();
        let mut m = Machine::new(s.clone(), Some("done.talk".into()));
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(m.wait, Wait::Line);
        m.resume();
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(m.wait, Wait::Choice);
        m.choose(0);
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(flags.get("met_bo"), Some(&1));
        assert_eq!(m.wait, Wait::Moving("bo".into()));
        m.resume();
        m.run(&mut host, &mut flags, 1.0);
        assert_eq!(m.wait, Wait::Timer(1.5));
        m.run(&mut host, &mut flags, 1.2);
        assert_eq!(m.wait, Wait::Timer(1.5), "still waiting");
        m.run(&mut host, &mut flags, 1.6);
        assert!(m.finished());
        assert_eq!(flags.get("done.talk"), Some(&1), "run once: remembered");
        assert_eq!(host.0, vec!["say Bo: Hello.", "ask Hi|Bye", "walk bo [[1, 1], [1, 2]]", "face bo player"]);
        // the other branch
        let mut host = Log::default();
        let mut m = Machine::new(s, None);
        m.run(&mut host, &mut flags, 0.0);
        m.resume();
        m.run(&mut host, &mut flags, 0.0);
        m.choose(1);
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(m.wait, Wait::Line);
        assert_eq!(host.0.last().unwrap(), "say Bo: Fine.");
    }

    #[test]
    fn flags_steer_and_a_warp_ends_the_script() {
        let s = script("unless key goto locked\nsay The door opens.\nwarp cellar 3,3\nsay never\nlabel locked\nsay It's locked.\nbattle guard");
        let mut host = Log::default();
        let mut flags = Flags::new();
        let mut m = Machine::new(s.clone(), None);
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(host.0, vec!["say : It's locked."]);
        m.resume();
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(m.wait, Wait::Battle);
        flags.insert("key".into(), 1);
        let mut host = Log::default();
        let mut m = Machine::new(s, None);
        m.run(&mut host, &mut flags, 0.0);
        m.resume();
        m.run(&mut host, &mut flags, 0.0);
        assert_eq!(m.wait, Wait::Warped);
        assert!(m.finished());
        assert_eq!(host.0, vec!["say : The door opens.", "warp cellar [3, 3]"]);
    }
}
