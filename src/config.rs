// In-memory th08.cfg fields (12 §2.7, 13 §1.3); offsets are the live config's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub lives: u8,
    pub bombs: u8,
    pub colour_mode: u8,
    pub music: u8,
    pub se_on: u8,
    pub difficulty: u8,
    pub windowed: u8,
    pub effect_level: u8,
    pub slow: u8,
    pub bgm_volume: u8,
    pub se_volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        // 15 §1.1 defaults (music 1 = WAV; 2 when thbgm.dat is invalid).
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
    // Option "reset" (12 §2.7).
    pub fn reset(&mut self) {
        self.lives = 2;
        self.bombs = 3;
        self.music = 1;
        self.se_on = 1;
        self.slow = 0;
    }
}

impl Config {
    // th08.cfg image (15 §1.1), as copied into replays and HSCR.
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
        b[0x38] = 1;
        b
    }

    // Config_Load (15 §1.2): None when the size, version or any byte field is invalid (→ defaults).
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != 0x3C || u32::from_le_bytes(b[0x14..0x18].try_into().ok()?) != 0x80001 {
            return None;
        }
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

    // The gameplay copy during playback takes the replay's recorded config (11 §3.4).
    pub fn apply_gameplay(&mut self, b: &[u8]) {
        if b.len() >= 0x26 {
            self.lives = b[0x1C];
            self.effect_level = b[0x24].min(2);
            self.slow = b[0x25];
        }
    }
}

// Volume curve (13 §1.3): table volume in 1/100 dB → attenuation for a 0–100 volume setting.
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
