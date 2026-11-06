//! th08 app: game window, launcher, settings, platform services, data sources, input and pacer; wasm / Android entries.

pub mod app;
pub mod crashlog;
#[cfg(target_os = "android")]
pub mod gameimport;
pub mod i18n;
pub mod joystick;
pub mod launcher;
pub mod pacer;
pub mod packs;
pub mod platform;
pub mod saves;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub mod selfupdate;
pub mod settings;
pub mod sources;
pub mod translations;
pub mod transport;
#[cfg(not(target_arch = "wasm32"))]
pub mod update;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
#[expect(
    clippy::needless_pass_by_value,
    reason = "wasm-bindgen exports take JS values by value"
)]
pub fn start(files: js_sys::Array) {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    std::panic::set_hook({
        let prev = std::panic::take_hook();
        Box::new(move |info| {
            crashlog::report("panic", &info.to_string(), None);
            prev(info);
        })
    });
    let _ = console_log::init_with_level(log::Level::Info);
    let files = files
        .iter()
        .filter_map(|f| f.dyn_into::<web_sys::File>().ok())
        .collect();
    let source = sources::web(files);
    // `?launcher=0` (or a saved launcher = off) starts the game directly; `?shot=` runs never show it.
    // Web dirs: none; port.cfg and saves are localStorage items, translation packs are in the site storage.
    let s = settings::Settings::game_environment(settings::PlatformDirs::default());
    wasm_bindgen_futures::spawn_local(async move {
        packs::web::load().await;
        if s.launcher && !s.is_tool_run() {
            app::run_launcher(Some(source), s);
        } else {
            app::run(source, s);
        }
    });
}

// Android entry: logcat logger, panics to logcat, config + saves in the app's internal storage. Game files, first
// found: `<internal>/game` (the launcher's import), the APK's bundled assets, `<external files dir>/game` (old installs).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    crashlog::Tee::install(
        Box::new(android_logger::AndroidLogger::new(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("th08"),
        )),
        log::LevelFilter::Info,
    );
    let data = app.internal_data_path();
    crashlog::set_dir(data.as_deref());
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    crashlog::install_panic_hook();
    // A second GameActivity created before the first was destroyed (same process): winit allows one event loop per
    // process, so this one finishes and the first exits the process on its Destroy.
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        log::warn!("lifecycle: second GameActivity in this process; it closes");
        return;
    }
    let imported = data.as_ref().map(|d| d.join("game"));
    let dirs = settings::PlatformDirs {
        config: data.clone(),
        save: data,
        soundfonts: Vec::new(),
    };
    let mut s = settings::Settings::game_environment(dirs);
    launch_once(&mut s);
    let legacy = app.external_data_path().map(|e| e.join("game"));
    let game = match imported.iter().find(|d| launcher::is_game_dir(d)) {
        Some(d) => Some(d.clone()),
        None => {
            if let Some(apk) = sources::ApkAssetSource::new(app.asset_manager()) {
                log::info!("game files bundled in the APK: {:?}", apk.names_list());
                app::run_android(app, Some(th08_core::assets::Source::new(apk)), s);
                // Saves are written before the event loop exits; a second android_main in this process would make
                // winit fail (RecreationAttempt), so the :game process ends here and the next Play starts a fresh one.
                std::process::exit(0);
            }
            legacy.filter(|d| launcher::is_game_dir(d)).or(imported)
        }
    };
    match game {
        Some(game) => {
            log::info!(
                "game folder {} (th08 content {})",
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
        None => log::error!("no internal files dir: game folder unavailable"),
    }
    app::run_android(app, None, s);
    std::process::exit(0);
}

// The Compose launcher's settings for this start only (developer rows): `key = value` lines in
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
