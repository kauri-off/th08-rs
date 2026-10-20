//! Port tool: renders the launcher screen offscreen to a PNG. `launchershot OUT.png [WxH] [th08 switches]...`

fn main() {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: launchershot OUT.png [WxH] [th08 switches]...");
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
    let s = match th08::settings::Settings::from_args(args) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
    let gfx = pollster::block_on(th08::gfx::Gfx::headless()).expect("GPU adapter");
    let model = th08::launcher::Model::new(&s, th08::launcher::Folder::Native);
    th08::launcher::render_image(&gfx, model, size.unwrap_or((640, 480)))
        .save(&out)
        .expect("save png");
}
