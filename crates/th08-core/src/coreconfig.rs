//! Port-only debug and start-up switches the game core reads (not a port; the game's own th08.cfg is 15 §1). Built by
//! the app / tools from `settings::Settings`; the core never reads the process-global settings.

// Debug ending start (`TH08_ENDING`): `<shot 0–11><a|b|c>` or a `.end` script name.
#[derive(Clone, Debug, PartialEq)]
pub enum Ending {
    Kind { shot: u8, kind: char },
    File(String),
}

#[derive(Clone, Debug, Default)]
pub struct CoreConfig {
    // `TH08_SEED`: fixed boot RNG seed instead of (u16) timeGetTime() (11 §1.3).
    pub seed: Option<u16>,
    // Debug start: stage index, start frame (ECL / stage fast-forward), shot, difficulty, power, continues.
    pub stage: Option<usize>,
    pub stage_frame: u32,
    pub player: Option<u8>,
    pub rank: Option<i32>,
    pub power: Option<f32>,
    pub continues: u8,
    pub ending: Option<Ending>,
    // `TH08_REPLAY=<file | demoN>` and its speed; `TH08_REPLAY_SAVE=<slot>` saves the run's replay.
    pub replay: Option<String>,
    pub replay_speed: u8,
    pub replay_save: Option<u32>,
    // `TH08_INPUT`: scripted input file.
    pub input: Option<String>,
    pub invuln: bool,
    pub unlock: bool,
    pub enemy_dmg: i32,
    // `TH08_BOMB`: forced bomb index (4 = dissolve spell).
    pub bomb: Option<usize>,
    pub clrd: Option<Vec<u16>>,
    // `<frame>:<type>,<p0>,<p1>,<p2>,<p3>`.
    pub screen_fx: Option<[i64; 6]>,
    pub slot_test: Option<usize>,
    // ANM viewer: script spec, interrupts, positions, draw mode.
    pub anm: Option<String>,
    pub anm_int: Vec<(u64, u16)>,
    pub anm_pos: Vec<f32>,
    pub anm_mode: Option<i32>,
    pub se_log: bool,
    // `TH08_FONT`: Japanese font file for the dialogue / card-name text raster.
    pub font: Option<String>,
}
