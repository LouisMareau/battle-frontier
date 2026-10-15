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

/// The local hour, for the time of day.
#[cfg(target_arch = "wasm32")]
pub fn local_hour() -> u32 {
    js_sys::Date::new_0().get_hours()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn local_hour() -> u32 {
    // no time zone database on the desktop build: UTC, shifted by GAME_UTC_OFFSET if set
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let offset: i64 = std::env::var("GAME_UTC_OFFSET").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    (((secs / 3600 + offset) % 24 + 24) % 24) as u32
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

/// Test hooks: in the browser, `<body data-KEY="value">`; elsewhere, nothing. Written only
/// when they change: dozens are set every frame, and each write is a trip into the page.
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
pub fn set_hook(_key: &str, _value: &str) {}

/// Saves a text file through the browser (a download).
#[cfg(target_arch = "wasm32")]
pub fn download(name: &str, text: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(text));
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(err)?.dyn_into().map_err(|_| "no anchor".to_string())?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
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
