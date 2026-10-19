//! The map editor's panels (all but the top bar and the status line): each docked on a side of
//! the window (left, right, along the bottom) or floating in a window of its own, moved, sized,
//! folded, open or closed as the user likes. A docked one's header floats it or closes it; a
//! floating one's button docks it again, on the side its window is nearest. The top bar's
//! right end has a toggle for each, their names folding away to leave their icons; a toggle
//! right-clicked pings its panel, to find it on a busy screen (opened if it was closed): shaken
//! a moment, rings round it (in from a docked one's edges, lit a moment; out round a window).
//! The layout is kept (`platform::store_text`, "editor_layout") and comes back the next run.

use std::collections::BTreeMap;

use egui::{Color32, Id, Pos2, Rect, RichText, Stroke, Vec2};

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
}

/// Where a docked panel goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Side {
    Left,
    Right,
    Bottom,
}

/// How a panel is laid out: open or not; docked (on its side, so wide or so high) or floating
/// (its window's place and size).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct PaneState {
    pub open: bool,
    pub docked: bool,
    pub side: Side,
    /// docked: its width (left, right) or height (bottom)
    pub size: f32,
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
        PaneState { open: false, docked: false, side: Side::Right, size: 300.0, window: [300.0, 80.0, 420.0, 480.0], made: 0, ping: None }
    }
}

impl Pane {
    pub const ALL: [Pane; 8] = [Pane::Toolbox, Pane::Inspector, Pane::Assets, Pane::World, Pane::Timeline, Pane::Requests, Pane::Post, Pane::Abilities];

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
        }
    }

    /// How it first comes: the Toolbox on the left, the Inspector on the right and the
    /// Assets along the bottom, open; the rest closed, the windows where they always were.
    pub(super) fn first(self) -> PaneState {
        let docked = |open: bool, side: Side, size: f32| PaneState { open, docked: true, side, size, ..Default::default() };
        let floating = |side: Side, window: [f32; 4]| PaneState { side, window, ..Default::default() };
        match self {
            Pane::Toolbox => docked(true, Side::Left, 250.0),
            Pane::Inspector => docked(true, Side::Right, 300.0),
            Pane::Assets => docked(true, Side::Bottom, 300.0),
            Pane::World => docked(false, Side::Bottom, 320.0),
            Pane::Timeline => docked(false, Side::Bottom, 280.0),
            Pane::Requests => floating(Side::Right, [270.0, 60.0, 840.0, 760.0]),
            Pane::Post => floating(Side::Right, [880.0, 70.0, 340.0, 620.0]),
            Pane::Abilities => floating(Side::Bottom, [160.0, 60.0, 1000.0, 640.0]),
        }
    }

    /// The least its window may be.
    fn least_window(self) -> Vec2 {
        match self {
            Pane::Requests => Vec2::new(620.0, 420.0),
            Pane::Abilities => Vec2::new(520.0, 320.0),
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
                let at = match (s.open, s.docked, s.side) {
                    (false, ..) => "-",
                    (true, false, _) => "float",
                    (true, true, Side::Left) => "left",
                    (true, true, Side::Right) => "right",
                    (true, true, Side::Bottom) => "bottom",
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

/// A docked panel's inside: its header (its name; float it; close it) over what `add` puts in
/// it. Whether float or close was clicked.
fn docked_inside(ui: &mut egui::Ui, pane: Pane, title: &str, add: impl FnOnce(&mut egui::Ui)) -> (bool, bool) {
    let mut asked = (false, false);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = icon_button(ui, egui::include_image!("../../icons/close.svg"), "Close it (its toggle in the top bar opens it again)", &format!("uiPaneClose_{}", pane.key()));
            let float = icon_button(ui, egui::include_image!("../../icons/float.svg"), "Float it in a window of its own", &format!("uiPaneFloat_{}", pane.key()));
            asked = (float.clicked(), close.clicked());
        });
    });
    ui.separator();
    add(ui);
    asked
}

/// `pane` as its state says (nothing when it's closed): docked, a panel on its side, its
/// header (its name; float it; close it) over what `add` puts in it; floating, a window (its
/// title bar folds it and closes it) with a button to dock it again. The state follows what
/// the user does: sizes and places, floated and docked. Pinged, it shakes a moment, rings
/// round it (coming in from a docked one's edges, going out round a window).
pub(super) fn show(ui: &mut egui::Ui, st: &mut PaneState, pane: Pane, title: &str, add: impl FnOnce(&mut egui::Ui)) {
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
            Side::Left => egui::Panel::left(id),
            Side::Right => egui::Panel::right(id),
            Side::Bottom => egui::Panel::bottom(id),
        };
        let shake = shaken(ping);
        let r = panel.resizable(true).default_size(st.size).min_size(pane.least()).show(ui, |ui| {
            // pinged: what's in it shaken side to side a moment (the panel itself stays)
            if shake != 0.0 {
                let moved = ui.max_rect().translate(Vec2::new(shake, 0.0));
                return ui.scope_builder(egui::UiBuilder::new().max_rect(moved), |ui| docked_inside(ui, pane, title, add)).inner;
            }
            docked_inside(ui, pane, title, add)
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
        let (float, close) = r.inner;
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
                    dock = icon_button(ui, egui::include_image!("../../icons/dock.svg"), "Dock it again (on the side of the window it's nearest)", &format!("uiPaneDock_{}", pane.key())).clicked();
                });
                add(ui);
                (size, dock)
            });
        if let Some(r) = r {
            let rect = r.response.rect;
            if let Some(t) = ping {
                rings(ui.ctx(), rect.translate(Vec2::new(shake, 0.0)), t, false);
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
/// before them folds the names away or brings them back. What each toggle's button was, for
/// more on it (Requests' count).
pub(super) fn toggles(ui: &mut egui::Ui, row: Rect, dock: &mut Dock) -> BTreeMap<Pane, egui::Response> {
    let mut out = BTreeMap::new();
    // from the right: the last first
    let r = ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for p in Pane::ALL.into_iter().rev() {
            let open = dock.is_open(p);
            let image = egui::Image::new(p.icon()).fit_to_exact_size(Vec2::splat(15.0)).tint(ui.visuals().text_color());
            let button = if dock.words { egui::Button::image_and_text(image, p.title()) } else { egui::Button::image(image) };
            let r = ui.add(button.selected(open));
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
}
