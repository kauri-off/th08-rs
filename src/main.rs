//! Native entry point: shows the launcher screen or, with `--no-launcher` / tool switches, runs the game directly.
// Release builds on Windows are GUI-subsystem (no console window); see attach_parent_console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

/// Started from a terminal, reuse its console so `--help`, errors and logs are visible; redirected output keeps its handle.
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
    let logger = env_logger::Builder::from_default_env().build();
    let max = logger.filter();
    th08::crashlog::Tee::install(Box::new(logger), max);
    let mut s = match th08::settings::Settings::game_from_args(
        th08::launcher::desktop_dirs(),
        std::env::args().skip(1),
    ) {
        Ok(s) => s,
        Err(e) => e.exit(),
    };
    th08::crashlog::set_dir(s.dirs.config.as_deref());
    th08::crashlog::install_panic_hook();
    // A saved `launcher = off` still goes through the launcher window, so Shift held at start-up can open it.
    let saved_off = !s.launcher && s.origin("launcher") == th08::settings::Origin::PortCfg;
    if (s.launcher || saved_off) && !s.is_tool_run() {
        th08::launcher::prefill_bundled_game_dir(&mut s);
        th08::app::run_launcher(None, s);
        return;
    }
    let dir = match th08::launcher::resolve_game_dir(&s) {
        Ok(Some(d)) => d,
        Ok(None) => return,
        Err(e) => {
            eprintln!("{e}");
            th08::crashlog::report("start-up failure", &e.to_string(), None);
            std::process::exit(1);
        }
    };
    s.game_dir = Some(dir.clone());
    th08::app::run(th08::sources::native(dir), s);
}
