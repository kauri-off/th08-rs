//! th08.cfg (15 §1): `Config_Load` 0x4477B1 and the volume curves; the 60-byte image is th08_formats::cfg.

pub use th08_formats::cfg::*;
pub use th08_formats::thbgm::bgm_header_valid;

// Config_Load (15 §1.2, 02 §1): the file (None = missing) and the thbgm.dat header (None = missing) give the
// config; Err when thbgm.dat exists with a bad header (startup fails).
pub fn load(file: Option<&[u8]>, bgm_header: Option<&[u8]>) -> anyhow::Result<Config> {
    if bgm_header.is_some_and(|h| !bgm_header_valid(h)) {
        anyhow::bail!("thbgm.dat: bad header");
    }
    if let Some(c) = file.and_then(Config::from_bytes) {
        return Ok(c);
    }
    log::warn!("th08.cfg: コンフィグデータが異常; using defaults");
    let mut c = Config::default();
    // A missing thbgm.dat only matters when the defaults are applied (02 §1); a valid cfg keeps its mode.
    // The original calls the empty function 0x437CDD here: nothing is shown (15 §1.2); the log line is the port's.
    if bgm_header.is_none() {
        log::warn!("thbgm.dat missing; music mode 2 (MIDI)");
        c.music = 2;
    }
    Ok(c)
}

// SE volume curve (13 §1.3, 0x45DFE7): k = 1 − (1 − v/100)³, result = trunc((table + 5000)·k) − 5000 (1/100 dB);
// v = 0 → −10000.
pub fn volume_curve(table: i32, v: u8) -> i32 {
    curve(table, v, 3)
}

// BGM volume scaling (02 §4 `0x472840`, 13 §1.3): the same with a square, k = 1 − (1 − v/100)²; `p` is the fade
// attenuation (0 when not fading).
pub fn bgm_volume_curve(p: i32, v: u8) -> i32 {
    curve(p, v, 2)
}

fn curve(table: i32, v: u8, exp: i32) -> i32 {
    if v == 0 {
        return -10000;
    }
    let k = 1.0 - (1.0 - f32::from(v.min(100)) / 100.0).powi(exp);
    ((table + 5000) as f32 * k) as i32 - 5000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve() {
        assert_eq!(volume_curve(-1100, 100), -1100);
        assert_eq!(volume_curve(-1100, 0), -10000);
        assert_eq!(volume_curve(-1100, 80), (3900.0f32 * 0.992) as i32 - 5000);
    }

    // 15 §1.2: invalid files fall back to defaults; 02 §1: the thbgm.dat probe.
    #[test]
    fn load_rules() {
        let good = Config::default().to_bytes();
        let hdr = [
            b"ZWAV".as_slice(),
            &1u32.to_le_bytes(),
            &0x800u32.to_le_bytes(),
            &[0; 4],
        ]
        .concat();
        let mut bad = good;
        bad[0x23] = 3;
        assert!(Config::from_bytes(&bad).is_none());
        assert!(Config::from_bytes(&good[..59]).is_none());
        let mut c = good;
        c[0x1F] = 1;
        assert_eq!(load(Some(&c), None).unwrap(), Config::default());
        assert_eq!(load(Some(&bad), None).unwrap().music, 2);
        assert_eq!(load(None, Some(&hdr)).unwrap(), Config::default());
        let mut h2 = hdr.clone();
        h2[4] = 2;
        assert!(load(Some(&good), Some(&h2)).is_err());
    }

    #[test]
    fn se_cube_and_bgm_square() {
        // v = 50: SE k = 0.875, BGM k = 0.75.
        assert_eq!([0, 50, 100].map(|v| volume_curve(0, v)), [-10000, -625, 0]);
        assert_eq!(
            [0, 50, 100].map(|v| bgm_volume_curve(0, v)),
            [-10000, -1250, 0]
        );
    }
}
