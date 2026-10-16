//! The map editor: a map seen from above at an angle (as the game will show it), the level
//! being worked on, and tools to paint the ground, place props, lay out tree areas and
//! build buildings (`build.rs`).
//! Maps are content records (content/data/maps/<id>.json): saved to disk on the desktop,
//! downloaded in the browser.
//!
//! Mouse: left uses the tool, right-drag pans, middle-drag turns the view, the wheel zooms.
//! Keys: 1-6 tools, PageUp/PageDown (or + and -) the level, Q/E turn (Shift: 90°), Delete,
//! Ctrl+Z / Ctrl+Y, Ctrl+S, G the grid, arrows pan, Home recentres. Walk (`walk.rs`) puts a
//! character on the map to try it on foot.

mod actors;
mod assets;
mod build;
mod events;
mod models;
mod requests;
mod rooms;
mod sounds;
mod stream;
mod templates;
mod walk;
mod worlds;
mod zones;

use std::sync::Arc;

use content::{Chunk, Content, Placed, Space, TreeArea, CHUNK};
use egui::{Color32, PointerButton, RichText, Sense};
use glam::{Mat4, Vec2, Vec3, Vec4};
use render::map::MapVertex;
use render::scene::{Frame, Glow, Shape, ShapeKind, MAX_LIGHTS};
use rules::config::Period;
use world::mesh;
use world::WorldMap;

pub use events::Request;
#[cfg(not(target_arch = "wasm32"))]
pub use models::{link_target, open_in_blender};
#[cfg(windows)]
pub use models::register_links;

/// The story as it's kept between runs: where the player stands, and the flags.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SaveGame {
    pub map: String,
    pub at: [i32; 2],
    pub facing: f64,
    /// the floor of the building there (0: the ground)
    pub floor: usize,
    pub flags: std::collections::BTreeMap<String, i64>,
}

/// The save's name (`platform::store_text`).
pub const SAVE_NAME: &str = "story";

use crate::stage::period_lighting;

const TEMP: Color32 = Color32::from_rgb(240, 170, 60);
const OWN: Color32 = Color32::from_rgb(110, 210, 130);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Select,
    Ground,
    Place,
    Trees,
    Build,
    Zones,
    Events,
}

impl Tool {
    const ALL: [(Tool, &'static str, &'static str); 7] = [
        (Tool::Select, "Select", "Pick a prop to move (drag), turn (Q/E), scale or delete it."),
        (Tool::Ground, "Ground", "Paint terrain; the borders draw themselves. With \"set the level\", painted cells rise or sink to the level you chose."),
        (Tool::Place, "Place", "Put a prop from the database at the chosen level. Shift: off the grid."),
        (Tool::Trees, "Trees", "Paint tree areas: hard edges the player can't cross, filled with trees from a few settings."),
        (Tool::Build, "Build", "Draw a building's footprint, then its floors, doors, windows, inside, walls and roof, and confirm it."),
        (Tool::Zones, "Zones", "Paint the areas creatures roam, say who lives there by time of day, and drag links from treelines to zones."),
        (Tool::Events, "Events", "Put actors on the map (people, creatures, things with ways of their own) and events on cells, each with a script: dialog, choices, moves, the camera, sounds, flags, warps, battles."),
    ];

    fn key(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Ground => "ground",
            Tool::Place => "place",
            Tool::Trees => "trees",
            Tool::Build => "build",
            Tool::Zones => "zones",
            Tool::Events => "events",
        }
    }
}

struct Camera {
    target: Vec3,
    /// degrees; 0 looks north (the camera south of what it looks at)
    yaw: f32,
    /// degrees above the ground
    pitch: f32,
    distance: f32,
    /// closer for a while (inside a building): times the distance
    zoom: f32,
}

impl Camera {
    fn eye(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        self.target + Vec3::new(y.sin() * p.cos(), p.sin(), y.cos() * p.cos()) * self.distance * self.zoom
    }

    fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        Mat4::perspective_rh(30f32.to_radians(), aspect.max(0.1), 0.1, 600.0) * view
    }

    /// Along the ground: right and forward (away from the camera).
    fn ground_axes(&self) -> (Vec3, Vec3) {
        let y = self.yaw.to_radians();
        (Vec3::new(y.cos(), 0.0, -y.sin()), Vec3::new(-y.sin(), 0.0, -y.cos()))
    }
}

struct NewMap {
    id: String,
    name: String,
    width: u16,
    height: u16,
    terrain: String,
    /// stored as chunk files
    chunked: bool,
}

/// A chunked map being opened: its chunk files are on their way.
struct Loading {
    record: content::Map,
    space: Space,
    paths: Vec<String>,
}

pub struct MapEditor {
    content: Arc<Content>,
    level_height: f32,
    max_level: u8,
    map: WorldMap,
    /// where the map came from (a new one is the game's own)
    space: Space,
    dirty: bool,
    cam: Camera,
    period: Period,
    grid: bool,
    /// the lamps lit whatever the time (a preview)
    lamps: bool,
    tool: Tool,
    /// the level tools work on
    level: u8,
    /// 0: one cell, 1: 3x3, 2: 5x5...
    brush: i32,
    terrain: String,
    set_level: bool,
    prop: Option<String>,
    turn: f32,
    prop_filter: String,
    selected: Option<usize>,
    area: Option<usize>,
    remove_cells: bool,
    /// under the pointer: on the chosen level's plane, and on the ground itself
    plane_hit: Option<Vec3>,
    ground_hit: Option<(i32, i32)>,
    /// moving a prop: where it was grabbed, from its centre
    grab: Option<Vec2>,
    stroke: bool,
    stroke_ground: bool,
    /// the last cell a stroke painted (strokes fill the cells between two frames)
    last_cell: Option<(i32, i32)>,
    undo: Vec<WorldMap>,
    redo: Vec<WorldMap>,
    need_ground: bool,
    need_cells: bool,
    need_props: bool,
    need_trees: bool,
    need_buildings: bool,
    /// the world map on the GPU, square by square
    gpu: stream::GpuState,
    /// a chunked map whose chunk files are on their way
    loading: Option<Loading>,
    /// the World view
    worlds: worlds::Worlds,
    /// a walker coming from the map next to this one: where it stands once the map is up
    arrive: Option<((i32, i32), f32)>,
    /// props' real models, loaded (by prop id), and the ones that wouldn't load
    models: world::mesh::Models,
    model_failed: std::collections::HashSet<String>,
    /// the building being drawn or designed
    draft: Option<build::Draft>,
    selected_building: Option<usize>,
    /// the cells lit while a building is designed, as last sent to the GPU
    focus_sent: Option<Vec<(i32, i32)>>,
    /// the asset browser
    assets: assets::Assets,
    /// pictures for Claude to model in Blender
    requests: requests::Requests,
    /// a world point the checks want on the screen (`point:x,y,z`)
    probe: Option<Vec3>,
    walker: Option<walk::Walker>,
    /// a building's upper part fading as the walker goes in or out
    fade: Option<walk::Fade>,
    /// which cells block on foot (until the map changes)
    blocked: Option<Vec<bool>>,
    /// the moving things' mesh must be sent again
    extra_changed: bool,
    files: crate::platform::Files,
    zone_sel: Option<usize>,
    /// the Zones tool links treelines (else it paints zones)
    linking: bool,
    link_drag: Option<Vec<(i32, i32)>>,
    hover_treeline: Vec<(i32, i32)>,
    zones_changed: bool,
    spawner: world::spawn::Spawner,
    /// who lives in the zones (the map's spawns record)
    spawns: content::Spawns,
    creatures_on: bool,
    debug: bool,
    /// creature pictures: loaded (size) or missing
    sprites: std::collections::HashMap<String, Option<(u32, u32)>>,
    sprites_wanted: Vec<String>,
    last_tick: f64,
    pool_period: String,
    creature_filter: String,
    last_ray: Option<(Vec3, Vec3)>,
    status: String,
    new_map: Option<NewMap>,
    width_edit: u16,
    height_edit: u16,
    view_proj: Mat4,
    viewport: egui::Rect,
    /// the actors alive on the map
    live: Vec<actors::Live>,
    /// the story: a running script, the flags, the dialog
    story: events::Story,
    /// the Events tool's state
    evt: events::Panel,
    sounds_on: bool,
    animate: bool,
    /// rigged props posed this frame
    animated_n: usize,
    /// this frame's time, for the scripts' host
    now: f64,
    /// templates: capturing one, the list, what's dragged over the map
    tpl: templates::Templates,
    /// the game's overworld: the map walked, no editing (`play_ui`)
    play: bool,
}

fn gpu(v: &[mesh::Vertex]) -> Vec<MapVertex> {
    v.iter().map(|v| MapVertex { pos: v.pos, normal: v.normal, uv: v.uv, color: v.color }).collect()
}

fn hex(c: &str) -> Color32 {
    let v = u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0xff00ff);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Where a widget is, for the checks (`<body data-ui-...>`, "x,y" in CSS pixels).
fn mark(name: &str, r: &egui::Response) {
    let c = r.rect.center();
    crate::platform::set_hook(name, &format!("{:.0},{:.0}", c.x, c.y));
}

// The panels' look, shared by every tool: sections, hints, warnings, swatches, tags and
// pickers, so the editor reads the same everywhere.

/// A section's title in a panel.
fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text.to_uppercase()).size(10.5).strong().color(Color32::from_gray(150)));
    ui.add_space(2.0);
}

/// A quiet line under a control: what it does, or a key.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(Color32::from_gray(140)));
}

/// Something that needs attention.
fn warn(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(Color32::from_rgb(255, 170, 90)));
}

/// A square of colour.
fn swatch(ui: &mut egui::Ui, color: &str) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
    ui.painter().rect_filled(r, 3.0, hex(color));
}

/// The space a record lives in, as a tag.
fn badge(ui: &mut egui::Ui, space: Space) {
    let (text, color) = match space {
        Space::Temporary => ("TEMPORARY", TEMP),
        Space::Content => ("GAME", OWN),
    };
    ui.label(RichText::new(text).size(9.5).strong().color(Color32::BLACK).background_color(color));
}

/// A drop-down of (id, name, colour) choices, the chosen one's swatch next to it; true when
/// the choice changed.
fn pick(ui: &mut egui::Ui, salt: &str, current: &mut String, choices: &[(String, String, String)]) -> bool {
    let chosen = choices.iter().find(|(id, _, _)| id == current);
    let shown = chosen.map(|(_, n, _)| n.clone()).unwrap_or_else(|| if current.is_empty() { "none".to_string() } else { current.clone() });
    let color = chosen.map(|(_, _, c)| c.clone());
    let mut changed = false;
    ui.horizontal(|ui| {
        if let Some(c) = &color {
            swatch(ui, c);
        }
        egui::ComboBox::from_id_salt(salt).selected_text(shown).width(150.0).show_ui(ui, |ui| {
            for (id, name, c) in choices {
                ui.horizontal(|ui| {
                    swatch(ui, c);
                    changed |= ui.selectable_value(current, id.clone(), name).changed();
                });
            }
        });
    });
    changed
}

fn slug_ok(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

const AREA_COLORS: [[f32; 4]; 6] = [
    [0.15, 0.55, 0.2, 0.35],
    [0.2, 0.4, 0.75, 0.35],
    [0.75, 0.45, 0.15, 0.35],
    [0.6, 0.2, 0.6, 0.35],
    [0.2, 0.65, 0.65, 0.35],
    [0.7, 0.7, 0.2, 0.35],
];

impl MapEditor {
    pub fn new(content: Arc<Content>, open: Option<&str>, files: crate::platform::Files) -> MapEditor {
        let world = content.world();
        let entry = open.and_then(|id| content.maps.entry(id)).or_else(|| content.maps.entries().first());
        let (map, space) = match entry {
            Some(e) => (WorldMap::from_record(&e.value), e.space),
            None => (WorldMap::new("new_map", "New map", 32, 24, Self::first_terrain(&content).as_str()), Space::Content),
        };
        let terrain = Self::first_terrain(&content);
        let prop = content.props.iter().next().map(|p| p.id.clone());
        // a chunked map's cells come from its files: opened once the editor stands
        let chunked = entry.filter(|e| e.value.chunked()).map(|e| e.value.id.clone());
        let mut ed = MapEditor {
            level_height: world.level_height() as f32,
            max_level: world.max_level,
            cam: Camera { target: Vec3::ZERO, yaw: 0.0, pitch: 52.0, distance: 40.0, zoom: 1.0 },
            period: Period::Day,
            grid: true,
            lamps: false,
            tool: Tool::Ground,
            level: 0,
            brush: 0,
            terrain,
            set_level: true,
            prop,
            turn: 0.0,
            prop_filter: String::new(),
            selected: None,
            area: None,
            remove_cells: false,
            plane_hit: None,
            ground_hit: None,
            grab: None,
            stroke: false,
            stroke_ground: false,
            last_cell: None,
            undo: vec![],
            redo: vec![],
            need_ground: true,
            need_cells: false,
            need_props: true,
            need_trees: true,
            need_buildings: true,
            gpu: Default::default(),
            loading: None,
            worlds: worlds::Worlds::default(),
            arrive: None,
            models: world::mesh::Models::new(),
            model_failed: std::collections::HashSet::new(),
            draft: None,
            selected_building: None,
            focus_sent: None,
            assets: assets::Assets::default(),
            requests: requests::Requests::default(),
            probe: None,
            walker: None,
            fade: None,
            blocked: None,
            extra_changed: false,
            files,
            zone_sel: None,
            linking: false,
            link_drag: None,
            hover_treeline: vec![],
            zones_changed: false,
            spawner: world::spawn::Spawner::new(7),
            spawns: content::Spawns::default(),
            creatures_on: true,
            debug: false,
            sprites: std::collections::HashMap::new(),
            sprites_wanted: vec![],
            last_tick: 0.0,
            pool_period: "day".into(),
            creature_filter: String::new(),
            last_ray: None,
            status: String::new(),
            new_map: None,
            width_edit: map.width as u16,
            height_edit: map.height as u16,
            view_proj: Mat4::IDENTITY,
            viewport: egui::Rect::NOTHING,
            live: vec![],
            story: Default::default(),
            evt: Default::default(),
            sounds_on: true,
            animate: true,
            animated_n: 0,
            now: 0.0,
            tpl: Default::default(),
            play: false,
            dirty: false,
            space,
            map,
            content,
        };
        ed.map.know_doors(&ed.content);
        ed.load_spawns();
        ed.recentre();
        if let Some(id) = chunked {
            ed.open(&id);
        }
        ed
    }

    /// The game's overworld on a map: walking from where the save left off (on that map),
    /// else from the middle, with the story's flags.
    pub fn new_play(content: Arc<Content>, map: &str, files: crate::platform::Files, save: Option<&SaveGame>) -> MapEditor {
        let mut ed = MapEditor::new(content, Some(map), files);
        ed.play = true;
        ed.creatures_on = true;
        ed.grid = false;
        if let Some(s) = save {
            ed.story.flags = s.flags.clone();
            if s.map == ed.map.id && ed.map.index(s.at[0], s.at[1]).is_some() {
                ed.arrive = Some(((s.at[0], s.at[1]), s.facing as f32));
            }
        }
        if ed.loading.is_none() {
            // a map held as rows is up already: stand on it now
            ed.finish_arrival();
            if ed.walker.is_none() {
                ed.start_walk();
            }
            if let Some(s) = save.filter(|s| s.map == ed.map.id) {
                ed.set_walker_floor(s.floor);
            }
            ed.play_camera();
        }
        ed
    }

    pub fn playing(&self) -> bool {
        self.play
    }

    /// The overworld's view: close, over the walker's shoulder.
    fn play_camera(&mut self) {
        self.cam.distance = 16.0;
        self.cam.pitch = 50.0;
        self.cam.yaw = 0.0;
        if let Some(w) = &self.walker {
            self.cam.target = w.pos(self.now);
        }
    }

    /// The story as it stands, to keep between runs.
    pub fn save_game(&self) -> SaveGame {
        let (at, facing) = match &self.walker {
            Some(w) => ([w.cell.0, w.cell.1], w.facing as f64),
            None => ([self.cam.target.x.floor() as i32, self.cam.target.z.floor() as i32], 0.0),
        };
        SaveGame { map: self.map.id.clone(), at, facing, floor: self.walker_floor(), flags: self.story.flags.clone() }
    }

    fn first_terrain(content: &Content) -> String {
        content.terrains.iter().next().map(|t| t.id.clone()).unwrap_or_else(|| "grass".into())
    }

    /// The content with what was saved here, for the rest of the game.
    pub fn content(&self) -> Arc<Content> {
        self.content.clone()
    }

    /// A world point whose place on the screen the checks read (`<body data-point>`).
    pub fn probe(&mut self, args: &str) {
        let v: Vec<f32> = args.split(',').filter_map(|a| a.trim().parse().ok()).collect();
        self.probe = (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]));
    }

    /// The time of day to preview (tests and screenshots): "morning", "day" or "night".
    pub fn set_period(&mut self, arg: &str) {
        self.period = match arg {
            "morning" => Period::Morning,
            "night" => Period::Night,
            _ => Period::Day,
        };
    }

    /// Points the camera (tests and screenshots): "x,z[,distance[,yaw[,pitch]]]".
    /// The Requests window open (`--requests`, `<body data-cmd="requests">`).
    pub fn open_requests(&mut self) {
        self.requests.open = true;
    }

    pub fn set_view(&mut self, args: &str) {
        let v: Vec<f32> = args.split(',').filter_map(|a| a.trim().parse().ok()).collect();
        if v.len() >= 2 {
            self.cam.target = Vec3::new(v[0], self.level as f32 * self.level_height, v[1]);
        }
        if let Some(&d) = v.get(2) {
            self.cam.distance = d.clamp(5.0, 220.0);
        }
        if let Some(&y) = v.get(3) {
            self.cam.yaw = y;
        }
        if let Some(&p) = v.get(4) {
            self.cam.pitch = p.clamp(15.0, 89.0);
        }
    }

    fn recentre(&mut self) {
        let (w, h) = (self.map.width as f32, self.map.height as f32);
        self.cam.target = Vec3::new(w / 2.0, self.level as f32 * self.level_height, h / 2.0);
        self.cam.distance = (w.max(h * 1.4) * 1.25).clamp(12.0, 160.0);
        self.cam.yaw = 0.0;
        self.cam.pitch = 52.0;
    }

    fn open(&mut self, id: &str) {
        let Some(e) = self.content.maps.entry(id) else { return };
        if e.value.chunked() {
            // the cells are in chunk files: fetched, then `poll_loading` opens the map
            let (gx, gz) = e.value.chunk_grid();
            let paths: Vec<String> = (0..gz).flat_map(|z| (0..gx).map(move |x| (x, z))).map(|(x, z)| format!("{}/{}", e.space.dir(), Chunk::path(&e.value.id, x, z))).collect();
            for p in &paths {
                self.files.request(p);
            }
            self.status = format!("Loading {}: {} chunk files", e.value.name, paths.len());
            self.loading = Some(Loading { record: e.value.clone(), space: e.space, paths });
            return;
        }
        self.map = WorldMap::from_record(&e.value);
        self.space = e.space;
        self.after_load();
    }

    /// A chunked map being opened: once every chunk file is here (or isn't there, a blank
    /// square), it's up.
    fn poll_loading(&mut self) {
        let Some(l) = &self.loading else { return };
        if l.paths.iter().any(|p| matches!(self.files.get(p), crate::platform::Load::Pending)) {
            return;
        }
        let l = self.loading.take().unwrap();
        let mut chunks = vec![];
        let mut bad = vec![];
        for p in &l.paths {
            if let crate::platform::Load::Ready(bytes) = self.files.get(p) {
                match serde_json::from_slice::<Chunk>(&bytes) {
                    Ok(c) => chunks.push(c),
                    Err(e) => bad.push(format!("{p}: {e}")),
                }
            }
        }
        self.map = WorldMap::from_chunks(&l.record, &chunks);
        self.space = l.space;
        self.after_load();
        self.status = match bad.is_empty() {
            true => format!("{}: {} chunk files", self.map.name, chunks.len()),
            false => format!("{}: {} chunk files; unreadable: {}", self.map.name, chunks.len(), bad.join("; ")),
        };
    }

    fn after_load(&mut self) {
        self.map.know_doors(&self.content);
        self.load_spawns();
        self.zone_sel = None;
        self.walker = None;
        self.extra_changed = true;
        self.draft = None;
        self.selected_building = None;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.area = None;
        self.width_edit = self.map.width as u16;
        self.height_edit = self.map.height as u16;
        self.worlds.levels.clear();
        let flags = std::mem::take(&mut self.story.flags);
        self.story = Default::default();
        self.story.flags = flags;
        self.evt = Default::default();
        self.rebuild_actors();
        self.everything_changed();
        self.recentre();
        self.finish_arrival();
        if self.play {
            if self.walker.is_none() {
                self.start_walk();
            }
            self.play_camera();
        }
    }

    fn everything_changed(&mut self) {
        self.need_ground = true;
        self.need_props = true;
        self.need_trees = true;
        self.need_buildings = true;
    }

    /// Before a change: what Ctrl+Z goes back to.
    fn checkpoint(&mut self) {
        self.undo.push(self.map.clone());
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// After undo or redo: what was selected stays selected if it's still there.
    fn keep_selection(&mut self) {
        self.spawner.reset();
        self.zone_sel = self.zone_sel.filter(|&i| i < self.map.zones.len());
        self.selected = self.selected.filter(|&i| i < self.map.props.len());
        self.selected_building = self.selected_building.filter(|&i| i < self.map.buildings.len());
        self.evt.actor_sel = self.evt.actor_sel.filter(|&i| i < self.map.actors.len());
        self.evt.event_sel = self.evt.event_sel.filter(|&r| self.event(r).is_some());
        self.rebuild_actors();
    }

    fn undo(&mut self) {
        if let Some(m) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.map, m));
            self.keep_selection();
            self.everything_changed();
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        if let Some(m) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.map, m));
            self.keep_selection();
            self.everything_changed();
            self.dirty = true;
        }
    }

    fn save(&mut self) {
        let (record, chunks) = self.map.records();
        if !slug_ok(&record.id) {
            self.status = format!("Not saved: the id \"{}\" must be lowercase letters, digits, _ or -", record.id);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            let written = content::disk::save(&root, Space::Content, &record).and_then(|path| {
                if record.chunked() {
                    content::disk::save_chunks(&root, Space::Content, &record, &chunks).map(|(w, gone)| format!("Saved {} and {w} chunk file{} ({gone} removed)", path.display(), if w == 1 { "" } else { "s" }))
                } else {
                    let gone = content::disk::remove_chunks(&root, Space::Content, &record.id);
                    Ok(if gone > 0 { format!("Saved {} as rows; {gone} chunk files removed", path.display()) } else { format!("Saved {}", path.display()) })
                }
            });
            match written {
                Ok(msg) => self.status = msg,
                Err(e) => {
                    self.status = format!("Not saved: {e}");
                    return;
                }
            }
            // the next opening reads the files as they are now
            let (gx, gz) = record.chunk_grid();
            for z in 0..gz {
                for x in 0..gx {
                    self.files.forget(&format!("{}/{}", Space::Content.dir(), Chunk::path(&record.id, x, z)));
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let name = format!("{}.json", record.id);
            let mut result = crate::platform::download(&name, &content::to_text(&record));
            for c in &chunks {
                if result.is_ok() {
                    result = crate::platform::download(&format!("{}_{}.json", c.chunk[0], c.chunk[1]), &content::to_text(c));
                }
            }
            match result {
                Ok(()) if chunks.is_empty() => self.status = format!("Downloaded {name}: put it in content/data/maps/"),
                Ok(()) => self.status = format!("Downloaded {name} (for content/data/maps/) and {} chunk files (for content/data/maps/{}/)", chunks.len(), record.id),
                Err(e) => {
                    self.status = format!("Not saved: {e}");
                    return;
                }
            }
        }
        let mut c = (*self.content).clone();
        c.maps.put(record, Space::Content);
        c.sort();
        self.content = Arc::new(c);
        self.space = Space::Content;
        self.dirty = false;
        if let Some(note) = self.save_spawns() {
            self.status = format!("{} · {note}", self.status);
        }
        if !self.assets.dirty.is_empty() {
            let map = self.status.clone();
            self.save_assets();
            self.status = format!("{map} · {}", self.status);
        }
        if !self.tpl.dirty.is_empty() {
            let map = self.status.clone();
            self.save_templates();
            self.status = format!("{map} · {}", self.status);
        }
    }

    // ------------------------------------------------------------ the GPU

    /// The cells texture's blue channel: 255 on the cells in the focus.
    fn mark_focus(&self, cells: &mut [u8], focus: Option<&[(i32, i32)]>) {
        let Some(focus) = focus else { return };
        for &(x, z) in focus {
            if let Some(i) = self.map.index(x, z) {
                if let Some(b) = cells.get_mut(i * 4 + 2) {
                    *b = 255;
                }
            }
        }
    }

    // ------------------------------------------------------------ picking

    fn ray(&self, pos: egui::Pos2) -> Option<(Vec3, Vec3)> {
        let r = self.viewport;
        if r.width() < 1.0 || !r.contains(pos) {
            return None;
        }
        let x = (pos.x - r.left()) / r.width() * 2.0 - 1.0;
        let y = 1.0 - (pos.y - r.top()) / r.height() * 2.0;
        let inv = self.view_proj.inverse();
        let near = inv * Vec4::new(x, y, 0.0, 1.0);
        let far = inv * Vec4::new(x, y, 1.0, 1.0);
        let (near, far) = (near.truncate() / near.w, far.truncate() / far.w);
        Some((near, (far - near).normalize()))
    }

    fn plane(origin: Vec3, dir: Vec3, y: f32) -> Option<Vec3> {
        if dir.y.abs() < 1e-5 {
            return None;
        }
        let t = (y - origin.y) / dir.y;
        (t > 0.0).then(|| origin + dir * t)
    }

    /// The first cell top the ray meets.
    fn ground(&self, origin: Vec3, dir: Vec3) -> Option<(i32, i32)> {
        let lh = self.level_height;
        let top = self.map.level.iter().copied().max().unwrap_or(0) as f32 * lh;
        // start where the ray comes down to the highest ground
        let mut t = if origin.y > top && dir.y < 0.0 { (origin.y - top) / -dir.y } else { 0.0 };
        for _ in 0..6000 {
            let p = origin + dir * t;
            if p.y < -1.0 {
                return None;
            }
            let (x, z) = (p.x.floor() as i32, p.z.floor() as i32);
            if let Some(l) = self.map.level_at(x, z) {
                if p.y <= l as f32 * lh + 0.001 {
                    return Some((x, z));
                }
            }
            t += 0.05;
        }
        None
    }

    fn prop_box(&self, p: &Placed) -> Option<(Vec3, Vec3)> {
        let prop = self.content.props.get(&p.prop)?;
        let quarter = ((p.turn / 90.0).round() as i64).rem_euclid(2) == 1;
        let (w, d) = if quarter { (prop.footprint[1], prop.footprint[0]) } else { (prop.footprint[0], prop.footprint[1]) };
        let s = if prop.scalable() { p.scale as f32 } else { 1.0 };
        let half = Vec3::new(w as f32 / 2.0, 0.0, d as f32 / 2.0) * s;
        let pos = p.pos();
        let base = Vec3::new(pos[0] as f32, p.level as f32 * self.level_height, pos[1] as f32);
        Some((base - half, base + half + Vec3::Y * (prop.height as f32 * s).max(0.3)))
    }

    fn pick_prop(&self, origin: Vec3, dir: Vec3) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, p) in self.map.props.iter().enumerate() {
            let Some((lo, hi)) = self.prop_box(p) else { continue };
            let inv = dir.recip();
            let (t1, t2) = ((lo - origin) * inv, (hi - origin) * inv);
            let (tmin, tmax) = (t1.min(t2).max_element(), t1.max(t2).min_element());
            if tmax >= tmin.max(0.0) && best.map(|(t, _)| tmin < t).unwrap_or(true) {
                best = Some((tmin, i));
            }
        }
        best
    }

    /// Where a prop's centre goes for a point: its footprint on whole cells.
    fn snap(&self, prop: &str, turn: f32, p: Vec3) -> [f64; 2] {
        let fp = self.content.props.get(prop).map(|q| q.footprint).unwrap_or([1, 1]);
        let quarter = ((turn / 90.0).round() as i64).rem_euclid(2) == 1;
        let (w, d) = if quarter { (fp[1] as f64, fp[0] as f64) } else { (fp[0] as f64, fp[1] as f64) };
        let x = (p.x as f64 - w / 2.0 + 0.5).floor() + w / 2.0;
        let z = (p.z as f64 - d / 2.0 + 0.5).floor() + d / 2.0;
        [x, z]
    }

    // ------------------------------------------------------------ the tools

    fn brush_cells(&self, centre: (i32, i32)) -> Vec<(i32, i32)> {
        let mut out = vec![];
        for dz in -self.brush..=self.brush {
            for dx in -self.brush..=self.brush {
                if self.map.inside(centre.0 + dx, centre.1 + dz) {
                    out.push((centre.0 + dx, centre.1 + dz));
                }
            }
        }
        out
    }

    /// The cells from the stroke's last one to this one (Bresenham), this one last.
    fn line_to(&mut self, to: (i32, i32)) -> Vec<(i32, i32)> {
        let from = self.last_cell.replace(to).unwrap_or(to);
        let (dx, dz) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
        let (sx, sz) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
        let (mut x, mut z, mut err) = (from.0, from.1, dx + dz);
        let mut out = vec![];
        loop {
            out.push((x, z));
            if (x, z) == to || out.len() > 1024 {
                return out;
            }
            let e2 = 2 * err;
            if e2 >= dz {
                err += dz;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                z += sz;
            }
        }
    }

    fn paint_ground(&mut self, centre: (i32, i32)) {
        let fresh = !self.map.terrains.contains(&self.terrain);
        let t = self.map.terrain_index(&self.terrain);
        let tall = |ed: &MapEditor, i: u16| ed.content.terrains.get(&ed.map.terrains[i as usize]).map(|t| t.pattern == "tall").unwrap_or(false);
        for (x, z) in self.brush_cells(centre) {
            let i = self.map.index(x, z).unwrap();
            let old = self.map.terrain[i];
            if old != t {
                if tall(self, old) || tall(self, t) {
                    self.need_ground = true;
                }
                self.map.terrain[i] = t;
                self.need_cells = true;
                self.stroke_ground = true;
            }
            if self.set_level && self.map.level[i] != self.level {
                self.map.level[i] = self.level;
                self.need_ground = true;
                // what stood on the old ground keeps standing on it
                self.need_trees = true;
            }
        }
        if fresh {
            self.need_ground = true;
        }
    }

    fn paint_area(&mut self, centre: (i32, i32), remove: bool) {
        if !remove && self.area.is_none() {
            self.new_area();
        }
        let mark = if remove { 0 } else { self.area.map(|a| a as u8 + 1).unwrap_or(0) };
        for (x, z) in self.brush_cells(centre) {
            let i = self.map.index(x, z).unwrap();
            if self.map.area[i] != mark && (remove || mark > 0) {
                self.map.area[i] = mark;
                self.need_trees = true;
            }
        }
    }

    fn new_area(&mut self) {
        let trees: Vec<String> = self.content.props.iter().filter(|p| p.category == "tree").map(|p| p.id.clone()).collect();
        let n = self.map.tree_areas.len() + 1;
        let seed = (crate::platform::now_ms() as u32).wrapping_mul(2654435761) % 100_000;
        self.map.tree_areas.push(TreeArea { mark: self.map.free_mark(), name: format!("Trees {n}"), trees, seed, ..Default::default() });
        self.area = Some(self.map.tree_areas.len() - 1);
    }

    /// How far a placed prop may stand from its cells' centre ([least x, z], [most x, z]);
    /// None when it fills them.
    fn prop_room(&self, i: usize) -> Option<([f64; 2], [f64; 2])> {
        let p = self.map.props.get(i)?;
        let prop = self.content.props.get(&p.prop)?;
        let scale = if prop.scalable() { p.scale } else { 1.0 };
        let (lo, hi) = world::mesh::offset_room(prop, &self.models, p.turn, scale);
        (hi[0] - lo[0] > 0.02 || hi[1] - lo[1] > 0.02).then_some((lo, hi))
    }

    /// After a turn or a new size: back on whole cells (a quarter turn of a long prop moves
    /// it by half a cell), and within them.
    fn reseat(&mut self, i: usize) {
        let Some(p) = self.map.props.get(i).cloned() else { return };
        let at = self.snap(&p.prop, p.turn as f32, Vec3::new(p.at[0] as f32, 0.0, p.at[1] as f32));
        let (lo, hi) = self.prop_room(i).unwrap_or(([0.0; 2], [0.0; 2]));
        let q = &mut self.map.props[i];
        q.at = at;
        q.offset = [q.offset[0].clamp(lo[0], hi[0]), q.offset[1].clamp(lo[1], hi[1])];
        self.need_props = true;
    }

    fn turn_selected(&mut self, by: f32) {
        if let Some(i) = self.selected {
            self.checkpoint();
            let p = &mut self.map.props[i];
            p.turn = ((p.turn as f32 + by).rem_euclid(360.0)) as f64;
            self.reseat(i);
        }
    }

    fn delete_selected(&mut self) {
        if let Some(i) = self.selected.take() {
            if i < self.map.props.len() {
                self.checkpoint();
                self.map.props.remove(i);
                self.need_props = true;
            }
        }
    }

    // ------------------------------------------------------------ the screen

    /// Draws the editor; true when the player asks to go back.
    /// What every frame does first, editing or playing: walking, the creatures, the actors,
    /// the story, the GPU, a map on its way.
    fn begin_frame(&mut self, ctx: &egui::Context, rs: Option<&eframe::egui_wgpu::RenderState>, now: f64) {
        self.now = now;
        self.walk(ctx, now);
        self.tick(now);
        self.tick_actors(now);
        self.step_story(ctx, now);
        if let Some(rs) = rs {
            self.sync(rs, now);
        }
        self.poll_loading();
    }

    /// What every frame does last: the dialog box over the view, what's heard, the hooks.
    fn end_frame(&mut self, ctx: &egui::Context, audio: &mut crate::audio::Audio, now: f64) {
        self.dialog_ui(ctx, now);
        let wants = self.sound_wants(now);
        audio.update(&self.files, self.listener(now), &wants, now);
        let shots = std::mem::take(&mut self.story.shots);
        for (file, volume) in shots {
            let space = self.content.maps.entry(&self.map.id).map(|e| e.space).unwrap_or(self.space);
            audio.play_once(&self.files, &space.asset(&file), None, volume, 0.0, now);
        }
        crate::platform::set_hook("sounds", &audio.playing().to_string());
        crate::platform::set_hook("audio", audio.state());
        crate::platform::set_hook("animated", &self.animated_n.to_string());
        crate::platform::set_hook("map", &self.map.id);
        crate::platform::set_hook("loading", if self.loading.is_some() { "1" } else { "0" });
        crate::platform::set_hook("status", &self.status);
    }

    /// The game's overworld: the map walked, full window, no editing. Says whether the
    /// player leaves (Esc, with no dialog up).
    pub fn play_ui(&mut self, ui: &mut egui::Ui, rs: Option<&eframe::egui_wgpu::RenderState>, now: f64, audio: &mut crate::audio::Audio, period: Period) -> bool {
        let ctx = ui.ctx().clone();
        self.period = period;
        self.begin_frame(&ctx, rs, now);
        let leave = !ctx.egui_wants_keyboard_input() && self.story.dialog.is_none() && self.story.machine.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
            self.viewport = rect;
            self.play_input(ui, &response);
            let frame = self.frame(now, rect);
            ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, crate::app::SceneCallback { frame }));
            // the place and the keys, bottom left; what a script said, above
            let p = ui.painter();
            let font = egui::FontId::proportional(12.0);
            p.text(rect.left_bottom() + egui::vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, format!("{} · arrows move · A speaks · Esc leaves", self.map.name), font.clone(), Color32::from_white_alpha(170));
            if self.loading.is_some() {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "Loading…", egui::FontId::proportional(18.0), Color32::from_white_alpha(200));
            }
        });
        self.end_frame(&ctx, audio, now);
        leave
    }

    /// The overworld's pointer: the wheel zooms, the middle button turns the view.
    fn play_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let (scroll, delta) = ui.input(|i| (i.smooth_scroll_delta.y, i.pointer.delta()));
        if response.hovered() && scroll != 0.0 {
            self.cam.distance = (self.cam.distance * (-scroll * 0.0015).exp()).clamp(6.0, 40.0);
        }
        if response.dragged_by(PointerButton::Middle) {
            self.cam.yaw = (self.cam.yaw - delta.x * 0.3).rem_euclid(360.0);
            self.cam.pitch = (self.cam.pitch + delta.y * 0.2).clamp(25.0, 80.0);
        }
        self.plane_hit = None;
        self.ground_hit = None;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, rs: Option<&eframe::egui_wgpu::RenderState>, now: f64, audio: &mut crate::audio::Audio) -> bool {
        let ctx = ui.ctx().clone();
        self.begin_frame(&ctx, rs, now);
        let mut back = false;
        self.take_drops(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.watch_assets(now);
            self.requests.watch(now);
        }
        self.keys(&ctx);
        egui::Panel::top("maps-top").show(ui, |ui| {
            ui.add_space(4.0);
            // one row at the window's first size (1400 wide), a second one when narrower,
            // rather than buttons off the edge
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.spacing_mut().button_padding.x = 3.0;
                ui.label(RichText::new("Maps").size(16.0).strong());
                if ui.button("Back to the game").clicked() {
                    back = true;
                }
                ui.separator();
                self.map_picker(ui);
                ui.separator();
                let (save, about) = if cfg!(target_arch = "wasm32") {
                    ("Download", "Ctrl+S · downloads the map: put it in content/data/maps/")
                } else {
                    ("Save", "Ctrl+S · writes content/data/maps/<id>.json")
                };
                let s = ui.button(if self.dirty { format!("{save} •") } else { save.to_string() }).on_hover_text(about);
                mark("uiSave", &s);
                if s.clicked() {
                    self.save();
                }
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).on_hover_text("Ctrl+Y").clicked() {
                    self.redo();
                }
                ui.separator();
                for (p, label) in [(Period::Morning, "Morning"), (Period::Day, "Day"), (Period::Night, "Night")] {
                    let r = ui.selectable_label(self.period == p, label).on_hover_text("The light to preview");
                    mark(&format!("ui{label}"), &r);
                    if r.clicked() {
                        self.period = p;
                    }
                }
                let lamps = ui.selectable_label(self.lamps, "Lamps").on_hover_text("The lamps lit whatever the time (they light up by night on their own)");
                mark("uiLamps", &lamps);
                if lamps.clicked() {
                    self.lamps = !self.lamps;
                }
                let sounds = ui.selectable_label(self.sounds_on, "Sounds").on_hover_text("The props' and creatures' sounds and the place's ambience, heard from the walker (or the view)");
                mark("uiSounds", &sounds);
                if sounds.clicked() {
                    self.sounds_on = !self.sounds_on;
                }
                let animate = ui.selectable_label(self.animate, "Animate").on_hover_text("Rigged props play their animations");
                mark("uiAnimate", &animate);
                if animate.clicked() {
                    self.animate = !self.animate;
                }
                if ui.selectable_label(self.grid, "Grid").on_hover_text("Cell lines on the ground (G)").clicked() {
                    self.grid = !self.grid;
                }
                if ui.button("Fit").on_hover_text("The whole map in view (Home)").clicked() {
                    self.recentre();
                }
                ui.separator();
                let creatures = ui.selectable_label(self.creatures_on, "Creatures").on_hover_text("Creatures roam their zones");
                mark("uiCreatures", &creatures);
                if creatures.clicked() {
                    self.creatures_on = !self.creatures_on;
                    self.spawner.reset();
                }
                let debug = ui.selectable_label(self.debug, "Debug").on_hover_text("Every zone, tree area, link and creature path");
                mark("uiDebug", &debug);
                if debug.clicked() {
                    self.debug = !self.debug;
                }
                let walk = ui.selectable_label(self.walker.is_some(), "Walk").on_hover_text("Try the map on foot: the arrows move, Esc stops");
                mark("uiWalk", &walk);
                if walk.clicked() {
                    if self.walker.is_some() {
                        self.stop_walk();
                    } else {
                        self.start_walk();
                    }
                }
                let assets = ui.selectable_label(self.assets.open, "Assets").on_hover_text("Every prop by folder; drop .glb files on the window to add them");
                mark("uiAssets", &assets);
                if assets.clicked() {
                    self.assets.open = !self.assets.open;
                }
                self.requests.toggle(ui);
                let worlds = ui.selectable_label(self.worlds.open, "World").on_hover_text("Maps stitched into a world: place this map next to others");
                mark("uiWorld", &worlds);
                if worlds.clicked() {
                    self.worlds.open = !self.worlds.open;
                }
                ui.separator();
                let export = ui.button("To Blender").on_hover_text("The whole map as a .glb: ground, props, trees and buildings as separate objects");
                mark("uiExportMap", &export);
                if export.clicked() {
                    self.export_map();
                }
            });
            ui.add_space(4.0);
        });
        egui::Panel::bottom("maps-status").show(ui, |ui| {
            ui.horizontal(|ui| {
                badge(ui, self.space);
                ui.label(RichText::new(format!("{} · {}×{}", self.map.id, self.map.width, self.map.height)).size(12.0));
                if let Some((x, z)) = self.ground_hit {
                    let t = self.map.terrain_at(x, z).unwrap_or("?");
                    let l = self.map.level_at(x, z).unwrap_or(0);
                    ui.separator();
                    ui.label(RichText::new(format!("{x}, {z} · {t} · level {l}")).size(12.0).color(Color32::from_gray(150)));
                }
                if self.debug {
                    ui.separator();
                    ui.label(RichText::new(self.roamer_summary()).size(12.0).color(Color32::from_rgb(255, 170, 90)));
                }
                if !self.status.is_empty() {
                    ui.separator();
                    // room for the frame readout at the right end
                    ui.scope(|ui| {
                        ui.set_max_width((ui.available_width() - 190.0).max(60.0));
                        ui.add(egui::Label::new(RichText::new(&self.status).size(12.0).color(Color32::LIGHT_GRAY)).truncate());
                    });
                }
            });
        });
        // the asset browser and the World view across the window, under the side panels
        if self.assets.open {
            egui::Panel::bottom("maps-assets").resizable(true).default_size(300.0).min_size(140.0).show(ui, |ui| self.assets_panel(ui));
        }
        if self.worlds.open {
            egui::Panel::bottom("maps-world").resizable(true).default_size(320.0).min_size(160.0).show(ui, |ui| self.worlds_panel(ui));
        }
        egui::Panel::left("maps-tools").exact_size(250.0).show(ui, |ui| {
            ui.add_space(6.0);
            self.toolbar(ui);
            egui::ScrollArea::vertical().show(ui, |ui| self.tools_panel(ui));
        });
        egui::Panel::right("maps-inspector").exact_size(230.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui));
        });
        self.light_window(ui);
        if let Some(id) = self.requests.window(&ctx, &self.content) {
            self.use_prop(&id);
            self.tool = Tool::Place;
            self.status = format!("{id}: the Place tool puts it down");
        }
        if let Some(said) = self.requests.said.take() {
            self.status = said;
        }
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
            self.viewport = rect;
            self.view_input(ui, &response);
            let frame = self.frame(now, rect);
            ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, crate::app::SceneCallback { frame }));
        });
        self.end_frame(&ctx, audio, now);
        crate::platform::set_hook("mapsize", &format!("{}x{}", self.map.width, self.map.height));
        crate::platform::set_hook("chunked", if self.map.chunk > 0 { "1" } else { "0" });
        crate::platform::set_hook("props", &self.map.props.len().to_string());
        crate::platform::set_hook("areas", &self.map.tree_areas.len().to_string());
        crate::platform::set_hook("level", &self.level.to_string());
        crate::platform::set_hook("dirty", if self.dirty { "1" } else { "0" });
        crate::platform::set_hook("buildings", &self.map.buildings.len().to_string());
        crate::platform::set_hook("zones", &self.map.zones.len().to_string());
        crate::platform::set_hook("links", &self.map.links.len().to_string());
        let step = match self.draft.as_ref().map(|d| d.step) {
            Some(build::Step::Footprint) => "footprint",
            Some(build::Step::Design) => "design",
            None => "",
        };
        crate::platform::set_hook("build", step);
        let draft = self.draft.as_ref().map(|d| {
            let f = &d.b.floors[d.floor];
            format!("floors:{};floor:{};openings:{};inside:{};pillars:{}", d.b.floors.len(), d.floor + 1, f.openings.len(), f.props.len(), d.b.pillars.len())
        });
        crate::platform::set_hook("draft", &draft.unwrap_or_default());
        let rooms = self.draft.as_ref().map(|d| {
            let wall = d.wall.map(|w| format!("{},{},{}", w.0, w.1, w.2)).unwrap_or_default();
            let inner = d.b.anchors.iter().filter(|a| a.inside).count();
            let stairs = |id: &str| self.stairs_footprint(id);
            let (holes, roof) = (d.b.stairwells(d.floor, &stairs).len(), d.b.stairwells(d.b.floors.len(), &stairs).len());
            format!("wall:{wall};inner:{inner};outer:{};outside:{};holes:{holes};roof:{roof};sel:{:?}", d.b.anchors.len() - inner, d.b.outside.len(), d.sel)
        });
        crate::platform::set_hook("rooms", &rooms.unwrap_or_default());
        let selection = match (self.selected, self.selected_building) {
            (_, Some(i)) => format!("building:{i}"),
            (Some(i), None) => format!("prop:{i}"),
            _ => String::new(),
        };
        crate::platform::set_hook("selection", &selection);
        let selprop = self.selected.and_then(|i| self.map.props.get(i)).map(|p| format!("{:.2},{:.2};{:.2},{:.2}", p.at[0], p.at[1], p.offset[0], p.offset[1]));
        crate::platform::set_hook("selprop", &selprop.unwrap_or_default());
        if let Some(b) = self.selected_building.and_then(|i| self.map.buildings.get(i)) {
            crate::platform::set_hook("bat", &format!("{},{}", b.at[0], b.at[1]));
            crate::platform::set_hook("bcells", &b.cells.join("/"));
            crate::platform::set_hook("anchors", &b.anchors.len().to_string());
        }
        crate::platform::set_hook("tool", self.tool.key());
        back
    }

    fn map_picker(&mut self, ui: &mut egui::Ui) {
        let ids: Vec<(String, String, Space)> = self.content.maps.entries().iter().map(|e| (e.value.id.clone(), e.value.name.clone(), e.space)).collect();
        let mut open = None;
        egui::ComboBox::from_id_salt("map-pick").selected_text(format!("{} ({})", self.map.name, self.map.id)).show_ui(ui, |ui| {
            for (id, name, space) in &ids {
                let tag = if *space == Space::Temporary { " · temporary" } else { "" };
                if ui.selectable_label(*id == self.map.id, format!("{name} ({id}){tag}")).clicked() {
                    open = Some(id.clone());
                }
            }
        });
        if let Some(id) = open {
            if self.dirty {
                self.status = format!("Unsaved changes on {}: save them, or pick it again to drop them", self.map.id);
                self.dirty = false;
            } else {
                self.open(&id);
            }
        }
        if ui.button("New").on_hover_text("A new map").clicked() {
            self.new_map = Some(NewMap { id: String::new(), name: String::new(), width: 32, height: 24, terrain: self.terrain.clone(), chunked: false });
        }
    }

    /// The tools and the level, at the top of the left panel.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        for row in Tool::ALL.chunks(3) {
            ui.horizontal(|ui| {
                for (tool, name, help) in row {
                    let n = Tool::ALL.iter().position(|t| t.0 == *tool).unwrap_or(0) + 1;
                    let r = ui.add_sized([72.0, 22.0], egui::Button::selectable(self.tool == *tool, *name)).on_hover_text(format!("{n} · {help}"));
                    if r.clicked() {
                        self.tool = *tool;
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding = egui::vec2(5.0, 1.0);
            let about = format!(
                "The level the tools work on: {:.1} cells up. A level is {:.1} cells, the character's height and some room. PageUp / PageDown",
                self.level as f32 * self.level_height,
                self.level_height
            );
            ui.label(RichText::new("Level").size(11.5).color(Color32::from_gray(150))).on_hover_text(&about);
            for l in 0..=self.max_level {
                if ui.selectable_label(self.level == l, RichText::new(l.to_string()).size(11.5)).on_hover_text(&about).clicked() {
                    self.level = l;
                }
            }
        });
        ui.add_space(2.0);
        ui.separator();
    }

    fn tools_panel(&mut self, ui: &mut egui::Ui) {
        if self.new_map.is_some() {
            self.new_map_panel(ui);
        }
        match self.tool {
            Tool::Select => {
                section(ui, "Select");
                hint(ui, "Click a prop or a building: the inspector shows it. Q/E turn it, Delete removes it. Things stay on their cells: a prop that leaves room moves within them (drag it).");
                self.templates_panel(ui);
            }
            Tool::Ground => self.ground_panel(ui),
            Tool::Place => self.place_panel(ui),
            Tool::Trees => self.trees_panel(ui),
            Tool::Build => self.build_panel(ui),
            Tool::Zones => self.zones_panel(ui),
            Tool::Events => self.events_panel(ui),
        }
        ui.add_space(8.0);
    }

    fn new_map_panel(&mut self, ui: &mut egui::Ui) {
        let Some(nm) = &mut self.new_map else { return };
        let mut create = false;
        let mut cancel = false;
        ui.group(|ui| {
            ui.label(RichText::new("New map").strong());
            ui.horizontal(|ui| {
                ui.label("Id");
                ui.text_edit_singleline(&mut nm.id);
            });
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut nm.name);
            });
            ui.horizontal(|ui| {
                ui.label("Size");
                ui.add(egui::DragValue::new(&mut nm.width).range(4..=256));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut nm.height).range(4..=256));
            });
            egui::ComboBox::from_id_salt("new-terrain").selected_text(nm.terrain.clone()).show_ui(ui, |ui| {
                for t in self.content.terrains.iter() {
                    ui.selectable_value(&mut nm.terrain, t.id.clone(), &t.name);
                }
            });
            ui.checkbox(&mut nm.chunked, "Chunk files").on_hover_text(format!("Stored as {CHUNK} x {CHUNK} squares in data/maps/<id>/, for big maps: an edit touches few files"));
            let taken = self.content.maps.contains(&nm.id);
            if taken {
                warn(ui, "That id is taken.");
            } else if !nm.id.is_empty() && !slug_ok(&nm.id) {
                warn(ui, "Ids are lowercase letters, digits, _ and -.");
            }
            ui.horizontal(|ui| {
                create = ui.add_enabled(slug_ok(&nm.id) && !taken, egui::Button::new("Create")).clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if create {
            let name = if nm.name.is_empty() { nm.id.clone() } else { nm.name.clone() };
            self.map = WorldMap::new(&nm.id, &name, nm.width as usize, nm.height as usize, &nm.terrain);
            self.map.chunk = if nm.chunked { CHUNK } else { 0 };
            self.space = Space::Content;
            self.new_map = None;
            self.after_load();
            self.dirty = true;
            self.status = "New map: save it to keep it".into();
        } else if cancel {
            self.new_map = None;
        }
        ui.add_space(8.0);
    }

    fn ground_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Terrain");
        let terrains: Vec<(String, String, String, String)> = self.content.terrains.iter().map(|t| (t.id.clone(), t.name.clone(), t.color.clone(), t.walk.clone())).collect();
        for (id, name, color, walk) in terrains {
            ui.horizontal(|ui| {
                swatch(ui, &color);
                let extra = if walk == "walk" || name.to_lowercase().contains(&walk) { String::new() } else { format!(" · {walk}") };
                let r = ui.selectable_label(self.terrain == id, format!("{name}{extra}"));
                mark(&format!("uiTerrain_{id}"), &r);
                if r.clicked() {
                    self.terrain = id.clone();
                }
            });
        }
        section(ui, "Brush");
        ui.add(egui::Slider::new(&mut self.brush, 0..=4).custom_formatter(|v, _| format!("{0}×{0}", v as i32 * 2 + 1)));
        ui.checkbox(&mut self.set_level, format!("Set the level to {}", self.level)).on_hover_text("Painted cells rise or sink to the chosen level; off: only the terrain changes");
        hint(ui, "Right-click picks a cell's terrain and level.");
    }

    fn place_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(id) = self.prop.clone() {
            section(ui, "Prop");
            let p = self.content.props.get(&id).cloned();
            ui.label(RichText::new(p.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| id.clone())).strong());
            let about = match &p {
                Some(p) => {
                    let model = match &p.model {
                        Some(_) if self.models.contains_key(&id) => "its own model".to_string(),
                        Some(_) => "its own model (not loaded yet)".to_string(),
                        None => format!("a stand-in of {} shape{}", p.parts.len(), if p.parts.len() == 1 { "" } else { "s" }),
                    };
                    format!("{}×{} · {:.1} high · {model}", p.footprint[0], p.footprint[1], p.height)
                }
                None => "no such prop".into(),
            };
            hint(ui, &about);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Turned {:.0}°", self.turn)).size(11.5));
                let export = ui.small_button("To Blender").on_hover_text("A .glb of it, standing at the origin");
                mark("uiExportProp", &export);
                if export.clicked() {
                    self.export_prop(&id);
                }
            });
        }
        section(ui, "Props");
        ui.add(egui::TextEdit::singleline(&mut self.prop_filter).hint_text("Filter").desired_width(f32::INFINITY));
        let f = self.prop_filter.to_lowercase();
        let mut last = String::new();
        let props: Vec<(String, String, String, [u8; 2])> = self.content.props.iter().map(|p| (p.id.clone(), p.name.clone(), p.category.clone(), p.footprint)).collect();
        for (id, name, category, fp) in props {
            if category == "character" || !f.is_empty() && !id.contains(&f) && !name.to_lowercase().contains(&f) && !category.contains(&f) {
                continue;
            }
            if category != last {
                ui.add_space(3.0);
                ui.label(RichText::new(category.replace('_', " ")).size(10.5).color(Color32::from_gray(130)));
                last = category.clone();
            }
            if ui.selectable_label(self.prop.as_deref() == Some(id.as_str()), format!("{name}  {}×{}", fp[0], fp[1])).clicked() {
                self.prop = Some(id);
            }
        }
        ui.add_space(4.0);
        hint(ui, &format!("Placed on level {}. Q/E turn it (Shift: by 90°); Shift places it off the grid. Every prop, by folder: Assets.", self.level));
    }

    fn trees_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Tree areas");
        hint(ui, "Cells the player can't cross, filled with trees.");
        for i in 0..self.map.tree_areas.len() {
            let a = &self.map.tree_areas[i];
            let cells = self.map.area.iter().filter(|&&c| c as usize == i + 1).count();
            if ui.selectable_label(self.area == Some(i), format!("{} ({}) · {cells} cells", a.name, a.mark)).clicked() {
                self.area = Some(i);
            }
        }
        if ui.small_button("New area").clicked() {
            self.checkpoint();
            self.new_area();
        }
        section(ui, "Paint");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.remove_cells, false, "Add cells");
            ui.radio_value(&mut self.remove_cells, true, "Remove");
        });
        ui.add(egui::Slider::new(&mut self.brush, 0..=4).custom_formatter(|v, _| format!("{0}×{0}", v as i32 * 2 + 1)));
        hint(ui, "Alt: the other way. Painting with no area picked makes one.");
        let Some(i) = self.area.filter(|&i| i < self.map.tree_areas.len()) else { return };
        section(ui, "Area");
        let before = self.map.tree_areas[i].clone();
        let tree_props: Vec<(String, String)> = self.content.props.iter().filter(|p| matches!(p.category.as_str(), "tree" | "flora")).map(|p| (p.id.clone(), p.name.clone())).collect();
        let mut remove = false;
        {
            let a = &mut self.map.tree_areas[i];
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(f32::INFINITY));
            });
            ui.label(RichText::new("Trees").size(11.5));
            for (id, name) in &tree_props {
                let mut on = a.trees.contains(id);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        a.trees.push(id.clone());
                    } else {
                        a.trees.retain(|t| t != id);
                    }
                }
            }
            ui.add(egui::Slider::new(&mut a.spacing, 0.6..=3.0).text("spacing").suffix(" cells"));
            let (mut lo, mut hi) = (a.height[0], a.height[1]);
            ui.add(egui::Slider::new(&mut lo, 0.5..=1.6).text("smallest"));
            ui.add(egui::Slider::new(&mut hi, 0.5..=1.6).text("tallest"));
            a.height = [lo.min(hi), hi.max(lo)];
            ui.add(egui::Slider::new(&mut a.density, 0.0..=1.0).text("density"));
            ui.checkbox(&mut a.solid_edge, "A tree in every edge cell");
            ui.horizontal(|ui| {
                ui.label("Seed");
                ui.add(egui::DragValue::new(&mut a.seed));
                if ui.small_button("Reroll").clicked() {
                    a.seed = (crate::platform::now_ms() as u32).wrapping_mul(2654435761) % 100_000;
                }
            });
            if ui.small_button("Remove this area").clicked() {
                remove = true;
            }
        }
        if self.map.tree_areas[i] != before {
            // one undo step per edit of the settings
            let after = self.map.tree_areas[i].clone();
            self.map.tree_areas[i] = before;
            if self.undo.last().map(|m| m.tree_areas.get(i) != Some(&self.map.tree_areas[i])).unwrap_or(true) {
                self.checkpoint();
            }
            self.map.tree_areas[i] = after;
            self.dirty = true;
            self.need_trees = true;
        }
        if remove {
            self.checkpoint();
            self.map.remove_area(i);
            self.area = None;
            self.need_trees = true;
        }
    }

    /// The right panel: the selected building or prop, else the map itself.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        if self.tool == Tool::Events && self.event_inspector(ui) {
            return;
        }
        if let Some(i) = self.selected_building.filter(|&i| i < self.map.buildings.len()) {
            self.building_inspector(ui, i);
            return;
        }
        let Some(i) = self.selected.filter(|&i| i < self.map.props.len()) else {
            self.map_inspector(ui);
            return;
        };
        let prop = self.content.props.get(&self.map.props[i].prop).cloned();
        let name = prop.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| self.map.props[i].prop.clone());
        ui.label(RichText::new(name).size(16.0).strong());
        if let Some(p) = &prop {
            hint(ui, &format!("{} · {}×{} cells · {:.1} high", p.category.replace('_', " "), p.footprint[0], p.footprint[1], p.height));
        }
        let before = self.map.props[i].clone();
        let mut delete = false;
        let mut template = false;
        let room = self.prop_room(i);
        {
            let p = &mut self.map.props[i];
            section(ui, "Placed");
            let (w, d) = prop.as_ref().map(|q| p.turned(q.footprint)).unwrap_or((1, 1));
            let (x0, z0) = ((p.at[0] - w as f64 / 2.0).round() as i32, (p.at[1] - d as f64 / 2.0).round() as i32);
            hint(ui, &format!("On {} cell{} from {x0}, {z0}, level {}: it stays on them.", w as u16 * d as u16, if w as u16 * d as u16 == 1 { "" } else { "s" }, p.level));
            ui.add(egui::Slider::new(&mut p.turn, 0.0..=360.0).step_by(15.0).text("turn").suffix("°"));
            let scalable = prop.as_ref().map(|q| q.scalable()).unwrap_or(false);
            ui.add_enabled(scalable, egui::Slider::new(&mut p.scale, 0.25..=1.0).text("scale")).on_disabled_hover_text("Trees and flora keep their size");
            match room {
                Some((lo, hi)) => {
                    for (k, label) in [(0, "x"), (1, "z")] {
                        if hi[k] - lo[k] > 0.02 {
                            let r = ui.add(egui::Slider::new(&mut p.offset[k], lo[k]..=hi[k]).text(label).fixed_decimals(2));
                            mark(&format!("uiOffset{}", label.to_uppercase()), &r);
                        }
                    }
                    hint(ui, "Where it stands within its cells (or drag it on the map).");
                }
                None => hint(ui, "It fills its cells: it can't move within them."),
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").on_hover_text("Delete").clicked() {
                    delete = true;
                }
                let t = ui.button("Make a template").on_hover_text("This prop as it is, to put down again anywhere (Select's Templates)");
                mark("uiTemplateProp", &t);
                if t.clicked() {
                    template = true;
                }
            });
        }
        if self.map.props[i] != before {
            let after = self.map.props[i].clone();
            self.map.props[i] = before.clone();
            if self.undo.last().map(|m| m.props.get(i) != Some(&self.map.props[i])).unwrap_or(true) {
                self.checkpoint();
            }
            self.map.props[i] = after;
            if self.map.props[i].turn != before.turn || self.map.props[i].scale != before.scale {
                self.reseat(i);
            }
            self.dirty = true;
            self.need_props = true;
        }
        if template {
            self.template_of_prop(i);
        }
        if delete {
            self.delete_selected();
            return;
        }
        // the prop's record itself: what it is and does, its light, its sound, Blender
        let id = self.map.props[i].prop.clone();
        self.prop_settings(ui, &id);
    }

    /// The map's own settings and what's on it, when nothing is selected.
    fn map_inspector(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(self.map.name.clone()).size(16.0).strong());
        ui.horizontal(|ui| {
            badge(ui, self.space);
            hint(ui, &format!("{} · {}×{} cells", self.map.id, self.map.width, self.map.height));
        });
        section(ui, "Map");
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.add(egui::TextEdit::singleline(&mut self.map.name).desired_width(f32::INFINITY)).changed() {
                self.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Size");
            ui.add(egui::DragValue::new(&mut self.width_edit).range(4..=256));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut self.height_edit).range(4..=256));
            let same = self.width_edit as usize == self.map.width && self.height_edit as usize == self.map.height;
            if ui.add_enabled(!same, egui::Button::new("Apply").small()).on_hover_text("New cells get the terrain picked in Ground").clicked() {
                self.checkpoint();
                let fill = self.terrain.clone();
                self.map.resize(self.width_edit as usize, self.height_edit as usize, &fill);
                self.everything_changed();
            }
        });
        section(ui, "Storage");
        let mut chunked = self.map.chunk > 0;
        if ui.checkbox(&mut chunked, format!("Chunk files ({CHUNK} × {CHUNK})")).on_hover_text("The cells in data/maps/<id>/<x>_<z>.json squares, with numbers for levels and no limit on terrains: an edit touches few files. Off: rows of characters in the record.").changed() {
            self.map.chunk = if chunked { CHUNK } else { 0 };
            self.dirty = true;
        }
        if chunked {
            let n = CHUNK as i32;
            hint(ui, &format!("{} × {} squares", (self.map.width as i32 + n - 1) / n, (self.map.height as i32 + n - 1) / n));
        }
        section(ui, "On it");
        let m = &self.map;
        let rows = [
            ("props", m.props.len()),
            ("buildings", m.buildings.len()),
            ("tree areas", m.tree_areas.len()),
            ("zones", m.zones.len()),
            ("links", m.links.len()),
        ];
        for (what, n) in rows {
            ui.label(RichText::new(format!("{n} {what}")).size(12.5));
        }
        ui.add_space(6.0);
        hint(ui, "Select a prop or a building to see it here.");
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (mods, keys) = ctx.input(|i| (i.modifiers, i.keys_down.clone()));
        let pressed = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
        if mods.command && pressed(egui::Key::Z) {
            if mods.shift { self.redo() } else { self.undo() }
            return;
        }
        if mods.command && pressed(egui::Key::Y) {
            self.redo();
            return;
        }
        if mods.command && pressed(egui::Key::S) {
            self.save();
            return;
        }
        for (k, t) in [(egui::Key::Num1, Tool::Select), (egui::Key::Num2, Tool::Ground), (egui::Key::Num3, Tool::Place), (egui::Key::Num4, Tool::Trees), (egui::Key::Num5, Tool::Build), (egui::Key::Num6, Tool::Zones), (egui::Key::Num7, Tool::Events)] {
            if pressed(k) {
                self.tool = t;
            }
        }
        if pressed(egui::Key::PageUp) || pressed(egui::Key::Plus) || pressed(egui::Key::Equals) {
            self.level = (self.level + 1).min(self.max_level);
        }
        if pressed(egui::Key::PageDown) || pressed(egui::Key::Minus) {
            self.level = self.level.saturating_sub(1);
        }
        if pressed(egui::Key::G) {
            self.grid = !self.grid;
        }
        if pressed(egui::Key::Home) {
            self.recentre();
        }
        if pressed(egui::Key::Escape) {
            if self.tpl.capturing {
                self.tpl.capturing = false;
                self.tpl.drag = None;
            } else if self.evt.placing.is_some() {
                self.evt.placing = None;
                self.evt.drag = None;
            } else if self.walker.is_some() {
                self.stop_walk();
            } else if let Some(d) = self.draft.as_mut().filter(|d| d.sel.is_some()) {
                d.sel = None;
            } else {
                self.selected = None;
            }
        }
        if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
            if self.tool == Tool::Build && self.draft.is_some() {
                self.build_keys(0.0, true);
            } else if let Some(i) = self.selected_building.take() {
                self.checkpoint();
                self.map.buildings.remove(i);
                self.need_buildings = true;
            } else {
                self.delete_selected();
            }
        }
        let step = if mods.shift { 90.0 } else { 15.0 };
        for (k, dir) in [(egui::Key::Q, -1.0), (egui::Key::E, 1.0)] {
            if pressed(k) {
                if self.tool == Tool::Build && self.draft.is_some() {
                    self.build_keys(dir * step, false);
                } else if let Some(i) = self.selected_building {
                    if dir > 0.0 {
                        self.turn_building(i);
                    } else {
                        // a quarter back is three quarters forward
                        for _ in 0..3 {
                            self.turn_building(i);
                        }
                    }
                } else if self.tool == Tool::Place {
                    self.turn = (self.turn + dir * step).rem_euclid(360.0);
                } else {
                    self.turn_selected(dir * step);
                }
            }
        }
        // arrows pan (they walk, while walking)
        if self.walker.is_some() {
            return;
        }
        let (right, forward) = self.cam.ground_axes();
        let speed = self.cam.distance * 0.02;
        for (k, d) in [(egui::Key::ArrowLeft, -right), (egui::Key::ArrowRight, right), (egui::Key::ArrowUp, forward), (egui::Key::ArrowDown, -forward)] {
            if keys.contains(&k) {
                self.cam.target += d * speed;
            }
        }
    }

    fn view_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let (pointer, scroll, mods, delta, primary_pressed, primary_down, primary_released, secondary_clicked) = ui.input(|i| {
            (
                i.pointer.hover_pos(),
                i.smooth_scroll_delta.y,
                i.modifiers,
                i.pointer.delta(),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.primary_down(),
                i.pointer.button_released(PointerButton::Primary),
                i.pointer.button_clicked(PointerButton::Secondary),
            )
        });
        // a template (or a prop) dragged in from a list
        let carried = egui::DragAndDrop::payload::<String>(ui.ctx());
        let dropping = carried.is_some() && response.contains_pointer();
        let hovered = response.hovered() || response.dragged() || dropping;
        // the view
        if response.hovered() && scroll != 0.0 {
            self.cam.distance = (self.cam.distance * (-scroll * 0.0015).exp()).clamp(5.0, 220.0);
        }
        if response.dragged_by(PointerButton::Secondary) {
            let (right, forward) = self.cam.ground_axes();
            let k = self.cam.distance * 0.0016;
            self.cam.target += (-right * delta.x + forward * delta.y) * k;
        }
        if response.dragged_by(PointerButton::Middle) {
            self.cam.yaw = (self.cam.yaw - delta.x * 0.3).rem_euclid(360.0);
            self.cam.pitch = (self.cam.pitch + delta.y * 0.2).clamp(15.0, 89.0);
        }
        // what's under the pointer
        self.plane_hit = None;
        self.ground_hit = None;
        let ray = if hovered { pointer.and_then(|p| self.ray(p)) } else { None };
        if let Some((o, d)) = ray {
            self.plane_hit = Self::plane(o, d, self.level as f32 * self.level_height);
            self.ground_hit = self.ground(o, d);
        }
        self.last_ray = ray;
        if let Some(released) = response.dnd_release_payload::<String>() {
            self.drop_on_map(&released);
        } else if let (true, Some(p)) = (dropping, carried.as_ref()) {
            self.drag_over(p);
        } else {
            self.tpl.ghost = None;
            if let Some((origin, dir)) = ray {
                self.tool_input(origin, dir, response, mods, primary_pressed, primary_down, primary_released, secondary_clicked);
            }
        }
        if primary_released {
            if self.stroke && self.stroke_ground {
                self.need_ground = true;
            }
            self.stroke = false;
            self.stroke_ground = false;
            self.last_cell = None;
            self.grab = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tool_input(&mut self, origin: Vec3, dir: Vec3, response: &egui::Response, mods: egui::Modifiers, primary_pressed: bool, primary_down: bool, primary_released: bool, secondary_clicked: bool) {
        let plane_cell = self.plane_hit.map(|p| (p.x.floor() as i32, p.z.floor() as i32));
        match self.tool {
            Tool::Ground => {
                if secondary_clicked {
                    if let Some((x, z)) = self.ground_hit {
                        self.terrain = self.map.terrain_at(x, z).unwrap_or("grass").to_string();
                        self.level = self.map.level_at(x, z).unwrap_or(0);
                    }
                }
                if primary_pressed && response.hovered() {
                    self.checkpoint();
                    self.stroke = true;
                }
                if self.stroke && primary_down {
                    let cell = if self.set_level { plane_cell } else { self.ground_hit };
                    if let Some(c) = cell {
                        for c in self.line_to(c) {
                            if self.map.inside(c.0, c.1) {
                                self.paint_ground(c);
                            }
                        }
                    }
                }
            }
            Tool::Trees => {
                if primary_pressed && response.hovered() {
                    self.checkpoint();
                    self.stroke = true;
                }
                if self.stroke && primary_down {
                    if let Some(c) = self.ground_hit {
                        for c in self.line_to(c) {
                            self.paint_area(c, self.remove_cells != mods.alt);
                        }
                    }
                }
            }
            Tool::Place => {
                if primary_pressed && response.hovered() {
                    if let (Some(prop), Some(p)) = (self.prop.clone(), self.plane_hit) {
                        let at = self.snap(&prop, self.turn, p);
                        // Shift: where the pointer is within its cells, as far as it leaves room
                        let offset = match (mods.shift, self.content.props.get(&prop)) {
                            (true, Some(q)) => {
                                let (lo, hi) = world::mesh::offset_room(q, &self.models, self.turn as f64, 1.0);
                                [((p.x as f64 - at[0]).clamp(lo[0], hi[0]) * 100.0).round() / 100.0, ((p.z as f64 - at[1]).clamp(lo[1], hi[1]) * 100.0).round() / 100.0]
                            }
                            _ => [0.0, 0.0],
                        };
                        let placed = Placed { prop: prop.clone(), at, level: self.level, turn: self.turn as f64, scale: 1.0, offset };
                        let cells = rooms::cells_of(self, &placed);
                        let clash = rooms::clash(self, &prop, &cells, &rooms::ground_items(self, self.level, None));
                        if let Some(other) = clash {
                            self.status = format!("No room: {other} is there, and one of them doesn't stack");
                        } else if at[0] >= 0.0 && at[1] >= 0.0 && at[0] <= self.map.width as f64 && at[1] <= self.map.height as f64 {
                            self.checkpoint();
                            self.map.props.push(placed);
                            self.selected = Some(self.map.props.len() - 1);
                            self.need_props = true;
                        }
                    }
                }
            }
            Tool::Build => {
                self.build_view(origin, dir, mods.alt, primary_pressed && response.hovered(), primary_down, primary_released, response.hovered());
            }
            Tool::Zones => {
                self.zones_view(mods.alt, primary_pressed && response.hovered(), primary_down, primary_released, response.hovered());
            }
            Tool::Events => {
                self.events_view(primary_pressed && response.hovered(), primary_down, primary_released, response.hovered(), secondary_clicked);
            }
            Tool::Select if self.tpl.capturing => {
                self.capture_view(primary_pressed && response.hovered(), primary_down, primary_released);
            }
            Tool::Select => {
                if primary_pressed && response.hovered() {
                    let prop = self.pick_prop(origin, dir);
                    let house = self.pick_building(origin, dir);
                    let nearer = match (prop, house) {
                        (Some((tp, _)), Some((tb, _))) => tb < tp,
                        (None, Some(_)) => true,
                        _ => false,
                    };
                    self.selected = if nearer { None } else { prop.map(|(_, i)| i) };
                    self.selected_building = if nearer { house.map(|(_, i)| i) } else { None };
                    if self.selected_building.is_some() {
                        self.status = "A building fills its cells: it stays where it was built (Edit changes it)".into();
                    }
                    if let Some(i) = self.selected {
                        let p = &self.map.props[i];
                        let pos = p.pos();
                        let y = p.level as f32 * self.level_height;
                        if let Some(hit) = Self::plane(origin, dir, y) {
                            self.grab = Some(Vec2::new(hit.x - pos[0] as f32, hit.z - pos[1] as f32));
                        }
                        if self.prop_room(i).is_none() {
                            self.status = "It fills its cells: it stays where it was put".into();
                        }
                    }
                }
                // a prop moves within its cells, as far as it leaves room there
                if let (Some(i), Some(grab), true) = (self.selected, self.grab, primary_down && response.dragged()) {
                    let level = self.map.props[i].level;
                    if let (Some(hit), Some((lo, hi))) = (Self::plane(origin, dir, level as f32 * self.level_height), self.prop_room(i)) {
                        let p = &self.map.props[i];
                        let want = [(hit.x - grab.x) as f64 - p.at[0], (hit.z - grab.y) as f64 - p.at[1]];
                        let offset = [(want[0].clamp(lo[0], hi[0]) * 100.0).round() / 100.0, (want[1].clamp(lo[1], hi[1]) * 100.0).round() / 100.0];
                        if offset != p.offset {
                            if !self.stroke {
                                self.checkpoint();
                                self.stroke = true;
                            }
                            self.map.props[i].offset = offset;
                            self.need_props = true;
                        }
                    }
                }
            }
        }
    }

    fn frame(&mut self, now: f64, rect: egui::Rect) -> Frame {
        let aspect = rect.width() / rect.height().max(1.0);
        self.view_proj = self.cam.view_proj(aspect);
        let radius = (self.cam.distance * 0.9).max(16.0);
        let (mut light, tint) = period_lighting(self.period, None, self.cam.target, radius);
        let mut billboards = vec![];
        let mut shapes = vec![];
        let mut overlays = vec![];
        let lh = self.level_height;
        // the lamps: lit by night (or always, with Lamps on), the nearest to the camera
        // first; each glows where it sits
        let on = if self.lamps { 1.0 } else { crate::stage::lamps_on(self.period) };
        let mut glows = vec![];
        let mut all = world::lights::lights(&self.map, &self.content, lh);
        if self.creatures_on {
            all.extend(self.creature_lights());
        }
        let mut lamps: Vec<(f32, world::lights::PlacedLight)> = all
            .into_iter()
            .filter(|l| !l.light.night || on > 0.0)
            .map(|l| (l.pos.distance(self.cam.target), l))
            .collect();
        lamps.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, l) in lamps.into_iter().take(MAX_LIGHTS) {
            let k = l.light.intensity as f32 * if l.light.night { on } else { 1.0 };
            let c = world::color(&l.light.color);
            light.lights.push(crate::stage::spot_light(&l.light, l.pos, l.dir, k));
            glows.push(Glow { pos: l.pos, size: 0.3 + 0.15 * k, color: [c[0], c[1], c[2], (0.6 * k).min(1.0)] });
        }
        crate::platform::set_hook("lights", &light.lights.len().to_string());
        let rect_at = |x: f32, y: f32, z: f32, w: f32, d: f32, color: [f32; 4]| Shape { kind: ShapeKind::Rect { width: w, depth: d }, center: Vec3::new(x, y, z), color, additive: false };
        match self.tool {
            Tool::Ground | Tool::Trees => {
                let at_plane = self.tool == Tool::Ground && self.set_level;
                let cell = if at_plane { self.plane_hit.map(|p| (p.x.floor() as i32, p.z.floor() as i32)) } else { self.ground_hit };
                if let Some((x, z)) = cell {
                    let size = (self.brush * 2 + 1) as f32;
                    let y = if at_plane { self.level as f32 * lh } else { self.map.level_at(x, z).unwrap_or(0) as f32 * lh };
                    let color = if self.tool == Tool::Trees && (self.remove_cells) { [1.0, 0.35, 0.3, 0.4] } else { [1.0, 1.0, 1.0, 0.35] };
                    shapes.push(rect_at(x as f32 + 0.5, y + 0.04, z as f32 + 0.5, size, size, color));
                    // where it falls on the ground below (or above)
                    if let Some(l) = self.map.level_at(x, z) {
                        if at_plane && l != self.level {
                            shapes.push(rect_at(x as f32 + 0.5, l as f32 * lh + 0.04, z as f32 + 0.5, size, size, [0.0, 0.0, 0.0, 0.3]));
                        }
                    }
                }
            }
            Tool::Place => {
                if let (Some(prop), Some(p)) = (&self.prop, self.plane_hit) {
                    let at = self.snap(prop, self.turn, p);
                    let fp = self.content.props.get(prop).map(|q| q.footprint).unwrap_or([1, 1]);
                    let quarter = ((self.turn / 90.0).round() as i64).rem_euclid(2) == 1;
                    let (w, d) = if quarter { (fp[1], fp[0]) } else { (fp[0], fp[1]) };
                    shapes.push(rect_at(at[0] as f32, self.level as f32 * lh + 0.04, at[1] as f32, w as f32, d as f32, [1.0, 1.0, 1.0, 0.4]));
                }
            }
            Tool::Build => self.build_shapes(&mut shapes, &mut overlays),
            Tool::Select | Tool::Zones | Tool::Events => {}
        }
        self.zones_shapes(&mut shapes);
        self.template_shapes(&mut shapes);
        self.creature_billboards([tint.x, tint.y, tint.z, 1.0], &mut billboards, &mut shapes);
        self.actor_billboards([tint.x, tint.y, tint.z, 1.0], now, &mut billboards, &mut shapes);
        self.events_shapes(&mut shapes, now);
        let animated = self.animated_draws(now);
        self.animated_n = animated.len();
        if let Some(b) = self.selected_building.and_then(|i| self.map.buildings.get(i)) {
            let y = b.level as f32 * lh + 0.05;
            for (x, z) in b.map_cells() {
                shapes.push(rect_at(x as f32 + 0.5, y, z as f32 + 0.5, 1.0, 1.0, [1.0, 0.85, 0.2, 0.3]));
            }
        }
        if self.tool == Tool::Trees {
            for z in 0..self.map.height as i32 {
                for x in 0..self.map.width as i32 {
                    let i = self.map.index(x, z).unwrap();
                    let a = self.map.area[i];
                    if a > 0 {
                        let mut c = AREA_COLORS[(a as usize - 1) % AREA_COLORS.len()];
                        if Some(a as usize - 1) == self.area {
                            c[3] = 0.5;
                        }
                        shapes.push(rect_at(x as f32 + 0.5, self.map.level[i] as f32 * lh + 0.05, z as f32 + 0.5, 1.0, 1.0, c));
                    }
                }
            }
        }
        if let Some(i) = self.selected {
            if let Some((lo, hi)) = self.map.props.get(i).and_then(|p| self.prop_box(p)) {
                let c = (lo + hi) * 0.5;
                shapes.push(rect_at(c.x, lo.y + 0.05, c.z, hi.x - lo.x + 0.1, hi.z - lo.z + 0.1, [1.0, 0.85, 0.2, 0.45]));
            }
        }
        if let Some(p) = self.probe {
            let c = self.view_proj * p.extend(1.0);
            let (x, y) = (c.x / c.w, c.y / c.w);
            let sx = rect.left() + (x + 1.0) / 2.0 * rect.width();
            let sy = rect.top() + (1.0 - y) / 2.0 * rect.height();
            crate::platform::set_hook("point", &format!("{sx:.0},{sy:.0}"));
        }
        Frame {
            view_proj: self.view_proj,
            camera_pos: self.cam.eye(),
            light,
            shadows: true,
            billboards,
            shapes,
            glows,
            animated,
            streaks: vec![],
            time: (now / 1000.0) as f32,
            grid: self.grid,
            focus: self.focus_box(),
            dim: self.focus_dim(now),
            overlays,
        }
    }
}
