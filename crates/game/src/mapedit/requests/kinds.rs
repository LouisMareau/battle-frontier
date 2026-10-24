//! The window's categories (its tabs; the files and the code call them kinds): each made as
//! one of the kit's eight (`Kind`: how the kit makes it and what it becomes in the game), with
//! the settings its requests are asked (`Param`, from the window's own pool), its instructions
//! (`Feature`: a keyword and how a run handles it, each a line of every run's prompt) and its
//! varieties, which a request is one of (a category is never asked for as itself: each has a
//! generic variety named after it; a creature is a variety of character; the environment's
//! grass, flower, rock and tree are each made their own way: `Variety::made_as`), with
//! settings and instructions of their own added to the category's (inherited: one with the
//! same keyword replacing the category's). The catalogue's own instructions go with every
//! request, whatever its category. The standard set is tools/blender/request_kinds.json; the
//! desktop edits that file (the one it read), a browser keeps its own copy
//! (private/requests/kinds.json), which its runs are given. The kit reads them as the window
//! does (model_requests.py's `catalogue`, `instructions`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};

use super::super::assets::slug;
use super::super::{hint, mark, row_icon, section, section_title, warn};
use super::desk::folder;
use super::disk;
use super::{Kind, ModelRequest};

/// The standard categories, as the project has them.
pub(super) const STANDARD: &str = include_str!("../../../../../tools/blender/request_kinds.json");
/// The standard categories' file in the project, which the desktop edits.
const PROJECT: &str = "tools/blender/request_kinds.json";
/// A browser's own copy, in the requests' folder (and its runs').
const OWN: &str = "kinds.json";
/// How long after the last change the categories are written (seconds).
const KEEP_AFTER: f64 = 0.6;
/// A variety just added, before it's named: its id's stem (named, the id follows the name).
const NEW_VARIETY: &str = "variety";

/// A setting the window asks a request, from its own pool: which a category's requests are
/// asked.
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

/// What a category asks of every run about one feature: its keyword ("fur", "colour") and how
/// a run handles it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct Feature {
    pub keyword: String,
    pub text: String,
}

/// A variety of a category, what a request is one of (a creature of the characters, a pine of
/// the environment's; the generic one named after its category): its own settings, added to
/// the category's, and its own instructions, added to the category's (inherited); made as
/// its category, unless it says how (the environment's grass, flower, rock and tree, each
/// made its own way by the kit).
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
    /// How the kit makes it: its own way, else its category's (`kind`).
    pub fn making(&self, kind: Kind) -> Kind {
        self.made_as.unwrap_or(kind)
    }
}

/// One of the window's categories.
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

    /// The variety a request of `id` is: that one, else the generic one (named after the
    /// category's making), else the first.
    pub fn variety_of(&self, id: &str) -> Option<&Variety> {
        self.variety(id).or_else(|| self.variety(self.made_as.id())).or_else(|| self.varieties.first())
    }

    /// The generic variety: named after the category, made as it, with nothing of its own.
    fn generic(made_as: Kind) -> Variety {
        let name = if made_as == Kind::Floor { "Floor set".to_string() } else { capital(made_as.name()) };
        Variety { id: made_as.id().into(), name, made_as: None, params: vec![], features: vec![] }
    }

    /// A variety added, made as the category (None) or its own way, nothing of its own yet
    /// (the category's settings and instructions are inherited); named `name`, or not yet
    /// (its id a stem, following the name once it's given). Its id.
    pub fn add_variety(&mut self, name: &str, made_as: Option<Kind>) -> String {
        let taken: Vec<String> = self.varieties.iter().map(|v| v.id.clone()).collect();
        let id = Catalogue::free(if name.trim().is_empty() { NEW_VARIETY } else { name }, &taken);
        self.varieties.push(Variety { id: id.clone(), name: name.trim().to_string(), made_as: made_as.filter(|m| *m != self.made_as), params: vec![], features: vec![] });
        id
    }

    /// A variety named (its edit icon): one just added, with nothing of its own yet, takes
    /// the name's slug as its id (free); the id of one that's in use stays. The variety's id.
    pub fn name_variety(&mut self, id: &str, name: &str) -> String {
        let Some(i) = self.varieties.iter().position(|v| v.id == id) else { return id.to_string() };
        let fresh = self.varieties[i].id.starts_with(NEW_VARIETY) && self.varieties[i].features.is_empty() && self.varieties[i].params.is_empty();
        self.varieties[i].name = name.trim().to_string();
        if fresh && !name.trim().is_empty() {
            let taken: Vec<String> = self.varieties.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, v)| v.id.clone()).collect();
            self.varieties[i].id = Catalogue::free(name, &taken);
        }
        self.varieties[i].id.clone()
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

/// The window's categories, the instructions for every one, and where they were read from
/// (the desktop writes them back there).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(in super::super) struct Catalogue {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub about: String,
    /// what every request's run is asked, whatever its category (a category's with the same
    /// keyword replaces one)
    pub features: Vec<Feature>,
    pub kinds: Vec<KindDef>,
    #[serde(skip)]
    pub from: Option<PathBuf>,
}

impl Catalogue {
    /// The standard categories (the project's file as built in).
    pub fn standard() -> Catalogue {
        let mut c: Catalogue = serde_json::from_str(STANDARD).unwrap_or_default();
        c.complete();
        c
    }

    /// The categories as the window has them: a browser's own copy (or the desktop's, if one
    /// was put there), else the project's file, else the standard; what was wrong with one that
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
                Err(e) => trouble = Some(format!("{}: {e} (the standard categories instead)", path.display())),
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

    /// Every one of the kit's eight made somewhere: a category of it, or a variety made as it,
    /// added as the standard has it when a file leaves one out (an older one: its creature
    /// category is the character category now, with a creature variety; its grass, flower,
    /// rock and tree categories stay its own); every category with a variety at least (the
    /// generic one, named after it: a request is always one of a variety).
    fn complete(&mut self) {
        let standard: Catalogue = serde_json::from_str(STANDARD).unwrap_or_default();
        if let Some(k) = self.kinds.iter_mut().find(|k| k.id == "creature" && k.made_as == Kind::Character) {
            k.id = "character".into();
            if k.name.trim().eq_ignore_ascii_case("creature") {
                k.name = "Character".into();
            }
            if k.variety("creature").is_none() {
                k.varieties.insert(0, Variety { id: "creature".into(), name: "Creature".into(), made_as: None, params: vec![], features: vec![] });
            }
        }
        // the standard's categories first (the environment before its varieties), then what it
        // hosts as a variety (else a category of its own, as the window always asked)
        for pass in 0..2 {
            for k in Kind::ALL {
                if self.kinds.iter().any(|d| d.id == k.id() || d.varieties.iter().any(|v| v.made_as == Some(k))) {
                    continue;
                }
                match (pass, standard.kinds.iter().find(|d| d.id == k.id())) {
                    (0, Some(def)) => {
                        // (a variety made as a category the file has of its own: that category's)
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
        for k in &mut self.kinds {
            if k.variety(k.made_as.id()).is_none() {
                k.varieties.insert(0, KindDef::generic(k.made_as));
            }
        }
    }

    pub fn kind(&self, id: &str) -> Option<&KindDef> {
        self.kinds.iter().find(|k| k.id == id)
    }

    pub fn kind_mut(&mut self, id: &str) -> Option<&mut KindDef> {
        self.kinds.iter_mut().find(|k| k.id == id)
    }

    /// A request's category (its window category's, else the kit's own it's made as) and its
    /// variety (the one it names; else the category's generic one).
    pub fn of(&self, r: &ModelRequest) -> (Option<&KindDef>, Option<&Variety>) {
        let k = self.kind(&r.kind_key()).or_else(|| self.kind(r.kind.id()));
        let v = k.and_then(|k| k.variety_of(&r.variety));
        (k, v)
    }

    /// The settings a request is asked: its category's and its variety's own (else as the
    /// window always asked), those its making can ask, with what the kit needs.
    pub fn params_of(&self, r: &ModelRequest) -> Vec<Param> {
        let (k, v) = self.of(r);
        let chosen: Vec<Param> = match k {
            Some(k) => k.params.iter().chain(v.map(|v| v.params.iter()).into_iter().flatten()).copied().collect(),
            None => default_params(r.kind),
        };
        Param::ALL.into_iter().filter(|p| p.fits(r.kind) && (p.needed(r.kind) || chosen.contains(p))).collect()
    }

    /// What a request's run is asked: the instructions for every category, then its
    /// category's, then its variety's, one with the same keyword replacing the earlier where
    /// it was (as model_requests.py's `instructions`).
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

    /// A request's variety as the cards say it ("creature", "rock", "pine"; a generic one is
    /// named after its category), else its category's name.
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

    /// A category added, made as `made_as`: its settings and instructions that category's, to
    /// start from, with a generic variety; its id.
    pub fn add_kind(&mut self, name: &str, made_as: Kind) -> String {
        let taken: Vec<String> = self.kinds.iter().map(|k| k.id.clone()).chain(Kind::ALL.iter().map(|k| k.id().to_string())).collect();
        let id = Catalogue::free(name, &taken);
        let base = self.kind(made_as.id()).cloned();
        let (params, features) = base.map(|b| (b.params, b.features)).unwrap_or_else(|| (default_params(made_as), vec![]));
        self.kinds.push(KindDef { id: id.clone(), name: name.trim().to_string(), made_as, params, features, varieties: vec![KindDef::generic(made_as)] });
        id
    }

    /// A category of the user's own taken out (the kit's own never are): whether it was. Its
    /// requests are then their making's category's.
    pub fn remove_kind(&mut self, id: &str) -> bool {
        let before = self.kinds.len();
        self.kinds.retain(|k| k.id != id || k.built_in());
        self.kinds.len() != before
    }

    /// For the checks: the instructions for every category ("*:"), then each category's id (a
    /// category of the user's own with its making), its instructions' keywords and its
    /// varieties' ("*:silhouette | environment:wind[environment:;grass:edges,pieces,density;pine:cones] | bush<environment>:wind[environment:]").
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

/// The window's categories being shown and edited: the tab shown (None: all of them), the
/// category's window open (the tab's; on All, the instructions for every category) and the
/// variety to unfold in it, a category being added (its name and making), a variety being
/// named (its id), when they last changed (written a moment after); the new request page
/// (shown in place of the list and the request) and what pictures dropped there start (a
/// category and a variety of it, else the tab's).
#[derive(Default)]
pub(in super::super) struct KindsUi {
    pub tab: Option<String>,
    pub open: bool,
    pub variety: Option<String>,
    pub adding: Option<(String, Kind)>,
    pub renaming: Option<String>,
    pub changed: Option<f64>,
    pub page: bool,
    pub next: Option<(String, String)>,
}

impl KindsUi {
    /// The categories written, a moment after their last change (`now`, seconds): where, or
    /// what went wrong.
    pub fn keep(&mut self, cat: &mut Catalogue, root: &Path, now: f64) -> Option<Result<PathBuf, String>> {
        let at = self.changed?;
        if now - at < KEEP_AFTER {
            return None;
        }
        self.changed = None;
        Some(cat.save(root))
    }
}

/// What a category's tab and its button say of it.
pub(in super::super) fn about_kind(k: &KindDef) -> String {
    if k.built_in() {
        k.made_as.about().to_string()
    } else {
        format!("A category of your own, made as {}: {} instruction{}", k.made_as.name(), k.features.len(), if k.features.len() == 1 { "" } else { "s" })
    }
}

/// A tab: its name with its count as a small tag ("Prop" and a 3), lit when it's the one
/// shown; while the pointer is over it, an edit icon in the tag's place (to open its window),
/// so nothing moves. `marks`: the tab's and the icon's. (the tab clicked, the icon clicked)
pub(in super::super) fn tab(ui: &mut egui::Ui, selected: bool, text: &str, count: Option<usize>, tip: &str, marks: (&str, &str), edit_tip: &str) -> (bool, bool) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, ui.visuals().text_color());
    let counted = count.map(|n| ui.painter().layout_no_wrap(n.to_string(), egui::FontId::proportional(9.0), ui.visuals().text_color()));
    let pad = ui.spacing().button_padding;
    let icon = 12.0;
    // the slot after the name: the tag's width (its number, padded), or the icon's, whichever is wider
    let tag = counted.as_ref().map(|c| egui::vec2(c.size().x + 8.0, c.size().y + 3.0));
    let slot = tag.map_or(icon, |t| t.x.max(icon));
    let size = egui::vec2(galley.size().x + pad.x * 2.0 + 5.0 + slot, galley.size().y + pad.y * 2.0);
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let visuals = ui.style().interact_selectable(&r, selected);
    if selected || hovered {
        ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
    }
    // the one shown: its name white, its tag a hole in the panel's background
    let text_color = if selected { Color32::WHITE } else { visuals.text_color() };
    ui.painter().galley(rect.min + pad, galley, text_color);
    let slot_rect = egui::Rect::from_min_size(egui::pos2(rect.max.x - pad.x - slot, rect.min.y), egui::vec2(slot, rect.height()));
    if let (Some(c), Some(t), false) = (counted, tag, hovered) {
        let tag_rect = egui::Rect::from_center_size(slot_rect.center(), t);
        let fill = if selected { ui.visuals().panel_fill } else { Color32::from_gray(72) };
        ui.painter().rect_filled(tag_rect, 3.0, fill);
        ui.painter().galley(egui::pos2(tag_rect.center().x - c.size().x / 2.0, tag_rect.center().y - c.size().y / 2.0), c, text_color);
    }
    // the icon, painted in the slot (no room of its own in the row: the tabs line up), its
    // own click over the tab's
    let icon_rect = egui::Rect::from_center_size(slot_rect.center(), egui::vec2(icon, icon));
    let e = ui.interact(icon_rect, egui::Id::new(("tab-edit", marks.1)), egui::Sense::click());
    if hovered {
        egui::Image::new(egui::include_image!("../../../icons/edit.svg")).fit_to_exact_size(egui::vec2(icon, icon)).tint(text_color).paint_at(ui, icon_rect);
    }
    let e = if hovered { e.on_hover_text(edit_tip) } else { e };
    let r = r.on_hover_text(tip);
    mark(marks.0, &r);
    mark(marks.1, &e);
    (r.clicked() && !e.clicked(), e.clicked())
}

/// A box round a row's widgets (the new request's +; the categories).
fn boxed<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new().stroke(egui::Stroke::new(1.0, Color32::from_gray(70))).corner_radius(4.0).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, add).inner
}

/// The tab row over the list: first, in a box of its own, + for a new request (the page);
/// then, boxed together, All and each category that has requests (how many), each with an
/// edit icon while hovered (its window; All's: the instructions for every category), and +
/// for a category of your own. Whether a new request was asked for.
pub(in super::super) fn tabs(ui: &mut egui::Ui, cat: &Catalogue, st: &mut KindsUi, counts: &BTreeMap<String, usize>, all: usize) -> bool {
    let mut new = false;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        boxed(ui, |ui| {
            let r = ui.button("+").on_hover_text("A new request: the model, its category and a variety, then its artwork dropped on the window");
            mark("uiRequestNew", &r);
            new = r.clicked();
        });
        boxed(ui, |ui| ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (clicked, edit) = tab(ui, st.tab.is_none(), "All", Some(all), &format!("Every request, whatever its category: {all}"), ("uiRequestTab_all", "uiRequestTab_allEdit"), "The instructions for every category, whatever a request's (a category's with the same keyword replaces one)");
        if clicked {
            st.tab = None;
            st.open = false;
        }
        if edit {
            (st.tab, st.open, st.variety) = (None, true, None);
        }
        for k in &cat.kinds {
            let n = counts.get(&k.id).copied().unwrap_or(0);
            let on = st.tab.as_deref() == Some(k.id.as_str());
            if n == 0 && !on {
                continue;
            }
            let tip = format!("{}\nIts {n} request{}; pictures dropped while it's shown start one of it", about_kind(k), if n == 1 { "" } else { "s" });
            let (clicked, edit) = tab(ui, on, &k.name, Some(n), &tip, (&format!("uiRequestTab_{}", k.id), &format!("uiRequestTab_{}Edit", k.id)), &format!("{}'s instructions, settings and varieties", k.name));
            if clicked && !on {
                st.tab = Some(k.id.clone());
                st.variety = None;
            }
            if edit {
                (st.tab, st.open, st.variety) = (Some(k.id.clone()), true, None);
            }
        }
        let add = ui.small_button("+").on_hover_text("A category of your own: its name, what the kit makes it as, then its settings, instructions and varieties");
        mark("uiRequestKindAdd", &add);
        if add.clicked() {
            let made = st.tab.as_deref().and_then(|t| cat.kind(t)).map_or(Kind::Prop, |k| k.made_as);
            st.adding = Some((String::new(), made));
        }
        }));
    });
    new
}

/// The dialog adding a category of your own (while one is being added): whether one was.
pub(in super::super) fn add_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    let Some((mut name, mut made)) = st.adding.clone() else { return false };
    let (mut done, mut added) = (None, false);
    let modal = egui::Modal::new(egui::Id::new("request-kind-new")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new("A category of your own").size(18.0).strong());
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
        hint(ui, &format!("How the kit makes it and what it becomes in the game: {} Its settings and instructions start as that category's, with a variety named after it.", making(made)));
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

/// A section's title with an "i" by it that says more of it on hover, and a + at its right
/// when `add` names one (what it adds): whether the + was clicked.
fn titled(ui: &mut egui::Ui, title: &str, about: &str, name: &str, add: Option<(&str, &str)>) -> bool {
    ui.add_space(8.0);
    let mut added = false;
    ui.horizontal(|ui| {
        ui.label(section_title(title));
        row_icon(ui, egui::include_image!("../../../icons/info.svg"), false, about, name);
        if let Some((tip, add_name)) = add {
            added = row_icon(ui, egui::include_image!("../../../icons/plus.svg"), true, tip, add_name).clicked();
        }
    });
    ui.add_space(2.0);
    added
}

/// The instructions edited: each a keyword and how a run handles it (taken out by its ×; a
/// keyword used twice warned), one added at the end. Marked `{prefix}Key_i`, `{prefix}Text_i`,
/// `{prefix}Remove_i` and `{prefix}Add`.
fn features_editor(ui: &mut egui::Ui, features: &mut Vec<Feature>, prefix: &str) {
    if features.is_empty() {
        hint(ui, "None yet: add one, a keyword and how a run handles it.");
    }
    let mut gone = None;
    let keywords: Vec<String> = features.iter().map(|f| f.keyword.trim().to_lowercase()).collect();
    for (i, f) in features.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let key = ui.add(egui::TextEdit::singleline(&mut f.keyword).desired_width(100.0).hint_text("keyword"));
            mark(&format!("{prefix}Key_{i}"), &key);
            let text = ui.add(egui::TextEdit::multiline(&mut f.text).desired_rows(2).desired_width(300.0).hint_text("How a run handles it"));
            mark(&format!("{prefix}Text_{i}"), &text);
            let x = ui.small_button("×").on_hover_text("Take it out");
            mark(&format!("{prefix}Remove_{i}"), &x);
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
    mark(&format!("{prefix}Add"), &add);
    if add.clicked() {
        features.push(Feature::default());
    }
}

/// The settings toggled: lit when asked; those the kit needs lit and left alone, those
/// `fixed` (a variety's inherited) left out: the category's, asked already. Marked
/// `{prefix}{param id}`. Whether any was shown.
fn params_toggles(ui: &mut egui::Ui, params: &mut Vec<Param>, made: Kind, fixed: &[Param], prefix: &str) -> bool {
    let mut shown = false;
    ui.horizontal_wrapped(|ui| {
        for p in Param::ALL.into_iter().filter(|p| p.fits(made) && !fixed.contains(p) && !(p.needed(made) && !fixed.is_empty())) {
            shown = true;
            let held = p.needed(made);
            let on = held || params.contains(&p);
            let r = ui.add_enabled(!held, egui::Button::selectable(on, p.name())).on_disabled_hover_text("The kit makes it by it: always asked");
            mark(&format!("{prefix}{}", p.id()), &r);
            if r.clicked() && !held {
                if on {
                    params.retain(|q| *q != p);
                } else {
                    params.push(p);
                    params.sort();
                }
            }
        }
    });
    shown
}

/// The window of the instructions for every category (All's edit icon): whether anything
/// changed.
pub(in super::super) fn general_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    if !st.open || st.tab.is_some() {
        return false;
    }
    let before = cat.features.clone();
    let mut features = before.clone();
    let (mut open, mut done) = (true, false);
    egui::Window::new("Instructions for every category").id(egui::Id::new("request-kind")).open(&mut open).collapsible(false).auto_sized().default_pos([320.0, 70.0]).show(ctx, |ui| {
        ui.set_width(470.0);
        hint(ui, "What every request's run is asked, whatever its category, a feature each: a keyword (silhouette, colour, the scale...) and how to handle it. A category's instruction with the same keyword replaces one. Kept as you type.");
        section(ui, "Instructions");
        features_editor(ui, &mut features, "uiKindFeature");
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

/// A category's window (its tab's, while it's open): its name and making, its instructions
/// and the settings its requests are asked, then its varieties folded, each with its own
/// instructions (the category's inherited, shown) and settings (the inherited ones held).
/// Whether anything changed.
pub(in super::super) fn kind_window(ctx: &egui::Context, cat: &mut Catalogue, st: &mut KindsUi) -> bool {
    let Some(id) = st.tab.clone().filter(|_| st.open) else { return false };
    let Some(before) = cat.kind(&id).cloned() else {
        st.open = false;
        return false;
    };
    let mut k = before.clone();
    let (mut open, mut remove, mut done) = (true, false, false);
    // the variety to unfold (and to name) this frame: asked for once
    let unfold = st.variety.take();
    let window_id = egui::Id::new("request-kind");
    // (fitted to what's in it: egui's windows are 420 tall else; the varieties may make it tall)
    let room = (ctx.content_rect().height() - 120.0).max(240.0);
    egui::Window::new(format!("Category: {}", before.name)).id(window_id).open(&mut open).collapsible(false).auto_sized().default_pos([320.0, 70.0]).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
            ui.set_width(470.0);
            hint(ui, "What every request of this category asks Claude, a feature each: a keyword (fur, colour, the trunk...) and how to handle it; its varieties add their own. Kept as you type.");
            titled(ui, "Category", &format!("How the kit makes it, whatever its instructions say: {} Every model is drawn with the toon look.", making(k.made_as)), "uiKindInfo", None);
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
                let r = ui.small_button("Delete this category").on_hover_text("Its requests stay, as the category they're made as");
                mark("uiKindDelete", &r);
                remove = r.clicked();
            }
            section(ui, "Instructions");
            features_editor(ui, &mut k.features, "uiKindFeature");
            titled(ui, "Settings the category's requests are asked", "Lit: asked in the request's panel, of every variety. What the kit makes it by (its height, a grass's cells, a rock's footprint) is always asked.", "uiKindParamsInfo", None);
            let made = k.made_as;
            params_toggles(ui, &mut k.params, made, &[], "uiKindParam_");
            // its varieties, each folded: what a request is one of
            let add = titled(ui, "Varieties", "What a request of this category is one of (the generic one is named after the category). A variety inherits the category's instructions and settings, and adds its own: one with the same keyword replaces the category's. Made as its category, unless it says how (the environment's grass, flower, rock and tree are each made their own way).", "uiKindVarietiesInfo", Some(("Add a variety: name it by its edit icon", "uiKindVarietyAdd")));
            if add {
                let vid = k.add_variety("", None);
                st.renaming = Some(vid.clone());
                st.variety = Some(vid);
            }
            let base = k.params.clone();
            let mut gone = None;
            let mut named: Option<(String, String)> = None;
            for vi in 0..k.varieties.len() {
                let vid = k.varieties[vi].id.clone();
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ctx, window_id.with(("variety", &vid)), false);
                if unfold.as_deref() == Some(vid.as_str()) {
                    state.set_open(true);
                }
                let renaming = st.renaming.as_deref() == Some(vid.as_str());
                let count = k.varieties.len();
                let header = state.show_header(ui, |ui| {
                    if renaming {
                        let mut name = k.varieties[vi].name.clone();
                        let field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0).hint_text("Its name"));
                        mark(&format!("uiVarietyName_{vid}"), &field);
                        if unfold.as_deref() == Some(vid.as_str()) {
                            field.request_focus();
                        }
                        if name != k.varieties[vi].name {
                            k.varieties[vi].name = name.clone();
                        }
                        let entered = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if field.lost_focus() || entered {
                            named = Some((vid.clone(), name));
                        }
                    } else {
                        let name = if k.varieties[vi].name.trim().is_empty() { "(unnamed)".to_string() } else { k.varieties[vi].name.clone() };
                        ui.label(RichText::new(name).strong());
                        if let Some(m) = k.varieties[vi].made_as {
                            ui.label(RichText::new(format!("made as {}", m.name())).size(11.0).color(Color32::GRAY));
                        }
                        if row_icon(ui, egui::include_image!("../../../icons/edit.svg"), false, "Name it", &format!("uiVarietyRename_{vid}")).clicked() {
                            st.renaming = Some(vid.clone());
                            st.variety = Some(vid.clone());
                        }
                        let del = ui.add_enabled_ui(count > 1, |ui| row_icon(ui, egui::include_image!("../../../icons/trash.svg"), false, "Delete the variety (its requests are the generic one's then)", &format!("uiVarietyDelete_{vid}"))).inner;
                        if del.clicked() {
                            gone = Some(vi);
                        }
                    }
                });
                header.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Made as");
                        let v = &mut k.varieties[vi];
                        let shown = v.made_as.map_or("the category", |m| m.name());
                        let r = egui::ComboBox::from_id_salt(("request-variety-made", &vid)).selected_text(shown).width(110.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut v.made_as, None, "the category");
                            for m in Kind::ALL.into_iter().filter(|m| *m != made) {
                                ui.selectable_value(&mut v.made_as, Some(m), m.name()).on_hover_text(making(m));
                            }
                        });
                        mark(&format!("uiVarietyMade_{vid}"), &r.response);
                    });
                    hint(ui, "Its own instructions, with the category's (one with the same keyword replaces the category's)");
                    features_editor(ui, &mut k.varieties[vi].features, &format!("uiVarietyFeature_{vid}_"));
                    ui.add_space(4.0);
                    hint(ui, "Its own settings, with the category's (asked of every variety already)");
                    let v_made = k.varieties[vi].making(made);
                    let held: Vec<Param> = base.iter().copied().chain(Param::ALL.into_iter().filter(|p| p.needed(v_made))).collect();
                    if !params_toggles(ui, &mut k.varieties[vi].params, v_made, &held, &format!("uiVarietyParam_{vid}_")) {
                        hint(ui, "none left: every setting the kit can ask is the category's already");
                    }
                });
            }
            if let Some((vid, name)) = named {
                let new_id = k.name_variety(&vid, &name);
                st.renaming = None;
                st.variety = Some(new_id);
            }
            if let Some(vi) = gone {
                k.varieties.remove(vi);
                st.variety = None;
            }
            ui.add_space(6.0);
            let r = ui.button("Done");
            mark("uiKindDone", &r);
            done = r.clicked();
        });
    });
    if remove {
        cat.remove_kind(&id);
        (st.tab, st.open, st.variety, st.renaming) = (None, false, None, None);
        return true;
    }
    if !open || done {
        st.open = false;
        st.renaming = None;
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

    /// The standard categories: character (itself and a creature its varieties), prop,
    /// environment (itself, grass, a flower, a rock and a tree, each made its own way), floor,
    /// each asking what the window always asked, with the instructions written so far; a
    /// request's settings (its category's with its variety's own), instructions and label
    /// from its category and its variety (one of its instructions replacing the category's of
    /// the same keyword, the instructions for every category first); a variety added, named,
    /// taken out; a category of the user's own starting as its making's, taken out again (the
    /// kit's own never are); what older files leave out or name otherwise completed.
    /// A catalogue as the standard one was first written (the standard file is the user's to
    /// teach: the tests below don't lean on what it says now).
    const FIXTURE: &str = r#"{"features": [], "kinds": [
        {"id": "character", "name": "Character", "made_as": "character", "params": ["height", "style", "pose", "shapes", "bones", "check_lines"], "features": [],
         "varieties": [{"id": "character", "name": "Character", "params": [], "features": []}, {"id": "creature", "name": "Creature", "params": [], "features": []}]},
        {"id": "prop", "name": "Prop", "made_as": "prop", "params": ["height", "style", "shapes", "many", "check_lines"], "features": [], "varieties": [{"id": "prop", "name": "Prop", "params": [], "features": []}]},
        {"id": "environment", "name": "Environment", "made_as": "environment", "params": ["height", "style", "shapes", "variants", "many", "check_lines"],
         "features": [{"keyword": "wind", "text": "what the wind moves painted for it"}],
         "varieties": [{"id": "environment", "name": "Environment", "params": [], "features": []},
                       {"id": "grass", "name": "Grass", "made_as": "grass", "params": ["height", "cells", "style", "shapes", "variants", "many", "check_lines"], "features": [{"keyword": "edges", "text": "e"}, {"keyword": "pieces", "text": "p"}, {"keyword": "density", "text": "d"}]},
                       {"id": "flower", "name": "Flower", "made_as": "flower", "params": [], "features": [{"keyword": "stem", "text": "s"}]},
                       {"id": "rock", "name": "Rock", "made_as": "rock", "params": ["cells", "style", "shapes", "variants", "many", "check_lines"], "features": [{"keyword": "detail", "text": "a"}, {"keyword": "colour", "text": "b"}, {"keyword": "moss", "text": "c"}, {"keyword": "plants", "text": "d"}]},
                       {"id": "tree", "name": "Tree", "made_as": "tree", "params": [], "features": [{"keyword": "trunk", "text": "t"}, {"keyword": "foliage", "text": "f"}, {"keyword": "colour", "text": "c"}, {"keyword": "wind", "text": "w"}]}]},
        {"id": "floor", "name": "Floor set", "made_as": "floor", "params": ["style", "shapes"], "features": [], "varieties": [{"id": "floor", "name": "Floor set", "params": [], "features": []}]}]}"#;

    #[test]
    fn the_categories_shape_what_a_request_asks() {
        // the standard file: its shape (what it says is the user's to change)
        let standard = Catalogue::standard();
        assert_eq!(standard.kinds.iter().map(|k| k.id.as_str()).collect::<Vec<_>>()[..4], ["character", "prop", "environment", "floor"]);
        for k in &standard.kinds {
            assert!(k.built_in(), "{}", k.id);
            assert_eq!(k.varieties[0].id, k.id, "{}: its generic variety first", k.id);
        }
        let env = standard.kind("environment").unwrap();
        assert_eq!(env.varieties.iter().map(|v| (v.id.as_str(), v.made_as)).collect::<Vec<_>>()[..5], [("environment", None), ("grass", Some(Kind::Grass)), ("flower", Some(Kind::Flower)), ("rock", Some(Kind::Rock)), ("tree", Some(Kind::Tree))]);
        assert_eq!(standard.kind("character").unwrap().varieties.iter().map(|v| v.id.as_str()).collect::<Vec<_>>()[..2], ["character", "creature"]);
        assert_eq!(standard.kind("floor").unwrap().varieties[0].name, "Floor set");
        // the rest on a catalogue of known words
        let mut c: Catalogue = serde_json::from_str(FIXTURE).unwrap();
        c.complete();
        for k in &c.kinds {
            assert_eq!(k.params, default_params(k.made_as), "{}: what the window always asked", k.id);
        }
        assert_eq!(c.kind("environment").unwrap().variety("tree").unwrap().features.iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["trunk", "foliage", "colour", "wind"]);
        // a rock: the environment's variety, made as a rock, its settings a rock's
        let rock = ModelRequest { kind: Kind::Rock, kind_id: "environment".into(), variety: "rock".into(), ..Default::default() };
        assert_eq!(c.params_of(&rock), [Param::Cells, Param::Style, Param::Shapes, Param::Variants, Param::Many, Param::CheckLines]);
        assert_eq!(c.label(&rock), "rock");
        assert_eq!(c.features_of(&rock).iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["wind", "detail", "colour", "moss", "plants"], "the environment's wind, then the rock's own");
        // a request naming no variety (from before): the generic one
        let plain = ModelRequest { kind: Kind::Environment, ..Default::default() };
        assert_eq!((c.of(&plain).1.map(|v| v.id.as_str()), c.label(&plain)), (Some("environment"), "environment".into()));
        // a variety added, then named (its id following its name while it has nothing of its
        // own), made its own way (a pine, a tree): its settings the category's with its own,
        // its instructions over the category's; the instructions for every category first
        c.features = vec![Feature { keyword: "silhouette".into(), text: "from the side first".into() }, Feature { keyword: "Wind".into(), text: "none".into() }];
        let env = c.kind_mut("environment").unwrap();
        let added = env.add_variety("", None);
        assert_eq!((added.as_str(), env.varieties.last().unwrap().name.as_str()), ("variety", ""));
        let pine = env.name_variety(&added, "Pine");
        assert_eq!((pine.as_str(), env.variety("pine").map(|v| v.name.as_str())), ("pine", Some("Pine")));
        let v = env.varieties.iter_mut().find(|v| v.id == "pine").unwrap();
        v.made_as = Some(Kind::Tree);
        v.params = vec![Param::Bones];
        v.features = vec![Feature { keyword: "cones".into(), text: "brown".into() }];
        assert_eq!(env.name_variety("pine", "Pine tree"), "pine", "named again with something of its own: its id stays");
        let r = ModelRequest { kind: Kind::Tree, kind_id: "environment".into(), variety: "pine".into(), ..Default::default() };
        assert_eq!(c.params_of(&r), default_params(Kind::Tree), "the category's (a tree asks no bones)");
        let f = c.features_of(&r);
        assert_eq!(f.iter().map(|f| f.keyword.as_str()).collect::<Vec<_>>(), ["silhouette", "wind", "cones"], "the environment's wind in the general one's place, the pine's cones after");
        assert_ne!(f[1].text, "none");
        assert_eq!(c.label(&r), "pine tree");
        // a category of the user's own: made as environment, starting as its settings and
        // instructions, a generic variety of its own
        let bush = c.add_kind("Bush", Kind::Environment);
        assert_eq!(bush, "bush");
        let b = ModelRequest { kind: Kind::Environment, kind_id: "bush".into(), variety: "environment".into(), ..Default::default() };
        assert_eq!((c.label(&b), c.params_of(&b)), ("environment".to_string(), default_params(Kind::Environment)));
        assert_eq!(c.features_of(&b).len(), c.features.len() + c.kind("environment").unwrap().features.len() - 1, "the general wind replaced by the environment's");
        let s = c.summary();
        assert!(s.starts_with("*:silhouette,Wind | character:[character:;creature:] | ") && s.contains("bush<environment>:wind[environment:]") && s.contains("environment:wind[environment:;grass:edges,pieces,density;flower:") && s.contains(";pine:cones]"), "{s}");
        // the kit needs a grass's cells, whatever its category says; a creature can't be strewn
        c.kind_mut("environment").unwrap().params = vec![];
        c.kind_mut("environment").unwrap().varieties.iter_mut().find(|v| v.id == "grass").unwrap().params = vec![];
        assert_eq!(c.params_of(&ModelRequest { kind: Kind::Grass, kind_id: "environment".into(), variety: "grass".into(), ..Default::default() }), [Param::Height, Param::Cells]);
        assert!(!Param::Many.fits(Kind::Character) && Param::Bones.fits(Kind::Character) && !Param::Bones.fits(Kind::Prop));
        // taken out: a category of the user's own (its requests then their making's), never
        // the kit's own
        assert!(c.remove_kind("bush") && !c.remove_kind("environment") && c.kind("environment").is_some());
        assert_eq!(c.label(&b), "environment", "its category gone: the category it's made as");
        assert_eq!(Catalogue::add_kind(&mut c, "Tree", Kind::Tree), "tree_2", "never a kit category's id");
        // a file from before: a creature category (the character category now, a creature its
        // variety), grass as a category of its own (kept, a generic variety given), the rest
        // completed as the standard has it
        let mut old: Catalogue = serde_json::from_str(r#"{"kinds": [{"id": "creature", "name": "Creature", "made_as": "creature", "params": ["height"], "features": [{"keyword": "fur", "text": "soft"}]}, {"id": "grass", "name": "Grass", "made_as": "grass", "params": ["cells"]}]}"#).unwrap();
        old.complete();
        assert_eq!(old.kinds.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["character", "grass", "prop", "environment", "floor"]);
        let ch = old.kind("character").unwrap();
        assert!(ch.made_as == Kind::Character && ch.name == "Character" && ch.features.len() == 1, "{ch:?}");
        assert_eq!(ch.varieties.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["character", "creature"], "its generic variety first, the creature kept");
        assert_eq!(old.kind("grass").unwrap().varieties.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["grass"]);
        assert_eq!(old.kind("environment").unwrap().varieties.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["environment", "flower", "rock", "tree"], "grass is a category of its own there");
    }

    /// Kept as written: the standard file reads back the same, and the editor writes it in its
    /// own layout (so a save changes only what was changed); a project's own copy read first.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_categories_are_kept_where_they_were_read() {
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
