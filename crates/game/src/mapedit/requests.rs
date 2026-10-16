//! Model requests: pictures of a creature or a prop dropped on the Requests window, for a
//! Claude session to model in Blender when asked to "make the waiting requests" (the recipe
//! is `tools/blender/requests.md`). A request is `private/requests/<id>/request.json` with
//! its pictures in the same folder; once done, renders of the model and the prop it became.
//! private/ is never committed: the pictures are often other artists' work, and the model of
//! a design that isn't the user's own (or licensed) goes to temporary/, never content/.
//! The desktop only: the browser can't keep files there, so its window just says so.

// the record is the desktop's: the browser's window only says it can't keep requests
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use serde::{Deserialize, Serialize};

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
    #[default]
    Creature,
    Prop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Style {
    /// rounded, smooth shaded
    #[default]
    Smooth,
    /// few faces, flat shaded
    LowPoly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    #[default]
    Waiting,
    InProgress,
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
    pub kind: Kind,
    /// how tall it stands, in cells
    pub height: f64,
    pub style: Style,
    /// what to know: colours, what the pictures don't show, what matters most
    pub notes: String,
    /// the design is the user's own, or licensed to them: its model goes to content/ (the
    /// public game); else to temporary/ (never committed)
    pub mine: bool,
    pub status: Status,
    /// while it's modelled, how it goes (and once done, how long it took)
    pub progress: Progress,
    /// the reference pictures: files in its folder
    pub images: Vec<String>,
    /// once done, renders of the model (front, side, three-quarter): files in its folder
    pub previews: Vec<String>,
    /// once done, the prop it became
    pub prop: String,
    /// what the modeller says back: what it guessed, what to look at
    pub reply: String,
}

impl Default for ModelRequest {
    fn default() -> Self {
        ModelRequest {
            id: String::new(),
            name: String::new(),
            kind: Kind::Creature,
            height: 1.0,
            style: Style::Smooth,
            notes: String::new(),
            mine: false,
            status: Status::Waiting,
            progress: Progress::default(),
            images: vec![],
            previews: vec![],
            prop: String::new(),
            reply: String::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
use desk::Desk;

#[derive(Default)]
pub(super) struct Requests {
    pub open: bool,
    /// pictures hovering over the window, on their way here
    pub hovering: bool,
    /// what the status line says, once
    pub said: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    desk: Desk,
}

impl Requests {
    /// The top bar's "Requests", with how many wait on its corner and, while one is being
    /// modelled, its bar along the bottom (drawn on the button: the top bar is full).
    pub fn toggle(&mut self, ui: &mut egui::Ui) {
        let waiting = self.waiting();
        let mut about = "Pictures of creatures and props for Claude to model in Blender: drop PNG, JPG or WebP files on the window while it's open".to_string();
        if waiting > 0 {
            about += &format!("\n{waiting} waiting");
        }
        #[cfg(not(target_arch = "wasm32"))]
        let working = self.desk.working();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((name, _, text)) = &working {
            about += &format!("\n{name}: {text}");
        }
        let r = ui.selectable_label(self.open, "Requests").on_hover_text(about);
        super::mark("uiRequests", &r);
        if r.clicked() {
            self.open = !self.open;
        }
        let p = ui.painter();
        if waiting > 0 {
            let at = r.rect.right_top() + egui::vec2(-1.0, 2.0);
            p.circle_filled(at, 6.5, egui::Color32::from_rgb(255, 200, 90));
            p.text(at, egui::Align2::CENTER_CENTER, waiting.min(99).to_string(), egui::FontId::proportional(9.5), egui::Color32::BLACK);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((_, fraction, _)) = working {
            let line = egui::Rect::from_min_size(r.rect.left_bottom() + egui::vec2(3.0, -2.0), egui::vec2(r.rect.width() - 6.0, 2.0));
            p.rect_filled(line, 1.0, egui::Color32::from_gray(60));
            p.rect_filled(egui::Rect::from_min_size(line.min, egui::vec2(line.width() * fraction, 2.0)), 1.0, egui::Color32::from_rgb(110, 170, 255));
            // kept moving by the clock
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Requests {
    /// Pictures dropped while the window is open: the browser can't keep them.
    pub fn take(&mut self, files: Vec<egui::DroppedFileHandle>) {
        self.said = Some(format!(
            "{} picture{} not kept: requests live in private/requests/ on your computer, so they need the desktop editor (cargo run -p engine-game --release -- --requests)",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        ));
    }

    pub fn waiting(&self) -> usize {
        0
    }

    /// The window: a note.
    pub fn window(&mut self, ctx: &egui::Context, _content: &content::Content) -> Option<String> {
        crate::platform::set_hook("requests", if self.open { "note" } else { "" });
        let mut open = self.open;
        egui::Window::new("Model requests").id(egui::Id::new("model-requests")).open(&mut open).collapsible(false).resizable(false).default_width(420.0).default_pos([270.0, 70.0]).show(ctx, |ui| {
            ui.label("Drop pictures of a creature or a prop here, and Claude makes a base 3D model of it in Blender.");
            ui.add_space(4.0);
            super::warn(ui, "Requests need the desktop app: they're kept in private/requests/ on your computer, which the browser can't write to.");
            super::hint(ui, "cargo run -p engine-game --release -- --requests opens the map editor with this window.");
        });
        self.open = open;
        None
    }
}

/// The desktop: the requests on disk, the window, the pictures' thumbnails.
#[cfg(not(target_arch = "wasm32"))]
mod desk {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::SystemTime;

    use content::{Content, Space};
    use egui::{Color32, RichText, Sense};

    use super::super::assets::{slug, title};
    use super::super::{badge, hint, mark, section, warn};
    use super::{Kind, ModelRequest, Requests, Status, Style};
    use crate::platform::{self, Dropped};

    const WAITING: Color32 = Color32::from_rgb(255, 200, 90);
    const WORKING: Color32 = Color32::from_rgb(110, 170, 255);
    const DONE: Color32 = Color32::from_rgb(110, 210, 130);
    /// a request in progress whose record hasn't changed for this long (minutes) may have
    /// been left: the window says so
    const QUIET: f64 = 20.0;

    /// Seconds since 1970, as the modeller writes them.
    pub(in super::super) fn now_secs() -> f64 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }

    // ------------------------------------------------------------ on disk

    /// Where the requests are, under the project.
    pub(in super::super) fn folder(root: &Path) -> PathBuf {
        root.join("private").join("requests")
    }

    fn record_path(root: &Path, id: &str) -> PathBuf {
        folder(root).join(id).join("request.json")
    }

    pub(in super::super) fn read(root: &Path, id: &str) -> Result<ModelRequest, String> {
        let path = record_path(root, id);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut r: ModelRequest = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        // the folder is the request's id, whatever the file says
        r.id = id.to_string();
        Ok(r)
    }

    /// Writes its request.json, laid out as the project's records are.
    pub(in super::super) fn save(root: &Path, r: &ModelRequest) -> Result<(), String> {
        let path = record_path(root, &r.id);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, content::to_text(r)).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every request (waiting first, then in progress, then done; by name), and what
    /// couldn't be read.
    pub(in super::super) fn load_all(root: &Path) -> (Vec<ModelRequest>, Vec<String>) {
        let (mut out, mut problems) = (vec![], vec![]);
        let Ok(list) = std::fs::read_dir(folder(root)) else { return (out, problems) };
        for e in list.flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            if !e.path().join("request.json").is_file() {
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
        while folder(root).join(&id).exists() {
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
        let r = ModelRequest { id: free_id(root, &name), name, ..Default::default() };
        save(root, &r)?;
        Ok(r)
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
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        let base = match slug(stem) {
            s if s.is_empty() => "picture".to_string(),
            s => s,
        };
        let mut file = format!("{base}.{ext}");
        let mut n = 2;
        loop {
            let path = dir.join(&file);
            if !path.exists() {
                break;
            }
            if std::fs::read(&path).map(|b| b == bytes).unwrap_or(false) {
                if !r.images.contains(&file) {
                    r.images.push(file.clone());
                }
                return Ok(Added::Already(file));
            }
            file = format!("{base}_{n}.{ext}");
            n += 1;
        }
        let path = dir.join(&file);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        r.images.push(file.clone());
        Ok(Added::New(file))
    }

    /// A picture taken out of the request: its copy in the folder goes too.
    pub(in super::super) fn remove_picture(root: &Path, r: &mut ModelRequest, file: &str) -> Result<(), String> {
        r.images.retain(|f| f != file);
        let path = folder(root).join(&r.id).join(file);
        match std::fs::remove_file(&path) {
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
        std::fs::rename(&from, &to).map_err(|e| format!("{} to {}: {e}", from.display(), to.display()))?;
        r.id = id.clone();
        save(root, r)?;
        Ok(Some(id))
    }

    /// The request and its folder gone (the prop it became stays in the project).
    pub(in super::super) fn delete(root: &Path, id: &str) -> Result<(), String> {
        let dir = folder(root).join(id);
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
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
        let Ok(list) = std::fs::read_dir(folder(root)) else { return out };
        for e in list.flatten() {
            let Ok(files) = std::fs::read_dir(e.path()) else { continue };
            for f in files.flatten() {
                if let Ok(t) = f.metadata().and_then(|m| m.modified()) {
                    out.push((f.path(), t));
                }
            }
        }
        out.sort();
        out
    }

    /// Opens a file or a folder the way the system does (a picture in its viewer, a folder
    /// in the file manager).
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

    // ------------------------------------------------------------ thumbnails

    /// A picture made small for the window: at most 320 pixels on its longer side.
    pub(in super::super) fn thumbnail(path: &Path) -> Result<egui::ColorImage, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let img = image::load_from_memory(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let small = img.thumbnail(320, 320).to_rgba8();
        let (w, h) = small.dimensions();
        Ok(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], small.as_raw()))
    }

    /// A picture read on the thread: which file, which time it was asked for, what came of it.
    type Read = (PathBuf, u64, Result<egui::ColorImage, String>);

    /// The pictures' textures, read by a thread of their own so the window never waits.
    #[derive(Default)]
    pub(in super::super) struct Thumbs {
        made: HashMap<PathBuf, Result<egui::TextureHandle, String>>,
        /// on their way, with the number of the asking (a file forgotten and asked again
        /// takes only its latest reading)
        asked: HashMap<PathBuf, u64>,
        asks: u64,
        jobs: Option<mpsc::Sender<(PathBuf, u64)>>,
        done: Arc<Mutex<Vec<Read>>>,
    }

    impl Thumbs {
        /// A picture's texture and its size, once read (None while on its way).
        pub(in super::super) fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<Result<(egui::TextureId, egui::Vec2), String>> {
            match self.made.get(path) {
                Some(Ok(t)) => return Some(Ok((t.id(), t.size_vec2()))),
                Some(Err(e)) => return Some(Err(e.clone())),
                None => {}
            }
            if !self.asked.contains_key(path) {
                self.asks += 1;
                self.asked.insert(path.to_path_buf(), self.asks);
                let jobs = self.jobs.get_or_insert_with(|| {
                    let (tx, rx) = mpsc::channel::<(PathBuf, u64)>();
                    let (done, ctx) = (self.done.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        for (path, n) in rx {
                            let made = thumbnail(&path);
                            done.lock().unwrap().push((path, n, made));
                            ctx.request_repaint();
                        }
                    });
                    tx
                });
                let _ = jobs.send((path.to_path_buf(), self.asks));
            }
            None
        }

        /// The pictures read since last time, made textures.
        pub(in super::super) fn take(&mut self, ctx: &egui::Context) {
            let done = std::mem::take(&mut *self.done.lock().unwrap());
            for (path, n, made) in done {
                if self.asked.get(&path) != Some(&n) {
                    continue; // forgotten meanwhile, or asked again since
                }
                self.asked.remove(&path);
                let made = made.map(|img| ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR));
                self.made.insert(path, made);
            }
        }

        /// A file changed or gone: read again next time.
        pub(in super::super) fn forget(&mut self, path: &Path) {
            self.made.remove(path);
            self.asked.remove(path);
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
    }

    impl Desk {
        pub(in super::super) fn root(&self) -> PathBuf {
            self.root.clone().unwrap_or_else(|| PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into())))
        }

        pub(in super::super) fn reload(&mut self) {
            let root = self.root();
            (self.list, self.problems) = load_all(&root);
            self.seen = snapshot(&root);
            self.loaded = true;
            if self.selected.as_ref().is_some_and(|id| !self.list.iter().any(|r| &r.id == id)) {
                self.selected = None;
            }
        }

        fn picture_path(&self, id: &str, file: &str) -> PathBuf {
            folder(&self.root()).join(id).join(file)
        }

        /// Minutes since the request's record last changed (its modeller's last word).
        fn quiet_for(&self, id: &str) -> Option<f64> {
            let path = record_path(&self.root(), id);
            let t = self.seen.iter().find(|(p, _)| *p == path)?.1;
            Some(SystemTime::now().duration_since(t).ok()?.as_secs_f64() / 60.0)
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
                    Ok(r) => r,
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
                    Some(format!("{}: now private/requests/{new}/", r.name))
                }
                Ok(None) => None,
                Err(e) => Some(format!("{}: its folder keeps its name ({e})", r.name)),
            };
            self.reload();
            said
        }

        /// What changed on disk (Claude's work, a picture added by hand) shows within a
        /// second.
        pub(in super::super) fn watch(&mut self, now: f64) {
            if now < self.next_look {
                return;
            }
            self.next_look = now + 1000.0;
            let seen = snapshot(&self.root());
            if self.loaded && seen == self.seen {
                return;
            }
            let before: HashMap<_, _> = self.seen.iter().cloned().collect();
            for (file, t) in &seen {
                if before.get(file) != Some(t) {
                    self.thumbs.forget(file);
                }
            }
            self.reload();
        }

        // ------------------------------------------------------------ drawing

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

        /// Where the next pictures go, lit while files hover.
        fn banner(&mut self, ui: &mut egui::Ui, hovering: bool) {
            let target = self.selected.as_ref().and_then(|id| self.list.iter().find(|r| &r.id == id)).map(|r| r.name.clone());
            let text = match (&target, hovering) {
                (Some(name), true) => format!("Let go: the pictures go to {name}"),
                (None, true) => "Let go: the pictures start a new request".to_string(),
                (Some(name), false) => format!("Pictures dropped on the window (PNG, JPG, WebP) go to {name}"),
                (None, false) => "Drop pictures of one creature or prop on the window (PNG, JPG, WebP): they start a new request".to_string(),
            };
            let stroke = if hovering { egui::Stroke::new(1.5, WAITING) } else { egui::Stroke::new(1.0, Color32::from_gray(70)) };
            egui::Frame::new().stroke(stroke).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(if hovering { WAITING } else { Color32::from_gray(200) }));
                    if target.is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let new = ui.button("New request").on_hover_text("The next pictures dropped start a new request");
                            mark("uiRequestNew", &new);
                            if new.clicked() {
                                self.selected = None;
                                self.deleting = false;
                            }
                        });
                    }
                });
            });
        }

        fn list_column(&mut self, ui: &mut egui::Ui) {
            let list = self.list.clone();
            if list.is_empty() && self.problems.is_empty() {
                hint(ui, "No requests yet.");
            }
            for r in &list {
                let chosen = self.selected.as_deref() == Some(r.id.as_str());
                let fill = if chosen { Color32::from_rgb(38, 52, 76) } else { Color32::from_gray(28) };
                let frame = egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        chip(ui, r.status);
                        ui.label(RichText::new(&r.name).strong());
                    });
                    hint(ui, &about(r));
                    if r.status == Status::InProgress {
                        bar(ui, r, ui.available_width(), 6.0);
                        hint(ui, &r.progress.at(now_secs()).1);
                    }
                    ui.horizontal_wrapped(|ui| {
                        for f in r.images.iter().take(4) {
                            let path = self.picture_path(&r.id, f);
                            self.picture(ui, &path, 34.0, Sense::hover());
                        }
                        if r.images.len() > 4 {
                            hint(ui, &format!("+{}", r.images.len() - 4));
                        }
                        if r.images.is_empty() {
                            hint(ui, "no pictures");
                        }
                    });
                    if r.status == Status::Done {
                        ui.horizontal(|ui| {
                            for f in r.previews.iter().take(3) {
                                let path = self.picture_path(&r.id, f);
                                self.picture(ui, &path, 34.0, Sense::hover());
                            }
                            if !r.prop.is_empty() {
                                ui.label(RichText::new(format!("→ {}", r.prop)).monospace().size(11.0).color(DONE));
                            }
                        });
                    }
                });
                let row = ui.interact(frame.response.rect, ui.id().with(("request-row", &r.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                mark(&format!("uiRequest_{}", r.id), &row);
                if row.clicked() {
                    self.selected = Some(r.id.clone());
                    self.deleting = false;
                }
                ui.add_space(4.0);
            }
            for p in &self.problems {
                warn(ui, p);
            }
        }

        /// The selected request, to edit; a prop to use when its "Use it" is clicked.
        fn pane(&mut self, ui: &mut egui::Ui, content: &Content, name_focused: &mut bool) -> (Option<String>, Option<String>) {
            let (mut use_prop, mut said) = (None, None);
            let Some(i) = self.selected.as_ref().and_then(|id| self.list.iter().position(|r| &r.id == id)) else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Drop pictures here").size(18.0).color(Color32::from_gray(170)));
                    hint(ui, "Front, side, back, close-ups of one creature or prop: one request.");
                    hint(ui, "Then ask Claude to \u{201c}make the waiting requests\u{201d}.");
                });
                return (None, None);
            };
            let before = self.list[i].clone();
            let mut r = before.clone();
            let mut remove = None;
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
                ui.label("Kind");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut r.kind, Kind::Creature, "creature").on_hover_text("Rigged: a spine, a head, limbs and a tail, and an idle animation");
                    ui.selectable_value(&mut r.kind, Kind::Prop, "prop");
                });
                ui.end_row();
                ui.label("Height");
                ui.add(egui::DragValue::new(&mut r.height).speed(0.05).range(0.1..=40.0).suffix(" cells").max_decimals(2)).on_hover_text("How tall it stands (a cell is a metre; the player is about 1.5)");
                ui.end_row();
                ui.label("Style");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut r.style, Style::Smooth, "smooth").on_hover_text("Rounded, smooth shaded");
                    ui.selectable_value(&mut r.style, Style::LowPoly, "low-poly").on_hover_text("Few faces, flat shaded");
                });
                ui.end_row();
                ui.label("Notes");
                let notes = ui.add(egui::TextEdit::multiline(&mut r.notes).desired_rows(3).desired_width(330.0).hint_text("Colours, what the pictures don't show, what matters most"));
                mark("uiRequestNotes", &notes);
                ui.end_row();
                ui.label("");
                let mine = ui.checkbox(&mut r.mine, "This design is mine or licensed").on_hover_text(
                    "Then its model goes into content/, the public game. Off: into temporary/, never committed (a design of someone else's stays a placeholder)",
                );
                mark("uiRequestMine", &mine);
                ui.end_row();
                ui.label("Status");
                ui.horizontal(|ui| {
                    chip(ui, r.status);
                    egui::ComboBox::from_id_salt("request-status").selected_text("change…").width(90.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut r.status, Status::Waiting, "waiting").on_hover_text("To model (again)");
                        ui.selectable_value(&mut r.status, Status::InProgress, "in progress");
                        ui.selectable_value(&mut r.status, Status::Done, "done");
                    });
                });
                ui.end_row();
            });
            if r.status == Status::InProgress {
                section(ui, "Being modelled");
                let shown = bar(ui, &r, 330.0, 18.0);
                mark("uiRequestBar", &shown);
                if let Some(quiet) = self.quiet_for(&r.id).filter(|q| *q > QUIET) {
                    warn(ui, &format!("No news for {} min: if no Claude session is on it any more, set it back to waiting", quiet.round()));
                }
            } else if r.status == Status::Done && r.progress.minutes > 0.0 {
                hint(ui, &format!("Modelled in {} min", r.progress.minutes.round()));
            }
            section(ui, &format!("Pictures ({})", r.images.len()));
            ui.horizontal_wrapped(|ui| {
                for f in r.images.clone() {
                    ui.vertical(|ui| {
                        let path = self.picture_path(&r.id, &f);
                        let shown = self.picture(ui, &path, 96.0, Sense::click()).on_hover_text(format!("{f}: click to open it"));
                        if shown.clicked() {
                            if let Err(e) = reveal(&path) {
                                said = Some(e);
                            }
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
            if !r.previews.is_empty() {
                section(ui, "The model");
                ui.horizontal_wrapped(|ui| {
                    for f in r.previews.clone() {
                        let path = self.picture_path(&r.id, &f);
                        let shown = self.picture(ui, &path, 130.0, Sense::click()).on_hover_text(format!("{f}: click to open it"));
                        if shown.clicked() {
                            if let Err(e) = reveal(&path) {
                                said = Some(e);
                            }
                        }
                    }
                });
            }
            if !r.prop.is_empty() {
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
            if !r.reply.is_empty() {
                section(ui, "What Claude says");
                ui.label(RichText::new(&r.reply).size(12.0));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Open the folder").on_hover_text("private/requests/<id>/ in the file manager").clicked() {
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
                return (use_prop, said);
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
            (use_prop, said)
        }
    }

    /// What it is, in a line: "creature · 1.2 cells · smooth · mine".
    fn about(r: &ModelRequest) -> String {
        let kind = match r.kind {
            Kind::Creature => "creature",
            Kind::Prop => "prop",
        };
        let style = match r.style {
            Style::Smooth => "smooth",
            Style::LowPoly => "low-poly",
        };
        format!("{kind} · {} cells · {style}{}", r.height, if r.mine { " · mine" } else { "" })
    }

    /// A request's progress bar, `width` by `height`, with its words on it when it's tall
    /// enough (and always when hovered).
    pub(in super::super) fn bar(ui: &mut egui::Ui, r: &ModelRequest, width: f32, height: f32) -> egui::Response {
        let (fraction, text) = r.progress.at(now_secs());
        let mut bar = egui::ProgressBar::new(fraction).desired_width(width).desired_height(height).fill(WORKING);
        if height >= 14.0 {
            bar = bar.text(RichText::new(&text).size(11.0).color(Color32::BLACK));
        }
        ui.add(bar).on_hover_text(format!("{}: {text}", r.name))
    }

    fn chip(ui: &mut egui::Ui, status: Status) {
        let (text, color) = match status {
            Status::Waiting => ("WAITING", WAITING),
            Status::InProgress => ("IN PROGRESS", WORKING),
            Status::Done => ("DONE", DONE),
        };
        ui.label(RichText::new(text).size(9.5).strong().color(Color32::BLACK).background_color(color));
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

        pub fn watch(&mut self, now: f64) {
            self.desk.watch(now);
        }

        /// The window, when open (and the pictures dropped, whether or not); a prop to use
        /// when a done request's "Use it" is clicked.
        pub fn window(&mut self, ctx: &egui::Context, content: &Content) -> Option<String> {
            let d = &mut self.desk;
            d.thumbs.take(ctx);
            if !d.loaded {
                d.reload();
            }
            if let Some(s) = d.take_arrived() {
                self.said = Some(s);
            }
            let (mut use_prop, mut name_focused) = (None, false);
            if self.open {
                let (mut open, hovering) = (true, self.hovering);
                let mut said = None;
                egui::Window::new(format!("Model requests ({})", d.list.len())).id(egui::Id::new("model-requests")).open(&mut open).default_size([780.0, 660.0]).default_pos([270.0, 70.0]).show(ctx, |ui| {
                    d.banner(ui, hovering);
                    ui.add_space(4.0);
                    let height = ui.available_height();
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(250.0);
                            egui::ScrollArea::vertical().id_salt("request-list").max_height(height).show(ui, |ui| d.list_column(ui));
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical().id_salt("request-pane").max_height(height).show(ui, |ui| {
                                (use_prop, said) = d.pane(ui, content, &mut name_focused);
                            });
                        });
                    });
                });
                if said.is_some() {
                    self.said = said;
                }
                self.open = open;
            }
            // the name no longer being typed (or the request no longer shown): its folder follows
            if !name_focused {
                if let Some(id) = self.desk.renamed.take() {
                    if let Some(s) = self.desk.follow(&id) {
                        self.said = Some(s);
                    }
                }
            }
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
            kind: Kind::Creature,
            height: 0.8,
            style: Style::LowPoly,
            notes: "Green back, cream belly".into(),
            mine: true,
            status: Status::InProgress,
            progress: Progress { step: "building the model".into(), done: 0.2, started: 1760000000.0, minutes: 25.0 },
            images: vec!["front.png".into(), "side.jpg".into()],
            ..Default::default()
        };
        let text = content::to_text(&r);
        assert!(text.contains("  \"kind\": \"creature\",\n") && text.contains("  \"style\": \"low_poly\",\n") && text.contains("  \"status\": \"in_progress\",\n"), "{text}");
        assert!(text.contains("  \"progress\": {\n    \"step\": \"building the model\",\n    \"done\": 0.2,\n    \"started\": 1760000000.0,\n    \"minutes\": 25.0\n  },\n"), "{text}");
        assert!(text.contains("  \"images\": [\"front.png\", \"side.jpg\"],\n"), "short lists on one line: {text}");
        assert!(text.starts_with("{\n  \"id\": \"moss_toad\",\n  \"name\": \"Moss toad\",\n"), "the fields in their order: {text}");
        let back: ModelRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, r);
        // a short record by hand: the rest are the defaults
        let short: ModelRequest = serde_json::from_str(r#"{ "id": "x", "name": "X", "kind": "prop" }"#).unwrap();
        assert_eq!((short.kind, short.height, short.style, short.status, short.mine), (Kind::Prop, 1.0, Style::Smooth, Status::Waiting, false));
        assert!(serde_json::from_str::<ModelRequest>(r#"{ "status": "finished" }"#).is_err(), "an unknown status is a mistake");
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
            let t = thumbnail(&path).unwrap();
            assert!(t.size[0] <= 320 && t.size[1] <= 320 && t.size[0] == 2 * t.size[1], "{file}: {:?}", t.size);
        }
        std::fs::write(root.join("d.png"), b"nope").unwrap();
        assert!(thumbnail(&root.join("d.png")).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    /// A frame of the window, headless, after what was dropped.
    fn frame(ctx: &egui::Context, req: &mut Requests, files: Vec<egui::DroppedFileHandle>) {
        let content = content::Content::default();
        if !files.is_empty() {
            req.take(files);
        }
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            req.toggle(ui);
            req.window(ui.ctx(), &content);
        });
        // no renderer here to take the textures
        out.textures_delta.clear();
    }

    #[test]
    fn pictures_dropped_on_the_window_make_requests() {
        let root = scratch("window");
        let ctx = egui::Context::default();
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
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn drops_go_to_the_window_only_while_it_is_open() {
        let root = scratch("routing");
        let mut ed = crate::mapedit::MapEditor::new(Arc::new(content::Content::default()), None, crate::platform::Files::new(root.to_str().unwrap()));
        ed.requests = Requests::at(&root);
        let ctx = egui::Context::default();
        let drop = |ed: &mut crate::mapedit::MapEditor, files: Vec<egui::DroppedFileHandle>| {
            let raw = egui::RawInput { dropped_files: files, ..Default::default() };
            let content = content::Content::default();
            let mut out = ctx.run_ui(raw, |ui| {
                ed.take_drops(ui.ctx());
                ed.requests.window(ui.ctx(), &content);
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
