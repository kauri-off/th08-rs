//! Gamepad remapping: one settings row per game input over the physical pad buttons; the defaults leave th08.cfg in charge.

use th08_core::input::*;

// Physical pad input bit: 0–15 = the joystick's button order (South, East, West, North, LB, RB, LT, RT, Select,
// Start, L3, R3, D-pad up / down / left / right), 16–21 = axes used as buttons.
pub const DPAD_UP: usize = 12;
pub const DPAD_DOWN: usize = 13;
pub const DPAD_LEFT: usize = 14;
pub const DPAD_RIGHT: usize = 15;
pub const LEFT_Z: usize = 16;
pub const RIGHT_Z: usize = 17;
pub const RSTICK_UP: usize = 18;
pub const RSTICK_DOWN: usize = 19;
pub const RSTICK_LEFT: usize = 20;
pub const RSTICK_RIGHT: usize = 21;

// The buttons a row may hold: (physical bit, row value name, short label). Select (port menu) and the d-pad (fixed
// directions) are not bindable.
const BINDABLE: &[(usize, &str, &str)] = &[
    (0, "South", "A"),
    (1, "East", "B"),
    (2, "West", "X"),
    (3, "North", "Y"),
    (4, "LeftTrigger", "LB"),
    (5, "RightTrigger", "RB"),
    (6, "LeftTrigger2", "LT"),
    (7, "RightTrigger2", "RT"),
    (9, "Start", "Start"),
    (10, "LeftThumb", "L3"),
    (11, "RightThumb", "R3"),
    (LEFT_Z, "LeftZ", "LT axis"),
    (RIGHT_Z, "RightZ", "RT axis"),
    (RSTICK_UP, "RightStickUp", "RS ↑"),
    (RSTICK_DOWN, "RightStickDown", "RS ↓"),
    (RSTICK_LEFT, "RightStickLeft", "RS ←"),
    (RSTICK_RIGHT, "RightStickRight", "RS →"),
];

// Most buttons one row holds.
pub const MAX_PER_INPUT: usize = 4;

// One rebindable game input: its settings row, its word bits and the button th08.cfg's defaults give it.
pub struct Input {
    pub key: &'static str,
    pub bit: u16,
    pub default: &'static str,
}

pub const INPUTS: [Input; 8] = [
    Input {
        key: "pad_shot",
        bit: SHOT,
        default: "South",
    },
    Input {
        key: "pad_bomb",
        bit: BOMB,
        default: "East",
    },
    Input {
        key: "pad_focus",
        bit: FOCUS,
        default: "West",
    },
    Input {
        key: "pad_pause",
        bit: PAUSE,
        default: "LeftTrigger",
    },
    Input {
        key: "pad_skip",
        bit: SKIP,
        default: "North",
    },
    Input {
        key: "pad_enter",
        bit: ENTER,
        default: "none",
    },
    Input {
        key: "pad_quit",
        bit: KEY_Q,
        default: "none",
    },
    Input {
        key: "pad_retry",
        bit: KEY_R,
        default: "none",
    },
];

pub fn button(name: &str) -> Option<usize> {
    BINDABLE.iter().find(|b| b.1 == name).map(|b| b.0)
}

pub fn name(bit: usize) -> Option<&'static str> {
    BINDABLE.iter().find(|b| b.0 == bit).map(|b| b.1)
}

pub fn label(bit: usize) -> &'static str {
    BINDABLE.iter().find(|b| b.0 == bit).map_or("?", |b| b.2)
}

// The lowest bindable physical bit set in `mask`.
pub fn first_bindable(mask: u32) -> Option<usize> {
    BINDABLE
        .iter()
        .map(|b| b.0)
        .filter(|&b| mask >> b & 1 != 0)
        .min()
}

// A row value: `none` or up to `MAX_PER_INPUT` distinct button names, comma-separated.
pub fn parse(v: &str) -> Result<Vec<usize>, String> {
    let v = v.trim();
    if v.is_empty() || v == "none" {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for n in v.split(',').map(str::trim) {
        let b = button(n).ok_or_else(|| {
            format!("{n}: not a bindable pad button (South, East, LeftTrigger2, RightZ…)")
        })?;
        if out.contains(&b) {
            return Err(format!("{n} is listed twice"));
        }
        out.push(b);
    }
    if out.len() > MAX_PER_INPUT {
        return Err(format!("at most {MAX_PER_INPUT} buttons"));
    }
    Ok(out)
}

pub fn format(buttons: &[usize]) -> String {
    if buttons.is_empty() {
        return "none".into();
    }
    buttons
        .iter()
        .filter_map(|&b| name(b))
        .collect::<Vec<_>>()
        .join(",")
}

// The buttons of every input row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadMap {
    pub binds: [Vec<usize>; INPUTS.len()],
}

impl Default for PadMap {
    fn default() -> Self {
        Self {
            binds: std::array::from_fn(|i| parse(INPUTS[i].default).expect("default buttons")),
        }
    }
}

impl PadMap {
    pub fn set(&mut self, input: usize, v: &str) -> Result<(), String> {
        self.binds[input] = parse(v)?;
        Ok(())
    }

    // A row differs from the defaults: the port maps the pad and the game sees no pad buttons, so th08.cfg's pad
    // fields (and its in-game key config) cannot map them a second time.
    pub fn custom(&self) -> bool {
        *self != Self::default()
    }

    // Buttons bound in more than one row, with those rows' indices.
    pub fn conflicts(&self) -> Vec<(usize, Vec<usize>)> {
        let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
        for (i, bs) in self.binds.iter().enumerate() {
            for &b in bs {
                match out.iter_mut().find(|e| e.0 == b) {
                    Some(e) => e.1.push(i),
                    None => out.push((b, vec![i])),
                }
            }
        }
        out.retain(|e| e.1.len() > 1);
        out
    }

    // One poll of physical input `phys` and stick `axis` (±1000, y down-positive): the pad state the game maps with
    // th08.cfg, and the word bits the port maps itself (ORed into the input word like a key).
    pub fn apply(&self, phys: u32, axis: [i32; 2]) -> (PadState, u16) {
        let down = |b: usize| phys >> b & 1 != 0;
        // The d-pad stands in for the stick at full deflection on its axis, with or without a custom map.
        let dir = |neg, pos, v| match (down(neg), down(pos)) {
            (true, false) => -1000,
            (false, true) => 1000,
            _ => v,
        };
        let axis = [
            dir(DPAD_LEFT, DPAD_RIGHT, axis[0]),
            dir(DPAD_UP, DPAD_DOWN, axis[1]),
        ];
        if !self.custom() {
            return (
                PadState {
                    buttons: phys & 0xFFFF,
                    axis,
                },
                0,
            );
        }
        let word = self
            .binds
            .iter()
            .zip(&INPUTS)
            .filter(|(bs, _)| bs.iter().any(|&b| down(b)))
            .fold(0, |w, (_, i)| w | i.bit);
        (PadState { buttons: 0, axis }, word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_core::config::Config;

    fn bits(m: &PadMap, phys: u32, axis: [i32; 2]) -> u16 {
        let (pad, word) = m.apply(phys, axis);
        th08_core::input::joystick_bits(&Config::default(), &pad, &mut 0) | word
    }

    // The default rows are th08.cfg's default pad fields: buttons reach the game unchanged, the d-pad also steers.
    #[test]
    fn defaults_equal_th08_cfg() {
        let m = PadMap::default();
        assert!(!m.custom());
        assert!(m.conflicts().is_empty());
        for (bs, input) in m.binds.iter().zip(&INPUTS) {
            for &b in bs {
                assert_eq!(bits(&m, 1 << b, [0, 0]), input.bit, "{}", input.key);
            }
        }
        for phys in (0..22)
            .map(|b| 1u32 << b)
            .chain([0, 0xF, 0x3F_FFFF, 0x1_0101])
        {
            for axis in [[0, 0], [1000, -1000], [-700, 300]] {
                let (pad, word) = m.apply(phys, axis);
                let d = |b: usize| phys >> b & 1 != 0;
                let steer = |neg, pos, v| match (d(neg), d(pos)) {
                    (true, false) => -1000,
                    (false, true) => 1000,
                    _ => v,
                };
                let want = [
                    steer(DPAD_LEFT, DPAD_RIGHT, axis[0]),
                    steer(DPAD_UP, DPAD_DOWN, axis[1]),
                ];
                assert_eq!((pad.buttons, pad.axis, word), (phys & 0xFFFF, want, 0));
            }
        }
    }

    #[test]
    fn parse_and_format() {
        assert_eq!(parse("South, RightZ").unwrap(), [0, RIGHT_Z]);
        assert!(parse("none").unwrap().is_empty());
        assert!(parse(" ").unwrap().is_empty());
        assert!(parse("South,South").is_err());
        assert!(parse("Select").is_err());
        assert!(parse("DPadUp").is_err());
        assert!(parse("Mode").is_err());
        assert!(parse("South,East,West,North,Start").is_err());
        for i in &INPUTS {
            assert_eq!(format(&parse(i.default).unwrap()), i.default);
        }
        for b in BINDABLE {
            assert_eq!(format(&parse(b.1).unwrap()), b.1);
            assert_eq!(label(b.0), b.2);
        }
        assert_eq!(format(&[]), "none");
        assert_eq!(
            first_bindable(1 << 8 | 1 << 12 | 1 << RIGHT_Z),
            Some(RIGHT_Z)
        );
        assert_eq!(first_bindable(1 << 8), None);
    }

    #[test]
    fn conflicts_listed() {
        let mut m = PadMap::default();
        m.set(5, "South").unwrap();
        assert!(m.custom());
        assert_eq!(m.conflicts(), [(0, vec![0, 5])]);
        assert_eq!(bits(&m, 1, [0, 0]), SHOT | ENTER);
        m.set(5, "none").unwrap();
        assert!(!m.custom());
    }

    // A custom map: only its rows reach the word; th08.cfg's default fields see no buttons; the d-pad steers.
    #[test]
    fn custom_map_over_a_button_sequence() {
        let mut m = PadMap::default();
        m.set(0, "RightTrigger2,South").unwrap();
        m.set(1, "RightZ").unwrap();
        m.set(3, "Start").unwrap();
        m.set(6, "LeftThumb").unwrap();
        let b = |n| 1u32 << button(n).unwrap();
        let seq: [(u32, [i32; 2], u16); 9] = [
            (0, [0, 0], 0),
            (b("RightTrigger2"), [0, 0], SHOT),
            (b("South") | b("RightZ"), [0, 0], SHOT | BOMB),
            (b("East"), [0, 0], 0),
            (b("LeftTrigger"), [0, 0], 0),
            (b("Start") | b("LeftThumb"), [0, 0], PAUSE | KEY_Q),
            (1 << DPAD_UP | 1 << DPAD_LEFT, [900, 900], UP | LEFT),
            (1 << DPAD_LEFT | 1 << DPAD_RIGHT, [900, 0], RIGHT),
            (1 << 8, [0, -1000], UP),
        ];
        for (phys, axis, want) in seq {
            let (pad, _) = m.apply(phys, axis);
            assert_eq!(pad.buttons, 0);
            assert_eq!(bits(&m, phys, axis), want, "phys {phys:#x}");
        }
    }
}
