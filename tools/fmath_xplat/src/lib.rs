//! Cross-target check of `crates/th08-core/src/fmath.rs` (05 §10): per-chunk digests of every fmath function, wasm vs native.

#[path = "../../../crates/th08-core/src/fmath.rs"]
#[allow(dead_code, unexpected_cfgs)]
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

fn eval(kind: u32, x: f32, y: f32, raw: bool) -> u64 {
    let one = |v: f32| if raw { v.to_bits() } else { canon(v) } as u64;
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

fn fnv(h: u64, v: u64) -> u64 {
    v.to_le_bytes()
        .iter()
        .fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
}

fn chunk_digest(kind: u32, chunk: u32, raw: bool) -> u64 {
    (chunk * CHUNK..(chunk + 1) * CHUNK).fold(0xCBF2_9CE4_8422_2325u64, |h, i| {
        let (x, y) = input(kind, i);
        fnv(h, eval(kind, x, y, raw))
    })
}

/// FNV-1a over the canonicalised outputs of inputs [chunk·CHUNK, (chunk+1)·CHUNK) of one function.
#[unsafe(no_mangle)]
pub extern "C" fn digest(kind: u32, chunk: u32) -> u64 {
    chunk_digest(kind, chunk, false)
}

/// The same over the raw output bits: NaN sign and payload included.
#[unsafe(no_mangle)]
pub extern "C" fn digest_raw(kind: u32, chunk: u32) -> u64 {
    chunk_digest(kind, chunk, true)
}

pub const NAN_PROBES: u32 = 8;

/// Bits of a freshly made NaN per probe: 0/0, inf−inf, 0·inf, sqrt(−1), f32 rem by 0, f64 0/0 → f32, acos_f(2),
/// and a quiet NaN input through `+ 1` (payload kept).
#[unsafe(no_mangle)]
pub extern "C" fn nan_probe(i: u32) -> u32 {
    let z = std::hint::black_box(0.0f32);
    let inf = std::hint::black_box(f32::INFINITY);
    let v = match i {
        0 => z / z,
        1 => inf - inf,
        2 => z * inf,
        3 => (z - 1.0).sqrt(),
        4 => (z + 1.0) % z,
        5 => (f64::from(z) / f64::from(z)) as f32,
        6 => fmath::acos_f(z + 2.0),
        _ => f32::from_bits(std::hint::black_box(0xFFC0_1234)) + 1.0,
    };
    v.to_bits()
}

/// Output of input i, to locate a differing chunk's first input.
#[unsafe(no_mangle)]
pub extern "C" fn value(kind: u32, i: u32) -> u64 {
    let (x, y) = input(kind, i);
    eval(kind, x, y, false)
}

/// Input i as bits (x | y << 32).
#[unsafe(no_mangle)]
pub extern "C" fn input_bits(kind: u32, i: u32) -> u64 {
    let (x, y) = input(kind, i);
    x.to_bits() as u64 | (y.to_bits() as u64) << 32
}
