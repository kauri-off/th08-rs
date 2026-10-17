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
// SPEC-GAP: the loading screen is drawn by a GM node at prio 16 (over the registered stage) instead of a loader thread.
pub const LOADING_DRAW_PRIO: u16 = 16;

// Rank init / min / max by difficulty (table 0x4C7880).
const RANK: [[i32; 3]; 5] = [[10, 8, 16], [10, 8, 16], [8, 8, 12], [8, 8, 12], [16, 15, 16]];

// Scenes (11 §7).
pub const SCENE_BOOT: i32 = 0;
pub const SCENE_MENU: i32 = 1;
pub const SCENE_GAME: i32 = 2;
pub const SCENE_NEXT_STAGE: i32 = 3;
pub const SCENE_RESULT: i32 = 6;
pub const SCENE_REPLAY_END: i32 = 7;
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
    pub frame_counter: u32,
    pub play_time: u32,
    pub rank: [i32; 4],
    pub config_lives: i32,
    pub stats: Stats,
    pub clear_nc: [[u16; 5]; 12],
    pub clear_any: [[u16; 5]; 12],
    pub stage_seed: u16,
    pub stages_started: u32,
    pub frame_skip: bool,
    pub clock: i32,
    pub continue_clock: bool,
    pub continue_apply: bool,
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
            frame_counter: 0,
            play_time: 0,
            rank: [16, 32, 0, 0],
            // SPEC-GAP: no config file yet; the option default (12 §2.7 "reset": lives 2) is used.
            config_lives: 2,
            stats: Stats::default(),
            clear_nc: [[0; 5]; 12],
            clear_any: [[0; 5]; 12],
            stage_seed: 0,
            stages_started: 0,
            frame_skip: false,
            clock: 0,
            continue_clock: false,
            continue_apply: false,
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

    pub fn replay(&self) -> bool {
        self.flags & F_REPLAY != 0
    }

    pub fn request(&mut self, scene: i32) {
        self.requested = scene;
    }

    fn diff(&self) -> usize {
        self.difficulty.clamp(0, 4) as usize
    }

    pub fn cleared(&self, any: bool, bit: usize) -> bool {
        let m = if any { &self.clear_any } else { &self.clear_nc };
        m[self.shot as usize % 12].iter().take(4).any(|v| v & (1 << bit) != 0)
    }

    // Rank sub-counter → rank (SPEC-GAP: ±100 sub-units per rank step, clamped to min..max).
    pub fn rank_add(&mut self, d: i32) {
        let [mut r, max, min, mut sub] = self.rank;
        sub += d;
        while sub >= 100 {
            sub -= 100;
            r += 1;
        }
        while sub < 0 {
            sub += 100;
            r -= 1;
        }
        self.rank = [r.clamp(min, max), max, min, sub];
    }
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

pub struct GameManager {
    pub player: PlayerRef,
    pub stage: Option<Rc<RefCell<Stage>>>,
    pub bullets: Option<Rc<RefCell<BulletManager>>>,
    pub enemies: Option<Rc<RefCell<EnemyManager>>>,
    pub hud: Option<Rc<RefCell<Hud>>>,
    tasks: Vec<TaskRef<Ctx>>,
    loading_vms: Vec<crate::anmvm::AnmVm>,
    wait_frames: u32,
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
            loading_vms: Vec::new(),
            wait_frames: 0,
        }));
        let t: TaskRef<Ctx> = gm.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, LOADING_DRAW_PRIO, 1, t)?;
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
        *st = Stats { difficulty: d as i32, ..Stats::default() };
        st.gauge = 0;
        st.clock = if g.stage % 9 == 8 { 6 } else { 0 };
        let lives = if d >= 4 {
            2
        } else if g.practice() {
            8
        } else {
            g.config_lives
        };
        st.lives = lives as f32;
        Stats::checksum(rng);
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
        let [init, min, max] = RANK[d];
        g.rank = [init, max, min, 0];
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
        g.fresh_start = false;
        let (stage, shot, d) = (g.stage, g.shot, g.diff());
        Player::register(ctx, self.player.clone(), shot)?;
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
            s.time_threshold = if ctx.game.spell_practice() { 0 } else { crate::player::TIME_THR.get(stage % 9).map_or(10, |r| r[d.min(3)]) };
            if ctx.game.practice() {
                let pw = match stage {
                    0 => 0.0,
                    1 => 112.0,
                    _ => 128.0,
                };
                s.set_power(pw, &mut ctx.anm.rng);
            }
            if let Some(pw) = crate::env::var("TH08_POWER").and_then(|v| v.parse::<f32>().ok()).filter(|_| ctx.game.stages_started == 0) {
                s.power = pw.clamp(0.0, 128.0);
            }
        }
        let g = &mut ctx.game;
        g.paused = 0;
        g.flags &= !0x2180;
        g.stage_seed = ctx.anm.rng.state;
        log::info!("stage {stage} start at frame {}: shot {shot} difficulty {d} seed {:#06x}", ctx.frame, g.stage_seed);
        let first = g.stages_started == 0;
        g.stages_started += 1;
        let stage_t = Stage::register(ctx, stage, Some(self.player.clone()))?;
        let bullets = BulletManager::register(ctx, self.player.clone())?;
        let enemies = EnemyManager::register(ctx, stage, Some(stage_t.clone()), bullets.clone(), d as i32)?;
        if !first {
            stage_t.borrow_mut().fast_forward = 0;
            enemies.borrow_mut().fast_forward = 0;
        }
        let hud = Hud::register(ctx, self.player.clone(), enemies.clone(), stage_t.clone())?;
        enemies.borrow_mut().msg = Some(hud.borrow().msg.clone());
        if !ctx.game.reload_same {
            let bgm = stage_t.borrow().std.bgm[0].clone();
            ctx.audio.play_bgm(&bgm);
        }
        ctx.game.reload_same = false;
        let g = &mut ctx.game;
        g.flags |= F_RUNNING;
        g.game_over = 0;
        g.flags &= !F_ENDING_OK;
        self.tasks = vec![self.player.clone(), stage_t.clone(), bullets.clone(), enemies.clone(), hud.clone()];
        self.stage = Some(stage_t);
        self.bullets = Some(bullets);
        self.enemies = Some(enemies);
        self.hud = Some(hud);
        self.loading_vms = (0..3)
            .map(|i| {
                let mut vm = crate::anmvm::AnmVm::default();
                ctx.anm.set_script(&mut vm, 2, i);
                vm.pos = [280.0, 430.0, 0.0];
                vm
            })
            .collect();
        self.wait_frames = 0;
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
            if !g.replay() {
                if st.continues == 0 {
                    g.clear_nc[shot][d] |= 1 << stage;
                }
                g.clear_any[shot][d] |= 1 << stage;
            }
            if g.practice() {
                g.next_scene_after = SCENE_RESULT;
            } else if stage >= 6 {
                if g.replay() {
                    g.next_scene_after = SCENE_REPLAY_END;
                } else if g.difficulty >= 4 {
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_RESULT;
                } else {
                    g.flags |= F_ENDING_OK;
                    g.next_scene_after = SCENE_ENDING;
                }
            } else if g.replay() {
                g.next_scene_after = SCENE_REPLAY_END;
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
            st.score10 = st.continues as i64;
            st.score_disp = st.score10;
            st.graze = 0;
            st.lives = g.config_lives as f32;
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
        if g.loading != 0 {
            g.loading_frames += 1;
            for vm in self.loading_vms.iter_mut().filter(|v| v.script.is_some()) {
                vm.run(&mut ctx.anm);
            }
            let g = &mut ctx.game;
            if g.loading_frames >= 30 {
                if g.end_state() == 0 {
                    g.loading = 0;
                    g.flags &= !F_CLEAR_PANEL;
                    self.loading_vms.iter_mut().for_each(|v| v.pending_interrupt = 1);
                } else {
                    if self.wait_frames == 0 {
                        self.loading_vms.iter_mut().for_each(|v| v.pending_interrupt = 2);
                    }
                    self.wait_frames += 1;
                }
            }
            return Ret::StopFrame;
        }
        {
            let mut p = self.player.borrow_mut();
            if std::mem::take(&mut p.game_over) {
                ctx.game.game_over = 1;
                log::info!("game over at frame {}", ctx.frame);
            }
        }
        let g = &mut ctx.game;
        if g.paused == 0 && g.game_over == 0 && g.flags & F_DEMO == 0 && ctx.input.pressed(crate::input::PAUSE) {
            g.paused = 1;
            ctx.audio.pause_bgm(true);
            let s = ctx.anm.rng.state;
            Stats::checksum(&mut ctx.anm.rng);
            ctx.anm.rng.state = s;
        }
        let g = &mut ctx.game;
        if g.paused != 0 || g.game_over != 0 {
            return Ret::StopFrame;
        }
        let mut p = self.player.borrow_mut();
        Self::roll_score(&mut p.stats);
        let delta = std::mem::take(&mut p.stats.rank_delta);
        drop(p);
        if let Some(e) = &self.enemies {
            let mut e = e.borrow_mut();
            g.rank[0] = e.g.rank;
            g.rank_add(delta);
            e.g.rank = g.rank[0];
            e.g.player_char = g.shot as i32;
        }
        g.frame_counter += 1;
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        if tag != 1 || ctx.game.loading == 0 {
            return Ret::Next;
        }
        ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
        ctx.gfx.rect(0.0, 0.0, 640.0, 480.0, [0, 0, 0, 255]);
        for vm in &mut self.loading_vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        if self.wait_frames >= 5 {
            let t = (self.wait_frames - 5) % 59;
            let a = if t < 30 { 255 - t * 128 / 30 } else { 127 + (t - 30) * 128 / 29 };
            ctx.ascii.push(crate::ascii::Text::new(288.0, 454.0, "Press Shot Button").scale(0.5).color([255, 255, 255, a as u8]));
        }
        Ret::Next
    }
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
        for (shot, four) in [(4, 4), (5, 4), (6, 3), (7, 3), (8, 3), (9, 3), (10, 4), (11, 4)] {
            assert_eq!(next_stage(2, shot, 0), four, "shot {shot}");
        }
    }

    #[test]
    fn clock_and_rank() {
        assert_eq!((clock_advance(0, 100, 2000), clock_advance(0, 2000, 2000), clock_advance(6, 0, 0), clock_advance(8, 0, 10)), (2, 1, 0, 4));
        let mut g = Game { rank: [10, 16, 8, 0], ..Game::default() };
        g.rank_add(250);
        assert_eq!(g.rank, [12, 16, 8, 50]);
        g.rank_add(-1600);
        assert_eq!(g.rank[0], 8);
        g.rank_add(10_000);
        assert_eq!(g.rank[0], 16);
    }

    #[test]
    fn score_roll() {
        let mut st = Stats { score10: 1000, ..Stats::default() };
        let mut frames = 0;
        while st.score_disp < st.score10 {
            GameManager::roll_score(&mut st);
            frames += 1;
        }
        assert_eq!((st.score_disp, st.score_step), (1000, 0));
        assert!(frames > 1 && frames < 200, "{frames}");
    }
}
