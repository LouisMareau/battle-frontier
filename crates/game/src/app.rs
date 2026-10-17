//! The game's shell: the encounter menu, the content browser and the map editor, loading
//! what a battle needs, the battle itself (3D scene + HUD), the results; frame-time
//! statistics and the test hooks. Everything it shows comes from the content (both spaces).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use content::{Content, Space};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Vec2};
use render::map::{find_spot, MapScene};
use render::scene::{DrawStats, Frame, Quality, SceneRenderer};
use rules::battle::{Battle, BattleKind, BattleSetup, Outcome, Side};
use rules::config::{Config, Period};
use rules::dex::{Dex, Rng};
use rules::mon::{create_mon, Mon, MonSpec};

use crate::director::Director;
use crate::editor::Browser;
use crate::hud::{Hud, ACCENT, MUTED, TEXT};
use crate::input::{Button, Input};
use crate::mapedit::{MapEditor, SaveGame, SAVE_NAME};
use crate::platform::{self, Files, Load};
use crate::presets::{kind_label, kind_of, pool, spec, DEFAULT_BAG};
use crate::stage::Stage;

#[derive(Clone, Copy, PartialEq)]
pub enum TimeChoice {
    Clock,
    Fixed(Period),
}

pub struct Options {
    pub time: TimeChoice,
    /// 0 easy (wide windows), 1 normal, 2 hard
    pub difficulty: usize,
    pub assist: bool,
    /// 0 fast, 1 normal, 2 slow
    pub text: usize,
    pub match_level: bool,
    pub auto: bool,
    pub graphics: Graphics,
}

/// How much the GPU draws (for machines that can't keep up): the 3D scene's resolution
/// against the screen's, and the shadows' sharpness.
#[derive(Clone, Copy, PartialEq)]
pub struct Graphics {
    /// percent of the screen's resolution
    pub scale: u8,
    /// 0 none, 1 low, 2 medium, 3 high
    pub shadows: usize,
}

impl Default for Graphics {
    fn default() -> Self {
        Graphics { scale: 100, shadows: 2 }
    }
}

impl Graphics {
    pub const SHADOWS: [(&'static str, u32); 4] = [("Off", 0), ("Low", 1024), ("Medium", 2048), ("High", 4096)];

    fn quality(self) -> Quality {
        Quality { scale: self.scale as f32 / 100.0, shadow_size: Self::SHADOWS[self.shadows.min(3)].1 }
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
    Menu,
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
    /// the player's trainer picture, from the root
    player_sprite: Option<String>,
    options: Options,
    screen: Screen,
    input: Input,
    stats: Stats,
    icons: HashMap<String, egui::TextureHandle>,
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
        let options = Options {
            time: match cli.time.as_deref() {
                Some("morning") => TimeChoice::Fixed(Period::Morning),
                Some("day") => TimeChoice::Fixed(Period::Day),
                Some("night") => TimeChoice::Fixed(Period::Night),
                _ => TimeChoice::Clock,
            },
            difficulty: 1,
            assist: true,
            text: if cli.preset.is_some() { 0 } else { 1 },
            match_level: true,
            auto: cli.auto,
            graphics: Graphics::default(),
        };
        let mut audio = crate::audio::Audio::new();
        audio.enabled = !cli.mute;
        GameApp {
            files, content: None, dex: None, party: vec![], player_sprite: None, options, screen: Screen::Boot, input: Input::default(),
            stats: Stats::default(), icons: HashMap::new(), error: None, cli, started_at: platform::now_ms(), menu_sel: 0, hitches: vec![],
            draws: DrawStats::default(), perf_panel: false, audio, parked: None, pending: None, leaving: false, last_save: None, last_save_at: 0.0,
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
            let mut texts = vec![];
            for space in Space::ALL {
                match self.files.get(&format!("{}/bundle.json", space.dir())) {
                    Load::Pending => return,
                    Load::Ready(b) => texts.push(String::from_utf8_lossy(&b).to_string()),
                    Load::Failed(_) => {} // a space the site doesn't have
                }
            }
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            self.set_content(content::from_bundles(&refs));
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
                self.error = Some(format!("No battles yet: the content has no creatures ({e}). The editors work; add creatures to content/ (or local placeholders to temporary/)."));
                self.screen = Screen::Menu;
            }
        }
        self.content = Some(Arc::new(content));
    }

    fn period(&self) -> Period {
        match self.options.time {
            TimeChoice::Clock => Period::of_hour(platform::local_hour()),
            TimeChoice::Fixed(p) => p,
        }
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

    /// The overworld: the saved map (or `map`, or the first), walked from where the save
    /// left off, with the story's flags.
    fn explore(&mut self, map: Option<&str>) {
        let Some(c) = self.content.clone() else { return };
        let save: Option<SaveGame> = platform::stored_text(SAVE_NAME).and_then(|t| serde_json::from_str(&t).ok());
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

    /// Keeps the story between runs: when the map or the flags change, where the player
    /// stands every couple of seconds, and whenever `force` says so.
    fn save_story(&mut self, s: &SaveGame, force: bool) {
        let now = platform::now_ms();
        let changed = match &self.last_save {
            Some(last) => last.map != s.map || last.flags != s.flags || (last.at != s.at && now - self.last_save_at > 2000.0),
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
        let period = self.period();
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
        self.screen = Screen::Loading(Loading { encounter, period, map_dir, scene: None, textures, player_key, foe_key });
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
        let (encounter, period) = (load.encounter, load.period);
        let (player_key, foe_key) = (load.player_key.clone(), load.foe_key.clone());
        let director = self.make_battle(encounter, period, spot, player_key, foe_key);
        self.screen = Screen::Battle(Box::new(director), encounter);
        Ok(true)
    }

    fn make_battle(&self, encounter: usize, period: Period, spot: render::map::Spot, player_key: Option<String>, foe_key: Option<String>) -> Director {
        let dex = self.dex.clone().unwrap();
        let content = self.content.clone().unwrap();
        let e = &content.encounters.entries()[encounter].value;
        let cfg = self.config();
        let mut rng = Rng::new((platform::now_ms() as u64 % 1_000_000_007) as u32 ^ 0x9e37_79b9);
        let party: Vec<Mon> = self
            .party
            .iter()
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
        let mut stage = Stage::new(spot, period);
        stage.toon = crate::stage::toon_look(&content.world().toon);
        stage.add_people(player_key, foe_key);
        let hud = Hud { icons: self.icons.clone(), ..Default::default() };
        Director::new(battle, stage, hud, self.options.auto, [220.0, 420.0, 700.0][self.options.text])
    }

    /// The party keeps what its moves learned (XP), not the levels set for the encounter.
    fn keep_xp(&mut self, battle: &Battle) {
        for (i, spec) in self.party.iter_mut().enumerate() {
            if let Some(m) = battle.side(Side::Player).party.get(i) {
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
                        if matches!(self.screen, Screen::Menu | Screen::Results(..)) {
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
                "time" => {
                    self.options.time = match arg {
                        "morning" => TimeChoice::Fixed(Period::Morning),
                        "day" => TimeChoice::Fixed(Period::Day),
                        "night" => TimeChoice::Fixed(Period::Night),
                        _ => TimeChoice::Clock,
                    }
                }
                "text" => self.options.text = match arg { "fast" => 0, "slow" => 2, _ => 1 },
                "scale" => self.options.graphics.scale = arg.parse::<u8>().unwrap_or(100).clamp(25, 100),
                "shadows" => {
                    if let Some(i) = Graphics::SHADOWS.iter().position(|(n, _)| n.eq_ignore_ascii_case(arg)) {
                        self.options.graphics.shadows = i;
                    }
                }
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
                "forget" => {
                    platform::forget_text(SAVE_NAME);
                    self.last_save = None;
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
                "requests" => {
                    if let Some(ed) = self.editor_mut() {
                        ed.open_requests();
                    }
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
                        if matches!(self.screen, Screen::Results(..) | Screen::Editor(_) | Screen::Maps(_)) {
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

    fn menu_screen(&mut self, ui: &mut egui::Ui) {
        let mut start = None;
        let mut open_editor = false;
        let mut open_maps = false;
        let mut open_explore = false;
        let mut forget = false;
        let has_save = platform::stored_text(SAVE_NAME).is_some();
        let order = self.encounter_order();
        let content = self.content.clone();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(12, 17, 28)).inner_margin(24.0)).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(RichText::new("Battle Frontier").size(28.0).strong().color(TEXT));
                ui.label(RichText::new("Creature battles with timed hits and blocks, MP, moves that level up and change shape, locks on big attacks, rounds ordered by Speed. Each battle happens on its region's map.").color(MUTED));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Browse the content").clicked() {
                        open_editor = true;
                    }
                    if ui.button("Edit maps").clicked() {
                        open_maps = true;
                    }
                    ui.separator();
                    let explore = ui.add(egui::Button::new(RichText::new("Explore").strong())).on_hover_text("Walk the maps: the arrows move, A speaks to what's in front, Esc comes back. The story is kept.");
                    platform::set_hook("ui-explore", &format!("{:.0},{:.0}", explore.rect.center().x, explore.rect.center().y));
                    if explore.clicked() {
                        open_explore = true;
                    }
                    if has_save && ui.button("Forget the story").on_hover_text("Starts the story over: the flags and where you stood").clicked() {
                        forget = true;
                    }
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Time of day").color(TEXT));
                    let now = Period::of_hour(platform::local_hour()).name();
                    for (label, t) in [(format!("Clock (now: {now})"), TimeChoice::Clock), ("Morning".into(), TimeChoice::Fixed(Period::Morning)), ("Day".into(), TimeChoice::Fixed(Period::Day)), ("Night".into(), TimeChoice::Fixed(Period::Night))] {
                        ui.selectable_value(&mut self.options.time, t, label);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Timing windows").color(TEXT));
                    for (i, l) in ["Easy", "Normal", "Hard"].iter().enumerate() {
                        ui.selectable_value(&mut self.options.difficulty, i, *l);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.options.assist, "Timing rings");
                    ui.separator();
                    ui.label(RichText::new("Text").color(TEXT));
                    for (i, l) in ["Fast", "Normal", "Slow"].iter().enumerate() {
                        ui.selectable_value(&mut self.options.text, i, *l);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.options.match_level, "Party matches each encounter's level");
                    ui.checkbox(&mut self.options.auto, "AI plays your side");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Graphics").color(TEXT));
                    ui.label(RichText::new("resolution").color(MUTED));
                    for p in [100u8, 75, 50] {
                        ui.selectable_value(&mut self.options.graphics.scale, p, format!("{p}%"));
                    }
                    ui.separator();
                    ui.label(RichText::new("shadows").color(MUTED));
                    for (i, (name, _)) in Graphics::SHADOWS.iter().enumerate() {
                        ui.selectable_value(&mut self.options.graphics.shadows, i, *name);
                    }
                    ui.label(RichText::new("(lower these if the frame rate drops)").color(MUTED).size(11.5));
                });
                ui.add_space(8.0);
                let Some(content) = content else { return };
                let names: Vec<String> = self.party.iter().map(|s| content.creatures.get(&s.species).map(|c| c.name.clone()).unwrap_or_else(|| s.species.clone())).collect();
                ui.label(RichText::new(format!("Your party: {}", names.join(", "))).color(MUTED));
                ui.add_space(8.0);
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
                ui.label(RichText::new("A: Z / Space / Enter · B: X / Esc · d-pad: arrows · L / R: Q / E. Contact moves: press A at impact. Beams: hold A, release when the ring fills. Multi-hit: press A on every hit. Foe attacks: press A at impact to block.").color(MUTED).size(12.0));
            });
        });
        // the keyboard picks an encounter too
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
        if open_editor {
            if let Some(c) = &self.content {
                self.screen = Screen::Editor(Box::new(Browser::new(c.clone())));
                return;
            }
        }
        if open_maps {
            if let Some(c) = &self.content {
                self.screen = Screen::Maps(Box::new(MapEditor::new(c.clone(), None, self.files.clone())));
                return;
            }
        }
        if forget {
            platform::forget_text(SAVE_NAME);
            self.last_save = None;
        }
        if open_explore {
            self.explore(None);
            return;
        }
        if let Some(i) = start {
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = platform::now_ms();
        let dt = ((now - self.stats.last).max(0.0) / 1000.0).min(0.05) as f32;
        self.frame_stats(now, frame.info().cpu_usage.map(|s| s as f64 * 1000.0));
        if let Some(rs) = frame.wgpu_render_state() {
            if let Some(r) = rs.renderer.write().callback_resources.get_mut::<SceneRenderer>() {
                let q = self.options.graphics.quality();
                if r.quality() != q {
                    r.set_quality(&rs.device, q);
                }
                self.draws = r.stats();
            }
        }
        let timing_read = matches!(&self.screen, Screen::Battle(d, _) if !d.menu_open());
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
            Screen::Editor(_) => "editor",
            Screen::Maps(_) => "maps",
            Screen::Overworld(_) => "explore",
            Screen::Loading(_) => "loading",
            Screen::Battle(..) => "battle",
            Screen::Results(..) => "results",
        };
        platform::set_hook("state", state);
        platform::set_hook("audio", self.audio.state());

        if let Screen::Loading(_) = self.screen {
            if let Err(e) = self.finish_loading(&ctx, frame) {
                self.error = Some(e);
                self.screen = Screen::Menu;
            }
        }

        let period = self.period();
        match &mut self.screen {
            Screen::Boot | Screen::Loading(_) => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(12, 17, 28))).show(ui, |ui| {
                    ui.centered_and_justified(|ui| ui.label(RichText::new("Loading…").color(MUTED).size(18.0)));
                });
            }
            Screen::Menu => {
                self.audio.stop_all();
                self.menu_screen(ui)
            }
            Screen::Editor(browser) => {
                if browser.ui(ui, &self.files) || self.input.pressed(Button::B) && !ctx.egui_wants_keyboard_input() {
                    self.screen = Screen::Menu;
                }
            }
            Screen::Maps(ed) => {
                let back = ed.ui(ui, frame.wgpu_render_state(), now, &mut self.audio);
                let request = ed.take_request();
                if back {
                    self.content = Some(ed.content());
                    self.screen = Screen::Menu;
                    self.audio.stop_all();
                } else if request.is_some() {
                    self.pending = request;
                }
            }
            Screen::Overworld(ed) => {
                let leave = ed.play_ui(ui, frame.wgpu_render_state(), now, &mut self.audio, period);
                let request = ed.take_request();
                let save = ed.save_game();
                if leave {
                    self.leaving = true;
                } else if request.is_some() {
                    self.pending = request;
                } else {
                    let changed = match &self.last_save {
                        Some(last) => last.map != save.map || last.flags != save.flags || (last.at != save.at && now - self.last_save_at > 2000.0),
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
                director.update(&ctx, &self.input, now, dt);
                for c in director.take_cries() {
                    self.audio.play_once(&self.files, &c, None, 1.0, 0.0, now);
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
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new("Fight again (A)").color(Color32::from_rgb(34, 34, 34)).strong()).fill(ACCENT)).clicked() {
                                choice = Some(true);
                            }
                            if ui.button(match &self.parked { Some(ed) if ed.playing() => "Back to the map (B)", Some(_) => "Back to the editor (B)", None => "Back to the menu (B)" }).clicked() {
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
                        None => self.screen = Screen::Menu,
                    },
                    None => {}
                }
            }
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
        painter.text(at, align, format!("{} fps · worst frame {:.1} ms", self.stats.fps, self.stats.worst_gap), FontId::proportional(11.0), Color32::from_white_alpha(150));
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
                format!("resolution {}% · shadows {}", g.scale, Graphics::SHADOWS[g.shadows.min(3)].0.to_lowercase()),
                "F3 hides this".to_string(),
            ];
            let font = FontId::monospace(11.0);
            let top = r.top() + 26.0;
            let box_rect = Rect::from_min_max(Pos2::new(r.right() - 270.0, top - 4.0), Pos2::new(r.right() - 6.0, top + lines.len() as f32 * 15.0 + 4.0));
            painter.rect_filled(box_rect, 4.0, Color32::from_black_alpha(190));
            for (k, l) in lines.iter().enumerate() {
                painter.text(Pos2::new(box_rect.left() + 8.0, top + k as f32 * 15.0), Align2::LEFT_TOP, l, font.clone(), Color32::from_white_alpha(210));
            }
        }
        // what went wrong with the content or a battle's loading: a small box at the bottom
        // of the landing page, and nowhere else (the editors have their own status lines)
        if let (Some(e), Screen::Menu) = (&self.error, &self.screen) {
            let galley = painter.layout(e.clone(), FontId::proportional(12.0), Color32::from_rgb(255, 150, 130), (r.width() - 40.0).max(120.0));
            let text_rect = Rect::from_min_size(Pos2::new(r.left() + 16.0, r.bottom() - 16.0 - galley.size().y), galley.size());
            painter.rect_filled(text_rect.expand(6.0), 4.0, Color32::from_black_alpha(180));
            painter.galley(text_rect.min, galley, Color32::from_rgb(255, 150, 130));
        }
        ctx.request_repaint();
    }
}
