//! The data the battle rules run on, built from the game's content (crates/content):
//! types and their chart, creatures, moves, abilities, with numeric indexes for speed.
//! Mechanics are looked up by `effect` keys and type roles, never by names.

use std::collections::HashMap;

use content::Content;

pub use content::Space;

/// A type index that matches nothing (a role the content has no type for).
pub const NO_TYPE: u8 = u8::MAX;

/// The types some mechanics single out (weather boosts water and fire, ground creatures
/// shrug off electric paralysis...), found by id in the content. A role the content doesn't have is
/// NO_TYPE and the mechanic never applies.
#[derive(Clone, Copy, Debug)]
pub struct TypeRoles {
    pub normal: u8,
    pub fire: u8,
    pub water: u8,
    pub grass: u8,
    pub electric: u8,
    pub ice: u8,
    pub ground: u8,
    pub bug: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Physical,
    Special,
    Status,
}

#[derive(Clone, Debug)]
pub struct MoveData {
    /// index in the dex (content order)
    pub id: u16,
    /// the content id
    pub key: String,
    pub name: String,
    /// the engine's mechanic, uppercase (e.g. PARALYZE_HIT)
    pub effect: String,
    pub power: u16,
    pub ty: u8,
    pub accuracy: u16,
    pub pp: u8,
    pub chance: u16,
    /// uppercase (e.g. SELECTED, BOTH)
    pub target: String,
    pub priority: i8,
    pub contact: bool,
    pub category: Category,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct Species {
    /// index in the dex (content order)
    pub id: u16,
    pub key: String,
    pub name: String,
    pub number: u16,
    pub types: Vec<u8>,
    /// hp, atk, def, spe, spa, spd
    pub base: [u16; 6],
    /// ability ids
    pub abilities: Vec<String>,
    pub catch_rate: u16,
    pub elevation: u16,
    pub learnset: Vec<(u8, u16)>,
    pub front_box: [i32; 4],
    pub back_box: [i32; 4],
    /// a light it carries
    pub light: Option<content::Light>,
    /// a sound it makes as it goes about, and its cry (a file under its space's assets)
    pub sound: Option<content::Sound>,
    pub cry: Option<String>,
    /// drawn with the toon look
    pub toon: bool,
    /// asset paths from the project root (space folder included)
    pub front: Option<String>,
    pub back: Option<String>,
    pub icon: Option<String>,
    /// a placeholder from the temporary space
    pub temporary: bool,
}

#[derive(Clone, Debug)]
pub struct AbilityData {
    pub name: String,
    pub effect: String,
}

pub struct Dex {
    /// display names, by type index
    pub types: Vec<String>,
    pub type_keys: Vec<String>,
    pub type_colors: Vec<u32>,
    pub type_short: Vec<String>,
    pub chart: Vec<Vec<f32>>,
    pub ty: TypeRoles,
    immune: Vec<Vec<String>>,
    moves: Vec<MoveData>,
    move_by_key: HashMap<String, u16>,
    pub species: Vec<Species>,
    species_by_key: HashMap<String, usize>,
    abilities: HashMap<String, AbilityData>,
    profiles: Vec<Option<crate::moves::Profile>>,
}

fn hex(color: &str) -> u32 {
    u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x888888)
}

impl Dex {
    pub fn from_content(c: &Content) -> Result<Dex, String> {
        let type_keys: Vec<String> = c.types.iter().map(|t| t.id.clone()).collect();
        if type_keys.is_empty() {
            return Err("no types in the content".into());
        }
        let type_index: HashMap<&str, u8> = type_keys.iter().enumerate().map(|(i, k)| (k.as_str(), i as u8)).collect();
        let ty_of = |k: &str| type_index.get(k).copied().ok_or_else(|| format!("no type \"{k}\""));
        let mut chart = vec![vec![1.0f32; type_keys.len()]; type_keys.len()];
        for (a, t) in c.types.iter().enumerate() {
            for (d, m) in &t.against {
                chart[a][ty_of(d)? as usize] = *m as f32;
            }
        }
        let role = |k: &str| type_index.get(k).copied().unwrap_or(NO_TYPE);
        let ty = TypeRoles {
            normal: role("normal"), fire: role("fire"), water: role("water"), grass: role("grass"),
            electric: role("electric"), ice: role("ice"), ground: role("ground"), bug: role("bug"),
        };
        let mut moves = vec![];
        for (i, m) in c.moves.iter().enumerate() {
            moves.push(MoveData {
                id: i as u16,
                key: m.id.clone(),
                name: m.name.clone(),
                effect: m.effect.to_uppercase(),
                power: m.power,
                ty: ty_of(&m.ty)?,
                accuracy: m.accuracy,
                pp: m.pp,
                chance: m.effect_chance,
                target: m.target.to_uppercase(),
                priority: m.priority,
                contact: m.contact,
                category: match m.category.as_str() {
                    "physical" => Category::Physical,
                    "special" => Category::Special,
                    _ => Category::Status,
                },
                description: m.description.clone(),
            });
        }
        let move_by_key: HashMap<String, u16> = moves.iter().map(|m| (m.key.clone(), m.id)).collect();
        let mut species = vec![];
        for (i, e) in c.creatures.entries().iter().enumerate() {
            let cr = &e.value;
            let s = &cr.base_stats;
            let learnset = cr
                .learnset
                .level
                .iter()
                .map(|(lv, m)| move_by_key.get(m).map(|&id| (*lv, id)).ok_or_else(|| format!("creatures/{}: no move \"{m}\"", cr.id)))
                .collect::<Result<Vec<_>, _>>()?;
            let path = |p: &Option<String>| p.as_ref().map(|p| e.space.asset(p));
            species.push(Species {
                id: i as u16,
                key: cr.id.clone(),
                name: cr.name.clone(),
                number: cr.number,
                types: cr.types.iter().map(|t| ty_of(t)).collect::<Result<_, _>>()?,
                base: [s.hp, s.attack, s.defense, s.speed, s.sp_attack, s.sp_defense],
                abilities: cr.abilities.clone(),
                catch_rate: cr.catch_rate,
                elevation: cr.sprites.elevation,
                learnset,
                front_box: cr.sprites.front_box,
                light: cr.light.clone(),
                sound: cr.sound.clone().map(|mut s| {
                    s.file = e.space.asset(&s.file);
                    s
                }),
                cry: path(&cr.cry),
                toon: cr.toon,
                back_box: cr.sprites.back_box,
                front: path(&cr.sprites.front),
                back: path(&cr.sprites.back),
                icon: path(&cr.sprites.icon),
                temporary: e.space == Space::Temporary,
            });
        }
        let profiles = moves.iter().map(crate::moves::build_profile).collect();
        Ok(Dex {
            types: c.types.iter().map(|t| t.name.clone()).collect(),
            type_colors: c.types.iter().map(|t| hex(&t.color)).collect(),
            type_short: c.types.iter().map(|t| t.short.clone()).collect(),
            immune: c.types.iter().map(|t| t.immune_to.clone()).collect(),
            type_keys,
            chart,
            ty,
            species_by_key: species.iter().enumerate().map(|(i, s)| (s.key.clone(), i)).collect(),
            species,
            move_by_key,
            moves,
            abilities: c.abilities.iter().map(|a| (a.id.clone(), AbilityData { name: a.name.clone(), effect: a.effect.clone() })).collect(),
            profiles,
        })
    }

    pub fn mv(&self, id: u16) -> &MoveData {
        &self.moves[id as usize]
    }

    pub fn move_id(&self, key: &str) -> u16 {
        *self.move_by_key.get(key).unwrap_or_else(|| panic!("unknown move {key}"))
    }

    pub fn try_move_id(&self, key: &str) -> Option<u16> {
        self.move_by_key.get(key).copied()
    }

    pub fn moves(&self) -> impl Iterator<Item = &MoveData> {
        self.moves.iter()
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    /// The creature's index in `species`.
    pub fn sp(&self, key: &str) -> usize {
        *self.species_by_key.get(key).unwrap_or_else(|| panic!("unknown creature {key}"))
    }

    pub fn try_sp(&self, key: &str) -> Option<usize> {
        self.species_by_key.get(key).copied()
    }

    pub fn profile(&self, id: u16) -> Option<&crate::moves::Profile> {
        self.profiles.get(id as usize).and_then(|p| p.as_ref())
    }

    pub fn supported(&self, id: u16) -> bool {
        self.profile(id).is_some()
    }

    pub fn effectiveness(&self, atk: u8, def: &[u8]) -> f32 {
        def.iter().map(|&d| self.chart[atk as usize][d as usize]).product()
    }

    /// Can creatures of this type get this status? ("burn", "poison", "freeze", ...)
    pub fn type_immune(&self, ty: u8, status: &str) -> bool {
        self.immune.get(ty as usize).map(|v| v.iter().any(|s| s == status)).unwrap_or(false)
    }

    /// The engine mechanic of an ability ("" when it has none).
    pub fn ability_effect(&self, id: &str) -> &str {
        self.abilities.get(id).map(|a| a.effect.as_str()).unwrap_or("")
    }

    pub fn ability_name(&self, id: &str) -> String {
        self.abilities.get(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string())
    }

    pub fn type_color(&self, ty: u8) -> u32 {
        self.type_colors.get(ty as usize).copied().unwrap_or(0x888888)
    }

    pub fn type_short(&self, ty: u8) -> &str {
        self.type_short.get(ty as usize).map(String::as_str).unwrap_or("???")
    }
}

/// mulberry32, the same generator as the JavaScript prototype.
#[derive(Clone, Debug)]
pub struct Rng {
    s: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng { s: seed }
    }

    #[allow(clippy::should_implement_trait)] // mulberry32 as in the prototype, not an iterator
    pub fn next(&mut self) -> f64 {
        self.s = self.s.wrapping_add(0x6d2b79f5);
        let mut t = self.s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }

    /// lo..=hi
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() * (hi - lo + 1) as f64).floor() as i32
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }

    pub fn pick<'a, T>(&mut self, list: &'a [T]) -> &'a T {
        let i = (self.next() * list.len() as f64).floor() as usize;
        &list[i.min(list.len() - 1)]
    }
}
