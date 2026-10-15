//! The records, one file each: `data/<collection>/<id>.json`. Everything refers to other
//! records by id (lowercase slugs). Asset paths are relative to the record's space's
//! `assets/` folder. Mechanics are named by `effect` keys the engine knows, never by ids
//! or names, so a renamed or new record can reuse a mechanic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// The six stats, in the order the game shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub hp: u16,
    pub attack: u16,
    pub defense: u16,
    pub speed: u16,
    pub sp_attack: u16,
    pub sp_defense: u16,
}

/// A type, and how hard its attacks hit each defending type (only the multipliers that
/// aren't 1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TypeDef {
    pub id: String,
    pub name: String,
    /// a 3-letter label
    pub short: String,
    /// "#rrggbb"
    pub color: String,
    /// the order types are listed in
    pub order: u16,
    /// not shown to players (e.g. a type for typeless moves)
    #[serde(skip_serializing_if = "is_default")]
    pub hidden: bool,
    pub against: BTreeMap<String, f64>,
    /// statuses its creatures can't get (keys: "burn", "poison", "freeze", "paralysis", "sleep")
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub immune_to: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeldItems {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rare: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Body {
    pub height_m: f64,
    pub weight_kg: f64,
    pub category: String,
    pub color: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Learnset {
    /// (level, move), in order
    pub level: Vec<(u8, String)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub machine: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tutor: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub egg: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evolution {
    pub into: String,
    /// "level", "item", "trade", "trade_item", "friendship", "friendship_day",
    /// "friendship_night", "level_attack_higher", "level_attack_equal", "level_attack_lower",
    /// "level_personality_a", "level_personality_b", "level_split", "level_extra", "beauty"
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sprites {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front_shiny: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_shiny: Option<String>,
    /// the visible pixels of the front and back pictures: [x0, y0, x1, y1)
    pub front_box: [i32; 4],
    pub back_box: [i32; 4],
    /// how high it floats above the ground, in pixels
    #[serde(skip_serializing_if = "is_default")]
    pub elevation: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Creature {
    pub id: String,
    pub name: String,
    /// its number in the creature index
    pub number: u16,
    pub types: Vec<String>,
    pub base_stats: Stats,
    pub abilities: Vec<String>,
    pub catch_rate: u16,
    pub exp_yield: u16,
    pub effort_yield: Stats,
    pub growth_rate: String,
    /// share of females, 0 to 1; None: genderless
    pub female_ratio: Option<f64>,
    pub egg_groups: Vec<String>,
    pub egg_cycles: u16,
    pub base_friendship: u16,
    pub held_items: HeldItems,
    pub body: Body,
    pub description: String,
    pub learnset: Learnset,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evolutions: Vec<Evolution>,
    pub sprites: Sprites,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cry: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Move {
    pub id: String,
    pub name: String,
    pub number: u16,
    #[serde(rename = "type")]
    pub ty: String,
    /// "physical", "special" or "status"
    pub category: String,
    pub power: u16,
    /// 0: never misses
    pub accuracy: u16,
    pub pp: u8,
    #[serde(skip_serializing_if = "is_default")]
    pub priority: i8,
    /// "selected", "both", "foes_and_ally", "user", "random", "depends", "user_or_selected", "opponents_field"
    pub target: String,
    /// the engine's mechanic for it (see the battle rules' move profiles)
    pub effect: String,
    #[serde(skip_serializing_if = "is_default")]
    pub effect_chance: u16,
    #[serde(skip_serializing_if = "is_default")]
    pub contact: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ability {
    pub id: String,
    pub name: String,
    pub number: u16,
    /// the engine's mechanic for it ("" = none yet)
    pub effect: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub number: u16,
    pub price: u32,
    /// "items", "balls", "machines", "berries", "key_items"
    pub pocket: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_effect: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub hold_param: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_use: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battle_use: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub key_item: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Nature {
    pub id: String,
    pub name: String,
    pub number: u16,
    /// stat keys as in `Stats`
    pub raises: Option<String>,
    pub lowers: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrowthRate {
    pub id: String,
    pub name: String,
    /// total experience needed to reach each level (index = level, 0..=100)
    pub exp: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EggGroup {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub order: u16,
    pub note: String,
    /// the map battles are staged on for now (a folder under assets/maps)
    pub map: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub creature: String,
    pub level: u8,
    /// empty: the learnset's last four at that level
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<String>,
    /// 1-5 per move; empty: the encounter's move_level
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub move_levels: Vec<u8>,
}

/// A battle to try: wild, trainer (fixed size) or gym (the leader matches what you send).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Encounter {
    pub id: String,
    pub name: String,
    pub region: String,
    pub order: u16,
    /// "wild", "trainer" or "gym"
    pub kind: String,
    /// creatures per side in trainer battles
    pub size: u8,
    /// the player's level for it
    pub level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trainer_sprite: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub move_level: u8,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub party: Vec<Member>,
    /// wild: who's around, by time of day ("morning", "day", "night"): (creature, level)
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub pools: BTreeMap<String, Vec<(String, u8)>>,
}

/// A ready-made team (the player's starting party, test teams...).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Party {
    pub id: String,
    pub name: String,
    /// the trainer standing behind them in battle (a 16x32 picture, seen from the back)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trainer_sprite: Option<String>,
    pub members: Vec<Member>,
}

// ---------------------------------------------------------------- the world

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

/// The world's measures, the same on every map (one record, id "world"). A cell is the
/// world's unit; a level (one step of elevation, one floor of a building) is the main
/// character's height plus some room above it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub id: String,
    /// in cells
    pub character_height: f64,
    /// room above the character in a level, in cells
    pub level_padding: f64,
    /// the highest level a map can use
    pub max_level: u8,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { id: "world".into(), character_height: 1.5, level_padding: 0.5, max_level: 9 }
    }
}

impl Settings {
    /// A level's height, in cells.
    pub fn level_height(&self) -> f64 {
        self.character_height + self.level_padding
    }
}

/// A kind of ground painted on cells. Where two meet, the engine draws the border itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terrain {
    pub id: String,
    pub name: String,
    pub order: u16,
    /// "#rrggbb": the main colour, and the one its pattern mixes in
    pub color: String,
    pub color2: String,
    /// how it's drawn: "grass", "meadow", "dirt", "sand", "stones", "water", "rock"
    pub pattern: String,
    /// the cliff face under its cells
    pub side: String,
    /// "walk", "water" (needs surfing) or "block"
    pub walk: String,
    /// where two terrains meet, the one with the higher priority reaches further
    #[serde(skip_serializing_if = "is_default")]
    pub priority: i16,
}

/// A piece of a prop's stand-in model, until it has a real one: a simple shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Part {
    /// "box", "cylinder", "cone", "sphere", "rock"
    pub shape: String,
    /// width (x), height (y), depth (z), in cells
    pub size: [f64; 3],
    /// the centre of its base, from the centre of the prop's footprint on the ground
    pub at: [f64; 3],
    pub color: String,
    /// degrees around the vertical axis
    #[serde(skip_serializing_if = "is_default")]
    pub turn: f64,
}

/// Something to place on maps: trees and flora, rocks, decorations, building pieces. The
/// asset database the map editor picks from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prop {
    pub id: String,
    pub name: String,
    /// "tree", "flora", "rock", "decoration", "fence", "furniture", "stairs", and building
    /// pieces: "door", "window", "wall", "pillar", "floor", "roof", "wall_decoration"
    pub category: String,
    /// the cells it covers: [along x, along z]; a door's or window's width is the first
    pub footprint: [u8; 2],
    /// in cells
    pub height: f64,
    /// windows: how high above the floor they start, in cells
    #[serde(skip_serializing_if = "is_default")]
    pub sill: f64,
    /// walls, floors and roofs: how the stand-in is patterned ("plaster", "planks",
    /// "bricks", "stone", "logs", "tiles", "shingles", "thatch"); its parts' first colour
    /// is the main one, the second the trim or joints
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pattern: String,
    /// the player can't walk through it
    #[serde(skip_serializing_if = "is_default")]
    pub blocks: bool,
    /// its stand-in model
    pub parts: Vec<Part>,
    /// a real model (glTF), when it has one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Prop {
    /// Trees and flora keep their size when placed; everything else can be scaled down.
    pub fn scalable(&self) -> bool {
        !matches!(self.category.as_str(), "tree" | "flora")
    }

    /// The colour of one of its parts ("#rrggbb"), or the first one's.
    pub fn color(&self, part: usize) -> &str {
        self.parts.get(part).or(self.parts.first()).map(|p| p.color.as_str()).unwrap_or("#888888")
    }
}

/// A prop placed on a map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Placed {
    pub prop: String,
    /// where the centre of its footprint stands, in cells from the map's north-west corner
    pub at: [f64; 2],
    pub level: u8,
    /// degrees around the vertical axis
    #[serde(skip_serializing_if = "is_default")]
    pub turn: f64,
    #[serde(skip_serializing_if = "is_one")]
    pub scale: f64,
}

impl Default for Placed {
    fn default() -> Placed {
        Placed { prop: String::new(), at: [0.0, 0.0], level: 0, turn: 0.0, scale: 1.0 }
    }
}

/// Cells the player can't cross, filled with trees placed by the engine from a few
/// parameters (the same trees every time, from the seed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeArea {
    /// its character in the map's `areas` rows
    pub mark: String,
    pub name: String,
    /// the props to pick trees from
    pub trees: Vec<String>,
    /// the least distance between two trunks, in cells
    pub spacing: f64,
    /// the trees' sizes, as scales of the prop: [smallest, tallest]
    pub height: [f64; 2],
    /// 0-1: how much of the room left gets a tree
    pub density: f64,
    /// a tree in every cell along the edge, so it reads as a wall
    pub solid_edge: bool,
    pub seed: u32,
}

impl Default for TreeArea {
    fn default() -> TreeArea {
        TreeArea { mark: "a".into(), name: String::new(), trees: vec![], spacing: 1.0, height: [0.9, 1.15], density: 0.8, solid_edge: true, seed: 1 }
    }
}

/// A map: a grid of cells, each with a terrain and the level its ground is at, and what
/// stands on it. Cells are rows of characters (north first), so a change reads as a
/// changed line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Map {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// cells along x
    pub width: u16,
    /// cells along z (rows)
    pub height: u16,
    /// which terrain each character of `terrain` is
    pub legend: BTreeMap<String, String>,
    pub terrain: Vec<String>,
    /// the level of each cell's ground: 0-9, then a-z
    pub levels: Vec<String>,
    /// the tree area each cell belongs to (its `mark`), '.' for none
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tree_areas: Vec<TreeArea>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Placed>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<crate::building::Building>,
    /// the zone each cell belongs to (its `mark`), '.' for none
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zone_cells: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    /// creatures that come out of a treeline and go roam a zone
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SpawnLink>,
}

/// An area creatures roam (freely, not cell by cell). Who lives there is in the map's
/// `spawns` record, by time of day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Zone {
    /// its character in the map's `zone_cells` rows
    pub mark: String,
    pub name: String,
    /// the most creatures in it at once (its own and the ones that came from treelines)
    pub count: u8,
    /// creatures appear inside it; off: they only come from linked treelines
    pub appear_inside: bool,
    /// seconds a creature stays before it leaves: [least, most]
    pub stay: [f64; 2],
}

impl Default for Zone {
    fn default() -> Zone {
        Zone { mark: "a".into(), name: String::new(), count: 4, appear_inside: true, stay: [20.0, 45.0] }
    }
}

/// Creatures coming out of a treeline (edge cells of a tree area) to roam a zone, then
/// going back into the trees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpawnLink {
    /// the tree area (its mark)
    pub area: String,
    /// the cells of its edge they come out of
    pub treeline: Vec<[i32; 2]>,
    /// the zone they go to (its mark)
    pub zone: String,
    /// seconds between two creatures coming out
    pub every: f64,
    /// seconds a creature roams the zone before going back: [least, most]
    pub stay: [f64; 2],
}

impl Default for SpawnLink {
    fn default() -> SpawnLink {
        SpawnLink { area: String::new(), treeline: vec![], zone: String::new(), every: 6.0, stay: [20.0, 40.0] }
    }
}

/// Who lives in a map's zones, by time of day: the zones' marks -> "morning", "day",
/// "night" -> (creature, level). Kept apart from the map because it names creatures: while
/// it names placeholders, it's saved in `temporary/`, never committed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spawns {
    /// the map's id
    pub id: String,
    pub zones: BTreeMap<String, BTreeMap<String, Vec<(String, u8)>>>,
}

/// A level as a map character: 0-9, then a-z.
pub fn level_char(level: u8) -> char {
    std::char::from_digit(level as u32, 36).unwrap_or('0')
}

pub fn char_level(c: char) -> Option<u8> {
    c.to_digit(36).map(|d| d as u8)
}
