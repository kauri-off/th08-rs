//! Input word and key repeat (12 intro, 11 §7 `Input_Poll` 0x43D970; gameplay word 09 §3, 15 §3.2–§3.3).

use winit::keyboard::KeyCode;

// Input word bits (09 §3, 12 intro); also the replay stream format (15 §3.2).
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
// Keyboard map 0x43D970 (12 intro): Q 0x200, S 0x400, Home/P 0x800, D 0x2000, R 0x4000.
pub const KEY_Q: u16 = 1 << 9;
pub const KEY_S: u16 = 1 << 10;
pub const KEY_HOME: u16 = 1 << 11;
pub const KEY_D: u16 = 1 << 13;
pub const KEY_R: u16 = 1 << 14;

// SPEC-GAP: the specs name only Z, Ctrl, ESC, Enter, Q, S, Home/P, D, R; arrows, X and Shift are assumed.
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

// One joystick poll (DirectInput DIJOYSTATE, 15 §1.1): `buttons` bit n = rgbButtons[n] & 0x80 (32 buttons, the raw
// array `Input_PollJoystickRaw` returns), `axis` = lX / lY.
#[derive(Default, Clone, Copy, Debug)]
pub struct PadState {
    pub buttons: u32,
    pub axis: [i32; 2],
}

impl PadState {
    fn button(&self, n: i16) -> bool {
        (0..32).contains(&n) && self.buttons >> n & 1 != 0
    }

    // First pressed button of the raw array (key config scan, 15 §1.3), −1 when none.
    pub fn first_button(&self) -> i16 {
        if self.buttons == 0 {
            -1
        } else {
            self.buttons.trailing_zeros() as i16
        }
    }
}

// Input_PollJoystick 0x43D120, DirectInput path (15 §1.1): pad fields +0x00…+0x10 in file order give bits 0x1…0x100;
// the stick is lX/lY against the +0x18/+0x1A thresholds. `counter` is the ShotSlow counter 0x164D51C (u16).
pub fn joystick_bits(cfg: &crate::config::Config, pad: &PadState, counter: &mut u16) -> u16 {
    let mut bits = 0;
    for (i, &n) in cfg.pad.iter().enumerate() {
        if pad.button(n) {
            bits |= 1 << i;
        }
    }
    let [x, y] = pad.axis;
    let [tx, ty] = cfg.stick.map(i32::from);
    if x > tx {
        bits |= RIGHT;
    } else if x < -tx {
        bits |= LEFT;
    }
    if y > ty {
        bits |= DOWN;
    } else if y < -ty {
        bits |= UP;
    }
    // +0x26 "shot also focuses" (0x43EF50): focus from the 10th held poll, and one more poll after release.
    if cfg.shot_slow != 0 {
        if pad.button(cfg.pad[0]) {
            if *counter < 20 {
                *counter += 1;
            }
            if *counter > 9 {
                bits |= FOCUS;
            }
        } else if *counter < 11 {
            *counter = 0;
        } else {
            *counter -= 10;
            bits |= FOCUS;
        }
    }
    bits
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    // Keyboard state; `held` / `prev` are the polled raw word 0x164D528 / previous 0x164D530 (12 intro, 11 §7).
    pub raw: u16,
    // Joystick bits of this poll, ORed into the raw word after the keyboard (15 §1.1), and the raw button array.
    pub joy: u16,
    pub pad: PadState,
    // ShotSlow counter 0x164D51C (15 §1.1).
    pub shot_slow: u16,
    pub held: u16,
    pub prev: u16,
    // Unchanged-input counter 0x164D53C and key-repeat flag 0x164D538.
    pub same_frames: u32,
    pub repeat: bool,
    // g_Input 0x164D52C / g_InputPrev 0x164D534: the gameplay word, written by the replay node (15 §3.2, §3.3).
    pub game: u16,
    pub game_prev: u16,
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

    // The joystick half of Input_Poll (15 §1.1), run before `tick`.
    pub fn poll_joystick(&mut self, cfg: &crate::config::Config, pad: PadState) {
        self.pad = pad;
        self.joy = joystick_bits(cfg, &pad, &mut self.shot_slow);
    }

    pub fn tick(&mut self) {
        self.prev = self.held;
        self.held = self.raw | self.joy;
        // Key repeat (11 §7, same rule as 15 §3.3): the counter is tested before it is incremented, and reset to 30
        // after 37, so the flag is set on the 33rd, 41st, … unchanged frame.
        self.repeat = false;
        if self.held == self.prev {
            if self.same_frames > 29 {
                self.repeat = self.same_frames % 8 == 0;
                if self.same_frames > 37 {
                    self.same_frames = 30;
                }
            }
            self.same_frames += 1;
        } else {
            self.same_frames = 0;
        }
    }

    // The view gameplay objects read (player, dialogue, HUD): g_Input as both raw and held.
    pub fn game_view(&self) -> Input {
        Input {
            raw: self.game,
            held: self.game,
            prev: self.game_prev,
            ..*self
        }
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

// Port debug tool, not a port of the original. Debug input script (`TH08_INPUT`): items separated by newlines/commas/spaces, each `mask` or `frames:mask`
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
    use super::*;
    use crate::config::Config;

    fn pad(buttons: u32, x: i32, y: i32) -> PadState {
        PadState {
            buttons,
            axis: [x, y],
        }
    }

    // 15 §1.1: default buttons 0 Shot, 1 Bomb, 2 Focus, 4 Pause, 3 Skip; stick thresholds 600 (strict).
    #[test]
    fn joystick_mapping() {
        let mut cfg = Config::default();
        let mut c = 0;
        let mut j = |cfg: &Config, p| joystick_bits(cfg, &p, &mut c);
        assert_eq!(
            j(&cfg, pad(0b11111, 0, 0)),
            SHOT | BOMB | FOCUS | PAUSE | SKIP
        );
        assert_eq!(j(&cfg, pad(1 << 5, 600, -600)), 0);
        assert_eq!(j(&cfg, pad(0, 601, -601)), RIGHT | UP);
        assert_eq!(j(&cfg, pad(0, -1000, 1000)), LEFT | DOWN);
        cfg.pad = [7, -1, -1, -1, 9, -1, -1, -1, 31];
        cfg.stick = [100, 200];
        assert_eq!(
            j(&cfg, pad(1 << 7 | 1 << 9 | 1 << 31 | 1, 150, 150)),
            SHOT | UP | SKIP | RIGHT
        );
        assert_eq!(pad(0b1100, 0, 0).first_button(), 2);
        assert_eq!(pad(0, 0, 0).first_button(), -1);
    }

    // 15 §1.1 +0x26: focus from the 10th held poll; after release it stays one poll (counter capped at 20).
    #[test]
    fn shot_slow_counter() {
        let mut cfg = Config::default();
        let mut c = 0;
        let focus = |cfg: &Config, c: &mut u16, held| {
            joystick_bits(cfg, &pad(held as u32, 0, 0), c) & FOCUS != 0
        };
        assert!((0..30).all(|_| !focus(&cfg, &mut c, true)));
        assert_eq!(c, 0);
        cfg.shot_slow = 1;
        let held: Vec<bool> = (0..25).map(|_| focus(&cfg, &mut c, true)).collect();
        assert_eq!(held.iter().position(|&f| f), Some(9));
        assert_eq!(c, 20);
        assert_eq!(
            [0; 3].map(|_| focus(&cfg, &mut c, false)),
            [true, false, false]
        );
        // A short tap (counter 5 < 11) never focuses.
        for _ in 0..5 {
            focus(&cfg, &mut c, true);
        }
        assert!(!focus(&cfg, &mut c, false));
        assert_eq!(c, 0);
        // Released at 15: 15 − 10 = 5 with focus, then 0.
        for _ in 0..15 {
            focus(&cfg, &mut c, true);
        }
        assert_eq!([0; 2].map(|_| focus(&cfg, &mut c, false)), [true, false]);
        // Keyboard input is unaffected: the joystick bits are ORed in.
        let mut i = Input {
            raw: BOMB,
            ..Default::default()
        };
        i.poll_joystick(&Config::default(), pad(1, 0, 0));
        i.tick();
        assert_eq!(i.held, SHOT | BOMB);
    }
    #[test]
    fn script() {
        assert_eq!(
            super::parse_script("2:0x11, 1 # c\n3:4").unwrap(),
            vec![0x11, 0x11, 1, 4, 4, 4]
        );
        assert!(super::parse_script("x").is_none());
    }

    // 11 §7: holding a key, the repeat flag first fires on the 33rd unchanged frame, then on the 41st.
    #[test]
    fn key_repeat() {
        let mut i = super::Input {
            raw: 1,
            ..Default::default()
        };
        i.tick();
        assert!(!i.repeat);
        let fired: Vec<u32> = (1..=50)
            .filter(|_| {
                i.tick();
                i.repeat
            })
            .collect();
        assert_eq!(fired, [33, 41, 49]);
    }
}
