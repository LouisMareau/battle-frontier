//! Reviewing a pull request in the editor. A collaborator (write access on the repository, as
//! GitHub says) picks one of the open pull requests in the Review window; the editor fetches
//! what it changes (`content::diff::Review`: its files, each record's fields) and the game's
//! files at its head, and opens its content as it would be once merged — in an environment of
//! its own: its own content and files, nothing saved or kept, the reviewer's own editor parked
//! meanwhile and back as it was after (`Leave::Review`, `Leave::EndReview`). Every change shows
//! where it is: a dot on each panel's button that holds one (`review_dots`), a badge on each
//! asset and record, a mark by each section and field that changed (`change_mark`), the code
//! it changes listed by area with its diffs; and **Review by Claude** asks Claude for a detailed
//! list of the changes (the Messages API on the user's own key, through the site's relay in the
//! browser), shown in a window of its own.

use std::collections::BTreeMap;
use std::sync::mpsc;

use content::diff::{fields, place_of, Counts, FileChange, Kind, Place, Review};
use content::Content;
use egui::{Color32, RichText};
use serde_json::{json, Value};

use super::publish::{spawn, Hub, OWNER, REPO};
use super::{dock, hint, mark, warn, MapEditor};

/// The Assets panel's folder of what the pull request changes (`assets::REVIEWED`).
#[cfg(test)]
const REVIEWED_FOLDER: &str = "#review";

/// The marks' colours: added, modified, removed (as the Publish window's).
const ADDED: Color32 = Color32::from_rgb(110, 200, 120);
const MODIFIED: Color32 = Color32::from_rgb(120, 170, 235);
const REMOVED: Color32 = Color32::from_rgb(235, 140, 110);

/// A change's colour.
pub(super) fn colour(kind: Kind) -> Color32 {
    match kind {
        Kind::Added => ADDED,
        Kind::Removed => REMOVED,
        _ => MODIFIED,
    }
}

/// A pull request open on the repository, as GitHub lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pull {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub branch: String,
    /// its branch's commit, and the commit it's against
    pub head: String,
    pub base: String,
    pub url: String,
    pub files: u64,
    pub draft: bool,
}

/// A pull request from GitHub's record of one.
pub(super) fn pull_of(v: &Value) -> Option<Pull> {
    let s = |p: &str| v.pointer(p).and_then(Value::as_str).unwrap_or("").to_string();
    Some(Pull {
        number: v.get("number")?.as_u64()?,
        title: s("/title"),
        author: s("/user/login"),
        branch: s("/head/ref"),
        head: s("/head/sha"),
        base: s("/base/sha"),
        url: s("/html_url"),
        files: v.get("changed_files").and_then(Value::as_u64).unwrap_or(0),
        draft: v.get("draft").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The pull requests GitHub lists.
pub(super) fn pulls_of(v: &Value) -> Vec<Pull> {
    v.as_array().into_iter().flatten().filter_map(pull_of).collect()
}

/// A changed file from GitHub's record of one (a pull request's files).
pub(super) fn file_of(v: &Value) -> Option<FileChange> {
    Some(FileChange {
        path: v.get("filename")?.as_str()?.to_string(),
        kind: Kind::of_status(v.get("status").and_then(Value::as_str).unwrap_or("modified")),
        from: v.get("previous_filename").and_then(Value::as_str).map(str::to_string),
        additions: v.get("additions").and_then(Value::as_u64).unwrap_or(0) as u32,
        deletions: v.get("deletions").and_then(Value::as_u64).unwrap_or(0) as u32,
        patch: v.get("patch").and_then(Value::as_str).map(str::to_string),
    })
}

/// Whether a permission on the repository (GitHub's word) lets its holder review: those who
/// can merge, not anyone who can read it.
pub(super) fn may_review(permission: &str) -> bool {
    matches!(permission, "admin" | "maintain" | "write")
}

/// Whether a changed file is one of the game's (a record, a map's square, an asset): fetched
/// for the review's content.
fn of_the_game(path: &str) -> bool {
    matches!(place_of(path), Place::Record { .. } | Place::Asset { .. })
}

/// A pull request fetched whole: what it is, what it changes, and the game's files as its head
/// has them (None: taken out), by path in the repository.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opened {
    pub pull: Pull,
    pub review: Review,
    pub heads: BTreeMap<String, Option<Vec<u8>>>,
}

impl Opened {
    /// From what GitHub gave: the pull request, its files, the game's files at its head and
    /// the records as they were at its base (for their fields' changes; a map's square's under
    /// the square's name).
    pub(super) fn build(pull: Pull, files: Vec<FileChange>, heads: BTreeMap<String, Option<Vec<u8>>>, bases: BTreeMap<String, Vec<u8>>) -> Opened {
        let mut review = Review {
            number: pull.number,
            title: pull.title.clone(),
            author: pull.author.clone(),
            head: pull.head.clone(),
            base: pull.base.clone(),
            url: pull.url.clone(),
            files,
            ..Default::default()
        };
        let json = |b: Option<&Vec<u8>>| b.and_then(|b| serde_json::from_slice::<Value>(b).ok()).unwrap_or(Value::Null);
        for f in &review.files {
            let Place::Record { collection, id } = f.place() else { continue };
            let key = format!("{collection}/{id}");
            let before = json(bases.get(&f.path));
            let after = json(heads.get(&f.path).and_then(|o| o.as_ref()));
            let square = (f.path.matches('/').count() == 4).then(|| f.path.rsplit('/').next().unwrap_or("").trim_end_matches(".json").to_string());
            let mut changes = if before.is_null() || after.is_null() { vec![] } else { fields(&before, &after) };
            if let Some(sq) = &square {
                for c in &mut changes {
                    c.path = if c.path.is_empty() { format!("squares[{sq}]") } else { format!("squares[{sq}].{}", c.path) };
                }
            }
            review.fields.entry(key.clone()).or_default().extend(changes);
            let name = after.get("name").or_else(|| before.get("name")).and_then(Value::as_str).map(str::to_string);
            if let Some(name) = name {
                review.names.insert(key, name);
            }
        }
        Opened { pull, review, heads }
    }
}

/// The review's content: `base` with the pull request's files laid over (records put in,
/// changed or taken out; the rest, a map's squares and the models, into `files`, the review's
/// own).
pub(super) fn overlay(base: &Content, opened: &Opened, files: &crate::platform::Files) -> Content {
    let mut c = base.clone();
    for (path, bytes) in &opened.heads {
        match (crate::keep::record_of(path), bytes) {
            (Some((space, rel)), Some(b)) => {
                let added = serde_json::from_slice::<Value>(b).map_err(|e| format!("{rel}: {e}")).and_then(|v| c.add(space, rel, v));
                if let Err(e) = added {
                    c.problems.push(format!("in the pull request: {e}"));
                }
            }
            (Some((_, rel)), None) => {
                c.remove_file(rel);
            }
            (None, Some(b)) => files.insert(path, b.clone()),
            (None, None) => {}
        }
    }
    c.sort();
    c
}

/// The pull request's files that a review's content needs, fetched: the game's files at its
/// head, and the records as they were at its base.
async fn fetch(hub: Hub, number: u64, tx: mpsc::Sender<Said>) -> Result<Opened, String> {
    let pull = pull_of(&hub.get(&format!("/repos/{OWNER}/{REPO}/pulls/{number}")).await?).ok_or("GitHub's answer isn't a pull request")?;
    let mut files = vec![];
    for page in 1..=10 {
        let v = hub.get(&format!("/repos/{OWNER}/{REPO}/pulls/{number}/files?per_page=100&page={page}")).await?;
        let got: Vec<FileChange> = v.as_array().into_iter().flatten().filter_map(file_of).collect();
        let n = got.len();
        files.extend(got);
        if n < 100 {
            break;
        }
    }
    let wanted: Vec<FileChange> = files.iter().filter(|f| of_the_game(&f.path)).cloned().collect();
    let (mut heads, mut bases) = (BTreeMap::new(), BTreeMap::new());
    for (k, f) in wanted.iter().enumerate() {
        let _ = tx.send(Said::Step(format!("{} of {}: {}", k + 1, wanted.len(), f.path)));
        let head = if f.kind == Kind::Removed { None } else { hub.contents(&f.path, &pull.head).await? };
        heads.insert(f.path.clone(), head);
        if matches!(f.place(), Place::Record { .. }) && f.kind != Kind::Added {
            let from = f.from.clone().unwrap_or_else(|| f.path.clone());
            if let Some(b) = hub.contents(&from, &pull.base).await? {
                bases.insert(f.path.clone(), b);
            }
        }
    }
    Ok(Opened::build(pull, files, heads, bases))
}

/// What the tasks say back.
enum Said {
    /// the user's permission and the pull requests open
    Listed(Result<(String, Vec<Pull>), String>),
    Step(String),
    Opened(Result<Box<Opened>, String>),
    Claude(Result<String, String>),
}

/// The Review window: the pull requests listed, one fetched, Claude asked.
pub(super) struct Reviewer {
    pub open: bool,
    /// the signed-in user's permission on the repository and the pull requests open, once
    /// GitHub has said
    pub listed: Option<Result<(String, Vec<Pull>), String>>,
    /// a pull request being fetched: which, how far
    pub fetching: Option<(u64, String)>,
    pub opened: Option<Result<Box<Opened>, String>>,
    /// a pull request asked for by a command (the checks): opened as soon as it's listed
    pub want: Option<u64>,
    /// Review by Claude: asked, what it said (or what went wrong), its window open
    pub asking: bool,
    pub said: Option<Result<String, String>>,
    pub said_open: bool,
    tx: mpsc::Sender<Said>,
    rx: mpsc::Receiver<Said>,
}

impl Default for Reviewer {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Reviewer { open: false, listed: None, fetching: None, opened: None, want: None, asking: false, said: None, said_open: false, tx, rx }
    }
}

impl Reviewer {
    /// What the tasks said since: true when anything did.
    fn take(&mut self) -> bool {
        let mut any = false;
        while let Ok(said) = self.rx.try_recv() {
            any = true;
            match said {
                Said::Listed(l) => self.listed = Some(l),
                Said::Step(s) => {
                    if let Some((_, step)) = &mut self.fetching {
                        *step = s;
                    }
                }
                Said::Opened(o) => {
                    self.fetching = None;
                    self.opened = Some(o);
                }
                Said::Claude(s) => {
                    self.asking = false;
                    self.said = Some(s);
                    self.said_open = true;
                }
            }
        }
        any
    }

    /// GitHub asked what `login` may do on the repository, and for the pull requests open.
    fn list(&mut self, hub: Hub, login: String, ctx: &egui::Context) {
        self.listed = None;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        spawn(async move {
            let permission = hub
                .get(&format!("/repos/{OWNER}/{REPO}/collaborators/{login}/permission"))
                .await
                .map(|v| v.get("permission").and_then(Value::as_str).unwrap_or("none").to_string());
            let pulls = hub.get(&format!("/repos/{OWNER}/{REPO}/pulls?state=open&per_page=50")).await.map(|v| pulls_of(&v));
            let _ = tx.send(Said::Listed(permission.and_then(|p| pulls.map(|l| (p, l)))));
            ctx.request_repaint();
        });
    }

    /// A pull request fetched whole.
    fn fetch(&mut self, hub: Hub, number: u64, ctx: &egui::Context) {
        self.fetching = Some((number, "asking GitHub".into()));
        self.opened = None;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        spawn(async move {
            let r = fetch(hub, number, tx.clone()).await.map(Box::new);
            let _ = tx.send(Said::Opened(r));
            ctx.request_repaint();
        });
    }

    /// Claude asked to review the pull request (`review`): on the user's own key, through the
    /// site's relay in the browser, straight to Anthropic on the desktop.
    fn ask_claude(&mut self, review: &Review, ctx: &egui::Context) {
        let Some(key) = claude_key() else {
            self.said = Some(Err(NO_KEY.to_string()));
            self.said_open = true;
            return;
        };
        let url = if cfg!(target_arch = "wasm32") { format!("{}/v1/messages", super::claude::RELAY) } else { "https://api.anthropic.com/v1/messages".to_string() };
        let body = json!({
            "model": "claude-opus-5-5",
            "max_tokens": 4000,
            "thinking": {"type": "adaptive"},
            "system": SYSTEM,
            "messages": [{"role": "user", "content": claude_prompt(review)}],
        });
        let mut req = ehttp::Request::post(url, serde_json::to_vec(&body).unwrap_or_default());
        req.headers = ehttp::Headers::new(&[("content-type", "application/json"), ("x-api-key", &key), ("anthropic-version", super::claude::VERSION)]);
        self.asking = true;
        self.said = None;
        self.said_open = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        ehttp::fetch(req, move |r| {
            let out = match r {
                Ok(resp) if resp.ok => serde_json::from_slice::<Value>(&resp.bytes).map_err(|e| format!("Claude's answer can't be read: {e}")).map(|v| answer_text(&v)),
                Ok(resp) => {
                    let body = serde_json::from_slice::<Value>(&resp.bytes).unwrap_or(Value::Null);
                    let message = body.pointer("/error/message").and_then(Value::as_str).unwrap_or(&resp.status_text).to_string();
                    Err(format!("Anthropic answered {}: {message}", resp.status))
                }
                Err(e) => Err(format!("Claude can't be reached: {e}")),
            };
            let _ = tx.send(Said::Claude(out));
            ctx.request_repaint();
        });
    }
}

/// The user's Claude API key: the one this browser keeps (the Claude sign-in), or on the
/// desktop the one in the environment.
fn claude_key() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        super::claude::key()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("ANTHROPIC_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
    }
}

/// Why Claude can't be asked without a key.
const NO_KEY: &str = if cfg!(target_arch = "wasm32") {
    "No Claude API key here: the top bar's Claude sign-in takes one (it stays in this browser)."
} else {
    "No Claude API key: set ANTHROPIC_API_KEY in the environment the editor runs in (the desktop's Claude Code sign-in can't be used for this)."
};

/// What Claude is told it's doing.
const SYSTEM: &str = "You review pull requests to Battle Frontier, an original creature RPG: a Rust workspace (crates/content: the game's records; crates/world: maps and meshes; crates/render: the renderer on wgpu; crates/game: the game and its map editor on egui), tools/ (checks in Node, the Blender pipeline in Python), functions/ (the site's own relays) and .github/ (the checks that run on every pull request). The reviewer is a maintainer who wants to know exactly what changes and what it means in the game and in the editor. Be concrete and complete; never pad, never guess what the diff doesn't show (say so instead).";

/// What Claude is asked for.
const ASK: &str = "Write a detailed review of this pull request for its maintainer, in these sections with exactly these headings:\n\
## Content\nBy collection, each record changed and what the change means in the game (a terrain's look, a map's cells painted or props placed, a plant's sway...).\n\
## Editor and engine code\nEach file: what changed and in what way (functions added or removed, changed behaviour, new settings, commands or hooks) and what it affects for players and for the editor's users. Name the functions.\n\
## Tools, checks and the site\nWhat the checks and scripts now cover or do differently.\n\
## Risks and what to look at\nWhat could break, what isn't tested, what to try before merging.\n\
## Verdict\nOne line: ready to merge, or what's missing.\n\
Short bullets; name files and records as they are.";

/// The most of the code's diffs sent (characters), and the most of one file's.
const DIFFS_MOST: usize = 80_000;
const DIFF_MOST: usize = 6_000;

/// What Claude is given: the review in words, then the code's diffs (the long ones cut), then
/// the ask.
pub(super) fn claude_prompt(review: &Review) -> String {
    let mut out = review.summary();
    let mut left = DIFFS_MOST;
    let code = review.code();
    let others = review.other();
    let with_patch = code.values().flatten().chain(others.iter()).filter(|f| f.patch.is_some());
    let mut any = false;
    for f in with_patch {
        let Some(patch) = &f.patch else { continue };
        if !any {
            out.push_str("\n\nThe diffs of the code and the other files (unified; the long ones cut):\n");
            any = true;
        }
        if left == 0 {
            out.push_str("\n(the rest of the diffs left out: too long)\n");
            break;
        }
        let take = DIFF_MOST.min(left);
        let cut: String = patch.chars().take(take).collect();
        left -= cut.chars().count();
        out.push_str(&format!("\n--- {}\n{cut}{}\n", f.line(), if patch.chars().count() > take { "\n(cut)" } else { "" }));
    }
    out.push_str("\n\n");
    out.push_str(ASK);
    out
}

/// What Claude said: its text blocks, together.
fn answer_text(v: &Value) -> String {
    v.get("content")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

// ---------------------------------------------------------------- the marks

/// A small dot of a change's colour, saying what changed when hovered.
pub(super) fn dot(ui: &mut egui::Ui, kind: Kind, about: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, colour(kind));
    r.on_hover_text(about)
}

/// A change's kind as a tag: ADDED, MODIFIED, REMOVED.
pub(super) fn change_badge(ui: &mut egui::Ui, kind: Kind) -> egui::Response {
    ui.label(RichText::new(kind.word().to_uppercase()).size(9.5).strong().color(Color32::BLACK).background_color(colour(kind)))
}

/// The panes a collection's records show in: where their changes are marked.
fn pane_of(collection: &str) -> Option<dock::Pane> {
    Some(match collection {
        "props" | "creatures" => dock::Pane::Assets,
        "terrains" | "templates" | "spawns" => dock::Pane::Toolbox,
        "maps" | "worlds" => dock::Pane::World,
        "settings" => dock::Pane::Post,
        "moves" => dock::Pane::Abilities,
        "fx" => dock::Pane::Timeline,
        _ => return None,
    })
}

/// What of a record changed under `prefix` in a pull request (`review`): how, and the changes in
/// words; None when nothing did (`MapEditor::review_changes`, for closures holding the review).
pub(super) fn changes_of(review: Option<&Opened>, collection: &str, id: &str, prefix: &str) -> Option<(Kind, String)> {
    let r = &review?.review;
    let kind = r.record_kind(collection, id)?;
    if matches!(kind, Kind::Added | Kind::Removed) {
        return Some((kind, format!("{} in the pull request", kind.word())));
    }
    let changes = r.changed(collection, id, prefix);
    if changes.is_empty() {
        return None;
    }
    let kinds: Vec<Kind> = changes.iter().map(|c| c.kind).collect();
    let kind = if kinds.iter().all(|k| *k == kinds[0]) { kinds[0] } else { Kind::Modified };
    Some((kind, changes.iter().map(|c| format!("{} {}: {}", c.kind.word(), c.path, c.detail)).collect::<Vec<_>>().join("\n")))
}

/// A dot by a record's row or field when a pull request (`review`) changed it
/// (`MapEditor::change_mark`, for closures holding the review).
pub(super) fn record_mark(ui: &mut egui::Ui, review: Option<&Opened>, collection: &str, id: &str, prefix: &str) {
    if let Some((kind, about)) = changes_of(review, collection, id, prefix) {
        let r = dot(ui, kind, &about);
        mark(&format!("uiChanged_{collection}_{id}{}", if prefix.is_empty() { String::new() } else { format!("_{}", prefix.replace(['.', '[', ']'], "_")) }), &r);
    }
}

impl MapEditor {
    /// The pull request under review, when the editor is reviewing one.
    pub fn reviewing(&self) -> Option<&Review> {
        self.review.as_deref().map(|o| &o.review)
    }

    /// How a record changed in the pull request under review, whole; None when it didn't (or
    /// nothing's reviewed).
    pub(super) fn review_kind(&self, collection: &str, id: &str) -> Option<Kind> {
        self.reviewing()?.record_kind(collection, id)
    }

    /// What of a record changed under `prefix` (a section's fields, or one field: ""
    /// for all of it): how, and the changes in words; None when nothing did.
    pub(super) fn review_changes(&self, collection: &str, id: &str, prefix: &str) -> Option<(Kind, String)> {
        changes_of(self.review.as_deref(), collection, id, prefix)
    }

    /// What of a record changed in any of `prefixes` (a box of fields), as `review_changes`.
    pub(super) fn review_changes_in(&self, collection: &str, id: &str, prefixes: &[&str]) -> Option<(Kind, String)> {
        let found: Vec<(Kind, String)> = prefixes.iter().filter_map(|p| self.review_changes(collection, id, p)).collect();
        if found.is_empty() {
            return None;
        }
        let kind = if found.iter().all(|(k, _)| *k == found[0].0) { found[0].0 } else { Kind::Modified };
        let mut about: Vec<String> = found.into_iter().map(|(_, a)| a).collect();
        about.dedup();
        Some((kind, about.join("\n")))
    }

    /// A dot by a section or a field of a record, when the pull request under review changed
    /// it (`prefix`: the field's path, or a section's; "" for the whole record); nothing else.
    pub(super) fn change_mark(&self, ui: &mut egui::Ui, collection: &str, id: &str, prefix: &str) {
        record_mark(ui, self.review.as_deref(), collection, id, prefix);
    }

    /// How many changes each panel holds: the records its windows show, the assets, and the
    /// map open (the Inspector's, and the tools').
    pub(super) fn pane_changes(&self) -> BTreeMap<dock::Pane, Counts> {
        let mut out: BTreeMap<dock::Pane, Counts> = BTreeMap::new();
        let Some(r) = self.reviewing() else { return out };
        for (collection, counts) in r.collections() {
            if let Some(pane) = pane_of(&collection) {
                out.entry(pane).or_default().add(counts);
            }
        }
        for (_, _, kind) in r.assets() {
            out.entry(dock::Pane::Assets).or_default().count(kind);
        }
        if let Some(kind) = r.record_kind("maps", &self.map.id) {
            out.entry(dock::Pane::Inspector).or_default().count(kind);
            out.entry(dock::Pane::Toolbox).or_default().count(kind);
        }
        out
    }

    /// A dot on each panel's toggle that holds changes (the pull request under review),
    /// saying how many when hovered; the hook `reviewdots` lists them.
    pub(super) fn review_dots(&self, ui: &mut egui::Ui, toggles: &BTreeMap<dock::Pane, egui::Response>) {
        let changes = self.pane_changes();
        let listed: Vec<String> = changes.iter().map(|(p, c)| format!("{}:{}", p.key(), c.total())).collect();
        crate::platform::set_hook("reviewdots", &listed.join(";"));
        for (pane, counts) in changes {
            let Some(r) = toggles.get(&pane) else { continue };
            let kind = if counts.modified > 0 || (counts.added > 0 && counts.removed > 0) {
                Kind::Modified
            } else if counts.added > 0 {
                Kind::Added
            } else {
                Kind::Removed
            };
            let at = r.rect.right_top() + egui::vec2(-2.0, 3.0);
            ui.painter().circle_filled(at, 4.0, colour(kind));
            ui.painter().circle_stroke(at, 4.0, egui::Stroke::new(1.0, Color32::from_gray(30)));
            let hover = ui.interact(egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)), r.id.with("review dot"), egui::Sense::hover());
            hover.on_hover_text(format!("In the pull request: {}", counts.text()));
        }
    }

    /// The top bar's Review (a signed-in user's: the pull requests open, one opened to review)
    /// and, reviewing, what's reviewed and the way out.
    pub(super) fn review_buttons(&mut self, ui: &mut egui::Ui) {
        if let Some(o) = self.review.clone() {
            let text = RichText::new(format!("Reviewing #{}", o.pull.number)).strong().color(MODIFIED);
            let b = ui.button(text).on_hover_text(format!("{} by @{}: its changes listed, Claude asked to review it", o.pull.title, o.pull.author));
            mark("uiReview", &b);
            if b.clicked() {
                self.reviewer.open = true;
            }
            let leave = ui.button("Leave the review").on_hover_text("Back to your own editor as you left it (nothing of the pull request is kept)");
            mark("uiReviewLeave", &leave);
            if leave.clicked() {
                self.review_leave = true;
            }
            return;
        }
        let signed = self.publisher.user.is_some();
        let b = ui.add_enabled(signed, egui::Button::new("Review…")).on_hover_text("A pull request's changes opened in the editor, read-only, every change marked (for the repository's collaborators)").on_disabled_hover_text("Sign in with GitHub first");
        mark("uiReview", &b);
        if b.clicked() {
            self.reviewer.open = true;
            self.reviewer.listed = None;
        }
    }

    /// The checks: a pull request asked for (opened as soon as GitHub lists it).
    pub fn review_ask(&mut self, number: u64) {
        self.reviewer.open = true;
        self.reviewer.want = Some(number);
        self.reviewer.listed = None;
    }

    /// The pull request the user chose to open in the editor, once: the app opens the review.
    pub fn take_review(&mut self) -> Option<Box<Opened>> {
        self.review_enter.take()
    }

    /// The Review window: the pull requests open (a collaborator's to open), one being
    /// fetched, one fetched (to open in the editor); reviewing, its changes by kind, the code
    /// it changes, Review by Claude and the way out. And Claude's answer, in its own window.
    pub(super) fn review_window(&mut self, ctx: &egui::Context) {
        if self.reviewer.take() {
            ctx.request_repaint();
        }
        crate::platform::set_hook("reviewing", &self.review.as_ref().map(|o| o.pull.number.to_string()).unwrap_or_default());
        crate::platform::set_hook("reviewpulls", &match &self.reviewer.listed {
            Some(Ok((p, pulls))) => format!("{p}|{}", pulls.iter().map(|q| format!("{}:{}", q.number, q.title)).collect::<Vec<_>>().join(";")),
            Some(Err(e)) => format!("error: {e}"),
            None => String::new(),
        });
        crate::platform::set_hook("reviewstep", &self.reviewer.fetching.as_ref().map(|(_, s)| s.clone()).unwrap_or_default());
        crate::platform::set_hook("reviewopened", &match &self.reviewer.opened {
            Some(Ok(o)) => format!("{}:{} files", o.pull.number, o.review.files.len()),
            Some(Err(e)) => format!("error: {e}"),
            None => String::new(),
        });
        crate::platform::set_hook("reviewclaude", &match (&self.reviewer.asking, &self.reviewer.said) {
            (true, _) => "asking".to_string(),
            (false, Some(Ok(_))) => "answered".to_string(),
            (false, Some(Err(e))) => format!("error: {e}"),
            (false, None) => String::new(),
        });
        self.claude_review_window(ctx);
        if !self.reviewer.open {
            return;
        }
        let signed = self.publisher.signed();
        // listed once a sign-in's there (and again when the window's opened anew)
        if let (Some((hub, login)), None) = (&signed, &self.reviewer.listed) {
            if self.reviewer.fetching.is_none() && self.review.is_none() {
                self.reviewer.list(hub.clone(), login.clone(), ctx);
                self.reviewer.listed = Some(Ok((String::new(), vec![])));
                self.reviewer.listed = None;
                // (asked just now: a frame of nothing, then GitHub's answer)
                self.reviewer.fetching = Some((0, String::new()));
            }
        }
        if self.reviewer.fetching.as_ref().is_some_and(|(n, _)| *n == 0) && self.reviewer.listed.is_some() {
            self.reviewer.fetching = None;
        }
        if self.reviewer.fetching.is_some() || self.reviewer.asking {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut close = false;
        let mut open_pull: Option<u64> = None;
        let mut enter = false;
        let mut ask_claude = false;
        let modal = egui::Modal::new(egui::Id::new("editor-review")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(640.0);
            match self.review.clone() {
                Some(o) => {
                    ui.label(RichText::new(format!("Reviewing #{}: {}", o.pull.number, o.pull.title)).size(20.0).color(crate::hud::TEXT));
                    hint(ui, &format!("by @{} · {} files · {}", o.pull.author, o.review.files.len(), o.pull.url));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| changes_listed(ui, &o.review));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let b = ui.add_enabled(!self.reviewer.asking, egui::Button::new(RichText::new("Review by Claude").strong())).on_hover_text("Claude reads every change (the records, the code's diffs) and lists them in detail, with what to look at before merging: on your own key");
                        mark("uiReviewClaude", &b);
                        ask_claude = b.clicked();
                        if self.reviewer.said.is_some() && !self.reviewer.asking && ui.button("What Claude said").clicked() {
                            self.reviewer.said_open = true;
                        }
                        let leave = ui.button("Leave the review").on_hover_text("Back to your own editor as you left it");
                        mark("uiReviewLeaveWindow", &leave);
                        if leave.clicked() {
                            self.review_leave = true;
                            close = true;
                        }
                        let c = ui.button("Close");
                        mark("uiReviewClose", &c);
                        if c.clicked() {
                            close = true;
                        }
                    });
                }
                None => {
                    ui.label(RichText::new("Review a pull request").size(20.0).color(crate::hud::TEXT));
                    hint(ui, &format!("One of {OWNER}/{REPO}'s open pull requests, opened in the editor as the game would be with it merged: in an environment of its own (nothing you've made is touched, nothing of it is kept), every change marked where it shows."));
                    ui.add_space(6.0);
                    match (&signed, &self.reviewer.listed) {
                        (None, _) => warn(ui, "Sign in with GitHub first (the top bar's Sign in)."),
                        (Some(_), None) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Asking GitHub…").color(crate::hud::TEXT));
                            });
                        }
                        (Some(_), Some(Err(e))) => warn(ui, e),
                        (Some((_, login)), Some(Ok((permission, pulls)))) => {
                            let may = may_review(permission);
                            hint(ui, &format!("@{login}: {permission} on the repository"));
                            if !may {
                                warn(ui, "Only the repository's collaborators who can merge (write access) review here: ask a maintainer for it.");
                            }
                            if pulls.is_empty() {
                                hint(ui, "No pull request is open.");
                            }
                            for p in pulls {
                                ui.horizontal(|ui| {
                                    let b = ui.add_enabled(may && self.reviewer.fetching.is_none(), egui::Button::new("Open")).on_hover_text("Fetch what it changes and open it in the editor");
                                    mark(&format!("uiReviewOpen_{}", p.number), &b);
                                    if b.clicked() {
                                        open_pull = Some(p.number);
                                    }
                                    ui.label(RichText::new(format!("#{} {}", p.number, p.title)).strong().color(crate::hud::TEXT));
                                    hint(ui, &format!("@{} · {} · {} file{}{}", p.author, p.branch, p.files, if p.files == 1 { "" } else { "s" }, if p.draft { " · draft" } else { "" }));
                                });
                            }
                            if may {
                                if let Some(n) = self.reviewer.want.take() {
                                    open_pull = Some(n);
                                }
                            }
                        }
                    }
                    if let Some((n, step)) = &self.reviewer.fetching {
                        if *n > 0 {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(format!("Fetching #{n}: {step}")).color(crate::hud::TEXT));
                            });
                        }
                    }
                    match &self.reviewer.opened {
                        Some(Ok(o)) => {
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("#{} fetched: {} files", o.pull.number, o.review.files.len())).strong().color(crate::hud::TEXT));
                            for (collection, counts) in o.review.collections() {
                                hint(ui, &format!("{collection}: {}", counts.text()));
                            }
                            let assets = o.review.assets().len();
                            let code: usize = o.review.code().values().map(Vec::len).sum();
                            if assets + code > 0 {
                                hint(ui, &format!("{assets} asset{}, {code} code file{}", if assets == 1 { "" } else { "s" }, if code == 1 { "" } else { "s" }));
                            }
                            let b = ui.button(RichText::new("Open it in the editor").strong()).on_hover_text("Your editor is parked as it is; the pull request's content opens in its place, read-only, every change marked");
                            mark("uiReviewEnter", &b);
                            enter = b.clicked();
                        }
                        Some(Err(e)) => warn(ui, e),
                        None => {}
                    }
                    ui.add_space(8.0);
                    let c = ui.button("Close");
                    mark("uiReviewClose", &c);
                    if c.clicked() {
                        close = true;
                    }
                }
            }
        });
        if let (Some(n), Some((hub, _))) = (open_pull, &signed) {
            self.reviewer.fetch(hub.clone(), n, ctx);
        }
        if enter {
            if let Some(Ok(o)) = self.reviewer.opened.take() {
                self.review_enter = Some(o);
                close = true;
            }
        }
        if ask_claude {
            if let Some(o) = self.review.clone() {
                self.reviewer.ask_claude(&o.review, ctx);
            }
        }
        if close || modal.should_close() {
            self.reviewer.open = false;
        }
    }

    /// Claude's review, in a window of its own: what it said, read in sections.
    fn claude_review_window(&mut self, ctx: &egui::Context) {
        if !self.reviewer.said_open {
            return;
        }
        let mut close = false;
        let number = self.review.as_ref().map(|o| o.pull.number).unwrap_or(0);
        let modal = egui::Modal::new(egui::Id::new("editor-review-claude")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(720.0);
            ui.label(RichText::new("Review by Claude").size(20.0).color(crate::hud::TEXT));
            if let Some(o) = &self.review {
                hint(ui, &format!("#{}: {} by @{}", o.pull.number, o.pull.title, o.pull.author));
            }
            ui.add_space(6.0);
            match (&self.reviewer.asking, &self.reviewer.said) {
                (true, _) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Claude is reading the pull request…").color(crate::hud::TEXT));
                    });
                }
                (false, Some(Ok(text))) => {
                    let text = text.clone();
                    egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| prose(ui, &text));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Copy").on_hover_text("The whole review, as text").clicked() {
                            ctx.copy_text(text.clone());
                        }
                        let c = ui.button("Close");
                        mark("uiReviewClaudeClose", &c);
                        if c.clicked() {
                            close = true;
                        }
                    });
                }
                (false, Some(Err(e))) => {
                    warn(ui, e);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
                (false, None) => {
                    close = true;
                }
            }
            let _ = number;
        });
        if close || modal.should_close() {
            self.reviewer.said_open = false;
        }
    }
}

/// The pull request's changes listed: the records by collection (each with its fields), the
/// assets, the code by area (each file with its diff to unfold), the rest.
fn changes_listed(ui: &mut egui::Ui, r: &Review) {
    for (collection, counts) in r.collections() {
        egui::CollapsingHeader::new(RichText::new(format!("{collection}: {}", counts.text())).strong()).id_salt(("review collection", &collection)).default_open(true).show(ui, |ui| {
            for (id, kind) in r.records_of(&collection) {
                ui.horizontal(|ui| {
                    change_badge(ui, kind);
                    ui.label(RichText::new(r.name_of(&collection, &id)).strong());
                    ui.label(RichText::new(&id).monospace().size(11.0).color(Color32::GRAY));
                });
                for c in r.changed(&collection, &id, "") {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        dot(ui, c.kind, &c.detail);
                        ui.label(RichText::new(format!("{}: {}", c.path, c.detail)).size(12.0));
                    });
                }
            }
        });
    }
    let assets = r.assets();
    if !assets.is_empty() {
        egui::CollapsingHeader::new(RichText::new(format!("assets: {}", assets.len())).strong()).id_salt("review assets").default_open(true).show(ui, |ui| {
            for (kind, name, k) in assets {
                ui.horizontal(|ui| {
                    change_badge(ui, k);
                    ui.label(RichText::new(format!("{kind}/{name}")).monospace().size(12.0));
                });
            }
        });
    }
    for (area, files) in r.code() {
        egui::CollapsingHeader::new(RichText::new(format!("code, {area}: {} file{}", files.len(), if files.len() == 1 { "" } else { "s" })).strong()).id_salt(("review code", &area)).default_open(true).show(ui, |ui| {
            for f in files {
                file_listed(ui, f);
            }
        });
    }
    let other = r.other();
    if !other.is_empty() {
        egui::CollapsingHeader::new(RichText::new(format!("other: {} file{}", other.len(), if other.len() == 1 { "" } else { "s" })).strong()).id_salt("review other").show(ui, |ui| {
            for f in other {
                file_listed(ui, f);
            }
        });
    }
}

/// A changed file: its badge, its path and lines; its diff, unfolded.
fn file_listed(ui: &mut egui::Ui, f: &FileChange) {
    ui.horizontal(|ui| {
        change_badge(ui, f.kind);
        ui.label(RichText::new(&f.path).monospace().size(12.0));
        hint(ui, &format!("+{} -{}", f.additions, f.deletions));
    });
    if let Some(patch) = &f.patch {
        ui.indent(("review patch", &f.path), |ui| {
            egui::CollapsingHeader::new(RichText::new("the diff").size(11.5)).id_salt(("review diff", &f.path)).show(ui, |ui| {
                for line in patch.lines().take(400) {
                    let colour = match line.chars().next() {
                        Some('+') => ADDED,
                        Some('-') => REMOVED,
                        Some('@') => Color32::from_gray(150),
                        _ => Color32::from_gray(200),
                    };
                    ui.label(RichText::new(line).monospace().size(11.0).color(colour));
                }
                if patch.lines().count() > 400 {
                    hint(ui, "(cut: the rest on GitHub)");
                }
            });
        });
    }
}

/// Claude's answer laid out: its headings larger, its bullets as bullets, `**bold**` strong.
fn prose(ui: &mut egui::Ui, text: &str) {
    for line in text.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = t.strip_prefix("### ") {
            ui.label(RichText::new(h).strong().size(14.0).color(crate::hud::TEXT));
        } else if let Some(h) = t.strip_prefix("## ") {
            ui.add_space(6.0);
            ui.label(RichText::new(h).strong().size(16.0).color(crate::hud::TEXT));
        } else if let Some(h) = t.strip_prefix("# ") {
            ui.add_space(6.0);
            ui.label(RichText::new(h).strong().size(18.0).color(crate::hud::TEXT));
        } else if let Some(b) = t.trim_start().strip_prefix("- ").or_else(|| t.trim_start().strip_prefix("* ")) {
            let depth = (t.len() - t.trim_start().len()) / 2;
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0 + 12.0 * depth as f32);
                ui.label("•");
                spans(ui, b);
            });
        } else {
            ui.horizontal_wrapped(|ui| spans(ui, t));
        }
    }
}

/// A line's text with its `**bold**` and `` `code` `` spans.
fn spans(ui: &mut egui::Ui, text: &str) {
    ui.spacing_mut().item_spacing.x = 0.0;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                ui.label(RichText::new(&after[..end]).strong().size(13.0));
                rest = &after[end + 2..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                ui.label(RichText::new(&after[..end]).monospace().size(12.0));
                rest = &after[end + 1..];
                continue;
            }
        }
        let next = [rest[1..].find("**").map(|i| i + 1), rest[1..].find('`').map(|i| i + 1)].into_iter().flatten().min().unwrap_or(rest.len());
        ui.label(RichText::new(&rest[..next]).size(13.0));
        rest = &rest[next..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn githubs_pull_requests_and_files_read_and_only_those_who_merge_review() {
        let list = json!([{"number": 7, "title": "Lava", "user": {"login": "someone"}, "head": {"ref": "lava", "sha": "h7"}, "base": {"sha": "b0"}, "html_url": "https://github.com/x/y/pull/7", "changed_files": 3, "draft": false}, {"title": "no number"}]);
        let pulls = pulls_of(&list);
        assert_eq!(pulls.len(), 1);
        assert_eq!((pulls[0].number, pulls[0].author.as_str(), pulls[0].head.as_str(), pulls[0].files), (7, "someone", "h7", 3));
        let f = file_of(&json!({"filename": "content/data/terrains/lava.json", "status": "added", "additions": 20, "deletions": 0, "patch": "@@ -0,0 +1 @@\n+{}"})).unwrap();
        assert_eq!((f.kind, f.additions, f.patch.as_deref()), (Kind::Added, 20, Some("@@ -0,0 +1 @@\n+{}")));
        let g = file_of(&json!({"filename": "b.rs", "status": "renamed", "previous_filename": "a.rs"})).unwrap();
        assert_eq!((g.kind, g.from.as_deref()), (Kind::Renamed, Some("a.rs")));
        assert!(may_review("admin") && may_review("write") && may_review("maintain") && !may_review("read") && !may_review("none") && !may_review(""));
        assert!(of_the_game("content/data/maps/east_field/0_0.json") && of_the_game("temporary/assets/models/x.glb") && !of_the_game("crates/game/src/main.rs"));
    }

    #[test]
    fn a_pull_request_built_knows_each_records_changes_and_lays_its_files_over_the_content() {
        let pull = Pull { number: 7, title: "Lava".into(), author: "someone".into(), head: "h7".into(), base: "b0".into(), ..Default::default() };
        let file = |path: &str, status: &str| FileChange { path: path.into(), kind: Kind::of_status(status), ..Default::default() };
        let files = vec![file("content/data/terrains/lava.json", "added"), file("content/data/terrains/grass.json", "modified"), file("content/data/props/old_post.json", "removed"), file("content/data/maps/field/0_0.json", "modified"), file("content/assets/models/lamp.glb", "added"), file("crates/game/src/mapedit.rs", "modified")];
        let heads = BTreeMap::from([
            ("content/data/terrains/lava.json".to_string(), Some(br##"{ "id": "lava", "name": "Lava", "walk": "block", "pattern": "custom" }"##.to_vec())),
            ("content/data/terrains/grass.json".to_string(), Some(br##"{ "id": "grass", "name": "Grass", "walk": "walk", "priority": 2 }"##.to_vec())),
            ("content/data/props/old_post.json".to_string(), None),
            ("content/data/maps/field/0_0.json".to_string(), Some(br##"{ "terrain": ["gd", "gg"] }"##.to_vec())),
            ("content/assets/models/lamp.glb".to_string(), Some(b"glTF".to_vec())),
        ]);
        let bases = BTreeMap::from([
            ("content/data/terrains/grass.json".to_string(), br##"{ "id": "grass", "name": "Grass", "walk": "walk", "priority": 1 }"##.to_vec()),
            ("content/data/props/old_post.json".to_string(), br##"{ "id": "old_post", "name": "Old post" }"##.to_vec()),
            ("content/data/maps/field/0_0.json".to_string(), br##"{ "terrain": ["gg", "gg"] }"##.to_vec()),
        ]);
        let o = Opened::build(pull, files, heads, bases);
        let r = &o.review;
        assert_eq!(r.record_kind("terrains", "lava"), Some(Kind::Added));
        assert_eq!(r.changed("terrains", "grass", "").iter().map(|c| format!("{} {}", c.path, c.detail)).collect::<Vec<_>>(), ["priority 1 -> 2"]);
        assert_eq!(r.name_of("props", "old_post"), "Old post", "a removed record's name, from before");
        assert_eq!(r.changed("maps", "field", "").iter().map(|c| c.path.clone()).collect::<Vec<_>>(), ["squares[0_0].terrain"], "a square's changes under its name");
        assert_eq!(r.collections().get("terrains").map(|c| c.text()), Some("1 added, 1 modified".to_string()));
        // laid over the content: the terrain added, the prop gone, the model in the files
        let mut base = Content::default();
        base.terrains.put(content::Terrain { id: "grass".into(), name: "Grass".into(), priority: 1, ..Default::default() }, content::Space::Content);
        base.props.put(content::Prop { id: "old_post".into(), name: "Old post".into(), ..Default::default() }, content::Space::Content);
        let files = crate::platform::Files::new(".");
        let c = overlay(&base, &o, &files);
        assert!(c.terrains.contains("lava") && c.terrains.get("grass").unwrap().priority == 2 && !c.props.contains("old_post"), "{:?}", c.problems);
        assert!(matches!(files.get("content/assets/models/lamp.glb"), crate::platform::Load::Ready(b) if b.as_slice() == b"glTF"));
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        // what Claude is given: the summary, the diffs, the ask
        let prompt = claude_prompt(r);
        assert!(prompt.starts_with("Pull request #7 by someone: Lava (6 files)") && prompt.contains("## Verdict"), "{prompt}");
        assert_eq!(answer_text(&json!({"content": [{"type": "thinking", "thinking": "hm"}, {"type": "text", "text": "## Content\n- lava"}]})), "## Content\n- lava");
    }

    /// The game a review starts from: a lawn, a crate, a map of the lawn.
    fn game() -> Content {
        let mut c = Content::default();
        c.terrains.put(content::Terrain { id: "grass".into(), name: "Grass".into(), pattern: "grass".into(), walk: "walk".into(), color: "#4a8a3a".into(), color2: "#2e6a28".into(), priority: 1, ..Default::default() }, content::Space::Content);
        c.props.put(content::Prop { id: "crate".into(), name: "Crate".into(), category: "decoration".into(), footprint: [1, 1], height: 1.0, ..Default::default() }, content::Space::Content);
        c.maps.put(content::Map { id: "field".into(), name: "Field".into(), width: 4, height: 4, legend: [("g".to_string(), "grass".to_string())].into_iter().collect(), ..Default::default() }, content::Space::Content);
        c
    }

    /// A frame of the editor's review window (`events` in it); the marks of what's drawn.
    fn frame(ctx: &egui::Context, ed: &mut MapEditor, events: Vec<egui::Event>) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        crate::platform::clear_hooks();
        let mut out = ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| ed.review_window(ui.ctx()));
        out.textures_delta.clear();
    }

    /// A click on a marked widget of the window, as a pointer makes it.
    fn click(ctx: &egui::Context, ed: &mut MapEditor, name: &str) {
        let at = crate::platform::hook(name).unwrap_or_else(|| panic!("{name} isn't drawn"));
        let (x, y) = at.split_once(',').unwrap();
        let pos = egui::pos2(x.parse().unwrap(), y.parse().unwrap());
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(ctx, ed, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(ctx, ed, vec![button(false)]);
    }

    /// A collaborator (write access, as GitHub says) sees the pull requests open, the one
    /// asked for is fetched whole (its files at its head, the records at its base) and opened
    /// in an editor of its own: its content as merged, read-only, each panel's changes counted.
    #[test]
    fn a_collaborator_lists_the_pull_requests_opens_one_and_reviews_it_read_only() {
        use base64::Engine as _;
        let (base, asked) = super::super::publish::stand_in(|_, path, _| {
            // (the editor's stand-in has the API under /api)
            let path = path.strip_prefix("/api").unwrap_or(path);
            let enc = |text: &str| json!({"encoding": "base64", "sha": "x", "content": base64::engine::general_purpose::STANDARD.encode(text)});
            let r = "/repos/LouisMareau/battle-frontier";
            let pull = json!({"number": 7, "title": "Lava", "user": {"login": "someone"}, "head": {"ref": "lava", "sha": "h7"}, "base": {"sha": "b0"}, "html_url": "https://github.com/x/y/pull/7", "changed_files": 4});
            if path.starts_with(&format!("{r}/collaborators/player/permission")) {
                return (200, json!({"permission": "write", "role_name": "write"}));
            }
            if path.starts_with(&format!("{r}/pulls/7/files")) {
                return (
                    200,
                    json!([
                        {"filename": "content/data/terrains/lava.json", "status": "added", "additions": 9, "deletions": 0},
                        {"filename": "content/data/terrains/grass.json", "status": "modified", "additions": 1, "deletions": 1},
                        {"filename": "content/data/props/crate.json", "status": "removed", "additions": 0, "deletions": 12},
                        {"filename": "crates/game/src/mapedit.rs", "status": "modified", "additions": 3, "deletions": 1, "patch": "@@ -1 +1 @@\n-a\n+b"}
                    ]),
                );
            }
            if path.starts_with(&format!("{r}/pulls/7")) {
                return (200, pull);
            }
            if path.starts_with(&format!("{r}/pulls")) {
                return (200, json!([pull]));
            }
            if path.contains("/contents/content/data/terrains/lava.json") {
                return (200, enc(r##"{ "id": "lava", "name": "Lava", "pattern": "custom", "walk": "block", "color": "#ff5a10", "color2": "#ffb030", "side": "#2a2220" }"##));
            }
            if path.contains("/contents/content/data/terrains/grass.json?ref=h7") {
                return (200, enc(r##"{ "id": "grass", "name": "Grass", "pattern": "grass", "walk": "walk", "color": "#4a8a3a", "color2": "#2e6a28", "priority": 2 }"##));
            }
            if path.contains("/contents/content/data/terrains/grass.json?ref=b0") {
                return (200, enc(r##"{ "id": "grass", "name": "Grass", "pattern": "grass", "walk": "walk", "color": "#4a8a3a", "color2": "#2e6a28", "priority": 1 }"##));
            }
            if path.contains("/contents/content/data/props/crate.json?ref=b0") {
                return (200, enc(r##"{ "id": "crate", "name": "Crate" }"##));
            }
            (404, json!({"message": "Not Found"}))
        });
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let game = std::sync::Arc::new(game());
        let mut ed = MapEditor::new(game.clone(), None, crate::platform::Files::new("."));
        ed.publisher.user = Some(super::super::publish::User { login: "player".into(), token: "t".into() });
        ed.publisher.loaded = true;
        ed.github_at(&base);
        ed.review_ask(7);
        // GitHub asked (the list, then the pull request wanted, fetched whole): waited for
        let mut fetched = false;
        for _ in 0..300 {
            frame(&ctx, &mut ed, vec![]);
            if matches!(ed.reviewer.opened, Some(Ok(_))) {
                fetched = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(fetched, "{:?} / {:?}", ed.reviewer.listed.as_ref().map(|l| l.as_ref().map(|(p, pulls)| (p.clone(), pulls.len()))), ed.reviewer.opened.as_ref().map(|o| o.as_ref().map(|_| ())));
        assert_eq!(crate::platform::hook("reviewpulls").as_deref(), Some("write|7:Lava"));
        assert!(crate::platform::hook("reviewopened").as_deref() == Some("7:4 files"), "{:?}", crate::platform::hook("reviewopened"));
        {
            let asked = asked.lock().unwrap();
            let paths: Vec<&str> = asked.iter().map(|(_, p, _)| p.as_str()).collect();
            assert!(paths.iter().any(|p| p.contains("/pulls/7/files")) && paths.iter().any(|p| p.contains("grass.json?ref=b0")) && !paths.iter().any(|p| p.contains("mapedit.rs")), "{paths:?}");
        }
        // opened in the editor: the app takes it
        frame(&ctx, &mut ed, vec![]);
        click(&ctx, &mut ed, "uiReviewEnter");
        let opened = ed.take_review().expect("the pull request chosen to open");
        assert!(ed.take_review().is_none() && !ed.reviewer.open);
        assert_eq!((opened.pull.number, opened.review.files.len()), (7, 4));
        let mut review = MapEditor::new_review(game, opened, ".");
        assert!(review.reviewing().is_some_and(|r| r.number == 7));
        assert!(review.content.terrains.contains("lava") && review.content.terrains.get("grass").unwrap().priority == 2 && !review.content.props.contains("crate"), "{:?}", review.content.problems);
        let panes = review.pane_changes();
        assert_eq!(panes.get(&dock::Pane::Toolbox).map(|c| c.text()), Some("1 added, 1 modified".to_string()));
        assert_eq!(panes.get(&dock::Pane::Assets).map(|c| c.text()), Some("1 removed".to_string()));
        assert!(!panes.contains_key(&dock::Pane::World));
        // read-only
        review.save();
        assert!(review.status.contains("nothing is saved"), "{}", review.status);
        review.dirty = true;
        review.save_some(true);
        assert!(review.status.contains("nothing is saved"));
        frame(&ctx, &mut review, vec![]);
        assert_eq!(crate::platform::hook("reviewing").as_deref(), Some("7"));
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() }, |ui| review.review_dots(ui, &BTreeMap::new()));
        out.textures_delta.clear();
        assert_eq!(crate::platform::hook("reviewdots").as_deref(), Some("toolbox:2;assets:1"));
        // leaving it
        review.review_leave = true;
        assert!(review.reviewing().is_some(), "the app brings the parked editor back");
    }

    /// Reviewing, the changes show where they are: a mark by the terrain changed (and the
    /// one added listed), the Assets panel's "Changed in the PR" folder with the prop added
    /// and the one taken out, a badge on the prop's row.
    #[test]
    fn the_changes_are_marked_where_they_show() {
        let pull = Pull { number: 7, title: "Lava".into(), author: "someone".into(), head: "h7".into(), base: "b0".into(), ..Default::default() };
        let file = |path: &str, status: &str| FileChange { path: path.into(), kind: Kind::of_status(status), ..Default::default() };
        let files = vec![file("content/data/terrains/lava.json", "added"), file("content/data/terrains/grass.json", "modified"), file("content/data/props/crate.json", "removed"), file("content/data/props/review_lamp.json", "added")];
        let heads = BTreeMap::from([
            ("content/data/terrains/lava.json".to_string(), Some(br##"{ "id": "lava", "name": "Lava", "pattern": "custom", "walk": "block", "color": "#ff5a10", "color2": "#ffb030", "side": "#2a2220" }"##.to_vec())),
            ("content/data/terrains/grass.json".to_string(), Some(br##"{ "id": "grass", "name": "Grass", "pattern": "grass", "walk": "walk", "color": "#4a8a3a", "color2": "#2e6a28", "priority": 2 }"##.to_vec())),
            ("content/data/props/crate.json".to_string(), None),
            ("content/data/props/review_lamp.json".to_string(), Some(br##"{ "id": "review_lamp", "name": "Review lamp", "category": "decoration", "footprint": [1, 1], "height": 1.2, "parts": [{ "shape": "box", "size": [0.3, 1.2, 0.3], "at": [0, 0, 0], "color": "#aa8844" }] }"##.to_vec())),
        ]);
        let bases = BTreeMap::from([
            ("content/data/terrains/grass.json".to_string(), br##"{ "id": "grass", "name": "Grass", "pattern": "grass", "walk": "walk", "color": "#4a8a3a", "color2": "#2e6a28", "priority": 1 }"##.to_vec()),
            ("content/data/props/crate.json".to_string(), br##"{ "id": "crate", "name": "Crate" }"##.to_vec()),
        ]);
        let opened = Box::new(Opened::build(pull, files, heads, bases));
        let mut ed = MapEditor::new_review(std::sync::Arc::new(game()), opened, ".");
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        crate::platform::clear_hooks();
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| ed.terrain_list(ui));
        out.textures_delta.clear();
        assert!(crate::platform::hook("uiChanged_terrains_grass").is_some() && crate::platform::hook("uiChanged_terrains_lava").is_some(), "marks by the terrains changed");
        assert!(crate::platform::hook("uiTerrain_lava").is_some(), "the terrain added, listed");
        assert_eq!(ed.review_changes("terrains", "grass", "priority").map(|(k, a)| (k, a)), Some((Kind::Modified, "modified priority: 1 -> 2".to_string())));
        assert!(ed.review_changes("terrains", "grass", "look").is_none() && ed.review_changes("terrains", "lava", "look").is_some());
        crate::platform::clear_hooks();
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| ed.assets_panel(ui));
        out.textures_delta.clear();
        assert!(crate::platform::hook("uiFolder_Changed").is_some());
        ed.assets.folder = Some(REVIEWED_FOLDER.into());
        crate::platform::clear_hooks();
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| ed.assets_panel(ui));
        out.textures_delta.clear();
        assert_eq!(crate::platform::hook("shown").as_deref(), Some("review_lamp"), "the prop added, alone in the folder (the one taken out is named under it)");
        assert_eq!(ed.reviewing().unwrap().name_of("props", "crate"), "Crate");
    }
}
