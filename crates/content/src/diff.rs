//! What a pull request changes, as the editor reviews it: its files by kind (a content record,
//! an asset, the game's or the editor's code) and, for a record, which of its fields changed
//! and how, so the editor can mark every place a change shows (a dot on a window's button, a
//! badge on an asset, a mark by a field) and a reviewer can read the whole of it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a file or a field changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Added,
    #[default]
    Modified,
    Removed,
    Renamed,
}

impl Kind {
    /// Its word in the editor.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Added => "added",
            Kind::Modified => "modified",
            Kind::Removed => "removed",
            Kind::Renamed => "renamed",
        }
    }

    /// From GitHub's word for a file's status.
    pub fn of_status(status: &str) -> Kind {
        match status {
            "added" | "copied" => Kind::Added,
            "removed" => Kind::Removed,
            "renamed" => Kind::Renamed,
            _ => Kind::Modified,
        }
    }
}

/// A file the pull request changes: its path in the repository, how, its lines added and
/// taken out, and for a text file the unified diff GitHub gives (`patch`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileChange {
    pub path: String,
    pub kind: Kind,
    /// renamed: where it was
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

/// Where a changed file belongs: a content record (its collection and id: a map's chunk files
/// count as the map's), an asset (models, sounds, fx...), code (the engine's crates, the tools,
/// the site's own functions, the workflows), or the rest (docs, configuration).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    Record { collection: String, id: String },
    Asset { kind: String, name: String },
    Code { area: String },
    Other,
}

impl FileChange {
    pub fn place(&self) -> Place {
        place_of(&self.path)
    }

    /// A line for a list: its path, how, and its lines.
    pub fn line(&self) -> String {
        let counts = match self.kind {
            Kind::Added => format!("+{}", self.additions),
            Kind::Removed => format!("-{}", self.deletions),
            _ => format!("+{} -{}", self.additions, self.deletions),
        };
        match &self.from {
            Some(from) => format!("{} ({} from {from}, {counts})", self.path, self.kind.word()),
            None => format!("{} ({}, {counts})", self.path, self.kind.word()),
        }
    }
}

/// Where a path belongs (`FileChange::place`).
pub fn place_of(path: &str) -> Place {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [space, "data", collection, rest @ ..] if matches!(*space, "content" | "temporary") && !rest.is_empty() => {
            // a record's file, or a chunked map's chunk file (data/maps/<id>/<x>_<y>.json)
            let id = rest[0].trim_end_matches(".json").to_string();
            Place::Record { collection: collection.to_string(), id }
        }
        [space, "assets", kind, rest @ ..] if matches!(*space, "content" | "temporary") && !rest.is_empty() => Place::Asset { kind: kind.to_string(), name: rest.join("/") },
        ["crates", crate_name, ..] => Place::Code { area: format!("crates/{crate_name}") },
        ["tools", ..] => Place::Code { area: "tools".into() },
        ["functions", ..] => Place::Code { area: "site".into() },
        [".github", ..] => Place::Code { area: "workflows".into() },
        [file] if file.ends_with(".rs") || file.ends_with(".toml") || file.ends_with(".html") || file.ends_with(".js") => Place::Code { area: "site".into() },
        _ => Place::Other,
    }
}

/// A field of a record that changed: its path in the record (`growth.meadow.plants[Daisy].sway`:
/// lists by their items' `id` or `name`, else by place), how, and what to ("0.8 -> 0.45"; a
/// list's items added or taken out say how many).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub path: String,
    pub kind: Kind,
    pub detail: String,
}

/// How a record's fields changed from `before` to `after` (both the record as JSON; a record
/// added or removed whole has one change at its root). Objects field by field; lists of
/// records keyed by their items' `id` or `name` (the rest by place); a map's `terrain` and
/// `levels` rows as cells painted.
pub fn fields(before: &Value, after: &Value) -> Vec<FieldChange> {
    let mut out = vec![];
    walk("", before, after, &mut out);
    out
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<FieldChange>) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                let p = join(path, k);
                match y.get(k) {
                    Some(vb) => walk(&p, va, vb, out),
                    None => out.push(FieldChange { path: p, kind: Kind::Removed, detail: short(va) }),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push(FieldChange { path: join(path, k), kind: Kind::Added, detail: short(vb) });
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            let name = path.rsplit('.').next().unwrap_or(path);
            if (name == "terrain" || name == "levels") && x.iter().chain(y.iter()).all(Value::is_string) {
                out.push(cells_painted(path, x, y));
            } else if let (Some(ka), Some(kb)) = (keys_of(x), keys_of(y)) {
                // records in a list, each by its key
                for (k, va) in &ka {
                    let p = format!("{path}[{k}]");
                    match kb.iter().find(|(kk, _)| kk == k) {
                        Some((_, vb)) => walk(&p, va, vb, out),
                        None => out.push(FieldChange { path: p, kind: Kind::Removed, detail: short(va) }),
                    }
                }
                for (k, vb) in &kb {
                    if !ka.iter().any(|(kk, _)| kk == k) {
                        out.push(FieldChange { path: format!("{path}[{k}]"), kind: Kind::Added, detail: short(vb) });
                    }
                }
            } else if x.iter().chain(y.iter()).all(|v| !v.is_object() && !v.is_array()) {
                // plain values: the list as one
                out.push(FieldChange { path: path.to_string(), kind: Kind::Modified, detail: format!("{} -> {}", short(a), short(b)) });
            } else {
                // items by place: those both have compared, the rest added or taken out (as
                // many as that: placed props, zones' cells)
                let n = x.len().min(y.len());
                for (i, (va, vb)) in x.iter().zip(y.iter()).enumerate() {
                    walk(&format!("{path}[{i}]"), va, vb, out);
                }
                if y.len() > n {
                    out.push(FieldChange { path: format!("{path}[+]"), kind: Kind::Added, detail: format!("{} more", y.len() - n) });
                } else if x.len() > n {
                    out.push(FieldChange { path: format!("{path}[-]"), kind: Kind::Removed, detail: format!("{} fewer", x.len() - n) });
                }
            }
        }
        _ => out.push(FieldChange { path: path.to_string(), kind: Kind::Modified, detail: format!("{} -> {}", short(a), short(b)) }),
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_string() } else { format!("{path}.{key}") }
}

/// A list's items each by its `id`, else its `name`, when every item has one and they differ;
/// else None.
fn keys_of(items: &[Value]) -> Option<Vec<(String, &Value)>> {
    for key in ["id", "name"] {
        let keys: Option<Vec<(String, &Value)>> = items.iter().map(|v| v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| (s.to_string(), v))).collect();
        if let Some(keys) = keys {
            let mut seen: Vec<&str> = keys.iter().map(|(k, _)| k.as_str()).collect();
            seen.sort_unstable();
            seen.dedup();
            if seen.len() == keys.len() && !keys.is_empty() {
                return Some(keys);
            }
        }
    }
    None
}

/// A map's rows of cells before and after: how many cells differ (rows added or taken out
/// count their cells).
fn cells_painted(path: &str, x: &[Value], y: &[Value]) -> FieldChange {
    let row = |v: &Value| v.as_str().unwrap_or("").to_string();
    let mut n = 0usize;
    for (ra, rb) in x.iter().zip(y.iter()) {
        let (ra, rb) = (row(ra), row(rb));
        n += ra.chars().zip(rb.chars()).filter(|(a, b)| a != b).count() + ra.chars().count().abs_diff(rb.chars().count());
    }
    for r in x.iter().skip(y.len()).chain(y.iter().skip(x.len())) {
        n += row(r).chars().count();
    }
    FieldChange { path: path.to_string(), kind: Kind::Modified, detail: format!("{n} cell{} painted", if n == 1 { "" } else { "s" }) }
}

/// A value in a few characters: a scalar as is, a list or an object by its size.
fn short(v: &Value) -> String {
    match v {
        Value::Null => "none".into(),
        Value::String(s) => {
            let s: String = s.chars().take(40).collect();
            if s.chars().count() >= 40 { format!("\"{s}…\"") } else { format!("\"{s}\"") }
        }
        Value::Array(a) => format!("[{} item{}]", a.len(), if a.len() == 1 { "" } else { "s" }),
        Value::Object(o) => format!("{{{} field{}}}", o.len(), if o.len() == 1 { "" } else { "s" }),
        other => other.to_string(),
    }
}

/// How many of a kind of thing changed each way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
}

impl Counts {
    pub fn count(&mut self, kind: Kind) {
        match kind {
            Kind::Added => self.added += 1,
            Kind::Removed => self.removed += 1,
            _ => self.modified += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.added + self.modified + self.removed
    }

    pub fn add(&mut self, other: Counts) {
        self.added += other.added;
        self.modified += other.modified;
        self.removed += other.removed;
    }

    /// "2 added, 1 modified" (nothing for 0 of a kind; "nothing" when none).
    pub fn text(&self) -> String {
        let parts: Vec<String> = [(self.added, "added"), (self.modified, "modified"), (self.removed, "removed")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
        if parts.is_empty() { "nothing".into() } else { parts.join(", ") }
    }
}

/// A pull request under review: what it is, its files, and each changed record's fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Review {
    pub number: u64,
    pub title: String,
    pub author: String,
    /// its branch's commit, and the commit it's against
    pub head: String,
    pub base: String,
    pub url: String,
    pub files: Vec<FileChange>,
    /// by "collection/id": the record's fields that changed
    pub fields: BTreeMap<String, Vec<FieldChange>>,
    /// by "collection/id": the record's name (a removed one's too, from before)
    pub names: BTreeMap<String, String>,
}

impl Review {
    fn key(collection: &str, id: &str) -> String {
        format!("{collection}/{id}")
    }

    /// The file change of a record, if the pull request touches it (a chunked map's by any of
    /// its chunk files).
    pub fn record(&self, collection: &str, id: &str) -> Option<&FileChange> {
        self.files.iter().find(|f| matches!(f.place(), Place::Record { collection: c, id: i } if c == collection && i == id))
    }

    /// How a record changed, whole: added, removed, else modified when any field did.
    pub fn record_kind(&self, collection: &str, id: &str) -> Option<Kind> {
        let f = self.record(collection, id)?;
        Some(match f.kind {
            Kind::Added | Kind::Removed => f.kind,
            _ => Kind::Modified,
        })
    }

    /// A record's changed fields, those under `prefix` (all with ""): what to mark by a section
    /// or a field.
    pub fn changed(&self, collection: &str, id: &str, prefix: &str) -> Vec<&FieldChange> {
        self.fields
            .get(&Self::key(collection, id))
            .map(|v| v.iter().filter(|c| prefix.is_empty() || c.path == prefix || c.path.starts_with(&format!("{prefix}.")) || c.path.starts_with(&format!("{prefix}["))).collect())
            .unwrap_or_default()
    }

    /// Whether a record changed at all under a prefix (a whole record added or removed counts
    /// everywhere in it).
    pub fn touches(&self, collection: &str, id: &str, prefix: &str) -> bool {
        matches!(self.record_kind(collection, id), Some(Kind::Added | Kind::Removed)) || !self.changed(collection, id, prefix).is_empty()
    }

    /// The records changed, by collection: how many each way.
    pub fn collections(&self) -> BTreeMap<String, Counts> {
        let mut out: BTreeMap<String, Counts> = BTreeMap::new();
        let mut seen: Vec<String> = vec![];
        for f in &self.files {
            if let Place::Record { collection, id } = f.place() {
                let key = Self::key(&collection, &id);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                out.entry(collection).or_default().count(f.kind);
            }
        }
        out
    }

    /// The records a collection changes, each with how.
    /// A record's name as the pull request has it (or had it), else its id.
    pub fn name_of(&self, collection: &str, id: &str) -> String {
        self.names.get(&Self::key(collection, id)).cloned().unwrap_or_else(|| id.to_string())
    }

    pub fn records_of(&self, collection: &str) -> Vec<(String, Kind)> {
        let mut out: Vec<(String, Kind)> = vec![];
        for f in &self.files {
            if let Place::Record { collection: c, id } = f.place() {
                if c == collection && !out.iter().any(|(i, _)| *i == id) {
                    out.push((id, if matches!(f.kind, Kind::Added | Kind::Removed) { f.kind } else { Kind::Modified }));
                }
            }
        }
        out
    }

    /// The assets changed (models, sounds, fx...), each by kind and name.
    pub fn assets(&self) -> Vec<(String, String, Kind)> {
        self.files.iter().filter_map(|f| match f.place() {
            Place::Asset { kind, name } => Some((kind, name, f.kind)),
            _ => None,
        }).collect()
    }

    /// The code changed (the engine's crates, the tools, the site, the workflows), by area.
    pub fn code(&self) -> BTreeMap<String, Vec<&FileChange>> {
        let mut out: BTreeMap<String, Vec<&FileChange>> = BTreeMap::new();
        for f in &self.files {
            if let Place::Code { area } = f.place() {
                out.entry(area).or_default().push(f);
            }
        }
        out
    }

    /// The rest: docs, configuration.
    pub fn other(&self) -> Vec<&FileChange> {
        self.files.iter().filter(|f| f.place() == Place::Other).collect()
    }

    /// Everything, in words: the records by collection with their fields, the assets, the code
    /// by area with its lines; what the window lists and what Claude is asked to review.
    pub fn summary(&self) -> String {
        let mut out = vec![format!("Pull request #{} by {}: {} ({} files)", self.number, self.author, self.title, self.files.len())];
        for (collection, counts) in self.collections() {
            out.push(format!("\n{collection}: {}", counts.text()));
            for (id, kind) in self.records_of(&collection) {
                out.push(format!("  {id}: {}", kind.word()));
                for c in self.changed(&collection, &id, "") {
                    out.push(format!("    {} {}: {}", c.kind.word(), c.path, c.detail));
                }
            }
        }
        let assets = self.assets();
        if !assets.is_empty() {
            out.push("\nassets:".into());
            for (kind, name, k) in assets {
                out.push(format!("  {kind}/{name}: {}", k.word()));
            }
        }
        for (area, files) in self.code() {
            out.push(format!("\ncode, {area}:"));
            for f in files {
                out.push(format!("  {}", f.line()));
            }
        }
        let other = self.other();
        if !other.is_empty() {
            out.push("\nother:".into());
            for f in other {
                out.push(format!("  {}", f.line()));
            }
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_records_fields_are_told_apart_lists_by_their_names_and_cells_counted() {
        let before = json!({"id": "lawn", "color": "#4a8a3a", "priority": 1, "growth": {"meadow": {"plants": [{"name": "Daisy", "sway": 0.8}, {"name": "Grass", "sway": 1.0}], "density": 48}}, "look": {"layers": [{"shape": "dots", "glow": 1.0}]}});
        let after = json!({"id": "lawn", "color": "#4a8a3a", "priority": 2, "growth": {"meadow": {"plants": [{"name": "Daisy", "sway": 0.45}, {"name": "Poppy", "sway": 0.45}], "density": 48, "toon": true}}, "look": {"layers": [{"shape": "dots", "glow": 2.0}, {"shape": "cracks"}]}});
        let changes = fields(&before, &after);
        let said: Vec<String> = changes.iter().map(|c| format!("{} {} {}", c.kind.word(), c.path, c.detail)).collect();
        assert_eq!(
            said,
            [
                "modified priority 1 -> 2",
                "modified growth.meadow.plants[Daisy].sway 0.8 -> 0.45",
                "removed growth.meadow.plants[Grass] {2 fields}",
                "added growth.meadow.plants[Poppy] {2 fields}",
                "added growth.meadow.toon true",
                "modified look.layers[0].glow 1.0 -> 2.0",
                "added look.layers[+] 1 more",
            ]
        );
        // a map: its cells painted, its props by place
        let before = json!({"terrain": ["gggg", "gggg"], "levels": ["0000", "0000"], "props": [{"prop": "well", "at": [1, 1]}]});
        let after = json!({"terrain": ["ggdd", "gggg", "dddd"], "levels": ["0000", "0010"], "props": [{"prop": "well", "at": [2, 1]}, {"prop": "lamp", "at": [0, 0]}]});
        let said: Vec<String> = fields(&before, &after).iter().map(|c| format!("{} {} {}", c.kind.word(), c.path, c.detail)).collect();
        assert_eq!(said, ["modified terrain 6 cells painted", "modified levels 1 cell painted", "modified props[0].at [2 items] -> [2 items]", "added props[+] 1 more"]);
        assert!(fields(&before, &before).is_empty());
    }

    #[test]
    fn files_know_where_they_belong_and_a_review_counts_and_marks() {
        assert_eq!(place_of("content/data/terrains/lava.json"), Place::Record { collection: "terrains".into(), id: "lava".into() });
        assert_eq!(place_of("content/data/maps/east_field/1_0.json"), Place::Record { collection: "maps".into(), id: "east_field".into() });
        assert_eq!(place_of("temporary/assets/models/kildin.glb"), Place::Asset { kind: "models".into(), name: "kildin.glb".into() });
        assert_eq!(place_of("crates/game/src/mapedit.rs"), Place::Code { area: "crates/game".into() });
        assert_eq!(place_of("tools/bladecheck.mjs"), Place::Code { area: "tools".into() });
        assert_eq!(place_of(".github/workflows/checks.yml"), Place::Code { area: "workflows".into() });
        assert_eq!(place_of("README.md"), Place::Other);
        let file = |path: &str, status: &str, a: u32, d: u32| FileChange { path: path.into(), kind: Kind::of_status(status), additions: a, deletions: d, ..Default::default() };
        let mut r = Review {
            number: 7,
            title: "Lava".into(),
            author: "someone".into(),
            files: vec![
                file("content/data/terrains/lava.json", "added", 20, 0),
                file("content/data/terrains/grass.json", "modified", 2, 1),
                file("content/data/maps/east_field/0_0.json", "modified", 5, 5),
                file("content/data/maps/east_field/1_0.json", "modified", 1, 1),
                file("content/data/props/old_post.json", "removed", 0, 9),
                file("content/assets/models/lamp.glb", "modified", 0, 0),
                file("crates/game/src/mapedit/terrains.rs", "modified", 40, 3),
                file("README.md", "modified", 3, 0),
            ],
            ..Default::default()
        };
        r.fields.insert("terrains/grass".into(), vec![FieldChange { path: "growth.meadow.density".into(), kind: Kind::Modified, detail: "48 -> 56".into() }]);
        assert_eq!(r.collections().get("terrains").copied(), Some(Counts { added: 1, modified: 1, removed: 0 }));
        assert_eq!(r.collections().get("maps").copied(), Some(Counts { added: 0, modified: 1, removed: 0 }), "a chunked map once");
        assert_eq!(r.collections().get("props").map(|c| c.text()), Some("1 removed".to_string()));
        assert_eq!(r.record_kind("terrains", "lava"), Some(Kind::Added));
        assert_eq!(r.record_kind("terrains", "grass"), Some(Kind::Modified));
        assert_eq!(r.record_kind("terrains", "sand"), None);
        assert!(r.touches("terrains", "grass", "growth") && r.touches("terrains", "grass", "growth.meadow.density") && !r.touches("terrains", "grass", "look"));
        assert!(r.touches("terrains", "lava", "look"), "added whole: everywhere");
        assert_eq!(r.changed("terrains", "grass", "growth.meadow").len(), 1);
        assert_eq!(r.assets(), [("models".to_string(), "lamp.glb".to_string(), Kind::Modified)]);
        assert_eq!(r.code().keys().cloned().collect::<Vec<_>>(), ["crates/game"]);
        assert_eq!(r.other().len(), 1);
        let text = r.summary();
        assert!(text.contains("terrains: 1 added, 1 modified") && text.contains("    modified growth.meadow.density: 48 -> 56") && text.contains("code, crates/game:") && text.contains("README.md (modified, +3 -0)"), "{text}");
        assert_eq!(FileChange { from: Some("a.rs".into()), ..file("b.rs", "renamed", 1, 1) }.line(), "b.rs (renamed from a.rs, +1 -1)");
    }
}
