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

// Game flags (11 §2.1).
pub const F_PRACTICE: u32 = 1 << 0;
pub const F_DEMO: u32 = 1 << 1;
pub const F_RUNNING: u32 = 1 << 2;
pub const F_REPLAY: u32 = 1 << 3;
pub const F_ENDING_OK: u32 = 1 << 4;
pub const F_CLEAR_PANEL: u32 = 1 << 9;
pub const F_DEATH: u32 = 1 << 10;
pub const F_SPELL_PRACTICE: u32 = 1 << 14;
const END_SHIFT: u32 = 5;
const ROUTE_SHIFT: u32 = 11;

pub const CALC_PRIO: u16 = 2;
pub const DRAW_PRIO: u16 = 5;

// Rank init / min / max by difficulty (table 0x4C7880).
const RANK: [[i32; 3]; 5] = [
    [10, 8, 16],
    [10, 8, 16],
    [8, 8, 12],
    [8, 8, 12],
    [16, 15, 16],
];

// Demo music-fade and end frames per demo index (11 §4).
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

// Game state that survives scene changes (GM object 11 §2 + the menu's choices).
pub struct Game {
    pub scene: i32,
    pub requested: i32,
    pub previous: i32,
    pub difficulty: i32,
    pub shot: u8,
    pub stage: usize,
    pub flags: u32,
    pub paused: u8,
    pub game_over: u8,
    pub fresh_start: bool,
    pub reload_same: bool,
    pub next_scene_after: i32,
    pub loading: u8,
    pub loading_frames: u32,
    pub loading_shown: bool,
    pub loading_wait: u32,
    pub frame_counter: u32,
    pub play_time: u32,
    pub cfg: crate::config::Config,
    pub stats: Stats,
    pub clear_nc: [[u16; 5]; 13],
    pub clear_any: [[u16; 5]; 13],
    pub score: crate::scoredat::ScoreDat,
    pub persist: bool,
    catk_backup: Option<Vec<crate::scoredat::Catk>>,
    run_clock: Option<web_time::Instant>,
    pub play_clock: Option<web_time::Instant>,
    pub retrying: bool,
    pub demo: u8,
    pub demo_frame: i32,
    pub demo_shot_bias: u8,
    pub replay_speed: u8,
    pub rpy: Option<crate::replay::Replay>,
    pub cfg_live: Option<crate::config::Config>,
    pub replay_ff: bool,
    pub dialogue_skippable: bool,
    pub replay_fps: u8,
    pub watched_replay: bool,
    slow_phase: u32,
    pub stage_seed: u16,
    pub stages_started: u32,
    pub frame_skip: bool,
    pub clock: i32,
    pub continue_clock: bool,
    pub continue_apply: bool,
    pub card: i16,
    pub menu_loading: u32,
    // Recorded loader timing of a session replay (src/session.rs).
    pub sched: Option<crate::session::Sched>,
    pub trace_menu: Option<(i32, i32)>,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            scene: SCENE_BOOT,
            requested: SCENE_BOOT,
            previous: SCENE_BOOT,
            difficulty: 1,
            shot: 0,
            stage: 0,
            flags: 0,
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
            play_time: 0,
            cfg: Default::default(),
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
            cfg_live: None,
            replay_ff: true,
            dialogue_skippable: false,
            replay_fps: 60,
            watched_replay: false,
            slow_phase: 0,
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
        }
    }
}

impl Game {
    pub fn end_state(&self) -> u32 {
        (self.flags >> END_SHIFT) & 3
    }

    pub fn set_end_state(&mut self, v: u32) {
        self.flags = self.flags & !(3 << END_SHIFT) | (v & 3) << END_SHIFT;
    }

    pub fn route(&self) -> u8 {
        ((self.flags >> ROUTE_SHIFT) & 3) as u8
    }

    pub fn set_route(&mut self, c: u8) {
        self.flags = self.flags & !(3 << ROUTE_SHIFT) | ((c & 3) as u32) << ROUTE_SHIFT;
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

    pub fn request(&mut self, scene: i32) {
        self.requested = scene;
    }

    fn diff(&self) -> usize {
        self.difficulty.clamp(0, 4) as usize
    }

    // Debug `TH08_CLRD=<shot>,<any>[,<nc>]`: sets that shot's clear masks on every difficulty.
    pub fn debug_clear_masks(&mut self) {
        let Some(v) = crate::env::var("TH08_CLRD") else {
            return;
        };
        let num = |t: &str| {
            t.strip_prefix("0x")
                .map_or_else(|| t.parse().ok(), |h| u16::from_str_radix(h, 16).ok())
        };
        let p: Vec<u16> = v.split(',').filter_map(num).collect();
        if let [shot, any, ref rest @ ..] = p[..] {
            self.clear_any[shot as usize % 12] = [any; 5];
            self.clear_nc[shot as usize % 12] = [rest.first().copied().unwrap_or(0); 5];
        }
    }

    pub fn cfg_bytes(&self) -> [u8; 60] {
        self.cfg.to_bytes()
    }

    // Boot load (15 §2.2); a missing or invalid file starts from defaults.
    pub fn load_score(&mut self, file: Option<&[u8]>) {
        self.score = match file.map(crate::scoredat::ScoreDat::parse) {
            Some(Ok(s)) => s,
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
    pub fn save_score(&mut self, rng: &mut crate::rng::Rng, flush: bool) {
        if flush && self.cfg.slow != 0 {
            if let Some(b) = self.catk_backup.clone() {
                self.score.catk = b;
            }
        }
        for (i, c) in self.score.clrd.iter_mut().enumerate() {
            c.nc = self.clear_nc[i];
            c.any = self.clear_any[i];
        }
        if let Some(t) = self.run_clock.replace(web_time::Instant::now()) {
            crate::scoredat::Plst::add_time(
                &mut self.score.plst.run_time,
                t.elapsed().as_millis() as u64,
            );
        }
        let bytes = self.score.encode(rng);
        if self.persist {
            crate::scoredat::storage_write("score.dat", &bytes);
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

    // The cfg copy's slow byte is forced to 0 for Extra and practice (15 §2.9).
    pub fn counts_plays(&self) -> bool {
        let slow = if self.difficulty >= 4 || self.practice() {
            0
        } else {
            self.cfg.slow
        };
        !self.replay() && !self.spell_practice() && slow == 0
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
    // GM rank, max, min, sub-counter (GM+0x3DE2C..0x3DE38); ECL var 10041 reads the rank directly.
    static RANK_STATE: std::cell::Cell<[i32; 4]> = const { std::cell::Cell::new([16, 32, 0, 0]) };
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

// RankAdd / RankSub (11 §3.2): ±1 rank per 100 sub-units, clamped to min..max; applied immediately.
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

// Stage_Advance (11 §4.1).
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

// GameMgr_ClockAdvance (11 §4.1).
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

pub struct GameManager {
    pub player: PlayerRef,
    pub stage: Option<Rc<RefCell<Stage>>>,
    pub bullets: Option<Rc<RefCell<BulletManager>>>,
    pub enemies: Option<Rc<RefCell<EnemyManager>>>,
    pub hud: Option<Rc<RefCell<Hud>>>,
    tasks: Vec<TaskRef<Ctx>>,
    gauge_at: Option<(u32, i32, i32)>,
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
        }));
        CURRENT.with_borrow_mut(|c| *c = Rc::downgrade(&gm));
        let t: TaskRef<Ctx> = gm.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(gm)
    }

    // New game (11 §3.2): RNG jitter + decoys, stats defaults, rank.
    fn fresh_start(ctx: &mut Ctx, st: &mut Stats) {
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
        *st = Stats {
            difficulty: d as i32,
            ..Stats::default()
        };
        st.gauge = 0;
        st.clock = if g.stage % 9 == 8 { 6 } else { 0 };
        let lives = if d >= 4 {
            2
        } else if g.practice() {
            8
        } else {
            g.cfg.lives as i32
        };
        if !g.replay() {
            st.lives = lives as f32;
            Stats::checksum(rng);
        }
        st.power = 0.0;
        Stats::checksum(rng);
        g.play_time = 0;
        (st.score10, st.score_disp, st.score_step) = (0, 0, 0);
        st.hiscore = 100000;
        st.continues = 0;
        st.graze_total = 0;
        st.point_items2 = 0;
        st.max_point = crate::player::MAX_POINT[d];
        st.extends = 0;
        st.next_extend = crate::item::next_extend(d as i32, 0);
        // GameMgr_LoadScoreData: flush (Result mode 2), hiscore, practice play count, CATK backup.
        g.save_score(rng, true);
        let shot = g.shot as usize % 12;
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
        let [init, min, max] = RANK[d];
        set_rank_state([init, max, min, 0]);
        st.deaths = 0;
        Stats::checksum(rng);
        st.bombs_used = 0;
        Stats::checksum(rng);
        st.spells = 0;
    }

    // GameManager_StageStart (11 §3.1) with the subsystems registered synchronously.
    fn stage_start(&mut self, ctx: &mut Ctx) -> Result<()> {
        let g = &mut ctx.game;
        g.loading = 1;
        g.loading_frames = 0;
        if g.end_state() >= 2 {
            g.set_end_state(1);
        }
        ctx.anm.speed = 1.0;
        g.flags &= !F_DEATH;
        let fresh = g.fresh_start || g.practice() || g.spell_practice() || g.difficulty >= 4;
        let mut st = std::mem::take(&mut ctx.game.stats);
        if fresh {
            Self::fresh_start(ctx, &mut st);
        } else {
            st.score_disp = st.score10;
            st.score_step = 0;
            Stats::checksum(&mut ctx.anm.rng);
            Stats::checksum(&mut ctx.anm.rng);
        }
        let g = &mut ctx.game;
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
        g.fresh_start = false;
        let (stage, shot, d) = (g.stage, g.shot, g.diff());
        Player::register(ctx, self.player.clone(), shot)?;
        // Player_Load shows the gauge readout on a fresh start; StageStart re-runs its setup (12 §5.9).
        // SPEC-GAP: setup order vs Player_Load inside StageStart is unspecified; run after it so the limits are current.
        if fresh {
            crate::ascii::gauge_interrupt(1);
        }
        let l = self.player.borrow().stats.limits;
        crate::ascii::gauge_player(&self.player);
        // The loader thread reaches Ascii_SetupGaugeVms k Ascii_Calc ticks before game frame 1 (answers_impl15 §Q7).
        let demo = (ctx.game.flags & F_DEMO != 0).then_some(ctx.game.demo);
        let k = match demo {
            Some(0) => 28,
            Some(1) => 27,
            Some(2) => 29,
            Some(3) => 33,
            _ => 28,
        };
        let total = loading_total(&ctx.game);
        self.gauge_at = Some(((total + 1).saturating_sub(k).max(1), l.min, l.max));
        {
            let mut p = self.player.borrow_mut();
            let limits = p.stats.limits;
            p.stats = st;
            p.stats.limits = limits;
            p.stats.difficulty = d as i32;
            p.gflags = ctx.game.flags & crate::player::GF_SPELL_PRACTICE;
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
            if let Some(pw) = crate::env::var("TH08_POWER")
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|_| ctx.game.stages_started == 0)
            {
                s.power = pw.clamp(0.0, 128.0);
            }
        }
        if ctx.game.replay() {
            let [init, min, max] = RANK[d];
            set_rank_state([init, max, min, 0]);
            let mut p = self.player.borrow_mut();
            crate::replay::play_stage_load(&mut ctx.game, &mut p.stats, &mut ctx.anm.rng)?;
        }
        let g = &mut ctx.game;
        g.paused = 0;
        g.flags &= !0x2180;
        g.stage_seed = ctx.anm.rng.state;
        log::info!(
            "stage {stage} start at frame {}: shot {shot} difficulty {d} seed {:#06x}",
            ctx.frame,
            g.stage_seed
        );
        let first = g.stages_started == 0;
        g.stages_started += 1;
        let stage_t = Stage::register(ctx, stage, Some(self.player.clone()))?;
        let bullets = BulletManager::register(ctx, self.player.clone())?;
        let enemies =
            EnemyManager::register(ctx, stage, Some(stage_t.clone()), bullets.clone(), d as i32)?;
        if !first {
            stage_t.borrow_mut().fast_forward = 0;
            enemies.borrow_mut().fast_forward = 0;
        }
        let hud = Hud::register(ctx, self.player.clone(), enemies.clone(), stage_t.clone())?;
        let effects = crate::effect::EffectManager::register(ctx)?;
        let gui = crate::gui::Gui::register(ctx, self.player.clone(), enemies.clone())?;
        enemies.borrow_mut().msg = Some(hud.borrow().msg.clone());
        if fresh && !ctx.game.replay() {
            ctx.game.rpy = None;
        }
        let st = self.player.borrow().stats.clone();
        let replay_t = crate::replay::register(ctx, ctx.game.replay(), &st)?;
        if !ctx.game.reload_same {
            let bgm = stage_t.borrow().std.bgm[0].clone();
            ctx.audio.play_bgm(&bgm);
        }
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
            hud.clone(),
            effects,
            gui,
            replay_t,
        ];
        self.stage = Some(stage_t);
        self.bullets = Some(bullets);
        self.enemies = Some(enemies);
        self.hud = Some(hud);
        Ok(())
    }

    // Stage-end handling (11 §4 step 1).
    fn stage_end(&mut self, ctx: &mut Ctx) -> Option<Ret> {
        let g = &mut ctx.game;
        if g.end_state() == 2 {
            g.set_end_state(3);
            g.next_scene_after = -1;
            let (shot, d, stage) = (g.shot as usize % 12, g.diff(), g.stage % 9);
            let st = &self.player.borrow().stats;
            let _ = d;
            if !g.replay() {
                g.set_clear(st.continues == 0, 1 << stage);
            }
            if g.practice() {
                g.next_scene_after = SCENE_RESULT;
            } else if stage >= 6 {
                if g.replay() {
                    g.next_scene_after = SCENE_REPLAY_END;
                } else if g.difficulty >= 4 {
                    g.clear_nc[shot][4] |= 0x8000;
                    g.clear_any[12][4] |= 0x8000;
                    g.play_counter(crate::scoredat::PL_CLEARS);
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_RESULT;
                } else {
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_ENDING;
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
            } else {
                g.stage = next_stage(g.stage % 9, g.shot, g.route());
                g.request(SCENE_NEXT_STAGE);
            }
        }
        let g = &mut ctx.game;
        if g.end_state() != 0 {
            let confirm = ctx.input.pressed(crate::input::SHOT | crate::input::ENTER);
            if confirm || g.replay() || g.stage % 9 >= 6 {
                g.set_end_state(0);
                if g.next_scene_after >= 0 {
                    g.request(g.next_scene_after);
                    return Some(Ret::StopFrame);
                }
            }
        }
        None
    }

    // Continue sequence side effects requested by the continue menu (11 §4.2).
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
            st.lives = g.cfg.lives as f32;
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

    // Score display roll (11 §4 step 9).
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

    // GM on-remove (11 §7): stats carried over, subsystems torn down, music stopped.
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain != Chain::Calc {
            return;
        }
        let mut st = self.player.borrow().stats.clone();
        st.score10 = st.score10.min(999_999_999);
        st.score_disp = st.score10;
        ctx.game.stats = st;
        ctx.anm.speed = 1.0;
        if !ctx.game.reload_same {
            ctx.audio.stop_bgm();
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
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.continue_effects(ctx);
        if let Some(r) = self.stage_end(ctx) {
            return r;
        }
        let g = &mut ctx.game;
        g.loading_shown = g.loading != 0;
        if g.loading != 0 {
            g.loading_frames += 1;
            // Session replay: the loader's recorded steps take effect before the calc of their poll index.
            let (gauge_due, wait_due, release_due) = match g
                .sched
                .as_ref()
                .and_then(|s| Some((s.poll + 1, *s.stages.front()?)))
            {
                Some((next, [gauge, wait, release])) => {
                    (next >= gauge, next >= wait, next >= release)
                }
                None => {
                    let done = g.loading_frames >= loading_total(g);
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
        {
            let mut p = self.player.borrow_mut();
            if std::mem::take(&mut p.game_over) {
                ctx.game.game_over = 1;
                ctx.game.add_play_time();
                log::info!("game over at frame {}", ctx.frame);
            }
        }
        let g = &mut ctx.game;
        if g.paused == 0
            && g.game_over == 0
            && g.flags & F_DEMO == 0
            && ctx.input.pressed(crate::input::PAUSE)
        {
            g.paused = 1;
            g.add_play_time();
            crate::sound::play(crate::sound::PAUSE);
            ctx.audio.pause_bgm(true);
            let s = ctx.anm.rng.state;
            Stats::checksum(&mut ctx.anm.rng);
            ctx.anm.rng.state = s;
        }
        let g = &mut ctx.game;
        // Replay "slowdown reproduction" (15 §3.3): run a frame only on a pattern given by the recorded fps.
        if g.replay() && g.replay_speed == 1 {
            let (run, of) = match g.replay_fps {
                0..20 => (1, 3),
                20..30 => (1, 2),
                30..40 => (2, 3),
                40..50 => (5, 6),
                _ => (1, 1),
            };
            g.slow_phase = (g.slow_phase + 1) % of;
            if g.slow_phase >= run {
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
                let gs = ctx.anm.speed;
                ctx.audio
                    .fade_bgm(if gs > 0.0 && gs <= 1.0 { 3.0 / gs } else { 3.0 });
                crate::effect::screen(2, end - fade, 0, 0, 0, 0x15);
            }
            if g.demo_frame >= end {
                g.request(SCENE_MENU);
                return Ret::StopFrame;
            }
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
        g.frame_counter += 1;
        if let Some(v) = crate::env::var("TH08_SCREEN_FX") {
            let n: Vec<i64> = v.split([':', ',']).filter_map(|s| s.parse().ok()).collect();
            if n.len() == 6 && n[0] == g.frame_counter as i64 {
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

    // While loading only the Supervisor's loading screen (draw prio 0–2) and the darken overlay are drawn.
    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if !ctx.game.loading_shown {
            return Ret::Next;
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

// Stage loading frames: replays measured 81 presents (answers_impl15 §Q7). SPEC-GAP: the original waits for its loader thread.
fn loading_total(g: &Game) -> u32 {
    if g.replay() { 81 } else { 30 }
}

#[cfg(test)]
mod tests {
    use super::*;

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
