//! The records, one file each: `data/<collection>/<id>.json`. Everything refers to other
//! records by id (lowercase slugs). Asset paths are relative to the record's space's
//! `assets/` folder. Mechanics are named by `effect` keys the engine knows, never by ids
//! or names, so a renamed or new record can reuse a mechanic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Functions saying whether a field is as its record's own default has it (`$record::default()`:
/// left out of the record then, read back the same).
macro_rules! as_default {
    ($record:ty: $($name:ident => $field:ident: $ty:ty),* $(,)?) => {$(
        fn $name(v: &$ty) -> bool {
            *v == <$record>::default().$field
        }
    )*};
}

as_default!(Growth: growth_density => density: f64, growth_scale => scale: [f64; 2], growth_patchy => patchy: f64, growth_turn => turn: bool);
as_default!(Look: look_scale => scale: f64, look_sharpness => sharpness: f64, look_balance => balance: f64, look_grain => grain: f64);
as_default!(Layer: layer_scale => scale: f64, layer_amount => amount: f64, layer_softness => softness: f64);
as_default!(Meadow: meadow_density => density: f64, meadow_patchy => patchy: f64, meadow_clump => clump: f64, meadow_clumping => clumping: f64);
as_default!(Plant:
    plant_weight => weight: f64,
    plant_height => height: [f64; 2],
    plant_scale => scale: [f64; 2],
    plant_width => width: f64,
    plant_lean => lean: f64,
    plant_bend => bend: f64,
    plant_sway => sway: f64,
    plant_petals => petals: u8,
    plant_petal => petal: String,
    plant_heart => heart: String,
    plant_bloom => bloom: f64,
);

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
    /// a light it carries (a tail light, a lantern): see `Light`; `at` and `aim` are from
    /// its feet, +z the way it faces
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<Light>,
    /// a sound it makes as it goes about (looping, from its feet): see `Sound`. Its `cry`
    /// is the one-shot it gives when a battle starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<Sound>,
    /// drawn with the toon look (the world settings' `toon`): ink lines round its picture
    #[serde(skip_serializing_if = "is_default")]
    pub toon: bool,
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
    /// what it gains at each move level past 1 (on top of the world's own growth)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<MoveLevel>,
    /// the stronger abilities it becomes (made separately, named here)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evolutions: Vec<MoveEvolution>,
    /// how it plays out: animations, FX, sounds and the player's inputs, in time
    #[serde(skip_serializing_if = "is_default")]
    pub timeline: Timeline,
}

/// What an ability gains at a move level: a field at 0 is unchanged from the level before
/// (and a level not listed changes nothing of its own).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveLevel {
    /// 2 to 5
    pub level: u8,
    #[serde(skip_serializing_if = "is_default")]
    pub power: u16,
    #[serde(skip_serializing_if = "is_default")]
    pub accuracy: u16,
    #[serde(skip_serializing_if = "is_default")]
    pub effect_chance: u16,
    /// hits per use (an ability that strikes several times)
    #[serde(skip_serializing_if = "is_default")]
    pub hits: u8,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// A stronger ability this one becomes, its own record: brought about by reaching a move
/// level, by an item, or both.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveEvolution {
    /// the ability it becomes
    pub into: String,
    /// the move level that brings it about (0: an item alone)
    #[serde(skip_serializing_if = "is_default")]
    pub at_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// How an ability plays out, in time, from the moment it starts: the user's and the
/// target's animations and motions, FX, sounds, the player's inputs, the camera.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timeline {
    /// how long it all takes, in seconds
    pub seconds: f64,
    pub clips: Vec<Clip>,
}

/// One thing on a timeline, at `at` seconds for `seconds` (0: an instant, or the file's own
/// length), on `who` ("user" or "target"). `kind` says what `what` names: "animation" (one
/// of the model's animations), "motion" (lunge, hop, shake, pulse, flash; `value`: how far
/// or hard), "fx" (an fx record, anchored on `who`), "sound" (a file under assets/;
/// `value`: volume), "input" (tap, cast, block, charge, rhythm; `value`: the window in ms,
/// `count`: rhythm beats), "camera" (shake, zoom; `value`: how much).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub kind: String,
    pub who: String,
    pub at: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub seconds: f64,
    pub what: String,
    #[serde(skip_serializing_if = "is_default")]
    pub value: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub count: u8,
}

impl Default for Clip {
    fn default() -> Clip {
        Clip { kind: "animation".into(), who: "user".into(), at: 0.0, seconds: 0.0, what: String::new(), value: 0.0, count: 0 }
    }
}

pub const CLIP_KINDS: [&str; 6] = ["animation", "motion", "fx", "sound", "input", "camera"];
pub const MOTIONS: [&str; 5] = ["lunge", "hop", "shake", "pulse", "flash"];
pub const INPUTS: [&str; 5] = ["tap", "cast", "block", "charge", "rhythm"];
pub const CAMERAS: [&str; 2] = ["shake", "zoom"];

impl Timeline {
    /// What's wrong with it (empty when nothing); `fx` says whether an fx record exists.
    pub fn problems(&self, fx: &dyn Fn(&str) -> bool) -> Vec<String> {
        let mut out = vec![];
        if self.seconds < 0.0 || self.seconds > 60.0 {
            out.push(format!("a timeline of {} s: 0 to 60", self.seconds));
        }
        for (i, c) in self.clips.iter().enumerate() {
            let at = format!("clip {i} ({} {})", c.kind, c.what);
            if !CLIP_KINDS.contains(&c.kind.as_str()) {
                out.push(format!("{at}: kind \"{}\": {}", c.kind, CLIP_KINDS.join(", ")));
            }
            if !matches!(c.who.as_str(), "user" | "target") {
                out.push(format!("{at}: on \"{}\": user or target", c.who));
            }
            if c.at < 0.0 || c.at > self.seconds.max(0.0) || c.seconds < 0.0 {
                out.push(format!("{at}: at {} s for {} s, off the timeline", c.at, c.seconds));
            }
            let named = match c.kind.as_str() {
                "motion" => MOTIONS.contains(&c.what.as_str()),
                "input" => INPUTS.contains(&c.what.as_str()),
                "camera" => CAMERAS.contains(&c.what.as_str()),
                "fx" => fx(&c.what),
                _ => !c.what.is_empty(),
            };
            if !named {
                out.push(format!("{at}: no such {}", if c.kind == "fx" { "fx record" } else { "one" }));
            }
        }
        out
    }
}

/// An FX: hand-drawn frames on a few simple meshes placed round what it's anchored on
/// (its feet: x right, y up, z forward, in cells). Each mesh plays its own flipbook, a
/// sheet of `frames` pictures `size` pixels square, `columns` across, at `fps`; the
/// frames as drawn are its `source` sheet, and `sheet` is what the game draws (the source
/// until it's exported at `export_size` pixels).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fx {
    pub id: String,
    pub name: String,
    pub fps: f64,
    pub frames: u8,
    pub size: u16,
    pub export_size: u16,
    pub meshes: Vec<FxMesh>,
}

impl Default for Fx {
    fn default() -> Fx {
        Fx { id: String::new(), name: String::new(), fps: 12.0, frames: 8, size: 128, export_size: 64, meshes: vec![] }
    }
}

/// One of an FX's meshes: "billboard" (upright, turning to face the camera), "quad" (flat,
/// turned as set), "sphere", "dome" (a half sphere, open underneath) or "ring"; placed
/// from the anchor's feet (x right, y up, z forward: a little in front of it, where it
/// isn't hidden by its body), sized and turned (degrees about x, y, z).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FxMesh {
    pub shape: String,
    pub at: [f64; 3],
    pub size: [f64; 3],
    pub turn: [f64; 3],
    /// its frames as drawn (a PNG under assets/, `columns` across)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// the flipbook the game draws: an export of the source (empty: the source itself)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sheet: String,
    pub columns: u8,
    /// pixel-sharp (else smoothed)
    #[serde(skip_serializing_if = "is_default")]
    pub pixelated: bool,
    /// lit like the world; else full bright, as drawn (a glow, a flash)
    #[serde(skip_serializing_if = "is_default")]
    pub lit: bool,
}

impl Default for FxMesh {
    fn default() -> FxMesh {
        FxMesh { shape: "billboard".into(), at: [0.0, 0.8, 0.6], size: [1.2, 1.2, 1.2], turn: [0.0; 3], source: String::new(), sheet: String::new(), columns: 4, pixelated: false, lit: false }
    }
}

pub const FX_SHAPES: [&str; 5] = ["billboard", "quad", "sphere", "dome", "ring"];

impl FxMesh {
    /// The flipbook the game draws.
    pub fn drawn(&self) -> &str {
        if self.sheet.is_empty() { &self.source } else { &self.sheet }
    }
}

impl Fx {
    /// What's wrong with it (empty when nothing).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if !(self.fps > 0.0 && self.fps <= 60.0) {
            out.push(format!("{} frames a second: over 0, up to 60", self.fps));
        }
        if self.frames == 0 || self.frames > 64 {
            out.push(format!("{} frames: 1 to 64", self.frames));
        }
        for (what, s) in [("size", self.size), ("export_size", self.export_size)] {
            if !matches!(s, 32 | 64 | 128 | 256 | 512) {
                out.push(format!("{what} {s}: 32, 64, 128, 256 or 512 pixels"));
            }
        }
        for (i, m) in self.meshes.iter().enumerate() {
            if !FX_SHAPES.contains(&m.shape.as_str()) {
                out.push(format!("mesh {i}: shape \"{}\": {}", m.shape, FX_SHAPES.join(", ")));
            }
            if m.columns == 0 {
                out.push(format!("mesh {i}: no columns"));
            }
            if m.size.iter().any(|&s| s <= 0.0) {
                out.push(format!("mesh {i}: a size of {:?}", m.size));
            }
        }
        out
    }
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

/// How long a creature's own name may be, in letters.
pub const NAME_MAX: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub creature: String,
    /// its own name, if it was given one (empty: its kind's), `NAME_MAX` letters at most
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
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

/// How many levels of ground a building's floor spans: a floor is a character's height and
/// some room above it, a level of ground half that (one cell).
pub const FLOOR_LEVELS: u8 = 2;

/// The world's measures, the same on every map (one record, id "world"). A cell is the
/// world's unit; a floor of a building is the main character's height plus some room above
/// it, and a level (one step of elevation) is half a floor (`FLOOR_LEVELS`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub id: String,
    /// in cells
    pub character_height: f64,
    /// room above the character in a building's floor, in cells
    pub level_padding: f64,
    /// the highest level a map can use
    pub max_level: u8,
    /// how what's toon looks (props and creatures say whether they are)
    #[serde(skip_serializing_if = "is_default")]
    pub toon: Toon,
    /// where a new game begins (the map editor's "Start new games here")
    #[serde(skip_serializing_if = "is_default")]
    pub start: Start,
    /// the wind (the Post-processing window)
    #[serde(skip_serializing_if = "is_default")]
    pub wind: Wind,
    /// a creature's lifeforce at most, a gauge (100): a knock-out costs some (its level, up
    /// to `lifeforce_ko_cap`), and with none left the creature is lost for good
    pub lifeforce: u8,
    /// the most lifeforce a knock-out costs: a creature loses its level's worth, up to this
    /// (20: from level 20 on, 20 a knock-out)
    pub lifeforce_ko_cap: u8,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { id: "world".into(), character_height: 1.5, level_padding: 0.5, max_level: 9, toon: Toon::default(), start: Start::default(), wind: Wind::default(), lifeforce: 100, lifeforce_ko_cap: 20 }
    }
}

/// The world's wind: the way it blows, how strong, how gusty. It moves what's painted for it
/// (props' and creatures' models, a stand-in's `wind`), the tall grass, the water's ripples
/// and waves, and the clouds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wind {
    /// the way it blows to, in degrees clockwise from north on the map (north is -z): 90 is
    /// east, 180 south
    pub direction: f64,
    /// 0 (calm) to 1 (a gale)
    pub strength: f64,
    /// how much of it comes in gusts: 0 (steady) to 1
    pub gusts: f64,
}

impl Default for Wind {
    /// A breeze going east and a little south (the way the clouds always drifted).
    fn default() -> Wind {
        Wind { direction: 112.0, strength: 0.35, gusts: 0.5 }
    }
}

impl Wind {
    /// The way it blows on the map's ground, a unit long: (x, z).
    pub fn way(&self) -> [f64; 2] {
        let a = self.direction.to_radians();
        [a.sin(), -a.cos()]
    }

    /// What's wrong with it (empty when nothing).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if !(0.0..=360.0).contains(&self.direction) {
            out.push(format!("direction {}: 0 to 360 degrees", self.direction));
        }
        for (what, v) in [("strength", self.strength), ("gusts", self.gusts)] {
            if !(0.0..=1.0).contains(&v) {
                out.push(format!("{what} {v}: 0 to 1"));
            }
        }
        out
    }
}

/// Where a new game begins: a map, the cell the player stands on and the way they face
/// (degrees, as the walker turns). No map: the first map, wherever the walk starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Start {
    pub map: String,
    pub at: [i32; 2],
    pub facing: f64,
}

/// The toon look (the Post-processing window): the sun's and the lamps' light in steps, the
/// shade tinted, a rim of light round the edge and a glint where the sun catches; and ink
/// lines round what's toon, where it stands in front of what's behind it, and along its
/// sharp folds. And the colour steps: the whole picture's light in bands, toon or not.
/// Colours "#rrggbb".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toon {
    /// steps of light from dark to lit, 1 to 5
    pub bands: u8,
    /// how soft a step's edge is, 0 (hard) to 1
    pub softness: f64,
    /// the colour the shade turns towards, and how far (0 to 1)
    pub shadow_color: String,
    pub shadow: f64,
    /// the rim of light: how bright (0: none) and how wide (0 to 1)
    pub rim: f64,
    pub rim_width: f64,
    /// the glint: how bright (0: none) and how big (0 to 1)
    pub glint: f64,
    pub glint_size: f64,
    /// the ink lines: on or off, their colour, how wide (pixels) and how opaque
    pub ink: bool,
    pub ink_color: String,
    pub ink_width: f64,
    pub ink_opacity: f64,
    /// how far behind what's around it must be for a line, as a share of its distance
    pub depth_step: f64,
    /// a fold sharper than this many degrees gets a line (0: none)
    pub crease: f64,
    /// the whole picture's light in this many steps, toon or not (0: smooth, else 2 to
    /// 16), and how soft their edges are (0 to 1)
    pub color_steps: u8,
    pub color_softness: f64,
    /// clouds' shadows drifting over the world, toon or not (a post-process): how dark they
    /// make what's in the sun (0: none, to 1), how wide a cloud is (cells), how fast they
    /// drift (cells a second) and how much of the sky they cover (0 to 1)
    pub cloud_shadow: f64,
    pub cloud_size: f64,
    pub cloud_speed: f64,
    pub cloud_cover: f64,
}

impl Toon {
    /// What's wrong with it (empty when nothing).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if !(1..=5).contains(&self.bands) {
            out.push(format!("{} bands: 1 to 5", self.bands));
        }
        for (what, c) in [("shadow_color", &self.shadow_color), ("ink_color", &self.ink_color)] {
            if c.len() != 7 || !c.starts_with('#') || u32::from_str_radix(&c[1..], 16).is_err() {
                out.push(format!("{what} \"{c}\" isn't #rrggbb"));
            }
        }
        if self.color_steps == 1 || self.color_steps > 16 {
            out.push(format!("{} colour steps: 0 (none) or 2 to 16", self.color_steps));
        }
        let shares = [
            ("softness", self.softness),
            ("shadow", self.shadow),
            ("rim_width", self.rim_width),
            ("glint_size", self.glint_size),
            ("ink_opacity", self.ink_opacity),
            ("color_softness", self.color_softness),
            ("cloud_shadow", self.cloud_shadow),
            ("cloud_cover", self.cloud_cover),
        ];
        for (what, v) in shares {
            if !(0.0..=1.0).contains(&v) {
                out.push(format!("{what} {v}: 0 to 1"));
            }
        }
        for (what, v) in [("rim", self.rim), ("glint", self.glint), ("depth_step", self.depth_step)] {
            if !(0.0..=4.0).contains(&v) {
                out.push(format!("{what} {v}: 0 to 4"));
            }
        }
        if !(0.0..=12.0).contains(&self.ink_width) {
            out.push(format!("ink_width {}: 0 to 12 pixels", self.ink_width));
        }
        if !(0.0..=179.0).contains(&self.crease) {
            out.push(format!("crease {}: 0 (none) to 179 degrees", self.crease));
        }
        if !(4.0..=200.0).contains(&self.cloud_size) {
            out.push(format!("cloud_size {}: 4 to 200 cells", self.cloud_size));
        }
        if !(0.0..=10.0).contains(&self.cloud_speed) {
            out.push(format!("cloud_speed {}: 0 to 10 cells a second", self.cloud_speed));
        }
        out
    }
}

impl Default for Toon {
    fn default() -> Toon {
        Toon {
            bands: 2,
            softness: 0.12,
            shadow_color: "#5c4d8a".into(),
            shadow: 0.35,
            rim: 0.35,
            rim_width: 0.3,
            glint: 0.3,
            glint_size: 0.2,
            ink: true,
            ink_color: "#2b2633".into(),
            ink_width: 2.5,
            ink_opacity: 0.9,
            depth_step: 0.04,
            crease: 50.0,
            color_steps: 0,
            color_softness: 0.15,
            cloud_shadow: 0.45,
            cloud_size: 26.0,
            cloud_speed: 0.8,
            cloud_cover: 0.45,
        }
    }
}

impl Settings {
    /// A level's height (one step of elevation), in cells: half a floor's.
    pub fn level_height(&self) -> f64 {
        self.floor_height() / FLOOR_LEVELS as f64
    }

    /// A building's floor's height, in cells: a character's and some room above it.
    pub fn floor_height(&self) -> f64 {
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
    /// the group it's listed in with the terrains like it (none: its pattern's, `PATTERNS`)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// "#rrggbb": the main colour, and the one its pattern mixes in
    pub color: String,
    pub color2: String,
    /// how it's drawn: one of `PATTERNS` ("grass", "meadow", "tall", "dirt", "sand",
    /// "stones", "rock", "water"), or "custom": as its `look` says
    pub pattern: String,
    /// a look of its own (`pattern` "custom"): its two colours in blotches, layers over them
    #[serde(skip_serializing_if = "Look::is_plain")]
    pub look: Look,
    /// the cliff face under its cells
    pub side: String,
    /// "walk", "water" (needs surfing) or "block"
    pub walk: String,
    /// where two terrains meet, the one with the higher priority reaches further
    #[serde(skip_serializing_if = "is_default")]
    pub priority: i16,
    /// water, and how it moves: "inland" (lakes, ponds, rivers: ripples going with the wind,
    /// small waves landing on the shore it blows towards) or "oceanic" (the sea: waves always
    /// rolling in to the land, stronger when the wind blows to land too). A terrain drawn as
    /// water (its pattern) is inland unless it says so.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub water: String,
    /// where water meets it, the water washes up onto it and back, as onto sand
    #[serde(skip_serializing_if = "is_default")]
    pub beach: bool,
    /// what grows through its cells: on tall ground (`pattern` "tall") grass props in place of
    /// the engine's own tufts; on any ground but water, a meadow of blades and flowers the
    /// engine grows itself (`Growth::meadow`; nothing listed: tall ground grows its tufts,
    /// other ground nothing)
    #[serde(skip_serializing_if = "Growth::is_empty")]
    pub growth: Growth,
}

/// How terrains are drawn (`Terrain::pattern`), each with the name its group goes by: the
/// engine's own patterns, then a look of a terrain's own.
pub const PATTERNS: [(&str, &str); 9] = [
    ("grass", "Grass"),
    ("meadow", "Meadow"),
    ("tall", "Tall grass"),
    ("dirt", "Dirt"),
    ("sand", "Sand"),
    ("stones", "Stones"),
    ("rock", "Rock"),
    ("water", "Water"),
    ("custom", "Its own look"),
];

impl Terrain {
    /// The group it's listed in: its own category, else its pattern's name (a look of its own:
    /// its name).
    pub fn category_name(&self) -> String {
        if !self.category.is_empty() {
            return self.category.clone();
        }
        match PATTERNS.iter().find(|(id, _)| *id == self.pattern) {
            Some((id, _)) if *id == "custom" => self.name.clone(),
            Some((_, name)) => name.to_string(),
            None => self.pattern.clone(),
        }
    }
}

/// A terrain's look of its own (`pattern` "custom"): its two colours (`color`, `color2`) in
/// blotches so big, so sharp, so much of the second, a grain of small specks; maybe glowing
/// (lava, magic), drifting along `angle` and pulsing; how glossy it is (wet, icy); and up to
/// `Look::LAYERS` layers over it in order (`Layer`): stones, cracks, dots, veins...
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    /// how big its blotches are (cells), how sharp their edges (0 soft, 1 sharp), how much of
    /// it is its second colour (0 none, 1 all)
    #[serde(skip_serializing_if = "look_scale")]
    pub scale: f64,
    #[serde(skip_serializing_if = "look_sharpness")]
    pub sharpness: f64,
    #[serde(skip_serializing_if = "look_balance")]
    pub balance: f64,
    /// how much its small specks vary in brightness (0 none, 1 much)
    #[serde(skip_serializing_if = "look_grain")]
    pub grain: f64,
    /// how glossy it is (0 matte, 1 wet or glassy: it catches the sun)
    #[serde(skip_serializing_if = "is_default")]
    pub shine: f64,
    /// how much it glows of itself, its colours lit by nothing (0 none, up to 4: molten), how
    /// fast its glow pulses (0 steady, up to 2), and how fast it drifts along `angle` (cells a
    /// second, up to 2: flowing)
    #[serde(skip_serializing_if = "is_default")]
    pub glow: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub pulse: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub flow: f64,
    /// the way it drifts (degrees, clockwise from east seen from above)
    #[serde(skip_serializing_if = "is_default")]
    pub angle: f64,
    /// its layers, the first drawn first
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
}

impl Default for Look {
    fn default() -> Self {
        Look { scale: 3.0, sharpness: 0.3, balance: 0.5, grain: 0.2, shine: 0.0, glow: 0.0, pulse: 0.0, flow: 0.0, angle: 0.0, layers: vec![] }
    }
}

impl Look {
    /// As every look starts: nothing of its own (left out of the record).
    pub fn is_plain(&self) -> bool {
        *self == Look::default()
    }

    /// The most layers a look has (more are left out).
    pub const LAYERS: usize = 4;
}

/// What a look's layer is: blotches of its colour, stones (cobbles, plates, crusts: cells
/// with gaps between them), cracks (a net of thin lines), dots (pebbles, leaves, sparks,
/// crystals), streaks (long marks along `angle`), veins (thin wavering lines), ripples (waves
/// across `angle`, as on dunes).
pub const LAYER_SHAPES: [&str; 7] = ["patches", "stones", "cracks", "dots", "streaks", "veins", "ripples"];

/// A layer of a look (`Look::layers`): its shape, its colour, how big what it's made of is
/// (cells), how much of the ground it covers (0 to 1), how soft its edges (0 crisp, 1 soft),
/// how much it glows of itself (up to 4), how fast that pulses and how fast it drifts along
/// `angle`, and its relief (sunken below 0, raised above, up to 1 either way: for the light).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub shape: String,
    pub color: String,
    #[serde(skip_serializing_if = "layer_scale")]
    pub scale: f64,
    #[serde(skip_serializing_if = "layer_amount")]
    pub amount: f64,
    #[serde(skip_serializing_if = "layer_softness")]
    pub softness: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub glow: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub pulse: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub flow: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub angle: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub relief: f64,
}

impl Default for Layer {
    fn default() -> Self {
        Layer { shape: "patches".into(), color: "#808080".into(), scale: 1.0, amount: 0.3, softness: 0.3, glow: 0.0, pulse: 0.0, flow: 0.0, angle: 0.0, relief: 0.0 }
    }
}

/// What grows through a terrain's cells. On tall ground, in place of the engine's own tufts:
/// grass props (the "grass" category), each picked as often as its weight says among them.
/// Scattered: so many to a cell on average, fewer where the ground thins in patches; each
/// turned and sized its own way, the same each time. Tiled (grass made to fill its cells, as a
/// tile is): each whole over its footprint's cells, lined up with them, so copies side by side
/// join. On any ground but water, a meadow (`Meadow`): blades and flowers the engine grows
/// itself, with the props or alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Growth {
    /// the grass props, with how often each is picked
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Grower>,
    /// how many to a cell, on average (0 to 32; scattered)
    #[serde(skip_serializing_if = "growth_density")]
    pub density: f64,
    /// how much smaller and how much bigger than its prop each one can be (scattered)
    #[serde(skip_serializing_if = "growth_scale")]
    pub scale: [f64; 2],
    /// how much the ground thins in patches (0: as thick everywhere; 1: bare in places;
    /// scattered)
    #[serde(skip_serializing_if = "growth_patchy")]
    pub patchy: f64,
    /// each turned its own way (else all as their props are made; tiled, by half or quarter
    /// turns, so it still covers its cells)
    #[serde(skip_serializing_if = "growth_turn")]
    pub turn: bool,
    /// tiled: each whole over its footprint's cells, lined up with them (else scattered)
    #[serde(skip_serializing_if = "is_default")]
    pub tiled: bool,
    /// the meadow the engine grows itself (no plants: none)
    #[serde(skip_serializing_if = "Meadow::is_empty")]
    pub meadow: Meadow,
}

impl Default for Growth {
    fn default() -> Self {
        Growth { props: vec![], density: 4.0, scale: [0.8, 1.25], patchy: 0.35, turn: true, tiled: false, meadow: Meadow::default() }
    }
}

impl Growth {
    /// Nothing listed to grow: on tall ground the engine's own tufts instead.
    pub fn is_empty(&self) -> bool {
        self.props.is_empty() && self.meadow.is_empty()
    }

    /// Grass props listed to grow (on tall ground).
    pub fn grows_props(&self) -> bool {
        !self.props.is_empty()
    }

    /// The most props to a cell.
    pub const MOST: f64 = 32.0;
}

/// A meadow: blades of grass and flowers the engine grows itself through a terrain's cells,
/// made on the GPU as they're drawn. So many to a cell, each one of its plants (picked by their
/// weights; one that keeps to clusters, mostly in its own), in clumps of blades alike, thinner
/// in patches; the same every time. Up close every blade is drawn in full; further off fewer,
/// each wider so the meadow looks as full and drawn more simply, and at the furthest shorter
/// and shorter until only the ground's colour is left. Its plants' looks (how tall, wide,
/// leaning, curved, coloured, swaying) change it as it's drawn; what grows where (how many,
/// which, the clumps and the patches) once its cells are built again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meadow {
    /// its plants, each with how often it grows
    pub plants: Vec<Plant>,
    /// how many to a cell, on average (0 to `MOST`)
    #[serde(skip_serializing_if = "meadow_density")]
    pub density: f64,
    /// how much it thins in patches a few cells across (0: as thick everywhere; 1: bare in
    /// places)
    #[serde(skip_serializing_if = "meadow_patchy")]
    pub patchy: f64,
    /// its clumps: how wide (cells; 0: none), and how alike the blades of one are (0 to 1: as
    /// tall, leaning the same way out from its middle)
    #[serde(skip_serializing_if = "meadow_clump")]
    pub clump: f64,
    #[serde(skip_serializing_if = "meadow_clumping")]
    pub clumping: f64,
    /// lit in the toon look's steps (as toon props are), else as the ground under it is
    #[serde(skip_serializing_if = "is_default")]
    pub toon: bool,
}

impl Default for Meadow {
    fn default() -> Self {
        Meadow { plants: vec![], density: 48.0, patchy: 0.3, clump: 0.6, clumping: 0.5, toon: false }
    }
}

impl Meadow {
    /// No plants: no meadow.
    pub fn is_empty(&self) -> bool {
        self.plants.is_empty()
    }

    /// The most to a cell.
    pub const MOST: f64 = 128.0;

    /// The most petals a flower may have.
    pub const PETALS: u8 = 8;
}

/// Whether a colour is written "#rrggbb".
pub fn is_hex_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit())
}

/// What a meadow's plant is: a blade of grass, a flower (a stem with a head of petals round its
/// heart), both made by the engine, or a grass prop's model (one blade or a few, a flower: as
/// it's made, placed as the meadow's own are and drawn thinner further off as they are).
pub const PLANT_SHAPES: [&str; 3] = ["blade", "flower", "model"];

/// A kind of plant in a meadow (`Meadow`), and how often it grows. Its record leaves out what's
/// as a plant has it by default (a blade writes no petals).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plant {
    /// what it's called in the editor (none: by its shape, or its model's prop)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// what it is: "blade", "flower" or "model" (`PLANT_SHAPES`)
    pub shape: String,
    /// a model's: its grass prop
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prop: String,
    /// how often it grows, against the others
    #[serde(skip_serializing_if = "plant_weight")]
    pub weight: f64,
    /// a blade's or a flower's: how tall (cells), the shortest and the tallest
    #[serde(skip_serializing_if = "plant_height")]
    pub height: [f64; 2],
    /// a model's: how big against its prop as made, the smallest and the biggest
    #[serde(skip_serializing_if = "plant_scale")]
    pub scale: [f64; 2],
    /// how wide at its foot (cells; a flower's stem)
    #[serde(skip_serializing_if = "plant_width")]
    pub width: f64,
    /// how far over it leans (0: straight up; 1: lying down)
    #[serde(skip_serializing_if = "plant_lean")]
    pub lean: f64,
    /// how much it curves over as it rises (0: straight)
    #[serde(skip_serializing_if = "plant_bend")]
    pub bend: f64,
    /// "#rrggbb": at its foot and at its tip (empty: its terrain's colours)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tip: String,
    /// how much the wind moves it (0: not at all; 1: as grass)
    #[serde(skip_serializing_if = "plant_sway")]
    pub sway: f64,
    /// a flower's: how many petals (3 to `Meadow::PETALS`), their colour, its heart's colour,
    /// how wide its head is (cells)
    #[serde(skip_serializing_if = "plant_petals")]
    pub petals: u8,
    #[serde(skip_serializing_if = "plant_petal")]
    pub petal: String,
    #[serde(skip_serializing_if = "plant_heart")]
    pub heart: String,
    #[serde(skip_serializing_if = "plant_bloom")]
    pub bloom: f64,
    /// how much it keeps to clusters of its own a few cells across (0: grows anywhere; 1: only
    /// in them)
    #[serde(skip_serializing_if = "is_default")]
    pub clusters: f64,
    /// how much it glows of itself, lit by nothing (0 none, up to 4): a flower its petals and
    /// heart, a blade towards its tip
    #[serde(skip_serializing_if = "is_default")]
    pub glow: f64,
}

impl Default for Plant {
    fn default() -> Self {
        Plant {
            name: String::new(),
            shape: "blade".into(),
            prop: String::new(),
            weight: 1.0,
            height: [0.35, 0.6],
            scale: [0.8, 1.25],
            width: 0.035,
            lean: 0.25,
            bend: 0.4,
            color: String::new(),
            tip: String::new(),
            sway: 1.0,
            petals: 5,
            petal: "#f4f1e8".into(),
            heart: "#e8b923".into(),
            bloom: 0.08,
            clusters: 0.0,
            glow: 0.0,
        }
    }
}

/// A grass prop grown through tall ground, and how often it's picked (its weight against the
/// others').
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grower {
    pub prop: String,
    pub weight: f64,
}

impl Default for Grower {
    fn default() -> Self {
        Grower { prop: String::new(), weight: 1.0 }
    }
}

/// A jump between two cells in a line with one cell between them, on one level: the player
/// jumps from one to the other over the cell between (both ways, unless `one_way`: from `from`
/// to `to` only). Each end it's jumped from has a little ramp at its edge poking out towards
/// the other; its ends keep their level while it's there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jump {
    pub from: [i32; 2],
    pub to: [i32; 2],
    /// jumped from `from` to `to` only, never back
    #[serde(skip_serializing_if = "is_default")]
    pub one_way: bool,
}

impl Jump {
    /// Whether two cells can be a jump's ends: in a line (along x or z) with one cell between.
    pub fn spans(a: [i32; 2], b: [i32; 2]) -> bool {
        matches!(((b[0] - a[0]).abs(), (b[1] - a[1]).abs()), (2, 0) | (0, 2))
    }

    /// The cell between its ends.
    pub fn between(&self) -> [i32; 2] {
        [(self.from[0] + self.to[0]) / 2, (self.from[1] + self.to[1]) / 2]
    }

    /// Whether a cell is one of its ends.
    pub fn has(&self, c: [i32; 2]) -> bool {
        self.from == c || self.to == c
    }

    /// Where one lands jumping from `c` (one of its ends), if it's jumped from there.
    pub fn from_cell(&self, c: [i32; 2]) -> Option<[i32; 2]> {
        if c == self.from {
            Some(self.to)
        } else if c == self.to && !self.one_way {
            Some(self.from)
        } else {
            None
        }
    }

    /// The way from one end to the other, a cell at a time (east: 1, 0).
    pub fn way_from(&self, c: [i32; 2]) -> (i32, i32) {
        let other = if c == self.from { self.to } else { self.from };
        ((other[0] - c[0]).signum(), (other[1] - c[1]).signum())
    }

    /// The same two cells, whichever way round.
    pub fn joins(&self, a: [i32; 2], b: [i32; 2]) -> bool {
        (self.from == a && self.to == b) || (self.from == b && self.to == a)
    }
}

/// The kinds of water (`Terrain::water`).
pub const WATERS: [&str; 2] = ["inland", "oceanic"];

impl Terrain {
    /// The kind of water it is, if it's water: "inland" or "oceanic".
    pub fn water_kind(&self) -> Option<&str> {
        match self.water.as_str() {
            "" if self.pattern == "water" => Some("inland"),
            "" => None,
            kind => Some(kind),
        }
    }
}

/// A piece of a prop's stand-in model, until it has a real one: a simple shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Part {
    /// "box", "cylinder", "cone", "sphere", "rock", "wedge" (a box whose top slopes from its
    /// full height along its back, -z, down to nothing along its front, +z)
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
    /// "tree", "flora", "grass" (grown through tall ground too: `Growth`), "rock",
    /// "decoration", "fence", "furniture", "stairs", "ramp" (what
    /// lies along ramps), and building pieces: "door", "window", "wall", "pillar", "floor",
    /// "roof", "wall_decoration"
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
    /// a sound it plays where it stands: see `Sound`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<Sound>,
    /// the asset browser's folders it's in ("Nature", "Buildings/Inside"...); empty: the
    /// ones its category gives it (see `Prop::folders`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// drawn with the toon look (the world settings' `toon`): light in steps, ink lines
    #[serde(skip_serializing_if = "is_default")]
    pub toon: bool,
    /// a piece of railing (category "railing"): its style, and where it goes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub railing: Option<RailingPiece>,
    /// a tile of a set of floor tiles (category "floor"): its set, and what its edges are
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<FloorTile>,
    /// how its stand-in moves in the wind (a model moves by its own wind paint instead)
    #[serde(skip_serializing_if = "is_default")]
    pub wind: Breeze,
    /// it lies along ramps (stairs, a mud slide; category "ramp"): see `RampFit`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ramp: Option<crate::ramp::RampFit>,
}

/// How a prop's stand-in (its parts, which have no paint) moves in the wind: `sway`, bending
/// with it, more the higher up (0: not at all, to 1), and `flutter`, a leaves' quick shiver
/// (0 to 1). A real model is painted for the wind instead (a "wind" colour layer: see the
/// Blender guide).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Breeze {
    pub sway: f64,
    pub flutter: f64,
}

impl Breeze {
    pub fn still(&self) -> bool {
        self.sway <= 0.0 && self.flutter <= 0.0
    }
}

/// A floor tile's place in a set of them ("floor" props laid together by wave function
/// collapse: `world::floors`). A set's tiles are each a cell's flooring, modelled a cell wide
/// as any flooring is; laid side by side, two tiles fit when the edge between them is the same
/// on both (the same word), so a set can carry a pattern across its cells: a big flagstone
/// over two, a border round a medallion, planks running on. `edges` go round from the north
/// (-z), then east (+x), south (+z) and west, as it's modelled (unturned); a tile that `turns`
/// may be laid a quarter, a half or three quarters turned clockwise (seen from above), its
/// edges going round with it. The likelier by `weight` among those that fit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FloorTile {
    /// the set's name ("flagstones"): one floor is laid with one set's tiles
    pub set: String,
    /// what its north, east, south and west edges are ("joint", "slab"...)
    pub edges: [String; 4],
    pub weight: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub turns: bool,
}

impl Default for FloorTile {
    fn default() -> Self {
        FloorTile { set: String::new(), edges: ["joint".into(), "joint".into(), "joint".into(), "joint".into()], weight: 1.0, turns: false }
    }
}

impl FloorTile {
    /// Its edge on a side (0 north, 1 east, 2 south, 3 west) laid `turn` quarter turns
    /// clockwise: what was on the side `turn` steps back round.
    pub fn edge(&self, turn: u8, side: usize) -> &str {
        self.edge_as(turn, false, side)
    }

    /// Its edge on a side laid `turn` quarter turns clockwise, mirrored first when `flip` (its
    /// east and west swapped, then turned): a tile put by hand as a symmetric design's other half.
    pub fn edge_as(&self, turn: u8, flip: bool, side: usize) -> &str {
        let s = (side + 4 - turn as usize % 4) % 4;
        &self.edges[if flip { (4 - s) % 4 } else { s }]
    }

    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if self.set.trim().is_empty() {
            out.push("floor: a tile of a set says which (\"set\")".to_string());
        }
        if self.edges.iter().any(|e| e.trim().is_empty()) {
            out.push("floor: each of its four edges says what it is (north, east, south, west)".to_string());
        }
        if self.weight <= 0.0 {
            out.push(format!("floor: weight {}: more than 0", self.weight));
        }
        out
    }
}

/// The points railing pieces stand on: where a span ends ("end"), ends against a wall
/// ("wall"), two spans in a line ("line") or at a right angle ("corner"), three ("tee"),
/// four ("cross").
pub const JUNCTIONS: [&str; 6] = ["end", "wall", "line", "corner", "tee", "cross"];

/// What a railing's piece is (category "railing"). A railing is put together from the
/// pieces of one style: a span along each cell edge (modelled from x -0.5 to 0.5 along the
/// edge, z across it, standing on y 0) and a post on each point where spans meet or end (at
/// the origin, its +x along a span going away from the point). A span's `ends` say what each
/// of its ends needs of the point there, a post `takes` some of those words and stands `at`
/// some kinds of points: the pieces are picked so they all agree (wave function collapse:
/// `world::railings`), the likelier by `weight`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RailingPiece {
    /// its style ("wood", "iron"...): one railing is all of one style
    pub style: String,
    /// "span" or "post"
    pub part: String,
    /// a span: its -x end's word, then its +x end's ("rail": the rail goes on through the
    /// point; "post": it needs a post there, say)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ends: Vec<String>,
    /// a post: the span ends it stands between
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<String>,
    /// a post: the points it stands on (`JUNCTIONS`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<String>,
    /// how likely it's picked among the pieces that fit (more: more often)
    pub weight: f64,
}

impl Default for RailingPiece {
    fn default() -> Self {
        RailingPiece { style: String::new(), part: "span".into(), ends: vec!["rail".into(), "rail".into()], takes: vec![], at: vec![], weight: 1.0 }
    }
}

impl RailingPiece {
    pub fn is_span(&self) -> bool {
        self.part == "span"
    }

    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if self.style.is_empty() {
            out.push("a railing piece with no style".to_string());
        }
        match self.part.as_str() {
            "span" if self.ends.len() != 2 => out.push(format!("a span has two ends' words, not {}", self.ends.len())),
            "span" => {}
            "post" => {
                if self.takes.is_empty() {
                    out.push("a post that takes no span's end".into());
                }
                if self.at.is_empty() {
                    out.push(format!("a post that stands nowhere (at: {})", JUNCTIONS.join(", ")));
                }
                for a in self.at.iter().filter(|a| !JUNCTIONS.contains(&a.as_str())) {
                    out.push(format!("a post at \"{a}\": {}", JUNCTIONS.join(", ")));
                }
            }
            other => out.push(format!("a railing piece is a span or a post, not \"{other}\"")),
        }
        if self.weight.is_nan() || self.weight <= 0.0 {
            out.push("a railing piece's weight is above 0".into());
        }
        out
    }
}

/// A light a prop gives off: a spot (a cone of light) or a point (light all around). `at`
/// is where it sits, from the prop's origin (its footprint's centre on the ground: x, y up,
/// z), before the prop is turned; `aim` is the way a spot points. It lights what's in its
/// cone within `range`, fading to nothing at the edge, and casts no shadow. In Blender it's
/// a spot or point light object on the prop: 1000 W is an intensity of 1, the spot size is
/// the cone, a custom distance is the range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    /// "spot" or "point"
    pub kind: String,
    /// "#rrggbb"
    pub color: String,
    pub intensity: f64,
    /// how far it reaches, in cells
    pub range: f64,
    /// a spot's cone, its full angle in degrees (the light fades over its outer third)
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

impl Light {
    /// What's wrong with it (empty when nothing).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if !matches!(self.kind.as_str(), "spot" | "point") {
            out.push(format!("a light of kind \"{}\": \"spot\" or \"point\"", self.kind));
        }
        if !(self.range > 0.0 && self.intensity >= 0.0 && (1.0..=180.0).contains(&self.cone)) {
            out.push("a light needs a range over 0, an intensity of 0 or more and a cone of 1 to 180 degrees".into());
        }
        if self.color.len() != 7 || u32::from_str_radix(self.color.trim_start_matches('#'), 16).is_err() {
            out.push(format!("a light's colour \"{}\" isn't #rrggbb", self.color));
        }
        out
    }
}

/// A sound a prop plays where it stands (a speaker object on it in Blender): a file under
/// its space's `assets/` (`sounds/<name>.ogg`), looping, at `volume` (1 is as recorded) up
/// close and fading to nothing `range` cells from `at` (from the prop's origin, like a
/// light's). Not played by the engine yet: carried with the prop, for when it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sound {
    pub file: String,
    pub volume: f64,
    /// how far it's heard, in cells
    pub range: f64,
    pub at: [f64; 3],
}

impl Default for Sound {
    fn default() -> Self {
        Sound { file: String::new(), volume: 1.0, range: 8.0, at: [0.0, 1.0, 0.0] }
    }
}

impl Sound {
    /// What's wrong with it (empty when nothing).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if self.file.is_empty() || self.file.contains("..") || self.file.starts_with('/') {
            out.push(format!("a sound's file \"{}\": a file under assets/, like sounds/hum.ogg", self.file));
        }
        if !(self.range > 0.0 && self.volume >= 0.0) {
            out.push("a sound needs a range over 0 and a volume of 0 or more".into());
        }
        out
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
    /// Trees and flora keep their size when placed, and what lies along ramps fits the
    /// ramp; everything else can be scaled down.
    pub fn scalable(&self) -> bool {
        !matches!(self.category.as_str(), "tree" | "flora") && self.ramp.is_none()
    }

    /// The folders it's in: its own list, or the one its category gives.
    pub fn folders(&self) -> Vec<String> {
        if !self.folders.is_empty() {
            return self.folders.clone();
        }
        let f = match self.category.as_str() {
            "tree" | "flora" | "rock" | "grass" => "Nature",
            "furniture" | "stairs" => INSIDE,
            "wall_decoration" => OUTSIDE,
            "door" | "window" | "wall" | "pillar" | "floor" | "roof" => "Buildings/Structure",
            "railing" => "Buildings/Railings",
            "ramp" => "Ramps",
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

/// A prop placed on a map: on whole cells, which don't change once it's put down (its
/// footprint's centre, `at`); it can stand anywhere within them (`offset`, from their
/// centre) as far as its model leaves room.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Placed {
    pub prop: String,
    /// the centre of its footprint's cells, in cells from the map's north-west corner
    pub at: [f64; 2],
    pub level: u8,
    /// degrees around the vertical axis
    #[serde(skip_serializing_if = "is_default")]
    pub turn: f64,
    #[serde(skip_serializing_if = "is_one")]
    pub scale: f64,
    /// where it stands within its cells, from their centre (x, z)
    #[serde(skip_serializing_if = "is_default")]
    pub offset: [f64; 2],
}

impl Default for Placed {
    fn default() -> Placed {
        Placed { prop: String::new(), at: [0.0, 0.0], level: 0, turn: 0.0, scale: 1.0, offset: [0.0, 0.0] }
    }
}

impl Placed {
    /// Where it stands: its cells' centre, moved by its offset within them.
    pub fn pos(&self) -> [f64; 2] {
        [self.at[0] + self.offset[0], self.at[1] + self.offset[1]]
    }

    /// Its footprint along x and z, turned by quarter turns.
    pub fn turned(&self, footprint: [u8; 2]) -> (u8, u8) {
        if ((self.turn / 90.0).round() as i64).rem_euclid(2) == 1 { (footprint[1].max(1), footprint[0].max(1)) } else { (footprint[0].max(1), footprint[1].max(1)) }
    }
}

/// Things that stand on a map, kept to be put down again anywhere as they are: props,
/// buildings, actors and events, placed from the template's north-west corner (cells) with
/// their levels from its lowest ground. The map editor captures one from an area and puts
/// copies of it down where it's dropped (ids made unique, the scripts' cells moved along).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Template {
    pub id: String,
    pub name: String,
    /// the cells it covers: [along x, along z]
    pub size: [u16; 2],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<Placed>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<crate::building::Building>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actors: Vec<Actor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
}

impl Template {
    /// What's in it, in a few words ("3 props, 1 building").
    pub fn summary(&self) -> String {
        let count = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
        let mut parts = vec![];
        for (n, one, many) in [(self.props.len(), "prop", "props"), (self.buildings.len(), "building", "buildings"), (self.actors.len(), "actor", "actors"), (self.events.len(), "event", "events")] {
            if n > 0 {
                parts.push(count(n, one, many));
            }
        }
        if parts.is_empty() { "nothing".into() } else { parts.join(", ") }
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
    /// rows of cells along the top of a step whose ground goes down a level (see `Ramp`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ramps: Vec<crate::ramp::Ramp>,
    /// cells laid with flooring out of doors (a pavement, a plaza): each one's tile and turn,
    /// a set's laid to fit (`world::floors`), on the cell's ground
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub floors: Vec<crate::building::CellTile>,
    /// jumps between two cells with one between them, on one level (see `Jump`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub jumps: Vec<Jump>,
    /// the zone each cell belongs to (its `mark`), '.' for none
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zone_cells: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    /// creatures that come out of a treeline and go roam a zone
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SpawnLink>,
    /// the people, creatures and things that move and talk: see `Actor`
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actors: Vec<Actor>,
    /// what happens on the map's cells: see `Event`
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// the sounds of the place: see `Ambience`
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ambience: Vec<Ambience>,
}

/// Someone or something on a map with a life of its own: a creature, a picture or a prop
/// that stands, wanders or walks a path, and answers when spoken to (its `events`). It
/// starts at `at`, facing `facing` (degrees clockwise from south, the camera's side).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Actor {
    /// unique on the map (scripts name it)
    pub id: String,
    /// shown when it speaks
    pub name: String,
    /// what it looks like: "creature:<id>", "sprite:<picture under assets/>" or
    /// "prop:<id>"
    pub look: String,
    pub at: [i32; 2],
    pub level: u8,
    pub facing: f64,
    /// "still", "wander" (within `area`, idling between strolls) or "path" (`path`'s cells
    /// one after the other, round and round)
    pub moves: String,
    /// x, z, width, height, in cells
    pub area: [i32; 4],
    pub path: Vec<[i32; 2]>,
    /// cells a second
    pub speed: f64,
    /// what it does when spoken to or approached: its own events (their `area` is itself)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
}

impl Default for Actor {
    fn default() -> Self {
        Actor { id: String::new(), name: String::new(), look: String::new(), at: [0, 0], level: 0, facing: 0.0, moves: "still".into(), area: [0, 0, 0, 0], path: vec![], speed: 2.0, events: vec![] }
    }
}

/// Something that happens on a map: when the player speaks to what's there (`interact`:
/// facing it and pressing A), steps onto it (`enter`) or when the map starts (`start`), its
/// `script` runs (see `script`): dialog, choices, people moving about, the camera, sounds,
/// flags, a battle, a warp. `area` is the cells it covers (x, z, width, height); an actor's
/// events cover the actor. `once` runs it a single time; `if` needs a flag set and `unless`
/// needs it unset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Event {
    pub id: String,
    /// "interact", "enter" or "start"
    pub trigger: String,
    #[serde(skip_serializing_if = "is_default")]
    pub area: [i32; 4],
    #[serde(skip_serializing_if = "is_default")]
    pub once: bool,
    #[serde(rename = "if", skip_serializing_if = "String::is_empty")]
    pub when: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub unless: String,
    /// one action a line
    pub script: Vec<String>,
}

impl Default for Event {
    fn default() -> Self {
        Event { id: String::new(), trigger: "interact".into(), area: [0, 0, 0, 0], once: false, when: String::new(), unless: String::new(), script: vec![] }
    }
}

/// A sound of the place, looping: everywhere on the map, or within `area` (x, z, width,
/// height) and fading out over `fade` cells past its edge; `when` is "always", "day",
/// "night" or "morning".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ambience {
    /// under the space's `assets/`
    pub file: String,
    pub volume: f64,
    pub when: String,
    #[serde(skip_serializing_if = "is_default")]
    pub area: [i32; 4],
    #[serde(skip_serializing_if = "is_default")]
    pub fade: f64,
}

impl Default for Ambience {
    fn default() -> Self {
        Ambience { file: String::new(), volume: 0.5, when: "always".into(), area: [0, 0, 0, 0], fade: 0.0 }
    }
}

/// Chunk files: how many cells a side.
pub const CHUNK: u16 = 64;

/// How many cells a side a map can be at most (a few kilometres: a cell is about a metre).
pub const MAP_MAX: u16 = 2048;
/// Past this many cells a side, a map keeps its cells in chunk files (rows of characters
/// that long are no use to read or to merge).
pub const ROWS_MAX: u16 = 256;

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
