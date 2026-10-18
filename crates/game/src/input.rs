//! Buttons, GBA-style (A, B, the d-pad, L, R) from the keyboard and mouse or a controller
//! (whichever is listened to: `pads`), and the timing reads the battle asks for: one beat, a
//! hold-and-release, a rhythm of beats. The keys and the controller's buttons are the
//! player's (the Options' Controls); as they come:
//!   A: Z, Space, Enter, left click; South (A, Cross)    B: X, Escape, Backspace; East (B, Circle)
//!   d-pad: arrows / WASD; the d-pad or the left stick   L / R: Q / E (Page Up / Page Down); LB / RB

use std::collections::BTreeMap;
use std::sync::RwLock;

use egui::Key;

use crate::pads::{Device, PadButton};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    A,
    B,
    Up,
    Down,
    Left,
    Right,
    L,
    R,
}

impl Button {
    pub const ALL: [Button; 8] = [Button::A, Button::B, Button::Up, Button::Down, Button::Left, Button::Right, Button::L, Button::R];

    pub fn name(self) -> &'static str {
        match self {
            Button::A => "A",
            Button::B => "B",
            Button::Up => "Up",
            Button::Down => "Down",
            Button::Left => "Left",
            Button::Right => "Right",
            Button::L => "L",
            Button::R => "R",
        }
    }

    /// What it does, in a few words.
    pub fn about(self) -> &'static str {
        match self {
            Button::A => "Confirm, talk, hit and block",
            Button::B => "Back, cancel",
            Button::Up => "Up",
            Button::Down => "Down",
            Button::Left => "Left",
            Button::Right => "Right",
            Button::L => "Previous (L)",
            Button::R => "Next (R)",
        }
    }

    pub(crate) fn index(self) -> usize {
        Button::ALL.iter().position(|&b| b == self).unwrap_or(0)
    }
}

/// The keys and the controller's buttons each button answers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    keys: [Vec<Key>; 8],
    pads: [Vec<PadButton>; 8],
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings {
            keys: [
                vec![Key::Z, Key::Space, Key::Enter],
                vec![Key::X, Key::Escape, Key::Backspace],
                vec![Key::ArrowUp, Key::W],
                vec![Key::ArrowDown, Key::S],
                vec![Key::ArrowLeft, Key::A],
                vec![Key::ArrowRight, Key::D],
                vec![Key::Q, Key::PageUp],
                vec![Key::E, Key::PageDown],
            ],
            pads: [
                vec![PadButton::South],
                vec![PadButton::East],
                vec![PadButton::DPadUp],
                vec![PadButton::DPadDown],
                vec![PadButton::DPadLeft],
                vec![PadButton::DPadRight],
                vec![PadButton::LeftBumper],
                vec![PadButton::RightBumper],
            ],
        }
    }
}

impl Bindings {
    pub fn keys(&self, b: Button) -> &[Key] {
        &self.keys[b.index()]
    }

    pub fn button_of(&self, key: Key) -> Option<Button> {
        Button::ALL.into_iter().find(|&b| self.keys(b).contains(&key))
    }

    /// `key` for `button`, in place of `old` (else added): taken from any other button that
    /// had it.
    pub fn assign(&mut self, button: Button, old: Option<Key>, key: Key) {
        for list in self.keys.iter_mut() {
            list.retain(|&k| k != key);
        }
        let list = &mut self.keys[button.index()];
        match old.and_then(|o| list.iter().position(|&k| k == o)) {
            Some(i) => list.insert(i, key),
            None => list.push(key),
        }
        if let Some(o) = old.filter(|&o| o != key) {
            list.retain(|&k| k != o);
        }
    }

    /// A key taken off a button (its last one stays: a button with no key can't be pressed).
    pub fn remove(&mut self, button: Button, key: Key) {
        let list = &mut self.keys[button.index()];
        if list.len() > 1 {
            list.retain(|&k| k != key);
        }
    }

    pub fn pads(&self, b: Button) -> &[PadButton] {
        &self.pads[b.index()]
    }

    pub fn button_of_pad(&self, p: PadButton) -> Option<Button> {
        Button::ALL.into_iter().find(|&b| self.pads(b).contains(&p))
    }

    /// The controller's button `p` for `button`, in place of `old` (else added): taken from
    /// any other button that had it.
    pub fn assign_pad(&mut self, button: Button, old: Option<PadButton>, p: PadButton) {
        for list in self.pads.iter_mut() {
            list.retain(|&k| k != p);
        }
        let list = &mut self.pads[button.index()];
        match old.and_then(|o| list.iter().position(|&k| k == o)) {
            Some(i) => list.insert(i, p),
            None => list.push(p),
        }
        if let Some(o) = old.filter(|&o| o != p) {
            list.retain(|&k| k != o);
        }
    }

    /// A controller's button taken off a button (its last one stays).
    pub fn remove_pad(&mut self, button: Button, p: PadButton) {
        let list = &mut self.pads[button.index()];
        if list.len() > 1 {
            list.retain(|&k| k != p);
        }
    }

    /// As they're kept: each button's keys by name ("A": ["Z", "Space", "Enter"]).
    pub fn names(&self) -> BTreeMap<String, Vec<String>> {
        Button::ALL.iter().map(|&b| (b.name().to_string(), self.keys(b).iter().map(|k| k.name().to_string()).collect())).collect()
    }

    /// As they're kept: each button's controller buttons by name ("A": ["South"]).
    pub fn pad_names(&self) -> BTreeMap<String, Vec<String>> {
        Button::ALL.iter().map(|&b| (b.name().to_string(), self.pads(b).iter().map(|p| p.name().to_string()).collect())).collect()
    }

    /// From what was kept; a button not there (or with nothing that can be read) as it comes.
    pub fn from_names(keys: &BTreeMap<String, Vec<String>>, pads: &BTreeMap<String, Vec<String>>) -> Bindings {
        let mut out = Bindings::default();
        for b in Button::ALL {
            if let Some(list) = keys.get(b.name()) {
                let keys: Vec<Key> = list.iter().filter_map(|n| Key::from_name(n)).collect();
                if !keys.is_empty() {
                    out.keys[b.index()] = keys;
                }
            }
            if let Some(list) = pads.get(b.name()) {
                let pads: Vec<PadButton> = list.iter().filter_map(|n| PadButton::from_name(n)).collect();
                if !pads.is_empty() {
                    out.pads[b.index()] = pads;
                }
            }
        }
        out
    }
}

/// The player's keys, for everything that reads buttons (the battles, the overworld's walk
/// and dialogs).
static BINDINGS: RwLock<Option<Bindings>> = RwLock::new(None);

pub fn bindings() -> Bindings {
    BINDINGS.read().ok().and_then(|b| b.clone()).unwrap_or_default()
}

pub fn set_bindings(b: Bindings) {
    if let Ok(mut held) = BINDINGS.write() {
        *held = Some(b);
    }
}

/// The button held this frame (or tapped: a tap can start and end between two frames), on
/// the device listened to: one of its keys, or on the controller.
pub fn held(ctx: &egui::Context, b: Button) -> bool {
    if crate::pads::on_pad() {
        return crate::pads::held(b);
    }
    let keys = bindings();
    ctx.input(|i| keys.keys(b).iter().any(|&k| i.key_down(k) || i.key_pressed(k)))
}

/// The button pressed this frame, on the device listened to.
pub fn pressed_now(ctx: &egui::Context, b: Button) -> bool {
    if crate::pads::on_pad() {
        return crate::pads::pressed(b);
    }
    let keys = bindings();
    ctx.input(|i| keys.keys(b).iter().any(|&k| i.key_pressed(k)))
}

#[derive(Clone, Copy, Debug)]
pub struct Press {
    pub button: Button,
    pub down: bool,
    /// when the frame that saw it began (ms)
    pub time: f64,
}

#[derive(Default)]
pub struct Input {
    pub presses: Vec<Press>,
    pub a_held: bool,
}

impl Input {
    /// This frame's presses, on the device listened to (`pads::Pads::update` has decided it).
    /// `pointer_a`: clicks on the 3D view count as A.
    pub fn read(&mut self, ctx: &egui::Context, now: f64, pointer_a: bool) {
        self.presses.clear();
        if let Device::Pad(_) = crate::pads::device() {
            for (button, down) in crate::pads::presses() {
                self.presses.push(Press { button, down, time: now });
            }
            for p in &self.presses {
                if p.button == Button::A {
                    self.a_held = p.down;
                }
            }
            return;
        }
        let keys = bindings();
        let map = |k: Key| keys.button_of(k);
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Key { key, pressed, repeat, .. } if !repeat => {
                        if let Some(button) = map(*key) {
                            self.presses.push(Press { button, down: *pressed, time: now });
                        }
                    }
                    egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed, .. } if pointer_a => {
                        self.presses.push(Press { button: Button::A, down: *pressed, time: now });
                    }
                    _ => {}
                }
            }
        });
        for p in &self.presses {
            if p.button == Button::A {
                self.a_held = p.down;
            }
        }
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.presses.iter().any(|p| p.button == b && p.down)
    }

    pub fn downs(&self, b: Button) -> impl Iterator<Item = f64> + '_ {
        self.presses.iter().filter(move |p| p.button == b && p.down).map(|p| p.time)
    }

    pub fn ups(&self, b: Button) -> impl Iterator<Item = f64> + '_ {
        self.presses.iter().filter(move |p| p.button == b && !p.down).map(|p| p.time)
    }
}

/// How a beat went: `delta` ms from the beat (negative = early), None = no press at all.
#[derive(Clone, Copy, Debug)]
pub struct Beat {
    pub success: bool,
    pub delta: Option<f64>,
    pub overheld: bool,
}

/// One beat at `beat` (ms). A press before the window counts: one try per beat.
pub struct TapRead {
    pub beat: f64,
    pub window: f64,
    pub result: Option<Beat>,
}

impl TapRead {
    pub fn new(beat: f64, window: f64) -> TapRead {
        TapRead { beat, window, result: None }
    }

    pub fn update(&mut self, input: &Input, now: f64) -> Option<Beat> {
        if self.result.is_none() {
            for t in input.downs(Button::A) {
                if t < self.beat - 600.0 {
                    continue; // a press long before the move started is the menu's
                }
                let delta = t - self.beat;
                self.result = Some(Beat { success: delta.abs() <= self.window, delta: Some(delta), overheld: false });
                break;
            }
        }
        if self.result.is_none() && now > self.beat + self.window {
            self.result = Some(Beat { success: false, delta: None, overheld: false });
        }
        self.result
    }
}

/// Hold A, release when the ring is full: success when released within
/// [fill, fill + window] ms after the press.
pub struct ChargeRead {
    pub fill: f64,
    pub window: f64,
    pub armed: f64,
    pub start: Option<f64>,
    pub result: Option<Beat>,
}

impl ChargeRead {
    pub fn new(now: f64, fill: f64, window: f64) -> ChargeRead {
        ChargeRead { fill, window, armed: now, start: None, result: None }
    }

    /// How full the ring is (0..1.25), None before the press.
    pub fn fraction(&self, now: f64) -> Option<f64> {
        self.start.map(|s| ((now - s) / self.fill).min(1.25))
    }

    pub fn update(&mut self, input: &Input, now: f64) -> Option<Beat> {
        if self.result.is_some() {
            return self.result;
        }
        if self.start.is_none() {
            if let Some(t) = input.downs(Button::A).next() {
                self.start = Some(t);
            }
        }
        if let Some(s) = self.start {
            if let Some(t) = input.ups(Button::A).find(|&t| t >= s) {
                let held = t - s;
                self.result = Some(Beat { success: held >= self.fill && held <= self.fill + self.window, delta: Some(held - self.fill), overheld: false });
            } else if now - s > self.fill + self.window + 40.0 {
                self.result = Some(Beat { success: false, delta: Some(now - s - self.fill), overheld: true });
            }
        } else if now - self.armed > 1600.0 {
            self.result = Some(Beat { success: false, delta: None, overheld: false });
        }
        self.result
    }
}

/// Several beats; each press lands on the next one. The combo goes on while they're hit.
pub struct RhythmRead {
    pub beats: Vec<f64>,
    pub window: f64,
    pub i: usize,
    pub streak: u8,
    pub going: bool,
}

impl RhythmRead {
    pub fn new(beats: Vec<f64>, window: f64) -> RhythmRead {
        RhythmRead { beats, window, i: 0, streak: 0, going: true }
    }

    pub fn done(&self) -> bool {
        self.i >= self.beats.len()
    }

    /// The beats settled this frame: (index, result, whether that hit is performed).
    pub fn update(&mut self, input: &Input, now: f64, guaranteed: u8) -> Vec<(usize, Beat, bool)> {
        let mut out = vec![];
        let mut downs: Vec<f64> = input.downs(Button::A).collect();
        loop {
            if self.done() {
                break;
            }
            let beat = self.beats[self.i];
            let early_limit = beat - (self.window * 2.5).max(250.0);
            let press = downs.iter().position(|&t| t >= early_limit);
            let result = if let Some(k) = press {
                let t = downs.remove(k);
                let delta = t - beat;
                Some(Beat { success: delta.abs() <= self.window, delta: Some(delta), overheld: false })
            } else if now > beat + self.window {
                Some(Beat { success: false, delta: None, overheld: false })
            } else {
                None
            };
            let Some(r) = result else { break };
            let performed = (self.i as u8) < guaranteed || self.going;
            if r.success && self.going {
                self.streak += 1;
            } else {
                self.going = false;
            }
            out.push((self.i, r, performed));
            self.i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_the_players() {
        let mut b = Bindings::default();
        assert_eq!(b.button_of(Key::Z), Some(Button::A));
        assert_eq!(b.button_of(Key::W), Some(Button::Up));
        // a key given to a button leaves the one that had it, in the place of the key it replaces
        b.assign(Button::A, Some(Key::Space), Key::X);
        assert_eq!(b.keys(Button::A), [Key::Z, Key::X, Key::Enter]);
        assert_eq!(b.keys(Button::B), [Key::Escape, Key::Backspace]);
        // one more; one taken off; the last one stays
        b.assign(Button::L, None, Key::Num1);
        assert_eq!(b.keys(Button::L), [Key::Q, Key::PageUp, Key::Num1]);
        b.remove(Button::R, Key::E);
        b.remove(Button::R, Key::PageDown);
        assert_eq!(b.keys(Button::R), [Key::PageDown], "a button keeps its last key");
        // kept by name, read back; what can't be read is as it came
        let mut names = b.names();
        assert_eq!(names["A"], ["Z", "X", "Enter"]);
        names.insert("Up".into(), vec!["NoSuchKey".into()]);
        let back = Bindings::from_names(&names, &b.pad_names());
        assert_eq!(back.keys(Button::A), b.keys(Button::A));
        assert_eq!(back.keys(Button::Up), Bindings::default().keys(Button::Up));
    }

    #[test]
    fn the_controllers_buttons_are_the_players_too() {
        let mut b = Bindings::default();
        assert_eq!(b.button_of_pad(PadButton::South), Some(Button::A));
        assert_eq!(b.button_of_pad(PadButton::DPadLeft), Some(Button::Left));
        assert_eq!(b.button_of_pad(PadButton::Start), None);
        // North for A in South's place; then East (B's) for A too, taken from B
        b.assign_pad(Button::A, Some(PadButton::South), PadButton::North);
        assert_eq!(b.pads(Button::A), [PadButton::North]);
        assert_eq!(b.button_of_pad(PadButton::South), None);
        b.assign_pad(Button::A, None, PadButton::East);
        assert_eq!(b.pads(Button::A), [PadButton::North, PadButton::East]);
        assert!(b.pads(Button::B).is_empty(), "taken from B");
        b.remove_pad(Button::A, PadButton::North);
        b.remove_pad(Button::A, PadButton::East);
        assert_eq!(b.pads(Button::A), [PadButton::East], "the last one stays");
        // kept by name with the keys, read back; nothing readable: as it came
        let mut pads = b.pad_names();
        assert_eq!(pads["A"], ["East"]);
        pads.insert("L".into(), vec!["Nothing".into()]);
        let back = Bindings::from_names(&b.names(), &pads);
        assert_eq!(back.pads(Button::A), [PadButton::East]);
        assert_eq!(back.pads(Button::L), [PadButton::LeftBumper]);
        assert_eq!(back.keys(Button::A), b.keys(Button::A));
    }
}
