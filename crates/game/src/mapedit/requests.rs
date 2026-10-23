//! Model requests: pictures of a creature or a prop dropped on the Requests window, for a
//! Claude session to model in Blender when asked to "make the waiting requests" (the recipe
//! is `tools/blender/requests.md`). A request is `private/requests/<id>/request.json` with
//! its pictures in the same folder; once done, renders of the model and the prop it became.
//! private/ is never committed: the pictures are often other artists' work, and the model of
//! a design that isn't the user's own (or licensed) goes to temporary/, never content/.
//! On the desktop the runs are Claude Code's (run_claude.py); in the browser the folder is kept
//! in the browser (`disk`), and the runs go in a sandbox of Anthropic's on the user's own Claude
//! API key (`cloud`): the same window, the same recipe.

use std::collections::BTreeMap;

mod cloud;
mod disk;
mod kinds;

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
pub(super) use cloud::may_write;

use serde::{Deserialize, Serialize};

/// The parts the user draws bones for in the Bones window, each with its colour (the same
/// in tools/blender/request_lines.py, which draws them over the pictures for Claude). "misc"
/// is any piece that has no name here: the user gives each misc line a name of their own.
pub(super) const BONE_PARTS: [(&str, [u8; 3]); 9] = [
    ("leg", [255, 138, 0]),
    ("arm", [0, 229, 255]),
    ("torso", [255, 230, 0]),
    ("head", [255, 61, 240]),
    ("ear", [125, 255, 58]),
    ("horn", [255, 77, 106]),
    ("tail", [77, 139, 255]),
    ("wing", [183, 123, 255]),
    ("misc", [225, 225, 225]),
];

/// A part's colour (a light grey for one not in the list).
pub(super) fn bone_colour(part: &str) -> [u8; 3] {
    BONE_PARTS.iter().find(|(p, _)| *p == part).map(|(_, c)| *c).unwrap_or([225, 225, 225])
}

/// A line of bones the user has drawn over a picture: which part it is, and its joints in the
/// picture's pixels (x right, y down), the root first (a hip, a shoulder, a tail's base);
/// each pair of joints is a bone. A misc line carries the name the user gave the piece.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Bone {
    pub part: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub points: Vec<[i32; 2]>,
}

/// A drawing the user has outlined on a picture (Process multiple): only the drawings outlined
/// are made, each a request of its own, as tall and on as many cells as the user says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Outline {
    /// its line round the drawing, closed, in the picture's pixels (x right, y down)
    pub points: Vec<[i32; 2]>,
    /// how tall it stands, in cells
    pub height: f64,
    /// the cells it covers (grass: fills), across x and z
    pub cells: [u32; 2],
}

impl Default for Outline {
    fn default() -> Self {
        Outline { points: vec![], height: 1.0, cells: [1, 1] }
    }
}

/// The outlines' colours, one after another.
const OUTLINE_COLOURS: [[u8; 3]; 8] = [[255, 196, 64], [90, 200, 255], [255, 110, 160], [140, 230, 120], [200, 150, 255], [255, 150, 90], [120, 220, 210], [240, 240, 120]];

/// The colour of outline `k` (0 on).
pub(super) fn outline_colour(k: usize) -> [u8; 3] {
    OUTLINE_COLOURS[k % OUTLINE_COLOURS.len()]
}

/// What each of a picture's bone lines is called: its name when it has one, else its part,
/// numbered when the picture has several of that part ("leg 1", "leg 2", "tail", "spike").
pub(super) fn bone_names(bones: &[Bone]) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for b in bones.iter().filter(|b| b.name.trim().is_empty()) {
        *counts.entry(b.part.as_str()).or_default() += 1;
    }
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    bones
        .iter()
        .map(|b| {
            if !b.name.trim().is_empty() {
                return b.name.trim().to_string();
            }
            let n = seen.entry(b.part.as_str()).or_default();
            *n += 1;
            if counts.get(b.part.as_str()).copied().unwrap_or(0) > 1 {
                format!("{} {n}", b.part)
            } else {
                b.part.clone()
            }
        })
        .collect()
}

/// A picture by its file name: PNG, JPG or WebP.
pub(super) fn is_picture(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".webp"].iter().any(|e| lower.ends_with(e))
}

/// A file hovering over the window that is a picture (its path on the desktop, its type in
/// the browser).
pub(super) fn hovered_picture(h: &egui::HoveredFile) -> bool {
    h.path.as_ref().map(|p| is_picture(&p.to_string_lossy())).unwrap_or(false) || h.mime.starts_with("image/")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    /// rigged, with an idle animation: a character of the story, or a creature of the fields
    /// (the window's creature variety: its prop's category creature, a creature record of its
    /// own); "creature" is what requests from before called it
    #[default]
    #[serde(alias = "creature")]
    Character,
    Prop,
    /// a patch of grass filling its cells to their edges (`cells`), so copies side by side join
    /// without a gap: a grass prop (one tall ground can grow too)
    Grass,
    /// a flower made finely: its stem, leaves, petals (curved, in rings) and heart (a disc, a
    /// cone, stamens); a meadow can grow it
    Flower,
    /// a rock made finely (its facets, cracks, colours, moss, what grows on it), on about its
    /// cells (`cells`: its footprint), as tall as its pictures' proportions make it there
    Rock,
    /// a tree: its trunk and branches as meshes, its foliage as clusters of leaves, each a few
    /// curved planes (cards) crossing, painted for the wind; tree areas grow it
    Tree,
    /// what dresses the outdoors: rocks, bushes, flowers, logs, trees
    Environment,
    /// a set of floor tiles, each a cell, laid to fit by wave function collapse (`world::floors`)
    Floor,
}

impl Kind {
    /// Its name in the window.
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Character => "character",
            Kind::Prop => "prop",
            Kind::Grass => "grass",
            Kind::Flower => "flower",
            Kind::Rock => "rock",
            Kind::Tree => "tree",
            Kind::Environment => "environment",
            Kind::Floor => "floor set",
        }
    }

    /// It can be drawn by the thousand (strewn over fields with the grass and the flowers): not
    /// a creature, not a set of floor tiles.
    pub(super) fn strewn(self) -> bool {
        !matches!(self, Kind::Character | Kind::Floor)
    }

    /// The kit's eight: how a request is made (the window's kinds and varieties are each made
    /// as one of them).
    pub(super) const ALL: [Kind; 8] = [Kind::Character, Kind::Prop, Kind::Grass, Kind::Flower, Kind::Rock, Kind::Tree, Kind::Environment, Kind::Floor];

    /// Its id, as the files have it ("floor" for a floor set).
    pub(super) fn id(self) -> &'static str {
        match self {
            Kind::Character => "character",
            Kind::Prop => "prop",
            Kind::Grass => "grass",
            Kind::Flower => "flower",
            Kind::Rock => "rock",
            Kind::Tree => "tree",
            Kind::Environment => "environment",
            Kind::Floor => "floor",
        }
    }

    /// What it is, in a few words (its tab's and its button's tip).
    pub(super) fn about(self) -> &'static str {
        match self {
            Kind::Character => "Rigged: a spine, a head, limbs and a tail, and an idle animation. A character of the story, or a creature of the fields (its creature variety: a creature record of its own, its model standing for it on the map)",
            Kind::Prop => "Anything put on a map: a barrel, a lamp, a sign",
            Kind::Grass => "A patch of grass filling its cells to their edges, so copies side by side join without a gap: a grass prop (tall ground can grow it too)",
            Kind::Flower => "A flower made finely: its stem, its leaves, its petals (curved, cupped, in rings) and its heart (a disc, a dome or a cone; stamens, long thin filaments tipped with pollen, where it has them). A meadow can grow it",
            Kind::Rock => "A rock made finely: its facets, cracks and strata, its colours, moss, grass and flowers on it. Asked for by its footprint: Claude works out its height from the pictures' proportions",
            Kind::Tree => "A tree: its trunk and branches as meshes, its foliage as clusters of leaves, each a few curved planes (cards) crossing, their edges cut to the leaves' silhouette, painted for the wind (the trunk rooted, the clusters swaying and fluttering). Tree areas grow it",
            Kind::Environment => "What dresses the outdoors: rocks, bushes, flowers, logs, trees. Grass, a flower, a rock and a tree are its varieties, each made its own way",
            Kind::Floor => "A set of floor tiles, each a cell, laid to fit (by wave function collapse): a pavement, old ruins, marble, a wood tiling. Laid in buildings (the Build tool's Flooring) and out of doors (the Ground tool's Floors tab)",
        }
    }
}

/// The most variants a request asks for.
pub(super) const VARIANTS_CAP: u32 = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Style {
    /// rounded, smooth shaded
    #[default]
    Smooth,
    /// few faces, flat shaded
    LowPoly,
    /// as close to the pictures as it can be, whatever it takes (the model details say how)
    Accurate,
}

/// How a creature stands in its model: the pose it's rigged in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Pose {
    /// arms (or front legs) straight out to the sides, legs straight, tail straight back
    #[default]
    TPose,
    /// the pose the pictures show
    AsIs,
}

/// "As many as it needs" stops at about this many shapes.
pub(super) const SHAPES_CAP: u32 = 100;

/// How many shapes (primitives) the model is built from: a range, or as many as the design
/// needs (up to SHAPES_CAP), which has Claude think it through and take its time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Shapes {
    pub min: u32,
    pub max: u32,
    pub unlimited: bool,
}

impl Default for Shapes {
    fn default() -> Self {
        Shapes { min: 8, max: 30, unlimited: false }
    }
}

/// In the list's order: waiting first, failed ones above the done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    #[default]
    Waiting,
    InProgress,
    /// the run ended without the model, stopped with an error, or went silent (its
    /// progress step says why; its log has the rest)
    Failed,
    Done,
}

/// How the modelling goes, as the modeller says it while it works
/// (`tools/blender/model_requests.py` and `request_kit.py` write it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Progress {
    /// what it's doing ("building the model", "rendering the previews"...)
    pub step: String,
    /// how far along its steps, 0 to 1
    pub done: f64,
    /// when the modelling began, in seconds since 1970
    pub started: f64,
    /// how long it should take in all, in minutes; once done, how long it took
    pub minutes: f64,
}

impl Progress {
    /// How full the bar is at `now` (seconds since 1970) and what it says: the modeller's own
    /// count, or the time gone on its estimate when that's further along; never full before
    /// it's done.
    pub(super) fn at(&self, now: f64) -> (f32, String) {
        if self.started <= 0.0 {
            return (self.done.clamp(0.0, 0.97) as f32, if self.step.is_empty() { "starting".into() } else { self.step.clone() });
        }
        let gone = (now - self.started).max(0.0) / 60.0;
        let by_time = if self.minutes > 0.0 { gone / self.minutes } else { 0.0 };
        let left = self.minutes - gone;
        let when = if self.minutes <= 0.0 {
            format!("{} min so far", gone.round())
        } else if left >= 1.5 {
            format!("about {} min left", left.round())
        } else if left > 0.0 {
            "about a minute left".into()
        } else if left > -3.0 {
            "almost done".into()
        } else {
            format!("longer than the {} min expected", self.minutes.round())
        };
        let text = if self.step.is_empty() { when } else { format!("{} · {when}", self.step) };
        (self.done.max(by_time).clamp(0.0, 0.97) as f32, text)
    }
}

/// A request for a model, as `request.json` holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ModelRequest {
    /// its folder's name: a slug of its name (following the name while it waits)
    pub id: String,
    pub name: String,
    /// how the kit makes it (one of its eight)
    pub kind: Kind,
    /// the window's kind it was asked as (a kind the user added names its own, made as `kind`;
    /// empty: `kind` itself), and its variety of that kind (empty: none): `kinds`
    pub kind_id: String,
    pub variety: String,
    /// how tall it stands, in cells
    pub height: f64,
    /// grass: the cells it fills to their edges, across x and z
    pub cells: [u32; 2],
    pub style: Style,
    /// with the accurate style: how to build it, what matters most (the fur, a texture, a
    /// detail to get exactly right)
    pub details: String,
    /// a creature's pose in its model
    pub pose: Pose,
    /// how many shapes it's built from
    pub shapes: Shapes,
    /// what to know: colours, what the pictures don't show, what matters most
    pub notes: String,
    /// how many variants of it to make besides it, each a prop of its own (with several of
    /// them, of each drawing): grass and environment from the first run, any made one by
    /// "Make variants"
    pub variants: u32,
    /// how the variants differ from it (its size, its colours...): "" leaves it to Claude
    pub vary: String,
    /// the design is the user's own, or licensed to them: its model goes to content/ (the
    /// public game); else to temporary/ (never committed)
    pub mine: bool,
    /// drawn with the toon look (its prop's `toon`): the game's style, always (kept for records
    /// from before the window's toggle went; the kit adds every prop toon)
    #[serde(skip_serializing)]
    pub toon: bool,
    /// drawn by the thousand (meadows, procedural ground, with grass and flowers): made as light
    /// as it can be
    pub many: bool,
    /// the user checks the lines first: a run stops once the lines and the front and the side
    /// are drawn, the request waiting (LINES_READY) until "The lines look right: go on"
    pub check_lines: bool,
    pub status: Status,
    /// while it's modelled, how it goes (and once done, how long it took)
    pub progress: Progress,
    /// the reference pictures: files in its folder
    pub images: Vec<String>,
    /// what each of its runs cost (dollars, at the API's prices), the first first: they add
    /// up as it's made again
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub costs: Vec<f64>,
    /// the bones the user has drawn over the pictures (the Bones window), by picture: the body
    /// plan as they see it, which parts there are and where they join, for Claude to read the
    /// pictures by and to rig the creature on
    pub bones: BTreeMap<String, Vec<Bone>>,
    /// with Process multiple, the drawings the user has outlined on the pictures (the Outlines
    /// window), by picture: only those are made, each with its own height and cells; none, all
    /// of them
    pub outlines: BTreeMap<String, Vec<Outline>>,
    /// the drawings the modeller made to work from and to check by: its lines over each
    /// picture, the front and the side drawn from them, and once rendered those lines over the model
    /// (files in its folder; tools/blender/request_lines.py)
    pub lines: Vec<String>,
    /// once done, renders of the model (front, side, three-quarter): files in its folder
    pub previews: Vec<String>,
    /// once done, the prop it became
    pub prop: String,
    /// once its variants are made, the props they became
    pub variant_props: Vec<String>,
    /// what the modeller says back: what it guessed, what to look at
    pub reply: String,
    /// what the user says of the model the last run made, for the next run: what to look for,
    /// what to change ("Make it again with these comments": a run from that model)
    pub comments: String,
    /// the comments the runs that made it again were given, oldest first (made: kept so)
    pub earlier: Vec<String>,
    /// the user's rating of the model made: 0 (nothing like it) to 100 (exactly as designed, as
    /// its description says); none: not rated. A kind's best and worst rated teach its next runs
    /// (`best_and_worst`)
    pub rating: Option<u8>,
    /// the pictures hold several drawings of the same kind of asset (trees, bushes, rocks):
    /// Start splits it, each drawing made a request of its own (`children`), and "Make them
    /// all" has those made one after another
    pub several: bool,
    /// the request this one was split from (it's one of its drawings)
    pub parent: String,
    /// the requests it was split into, in the drawings' order
    pub children: Vec<String>,
}

impl Default for ModelRequest {
    fn default() -> Self {
        ModelRequest {
            id: String::new(),
            name: String::new(),
            kind: Kind::Character,
            kind_id: String::new(),
            variety: String::new(),
            height: 1.0,
            cells: [1, 1],
            style: Style::Smooth,
            details: String::new(),
            pose: Pose::TPose,
            shapes: Shapes::default(),
            notes: String::new(),
            variants: 0,
            vary: String::new(),
            mine: false,
            toon: true,
            many: false,
            check_lines: false,
            status: Status::Waiting,
            progress: Progress::default(),
            images: vec![],
            costs: vec![],
            bones: BTreeMap::new(),
            outlines: BTreeMap::new(),
            lines: vec![],
            previews: vec![],
            prop: String::new(),
            variant_props: vec![],
            reply: String::new(),
            comments: String::new(),
            earlier: vec![],
            rating: None,
            several: false,
            parent: String::new(),
            children: vec![],
        }
    }
}

/// A kind's models rated best (at least `BEST`) and worst (at most `WORST`) that the next runs
/// of its kind look at, at most `SHOWN` of each (model_requests.py's).
pub(super) const BEST: u8 = 60;
pub(super) const WORST: u8 = 40;
pub(super) const SHOWN: usize = 3;

/// Of the requests, the models of a kind the user rated best (BEST or more, the best first) and
/// worst (WORST or less, the worst first), at most SHOWN of each, `but` left out: what the next
/// runs of that kind look at (model_requests.py's best_and_worst; a browser's run is given
/// their files, the desktop's runs find them themselves). By rating, then by name.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) fn best_and_worst<'a>(list: &'a [ModelRequest], kind: &str, but: &str) -> (Vec<&'a ModelRequest>, Vec<&'a ModelRequest>) {
    let mut rated: Vec<&ModelRequest> = list.iter().filter(|r| r.kind_key() == kind && r.status == Status::Done && r.rating.is_some() && r.id != but).collect();
    rated.sort_by(|a, b| b.rating.cmp(&a.rating).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())).then(a.id.cmp(&b.id)));
    let best = rated.iter().filter(|r| r.rating >= Some(BEST)).take(SHOWN).copied().collect();
    let worst = rated.iter().rev().filter(|r| r.rating <= Some(WORST)).take(SHOWN).copied().collect();
    (best, worst)
}

/// The step of a request waiting for the user to check its lines (run_claude.py writes it:
/// model_requests.py's LINES_READY).
pub(super) const LINES_READY: &str = "lines ready: check them";

impl ModelRequest {
    /// The window's kind it was asked as (its tab): its kind id, else its kind's.
    pub(super) fn kind_key(&self) -> String {
        if self.kind_id.is_empty() {
            self.kind.id().to_string()
        } else {
            self.kind_id.clone()
        }
    }

    /// What its ratings and lessons go by: a kind of the user's own (its kind id when that's
    /// not one of the kit's), else how the kit makes it (a rock's lessons are the rocks',
    /// whatever tab asked for it).
    #[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
    pub(super) fn lesson_key(&self) -> String {
        if !self.kind_id.is_empty() && !Kind::ALL.iter().any(|k| k.id() == self.kind_id) {
            self.kind_id.clone()
        } else {
            self.kind.id().to_string()
        }
    }

    /// Its lines drawn, waiting for the user's check: the window shows them with "The lines
    /// look right: go on".
    pub(super) fn lines_ready(&self) -> bool {
        self.status == Status::Waiting && self.progress.step == LINES_READY
    }

    /// Its outlines in turn, numbered as the split run numbers them: picture by picture in its
    /// pictures' order, each picture's in the order drawn.
    pub(super) fn outlined(&self) -> Vec<(&str, &Outline)> {
        self.images.iter().flat_map(|f| self.outlines.get(f).into_iter().flatten().map(move |o| (f.as_str(), o))).collect()
    }

    /// Variants of it can be made from its model now: made, a model of its own (not a set of
    /// floor tiles, which varies by its tiles), not being made.
    pub(super) fn can_vary(&self) -> bool {
        self.status == Status::Done && !self.prop.is_empty() && self.kind != Kind::Floor
    }
}

use desk::Desk;

#[derive(Default)]
pub(super) struct Requests {
    pub open: bool,
    /// pictures hovering over the window, on their way here
    pub hovering: bool,
    /// what the status line says, once
    pub said: Option<String>,
    /// the Claude sign-in asked for (a run failed for want of one): the editor opens its window
    pub sign_in: bool,
    desk: Desk,
}

impl Requests {
    /// The top bar's "Requests" (`r`, its toggle): its tip, how many wait on its corner and,
    /// while one is being modelled, its bar along the bottom (drawn on the button).
    pub fn decorate(&mut self, ui: &mut egui::Ui, r: &egui::Response) {
        let waiting = self.waiting();
        let mut about = "Pictures of characters, creatures and props for Claude to model in Blender: drop PNG, JPG or WebP files on the window while it's open".to_string();
        if waiting > 0 {
            about += &format!("\n{waiting} waiting");
        }
        if !self.desk.queue.runs.is_empty() {
            about += &format!("\n{} queued{}", self.desk.queue.runs.len(), if self.desk.queue.paused { " (the queue paused)" } else { "" });
        }
        let working = self.desk.working();
        if let Some((name, _, text)) = &working {
            about += &format!("\n{name}: {text}");
        }
        for r in self.desk.list.iter().filter(|r| r.lines_ready()) {
            about += &format!("\n{}: {LINES_READY}", r.name);
        }
        about += "\nRight-click: show where it is";
        let r = r.clone().on_hover_text(about);
        let p = ui.painter();
        if waiting > 0 {
            let at = r.rect.right_top() + egui::vec2(-1.0, 2.0);
            p.circle_filled(at, 6.5, egui::Color32::from_rgb(255, 200, 90));
            p.text(at, egui::Align2::CENTER_CENTER, waiting.min(99).to_string(), egui::FontId::proportional(9.5), egui::Color32::BLACK);
        }
        if let Some((_, fraction, _)) = working {
            let line = egui::Rect::from_min_size(r.rect.left_bottom() + egui::vec2(3.0, -2.0), egui::vec2(r.rect.width() - 6.0, 2.0));
            p.rect_filled(line, 1.0, egui::Color32::from_gray(60));
            p.rect_filled(egui::Rect::from_min_size(line.min, egui::vec2(line.width() * fraction, 2.0)), 1.0, egui::Color32::from_rgb(110, 170, 255));
            // kept moving by the clock
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

/// The requests in their folder, the window, the pictures' thumbnails.
mod desk {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::SystemTime;

    use content::{Content, Space};
    use egui::{Color32, RichText, Sense};

    use super::super::assets::{slug, title};
    use super::super::{badge, hint, mark, section, warn};
    use super::disk;
    use super::kinds::{KindDef, Param};
    use super::{bone_colour, bone_names, outline_colour, Bone, Kind, ModelRequest, Outline, Pose, Progress, Requests, Status, Style, BONE_PARTS, LINES_READY, SHAPES_CAP, VARIANTS_CAP};
    use crate::platform::{self, Dropped};

    const WAITING: Color32 = Color32::from_rgb(255, 200, 90);
    const WORKING: Color32 = Color32::from_rgb(110, 170, 255);
    const DONE: Color32 = Color32::from_rgb(110, 210, 130);
    const FAILED: Color32 = Color32::from_rgb(235, 95, 85);
    /// a request in progress with nothing written in its folder for this long (minutes) has
    /// failed: run_claude.py stops a silent run at 30, this catches one whose run is gone
    pub(in super::super) const SILENT: f64 = 31.0;
    /// the models a run can be given (the empty one: Claude Code's own default)
    const MODELS: [(&str, &str); 4] = [("", "default model"), ("opus", "opus"), ("sonnet", "sonnet"), ("haiku", "haiku")];
    /// the console shows this many of the log's last lines (Copy takes them all)
    const CONSOLE_LINES: usize = 600;
    /// Who makes the requests: Claude Code on the desktop; in the browser, Claude in a sandbox of
    /// Anthropic's, on the user's own API key.
    const MAKER: &str = if cfg!(target_arch = "wasm32") { "Claude" } else { "Claude Code" };

    /// Seconds since 1970, as the modeller writes them.
    pub(in super::super) fn now_secs() -> f64 {
        disk::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }

    // ------------------------------------------------------------ on disk

    /// Where the requests are, under the project.
    pub(in super::super) fn folder(root: &Path) -> PathBuf {
        root.join("private").join("requests")
    }

    pub(super) fn record_path(root: &Path, id: &str) -> PathBuf {
        folder(root).join(id).join("request.json")
    }

    pub(in super::super) fn read(root: &Path, id: &str) -> Result<ModelRequest, String> {
        let path = record_path(root, id);
        let text = disk::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut r: ModelRequest = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        // the folder is the request's id, whatever the file says
        r.id = id.to_string();
        // from before the window's character kind: a creature of the characters; one of the
        // kit's grass, flower, rock or tree from before they were the environment's varieties
        if r.kind == Kind::Character && r.variety.is_empty() && text.contains("\"kind\": \"creature\"") {
            r.variety = "creature".into();
        }
        if matches!(r.kind, Kind::Grass | Kind::Flower | Kind::Rock | Kind::Tree) && r.kind_id.is_empty() {
            r.kind_id = "environment".into();
        }
        // a request is one of a variety: the generic one, named after its making, when none
        if r.variety.is_empty() {
            r.variety = r.kind.id().into();
        }
        Ok(r)
    }

    /// Writes its request.json, laid out as the project's records are.
    pub(in super::super) fn save(root: &Path, r: &ModelRequest) -> Result<(), String> {
        let path = record_path(root, &r.id);
        if let Some(dir) = path.parent() {
            disk::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        disk::write(&path, content::to_text(r)).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every request (waiting first, then in progress, then done; by name), and what
    /// couldn't be read.
    pub(in super::super) fn load_all(root: &Path) -> (Vec<ModelRequest>, Vec<String>) {
        let (mut out, mut problems) = (vec![], vec![]);
        let Ok(list) = disk::read_dir(&folder(root)) else { return (out, problems) };
        for e in list {
            let id = e.name.clone();
            if !disk::is_file(&e.path.join("request.json")) {
                continue;
            }
            match read(root, &id) {
                Ok(r) => out.push(r),
                Err(err) => problems.push(err),
            }
        }
        out.sort_by(|a, b| (a.status, a.name.to_lowercase(), &a.id).cmp(&(b.status, b.name.to_lowercase(), &b.id)));
        problems.sort();
        (out, problems)
    }

    /// An id for a new request (or a renamed one): the slug, numbered if it's taken.
    fn free_id(root: &Path, name: &str) -> String {
        let base = match slug(name) {
            s if s.is_empty() => "request".to_string(),
            s => s,
        };
        let mut id = base.clone();
        let mut n = 2;
        while disk::exists(&folder(root).join(&id)) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }

    /// A new request, named after a picture's file ("moss_toad-front.png": "Moss toad front"),
    /// written with no pictures yet.
    pub(in super::super) fn create(root: &Path, picture: &str) -> Result<ModelRequest, String> {
        let stem = picture.rsplit_once('.').map(|(s, _)| s).unwrap_or(picture);
        let name = title(stem);
        // (one of a variety always: its making's generic one, until a category is picked)
        let kind = Kind::default();
        let r = ModelRequest { id: free_id(root, &name), name, variety: kind.id().into(), ..Default::default() };
        save(root, &r)?;
        Ok(r)
    }

    /// What a request's settings no longer ask (its kind's or its variety's params: `params`),
    /// set back to what a new request has (a drawing's cells, from its outline, kept).
    pub(in super::super) fn unasked(r: &mut ModelRequest, params: &[Param]) {
        let fresh = ModelRequest::default();
        if !params.contains(&Param::Pose) {
            r.pose = fresh.pose;
        }
        if !params.contains(&Param::Cells) && r.parent.is_empty() {
            r.cells = fresh.cells;
        }
        if !params.contains(&Param::Style) {
            r.style = fresh.style;
        }
        if !params.contains(&Param::Shapes) {
            r.shapes = fresh.shapes;
        }
        if !params.contains(&Param::Many) {
            r.many = false;
        }
        if !params.contains(&Param::CheckLines) {
            r.check_lines = false;
        }
        if !params.contains(&Param::Variants) && !r.can_vary() {
            (r.variants, r.vary) = (0, String::new());
        }
    }

    /// The first letter made a capital ("grass": "Grass"), for a mark's name.
    fn capital(word: &str) -> String {
        let mut c = word.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }

    /// The kind of picture the bytes are, as a file extension.
    fn picture_kind(bytes: &[u8]) -> Option<&'static str> {
        match image::guess_format(bytes).ok()? {
            image::ImageFormat::Png => Some("png"),
            image::ImageFormat::Jpeg => Some("jpg"),
            image::ImageFormat::WebP => Some("webp"),
            _ => None,
        }
    }

    #[derive(Debug, PartialEq)]
    pub(in super::super) enum Added {
        /// copied in, under this name
        New(String),
        /// the same picture was there already
        Already(String),
    }

    /// A picture as it's kept: PNG and JPG as they come, WebP made PNG (Blender 3.1 can't read
    /// WebP). Its bytes and its extension.
    fn kept(name: &str, bytes: &[u8]) -> Result<(Vec<u8>, &'static str), String> {
        match picture_kind(bytes) {
            Some("webp") => {
                let img = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).map_err(|e| format!("{name}: {e}"))?;
                let mut png = std::io::Cursor::new(vec![]);
                img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| format!("{name}: {e}"))?;
                Ok((png.into_inner(), "png"))
            }
            Some(ext) => Ok((bytes.to_vec(), ext)),
            None => Err(format!("{name}: not a PNG, JPG or WebP picture")),
        }
    }

    /// A picture copied into the request's folder (its name made safe, numbered when another
    /// picture has it) and listed; the same picture twice is kept once.
    pub(in super::super) fn add_picture(root: &Path, r: &mut ModelRequest, name: &str, bytes: &[u8]) -> Result<Added, String> {
        let (bytes, ext) = kept(name, bytes)?;
        let bytes = bytes.as_slice();
        let dir = folder(root).join(&r.id);
        disk::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        let base = match slug(stem) {
            s if s.is_empty() => "picture".to_string(),
            s => s,
        };
        let mut file = format!("{base}.{ext}");
        let mut n = 2;
        loop {
            let path = dir.join(&file);
            if !disk::exists(&path) {
                break;
            }
            if disk::read(&path).map(|b| b == bytes).unwrap_or(false) {
                if !r.images.contains(&file) {
                    r.images.push(file.clone());
                }
                return Ok(Added::Already(file));
            }
            file = format!("{base}_{n}.{ext}");
            n += 1;
        }
        let path = dir.join(&file);
        disk::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        r.images.push(file.clone());
        Ok(Added::New(file))
    }

    /// A picture taken out of the request: its copy in the folder goes too, the lines drawn
    /// over it, and the bones the user has drawn on it.
    pub(in super::super) fn remove_picture(root: &Path, r: &mut ModelRequest, file: &str) -> Result<(), String> {
        r.images.retain(|f| f != file);
        r.bones.remove(file);
        let dir = folder(root).join(&r.id);
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        let drawn = [format!("bones_{stem}.png"), format!("lines_{stem}.png"), format!("match_{stem}.png"), format!("seen_{stem}.png")];
        r.lines.retain(|f| !drawn.contains(f));
        for f in &drawn {
            disk::remove_file(dir.join(f)).ok();
        }
        let path = dir.join(file);
        match disk::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// While it waits, the request's id (its folder) follows its name; once it's being
    /// modelled or done it keeps the one it has. The new id, if it changed.
    pub(in super::super) fn follow_name(root: &Path, r: &mut ModelRequest) -> Result<Option<String>, String> {
        let wanted = slug(&r.name);
        if r.status != Status::Waiting || wanted.is_empty() || wanted == r.id {
            return Ok(None);
        }
        let id = free_id(root, &r.name);
        let (from, to) = (folder(root).join(&r.id), folder(root).join(&id));
        disk::rename(&from, &to).map_err(|e| format!("{} to {}: {e}", from.display(), to.display()))?;
        r.id = id.clone();
        save(root, r)?;
        Ok(Some(id))
    }

    /// The request and its folder gone (the prop it became stays in the project).
    pub(in super::super) fn delete(root: &Path, id: &str) -> Result<(), String> {
        let dir = folder(root).join(id);
        disk::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
    }

    /// The fields changed here (from `before` to `after`) put on what's on disk now, so a
    /// change made elsewhere meanwhile (Claude marking it done) isn't lost.
    pub(in super::super) fn merge(before: &ModelRequest, after: &ModelRequest, disk: &ModelRequest) -> ModelRequest {
        let (b, a) = (serde_json::to_value(before).unwrap_or_default(), serde_json::to_value(after).unwrap_or_default());
        let mut out = serde_json::to_value(disk).unwrap_or_default();
        if let (Some(b), Some(a), Some(o)) = (b.as_object(), a.as_object(), out.as_object_mut()) {
            for (k, v) in a {
                if b.get(k) != Some(v) {
                    o.insert(k.clone(), v.clone());
                }
            }
        }
        serde_json::from_value(out).unwrap_or_else(|_| after.clone())
    }

    /// Every file under the requests, with when it last changed.
    pub(in super::super) fn snapshot(root: &Path) -> Vec<(PathBuf, SystemTime)> {
        let mut out = vec![];
        let Ok(list) = disk::read_dir(&folder(root)) else { return out };
        for e in list {
            let Ok(files) = disk::read_dir(&e.path) else { continue };
            for f in files {
                if let Some(t) = disk::modified(&f.path) {
                    out.push((f.path, t));
                }
            }
        }
        out.sort();
        out
    }

    // ------------------------------------------------------------ runs: Claude Code, through Python

    /// Python: PYTHON names it, else `python` (or `python3`) on the PATH, never the Store's
    /// stand-ins in WindowsApps, else Windows' `py`.
    #[cfg(not(target_arch = "wasm32"))]
    fn find_python() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("PYTHON") {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
        let path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
        for dir in path.iter().filter(|d| !d.to_string_lossy().contains("WindowsApps")) {
            for name in ["python.exe", "python3.exe", "python", "python3"] {
                if dir.join(name).is_file() {
                    return Some(dir.join(name));
                }
            }
        }
        path.iter().map(|d| d.join("py.exe")).find(|p| p.is_file())
    }

    /// One of the requests' Python tools started in the project's folder, on its own (no
    /// console window; it outlives the editor).
    #[cfg(not(target_arch = "wasm32"))]
    fn python(root: &Path, args: &[&str]) -> Result<std::process::Child, String> {
        let python = find_python().ok_or("Python not found: set PYTHON to python.exe")?;
        let mut c = std::process::Command::new(&python);
        c.args(args).current_dir(root).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c.spawn().map_err(|e| format!("{}: {e}", python.display()))
    }

    /// A short one, waited for: true when it worked (in the browser, none: the window sets the
    /// status itself).
    fn python_now(root: &Path, args: &[&str]) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            python(root, args).and_then(|mut c| c.wait().map_err(|e| e.to_string())).map(|s| s.success()).unwrap_or(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (root, args);
            false
        }
    }

    /// The newest time anything in a request's folder changed (its run writes there as it goes).
    fn last_word(seen: &[(PathBuf, SystemTime)], dir: &Path) -> Option<SystemTime> {
        seen.iter().filter(|(p, _)| p.parent() == Some(dir)).map(|(_, t)| *t).max()
    }

    /// In progress with nothing written in its folder for SILENT minutes: failed.
    pub(in super::super) fn gone_silent(r: &ModelRequest, last: Option<SystemTime>, now: SystemTime) -> bool {
        r.status == Status::InProgress && last.and_then(|t| now.duration_since(t).ok()).map(|d| d.as_secs_f64() / 60.0 > SILENT).unwrap_or(false)
    }

    /// What one of a request's drawings (its `lines`) shows, for its tooltip.
    pub(in super::super) fn drawing_about(file: &str) -> String {
        let stem = file.strip_suffix(".png").unwrap_or(file);
        if let Some(p) = stem.strip_prefix("bones_") {
            format!("The bones you've drawn over {p}, with the part of the creature Claude took each for")
        } else if let Some(p) = stem.strip_prefix("lines_") {
            format!("Claude's lines over {p}")
        } else if let Some(p) = stem.strip_prefix("match_") {
            format!("{p}'s lines over the model, seen from where {p} was drawn")
        } else {
            match stem {
                "profiles" => "The front and the side Claude made from the lines, in cells".into(),
                "fit_front" => "The front's lines over the model's front".into(),
                "fit_side" => "The side's lines over the model's side".into(),
                _ => file.to_string(),
            }
        }
    }

    /// How long a log line's time is (`[2026-10-16 09:41:07] `: 22 characters), or 0 when it
    /// has none (a line going on under the one before).
    pub(in super::super) fn stamp_len(line: &str) -> usize {
        let b = line.as_bytes();
        let stamped = b.len() >= 22 && b[0] == b'[' && b[20] == b']' && b[21] == b' ' && b[1..20].iter().all(|c| c.is_ascii_digit() || matches!(c, b'-' | b':' | b' '));
        if stamped {
            22
        } else {
            0
        }
    }

    /// The log as the console shows it: each line's time in a darker grey than its words.
    pub(in super::super) fn console_text(text: &str, folded: bool) -> egui::text::LayoutJob {
        let font = egui::FontId::monospace(11.0);
        let (stamp, words) = (Color32::from_gray(100), Color32::from_gray(if folded { 165 } else { 222 }));
        let mut job = egui::text::LayoutJob::default();
        for line in text.split_inclusive('\n') {
            let cut = stamp_len(line);
            if cut > 0 {
                job.append(&line[..cut], 0.0, egui::TextFormat::simple(font.clone(), stamp));
            }
            job.append(&line[cut..], 0.0, egui::TextFormat::simple(font.clone(), words));
        }
        job
    }

    /// The last `n` lines of a log, and how many came before them.
    pub(in super::super) fn tail(text: &str, n: usize) -> (&str, usize) {
        let total = text.lines().count();
        if total <= n {
            return (text, 0);
        }
        let skip = total - n;
        let at = text.match_indices('\n').nth(skip - 1).map(|(i, _)| i + 1).unwrap_or(0);
        (&text[at..], skip)
    }

    /// Opens a file or a folder the way the system does (a picture in its viewer, a folder
    /// in the file manager).
    #[cfg(not(target_arch = "wasm32"))]
    fn reveal(path: &Path) -> Result<(), String> {
        let program = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program).arg(path).spawn().map(|_| ()).map_err(|e| format!("{program} {}: {e}", path.display()))
    }

    /// The browser: a file downloaded (it opens no program), a folder as a zip of its files.
    #[cfg(target_arch = "wasm32")]
    fn reveal(path: &Path) -> Result<(), String> {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "request".into());
        if disk::is_file(path) {
            let bytes = disk::read(path).map_err(|e| e.to_string())?;
            let kind = match name.rsplit_once('.').map(|(_, e)| e) {
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("txt") | Some("py") | Some("json") => "text/plain",
                _ => "application/octet-stream",
            };
            return platform::download_bytes(&name, &bytes, kind);
        }
        let mut files = std::collections::BTreeMap::new();
        for e in disk::read_dir(path).map_err(|e| e.to_string())? {
            if let Ok(bytes) = disk::read(&e.path) {
                files.insert(format!("{name}/{}", e.name), crate::keep::Kept::File(Arc::new(bytes)));
            }
        }
        platform::download_bytes(&format!("{name}.zip"), &crate::keep::zip(&files), "application/zip")
    }

    // ------------------------------------------------------------ thumbnails

    /// A picture made small for the window: at most `max` pixels on its longer side (never
    /// enlarged), and the picture's own size in pixels.
    pub(in super::super) fn thumbnail(path: &Path, max: u32) -> Result<(egui::ColorImage, egui::Vec2), String> {
        let bytes = disk::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let img = image::load_from_memory(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let full = egui::vec2(img.width() as f32, img.height() as f32);
        let small = if img.width().max(img.height()) > max { img.thumbnail(max, max) } else { img }.to_rgba8();
        let (w, h) = small.dimensions();
        Ok((egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], small.as_raw()), full))
    }

    /// A picture read on the thread: which file at which size, which time it was asked for,
    /// what came of it.
    type Read = (PathBuf, u32, u64, Result<(egui::ColorImage, egui::Vec2), String>);

    /// The pictures' textures, read by a thread of their own so the window never waits: the
    /// small ones the lists show, and a picture large for the Bones window.
    #[derive(Default)]
    pub(in super::super) struct Thumbs {
        made: HashMap<(PathBuf, u32), Result<(egui::TextureHandle, egui::Vec2), String>>,
        /// on their way, with the number of the asking (a file forgotten and asked again
        /// takes only its latest reading)
        asked: HashMap<(PathBuf, u32), u64>,
        asks: u64,
        /// the thread's (the desktop's: the browser reads them as they're asked for)
        #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
        jobs: Option<mpsc::Sender<(PathBuf, u32, u64)>>,
        done: Arc<Mutex<Vec<Read>>>,
    }

    /// The lists' pictures are at most this many pixels across; the Bones window's this many.
    const SMALL: u32 = 320;
    const LARGE: u32 = 1024;

    impl Thumbs {
        /// A picture's small texture and the picture's own size in pixels, once read (None
        /// while on its way).
        pub(in super::super) fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<Result<(egui::TextureId, egui::Vec2), String>> {
            self.at(ctx, path, SMALL)
        }

        /// A picture's texture at most `max` pixels across, and the picture's own size.
        pub(in super::super) fn at(&mut self, ctx: &egui::Context, path: &Path, max: u32) -> Option<Result<(egui::TextureId, egui::Vec2), String>> {
            let key = (path.to_path_buf(), max);
            match self.made.get(&key) {
                Some(Ok((t, full))) => return Some(Ok((t.id(), *full))),
                Some(Err(e)) => return Some(Err(e.clone())),
                None => {}
            }
            if !self.asked.contains_key(&key) {
                self.asks += 1;
                self.asked.insert(key, self.asks);
                // (the browser has no threads: read now, taken in with the rest)
                #[cfg(target_arch = "wasm32")]
                {
                    let made = thumbnail(path, max);
                    self.done.lock().unwrap().push((path.to_path_buf(), max, self.asks, made));
                    ctx.request_repaint();
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let jobs = self.jobs.get_or_insert_with(|| {
                        let (tx, rx) = mpsc::channel::<(PathBuf, u32, u64)>();
                        let (done, ctx) = (self.done.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            for (path, max, n) in rx {
                                let made = thumbnail(&path, max);
                                done.lock().unwrap().push((path, max, n, made));
                                ctx.request_repaint();
                            }
                        });
                        tx
                    });
                    let _ = jobs.send((path.to_path_buf(), max, self.asks));
                }
            }
            None
        }

        /// The pictures read since last time, made textures.
        pub(in super::super) fn take(&mut self, ctx: &egui::Context) {
            let done = std::mem::take(&mut *self.done.lock().unwrap());
            for (path, max, n, made) in done {
                let key = (path, max);
                if self.asked.get(&key) != Some(&n) {
                    continue; // forgotten meanwhile, or asked again since
                }
                self.asked.remove(&key);
                let made = made.map(|(img, full)| (ctx.load_texture(format!("{}@{max}", key.0.to_string_lossy()), img, egui::TextureOptions::LINEAR), full));
                self.made.insert(key, made);
            }
        }

        /// A file changed or gone: read again next time, at every size.
        pub(in super::super) fn forget(&mut self, path: &Path) {
            self.made.retain(|(p, _), _| p != path);
            self.asked.retain(|(p, _), _| p != path);
        }

        /// The pictures read so far (the tests wait on them).
        #[cfg(test)]
        pub(in super::super) fn ready(&self) -> usize {
            self.made.len()
        }
    }

    // ------------------------------------------------------------ the window's state

    #[derive(Default)]
    pub(in super::super) struct Desk {
        /// the project (None: GAME_ROOT, else the folder the game runs in)
        pub root: Option<PathBuf>,
        pub list: Vec<ModelRequest>,
        problems: Vec<String>,
        /// the request dropped pictures go to (None: a new one)
        pub selected: Option<String>,
        loaded: bool,
        seen: Vec<(PathBuf, SystemTime)>,
        next_look: f64,
        /// pictures dropped, read
        pub dropped: Dropped,
        pub thumbs: Thumbs,
        /// a request renamed: its folder follows once the name is no longer being typed
        renamed: Option<String>,
        /// asking whether to delete the selected request
        deleting: bool,
        /// the browser: a picture clicked, shown full size in a window of its own
        viewing: Option<PathBuf>,
        /// the model the runs ask for ("": Claude Code's default)
        model: String,
        /// the console open at the window's foot
        pub console_open: bool,
        /// the requests' logs as last read, by id, with the file's time
        logs: HashMap<String, (SystemTime, Arc<String>)>,
        /// a run being started (its request and when): until it says it's in progress
        starting: Option<(String, f64)>,
        /// a run being cancelled (its request and when): its stop file waits for the run
        cancelling: Option<(String, f64)>,
        /// silent requests already marked failed (once each)
        failed_silent: std::collections::HashSet<String>,
        /// the tools started, reaped as they end
        children: Vec<std::process::Child>,
        /// what the user says is off in a request's lines, by id, for its next run
        pub lines_notes: HashMap<String, String>,
        /// the runs to make one after another (the queue, "Make them all"), the next first
        pub queue: Queue,
        /// a request split into several by a run from the queue: its own requests go next,
        /// once it's split
        splitting: Option<String>,
        /// the Bones window: the user drawing the body's bones over a picture
        pub bones: Sketch,
        /// the Outlines window: the user outlining the drawings to make (Process multiple)
        pub outlines: Outlining,
        /// the Claude sign-in asked for (a run failed for want of one)
        pub sign_in: bool,
        /// the browser's runs, in the cloud
        #[cfg(target_arch = "wasm32")]
        pub cloud: super::cloud::Runner,
        /// the runs that would have started (the tests start none)
        #[cfg(test)]
        pub launched: Vec<Vec<String>>,
        /// the window's kinds (its tabs), and how they're shown and edited
        pub kinds: super::kinds::Catalogue,
        pub kinds_ui: super::kinds::KindsUi,
        kinds_loaded: bool,
    }

    /// The Bones window's state: which request and picture are drawn on, the part the next
    /// line is, the line being drawn.
    #[derive(Default)]
    pub(in super::super) struct Sketch {
        pub open: bool,
        /// the request it's on (another selected: it starts afresh)
        of: String,
        /// the picture drawn on (None, or one gone: the request's first)
        pub picture: Option<String>,
        /// the part the next line is ("": BONE_PARTS' first)
        pub part: String,
        /// the name the next misc line gets
        pub name: String,
        /// the line being drawn: its joints so far, in the picture's pixels
        pub drawing: Vec<[i32; 2]>,
    }

    /// The Outlines window's state: which request and picture are outlined on, the outline
    /// being drawn.
    #[derive(Default)]
    pub(in super::super) struct Outlining {
        pub open: bool,
        /// the request it's on (another selected: it starts afresh)
        of: String,
        /// the picture outlined on (None, or one gone: the request's first)
        pub picture: Option<String>,
        /// the outline being drawn: its points so far, in the picture's pixels
        pub drawing: Vec<[i32; 2]>,
    }

    /// A run waiting its turn in the queue: the request, how it goes (its whole recipe, on
    /// from its lines, made again) and, going on from its lines, what the user said is off in
    /// them.
    #[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(default)]
    pub(in super::super) struct Queued {
        pub id: String,
        pub how: How,
        #[serde(skip_serializing_if = "String::is_empty")]
        pub note: String,
        /// how the request stood when it was queued (made, to make again from its pictures)
        pub from: Status,
    }

    /// The queue: runs made one after another, each once the last has ended (the first first),
    /// unless it's paused; kept in private/requests/queue.json between runs of the editor.
    #[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(default)]
    pub(in super::super) struct Queue {
        pub runs: Vec<Queued>,
        pub paused: bool,
    }

    impl Queue {
        /// The requests queued, in their order.
        pub(in super::super) fn ids(&self) -> Vec<&str> {
            self.runs.iter().map(|q| q.id.as_str()).collect()
        }

        /// Where a request is in it (0: next).
        pub(in super::super) fn place(&self, id: &str) -> Option<usize> {
            self.runs.iter().position(|q| q.id == id)
        }
    }

    fn queue_path(root: &Path) -> PathBuf {
        folder(root).join("queue.json")
    }

    /// The queue as kept (none, or unreadable: empty).
    pub(in super::super) fn load_queue(root: &Path) -> Queue {
        disk::read_to_string(queue_path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// A small button with an arrow drawn on it, up or down (the fonts have no arrows).
    fn arrow(ui: &mut egui::Ui, up: bool, enabled: bool) -> egui::Response {
        let r = ui.add_enabled(enabled, egui::Button::new(" ").small().min_size(egui::vec2(18.0, 16.0)));
        let (c, s) = (r.rect.center(), if up { -1.0 } else { 1.0 });
        let colour = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
        let tip = c + egui::vec2(0.0, 3.0 * s);
        ui.painter().add(egui::Shape::convex_polygon(vec![c + egui::vec2(-4.0, -2.5 * s), c + egui::vec2(4.0, -2.5 * s), tip], colour, egui::Stroke::NONE));
        r
    }

    /// "1st", "2nd", "3rd", "4th"...
    fn nth(n: usize) -> String {
        let end = match (n % 10, n % 100) {
            (1, k) if k != 11 => "st",
            (2, k) if k != 12 => "nd",
            (3, k) if k != 13 => "rd",
            _ => "th",
        };
        format!("{n}{end}")
    }

    /// Where a point of the canvas (`rect`, showing a picture `size` pixels across and down)
    /// is in the picture's pixels, kept within it.
    pub(in super::super) fn on_picture(rect: egui::Rect, size: egui::Vec2, p: egui::Pos2) -> [i32; 2] {
        let u = ((p.x - rect.left()) / rect.width().max(1.0) * size.x).round();
        let v = ((p.y - rect.top()) / rect.height().max(1.0) * size.y).round();
        [u.clamp(0.0, size.x) as i32, v.clamp(0.0, size.y) as i32]
    }

    /// A joint added to the line being drawn, unless it's where the last one is.
    fn add_joint(line: &mut Vec<[i32; 2]>, at: [i32; 2]) {
        if line.last() != Some(&at) {
            line.push(at);
        }
    }

    /// Outline `k`'s colour (0 on), to paint with.
    fn tint(k: usize) -> Color32 {
        let c = outline_colour(k);
        Color32::from_rgb(c[0], c[1], c[2])
    }

    /// The outlines over the picture on the canvas: each closed, in its colour, numbered (from
    /// `first` + 1) by its highest point; then the one being drawn, its next side following the
    /// pointer (`hover`) and its closing side dashed back to its first point.
    fn draw_outlines(p: &egui::Painter, rect: egui::Rect, size: egui::Vec2, outlines: &[Outline], first: usize, drawing: &[[i32; 2]], hover: Option<egui::Pos2>) {
        let to_screen = |j: &[i32; 2]| rect.min + egui::vec2(j[0] as f32 / size.x.max(1.0) * rect.width(), j[1] as f32 / size.y.max(1.0) * rect.height());
        let halo = Color32::from_rgba_unmultiplied(20, 20, 24, 200);
        let line = |pts: &[egui::Pos2], colour: Color32| {
            for pair in pts.windows(2) {
                p.line_segment([pair[0], pair[1]], egui::Stroke::new(5.0, halo));
            }
            for pair in pts.windows(2) {
                p.line_segment([pair[0], pair[1]], egui::Stroke::new(2.5, colour));
            }
            for at in pts {
                p.circle(*at, 3.0, colour, egui::Stroke::new(1.0, halo));
            }
        };
        for (i, o) in outlines.iter().enumerate() {
            let colour = tint(first + i);
            let mut pts: Vec<egui::Pos2> = o.points.iter().map(to_screen).collect();
            if let Some(start) = pts.first().copied() {
                pts.push(start);
            }
            line(&pts, colour);
            if let Some(top) = pts.iter().min_by(|a, b| a.y.total_cmp(&b.y)) {
                let text = format!("{}", first + i + 1);
                let galley = p.layout_no_wrap(text, egui::FontId::proportional(13.0), colour);
                let at = (*top + egui::vec2(-galley.size().x / 2.0, -galley.size().y - 6.0)).max(rect.min);
                p.rect_filled(egui::Rect::from_min_size(at, galley.size()).expand(2.0), 2.0, halo);
                p.galley(at, galley, colour);
            }
        }
        if !drawing.is_empty() {
            let colour = tint(first + outlines.len());
            let pts: Vec<egui::Pos2> = drawing.iter().map(to_screen).collect();
            line(&pts, colour);
            if let (Some(last), Some(h)) = (pts.last(), hover) {
                let next = h.clamp(rect.min, rect.max);
                p.add(egui::Shape::dashed_line(&[*last, next], egui::Stroke::new(2.0, colour.gamma_multiply(0.75)), 6.0, 4.0));
                p.add(egui::Shape::dashed_line(&[next, pts[0]], egui::Stroke::new(1.5, colour.gamma_multiply(0.45)), 4.0, 5.0));
                p.circle_stroke(next, 3.5, egui::Stroke::new(1.5, colour));
            }
        }
    }

    /// The bone lines over the picture on the canvas: each in its part's colour, a dot per
    /// joint, a ring on its root and its name there; then the line being drawn (the sketch's),
    /// its next bone following the pointer (`hover`).
    fn draw_bones(p: &egui::Painter, rect: egui::Rect, size: egui::Vec2, bones: &[Bone], sketch: &Sketch, hover: Option<egui::Pos2>) {
        let (drawing, part, name) = (&sketch.drawing, sketch.part.as_str(), sketch.name.as_str());
        let to_screen = |j: &[i32; 2]| rect.min + egui::vec2(j[0] as f32 / size.x.max(1.0) * rect.width(), j[1] as f32 / size.y.max(1.0) * rect.height());
        let halo = Color32::from_rgba_unmultiplied(20, 20, 24, 200);
        let line = |pts: &[egui::Pos2], colour: Color32| {
            for pair in pts.windows(2) {
                p.line_segment([pair[0], pair[1]], egui::Stroke::new(5.0, halo));
            }
            for pair in pts.windows(2) {
                p.line_segment([pair[0], pair[1]], egui::Stroke::new(2.5, colour));
            }
            for (i, at) in pts.iter().enumerate() {
                p.circle(*at, 3.5, colour, egui::Stroke::new(1.0, halo));
                if i == 0 {
                    p.circle_stroke(*at, 6.5, egui::Stroke::new(2.0, colour));
                }
            }
        };
        // each name by its root: right of it and up, else down, else left, so none covers another
        let mut taken: Vec<egui::Rect> = vec![];
        let mut label = |at: egui::Pos2, text: &str, colour: Color32| {
            let galley = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(11.0), colour);
            let s = galley.size();
            let tries = [egui::vec2(9.0, -s.y - 4.0), egui::vec2(9.0, 5.0), egui::vec2(-9.0 - s.x, -s.y - 4.0), egui::vec2(-9.0 - s.x, 5.0)];
            let fits = |r: egui::Rect| rect.contains_rect(r) && !taken.iter().any(|t| t.intersects(r));
            let pos = tries.iter().map(|d| at + *d).find(|pos| fits(egui::Rect::from_min_size(*pos, s).expand(2.0))).unwrap_or(at + tries[0]);
            let r = egui::Rect::from_min_size(pos, s).expand(2.0);
            taken.push(r);
            p.rect_filled(r, 2.0, halo);
            p.galley(pos, galley, colour);
        };
        // the lines, then their names over them all: (root, name, colour)
        let mut names = vec![];
        for (i, (b, name)) in bones.iter().zip(bone_names(bones)).enumerate() {
            let c = bone_colour(&b.part);
            let colour = Color32::from_rgb(c[0], c[1], c[2]);
            let pts: Vec<egui::Pos2> = b.points.iter().map(to_screen).collect();
            line(&pts, colour);
            if let Some(root) = pts.first() {
                names.push((*root, format!("{}. {name}", i + 1), colour));
            }
        }
        if !drawing.is_empty() {
            let c = bone_colour(part);
            let colour = Color32::from_rgb(c[0], c[1], c[2]);
            let pts: Vec<egui::Pos2> = drawing.iter().map(to_screen).collect();
            line(&pts, colour);
            if let (Some(last), Some(h)) = (pts.last(), hover) {
                let next = h.clamp(rect.min, rect.max);
                p.add(egui::Shape::dashed_line(&[*last, next], egui::Stroke::new(2.0, colour.gamma_multiply(0.75)), 6.0, 4.0));
                p.circle_stroke(next, 3.5, egui::Stroke::new(1.5, colour));
            }
            let text = if part == "misc" && !name.trim().is_empty() { name.trim().to_string() } else { part.to_string() };
            names.push((pts[0], text, colour));
        }
        for (at, text, colour) in &names {
            label(*at, text, *colour);
        }
    }

    impl Desk {
        pub(in super::super) fn root(&self) -> PathBuf {
            self.root.clone().unwrap_or_else(|| PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into())))
        }

        pub(in super::super) fn reload(&mut self) {
            let root = self.root();
            if !self.loaded {
                self.queue = load_queue(&root);
            }
            (self.list, self.problems) = load_all(&root);
            self.seen = snapshot(&root);
            self.loaded = true;
            if !self.kinds_loaded {
                let (kinds, trouble) = super::kinds::Catalogue::load(&root);
                self.kinds = kinds;
                self.kinds_loaded = true;
                if let Some(t) = trouble {
                    self.problems.push(t);
                }
            }
            if self.selected.as_ref().is_some_and(|id| !self.list.iter().any(|r| &r.id == id)) {
                self.selected = None;
            }
        }

        fn picture_path(&self, id: &str, file: &str) -> PathBuf {
            folder(&self.root()).join(id).join(file)
        }

        /// Whether a request is in the tab shown (all of them on All).
        fn in_tab(&self, r: &ModelRequest) -> bool {
            self.kinds_ui.tab.as_ref().is_none_or(|t| *t == r.kind_key())
        }

        /// A request made of a category of the window's, and a variety of it (the one named,
        /// else the category's generic one): made as the variety's making (else the
        /// category's), asked as the category (named when that isn't its making's own); what
        /// its settings no longer ask set back.
        pub(in super::super) fn set_kind(&self, r: &mut ModelRequest, k: &KindDef, variety: Option<&str>) {
            let v = k.variety_of(variety.unwrap_or(""));
            r.kind = v.map_or(k.made_as, |v| v.making(k.made_as));
            r.kind_id = if k.id == r.kind.id() { String::new() } else { k.id.clone() };
            r.variety = v.map(|v| v.id.clone()).unwrap_or_else(|| r.kind.id().into());
            let params = self.kinds.params_of(r);
            unasked(r, &params);
        }

        /// A request made a variety of its category: what its settings no longer ask set back.
        fn set_variety(&self, r: &mut ModelRequest, variety: &str) {
            let Some(k) = self.kinds.of(r).0.cloned() else { return };
            self.set_kind(r, &k, Some(variety));
        }

        /// The new request page shown (the header's +): what pictures dropped there start,
        /// the tab's kind (else the first) with no variety, to pick from.
        fn open_page(&mut self) {
            self.kinds_ui.page = true;
            self.selected = None;
            self.deleting = false;
            let kind = self.kinds_ui.tab.clone().filter(|t| self.kinds.kind(t).is_some()).or_else(|| self.kinds.kinds.first().map(|k| k.id.clone()));
            self.kinds_ui.next = kind.map(|k| {
                let generic = self.kinds.kind(&k).and_then(|d| d.variety_of("")).map(|v| v.id.clone()).unwrap_or_default();
                (k, generic)
            });
        }

        /// The model {MAKER} runs on for the next Start (and, in a browser, the most a run may
        /// cost): where a run is started from.
        fn model_pick(&mut self, ui: &mut egui::Ui) {
            hint(ui, "Claude:");
            let shown = MODELS.iter().find(|(m, _)| *m == self.model).map(|(_, label)| *label).unwrap_or(self.model.as_str()).to_string();
            let pick = egui::ComboBox::from_id_salt("request-model").selected_text(shown).width(110.0).show_ui(ui, |ui| {
                for (m, label) in MODELS {
                    ui.selectable_value(&mut self.model, m.to_string(), label);
                }
            });
            pick.response.on_hover_text(format!("The model {MAKER} runs on for the next Start (the console says which it got)"));
            #[cfg(target_arch = "wasm32")]
            self.cap(ui);
        }

        /// The new request page, in place of the list and the request (the header's +, or no
        /// request yet): the model, the kind and a variety of it, and the artwork to drop.
        fn new_page(&mut self, ui: &mut egui::Ui, hovering: bool) {
            platform::set_hook("requestpage", "new");
            let (kid, vid) = self.kinds_ui.next.clone().unwrap_or_default();
            platform::set_hook("requestnext", &if vid.is_empty() { kid.clone() } else { format!("{kid}:{vid}") });
            let kinds = self.kinds.clone();
            let picked = kinds.kind(&kid).cloned();
            ui.add_space((ui.available_height() * 0.16).clamp(8.0, 90.0));
            ui.vertical_centered(|ui| {
                let title = if hovering { "Let go: the pictures begin the request" } else { "A new request" };
                ui.label(RichText::new(title).size(20.0).color(if hovering { WAITING } else { Color32::from_gray(210) }));
                ui.add_space(10.0);
                // (a row's widgets sit left: set in from the side by what's left over)
                let inset = |ui: &mut egui::Ui, width: f32| ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                ui.horizontal(|ui| {
                    inset(ui, if cfg!(target_arch = "wasm32") { 330.0 } else { 190.0 });
                    self.model_pick(ui);
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    inset(ui, 80.0 + 92.0 * kinds.kinds.len() as f32);
                    hint(ui, "Category");
                    for k in &kinds.kinds {
                        let on = k.id == kid;
                        let (clicked, edit) = super::kinds::tab(ui, on, &k.name.to_lowercase(), None, &super::kinds::about_kind(k), (&format!("uiRequestKind{}", capital(&k.id)), &format!("uiRequestKind{}Edit", capital(&k.id))), &format!("{}'s instructions, settings and varieties", k.name));
                        if clicked && !on {
                            let generic = k.variety_of("").map(|v| v.id.clone()).unwrap_or_default();
                            self.kinds_ui.next = Some((k.id.clone(), generic));
                        }
                        if edit {
                            (self.kinds_ui.tab, self.kinds_ui.open) = (Some(k.id.clone()), true);
                            self.kinds_ui.variety = (k.id == kid && !vid.is_empty()).then(|| vid.clone());
                        }
                    }
                    let add = ui.small_button("+").on_hover_text("A category of your own: its name, what the kit makes it as, then its settings, instructions and varieties");
                    mark("uiRequestKindAdd", &add);
                    if add.clicked() {
                        self.kinds_ui.adding = Some((String::new(), picked.as_ref().map_or(Kind::Prop, |k| k.made_as)));
                    }
                });
                if let Some(k) = picked.as_ref().filter(|k| !k.varieties.is_empty()) {
                    ui.horizontal_wrapped(|ui| {
                        inset(ui, 80.0 + 72.0 * k.varieties.len() as f32);
                        hint(ui, "variety");
                        for v in &k.varieties {
                            let b = ui.selectable_label(vid == v.id, v.name.to_lowercase()).on_hover_text(match v.made_as {
                                Some(m) => format!("Made as {}: {}", m.name(), super::kinds::making(m)),
                                None => format!("{} of its own instructions, added to the category's", v.features.len()),
                            });
                            mark(&format!("uiRequestVariety_{}", v.id), &b);
                            if b.clicked() {
                                self.kinds_ui.next = Some((kid.clone(), v.id.clone()));
                            }
                        }
                    });
                }
                ui.add_space(14.0);
                let what = match (&picked, picked.as_ref().and_then(|k| k.variety_of(&vid))) {
                    (Some(_), Some(v)) => format!("one {}", v.name.to_lowercase()),
                    (Some(k), None) => format!("one {}", k.name.to_lowercase()),
                    (None, _) => "one character, creature or prop".to_string(),
                };
                ui.label(RichText::new(format!("Drop the artwork of {what} on the window (PNG, JPG, WebP): it begins the request")).size(14.5).color(Color32::from_gray(200)));
                hint(ui, "Front, side, back, close-ups: one request. Then Start it: Claude models it in Blender by itself.");
                if !self.list.is_empty() {
                    ui.add_space(14.0);
                    let back = ui.button("Back to the requests");
                    mark("uiRequestBack", &back);
                    if back.clicked() {
                        self.kinds_ui.page = false;
                        self.kinds_ui.next = None;
                    }
                }
            });
        }

        /// While pictures hover over the window: where they'll go, in a strip over the list.
        fn drop_strip(&self, ui: &mut egui::Ui, hovering: bool) {
            if !hovering {
                return;
            }
            let target = self.selected.as_ref().and_then(|id| self.list.iter().find(|r| &r.id == id)).map(|r| r.name.clone());
            let text = match target {
                Some(name) => format!("Let go: the pictures go to {name}"),
                None => "Let go: the pictures start a new request".to_string(),
            };
            egui::Frame::new().stroke(egui::Stroke::new(1.5, WAITING)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(WAITING));
            });
            ui.add_space(2.0);
        }

        /// The tab row over the list: + for a new request (the page), All, each category, +.
        pub(in super::super) fn kind_tabs(&mut self, ui: &mut egui::Ui) {
            let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for r in &self.list {
                *counts.entry(r.kind_key()).or_default() += 1;
            }
            if super::kinds::tabs(ui, &self.kinds, &mut self.kinds_ui, &counts, self.list.len()) {
                self.open_page();
            }
            platform::set_hook("requesttab", self.kinds_ui.tab.as_deref().unwrap_or("all"));
        }

        /// The kinds' windows (a kind added; a kind's own), the kinds written a moment after a
        /// change: what to say when that went wrong.
        pub(in super::super) fn kind_windows(&mut self, ctx: &egui::Context) -> Option<String> {
            let now = ctx.input(|i| i.time);
            let mut changed = super::kinds::add_window(ctx, &mut self.kinds, &mut self.kinds_ui);
            changed |= super::kinds::kind_window(ctx, &mut self.kinds, &mut self.kinds_ui);
            changed |= super::kinds::general_window(ctx, &mut self.kinds, &mut self.kinds_ui);
            if changed {
                self.kinds_ui.changed = Some(now);
            }
            if self.kinds_ui.changed.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            let root = self.root();
            let kept = self.kinds_ui.keep(&mut self.kinds, &root, now);
            platform::set_hook("requestkinds", &self.kinds.summary());
            platform::set_hook("requestkind", if self.kinds_ui.open { self.kinds_ui.tab.as_deref().unwrap_or("*") } else { "" });
            match kept {
                Some(Err(e)) => Some(format!("The kinds weren't kept: {e}")),
                _ => None,
            }
        }

        /// The first request being modelled, if any.
        fn in_progress(&self) -> Option<&ModelRequest> {
            self.list.iter().find(|r| r.status == Status::InProgress)
        }

        /// The request being modelled: its name, how far along, what its bar says.
        pub(in super::super) fn working(&self) -> Option<(String, f32, String)> {
            let r = self.in_progress()?;
            let (fraction, text) = r.progress.at(now_secs());
            Some((r.name.clone(), fraction, text))
        }

        /// The pictures dropped since last time: to the selected request, else to a new one
        /// named after the first picture. Says what happened.
        pub(in super::super) fn take_arrived(&mut self) -> Option<String> {
            let arrived = std::mem::take(&mut *self.dropped.lock().unwrap());
            if arrived.is_empty() {
                return None;
            }
            let root = self.root();
            let mut problems = vec![];
            let pictures: Vec<(String, Vec<u8>)> = arrived
                .into_iter()
                .filter_map(|(name, bytes)| match bytes {
                    Ok(b) if picture_kind(&b).is_some() => Some((name, b)),
                    Ok(_) => {
                        problems.push(format!("{name}: not a PNG, JPG or WebP picture"));
                        None
                    }
                    Err(e) => {
                        problems.push(format!("{name}: {e}"));
                        None
                    }
                })
                .collect();
            let Some((first, _)) = pictures.first() else { return Some(problems.join("; ")) };
            let target = self.selected.clone().filter(|id| self.list.iter().any(|r| &r.id == id));
            let made = target.is_none();
            let mut r = match target {
                Some(id) => match read(&root, &id) {
                    Ok(r) => r,
                    Err(e) => return Some(e),
                },
                None => match create(&root, first) {
                    Ok(mut r) => {
                        // what the page picked (a kind and a variety of it), else the tab's kind
                        let pick = self.kinds_ui.next.take().or_else(|| self.kinds_ui.tab.clone().map(|t| (t, String::new())));
                        if let Some(k) = pick.as_ref().and_then(|(kid, _)| self.kinds.kind(kid)).cloned() {
                            self.set_kind(&mut r, &k, pick.as_ref().map(|(_, v)| v.as_str()));
                        }
                        self.kinds_ui.page = false;
                        r
                    }
                    Err(e) => return Some(e),
                },
            };
            let (mut added, mut already) = (0, 0);
            for (name, bytes) in &pictures {
                match add_picture(&root, &mut r, name, bytes) {
                    Ok(Added::New(_)) => added += 1,
                    Ok(Added::Already(_)) => already += 1,
                    Err(e) => problems.push(e),
                }
            }
            if let Err(e) = save(&root, &r) {
                problems.push(e);
            }
            let mut said = if made {
                format!("New request {}: {added} picture{} (private/requests/{}/)", r.name, if added == 1 { "" } else { "s" }, r.id)
            } else {
                format!("{}: {added} picture{} added", r.name, if added == 1 { "" } else { "s" })
            };
            if already > 0 {
                said += &format!(", {already} already there");
            }
            if !problems.is_empty() {
                said += &format!(" · not taken: {}", problems.join("; "));
            }
            self.selected = Some(r.id.clone());
            self.reload();
            Some(said)
        }

        /// A change made in the window, written (on what's on disk now: see `merge`).
        fn keep(&mut self, before: &ModelRequest, after: ModelRequest) -> Result<(), String> {
            let root = self.root();
            let out = match read(&root, &after.id) {
                Ok(disk) if disk != *before => merge(before, &after, &disk),
                _ => after,
            };
            save(&root, &out)?;
            if let Some(r) = self.list.iter_mut().find(|r| r.id == out.id) {
                *r = out;
            }
            self.seen = snapshot(&root);
            Ok(())
        }

        /// The renamed request's folder follows its name.
        fn follow(&mut self, id: &str) -> Option<String> {
            let root = self.root();
            let mut r = read(&root, id).ok()?;
            let said = match follow_name(&root, &mut r) {
                Ok(Some(new)) => {
                    for (file, _) in &self.seen {
                        if file.parent().and_then(|p| p.file_name()).map(|n| n == id).unwrap_or(false) {
                            self.thumbs.forget(file);
                        }
                    }
                    if self.selected.as_deref() == Some(id) {
                        self.selected = Some(new.clone());
                    }
                    if let Some(note) = self.lines_notes.remove(id) {
                        self.lines_notes.insert(new.clone(), note);
                    }
                    if let Some(k) = self.queue.place(id) {
                        self.queue.runs[k].id = new.clone();
                        self.keep_queue();
                    }
                    Some(format!("{}: now private/requests/{new}/", r.name))
                }
                Ok(None) => None,
                Err(e) => Some(format!("{}: its folder keeps its name ({e})", r.name)),
            };
            self.reload();
            said
        }

        /// The request a run is on (or being started for), if any: one at a time.
        pub(in super::super) fn busy(&self) -> Option<String> {
            self.starting.as_ref().map(|(id, _)| id.clone()).or_else(|| self.in_progress().map(|r| r.id.clone()))
        }

        /// Start: Claude Code on the request, on its own (run_claude.py, which keeps the log
        /// and the status); `GoOn` ("The lines look right: go on"): from the model, on the
        /// lines the user checked, what the user says is off in them going with it; `Again`
        /// ("Make it again with these comments"): from the model the last run made, the
        /// user's comments on it going with it. The console opens on it.
        pub(in super::super) fn start_run(&mut self, id: &str, how: How) -> String {
            if let Some(other) = self.busy() {
                return format!("One at a time: {other} is being made");
            }
            let mut args = vec!["tools/blender/run_claude.py".to_string(), id.to_string()];
            match how {
                How::GoOn => args.push("--go-on".into()),
                How::Again => args.push("--again".into()),
                How::Variants => args.push("--variants".into()),
                How::Whole => {}
            }
            let note = match how {
                How::Again => self.list.iter().find(|r| r.id == id).map(|r| r.comments.trim().to_string()).unwrap_or_default(),
                // (the variants run reads how many and how they differ in the request)
                How::Variants => String::new(),
                _ => self.lines_notes.get(id).map(|n| n.trim().to_string()).unwrap_or_default(),
            };
            if !note.is_empty() {
                args.extend(["--note".into(), note]);
            }
            if !self.model.is_empty() {
                args.extend(["--model".into(), self.model.clone()]);
            }
            match self.launch(&args) {
                Ok(()) => {
                    if matches!(how, How::Whole | How::GoOn) {
                        self.lines_notes.remove(id);
                    }
                    self.starting = Some((id.to_string(), now_secs()));
                    self.unqueue(id);
                    self.selected = Some(id.to_string());
                    self.console_open = true;
                    self.failed_silent.remove(id);
                    match how {
                        How::GoOn => format!("{MAKER} goes on with {id} from its lines: the console shows everything it does"),
                        How::Again => format!("{MAKER} makes {id} again from its last model, with your comments: the console shows everything it does"),
                        How::Variants => format!("{MAKER} makes variants of {id} from its model: the console shows everything it does"),
                        How::Whole => format!("{MAKER} is starting on {id}: the console shows everything it does"),
                    }
                }
                Err(e) => format!("Not started: {e}"),
            }
        }

        /// "Make them all": the requests one was split into that aren't made yet, one after
        /// another (the next once the last has ended, done or failed; one whose lines wait for
        /// the user's check is left to them).
        pub(in super::super) fn make_all(&mut self, id: &str) -> String {
            let Some(parent) = self.list.iter().find(|r| r.id == id) else { return format!("{id} isn't there any more") };
            let name = parent.name.clone();
            let todo: Vec<String> = parent.children.iter().filter(|c| self.to_make(c) && self.queue.place(c).is_none()).cloned().collect();
            if todo.is_empty() {
                return format!("{name}: its requests are all made (or being made, or queued)");
            }
            let n = todo.len();
            self.queue.runs.extend(todo.into_iter().map(|id| Queued { id, ..Default::default() }));
            self.keep_queue();
            let first = self.next_in_queue();
            format!("{name}: {n} queued, to be made one after another{}", first.map(|s| format!(". {s}")).unwrap_or_default())
        }

        /// Whether a request is still to be made: waiting (or failed), not split already, its
        /// lines not waiting for the user's check.
        fn to_make(&self, id: &str) -> bool {
            self.list.iter().find(|r| r.id == id).is_some_and(|r| matches!(r.status, Status::Waiting | Status::Failed) && !r.lines_ready() && r.children.is_empty())
        }

        /// Whether a queued run can go now: its whole recipe while the request is still to be
        /// made, or still as it was when queued (made, to make it again from its pictures); on
        /// from lines that wait for the user's check; made again once made. Never while it's
        /// being made.
        fn can_run(&self, q: &Queued) -> bool {
            self.list.iter().find(|r| r.id == q.id).is_some_and(|r| match q.how {
                How::Whole => r.status != Status::InProgress && !r.lines_ready() && r.children.is_empty() && (matches!(r.status, Status::Waiting | Status::Failed) || r.status == q.from),
                How::GoOn => r.lines_ready(),
                How::Again => r.status == Status::Done,
                How::Variants => r.can_vary() && r.variants > 0,
            })
        }

        /// The queue written (what goes wrong said in the window's problems).
        fn keep_queue(&mut self) {
            let path = queue_path(&self.root());
            let written = path.parent().map(disk::create_dir_all).unwrap_or(Ok(())).and_then(|()| disk::write(&path, content::to_text(&self.queue)));
            if let Err(e) = written {
                self.problems.push(format!("the queue isn't kept: {}: {e}", path.display()));
            }
        }

        /// A run put in the queue, after the others (a request already in it isn't put twice);
        /// with nothing being made, it starts now. What to say.
        pub(in super::super) fn enqueue(&mut self, id: &str, how: How) -> String {
            let name = self.list.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_else(|| id.to_string());
            if self.busy().as_deref() == Some(id) {
                return format!("{name} is being made now");
            }
            if let Some(k) = self.queue.place(id) {
                return format!("{name} is {} in the queue already", nth(k + 1));
            }
            let note = if how == How::GoOn { self.lines_notes.get(id).map(|n| n.trim().to_string()).unwrap_or_default() } else { String::new() };
            let from = self.list.iter().find(|r| r.id == id).map(|r| r.status).unwrap_or_default();
            self.queue.runs.push(Queued { id: id.to_string(), how, note, from });
            self.keep_queue();
            if let Some(s) = self.next_in_queue() {
                return s;
            }
            let k = self.queue.runs.len();
            format!("{name} queued, {}: it starts once {}", nth(k), if self.queue.paused { "the queue goes on" } else if k == 1 { "the one being made has ended" } else { "those before it have" })
        }

        /// Every request waiting to be made and not queued yet put in the queue, in the list's
        /// order. What to say.
        pub(in super::super) fn enqueue_waiting(&mut self) -> String {
            let busy = self.busy();
            let todo: Vec<String> = self.list.iter().filter(|r| r.status == Status::Waiting && Some(&r.id) != busy.as_ref() && self.to_make(&r.id) && self.queue.place(&r.id).is_none()).map(|r| r.id.clone()).collect();
            if todo.is_empty() {
                return "Nothing waiting to be made that isn't queued already".into();
            }
            let n = todo.len();
            self.queue.runs.extend(todo.into_iter().map(|id| Queued { id, ..Default::default() }));
            self.keep_queue();
            let first = self.next_in_queue();
            format!("{n} queued, to be made one after another{}", first.map(|s| format!(". {s}")).unwrap_or_default())
        }

        /// A queued run taken out of the queue.
        pub(in super::super) fn unqueue(&mut self, id: &str) {
            if let Some(k) = self.queue.place(id) {
                self.queue.runs.remove(k);
                self.keep_queue();
            }
        }

        /// A queued run moved a place up (`by` -1) or down (1).
        pub(in super::super) fn move_queued(&mut self, id: &str, by: i32) {
            if let Some(k) = self.queue.place(id) {
                let to = (k as i32 + by).clamp(0, self.queue.runs.len() as i32 - 1) as usize;
                let q = self.queue.runs.remove(k);
                self.queue.runs.insert(to, q);
                self.keep_queue();
            }
        }

        /// The queue emptied.
        pub(in super::super) fn clear_queue(&mut self) {
            self.queue.runs.clear();
            self.keep_queue();
        }

        /// The queue paused (nothing more starts, the run going on ends as it would) or going on.
        pub(in super::super) fn pause_queue(&mut self, paused: bool) -> Option<String> {
            self.queue.paused = paused;
            self.keep_queue();
            if paused { None } else { self.next_in_queue() }
        }

        /// The next run in the queue started, when nothing is being made and the queue isn't
        /// paused; one that can't go (made meanwhile, being made, gone) taken out on the way.
        fn next_in_queue(&mut self) -> Option<String> {
            if self.busy().is_some() || self.queue.paused || self.queue.runs.is_empty() {
                return None;
            }
            while !self.queue.runs.is_empty() {
                let q = self.queue.runs.remove(0);
                if !self.can_run(&q) {
                    continue;
                }
                if q.how == How::GoOn && !q.note.is_empty() {
                    self.lines_notes.insert(q.id.clone(), q.note.clone());
                }
                // split into several by this run: its own requests next
                if self.list.iter().any(|r| r.id == q.id && r.several && r.parent.is_empty()) {
                    self.splitting = Some(q.id.clone());
                }
                self.keep_queue();
                return Some(self.start_run(&q.id, q.how));
            }
            self.keep_queue();
            None
        }

        /// run_claude.py started on its own, reaped by `watch` as it ends (the tests keep what
        /// would have run instead).
        fn launch(&mut self, args: &[String]) -> Result<(), String> {
            #[cfg(test)]
            {
                self.launched.push(args.to_vec());
                Ok(())
            }
            #[cfg(all(not(test), not(target_arch = "wasm32")))]
            {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let child = python(&self.root(), &args)?;
                self.children.push(child);
                Ok(())
            }
            // the browser: in a sandbox of Anthropic's, on the user's own key
            #[cfg(all(not(test), target_arch = "wasm32"))]
            {
                let root = self.root();
                self.cloud.start(&root, args)
            }
        }

        /// Cancel: the run's stop file (run_claude.py stops Claude and the request waits
        /// again); with no run there to read it, the editor does that a few seconds later.
        pub(in super::super) fn cancel_run(&mut self, id: &str) -> String {
            let stop = folder(&self.root()).join(id).join("stop");
            match disk::write(&stop, "") {
                Ok(()) => {
                    self.cancelling = Some((id.to_string(), now_secs()));
                    format!("Cancelling {id}")
                }
                Err(e) => format!("Not cancelled: {}: {e}", stop.display()),
            }
        }

        /// The cancel asked for `secs` earlier (the tests).
        #[cfg(test)]
        pub(in super::super) fn age_cancel(&mut self, secs: f64) {
            if let Some((_, at)) = &mut self.cancelling {
                *at -= secs;
            }
        }

        /// A request's status set by model_requests.py (which writes the log line, timed by the
        /// computer's clock), or here when Python can't.
        fn settle(&mut self, id: &str, args: &[&str], fallback: impl FnOnce(&mut ModelRequest)) {
            let root = self.root();
            if !python_now(&root, args) {
                if let Ok(mut r) = read(&root, id) {
                    fallback(&mut r);
                    let _ = save(&root, &r);
                }
            }
        }

        /// Each second: what changed on disk (Claude's work, a picture added by hand) shows;
        /// a cancel nobody answered, a run that never started and a run gone silent are
        /// settled. What to say, when there's something.
        pub(in super::super) fn watch(&mut self, now: f64) -> Option<String> {
            if now < self.next_look {
                return None;
            }
            self.next_look = now + 1000.0;
            let root = self.root();
            let mut said = None;
            self.children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
            // the browser's run, in the cloud: its events into the request's files
            #[cfg(target_arch = "wasm32")]
            if let Some(s) = self.cloud.poll(&root) {
                said = Some(s);
            }
            if let Some((id, at)) = self.cancelling.clone() {
                let stop = folder(&root).join(&id).join("stop");
                if !disk::exists(&stop) {
                    self.cancelling = None;
                } else if now_secs() - at > 6.0 {
                    // no run took it: nothing was running
                    let _ = disk::remove_file(&stop);
                    let args = ["tools/blender/model_requests.py", "wait", id.as_str(), "--note", "Cancelled from the editor: nothing was running"];
                    self.settle(&id, &args, |r| {
                        r.status = Status::Waiting;
                        r.progress = Progress::default();
                    });
                    self.cancelling = None;
                    said = Some(format!("{id}: nothing was running; it waits again"));
                }
            }
            if let Some((id, at)) = self.starting.clone() {
                let begun = self.list.iter().find(|r| r.id == id).map(|r| r.progress.started >= at - 2.0 || r.status == Status::Failed).unwrap_or(true);
                if begun {
                    self.starting = None;
                } else if now_secs() - at > 30.0 {
                    self.starting = None;
                    said = Some(format!("{id}: the run didn't start (is Python there?)"));
                }
            }
            let mut seen = snapshot(&root);
            let wall = disk::now();
            for r in self.list.clone() {
                let dir = folder(&root).join(&r.id);
                if gone_silent(&r, last_word(&seen, &dir), wall) && self.failed_silent.insert(r.id.clone()) {
                    let reason = format!("no news for {} min (nothing written in its folder)", SILENT as i64 - 1);
                    let args = ["tools/blender/model_requests.py", "fail", r.id.as_str(), reason.as_str()];
                    self.settle(&r.id, &args, |rec| {
                        rec.status = Status::Failed;
                        rec.progress.step = format!("failed: {reason}");
                    });
                    said = Some(format!("{}: failed, {reason}", r.name));
                    seen = snapshot(&root);
                }
            }
            if !(self.loaded && seen == self.seen) {
                let before: HashMap<_, _> = self.seen.iter().cloned().collect();
                for (file, t) in &seen {
                    if before.get(file) != Some(t) {
                        self.thumbs.forget(file);
                    }
                }
                self.reload();
            }
            // a request the queue had split: once it's split (its run over), its requests next,
            // in the drawings' order
            if self.busy().is_none() {
                if let Some(id) = self.splitting.take() {
                    let children = self.list.iter().find(|r| r.id == id).map(|r| r.children.clone()).unwrap_or_default();
                    let todo: Vec<String> = children.into_iter().filter(|c| self.to_make(c) && self.queue.place(c).is_none()).collect();
                    for (k, c) in todo.into_iter().enumerate() {
                        self.queue.runs.insert(k, Queued { id: c, ..Default::default() });
                    }
                    self.keep_queue();
                }
            }
            // the queue: the next, once the last has ended (and its start was seen, which can
            // be a look after its files changed)
            if let Some(s) = self.next_in_queue() {
                said = Some(s);
            }
            said
        }

        /// A request's log.txt, read again when it changes.
        fn log_of(&mut self, id: &str) -> Option<Arc<String>> {
            let path = folder(&self.root()).join(id).join("log.txt");
            let t = disk::modified(&path)?;
            if self.logs.get(id).map(|(at, _)| *at != t).unwrap_or(true) {
                let text = disk::read_to_string(&path).ok()?;
                self.logs.insert(id.to_string(), (t, Arc::new(text)));
            }
            self.logs.get(id).map(|(_, s)| s.clone())
        }

        /// The console at the window's foot: the log of the request selected (else of the one
        /// being made), its last lines when closed, all of it open; Copy takes it all.
        fn console(&mut self, ui: &mut egui::Ui, id: Option<String>) -> Option<String> {
            let mut said = None;
            let log = id.as_deref().and_then(|id| self.log_of(id));
            // (for the checks: the log's last lines)
            platform::set_hook("requestlog", log.as_deref().map(|t| tail(t, 6).0.trim_end()).unwrap_or(""));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // a fold's triangle, painted (the fonts have no arrow glyphs), and its name
                let (_, icon) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::click());
                egui::collapsing_header::paint_default_icon(ui, if self.console_open { 1.0 } else { 0.0 }, &icon);
                let name = ui.add(egui::Label::new(RichText::new("Console").strong()).sense(Sense::click()));
                let toggle = icon.union(name).on_hover_text("Everything the runs did: Claude reached, the model it runs, what was sent, each step, the end. Click to see it all, or fold it");
                mark("uiRequestConsole", &toggle);
                if toggle.clicked() {
                    self.console_open = !self.console_open;
                }
                match (&id, &log) {
                    (Some(id), Some(text)) => hint(ui, &format!("{id} · {} lines · private/requests/{id}/log.txt", text.lines().count())),
                    (Some(id), None) => hint(ui, &format!("{id}: no run yet")),
                    _ => hint(ui, "the selected request's runs"),
                }
                if let (Some(id), Some(text)) = (&id, &log) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(if cfg!(target_arch = "wasm32") { "Download log.txt" } else { "Open log.txt" }).clicked() {
                            if let Err(e) = reveal(&folder(&self.root()).join(id).join("log.txt")) {
                                said = Some(e);
                            }
                        }
                        let copy = ui.small_button("Copy").on_hover_text("The whole log, each line with its time");
                        mark("uiRequestCopy", &copy);
                        if copy.clicked() {
                            ui.ctx().copy_text(text.to_string());
                            said = Some(format!("{id}'s log copied ({} lines)", text.lines().count()));
                        }
                    });
                }
            });
            let Some(text) = log else {
                if self.console_open {
                    hint(ui, "Start a request: everything Claude does shows here, and stays in its log.txt.");
                }
                return said;
            };
            if self.console_open {
                let (shown, before) = tail(&text, CONSOLE_LINES);
                // all of its panel's height (its top edge drags it taller)
                egui::ScrollArea::vertical().id_salt("request-console").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                    if before > 0 {
                        hint(ui, &format!("… {before} lines before these: Copy takes them all"));
                    }
                    ui.add(egui::Label::new(console_text(shown.trim_end(), false)).selectable(true).wrap());
                });
            } else {
                let (last, _) = tail(&text, 3);
                ui.add(egui::Label::new(console_text(last.trim_end(), true)).truncate());
            }
            said
        }

        // ------------------------------------------------------------ drawing

        /// A picture clicked, opened: the desktop in the system's viewer; the browser in a
        /// window of its own (`viewer`), never a download. What went wrong, if it did.
        fn open_picture(&mut self, path: &Path) -> Option<String> {
            if cfg!(target_arch = "wasm32") {
                self.viewing = Some(path.to_path_buf());
                return None;
            }
            reveal(path).err()
        }

        /// The browser's picture viewer, while a picture is open: the picture as big as the
        /// screen allows (at most its own size), its size in pixels, Download for a copy of
        /// it, Close (Esc or the window's close too). A hook (picture) says which is open.
        pub(in super::super) fn viewer(&mut self, ctx: &egui::Context) {
            let Some(path) = self.viewing.clone() else {
                platform::set_hook("picture", "");
                return;
            };
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            platform::set_hook("picture", &name);
            let (mut open, mut close) = (true, false);
            let screen = ctx.content_rect();
            let most = egui::vec2(screen.width() * 0.8, screen.height() * 0.8 - 40.0);
            egui::Window::new(&name).id(egui::Id::new("request-picture")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                match self.thumbs.at(ctx, &path, 1600) {
                    Some(Ok((id, size))) => {
                        let scale = (most.x / size.x.max(1.0)).min(most.y / size.y.max(1.0)).min(1.0);
                        let r = ui.add(egui::Image::new((id, size * scale)).corner_radius(3.0));
                        mark("uiPictureShown", &r);
                        hint(ui, &format!("{:.0} × {:.0} pixels", size.x, size.y));
                    }
                    Some(Err(e)) => warn(ui, &e),
                    None => hint(ui, "Reading it…"),
                }
                ui.horizontal(|ui| {
                    let download = ui.button("Download").on_hover_text("A copy of it, saved by the browser");
                    mark("uiPictureDownload", &download);
                    if download.clicked() {
                        if let Err(e) = reveal(&path) {
                            self.problems.push(e);
                        }
                    }
                    let c = ui.button("Close");
                    mark("uiPictureClose", &c);
                    close = c.clicked();
                });
            });
            if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.viewing = None;
            }
        }

        /// A picture, `height` points high (its width as it is); clicked, it opens.
        fn picture(&mut self, ui: &mut egui::Ui, path: &Path, height: f32, sense: Sense) -> egui::Response {
            match self.thumbs.get(ui.ctx(), path) {
                Some(Ok((id, size))) => {
                    let w = (size.x / size.y.max(1.0) * height).clamp(height * 0.4, height * 2.5);
                    ui.add(egui::Image::new((id, egui::vec2(w, height))).corner_radius(3.0).sense(sense))
                }
                other => {
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(height * 0.8, height), sense);
                    ui.painter().rect_filled(rect, 3.0, Color32::from_gray(40));
                    let (text, why) = match other {
                        Some(Err(e)) => ("?", e),
                        _ => ("…", "Reading it".to_string()),
                    };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(13.0), Color32::from_gray(150));
                    r.on_hover_text(why)
                }
            }
        }

        /// A request's drawings (its `lines` that are there), each with what it shows; clicked,
        /// one opens full size. What to say when it doesn't.
        fn drawings(&mut self, ui: &mut egui::Ui, id: &str, files: &[String]) -> Option<String> {
            let mut said = None;
            ui.horizontal_wrapped(|ui| {
                for f in files {
                    let path = self.picture_path(id, f);
                    let shown = self.picture(ui, &path, 130.0, Sense::click()).on_hover_text(format!("{}: click to open it", drawing_about(f)));
                    if shown.clicked() {
                        said = self.open_picture(&path);
                    }
                }
            });
            said
        }

        /// Where the next pictures go, lit while files hover.
        /// The browser's runs: what one may cost at most (on the user's key; kept in this
        /// browser), as the window's banner shows it.
        #[cfg(target_arch = "wasm32")]
        fn cap(&mut self, ui: &mut egui::Ui) {
            const NAME: &str = "claude_run_cap";
            if self.cloud.cap == 0 {
                self.cloud.cap = platform::stored_text(NAME).and_then(|t| t.trim().parse().ok()).unwrap_or(super::cloud::CAP_CENTS);
            }
            let mut dollars = self.cloud.cap / 100;
            let set = ui
                .add(egui::DragValue::new(&mut dollars).range(1..=200).prefix("$"))
                .on_hover_text("The most a run may cost, at the API's prices, on your key: it stops there (a request has cost $0.20 to $7.50 so far, about $2 typically)");
            mark("uiRequestCap", &set);
            if set.changed() {
                self.cloud.cap = dollars * 100;
                let _ = platform::store_text(NAME, &self.cloud.cap.to_string());
            }
            hint(ui, "at most");
        }

        /// A request's run button: Start (Start again, Make it again) or Cancel, and what was
        /// asked for. One run at a time.
        fn run_button(&self, ui: &mut egui::Ui, r: &ModelRequest, short: bool) -> Option<Run> {
            if !r.children.is_empty() {
                hint(ui, &format!("split into {}", r.children.len()));
                return None;
            }
            let starting = self.starting.as_ref().is_some_and(|(id, _)| *id == r.id);
            if r.status == Status::InProgress || starting {
                let cancelling = self.cancelling.as_ref().is_some_and(|(id, _)| *id == r.id);
                let cancel = ui.add_enabled(!starting && !cancelling, egui::Button::new(if cancelling { "Cancelling…" } else { "Cancel" })).on_hover_text(format!("Stops {MAKER}: the request waits again, its log kept"));
                mark(&format!("uiRequestCancel_{}", r.id), &cancel);
                return cancel.clicked().then(|| Run::Cancel(r.id.clone()));
            }
            let busy = self.busy();
            if let Some(k) = self.queue.place(&r.id) {
                let out = ui.button(format!("Queued, {}", nth(k + 1))).on_hover_text("In the queue: it starts on its own once those before it have been made. Click to take it out");
                mark(&format!("uiRequestUnqueue_{}", r.id), &out);
                return out.clicked().then(|| Run::Unqueue(r.id.clone()));
            }
            if busy.is_some() {
                let label = match (r.status, short) {
                    (Status::Done, false) => "Queue it to make again",
                    _ => "Queue",
                };
                let queue = ui.button(label).on_hover_text(format!("{} is being made: this one starts on its own once it (and the queue before it) has ended", busy.unwrap_or_default()));
                mark(&format!("uiRequestQueue_{}", r.id), &queue);
                return queue.clicked().then(|| Run::Queue(r.id.clone(), How::Whole));
            }
            let label = match (r.status, short) {
                (Status::Failed, false) => "Start again",
                (Status::Done, false) => "Make it again",
                (Status::Waiting, false) if r.lines_ready() => "Start over",
                _ => "Start",
            };
            let about = match (r.lines_ready(), r.check_lines) {
                _ if r.several && r.parent.is_empty() => format!("{MAKER} finds each drawing on the pictures and makes it a request of its own; Make them all then has them made one after another"),
                _ if r.status == Status::Done => format!("{MAKER} makes it once more, from the pictures; to change the model it made instead, say what in \"Make it again\" below"),
                (true, true) => format!("{MAKER} starts again from the pictures: it checks the lines there against them (fixing first what your note above says is off), and stops for your check again"),
                (true, false) => format!("{MAKER} starts again from the pictures, checking the lines there against them (fixing first what your note above says is off), and makes the whole model this time"),
                (false, true) => format!("{MAKER} draws its lines over the pictures and the front and the side from them, then stops for you to check them; the console shows everything it does"),
                (false, false) => format!("{MAKER} models it in Blender, unattended, following tools/blender/requests.md; the console shows everything it does"),
            };
            let start = ui
                .add_enabled(busy.is_none(), egui::Button::new(label))
                .on_hover_text(about)
                .on_disabled_hover_text(format!("One at a time: {} is being made", busy.unwrap_or_default()));
            mark(&format!("uiRequestStart_{}", r.id), &start);
            start.clicked().then(|| Run::Start(r.id.clone()))
        }

        /// The queue, over the list: the one being made, then each queued run in its order (a
        /// place up or down, or out), Pause or Go on, Clear, and "Queue all waiting".
        fn queue_column(&mut self, ui: &mut egui::Ui) -> Option<Run> {
            let mut asked = None;
            let busy = self.busy();
            let waiting = self.list.iter().filter(|r| r.status == Status::Waiting && self.to_make(&r.id) && self.queue.place(&r.id).is_none() && Some(&r.id) != busy.as_ref()).count();
            egui::Frame::new().fill(Color32::from_gray(22)).stroke(egui::Stroke::new(1.0, Color32::from_gray(58))).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let state = if self.queue.paused { " · paused" } else { "" };
                    ui.label(RichText::new(format!("Queue ({}){state}", self.queue.runs.len())).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.queue.runs.is_empty() {
                            let clear = ui.small_button("Clear").on_hover_text("Empties the queue (the run being made goes on)");
                            mark("uiQueueClear", &clear);
                            if clear.clicked() {
                                asked = Some(Run::QueueClear);
                            }
                        }
                        let (label, about) = if self.queue.paused { ("Go on", "The queue goes on: its next starts once nothing's being made") } else { ("Pause", "Nothing more starts from the queue until it goes on (the run being made ends as it would)") };
                        let pause = ui.small_button(label).on_hover_text(about);
                        mark("uiQueuePause", &pause);
                        if pause.clicked() {
                            asked = Some(Run::QueuePause(!self.queue.paused));
                        }
                    });
                });
                if let Some(r) = self.in_progress().cloned() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("now").size(11.0).color(crate::hud::MUTED));
                        ui.label(RichText::new(&r.name).size(12.5));
                    });
                    bar(ui, &r, ui.available_width(), 4.0);
                } else if let Some((id, _)) = &self.starting {
                    hint(ui, &format!("now: {id}, starting"));
                }
                if self.queue.runs.is_empty() {
                    hint(ui, "Queued requests are made one after another, each once the last has ended: Queue on a request (while another is being made), or all of those waiting.");
                }
                let runs = self.queue.runs.clone();
                let last = runs.len().saturating_sub(1);
                for (k, q) in runs.iter().enumerate() {
                    let r = self.list.iter().find(|r| r.id == q.id);
                    let name = r.map(|r| r.name.clone()).unwrap_or_else(|| q.id.clone());
                    let how = match q.how {
                        How::Whole => "",
                        How::GoOn => " · go on",
                        How::Again => " · again",
                        How::Variants => " · variants",
                    };
                    let row = ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", k + 1)).size(11.5).color(Color32::from_gray(150)));
                        if let Some(r) = r {
                            chip(ui, r);
                        }
                        // the name in what the buttons leave, cut short
                        let room = (ui.available_width() - 70.0).max(40.0);
                        ui.allocate_ui(egui::vec2(room, ui.spacing().interact_size.y), |ui| {
                            ui.add(egui::Label::new(RichText::new(format!("{name}{how}")).size(12.5)).truncate());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let out = ui.small_button("×").on_hover_text("Out of the queue");
                            mark(&format!("uiQueueOut_{}", q.id), &out);
                            if out.clicked() {
                                asked = Some(Run::Unqueue(q.id.clone()));
                            }
                            let down = arrow(ui, false, k < last).on_hover_text("A place later");
                            mark(&format!("uiQueueDown_{}", q.id), &down);
                            if down.clicked() {
                                asked = Some(Run::QueueMove(q.id.clone(), 1));
                            }
                            let up = arrow(ui, true, k > 0).on_hover_text("A place sooner");
                            mark(&format!("uiQueueUp_{}", q.id), &up);
                            if up.clicked() {
                                asked = Some(Run::QueueMove(q.id.clone(), -1));
                            }
                        });
                    });
                    let how_long = match q.how {
                        How::Whole => "its whole recipe, from its pictures",
                        How::GoOn => "on from the lines you checked",
                        How::Again => "made again with your comments",
                        How::Variants => "its variants, from its model",
                    };
                    let row = row.response.on_hover_text(format!("{name}: {how_long}"));
                    mark(&format!("uiQueued_{}", q.id), &row);
                }
                let all = ui.add_enabled(waiting > 0, egui::Button::new(format!("Queue all waiting ({waiting})"))).on_hover_text("Every request waiting to be made goes in the queue, in the list's order").on_disabled_hover_text("Nothing waiting that isn't queued (or being made) already");
                mark("uiQueueWaiting", &all);
                if all.clicked() {
                    asked = Some(Run::QueueWaiting);
                }
            });
            crate::platform::set_hook("requestqueue", &self.queue.ids().join(";"));
            crate::platform::set_hook("requestqueuepaused", if self.queue.paused { "1" } else { "" });
            ui.add_space(6.0);
            asked
        }

        fn list_column(&mut self, ui: &mut egui::Ui) -> Option<Run> {
            let mut asked = self.queue_column(ui);
            let list: Vec<ModelRequest> = self.list.iter().filter(|r| self.in_tab(r)).cloned().collect();
            platform::set_hook("requestlisted", &list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>().join(","));
            if list.is_empty() && self.problems.is_empty() {
                hint(ui, if self.kinds_ui.tab.is_some() { "No requests of this kind yet: pictures dropped now start one." } else { "No requests yet." });
            }
            for r in &list {
                // (for the checks: its status, how far along, its step)
                let status = serde_json::to_value(r.status).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                platform::set_hook(&format!("request_{}", r.id), &format!("{status}|{:.2}|{}", r.progress.done, r.progress.step));
                let chosen = self.selected.as_deref() == Some(r.id.as_str());
                let (fill, edge) = if chosen { (Color32::from_rgb(38, 52, 76), Color32::from_rgb(84, 116, 164)) } else { (Color32::from_gray(28), Color32::from_gray(50)) };
                // the card's click, under what's drawn on it (its size from the frame before)
                let row_id = ui.id().with(("request-row", &r.id));
                let row = ui.ctx().data(|d| d.get_temp::<egui::Rect>(row_id)).map(|rect| ui.interact(rect, row_id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand));
                let starting = self.starting.as_ref().is_some_and(|(id, _)| *id == r.id);
                // folded by default: its first row and its progress only
                let mut fold = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.id().with(("request-card", &r.id)), false);
                let frame = egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, edge)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(6, 5)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // its first row: unfolded or not, its status, its name, its kind, the rest of
                    // what it asks for (hovered), and its button at the end
                    ui.horizontal(|ui| {
                        let open = fold.is_open();
                        let toggle = fold.show_toggle_button(ui, egui::collapsing_header::paint_default_icon).on_hover_text(if open { "Fold it" } else { "Unfold it: what it made, its pictures, its models" });
                        mark(&format!("uiRequestFold_{}", r.id), &toggle);
                        chip(ui, r);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if r.lines_ready() && !starting {
                                // the user's turn: the request's panel has the lines and what's next
                                let check = ui.button("Check").on_hover_text("Its lines are ready: see them, say what's off, and go on");
                                mark(&format!("uiRequestCheck_{}", r.id), &check);
                                if check.clicked() {
                                    self.selected = Some(r.id.clone());
                                    self.deleting = false;
                                }
                            } else if let Some(a) = self.run_button(ui, r, true) {
                                asked = Some(a);
                            }
                            let label = self.kinds.label(r);
                            let info = info_icon(ui).on_hover_text(details_as(r, &label));
                            mark(&format!("uiRequestInfo_{}", r.id), &info);
                            if let Some(v) = r.rating {
                                rating_tag(ui, v);
                            }
                            kind_tag(ui, &label, r.kind);
                            // the name in what's left, cut short (whole when hovered)
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(RichText::new(&r.name).strong()).truncate()).on_hover_text(&r.name);
                            });
                        });
                    });
                    fold.show_body_unindented(ui, |ui| {
                        ui.add_space(2.0);
                        // what it made, by its whole file's name
                        if r.status == Status::Done && !r.prop.is_empty() {
                            let space = if r.mine { "content" } else { "temporary" };
                            let file = format!("{space}/assets/models/{}.glb", r.prop);
                            let made = ui.add(egui::Label::new(RichText::new(&file).monospace().size(11.0).color(DONE)).wrap());
                            let made = made.on_hover_text(format!("The prop it made, {} (its record {space}/data/props/{}.json)", r.prop, r.prop));
                            mark(&format!("uiRequestMade_{}", r.id), &made);
                        }
                        if starting {
                            hint(ui, &format!("starting {MAKER}…"));
                        } else if r.status == Status::Failed {
                            ui.label(RichText::new(&r.progress.step).size(11.5).color(FAILED));
                        } else if r.lines_ready() {
                            ui.label(RichText::new(LINES_READY).size(11.5).color(WAITING));
                        }
                        // its drawings, and the models it made (its line work left out)
                        ui.columns(2, |cols| {
                            boxed(&mut cols[0], "Drawings", |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for f in r.images.iter().take(6) {
                                        let path = self.picture_path(&r.id, f);
                                        self.picture(ui, &path, 40.0, Sense::hover()).on_hover_text(f.as_str());
                                    }
                                    if r.images.len() > 6 {
                                        hint(ui, &format!("+{}", r.images.len() - 6));
                                    }
                                    if r.images.is_empty() {
                                        hint(ui, "no pictures");
                                    }
                                });
                            });
                            boxed(&mut cols[1], "Models", |ui| {
                                let made = made_models(r);
                                if !r.children.is_empty() {
                                    hint(ui, &format!("split into {}: each its own request", r.children.len()));
                                } else if made.is_empty() {
                                    hint(ui, if r.status == Status::Done { "none" } else { "none yet" });
                                }
                                for (k, (prop, render)) in made.into_iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        match render {
                                            Some(f) => {
                                                let path = self.picture_path(&r.id, &f);
                                                self.picture(ui, &path, 32.0, Sense::hover());
                                            }
                                            None => {
                                                ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::hover());
                                            }
                                        }
                                        // (its prop's id too long for the box: whole when hovered)
                                        let name = if k == 0 { "model".to_string() } else { format!("variant {}", k + 1) };
                                        ui.add(egui::Label::new(RichText::new(name).size(11.5).color(DONE)).truncate()).on_hover_text(format!("The prop {prop}"));
                                    });
                                }
                            });
                        });
                    });
                    // its progress, at its foot, folded or not
                    foot(ui, r, starting);
                });
                fold.store(ui.ctx());
                ui.ctx().data_mut(|d| d.insert_temp(row_id, frame.response.rect));
                if let Some(row) = row {
                    mark(&format!("uiRequest_{}", r.id), &row);
                    if row.clicked() {
                        self.selected = Some(r.id.clone());
                        self.deleting = false;
                    }
                }
                ui.add_space(4.0);
            }
            for p in &self.problems {
                warn(ui, p);
            }
            asked
        }

        /// The selected request, to edit: a prop to use when its "Use it" is clicked, what to
        /// say, a run asked for.
        fn pane(&mut self, ui: &mut egui::Ui, content: &Content, name_focused: &mut bool) -> (Option<String>, Option<String>, Option<Run>) {
            let (mut use_prop, mut said, mut asked) = (None, None, None);
            // another request to show (a split one's, the one it was split from)
            let mut show: Option<String> = None;
            let Some(i) = self.selected.as_ref().and_then(|id| self.list.iter().position(|r| &r.id == id)) else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No request shown").size(18.0).color(Color32::from_gray(170)));
                    hint(ui, "Pick one on the left, or + by the tabs for a new one: pictures dropped on the window start it.");
                });
                return (None, None, None);
            };
            let before = self.list[i].clone();
            let mut r = before.clone();
            let mut remove = None;
            // what its kind (or its variety) asks: the settings shown
            let kinds = self.kinds.clone();
            let params = kinds.params_of(&r);
            // the drawings Claude works from, as it makes them: see how well the lines sit
            let drawn: Vec<String> = r.lines.iter().filter(|f| disk::is_file(&self.picture_path(&r.id, f))).cloned().collect();
            if r.lines_ready() {
                // Claude stopped for the user's check ("Let me check the lines first"): the
                // lines and what to do next first, in reach
                section(ui, "Check the lines");
                hint(ui, "Claude stopped here for you: do its lines sit on the design, and are the front and the side right? Click one to see it full size.");
                if let Some(s) = self.drawings(ui, &r.id, &drawn) {
                    said = Some(s);
                }
                if !r.reply.is_empty() {
                    ui.label(RichText::new(format!("Claude: {}", r.reply)).size(12.0));
                }
                let note = self.lines_notes.entry(r.id.clone()).or_default();
                let field = ui.add(
                    egui::TextEdit::multiline(note)
                        .desired_rows(2)
                        .desired_width(420.0)
                        .hint_text("What's off, if anything (the tail is longer, the ears sit higher): Claude fixes it first"),
                );
                mark("uiRequestLinesNote", &field);
                let busy = self.busy();
                // (it being made itself: nothing more)
                let mine = busy.as_deref() == Some(r.id.as_str());
                let other = busy.filter(|_| !mine);
                let label = if other.is_some() { "The lines look right: queue it to go on" } else { "The lines look right: go on" };
                let about = match &other {
                    Some(b) => format!("{b} is being made: Claude goes on from these lines on its own once it (and the queue before it) has ended"),
                    None => "Claude keeps these lines (fixing first what you say is off) and goes on to build the model from them: the long part".into(),
                };
                let go = ui
                    .add_enabled(!mine && self.queue.place(&r.id).is_none(), egui::Button::new(RichText::new(label).strong()))
                    .on_hover_text(about)
                    .on_disabled_hover_text(if mine { "Being made now" } else { "In the queue already" });
                mark("uiRequestGoOn", &go);
                if go.clicked() {
                    asked = Some(if other.is_some() { Run::Queue(r.id.clone(), How::GoOn) } else { Run::GoOn(r.id.clone()) });
                }
                ui.add_space(6.0);
            }
            egui::Grid::new("request-fields").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                ui.horizontal(|ui| {
                    let name = ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(200.0));
                    mark("uiRequestName", &name);
                    *name_focused = name.has_focus();
                    if name.changed() {
                        self.renamed = Some(r.id.clone());
                    }
                    ui.label(RichText::new(&r.id).monospace().size(11.0).color(Color32::GRAY)).on_hover_text("Its folder under private/requests/ (it follows the name while it waits)");
                });
                ui.end_row();
                // what its runs have cost: the total, each run's under it when there's more
                // than one
                if !r.costs.is_empty() {
                    ui.label("Cost");
                    let total = costs_text(&r);
                    if r.costs.len() == 1 {
                        let c = ui.label(RichText::new(&total).size(12.5)).on_hover_text("At the API's prices, for its one run");
                        mark("uiRequestCost", &c);
                    } else {
                        let fold = egui::CollapsingHeader::new(RichText::new(format!("{total} over {} runs", r.costs.len())).size(12.5)).id_salt(("request-costs", &r.id)).show(ui, |ui| {
                            for (k, c) in r.costs.iter().enumerate() {
                                ui.label(RichText::new(format!("run {}: ${c:.2}", k + 1)).size(11.5).color(Color32::from_gray(190)));
                            }
                        });
                        mark("uiRequestCost", &fold.header_response);
                    }
                    ui.end_row();
                }
                ui.label("Category");
                ui.vertical(|ui| {
                    // the window's categories (its tabs), and the varieties of its own
                    ui.horizontal_wrapped(|ui| {
                        for k in &kinds.kinds {
                            let on = r.kind_key() == k.id;
                            let about = super::kinds::about_kind(k);
                            let b = ui.selectable_label(on, k.name.to_lowercase()).on_hover_text(about);
                            mark(&format!("uiRequestKind{}", capital(&k.id)), &b);
                            if b.clicked() && !on {
                                self.set_kind(&mut r, k, None);
                            }
                        }
                    });
                    // what its category (and its variety) will ask of the run, folded; its window to edit it
                    let says = kinds.features_of(&r);
                    ui.horizontal(|ui| {
                        egui::CollapsingHeader::new(RichText::new(format!("its category's instructions ({})", says.len())).size(11.5)).id_salt(("request-kind-says", &r.id)).show(ui, |ui| {
                            for f in &says {
                                ui.label(RichText::new(format!("{}: {}", f.keyword.trim(), f.text.trim())).size(11.5).color(Color32::from_gray(185)));
                            }
                            if says.is_empty() {
                                hint(ui, "none yet");
                            }
                        });
                        let edit = ui.small_button("Edit…").on_hover_text("Its category's instructions, settings and varieties: what every request of it asks");
                        mark("uiRequestKindInstructions", &edit);
                        if edit.clicked() {
                            if let Some(k) = kinds.of(&r).0 {
                                (self.kinds_ui.tab, self.kinds_ui.open) = (Some(k.id.clone()), true);
                                self.kinds_ui.variety = (!r.variety.is_empty()).then(|| r.variety.clone());
                            }
                        }
                    });
                    if let Some(k) = kinds.of(&r).0.filter(|k| !k.varieties.is_empty()).cloned() {
                        ui.horizontal_wrapped(|ui| {
                            hint(ui, "variety");
                            for v in &k.varieties {
                                let b = ui.selectable_label(r.variety == v.id, v.name.to_lowercase()).on_hover_text(format!("{} of its own instructions, added to the category's", v.features.len()));
                                mark(&format!("uiRequestVariety_{}", v.id), &b);
                                if b.clicked() && r.variety != v.id {
                                    self.set_variety(&mut r, &v.id);
                                }
                            }
                        });
                    }
                });
                ui.end_row();
                // (a rock's height is its pictures' proportions' on its footprint)
                if params.contains(&Param::Height) {
                    ui.label("Height");
                    ui.add(egui::DragValue::new(&mut r.height).speed(0.05).range(0.1..=40.0).suffix(" cells").max_decimals(2)).on_hover_text("How tall it stands (a cell is a metre; the player is about 1.5)");
                    ui.end_row();
                }
                if params.contains(&Param::Cells) {
                    ui.label(if r.kind == Kind::Rock { "Footprint" } else { "Cells" });
                    ui.horizontal(|ui| {
                        let across = ui.add(egui::DragValue::new(&mut r.cells[0]).range(1..=8)).on_hover_text("Across (x)");
                        mark("uiRequestCellsX", &across);
                        ui.label("×");
                        let deep = ui.add(egui::DragValue::new(&mut r.cells[1]).range(1..=8)).on_hover_text("Deep (z)");
                        mark("uiRequestCellsZ", &deep);
                        hint(ui, match r.kind {
                            Kind::Rock => "cells, about: its height comes from the pictures' proportions",
                            Kind::Grass => "filled to their edges, so copies side by side join without a gap",
                            _ => "the ground it covers, about",
                        });
                    });
                    ui.end_row();
                }
                if params.contains(&Param::Style) {
                ui.label("Style");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut r.style, Style::Smooth, "smooth").on_hover_text("Rounded, smooth shaded");
                    ui.selectable_value(&mut r.style, Style::LowPoly, "low-poly").on_hover_text("Few faces, flat shaded");
                    let accurate = ui.selectable_value(&mut r.style, Style::Accurate, "accurate").on_hover_text(
                        "As close to the pictures as it can be, whatever it takes: Claude uses the fitting tool for each part (fur, textures...) and may take a long time",
                    );
                    mark("uiRequestAccurate", &accurate);
                });
                ui.end_row();
                if r.style == Style::Accurate {
                    ui.label("Model details");
                    let details = ui.add(
                        egui::TextEdit::multiline(&mut r.details).desired_rows(3).desired_width(330.0).hint_text("How to build it, what matters most: the fur, a marking, a detail to get exactly right"),
                    );
                    mark("uiRequestDetails", &details);
                    ui.end_row();
                }
                }
                if params.contains(&Param::Pose) {
                    ui.label("Default pose");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut r.pose, Pose::TPose, "T-pose").on_hover_text("Arms (or front legs) straight out to the sides, legs straight, tail straight back: the easiest to rig and animate");
                        ui.selectable_value(&mut r.pose, Pose::AsIs, "as-is").on_hover_text("The pose the pictures show");
                    });
                    ui.end_row();
                }
                if params.contains(&Param::Shapes) {
                ui.label("Shapes");
                ui.horizontal(|ui| {
                    let meter = ui
                        .add_enabled_ui(!r.shapes.unlimited, |ui| range_meter(ui, &mut r.shapes.min, &mut r.shapes.max, SHAPES_CAP))
                        .inner
                        .on_hover_text("How many shapes (primitives) Claude should build it from: drag either end");
                    mark("uiRequestShapes", &meter);
                    ui.checkbox(&mut r.shapes.unlimited, "unlimited").on_hover_text(format!(
                        "As many as the design needs, up to about {SHAPES_CAP}: Claude works out which parts it's made of first, and may take a long time"
                    ));
                    hint(ui, &if r.shapes.unlimited { format!("up to ~{SHAPES_CAP}") } else { format!("{} to {}", r.shapes.min, r.shapes.max) });
                });
                ui.end_row();
                }
                ui.label("Notes");
                let notes = ui.add(egui::TextEdit::multiline(&mut r.notes).desired_rows(3).desired_width(330.0).hint_text("Colours, what the pictures don't show, what matters most"));
                mark("uiRequestNotes", &notes);
                ui.end_row();
                // grass and environment from the first run on; any other once it's made
                // (Make variants, below)
                if params.contains(&Param::Variants) || r.can_vary() {
                    ui.label("Variants");
                    ui.horizontal(|ui| {
                        let count = ui.add(egui::DragValue::new(&mut r.variants).range(0..=VARIANTS_CAP)).on_hover_text(
                            "How many variants of it to make besides it, each a prop of its own (with Process multiple: of each drawing)",
                        );
                        mark("uiRequestVariants", &count);
                        hint(ui, &match (r.variants, r.several && r.parent.is_empty()) {
                            (0, _) => "none: only it".to_string(),
                            (n, true) => format!("besides each drawing's own: {n} more of each"),
                            (n, false) => format!("besides it: {n} more, {} in all", n + 1),
                        });
                    });
                    ui.end_row();
                    if r.variants > 0 {
                        ui.label("What changes");
                        let vary = ui.add(
                            egui::TextEdit::multiline(&mut r.vary)
                                .desired_rows(2)
                                .desired_width(330.0)
                                .hint_text("How the variants differ (optional): size, colours, how the blades lean, a flower or none"),
                        );
                        mark("uiRequestVary", &vary);
                        ui.end_row();
                    }
                }
                ui.label("");
                let mine = ui.checkbox(&mut r.mine, "This design is mine or licensed").on_hover_text(
                    "Then its model goes into content/, the public game. Off: into temporary/, never committed (a design of someone else's stays a placeholder)",
                );
                mark("uiRequestMine", &mine);
                ui.end_row();
                if params.contains(&Param::Many) {
                    ui.label("");
                    let many = ui.checkbox(&mut r.many, "Expect large rendered count").on_hover_text(
                        "It'll be drawn by the thousand, strewn over fields with the grass and the flowers (meadows, procedural ground): made as light as it can be, the fewest vertices that keep its look, in pieces the engine can leave out further off",
                    );
                    mark("uiRequestMany", &many);
                    ui.end_row();
                }
                // (a set of floor tiles draws no lines: it's seen from above, laid out)
                if params.contains(&Param::CheckLines) {
                    ui.label("");
                    let check = ui.checkbox(&mut r.check_lines, "Let me check the lines first").on_hover_text(
                        "Claude stops once it has drawn its lines over the pictures and the front and the side from them: you look, say what's off, and it goes on to the model (the long part) from there",
                    );
                    mark("uiRequestCheckLines", &check);
                    ui.end_row();
                }
                if r.children.is_empty() && r.parent.is_empty() {
                    ui.label("");
                    let several = ui.checkbox(&mut r.several, "Process multiple").on_hover_text(
                        "The pictures hold several drawings of the same kind of asset (trees, bushes, rocks, plants): Start makes each one a request of its own, with these settings (outline the ones to make, each with its height and cells; none outlined, all of them); Make them all then has them made one after another",
                    );
                    mark("uiRequestSeveral", &several);
                    ui.end_row();
                    if r.several && !r.images.is_empty() {
                        ui.label("");
                        ui.horizontal(|ui| {
                            let open = self.outlines.open && self.outlines.of == r.id;
                            let b = ui.button(if open { "Outlining the drawings…" } else { "Outline the drawings…" }).on_hover_text(
                                "A window with a picture large, to draw a line round each drawing to make, and give each its height and cells: only those are made, the rest left out",
                            );
                            mark("uiRequestOutlines", &b);
                            if b.clicked() {
                                self.outlines.open = !open;
                            }
                            let n = r.outlined().len();
                            hint(ui, &if n == 0 { "none outlined: all of them made".to_string() } else { format!("{n} outlined: only those made") });
                        });
                        ui.end_row();
                    }
                }
                if !r.parent.is_empty() {
                    ui.label("From");
                    ui.horizontal(|ui| {
                        let name = self.list.iter().find(|q| q.id == r.parent).map(|q| q.name.clone()).unwrap_or_else(|| r.parent.clone());
                        ui.label(RichText::new(format!("one of {name}'s drawings")).size(12.0));
                        let up = ui.small_button("Show it").on_hover_text("The request it was split from");
                        mark("uiRequestParent", &up);
                        if up.clicked() {
                            show = Some(r.parent.clone());
                        }
                    });
                    ui.end_row();
                }
                ui.label("Status");
                ui.horizontal(|ui| {
                    chip(ui, &r);
                    if let Some(a) = self.run_button(ui, &r, false) {
                        asked = Some(a);
                    }
                    ui.add_space(6.0);
                    self.model_pick(ui);
                });
                ui.end_row();
            });
            if !r.children.is_empty() {
                // split: its requests, each made by its own run
                section(ui, &format!("Its requests ({})", r.children.len()));
                hint(ui, "One per drawing on its pictures, each with these settings and its own picture, name and height: made one after another, each a request as any other (its log, Make it again).");
                for c in &r.children {
                    let Some(q) = self.list.iter().find(|q| &q.id == c) else { continue };
                    ui.horizontal(|ui| {
                        chip(ui, q);
                        ui.label(RichText::new(format!("{} · {} cells", q.name, q.height)).size(12.0));
                        let open = ui.small_button("Show").on_hover_text("Its own panel: its picture, its log, its model");
                        mark(&format!("uiRequestChild_{c}"), &open);
                        if open.clicked() {
                            show = Some(c.clone());
                        }
                    });
                }
                let left = r.children.iter().filter(|c| self.to_make(c) && self.queue.place(c).is_none()).count();
                let all = ui
                    .add_enabled(left > 0, egui::Button::new(RichText::new(format!("Make them all ({left})")).strong()))
                    .on_hover_text("Each one not made yet goes in the queue, to be made one after another: the next starts once the last has ended")
                    .on_disabled_hover_text("All made, being made or queued (or waiting for your check of their lines)");
                mark("uiRequestMakeAll", &all);
                if all.clicked() {
                    asked = Some(Run::MakeAll(r.id.clone()));
                }
                let queued = r.children.iter().filter(|c| self.queue.place(c).is_some()).count();
                if queued > 0 {
                    hint(ui, &format!("{queued} of them queued"));
                }
            }
            if r.status == Status::InProgress {
                section(ui, "Being modelled");
                let shown = bar(ui, &r, 330.0, 18.0);
                mark("uiRequestBar", &shown);
            } else if r.status == Status::Failed {
                warn(ui, &r.progress.step);
                if r.progress.step.contains("signed in") || r.progress.step.contains("logged in") {
                    let login = ui.button("Claude sign-in…").on_hover_text("Claude Code's own sign-in (a Claude subscription, or a Console account billed by the API), then Start again");
                    mark("uiRequestLogin", &login);
                    if login.clicked() {
                        self.sign_in = true;
                    }
                }
                hint(ui, "The console has its log (Copy takes it all); Start again makes it once more.");
            } else if r.status == Status::Done && r.progress.minutes > 0.0 {
                hint(ui, &format!("Modelled in {} min", r.progress.minutes.round()));
            }
            section(ui, &format!("Pictures ({})", r.images.len()));
            hint(ui, "Pictures dropped on the window go here (PNG, JPG, WebP): front, side, back, close-ups.");
            ui.horizontal_wrapped(|ui| {
                for (i, f) in r.images.clone().into_iter().enumerate() {
                    ui.vertical(|ui| {
                        let path = self.picture_path(&r.id, &f);
                        let shown = self.picture(ui, &path, 96.0, Sense::click()).on_hover_text(format!("{f}: click to open it"));
                        mark(&format!("uiRequestPicture_{i}"), &shown);
                        if shown.clicked() {
                            said = self.open_picture(&path);
                        }
                        if ui.small_button("× take out").on_hover_text("Its copy in the request's folder goes too").clicked() {
                            remove = Some(f.clone());
                        }
                    });
                }
            });
            if r.images.is_empty() {
                hint(ui, "Drop pictures on the window to add them.");
            }
            if params.contains(&Param::Bones) && !r.images.is_empty() {
                ui.horizontal(|ui| {
                    let open = self.bones.open && self.bones.of == r.id;
                    let b = ui.button(if open { "Drawing the bones…" } else { "Draw the bones…" }).on_hover_text(
                        "A window with a picture large, to draw the body's bones over it, a colour per part: Claude reads them when it draws its own lines, and rigs the creature on them. Best drawn before Start",
                    );
                    mark("uiRequestBones", &b);
                    if b.clicked() {
                        self.bones.open = !open;
                    }
                    let n: usize = r.bones.values().map(Vec::len).sum();
                    hint(ui, &if n == 0 { "no bones drawn yet".to_string() } else { format!("{n} bone line{} drawn", if n == 1 { "" } else { "s" }) });
                });
            }
            if !drawn.is_empty() && !r.lines_ready() {
                section(ui, "Lines");
                hint(ui, "Claude's lines over the pictures and the front and side it made from them; once rendered, those lines over the model, with how far they overlap. Click one to see it full size.");
                if let Some(s) = self.drawings(ui, &r.id, &drawn) {
                    said = Some(s);
                }
            }
            if !r.previews.is_empty() {
                section(ui, "The model");
                ui.horizontal_wrapped(|ui| {
                    for f in r.previews.clone() {
                        let path = self.picture_path(&r.id, &f);
                        let shown = self.picture(ui, &path, 130.0, Sense::click()).on_hover_text(format!("{f}: click to open it"));
                        if shown.clicked() {
                            said = self.open_picture(&path);
                        }
                    }
                });
            }
            // made: how close it came, for the next ones of its kind
            if r.status == Status::Done && !r.prop.is_empty() {
                section(ui, "Rating");
                ui.horizontal(|ui| {
                    let mut v = r.rating.unwrap_or(50) as f64;
                    let s = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).step_by(1.0).show_value(r.rating.is_some())).on_hover_text("0: nothing like it; 100: exactly as designed, as its description says");
                    mark("uiRequestRating", &s);
                    // (a click where the slider already stands rates it too)
                    if s.changed() || s.clicked() || s.drag_stopped() {
                        r.rating = Some(v.round() as u8);
                    }
                    if r.rating.is_some() {
                        let clear = ui.small_button("×").on_hover_text("Not rated");
                        mark("uiRequestRatingClear", &clear);
                        if clear.clicked() {
                            r.rating = None;
                        }
                    } else {
                        hint(ui, "not rated yet: drag to rate it");
                    }
                });
                hint(ui, &format!(
                    "0: nothing like it; 100: exactly as designed. The {}s rated best ({} or more) and worst ({} or less), {} of each, teach the next {} models: their style carried on, what went wrong left out.",
                    r.kind.name(),
                    super::BEST,
                    super::WORST,
                    super::SHOWN,
                    r.kind.name()
                ));
            }
            // a set of floor tiles: its tiles (the request's prop its first)
            let set = if r.kind == Kind::Floor { world::floors::set_of(content, &r.prop) } else { None };
            if let Some(set) = set {
                let tiles = content.floor_sets().remove(set).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label("Became the set");
                    ui.label(RichText::new(set).monospace());
                    hint(ui, &format!("{} tile{}", tiles.len(), if tiles.len() == 1 { "" } else { "s" }));
                    if let Some(e) = content.props.entry(&r.prop) {
                        badge(ui, e.space);
                        if e.space == Space::Content && !r.mine {
                            warn(ui, "in content/, but the design isn't marked as yours");
                        }
                    }
                    let pick = ui.button("Use it").on_hover_text("The Ground tool's Floors tab lays it out of doors; in a building, the Build tool's Flooring");
                    mark("uiRequestUse", &pick);
                    if pick.clicked() {
                        use_prop = Some(r.prop.clone());
                    }
                });
                ui.collapsing(format!("Its tiles ({})", tiles.len()), |ui| {
                    for p in &tiles {
                        let Some(f) = &p.floor else { continue };
                        let weight = if f.weight != 1.0 { format!(", weight {}", f.weight) } else { String::new() };
                        ui.label(RichText::new(format!("{}: {} (north east south west){weight}{}", p.id, f.edges.join(" "), if f.turns { ", turns" } else { "" })).size(11.5));
                    }
                })
                .header_response
                .on_hover_text("Each tile and its edges' words: two tiles side by side fit where the edge between them is the same word on both");
            } else if !r.prop.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Became the prop");
                    ui.label(RichText::new(&r.prop).monospace());
                    match content.props.entry(&r.prop) {
                        Some(e) => {
                            badge(ui, e.space);
                            if e.space == Space::Content && !r.mine {
                                warn(ui, "in content/, but the design isn't marked as yours");
                            }
                            let pick = ui.button("Use it").on_hover_text("The Place tool puts it down");
                            mark("uiRequestUse", &pick);
                            if pick.clicked() {
                                use_prop = Some(r.prop.clone());
                            }
                        }
                        None => hint(ui, "(not in the project)"),
                    }
                });
            }
            // the variants it was made into, each to put down
            if set.is_none() && !r.variant_props.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Its variants");
                    for v in r.variant_props.clone() {
                        let known = content.props.entry(&v).is_some();
                        let pick = ui.add_enabled(known, egui::Button::new(RichText::new(&v).monospace().size(11.5))).on_hover_text("The Place tool puts it down").on_disabled_hover_text("Not in the project");
                        mark(&format!("uiRequestUseVariant_{v}"), &pick);
                        if pick.clicked() {
                            use_prop = Some(v.clone());
                        }
                    }
                });
            }
            if !r.reply.is_empty() && !r.lines_ready() {
                section(ui, "What Claude says");
                ui.label(RichText::new(&r.reply).size(12.0));
            }
            // made once: what to look for and change in the next run, which starts from it
            if !r.prop.is_empty() && r.status != Status::InProgress && !r.lines_ready() {
                section(ui, "Make it again");
                hint(ui, "What to look for and what to change: the next run starts from this one (its model.py, its lines, what it said) and changes that, keeping the rest.");
                if !r.earlier.is_empty() {
                    ui.collapsing(format!("Told before ({})", r.earlier.len()), |ui| {
                        for (k, c) in r.earlier.iter().enumerate() {
                            ui.label(RichText::new(format!("{}. {c}", k + 1)).size(11.5));
                        }
                    })
                    .header_response
                    .on_hover_text("The comments the runs that made it again were given: the next run keeps those changes");
                }
                let field = ui.add(
                    egui::TextEdit::multiline(&mut r.comments)
                        .desired_rows(3)
                        .desired_width(420.0)
                        .hint_text("The trunk thicker at its foot, fewer branches low down, the leaves a lighter green"),
                );
                mark("uiRequestComments", &field);
                let busy = self.busy();
                let mine = busy.as_deref() == Some(r.id.as_str());
                let other = busy.filter(|_| !mine);
                let said_what = !r.comments.trim().is_empty();
                let queued = self.queue.place(&r.id).is_some();
                let label = if other.is_some() { "Queue it to make again with these comments" } else { "Make it again with these comments" };
                let about = match &other {
                    Some(b) => format!("{b} is being made: {MAKER} makes this one again on its own once it (and the queue before it) has ended"),
                    None => format!("{MAKER} starts from the model the last run made and changes what you say, keeping the rest"),
                };
                let again = ui
                    .add_enabled(said_what && !queued && !mine, egui::Button::new(RichText::new(label).strong()))
                    .on_hover_text(about)
                    .on_disabled_hover_text(if mine {
                        "Being made now"
                    } else if queued {
                        "In the queue already"
                    } else {
                        "Say what to look for or change first"
                    });
                mark("uiRequestAgain", &again);
                if again.clicked() {
                    asked = Some(if other.is_some() { Run::Queue(r.id.clone(), How::Again) } else { Run::Again(r.id.clone()) });
                }
            }
            // made: variants of it, as many as its Variants says, each a prop of its own
            if r.can_vary() {
                section(ui, "Make variants");
                hint(ui, "From the model made, as many variants as Variants above says, each a prop of its own, as What changes says (else as Claude sees fit); the style, the notes and the rest as they are, the model itself kept.");
                let busy = self.busy();
                let mine = busy.as_deref() == Some(r.id.as_str());
                let other = busy.filter(|_| !mine);
                let queued = self.queue.place(&r.id).is_some();
                let n = r.variants;
                let label = match (&other, n) {
                    (_, 0) => "Make variants".to_string(),
                    (Some(_), _) => format!("Queue it to make {n} variant{}", if n == 1 { "" } else { "s" }),
                    (None, _) => format!("Make {n} variant{}", if n == 1 { "" } else { "s" }),
                };
                let about = match &other {
                    Some(b) => format!("{b} is being made: {MAKER} makes these on its own once it (and the queue before it) has ended"),
                    None => format!("{MAKER} starts from the model made and makes its variants, keeping the model itself as it is"),
                };
                let vary = ui
                    .add_enabled(n > 0 && !queued && !mine, egui::Button::new(RichText::new(label).strong()))
                    .on_hover_text(about)
                    .on_disabled_hover_text(if mine {
                        "Being made now"
                    } else if queued {
                        "In the queue already"
                    } else {
                        "Say how many in Variants above first"
                    });
                mark("uiRequestMakeVariants", &vary);
                if vary.clicked() {
                    asked = Some(if other.is_some() { Run::Queue(r.id.clone(), How::Variants) } else { Run::Variants(r.id.clone()) });
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (open, about) = if cfg!(target_arch = "wasm32") { ("Download the folder", "Its files as a zip") } else { ("Open the folder", "private/requests/<id>/ in the file manager") };
                if ui.button(open).on_hover_text(about).clicked() {
                    if let Err(e) = reveal(&folder(&self.root()).join(&r.id)) {
                        said = Some(e);
                    }
                }
                if self.deleting {
                    warn(ui, "Delete the request, its pictures and renders?");
                    if ui.button("Delete").clicked() {
                        let root = self.root();
                        said = Some(match delete(&root, &r.id) {
                            Ok(()) => format!("Deleted the request {}{}", r.name, if r.prop.is_empty() { String::new() } else { format!(" (the prop {} stays)", r.prop) }),
                            Err(e) => e,
                        });
                        self.unqueue(&r.id);
                        self.deleting = false;
                        self.selected = None;
                        self.reload();
                    }
                    if ui.button("Keep it").clicked() {
                        self.deleting = false;
                    }
                } else if ui.button("Delete…").clicked() {
                    self.deleting = true;
                }
            });
            if self.selected.as_deref() != Some(before.id.as_str()) {
                return (use_prop, said, asked);
            }
            if let Some(f) = remove {
                if let Err(e) = remove_picture(&self.root(), &mut r, &f) {
                    said = Some(e);
                }
                self.thumbs.forget(&self.picture_path(&r.id, &f));
            }
            if r != before {
                if let Err(e) = self.keep(&before, r) {
                    said = Some(e);
                }
            }
            if let Some(id) = show {
                self.selected = Some(id);
                self.deleting = false;
            }
            (use_prop, said, asked)
        }

        // ------------------------------------------------------------ the Outlines window

        /// The Outlines window, while it's open: the selected request's picture, large, with the
        /// outlines the user draws round the drawings to make (Process multiple, before it's
        /// split). What changed is written at once.
        pub(in super::super) fn outlines_window(&mut self, ctx: &egui::Context) -> Option<String> {
            if !self.outlines.open {
                return None;
            }
            let Some(before) = self.selected.as_ref().and_then(|id| self.list.iter().find(|r| &r.id == id && r.several && r.parent.is_empty() && r.children.is_empty())).cloned() else {
                self.outlines.open = false;
                return None;
            };
            if self.outlines.of != before.id {
                self.outlines = Outlining { open: true, of: before.id.clone(), ..Default::default() };
            }
            let mut r = before.clone();
            let mut open = true;
            let screen = ctx.content_rect();
            // the picture as large as the screen leaves room for
            let room = egui::vec2((screen.width() - 400.0).clamp(240.0, 900.0), (screen.height() - 330.0).clamp(200.0, 640.0));
            let mut said = None;
            egui::Window::new(format!("Outlines: {}", r.name)).id(egui::Id::new("request-outlines")).open(&mut open).collapsible(false).resizable(false).default_pos([300.0, 70.0]).show(ctx, |ui| {
                said = self.outlines_pane(ui, &mut r, room);
            });
            self.outlines.open = open;
            if r != before {
                if let Err(e) = self.keep(&before, r) {
                    said = Some(e);
                }
            }
            said
        }

        /// Inside the Outlines window: which picture, the canvas (a click starts an outline and
        /// adds each point, a right-click closes it there, Escape drops it, Backspace takes the
        /// last point back), and the outlines drawn, each numbered with its height, its cells
        /// and its ×.
        fn outlines_pane(&mut self, ui: &mut egui::Ui, r: &mut ModelRequest, room: egui::Vec2) -> Option<String> {
            if r.images.is_empty() {
                hint(ui, "Drop pictures on the Requests window first.");
                return None;
            }
            let picture = match self.outlines.picture.clone().filter(|p| r.images.contains(p)) {
                Some(p) => p,
                None => r.images[0].clone(),
            };
            if self.outlines.picture.as_deref() != Some(picture.as_str()) {
                self.outlines.picture = Some(picture.clone());
                self.outlines.drawing.clear();
            }
            let path = self.picture_path(&r.id, &picture);
            let texture = self.thumbs.at(ui.ctx(), &path, LARGE);
            let fitted = |size: egui::Vec2| {
                let k = (room.x / size.x.max(1.0)).min(room.y / size.y.max(1.0)).min(6.0);
                egui::vec2((size.x * k).max(120.0), (size.y * k).max(80.0))
            };
            let width = match &texture {
                Some(Ok((_, size))) => fitted(*size).x,
                _ => 480.0,
            };
            ui.set_max_width(width.max(480.0));
            hint(ui, "A line round each drawing to make: a click starts it and adds each point, a right-click closes it there. Only the drawings outlined are made, each a request of its own, as tall and on as many cells as you say; with none outlined, all of them are.");
            // (numbered across the pictures, as the split run numbers them)
            let before: usize = r.images.iter().take_while(|f| **f != picture).map(|f| r.outlines.get(f).map(Vec::len).unwrap_or(0)).sum();
            ui.horizontal_wrapped(|ui| {
                if r.images.len() > 1 {
                    hint(ui, "On");
                    for f in r.images.clone() {
                        let n = r.outlines.get(&f).map(Vec::len).unwrap_or(0);
                        let pick = ui.selectable_label(f == picture, if n > 0 { format!("{f} ({n})") } else { f.clone() });
                        mark(&format!("uiRequestOutlinesOn_{f}"), &pick);
                        if pick.clicked() {
                            self.outlines.picture = Some(f.clone());
                            self.outlines.drawing.clear();
                        }
                    }
                    ui.separator();
                }
                let undo = ui.small_button("Undo").on_hover_text("Takes back the last point, or the last outline (Backspace does too)");
                mark("uiRequestOutlinesUndo", &undo);
                if undo.clicked() && self.outlines.drawing.pop().is_none() {
                    if let Some(list) = r.outlines.get_mut(&picture) {
                        list.pop();
                    }
                }
                let clear = ui.small_button("Clear").on_hover_text("Every outline on this picture, gone");
                mark("uiRequestOutlinesClear", &clear);
                if clear.clicked() {
                    r.outlines.remove(&picture);
                    self.outlines.drawing.clear();
                }
            });
            let (id, size) = match texture {
                Some(Ok(t)) => t,
                Some(Err(e)) => {
                    warn(ui, &e);
                    return None;
                }
                None => {
                    hint(ui, "Reading the picture…");
                    return None;
                }
            };
            let (rect, resp) = ui.allocate_exact_size(fitted(size), Sense::click());
            platform::set_hook("requestoutlines", &format!("{:.0},{:.0},{:.0},{:.0}", rect.left(), rect.top(), rect.width(), rect.height()));
            let p = ui.painter_at(rect);
            p.image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Outside);
            let hover = resp.hover_pos();
            if let Some(at) = hover.map(|h| on_picture(rect, size, h)) {
                if resp.clicked() {
                    add_joint(&mut self.outlines.drawing, at);
                }
                if resp.secondary_clicked() {
                    add_joint(&mut self.outlines.drawing, at);
                    let points = std::mem::take(&mut self.outlines.drawing);
                    if points.len() >= 3 {
                        let cells = if matches!(r.kind, Kind::Grass | Kind::Rock) { r.cells } else { [1, 1] };
                        r.outlines.entry(picture.clone()).or_default().push(Outline { points, height: r.height, cells });
                    }
                }
            }
            // while an outline is drawn these keys are the window's (the editor's own stand aside:
            // MapEditor::keys), unless a number is being typed
            if !self.outlines.drawing.is_empty() && !ui.ctx().egui_wants_keyboard_input() {
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.outlines.drawing.clear();
                } else if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    self.outlines.drawing.pop();
                }
            }
            let drawn = r.outlines.get(&picture).cloned().unwrap_or_default();
            draw_outlines(&p, rect, size, &drawn, before, &self.outlines.drawing, hover);
            resp.on_hover_cursor(egui::CursorIcon::Crosshair);
            let points = self.outlines.drawing.len();
            hint(
                ui,
                &if points == 0 {
                    format!("{} outline{} on {picture} · click to start one round a drawing", drawn.len(), if drawn.len() == 1 { "" } else { "s" })
                } else {
                    format!("an outline, {points} point{} so far · click for the next, right-click to close it (Escape drops it)", if points == 1 { "" } else { "s" })
                },
            );
            // the outlines drawn: each one's height and cells, its × taking it out
            let mut gone = None;
            if let Some(list) = r.outlines.get_mut(&picture) {
                for (i, o) in list.iter_mut().enumerate() {
                    let k = before + i + 1;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{k}.")).color(tint(k - 1)).strong());
                        hint(ui, "height");
                        let h = ui.add(egui::DragValue::new(&mut o.height).speed(0.05).range(0.1..=40.0).suffix(" cells").max_decimals(2)).on_hover_text("How tall it stands");
                        mark(&format!("uiRequestOutlineHeight_{k}"), &h);
                        hint(ui, "on");
                        let x = ui.add(egui::DragValue::new(&mut o.cells[0]).range(1..=8)).on_hover_text("The cells it covers across (x)");
                        mark(&format!("uiRequestOutlineCellsX_{k}"), &x);
                        ui.label("×");
                        let z = ui.add(egui::DragValue::new(&mut o.cells[1]).range(1..=8)).on_hover_text("The cells it covers deep (z)");
                        mark(&format!("uiRequestOutlineCellsZ_{k}"), &z);
                        hint(ui, "cells");
                        let out = ui.small_button("×").on_hover_text("Takes this outline out");
                        mark(&format!("uiRequestOutlineOut_{k}"), &out);
                        if out.clicked() {
                            gone = Some(i);
                        }
                    });
                }
                if let Some(i) = gone {
                    list.remove(i);
                }
            }
            if r.outlines.get(&picture).is_some_and(Vec::is_empty) {
                r.outlines.remove(&picture);
            }
            None
        }

        // ------------------------------------------------------------ the Bones window

        /// The Bones window, while it's open: the selected request's picture, large, with the
        /// bone lines the user draws over it. What changed is written at once.
        pub(in super::super) fn bones_window(&mut self, ctx: &egui::Context) -> Option<String> {
            if !self.bones.open {
                return None;
            }
            // a creature's (a prop has no bones)
            let Some(before) = self.selected.as_ref().and_then(|id| self.list.iter().find(|r| &r.id == id && r.kind == Kind::Character)).cloned() else {
                self.bones.open = false;
                return None;
            };
            if self.bones.of != before.id {
                self.bones = Sketch { open: true, of: before.id.clone(), ..Default::default() };
            }
            let mut r = before.clone();
            let mut open = true;
            let screen = ctx.content_rect();
            // the picture as large as the screen leaves room for
            let room = egui::vec2((screen.width() - 400.0).clamp(240.0, 900.0), (screen.height() - 330.0).clamp(200.0, 640.0));
            let mut said = None;
            egui::Window::new(format!("Bones: {}", r.name)).id(egui::Id::new("request-bones")).open(&mut open).collapsible(false).resizable(false).default_pos([300.0, 70.0]).show(ctx, |ui| {
                said = self.bones_pane(ui, &mut r, room);
            });
            self.bones.open = open;
            if r != before {
                if let Err(e) = self.keep(&before, r) {
                    said = Some(e);
                }
            }
            said
        }

        /// Inside the Bones window: which picture, which part, the canvas (a click starts a
        /// line and adds each joint, a right-click ends it there, Escape drops it, Backspace
        /// takes the last joint back), and the lines drawn, each with its name and its ×.
        fn bones_pane(&mut self, ui: &mut egui::Ui, r: &mut ModelRequest, room: egui::Vec2) -> Option<String> {
            if r.images.is_empty() {
                hint(ui, "Drop pictures on the Requests window first.");
                return None;
            }
            let picture = match self.bones.picture.clone().filter(|p| r.images.contains(p)) {
                Some(p) => p,
                None => r.images[0].clone(),
            };
            if self.bones.picture.as_deref() != Some(picture.as_str()) {
                self.bones.picture = Some(picture.clone());
                self.bones.drawing.clear();
            }
            if self.bones.part.is_empty() {
                self.bones.part = BONE_PARTS[0].0.to_string();
            }
            let path = self.picture_path(&r.id, &picture);
            let texture = self.thumbs.at(ui.ctx(), &path, LARGE);
            // the picture fitted into the room (a small one at most six times its size); the
            // window as wide as it, its rows wrapping under it
            let fitted = |size: egui::Vec2| {
                let k = (room.x / size.x.max(1.0)).min(room.y / size.y.max(1.0)).min(6.0);
                egui::vec2((size.x * k).max(120.0), (size.y * k).max(80.0))
            };
            let width = match &texture {
                Some(Ok((_, size))) => fitted(*size).x,
                _ => 480.0,
            };
            ui.set_max_width(width.max(480.0));
            hint(ui, "The body's bones over the picture, a colour per part: a click starts a line and adds each joint (hip, knee, foot...), a right-click ends it there.");
            hint(ui, "Claude reads them when it draws its own lines (which part of the drawing each is on) and rigs the creature on them.");
            if r.images.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    hint(ui, "On");
                    for f in r.images.clone() {
                        let n = r.bones.get(&f).map(Vec::len).unwrap_or(0);
                        let pick = ui.selectable_label(f == picture, if n > 0 { format!("{f} ({n})") } else { f.clone() });
                        mark(&format!("uiRequestBonesOn_{f}"), &pick);
                        if pick.clicked() {
                            self.bones.picture = Some(f.clone());
                            self.bones.drawing.clear();
                        }
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                for (part, c) in BONE_PARTS {
                    let colour = Color32::from_rgb(c[0], c[1], c[2]);
                    let b = ui.selectable_label(self.bones.part == part, RichText::new(part).color(colour).strong()).on_hover_text(if part == "misc" {
                        "Any piece that has no name here (a crest, a shell, a fin...): give it a name of your own"
                    } else {
                        "The next line is this part"
                    });
                    mark(&format!("uiRequestBone_{part}"), &b);
                    if b.clicked() {
                        self.bones.part = part.to_string();
                    }
                }
                // always there (greyed but for misc), so the picture never moves as parts change
                let misc = self.bones.part == "misc";
                let name = ui
                    .add_enabled(misc, egui::TextEdit::singleline(&mut self.bones.name).desired_width(110.0).hint_text("misc: named…"))
                    .on_hover_text("What the piece is: the next misc line gets this name")
                    .on_disabled_hover_text("Pick misc to draw a piece with no part of its own here, named as you like");
                mark("uiRequestBoneName", &name);
                ui.separator();
                let undo = ui.small_button("Undo").on_hover_text("Takes back the last joint, or the last line (Backspace does too)");
                mark("uiRequestBonesUndo", &undo);
                if undo.clicked() && self.bones.drawing.pop().is_none() {
                    if let Some(list) = r.bones.get_mut(&picture) {
                        list.pop();
                    }
                }
                let clear = ui.small_button("Clear").on_hover_text("Every line on this picture, gone");
                mark("uiRequestBonesClear", &clear);
                if clear.clicked() {
                    r.bones.remove(&picture);
                    self.bones.drawing.clear();
                }
            });
            let (id, size) = match texture {
                Some(Ok(t)) => t,
                Some(Err(e)) => {
                    warn(ui, &e);
                    return None;
                }
                None => {
                    hint(ui, "Reading the picture…");
                    return None;
                }
            };
            let (rect, resp) = ui.allocate_exact_size(fitted(size), Sense::click());
            platform::set_hook("requestbones", &format!("{:.0},{:.0},{:.0},{:.0}", rect.left(), rect.top(), rect.width(), rect.height()));
            let p = ui.painter_at(rect);
            p.image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Outside);
            let hover = resp.hover_pos();
            if let Some(at) = hover.map(|h| on_picture(rect, size, h)) {
                if resp.clicked() {
                    add_joint(&mut self.bones.drawing, at);
                }
                if resp.secondary_clicked() {
                    add_joint(&mut self.bones.drawing, at);
                    let points = std::mem::take(&mut self.bones.drawing);
                    if points.len() >= 2 {
                        let name = if self.bones.part == "misc" { self.bones.name.trim().to_string() } else { String::new() };
                        r.bones.entry(picture.clone()).or_default().push(Bone { part: self.bones.part.clone(), name, points });
                    }
                }
            }
            // while a line is drawn these keys are the window's (the editor's own stand aside:
            // MapEditor::keys), unless a name is being typed
            if !self.bones.drawing.is_empty() && !ui.ctx().egui_wants_keyboard_input() {
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.bones.drawing.clear();
                } else if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    self.bones.drawing.pop();
                }
            }
            let drawn = r.bones.get(&picture).cloned().unwrap_or_default();
            draw_bones(&p, rect, size, &drawn, &self.bones, hover);
            resp.on_hover_cursor(egui::CursorIcon::Crosshair);
            let joints = self.bones.drawing.len();
            hint(
                ui,
                &if joints == 0 {
                    format!("{} line{} on {picture} · click to start a {} line", drawn.len(), if drawn.len() == 1 { "" } else { "s" }, self.bones.part)
                } else {
                    format!("a {} line, {joints} joint{} so far · click for the next joint, right-click for the last (Escape drops the line)", self.bones.part, if joints == 1 { "" } else { "s" })
                },
            );
            // the lines drawn, each named (a misc line's name to type), its × taking it out
            let names = bone_names(&drawn);
            let mut gone = None;
            // in rows as wide as the picture, each line whole (its number, its name, its ×)
            let font = egui::TextStyle::Body.resolve(ui.style());
            let wide = |text: String| ui.painter().layout_no_wrap(text, font.clone(), Color32::WHITE).size().x;
            let mut rows: Vec<Vec<usize>> = vec![vec![]];
            let mut used = 0.0;
            for (i, (b, name)) in drawn.iter().zip(&names).enumerate() {
                let w = wide(format!("{}.", i + 1)) + if b.part == "misc" { 90.0 } else { wide(name.clone()) } + 40.0;
                if used + w > width && !rows.last().is_some_and(Vec::is_empty) {
                    rows.push(vec![]);
                    used = 0.0;
                }
                rows.last_mut().unwrap().push(i);
                used += w;
            }
            for row in rows.iter().filter(|row| !row.is_empty()) {
                ui.horizontal(|ui| {
                    for &i in row {
                        let (b, name) = (&drawn[i], &names[i]);
                        let c = bone_colour(&b.part);
                        let colour = Color32::from_rgb(c[0], c[1], c[2]);
                        let joints = format!("{} joint{}", b.points.len(), if b.points.len() == 1 { "" } else { "s" });
                        ui.label(RichText::new(format!("{}.", i + 1)).color(colour));
                        if b.part == "misc" {
                            if let Some(line) = r.bones.get_mut(&picture).and_then(|l| l.get_mut(i)) {
                                let field = ui
                                    .add(egui::TextEdit::singleline(&mut line.name).desired_width(90.0).hint_text("misc: named…").text_color(colour))
                                    .on_hover_text(format!("A misc piece ({joints}): what it is, in your words"));
                                mark(&format!("uiRequestBoneName_{i}"), &field);
                            }
                        } else {
                            ui.label(RichText::new(name).color(colour)).on_hover_text(format!("A {} line, {joints}", b.part));
                        }
                        let x = ui.small_button("×").on_hover_text(format!("Takes the {name} line out"));
                        mark(&format!("uiRequestBoneOut_{i}"), &x);
                        if x.clicked() {
                            gone = Some(i);
                        }
                        ui.add_space(8.0);
                    }
                });
            }
            if let Some(i) = gone {
                if let Some(list) = r.bones.get_mut(&picture) {
                    if i < list.len() {
                        list.remove(i);
                    }
                }
            }
            if r.bones.get(&picture).is_some_and(Vec::is_empty) {
                r.bones.remove(&picture);
            }
            None
        }
    }

    /// What it is, in a line: "creature · 1.2 cells · smooth · mine · lines checked first";
    /// grass, the cells it fills and how tall; its variants, how many.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn about(r: &ModelRequest) -> String {
        about_as(r, r.kind.name())
    }

    /// As `about`, its kind named `kind` (the window's: "bush", "pine (tree)").
    pub(super) fn about_as(r: &ModelRequest, kind: &str) -> String {
        let style = match r.style {
            Style::Smooth => "smooth",
            Style::LowPoly => "low-poly",
            Style::Accurate => "accurate",
        };
        let mine = if r.mine { " · mine" } else { "" };
        if r.kind == Kind::Floor {
            // a cell each, drawn no lines
            return format!("{kind} · {style}{mine}");
        }
        let check = if r.check_lines { " · lines checked first" } else { "" };
        let variants = match r.variants {
            0 => String::new(),
            1 => " · 1 variant".to_string(),
            n => format!(" · {n} variants"),
        };
        let rated = r.rating.map(|v| format!(" · rated {v}")).unwrap_or_default();
        if r.kind == Kind::Grass {
            return format!("{kind} · {} × {} cells, {} tall · {style}{variants}{mine}{check}{rated}", r.cells[0], r.cells[1], r.height);
        }
        if r.kind == Kind::Rock {
            return format!("{kind} · about {} × {} cells · {style}{variants}{mine}{check}{rated}", r.cells[0], r.cells[1]);
        }
        format!("{kind} · {} cells · {style}{variants}{mine}{check}{rated}", r.height)
    }

    /// Where a value sits on the meter's track (0 to 1), from 1 to `top`.
    pub(in super::super) fn meter_place(v: u32, top: u32) -> f32 {
        (v.clamp(1, top) - 1) as f32 / (top.max(2) - 1) as f32
    }

    /// The value at a place on the meter's track (0 to 1).
    pub(in super::super) fn meter_value(t: f32, top: u32) -> u32 {
        (t.clamp(0.0, 1.0) * (top.max(2) - 1) as f32).round() as u32 + 1
    }

    /// A track with two handles, from 1 to `top`: the least and the most. The handle nearer
    /// where the pointer went down follows it; the least never passes the most.
    fn range_meter(ui: &mut egui::Ui, lo: &mut u32, hi: &mut u32, top: u32) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(200.0, 20.0), Sense::click_and_drag());
        let track = rect.shrink2(egui::vec2(8.0, 0.0));
        let x_of = |v: u32| track.left() + meter_place(v, top) * track.width();
        let grabbed = response.id.with("handle");
        if let Some(p) = response.interact_pointer_pos() {
            if response.drag_started() || response.clicked() {
                let high = (p.x - x_of(*hi)).abs() < (p.x - x_of(*lo)).abs() || (*lo == *hi && p.x > x_of(*hi));
                ui.data_mut(|d| d.insert_temp(grabbed, high));
            }
            if response.dragged() || response.clicked() {
                let v = meter_value((p.x - track.left()) / track.width(), top);
                if ui.data(|d| d.get_temp::<bool>(grabbed)).unwrap_or(false) {
                    *hi = v.max(*lo);
                } else {
                    *lo = v.min(*hi);
                }
            }
        }
        let painter = ui.painter();
        let y = rect.center().y;
        let lit = if ui.is_enabled() { WORKING } else { Color32::from_gray(90) };
        painter.line_segment([egui::pos2(track.left(), y), egui::pos2(track.right(), y)], egui::Stroke::new(4.0, Color32::from_gray(55)));
        painter.line_segment([egui::pos2(x_of(*lo), y), egui::pos2(x_of(*hi), y)], egui::Stroke::new(4.0, lit));
        for v in [*lo, *hi] {
            painter.circle(egui::pos2(x_of(v), y), 6.0, ui.style().interact(&response).bg_fill, egui::Stroke::new(1.5, lit));
        }
        response
    }

    /// A request's progress bar, `width` by `height`, with its words on it when it's tall
    /// enough (and always when hovered).
    pub(in super::super) fn bar(ui: &mut egui::Ui, r: &ModelRequest, width: f32, height: f32) -> egui::Response {
        let (fraction, text) = r.progress.at(now_secs());
        let mut bar = egui::ProgressBar::new(fraction).desired_width(width).desired_height(height).fill(WORKING);
        if height >= 14.0 {
            bar = bar.text(RichText::new(&text).size(11.0));
        }
        ui.add(bar).on_hover_text(format!("{}: {text}", r.name))
    }

    /// A word on a colour, its corners a little rounded.
    fn tag(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
        egui::Frame::new()
            .fill(color)
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(4, 1))
            .show(ui, |ui| ui.label(RichText::new(text).size(9.5).strong().color(Color32::BLACK)))
            .response
    }

    /// Its status, as a tag (waiting for the user's check of its lines: LINES READY).
    fn chip(ui: &mut egui::Ui, r: &ModelRequest) {
        let (text, color) = match r.status {
            Status::Waiting if r.lines_ready() => ("LINES READY", WAITING),
            Status::Waiting => ("WAITING", WAITING),
            Status::InProgress => ("IN PROGRESS", WORKING),
            Status::Failed => ("FAILED", FAILED),
            Status::Done => ("DONE", DONE),
        };
        tag(ui, text, color);
    }

    /// Its kind, as a tag, a colour each.
    fn kind_tag(ui: &mut egui::Ui, label: &str, kind: Kind) {
        let color = match kind {
            Kind::Character => Color32::from_rgb(196, 160, 238),
            Kind::Prop => Color32::from_rgb(158, 186, 220),
            Kind::Grass => Color32::from_rgb(150, 210, 128),
            Kind::Flower => Color32::from_rgb(236, 164, 204),
            Kind::Rock => Color32::from_rgb(178, 176, 168),
            Kind::Tree => Color32::from_rgb(116, 178, 104),
            Kind::Environment => Color32::from_rgb(118, 200, 182),
            Kind::Floor => Color32::from_rgb(220, 186, 132),
        };
        tag(ui, label, color);
    }

    /// The user's rating of its model, as a tag: greener the better.
    fn rating_tag(ui: &mut egui::Ui, v: u8) {
        let t = v as f32 / 100.0;
        let color = Color32::from_rgb((236.0 - 96.0 * t) as u8, (150.0 + 70.0 * t) as u8, 120);
        tag(ui, &v.to_string(), color).on_hover_text(format!("Rated {v} of 100"));
    }

    /// A small "i" in a ring, for what's said when it's hovered.
    fn info_icon(ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(15.0, 15.0), Sense::hover());
        let color = if response.hovered() { Color32::from_gray(235) } else { Color32::from_gray(150) };
        let p = ui.painter();
        p.circle_stroke(rect.center(), 6.5, egui::Stroke::new(1.2, color));
        p.text(rect.center() + egui::vec2(0.0, 0.5), egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(10.5), color);
        response
    }

    /// What a request asks for, beyond its name and kind: its height and style (`about`), then
    /// a line each for the rest it says (its cells, shapes, pose, variants, drawings outlined,
    /// lines checked first, where its model goes, its notes), and its folder.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in super::super) fn details(r: &ModelRequest) -> String {
        details_as(r, r.kind.name())
    }

    /// As `details`, its kind named `kind` (the window's).
    pub(in super::super) fn details_as(r: &ModelRequest, kind: &str) -> String {
        let mut out = vec![about_as(r, kind)];
        if r.kind == Kind::Rock {
            out.push(format!("Footprint: about {} × {} cells, as tall as its pictures' proportions make it", r.cells[0], r.cells[1]));
        } else if r.kind != Kind::Floor {
            out.push(format!("Height: {} cells", r.height));
        }
        out.push(format!(
            "Style: {}",
            match r.style {
                Style::Smooth => "smooth",
                Style::LowPoly => "low-poly",
                Style::Accurate => "accurate",
            }
        ));
        if r.kind != Kind::Rock && (r.kind == Kind::Grass || r.cells != [1, 1]) {
            out.push(format!("Cells: {} × {}", r.cells[0], r.cells[1]));
        }
        out.push(if r.shapes.unlimited { format!("Shapes: as many as it needs, up to about {SHAPES_CAP}") } else { format!("Shapes: {} to {}", r.shapes.min, r.shapes.max) });
        if r.kind == Kind::Character {
            out.push(format!("Pose: {}", if r.pose == Pose::TPose { "T-pose" } else { "as the pictures show" }));
        }
        if r.variants > 0 {
            let how = if r.vary.trim().is_empty() { String::new() } else { format!(": {}", r.vary.trim()) };
            out.push(format!("Variants: {}{how}", r.variants));
        }
        if r.several && r.parent.is_empty() {
            let n = r.outlined().len();
            out.push(if n == 0 { "Process multiple: every drawing made".to_string() } else { format!("Process multiple: {n} drawing{} outlined", if n == 1 { "" } else { "s" }) });
        }
        if r.check_lines {
            out.push("Its lines checked first".to_string());
        }
        if r.many && r.kind.strewn() {
            out.push("Drawn by the thousand: made as light as it can be".to_string());
        }
        if let Some(v) = r.rating {
            out.push(format!("Rated {v} of 100"));
        }
        out.push(if r.mine { "Mine or licensed: made into content/".to_string() } else { "A placeholder: made into temporary/".to_string() });
        let notes = r.notes.trim();
        if !notes.is_empty() {
            let short: String = notes.chars().take(140).collect();
            out.push(format!("Notes: {short}{}", if notes.chars().count() > 140 { "…" } else { "" }));
        }
        out.push(format!("Folder: private/requests/{}", r.id));
        out.join("\n")
    }

    /// A small box on a card, its title over what's in it.
    fn boxed(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new().fill(Color32::from_gray(20)).stroke(egui::Stroke::new(1.0, Color32::from_gray(44))).corner_radius(4.0).inner_margin(5.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(10.5).color(crate::hud::MUTED));
            add(ui);
        });
    }

    /// The models a request made: its prop with its own render (three-quarter, else the
    /// first), then each variant's prop with its render (`variant_k.png`); none for its line
    /// work.
    pub(in super::super) fn made_models(r: &ModelRequest) -> Vec<(String, Option<String>)> {
        if r.status != Status::Done || r.prop.is_empty() {
            return vec![];
        }
        let own = r.previews.iter().find(|f| f.as_str() == "preview_three_quarter.png").or_else(|| r.previews.iter().find(|f| f.starts_with("preview_"))).cloned();
        let mut out = vec![(r.prop.clone(), own)];
        for (k, prop) in r.variant_props.iter().enumerate() {
            let file = format!("variant_{}.png", k + 2);
            out.push((prop.clone(), r.previews.contains(&file).then_some(file)));
        }
        out
    }

    /// A card's foot, folded or not: how far its making is, in its status's colour (waiting,
    /// nothing yet; being made, its progress; made, all of it; failed, where it stopped), its
    /// words when hovered.
    fn foot(ui: &mut egui::Ui, r: &ModelRequest, starting: bool) {
        let (at, words) = r.progress.at(now_secs());
        let (fraction, color, text) = match r.status {
            _ if starting => (0.02, WORKING, format!("starting {MAKER}")),
            Status::InProgress => (at, WORKING, words),
            Status::Done => (1.0, DONE, "made".to_string()),
            Status::Failed => (r.progress.done.clamp(0.02, 1.0) as f32, FAILED, r.progress.step.clone()),
            Status::Waiting if r.lines_ready() => (r.progress.done as f32, WAITING, LINES_READY.to_string()),
            Status::Waiting => (0.0, WAITING, "waiting to be made".to_string()),
        };
        ui.add_space(3.0);
        // (painted: egui's bar is never shorter than it's high, a dot where nothing's done);
        // what its runs have cost so far, small, at the bar's end
        let cost = costs_text(r);
        let cost_w = if cost.is_empty() { 0.0 } else { 44.0 };
        let (rect, bar) = ui.allocate_exact_size(egui::vec2((ui.available_width() - cost_w).max(20.0), 4.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, Color32::from_gray(48));
        if fraction > 0.0 {
            p.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height())), 2.0, color);
        }
        if !cost.is_empty() {
            p.text(rect.right_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &cost, egui::FontId::proportional(10.5), Color32::from_gray(170));
            crate::platform::set_hook(&format!("requestcost_{}", r.id), &cost);
        }
        mark(&format!("uiRequestFoot_{}", r.id), &bar.on_hover_text(format!("{}: {text}", r.name)));
    }

    /// What a request's runs have cost in all ("$2.12"; nothing before a run has ended).
    fn costs_text(r: &ModelRequest) -> String {
        if r.costs.is_empty() {
            String::new()
        } else {
            format!("${:.2}", r.costs.iter().sum::<f64>())
        }
    }

    /// A run asked for from the window.
    pub(in super::super) enum Run {
        Start(String),
        /// "The lines look right: go on": from the model, on the lines the user checked
        GoOn(String),
        /// "Make it again with these comments": from the model the last run made, changed as
        /// the user's comments say
        Again(String),
        /// "Make variants": from the model made, as many variants of it as the request says,
        /// each a prop of its own
        Variants(String),
        /// "Make them all": the requests a request was split into, one after another
        MakeAll(String),
        Cancel(String),
        /// put in the queue (a run asked for while another is being made, or "Queue")
        Queue(String, How),
        /// out of the queue, or a place up (-1) or down (1) in it
        Unqueue(String),
        QueueMove(String, i32),
        /// every request waiting to be made, queued
        QueueWaiting,
        QueueClear,
        /// the queue paused, or going on
        QueuePause(bool),
    }

    /// How a run goes: the whole recipe from the pictures (Start), on from the model on the
    /// lines the user checked (GoOn), the last run's model made again with the user's comments
    /// on it (Again), or variants of the model made (Variants).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub(in super::super) enum How {
        #[default]
        Whole,
        GoOn,
        Again,
        Variants,
    }

    impl Requests {
        /// The window's requests in a project of their own (the tests).
        #[cfg(test)]
        pub(in super::super) fn at(root: &Path) -> Requests {
            Requests { desk: Desk { root: Some(root.to_path_buf()), ..Default::default() }, ..Default::default() }
        }

        #[cfg(test)]
        pub(in super::super) fn desk(&mut self) -> &mut Desk {
            &mut self.desk
        }

        /// Pictures dropped while the window is open.
        pub fn take(&mut self, files: Vec<egui::DroppedFileHandle>) {
            platform::read_dropped(files, &self.desk.dropped);
        }

        /// How many wait to be modelled.
        pub fn waiting(&self) -> usize {
            self.desk.list.iter().filter(|r| r.status == Status::Waiting).count()
        }

        /// A line being drawn in the Bones window or the Outlines window: Escape and Backspace
        /// are its.
        pub fn drawing_on_picture(&self) -> bool {
            self.open && ((self.desk.bones.open && !self.desk.bones.drawing.is_empty()) || (self.desk.outlines.open && !self.desk.outlines.drawing.is_empty()))
        }

        pub fn watch(&mut self, now: f64) {
            if let Some(s) = self.desk.watch(now) {
                self.said = Some(s);
            }
        }

        /// What a run made in the cloud for the editor to take in: its props' records and
        /// models, by their paths in the project.
        #[cfg(target_arch = "wasm32")]
        pub fn take_made(&mut self) -> Vec<(String, Vec<u8>)> {
            std::mem::take(&mut self.desk.cloud.made)
        }

        /// The panel (`st`: as the layout has it), when open (and the pictures dropped,
        /// whether or not); a prop to use when a done request's "Use it" is clicked.
        pub fn window(&mut self, ui: &mut egui::Ui, st: &mut super::super::dock::PaneState, content: &Content) -> Option<String> {
            let d = &mut self.desk;
            d.thumbs.take(ui.ctx());
            if !d.loaded {
                d.reload();
            }
            if let Some(s) = d.take_arrived() {
                self.said = Some(s);
            }
            self.sign_in |= std::mem::take(&mut d.sign_in);
            let (mut use_prop, mut name_focused) = (None, false);
            platform::set_hook("requests", if self.open { "open" } else { "" });
            if self.open {
                let hovering = self.hovering;
                st.open = true;
                let title = format!("Model requests ({})", d.list.len());
                let (mut said, mut asked) = (None, None);
                // resizable every way (its corner too); the console in a panel at its foot (open,
                // its top edge drags), the list and the request in what's left
                super::super::dock::show(ui, st, super::super::dock::Pane::Requests, &title, |ui| {
                        // nothing yet, or + pressed: the new request page alone
                        if d.kinds_ui.page || d.list.is_empty() {
                            d.new_page(ui, hovering);
                            return;
                        }
                        platform::set_hook("requestpage", "");
                        d.drop_strip(ui, hovering);
                        d.kind_tabs(ui);
                        ui.add_space(2.0);
                        let shown = d.selected.clone().or_else(|| d.busy());
                        let console = if d.console_open {
                            egui::Panel::bottom("request-console-open").resizable(true).default_size(260.0).min_size(120.0).show(ui, |ui| d.console(ui, shown))
                        } else {
                            egui::Panel::bottom("request-console-folded").resizable(false).exact_size(84.0).show(ui, |ui| d.console(ui, shown))
                        };
                        if let Some(s) = console.inner {
                            said = Some(s);
                        }
                        // a row of what's left: its separator (which takes all the height it's
                        // given) stops there
                        let height = ui.available_height();
                        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(270.0);
                                egui::ScrollArea::vertical().id_salt("request-list").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                                    if let Some(a) = d.list_column(ui) {
                                        asked = Some(a);
                                    }
                                });
                            });
                            ui.separator();
                            ui.vertical(|ui| {
                                egui::ScrollArea::vertical().id_salt("request-pane").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                                    let (prop, s, a) = d.pane(ui, content, &mut name_focused);
                                    (use_prop, said) = (prop, s.or(said.take()));
                                    if a.is_some() {
                                        asked = a;
                                    }
                                });
                            });
                        });
                    });
                if let Some(s) = d.kind_windows(ui.ctx()) {
                    said = Some(s);
                }
                match asked {
                    Some(Run::Start(id)) => said = Some(d.start_run(&id, How::Whole)),
                    Some(Run::GoOn(id)) => said = Some(d.start_run(&id, How::GoOn)),
                    Some(Run::Again(id)) => said = Some(d.start_run(&id, How::Again)),
                    Some(Run::Variants(id)) => said = Some(d.start_run(&id, How::Variants)),
                    Some(Run::MakeAll(id)) => said = Some(d.make_all(&id)),
                    Some(Run::Cancel(id)) => said = Some(d.cancel_run(&id)),
                    Some(Run::Queue(id, how)) => said = Some(d.enqueue(&id, how)),
                    Some(Run::Unqueue(id)) => d.unqueue(&id),
                    Some(Run::QueueMove(id, by)) => d.move_queued(&id, by),
                    Some(Run::QueueWaiting) => said = Some(d.enqueue_waiting()),
                    Some(Run::QueueClear) => d.clear_queue(),
                    Some(Run::QueuePause(paused)) => said = d.pause_queue(paused),
                    None => {}
                }
                if let Some(s) = d.bones_window(ui.ctx()) {
                    said = Some(s);
                }
                if let Some(s) = d.outlines_window(ui.ctx()) {
                    said = Some(s);
                }
                if said.is_some() {
                    self.said = said;
                }
                self.open = st.open;
            } else {
                // the Bones window goes with it
                self.desk.bones.open = false;
            }
            // the name no longer being typed (or the request no longer shown): its folder follows
            if !name_focused {
                if let Some(id) = self.desk.renamed.take() {
                    if let Some(s) = self.desk.follow(&id) {
                        self.said = Some(s);
                    }
                }
            }
            // the browser's picture viewer, when a picture is open
            self.desk.viewer(ui.ctx());
            use_prop
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::desk::*;
    use super::*;

    /// A folder of its own under build/ (never the system's temp: it's on another drive).
    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/tests").join(format!("requests-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(w: u32, h: u32, shade: u8) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([shade, 120, 200, 255]));
        let mut out = std::io::Cursor::new(vec![]);
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn encoded(format: image::ImageFormat) -> Vec<u8> {
        let img = image::RgbImage::from_fn(64, 32, |x, y| image::Rgb([x as u8 * 4, y as u8 * 8, 90]));
        let mut out = std::io::Cursor::new(vec![]);
        img.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    /// A file "dropped" on the window.
    #[derive(Debug)]
    struct Dropped {
        path: PathBuf,
        bytes: Vec<u8>,
    }

    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &Path {
            &self.path
        }

        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(self.bytes.clone())
        }
    }

    fn dropped(name: &str, bytes: Vec<u8>) -> egui::DroppedFileHandle {
        Arc::new(Dropped { path: PathBuf::from("C:/Users/someone/Pictures").join(name), bytes })
    }

    #[test]
    fn a_request_reads_and_writes_as_the_projects_records() {
        let r = ModelRequest {
            id: "moss_toad".into(),
            name: "Moss toad".into(),
            kind: Kind::Character,
            height: 0.8,
            style: Style::LowPoly,
            notes: "Green back, cream belly".into(),
            mine: true,
            status: Status::InProgress,
            progress: Progress { step: "building the model".into(), done: 0.2, started: 1760000000.0, minutes: 25.0 },
            images: vec!["front.png".into(), "side.jpg".into()],
            bones: BTreeMap::from([(
                "front.png".to_string(),
                vec![Bone { part: "leg".into(), name: String::new(), points: vec![[1, 2], [3, 4]] }, Bone { part: "misc".into(), name: "crest".into(), points: vec![[5, 6], [7, 8]] }],
            )]),
            lines: vec!["lines_front.png".into(), "profiles.png".into()],
            ..Default::default()
        };
        let text = content::to_text(&r);
        // the bones as model_requests.py writes them too (test_run_claude.py has the same text):
        // a name only on a line that has one
        assert!(text.contains(BONES_TEXT), "{text}");
        assert!(text.contains("  \"kind\": \"character\",\n") && text.contains("  \"style\": \"low_poly\",\n") && text.contains("  \"status\": \"in_progress\",\n"), "{text}");
        // requests from before: a creature is a character's creature variety; the kit's grass,
        // flower, rock and tree are the environment's varieties (a window kind's stays its own)
        let root = scratch("before");
        for (id, kind, extra) in [("newt", "creature", ""), ("boulder", "rock", ""), ("hedge", "environment", "\"kind_id\": \"bush\", "), ("lawn", "grass", "\"kind_id\": \"turf\", \"variety\": \"short\", ")] {
            std::fs::create_dir_all(root.join("private/requests").join(id)).unwrap();
            std::fs::write(record_path(&root, id), format!("{{\"id\": \"{id}\", \"name\": \"{id}\", {extra}\"kind\": \"{kind}\"}}")).unwrap();
        }
        let read_as = |id: &str| {
            let r = read(&root, id).unwrap();
            (r.kind, r.kind_id.clone(), r.variety.clone(), r.kind_key(), r.lesson_key())
        };
        assert_eq!(read_as("newt"), (Kind::Character, "".into(), "creature".into(), "character".into(), "character".into()));
        assert_eq!(read_as("boulder"), (Kind::Rock, "environment".into(), "rock".into(), "environment".into(), "rock".into()));
        assert_eq!(read_as("hedge"), (Kind::Environment, "bush".into(), "environment".into(), "bush".into(), "bush".into()), "a kind of the user's own: its lessons its own; the generic variety when none");
        assert_eq!(read_as("lawn"), (Kind::Grass, "turf".into(), "short".into(), "turf".into(), "turf".into()), "asked as a window kind already: left as it was");
        std::fs::remove_dir_all(&root).ok();
        assert!(text.contains("  \"progress\": {\n    \"step\": \"building the model\",\n    \"done\": 0.2,\n    \"started\": 1760000000.0,\n    \"minutes\": 25.0\n  },\n"), "{text}");
        assert!(text.contains("  \"images\": [\"front.png\", \"side.jpg\"],\n"), "short lists on one line: {text}");
        let at = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
        assert!(at("images") < at("bones") && at("bones") < at("lines") && at("lines") < at("previews"), "the drawings between the pictures and the renders, as model_requests.py has them: {text}");
        assert!(text.contains("  \"outlines\": {},\n  \"lines\""), "the outlines after the bones, as model_requests.py has them: {text}");
        assert!(
            text.contains("  \"mine\": true,\n  \"many\": false,\n  \"check_lines\": false,\n  \"status\""),
            "drawn by the thousand after mine (toon no longer written: every model is), then the lines' check, as model_requests.py has them: {text}"
        );
        assert!(
            text.ends_with("  \"reply\": \"\",\n  \"comments\": \"\",\n  \"earlier\": [],\n  \"rating\": null,\n  \"several\": false,\n  \"parent\": \"\",\n  \"children\": []\n}\n"),
            "the comments for the next run after the reply, the user's rating, then several of them, as model_requests.py has them: {text}"
        );
        assert!(text.starts_with("{\n  \"id\": \"moss_toad\",\n  \"name\": \"Moss toad\",\n"), "the fields in their order: {text}");
        // grass's cells after the height, the variants after the notes, the props they became
        // after its own, as model_requests.py has them
        assert!(text.contains("  \"height\": 0.8,\n  \"cells\": [1, 1],\n  \"style\""), "{text}");
        assert!(text.contains("  \"notes\": \"Green back, cream belly\",\n  \"variants\": 0,\n  \"vary\": \"\",\n  \"mine\""), "{text}");
        assert!(text.contains("  \"prop\": \"\",\n  \"variant_props\": [],\n  \"reply\""), "{text}");
        let back: ModelRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, r);
        // a short record by hand: the rest are the defaults
        let short: ModelRequest = serde_json::from_str(r#"{ "id": "x", "name": "X", "kind": "prop" }"#).unwrap();
        assert_eq!((short.kind, short.height, short.style, short.status, short.mine), (Kind::Prop, 1.0, Style::Smooth, Status::Waiting, false));
        // a set of floor tiles, its kind as model_requests.py writes it: no height, no lines said
        let floor: ModelRequest = serde_json::from_str(r#"{ "id": "ruins", "name": "Ruins pavement", "kind": "floor", "check_lines": true }"#).unwrap();
        assert_eq!(floor.kind, Kind::Floor);
        assert!(content::to_text(&floor).contains("  \"kind\": \"floor\",
"));
        assert_eq!(about(&floor), "floor set · smooth");
        assert_eq!(about(&short), "prop · 1 cells · smooth");
        let grass: ModelRequest = serde_json::from_str(r#"{ "id": "tuft", "kind": "grass", "height": 0.4, "cells": [2, 1], "variants": 2 }"#).unwrap();
        assert_eq!((grass.kind, about(&grass)), (Kind::Grass, "grass · 2 × 1 cells, 0.4 tall · smooth · 2 variants".to_string()));
        let rock: ModelRequest = serde_json::from_str(r#"{ "kind": "environment", "variants": 1 }"#).unwrap();
        assert_eq!(about(&rock), "environment · 1 cells · smooth · 1 variant");
        // a rock by its footprint, a flower rated, a model not toon (none of it in a short record:
        // toon)
        let boulder: ModelRequest = serde_json::from_str(r#"{ "kind": "rock", "cells": [2, 1], "rating": 85, "toon": false, "many": true }"#).unwrap();
        assert_eq!((boulder.kind, about(&boulder)), (Kind::Rock, "rock · about 2 × 1 cells · smooth · rated 85".to_string()));
        assert!(details(&boulder).contains("Footprint: about 2 × 1 cells") && !details(&boulder).contains("Height") && !details(&boulder).contains("toon"), "toon isn't a setting any more");
        assert!(details(&boulder).contains("Drawn by the thousand") && details(&boulder).contains("Rated 85 of 100"));
        let daisy: ModelRequest = serde_json::from_str(r#"{ "kind": "flower", "height": 0.3 }"#).unwrap();
        assert_eq!((daisy.kind, daisy.toon, daisy.many, daisy.rating, about(&daisy)), (Kind::Flower, true, false, None, "flower · 0.3 cells · smooth".to_string()));
        let varies = |k: Kind| kinds::default_params(k).contains(&kinds::Param::Variants);
        assert!(varies(Kind::Flower) && varies(Kind::Rock) && varies(Kind::Tree) && !varies(Kind::Character) && Kind::Rock.strewn() && !Kind::Character.strewn() && !Kind::Floor.strewn());
        assert!(content::to_text(&grass).contains("  \"kind\": \"grass\",\n") && content::to_text(&rock).contains("  \"kind\": \"environment\",\n"));
        assert!(!short.check_lines && !short.lines_ready(), "no check unless asked");
        let ready: ModelRequest = serde_json::from_str(r#"{ "check_lines": true, "status": "waiting", "progress": { "step": "lines ready: check them" } }"#).unwrap();
        assert!(ready.check_lines && ready.lines_ready());
        assert!(!ModelRequest { status: Status::InProgress, ..ready }.lines_ready(), "only while it waits");
        assert!(serde_json::from_str::<ModelRequest>(r#"{ "status": "finished" }"#).is_err(), "an unknown status is a mistake");
    }

    /// A kind's best and worst rated, as model_requests.py picks them: the best 60 or more,
    /// the worst 40 or less, three of each at most, made ones of that kind only, the one being
    /// made left out; by rating, then by name.
    #[test]
    fn the_best_and_worst_rated_of_a_kind_are_its_lessons() {
        let made = |id: &str, kind: Kind, rating: Option<u8>| ModelRequest { id: id.into(), name: id.into(), kind, rating, status: Status::Done, prop: id.into(), ..Default::default() };
        let list = vec![
            made("a", Kind::Flower, Some(90)),
            made("b", Kind::Flower, Some(95)),
            made("c", Kind::Flower, Some(60)),
            made("d", Kind::Flower, Some(61)),
            made("e", Kind::Flower, Some(50)),
            made("f", Kind::Flower, Some(10)),
            made("g", Kind::Flower, Some(40)),
            made("h", Kind::Flower, None),
            made("i", Kind::Rock, Some(100)),
            ModelRequest { status: Status::Waiting, ..made("j", Kind::Flower, Some(99)) },
        ];
        let ids = |v: Vec<&ModelRequest>| v.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        let (best, worst) = best_and_worst(&list, "flower", "");
        assert_eq!((ids(best), ids(worst)), (vec!["b".to_string(), "a".into(), "d".into()], vec!["f".to_string(), "g".into()]));
        let (best, _) = best_and_worst(&list, "flower", "b");
        assert_eq!(ids(best), ["a", "d", "c"], "the one being made left out");
        let (best, worst) = best_and_worst(&list, "grass", "");
        assert!(best.is_empty() && worst.is_empty(), "none of that kind");
    }

    /// A request's bones as request.json has them (tools/blender/test_run_claude.py checks
    /// model_requests.py writes the same).
    const BONES_TEXT: &str = "  \"bones\": {\n    \"front.png\": [\n      {\n        \"part\": \"leg\",\n        \"points\": [\n          [1, 2],\n          [3, 4]\n        ]\n      },\n      {\n        \"part\": \"misc\",\n        \"name\": \"crest\",\n        \"points\": [\n          [5, 6],\n          [7, 8]\n        ]\n      }\n    ]\n  },\n";

    #[test]
    fn bone_lines_are_named_by_their_part_or_the_users_name() {
        let line = |part: &str, name: &str| Bone { part: part.into(), name: name.into(), points: vec![[0, 0], [1, 1]] };
        let bones = [line("leg", ""), line("tail", ""), line("leg", ""), line("misc", "crest"), line("misc", ""), line("misc", "  ")];
        assert_eq!(bone_names(&bones), ["leg 1", "tail", "leg 2", "crest", "misc 1", "misc 2"]);
        assert_eq!(bone_colour("horn"), [255, 77, 106]);
        assert_eq!(BONE_PARTS.iter().map(|(p, _)| *p).collect::<Vec<_>>(), ["leg", "arm", "torso", "head", "ear", "horn", "tail", "wing", "misc"]);
        // where the canvas's points are on the picture, kept on it
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 200.0));
        assert_eq!(on_picture(rect, egui::vec2(200.0, 100.0), egui::pos2(300.0, 150.0)), [100, 50]);
        assert_eq!(on_picture(rect, egui::vec2(200.0, 100.0), egui::pos2(0.0, 999.0)), [0, 100]);
    }

    #[test]
    fn the_bar_goes_by_the_modellers_word_and_by_the_time() {
        let p = Progress { step: "rendering the previews".into(), done: 0.5, started: 1000.0, minutes: 10.0 };
        // 2 minutes in: its own word is further along than the time
        let (f, text) = p.at(1000.0 + 120.0);
        assert!((f - 0.5).abs() < 1e-6 && text == "rendering the previews · about 8 min left", "{f} {text}");
        // 8 minutes in: the time is
        let (f, text) = p.at(1000.0 + 480.0);
        assert!((f - 0.8).abs() < 1e-3 && text.ends_with("about 2 min left"), "{f} {text}");
        // past the estimate: never full before it's done, and it says so
        assert_eq!(p.at(1000.0 + 660.0), (0.97, "rendering the previews · almost done".to_string()));
        let (f, text) = p.at(1000.0 + 1800.0);
        assert!(f == 0.97 && text.ends_with("longer than the 10 min expected"), "{text}");
        // no start yet, no estimate
        assert_eq!(Progress::default().at(5000.0).1, "starting");
        assert_eq!(Progress { started: 1000.0, ..Default::default() }.at(1000.0 + 300.0).1, "5 min so far");
    }

    #[test]
    fn accurate_requests_say_how_their_poses_and_their_shapes() {
        let r = ModelRequest {
            style: Style::Accurate,
            details: "long brown fur".into(),
            pose: Pose::AsIs,
            shapes: Shapes { min: 1, max: SHAPES_CAP, unlimited: true },
            ..Default::default()
        };
        let text = content::to_text(&r);
        assert!(text.contains("  \"style\": \"accurate\",\n  \"details\": \"long brown fur\",\n  \"pose\": \"as_is\",\n"), "{text}");
        assert!(text.contains("  \"shapes\": {\n    \"min\": 1,\n    \"max\": 100,\n    \"unlimited\": true\n  },\n"), "{text}");
        assert_eq!(serde_json::from_str::<ModelRequest>(&text).unwrap(), r);
        // an older record, without them: a T-pose, 8 to 30 shapes
        let old: ModelRequest = serde_json::from_str(r#"{ "id": "x", "style": "smooth" }"#).unwrap();
        assert_eq!((old.pose, old.shapes, old.details.as_str()), (Pose::TPose, Shapes { min: 8, max: 30, unlimited: false }, ""));
        assert_eq!(serde_json::from_str::<ModelRequest>(r#"{ "pose": "t_pose" }"#).unwrap().pose, Pose::TPose);
    }

    #[test]
    fn the_shapes_meter_reads_its_track() {
        assert_eq!((meter_value(0.0, 100), meter_value(1.0, 100), meter_value(0.5, 100)), (1, 100, 51));
        assert_eq!((meter_value(-3.0, 100), meter_value(7.0, 100)), (1, 100));
        assert_eq!((meter_place(1, 100), meter_place(100, 100)), (0.0, 1.0));
        for v in [1, 8, 30, 77, 100] {
            assert_eq!(meter_value(meter_place(v, 100), 100), v, "{v} there and back");
        }
    }

    #[test]
    fn the_console_greys_each_lines_time() {
        assert_eq!(stamp_len("[2026-10-16 12:31:26] Claude reached"), 22);
        assert_eq!(stamp_len("                      the prompt goes on"), 0);
        assert_eq!(stamp_len("[not a time] at all, but long enough"), 0);
        let job = console_text("[2026-10-16 12:31:26] Claude reached\n                      more of it\n", false);
        let colours: Vec<_> = job.sections.iter().map(|s| (&job.text[s.byte_range.start.0..s.byte_range.end.0], s.format.color)).collect();
        // the time, then the words: the line going on under it is words too (one run of them)
        assert_eq!(colours.len(), 2, "{colours:?}");
        assert_eq!(colours[0].0, "[2026-10-16 12:31:26] ");
        assert!(colours[0].1.r() < colours[1].1.r(), "the time darker than the words: {colours:?}");
        assert_eq!(colours[1].0, "Claude reached\n                      more of it\n");
    }

    #[test]
    fn failed_requests_read_and_sort() {
        let r: ModelRequest = serde_json::from_str(r#"{ "id": "x", "status": "failed", "progress": { "step": "failed: no word from Claude" } }"#).unwrap();
        assert_eq!((r.status, r.progress.step.as_str()), (Status::Failed, "failed: no word from Claude"));
        assert!(content::to_text(&r).contains("  \"status\": \"failed\",\n"));
        // failed ones above the done, under the waiting and those in progress
        assert!(Status::Waiting < Status::InProgress && Status::InProgress < Status::Failed && Status::Failed < Status::Done);
    }

    #[test]
    fn the_console_shows_the_logs_end() {
        let text: String = (1..=10).map(|i| format!("[2026-10-16 09:00:0{}] line {i}\n", i % 10)).collect();
        let (shown, before) = tail(&text, 3);
        assert_eq!(before, 7);
        assert!(shown.starts_with("[2026-10-16 09:00:08] line 8\n") && shown.ends_with("line 10\n"), "{shown}");
        assert_eq!(tail(&text, 20), (text.as_str(), 0));
        assert_eq!(tail("", 3), ("", 0));
    }

    #[test]
    fn a_silent_run_fails_and_a_cancel_nobody_takes_settles() {
        use std::time::{Duration, SystemTime};
        let root = scratch("silent");
        let now = SystemTime::now();
        let mut quiet = create(&root, "Quiet one.png").unwrap();
        quiet.status = Status::InProgress;
        quiet.progress = Progress { step: "rendering the previews".into(), done: 0.5, started: now_secs() - 3600.0, minutes: 12.0 };
        save(&root, &quiet).unwrap();
        assert!(!gone_silent(&quiet, Some(now - Duration::from_secs(60)), now));
        assert!(gone_silent(&quiet, Some(now - Duration::from_secs(40 * 60)), now));
        assert!(!gone_silent(&ModelRequest { status: Status::Waiting, ..quiet.clone() }, Some(now - Duration::from_secs(40 * 60)), now));
        // nothing written in its folder for 40 minutes: the watch marks it failed
        let file = std::fs::File::options().write(true).open(root.join("private/requests/quiet_one/request.json")).unwrap();
        file.set_modified(now - Duration::from_secs(40 * 60)).unwrap();
        drop(file);
        let mut req = Requests::at(&root);
        req.desk().reload();
        let said = req.desk().watch(1.0e9);
        let failed = read(&root, "quiet_one").unwrap();
        assert_eq!(failed.status, Status::Failed);
        assert!(failed.progress.step.starts_with("failed: no news for 30 min"), "{}", failed.progress.step);
        assert!(said.unwrap().contains("failed"));
        // a cancel with no run there to take its stop file: the request waits again
        let mut busy = create(&root, "Busy one.png").unwrap();
        busy.status = Status::InProgress;
        busy.progress.started = now_secs();
        save(&root, &busy).unwrap();
        req.desk().reload();
        assert_eq!(req.desk().busy().as_deref(), Some("busy_one"));
        req.desk().cancel_run("busy_one");
        assert!(root.join("private/requests/busy_one/stop").is_file());
        req.desk().watch(2.0e9);
        assert_eq!(read(&root, "busy_one").unwrap().status, Status::InProgress, "a run has a few seconds to take it");
        req.desk().age_cancel(10.0);
        req.desk().watch(3.0e9);
        assert_eq!(read(&root, "busy_one").unwrap().status, Status::Waiting);
        assert!(!root.join("private/requests/busy_one/stop").exists());
        assert_eq!(req.desk().busy(), None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn pictures_by_their_names() {
        for name in ["front.png", "Side View.JPG", "a.jpeg", "b.WebP"] {
            assert!(is_picture(name), "{name}");
        }
        for name in ["lantern.glb", "notes.txt", "png", "photo.png.zip"] {
            assert!(!is_picture(name), "{name}");
        }
        assert!(hovered_picture(&egui::HoveredFile { path: Some("C:/x/front.png".into()), mime: String::new() }));
        assert!(hovered_picture(&egui::HoveredFile { path: None, mime: "image/webp".into() }));
        assert!(!hovered_picture(&egui::HoveredFile { path: Some("C:/x/model.glb".into()), mime: String::new() }));
        assert!(!hovered_picture(&egui::HoveredFile::default()));
    }

    #[test]
    fn requests_kept_on_disk() {
        let root = scratch("disk");
        // one made from a picture: named after it, written at once
        let mut r = create(&root, "moss_toad-front.png").unwrap();
        assert_eq!((r.id.as_str(), r.name.as_str()), ("moss_toad_front", "Moss toad front"));
        assert!(root.join("private/requests/moss_toad_front/request.json").is_file());
        // pictures in: named safely, the same one kept once, another of the same name numbered
        let front = png(40, 20, 10);
        assert_eq!(add_picture(&root, &mut r, "Moss Toad (front).PNG", &front).unwrap(), Added::New("moss_toad_front.png".into()));
        assert_eq!(add_picture(&root, &mut r, "Moss Toad (front).PNG", &front).unwrap(), Added::Already("moss_toad_front.png".into()));
        assert_eq!(add_picture(&root, &mut r, "moss toad front.png", &png(40, 20, 99)).unwrap(), Added::New("moss_toad_front_2.png".into()));
        assert_eq!(add_picture(&root, &mut r, "side.jpg", &encoded(image::ImageFormat::Jpeg)).unwrap(), Added::New("side.jpg".into()));
        // a picture's kind comes from its bytes, not its name; WebP is kept as PNG (for Blender)
        assert_eq!(add_picture(&root, &mut r, "top.png", &encoded(image::ImageFormat::Jpeg)).unwrap(), Added::New("top.jpg".into()));
        assert_eq!(add_picture(&root, &mut r, "back.webp", &encoded(image::ImageFormat::WebP)).unwrap(), Added::New("back.png".into()));
        let back = std::fs::read(root.join("private/requests/moss_toad_front/back.png")).unwrap();
        assert_eq!(image::guess_format(&back).unwrap(), image::ImageFormat::Png);
        assert_eq!(image::load_from_memory(&back).unwrap().width(), 64);
        assert_eq!(add_picture(&root, &mut r, "back.webp", &encoded(image::ImageFormat::WebP)).unwrap(), Added::Already("back.png".into()));
        assert!(add_picture(&root, &mut r, "notes.png", b"not a picture").is_err());
        assert_eq!(r.images, ["moss_toad_front.png", "moss_toad_front_2.png", "side.jpg", "top.jpg", "back.png"]);
        save(&root, &r).unwrap();
        assert_eq!(read(&root, &r.id).unwrap(), r);
        // taken out: its copy goes too
        remove_picture(&root, &mut r, "moss_toad_front_2.png").unwrap();
        assert!(!root.join("private/requests/moss_toad_front/moss_toad_front_2.png").exists());
        assert_eq!(r.images.len(), 4);
        // renamed while it waits: the folder follows, its pictures with it
        r.name = "Moss toad".into();
        assert_eq!(follow_name(&root, &mut r).unwrap().as_deref(), Some("moss_toad"));
        assert!(root.join("private/requests/moss_toad/side.jpg").is_file() && !root.join("private/requests/moss_toad_front").exists());
        assert_eq!(read(&root, "moss_toad").unwrap().name, "Moss toad");
        // a second request whose name is taken gets a number; once done, renaming keeps the folder
        let mut other = create(&root, "Moss Toad.png").unwrap();
        assert_eq!(other.id, "moss_toad_2");
        other.status = Status::Done;
        other.name = "Something else".into();
        assert_eq!(follow_name(&root, &mut other).unwrap(), None);
        assert_eq!(other.id, "moss_toad_2");
        save(&root, &other).unwrap();
        // the list: waiting first; a broken file reported, not fatal
        std::fs::create_dir_all(root.join("private/requests/broken")).unwrap();
        std::fs::write(root.join("private/requests/broken/request.json"), "{ not json").unwrap();
        let (list, problems) = load_all(&root);
        assert_eq!(list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["moss_toad", "moss_toad_2"]);
        assert!(problems.len() == 1 && problems[0].contains("broken"), "{problems:?}");
        // the watch sees a change
        let before = snapshot(&root);
        std::fs::write(root.join("private/requests/moss_toad/preview_front.png"), png(8, 8, 1)).unwrap();
        assert_ne!(snapshot(&root), before);
        // deleted: the folder goes
        delete(&root, "moss_toad_2").unwrap();
        assert!(!root.join("private/requests/moss_toad_2").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_change_here_keeps_one_made_on_disk_meanwhile() {
        let before = ModelRequest { id: "a".into(), name: "A".into(), notes: "old".into(), ..Default::default() };
        let mut after = before.clone();
        after.notes = "new notes".into();
        // meanwhile, the modeller marked it done
        let disk = ModelRequest { status: Status::Done, prop: "a".into(), previews: vec!["preview_front.png".into()], ..before.clone() };
        let merged = merge(&before, &after, &disk);
        assert_eq!((merged.notes.as_str(), merged.status, merged.prop.as_str(), merged.previews.len()), ("new notes", Status::Done, "a", 1));
    }

    #[test]
    fn thumbnails_of_every_kind() {
        let root = scratch("thumbs");
        for (file, bytes) in [("a.png", png(640, 320, 5)), ("b.jpg", encoded(image::ImageFormat::Jpeg)), ("c.webp", encoded(image::ImageFormat::WebP))] {
            let path = root.join(file);
            std::fs::write(&path, bytes).unwrap();
            let (t, full) = thumbnail(&path, 320).unwrap();
            assert!(t.size[0] <= 320 && t.size[1] <= 320 && t.size[0] == 2 * t.size[1], "{file}: {:?}", t.size);
            assert_eq!(full.x, 2.0 * full.y, "{file}: the picture's own size, {full:?}");
        }
        // larger for the Bones window, never enlarged
        let (big, full) = thumbnail(&root.join("a.png"), 1024).unwrap();
        assert_eq!((big.size, full), ([640, 320], egui::vec2(640.0, 320.0)));
        std::fs::write(root.join("d.png"), b"nope").unwrap();
        assert!(thumbnail(&root.join("d.png"), 320).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    /// The top bar's toggle (a stand-in for the layout's: its count and bar) and the panel,
    /// floating where it first comes.
    fn shown(ui: &mut egui::Ui, req: &mut Requests, content: &content::Content) {
        let r = ui.button("Requests");
        req.decorate(ui, &r);
        let mut st = crate::mapedit::dock::Pane::Requests.first();
        req.window(ui, &mut st, content);
    }

    /// A frame of the window, headless, after what was dropped.
    fn frame(ctx: &egui::Context, req: &mut Requests, files: Vec<egui::DroppedFileHandle>) {
        let content = content::Content::default();
        if !files.is_empty() {
            req.take(files);
        }
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            shown(ui, req, &content);
        });
        // no renderer here to take the textures
        out.textures_delta.clear();
    }

    #[test]
    fn pictures_dropped_on_the_window_make_requests() {
        let root = scratch("window");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        frame(&ctx, &mut req, vec![]);
        assert_eq!(req.waiting(), 0);
        // two pictures at once: one new request, named after the first
        frame(&ctx, &mut req, vec![dropped("Stone Lantern.png", png(60, 90, 1)), dropped("lantern side.jpg", encoded(image::ImageFormat::Jpeg))]);
        assert_eq!(req.waiting(), 1);
        let r = read(&root, "stone_lantern").unwrap();
        assert_eq!(r.images, ["stone_lantern.png", "lantern_side.jpg"]);
        assert!(req.said.take().unwrap().starts_with("New request Stone Lantern: 2 pictures"));
        // another picture goes to the selected one
        frame(&ctx, &mut req, vec![dropped("back.webp", encoded(image::ImageFormat::WebP))]);
        assert_eq!(read(&root, "stone_lantern").unwrap().images.len(), 3);
        // "New request": the next ones start another; something that isn't a picture is refused
        req.desk().selected = None;
        frame(&ctx, &mut req, vec![dropped("toad.png", png(10, 10, 2)), dropped("fake.png", b"text".to_vec())]);
        assert_eq!(req.waiting(), 2);
        assert!(req.said.take().unwrap().contains("fake.png: not a PNG, JPG or WebP picture"));
        // the thumbnails come in on their own thread, the window drawn meanwhile
        for _ in 0..200 {
            frame(&ctx, &mut req, vec![]);
            if req.desk().thumbs.ready() >= 4 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(req.desk().thumbs.ready() >= 4, "{} thumbnails", req.desk().thumbs.ready());
        // being modelled elsewhere: the window sees it within a second, its bar drawn
        let mut working = read(&root, "stone_lantern").unwrap();
        working.status = Status::InProgress;
        working.progress = Progress { step: "rendering the previews".into(), done: 0.5, started: now_secs() - 60.0, minutes: 12.0 };
        save(&root, &working).unwrap();
        req.watch(1.0e9);
        req.desk().selected = Some("stone_lantern".into());
        frame(&ctx, &mut req, vec![]);
        assert_eq!(req.waiting(), 1);
        // its log in the console, closed (its last lines) and open (all of it)
        let log: String = (1..=700).map(|i| format!("[2026-10-16 09:{:02}:00] step {i}\n", i % 60)).collect();
        std::fs::write(root.join("private/requests/stone_lantern/log.txt"), &log).unwrap();
        frame(&ctx, &mut req, vec![]);
        req.desk().console_open = true;
        frame(&ctx, &mut req, vec![]);
        assert_eq!(req.desk().busy().as_deref(), Some("stone_lantern"), "one run at a time: the others' Start waits");
        // and done
        let mut done = read(&root, "stone_lantern").unwrap();
        done.status = Status::Done;
        done.prop = "stone_lantern".into();
        done.progress.minutes = 11.0;
        save(&root, &done).unwrap();
        req.watch(2.0e9);
        frame(&ctx, &mut req, vec![]);
        assert_eq!(read(&root, "stone_lantern").unwrap().status, Status::Done);
        assert_eq!(req.waiting(), 1);
        // failed for want of a login: its reason, and the way to log in
        let mut toad = read(&root, "toad").unwrap();
        toad.status = Status::Failed;
        toad.progress.step = "failed: Claude Code isn't signed in to Claude: the editor's Claude sign-in (or \"claude auth login\" in a terminal), then Start again".into();
        save(&root, &toad).unwrap();
        req.watch(3.0e9);
        req.desk().selected = Some("toad".into());
        frame(&ctx, &mut req, vec![]);
        assert_eq!(req.waiting(), 0);
        // the lines Claude made, shown as they come (a listed one not there yet isn't); taking a
        // picture out takes its lines too
        let dir = root.join("private/requests/stone_lantern");
        std::fs::write(dir.join("lines_stone_lantern.png"), png(40, 30, 4)).unwrap();
        std::fs::write(dir.join("profiles.png"), png(50, 30, 5)).unwrap();
        let mut sketched = read(&root, "stone_lantern").unwrap();
        sketched.lines = vec!["lines_stone_lantern.png".into(), "profiles.png".into(), "fit_front.png".into()];
        save(&root, &sketched).unwrap();
        req.watch(2.5e9);
        req.desk().selected = Some("stone_lantern".into());
        frame(&ctx, &mut req, vec![]);
        remove_picture(&root, &mut sketched, "stone_lantern.png").unwrap();
        assert_eq!(sketched.lines, ["profiles.png", "fit_front.png"]);
        assert!(!dir.join("lines_stone_lantern.png").exists() && dir.join("profiles.png").exists());
        req.desk().selected = Some("toad".into());
        // an accurate creature: its model details, its pose and its shapes drawn
        toad.style = Style::Accurate;
        toad.details = "short ears".into();
        toad.shapes.unlimited = true;
        save(&root, &toad).unwrap();
        req.watch(4.0e9);
        frame(&ctx, &mut req, vec![]);
        req.desk().console_open = false;
        frame(&ctx, &mut req, vec![]);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A frame of the window on a desktop's screen, with what the user does in it (the marks
    /// say where each widget is drawn this time).
    fn frame_with(ctx: &egui::Context, req: &mut Requests, events: Vec<egui::Event>) {
        let content = content::Content::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        crate::platform::clear_hooks();
        let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| {
            shown(ui, req, &content);
        });
        out.textures_delta.clear();
    }

    /// Where a marked widget was drawn in the last frame.
    fn marked(name: &str) -> Option<egui::Pos2> {
        let at = crate::platform::hook(name)?;
        let (x, y) = at.split_once(',')?;
        Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
    }

    /// A click on a marked widget, as a pointer makes it: over it and down, then up.
    fn click(ctx: &egui::Context, req: &mut Requests, name: &str) {
        let pos = marked(name).unwrap_or_else(|| panic!("{name} isn't drawn"));
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(ctx, req, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame_with(ctx, req, vec![button(false)]);
    }

    #[test]
    fn the_lines_wait_for_the_users_check_then_go_on() {
        let root = scratch("check");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Moss toad.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("moss_toad".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiRequestGoOn").is_none(), "nothing to check yet");
        // "Let me check the lines first": ticked, kept in its request.json
        assert!(!read(&root, "moss_toad").unwrap().check_lines);
        click(&ctx, &mut req, "uiRequestCheckLines");
        assert!(read(&root, "moss_toad").unwrap().check_lines, "the box ticked");
        // Start: run_claude.py on it (which, ticked, stops once the lines are drawn)
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestStart_moss_toad");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "moss_toad"]]);
        assert_eq!(req.desk().busy().as_deref(), Some("moss_toad"));
        // the run draws the lines and the front and the side, and leaves it waiting for the check
        let dir = root.join("private/requests/moss_toad");
        std::fs::write(dir.join("lines_front.png"), png(40, 30, 4)).unwrap();
        std::fs::write(dir.join("profiles.png"), png(50, 30, 5)).unwrap();
        let mut drawn = read(&root, "moss_toad").unwrap();
        drawn.progress = Progress { step: LINES_READY.into(), done: 0.12, started: now_secs(), minutes: 4.0 };
        drawn.lines = vec!["lines_front.png".into(), "profiles.png".into()];
        drawn.reply = "Lines ready: the shine on its back is the drawing's; how long its tail is, a guess.".into();
        save(&root, &drawn).unwrap();
        // a second's look reads it again, the next one sees the run began
        req.watch(1.0e9);
        req.watch(1.5e9);
        assert!(drawn.lines_ready() && req.desk().busy().is_none(), "the user's turn: the run seen to start and end, nothing running");
        assert_eq!(req.waiting(), 1);
        // the row says so, and its Check shows the request
        req.desk().selected = None;
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestStart_moss_toad").is_none(), "the row's button is Check, not Start");
        click(&ctx, &mut req, "uiRequestCheck_moss_toad");
        assert_eq!(req.desk().selected.as_deref(), Some("moss_toad"));
        // the lines, the note for what's off and the button, at the top of its panel
        frame_with(&ctx, &mut req, vec![]);
        let (note, go, start) = (marked("uiRequestLinesNote").unwrap(), marked("uiRequestGoOn").unwrap(), marked("uiRequestStart_moss_toad").unwrap());
        assert!(note.y < go.y && go.y < start.y, "the check before the request's fields: {note:?} {go:?} {start:?}");
        click(&ctx, &mut req, "uiRequestLinesNote");
        frame_with(&ctx, &mut req, vec![egui::Event::Text("the tail is longer".into())]);
        assert_eq!(req.desk().lines_notes.get("moss_toad").map(String::as_str), Some("the tail is longer"));
        // one at a time: while another is being made, the button waits
        let mut other = create(&root, "Other one.png").unwrap();
        other.status = Status::InProgress;
        other.progress.started = now_secs();
        save(&root, &other).unwrap();
        req.watch(2.0e9);
        frame_with(&ctx, &mut req, vec![]);
        // "The lines look right" while another runs: nothing started, it waits its turn in the
        // queue, the note with it
        click(&ctx, &mut req, "uiRequestGoOn");
        assert_eq!(req.desk().launched.len(), 1, "nothing started while another runs");
        assert_eq!(req.desk().queue.ids(), ["moss_toad"]);
        other.status = Status::Done;
        save(&root, &other).unwrap();
        // the other made: run_claude.py again, from the model, with the note
        req.watch(3.0e9);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "moss_toad", "--go-on", "--note", "the tail is longer"]);
        assert_eq!(req.desk().busy().as_deref(), Some("moss_toad"), "being started: one at a time");
        assert!(!req.desk().lines_notes.contains_key("moss_toad"), "the note went with it");
        assert!(req.said.take().unwrap().contains("goes on with moss_toad"));
        // clicked again while it starts: nothing more
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestGoOn");
        assert_eq!(req.desk().launched.len(), 2);
        assert!(req.desk().queue.runs.is_empty(), "nor queued");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_request_made_once_is_made_again_with_the_users_comments() {
        let root = scratch("again");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Tall tree.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("tall_tree".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiRequestComments").is_none(), "nothing made yet: nothing to make again");
        // the run made it
        let mut made = read(&root, "tall_tree").unwrap();
        made.status = Status::Done;
        made.prop = "tall_tree".into();
        made.reply = "A redwood, mostly trunk.".into();
        made.earlier = vec!["fewer branches low down".into()];
        save(&root, &made).unwrap();
        req.watch(1.0e9);
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        // its section is down the panel, under the model and what Claude says
        wheel(&ctx, &mut req, marked("uiRequestName").unwrap(), 10.0);
        // with nothing said, the button waits
        click(&ctx, &mut req, "uiRequestAgain");
        assert!(req.desk().launched.is_empty(), "nothing said: nothing started");
        // what to change, kept in its request.json as it's typed
        click(&ctx, &mut req, "uiRequestComments");
        frame_with(&ctx, &mut req, vec![egui::Event::Text("the trunk thicker".into())]);
        assert_eq!(read(&root, "tall_tree").unwrap().comments, "the trunk thicker");
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestAgain");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "tall_tree", "--again", "--note", "the trunk thicker"]]);
        assert_eq!(req.desk().busy().as_deref(), Some("tall_tree"), "being started: one at a time");
        assert!(req.said.take().unwrap().contains("makes tall_tree again from its last model"));
        // the status row's Make it again stays a whole run, from the pictures
        assert_eq!(read(&root, "tall_tree").unwrap().earlier, ["fewer branches low down"], "the comments told before kept");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A rock asks for its footprint, not its height; made, it's rated in its panel (0 to 100,
    /// kept in its request.json), and the rating taken off again; one not made has none to give.
    #[test]
    fn a_rock_asks_for_its_footprint_and_a_model_made_is_rated() {
        let root = scratch("rated");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Round stone.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        r.kind = Kind::Rock;
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("round_stone".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiRequestCellsX").is_some() && marked("uiRequestToon").is_none() && marked("uiRequestMany").is_some(), "its footprint, drawn by the thousand; no toon toggle");
        assert!(marked("uiRequestRating").is_none(), "nothing made yet: nothing to rate");
        let mut made = read(&root, "round_stone").unwrap();
        (made.status, made.prop) = (Status::Done, "round_stone".into());
        save(&root, &made).unwrap();
        req.watch(1.0e9);
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        // its rating is down the panel, under the model
        wheel(&ctx, &mut req, marked("uiRequestName").unwrap(), 10.0);
        click(&ctx, &mut req, "uiRequestRating");
        let rated = read(&root, "round_stone").unwrap().rating;
        assert!(rated.is_some_and(|v| (35..=65).contains(&v)), "a click on its middle: about 50 ({rated:?})");
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestRatingClear");
        assert_eq!(read(&root, "round_stone").unwrap().rating, None, "taken off");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn grass_and_environment_ask_for_variants_and_one_made_makes_them() {
        let root = scratch("variants");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Meadow tuft.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("meadow_tuft".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        // a creature not made yet: no variants
        assert!(marked("uiRequestVariants").is_none() && marked("uiRequestCellsX").is_none());
        // grass (the environment's variety): the cells it fills to the edges, and its variants
        click(&ctx, &mut req, "uiRequestKindEnvironment");
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestVariety_grass");
        let tuft = read(&root, "meadow_tuft").unwrap();
        assert_eq!((tuft.kind, tuft.kind_id.as_str(), tuft.variety.as_str()), (Kind::Grass, "environment", "grass"), "made as grass, asked as the environment's grass");
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestCellsX").is_some() && marked("uiRequestCellsZ").is_some() && marked("uiRequestVariants").is_some());
        assert!(marked("uiRequestVary").is_none(), "how they differ, once there are some");
        let mut grass = read(&root, "meadow_tuft").unwrap();
        grass.variants = 2;
        save(&root, &grass).unwrap();
        req.watch(1.0e9);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestVary");
        frame_with(&ctx, &mut req, vec![egui::Event::Text("taller, yellower".into())]);
        assert_eq!(read(&root, "meadow_tuft").unwrap().vary, "taller, yellower", "kept as it's typed");
        // an environment asset (the kind itself): its variants, no cells
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestVariety_environment");
        frame_with(&ctx, &mut req, vec![]);
        assert_eq!(read(&root, "meadow_tuft").unwrap().kind, Kind::Environment);
        assert!(marked("uiRequestVariants").is_some() && marked("uiRequestCellsX").is_none());
        assert!(marked("uiRequestMakeVariants").is_none(), "not made: nothing to make variants of");
        // made, a prop asking for none: its variants to say, Make variants waiting for a count
        let mut made = read(&root, "meadow_tuft").unwrap();
        (made.kind, made.status, made.prop, made.variants) = (Kind::Prop, Status::Done, "meadow_tuft".into(), 0);
        made.variant_props = vec!["meadow_tuft_variant_2".into()];
        save(&root, &made).unwrap();
        req.watch(2.0e9);
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestVariants").is_some(), "any request made can have some");
        assert!(marked("uiRequestUseVariant_meadow_tuft_variant_2").is_some(), "the variants made, listed");
        wheel(&ctx, &mut req, marked("uiRequestName").unwrap(), 10.0);
        click(&ctx, &mut req, "uiRequestMakeVariants");
        assert!(req.desk().launched.is_empty(), "none asked for: nothing started");
        // three: run_claude.py on it, from its model
        made.variants = 3;
        save(&root, &made).unwrap();
        req.watch(3.0e9);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestMakeVariants");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "meadow_tuft", "--variants"]]);
        assert_eq!(req.desk().busy().as_deref(), Some("meadow_tuft"), "being started: one at a time");
        assert!(req.said.take().unwrap().contains("makes variants of meadow_tuft from its model"));
        // another made one while that runs: queued, to make its variants once it's ended
        let mut rock = create(&root, "Flat rock.png").unwrap();
        (rock.kind, rock.status, rock.prop, rock.variants) = (Kind::Environment, Status::Done, "flat_rock".into(), 2);
        save(&root, &rock).unwrap();
        req.watch(4.0e9);
        req.desk().selected = Some("flat_rock".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        wheel(&ctx, &mut req, marked("uiRequestName").unwrap(), 10.0);
        click(&ctx, &mut req, "uiRequestMakeVariants");
        assert_eq!(req.desk().launched.len(), 1, "nothing started while another runs");
        let queue = req.desk().queue.clone();
        assert_eq!((queue.ids(), queue.runs[0].how), (vec!["flat_rock"], How::Variants));
        // a set of floor tiles varies by its tiles: none
        let mut floor = create(&root, "Ruins.png").unwrap();
        (floor.kind, floor.status, floor.prop, floor.variants) = (Kind::Floor, Status::Done, "ruins_slab".into(), 2);
        save(&root, &floor).unwrap();
        req.watch(5.0e9);
        req.desk().selected = Some("ruins".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiRequestVariants").is_none() && marked("uiRequestMakeVariants").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn several_drawings_are_split_then_made_one_after_another() {
        let root = scratch("several");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Shrubs.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("shrubs".into());
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiRequestMakeAll").is_none(), "nothing split yet");
        // "Process multiple", ticked: kept in its request.json
        click(&ctx, &mut req, "uiRequestSeveral");
        assert!(read(&root, "shrubs").unwrap().several, "the box ticked");
        // Start: run_claude.py on it (which splits it)
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestStart_shrubs");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "shrubs"]]);
        // the run makes a request of each drawing, and the request is done once split
        let began = now_secs();
        for (id, name) in [("shrubs_tall_one", "Tall one"), ("shrubs_low_one", "Low one")] {
            save(&root, &ModelRequest { id: id.into(), name: name.into(), kind: Kind::Prop, parent: "shrubs".into(), images: vec!["drawing.png".into()], ..Default::default() }).unwrap();
        }
        let mut split = read(&root, "shrubs").unwrap();
        split.status = Status::Done;
        split.progress = Progress { step: "split into 2 requests".into(), done: 1.0, started: began, minutes: 2.0 };
        split.children = vec!["shrubs_tall_one".into(), "shrubs_low_one".into()];
        save(&root, &split).unwrap();
        req.watch(1.0e9);
        req.watch(1.5e9);
        assert!(req.desk().busy().is_none(), "the split run seen to start and end");
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestStart_shrubs").is_none(), "split: no Start of its own, its requests have theirs");
        assert!(marked("uiRequestChild_shrubs_tall_one").is_some() && marked("uiRequestChild_shrubs_low_one").is_some(), "its requests listed");
        // Make them all: the first one now, the next once it has ended
        click(&ctx, &mut req, "uiRequestMakeAll");
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "shrubs_tall_one"]);
        assert_eq!(req.desk().queue.ids(), ["shrubs_low_one"]);
        let mut first = read(&root, "shrubs_tall_one").unwrap();
        first.status = Status::Done;
        first.progress = Progress { step: "done".into(), done: 1.0, started: now_secs(), minutes: 9.0 };
        save(&root, &first).unwrap();
        req.watch(2.0e9);
        req.watch(2.5e9);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "shrubs_low_one"], "the next one, once the first was made");
        assert!(req.desk().queue.runs.is_empty());
        // one of them shows where it came from
        req.desk().selected = Some("shrubs_low_one".into());
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestParent");
        assert_eq!(req.desk().selected.as_deref(), Some("shrubs"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// A request's run seen to begin and end with its model made (as run_claude.py leaves it).
    fn made(root: &Path, id: &str) {
        let mut r = read(root, id).unwrap();
        r.status = Status::Done;
        r.progress = Progress { step: "done".into(), done: 1.0, started: now_secs(), minutes: 9.0 };
        save(root, &r).unwrap();
    }

    /// Requests for the queue's tests, each with a picture.
    fn requests_named(root: &Path, names: &[&str]) {
        for name in names {
            let mut r = create(root, &format!("{name}.png")).unwrap();
            add_picture(root, &mut r, "front.png", &png(30, 30, 1)).unwrap();
            save(root, &r).unwrap();
        }
    }

    /// One being made, three queued on their rows' buttons (Queue, while another is made),
    /// shown in the queue over the list and one moved a place sooner; kept for the editor's
    /// next run; each made once the last has ended, in the queue's order; paused, nothing more
    /// starts until it goes on; one made some other way meanwhile isn't made again.
    #[test]
    fn queued_requests_are_made_one_after_another() {
        let root = scratch("queue");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        requests_named(&root, &["Ant", "Bee", "Cat", "Dog"]);
        req.desk().reload();
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        assert!(marked("uiQueueWaiting").is_some(), "the queue over the list");
        click(&ctx, &mut req, "uiRequestStart_ant");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "ant"]]);
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestStart_bee").is_none(), "another being made: Queue, not Start");
        for id in ["bee", "cat", "dog"] {
            click(&ctx, &mut req, &format!("uiRequestQueue_{id}"));
            frame_with(&ctx, &mut req, vec![]);
        }
        assert_eq!(req.desk().queue.ids(), ["bee", "cat", "dog"]);
        assert_eq!(crate::platform::hook("requestqueue").as_deref(), Some("bee;cat;dog"));
        assert!(marked("uiRequestUnqueue_cat").is_some(), "a queued one's row says where it is");
        click(&ctx, &mut req, "uiQueueUp_dog");
        assert_eq!(req.desk().queue.ids(), ["bee", "dog", "cat"]);
        // kept: the editor's next run reads it back
        let mut next_run = Requests::at(&root);
        next_run.desk().reload();
        assert_eq!(next_run.desk().queue.ids(), ["bee", "dog", "cat"]);
        // the ant made: the bee next, on its own
        made(&root, "ant");
        req.watch(1.0e9);
        req.watch(1.5e9);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "bee"]);
        assert_eq!(req.desk().queue.ids(), ["dog", "cat"]);
        // paused: the bee made, nothing more
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiQueuePause");
        assert!(req.desk().queue.paused);
        made(&root, "bee");
        req.watch(2.0e9);
        req.watch(2.5e9);
        assert_eq!(req.desk().launched.len(), 2, "nothing starts while it's paused");
        // the dog made some other way meanwhile; going on, the cat (the dog isn't made again)
        made(&root, "dog");
        req.watch(3.0e9);
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiQueuePause");
        assert!(!req.desk().queue.paused);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "cat"]);
        assert!(req.desk().queue.runs.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// "Queue all waiting": the requests waiting to be made, in the list's order (not one made,
    /// nor one whose lines wait for the user's check), the first started at once; one split
    /// into several from the queue has its own requests made next, before the rest; going on
    /// from checked lines, asked while another is made, waits its turn with the user's note.
    #[test]
    fn all_waiting_queued_and_a_split_one_s_requests_go_next() {
        let root = scratch("queue-all");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        requests_named(&root, &["Ant", "Bushes", "Cat", "Dog", "Eel"]);
        let mut bushes = read(&root, "bushes").unwrap();
        bushes.several = true;
        save(&root, &bushes).unwrap();
        made(&root, "cat");
        let mut dog = read(&root, "dog").unwrap();
        dog.progress.step = LINES_READY.into();
        save(&root, &dog).unwrap();
        req.desk().reload();
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        click(&ctx, &mut req, "uiQueueWaiting");
        assert_eq!(req.desk().launched, [["tools/blender/run_claude.py", "ant"]], "the first at once");
        assert_eq!(req.desk().queue.ids(), ["bushes", "eel"], "not the cat (made) nor the dog (its lines wait)");
        // the dog's lines checked while the ant is made: it waits its turn, the note with it
        req.desk().lines_notes.insert("dog".into(), "the tail longer".into());
        req.desk().selected = Some("dog".into());
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestGoOn");
        assert_eq!(req.desk().queue.ids(), ["bushes", "eel", "dog"]);
        // the ant made: the bushes, whose run splits them in two
        made(&root, "ant");
        req.watch(1.0e9);
        req.watch(1.5e9);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "bushes"]);
        for (id, name) in [("bushes_tall", "Tall"), ("bushes_low", "Low")] {
            save(&root, &ModelRequest { id: id.into(), name: name.into(), kind: Kind::Prop, parent: "bushes".into(), images: vec!["drawing.png".into()], ..Default::default() }).unwrap();
        }
        let mut split = read(&root, "bushes").unwrap();
        split.status = Status::Done;
        split.progress = Progress { step: "split into 2 requests".into(), done: 1.0, started: now_secs(), minutes: 2.0 };
        split.children = vec!["bushes_tall".into(), "bushes_low".into()];
        save(&root, &split).unwrap();
        req.watch(2.0e9);
        req.watch(2.5e9);
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "bushes_tall"], "its own requests next");
        assert_eq!(req.desk().queue.ids(), ["bushes_low", "eel", "dog"]);
        // and so on to the dog, gone on from its lines with the note
        for (k, id) in ["bushes_tall", "bushes_low", "eel"].iter().enumerate() {
            made(&root, id);
            req.watch(3.0e9 + k as f64 * 1.0e9);
            req.watch(3.5e9 + k as f64 * 1.0e9);
        }
        assert_eq!(req.desk().launched.last().unwrap(), &["tools/blender/run_claude.py", "dog", "--go-on", "--note", "the tail longer"]);
        assert!(req.desk().queue.runs.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// The wheel turned over a point: what's under it scrolled down by `pages` (and the frames
    /// it takes to get there).
    fn wheel(ctx: &egui::Context, req: &mut Requests, at: egui::Pos2, pages: f32) {
        let turn = egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Page, delta: egui::vec2(0.0, -pages), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE };
        frame_with(ctx, req, vec![egui::Event::PointerMoved(at), turn]);
        for _ in 0..40 {
            frame_with(ctx, req, vec![]);
        }
    }

    /// A click (or a right-click) at a point of the screen, as a pointer makes it.
    fn click_at(ctx: &egui::Context, req: &mut Requests, pos: egui::Pos2, button: egui::PointerButton) {
        let event = |pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(ctx, req, vec![egui::Event::PointerMoved(pos), event(true)]);
        frame_with(ctx, req, vec![event(false)]);
    }

    fn press(ctx: &egui::Context, req: &mut Requests, key: egui::Key) {
        frame_with(ctx, req, vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
        frame_with(ctx, req, vec![]);
    }

    #[test]
    fn the_console_folds_with_its_window() {
        let root = scratch("fold");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        create(&root, "Moss toad.png").unwrap();
        req.desk().reload();
        req.desk().selected = Some("moss_toad".into());
        req.desk().console_open = true;
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        let shown = || marked("uiPaneDock_requests").is_some() && marked("uiRequestConsole").is_some();
        assert!(shown(), "the window, its console open at its foot");
        // its title bar double-clicked: folded, its console with it (it stays open)
        let window = egui::Id::new(("editor-pane", "requests")).with(("window", 0u32));
        let fold = |req: &mut Requests| {
            // (folded, it's as wide as its title)
            let bar = ctx.memory(|m| m.area_rect(window)).map(|r| egui::pos2(r.center().x, r.top() + 10.0)).expect("the window's place");
            click_at(&ctx, req, bar, egui::PointerButton::Primary);
            click_at(&ctx, req, bar, egui::PointerButton::Primary);
            // (it folds and unfolds in a moment; and the next two clicks within 0.6 s of these
            // would be a triple click)
            for _ in 0..60 {
                frame_with(&ctx, req, vec![]);
            }
        };
        fold(&mut req);
        assert!(marked("uiPaneDock_requests").is_none(), "folded");
        assert!(marked("uiRequestConsole").is_none(), "its console folded with it");
        assert!(req.open);
        // again: as it was, its console too
        fold(&mut req);
        assert!(shown(), "unfolded, its console too");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The Bones window's canvas, once its picture is read and the window has settled its
    /// size (its hints wrap otherwise while it grows): where it is and how big.
    fn canvas(ctx: &egui::Context, req: &mut Requests) -> egui::Rect {
        let (mut last, mut same) = (String::new(), 0);
        for _ in 0..300 {
            frame_with(ctx, req, vec![]);
            match crate::platform::hook("requestbones") {
                Some(at) if at == last => same += 1,
                Some(at) => (last, same) = (at, 0),
                None => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
            if same >= 3 {
                let v: Vec<f32> = last.split(',').map(|n| n.parse().unwrap()).collect();
                return egui::Rect::from_min_size(egui::pos2(v[0], v[1]), egui::vec2(v[2], v[3]));
            }
        }
        panic!("the Bones window's picture never came");
    }

    /// The Outlines window's picture, once it's come and stays put.
    fn outline_canvas(ctx: &egui::Context, req: &mut Requests) -> egui::Rect {
        let (mut last, mut same) = (String::new(), 0);
        for _ in 0..300 {
            frame_with(ctx, req, vec![]);
            match crate::platform::hook("requestoutlines") {
                Some(at) if at == last => same += 1,
                Some(at) => (last, same) = (at, 0),
                None => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
            if same >= 3 {
                let v: Vec<f32> = last.split(',').map(|n| n.parse().unwrap()).collect();
                return egui::Rect::from_min_size(egui::pos2(v[0], v[1]), egui::vec2(v[2], v[3]));
            }
        }
        panic!("the Outlines window's picture never came");
    }

    #[test]
    fn the_drawings_to_make_are_outlined_each_with_its_height_and_cells() {
        let root = scratch("outlines");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Shrubs.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(200, 100, 3)).unwrap();
        (r.kind, r.height) = (Kind::Environment, 1.5);
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("shrubs".into());
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestOutlines").is_none(), "nothing to outline unless it processes several");
        // Process multiple ticked: the drawings to make outlined in a window of their own
        click(&ctx, &mut req, "uiRequestSeveral");
        assert!(read(&root, "shrubs").unwrap().several);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestOutlines");
        assert!(req.desk().outlines.open, "Outline the drawings… opens its window");
        let c = outline_canvas(&ctx, &mut req);
        let at = |u: f32, v: f32| c.min + egui::vec2(u * c.width(), v * c.height());
        let outlines = |root: &Path| read(root, "shrubs").unwrap().outlines.get("front.png").cloned().unwrap_or_default();
        // a click for each point round it, a right-click for the last: closed, kept at once, as
        // tall as the request, on a cell
        click_at(&ctx, &mut req, at(0.1, 0.1), egui::PointerButton::Primary);
        assert!(req.drawing_on_picture(), "an outline being drawn: the editor's own Escape and Backspace stand aside");
        click_at(&ctx, &mut req, at(0.4, 0.1), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.4, 0.9), egui::PointerButton::Primary);
        assert!(outlines(&root).is_empty(), "nothing kept until it's closed");
        click_at(&ctx, &mut req, at(0.1, 0.9), egui::PointerButton::Secondary);
        assert!(!req.drawing_on_picture());
        assert_eq!(outlines(&root), [Outline { points: vec![[20, 10], [80, 10], [80, 90], [20, 90]], height: 1.5, cells: [1, 1] }]);
        // two points make no outline; Escape drops the one being drawn
        click_at(&ctx, &mut req, at(0.6, 0.2), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.7, 0.2), egui::PointerButton::Secondary);
        assert_eq!(outlines(&root).len(), 1, "an outline takes three points or more");
        click_at(&ctx, &mut req, at(0.6, 0.2), egui::PointerButton::Primary);
        press(&ctx, &mut req, egui::Key::Escape);
        assert!(req.desk().outlines.drawing.is_empty());
        // listed with its height and its cells; a second one, then the first out by its ×
        frame_with(&ctx, &mut req, vec![]);
        assert!(marked("uiRequestOutlineHeight_1").is_some() && marked("uiRequestOutlineCellsX_1").is_some() && marked("uiRequestOutlineCellsZ_1").is_some());
        click_at(&ctx, &mut req, at(0.6, 0.2), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.9, 0.2), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.9, 0.8), egui::PointerButton::Secondary);
        assert_eq!(outlines(&root).len(), 2);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestOutlineOut_1");
        assert_eq!((outlines(&root).len(), outlines(&root)[0].points[0]), (1, [120, 20]));
        // a second picture's outlines numbered after the first's
        let mut two = read(&root, "shrubs").unwrap();
        add_picture(&root, &mut two, "side.png", &png(100, 100, 9)).unwrap();
        two.outlines.insert("side.png".into(), vec![Outline { points: vec![[1, 1], [50, 1], [50, 50]], height: 2.0, cells: [2, 1] }]);
        save(&root, &two).unwrap();
        let numbered: Vec<(&str, f64)> = two.outlined().iter().map(|(p, o)| (*p, o.height)).collect();
        assert_eq!(numbered, [("front.png", 1.5), ("side.png", 2.0)]);
        req.watch(1.0e9);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestOutlinesOn_side.png");
        crate::platform::clear_hooks();
        outline_canvas(&ctx, &mut req);
        assert!(marked("uiRequestOutlineHeight_2").is_some() && marked("uiRequestOutlineHeight_1").is_none(), "side.png's first is the 2nd");
        // split: nothing more to outline, its window gone
        let mut split = read(&root, "shrubs").unwrap();
        split.children = vec!["shrubs_round_bush".into()];
        save(&root, &split).unwrap();
        req.watch(2.0e9);
        frame_with(&ctx, &mut req, vec![]);
        assert!(!req.desk().outlines.open);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_bones_are_drawn_over_a_picture() {
        let root = scratch("bones");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Moss toad.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(200, 100, 3)).unwrap();
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some("moss_toad".into());
        frame_with(&ctx, &mut req, vec![]);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestBones");
        assert!(req.desk().bones.open, "Draw the bones… opens its window");
        let c = canvas(&ctx, &mut req);
        let at = |u: f32, v: f32| c.min + egui::vec2(u * c.width(), v * c.height());
        let bones = |root: &Path| read(root, "moss_toad").unwrap().bones.get("front.png").cloned().unwrap_or_default();
        // a leg: a click for each joint, chained, and a right-click for the last
        click_at(&ctx, &mut req, at(0.25, 0.5), egui::PointerButton::Primary);
        assert!(req.drawing_on_picture(), "a line being drawn: the editor's own Escape and Backspace stand aside");
        click_at(&ctx, &mut req, at(0.5, 0.5), egui::PointerButton::Primary);
        assert!(bones(&root).is_empty(), "nothing kept until the line ends");
        click_at(&ctx, &mut req, at(0.75, 0.6), egui::PointerButton::Secondary);
        assert!(!req.drawing_on_picture());
        assert_eq!(bones(&root), [Bone { part: "leg".into(), name: String::new(), points: vec![[50, 50], [100, 50], [150, 60]] }], "kept in request.json at once");
        // Backspace takes a joint back, Escape drops the line being drawn
        click_at(&ctx, &mut req, at(0.1, 0.1), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.2, 0.1), egui::PointerButton::Primary);
        press(&ctx, &mut req, egui::Key::Backspace);
        assert_eq!(req.desk().bones.drawing, [[20, 10]]);
        press(&ctx, &mut req, egui::Key::Escape);
        assert!(req.desk().bones.drawing.is_empty() && bones(&root).len() == 1, "the line dropped, the leg kept");
        // a right-click on its own makes no line
        click_at(&ctx, &mut req, at(0.3, 0.3), egui::PointerButton::Secondary);
        assert_eq!(bones(&root).len(), 1);
        // another part: a tail, then a misc piece named as it's drawn
        click(&ctx, &mut req, "uiRequestBone_tail");
        click_at(&ctx, &mut req, at(0.9, 0.5), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.95, 0.2), egui::PointerButton::Secondary);
        click(&ctx, &mut req, "uiRequestBone_misc");
        click(&ctx, &mut req, "uiRequestBoneName");
        frame_with(&ctx, &mut req, vec![egui::Event::Text("crest".into())]);
        click_at(&ctx, &mut req, at(0.4, 0.1), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.6, 0.05), egui::PointerButton::Secondary);
        let drawn = bones(&root);
        assert_eq!(drawn.iter().map(|b| (b.part.as_str(), b.name.as_str())).collect::<Vec<_>>(), [("leg", ""), ("tail", ""), ("misc", "crest")]);
        // its name changed in the list under the picture
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestBoneName_2");
        frame_with(&ctx, &mut req, vec![egui::Event::Text(" spike".into())]);
        assert_eq!(bones(&root)[2].name, "crest spike");
        // the record says so on disk as the editor and model_requests.py write it
        let text = std::fs::read_to_string(root.join("private/requests/moss_toad/request.json")).unwrap();
        assert!(text.contains("\"part\": \"misc\",\n        \"name\": \"crest spike\",") && !text.contains("\"name\": \"\""), "{text}");
        // a line taken out by its ×, the last by Undo
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestBoneOut_0");
        assert_eq!(bones(&root).iter().map(|b| b.part.as_str()).collect::<Vec<_>>(), ["tail", "misc"]);
        click(&ctx, &mut req, "uiRequestBonesUndo");
        assert_eq!(bones(&root).iter().map(|b| b.part.as_str()).collect::<Vec<_>>(), ["tail"]);
        // a second picture: its own lines
        let mut r = read(&root, "moss_toad").unwrap();
        add_picture(&root, &mut r, "side.png", &png(100, 100, 9)).unwrap();
        save(&root, &r).unwrap();
        req.watch(1.0e9);
        frame_with(&ctx, &mut req, vec![]);
        click(&ctx, &mut req, "uiRequestBonesOn_side.png");
        crate::platform::clear_hooks();
        let c = canvas(&ctx, &mut req);
        let at = |u: f32, v: f32| c.min + egui::vec2(u * c.width(), v * c.height());
        click(&ctx, &mut req, "uiRequestBone_ear");
        assert_eq!(canvas(&ctx, &mut req), c, "the picture stays put as the part changes");
        click_at(&ctx, &mut req, at(0.5, 0.5), egui::PointerButton::Primary);
        click_at(&ctx, &mut req, at(0.5, 0.1), egui::PointerButton::Secondary);
        let all = read(&root, "moss_toad").unwrap().bones;
        assert_eq!(all.keys().collect::<Vec<_>>(), ["front.png", "side.png"]);
        assert_eq!(all["side.png"], [Bone { part: "ear".into(), name: String::new(), points: vec![[50, 50], [50, 10]] }]);
        // Clear empties a picture; a picture taken out takes its bones with it
        click(&ctx, &mut req, "uiRequestBonesClear");
        assert!(!read(&root, "moss_toad").unwrap().bones.contains_key("side.png"));
        let mut r = read(&root, "moss_toad").unwrap();
        remove_picture(&root, &mut r, "front.png").unwrap();
        assert!(r.bones.is_empty(), "{:?}", r.bones);
        // the window goes with the Requests window
        req.open = false;
        frame_with(&ctx, &mut req, vec![]);
        assert!(!req.desk().bones.open);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A look by eye at the queue over the list: one being made, three queued (one gone on from
    /// its lines, one made again), one waiting; written to build/shots/requests-queue.png. It
    /// needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_queue() {
        let root = scratch("shot-queue");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        requests_named(&root, &["Moss toad", "Tall tree", "Shrubs", "Stone lantern", "Reed bed"]);
        let mut toad = read(&root, "moss_toad").unwrap();
        toad.status = Status::InProgress;
        toad.progress = Progress { step: "building the model".into(), done: 0.45, started: now_secs() - 240.0, minutes: 12.0 };
        save(&root, &toad).unwrap();
        let mut shrubs = read(&root, "shrubs").unwrap();
        shrubs.progress.step = LINES_READY.into();
        save(&root, &shrubs).unwrap();
        made(&root, "stone_lantern");
        req.desk().reload();
        let d = req.desk();
        d.queue.runs = vec![
            Queued { id: "tall_tree".into(), ..Default::default() },
            Queued { id: "shrubs".into(), how: How::GoOn, ..Default::default() },
            Queued { id: "stone_lantern".into(), how: How::Again, from: Status::Done, ..Default::default() },
        ];
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let content = content::Content::default();
        let mut out = None;
        for _ in 0..40 {
            crate::platform::clear_hooks();
            let mut o = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                shown(ui, &mut req, &content);
            });
            shooter.textures(&mut o.textures_delta);
            out = Some(o);
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/shots/requests-queue.png");
        shooter.shoot(&ctx, out.unwrap().shapes, [1400, 900], &path).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    /// A look by eye at a grass request's panel (its cells, its variants, how they differ) and
    /// at a made one's Make variants: build/shots/requests-grass.png and
    /// requests-variants.png. It needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_grass_and_variants() {
        let root = scratch("shot-grass");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut r = create(&root, "Meadow tuft.png").unwrap();
        add_picture(&root, &mut r, "front.png", &png(60, 40, 3)).unwrap();
        (r.kind, r.height, r.variants, r.vary) = (Kind::Grass, 0.4, 2, "one taller, one a little yellower".into());
        save(&root, &r).unwrap();
        let mut rock = create(&root, "Flat rock.png").unwrap();
        add_picture(&root, &mut rock, "front.png", &png(60, 40, 5)).unwrap();
        (rock.kind, rock.status, rock.prop, rock.variants, rock.vary) = (Kind::Environment, Status::Done, "flat_rock".into(), 3, "smaller, mossier".into());
        rock.variant_props = vec!["flat_rock_variant_2".into()];
        rock.reply = "A flat grey rock, its top worn smooth.".into();
        save(&root, &rock).unwrap();
        req.desk().reload();
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let content = content::Content::default();
        let run = |req: &mut Requests, shooter: &mut crate::shot::Shooter, events: Vec<egui::Event>| {
            crate::platform::clear_hooks();
            let mut o = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| {
                shown(ui, req, &content);
            });
            shooter.textures(&mut o.textures_delta);
            o
        };
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/shots");
        req.desk().selected = Some("meadow_tuft".into());
        let mut out = None;
        for _ in 0..40 {
            out = Some(run(&mut req, &mut shooter, vec![]));
        }
        shooter.shoot(&ctx, out.take().unwrap().shapes, [1400, 900], &dir.join("requests-grass.png")).unwrap();
        // the made one, scrolled down to Make variants
        req.desk().selected = Some("flat_rock".into());
        for _ in 0..10 {
            run(&mut req, &mut shooter, vec![]);
        }
        let at = marked("uiRequestName").unwrap();
        let turn = egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Page, delta: egui::vec2(0.0, -10.0), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE };
        run(&mut req, &mut shooter, vec![egui::Event::PointerMoved(at), turn]);
        for _ in 0..40 {
            out = Some(run(&mut req, &mut shooter, vec![]));
        }
        shooter.shoot(&ctx, out.take().unwrap().shapes, [1400, 900], &dir.join("requests-variants.png")).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    /// A request's card: folded at first, its first row (its fold, its button, the rest of what
    /// it asks for under its "i") and its progress at its foot only; unfolded, what it made by
    /// its whole file's name, its drawings and its models (its line work left out); its foot
    /// still there.
    #[test]
    fn a_card_is_folded_at_first_and_unfolds_to_what_it_made() {
        let root = scratch("cards");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut rock = create(&root, "Flat rock.png").unwrap();
        add_picture(&root, &mut rock, "front.png", &png(60, 40, 5)).unwrap();
        (rock.kind, rock.status, rock.prop, rock.height, rock.style) = (Kind::Environment, Status::Done, "flat_rock".into(), 1.2, Style::Accurate);
        (rock.variants, rock.vary, rock.variant_props) = (1, "mossier".into(), vec!["flat_rock_variant_2".into()]);
        rock.previews = vec!["preview_front.png".into(), "preview_three_quarter.png".into(), "variant_2.png".into()];
        save(&root, &rock).unwrap();
        let mut pot = create(&root, "Clay pot.png").unwrap();
        add_picture(&root, &mut pot, "pot.png", &png(40, 40, 2)).unwrap();
        save(&root, &pot).unwrap();
        req.desk().reload();
        for _ in 0..3 {
            frame_with(&ctx, &mut req, vec![]);
        }
        for id in ["flat_rock", "clay_pot"] {
            assert!(marked(&format!("uiRequestFold_{id}")).is_some() && marked(&format!("uiRequestInfo_{id}")).is_some() && marked(&format!("uiRequestFoot_{id}")).is_some(), "{id}'s first row and its foot");
        }
        assert!(marked("uiRequestStart_clay_pot").is_some(), "its button on its first row");
        assert!(marked("uiRequestMade_flat_rock").is_none(), "folded at first");
        // unfolded: what it made, by its whole file's name; its foot under it still
        click(&ctx, &mut req, "uiRequestFold_flat_rock");
        for _ in 0..20 {
            frame_with(&ctx, &mut req, vec![]);
        }
        let (made, foot) = (marked("uiRequestMade_flat_rock").expect("unfolded"), marked("uiRequestFoot_flat_rock").unwrap());
        assert!(foot.y > made.y, "the progress at its foot: {foot:?} under {made:?}");
        assert!(marked("uiRequestMade_clay_pot").is_none(), "the other one folded still");
        // what's under its "i"; its models, its own render and its variant's
        let said = details(&req.desk().list.iter().find(|r| r.id == "flat_rock").unwrap().clone());
        for line in ["Height: 1.2 cells", "Style: accurate", "Variants: 1: mossier", "A placeholder: made into temporary/", "Folder: private/requests/flat_rock"] {
            assert!(said.lines().any(|l| l == line), "{line:?} in {said}");
        }
        let rock = req.desk().list.iter().find(|r| r.id == "flat_rock").unwrap().clone();
        assert_eq!(
            made_models(&rock),
            vec![("flat_rock".to_string(), Some("preview_three_quarter.png".to_string())), ("flat_rock_variant_2".to_string(), Some("variant_2.png".to_string()))]
        );
        assert!(made_models(&req.desk().list.iter().find(|r| r.id == "clay_pot").unwrap().clone()).is_empty(), "nothing made yet");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A look by eye at the requests' cards: one made (unfolded: its file, its drawing, its
    /// model and variant), one being made, one waiting, one failed, one split into two:
    /// build/shots/requests-cards.png. It needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_request_cards() {
        let root = scratch("shot-cards");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let mut tuft = create(&root, "Tall grass tuft.png").unwrap();
        add_picture(&root, &mut tuft, "tuft.png", &png(80, 60, 3)).unwrap();
        (tuft.kind, tuft.status, tuft.prop, tuft.height, tuft.style) = (Kind::Grass, Status::Done, "tall_grass_tuft".into(), 0.8, Style::Accurate);
        (tuft.variants, tuft.vary, tuft.variant_props) = (1, "a shade warmer".into(), vec!["tall_grass_tuft_variant_2".into()]);
        tuft.previews = vec!["preview_three_quarter.png".into(), "variant_2.png".into()];
        save(&root, &tuft).unwrap();
        for (f, seed) in [("preview_three_quarter.png", 7), ("variant_2.png", 9)] {
            std::fs::write(root.join("private").join("requests").join("tall_grass_tuft").join(f), png(64, 64, seed)).unwrap();
        }
        let mut fox = create(&root, "Ember fox.png").unwrap();
        add_picture(&root, &mut fox, "fox.png", &png(70, 60, 4)).unwrap();
        (fox.kind, fox.status) = (Kind::Character, Status::InProgress);
        fox.progress = Progress { step: "building the model".into(), done: 0.45, started: 0.0, minutes: 12.0 };
        save(&root, &fox).unwrap();
        let mut pot = create(&root, "Clay pot.png").unwrap();
        add_picture(&root, &mut pot, "pot.png", &png(40, 40, 2)).unwrap();
        save(&root, &pot).unwrap();
        let mut well = create(&root, "Stone well.png").unwrap();
        add_picture(&root, &mut well, "well.png", &png(50, 50, 6)).unwrap();
        (well.status, well.progress) = (Status::Failed, Progress { step: "failed: Claude stopped with an error".into(), done: 0.3, ..Default::default() });
        save(&root, &well).unwrap();
        let mut bushes = create(&root, "Round bushes.png").unwrap();
        add_picture(&root, &mut bushes, "bushes.png", &png(90, 50, 8)).unwrap();
        (bushes.kind, bushes.several, bushes.children) = (Kind::Environment, true, vec!["round_bush".into(), "flat_bush".into()]);
        save(&root, &bushes).unwrap();
        req.desk().reload();
        req.desk().selected = Some("tall_grass_tuft".into());
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let content = content::Content::default();
        let mut run = |req: &mut Requests, events: Vec<egui::Event>| {
            crate::platform::clear_hooks();
            let mut o = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| {
                shown(ui, req, &content);
            });
            shooter.textures(&mut o.textures_delta);
            o
        };
        for _ in 0..30 {
            run(&mut req, vec![]);
        }
        let at = marked("uiRequestFold_tall_grass_tuft").unwrap();
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        run(&mut req, vec![egui::Event::PointerMoved(at), button(true)]);
        run(&mut req, vec![button(false)]);
        let mut out = None;
        for _ in 0..40 {
            out = Some(run(&mut req, vec![egui::Event::PointerMoved(egui::pos2(700.0, 880.0))]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/shots/requests-cards.png");
        shooter.shoot(&ctx, out.unwrap().shapes, [1400, 900], &path).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    /// A look by eye at the Outlines window: three bushes on a picture, two outlined with their
    /// heights and cells, the third being outlined: build/shots/requests-outlines.png. It needs
    /// a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_outlines_window() {
        let root = scratch("shot-outlines");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        // three round bushes on a pale ground
        let bushes = [(130.0, 150.0, 90.0, 70.0), (330.0, 170.0, 70.0, 55.0), (500.0, 140.0, 80.0, 95.0)];
        let img = image::RgbaImage::from_fn(640, 280, |x, y| {
            let inside = bushes.iter().any(|&(cx, cy, rx, ry)| ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2) <= 1.0);
            if inside { image::Rgba([70, 140, 60, 255]) } else { image::Rgba([236, 232, 220, 255]) }
        });
        let mut bytes = std::io::Cursor::new(vec![]);
        img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let mut r = create(&root, "Round bushes.png").unwrap();
        let Ok(Added::New(file)) = add_picture(&root, &mut r, "bushes.png", bytes.get_ref()) else { panic!("the picture") };
        let ring = |cx: f32, cy: f32, rx: f32, ry: f32| (0..10).map(|k| {
            let t = k as f32 / 10.0 * std::f32::consts::TAU;
            [(cx + rx * 1.1 * t.cos()) as i32, (cy + ry * 1.1 * t.sin()) as i32]
        }).collect::<Vec<_>>();
        (r.kind, r.several, r.height) = (Kind::Environment, true, 1.2);
        r.outlines.insert(file.clone(), vec![
            Outline { points: ring(130.0, 150.0, 90.0, 70.0), height: 1.2, cells: [2, 2] },
            Outline { points: ring(500.0, 140.0, 80.0, 95.0), height: 1.8, cells: [1, 1] },
        ]);
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some(r.id.clone());
        req.desk().outlines.open = true;
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let content = content::Content::default();
        let mut run = |req: &mut Requests, events: Vec<egui::Event>| {
            crate::platform::clear_hooks();
            let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| {
                shown(ui, req, &content);
            });
            shooter.textures(&mut out.textures_delta);
            out
        };
        for _ in 0..300 {
            run(&mut req, vec![]);
            if crate::platform::hook("requestoutlines").is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        for _ in 0..5 {
            run(&mut req, vec![]);
        }
        let v: Vec<f32> = crate::platform::hook("requestoutlines").unwrap().split(',').map(|n| n.parse().unwrap()).collect();
        let at = |x: f32, y: f32| egui::pos2(v[0] + x / 640.0 * v[2], v[1] + y / 280.0 * v[3]);
        // the third being outlined, its next side following the pointer
        req.desk().outlines.drawing = ring(330.0, 170.0, 70.0, 55.0)[..6].to_vec();
        let mut out = None;
        for _ in 0..20 {
            out = Some(run(&mut req, vec![egui::Event::PointerMoved(at(255.0, 150.0))]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/shots/requests-outlines.png");
        shooter.shoot(&ctx, out.unwrap().shapes, [1400, 900], &path).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    /// A look by eye at the Requests window and its Bones window: a picture of them with bones
    /// drawn and one being drawn (on SHOT_PICTURE when it names a picture, else on one made
    /// here), written to build/shots/requests-bones.png. It needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_bones_window() {
        let root = scratch("shot");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut req = Requests::at(&root);
        req.open = true;
        let bytes = std::env::var("SHOT_PICTURE").ok().map(|p| std::fs::read(p).unwrap()).unwrap_or_else(|| png(682, 852, 140));
        let mut r = create(&root, "Moss toad.png").unwrap();
        let Ok(Added::New(file)) = add_picture(&root, &mut r, "front.png", &bytes) else { panic!("the picture") };
        let size = image::load_from_memory(&bytes).unwrap();
        let (w, h) = (size.width() as f32, size.height() as f32);
        // where a bear's bones would be, in fractions of the picture
        let line = |part: &str, name: &str, pts: &[[f32; 2]]| Bone { part: part.into(), name: name.into(), points: pts.iter().map(|p| [(p[0] * w) as i32, (p[1] * h) as i32]).collect() };
        r.bones.insert(
            file.clone(),
            vec![
                line("torso", "", &[[0.557, 0.763], [0.55, 0.61], [0.535, 0.469]]),
                line("head", "", &[[0.535, 0.469], [0.564, 0.293]]),
                line("arm", "", &[[0.44, 0.469], [0.345, 0.376], [0.279, 0.235]]),
                line("arm", "", &[[0.645, 0.493], [0.667, 0.616], [0.557, 0.692]]),
                line("leg", "", &[[0.469, 0.775], [0.44, 0.845], [0.44, 0.933]]),
                line("leg", "", &[[0.645, 0.775], [0.674, 0.845], [0.689, 0.933]]),
                line("ear", "", &[[0.469, 0.205], [0.455, 0.15]]),
                line("ear", "", &[[0.733, 0.235], [0.799, 0.217]]),
                line("tail", "", &[[0.733, 0.704], [0.77, 0.622]]),
                line("misc", "chest fluff", &[[0.484, 0.469], [0.557, 0.534]]),
            ],
        );
        save(&root, &r).unwrap();
        req.desk().reload();
        req.desk().selected = Some(r.id.clone());
        req.desk().bones.open = true;
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let content = content::Content::default();
        let mut run = |req: &mut Requests, events: Vec<egui::Event>| {
            crate::platform::clear_hooks();
            let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| {
                shown(ui, req, &content);
            });
            shooter.textures(&mut out.textures_delta);
            out
        };
        // the picture read, the window settled
        let mut c = None;
        for _ in 0..300 {
            run(&mut req, vec![]);
            if let Some(at) = crate::platform::hook("requestbones") {
                c = Some(at);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        for _ in 0..5 {
            run(&mut req, vec![]);
        }
        let v: Vec<f32> = crate::platform::hook("requestbones").or(c).unwrap().split(',').map(|n| n.parse().unwrap()).collect();
        let at = |u: f32, t: f32| egui::pos2(v[0] + u * v[2], v[1] + t * v[3]);
        // a misc piece being drawn, named, its next bone following the pointer
        let d = req.desk();
        d.bones.part = "misc".into();
        d.bones.name = "crest".into();
        d.bones.drawing = vec![[(0.53 * w) as i32, (0.16 * h) as i32], [(0.6 * w) as i32, (0.12 * h) as i32]];
        run(&mut req, vec![egui::Event::PointerMoved(at(0.66, 0.14))]);
        let out = run(&mut req, vec![]);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/shots/requests-bones.png");
        shooter.shoot(&ctx, out.shapes, [1400, 900], &path).unwrap();
        println!("{}", path.display());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_drawings_say_what_they_show() {
        assert_eq!(drawing_about("lines_front.png"), "Claude's lines over front");
        assert_eq!(drawing_about("match_side_view.png"), "side_view's lines over the model, seen from where side_view was drawn");
        assert!(drawing_about("profiles.png").starts_with("The front and the side"));
        assert_eq!(drawing_about("fit_side.png"), "The side's lines over the model's side");
    }

    #[test]
    fn drops_go_to_the_window_only_while_it_is_open() {
        let root = scratch("routing");
        let mut ed = crate::mapedit::MapEditor::new(Arc::new(content::Content::default()), None, crate::platform::Files::new(root.to_str().unwrap()));
        ed.requests = Requests::at(&root);
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let drop = |ed: &mut crate::mapedit::MapEditor, files: Vec<egui::DroppedFileHandle>| {
            let raw = egui::RawInput { dropped_files: files, ..Default::default() };
            let content = content::Content::default();
            let mut out = ctx.run_ui(raw, |ui| {
                ed.take_drops(ui.ctx());
                shown(ui, &mut ed.requests, &content);
            });
            out.textures_delta.clear();
        };
        // closed: a picture goes to the asset browser, which says what it takes
        drop(&mut ed, vec![dropped("front.png", png(10, 10, 3))]);
        assert!(ed.assets.open && ed.requests.waiting() == 0 && ed.status.contains("Requests"), "{}", ed.status);
        // open: pictures to it, anything else still to the asset browser
        ed.assets.open = false;
        ed.requests.open = true;
        drop(&mut ed, vec![dropped("front.png", png(10, 10, 3)), dropped("readme.txt", b"hello".to_vec())]);
        assert_eq!(ed.requests.waiting(), 1);
        assert!(read(&root, "front").unwrap().images == ["front.png"]);
        assert!(ed.assets.open && ed.status.contains("readme.txt"), "{}", ed.status);
        std::fs::remove_dir_all(&root).ok();
    }
}
