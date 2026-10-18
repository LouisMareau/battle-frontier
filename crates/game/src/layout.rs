//! The HUD's layout groups: rows of buttons anchored to a corner of the screen. A group
//! places what's in it itself, one after another from its corner (a new button is one more
//! line in its group), and keeps where each button is: with the game's buttons a ring goes
//! from one to the next (`step`), within a group and across them.
//!
//! The buttons are frosted glass: the world behind them blurred (the renderer's small blurred
//! copy of the picture, `set_frost`), a faint white veil over it, a white icon on it.

use std::sync::RwLock;

use egui::{Align2, Color32, Id, Order, Pos2, Rect, Response, Sense, Stroke, Vec2};

/// A button's side, the space round a group (from the screen's edges) and between two of
/// its buttons, how round the glass's corners are.
pub const BUTTON: f32 = 52.0;
pub const MARGIN: f32 = 18.0;
pub const GAP: f32 = 10.0;
pub const RADIUS: f32 = 16.0;
/// A hover's tween.
const HOVER_S: f32 = 0.15;

/// A group of the HUD: the corner it's anchored to. Its buttons go left to right from there
/// (a group in a right-hand corner ends at it).
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub id: &'static str,
    pub corner: Align2,
}

/// The groups the HUD has (one in another corner is one more of these).
pub const TOP_RIGHT: Group = Group { id: "top-right", corner: Align2::RIGHT_TOP };
pub const BOTTOM_LEFT: Group = Group { id: "bottom-left", corner: Align2::LEFT_BOTTOM };

/// A place in a group the game's buttons can be on: its name (the checks' mark too) and where
/// it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub name: String,
    pub rect: Rect,
}

/// A group as it's drawn this frame: what's added goes after what's there (a button, or
/// anything drawn in `ui`, its places given with `stop`).
pub struct Row<'a> {
    pub ui: &'a mut egui::Ui,
    /// its places, in order
    pub stops: Vec<Stop>,
    /// the buttons answer (else they're dimmed, and only shown)
    pub enabled: bool,
}

impl Group {
    /// The group, its buttons `add`ed in order from its corner. What `add` gives back, and its
    /// places.
    pub fn show<R>(self, ctx: &egui::Context, enabled: bool, add: impl FnOnce(&mut Row) -> R) -> (R, Vec<Stop>) {
        let x = if self.corner.x() == egui::Align::Max { -MARGIN } else { MARGIN };
        let y = if self.corner.y() == egui::Align::Max { -MARGIN } else { MARGIN };
        egui::Area::new(Id::new(("hud", self.id)))
            .anchor(self.corner, [x, y])
            .order(Order::Foreground)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(GAP);
                ui.horizontal(|ui| {
                    let mut row = Row { ui, stops: vec![], enabled };
                    let out = add(&mut row);
                    (out, row.stops)
                })
                .inner
            })
            .inner
    }
}

impl Row<'_> {
    /// A frosted button with a white icon, `tip` when hovered; `mark` names its place (the
    /// checks find it by that: `data-<mark>`).
    pub fn button(&mut self, icon: egui::ImageSource<'static>, tip: &str, mark: &str) -> Response {
        let (rect, response) = self.ui.allocate_exact_size(Vec2::splat(BUTTON), if self.enabled { Sense::click() } else { Sense::hover() });
        let hover = self.ui.ctx().animate_bool_with_time(response.id, self.enabled && response.hovered(), HOVER_S);
        frosted(self.ui.painter(), rect, RADIUS, hover);
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(BUTTON * (0.48 + 0.05 * hover)));
        egui::Image::new(icon).fit_to_exact_size(Vec2::splat(64.0)).tint(Color32::from_white_alpha(if self.enabled { 255 } else { 110 })).paint_at(self.ui, icon_rect);
        self.stop(mark, rect);
        if self.enabled {
            response.on_hover_text(tip)
        } else {
            response
        }
    }

    /// A place of the row's own the game's buttons can be on, painted in `ui` by the caller (a
    /// creature in the Team's list).
    pub fn stop(&mut self, mark: &str, rect: Rect) {
        crate::platform::set_hook(mark, &format!("{:.0},{:.0}", rect.center().x, rect.center().y));
        self.stops.push(Stop { name: mark.to_string(), rect });
    }
}

/// The renderer's frosted copy of the picture this frame, and the part of the screen it was
/// drawn for (none: the glass is a plain dark veil).
static FROST: RwLock<Option<(egui::TextureId, Rect)>> = RwLock::new(None);

pub fn set_frost(frost: Option<(egui::TextureId, Rect)>) {
    if let Ok(mut f) = FROST.write() {
        *f = frost;
    }
}

/// Frosted glass over `rect`: a soft shadow under it, the picture behind it blurred (a little
/// darker, for the white on it), a faint white veil (brighter when `hover`ed, 0 to 1), a thin
/// light edge.
pub fn frosted(p: &egui::Painter, rect: Rect, radius: f32, hover: f32) {
    p.add(egui::Shadow { offset: [0, 3], blur: 14, spread: 0, color: Color32::from_black_alpha(55) }.as_shape(rect, radius));
    match FROST.read().ok().and_then(|f| *f) {
        Some((id, screen)) if screen.width() > 0.0 && screen.height() > 0.0 => {
            let at = |q: Pos2| Pos2::new((q.x - screen.min.x) / screen.width(), (q.y - screen.min.y) / screen.height());
            p.add(egui::epaint::RectShape::filled(rect, radius, Color32::from_gray(190)).with_texture(id, Rect::from_min_max(at(rect.min), at(rect.max))));
        }
        _ => {
            p.rect_filled(rect, radius, Color32::from_rgba_unmultiplied(28, 34, 48, 170));
        }
    }
    p.rect_filled(rect, radius, Color32::from_white_alpha((22.0 + 26.0 * hover) as u8));
    p.rect_stroke(rect, radius, Stroke::new(1.0, Color32::from_white_alpha((60.0 + 70.0 * hover) as u8)), egui::StrokeKind::Inside);
}

/// The ring round where the game's buttons are, over everything of the HUD's.
pub fn ring(ctx: &egui::Context, rect: Rect) {
    let p = ctx.layer_painter(egui::LayerId::new(Order::Tooltip, Id::new("hud-ring")));
    p.rect_stroke(rect.expand(4.0), RADIUS + 4.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Outside);
}

/// Where the ring goes from the place `from` with one of the four directions (`dir`: (1, 0)
/// right, (0, -1) up...): to the nearest place that way, the straighter ahead the nearer it
/// counts, in its group or another; none that way: it stays.
pub fn step(stops: &[Stop], from: usize, dir: Vec2) -> usize {
    let Some(here) = stops.get(from) else { return 0 };
    let c = here.rect.center();
    let score = |r: &Rect| {
        let d = r.center() - c;
        let along = d.dot(dir);
        let across = (d - dir * along).length();
        (along, along + across * 2.0)
    };
    stops
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != from && score(&s.rect).0 > 1.0)
        .min_by(|a, b| score(&a.1.rect).1.total_cmp(&score(&b.1.rect).1))
        .map_or(from, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(name: &str, x: f32, y: f32) -> Stop {
        Stop { name: name.into(), rect: Rect::from_center_size(Pos2::new(x, y), Vec2::splat(BUTTON)) }
    }

    /// The overworld's HUD on a 1280 by 720 screen: Settings and Exit top right, the Team
    /// bottom left with its list open (three creatures and View all).
    #[test]
    fn the_ring_goes_to_the_nearest_place_that_way() {
        let stops = vec![
            stop("settings", 1148.0, 44.0),
            stop("exit", 1210.0, 44.0),
            stop("team", 44.0, 676.0),
            stop("one", 110.0, 676.0),
            stop("two", 160.0, 676.0),
            stop("three", 210.0, 676.0),
            stop("all", 260.0, 676.0),
        ];
        let right = Vec2::new(1.0, 0.0);
        let left = Vec2::new(-1.0, 0.0);
        let up = Vec2::new(0.0, -1.0);
        let down = Vec2::new(0.0, 1.0);
        // along a group, both ways, stopping at its ends
        assert_eq!(step(&stops, 0, right), 1);
        assert_eq!(step(&stops, 1, right), 1, "nothing right of Exit");
        assert_eq!(step(&stops, 2, right), 3);
        assert_eq!(step(&stops, 5, left), 4);
        assert_eq!(step(&stops, 2, left), 2, "nothing left of the Team");
        // across the screen: up from the Team to Settings (the nearer of the two), down from
        // Exit to the list's end
        assert_eq!(step(&stops, 2, up), 0);
        assert_eq!(step(&stops, 1, down), 6);
        assert_eq!(step(&stops, 0, down), 6);
        // nothing below the bottom row
        assert_eq!(step(&stops, 4, down), 4);
        // a place gone: back to the first
        assert_eq!(step(&stops, 99, up), 0);
    }
}
