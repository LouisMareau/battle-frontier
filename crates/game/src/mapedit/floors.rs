//! Flooring: the floorings to pick from (tiles, and sets of them laid to fit: `world::floors`),
//! shared by the Build tool's Flooring and the Ground tool's Floors tab, which lays flooring on
//! the map's cells out of doors (a pavement, a plaza): each cell its tile and turn, a set's
//! tile the one that fits the cells round it; laying it clears trees from the cells. A tile
//! put by hand may be mirrored, and put again round a centre as a symmetry takes it (a mosaic,
//! a church floor's design): `Mirroring`, shared by both.

use std::collections::HashMap;

use egui::RichText;
use glam::Vec3;
use render::scene::{Shape, ShapeKind};
use world::floors::{self, Laid, Symmetry};

use super::{hint, mark, section, swatch, warn, MapEditor};

/// A flooring that's a set of floor tiles, laid to fit: this, then the set.
pub(super) const SET: &str = "#set:";

/// The Floors tab's brush that takes the flooring away.
const NONE: &str = "#none";

/// A set's name as the panels say it ("old_ruins" → "Old ruins").
pub(super) fn set_name(set: &str) -> String {
    let words = set.replace('_', " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The floorings to pick from: each "floor" prop of no set, and each set once (with its first
/// tile's colour): (id, name, colour), a set's id SET and its name.
pub(super) fn floorings(ed: &MapEditor) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = ed.content.props.iter().filter(|p| p.category == "floor" && p.floor.is_none()).map(|p| (p.id.clone(), p.name.clone(), p.color(0).to_string())).collect();
    for (set, tiles) in ed.content.floor_sets() {
        let color = tiles.first().map(|p| p.color(0).to_string()).unwrap_or_else(|| "#888888".into());
        out.push((format!("{SET}{set}"), format!("{}: a set of {}, laid to fit", set_name(&set), tiles.len()), color));
    }
    out
}

/// A set's tiles, to put by hand: (id, name, colour).
pub(super) fn set_tiles(content: &content::Content, set: &str) -> Vec<(String, String, String)> {
    content.floor_sets().get(set).map(|t| t.iter().map(|p| (p.id.clone(), p.name.clone(), p.color(0).to_string())).collect()).unwrap_or_default()
}

/// The set a brush is, or is one of the tiles of.
pub(super) fn brush_set<'a>(content: &'a content::Content, brush: &'a str) -> Option<&'a str> {
    brush.strip_prefix(SET).or_else(|| floors::set_of(content, brush))
}

/// The turn a tile is put by hand at, as a button that turns it a quarter more: true when it
/// was clicked.
pub(super) fn turn_button(ui: &mut egui::Ui, turn: &mut u8, key: &str) -> bool {
    let r = ui.button(format!("Turned {}° ⟳", *turn as u32 * 90)).on_hover_text("The turn a tile put by hand gets: a quarter more clockwise each click");
    mark(key, &r);
    if r.clicked() {
        *turn = (*turn + 1) % 4;
    }
    r.clicked()
}

/// Whether a tile put by hand is mirrored (its east and west swapped, before it's turned), as
/// a button that says so: true when it was clicked.
pub(super) fn flip_button(ui: &mut egui::Ui, flip: &mut bool, key: &str) -> bool {
    let r = ui.selectable_label(*flip, "Mirrored").on_hover_text("The tile put by hand mirrored, its east and west swapped (then turned): a design's other half");
    mark(key, &r);
    if r.clicked() {
        *flip = !*flip;
    }
    r.clicked()
}

/// Tiles put by hand (or taken away) put again round a centre as a symmetry takes them: the
/// symmetry, its centre in half cells (a cell's middle odd both ways, a corner even both ways;
/// none picked: out of doors none yet, in a building its floor's middle), and whether the next
/// click picks the centre.
#[derive(Clone, Debug, Default)]
pub(super) struct Mirroring {
    pub symmetry: Symmetry,
    pub centre: Option<(i32, i32)>,
    pub picking: bool,
}

impl Mirroring {
    /// Where a tile put on `cell` (as it lies) goes: there, and again where the symmetry takes
    /// it round its centre (`middle` when none is picked); just there with no centre.
    pub fn places(&self, middle: Option<(i32, i32)>, cell: (i32, i32), turn: u8, flip: bool) -> Vec<((i32, i32), u8, bool)> {
        match self.centre.or(middle) {
            Some(c) if self.symmetry != Symmetry::None => floors::symmetric(self.symmetry, c, cell, turn, flip),
            _ => vec![(cell, turn, flip)],
        }
    }

    /// The panels' part: the symmetries, the centre (the next click on the map picks it),
    /// what a centre can't do. `middle`: the centre when none is picked (a building floor's
    /// middle); `key`: the marks' start ("uiFloor" makes uiFloorSym_turn4, uiFloorCentre).
    pub fn panel(&mut self, ui: &mut egui::Ui, middle: Option<(i32, i32)>, key: &str) {
        ui.horizontal_wrapped(|ui| {
            for s in Symmetry::ALL {
                let (name, about) = symmetry_name(s);
                let r = ui.selectable_label(self.symmetry == s, name).on_hover_text(about);
                mark(&format!("{key}Sym_{}", s.key()), &r);
                if r.clicked() {
                    self.symmetry = s;
                    // no centre yet: the next click picks it
                    self.picking = s != Symmetry::None && self.centre.or(middle).is_none();
                }
            }
        });
        if self.symmetry == Symmetry::None {
            self.picking = false;
            return;
        }
        let centre = self.centre.or(middle);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(centre.map(centre_name).unwrap_or_else(|| "Centre: not picked yet".into())).size(12.0));
            let pick = ui.selectable_label(self.picking, "Pick it").on_hover_text("The next click sets the centre: the middle of a cell, of an edge, or a corner, whichever is nearest");
            mark(&format!("{key}Centre"), &pick);
            if pick.clicked() {
                self.picking = !self.picking;
            }
            if self.centre.is_some() && middle.is_some() {
                let back = ui.small_button("The middle").on_hover_text("Back to the floor's middle");
                mark(&format!("{key}CentreMiddle"), &back);
                if back.clicked() {
                    self.centre = None;
                }
            }
        });
        if self.picking {
            hint(ui, "Click where the centre goes: the middle of a cell, of an edge, or a corner.");
        } else if centre.is_some_and(|c| self.symmetry.quarter_turns() && (c.0 + c.1).rem_euclid(2) == 1) {
            warn(ui, "Quarter turns round the middle of an edge land between cells: only the half turn's copies are put. Pick a cell's middle or a corner.");
        }
    }

    /// The marks: where a tile put on each of `cells` goes again (not the cell itself), the
    /// centre and the lines it mirrors across, `reach` cells out from it; `y` the height of a
    /// cell's ground (none: no tile goes there); `o` where cell (0, 0) starts, at the height
    /// the centre's marks go when no cell touching it has one.
    pub fn shapes(&self, middle: Option<(i32, i32)>, cells: &[(i32, i32)], reach: f32, o: Vec3, y: &dyn Fn(i32, i32) -> Option<f32>, out: &mut Vec<Shape>) {
        let Some(c) = self.centre.or(middle).filter(|_| self.symmetry != Symmetry::None) else { return };
        let gold = [1.0, 0.82, 0.3, 0.85];
        let (cx, cz) = (o.x + c.0 as f32 / 2.0, o.z + c.1 as f32 / 2.0);
        // over the highest of the cells it touches
        let touching = [(c.0 - 1).div_euclid(2), c.0.div_euclid(2)].into_iter().flat_map(|x| [(c.1 - 1).div_euclid(2), c.1.div_euclid(2)].map(|z| (x, z)));
        let cy = touching.filter_map(|(x, z)| y(x, z)).reduce(f32::max).unwrap_or(o.y) + 0.08;
        out.push(Shape { kind: ShapeKind::Ring { inner: 0.12, outer: 0.2 }, center: Vec3::new(cx, cy, cz), color: gold, additive: false });
        let line = [1.0, 0.82, 0.3, 0.45];
        let (across, along, diagonals) = self.symmetry.lines();
        if across {
            out.push(Shape { kind: ShapeKind::Rect { width: 0.05, depth: reach * 2.0 }, center: Vec3::new(cx, cy, cz), color: line, additive: false });
        }
        if along {
            out.push(Shape { kind: ShapeKind::Rect { width: reach * 2.0, depth: 0.05 }, center: Vec3::new(cx, cy, cz), color: line, additive: false });
        }
        if diagonals {
            for (dx, dz) in [(1.0f32, 1.0f32), (1.0, -1.0)] {
                let (a, b) = (Vec3::new(dx, 0.0, dz) * reach * std::f32::consts::FRAC_1_SQRT_2, Vec3::new(-dz, 0.0, dx) * 0.025);
                let at = Vec3::new(cx, cy, cz);
                out.push(Shape { kind: ShapeKind::Quad { corners: [at - a - b, at - a + b, at + a + b, at + a - b] }, center: Vec3::ZERO, color: line, additive: false });
            }
        }
        for &cell in cells {
            for ((x, z), ..) in self.places(middle, cell, 0, false).into_iter().skip(1) {
                let Some(h) = y(x, z) else { continue };
                out.push(Shape { kind: ShapeKind::Rect { width: 0.9, depth: 0.9 }, center: Vec3::new(o.x + x as f32 + 0.5, h + 0.06, o.z + z as f32 + 0.5), color: [0.55, 0.8, 1.0, 0.4], additive: false });
            }
        }
    }
}

/// A symmetry's name on its button, and what it does.
fn symmetry_name(s: Symmetry) -> (&'static str, &'static str) {
    match s {
        Symmetry::None => ("None", "Each tile put once, where it's clicked"),
        Symmetry::MirrorX => ("Mirror left-right", "Put again mirrored across the line from north to south through the centre"),
        Symmetry::MirrorZ => ("Mirror top-bottom", "Put again mirrored across the line from east to west through the centre"),
        Symmetry::MirrorBoth => ("Both mirrors", "Mirrored across both lines: four of each"),
        Symmetry::Turn2 => ("Half turn", "Put again turned half round the centre: two of each"),
        Symmetry::Turn4 => ("Quarter turns", "Put again a quarter turn round the centre each time: four of each (a pinwheel)"),
        Symmetry::All8 => ("All eight", "Quarter turns, and mirrored across both lines and both diagonals: eight of each (a medallion, a rose)"),
    }
}

/// A centre (in half cells) as the panels say it: "Centre: 12.5, 8, a cell's middle".
fn centre_name((x, z): (i32, i32)) -> String {
    let half = |v: i32| if v.rem_euclid(2) == 0 { format!("{}", v / 2) } else { format!("{:.1}", v as f32 / 2.0) };
    let what = match (x.rem_euclid(2), z.rem_euclid(2)) {
        (1, 1) => "a cell's middle",
        (0, 0) => "a corner",
        _ => "an edge's middle",
    };
    format!("Centre: {}, {}, {what}", half(x), half(z))
}

/// A point on the ground (x, z) as the nearest centre there can be, in half cells.
pub(super) fn centre_at(x: f32, z: f32) -> (i32, i32) {
    ((2.0 * x).round() as i32, (2.0 * z).round() as i32)
}

/// A laid cell as the checks read it: "x,z:tile:turn", then ":flip" when mirrored and ":hand"
/// when put by hand.
pub(super) fn laid_hook(tiles: &[content::CellTile]) -> String {
    let mut out: Vec<String> =
        tiles.iter().map(|t| format!("{},{}:{}:{}{}{}", t.cell[0], t.cell[1], t.tile, t.turn, if t.flip { ":flip" } else { "" }, if t.by_hand { ":hand" } else { "" })).collect();
    out.sort();
    out.join(";")
}

/// What a floor's own tile is among the floorings: its set, when it's a tile of one.
pub(super) fn flooring_of(content: &content::Content, tile: &str) -> String {
    floors::set_of(content, tile).map(|set| format!("{SET}{set}")).unwrap_or_else(|| tile.to_string())
}

/// The tile a flooring picked makes a floor's own: a set's first (a floor of a set's tile is
/// laid with the set).
pub(super) fn tile_of(content: &content::Content, flooring: &str) -> String {
    match flooring.strip_prefix(SET) {
        Some(set) => content.floor_sets().get(set).and_then(|t| t.first()).map(|p| p.id.clone()).unwrap_or_default(),
        None => flooring.to_string(),
    }
}

/// The Ground tool's Floors tab.
#[derive(Default)]
pub(super) struct Floorer {
    /// the tab open (else the Ground tool paints terrain, or ramps)
    pub on: bool,
    /// what a stroke lays: a flooring (a tile, or SET and a set), or NONE
    pub brush: String,
    /// a stroke going on (the button held), and the cells a set's tiles were laid on in it
    /// (laid again all together as it ends)
    pub painting: bool,
    pub stroke: Vec<(i32, i32)>,
    /// how many times "Lay it again" was asked (its seed)
    pub relaid: u64,
    /// the turn a tile put by hand gets, and whether it's mirrored first
    pub turn: u8,
    pub flip: bool,
    /// tiles put by hand (or taken away) put again round a centre
    pub mirror: Mirroring,
}

impl MapEditor {
    /// A seed of the map's and `salt`'s: the same tiles every time.
    fn floor_seed(&self, salt: u64) -> u64 {
        self.map.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100_0000_01b3)) ^ salt.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// The flooring out of doors as laid, but on `except`.
    fn floors_laid(&self, except: &[(i32, i32)]) -> HashMap<(i32, i32), Laid> {
        self.map.floors.iter().filter(|t| !except.contains(&(t.cell[0], t.cell[1]))).map(|t| ((t.cell[0], t.cell[1]), Laid { tile: t.tile.clone(), turn: t.turn, flip: t.flip })).collect()
    }

    /// The Floors tab's panel: the floorings, none, the brush's size, laid again.
    pub(super) fn floor_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Flooring");
        hint(ui, "Click or drag over the ground: each cell takes the flooring picked (on its own level; never on a ramp's slope), and its trees go. A set's tile is the one that fits the cells round it.");
        let choices = floorings(self);
        if self.floorer.brush.is_empty() {
            self.floorer.brush = choices.first().map(|c| c.0.clone()).unwrap_or_else(|| NONE.into());
        }
        let open = brush_set(&self.content, &self.floorer.brush).map(str::to_string);
        for (id, name, color) in &choices {
            ui.horizontal(|ui| {
                swatch(ui, color);
                let r = ui.selectable_label(self.floorer.brush == *id, name);
                mark(&id.strip_prefix(SET).map(|set| format!("uiFloorSet_{set}")).unwrap_or_else(|| format!("uiFloorBrush_{id}")), &r);
                if r.clicked() {
                    self.floorer.brush = id.clone();
                }
            });
            // a set picked: its tiles too, each to put by hand (a mosaic, a design of one's own)
            if let Some(set) = id.strip_prefix(SET).filter(|s| open.as_deref() == Some(*s)) {
                ui.indent(("floor-set", set), |ui| {
                    for (tile, tname, tcolor) in set_tiles(&self.content, set) {
                        ui.horizontal(|ui| {
                            swatch(ui, &tcolor);
                            let r = ui.selectable_label(self.floorer.brush == tile, format!("{tname} · by hand")).on_hover_text("Put just where you click, as it's turned, whatever the set's laying does round it (which fits it)");
                            mark(&format!("uiFloorBrush_{tile}"), &r);
                            if r.clicked() {
                                self.floorer.brush = tile.clone();
                            }
                        });
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            swatch(ui, "#1c2130");
            let r = ui.selectable_label(self.floorer.brush == NONE, "None: the flooring taken away");
            mark("uiFloorNone", &r);
            if r.clicked() {
                self.floorer.brush = NONE.into();
            }
        });
        // a tile put by hand: its turn, mirrored or not
        let by_hand = !self.floorer.brush.starts_with(SET);
        if self.floorer.brush != NONE && by_hand {
            ui.horizontal(|ui| {
                turn_button(ui, &mut self.floorer.turn, "uiFloorTurn");
                flip_button(ui, &mut self.floorer.flip, "uiFloorFlip");
            });
        }
        // put by hand or taken away: again round a centre
        if by_hand {
            section(ui, "Symmetry");
            hint(ui, "Each tile put by hand (or taken away) is put again where the symmetry takes it round its centre, mirrored and turned to match: a mosaic, a church floor's design.");
            self.floorer.mirror.panel(ui, None, "uiFloor");
        }
        section(ui, "Brush");
        ui.add(egui::Slider::new(&mut self.brush, 0..=4).custom_formatter(|v, _| format!("{0}×{0}", v as i32 * 2 + 1)));
        let n = self.map.floors.len();
        ui.label(RichText::new(format!("{n} cell{} laid with flooring", if n == 1 { "" } else { "s" })).size(12.0));
        if let Some(set) = self.floorer.brush.strip_prefix(SET).map(str::to_string) {
            // (those put by hand stay)
            let cells: Vec<(i32, i32)> = self.map.floors.iter().filter(|t| !t.by_hand && floors::set_of(&self.content, &t.tile) == Some(set.as_str())).map(|t| (t.cell[0], t.cell[1])).collect();
            let again = ui.add_enabled(!cells.is_empty(), egui::Button::new("Lay it again").small()).on_hover_text("The map's cells of the set laid again another way, still fitting each other and what's round them (and those put by hand, which stay)");
            mark("uiFloorRelay", &again);
            if again.clicked() {
                self.checkpoint();
                self.floorer.relaid += 1;
                if self.relay_floors(&cells, self.floorer.relaid ^ 0xa11) {
                    self.need_props = true;
                }
            }
        }
        // for the checks
        crate::platform::set_hook("floorbrush", &self.floorer.brush);
        crate::platform::set_hook("floorturn", &self.floorer.turn.to_string());
        crate::platform::set_hook("floorflip", if self.floorer.flip { "1" } else { "" });
        crate::platform::set_hook("floorsym", self.floorer.mirror.symmetry.key());
        crate::platform::set_hook("floorcentre", &self.floorer.mirror.centre.map(|c| format!("{},{}", c.0, c.1)).unwrap_or_default());
        crate::platform::set_hook("floorlaid", &laid_hook(&self.map.floors));
    }

    /// The Floors tab's strokes: from a press on the map, the cells under the pointer (and its
    /// brush round them) laid as it goes; `floor_stroke_ends` as it's let go.
    pub(super) fn floor_input(&mut self, hovered: bool, pressed: bool, down: bool) {
        // picking the symmetry's centre: this click sets it, and lays nothing
        if self.floorer.mirror.picking {
            if pressed && hovered {
                if let Some((x, z)) = self.floor_point() {
                    self.floorer.mirror.centre = Some(centre_at(x, z));
                    self.floorer.mirror.picking = false;
                }
            }
            return;
        }
        if pressed && hovered && self.ground_hit.is_some_and(|c| self.map.inside(c.0, c.1)) {
            self.checkpoint();
            self.floorer.painting = true;
            self.floorer.stroke.clear();
        }
        if !(self.floorer.painting && down) {
            return;
        }
        let Some(c) = self.ground_hit else { return };
        for c in self.line_to(c) {
            for cell in self.brush_cells(c) {
                if self.lay_floor(cell) {
                    self.need_props = true;
                }
            }
        }
    }

    /// A stroke let go: a set's cells laid in it laid again all together, to fit each other as
    /// well as what's round them.
    pub(super) fn floor_stroke_ends(&mut self) {
        let stroke = std::mem::take(&mut self.floorer.stroke);
        if stroke.len() > 1 && self.relay_floors(&stroke, stroke[0].0 as u64 ^ ((stroke[0].1 as u64) << 20)) {
            self.need_props = true;
        }
        self.floorer.painting = false;
    }

    /// The brush laid on a cell: true when it changed. A set's tile is the one that fits what's
    /// round it (once a stroke: kept as it goes on); a tile put by hand, or none, is put again
    /// where the symmetry takes it; the cells' trees go.
    fn lay_floor(&mut self, (x, z): (i32, i32)) -> bool {
        if !self.map.inside(x, z) {
            return false;
        }
        let brush = self.floorer.brush.clone();
        if let Some(set) = brush.strip_prefix(SET) {
            if self.floorer.stroke.contains(&(x, z)) {
                return false;
            }
            self.floorer.stroke.push((x, z));
            let fixed = self.floors_laid(&[(x, z)]);
            let laid = floors::lay(&self.content, set, &[(x, z)], &fixed, self.floor_seed((x as u64) << 20 ^ z as u64)).remove(&(x, z));
            return self.put_floor((x, z), laid, false);
        }
        // a set's tile put by hand stays as it's put
        let by_hand = floors::set_of(&self.content, &brush).is_some();
        let mut changed = false;
        for (cell, turn, flip) in self.floorer.mirror.places(None, (x, z), self.floorer.turn, self.floorer.flip) {
            let laid = (brush != NONE).then(|| Laid { tile: brush.clone(), turn, flip });
            changed |= self.put_floor(cell, laid, by_hand);
        }
        changed
    }

    /// The point on the ground under the pointer (x, z), on its cell's level.
    fn floor_point(&self) -> Option<(f32, f32)> {
        let (x, z) = self.ground_hit?;
        let (o, d) = self.last_ray?;
        let y = self.map.level_at(x, z)? as f32 * self.level_height;
        let p = if d.y.abs() > 1e-6 { o + d * ((y - o.y) / d.y) } else { Vec3::new(x as f32 + 0.5, y, z as f32 + 0.5) };
        Some((p.x.clamp(x as f32, x as f32 + 1.0), p.z.clamp(z as f32, z as f32 + 1.0)))
    }

    /// A cell laid with `laid` (none: its flooring taken away): true when it changed. Laid,
    /// its trees go.
    fn put_floor(&mut self, (x, z): (i32, i32), laid: Option<Laid>, by_hand: bool) -> bool {
        if !self.map.inside(x, z) {
            return false;
        }
        let changed = self.map.set_floor(x, z, laid.as_ref().map(|l| (l.tile.as_str(), l.turn, l.flip)), by_hand);
        // a pavement clears the trees from its cells
        if laid.is_some() {
            if let Some(i) = self.map.index(x, z) {
                if self.map.area[i] != 0 {
                    self.map.area[i] = 0;
                    self.need_trees = true;
                }
            }
        }
        changed
    }

    /// Cells laid again with the brush's set, all together, to fit each other and what's round
    /// them: true when any changed.
    fn relay_floors(&mut self, cells: &[(i32, i32)], salt: u64) -> bool {
        let Some(set) = self.floorer.brush.strip_prefix(SET).map(str::to_string) else { return false };
        let fixed = self.floors_laid(cells);
        let laid = floors::lay(&self.content, &set, cells, &fixed, self.floor_seed(salt));
        let mut changed = false;
        for ((x, z), l) in laid {
            changed |= self.map.set_floor(x, z, Some((&l.tile, l.turn, l.flip)), false);
        }
        changed
    }

    /// The Floors tab's marks: the brush's cells under the pointer, on their ground; with a
    /// symmetry, where they're put again, its centre and its lines (picking the centre: where
    /// the click would put it).
    pub(super) fn floor_shapes(&self, shapes: &mut Vec<Shape>) {
        let lh = self.level_height;
        let ground = |x: i32, z: i32| self.map.level_at(x, z).unwrap_or(0) as f32 * lh;
        if !self.floorer.brush.starts_with(SET) {
            let reach = 16.0f32.min(self.map.width.max(self.map.height) as f32);
            let under: Vec<(i32, i32)> = self.ground_hit.filter(|_| !self.floorer.mirror.picking).map(|c| self.brush_cells(c)).unwrap_or_default();
            self.floorer.mirror.shapes(None, &under, reach, Vec3::ZERO, &|x, z| self.map.level_at(x, z).map(|l| l as f32 * lh), shapes);
        }
        if self.floorer.mirror.picking {
            if let Some((x, z)) = self.floor_point() {
                let c = centre_at(x, z);
                let y = ground(c.0.div_euclid(2), c.1.div_euclid(2)) + 0.08;
                shapes.push(Shape { kind: ShapeKind::Ring { inner: 0.1, outer: 0.22 }, center: Vec3::new(c.0 as f32 / 2.0, y, c.1 as f32 / 2.0), color: [1.0, 0.82, 0.3, 0.6], additive: false });
            }
            return;
        }
        let Some((x, z)) = self.ground_hit else { return };
        let size = (self.brush * 2 + 1) as f32;
        let y = ground(x, z) + 0.06;
        let color = if self.floorer.brush == NONE { [1.0, 0.35, 0.3, 0.4] } else { [0.95, 0.9, 0.75, 0.4] };
        shapes.push(Shape { kind: ShapeKind::Rect { width: size, depth: size }, center: Vec3::new(x as f32 + 0.5, y, z as f32 + 0.5), color, additive: false });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use content::{FloorTile, Prop, Space};

    use super::*;

    /// An editor on a plaza 12 x 10, with a marble set: a corner (rims north and east), and
    /// the field.
    fn editor() -> MapEditor {
        let mut c = content::Content::default();
        let tile = |id: &str, edges: [&str; 4]| Prop {
            id: id.into(),
            name: id.into(),
            category: "floor".into(),
            footprint: [1, 1],
            height: 0.05,
            floor: Some(FloorTile { set: "marble".into(), edges: edges.map(String::from), weight: 1.0, turns: true }),
            ..Default::default()
        };
        c.props.put(tile("corner", ["rim", "rim", "b", "b"]), Space::Content);
        c.props.put(tile("field", ["m"; 4]), Space::Content);
        let root = std::env::temp_dir().join("battle-frontier-floors");
        let mut ed = MapEditor::new(Arc::new(c), None, crate::platform::Files::new(root.to_str().unwrap()));
        ed.map = world::WorldMap::new("plaza", "Plaza", 12, 10, "grass");
        ed
    }

    #[test]
    fn a_tile_put_by_hand_is_put_again_as_the_symmetry_takes_it() {
        let mut ed = editor();
        ed.floorer.brush = "corner".into();
        ed.floorer.turn = 1;
        // mirrored both ways round the corner between cells (5, 4) and (6, 5)
        ed.floorer.mirror = Mirroring { symmetry: Symmetry::MirrorBoth, centre: Some((12, 10)), picking: false };
        assert!(ed.lay_floor((1, 2)));
        assert_eq!(laid_hook(&ed.map.floors), "1,2:corner:1:hand;1,7:corner:1:flip:hand;10,2:corner:3:flip:hand;10,7:corner:3:hand");
        // taken away from any of them: from all four
        ed.floorer.brush = NONE.into();
        assert!(ed.lay_floor((10, 7)));
        assert!(ed.map.floors.is_empty(), "{}", laid_hook(&ed.map.floors));
        // quarter turns round a cell's middle, mirrored first
        ed.floorer.brush = "corner".into();
        ed.floorer.flip = true;
        ed.floorer.mirror = Mirroring { symmetry: Symmetry::Turn4, centre: Some((9, 9)), picking: false };
        ed.lay_floor((4, 2));
        assert_eq!(laid_hook(&ed.map.floors), "2,4:corner:0:flip:hand;4,2:corner:1:flip:hand;4,6:corner:3:flip:hand;6,4:corner:2:flip:hand");
        // a copy off the map isn't put
        ed.map.floors.clear();
        ed.floorer.mirror = Mirroring { symmetry: Symmetry::MirrorX, centre: Some((2, 2)), picking: false };
        ed.lay_floor((3, 0));
        assert_eq!(laid_hook(&ed.map.floors), "3,0:corner:1:flip:hand");
        // no centre picked yet: just where it's clicked
        ed.map.floors.clear();
        ed.floorer.mirror.centre = None;
        ed.lay_floor((4, 4));
        assert_eq!(ed.map.floors.len(), 1);
        // a set laid to fit isn't mirrored: its tiles are picked to fit
        ed.map.floors.clear();
        ed.floorer.brush = format!("{SET}marble");
        ed.floorer.mirror = Mirroring { symmetry: Symmetry::MirrorBoth, centre: Some((12, 10)), picking: false };
        ed.lay_floor((1, 2));
        assert_eq!(ed.map.floors.len(), 1, "{}", laid_hook(&ed.map.floors));
    }

    #[test]
    fn a_centre_is_the_nearest_middle_edge_or_corner() {
        assert_eq!(centre_at(3.1, 4.9), (6, 10), "a corner");
        assert_eq!(centre_at(3.45, 4.6), (7, 9), "a cell's middle");
        assert_eq!(centre_at(3.5, 4.1), (7, 8), "an edge's middle");
        assert_eq!(centre_name((7, 9)), "Centre: 3.5, 4.5, a cell's middle");
        assert_eq!(centre_name((6, 10)), "Centre: 3, 5, a corner");
    }
}
