//! Pinned transcendental math and the AMD `rsqrtss` model (05 §10): the only place logic computes sin/cos/atan2/…
// Uses only `core` and the pinned `libm`, so `tools/fmath_xplat` includes this file unchanged for the wasm check.

// 05 §10: x87 fsin/fcos/fpatan/fptan are extended; the port evaluates them in f64 with the pinned `libm` (the same
// bits on every target, unlike the platform libm) and rounds once to f32. Single-precision libm is never used.

/// `Math_Sin` result stored to f32 (05 §10 "rounded to f32 first").
pub fn sin_f(a: f32) -> f32 {
    libm::sin(f64::from(a)) as f32
}

/// `Math_Cos` result stored to f32 (05 §10 "rounded to f32 first").
pub fn cos_f(a: f32) -> f32 {
    libm::cos(f64::from(a)) as f32
}

/// `Math_SinCosF32` 0x433880: `fsincos`, both stored as f32 (05 §10). Returns (sin, cos).
pub fn sin_cos_f(a: f32) -> (f32, f32) {
    (sin_f(a), cos_f(a))
}

/// `Vec3_FromPolar` 0x4286E0 (05 §10): f32(cos a·r), f32(sin a·r), the product taken in f64 and rounded once.
pub fn polar(a: f32, r: f32) -> [f32; 3] {
    let (a, r) = (f64::from(a), f64::from(r));
    [(libm::cos(a) * r) as f32, (libm::sin(a) * r) as f32, 0.0]
}

/// `call Math_Sin; fmul` site (05 §10): f32(sin a·k) rounded once; k is f64 so qword constants stay exact.
pub fn sin_mul(a: f32, k: f64) -> f32 {
    (libm::sin(f64::from(a)) * k) as f32
}

/// `call Math_Cos; fmul` site (05 §10): f32(cos a·k) rounded once.
pub fn cos_mul(a: f32, k: f64) -> f32 {
    (libm::cos(f64::from(a)) * k) as f32
}

/// x87 `fpatan` rounded to f32 (05 §10 rule for extended transcendentals).
pub fn atan2_f(y: f32, x: f32) -> f32 {
    libm::atan2(f64::from(y), f64::from(x)) as f32
}

/// x87 `fptan` rounded to f32 (ANM op 63, 05 §10 rule).
pub fn tan_f(x: f32) -> f32 {
    libm::tan(f64::from(x)) as f32
}

/// CRT `acos` (x87) rounded to f32 (ANM op 64, 05 §10 rule).
pub fn acos_f(x: f32) -> f32 {
    libm::acos(f64::from(x)) as f32
}

/// x87 `fpatan(x, 1)` rounded to f32 (ANM op 65, 05 §10 rule).
pub fn atan_f(x: f32) -> f32 {
    libm::atan(f64::from(x)) as f32
}

// `rsqrtss` of the reference CPU (AMD Ryzen 9 7945HX, 05 §10 "an exact model of that CPU's table elsewhere").
// rsqrt_amd.bin: runs of equal output over the 2²⁴ inputs of [0.5, 2) (exponents 126, 127), each run a pair of
// little-endian u32 (index of its first input = (exponent − 126)·2²³ + mantissa, output bits). Written by
// `src/bin/rsqrt_table.rs`; every other exponent follows rsqrt(4ᵏx) = 2⁻ᵏ·rsqrt(x), exact on that CPU.
const RSQRT_BIN: &[u8] = include_bytes!("rsqrt_amd.bin");
const RSQRT_RUNS: usize = RSQRT_BIN.len() / 8;
static RSQRT_START: [u32; RSQRT_RUNS] = rsqrt_column(0);
static RSQRT_OUT: [u32; RSQRT_RUNS] = rsqrt_column(4);

const fn rsqrt_column(off: usize) -> [u32; RSQRT_RUNS] {
    let mut t = [0u32; RSQRT_RUNS];
    let mut i = 0;
    while i < RSQRT_RUNS {
        let b = 8 * i + off;
        t[i] = u32::from_le_bytes([
            RSQRT_BIN[b],
            RSQRT_BIN[b + 1],
            RSQRT_BIN[b + 2],
            RSQRT_BIN[b + 3],
        ]);
        i += 1;
    }
    t
}

/// Number of runs in the checked-in table (report/tests).
pub const RSQRT_TABLE_RUNS: usize = RSQRT_RUNS;

/// AMD `rsqrtss`, bit-exact on every target. Inputs outside the positive normal range follow the instruction's
/// IEEE special cases (±0 and subnormals as zero → ±∞, negative → default NaN, +∞ → +0).
pub fn rsqrt_amd(s: f32) -> f32 {
    let b = s.to_bits();
    let e = (b >> 23) & 0xFF;
    if s.is_nan() {
        return f32::from_bits(b | 0x0040_0000);
    }
    if e == 0 {
        return f32::from_bits((b & 0x8000_0000) | 0x7F80_0000);
    }
    if b >> 31 != 0 {
        return f32::from_bits(0xFFC0_0000);
    }
    if e == 0xFF {
        return 0.0;
    }
    let base = if e.is_multiple_of(2) { 126 } else { 127 };
    let idx = (base - 126) << 23 | (b & 0x7F_FFFF);
    let run = RSQRT_START.partition_point(|&st| st <= idx) - 1;
    let k = (e.cast_signed() - base.cast_signed()) / 2;
    f32::from_bits((RSQRT_OUT[run].cast_signed() - (k << 23)).cast_unsigned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 05 §10 (RE-Q58 AS): f32 of the extended cos, not cosf (0xC0045DCE → 0xBEF44F55, cosf gives …54).
    #[test]
    fn cos_rounds_once_spec_example() {
        assert_eq!(cos_f(f32::from_bits(0xC004_5DCE)).to_bits(), 0xBEF4_4F55);
    }

    // 05 §10: Vec3_FromPolar rounds the extended product once, not f32(cos a)·r.
    #[test]
    fn polar_rounds_once() {
        let mut differs = 0;
        for i in 0..1000 {
            let (a, r) = (i as f32 * 0.0123 - 6.0, 1.0 + i as f32 * 0.37);
            let v = polar(a, r);
            assert_eq!(v[0], (libm::cos(f64::from(a)) * f64::from(r)) as f32);
            assert_eq!(v[1], (libm::sin(f64::from(a)) * f64::from(r)) as f32);
            differs += u32::from(v[0] != cos_f(a) * r || v[1] != sin_f(a) * r);
        }
        assert!(differs > 0);
    }

    #[test]
    fn rsqrt_table_shape() {
        assert_eq!(RSQRT_BIN.len() % 8, 0);
        assert_eq!(RSQRT_START[0], 0);
        assert!(RSQRT_START.windows(2).all(|w| w[0] < w[1]));
        assert!(RSQRT_OUT.iter().all(|o| o & 0x7FF == 0));
    }

    // Native `rsqrtss` comparisons (unsafe intrinsics, not in core): `rsqrt_table --verify` and its bin tests.
}
