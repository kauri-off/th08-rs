//! th08 app: game window, launcher, settings, platform services, data sources, input and pacer; wasm / Android entries.

pub mod app;
pub mod i18n;
pub mod joystick;
pub mod launcher;
pub mod pacer;
pub mod packs;
pub mod platform;
pub mod settings;
pub mod sources;
#[cfg(not(target_arch = "wasm32"))]
pub mod translations;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
#[expect(
    clippy::needless_pass_by_value,
    reason = "wasm-bindgen exports take JS values by value"
)]
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

// Android entry: logcat logger, panics to logcat, config + saves in the app's internal storage. Game files: the
// APK's bundled assets if it has th08.dat, else `<external files dir>/game` (adb push target).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("th08"),
    );
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    let data = app.internal_data_path();
    let dirs = settings::PlatformDirs {
        config: data.clone(),
        save: data,
        soundfonts: Vec::new(),
    };
    let mut s = settings::Settings::game_environment(dirs);
    launch_once(&mut s);
    if let Some(apk) = sources::ApkAssetSource::new(app.asset_manager()) {
        log::info!("game files bundled in the APK: {:?}", apk.names_list());
        app::run_android(app, Some(th08_core::assets::Source::new(apk)), s);
        // Saves are written before the event loop exits; a second android_main in this process would make winit
        // fail (RecreationAttempt), so the :game process ends here and the next Play starts a fresh one.
        std::process::exit(0);
    }
    match app.external_data_path() {
        Some(ext) => {
            let game = ext.join("game");
            let _ = std::fs::create_dir_all(&game);
            log::info!(
                "game folder {} (th08.dat {})",
                game.display(),
                if launcher::is_game_dir(&game) {
                    "found"
                } else {
                    "missing"
                }
            );
            // The folder row is read-only on Android, so the fixed folder wins over a port.cfg value.
            s.set_from(
                settings::Origin::Default,
                "game_dir",
                &game.to_string_lossy(),
            )
            .expect("game_dir accepts any path");
        }
        None => log::error!("no external files dir: game folder unavailable"),
    }
    app::run_android(app, None, s);
    std::process::exit(0);
}

// The Compose launcher's settings for this start only (`launcher = off`, developer rows): `key = value` lines in
// `<internal>/launch.cfg`, applied over port.cfg and env, then deleted.
#[cfg(target_os = "android")]
fn launch_once(s: &mut settings::Settings) {
    let Some(path) = s.dirs.config.as_ref().map(|d| d.join("launch.cfg")) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let _ = std::fs::remove_file(&path);
    for l in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if let Some((k, v)) = l.split_once('=')
            && let Err(e) = s.set_from(settings::Origin::Cli, k.trim(), v.trim())
        {
            log::error!("launch.cfg: {l:?}: {e}");
        }
    }
}
