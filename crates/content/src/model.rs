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
    /// nothing goes on it or overlaps its cells, and it goes on or over nothing (stairs,
    /// shelves, beds...)
    #[serde(skip_serializing_if = "is_default")]
    pub unstackable: bool,
    /// its stand-in model
    pub parts: Vec<Part>,
    /// a real model (glTF), when it has one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// a light it gives off (a lamp): see `Light`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<Light>,
    /// the asset browser's folders it's in ("Nature", "Buildings/Inside"...); empty: the
    /// ones its category gives it (see `Prop::folders`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
}

/// A light a prop gives off: a spot (a cone of light) for now. `at` is where it sits, from
/// the prop's origin (its footprint's centre on the ground: x, y up, z), before the prop is
/// turned; `aim` is the way it points. It lights what's in its cone within `range`, fading
/// to nothing at the edge, and casts no shadow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    /// "spot"
    pub kind: String,
    /// "#rrggbb"
    pub color: String,
    pub intensity: f64,
    /// how far it reaches, in cells
    pub range: f64,
    /// the cone's full angle, in degrees (the light fades over its outer third)
    pub cone: f64,
    pub at: [f64; 3],
    pub aim: [f64; 3],
    /// lit at night (and dimly in the morning) only; else always
    pub night: bool,
}

impl Default for Light {
    fn default() -> Self {
        Light { kind: "spot".into(), color: "#ffd59a".into(), intensity: 1.0, range: 6.0, cone: 100.0, at: [0.0, 2.0, 0.0], aim: [0.0, -1.0, 0.0], night: true }
    }
}

/// The folder of what can go inside buildings (the Build tool's Inside list).
pub const INSIDE: &str = "Buildings/Inside";
/// The folder of what can go outside buildings, on their walls or next to them.
pub const OUTSIDE: &str = "Buildings/Outside";

/// The cells something placed at `at` covers: its footprint turned by quarter turns (and
/// `scale` of it), centred there.
pub fn footprint_cells(at: [f64; 2], turn: f64, footprint: [u8; 2], scale: f64) -> Vec<(i32, i32)> {
    let quarter = ((turn / 90.0).round() as i64).rem_euclid(2) == 1;
    let (fw, fd) = if quarter { (footprint[1], footprint[0]) } else { (footprint[0], footprint[1]) };
    let (fw, fd) = (fw.max(1) as f64 * scale, fd.max(1) as f64 * scale);
    let x0 = (at[0] - fw / 2.0 + 0.01).floor() as i32;
    let z0 = (at[1] - fd / 2.0 + 0.01).floor() as i32;
    let x1 = (at[0] + fw / 2.0 - 0.01).floor() as i32;
    let z1 = (at[1] + fd / 2.0 - 0.01).floor() as i32;
    let mut out = vec![];
    for z in z0..=z1 {
        for x in x0..=x1 {
            out.push((x, z));
        }
    }
    out
}

impl Prop {
    /// Trees and flora keep their size when placed; everything else can be scaled down.
    pub fn scalable(&self) -> bool {
        !matches!(self.category.as_str(), "tree" | "flora")
    }

    /// The folders it's in: its own list, or the one its category gives.
    pub fn folders(&self) -> Vec<String> {
        if !self.folders.is_empty() {
            return self.folders.clone();
        }
        let f = match self.category.as_str() {
            "tree" | "flora" | "rock" => "Nature",
            "furniture" | "stairs" => INSIDE,
            "wall_decoration" => OUTSIDE,
            "door" | "window" | "wall" | "pillar" | "floor" | "roof" => "Buildings/Structure",
            "character" => "Characters",
            _ => "Town",
        };
        vec![f.to_string()]
    }

    /// Hung on walls rather than standing on the ground.
    pub fn on_walls(&self) -> bool {
        self.category == "wall_decoration"
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
    /// stored as chunk files this many cells a side (`data/maps/<id>/<cx>_<cz>.json`, see
    /// `Chunk`); 0: the rows below hold the cells
    #[serde(skip_serializing_if = "is_default")]
    pub chunk: u16,
    /// chunked maps: the terrains the chunks' numbers index
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terrains: Vec<String>,
    /// which terrain each character of `terrain` is
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub legend: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terrain: Vec<String>,
    /// the level of each cell's ground: 0-9, then a-z
    #[serde(skip_serializing_if = "Vec::is_empty")]
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

/// Chunk files: how many cells a side.
pub const CHUNK: u16 = 64;

/// One square of a chunked map's cells (`data/maps/<map>/<cx>_<cz>.json`): rows of
/// numbers, north first, as many as the square has within the map. `terrain` indexes the
/// map's `terrains`; `areas` and `zones` are 0 for none, else 1 + the index in the map's
/// `tree_areas` / `zones`. A square with no file is blank: the first terrain, level 0,
/// nothing on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chunk {
    pub map: String,
    /// which square, in chunks: x, z
    pub chunk: [i32; 2],
    pub terrain: Vec<Vec<u16>>,
    pub levels: Vec<Vec<u8>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<Vec<u8>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Vec<u8>>,
}

impl Chunk {
    /// Its file, under a space's folder.
    pub fn path(map: &str, cx: i32, cz: i32) -> String {
        format!("data/maps/{map}/{cx}_{cz}.json")
    }

    /// The square a file name is for ("3_1.json": (3, 1)).
    pub fn parse_name(name: &str) -> Option<(i32, i32)> {
        let (x, z) = name.strip_suffix(".json")?.split_once('_')?;
        Some((x.parse().ok()?, z.parse().ok()?))
    }

    /// Nothing but zeros: not worth a file.
    pub fn is_blank(&self) -> bool {
        self.terrain.iter().flatten().all(|&v| v == 0)
            && self.levels.iter().flatten().all(|&v| v == 0)
            && self.areas.iter().flatten().all(|&v| v == 0)
            && self.zones.iter().flatten().all(|&v| v == 0)
    }
}

/// A map's cells, decoded: per cell (row by row, north first) its terrain (an index into
/// `terrains`), level, tree area (0 none, else 1 + the index in `tree_areas`) and zone (the
/// same way).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cells {
    pub terrains: Vec<String>,
    pub terrain: Vec<u16>,
    pub level: Vec<u8>,
    pub area: Vec<u8>,
    pub zone: Vec<u8>,
}

/// Copies a chunk's rows into a map-sized grid at `at` (cells), `n` cells a side at most.
fn blit<T: Copy>(rows: &[Vec<T>], grid: &mut [T], (w, h): (usize, usize), (ox, oz): (usize, usize), n: usize) {
    for (z, row) in rows.iter().take(n).enumerate() {
        for (x, &v) in row.iter().take(n).enumerate() {
            if ox + x < w && oz + z < h {
                grid[(oz + z) * w + ox + x] = v;
            }
        }
    }
}

impl Map {
    /// Stored as chunk files (else as rows of characters in the record).
    pub fn chunked(&self) -> bool {
        self.chunk > 0
    }

    /// How many chunks along x and along z.
    pub fn chunk_grid(&self) -> (i32, i32) {
        let c = self.chunk.max(1) as i32;
        ((self.width as i32 + c - 1) / c, (self.height as i32 + c - 1) / c)
    }

    /// The cells, decoded: from the rows, or from the chunk files given (a square with no
    /// chunk is blank).
    pub fn cells(&self, chunks: &[Chunk]) -> Cells {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Cells { terrains: vec![], terrain: vec![0; w * h], level: vec![0; w * h], area: vec![0; w * h], zone: vec![0; w * h] };
        if self.chunked() {
            out.terrains = self.terrains.clone();
            let n = self.chunk as usize;
            for c in chunks {
                let at = (c.chunk[0].max(0) as usize * n, c.chunk[1].max(0) as usize * n);
                blit(&c.terrain, &mut out.terrain, (w, h), at, n);
                blit(&c.levels, &mut out.level, (w, h), at, n);
                blit(&c.areas, &mut out.area, (w, h), at, n);
                blit(&c.zones, &mut out.zone, (w, h), at, n);
            }
        } else {
            let mut ids: BTreeMap<char, u16> = BTreeMap::new();
            for (c, t) in &self.legend {
                let Some(c) = c.chars().next() else { continue };
                let i = match out.terrains.iter().position(|x| x == t) {
                    Some(i) => i as u16,
                    None => {
                        out.terrains.push(t.clone());
                        (out.terrains.len() - 1) as u16
                    }
                };
                ids.insert(c, i);
            }
            for (z, row) in self.terrain.iter().take(h).enumerate() {
                for (x, c) in row.chars().take(w).enumerate() {
                    out.terrain[z * w + x] = ids.get(&c).copied().unwrap_or(0);
                }
            }
            let rows = |rows: &[String], grid: &mut [u8], f: &dyn Fn(char) -> u8| {
                for (z, row) in rows.iter().take(h).enumerate() {
                    for (x, c) in row.chars().take(w).enumerate() {
                        grid[z * w + x] = f(c);
                    }
                }
            };
            rows(&self.levels, &mut out.level, &|c| char_level(c).unwrap_or(0));
            let marks: BTreeMap<char, u8> = self.tree_areas.iter().enumerate().filter_map(|(i, a)| a.mark.chars().next().map(|c| (c, i as u8 + 1))).collect();
            rows(&self.areas, &mut out.area, &|c| marks.get(&c).copied().unwrap_or(0));
            let zmarks: BTreeMap<char, u8> = self.zones.iter().enumerate().filter_map(|(i, z)| z.mark.chars().next().map(|c| (c, i as u8 + 1))).collect();
            rows(&self.zone_cells, &mut out.zone, &|c| zmarks.get(&c).copied().unwrap_or(0));
        }
        if out.terrains.is_empty() {
            out.terrains.push("grass".into());
        }
        out
    }

    /// This map with `cells` as rows of characters: the legend's characters kept where it
    /// had them, a terrain's first letter or any free character otherwise.
    pub fn with_rows(&self, cells: &Cells) -> Map {
        let (w, h) = (self.width as usize, self.height as usize);
        let used: std::collections::BTreeSet<u16> = cells.terrain.iter().copied().collect();
        let mut legend: BTreeMap<String, String> = BTreeMap::new();
        let mut chars: BTreeMap<u16, char> = BTreeMap::new();
        for &t in &used {
            let Some(id) = cells.terrains.get(t as usize) else { continue };
            if let Some((c, _)) = self.legend.iter().find(|(c, v)| *v == id && !legend.contains_key(*c)) {
                legend.insert(c.clone(), id.clone());
                chars.insert(t, c.chars().next().unwrap_or('?'));
            }
        }
        for &t in &used {
            if chars.contains_key(&t) {
                continue;
            }
            let Some(id) = cells.terrains.get(t as usize) else { continue };
            let pool = id.chars().next().into_iter().chain('a'..='z').chain('A'..='Z').chain('0'..='9');
            for c in pool {
                if c != '.' && !legend.contains_key(&c.to_string()) {
                    legend.insert(c.to_string(), id.clone());
                    chars.insert(t, c);
                    break;
                }
            }
        }
        let rows = |f: &dyn Fn(usize) -> char| -> Vec<String> { (0..h).map(|z| (0..w).map(|x| f(z * w + x)).collect()).collect() };
        let marks: Vec<char> = self.tree_areas.iter().map(|a| a.mark.chars().next().unwrap_or('a')).collect();
        let zmarks: Vec<char> = self.zones.iter().map(|z| z.mark.chars().next().unwrap_or('a')).collect();
        Map {
            chunk: 0,
            terrains: vec![],
            legend,
            terrain: rows(&|i| chars.get(&cells.terrain[i]).copied().unwrap_or('?')),
            levels: rows(&|i| level_char(cells.level[i])),
            areas: if cells.area.iter().any(|&a| a > 0) { rows(&|i| if cells.area[i] == 0 { '.' } else { marks.get(cells.area[i] as usize - 1).copied().unwrap_or('?') }) } else { vec![] },
            zone_cells: if cells.zone.iter().any(|&z| z > 0) { rows(&|i| if cells.zone[i] == 0 { '.' } else { zmarks.get(cells.zone[i] as usize - 1).copied().unwrap_or('?') }) } else { vec![] },
            ..self.clone()
        }
    }

    /// This map with `cells` as chunk files, `chunk` cells a side (its own size, else
    /// CHUNK): the header record (no rows) and the chunks, blank squares left out.
    pub fn with_chunks(&self, cells: &Cells) -> (Map, Vec<Chunk>) {
        let size = if self.chunk > 0 { self.chunk } else { CHUNK };
        let header = Map { chunk: size, terrains: cells.terrains.clone(), legend: BTreeMap::new(), terrain: vec![], levels: vec![], areas: vec![], zone_cells: vec![], ..self.clone() };
        let (w, h) = (self.width as usize, self.height as usize);
        let n = size as usize;
        let (gx, gz) = header.chunk_grid();
        let mut out = vec![];
        for cz in 0..gz as usize {
            for cx in 0..gx as usize {
                let (x0, z0) = (cx * n, cz * n);
                let (x1, z1) = ((x0 + n).min(w), (z0 + n).min(h));
                let grid = |g: &[u8]| -> Vec<Vec<u8>> {
                    let rows: Vec<Vec<u8>> = (z0..z1).map(|z| g[z * w + x0..z * w + x1].to_vec()).collect();
                    if rows.iter().flatten().any(|&v| v > 0) { rows } else { vec![] }
                };
                let c = Chunk {
                    map: self.id.clone(),
                    chunk: [cx as i32, cz as i32],
                    terrain: (z0..z1).map(|z| cells.terrain[z * w + x0..z * w + x1].to_vec()).collect(),
                    levels: (z0..z1).map(|z| cells.level[z * w + x0..z * w + x1].to_vec()).collect(),
                    areas: grid(&cells.area),
                    zones: grid(&cells.zone),
                };
                if !c.is_blank() {
                    out.push(c);
                }
            }
        }
        (header, out)
    }
}

/// Maps stitched into one world: each piece is a map at a place on the world's grid of
/// cells (its north-west cell); walking off a map's edge goes on in the map there.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct World {
    pub id: String,
    pub name: String,
    pub pieces: Vec<Piece>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Piece {
    pub map: String,
    /// the map's north-west cell on the world's grid: x, z
    pub at: [i32; 2],
}

/// Where two pieces touch: how many cells along the edge, and the cells (on `a`'s side,
/// world coordinates) whose level differs from the cell across, where walking can't cross.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seam {
    pub a: String,
    pub b: String,
    /// where b is, from a: "e" or "s"
    pub side: String,
    /// the first shared cell on a's side (world coordinates); the seam runs south from it
    /// (an "e" seam) or east (an "s" one), `cells` long
    pub from: [i32; 2],
    pub cells: usize,
    pub mismatches: Vec<[i32; 2]>,
}

/// A map's size (cells) and its cells' levels, for the seams.
pub type Levels = (u16, u16, Vec<u8>);

impl World {
    pub fn piece_of(&self, map: &str) -> Option<&Piece> {
        self.pieces.iter().find(|r| r.map == map)
    }

    /// The piece under a world cell (its index) and the cell in its map.
    pub fn at(&self, size: &dyn Fn(&str) -> Option<(u16, u16)>, cell: [i32; 2]) -> Option<(usize, [i32; 2])> {
        self.pieces.iter().enumerate().find_map(|(i, r)| {
            let (w, h) = size(&r.map)?;
            let (x, z) = (cell[0] - r.at[0], cell[1] - r.at[1]);
            (x >= 0 && z >= 0 && x < w as i32 && z < h as i32).then_some((i, [x, z]))
        })
    }

    /// What's wrong: a map that isn't there, one placed twice, pieces overlapping.
    pub fn problems(&self, size: &dyn Fn(&str) -> Option<(u16, u16)>) -> Vec<String> {
        let mut out = vec![];
        let rect = |r: &Piece| size(&r.map).map(|(w, h)| (r.at[0], r.at[1], r.at[0] + w as i32, r.at[1] + h as i32));
        for (i, r) in self.pieces.iter().enumerate() {
            if size(&r.map).is_none() {
                out.push(format!("piece {i}: no map \"{}\"", r.map));
            }
            if self.pieces[..i].iter().any(|o| o.map == r.map) {
                out.push(format!("\"{}\" is placed twice", r.map));
            }
            for o in &self.pieces[..i] {
                if let (Some(a), Some(b)) = (rect(r), rect(o)) {
                    if a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3 {
                        out.push(format!("\"{}\" and \"{}\" overlap", o.map, r.map));
                    }
                }
            }
        }
        out
    }

    /// Where pieces touch, with their level mismatches (`levels`: a map's size and levels;
    /// a map it can't give is left out).
    pub fn seams(&self, levels: &dyn Fn(&str) -> Option<Levels>) -> Vec<Seam> {
        let known: Vec<(&Piece, Levels)> = self.pieces.iter().filter_map(|r| levels(&r.map).map(|l| (r, l))).collect();
        let at = |l: &[u8], i: i32| l.get(i.max(0) as usize).copied().unwrap_or(0);
        let mut out = vec![];
        for (a, (aw, ah, al)) in &known {
            for (b, (bw, bh, bl)) in &known {
                if a.map == b.map {
                    continue;
                }
                let (aw, ah, bw, bh) = (*aw as i32, *ah as i32, *bw as i32, *bh as i32);
                if b.at[0] == a.at[0] + aw {
                    // b east of a
                    let (z0, z1) = (a.at[1].max(b.at[1]), (a.at[1] + ah).min(b.at[1] + bh));
                    if z1 > z0 {
                        let mismatches = (z0..z1).filter(|&z| at(al, (z - a.at[1]) * aw + aw - 1) != at(bl, (z - b.at[1]) * bw)).map(|z| [a.at[0] + aw - 1, z]).collect();
                        out.push(Seam { a: a.map.clone(), b: b.map.clone(), side: "e".into(), from: [a.at[0] + aw - 1, z0], cells: (z1 - z0) as usize, mismatches });
                    }
                }
                if b.at[1] == a.at[1] + ah {
                    // b south of a
                    let (x0, x1) = (a.at[0].max(b.at[0]), (a.at[0] + aw).min(b.at[0] + bw));
                    if x1 > x0 {
                        let mismatches = (x0..x1).filter(|&x| at(al, (ah - 1) * aw + x - a.at[0]) != at(bl, x - b.at[0])).map(|x| [x, a.at[1] + ah - 1]).collect();
                        out.push(Seam { a: a.map.clone(), b: b.map.clone(), side: "s".into(), from: [x0, a.at[1] + ah - 1], cells: (x1 - x0) as usize, mismatches });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_chunks_hold_the_same_cells() {
        let mut m = Map { id: "m".into(), width: 70, height: 20, ..Default::default() };
        m.legend.insert("g".into(), "grass".into());
        m.legend.insert("s".into(), "sand".into());
        m.terrain = (0..20).map(|z| (0..70).map(|x| if (x + z) % 7 == 0 { 's' } else { 'g' }).collect()).collect();
        m.levels = (0..20).map(|z| (0..70).map(|x| level_char(((x / 10 + z / 5) % 4) as u8)).collect()).collect();
        m.tree_areas.push(TreeArea { mark: "a".into(), ..Default::default() });
        m.areas = (0..20).map(|_| (0..70).map(|x| if x < 3 { 'a' } else { '.' }).collect()).collect();
        let cells = m.cells(&[]);
        assert_eq!(cells.terrains, vec!["grass", "sand"]);
        let (header, chunks) = m.with_chunks(&cells);
        assert_eq!(header.chunk, CHUNK);
        assert_eq!(header.chunk_grid(), (2, 1));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].terrain[0].len(), 6, "the second square holds the 6 columns left");
        assert!(chunks[1].areas.is_empty(), "no tree area in the second square");
        assert_eq!(header.cells(&chunks), cells);
        assert_eq!(header.with_rows(&cells).terrain, m.terrain);
        assert_eq!(header.with_rows(&cells).legend, m.legend, "the characters come back");
        let blank = Map { width: 128, height: 64, chunk: CHUNK, terrains: vec!["grass".into()], ..Default::default() };
        assert!(blank.with_chunks(&blank.cells(&[])).1.is_empty(), "blank squares have no file");
        assert_eq!(Chunk::parse_name("3_1.json"), Some((3, 1)));
        assert_eq!(Chunk::parse_name("3-1.json"), None);
    }

    #[test]
    fn worlds_see_overlaps_and_seams() {
        let size = |id: &str| match id {
            "a" => Some((4u16, 3u16)),
            "b" => Some((2, 3)),
            "c" => Some((4, 2)),
            _ => None,
        };
        let w = World {
            id: "w".into(),
            pieces: vec![Piece { map: "a".into(), at: [0, 0] }, Piece { map: "b".into(), at: [4, 0] }, Piece { map: "c".into(), at: [0, 3] }, Piece { map: "x".into(), at: [9, 9] }],
            ..Default::default()
        };
        let p = w.problems(&size);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("no map \"x\""));
        assert_eq!(w.at(&size, [5, 2]), Some((1, [1, 2])));
        assert_eq!(w.at(&size, [8, 0]), None);
        let levels = |id: &str| match id {
            "a" => Some((4, 3, vec![0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0])),
            "b" => Some((2, 3, vec![1, 0, 1, 0, 0, 0])),
            "c" => Some((4, 2, vec![0, 0, 5, 0, 0, 0, 0, 0])),
            _ => None,
        };
        let seams = w.seams(&levels);
        assert_eq!(seams.len(), 2, "{seams:?}");
        let e = seams.iter().find(|s| s.b == "b").unwrap();
        assert_eq!((e.side.as_str(), e.cells, e.mismatches.len()), ("e", 3, 0), "a's east column 1, 1, 0 meets b's west column 1, 1, 0");
        let s = seams.iter().find(|s| s.b == "c").unwrap();
        assert_eq!((s.side.as_str(), s.cells, s.mismatches.clone()), ("s", 4, vec![[2, 2]]), "c rises to 5 under a's south row");
        let mut overlapping = w.clone();
        overlapping.pieces[1].at = [3, 0];
        assert!(overlapping.problems(&size).iter().any(|s| s.contains("overlap")));
    }
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
