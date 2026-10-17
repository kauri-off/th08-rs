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

impl Rng {
    pub fn next_u16(&mut self) -> u16 {
        (self.next_u32() >> 16) as u16
    }

    pub fn range(&mut self, n: i32) -> i32 {
        if n <= 0 { 0 } else { (self.next_u32() % n as u32) as i32 }
    }

    // Next32 / 2^31 − 1, in [−1, 1).
    pub fn next_signed(&mut self) -> f32 {
        (self.next_u32() as f64 / 2147483648.0 - 1.0) as f32
    }
}
