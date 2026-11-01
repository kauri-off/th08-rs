//! Native side of the fmath cross-target check: prints one digest line per (function, chunk).

use fmath_xplat::{CHUNKS, KINDS, digest};

fn main() {
    for k in 0..KINDS {
        for c in 0..CHUNKS {
            println!("{k} {c} {:016x}", digest(k, c));
        }
    }
}
