//! Generates `src/rsqrt_amd.bin`, the run table of this CPU's `rsqrtss` over [0.5, 2) (05 §10, `fmath::rsqrt_amd`).
// usage: rsqrt_table [OUT] — run only on the reference CPU (AMD Ryzen 9 7945HX); checks the scaling law first.

#[cfg(target_arch = "x86_64")]
fn native(s: f32) -> f32 {
    use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
    // SAFETY: SSE is part of the x86_64 baseline.
    unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(s))) }
}

#[cfg(target_arch = "x86_64")]
fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "src/rsqrt_amd.bin".into());
    let base: Vec<u32> = ((126u32 << 23)..(128u32 << 23))
        .map(|b| native(f32::from_bits(b)).to_bits())
        .collect();
    let mut bad = 0u64;
    for e in 1u32..255 {
        let b0 = if e % 2 == 0 { 126 } else { 127 };
        let k = (e as i32 - b0 as i32) / 2;
        for m in (0..1u32 << 23).step_by(97) {
            let want = (base[((b0 - 126) << 23 | m) as usize] as i32 - (k << 23)) as u32;
            bad += (native(f32::from_bits(e << 23 | m)).to_bits() != want) as u64;
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
