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
    pub kind: Kind,
    /// how tall it stands, in cells
    pub height: f64,
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
    /// the design is the user's own, or licensed to them: its model goes to content/ (the
    /// public game); else to temporary/ (never committed)
    pub mine: bool,
    pub status: Status,
    /// while it's modelled, how it goes (and once done, how long it took)
    pub progress: Progress,
    /// the reference pictures: files in its folder
    pub images: Vec<String>,
    /// the drawings the modeller made to work from and to check by: its lines over each
    /// picture, the front and the side drawn from them, and once rendered those lines over the model
    /// (files in its folder; tools/blender/request_lines.py)
    pub lines: Vec<String>,
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
            details: String::new(),
            pose: Pose::TPose,
            shapes: Shapes::default(),
            notes: String::new(),
            mine: false,
            status: Status::Waiting,
            progress: Progress::default(),
            images: vec![],
            lines: vec![],
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
    use super::{Kind, ModelRequest, Pose, Progress, Requests, Status, Style, SHAPES_CAP};
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

    /// A picture taken out of the request: its copy in the folder goes too, and the lines
    /// drawn over it.
    pub(in super::super) fn remove_picture(root: &Path, r: &mut ModelRequest, file: &str) -> Result<(), String> {
        r.images.retain(|f| f != file);
        let dir = folder(root).join(&r.id);
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        let drawn = [format!("lines_{stem}.png"), format!("match_{stem}.png"), format!("seen_{stem}.png")];
        r.lines.retain(|f| !drawn.contains(f));
        for f in &drawn {
            std::fs::remove_file(dir.join(f)).ok();
        }
        let path = dir.join(file);
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

    // ------------------------------------------------------------ runs: Claude Code, through Python

    /// Python: PYTHON names it, else `python` (or `python3`) on the PATH, never the Store's
    /// stand-ins in WindowsApps, else Windows' `py`.
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

    /// A short one, waited for: true when it worked.
    fn python_now(root: &Path, args: &[&str]) -> bool {
        python(root, args).and_then(|mut c| c.wait().map_err(|e| e.to_string())).map(|s| s.success()).unwrap_or(false)
    }

    /// Claude Code, where run_claude.py looks for it: CLAUDE, the PATH, ~/.local/bin.
    fn find_claude() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("CLAUDE") {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
        let path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
        for dir in &path {
            for name in ["claude.exe", "claude.cmd", "claude"] {
                if dir.join(name).is_file() {
                    return Some(dir.join(name));
                }
            }
        }
        let home = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?);
        [home.join(".local/bin/claude.exe"), home.join(".local/bin/claude"), home.join(".claude/local/claude")].into_iter().find(|p| p.is_file())
    }

    /// Claude Code in a terminal of its own, in the project, to log in (`/login`): a run needs
    /// its command line logged in once, apart from the desktop app's.
    fn open_login(root: &Path) -> Result<String, String> {
        let claude = find_claude().ok_or("Claude Code not found: install it, or set CLAUDE to the claude program")?;
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/c", "start", "Claude Code: type /login"])
                .arg(&claude)
                .current_dir(root)
                .spawn()
                .map(|_| "Claude Code is opening in a terminal: type /login there (once), then Start again".to_string())
                .map_err(|e| format!("{}: {e}", claude.display()))
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Err(format!("Run {} in a terminal and type /login there (once), then Start again", claude.display()))
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
        if let Some(p) = stem.strip_prefix("lines_") {
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

        /// The request a run is on (or being started for), if any: one at a time.
        pub(in super::super) fn busy(&self) -> Option<String> {
            self.starting.as_ref().map(|(id, _)| id.clone()).or_else(|| self.in_progress().map(|r| r.id.clone()))
        }

        /// Start: Claude Code on the request, on its own (run_claude.py, which keeps the log
        /// and the status); the console opens on it.
        pub(in super::super) fn start_run(&mut self, id: &str) -> String {
            if let Some(other) = self.busy() {
                return format!("One at a time: {other} is being made");
            }
            let root = self.root();
            let model = self.model.clone();
            let mut args = vec!["tools/blender/run_claude.py", id];
            if !model.is_empty() {
                args.extend(["--model", model.as_str()]);
            }
            match python(&root, &args) {
                Ok(child) => {
                    self.children.push(child);
                    self.starting = Some((id.to_string(), now_secs()));
                    self.selected = Some(id.to_string());
                    self.console_open = true;
                    self.failed_silent.remove(id);
                    format!("Claude Code is starting on {id}: the console shows everything it does")
                }
                Err(e) => format!("Not started: {e}"),
            }
        }

        /// Cancel: the run's stop file (run_claude.py stops Claude and the request waits
        /// again); with no run there to read it, the editor does that a few seconds later.
        pub(in super::super) fn cancel_run(&mut self, id: &str) -> String {
            let stop = folder(&self.root()).join(id).join("stop");
            match std::fs::write(&stop, "") {
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
            if let Some((id, at)) = self.cancelling.clone() {
                let stop = folder(&root).join(&id).join("stop");
                if !stop.exists() {
                    self.cancelling = None;
                } else if now_secs() - at > 6.0 {
                    // no run took it: nothing was running
                    let _ = std::fs::remove_file(&stop);
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
            let wall = SystemTime::now();
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
            if self.loaded && seen == self.seen {
                return said;
            }
            let before: HashMap<_, _> = self.seen.iter().cloned().collect();
            for (file, t) in &seen {
                if before.get(file) != Some(t) {
                    self.thumbs.forget(file);
                }
            }
            self.reload();
            said
        }

        /// A request's log.txt, read again when it changes.
        fn log_of(&mut self, id: &str) -> Option<Arc<String>> {
            let path = folder(&self.root()).join(id).join("log.txt");
            let t = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            if self.logs.get(id).map(|(at, _)| *at != t).unwrap_or(true) {
                let text = std::fs::read_to_string(&path).ok()?;
                self.logs.insert(id.to_string(), (t, Arc::new(text)));
            }
            self.logs.get(id).map(|(_, s)| s.clone())
        }

        /// The console at the window's foot: the log of the request selected (else of the one
        /// being made), its last lines when closed, all of it open; Copy takes it all.
        fn console(&mut self, ui: &mut egui::Ui, id: Option<String>) -> Option<String> {
            let mut said = None;
            let log = id.as_deref().and_then(|id| self.log_of(id));
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
                        if ui.small_button("Open log.txt").clicked() {
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if target.is_some() {
                            let new = ui.button("New request").on_hover_text("The next pictures dropped start a new request");
                            mark("uiRequestNew", &new);
                            if new.clicked() {
                                self.selected = None;
                                self.deleting = false;
                            }
                        }
                        let shown = MODELS.iter().find(|(m, _)| *m == self.model).map(|(_, label)| *label).unwrap_or(self.model.as_str()).to_string();
                        let pick = egui::ComboBox::from_id_salt("request-model").selected_text(shown).width(110.0).show_ui(ui, |ui| {
                            for (m, label) in MODELS {
                                ui.selectable_value(&mut self.model, m.to_string(), label);
                            }
                        });
                        pick.response.on_hover_text("The model Claude Code runs on for the next Start (the console says which it got)");
                        hint(ui, "Claude:");
                    });
                });
            });
        }

        /// A request's run button: Start (Start again, Make it again) or Cancel, and what was
        /// asked for. One run at a time.
        fn run_button(&self, ui: &mut egui::Ui, r: &ModelRequest, short: bool) -> Option<Run> {
            let starting = self.starting.as_ref().is_some_and(|(id, _)| *id == r.id);
            if r.status == Status::InProgress || starting {
                let cancelling = self.cancelling.as_ref().is_some_and(|(id, _)| *id == r.id);
                let cancel = ui.add_enabled(!starting && !cancelling, egui::Button::new(if cancelling { "Cancelling…" } else { "Cancel" })).on_hover_text("Stops Claude Code: the request waits again, its log kept");
                mark(&format!("uiRequestCancel_{}", r.id), &cancel);
                return cancel.clicked().then(|| Run::Cancel(r.id.clone()));
            }
            let busy = self.busy();
            let label = match (r.status, short) {
                (Status::Failed, false) => "Start again",
                (Status::Done, false) => "Make it again",
                _ => "Start",
            };
            let start = ui
                .add_enabled(busy.is_none(), egui::Button::new(label))
                .on_hover_text("Claude Code models it in Blender, unattended, following tools/blender/requests.md; the console shows everything it does")
                .on_disabled_hover_text(format!("One at a time: {} is being made", busy.unwrap_or_default()));
            mark(&format!("uiRequestStart_{}", r.id), &start);
            start.clicked().then(|| Run::Start(r.id.clone()))
        }

        fn list_column(&mut self, ui: &mut egui::Ui) -> Option<Run> {
            let mut asked = None;
            let list = self.list.clone();
            if list.is_empty() && self.problems.is_empty() {
                hint(ui, "No requests yet.");
            }
            for r in &list {
                let chosen = self.selected.as_deref() == Some(r.id.as_str());
                let fill = if chosen { Color32::from_rgb(38, 52, 76) } else { Color32::from_gray(28) };
                // the row's click, under the buttons drawn on it (its size from the frame before)
                let row_id = ui.id().with(("request-row", &r.id));
                let row = ui.ctx().data(|d| d.get_temp::<egui::Rect>(row_id)).map(|rect| ui.interact(rect, row_id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand));
                let starting = self.starting.as_ref().is_some_and(|(id, _)| *id == r.id);
                let frame = egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        chip(ui, r.status);
                        ui.label(RichText::new(&r.name).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(a) = self.run_button(ui, r, true) {
                                asked = Some(a);
                            }
                        });
                    });
                    hint(ui, &about(r));
                    if starting {
                        hint(ui, "starting Claude Code…");
                    } else if r.status == Status::InProgress {
                        bar(ui, r, ui.available_width(), 6.0);
                        hint(ui, &r.progress.at(now_secs()).1);
                    } else if r.status == Status::Failed {
                        ui.label(RichText::new(&r.progress.step).size(11.5).color(FAILED));
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
            let Some(i) = self.selected.as_ref().and_then(|id| self.list.iter().position(|r| &r.id == id)) else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Drop pictures here").size(18.0).color(Color32::from_gray(170)));
                    hint(ui, "Front, side, back, close-ups of one creature or prop: one request.");
                    hint(ui, "Then Start it: Claude Code models it in Blender by itself.");
                });
                return (None, None, None);
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
                if r.kind == Kind::Creature {
                    ui.label("Default pose");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut r.pose, Pose::TPose, "T-pose").on_hover_text("Arms (or front legs) straight out to the sides, legs straight, tail straight back: the easiest to rig and animate");
                        ui.selectable_value(&mut r.pose, Pose::AsIs, "as-is").on_hover_text("The pose the pictures show");
                    });
                    ui.end_row();
                }
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
                    asked = self.run_button(ui, &r, false);
                });
                ui.end_row();
            });
            if r.status == Status::InProgress {
                section(ui, "Being modelled");
                let shown = bar(ui, &r, 330.0, 18.0);
                mark("uiRequestBar", &shown);
            } else if r.status == Status::Failed {
                warn(ui, &r.progress.step);
                if r.progress.step.contains("logged in") {
                    let login = ui.button("Log in to Claude Code…").on_hover_text("Opens Claude Code in a terminal: type /login there, once, then Start again");
                    mark("uiRequestLogin", &login);
                    if login.clicked() {
                        said = Some(open_login(&self.root()).unwrap_or_else(|e| e));
                    }
                }
                hint(ui, "The console has its log (Copy takes it all); Start again makes it once more.");
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
            // the drawings Claude works from, as it makes them: see how well the lines sit
            let drawn: Vec<String> = r.lines.iter().filter(|f| self.picture_path(&r.id, f).is_file()).cloned().collect();
            if !drawn.is_empty() {
                section(ui, "Lines");
                hint(ui, "Claude's lines over the pictures and the front and side it made from them; once rendered, those lines over the model, with how far they overlap. Click one to see it full size.");
                ui.horizontal_wrapped(|ui| {
                    for f in drawn {
                        let path = self.picture_path(&r.id, &f);
                        let shown = self.picture(ui, &path, 130.0, Sense::click()).on_hover_text(format!("{}: click to open it", drawing_about(&f)));
                        if shown.clicked() {
                            if let Err(e) = reveal(&path) {
                                said = Some(e);
                            }
                        }
                    }
                });
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
            (use_prop, said, asked)
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
            Style::Accurate => "accurate",
        };
        format!("{kind} · {} cells · {style}{}", r.height, if r.mine { " · mine" } else { "" })
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

    fn chip(ui: &mut egui::Ui, status: Status) {
        let (text, color) = match status {
            Status::Waiting => ("WAITING", WAITING),
            Status::InProgress => ("IN PROGRESS", WORKING),
            Status::Failed => ("FAILED", FAILED),
            Status::Done => ("DONE", DONE),
        };
        ui.label(RichText::new(text).size(9.5).strong().color(Color32::BLACK).background_color(color));
    }

    /// A run asked for from the window.
    pub(in super::super) enum Run {
        Start(String),
        Cancel(String),
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
            if let Some(s) = self.desk.watch(now) {
                self.said = Some(s);
            }
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
                let (mut said, mut asked) = (None, None);
                // resizable every way (its corner too); the console in a panel at its foot (open,
                // its top edge drags), the list and the request in what's left
                egui::Window::new(format!("Model requests ({})", d.list.len()))
                    .id(egui::Id::new("model-requests"))
                    .open(&mut open)
                    .default_size([840.0, 760.0])
                    .min_size([620.0, 420.0])
                    .resizable(true)
                    .default_pos([270.0, 60.0])
                    .show(ctx, |ui| {
                        d.banner(ui, hovering);
                        ui.add_space(4.0);
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
                match asked {
                    Some(Run::Start(id)) => said = Some(d.start_run(&id)),
                    Some(Run::Cancel(id)) => said = Some(d.cancel_run(&id)),
                    None => {}
                }
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
            lines: vec!["lines_front.png".into(), "profiles.png".into()],
            ..Default::default()
        };
        let text = content::to_text(&r);
        assert!(text.contains("  \"kind\": \"creature\",\n") && text.contains("  \"style\": \"low_poly\",\n") && text.contains("  \"status\": \"in_progress\",\n"), "{text}");
        assert!(text.contains("  \"progress\": {\n    \"step\": \"building the model\",\n    \"done\": 0.2,\n    \"started\": 1760000000.0,\n    \"minutes\": 25.0\n  },\n"), "{text}");
        assert!(text.contains("  \"images\": [\"front.png\", \"side.jpg\"],\n"), "short lists on one line: {text}");
        let at = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
        assert!(at("images") < at("lines") && at("lines") < at("previews"), "the drawings between the pictures and the renders, as model_requests.py has them: {text}");
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
        toad.progress.step = "failed: Claude Code isn't logged in: run \"claude\" in a terminal and type /login, once".into();
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
