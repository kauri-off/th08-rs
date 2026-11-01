//! RNG `g_Rng` 0x164D520 (11 §1.1–§1.3).

// 11 §1.1: 16-bit state, rotate-left-2 of (s ^ 0x9630) − 0x6553; a draw counter that never affects values.
#[derive(Clone, Debug, Default)]
pub struct Rng {
    // +0 state `s`; Rng_GetState/SetState 0x453C20/0x453BE0.
    pub state: u16,
    // +4 Next16 call counter; reset every frame by the replay hook (11 §1.4).
    pub counter: u32,
    // Port-only: every Next16 since seeding, never reset; the traces' `rng_ctr` column (11 §1.4).
    pub total: u32,
}

impl Rng {
    pub fn seeded(state: u16) -> Self {
        Self {
            state,
            counter: 0,
            total: 0,
        }
    }

    // Boot seed (11 §1.3, 0x4464C8): (u16) timeGetTime() = the host's `boot_clock_ms`; `TH08_SEED` fixes it for
    // reproducible runs.
    pub fn boot(seed: Option<u16>, clock_ms: u32) -> Self {
        Self::seeded(seed.unwrap_or(clock_ms as u16))
    }

    // Rng_Next16 0x43ECC0 (11 §1.1).
    pub fn next_u16(&mut self) -> u16 {
        #[cfg(not(target_arch = "wasm32"))]
        if TRACE.get() {
            let bt = std::backtrace::Backtrace::force_capture().to_string();
            CALLS.with_borrow_mut(|c| c.push(bt));
        }
        let t = (self.state ^ 0x9630).wrapping_sub(0x6553);
        self.state = t.rotate_left(2);
        self.counter = self.counter.wrapping_add(1);
        self.total = self.total.wrapping_add(1);
        self.state
    }

    // Rng_Next32 0x43ED20: the first Next16 is the high half.
    pub fn next_u32(&mut self) -> u32 {
        let hi = self.next_u16() as u32;
        let lo = self.next_u16() as u32;
        hi << 16 | lo
    }

    // Rng_Float 0x43ED50; single-precision division (11 §1.2, x87 CW 0x007F): values ≥ 0xFFFFFF80 round to 1.0.
    pub fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / 4294967296.0
    }

    // Rng_Range 0x406EF0 (11 §1.1): unsigned modulus; n == 0 returns 0 without drawing.
    pub fn range(&mut self, n: i32) -> i32 {
        if n == 0 {
            0
        } else {
            (self.next_u32() % n as u32) as i32
        }
    }

    // Rng_Range16 0x410C00: n == 0 returns 0 without drawing.
    pub fn range16(&mut self, n: u16) -> u16 {
        if n == 0 { 0 } else { self.next_u16() % n }
    }

    // Rng_Signed 0x43ED80 (11 §1.1): Next32 / 2^31 − 1, in [−1, 1].
    pub fn next_signed(&mut self) -> f32 {
        self.next_u32() as f32 / 2147483648.0 - 1.0
    }
}

// The savers' random bytes are Rng_Range16(256) draws (15 §2.1, §3.6).
impl th08_formats::ByteSource for Rng {
    fn next_byte(&mut self) -> u8 {
        self.range16(256) as u8
    }
}

// Debug: record a backtrace per draw while enabled (demo sync investigation).
// TODO(step 10): a thread-local debug hook in core; moves to the tools with the crate cut.
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    pub static TRACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub static CALLS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence() {
        let mut r = Rng::seeded(0);
        let t = 0x9630u16.wrapping_sub(0x6553);
        assert_eq!(r.next_u16(), t.rotate_left(2));
        let mut a = Rng::seeded(1234);
        let mut b = Rng::seeded(1234);
        let hi = b.next_u16() as u32;
        let lo = b.next_u16() as u32;
        assert_eq!(a.next_u32(), hi << 16 | lo);
        assert_eq!(a.counter, 2);
        assert_eq!(Rng::seeded(5).range(0), 0);
        assert!((0..1000).all(|_| a.next_f32() <= 1.0 && a.next_signed() >= -1.0));
    }
}
