//! The game: one program for the browser (WebAssembly, WebGPU or WebGL 2) and the desktop.
//!
//! Desktop options: game [--preset ID] [--auto] [--time morning|day|night] [--levels 1-5]
//!                       [--maps [ID]] [--explore [ID]] [--exit-after SECONDS] [--stats] [--no-vsync]
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
mod mapedit;
mod menu;
mod platform;
mod presets;
mod stage;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let mut cli = app::Cli::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
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
            other => eprintln!("unknown option {other}"),
        }
        i += 1;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Battle engine").with_inner_size([1400.0, 900.0]).with_min_inner_size([800.0, 500.0]),
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
