//! Publishing: what's been made in the editor sent to the game's repository on GitHub as a
//! pull request, for its maintainers to look at before it's merged (only those who can write
//! to the repository can merge one: nothing gets into the game unreviewed). The top bar's
//! "Sign in" signs in with GitHub, by its device flow (a code typed on github.com, once: the
//! repository's OAuth App, `CLIENT_ID`) or with a token pasted; "Publish" waits for it, then
//! lists every file of content/ changed, each to send or leave out, under a title and a
//! description: on the desktop the project's folder against the commit its git checkout is
//! on (else the repository's main); in the browser what was saved there (records, models
//! dropped, a chunked map's squares) against the site it runs on (its bundle; the commit it
//! was built from, else main). Sent, it's a pull request from the user's fork (made when they
//! can't write to the repository themselves): the files' blobs, a tree on the copy's, a
//! commit signed off as the user, a branch, the pull request. On the desktop the names check
//! (tools/check-names.py) reads the files first, when Python is there (the repository's checks
//! read every pull request anyway). GitHub's calls are the same on both: blocking on a thread
//! of their own on the desktop, the page's fetch in the browser, where signing in by code goes
//! through the site's own address (`functions/github/login/`: github.com's sign-in sends no
//! CORS headers; its API does).

use std::collections::BTreeMap;
use std::future::Future;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use base64::Engine as _;
use egui::{Color32, RichText};
use serde_json::{json, Value};

use super::{hint, mark, warn, MapEditor};
use crate::hud::TEXT;

/// The game's repository on GitHub.
pub(super) const OWNER: &str = "LouisMareau";
pub(super) const REPO: &str = "battle-frontier";
/// The editor's OAuth App on GitHub ("Battle Frontier editor", the repository owner's, its
/// device flow on), whose client id is public; GITHUB_CLIENT_ID overrides it (where the editor
/// runs, or where it's built). A token pasted signs in too.
const CLIENT_ID: &str = "Ov23liQZPiLKCAggEM69";
/// What signing in asks for: forks, branches and pull requests on public repositories.
const SCOPE: &str = "public_repo";
/// What's published: the game's own content (never temporary/, never private/).
const PUBLISHED: &str = "content";

/// Whether a file's the game's own, to publish: content/'s (never temporary/'s, nor the model
/// requests' private/ folders).
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
pub(super) fn published(path: &str) -> bool {
    path.strip_prefix(PUBLISHED).is_some_and(|rest| rest.starts_with('/'))
}
/// The browser: the name the sign-in is kept under (`platform::store_text`).
#[cfg(target_arch = "wasm32")]
const SIGNIN: &str = "github_signin";

/// The OAuth App's client id: GITHUB_CLIENT_ID where the editor runs, else where it was
/// built, else the one written here (empty: none yet).
fn client_id() -> String {
    std::env::var("GITHUB_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("GITHUB_CLIENT_ID").map(str::to_string))
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| CLIENT_ID.to_string())
}

/// How a file has changed since the copy was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum How {
    Added,
    Changed,
    Removed,
}

/// A file of content/ changed since the project's copy was taken.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Change {
    /// its path in the repository ("content/data/maps/meadow.json")
    pub path: String,
    pub how: How,
    /// its size as it's sent (0 when removed)
    pub size: u64,
    /// sent (ticked) or left out
    pub send: bool,
}

/// Text or not, as git tells it: a model, a picture or a sound (.gitattributes says they're
/// binary), or a zero byte in its first 8000 bytes.
#[cfg(not(target_arch = "wasm32"))]
fn binary(path: &str, bytes: &[u8]) -> bool {
    let lower = path.to_lowercase();
    [".glb", ".png", ".jpg", ".jpeg", ".webp", ".wav", ".ogg", ".mp3", ".blend"].iter().any(|e| lower.ends_with(e)) || bytes.iter().take(8000).any(|&b| b == 0)
}

/// The bytes git keeps for a file: text with its line ends LF (what a Windows checkout's CRLF
/// go back to when committed), anything else as it is.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn committed(path: &str, bytes: Vec<u8>) -> Vec<u8> {
    if binary(path, &bytes) || !bytes.contains(&b'\r') {
        return bytes;
    }
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            continue;
        }
        out.push(b);
    }
    out
}

/// A blob's id as git names it: the SHA-1 of "blob <size>\0" and its bytes.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn blob_id(bytes: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.digest().to_string()
}

fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The commit the project's folder is a copy of: its git checkout's HEAD, read from .git (git
/// needn't be installed), when it's one.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn head_commit(root: &Path) -> Option<String> {
    let dot = root.join(".git");
    // a worktree's .git is a file naming its folder in the repository's
    let git = if dot.is_file() {
        let text = std::fs::read_to_string(&dot).ok()?;
        let p = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        if p.is_absolute() {
            p
        } else {
            root.join(p)
        }
    } else {
        dot
    };
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    let Some(name) = head.strip_prefix("ref:").map(str::trim) else {
        return is_sha(head).then(|| head.to_string());
    };
    // its refs, then (a worktree's) the repository's own
    let common = std::fs::read_to_string(git.join("commondir")).ok().map(|c| {
        let p = PathBuf::from(c.trim());
        if p.is_absolute() {
            p
        } else {
            git.join(p)
        }
    });
    for dir in std::iter::once(git.clone()).chain(common) {
        if let Ok(sha) = std::fs::read_to_string(dir.join(name)) {
            if is_sha(sha.trim()) {
                return Some(sha.trim().to_string());
            }
        }
        if let Ok(packed) = std::fs::read_to_string(dir.join("packed-refs")) {
            let found = packed.lines().filter_map(|l| l.split_once(' ')).find(|(sha, r)| r.trim() == name && is_sha(sha));
            if let Some((sha, _)) = found {
                return Some(sha.to_string());
            }
        }
    }
    None
}

/// Every file of content/ git would commit, by its path in the repository (not the web
/// build's bundle, a log or what a system leaves in folders).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn local_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    let mut dirs = vec![root.join(PUBLISHED)];
    while let Some(dir) = dirs.pop() {
        let Ok(list) = std::fs::read_dir(&dir) else { continue };
        for e in list.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "__pycache__" && !name.starts_with('.') {
                    dirs.push(p);
                }
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let skipped = rel == "content/bundle.json" || name.ends_with(".log") || name.starts_with('.') || name.eq_ignore_ascii_case("thumbs.db") || name.eq_ignore_ascii_case("desktop.ini");
            if !skipped {
                out.push((rel, p));
            }
        }
    }
    out.sort();
    out
}

/// What's changed between the copy's files (`tree`: each path's blob id) and the folder's
/// (`local`: each path's id as committed, its own bytes' id, its size): there and not in the
/// copy (added), with other bytes (changed: neither id matches), in the copy and gone
/// (removed). In path order, each ticked.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn changes(tree: &BTreeMap<String, String>, local: &BTreeMap<String, (String, String, u64)>) -> Vec<Change> {
    let mut out = vec![];
    for (path, (as_committed, raw, size)) in local {
        let how = match tree.get(path) {
            None => How::Added,
            Some(id) if id == as_committed || id == raw => continue,
            Some(_) => How::Changed,
        };
        out.push(Change { path: path.clone(), how, size: *size, send: true });
    }
    out.extend(tree.keys().filter(|p| !local.contains_key(*p)).map(|p| Change { path: p.clone(), how: How::Removed, size: 0, send: true }));
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The browser's records against the site's (`was`: its bundle), in the game's own space
/// (never the placeholders'): those it hasn't (added), with other text (changed), those
/// gone (removed, left unticked: deleting is rare and easy to do by mistake), each as the
/// file it's kept in; and the files' text to send.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn record_changes(now: &content::Content, was: &content::Content) -> (Vec<Change>, BTreeMap<String, Vec<u8>>) {
    let (now, was) = (now.space_texts(content::Space::Content), was.space_texts(content::Space::Content));
    let (mut out, mut sent) = (vec![], BTreeMap::new());
    for (file, text) in &now {
        let how = match was.get(file) {
            None => How::Added,
            Some(old) if old == text => continue,
            Some(_) => How::Changed,
        };
        let path = format!("{PUBLISHED}/data/{file}");
        out.push(Change { path: path.clone(), how, size: text.len() as u64, send: true });
        sent.insert(path, text.clone().into_bytes());
    }
    out.extend(was.keys().filter(|f| !now.contains_key(*f)).map(|f| Change { path: format!("{PUBLISHED}/data/{f}"), how: How::Removed, size: 0, send: false }));
    (out, sent)
}

/// The models the browser's props have that the site's don't (dropped on the page): their
/// paths in the repository.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn new_models(now: &content::Content, was: &content::Content) -> Vec<String> {
    let models = |c: &content::Content| -> std::collections::BTreeSet<String> {
        c.props.entries().iter().filter(|e| e.space == content::Space::Content).filter_map(|e| e.value.model.clone()).collect()
    };
    let old = models(was);
    models(now).into_iter().filter(|m| !old.contains(m)).map(|m| format!("{PUBLISHED}/assets/{m}")).collect()
}

/// A file saved in the browser against the site's (`site`: None when it hasn't one): added,
/// changed, or nothing when it's the same.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn file_change(path: &str, now: &[u8], site: Option<&[u8]>) -> Option<Change> {
    let how = match site {
        None => How::Added,
        Some(old) if old == now => return None,
        Some(_) => How::Changed,
    };
    Some(Change { path: path.to_string(), how, size: now.len() as u64, send: true })
}

/// What a change is, for the list's groups.
pub(super) fn group(path: &str) -> &'static str {
    let rest = path.strip_prefix("content/").unwrap_or(path);
    let mut parts = rest.split('/');
    match (parts.next().unwrap_or(""), parts.next().unwrap_or("")) {
        ("data", "maps") => "Maps and their events",
        ("data", "props") => "Props",
        ("data", "creatures") => "Creatures",
        ("data", "abilities" | "moves") => "Abilities and moves",
        ("data", "encounters" | "parties") => "Encounters and parties",
        ("data", "worlds" | "regions") => "Worlds",
        ("data", "settings") => "Settings",
        ("data", "items") => "Items",
        ("data", _) => "Other records",
        ("assets", "models") => "Models",
        ("assets", "sounds") => "Sounds",
        ("assets", _) => "Other assets",
        _ => "Other files",
    }
}

/// A short name for a branch, from a title: "publish/a-bigger-pond-1760000000".
pub(super) fn branch_name(title: &str, secs: u64) -> String {
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    format!("publish/{}{secs}", if slug.is_empty() { String::new() } else { format!("{slug}-") })
}

/// Signed in: who, with what token.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct User {
    pub login: String,
    pub token: String,
}

/// What a publish starts from: the copy's commit and tree (and whether it's the repository's
/// main, the copy's own commit being unknown), the repository's main branch, whether the user
/// can write to it, what's changed and the files to send (their bytes as committed), and who
/// signs the commit off ("Name <email>", GitHub's private address when theirs isn't public).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Survey {
    pub base: String,
    pub base_tree: String,
    pub on_main: bool,
    pub branch: String,
    pub can_push: bool,
    pub changes: Vec<Change>,
    pub contents: BTreeMap<String, Vec<u8>>,
    pub signoff: String,
}

/// Who signs a commit off, from GitHub's account: their name (else login) and their public
/// address (else the private one GitHub gives each account).
pub(super) fn signoff(user: &Value) -> String {
    let text = |k: &str| user.get(k).and_then(Value::as_str).filter(|v| !v.trim().is_empty()).map(str::to_string);
    let login = text("login").unwrap_or_default();
    let email = text("email").unwrap_or_else(|| match user.get("id").and_then(Value::as_u64) {
        Some(id) => format!("{id}+{login}@users.noreply.github.com"),
        None => format!("{login}@users.noreply.github.com"),
    });
    format!("{} <{email}>", text("name").unwrap_or_else(|| login.clone()))
}

// ---------------------------------------------------------------- running GitHub's calls

/// Runs a task to its end on its own: on a thread of its own on the desktop (GitHub's calls
/// block there), on the page's event loop in the browser.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn spawn(task: impl Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || block_on(task));
}

#[cfg(target_arch = "wasm32")]
pub(super) fn spawn(task: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(task);
}

/// A future run to its end on this thread: the desktop's never wait (their calls block).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// A wait: the thread sleeps on the desktop, a timer in the browser.
#[cfg(not(target_arch = "wasm32"))]
async fn pause(ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

#[cfg(target_arch = "wasm32")]
async fn pause(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Seconds since 1970 (a branch's name, a code's time running out).
fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
}

/// A file of the site's (the browser: its bundle, a map's square), by its address there.
#[cfg(target_arch = "wasm32")]
async fn site_file(path: &str) -> Option<Vec<u8>> {
    match ehttp::fetch_async(ehttp::Request::get(path)).await {
        Ok(r) if r.ok => Some(r.bytes),
        _ => None,
    }
}

/// GitHub, as the signed-in user (`token`): its API (`api`) and its site (`web`, where
/// signing in goes: the site's own address in the browser), stand-ins in the tests.
#[derive(Clone, Debug)]
pub(super) struct Hub {
    pub api: String,
    pub web: String,
    pub token: String,
}

/// The device flow's first step: the code the user types on github.com, where, how often
/// to ask whether it's been typed and how long it lasts (seconds).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// Whether the code's been typed: the token, not yet, or not so often; or the asking turned
/// away for now (asked again later, less and less often), with what's said if signing in is
/// given up on.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Poll {
    Token(String),
    Wait,
    SlowDown,
    Busy(String),
}

/// A form's value, percent-encoded.
fn encode(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// GitHub turning a request away for now: too many from where it came (github.com limits the
/// addresses the site's sign-in calls come from, shared with many others), or a hiccup.
/// Asked again a little later.
fn busy(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// How long the first asking again waits (ms; each after waits that much more): short in the
/// tests.
const AGAIN_MS: u32 = if cfg!(test) { 10 } else { 1500 };

/// A second, as signing in by a code waits between askings (ms): short in the tests.
const SECOND_MS: u32 = if cfg!(test) { 10 } else { 1000 };

/// Askings turned away in a row before signing in by a code is given up on: about a minute at
/// GitHub's pace, the waits before them doubling (5, 10, 20, 30 s).
const AWAY_TIMES: u32 = 4;

/// Where a token is made: a classic one, its scope ticked already and its note filled in.
const TOKEN_PAGE: &str = "https://github.com/settings/tokens/new?scopes=public_repo&description=Battle%20Frontier%20editor";

/// Signing in by a code given up on while GitHub turns its calls away (`why`: its last
/// answer), and what to do instead.
fn turned_away(why: &str) -> String {
    if cfg!(target_arch = "wasm32") {
        format!("GitHub keeps turning the site's sign-in calls away ({why}). It limits the shared addresses they go out from, so it isn't you: paste a token above instead (it goes from this browser straight to GitHub), or ask for a new code in a few minutes.")
    } else {
        format!("GitHub keeps turning the sign-in away ({why}): ask for a new code in a few minutes, or paste a token below.")
    }
}

/// What GitHub said no with, in words: its own message, else the status's.
fn failure(status: u16, text: &str, body: &Value, what: &str) -> String {
    let message = body.get("message").and_then(Value::as_str).unwrap_or(text);
    match status {
        401 => "GitHub doesn't take the sign-in any more: sign in again".to_string(),
        _ => format!("GitHub answered {status} to {what}: {message}"),
    }
}

impl Hub {
    /// GitHub's answer to a request, whatever it is: its status (and its words) and its JSON
    /// (Null when it isn't); an error only when there was no answer.
    async fn fetch(&self, req: ehttp::Request) -> Result<(u16, String, Value), String> {
        // (the browser sends its own User-Agent: a page can't set one)
        #[cfg(not(target_arch = "wasm32"))]
        let r = {
            let mut req = req;
            req.headers.insert("User-Agent", "battle-frontier-editor");
            req.timeout = Some(std::time::Duration::from_secs(90));
            ehttp::fetch_blocking(&req)
        };
        #[cfg(target_arch = "wasm32")]
        let r = ehttp::fetch_async(req).await;
        let r = r.map_err(|e| format!("GitHub can't be reached ({e})"))?;
        Ok((r.status, r.status_text, serde_json::from_slice(&r.bytes).unwrap_or(Value::Null)))
    }

    async fn send(&self, req: ehttp::Request, what: &str) -> Result<Value, String> {
        let (status, text, body) = self.fetch(req).await?;
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(failure(status, &text, &body, what))
    }

    /// A call to the API: its answer, or what went wrong (GitHub's own words).
    async fn call(&self, method: ehttp::Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        let what = format!("{method:?} {path}");
        let mut req = ehttp::Request::new(method, format!("{}{path}", self.api), ehttp::Headers::new(&[("Accept", "application/vnd.github+json")]));
        // (the browser asks for no more headers than GitHub lets other sites send)
        #[cfg(not(target_arch = "wasm32"))]
        req.headers.insert("X-GitHub-Api-Version", "2022-11-28");
        if !self.token.is_empty() {
            req.headers.insert("Authorization", format!("Bearer {}", self.token));
        }
        if let Some(b) = body {
            req.body = serde_json::to_vec(&b).map_err(|e| e.to_string())?;
            req.headers.insert("Content-Type", "application/json");
        }
        self.send(req, &what).await
    }

    pub(super) async fn get(&self, path: &str) -> Result<Value, String> {
        self.call(ehttp::Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.call(ehttp::Method::POST, path, Some(body)).await
    }

    /// A file of the repository as it is at `reference` (a commit, a branch): its bytes; None
    /// when there's no such file there. GitHub gives it base64, up to a megabyte; bigger, as a
    /// blob by its id.
    pub(super) async fn contents(&self, path: &str, reference: &str) -> Result<Option<Vec<u8>>, String> {
        let what = format!("GET the contents of {path}");
        let mut req = ehttp::Request::get(format!("{}/repos/{OWNER}/{REPO}/contents/{path}?ref={reference}", self.api));
        req.headers = ehttp::Headers::new(&[("Accept", "application/vnd.github+json")]);
        if !self.token.is_empty() {
            req.headers.insert("Authorization", format!("Bearer {}", self.token));
        }
        let (status, text, body) = self.fetch(req).await?;
        if status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(failure(status, &text, &body, &what));
        }
        let decoded = |v: &Value| -> Result<Vec<u8>, String> {
            let text: String = v.get("content").and_then(Value::as_str).unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD.decode(text).map_err(|e| format!("{path}: GitHub's base64 can't be read ({e})"))
        };
        if body.get("encoding").and_then(Value::as_str) == Some("base64") && body.get("content").and_then(Value::as_str).is_some_and(|c| !c.trim().is_empty()) {
            return decoded(&body).map(Some);
        }
        // too big for that call: by its blob
        let sha = body.get("sha").and_then(Value::as_str).ok_or_else(|| format!("{path}: GitHub gave neither its content nor its id"))?;
        let blob = self.get(&format!("/repos/{OWNER}/{REPO}/git/blobs/{sha}")).await?;
        decoded(&blob).map(Some)
    }

    /// A form posted to the site (signing in): its answer, whatever it is (`fetch`).
    async fn form(&self, path: &str, fields: &[(&str, &str)]) -> Result<(u16, String, Value), String> {
        let body = fields.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect::<Vec<_>>().join("&");
        let mut req = ehttp::Request::post(format!("{}{path}", self.web), body.into_bytes());
        req.headers = ehttp::Headers::new(&[("Accept", "application/json"), ("Content-Type", "application/x-www-form-urlencoded")]);
        self.fetch(req).await
    }

    /// Signing in: the code to type on github.com (asked again, a little later each time,
    /// while GitHub turns the asking away).
    pub async fn device_code(&self, client: &str) -> Result<DeviceCode, String> {
        let path = "/login/device/code";
        let mut tries = 0;
        let v = loop {
            let (status, text, v) = self.form(path, &[("client_id", client), ("scope", SCOPE)]).await?;
            match status {
                200..300 => break v,
                s if busy(s) && tries < 4 => {
                    tries += 1;
                    pause(AGAIN_MS * tries).await;
                }
                s if busy(s) => return Err(turned_away(&format!("it answered {s}"))),
                s => return Err(failure(s, &text, &v, path)),
            }
        };
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("GitHub's answer has no {k}: {v}"));
        Ok(DeviceCode {
            device_code: text("device_code")?,
            user_code: text("user_code")?,
            verification_uri: text("verification_uri")?,
            interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5),
            expires_in: v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
        })
    }

    /// Whether the code's been typed (and the sign-in let through): the token, once it is.
    /// Turned away for now (or no answer): busy, and what's said if it stays so.
    pub async fn poll(&self, client: &str, device_code: &str) -> Result<Poll, String> {
        let path = "/login/oauth/access_token";
        let (status, text, v) = match self.form(path, &[("client_id", client), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")]).await {
            Ok(answer) => answer,
            Err(e) => return Ok(Poll::Busy(format!("{e}: ask for a new code once it can be, or paste a token"))),
        };
        if busy(status) {
            return Ok(Poll::Busy(turned_away(&format!("it answered {status}"))));
        }
        if !(200..300).contains(&status) {
            return Err(failure(status, &text, &v, path));
        }
        if let Some(t) = v.get("access_token").and_then(Value::as_str) {
            return Ok(Poll::Token(t.to_string()));
        }
        match v.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => Ok(Poll::Wait),
            Some("slow_down") => Ok(Poll::SlowDown),
            Some("expired_token") => Err("The code ran out of time: sign in again".into()),
            Some("access_denied") => Err("The sign-in was turned down on GitHub".into()),
            _ => Err(format!("GitHub didn't sign you in: {v}")),
        }
    }

    /// Who the token is (their login): that they've an account, and it's let in.
    pub async fn who(&self) -> Result<String, String> {
        let v = self.get("/user").await?;
        v.get("login").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "GitHub didn't say who you are".into())
    }

    /// What every publish starts from: who signs it off, the repository (its main branch,
    /// whether the user can write to it) and the copy's commit (`head`, when GitHub has it;
    /// else main's) and its tree. No changes yet.
    pub async fn base(&self, head: Option<String>) -> Result<Survey, String> {
        let signoff = signoff(&self.get("/user").await?);
        let repo = self.get(&format!("/repos/{OWNER}/{REPO}")).await?;
        let branch = repo.get("default_branch").and_then(Value::as_str).unwrap_or("main").to_string();
        let can_push = repo.pointer("/permissions/push").and_then(Value::as_bool).unwrap_or(false);
        let mut found = None;
        if let Some(sha) = head.filter(|s| is_sha(s)) {
            if let Ok(c) = self.get(&format!("/repos/{OWNER}/{REPO}/git/commits/{sha}")).await {
                found = Some((sha, c, false));
            }
        }
        let (base, commit, on_main) = match found {
            Some(f) => f,
            None => {
                let r = self.get(&format!("/repos/{OWNER}/{REPO}/git/ref/heads/{branch}")).await?;
                let sha = r.pointer("/object/sha").and_then(Value::as_str).ok_or("GitHub didn't say where main is")?.to_string();
                let c = self.get(&format!("/repos/{OWNER}/{REPO}/git/commits/{sha}")).await?;
                (sha, c, true)
            }
        };
        let base_tree = commit.pointer("/tree/sha").and_then(Value::as_str).ok_or("GitHub didn't give the commit's tree")?.to_string();
        Ok(Survey { base, base_tree, on_main, branch, can_push, signoff, ..Default::default() })
    }

    /// The copy's content/ (its own tree, read whole): each file's blob id.
    #[cfg(not(target_arch = "wasm32"))]
    async fn content_tree(&self, base_tree: &str) -> Result<BTreeMap<String, String>, String> {
        let top = self.get(&format!("/repos/{OWNER}/{REPO}/git/trees/{base_tree}")).await?;
        let content = top.get("tree").and_then(Value::as_array).and_then(|t| t.iter().find(|e| e["path"] == PUBLISHED && e["type"] == "tree")).and_then(|e| e["sha"].as_str()).map(str::to_string);
        let mut tree = BTreeMap::new();
        if let Some(sha) = content {
            let t = self.get(&format!("/repos/{OWNER}/{REPO}/git/trees/{sha}?recursive=1")).await?;
            if t.get("truncated").and_then(Value::as_bool) == Some(true) {
                return Err("content/ on GitHub is too big to read in one go".into());
            }
            for e in t.get("tree").and_then(Value::as_array).into_iter().flatten().filter(|e| e["type"] == "blob") {
                if let (Some(p), Some(s)) = (e["path"].as_str(), e["sha"].as_str()) {
                    tree.insert(format!("{PUBLISHED}/{p}"), s.to_string());
                }
            }
        }
        Ok(tree)
    }

    /// The desktop's survey (`root`: the project's folder): its content/ against the commit
    /// its checkout is on (else main: then removed files are left unticked, as they may be
    /// new there rather than gone here).
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn survey(&self, root: &Path) -> Result<Survey, String> {
        let mut s = self.base(head_commit(root)).await?;
        let tree = self.content_tree(&s.base_tree).await?;
        let mut local = BTreeMap::new();
        let mut kept = BTreeMap::new();
        for (rel, path) in local_files(root) {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let raw = blob_id(&bytes);
            let bytes = committed(&rel, bytes);
            local.insert(rel.clone(), (blob_id(&bytes), raw, bytes.len() as u64));
            kept.insert(rel, bytes);
        }
        s.changes = changes(&tree, &local);
        for c in &mut s.changes {
            if c.how == How::Removed && s.on_main {
                c.send = false;
            }
            if let Some(b) = kept.remove(&c.path) {
                s.contents.insert(c.path.clone(), b);
            }
        }
        Ok(s)
    }

    /// The browser's survey: what was saved there (`site`) against the site it runs on (its
    /// bundle, its squares; the commit it was built from, when the build said, else main).
    #[cfg(target_arch = "wasm32")]
    pub async fn survey_browser(&self, site: Site) -> Result<Survey, String> {
        let mut s = self.base(option_env!("CF_PAGES_COMMIT_SHA").map(str::to_string)).await?;
        let bundle = site_file("content/bundle.json").await.ok_or("The site's own records (content/bundle.json) can't be read")?;
        let was = content::from_bundles(&[&String::from_utf8_lossy(&bundle)]);
        let (mut changed, mut contents) = record_changes(&site.content, &was);
        // models dropped on the page (kept in the editor's files)
        for path in new_models(&site.content, &was) {
            if let crate::platform::Load::Ready(bytes) = site.files.get(&path) {
                changed.extend(file_change(&path, &bytes, None));
                contents.insert(path, bytes.to_vec());
            }
        }
        // the other files saved here (a big map's squares, models, FX sheets), against the
        // site's
        for (path, bytes) in &site.saved {
            if contents.contains_key(path) {
                continue;
            }
            if let Some(c) = file_change(path, bytes, site_file(path).await.as_deref()) {
                changed.push(c);
                contents.insert(path.clone(), bytes.clone());
            }
        }
        changed.sort_by(|a, b| a.path.cmp(&b.path));
        (s.changes, s.contents) = (changed, contents);
        Ok(s)
    }

    /// The pull request: the files ticked (`sent`, their bytes the survey's) on the copy's
    /// tree, committed on a branch of the repository's (the user can write to it) or of their
    /// fork's (made, and brought up to date, when they can't), opened against main. `step`
    /// says how far it's got. Its address.
    #[allow(clippy::too_many_arguments)]
    pub async fn publish(&self, survey: &Survey, sent: &[Change], title: &str, body: &str, secs: u64, step: &(dyn Fn(String) + Sync)) -> Result<String, String> {
        if sent.is_empty() {
            return Err("Nothing ticked to send".into());
        }
        // where the branch goes
        let (head_repo, head_owner) = if survey.can_push {
            (format!("{OWNER}/{REPO}"), None)
        } else {
            step("making your fork of the repository (or finding it)".into());
            let fork = self.post(&format!("/repos/{OWNER}/{REPO}/forks"), json!({})).await?;
            let name = fork.get("full_name").and_then(Value::as_str).ok_or("GitHub didn't say where your fork is")?.to_string();
            let owner = fork.pointer("/owner/login").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| name.split('/').next().unwrap_or_default().to_string());
            // a new fork takes a moment to be there
            let until = now_secs() + 90;
            while self.get(&format!("/repos/{name}/git/ref/heads/{}", survey.branch)).await.is_err() {
                if now_secs() > until {
                    return Err(format!("Your fork {name} isn't ready yet: publish again in a minute"));
                }
                pause(1500).await;
            }
            // brought up to date (an old fork hasn't the copy's commit); a fork whose main
            // went its own way stays as it is
            let _ = self.post(&format!("/repos/{name}/merge-upstream"), json!({ "branch": survey.branch })).await;
            if self.get(&format!("/repos/{name}/git/commits/{}", survey.base)).await.is_err() {
                return Err(format!(
                    "Your fork {name} hasn't the commit your copy is on ({}): bring its main up to date with the repository's on GitHub, then publish again",
                    &survey.base[..7.min(survey.base.len())]
                ));
            }
            (name, Some(owner))
        };
        let mut entries = vec![];
        let to_send: Vec<&Change> = sent.iter().filter(|c| c.how != How::Removed).collect();
        for (k, c) in to_send.iter().enumerate() {
            step(format!("sending {} ({} of {})", c.path, k + 1, to_send.len()));
            let bytes = survey.contents.get(&c.path).ok_or_else(|| format!("{}: its bytes weren't kept: look again", c.path))?;
            let blob = self.post(&format!("/repos/{head_repo}/git/blobs"), json!({ "content": base64::engine::general_purpose::STANDARD.encode(bytes), "encoding": "base64" })).await?;
            let sha = blob.get("sha").and_then(Value::as_str).ok_or("GitHub didn't keep a file")?;
            entries.push(json!({ "path": c.path, "mode": "100644", "type": "blob", "sha": sha }));
        }
        for c in sent.iter().filter(|c| c.how == How::Removed) {
            entries.push(json!({ "path": c.path, "mode": "100644", "type": "blob", "sha": Value::Null }));
        }
        step("putting the files together".into());
        let tree = self.post(&format!("/repos/{head_repo}/git/trees"), json!({ "base_tree": survey.base_tree, "tree": entries })).await?;
        let tree = tree.get("sha").and_then(Value::as_str).ok_or("GitHub didn't make the tree")?.to_string();
        // signed off as the checks ask (CONTRIBUTING.md): the user ticked that they certify it
        let message = if body.trim().is_empty() { title.trim().to_string() } else { format!("{}\n\n{}", title.trim(), body.trim()) };
        let message = format!("{message}\n\nSigned-off-by: {}", survey.signoff);
        let commit = self.post(&format!("/repos/{head_repo}/git/commits"), json!({ "message": message, "tree": tree, "parents": [survey.base] })).await?;
        let commit = commit.get("sha").and_then(Value::as_str).ok_or("GitHub didn't make the commit")?.to_string();
        let branch = branch_name(title, secs);
        step(format!("making the branch {branch}"));
        self.post(&format!("/repos/{head_repo}/git/refs"), json!({ "ref": format!("refs/heads/{branch}"), "sha": commit })).await?;
        step("opening the pull request".into());
        let listed: Vec<String> = sent
            .iter()
            .map(|c| {
                let how = match c.how {
                    How::Added => "added",
                    How::Changed => "changed",
                    How::Removed => "removed",
                };
                format!("- {how} `{}`", c.path)
            })
            .collect();
        let text = format!("{}\n\nPublished from the Battle Frontier editor.\n\n{}", body.trim(), listed.join("\n"));
        let head = match &head_owner {
            Some(owner) => format!("{owner}:{branch}"),
            None => branch.clone(),
        };
        let pr = self.post(&format!("/repos/{OWNER}/{REPO}/pulls"), json!({ "title": title.trim(), "body": text.trim(), "head": head, "base": survey.branch, "maintainer_can_modify": true })).await?;
        pr.get("html_url").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "GitHub didn't say where the pull request is".into())
    }
}

// ---------------------------------------------------------------- the desktop's own

/// Python: PYTHON names it, else `python` (or `python3`) on the PATH (never the Store's
/// stand-ins), else Windows' `py`.
#[cfg(not(target_arch = "wasm32"))]
fn find_python() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PYTHON").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
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

/// The names check (tools/check-names.py: another game's names) on the text files sent:
/// what it found stops the publish. Nothing to do without the tool or Python.
#[cfg(not(target_arch = "wasm32"))]
fn check_names(root: &Path, sent: &[Change]) -> Result<(), String> {
    let tool = root.join("tools").join("check-names.py");
    let files: Vec<&str> = sent.iter().filter(|c| c.how != How::Removed && !binary(&c.path, &[])).map(|c| c.path.as_str()).collect();
    let (true, Some(python), false) = (tool.is_file(), find_python(), files.is_empty()) else { return Ok(()) };
    let mut c = std::process::Command::new(python);
    c.arg(&tool).args(&files).current_dir(root).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().map_err(|e| format!("The names check didn't run: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Err(format!("The names check found another game's names: take them out first.\n{said}"))
}

/// Where the sign-in is kept on the desktop: the user's own settings folder (not the project's).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn kept_path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    dir.map(|d| d.join("BattleFrontier").join("github.json"))
}

/// The project's folder (GAME_ROOT, else where the game runs).
#[cfg(not(target_arch = "wasm32"))]
fn project_root() -> PathBuf {
    let root = PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
    root.canonicalize().unwrap_or(root)
}

/// What a survey reads: the project's folder on the desktop.
#[cfg(not(target_arch = "wasm32"))]
pub(super) type Source = PathBuf;

/// What a survey reads in the browser: the editor's records as they are now, its files (the
/// models dropped on the page among them), and the other files saved here (kept by the
/// browser, or saved since the page opened when it downloads them) by their paths in the
/// repository (a chunked map's squares, models, FX sheets).
#[cfg(target_arch = "wasm32")]
pub(super) struct Site {
    pub content: Arc<content::Content>,
    pub files: crate::platform::Files,
    pub saved: BTreeMap<String, Vec<u8>>,
}

#[cfg(target_arch = "wasm32")]
pub(super) type Source = Site;

// ---------------------------------------------------------------- signing in and publishing

/// What the tasks say.
pub(super) enum Said {
    Code(DeviceCode),
    In(User),
    NotIn(String),
    Surveyed(Result<Survey, String>),
    Step(String),
    Opened(Result<String, String>),
}

/// Signing in: the code to type (once it's come; asked for at all), a token pasted instead,
/// what went wrong; GitHub being asked for the code or about the token (not while the code's
/// waited for: a token can be pasted meanwhile); the code's waiting stopped.
#[derive(Default)]
pub(super) struct Signing {
    pub code: Option<DeviceCode>,
    pub asked: bool,
    pub pasted: String,
    pub error: Option<String>,
    pub busy: bool,
    pub stop: Arc<AtomicBool>,
}

/// Signing in and publishing.
pub(super) struct Publisher {
    pub user: Option<User>,
    pub signing: Option<Signing>,
    /// the Publish window: open; what's changed (None while it's looked for)
    pub open: bool,
    pub survey: Option<Result<Survey, String>>,
    pub title: String,
    pub body: String,
    /// the Developer Certificate of Origin certified (the commit signed off): ticked each time
    pub certify: bool,
    /// sending: how far it's got; sent: the pull request's address, or what went wrong
    pub step: Option<String>,
    pub opened: Option<Result<String, String>>,
    /// "Look again" clicked (the editor says what to look at); the map open has changes
    /// not saved (not sent)
    pub again: bool,
    pub unsaved: bool,
    /// the desktop: where the sign-in is kept (None: nowhere); the folder published from
    /// (None: the project's)
    #[cfg(not(target_arch = "wasm32"))]
    pub kept: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub root: Option<PathBuf>,
    /// the kept sign-in read
    pub loaded: bool,
    /// GitHub (stand-ins in the tests and the checks)
    pub api: String,
    pub web: String,
    /// what the tasks say, and how they say it
    pub tx: mpsc::Sender<Said>,
    pub rx: mpsc::Receiver<Said>,
}

impl Default for Publisher {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Publisher {
            user: None,
            signing: None,
            open: false,
            survey: None,
            title: String::new(),
            body: String::new(),
            certify: false,
            step: None,
            opened: None,
            again: false,
            unsaved: false,
            #[cfg(not(target_arch = "wasm32"))]
            kept: kept_path(),
            #[cfg(not(target_arch = "wasm32"))]
            root: None,
            loaded: false,
            api: "https://api.github.com".into(),
            // the browser signs in through the site's own address (functions/github/login/)
            web: if cfg!(target_arch = "wasm32") { "github".into() } else { "https://github.com".into() },
            tx,
            rx,
        }
    }
}

impl Publisher {
    /// The folder published from.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn root(&self) -> PathBuf {
        self.root.clone().unwrap_or_else(project_root)
    }

    pub(super) fn hub(&self, token: &str) -> Hub {
        Hub { api: self.api.clone(), web: self.web.clone(), token: token.into() }
    }

    /// GitHub as the signed-in user, and who that is; None signed out.
    pub(super) fn signed(&self) -> Option<(Hub, String)> {
        self.user.as_ref().map(|u| (self.hub(&u.token), u.login.clone()))
    }

    /// The kept sign-in, read the first time it's asked for.
    pub fn load(&mut self) {
        if std::mem::replace(&mut self.loaded, true) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let read = self.kept.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        #[cfg(target_arch = "wasm32")]
        let read = crate::platform::stored_text(SIGNIN);
        if let Some(u) = read.and_then(|t| serde_json::from_str::<User>(&t).ok()).filter(|u| !u.token.is_empty()) {
            self.user = Some(u);
        }
    }

    /// The sign-in kept (on the desktop in the user's settings folder; in the browser its
    /// storage), or forgotten.
    fn keep(&self, user: Option<&User>) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = &self.kept {
            match user {
                Some(u) => {
                    let written = p.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|()| std::fs::write(p, serde_json::to_string(u).unwrap_or_default()));
                    if let Err(e) = written {
                        log::warn!("the sign-in isn't kept ({}): {e}", p.display());
                    }
                }
                None => {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        match user {
            Some(u) => {
                if let Err(e) = crate::platform::store_text(SIGNIN, &serde_json::to_string(u).unwrap_or_default()) {
                    log::warn!("the sign-in isn't kept: {e}");
                }
            }
            None => crate::platform::forget_text(SIGNIN),
        }
    }

    /// What the tasks said since: true when anything did (the window drawn again).
    pub fn take(&mut self) -> bool {
        let mut any = false;
        while let Ok(said) = self.rx.try_recv() {
            any = true;
            match said {
                Said::Code(c) => {
                    if let Some(s) = &mut self.signing {
                        (s.code, s.busy) = (Some(c), false);
                    }
                }
                Said::In(u) => {
                    self.keep(Some(&u));
                    self.user = Some(u);
                    self.signing = None;
                }
                Said::NotIn(e) => {
                    if let Some(s) = &mut self.signing {
                        s.error = Some(e);
                        s.busy = false;
                        s.code = None;
                    }
                }
                Said::Surveyed(s) => {
                    // what GitHub doesn't take any more: signed out
                    if matches!(&s, Err(e) if e.contains("sign in again")) {
                        self.sign_out();
                    }
                    self.survey = Some(s);
                }
                Said::Step(s) => self.step = Some(s),
                Said::Opened(r) => {
                    self.step = None;
                    self.opened = Some(r);
                }
            }
        }
        any
    }

    pub fn busy(&self) -> bool {
        self.signing.as_ref().is_some_and(|s| s.busy) || (self.open && self.survey.is_none()) || self.step.is_some()
    }

    /// Signing in by the device flow: its code asked for, then GitHub asked whether it's
    /// been typed until it has (or it's stopped, or out of time), at GitHub's pace; while it
    /// turns the asking away, less and less often, given up on after `AWAY_TIMES` in a row.
    /// Nothing's said once it's stopped.
    pub fn start_device(&mut self, ctx: &egui::Context) {
        let client = client_id();
        let s = self.signing.get_or_insert_with(Default::default);
        if client.is_empty() {
            s.error = Some("Signing in on github.com isn't set up for this project yet (README, \"Publish\"): paste a token instead".into());
            return;
        }
        s.stop.store(true, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        (s.stop, s.busy, s.asked, s.error, s.code) = (stop.clone(), true, true, None, None);
        let (hub, tx, ctx) = (self.hub(""), self.tx.clone(), ctx.clone());
        spawn(async move {
            let said = |s: Said| {
                if !stop.load(Ordering::Relaxed) {
                    let _ = tx.send(s);
                    ctx.request_repaint();
                }
            };
            let code = match hub.device_code(&client).await {
                Ok(c) => c,
                Err(e) => return said(Said::NotIn(e)),
            };
            said(Said::Code(code.clone()));
            let mut every = code.interval.max(1);
            let (mut wait, mut away) = (every, 0);
            let until = now_secs() + code.expires_in;
            loop {
                for _ in 0..wait * 10 {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    pause(SECOND_MS / 10).await;
                }
                if now_secs() > until {
                    return said(Said::NotIn("The code ran out of time: sign in again".into()));
                }
                match hub.poll(&client, &code.device_code).await {
                    Ok(Poll::Token(t)) => {
                        let you = Hub { token: t.clone(), ..hub.clone() };
                        return said(match you.who().await {
                            Ok(login) => Said::In(User { login, token: t }),
                            Err(e) => Said::NotIn(e),
                        });
                    }
                    Ok(Poll::Wait) => (wait, away) = (every, 0),
                    Ok(Poll::SlowDown) => {
                        every = (every + 5).min(30);
                        (wait, away) = (every, 0);
                    }
                    Ok(Poll::Busy(why)) => {
                        away += 1;
                        if away >= AWAY_TIMES {
                            return said(Said::NotIn(why));
                        }
                        wait = (wait * 2).min(30);
                    }
                    Err(e) => return said(Said::NotIn(e)),
                }
            }
        });
    }

    /// Signing in with a token pasted: who it is, asked of GitHub. A code being waited for is
    /// given up (one way at a time).
    pub fn start_token(&mut self, ctx: &egui::Context) {
        let Some(s) = &mut self.signing else { return };
        let token = s.pasted.trim().to_string();
        if token.is_empty() {
            return;
        }
        s.stop.store(true, Ordering::Relaxed);
        (s.busy, s.error, s.code) = (true, None, None);
        let (hub, tx, ctx) = (self.hub(&token), self.tx.clone(), ctx.clone());
        spawn(async move {
            let said = match hub.who().await {
                Ok(login) => Said::In(User { login, token }),
                Err(e) => Said::NotIn(format!("That token isn't let in: {e}")),
            };
            let _ = tx.send(said);
            ctx.request_repaint();
        });
    }

    pub fn stop_signing(&mut self) {
        if let Some(s) = self.signing.take() {
            s.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Signed out: the kept sign-in forgotten too.
    pub fn sign_out(&mut self) {
        self.user = None;
        self.keep(None);
    }

    /// The Publish window opened: what's changed, looked for in `source`.
    pub fn open_window(&mut self, source: Source, ctx: &egui::Context) {
        let Some(user) = &self.user else { return };
        (self.open, self.survey, self.opened, self.step, self.certify) = (true, None, None, None, false);
        let (hub, tx, ctx) = (self.hub(&user.token), self.tx.clone(), ctx.clone());
        spawn(async move {
            #[cfg(not(target_arch = "wasm32"))]
            let s = hub.survey(&source).await;
            #[cfg(target_arch = "wasm32")]
            let s = hub.survey_browser(source).await;
            let _ = tx.send(Said::Surveyed(s));
            ctx.request_repaint();
        });
    }

    /// The pull request asked for: the files ticked sent, as a task of its own.
    pub fn start_publish(&mut self, ctx: &egui::Context) {
        let (Some(user), Some(Ok(survey))) = (&self.user, &self.survey) else { return };
        let sent: Vec<Change> = survey.changes.iter().filter(|c| c.send).cloned().collect();
        let (hub, survey, tx, ctx) = (self.hub(&user.token), survey.clone(), self.tx.clone(), ctx.clone());
        let (title, body) = (self.title.clone(), self.body.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let root = self.root();
        self.step = Some("starting".into());
        self.opened = None;
        spawn(async move {
            let step = |s: String| {
                let _ = tx.send(Said::Step(s));
                ctx.request_repaint();
            };
            #[cfg(not(target_arch = "wasm32"))]
            {
                step("checking the names in the files".into());
                if let Err(e) = check_names(&root, &sent) {
                    let _ = tx.send(Said::Opened(Err(e)));
                    ctx.request_repaint();
                    return;
                }
            }
            let r = hub.publish(&survey, &sent, &title, &body, now_secs(), &step).await;
            let _ = tx.send(Said::Opened(r));
            ctx.request_repaint();
        });
    }
}

/// A change's mark in the list.
fn badge(ui: &mut egui::Ui, how: How) {
    let (text, color) = match how {
        How::Added => ("added", Color32::from_rgb(110, 200, 120)),
        How::Changed => ("changed", Color32::from_rgb(120, 170, 235)),
        How::Removed => ("removed", Color32::from_rgb(235, 140, 110)),
    };
    ui.label(RichText::new(text).size(11.0).color(color));
}

/// The code to type on github.com while it's waited for, and a way there (a tab of its own:
/// the editor's stays as it is).
fn code_shown(ui: &mut egui::Ui, code: &DeviceCode) {
    ui.label(RichText::new("Type this code on GitHub's page (an account there is needed):").color(TEXT));
    ui.horizontal(|ui| {
        ui.label(RichText::new(&code.user_code).monospace().size(24.0).strong().color(TEXT));
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(code.user_code.clone());
        }
    });
    ui.add(egui::Hyperlink::from_label_and_url(format!("Open {}", code.verification_uri), &code.verification_uri).open_in_new_tab(true));
    ui.horizontal(|ui| {
        ui.spinner();
        hint(ui, "Waiting for GitHub to let you in…");
    });
}

/// A token to paste: what it is, where one's made (a tab of its own) and where it's kept.
/// True when it's to be tried.
fn token_part(ui: &mut egui::Ui, s: &mut Signing) -> bool {
    hint(
        ui,
        if cfg!(target_arch = "wasm32") {
            "A token of yours (a classic one, with the public_repo scope; an account on GitHub is needed): it goes from this browser straight to GitHub, and it's kept in this browser's storage for the site, never in the project."
        } else {
            "A token of yours (a classic one, with the public_repo scope): it's kept in your own settings folder, never in the project."
        },
    );
    let made = ui.add(egui::Hyperlink::from_label_and_url("Make one on GitHub (its scope ticked already)", TOKEN_PAGE).open_in_new_tab(true));
    mark("uiSignInMake", &made);
    let mut go = false;
    ui.horizontal(|ui| {
        let t = ui.add(egui::TextEdit::singleline(&mut s.pasted).password(true).desired_width(260.0).hint_text("ghp_…"));
        mark("uiSignInToken", &t);
        let b = ui.add_enabled(!s.pasted.trim().is_empty() && !s.busy, egui::Button::new("Sign in"));
        mark("uiSignInPaste", &b);
        go = b.clicked();
    });
    go
}

/// A size, short.
pub(super) fn size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

impl MapEditor {
    /// The top bar's Sign in (or who's signed in, to sign out) and Publish (waiting for it).
    pub(super) fn publish_buttons(&mut self, ui: &mut egui::Ui) {
        let reviewing = self.review.is_some();
        let p = &mut self.publisher;
        p.load();
        if p.take() {
            ui.ctx().request_repaint();
        }
        let ctx = ui.ctx().clone();
        match p.user.clone() {
            Some(u) => {
                let who = ui.menu_button(format!("@{}", u.login), |ui| {
                    hint(ui, "Signed in with GitHub: pull requests are opened as you.");
                    let out = ui.button("Sign out").on_hover_text("Forgets the sign-in here (it stays on GitHub's side until you take it back there)");
                    mark("uiSignOut", &out);
                    if out.clicked() {
                        p.sign_out();
                        ui.close();
                    }
                });
                mark("uiSignIn", &who.response);
            }
            None => {
                let s = ui.button(if p.signing.is_some() { "Signing in…" } else { "Sign in" }).on_hover_text("Sign in with GitHub to publish what you make as a pull request (an account there is needed)");
                mark("uiSignIn", &s);
                if s.clicked() && p.signing.is_none() {
                    p.signing = Some(Default::default());
                    // the browser starts with a token: the code's calls go through the site,
                    // which github.com turns away at times
                    if !cfg!(target_arch = "wasm32") {
                        p.start_device(&ctx);
                    }
                }
            }
        }
        let signed = p.user.is_some() && !reviewing;
        let about = if cfg!(target_arch = "wasm32") {
            "What you've saved in this browser (records, models dropped, a big map's squares), to look over and send as a pull request: the maintainers look at it before it's in the game"
        } else {
            "Every file of content/ you've changed, to look over and send as a pull request: the maintainers look at it before it's in the game"
        };
        let b = ui.add_enabled(signed, egui::Button::new("Publish")).on_hover_text(about).on_disabled_hover_text(if reviewing { "Reviewing a pull request: nothing of it is yours to publish" } else { "Sign in with GitHub first: the pull request is opened as you" });
        mark("uiPublish", &b);
        crate::platform::set_hook("signedin", p.user.as_ref().map(|u| u.login.as_str()).unwrap_or(""));
        if b.clicked() {
            let source = self.publish_source();
            self.publisher.open_window(source, &ctx);
        }
    }

    /// The sign-in window and the Publish window, when they're up; "Look again" looked.
    pub(super) fn publish_window(&mut self, ctx: &egui::Context) {
        self.publisher.unsaved = self.dirty;
        self.publisher.windows(ctx);
        if std::mem::take(&mut self.publisher.again) {
            let source = self.publish_source();
            self.publisher.open_window(source, ctx);
        }
    }

    /// What a survey reads: the project's folder; in the browser, what the editor holds now.
    fn publish_source(&self) -> Source {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.publisher.root()
        }
        #[cfg(target_arch = "wasm32")]
        {
            // the files besides records saved here (`crate::keep`), the game's own (content/:
            // never temporary/'s, never the model requests' private/ folders)
            let saved = crate::keep::files()
                .into_iter()
                .filter_map(|(path, k)| match k {
                    crate::keep::Kept::File(b) if crate::keep::record_of(&path).is_none() && published(&path) => Some((path, b.to_vec())),
                    _ => None,
                })
                .collect();
            Site { content: self.content.clone(), files: self.files.clone(), saved }
        }
    }

    /// The checks: GitHub at a stand-in (`base`: its address, the API under base/api).
    pub fn github_at(&mut self, base: &str) {
        let base = base.trim_end_matches('/');
        (self.publisher.api, self.publisher.web) = (format!("{base}/api"), base.to_string());
    }
}

impl Publisher {
    /// The sign-in window and the Publish window, when they're up.
    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        self.sign_in_window(ctx);
        let p = self;
        crate::platform::set_hook("publish", if p.open { "open" } else { "" });
        let listed = match &p.survey {
            Some(Ok(s)) => s
                .changes
                .iter()
                .map(|c| {
                    let how = match c.how {
                        How::Added => "added",
                        How::Changed => "changed",
                        How::Removed => "removed",
                    };
                    format!("{}:{how}{}", c.path, if c.send { "" } else { ":out" })
                })
                .collect::<Vec<_>>()
                .join(";"),
            Some(Err(e)) => format!("error: {e}"),
            None => String::new(),
        };
        crate::platform::set_hook("publishfiles", &listed);
        crate::platform::set_hook(
            "published",
            &match &p.opened {
                Some(Ok(url)) => url.clone(),
                Some(Err(e)) => format!("error: {e}"),
                None => String::new(),
            },
        );
        if !p.open {
            return;
        }
        if p.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("editor-publish")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Publish your changes").size(20.0).color(TEXT));
            ui.add_space(4.0);
            hint(ui, &format!("They go to {OWNER}/{REPO} on GitHub as a pull request: its maintainers look at every one, and only they can merge it into the game."));
            if p.unsaved {
                warn(ui, "The map open has changes not saved: save it first, or they're not sent.");
            }
            ui.add_space(6.0);
            let since = if cfg!(target_arch = "wasm32") { "saved in this browser, against the site" } else { "changed since your copy" };
            match &mut p.survey {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Looking for what's changed in content/…").color(TEXT));
                    });
                }
                Some(Err(e)) => {
                    warn(ui, e);
                }
                Some(Ok(s)) if s.changes.is_empty() => {
                    let none = if cfg!(target_arch = "wasm32") { "Nothing's been saved in this browser that the site hasn't: save (download) what you've made first." } else { "Nothing's changed in content/ since your copy." };
                    ui.label(RichText::new(format!("{none} ({})", &s.base[..7.min(s.base.len())])).color(TEXT));
                }
                Some(Ok(s)) => {
                    if s.on_main {
                        let note = if cfg!(target_arch = "wasm32") {
                            "This site doesn't say which commit it was built from: it's sent onto main, so what's changed there since (in the same files) is the maintainers' to sort out."
                        } else {
                            "Your folder isn't a git checkout of one of the repository's commits: it's compared with main, so what's new there since your copy shows too (removed files are left unticked)."
                        };
                        warn(ui, note);
                    }
                    let sending = s.changes.iter().filter(|c| c.send).count();
                    hint(ui, &format!("{} file{} {since} ({}), {sending} ticked to send:", s.changes.len(), if s.changes.len() == 1 { "" } else { "s" }, &s.base[..7.min(s.base.len())]));
                    let mut groups: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
                    for (i, c) in s.changes.iter().enumerate() {
                        groups.entry(group(&c.path)).or_default().push(i);
                    }
                    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (name, list) in groups {
                            ui.label(RichText::new(format!("{name} ({})", list.len())).strong().size(12.5).color(TEXT));
                            for i in list {
                                let c = &mut s.changes[i];
                                ui.horizontal(|ui| {
                                    let tick = ui.checkbox(&mut c.send, "");
                                    mark(&format!("uiPublishFile_{}", c.path.trim_start_matches("content/")), &tick);
                                    badge(ui, c.how);
                                    ui.label(RichText::new(c.path.trim_start_matches("content/")).monospace().size(11.5).color(TEXT));
                                    if c.how != How::Removed {
                                        hint(ui, &size(c.size));
                                    }
                                });
                            }
                            ui.add_space(4.0);
                        }
                    });
                }
            }
            let ready = matches!(&p.survey, Some(Ok(s)) if s.changes.iter().any(|c| c.send));
            if ready {
                ui.add_space(6.0);
                ui.label(RichText::new("Title").color(TEXT));
                let t = ui.add(egui::TextEdit::singleline(&mut p.title).desired_width(f32::INFINITY).hint_text("What it adds or changes: \"A bigger pond on the meadow\""));
                mark("uiPublishTitle", &t);
                ui.label(RichText::new("Description").color(TEXT));
                let d = ui.add(egui::TextEdit::multiline(&mut p.body).desired_rows(3).desired_width(f32::INFINITY).hint_text("Anything the maintainers should know (optional)"));
                mark("uiPublishBody", &d);
                ui.add_space(4.0);
                let sign = ui.checkbox(&mut p.certify, RichText::new("It's my own work, or I've the right to send it under the game's licences").color(TEXT)).on_hover_text(
                    "The Developer Certificate of Origin (CONTRIBUTING.md): the commit is signed off as you, as every commit to the game is. Content is under CC BY-SA 4.0",
                );
                mark("uiPublishCertify", &sign);
                if let Some(Ok(s)) = &p.survey {
                    hint(ui, &format!("Signed-off-by: {}", s.signoff));
                }
            }
            ui.add_space(8.0);
            if let Some(step) = &p.step {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Publishing: {step}…")).color(TEXT));
                });
            }
            match &p.opened {
                Some(Ok(url)) => {
                    ui.label(RichText::new("The pull request is open:").color(Color32::from_rgb(130, 210, 140)));
                    ui.add(egui::Hyperlink::from_label_and_url(url.as_str(), url.as_str()).open_in_new_tab(true));
                }
                Some(Err(e)) => warn(ui, e),
                None => {}
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let done = matches!(p.opened, Some(Ok(_)));
                let can = ready && !done && p.step.is_none() && !p.title.trim().is_empty() && p.certify;
                let why = if !ready {
                    "Nothing ticked to send"
                } else if p.title.trim().is_empty() {
                    "A title first"
                } else {
                    "Tick that it's yours to send first (it's signed off as you)"
                };
                let go = ui.add_enabled(can, egui::Button::new(RichText::new("Open the pull request").strong())).on_disabled_hover_text(why);
                mark("uiPublishSend", &go);
                if go.clicked() {
                    p.start_publish(ctx);
                }
                if matches!(p.survey, Some(Err(_))) || (matches!(&p.survey, Some(Ok(s)) if s.changes.is_empty())) {
                    let again = ui.button("Look again");
                    mark("uiPublishAgain", &again);
                    if again.clicked() {
                        p.again = true;
                    }
                }
                let c = ui.add_enabled(p.step.is_none(), egui::Button::new(if done { "Done" } else { "Close" }));
                mark("uiPublishClose", &c);
                if c.clicked() {
                    close = true;
                }
            });
        });
        if close || (modal.should_close() && p.step.is_none()) {
            p.open = false;
            if matches!(p.opened, Some(Ok(_))) {
                // sent: the next one starts afresh
                (p.title, p.body, p.opened) = (String::new(), String::new(), None);
            }
        }
    }

    /// Signing in: on the desktop the code to type on github.com first, a token to paste
    /// instead; in the browser the token first (it goes from the page straight to GitHub's
    /// API, while the code's calls go through the site, which github.com turns away at times),
    /// the code a button away; what went wrong.
    fn sign_in_window(&mut self, ctx: &egui::Context) {
        let p = self;
        crate::platform::set_hook("signing", if p.signing.is_some() { "open" } else { "" });
        crate::platform::set_hook("signincode", p.signing.as_ref().and_then(|s| s.code.as_ref()).map(|c| c.user_code.as_str()).unwrap_or(""));
        crate::platform::set_hook("signinerror", p.signing.as_ref().and_then(|s| s.error.as_deref()).unwrap_or(""));
        let Some(s) = &mut p.signing else { return };
        let browser = cfg!(target_arch = "wasm32");
        let (mut stop, mut paste, mut code) = (false, false, false);
        let modal = egui::Modal::new(egui::Id::new("editor-sign-in")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Sign in with GitHub").size(20.0).color(TEXT));
            ui.add_space(6.0);
            if browser {
                paste = token_part(ui, s);
                ui.add_space(4.0);
            }
            match &s.code {
                Some(c) => code_shown(ui, c),
                None if s.busy => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        hint(ui, "Asking GitHub…");
                    });
                }
                None => {}
            }
            if let Some(e) = &s.error {
                warn(ui, e);
            }
            if s.code.is_none() && !s.busy {
                ui.add_space(4.0);
                let a = ui.button(if s.asked { "Ask for a new code" } else { "Sign in with a code instead" });
                mark("uiSignInCode", &a);
                code = a.clicked();
                if browser && !s.asked {
                    hint(ui, "A code to type on github.com: its calls go through the site, which GitHub turns away at times.");
                }
            }
            if !browser {
                ui.add_space(8.0);
                ui.collapsing("Or paste a token", |ui| paste = token_part(ui, s));
            }
            ui.add_space(6.0);
            let c = ui.button("Cancel");
            mark("uiSignInCancel", &c);
            stop = c.clicked();
        });
        if paste {
            p.start_token(ctx);
        } else if code {
            p.start_device(ctx);
        }
        if stop || modal.should_close() {
            p.stop_signing();
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) use tests::stand_in;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use super::*;

    pub(in crate::mapedit) type Asked = Arc<Mutex<Vec<(String, String, Value)>>>;

    /// A stand-in for GitHub on this machine: each request answered by `answer` (its status
    /// and JSON), and kept (its method, path and body: JSON, or a form as a string).
    pub(in crate::mapedit) fn stand_in(answer: impl Fn(&str, &str, &Value) -> (u16, Value) + Send + Sync + 'static) -> (String, Asked) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked: Asked = Arc::default();
        let (kept, answer) = (asked.clone(), Arc::new(answer));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (kept, answer) = (kept.clone(), answer.clone());
                std::thread::spawn(move || serve(stream, &kept, &*answer));
            }
        });
        (base, asked)
    }

    fn serve(stream: TcpStream, kept: &Asked, answer: &dyn Fn(&str, &str, &Value) -> (u16, Value)) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut out = stream;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let mut parts = line.split_whitespace();
            let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
            let mut length = 0;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 {
                    return;
                }
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            let json = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into())) };
            let (status, reply) = answer(&method, &path, &json);
            kept.lock().unwrap().push((method, path, json));
            let text = reply.to_string();
            if write!(out, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}", text.len()).is_err() {
                return;
            }
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("battle-frontier-publish-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    #[test]
    fn a_file_s_blob_as_git_names_it_its_line_ends_as_committed() {
        assert_eq!(blob_id(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert_eq!(blob_id(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
        assert_eq!(committed("content/data/maps/a.json", b"{\r\n  \"a\": 1\r\n}\r\n".to_vec()), b"{\n  \"a\": 1\n}\n");
        assert_eq!(committed("content/data/maps/a.json", b"one\rtwo\n".to_vec()), b"one\rtwo\n", "a lone CR stays");
        assert_eq!(committed("content/assets/models/a.glb", b"glTF\r\n".to_vec()), b"glTF\r\n", "a model as it is");
        assert_eq!(committed("content/data/x.bin", b"a\0\r\n".to_vec()), b"a\0\r\n", "a zero byte: binary");
    }

    #[test]
    fn the_commit_a_folder_is_a_copy_of_read_from_its_git() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let other = "89abcdef0123456789abcdef0123456789abcdef";
        let root = folder("head");
        assert_eq!(head_commit(&root), None, "no checkout");
        write(&root, ".git/HEAD", b"ref: refs/heads/main\n");
        write(&root, ".git/refs/heads/main", format!("{sha}\n").as_bytes());
        assert_eq!(head_commit(&root).as_deref(), Some(sha), "its branch");
        std::fs::remove_file(root.join(".git/refs/heads/main")).unwrap();
        write(&root, ".git/packed-refs", format!("# pack-refs\n{other} refs/heads/side\n{sha} refs/heads/main\n").as_bytes());
        assert_eq!(head_commit(&root).as_deref(), Some(sha), "packed");
        write(&root, ".git/HEAD", format!("{other}\n").as_bytes());
        assert_eq!(head_commit(&root).as_deref(), Some(other), "detached");
        // a worktree: .git names its folder, whose refs are the repository's
        let tree = folder("head-worktree");
        write(&root, ".git/worktrees/w/HEAD", b"ref: refs/heads/main\n");
        write(&root, ".git/worktrees/w/commondir", b"../..\n");
        write(&tree, ".git", format!("gitdir: {}\n", root.join(".git/worktrees/w").display()).as_bytes());
        assert_eq!(head_commit(&tree).as_deref(), Some(sha), "a worktree's");
    }

    #[test]
    fn what_s_changed_added_changed_and_removed_in_groups() {
        let tree = BTreeMap::from([
            ("content/data/maps/same.json".to_string(), blob_id(b"a\n")),
            ("content/data/maps/crlf.json".to_string(), blob_id(b"b\n")),
            ("content/data/props/lamp.json".to_string(), blob_id(b"old\n")),
            ("content/assets/models/gone.glb".to_string(), blob_id(b"glTF")),
        ]);
        let entry = |raw: &[u8], kept: &[u8]| (blob_id(kept), blob_id(raw), kept.len() as u64);
        let local = BTreeMap::from([
            ("content/data/maps/same.json".to_string(), entry(b"a\n", b"a\n")),
            ("content/data/maps/crlf.json".to_string(), entry(b"b\r\n", b"b\n")),
            ("content/data/props/lamp.json".to_string(), entry(b"new\n", b"new\n")),
            ("content/data/maps/pond.json".to_string(), entry(b"{}\n", b"{}\n")),
        ]);
        let all = changes(&tree, &local);
        let found: Vec<(&str, How)> = all.iter().map(|c| (c.path.as_str(), c.how)).collect();
        assert_eq!(
            found,
            [("content/assets/models/gone.glb", How::Removed), ("content/data/maps/pond.json", How::Added), ("content/data/props/lamp.json", How::Changed)],
            "the same bytes (with CRLF or not) unchanged"
        );
        assert_eq!(group("content/data/maps/pond.json"), "Maps and their events");
        assert_eq!(group("content/assets/models/gone.glb"), "Models");
        assert_eq!(group("content/data/moves/x.json"), "Abilities and moves");
        assert_eq!(group("content/README.md"), "Other files");
        assert_eq!(branch_name("A bigger pond, on the meadow!", 7), "publish/a-bigger-pond-on-the-meadow-7");
        assert_eq!(branch_name("!!!", 7), "publish/7");
    }

    #[test]
    fn the_folder_s_files_but_the_web_build_s_bundle() {
        let root = folder("files");
        write(&root, "content/data/maps/a.json", b"{}");
        write(&root, "content/bundle.json", b"{}");
        write(&root, "content/assets/models/a.glb", b"glTF");
        write(&root, "content/data/run.log", b"x");
        write(&root, "temporary/data/maps/b.json", b"{}");
        let names: Vec<String> = local_files(&root).into_iter().map(|(p, _)| p).collect();
        assert_eq!(names, ["content/assets/models/a.glb", "content/data/maps/a.json"]);
    }

    /// The browser's side: what the editor holds against what the site has: a prop changed,
    /// one added with a model dropped, one gone (left out), a placeholder's never; a square
    /// saved the same as the site's (nothing), and another (changed).
    #[test]
    fn what_the_browser_saved_against_the_site_s_records() {
        use content::{Prop, Space};
        let prop = |id: &str, height: f64, model: Option<&str>| Prop { id: id.into(), name: id.into(), height, model: model.map(str::to_string), ..Default::default() };
        let mut was = content::Content::default();
        was.props.put(prop("lamp", 2.0, Some("models/lamp.glb")), Space::Content);
        was.props.put(prop("old_sign", 1.0, None), Space::Content);
        was.props.put(prop("statue", 3.0, None), Space::Temporary);
        let mut now = content::Content::default();
        now.props.put(prop("lamp", 2.5, Some("models/lamp.glb")), Space::Content);
        now.props.put(prop("pond_rock", 0.5, Some("models/pond_rock.glb")), Space::Content);
        now.props.put(prop("statue", 4.0, None), Space::Temporary);
        let (found, sent) = record_changes(&now, &was);
        let listed: Vec<(&str, How, bool)> = found.iter().map(|c| (c.path.as_str(), c.how, c.send)).collect();
        assert_eq!(
            listed,
            [("content/data/props/lamp.json", How::Changed, true), ("content/data/props/pond_rock.json", How::Added, true), ("content/data/props/old_sign.json", How::Removed, false)],
            "the placeholders' never"
        );
        assert_eq!(sent["content/data/props/lamp.json"], content::to_text(now.props.get("lamp").unwrap()).into_bytes(), "as the desktop writes it");
        assert_eq!(new_models(&now, &was), ["content/assets/models/pond_rock.glb"]);
        assert_eq!(file_change("content/data/maps/m/0_0.json", b"a", Some(b"a")), None);
        assert_eq!(file_change("content/data/maps/m/1_0.json", b"b", Some(b"a")).map(|c| c.how), Some(How::Changed));
        assert_eq!(file_change("content/data/maps/m/2_0.json", b"b", None).map(|c| (c.how, c.size)), Some((How::Added, 1)));
    }

    /// A project of three files changed since its commit (one added, one changed, one gone),
    /// against a GitHub where the user can't write to the repository: its fork made, the
    /// changed files sent there, a tree on the copy's, a commit on the copy's commit, a branch,
    /// the pull request from the fork's branch to main.
    #[test]
    fn published_from_a_fork_the_files_ticked_on_the_copy_s_tree() {
        let base = "1111111111111111111111111111111111111111";
        let root = folder("publish");
        write(&root, ".git/HEAD", b"ref: refs/heads/main\n");
        write(&root, ".git/refs/heads/main", base.as_bytes());
        write(&root, "content/data/maps/same.json", b"a\n");
        write(&root, "content/data/maps/pond.json", b"{\r\n  \"id\": \"pond\"\r\n}\r\n");
        write(&root, "content/data/props/lamp.json", b"new\n");
        let (same, lamp_old) = (blob_id(b"a\n"), blob_id(b"old\n"));
        let (api, asked) = stand_in(move |method, path, _body| match (method, path) {
            ("GET", "/user") => (200, json!({ "login": "player", "id": 42, "name": "Pat Player", "email": null })),
            ("GET", "/repos/LouisMareau/battle-frontier") => (200, json!({ "default_branch": "main", "permissions": { "push": false } })),
            ("GET", p) if p.ends_with(&format!("/git/commits/{base}")) => (200, json!({ "sha": base, "tree": { "sha": "t0" } })),
            ("GET", "/repos/LouisMareau/battle-frontier/git/trees/t0") => (200, json!({ "tree": [{ "path": "content", "type": "tree", "sha": "c0" }, { "path": "README.md", "type": "blob", "sha": "r0" }] })),
            ("GET", "/repos/LouisMareau/battle-frontier/git/trees/c0?recursive=1") => (
                200,
                json!({ "truncated": false, "tree": [
                    { "path": "data", "type": "tree", "sha": "d0" },
                    { "path": "data/maps/same.json", "type": "blob", "sha": same },
                    { "path": "data/props/lamp.json", "type": "blob", "sha": lamp_old },
                    { "path": "data/props/gone.json", "type": "blob", "sha": "g0" },
                ] }),
            ),
            ("POST", "/repos/LouisMareau/battle-frontier/forks") => (202, json!({ "full_name": "player/battle-frontier", "owner": { "login": "player" } })),
            ("GET", "/repos/player/battle-frontier/git/ref/heads/main") => (200, json!({ "object": { "sha": base } })),
            ("POST", "/repos/player/battle-frontier/merge-upstream") => (200, json!({})),
            ("POST", "/repos/player/battle-frontier/git/blobs") => (201, json!({ "sha": "b1" })),
            ("POST", "/repos/player/battle-frontier/git/trees") => (201, json!({ "sha": "t1" })),
            ("POST", "/repos/player/battle-frontier/git/commits") => (201, json!({ "sha": "k1" })),
            ("POST", "/repos/player/battle-frontier/git/refs") => (201, json!({})),
            ("POST", "/repos/LouisMareau/battle-frontier/pulls") => (201, json!({ "html_url": "https://github.com/LouisMareau/battle-frontier/pull/7" })),
            _ => (404, json!({ "message": "Not Found" })),
        });
        let hub = Hub { api: api.clone(), web: api, token: "t".into() };
        let survey = block_on(hub.survey(&root)).unwrap();
        assert_eq!((survey.base.as_str(), survey.base_tree.as_str(), survey.on_main, survey.can_push), (base, "t0", false, false));
        assert_eq!(survey.signoff, "Pat Player <42+player@users.noreply.github.com>");
        let found: Vec<(&str, How, bool)> = survey.changes.iter().map(|c| (c.path.as_str(), c.how, c.send)).collect();
        assert_eq!(
            found,
            [("content/data/maps/pond.json", How::Added, true), ("content/data/props/gone.json", How::Removed, true), ("content/data/props/lamp.json", How::Changed, true)]
        );
        assert_eq!(survey.changes[0].size, 19, "sent with LF line ends");
        assert_eq!(survey.contents.keys().collect::<Vec<_>>(), ["content/data/maps/pond.json", "content/data/props/lamp.json"], "the bytes of those sent kept");
        let steps = Mutex::new(vec![]);
        let url = block_on(hub.publish(&survey, &survey.changes, "A pond", "Round, by the path.", 99, &|s| steps.lock().unwrap().push(s))).unwrap();
        assert_eq!(url, "https://github.com/LouisMareau/battle-frontier/pull/7");
        let asked = asked.lock().unwrap();
        let posted = |path: &str| asked.iter().filter(|(m, p, _)| m == "POST" && p.ends_with(path)).map(|(_, _, b)| b.clone()).collect::<Vec<_>>();
        // the two files there, as git keeps them
        let blobs: Vec<Vec<u8>> = posted("/git/blobs").iter().map(|b| base64::engine::general_purpose::STANDARD.decode(b["content"].as_str().unwrap()).unwrap()).collect();
        assert_eq!(blobs, [b"{\n  \"id\": \"pond\"\n}\n".to_vec(), b"new\n".to_vec()]);
        let tree = &posted("/git/trees")[0];
        assert_eq!(tree["base_tree"], "t0");
        let entries: Vec<(String, Value)> = tree["tree"].as_array().unwrap().iter().map(|e| (e["path"].as_str().unwrap().to_string(), e["sha"].clone())).collect();
        assert_eq!(entries, [("content/data/maps/pond.json".to_string(), json!("b1")), ("content/data/props/lamp.json".to_string(), json!("b1")), ("content/data/props/gone.json".to_string(), Value::Null)]);
        let commit = &posted("/git/commits")[0];
        assert_eq!(
            (commit["tree"].clone(), commit["parents"].clone(), commit["message"].clone()),
            (json!("t1"), json!([base]), json!("A pond\n\nRound, by the path.\n\nSigned-off-by: Pat Player <42+player@users.noreply.github.com>")),
            "signed off as the checks ask, with GitHub's private address"
        );
        assert_eq!(posted("/git/refs")[0], json!({ "ref": "refs/heads/publish/a-pond-99", "sha": "k1" }));
        let pr = &posted("/pulls")[0];
        assert_eq!((pr["head"].clone(), pr["base"].clone(), pr["title"].clone()), (json!("player:publish/a-pond-99"), json!("main"), json!("A pond")));
        assert!(pr["body"].as_str().unwrap().contains("- removed `content/data/props/gone.json`"), "{}", pr["body"]);
        assert!(steps.lock().unwrap().iter().any(|s| s.contains("fork")));
        // nothing ticked: nothing sent
        assert!(block_on(hub.publish(&survey, &[], "A pond", "", 99, &|_| {})).unwrap_err().contains("Nothing"));
        // a public name and address, as they are; a login alone
        assert_eq!(signoff(&json!({ "login": "ann", "id": 7, "name": "Ann B", "email": "ann@example.org" })), "Ann B <ann@example.org>");
        assert_eq!(signoff(&json!({ "login": "ann", "name": "" })), "ann <ann@users.noreply.github.com>");
    }

    /// The device flow against a stand-in: the code, "not yet" once, then the token, and who
    /// it is; then the user can write to the repository: the branch is its own, no fork.
    #[test]
    fn signed_in_by_a_code_typed_then_publishing_to_the_repository_itself() {
        let polls = Arc::new(Mutex::new(0));
        let n = polls.clone();
        let base = "2222222222222222222222222222222222222222";
        let (web, asked) = stand_in(move |method, path, _| match (method, path) {
            ("POST", "/login/device/code") => (200, json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "interval": 5, "expires_in": 900 })),
            ("POST", "/login/oauth/access_token") => {
                let mut k = n.lock().unwrap();
                *k += 1;
                if *k == 1 {
                    (200, json!({ "error": "authorization_pending" }))
                } else {
                    (200, json!({ "access_token": "gho_x", "token_type": "bearer" }))
                }
            }
            ("GET", "/user") => (200, json!({ "login": "maintainer" })),
            ("GET", "/repos/LouisMareau/battle-frontier") => (200, json!({ "default_branch": "main", "permissions": { "push": true } })),
            ("GET", "/repos/LouisMareau/battle-frontier/git/ref/heads/main") => (200, json!({ "object": { "sha": base } })),
            ("GET", p) if p.ends_with(&format!("/git/commits/{base}")) => (200, json!({ "sha": base, "tree": { "sha": "t0" } })),
            ("GET", "/repos/LouisMareau/battle-frontier/git/trees/t0") => (200, json!({ "tree": [] })),
            ("POST", p) if p.starts_with("/repos/LouisMareau/battle-frontier/git/") => (201, json!({ "sha": "s1" })),
            ("POST", "/repos/LouisMareau/battle-frontier/pulls") => (201, json!({ "html_url": "https://github.com/LouisMareau/battle-frontier/pull/8" })),
            _ => (404, json!({ "message": "Not Found" })),
        });
        let hub = Hub { api: web.clone(), web, token: String::new() };
        let code = block_on(hub.device_code("client")).unwrap();
        assert_eq!((code.user_code.as_str(), code.interval), ("ABCD-1234", 5));
        assert_eq!(block_on(hub.poll("client", &code.device_code)).unwrap(), Poll::Wait);
        assert_eq!(block_on(hub.poll("client", &code.device_code)).unwrap(), Poll::Token("gho_x".into()));
        let you = Hub { token: "gho_x".into(), ..hub.clone() };
        assert_eq!(block_on(you.who()).unwrap(), "maintainer");
        // what the form said: the client, the scope; the code and the grant
        {
            let asked = asked.lock().unwrap();
            assert_eq!(asked[0].2, json!("client_id=client&scope=public_repo"));
            assert_eq!(asked[1].2, json!("client_id=client&device_code=dc&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
        }
        // no checkout: against main, a new file on the repository's own branch
        let root = folder("publish-push");
        write(&root, "content/data/maps/pond.json", b"{}\n");
        let survey = block_on(you.survey(&root)).unwrap();
        assert!(survey.on_main && survey.can_push);
        assert_eq!(survey.changes.iter().map(|c| (c.path.as_str(), c.how)).collect::<Vec<_>>(), [("content/data/maps/pond.json", How::Added)]);
        block_on(you.publish(&survey, &survey.changes, "Pond", "", 5, &|_| {})).unwrap();
        let asked = asked.lock().unwrap();
        assert!(!asked.iter().any(|(_, p, _)| p.contains("forks")), "no fork");
        let pr = asked.iter().find(|(_, p, _)| p.ends_with("/pulls")).unwrap();
        assert_eq!(pr.2["head"], "publish/pond-5", "the repository's own branch");
    }

    /// GitHub turning the sign-in's calls away for now (429: it limits the addresses the site's
    /// calls come from): the code asked for again, a little later; a poll turned away is busy
    /// (asked again later), not the end of signing in; past four tries, said so.
    #[test]
    fn signing_in_asked_again_while_github_turns_it_away() {
        let (codes, polls) = (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(0)));
        let (c, p) = (codes.clone(), polls.clone());
        let away = json!({ "code": "429", "message": "Access to this site has been restricted. Please wait a few minutes before you try again." });
        let (web, _) = stand_in(move |method, path, _| match (method, path) {
            ("POST", "/login/device/code") => {
                let mut n = c.lock().unwrap();
                *n += 1;
                if *n == 1 {
                    (429, away.clone())
                } else {
                    (200, json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "interval": 5, "expires_in": 900 }))
                }
            }
            ("POST", "/login/oauth/access_token") => {
                let mut n = p.lock().unwrap();
                *n += 1;
                if *n == 1 {
                    (429, away.clone())
                } else {
                    (200, json!({ "access_token": "gho_x" }))
                }
            }
            _ => (404, json!({ "message": "Not Found" })),
        });
        let hub = Hub { api: web.clone(), web, token: String::new() };
        let code = block_on(hub.device_code("client")).unwrap();
        assert_eq!((code.user_code.as_str(), *codes.lock().unwrap()), ("ABCD-1234", 2), "asked again once turned away");
        assert!(matches!(block_on(hub.poll("client", "dc")).unwrap(), Poll::Busy(m) if m.contains("answered 429")));
        assert_eq!(block_on(hub.poll("client", "dc")).unwrap(), Poll::Token("gho_x".into()));
        // a GitHub that never lets it through: said so, with what to do instead
        let (never, _) = stand_in(|_, _, _| (429, json!({ "message": "Access to this site has been restricted." })));
        let hub = Hub { api: never.clone(), web: never, token: String::new() };
        let e = block_on(hub.device_code("client")).unwrap_err();
        assert!(e.contains("keeps turning") && e.contains("paste a token"), "{e}");
        // a refusal that isn't for now: GitHub's words
        let (wrong, _) = stand_in(|_, _, _| (404, json!({ "error": "Not Found" })));
        let hub = Hub { api: wrong.clone(), web: wrong, token: String::new() };
        assert!(block_on(hub.device_code("nobody")).unwrap_err().contains("answered 404"));
    }

    /// Signing in by a code while GitHub turns the asking away: asked less and less often, and
    /// as before once let through (three refusals, "not yet", three more: still going, then the
    /// token); given up on after four in a row, said so with what to do instead; a token pasted
    /// while the code's waited for (it can be): that one tried, the code's waiting stopped.
    #[test]
    fn signing_in_by_a_code_while_github_turns_it_away_then_given_up() {
        let away = || (429, json!({ "message": "Access to this site has been restricted." }));
        let pending = || (200, json!({ "error": "authorization_pending" }));
        // a stand-in answering each poll in turn (the last answer kept for the ones after)
        let answering = |list: Vec<(u16, Value)>| {
            let polls = Arc::new(Mutex::new(0usize));
            let n = polls.clone();
            let (web, _) = stand_in(move |method, path, _| match (method, path) {
                ("POST", "/login/device/code") => (200, json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "interval": 1, "expires_in": 900 })),
                ("POST", "/login/oauth/access_token") => {
                    let mut k = n.lock().unwrap();
                    *k += 1;
                    list[(*k - 1).min(list.len() - 1)].clone()
                }
                ("GET", "/user") => (200, json!({ "login": "player" })),
                _ => (404, json!({ "message": "Not Found" })),
            });
            let mut p = Publisher { kept: None, api: web.clone(), web, ..Default::default() };
            p.signing = Some(Signing::default());
            (p, polls)
        };
        // the editor's frames until `done` (or a few seconds)
        let until = |p: &mut Publisher, done: &dyn Fn(&Publisher) -> bool| {
            for _ in 0..500 {
                p.take();
                if done(p) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("not done in time");
        };
        let ended = |p: &Publisher| p.user.is_some() || p.signing.as_ref().is_some_and(|s| s.error.is_some());
        let ctx = egui::Context::default();
        let (mut p, polls) = answering(vec![away(), away(), away(), pending(), away(), away(), away(), (200, json!({ "access_token": "gho_x" }))]);
        p.start_device(&ctx);
        until(&mut p, &ended);
        assert_eq!(p.user.as_ref().map(|u| u.login.as_str()), Some("player"), "{:?}", p.signing.as_ref().and_then(|s| s.error.clone()));
        assert_eq!(*polls.lock().unwrap(), 8);
        // turned away for good: given up on after four in a row, and said so
        let (mut p, polls) = answering(vec![away()]);
        p.start_device(&ctx);
        until(&mut p, &ended);
        let e = p.signing.as_ref().and_then(|s| s.error.clone()).unwrap_or_default();
        assert!(e.contains("keeps turning") && e.contains("answered 429") && e.contains("paste a token"), "{e}");
        assert_eq!(*polls.lock().unwrap(), AWAY_TIMES as usize);
        assert!(p.signing.as_ref().is_some_and(|s| s.code.is_none() && !s.busy && s.asked), "a new code can be asked for");
        // a token pasted while the code's waited for
        let (mut p, polls) = answering(vec![pending()]);
        p.start_device(&ctx);
        until(&mut p, &|p| p.signing.as_ref().is_some_and(|s| s.code.is_some()));
        assert!(!p.signing.as_ref().unwrap().busy, "nothing asked of GitHub but whether the code's been typed");
        p.signing.as_mut().unwrap().pasted = "ghp_pasted".into();
        p.start_token(&ctx);
        until(&mut p, &|p| p.user.is_some());
        let n = *polls.lock().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(*polls.lock().unwrap(), n, "the code's waiting stopped");
    }

    /// The sign-in window (a code to type) and the Publish window (a few changes in groups, one
    /// left out, a title) drawn for a look by eye, to build/shots/publish-sign-in.png and
    /// build/shots/publish.png. It needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_publish_windows() {
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        let mut shoot = |p: &mut Publisher, name: &str| {
            let mut out = None;
            for _ in 0..4 {
                let mut o = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| p.windows(ui.ctx()));
                shooter.textures(&mut o.textures_delta);
                out = Some(o);
            }
            let out = out.unwrap();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../build/shots/{name}.png"));
            shooter.shoot(&ctx, out.shapes, [1400, 900], &path).unwrap();
        };
        let mut p = Publisher { kept: None, ..Default::default() };
        p.signing = Some(Signing { code: Some(DeviceCode { device_code: "dc".into(), user_code: "WDJB-MJHT".into(), verification_uri: "https://github.com/login/device".into(), interval: 5, expires_in: 900 }), ..Default::default() });
        shoot(&mut p, "publish-sign-in");
        p.signing = None;
        p.user = Some(User { login: "player".into(), token: "t".into() });
        let change = |path: &str, how: How, size: u64, send: bool| Change { path: path.into(), how, size, send };
        p.survey = Some(Ok(Survey {
            base: "0123456789abcdef0123456789abcdef01234567".into(),
            base_tree: "t".into(),
            branch: "main".into(),
            signoff: "Pat Player <42+player@users.noreply.github.com>".into(),
            changes: vec![
                change("content/assets/models/moss_toad.glb", How::Changed, 184_320, true),
                change("content/data/creatures/moss_toad.json", How::Changed, 2_140, true),
                change("content/data/maps/test_meadow.json", How::Changed, 41_812, true),
                change("content/data/maps/pond_path.json", How::Added, 9_311, true),
                change("content/data/props/old_sign.json", How::Removed, 0, false),
            ],
            ..Default::default()
        }));
        p.open = true;
        p.title = "A pond by the path, and the moss toad's limbs".into();
        p.certify = true;
        shoot(&mut p, "publish");
    }

    #[test]
    fn a_sign_in_kept_and_forgotten_in_its_own_folder() {
        let dir = folder("kept");
        let mut p = Publisher { kept: Some(dir.join("github.json")), ..Default::default() };
        p.load();
        assert!(p.user.is_none(), "nothing kept yet");
        p.tx.send(Said::In(User { login: "player".into(), token: "t".into() })).unwrap();
        assert!(p.take() && p.user.as_ref().is_some_and(|u| u.login == "player"));
        let mut again = Publisher { kept: Some(dir.join("github.json")), ..Default::default() };
        again.load();
        assert_eq!(again.user, p.user, "read back the next run");
        again.sign_out();
        assert!(!dir.join("github.json").exists(), "forgotten");
    }

    /// What the browser keeps besides records goes to Publish only when it's the game's own:
    /// never temporary/'s, nor the model requests' folders.
    #[test]
    fn only_the_games_own_files_are_published() {
        assert!(published("content/assets/models/moss_toad.glb") && published("content/data/maps/meadow/0_1.json"));
        assert!(!published("temporary/assets/models/moss_toad.glb"), "a placeholder's model");
        assert!(!published("private/requests/moss_toad/front.png"), "a request's picture");
        assert!(!published("contents/assets/x.glb") && !published("content"));
    }
}
