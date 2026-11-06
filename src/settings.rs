//! Port settings: one typed struct from command-line flags > env vars / URL params > `port.cfg` (web: localStorage) > defaults.

use std::path::PathBuf;
use std::sync::OnceLock;

pub use th08_core::coreconfig::Ending;

// Where a value came from, highest priority last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    #[default]
    Default,
    // The game data bundled with this build (launcher prefill); never written to port.cfg.
    Bundled,
    PortCfg,
    Env,
    Cli,
    // Edited on a settings screen (the egui launcher's Play).
    Launcher,
}

// Who a settings screen shows an entry to: players, or the developer view only (cheats, tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    Player,
    Dev,
}

// Platforms whose settings screen shows an entry (bit set: desktop, web, Android).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platforms(u8);

impl Platforms {
    pub const NONE: Self = Self(0);
    pub const DESKTOP: Self = Self(1);
    pub const WEB: Self = Self(2);
    pub const ANDROID: Self = Self(4);
    pub const ALL: Self = Self(7);
    pub const DESKTOP_WEB: Self = Self(3);
    // Desktop and Android (host files, no browser sandbox).
    pub const NATIVE: Self = Self(5);
    // Web and Android (touch screens).
    pub const TOUCH: Self = Self(6);
    pub const NAMES: [(Self, &'static str); 3] = [
        (Self::DESKTOP, "desktop"),
        (Self::WEB, "web"),
        (Self::ANDROID, "android"),
    ];

    // The platform this build runs on.
    pub const HERE: Self = if cfg!(target_os = "android") {
        Self::ANDROID
    } else if cfg!(target_arch = "wasm32") {
        Self::WEB
    } else {
        Self::DESKTOP
    };

    pub const fn contains(self, p: Self) -> bool {
        p.0 != 0 && self.0 & p.0 == p.0
    }

    // Inverse of `names`; unknown names are ignored.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut bits = 0;
        for n in names {
            if let Some((p, _)) = Self::NAMES.iter().find(|(_, m)| *m == n) {
                bits |= p.0;
            }
        }
        Self(bits)
    }

    pub fn names(self) -> Vec<&'static str> {
        Self::NAMES
            .iter()
            .filter(|(p, _)| self.contains(*p))
            .map(|(_, n)| *n)
            .collect()
    }
}

// Value type of an entry (drives parsing checks, `--help` and a settings UI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    // Known choices; the parser may accept aliases too (numbers, short names).
    Enum(&'static [&'static str]),
    // Free text in the given format.
    Text(&'static str),
    // A folder / a file.
    Path,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    GameData,
    Language,
    Display,
    Sound,
    Accessibility,
    Keyboard,
    Gamepad,
    Touch,
    StartGame,
    Cheats,
    Tools,
}

impl Group {
    pub const ALL: [Group; 11] = [
        Group::GameData,
        Group::Language,
        Group::Display,
        Group::Sound,
        Group::Accessibility,
        Group::Keyboard,
        Group::Gamepad,
        Group::Touch,
        Group::StartGame,
        Group::Cheats,
        Group::Tools,
    ];

    // Stable id: settings_schema `group`, i18n table `group.<id>`.
    pub fn id(self) -> &'static str {
        match self {
            Group::GameData => "game_data",
            Group::Language => "language",
            Group::Display => "display",
            Group::Sound => "sound",
            Group::Accessibility => "accessibility",
            Group::Keyboard => "keyboard",
            Group::Gamepad => "gamepad",
            Group::Touch => "touch",
            Group::StartGame => "start_game",
            Group::Cheats => "cheats",
            Group::Tools => "tools",
        }
    }

    // English title; settings screens show `i18n::group_title`.
    pub fn title(self) -> &'static str {
        match self {
            Group::GameData => "Game data",
            Group::Language => "Language",
            Group::Display => "Display",
            Group::Sound => "Sound",
            Group::Accessibility => "Accessibility",
            Group::Keyboard => "Keyboard",
            Group::Gamepad => "Gamepad",
            Group::Touch => "Touch controls",
            Group::StartGame => "Start game",
            Group::Cheats => "Cheats/debug",
            Group::Tools => "Recording/tools",
        }
    }
}

const U32: i64 = u32::MAX as i64;
const I32: i64 = i32::MAX as i64;
const I32_MIN: i64 = i32::MIN as i64;
const I64: i64 = i64::MAX;
const I64_MIN: i64 = i64::MIN;

type Setter = fn(&mut Settings, &str) -> Result<(), String>;

// One setting: `key` is the port.cfg key and the `--key` flag (`-` or `_`); `env` the env var; the web URL param
// is `env` without `TH08_`, lowercase. `default` is the value text ("" = unset). `label`, `summary`, `unit`,
// `words`, `audience`, `platforms` and `persist` are for settings screens (egui launcher, Android launcher via `settings_schema`).
pub struct Entry {
    pub key: &'static str,
    pub env: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    pub group: Group,
    pub label: &'static str,
    // Short plain-words line under the label on settings screens (sentence case, no flags); `help` is the long text.
    pub summary: &'static str,
    // Suffix after a number on settings screens ("%", "×", " dp").
    pub unit: &'static str,
    // Display words by value text: every enum choice, special numbers ("0" → "Off"), "" = the unset value.
    pub words: &'static [(&'static str, &'static str)],
    pub audience: Audience,
    pub platforms: Platforms,
    // Saved to port.cfg / localStorage when edited on a screen; false = this run only.
    pub persist: bool,
    pub help: &'static str,
    set: Setter,
}

impl Entry {
    // Shown on this platform's settings screen.
    pub fn shown_on_this_platform(&self) -> bool {
        self.platforms.contains(Platforms::HERE)
    }

    pub fn url(&self) -> String {
        self.env
            .strip_prefix("TH08_")
            .unwrap_or(self.env)
            .to_ascii_lowercase()
    }

    pub fn is_bool(&self) -> bool {
        self.kind == Kind::Bool
    }

    // Value hint for `--help`.
    pub fn hint(&self) -> String {
        match self.kind {
            Kind::Bool => String::new(),
            Kind::Int { min, max } if max.checked_sub(min).is_some_and(|d| d <= 1000) => {
                format!("{min}-{max}")
            }
            Kind::Int { .. } => "N".into(),
            Kind::Enum(c) if c.len() <= 9 => c.join("|"),
            Kind::Enum(_) => "NAME".into(),
            Kind::Text(f) => f.into(),
            Kind::Path => "PATH".into(),
            Kind::File => "FILE".into(),
        }
    }

    // Whether `v` is a valid value (parsed into a scratch `Settings`).
    pub fn check(&self, v: &str) -> Result<(), String> {
        self.parse_into(&mut Settings::empty(), v.trim())
    }

    // Type check from `kind`, then the entry's own parser.
    fn parse_into(&self, s: &mut Settings, v: &str) -> Result<(), String> {
        match self.kind {
            Kind::Bool => {
                parse_bool(v)?;
            }
            Kind::Int { min, max } => {
                let n: f64 = v.trim().parse().map_err(|_| "expected a number")?;
                if n < min as f64 || n > max as f64 {
                    return Err(format!("expected {min}-{max}"));
                }
            }
            _ => {}
        }
        (self.set)(s, v)
    }
}

// Platform directories given by the entry point; settings.rs never looks them up itself.
// Desktop: `launcher::desktop_dirs`; web: none (port.cfg and saves live in localStorage).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlatformDirs {
    // Holds port.cfg.
    pub config: Option<PathBuf>,
    // score.dat, th08.cfg, replay/ unless the `save_dir` setting overrides it; None = no saves.
    pub save: Option<PathBuf>,
    // SoundFont search dirs after the `soundfont` setting, in lookup order.
    pub soundfonts: Vec<PathBuf>,
}

impl PlatformDirs {
    pub fn port_cfg(&self) -> Option<PathBuf> {
        Some(self.config.as_ref()?.join("port.cfg"))
    }

    // Tool and test runs: no config dir (no port.cfg, no downloaded packs) and no save dir unless `save_dir` is set.
    #[must_use]
    pub fn tool(self) -> Self {
        Self {
            config: None,
            save: None,
            soundfonts: self.soundfonts,
        }
    }
}

#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per independent settings-table entry"
)]
pub struct Settings {
    // Effective value text and origin of every entry set from a source (defaults included).
    values: std::collections::BTreeMap<&'static str, (String, Origin)>,
    pub dirs: PlatformDirs,
    // Tool or test run: defaults plus explicit env / CLI only, saves only in an explicit `save_dir`.
    pub hermetic: bool,
    pub game_dir: Option<PathBuf>,
    // Window client size 640×480 × scale (1–4); native only.
    pub scale: u32,
    pub fullscreen: bool,
    pub vsync: bool,
    // Extra dp kept clear on every window edge, on top of the platform's safe-area insets (platform/safearea.rs).
    pub screen_margin: u32,
    // Show the launcher screen before the game (th08 binary only).
    pub launcher: bool,
    // Ask GitHub once a day whether a newer release exists (launcher only, release builds only; update.rs).
    pub update_check: bool,
    // Discord Rich Presence through the local Discord app (discord.rs; Windows and Linux).
    pub discord: bool,
    // Open the port menu when the window / tab loses focus in a stage (desktop, web).
    pub pause_unfocused: bool,
    pub blit: th08_render::gfx::BlitOptions,
    pub hitboxes: crate::platform::hitbox::Hitboxes,
    pub keys: crate::platform::keymap::KeyMap,
    pub pads: crate::platform::padmap::PadMap,
    // F12 / port-menu screenshots at the window size (else 640×480).
    pub screenshot_window: bool,
    // Input display corner (None = off) and opacity 0–1.
    pub input_display: Option<crate::platform::overlay::Corner>,
    pub input_opacity: f32,
    // "Now playing" toast when a BGM track starts.
    pub now_playing: bool,
    pub volume: th08_audio::audio::PortVolume,
    pub mute_unfocused: bool,
    pub audio_buffer: Option<u32>,
    // Touch overlay rows.
    pub touch: crate::platform::touch::TouchConfig,
    pub save_dir: Option<PathBuf>,
    pub font: Option<String>,
    // Launcher / port menu language; None = the system locale (i18n.rs).
    pub language: Option<crate::i18n::Lang>,
    // Game text language: `ja` (the original) or the language code of a presentation pack (`en`, `pt-br`…);
    // None = follow `language`.
    pub game_text: Option<String>,
    // Presentation pack to mount (packs.rs): a folder / .zip path or an id in the pack folders; None = by `game_text`.
    pub pack: Option<String>,
    pub soundfont: Option<PathBuf>,
    pub seed: Option<u16>,
    pub stage: Option<usize>,
    pub stage_frame: u32,
    pub player: Option<u8>,
    pub rank: Option<i32>,
    pub power: Option<f32>,
    pub continues: u8,
    pub ending: Option<Ending>,
    pub replay: Option<String>,
    pub replay_speed: u8,
    // Index into `transport::RATES`.
    pub replay_rate: usize,
    pub replay_save: Option<u32>,
    pub input: Option<String>,
    pub invuln: bool,
    pub unlock: bool,
    pub enemy_dmg: i32,
    pub bomb: Option<usize>,
    pub clrd: Option<Vec<u16>>,
    // `<frame>:<type>,<p0>,<p1>,<p2>,<p3>`.
    pub screen_fx: Option<[i64; 6]>,
    pub shot: Option<u64>,
    pub shot_size: Option<(u32, u32)>,
    pub turbo: u32,
    pub present_hz: Option<u32>,
    pub slot_test: Option<usize>,
    pub anm: Option<String>,
    pub anm_int: Vec<(u64, u16)>,
    pub anm_pos: Vec<f32>,
    pub anm_mode: Option<i32>,
    pub se_log: bool,
    pub drawlog: Option<String>,
    pub result_shots: Vec<u32>,
    pub test_frames: u32,
    pub session_rng: Option<i64>,
    pub session_dump: Option<(i64, i64)>,
    pub session_pb: bool,
    pub bdump: bool,
    pub idump: bool,
    pub edump: Option<u8>,
    pub sht_dump: bool,
}

impl Default for Settings {
    // The table defaults.
    fn default() -> Self {
        let mut s = Self::empty();
        for e in ENTRIES.iter().filter(|e| !e.default.is_empty()) {
            s.apply(Origin::Default, e.key, e, e.default);
        }
        s
    }
}

impl Settings {
    fn empty() -> Self {
        Self {
            values: std::collections::BTreeMap::default(),
            dirs: PlatformDirs::default(),
            hermetic: false,
            game_dir: None,
            scale: 1,
            fullscreen: false,
            vsync: true,
            screen_margin: 0,
            launcher: true,
            update_check: true,
            discord: false,
            pause_unfocused: true,
            blit: th08_render::gfx::BlitOptions::default(),
            hitboxes: crate::platform::hitbox::Hitboxes::Off,
            keys: crate::platform::keymap::KeyMap::default(),
            pads: crate::platform::padmap::PadMap::default(),
            screenshot_window: false,
            input_display: None,
            input_opacity: 0.8,
            now_playing: false,
            volume: th08_audio::audio::PortVolume::default(),
            mute_unfocused: true,
            audio_buffer: None,
            touch: crate::platform::touch::TouchConfig::default(),
            save_dir: None,
            font: None,
            language: None,
            game_text: None,
            pack: None,
            soundfont: None,
            seed: None,
            stage: None,
            stage_frame: 0,
            player: None,
            rank: None,
            power: None,
            continues: 0,
            ending: None,
            replay: None,
            replay_speed: 0,
            replay_rate: crate::transport::NORMAL,
            replay_save: None,
            input: None,
            invuln: false,
            unlock: false,
            enemy_dmg: 0,
            bomb: None,
            clrd: None,
            screen_fx: None,
            shot: None,
            shot_size: None,
            turbo: 0,
            present_hz: None,
            slot_test: None,
            anm: None,
            anm_int: Vec::new(),
            anm_pos: Vec::new(),
            anm_mode: None,
            se_log: false,
            drawlog: None,
            result_shots: Vec::new(),
            test_frames: 4000,
            session_rng: None,
            session_dump: None,
            session_pb: false,
            bdump: false,
            idump: false,
            edump: None,
            sht_dump: false,
        }
    }
}

// Empty means off: a cleared env var (`TH08_INVULN=`) or URL param never turns a switch on. `--switch` alone is
// clap's "on".
pub fn parse_bool(v: &str) -> Result<bool, String> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Ok(true),
        "" | "0" | "false" | "off" | "no" => Ok(false),
        _ => Err("expected 1/0, true/false or on/off".into()),
    }
}

// A pack language code: `ja`, `en`, `pt-br`… ([a-z0-9-], as thpatch's `lang_<code>`).
pub fn is_lang_code(c: &str) -> bool {
    !c.is_empty()
        && c.len() <= 32
        && c.chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn num<T: std::str::FromStr>(v: &str) -> Result<T, String> {
    v.trim()
        .parse()
        .map_err(|_| format!("expected a {}", std::any::type_name::<T>()))
}

fn list<T: std::str::FromStr>(v: &str) -> Result<Vec<T>, String> {
    v.split(',').map(num).collect()
}

fn pair<A: std::str::FromStr, B: std::str::FromStr>(v: &str, sep: char) -> Result<(A, B), String> {
    let (a, b) = v
        .split_once(sep)
        .ok_or_else(|| format!("expected <a>{sep}<b>"))?;
    Ok((num(a)?, num(b)?))
}

// `0x` hex or decimal u16 (TH08_CLRD masks).
fn mask(t: &str) -> Result<u16, String> {
    let t = t.trim();
    t.strip_prefix("0x")
        .map_or_else(|| t.parse().ok(), |h| u16::from_str_radix(h, 16).ok())
        .ok_or_else(|| format!("{t}: expected a decimal or 0x hex mask"))
}

fn parse_ending(v: &str) -> Result<Ending, String> {
    if v.ends_with(".end") {
        return Ok(Ending::File(v.to_string()));
    }
    let (n, k) = v.split_at(v.len().saturating_sub(1));
    let shot: u8 = n
        .parse()
        .map_err(|_| "expected <shot><a|b|c> or a .end file".to_string())?;
    Ok(Ending::Kind {
        shot,
        kind: k.chars().next().unwrap_or('b'),
    })
}

// A Keyboard row: the keys of game input `INPUTS[$i]`.
macro_rules! key_entry {
    ($i:literal, $env:literal, $label:literal, $summary:literal, $help:literal) => {
        Entry {
            key: crate::platform::keymap::INPUTS[$i].key,
            env: $env,
            kind: Kind::Text("KEY[,KEY…] | none"),
            default: crate::platform::keymap::INPUTS[$i].default,
            group: Group::Keyboard,
            label: $label,
            summary: $summary,
            unit: "",
            words: &[("none", "Not bound")],
            audience: Audience::Player,
            platforms: Platforms::DESKTOP_WEB,
            persist: true,
            help: $help,
            set: |s, v| s.keys.set($i, v),
        }
    };
}

// A Gamepad row: the pad buttons of game input `padmap::INPUTS[$i]`; read on Android too (no screen there).
macro_rules! pad_entry {
    ($i:literal, $env:literal, $label:literal, $summary:literal, $help:literal) => {
        Entry {
            key: crate::platform::padmap::INPUTS[$i].key,
            env: $env,
            kind: Kind::Text("BUTTON[,BUTTON…] | none"),
            default: crate::platform::padmap::INPUTS[$i].default,
            group: Group::Gamepad,
            label: $label,
            summary: $summary,
            unit: "",
            words: &[("none", "Not bound")],
            audience: Audience::Player,
            platforms: Platforms::DESKTOP,
            persist: true,
            help: $help,
            set: |s, v| s.pads.set($i, v),
        }
    };
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        key: "game_dir",
        env: "TH08_DIR",
        kind: Kind::Path,
        default: "",
        group: Group::GameData,
        label: "Game folder",
        summary: "Folder with th08.dat and thbgm.dat",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "game folder with th08.dat and thbgm.dat (also the first plain argument)",
        set: |s, v| {
            s.game_dir = Some(PathBuf::from(v));
            Ok(())
        },
    },
    Entry {
        key: "language",
        env: "TH08_LANGUAGE",
        kind: Kind::Enum(&["auto", "ja", "en", "ru"]),
        default: "auto",
        group: Group::Language,
        label: "Language",
        summary: "Language of the settings screens",
        unit: "",
        words: &[
            ("auto", "System"),
            ("ja", "日本語"),
            ("en", "English"),
            ("ru", "Русский"),
        ],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "language of this screen and the port menu: auto (system language), ja, en or ru",
        set: |s, v| {
            s.language = match v.trim() {
                "auto" => None,
                l => Some(crate::i18n::Lang::from_code(l).ok_or("expected auto, ja, en or ru")?),
            };
            Ok(())
        },
    },
    // Choices: follow, ja, then the languages of the installed packs (the settings screens add those).
    Entry {
        key: "game_text",
        env: "TH08_GAME_TEXT",
        kind: Kind::Enum(&["follow", "ja"]),
        default: "follow",
        group: Group::Language,
        label: "Game text language",
        summary: "Language of the in-game text",
        unit: "",
        words: &[
            ("follow", "Same as the screens"),
            ("ja", "日本語 (original)"),
        ],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "language of dialogue and other game text: follow (the screen language), ja (original) or the code \
               of an installed translation (en, ru, de, pt-br…)",
        set: |s, v| {
            s.game_text = match v.trim() {
                "follow" => None,
                l if is_lang_code(l) => Some(l.to_string()),
                _ => return Err("expected follow, ja or a language code (en, pt-br…)".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "scale",
        env: "TH08_SCALE",
        kind: Kind::Int { min: 1, max: 4 },
        default: "1",
        group: Group::Display,
        label: "Window size",
        summary: "Window size as a multiple of 640×480",
        unit: "×",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "window size 640x480 times this (native)",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.scale = (1..=4).contains(&n).then_some(n).ok_or("expected 1-4")?;
            Ok(())
        },
    },
    Entry {
        key: "fullscreen",
        env: "TH08_FULLSCREEN",
        kind: Kind::Bool,
        default: "off",
        group: Group::Display,
        label: "Fullscreen",
        summary: "Start in fullscreen",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "start fullscreen (native; F11 or Alt+Enter toggles and saves)",
        set: |s, v| {
            s.fullscreen = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "vsync",
        env: "TH08_VSYNC",
        kind: Kind::Bool,
        default: "on",
        group: Group::Display,
        label: "VSync",
        summary: "Sync frames to the display",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "present with vsync (default on); on displays below 60 Hz logic still runs at 60 frames per second",
        set: |s, v| {
            s.vsync = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "screen_margin",
        env: "TH08_SCREEN_MARGIN",
        kind: Kind::Int { min: 0, max: 200 },
        default: "0",
        group: Group::Display,
        label: "Screen margin (dp)",
        summary: "Extra space kept clear at the screen edges",
        unit: " dp",
        words: &[("0", "Off")],
        audience: Audience::Player,
        platforms: Platforms::TOUCH,
        persist: true,
        help: "extra margin on every screen edge in dp, for phones whose cutout / corner insets are wrong",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.screen_margin = n.min(200);
            Ok(())
        },
    },
    Entry {
        key: "launcher",
        env: "TH08_LAUNCHER",
        kind: Kind::Bool,
        default: "on",
        group: Group::Display,
        label: "Show this screen at start",
        summary: "Show this screen before the game starts",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP_WEB,
        persist: true,
        help: "show this screen at start; when off, hold Shift while th08 starts (web: ?launcher=1) to see it again",
        set: |s, v| {
            s.launcher = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "update_check",
        env: "TH08_UPDATE_CHECK",
        kind: Kind::Bool,
        default: "on",
        group: Group::Display,
        label: "Check for updates",
        summary: "Show on this screen when a newer version is out",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::NATIVE,
        persist: true,
        help: "ask GitHub at most once a day, from this screen only, whether a newer release exists; nothing is sent but the request itself",
        set: |s, v| {
            s.update_check = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "discord",
        env: "TH08_DISCORD",
        kind: Kind::Bool,
        default: "off",
        group: Group::Display,
        label: "Discord status",
        summary: "Show your game in Discord; talks only to the local app",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "show the character, difficulty and stage (or replay / menu) in your Discord status; only the Discord app running on this computer is contacted, nothing goes to the internet from the game",
        set: |s, v| {
            s.discord = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "pause_unfocused",
        env: "TH08_PAUSE_UNFOCUSED",
        kind: Kind::Bool,
        default: "on",
        group: Group::Display,
        label: "Pause when unfocused",
        summary: "Open the port menu when you switch away in a stage",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "open the port menu when the window, browser tab or Android app loses focus during a stage (notification shade, split screen)",
        set: |s, v| {
            s.pause_unfocused = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "scale_filter",
        env: "TH08_SCALE_FILTER",
        kind: Kind::Enum(&["smooth", "sharp", "nearest"]),
        default: "sharp",
        group: Group::Display,
        label: "Scaling filter",
        summary: "How the picture is scaled to the window",
        unit: "",
        words: &[
            ("smooth", "Smooth"),
            ("sharp", "Sharp"),
            ("nearest", "Pixelated"),
        ],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "window scaling: smooth (bilinear), sharp (crisp pixels with smooth edges) or nearest",
        set: |s, v| {
            use th08_render::gfx::ScaleFilter;
            s.blit.filter = match v.trim() {
                "smooth" => ScaleFilter::Smooth,
                "sharp" => ScaleFilter::Sharp,
                "nearest" => ScaleFilter::Nearest,
                _ => return Err("expected smooth, sharp or nearest".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "integer_scale",
        env: "TH08_INTEGER_SCALE",
        kind: Kind::Bool,
        default: "off",
        group: Group::Display,
        label: "Integer scaling",
        summary: "Scale only by whole multiples of 640×480",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "scale the picture by the largest whole multiple of 640x480 that fits",
        set: |s, v| {
            s.blit.integer = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "volume_master",
        env: "TH08_VOLUME_MASTER",
        kind: Kind::Int { min: 0, max: 100 },
        default: "100",
        group: Group::Sound,
        label: "Master volume",
        summary: "Volume of all game sound",
        unit: "%",
        words: &[("0", "Off")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "port master volume in percent (the game's own BGM volume option stays as it is)",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.volume.master = n.min(100) as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "volume_bgm",
        env: "TH08_VOLUME_BGM",
        kind: Kind::Int { min: 0, max: 100 },
        default: "100",
        group: Group::Sound,
        label: "Music volume",
        summary: "Volume of the music",
        unit: "%",
        words: &[("0", "Off")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "port music volume in percent",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.volume.bgm = n.min(100) as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "volume_se",
        env: "TH08_VOLUME_SE",
        kind: Kind::Int { min: 0, max: 100 },
        default: "100",
        group: Group::Sound,
        label: "Sound effects volume",
        summary: "Volume of the sound effects",
        unit: "%",
        words: &[("0", "Off")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "port sound effect volume in percent",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.volume.se = n.min(100) as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "mute_unfocused",
        env: "TH08_MUTE_UNFOCUSED",
        kind: Kind::Bool,
        default: "on",
        group: Group::Sound,
        label: "Mute when unfocused",
        summary: "Silence the game while another window is active",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP_WEB,
        persist: true,
        help: "mute the sound while the window or browser tab is not focused",
        set: |s, v| {
            s.mute_unfocused = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "now_playing",
        env: "TH08_NOW_PLAYING",
        kind: Kind::Bool,
        default: "off",
        group: Group::Sound,
        label: "Now playing",
        summary: "Show the track title when the music changes",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "show the music room title of a BGM track (translated by the active pack) for a few seconds when it starts",
        set: |s, v| {
            s.now_playing = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "audio_buffer",
        env: "TH08_AUDIO_BUFFER",
        kind: Kind::Int { min: 0, max: 8192 },
        default: "0",
        group: Group::Sound,
        label: "Audio buffer",
        summary: "Output buffer size; larger fixes crackling",
        unit: " frames",
        words: &[("0", "Auto")],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP_WEB,
        persist: true,
        help: "audio output buffer in frames (0 = the device default); raise it if the sound crackles",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.audio_buffer = (n > 0).then_some(n.min(8192));
            Ok(())
        },
    },
    Entry {
        key: "color_filter",
        env: "TH08_COLOR_FILTER",
        kind: Kind::Enum(&["off", "protan", "deutan", "tritan", "high_contrast"]),
        default: "off",
        group: Group::Accessibility,
        label: "Colour filter",
        summary: "Colours adjusted for colour vision or contrast",
        unit: "",
        words: &[
            ("off", "Off"),
            ("protan", "Protanopia"),
            ("deutan", "Deuteranopia"),
            ("tritan", "Tritanopia"),
            ("high_contrast", "High contrast"),
        ],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "colour filter on the window picture: off, protan, deutan, tritan (colour-vision correction) or high_contrast",
        set: |s, v| {
            use th08_render::gfx::ColorFilter;
            s.blit.color = match v.trim() {
                "off" => ColorFilter::Off,
                "protan" => ColorFilter::Protan,
                "deutan" => ColorFilter::Deutan,
                "tritan" => ColorFilter::Tritan,
                "high_contrast" => ColorFilter::HighContrast,
                _ => return Err("expected off, protan, deutan, tritan or high_contrast".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "brightness",
        env: "TH08_BRIGHTNESS",
        kind: Kind::Int { min: 50, max: 200 },
        default: "100",
        group: Group::Accessibility,
        label: "Brightness",
        summary: "Gamma of the picture; higher brightens dark areas",
        unit: "%",
        words: &[("100", "Normal")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "picture gamma in percent (100 = unchanged; higher brightens dark areas)",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.blit.gamma = n.clamp(50, 200) as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "hitboxes",
        env: "TH08_HITBOXES",
        kind: Kind::Enum(&crate::platform::hitbox::Hitboxes::IDS),
        default: "off",
        group: Group::Accessibility,
        label: "Hitboxes",
        summary: "Outline the collision shapes (replays stay the same)",
        unit: "",
        words: &[("off", "Off"), ("player", "Player"), ("all", "All")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "hitbox viewer: off, player (the player's hit box and graze area) or all (plus bullets, lasers and enemies); F3 or the port menu switches it in the game",
        set: |s, v| {
            s.hitboxes =
                crate::platform::hitbox::Hitboxes::parse(v).ok_or("expected off, player or all")?;
            Ok(())
        },
    },
    key_entry!(
        0,
        "TH08_KEY_UP",
        "Up",
        "Move up; menu cursor up",
        "keys for up: W3C key names such as ArrowUp, KeyW or Numpad8, comma-separated, or none"
    ),
    key_entry!(
        1,
        "TH08_KEY_DOWN",
        "Down",
        "Move down; menu cursor down",
        "keys for down (key names, comma-separated, or none)"
    ),
    key_entry!(
        2,
        "TH08_KEY_LEFT",
        "Left",
        "Move left",
        "keys for left (key names, comma-separated, or none)"
    ),
    key_entry!(
        3,
        "TH08_KEY_RIGHT",
        "Right",
        "Move right",
        "keys for right (key names, comma-separated, or none)"
    ),
    key_entry!(
        4,
        "TH08_KEY_SHOT",
        "Shot",
        "Shoot; confirm in menus",
        "keys for shot (key names, comma-separated, or none)"
    ),
    key_entry!(
        5,
        "TH08_KEY_BOMB",
        "Bomb",
        "Spell card; cancel in menus",
        "keys for bomb (key names, comma-separated, or none)"
    ),
    key_entry!(
        6,
        "TH08_KEY_FOCUS",
        "Focus",
        "Slow movement and the partner's shot",
        "keys for focus (key names, comma-separated, or none)"
    ),
    key_entry!(
        7,
        "TH08_KEY_PAUSE",
        "Pause",
        "Pause menu",
        "keys for pause (key names, comma-separated, or none)"
    ),
    key_entry!(
        8,
        "TH08_KEY_SKIP",
        "Skip",
        "Fast-forward dialogue",
        "keys for skip (key names, comma-separated, or none)"
    ),
    key_entry!(
        9,
        "TH08_KEY_ENTER",
        "Enter",
        "Confirm in menus",
        "keys for enter (key names, comma-separated, or none)"
    ),
    key_entry!(
        10,
        "TH08_KEY_QUIT",
        "Quit (Q)",
        "Quit from the pause menu",
        "keys for the pause menu's quit shortcut (key names, comma-separated, or none)"
    ),
    key_entry!(
        11,
        "TH08_KEY_RETRY",
        "Retry (R)",
        "Restart from the pause menu",
        "keys for the pause menu's retry shortcut (key names, comma-separated, or none)"
    ),
    pad_entry!(
        0,
        "TH08_PAD_SHOT",
        "Pad shot",
        "Shoot; confirm in menus",
        "pad buttons for shot: South, East, West, North, LeftTrigger, RightTrigger, LeftTrigger2, RightTrigger2, Start, LeftThumb, RightThumb, LeftZ, RightZ or RightStickUp / Down / Left / Right, comma-separated, or none; any row off its default replaces th08.cfg's pad config"
    ),
    pad_entry!(
        1,
        "TH08_PAD_BOMB",
        "Pad bomb",
        "Spell card; cancel in menus",
        "pad buttons for bomb (button names, comma-separated, or none)"
    ),
    pad_entry!(
        2,
        "TH08_PAD_FOCUS",
        "Pad focus",
        "Slow movement and the partner's shot",
        "pad buttons for focus (button names, comma-separated, or none)"
    ),
    pad_entry!(
        3,
        "TH08_PAD_PAUSE",
        "Pad pause",
        "Pause menu",
        "pad buttons for pause (button names, comma-separated, or none)"
    ),
    pad_entry!(
        4,
        "TH08_PAD_SKIP",
        "Pad skip",
        "Fast-forward dialogue",
        "pad buttons for skip (button names, comma-separated, or none)"
    ),
    pad_entry!(
        5,
        "TH08_PAD_ENTER",
        "Pad enter",
        "Confirm in menus",
        "pad buttons for enter (button names, comma-separated, or none)"
    ),
    pad_entry!(
        6,
        "TH08_PAD_QUIT",
        "Pad quit (Q)",
        "Quit from the pause menu",
        "pad buttons for the pause menu's quit shortcut (button names, comma-separated, or none)"
    ),
    pad_entry!(
        7,
        "TH08_PAD_RETRY",
        "Pad retry (R)",
        "Restart from the pause menu",
        "pad buttons for the pause menu's retry shortcut (button names, comma-separated, or none)"
    ),
    Entry {
        key: "screenshot_size",
        env: "TH08_SCREENSHOT_SIZE",
        kind: Kind::Enum(&["game", "window"]),
        default: "game",
        group: Group::Display,
        label: "Screenshot resolution",
        summary: "F12 and the port menu save a PNG into screenshots/",
        unit: "",
        words: &[("game", "640×480"), ("window", "Window size")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "screenshot size: game (the 640x480 frame) or window (scaled like the window)",
        set: |s, v| {
            s.screenshot_window = match v.trim() {
                "game" => false,
                "window" => true,
                _ => return Err("expected game or window".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "input_display",
        env: "TH08_INPUT_DISPLAY",
        kind: Kind::Enum(&crate::platform::overlay::Corner::IDS),
        default: "off",
        group: Group::Display,
        label: "Input display",
        summary: "Show each frame's game inputs in a corner, replays too",
        unit: "",
        words: &[
            ("off", "Off"),
            ("top-left", "Top left"),
            ("top-right", "Top right"),
            ("bottom-left", "Bottom left"),
            ("bottom-right", "Bottom right"),
        ],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "input display: off, or the window corner where the arrows, shot, bomb, focus and pause of each frame are shown",
        set: |s, v| {
            s.input_display = match v.trim() {
                "off" => None,
                c => Some(
                    crate::platform::overlay::Corner::parse(c)
                        .ok_or("expected off, top-left, top-right, bottom-left or bottom-right")?,
                ),
            };
            Ok(())
        },
    },
    Entry {
        key: "input_display_opacity",
        env: "TH08_INPUT_DISPLAY_OPACITY",
        kind: Kind::Int { min: 10, max: 100 },
        default: "80",
        group: Group::Display,
        label: "Input display opacity",
        summary: "How visible the input display is",
        unit: "%",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "input display opacity in percent",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.input_opacity = n.clamp(10, 100) as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "touch",
        env: "TH08_TOUCH",
        kind: Kind::Enum(&["auto", "on", "off"]),
        default: "auto",
        group: Group::Touch,
        label: "Touch overlay",
        summary: "On-screen buttons for touch screens",
        unit: "",
        words: &[("auto", "Auto"), ("on", "On"), ("off", "Off")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "touch overlay: auto (touch platforms), on, off",
        set: |s, v| {
            use crate::platform::touch::TouchMode;
            s.touch.mode = match v.trim() {
                "auto" => TouchMode::Auto,
                "on" => TouchMode::On,
                "off" => TouchMode::Off,
                _ => return Err("expected auto, on or off".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "touch_move",
        env: "TH08_TOUCH_MOVE",
        kind: Kind::Enum(&["drag", "stick"]),
        default: "stick",
        group: Group::Touch,
        label: "Movement",
        summary: "How your finger moves the player",
        unit: "",
        words: &[("drag", "Drag"), ("stick", "Floating stick")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "movement: drag (the player follows the finger's movement) or a floating stick",
        set: |s, v| {
            use crate::platform::touch::Movement;
            s.touch.movement = match v.trim() {
                "drag" => Movement::Drag,
                "stick" => Movement::Stick,
                _ => return Err("expected drag or stick".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "touch_sens",
        env: "TH08_TOUCH_SENS",
        kind: Kind::Int { min: 25, max: 400 },
        default: "100",
        group: Group::Touch,
        label: "Drag sensitivity %",
        summary: "How far the player moves per finger move",
        unit: "%",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "drag sensitivity in percent (100 = the player follows the finger 1:1 on the game picture)",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.touch.sens = n as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "touch_focus",
        env: "TH08_TOUCH_FOCUS",
        kind: Kind::Enum(&["hold", "toggle"]),
        default: "hold",
        group: Group::Touch,
        label: "Focus button",
        summary: "Hold the button or tap to switch focus",
        unit: "",
        words: &[("hold", "Hold"), ("toggle", "Toggle")],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "FOCUS button: held, or a toggle",
        set: |s, v| {
            s.touch.focus_toggle = match v.trim() {
                "hold" => false,
                "toggle" => true,
                _ => return Err("expected hold or toggle".into()),
            };
            Ok(())
        },
    },
    Entry {
        key: "touch_autofire",
        env: "TH08_TOUCH_AUTOFIRE",
        kind: Kind::Bool,
        default: "on",
        group: Group::Touch,
        label: "Auto-fire",
        summary: "Shoot without holding a button",
        unit: "",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "auto-fire at start (in play only; the AUTO button toggles it)",
        set: |s, v| {
            s.touch.autofire = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "touch_opacity",
        env: "TH08_TOUCH_OPACITY",
        kind: Kind::Int { min: 10, max: 100 },
        default: "50",
        group: Group::Touch,
        label: "Overlay opacity %",
        summary: "How visible the on-screen buttons are",
        unit: "%",
        words: &[],
        audience: Audience::Player,
        platforms: Platforms::ALL,
        persist: true,
        help: "overlay opacity in percent",
        set: |s, v| {
            let n: u32 = num(v)?;
            s.touch.opacity = n as f32 / 100.0;
            Ok(())
        },
    },
    Entry {
        key: "touch_layout",
        env: "TH08_TOUCH_LAYOUT",
        kind: Kind::Text("L:name,x,y,w,h,opacity,visible;…|P:…"),
        default: "",
        group: Group::Touch,
        label: "Overlay layout",
        summary: "Button positions from the layout editor",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Player,
        platforms: Platforms::NONE,
        persist: true,
        help: "overlay layout per orientation (empty = default; written by the layout editor)",
        set: |s, v| {
            s.touch.layout = crate::platform::touch::TouchLayout::parse(v)?;
            Ok(())
        },
    },
    Entry {
        key: "save_dir",
        env: "TH08_SAVE_DIR",
        kind: Kind::Path,
        default: "",
        group: Group::GameData,
        label: "Save folder",
        summary: "Where scores and replays are saved",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "where score.dat and replays are saved (default: the port config dir)",
        set: |s, v| {
            s.save_dir = Some(PathBuf::from(v));
            Ok(())
        },
    },
    Entry {
        key: "font",
        env: "TH08_FONT",
        kind: Kind::File,
        default: "",
        group: Group::GameData,
        label: "Dialogue font",
        summary: "Japanese font for dialogue text",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "Japanese font for dialogue text (native)",
        set: |s, v| {
            s.font = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "pack",
        env: "TH08_PACK",
        kind: Kind::Path,
        default: "",
        group: Group::Tools,
        label: "Presentation pack",
        summary: "Translation pack folder or zip",
        unit: "",
        words: &[("", "By game text language")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP,
        persist: false,
        help: "presentation pack to mount: a folder or .zip, or a pack id in <game folder>/packs or \
               <config folder>/packs; empty = the pack of the game text language (texts, fonts, images only)",
        set: |s, v| {
            s.pack = (!v.trim().is_empty()).then(|| v.trim().to_string());
            Ok(())
        },
    },
    Entry {
        key: "soundfont",
        env: "TH08_SOUNDFONT",
        kind: Kind::File,
        default: "",
        group: Group::GameData,
        label: "MIDI SoundFont",
        summary: "SoundFont for MIDI music",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Player,
        platforms: Platforms::DESKTOP,
        persist: true,
        help: "General MIDI SoundFont (.sf2) for music mode MIDI (native; default: system soundfonts dirs)",
        set: |s, v| {
            s.soundfont = Some(PathBuf::from(v));
            Ok(())
        },
    },
    Entry {
        key: "seed",
        env: "TH08_SEED",
        kind: Kind::Int { min: 0, max: 65535 },
        default: "",
        group: Group::StartGame,
        label: "RNG seed",
        summary: "Fixed random seed at boot",
        unit: "",
        words: &[("", "Random")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "fixed boot RNG seed (0-65535)",
        set: |s, v| {
            s.seed = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "stage",
        env: "TH08_STAGE",
        kind: Kind::Enum(&["1", "2", "3", "4a", "4b", "5", "6a", "6b", "ex"]),
        default: "",
        group: Group::StartGame,
        label: "Start at stage",
        summary: "Skip the menus and start at a stage",
        unit: "",
        words: &[
            ("", "Off"),
            ("1", "Stage 1"),
            ("2", "Stage 2"),
            ("3", "Stage 3"),
            ("4a", "Stage 4A"),
            ("4b", "Stage 4B"),
            ("5", "Stage 5"),
            ("6a", "Final A"),
            ("6b", "Final B"),
            ("ex", "Extra"),
        ],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "start a game at that stage (also stageN[.std], idx:N)",
        set: |s, v| {
            s.stage = Some(th08_core::stage::parse_stage_arg(v).ok_or("unknown stage")?);
            Ok(())
        },
    },
    Entry {
        key: "stage_frame",
        env: "TH08_STAGE_FRAME",
        kind: Kind::Int { min: 0, max: U32 },
        default: "0",
        group: Group::StartGame,
        label: "Stage start frame",
        summary: "Skip ahead into the stage",
        unit: " frames",
        words: &[("0", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "fast-forward the stage n frames on its first calc",
        set: |s, v| {
            s.stage_frame = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "player",
        env: "TH08_PLAYER",
        kind: Kind::Enum(&[
            "reimu&yukari",
            "marisa&alice",
            "sakuya&remilia",
            "youmu&yuyuko",
            "reimu",
            "yukari",
            "marisa",
            "alice",
            "sakuya",
            "remilia",
            "youmu",
            "yuyuko",
        ]),
        default: "",
        group: Group::StartGame,
        label: "Shot type",
        summary: "Shot type for a direct stage or ending start",
        unit: "",
        words: &[
            ("", "Default"),
            ("reimu&yukari", "Reimu & Yukari"),
            ("marisa&alice", "Marisa & Alice"),
            ("sakuya&remilia", "Sakuya & Remilia"),
            ("youmu&yuyuko", "Youmu & Yuyuko"),
            ("reimu", "Reimu"),
            ("yukari", "Yukari"),
            ("marisa", "Marisa"),
            ("alice", "Alice"),
            ("sakuya", "Sakuya"),
            ("remilia", "Remilia"),
            ("youmu", "Youmu"),
            ("yuyuko", "Yuyuko"),
        ],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "shot type 0-11 or a name (reimu&yukari, ry, marisa, ...) for --stage/--ending",
        set: |s, v| {
            s.player = Some(th08_core::player::parse_shot(v).ok_or("unknown shot type")?);
            Ok(())
        },
    },
    Entry {
        key: "rank",
        env: "TH08_RANK",
        kind: Kind::Enum(&["easy", "normal", "hard", "lunatic", "extra"]),
        default: "",
        group: Group::StartGame,
        label: "Difficulty",
        summary: "Difficulty for a direct stage or ending start",
        unit: "",
        words: &[
            ("", "Default"),
            ("easy", "Easy"),
            ("normal", "Normal"),
            ("hard", "Hard"),
            ("lunatic", "Lunatic"),
            ("extra", "Extra"),
        ],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "difficulty for --stage/--ending (also e/n/h/l/x, 0-4)",
        set: |s, v| {
            s.rank = Some(th08_core::enemy::parse_difficulty(v).ok_or("unknown difficulty")?);
            Ok(())
        },
    },
    Entry {
        key: "power",
        env: "TH08_POWER",
        kind: Kind::Int { min: 0, max: 128 },
        default: "",
        group: Group::StartGame,
        label: "Starting power",
        summary: "Player power at the first stage",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "player power at the first stage",
        set: |s, v| {
            s.power = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "continues",
        env: "TH08_CONTINUES",
        kind: Kind::Int { min: 0, max: 255 },
        default: "0",
        group: Group::StartGame,
        label: "Continues used",
        summary: "Continues counted when an ending starts",
        unit: "",
        words: &[("0", "None")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "continues used, for --ending",
        set: |s, v| {
            s.continues = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "ending",
        env: "TH08_ENDING",
        kind: Kind::Text("<shot><a|b|c> | FILE.end"),
        default: "",
        group: Group::StartGame,
        label: "Play ending",
        summary: "Watch an ending directly",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "start an ending: <shot><a|b|c> (bad / good 6A / good 6B) or a .end file",
        set: |s, v| {
            s.ending = Some(parse_ending(v)?);
            Ok(())
        },
    },
    Entry {
        key: "replay",
        env: "TH08_REPLAY",
        kind: Kind::Text("FILE | demo0-3"),
        default: "",
        group: Group::StartGame,
        label: "Play replay",
        summary: "Play a replay file or a demo",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "play a replay file or an attract demo",
        set: |s, v| {
            s.replay = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "replay_speed",
        env: "TH08_REPLAY_SPEED",
        kind: Kind::Int { min: 0, max: 2 },
        default: "0",
        group: Group::StartGame,
        label: "Replay speed",
        summary: "Playback speed of that replay",
        unit: "",
        words: &[("0", "Normal"), ("1", "Slowdown"), ("2", "Fast")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "replay playback speed",
        set: |s, v| {
            s.replay_speed = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "replay_rate",
        env: "TH08_REPLAY_RATE",
        kind: Kind::Enum(&["1/4", "1/2", "3/4", "1", "2", "4", "8"]),
        default: "1",
        group: Group::StartGame,
        label: "Replay rate",
        summary: "Port playback rate a replay starts at",
        unit: "×",
        words: &[
            ("1/4", "¼×"),
            ("1/2", "½×"),
            ("3/4", "¾×"),
            ("1", "1×"),
            ("2", "2×"),
            ("4", "4×"),
            ("8", "8×"),
        ],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "port replay transport rate at the start of playback (F7 / F8 change it while it plays)",
        set: |s, v| {
            let r =
                crate::transport::RATES
                    .iter()
                    .position(|&(n, d)| match v.trim().split_once('/') {
                        Some((a, b)) => a == n.to_string() && b == d.to_string(),
                        None => d == 1 && v.trim() == n.to_string(),
                    });
            s.replay_rate = r.ok_or("expected 1/4, 1/2, 3/4, 1, 2, 4 or 8")?;
            Ok(())
        },
    },
    Entry {
        key: "replay_save",
        env: "TH08_REPLAY_SAVE",
        kind: Kind::Int { min: 1, max: 15 },
        default: "",
        group: Group::Tools,
        label: "Save replay to slot",
        summary: "Save the recording when the game ends",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "save the recording to that replay slot when the game ends",
        set: |s, v| {
            s.replay_save = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "input",
        env: "TH08_INPUT",
        kind: Kind::Text("FILE | frames:mask,..."),
        default: "",
        group: Group::Tools,
        label: "Input script",
        summary: "Scripted button presses",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "input script file or inline `frames:mask,...`",
        set: |s, v| {
            s.input = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "invuln",
        env: "TH08_INVULN",
        kind: Kind::Bool,
        default: "off",
        group: Group::Cheats,
        label: "Invulnerable",
        summary: "The player can't die",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "the player can't die",
        set: |s, v| {
            s.invuln = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "unlock",
        env: "TH08_UNLOCK",
        kind: Kind::Bool,
        default: "off",
        group: Group::Cheats,
        label: "Unlock everything",
        summary: "Unlock everything in the menus",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "unlock everything in the menus",
        set: |s, v| {
            s.unlock = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "enemy_dmg",
        env: "TH08_ENEMY_DMG",
        kind: Kind::Int { min: 0, max: I32 },
        default: "0",
        group: Group::Cheats,
        label: "Enemy damage per frame",
        summary: "Damage dealt to every enemy each frame",
        unit: "",
        words: &[("0", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "damage per frame to every on-screen shootable enemy",
        set: |s, v| {
            s.enemy_dmg = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "bomb",
        env: "TH08_BOMB",
        kind: Kind::Int { min: 0, max: 4 },
        default: "",
        group: Group::Cheats,
        label: "Force bomb",
        summary: "Use this bomb for every shot type",
        unit: "",
        words: &[("", "Off"), ("4", "Dissolve spell")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "force the bomb index (4 = dissolve spell)",
        set: |s, v| {
            let i: usize = num(v)?;
            s.bomb = Some((i <= 4).then_some(i).ok_or("expected 0-4")?);
            Ok(())
        },
    },
    Entry {
        key: "clrd",
        env: "TH08_CLRD",
        kind: Kind::Text("SHOT,ANY[,NC]"),
        default: "",
        group: Group::Cheats,
        label: "Clear masks",
        summary: "Mark a shot type as cleared",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "set that shot's clear masks on every difficulty",
        set: |s, v| {
            let m = v.split(',').map(mask).collect::<Result<Vec<_>, _>>()?;
            s.clrd = Some(
                (m.len() >= 2)
                    .then_some(m)
                    .ok_or("expected <shot>,<any>[,<nc>]")?,
            );
            Ok(())
        },
    },
    Entry {
        key: "screen_fx",
        env: "TH08_SCREEN_FX",
        kind: Kind::Text("F:T,P0,P1,P2,P3"),
        default: "",
        group: Group::Cheats,
        label: "Screen effect",
        summary: "Play a screen effect at a frame",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::ALL,
        persist: false,
        help: "queue screen effect T at game frame F",
        set: |s, v| {
            let n: Vec<i64> = v.split([':', ',']).map(num).collect::<Result<_, _>>()?;
            s.screen_fx = Some(n.try_into().map_err(|_| "expected 6 numbers")?);
            Ok(())
        },
    },
    Entry {
        key: "shot",
        env: "TH08_SHOT",
        kind: Kind::Int { min: 0, max: I64 },
        default: "",
        group: Group::Tools,
        label: "Screenshot at frame",
        summary: "Save a screenshot at a frame and quit",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "save /tmp/th08_shot.png at that frame and exit (web: freeze)",
        set: |s, v| {
            s.shot = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "shot_size",
        env: "TH08_SHOT_SIZE",
        kind: Kind::Text("WxH"),
        default: "",
        group: Group::Tools,
        label: "Screenshot size",
        summary: "Size of that screenshot",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "send --shot through the window's scaling blit at that size",
        set: |s, v| {
            s.shot_size = Some(pair(v, 'x')?);
            Ok(())
        },
    },
    Entry {
        key: "touch_mouse",
        env: "TH08_TOUCH_MOUSE",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Mouse as touch",
        summary: "Use the mouse as a finger",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP,
        persist: false,
        help: "the mouse acts as one finger and the touch overlay is shown (test touch without a phone)",
        set: |s, v| {
            s.touch.mouse = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "turbo",
        env: "TH08_TURBO",
        kind: Kind::Int { min: 0, max: U32 },
        default: "0",
        group: Group::Tools,
        label: "Turbo",
        summary: "Logic frames per drawn frame",
        unit: "",
        words: &[("0", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "N logic frames per drawn frame, no frame pacing",
        set: |s, v| {
            s.turbo = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "present_hz",
        env: "TH08_PRESENT_HZ",
        kind: Kind::Int { min: 1, max: 240 },
        default: "",
        group: Group::Tools,
        label: "Simulated display",
        summary: "Present as on a display of this refresh rate",
        unit: " Hz",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "each present waits for the next tick of an N Hz grid (test pacing on slow displays); pace is logged",
        set: |s, v| {
            s.present_hz = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "slot_test",
        env: "TH08_SLOT_TEST",
        kind: Kind::Int { min: 0, max: I32 },
        default: "",
        group: Group::Tools,
        label: "ANM slot test",
        summary: "Show one sprite sheet in a grid",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "draw that ANM slot's sprites in a grid",
        set: |s, v| {
            s.slot_test = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "anm",
        env: "TH08_ANM",
        kind: Kind::Text("FILE:SCRIPTS[:pf]"),
        default: "",
        group: Group::Tools,
        label: "ANM viewer",
        summary: "Run animation scripts instead of the game",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "run ANM scripts live instead of the game",
        set: |s, v| {
            s.anm = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "anm_int",
        env: "TH08_ANM_INT",
        kind: Kind::Text("F:ID,..."),
        default: "",
        group: Group::Tools,
        label: "ANM interrupts",
        summary: "Animation interrupts at frames",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "--anm interrupts at those frames",
        set: |s, v| {
            s.anm_int = v
                .split(',')
                .map(|p| pair(p, ':'))
                .collect::<Result<_, _>>()?;
            Ok(())
        },
    },
    Entry {
        key: "anm_pos",
        env: "TH08_ANM_POS",
        kind: Kind::Text("X,Y[,DX]"),
        default: "",
        group: Group::Tools,
        label: "ANM positions",
        summary: "Where the animations are drawn",
        unit: "",
        words: &[("", "Default")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "--anm VM i at (x + i*dx, y)",
        set: |s, v| {
            s.anm_pos = list(v)?;
            Ok(())
        },
    },
    Entry {
        key: "anm_mode",
        env: "TH08_ANM_MODE",
        kind: Kind::Int {
            min: I32_MIN,
            max: I32,
        },
        default: "",
        group: Group::Tools,
        label: "ANM draw mode",
        summary: "Forced draw mode for the animations",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "--anm forced draw mode",
        set: |s, v| {
            s.anm_mode = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "se_log",
        env: "TH08_SE_LOG",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Log sound effects",
        summary: "Log the sound effects of each frame",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "log each frame's flushed sound effect ids",
        set: |s, v| {
            s.se_log = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "drawlog",
        env: "TH08_DRAWLOG",
        kind: Kind::Text("PATH[,A-B | ,dA-B]"),
        default: "",
        group: Group::Tools,
        label: "Draw log",
        summary: "Write a text log of every draw",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::DESKTOP_WEB,
        persist: false,
        help: "write a plain-text draw log: <path>[,from-to] (.xz logs: session / drawrun tools)",
        set: |s, v| {
            s.drawlog = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "result_shots",
        env: "TH08_RESULT_SHOTS",
        kind: Kind::Text("K,..."),
        default: "",
        group: Group::Tools,
        label: "Result screenshots",
        summary: "Result screen screenshots for tests",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "tests: result-screen screenshots at those frames",
        set: |s, v| {
            s.result_shots = list(v)?;
            Ok(())
        },
    },
    Entry {
        key: "test_frames",
        env: "TH08_TEST_FRAMES",
        kind: Kind::Int { min: 0, max: U32 },
        default: "4000",
        group: Group::Tools,
        label: "Test frames per shot",
        summary: "Frames per shot type in the player test",
        unit: " frames",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "tests: frames per shot type in the scripted player test",
        set: |s, v| {
            s.test_frames = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "session_rng",
        env: "TH08_SESSION_RNG",
        kind: Kind::Int {
            min: I64_MIN,
            max: I64,
        },
        default: "",
        group: Group::Tools,
        label: "Session RNG sites",
        summary: "Log random calls of a menu frame",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "session tool: RNG call sites of that menu frame",
        set: |s, v| {
            s.session_rng = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "session_dump",
        env: "TH08_SESSION_DUMP",
        kind: Kind::Text("A-B"),
        default: "",
        group: Group::Tools,
        label: "Session dump",
        summary: "Log player and items over menu frames",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "session tool: player and items after each calc of menu frames A..B",
        set: |s, v| {
            s.session_dump = Some(pair(v, '-')?);
            Ok(())
        },
    },
    Entry {
        key: "session_pb",
        env: "TH08_SESSION_PB",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Session player bullets",
        summary: "Also log player bullets",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "session tool: also dump player bullets",
        set: |s, v| {
            s.session_pb = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "bdump",
        env: "TH08_BDUMP",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Bullet dump",
        summary: "Log enemies and bullets each frame",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "demosync tool: enemies and bullets per frame",
        set: |s, v| {
            s.bdump = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "idump",
        env: "TH08_IDUMP",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Item dump",
        summary: "Log player and items each frame",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "demosync tool: player and items per frame",
        set: |s, v| {
            s.idump = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "edump",
        env: "TH08_EDUMP",
        kind: Kind::Int { min: 0, max: 255 },
        default: "",
        group: Group::Tools,
        label: "Effect dump",
        summary: "Log effects of one type each frame",
        unit: "",
        words: &[("", "Off")],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "demosync tool: effects of that type per frame",
        set: |s, v| {
            s.edump = Some(num(v)?);
            Ok(())
        },
    },
    Entry {
        key: "sht_dump",
        env: "SHT_DUMP",
        kind: Kind::Bool,
        default: "off",
        group: Group::Tools,
        label: "Shot file dump",
        summary: "Print every parsed shot file",
        unit: "",
        words: &[],
        audience: Audience::Dev,
        platforms: Platforms::NATIVE,
        persist: false,
        help: "tests: print every parsed .sht file",
        set: |s, v| {
            s.sht_dump = parse_bool(v)?;
            Ok(())
        },
    },
];

pub fn find(name: &str) -> Option<&'static Entry> {
    let n = name.replace('-', "_").to_ascii_lowercase();
    ENTRIES.iter().find(|s| s.key == n || s.url() == n)
}

impl Settings {
    // One value from `origin`; a bad value is logged and the previous one kept. Empty = unset, except booleans (off).
    fn apply(&mut self, origin: Origin, name: &str, sw: &Entry, v: &str) {
        let v = match v.trim() {
            "" if sw.is_bool() => "off",
            "" => return,
            v => v,
        };
        match sw.parse_into(self, v) {
            Ok(()) => {
                self.values.insert(sw.key, (v.to_string(), origin));
            }
            Err(e) if origin == Origin::PortCfg => {
                log::warn!("{} {name}={v}: {e}; ignored", origin_name(origin));
            }
            Err(e) => log::error!("{} {name}={v}: {e}; ignored", origin_name(origin)),
        }
    }

    // Sets one entry by key; the caller saves it with `save_port_cfg` if wanted.
    pub fn set_from(&mut self, origin: Origin, key: &str, v: &str) -> Result<(), String> {
        let e = find(key).ok_or_else(|| format!("unknown setting {key}"))?;
        e.parse_into(self, v.trim())?;
        self.values.insert(e.key, (v.trim().to_string(), origin));
        Ok(())
    }

    // Current value text of an entry (None = unset).
    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|v| v.0.as_str())
    }

    pub fn origin(&self, key: &str) -> Origin {
        self.values.get(key).map_or(Origin::Default, |v| v.1)
    }

    // Native: CLI args (without the program name) parsed by the clap command built from `ENTRIES`. `Err` = clap's
    // help/version output or a parse error (`Error::exit` prints it with the right exit code).
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    pub fn from_args(
        dirs: PlatformDirs,
        args: impl IntoIterator<Item = String>,
    ) -> Result<Self, clap::Error> {
        Self::from_args_over(Self::from_environment(dirs), args)
    }

    // The `th08` game binary: as `from_args`, with port.cfg under env and command line. A tool run (`is_tool_run`)
    // is parsed again without port.cfg and with the tool dirs, as the headless tools are.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    pub fn game_from_args(
        dirs: PlatformDirs,
        args: impl IntoIterator<Item = String>,
    ) -> Result<Self, clap::Error> {
        let args: Vec<String> = args.into_iter().collect();
        let s = Self::from_args_over(Self::game_environment(dirs.clone()), args.clone())?;
        if !s.is_tool_run() {
            return Ok(s);
        }
        Self::from_args_over(Self::from_environment(dirs.tool()), args)
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn from_args_over(
        mut s: Self,
        args: impl IntoIterator<Item = String>,
    ) -> Result<Self, clap::Error> {
        let m = command(&s.dirs).try_get_matches_from(
            std::iter::once("th08".to_string()).chain(join_bool_values(args)),
        )?;
        // The positional folder first, so a `--game-dir` flag beats it.
        if let Some(v) = m.get_one::<String>(POS_DIR) {
            s.apply(Origin::Cli, "game dir", find("game_dir").unwrap(), v);
        }
        let mut cli: Vec<(usize, &'static Entry, String)> = Vec::new();
        for e in ENTRIES {
            if let (Some(i), Some(v)) = (m.index_of(e.key), m.get_one::<String>(e.key)) {
                cli.push((i, e, v.clone()));
            }
            // Only booleans have a `--no-x` arg; clap's debug build panics on an unknown id.
            if let Some(i) = e
                .is_bool()
                .then(|| m.index_of(&no_id(e)))
                .flatten()
                .filter(|_| m.get_flag(&no_id(e)))
            {
                cli.push((i, e, "off".into()));
            }
        }
        // Last on the command line wins (`--no-x --x`).
        cli.sort_by_key(|c| c.0);
        for (_, e, v) in cli {
            s.apply(Origin::Cli, e.key, e, &v);
        }
        Ok(s)
    }

    // Defaults, then each (entry, value text, origin); used by the launcher screen after Play.
    pub fn from_values<'a>(
        dirs: PlatformDirs,
        values: impl IntoIterator<Item = (&'static Entry, &'a str, Origin)>,
    ) -> Self {
        let mut s = Self {
            dirs,
            ..Self::default()
        };
        for (e, v, o) in values {
            s.apply(o, e.key, e, v);
        }
        s
    }

    // The switches the game core reads (`Ctx::core`); the rest stays host / tool side.
    pub fn core(&self) -> th08_core::coreconfig::CoreConfig {
        th08_core::coreconfig::CoreConfig {
            seed: self.seed,
            stage: self.stage,
            stage_frame: self.stage_frame,
            player: self.player,
            rank: self.rank,
            power: self.power,
            continues: self.continues,
            ending: self.ending.clone(),
            replay: self.replay.clone(),
            replay_speed: self.replay_speed,
            replay_save: self.replay_save,
            input: self.input.clone(),
            invuln: self.invuln,
            unlock: self.unlock,
            enemy_dmg: self.enemy_dmg,
            bomb: self.bomb,
            clrd: self.clrd.clone(),
            screen_fx: self.screen_fx,
            slot_test: self.slot_test,
            anm: self.anm.clone(),
            anm_int: self.anm_int.clone(),
            anm_pos: self.anm_pos.clone(),
            anm_mode: self.anm_mode,
            se_log: self.se_log,
            font: self.font.clone(),
        }
    }

    // Port save storage (`Ctx::host`): `TH08_SAVE_DIR`, else the entry point's save dir (desktop: config dir, else
    // next to the binary).
    pub fn storage(&self) -> crate::platform::Storage {
        #[cfg(not(target_arch = "wasm32"))]
        let dir = self.storage_dir();
        #[cfg(target_arch = "wasm32")]
        let dir = None;
        crate::platform::Storage::new(dir)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn storage_dir(&self) -> Option<PathBuf> {
        self.save_dir.clone().or_else(|| self.dirs.save.clone())
    }

    // The game window's host: winit keyboard, gilrs joystick, the save storage.
    pub fn app_host(&self) -> Box<dyn th08_core::host::Host> {
        Box::new(crate::platform::AppHost::new(
            self.storage(),
            self.touch.clone(),
            &self.keys,
            &self.pads,
        ))
    }

    // Audio output for `audio::Audio`: port volume, buffer size and the MIDI SoundFont lookup (`soundfont`, then the platform's soundfont dirs).
    pub fn audio(&self) -> th08_audio::audio::AudioConfig {
        th08_audio::audio::AudioConfig {
            soundfont: self.soundfont.clone(),
            soundfont_dirs: self.dirs.soundfonts.clone(),
            volume: self.volume,
            buffer: self.audio_buffer,
        }
    }

    // Tools and tests: the headless IO bundle for `Ctx::load_with`: `renderer`, a real `audio::Audio` over `source`
    // (its SoundFont probe sets `midi_ready` as in the game), `tool_host`, no draw log.
    pub fn tool_io(
        &self,
        renderer: impl th08_core::drawlist::Renderer + 'static,
        source: &th08_core::assets::Source,
    ) -> th08_core::game::Io {
        let mut renderer: Box<dyn th08_core::drawlist::Renderer> = Box::new(renderer);
        let overlay = self.mount_pack(renderer.as_mut());
        th08_core::game::Io {
            renderer,
            audio: Box::new(th08_audio::audio::Audio::new(source.clone(), self.audio())),
            host: self.tool_host(),
            drawlog: None,
            overlay,
        }
    }

    // The selected presentation pack (packs.rs) into the renderer (when it is a `Gfx`); its images for `Io::overlay`.
    pub fn mount_pack(
        &self,
        renderer: &mut dyn th08_core::drawlist::Renderer,
    ) -> Option<std::sync::Arc<th08_core::content::Overlay>> {
        let m = crate::packs::select(self)?;
        let overlay = m.overlay();
        if let Some(g) = renderer
            .as_any_mut()
            .downcast_mut::<th08_render::gfx::Gfx>()
        {
            g.set_pack(m.text);
        }
        overlay
    }

    // Tools and tests: no devices, scripted / recorded input (`platform::ToolHost`).
    pub fn tool_host(&self) -> Box<dyn th08_core::host::Host> {
        Box::new(crate::platform::ToolHost::new(self.storage()))
    }

    // Scripted / headless runs of the th08 binary (screenshot, draw log, input script, anm viewer, slot test):
    // never show the launcher.
    pub fn is_tool_run(&self) -> bool {
        self.shot.is_some()
            || self.drawlog.is_some()
            || self.input.is_some()
            || self.anm.is_some()
            || self.slot_test.is_some()
    }

    // Defaults < env vars / URL params: tests and tools never read the user's port.cfg / localStorage.
    pub fn from_environment(dirs: PlatformDirs) -> Self {
        Self {
            dirs,
            hermetic: true,
            ..Self::default()
        }
        .with_env()
    }

    // Whether the game loads and writes the save files (score.dat, th08.cfg, replays): a tool run only with an
    // explicit `save_dir`.
    pub fn uses_saves(&self) -> bool {
        !self.hermetic || self.save_dir.is_some()
    }

    // Cheats rows not at their default (keys); any one keeps score.dat and replays from being written.
    pub fn active_cheats(&self) -> Vec<&'static str> {
        [
            ("invuln", self.invuln),
            ("unlock", self.unlock),
            ("enemy_dmg", self.enemy_dmg != 0),
            ("bomb", self.bomb.is_some()),
            ("clrd", self.clrd.is_some()),
            ("screen_fx", self.screen_fx.is_some()),
        ]
        .into_iter()
        .filter_map(|(k, on)| on.then_some(k))
        .collect()
    }

    // The game (native binary, web build): defaults < port.cfg (web: localStorage) < env vars / URL params.
    pub fn game_environment(dirs: PlatformDirs) -> Self {
        let mut s = Self {
            dirs,
            ..Self::default()
        };
        for (k, v) in read_port_cfg(&s.dirs) {
            match find(&k) {
                // Developer rows are never saved; a stale or hand-written one would silently turn on a cheat.
                Some(sw) if sw.audience == Audience::Dev => {
                    log::warn!(
                        "port.cfg: developer setting {k} ignored (use --{k} or {})",
                        sw.env
                    );
                }
                Some(sw) => {
                    let v = cfg_path(sw, &v, s.dirs.config.as_deref());
                    s.apply(Origin::PortCfg, &k, sw, &v);
                }
                // A newer build's key: kept in the file for that build.
                None => log::info!("port.cfg: unknown key {k} ignored"),
            }
        }
        s.with_env()
    }

    fn with_env(mut self) -> Self {
        for sw in ENTRIES {
            if let Some(v) = env_value(sw) {
                self.apply(Origin::Env, sw.env, sw, &v);
            }
        }
        let cheats = self.active_cheats();
        if cheats.iter().any(|k| self.origin(k) == Origin::Env) {
            log::warn!(
                "{} cheats on: {}",
                origin_name(Origin::Env),
                cheats.join(", ")
            );
        }
        self
    }
}

fn origin_name(o: Origin) -> &'static str {
    match o {
        Origin::Default => "default",
        Origin::Bundled => "bundled:",
        Origin::PortCfg => "port.cfg:",
        Origin::Env if cfg!(target_arch = "wasm32") => "URL:",
        Origin::Env => "env:",
        Origin::Cli => "command line:",
        Origin::Launcher => "settings screen:",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn env_value(sw: &Entry) -> Option<String> {
    std::env::var(sw.env).ok()
}

// Web: `?stage_frame=600` is TH08_STAGE_FRAME.
#[cfg(target_arch = "wasm32")]
fn env_value(sw: &Entry) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(&sw.url())
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
const POS_DIR: &str = "game dir";

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn no_id(e: &Entry) -> String {
    format!("no-{}", e.key)
}

// Validates a CLI value with the entry's own table parser (kind range/choices plus aliases such as `idx:N`).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
#[derive(Clone)]
struct EntryParser(&'static Entry);

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl clap::builder::TypedValueParser for EntryParser {
    type Value = String;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        v: &std::ffi::OsStr,
    ) -> Result<String, clap::Error> {
        let v = v
            .to_str()
            .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8).with_cmd(cmd))?;
        self.0.check(v).map(|_| v.to_string()).map_err(|msg| {
            let name = arg.map_or(self.0.key.to_string(), ToString::to_string);
            clap::Error::raw(
                clap::error::ErrorKind::ValueValidation,
                format!("invalid value '{v}' for '{name}': {msg}\n"),
            )
            .with_cmd(cmd)
        })
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        match self.0.kind {
            Kind::Enum(c) if c.len() <= 9 => Some(Box::new(
                c.iter().map(|&v| clap::builder::PossibleValue::new(v)),
            )),
            _ => None,
        }
    }
}

// clap cannot let an optional flag value depend on the next word, so `--bool on` is joined to `--bool=on` first
// (a boolean takes the next word only if it is an on/off value; else `--bool` alone = on).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn join_bool_values(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut it = args.into_iter().peekable();
    while let Some(a) = it.next() {
        let is_bool_flag = a
            .strip_prefix("--")
            .filter(|f| !f.contains('='))
            .and_then(find)
            .is_some_and(Entry::is_bool);
        match it.peek() {
            Some(n) if is_bool_flag && !n.trim().is_empty() && parse_bool(n).is_ok() => {
                let n = it.next().unwrap();
                out.push(format!("{a}={n}"));
            }
            _ => out.push(a),
        }
    }
    out
}

// The `th08` command line, one flag per `ENTRIES` row (`--key`, `--k_e_y` and the URL name as aliases), grouped by
// `Group` in `--help`.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn command(dirs: &PlatformDirs) -> clap::Command {
    use clap::{Arg, ArgAction, Command};
    let mut cmd = Command::new("th08")
        .about("Touhou 8 (Imperishable Night) engine port")
        .args_override_self(true)
        .after_help(format!(
            "Each switch is also an env var (shown), a web URL parameter (name without TH08_, lowercase) and a\n\
             `key = value` line in port.cfg. Priority: command line > env > port.cfg > default.\n\
             Booleans take 1/0, true/false or on/off; `--switch` alone means on, `--no-switch` off.\n\
             The launcher screen turned off (launcher = off): hold Shift while th08 starts to show it again.\n\
             In the game: F1 port menu, F11 or Alt+Enter fullscreen, F3 hitbox viewer (off → player → all). While a\n\
             replay plays: F5 pause / resume, F6 one frame (pauses), F7 slower, F8 faster (¼× ½× 1× 2× 4× 8×).\n\
             port.cfg: {}{}",
            dirs.port_cfg().map_or("(no config dir)".into(), |p| p.display().to_string()),
            if cfg!(debug_assertions) {
                "\nDebug build: TH08_DEVICE_LOSS=<n> destroys the GPU device at the n-th present (recovery test)."
            } else {
                ""
            }
        ))
        .arg(
            Arg::new(POS_DIR)
                .value_name("GAME DIR")
                .help("game folder (same as --game-dir)")
                .value_parser(EntryParser(find("game_dir").unwrap())),
        );
    for e in ENTRIES {
        let long = e.key.replace('_', "-");
        let def = match e.default {
            "" => String::new(),
            d => format!(", default {d}"),
        };
        let mut arg = Arg::new(e.key)
            .long(long.clone())
            .help(format!("{} ({}{def})", e.help, e.env))
            .help_heading(e.group.title())
            .action(ArgAction::Set)
            .value_parser(EntryParser(e));
        let mut aliases = vec![e.key.to_string(), e.url(), e.url().replace('_', "-")];
        aliases.sort();
        aliases.dedup();
        for a in aliases.into_iter().filter(|a| *a != long) {
            arg = arg.alias(a);
        }
        arg = match e.kind {
            Kind::Bool => arg
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("on")
                .value_name("on|off"),
            Kind::Enum(_) => arg.value_name(e.key.to_uppercase()),
            _ => arg.value_name(e.hint()),
        };
        cmd = cmd.arg(arg);
        if e.is_bool() {
            cmd = cmd.arg(
                Arg::new(no_id(e))
                    .long(format!("no-{long}"))
                    .alias(format!("no_{}", e.key))
                    .action(ArgAction::SetTrue)
                    .hide(true),
            );
        }
    }
    cmd
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

// Defaults + env / URL, never port.cfg, with the tool dirs: no config dir, saves only in an explicit `save_dir`.
// Used by tools and tests only; the entry points pass their `Settings` down.
pub fn get() -> &'static Settings {
    SETTINGS.get_or_init(|| Settings::from_environment(default_dirs().tool()))
}

#[cfg(not(target_arch = "wasm32"))]
fn default_dirs() -> PlatformDirs {
    crate::launcher::desktop_dirs()
}

#[cfg(target_arch = "wasm32")]
fn default_dirs() -> PlatformDirs {
    PlatformDirs::default()
}

// Tests and tools: the game directory (TH08_DIR only).
pub fn test_game_dir() -> Option<PathBuf> {
    get().game_dir.clone()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

// `key = value` lines; `#` comments and blank lines skipped.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn parse_port_cfg(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            if let Some((k, v)) = l.split_once('=') {
                Some((k.trim().to_string(), v.trim().to_string()))
            } else {
                log::warn!("port.cfg: line {l:?} is not `key = value`; ignored");
                None
            }
        })
        .collect()
}

// port.cfg as text: None = no file; a non-UTF-8 file is decoded lossily (Ok(Some((text, false)))).
#[cfg(not(target_arch = "wasm32"))]
fn read_cfg_text(path: &std::path::Path) -> Result<Option<(String, bool)>, String> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(match String::from_utf8(b) {
            Ok(t) => (t, true),
            Err(e) => {
                log::warn!(
                    "{}: not UTF-8; read with the bad bytes replaced",
                    path.display()
                );
                (String::from_utf8_lossy(e.as_bytes()).into_owned(), false)
            }
        })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_port_cfg(dirs: &PlatformDirs) -> Vec<(String, String)> {
    let Some(p) = dirs.port_cfg() else {
        return Vec::new();
    };
    match read_cfg_text(&p) {
        Ok(t) => t.map(|(t, _)| parse_port_cfg(&t)).unwrap_or_default(),
        Err(e) => {
            log::warn!("{e}; settings from port.cfg not applied");
            Vec::new()
        }
    }
}

// Web: the port.cfg slot is localStorage, one `th08.<key>` item per setting.
#[cfg(target_arch = "wasm32")]
fn read_port_cfg(_: &PlatformDirs) -> Vec<(String, String)> {
    let Some(st) = local_storage() else {
        return Vec::new();
    };
    ENTRIES
        .iter()
        .filter_map(|e| Some((e.key.to_string(), st.get_item(&storage_key(e.key)).ok()??)))
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn storage_key(key: &str) -> String {
    format!("th08.{key}")
}

// Sets `key = value` in port.cfg (other lines kept, the key appended if new). Called only after a folder pick or
// an interactive setting change.
pub fn save_port_cfg(dirs: &PlatformDirs, key: &str, value: &str) {
    write_port_cfg(dirs, &[(key, Some(value))]);
}

// Sets (Some) or removes (None) port.cfg keys in one write; web: localStorage items.
pub fn write_port_cfg(dirs: &PlatformDirs, changes: &[(&str, Option<&str>)]) {
    if let Err(e) = try_write_port_cfg(dirs, changes) {
        log::error!("{e}");
        if cfg!(target_arch = "wasm32") || dirs.config.is_some() {
            crate::crashlog::save_failed("port.cfg", &e);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn try_write_port_cfg(
    dirs: &PlatformDirs,
    changes: &[(&str, Option<&str>)],
) -> Result<(), String> {
    let path = dirs
        .port_cfg()
        .ok_or("no config directory; port.cfg not saved")?;
    let (old, utf8) = read_cfg_text(&path)?.unwrap_or((String::new(), true));
    let mut text = drop_dev_keys(&old);
    for &(key, value) in changes {
        match find(key) {
            Some(e) => text = set_cfg_line(&text, e.key, value),
            None => log::error!("port.cfg: unknown key {key} not saved"),
        }
    }
    if text == old {
        return Ok(());
    }
    if !utf8 {
        // The original bytes survive the rewrite as port.cfg.bak.
        let bak = path.with_extension("cfg.bak");
        let raw = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        crate::platform::write_atomic(&bak, &raw).map_err(|e| format!("{}: {e}", bak.display()))?;
        log::warn!(
            "{}: not UTF-8; the original is kept as {}",
            path.display(),
            bak.display()
        );
    }
    crate::platform::write_atomic(&path, text.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(target_arch = "wasm32")]
pub fn try_write_port_cfg(
    _: &PlatformDirs,
    changes: &[(&str, Option<&str>)],
) -> Result<(), String> {
    let st = local_storage().ok_or("no localStorage; settings not saved")?;
    let mut failed = Vec::new();
    for &(key, value) in changes {
        let Some(e) = find(key) else {
            log::error!("localStorage: unknown key {key} not saved");
            continue;
        };
        let res = match value {
            Some(v) => st.set_item(&storage_key(e.key), v),
            None => st.remove_item(&storage_key(e.key)),
        };
        if res.is_err() {
            failed.push(key);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("localStorage: {} not saved", failed.join(", ")))
    }
}

// The saved player rows (port.cfg, web: localStorage) as `key = value` lines in table order; the last line of a key
// wins, as when it is read.
pub fn player_port_cfg(dirs: &PlatformDirs) -> String {
    let saved = read_port_cfg(dirs);
    ENTRIES
        .iter()
        .filter(|e| e.audience == Audience::Player)
        .filter_map(|e| {
            let (_, v) = saved.iter().rev().find(|(k, _)| k == e.key)?;
            Some(format!("{} = {v}\n", e.key))
        })
        .collect()
}

// A port.cfg path relative to the config dir, as the file may be read from any working directory; CLI and env paths
// stay relative to the working directory.
fn cfg_path(e: &Entry, v: &str, config: Option<&std::path::Path>) -> String {
    let rel = !v.trim().is_empty() && std::path::Path::new(v.trim()).is_relative();
    match config {
        Some(c) if rel && matches!(e.kind, Kind::Path | Kind::File) => {
            c.join(v.trim()).to_string_lossy().into_owned()
        }
        _ => v.to_string(),
    }
}

// port.cfg text without developer rows: the game ignores them there, so every save drops them.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn drop_dev_keys(text: &str) -> String {
    let dev: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| Some(l.split_once('=')?.0.trim()))
        .filter(|k| find(k).is_some_and(|e| e.audience == Audience::Dev))
        .collect();
    dev.iter()
        .fold(text.to_string(), |t, k| set_cfg_line(&t, k, None))
}

// port.cfg text with `key = value` replaced in place, appended (new key) or removed (None); other lines (comments
// too) kept.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn set_cfg_line(old: &str, key: &str, value: Option<&str>) -> String {
    let mut found = false;
    let is_key = |l: &str| {
        !l.trim_start().starts_with('#') && l.split_once('=').is_some_and(|(k, _)| k.trim() == key)
    };
    let mut out: Vec<String> = Vec::new();
    for l in old.lines() {
        if !is_key(l) {
            out.push(l.to_string());
        } else if let Some(v) = value.filter(|_| !found) {
            found = true;
            out.push(format!("{key} = {v}"));
        }
    }
    if let (false, Some(v)) = (found, value) {
        out.push(format!("{key} = {v}"));
    }
    if out.is_empty() {
        return String::new();
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Settings {
        Settings::from_args(PlatformDirs::default(), a.iter().map(ToString::to_string)).unwrap()
    }

    #[test]
    fn cli_flags() {
        let s = args(&[
            "/g",
            "--stage",
            "4a",
            "--player=3",
            "--invuln",
            "--unlock",
            "off",
            "--shot-size",
            "800x600",
            "--ending",
            "7c",
        ]);
        assert_eq!(s.game_dir, Some(PathBuf::from("/g")));
        assert_eq!(s.origin("game_dir"), Origin::Cli);
        assert_eq!(s.stage, Some(3));
        assert_eq!(s.player, Some(3));
        assert!(s.invuln && !s.unlock);
        assert_eq!(s.shot_size, Some((800, 600)));
        assert_eq!(s.ending, Some(Ending::Kind { shot: 7, kind: 'c' }));
        assert!(Settings::from_args(PlatformDirs::default(), ["--help".to_string()]).is_err());
        let bad = |a: &[&str]| {
            Settings::from_args(PlatformDirs::default(), a.iter().map(ToString::to_string)).is_err()
        };
        assert!(
            bad(&["--scale", "9"])
                && bad(&["--bogus"])
                && bad(&["--stage"])
                && bad(&["--stage", "9z"])
        );
        let s = args(&[
            "--se_log",
            "--stage_frame=60",
            "--dir",
            "/d",
            "--game-dir",
            "/e",
        ]);
        assert!(s.se_log && s.stage_frame == 60 && s.game_dir == Some(PathBuf::from("/e")));
        assert!(
            args(&["--fullscreen=on", "--vsync", "0", "/g"])
                .game_dir
                .is_some()
        );
        assert!(!args(&["--vsync", "0"]).vsync && args(&["--fullscreen", "/g"]).fullscreen);
        assert!(!args(&["--no-launcher"]).launcher);
        assert!(args(&["--no-launcher", "--launcher"]).launcher);
        assert_eq!(args(&["--no-launcher"]).origin("launcher"), Origin::Cli);
    }

    #[test]
    fn values() {
        let mut s = Settings::default();
        let set = |s: &mut Settings, k: &str, v: &str| s.apply(Origin::Cli, k, find(k).unwrap(), v);
        set(&mut s, "screen_fx", "100:3,1,2,3,-4");
        assert_eq!(s.screen_fx, Some([100, 3, 1, 2, 3, -4]));
        set(&mut s, "clrd", "3,0x8000");
        assert_eq!(s.clrd, Some(vec![3, 0x8000]));
        set(&mut s, "anm_int", "10:2,20:3");
        assert_eq!(s.anm_int, vec![(10, 2), (20, 3)]);
        set(&mut s, "turbo", "x");
        assert_eq!(s.turbo, 0);
        set(&mut s, "se_log", "on");
        set(&mut s, "se_log", "");
        assert!(!s.se_log);
        assert_eq!(s.value("se_log"), Some("off"));
        assert!(find("stage-frame").is_some() && find("dir").is_some());
    }

    // Only explicit truthy text turns a switch on; empty and the off words are off.
    #[test]
    fn bool_values() {
        for v in ["1", "true", "On", " yes "] {
            assert_eq!(parse_bool(v), Ok(true), "{v:?}");
        }
        for v in ["", " ", "0", "false", "OFF", "no"] {
            assert_eq!(parse_bool(v), Ok(false), "{v:?}");
        }
        assert!(parse_bool("2").is_err() && parse_bool("enabled").is_err());
        for e in ENTRIES.iter().filter(|e| e.is_bool()) {
            let mut s = Settings::default();
            s.apply(Origin::Env, e.env, e, "on");
            s.apply(Origin::Env, e.env, e, "");
            assert_eq!(s.value(e.key), Some("off"), "{}", e.key);
            assert_eq!(s.origin(e.key), Origin::Env);
        }
        let s = args(&["--invuln", "--unlock="]);
        assert!(s.invuln && !s.unlock);
    }

    // `active_cheats` covers exactly the Cheats rows, each only when off its default.
    #[test]
    fn cheat_rows() {
        let mut s = Settings::default();
        assert!(s.active_cheats().is_empty() && s.uses_saves());
        let cheats: Vec<&str> = ENTRIES
            .iter()
            .filter(|e| e.group == Group::Cheats)
            .map(|e| e.key)
            .collect();
        let on = [
            ("invuln", "1"),
            ("unlock", "on"),
            ("enemy_dmg", "5"),
            ("bomb", "4"),
            ("clrd", "3,0x8000"),
            ("screen_fx", "1:2,0,0,0,0"),
        ];
        assert_eq!(on.map(|c| c.0).to_vec(), cheats);
        for (k, v) in on {
            let mut one = Settings::default();
            one.set_from(Origin::Env, k, v).unwrap();
            assert_eq!(one.active_cheats(), [k]);
            s.set_from(Origin::Cli, k, v).unwrap();
        }
        assert_eq!(s.active_cheats(), cheats);
        s.set_from(Origin::Cli, "invuln", "").unwrap();
        s.set_from(Origin::Cli, "enemy_dmg", "0").unwrap();
        assert!(
            !s.active_cheats().contains(&"invuln") && !s.active_cheats().contains(&"enemy_dmg")
        );
    }

    // A tool run of the game binary reads no port.cfg and has no config or save dir unless `save_dir` is given.
    #[test]
    fn tool_runs_are_hermetic() {
        let root = std::env::temp_dir().join(format!("th08-hermetic-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("port.cfg"), "scale = 3\nlanguage = ru\n").unwrap();
        let dirs = PlatformDirs {
            config: Some(root.clone()),
            save: Some(root.clone()),
            soundfonts: vec![root.join("sf")],
        };
        let game = |a: &[&str]| {
            Settings::game_from_args(dirs.clone(), a.iter().map(ToString::to_string)).unwrap()
        };
        let play = game(&["--stage", "1"]);
        assert!(!play.hermetic && play.uses_saves() && play.scale == 3);
        assert_eq!(play.storage_dir(), Some(root.clone()));
        let tool = game(&["--shot", "100", "--input", "x.txt"]);
        assert!(tool.hermetic && !tool.uses_saves());
        assert_eq!((tool.scale, tool.origin("scale")), (1, Origin::Default));
        assert_eq!((tool.dirs.config.clone(), tool.storage_dir()), (None, None));
        assert_eq!(tool.dirs.soundfonts, [root.join("sf")]);
        let saves = game(&["--shot", "100", "--save-dir", "/tmp/s"]);
        assert!(saves.uses_saves());
        assert_eq!(saves.storage_dir(), Some(PathBuf::from("/tmp/s")));
        assert_eq!(dirs.clone().tool().port_cfg(), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn from_values() {
        let e = |k| find(k).unwrap();
        let s = Settings::from_values(
            PlatformDirs::default(),
            [
                (e("stage"), "4a", Origin::Cli),
                (e("scale"), "3", Origin::PortCfg),
                (e("vsync"), "off", Origin::Cli),
            ],
        );
        assert_eq!((s.stage, s.scale, s.vsync), (Some(3), 3, false));
        assert_eq!(s.origin("scale"), Origin::PortCfg);
        assert_eq!(s.value("fullscreen"), Some("off"));
        assert!(e("power").check("129").is_err() && e("power").check("128").is_ok());
    }

    #[test]
    fn port_cfg_write() {
        let t = set_cfg_line("# keep\nscale = 2\n", "fullscreen", Some("on"));
        assert_eq!(t, "# keep\nscale = 2\nfullscreen = on\n");
        assert_eq!(
            set_cfg_line(&t, "scale", Some("3")),
            "# keep\nscale = 3\nfullscreen = on\n"
        );
        assert_eq!(set_cfg_line(&t, "scale", None), "# keep\nfullscreen = on\n");
    }

    // A Shift-JIS port.cfg still applies its ASCII lines, and the first rewrite keeps the original as port.cfg.bak.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn port_cfg_non_utf8_kept() {
        let d = std::env::temp_dir().join(format!("th08-portcfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let raw = b"# \x93\x8c\x95\xfb\ngame_dir = C:\\\x93\x8c\x95\xfb\nscale = 3\n".to_vec();
        std::fs::write(d.join("port.cfg"), &raw).unwrap();
        let dirs = PlatformDirs {
            config: Some(d.clone()),
            ..PlatformDirs::default()
        };
        assert!(read_port_cfg(&dirs).contains(&("scale".into(), "3".into())));
        try_write_port_cfg(&dirs, &[("fullscreen", Some("on"))]).unwrap();
        assert_eq!(std::fs::read(d.join("port.cfg.bak")).unwrap(), raw);
        let t = std::fs::read_to_string(d.join("port.cfg")).unwrap();
        assert!(
            t.contains("scale = 3\n")
                && t.contains("fullscreen = on\n")
                && t.contains("game_dir = ")
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    // Keyboard rows: port.cfg round trip into the key map; a bad key name is rejected and leaves the default.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn port_cfg_keys() {
        use winit::keyboard::KeyCode;
        let d = std::env::temp_dir().join(format!("th08-portcfg-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let dirs = PlatformDirs {
            config: Some(d.clone()),
            ..PlatformDirs::default()
        };
        try_write_port_cfg(
            &dirs,
            &[
                ("key_up", Some("KeyW,ArrowUp")),
                ("key_bomb", Some("none")),
                ("key_shot", Some("Bogus")),
            ],
        )
        .unwrap();
        let s = Settings::game_environment(dirs);
        assert_eq!(s.keys.binds[0], [KeyCode::KeyW, KeyCode::ArrowUp]);
        assert!(s.keys.binds[5].is_empty());
        assert_eq!(s.keys.binds[4], [KeyCode::KeyZ]);
        assert_eq!(s.value("key_up"), Some("KeyW,ArrowUp"));
        assert!(Settings::default().keys == crate::platform::keymap::KeyMap::default());
        assert!(find("key_down").unwrap().check("KeyS,Numpad2").is_ok());
        assert!(find("key_down").unwrap().check("F1").is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    // Gamepad rows load from and save to port.cfg; a bad value keeps the default.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn port_cfg_pads() {
        use crate::platform::padmap::{PadMap, RIGHT_Z};
        let d = std::env::temp_dir().join(format!("th08-portcfg-pads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let dirs = PlatformDirs {
            config: Some(d.clone()),
            ..PlatformDirs::default()
        };
        assert!(!Settings::game_environment(dirs.clone()).pads.custom());
        try_write_port_cfg(
            &dirs,
            &[
                ("pad_shot", Some("RightTrigger2,South")),
                ("pad_bomb", Some("RightZ")),
                ("pad_focus", Some("Select")),
                ("pad_enter", Some("Start")),
            ],
        )
        .unwrap();
        let text = std::fs::read_to_string(d.join("port.cfg")).unwrap();
        assert!(text.contains("pad_shot = RightTrigger2,South\n"), "{text}");
        let s = Settings::game_environment(dirs);
        assert_eq!(s.pads.binds[0], [7, 0]);
        assert_eq!(s.pads.binds[1], [RIGHT_Z]);
        assert_eq!(s.pads.binds[2], PadMap::default().binds[2]);
        assert_eq!(s.pads.binds[5], [9]);
        assert!(s.pads.custom());
        assert!(Settings::default().pads == PadMap::default());
        assert!(find("pad_skip").unwrap().check("North,LeftZ").is_ok());
        assert!(find("pad_skip").unwrap().check("DPadUp").is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn port_cfg_dev_keys_and_paths() {
        let t = "# c\nscale = 2\ninvuln = on\nfuture_key = 1\nenemy-dmg=3\n";
        assert_eq!(drop_dev_keys(t), "# c\nscale = 2\nfuture_key = 1\n");
        let cfg = Some(std::path::Path::new("/cfg"));
        let e = find("save_dir").unwrap();
        assert_eq!(cfg_path(e, "saves", cfg), "/cfg/saves");
        assert_eq!(cfg_path(e, "/abs", cfg), "/abs");
        assert_eq!(cfg_path(e, "saves", None), "saves");
        assert_eq!(cfg_path(find("scale").unwrap(), "2", cfg), "2");
        assert_eq!(cfg_path(find("font").unwrap(), "f.ttf", cfg), "/cfg/f.ttf");
    }

    #[test]
    fn port_cfg_lines() {
        let v = parse_port_cfg("# c\ngame_dir = /a b/c\n\nscale=2\nbad\n");
        assert_eq!(
            v,
            vec![
                ("game_dir".into(), "/a b/c".into()),
                ("scale".into(), "2".into())
            ]
        );
    }

    #[test]
    fn entry_metadata() {
        let mut labels = std::collections::HashSet::new();
        for e in ENTRIES {
            assert!(
                e.label.starts_with(char::is_uppercase),
                "{}: no label",
                e.key
            );
            assert!(labels.insert(e.label), "{}: label used twice", e.key);
            if e.audience == Audience::Dev {
                assert!(!e.persist, "{}: developer rows are never saved", e.key);
            }
            assert_eq!(
                e.audience == Audience::Dev,
                matches!(e.group, Group::StartGame | Group::Cheats | Group::Tools)
            );
        }
        assert_eq!(find("scale").unwrap().label, "Window size");
    }

    #[test]
    fn platform_filter() {
        let p = |k: &str| find(k).unwrap().platforms;
        assert!(
            Platforms::ALL.contains(Platforms::HERE) && !Platforms::NONE.contains(Platforms::HERE)
        );
        assert!(!Platforms::DESKTOP.contains(Platforms::NONE));
        assert_eq!(p("vsync").names(), ["desktop", "web", "android"]);
        assert_eq!(p("scale").names(), ["desktop"]);
        assert_eq!(p("fullscreen").names(), ["desktop"]);
        assert_eq!(p("game_dir").names(), ["desktop"]);
        assert_eq!(p("screen_margin").names(), ["web", "android"]);
        assert_eq!(p("turbo").names(), ["desktop", "android"]);
        assert!(p("touch_layout").names().is_empty());
        if Platforms::HERE == Platforms::DESKTOP {
            assert!(find("scale").unwrap().shown_on_this_platform());
            assert!(!find("screen_margin").unwrap().shown_on_this_platform());
        }
    }
}
