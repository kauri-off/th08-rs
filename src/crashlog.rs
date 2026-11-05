//! Failure reports for players: `th08.log` (warnings and up), `last_error.txt` from panics and start-up errors, and save-write errors for the in-game toast.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

pub const LAST_ERROR: &str = "last_error.txt";
pub const LOG_FILE: &str = "th08.log";
// The log stops growing here; the run's first warnings are the useful ones.
#[cfg(not(target_arch = "wasm32"))]
const LOG_MAX: u64 = 256 * 1024;
// Warnings kept in memory and copied into `last_error.txt`.
const TAIL: usize = 30;
const SAVE_ERRORS: usize = 4;

#[derive(Default)]
struct State {
    #[cfg(not(target_arch = "wasm32"))]
    dir: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    file: Option<std::fs::File>,
    #[cfg(not(target_arch = "wasm32"))]
    written: u64,
    tail: VecDeque<String>,
    saves: SaveQueue,
}

// Save failures not shown yet: at most a few, oldest first, one per file.
#[derive(Default)]
struct SaveQueue(VecDeque<(String, String)>);

impl SaveQueue {
    fn push(&mut self, name: &str, err: &str) {
        if self.0.iter().any(|(n, _)| n == name) {
            return;
        }
        if self.0.len() == SAVE_ERRORS {
            self.0.pop_front();
        }
        self.0.push_back((name.to_owned(), err.to_owned()));
    }
}

static STATE: Mutex<State> = Mutex::new(State {
    #[cfg(not(target_arch = "wasm32"))]
    dir: None,
    #[cfg(not(target_arch = "wasm32"))]
    file: None,
    #[cfg(not(target_arch = "wasm32"))]
    written: 0,
    tail: VecDeque::new(),
    saves: SaveQueue(VecDeque::new()),
});

// Nothing panics while the lock is held (lines are formatted before), so a poisoned lock is still consistent.
fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

// A logger that also keeps warnings and errors: in `th08.log` and in the tail for `last_error.txt`.
pub struct Tee {
    inner: Box<dyn log::Log>,
}

impl Tee {
    // Installs `inner` (which filters itself) behind the tee; warnings and up always pass.
    pub fn install(inner: Box<dyn log::Log>, inner_max: log::LevelFilter) {
        if log::set_boxed_logger(Box::new(Tee { inner })).is_ok() {
            log::set_max_level(inner_max.max(log::LevelFilter::Warn));
        }
    }
}

impl log::Log for Tee {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn || self.inner.enabled(m)
    }

    fn log(&self, r: &log::Record<'_>) {
        if self.inner.enabled(r.metadata()) {
            self.inner.log(r);
        }
        if r.level() <= log::Level::Warn {
            record(&format!(
                "{} {} {}: {}",
                now(),
                r.level(),
                r.target(),
                r.args()
            ));
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

fn record(line: &str) {
    let mut s = state();
    if s.tail.len() == TAIL {
        s.tail.pop_front();
    }
    s.tail.push_back(line.to_owned());
    #[cfg(not(target_arch = "wasm32"))]
    if s.written < LOG_MAX {
        use std::io::Write;
        let n = line.len() as u64 + 1;
        if let Some(f) = s.file.as_mut()
            && writeln!(f, "{line}").is_ok()
        {
            s.written += n;
        }
    }
}

// The config folder for `th08.log` and `last_error.txt`; the log starts empty for this run.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_dir(dir: Option<&std::path::Path>) {
    let file = dir.and_then(|d| {
        std::fs::create_dir_all(d).ok()?;
        std::fs::File::create(d.join(LOG_FILE)).ok()
    });
    let mut s = state();
    s.dir = dir.map(std::path::Path::to_path_buf);
    s.file = file;
    s.written = 0;
}

// Panics write `last_error.txt` (with a backtrace), then run the previous hook.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        report("panic", &info.to_string(), Some(&bt.to_string()));
        prev(info);
    }));
}

// The report text: time, version, what failed, the backtrace and the last warnings.
pub fn report_text(kind: &str, msg: &str, backtrace: Option<&str>, tail: &[String]) -> String {
    let mut t = format!(
        "th08 last error\ntime: {}\nversion: {}\nplatform: {}-{}\n\n{kind}:\n{msg}\n",
        now(),
        crate::launcher::version_line(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    if let Some(bt) = backtrace.filter(|b| !b.trim().is_empty()) {
        t.push_str("\nbacktrace:\n");
        t.push_str(bt);
        t.push('\n');
    }
    if !tail.is_empty() {
        t.push_str("\nlast warnings:\n");
        for l in tail {
            t.push_str(l);
            t.push('\n');
        }
    }
    t
}

// Writes `last_error.txt` (native) or shows the text in the page (web); returns the file written.
pub fn report(kind: &str, msg: &str, backtrace: Option<&str>) -> Option<std::path::PathBuf> {
    let tail: Vec<String> = state().tail.iter().cloned().collect();
    let text = report_text(kind, msg, backtrace, &tail);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = state().dir.clone()?;
        let path = dir.join(LAST_ERROR);
        match crate::platform::write_atomic(&path, text.as_bytes()) {
            Ok(()) => Some(path),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                None
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        show_in_page(&text);
        None
    }
}

// The page's error box (web/index.html `#error`), created when missing.
#[cfg(target_arch = "wasm32")]
fn show_in_page(text: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let el = doc.get_element_by_id("error").or_else(|| {
        let e = doc.create_element("pre").ok()?;
        e.set_id("error");
        doc.body()?.append_child(&e).ok()?;
        Some(e)
    });
    if let Some(e) = el {
        e.set_text_content(Some(text));
        let _ = e.remove_attribute("hidden");
    }
}

// The previous run's report in `dir`, if any.
#[cfg(not(target_arch = "wasm32"))]
pub fn last_error(dir: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(dir.join(LAST_ERROR))
        .ok()
        .filter(|t| !t.trim().is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_last_error(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir.join(LAST_ERROR));
}

// The failure line of a report: the text after the `panic:` / `start-up failure:` heading.
pub fn headline(report: &str) -> &str {
    let Some((_, body)) = report
        .split_once("\n\n")
        .and_then(|(_, b)| b.split_once(":\n"))
    else {
        return report.trim();
    };
    let end = ["\nbacktrace:\n", "\nlast warnings:\n"]
        .iter()
        .filter_map(|m| body.find(m))
        .min()
        .unwrap_or(body.len());
    body[..end].trim()
}

// A save write failed (the caller logs it): kept for the in-game toast.
pub fn save_failed(name: &str, err: &str) {
    state().saves.push(name, err);
}

// The oldest save failure not shown yet: (file name, error).
pub fn take_save_error() -> Option<(String, String)> {
    state().saves.0.pop_front()
}

#[cfg(not(target_arch = "wasm32"))]
fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(target_arch = "wasm32")]
fn now() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_and_headline() {
        let t = report_text(
            "start-up failure",
            "th08.dat: not found\nsecond line",
            Some("  0: main"),
            &["w1".into()],
        );
        assert!(
            t.contains("version: ") && t.contains("backtrace:\n  0: main") && t.ends_with("w1\n")
        );
        assert_eq!(headline(&t), "th08.dat: not found\nsecond line");
        let t = report_text("panic", "boom", None, &[]);
        assert!(!t.contains("backtrace") && !t.contains("last warnings"));
        assert_eq!(headline(&t), "boom");
    }

    #[test]
    fn save_queue() {
        let mut q = SaveQueue::default();
        q.push("score.dat", "disk full");
        q.push("score.dat", "disk full");
        for n in ["th08.cfg", "a.rpy", "b.rpy", "c.rpy"] {
            q.push(n, "read-only");
        }
        let names: Vec<&str> = q.0.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["th08.cfg", "a.rpy", "b.rpy", "c.rpy"]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn log_file_and_report() {
        let dir = std::env::temp_dir().join(format!("th08-crashlog-{}", std::process::id()));
        set_dir(Some(&dir));
        record("2026 WARN x: first");
        let path = report("start-up failure", "no adapter", None).unwrap();
        let text = last_error(&dir).unwrap();
        assert!(text.contains("no adapter") && text.contains("first"));
        assert!(
            std::fs::read_to_string(dir.join(LOG_FILE))
                .unwrap()
                .contains("first")
        );
        clear_last_error(&dir);
        assert!(!path.exists() && last_error(&dir).is_none());
        install_panic_hook();
        assert!(std::panic::catch_unwind(|| panic!("boom")).is_err());
        drop(std::panic::take_hook());
        let text = last_error(&dir).unwrap();
        assert!(text.contains("panic:\n") && text.contains("boom") && text.contains("backtrace:"));
        set_dir(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
