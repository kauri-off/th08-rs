//! th08 app: the game window and launcher (app, launcher), settings and platform services, data sources, input
//! devices and the frame pacer; entries `start` (wasm) and `android_main` (STEP10_CRATES.md §1). Logic lives in
//! th08-core, drawing in th08-render, sound in th08-audio.

pub mod app;
pub mod joystick;
pub mod launcher;
pub mod pacer;
pub mod platform;
pub mod settings;
pub mod sources;

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
    let source = sources::web(files);
    // `?launcher=0` (or a saved launcher = off) starts the game directly; `?shot=` runs never show it.
    // Web dirs: none; port.cfg and saves are localStorage items.
    let s = settings::Settings::game_environment(settings::PlatformDirs::default());
    if s.launcher && !s.is_tool_run() {
        app::run_launcher(Some(source), s);
    } else {
        app::run(source, s);
    }
}

// Android entry (ARCH_D_MOBILE §6.1 item 6). Stub: the event loop gets `app` in Android run 1 (ARCH_D §6.3).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    let data = app.internal_data_path();
    let dirs = settings::PlatformDirs {
        config: data.clone(),
        save: data,
        soundfonts: Vec::new(),
    };
    app::run_launcher(None, settings::Settings::game_environment(dirs));
}
