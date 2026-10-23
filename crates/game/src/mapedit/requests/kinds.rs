//! The window's kinds (its tabs): each made as one of the kit's eight (`Kind`: how the kit makes
//! it and what it becomes in the game), with the settings its requests are asked (`Param`, from
//! the window's own pool), its instructions (`Feature`: a keyword and how a run handles it, each
//! a line of every run's prompt) and its varieties (a creature is a variety of character; the
//! environment's grass, flower, rock and tree are each made their own way: `Variety::made_as`),
//! with settings and instructions of their own added to the kind's, one with the same keyword
//! replacing the kind's. The catalogue's own instructions go with every request, whatever its
//! kind. The standard set is tools/blender/request_kinds.json; the desktop edits that file (the
//! one it read), a browser keeps its own copy (private/requests/kinds.json), which its runs are
//! given. The kit reads them as the window does (model_requests.py's `catalogue`,
//! `instructions`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};

use super::super::assets::slug;
use super::super::{hint, mark, row_icon, section, section_title, warn};
use super::desk::folder;
use super::disk;
use super::{Kind, ModelRequest};

/// The standard kinds, as the project has them.
pub(super) const STANDARD: &str = include_str!("../../../../../tools/blender/request_kinds.json");
/// The standard kinds' file in the project, which the desktop edits.
const PROJECT: &str = "tools/blender/request_kinds.json";
/// A browser's own copy, in the requests' folder (and its runs').
const OWN: &str = "kinds.json";
/// How long after the last change the kinds are written (seconds).
const KEEP_AFTER: f64 = 0.6;

/// A setting the window asks a request, from its own pool: which a kind's requests are asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in super::super) enum Param {
    Height,
    Cells,
    Style,
    Pose,
    Shapes,
    Bones,
    Variants,
    Many,
    CheckLines,
}

impl Param {
    pub const ALL: [Param; 9] = [Param::Height, Param::Cells, Param::Style, Param::Pose, Param::Shapes, Param::Bones, Param::Variants, Param::Many, Param::CheckLines];

    pub fn name(self) -> &'static str {
        match self {
            Param::Height => "Height",
            Param::Cells => "Cells (a footprint)",
            Param::Style => "Style (and model details)",
            Param::Pose => "Default pose",
            Param::Shapes => "Shapes",
            Param::Bones => "Draw the bones",
            Param::Variants => "Variants from the first run",
            Param::Many => "Expect large rendered count",
            Param::CheckLines => "Let me check the lines first",
        }
    }

    /// Its id, as the files have it ("check_lines").
    pub fn id(self) -> &'static str {
        match self {
            Param::Height => "height",
            Param::Cells => "cells",
            Param::Style => "style",
            Param::Pose => "pose",
            Param::Shapes => "shapes",
            Param::Bones => "bones",
            Param::Variants => "variants",
            Param::Many => "many",
            Param::CheckLines => "check_lines",
        }
    }

    /// Whether a request made as `k` can ask it.
    pub fn fits(self, k: Kind) -> bool {
        match self {
            Param::Height => !matches!(k, Kind::Floor | Kind::Rock),
            Param::Cells | Param::Variants | Param::CheckLines => k != Kind::Floor,
            Param::Style | Param::Shapes => true,
            Param::Pose | Param::Bones => k == Kind::Character,
            Param::Many => k.strewn(),
        }
    }

    /// Whether a request made as `k` always asks it: the kit makes it by it (grass's cells, a
    /// rock's footprint, any height but a rock's and a floor's).
    pub fn needed(self, k: Kind) -> bool {
        match self {
            Param::Height => self.fits(k),
            Param::Cells => matches!(k, Kind::Grass | Kind::Rock),
            _ => false,
        }
    }
}

/// What a kind asks of every run about one feature: its keyword ("fur", "colour") and how a
/// run handles it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct Feature {
    pub keyword: String,
    pub text: String,
}

/// A variety of a kind (a creature of the characters, a pine of the trees): its own settings
/// and instructions; made as its kind, unless it says how (the environment's grass, flower,
/// rock and tree, each made its own way by the kit).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct Variety {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub made_as: Option<Kind>,
    pub params: Vec<Param>,
    pub features: Vec<Feature>,
}

impl Variety {
    /// How the kit makes it: its own way, else its kind's (`kind`).
    pub fn making(&self, kind: Kind) -> Kind {
        self.made_as.unwrap_or(kind)
    }
}

/// One of the window's kinds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct KindDef {
    pub id: String,
    pub name: String,
    pub made_as: Kind,
    pub params: Vec<Param>,
    pub features: Vec<Feature>,
    pub varieties: Vec<Variety>,
}

impl Default for KindDef {
    fn default() -> Self {
        KindDef { id: String::new(), name: String::new(), made_as: Kind::Prop, params: vec![], features: vec![], varieties: vec![] }
    }
}

impl KindDef {
    /// One of the kit's own (made as itself): never removed, its making never changed.
    pub fn built_in(&self) -> bool {
        self.id == self.made_as.id()
    }

    pub fn variety(&self, id: &str) -> Option<&Variety> {
        self.varieties.iter().find(|v| v.id == id)
    }

    /// A variety added, made as the kind (None) or its own way: its settings the kind's (what
    /// the window always asked, when it's made otherwise), no instructions of its own yet; its id.
    pub fn add_variety(&mut self, name: &str, made_as: Option<Kind>) -> String {
        let taken: Vec<String> = self.varieties.iter().map(|v| v.id.clone()).collect();
        let id = Catalogue::free(name, &taken);
        let params = match made_as {
            Some(m) if m != self.made_as => default_params(m),
            _ => self.params.clone(),
        };
        self.varieties.push(Variety { id: id.clone(), name: name.trim().to_string(), made_as: made_as.filter(|m| *m != self.made_as), params, features: vec![] });
        id
    }
}

/// The settings a request made as `k` asks when the files say none (as the window always asked).
pub(super) fn default_params(k: Kind) -> Vec<Param> {
    use Param::*;
    match k {
        Kind::Character => vec![Height, Style, Pose, Shapes, Bones, CheckLines],
        Kind::Prop => vec![Height, Style, Shapes, Many, CheckLines],
        Kind::Grass => vec![Height, Cells, Style, Shapes, Variants, Many, CheckLines],
        Kind::Rock => vec![Cells, Style, Shapes, Variants, Many, CheckLines],
        Kind::Flower | Kind::Tree | Kind::Environment => vec![Height, Style, Shapes, Variants, Many, CheckLines],
        Kind::Floor => vec![Style, Shapes],
    }
}

/// How the kit makes a request made as `k`, whatever its instructions say (its own rules: what
/// the window shows, unchanged).
pub(in super::super) fn making(k: Kind) -> &'static str {
    match k {
        Kind::Character => "Rigged on a spine, a head, limbs and a tail (on your bones when they're drawn), with an idle animation, in the pose asked: a character of the story, or a creature (its variety) of the fields, with a creature record of its own.",
        Kind::Prop => "A prop as the pictures show it, fitted to its height.",
        Kind::Grass => "Fitted to its cells exactly (kit.fit_cells), so copies side by side join; a grass prop that never blocks the way, in light pieces within the engine's vertex budget (kit.finish refuses more).",
        Kind::Flower => "Made part by part, a flower prop that never blocks the way: a meadow can grow it among its grass.",
        Kind::Rock => "On about its footprint (kit.fit_footprint), its height worked out from the pictures' proportions; a rock prop that blocks the way.",
        Kind::Tree => "Its leaves as geometry (the engine draws no cut-out textures and no camera-facing billboards for a model's parts); a tree prop that blocks the way on its trunk's cells; tree areas grow it.",
        Kind::Environment => "The prop category that fits it (tree, flora, rock, grass, decoration), blocking the way when it's big enough to walk round. Its grass, flower, rock and tree varieties are each made their own way.",
        Kind::Floor => "A set of floor tiles, each a cell, laid to fit by wave function collapse: no lines drawn, each tile light.",
    }
}

/// The first letter made a capital ("grass": "Grass").
fn capital(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The window's kinds, the instructions for every kind, and where they were read from (the
/// desktop writes them back there).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct Catalogue {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub about: String,
    /// what every request's run is asked, whatever its kind (a kind's with the same keyword
    /// replaces one)
    pub features: Vec<Feature>,
    pub kinds: Vec<KindDef>,
    #[serde(skip)]
    pub from: Option<PathBuf>,
}

impl Catalogue {
    /// The standard kinds (the project's file as built in).
    pub fn standard() -> Catalogue {
        let mut c: Catalogue = serde_json::from_str(STANDARD).unwrap_or_default();
        c.complete();
        c
    }

    /// The kinds as the window has them: a browser's own copy (or the desktop's, if one was
    /// put there), else the project's file, else the standard; what was wrong with one that
    /// couldn't be read.
    pub fn load(root: &Path) -> (Catalogue, Option<String>) {
        let mut trouble = None;
        for path in [folder(root).join(OWN), root.join(PROJECT)] {
            if !disk::is_file(&path) {
                continue;
            }
            match disk::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Catalogue>(&t).map_err(|e| e.to_string())) {
                Ok(mut c) => {
                    c.complete();
                    c.from = Some(path);
                    return (c, trouble);
                }
                Err(e) => trouble = Some(format!("{}: {e} (the standard kinds instead)", path.display())),
            }
        }
        (Catalogue::standard(), trouble)
    }

    /// Written where it was read on the desktop (else the project's file); in a browser, its own
    /// copy. Where it went.
    pub fn save(&mut self, root: &Path) -> Result<PathBuf, String> {
        let path = if cfg!(target_arch = "wasm32") { folder(root).join(OWN) } else { self.from.clone().unwrap_or_else(|| root.join(PROJECT)) };
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n";
        if let Some(dir) = path.parent() {
            let _ = disk::create_dir_all(dir);
        }
        disk::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.from = Some(path.clone());
        Ok(path)
    }

    /// Every one of the kit's eight made somewhere: a kind of it, or a variety made as it, added
    /// as the standard has it when a file leaves one out (an older one: its creature kind is
    /// the character kind now, with a creature variety; its grass, flower, rock and tree kinds
    /// stay its own).
    fn complete(&mut self) {
        let standard: Catalogue = serde_json::from_str(STANDARD).unwrap_or_default();
        if let Some(k) = self.kinds.iter_mut().find(|k| k.id == "creature" && k.made_as == Kind::Character) {
            k.id = "character".into();
            if k.name.trim().eq_ignore_ascii_case("creature") {
                k.name = "Character".into();
            }
            if k.variety("creature").is_none() {
                k.varieties.insert(0, Variety { id: "creature".into(), name: "Creature".into(), made_as: None, params: k.params.clone(), features: vec![] });
            }
        }
        // the standard's kinds first (the environment before its varieties), then what it hosts
        // as a variety (else a kind of its own, as the window always asked)
        for pass in 0..2 {
            for k in Kind::ALL {
                if self.kinds.iter().any(|d| d.id == k.id() || d.varieties.iter().any(|v| v.made_as == Some(k))) {
                    continue;
                }
                match (pass, standard.kinds.iter().find(|d| d.id == k.id())) {
                    (0, Some(def)) => {
                        // (a variety made as a kind the file has of its own: that kind's)
                        let mut def = def.clone();
                        def.varieties.retain(|v| v.made_as.is_none_or(|m| !self.kinds.iter().any(|d| d.id == m.id())));
                        self.kinds.push(def);
                    }
                    (1, None) => {
                        let hosted = standard.kinds.iter().find_map(|d| d.varieties.iter().find(|v| v.made_as == Some(k)).map(|v| (d.id.clone(), v.clone())));
                        match hosted.and_then(|(host, v)| self.kind_mut(&host).map(|h| (h, v))) {
                            Some((h, v)) => h.varieties.push(v),
                            None => self.kinds.push(KindDef { id: k.id().into(), name: capital(k.name()), made_as: k, params: default_params(k), ..Default::default() }),
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn kind(&self, id: &str) -> Option<&KindDef> {
        self.kinds.iter().find(|k| k.id == id)
    }

    pub fn kind_mut(&mut self, id: &str) -> Option<&mut KindDef> {
        self.kinds.iter_mut().find(|k| k.id == id)
    }

    /// A request's kind (its window kind's, else the kit's own it's made as) and its variety.
    pub fn of(&self, r: &ModelRequest) -> (Option<&KindDef>, Option<&Variety>) {
        let k = self.kind(&r.kind_key()).or_else(|| self.kind(r.kind.id()));
        let v = k.and_then(|k| k.variety(&r.variety)).filter(|_| !r.variety.is_empty());
        (k, v)
    }

    /// The settings a request is asked: its variety's, else its kind's (else as the window
    /// always asked), those its making can ask, with what the kit needs.
    pub fn params_of(&self, r: &ModelRequest) -> Vec<Param> {
        let (k, v) = self.of(r);
        let chosen = v.map(|v| v.params.clone()).or_else(|| k.map(|k| k.params.clone())).unwrap_or_else(|| default_params(r.kind));
        Param::ALL.into_iter().filter(|p| p.fits(r.kind) && (p.needed(r.kind) || chosen.contains(p))).collect()
    }

    /// What a request's run is asked: the instructions for every kind, then its kind's, then
    /// its variety's, one with the same keyword replacing the earlier where it was (as
    /// model_requests.py's `instructions`).
    pub fn features_of(&self, r: &ModelRequest) -> Vec<Feature> {
        let (k, v) = self.of(r);
        let mut out: Vec<Feature> = vec![];
        let all = self.features.iter().chain(k.map(|k| k.features.iter()).into_iter().flatten()).chain(v.map(|v| v.features.iter()).into_iter().flatten());
        for f in all.cloned() {
            let word = f.keyword.trim().to_lowercase();
            match out.iter().position(|o| !word.is_empty() && o.keyword.trim().to_lowercase() == word) {
                Some(i) => out[i] = f,
                None => out.push(f),
            }
        }
        out.retain(|f| !f.keyword.trim().is_empty() || !f.text.trim().is_empty());
        out
    }

    /// A request's kind as the cards say it: its variety's name ("creature", "rock", "pine"),
    /// else its kind's ("environment", "bush").
    pub fn label(&self, r: &ModelRequest) -> String {
        let (k, v) = self.of(r);
        let kind = k.map(|k| k.name.trim().to_lowercase()).filter(|n| !n.is_empty()).unwrap_or_else(|| r.kind.name().to_string());
        match v {
            Some(v) if !v.name.trim().is_empty() => v.name.trim().to_lowercase(),
            _ => kind,
        }
    }

    /// A free id for `name` among `taken` (its slug, numbered when taken).
    fn free(name: &str, taken: &[String]) -> String {
        let base = slug(name);
        let base = if base.is_empty() { "kind".to_string() } else { base };
        let mut id = base.clone();
        let mut n = 2;
        while taken.contains(&id) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }

    /// A kind added, made as `made_as`: its settings and instructions that kind's, to start
    /// from; its id.
    pub fn add_kind(&mut self, name: &str, made_as: Kind) -> String {
        let taken: Vec<String> = self.kinds.iter().map(|k| k.id.clone()).chain(Kind::ALL.iter().map(|k| k.id().to_string())).collect();
        let id = Catalogue::free(name, &taken);
        let base = self.kind(made_as.id()).cloned();
        let (params, features) = base.map(|b| (b.params, b.features)).unwrap_or_else(|| (default_params(made_as), vec![]));
        self.kinds.push(KindDef { id: id.clone(), name: name.trim().to_string(), made_as, params, features, varieties: vec![] });
        id
    }

    /// A kind of the user's own taken out (the kit's own never are): whether it was. Its
    /// requests are then their making's kind's.
    pub fn remove_kind(&mut self, id: &str) -> bool {
        let before = self.kinds.len();
        self.kinds.retain(|k| k.id != id || k.built_in());
        self.kinds.len() != before
    }

    /// For the checks: the instructions for every kind ("*:"), then each kind's id (a kind of
    /// the user's own with its making), its instructions' keywords and its varieties'
    /// ("*:silhouette | environment:wind[grass:edges,pieces,density;pine:cones] | bush<environment>:wind").
    pub fn summary(&self) -> String {
        let words = |f: &[Feature]| f.iter().map(|f| f.keyword.trim().to_string()).collect::<Vec<_>>().join(",");
        let kinds = self.kinds.iter().map(|k| {
            let made = if k.built_in() { String::new() } else { format!("<{}>", k.made_as.id()) };
            let varieties = if k.varieties.is_empty() { String::new() } else { format!("[{}]", k.varieties.iter().map(|v| format!("{}:{}", v.id, words(&v.features))).collect::<Vec<_>>().join(";")) };
            format!("{}{made}:{}{varieties}", k.id, words(&k.features))
        });
        std::iter::once(format!("*:{}", words(&self.features))).chain(kinds).collect::<Vec<_>>().join(" | ")
    }
}

/// The window's kinds being shown and edited: the tab shown (None: all of them), the kind's
/// window open (the tab's; on All, the instructions for every kind) and the variety in it
/// (None: the kind itself), a kind being added (its name and making), a variety's name and
/// making being typed, when they last changed (written a moment after); the new request page
/// (shown in place of the list and the request) and what pictures dropped there start (a kind
/// and a variety of it, else the tab's kind).
#[derive(Default)]
pub(in super::super) struct KindsUi {
    pub tab: Option<String>,
    pub open: bool,
    pub variety: Option<String>,
    pub adding: Option<(String, Kind)>,
    pub new_variety: String,
    pub new_variety_as: Option<Kind>,
    pub changed: Option<f64>,
    pub page: bool,
    pub next: Option<(String, String)>,
}

impl KindsUi {
    /// The kinds written, a moment after their last change (`now`, seconds): where, or what went
    /// wrong.
    pub fn keep(&mut self, cat: &mut Catalogue, root: &Path, now: f64) -> Option<Result<PathBuf, String>> {
        let at = self.changed?;
        if now - at < KEEP_AFTER {
            return None;
        }
        self.changed = None;
        Some(cat.save(root))
    }
}

/// What a kind's tab and its button say of it.
fn about_kind(k: &KindDef) -> String {
    if k.built_in() {
        k.made_as.about().to_string()
    } else {
        format!("A kind of your own, made as {}: {} instruction{}", k.made_as.name(), k.features.len(), if k.features.len() == 1 { "" } else { "s" })
    }
}

/// The tab row over the list: All, each kind that has requests (how many), + for a kind of
/// your own, and Instructions… (the shown kind's window; on All, the instructions for every
/// kind).
pub(in super::super) fn tabs(ui: &mut egui::Ui, cat: &Catalogue, st: &mut KindsUi, counts: &BTreeMap<String, usize>, all: usize) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let r = ui.selectable_label(st.tab.is_none(), format!("All ({all})")).on_hover_text("Every request, whatever its kind");
        mark("uiRequestTab_all", &r);
        if r.clicked() {
            st.tab = None;
            st.open = false;
        }
        for k in &cat.kinds {
            let n = counts.get(&k.id).copied().unwrap_or(0);
            let on = st.tab.as_deref() == Some(k.id.as_str());
            if n == 0 && !on {
                continue;
            }
            let r = ui.selectable_label(on, format!("{} ({n})", k.name)).on_hover_text(format!("{}\nIts requests; pictures dropped while it's shown start one of it", about_kind(k)));
            mark(&format!("uiRequestTab_{}", k.id), &r);
            if r.clicked() && !on {
                st.tab = Some(k.id.clone());
                st.variety = None;
            }
        }
        let add = ui.small_button("+").on_hover_text("A kind of your own: its name, what the kit makes it as, then its settings and instructions");
        mark("uiRequestKindAdd", &add);
        if add.clicked() {
            let made = st.tab.as_deref().and_then(|t| cat.kind(t)).map_or(Kind::Prop, |k| k.made_as);
            st.adding = Some((String::new(), made));
        }
        let what = match st.tab.as_deref().and_then(|t| cat.kind(t)) {
            Some(k) => format!("{}'s instructions, settings and varieties", k.name),
            None => "The instructions for every kind, whatever its kind (a kind's with the same keyword replaces one)".to_string(),
        };
        let edit = ui.add(egui::Button::new("Instructions…").small()).on_hover_text(what);
        mark("uiRequestKindEdit", &edit);
        if edit.clicked() {
            st.open = true;
            st.variety = None;
        }
    });
}

/// The dialog adding a kind of your own (while one is being added): whether one was.
pub(in super::super) fn add_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    let Some((mut name, mut made)) = st.adding.clone() else { return false };
    let (mut done, mut added) = (None, false);
    let modal = egui::Modal::new(egui::Id::new("request-kind-new")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new("A kind of your own").size(18.0).strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name");
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY).hint_text("Bush, Crystal, Statue..."));
            mark("uiKindNewName", &r);
        });
        ui.horizontal(|ui| {
            ui.label("Made as");
            let r = egui::ComboBox::from_id_salt("request-kind-new-made").selected_text(made.name()).show_ui(ui, |ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut made, k, k.name()).on_hover_text(making(k));
                }
            });
            mark("uiKindNewMadeAs", &r.response);
        });
        hint(ui, &format!("How the kit makes it and what it becomes in the game: {} Its settings and instructions start as that kind's.", making(made)));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let go = ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Add"));
            mark("uiKindNewOk", &go);
            if go.clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    if modal.should_close() {
        done = done.or(Some(false));
    }
    match done {
        Some(true) => {
            let id = cat.add_kind(&name, made);
            st.tab = Some(id);
            (st.open, st.variety, st.adding) = (true, None, None);
            added = true;
        }
        Some(false) => st.adding = None,
        None => st.adding = Some((name, made)),
    }
    added
}

/// A section's title with an "i" by it that says more of it on hover.
fn titled(ui: &mut egui::Ui, title: &str, about: &str, name: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(section_title(title));
        row_icon(ui, egui::include_image!("../../../icons/info.svg"), false, about, name);
    });
    ui.add_space(2.0);
}

/// The instructions edited: each a keyword and how a run handles it (taken out by its ×; a
/// keyword used twice warned), one added at the end.
fn features_editor(ui: &mut egui::Ui, features: &mut Vec<Feature>) {
    if features.is_empty() {
        hint(ui, "None yet: add one, a keyword and how a run handles it.");
    }
    let mut gone = None;
    let keywords: Vec<String> = features.iter().map(|f| f.keyword.trim().to_lowercase()).collect();
    for (i, f) in features.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let key = ui.add(egui::TextEdit::singleline(&mut f.keyword).desired_width(100.0).hint_text("keyword"));
            mark(&format!("uiKindFeatureKey_{i}"), &key);
            let text = ui.add(egui::TextEdit::multiline(&mut f.text).desired_rows(2).desired_width(310.0).hint_text("How a run handles it"));
            mark(&format!("uiKindFeatureText_{i}"), &text);
            let x = ui.small_button("×").on_hover_text("Take it out");
            mark(&format!("uiKindFeatureRemove_{i}"), &x);
            if x.clicked() {
                gone = Some(i);
            }
        });
        let word = f.keyword.trim().to_lowercase();
        if !word.is_empty() && keywords.iter().filter(|w| **w == word).count() > 1 {
            warn(ui, &format!("\"{}\" is there twice: one keyword a feature", f.keyword.trim()));
        }
    }
    if let Some(i) = gone {
        features.remove(i);
    }
    let add = ui.button("+ Add an instruction").on_hover_text("A keyword for the feature (fur, colour, the trunk...), then how a run handles it");
    mark("uiKindFeatureAdd", &add);
    if add.clicked() {
        features.push(Feature::default());
    }
}

/// The window of the instructions for every kind (open from All): whether anything changed.
pub(in super::super) fn general_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    if !st.open || st.tab.is_some() {
        return false;
    }
    let before = cat.features.clone();
    let mut features = before.clone();
    let (mut open, mut done) = (true, false);
    egui::Window::new("Instructions for every kind").id(egui::Id::new("request-kind")).open(&mut open).collapsible(false).auto_sized().default_pos([320.0, 70.0]).show(ctx, |ui| {
        ui.set_width(470.0);
        hint(ui, "What every request's run is asked, whatever its kind, a feature each: a keyword (silhouette, colour, the scale...) and how to handle it. A kind's instruction with the same keyword replaces one. Kept as you type.");
        section(ui, "Instructions");
        features_editor(ui, &mut features);
        ui.add_space(6.0);
        let r = ui.button("Done");
        mark("uiKindDone", &r);
        done = r.clicked();
    });
    if !open || done {
        st.open = false;
    }
    let changed = features != before;
    if changed {
        cat.features = features;
    }
    changed
}

/// A kind's window (its tab's, while it's open): its name and making, its varieties, the
/// settings asked and its instructions (or a variety's). Whether anything changed.
pub(in super::super) fn kind_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    let Some(id) = st.tab.clone().filter(|_| st.open) else { return false };
    let Some(before) = cat.kind(&id).cloned() else {
        st.open = false;
        return false;
    };
    let mut k = before.clone();
    let (mut open, mut remove, mut done) = (true, false, false);
    // (fitted to what's in it: egui's windows are 420 tall else)
    egui::Window::new(format!("Kind: {}", before.name)).id(egui::Id::new("request-kind")).open(&mut open).collapsible(false).auto_sized().default_pos([320.0, 70.0]).show(ctx, |ui| {
        ui.set_width(470.0);
        hint(ui, "What every request of this kind asks Claude, a feature each: a keyword (fur, colour, the trunk...) and how to handle it. Kept as you type.");
        titled(ui, "Kind", &format!("How the kit makes it, whatever its instructions say: {} Every model is drawn with the toon look.", making(k.made_as)), "uiKindInfo");
        ui.horizontal(|ui| {
            ui.label("Name");
            let r = ui.add(egui::TextEdit::singleline(&mut k.name).desired_width(200.0));
            mark("uiKindName", &r);
            if k.built_in() {
                hint(ui, "one of the kit's own");
            } else {
                ui.label("made as");
                let r = egui::ComboBox::from_id_salt("request-kind-made").selected_text(k.made_as.name()).show_ui(ui, |ui| {
                    for m in Kind::ALL {
                        ui.selectable_value(&mut k.made_as, m, m.name()).on_hover_text(making(m));
                    }
                });
                mark("uiKindMadeAs", &r.response);
            }
        });
        if !k.built_in() {
            let r = ui.small_button("Delete this kind").on_hover_text("Its requests stay, as the kind they're made as");
            mark("uiKindDelete", &r);
            remove = r.clicked();
        }
        // its varieties: the kind itself, or one of them, to edit below; one added
        titled(ui, "Varieties", "A variety has its own settings and instructions, added to the kind's: one with the same keyword replaces the kind's. Made as its kind, unless it says how (the environment's grass, flower, rock and tree are each made their own way).", "uiKindVarietiesInfo");
        ui.horizontal_wrapped(|ui| {
            let r = ui.selectable_label(st.variety.is_none(), "the kind itself");
            mark("uiKindVariety_", &r);
            if r.clicked() {
                st.variety = None;
            }
            for v in &k.varieties {
                let r = ui.selectable_label(st.variety.as_deref() == Some(v.id.as_str()), &v.name).on_hover_text(match v.made_as {
                    Some(m) => format!("Made as {}: {}", m.name(), making(m)),
                    None => "Made as the kind".to_string(),
                });
                mark(&format!("uiKindVariety_{}", v.id), &r);
                if r.clicked() {
                    st.variety = Some(v.id.clone());
                }
            }
        });
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut st.new_variety).desired_width(130.0).hint_text("Pine, Palm, Willow..."));
            mark("uiKindVarietyName", &r);
            ui.label("made as");
            let shown = st.new_variety_as.map_or("the kind", |m| m.name());
            let r = egui::ComboBox::from_id_salt("request-variety-new-made").selected_text(shown).width(100.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.new_variety_as, None, "the kind");
                for m in Kind::ALL {
                    ui.selectable_value(&mut st.new_variety_as, Some(m), m.name()).on_hover_text(making(m));
                }
            });
            mark("uiKindVarietyMadeAs", &r.response);
            let add = ui.add_enabled(!st.new_variety.trim().is_empty(), egui::Button::new("Add a variety").small());
            mark("uiKindVarietyAdd", &add);
            if add.clicked() {
                st.variety = Some(k.add_variety(&st.new_variety, st.new_variety_as));
                st.new_variety.clear();
                st.new_variety_as = None;
            }
        });
        let kind_features = k.features.clone();
        let vi = st.variety.as_ref().and_then(|v| k.varieties.iter().position(|x| &x.id == v));
        if st.variety.is_some() && vi.is_none() {
            st.variety = None;
        }
        if let Some(vi) = vi {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Variety").strong());
                let r = ui.add(egui::TextEdit::singleline(&mut k.varieties[vi].name).desired_width(130.0));
                mark("uiKindVarietyRename", &r);
                ui.label("made as");
                let kind_made = k.made_as;
                let made = &mut k.varieties[vi].made_as;
                let shown = made.map_or("the kind", |m| m.name());
                let r = egui::ComboBox::from_id_salt("request-variety-made").selected_text(shown).width(100.0).show_ui(ui, |ui| {
                    ui.selectable_value(made, None, "the kind");
                    for m in Kind::ALL.into_iter().filter(|m| *m != kind_made) {
                        ui.selectable_value(made, Some(m), m.name()).on_hover_text(making(m));
                    }
                });
                mark("uiKindVarietyMade", &r.response);
                let del = ui.small_button("Delete the variety");
                mark("uiKindVarietyDelete", &del);
                if del.clicked() {
                    k.varieties.remove(vi);
                    st.variety = None;
                }
            });
        }
        let vi = vi.filter(|&i| i < k.varieties.len());
        // the settings asked: the variety's or the kind's, as the making can ask them
        let made = vi.map_or(k.made_as, |i| k.varieties[i].making(k.made_as));
        titled(ui, if vi.is_some() { "Settings its requests are asked" } else { "Settings the kind's requests are asked" }, "Lit: asked in the request's panel. What the kit makes it by (its height, a grass's cells, a rock's footprint) is always asked.", "uiKindParamsInfo");
        {
            let params = match vi {
                Some(i) => &mut k.varieties[i].params,
                None => &mut k.params,
            };
            ui.horizontal_wrapped(|ui| {
                for p in Param::ALL.into_iter().filter(|p| p.fits(made)) {
                    let needed = p.needed(made);
                    let on = needed || params.contains(&p);
                    let r = ui.add_enabled(!needed, egui::Button::selectable(on, p.name())).on_disabled_hover_text("The kit makes it by it: always asked");
                    mark(&format!("uiKindParam_{}", p.id()), &r);
                    if r.clicked() && !needed {
                        if on {
                            params.retain(|q| *q != p);
                        } else {
                            params.push(p);
                            params.sort();
                        }
                    }
                }
            });
        }
        // the instructions: the variety's (the kind's shown, folded) or the kind's
        section(ui, "Instructions");
        if vi.is_some() {
            egui::CollapsingHeader::new(format!("The kind's ({})", kind_features.len())).id_salt("request-kind-own").show(ui, |ui| {
                for f in &kind_features {
                    ui.label(RichText::new(format!("{}: {}", f.keyword, f.text)).size(11.5).color(Color32::from_gray(180)));
                }
                if kind_features.is_empty() {
                    hint(ui, "none");
                }
            });
        }
        let features = match vi {
            Some(i) => &mut k.varieties[i].features,
            None => &mut k.features,
        };
        features_editor(ui, features);
        ui.add_space(6.0);
        let r = ui.button("Done");
        mark("uiKindDone", &r);
        done = r.clicked();
    });
    if remove {
        cat.remove_kind(&id);
        (st.tab, st.open, st.variety) = (None, false, None);
        return true;
    }
    if !open || done {
        st.open = false;
    }
    let changed = k != before;
    if changed {
        if let Some(slot) = cat.kind_mut(&id) {
            *slot = k;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard kinds: character (a creature its variety), prop, environment (grass, a
    /// flower, a rock and a tree its varieties, each made its own way), floor, each asking
    /// what the window always asked, with the instructions written so far; a request's
    /// settings, instructions and label from its kind and its variety (one of its instructions
    /// replacing the kind's of the same keyword, the instructions for every kind first); a kind
    /// of the user's own starting as its making's, taken out again (the kit's own never are);
    /// what older files leave out or name otherwise completed.
    #[test]
    fn the_kinds_shape_what_a_request_asks() {
        let mut c = Catalogue::standard();
        assert_eq!(c.kinds.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["character", "prop", "environment", "floor"]);
        for k in &c.kinds {
            assert!(k.built_in(), "{}", k.id);
            assert_eq!(k.params, default_params(k.made_as), "{}: what the window always asked", k.id);
        }
        let env = c.kind("environment").unwrap();
        assert_eq!(env.varieties.iter().map(|v| (v.id.as_str(), v.made_as)).collect::<Vec<_>>(), [("grass", Some(Kind::Grass)), ("flower", Some(Kind::Flower)), ("rock", Some(Kind::Rock)), ("tree", Some(Kind::Tree))]);
        assert_eq!(env.variety("tree").unwrap().features.iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["trunk", "foliage", "colour", "wind"]);
        assert_eq!(c.kind("character").unwrap().varieties.iter().map(|v| (v.id.as_str(), v.made_as)).collect::<Vec<_>>(), [("creature", None)]);
        // a rock: the environment's variety, made as a rock, its settings a rock's
        let rock = ModelRequest { kind: Kind::Rock, kind_id: "environment".into(), variety: "rock".into(), ..Default::default() };
        assert_eq!(c.params_of(&rock), [Param::Cells, Param::Style, Param::Shapes, Param::Variants, Param::Many, Param::CheckLines]);
        assert_eq!(c.label(&rock), "rock");
        assert_eq!(c.features_of(&rock).iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["wind", "detail", "colour", "moss", "plants"], "the environment's wind, then the rock's own");
        // a variety of the user's own, made its own way (a pine, a tree): its settings, its
        // instructions over the kind's; the instructions for every kind first
        c.features = vec![Feature { keyword: "silhouette".into(), text: "from the side first".into() }, Feature { keyword: "Wind".into(), text: "none".into() }];
        let pine = c.kind_mut("environment").unwrap().add_variety("Pine", Some(Kind::Tree));
        assert_eq!(pine, "pine");
        let v = c.kind_mut("environment").unwrap().varieties.iter_mut().find(|v| v.id == "pine").unwrap();
        assert_eq!((v.made_as, v.params.clone()), (Some(Kind::Tree), default_params(Kind::Tree)), "made as a tree: a tree's settings to start");
        v.params = vec![Param::Height, Param::Style];
        v.features = vec![Feature { keyword: "cones".into(), text: "brown".into() }];
        let r = ModelRequest { kind: Kind::Tree, kind_id: "environment".into(), variety: "pine".into(), ..Default::default() };
        assert_eq!(c.params_of(&r), [Param::Height, Param::Style]);
        let f = c.features_of(&r);
        assert_eq!(f.iter().map(|f| (f.keyword.as_str(), f.text.as_str())).collect::<Vec<_>>()[..1], [("silhouette", "from the side first")]);
        assert_eq!(f.iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["silhouette", "wind", "cones"], "the environment's wind in the general one's place, the pine's cones after");
        assert_ne!(f[1].text, "none");
        assert_eq!(c.label(&r), "pine");
        // a kind of the user's own: made as environment, starting as its settings and instructions
        let bush = c.add_kind("Bush", Kind::Environment);
        assert_eq!(bush, "bush");
        let b = ModelRequest { kind: Kind::Environment, kind_id: "bush".into(), ..Default::default() };
        assert_eq!((c.label(&b), c.params_of(&b)), ("bush".to_string(), default_params(Kind::Environment)));
        assert_eq!(c.features_of(&b).len(), c.features.len() + c.kind("environment").unwrap().features.len() - 1, "the general wind replaced by the environment's");
        let s = c.summary();
        assert!(s.starts_with("*:silhouette,Wind | character:[creature:] | ") && s.contains("bush<environment>:wind") && s.contains("environment:wind[grass:edges,pieces,density;flower:") && s.contains(";pine:cones]"), "{s}");
        // the kit needs a grass's cells, whatever its kind says; a creature can't be strewn
        c.kind_mut("environment").unwrap().varieties.iter_mut().find(|v| v.id == "grass").unwrap().params = vec![];
        assert_eq!(c.params_of(&ModelRequest { kind: Kind::Grass, kind_id: "environment".into(), variety: "grass".into(), ..Default::default() }), [Param::Height, Param::Cells]);
        assert!(!Param::Many.fits(Kind::Character) && Param::Bones.fits(Kind::Character) && !Param::Bones.fits(Kind::Prop));
        // taken out: a kind of the user's own (its requests then their making's), never the kit's own
        assert!(c.remove_kind("bush") && !c.remove_kind("environment") && c.kind("environment").is_some());
        assert_eq!(c.label(&b), "environment", "its kind gone: the kind it's made as");
        assert_eq!(Catalogue::add_kind(&mut c, "Tree", Kind::Tree), "tree_2", "never a kit kind's id");
        // a file from before: a creature kind (the character kind now, a creature its variety),
        // grass as a kind of its own (kept), the rest completed as the standard has it
        let mut old: Catalogue = serde_json::from_str(r#"{"kinds": [{"id": "creature", "name": "Creature", "made_as": "creature", "params": ["height"], "features": [{"keyword": "fur", "text": "soft"}]}, {"id": "grass", "name": "Grass", "made_as": "grass", "params": ["cells"]}]}"#).unwrap();
        old.complete();
        assert_eq!(old.kinds.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["character", "grass", "prop", "environment", "floor"]);
        let ch = old.kind("character").unwrap();
        assert!(ch.made_as == Kind::Character && ch.name == "Character" && ch.features.len() == 1 && ch.variety("creature").is_some(), "{ch:?}");
        assert_eq!(old.kind("environment").unwrap().varieties.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["flower", "rock", "tree"], "grass is a kind of its own there");
    }

    /// Kept as written: the standard file reads back the same, and the editor writes it in its
    /// own layout (so a save changes only what was changed); a project's own copy read first.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_kinds_are_kept_where_they_were_read() {
        let c = Catalogue::standard();
        // (a checkout on Windows may give the file CRLF line ends)
        assert_eq!(serde_json::to_string_pretty(&c).unwrap() + "\n", STANDARD.replace("\r\n", "\n"), "the standard file in the editor's own layout");
        let root = std::env::temp_dir().join(format!("bf-kinds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("private/requests")).unwrap();
        let (mut fresh, trouble) = Catalogue::load(&root);
        assert!(trouble.is_none() && fresh.from.is_none() && fresh.kinds.len() == 4, "no file: the standard");
        fresh.add_kind("Bush", Kind::Environment);
        let at = fresh.save(&root).unwrap();
        assert_eq!(at, root.join(PROJECT), "the desktop writes the project's file");
        std::fs::write(folder(&root).join(OWN), serde_json::to_string(&Catalogue::standard()).unwrap()).unwrap();
        let (own, _) = Catalogue::load(&root);
        assert!(own.kind("bush").is_none() && own.from == Some(folder(&root).join(OWN)), "a project's own copy first");
        std::fs::write(folder(&root).join(OWN), "not json").unwrap();
        let (fallen, trouble) = Catalogue::load(&root);
        assert!(trouble.is_some_and(|t| t.contains("kinds.json")) && fallen.kind("bush").is_some(), "unreadable: said, the next one read");
        let _ = std::fs::remove_dir_all(&root);
    }
}
