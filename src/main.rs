//! Native entry point: `th08 [game dir] [--switch value]...` (settings.rs) shows the launcher screen (launcher.rs)
//! or, with `--no-launcher` / tool switches, runs the game window directly (app.rs).

fn main() {
    env_logger::init();
    let mut s = match th08::settings::Settings::from_args(std::env::args().skip(1)) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
    if s.launcher && !s.is_tool_run() {
        th08::app::run_launcher(None, s);
        return;
    }
    let dir = match th08::launcher::resolve_game_dir(&s) {
        Ok(Some(d)) => d,
        Ok(None) => return,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    s.game_dir = Some(dir.clone());
    th08::settings::init(s);
    th08::app::run(th08::assets::Source::native(dir));
}
