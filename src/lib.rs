//! th08 port library: one module per ported subsystem plus the debug tools (session, demosync, drawlog,
//! drawreport, anmdebug).

pub mod anm;
pub mod anmdebug;
pub mod anmmgr;
pub mod anmvm;
pub mod app;
pub mod archive;
pub mod ascii;
pub mod assets;
pub mod audio;
pub mod bomb;
pub mod bullet;
pub mod config;
pub mod demosync;
pub mod drawlog;
#[cfg(not(target_arch = "wasm32"))]
pub mod drawreport;
pub mod ecl;
pub mod eclvm;
pub mod effect;
pub mod ending;
pub mod enemy;
pub mod game;
pub mod gamemgr;
pub mod gfx;
pub mod gui;
pub mod hud;
pub mod input;
pub mod item;
pub mod launcher;
pub mod menu;
pub mod menulist;
pub mod msg;
pub mod player;
pub mod pshot;
pub mod replay;
pub mod result;
pub mod rng;
pub mod scoredat;
pub mod session;
pub mod settings;
pub mod sht;
pub mod sound;
pub mod stage;
pub mod stdfile;
pub mod supervisor;
pub mod task;
pub mod text;
#[cfg(not(target_arch = "wasm32"))]
pub mod xz;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start(files: js_sys::Array) {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    let files = files
        .iter()
        .filter_map(|f| f.dyn_into::<web_sys::File>().ok())
        .collect();
    let source = assets::Source::web(files);
    // `?launcher=0` (or a saved launcher = off) starts the game directly; `?shot=` runs never show it.
    let s = settings::Settings::game_environment();
    if s.launcher && !s.is_tool_run() {
        app::run_launcher(Some(source), s);
    } else {
        settings::init(s);
        app::run(source);
    }
}
