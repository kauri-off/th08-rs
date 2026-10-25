//! D3DX CPU-dispatched helpers used by game logic (05 §10 "Vector normalise").

/// `D3DXVec3Normalize`, SSE variant 0x48EFB0 (05 §10): s = (x²+y²)+z², m = (0.5·r)·(3 − (s·r)·r) with r = rsqrtss(s).
pub fn vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let s = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if s < f32::from_bits(0x2880_0000) {
        return [0.0; 3];
    }
    let r = rsqrt(s);
    let m = (0.5 * r) * (3.0 - (s * r) * r);
    [m * v[0], m * v[1], m * v[2]]
}

// Native rsqrtss: the sessions were recorded on an x86 CPU, so its table reproduces them (05 §10).
#[cfg(target_arch = "x86_64")]
fn rsqrt(s: f32) -> f32 {
    use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
    // SAFETY: SSE is part of the x86_64 baseline.
    unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(s))) }
}

#[cfg(target_arch = "x86")]
fn rsqrt(s: f32) -> f32 {
    use std::arch::x86::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
    // SAFETY: the original requires SSE for this path; every x86 target the port supports has it.
    unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(s))) }
}

// Exact 1/sqrt instead of the hardware approximation: can differ from the original in the last bit.
#[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
fn rsqrt(s: f32) -> f32 {
    1.0 / s.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_example_matches_spec() {
        let n = vec3_normalize([0.0, 500.0, 460.0]);
        assert_eq!(n[0], 0.0);
        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
        assert_eq!([n[1].to_bits(), n[2].to_bits()], [0x3F3C65F9, 0x3F2D5393]);
    }

    #[test]
    fn tiny_vector_is_zero() {
        assert_eq!(vec3_normalize([1e-24, 0.0, 0.0]), [0.0; 3]);
        assert_eq!(vec3_normalize([0.0; 3]), [0.0; 3]);
    }
}
