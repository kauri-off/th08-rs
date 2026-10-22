//! Port settings, not a port of the original (the game's own th08.cfg is 15 §1): one typed struct, read once from
//! command-line flags > env vars (native) or URL params (web) > the port config file `port.cfg` (web: localStorage)
//! > defaults.

use std::path::PathBuf;
use std::sync::OnceLock;

// Debug ending start (`TH08_ENDING`): `<shot 0–11><a|b|c>` or a `.end` script name.
#[derive(Clone, Debug, PartialEq)]
pub enum Ending {
    Kind { shot: u8, kind: char },
    File(String),
}

// Where a value came from, highest priority last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    #[default]
    Default,
    PortCfg,
    Env,
    Cli,
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
    Display,
    StartGame,
    Cheats,
    Tools,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::GameData,
        Group::Display,
        Group::StartGame,
        Group::Cheats,
        Group::Tools,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::GameData => "Game data",
            Group::Display => "Display",
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
// is `env` without `TH08_`, lowercase. `default` is the value text ("" = unset).
pub struct Entry {
    pub key: &'static str,
    pub env: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    pub group: Group,
    pub help: &'static str,
    set: Setter,
}

impl Entry {
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

#[derive(Clone, Debug)]
pub struct Settings {
    // Effective value text and origin of every entry set from a source (defaults included).
    values: std::collections::BTreeMap<&'static str, (String, Origin)>,
    pub game_dir: Option<PathBuf>,
    // Window client size 640×480 × scale (1–4); native only.
    pub scale: u32,
    pub fullscreen: bool,
    pub vsync: bool,
    // Show the launcher screen before the game (th08 binary only).
    pub launcher: bool,
    pub save_dir: Option<PathBuf>,
    pub font: Option<String>,
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
            values: Default::default(),
            game_dir: None,
            scale: 1,
            fullscreen: false,
            vsync: true,
            launcher: true,
            save_dir: None,
            font: None,
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

pub fn parse_bool(v: &str) -> Result<bool, String> {
    match v.trim().to_ascii_lowercase().as_str() {
        "" | "1" | "true" | "on" | "yes" => Ok(true),
        "0" | "false" | "off" | "no" => Ok(false),
        _ => Err("expected 1/0, true/false or on/off".into()),
    }
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

pub const ENTRIES: &[Entry] = &[
    Entry {
        key: "game_dir",
        env: "TH08_DIR",
        kind: Kind::Path,
        default: "",
        group: Group::GameData,
        help: "game folder with th08.dat and thbgm.dat (also the first plain argument)",
        set: |s, v| {
            s.game_dir = Some(PathBuf::from(v));
            Ok(())
        },
    },
    Entry {
        key: "scale",
        env: "TH08_SCALE",
        kind: Kind::Int { min: 1, max: 4 },
        default: "1",
        group: Group::Display,
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
        help: "start fullscreen (native; Alt+Enter toggles and saves)",
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
        help: "present with vsync (default on); frame pacing is separate",
        set: |s, v| {
            s.vsync = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "launcher",
        env: "TH08_LAUNCHER",
        kind: Kind::Bool,
        default: "on",
        group: Group::Display,
        help: "show this settings screen at start (--no-launcher skips it once)",
        set: |s, v| {
            s.launcher = parse_bool(v)?;
            Ok(())
        },
    },
    Entry {
        key: "save_dir",
        env: "TH08_SAVE_DIR",
        kind: Kind::Path,
        default: "",
        group: Group::GameData,
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
        help: "Japanese font for dialogue text (native)",
        set: |s, v| {
            s.font = Some(v.to_string());
            Ok(())
        },
    },
    Entry {
        key: "seed",
        env: "TH08_SEED",
        kind: Kind::Int { min: 0, max: 65535 },
        default: "",
        group: Group::StartGame,
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
        help: "start a game at that stage (also stageN[_s][.std], idx:N)",
        set: |s, v| {
            s.stage = Some(crate::stage::parse_stage_arg(v).ok_or("unknown stage")?);
            Ok(())
        },
    },
    Entry {
        key: "stage_frame",
        env: "TH08_STAGE_FRAME",
        kind: Kind::Int { min: 0, max: U32 },
        default: "0",
        group: Group::StartGame,
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
        help: "shot type 0-11 or a name (reimu&yukari, ry, marisa, ...) for --stage/--ending",
        set: |s, v| {
            s.player = Some(crate::player::parse_shot(v).ok_or("unknown shot type")?);
            Ok(())
        },
    },
    Entry {
        key: "rank",
        env: "TH08_RANK",
        kind: Kind::Enum(&["easy", "normal", "hard", "lunatic", "extra"]),
        default: "",
        group: Group::StartGame,
        help: "difficulty for --stage/--ending (also e/n/h/l/x, 0-4)",
        set: |s, v| {
            s.rank = Some(crate::enemy::parse_difficulty(v).ok_or("unknown difficulty")?);
            Ok(())
        },
    },
    Entry {
        key: "power",
        env: "TH08_POWER",
        kind: Kind::Int { min: 0, max: 128 },
        default: "",
        group: Group::StartGame,
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
        help: "replay playback speed",
        set: |s, v| {
            s.replay_speed = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "replay_save",
        env: "TH08_REPLAY_SAVE",
        kind: Kind::Int { min: 1, max: 15 },
        default: "",
        group: Group::Tools,
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
        help: "send --shot through the window's scaling blit at that size",
        set: |s, v| {
            s.shot_size = Some(pair(v, 'x')?);
            Ok(())
        },
    },
    Entry {
        key: "turbo",
        env: "TH08_TURBO",
        kind: Kind::Int { min: 0, max: U32 },
        default: "0",
        group: Group::Tools,
        help: "N logic frames per drawn frame, no frame pacing",
        set: |s, v| {
            s.turbo = num(v)?;
            Ok(())
        },
    },
    Entry {
        key: "slot_test",
        env: "TH08_SLOT_TEST",
        kind: Kind::Int { min: 0, max: I32 },
        default: "",
        group: Group::Tools,
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
        help: "write a draw log (.xz through the xz tool)",
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
    // One value from `origin`; a bad value is logged and the previous one kept. Empty = unset, except booleans (on).
    fn apply(&mut self, origin: Origin, name: &str, sw: &Entry, v: &str) {
        let v = v.trim();
        if v.is_empty() && !sw.is_bool() {
            return;
        }
        match sw.parse_into(self, v) {
            Ok(()) => {
                self.values.insert(sw.key, (v.to_string(), origin));
            }
            Err(e) => log::error!("{} {name}={v}: {e}; ignored", origin_name(origin)),
        }
    }

    // Sets one entry by key (a settings UI); the caller saves it with `save_port_cfg` if wanted.
    pub fn set(&mut self, key: &str, v: &str) -> Result<(), String> {
        let e = find(key).ok_or_else(|| format!("unknown setting {key}"))?;
        e.parse_into(self, v.trim())?;
        self.values
            .insert(e.key, (v.trim().to_string(), Origin::Cli));
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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, clap::Error> {
        Self::from_args_over(Self::from_environment(), args)
    }

    // The `th08` game binary: as `from_args`, with port.cfg under env and command line.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn game_from_args(args: impl IntoIterator<Item = String>) -> Result<Self, clap::Error> {
        Self::from_args_over(Self::game_environment(), args)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_args_over(
        mut s: Self,
        args: impl IntoIterator<Item = String>,
    ) -> Result<Self, clap::Error> {
        let m = command().try_get_matches_from(
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
        values: impl IntoIterator<Item = (&'static Entry, &'a str, Origin)>,
    ) -> Self {
        let mut s = Self::default();
        for (e, v, o) in values {
            s.apply(o, e.key, e, v);
        }
        s
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
    pub fn from_environment() -> Self {
        Self::default().with_env()
    }

    // The game (native binary, web build): defaults < port.cfg (web: localStorage) < env vars / URL params.
    pub fn game_environment() -> Self {
        let mut s = Self::default();
        for (k, v) in read_port_cfg() {
            match find(&k) {
                Some(sw) => s.apply(Origin::PortCfg, &k, sw, &v),
                None => log::error!("port.cfg: unknown key {k} ignored"),
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
        self
    }
}

fn origin_name(o: Origin) -> &'static str {
    match o {
        Origin::Default => "default",
        Origin::PortCfg => "port.cfg:",
        Origin::Env if cfg!(target_arch = "wasm32") => "URL:",
        Origin::Env => "env:",
        Origin::Cli => "command line:",
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

#[cfg(not(target_arch = "wasm32"))]
const POS_DIR: &str = "game dir";

#[cfg(not(target_arch = "wasm32"))]
fn no_id(e: &Entry) -> String {
    format!("no-{}", e.key)
}

// Validates a CLI value with the entry's own table parser (kind range/choices plus aliases such as `idx:N`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct EntryParser(&'static Entry);

#[cfg(not(target_arch = "wasm32"))]
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
            let name = arg.map_or(self.0.key.to_string(), |a| a.to_string());
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
// (old rule: a boolean takes the next word only if it is an on/off value; else `--bool` alone = on).
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
pub fn command() -> clap::Command {
    use clap::{Arg, ArgAction, Command};
    let mut cmd = Command::new("th08")
        .about("Touhou 8 (Imperishable Night) engine port")
        .args_override_self(true)
        .after_help(format!(
            "Each switch is also an env var (shown), a web URL parameter (name without TH08_, lowercase) and a\n\
             `key = value` line in port.cfg. Priority: command line > env > port.cfg > default.\n\
             Booleans take 1/0, true/false or on/off; `--switch` alone means on, `--no-switch` off.\n\
             port.cfg: {}",
            port_cfg_path().map_or("(no config dir)".into(), |p| p.display().to_string())
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

// Entry points: install the settings built from the command line before anything reads them.
pub fn init(s: Settings) {
    if SETTINGS.set(s).is_err() {
        log::error!("settings already initialised");
    }
}

// The settings, read once (defaults + env / URL, never port.cfg, if the entry point did not call `init`).
pub fn get() -> &'static Settings {
    SETTINGS.get_or_init(Settings::from_environment)
}

// Tests and tools: the game directory (TH08_DIR only).
pub fn test_game_dir() -> Option<PathBuf> {
    get().game_dir.clone()
}

// Platform config dir (`directories` BaseDirs): $XDG_CONFIG_HOME/th08 (~/.config/th08), %APPDATA%\th08,
// ~/Library/Application Support/th08.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir() -> Option<PathBuf> {
    Some(directories::BaseDirs::new()?.config_dir().join("th08"))
}

#[cfg(target_arch = "wasm32")]
pub fn config_dir() -> Option<PathBuf> {
    None
}

// Directory of the running binary (save fallback when there is no config dir).
#[cfg(not(target_arch = "wasm32"))]
pub fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

pub fn port_cfg_path() -> Option<PathBuf> {
    Some(config_dir()?.join("port.cfg"))
}

// `key = value` lines; `#` comments and blank lines skipped.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn parse_port_cfg(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| match l.split_once('=') {
            Some((k, v)) => Some((k.trim().to_string(), v.trim().to_string())),
            None => {
                log::error!("port.cfg: line {l:?} is not `key = value`; ignored");
                None
            }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn read_port_cfg() -> Vec<(String, String)> {
    port_cfg_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| parse_port_cfg(&t))
        .unwrap_or_default()
}

// Web: the port.cfg slot is localStorage, one `th08.<key>` item per setting.
#[cfg(target_arch = "wasm32")]
fn read_port_cfg() -> Vec<(String, String)> {
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
pub fn save_port_cfg(key: &str, value: &str) {
    write_port_cfg(&[(key, Some(value))]);
}

// Sets (Some) or removes (None) port.cfg keys in one write; web: localStorage items.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_port_cfg(changes: &[(&str, Option<&str>)]) {
    let Some(path) = port_cfg_path() else {
        log::error!("no config directory; port.cfg not saved");
        return;
    };
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut text = old.clone();
    for &(key, value) in changes {
        match find(key) {
            Some(e) => text = set_cfg_line(&text, e.key, value),
            None => log::error!("port.cfg: unknown key {key} not saved"),
        }
    }
    if text == old {
        return;
    }
    let res = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(&path, text));
    if let Err(e) = res {
        log::error!("{}: {e}", path.display());
    }
}

#[cfg(target_arch = "wasm32")]
pub fn write_port_cfg(changes: &[(&str, Option<&str>)]) {
    let Some(st) = local_storage() else {
        log::error!("no localStorage; settings not saved");
        return;
    };
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
            log::error!("localStorage: {key} not saved");
        }
    }
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
        Settings::from_args(a.iter().map(|s| s.to_string())).unwrap()
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
        assert!(Settings::from_args(["--help".to_string()]).is_err());
        let bad = |a: &[&str]| Settings::from_args(a.iter().map(|s| s.to_string())).is_err();
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
        set(&mut s, "se_log", "");
        assert!(s.se_log);
        assert!(find("stage-frame").is_some() && find("dir").is_some());
    }

    #[test]
    fn from_values() {
        let e = |k| find(k).unwrap();
        let s = Settings::from_values([
            (e("stage"), "4a", Origin::Cli),
            (e("scale"), "3", Origin::PortCfg),
            (e("vsync"), "off", Origin::Cli),
        ]);
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
}
