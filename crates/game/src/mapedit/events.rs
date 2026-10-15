//! Events in the editor: the story state (a running script, the flags, the dialog box,
//! the camera a script holds), what sets events off while walking (speaking to what's in
//! front, stepping onto a cell, the map starting), the editor as the scripts' host, and the
//! Events tool: the actors and the events on the map, each with its script.

use std::collections::VecDeque;

use content::script::{self, Target};
use content::{Actor, Event};
use egui::{Color32, RichText};
use glam::Vec3;
use render::scene::{Shape, ShapeKind};
use world::events::{Flags, Host, Machine, Wait};

use super::actors::{facing_of, facing_toward, step_of};
use super::{hint, mark, section, warn, MapEditor, Tool};
use crate::hud::{ACCENT, MUTED, TEXT};

/// Which event: one of the map's, or one of an actor's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventRef {
    Map(usize),
    Actor(usize, usize),
}

/// What the editor asks the game for.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// a battle; the script goes on after it
    Battle(String),
}

/// What a click on the map does next, in the Events tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placing {
    /// a new actor on the cell clicked
    Actor,
    /// a new event over the cells dragged
    Event,
    /// an event's area, dragged again
    EventArea(EventRef),
    /// an actor's wander area
    ActorArea(usize),
    /// cells clicked join an actor's path
    Path(usize),
}

/// The dialog box: a line being typed out, a choice being made.
pub(super) struct Dialog {
    pub who: String,
    pub text: String,
    pub t0: f64,
    pub choices: Vec<String>,
    pub pick: usize,
    pub all_shown: bool,
}

/// Milliseconds per character typed out.
const TYPE_MS: f64 = 22.0;

impl Dialog {
    fn shown(&self, now: f64) -> &str {
        if self.all_shown {
            return &self.text;
        }
        let n = ((now - self.t0) / TYPE_MS).max(0.0) as usize;
        match self.text.char_indices().nth(n) {
            Some((i, _)) => &self.text[..i],
            None => &self.text,
        }
    }

    fn typing(&self, now: f64) -> bool {
        !self.all_shown && self.shown(now).len() < self.text.len()
    }
}

#[derive(Default)]
pub(super) struct Story {
    pub machine: Option<Machine>,
    /// the event running (for the status)
    pub running: String,
    pub flags: Flags,
    pub dialog: Option<Dialog>,
    /// a script holds the camera here
    pub cam_hold: Option<Vec3>,
    pub cam_until: f64,
    /// the walker's last cell, for `enter`
    pub last_cell: Option<(i32, i32)>,
    /// the map's `start` events, fired when walking starts
    pub started: bool,
    pub pending: VecDeque<EventRef>,
    pub request: Option<Request>,
    /// one-shots to play: (file, volume)
    pub shots: Vec<(String, f32)>,
    /// a battle a script asked for is over
    pub battle_over: bool,
}

/// The Events tool's state.
#[derive(Default)]
pub(super) struct Panel {
    pub event_sel: Option<EventRef>,
    pub actor_sel: Option<usize>,
    pub placing: Option<Placing>,
    /// a drag on the map: from cell, to cell
    pub drag: Option<((i32, i32), (i32, i32))>,
}

fn rect_cells(a: (i32, i32), b: (i32, i32)) -> [i32; 4] {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
    [x0, z0, x1 - x0 + 1, z1 - z0 + 1]
}

fn in_area(area: [i32; 4], c: (i32, i32)) -> bool {
    c.0 >= area[0] && c.0 < area[0] + area[2] && c.1 >= area[1] && c.1 < area[1] + area[3]
}

fn trigger_color(trigger: &str) -> [f32; 4] {
    match trigger {
        "enter" => [0.3, 0.7, 1.0, 0.35],
        "start" => [0.4, 0.95, 0.5, 0.35],
        _ => [1.0, 0.8, 0.25, 0.35],
    }
}

// ---------------------------------------------------------------- the scripts' host

impl Host for MapEditor {
    fn say(&mut self, who: &str, text: &str) {
        self.story.dialog = Some(Dialog { who: who.to_string(), text: text.to_string(), t0: self.now, choices: vec![], pick: 0, all_shown: false });
    }

    fn ask(&mut self, options: &[String]) {
        match &mut self.story.dialog {
            Some(d) => {
                d.choices = options.to_vec();
                d.pick = 0;
                d.all_shown = true;
            }
            None => self.story.dialog = Some(Dialog { who: String::new(), text: String::new(), t0: self.now, choices: options.to_vec(), pick: 0, all_shown: true }),
        }
    }

    fn walk(&mut self, who: &str, cells: &[[i32; 2]]) {
        if who == "player" {
            if let Some(w) = &mut self.walker {
                w.route = cells.iter().map(|c| (c[0], c[1])).collect();
            }
        } else if let Some(i) = self.actor_index(who) {
            self.route_actor(i, cells);
        }
    }

    fn face(&mut self, who: &str, to: &str) {
        let at = |ed: &MapEditor, name: &str| -> Option<(i32, i32)> {
            if name == "player" {
                ed.walker.as_ref().map(|w| w.cell)
            } else {
                ed.actor_index(name).map(|i| ed.live[i].cell)
            }
        };
        let Some(from) = at(self, who) else { return };
        let facing = match to {
            "n" => 180.0,
            "s" => 0.0,
            "e" => 270.0,
            "w" => 90.0,
            other => match at(self, other) {
                Some(cell) => facing_toward(from, cell),
                None => return,
            },
        };
        if who == "player" {
            if let Some(w) = &mut self.walker {
                w.facing = facing;
            }
        } else if let Some(i) = self.actor_index(who) {
            self.live[i].facing = facing;
        }
        self.extra_changed = true;
    }

    fn show(&mut self, who: &str, shown: bool) {
        if let Some(i) = self.actor_index(who) {
            self.live[i].hidden = !shown;
            self.extra_changed = true;
        }
    }

    fn sound(&mut self, file: &str, volume: f64) {
        self.story.shots.push((file.to_string(), volume as f32));
    }

    fn camera(&mut self, to: &Target) {
        self.story.cam_until = self.now + 700.0;
        self.story.cam_hold = match to {
            Target::Cell(c) => Some(self.cell_pos((c[0], c[1]))),
            Target::Who(w) if w == "player" => None,
            Target::Who(w) => self.actor_index(w).map(|i| self.actor_pos(i, self.now)),
        };
    }

    fn warp(&mut self, map: &str, at: [i32; 2]) {
        if self.dirty {
            self.status = format!("Save {} first, then the warp to {map} works", self.map.id);
            return;
        }
        if self.content.maps.entry(map).is_none() {
            self.status = format!("No map {map} to warp to");
            return;
        }
        let facing = self.walker.as_ref().map(|w| w.facing).unwrap_or(0.0);
        self.arrive = Some(((at[0], at[1]), facing));
        self.story.dialog = None;
        self.open(map);
    }

    fn battle(&mut self, encounter: &str) {
        self.story.battle_over = false;
        self.story.request = Some(Request::Battle(encounter.to_string()));
    }
}

// ---------------------------------------------------------------- running

impl MapEditor {
    /// Whether an event may run now: its flags, and once means once.
    fn event_allowed(&self, e: &Event) -> bool {
        let set = |f: &str| self.story.flags.get(f).copied().unwrap_or(0) != 0;
        (!e.once || !set(&format!("done.{}", e.id))) && (e.when.is_empty() || set(&e.when)) && (e.unless.is_empty() || !set(&e.unless))
    }

    pub(super) fn event(&self, r: EventRef) -> Option<&Event> {
        match r {
            EventRef::Map(i) => self.map.events.get(i),
            EventRef::Actor(a, i) => self.map.actors.get(a).and_then(|x| x.events.get(i)),
        }
    }

    fn event_mut(&mut self, r: EventRef) -> Option<&mut Event> {
        match r {
            EventRef::Map(i) => self.map.events.get_mut(i),
            EventRef::Actor(a, i) => self.map.actors.get_mut(a).and_then(|x| x.events.get_mut(i)),
        }
    }

    /// Starts an event's script.
    pub(super) fn run_event(&mut self, r: EventRef) {
        let Some(e) = self.event(r).cloned() else { return };
        match script::parse(&e.script) {
            Ok(actions) => {
                self.story.machine = Some(Machine::new(actions, e.once.then(|| format!("done.{}", e.id))));
                self.story.running = e.id.clone();
                self.status = format!("Event {}", e.id);
            }
            Err(errors) => self.status = format!("Event {}: {}", e.id, errors.join("; ")),
        }
    }

    /// The cell the walker faces, and what's there to speak to.
    fn try_interact(&mut self) {
        let Some(w) = &self.walker else { return };
        let d = step_of(w.facing);
        let ahead = (w.cell.0 + d.0, w.cell.1 + d.1);
        let own = w.cell;
        if let Some(a) = self.actor_at(ahead) {
            let events: Vec<EventRef> = (0..self.map.actors[a].events.len()).map(|i| EventRef::Actor(a, i)).collect();
            for r in events {
                let e = self.event(r).unwrap();
                if e.trigger == "interact" && self.event_allowed(e) {
                    // it turns to the player first
                    self.live[a].facing = facing_toward(ahead, own);
                    self.run_event(r);
                    return;
                }
            }
        }
        for cell in [ahead, own] {
            for i in 0..self.map.events.len() {
                let e = &self.map.events[i];
                if e.trigger == "interact" && in_area(e.area, cell) && self.event_allowed(e) {
                    self.run_event(EventRef::Map(i));
                    return;
                }
            }
        }
    }

    /// A frame of the story: what the walker sets off, the script going on, the dialog.
    pub(super) fn step_story(&mut self, ctx: &egui::Context, now: f64) {
        self.now = now;
        let typing = ctx.egui_wants_keyboard_input();
        let pressed = |k: egui::Key| !typing && ctx.input(|i| i.key_pressed(k));
        let a = pressed(egui::Key::Z) || pressed(egui::Key::Space) || pressed(egui::Key::Enter);
        // the dialog's keys
        if let Some(d) = &mut self.story.dialog {
            if !d.choices.is_empty() {
                if pressed(egui::Key::ArrowUp) {
                    d.pick = d.pick.saturating_sub(1);
                }
                if pressed(egui::Key::ArrowDown) {
                    d.pick = (d.pick + 1).min(d.choices.len() - 1);
                }
            }
            if a {
                if d.typing(now) {
                    d.all_shown = true;
                } else if d.choices.is_empty() {
                    let line_over = matches!(self.story.machine.as_ref().map(|m| &m.wait), Some(Wait::Line));
                    self.story.dialog = None;
                    if line_over {
                        self.story.machine.as_mut().unwrap().resume();
                    }
                } else {
                    let pick = d.pick;
                    self.story.dialog = None;
                    if let Some(m) = &mut self.story.machine {
                        m.choose(pick);
                    }
                }
            }
        } else if self.story.machine.is_none() && self.walker.is_some() {
            // nothing running: what the walker does sets events off
            let cell = self.walker.as_ref().unwrap().cell;
            if !self.story.started {
                self.story.started = true;
                self.story.pending = (0..self.map.events.len()).filter(|&i| self.map.events[i].trigger == "start").map(EventRef::Map).collect();
            }
            if self.story.last_cell != Some(cell) {
                self.story.last_cell = Some(cell);
                for i in 0..self.map.events.len() {
                    let e = &self.map.events[i];
                    if e.trigger == "enter" && in_area(e.area, cell) && self.event_allowed(e) {
                        self.run_event(EventRef::Map(i));
                        break;
                    }
                }
            }
            if self.story.machine.is_none() {
                while let Some(r) = self.story.pending.pop_front() {
                    if self.event(r).map(|e| self.event_allowed(e)).unwrap_or(false) {
                        self.run_event(r);
                        break;
                    }
                }
            }
            if self.story.machine.is_none() && a {
                self.try_interact();
            }
        }
        // the script goes on
        if let Some(mut m) = self.story.machine.take() {
            match &m.wait {
                Wait::Moving(who) => {
                    let moving = if who == "player" {
                        self.walker.as_ref().map(|w| !w.route.is_empty() || w.stepping(now)).unwrap_or(false)
                    } else {
                        self.actor_index(who).map(|i| self.actor_moving(i, now)).unwrap_or(false)
                    };
                    if !moving {
                        m.resume();
                    }
                }
                Wait::Camera if now >= self.story.cam_until => m.resume(),
                Wait::Battle if self.story.battle_over => {
                    self.story.battle_over = false;
                    m.resume();
                }
                _ => {}
            }
            let mut flags = std::mem::take(&mut self.story.flags);
            m.run(self, &mut flags, now / 1000.0);
            self.story.flags = flags;
            if m.finished() {
                self.story.running.clear();
                if self.story.dialog.as_ref().map(|d| d.choices.is_empty()).unwrap_or(false) && m.wait != Wait::Line {
                    // a last line: shown until A
                } else if self.story.dialog.is_none() {
                    self.story.cam_hold = None;
                }
            } else {
                self.story.machine = Some(m);
            }
        } else if self.story.dialog.is_none() {
            self.story.cam_hold = None;
        }
        let d = self.story.dialog.as_ref();
        crate::platform::set_hook("dialog", d.map(|d| d.text.as_str()).unwrap_or(""));
        crate::platform::set_hook("dialogwho", d.map(|d| d.who.as_str()).unwrap_or(""));
        crate::platform::set_hook("choices", &d.map(|d| d.choices.join("|")).unwrap_or_default());
        crate::platform::set_hook("pick", &d.map(|d| d.pick.to_string()).unwrap_or_default());
        crate::platform::set_hook("running", &self.story.running);
        let flags: Vec<String> = self.story.flags.iter().map(|(k, v)| format!("{k}={v}")).collect();
        crate::platform::set_hook("flags", &flags.join(";"));
        crate::platform::set_hook("actors", &self.map.actors.len().to_string());
        crate::platform::set_hook("events", &(self.map.events.len() + self.map.actors.iter().map(|a| a.events.len()).sum::<usize>()).to_string());
    }

    /// Back from a battle a script asked for.
    pub fn resume_after_battle(&mut self) {
        self.story.battle_over = true;
    }

    /// What the editor wants the game to do (taken once).
    pub fn take_request(&mut self) -> Option<Request> {
        self.story.request.take()
    }

    /// The dialog box over the view.
    pub(super) fn dialog_ui(&mut self, ctx: &egui::Context, now: f64) {
        let Some(d) = &self.story.dialog else { return };
        let v = self.viewport;
        let width = (v.width() - 40.0).clamp(240.0, 640.0);
        let shown = d.shown(now).to_string();
        let typing = d.typing(now);
        egui::Area::new(egui::Id::new("dialog")).order(egui::Order::Foreground).fixed_pos(egui::pos2(v.center().x - width / 2.0, v.bottom() - 24.0)).pivot(egui::Align2::LEFT_BOTTOM).show(ctx, |ui| {
            egui::Frame::new().fill(Color32::from_rgba_unmultiplied(20, 27, 41, 236)).corner_radius(10.0).inner_margin(14.0).stroke(egui::Stroke::new(1.0, Color32::from_gray(70))).show(ui, |ui| {
                ui.set_width(width);
                if !d.who.is_empty() {
                    ui.label(RichText::new(&d.who).strong().color(ACCENT).size(14.0));
                }
                if !d.text.is_empty() {
                    ui.label(RichText::new(shown).color(TEXT).size(15.0));
                }
                if !d.choices.is_empty() && !typing {
                    ui.add_space(4.0);
                    for (i, c) in d.choices.iter().enumerate() {
                        let picked = i == d.pick;
                        let text = format!("{} {c}", if picked { ">" } else { "  " });
                        ui.label(RichText::new(text).color(if picked { ACCENT } else { MUTED }).size(14.0));
                    }
                    ui.label(RichText::new("Up and down choose, A picks").size(11.0).color(MUTED));
                } else {
                    ui.label(RichText::new(if typing { "A: all of it" } else { "A: go on" }).size(11.0).color(MUTED));
                }
            });
        });
    }

    // ------------------------------------------------------------ the tool on the map

    pub(super) fn events_view(&mut self, pressed: bool, down: bool, released: bool, over_view: bool, secondary: bool) {
        let cell = self.ground_hit;
        if secondary {
            self.evt.placing = None;
            self.evt.drag = None;
        }
        match self.evt.placing {
            Some(Placing::Actor) => {
                if let (true, Some(c)) = (pressed && over_view, cell) {
                    self.checkpoint();
                    let n = self.map.actors.len() + 1;
                    let mut id = format!("actor_{n}");
                    let mut k = n;
                    while self.map.actors.iter().any(|a| a.id == id) {
                        k += 1;
                        id = format!("actor_{k}");
                    }
                    let look = self.content.props.get("stand_in_character").map(|_| "prop:stand_in_character".to_string()).unwrap_or_default();
                    self.map.actors.push(Actor { id, name: "Someone".into(), look, at: [c.0, c.1], level: self.map.level_at(c.0, c.1).unwrap_or(0), area: [c.0 - 2, c.1 - 2, 5, 5], ..Default::default() });
                    self.evt.actor_sel = Some(self.map.actors.len() - 1);
                    self.evt.event_sel = None;
                    self.evt.placing = None;
                    self.rebuild_actors();
                }
            }
            Some(Placing::Path(i)) => {
                if let (true, Some(c)) = (pressed && over_view, cell) {
                    if i < self.map.actors.len() {
                        self.checkpoint();
                        self.map.actors[i].path.push([c.0, c.1]);
                        self.map.actors[i].moves = "path".into();
                    }
                }
            }
            Some(Placing::Event) | Some(Placing::EventArea(_)) | Some(Placing::ActorArea(_)) => {
                if let (true, Some(c)) = (pressed && over_view, cell) {
                    self.evt.drag = Some((c, c));
                }
                if let (true, Some(c), Some(d)) = (down, cell, self.evt.drag.as_mut()) {
                    d.1 = c;
                }
                if let (true, Some((a, b))) = (released, self.evt.drag.take()) {
                    let area = rect_cells(a, b);
                    self.checkpoint();
                    match self.evt.placing {
                        Some(Placing::Event) => {
                            let n = self.map.events.len() + 1;
                            let mut id = format!("event_{n}");
                            let mut k = n;
                            while self.map.events.iter().any(|e| e.id == id) {
                                k += 1;
                                id = format!("event_{k}");
                            }
                            self.map.events.push(Event { id, area, script: vec!["say Something happens here.".into()], ..Default::default() });
                            self.evt.event_sel = Some(EventRef::Map(self.map.events.len() - 1));
                            self.evt.actor_sel = None;
                        }
                        Some(Placing::EventArea(r)) => {
                            if let Some(e) = self.event_mut(r) {
                                e.area = area;
                            }
                        }
                        Some(Placing::ActorArea(i)) => {
                            if let Some(a) = self.map.actors.get_mut(i) {
                                a.area = area;
                                a.moves = "wander".into();
                            }
                        }
                        _ => {}
                    }
                    self.evt.placing = None;
                }
            }
            None => {
                // a click picks what's there: an actor, else an event
                if let (true, Some(c)) = (pressed && over_view, cell) {
                    if let Some(a) = self.map.actors.iter().position(|a| (a.at[0], a.at[1]) == c).or_else(|| self.actor_at(c)) {
                        self.evt.actor_sel = Some(a);
                        self.evt.event_sel = None;
                    } else if let Some(i) = self.map.events.iter().position(|e| in_area(e.area, c)) {
                        self.evt.event_sel = Some(EventRef::Map(i));
                        self.evt.actor_sel = None;
                    } else {
                        self.evt.event_sel = None;
                        self.evt.actor_sel = None;
                    }
                }
            }
        }
        crate::platform::set_hook("placing", &self.evt.placing.map(|p| format!("{p:?}")).unwrap_or_default());
    }

    /// The events' areas, the actors' areas and paths, the drag going on.
    pub(super) fn events_shapes(&self, shapes: &mut Vec<Shape>, now: f64) {
        let lh = self.level_height;
        let rect = |x: i32, z: i32, w: i32, h: i32, y: f32, color: [f32; 4]| Shape {
            kind: ShapeKind::Rect { width: w as f32, depth: h as f32 },
            center: Vec3::new(x as f32 + w as f32 / 2.0, y, z as f32 + h as f32 / 2.0),
            color,
            additive: false,
        };
        let y_at = |x: i32, z: i32| self.map.level_at(x, z).unwrap_or(0) as f32 * lh + 0.06;
        if self.tool != Tool::Events {
            return;
        }
        for (i, e) in self.map.events.iter().enumerate() {
            if e.area[2] < 1 || e.area[3] < 1 {
                continue;
            }
            let mut c = trigger_color(&e.trigger);
            if self.evt.event_sel == Some(EventRef::Map(i)) {
                c[3] = 0.6;
            }
            shapes.push(rect(e.area[0], e.area[1], e.area[2], e.area[3], y_at(e.area[0], e.area[1]), c));
        }
        for (i, a) in self.map.actors.iter().enumerate() {
            let sel = self.evt.actor_sel == Some(i);
            if a.moves == "wander" && a.area[2] > 0 && a.area[3] > 0 {
                shapes.push(rect(a.area[0], a.area[1], a.area[2], a.area[3], y_at(a.area[0], a.area[1]), [0.75, 0.45, 0.95, if sel { 0.4 } else { 0.18 }]));
            }
            if a.moves == "path" {
                let mut prev = (a.at[0], a.at[1]);
                for p in &a.path {
                    let to = (p[0], p[1]);
                    let from3 = Vec3::new(prev.0 as f32 + 0.5, y_at(prev.0, prev.1), prev.1 as f32 + 0.5);
                    let to3 = Vec3::new(to.0 as f32 + 0.5, y_at(to.0, to.1), to.1 as f32 + 0.5);
                    super::zones::arrow(from3, to3, 0.18, [0.75, 0.45, 0.95, if sel { 0.9 } else { 0.5 }], shapes);
                    prev = to;
                }
            }
            let pos = if i < self.live.len() { self.actor_pos(i, now) } else { self.cell_pos((a.at[0], a.at[1])) };
            shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.4, outer: 0.5 }, center: pos + Vec3::Y * 0.05, color: [0.75, 0.45, 0.95, if sel { 0.95 } else { 0.5 }], additive: false });
        }
        if let Some((a, b)) = self.evt.drag {
            let r = rect_cells(a, b);
            shapes.push(rect(r[0], r[1], r[2], r[3], y_at(r[0], r[1]), [1.0, 1.0, 1.0, 0.35]));
        }
    }

    // ------------------------------------------------------------ the panels

    pub(super) fn events_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Actors");
        hint(ui, "People, creatures and things with a life of their own: they stand, wander or walk a path, and answer when spoken to.");
        let add = ui.selectable_label(self.evt.placing == Some(Placing::Actor), "Add an actor (click a cell)");
        mark("uiAddActor", &add);
        if add.clicked() {
            self.evt.placing = if self.evt.placing == Some(Placing::Actor) { None } else { Some(Placing::Actor) };
        }
        let ids: Vec<(String, String)> = self.map.actors.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        for (i, (id, name)) in ids.iter().enumerate() {
            let r = ui.selectable_label(self.evt.actor_sel == Some(i), format!("{name} ({id})"));
            mark(&format!("uiActor_{id}"), &r);
            if r.clicked() {
                self.evt.actor_sel = Some(i);
                self.evt.event_sel = None;
            }
        }
        if let Some(i) = self.evt.actor_sel.filter(|&i| i < self.map.actors.len()) {
            self.actor_fields(ui, i);
        }
        section(ui, "Events");
        hint(ui, "What happens on cells: spoken to (interact), stepped on (enter) or when the map starts.");
        let add = ui.selectable_label(self.evt.placing == Some(Placing::Event), "Add an event (drag cells)");
        mark("uiAddEvent", &add);
        if add.clicked() {
            self.evt.placing = if self.evt.placing == Some(Placing::Event) { None } else { Some(Placing::Event) };
        }
        let mut list: Vec<(EventRef, String)> = self.map.events.iter().enumerate().map(|(i, e)| (EventRef::Map(i), format!("{} · {} · {},{} {}×{}", e.id, e.trigger, e.area[0], e.area[1], e.area[2], e.area[3]))).collect();
        if let Some(a) = self.evt.actor_sel.filter(|&a| a < self.map.actors.len()) {
            let actor = &self.map.actors[a];
            list.extend(actor.events.iter().enumerate().map(|(i, e)| (EventRef::Actor(a, i), format!("{} · {} · on {}", e.id, e.trigger, actor.id))));
            let r = ui.button(format!("Add an event on {}", actor.id));
            mark("uiAddActorEvent", &r);
            if r.clicked() {
                self.checkpoint();
                let n = self.map.actors[a].events.len() + 1;
                let id = format!("{}_{n}", self.map.actors[a].id);
                let name = self.map.actors[a].name.clone();
                self.map.actors[a].events.push(Event { id, script: vec![format!("say {name}: Hello there.")], ..Default::default() });
                self.evt.event_sel = Some(EventRef::Actor(a, self.map.actors[a].events.len() - 1));
            }
        }
        for (r, label) in list {
            let id = self.event(r).map(|e| e.id.clone()).unwrap_or_default();
            let row = ui.selectable_label(self.evt.event_sel == Some(r), label);
            mark(&format!("uiEvent_{id}"), &row);
            if row.clicked() {
                self.evt.event_sel = Some(r);
            }
        }
        if self.evt.event_sel.is_some() {
            hint(ui, "The event's script is in the inspector, on the right.");
        }
        section(ui, "Story");
        hint(ui, "Walk the map (Walk) to try it: A speaks to what's in front, cells set off their events. Flags are what the story remembers.");
        if self.story.flags.is_empty() {
            hint(ui, "No flags set.");
        }
        let flags: Vec<String> = self.story.flags.iter().map(|(k, v)| format!("{k} = {v}")).collect();
        for f in flags {
            ui.label(RichText::new(f).size(11.5));
        }
        ui.horizontal(|ui| {
            let reset = ui.button("Forget the flags");
            mark("uiResetFlags", &reset);
            if reset.clicked() {
                self.story.flags.clear();
                self.story.started = false;
            }
            if ui.button("Stop the script").clicked() {
                self.story.machine = None;
                self.story.dialog = None;
                self.story.running.clear();
                self.story.cam_hold = None;
            }
        });
    }

    fn actor_fields(&mut self, ui: &mut egui::Ui, i: usize) {
        let mut a = self.map.actors[i].clone();
        let mut delete = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Id");
            ui.add(egui::TextEdit::singleline(&mut a.id).desired_width(100.0));
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(90.0));
        });
        ui.horizontal(|ui| {
            ui.label("Looks like");
            let (kind, rest) = a.look.split_once(':').map(|(k, r)| (k.to_string(), r.to_string())).unwrap_or_default();
            for (k, label) in [("prop", "a prop"), ("creature", "a creature"), ("sprite", "a picture")] {
                if ui.selectable_label(kind == k, label).clicked() && kind != k {
                    let first = match k {
                        "prop" => self.content.props.get("stand_in_character").map(|p| p.id.clone()).or_else(|| self.content.props.iter().next().map(|p| p.id.clone())).unwrap_or_default(),
                        "creature" => self.content.creatures.iter().next().map(|c| c.id.clone()).unwrap_or_default(),
                        _ => rest.clone(),
                    };
                    a.look = format!("{k}:{first}");
                }
            }
        });
        let (kind, mut rest) = a.look.split_once(':').map(|(k, r)| (k.to_string(), r.to_string())).unwrap_or_default();
        match kind.as_str() {
            "prop" => {
                let ids: Vec<(String, String)> = self.content.props.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
                let shown = ids.iter().find(|(id, _)| *id == rest).map(|(_, n)| n.clone()).unwrap_or(rest.clone());
                egui::ComboBox::from_id_salt("actor-prop").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                    for (id, name) in &ids {
                        if ui.selectable_label(*id == rest, name).clicked() {
                            rest = id.clone();
                        }
                    }
                });
                a.look = format!("prop:{rest}");
            }
            "creature" => {
                let ids: Vec<(String, String)> = self.content.creatures.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
                if ids.is_empty() {
                    warn(ui, "No creatures in the content yet.");
                }
                let shown = ids.iter().find(|(id, _)| *id == rest).map(|(_, n)| n.clone()).unwrap_or(rest.clone());
                egui::ComboBox::from_id_salt("actor-creature").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                    for (id, name) in &ids {
                        if ui.selectable_label(*id == rest, name).clicked() {
                            rest = id.clone();
                        }
                    }
                });
                a.look = format!("creature:{rest}");
            }
            _ => {
                ui.add(egui::TextEdit::singleline(&mut rest).hint_text("pictures/someone.png (under assets/)").desired_width(200.0));
                a.look = format!("sprite:{rest}");
            }
        }
        ui.horizontal(|ui| {
            ui.label("At");
            ui.add(egui::DragValue::new(&mut a.at[0]).range(0..=self.map.width as i32 - 1));
            ui.add(egui::DragValue::new(&mut a.at[1]).range(0..=self.map.height as i32 - 1));
            ui.label("faces");
            ui.add(egui::DragValue::new(&mut a.facing).speed(15.0).range(0.0..=359.0).suffix("°"));
        });
        ui.horizontal(|ui| {
            ui.label("Moves");
            for (k, label) in [("still", "not at all"), ("wander", "about an area"), ("path", "along a path")] {
                let r = ui.selectable_label(a.moves == k, label);
                mark(&format!("uiMoves_{k}"), &r);
                if r.clicked() {
                    a.moves = k.into();
                }
            }
        });
        match a.moves.as_str() {
            "wander" => {
                ui.horizontal(|ui| {
                    ui.label(format!("Area {},{} {}×{}", a.area[0], a.area[1], a.area[2], a.area[3]));
                    let r = ui.selectable_label(self.evt.placing == Some(Placing::ActorArea(i)), "Drag it");
                    if r.clicked() {
                        self.evt.placing = if self.evt.placing == Some(Placing::ActorArea(i)) { None } else { Some(Placing::ActorArea(i)) };
                    }
                });
            }
            "path" => {
                ui.horizontal(|ui| {
                    ui.label(format!("{} cells", a.path.len()));
                    let r = ui.selectable_label(self.evt.placing == Some(Placing::Path(i)), "Click cells to add");
                    mark("uiAddPath", &r);
                    if r.clicked() {
                        self.evt.placing = if self.evt.placing == Some(Placing::Path(i)) { None } else { Some(Placing::Path(i)) };
                    }
                    if ui.button("Clear").clicked() {
                        a.path.clear();
                    }
                });
            }
            _ => {}
        }
        ui.add(egui::Slider::new(&mut a.speed, 0.5..=8.0).text("cells a second"));
        let mut template = false;
        ui.horizontal(|ui| {
            if ui.button("Delete the actor").clicked() {
                delete = true;
            }
            let t = ui.button("Make a template").on_hover_text("This actor with its events, to put down again anywhere (Select's Templates)");
            mark("uiTemplateActor", &t);
            template = t.clicked();
        });
        if a != self.map.actors[i] {
            if self.undo.last().map(|m| m.actors.get(i) != Some(&self.map.actors[i])).unwrap_or(true) {
                self.checkpoint();
            }
            let moved = a.at != self.map.actors[i].at || a.id != self.map.actors[i].id;
            self.map.actors[i] = a;
            self.dirty = true;
            if moved {
                self.rebuild_actors();
            }
        }
        if template {
            self.template_of_actor(i);
        }
        if delete {
            self.checkpoint();
            self.map.actors.remove(i);
            self.evt.actor_sel = None;
            self.evt.event_sel = None;
            self.rebuild_actors();
        }
    }

    /// The selected event in the inspector: its settings and its script. Says whether it
    /// filled the panel (else the usual inspector shows).
    pub(super) fn event_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(r) = self.evt.event_sel else { return false };
        let Some(mut e) = self.event(r).cloned() else {
            self.evt.event_sel = None;
            return false;
        };
        let owner = match r {
            EventRef::Map(_) => None,
            EventRef::Actor(a, _) => self.map.actors.get(a).map(|x| x.id.clone()),
        };
        ui.label(RichText::new(format!("Event {}", e.id)).size(16.0).strong());
        match &owner {
            Some(o) => hint(ui, &format!("On the actor {o}: spoken to, or approached.")),
            None => hint(ui, "On the map's cells."),
        }
        let mut delete = false;
        let mut run = false;
        section(ui, "When");
        ui.horizontal(|ui| {
            ui.label("Id");
            ui.add(egui::TextEdit::singleline(&mut e.id).desired_width(120.0));
        });
        ui.horizontal(|ui| {
            for (k, label) in [("interact", "spoken to"), ("enter", "stepped on"), ("start", "map start")] {
                let row = ui.selectable_label(e.trigger == k, label);
                mark(&format!("uiTrigger_{k}"), &row);
                if row.clicked() {
                    e.trigger = k.into();
                }
            }
        });
        if owner.is_none() && e.trigger != "start" {
            ui.horizontal(|ui| {
                ui.label(format!("Cells {},{} {}×{}", e.area[0], e.area[1], e.area[2], e.area[3]));
                let drag = ui.selectable_label(self.evt.placing == Some(Placing::EventArea(r)), "Drag them");
                if drag.clicked() {
                    self.evt.placing = if self.evt.placing == Some(Placing::EventArea(r)) { None } else { Some(Placing::EventArea(r)) };
                }
            });
        }
        let once = ui.checkbox(&mut e.once, "Once only");
        mark("uiOnce", &once);
        ui.horizontal(|ui| {
            ui.label("If flag");
            ui.add(egui::TextEdit::singleline(&mut e.when).desired_width(70.0));
            ui.label("unless");
            ui.add(egui::TextEdit::singleline(&mut e.unless).desired_width(70.0));
        });
        section(ui, "Script");
        let mut text = e.script.join("\n");
        let edit = ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(12).desired_width(f32::INFINITY));
        mark("uiScript", &edit);
        if edit.changed() {
            e.script = text.lines().map(String::from).collect();
        }
        let actor_ids: Vec<String> = self.map.actors.iter().map(|a| a.id.clone()).collect();
        let maps: Vec<String> = self.content.maps.iter().map(|m| m.id.clone()).collect();
        let encounters: Vec<String> = self.content.encounters.iter().map(|x| x.id.clone()).collect();
        let problems = script::problems(&e.script, &script::Known { actors: Some(&actor_ids), maps: Some(&maps), encounters: Some(&encounters) });
        for p in &problems {
            warn(ui, p);
        }
        crate::platform::set_hook("scriptproblems", &problems.len().to_string());
        hint(ui, "say Who: text · ask text -> label | text -> label · label/goto · if/unless flag goto · set/add/clear flag · move who x,z … · face who n|s|e|w|player · wait s · show/hide who · sound file · camera x,z|who · warp map x,z · battle id · end");
        ui.horizontal(|ui| {
            let r = ui.add_enabled(self.walker.is_some() && problems.is_empty(), egui::Button::new("Run it now")).on_disabled_hover_text("Walk the map first, with no mistakes in the script");
            mark("uiRunEvent", &r);
            if r.clicked() {
                run = true;
            }
            if ui.button("Delete").clicked() {
                delete = true;
            }
        });
        if Some(&e) != self.event(r) {
            let same_as_undo = self.undo.last().map(|m| {
                let old = match r {
                    EventRef::Map(i) => m.events.get(i),
                    EventRef::Actor(a, i) => m.actors.get(a).and_then(|x| x.events.get(i)),
                };
                old == self.event(r)
            });
            if !same_as_undo.unwrap_or(false) {
                self.checkpoint();
            }
            if let Some(slot) = self.event_mut(r) {
                *slot = e;
            }
            self.dirty = true;
        }
        if run {
            self.story.machine = None;
            self.story.dialog = None;
            self.run_event(r);
        }
        if delete {
            self.checkpoint();
            match r {
                EventRef::Map(i) => {
                    self.map.events.remove(i);
                }
                EventRef::Actor(a, i) => {
                    self.map.actors[a].events.remove(i);
                }
            }
            self.evt.event_sel = None;
        }
        true
    }
}

/// The facing someone has when first placed, for the record.
#[allow(dead_code)]
pub(super) fn default_facing() -> f32 {
    facing_of((0, 1))
}
