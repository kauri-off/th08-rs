//! Keyboard remapping: one settings row per game input over the Input_Poll DIK default table (11 §7, 0x43D970).

use th08_core::input::*;
use winit::keyboard::KeyCode;

// The keys a row may hold, by W3C `code` name; port-only keys (F1–F12, Alt, the system keys) are not bindable.
macro_rules! codes {
    ($($k:ident),* $(,)?) => {
        const CODES: &[(KeyCode, &str)] = &[$((KeyCode::$k, stringify!($k))),*];
    };
}

codes!(
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,
    CapsLock,
);

// Most keys one row holds.
pub const MAX_PER_INPUT: usize = 4;

// One rebindable game input: its settings row, its word bits, the original keys and the WASD + J / K / L preset.
pub struct Input {
    pub key: &'static str,
    pub bit: u16,
    pub default: &'static str,
    pub wasd: &'static str,
}

pub const INPUTS: [Input; 12] = [
    Input {
        key: "key_up",
        bit: UP,
        default: "ArrowUp,Numpad8",
        wasd: "KeyW,ArrowUp,Numpad8",
    },
    Input {
        key: "key_down",
        bit: DOWN,
        default: "ArrowDown,Numpad2",
        wasd: "KeyS,ArrowDown,Numpad2",
    },
    Input {
        key: "key_left",
        bit: LEFT,
        default: "ArrowLeft,Numpad4",
        wasd: "KeyA,ArrowLeft,Numpad4",
    },
    Input {
        key: "key_right",
        bit: RIGHT,
        default: "ArrowRight,Numpad6",
        wasd: "KeyD,ArrowRight,Numpad6",
    },
    Input {
        key: "key_shot",
        bit: SHOT,
        default: "KeyZ",
        wasd: "KeyJ,KeyZ",
    },
    Input {
        key: "key_bomb",
        bit: BOMB,
        default: "KeyX",
        wasd: "KeyK,KeyX",
    },
    Input {
        key: "key_focus",
        bit: FOCUS,
        default: "ShiftLeft,ShiftRight",
        wasd: "KeyL,ShiftLeft,ShiftRight",
    },
    Input {
        key: "key_pause",
        bit: PAUSE,
        default: "Escape",
        wasd: "Escape",
    },
    Input {
        key: "key_skip",
        bit: SKIP,
        default: "ControlLeft,ControlRight",
        wasd: "ControlLeft,ControlRight",
    },
    Input {
        key: "key_enter",
        bit: ENTER,
        default: "Enter",
        wasd: "Enter",
    },
    Input {
        key: "key_quit",
        bit: KEY_Q,
        default: "KeyQ",
        wasd: "KeyQ",
    },
    Input {
        key: "key_retry",
        bit: KEY_R,
        default: "KeyR",
        wasd: "KeyR",
    },
];

// The rest of the DIK table: keypad diagonals and the keys no game screen reads (S, D, Home / P). A key bound in any
// row drops its fixed bits, so WASD never also sends S or D.
const FIXED: [(KeyCode, u16); 8] = [
    (KeyCode::Numpad7, UP | LEFT),
    (KeyCode::Numpad9, UP | RIGHT),
    (KeyCode::Numpad1, DOWN | LEFT),
    (KeyCode::Numpad3, DOWN | RIGHT),
    (KeyCode::KeyS, KEY_S),
    (KeyCode::Home, KEY_HOME),
    (KeyCode::KeyP, KEY_HOME),
    (KeyCode::KeyD, KEY_D),
];

pub fn code(name: &str) -> Option<KeyCode> {
    CODES.iter().find(|c| c.1 == name).map(|c| c.0)
}

pub fn name(k: KeyCode) -> Option<&'static str> {
    CODES.iter().find(|c| c.0 == k).map(|c| c.1)
}

// Short label on settings screens: `KeyZ` → `Z`, `ArrowUp` → `Up`, `Numpad8` → `Num8`.
pub fn label(name: &str) -> String {
    for (p, r) in [("Key", ""), ("Digit", ""), ("Arrow", ""), ("Numpad", "Num")] {
        if let Some(rest) = name.strip_prefix(p) {
            return format!("{r}{rest}");
        }
    }
    match name {
        "ShiftLeft" => "LShift".into(),
        "ShiftRight" => "RShift".into(),
        "ControlLeft" => "LCtrl".into(),
        "ControlRight" => "RCtrl".into(),
        "Escape" => "Esc".into(),
        n => n.into(),
    }
}

// A row value: `none` or up to `MAX_PER_INPUT` distinct key names, comma-separated.
pub fn parse(v: &str) -> Result<Vec<KeyCode>, String> {
    let v = v.trim();
    if v.is_empty() || v == "none" {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for n in v.split(',').map(str::trim) {
        let k = code(n)
            .ok_or_else(|| format!("{n}: not a bindable key name (KeyZ, ArrowUp, ShiftLeft…)"))?;
        if out.contains(&k) {
            return Err(format!("{n} is listed twice"));
        }
        out.push(k);
    }
    if out.len() > MAX_PER_INPUT {
        return Err(format!("at most {MAX_PER_INPUT} keys"));
    }
    Ok(out)
}

pub fn format(keys: &[KeyCode]) -> String {
    if keys.is_empty() {
        return "none".into();
    }
    keys.iter()
        .filter_map(|&k| name(k))
        .collect::<Vec<_>>()
        .join(",")
}

// The keys of every input row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMap {
    pub binds: [Vec<KeyCode>; INPUTS.len()],
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            binds: std::array::from_fn(|i| parse(INPUTS[i].default).expect("default keys")),
        }
    }
}

impl KeyMap {
    pub fn set(&mut self, input: usize, v: &str) -> Result<(), String> {
        self.binds[input] = parse(v)?;
        Ok(())
    }

    // Key → word bits; a key in several rows sends all their bits.
    pub fn table(&self) -> Vec<(KeyCode, u16)> {
        let mut t: Vec<(KeyCode, u16)> = Vec::new();
        for (keys, input) in self.binds.iter().zip(&INPUTS) {
            for &k in keys {
                match t.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1 |= input.bit,
                    None => t.push((k, input.bit)),
                }
            }
        }
        for &(k, bits) in &FIXED {
            if !t.iter().any(|e| e.0 == k) {
                t.push((k, bits));
            }
        }
        t
    }

    // Keys bound in more than one row, with those rows' indices.
    pub fn conflicts(&self) -> Vec<(KeyCode, Vec<usize>)> {
        let mut out: Vec<(KeyCode, Vec<usize>)> = Vec::new();
        for (i, keys) in self.binds.iter().enumerate() {
            for &k in keys {
                match out.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1.push(i),
                    None => out.push((k, vec![i])),
                }
            }
        }
        out.retain(|e| e.1.len() > 1);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Input_Poll 0x43D970 DIK table: keypad 7/9/1/3 give the diagonals; only the main Enter (0x1C) is mapped.
    const ORIGINAL: [(KeyCode, u16); 26] = [
        (KeyCode::ArrowUp, UP),
        (KeyCode::ArrowDown, DOWN),
        (KeyCode::ArrowLeft, LEFT),
        (KeyCode::ArrowRight, RIGHT),
        (KeyCode::Numpad8, UP),
        (KeyCode::Numpad2, DOWN),
        (KeyCode::Numpad4, LEFT),
        (KeyCode::Numpad6, RIGHT),
        (KeyCode::Numpad7, UP | LEFT),
        (KeyCode::Numpad9, UP | RIGHT),
        (KeyCode::Numpad1, DOWN | LEFT),
        (KeyCode::Numpad3, DOWN | RIGHT),
        (KeyCode::KeyZ, SHOT),
        (KeyCode::KeyX, BOMB),
        (KeyCode::ShiftLeft, FOCUS),
        (KeyCode::ShiftRight, FOCUS),
        (KeyCode::Escape, PAUSE),
        (KeyCode::ControlLeft, SKIP),
        (KeyCode::ControlRight, SKIP),
        (KeyCode::KeyQ, KEY_Q),
        (KeyCode::KeyS, KEY_S),
        (KeyCode::KeyR, KEY_R),
        (KeyCode::Home, KEY_HOME),
        (KeyCode::KeyP, KEY_HOME),
        (KeyCode::Enter, ENTER),
        (KeyCode::KeyD, KEY_D),
    ];

    fn sorted(mut t: Vec<(KeyCode, u16)>) -> Vec<(String, u16)> {
        let mut v: Vec<(String, u16)> = t.drain(..).map(|(k, b)| (format!("{k:?}"), b)).collect();
        v.sort();
        v
    }

    #[test]
    fn defaults_equal_original_table() {
        assert_eq!(sorted(KeyMap::default().table()), sorted(ORIGINAL.to_vec()));
        assert!(KeyMap::default().conflicts().is_empty());
        let bits = INPUTS.iter().fold(0, |a, i| a | i.bit);
        assert_eq!(
            INPUTS.iter().map(|i| i.bit.count_ones()).sum::<u32>(),
            bits.count_ones()
        );
    }

    #[test]
    fn parse_and_format() {
        assert_eq!(
            parse("KeyW, ArrowUp").unwrap(),
            [KeyCode::KeyW, KeyCode::ArrowUp]
        );
        assert!(parse("none").unwrap().is_empty());
        assert!(parse("").unwrap().is_empty());
        assert!(parse("KeyW,KeyW").is_err());
        assert!(parse("F12").is_err());
        assert!(parse("AltLeft").is_err());
        assert!(parse("KeyA,KeyB,KeyC,KeyD,KeyE").is_err());
        for i in &INPUTS {
            assert_eq!(format(&parse(i.default).unwrap()), i.default);
            assert_eq!(format(&parse(i.wasd).unwrap()), i.wasd);
        }
        assert_eq!(format(&[]), "none");
        assert_eq!(label("KeyZ"), "Z");
        assert_eq!(label("ArrowUp"), "Up");
        assert_eq!(label("Numpad8"), "Num8");
        assert_eq!(label("ShiftLeft"), "LShift");
    }

    #[test]
    fn wasd_preset_drops_fixed_bits() {
        let mut m = KeyMap::default();
        for (i, input) in INPUTS.iter().enumerate() {
            m.set(i, input.wasd).unwrap();
        }
        let t = m.table();
        let bits = |k| t.iter().find(|e| e.0 == k).map_or(0, |e| e.1);
        assert_eq!(bits(KeyCode::KeyS), DOWN);
        assert_eq!(bits(KeyCode::KeyD), RIGHT);
        assert_eq!(bits(KeyCode::KeyJ), SHOT);
        assert_eq!(bits(KeyCode::KeyL), FOCUS);
        assert_eq!(bits(KeyCode::Numpad7), UP | LEFT);
        assert_eq!(bits(KeyCode::Home), KEY_HOME);
        assert!(m.conflicts().is_empty());
    }

    #[test]
    fn conflicts_listed_and_or_bits() {
        let mut m = KeyMap::default();
        m.set(5, "KeyZ").unwrap();
        assert_eq!(m.conflicts(), [(KeyCode::KeyZ, vec![4, 5])]);
        let t = m.table();
        assert_eq!(
            t.iter().find(|e| e.0 == KeyCode::KeyZ).unwrap().1,
            SHOT | BOMB
        );
        assert!(!t.iter().any(|e| e.0 == KeyCode::KeyX));
    }
}
