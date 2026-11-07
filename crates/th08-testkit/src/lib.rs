//! Test support (dev-dependency only): tests that need game data or a GPU report a skip, or fail under `TH08_REQUIRE_DATA=1`.

use std::io::Write;
use std::path::PathBuf;

pub const NO_DATA: &str = "no game data (TH08_DIR unset)";
pub const NO_GPU: &str = "no GPU adapter";

// `TH08_REQUIRE_DATA=1` (arch_check): a test that would skip fails instead.
pub fn required() -> bool {
    std::env::var_os("TH08_REQUIRE_DATA").is_some_and(|v| v == "1")
}

// One line on the process stderr, which the test harness does not capture, so a skip shows without --nocapture.
pub fn skip(why: &str) {
    let t = std::thread::current();
    let name = t.name().unwrap_or("?");
    assert!(!required(), "{name}: {why}, and TH08_REQUIRE_DATA=1");
    let _ = writeln!(std::io::stderr().lock(), "skipped: {name}: {why}");
}

// `v`, with a skip for `why` reported when it is None.
pub fn need<T>(v: Option<T>, why: &str) -> Option<T> {
    if v.is_none() {
        skip(why);
    }
    v
}

// The game directory (TH08_DIR), with a skip reported when it is unset.
pub fn game_dir() -> Option<PathBuf> {
    need(std::env::var_os("TH08_DIR").map(PathBuf::from), NO_DATA)
}
