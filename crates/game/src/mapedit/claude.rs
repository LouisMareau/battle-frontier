//! Claude, for the model requests: whether the user is signed in to it, and the top bar's
//! "Claude sign-in" (beside GitHub's) with its window. The Requests button waits for it.
//!
//! On the desktop it's Claude Code's own sign-in, as `claude auth status` tells (asked on a
//! thread of its own, now and then): a Claude subscription, which the runs go on
//! (run_claude.py routes them to it: an API key in the environment, which would bill the key
//! instead, is left out then), or an Anthropic Console account, billed by the API. Signing in
//! opens Claude Code's own login in a terminal of its own, which goes on in the browser
//! (Anthropic's sign-in: the editor sees no password and no token).
//!
//! In the browser it's the user's own Claude API key, typed in the window and kept in this
//! browser only: the requests made there run on it, billed to them, nobody else's. The site
//! has no key of its own, and passes the user's on to Anthropic only (functions/claude/: the
//! calls Anthropic's API won't take from a page go through the site's own address).

use egui::{Color32, RichText};

use super::{hint, mark, warn};
use crate::hud::TEXT;

/// What the Requests button says while there's no sign-in.
pub(super) const NEEDS: &str = "Only available with Claude subscription or API keys";

/// Where the browser's calls to Anthropic go: the site's own address (functions/claude/),
/// which passes them on.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) const RELAY: &str = "claude";

/// The version of Anthropic's API the calls ask for.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) const VERSION: &str = "2023-06-01";

/// What's known of the sign-in.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Signed {
    /// not known yet: being asked (Claude Code, on the desktop)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Asking,
    /// not signed in, and why
    Out(String),
    /// a Claude subscription, its plan ("Max", "Pro"; "" when it doesn't say)
    Plan(String),
    /// billed by the API: what it is (an Anthropic Console account, an API key)
    Api(String),
}

impl Signed {
    pub(super) fn is_in(&self) -> bool {
        matches!(self, Signed::Plan(_) | Signed::Api(_))
    }

    /// The top bar button's word for it.
    fn label(&self) -> String {
        match self {
            Signed::Plan(plan) if !plan.is_empty() => format!("Claude: {plan}"),
            Signed::Plan(_) => "Claude: subscription".into(),
            Signed::Api(_) => "Claude: API key".into(),
            Signed::Asking | Signed::Out(_) => "Claude sign-in".into(),
        }
    }

    /// What it means for the requests, in a line.
    pub(super) fn about(&self) -> String {
        match self {
            Signed::Asking => "Asking Claude Code whether it's signed in…".into(),
            Signed::Out(why) => format!("{why}. {NEEDS}."),
            Signed::Plan(plan) => {
                format!("Signed in with your Claude subscription{}: model requests run on it", if plan.is_empty() { String::new() } else { format!(" ({plan})") })
            }
            Signed::Api(what) => format!("Signed in with {what}: model requests run on it, billed by the API"),
        }
    }
}

/// The API keys an environment can hold, which Claude Code would go on before its own sign-in.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) const KEYS: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// What `claude auth status` says (its JSON), the environment's API key aside (`env_key`: the
/// name of one that's set, which a run goes on when Claude Code itself isn't signed in).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) fn read_status(json: &str, env_key: Option<&str>) -> Signed {
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap_or_default();
    let signed = v.get("loggedIn").and_then(serde_json::Value::as_bool) == Some(true);
    match v.get("authMethod").and_then(serde_json::Value::as_str).unwrap_or("") {
        "claude.ai" if signed => {
            let plan = v.get("subscriptionType").and_then(serde_json::Value::as_str).unwrap_or("");
            let mut letters = plan.chars();
            Signed::Plan(letters.next().map(|c| c.to_uppercase().collect::<String>() + letters.as_str()).unwrap_or_default())
        }
        "oauth_token" if signed => Signed::Plan(String::new()),
        "api_key" | "api_key_helper" if signed => Signed::Api("an Anthropic Console account (Claude Code's sign-in)".into()),
        _ if signed => Signed::Api("Claude Code's own sign-in".into()),
        _ => match env_key {
            Some(k) => Signed::Api(format!("the API key in {k}")),
            None if v.is_null() => Signed::Out("Claude Code didn't say whether it's signed in (an older one? `claude update` brings one that does)".into()),
            None => Signed::Out("Claude Code isn't signed in".into()),
        },
    }
}

/// Claude Code, where run_claude.py looks for it: CLAUDE, the PATH, ~/.local/bin.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn find_claude() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
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

/// Claude Code asked whether it's signed in (the environment's keys left out, so it tells its
/// own sign-in), without a console window.
#[cfg(not(target_arch = "wasm32"))]
fn ask() -> Signed {
    let Some(claude) = find_claude() else {
        return Signed::Out("Claude Code isn't installed (or not where the editor looks: CLAUDE names the program)".into());
    };
    let mut c = if claude.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/c").arg(&claude);
        c
    } else {
        std::process::Command::new(&claude)
    };
    c.args(["auth", "status"]).stdin(std::process::Stdio::null());
    for k in KEYS {
        c.env_remove(k);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    match c.output() {
        Ok(out) => read_status(&String::from_utf8_lossy(&out.stdout), KEYS.into_iter().find(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))),
        Err(e) => Signed::Out(format!("Claude Code didn't answer ({}: {e})", claude.display())),
    }
}

/// Claude Code's own login in a terminal of its own (it goes on in the browser): with a Claude
/// subscription, or with an Anthropic Console account (`console`: billed by the API). What to
/// say.
#[cfg(not(target_arch = "wasm32"))]
fn sign_in(console: bool) -> String {
    let Some(claude) = find_claude() else {
        return "Claude Code isn't installed: install it (code.claude.com), then sign in".into();
    };
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "start", "Claude sign-in"]).arg(&claude).args(["auth", "login"]);
        if console {
            c.arg("--console");
        }
        match c.spawn() {
            Ok(_) => "Claude Code's sign-in opened in a terminal: it goes on in your browser".into(),
            Err(e) => format!("{}: {e}", claude.display()),
        }
    }
    #[cfg(not(windows))]
    {
        format!("Run {} auth login{} in a terminal: it goes on in your browser", claude.display(), if console { " --console" } else { "" })
    }
}

/// How long the window keeps asking whether the sign-in's done after opening Claude Code's
/// login (ms), and how often (ms); how often otherwise, the window open or not.
#[cfg(not(target_arch = "wasm32"))]
const SIGNING_MS: f64 = 5.0 * 60_000.0;
#[cfg(not(target_arch = "wasm32"))]
const SIGNING_EVERY_MS: f64 = 3_000.0;
#[cfg(not(target_arch = "wasm32"))]
const EVERY_MS: f64 = 120_000.0;

/// The name the browser keeps the user's API key under (`platform::store_text`).
#[cfg(target_arch = "wasm32")]
const KEY_NAME: &str = "claude_api_key";

/// The user's own Claude API key, as this browser keeps it.
#[cfg(target_arch = "wasm32")]
pub(super) fn key() -> Option<String> {
    crate::platform::stored_text(KEY_NAME).map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// A key shown without itself: its start and its last four.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) fn masked(key: &str) -> String {
    let key = key.trim();
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    if key.starts_with("sk-ant-") && key.chars().count() > 12 {
        format!("sk-ant-…{tail}")
    } else {
        format!("…{tail}")
    }
}

/// The sign-in: what's known of it, its window, the asking.
pub(super) struct Claude {
    pub signed: Signed,
    pub open: bool,
    /// what to say, once
    pub said: Option<String>,
    /// an ask on its way: its answer, when it's come
    #[cfg(not(target_arch = "wasm32"))]
    asked: Option<std::sync::Arc<std::sync::Mutex<Option<Signed>>>>,
    /// when to ask again (ms)
    #[cfg(not(target_arch = "wasm32"))]
    next_ask: f64,
    /// asked every few seconds until then, after opening Claude Code's login (ms)
    #[cfg(not(target_arch = "wasm32"))]
    signing_until: f64,
    /// the key being typed
    #[cfg(target_arch = "wasm32")]
    typed: String,
    /// a key being checked with Anthropic: it, and the answer when it's come
    #[cfg(target_arch = "wasm32")]
    checking: Option<(String, Checked)>,
}

/// A key's check: Anthropic's answer, when it's come.
#[cfg(target_arch = "wasm32")]
type Checked = std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>;

impl Default for Claude {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let signed = Signed::Asking;
        #[cfg(target_arch = "wasm32")]
        let signed = match key() {
            Some(k) => Signed::Api(format!("your API key ({}, kept in this browser)", masked(&k))),
            None => Signed::Out("No Claude API key in this browser yet".into()),
        };
        Claude {
            signed,
            open: false,
            said: None,
            #[cfg(not(target_arch = "wasm32"))]
            asked: None,
            #[cfg(not(target_arch = "wasm32"))]
            next_ask: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            signing_until: 0.0,
            #[cfg(target_arch = "wasm32")]
            typed: String::new(),
            #[cfg(target_arch = "wasm32")]
            checking: None,
        }
    }
}

impl Claude {
    /// Each frame: an answer taken in; on the desktop, Claude Code asked again when it's time
    /// (every few seconds while a sign-in is underway).
    pub fn poll(&mut self, ctx: &egui::Context, now: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let answer = self.asked.as_ref().and_then(|a| a.lock().unwrap().take());
            if let Some(signed) = answer {
                self.asked = None;
                if signed.is_in() && self.signing_until > 0.0 {
                    self.said = Some(signed.about());
                    self.signing_until = 0.0;
                }
                self.signed = signed;
                crate::platform::set_hook("claude", if self.signed.is_in() { "in" } else { "out" });
            }
            if self.asked.is_none() && now >= self.next_ask {
                let signing = now < self.signing_until;
                self.next_ask = now + if signing { SIGNING_EVERY_MS } else { EVERY_MS };
                let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                self.asked = Some(slot.clone());
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let signed = ask();
                    *slot.lock().unwrap() = Some(signed);
                    ctx.request_repaint();
                });
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (ctx, now);
            let answer = self.checking.as_ref().and_then(|(_, a)| a.lock().unwrap().take());
            if let Some(answer) = answer {
                let (typed, _) = self.checking.take().unwrap();
                match answer {
                    Ok(()) => match crate::platform::store_text(KEY_NAME, &typed) {
                        Ok(()) => {
                            self.signed = Signed::Api(format!("your API key ({}, kept in this browser)", masked(&typed)));
                            self.said = Some(self.signed.about());
                            self.typed.clear();
                        }
                        Err(e) => self.said = Some(format!("The key isn't kept: this browser keeps nothing ({e})")),
                    },
                    Err(e) => self.said = Some(e),
                }
            }
            crate::platform::set_hook("claude", if self.signed.is_in() { "in" } else { "out" });
        }
    }

    /// The top bar's button: "Claude sign-in", or what it's signed in with; it opens the window.
    pub fn button(&mut self, ui: &mut egui::Ui) {
        let b = ui.button(self.signed.label()).on_hover_text(self.signed.about());
        mark("uiClaudeSignIn", &b);
        if b.clicked() {
            self.open = !self.open;
            // asked afresh, whatever it said a while ago
            #[cfg(not(target_arch = "wasm32"))]
            if self.open {
                self.next_ask = 0.0;
            }
        }
    }

    /// The window, when it's open: a dialog, as GitHub's sign-in is.
    pub fn window(&mut self, ctx: &egui::Context, now: f64) {
        crate::platform::set_hook("claudewindow", if self.open { "open" } else { "" });
        if !self.open {
            return;
        }
        let modal = egui::Modal::new(egui::Id::new("editor-claude")).frame(crate::title::dialog_frame()).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Claude, for the model requests").size(20.0).color(TEXT));
            ui.add_space(6.0);
            match &self.signed {
                Signed::Asking => hint(ui, &self.signed.about()),
                Signed::Out(_) => warn(ui, &self.signed.about()),
                _ => {
                    ui.label(RichText::new(self.signed.about()).color(Color32::from_rgb(140, 220, 150)));
                }
            }
            ui.add_space(6.0);
            self.ways(ui, now);
            if let Some(said) = &self.said {
                ui.add_space(4.0);
                hint(ui, said);
            }
            ui.add_space(6.0);
            let c = ui.button("Close");
            mark("uiClaudeClose", &c);
            if c.clicked() {
                self.open = false;
            }
        });
        if modal.should_close() {
            self.open = false;
        }
    }

    /// The ways to sign in: Claude Code's own login (a subscription, or a Console account).
    #[cfg(not(target_arch = "wasm32"))]
    fn ways(&mut self, ui: &mut egui::Ui, now: f64) {
        let mut go = None;
        if !self.signed.is_in() {
            let plan = ui.button("Sign in with a Claude subscription").on_hover_text("claude auth login: Claude Pro, Max, Team or Enterprise; the requests go on its usage");
            mark("uiClaudePlan", &plan);
            if plan.clicked() {
                go = Some(false);
            }
            let console = ui.button("Sign in with a Console account (API billing)").on_hover_text("claude auth login --console: an Anthropic Console account, billed by the API");
            mark("uiClaudeConsole", &console);
            if console.clicked() {
                go = Some(true);
            }
        }
        if let Some(console) = go {
            self.said = Some(sign_in(console));
            (self.signing_until, self.next_ask) = (now + SIGNING_MS, now + SIGNING_EVERY_MS);
        }
        let again = ui.button("Ask again").on_hover_text("Asks Claude Code whether it's signed in now");
        mark("uiClaudeAsk", &again);
        if again.clicked() {
            self.next_ask = 0.0;
        }
        hint(
            ui,
            if self.signed.is_in() {
                "It's Claude Code's own sign-in (claude auth login in a terminal switches the account). The runs go on it: a Claude subscription before any API key in your environment."
            } else {
                "Claude Code's own sign-in opens in a terminal, then your browser: the editor never sees your password or a token. The runs go on it: a Claude subscription before any API key in your environment."
            },
        );
        if matches!(&self.signed, Signed::Out(why) if why.contains("isn't installed")) {
            ui.hyperlink_to("Install Claude Code", "https://code.claude.com/docs/en/setup");
        }
    }

    /// The way to sign in: the user's own API key, kept in this browser.
    #[cfg(target_arch = "wasm32")]
    fn ways(&mut self, ui: &mut egui::Ui, _now: f64) {
        hint(
            ui,
            "Model requests run in a sandbox of Anthropic's on your own Claude API key, billed to you at the API's prices (a request has cost $0.20 to $7.50 so far, about $2 typically). The key's kept in this browser only and goes to Anthropic only: nobody else's key is used, and nobody else can use yours.",
        );
        ui.add_space(4.0);
        if let Some(k) = key() {
            ui.label(format!("Your key: {}", masked(&k)));
            let forget = ui.button("Forget the key").on_hover_text("Takes it out of this browser (it stays valid at Anthropic until you take it back in the Console)");
            mark("uiClaudeForget", &forget);
            if forget.clicked() {
                crate::platform::forget_text(KEY_NAME);
                self.signed = Signed::Out("No Claude API key in this browser".into());
                self.said = Some("The key's forgotten here".into());
            }
        } else {
            let checking = self.checking.is_some();
            ui.horizontal(|ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut self.typed).password(true).hint_text("sk-ant-…").desired_width(230.0));
                mark("uiClaudeKey", &field);
                let use_it = ui.add_enabled(!checking && !self.typed.trim().is_empty(), egui::Button::new(if checking { "Checking…" } else { "Use this key" }));
                mark("uiClaudeUse", &use_it);
                if use_it.clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !checking && !self.typed.trim().is_empty()) {
                    self.check(ui.ctx());
                }
            });
            ui.hyperlink_to("Make a key in the Claude Console", "https://platform.claude.com/settings/keys");
        }
        hint(ui, "A Claude subscription signs in in the desktop app only: Anthropic lets no other site use it.");
    }

    /// The typed key checked with Anthropic (the models it may use: a quick call, free), kept
    /// once it's taken.
    #[cfg(target_arch = "wasm32")]
    fn check(&mut self, ctx: &egui::Context) {
        let typed = self.typed.trim().to_string();
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.checking = Some((typed.clone(), slot.clone()));
        self.said = None;
        let ctx = ctx.clone();
        let mut req = ehttp::Request::get(format!("{RELAY}/v1/models?limit=1"));
        req.headers.insert("x-api-key", &typed);
        req.headers.insert("anthropic-version", VERSION);
        ehttp::fetch(req, move |r| {
            let answer = match r {
                Ok(resp) if resp.ok => Ok(()),
                Ok(resp) if resp.status == 401 || resp.status == 403 => Err("Anthropic turned this key away: is it whole, and still valid?".to_string()),
                Ok(resp) => Err(format!("The key couldn't be checked (HTTP {}): try again in a moment", resp.status)),
                Err(e) => Err(format!("The key couldn't be checked: {e}")),
            };
            *slot.lock().unwrap() = Some(answer);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code's answers: a subscription (its plan said as a name), a Console account, a
    /// long-lived subscription token, not signed in (with an API key in the environment, which
    /// the runs go on then, or without), nothing readable.
    #[test]
    fn claude_codes_status_reads() {
        let plan = r#"{"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "subscriptionType": "max"}"#;
        assert_eq!(read_status(plan, None), Signed::Plan("Max".into()));
        assert_eq!(read_status(plan, Some("ANTHROPIC_API_KEY")), Signed::Plan("Max".into()), "the subscription first");
        assert_eq!(read_status(r#"{"loggedIn": true, "authMethod": "api_key"}"#, None), Signed::Api("an Anthropic Console account (Claude Code's sign-in)".into()));
        assert_eq!(read_status(r#"{"loggedIn": true, "authMethod": "oauth_token"}"#, None), Signed::Plan(String::new()));
        let out = r#"{"loggedIn": false, "authMethod": "none"}"#;
        assert_eq!(read_status(out, None), Signed::Out("Claude Code isn't signed in".into()));
        assert_eq!(read_status(out, Some("ANTHROPIC_API_KEY")), Signed::Api("the API key in ANTHROPIC_API_KEY".into()));
        assert!(matches!(read_status("error: unknown command 'auth'", None), Signed::Out(why) if why.contains("claude update")));
        assert!(Signed::Plan("Max".into()).is_in() && Signed::Api("x".into()).is_in() && !Signed::Asking.is_in() && !Signed::Out("x".into()).is_in());
        assert_eq!(Signed::Plan("Max".into()).label(), "Claude: Max");
        assert_eq!(Signed::Out("Claude Code isn't signed in".into()).label(), "Claude sign-in");
        assert!(Signed::Out("Claude Code isn't signed in".into()).about().ends_with(&format!("{NEEDS}.")));
    }

    #[test]
    fn a_key_shows_without_itself() {
        assert_eq!(masked("sk-ant-api03-abcdefghijklmnopWXYZ"), "sk-ant-…WXYZ");
        assert_eq!(masked("  short1234 "), "…1234");
    }

    /// The Requests button greyed out while there's no sign-in, not once there's one.
    #[test]
    fn the_requests_button_waits_for_a_sign_in() {
        use super::super::dock::{toggles, Dock, Pane};
        let ctx = egui::Context::default();
        let mut dock = Dock::default();
        let mut enabled = |locked: &[(Pane, &str)]| {
            let mut out = None;
            let mut o = ctx.run_ui(egui::RawInput::default(), |ui| {
                let row = ui.max_rect();
                out = toggles(ui, row, &mut dock, locked).get(&Pane::Requests).map(|r| r.enabled());
            });
            o.textures_delta.clear();
            out
        };
        assert_eq!(enabled(&[(Pane::Requests, NEEDS)]), Some(false));
        assert_eq!(enabled(&[]), Some(true));
    }

    /// The top bar's Claude sign-in beside the panels' toggles (Requests greyed out), and its
    /// window, drawn for a look by eye: build/shots/claude-out.png (not signed in) and
    /// build/shots/claude-in.png (a subscription). It needs a GPU, so it's run by hand:
    ///     cargo test -p engine-game -- --ignored shot
    #[test]
    #[ignore]
    fn shot_of_the_claude_sign_in() {
        use super::super::dock::{toggles, Dock, Pane};
        let ctx = egui::Context::default();
        crate::title::install_fonts(&ctx);
        let mut shooter = crate::shot::Shooter::new().unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 460.0));
        // the pointer on the Requests button, its tip at once
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        // (the frames' clock goes on from one shot to the next: egui's never goes back)
        let mut clock = 0.0;
        let mut shoot = |c: &mut Claude, name: &str| {
            let mut dock = Dock::default();
            let mut out = None;
            for k in 0..6 {
                let at = crate::platform::hook("uiRequests").and_then(|h| h.split_once(',').and_then(|(x, y)| Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))));
                let events = at.filter(|_| k >= 2).map(|p| vec![egui::Event::PointerMoved(p)]).unwrap_or_default();
                clock += 0.1;
                let mut o = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, time: Some(clock), ..Default::default() }, |ui| {
                    egui::Panel::top("bar").show(ui, |ui| {
                        ui.add_space(4.0);
                        let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 30.0));
                        ui.horizontal(|ui| super::super::group(ui, "uiGroupClaude", |ui| c.button(ui)));
                        let locked = if c.signed.is_in() { vec![] } else { vec![(Pane::Requests, NEEDS)] };
                        toggles(ui, row, &mut dock, &locked);
                        ui.add_space(4.0);
                    });
                    c.window(ui.ctx(), 0.0);
                });
                shooter.textures(&mut o.textures_delta);
                out = Some(o);
            }
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../build/shots/{name}.png"));
            shooter.shoot(&ctx, out.unwrap().shapes, [1000, 460], &path).unwrap();
        };
        let mut c = Claude { signed: Signed::Out("Claude Code isn't signed in".into()), open: true, ..Default::default() };
        shoot(&mut c, "claude-out");
        c.signed = Signed::Plan("Max".into());
        shoot(&mut c, "claude-in");
    }
}
