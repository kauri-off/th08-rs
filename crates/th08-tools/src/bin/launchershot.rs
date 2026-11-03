//! Port tool: renders the launcher screen offscreen to a PNG.
//! `launchershot OUT.png [WxH] [--developer] [--error=TEXT] [--view=waiting|loading|portmenu] [--safe=L,T,R,B]
//! [th08 switches]...` (`--safe`: safe-area insets in px, as an Android cutout / rounded corners give).

fn main() {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: launchershot OUT.png [WxH] [--developer] [--error=TEXT] [--view=waiting|loading|portmenu] [--safe=L,T,R,B] [th08 switches]..."
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
    args.retain(|a| {
        a != "--developer"
            && !a.starts_with("--error=")
            && !a.starts_with("--view=")
            && !a.starts_with("--safe=")
    });
    let s = match th08::settings::Settings::from_args(th08::launcher::desktop_dirs(), args) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
    let gfx = pollster::block_on(th08_render::gfx::Gfx::headless()).expect("GPU adapter");
    let mut model = th08::launcher::Model::new(&s, th08::launcher::Folder::Native);
    model.dev |= dev;
    model.view = match error {
        Some(e) => th08::launcher::View::Error(e),
        None => view,
    };
    th08::launcher::render_image(&gfx, model, size.unwrap_or((640, 480)), safe)
        .save(&out)
        .expect("save png");
}
