//! Game manager GM 0x160F508: stage start, GameMgr_Calc flow, rank, routing, scene state (11 §2–§7).

use crate::bullet::BulletManager;
use crate::enemy::EnemyManager;
use crate::game::Ctx;
use crate::hud::Hud;
use crate::player::{Player, PlayerRef, Stats};
use crate::stage::Stage;
use crate::task::{self, Chain, Ret, Task, TaskRef};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

// Game flags GM +0x3DBAC = 0x164D0B4 (11 §2.1).
pub const F_PRACTICE: u32 = 1 << 0;
pub const F_DEMO: u32 = 1 << 1;
pub const F_RUNNING: u32 = 1 << 2;
pub const F_REPLAY: u32 = 1 << 3;
pub const F_ENDING_OK: u32 = 1 << 4;
pub const F_CLEAR_PANEL: u32 = 1 << 9;
pub const F_DEATH: u32 = 1 << 10;
pub const F_SPELL_PRACTICE: u32 = 1 << 14;
// Bits 7 / 8 / 13: last spell running / captured, last-spell damage box (11 §2.1, written by ECL op 176 and
// `Spell_End`, read by the player, bombs and the enemy manager).
pub const F_LAST_SPELL: u32 = 1 << 7;
pub const F_LAST_CAPTURED: u32 = 1 << 8;
pub const F_13: u32 = 1 << 13;
// Bits 5–6 stage-end state, bits 11–12 final route (11 §2.1).
const END_SHIFT: u32 = 5;
const ROUTE_SHIFT: u32 = 11;

// GameMgr_Calc 0x439BC7 prio 2, draw 0x43AA03 prio 5 (11 §3).
pub const CALC_PRIO: u16 = 2;
pub const DRAW_PRIO: u16 = 5;

// Rank init / min / max by difficulty (table 0x4C7880, 11 §3.2).
const RANK: [[i32; 3]; 5] = [
    [10, 8, 16],
    [10, 8, 16],
    [8, 8, 12],
    [8, 8, 12],
    [16, 15, 16],
];

// Spell-practice BGM (02 §4, table 0x4C7670): the first record whose max spell id >= the card id; the flag is
// record +0x10, 1 only for the two th08_13b records (retry music rule, 11 §4 step 3, 12 §3 state 10).
const SPELL_BGM: [(i16, &str, bool); 18] = [
    (1, "bgm/th08_00.mid", false),
    (12, "bgm/th08_03.mid", false),
    (16, "bgm/th08_04.mid", false),
    (31, "bgm/th08_05.mid", false),
    (35, "bgm/th08_06.mid", false),
    (53, "bgm/th08_07.mid", false),
    (76, "bgm/th08_09.mid", false),
    (99, "bgm/th08_10.mid", false),
    (118, "bgm/th08_12.mid", false),
    (122, "bgm/th08_13.mid", false),
    (142, "bgm/th08_14.mid", false),
    (146, "bgm/th08_13b.mid", true),
    (150, "bgm/th08_13.mid", false),
    (170, "bgm/th08_15.mid", false),
    (190, "bgm/th08_13b.mid", true),
    (193, "bgm/th08_18.mid", false),
    (204, "bgm/th08_19.mid", false),
    (222, "bgm/th08_20.mid", false),
];

fn spell_bgm(card: i16) -> Option<&'static str> {
    SPELL_BGM.iter().find(|r| r.0 >= card).map(|r| r.1)
}

// `FUN_00439916`: the card's 0x4C7670 record +0x10 retry-restart flag (02 §4).
pub fn spell_bgm_restarts(card: i16) -> bool {
    SPELL_BGM.iter().find(|r| r.0 >= card).is_some_and(|r| r.2)
}

// gm+0x3DE28 at every stage start, reloads included: 2 (play nothing) only for a spell-practice reload whose card
// record flag is 0, else 1 (11 §2, VERIFIED 0x43ABD7).
fn music_start(g: &Game) -> u8 {
    if g.reload_same && g.spell_practice() && !spell_bgm_restarts(g.card) {
        2
    } else {
        1
    }
}

// Demo music-fade and end frames per demo index (11 §4 step 6).
const DEMO_LIMITS: [(i32, i32); 4] = [(6000, 6120), (4800, 4920), (4920, 5040), (6900, 7020)];

// Scenes (11 §7).
pub const SCENE_BOOT: i32 = 0;
pub const SCENE_MENU: i32 = 1;
pub const SCENE_GAME: i32 = 2;
pub const SCENE_NEXT_STAGE: i32 = 3;
pub const SCENE_RESULT_MENU: i32 = 5;
pub const SCENE_RESULT: i32 = 6;
pub const SCENE_REPLAY_END: i32 = 7;
pub const SCENE_MUSIC_ROOM: i32 = 8;
pub const SCENE_ENDING: i32 = 9;
pub const SCENE_RETRY: i32 = 10;
pub const SCENE_RELOAD: i32 = 11;

thread_local! {
    // The game flags word GM+0x3DBAC = 0x164D0B4 (11 §2.1). One word: `Game::flags` and the subsystems that
    // have no `Ctx` (player, bombs, enemy manager, ECL) all read and write it here.
    static FLAGS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub fn flags() -> u32 {
    FLAGS.get()
}

pub fn set_flags(v: u32) {
    FLAGS.set(v);
}

// `Game::flags`: a handle on the thread-local word above, so `g.flags & F`, `g.flags |= F`, `g.flags &= !F` keep
// working on the one word.
#[derive(Clone, Copy, Debug, Default)]
pub struct GameFlags;

impl GameFlags {
    pub fn get(self) -> u32 {
        flags()
    }

    pub fn set(&mut self, v: u32) {
        set_flags(v);
    }
}

impl std::ops::BitAnd<u32> for GameFlags {
    type Output = u32;
    fn bitand(self, m: u32) -> u32 {
        flags() & m
    }
}

impl std::ops::Shr<u32> for GameFlags {
    type Output = u32;
    fn shr(self, n: u32) -> u32 {
        flags() >> n
    }
}

impl std::ops::BitOrAssign<u32> for GameFlags {
    fn bitor_assign(&mut self, m: u32) {
        set_flags(flags() | m);
    }
}

impl std::ops::BitAndAssign<u32> for GameFlags {
    fn bitand_assign(&mut self, m: u32) {
        set_flags(flags() & m);
    }
}

// Game state that survives scene changes (GM object 11 §2 + the menu's choices).
pub struct Game {
    // Supervisor +0x158 current, +0x15C requested (0x17CE8B4), +0x160 previous scene (11 §7).
    pub scene: i32,
    pub requested: i32,
    pub previous: i32,
    // GM +0x30: 0 Easy .. 4 Extra.
    pub difficulty: i32,
    // GM +0x3DBA9 shot type 0–11.
    pub shot: u8,
    // GM +0x3DDC4 stage index 0–8.
    pub stage: usize,
    // GM +0x3DBAC game flags (the shared word, `GameFlags`).
    pub flags: GameFlags,
    // GM +0x3DBB2 paused (1 menu open, 2 drawn once), +0x3DBB3 game-over / continue menu.
    pub paused: u8,
    pub game_over: u8,
    // 0x17CE8BC fresh-start flag, 0x17CE8C4 reload-same-stage flag (11 §7).
    pub fresh_start: bool,
    pub reload_same: bool,
    // Scene to request once the stage-end state is confirmed (11 §4 step 1).
    pub next_scene_after: i32,
    // GM +0x38 loading state (0 playing, 1 loading, 2 failed), +0x3C frames spent loading.
    pub loading: u8,
    pub loading_frames: u32,
    // Port-only: loading screen drawn this frame; frames in Loading_Wait (12 §1 "Press Shot Button").
    pub loading_shown: bool,
    pub loading_wait: u32,
    // 0x164D2C8 = GM +0x3DDC0 gameplay frame counter (11 §2).
    pub frame_counter: u32,
    pub cfg: crate::config::Config,
    // *(GM+4): the 60-byte gameplay config copy (11 §3.2, 15 §1); +0x1C lives, +0x25 slow mode. The rest of the
    // game (SE, volumes, effect level) reads the Supervisor config `cfg`.
    pub cfg_game: [u8; 60],
    pub stats: Stats,
    // Clear masks per shot (12 = union) and difficulty: no-continue / any (11 §4 step 1, 15 CLRD).
    pub clear_nc: [[u16; 5]; 13],
    pub clear_any: [[u16; 5]; 13],
    pub score: crate::scoredat::ScoreDat,
    pub persist: bool,
    catk_backup: Option<Vec<crate::scoredat::Catk>>,
    run_clock: Option<web_time::Instant>,
    pub play_clock: Option<web_time::Instant>,
    pub retrying: bool,
    // GM +0x3DBB4 demo index (3 at boot, 11 §4 step 6), +0x3DBB8 demo frame counter, +0x3DBAA shot bias.
    pub demo: u8,
    pub demo_frame: i32,
    pub demo_shot_bias: u8,
    // 0x164D0BD playback speed setting (1 = slowdown reproduction, 15 §3.3).
    pub replay_speed: u8,
    pub rpy: Option<crate::replay::Replay>,
    pub replay_ff: bool,
    pub dialogue_skippable: bool,
    // fps_now u8 0x17CE8F0 (15 §3.2): 0 at boot, carried across scenes and games; red flag 0x17CEA94.
    pub replay_fps: u8,
    pub replay_fps_red: bool,
    pub noclip: bool,
    pub watched_replay: bool,
    // GM+0x3DE08 frame-skip counter shared by playback slowdown (11 §4 step 5) and slow mode (step 10). Zeroed only
    // by the GM static constructor, so its phase carries over between games and between playback and live play.
    skip_ctr: u32,
    // gm+0x2D slow-mode skipped frame (11 §4 step 10): while set the Supervisor OR-accumulates input (11 §7) and
    // ESC does not pause (step 4). Cleared by step 10, StageStart and the GM on-remove.
    pub slow_skip: u8,
    // 0x164D2C4 RNG state captured at stage start (11 §1.3).
    pub stage_seed: u16,
    pub stages_started: u32,
    pub frame_skip: bool,
    pub clock: i32,
    pub continue_clock: bool,
    pub continue_apply: bool,
    // GM +0x3DBB0 spell id for spell practice.
    pub card: i16,
    pub menu_loading: u32,
    // Recorded loader timing of a session replay (src/session.rs).
    pub sched: Option<crate::session::Sched>,
    pub trace_menu: Option<(i32, i32)>,
    // Result state (+0x08) after Result_Calc, session trace column 31.
    pub trace_result: Option<i32>,
    // HSCR +0x88: cards captured in this game (15 §2.3).
    pub rec_cards: Vec<u8>,
    // An LSNM chunk was read from score.dat (16 §2.2: name-entry cursor starts on END).
    pub lsnm_found: bool,
    // Loading_Begin requested by the result screen (16 §2.1 state 2).
    pub loading_req: bool,
}

impl Default for Game {
    fn default() -> Self {
        // GameMgr_Ctor zeroes the GM, flags included (11 §2).
        set_flags(0);
        Self {
            scene: SCENE_BOOT,
            requested: SCENE_BOOT,
            previous: SCENE_BOOT,
            difficulty: 1,
            shot: 0,
            stage: 0,
            flags: GameFlags,
            paused: 0,
            game_over: 0,
            fresh_start: true,
            reload_same: false,
            next_scene_after: -1,
            loading: 0,
            loading_frames: 0,
            loading_shown: false,
            loading_wait: 0,
            frame_counter: 0,
            cfg: Default::default(),
            cfg_game: crate::config::Config::default().to_bytes(),
            stats: Stats::default(),
            clear_nc: [[1; 5]; 13],
            clear_any: [[1; 5]; 13],
            score: Default::default(),
            persist: false,
            catk_backup: None,
            run_clock: None,
            play_clock: None,
            retrying: false,
            demo: 3,
            demo_frame: 0,
            demo_shot_bias: 0,
            replay_speed: 0,
            rpy: None,
            replay_ff: true,
            dialogue_skippable: false,
            replay_fps: 0,
            replay_fps_red: false,
            noclip: false,
            watched_replay: false,
            skip_ctr: 0,
            slow_skip: 0,
            stage_seed: 0,
            stages_started: 0,
            frame_skip: false,
            clock: 0,
            continue_clock: false,
            continue_apply: false,
            card: 0,
            menu_loading: 0,
            sched: None,
            trace_menu: None,
            trace_result: None,
            rec_cards: vec![0; crate::scoredat::CARDS],
            lsnm_found: false,
            loading_req: false,
        }
    }
}

impl Game {
    // Bits 5–6: 0 none, 1 waiting for load, 2 clear triggered, 3 clear handled (11 §2.1).
    pub fn end_state(&self) -> u32 {
        (self.flags >> END_SHIFT) & 3
    }

    pub fn set_end_state(&mut self, v: u32) {
        set_flags(flags() & !(3 << END_SHIFT) | (v & 3) << END_SHIFT);
    }

    // Bits 11–12: 0 → 6A, else 6B (11 §4.1).
    pub fn route(&self) -> u8 {
        ((self.flags >> ROUTE_SHIFT) & 3) as u8
    }

    pub fn set_route(&mut self, c: u8) {
        set_flags(flags() & !(3 << ROUTE_SHIFT) | ((c & 3) as u32) << ROUTE_SHIFT);
    }

    pub fn practice(&self) -> bool {
        self.flags & F_PRACTICE != 0
    }

    pub fn spell_practice(&self) -> bool {
        self.flags & F_SPELL_PRACTICE != 0
    }

    // 0x418130 / 0x418180: spell practice of card `id` (GM +0x3DBB0) / of a card in lo..=hi.
    pub fn is_practice_card(&self, id: i16) -> bool {
        self.spell_practice() && self.card == id
    }

    pub fn is_practice_card_in(&self, lo: i16, hi: i16) -> bool {
        self.spell_practice() && (lo..=hi).contains(&self.card)
    }

    pub fn replay(&self) -> bool {
        self.flags & F_REPLAY != 0
    }

    // 0x43C322 (12 §5.6): replay playback of a practice game (replay info +0x7B).
    pub fn practice_replay(&self) -> bool {
        self.replay() && self.rpy.as_ref().is_some_and(|r| r.practice())
    }

    pub fn request(&mut self, scene: i32) {
        self.requested = scene;
    }

    fn diff(&self) -> usize {
        self.difficulty.clamp(0, 4) as usize
    }

    // Debug `TH08_CLRD=<shot>,<any>[,<nc>]`: sets that shot's clear masks on every difficulty.
    pub fn debug_clear_masks(&mut self, clrd: Option<&[u16]>) {
        if let Some(&[shot, any, ref rest @ ..]) = clrd {
            self.clear_any[shot as usize % 12] = [any; 5];
            self.clear_nc[shot as usize % 12] = [rest.first().copied().unwrap_or(0); 5];
        }
    }

    pub fn cfg_bytes(&self) -> [u8; 60] {
        self.cfg.to_bytes()
    }

    // Boot load (15 §2.2); a missing or invalid file starts from defaults.
    pub fn load_score(&mut self, file: Option<&[u8]>) {
        self.lsnm_found = false;
        self.score = match file.map(crate::scoredat::ScoreDat::parse) {
            Some(Ok(s)) => {
                self.lsnm_found = true;
                s
            }
            Some(Err(e)) => {
                log::warn!("score.dat: {e:#}; starting from defaults");
                Default::default()
            }
            None => Default::default(),
        };
        for (i, c) in self.score.clrd.iter().enumerate() {
            self.clear_nc[i] = c.nc;
            self.clear_any[i] = c.any;
        }
        self.run_clock = Some(web_time::Instant::now());
    }

    // ScoreDat_Save (Result removal): 2 RNG draws. `flush` = Result mode 2 (slow mode drops this run's card credit).
    pub fn save_score(&mut self, rng: &mut crate::rng::Rng, flush: bool) -> Vec<u8> {
        // Result mode 2 registers and is removed at once: its Result_OnRegister list build runs first (15 §2.3).
        if flush {
            self.score.rebuild_lists();
        }
        if flush && self.cfg.slow != 0 {
            self.restore_catk_backup();
        }
        for (i, c) in self.score.clrd.iter_mut().enumerate() {
            c.nc = self.clear_nc[i];
            c.any = self.clear_any[i];
        }
        self.update_run_time();
        let bytes = self.score.encode(rng);
        self.lsnm_found = true;
        if self.persist {
            crate::scoredat::storage_write("score.dat", &bytes);
        }
        bytes
    }

    // CATK backup taken at game start (15 §2.5): slow mode drops this run's card credit.
    pub fn restore_catk_backup(&mut self) {
        if let Some(b) = self.catk_backup.clone() {
            self.score.catk = b;
        }
    }

    // Supervisor_UpdateRunTime (15 §2.9): PLST run time += time since the last update.
    pub fn update_run_time(&mut self) {
        if let Some(t) = self.run_clock.replace(web_time::Instant::now()) {
            crate::scoredat::Plst::add_time(
                &mut self.score.plst.run_time,
                t.elapsed().as_millis() as u64,
            );
        }
    }

    // Stage-clear / ending bits: this shot's masks and the union entry 12.
    pub fn set_clear(&mut self, no_continue: bool, bit: u16) {
        let (shot, d) = (self.shot as usize % 12, self.diff());
        for k in [shot, 12] {
            if no_continue {
                self.clear_nc[k][d] |= bit;
            }
            self.clear_any[k][d] |= bit;
        }
    }

    pub fn play_counter(&mut self, idx: usize) {
        let d = self.diff();
        self.score.plst.bump(d, idx);
    }

    // PLST plays: block [plays] and [1 + shot] (fresh starts and continues).
    pub fn count_play(&mut self) {
        if self.counts_plays() {
            self.play_counter(crate::scoredat::PL_PLAYS);
            self.play_counter(1 + self.shot as usize % 12);
        }
    }

    // Gameplay copy +0x1C starting lives (11 §3.2, 12 §5).
    pub fn game_lives(&self) -> u8 {
        self.cfg_game[0x1C]
    }

    // Gameplay copy +0x25 slow mode; forced to 0 at a fresh start for Extra and (spell) practice (11 §3.2).
    pub fn game_slow(&self) -> u8 {
        self.cfg_game[0x25]
    }

    // PLST play counters read the cfg copy's slow byte (15 §2.9).
    pub fn counts_plays(&self) -> bool {
        !self.replay() && !self.spell_practice() && self.game_slow() == 0
    }

    // Supervisor_UpdatePlayTime (15 §2.9): adds the time since the mark, then clears it.
    pub fn add_play_time(&mut self) {
        if let Some(t) = self.play_clock.take() {
            crate::scoredat::Plst::add_time(
                &mut self.score.plst.play_time,
                t.elapsed().as_millis() as u64,
            );
        }
    }

    pub fn mark_play_time(&mut self) {
        self.play_clock = Some(web_time::Instant::now());
    }

    pub fn cleared(&self, any: bool, bit: usize) -> bool {
        let m = if any { &self.clear_any } else { &self.clear_nc };
        m[self.shot as usize % 12]
            .iter()
            .take(4)
            .any(|v| v & (1 << bit) != 0)
    }
}

thread_local! {
    // GM rank, max, min, sub-counter (GM+0x3DE2C..0x3DE38, 11 §2); ECL var 10041 reads the rank directly.
    // Boot values 0, 0, 0, 0 (GameMgr_Ctor zeroes the GM) until the first GameMgr_InitRank.
    static RANK_STATE: std::cell::Cell<[i32; 4]> = const { std::cell::Cell::new([0; 4]) };
}

pub fn rank_state() -> [i32; 4] {
    RANK_STATE.get()
}

pub fn set_rank_state(r: [i32; 4]) {
    RANK_STATE.set(r);
}

pub fn rank() -> i32 {
    RANK_STATE.get()[0]
}

pub fn set_rank(v: i32) {
    RANK_STATE.with(|c| {
        let mut r = c.get();
        r[0] = v;
        c.set(r);
    });
}

// GameMgr_InitRank 0x43B936 (11 §3.2): rank, min, max from the table; the sub-counter is left alone.
fn init_rank(d: usize) {
    let [init, min, max] = RANK[d];
    let [.., sub] = RANK_STATE.get();
    RANK_STATE.set([init, max, min, sub]);
}

// 0x164D340 = GM+0x3DE38 := 0 at every stage start (11 §3.1).
fn clear_rank_sub() {
    let [r, max, min, _] = RANK_STATE.get();
    RANK_STATE.set([r, max, min, 0]);
}

// RankAdd / RankSub 0x43BFC3 / 0x43C03F (11 §3.2): ±1 rank per 100 sub-units, clamped to min..max.
pub fn rank_add(d: i32) {
    let [mut r, max, min, mut sub] = RANK_STATE.get();
    sub += d;
    while sub >= 100 {
        sub -= 100;
        r += 1;
    }
    while sub < 0 {
        sub += 100;
        r -= 1;
    }
    RANK_STATE.set([r.clamp(min, max), max, min, sub]);
}

// Stage_Advance 0x43C4B3 (11 §4.1): stage 3 → 4A for teams 1, 2 and solos 6–9, else 4B; 6A/6B by route.
pub fn next_stage(stage: usize, shot: u8, route: u8) -> usize {
    match stage {
        2 => match shot {
            1 | 2 | 6..=9 => 3,
            _ => 4,
        },
        3 | 4 => 5,
        5 => {
            if route == 0 {
                6
            } else {
                7
            }
        }
        s => s + 1,
    }
}

// GameMgr_ClockAdvance 0x43C35F (11 §4.1): stages 1–5 +2 if time < threshold else +1; 6A/6B 0; Extra 4.
pub fn clock_advance(stage: usize, time: i32, threshold: i32) -> i32 {
    match stage {
        0..=5 => {
            if time < threshold {
                2
            } else {
                1
            }
        }
        8 => 4,
        _ => 0,
    }
}

thread_local! {
    static CURRENT: RefCell<std::rc::Weak<RefCell<GameManager>>> = RefCell::new(std::rc::Weak::new());
}

// The registered game manager (debug tools and the demo sync test).
pub fn current() -> Option<Rc<RefCell<GameManager>>> {
    CURRENT.with_borrow(std::rc::Weak::upgrade)
}

// 11 §4 step 5: whether a slowdown-reproduction frame runs, from the recorded fps `f` and the counter after += 1.
fn playback_runs(f: u8, c: u32) -> bool {
    (f >= 20 || c % 3 == 0)
        && (f < 20 || f >= 30 || c % 2 == 0)
        && (f < 30 || f >= 40 || c % 3 != 0)
        && (f < 40 || f >= 50 || c % 6 != 0)
}

// 11 §4 step 10: whether slow mode skips this frame, from `n` live bullets and the counter after += 1.
fn slow_skips(n: usize, c: u32) -> bool {
    (n >= 320 && c % 3 == 0)
        || ((224..320).contains(&n) && c % 4 == 0)
        || ((128..224).contains(&n) && c % 5 == 0)
}

pub struct GameManager {
    pub player: PlayerRef,
    pub stage: Option<Rc<RefCell<Stage>>>,
    pub bullets: Option<Rc<RefCell<BulletManager>>>,
    pub enemies: Option<Rc<RefCell<EnemyManager>>>,
    pub hud: Option<Rc<RefCell<Hud>>>,
    // Subsystems unregistered by the GM on-remove (11 §7).
    tasks: Vec<TaskRef<Ctx>>,
    // Port-only: loading frame at which Ascii_SetupGaugeVms runs, with the gauge limits (12 §5.9).
    gauge_at: Option<(u32, i32, i32)>,
    // Port-only: loading calcs of this stage start (11 §3.1 "Loading frame count").
    loading_total: u32,
    // gm+0x3DE28 deferred music start (11 §4 step 3): 1 plays BGM slot 0 on the first calc after loading, 2 nothing.
    music_start: u8,
    // Port-only: the stage-clear frame has run (11 §4 step 1 loading := 1 with no loader behind it).
    cleared: bool,
}

impl GameManager {
    pub fn register(ctx: &mut Ctx) -> Result<Rc<RefCell<GameManager>>> {
        let gm = Rc::new(RefCell::new(GameManager {
            player: Player::new_ref(),
            stage: None,
            bullets: None,
            enemies: None,
            hud: None,
            tasks: Vec::new(),
            gauge_at: None,
            loading_total: 31,
            music_start: 0,
            cleared: false,
        }));
        gm.borrow().player.borrow_mut().debug_invuln |= ctx.game.noclip;
        // GameManager_Register zeroes the gameplay frame counter 0x164D2C8 (11 §2).
        ctx.game.frame_counter = 0;
        CURRENT.with_borrow_mut(|c| *c = Rc::downgrade(&gm));
        let t: TaskRef<Ctx> = gm.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(gm)
    }

    // Fresh start (11 §3.2) in its RNG order: heap jitter (2 draws), decoys (60), lives / power checksums,
    // score.dat flush (2), rank, deaths / bombs-used checksums.
    fn fresh_start(ctx: &mut Ctx, st: &mut Stats) {
        // Port rule (11 §1.2): the loader thread's 53-bit precision does not affect these integer/decoy draws.
        let rng = &mut ctx.anm.rng;
        rng.next_u32();
        for _ in 0..21 {
            rng.next_u32();
        }
        for _ in 0..9 {
            rng.next_f32();
        }
        let g = &mut ctx.game;
        let d = g.diff();
        // Also 0x164D0B0 := 0, the fresh-start writer of 11 §2.
        *st = Stats {
            difficulty: d as i32,
            ..Stats::default()
        };
        st.gauge = 0;
        st.clock = if g.stage % 9 == 8 { 6 } else { 0 };
        // Config copy = current config (60 bytes), then lives and slow mode overridden (11 §3.2).
        g.cfg_game = g.cfg.to_bytes();
        if d >= 4 {
            g.cfg_game[0x1C] = 2;
        } else if g.practice() {
            g.cfg_game[0x1C] = 8;
        }
        if d >= 4 || g.practice() || g.spell_practice() {
            g.cfg_game[0x25] = 0;
        }
        let lives = g.game_lives() as i32;
        if !g.replay() {
            st.lives = lives as f32;
            Stats::checksum(rng);
        }
        st.power = 0.0;
        Stats::checksum(rng);
        g.rec_cards = vec![0; crate::scoredat::CARDS];
        (st.score10, st.score_disp, st.score_step) = (0, 0, 0);
        st.hiscore = 100000;
        st.continues = 0;
        st.graze_total = 0;
        st.point_items2 = 0;
        st.max_point = crate::player::MAX_POINT[d];
        st.extends = 0;
        st.next_extend = crate::item::next_extend(d as i32, 0);
        // GameMgr_LoadScoreData: flush (Result mode 2), CATK stamp, hiscore, practice play count, CATK backup.
        g.save_score(rng, true);
        g.score.stamp_catk();
        let shot = g.shot as usize % 12;
        // Hiscore-continues digit stats+0x18 from the top HSCR entry, also in practice (11 §5, 15 §2.3).
        st.hiscore_cont = g.score.hiscore_continues(shot, d);
        if g.practice() && !g.spell_practice() {
            let counts = g.counts_plays();
            let p = &mut g.score.pscr[shot];
            st.hiscore = p.hiscore[g.stage % 9][d] as i64;
            if counts {
                p.plays[g.stage % 9][d] += 1;
                p.used = 1;
            }
        } else {
            st.hiscore = g.score.hiscore(shot, d) as i64;
        }
        g.catk_backup = Some(g.score.catk.clone());
        init_rank(d);
        st.deaths = 0;
        Stats::checksum(rng);
        st.bombs_used = 0;
        Stats::checksum(rng);
        st.spells = 0;
    }

    // GameManager_StageStart 0x43ABD7 (11 §3.1) with the subsystems registered synchronously (no loader thread).
    // Not implemented: spell-practice power (11 §3.1), the spell-practice stage overlay base 2.
    fn stage_start(&mut self, ctx: &mut Ctx) -> Result<()> {
        let entries0 = ctx.anm.entries_loaded;
        let g = &mut ctx.game;
        g.loading = 1;
        g.loading_frames = 0;
        // GM on-register: a cleared stage (bits 5–6 ≥ 2) waits for the confirm press.
        if g.end_state() >= 2 {
            g.set_end_state(1);
        }
        ctx.anm.speed = 1.0;
        g.flags &= !F_DEATH;
        // Darkness-mask alpha 0x4E3D28 := 0 (13 §4.1).
        crate::ascii::clear_darkness();
        // gm+0x2D := 0 (0x43B835, 11 §4 step 10).
        g.slow_skip = 0;
        let fresh = g.fresh_start || g.practice() || g.spell_practice() || g.difficulty >= 4;
        let mut st = std::mem::take(&mut ctx.game.stats);
        if fresh {
            Self::fresh_start(ctx, &mut st);
        } else {
            // 11 §3.3 continuing: score display synced, two decoy checksums.
            st.score_disp = st.score10;
            st.score_step = 0;
            Stats::checksum(&mut ctx.anm.rng);
            Stats::checksum(&mut ctx.anm.rng);
        }
        // The rank sub-counter restarts after both branches; a continuing stage keeps the rank (11 §3.1).
        clear_rank_sub();
        let g = &mut ctx.game;
        // PLST play counters (15 §2.9).
        if fresh && g.counts_plays() {
            g.count_play();
            if g.retrying {
                g.play_counter(crate::scoredat::PL_RETRIES);
            }
            if g.practice() {
                g.play_counter(crate::scoredat::PL_PRACTICE);
            }
        }
        g.retrying = false;
        g.mark_play_time();
        // Hud_Load branches on the raw flag 0x17CE8BC, not on the StageStart fresh test (12 §5.1a).
        let hud_fresh = g.fresh_start;
        g.fresh_start = false;
        let (stage, shot, d) = (g.stage, g.shot, g.diff());
        Player::register(ctx, self.player.clone(), shot)?;
        // Player_Load fades out boss markers M_0..M_2 (M_3 is never sent this, 12 §5.10).
        (0..3).for_each(|k| crate::ascii::marker_interrupt(k, 2));
        // Player_Load shows the gauge readout on a fresh start; StageStart re-runs its setup after Player_Load, so
        // it uses the new game's limits (12 §5.9).
        if fresh {
            crate::ascii::gauge_interrupt(1);
        }
        let l = self.player.borrow().stats.limits;
        crate::ascii::gauge_player(&self.player);
        // The loader thread reaches Ascii_SetupGaugeVms k Ascii_Calc ticks before game frame 1 (12 §5.9 timing).
        let demo = (ctx.game.flags & F_DEMO != 0).then_some(ctx.game.demo);
        let k = match demo {
            Some(0) => 28,
            Some(1) => 27,
            Some(2) => 29,
            Some(3) => 33,
            _ => 28,
        };
        {
            let mut p = self.player.borrow_mut();
            let limits = p.stats.limits;
            p.stats = st;
            p.stats.limits = limits;
            p.stats.difficulty = d as i32;
            if fresh && !ctx.game.replay() {
                p.stats.bombs = p.sht.bombs.trunc();
            }
            let s = &mut p.stats;
            s.point_items = 0;
            s.graze = 0;
            s.gauge_start = s.gauge;
            s.time = 0;
            s.time_added = 0;
            s.time_threshold = if ctx.game.spell_practice() {
                0
            } else {
                crate::player::TIME_THR
                    .get(stage % 9)
                    .map_or(10, |r| r[d.min(3)])
            };
            if ctx.game.practice() {
                let pw = match stage {
                    0 => 0.0,
                    1 => 112.0,
                    _ => 128.0,
                };
                s.set_power(pw, &mut ctx.anm.rng);
            }
            // Port-only debug override.
            if let Some(pw) = ctx.settings.power.filter(|_| ctx.game.stages_started == 0) {
                s.power = pw.clamp(0.0, 128.0);
            }
        }
        // Replay playback: rank init, stage record load (seed set), checksum with RNG preserved (11 §3.4).
        if ctx.game.replay() {
            init_rank(d);
            let mut p = self.player.borrow_mut();
            crate::replay::play_stage_load(&mut ctx.game, &mut p.stats, &mut ctx.anm.rng)?;
        }
        let g = &mut ctx.game;
        g.paused = 0;
        g.flags &= !0x2180;
        // stage_seed is captured before the subsystems register (11 §1.3).
        g.stage_seed = ctx.anm.rng.state;
        log::info!(
            "stage {stage} start at frame {}: shot {shot} difficulty {d} seed {:#06x}",
            ctx.frame,
            g.stage_seed
        );
        let first = g.stages_started == 0;
        g.stages_started += 1;
        // Register order Stage, Bullets, Enemies, Effects, HUD, Gui (11 §3.1, 0x43ABD7).
        let stage_t = Stage::register(ctx, stage, Some(self.player.clone()))?;
        let bullets = BulletManager::register(ctx, self.player.clone())?;
        let enemies =
            EnemyManager::register(ctx, stage, Some(stage_t.clone()), bullets.clone(), d as i32)?;
        if !first {
            stage_t.borrow_mut().fast_forward = 0;
            enemies.borrow_mut().fast_forward = 0;
        }
        let effects = crate::effect::EffectManager::register(ctx)?;
        let hud = Hud::register(
            ctx,
            self.player.clone(),
            enemies.clone(),
            stage_t.clone(),
            hud_fresh,
        )?;
        let gui = crate::gui::Gui::register(ctx, self.player.clone(), enemies.clone())?;
        enemies.borrow_mut().msg = Some(hud.borrow().msg.clone());
        if fresh && !ctx.game.replay() {
            ctx.game.rpy = None;
        }
        let st = self.player.borrow().stats.clone();
        let replay_t = crate::replay::register(ctx, ctx.game.replay(), &st)?;
        // 11 §3.1 / 06 §2 +0x290: load (not play) BGM slot 0, and slots 1/2 unless their name starts with ' ';
        // spell practice loads slot 0 only, chosen by card id. Slot 0 is played by GameMgr_Calc step 3.
        if !ctx.game.reload_same {
            let names = stage_t.borrow().std.bgm.clone();
            if ctx.game.spell_practice() {
                ctx.audio
                    .load_bgm(0, spell_bgm(ctx.game.card).unwrap_or(&names[0]));
            } else {
                ctx.audio.load_bgm(0, &names[0]);
                for (k, name) in names.iter().enumerate().take(3).skip(1) {
                    if !name.starts_with(' ') {
                        ctx.audio.load_bgm(k, name);
                    }
                }
            }
        }
        self.music_start = music_start(&ctx.game);
        ctx.game.reload_same = false;
        let g = &mut ctx.game;
        g.flags |= F_RUNNING;
        g.game_over = 0;
        g.flags &= !F_ENDING_OK;
        self.tasks = vec![
            self.player.clone(),
            stage_t.clone(),
            bullets.clone(),
            enemies.clone(),
            effects,
            hud.clone(),
            gui,
            replay_t,
        ];
        let total = loading_total(&ctx.game, ctx.anm.entries_loaded - entries0);
        self.loading_total = total;
        self.gauge_at = Some(((total + 1).saturating_sub(k).max(1), l.min, l.max));
        self.stage = Some(stage_t);
        self.bullets = Some(bullets);
        self.enemies = Some(enemies);
        self.hud = Some(hud);
        // 0x18B8A2C := 2 at the end of StageStart (13 §3).
        crate::enemy::set_load_frames(2);
        Ok(())
    }

    // Stage-end handling (11 §4 step 1); the clear-mask bits are set unless replaying. On the clear frame loading
    // := 1 and the practice / 6A–Extra / night-over paths return 3 before the confirm test (it runs next frame).
    fn stage_end(g: &mut Game, st: &mut Stats, confirm: bool) -> Option<Ret> {
        if g.end_state() == 2 {
            g.set_end_state(3);
            g.loading = 1;
            g.next_scene_after = -1;
            let (shot, stage) = (g.shot as usize % 12, g.stage % 9);
            if !g.replay() {
                g.set_clear(st.continues == 0, 1 << stage);
            }
            st.score_disp = st.score10;
            if g.practice() {
                g.next_scene_after = SCENE_RESULT;
                return Some(Ret::StopFrame);
            } else if stage >= 6 {
                if g.replay() {
                    g.next_scene_after = SCENE_REPLAY_END;
                } else if g.difficulty >= 4 {
                    g.clear_nc[shot][4] |= 0x8000;
                    g.clear_any[12][4] |= 0x8000;
                    g.play_counter(crate::scoredat::PL_CLEARS);
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_RESULT;
                    return Some(Ret::StopFrame);
                } else {
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_ENDING;
                    return Some(Ret::StopFrame);
                }
            } else if g.replay() {
                let next = g
                    .rpy
                    .as_ref()
                    .and_then(|r| (stage + 1..9).find(|&s| r.stages[s].is_some()));
                match next {
                    Some(s) => {
                        g.stage = s;
                        g.request(SCENE_NEXT_STAGE);
                    }
                    None => g.next_scene_after = SCENE_REPLAY_END,
                }
            } else if st.clock > 11 {
                g.next_scene_after = SCENE_ENDING;
                g.flags &= !F_ENDING_OK;
                return Some(Ret::StopFrame);
            } else {
                g.stage = next_stage(stage, g.shot, g.route());
                g.request(SCENE_NEXT_STAGE);
            }
        }
        if g.end_state() != 0 && (confirm || g.replay() || g.stage % 9 >= 6) {
            g.set_end_state(0);
            if g.next_scene_after >= 0 {
                g.request(g.next_scene_after);
                return Some(Ret::StopFrame);
            }
        }
        None
    }

    // Continue sequence side effects requested by the continue menu (11 §4.2): frame 15 clock + 1; frame 90
    // continues + 1, score = display = continues, lives (checksum), bombs, point items, power 0 (checksum).
    fn continue_effects(&mut self, ctx: &mut Ctx) {
        let g = &mut ctx.game;
        let mut p = self.player.borrow_mut();
        if std::mem::take(&mut g.continue_clock) {
            p.stats.clock += 1;
        }
        if std::mem::take(&mut g.continue_apply) {
            let bombs = p.sht.bombs.trunc();
            let rng = &mut ctx.anm.rng;
            let st = &mut p.stats;
            st.continues += 1;
            if g.counts_plays() {
                g.count_play();
                g.play_counter(crate::scoredat::PL_CONTINUES);
            }
            st.score10 = st.continues as i64;
            st.score_disp = st.score10;
            st.graze = 0;
            st.lives = g.game_lives() as f32;
            Stats::checksum(rng);
            st.bombs = bombs;
            st.point_items = 0;
            st.point_items2 = 0;
            st.set_power(0.0, rng);
            st.extends = 0;
            st.next_extend = 100;
            ctx.audio.pause_bgm(false);
        }
        g.clock = p.stats.clock;
    }

    // Score display roll (11 §4 step 9): step = max(step, clamp((score − disp) >> 5, 1, 578910)).
    fn roll_score(st: &mut Stats) {
        st.score10 = st.score10.min(999_999_999);
        let d = ((st.score10 - st.score_disp) >> 5).clamp(1, 578_910);
        st.score_step = st.score_step.max(d);
        if st.score_disp + st.score_step > st.score10 {
            st.score_step = st.score10 - st.score_disp;
        }
        st.score_disp += st.score_step;
        if st.score_disp >= st.score10 {
            st.score_disp = st.score10;
            st.score_step = 0;
        }
        if st.score_disp > st.hiscore {
            st.hiscore = st.score_disp;
            st.hiscore_cont = st.continues;
        }
    }
}

impl Task<Ctx> for GameManager {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.stage_start(ctx)?;
        }
        Ok(())
    }

    // GM on-remove 0x43BE2C (11 §7): score clamped and shown, game speed 1, music stopped (unless reloading the
    // same stage), subsystems torn down.
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        // 0x18B8A2C := 1 (13 §3).
        crate::enemy::set_load_frames(1);
        // Darkness-mask alpha 0x4E3D28 := 0 (13 §4.1).
        crate::ascii::clear_darkness();
        let mut st = self.player.borrow().stats.clone();
        st.score10 = st.score10.min(999_999_999);
        st.score_disp = st.score10;
        ctx.game.stats = st;
        ctx.anm.speed = 1.0;
        if !ctx.game.reload_same {
            ctx.audio.stop_bgm();
            // Mode 2: the init.mid reset right after Music_Stop (02 §5).
            ctx.audio.midi_reset();
        }
        ctx.audio.pause_bgm(false);
        for t in std::mem::take(&mut self.tasks) {
            task::remove_task(ctx, &t);
        }
        if ctx.game.replay() {
            ctx.game.play_clock = None;
        } else {
            ctx.game.add_play_time();
        }
        ctx.game.flags &= !F_RUNNING;
        ctx.game.paused = 0;
        ctx.game.game_over = 0;
        // gm+0x2D := 0 (0x43BF5C, 11 §4 step 10).
        ctx.game.slow_skip = 0;
        // 0x164D2C8 := 0 as the last statement (0x43BE2C), so the menus after a pause-quit read 0 (11 §2).
        ctx.game.frame_counter = 0;
    }

    // GameMgr_Calc 0x439BC7 (11 §4); Ret::StopFrame = return 3 (the rest of the calc chain does not run).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // 0x164D09C += 1 at the top of GameMgr_Calc (11 §4): HSCR play time in frames.
        self.player.borrow_mut().stats.rec.frames += 1;
        self.continue_effects(ctx);
        let cleared = ctx.game.end_state() == 2;
        let confirm = ctx.input.pressed(crate::input::SHOT | crate::input::ENTER);
        let r = Self::stage_end(&mut ctx.game, &mut self.player.borrow_mut().stats, confirm);
        // Clear frame with next_scene_after < 0 (stage 1–5 advance, replay next stage): Hud_StageTransition (11 §4 step 1).
        if cleared
            && ctx.game.next_scene_after < 0
            && let Some(h) = &self.hud
        {
            h.borrow_mut().stage_transition(&mut ctx.anm);
        }
        self.cleared |= cleared;
        if let Some(r) = r {
            return r;
        }
        let g = &mut ctx.game;
        g.loading_shown = g.loading != 0 && !self.cleared;
        if g.loading != 0 {
            g.loading_frames += 1;
            // After a clear no loader thread runs for this GM: loading stays 1 until the scene switch (11 §4 step 1).
            if self.cleared {
                return Ret::StopFrame;
            }
            // Port-only session replay: the loader's recorded steps take effect before the calc of their poll index.
            let (gauge_due, wait_due, release_due) = match g
                .sched
                .as_ref()
                .and_then(|s| Some((s.poll + 1, *s.stages.front()?)))
            {
                Some((next, [gauge, wait, release])) => {
                    (next >= gauge, next >= wait, next >= release)
                }
                None => {
                    let done = g.loading_frames >= self.loading_total;
                    let gauge = self.gauge_at.is_some_and(|a| g.loading_frames >= a.0);
                    (gauge, done, done)
                }
            };
            if let Some((_, min, max)) = self.gauge_at
                && gauge_due
            {
                self.gauge_at = None;
                crate::ascii::gauge_setup(&mut ctx.anm, min, max);
            }
            let g = &mut ctx.game;
            if wait_due || release_due {
                if release_due && g.end_state() == 0 {
                    g.loading = 0;
                    g.loading_wait = 0;
                    g.flags &= !F_CLEAR_PANEL;
                    crate::effect::release_loading_overlay();
                    if let Some(s) = &mut g.sched {
                        s.stages.pop_front();
                    }
                } else {
                    if g.loading_wait == 0 {
                        crate::effect::release_loading_overlay();
                    }
                    g.loading_wait += 1;
                }
            }
            return Ret::StopFrame;
        }
        // Step 3: deferred music start (gm+0x3DE28), Music_PlaySlot of slot 0 on 1.
        if std::mem::take(&mut self.music_start) == 1 {
            ctx.audio.play_slot(0);
        }
        let g = &mut ctx.game;
        // Step 4 pause: sfx 0x22, music paused, pauses + 1, checksum with the RNG saved and restored.
        if g.paused == 0
            && g.game_over == 0
            && g.flags & F_DEMO == 0
            && g.slow_skip == 0
            && ctx.input.pressed(crate::input::PAUSE)
        {
            g.paused = 1;
            // HSCR +0x7C pauses (GM+0x3DAA8++, 15 §2.3).
            self.player.borrow_mut().stats.rec.pauses += 1;
            g.add_play_time();
            crate::sound::play(crate::sound::PAUSE);
            ctx.audio.pause_bgm(true);
            let s = ctx.anm.rng.state;
            Stats::checksum(&mut ctx.anm.rng);
            ctx.anm.rng.state = s;
        }
        let msg_active = self
            .hud
            .as_ref()
            .is_some_and(|h| h.borrow().msg.borrow().active());
        let g = &mut ctx.game;
        // Step 5 (0x43A24A): replay "slowdown reproduction" (15 §3.3) runs a frame only on a pattern of the recorded
        // fps, never while dialogue is active; a skipped frame leaves gm+0x2D as it is.
        if g.replay() && g.replay_speed == 1 && !msg_active {
            g.skip_ctr = g.skip_ctr.wrapping_add(1);
            if !playback_runs(g.replay_fps, g.skip_ctr) {
                return Ret::StopFrame;
            }
        }
        if g.flags & F_DEMO != 0 {
            // Attract demo (11 §4 step 6).
            g.demo_frame += 1;
            let (fade, end) = DEMO_LIMITS[g.demo as usize % 4];
            if ctx.input.held & !ctx.input.prev != 0 {
                g.request(SCENE_MENU);
            }
            if g.demo_frame == fade {
                ctx.audio.fade_bgm(3.0, ctx.anm.speed);
                crate::effect::screen_now(ctx, 2, end - fade, 0, 0, 0, 0x15);
            }
            let g = &mut ctx.game;
            if g.demo_frame >= end {
                g.request(SCENE_MENU);
                return Ret::StopFrame;
            }
        }
        // GameMgr_Calc's own z clear (11 §3.1), the previous frame left the viewport full-screen.
        ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
        ctx.gfx.clear_depth(None);
        let g = &mut ctx.game;
        // Step 7 (0x43A5BC): bit 2 := not paused and no game-over menu, so it is already 0 on the ESC frame (11 §2.1).
        if g.paused == 0 && g.game_over == 0 {
            g.flags |= F_RUNNING;
        } else {
            g.flags &= !F_RUNNING;
        }
        if g.paused != 0 || g.game_over != 0 {
            return Ret::StopFrame;
        }
        let mut p = self.player.borrow_mut();
        Self::roll_score(&mut p.stats);
        p.stats.rank_delta = 0;
        drop(p);
        if let Some(e) = &self.enemies {
            let mut e = e.borrow_mut();
            e.g.player_char = g.shot as i32;
        }
        // Step 10 slow mode (0x43A92D): skip on a pattern of the live-bullet count from the last bullet calc.
        if g.game_slow() != 0 {
            g.slow_skip = 0;
            g.skip_ctr = g.skip_ctr.wrapping_add(1);
            let n = self.bullets.as_ref().map_or(0, |b| b.borrow().live);
            if slow_skips(n, g.skip_ctr) {
                g.slow_skip = 1;
                return Ret::StopFrame;
            }
            if n < 128 {
                g.skip_ctr = 0;
            }
        }
        g.frame_counter += 1;
        // Port-only debug: TH08_SCREEN_FX=<frame>:<type>,<p0>,<p1>,<p2>,<p3>.
        if let Some(n) = ctx.settings.screen_fx {
            if n[0] == g.frame_counter as i64 {
                crate::effect::screen(
                    n[1] as u8,
                    n[2] as i32,
                    n[3] as i32,
                    n[4] as i32,
                    n[5] as i32,
                    0x15,
                );
            }
        }
        Ret::Next
    }

    // GameMgr_Draw 0x43AA03 (11 §3): return 3 (nothing above prio 5) when the requested scene 0x17CE8B4 ≠ 2,
    // bits 5–6 == 1 or loading ≠ 0, post-clear frames included. The port's in-calc release stands for the loader
    // thread's after this frame, so `loading_shown` keeps that frame a loading one. In Loading_Wait "Press Shot
    // Button" from the 5th frame, alpha pulsing with a 59-frame loop (12 §1).
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let g = &ctx.game;
        if g.requested == SCENE_GAME && g.end_state() != 1 && g.loading == 0 && !g.loading_shown {
            return Ret::Next;
        }
        if !g.loading_shown {
            return Ret::StopFrame;
        }
        let w = ctx.game.loading_wait;
        if w >= 5 {
            let t = (w - 5) % 59;
            let a = if t < 30 {
                255 - t * 128 / 30
            } else {
                127 + (t - 30) * 128 / 29
            };
            crate::ascii::draw_now(
                &ctx.anm,
                &mut ctx.gfx,
                crate::ascii::Text::new(288.0, 454.0, "Press Shot Button")
                    .scale(0.5)
                    .color([255, 255, 255, a as u8]),
            );
        }
        Ret::StopFrame
    }
}

// Loader minimum 31 / 81 calcs (11 §3.1), floored by one texture upload per frame; +6 fits stage 1 (42 → 48).
fn loading_total(g: &Game, entries: u32) -> u32 {
    let min = if g.replay() { 81 } else { 31 };
    min.max(entries + 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 11 §4 step 5: fps < 20 runs 1 of 3, 20–29 1 of 2, 30–39 2 of 3, 40–49 5 of 6, ≥ 50 every frame.
    #[test]
    fn playback_slowdown_pattern() {
        let runs = |f| (1..=6).filter(|&c| playback_runs(f, c)).count();
        assert_eq!(
            [runs(10), runs(25), runs(35), runs(45), runs(60)],
            [2, 3, 4, 5, 6]
        );
        assert!(!playback_runs(10, 1) && playback_runs(10, 3));
    }

    // 11 §4 step 10: every 3rd / 4th / 5th frame at ≥ 320 / 224 / 128 live bullets, never below 128.
    #[test]
    fn slow_mode_pattern() {
        let skips = |n| (1..=60).filter(|&c| slow_skips(n, c)).count();
        assert_eq!(
            [
                skips(320),
                skips(319),
                skips(224),
                skips(223),
                skips(128),
                skips(127)
            ],
            [20, 15, 15, 12, 12, 0]
        );
    }

    // 02 §4 table 0x4C7670: first record with max spell id >= the card id.
    #[test]
    fn spell_practice_bgm() {
        let names = [0, 1, 2, 12, 13, 146, 147, 222, 223].map(spell_bgm);
        assert_eq!(
            names,
            [
                Some("bgm/th08_00.mid"),
                Some("bgm/th08_00.mid"),
                Some("bgm/th08_03.mid"),
                Some("bgm/th08_03.mid"),
                Some("bgm/th08_04.mid"),
                Some("bgm/th08_13b.mid"),
                Some("bgm/th08_13.mid"),
                Some("bgm/th08_20.mid"),
                None
            ]
        );
    }

    #[test]
    fn routing() {
        let run = |shot: u8, route: u8| {
            let mut s = vec![0];
            while *s.last().unwrap() < 6 {
                s.push(next_stage(*s.last().unwrap(), shot, route));
            }
            s
        };
        assert_eq!(run(0, 0), [0, 1, 2, 4, 5, 6]);
        assert_eq!(run(1, 1), [0, 1, 2, 3, 5, 7]);
        assert_eq!(run(2, 0), [0, 1, 2, 3, 5, 6]);
        assert_eq!(run(3, 1), [0, 1, 2, 4, 5, 7]);
        for (shot, four) in [
            (4, 4),
            (5, 4),
            (6, 3),
            (7, 3),
            (8, 3),
            (9, 3),
            (10, 4),
            (11, 4),
        ] {
            assert_eq!(next_stage(2, shot, 0), four, "shot {shot}");
        }
    }

    #[test]
    fn clock_and_rank() {
        assert_eq!(
            (
                clock_advance(0, 100, 2000),
                clock_advance(0, 2000, 2000),
                clock_advance(6, 0, 0),
                clock_advance(8, 0, 10)
            ),
            (2, 1, 0, 4)
        );
        set_rank_state([10, 16, 8, 0]);
        rank_add(250);
        assert_eq!(rank_state(), [12, 16, 8, 50]);
        rank_add(-1600);
        assert_eq!(rank(), 8);
        rank_add(10_000);
        assert_eq!(rank(), 16);
    }

    // 11 §3.1 / §3.2: GameMgr_InitRank writes rank / min / max only; every stage start zeroes the sub-counter.
    #[test]
    fn rank_init_and_stage_sub_reset() {
        set_rank_state([12, 16, 8, 70]);
        init_rank(2);
        assert_eq!(rank_state(), [8, 12, 8, 70]);
        set_rank_state([14, 16, 8, 70]);
        clear_rank_sub();
        assert_eq!(rank_state(), [14, 16, 8, 0]);
    }

    // 11 §2 gm+0x3DE28: 2 only for a spell-practice reload of a card whose 0x4C7670 flag is 0 (th08_13b ones are 1).
    #[test]
    fn deferred_music_value() {
        let mut g = Game::default();
        set_flags(0);
        g.card = 10;
        assert_eq!(music_start(&g), 1);
        g.reload_same = true;
        assert_eq!(music_start(&g), 1);
        g.flags |= F_SPELL_PRACTICE;
        assert_eq!(music_start(&g), 2);
        for c in [143, 146, 171, 190] {
            g.card = c;
            assert_eq!(music_start(&g), 1);
        }
        g.card = 147;
        assert_eq!(music_start(&g), 2);
        g.reload_same = false;
        assert_eq!(music_start(&g), 1);
        set_flags(0);
    }

    // 11 §2.1: bits 7 / 8 / 13 written outside the GM are in the one flags word `Game::flags` reads.
    #[test]
    fn game_flags_one_word() {
        let mut g = Game::default();
        assert_eq!(g.flags.get(), 0);
        set_flags(F_LAST_SPELL | F_13);
        assert_eq!(g.flags & (F_LAST_SPELL | F_13), F_LAST_SPELL | F_13);
        g.flags &= !0x2180;
        assert_eq!(flags(), 0);
        g.flags |= F_SPELL_PRACTICE;
        assert_eq!(flags(), F_SPELL_PRACTICE);
    }

    // RANK_STATE before the first GameMgr_InitRank: GameMgr_Ctor zeroes the GM (11 §2).
    #[test]
    fn rank_boot_state() {
        assert_eq!(std::thread::spawn(rank_state).join().unwrap(), [0; 4]);
    }

    // 11 §4 step 1: the clear frame sets loading and score_display; the returning paths skip the confirm test.
    #[test]
    fn clear_frame() {
        let clear = |flags: u32, stage: usize, clock: i32, confirm: bool| {
            let mut g = Game {
                stage,
                ..Game::default()
            };
            g.flags.set(flags);
            g.set_end_state(2);
            let mut st = Stats {
                score10: 1234,
                clock,
                ..Stats::default()
            };
            let r = GameManager::stage_end(&mut g, &mut st, confirm);
            assert_eq!((g.loading, st.score_disp), (1, 1234));
            (r, g)
        };
        // Practice, Extra, 6A, night over: return 3 with the bits still 3; the next frame confirms.
        for (flags, stage, clock, next) in [
            (F_PRACTICE, 1, 0, SCENE_RESULT),
            (0, 6, 0, SCENE_ENDING),
            (0, 2, 12, SCENE_ENDING),
        ] {
            let (r, mut g) = clear(flags, stage, clock, true);
            assert_eq!(
                (r, g.end_state(), g.requested),
                (Some(Ret::StopFrame), 3, SCENE_BOOT)
            );
            let mut st = Stats::default();
            let r = GameManager::stage_end(&mut g, &mut st, true);
            assert_eq!(
                (r, g.end_state(), g.requested),
                (Some(Ret::StopFrame), 0, next)
            );
        }
        // Stage 1–5 advance: confirm tested on the clear frame itself, then the loading branch runs.
        let (r, g) = clear(0, 0, 0, true);
        assert_eq!((r, g.end_state(), g.requested), (None, 0, SCENE_NEXT_STAGE));
        let (r, g) = clear(0, 0, 0, false);
        assert_eq!((r, g.end_state(), g.stage), (None, 3, 1));
    }

    #[test]
    fn score_roll() {
        let mut st = Stats {
            score10: 1000,
            ..Stats::default()
        };
        let mut frames = 0;
        while st.score_disp < st.score10 {
            GameManager::roll_score(&mut st);
            frames += 1;
        }
        assert_eq!((st.score_disp, st.score_step), (1000, 0));
        assert!(frames > 1 && frames < 200, "{frames}");
    }
}
