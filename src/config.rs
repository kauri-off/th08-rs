//! th08.cfg (15 §1): the byte options of the 60-byte config at Supervisor+0x118 (0x17CE870), `Config_Load` 0x4477B1.

// SPEC-GAP: pad buttons, stick thresholds, frame skip +0x23, ShotSlow +0x26 and debug flags +0x38 are not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
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
    // +0x24 effect level 0 none / 1 half / 2 all (13 §2).
    pub effect_level: u8,
    // +0x25 slow mode.
    pub slow: u8,
    // +0x27 / +0x28, 0..100 (13 §1.3 volume curve).
    pub bgm_volume: u8,
    pub se_volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        // 15 §1.1 defaults (music 1 = WAV; 2 when thbgm.dat is invalid).
        // SPEC-GAP: the thbgm.dat header probe that picks 1 or 2 is not made.
        Self {
            lives: 2,
            bombs: 3,
            colour_mode: 0,
            music: 1,
            se_on: 1,
            difficulty: 1,
            windowed: 0,
            effect_level: 2,
            slow: 0,
            bgm_volume: 100,
            se_volume: 80,
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

impl Config {
    // th08.cfg image (15 §1.1), as copied into replays (info +0xB4) and HSCR +0x28.
    // Fields the port does not keep get their 15 §1.1 defaults (pad table 0x4C78C8, thresholds 600, version 0x80001).
    pub fn to_bytes(&self) -> [u8; 60] {
        let mut b = [0u8; 60];
        for (i, v) in [0i16, 1, 2, 4, -1, -1, -1, -1, 3].iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        b[0x14..0x18].copy_from_slice(&0x80001u32.to_le_bytes());
        b[0x18..0x1A].copy_from_slice(&600i16.to_le_bytes());
        b[0x1A..0x1C].copy_from_slice(&600i16.to_le_bytes());
        b[0x1C] = self.lives;
        b[0x1D] = self.bombs;
        b[0x1E] = self.colour_mode;
        b[0x1F] = self.music;
        b[0x20] = self.se_on;
        b[0x21] = self.difficulty;
        b[0x22] = self.windowed;
        b[0x24] = self.effect_level;
        b[0x25] = self.slow;
        b[0x27] = self.bgm_volume;
        b[0x28] = self.se_volume;
        // +0x38: bit 0 is always forced to 1 (15 §1.1); the real file has 0x4001.
        b[0x38] = 1;
        b
    }

    // Config_Load (15 §1.2): None when the size, version or any byte field is invalid (→ defaults).
    // SPEC-GAP: the write-back after load and at exit (15 §1.2) is not done.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != 0x3C || u32::from_le_bytes(b[0x14..0x18].try_into().ok()?) != 0x80001 {
            return None;
        }
        // Valid ranges of bytes +0x1C..+0x26 (15 §1.1 "valid" column).
        let limits = [7, 4, 2, 3, 2, 6, 2, 3, 3, 2, 2];
        if (0..11).any(|i| b[0x1C + i] >= limits[i]) {
            return None;
        }
        Some(Self {
            lives: b[0x1C],
            bombs: b[0x1D],
            colour_mode: b[0x1E],
            music: b[0x1F],
            se_on: b[0x20],
            difficulty: b[0x21],
            windowed: b[0x22],
            effect_level: b[0x24],
            slow: b[0x25],
            bgm_volume: b[0x27],
            se_volume: b[0x28],
        })
    }

    // The gameplay copy during playback takes the replay's recorded config (11 §3.4, info +0xB4).
    // SPEC-GAP: only lives / effect level / slow are copied, and the effect level is clamped to 2.
    pub fn apply_gameplay(&mut self, b: &[u8]) {
        if b.len() >= 0x26 {
            self.lives = b[0x1C];
            self.effect_level = b[0x24].min(2);
            self.slow = b[0x25];
        }
    }
}

// Volume curve (13 §1.3): k = 1 − (1 − v/100)³, result = trunc((table + 5000)·k) − 5000 (1/100 dB); v = 0 → −10000.
pub fn volume_curve(table: i32, v: u8) -> i32 {
    if v == 0 {
        return -10000;
    }
    let k = 1.0 - (1.0 - v.min(100) as f32 / 100.0).powi(3);
    ((table + 5000) as f32 * k) as i32 - 5000
}

#[cfg(test)]
mod tests {
    #[test]
    fn curve() {
        assert_eq!(super::volume_curve(-1100, 100), -1100);
        assert_eq!(super::volume_curve(-1100, 0), -10000);
        assert_eq!(
            super::volume_curve(-1100, 80),
            (3900.0f32 * 0.992) as i32 - 5000
        );
    }
}
