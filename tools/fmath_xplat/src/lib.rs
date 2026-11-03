//! Cross-target check of `crates/th08-core/src/fmath.rs` (05 §10): per-chunk digests of every fmath function, wasm vs native.

#[path = "../../../crates/th08-core/src/fmath.rs"]
#[allow(dead_code)]
mod fmath;

pub const KINDS: u32 = 10;
pub const CHUNK: u32 = 1 << 16;
pub const CHUNKS: u32 = 16;

fn xs(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

// Even inputs: gameplay range (angles ±8, coordinates ±600, acos in [−1, 1]); odd inputs: any f32 bit pattern.
fn input(kind: u32, i: u32) -> (f32, f32) {
    let mut s = 0x9E37_79B9_7F4A_7C15u64 ^ ((kind as u64) << 40) ^ i as u64;
    for _ in 0..3 {
        xs(&mut s);
    }
    let r = xs(&mut s);
    if i % 2 == 1 {
        return (f32::from_bits(r as u32), f32::from_bits((r >> 32) as u32));
    }
    let u = |b: u64| (b & 0xFF_FFFF) as f32 / 16_777_216.0 - 0.5;
    let (a, b) = (u(r), u(r >> 24));
    match kind {
        5 => (a * 1200.0, b * 1200.0),
        7 => (a * 2.0, b),
        9 => (f32::from_bits(0x2880_0000 + (r as u32 % 0x2000_0000)), b),
        _ => (a * 16.0, b * 1200.0),
    }
}

fn canon(x: f32) -> u32 {
    if x.is_nan() { 0x7FC0_0000 } else { x.to_bits() }
}

fn eval(kind: u32, x: f32, y: f32) -> u64 {
    let one = |v: f32| canon(v) as u64;
    match kind {
        0 => one(fmath::sin_f(x)),
        1 => one(fmath::cos_f(x)),
        2 => {
            let p = fmath::polar(x, y);
            one(p[0]) | one(p[1]) << 32
        }
        3 => one(fmath::sin_mul(x, y as f64)),
        4 => one(fmath::cos_mul(x, y as f64)),
        5 => one(fmath::atan2_f(x, y)),
        6 => one(fmath::tan_f(x)),
        7 => one(fmath::acos_f(x)),
        8 => one(fmath::atan_f(x)),
        _ => one(fmath::rsqrt_amd(x)),
    }
}

/// FNV-1a over the canonicalised outputs of inputs [chunk·CHUNK, (chunk+1)·CHUNK) of one function.
#[unsafe(no_mangle)]
pub extern "C" fn digest(kind: u32, chunk: u32) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for i in chunk * CHUNK..(chunk + 1) * CHUNK {
        let (x, y) = input(kind, i);
        for b in eval(kind, x, y).to_le_bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01B3);
        }
    }
    h
}

/// Output of input i, to locate a differing chunk's first input.
#[unsafe(no_mangle)]
pub extern "C" fn value(kind: u32, i: u32) -> u64 {
    let (x, y) = input(kind, i);
    eval(kind, x, y)
}

/// Input i as bits (x | y << 32).
#[unsafe(no_mangle)]
pub extern "C" fn input_bits(kind: u32, i: u32) -> u64 {
    let (x, y) = input(kind, i);
    x.to_bits() as u64 | (y.to_bits() as u64) << 32
}
