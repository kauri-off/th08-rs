// SPEC-GAP: the game RNG algorithm (g_Rng) is a later-phase spec; xorshift32 placeholder behind this one type.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u32,
}

impl Default for Rng {
    fn default() -> Self {
        Self { state: 0x1234_5678 }
    }
}

impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f64 / 4294967296.0) as f32
    }
}
