//! Renders the launcher screen offscreen to a PNG (`--safe`: safe-area insets in px, as an Android cutout gives).

fn main() {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: launchershot OUT.png [WxH] [--developer] [--error=TEXT] [--view=waiting|loading|portmenu] [--safe=L,T,R,B] [--config=DIR] [--tr-install=CODE] [--tr-list] [th08 switches]..."
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
        Some("portmenu") => th08::launcher::View::PortMenu,
        Some(v) => {
            eprintln!("--view={v}: expected waiting, loading or portmenu");
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
    args.retain(|a| {
        a != "--developer"
            && a != "--tr-list"
            && !a.starts_with("--error=")
            && !a.starts_with("--view=")
            && !a.starts_with("--safe=")
            && !a.starts_with("--config=")
            && !a.starts_with("--tr-install=")
    });
    let mut dirs = th08::launcher::desktop_dirs();
    if let Some(c) = config {
        dirs.config = Some(c.clone());
        dirs.save = Some(c);
    }
    let s = match th08::settings::Settings::from_args(dirs, args) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
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
    model.view = match error {
        Some(e) => th08::launcher::View::Error(e),
        None => view,
    };
    th08::launcher::render_image(&gfx, model, size.unwrap_or((640, 480)), safe)
        .save(&out)
        .expect("save png");
}
