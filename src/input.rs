use winit::keyboard::KeyCode;

// Game input word layout (09 §3).
pub const SHOT: u16 = 1 << 0;
pub const BOMB: u16 = 1 << 1;
pub const FOCUS: u16 = 1 << 2;
pub const PAUSE: u16 = 1 << 3;
pub const UP: u16 = 1 << 4;
pub const DOWN: u16 = 1 << 5;
pub const LEFT: u16 = 1 << 6;
pub const RIGHT: u16 = 1 << 7;
pub const SKIP: u16 = 1 << 8;
pub const ENTER: u16 = 1 << 12;
// Keyboard map 0x43D970 (answers_phase5): Q 0x200, S 0x400, Home/P 0x800, D 0x2000, R 0x4000.
pub const KEY_Q: u16 = 1 << 9;
pub const KEY_S: u16 = 1 << 10;
pub const KEY_HOME: u16 = 1 << 11;
pub const KEY_D: u16 = 1 << 13;
pub const KEY_R: u16 = 1 << 14;

pub fn map_key(k: KeyCode) -> u16 {
    match k {
        KeyCode::ArrowUp => UP,
        KeyCode::ArrowDown => DOWN,
        KeyCode::ArrowLeft => LEFT,
        KeyCode::ArrowRight => RIGHT,
        KeyCode::KeyZ => SHOT,
        KeyCode::KeyX => BOMB,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => FOCUS,
        KeyCode::Escape => PAUSE,
        KeyCode::ControlLeft | KeyCode::ControlRight => SKIP,
        KeyCode::Enter | KeyCode::NumpadEnter => ENTER,
        KeyCode::KeyQ => KEY_Q,
        KeyCode::KeyS => KEY_S,
        KeyCode::Home | KeyCode::KeyP => KEY_HOME,
        KeyCode::KeyD => KEY_D,
        KeyCode::KeyR => KEY_R,
        _ => 0,
    }
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub raw: u16,
    pub held: u16,
    pub prev: u16,
    pub same_frames: u32,
    pub repeat: bool,
}

impl Input {
    pub fn key(&mut self, k: KeyCode, down: bool) {
        let b = map_key(k);
        if down {
            self.raw |= b;
        } else {
            self.raw &= !b;
        }
    }

    pub fn tick(&mut self) {
        self.prev = self.held;
        self.held = self.raw;
        // Key repeat (12 header): after 30 frames of unchanged input, true on every 8th frame.
        self.same_frames = if self.held == self.prev {
            self.same_frames + 1
        } else {
            0
        };
        self.repeat = self.same_frames >= 30 && self.same_frames % 8 == 0;
    }

    pub fn down(&self, b: u16) -> bool {
        self.held & b != 0
    }

    pub fn pressed(&self, b: u16) -> bool {
        self.held & b != 0 && self.prev & b == 0
    }

    pub fn pressed_or_repeat(&self, b: u16) -> bool {
        self.pressed(b) || self.repeat && self.down(b)
    }
}

// Debug input script (`TH08_INPUT`): items separated by newlines/commas/spaces, each `mask` or `frames:mask`
// (decimal or 0x hex, game bit layout); `#` starts a comment. The script then holds 0.
pub fn parse_script(s: &str) -> Option<Vec<u16>> {
    let num = |t: &str| {
        t.strip_prefix("0x")
            .map_or_else(|| t.parse().ok(), |h| u16::from_str_radix(h, 16).ok())
    };
    let mut out = Vec::new();
    for line in s.lines() {
        let line = line.split('#').next().unwrap_or("");
        for item in line.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
            let (n, m) = match item.split_once(':') {
                Some((n, m)) => (n.parse::<usize>().ok()?, num(m)?),
                None => (1, num(item)?),
            };
            out.extend(std::iter::repeat_n(m, n));
        }
    }
    Some(out)
}

// The value is a file path natively; anything containing ':' or ',' that is not a file is taken inline.
pub fn load_script(v: &str) -> Option<Vec<u16>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(s) = std::fs::read_to_string(v) {
        return parse_script(&s);
    }
    parse_script(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn script() {
        assert_eq!(
            super::parse_script("2:0x11, 1 # c\n3:4").unwrap(),
            vec![0x11, 0x11, 1, 4, 4, 4]
        );
        assert!(super::parse_script("x").is_none());
    }
}
