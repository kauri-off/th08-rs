//! Native entry point: `th08 [game dir] [--switch value]...` (settings.rs) shows the launcher screen (launcher.rs)
//! or, with `--no-launcher` / tool switches, runs the game window directly (app.rs).
// Release builds on Windows are GUI-subsystem (no console window); see attach_parent_console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

/// Started from a terminal, reuse its console so `--help`, errors and logs are visible; redirected
/// output keeps its handle.
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    unsafe {
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() && GetStdHandle(STD_ERROR_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn main() {
    #[cfg(windows)]
    attach_parent_console();
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
