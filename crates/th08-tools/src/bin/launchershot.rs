//! Renders the launcher screen offscreen to a PNG (`--safe`: safe-area insets in px, as an Android cutout gives).

fn main() {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: launchershot OUT.png [WxH] [--developer] [--error=TEXT] [--view=waiting|loading] [--safe=L,T,R,B] [--config=DIR] [--bundled=DIR] [--tr-install=CODE] [--tr-list] [--capture=KEY_ROW] [--pad-capture=PAD_ROW] [--pad-press=PAD_ROW:BUTTON]... [--tab=replays|stats] [--video=ask:RPY|progress:FILE:DONE:TOTAL] [th08 switches]..."
        );
        std::process::exit(2);
    }
    let out = args.remove(0);
    let size = args
        .first()
        .and_then(|a| a.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
    if size.is_some() {
        args.remove(0);
    }
    // Launcher views: the developer view and the screens of the app's Shell states.
    let dev = args.iter().any(|a| a == "--developer");
    let error = args
        .iter()
        .find_map(|a| a.strip_prefix("--error="))
        .map(String::from);
    let view = match args.iter().find_map(|a| a.strip_prefix("--view=")) {
        None => th08::launcher::View::Settings,
        Some("waiting") => th08::launcher::View::Waiting,
        Some("loading") => th08::launcher::View::Loading,
        Some(v) => {
            eprintln!("--view={v}: expected waiting or loading");
            std::process::exit(2);
        }
    };
    let safe = args
        .iter()
        .find_map(|a| a.strip_prefix("--safe="))
        .map(|v| {
            let n: Vec<u32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if n.len() != 4 {
                eprintln!("--safe={v}: expected L,T,R,B");
                std::process::exit(2);
            }
            th08_render::gfx::SafeArea {
                left: n[0],
                top: n[1],
                right: n[2],
                bottom: n[3],
            }
        })
        .unwrap_or_default();
    // Translations: a config folder of its own, a pack installed and the server list loaded before the shot.
    let config = args
        .iter()
        .find_map(|a| a.strip_prefix("--config="))
        .map(std::path::PathBuf::from);
    let install = args
        .iter()
        .find_map(|a| a.strip_prefix("--tr-install="))
        .map(String::from);
    let list = args.iter().any(|a| a == "--tr-list");
    // A Keyboard row (`key_bomb`) shown waiting for a key press.
    let capture = args
        .iter()
        .find_map(|a| a.strip_prefix("--capture="))
        .map(String::from);
    // A Gamepad row waiting for a button, and captures fed a button as if pressed on a pad.
    let pad_capture = args
        .iter()
        .find_map(|a| a.strip_prefix("--pad-capture="))
        .map(String::from);
    let pad_press: Vec<String> = args
        .iter()
        .filter_map(|a| a.strip_prefix("--pad-press="))
        .map(String::from)
        .collect();
    // A build with bundled game data: DIR as its bundled folder.
    let bundled = args
        .iter()
        .find_map(|a| a.strip_prefix("--bundled="))
        .map(String::from);
    let tab = args
        .iter()
        .find_map(|a| a.strip_prefix("--tab="))
        .map(|t| match t {
            "replays" => th08::launcher::Tab::Replays,
            "stats" => th08::launcher::Tab::Stats,
            _ => {
                eprintln!("--tab={t}: expected replays or stats");
                std::process::exit(2);
            }
        });
    // Replays tab: the video export dialog of a replay, or an export in progress.
    let video = args
        .iter()
        .find_map(|a| a.strip_prefix("--video="))
        .map(String::from);
    strip_shot_switches(&mut args);
    let mut dirs = th08::launcher::desktop_dirs().tool();
    if let Some(c) = config {
        dirs.config = Some(c.clone());
        dirs.save = Some(c);
    }
    let mut s = match th08::settings::Settings::from_args(dirs, args) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
    if let Some(d) = bundled {
        s.set_from(th08::settings::Origin::Bundled, "game_dir", &d)
            .expect("game_dir accepts any path");
    }
    let gfx = pollster::block_on(th08_render::gfx::Gfx::headless()).expect("GPU adapter");
    let mut model = th08::launcher::Model::new(&s, th08::launcher::Folder::Native);
    if let Some(t) = model.translations() {
        let wait = |t: &th08::translations::Manager| {
            while t.busy() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        };
        if let (Some(code), Some(d)) = (&install, &s.game_dir) {
            t.install(code, th08::translations::Data::Dir(d.clone()));
            wait(t);
        }
        if list {
            t.load_list();
            wait(t);
        }
    }
    model.dev |= dev;
    if let Some(k) = capture {
        let Some(e) =
            th08::settings::find(&k).filter(|e| e.group == th08::settings::Group::Keyboard)
        else {
            eprintln!("--capture={k}: not a Keyboard row");
            std::process::exit(2);
        };
        model.start_capture(e.key);
    }
    pad_shot_rows(&mut model, &pad_press, pad_capture.as_deref());
    if let Some(t) = tab {
        model.set_tab(t);
    }
    if let Some(v) = video {
        video_shot(&mut model, &v);
    }
    model.view = match error {
        Some(e) => th08::launcher::View::Error(e),
        None => view,
    };
    th08::launcher::render_image(
        &gfx,
        model,
        size.unwrap_or((640, 480)),
        safe,
        &th08::launcher::ShotOpts::default(),
    )
    .save(&out)
    .expect("save png");
}

// The launchershot switches out of the th08 switches.
fn strip_shot_switches(args: &mut Vec<String>) {
    args.retain(|a| {
        a != "--developer"
            && !a.starts_with("--video=")
            && !a.starts_with("--tab=")
            && a != "--tr-list"
            && !a.starts_with("--error=")
            && !a.starts_with("--view=")
            && !a.starts_with("--safe=")
            && !a.starts_with("--config=")
            && !a.starts_with("--tr-install=")
            && !a.starts_with("--bundled=")
            && !a.starts_with("--capture=")
            && !a.starts_with("--pad-capture=")
            && !a.starts_with("--pad-press=")
    });
}

// `--video`: the export dialog of a replay file, or an export shown at DONE of TOTAL frames.
fn video_shot(model: &mut th08::launcher::Model, v: &str) {
    match v.split(':').collect::<Vec<_>>().as_slice() {
        ["ask", rpy] => model.shot_video_ask((*rpy).into()),
        ["progress", file, done, total] => model.shot_video_progress(
            (*file).to_string(),
            done.parse().unwrap_or(0),
            total.parse().unwrap_or(0),
        ),
        _ => {
            eprintln!("--video: expected ask:RPY or progress:FILE:DONE:TOTAL");
            std::process::exit(2);
        }
    }
}

// `--pad-press` captures fed their button, then the `--pad-capture` row left waiting.
fn pad_shot_rows(
    model: &mut th08::launcher::Model,
    pad_press: &[String],
    pad_capture: Option<&str>,
) {
    let pad_row = |k: &str| {
        th08::settings::find(k)
            .filter(|e| e.group == th08::settings::Group::Gamepad)
            .unwrap_or_else(|| {
                eprintln!("{k}: not a Gamepad row");
                std::process::exit(2);
            })
            .key
    };
    for p in pad_press {
        let Some((row, b)) = p.split_once(':') else {
            eprintln!("--pad-press={p}: expected PAD_ROW:BUTTON");
            std::process::exit(2);
        };
        let Some(b) = th08::platform::padmap::button(b) else {
            eprintln!("--pad-press={p}: not a bindable pad button");
            std::process::exit(2);
        };
        model.start_pad_capture(pad_row(row));
        model.capture_pad(b);
    }
    if let Some(k) = pad_capture {
        model.start_pad_capture(pad_row(k));
    }
}
