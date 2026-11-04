//! th08 debug tooling (STEP10_CRATES.md §1): session / demo trace runners and comparisons, draw-log files and
//! reports, xz streams. Native only; the bins under src/bin keep their names.
#![cfg(not(target_arch = "wasm32"))]

pub mod demosync;
pub mod drawlog;
pub mod drawreport;
pub mod fontchain;
pub mod packcheck;
pub mod session;
pub mod thcrap;
pub mod thpatch;
pub mod xz;

/// A count or index as a trace column (i64); exactly `as i64`.
#[expect(clippy::cast_possible_wrap, reason = "counts and indices < 2^63")]
#[must_use]
pub const fn int(n: usize) -> i64 {
    n as i64
}
