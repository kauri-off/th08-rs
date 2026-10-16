use winit::keyboard::KeyCode;

pub const UP: u16 = 1 << 0;
pub const DOWN: u16 = 1 << 1;
pub const LEFT: u16 = 1 << 2;
pub const RIGHT: u16 = 1 << 3;
pub const SHOT: u16 = 1 << 4;
pub const BOMB: u16 = 1 << 5;
pub const FOCUS: u16 = 1 << 6;
pub const PAUSE: u16 = 1 << 7;
pub const SKIP: u16 = 1 << 8;

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
        _ => 0,
    }
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub raw: u16,
    pub held: u16,
    pub prev: u16,
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
    }

    pub fn down(&self, b: u16) -> bool {
        self.held & b != 0
    }

    pub fn pressed(&self, b: u16) -> bool {
        self.held & b != 0 && self.prev & b == 0
    }
}
