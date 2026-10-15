//! The game's content as data: creatures, moves, types, abilities, items, natures, growth
//! rates, egg groups, regions, encounters, parties, and the world (settings, terrains,
//! props, maps). One JSON file per record, in two spaces with the same layout:
//!
//! - `content/`: the game's own content. Committed; what the public game ships.
//! - `temporary/`: placeholders imported locally by a private tool (not in the repository), to
//!   build and test with until original content replaces them. Never committed.
//!
//! A space is `data/<collection>/<id>.json` + `assets/` (pictures and sounds its records
//! name). When both spaces have a record with the same id, the game's own wins. For the
//! browser, a space's files are packed into one `bundle.json` (`content bundle <space>`).

pub mod building;
pub mod model;

use std::collections::{BTreeMap, HashMap};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use building::*;
pub use model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Space {
    /// the game's own content (committed)
    Content,
    /// imported placeholders (never committed)
    Temporary,
}

impl Space {
    pub const ALL: [Space; 2] = [Space::Content, Space::Temporary];

    /// The space's folder at the root of the project (and of the site).
    pub fn dir(self) -> &'static str {
        match self {
            Space::Content => "content",
            Space::Temporary => "temporary",
        }
    }

    /// Where a record's asset path points, from the root.
    pub fn asset(self, path: &str) -> String {
        format!("{}/assets/{path}", self.dir())
    }
}

/// A kind of record, stored under `data/<COLLECTION>/`.
pub trait Record: Serialize + DeserializeOwned + Clone {
    const COLLECTION: &'static str;
    fn id(&self) -> &str;
}

macro_rules! record {
    ($t:ty, $c:literal) => {
        impl Record for $t {
            const COLLECTION: &'static str = $c;
            fn id(&self) -> &str {
                &self.id
            }
        }
    };
}
record!(TypeDef, "types");
record!(Creature, "creatures");
record!(Move, "moves");
record!(Ability, "abilities");
record!(Item, "items");
record!(Nature, "natures");
record!(GrowthRate, "growth_rates");
record!(EggGroup, "egg_groups");
record!(Region, "regions");
record!(Encounter, "encounters");
record!(Party, "parties");
record!(Settings, "settings");
record!(Terrain, "terrains");
record!(Prop, "props");
record!(Map, "maps");
record!(Spawns, "spawns");
record!(World, "worlds");

pub const COLLECTIONS: [&str; 17] = [
    "types", "creatures", "moves", "abilities", "items", "natures", "growth_rates", "egg_groups", "regions", "encounters", "parties",
    "settings", "terrains", "props", "maps", "spawns", "worlds",
];

#[derive(Clone, Debug)]
pub struct Entry<T> {
    pub value: T,
    pub space: Space,
}

/// The records of one kind, the game's own over temporary ones.
#[derive(Clone, Debug)]
pub struct Collection<T> {
    entries: Vec<Entry<T>>,
    index: HashMap<String, usize>,
}

impl<T> Default for Collection<T> {
    fn default() -> Self {
        Collection { entries: vec![], index: HashMap::new() }
    }
}

impl<T: Record> Collection<T> {
    pub fn get(&self, id: &str) -> Option<&T> {
        self.index.get(id).map(|&i| &self.entries[i].value)
    }

    pub fn entry(&self, id: &str) -> Option<&Entry<T>> {
        self.index.get(id).map(|&i| &self.entries[i])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|e| &e.value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a record; one with the same id is replaced by the game's own, or by a later
    /// one from the same space.
    fn insert(&mut self, value: T, space: Space) {
        self.put_in(value, space, false)
    }

    /// Adds or replaces a record whatever the spaces (the editor, saving one).
    pub fn put(&mut self, value: T, space: Space) {
        self.put_in(value, space, true)
    }

    fn put_in(&mut self, value: T, space: Space, always: bool) {
        match self.index.get(value.id()) {
            Some(&i) => {
                if always || !(self.entries[i].space == Space::Content && space == Space::Temporary) {
                    self.entries[i] = Entry { value, space };
                }
            }
            None => {
                self.index.insert(value.id().to_string(), self.entries.len());
                self.entries.push(Entry { value, space });
            }
        }
    }

    /// Orders the records (by number, then id, for most collections).
    pub fn sort_by_key<K: Ord>(&mut self, key: impl Fn(&T) -> K) {
        self.entries.sort_by_key(|e| key(&e.value));
        self.index = self.entries.iter().enumerate().map(|(i, e)| (e.value.id().to_string(), i)).collect();
    }
}

/// A record as the text of its file: 2-space indents, short lists and small objects on one
/// line, lists of those one item per line, so records read well and diff line by line (the
/// same layout as the placeholder import writes).
pub fn to_text<T: Serialize>(record: &T) -> String {
    let value = serde_json::to_value(record).unwrap_or_default();
    let mut out = String::new();
    pretty(&value, 0, &mut out);
    out.push('\n');
    out
}

fn is_scalar(v: &serde_json::Value) -> bool {
    !v.is_array() && !v.is_object()
}

/// A list of scalars, or of lists of scalars, or an object of those: on one line.
fn inline(v: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    let flat = |x: &Value| is_scalar(x) || x.as_array().map(|a| a.iter().all(is_scalar)).unwrap_or(false);
    match v {
        Value::Array(a) if a.iter().all(flat) => {
            let items: Vec<String> = a.iter().map(|x| inline(x).unwrap_or_default()).collect();
            Some(format!("[{}]", items.join(", ")))
        }
        Value::Object(o) if o.values().all(flat) => {
            let items: Vec<String> = o.iter().map(|(k, x)| format!("{}: {}", serde_json::to_string(k).unwrap_or_default(), inline(x).unwrap_or_default())).collect();
            Some(if items.is_empty() { "{}".into() } else { format!("{{ {} }}", items.join(", ")) })
        }
        Value::Array(_) | Value::Object(_) => None,
        _ => Some(serde_json::to_string(v).unwrap_or_default()),
    }
}

fn pretty(v: &serde_json::Value, indent: usize, out: &mut String) {
    use serde_json::Value;
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    match v {
        Value::Object(o) if !o.is_empty() => {
            out.push_str("{\n");
            for (i, (k, x)) in o.iter().enumerate() {
                out.push_str(&inner);
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push_str(": ");
                pretty(x, indent + 1, out);
                out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad);
            out.push('}');
        }
        Value::Array(a) if !a.is_empty() => {
            if a.iter().all(is_scalar) {
                if let Some(line) = inline(v).filter(|l| l.len() <= 100) {
                    out.push_str(&line);
                    return;
                }
            }
            // one item per line: on one line if it fits, else laid out
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&inner);
                match inline(x).filter(|l| l.len() <= 120) {
                    Some(line) => out.push_str(&line),
                    None => pretty(x, indent + 1, out),
                }
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad);
            out.push(']');
        }
        _ => out.push_str(&inline(v).unwrap_or_default()),
    }
}

/// A space's files in one file, for the browser.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    /// "game-content/1"
    pub format: String,
    pub space: Space,
    /// "creatures/voltkit.json" -> the record
    pub files: BTreeMap<String, serde_json::Value>,
}

pub const FORMAT: &str = "game-content/1";

#[derive(Clone, Debug, Default)]
pub struct Content {
    pub types: Collection<TypeDef>,
    pub creatures: Collection<Creature>,
    pub moves: Collection<Move>,
    pub abilities: Collection<Ability>,
    pub items: Collection<Item>,
    pub natures: Collection<Nature>,
    pub growth_rates: Collection<GrowthRate>,
    pub egg_groups: Collection<EggGroup>,
    pub regions: Collection<Region>,
    pub encounters: Collection<Encounter>,
    pub parties: Collection<Party>,
    pub settings: Collection<Settings>,
    pub terrains: Collection<Terrain>,
    pub props: Collection<Prop>,
    pub maps: Collection<Map>,
    pub spawns: Collection<Spawns>,
    pub worlds: Collection<World>,
    /// what couldn't be read
    pub problems: Vec<String>,
}

fn parse<T: Record>(path: &str, value: serde_json::Value) -> Result<T, String> {
    let record: T = serde_json::from_value(value).map_err(|e| format!("{path}: {e}"))?;
    let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".json");
    if record.id() != stem {
        return Err(format!("{path}: its id is \"{}\", the file says \"{stem}\"", record.id()));
    }
    Ok(record)
}

impl Content {
    /// Adds one record file, `path` = "<collection>/<id>.json".
    pub fn add(&mut self, space: Space, path: &str, value: serde_json::Value) -> Result<(), String> {
        let collection = path.split('/').next().unwrap_or("");
        macro_rules! go {
            ($field:ident) => {
                self.$field.insert(parse(path, value)?, space)
            };
        }
        match collection {
            "types" => go!(types),
            "creatures" => go!(creatures),
            "moves" => go!(moves),
            "abilities" => go!(abilities),
            "items" => go!(items),
            "natures" => go!(natures),
            "growth_rates" => go!(growth_rates),
            "egg_groups" => go!(egg_groups),
            "regions" => go!(regions),
            "encounters" => go!(encounters),
            "parties" => go!(parties),
            "settings" => go!(settings),
            "terrains" => go!(terrains),
            "props" => go!(props),
            "maps" => go!(maps),
            "spawns" => go!(spawns),
            "worlds" => go!(worlds),
            other => return Err(format!("{path}: no collection named \"{other}\"")),
        }
        Ok(())
    }

    /// Adds a bundle's records; returns how many. Unreadable records go to `problems`.
    pub fn add_bundle(&mut self, text: &str) -> Result<usize, String> {
        let bundle: Bundle = serde_json::from_str(text).map_err(|e| format!("bundle: {e}"))?;
        if bundle.format != FORMAT {
            return Err(format!("bundle: format \"{}\", expected \"{FORMAT}\"", bundle.format));
        }
        let n = bundle.files.len();
        for (path, value) in bundle.files {
            if let Err(e) = self.add(bundle.space, &path, value) {
                self.problems.push(e);
            }
        }
        Ok(n)
    }

    /// Puts every collection in its order: by order or number, then id.
    pub fn sort(&mut self) {
        self.types.sort_by_key(|t| (t.order, t.id.clone()));
        self.creatures.sort_by_key(|c| (c.number, c.id.clone()));
        self.moves.sort_by_key(|m| (m.number, m.id.clone()));
        self.abilities.sort_by_key(|a| (a.number, a.id.clone()));
        self.items.sort_by_key(|i| (i.number, i.id.clone()));
        self.natures.sort_by_key(|n| (n.number, n.id.clone()));
        self.growth_rates.sort_by_key(|g| g.id.clone());
        self.egg_groups.sort_by_key(|g| g.id.clone());
        self.regions.sort_by_key(|r| (r.order, r.id.clone()));
        self.encounters.sort_by_key(|e| (e.order, e.id.clone()));
        self.parties.sort_by_key(|p| p.id.clone());
        self.settings.sort_by_key(|s| s.id.clone());
        self.terrains.sort_by_key(|t| (t.order, t.id.clone()));
        self.props.sort_by_key(|p| (p.category.clone(), p.id.clone()));
        self.maps.sort_by_key(|m| m.id.clone());
        self.spawns.sort_by_key(|s| s.id.clone());
        self.worlds.sort_by_key(|w| w.id.clone());
    }

    /// The world's measures (the "world" settings record, or the defaults).
    pub fn world(&self) -> Settings {
        self.settings.get("world").cloned().unwrap_or_default()
    }


    /// Records per space and collection.
    pub fn counts(&self) -> BTreeMap<(&'static str, Space), usize> {
        let mut out = BTreeMap::new();
        macro_rules! count {
            ($field:ident, $name:literal) => {
                for e in self.$field.entries() {
                    *out.entry(($name, e.space)).or_insert(0) += 1;
                }
            };
        }
        count!(types, "types");
        count!(creatures, "creatures");
        count!(moves, "moves");
        count!(abilities, "abilities");
        count!(items, "items");
        count!(natures, "natures");
        count!(growth_rates, "growth_rates");
        count!(egg_groups, "egg_groups");
        count!(regions, "regions");
        count!(encounters, "encounters");
        count!(parties, "parties");
        count!(settings, "settings");
        count!(terrains, "terrains");
        count!(props, "props");
        count!(maps, "maps");
        count!(spawns, "spawns");
        count!(worlds, "worlds");
        out
    }

    /// Every reference that points nowhere, and other mistakes a review should catch.
    pub fn validate(&self) -> Vec<String> {
        let mut out = self.problems.clone();
        let mut need = |ok: bool, what: String| {
            if !ok {
                out.push(what);
            }
        };
        for t in self.types.iter() {
            for d in t.against.keys() {
                need(self.types.contains(d), format!("types/{}: against \"{d}\", no such type", t.id));
            }
        }
        for c in self.creatures.iter() {
            let at = format!("creatures/{}", c.id);
            for e in c.light.iter().flat_map(|l| l.problems()) {
                need(false, format!("{at}: {e}"));
            }
            need(!c.types.is_empty() && c.types.len() <= 2, format!("{at}: needs 1 or 2 types"));
            for t in &c.types {
                need(self.types.contains(t), format!("{at}: type \"{t}\", no such type"));
            }
            for a in &c.abilities {
                need(self.abilities.contains(a), format!("{at}: ability \"{a}\", no such ability"));
            }
            for g in &c.egg_groups {
                need(self.egg_groups.contains(g), format!("{at}: egg group \"{g}\", no such group"));
            }
            need(self.growth_rates.contains(&c.growth_rate), format!("{at}: growth rate \"{}\", no such rate", c.growth_rate));
            for i in c.held_items.common.iter().chain(c.held_items.rare.iter()) {
                need(self.items.contains(i), format!("{at}: held item \"{i}\", no such item"));
            }
            let moves = c.learnset.level.iter().map(|(_, m)| m).chain(&c.learnset.machine).chain(&c.learnset.tutor).chain(&c.learnset.egg);
            for m in moves {
                need(self.moves.contains(m), format!("{at}: learns \"{m}\", no such move"));
            }
            for e in &c.evolutions {
                need(self.creatures.contains(&e.into), format!("{at}: evolves into \"{}\", no such creature", e.into));
                if let Some(i) = &e.item {
                    need(self.items.contains(i), format!("{at}: evolution item \"{i}\", no such item"));
                }
            }
        }
        for m in self.moves.iter() {
            need(self.types.contains(&m.ty), format!("moves/{}: type \"{}\", no such type", m.id, m.ty));
            need(matches!(m.category.as_str(), "physical" | "special" | "status"), format!("moves/{}: category \"{}\"", m.id, m.category));
        }
        let member = |m: &Member, at: &str, out: &mut Vec<String>| {
            if !self.creatures.contains(&m.creature) {
                out.push(format!("{at}: creature \"{}\", no such creature", m.creature));
            }
            for mv in &m.moves {
                if !self.moves.contains(mv) {
                    out.push(format!("{at}: move \"{mv}\", no such move"));
                }
            }
        };
        for e in self.encounters.iter() {
            let at = format!("encounters/{}", e.id);
            if !self.regions.contains(&e.region) {
                out.push(format!("{at}: region \"{}\", no such region", e.region));
            }
            for m in &e.party {
                member(m, &at, &mut out);
            }
            for (period, pool) in &e.pools {
                for (c, _) in pool {
                    if !self.creatures.contains(c) {
                        out.push(format!("{at}: {period} pool has \"{c}\", no such creature"));
                    }
                }
            }
        }
        for p in self.parties.iter() {
            for m in &p.members {
                member(m, &format!("parties/{}", p.id), &mut out);
            }
        }
        for t in self.terrains.iter() {
            if !matches!(t.walk.as_str(), "walk" | "water" | "block") {
                out.push(format!("terrains/{}: walk \"{}\" (walk, water or block)", t.id, t.walk));
            }
        }
        for p in self.props.iter() {
            if p.footprint[0] == 0 || p.footprint[1] == 0 {
                out.push(format!("props/{}: an empty footprint", p.id));
            }
            if let Some(l) = &p.light {
                out.extend(l.problems().into_iter().map(|e| format!("props/{}: {e}", p.id)));
            }
        }
        let world = self.world();
        for m in self.maps.iter() {
            out.extend(self.check_map(m, &world));
        }
        let size = |id: &str| self.maps.get(id).map(|m| (m.width, m.height));
        for w in self.worlds.iter() {
            out.extend(w.problems(&size).into_iter().map(|p| format!("worlds/{}: {p}", w.id)));
        }
        for e in self.spawns.entries() {
            let s = &e.value;
            let at = format!("spawns/{}", s.id);
            let Some(map) = self.maps.get(&s.id) else {
                out.push(format!("{at}: no map \"{}\"", s.id));
                continue;
            };
            for (mark, periods) in &s.zones {
                if !map.zones.iter().any(|z| &z.mark == mark) {
                    out.push(format!("{at}: zone \"{mark}\", the map has none"));
                }
                for (period, pool) in periods {
                    if !matches!(period.as_str(), "morning" | "day" | "night") {
                        out.push(format!("{at}: \"{period}\" isn't morning, day or night"));
                    }
                    for (c, _) in pool {
                        match self.creatures.entry(c) {
                            None => out.push(format!("{at}: creature \"{c}\", no such creature")),
                            // the game's own records only name the game's own creatures
                            Some(ce) if e.space == Space::Content && ce.space == Space::Temporary => {
                                out.push(format!("{at}: in content/ but names the placeholder \"{c}\": save it in temporary/"))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        out
    }

    fn check_map(&self, m: &Map, world: &Settings) -> Vec<String> {
        let at = format!("maps/{}", m.id);
        let mut out = vec![];
        if let Some(r) = &m.region {
            if !self.regions.contains(r) {
                out.push(format!("{at}: region \"{r}\", no such region"));
            }
        }
        for (c, t) in &m.legend {
            if c.chars().count() != 1 {
                out.push(format!("{at}: legend key \"{c}\" must be one character"));
            }
            if !self.terrains.contains(t) {
                out.push(format!("{at}: legend \"{c}\" is \"{t}\", no such terrain"));
            }
        }
        let rows = |name: &str, rows: &[String], out: &mut Vec<String>| {
            if rows.len() != m.height as usize {
                out.push(format!("{at}: {} {name} rows, the map is {} high", rows.len(), m.height));
            }
            for (i, r) in rows.iter().enumerate() {
                if r.chars().count() != m.width as usize {
                    out.push(format!("{at}: {name} row {i} has {} cells, the map is {} wide", r.chars().count(), m.width));
                }
            }
        };
        if m.chunked() {
            // the cells are in chunk files (checked from disk: `disk::check_chunks`)
            if !(8..=512).contains(&m.chunk) {
                out.push(format!("{at}: chunks of {} cells: 8 to 512", m.chunk));
            }
            if m.terrains.is_empty() {
                out.push(format!("{at}: chunked, but no terrains list"));
            }
            for t in &m.terrains {
                if !self.terrains.contains(t) {
                    out.push(format!("{at}: terrain \"{t}\", no such terrain"));
                }
            }
            if !m.terrain.is_empty() || !m.levels.is_empty() || !m.areas.is_empty() || !m.zone_cells.is_empty() {
                out.push(format!("{at}: chunked, so its rows are ignored: remove them"));
            }
        } else {
            rows("terrain", &m.terrain, &mut out);
            rows("levels", &m.levels, &mut out);
            if !m.areas.is_empty() {
                rows("areas", &m.areas, &mut out);
            }
        }
        let unknown: std::collections::BTreeSet<char> = m.terrain.iter().flat_map(|r| r.chars()).filter(|c| !m.legend.contains_key(&c.to_string())).collect();
        if !unknown.is_empty() {
            out.push(format!("{at}: terrain characters {unknown:?} aren't in the legend"));
        }
        if m.levels.iter().flat_map(|r| r.chars()).any(|c| char_level(c).map(|l| l > world.max_level).unwrap_or(true)) {
            out.push(format!("{at}: levels must be 0 to {}", world.max_level));
        }
        let marks: std::collections::BTreeSet<String> = m.tree_areas.iter().map(|a| a.mark.clone()).collect();
        if m.areas.iter().flat_map(|r| r.chars()).any(|c| c != '.' && !marks.contains(&c.to_string())) {
            out.push(format!("{at}: an area cell has a mark no tree area uses"));
        }
        for a in &m.tree_areas {
            for t in &a.trees {
                if !self.props.contains(t) {
                    out.push(format!("{at}: tree area \"{}\" uses \"{t}\", no such prop", a.mark));
                }
            }
        }
        for p in &m.props {
            if !self.props.contains(&p.prop) {
                out.push(format!("{at}: \"{}\" at {:?}, no such prop", p.prop, p.at));
            }
        }
        if !m.zone_cells.is_empty() {
            rows("zone_cells", &m.zone_cells, &mut out);
        }
        let zone_marks: std::collections::BTreeSet<String> = m.zones.iter().map(|z| z.mark.clone()).collect();
        if m.zone_cells.iter().flat_map(|r| r.chars()).any(|c| c != '.' && !zone_marks.contains(&c.to_string())) {
            out.push(format!("{at}: a zone cell has a mark no zone uses"));
        }
        for l in &m.links {
            if !marks.contains(&l.area) {
                out.push(format!("{at}: a link from tree area \"{}\", the map has none", l.area));
            }
            if !zone_marks.contains(&l.zone) {
                out.push(format!("{at}: a link to zone \"{}\", the map has none", l.zone));
            }
            if l.treeline.is_empty() {
                out.push(format!("{at}: a link to \"{}\" with no treeline", l.zone));
            }
        }
        let prop = |id: &str| self.props.get(id).map(|p| (p.category.clone(), p.footprint));
        let mut taken: BTreeMap<(i32, i32), String> = BTreeMap::new();
        for b in &m.buildings {
            out.extend(b.problems(&prop).into_iter().map(|p| format!("{at}: {p}")));
            if m.buildings.iter().filter(|o| o.id == b.id).count() > 1 {
                out.push(format!("{at}: two buildings with the id \"{}\"", b.id));
            }
            for (x, z) in b.map_cells() {
                if x < 0 || z < 0 || x >= m.width as i32 || z >= m.height as i32 {
                    out.push(format!("{at}: building \"{}\" goes off the map at {x}, {z}", b.id));
                    break;
                }
                if let Some(other) = taken.insert((x, z), b.id.clone()) {
                    out.push(format!("{at}: buildings \"{other}\" and \"{}\" both cover {x}, {z}", b.id));
                    break;
                }
            }
            if b.level > world.max_level {
                out.push(format!("{at}: building \"{}\" on level {}", b.id, b.level));
            }
        }
        out
    }
}

/// Reading a space from disk (the desktop, the tools).
#[cfg(not(target_arch = "wasm32"))]
pub mod disk {
    use super::*;
    use std::path::Path;

    /// Every record file of a space: ("<collection>/<id>.json", record).
    pub fn read_space(dir: &Path) -> Result<Vec<(String, serde_json::Value)>, String> {
        let data = dir.join("data");
        let mut out = vec![];
        for collection in COLLECTIONS {
            let Ok(list) = std::fs::read_dir(data.join(collection)) else { continue };
            let mut names: Vec<String> = list.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json")).collect();
            names.sort();
            for name in names {
                let path = data.join(collection).join(&name);
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                out.push((format!("{collection}/{name}"), value));
            }
        }
        Ok(out)
    }

    /// Both spaces under `root` (the project folder), the game's own over the temporary.
    pub fn load(root: &Path) -> Content {
        let mut content = Content::default();
        for space in [Space::Temporary, Space::Content] {
            match read_space(&root.join(space.dir())) {
                Ok(files) => {
                    for (path, value) in files {
                        if let Err(e) = content.add(space, &path, value) {
                            content.problems.push(e);
                        }
                    }
                }
                Err(e) => content.problems.push(e),
            }
        }
        content.sort();
        content
    }

    /// Files records name that aren't there (props' models), under `root`.
    pub fn missing_files(root: &Path, content: &Content) -> Vec<String> {
        let mut out = vec![];
        for e in content.props.entries() {
            if let Some(m) = &e.value.model {
                let rel = e.space.asset(m);
                if !root.join(&rel).is_file() {
                    out.push(format!("props/{}: its model {rel} isn't there", e.value.id));
                }
            }
        }
        out
    }

    fn chunk_dir(root: &Path, space: Space, id: &str) -> std::path::PathBuf {
        root.join(space.dir()).join("data").join("maps").join(id)
    }

    /// A chunked map's chunk files (those that are there), under a space.
    pub fn read_chunks(root: &Path, space: Space, map: &Map) -> Result<Vec<Chunk>, String> {
        let dir = chunk_dir(root, space, &map.id);
        let mut out = vec![];
        let Ok(list) = std::fs::read_dir(&dir) else { return Ok(out) };
        let mut names: Vec<String> = list.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json")).collect();
        names.sort();
        for name in names {
            let path = dir.join(&name);
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            out.push(serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?);
        }
        Ok(out)
    }

    /// Writes a chunked map's chunk files: those whose text changed; blank squares and
    /// squares not given lose their file. Returns (written, removed).
    pub fn save_chunks(root: &Path, space: Space, map: &Map, chunks: &[Chunk]) -> Result<(usize, usize), String> {
        let dir = chunk_dir(root, space, &map.id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut keep = std::collections::BTreeSet::new();
        let (mut written, mut removed) = (0, 0);
        for c in chunks.iter().filter(|c| !c.is_blank()) {
            let name = format!("{}_{}.json", c.chunk[0], c.chunk[1]);
            let path = dir.join(&name);
            keep.insert(name);
            let text = to_text(c);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
                written += 1;
            }
        }
        if let Ok(list) = std::fs::read_dir(&dir) {
            for e in list.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".json") && !keep.contains(&name) && std::fs::remove_file(e.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        Ok((written, removed))
    }

    /// Removes a map's chunk folder (a map stored as rows again, or gone); how many files.
    pub fn remove_chunks(root: &Path, space: Space, id: &str) -> usize {
        let dir = chunk_dir(root, space, id);
        let n = std::fs::read_dir(&dir).map(|l| l.count()).unwrap_or(0);
        std::fs::remove_dir_all(&dir).ok();
        n
    }

    /// What's wrong with the chunk files of the chunked maps.
    pub fn check_chunks(root: &Path, content: &Content) -> Vec<String> {
        let world = content.world();
        let mut out = vec![];
        for e in content.maps.entries() {
            let m = &e.value;
            if !m.chunked() {
                continue;
            }
            let at = format!("maps/{}", m.id);
            let dir = chunk_dir(root, e.space, &m.id);
            let Ok(list) = std::fs::read_dir(&dir) else { continue };
            let (gx, gz) = m.chunk_grid();
            for entry in list.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".json") {
                    continue;
                }
                let Some((cx, cz)) = Chunk::parse_name(&name) else {
                    out.push(format!("{at}: chunk file {name}: named <x>_<z>.json"));
                    continue;
                };
                if cx < 0 || cz < 0 || cx >= gx || cz >= gz {
                    out.push(format!("{at}: chunk {cx}, {cz} is outside the map ({gx} x {gz} chunks)"));
                }
                let c: Chunk = match std::fs::read_to_string(entry.path()).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
                    Ok(c) => c,
                    Err(err) => {
                        out.push(format!("{at}: chunk {name}: {err}"));
                        continue;
                    }
                };
                if c.map != m.id || c.chunk != [cx, cz] {
                    out.push(format!("{at}: chunk {name} says it's {} {:?}", c.map, c.chunk));
                }
                let n = m.chunk as usize;
                let shape = |rows: &[Vec<u8>]| rows.len() <= n && rows.iter().all(|r| r.len() <= n);
                if c.terrain.len() > n || c.terrain.iter().any(|r| r.len() > n) || !shape(&c.levels) || !shape(&c.areas) || !shape(&c.zones) {
                    out.push(format!("{at}: chunk {name} has rows past {n} cells"));
                }
                if c.terrain.iter().flatten().any(|&t| t as usize >= m.terrains.len()) {
                    out.push(format!("{at}: chunk {name} uses a terrain past the map's list of {}", m.terrains.len()));
                }
                if c.levels.iter().flatten().any(|&l| l > world.max_level) {
                    out.push(format!("{at}: chunk {name}: levels must be 0 to {}", world.max_level));
                }
                if c.areas.iter().flatten().any(|&a| a as usize > m.tree_areas.len()) {
                    out.push(format!("{at}: chunk {name} names a tree area the map doesn't have"));
                }
                if c.zones.iter().flatten().any(|&z| z as usize > m.zones.len()) {
                    out.push(format!("{at}: chunk {name} names a zone the map doesn't have"));
                }
            }
        }
        out
    }

    /// Where the worlds' regions touch and the levels differ on the two sides: notes, not
    /// problems (a cliff at a seam is allowed; walking can't cross there).
    pub fn seam_notes(root: &Path, content: &Content) -> Vec<String> {
        let levels = |id: &str| {
            let e = content.maps.entry(id)?;
            let chunks = if e.value.chunked() { read_chunks(root, e.space, &e.value).ok()? } else { vec![] };
            Some((e.value.width, e.value.height, e.value.cells(&chunks).level))
        };
        let mut out = vec![];
        for w in content.worlds.iter() {
            for s in w.seams(&levels) {
                if !s.mismatches.is_empty() {
                    out.push(format!("worlds/{}: {} and {} meet along {} cells; the levels differ on {} of them (no walking across there)", w.id, s.a, s.b, s.cells, s.mismatches.len()));
                }
            }
        }
        out
    }

    /// Writes one record to `<root>/<space>/data/<collection>/<id>.json`; returns the path.
    pub fn save<T: Record>(root: &Path, space: Space, record: &T) -> Result<std::path::PathBuf, String> {
        let dir = root.join(space.dir()).join("data").join(T::COLLECTION);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(format!("{}.json", record.id()));
        std::fs::write(&path, to_text(record)).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// Packs a space's files into its bundle.json (what the browser loads).
    pub fn write_bundle(dir: &Path, space: Space) -> Result<usize, String> {
        let files: BTreeMap<String, serde_json::Value> = read_space(dir)?.into_iter().collect();
        let n = files.len();
        let bundle = Bundle { format: FORMAT.into(), space, files };
        let text = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("bundle.json"), text).map_err(|e| e.to_string())?;
        Ok(n)
    }
}

/// Loads bundles (the browser): text of each space's bundle.json that exists.
pub fn from_bundles(bundles: &[&str]) -> Content {
    let mut content = Content::default();
    // temporary first, so the game's own records win
    let mut parsed: Vec<(Space, &str)> = bundles
        .iter()
        .filter_map(|t| serde_json::from_str::<serde_json::Value>(t).ok().and_then(|v| v.get("space").cloned()).and_then(|s| serde_json::from_value::<Space>(s).ok()).map(|s| (s, *t)))
        .collect();
    parsed.sort_by_key(|(s, _)| match s {
        Space::Temporary => 0,
        Space::Content => 1,
    });
    for (_, text) in parsed {
        if let Err(e) = content.add_bundle(text) {
            content.problems.push(e);
        }
    }
    content.sort();
    content
}
