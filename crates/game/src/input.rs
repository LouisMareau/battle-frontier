//! Buttons, GBA-style (A, B, the d-pad, L, R) from the keyboard and mouse, and the
//! timing reads the battle asks for: one beat, a hold-and-release, a rhythm of beats.
//!   A: Z, Space, Enter, left click    B: X, Escape, Backspace
//!   d-pad: arrows / WASD              L / R: Q / E (Page Up / Page Down)

use egui::Key;

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

fn map(key: Key) -> Option<Button> {
    Some(match key {
        Key::Z | Key::Space | Key::Enter => Button::A,
        Key::X | Key::Escape | Key::Backspace => Button::B,
        Key::ArrowUp | Key::W => Button::Up,
        Key::ArrowDown | Key::S => Button::Down,
        Key::ArrowLeft | Key::A => Button::Left,
        Key::ArrowRight | Key::D => Button::Right,
        Key::Q | Key::PageUp => Button::L,
        Key::E | Key::PageDown => Button::R,
        _ => return None,
    })
}

impl Input {
    /// This frame's presses. `pointer_a`: clicks on the 3D view count as A.
    pub fn read(&mut self, ctx: &egui::Context, now: f64, pointer_a: bool) {
        self.presses.clear();
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
