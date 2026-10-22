//! The game's shell: the encounter menu, the content browser and the map editor, loading
//! what a battle needs, the battle itself (3D scene + HUD), the results; frame-time
//! statistics and the test hooks. Everything it shows comes from the content (both spaces).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use content::{Content, Space};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Vec2};
use render::map::{find_spot, MapScene};
use render::scene::{DrawStats, Frame, Handheld, Quality, Retro, SceneRenderer};
use rules::battle::{Battle, BattleKind, BattleSetup, Outcome, Side};
use rules::config::{Config, Period};
use rules::dex::{Dex, Rng};
use rules::mon::{create_mon, Mon, MonSpec};

use crate::director::Director;
use crate::editor::Browser;
use crate::hud::{Hud, ACCENT, MUTED, TEXT};
use crate::input::{Button, Input};
use crate::mapedit::{Leave, MapEditor, SaveGame, SAVE_NAME, WELCOME_NAME};
use crate::platform::{self, Files, Load};
use crate::presets::{kind_label, kind_of, pool, spec, DEFAULT_BAG};
use crate::stage::Stage;
use crate::title::Choice;

pub struct Options {
    /// the time of day, in hours, when a test fixes it (`time:` or `--time`); else the real one
    pub time: Option<f32>,
    /// 0 easy (wide windows), 1 normal, 2 hard
    pub difficulty: usize,
    pub assist: bool,
    /// 0 fast, 1 normal, 2 slow
    pub text: usize,
    /// the editor's Test battles: the party at each encounter's level, the AI on your side
    pub match_level: bool,
    pub auto: bool,
    pub graphics: Graphics,
    /// the sound's volume (all of it), the one-shots' and the loops', percent
    pub volume: u8,
    pub effects: u8,
    pub ambience: u8,
    /// the keys and the controller's buttons each button answers to
    pub controls: crate::input::Bindings,
}

/// The options kept between runs (the Options window's; `platform::store_text`, "options"),
/// as words a person can read in the file.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct KeptOptions {
    /// fast, normal or slow
    pub text: String,
    /// easy, normal or hard
    pub timing: String,
    pub rings: bool,
    /// percent of the screen's resolution
    pub resolution: u8,
    /// off, low, medium or high
    pub shadows: String,
    /// the pixelation, and how dense its pixels are (0 the fewest, 1 the screen's own)
    pub pixelation: bool,
    pub pixel_density: f32,
    /// an old handheld's LCD colours: off, gba or gbc
    pub lcd: String,
    pub volume: u8,
    pub effects: u8,
    pub ambience: u8,
    /// each button's keys, by name ("A": ["Z", "Space", "Enter"])
    pub controls: BTreeMap<String, Vec<String>>,
    /// each button's controller buttons, by name ("A": ["South"])
    pub pad: BTreeMap<String, Vec<String>>,
}

impl Default for KeptOptions {
    fn default() -> Self {
        Options::default().saved()
    }
}

/// The name the options are kept under.
pub const OPTIONS_NAME: &str = "options";
const SPEEDS: [&str; 3] = ["fast", "normal", "slow"];
const TIMINGS: [&str; 3] = ["easy", "normal", "hard"];

impl Default for Options {
    fn default() -> Self {
        Options {
            time: None,
            difficulty: 1,
            assist: true,
            text: 1,
            match_level: true,
            auto: false,
            graphics: Graphics::default(),
            volume: 100,
            effects: 100,
            ambience: 100,
            controls: crate::input::Bindings::default(),
        }
    }
}

impl Options {
    /// What's kept of them.
    pub fn saved(&self) -> KeptOptions {
        KeptOptions {
            text: SPEEDS[self.text.min(2)].into(),
            timing: TIMINGS[self.difficulty.min(2)].into(),
            rings: self.assist,
            resolution: self.graphics.scale,
            shadows: Graphics::SHADOWS[self.graphics.shadows.min(3)].0.to_lowercase(),
            pixelation: self.graphics.pixelation,
            pixel_density: (self.graphics.density * 100.0).round() / 100.0,
            lcd: Graphics::HANDHELDS[self.graphics.lcd.min(2)].to_lowercase(),
            volume: self.volume,
            effects: self.effects,
            ambience: self.ambience,
            controls: self.controls.names(),
            pad: self.controls.pad_names(),
        }
    }

    /// Kept options put back (what can't be read keeps its value).
    pub fn apply(&mut self, k: &KeptOptions) {
        self.text = SPEEDS.iter().position(|s| *s == k.text).unwrap_or(self.text);
        self.difficulty = TIMINGS.iter().position(|s| *s == k.timing).unwrap_or(self.difficulty);
        self.assist = k.rings;
        if [100, 75, 50].contains(&k.resolution) {
            self.graphics.scale = k.resolution;
        }
        self.graphics.shadows = Graphics::SHADOWS.iter().position(|(n, _)| n.eq_ignore_ascii_case(&k.shadows)).unwrap_or(self.graphics.shadows);
        self.graphics.pixelation = k.pixelation;
        if k.pixel_density.is_finite() {
            self.graphics.density = k.pixel_density.clamp(0.0, 1.0);
        }
        self.graphics.lcd = Graphics::handheld(&k.lcd).unwrap_or(self.graphics.lcd);
        self.volume = k.volume.min(100);
        self.effects = k.effects.min(100);
        self.ambience = k.ambience.min(100);
        self.controls = crate::input::Bindings::from_names(&k.controls, &k.pad);
    }

    /// One of the Options' tabs as it came: 0 Game, 1 Graphics, 2 Controls, 3 Audio.
    pub fn reset_part(&mut self, tab: usize) {
        let d = Options::default();
        match tab {
            0 => (self.text, self.difficulty, self.assist) = (d.text, d.difficulty, d.assist),
            1 => self.graphics = d.graphics,
            2 => self.controls = d.controls,
            _ => (self.volume, self.effects, self.ambience) = (d.volume, d.effects, d.ambience),
        }
    }

    /// The sound's levels and the keys, where they're used.
    pub fn put_to_use(&self, audio: &mut crate::audio::Audio) {
        audio.master = self.volume as f32 / 100.0;
        audio.effects = self.effects as f32 / 100.0;
        audio.ambience = self.ambience as f32 / 100.0;
        crate::input::set_bindings(self.controls.clone());
    }
}

/// How much the GPU draws (for machines that can't keep up): the 3D scene's resolution
/// against the screen's, and the shadows' sharpness; and the post-processing: the pixelation
/// and an old handheld's LCD.
#[derive(Clone, Copy, PartialEq)]
pub struct Graphics {
    /// percent of the screen's resolution
    pub scale: u8,
    /// 0 none, 1 low, 2 medium, 3 high
    pub shadows: usize,
    /// the world in big square pixels (in 15-bit colours), and how dense they are: 0 the
    /// fewest, 1 the screen's own (none); they grow and shrink with the camera's zoom
    pub pixelation: bool,
    pub density: f32,
    /// an old handheld's LCD colours: 0 none, 1 the GBA's, 2 the GBC's (`HANDHELDS`)
    pub lcd: usize,
    /// the indirect light of what glows (the radiance cascades each frame; WebGPU only)
    pub indirect: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Graphics { scale: 100, shadows: 2, pixelation: false, density: 0.35, lcd: 0, indirect: true }
    }
}

impl Graphics {
    pub const SHADOWS: [(&'static str, u32); 4] = [("Off", 0), ("Low", 1024), ("Medium", 2048), ("High", 4096)];
    pub const HANDHELDS: [&'static str; 3] = ["Off", "GBA", "GBC"];

    /// A handheld by its name ("gba"), as `HANDHELDS` numbers them.
    pub fn handheld(name: &str) -> Option<usize> {
        Self::HANDHELDS.iter().position(|h| h.eq_ignore_ascii_case(name))
    }

    /// The post-processing, for the game's screens (the camera's zoom is theirs to give).
    pub fn retro(self) -> Retro {
        let lcd = match self.lcd {
            1 => Some(Handheld::Gba),
            2 => Some(Handheld::Gbc),
            _ => None,
        };
        Retro { pixelation: self.pixelation.then_some(self.density.clamp(0.0, 1.0)), lcd, ..Retro::default() }
    }

    fn quality(self) -> Quality {
        Quality { scale: self.scale as f32 / 100.0, shadow_size: Self::SHADOWS[self.shadows.min(3)].1, indirect: self.indirect }
    }
}

/// Command-line options of the desktop build (the browser takes them as `<body data-cmd>`).
#[derive(Default, Clone)]
pub struct Cli {
    pub preset: Option<String>,
    pub auto: bool,
    pub time: Option<String>,
    pub exit_after: Option<f64>,
    pub stats: bool,
    pub levels: Option<u8>,
    /// open the map editor (on that map, if not empty)
    pub maps: Option<String>,
    /// walk a map in the game (the saved one, or the first, if empty)
    pub explore: Option<String>,
    /// draw as fast as possible, not at the screen's rate (to measure; the desktop only)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub no_vsync: bool,
    /// the map editor with its Requests window open
    pub requests: bool,
    /// no sound at all (checks, screenshots)
    pub mute: bool,
}

struct Loading {
    encounter: usize,
    period: Period,
    /// the time of day it starts at, for its light
    hours: f32,
    /// the map's folder from the root (e.g. temporary/assets/maps/Route101)
    map_dir: String,
    scene: Option<MapScene>,
    /// (file, texture key)
    textures: Vec<(String, String)>,
    player_key: Option<String>,
    foe_key: Option<String>,
}

struct Results {
    encounter: usize,
    outcome: Option<Outcome>,
    rounds: u32,
    level_ups: Vec<String>,
    caught: Vec<String>,
    since: f64,
}

enum Screen {
    Boot,
    /// the main menu
    Menu,
    /// the editor's Test battles: any encounter, to try it
    Battles,
    /// the editor's content browser
    Editor(Box<Browser>),
    Maps(Box<MapEditor>),
    /// the overworld: a map walked in the game
    Overworld(Box<MapEditor>),
    Loading(Loading),
    Battle(Box<Director>, usize),
    Results(Box<Director>, Results),
}

/// Frame times: frames per second, the worst gap between two frames, and the processor's
/// time per frame (the game's and egui's work, without waiting for the screen), per second.
#[derive(Default)]
struct Stats {
    last: f64,
    window_start: f64,
    frames: u32,
    worst: f64,
    cpu_sum: f64,
    cpu_worst: f64,
    pub fps: u32,
    pub worst_gap: f64,
    /// milliseconds: average and worst
    pub cpu: f64,
    pub cpu_peak: f64,
}

pub struct GameApp {
    files: Files,
    content: Option<Arc<Content>>,
    dex: Option<Arc<Dex>>,
    party: Vec<MonSpec>,
    /// the party's places in the battle's order (the story's main first, sent out first)
    fielded: Vec<usize>,
    /// the player's trainer picture, from the root
    player_sprite: Option<String>,
    options: Options,
    screen: Screen,
    input: Input,
    stats: Stats,
    icons: HashMap<String, egui::TextureHandle>,
    /// the team's front pictures, for their cards (View all): the texture, the part with the
    /// creature in it (in the texture's 0 to 1), and that part's size in pixels
    pictures: HashMap<String, (egui::TextureHandle, egui::Rect, egui::Vec2)>,
    /// FX flipbooks that couldn't be read (not asked for again)
    fx_failed: std::collections::HashSet<String>,
    /// FX meshes were sent to the scene last frame (cleared when they're gone)
    fx_meshes_shown: bool,
    error: Option<String>,
    cli: Cli,
    started_at: f64,
    menu_sel: usize,
    /// frames over 25 ms and what was going on (for the checks)
    hitches: Vec<String>,
    /// what the renderer put on screen last frame
    draws: DrawStats,
    /// the frame-time details (F3)
    perf_panel: bool,
    audio: crate::audio::Audio,
    /// the map editor (or the overworld), kept while a battle it asked for plays
    parked: Option<Box<MapEditor>>,
    pending: Option<crate::mapedit::Request>,
    /// the overworld asks to leave, and what it last saved (and when)
    leaving: bool,
    last_save: Option<SaveGame>,
    last_save_at: f64,
    /// the main menu
    title: crate::title::Title,
    /// the controllers, and who's listened to
    pads: crate::pads::Pads,
    /// the Test battles' own Options window, and its tab
    battles_options: bool,
    battles_options_ui: crate::title::OptionsUi,
    /// the world behind the main menu
    menu_world: Option<Box<MapEditor>>,
    /// a battle from the Test battles goes back to them
    battles_back: bool,
    /// the map the editor had open (it opens on it again)
    editor_map: Option<String>,
    /// the editor's welcome shown this run (and the player's word that it needn't be again)
    welcomed: bool,
    /// why there are no battles (no creatures): the Test battles say so
    no_battles: Option<String>,
    /// when the main menu last looked for the saved story, and its place
    saved_at: (f64, Option<String>),
    /// the screen last frame (its `data-state`)
    last_state: &'static str,
    /// the HUD over the overworld (its buttons, the Team, Settings, Exit)
    hud: crate::play_hud::PlayHud,
    /// the renderer's frosted copy of the picture as egui's texture, and its generation
    frost: Option<(egui::TextureId, u64)>,
}

pub struct SceneCallback {
    pub frame: Frame,
}

impl eframe::egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &eframe::wgpu::Device,
        queue: &eframe::wgpu::Queue,
        screen: &eframe::egui_wgpu::ScreenDescriptor,
        encoder: &mut eframe::wgpu::CommandEncoder,
        resources: &mut eframe::egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        if let Some(r) = resources.get_mut::<SceneRenderer>() {
            r.prepare(device, queue, encoder, &self.frame, screen.size_in_pixels);
        }
        vec![]
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut eframe::wgpu::RenderPass<'static>, resources: &eframe::egui_wgpu::CallbackResources) {
        if let Some(r) = resources.get::<SceneRenderer>() {
            r.paint(pass);
        }
    }
}

fn decode(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}

impl GameApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli) -> GameApp {
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            let scene = SceneRenderer::new(&rs.device, &rs.queue, rs.target_format);
            rs.renderer.write().callback_resources.insert(scene);
            let info = rs.adapter.get_info();
            let backend = format!("{:?} on {}", info.backend, info.name);
            platform::set_hook("backend", &backend);
            log::info!("graphics: {backend}");
        }
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(14, 20, 32);
        cc.egui_ctx.set_global_style(style);
        // the project folder on the desktop (content/ and temporary/ in it), the site's root in the browser
        let root = if cfg!(target_arch = "wasm32") { ".".to_string() } else { std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()) };
        let files = Files::new(&root);
        if cfg!(target_arch = "wasm32") {
            for space in Space::ALL {
                files.request(&format!("{}/bundle.json", space.dir()));
            }
        }
        // what this browser kept of the editor's saves, read while the bundles come
        #[cfg(target_arch = "wasm32")]
        crate::keep::start(&cc.egui_ctx);
        // the options as the player left them, then what the command line says
        let mut options = Options::default();
        if let Some(kept) = platform::stored_text(OPTIONS_NAME).and_then(|t| serde_json::from_str::<KeptOptions>(&t).ok()) {
            options.apply(&kept);
        }
        options.time = cli.time.as_deref().and_then(platform::parse_clock);
        if cli.preset.is_some() {
            options.text = 0;
        }
        options.auto = cli.auto;
        let mut audio = crate::audio::Audio::new();
        audio.enabled = !cli.mute;
        options.put_to_use(&mut audio);
        crate::title::install_fonts(&cc.egui_ctx);
        // the icons are SVG
        egui_extras::install_image_loaders(&cc.egui_ctx);
        GameApp {
            files, content: None, dex: None, party: vec![], fielded: vec![], player_sprite: None, options, screen: Screen::Boot, input: Input::default(),
            stats: Stats::default(), icons: HashMap::new(), pictures: HashMap::new(), fx_failed: std::collections::HashSet::new(), fx_meshes_shown: false, error: None, cli, started_at: platform::now_ms(), menu_sel: 0, hitches: vec![],
            draws: DrawStats::default(), perf_panel: false, audio, parked: None, pending: None, leaving: false, last_save: None, last_save_at: 0.0,
            title: Default::default(), pads: crate::pads::Pads::new(), battles_options: false, battles_options_ui: Default::default(), menu_world: None, battles_back: false, editor_map: None, welcomed: false, no_battles: None, saved_at: (f64::NEG_INFINITY, None), last_state: "boot",
            hud: Default::default(), frost: None,
        }
    }

    /// Both spaces: read from disk on the desktop, from their bundles in the browser.
    fn load_content(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
            self.set_content(content::disk::load(&root));
        }
        #[cfg(target_arch = "wasm32")]
        {
            if crate::keep::keeping() == crate::keep::Keeping::Loading {
                return;
            }
            let mut texts = vec![];
            for space in Space::ALL {
                match self.files.get(&format!("{}/bundle.json", space.dir())) {
                    Load::Pending => return,
                    Load::Ready(b) => texts.push(String::from_utf8_lossy(&b).to_string()),
                    Load::Failed(_) => {} // a space the site doesn't have
                }
            }
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            let mut content = content::from_bundles(&refs);
            // the editor's saves this browser kept, over the site's (what the site has just
            // the same is forgotten: the game has it now)
            if crate::keep::keeping() == crate::keep::Keeping::Ready {
                for path in crate::keep::lay_over(&mut content, &self.files, &crate::keep::files()) {
                    crate::keep::forget(&path);
                }
            }
            self.set_content(content);
        }
    }

    fn set_content(&mut self, content: Content) {
        match Dex::from_content(&content) {
            Ok(dex) => {
                if let Some(e) = content.parties.entry("player") {
                    self.party = e.value.members.iter().map(|m| {
                        let mut s = spec(m, 1);
                        if let Some(lv) = self.cli.levels {
                            s.move_levels = vec![lv; 4];
                        } else if m.move_levels.is_empty() {
                            s.move_levels.clear();
                        }
                        s
                    }).collect();
                    self.player_sprite = e.value.trainer_sprite.as_ref().map(|p| e.space.asset(p));
                }
                self.dex = Some(Arc::new(dex));
                self.screen = Screen::Menu;
            }
            Err(e) => {
                self.no_battles = Some(format!("No battles yet: the content has no creatures ({e}). The editors work; add creatures to content/ (or local placeholders to temporary/)."));
                self.screen = Screen::Menu;
            }
        }
        self.content = Some(Arc::new(content));
    }

    /// The time of day, in hours: the real one (or a test's).
    fn hours(&self) -> f32 {
        self.options.time.unwrap_or_else(platform::local_hours)
    }

    fn period(&self) -> Period {
        Period::of_hour(self.hours() as u32)
    }

    fn config(&self) -> Config {
        let mut c = Config::default();
        let k = [1.6, 1.0, 0.65][self.options.difficulty];
        c.timing.tap = (c.timing.tap * k).round();
        c.timing.block = (c.timing.block * k).round();
        c.timing.cast = (c.timing.cast * k).round();
        c.timing.charge_window = (c.timing.charge_window * k).round();
        c.timing.assist = self.options.assist;
        c
    }

    /// The map editor or the overworld, whichever is up.
    fn editor_mut(&mut self) -> Option<&mut MapEditor> {
        match &mut self.screen {
            Screen::Maps(ed) | Screen::Overworld(ed) => Some(ed),
            _ => None,
        }
    }

    /// The screen a map editor goes back to: the overworld when it's playing.
    fn screen_of(ed: Box<MapEditor>) -> Screen {
        if ed.playing() { Screen::Overworld(ed) } else { Screen::Maps(ed) }
    }

    /// A battle a map's script asked for: the editor (or the overworld) waits, parked, and
    /// gets the player back after the results.
    fn battle_from_map(&mut self, id: &str) {
        let index = self.content.as_ref().and_then(|c| c.encounters.entries().iter().position(|e| e.value.id == id));
        let mut ed = match std::mem::replace(&mut self.screen, Screen::Menu) {
            Screen::Maps(ed) | Screen::Overworld(ed) => ed,
            other => {
                self.screen = other;
                return;
            }
        };
        match index {
            // none of the team can fight any more (all lost for good): no battle, the story
            // goes on
            Some(_) if ed.playing() && ed.team.battle_order(&crate::team::ids(&self.party)).is_empty() => {
                ed.resume_after_battle();
                self.screen = Self::screen_of(ed);
            }
            Some(i) if self.dex.is_some() => {
                if ed.playing() {
                    self.save_story(&ed.save_game(), true);
                }
                self.content = Some(ed.content());
                self.parked = Some(ed);
                self.audio.stop_all();
                self.start(i);
            }
            _ => {
                ed.resume_after_battle();
                self.screen = Self::screen_of(ed);
            }
        }
    }

    /// The overworld: the saved map (or `map`), walked from where the save left off, with
    /// the story's flags; with no story saved, where a new game starts.
    fn explore(&mut self, map: Option<&str>) {
        let Some(c) = self.content.clone() else { return };
        let save: Option<SaveGame> = platform::stored_text(SAVE_NAME).and_then(|t| serde_json::from_str(&t).ok());
        if save.is_none() && map.is_none() {
            self.new_game();
            return;
        }
        let map = map
            .map(String::from)
            .or_else(|| save.as_ref().map(|s| s.map.clone()).filter(|m| c.maps.contains(m)))
            .or_else(|| c.maps.iter().next().map(|m| m.id.clone()));
        let Some(map) = map.filter(|m| c.maps.contains(m)) else {
            self.error = Some("No map to explore: make one in the map editor first.".into());
            return;
        };
        self.last_save = save.clone();
        self.screen = Screen::Overworld(Box::new(MapEditor::new_play(c, &map, self.files.clone(), save.as_ref())));
    }

    /// A new story: the saved one given up, the player where the world says a game starts
    /// (the editor's "Start new games here"; else on the first map).
    fn new_game(&mut self) {
        let Some(c) = self.content.clone() else { return };
        platform::forget_text(SAVE_NAME);
        self.last_save = None;
        let start = c.world().start;
        let (map, arrival) = if c.maps.contains(&start.map) {
            let at = SaveGame { map: start.map.clone(), at: start.at, facing: start.facing, ..Default::default() };
            (Some(start.map.clone()), Some(at))
        } else {
            (c.maps.iter().next().map(|m| m.id.clone()), None)
        };
        let Some(map) = map else {
            self.error = Some("No map to start on: make one in the editor first.".into());
            return;
        };
        self.screen = Screen::Overworld(Box::new(MapEditor::new_play(c, &map, self.files.clone(), arrival.as_ref())));
    }

    /// The editor, on the map it had open; the welcome first when the player came in by
    /// the main menu's Editor (once a run, until they say not to).
    fn open_editor(&mut self, welcome: bool) {
        let Some(c) = self.content.clone() else { return };
        // the map it had open, else the one games start on
        let start = c.world().start.map;
        let open = self.editor_map.clone().or_else(|| (!start.is_empty()).then_some(start));
        let mut ed = MapEditor::new(c, open.as_deref(), self.files.clone());
        if welcome && !self.welcomed && platform::stored_text(WELCOME_NAME).is_none() {
            ed.welcome = true;
            self.welcomed = true;
        }
        self.screen = Screen::Maps(Box::new(ed));
    }

    /// Out of the map editor (its content kept, the map it had open remembered).
    fn leave_editor(&mut self, ed: &MapEditor) {
        self.content = Some(ed.content());
        self.editor_map = Some(ed.map_id().to_string());
        self.audio.stop_all();
    }

    /// Keeps the story between runs: when the map or the flags change, where the player
    /// stands every couple of seconds, and whenever `force` says so.
    fn save_story(&mut self, s: &SaveGame, force: bool) {
        let now = platform::now_ms();
        let changed = match &self.last_save {
            Some(last) => last.map != s.map || last.flags != s.flags || last.team != s.team || (last.at != s.at && now - self.last_save_at > 2000.0),
            None => true,
        };
        if changed || force {
            match serde_json::to_string(s) {
                Ok(text) => {
                    if let Err(e) = platform::store_text(SAVE_NAME, &text) {
                        log::warn!("the story wasn't saved: {e}");
                    }
                }
                Err(e) => log::warn!("the story wasn't saved: {e}"),
            }
            self.last_save = Some(s.clone());
            self.last_save_at = now;
        }
    }

    /// Out of the overworld, the story kept.
    fn leave_overworld(&mut self) {
        let Screen::Overworld(ed) = std::mem::replace(&mut self.screen, Screen::Menu) else { return };
        self.save_story(&ed.save_game(), true);
        self.content = Some(ed.content());
        self.audio.stop_all();
    }

    /// The encounters in the menu's order: by region, then by their own order.
    fn encounter_order(&self) -> Vec<usize> {
        let Some(c) = &self.content else { return vec![] };
        let entries = c.encounters.entries();
        c.regions.iter().flat_map(|r| (0..entries.len()).filter(move |&i| entries[i].value.region == r.id)).collect()
    }

    fn start(&mut self, encounter: usize) {
        let (Some(dex), Some(content)) = (self.dex.clone(), self.content.clone()) else { return };
        let entry = &content.encounters.entries()[encounter];
        let e = &entry.value;
        let Some(region) = content.regions.entry(&e.region) else {
            self.error = Some(format!("encounters/{}: no region {}", e.id, e.region));
            return;
        };
        let map_dir = region.space.asset(&format!("maps/{}", region.value.map));
        let (period, hours) = (self.period(), self.hours());
        let mut keys: Vec<String> = self.party.iter().map(|s| s.species.clone()).collect();
        keys.extend(e.party.iter().map(|m| m.creature.clone()));
        for pool in e.pools.values() {
            keys.extend(pool.iter().map(|(c, _)| c.clone()));
        }
        keys.sort();
        keys.dedup();
        let mut textures = vec![];
        for k in &keys {
            let Some(i) = dex.try_sp(k) else { continue };
            let sp = &dex.species[i];
            for (path, key) in [(&sp.front, format!("front/{k}")), (&sp.back, format!("back/{k}")), (&sp.icon, format!("icon:{k}"))] {
                if let Some(p) = path {
                    textures.push((p.clone(), key));
                }
            }
        }
        let player_key = self.player_sprite.clone();
        let foe_key = e.trainer_sprite.as_ref().map(|p| entry.space.asset(p));
        for p in player_key.iter().chain(foe_key.iter()) {
            textures.push((p.clone(), p.clone()));
        }
        for (path, _) in &textures {
            self.files.request(path);
        }
        self.files.request(&format!("{map_dir}/scene.json"));
        self.screen = Screen::Loading(Loading { encounter, period, hours, map_dir, scene: None, textures, player_key, foe_key });
    }

    /// Everything a battle needs is in: textures up, map built, battle made.
    fn finish_loading(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> Result<bool, String> {
        let Screen::Loading(load) = &mut self.screen else { return Ok(false) };
        if load.scene.is_none() {
            match self.files.get(&format!("{}/scene.json", load.map_dir)) {
                Load::Ready(b) => {
                    let scene = MapScene::parse(std::str::from_utf8(&b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    for file in scene.sheets.values() {
                        self.files.request(&format!("{}/{file}", load.map_dir));
                    }
                    load.scene = Some(scene);
                }
                Load::Failed(e) => return Err(e),
                Load::Pending => return Ok(false),
            }
        }
        let scene = load.scene.as_ref().unwrap();
        let sheets: Vec<String> = scene.sheets.values().map(|f| format!("{}/{f}", load.map_dir)).collect();
        for path in &sheets {
            match self.files.get(path) {
                Load::Pending => return Ok(false),
                Load::Failed(e) => return Err(e),
                _ => {}
            }
        }
        for (path, _) in &load.textures {
            if let Load::Pending = self.files.get(path) {
                return Ok(false);
            }
        }
        let rs = frame.wgpu_render_state().ok_or("no wgpu")?;
        let mut guard = rs.renderer.write();
        let renderer = guard.callback_resources.get_mut::<SceneRenderer>().ok_or("no scene renderer")?;
        for (sheet, file) in &scene.sheets {
            let key = format!("{}/{sheet}", scene.map);
            if !renderer.has_texture(&key) {
                if let Load::Ready(b) = self.files.get(&format!("{}/{file}", load.map_dir)) {
                    let (rgba, w, h) = decode(&b)?;
                    renderer.add_texture(&rs.device, &rs.queue, &key, &rgba, w, h, false);
                }
            }
        }
        for (path, key) in &load.textures {
            let Load::Ready(b) = self.files.get(path) else { continue };
            if let Some(species) = key.strip_prefix("icon:") {
                if !self.icons.contains_key(species) {
                    let (rgba, w, h) = decode(&b)?;
                    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    self.icons.insert(species.to_string(), ctx.load_texture(format!("icon-{species}"), img, egui::TextureOptions::NEAREST));
                }
            } else if !renderer.has_texture(key) {
                let (rgba, w, h) = decode(&b)?;
                renderer.add_texture(&rs.device, &rs.queue, key, &rgba, w, h, false);
            }
        }
        let spot = find_spot(scene);
        if renderer.map_name.as_deref() != Some(scene.map.as_str()) {
            renderer.clear_world();
            renderer.load_map(&rs.device, scene, spot);
        }
        drop(guard);
        let (encounter, period, hours) = (load.encounter, load.period, load.hours);
        let (player_key, foe_key) = (load.player_key.clone(), load.foe_key.clone());
        self.fielded = self.battle_order();
        // the party as it goes into the battle, for the checks (the main first)
        let ids = crate::team::ids(&self.party);
        platform::set_hook("battleparty", &self.fielded.iter().filter_map(|&i| ids.get(i).map(String::as_str)).collect::<Vec<_>>().join(","));
        let director = self.make_battle(encounter, period, hours, spot, player_key, foe_key);
        self.screen = Screen::Battle(Box::new(director), encounter);
        Ok(true)
    }

    /// The party in the order it goes into a battle (its places): in the story, the main
    /// first (it's sent out first); else as it is.
    fn battle_order(&self) -> Vec<usize> {
        match self.parked.as_ref().filter(|ed| ed.playing()) {
            Some(ed) => ed.team.battle_order(&crate::team::ids(&self.party)),
            None => (0..self.party.len()).collect(),
        }
    }

    fn make_battle(&self, encounter: usize, period: Period, hours: f32, spot: render::map::Spot, player_key: Option<String>, foe_key: Option<String>) -> Director {
        let dex = self.dex.clone().unwrap();
        let content = self.content.clone().unwrap();
        let e = &content.encounters.entries()[encounter].value;
        let cfg = self.config();
        let mut rng = Rng::new((platform::now_ms() as u64 % 1_000_000_007) as u32 ^ 0x9e37_79b9);
        let party: Vec<Mon> = self
            .fielded
            .iter()
            .filter_map(|&i| self.party.get(i))
            .map(|s| {
                let mut s = s.clone();
                if self.options.match_level {
                    s.level = e.level;
                }
                create_mon(&dex, &s, Some(&mut rng), &cfg)
            })
            .collect();
        let kind = kind_of(e);
        let mut wild_pool = vec![];
        let foe_party: Vec<Mon> = if kind == BattleKind::Wild {
            wild_pool = pool(e, period);
            let (sp, lv) = rng.pick(&wild_pool).clone();
            vec![create_mon(&dex, &MonSpec::new(&sp, lv), Some(&mut rng), &cfg)]
        } else {
            e.party.iter().map(|m| create_mon(&dex, &spec(m, e.move_level), Some(&mut rng), &cfg)).collect()
        };
        let battle = Battle::new(
            dex,
            BattleSetup {
                kind, size: e.size.max(1) as usize, seed: rng.int(1, 1_000_000_000) as u32, config: cfg, period,
                player_name: "You".into(), player_party: party, bag: BTreeMap::from(DEFAULT_BAG),
                foe_name: e.name.clone(), foe_party, pool: wild_pool,
            },
        );
        let mut stage = Stage::new(spot, hours);
        stage.toon = crate::stage::toon_look(&content.world().toon);
        stage.wind = crate::stage::wind_look(&content.world().wind);
        stage.radiance = crate::stage::radiance_look(&content.world().light);
        stage.content = Some(content.clone());
        stage.add_people(player_key, foe_key);
        let hud = Hud { icons: self.icons.clone(), ..Default::default() };
        Director::new(battle, stage, hud, self.options.auto, [220.0, 420.0, 700.0][self.options.text])
    }

    /// The team's pictures, for the HUD: each creature's icon asked for, made a texture once
    /// it's in, and its front picture for its card (the part with the creature in it framed).
    fn team_icons(&mut self, ctx: &egui::Context) {
        let Some(dex) = self.dex.clone() else { return };
        for s in &self.party {
            let Some(sp) = dex.try_sp(&s.species).map(|i| &dex.species[i]) else { continue };
            if let (false, Some(path)) = (self.icons.contains_key(&s.species), sp.icon.as_ref()) {
                if let Load::Ready(b) = self.files.get(path) {
                    if let Ok((rgba, w, h)) = decode(&b) {
                        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                        self.icons.insert(s.species.clone(), ctx.load_texture(format!("icon-{}", s.species), img, egui::TextureOptions::NEAREST));
                    }
                }
            }
            if let (false, Some(path)) = (self.pictures.contains_key(&s.species), sp.front.as_ref()) {
                if let Load::Ready(b) = self.files.get(path) {
                    if let Ok((rgba, w, h)) = decode(&b) {
                        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                        let texture = ctx.load_texture(format!("front-{}", s.species), img, egui::TextureOptions::NEAREST);
                        // its visible pixels (all of it when they aren't known)
                        let [x0, y0, x1, y1] = sp.front_box;
                        let (lo, hi) = if x1 > x0 && y1 > y0 { (egui::pos2(x0 as f32, y0 as f32), egui::pos2(x1 as f32, y1 as f32)) } else { (egui::Pos2::ZERO, egui::pos2(w as f32, h as f32)) };
                        let uv = egui::Rect::from_min_max(egui::pos2(lo.x / w as f32, lo.y / h as f32), egui::pos2(hi.x / w as f32, hi.y / h as f32));
                        self.pictures.insert(s.species.clone(), (texture, uv, hi - lo));
                    }
                }
            }
        }
    }

    /// The team as the HUD shows it: each creature's id, name (its own, else its kind's),
    /// level and icon, in its order, and its card for the board (View all): its kind, number,
    /// picture, types, status, HP, MP, Exp, base stats, lifeforce (the story's) and moves. Out of
    /// battle it's as it is between battles now: whole (each battle starts it so, none of it
    /// kept after), and where its level's Exp begins (none is earned yet).
    fn team_members(&self) -> Vec<crate::play_hud::Member> {
        use crate::play_hud::{Card, Member, MoveInfo, TypeTag};
        let cfg = self.config();
        let content = self.content.as_deref();
        let max_lifeforce = content.map_or(100, |c| c.world().lifeforce);
        let log = match &self.screen {
            Screen::Overworld(ed) => Some(&ed.team),
            _ => None,
        };
        // a type by its id (or its name, as the dex has a move's), as the cards show it
        let tag = |t: &content::TypeDef| TypeTag { name: t.name.clone(), color: Color32::from_hex(&t.color).unwrap_or(Color32::GRAY), order: t.order };
        self.party
            .iter()
            .zip(crate::team::ids(&self.party))
            .map(|(s, id)| {
                let creature = content.and_then(|c| c.creatures.get(&s.species));
                let kind = creature.map(|c| c.name.clone()).unwrap_or_else(|| s.species.clone());
                let name = s.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| kind.clone());
                let sp = self.dex.as_ref().and_then(|d| d.try_sp(&s.species).map(|i| &d.species[i]));
                let hp = sp.map_or(0, |sp| rules::mon::calc_stats(sp.base, s.level, s.ivs.unwrap_or([20; 6]), sp.base[0] == 1).hp);
                let max_mp = rules::mon::max_mp(s.level, &cfg);
                let growth = content.zip(creature).and_then(|(c, cr)| c.growth_rates.get(&cr.growth_rate));
                let exp_at = |level: u32| growth.and_then(|g| g.exp.get(level as usize).copied());
                let from = exp_at(s.level as u32).unwrap_or(0);
                let types = creature.map(|cr| cr.types.iter().filter_map(|t| content?.types.get(t)).filter(|t| !t.hidden).map(tag).collect()).unwrap_or_default();
                // its moves: their XP (the levels a party gives them at least), what they cost
                // and bring at each level
                let moves: Vec<MoveInfo> = match (self.dex.as_ref(), sp) {
                    (Some(dex), Some(_)) => {
                        let ids: Vec<u16> = if s.moves.is_empty() { rules::mon::default_moves(dex, dex.sp(&s.species), s.level) } else { s.moves.iter().map(|m| dex.move_id(m)).collect() };
                        ids.iter()
                            .enumerate()
                            .map(|(k, &mid)| {
                                let m = dex.mv(mid);
                                let mut xp = s.move_xp.get(k).copied().unwrap_or(0);
                                if let Some(&lv) = s.move_levels.get(k).filter(|&&lv| lv >= 1) {
                                    xp = xp.max(cfg.xp_levels[(lv.min(5) - 1) as usize]);
                                }
                                let level = rules::moves::level_from_xp(xp, &cfg);
                                let ty = dex.types.get(m.ty as usize).and_then(|name| content?.types.iter().find(|t| &t.name == name)).map(tag);
                                MoveInfo {
                                    name: m.name.clone(),
                                    ty,
                                    category: match m.category {
                                        rules::dex::Category::Physical => "Physical",
                                        rules::dex::Category::Special => "Special",
                                        rules::dex::Category::Status => "Status",
                                    },
                                    power: m.power,
                                    accuracy: m.accuracy,
                                    mp: rules::moves::mp_cost(m, dex.profile(mid), level, &cfg),
                                    level,
                                    xp,
                                    next: cfg.xp_levels.get(level as usize).copied(),
                                    description: m.description.clone(),
                                    levels: rules::moves::describe_levels(dex.profile(mid), &cfg),
                                }
                            })
                            .collect()
                    }
                    _ => vec![],
                };
                // the dex has them hp, atk, def, spe, spa, spd; the card HP, Atk, Def, SpA, SpD, Spe
                let b = sp.map_or([0; 6], |sp| sp.base);
                let card = Card {
                    kind,
                    number: sp.map_or(0, |sp| sp.number),
                    picture: self.pictures.get(&s.species).map(|(t, uv, px)| (t.id(), *uv, *px)),
                    types,
                    status: None,
                    hp: (hp, hp),
                    mp: (s.mp.unwrap_or(max_mp).clamp(0, max_mp), max_mp),
                    exp: (from, from, exp_at(s.level as u32 + 1)),
                    base: [b[0], b[1], b[2], b[4], b[5], b[3]],
                    lifeforce: (log.map_or(max_lifeforce, |l| l.lifeforce(&id, max_lifeforce)), max_lifeforce),
                    moves,
                };
                Member { id, name, level: s.level, icon: self.icons.get(&s.species).map(|t| t.id()), card }
            })
            .collect()
    }

    /// The party keeps what its moves learned (XP), not the levels set for the encounter (the
    /// battle's party in its order: `fielded`).
    fn keep_xp(&mut self, battle: &Battle) {
        for (k, &i) in self.fielded.iter().enumerate() {
            if let (Some(spec), Some(m)) = (self.party.get_mut(i), battle.side(Side::Player).party.get(k)) {
                spec.moves = m.moves.iter().map(|s| battle.dex.mv(s.id).key.clone()).collect();
                spec.move_xp = m.moves.iter().map(|s| s.xp).collect();
                spec.move_levels.clear();
            }
        }
        for m in &battle.caught {
            if self.party.len() < 6 {
                self.party.push(MonSpec::new(&battle.dex.species[m.species].key, m.level));
            }
        }
    }

    fn commands(&mut self) {
        let mut cmds: Vec<String> = vec![];
        while let Some(c) = platform::take_command() {
            cmds.extend(c.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
        if let Some(p) = self.cli.preset.take() {
            cmds.push(format!("start:{p}"));
        }
        if let Some(m) = self.cli.maps.take() {
            cmds.push(if m.is_empty() { "maps".into() } else { format!("map:{m}") });
        }
        if std::mem::take(&mut self.cli.requests) {
            cmds.push("requests".into());
        }
        if let Some(m) = self.cli.explore.take() {
            cmds.push(if m.is_empty() { "explore".into() } else { format!("explore:{m}") });
        }
        for c in cmds {
            let (verb, arg) = c.split_once(':').unwrap_or((c.as_str(), ""));
            match verb {
                "start" => {
                    let found = self.content.as_ref().and_then(|c| c.encounters.entries().iter().position(|e| e.value.id == arg));
                    if let Some(i) = found {
                        match self.screen {
                            Screen::Menu => self.battles_back = false,
                            Screen::Battles => self.battles_back = true,
                            _ => {}
                        }
                        if matches!(self.screen, Screen::Menu | Screen::Battles | Screen::Results(..)) {
                            self.start(i);
                        }
                    }
                }
                "auto" => {
                    self.options.auto = arg == "1";
                    if let Screen::Battle(d, _) = &mut self.screen {
                        d.auto = self.options.auto;
                    }
                }
                // a test's time of day ("day", "night", "18:30"); anything else: the real one
                "time" => self.options.time = platform::parse_clock(arg),
                "text" => self.options.text = match arg { "fast" => 0, "slow" => 2, _ => 1 },
                "scale" => self.options.graphics.scale = arg.parse::<u8>().unwrap_or(100).clamp(25, 100),
                "shadows" => {
                    if let Some(i) = Graphics::SHADOWS.iter().position(|(n, _)| n.eq_ignore_ascii_case(arg)) {
                        self.options.graphics.shadows = i;
                    }
                }
                // the indirect light: on or off
                "indirect" => self.options.graphics.indirect = !arg.eq_ignore_ascii_case("off"),
                // the old handhelds' looks: off, gba or gbc
                "pixelation" => match arg {
                    "off" => self.options.graphics.pixelation = false,
                    "on" => self.options.graphics.pixelation = true,
                    d => {
                        if let Ok(d) = d.parse::<f32>() {
                            self.options.graphics.pixelation = true;
                            self.options.graphics.density = d.clamp(0.0, 1.0);
                        }
                    }
                },
                "lcd" => self.options.graphics.lcd = Graphics::handheld(arg).unwrap_or(self.options.graphics.lcd),
                "levels" => {
                    if let Ok(lv) = arg.parse::<u8>() {
                        for s in self.party.iter_mut() {
                            s.move_xp.clear();
                            s.move_levels = vec![lv; s.moves.len().max(4)];
                        }
                    }
                }
                "editor" => {
                    if let (Screen::Menu, Some(c)) = (&self.screen, &self.content) {
                        self.screen = Screen::Editor(Box::new(Browser::new(c.clone())));
                    }
                }
                "maps" | "map" => {
                    if let (Screen::Menu, Some(c)) = (&self.screen, &self.content) {
                        let open = (!arg.is_empty()).then_some(arg);
                        self.screen = Screen::Maps(Box::new(MapEditor::new(c.clone(), open, self.files.clone())));
                    }
                }
                "explore" => {
                    if matches!(self.screen, Screen::Menu) {
                        self.explore((!arg.is_empty()).then_some(arg));
                    }
                }
                "battles" => {
                    if matches!(self.screen, Screen::Menu) && self.content.is_some() {
                        self.screen = Screen::Battles;
                    }
                }
                "forget" => {
                    platform::forget_text(SAVE_NAME);
                    self.last_save = None;
                    self.saved_at.0 = f64::NEG_INFINITY;
                }
                "view" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.set_view(arg);
                    }
                }
                "period" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.set_period(arg);
                    }
                }
                "point" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.probe(arg);
                    }
                }
                "brush" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.set_brush(arg);
                    }
                }
                "requests" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.open_requests();
                    }
                }
                // the checks: Publish's GitHub at a stand-in on the page's own address
                "github" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.github_at(arg);
                    }
                }
                // the checks: a file this browser kept (`data-keptfile`: its text, "removed",
                // or nothing)
                #[cfg(target_arch = "wasm32")]
                "kept" => {
                    let text = match crate::keep::files().get(arg) {
                        Some(crate::keep::Kept::File(b)) => String::from_utf8_lossy(b).to_string(),
                        Some(crate::keep::Kept::Removed) => "removed".to_string(),
                        None => String::new(),
                    };
                    platform::set_hook("keptfile", &text);
                }
                "walkto" => {
                    if let (Some(ed), Some((x, z))) = (self.editor_mut(), arg.split_once(',')) {
                        if let (Ok(x), Ok(z)) = (x.trim().parse(), z.trim().parse()) {
                            ed.walk_to(x, z);
                        }
                    }
                }
                "back" => {
                    if matches!(self.screen, Screen::Overworld(_)) {
                        self.leaving = true;
                    } else {
                        if let Screen::Maps(ed) = &self.screen {
                            self.content = Some(ed.content());
                        }
                        if matches!(self.screen, Screen::Results(..) | Screen::Editor(_) | Screen::Maps(_) | Screen::Battles) {
                            self.screen = Screen::Menu;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn frame_stats(&mut self, now: f64, cpu_ms: Option<f64>) {
        let gap = now - self.stats.last;
        if self.stats.last > 0.0 && gap > 25.0 {
            let what = match &self.screen {
                Screen::Battle(d, _) => format!("battle round {}", d.battle.round),
                Screen::Results(..) => "results".into(),
                Screen::Loading(_) => "loading".into(),
                Screen::Editor(_) => "editor".into(),
                Screen::Maps(_) => "map editor".into(),
                Screen::Overworld(_) => "overworld".into(),
                Screen::Battles => "test battles".into(),
                _ => "menu".into(),
            };
            self.hitches.push(format!("{gap:.0}ms at {:.1}s {what}", (now - self.started_at) / 1000.0));
            if self.hitches.len() > 30 {
                self.hitches.remove(0);
            }
            platform::set_hook("hitches", &self.hitches.join("; "));
            if self.cli.stats {
                println!("hitch {}", self.hitches.last().unwrap());
            }
        }
        let s = &mut self.stats;
        if s.last > 0.0 {
            s.worst = s.worst.max(now - s.last);
        }
        s.last = now;
        s.frames += 1;
        if let Some(c) = cpu_ms {
            s.cpu_sum += c;
            s.cpu_worst = s.cpu_worst.max(c);
        }
        if now - s.window_start >= 1000.0 {
            s.fps = s.frames;
            s.worst_gap = s.worst;
            s.cpu = s.cpu_sum / s.frames.max(1) as f64;
            s.cpu_peak = s.cpu_worst;
            s.frames = 0;
            s.worst = 0.0;
            s.cpu_sum = 0.0;
            s.cpu_worst = 0.0;
            s.window_start = now;
            platform::set_hook("fps", &s.fps.to_string());
            platform::set_hook("maxgap", &format!("{:.1}", s.worst_gap));
            platform::set_hook("cpu", &format!("{:.2}", s.cpu));
            platform::set_hook("cpupeak", &format!("{:.2}", s.cpu_peak));
            let d = self.draws;
            platform::set_hook("draws", &d.draws.to_string());
            platform::set_hook("tris", &d.triangles.to_string());
            platform::set_hook("shadowtris", &d.shadow_triangles.to_string());
            platform::set_hook("culled", &d.culled.to_string());
            platform::set_hook("grass", &d.grass.to_string());
            platform::set_hook("grassverts", &d.grass_vertices.to_string());
            platform::set_hook("grasslevels", &d.grass_levels.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
            platform::set_hook("grassgpu", if d.grass_on_gpu { "1" } else { "0" });
            platform::set_hook("blades", &d.blades.to_string());
            platform::set_hook("bladeverts", &d.blade_vertices.to_string());
            platform::set_hook("bladetemplates", &d.blade_templates.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
            platform::set_hook("bladesgpu", if d.blades_on_gpu { "1" } else { "0" });
            if self.cli.stats {
                let state = match &self.screen {
                    Screen::Battle(d, _) => format!("battle round {}", d.battle.round),
                    Screen::Results(..) => "results".into(),
                    Screen::Loading(_) => "loading".into(),
                    Screen::Maps(_) => "map editor".into(),
                    Screen::Overworld(_) => "overworld".into(),
                    _ => "menu".into(),
                };
                println!("fps {} worst-gap-ms {:.1} cpu-ms {:.2} (worst {:.2}) {state}", s.fps, s.worst_gap, s.cpu, s.cpu_peak);
            }
        }
    }

    /// The main menu: Continue, New game, Options, Exit, and the Editor in the corner, over
    /// the world (the map new games start on, its camera drifting, lit at the real time).
    fn menu_screen(&mut self, ui: &mut egui::Ui, now: f64, rs: Option<&eframe::egui_wgpu::RenderState>) {
        if self.menu_world.is_none() {
            if let Some(c) = self.content.clone() {
                let start = c.world().start.map;
                let map = if c.maps.contains(&start) { Some(start) } else { c.maps.iter().next().map(|m| m.id.clone()) };
                self.menu_world = map.map(|m| Box::new(MapEditor::new_backdrop(c, &m, self.files.clone())));
            }
        }
        let hours = self.hours();
        let retro = self.options.graphics.retro();
        let world_ready = match self.menu_world.as_mut() {
            Some(w) => {
                w.retro = retro;
                w.backdrop(ui.ctx(), rs, now, hours);
                w.backdrop_ready()
            }
            None => false,
        };
        // the saved story's place, looked at once a second (a file on the desktop)
        if now - self.saved_at.0 > 1000.0 {
            let place = platform::stored_text(SAVE_NAME).and_then(|t| serde_json::from_str::<SaveGame>(&t).ok()).map(|s| {
                self.content.as_ref().and_then(|c| c.maps.get(&s.map)).map(|m| m.name.clone()).unwrap_or(s.map)
            });
            self.saved_at = (now, place);
        }
        let saved = self.saved_at.1.clone();
        platform::set_hook("continue", if saved.is_some() { "1" } else { "" });
        platform::set_hook("keptoptions", &serde_json::to_string(&self.options.saved()).unwrap_or_default());
        let mut changed = false;
        let choice = self.title.ui(ui, &self.input, now, world_ready, saved.as_deref(), &mut self.options, &mut changed);
        if choice.is_some() {
            // off the menu: its world goes (the next screen has its own)
            self.menu_world = None;
        }
        if changed {
            self.keep_options();
        }
        match choice {
            Some(Choice::Continue) => self.explore(None),
            Some(Choice::NewGame) => self.new_game(),
            Some(Choice::Exit) => {
                if cfg!(target_arch = "wasm32") {
                    platform::close_window();
                } else {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Some(Choice::Editor) => self.open_editor(true),
            None => {}
        }
    }

    /// The options written down (and put to use), as the Options left them.
    fn keep_options(&mut self) {
        self.options.put_to_use(&mut self.audio);
        if let Ok(text) = serde_json::to_string_pretty(&self.options.saved()) {
            if let Err(e) = platform::store_text(OPTIONS_NAME, &text) {
                log::warn!("the options weren't kept: {e}");
            }
        }
    }

    /// The editor's Test battles: every encounter, by region, to play; the party, its own
    /// options; back to the editor.
    fn battles_screen(&mut self, ui: &mut egui::Ui) {
        let mut start = None;
        let mut back = false;
        let order = self.encounter_order();
        let content = self.content.clone();
        egui::Panel::top("battles-top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Test battles").strong());
                ui.add_space(12.0);
                let b = ui.button("Back to the editor").on_hover_text(crate::title::button_label(Button::B));
                platform::set_hook("uiBackToEditor", &format!("{:.0},{:.0}", b.rect.center().x, b.rect.center().y));
                if b.clicked() {
                    back = true;
                }
                if ui.button("Options").on_hover_text("Time of day, text, timing windows, graphics and sound (the main menu's)").clicked() {
                    self.battles_options = true;
                }
                ui.separator();
                ui.checkbox(&mut self.options.match_level, "Party matches each encounter's level");
                ui.checkbox(&mut self.options.auto, "AI plays your side");
            });
            ui.add_space(6.0);
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(12, 17, 28)).inner_margin(24.0)).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("Any encounter, to try it: its creatures, their moves and its map. The party is the content's player party.").color(MUTED));
                if let Some(why) = &self.no_battles {
                    ui.add_space(6.0);
                    ui.label(RichText::new(why).color(Color32::from_rgb(255, 150, 130)));
                }
                let Some(content) = content else { return };
                ui.add_space(8.0);
                let names: Vec<String> = self.party.iter().map(|s| content.creatures.get(&s.species).map(|c| c.name.clone()).unwrap_or_else(|| s.species.clone())).collect();
                if !names.is_empty() {
                    ui.label(RichText::new(format!("Your party: {}", names.join(", "))).color(MUTED));
                }
                let entries = content.encounters.entries();
                for region in content.regions.iter() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&region.name).size(18.0).strong().color(TEXT));
                    ui.label(RichText::new(&region.note).color(MUTED).size(12.0));
                    for (pos, &i) in order.iter().enumerate() {
                        let e = &entries[i];
                        if e.value.region != region.id {
                            continue;
                        }
                        let sel = pos == self.menu_sel;
                        let mut text = format!("{}   {}   Lv {}", kind_label(&e.value), e.value.name, e.value.level);
                        if e.space == Space::Temporary {
                            text += "   · temporary";
                        }
                        let b = egui::Button::new(RichText::new(text).size(14.0).color(TEXT)).fill(if sel { Color32::from_rgba_unmultiplied(255, 213, 74, 40) } else { Color32::from_white_alpha(10) }).min_size(Vec2::new(560.0, 30.0));
                        if ui.add(b).clicked() {
                            start = Some(i);
                        }
                        if !e.value.note.is_empty() {
                            ui.label(RichText::new(&e.value.note).color(MUTED).size(11.5));
                        }
                    }
                }
                ui.add_space(12.0);
                let buttons = crate::title::buttons_text(&crate::input::bindings(), crate::title::pad_shown());
                let a = crate::title::button_label(Button::A);
                ui.label(RichText::new(format!("{buttons}. Contact moves: press {a} at impact. Beams: hold {a}, release when the ring fills. Multi-hit: press {a} on every hit. Foe attacks: press {a} at impact to block.")).color(MUTED).size(12.0));
            });
        });
        let ctx = ui.ctx().clone();
        if self.battles_options {
            if crate::title::options_window(&ctx, &mut self.options, &mut self.battles_options, &mut self.battles_options_ui, &self.input) {
                self.keep_options();
            }
            return;
        }
        // the keyboard picks an encounter too; B goes back
        let n = order.len().max(1);
        if self.input.pressed(Button::Down) {
            self.menu_sel = (self.menu_sel + 1) % n;
        }
        if self.input.pressed(Button::Up) {
            self.menu_sel = (self.menu_sel + n - 1) % n;
        }
        if self.input.pressed(Button::A) && !order.is_empty() {
            start = Some(order[self.menu_sel.min(order.len() - 1)]);
        }
        if self.input.pressed(Button::B) && !ctx.egui_wants_keyboard_input() {
            back = true;
        }
        if back {
            self.open_editor(false);
            return;
        }
        if let Some(i) = start {
            self.battles_back = true;
            self.start(i);
        }
    }

    fn draw_scene(ui: &mut egui::Ui, director: &mut Director, now: f64) -> Rect {
        let rect = ui.ctx().content_rect();
        let frame = director.stage.frame(now, rect);
        ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(rect, SceneCallback { frame }));
        rect
    }

    fn encounter_name(&self, i: usize) -> String {
        self.content.as_ref().map(|c| c.encounters.entries()[i].value.name.clone()).unwrap_or_default()
    }
}

impl eframe::App for GameApp {
    /// A controller has just taken over: egui is told the mouse pointer is gone (where it
    /// rests, nothing shows hovered) until it moves again.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.pads.take_hide_pointer() {
            raw_input.events.push(egui::Event::PointerGone);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = platform::now_ms();
        let dt = ((now - self.stats.last).max(0.0) / 1000.0).min(0.05) as f32;
        self.frame_stats(now, frame.info().cpu_usage.map(|s| s as f64 * 1000.0));
        if let Some(rs) = frame.wgpu_render_state() {
            let mut guard = rs.renderer.write();
            if let Some(r) = guard.callback_resources.get_mut::<SceneRenderer>() {
                let q = self.options.graphics.quality();
                if r.quality() != q {
                    r.set_quality(&rs.device, q);
                }
                self.draws = r.stats();
            }
            // the HUD's frosted glass: the renderer's blurred copy of the picture, as a texture
            // egui can draw (registered again when it's made again)
            let frost = guard.callback_resources.get::<SceneRenderer>().and_then(|r| r.frost_view()).map(|(v, g)| (v.clone(), g));
            if let Some((view, generation)) = frost {
                if self.frost.map(|(_, g)| g) != Some(generation) {
                    let id = match self.frost {
                        Some((id, _)) => {
                            guard.update_egui_texture_from_wgpu_texture(&rs.device, &view, eframe::wgpu::FilterMode::Linear, id);
                            id
                        }
                        None => guard.register_native_texture(&rs.device, &view, eframe::wgpu::FilterMode::Linear),
                    };
                    self.frost = Some((id, generation));
                }
            }
        }
        platform::set_hook("frost", if self.frost.is_some() { "1" } else { "" });
        let g = self.options.graphics;
        let pixelation = if g.pixelation { format!("{:.2}", g.density) } else { "off".to_string() };
        platform::set_hook("retro", &format!("{pixelation}/{}", Graphics::HANDHELDS[g.lcd.min(2)].to_lowercase()));
        // how big the pixelation's pixels were last frame (the camera's zoom sets them too)
        platform::set_hook("pixel", &self.draws.pixel.to_string());
        let timing_read = matches!(&self.screen, Screen::Battle(d, _) if !d.menu_open());
        // the controllers first: who's listened to this frame, then what it pressed
        self.pads.update(&ctx, now);
        self.input.read(&ctx, now, timing_read);
        // browsers play no sound before a click or a key
        if ctx.input(|i| i.pointer.any_pressed() || !i.keys_down.is_empty()) {
            self.audio.unlock();
        }
        if let Some(limit) = self.cli.exit_after {
            if now - self.started_at > limit * 1000.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.content.is_none() {
            self.load_content();
        }
        if self.content.is_some() {
            self.commands();
        }
        let state = match &self.screen {
            Screen::Boot => "boot",
            Screen::Menu => "menu",
            Screen::Battles => "battles",
            Screen::Editor(_) => "editor",
            Screen::Maps(_) => "maps",
            Screen::Overworld(_) => "explore",
            Screen::Loading(_) => "loading",
            Screen::Battle(..) => "battle",
            Screen::Results(..) => "results",
        };
        platform::set_hook("state", state);
        // back on the main menu: it looks at the saved story again
        if state != self.last_state {
            self.saved_at.0 = f64::NEG_INFINITY;
            self.last_state = state;
        }
        platform::set_hook("audio", self.audio.state());

        if let Screen::Loading(_) = self.screen {
            if let Err(e) = self.finish_loading(&ctx, frame) {
                self.error = Some(e);
                self.screen = Screen::Menu;
            }
        }

        let hours = self.hours();
        let retro = self.options.graphics.retro();
        let mut options_changed = false;
        // the team, for the overworld's HUD (its pictures asked for, its cards made)
        let team = if matches!(self.screen, Screen::Overworld(_)) {
            self.team_icons(&ctx);
            self.team_members()
        } else {
            vec![]
        };
        match &mut self.screen {
            Screen::Boot | Screen::Loading(_) => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(12, 17, 28))).show(ui, |ui| {
                    ui.centered_and_justified(|ui| ui.label(RichText::new("Loading…").color(MUTED).size(18.0)));
                });
            }
            Screen::Menu => {
                self.audio.stop_all();
                self.menu_screen(ui, now, frame.wgpu_render_state())
            }
            Screen::Battles => {
                self.audio.stop_all();
                self.battles_screen(ui)
            }
            Screen::Editor(browser) => {
                if browser.ui(ui, &self.files) || self.input.pressed(Button::B) && !ctx.egui_wants_keyboard_input() {
                    self.open_editor(false);
                }
            }
            Screen::Maps(ed) => {
                let leave = ed.ui(ui, frame.wgpu_render_state(), now, &mut self.audio);
                let request = ed.take_request();
                if let Some(to) = leave {
                    let Screen::Maps(ed) = std::mem::replace(&mut self.screen, Screen::Menu) else { unreachable!() };
                    self.leave_editor(&ed);
                    self.screen = match to {
                        Leave::Menu => Screen::Menu,
                        Leave::Battles => Screen::Battles,
                        Leave::Content => match &self.content {
                            Some(c) => Screen::Editor(Box::new(Browser::new(c.clone()))),
                            None => Screen::Menu,
                        },
                    };
                } else if request.is_some() {
                    self.pending = request;
                }
            }
            Screen::Overworld(ed) => {
                // the game's look, the glass the HUD's buttons are; the world waits while the
                // HUD has the game's buttons
                ed.retro = retro;
                ed.frost = true;
                ed.hud_busy = self.hud.holds();
                ed.play_ui(ui, frame.wgpu_render_state(), now, &mut self.audio, hours);
                let request = ed.take_request();
                // the HUD over it: the map's name and the keys, the Team, Settings, Exit
                crate::layout::set_frost(self.frost.map(|(id, _)| (id, ctx.content_rect())));
                let map_name = ed.map_name().to_string();
                let busy = ed.story_busy();
                let out = self.hud.ui(&ctx, &map_name, busy, &team, &mut ed.team, &mut self.options, &self.input, &mut options_changed);
                let save = ed.save_game();
                if out == crate::play_hud::Out::Exit {
                    self.leaving = true;
                } else if request.is_some() {
                    self.pending = request;
                } else {
                    let changed = match &self.last_save {
                        Some(last) => last.map != save.map || last.flags != save.flags || last.team != save.team || (last.at != save.at && now - self.last_save_at > 2000.0),
                        None => true,
                    };
                    if changed {
                        self.last_save_at = now;
                        let text = serde_json::to_string(&save).unwrap_or_default();
                        if let Err(e) = platform::store_text(SAVE_NAME, &text) {
                            log::warn!("the story wasn't saved: {e}");
                        }
                        self.last_save = Some(save);
                    }
                }
            }
            Screen::Battle(director, encounter) => {
                director.stage.retro = retro;
                director.update(&ctx, &self.input, now, dt);
                for c in director.take_cries() {
                    self.audio.play_once(&self.files, &c, None, 1.0, 0.0, now);
                }
                for (path, volume) in director.stage.take_sounds() {
                    self.audio.play_once(&self.files, &path, None, volume, 0.0, now);
                }
                // a timeline's FX: its flipbooks as textures (read as they come; one that
                // can't be read is left out), its meshes as what moves in the scene
                if let Some(rs) = frame.wgpu_render_state() {
                    let mut guard = rs.renderer.write();
                    if let Some(r) = guard.callback_resources.get_mut::<SceneRenderer>() {
                        for (key, pixelated) in director.stage.fx_textures(now) {
                            if r.has_texture(&key) || self.fx_failed.contains(&key) {
                                continue;
                            }
                            self.files.request(&key);
                            match self.files.get(&key) {
                                Load::Ready(b) => match decode(&b) {
                                    Ok((rgba, w, h)) => r.add_texture(&rs.device, &rs.queue, &key, &rgba, w, h, !pixelated),
                                    Err(_) => {
                                        self.fx_failed.insert(key);
                                    }
                                },
                                Load::Failed(_) => {
                                    self.fx_failed.insert(key);
                                }
                                Load::Pending => {}
                            }
                        }
                        let batches = director.stage.fx_batches(now);
                        if !batches.is_empty() || self.fx_meshes_shown {
                            self.fx_meshes_shown = !batches.is_empty();
                            r.set_world_extra(&rs.device, &batches);
                        }
                    }
                }
                let wants: Vec<crate::audio::Want> = director
                    .stage
                    .vis
                    .iter()
                    .filter_map(|(uid, v)| v.sound.as_ref().map(|s| crate::audio::Want { key: format!("battler:{uid:?}"), file: s.file.clone(), pos: None, volume: s.volume as f32 * 0.5, range: 0.0 }))
                    .collect();
                self.audio.update(&self.files, glam::Vec3::ZERO, &wants, now);
                Self::draw_scene(ui, director, now);
                director.hud.draw(&ctx, &director.battle, &director.stage, now);
                let encounter = *encounter;
                platform::set_hook("round", &director.battle.round.to_string());
                platform::set_hook("menu", if director.menu_open() { "open" } else { "" });
                if director.finished() {
                    let Screen::Battle(director, _) = std::mem::replace(&mut self.screen, Screen::Menu) else { unreachable!() };
                    // the story's: who of the team fought (the HUD's Team counts them), before
                    // what was caught joins it
                    if let Some(ed) = self.parked.as_mut().filter(|ed| ed.playing()) {
                        let ids = crate::team::ids(&self.party);
                        let in_battle: Vec<String> = self.fielded.iter().filter_map(|&i| ids.get(i).cloned()).collect();
                        ed.team.record(crate::team::fought(&director.battle, &in_battle));
                        // each knocked out at the end loses lifeforce: its level's worth (its own,
                        // as its card says), up to the world's cap
                        let (max, cap) = self.content.as_ref().map(|c| c.world()).map_or((100, 20), |w| (w.lifeforce, w.lifeforce_ko_cap));
                        for (k, &i) in self.fielded.iter().enumerate() {
                            let (Some(id), Some(spec)) = (ids.get(i), self.party.get(i)) else { continue };
                            if director.battle.side(Side::Player).party.get(k).is_some_and(|m| m.hp <= 0) {
                                ed.team.knocked_out(id, max, crate::team::ko_cost(spec.level, cap));
                            }
                        }
                    }
                    self.keep_xp(&director.battle);
                    let results = Results {
                        encounter,
                        outcome: director.outcome(),
                        rounds: director.battle.round,
                        level_ups: director.level_ups.clone(),
                        caught: director.battle.caught.iter().map(|m| m.name.clone()).collect(),
                        since: now,
                    };
                    platform::set_hook("outcome", match results.outcome { Some(Outcome::Win) => "win", Some(Outcome::Lose) => "lose", Some(Outcome::Ran) => "ran", None => "" });
                    self.screen = Screen::Results(director, results);
                }
            }
            Screen::Results(..) => {
                let name = if let Screen::Results(_, r) = &self.screen { self.encounter_name(r.encounter) } else { String::new() };
                let Screen::Results(director, results) = &mut self.screen else { unreachable!() };
                director.stage.retro = retro;
                director.stage.update(now, dt);
                Self::draw_scene(ui, director, now);
                let mut choice = None;
                egui::Area::new(egui::Id::new("results")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                    egui::Frame::new().fill(Color32::from_rgb(20, 27, 41)).corner_radius(14.0).inner_margin(20.0).show(ui, |ui| {
                        ui.set_width(480.0);
                        let title = match results.outcome {
                            Some(Outcome::Win) => "Victory!",
                            Some(Outcome::Lose) => "Defeat",
                            Some(Outcome::Ran) => "Got away",
                            None => "Battle over",
                        };
                        ui.label(RichText::new(title).size(24.0).strong().color(TEXT));
                        ui.label(RichText::new(format!("{name} · {} rounds", results.rounds)).color(MUTED));
                        if !results.level_ups.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("Moves that leveled up").strong().color(TEXT));
                            for l in &results.level_ups {
                                ui.label(RichText::new(format!("• {l}")).color(TEXT).size(12.5));
                            }
                        }
                        if !results.caught.is_empty() {
                            ui.label(RichText::new(format!("Caught: {}", results.caught.join(", "))).color(TEXT));
                        }
                        ui.add_space(8.0);
                        // the buttons that answer, as the device in use has them
                        let again = format!("Fight again ({})", crate::title::button_label(Button::A));
                        let back_to = match &self.parked {
                            Some(ed) if ed.playing() => "Back to the map",
                            Some(_) => "Back to the editor",
                            None if self.battles_back => "Back to the battles",
                            None => "Back to the menu",
                        };
                        let back_to = format!("{back_to} ({})", crate::title::button_label(Button::B));
                        platform::set_hook("results", &format!("{again} · {back_to}"));
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new(again).color(Color32::from_rgb(34, 34, 34)).strong()).fill(ACCENT)).clicked() {
                                choice = Some(true);
                            }
                            if ui.button(back_to).clicked() {
                                choice = Some(false);
                            }
                        });
                    });
                });
                if now - results.since > 500.0 {
                    if self.input.pressed(Button::A) {
                        choice = Some(true);
                    }
                    if self.input.pressed(Button::B) {
                        choice = Some(false);
                    }
                }
                let encounter = results.encounter;
                match choice {
                    Some(true) => self.start(encounter),
                    Some(false) => match self.parked.take() {
                        Some(mut ed) => {
                            ed.resume_after_battle();
                            self.screen = Self::screen_of(ed);
                        }
                        None if self.battles_back => self.screen = Screen::Battles,
                        None => self.screen = Screen::Menu,
                    },
                    None => {}
                }
            }
        }

        if options_changed {
            self.keep_options();
        }
        if let Some(crate::mapedit::Request::Battle(id)) = self.pending.take() {
            self.battle_from_map(&id);
        }
        if std::mem::take(&mut self.leaving) {
            self.leave_overworld();
        }

        // frame times, top right; F3: the details
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("stats")));
        let r = ctx.content_rect();
        // (at the bottom in the map editor, whose top bar is full)
        let (at, align) = if matches!(self.screen, Screen::Maps(_) | Screen::Overworld(_)) { (Pos2::new(r.right() - 10.0, r.bottom() - 7.0), Align2::RIGHT_BOTTOM) } else { (Pos2::new(r.right() - 10.0, r.top() + 8.0), Align2::RIGHT_TOP) };
        // (not over the main menu: F3 has them there)
        if !matches!(self.screen, Screen::Menu) {
            painter.text(at, align, format!("{} fps · worst frame {:.1} ms", self.stats.fps, self.stats.worst_gap), FontId::proportional(11.0), Color32::from_white_alpha(150));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            self.perf_panel = !self.perf_panel;
        }
        if self.perf_panel {
            let (d, g) = (self.draws, self.options.graphics);
            let lines = [
                format!("{} fps · worst frame {:.1} ms", self.stats.fps, self.stats.worst_gap),
                format!("busy {:.2} ms a frame (worst {:.2})", self.stats.cpu, self.stats.cpu_peak),
                format!("scene: {} draws, {}k triangles", d.draws, d.triangles / 1000),
                format!("shadow: {} draws, {}k triangles", d.shadow_draws, d.shadow_triangles / 1000),
                format!("out of sight: {} squares", d.culled),
                format!(
                    "resolution {}% · shadows {} · pixelation {} · lcd {}",
                    g.scale,
                    Graphics::SHADOWS[g.shadows.min(3)].0.to_lowercase(),
                    if g.pixelation { format!("{:.2} ({} px)", g.density, d.pixel) } else { "off".to_string() },
                    Graphics::HANDHELDS[g.lcd.min(2)]
                ),
                "F3 hides this".to_string(),
            ];
            let font = FontId::monospace(11.0);
            // (under the HUD's buttons in the overworld)
            let top = r.top() + if matches!(self.screen, Screen::Overworld(_)) { 92.0 } else { 26.0 };
            let box_rect = Rect::from_min_max(Pos2::new(r.right() - 270.0, top - 4.0), Pos2::new(r.right() - 6.0, top + lines.len() as f32 * 15.0 + 4.0));
            painter.rect_filled(box_rect, 4.0, Color32::from_black_alpha(190));
            for (k, l) in lines.iter().enumerate() {
                painter.text(Pos2::new(box_rect.left() + 8.0, top + k as f32 * 15.0), Align2::LEFT_TOP, l, font.clone(), Color32::from_white_alpha(210));
            }
        }
        // what went wrong with the content or a battle's loading: a small box at the bottom
        // of the landing page, and nowhere else (the editors have their own status lines)
        if let (Some(e), Screen::Menu | Screen::Battles) = (&self.error, &self.screen) {
            let galley = painter.layout(e.clone(), FontId::proportional(12.0), Color32::from_rgb(255, 150, 130), (r.width() - 40.0).max(120.0));
            let text_rect = Rect::from_min_size(Pos2::new(r.left() + 16.0, r.bottom() - 16.0 - galley.size().y), galley.size());
            painter.rect_filled(text_rect.expand(6.0), 4.0, Color32::from_black_alpha(180));
            painter.galley(text_rect.min, galley, Color32::from_rgb(255, 150, 130));
        }
        // a controller come or gone; with one in hand, no mouse pointer over the game (the
        // editors keep theirs)
        self.pads.note_ui(&ctx, now);
        if crate::pads::on_pad() && !matches!(self.screen, Screen::Maps(_) | Screen::Editor(_)) {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        ctx.request_repaint();
    }
}
