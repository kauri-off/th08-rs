//! Host interface (ARCH_A §2.2, ARCH_C decisions 3 and 6): the core's only way to platform input, clocks and save
//! storage. Implemented by the app (`platform::AppHost`), the tools (`platform::ToolHost`) and core tests (`MemHost`).

use crate::input::PadState;

// Per-frame input is a per-player array; vanilla th08 is single-player and reads `[0]` (ARCH_C decision 6).
pub const MAX_PLAYERS: usize = 2;
pub type Words = [u16; MAX_PLAYERS];

// One Input_Poll 0x43D970 device read per player (15 §1.1): the keyboard word (0 when GetDeviceState fails) and the
// raw joystick state; the joystick → word mapping uses cfg and the ShotSlow counter, so it stays in core (`Input::poll`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RawInput {
    pub keys: Words,
    pub pads: [PadState; MAX_PLAYERS],
}

// C `localtime` fields (15 §2.3, §3.5; 16 §2.2): month 1–12, day 1–31.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl LocalTime {
    // "%m/%d" for HSCR +0x20 / replay info +0x6C (15 §2.3, §3.5).
    pub fn date_md(&self) -> String {
        format!("{:02}/{:02}", self.month, self.day)
    }

    // "%Y/%m/%d %H:%M:%S" for the replay USER line プレイ時刻 (15 §3.5).
    pub fn datetime(&self) -> String {
        format!(
            "{}/{:02}/{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

// Called per poll, not per frame: a restarted calc chain polls again inside one frame (00 §3), and the session tool
// answers each poll with the recorded value of that poll index. Save names are relative ("score.dat", "replay/…").
pub trait Host {
    // Input_Poll 0x43D970 (11 §7, 15 §1.1).
    fn poll_input(&mut self) -> RawInput;
    // timeGetTime() for the boot seed (11 §1.3, 0x4464C8).
    fn boot_clock_ms(&mut self) -> u32;
    // Monotonic ms for the PLST run / play times (15 §2.9).
    fn ticks_ms(&mut self) -> u64;
    fn local_time(&mut self) -> LocalTime;
    fn read(&mut self, name: &str) -> Option<Vec<u8>>;
    fn write(&mut self, name: &str, data: &[u8]);
    // Result state 12 creates the directory `replay` (16 §2.6).
    fn mkdir(&mut self, name: &str);
    // Save names starting with `prefix` ("replay/th8_ud").
    fn list(&mut self, prefix: &str) -> Vec<String>;
    // Port debug (`TH08_REPLAY`, `TH08_INPUT`): a user-given path, else a save name.
    fn read_path(&mut self, path: &str) -> Option<Vec<u8>> {
        self.read(path)
    }
    // Tools reach their own host type through `Ctx::host_as`.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// In-memory host (core tests, headless runs): saves in a map, fixed clocks, input `keys` on every poll.
#[derive(Clone, Debug, Default)]
pub struct MemHost {
    pub keys: Words,
    pub files: std::collections::BTreeMap<String, Vec<u8>>,
    pub boot_ms: u32,
    pub ticks: u64,
    pub time: LocalTime,
}

impl Host for MemHost {
    fn poll_input(&mut self) -> RawInput {
        RawInput {
            keys: self.keys,
            ..Default::default()
        }
    }
    fn boot_clock_ms(&mut self) -> u32 {
        self.boot_ms
    }
    fn ticks_ms(&mut self) -> u64 {
        self.ticks
    }
    fn local_time(&mut self) -> LocalTime {
        self.time
    }
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).cloned()
    }
    fn write(&mut self, name: &str, data: &[u8]) {
        self.files.insert(name.to_string(), data.to_vec());
    }
    // Directories are implicit in the names.
    fn mkdir(&mut self, _: &str) {}
    fn list(&mut self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|n| n.starts_with(prefix))
            .cloned()
            .collect()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
