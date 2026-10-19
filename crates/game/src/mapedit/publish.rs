//! Publishing: what's been made in the editor sent to the game's repository on GitHub as a
//! pull request, for its maintainers to look at before it's merged (only those who can write
//! to the repository can merge one: nothing gets into the game unreviewed). The top bar's
//! "Sign in" signs in with GitHub, by its device flow (a code typed on github.com, once: the
//! repository's OAuth App, `CLIENT_ID`) or with a token pasted; "Publish" waits for it, then
//! lists every file of content/ changed since the project's copy was taken (the commit its
//! git checkout is on, else the repository's main), each to send or leave out, under a title
//! and a description. Sent, it's a pull request from the user's fork (made when they can't
//! write to the repository themselves): the files' blobs, a tree on the copy's, a commit, a
//! branch, the pull request. The names check (tools/check-names.py) reads the files first,
//! when Python is there. The desktop only: the browser can't read the project's folder.
// (in the browser only the buttons are: the rest is the desktop's)
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText};

use super::{hint, mark, warn, MapEditor};
use crate::hud::TEXT;

/// The game's repository on GitHub.
pub(super) const OWNER: &str = "LouisMareau";
pub(super) const REPO: &str = "battle-frontier";
/// The editor's OAuth App on GitHub (its device flow on), whose client id is public: empty
/// until the repository's owner registers one (README, "Publishing"); GITHUB_CLIENT_ID
/// overrides it. Without one, a token pasted signs in.
const CLIENT_ID: &str = "";
/// What signing in asks for: forks, branches and pull requests on public repositories.
const SCOPE: &str = "public_repo";
/// What's published: the game's own content (never temporary/, never private/).
const PUBLISHED: &str = "content";

/// The OAuth App's client id: GITHUB_CLIENT_ID, else the one built in (empty: none yet).
fn client_id() -> String {
    std::env::var("GITHUB_CLIENT_ID").ok().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| CLIENT_ID.to_string())
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
fn binary(path: &str, bytes: &[u8]) -> bool {
    let lower = path.to_lowercase();
    [".glb", ".png", ".jpg", ".jpeg", ".webp", ".wav", ".ogg", ".mp3", ".blend"].iter().any(|e| lower.ends_with(e)) || bytes.iter().take(8000).any(|&b| b == 0)
}

/// The bytes git keeps for a file: text with its line ends LF (what a Windows checkout's CRLF
/// go back to when committed), anything else as it is.
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
/// main, the folder being no checkout of one of its commits), the repository's main branch,
/// whether the user can write to it, what's changed, and who signs the commit off ("Name
/// <email>", GitHub's private address when theirs isn't public).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Survey {
    pub base: String,
    pub base_tree: String,
    pub on_main: bool,
    pub branch: String,
    pub can_push: bool,
    pub changes: Vec<Change>,
    pub signoff: String,
}

/// Who signs a commit off, from GitHub's account: their name (else login) and their public
/// address (else the private one GitHub gives each account).
pub(super) fn signoff(user: &serde_json::Value) -> String {
    let text = |k: &str| user.get(k).and_then(serde_json::Value::as_str).filter(|v| !v.trim().is_empty()).map(str::to_string);
    let login = text("login").unwrap_or_default();
    let email = text("email").unwrap_or_else(|| match user.get("id").and_then(serde_json::Value::as_u64) {
        Some(id) => format!("{id}+{login}@users.noreply.github.com"),
        None => format!("{login}@users.noreply.github.com"),
    });
    format!("{} <{email}>", text("name").unwrap_or_else(|| login.clone()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) use desk::*;

#[cfg(not(target_arch = "wasm32"))]
mod desk {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::{Duration, Instant};

    use base64::Engine as _;
    use serde_json::{json, Value};

    use super::{blob_id, branch_name, changes, client_id, committed, head_commit, local_files, How, Survey, User, OWNER, PUBLISHED, REPO, SCOPE};

    /// GitHub, as the signed-in user (`token`): its API (`api`) and its site (`web`, where
    /// signing in goes), stand-ins in the tests.
    #[derive(Clone, Debug)]
    pub(in super::super) struct Hub {
        pub api: String,
        pub web: String,
        pub token: String,
    }

    /// The device flow's first step: the code the user types on github.com, where, how often
    /// to ask whether it's been typed and how long it lasts (seconds).
    #[derive(Clone, Debug, PartialEq)]
    pub(in super::super) struct DeviceCode {
        pub device_code: String,
        pub user_code: String,
        pub verification_uri: String,
        pub interval: u64,
        pub expires_in: u64,
    }

    /// Whether the code's been typed: the token, not yet, or not so often.
    #[derive(Clone, Debug, PartialEq)]
    pub(in super::super) enum Poll {
        Token(String),
        Wait,
        SlowDown,
    }

    /// A form's value, percent-encoded.
    fn encode(v: &str) -> String {
        v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    }

    impl Hub {
        fn send(&self, req: &mut ehttp::Request, what: &str) -> Result<Value, String> {
            req.headers.insert("User-Agent", "battle-frontier-editor");
            req.timeout = Some(Duration::from_secs(90));
            let r = ehttp::fetch_blocking(req).map_err(|e| format!("GitHub can't be reached ({e})"))?;
            let body: Value = serde_json::from_slice(&r.bytes).unwrap_or(Value::Null);
            if r.ok {
                return Ok(body);
            }
            let message = body.get("message").and_then(Value::as_str).unwrap_or(&r.status_text).to_string();
            Err(match r.status {
                401 => "GitHub doesn't take the sign-in any more: sign in again".to_string(),
                _ => format!("GitHub answered {} to {what}: {message}", r.status),
            })
        }

        /// A call to the API: its answer, or what went wrong (GitHub's own words).
        fn call(&self, method: ehttp::Method, path: &str, body: Option<Value>) -> Result<Value, String> {
            let what = format!("{method:?} {path}");
            let mut req = ehttp::Request::new(method, format!("{}{path}", self.api), ehttp::Headers::new(&[("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28")]));
            if !self.token.is_empty() {
                req.headers.insert("Authorization", format!("Bearer {}", self.token));
            }
            if let Some(b) = body {
                req.body = serde_json::to_vec(&b).map_err(|e| e.to_string())?;
                req.headers.insert("Content-Type", "application/json");
            }
            self.send(&mut req, &what)
        }

        fn get(&self, path: &str) -> Result<Value, String> {
            self.call(ehttp::Method::GET, path, None)
        }

        fn post(&self, path: &str, body: Value) -> Result<Value, String> {
            self.call(ehttp::Method::POST, path, Some(body))
        }

        /// A form posted to the site (signing in), its answer in JSON.
        fn form(&self, path: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
            let body = fields.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect::<Vec<_>>().join("&");
            let mut req = ehttp::Request::post(format!("{}{path}", self.web), body.into_bytes());
            req.headers = ehttp::Headers::new(&[("Accept", "application/json"), ("Content-Type", "application/x-www-form-urlencoded")]);
            self.send(&mut req, path)
        }

        /// Signing in: the code to type on github.com.
        pub fn device_code(&self, client: &str) -> Result<DeviceCode, String> {
            let v = self.form("/login/device/code", &[("client_id", client), ("scope", SCOPE)])?;
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
        pub fn poll(&self, client: &str, device_code: &str) -> Result<Poll, String> {
            let v = self.form("/login/oauth/access_token", &[("client_id", client), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")])?;
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
        pub fn who(&self) -> Result<String, String> {
            let v = self.get("/user")?;
            v.get("login").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "GitHub didn't say who you are".into())
        }

        /// What a publish starts from (`root`: the project's folder): the repository, the copy's
        /// commit (the checkout's, when GitHub has it; else main), its content/ and what's
        /// changed in the folder since. Removed files are left unticked when the copy's
        /// commit isn't known (they may be new on main rather than gone here).
        pub fn survey(&self, root: &Path) -> Result<Survey, String> {
            let signoff = super::signoff(&self.get("/user")?);
            let repo = self.get(&format!("/repos/{OWNER}/{REPO}"))?;
            let branch = repo.get("default_branch").and_then(Value::as_str).unwrap_or("main").to_string();
            let can_push = repo.pointer("/permissions/push").and_then(Value::as_bool).unwrap_or(false);
            let commit = |sha: &str| self.get(&format!("/repos/{OWNER}/{REPO}/git/commits/{sha}"));
            let (base, commit, on_main) = match head_commit(root).and_then(|sha| commit(&sha).ok().map(|c| (sha, c))) {
                Some((sha, c)) => (sha, c, false),
                None => {
                    let r = self.get(&format!("/repos/{OWNER}/{REPO}/git/ref/heads/{branch}"))?;
                    let sha = r.pointer("/object/sha").and_then(Value::as_str).ok_or("GitHub didn't say where main is")?.to_string();
                    let c = commit(&sha)?;
                    (sha, c, true)
                }
            };
            let base_tree = commit.pointer("/tree/sha").and_then(Value::as_str).ok_or("GitHub didn't give the commit's tree")?.to_string();
            // the copy's content/ (its own tree, read whole)
            let top = self.get(&format!("/repos/{OWNER}/{REPO}/git/trees/{base_tree}"))?;
            let content = top.get("tree").and_then(Value::as_array).and_then(|t| t.iter().find(|e| e["path"] == PUBLISHED && e["type"] == "tree")).and_then(|e| e["sha"].as_str()).map(str::to_string);
            let mut tree = BTreeMap::new();
            if let Some(sha) = content {
                let t = self.get(&format!("/repos/{OWNER}/{REPO}/git/trees/{sha}?recursive=1"))?;
                if t.get("truncated").and_then(Value::as_bool) == Some(true) {
                    return Err("content/ on GitHub is too big to read in one go".into());
                }
                for e in t.get("tree").and_then(Value::as_array).into_iter().flatten().filter(|e| e["type"] == "blob") {
                    if let (Some(p), Some(s)) = (e["path"].as_str(), e["sha"].as_str()) {
                        tree.insert(format!("{PUBLISHED}/{p}"), s.to_string());
                    }
                }
            }
            let mut local = BTreeMap::new();
            for (rel, path) in local_files(root) {
                let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let raw = blob_id(&bytes);
                let kept = committed(&rel, bytes);
                local.insert(rel, (blob_id(&kept), raw, kept.len() as u64));
            }
            let mut found = changes(&tree, &local);
            if on_main {
                for c in found.iter_mut().filter(|c| c.how == How::Removed) {
                    c.send = false;
                }
            }
            Ok(Survey { base, base_tree, on_main, branch, can_push, changes: found, signoff })
        }

        /// The pull request: the files ticked (`sent`) on the copy's tree, committed on a branch
        /// of the repository's (the user can write to it) or of their fork's (made, and brought
        /// up to date, when they can't), opened against main. `step` says how far it's got.
        /// Its address.
        #[allow(clippy::too_many_arguments)]
        pub fn publish(&self, root: &Path, survey: &Survey, sent: &[super::Change], title: &str, body: &str, secs: u64, step: &dyn Fn(String)) -> Result<String, String> {
            if sent.is_empty() {
                return Err("Nothing ticked to send".into());
            }
            step("checking the names in the files".into());
            check_names(root, sent)?;
            // where the branch goes
            let (head_repo, head_owner) = if survey.can_push {
                (format!("{OWNER}/{REPO}"), None)
            } else {
                step("making your fork of the repository (or finding it)".into());
                let fork = self.post(&format!("/repos/{OWNER}/{REPO}/forks"), json!({}))?;
                let name = fork.get("full_name").and_then(Value::as_str).ok_or("GitHub didn't say where your fork is")?.to_string();
                let owner = fork.pointer("/owner/login").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| name.split('/').next().unwrap_or_default().to_string());
                // a new fork takes a moment to be there
                let until = Instant::now() + Duration::from_secs(90);
                while self.get(&format!("/repos/{name}/git/ref/heads/{}", survey.branch)).is_err() {
                    if Instant::now() > until {
                        return Err(format!("Your fork {name} isn't ready yet: publish again in a minute"));
                    }
                    std::thread::sleep(Duration::from_millis(1500));
                }
                // brought up to date (an old fork hasn't the copy's commit); a fork whose main
                // went its own way stays as it is
                let _ = self.post(&format!("/repos/{name}/merge-upstream"), json!({ "branch": survey.branch }));
                if self.get(&format!("/repos/{name}/git/commits/{}", survey.base)).is_err() {
                    return Err(format!("Your fork {name} hasn't the commit your copy is on ({}): bring its main up to date with the repository's on GitHub, then publish again", &survey.base[..7]));
                }
                (name, Some(owner))
            };
            let mut entries = vec![];
            let to_send: Vec<&super::Change> = sent.iter().filter(|c| c.how != How::Removed).collect();
            for (k, c) in to_send.iter().enumerate() {
                step(format!("sending {} ({} of {})", c.path, k + 1, to_send.len()));
                let bytes = std::fs::read(root.join(&c.path)).map_err(|e| format!("{}: {e}", c.path))?;
                let bytes = committed(&c.path, bytes);
                let blob = self.post(&format!("/repos/{head_repo}/git/blobs"), json!({ "content": base64::engine::general_purpose::STANDARD.encode(&bytes), "encoding": "base64" }))?;
                let sha = blob.get("sha").and_then(Value::as_str).ok_or("GitHub didn't keep a file")?;
                entries.push(json!({ "path": c.path, "mode": "100644", "type": "blob", "sha": sha }));
            }
            for c in sent.iter().filter(|c| c.how == How::Removed) {
                entries.push(json!({ "path": c.path, "mode": "100644", "type": "blob", "sha": Value::Null }));
            }
            step("putting the files together".into());
            let tree = self.post(&format!("/repos/{head_repo}/git/trees"), json!({ "base_tree": survey.base_tree, "tree": entries }))?;
            let tree = tree.get("sha").and_then(Value::as_str).ok_or("GitHub didn't make the tree")?.to_string();
            // signed off as the checks ask (CONTRIBUTING.md): the user ticked that they certify it
            let message = if body.trim().is_empty() { title.trim().to_string() } else { format!("{}\n\n{}", title.trim(), body.trim()) };
            let message = format!("{message}\n\nSigned-off-by: {}", survey.signoff);
            let commit = self.post(&format!("/repos/{head_repo}/git/commits"), json!({ "message": message, "tree": tree, "parents": [survey.base] }))?;
            let commit = commit.get("sha").and_then(Value::as_str).ok_or("GitHub didn't make the commit")?.to_string();
            let branch = branch_name(title, secs);
            step(format!("making the branch {branch}"));
            self.post(&format!("/repos/{head_repo}/git/refs"), json!({ "ref": format!("refs/heads/{branch}"), "sha": commit }))?;
            step("opening the pull request".into());
            let listed: Vec<String> = sent.iter().map(|c| format!("- {} `{}`", match c.how { How::Added => "added", How::Changed => "changed", How::Removed => "removed" }, c.path)).collect();
            let text = format!("{}\n\nPublished from the Battle Frontier editor.\n\n{}", body.trim(), listed.join("\n"));
            let head = match &head_owner {
                Some(owner) => format!("{owner}:{branch}"),
                None => branch.clone(),
            };
            let pr = self.post(&format!("/repos/{OWNER}/{REPO}/pulls"), json!({ "title": title.trim(), "body": text.trim(), "head": head, "base": survey.branch, "maintainer_can_modify": true }))?;
            pr.get("html_url").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "GitHub didn't say where the pull request is".into())
        }
    }

    /// Python: PYTHON names it, else `python` (or `python3`) on the PATH (never the Store's
    /// stand-ins), else Windows' `py`.
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
    fn check_names(root: &Path, sent: &[super::Change]) -> Result<(), String> {
        let tool = root.join("tools").join("check-names.py");
        let files: Vec<&str> = sent.iter().filter(|c| c.how != How::Removed && !super::binary(&c.path, &[])).map(|c| c.path.as_str()).collect();
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

    /// Where the sign-in is kept: the user's own settings folder (not the project's).
    pub(in super::super) fn kept_path() -> Option<PathBuf> {
        let dir = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        dir.map(|d| d.join("BattleFrontier").join("github.json"))
    }

    /// What the threads say.
    pub(in super::super) enum Said {
        Code(DeviceCode),
        In(User),
        NotIn(String),
        Surveyed(Result<Survey, String>),
        Step(String),
        Opened(Result<String, String>),
    }

    /// Signing in: the code to type (once it's come), a token pasted instead, what went wrong.
    #[derive(Default)]
    pub(in super::super) struct Signing {
        pub code: Option<DeviceCode>,
        pub pasted: String,
        pub error: Option<String>,
        pub busy: bool,
        pub stop: Arc<AtomicBool>,
    }

    /// Signing in and publishing.
    pub(in super::super) struct Publisher {
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
        /// where the sign-in is kept (None: nowhere), read once
        pub kept: Option<PathBuf>,
        /// the folder published from (None: the project's)
        pub root: Option<PathBuf>,
        pub(in super::super) loaded: bool,
        /// GitHub (stand-ins in the tests)
        pub api: String,
        pub web: String,
        /// what the threads say, and how they say it
        pub(in super::super) tx: mpsc::Sender<Said>,
        pub(in super::super) rx: mpsc::Receiver<Said>,
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
                kept: kept_path(),
                root: None,
                loaded: false,
                api: "https://api.github.com".into(),
                web: "https://github.com".into(),
                tx,
                rx,
            }
        }
    }

    impl Publisher {
        /// The folder published from.
        pub fn root(&self) -> PathBuf {
            self.root.clone().unwrap_or_else(super::project_root)
        }

        fn hub(&self, token: &str) -> Hub {
            Hub { api: self.api.clone(), web: self.web.clone(), token: token.into() }
        }

        /// The kept sign-in, read the first time it's asked for.
        pub fn load(&mut self) {
            if std::mem::replace(&mut self.loaded, true) {
                return;
            }
            let read = self.kept.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<User>(&t).ok());
            if let Some(u) = read.filter(|u| !u.token.is_empty()) {
                self.user = Some(u);
            }
        }

        /// What the threads said since: true when anything did (the window drawn again).
        pub fn take(&mut self) -> bool {
            let mut any = false;
            while let Ok(said) = self.rx.try_recv() {
                any = true;
                match said {
                    Said::Code(c) => {
                        if let Some(s) = &mut self.signing {
                            s.code = Some(c);
                        }
                    }
                    Said::In(u) => {
                        if let Some(p) = &self.kept {
                            let written = p.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|()| std::fs::write(p, serde_json::to_string(&u).unwrap_or_default()));
                            if let Err(e) = written {
                                log::warn!("the sign-in isn't kept ({}): {e}", p.display());
                            }
                        }
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
        /// been typed until it has (or it's stopped, or out of time).
        pub fn start_device(&mut self, ctx: &egui::Context) {
            let client = client_id();
            let s = self.signing.get_or_insert_with(Default::default);
            if client.is_empty() {
                s.error = Some("Signing in on github.com isn't set up for this project yet (README, \"Publishing\"): paste a token instead".into());
                return;
            }
            s.stop.store(true, Ordering::Relaxed);
            let stop = Arc::new(AtomicBool::new(false));
            (s.stop, s.busy, s.error, s.code) = (stop.clone(), true, None, None);
            let (hub, tx, ctx) = (self.hub(""), self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let said = |s: Said| {
                    let _ = tx.send(s);
                    ctx.request_repaint();
                };
                let code = match hub.device_code(&client) {
                    Ok(c) => c,
                    Err(e) => return said(Said::NotIn(e)),
                };
                said(Said::Code(code.clone()));
                let mut wait = code.interval.max(1);
                let until = Instant::now() + Duration::from_secs(code.expires_in);
                loop {
                    for _ in 0..wait * 10 {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    if Instant::now() > until {
                        return said(Said::NotIn("The code ran out of time: sign in again".into()));
                    }
                    match hub.poll(&client, &code.device_code) {
                        Ok(Poll::Token(t)) => {
                            let you = Hub { token: t.clone(), ..hub.clone() };
                            return said(match you.who() {
                                Ok(login) => Said::In(User { login, token: t }),
                                Err(e) => Said::NotIn(e),
                            });
                        }
                        Ok(Poll::Wait) => {}
                        Ok(Poll::SlowDown) => wait += 5,
                        Err(e) => return said(Said::NotIn(e)),
                    }
                }
            });
        }

        /// Signing in with a token pasted: who it is, asked of GitHub.
        pub fn start_token(&mut self, ctx: &egui::Context) {
            let Some(s) = &mut self.signing else { return };
            let token = s.pasted.trim().to_string();
            if token.is_empty() {
                return;
            }
            (s.busy, s.error) = (true, None);
            let (hub, tx, ctx) = (self.hub(&token), self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let said = match hub.who() {
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
            if let Some(p) = &self.kept {
                let _ = std::fs::remove_file(p);
            }
        }

        /// The Publish window opened: what's changed, looked for.
        pub fn open_window(&mut self, root: PathBuf, ctx: &egui::Context) {
            let Some(user) = &self.user else { return };
            (self.open, self.survey, self.opened, self.step, self.certify) = (true, None, None, None, false);
            let (hub, tx, ctx) = (self.hub(&user.token), self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Said::Surveyed(hub.survey(&root)));
                ctx.request_repaint();
            });
        }

        /// The pull request asked for: the files ticked sent, on its own thread.
        pub fn start_publish(&mut self, root: PathBuf, ctx: &egui::Context) {
            let (Some(user), Some(Ok(survey))) = (&self.user, &self.survey) else { return };
            let sent: Vec<super::Change> = survey.changes.iter().filter(|c| c.send).cloned().collect();
            let (hub, survey, tx, ctx) = (self.hub(&user.token), survey.clone(), self.tx.clone(), ctx.clone());
            let (title, body) = (self.title.clone(), self.body.clone());
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            self.step = Some("starting".into());
            self.opened = None;
            std::thread::spawn(move || {
                let step = |s: String| {
                    let _ = tx.send(Said::Step(s));
                    ctx.request_repaint();
                };
                let r = hub.publish(&root, &survey, &sent, &title, &body, secs, &step);
                let _ = tx.send(Said::Opened(r));
                ctx.request_repaint();
            });
        }
    }
}

/// The project's folder (GAME_ROOT, else where the game runs).
#[cfg(not(target_arch = "wasm32"))]
fn project_root() -> PathBuf {
    let root = PathBuf::from(std::env::var("GAME_ROOT").unwrap_or_else(|_| ".".into()));
    root.canonicalize().unwrap_or(root)
}

/// The browser's stand-in: nothing to sign in to (it can't read the project's folder).
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub(super) struct Publisher;

/// A change's mark in the list.
fn badge(ui: &mut egui::Ui, how: How) {
    let (text, color) = match how {
        How::Added => ("added", Color32::from_rgb(110, 200, 120)),
        How::Changed => ("changed", Color32::from_rgb(120, 170, 235)),
        How::Removed => ("removed", Color32::from_rgb(235, 140, 110)),
    };
    ui.label(RichText::new(text).size(11.0).color(color));
}

/// A size, short.
fn size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

impl MapEditor {
    /// The top bar's Sign in (or who's signed in, to sign out) and Publish (waiting for it).
    #[cfg(target_arch = "wasm32")]
    pub(super) fn publish_buttons(&mut self, ui: &mut egui::Ui) {
        let _ = &self.publisher;
        let why = "Publishing needs the desktop app: the browser can't read the project's folder";
        let s = ui.add_enabled(false, egui::Button::new("Sign in")).on_disabled_hover_text(why);
        mark("uiSignIn", &s);
        let p = ui.add_enabled(false, egui::Button::new("Publish")).on_disabled_hover_text(why);
        mark("uiPublish", &p);
        crate::platform::set_hook("signedin", "");
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn publish_window(&mut self, _ctx: &egui::Context) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn publish_buttons(&mut self, ui: &mut egui::Ui) {
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
                    p.start_device(&ctx);
                }
            }
        }
        let signed = p.user.is_some();
        let b = ui
            .add_enabled(signed, egui::Button::new("Publish"))
            .on_hover_text("Every file of content/ you've changed, to look over and send as a pull request: the maintainers look at it before it's in the game")
            .on_disabled_hover_text("Sign in with GitHub first: the pull request is opened as you");
        mark("uiPublish", &b);
        if b.clicked() {
            p.open_window(p.root(), &ctx);
        }
        crate::platform::set_hook("signedin", p.user.as_ref().map(|u| u.login.as_str()).unwrap_or(""));
    }

    /// The sign-in window and the Publish window, when they're up.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn publish_window(&mut self, ctx: &egui::Context) {
        self.publisher.windows(ctx);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Publisher {
    /// The sign-in window and the Publish window, when they're up.
    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        self.sign_in_window(ctx);
        let p = self;
        crate::platform::set_hook("publish", if p.open { "open" } else { "" });
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
            ui.add_space(6.0);
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
                    ui.label(RichText::new(format!("Nothing's changed in content/ since your copy ({}).", &s.base[..7.min(s.base.len())])).color(TEXT));
                }
                Some(Ok(s)) => {
                    if s.on_main {
                        warn(ui, "Your folder isn't a git checkout of one of the repository's commits: it's compared with main, so what's new there since your copy shows too (removed files are left unticked).");
                    }
                    let sending = s.changes.iter().filter(|c| c.send).count();
                    hint(ui, &format!("{} file{} changed since your copy ({}), {sending} ticked to send:", s.changes.len(), if s.changes.len() == 1 { "" } else { "s" }, &s.base[..7.min(s.base.len())]));
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
                    ui.hyperlink_to(url.as_str(), url.as_str());
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
                    p.start_publish(p.root(), ctx);
                }
                if matches!(p.survey, Some(Err(_))) {
                    let again = ui.button("Look again");
                    mark("uiPublishAgain", &again);
                    if again.clicked() {
                        p.open_window(p.root(), ctx);
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

    /// Signing in: the code to type on github.com (and a button to go there), or a token to
    /// paste; what went wrong.
    fn sign_in_window(&mut self, ctx: &egui::Context) {
        let p = self;
        crate::platform::set_hook("signing", if p.signing.is_some() { "open" } else { "" });
        let Some(s) = &mut p.signing else { return };
        let mut stop = false;
        let mut paste = false;
        let mut again = false;
        let modal = egui::Modal::new(egui::Id::new("editor-sign-in")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Sign in with GitHub").size(20.0).color(TEXT));
            ui.add_space(6.0);
            match &s.code {
                Some(code) => {
                    ui.label(RichText::new("Type this code on GitHub's page (an account there is needed):").color(TEXT));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&code.user_code).monospace().size(24.0).strong().color(TEXT));
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(code.user_code.clone());
                        }
                    });
                    ui.hyperlink_to(format!("Open {}", code.verification_uri), &code.verification_uri);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        hint(ui, "Waiting for GitHub to let you in…");
                    });
                }
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
                if client_id().is_empty() {
                    hint(ui, "A classic token with the public_repo scope (github.com/settings/tokens) works too.");
                } else {
                    let a = ui.button("Ask for a new code");
                    mark("uiSignInAgain", &a);
                    again = a.clicked();
                }
            }
            ui.add_space(8.0);
            ui.collapsing("Or paste a token", |ui| {
                hint(ui, "A token of yours (github.com/settings/tokens, with the public_repo scope): it's kept in your own settings folder, never in the project.");
                ui.horizontal(|ui| {
                    let t = ui.add(egui::TextEdit::singleline(&mut s.pasted).password(true).desired_width(260.0).hint_text("ghp_…"));
                    mark("uiSignInToken", &t);
                    let go = ui.add_enabled(!s.pasted.trim().is_empty() && !s.busy, egui::Button::new("Sign in"));
                    mark("uiSignInPaste", &go);
                    paste = go.clicked();
                });
            });
            ui.add_space(6.0);
            let c = ui.button("Cancel");
            mark("uiSignInCancel", &c);
            stop = c.clicked();
        });
        if paste {
            p.start_token(ctx);
        } else if again {
            p.start_device(ctx);
        }
        if stop || modal.should_close() {
            p.stop_signing();
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use base64::Engine as _;
    use serde_json::{json, Value};

    use super::*;

    type Asked = Arc<Mutex<Vec<(String, String, Value)>>>;

    /// A stand-in for GitHub on this machine: each request answered by `answer` (its status
    /// and JSON), and kept (its method, path and body: JSON, or a form as a string).
    fn stand_in(answer: impl Fn(&str, &str, &Value) -> (u16, Value) + Send + Sync + 'static) -> (String, Asked) {
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
        let survey = hub.survey(&root).unwrap();
        assert_eq!((survey.base.as_str(), survey.base_tree.as_str(), survey.on_main, survey.can_push), (base, "t0", false, false));
        assert_eq!(survey.signoff, "Pat Player <42+player@users.noreply.github.com>");
        let found: Vec<(&str, How, bool)> = survey.changes.iter().map(|c| (c.path.as_str(), c.how, c.send)).collect();
        assert_eq!(
            found,
            [("content/data/maps/pond.json", How::Added, true), ("content/data/props/gone.json", How::Removed, true), ("content/data/props/lamp.json", How::Changed, true)]
        );
        assert_eq!(survey.changes[0].size, 19, "sent with LF line ends");
        let steps = Mutex::new(vec![]);
        let url = hub.publish(&root, &survey, &survey.changes, "A pond", "Round, by the path.", 99, &|s| steps.lock().unwrap().push(s)).unwrap();
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
        assert!(hub.publish(&root, &survey, &[], "A pond", "", 99, &|_| {}).unwrap_err().contains("Nothing"));
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
        let code = hub.device_code("client").unwrap();
        assert_eq!((code.user_code.as_str(), code.interval), ("ABCD-1234", 5));
        assert_eq!(hub.poll("client", &code.device_code).unwrap(), Poll::Wait);
        assert_eq!(hub.poll("client", &code.device_code).unwrap(), Poll::Token("gho_x".into()));
        let you = Hub { token: "gho_x".into(), ..hub.clone() };
        assert_eq!(you.who().unwrap(), "maintainer");
        // what the form said: the client, the scope; the code and the grant
        {
            let asked = asked.lock().unwrap();
            assert_eq!(asked[0].2, json!("client_id=client&scope=public_repo"));
            assert_eq!(asked[1].2, json!("client_id=client&device_code=dc&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
        }
        // no checkout: against main, a new file on the repository's own branch
        let root = folder("publish-push");
        write(&root, "content/data/maps/pond.json", b"{}\n");
        let survey = you.survey(&root).unwrap();
        assert!(survey.on_main && survey.can_push);
        assert_eq!(survey.changes.iter().map(|c| (c.path.as_str(), c.how)).collect::<Vec<_>>(), [("content/data/maps/pond.json", How::Added)]);
        you.publish(&root, &survey, &survey.changes, "Pond", "", 5, &|_| {}).unwrap();
        let asked = asked.lock().unwrap();
        assert!(!asked.iter().any(|(_, p, _)| p.contains("forks")), "no fork");
        let pr = asked.iter().find(|(_, p, _)| p.ends_with("/pulls")).unwrap();
        assert_eq!(pr.2["head"], "publish/pond-5", "the repository's own branch");
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
            on_main: false,
            branch: "main".into(),
            can_push: false,
            signoff: "Pat Player <42+player@users.noreply.github.com>".into(),
            changes: vec![
                change("content/assets/models/moss_toad.glb", How::Changed, 184_320, true),
                change("content/data/creatures/moss_toad.json", How::Changed, 2_140, true),
                change("content/data/maps/test_meadow.json", How::Changed, 41_812, true),
                change("content/data/maps/pond_path.json", How::Added, 9_311, true),
                change("content/data/props/old_sign.json", How::Removed, 0, false),
            ],
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
}
