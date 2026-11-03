//! th08 debug tooling (STEP10_CRATES.md §1): session / demo trace runners and comparisons, draw-log files and
//! reports, xz streams. Native only; the bins under src/bin keep their names.
#![cfg(not(target_arch = "wasm32"))]

pub mod demosync;
pub mod drawlog;
pub mod drawreport;
pub mod session;
pub mod xz;
