//! What differs between the browser and the desktop: the clock, loading files, and the
//! test hooks (in the browser, `<body data-*>` attributes a headless check reads and writes).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Milliseconds on a steady clock (the page's performance.now() in the browser).
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// The time of day where the player is, in hours (0 to 24, minutes and seconds as the
/// fraction): the game's time is the real one.
#[cfg(target_arch = "wasm32")]
pub fn local_hours() -> f32 {
    let d = js_sys::Date::new_0();
    d.get_hours() as f32 + d.get_minutes() as f32 / 60.0 + d.get_seconds() as f32 / 3600.0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn local_hours() -> f32 {
    // GAME_UTC_OFFSET (hours) puts it on UTC plus that, for a machine whose clock says otherwise
    if let Some(offset) = std::env::var("GAME_UTC_OFFSET").ok().and_then(|v| v.parse::<f64>().ok()) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        return ((secs / 3600.0 + offset).rem_euclid(24.0)) as f32;
    }
    os_local_hours()
}

/// The system's local time (its time zone and daylight saving), from Windows.
#[cfg(windows)]
fn os_local_hours() -> f32 {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut SystemTime);
    }
    let mut t = SystemTime::default();
    // SAFETY: GetLocalTime fills the struct it's given, laid out as Windows' SYSTEMTIME
    unsafe { GetLocalTime(&mut t) };
    t.hour as f32 + t.minute as f32 / 60.0 + t.second as f32 / 3600.0
}

/// The system's local time, from the C library (its time zone and daylight saving).
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn os_local_hours() -> f32 {
    // SAFETY: localtime_r fills the tm it's given from the time it's given; both are ours
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return 12.0;
        }
        tm.tm_hour as f32 + tm.tm_min as f32 / 60.0 + tm.tm_sec as f32 / 3600.0
    }
}

/// The game's window is the one in use: on the desktop, focused; in the browser, the page
/// shown and focused (whichever of its parts has the focus).
#[cfg(not(target_arch = "wasm32"))]
pub fn window_active(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.focused)
}

#[cfg(target_arch = "wasm32")]
pub fn window_active(_ctx: &egui::Context) -> bool {
    web_sys::window().and_then(|w| w.document()).is_some_and(|d| !d.hidden() && d.has_focus().unwrap_or(true))
}

/// What each of the browser's controller slots holds now (`navigator.getGamepads()`: each
/// controller's id, None for an empty slot); nothing on the desktop, where gilrs names the
/// controllers itself.
#[cfg(not(target_arch = "wasm32"))]
pub fn gamepad_ids() -> Vec<Option<String>> {
    vec![]
}

#[cfg(target_arch = "wasm32")]
pub fn gamepad_ids() -> Vec<Option<String>> {
    use wasm_bindgen::JsCast;
    let Some(list) = web_sys::window().and_then(|w| w.navigator().get_gamepads().ok()) else {
        return vec![];
    };
    // unchecked, as gilrs reads them (the checks' stand-in controllers are plain objects)
    list.iter().map(|g| (!g.is_null() && !g.is_undefined()).then(|| g.unchecked_into::<web_sys::Gamepad>().id())).collect()
}

/// A "HH:MM" of hours (0 to 24).
pub fn clock_text(hours: f32) -> String {
    let minutes = (hours.rem_euclid(24.0) * 60.0).floor() as u32;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// Hours from "HH:MM" (or "HH", or the periods' names: morning 8:00, day 12:00, night 22:00).
pub fn parse_clock(text: &str) -> Option<f32> {
    match text.trim() {
        "morning" => return Some(8.0),
        "day" => return Some(12.0),
        "night" => return Some(22.0),
        _ => {}
    }
    let (h, m) = text.trim().split_once(':').unwrap_or((text.trim(), "0"));
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then(|| h as f32 + m as f32 / 60.0)
}

enum Slot {
    Pending,
    Ready(Arc<Vec<u8>>),
    Failed(String),
}

/// Files under assets/: read from disk on the desktop, fetched in the browser.
#[derive(Clone)]
pub struct Files {
    base: String,
    slots: Arc<Mutex<HashMap<String, Slot>>>,
}

pub enum Load {
    Pending,
    Ready(Arc<Vec<u8>>),
    Failed(String),
}

impl Files {
    pub fn new(base: &str) -> Files {
        Files { base: base.trim_end_matches('/').to_string(), slots: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Asks for a file (once); `get` tells when it's there.
    pub fn request(&self, path: &str) {
        let mut slots = self.slots.lock().unwrap();
        if slots.contains_key(path) {
            return;
        }
        slots.insert(path.to_string(), Slot::Pending);
        drop(slots);
        self.fetch(path);
    }

    pub fn get(&self, path: &str) -> Load {
        self.request(path);
        match self.slots.lock().unwrap().get(path) {
            Some(Slot::Ready(b)) => Load::Ready(b.clone()),
            Some(Slot::Failed(e)) => Load::Failed(e.clone()),
            _ => Load::Pending,
        }
    }

    /// Puts a file in by hand (one dropped on the page): `get` hands it out from now on.
    pub fn insert(&self, path: &str, bytes: Vec<u8>) {
        self.done(path, Ok(bytes));
    }

    /// A file here already (read, or put in by hand), without asking for it.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn peek(&self, path: &str) -> Option<Arc<Vec<u8>>> {
        match self.slots.lock().unwrap().get(path) {
            Some(Slot::Ready(b)) => Some(b.clone()),
            _ => None,
        }
    }

    /// Forgets a file, so the next `get` reads it again (it changed on disk).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn forget(&self, path: &str) {
        self.slots.lock().unwrap().remove(path);
    }

    fn done(&self, path: &str, result: Result<Vec<u8>, String>) {
        let slot = match result {
            Ok(b) => Slot::Ready(Arc::new(b)),
            Err(e) => Slot::Failed(e),
        };
        self.slots.lock().unwrap().insert(path.to_string(), slot);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fetch(&self, path: &str) {
        let full = format!("{}/{}", self.base, path);
        self.done(path, std::fs::read(&full).map_err(|e| format!("{full}: {e}")));
    }

    #[cfg(target_arch = "wasm32")]
    fn fetch(&self, path: &str) {
        let url = format!("{}/{}", self.base, path);
        let me = self.clone();
        let key = path.to_string();
        ehttp::fetch(ehttp::Request::get(&url), move |r| {
            let result = match r {
                Ok(resp) if resp.ok => Ok(resp.bytes),
                Ok(resp) => Err(format!("{url}: HTTP {}", resp.status)),
                Err(e) => Err(format!("{url}: {e}")),
            };
            me.done(&key, result);
        });
    }
}

/// Files dropped on the window, read: (file name, its bytes or why not). On the desktop at
/// once, in the browser when the page has read them.
pub type Dropped = Arc<Mutex<Vec<(String, Result<Vec<u8>, String>)>>>;

pub fn read_dropped(files: Vec<egui::DroppedFileHandle>, out: &Dropped) {
    for f in files {
        let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        out.lock().unwrap().push((name, f.bytes()));
        #[cfg(target_arch = "wasm32")]
        {
            let out = out.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let bytes = f.bytes_async().await;
                out.lock().unwrap().push((name, bytes));
            });
        }
    }
}

/// Test hooks: in the browser, `<body data-KEY="value">`; elsewhere, nothing (but in the Rust
/// tests, which keep them: `hook`). Written only when they change: dozens are set every frame,
/// and each write is a trip into the page.
#[cfg(target_arch = "wasm32")]
pub fn set_hook(key: &str, value: &str) {
    use std::cell::RefCell;
    thread_local! {
        static LAST: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
        static BODY: Option<web_sys::HtmlElement> = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body());
    }
    let changed = LAST.with(|last| {
        let mut last = last.borrow_mut();
        match last.get_mut(key) {
            Some(v) if v == value => false,
            Some(v) => {
                value.clone_into(v);
                true
            }
            None => {
                last.insert(key.to_string(), value.to_string());
                true
            }
        }
    });
    if changed {
        BODY.with(|body| {
            if let Some(body) = body {
                let _ = body.dataset().set(key, value);
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_hook(key: &str, value: &str) {
    // the tests find a widget by its mark (where it was drawn): `hook`
    #[cfg(test)]
    HOOKS.with(|hooks| hooks.borrow_mut().insert(key.to_string(), value.to_string()));
    #[cfg(not(test))]
    let _ = (key, value);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
thread_local! {
    static HOOKS: std::cell::RefCell<HashMap<String, String>> = std::cell::RefCell::new(HashMap::new());
}

/// A hook as this thread last set it (the tests: a marked widget's "x,y").
#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn hook(key: &str) -> Option<String> {
    HOOKS.with(|hooks| hooks.borrow().get(key).cloned())
}

/// This thread's hooks forgotten (the tests: before a frame, so a widget no longer drawn has
/// no mark).
#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn clear_hooks() {
    HOOKS.with(|hooks| hooks.borrow_mut().clear());
}

/// Saves a binary file through the browser (a download).
#[cfg(target_arch = "wasm32")]
pub fn download_bytes(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(err)?.dyn_into().map_err(|_| "no anchor".to_string())?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// A small text kept between runs (a save): the browser's localStorage, or a file under
/// build/save/ on the desktop.
#[cfg(target_arch = "wasm32")]
pub fn store_text(name: &str, text: &str) -> Result<(), String> {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("no storage")?;
    storage.set_item(name, text).map_err(|e| format!("{e:?}"))
}

#[cfg(target_arch = "wasm32")]
pub fn stored_text(name: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(name).ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn forget_text(name: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.remove_item(name);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn store_path(name: &str) -> std::path::PathBuf {
    let root = std::path::PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
    root.join("build").join("save").join(format!("{name}.json"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn store_text(name: &str, text: &str) -> Result<(), String> {
    let path = store_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stored_text(name: &str) -> Option<String> {
    std::fs::read_to_string(store_path(name)).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn forget_text(name: &str) {
    let _ = std::fs::remove_file(store_path(name));
}

/// Follows a link the system hands to a program (battlefrontier: ones go to the desktop
/// game, which opens Blender).
#[cfg(target_arch = "wasm32")]
pub fn open_link(url: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(|e| format!("{e:?}"))?.dyn_into().map_err(|_| "no anchor".to_string())?;
    a.set_href(url);
    a.click();
    Ok(())
}

/// The main menu's Exit in the browser: the tab closes when the browser lets a page close
/// it (one opened on its own, an installed app's window); else it stays, and the menu says so.
#[cfg(target_arch = "wasm32")]
pub fn close_window() {
    if let Some(w) = web_sys::window() {
        let _ = w.close();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn close_window() {}

/// A command left by a test in `<body data-cmd>` (taken once).
#[cfg(target_arch = "wasm32")]
pub fn take_command() -> Option<String> {
    let body = web_sys::window()?.document()?.body()?;
    let data = body.dataset();
    let cmd = data.get("cmd")?;
    data.delete("cmd");
    Some(cmd)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn take_command() -> Option<String> {
    None
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod clock_tests {
    use super::*;

    #[test]
    fn the_clock_reads_and_says_hours() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(13.5), "13:30");
        assert_eq!(clock_text(23.0 + 59.0 / 60.0), "23:59");
        assert_eq!(parse_clock("18:30"), Some(18.5));
        assert_eq!(parse_clock("7"), Some(7.0));
        assert_eq!((parse_clock("morning"), parse_clock("day"), parse_clock("night")), (Some(8.0), Some(12.0), Some(22.0)));
        assert_eq!(parse_clock("24:00"), None);
        assert_eq!(parse_clock("clock"), None, "anything else: the real time");
        let now = local_hours();
        assert!((0.0..24.0).contains(&now), "{now}");
    }
}
