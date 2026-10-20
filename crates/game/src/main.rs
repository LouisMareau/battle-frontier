//! The game: one program for the browser (WebAssembly, WebGPU or WebGL 2) and the desktop.
//!
//! Desktop options: game [--preset ID] [--auto] [--time morning|day|night] [--levels 1-5]
//!                       [--maps [ID]] [--requests] [--explore [ID]] [--exit-after SECONDS] [--stats]
//!                       [--no-vsync] [--mute]
//! (--requests opens the map editor with its Requests window; --mute plays no sound)
//!        game --link battlefrontier://edit/PROP [--root PROJECT]   (opens Blender on the prop)
//!        game --register-links                                    (sets those links up; a release build does it at every start too)
//! (--stats prints frames per second and processor time each second; with --no-vsync the
//! frame rate is uncapped: what the machine can do, not the screen's rate)
//! (content from ./content and ./temporary, or under $GAME_ROOT)

#![cfg_attr(all(not(debug_assertions), not(target_arch = "wasm32")), windows_subsystem = "windows")]

mod app;
mod audio;
mod director;
mod editor;
mod hud;
mod input;
mod keep;
mod layout;
mod mapedit;
mod menu;
mod pads;
mod platform;
mod play_hud;
mod presets;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod shot;
mod stage;
mod team;
mod title;

/// What ends the program (a panic: a GPU error, a bug) also goes, timed, to build/crash.log
/// in the project: the release build has no console to say it in.
#[cfg(not(target_arch = "wasm32"))]
fn keep_crashes(root: &std::path::Path) {
    let log = root.join("build").join("crash.log");
    let said = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        said(info);
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("[{} UTC] the game stopped: {info}\n{}\n\n", utc(secs), std::backtrace::Backtrace::force_capture());
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }));
}

/// Seconds since 1970 as a UTC date and time: "2026-10-17 19:40:12".
#[cfg(not(target_arch = "wasm32"))]
fn utc(secs: u64) -> String {
    // days to a civil date (Howard Hinnant's days_from_civil, backwards)
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let mut cli = app::Cli::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let root = std::path::PathBuf::from(arg_after("--root").or_else(|| std::env::var("GAME_ROOT").ok()).unwrap_or_else(|| ".".into()));
    keep_crashes(&root);
    // a battlefrontier: link (the browser's "Edit in Blender"): Blender opens on the prop,
    // and that's all
    if let Some(url) = arg_after("--link") {
        match mapedit::link_target(&url) {
            Some(id) => {
                if let Err(e) = mapedit::open_in_blender(&root, &id) {
                    eprintln!("{e}");
                }
            }
            None => eprintln!("not a Battle Frontier link: {url}"),
        }
        return Ok(());
    }
    // the links go to this program, with this project (cheap, the user's own registry)
    #[cfg(windows)]
    {
        if root.join("content").is_dir() {
            let at = root.clone();
            if args.iter().any(|a| a == "--register-links") {
                mapedit::register_links(&at);
                return Ok(());
            }
            // a debug build (a console program, slow to start) leaves the links to the
            // release build, unless asked with --register-links
            if !cfg!(debug_assertions) {
                std::thread::spawn(move || mapedit::register_links(&at));
            }
        }
    }
    let mut i = 0;
    while i < args.len() {
        let next = args.get(i + 1).cloned();
        match args[i].as_str() {
            "--preset" => {
                cli.preset = next;
                i += 1;
            }
            "--maps" => {
                match next.filter(|n| !n.starts_with("--")) {
                    Some(id) => {
                        cli.maps = Some(id);
                        i += 1;
                    }
                    None => cli.maps = Some(String::new()),
                }
            }
            "--explore" => {
                match next.filter(|n| !n.starts_with("--")) {
                    Some(id) => {
                        cli.explore = Some(id);
                        i += 1;
                    }
                    None => cli.explore = Some(String::new()),
                }
            }
            "--time" => {
                cli.time = next;
                i += 1;
            }
            "--levels" => {
                cli.levels = next.and_then(|v| v.parse().ok());
                i += 1;
            }
            "--exit-after" => {
                cli.exit_after = next.and_then(|v| v.parse().ok());
                i += 1;
            }
            "--auto" => cli.auto = true,
            "--stats" => cli.stats = true,
            "--no-vsync" => cli.no_vsync = true,
            "--mute" => cli.mute = true,
            "--requests" => {
                cli.requests = true;
                cli.maps.get_or_insert_with(String::new);
            }
            "--root" | "--register-links" => {
                if args[i] == "--root" {
                    i += 1;
                }
            }
            other => eprintln!("unknown option {other}"),
        }
        i += 1;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Battle Frontier").with_inner_size([1400.0, 900.0]).with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let mut options = options;
    if cli.no_vsync {
        options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    }
    eframe::run_native("Battle engine", options, Box::new(move |cc| Ok(Box::new(app::GameApp::new(cc, cli)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("no document");
        let canvas = document
            .get_element_by_id("game")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("no canvas with the id game");
        let started = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(app::GameApp::new(cc, app::Cli::default())))))
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            match &started {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_inner_html(&format!("<p>The game couldn't start (no WebGPU or WebGL 2?): {e:?}</p>")),
            }
        }
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn a_crash_is_kept_in_build_crash_log() {
        assert_eq!(super::utc(0), "1970-01-01 00:00:00");
        assert_eq!(super::utc(1_792_266_012), "2026-10-17 19:40:12");
        assert_eq!(super::utc(951_782_400), "2000-02-29 00:00:00", "a leap day");
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/tests").join(format!("crash-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        super::keep_crashes(&root);
        let caught = std::panic::catch_unwind(|| panic!("the bones overran"));
        // the default hook again (the other tests' panics are theirs)
        let _ = std::panic::take_hook();
        assert!(caught.is_err());
        let log = std::fs::read_to_string(root.join("build/crash.log")).unwrap();
        assert!(log.contains(" UTC] the game stopped: panicked at ") && log.contains("the bones overran"), "{log}");
        std::fs::remove_dir_all(&root).ok();
    }
}
