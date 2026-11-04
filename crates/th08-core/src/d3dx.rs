//! D3DX CPU-dispatched helpers used by game logic (05 §10 "Vector normalise").

/// `D3DXVec3Normalize`, SSE variant 0x48EFB0: s = (x²+y²)+z², m = (0.5·r)·(3 − (s·r)·r) with r = rsqrtss(s).
/// rsqrtss is the reference CPU's (AMD) table on every target and host CPU (`fmath::rsqrt_amd`).
pub fn vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let s = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if s < f32::from_bits(0x2880_0000) {
        return [0.0; 3];
    }
    let r = crate::fmath::rsqrt_amd(s);
    let m = (0.5 * r) * (3.0 - (s * r) * r);
    [m * v[0], m * v[1], m * v[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spec example; holds on every target since rsqrtss is the table model.
    #[test]
    fn facing_example_matches_spec() {
        let n = vec3_normalize([0.0, 500.0, 460.0]);
        assert_eq!(n[0], 0.0);
        assert_eq!([n[1].to_bits(), n[2].to_bits()], [0x3F3C_65F9, 0x3F2D_5393]);
    }

    #[test]
    fn tiny_vector_is_zero() {
        assert_eq!(vec3_normalize([1e-24, 0.0, 0.0]), [0.0; 3]);
        assert_eq!(vec3_normalize([0.0; 3]), [0.0; 3]);
    }
}
