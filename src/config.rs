//! th08.cfg (15 §1): the 60-byte config at Supervisor+0x118 (0x17CE870), `Config_Load` 0x4477B1.

// Pad buttons +0x00…+0x10 in file order (Shot, Bomb, Focus, Pause, Up, Down, Left, Right, Skip): DirectInput button
// indices, −1 = none. Defaults = the key-config reset table 0x4C78C8 (15 §1.3).
pub const PAD_DEFAULT: [i16; 9] = [0, 1, 2, 4, -1, -1, -1, -1, 3];
// +0x14, the only accepted version (15 §1.2).
pub const VERSION: u32 = 0x80001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub pad: [i16; 9],
    // +0x18 / +0x1A DirectInput stick X / Y thresholds (15 §1.1 joystick mapping).
    pub stick: [i16; 2],
    // +0x1C starting-lives option (< 7).
    pub lives: u8,
    // +0x1D, unused by the game (reset to 3 by the option "Reset", 15 §1.1).
    pub bombs: u8,
    // +0x1E force 16-bit colour.
    pub colour_mode: u8,
    // +0x1F music 0 off / 1 WAV / 2 MIDI.
    pub music: u8,
    // +0x20 SE on (13 §1.3).
    pub se_on: u8,
    // +0x21 last chosen difficulty.
    pub difficulty: u8,
    // +0x22.
    pub windowed: u8,
    // +0x23 frame skip: the draw chain runs when the per-frame counter reaches it (0 and 1 = every frame, 00 §2).
    pub frame_skip: u8,
    // +0x24 effect level 0 none / 1 half / 2 all (13 §2).
    pub effect_level: u8,
    // +0x25 slow mode.
    pub slow: u8,
    // +0x26 joystick "shot also focuses" (ShotSlow row of the key config, 15 §1.3).
    pub shot_slow: u8,
    // +0x27 / +0x28, 0..100 (13 §1.3 volume curve).
    pub bgm_volume: u8,
    pub se_volume: u8,
    // +0x38 debug/driver flags (15 §1.1 table); bit 0 is always forced to 1.
    pub debug_flags: u32,
    // Padding +0x12..+0x14 and unused +0x29..+0x38: never read, kept so the file is written back as loaded.
    pub padding: [u8; 2],
    pub unused: [u8; 15],
}

impl Default for Config {
    fn default() -> Self {
        // 15 §1.1 defaults (music 1 = WAV; 2 when thbgm.dat is missing, `apply_bgm_probe`).
        Self {
            pad: PAD_DEFAULT,
            stick: [600, 600],
            lives: 2,
            bombs: 3,
            colour_mode: 0,
            music: 1,
            se_on: 1,
            difficulty: 1,
            windowed: 0,
            frame_skip: 0,
            effect_level: 2,
            slow: 0,
            shot_slow: 0,
            bgm_volume: 100,
            se_volume: 80,
            debug_flags: 1,
            padding: [0; 2],
            unused: [0; 15],
        }
    }
}

impl Config {
    // Option item 7 "Reset" (15 §1.3): lives 2, +0x1D 3, music 1, SE 1, slow 0.
    pub fn reset(&mut self) {
        self.lives = 2;
        self.bombs = 3;
        self.music = 1;
        self.se_on = 1;
        self.slow = 0;
    }
}

// thbgm.dat header probe made by `Config_Load` (02 §1): "ZWAV", version 1, 0x800.
pub fn bgm_header_valid(h: &[u8]) -> bool {
    let word = |i: usize| {
        h.get(4 * i..4 * i + 4)
            .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
    };
    word(0) == Some(0x5641575A) && word(1) == Some(1) && word(2) == Some(0x800)
}

impl Config {
    // The 60-byte th08.cfg image (15 §1.1), as written to disk and copied into replays (info +0xB4) and HSCR +0x28.
    pub fn to_bytes(&self) -> [u8; 60] {
        let mut b = [0u8; 60];
        for (i, v) in self.pad.iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        b[0x12..0x14].copy_from_slice(&self.padding);
        b[0x14..0x18].copy_from_slice(&VERSION.to_le_bytes());
        b[0x18..0x1A].copy_from_slice(&self.stick[0].to_le_bytes());
        b[0x1A..0x1C].copy_from_slice(&self.stick[1].to_le_bytes());
        b[0x1C] = self.lives;
        b[0x1D] = self.bombs;
        b[0x1E] = self.colour_mode;
        b[0x1F] = self.music;
        b[0x20] = self.se_on;
        b[0x21] = self.difficulty;
        b[0x22] = self.windowed;
        b[0x23] = self.frame_skip;
        b[0x24] = self.effect_level;
        b[0x25] = self.slow;
        b[0x26] = self.shot_slow;
        b[0x27] = self.bgm_volume;
        b[0x28] = self.se_volume;
        b[0x29..0x38].copy_from_slice(&self.unused);
        b[0x38..0x3C].copy_from_slice(&self.debug_flags.to_le_bytes());
        b
    }

    // Config_Load (15 §1.2): None when the size, version or any byte field is invalid (→ defaults).
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != 0x3C || u32::from_le_bytes(b[0x14..0x18].try_into().ok()?) != VERSION {
            return None;
        }
        // Valid ranges of bytes +0x1C..+0x26 (15 §1.1 "valid" column).
        let limits = [7, 4, 2, 3, 2, 6, 2, 3, 3, 2, 2];
        if (0..11).any(|i| b[0x1C + i] >= limits[i]) {
            return None;
        }
        let i16_at = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        Some(Self {
            pad: std::array::from_fn(|i| i16_at(2 * i)),
            stick: [i16_at(0x18), i16_at(0x1A)],
            lives: b[0x1C],
            bombs: b[0x1D],
            colour_mode: b[0x1E],
            music: b[0x1F],
            se_on: b[0x20],
            difficulty: b[0x21],
            windowed: b[0x22],
            frame_skip: b[0x23],
            effect_level: b[0x24],
            slow: b[0x25],
            shot_slow: b[0x26],
            bgm_volume: b[0x27],
            se_volume: b[0x28],
            debug_flags: u32::from_le_bytes(b[0x38..0x3C].try_into().ok()?) | 1,
            padding: [b[0x12], b[0x13]],
            unused: b[0x29..0x38].try_into().ok()?,
        })
    }

    // Config_Load (15 §1.2, 02 §1): the file (None = missing) and the thbgm.dat header (None = missing) give the
    // config; Err when thbgm.dat exists with a bad header (startup fails).
    pub fn load(file: Option<&[u8]>, bgm_header: Option<&[u8]>) -> anyhow::Result<Self> {
        if bgm_header.is_some_and(|h| !bgm_header_valid(h)) {
            anyhow::bail!("thbgm.dat: bad header");
        }
        if let Some(c) = file.and_then(Self::from_bytes) {
            return Ok(c);
        }
        log::warn!("th08.cfg: コンフィグデータが異常; using defaults");
        let mut c = Self::default();
        // A missing thbgm.dat only matters when the defaults are applied (02 §1); a valid cfg keeps its mode.
        // SPEC-GAP: the message box 0x437CDD shown here has no text in the spec; the port logs a warning.
        if bgm_header.is_none() {
            log::warn!("thbgm.dat missing; music mode 2 (MIDI)");
            c.music = 2;
        }
        Ok(c)
    }
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
    let k = 1.0 - (1.0 - v.min(100) as f32 / 100.0).powi(exp);
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

    // 15 §1.1/§1.2: every field survives a load → save round trip, bit 0 of +0x38 is forced.
    #[test]
    fn round_trip() {
        let mut b = Config::default().to_bytes();
        for (i, v) in [3i16, -1, 7, 0, 5, 6, -1, 9, 2].iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        b[0x12] = 0xAB;
        b[0x18..0x1C].copy_from_slice(&[0x10, 0x02, 0x20, 0x03]);
        b[0x1C..0x29].copy_from_slice(&[6, 0, 1, 2, 0, 5, 1, 2, 1, 1, 1, 37, 99]);
        b[0x30] = 0x5A;
        b[0x38..0x3C].copy_from_slice(&0x4001u32.to_le_bytes());
        let c = Config::from_bytes(&b).unwrap();
        assert_eq!(c.pad, [3, -1, 7, 0, 5, 6, -1, 9, 2]);
        assert_eq!((c.stick, c.frame_skip, c.shot_slow), ([0x210, 0x320], 2, 1));
        assert_eq!(c.debug_flags, 0x4001);
        assert_eq!(c.to_bytes(), b);
        b[0x38] = 0;
        assert_eq!(Config::from_bytes(&b).unwrap().to_bytes()[0x38], 1);
        assert_eq!(
            Config::from_bytes(&Config::default().to_bytes()),
            Some(Config::default())
        );
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
        assert_eq!(Config::load(Some(&c), None).unwrap(), Config::default());
        assert_eq!(Config::load(Some(&bad), None).unwrap().music, 2);
        assert_eq!(Config::load(None, Some(&hdr)).unwrap(), Config::default());
        let mut h2 = hdr.clone();
        h2[4] = 2;
        assert!(Config::load(Some(&good), Some(&h2)).is_err());
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
