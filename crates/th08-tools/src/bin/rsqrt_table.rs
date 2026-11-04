//! Generates `crates/th08-core/src/rsqrt_amd.bin`, the run table of this CPU's `rsqrtss` over [0.5, 2) (05 §10, `fmath::rsqrt_amd`),
//! and checks the table model against the native instruction (`--verify`; moved out of fmath's tests in 10b).
// usage: rsqrt_table [OUT] | --verify — run only on the reference CPU (AMD Ryzen 9 7945HX); checks the scaling law first.

#[cfg(target_arch = "x86_64")]
fn native(s: f32) -> f32 {
    use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
    // SAFETY: SSE is part of the x86_64 baseline.
    unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(s))) }
}

// The model equals native only on the reference CPU (AMD Zen 4); other x86 vendors have a different rsqrtss table.
#[cfg(target_arch = "x86_64")]
fn on_reference_cpu() -> bool {
    use th08_core::fmath::rsqrt_amd;
    native(f32::from_bits(0x3F80_0001)).to_bits()
        == rsqrt_amd(f32::from_bits(0x3F80_0001)).to_bits()
        && native(500.0 * 500.0 + 460.0 * 460.0).to_bits()
            == rsqrt_amd(500.0 * 500.0 + 460.0 * 460.0).to_bits()
}

// First input where `fmath::rsqrt_amd` differs from `rsqrtss`.
#[cfg(target_arch = "x86_64")]
fn first_diff(mut inputs: impl Iterator<Item = f32>) -> Option<u32> {
    inputs
        .find(|&s| th08_core::fmath::rsqrt_amd(s).to_bits() != native(s).to_bits())
        .map(f32::to_bits)
}

// Every input of the stored range [0.5, 2).
#[cfg(target_arch = "x86_64")]
fn verify_half_to_two() -> Option<u32> {
    first_diff(((126u32 << 23)..(128u32 << 23)).map(f32::from_bits))
}

// Every exponent (mantissa step 997) plus zeros, denormals, negatives and infinity.
#[cfg(target_arch = "x86_64")]
fn verify_other_exponents() -> Option<u32> {
    let grid = (1u32..255).flat_map(|e| {
        (0..1u32 << 23)
            .step_by(997)
            .map(move |m| f32::from_bits(e << 23 | m))
    });
    let special = [
        0.0f32,
        -0.0,
        f32::from_bits(1),
        f32::from_bits(0x807F_FFFF),
        -1.0,
        f32::INFINITY,
    ];
    first_diff(grid.chain(special))
}

#[cfg(target_arch = "x86_64")]
fn verify() -> anyhow::Result<()> {
    anyhow::ensure!(on_reference_cpu(), "not the reference CPU");
    if let Some(b) = verify_half_to_two().or_else(verify_other_exponents) {
        anyhow::bail!("rsqrt_amd differs from rsqrtss at input {b:08x}");
    }
    println!("rsqrt_amd equals rsqrtss");
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn main() -> anyhow::Result<()> {
    let arg = std::env::args().nth(1);
    if arg.as_deref() == Some("--verify") {
        return verify();
    }
    let out = arg.unwrap_or_else(|| "crates/th08-core/src/rsqrt_amd.bin".into());
    let base: Vec<u32> = ((126u32 << 23)..(128u32 << 23))
        .map(|b| native(f32::from_bits(b)).to_bits())
        .collect();
    let mut bad = 0u64;
    for e in 1u32..255 {
        let b0: u32 = if e % 2 == 0 { 126 } else { 127 };
        let k = (e.cast_signed() - b0.cast_signed()) / 2;
        for m in (0..1u32 << 23).step_by(97) {
            let want =
                (base[((b0 - 126) << 23 | m) as usize].cast_signed() - (k << 23)).cast_unsigned();
            bad += u64::from(native(f32::from_bits(e << 23 | m)).to_bits() != want);
        }
    }
    anyhow::ensure!(
        bad == 0,
        "scaling law rsqrt(4^k x) = 2^-k rsqrt(x) fails on {bad} inputs: not the reference CPU"
    );
    let mut bin = Vec::new();
    for (i, &o) in base.iter().enumerate() {
        if i == 0 || base[i - 1] != o {
            bin.extend_from_slice(&(i as u32).to_le_bytes());
            bin.extend_from_slice(&o.to_le_bytes());
        }
    }
    std::fs::write(&out, &bin)?;
    println!("{out}: {} runs, {} bytes", bin.len() / 8, bin.len());
    Ok(())
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {
    eprintln!("rsqrt_table needs x86_64 rsqrtss");
    std::process::exit(1);
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    #[test]
    fn rsqrt_table_equals_native_over_half_to_two() {
        if !on_reference_cpu() {
            eprintln!("rsqrt: not the reference CPU, native comparison skipped");
            return;
        }
        assert_eq!(verify_half_to_two().map(|b| format!("{b:08x}")), None);
    }

    #[test]
    fn rsqrt_table_equals_native_other_exponents() {
        if !on_reference_cpu() {
            return;
        }
        assert_eq!(verify_other_exponents().map(|b| format!("{b:08x}")), None);
    }
}
