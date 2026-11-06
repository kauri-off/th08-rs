//! Opt-in count of NaNs made in logic (feature `nan-probe`): fresh ones per site and NaN stores into VM variables.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    Sin,
    Cos,
    Polar,
    SinMul,
    CosMul,
    Atan2,
    Tan,
    Acos,
    Atan,
    AnmArith,
    EclArith,
    EclDelta,
    EclDist,
    EclLerp,
    AnmStore,
    EclStore,
}

pub const SITES: [Site; 16] = [
    Site::Sin,
    Site::Cos,
    Site::Polar,
    Site::SinMul,
    Site::CosMul,
    Site::Atan2,
    Site::Tan,
    Site::Acos,
    Site::Atan,
    Site::AnmArith,
    Site::EclArith,
    Site::EclDelta,
    Site::EclDist,
    Site::EclLerp,
    Site::AnmStore,
    Site::EclStore,
];

#[cfg(feature = "nan-probe")]
#[expect(
    clippy::disallowed_types,
    reason = "debug counter, never read by logic"
)]
static COUNTS: [std::sync::atomic::AtomicU64; SITES.len()] =
    [const { std::sync::atomic::AtomicU64::new(0) }; SITES.len()];

/// Counts `out` at `site` when it is NaN and no input was (`fresh`); a no-op without the feature.
#[inline(always)]
pub fn fresh(site: Site, out: f32, inputs_nan: bool) {
    #[cfg(feature = "nan-probe")]
    if out.is_nan() && !inputs_nan {
        COUNTS[site as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    #[cfg(not(feature = "nan-probe"))]
    let _ = (site, out, inputs_nan);
}

/// Counts any NaN stored at `site`, fresh or passed through.
#[inline(always)]
pub fn store(site: Site, v: f32) {
    fresh(site, v, false);
}

/// Counts per site; all 0 without the feature.
pub fn counts() -> [(Site, u64); SITES.len()] {
    SITES.map(|s| {
        #[cfg(feature = "nan-probe")]
        let n = COUNTS[s as usize].load(std::sync::atomic::Ordering::Relaxed);
        #[cfg(not(feature = "nan-probe"))]
        let n = 0;
        (s, n)
    })
}

pub const ENABLED: bool = cfg!(feature = "nan-probe");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_in_index_order() {
        for (i, s) in SITES.iter().enumerate() {
            assert_eq!(*s as usize, i);
        }
    }

    #[cfg(feature = "nan-probe")]
    #[test]
    fn counts_fresh_nan_only() {
        let n = |s: Site| counts()[s as usize].1;
        let before = n(Site::Acos);
        assert!(crate::fmath::acos_f(2.0).is_nan());
        assert!(n(Site::Acos) > before);
        let before = n(Site::EclLerp);
        fresh(Site::EclLerp, f32::NAN, true);
        fresh(Site::EclLerp, 1.0, false);
        assert_eq!(n(Site::EclLerp), before);
    }
}
