//! Native side of the fmath cross-target check: digest lines per (function, chunk), raw-bit digests, NaN probes.

use fmath_xplat::{CHUNKS, KINDS, NAN_PROBES, digest, digest_raw, nan_probe};

fn main() {
    for k in 0..KINDS {
        for c in 0..CHUNKS {
            println!("{k} {c} {:016x}", digest(k, c));
        }
    }
    for k in 0..KINDS {
        for c in 0..CHUNKS {
            println!("raw {k} {c} {:016x}", digest_raw(k, c));
        }
    }
    for i in 0..NAN_PROBES {
        println!("nan {i} {:08x}", nan_probe(i));
    }
}
