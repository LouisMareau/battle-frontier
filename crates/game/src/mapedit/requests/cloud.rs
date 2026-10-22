//! The browser's runs: a model request made in a sandbox of Anthropic's (Managed Agents) on the
//! user's own Claude API key (the Claude sign-in's), as run_claude.py makes one with Claude Code
//! on the desktop. The same recipe runs there: the sandbox's Claude checks out the commit the
//! site was built from, sets the sandbox up (tools/blender/cloud_run.py: the request's files,
//! Blender 3.1.2 on a virtual display), runs run_claude.py --prompt and follows the prompt it
//! writes, ends the run (--settle) and puts what it made in the session's outputs.
//!
//! The editor follows the run by the session's events (streamed, listed again whole after a
//! break): what Claude says and does goes into the request's log.txt as run_claude.py writes it
//! on the desktop, and what the request scripts print (`[request] {...}`, `[request-log]
//! {...}`) changes the request and its log as it would on disk there. Once the session's done
//! its outputs come back (manifest.json: the request's renders and script into its folder, the
//! props made and their models into the editor). The calls go through the site's own address
//! (functions/claude/), which passes the key on to Anthropic and keeps nothing. Made once a key,
//! and found again by their names: the agent (tools/blender/cloud_agent.md its system prompt)
//! and its environment (the packages Blender needs, the hosts the sandbox may reach).

// what's here is the browser's; the tests use the parts that aren't
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde_json::{json, Value};

use super::desk::How;

/// The agent's and its environment's names in the user's workspace.
pub(super) const AGENT: &str = "Battle Frontier model requests";
pub(super) const ENVIRONMENT: &str = "battle-frontier-model-requests";
/// What the agent does with a request.
const SYSTEM: &str = include_str!("../../../../../tools/blender/cloud_agent.md");
/// The beta Anthropic's agents, sessions and environments are under.
pub(super) const BETA: &str = "managed-agents-2026-04-01";
/// What Blender 3.1.2 needs to render on a virtual display (Xvfb, and OpenGL in software), and
/// a Pillow that draws the request scripts' captions.
const APT: [&str; 13] = ["xvfb", "xauth", "libgl1", "libgl1-mesa-dri", "libglu1-mesa", "libegl1", "libxi6", "libxxf86vm1", "libxfixes3", "libxrender1", "libxkbcommon0", "libsm6", "libice6"];
const PIP: [&str; 2] = ["pillow>=10", "numpy"];
/// The most a run may cost by default (cents): a creature has cost up to $7.40 so far.
pub(super) const CAP_CENTS: u32 = 1500;
/// The lines of a tool's answer shown in the log (as run_claude.py's).
const SHOWN: usize = 30;

/// The model a run asks for, as the window names them ("": the default).
pub(super) fn model_of(choice: &str) -> &'static str {
    match choice {
        "sonnet" => "claude-sonnet-5-5",
        "haiku" => "claude-haiku-5-5",
        _ => "claude-opus-5-5",
    }
}

/// The commit the site was built from (Cloudflare's, as it builds), else main.
pub(super) fn commit() -> &'static str {
    option_env!("CF_PAGES_COMMIT_SHA").unwrap_or("main")
}

/// The agent, as the recipe makes it (`recipe`: its version, kept with it).
pub(super) fn agent_body(recipe: &str) -> Value {
    json!({
        "name": AGENT,
        "description": "Makes a Battle Frontier model request from its pictures in Blender, as the desktop editor's runs do",
        "model": {"id": model_of(""), "effort": "high"},
        "system": SYSTEM,
        // unattended: nobody's there to answer a call that waits; in the user's own sandbox,
        // which holds no secret (the key stays out of it), its network the package registries,
        // GitHub and Blender's downloads only
        "tools": [{
            "type": "agent_toolset_20260401",
            "default_config": {"enabled": true, "permission_policy": {"type": "always_allow"}},
            "configs": [{"name": "web_fetch", "enabled": false}, {"name": "web_search", "enabled": false}],
        }],
        "metadata": {"app": "battle-frontier", "recipe": recipe},
    })
}

/// The agent's environment: its sandbox's packages and the hosts it may reach.
pub(super) fn environment_body(recipe: &str) -> Value {
    json!({
        "name": ENVIRONMENT,
        "description": "Battle Frontier's model requests: Blender 3.1.2 on a virtual display",
        "config": {
            "type": "cloud",
            "networking": {"type": "limited", "allow_package_managers": true, "allowed_hosts": ["download.blender.org"]},
            "packages": {"apt": APT, "pip": PIP},
        },
        "metadata": {"app": "battle-frontier", "recipe": recipe},
    })
}

/// The recipe's version: the agent and its environment as it makes them (made again when it changes).
pub(super) fn recipe() -> String {
    format!("{:08x}", crc32fast::hash(format!("{}{}", agent_body(""), environment_body("")).as_bytes()))
}

/// run_claude.py's flags for how a run goes (`noted`: with the user's note, uploaded).
pub(super) fn flags(how: How, noted: bool) -> String {
    let mut out = match how {
        How::Whole => "",
        How::GoOn => " --go-on",
        How::Again => " --again",
        How::Variants => " --variants",
    }
    .to_string();
    if noted {
        out += " --note-file /mnt/session/uploads/note.txt";
    }
    out
}

/// The session's first message: the request, and the commands that make it, in order.
pub(super) fn first_message(id: &str, name: &str, how: How, noted: bool) -> String {
    let what = match how {
        How::Whole => "its whole recipe",
        How::GoOn => "on from its lines, which the user has checked",
        How::Again => "made again from the model the last run made, with the user's comments",
        How::Variants => "variants of the model it made",
    };
    let (flags, at, repository) = (flags(how, noted), "cd /workspace/bf &&", format!("https://github.com/{}/{}.git", super::super::publish::OWNER, super::super::publish::REPO));
    format!(
        "Make the model request \"{name}\" ({id}): {what}. Its files are in /mnt/session/uploads/request/ (and its props' own, \
         when it has made any, in /mnt/session/uploads/project/: setup puts them in the project).\n\n\
         Run these in order, as your instructions say:\n\
         1. git clone --quiet --filter=blob:none --no-checkout {repository} /workspace/bf && {at} git checkout --quiet {commit}\n\
         2. {at} python3 tools/blender/cloud_run.py setup {id}\n\
         3. {at} python3 tools/blender/run_claude.py {id} --prompt{flags}\n   \
         then read the file it names, whole, and follow it from /workspace/bf to its end\n\
         4. {at} python3 tools/blender/run_claude.py {id} --settle{flags} --reply \"<your one line on what you made>\"\n\
         5. {at} python3 tools/blender/cloud_run.py outputs {id}\n",
        commit = commit()
    )
}

/// What a run asks of its session: the model, its first message, its request (its id), its
/// title, the most it may cost (cents).
pub(super) struct Ask {
    pub model: String,
    pub text: String,
    pub request: String,
    pub title: String,
    pub cap: u32,
}

/// A session for a run: the agent (on the model asked for), its environment, the files it's
/// given (each its id and where it's mounted, under /mnt/session/uploads/), its first message,
/// what it may cost at most.
pub(super) fn session_body(agent: &str, environment: &str, files: &[(String, String)], ask: &Ask) -> Value {
    json!({
        "agent": {"type": "agent_with_overrides", "id": agent, "model": {"id": ask.model, "effort": "high"}},
        "environment_id": environment,
        "title": ask.title,
        "resources": files.iter().map(|(id, at)| json!({"type": "file", "file_id": id, "mount_path": format!("/{at}")})).collect::<Vec<_>>(),
        "budget": {"type": "limit", "max_list_cost": {"amount": ask.cap.to_string(), "currency": "USD"}},
        "metadata": {"app": "battle-frontier", "request": ask.request},
        "initial_events": [{"type": "user.message", "content": [{"type": "text", "text": ask.text}]}],
    })
}

/// A file uploaded as Anthropic's Files API takes one: a form with the file, under `boundary`.
/// The body, and its content type.
pub(super) fn form(name: &str, bytes: &[u8], boundary: &str) -> (Vec<u8>, String) {
    let lower = name.to_lowercase();
    let kind = match lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "json" => "application/json",
        "txt" | "py" | "md" => "text/plain",
        _ => "application/octet-stream",
    };
    let safe: String = name.chars().map(|c| if c == '"' || c == '\\' || c.is_control() { '_' } else { c }).collect();
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe}\"\r\nContent-Type: {kind}\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/form-data; boundary={boundary}"))
}

/// What one of the session's events says, for the request: lines for its log (as run_claude.py
/// writes Claude Code's), the request's fields as the scripts wrote them (each with its "id"),
/// the lines they logged (by request), how the session stopped when it did, what it's cost so
/// far (cents).
#[derive(Debug, Default, PartialEq)]
pub(super) struct Followed {
    pub log: Vec<String>,
    pub records: Vec<Value>,
    pub logged: Vec<(String, String)>,
    pub end: Option<End>,
    pub cost: Option<f64>,
}

/// How a session stopped: idle, and why (`end_turn`: done; `budget_reached`: at its cap), or
/// over for good.
#[derive(Debug, PartialEq)]
pub(super) enum End {
    Idle(String),
    Over,
}

/// A tool's answer as text (a picture as a word).
fn flat(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p.get("type").and_then(Value::as_str) {
                Some("text") => p.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
                Some("image") => "(a picture)".to_string(),
                _ => p.to_string().chars().take(200).collect(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Every text an event holds (a tool's answer found wherever it is), in order.
fn texts(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| texts(x, out)),
        Value::Object(o) => o.iter().filter(|(k, _)| !matches!(k.as_str(), "id" | "type" | "processed_at" | "tool_use_id")).for_each(|(_, x)| texts(x, out)),
        _ => {}
    }
}

/// A tool's answer as text: its content (or its output), else every text the event holds.
fn answer_of(event: &Value) -> String {
    match event.get("content").or_else(|| event.get("output")) {
        Some(c) if !flat(c).is_empty() => flat(c),
        _ => {
            let mut all = vec![];
            texts(event, &mut all);
            all.join("\n")
        }
    }
}

/// Where a run's outputs may go in the project: a request's own folder (its files), a prop's
/// record or its model, in the game's own space or the placeholders'. Nowhere else.
pub(in super::super) fn may_write(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        return false;
    }
    match parts.as_slice() {
        ["private", "requests", _, _] => true,
        [space, "data", "props", file] => matches!(*space, "content" | "temporary") && file.ends_with(".json"),
        [space, "assets", "models", _] => matches!(*space, "content" | "temporary"),
        _ => false,
    }
}

/// A text's first `n` lines, and how many more there were.
fn clip(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        return text.to_string();
    }
    format!("{}\n... ({} more lines)", lines[..n].join("\n"), lines.len() - n)
}

/// A tool call in a line: its command, its file or its pattern.
fn what(input: &Value) -> String {
    for key in ["command", "file_path", "path", "pattern"] {
        if let Some(v) = input.get(key).and_then(Value::as_str) {
            return v.to_string();
        }
    }
    input.to_string().chars().take(300).collect()
}

/// What one of the session's events says (see `Followed`).
pub(super) fn follow(event: &Value) -> Followed {
    let mut out = Followed::default();
    match event.get("type").and_then(Value::as_str).unwrap_or("") {
        "agent.message" => {
            let text = flat(event.get("content").unwrap_or(&Value::Null));
            if !text.trim().is_empty() {
                out.log.push(format!("Claude: {}", text.trim()));
            }
        }
        "agent.tool_use" => {
            let name = event.get("name").and_then(Value::as_str).unwrap_or("a tool");
            out.log.push(format!("→ {name}: {}", what(event.get("input").unwrap_or(&Value::Null))));
        }
        "agent.tool_result" => {
            let (answer, mut rest) = (answer_of(event), vec![]);
            for line in answer.lines() {
                if let Some(v) = line.strip_prefix("[request] ").and_then(|j| serde_json::from_str::<Value>(j).ok()) {
                    out.records.push(v);
                } else if let Some(v) = line.strip_prefix("[request-log] ").and_then(|j| serde_json::from_str::<Value>(j).ok()) {
                    if let (Some(id), Some(text)) = (v.get("id").and_then(Value::as_str), v.get("text").and_then(Value::as_str)) {
                        out.logged.push((id.to_string(), text.to_string()));
                    }
                } else {
                    rest.push(line);
                }
            }
            let shown = rest.join("\n");
            // (an answer that's only the scripts' lines for the editor says nothing more)
            if !(shown.trim().is_empty() && (!out.records.is_empty() || !out.logged.is_empty())) {
                let mark = if event.get("is_error").and_then(Value::as_bool) == Some(true) { "× " } else { "← " };
                out.log.push(format!("{mark}{}", if shown.trim().is_empty() { "(nothing)".to_string() } else { clip(shown.trim(), SHOWN) }));
            }
        }
        "agent.thread_context_compacted" => out.log.push("(Claude's earlier turns summed up, so the rest fits)".into()),
        "session.error" => {
            let e = event.get("error").unwrap_or(event);
            out.log.push(format!("(the session says) {}", e.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| e.to_string())));
        }
        "session.usage" => out.cost = event.pointer("/list_cost/amount").and_then(|a| a.as_str().and_then(|s| s.parse().ok()).or_else(|| a.as_f64())),
        "session.status_idle" => out.end = Some(End::Idle(event.pointer("/stop_reason/type").and_then(Value::as_str).unwrap_or("").to_string())),
        "session.status_terminated" => out.end = Some(End::Over),
        _ => {}
    }
    out
}

/// The events whole in `bytes` (the stream's so far), taken out of them: each one's `data:`
/// lines read as JSON (what's left waits for the rest of its event).
pub(super) fn events(bytes: &mut Vec<u8>) -> Vec<Value> {
    let mut out = vec![];
    while let Some(end) = bytes.windows(2).position(|w| w == b"\n\n") {
        let block: Vec<u8> = bytes.drain(..end + 2).collect();
        let text = String::from_utf8_lossy(&block);
        let data: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect();
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            out.push(v);
        }
    }
    out
}

/// A request's record with the fields the scripts wrote put on it (`record`: its request.json as
/// it is here; none for one a run made, a split's, which comes whole).
pub(super) fn merged(record: Option<&str>, fields: &Value) -> Option<super::ModelRequest> {
    let mut out = match record {
        Some(text) => serde_json::from_str::<Value>(text).ok()?,
        None => json!({}),
    };
    let (Some(o), Some(f)) = (out.as_object_mut(), fields.as_object()) else { return None };
    for (k, v) in f {
        o.insert(k.clone(), v.clone());
    }
    serde_json::from_value(out).ok()
}

/// Lines as log.txt has them: `[2026-10-21 09:41:07] text`, a text of several lines indented
/// under its time (as model_requests.py's `log` writes them).
pub(super) fn log_lines(at: &str, text: &str) -> String {
    let mut lines = text.lines();
    let mut out = format!("[{at}] {}\n", lines.next().unwrap_or(""));
    for line in lines {
        out += &format!("{}{line}\n", " ".repeat(at.chars().count() + 3));
    }
    out
}

/// A key's own name for what the editor keeps of it here (its agent's and environment's ids):
/// a sum of it, not the key.
pub(super) fn key_name(key: &str) -> String {
    format!("claude_agent_{:08x}", crc32fast::hash(key.trim().as_bytes()))
}

#[cfg(target_arch = "wasm32")]
pub(super) use run::Runner;

/// The runs themselves: the browser's calls to Anthropic, and the request's files as they go.
#[cfg(target_arch = "wasm32")]
mod run {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};

    use super::super::desk::{folder, load_all, now_secs, read, save, How};
    use super::super::{best_and_worst, disk, ModelRequest, Progress, Status};
    use super::*;
    use crate::mapedit::claude::{key, RELAY, VERSION};

    /// What a run's tasks hand the window: its session once made, the stream's bytes, the
    /// events listed on (re)connecting, its outputs once fetched, what went wrong.
    #[derive(Default)]
    struct Shared {
        session: Option<String>,
        inputs: Vec<String>,
        bytes: Vec<u8>,
        streaming: bool,
        listed: Option<Vec<Value>>,
        outputs: Option<Outputs>,
        trouble: Option<String>,
    }

    /// A run's outputs fetched: each file's path in the project and its bytes, or why not.
    type Outputs = Result<Vec<(String, Vec<u8>)>, String>;

    #[derive(Clone, Copy, PartialEq)]
    enum Stage {
        /// the agent made, the files uploaded, the session made: on their way
        Starting,
        /// the session's events followed
        Following,
        /// its outputs on their way back
        Fetching,
    }

    struct Run {
        id: String,
        key: String,
        shared: Arc<Mutex<Shared>>,
        stage: Stage,
        seen: HashSet<String>,
        /// the stream to stop (the run over, cancelled)
        stop: Arc<AtomicBool>,
        /// when the stream broke (s): opened again a few seconds later
        broke: Option<f64>,
        /// what it's cost so far (cents), and what the log last said of it
        cost: f64,
        told: f64,
        /// the most it may cost (cents)
        cap: u32,
        /// the events listed first are what the log has already (a page opened again): only
        /// their changes to the request taken, not their lines
        quiet: bool,
    }

    /// The browser's runs: one at a time, as the desktop's.
    #[derive(Default)]
    pub(in super::super) struct Runner {
        run: Option<Run>,
        /// the props and models a run made, for the editor to take in (paths in the project)
        pub made: Vec<(String, Vec<u8>)>,
        /// a run the page was following before it was opened again, looked for once
        looked: bool,
        /// what a run's cap is (cents): the window's
        pub cap: u32,
    }

    /// A call to Anthropic through the site's address (`body`: its bytes and their type).
    fn call(method: ehttp::Method, path: &str, key: &str, body: Option<(Vec<u8>, String)>) -> ehttp::Request {
        let mut r = ehttp::Request::new(method, format!("{RELAY}/{path}"), &[("x-api-key", key), ("anthropic-version", VERSION), ("anthropic-beta", BETA)]);
        if let Some((bytes, kind)) = body {
            r.headers.insert("content-type", kind);
            r.body = bytes;
        }
        r
    }

    /// Anthropic's error in a line.
    fn refused(path: &str, status: u16, text: &str) -> String {
        let said = serde_json::from_str::<Value>(text).ok().and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| text.chars().take(200).collect());
        match status {
            401 | 403 => format!("Anthropic turned the key away ({status}: {said}): check it in the Claude sign-in"),
            404 if path.contains("agents") || path.contains("sessions") || path.contains("environments") => {
                format!("Anthropic's agents aren't open to this key's workspace ({said})")
            }
            _ => format!("{path}: {status} {said}"),
        }
    }

    async fn json_of(method: ehttp::Method, path: &str, key: &str, body: Option<Value>) -> Result<Value, String> {
        let body = body.map(|b| (b.to_string().into_bytes(), "application/json".to_string()));
        let answer = ehttp::fetch_async(call(method, path, key, body)).await.map_err(|e| format!("{path}: {e}"))?;
        let text = String::from_utf8_lossy(&answer.bytes).to_string();
        if !answer.ok {
            return Err(refused(path, answer.status, &text));
        }
        serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
    }

    /// Every item of a paged list (Anthropic's `page`/`next_page`, or `after_id`/`has_more`).
    async fn all_of(path: &str, key: &str) -> Result<Vec<Value>, String> {
        let mut out = vec![];
        let mut next: Option<String> = None;
        for _ in 0..50 {
            let sep = if path.contains('?') { '&' } else { '?' };
            let url = match &next {
                Some(n) => format!("{path}{sep}{n}"),
                None => path.to_string(),
            };
            let page = json_of(ehttp::Method::GET, &url, key, None).await?;
            let items = page.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
            out.extend(items.iter().cloned());
            next = match (page.get("next_page").and_then(Value::as_str), page.get("has_more").and_then(Value::as_bool), page.get("last_id").and_then(Value::as_str)) {
                (Some(p), _, _) => Some(format!("page={p}")),
                (None, Some(true), Some(last)) => Some(format!("after_id={last}")),
                _ => None,
            };
            if next.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// The agent or the environment named so in the key's workspace, made (or brought up to
    /// the recipe) when it isn't: its id.
    async fn made(kind: &str, name: &str, body: Value, recipe: &str, key: &str) -> Result<String, String> {
        let found = all_of(&format!("v1/{kind}?limit=100"), key).await?.into_iter().find(|v| v.get("name").and_then(Value::as_str) == Some(name) && v.get("archived_at").is_none_or(Value::is_null));
        match found {
            Some(v) => {
                let id = v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                if v.pointer("/metadata/recipe").and_then(Value::as_str) != Some(recipe) {
                    json_of(ehttp::Method::POST, &format!("v1/{kind}/{id}"), key, Some(body)).await?;
                }
                Ok(id)
            }
            None => json_of(ehttp::Method::POST, &format!("v1/{kind}"), key, Some(body)).await?.get("id").and_then(Value::as_str).map(str::to_string).ok_or(format!("Anthropic made no {kind} id")),
        }
    }

    /// The agent and its environment, as kept for this key (made, or found, once a recipe).
    async fn agent(key: &str) -> Result<(String, String), String> {
        let recipe = recipe();
        let name = key_name(key);
        if let Some(kept) = crate::platform::stored_text(&name).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            if kept.get("recipe").and_then(Value::as_str) == Some(recipe.as_str()) {
                if let (Some(a), Some(e)) = (kept.get("agent").and_then(Value::as_str), kept.get("environment").and_then(Value::as_str)) {
                    return Ok((a.to_string(), e.to_string()));
                }
            }
        }
        let environment = made("environments", ENVIRONMENT, environment_body(&recipe), &recipe, key).await?;
        let agent = made("agents", AGENT, agent_body(&recipe), &recipe, key).await?;
        let _ = crate::platform::store_text(&name, &json!({"recipe": recipe, "agent": agent, "environment": environment}).to_string());
        Ok((agent, environment))
    }

    /// A file uploaded to the key's Files: its id.
    async fn upload(name: &str, bytes: &[u8], key: &str) -> Result<String, String> {
        let boundary = format!("bf{:x}{:x}", (js_sys::Date::now() * 1000.0) as u64, crc32fast::hash(name.as_bytes()));
        let (body, kind) = form(name, bytes, &boundary);
        let answer = ehttp::fetch_async(call(ehttp::Method::POST, "v1/files", key, Some((body, kind)))).await.map_err(|e| format!("{name}: {e}"))?;
        let text = String::from_utf8_lossy(&answer.bytes).to_string();
        if !answer.ok {
            return Err(refused("v1/files", answer.status, &text));
        }
        serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string)).ok_or(format!("{name}: no file id"))
    }

    /// The run's start: the agent, the files uploaded, the session made with its first message.
    async fn begin(shared: Arc<Mutex<Shared>>, key: String, files: Vec<(String, Vec<u8>)>, ask: Ask) {
        let went = async {
            let (agent, environment) = agent(&key).await?;
            let mut mounted = vec![];
            for (at, bytes) in &files {
                let name = at.rsplit('/').next().unwrap_or(at);
                let id = upload(name, bytes, &key).await?;
                shared.lock().unwrap().inputs.push(id.clone());
                mounted.push((id, at.clone()));
            }
            let body = session_body(&agent, &environment, &mounted, &ask);
            let session = json_of(ehttp::Method::POST, "v1/sessions", &key, Some(body)).await?;
            session.get("id").and_then(Value::as_str).map(str::to_string).ok_or("Anthropic made no session".to_string())
        }
        .await;
        let mut s = shared.lock().unwrap();
        match went {
            Ok(session) => s.session = Some(session),
            Err(e) => s.trouble = Some(e),
        }
    }

    /// The session's events so far (on connecting, after a break: what the stream missed).
    async fn listed(shared: Arc<Mutex<Shared>>, session: String, key: String) {
        let got = all_of(&format!("v1/sessions/{session}/events?beta=true&limit=1000"), &key).await;
        let mut s = shared.lock().unwrap();
        match got {
            Ok(events) => s.listed = Some(events),
            Err(e) => s.trouble = Some(e),
        }
    }

    /// The session's events as they come, their bytes handed to the window.
    fn stream(shared: Arc<Mutex<Shared>>, session: &str, key: &str, stop: Arc<AtomicBool>) {
        shared.lock().unwrap().streaming = true;
        let mut req = call(ehttp::Method::GET, &format!("v1/sessions/{session}/events/stream?beta=true"), key, None);
        req.headers.insert("accept", "text/event-stream");
        req.timeout = None;
        ehttp::streaming::fetch(req, move |part| {
            let mut s = shared.lock().unwrap();
            if stop.load(Ordering::Relaxed) {
                s.streaming = false;
                return std::ops::ControlFlow::Break(());
            }
            match part {
                Ok(ehttp::streaming::Part::Response(r)) if r.ok => std::ops::ControlFlow::Continue(()),
                Ok(ehttp::streaming::Part::Chunk(bytes)) if !bytes.is_empty() => {
                    s.bytes.extend(bytes.iter().copied().filter(|b| *b != b'\r'));
                    std::ops::ControlFlow::Continue(())
                }
                _ => {
                    s.streaming = false;
                    std::ops::ControlFlow::Break(())
                }
            }
        });
    }

    /// The session's outputs (its manifest.json says where each goes in the project), fetched,
    /// each taken off the key's Files once it's here, the run's uploads too.
    async fn fetch_outputs(shared: Arc<Mutex<Shared>>, session: String, key: String) {
        let got = async {
            let mut listed = vec![];
            // (the outputs are listed a moment after the session's idle)
            for wait in [0, 2000, 4000, 8000] {
                if wait > 0 {
                    pause(wait).await;
                }
                listed = all_of(&format!("v1/files?scope_id={session}&limit=100"), &key).await?;
                if listed.iter().any(|f| f.get("filename").and_then(Value::as_str).is_some_and(|n| n.ends_with("manifest.json"))) {
                    break;
                }
            }
            let named = |n: &str| listed.iter().find(|f| f.get("filename").and_then(Value::as_str).is_some_and(|f| f == n || f.ends_with(&format!("/{n}")))).and_then(|f| f.get("id").and_then(Value::as_str)).map(str::to_string);
            let manifest_id = named("manifest.json").ok_or("the run sent nothing back (no manifest.json in its outputs)")?;
            let manifest: Value = serde_json::from_slice(&download(&manifest_id, &key).await?).map_err(|e| format!("manifest.json: {e}"))?;
            let mut out = vec![];
            for f in manifest.get("files").and_then(Value::as_array).cloned().unwrap_or_default() {
                let (Some(name), Some(path)) = (f.get("name").and_then(Value::as_str), f.get("path").and_then(Value::as_str)) else { continue };
                // only where a run's own go (a request's folder, a prop's record and model)
                if !may_write(path) {
                    continue;
                }
                if let Some(id) = named(name) {
                    out.push((path.to_string(), download(&id, &key).await?));
                }
            }
            for f in &listed {
                if let Some(id) = f.get("id").and_then(Value::as_str) {
                    let _ = ehttp::fetch_async(call(ehttp::Method::DELETE, &format!("v1/files/{id}"), &key, None)).await;
                }
            }
            Ok(out)
        }
        .await;
        let inputs = std::mem::take(&mut shared.lock().unwrap().inputs);
        for id in inputs {
            let _ = ehttp::fetch_async(call(ehttp::Method::DELETE, &format!("v1/files/{id}"), &key, None)).await;
        }
        shared.lock().unwrap().outputs = Some(got);
    }

    async fn download(id: &str, key: &str) -> Result<Vec<u8>, String> {
        let answer = ehttp::fetch_async(call(ehttp::Method::GET, &format!("v1/files/{id}/content"), key, None)).await.map_err(|e| format!("{id}: {e}"))?;
        if !answer.ok {
            return Err(refused("v1/files", answer.status, &String::from_utf8_lossy(&answer.bytes)));
        }
        Ok(answer.bytes)
    }

    /// A wait, on the page's timer (ms).
    async fn pause(ms: i32) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
            }
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }

    /// The page's clock as log.txt has it: the user's local time.
    fn stamp() -> String {
        let d = js_sys::Date::new_0();
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", d.get_full_year(), d.get_month() + 1, d.get_date(), d.get_hours(), d.get_minutes(), d.get_seconds())
    }

    /// Lines added to a request's log.txt.
    fn log(root: &Path, id: &str, text: &str) {
        let path = folder(root).join(id).join("log.txt");
        let mut all = disk::read_to_string(&path).unwrap_or_default();
        all += &log_lines(&stamp(), text);
        let _ = disk::write(&path, all);
    }

    /// The run's record of itself, in the request's folder: its session, so a page opened again
    /// follows it on.
    fn note_path(root: &Path, id: &str) -> PathBuf {
        folder(root).join(id).join("cloud.json")
    }

    /// The request's files a run is given: its folder's (its log and the run's own notes aside),
    /// each at request/NAME; the props it made, kept in this browser, at project/PATH; and what
    /// the ratings teach of its kind (requests.md, "What the ratings teach"): its best and worst
    /// rated models' records, renders and scripts, and the kind's lessons, at project/PATH.
    fn given(root: &Path, r: &ModelRequest) -> Vec<(String, Vec<u8>)> {
        let mut out = vec![];
        for e in disk::read_dir(&folder(root).join(&r.id)).unwrap_or_default() {
            if e.dir || ["log.txt", "run.jsonl", "claude-settings.json", "stop", "cloud.json"].contains(&e.name.as_str()) {
                continue;
            }
            if let Ok(bytes) = disk::read(&e.path) {
                out.push((format!("request/{}", e.name), bytes));
            }
        }
        let props: Vec<&String> = std::iter::once(&r.prop).chain(r.variant_props.iter()).filter(|p| !p.is_empty()).collect();
        for pid in props {
            for space in ["content", "temporary"] {
                let record = format!("{space}/data/props/{pid}.json");
                let Some(bytes) = crate::keep::get(&record) else { continue };
                if let Some(model) = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| v.get("model").and_then(Value::as_str).map(str::to_string)) {
                    let path = format!("{space}/assets/{model}");
                    if let Some(m) = crate::keep::get(&path) {
                        out.push((format!("project/{path}"), m.to_vec()));
                    }
                }
                out.push((format!("project/{record}"), bytes.to_vec()));
            }
        }
        let (all, _) = load_all(root);
        let (best, worst) = best_and_worst(&all, r.kind, &r.id);
        for q in best.into_iter().chain(worst) {
            let names = std::iter::once("request.json".to_string()).chain(q.previews.iter().cloned()).chain(std::iter::once("model.py".to_string()));
            for name in names {
                if let Ok(bytes) = disk::read(folder(root).join(&q.id).join(&name)) {
                    out.push((format!("project/private/requests/{}/{name}", q.id), bytes));
                }
            }
        }
        let word = serde_json::to_value(r.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        if let Ok(bytes) = disk::read(folder(root).join(".lessons").join(format!("{word}.md"))) {
            out.push((format!("project/private/requests/.lessons/{word}.md"), bytes));
        }
        out
    }

    impl Runner {
        /// The request a run is on, if any.
        pub fn busy(&self) -> Option<&str> {
            self.run.as_ref().map(|r| r.id.as_str())
        }

        /// A run started as run_claude.py's `args` ask (the request, how, the user's note, the
        /// model): the request in progress at once, the rest on its way.
        pub fn start(&mut self, root: &Path, args: &[String]) -> Result<(), String> {
            if let Some(other) = self.busy() {
                return Err(format!("one at a time: {other} is being made"));
            }
            let key = key().ok_or("no Claude API key in this browser: the Claude sign-in takes one")?;
            let id = args.get(1).cloned().ok_or("no request")?;
            let how = if args.iter().any(|a| a == "--go-on") {
                How::GoOn
            } else if args.iter().any(|a| a == "--again") {
                How::Again
            } else if args.iter().any(|a| a == "--variants") {
                How::Variants
            } else {
                How::Whole
            };
            let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|k| args.get(k + 1)).cloned().unwrap_or_default();
            let (note, model) = (after("--note"), model_of(&after("--model")).to_string());
            let mut r = read(root, &id)?;
            let mut files = given(root, &r);
            if !note.trim().is_empty() {
                files.push(("note.txt".into(), note.clone().into_bytes()));
            }
            let text = first_message(&r.id, &r.name, how, !note.trim().is_empty());
            let cap = if self.cap > 0 { self.cap } else { CAP_CENTS };
            r.status = Status::InProgress;
            r.progress = Progress { step: "the sandbox is starting".into(), done: 0.02, started: now_secs().round(), minutes: r.progress.minutes.max(10.0) };
            save(root, &r)?;
            log(root, &id, &format!("———————— in the cloud ————————\nA sandbox of Anthropic's, on your API key: {model}, at most ${:.2} for this run", cap as f64 / 100.0));
            let shared = Arc::new(Mutex::new(Shared::default()));
            let ask = Ask { model, text, request: id.clone(), title: format!("{} (Battle Frontier model request)", r.name), cap };
            wasm_bindgen_futures::spawn_local(begin(shared.clone(), key.clone(), files, ask));
            self.run = Some(Run { id, key, shared, stage: Stage::Starting, seen: HashSet::new(), stop: Arc::new(AtomicBool::new(false)), broke: None, cost: 0.0, told: 0.0, cap, quiet: false });
            Ok(())
        }

        /// Each look (a second or so): the run followed on (its events into the request's files),
        /// cancelled when its stop file's there, ended once the session's done. What to say.
        pub fn poll(&mut self, root: &Path) -> Option<String> {
            if !self.looked {
                self.looked = true;
                self.resume(root);
            }
            let run = self.run.as_mut()?;
            let id = run.id.clone();
            let (session, trouble, listed, outputs, streaming, bytes) = {
                let mut s = run.shared.lock().unwrap();
                (s.session.clone(), s.trouble.take(), s.listed.take(), s.outputs.take(), s.streaming, std::mem::take(&mut s.bytes))
            };
            if let Some(why) = trouble.filter(|_| run.stage != Stage::Fetching) {
                return Some(self.fail(root, &id, &why));
            }
            // cancelled from the window: the session stopped, the request waits again
            let stop = folder(root).join(&id).join("stop");
            if disk::is_file(&stop) {
                if let Some(session) = &session {
                    let event = json!({"events": [{"type": "user.interrupt"}]}).to_string().into_bytes();
                    let req = call(ehttp::Method::POST, &format!("v1/sessions/{session}/events"), &run.key, Some((event, "application/json".into())));
                    ehttp::fetch(req, |_| {});
                }
                run.stop.store(true, Ordering::Relaxed);
                let _ = disk::remove_file(&stop);
                let _ = disk::remove_file(note_path(root, &id));
                if let Ok(mut r) = read(root, &id) {
                    r.status = Status::Waiting;
                    r.progress = Progress::default();
                    let _ = save(root, &r);
                }
                log(root, &id, "Cancelled from the editor: Claude stopped, the request waits again");
                self.run = None;
                return Some(format!("{id}: cancelled; it waits again"));
            }
            match run.stage {
                Stage::Starting => {
                    let session = session?;
                    let _ = disk::write(note_path(root, &id), json!({"session": session, "cap": run.cap}).to_string());
                    log(root, &id, &format!("The session: {session} (the Claude Console's Sessions show it too)"));
                    run.stage = Stage::Following;
                    self.connect();
                    None
                }
                Stage::Following => {
                    let mut said = None;
                    let quiet = std::mem::replace(&mut run.quiet, false) && listed.is_some();
                    let history = listed.unwrap_or_default();
                    let mut bytes = bytes;
                    let streamed = super::events(&mut bytes);
                    if !bytes.is_empty() {
                        // the rest of an event, for the next look
                        let mut s = run.shared.lock().unwrap();
                        let more = std::mem::take(&mut s.bytes);
                        s.bytes = bytes;
                        s.bytes.extend(more);
                    }
                    let mut end = None;
                    let n = history.len();
                    for (k, e) in history.into_iter().chain(streamed).enumerate() {
                        let eid = e.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                        if !eid.is_empty() && !run.seen.insert(eid) {
                            continue;
                        }
                        let mut f = follow(&e);
                        if quiet && k < n {
                            (f.log, f.logged) = (vec![], vec![]);
                        }
                        apply(root, &id, &f);
                        if let Some(c) = f.cost {
                            run.cost = c;
                        }
                        if f.end.is_some() {
                            end = f.end;
                        }
                    }
                    if run.cost - run.told >= 50.0 {
                        run.told = run.cost;
                        log(root, &id, &format!("So far: ${:.2} of the run's ${:.2}", run.cost / 100.0, run.cap as f64 / 100.0));
                    }
                    match end {
                        Some(End::Idle(why)) if why == "budget_reached" => {
                            let cap = run.cap as f64 / 100.0;
                            return Some(self.fail(root, &id, &format!("stopped at its cap of ${cap:.2} (the window's \"at most\"): raise it and Start again")));
                        }
                        Some(End::Idle(why)) if why == "requires_action" => {}
                        Some(End::Idle(_)) | Some(End::Over) => {
                            log(root, &id, &format!("The session's done: ${:.2} at the API's prices. Its outputs coming back…", run.cost / 100.0));
                            run.stop.store(true, Ordering::Relaxed);
                            run.stage = Stage::Fetching;
                            let session = session.unwrap_or_default();
                            wasm_bindgen_futures::spawn_local(fetch_outputs(run.shared.clone(), session, run.key.clone()));
                            said = Some(format!("{id}: the run's done, its outputs coming back"));
                        }
                        None => {}
                    }
                    // the stream broke: opened again (after what it missed) a few seconds on
                    if run.stage == Stage::Following && !streaming {
                        let now = now_secs();
                        match run.broke {
                            None => run.broke = Some(now),
                            Some(at) if now - at > 4.0 => {
                                run.broke = None;
                                self.connect();
                            }
                            _ => {}
                        }
                    }
                    said
                }
                Stage::Fetching => {
                    let outputs = outputs?;
                    self.run = None;
                    let _ = disk::remove_file(note_path(root, &id));
                    match outputs {
                        Ok(files) => {
                            let mut back = 0;
                            for (path, bytes) in files {
                                if path.starts_with("private/requests/") {
                                    let _ = disk::write(Path::new(&path), &bytes);
                                } else {
                                    self.made.push((path, bytes));
                                }
                                back += 1;
                            }
                            log(root, &id, &format!("{back} files back from the cloud"));
                            // a run that ended without settling: failed, as a run here does
                            if read(root, &id).is_ok_and(|r| r.status == Status::InProgress) {
                                return Some(self.fail(root, &id, "the run ended without saying how (the log says what it did)"));
                            }
                            let r = read(root, &id).ok();
                            Some(match r.map(|r| (r.status, r.name)) {
                                Some((Status::Done, name)) => format!("{name}: made in the cloud"),
                                Some((_, name)) => format!("{name}: the run's over (the console says how)"),
                                None => format!("{id}: the run's over"),
                            })
                        }
                        Err(e) => Some(self.fail(root, &id, &e)),
                    }
                }
            }
        }

        /// The session's events: those so far, listed, then the stream.
        fn connect(&mut self) {
            let Some(run) = self.run.as_mut() else { return };
            let Some(session) = run.shared.lock().unwrap().session.clone() else { return };
            run.stop = Arc::new(AtomicBool::new(false));
            wasm_bindgen_futures::spawn_local(listed(run.shared.clone(), session.clone(), run.key.clone()));
            stream(run.shared.clone(), &session, &run.key, run.stop.clone());
        }

        /// A run the page was following when it closed (a request in progress with its
        /// session noted): followed on.
        fn resume(&mut self, root: &Path) {
            let Some(key) = key() else { return };
            for e in disk::read_dir(&folder(root)).unwrap_or_default() {
                let note = note_path(root, &e.name);
                let Some(kept) = disk::read_to_string(&note).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
                let (Some(session), Ok(r)) = (kept.get("session").and_then(Value::as_str), read(root, &e.name)) else { continue };
                if r.status != Status::InProgress {
                    let _ = disk::remove_file(&note);
                    continue;
                }
                let shared = Arc::new(Mutex::new(Shared { session: Some(session.to_string()), ..Default::default() }));
                let cap = kept.get("cap").and_then(Value::as_u64).unwrap_or(CAP_CENTS as u64) as u32;
                // (what it did before is in its log already, up to when the page closed: only its
                // changes to the request are taken from it, then what's new)
                self.run = Some(Run { id: r.id.clone(), key: key.clone(), shared, stage: Stage::Following, seen: HashSet::new(), stop: Arc::new(AtomicBool::new(false)), broke: None, cost: 0.0, told: 0.0, cap, quiet: true });
                self.connect();
                log(root, &r.id, "Followed on: the page was opened again (what the run did while it was closed is in the Claude Console's session)");
                return;
            }
        }

        /// The run failed, and why (the request failed with it, as run_claude.py's are).
        fn fail(&mut self, root: &Path, id: &str, why: &str) -> String {
            if let Some(run) = self.run.take() {
                run.stop.store(true, Ordering::Relaxed);
            }
            let _ = disk::remove_file(note_path(root, id));
            if let Ok(mut r) = read(root, id) {
                r.status = Status::Failed;
                r.progress.step = format!("failed: {why}");
                let _ = save(root, &r);
            }
            log(root, id, &format!("FAILED: {why}"));
            format!("{id}: failed, {why}")
        }
    }

    /// An event's lines in the log, the request's fields as the scripts wrote them, theirs in
    /// their logs.
    fn apply(root: &Path, id: &str, f: &Followed) {
        for line in &f.log {
            log(root, id, line);
        }
        for (rid, text) in &f.logged {
            log(root, rid, text);
        }
        for fields in &f.records {
            let Some(rid) = fields.get("id").and_then(Value::as_str) else { continue };
            let path = folder(root).join(rid).join("request.json");
            let had = disk::read_to_string(&path).ok();
            if let Some(mut r) = merged(had.as_deref(), fields) {
                r.id = rid.to_string();
                let _ = save(root, &r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent: its prompt the recipe's, its tools the sandbox's (the web's off, nothing waits
    /// for a yes), its version kept with it; its environment's packages Blender's, its network
    /// the package registries and Blender's downloads only.
    #[test]
    fn the_agent_and_its_sandbox() {
        let a = agent_body("r1");
        assert_eq!(a["name"], AGENT);
        assert_eq!(a["model"]["id"], "claude-opus-5-5");
        assert!(a["system"].as_str().unwrap().contains("cloud_run.py setup ID"));
        let tools = &a["tools"][0];
        assert_eq!(tools["type"], "agent_toolset_20260401");
        assert_eq!(tools["default_config"]["permission_policy"]["type"], "always_allow");
        assert!(tools["configs"].as_array().unwrap().iter().all(|c| c["enabled"] == false), "the web tools off");
        assert_eq!(a["metadata"]["recipe"], "r1");
        let e = environment_body("r1");
        assert_eq!(e["config"]["networking"]["type"], "limited");
        assert_eq!(e["config"]["networking"]["allowed_hosts"], json!(["download.blender.org"]));
        assert!(e["config"]["packages"]["apt"].as_array().unwrap().iter().any(|p| p == "xvfb"));
        assert_eq!(recipe().len(), 8);
        assert_eq!(recipe(), recipe(), "the same recipe, the same version");
    }

    /// A run's first message: the commit checked out, the request's setup, run_claude.py's
    /// --prompt and --settle with the run's flags (the note uploaded), its outputs; the session:
    /// the model asked for, each file mounted under uploads, its cap.
    #[test]
    fn a_run_starts_with_its_commands() {
        let text = first_message("moss_toad", "Moss toad", How::GoOn, true);
        assert!(text.starts_with("Make the model request \"Moss toad\" (moss_toad): on from its lines"));
        for want in [
            "git clone --quiet --filter=blob:none --no-checkout https://github.com/LouisMareau/battle-frontier.git /workspace/bf",
            "python3 tools/blender/cloud_run.py setup moss_toad",
            "python3 tools/blender/run_claude.py moss_toad --prompt --go-on --note-file /mnt/session/uploads/note.txt",
            "python3 tools/blender/run_claude.py moss_toad --settle --go-on --note-file /mnt/session/uploads/note.txt --reply",
            "python3 tools/blender/cloud_run.py outputs moss_toad",
        ] {
            assert!(text.contains(want), "{want}\n{text}");
        }
        assert_eq!(flags(How::Whole, false), "");
        assert_eq!(flags(How::Variants, false), " --variants");
        let ask = Ask { model: "claude-sonnet-5-5".into(), text: text.clone(), request: "moss_toad".into(), title: "Moss toad".into(), cap: 1500 };
        let body = session_body("agent_1", "env_1", &[("file_a".into(), "request/front.png".into())], &ask);
        assert_eq!(body["agent"]["type"], "agent_with_overrides");
        assert_eq!(body["agent"]["model"]["id"], "claude-sonnet-5-5");
        assert_eq!(body["resources"][0]["mount_path"], "/request/front.png");
        assert_eq!(body["budget"]["max_list_cost"]["amount"], "1500");
        assert_eq!(body["initial_events"][0]["content"][0]["text"], text.as_str());
        assert_eq!((model_of(""), model_of("opus"), model_of("haiku")), ("claude-opus-5-5", "claude-opus-5-5", "claude-haiku-5-5"));
    }

    /// The events: what Claude says and does in the log as run_claude.py writes it; what the
    /// scripts print taken out of a tool's answer (the request's fields, their log lines); a
    /// long answer cut; how the session stopped; what it's cost.
    #[test]
    fn the_events_read_as_the_log_and_the_request() {
        let said = follow(&json!({"type": "agent.message", "id": "e1", "content": [{"type": "text", "text": " Reading the request. "}]}));
        assert_eq!(said.log, ["Claude: Reading the request."]);
        let call = follow(&json!({"type": "agent.tool_use", "name": "bash", "input": {"command": "python3 tools/blender/run_request.py moss_toad"}}));
        assert_eq!(call.log, ["→ bash: python3 tools/blender/run_request.py moss_toad"]);
        let answer = "rendered\n[request] {\"id\": \"moss_toad\", \"progress\": {\"step\": \"building\", \"done\": 0.4}}\n[request-log] {\"id\": \"moss_toad\", \"text\": \"a line\"}\n".to_string() + &"more\n".repeat(40);
        let got = follow(&json!({"type": "agent.tool_result", "content": [{"type": "text", "text": answer}], "is_error": false}));
        assert_eq!(got.records, [json!({"id": "moss_toad", "progress": {"step": "building", "done": 0.4}})]);
        assert_eq!(got.logged, [("moss_toad".to_string(), "a line".to_string())]);
        assert!(got.log[0].starts_with("← rendered\nmore") && got.log[0].ends_with("... (11 more lines)"), "{}", got.log[0]);
        let failed = follow(&json!({"type": "agent.tool_result", "content": "oops", "is_error": true}));
        assert_eq!(failed.log, ["× oops"]);
        let only = follow(&json!({"type": "agent.tool_result", "content": "[request] {\"id\": \"moss_toad\", \"status\": \"done\"}\n"}));
        assert!(only.log.is_empty() && only.records.len() == 1, "only the scripts' lines: nothing more said");
        assert_eq!(follow(&json!({"type": "agent.tool_result", "content": ""})).log, ["← (nothing)"]);
        let elsewhere = follow(&json!({"type": "agent.tool_result", "id": "e9", "result": {"stdout": "[request] {\"id\": \"moss_toad\", \"status\": \"done\"}\nok"}}));
        assert_eq!((elsewhere.records.len(), elsewhere.log.as_slice()), (1, ["← ok".to_string()].as_slice()), "the answer wherever its text is");
        for (path, ok) in [
            ("private/requests/moss_toad/preview_front.png", true),
            ("temporary/data/props/moss_toad.json", true),
            ("content/assets/models/moss_toad.glb", true),
            ("content/data/maps/test_meadow.json", false),
            ("content/data/maps/test_meadow/0_0.json", false),
            ("private/requests/../../content/data/props/x.json", false),
            ("private/requests/moss_toad", false),
            ("crates/game/src/main.rs", false),
        ] {
            assert_eq!(may_write(path), ok, "{path}");
        }
        assert_eq!(follow(&json!({"type": "session.status_idle", "stop_reason": {"type": "end_turn"}})).end, Some(End::Idle("end_turn".into())));
        assert_eq!(follow(&json!({"type": "session.status_terminated"})).end, Some(End::Over));
        assert_eq!(follow(&json!({"type": "session.usage", "list_cost": {"amount": "212", "currency": "USD"}})).cost, Some(212.0));
        assert_eq!(follow(&json!({"type": "span.model_request_start"})), Followed::default());
    }

    /// The stream's bytes: each whole event read, a part of one kept for the rest.
    #[test]
    fn the_stream_reads_event_by_event() {
        let mut bytes = b"event: agent.message\ndata: {\"type\":\"agent.message\",\"id\":\"e1\"}\n\nevent: session.status_idle\ndata: {\"type\":\"session.st".to_vec();
        let got = events(&mut bytes);
        assert_eq!(got, [json!({"type": "agent.message", "id": "e1"})]);
        assert_eq!(bytes, b"event: session.status_idle\ndata: {\"type\":\"session.st".to_vec(), "the rest kept");
        bytes.extend(b"atus_idle\",\"id\":\"e2\"}\n\n: a heartbeat\n\n");
        assert_eq!(events(&mut bytes), [json!({"type": "session.status_idle", "id": "e2"})]);
        assert!(bytes.is_empty());
    }

    /// The scripts' fields put on the request as it is (the rest kept), a split's request made
    /// whole; log lines as log.txt has them; a file sent as a form.
    #[test]
    fn the_request_its_log_and_its_files() {
        let r = super::super::ModelRequest { id: "moss_toad".into(), name: "Moss toad".into(), notes: "green".into(), ..Default::default() };
        let text = content::to_text(&r);
        let m = merged(Some(&text), &json!({"id": "moss_toad", "status": "in_progress", "progress": {"step": "building", "done": 0.4, "started": 5.0, "minutes": 8.0}})).unwrap();
        assert_eq!((m.status, m.progress.step.as_str(), m.notes.as_str()), (super::super::Status::InProgress, "building", "green"));
        let child = merged(None, &serde_json::to_value(super::super::ModelRequest { id: "tall_pine".into(), name: "Tall pine".into(), ..Default::default() }).unwrap()).unwrap();
        assert_eq!(child.name, "Tall pine");
        assert_eq!(log_lines("2026-10-21 09:41:07", "one\ntwo"), "[2026-10-21 09:41:07] one\n                      two\n");
        let (body, kind) = form("front.png", b"PNG", "b0");
        assert_eq!(kind, "multipart/form-data; boundary=b0");
        assert_eq!(String::from_utf8(body).unwrap(), "--b0\r\nContent-Disposition: form-data; name=\"file\"; filename=\"front.png\"\r\nContent-Type: image/png\r\n\r\nPNG\r\n--b0--\r\n");
        assert_ne!(key_name("sk-ant-a"), key_name("sk-ant-b"));
        assert!(!key_name("sk-ant-secret").contains("secret"), "a sum, not the key");
    }
}
