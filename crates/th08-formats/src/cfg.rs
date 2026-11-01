//! th08.cfg image (15 §1): the 60-byte config at Supervisor+0x118 (0x17CE870), read by `Config_Load` 0x4477B1.

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
