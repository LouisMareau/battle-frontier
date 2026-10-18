//! Controllers, and the device the game listens to.
//!
//! gilrs reads the controllers: the system's own way on the desktop (Windows.Gaming.Input on
//! Windows, evdev on Linux, IOKit on macOS), the Gamepad API in the browser (where a
//! controller shows once one of its buttons is pressed). The game listens to one device at a
//! time, the keyboard and mouse or one controller: a controller connected (or there at the
//! start) takes over, and whichever device is used while the game's window is active takes
//! over from the one before. What a controller's buttons stand for is the player's
//! (`input::Bindings`, the Options' Controls); its left stick moves like the d-pad.

use std::collections::BTreeMap;
use std::sync::RwLock;

use egui::{Color32, FontId, Pos2, Rect, Vec2};

use crate::input::{Bindings, Button};

/// A controller's buttons, by where they are: South is the lower face button whatever is
/// printed on it (A on an Xbox controller, Cross on a PlayStation one, B on one with A on
/// the right).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    pub const ALL: [PadButton; 16] = [
        PadButton::South,
        PadButton::East,
        PadButton::West,
        PadButton::North,
        PadButton::LeftBumper,
        PadButton::RightBumper,
        PadButton::LeftTrigger,
        PadButton::RightTrigger,
        PadButton::Select,
        PadButton::Start,
        PadButton::LeftStick,
        PadButton::RightStick,
        PadButton::DPadUp,
        PadButton::DPadDown,
        PadButton::DPadLeft,
        PadButton::DPadRight,
    ];

    /// Its name as the options keep it.
    pub fn name(self) -> &'static str {
        match self {
            PadButton::South => "South",
            PadButton::East => "East",
            PadButton::West => "West",
            PadButton::North => "North",
            PadButton::LeftBumper => "LeftBumper",
            PadButton::RightBumper => "RightBumper",
            PadButton::LeftTrigger => "LeftTrigger",
            PadButton::RightTrigger => "RightTrigger",
            PadButton::Select => "Select",
            PadButton::Start => "Start",
            PadButton::LeftStick => "LeftStick",
            PadButton::RightStick => "RightStick",
            PadButton::DPadUp => "DPadUp",
            PadButton::DPadDown => "DPadDown",
            PadButton::DPadLeft => "DPadLeft",
            PadButton::DPadRight => "DPadRight",
        }
    }

    pub fn from_name(name: &str) -> Option<PadButton> {
        PadButton::ALL.into_iter().find(|b| b.name() == name)
    }

    /// What's printed on it, on a controller of that kind.
    pub fn label(self, family: Family) -> &'static str {
        use Family::*;
        use PadButton::*;
        match (self, family) {
            (South, Xbox) => "A",
            (East, Xbox) => "B",
            (West, Xbox) => "X",
            (North, Xbox) => "Y",
            (South, PlayStation) => "Cross",
            (East, PlayStation) => "Circle",
            (West, PlayStation) => "Square",
            (North, PlayStation) => "Triangle",
            (South, RightA) => "B",
            (East, RightA) => "A",
            (West, RightA) => "Y",
            (North, RightA) => "X",
            (LeftBumper, Xbox) => "LB",
            (RightBumper, Xbox) => "RB",
            (LeftTrigger, Xbox) => "LT",
            (RightTrigger, Xbox) => "RT",
            (LeftBumper, PlayStation) => "L1",
            (RightBumper, PlayStation) => "R1",
            (LeftTrigger, PlayStation) => "L2",
            (RightTrigger, PlayStation) => "R2",
            (LeftBumper, RightA) => "L",
            (RightBumper, RightA) => "R",
            (LeftTrigger, RightA) => "ZL",
            (RightTrigger, RightA) => "ZR",
            (Select, Xbox) => "View",
            (Start, Xbox) => "Menu",
            (Select, PlayStation) => "Share",
            (Start, PlayStation) => "Options",
            (Select, RightA) => "Minus",
            (Start, RightA) => "Plus",
            (LeftStick, PlayStation) => "L3",
            (RightStick, PlayStation) => "R3",
            (LeftStick, _) => "Left stick",
            (RightStick, _) => "Right stick",
            (DPadUp, _) => "D-pad up",
            (DPadDown, _) => "D-pad down",
            (DPadLeft, _) => "D-pad left",
            (DPadRight, _) => "D-pad right",
        }
    }

    fn from_gilrs(b: gilrs::Button) -> Option<PadButton> {
        use gilrs::Button as G;
        Some(match b {
            G::South => PadButton::South,
            G::East => PadButton::East,
            G::West => PadButton::West,
            G::North => PadButton::North,
            G::LeftTrigger => PadButton::LeftBumper,
            G::RightTrigger => PadButton::RightBumper,
            G::LeftTrigger2 => PadButton::LeftTrigger,
            G::RightTrigger2 => PadButton::RightTrigger,
            G::Select => PadButton::Select,
            G::Start => PadButton::Start,
            G::LeftThumb => PadButton::LeftStick,
            G::RightThumb => PadButton::RightStick,
            G::DPadUp => PadButton::DPadUp,
            G::DPadDown => PadButton::DPadDown,
            G::DPadLeft => PadButton::DPadLeft,
            G::DPadRight => PadButton::DPadRight,
            _ => return None,
        })
    }

    fn to_gilrs(self) -> gilrs::Button {
        use gilrs::Button as G;
        match self {
            PadButton::South => G::South,
            PadButton::East => G::East,
            PadButton::West => G::West,
            PadButton::North => G::North,
            PadButton::LeftBumper => G::LeftTrigger,
            PadButton::RightBumper => G::RightTrigger,
            PadButton::LeftTrigger => G::LeftTrigger2,
            PadButton::RightTrigger => G::RightTrigger2,
            PadButton::Select => G::Select,
            PadButton::Start => G::Start,
            PadButton::LeftStick => G::LeftThumb,
            PadButton::RightStick => G::RightThumb,
            PadButton::DPadUp => G::DPadUp,
            PadButton::DPadDown => G::DPadDown,
            PadButton::DPadLeft => G::DPadLeft,
            PadButton::DPadRight => G::DPadRight,
        }
    }
}

/// A controller's kind, for what its buttons have printed on them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Family {
    /// A at the bottom, B on the right, X on the left, Y at the top (and the many made like it)
    #[default]
    Xbox,
    /// Cross, Circle, Square, Triangle
    PlayStation,
    /// A on the right and B at the bottom, X at the top and Y on the left (the face labels
    /// swapped against an Xbox controller's)
    RightA,
}

impl Family {
    /// From its maker's USB number or, failing that, its name.
    pub fn of(name: &str, vendor: Option<u16>) -> Family {
        match vendor {
            Some(0x054c) => return Family::PlayStation,
            Some(0x057e) => return Family::RightA,
            Some(0x045e) => return Family::Xbox,
            _ => {}
        }
        let n = name.to_lowercase();
        if n.contains("xbox") || n.contains("xinput") {
            Family::Xbox
        } else if ["playstation", "dualsense", "dualshock", "ps3", "ps4", "ps5", "sony"].iter().any(|w| n.contains(w)) || n.starts_with("wireless controller") {
            // Sony's own controllers call themselves "Wireless Controller"
            Family::PlayStation
        } else {
            Family::Xbox
        }
    }
}

/// A controller's name as people say it: the browser's id without its layout and its maker's
/// numbers ("Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)", or
/// "045e-0b13-Xbox Wireless Controller" in Firefox).
pub fn plain_name(id: &str) -> String {
    let mut s = id.trim();
    let b = s.as_bytes();
    let hex = |t: &str| t.chars().all(|c| c.is_ascii_hexdigit());
    if b.len() > 10 && b[4] == b'-' && b[9] == b'-' && hex(&s[..4]) && hex(&s[5..9]) {
        s = &s[10..];
    }
    if let Some(i) = s.find(" (") {
        let rest = &s[i..];
        if ["Vendor", "STANDARD GAMEPAD", "XInput"].iter().any(|w| rest.contains(w)) {
            s = &s[..i];
        }
    }
    let s = s.trim();
    if s.is_empty() {
        "Controller".to_string()
    } else {
        s.to_string()
    }
}

/// The maker's USB number in a browser's id for a controller: Chrome's "… (STANDARD GAMEPAD
/// Vendor: 054c Product: 0ce6)", Firefox's "054c-0ce6-…".
fn vendor_in(id: &str) -> Option<u16> {
    let hex = |t: &str| if t.len() == 4 && t.chars().all(|c| c.is_ascii_hexdigit()) { u16::from_str_radix(t, 16).ok() } else { None };
    let b = id.as_bytes();
    if b.len() > 10 && b[4] == b'-' && b[9] == b'-' {
        if let (Some(v), Some(_)) = (hex(&id[..4]), hex(&id[5..9])) {
            return Some(v);
        }
    }
    let at = id.find("Vendor: ")? + "Vendor: ".len();
    hex(id.get(at..at + 4)?)
}

/// In the browser, the Gamepad API's slot each controller came from. gilrs keeps a slot's
/// first controller (its name and maker) for any controller connected there after it, so one
/// connected again goes by what its slot holds now. On the desktop gilrs tells controllers
/// apart itself and nothing is kept.
#[derive(Debug, Default)]
struct Slots {
    /// gilrs's number for a controller: its slot
    of: BTreeMap<usize, usize>,
}

impl Slots {
    /// Controller `pad` connected, which gilrs calls `named` (its maker `vendor`): the name and
    /// maker to go by. `now`: what each of the browser's slots holds (empty on the desktop).
    fn connected(&mut self, pad: usize, named: &str, vendor: Option<u16>, now: &[Option<String>]) -> (String, Option<u16>) {
        if let Some(&slot) = self.of.get(&pad) {
            // seen before: whatever is in its slot now
            if let Some(Some(id)) = now.get(slot) {
                return (id.clone(), vendor_in(id));
            }
        } else if let Some(slot) = (0..now.len()).find(|s| !self.of.values().any(|v| v == s) && now[*s].as_deref() == Some(named)) {
            // new to gilrs, so its words are right: the first slot no other controller has
            // that holds it
            self.of.insert(pad, slot);
        }
        (named.to_string(), vendor)
    }
}

/// Who the game listens to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Device {
    /// the keyboard and the mouse
    #[default]
    Keys,
    /// a controller (gilrs's number for it)
    Pad(usize),
}

/// Something a device did this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Happening {
    Connected(usize),
    Disconnected(usize),
    /// a controller's button pressed or released
    Pad { pad: usize, button: PadButton, down: bool },
    /// a controller's left stick past halfway one way (a direction pressed), or back
    Stick { pad: usize, dir: Button, down: bool },
    /// a key pressed, a click, the wheel, a touch, the mouse moved
    Keys,
}

/// Which device is listened to: controllers coming and going, and who's used.
#[derive(Clone, Debug, Default)]
pub struct Router {
    pub active: Device,
    /// the controllers connected, the latest last
    connected: Vec<usize>,
}

impl Router {
    pub fn pads(&self) -> &[usize] {
        &self.connected
    }

    /// A controller connected (or found at the start): it's listened to.
    pub fn connected(&mut self, pad: usize) {
        self.connected.retain(|&p| p != pad);
        self.connected.push(pad);
        self.active = Device::Pad(pad);
    }

    /// A controller gone: if it was listened to, the controller connected last is, else the
    /// keyboard.
    pub fn disconnected(&mut self, pad: usize) {
        self.connected.retain(|&p| p != pad);
        if self.active == Device::Pad(pad) {
            self.active = self.connected.last().map_or(Device::Keys, |&p| Device::Pad(p));
        }
    }

    /// A device used: it's listened to.
    pub fn used(&mut self, device: Device) {
        if let Device::Pad(p) = device {
            if !self.connected.contains(&p) {
                self.connected.push(p);
            }
        }
        self.active = device;
    }

    /// This frame's happenings, in order: who's listened to after them, then what that
    /// controller did, as the game's buttons (pressed or released) and as its own buttons
    /// (pressed). `window`: the game's window is active (else only controllers coming and
    /// going count: nothing done elsewhere moves the game).
    pub fn step(&mut self, happenings: &[Happening], window: bool, keys: &Bindings) -> (Vec<(Button, bool)>, Vec<PadButton>) {
        for h in happenings {
            match *h {
                Happening::Connected(p) => self.connected(p),
                Happening::Disconnected(p) => self.disconnected(p),
                Happening::Pad { pad, down: true, .. } | Happening::Stick { pad, down: true, .. } if window => self.used(Device::Pad(pad)),
                Happening::Keys if window => self.used(Device::Keys),
                _ => {}
            }
        }
        let mut presses = vec![];
        let mut raw = vec![];
        let active = match self.active {
            Device::Pad(p) if window => p,
            _ => return (presses, raw),
        };
        for h in happenings {
            match *h {
                Happening::Pad { pad, button, down } if pad == active => {
                    if down {
                        raw.push(button);
                    }
                    if let Some(b) = keys.button_of_pad(button) {
                        presses.push((b, down));
                    }
                }
                Happening::Stick { pad, dir, down } if pad == active => presses.push((dir, down)),
                _ => {}
            }
        }
        (presses, raw)
    }
}

/// The directions a stick is pushed (Up, Down, Left, Right; up is +y): past `PUSH` pressed,
/// back under `LET_GO` released (in between, as it was).
pub fn stick_directions(x: f32, y: f32, was: [bool; 4]) -> [bool; 4] {
    const PUSH: f32 = 0.55;
    const LET_GO: f32 = 0.35;
    let past = |v: f32, held: bool| if held { v > LET_GO } else { v > PUSH };
    [past(y, was[0]), past(-y, was[1]), past(-x, was[2]), past(x, was[3])]
}

/// What the game reads of the device listened to, this frame.
#[derive(Clone, Debug, Default)]
pub struct PadFrame {
    pub device: Device,
    /// the controller listened to (else the one connected last): its name and kind
    pub name: String,
    pub family: Family,
    /// how many controllers are connected
    pub count: usize,
    /// the game's buttons pressed and released on the controller this frame, in order
    pub presses: Vec<(Button, bool)>,
    /// the game's buttons held on it (or pressed this frame)
    pub held: [bool; 8],
    /// its own buttons pressed this frame (what the Controls take when one is changed)
    pub raw: Vec<PadButton>,
}

static FRAME: RwLock<Option<PadFrame>> = RwLock::new(None);

fn with<T>(f: impl FnOnce(&PadFrame) -> T, or: T) -> T {
    match FRAME.read() {
        Ok(frame) => frame.as_ref().map_or(or, f),
        Err(_) => or,
    }
}

/// The device listened to.
pub fn device() -> Device {
    with(|f| f.device, Device::Keys)
}

/// A controller is listened to.
pub fn on_pad() -> bool {
    matches!(device(), Device::Pad(_))
}

/// The kind of controller whose labels show (the one listened to, else the one connected last).
pub fn family() -> Family {
    with(|f| f.family, Family::Xbox)
}

/// The controller's name (the one listened to, else the one connected last), "" without one.
pub fn name() -> String {
    with(|f| f.name.clone(), String::new())
}

/// How many controllers are connected.
pub fn count() -> usize {
    with(|f| f.count, 0)
}

/// A game button held on the controller listened to.
pub fn held(b: Button) -> bool {
    with(|f| f.held[b.index()], false)
}

/// A game button pressed on the controller listened to, this frame.
pub fn pressed(b: Button) -> bool {
    with(|f| f.presses.iter().any(|&(p, down)| p == b && down), false)
}

/// The game's buttons pressed and released on the controller listened to, this frame.
pub fn presses() -> Vec<(Button, bool)> {
    with(|f| f.presses.clone(), vec![])
}

/// The controller's own buttons pressed this frame.
pub fn raw_pressed() -> Vec<PadButton> {
    with(|f| f.raw.clone(), vec![])
}

/// The controller's Start (Menu, Options, Plus) pressed this frame, when none of the game's
/// buttons is on it: the overworld leaves with it, as with Esc.
pub fn start_pressed() -> bool {
    with(|f| f.raw.contains(&PadButton::Start), false) && crate::input::bindings().button_of_pad(PadButton::Start).is_none()
}

/// The keyboard or the mouse used this frame (a key, a click, the wheel, a touch, the pointer
/// moved more than a jiggle).
fn keys_or_mouse(i: &egui::InputState) -> bool {
    i.pointer.delta().length() > 2.0
        || i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key { pressed: true, .. } | egui::Event::Text(_) | egui::Event::PointerButton { pressed: true, .. } | egui::Event::MouseWheel { .. } | egui::Event::Touch { .. }
            )
        })
}

/// The controllers, read once a frame (the app owns it).
pub struct Pads {
    gilrs: Option<gilrs::Gilrs>,
    router: Router,
    /// each controller's left stick: the directions pushed (Up, Down, Left, Right)
    sticks: BTreeMap<usize, [bool; 4]>,
    /// each controller's name and kind
    names: BTreeMap<usize, (String, Family)>,
    /// in the browser, the slot each controller came from
    slots: Slots,
    /// a word on a controller come or gone, and when (ms)
    note: Option<(String, f64)>,
    /// a controller has just taken over: the mouse pointer is to be put out of the game's sight
    /// (where it rests, nothing shows hovered)
    hide_pointer: bool,
}

/// How long a word on a controller stays (ms).
const NOTE_MS: f64 = 3000.0;

impl Pads {
    pub fn new() -> Pads {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(g) => Some(g),
            Err(gilrs::Error::NotImplemented(_)) => {
                log::info!("no controllers on this system");
                None
            }
            Err(e) => {
                log::warn!("the controllers can't be read: {e}");
                None
            }
        };
        let mut pads = Pads { gilrs, router: Router::default(), sticks: BTreeMap::new(), names: BTreeMap::new(), slots: Slots::default(), note: None, hide_pointer: false };
        // the controllers already there: the one found last is listened to
        let found: Vec<(usize, String, Family)> = pads.gilrs.as_ref().map_or(vec![], |g| g.gamepads().map(|(id, p)| (usize::from(id), plain_name(p.name()), Family::of(p.name(), p.vendor_id()))).collect());
        for (id, name, family) in found {
            pads.names.insert(id, (name, family));
            pads.router.connected(id);
        }
        pads
    }

    /// Once a frame, before anything reads the buttons: what the controllers, the keyboard and
    /// the mouse did, who's listened to, and what the game reads of it.
    pub fn update(&mut self, ctx: &egui::Context, now: f64) {
        let window = crate::platform::window_active(ctx);
        let keys = crate::input::bindings();
        let mut happenings = vec![];
        // what the browser's slots hold, read once a controller connects
        let mut slots_now = None;
        if let Some(g) = &mut self.gilrs {
            while let Some(ev) = g.next_event() {
                let pad = usize::from(ev.id);
                match ev.event {
                    gilrs::EventType::Connected => {
                        let p = g.gamepad(ev.id);
                        let (id, vendor) = self.slots.connected(pad, p.name(), p.vendor_id(), slots_now.get_or_insert_with(crate::platform::gamepad_ids));
                        let name = plain_name(&id);
                        let family = Family::of(&id, vendor);
                        self.note = Some((format!("Controller connected: {name}"), now));
                        self.names.insert(pad, (name, family));
                        happenings.push(Happening::Connected(pad));
                    }
                    gilrs::EventType::Disconnected => {
                        let name = self.names.get(&pad).map(|(n, _)| n.clone()).unwrap_or_default();
                        self.note = Some((format!("Controller disconnected: {name}"), now));
                        self.sticks.remove(&pad);
                        happenings.push(Happening::Disconnected(pad));
                    }
                    gilrs::EventType::ButtonPressed(b, _) => {
                        if let Some(button) = PadButton::from_gilrs(b) {
                            happenings.push(Happening::Pad { pad, button, down: true });
                        }
                    }
                    gilrs::EventType::ButtonReleased(b, _) => {
                        if let Some(button) = PadButton::from_gilrs(b) {
                            happenings.push(Happening::Pad { pad, button, down: false });
                        }
                    }
                    _ => {}
                }
            }
            // the left sticks: a direction pressed when pushed past halfway
            for (id, p) in g.gamepads() {
                let pad = usize::from(id);
                let was = self.sticks.get(&pad).copied().unwrap_or_default();
                let now_pushed = stick_directions(p.value(gilrs::Axis::LeftStickX), p.value(gilrs::Axis::LeftStickY), was);
                for (k, dir) in [Button::Up, Button::Down, Button::Left, Button::Right].into_iter().enumerate() {
                    if now_pushed[k] != was[k] {
                        happenings.push(Happening::Stick { pad, dir, down: now_pushed[k] });
                    }
                }
                self.sticks.insert(pad, now_pushed);
            }
        }
        // the keyboard and the mouse after the controllers: both in one frame, they win
        if ctx.input(keys_or_mouse) {
            happenings.push(Happening::Keys);
        }
        let before = self.router.active;
        let (presses, raw) = self.router.step(&happenings, window, &keys);
        if before != self.router.active {
            log::info!("listening to {:?}", self.router.active);
            if before == Device::Keys {
                self.hide_pointer = true;
            }
        }
        // what's held on the controller listened to
        let mut held = [false; 8];
        if let (Device::Pad(active), Some(g), true) = (self.router.active, &self.gilrs, window) {
            if let Some((_, p)) = g.gamepads().find(|(id, _)| usize::from(*id) == active) {
                let stick = self.sticks.get(&active).copied().unwrap_or_default();
                for b in Button::ALL {
                    let along = match b {
                        Button::Up => stick[0],
                        Button::Down => stick[1],
                        Button::Left => stick[2],
                        Button::Right => stick[3],
                        _ => false,
                    };
                    held[b.index()] = along || keys.pads(b).iter().any(|&pb| p.is_pressed(pb.to_gilrs()));
                }
            }
        }
        // a tap that began and ended between two frames still counts as held for one
        for &(b, down) in &presses {
            if down {
                held[b.index()] = true;
            }
        }
        let shown = match self.router.active {
            Device::Pad(p) => Some(p),
            Device::Keys => self.router.pads().last().copied(),
        };
        let (name, family) = shown.and_then(|p| self.names.get(&p).cloned()).unwrap_or_default();
        crate::platform::set_hook("device", if matches!(self.router.active, Device::Pad(_)) { "pad" } else { "keys" });
        crate::platform::set_hook("padname", &name);
        crate::platform::set_hook("pads", &self.router.pads().len().to_string());
        crate::platform::set_hook("padnote", self.note.as_ref().filter(|(_, at)| now - at < NOTE_MS).map_or("", |(t, _)| t.as_str()));
        let frame = PadFrame { device: self.router.active, name, family, count: self.router.pads().len(), presses, held, raw };
        if let Ok(mut f) = FRAME.write() {
            *f = Some(frame);
        }
    }

    /// A controller has just taken over: the mouse pointer goes out of the game's sight (asked
    /// once; the app tells egui, before the next frame).
    pub fn take_hide_pointer(&mut self) -> bool {
        std::mem::take(&mut self.hide_pointer)
    }

    /// The word on a controller come or gone, at the foot of the screen (it fades away).
    pub fn note_ui(&self, ctx: &egui::Context, now: f64) {
        let Some((text, at)) = &self.note else { return };
        let age = now - at;
        if age >= NOTE_MS {
            return;
        }
        let alpha = (((NOTE_MS - age) / 400.0).min(1.0) * (age / 150.0).min(1.0)) as f32;
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("pad-note")));
        let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(15.0), Color32::from_white_alpha((235.0 * alpha) as u8));
        let size = galley.size() + Vec2::new(32.0, 16.0);
        let rect = Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 64.0), size);
        painter.rect_filled(rect, 10.0, Color32::from_black_alpha((190.0 * alpha) as u8));
        painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_used_is_the_one_listened_to() {
        let keys = Bindings::default();
        let mut r = Router::default();
        assert_eq!(r.active, Device::Keys);
        // a controller connected takes over
        r.step(&[Happening::Connected(0)], true, &keys);
        assert_eq!(r.active, Device::Pad(0));
        // a key pressed: the keyboard again, the controller still connected
        r.step(&[Happening::Keys], true, &keys);
        assert_eq!(r.active, Device::Keys);
        assert_eq!(r.pads(), [0]);
        // the controller's button: it's listened to, and its press is read (South is A)
        let (presses, raw) = r.step(&[Happening::Pad { pad: 0, button: PadButton::South, down: true }], true, &keys);
        assert_eq!(r.active, Device::Pad(0));
        assert_eq!(presses, [(Button::A, true)]);
        assert_eq!(raw, [PadButton::South]);
        // a release alone doesn't take over; a second controller's press does
        r.step(&[Happening::Connected(1)], true, &keys);
        r.step(&[Happening::Pad { pad: 0, button: PadButton::South, down: true }], true, &keys);
        assert_eq!(r.active, Device::Pad(0));
        r.step(&[Happening::Pad { pad: 1, button: PadButton::East, down: false }], true, &keys);
        assert_eq!(r.active, Device::Pad(0), "a release doesn't take over");
        let (presses, _) = r.step(&[Happening::Stick { pad: 1, dir: Button::Down, down: true }], true, &keys);
        assert_eq!(r.active, Device::Pad(1));
        assert_eq!(presses, [(Button::Down, true)]);
        // the window not active: nothing done takes over, nothing is read
        r.step(&[Happening::Keys], false, &keys);
        assert_eq!(r.active, Device::Pad(1));
        let (presses, _) = r.step(&[Happening::Pad { pad: 1, button: PadButton::South, down: true }], false, &keys);
        assert!(presses.is_empty());
        // the controller listened to gone: the one connected before; then the keyboard
        r.step(&[Happening::Disconnected(1)], false, &keys);
        assert_eq!(r.active, Device::Pad(0));
        r.step(&[Happening::Disconnected(0)], true, &keys);
        assert_eq!(r.active, Device::Keys);
        // the keyboard and a controller in one frame: the keyboard (it comes last)
        r.step(&[Happening::Connected(2), Happening::Keys], true, &keys);
        assert_eq!(r.active, Device::Keys);
        // another controller's press while one is listened to isn't read as the first's
        r.step(&[Happening::Connected(3)], true, &keys);
        let (presses, _) = r.step(&[Happening::Pad { pad: 2, button: PadButton::South, down: true }, Happening::Pad { pad: 3, button: PadButton::East, down: true }], true, &keys);
        assert_eq!(r.active, Device::Pad(3));
        assert_eq!(presses, [(Button::B, true)]);
    }

    #[test]
    fn the_systems_controllers_can_be_asked_for() {
        // whatever the machine has (a CI machine has none, maybe not even a way to ask)
        let pads = Pads::new();
        eprintln!("controllers readable: {}, connected: {}", pads.gilrs.is_some(), pads.router.pads().len());
    }

    #[test]
    fn the_stick_moves_like_the_dpad() {
        let none = [false; 4];
        assert_eq!(stick_directions(0.0, 0.3, none), none, "a small push is nothing");
        let up = stick_directions(0.1, 0.8, none);
        assert_eq!(up, [true, false, false, false]);
        // held while it stays past the lower line, let go under it
        assert_eq!(stick_directions(0.0, 0.45, up), up);
        assert_eq!(stick_directions(0.0, 0.2, up), none);
        assert_eq!(stick_directions(-0.9, -0.7, none), [false, true, true, false]);
    }

    #[test]
    fn controllers_are_told_apart_and_named() {
        assert_eq!(Family::of("Xbox Wireless Controller", None), Family::Xbox);
        assert_eq!(Family::of("Wireless Controller", None), Family::PlayStation);
        assert_eq!(Family::of("DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)", Some(0x054c)), Family::PlayStation);
        assert_eq!(Family::of("Some Gamepad", Some(0x057e)), Family::RightA);
        assert_eq!(Family::of("Generic USB Joystick", None), Family::Xbox);
        assert_eq!(PadButton::South.label(Family::Xbox), "A");
        assert_eq!(PadButton::South.label(Family::PlayStation), "Cross");
        assert_eq!(PadButton::East.label(Family::RightA), "A");
        assert_eq!(plain_name("Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)"), "Xbox Wireless Controller");
        assert_eq!(plain_name("054c-0ce6-DualSense Wireless Controller"), "DualSense Wireless Controller");
        assert_eq!(plain_name("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), "Xbox 360 Controller");
        assert_eq!(plain_name("Arcade Stick (2 players)"), "Arcade Stick (2 players)");
        assert_eq!(plain_name("  "), "Controller");
        for b in PadButton::ALL {
            assert_eq!(PadButton::from_name(b.name()), Some(b));
            assert_eq!(PadButton::from_gilrs(b.to_gilrs()), Some(b));
        }
    }

    #[test]
    fn a_controller_in_a_slot_used_before_goes_by_its_own_name() {
        let xbox = "Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)";
        let dualsense = "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)";
        let slots = |ids: &[Option<&str>]| ids.iter().map(|i| i.map(str::to_string)).collect::<Vec<_>>();
        // the desktop: gilrs's words, nothing kept
        let mut s = Slots::default();
        assert_eq!(s.connected(0, "Xbox Wireless Controller", Some(0x045e), &[]), ("Xbox Wireless Controller".to_string(), Some(0x045e)));
        assert!(s.of.is_empty());
        // the browser: an Xbox controller in slot 0, then a DualSense in slot 1 (new to gilrs:
        // its words, and the slot each came from)
        assert_eq!(s.connected(0, xbox, Some(0x045e), &slots(&[Some(xbox)])).0, xbox);
        assert_eq!(s.connected(1, dualsense, Some(0x054c), &slots(&[Some(xbox), Some(dualsense)])).0, dualsense);
        assert_eq!((s.of[&0], s.of[&1]), (0, 1));
        // both gone, a DualSense connected in slot 0: gilrs still says Xbox, the slot DualSense
        assert_eq!(s.connected(0, xbox, Some(0x045e), &slots(&[Some(dualsense), None])), (dualsense.to_string(), Some(0x054c)));
        assert_eq!(Family::of(dualsense, vendor_in(dualsense)), Family::PlayStation);
        // the same controller back in its slot: the same name; its slot empty by then: gilrs's
        assert_eq!(s.connected(1, dualsense, Some(0x054c), &slots(&[Some(dualsense), Some(dualsense)])).0, dualsense);
        assert_eq!(s.connected(0, xbox, Some(0x045e), &slots(&[None, None])).0, xbox);
        // two of a kind new at once: a slot each
        let mut s = Slots::default();
        let both = slots(&[Some(xbox), Some(xbox)]);
        s.connected(0, xbox, Some(0x045e), &both);
        s.connected(1, xbox, Some(0x045e), &both);
        assert_eq!((s.of[&0], s.of[&1]), (0, 1));
        // the maker's number in Chrome's ids and Firefox's
        assert_eq!(vendor_in(dualsense), Some(0x054c));
        assert_eq!(vendor_in("045e-0b13-Xbox Wireless Controller"), Some(0x045e));
        assert_eq!(vendor_in("057e-2009-Gamepad"), Some(0x057e));
        assert_eq!(vendor_in("Arcade Stick (2 players)"), None);
        assert_eq!(vendor_in("Pad (Vendor: 12)"), None);
    }
}
