//! Flooring: the floorings to pick from (tiles, and sets of them laid to fit: `world::floors`),
//! shared by the Build tool's Flooring and the Ground tool's Floors tab, which lays flooring on
//! the map's cells out of doors (a pavement, a plaza): each cell its tile and turn, a set's
//! tile the one that fits the cells round it; laying it clears trees from the cells.

use std::collections::HashMap;

use egui::RichText;
use render::scene::{Shape, ShapeKind};
use world::floors::{self, Laid};

use super::{hint, mark, section, swatch, MapEditor};

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

/// A laid cell as the checks read it: "x,z:tile:turn", and ":hand" when put by hand.
pub(super) fn laid_hook(tiles: &[content::CellTile]) -> String {
    let mut out: Vec<String> = tiles.iter().map(|t| format!("{},{}:{}:{}{}", t.cell[0], t.cell[1], t.tile, t.turn, if t.by_hand { ":hand" } else { "" })).collect();
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
    /// the turn a tile put by hand gets
    pub turn: u8,
}

impl MapEditor {
    /// A seed of the map's and `salt`'s: the same tiles every time.
    fn floor_seed(&self, salt: u64) -> u64 {
        self.map.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100_0000_01b3)) ^ salt.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// The flooring out of doors as laid, but on `except`.
    fn floors_laid(&self, except: &[(i32, i32)]) -> HashMap<(i32, i32), Laid> {
        self.map.floors.iter().filter(|t| !except.contains(&(t.cell[0], t.cell[1]))).map(|t| ((t.cell[0], t.cell[1]), Laid { tile: t.tile.clone(), turn: t.turn })).collect()
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
        // a tile put by hand: its turn
        if self.floorer.brush != NONE && !self.floorer.brush.starts_with(SET) {
            turn_button(ui, &mut self.floorer.turn, "uiFloorTurn");
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
        crate::platform::set_hook("floorlaid", &laid_hook(&self.map.floors));
    }

    /// The Floors tab's strokes: from a press on the map, the cells under the pointer (and its
    /// brush round them) laid as it goes; `floor_stroke_ends` as it's let go.
    pub(super) fn floor_input(&mut self, hovered: bool, pressed: bool, down: bool) {
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
    /// round it (once a stroke: kept as it goes on); the cell's trees go.
    fn lay_floor(&mut self, (x, z): (i32, i32)) -> bool {
        if !self.map.inside(x, z) {
            return false;
        }
        let brush = self.floorer.brush.clone();
        // a set's tile put by hand stays as it's put
        let by_hand = floors::set_of(&self.content, &brush).is_some();
        let laid = if brush == NONE {
            None
        } else if let Some(set) = brush.strip_prefix(SET) {
            if self.floorer.stroke.contains(&(x, z)) {
                return false;
            }
            self.floorer.stroke.push((x, z));
            let fixed = self.floors_laid(&[(x, z)]);
            floors::lay(&self.content, set, &[(x, z)], &fixed, self.floor_seed((x as u64) << 20 ^ z as u64)).remove(&(x, z))
        } else {
            Some(Laid { tile: brush, turn: self.floorer.turn })
        };
        let changed = self.map.set_floor(x, z, laid.as_ref().map(|l| (l.tile.as_str(), l.turn)), by_hand);
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
            changed |= self.map.set_floor(x, z, Some((&l.tile, l.turn)), false);
        }
        changed
    }

    /// The Floors tab's mark: the brush's cells under the pointer, on their ground.
    pub(super) fn floor_shapes(&self, shapes: &mut Vec<Shape>) {
        let Some((x, z)) = self.ground_hit else { return };
        let size = (self.brush * 2 + 1) as f32;
        let y = self.map.level_at(x, z).unwrap_or(0) as f32 * self.level_height + 0.06;
        let color = if self.floorer.brush == NONE { [1.0, 0.35, 0.3, 0.4] } else { [0.95, 0.9, 0.75, 0.4] };
        shapes.push(Shape { kind: ShapeKind::Rect { width: size, depth: size }, center: glam::Vec3::new(x as f32 + 0.5, y, z as f32 + 0.5), color, additive: false });
    }
}
