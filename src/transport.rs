//! Replay transport: pause, one-frame step and playback rates while a replay plays; keys, touch strip and label.

use crate::i18n::{self, Lang};
use winit::keyboard::KeyCode;

// (logic frames, frame periods): ¼× … 8×.
pub const RATES: [(u32, u32); 7] = [(1, 4), (1, 2), (3, 4), (1, 1), (2, 1), (4, 1), (8, 1)];
pub const NORMAL: usize = 3;
// Practice slow motion (port menu): 1×, ¾×, ½×.
pub const SLOW: [(u32, u32); 3] = [(1, 1), (3, 4), (1, 2)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pause,
    Step,
    Slower,
    Faster,
}

impl Action {
    pub const ALL: [Action; 4] = [Action::Slower, Action::Pause, Action::Step, Action::Faster];

    // Desktop keys; none of them can be bound to a game input (`keymap.rs`).
    pub fn of_key(k: KeyCode) -> Option<Action> {
        match k {
            KeyCode::F5 => Some(Action::Pause),
            KeyCode::F6 => Some(Action::Step),
            KeyCode::F7 => Some(Action::Slower),
            KeyCode::F8 => Some(Action::Faster),
            _ => None,
        }
    }
}

// Index into `RATES`, the pause flag; the pacer gets them from the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transport {
    pub rate: usize,
    pub paused: bool,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            rate: NORMAL,
            paused: false,
        }
    }
}

impl Transport {
    // Step is the pacer's (one frame owed while paused); here it only pauses.
    pub fn act(&mut self, a: Action) {
        match a {
            Action::Pause => self.paused = !self.paused,
            Action::Step => self.paused = true,
            Action::Slower => self.rate = self.rate.saturating_sub(1),
            Action::Faster => self.rate = (self.rate + 1).min(RATES.len() - 1),
        }
    }

    pub fn ratio(self) -> (u32, u32) {
        RATES[self.rate]
    }

    pub fn label(self, lang: Lang) -> String {
        if self.paused {
            return i18n::ui(lang, "replay_paused").to_string();
        }
        rate_label(lang, self.ratio())
    }
}

// "½×" for (1, 2).
pub fn rate_label(lang: Lang, rate: (u32, u32)) -> String {
    let r = match rate {
        (1, 4) => "¼".to_string(),
        (1, 2) => "½".to_string(),
        (3, 4) => "¾".to_string(),
        (n, _) => n.to_string(),
    };
    i18n::ui_fmt(lang, "replay_rate", &[("rate", &r)])
}

// The touch strip: four square buttons, top centre of the safe rect `[x, y, w, h]`; `px` = physical px per dp.
pub fn strip(safe: [f32; 4], px: f32) -> [(Action, [f32; 4]); 4] {
    let (size, gap) = (44.0 * px, 8.0 * px);
    let total = 4.0 * size + 3.0 * gap;
    let x0 = safe[0] + (safe[2] - total) / 2.0;
    let y = safe[1] + gap;
    let mut out = [(Action::Pause, [0.0; 4]); 4];
    for (i, a) in Action::ALL.into_iter().enumerate() {
        out[i] = (a, [x0 + i as f32 * (size + gap), y, size, size]);
    }
    out
}

pub fn hit(strip: &[(Action, [f32; 4])], x: f32, y: f32) -> Option<Action> {
    strip
        .iter()
        .find(|(_, r)| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
        .map(|(a, _)| *a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_clamp_and_label() {
        let mut t = Transport::default();
        assert_eq!(t.label(Lang::En), "1×");
        for _ in 0..9 {
            t.act(Action::Faster);
        }
        assert_eq!(t.ratio(), (8, 1));
        for _ in 0..9 {
            t.act(Action::Slower);
        }
        assert_eq!((t.ratio(), t.label(Lang::En).as_str()), ((1, 4), "¼×"));
        t.act(Action::Step);
        assert!(t.paused);
        assert_eq!(t.label(Lang::En), "Paused");
        t.act(Action::Pause);
        assert!(!t.paused);
        for l in Lang::ALL {
            assert!(!Transport { paused: true, ..t }.label(l).is_empty());
        }
    }

    #[test]
    fn keys_outside_game_table() {
        for k in [KeyCode::F5, KeyCode::F6, KeyCode::F7, KeyCode::F8] {
            assert!(Action::of_key(k).is_some());
            assert_eq!(crate::platform::map_key(k), 0);
        }
        for k in [
            KeyCode::F1,
            KeyCode::F3,
            KeyCode::F5,
            KeyCode::F8,
            KeyCode::F11,
            KeyCode::F12,
        ] {
            assert!(crate::platform::keymap::name(k).is_none());
        }
    }

    #[test]
    fn strip_hits() {
        let s = strip([0.0, 0.0, 1000.0, 600.0], 1.0);
        let c = |i: usize| (s[i].1[0] + 5.0, s[i].1[1] + 5.0);
        for (i, a) in Action::ALL.into_iter().enumerate() {
            let (x, y) = c(i);
            assert_eq!(hit(&s, x, y), Some(a));
        }
        assert_eq!(hit(&s, 5.0, 300.0), None);
    }
}
