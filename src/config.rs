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
        // SPEC-GAP: defaults beyond the option "reset" values and SE volume 80 (colour mode, window, effect level, BGM volume) are guesses.
        Self { lives: 2, bombs: 3, colour_mode: 1, music: 1, se_on: 1, difficulty: 1, windowed: 1, effect_level: 2, slow: 0, bgm_volume: 100, se_volume: 80 }
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
        assert_eq!(super::volume_curve(-1100, 80), (3900.0f32 * 0.992) as i32 - 5000);
    }
}
