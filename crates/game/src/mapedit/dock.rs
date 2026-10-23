//! The map editor's panels (all but the top bar and the status line): each docked on a side of
//! the window (left, right, along the bottom) or floating in a window of its own, moved, sized,
//! folded, open or closed as the user likes. A docked one's header floats it or closes it, and
//! drags it: let go on a side of the view (the targets that come up on its three sides, or the
//! panels docked there already) it docks there, innermost; let go anywhere else it floats where
//! it's dropped. A window dragged by its title bar docks the same way, let go on a side; its
//! button docks it on the side its window is nearest. The top bar's right end has a toggle for
//! each, their names folding away to leave their icons; a toggle right-clicked pings its panel,
//! to find it on a busy screen (opened if it was closed): shaken a moment, rings round it (in
//! from a docked one's edges, lit a moment; out round a window). The layout is kept
//! (`platform::store_text`, "editor_layout") and comes back the next run.

use std::collections::BTreeMap;

use egui::{Color32, CursorIcon, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::mark;
use crate::platform;

/// The name the layout is kept under.
const LAYOUT_NAME: &str = "editor_layout";
/// How long after a change the layout is kept (moving a window changes it all the way).
const KEEP_AFTER_MS: f64 = 600.0;
/// A ping: how long in all (seconds), how long a floating panel shakes, its rings.
const PING_S: f64 = 0.95;
const SHAKE_S: f64 = 0.35;
const RINGS: usize = 3;
/// How far in from the view's edges a panel let go docks (where the targets come up).
const ZONE: f32 = 72.0;
/// A target's size.
const TARGET: f32 = 44.0;

/// One of the editor's panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Pane {
    Toolbox,
    Inspector,
    Assets,
    World,
    Timeline,
    Requests,
    Post,
    Abilities,
    Builds,
}

/// Where a docked panel goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Side {
    Left,
    Right,
    Bottom,
}

/// How a panel is laid out: open or not; docked (on its side, so wide or so high, in its place
/// among the side's) or floating (its window's place and size).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct PaneState {
    pub open: bool,
    pub docked: bool,
    pub side: Side,
    /// docked: its width (left, right) or height (bottom)
    pub size: f32,
    /// docked: its place among the panels on its side (the lower the further out, by the
    /// window's edge; one dropped on a side goes innermost, by the view)
    pub order: u32,
    /// floating: its window's place and size (x, y, width, height)
    pub window: [f32; 4],
    /// floated again: its window made anew there (egui keeps a window where it last was)
    #[serde(skip)]
    pub made: u32,
    /// pinged (its toggle right-clicked) at this time (egui's, in seconds)
    #[serde(skip)]
    pub ping: Option<f64>,
}

impl Default for PaneState {
    fn default() -> Self {
        PaneState { open: false, docked: false, side: Side::Right, size: 300.0, order: 0, window: [300.0, 80.0, 420.0, 480.0], made: 0, ping: None }
    }
}

impl Pane {
    pub const ALL: [Pane; 9] = [Pane::Toolbox, Pane::Inspector, Pane::Assets, Pane::World, Pane::Timeline, Pane::Requests, Pane::Post, Pane::Abilities, Pane::Builds];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Toolbox => "Toolbox",
            Pane::Inspector => "Inspector",
            Pane::Assets => "Assets",
            Pane::World => "World",
            Pane::Timeline => "Timeline",
            Pane::Requests => "Requests",
            Pane::Post => "Post-processing",
            Pane::Abilities => "Abilities",
            Pane::Builds => "Builds",
        }
    }

    /// Its name in marks and ids ("toolbox").
    pub fn key(self) -> &'static str {
        match self {
            Pane::Toolbox => "toolbox",
            Pane::Inspector => "inspector",
            Pane::Assets => "assets",
            Pane::World => "world",
            Pane::Timeline => "timeline",
            Pane::Requests => "requests",
            Pane::Post => "post",
            Pane::Abilities => "abilities",
            Pane::Builds => "builds",
        }
    }

    /// Its top bar toggle's mark, for the checks.
    pub fn mark(self) -> &'static str {
        match self {
            Pane::Toolbox => "uiToolbox",
            Pane::Inspector => "uiInspector",
            Pane::Assets => "uiAssets",
            Pane::World => "uiWorld",
            Pane::Timeline => "uiTimeline",
            Pane::Requests => "uiRequests",
            Pane::Post => "uiPost",
            Pane::Abilities => "uiAbilities",
            Pane::Builds => "uiBuilds",
        }
    }

    /// What its toggle's tip says.
    fn about(self) -> &'static str {
        match self {
            Pane::Toolbox => "The tools (Select, Ground, Place, Trees, Build, Zones, Events) and what they work with",
            Pane::Inspector => "What's selected, and the map's own settings",
            Pane::Assets => "Every prop by folder; drop .glb files on the window to add them",
            Pane::World => "Maps stitched into a world: place this map next to others",
            Pane::Timeline => "An ability's timeline: its clips, played and edited (an ability's Timeline opens it on that one)",
            Pane::Requests => "Pictures of creatures and props for Claude to model in Blender: drop PNG, JPG or WebP files on the window while it's open",
            Pane::Post => "The toon look: how what's toon is lit, and its ink lines; the clouds, the wind",
            Pane::Abilities => "Every ability: make one, change what it does, how it grows and what it becomes, and its timeline",
            Pane::Builds => "Build the game into a folder: the desktop program or the site, without the terminal",
        }
    }

    fn icon(self) -> egui::ImageSource<'static> {
        match self {
            Pane::Toolbox => egui::include_image!("../../icons/toolbox.svg"),
            Pane::Inspector => egui::include_image!("../../icons/inspector.svg"),
            Pane::Assets => egui::include_image!("../../icons/assets.svg"),
            Pane::World => egui::include_image!("../../icons/world.svg"),
            Pane::Timeline => egui::include_image!("../../icons/timeline.svg"),
            Pane::Requests => egui::include_image!("../../icons/requests.svg"),
            Pane::Post => egui::include_image!("../../icons/post.svg"),
            Pane::Abilities => egui::include_image!("../../icons/abilities.svg"),
            Pane::Builds => egui::include_image!("../../icons/builds.svg"),
        }
    }

    /// How it first comes: the Toolbox on the left, the Inspector on the right and the
    /// Assets along the bottom, open; the rest closed, the windows where they always were;
    /// each in its place among its side's as they're listed.
    pub(super) fn first(self) -> PaneState {
        let order = Pane::ALL.iter().position(|&p| p == self).unwrap_or(0) as u32;
        let docked = |open: bool, side: Side, size: f32| PaneState { open, docked: true, side, size, order, ..Default::default() };
        let floating = |side: Side, window: [f32; 4]| PaneState { side, window, order, ..Default::default() };
        match self {
            Pane::Toolbox => docked(true, Side::Left, 250.0),
            Pane::Inspector => docked(true, Side::Right, 300.0),
            Pane::Assets => docked(true, Side::Bottom, 300.0),
            Pane::World => docked(false, Side::Bottom, 320.0),
            Pane::Timeline => docked(false, Side::Bottom, 280.0),
            Pane::Requests => floating(Side::Right, [270.0, 60.0, 840.0, 760.0]),
            Pane::Post => floating(Side::Right, [880.0, 70.0, 340.0, 620.0]),
            Pane::Abilities => floating(Side::Bottom, [160.0, 60.0, 1000.0, 640.0]),
            Pane::Builds => floating(Side::Right, [700.0, 80.0, 440.0, 560.0]),
        }
    }

    /// The least its window may be.
    fn least_window(self) -> Vec2 {
        match self {
            Pane::Requests => Vec2::new(620.0, 420.0),
            Pane::Abilities => Vec2::new(520.0, 320.0),
            Pane::Builds => Vec2::new(380.0, 300.0),
            _ => Vec2::new(220.0, 140.0),
        }
    }

    /// The least it may be docked (across its side).
    fn least(self) -> f32 {
        match self {
            Pane::Toolbox => 200.0,
            Pane::Inspector => 240.0,
            Pane::Requests => 420.0,
            Pane::Post => 300.0,
            _ => 140.0,
        }
    }
}

/// The editor's layout: each panel's, and whether the top bar's toggles have their names.
pub(super) struct Dock {
    panes: BTreeMap<Pane, PaneState>,
    pub words: bool,
    /// how wide the top bar's toggles were last frame (the rest of it wraps in what's left)
    pub toggles_width: f32,
    /// what was last kept, and when it changed since
    kept: String,
    changed: Option<f64>,
}

/// What's kept of it.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Kept {
    panes: BTreeMap<Pane, PaneState>,
    words: Option<bool>,
}

impl Default for Dock {
    /// As the editor first comes.
    fn default() -> Self {
        let mut d = Dock { panes: Pane::ALL.iter().map(|&p| (p, p.first())).collect(), words: true, toggles_width: 640.0, kept: String::new(), changed: None };
        d.kept = d.text();
        d
    }
}

impl Dock {
    /// The layout as it was left (what isn't kept, as it first comes).
    pub fn load() -> Dock {
        let mut d = Dock::default();
        if let Some(k) = platform::stored_text(LAYOUT_NAME).and_then(|t| serde_json::from_str::<Kept>(&t).ok()) {
            d.panes.extend(k.panes);
            d.words = k.words.unwrap_or(true);
        }
        d.kept = d.text();
        d
    }

    pub fn get(&self, p: Pane) -> PaneState {
        self.panes.get(&p).copied().unwrap_or_else(|| p.first())
    }

    pub fn set(&mut self, p: Pane, s: PaneState) {
        self.panes.insert(p, s);
    }

    pub fn is_open(&self, p: Pane) -> bool {
        self.get(p).open
    }

    pub fn set_open(&mut self, p: Pane, open: bool) {
        let mut s = self.get(p);
        s.open = open;
        self.set(p, s);
    }

    /// The order the panels are shown in: those docked along the bottom first (across the
    /// window, under the side ones), then the sides' (on each side the outermost first: as
    /// they were docked, the last dropped there innermost), then the rest (floating or
    /// closed) as they come.
    pub fn sequence(&self) -> Vec<Pane> {
        let mut docked: Vec<(bool, u32, Pane)> = Pane::ALL.into_iter().map(|p| (p, self.get(p))).filter(|(_, s)| s.open && s.docked).map(|(p, s)| (s.side != Side::Bottom, s.order, p)).collect();
        docked.sort();
        let docked: Vec<Pane> = docked.into_iter().map(|(.., p)| p).collect();
        let rest = Pane::ALL.into_iter().filter(|p| !docked.contains(p));
        docked.iter().copied().chain(rest).collect()
    }

    /// A drag going on, settled at the frame's end (`view`: what's between the docked
    /// panels, where the targets come up): the targets on the view's three sides, the one
    /// the pointer is over lit and where the panel would go shaded, the panel's name by the
    /// pointer; let go, the panel where it lands (`landed`). The side under the pointer is in
    /// a hook ("dockzone") for the checks.
    pub fn settle(&mut self, ctx: &egui::Context, view: Rect) {
        let Some(d) = dragging(ctx) else { return };
        let at = ctx.pointer_latest_pos();
        let zone = at.and_then(|at| zone_at(view, at));
        platform::set_hook("dockzone", zone.map_or("", side_name));
        if d.dropped {
            if let Some(at) = at {
                let order = self.panes.values().map(|s| s.order).max().unwrap_or(0) + 1;
                let mut st = self.get(d.pane);
                landed(&mut st, d.pane, d, at, view, ctx.content_rect(), order);
                if st.docked {
                    forget_size(ctx, d.pane, st.side);
                }
                self.set(d.pane, st);
            }
            set_dragging(ctx, None);
            return;
        }
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("pane-dock-zones")));
        if let Some(side) = zone {
            let size = docked_size(d.pane, side, d.from, view);
            p.rect(preview(view, side, size), 4.0, Color32::from_rgba_unmultiplied(120, 180, 255, 40), Stroke::new(1.5, Color32::from_rgba_unmultiplied(150, 205, 255, 200)), egui::StrokeKind::Inside);
        }
        for side in [Side::Left, Side::Right, Side::Bottom] {
            let lit = zone == Some(side);
            let t = target(view, side);
            let (fill, line) = if lit { (Color32::from_rgb(60, 120, 200), Color32::from_rgb(200, 230, 255)) } else { (Color32::from_rgba_unmultiplied(22, 30, 44, 235), Color32::from_gray(160)) };
            p.rect(t, 7.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
            // the window, the panel on its side
            let g = Rect::from_center_size(t.center(), Vec2::new(26.0, 19.0));
            p.rect_stroke(g, 2.5, Stroke::new(1.5, line), egui::StrokeKind::Inside);
            let strip = match side {
                Side::Left => Rect::from_min_size(g.min, Vec2::new(9.0, g.height())),
                Side::Right => Rect::from_min_max(Pos2::new(g.right() - 9.0, g.top()), g.max),
                Side::Bottom => Rect::from_min_max(Pos2::new(g.left(), g.bottom() - 7.0), g.max),
            };
            p.rect_filled(strip.shrink(1.5), 1.0, line);
        }
        if let Some(at) = at {
            egui::Area::new(Id::new("pane-drag-ghost")).order(egui::Order::Tooltip).fixed_pos(at + Vec2::new(16.0, 14.0)).interactable(false).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(d.pane.icon()).fit_to_exact_size(Vec2::splat(15.0)).tint(ui.visuals().text_color()));
                        ui.label(RichText::new(d.pane.title()).strong());
                    });
                });
            });
        }
        ctx.set_cursor_icon(CursorIcon::Grabbing);
        ctx.request_repaint();
    }

    /// What it's kept as.
    fn text(&self) -> String {
        serde_json::to_string(&Kept { panes: self.panes.clone(), words: Some(self.words) }).unwrap_or_default()
    }

    /// Kept, a little after it last changed (and its marks for the checks set).
    pub fn keep(&mut self, now: f64) {
        let text = self.text();
        let shown: Vec<String> = Pane::ALL
            .iter()
            .map(|&p| {
                let s = self.get(p);
                let at = match (s.open, s.docked) {
                    (false, _) => "-",
                    (true, false) => "float",
                    (true, true) => side_name(s.side),
                };
                format!("{}={at}", p.key())
            })
            .collect();
        platform::set_hook("layout", &shown.join(" "));
        platform::set_hook("layoutwords", if self.words { "1" } else { "" });
        if text == self.kept {
            self.changed = None;
            return;
        }
        let since = *self.changed.get_or_insert(now);
        if now - since >= KEEP_AFTER_MS {
            if let Err(e) = platform::store_text(LAYOUT_NAME, &text) {
                log::warn!("the editor's layout wasn't kept: {e}");
            }
            self.kept = text;
            self.changed = None;
        }
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
        Side::Bottom => "bottom",
    }
}

/// A docked panel's id on `side`: one a side, so egui's own memory of its size (kept by id)
/// never carries its width along the bottom over to a side.
fn panel_id(pane: Pane, side: Side) -> Id {
    Id::new(("editor-pane", pane.key(), side_name(side)))
}

/// egui's memory of a docked panel's size forgotten, so it comes as big as its state says
/// (`PaneState::size`: docked anew, as big across as it was where it came from).
fn forget_size(ctx: &egui::Context, pane: Pane, side: Side) {
    ctx.data_mut(|d| d.remove::<egui::containers::panel::PanelState>(panel_id(pane, side)));
}

/// A panel taken hold of (a docked one by its header, a window by its title bar): which, what
/// it's dragged from (the panel's rect, the window's), whether it was floating; `dropped` the
/// frame it's let go. In the context's data while it lasts.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    pane: Pane,
    from: Rect,
    floating: bool,
    dropped: bool,
}

fn drag_id() -> Id {
    Id::new("pane-drag")
}

fn dragging(ctx: &egui::Context) -> Option<Drag> {
    ctx.data(|d| d.get_temp(drag_id()))
}

fn set_dragging(ctx: &egui::Context, d: Option<Drag>) {
    ctx.data_mut(|data| match d {
        Some(d) => {
            data.insert_temp(drag_id(), d);
        }
        None => data.remove::<Drag>(drag_id()),
    });
}

/// What a header's grip or a window's title bar says of its drag: taken hold of (a drag
/// begins, from `from`), let go (dropped: settled at the frame's end).
fn follow(ctx: &egui::Context, r: &egui::Response, pane: Pane, from: Rect, floating: bool) {
    if r.drag_started() {
        set_dragging(ctx, Some(Drag { pane, from, floating, dropped: false }));
    } else if r.drag_stopped() {
        if let Some(d) = dragging(ctx).filter(|d| d.pane == pane) {
            set_dragging(ctx, Some(Drag { dropped: true, ..d }));
        }
    }
}

/// The side a panel let go at `at` docks on: over the panels docked there already (left or
/// right of the view, under it), or within `ZONE` of the view's edge (the nearest); none over
/// the view's middle or the top bar.
fn zone_at(view: Rect, at: Pos2) -> Option<Side> {
    if at.y < view.top() {
        return None;
    }
    if at.y > view.bottom() {
        return Some(Side::Bottom);
    }
    if at.x < view.left() {
        return Some(Side::Left);
    }
    if at.x > view.right() {
        return Some(Side::Right);
    }
    let near = [(at.x - view.left(), Side::Left), (view.right() - at.x, Side::Right), (view.bottom() - at.y, Side::Bottom)];
    near.into_iter().filter(|(d, _)| *d < ZONE).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, s)| s)
}

/// Where a side's target is drawn: in from the middle of the view's edge.
fn target(view: Rect, side: Side) -> Rect {
    let c = match side {
        Side::Left => Pos2::new(view.left() + ZONE / 2.0, view.center().y),
        Side::Right => Pos2::new(view.right() - ZONE / 2.0, view.center().y),
        Side::Bottom => Pos2::new(view.center().x, view.bottom() - ZONE / 2.0),
    };
    Rect::from_center_size(c, Vec2::splat(TARGET))
}

/// How big across its side a panel is once dropped there: as it was (a panel's width or
/// height, a window's), within half the view (along the bottom, well under it), not less than
/// its least.
fn docked_size(pane: Pane, side: Side, from: Rect, view: Rect) -> f32 {
    let (across, most) = match side {
        Side::Bottom => (from.height(), view.height() * 0.6),
        _ => (from.width(), view.width() * 0.5),
    };
    across.min(most).max(pane.least())
}

/// Where a panel docked on `side`, `size` across, would go (that much of the view).
fn preview(view: Rect, side: Side, size: f32) -> Rect {
    match side {
        Side::Left => Rect::from_min_size(view.left_top(), Vec2::new(size, view.height())),
        Side::Right => Rect::from_min_max(Pos2::new(view.right() - size, view.top()), view.right_bottom()),
        Side::Bottom => Rect::from_min_max(Pos2::new(view.left(), view.bottom() - size), view.right_bottom()),
    }
}

/// A panel let go at `at`: on a side (`zone_at`), docked there, innermost (`order`), as big
/// across it as it was; off the sides, a docked one floats where it's dropped (its title bar
/// under the pointer, as big as it was within the screen), a window stays where its drag left
/// it.
fn landed(st: &mut PaneState, pane: Pane, d: Drag, at: Pos2, view: Rect, screen: Rect, order: u32) {
    match zone_at(view, at) {
        Some(side) => {
            st.size = docked_size(pane, side, d.from, view);
            st.docked = true;
            st.side = side;
            st.order = order;
        }
        None if !d.floating => {
            let least = pane.least_window();
            let size = Vec2::new(d.from.width().min(screen.width() * 0.6).max(least.x), d.from.height().min(screen.height() * 0.8).max(least.y));
            let corner = at - Vec2::new(80.0, 14.0);
            st.window = [corner.x.clamp(screen.left(), (screen.right() - size.x).max(screen.left())), corner.y.clamp(screen.top(), (screen.bottom() - size.y).max(screen.top())), size.x, size.y];
            st.docked = false;
            st.made += 1;
        }
        None => {}
    }
}

/// A small icon button (a panel's header's, a window's dock): its mark for the checks.
fn icon_button(ui: &mut egui::Ui, icon: egui::ImageSource<'static>, tip: &str, name: &str) -> egui::Response {
    let image = egui::Image::new(icon).fit_to_exact_size(Vec2::splat(14.0)).tint(ui.visuals().text_color());
    let r = ui.add(egui::Button::image(image).frame(false)).on_hover_text(tip);
    mark(name, &r);
    r
}

/// The side a window is nearest: the left or right third of the screen, else the bottom.
fn nearest_side(window: Rect, screen: Rect) -> Side {
    let x = (window.center().x - screen.left()) / screen.width().max(1.0);
    if x < 1.0 / 3.0 {
        Side::Left
    } else if x > 2.0 / 3.0 {
        Side::Right
    } else {
        Side::Bottom
    }
}

/// A ping's rings round `rect` (`t` seconds in), over everything: going out from a window,
/// in from a docked panel's edges (the screen's edges would hide them going out).
fn rings(ctx: &egui::Context, rect: Rect, t: f64, inward: bool) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("pane-ping")));
    for k in 0..RINGS {
        let age = (t - k as f64 * 0.13) / (PING_S - 0.26);
        if !(0.0..1.0).contains(&age) {
            continue;
        }
        let ease = 1.0 - (1.0 - age as f32).powi(3);
        let alpha = ((1.0 - age as f32) * 220.0) as u8;
        let reach = 3.0 + 34.0 * ease;
        let ring = if inward { rect.shrink(reach.min(rect.width().min(rect.height()) * 0.45)) } else { rect.expand(reach) };
        p.rect_stroke(ring, 10.0 + 12.0 * ease, Stroke::new(2.5 - 1.2 * ease, Color32::from_rgba_unmultiplied(150, 205, 255, alpha)), egui::StrokeKind::Outside);
    }
}

/// How far a pinged panel is shaken just now, side to side (0 once it's done).
fn shaken(ping: Option<f64>) -> f32 {
    ping.filter(|&t| t < SHAKE_S).map_or(0.0, |t| ((t * 42.0).sin() * 7.0 * (1.0 - t / SHAKE_S)) as f32)
}

/// What a docked panel's header was asked: float it, close it; its grip (the drag).
struct Header {
    float: bool,
    close: bool,
    grip: egui::Response,
}

/// A grip (two columns of dots) where a docked panel's header is taken hold of.
fn grip_dots(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 14.0), Sense::hover());
    let c = ui.visuals().weak_text_color();
    for x in 0..2 {
        for y in 0..3 {
            ui.painter().circle_filled(rect.left_top() + Vec2::new(2.5 + x as f32 * 5.0, 2.0 + y as f32 * 5.0), 1.3, c);
        }
    }
}

/// A docked panel's inside: its header (its grip and name, which drag it; float it; close it)
/// over what `add` puts in it.
fn docked_inside(ui: &mut egui::Ui, pane: Pane, title: &str, header: impl FnOnce(&mut egui::Ui), add: impl FnOnce(&mut egui::Ui)) -> Header {
    let mut asked = (false, false);
    let mut buttons = f32::MAX;
    let row = ui.horizontal(|ui| {
        grip_dots(ui);
        ui.label(RichText::new(title).strong());
        header(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = icon_button(ui, egui::include_image!("../../icons/close.svg"), "Close it (its toggle in the top bar opens it again)", &format!("uiPaneClose_{}", pane.key()));
            let float = icon_button(ui, egui::include_image!("../../icons/float.svg"), "Float it in a window of its own", &format!("uiPaneFloat_{}", pane.key()));
            asked = (float.clicked(), close.clicked());
            buttons = float.rect.left();
        });
    });
    // the header short of its buttons takes hold of the panel (a click there does nothing:
    // the drag begins once the pointer has moved)
    let handle = Rect::from_min_max(row.response.rect.min, Pos2::new((buttons - 6.0).max(row.response.rect.min.x + 1.0), row.response.rect.max.y));
    let grip = ui.interact(handle, Id::new(("pane-grip", pane.key())), Sense::click_and_drag()).on_hover_cursor(CursorIcon::Grab).on_hover_text("Drag it to a side of the view to dock it there, or anywhere else to float it");
    mark(&format!("uiPaneGrip_{}", pane.key()), &grip);
    ui.separator();
    add(ui);
    Header { float: asked.0, close: asked.1, grip }
}

/// `pane` as its state says (nothing when it's closed): docked, a panel on its side, its
/// header (its name; float it; close it; dragged, the panel goes with it) over what `add` puts
/// in it; floating, a window (its title bar folds it, closes it and drags it) with a button to
/// dock it again. The state follows what the user does: sizes and places, floated and docked
/// (a drag lands at the frame's end: `Dock::settle`). Pinged, it shakes a moment, rings round
/// it (coming in from a docked one's edges, going out round a window).
pub(super) fn show(ui: &mut egui::Ui, st: &mut PaneState, pane: Pane, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    show_with(ui, st, pane, title, |_| {}, add);
}

/// As `show`, with widgets of the pane's own in its header, by its name (`header`: a button
/// or two; floating, in the row with the dock button).
pub(super) fn show_with(ui: &mut egui::Ui, st: &mut PaneState, pane: Pane, title: &str, header: impl FnOnce(&mut egui::Ui), add: impl FnOnce(&mut egui::Ui)) {
    if !st.open {
        return;
    }
    let id = Id::new(("editor-pane", pane.key()));
    let now = ui.input(|i| i.time);
    let ping = st.ping.map(|t0| now - t0).filter(|&t| (0.0..PING_S).contains(&t));
    if ping.is_none() {
        st.ping = None;
    } else {
        ui.ctx().request_repaint();
        platform::set_hook("ping", pane.key());
    }
    if st.docked {
        let panel = match st.side {
            Side::Left => egui::Panel::left(panel_id(pane, st.side)),
            Side::Right => egui::Panel::right(panel_id(pane, st.side)),
            Side::Bottom => egui::Panel::bottom(panel_id(pane, st.side)),
        };
        let shake = shaken(ping);
        let r = panel.resizable(true).default_size(st.size).min_size(pane.least()).show(ui, |ui| {
            // pinged: what's in it shaken side to side a moment (the panel itself stays)
            if shake != 0.0 {
                let moved = ui.max_rect().translate(Vec2::new(shake, 0.0));
                return ui.scope_builder(egui::UiBuilder::new().max_rect(moved), |ui| docked_inside(ui, pane, title, header, add)).inner;
            }
            docked_inside(ui, pane, title, header, add)
        });
        let rect = r.response.rect;
        st.size = if st.side == Side::Bottom { rect.height() } else { rect.width() };
        if let Some(t) = ping {
            // lit a moment, its edge too, its rings coming in from its edges
            let flash = (1.0 - t / PING_S) as f32;
            let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("pane-ping")));
            p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(120, 180, 255, (flash * flash * 46.0) as u8));
            p.rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(2.0, Color32::from_rgba_unmultiplied(150, 205, 255, (flash * 255.0) as u8)), egui::StrokeKind::Inside);
            rings(ui.ctx(), rect, t, true);
        }
        let Header { float, close, grip } = r.inner;
        follow(ui.ctx(), &grip, pane, rect, false);
        if float {
            // its window where it was docked, as big but not the screen's width (a panel
            // along the bottom is), centred there: docked again at once, it goes back
            let screen = ui.ctx().content_rect();
            let size = Vec2::new(rect.width().min(screen.width() * 0.6), rect.height().min(screen.height() * 0.8));
            let corner = rect.center() - size / 2.0;
            st.window = [corner.x.clamp(screen.left(), screen.right() - size.x), corner.y.clamp(screen.top(), screen.bottom() - size.y), size.x, size.y];
            st.docked = false;
            st.made += 1;
        }
        if close {
            st.open = false;
        }
    } else {
        let mut open = true;
        let window_id = id.with(("window", st.made));
        // pinged: shaken side to side a moment (its picture only: it stays where it is)
        let shake = shaken(ping);
        ui.ctx().set_transform_layer(egui::LayerId::new(egui::Order::Middle, window_id), egui::emath::TSTransform::from_translation(Vec2::new(shake, 0.0)));
        let r = egui::Window::new(title)
            .id(window_id)
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .min_size(pane.least_window())
            .default_pos(Pos2::new(st.window[0], st.window[1]))
            .default_size(Vec2::new(st.window[2], st.window[3]))
            .show(ui.ctx(), |ui| {
                let size = ui.available_size();
                let mut dock = false;
                // its own row, as high as the button (a layout of its own would take all the
                // height there is)
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 18.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    dock = icon_button(ui, egui::include_image!("../../icons/dock.svg"), "Dock it again (on the side of the window it's nearest; or drag it by its title bar onto a side)", &format!("uiPaneDock_{}", pane.key())).clicked();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), header);
                });
                add(ui);
                (size, dock)
            });
        if let Some(r) = r {
            let rect = r.response.rect;
            if let Some(t) = ping {
                rings(ui.ctx(), rect.translate(Vec2::new(shake, 0.0)), t, false);
            }
            // moved by its title bar (what egui's window takes the drag on: its widget's id
            // is egui's own), it may be dropped on a side
            if let Some(title) = ui.ctx().read_response(window_id.with("__title_click")) {
                follow(ui.ctx(), &title, pane, rect, true);
            }
            st.window[0] = rect.left();
            st.window[1] = rect.top();
            // folded, it says nothing of its size
            if let Some((size, dock)) = r.inner {
                st.window[2] = size.x.max(120.0);
                st.window[3] = size.y.max(80.0);
                if dock {
                    st.docked = true;
                    st.side = nearest_side(rect, ui.ctx().content_rect());
                    forget_size(ui.ctx(), pane, st.side);
                }
            }
        }
        if !open {
            st.open = false;
        }
    }
}

/// The top bar's toggles for the panels, at the right end of `row` (its first row: the rest of
/// it wraps in what's left): each its icon and, while they have them, its name; a chevron
/// before them folds the names away or brings them back; those `locked` greyed out, saying
/// why (Requests, with no Claude sign-in). What each toggle's button was, for more on it
/// (Requests' count).
pub(super) fn toggles(ui: &mut egui::Ui, row: Rect, dock: &mut Dock, locked: &[(Pane, &str)]) -> BTreeMap<Pane, egui::Response> {
    let mut out = BTreeMap::new();
    // at the right end, in a border as the top bar's other groups are; its row runs right to
    // left as the scope does: the last panel first, the fold last (at its left)
    let r = ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        super::group(ui, "uiGroupPanels", |ui| {
            for p in Pane::ALL.into_iter().rev() {
                let open = dock.is_open(p);
                let image = egui::Image::new(p.icon()).fit_to_exact_size(Vec2::splat(15.0)).tint(ui.visuals().text_color());
                let button = if dock.words { egui::Button::image_and_text(image, p.title()) } else { egui::Button::image(image) };
                let why = locked.iter().find(|(l, _)| *l == p).map(|(_, why)| *why);
                let r = ui.add_enabled(why.is_none(), button.selected(open && why.is_none()));
                let r = match why {
                    Some(why) => r.on_disabled_hover_text(why),
                    None => r,
                };
                // (Requests says more on its own: how many wait, what's being modelled)
                let r = if p == Pane::Requests { r } else { r.on_hover_text(format!("{}{}\nRight-click: show where it is", if dock.words { String::new() } else { format!("{}: ", p.title()) }, p.about())) };
                mark(p.mark(), &r);
                if r.clicked() {
                    dock.set_open(p, !open);
                }
                // right-clicked: pinged (opened first if it was closed), to see where it is
                if r.secondary_clicked() {
                    let mut s = dock.get(p);
                    s.open = true;
                    s.ping = Some(ui.input(|i| i.time));
                    dock.set(p, s);
                }
                out.insert(p, r);
            }
            let fold = ui.add(egui::Button::new(if dock.words { "‹" } else { "›" }).frame(false)).on_hover_text(if dock.words { "Only the panels' icons" } else { "The panels' names too" });
            mark("uiPanesFold", &fold);
            if fold.clicked() {
                dock.words = !dock.words;
            }
        });
    });
    dock.toggles_width = r.response.rect.width();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As it first comes: the Toolbox left, the Inspector right, the Assets along the bottom,
    /// the rest closed. What's kept reads back as it was; what's missing from it, as it first
    /// comes; a window docks on the side it's nearest.
    #[test]
    fn the_layout_comes_as_it_was_left() {
        let d = Dock::default();
        let docked = |p: Pane| (d.get(p).open, d.get(p).docked, d.get(p).side);
        assert_eq!(docked(Pane::Toolbox), (true, true, Side::Left));
        assert_eq!(docked(Pane::Inspector), (true, true, Side::Right));
        assert_eq!(docked(Pane::Assets), (true, true, Side::Bottom));
        assert!(Pane::ALL[3..].iter().all(|&p| !d.is_open(p)), "the rest closed");
        let mut changed = Dock::default();
        changed.set(Pane::Inspector, PaneState { docked: false, window: [10.0, 20.0, 300.0, 400.0], ..changed.get(Pane::Inspector) });
        changed.words = false;
        let kept: Kept = serde_json::from_str(&changed.text()).unwrap();
        assert_eq!(kept.panes[&Pane::Inspector].window, [10.0, 20.0, 300.0, 400.0]);
        assert_eq!(kept.words, Some(false));
        // a layout kept before a panel existed: that one as it first comes
        let old: Kept = serde_json::from_str(r#"{"panes":{"toolbox":{"open":false}}}"#).unwrap();
        assert!(!old.panes[&Pane::Toolbox].open && !old.panes.contains_key(&Pane::Assets));
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 900.0));
        let at = |x: f32| nearest_side(Rect::from_center_size(Pos2::new(x, 400.0), Vec2::splat(200.0)), screen);
        assert_eq!((at(200.0), at(750.0), at(1300.0)), (Side::Left, Side::Bottom, Side::Right));
    }

    /// Let go on a side (in from the view's edge, or over the panels docked there) a panel
    /// docks there innermost, as big across as it was; off the sides a docked one floats where
    /// it's dropped and a window stays; the panels show bottom first, then each side's
    /// outermost first.
    #[test]
    fn a_panel_dropped_docks_where_it_lands() {
        let view = Rect::from_min_max(Pos2::new(250.0, 40.0), Pos2::new(1100.0, 600.0));
        assert_eq!(zone_at(view, Pos2::new(280.0, 300.0)), Some(Side::Left));
        assert_eq!(zone_at(view, Pos2::new(100.0, 300.0)), Some(Side::Left), "over the panels docked there");
        assert_eq!(zone_at(view, Pos2::new(1080.0, 300.0)), Some(Side::Right));
        assert_eq!(zone_at(view, Pos2::new(600.0, 580.0)), Some(Side::Bottom));
        assert_eq!(zone_at(view, Pos2::new(600.0, 700.0)), Some(Side::Bottom), "under the view");
        assert_eq!(zone_at(view, Pos2::new(290.0, 590.0)), Some(Side::Bottom), "the nearer edge");
        assert_eq!(zone_at(view, Pos2::new(600.0, 300.0)), None, "the middle");
        assert_eq!(zone_at(view, Pos2::new(600.0, 20.0)), None, "the top bar");
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0));
        let mut d = Dock::default();
        assert_eq!(d.sequence()[..3], [Pane::Assets, Pane::Toolbox, Pane::Inspector]);
        // the Inspector (on the right, 300 wide) dropped on the left: there, inside the Toolbox
        let from = Rect::from_min_size(Pos2::new(1100.0, 40.0), Vec2::new(300.0, 560.0));
        let drag = |from, floating| Drag { pane: Pane::Inspector, from, floating, dropped: true };
        let mut st = d.get(Pane::Inspector);
        landed(&mut st, Pane::Inspector, drag(from, false), Pos2::new(280.0, 300.0), view, screen, 9);
        assert_eq!((st.docked, st.side, st.size, st.order), (true, Side::Left, 300.0, 9));
        d.set(Pane::Inspector, st);
        assert_eq!(d.sequence()[..3], [Pane::Assets, Pane::Toolbox, Pane::Inspector]);
        assert!(d.sequence().len() == Pane::ALL.len() && Pane::ALL.iter().all(|p| d.sequence().contains(p)), "every panel, once");
        // dropped along the bottom: as high as it was, within the view
        landed(&mut st, Pane::Inspector, drag(from, false), Pos2::new(600.0, 590.0), view, screen, 10);
        assert_eq!((st.side, st.size), (Side::Bottom, 336.0));
        d.set(Pane::Inspector, st);
        assert_eq!(d.sequence()[..3], [Pane::Assets, Pane::Inspector, Pane::Toolbox]);
        // torn off into the middle: a window there, its title bar under the pointer
        landed(&mut st, Pane::Inspector, drag(from, false), Pos2::new(600.0, 300.0), view, screen, 11);
        assert!(!st.docked);
        assert_eq!(st.window, [520.0, 286.0, 300.0, 560.0]);
        // a window let go in the middle stays one; on a side, docks there
        let window = Rect::from_min_size(Pos2::new(500.0, 200.0), Vec2::new(420.0, 480.0));
        let before = st;
        landed(&mut st, Pane::Inspector, drag(window, true), Pos2::new(650.0, 210.0), view, screen, 12);
        assert_eq!(st, before);
        landed(&mut st, Pane::Inspector, drag(window, true), Pos2::new(1090.0, 210.0), view, screen, 12);
        assert_eq!((st.docked, st.side, st.size, st.order), (true, Side::Right, 420.0, 12));
        // a layout kept before panels had their places: as they're listed
        let old: Kept = serde_json::from_str(r#"{"panes":{"inspector":{"open":true,"docked":true,"side":"left"}}}"#).unwrap();
        let mut d = Dock::default();
        d.panes.extend(old.panes);
        assert_eq!(d.sequence()[..3], [Pane::Assets, Pane::Toolbox, Pane::Inspector], "both at 0: the Toolbox first, as listed");
    }
}
