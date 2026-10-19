//! The Builds panel (the top bar's "Builds"): the game built into a folder from the editor, no
//! terminal needed, as the README's commands build it: the desktop program (`cargo build -p
//! engine-game`: the program, content/ beside it, temporary/ too when asked) or the site
//! (`trunk build`: its files). Its settings are kept. A build shows how far cargo is and what
//! its programs say, and can be cancelled at any time: all its programs stopped (the compilers
//! cargo started too), what it made in the folder taken away and what it put aside there put
//! back, the folder as it was. Only its last step, the copying, touches the folder (a
//! transaction, `Undo`); compiling happens in the project, in a cache of its own that stays
//! (the next build only compiles what changed). The browser can't run builds: its panel says
//! so.

use super::dock::{self, Pane, PaneState};

/// The panel.
#[derive(Default)]
pub(super) struct Builds {
    pub open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    desk: desk::Desk,
}

#[cfg(target_arch = "wasm32")]
impl Builds {
    /// The panel (`st`: as the layout has it): a note.
    pub fn window(&mut self, ui: &mut egui::Ui, st: &mut PaneState) -> Option<String> {
        crate::platform::set_hook("build", if self.open { "note" } else { "" });
        if self.open {
            st.open = true;
            dock::show(ui, st, Pane::Builds, "Builds", |ui| {
                ui.label("Build the game into a folder: the desktop program, or the site.");
                ui.add_space(4.0);
                super::warn(ui, "Builds need the desktop app: they run cargo and Trunk on your computer, which the browser can't.");
                super::hint(ui, "cargo run -p engine-game --release -- --maps opens the map editor there.");
            });
            self.open = st.open;
        }
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Builds {
    /// The panel (`st`: as the layout has it), when it's open; what the status line says as a
    /// build starts or ends (open or not).
    pub fn window(&mut self, ui: &mut egui::Ui, st: &mut PaneState) -> Option<String> {
        let mut said = self.desk.watch();
        crate::platform::set_hook("build", &self.desk.hook());
        if self.open {
            st.open = true;
            let d = &mut self.desk;
            dock::show(ui, st, Pane::Builds, "Builds", |ui| {
                if let Some(s) = d.panel(ui) {
                    said = Some(s);
                }
            });
            self.open = st.open;
        }
        if self.desk.running() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        said
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod desk {
    use std::collections::VecDeque;
    use std::io::Read;
    use std::path::{Component, Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    use egui::RichText;
    use serde::{Deserialize, Serialize};

    use super::super::{hint, mark, section, warn};
    use crate::platform;

    /// The name the settings are kept under.
    const KEPT: &str = "editor_build";
    /// What a build leaves in its folder: what it made there (the next build into it replaces
    /// those, and only those).
    pub(super) const MANIFEST: &str = "battle-frontier-build.json";
    /// Inside the project, builds go only under here.
    pub(super) const LOCAL: &str = "build/local";
    /// Where the site is put together before it's copied (Trunk empties its folder first).
    const SITE: &str = "build/builds/site";
    /// How many of the programs' lines are kept, and how many shown.
    const LOG_LINES: usize = 2000;
    const SHOWN_LINES: usize = 400;
    /// What a step says when the build was cancelled.
    pub(super) const CANCELLED: &str = "cancelled";
    /// Windows: a program started with no console window (the release editor has none).
    #[cfg(windows)]
    const NO_WINDOW: u32 = 0x0800_0000;

    // ------------------------------------------------------------ the settings

    /// What a build makes.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub(super) enum Target {
        /// the desktop program, content/ beside it
        #[default]
        Desktop,
        /// the site: Trunk's files (index.html, the WebAssembly, the content's bundle)
        Web,
    }

    /// How it's compiled.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub(super) enum Profile {
        /// optimised, as the game ships: slower to build
        #[default]
        Release,
        /// quick to build, slow to play (the desktop program with a console beside it)
        Debug,
    }

    impl Profile {
        /// cargo's name for it: its folder in the cache.
        fn dir(self) -> &'static str {
            match self {
                Profile::Release => "release",
                Profile::Debug => "debug",
            }
        }
    }

    /// A build's settings, kept between runs.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    pub(super) struct Settings {
        /// the folder it goes into (needed)
        pub folder: String,
        pub target: Target,
        pub profile: Profile,
        /// the desktop program's placeholders too (temporary/): for one's own play only
        pub placeholders: bool,
        /// where the site is served from ("./": any address)
        pub public_url: String,
        /// how many compile at once (0: as .cargo/config.toml says)
        pub jobs: u32,
        /// where cargo keeps what it compiled (in the project, or a whole path): its own, so
        /// the editor's own program (locked while it runs) is never in its way
        pub cache: String,
    }

    impl Default for Settings {
        fn default() -> Self {
            Settings {
                folder: String::new(),
                target: Target::Desktop,
                profile: Profile::Release,
                placeholders: false,
                public_url: "./".into(),
                jobs: 0,
                cache: "build/target-builds".into(),
            }
        }
    }

    // ------------------------------------------------------------ where a build may go

    /// A path without Windows' `\\?\` (canonicalize adds it; people and programs don't write it).
    fn plain(p: PathBuf) -> PathBuf {
        let s = p.to_string_lossy();
        match s.strip_prefix(r"\\?\") {
            Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
            _ => p,
        }
    }

    /// A path whole: "." and ".." gone, the part of it that exists as the disk has it (links
    /// followed, its letters' case), the rest as written.
    pub(super) fn whole(p: &Path) -> PathBuf {
        let mut parts: Vec<Component> = vec![];
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    if matches!(parts.last(), Some(Component::Normal(_))) {
                        parts.pop();
                    }
                }
                c => parts.push(c),
            }
        }
        let written: PathBuf = parts.iter().collect();
        let (mut base, mut rest) = (written.clone(), vec![]);
        loop {
            if let Ok(found) = std::fs::canonicalize(&base) {
                let mut out = plain(found);
                for r in rest.iter().rev() {
                    out.push(r);
                }
                return out;
            }
            match (base.file_name().map(|n| n.to_os_string()), base.parent().map(Path::to_path_buf)) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    base = parent;
                }
                _ => return written,
            }
        }
    }

    /// Two parts of paths the same (on Windows, whatever their letters' case).
    fn same_part(a: &Component, b: &Component) -> bool {
        if cfg!(windows) {
            a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        } else {
            a == b
        }
    }

    /// `p` is `dir` or inside it (both whole).
    pub(super) fn inside(p: &Path, dir: &Path) -> bool {
        let (a, b): (Vec<Component>, Vec<Component>) = (p.components().collect(), dir.components().collect());
        a.len() >= b.len() && a.iter().zip(&b).all(|(x, y)| same_part(x, y))
    }

    /// Two names of a folder's the same one (on Windows, whatever their letters' case).
    fn same_name(a: &str, b: &str) -> bool {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    }

    /// A folder to show as an example.
    fn example() -> &'static str {
        if cfg!(windows) {
            r"C:\Games\Battle Frontier"
        } else {
            "/home/me/games/battle-frontier"
        }
    }

    /// Why a build can't go into `folder` (None: it can): a whole path, never the project nor
    /// a folder holding it, inside the project only under build/local/, never the compile
    /// cache (`cache`, whole) nor where the site is put together.
    pub(super) fn refused(folder: &str, root: &Path, cache: &Path) -> Option<String> {
        let folder = folder.trim();
        if folder.is_empty() {
            return Some("Pick the folder it goes into".into());
        }
        let p = Path::new(folder);
        if !p.is_absolute() {
            return Some(format!("A whole path, from the drive ({})", example()));
        }
        let p = whole(p);
        if inside(root, &p) {
            return Some("That folder holds the project: pick one of its own".into());
        }
        if inside(&p, root) && !inside(&p, &under(root, LOCAL)) {
            return Some(format!("Inside the project, builds go only under {LOCAL}/ ({LOCAL}/desktop, say)"));
        }
        if inside(&p, cache) || inside(cache, &p) {
            return Some("That's the compile cache's folder (or holds it)".into());
        }
        if inside(&p, &under(root, "build/builds")) {
            return Some("That's where the site is put together".into());
        }
        if p.is_file() {
            return Some("That's a file, not a folder".into());
        }
        None
    }

    /// The compile cache's folder, whole (relative to the project, or a whole path).
    pub(super) fn cache_of(cache: &str, root: &Path) -> PathBuf {
        let c = Path::new(cache.trim());
        whole(&if c.is_absolute() { c.to_path_buf() } else { root.join(c) })
    }

    /// Why the compile cache can't be `cache` (whole): named, never the project nor a folder
    /// holding it, inside the project only in a folder of its own under build/ (none of the
    /// editor's own, nor where builds go).
    pub(super) fn cache_refused(written: &str, cache: &Path, root: &Path) -> Option<String> {
        if written.trim().is_empty() {
            return Some("Name the compile cache's folder (build/target-builds)".into());
        }
        if inside(root, cache) {
            return Some("The compile cache can't hold the project".into());
        }
        let build = root.join("build");
        if inside(cache, root) && (!inside(cache, &build) || inside(&build, cache)) {
            return Some("Inside the project, the compile cache goes in a folder under build/ (build/target-builds)".into());
        }
        for own in [LOCAL, "build/builds", "build/save"] {
            let d = under(root, own);
            if inside(cache, &d) || inside(&d, cache) {
                return Some(format!("The compile cache can't be in {own}/"));
            }
        }
        None
    }

    /// A folder of the project's by its path there ("build/local"), each part joined (no
    /// slashes of the wrong kind on Windows).
    pub(super) fn under(root: &Path, rel: &str) -> PathBuf {
        rel.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
    }

    /// The project's folder, whole: GAME_ROOT, else where the editor was started.
    fn project() -> PathBuf {
        whole(&std::env::current_dir().unwrap_or_default().join(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into())))
    }

    /// A build ready to go: its settings, every path whole.
    #[derive(Clone, Debug)]
    pub(super) struct Plan {
        pub settings: Settings,
        pub root: PathBuf,
        pub folder: PathBuf,
        pub cache: PathBuf,
        /// what its asides are named with: when it started
        pub stamp: String,
    }

    impl Plan {
        /// The settings made a build, or why they can't be.
        pub(super) fn of(settings: &Settings, root: &Path) -> Result<Plan, String> {
            let cache = cache_of(&settings.cache, root);
            if let Some(why) = refused(&settings.folder, root, &cache).or_else(|| cache_refused(&settings.cache, &cache, root)) {
                return Err(why);
            }
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0).to_string();
            Ok(Plan { settings: settings.clone(), root: root.to_path_buf(), folder: whole(Path::new(settings.folder.trim())), cache, stamp })
        }
    }

    // ------------------------------------------------------------ how it stands

    /// How a build stands.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub(super) enum Stage {
        #[default]
        Starting,
        /// cargo compiling: how many of how many (0 of 0 until it says)
        Compiling(u32, u32),
        /// Trunk packing the site, once it's compiled
        Packing,
        /// into the folder: how many files of how many
        Copying(usize, usize),
        /// stopped: what it made being taken away
        Cancelling,
        Done,
        Failed(String),
        Cancelled,
    }

    impl Stage {
        /// Over: done, failed or cancelled.
        pub(super) fn over(&self) -> bool {
            matches!(self, Stage::Done | Stage::Failed(_) | Stage::Cancelled)
        }

        /// How far along, 0 to 1: compiling the most of it (as cargo counts), packing and
        /// copying the rest.
        fn fraction(&self) -> f32 {
            let part = |n: f32, all: f32| if all > 0.0 { n / all } else { 0.0 };
            match self {
                Stage::Starting => 0.0,
                Stage::Compiling(n, all) => 0.86 * part(*n as f32, *all as f32),
                Stage::Packing => 0.88,
                Stage::Copying(k, all) => 0.9 + 0.1 * part(*k as f32, *all as f32),
                _ => 1.0,
            }
        }

        /// What it says.
        fn text(&self) -> String {
            match self {
                Stage::Starting => "Starting…".into(),
                Stage::Compiling(_, 0) => "Compiling…".into(),
                Stage::Compiling(n, all) => format!("Compiling: {n} of {all}"),
                Stage::Packing => "Packing the site…".into(),
                Stage::Copying(k, all) => format!("Into the folder: {k} of {all} files"),
                Stage::Cancelling => "Cancelling: taking away what it made…".into(),
                Stage::Done => "Built".into(),
                Stage::Failed(why) => format!("Failed: {why}"),
                Stage::Cancelled => "Cancelled: the folder is as it was".into(),
            }
        }

        /// Its word for the checks.
        fn key(&self) -> &'static str {
            match self {
                Stage::Starting => "starting",
                Stage::Compiling(..) => "compiling",
                Stage::Packing => "packing",
                Stage::Copying(..) => "copying",
                Stage::Cancelling => "cancelling",
                Stage::Done => "done",
                Stage::Failed(_) => "failed",
                Stage::Cancelled => "cancelled",
            }
        }
    }

    /// What a build's thread shares with the panel.
    #[derive(Default)]
    struct Shared {
        stage: Stage,
        /// what its programs said, line by line (the last LOG_LINES)
        log: VecDeque<String>,
        /// what couldn't be taken away or put back, or the asides that couldn't go
        left: Option<String>,
    }

    /// A lock, even on what a panicked thread held (what it shares stays readable).
    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The stage moved on, unless it's being cancelled (then only to its end).
    fn advance(shared: &Mutex<Shared>, stage: Stage) {
        let mut s = lock(shared);
        if s.stage != Stage::Cancelling || stage.over() {
            s.stage = stage;
        }
    }

    /// A line into the log (its last LOG_LINES).
    fn say(shared: &Mutex<Shared>, line: String) {
        let mut s = lock(shared);
        s.log.push_back(line);
        while s.log.len() > LOG_LINES {
            s.log.pop_front();
        }
    }

    // ------------------------------------------------------------ the programs' lines

    /// cargo's progress line ("    Building [=====>   ] 45/312: syn, serde"): how many of how many.
    pub(super) fn progress(line: &str) -> Option<(u32, u32)> {
        let rest = line.trim_start().strip_prefix("Building [")?;
        let counts = rest[rest.find(']')? + 1..].trim_start().split(':').next()?;
        let (n, all) = counts.split_once('/')?;
        Some((n.trim().parse().ok()?, all.trim().parse().ok()?))
    }

    /// A line without its terminal colours (ESC [ ..., to its letter).
    pub(super) fn uncoloured(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\u{1b}' {
                out.push(c);
                continue;
            }
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() || d == '~' {
                        break;
                    }
                }
            }
        }
        out
    }

    /// A line of a program's into the build: cargo's progress moves the stage on (the site's:
    /// once compiled, Trunk packs it), the rest goes into the log.
    fn heard(shared: &Mutex<Shared>, raw: &str, site: bool) {
        let line = uncoloured(raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            return;
        }
        let compiling = matches!(lock(shared).stage, Stage::Starting | Stage::Compiling(..));
        if let Some((n, all)) = progress(line) {
            if compiling {
                advance(shared, Stage::Compiling(n, all));
            }
            return;
        }
        if site && compiling && line.trim_start().starts_with("Finished") {
            advance(shared, Stage::Packing);
        }
        say(shared, line.to_string());
    }

    /// A program's output read to its end, line by line (cargo redraws its progress with
    /// carriage returns: those end lines too).
    fn read_lines(mut from: impl Read, mut each: impl FnMut(&str)) {
        let (mut buf, mut line) = ([0u8; 4096], Vec::<u8>::new());
        loop {
            match from.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        if b == b'\n' || b == b'\r' {
                            if !line.is_empty() {
                                each(&String::from_utf8_lossy(&line));
                                line.clear();
                            }
                        } else {
                            line.push(b);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        if !line.is_empty() {
            each(&String::from_utf8_lossy(&line));
        }
    }

    // ------------------------------------------------------------ the programs

    /// A program: `var` names it, else the PATH has it, else cargo's own folder (where rustup
    /// and `cargo install` put theirs).
    fn find(name: &str, var: &str) -> Option<PathBuf> {
        if let Some(p) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(p);
        }
        let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
        let path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
        if let Some(p) = path.iter().map(|d| d.join(&exe)).find(|p| p.is_file()) {
            return Some(p);
        }
        let home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".cargo")))?;
        Some(home.join("bin").join(exe)).filter(|p| p.is_file())
    }

    /// A program started with no console window of its own (the release editor has none), and
    /// off Windows in a process group of its own (stopped all together).
    fn quiet(c: &mut Command) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(NO_WINDOW);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            c.process_group(0);
        }
    }

    /// The WebAssembly target installed (`rustup target add wasm32-unknown-unknown`): the
    /// toolchain's own folder has it.
    fn wasm_ready(root: &Path) -> Result<(), String> {
        let rustc = find("rustc", "RUSTC").ok_or("rustc not found: install Rust (rustup.rs)")?;
        let mut c = Command::new(&rustc);
        c.args(["--print", "sysroot"]).current_dir(root).stdin(Stdio::null()).stderr(Stdio::null());
        quiet(&mut c);
        let out = c.output().map_err(|e| format!("{}: {e}", rustc.display()))?;
        let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if sysroot.join("lib").join("rustlib").join("wasm32-unknown-unknown").is_dir() {
            Ok(())
        } else {
            Err("the WebAssembly target isn't installed: rustup target add wasm32-unknown-unknown".into())
        }
    }

    /// Windows: a job object holding a build's programs, stopped all at once, and when it's
    /// closed (the editor gone too).
    #[cfg(windows)]
    mod job {
        use std::ffi::c_void;
        use std::os::windows::io::AsRawHandle;

        /// JOBOBJECT_BASIC_LIMIT_INFORMATION
        #[repr(C)]
        #[derive(Default)]
        struct BasicLimits {
            per_process_user_time: i64,
            per_job_user_time: i64,
            flags: u32,
            min_working_set: usize,
            max_working_set: usize,
            active_processes: u32,
            affinity: usize,
            priority_class: u32,
            scheduling_class: u32,
        }

        /// JOBOBJECT_EXTENDED_LIMIT_INFORMATION
        #[repr(C)]
        #[derive(Default)]
        struct Limits {
            basic: BasicLimits,
            io: [u64; 6],
            process_memory: usize,
            job_memory: usize,
            peak_process_memory: usize,
            peak_job_memory: usize,
        }

        /// JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, and the class that sets it
        /// (JobObjectExtendedLimitInformation)
        const KILL_ON_CLOSE: u32 = 0x2000;
        const EXTENDED_LIMITS: i32 = 9;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
            fn SetInformationJobObject(job: *mut c_void, class: i32, info: *mut c_void, length: u32) -> i32;
            fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
            fn TerminateJobObject(job: *mut c_void, code: u32) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }

        pub(super) struct Job(*mut c_void);

        // SAFETY: a job object's handle may be used from any thread
        unsafe impl Send for Job {}

        impl Job {
            /// One holding `child` and all it starts, or None when Windows won't have one.
            pub(super) fn holding(child: &std::process::Child) -> Option<Job> {
                let mut limits = Limits { basic: BasicLimits { flags: KILL_ON_CLOSE, ..Default::default() }, ..Default::default() };
                // SAFETY: the job is made here (and closed again if it can't hold the child);
                // the limits are laid out as Windows' struct, ours for the call; the child's
                // handle is std's, open while `child` is
                unsafe {
                    let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                    if job.is_null() {
                        return None;
                    }
                    SetInformationJobObject(job, EXTENDED_LIMITS, std::ptr::from_mut(&mut limits).cast(), std::mem::size_of::<Limits>() as u32);
                    if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                        CloseHandle(job);
                        return None;
                    }
                    Some(Job(job))
                }
            }

            /// Every program in it stopped.
            pub(super) fn stop(&self) {
                // SAFETY: the job's own handle, open until it's dropped
                unsafe { TerminateJobObject(self.0, 1) };
            }
        }

        impl Drop for Job {
            fn drop(&mut self) {
                // SAFETY: closed once; whatever is still in it stops with it
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    /// What stops a build's programs, all of them (the compilers cargo started too).
    struct Stopper {
        pid: u32,
        #[cfg(windows)]
        job: Option<job::Job>,
    }

    impl Stopper {
        fn of(child: &Child) -> Stopper {
            Stopper {
                pid: child.id(),
                #[cfg(windows)]
                job: job::Job::holding(child),
            }
        }

        fn stop(&self) {
            #[cfg(windows)]
            {
                match &self.job {
                    Some(j) => j.stop(),
                    // the tree from it, by its number (never by a program's name)
                    None => {
                        let mut c = Command::new("taskkill");
                        c.args(["/PID", &self.pid.to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
                        quiet(&mut c);
                        let _ = c.status();
                    }
                }
            }
            #[cfg(unix)]
            {
                // SAFETY: a signal to the build's own process group, made for it as it started
                unsafe {
                    libc::kill(-(self.pid as libc::pid_t), libc::SIGKILL);
                }
            }
        }
    }

    /// What a build's steps share: its plan, the panel's view of it, its cancel and what stops
    /// its programs.
    struct Steps<'a> {
        plan: &'a Plan,
        shared: &'a Arc<Mutex<Shared>>,
        cancel: &'a AtomicBool,
        stopper: &'a Mutex<Option<Stopper>>,
    }

    impl Steps<'_> {
        fn cancelled(&self) -> bool {
            self.cancel.load(Ordering::SeqCst)
        }

        /// A program of the build's run in the project to its end, its lines into the log:
        /// Err when it fails, or when the build is cancelled.
        fn run(&self, program: &Path, args: &[String]) -> Result<(), String> {
            let plan = self.plan;
            let name = program.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut c = Command::new(program);
            c.args(args)
                .current_dir(&plan.root)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env("CARGO_TARGET_DIR", &plan.cache)
                .env("CARGO_TERM_PROGRESS_WHEN", "always")
                .env("CARGO_TERM_PROGRESS_WIDTH", "100")
                .env("CARGO_TERM_COLOR", "never")
                // (Trunk reads it as its --no-color, which takes only true or false)
                .env("NO_COLOR", "true");
            if plan.settings.jobs > 0 {
                c.env("CARGO_BUILD_JOBS", plan.settings.jobs.to_string());
            }
            quiet(&mut c);
            say(self.shared, format!("> {name} {}", args.join(" ")));
            let mut child = {
                // started, and what stops it in place, before a cancel looks for it
                let mut stopper = lock(self.stopper);
                if self.cancelled() {
                    return Err(CANCELLED.into());
                }
                let child = c.spawn().map_err(|e| format!("{}: {e}", program.display()))?;
                *stopper = Some(Stopper::of(&child));
                child
            };
            let (tx, rx) = mpsc::channel();
            let site = plan.settings.target == Target::Web;
            let streams: [Option<Box<dyn Read + Send>>; 2] = [child.stdout.take().map(|s| Box::new(s) as _), child.stderr.take().map(|s| Box::new(s) as _)];
            for stream in streams.into_iter().flatten() {
                let (shared, tx) = (self.shared.clone(), tx.clone());
                std::thread::spawn(move || {
                    read_lines(stream, |line| heard(&shared, line, site));
                    let _ = tx.send(());
                });
            }
            drop(tx);
            let status = child.wait();
            // its last lines (a program it started may hold its output open: not waited on long)
            for _ in 0..2 {
                let _ = rx.recv_timeout(Duration::from_secs(2));
            }
            // (the job closed: anything of its still running stops)
            *lock(self.stopper) = None;
            if self.cancelled() {
                return Err(CANCELLED.into());
            }
            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("{name} stopped ({s}): its lines say why")),
                Err(e) => Err(format!("{name}: {e}")),
            }
        }

        /// Compiled: the desktop program by cargo, the site by Trunk (into SITE).
        fn compile(&self) -> Result<(), String> {
            let s = &self.plan.settings;
            let release = s.profile == Profile::Release;
            match s.target {
                Target::Desktop => {
                    let cargo = find("cargo", "CARGO").ok_or("cargo not found: install Rust (rustup.rs), or set CARGO to it")?;
                    let mut args: Vec<String> = ["build", "-p", "engine-game"].map(String::from).into();
                    if release {
                        args.push("--release".into());
                    }
                    self.run(&cargo, &args)
                }
                Target::Web => {
                    let trunk = find("trunk", "TRUNK").ok_or("Trunk not found: cargo install trunk --locked (trunkrs.dev), or set TRUNK to it")?;
                    wasm_ready(&self.plan.root)?;
                    let mut args = vec!["build".to_string()];
                    if release {
                        args.push("--release".into());
                    }
                    let url = if s.public_url.trim().is_empty() { "./" } else { s.public_url.trim() };
                    args.extend(["--dist".into(), under(&self.plan.root, SITE).display().to_string(), "--public-url".into(), url.to_string()]);
                    self.run(&trunk, &args)
                }
            }
        }

        /// What goes into the folder: each entry's name there and where it comes from.
        fn outputs(&self) -> Result<Vec<(String, PathBuf)>, String> {
            let (plan, mut out) = (self.plan, vec![]);
            match plan.settings.target {
                Target::Desktop => {
                    let exe = if cfg!(windows) { "game.exe" } else { "game" };
                    let made = plan.cache.join(plan.settings.profile.dir()).join(exe);
                    if !made.is_file() {
                        return Err(format!("{} wasn't made", made.display()));
                    }
                    out.push((exe.to_string(), made));
                    out.push(("content".to_string(), plan.root.join("content")));
                    if plan.settings.placeholders && plan.root.join("temporary").is_dir() {
                        out.push(("temporary".to_string(), plan.root.join("temporary")));
                    }
                    for l in ["LICENSE-MIT", "LICENSE-APACHE"] {
                        if plan.root.join(l).is_file() {
                            out.push((l.to_string(), plan.root.join(l)));
                        }
                    }
                }
                Target::Web => {
                    let site = under(&plan.root, SITE);
                    let list = std::fs::read_dir(&site).map_err(|e| format!("{}: {e}", site.display()))?;
                    out = list.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect();
                    out.sort();
                    if out.is_empty() {
                        return Err("Trunk made nothing".into());
                    }
                }
            }
            Ok(out)
        }

        /// The whole build: compiled, then copied into the folder (taken back if it's cancelled
        /// or fails there). The names it made there.
        fn build(&self) -> Result<Vec<String>, String> {
            self.compile()?;
            if self.cancelled() {
                return Err(CANCELLED.into());
            }
            let entries = self.outputs()?;
            let shown = |k: usize, all: usize| advance(self.shared, Stage::Copying(k, all));
            let s = &self.plan.settings;
            let about = serde_json::json!({
                "platform": if s.target == Target::Desktop { "desktop" } else { "web" },
                "profile": s.profile.dir(),
                "placeholders": s.target == Target::Desktop && s.placeholders,
            });
            let mut copier = Copier {
                skip: if s.target == Target::Desktop { no_bundle } else { nothing_skipped },
                shown: Some(&shown),
                about,
                ..Copier::new(&self.plan.folder, &self.plan.stamp, self.cancel)
            };
            let mut undo = Undo::default();
            let made = copier.put_in(&entries, &mut undo);
            match made {
                Ok(made) if !self.cancelled() => {
                    if let Err(e) = undo.keep() {
                        lock(self.shared).left = Some(e);
                    }
                    Ok(made)
                }
                other => {
                    if self.cancelled() {
                        advance(self.shared, Stage::Cancelling);
                    }
                    if let Err(e) = undo.undo() {
                        lock(self.shared).left = Some(e);
                    }
                    Err(other.err().unwrap_or_else(|| CANCELLED.into()))
                }
            }
        }
    }

    /// A build from its start to its end, in its own thread.
    fn work(plan: &Plan, shared: &Arc<Mutex<Shared>>, cancel: &AtomicBool, stopper: &Mutex<Option<Stopper>>) {
        let steps = Steps { plan, shared, cancel, stopper };
        let outcome = steps.build();
        if plan.settings.target == Target::Web {
            let _ = std::fs::remove_dir_all(under(&plan.root, SITE));
        }
        let end = match outcome {
            Ok(made) => {
                say(shared, format!("Into {}: {}", plan.folder.display(), made.join(", ")));
                Stage::Done
            }
            Err(_) if cancel.load(Ordering::SeqCst) => Stage::Cancelled,
            Err(why) => Stage::Failed(why),
        };
        advance(shared, end);
    }

    // ------------------------------------------------------------ into the folder

    /// A file, or a folder and all in it, gone (gone already is fine; a file kept a moment by
    /// another program, tried again).
    fn remove(p: &Path) -> std::io::Result<()> {
        let mut tries = 0;
        loop {
            let r = match std::fs::symlink_metadata(p) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => Err(e),
                Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
                Ok(_) => std::fs::remove_file(p),
            };
            tries += 1;
            if r.is_ok() || tries >= 4 {
                return r;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    /// A file copied made writable (Windows' read-only is an attribute; elsewhere its folder
    /// says whether it can go): taking it away never trips on it.
    #[cfg(windows)]
    #[allow(clippy::permissions_set_readonly_false)]
    fn writable(p: &Path) {
        if let Ok(m) = std::fs::metadata(p) {
            let mut perms = m.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(p, perms);
            }
        }
    }

    #[cfg(not(windows))]
    fn writable(_: &Path) {}

    /// There, whatever it is.
    fn exists(p: &Path) -> bool {
        std::fs::symlink_metadata(p).is_ok()
    }

    /// A name a manifest may list: an entry of its folder's (no path, no "..", not an aside).
    fn plain_name(n: &str) -> bool {
        !n.is_empty() && n != "." && n != ".." && !n.contains(['/', '\\', ':']) && !n.starts_with(".build-aside-")
    }

    /// What a build before put in the folder, as its manifest says (nothing: no build went there).
    fn earlier_build(folder: &Path) -> Vec<String> {
        let Some(v) = std::fs::read_to_string(folder.join(MANIFEST)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return vec![] };
        v["made"].as_array().map(|a| a.iter().filter_map(|n| n.as_str()).filter(|n| plain_name(n)).map(String::from).collect()).unwrap_or_default()
    }

    /// What's never copied: nothing.
    fn nothing_skipped(_: &str, _: &Path) -> bool {
        false
    }

    /// The desktop program's: the browser's bundle of the content (its records one by one are
    /// what it reads).
    fn no_bundle(entry: &str, inside: &Path) -> bool {
        entry == "content" && inside == Path::new("bundle.json")
    }

    /// What a build did to its folder, to take back: the folders it made to hold it (outermost
    /// first), what it put aside there (renamed: where it was, where it went), what it made.
    #[derive(Debug, Default)]
    pub(super) struct Undo {
        dirs: Vec<PathBuf>,
        aside: Vec<(PathBuf, PathBuf)>,
        made: Vec<PathBuf>,
    }

    impl Undo {
        /// The folder as it was: what was made taken away, what was put aside put back, the
        /// folders made for it gone. What couldn't be, said.
        pub(super) fn undo(&mut self) -> Result<(), String> {
            let mut wrong = vec![];
            for p in self.made.drain(..).rev() {
                if let Err(e) = remove(&p) {
                    wrong.push(format!("{}: {e}", p.display()));
                }
            }
            for (was, aside) in self.aside.drain(..).rev() {
                if let Err(e) = std::fs::rename(&aside, &was) {
                    wrong.push(format!("{} (put aside as {}): {e}", was.display(), aside.display()));
                }
            }
            for d in self.dirs.drain(..).rev() {
                // only when it's empty again: nothing of anyone else's goes
                let _ = std::fs::remove_dir(&d);
            }
            if wrong.is_empty() {
                Ok(())
            } else {
                Err(wrong.join("; "))
            }
        }

        /// It's all in: what was put aside goes for good (what can't, said).
        pub(super) fn keep(&mut self) -> Result<(), String> {
            let wrong: Vec<String> = self.aside.drain(..).filter_map(|(_, aside)| remove(&aside).err().map(|e| format!("{}: {e}", aside.display()))).collect();
            self.made.clear();
            self.dirs.clear();
            if wrong.is_empty() {
                Ok(())
            } else {
                Err(wrong.join("; "))
            }
        }
    }

    /// A build's entries going into its folder, file by file, counted, the cancel looked at
    /// between them.
    pub(super) struct Copier<'a> {
        pub folder: &'a Path,
        pub stamp: &'a str,
        pub cancel: &'a AtomicBool,
        /// what's never copied: (an entry's name, a path inside it)
        pub skip: fn(&str, &Path) -> bool,
        /// how many files of how many are in, as they go
        pub shown: Option<&'a dyn Fn(usize, usize)>,
        /// what the manifest says of the build (its platform, its profile)
        pub about: serde_json::Value,
        /// how many files are in, of how many
        pub(super) done: usize,
        pub(super) total: usize,
    }

    impl<'a> Copier<'a> {
        pub(super) fn new(folder: &'a Path, stamp: &'a str, cancel: &'a AtomicBool) -> Copier<'a> {
            Copier { folder, stamp, cancel, skip: nothing_skipped, shown: None, about: serde_json::json!({}), done: 0, total: 0 }
        }

        /// The entries into the folder, as a transaction `undo` takes back (or `keep`s): the
        /// folders missing made; what a build before put there (its manifest says) put aside,
        /// and refused when an entry's name is something of the folder's own; each file
        /// copied; then the manifest. The names it made.
        pub(super) fn put_in(&mut self, entries: &[(String, PathBuf)], undo: &mut Undo) -> Result<Vec<String>, String> {
            let folder = self.folder;
            let mut missing = vec![];
            let mut at = folder.to_path_buf();
            while !exists(&at) {
                missing.push(at.clone());
                match at.parent() {
                    Some(p) if p != at.as_path() => at = p.to_path_buf(),
                    _ => break,
                }
            }
            for d in missing.iter().rev() {
                std::fs::create_dir(d).map_err(|e| format!("{}: {e}", d.display()))?;
                undo.dirs.push(d.clone());
            }
            if !folder.is_dir() {
                return Err(format!("{} isn't a folder", folder.display()));
            }
            let earlier = earlier_build(folder);
            for (name, _) in entries {
                if exists(&folder.join(name)) && !earlier.iter().any(|e| same_name(e, name)) {
                    return Err(format!("the folder has a {name} of its own: pick an empty folder, or one a build went into"));
                }
            }
            // the build before: all it made put aside (its manifest too)
            let mut aside: Vec<String> = vec![];
            for n in earlier.into_iter().chain([MANIFEST.to_string()]) {
                if !aside.iter().any(|a| same_name(a, &n)) {
                    aside.push(n);
                }
            }
            for (k, name) in aside.iter().enumerate() {
                let was = folder.join(name);
                if !exists(&was) {
                    continue;
                }
                let to = folder.join(format!(".build-aside-{}-{k}-{name}", self.stamp));
                std::fs::rename(&was, &to).map_err(|e| format!("{} couldn't be put aside ({e}): is it open?", was.display()))?;
                undo.aside.push((was, to));
            }
            self.total = entries.iter().map(|(n, from)| self.count(n, from, Path::new(""))).sum();
            let mut made = vec![];
            for (name, from) in entries {
                let to = folder.join(name);
                // (before it's there: a part of one goes too)
                undo.made.push(to.clone());
                self.copy(name, from, &to, Path::new(""))?;
                made.push(name.clone());
            }
            if self.cancel.load(Ordering::SeqCst) {
                return Err(CANCELLED.into());
            }
            made.push(MANIFEST.to_string());
            let mut manifest = serde_json::json!({ "made": &made });
            if let (Some(m), Some(about)) = (manifest.as_object_mut(), self.about.as_object()) {
                m.extend(about.clone());
            }
            let path = folder.join(MANIFEST);
            undo.made.push(path.clone());
            std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(made)
        }

        /// How many files an entry holds (but what's skipped).
        fn count(&self, name: &str, from: &Path, rel: &Path) -> usize {
            if !from.is_dir() {
                return 1;
            }
            std::fs::read_dir(from)
                .map(|list| {
                    list.flatten()
                        .map(|e| {
                            let r = rel.join(e.file_name());
                            if (self.skip)(name, &r) {
                                0
                            } else {
                                self.count(name, &e.path(), &r)
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }

        /// An entry copied, all in it (`rel`: where in its entry it is).
        fn copy(&mut self, name: &str, from: &Path, to: &Path, rel: &Path) -> Result<(), String> {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(CANCELLED.into());
            }
            let meta = std::fs::metadata(from).map_err(|e| format!("{}: {e}", from.display()))?;
            if meta.is_dir() {
                std::fs::create_dir(to).map_err(|e| format!("{}: {e}", to.display()))?;
                let mut list: Vec<_> = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten().map(|e| e.file_name()).collect();
                list.sort();
                for n in list {
                    let r = rel.join(&n);
                    if (self.skip)(name, &r) {
                        continue;
                    }
                    self.copy(name, &from.join(&n), &to.join(&n), &r)?;
                }
                return Ok(());
            }
            std::fs::copy(from, to).map_err(|e| format!("{}: {e}", to.display()))?;
            writable(to);
            self.done += 1;
            if let Some(shown) = self.shown {
                shown(self.done, self.total);
            }
            Ok(())
        }
    }

    // ------------------------------------------------------------ a build going on

    /// A build going on (or over): its thread, what it shares, how to stop it.
    struct Run {
        shared: Arc<Mutex<Shared>>,
        cancel: Arc<AtomicBool>,
        stopper: Arc<Mutex<Option<Stopper>>>,
        thread: Option<std::thread::JoinHandle<()>>,
        started: Instant,
        /// how long it took, once it's over
        took: Option<Duration>,
        plan: Plan,
    }

    impl Run {
        fn start(plan: Plan) -> Run {
            let shared = Arc::new(Mutex::new(Shared::default()));
            let cancel = Arc::new(AtomicBool::new(false));
            let stopper = Arc::new(Mutex::new(None));
            let (s, c, k, p) = (shared.clone(), cancel.clone(), stopper.clone(), plan.clone());
            let thread = std::thread::Builder::new().name("build".into()).spawn(move || work(&p, &s, &c, &k)).ok();
            if thread.is_none() {
                lock(&shared).stage = Stage::Failed("its thread couldn't start".into());
            }
            Run { shared, cancel, stopper, thread, started: Instant::now(), took: None, plan }
        }

        fn stage(&self) -> Stage {
            lock(&self.shared).stage.clone()
        }

        /// Stopped: its programs at once (its thread then takes away what it made).
        fn cancel(&self) {
            self.cancel.store(true, Ordering::SeqCst);
            if let Some(s) = lock(&self.stopper).as_ref() {
                s.stop();
            }
            advance(&self.shared, Stage::Cancelling);
        }

        /// How long it's taken (or took).
        fn time(&self) -> Duration {
            self.took.unwrap_or_else(|| self.started.elapsed())
        }
    }

    /// "42s", "3m 05s".
    fn clock(d: Duration) -> String {
        let s = d.as_secs();
        if s < 60 {
            format!("{s}s")
        } else {
            format!("{}m {:02}s", s / 60, s % 60)
        }
    }

    /// A folder in the file browser (the nearest one there is).
    fn reveal(path: &Path) -> Result<(), String> {
        let mut at = path.to_path_buf();
        while !at.is_dir() {
            match at.parent() {
                Some(p) => at = p.to_path_buf(),
                None => break,
            }
        }
        let program = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(program).arg(&at).spawn().map(|_| ()).map_err(|e| format!("{program} {}: {e}", at.display()))
    }

    /// Windows' folder picker, over everything, from `start`: the folder picked (None when
    /// it's closed without one), sent as it closes.
    #[cfg(windows)]
    fn choose_folder(start: &str) -> mpsc::Receiver<Option<String>> {
        const SCRIPT: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; \
            $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = 'The folder the build goes into'; $f.ShowNewFolderButton = $true; \
            if ($env:BUILD_FOLDER -and (Test-Path -LiteralPath $env:BUILD_FOLDER)) { $f.SelectedPath = $env:BUILD_FOLDER }; \
            $top = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true }; \
            if ($f.ShowDialog($top) -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($f.SelectedPath) }";
        let (tx, rx) = mpsc::channel();
        let start = start.to_string();
        std::thread::spawn(move || {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT]).env("BUILD_FOLDER", start).stdin(Stdio::null()).stderr(Stdio::null());
            quiet(&mut c);
            let picked = c.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|p| !p.is_empty());
            let _ = tx.send(picked);
        });
        rx
    }

    // ------------------------------------------------------------ the panel

    /// The panel's own: the settings, the build going on (or the last one).
    #[derive(Default)]
    pub(super) struct Desk {
        loaded: bool,
        pub(super) settings: Settings,
        /// the settings as last kept
        kept: String,
        /// the project (whole)
        root: Option<PathBuf>,
        run: Option<Run>,
        /// its end said in the status line
        told: bool,
        /// the folder picker's answer, while it's open
        choosing: Option<mpsc::Receiver<Option<String>>>,
    }

    impl Drop for Desk {
        /// The editor going mid-build: the build cancelled and its folder taken back, waited for.
        fn drop(&mut self) {
            if let Some(run) = self.run.as_mut() {
                if !run.stage().over() {
                    run.cancel();
                }
                if let Some(t) = run.thread.take() {
                    let _ = t.join();
                }
            }
        }
    }

    impl Desk {
        fn load(&mut self) {
            if self.loaded {
                return;
            }
            self.loaded = true;
            if let Some(s) = platform::stored_text(KEPT).and_then(|t| serde_json::from_str::<Settings>(&t).ok()) {
                self.settings = s;
            }
            self.kept = serde_json::to_string(&self.settings).unwrap_or_default();
        }

        /// The settings kept as they change.
        fn keep(&mut self) {
            let text = serde_json::to_string(&self.settings).unwrap_or_default();
            if text != self.kept {
                if let Err(e) = platform::store_text(KEPT, &text) {
                    log::warn!("the build's settings weren't kept: {e}");
                }
                self.kept = text;
            }
        }

        fn root(&mut self) -> PathBuf {
            self.root.get_or_insert_with(project).clone()
        }

        pub(super) fn running(&self) -> bool {
            self.run.as_ref().is_some_and(|r| !r.stage().over())
        }

        /// How it stands, for the checks: "" (no build yet), "compiling 45/312", "done"...
        pub(super) fn hook(&self) -> String {
            match self.run.as_ref().map(Run::stage) {
                None => String::new(),
                Some(Stage::Compiling(n, all)) => format!("compiling {n}/{all}"),
                Some(Stage::Copying(k, all)) => format!("copying {k}/{all}"),
                Some(s) => s.key().to_string(),
            }
        }

        /// A build that's just ended: what the status line says of it (once).
        pub(super) fn watch(&mut self) -> Option<String> {
            let run = self.run.as_mut()?;
            let stage = run.stage();
            if !stage.over() || self.told {
                return None;
            }
            self.told = true;
            run.took.get_or_insert(run.started.elapsed());
            if let Some(t) = run.thread.take() {
                let _ = t.join();
            }
            let left = lock(&run.shared).left.clone().map(|l| format!(" (left behind: {l})")).unwrap_or_default();
            Some(match stage {
                Stage::Done => format!("Built into {} in {}{left}", run.plan.folder.display(), clock(run.time())),
                Stage::Failed(why) => format!("The build failed: {why}{left}"),
                _ => format!("Build cancelled: the folder is as it was{left}"),
            })
        }

        /// Why Start can't go (None: it can).
        fn reason(&mut self) -> Option<String> {
            let root = self.root();
            Plan::of(&self.settings, &root).err()
        }

        /// The panel: the settings, Start, the build going on, its lines. What the status line
        /// says, when something happened.
        pub(super) fn panel(&mut self, ui: &mut egui::Ui) -> Option<String> {
            self.load();
            let root = self.root();
            let mut said = None;
            if let Some(rx) = &self.choosing {
                match rx.try_recv() {
                    Ok(picked) => {
                        if let Some(p) = picked {
                            self.settings.folder = p;
                        }
                        self.choosing = None;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => self.choosing = None,
                    Err(mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(Duration::from_millis(200)),
                }
            }
            let running = self.running();
            hint(ui, "The game built into a folder, as the terminal would: the desktop program, or the site.");
            ui.add_space(4.0);
            ui.add_enabled_ui(!running, |ui| {
                if let Some(s) = self.settings_ui(ui, &root) {
                    said = Some(s);
                }
            });
            ui.add_space(8.0);
            if !running {
                let reason = self.reason();
                ui.horizontal(|ui| {
                    let start = ui.add_enabled(reason.is_none(), egui::Button::new(RichText::new("Start build").strong())).on_hover_text("Compiles it in the project (its own cache), then copies it into the folder");
                    mark("uiBuildStart", &start);
                    if start.clicked() {
                        said = Some(self.start());
                    }
                    if let Some(why) = &reason {
                        warn(ui, why);
                    }
                });
            }
            if let Some(s) = self.progress_ui(ui) {
                said = Some(s);
            }
            self.keep();
            said
        }

        /// The settings: what's needed, then what's not.
        fn settings_ui(&mut self, ui: &mut egui::Ui, root: &Path) -> Option<String> {
            let mut said = None;
            let choosing = self.choosing.is_some();
            let mut choose = false;
            let s = &mut self.settings;
            egui::Grid::new("build-settings").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Platform");
                ui.horizontal(|ui| {
                    let d = ui.selectable_value(&mut s.target, Target::Desktop, "Desktop").on_hover_text("This computer's program, content/ beside it: double-click it to play");
                    mark("uiBuildDesktop", &d);
                    let w = ui.selectable_value(&mut s.target, Target::Web, "Web").on_hover_text("The site, as Trunk builds it: the folder can go on any web server");
                    mark("uiBuildWeb", &w);
                });
                ui.end_row();
                ui.label("Build");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.profile, Profile::Release, "Release").on_hover_text("Optimised, as the game ships: slower to build");
                    ui.selectable_value(&mut s.profile, Profile::Debug, "Debug").on_hover_text("Quick to build, slow to play");
                });
                ui.end_row();
                ui.label("Folder");
                ui.horizontal(|ui| {
                    let f = ui.add(egui::TextEdit::singleline(&mut s.folder).desired_width(250.0).hint_text(example())).on_hover_text("Where it goes: made if it isn't there; a build already there is replaced, nothing else");
                    mark("uiBuildFolder", &f);
                    if cfg!(windows) {
                        let c = ui.add_enabled(!choosing, egui::Button::new("Choose…")).on_hover_text("Windows' folder picker");
                        mark("uiBuildChoose", &c);
                        choose = c.clicked();
                    }
                });
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    let kind = if s.target == Target::Desktop { "desktop" } else { "web" };
                    let local = ui.small_button(format!("{LOCAL}/{kind}")).on_hover_text("A folder of the project's build/ (never committed)");
                    mark("uiBuildLocal", &local);
                    if local.clicked() {
                        s.folder = under(root, LOCAL).join(kind).display().to_string();
                    }
                    if ui.small_button("Open").on_hover_text("The folder in the file browser (the nearest one there is)").clicked() && !s.folder.trim().is_empty() {
                        if let Err(e) = reveal(Path::new(s.folder.trim())) {
                            said = Some(e);
                        }
                    }
                });
                ui.end_row();
            });
            section(ui, "Optional");
            egui::Grid::new("build-optional").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                match s.target {
                    Target::Desktop => {
                        ui.label("Placeholders");
                        let p = ui.checkbox(&mut s.placeholders, "temporary/ too").on_hover_text("The imported placeholders, for your own play");
                        mark("uiBuildPlaceholders", &p);
                    }
                    Target::Web => {
                        ui.label("Address");
                        ui.add(egui::TextEdit::singleline(&mut s.public_url).desired_width(160.0))
                            .on_hover_text("Where the site is served from (Trunk's --public-url): ./ works at any address");
                    }
                }
                ui.end_row();
                ui.label("Compilers");
                ui.add(egui::DragValue::new(&mut s.jobs).range(0..=64).custom_formatter(|v, _| if v == 0.0 { "as the project says".into() } else { format!("{v} at once") }))
                    .on_hover_text("How many compile at once (0: as .cargo/config.toml says): fewer if the computer runs out of memory");
                ui.end_row();
                ui.label("Compile cache");
                ui.add(egui::TextEdit::singleline(&mut s.cache).desired_width(200.0)).on_hover_text(
                    "Where cargo keeps what it compiled, in the project: a folder of its own, so the editor's own program never gets in its way. The first build compiles everything (minutes), the next only what changed",
                );
                ui.end_row();
            });
            if s.target == Target::Desktop && s.placeholders {
                warn(ui, "With the placeholders: for your own play only, never share this build.");
            }
            if choose {
                #[cfg(windows)]
                {
                    self.choosing = Some(choose_folder(self.settings.folder.trim()));
                }
            }
            said
        }

        /// The build going on (its progress, Cancel) or the last one (how it went), its lines.
        fn progress_ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
            let run = self.run.as_ref()?;
            let stage = run.stage();
            let mut said = None;
            ui.add_space(6.0);
            if !stage.over() {
                ui.add(egui::ProgressBar::new(stage.fraction()).text(stage.text()).animate(true));
                let mut cancel = false;
                ui.horizontal(|ui| {
                    hint(ui, &format!("{} so far", clock(run.time())));
                    let c = ui.add_enabled(stage != Stage::Cancelling, egui::Button::new("Cancel")).on_hover_text("Stops it at once: what it made in the folder goes, what was there comes back");
                    mark("uiBuildCancel", &c);
                    cancel = c.clicked();
                });
                hint(ui, "Leaving the editor cancels it too.");
                if cancel {
                    run.cancel();
                    said = Some("Cancelling the build: what it made goes".into());
                }
            } else {
                match &stage {
                    Stage::Done => {
                        ui.horizontal(|ui| {
                            ui.label(format!("Built in {}: {}", clock(run.time()), run.plan.folder.display()));
                            let open = ui.small_button("Open the folder");
                            mark("uiBuildOpen", &open);
                            if open.clicked() {
                                if let Err(e) = reveal(&run.plan.folder) {
                                    said = Some(e);
                                }
                            }
                        });
                    }
                    Stage::Failed(why) => warn(ui, &format!("The build failed: {why}")),
                    _ => hint(ui, "Cancelled: the folder is as it was."),
                }
                if let Some(left) = &lock(&run.shared).left {
                    warn(ui, &format!("Left behind: {left}"));
                }
            }
            let shared = lock(&run.shared);
            let n = shared.log.len();
            egui::CollapsingHeader::new(format!("Its lines ({n})")).id_salt("build-lines").default_open(matches!(stage, Stage::Failed(_))).show(ui, |ui| {
                let text: Vec<&str> = shared.log.iter().skip(n.saturating_sub(SHOWN_LINES)).map(String::as_str).collect();
                egui::ScrollArea::vertical().id_salt("build-lines-scroll").max_height(240.0).auto_shrink([false, true]).stick_to_bottom(true).show(ui, |ui| {
                    ui.label(RichText::new(text.join("\n")).monospace().size(11.0));
                });
            });
            said
        }

        /// Started, if it can be: what the status line says.
        fn start(&mut self) -> String {
            let root = self.root();
            match Plan::of(&self.settings, &root) {
                Ok(plan) => {
                    let to = plan.folder.display().to_string();
                    self.run = Some(Run::start(plan));
                    self.told = false;
                    format!("Building into {to}…")
                }
                Err(why) => why,
            }
        }

        /// Ready for the tests to draw: the project `root`, the settings as they are (never
        /// written to the save).
        #[cfg(test)]
        pub(super) fn ready(&mut self, root: &Path) {
            self.root = Some(root.to_path_buf());
            self.loaded = true;
            self.kept = serde_json::to_string(&self.settings).unwrap_or_default();
        }

        /// A build shown as going on (or over) for a picture of the panel: no programs.
        #[cfg(test)]
        pub(super) fn showing(&mut self, root: &Path, stage: Stage, lines: &[&str]) {
            self.ready(root);
            let plan = Plan::of(&self.settings, root).unwrap();
            let shared = Arc::new(Mutex::new(Shared { stage, log: lines.iter().map(|l| l.to_string()).collect(), left: None }));
            let started = Instant::now().checked_sub(Duration::from_secs(95)).unwrap_or_else(Instant::now);
            self.run = Some(Run { shared, cancel: Arc::new(AtomicBool::new(false)), stopper: Arc::new(Mutex::new(None)), thread: None, started, took: None, plan });
            self.told = true;
        }

        /// What the build's programs said (the tests print it when it fails).
        #[cfg(test)]
        pub(super) fn lines(&self) -> Vec<String> {
            self.run.as_ref().map(|r| lock(&r.shared).log.iter().cloned().collect()).unwrap_or_default()
        }

        /// Started from the tests, as Start does, and waited for to the end (or cancelled once
        /// `cancel_at` says so of how it stands): how it ended.
        #[cfg(test)]
        pub(super) fn build_now(&mut self, root: &Path, cancel_at: impl Fn(&Stage) -> bool) -> Stage {
            self.root = Some(root.to_path_buf());
            self.loaded = true;
            let _ = self.start();
            loop {
                let Some(run) = self.run.as_ref() else { return Stage::Failed("not started".into()) };
                let stage = run.stage();
                if stage.over() {
                    return stage;
                }
                if cancel_at(&stage) && !run.cancel.load(Ordering::SeqCst) {
                    run.cancel();
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::desk::*;

    /// A folder of its own under build/tests (on the project's drive).
    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/tests").join(format!("builds-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        whole(&dir)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Everything under a folder: each folder, each file with what's in it.
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) {
            let Ok(list) = std::fs::read_dir(dir) else { return };
            for e in list.flatten() {
                let r = format!("{rel}{}", e.file_name().to_string_lossy());
                if e.path().is_dir() {
                    out.push((format!("{r}/"), vec![]));
                    walk(&e.path(), &format!("{r}/"), out);
                } else {
                    out.push((r, std::fs::read(e.path()).unwrap()));
                }
            }
        }
        let mut out = vec![];
        walk(dir, "", &mut out);
        out.sort();
        out
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        out.sort();
        out
    }

    #[test]
    fn cargo_says_how_far_it_is() {
        assert_eq!(progress("    Building [=======>                 ] 45/312: syn, serde"), Some((45, 312)));
        assert_eq!(progress("    Building [                            ] 0/29: proc-macro2(build.rs), serde(build.rs…"), Some((0, 29)));
        assert_eq!(progress("   Compiling serde v1.0.229"), None);
        assert_eq!(progress("    Finished `release` profile [optimized] target(s) in 1m 02s"), None);
        assert_eq!(uncoloured("\u{1b}[32m INFO\u{1b}[0m done \u{1b}[1;31mwarm\u{1b}[0m"), " INFO done warm");
    }

    #[test]
    fn where_a_build_may_go() {
        let root = scratch("rules").join("project");
        std::fs::create_dir_all(root.join("build/save")).unwrap();
        std::fs::create_dir_all(root.join("content")).unwrap();
        let cache = cache_of("build/target-builds", &root);
        let at = |p: &Path| refused(&p.display().to_string(), &root, &cache);
        assert!(refused("", &root, &cache).is_some() && refused("  ", &root, &cache).is_some(), "a folder is needed");
        assert!(refused("relative/folder", &root, &cache).is_some(), "a whole path");
        assert!(at(&root).is_some(), "the project");
        assert!(at(root.parent().unwrap()).is_some(), "a folder holding it");
        for inside in ["content", "build", "build/save", "crates/game"] {
            assert!(at(&root.join(inside)).is_some(), "{inside}: inside the project, not under build/local");
        }
        assert!(at(&root.join("build/local")).is_none() && at(&root.join("build/local/desktop")).is_none());
        assert!(at(&root.join("build/local/../../content")).is_some(), "the .. followed");
        assert!(at(&root.parent().unwrap().join("elsewhere/game")).is_none(), "outside the project");
        assert!(at(&cache).is_some() && at(&cache.join("release")).is_some(), "the cache");
        assert!(at(&root.join("build/builds/site")).is_some());
        if cfg!(windows) {
            // the letters' case doesn't matter there
            assert!(at(&root.join("BUILD").join("Local").join("x")).is_none());
            assert!(at(&root.join("Content")).is_some());
        }
        // the cache: a folder of its own under build/
        assert!(cache_refused("build/target-builds", &cache, &root).is_none());
        assert!(cache_refused("", &root, &root).is_some());
        for bad in ["build", "content/x", "build/local/x", "build/save", ".", ".."] {
            assert!(cache_refused(bad, &cache_of(bad, &root), &root).is_some(), "{bad}");
        }
        assert!(cache_refused("x", &cache_of(&root.parent().unwrap().join("cache").display().to_string(), &root), &root).is_none(), "outside the project");
        // a plan: its folder whole
        let settings = Settings { folder: root.join("build/local/./desktop").display().to_string(), ..Default::default() };
        let plan = Plan::of(&settings, &root).unwrap();
        assert_eq!(plan.folder, root.join("build").join("local").join("desktop"));
        assert_eq!(plan.cache, root.join("build").join("target-builds"));
        assert!(Plan::of(&Settings { folder: root.display().to_string(), ..Default::default() }, &root).is_err());
    }

    /// A build replaces the one before it in its folder (all of it, and only it: a file of the
    /// user's stays), the way it was until it's all in; taken back, the folder is as it was,
    /// byte for byte.
    #[test]
    fn a_build_replaces_the_one_before_and_nothing_else() {
        let dir = scratch("into");
        let src = dir.join("src");
        write(&src.join("game.exe"), "the new program");
        write(&src.join("content/data/a.json"), "{}");
        write(&src.join("content/bundle.json"), "[]");
        let folder = dir.join("out");
        write(&folder.join("game.exe"), "the old program");
        write(&folder.join("content/old.json"), "old");
        write(&folder.join("old-extra.txt"), "made by the build before");
        write(&folder.join(MANIFEST), r#"{ "made": ["game.exe", "content", "old-extra.txt", "battle-frontier-build.json", "../../escape"] }"#);
        write(&folder.join("notes.txt"), "mine");
        let entries = vec![("game.exe".to_string(), src.join("game.exe")), ("content".to_string(), src.join("content"))];
        let before = snapshot(&folder);
        let cancel = AtomicBool::new(false);
        let mut undo = Undo::default();
        let made = Copier::new(&folder, "1", &cancel).put_in(&entries, &mut undo).unwrap();
        assert_eq!(made, ["game.exe", "content", MANIFEST]);
        assert_eq!(std::fs::read_to_string(folder.join("game.exe")).unwrap(), "the new program");
        assert!(!folder.join("old-extra.txt").exists() && !folder.join("content/old.json").exists(), "what the build before made, put aside");
        assert_eq!(std::fs::read_to_string(folder.join("notes.txt")).unwrap(), "mine");
        undo.undo().unwrap();
        assert_eq!(snapshot(&folder), before, "taken back: as it was");
        // kept: the asides gone; the bundle skipped (the desktop program's way)
        let mut undo = Undo::default();
        let shown = std::cell::Cell::new((0usize, 0usize));
        let show = |k: usize, all: usize| shown.set((k, all));
        let mut copier = Copier { skip: |name, rel| name == "content" && rel == Path::new("bundle.json"), shown: Some(&show), ..Copier::new(&folder, "2", &cancel) };
        copier.put_in(&entries, &mut undo).unwrap();
        undo.keep().unwrap();
        assert_eq!(names(&folder), [MANIFEST, "content", "game.exe", "notes.txt"]);
        assert!(!folder.join("content/bundle.json").exists() && folder.join("content/data/a.json").is_file());
        assert_eq!(shown.get(), (2, 2), "the files counted as they went: the program and a.json");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(manifest["made"], serde_json::json!(["game.exe", "content", MANIFEST]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Never what isn't a build's: a name of the folder's own is refused; a cancel takes back
    /// what was made, even halfway, and the folders made for it.
    #[test]
    fn a_build_never_takes_what_isnt_a_builds() {
        let dir = scratch("own");
        let src = dir.join("src");
        write(&src.join("game.exe"), "program");
        for k in 0..5 {
            write(&src.join(format!("content/{k}.json")), "{}");
        }
        let entries = vec![("game.exe".to_string(), src.join("game.exe")), ("content".to_string(), src.join("content"))];
        let folder = dir.join("theirs");
        write(&folder.join("content/mine.txt"), "the user's");
        let before = snapshot(&folder);
        let cancel = AtomicBool::new(false);
        let mut undo = Undo::default();
        let refused = Copier::new(&folder, "1", &cancel).put_in(&entries, &mut undo).unwrap_err();
        assert!(refused.contains("content"), "{refused}");
        undo.undo().unwrap();
        assert_eq!(snapshot(&folder), before);
        // cancelled halfway: the folders made for it gone too
        let deep = dir.join("new/deeper/out");
        let mut undo = Undo::default();
        let stop = |k: usize, _: usize| {
            if k == 3 {
                cancel.store(true, Ordering::SeqCst);
            }
        };
        let err = Copier { shown: Some(&stop), ..Copier::new(&deep, "2", &cancel) }.put_in(&entries, &mut undo).unwrap_err();
        assert_eq!(err, CANCELLED);
        assert!(deep.join("content").is_dir(), "halfway in");
        undo.undo().unwrap();
        assert!(!dir.join("new").exists(), "the folders it made, gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real build (`cargo test -p engine-game -- --ignored real_build`): the desktop program
    /// into build/local/test, then one cancelled as it compiles, its folder as it was.
    #[test]
    #[ignore]
    fn real_build() {
        let root = whole(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
        let folder = root.join("build/local/test");
        let _ = std::fs::remove_dir_all(&folder);
        let mut desk = Desk::default();
        desk.settings = Settings { folder: folder.display().to_string(), profile: Profile::Debug, ..Default::default() };
        let end = desk.build_now(&root, |_| false);
        assert_eq!(end, Stage::Done, "{}", desk.lines().join("
"));
        let exe = if cfg!(windows) { "game.exe" } else { "game" };
        assert!(folder.join(exe).is_file() && folder.join("content/data").is_dir() && folder.join(MANIFEST).is_file());
        assert!(!folder.join("content/bundle.json").exists() && !folder.join("temporary").exists());
        let built = snapshot(&folder);
        // again, cancelled as it compiles (a clean cache of its own: it has a lot to do)
        let mut again = Desk::default();
        again.settings = Settings { folder: folder.display().to_string(), profile: Profile::Debug, cache: "build/target-builds-cancel".into(), ..Default::default() };
        let end = again.build_now(&root, |s| matches!(s, Stage::Compiling(n, _) if *n >= 3));
        assert_eq!(end, Stage::Cancelled);
        assert_eq!(snapshot(&folder), built, "the build before, as it was");
        let _ = std::fs::remove_dir_all(root.join("build/target-builds-cancel"));
    }

    /// Pictures of the panel (`cargo test -p engine-game -- --ignored shot`): waiting to start,
    /// a build going on, one done: build/shots/builds-*.png.
    #[test]
    #[ignore]
    fn shot_of_the_builds_panel() {
        let root = whole(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let settings = Settings { folder: under(&root, "build/local/desktop").display().to_string(), ..Default::default() };
        let lines = [
            "> cargo build -p engine-game --release",
            "   Compiling engine-core v0.1.0",
            "   Compiling wgpu-core v27.0.3",
            "   Compiling engine-world v0.1.0",
        ];
        for (name, stage) in [("idle", None), ("going", Some(Stage::Compiling(187, 402))), ("done", Some(Stage::Done))] {
            let mut builds = super::Builds { open: true, ..Default::default() };
            builds.desk.settings = settings.clone();
            match stage {
                Some(s) => builds.desk.showing(&root, s, &lines),
                None => builds.desk.ready(&root),
            }
            let mut out = None;
            for _ in 0..4 {
                let mut o = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                    let mut st = super::Pane::Builds.first();
                    builds.window(ui, &mut st);
                });
                shooter.textures(&mut o.textures_delta);
                out = Some(o);
            }
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../build/shots/builds-{name}.png"));
            shooter.shoot(&ctx, out.unwrap().shapes, [1400, 900], &path).unwrap();
            println!("{}", path.display());
        }
    }

    /// The site for real (`cargo test -p engine-game -- --ignored real_site`): Trunk's files
    /// into build/local/test-web, the folder they were put together in gone after.
    #[test]
    #[ignore]
    fn real_site() {
        let root = whole(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
        let folder = root.join("build/local/test-web");
        let _ = std::fs::remove_dir_all(&folder);
        let mut desk = Desk::default();
        desk.settings = Settings { folder: folder.display().to_string(), target: Target::Web, profile: Profile::Debug, ..Default::default() };
        let end = desk.build_now(&root, |_| false);
        assert_eq!(end, Stage::Done, "{}", desk.lines().join("
"));
        let made = names(&folder);
        assert!(made.iter().any(|n| n == "index.html") && made.iter().any(|n| n.ends_with("_bg.wasm")), "{made:?}");
        assert!(folder.join("content/bundle.json").is_file() && folder.join(MANIFEST).is_file());
        assert!(!root.join("build/builds/site").exists(), "put together, then gone");
    }
}
