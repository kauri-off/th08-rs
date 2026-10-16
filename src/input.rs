use macroquad::prelude::*;

pub const UP: u16 = 1 << 0;
pub const DOWN: u16 = 1 << 1;
pub const LEFT: u16 = 1 << 2;
pub const RIGHT: u16 = 1 << 3;
pub const SHOT: u16 = 1 << 4;
pub const BOMB: u16 = 1 << 5;
pub const FOCUS: u16 = 1 << 6;
pub const PAUSE: u16 = 1 << 7;
pub const SKIP: u16 = 1 << 8;

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub held: u16,
    pub prev: u16,
}

impl Input {
    pub fn poll(&mut self) {
        let map = [
            (KeyCode::Up, UP),
            (KeyCode::Down, DOWN),
            (KeyCode::Left, LEFT),
            (KeyCode::Right, RIGHT),
            (KeyCode::Z, SHOT),
            (KeyCode::X, BOMB),
            (KeyCode::LeftShift, FOCUS),
            (KeyCode::Escape, PAUSE),
            (KeyCode::LeftControl, SKIP),
        ];
        self.prev = self.held;
        self.held = map.iter().filter(|(k, _)| is_key_down(*k)).fold(0, |a, (_, b)| a | b);
    }

    pub fn down(&self, b: u16) -> bool {
        self.held & b != 0
    }

    pub fn pressed(&self, b: u16) -> bool {
        self.held & b != 0 && self.prev & b == 0
    }
}
