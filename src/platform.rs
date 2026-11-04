//! Platform side of `host::Host` (ARCH_A §2.2): save storage (fs / localStorage), clocks, the winit keymap and the
//! gilrs joystick (app), and the scripted / recorded-poll host of the tools and tests. Not a port of the original.

pub mod overlay;
pub mod portmenu;
pub mod safearea;
pub mod touch;

use th08_core::host::{Host, LocalTime, MAX_PLAYERS, RawInput, Words};
use th08_core::input::*;
use winit::keyboard::KeyCode;

// Input_Poll 0x43D970 DIK table, the default path (15 §1.1): physical keys = DIK scan codes. Keypad 7/9/1/3 give the
// diagonals; only the main Enter (0x1C) is mapped, keypad Enter (0x9C) is not.
const KEYS: [(KeyCode, u16); 26] = [
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

pub fn map_key(k: KeyCode) -> u16 {
    KEYS.iter().find(|e| e.0 == k).map_or(0, |e| e.1)
}

// The keyboard device: held keys (bit i = KEYS[i] down); each held key ORs its bits into the word (15 §1.1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Keyboard {
    keys: u32,
    // The keyboard read fails (15 §1.1). Port: winit has no device read, so focus loss stands in for a failing
    // GetDeviceState (keys released).
    failed: bool,
}

impl Keyboard {
    pub fn key(&mut self, k: KeyCode, down: bool) {
        let Some(i) = KEYS.iter().position(|e| e.0 == k) else {
            return;
        };
        if down {
            self.keys |= 1 << i;
        } else {
            self.keys &= !(1 << i);
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.failed = !focused;
        if !focused {
            self.keys = 0;
        }
    }

    // A failed GetDeviceState zeroes the word, so the poll returns the joystick bits only (15 §1.1, 0x43DD3C).
    pub fn word(&self) -> u16 {
        if self.failed {
            return 0;
        }
        KEYS.iter()
            .enumerate()
            .filter(|(i, _)| self.keys >> i & 1 != 0)
            .fold(0, |w, (_, e)| w | e.1)
    }
}

pub use touch::TouchSource;

// App-side input mux (ARCH_D §3.1): per poll, `keys[p]` = the OR of the device words, as Input_Poll ORs keyboard and
// joystick bits (15 §1.1). Gamepads that report buttons as keys (Android) plug in here as further sources; the gilrs
// pad state goes to core via `RawInput::pads`, which maps it with cfg / ShotSlow.
#[derive(Clone, Debug, Default)]
pub struct InputMux {
    pub keyboard: Keyboard,
    pub touch: TouchSource,
}

impl InputMux {
    // Focus loss stands in for a failing device read on every source (see `Keyboard::failed`).
    pub fn set_focus(&mut self, focused: bool) {
        self.keyboard.set_focus(focused);
        self.touch.set_focus(focused);
    }

    // One Input_Poll (touch pulses advance per poll, ARCH_D §3.3). Vanilla th08 is single-player: player 1's word
    // stays 0.
    pub fn poll(&mut self) -> Words {
        let mut w = [0; MAX_PLAYERS];
        w[0] = self.keyboard.word() | self.touch.poll();
        w
    }
}

// Save storage (not a port of the original, which writes next to th08.exe, 00 §1): never the game directory.
// Native: `save_dir`, None = no saves. Web: localStorage (hex).
#[derive(Clone, Debug, Default)]
pub struct Storage {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    save_dir: Option<std::path::PathBuf>,
}

impl Storage {
    pub fn new(save_dir: Option<std::path::PathBuf>) -> Self {
        Self { save_dir }
    }

    // Tools/tests: a private save directory.
    pub fn set_save_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.save_dir = dir;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage {
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.save_dir.as_ref()?.join(name)).ok()
    }

    pub fn write(&self, name: &str, data: &[u8]) {
        let Some(dir) = &self.save_dir else { return };
        let path = dir.join(name);
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if let Err(e) = std::fs::write(&path, data) {
            log::warn!("{}: {e}", path.display());
        }
    }

    // Result state 12 creates the directory `replay` (16 §2.6); web storage has no directories.
    pub fn mkdir(&self, name: &str) {
        if let Some(dir) = &self.save_dir {
            let _ = std::fs::create_dir_all(dir.join(name));
        }
    }

    pub fn list(&self, prefix: &str) -> Vec<String> {
        let Some(dir) = &self.save_dir else {
            return Vec::new();
        };
        let (sub, file) = prefix.rsplit_once('/').unwrap_or(("", prefix));
        let Ok(rd) = std::fs::read_dir(dir.join(sub)) else {
            return Vec::new();
        };
        rd.filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with(file))
            .map(|n| {
                if sub.is_empty() {
                    n
                } else {
                    format!("{sub}/{n}")
                }
            })
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
impl Storage {
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let s = local_storage()?.get_item(&format!("th08:{name}")).ok()??;
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
            .collect()
    }

    pub fn write(&self, name: &str, data: &[u8]) {
        use std::fmt::Write;
        let mut s = String::with_capacity(data.len() * 2);
        for b in data {
            let _ = write!(s, "{b:02x}");
        }
        if let Some(ls) = local_storage() {
            let _ = ls.set_item(&format!("th08:{name}"), &s);
        }
    }

    pub fn mkdir(&self, _: &str) {}

    pub fn list(&self, prefix: &str) -> Vec<String> {
        let Some(ls) = local_storage() else {
            return Vec::new();
        };
        let n = ls.length().unwrap_or(0);
        (0..n)
            .filter_map(|i| ls.key(i).ok()?)
            .filter_map(|k| k.strip_prefix("th08:").map(str::to_string))
            .filter(|k| k.starts_with(prefix))
            .collect()
    }
}

// Wall clock and monotonic clock of the platform (web_time / js Date on the web).
#[derive(Clone, Debug)]
pub struct Clock {
    epoch: web_time::Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            epoch: web_time::Instant::now(),
        }
    }
}

impl Clock {
    #[cfg_attr(
        target_arch = "wasm32",
        expect(clippy::cast_sign_loss, reason = "Date.now() is ≥ 0")
    )]
    pub fn boot_ms(&self) -> u32 {
        #[cfg(not(target_arch = "wasm32"))]
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        #[cfg(target_arch = "wasm32")]
        let ms = js_sys::Date::now() as u64;
        ms as u32
    }

    pub fn ticks_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn local_time(&self) -> LocalTime {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use chrono::{Datelike, Timelike};
            let t = chrono::Local::now();
            LocalTime {
                year: t.year(),
                month: t.month(),
                day: t.day(),
                hour: t.hour(),
                minute: t.minute(),
                second: t.second(),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // The JS Date getters are local time.
            let d = js_sys::Date::new_0();
            LocalTime {
                year: d.get_full_year().cast_signed(),
                month: d.get_month() + 1,
                day: d.get_date(),
                hour: d.get_hours(),
                minute: d.get_minutes(),
                second: d.get_seconds(),
            }
        }
    }
}

// Debug path reads (`TH08_REPLAY`, `TH08_INPUT`): a file from disk natively, else the save storage.
fn read_path(storage: &Storage, path: &str) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(b) = std::fs::read(path) {
        return Some(b);
    }
    storage.read(path)
}

// The game window's host: the input mux (fed by the event loop), the first gilrs gamepad, platform storage.
#[derive(Default)]
pub struct AppHost {
    pub input: InputMux,
    pub joystick: crate::joystick::Joystick,
    pub storage: Storage,
    pub clock: Clock,
}

impl AppHost {
    pub fn new(storage: Storage, touch: touch::TouchConfig) -> Self {
        Self {
            joystick: crate::joystick::Joystick::new(),
            input: InputMux {
                touch: TouchSource::new(touch),
                ..Default::default()
            },
            storage,
            ..Default::default()
        }
    }
}

impl Host for AppHost {
    fn poll_input(&mut self) -> RawInput {
        let mut r = RawInput {
            keys: self.input.poll(),
            ..Default::default()
        };
        r.pads[0] = self.joystick.poll();
        r
    }
    fn boot_clock_ms(&mut self) -> u32 {
        self.clock.boot_ms()
    }
    fn ticks_ms(&mut self) -> u64 {
        self.clock.ticks_ms()
    }
    fn local_time(&mut self) -> LocalTime {
        self.clock.local_time()
    }
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.storage.read(name)
    }
    fn write(&mut self, name: &str, data: &[u8]) {
        self.storage.write(name, data);
    }
    fn mkdir(&mut self, name: &str) {
        self.storage.mkdir(name);
    }
    fn list(&mut self, prefix: &str) -> Vec<String> {
        self.storage.list(prefix)
    }
    fn read_path(&mut self, path: &str) -> Option<Vec<u8>> {
        read_path(&self.storage, path)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Tools and tests: no devices. Each poll answers `keys`, or with recorded polls (session polls.txt) the value of poll
// index `cursor`, then advances it (one poll per calc pass, so a restarted pass reads the next index, 00 §3).
#[derive(Clone, Debug, Default)]
pub struct ToolHost {
    pub storage: Storage,
    pub clock: Clock,
    pub keys: Words,
    pub polls: Option<std::sync::Arc<[u16]>>,
    // Poll index of the next poll; negative = boot frames before the first menu frame, which never poll (0).
    pub cursor: i64,
    // Port-only: a fixed `localtime` answer (session replays: the recording time), so dated saves draw the same any day.
    pub fixed_time: Option<LocalTime>,
}

impl ToolHost {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            ..Default::default()
        }
    }

    // Recorded Input_Poll return of poll `poll` (polls.txt `S`/`I` rows); 0 outside the recording.
    pub fn recorded(&self, poll: i64) -> u16 {
        let Some(p) = &self.polls else { return 0 };
        usize::try_from(poll)
            .ok()
            .and_then(|i| p.get(i).copied())
            .unwrap_or(0)
    }
}

impl Host for ToolHost {
    fn poll_input(&mut self) -> RawInput {
        let mut keys = self.keys;
        if self.polls.is_some() {
            keys = [0; MAX_PLAYERS];
            keys[0] = self.recorded(self.cursor);
            self.cursor += 1;
        }
        RawInput {
            keys,
            ..Default::default()
        }
    }
    fn boot_clock_ms(&mut self) -> u32 {
        self.clock.boot_ms()
    }
    fn ticks_ms(&mut self) -> u64 {
        self.clock.ticks_ms()
    }
    fn local_time(&mut self) -> LocalTime {
        self.fixed_time.unwrap_or_else(|| self.clock.local_time())
    }
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.storage.read(name)
    }
    fn write(&mut self, name: &str, data: &[u8]) {
        self.storage.write(name, data);
    }
    fn mkdir(&mut self, name: &str) {
        self.storage.mkdir(name);
    }
    fn list(&mut self, prefix: &str) -> Vec<String> {
        self.storage.list(prefix)
    }
    fn read_path(&mut self, path: &str) -> Option<Vec<u8>> {
        read_path(&self.storage, path)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// Tools and tests: the `ToolHost` of a context loaded with `Settings::tool_host`.
pub fn tool_host(ctx: &mut th08_core::game::Ctx) -> &mut ToolHost {
    ctx.host_as().expect("not a ToolHost")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 15 §1.1 DIK table: keypad directions and diagonals, both Shift/Ctrl, Home/P; keypad Enter unmapped.
    #[test]
    fn key_table() {
        assert_eq!(map_key(KeyCode::Numpad8), UP);
        assert_eq!(map_key(KeyCode::Numpad3), DOWN | RIGHT);
        assert_eq!(map_key(KeyCode::Numpad7), 0x50);
        assert_eq!(map_key(KeyCode::Numpad9), 0x90);
        assert_eq!(map_key(KeyCode::Numpad1), 0x60);
        assert_eq!(map_key(KeyCode::Numpad3), 0xA0);
        assert_eq!(map_key(KeyCode::NumpadEnter), 0);
        assert_eq!(map_key(KeyCode::Enter), ENTER);
        let mut k = Keyboard::default();
        k.key(KeyCode::Numpad7, true);
        k.key(KeyCode::ArrowUp, true);
        k.key(KeyCode::Numpad7, false);
        assert_eq!(k.word(), UP);
        k.key(KeyCode::KeyP, true);
        k.key(KeyCode::Home, true);
        k.key(KeyCode::Home, false);
        assert_eq!(k.word(), UP | KEY_HOME);
    }

    // 15 §1.1: a failed keyboard read gives the joystick bits only; focus loss releases the keys.
    #[test]
    fn failed_read() {
        let mut k = Keyboard::default();
        k.key(KeyCode::KeyX, true);
        k.set_focus(false);
        assert_eq!(k.word(), 0);
        k.set_focus(true);
        assert_eq!(k.word(), 0);
        k.key(KeyCode::KeyX, true);
        assert_eq!(k.word(), BOMB);
    }

    // The mux ORs the sources into player 0's word; focus loss clears them all.
    #[test]
    fn mux_words() {
        let mut m = InputMux::default();
        m.keyboard.key(KeyCode::KeyZ, true);
        m.keyboard.key(KeyCode::ArrowLeft, true);
        assert_eq!(m.poll(), [SHOT | LEFT, 0]);
        m.set_focus(false);
        assert_eq!(m.poll(), [0, 0]);
    }

    // A restarted pass polls again and reads the next recorded index (00 §3).
    #[test]
    fn recorded_polls() {
        let mut h = ToolHost {
            polls: Some(vec![1, 2, 3].into()),
            cursor: -1,
            ..Default::default()
        };
        let p: Vec<u16> = (0..5).map(|_| h.poll_input().keys[0]).collect();
        assert_eq!(p, [0, 1, 2, 3, 0]);
    }
}
