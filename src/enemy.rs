use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::{BulletDesc, BulletManager, LaserDesc};
use crate::player::PlayerRef;
use crate::ecl::{self, Ecl};
use crate::game::Ctx;
use crate::gfx::PLAYFIELD;
use crate::stage::Stage;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

pub const CALC_PRIO: u16 = 11;
pub const DRAW_LOW_PRIO: u16 = 8;
pub const DRAW_HIGH_PRIO: u16 = 11;
pub const ANM_ENEMY: usize = 7;
pub const ANM_STAGE: usize = 8;
pub const MAX_ENEMIES: usize = 480;

// Flags A (e+0x3324).
pub const A_ALIVE: u32 = 1 << 0;
pub const A_BOSS: u32 = 1 << 1;
pub const A_CONTACT: u32 = 1 << 2;
pub const A_LOSES_LIFE: u32 = 1 << 3;
pub const A_HIDDEN: u32 = 1 << 4;
pub const A_SHOT_HIT: u32 = 1 << 6;
pub const A_CANCEL_BONUS: u32 = 1 << 7;
pub const A_MODE_REACT: u32 = 1 << 8;
pub const A_FOLLOW: u32 = 1 << 9;
pub const A_CHAIN_KILLED: u32 = 1 << 10;
pub const A_YOUKAI: u32 = 1 << 11;
pub const A_MOVE_SHIFT: u32 = 12;
pub const A_EASE_SHIFT: u32 = 14;
pub const A_STORE_FIRE: u32 = 1 << 17;
pub const A_MIRROR: u32 = 1 << 18;
pub const A_CLAMP: u32 = 1 << 19;
pub const A_DEATH_SHIFT: u32 = 20;
pub const A_DEAD_CONT: u32 = 1 << 23;
pub const A_ON_SCREEN: u32 = 1 << 24;
pub const A_NO_PUSH: u32 = 1 << 26;
pub const A_FIXED_BONUS: u32 = 1 << 27;
pub const A_KEEP_OFFSCREEN: u32 = 1 << 28;
pub const A_NO_MOVE: u32 = 1 << 29;
// Flags B (e+0x3328).
pub const B_STAGE_ANM: u32 = 1 << 2;
pub const B_DEATH_LATCH: u32 = 1 << 3;
pub const B_CANNOT_DIE: u32 = 1 << 6;

const DROP_TABLE: [i32; 32] = [0, 0, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1, 0, 0];

const ECL_NAMES: [&str; 9] = ["1", "2", "3", "4a", "4b", "5", "6", "7", "8"];

pub fn files(index: usize) -> (String, String) {
    let k = ECL_NAMES[index % 9];
    let sp = if index >= 9 { "sp" } else { "" };
    (format!("ecldata{k}{sp}.ecl"), format!("stg{k}enm.anm"))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Interp {
    pub active: bool,
    pub elapsed: Timer,
    pub dur: i32,
    pub func: i32,
    pub mode: i32,
    pub abcd: [f32; 4],
    pub target: i32,
}

// Script context (0x228 bytes). `vars` = the 30 dwords copied to spawned children and threads:
// 0..8 ints 10000–07, 8..16 floats 10016–23, 16..20 ints 10036–39, 20..22 floats 10094–95,
// 22..26 call-argument ints 10053–56, 26..30 call-argument floats 10057–60.
#[derive(Clone, Debug)]
pub struct Context {
    pub ip: usize,
    pub clock: Timer,
    pub callback: i32,
    pub callback_arg: i32,
    pub vars: [u32; 30],
    pub wait: Timer,
    pub interp: [Interp; 8],
    pub sub: i32,
}

impl Default for Context {
    fn default() -> Self {
        Self { ip: 0, clock: Timer::default(), callback: -1, callback_arg: 0, vars: [0; 30], wait: Timer::default(), interp: [Interp::default(); 8], sub: -1 }
    }
}

pub const V_INT0: usize = 0;
pub const V_FLT0: usize = 8;
pub const V_INT1: usize = 16;
pub const V_FLT1: usize = 20;
pub const V_ARGI: usize = 22;
pub const V_ARGF: usize = 26;

impl Context {
    pub fn int(&self, k: usize) -> i32 {
        self.vars[k] as i32
    }

    pub fn flt(&self, k: usize) -> f32 {
        f32::from_bits(self.vars[k])
    }

    pub fn set_int(&mut self, k: usize, v: i32) {
        self.vars[k] = v as u32;
    }

    pub fn set_flt(&mut self, k: usize, v: f32) {
        self.vars[k] = v.to_bits();
    }

    pub fn enter(&mut self, ecl: &Ecl, sub: i32) -> bool {
        let Some(ip) = ecl.sub_start(sub) else { return false };
        self.ip = ip;
        self.clock.set(0);
        self.wait.set(0);
        self.sub = sub;
        true
    }
}

// The main context or a thread (op 135): a context plus its own 16-entry call stack.
#[derive(Clone, Debug, Default)]
pub struct Thread {
    pub ctx: Context,
    pub stack: Vec<Context>,
    pub depth: i32,
}

impl Thread {
    pub fn push(&mut self) {
        if self.stack.len() < 16 {
            self.stack.resize(16, Context::default());
        }
        let d = self.depth.clamp(0, 15) as usize;
        self.stack[d] = self.ctx.clone();
        self.depth = (self.depth + 1).min(15);
    }
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub vm: AnmVm,
    pub sub_vms: [AnmVm; 2],
    pub main: Thread,
    pub threads: [Option<Box<Thread>>; 4],
    pub ivars: [i32; 8],
    pub fvars: [f32; 8],
    pub death_sub: i32,
    pub interrupts: [i32; 32],
    pub pending_interrupt: i32,
    pub pos: [f32; 3],
    pub pos_offset: [f32; 3],
    pub vel: [f32; 3],
    pub prev_pos: [f32; 3],
    pub displacement: [f32; 3],
    pub hitbox: [f32; 3],
    pub shotbox: [f32; 3],
    pub final_pos: [f32; 3],
    pub angle: f32,
    pub angvel: f32,
    pub orbit_angle: f32,
    pub orbit_av: f32,
    pub speed: f32,
    pub accel: f32,
    pub radius: f32,
    pub radius_vel: f32,
    pub shot_offset: [f32; 3],
    pub interp_delta: [f32; 3],
    pub interp_start: [f32; 3],
    pub move_timer: Timer,
    pub move_dur: i32,
    pub rank_speed: [f32; 2],
    pub rank_counts: [i32; 4],
    pub life: i32,
    pub max_life: i32,
    pub hud_life: i32,
    pub score: i32,
    pub slot: i32,
    pub f2e1c: i32,
    pub boss_timer: Timer,
    pub main_color: [u8; 4],
    pub desc: BulletDesc,
    pub stored_fire: Option<Vec<u8>>,
    pub autofire_interval: i32,
    pub autofire_timer: Timer,
    pub laser_desc: LaserDesc,
    pub lasers: [Option<usize>; 32],
    pub laser_cur: usize,
    pub item: i32,
    pub extra_point: i32,
    pub extra_power: i32,
    pub death_bytes: [u8; 3],
    pub boss_slot: i32,
    pub flags: u32,
    pub flags_b: u32,
    pub dir: i8,
    pub layer: u8,
    pub mode_bits: u8,
    pub dir_scripts: [i32; 6],
    pub clamp: [f32; 4],
    pub nofire_r2: f32,
    pub last_damage: i32,
    pub thresholds: [i32; 4],
    pub threshold_subs: [i32; 4],
    pub timeout: i32,
    pub timeout_sub: i32,
    pub children: i32,
    pub chain_prev: Option<usize>,
    pub chain_next: Option<usize>,
    pub root: Option<usize>,
    pub dmg_timer: Timer,
    pub orb_acc: i32,
    pub bonus_seconds: i32,
    pub hit: bool,
    pub flash: bool,
    pub kill_running_thread: bool,
    pub running_thread: Option<usize>,
    pub trail: [i32; 4],
    pub effects: u32,
}

impl Default for Enemy {
    // EnemyMgr_Ctor template defaults (07 §4.2).
    fn default() -> Self {
        Self {
            vm: AnmVm::default(),
            sub_vms: [AnmVm::default(), AnmVm::default()],
            main: Thread::default(),
            threads: Default::default(),
            ivars: [0; 8],
            fvars: [0.0; 8],
            death_sub: -1,
            interrupts: [-1; 32],
            pending_interrupt: -1,
            pos: [0.0; 3],
            pos_offset: [0.0; 3],
            vel: [0.0; 3],
            prev_pos: [0.0; 3],
            displacement: [0.0; 3],
            hitbox: [24.0; 3],
            shotbox: [0.0; 3],
            final_pos: [0.0; 3],
            angle: 0.0,
            angvel: 0.0,
            orbit_angle: 0.0,
            orbit_av: 0.0,
            speed: 0.0,
            accel: 0.0,
            radius: 0.0,
            radius_vel: 0.0,
            shot_offset: [0.0; 3],
            interp_delta: [0.0; 3],
            interp_start: [0.0; 3],
            move_timer: Timer::default(),
            move_dur: 0,
            rank_speed: [-0.15, 0.15],
            rank_counts: [0; 4],
            life: 1,
            max_life: 1,
            hud_life: 1,
            score: 100,
            slot: 0,
            f2e1c: 0,
            boss_timer: Timer::default(),
            main_color: [255; 4],
            desc: BulletDesc::default(),
            stored_fire: None,
            autofire_interval: 0,
            autofire_timer: Timer::default(),
            laser_desc: LaserDesc::default(),
            lasers: [None; 32],
            laser_cur: 0,
            item: 0,
            extra_point: 0,
            extra_power: 0,
            death_bytes: [0; 3],
            boss_slot: -1,
            flags: A_ALIVE | A_CONTACT | A_LOSES_LIFE | A_SHOT_HIT,
            flags_b: 0,
            dir: -1,
            layer: 0,
            mode_bits: 0,
            dir_scripts: [-1; 6],
            clamp: [0.0; 4],
            nofire_r2: 1024.0,
            last_damage: 0,
            thresholds: [-1; 4],
            threshold_subs: [-1; 4],
            timeout: -1,
            timeout_sub: -1,
            children: 0,
            chain_prev: None,
            chain_next: None,
            root: None,
            dmg_timer: Timer::default(),
            orb_acc: 0,
            bonus_seconds: 0,
            hit: false,
            flash: false,
            kill_running_thread: false,
            running_thread: None,
            trail: [0; 4],
            effects: 0,
        }
    }
}

impl Enemy {
    pub fn alive(&self) -> bool {
        self.flags & A_ALIVE != 0
    }

    pub fn set_flag(&mut self, f: u32, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    pub fn set_flag_b(&mut self, f: u32, on: bool) {
        if on {
            self.flags_b |= f;
        } else {
            self.flags_b &= !f;
        }
    }

    pub fn move_mode(&self) -> u32 {
        (self.flags >> A_MOVE_SHIFT) & 3
    }

    pub fn set_move_mode(&mut self, m: u32) {
        self.flags = (self.flags & !(3 << A_MOVE_SHIFT)) | ((m & 3) << A_MOVE_SHIFT);
    }

    pub fn set_ease(&mut self, m: i32) {
        self.flags = (self.flags & !(7 << A_EASE_SHIFT)) | (((m as u32) & 7) << A_EASE_SHIFT);
    }

    pub fn anm_slot(&self) -> usize {
        if self.flags_b & B_STAGE_ANM != 0 { ANM_STAGE } else { ANM_ENEMY }
    }

    pub fn clamp_pos(&mut self) {
        if self.flags & A_CLAMP != 0 {
            let [x0, y0, x1, y1] = self.clamp;
            self.pos[0] = self.pos[0].max(x0).min(x1);
            self.pos[1] = self.pos[1].max(y0).min(y1);
        }
    }

    pub fn reset_rank_ranges(&mut self) {
        self.rank_speed = [-0.5, 0.5];
        self.rank_counts = [0; 4];
    }

    // Enemy_UpdateMoveMode (07 §8).
    pub fn update_move_mode(&mut self, gs: f32) {
        let mirror = self.flags & A_MIRROR != 0;
        match self.move_mode() {
            1 => {
                self.angle = anmvm::wrap_add(self.angle, gs * self.angvel);
                self.speed += gs * self.accel;
                self.vel = [self.angle.cos() * self.speed, self.angle.sin() * self.speed, 0.0];
                self.tick_duration(gs);
            }
            2 => {
                self.move_timer.dec(gs);
                let dur = self.move_dur.max(1) as f32;
                let t = (1.0 - self.move_timer.getf() / dur).max(0.0);
                let t = anmvm::ease(((self.flags >> A_EASE_SHIFT) & 7) as u8, t);
                let target: [f32; 3] = std::array::from_fn(|k| self.interp_start[k] + self.interp_delta[k] * t);
                self.vel = std::array::from_fn(|k| target[k] - self.pos[k]);
                if mirror {
                    self.vel[0] = -self.vel[0];
                }
                self.angle = self.vel[1].atan2(self.vel[0]);
                if self.move_timer.cur <= 0 {
                    self.set_move_mode(0);
                    self.pos = std::array::from_fn(|k| self.interp_start[k] + self.interp_delta[k]);
                    self.vel = [0.0; 3];
                }
            }
            3 => {
                self.orbit_angle = anmvm::wrap_add(self.orbit_angle, gs * self.orbit_av);
                self.radius += gs * self.radius_vel;
                let c = self.interp_start;
                self.vel[0] = c[0] + self.orbit_angle.cos() * self.radius - self.pos[0];
                self.vel[1] = c[1] + self.orbit_angle.sin() * self.radius - self.pos[1];
                self.angle = self.vel[1].atan2(self.vel[0]);
                self.tick_duration(gs);
            }
            _ => {}
        }
    }

    fn tick_duration(&mut self, gs: f32) {
        if self.move_dur > 0 {
            self.move_timer.dec(gs);
            if self.move_timer.cur <= 0 {
                self.set_move_mode(0);
            }
        }
    }

    pub fn reset_for_jump(&mut self) {
        self.main.depth = 0;
        self.threads = Default::default();
        self.desc = BulletDesc::default();
    }
}

#[derive(Clone, Debug, Default)]
pub struct Spell {
    pub id: i32,
    pub face: i32,
    pub name: Vec<u8>,
    pub owner: Vec<u8>,
    pub comment: [Vec<u8>; 2],
    pub bonus: i32,
    pub decay: i32,
    pub frames: i32,
    pub misses: u32,
}

// Globals touched by ECL (difficulty, rank, argument registers, spell state, …).
#[derive(Clone, Debug, Default)]
pub struct Globals {
    pub difficulty: i32,
    pub rank: i32,
    pub args: [u32; 8],
    pub player: [f32; 3],
    pub player_char: i32,
    pub player_mode: i32,
    pub spell: Option<Spell>,
    pub spawn_disabled: i32,
    pub timer_pause: i32,
    pub hud_boss: bool,
    pub dialogue: bool,
    pub drop_counter: u32,
    pub drop_index: usize,
    pub native_globals: [i32; 2],
    pub flag_164d0bb: bool,
    pub g_f54cec: i32,
    pub g_18b8a2c: i32,
    pub g_4e4b60: i32,
    pub hud_markers: [[i32; 3]; 8],
    pub fixed_bonus: bool,
    pub hud_bars: i32,
    pub hud_requests: Vec<u16>,
    pub misc_calls: u64,
}

pub struct EnemyManager {
    pub enemies: Vec<Box<Enemy>>,
    spare: Vec<Box<Enemy>>,
    template: Box<Enemy>,
    pub ecl: Ecl,
    pub bosses: [Option<usize>; 4],
    timelines: Vec<(Timer, usize)>,
    pub sync: [i32; 4],
    pub g: Globals,
    pub bullets: Rc<RefCell<BulletManager>>,
    pub player: PlayerRef,
    pub stage: Option<Rc<RefCell<Stage>>>,
    pub full: bool,
    pub frame: u32,
    pub debug_damage: i32,
    pub fast_forward: u32,
    pub msg: Option<Rc<RefCell<crate::msg::Dialogue>>>,
}

pub fn parse_difficulty(s: &str) -> Option<i32> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "easy" | "e" | "0" => 0,
        "normal" | "n" | "1" => 1,
        "hard" | "h" | "2" => 2,
        "lunatic" | "l" | "3" => 3,
        "extra" | "x" | "4" => 4,
        _ => return None,
    })
}

impl EnemyManager {
    pub fn new(ecl: Ecl, bullets: Rc<RefCell<BulletManager>>, difficulty: i32) -> Self {
        let timelines = ecl.timelines.iter().map(|&o| (Timer::default(), o)).collect();
        // SPEC-GAP: the initial rank value (0–32) is not specified; 16 assumed.
        let player = bullets.borrow().player.clone();
        player.borrow_mut().stats.difficulty = difficulty;
        let g = Globals { difficulty, rank: 16, player: player.borrow().pos, ..Default::default() };
        Self {
            enemies: (0..MAX_ENEMIES).map(|_| Box::new(Enemy { flags: 0, ..Enemy::default() })).collect(),
            spare: Vec::new(),
            template: Box::default(),
            ecl,
            bosses: [None; 4],
            timelines,
            sync: [-1; 4],
            g,
            bullets,
            player,
            stage: None,
            full: false,
            frame: 0,
            debug_damage: 0,
            fast_forward: 0,
            msg: None,
        }
    }

    pub fn register(ctx: &mut Ctx, index: usize, stage: Option<Rc<RefCell<Stage>>>, bullets: Rc<RefCell<BulletManager>>, diff: i32) -> Result<Rc<RefCell<EnemyManager>>> {
        let (ecl_name, enm) = files(index);
        let ecl = ecl::parse(ctx.archive.read(&ecl_name)?)?;
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_ENEMY, "enemy.anm")?;
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_STAGE, &enm)?;
        let mut m = EnemyManager::new(ecl, bullets, diff);
        m.stage = stage;
        m.fast_forward = crate::env::var("TH08_STAGE_FRAME").and_then(|v| v.parse().ok()).unwrap_or(0);
        m.debug_damage = crate::env::var("TH08_ENEMY_DMG").and_then(|v| v.parse().ok()).unwrap_or(0);
        m.stage_start(&mut ctx.anm, index);
        let m = Rc::new(RefCell::new(m));
        let t: task::TaskRef<Ctx> = m.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        Ok(m)
    }

    // Stage start values owned by the game manager (11 §3) and EnemyManager_Load's two draws (11 §1.5).
    pub fn stage_start(&mut self, anm: &mut AnmManager, index: usize) {
        let d = self.g.difficulty.clamp(0, 4) as usize;
        let mut p = self.player.borrow_mut();
        p.stats.max_point = crate::player::MAX_POINT[d];
        p.stats.time_threshold = crate::player::TIME_THR.get(index % 9).map_or(10, |r| r[d.min(3)]);
        p.stats.time = 0;
        p.stats.time_added = 0;
        drop(p);
        anm.rng.range16(3);
        anm.rng.range16(8);
    }

    // Player collision (07 §10.1 step 6): contact, bomb-area touch, shot/bomb damage, homing target.
    // SPEC-GAP: pairing of the 0.7 / 1.5 box scales with the two player tests is assumed: 0.7 kills the player,
    // 1.5 against the bomb cancel areas costs a non-boss without flag bit 7 10 life; the shot box replaces the
    // hitbox when its width > 0.
    fn player_collision(&mut self, idx: usize, anm: &mut AnmManager) {
        let e = &self.enemies[idx];
        let (flags, pos, hb, sb) = (e.flags, e.final_pos, e.hitbox, e.shotbox);
        let boss = flags & A_BOSS != 0;
        if flags & A_CONTACT != 0 {
            let mut p = self.player.borrow_mut();
            p.contact_test(pos, [hb[0], hb[1]], &mut anm.rng);
            let touched = p.box_in_cancel_area(pos, [hb[0] * 0.75, hb[1] * 0.75]);
            drop(p);
            if touched && !boss && flags & A_CANCEL_BONUS == 0 {
                self.enemies[idx].life -= 10;
            }
        }
        if flags & A_SHOT_HIT != 0 {
            let bx = if sb[0] > 0.0 { sb } else { hb };
            let mut acc = self.enemies[idx].orb_acc;
            let mut by_bomb = false;
            let d = {
                let mut b = self.bullets.borrow_mut();
                let mut p = self.player.borrow_mut();
                if flags & A_ON_SCREEN != 0 {
                    p.offer_target(pos);
                }
                p.damage_against(anm, Some(&mut b.items), pos, [bx[0], bx[1]], &mut acc, &mut by_bomb)
            };
            self.enemies[idx].orb_acc = acc;
            self.damage(idx, d, by_bomb);
        }
    }

    // The running enemy is swapped out for a placeholder whose alive bit keeps its slot reserved.
    pub(crate) fn take(&mut self, idx: usize) -> Box<Enemy> {
        let mut p = self.spare.pop().unwrap_or_default();
        p.flags = A_ALIVE;
        std::mem::replace(&mut self.enemies[idx], p)
    }

    pub(crate) fn put(&mut self, idx: usize, e: Box<Enemy>) {
        let p = std::mem::replace(&mut self.enemies[idx], e);
        self.spare.push(p);
    }

    pub fn free(&mut self, idx: usize) {
        self.enemies[idx].flags &= !A_ALIVE;
        self.unregister_boss(idx);
    }

    // SPEC-GAP: the HUD boss display is assumed to switch off whenever boss slot 0 is vacated.
    pub(crate) fn unregister_boss(&mut self, idx: usize) {
        for (s, b) in self.bosses.iter_mut().enumerate() {
            if *b == Some(idx) {
                *b = None;
                if s == 0 {
                    self.g.hud_boss = false;
                }
            }
        }
    }

    pub fn boss(&self, slot: i32) -> Option<usize> {
        self.bosses.get(usize::try_from(slot).ok()?).copied().flatten()
    }

    pub fn alive_count(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive()).count()
    }

    pub fn rank_mask(&self, mode_bits: u8) -> u8 {
        let d = self.g.difficulty;
        (if d < 4 { 1u8 << d } else { 0x0F }) | mode_bits
    }

    // EnemyMgr_Spawn* steps 1–3; the caller finishes with `spawn_finish`.
    pub(crate) fn spawn_prepare(&mut self, sub: i32, pos: [f32; 3], life: i32, mirror: bool) -> Option<usize> {
        let Some(idx) = self.enemies.iter().position(|e| !e.alive()) else {
            self.full = true;
            return None;
        };
        let mut e = self.template.clone();
        e.slot = idx as i32;
        e.set_flag(A_MIRROR, mirror);
        if life >= 0 {
            e.life = life;
        }
        e.pos = pos;
        e.prev_pos = pos;
        e.final_pos = pos;
        e.main.ctx.enter(&self.ecl, sub);
        self.enemies[idx] = e;
        Some(idx)
    }

    pub(crate) fn spawn_finish(&mut self, anm: &mut AnmManager, idx: usize, life: i32, item: i32, score: i32, from_ecl: bool) -> Option<usize> {
        if crate::eclvm::ecl_run(self, anm, idx) == -1 {
            self.free(idx);
            return None;
        }
        let e = &mut self.enemies[idx];
        e.main_color = e.vm.color;
        e.item = item as i8 as i32;
        if score >= 0 {
            e.score = score;
        }
        if from_ecl && life >= 0 {
            e.life = life;
        }
        e.max_life = e.life;
        e.hud_life = e.life;
        Some(idx)
    }

    pub fn spawn(&mut self, anm: &mut AnmManager, sub: i32, pos: [f32; 3], life: i32, item: i32, score: i32, mirror: bool) -> Option<usize> {
        let idx = self.spawn_prepare(sub, pos, life, mirror)?;
        self.spawn_finish(anm, idx, life, item, score, false)
    }

    fn spawn_blocked(&self) -> bool {
        // SPEC-GAP: the HUD query 0x42F320 is taken as "boss bar shown"; dialogue alone does not block (bosses spawn mid-dialogue).
        self.g.hud_boss || self.g.spawn_disabled != 0
    }

    // EclTimeline_Run (07 §3).
    fn run_timelines(&mut self, anm: &mut AnmManager) {
        let gs = anm.speed;
        let dmask = self.rank_mask(0);
        for i in 0..self.timelines.len() {
            loop {
                let (clock, ptr) = self.timelines[i];
                let Some(ins) = self.ecl.tl_ins(ptr) else { break };
                if ins.time < 0 {
                    self.timelines[i].0.tick(gs);
                    break;
                }
                if clock.cur == ins.time {
                    if ins.rank & dmask != 0 && !self.timeline_op(anm, &ins) {
                        // Stall: the clock does not advance.
                        self.timelines[i].0.dec(gs);
                        self.timelines[i].0.tick(gs);
                        break;
                    }
                } else if clock.cur < ins.time {
                    self.timelines[i].0.tick(gs);
                    break;
                }
                self.timelines[i].1 += ins.size;
            }
        }
    }

    // Returns false to stall.
    fn timeline_op(&mut self, anm: &mut AnmManager, ins: &ecl::TlIns) -> bool {
        let args: [u32; 7] = std::array::from_fn(|k| self.ecl.tl_arg(ins, k));
        let (ai, af) = (|k: usize| args[k] as i32, |k: usize| f32::from_bits(args[k]));
        match ins.op {
            0 | 1 | 15 => {
                if ins.op == 15 || !self.spawn_blocked() {
                    self.spawn(anm, ai(0), [af(1), af(2), 0.0], ai(3), ai(4), ai(5), ins.op == 1);
                }
            }
            2 | 4 => {
                if !self.spawn_blocked() {
                    let x = anm.rng.next_f32() * (af(2) - af(1)) + af(1);
                    self.spawn(anm, ai(0), [x, af(3), 0.0], ai(4), ai(5), ai(6), ins.op == 4);
                }
            }
            3 | 5 => {
                if !self.spawn_blocked() {
                    let x = anm.rng.next_f32() * 384.0;
                    self.spawn(anm, ai(0), [x, af(1), 0.0], ai(2), ai(3), ai(4), ins.op == 5);
                }
            }
            6 => self.start_msg(anm, ai(0)),
            7 => return !self.msg.as_ref().is_some_and(|m| m.borrow().blocking()),
            8 => {
                if let Some(b) = self.boss(ai(0)) {
                    self.enemies[b].pending_interrupt = ai(1);
                }
            }
            9 => self.g.misc_calls += 1,
            10 => return self.boss(ai(0)).is_none_or(|b| self.enemies[b].life <= 0 || !self.enemies[b].alive()),
            11 | 12 => {
                if !self.spawn_blocked()
                    && let Some(idx) = self.spawn(anm, ai(0), [af(1), af(2), 0.0], ai(3), -1, ai(6), ins.op == 12)
                {
                    self.enemies[idx].extra_point = ai(4);
                    self.enemies[idx].extra_power = ai(5);
                }
            }
            13 => {
                let v = ai(0);
                let mut hit = false;
                for s in &mut self.sync {
                    if *s == v {
                        *s = -1;
                        hit = true;
                    }
                }
                return hit;
            }
            14 => {
                let v = ai(0);
                self.sync.iter_mut().filter(|s| **s < 0).for_each(|s| *s = v);
            }
            16 => self.player.borrow_mut().game_over = true,
            _ => {}
        }
        true
    }

    // Enemy_Move: clamp, step by velocity (x mirrored), clamp, follow parent, final position.
    fn move_enemy(&mut self, idx: usize, gs: f32) {
        let root_pos = self.enemies[idx].root.map(|r| self.enemies[r].pos);
        let e = &mut self.enemies[idx];
        if e.flags & A_NO_MOVE == 0 {
            e.clamp_pos();
            e.displacement = std::array::from_fn(|k| e.pos[k] - e.prev_pos[k]);
            e.prev_pos = e.pos;
            if e.flags & A_MIRROR != 0 {
                e.pos[0] -= gs * e.vel[0];
            } else {
                e.pos[0] += gs * e.vel[0];
            }
            e.pos[1] += gs * e.vel[1];
            e.pos[2] += gs * e.vel[2];
            e.clamp_pos();
        }
        if e.flags & A_FOLLOW != 0
            && let Some(p) = root_pos
        {
            e.pos_offset = p;
        }
        e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
    }

    // Returns true when the enemy left the screen and must be freed.
    fn screen_check(&mut self, idx: usize) -> bool {
        let e = &mut self.enemies[idx];
        if e.flags & A_HIDDEN != 0 {
            return false;
        }
        let (hw, hh) = (e.hitbox[0] / 2.0, e.hitbox[1] / 2.0);
        let [x, y, _] = e.final_pos;
        let inside = x + hw >= 0.0 && x - hw <= 384.0 && y + hh >= 0.0 && y - hh <= 448.0;
        if e.flags & A_ON_SCREEN == 0 {
            if inside {
                e.flags |= A_ON_SCREEN;
            }
            false
        } else {
            !inside && e.flags & A_KEEP_OFFSCREEN == 0
        }
    }

    pub(crate) fn jump_main(&mut self, idx: usize, sub: i32) {
        let ecl = &self.ecl;
        let e = &mut self.enemies[idx];
        e.main.depth = 0;
        e.main.ctx.enter(ecl, sub);
    }

    pub(crate) fn jump_to_death_sub(&mut self, idx: usize) {
        let sub = self.enemies[idx].death_sub;
        if sub >= 0 {
            self.enemies[idx].reset_for_jump();
            self.jump_main(idx, sub);
            self.enemies[idx].death_sub = -1;
        }
    }

    // Op 95 / threshold cleanup: every other alive non-boss enemy gets life 0 and jumps to its death sub.
    // SPEC-GAP: the jump is assumed to consume the death sub (otherwise death processing would restart it).
    pub(crate) fn kill_all(&mut self, anm: &mut AnmManager, except: Option<usize>) -> i32 {
        let mut n = 0;
        for j in 0..self.enemies.len() {
            let e = &self.enemies[j];
            if Some(j) == except || !e.alive() || e.flags & A_BOSS != 0 || e.flags_b & B_CANNOT_DIE != 0 {
                continue;
            }
            if e.flags & A_CANCEL_BONUS != 0 {
                n += 1;
                let p = e.final_pos;
                // SPEC-GAP: item mode of the cancel-bonus star is not given; 1 (attracted) like bullet cancels.
                self.spawn_item(anm, p, 6, 1);
                self.add_score((2000 + 30 * n).min(8000));
            }
            self.enemies[j].life = 0;
            self.jump_to_death_sub(j);
        }
        n
    }

    // 0x42ADB0 chain kill; SPEC-GAP: score popups / item showers proportional to chain length are not modelled.
    pub(crate) fn kill_chain(&mut self, idx: usize, drops: bool) {
        let mut next = self.enemies[idx].chain_next;
        let mut guard = 0;
        while let Some(c) = next {
            guard += 1;
            if guard > MAX_ENEMIES {
                break;
            }
            let e = &mut self.enemies[c];
            next = e.chain_next;
            if !e.alive() {
                continue;
            }
            e.flags |= A_CHAIN_KILLED;
            e.life = 0;
            if !drops {
                e.item = -2;
                e.extra_point = 0;
                e.extra_power = 0;
            }
        }
        self.enemies[idx].children = 0;
    }

    // Hud_StartMsg: Msg_Start plus its playfield side effects (10 §4.6).
    pub fn start_msg(&mut self, anm: &mut AnmManager, n: i32) {
        let Some(m) = self.msg.clone() else { return };
        let mut m = m.borrow_mut();
        m.info.clock = self.player.borrow().stats.clock;
        m.start(n);
        if std::mem::take(&mut m.side_effects) {
            drop(m);
            self.msg_side_effects(anm);
        }
    }

    // SPEC-GAP: "destroy" (0x42EFB0) also frees killed enemies that have no death sub and already passed death
    // processing (latch set); otherwise a stage-4 midboss continuing in its consumed death sub stays forever.
    pub fn msg_side_effects(&mut self, anm: &mut AnmManager) {
        self.cancel_all(anm, 1);
        let doomed: Vec<usize> = (0..self.enemies.len())
            .filter(|&j| {
                let e = &self.enemies[j];
                e.alive() && e.flags & A_BOSS == 0 && e.flags_b & B_CANNOT_DIE == 0 && e.death_sub < 0 && e.flags_b & B_DEATH_LATCH != 0
            })
            .collect();
        self.kill_all(anm, None);
        for j in doomed {
            self.free(j);
        }
        self.bullets.borrow_mut().items.attract_all();
    }

    // Spell end (op 123, 0x4161B0). SPEC-GAP: "captured" = no death and no bomb since the card started.
    pub fn finish_spell(&mut self, total: i32, remaining: i32, anm: &mut AnmManager) {
        let Some(s) = self.g.spell.take() else { return };
        let mut p = self.player.borrow_mut();
        if p.deaths + p.bombs_started != s.misses {
            p.stats.popups.push(crate::player::Popup::Message(5, 0));
            return;
        }
        let thr = total - total / 7;
        let time = if self.g.fixed_bonus {
            700
        } else if remaining >= thr {
            1000
        } else if remaining < 180 {
            100
        } else {
            (remaining - 180) * 900 / (thr - 180).max(1) + 100
        };
        let st = &mut p.stats;
        st.add_score(s.bonus);
        st.spells += 1;
        crate::player::Stats::checksum(&mut anm.rng);
        st.add_time(time, &mut anm.rng);
        st.popups.push(crate::player::Popup::SpellBonus(s.bonus));
    }

    pub fn dialogue_active(&self) -> bool {
        self.msg.as_ref().is_some_and(|m| m.borrow().active())
    }

    pub fn spawn_item(&mut self, anm: &mut AnmManager, pos: [f32; 3], kind: i32, mode: u8) {
        let p = self.player.borrow();
        self.bullets.borrow_mut().items.spawn(anm, &p, pos, kind, mode);
    }

    pub fn add_score(&mut self, v: i32) {
        self.player.borrow_mut().stats.add_score(v);
    }

    pub fn cancel_all(&mut self, anm: &mut AnmManager, mode: i32) {
        let mut p = self.player.borrow_mut();
        self.bullets.borrow_mut().cancel_all(anm, &mut p, mode);
    }

    // SPEC-GAP: drops spawn in mode 0; when a drop counts as "auto-collected" (mode 1) is not given.
    fn drop_items(&mut self, idx: usize, anm: &mut AnmManager) {
        let e = &mut self.enemies[idx];
        let p = e.final_pos;
        let mut items = Vec::new();
        if e.item >= 0 {
            items.push((p, e.item));
        } else if e.item == -1 {
            // SPEC-GAP: counter/index update order of the every-third-death table is not specified.
            if self.g.drop_counter.is_multiple_of(3) {
                items.push((p, DROP_TABLE[self.g.drop_index % 32]));
                self.g.drop_index += 1;
            }
            self.g.drop_counter += 1;
        }
        let power_kind = if self.player.borrow().stats.power < 128.0 { 0 } else { 1 };
        for (n, kind) in [(e.extra_power, power_kind), (e.extra_point, 1)] {
            for _ in 0..n.max(0) {
                let dx = anm.rng.next_f32() * 128.0 - 64.0;
                let dy = anm.rng.next_f32() * 128.0 - 64.0;
                items.push(([p[0] + dx, p[1] + dy, p[2]], kind));
            }
        }
        e.extra_point = 0;
        e.extra_power = 0;
        for (pos, kind) in items {
            self.spawn_item(anm, pos, kind, 0);
        }
    }

    fn cleanup_after_jump(&mut self, anm: &mut AnmManager, idx: usize, drops: bool) {
        let e = &mut self.enemies[idx];
        e.threads = Default::default();
        e.desc = BulletDesc::default();
        e.autofire_interval = 0;
        e.reset_rank_ranges();
        self.kill_chain(idx, drops);
        self.kill_all(anm, Some(idx));
    }

    // Enemy_CheckLifeThresholds (07 §10.2).
    fn check_thresholds(&mut self, anm: &mut AnmManager, idx: usize) -> bool {
        let e = &self.enemies[idx];
        let Some(k) = (0..4).find(|&k| e.thresholds[k] >= 0 && e.life < e.thresholds[k]) else { return false };
        let sub = e.threshold_subs[k];
        let e = &mut self.enemies[idx];
        e.life = e.thresholds[k];
        e.thresholds[k] = -1;
        e.bonus_seconds = (e.timeout - e.boss_timer.cur) / 60;
        e.timeout = -1;
        self.jump_main(idx, sub);
        self.cleanup_after_jump(anm, idx, true);
        true
    }

    // Enemy_CheckTimeout (07 §10.2).
    fn check_timeout(&mut self, anm: &mut AnmManager, idx: usize) -> bool {
        let e = &mut self.enemies[idx];
        if e.timeout < 0 || e.boss_timer.cur < e.timeout {
            return false;
        }
        if let Some(k) = (0..4).filter(|&k| e.thresholds[k] >= 0).max_by_key(|&k| e.thresholds[k]) {
            e.life = e.thresholds[k];
            e.thresholds[k] = -1;
        }
        let sub = e.timeout_sub;
        e.timeout_sub = e.death_sub;
        e.boss_timer.set(0);
        let fixed = e.flags & A_FIXED_BONUS != 0;
        self.jump_main(idx, sub);
        if !fixed {
            self.g.spell = None;
            self.cancel_all(anm, 1);
        }
        self.cleanup_after_jump(anm, idx, false);
        true
    }

    // Shot damage (07 §10.1 step 6); `d` comes from the player side.
    pub fn damage(&mut self, idx: usize, d: i32, bomb: bool) {
        if d <= 0 {
            return;
        }
        let d = d.min(70);
        self.add_score((d / 5) * 10);
        let spell = self.g.spell.is_some();
        let e = &mut self.enemies[idx];
        if e.flags & A_LOSES_LIFE == 0 {
            return;
        }
        // SPEC-GAP: the "reduced" bomb damage during spell cards is not given; d / 7 as for shots.
        let child = e.root.is_some();
        let mut d = match (spell, bomb) {
            (true, true) if child => 0,
            (true, true) => if d < 3 { d.min(1) } else { d / 7 },
            (true, false) => if d < 8 { d.min(1) } else { d / 7 },
            _ => d,
        };
        if e.dmg_timer.cur > 0 {
            d = if e.flags & A_BOSS != 0 { d / 9 } else { 0 };
        }
        e.life -= d;
        e.last_damage = d;
        e.hit = true;
        if let Some(r) = e.root {
            let root = &mut self.enemies[r];
            let mut rd = d / 2;
            if root.dmg_timer.cur > 0 {
                rd = if root.flags & A_BOSS != 0 { d / 9 } else { 0 };
            }
            let floor = root.thresholds.iter().copied().filter(|&t| t >= 0).max().unwrap_or(i32::MIN);
            root.life = (root.life - rd).max(floor);
        }
    }

    // Death processing (07 §10.3).
    fn death(&mut self, idx: usize, anm: &mut AnmManager) {
        let e = &self.enemies[idx];
        if e.life >= 1 || e.flags_b & (B_DEATH_LATCH | B_CANNOT_DIE) != 0 || !e.alive() {
            return;
        }
        self.enemies[idx].flags_b |= B_DEATH_LATCH;
        self.kill_chain(idx, true);
        let e = &mut self.enemies[idx];
        let mode = (e.flags >> A_DEATH_SHIFT) & 7;
        match mode {
            0 => {
                let sc = e.score;
                e.flags &= !A_ALIVE;
                self.add_score(sc);
            }
            1 => {
                let sc = e.score;
                e.flags |= A_DEAD_CONT;
                e.flags &= !(A_CONTACT | A_LOSES_LIFE | A_SHOT_HIT);
                self.add_score(sc);
            }
            3 => {
                e.life = 1;
                e.flags_b &= !B_DEATH_LATCH;
                e.flags &= !(7 << A_DEATH_SHIFT);
            }
            _ => {}
        }
        if mode <= 2 {
            if self.enemies[idx].flags & A_BOSS != 0 {
                self.unregister_boss(idx);
                self.enemies[idx].flags &= !A_BOSS;
                if self.g.spell.is_none() {
                    // SPEC-GAP: CancelBonus arguments for a boss death are not given; cap 8000, lasers included.
                    let sum = {
                        let mut p = self.player.borrow_mut();
                        self.bullets.borrow_mut().cancel_bonus(anm, &mut p, 8000, true)
                    };
                    self.add_score(sum);
                    self.kill_all(anm, Some(idx));
                }
            }
            self.drop_items(idx, anm);
        }
        if mode == 0 {
            self.free(idx);
            return;
        }
        self.jump_to_death_sub(idx);
    }

    fn run_vms(&mut self, idx: usize, anm: &mut AnmManager) {
        let e = &mut self.enemies[idx];
        if e.vm.script.is_some() {
            e.vm.run(anm);
        }
        for vm in &mut e.sub_vms {
            if vm.script.is_some() && vm.run(anm) {
                *vm = AnmVm::default();
            }
        }
    }

    // EnemyMgr_Calc per enemy (07 §10.1).
    fn update(&mut self, idx: usize, anm: &mut AnmManager) {
        let gs = anm.speed;
        if self.enemies[idx].flags & A_CHAIN_KILLED != 0 {
            let root_pos = self.enemies[idx].root.map(|r| self.enemies[r].pos);
            let e = &mut self.enemies[idx];
            if let (true, Some(p)) = (e.flags & A_FOLLOW != 0, root_pos) {
                e.pos_offset = p;
            }
            e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
            self.death(idx, anm);
            return;
        }
        if self.enemies[idx].flags & A_MODE_REACT != 0 {
            let youkai = self.g.player_mode != 0;
            let e = &mut self.enemies[idx];
            e.mode_bits = if youkai { 0x40 } else { 0x20 };
            e.set_flag(A_YOUKAI, youkai);
        }
        loop {
            if crate::eclvm::ecl_run(self, anm, idx) == -1 {
                self.free(idx);
                return;
            }
            self.move_enemy(idx, gs);
            if self.screen_check(idx) {
                self.free(idx);
                return;
            }
            if !(self.check_thresholds(anm, idx) || self.check_timeout(anm, idx)) {
                break;
            }
        }
        self.run_vms(idx, anm);
        if self.enemies[idx].flags & (A_HIDDEN | 1 << 5 | A_YOUKAI) == 0 {
            self.player_collision(idx, anm);
        }
        if self.debug_damage > 0 && self.enemies[idx].flags & (A_SHOT_HIT | A_HIDDEN) == A_SHOT_HIT && self.enemies[idx].flags & A_ON_SCREEN != 0 {
            self.damage(idx, self.debug_damage, false);
        }
        let pause = self.g.timer_pause != 0;
        let e = &mut self.enemies[idx];
        if std::mem::take(&mut e.hit) {
            e.vm.flags |= anmvm::F_COLOR2;
            e.vm.color2 = [0x80, 0x60, 0xFF, e.vm.color[anmvm::A]];
            e.flash = true;
        } else if std::mem::take(&mut e.flash) {
            e.vm.flags &= !anmvm::F_COLOR2;
        }
        if !pause {
            e.boss_timer.tick(gs);
        }
        if e.dmg_timer.cur > 0 {
            e.dmg_timer.dec(gs);
        }
        self.death(idx, anm);
    }

    pub fn step(&mut self, anm: &mut AnmManager) {
        {
            let p = self.player.borrow();
            self.g.player = p.pos;
            self.g.player_mode = p.mode as i32;
        }
        let bonus = std::mem::take(&mut self.player.borrow_mut().stats.spell_bonus_add);
        if let Some(s) = &mut self.g.spell {
            s.bonus += bonus;
        }
        self.full = false;
        self.run_timelines(anm);
        for idx in 0..self.enemies.len() {
            if self.enemies[idx].alive() {
                self.update(idx, anm);
            }
        }
        if let Some(s) = &mut self.g.spell {
            s.frames += 1;
            if s.frames % 60 == 0 && !self.g.fixed_bonus {
                s.bonus = (s.bonus - s.decay).max(0);
            }
        }
        let mut b = self.bullets.borrow_mut();
        b.env.boss = self.bosses.iter().any(Option::is_some);
        b.env.spell = self.g.spell.is_some();
        b.env.dialogue = self.dialogue_active();
        b.freeze = self.g.timer_pause != 0;
        let env = b.env;
        drop(b);
        self.player.borrow_mut().env = env;
        self.frame += 1;
    }

    fn draw_layer(&mut self, ctx_anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, high: bool) {
        gfx.set_viewport(PLAYFIELD);
        for e in &mut self.enemies {
            if !e.alive() || e.flags & A_HIDDEN != 0 || (e.layer >= 2) != high {
                continue;
            }
            let p = [e.final_pos[0] + 32.0, e.final_pos[1] + 16.0, 0.0];
            // SPEC-GAP: the draw mode enemies use is not specified; mode 2 (rotation-aware) unless the script set one.
            for vm in std::iter::once(&mut e.vm).chain(e.sub_vms.iter_mut()) {
                if vm.script.is_none() {
                    continue;
                }
                vm.pos = p;
                let mode = if vm.draw_mode == 0 { 2 } else { vm.draw_mode };
                ctx_anm.draw_mode(gfx, vm, mode);
            }
        }
    }
}

impl Task<Ctx> for EnemyManager {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.gfx, ANM_ENEMY);
            ctx.anm.unload(&mut ctx.gfx, ANM_STAGE);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for _ in 0..std::mem::take(&mut self.fast_forward) {
            self.step(&mut ctx.anm);
            let b = self.bullets.clone();
            b.borrow_mut().step(&mut ctx.anm);
        }
        self.step(&mut ctx.anm);
        Ret::Next
    }

    // SPEC-GAP: which draw layers (e+0x332F) go to draw prio 8 vs 11 is not specified; layers 0–1 → 8, ≥ 2 → 11.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        self.draw_layer(&mut ctx.anm, &mut ctx.gfx, tag == 1);
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Scripted input per shot type through a stage: shooting, moving, focusing, bombing.
    #[test]
    fn scripted_player_runs() {
        let Some(ar) = crate::archive::test_archive() else { return };
        let frames = std::env::var("TH08_TEST_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(4000);
        for shot in 0..12u8 {
            let index = [0, 1, 2, 4][shot as usize % 4];
            let (ecl_name, enm) = files(index);
            let ecl = ecl::parse(ar.read(&ecl_name).unwrap()).unwrap();
            let mut anm = AnmManager::default();
            anm.insert(ANM_ENEMY, "enemy.anm", crate::anm::parse(&ar.read("enemy.anm").unwrap()).unwrap(), |_| 0);
            anm.insert(ANM_STAGE, &enm, crate::anm::parse(&ar.read(&enm).unwrap()).unwrap(), |_| 0);
            anm.insert(crate::bullet::ANM_SLOT, "etama.anm", crate::anm::parse(&ar.read("etama.anm").unwrap()).unwrap(), |_| 0);
            let (pa, a, sa) = crate::sht::files(shot);
            anm.insert(crate::player::ANM_SLOT, &pa, crate::anm::parse(&ar.read(&pa).unwrap()).unwrap(), |_| 0);
            let player = crate::player::Player::new_ref();
            {
                let human = crate::sht::parse(&ar.read(&a).unwrap()).unwrap();
                let youkai = crate::sht::parse(&ar.read(&sa).unwrap()).unwrap();
                player.borrow_mut().load(&mut anm, human, youkai, shot);
                player.borrow_mut().stats.power = 128.0;
            }
            let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
            player.borrow_mut().world = Rc::downgrade(&bullets);
            let mut m = EnemyManager::new(ecl, bullets.clone(), 1);
            m.stage_start(&mut anm, index);
            let (mut shots, mut dmg_frames) = (0, 0);
            let mut inp = crate::input::Input::default();
            for f in 0..frames {
                use crate::input::*;
                let mut bits = SHOT;
                bits |= if (f / 90) % 2 == 0 { LEFT } else { RIGHT };
                if (f / 240) % 2 == 1 {
                    bits |= FOCUS;
                }
                if f % 50 < 10 {
                    bits |= UP;
                }
                if f % 700 == 350 {
                    bits |= BOMB;
                }
                inp.raw = bits;
                inp.tick();
                {
                    let mut b = bullets.borrow_mut();
                    let mut p = player.borrow_mut();
                    p.frame(&mut anm, &inp, Some(&mut b.items));
                    if std::mem::take(&mut p.bomb.attract) {
                        b.items.attract_all();
                    }
                }
                let before: i32 = m.enemies.iter().filter(|e| e.alive()).map(|e| e.life).sum();
                m.step(&mut anm);
                let after: i32 = m.enemies.iter().filter(|e| e.alive()).map(|e| e.life).sum();
                dmg_frames += (after < before) as i32;
                bullets.borrow_mut().step(&mut anm);
                let p = player.borrow();
                shots += p.bullets.iter().filter(|b| b.state != 0).count();
                assert!(p.pos.iter().all(|v| v.is_finite()), "shot {shot}: non-finite player");
                for b in p.bullets.iter().filter(|b| b.state != 0) {
                    assert!(b.pos.iter().all(|v| v.is_finite()), "shot {shot}: non-finite bullet");
                }
            }
            let p = player.borrow();
            eprintln!(
                "shot {shot} {ecl_name}: bullet-frames {shots} damage-frames {dmg_frames} bombs {} hits {} deaths {} lives {} score {}",
                p.bombs_started,
                p.hits,
                p.deaths,
                p.stats.lives,
                p.stats.score()
            );
            assert!(shots > 0 && dmg_frames > 0, "shot {shot}: no shooting/damage");
            assert!(p.bombs_started > 0, "shot {shot}: never bombed");
        }
    }

    #[test]
    fn all_stage_scripts_run() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else { return };
        let ar = crate::archive::Archive::from_bytes(std::fs::read(format!("{dir}/th08.dat")).unwrap()).unwrap();
        for index in 0..18 {
            let mut fired = 0;
            for diff in [0, 3] {
                let (ecl_name, enm) = files(index);
                let ecl = ecl::parse(ar.read(&ecl_name).unwrap()).unwrap();
                let mut anm = AnmManager::default();
                anm.insert(ANM_ENEMY, "enemy.anm", crate::anm::parse(&ar.read("enemy.anm").unwrap()).unwrap(), |_| 0);
                anm.insert(ANM_STAGE, &enm, crate::anm::parse(&ar.read(&enm).unwrap()).unwrap(), |_| 0);
                anm.insert(crate::bullet::ANM_SLOT, "etama.anm", crate::anm::parse(&ar.read("etama.anm").unwrap()).unwrap(), |_| 0);
                let player = crate::player::Player::new_ref();
                let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
                let diff = if index % 9 == 8 { 4 } else { diff };
                let mut m = EnemyManager::new(ecl, bullets.clone(), diff);
                m.debug_damage = 8;
                let (mut spawned, mut max_alive, mut max_live, mut lasers, mut items) = (0, 0, 0, 0, 0);
                let inp = crate::input::Input::default();
                for _ in 0..6000 {
                    player.borrow_mut().step(&mut anm, &inp);
                    m.step(&mut anm);
                    bullets.borrow_mut().step(&mut anm);
                    let b = bullets.borrow();
                    max_live = max_live.max(b.live);
                    lasers += b.lasers.iter().filter(|l| l.alive).count();
                    items = items.max(b.items.count);
                    for x in b.live_bullets() {
                        assert!(x.pos.iter().all(|v| v.is_finite()) && x.vel.iter().all(|v| v.is_finite()), "{ecl_name}: non-finite bullet");
                    }
                    drop(b);
                    let n = m.alive_count();
                    max_alive = max_alive.max(n);
                    spawned += (m.enemies.iter().filter(|e| e.alive() && e.main.ctx.clock.cur == 1).count() > 0) as i32;
                    for e in m.enemies.iter().filter(|e| e.alive()) {
                        assert!(e.pos.iter().all(|v| v.is_finite()), "{ecl_name}: non-finite position");
                    }
                }
                assert!(max_alive > 0, "{ecl_name} diff {diff}: no enemy ever spawned");
                assert!(spawned > 0);
                assert!(max_live <= crate::bullet::MAX_BULLETS);
                fired += bullets.borrow().patterns;
                let p = player.borrow();
                eprintln!(
                    "{ecl_name} d{diff}: patterns {} max live {max_live} laser-frames {lasers} max items {items} collected {} hits {} grazes {} score {}",
                    bullets.borrow().patterns,
                    bullets.borrow().items.collected,
                    p.hits,
                    p.grazes,
                    p.stats.score()
                );
            }
            assert!(index >= 9 || fired > 0, "{}: nothing fired", files(index).0);
        }
    }
}


#[cfg(test)]
mod laser_tests {
    use super::*;

    // Lasers only appear in later boss phases: run the laser stages longer with more debug damage.
    #[test]
    fn laser_stages_run() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else { return };
        let ar = crate::archive::Archive::from_bytes(std::fs::read(format!("{dir}/th08.dat")).unwrap()).unwrap();
        let mut total = 0;
        for index in [2, 4, 6, 7, 8] {
            let (ecl_name, enm) = files(index);
            let mut anm = AnmManager::default();
            for (slot, name) in [(ANM_ENEMY, "enemy.anm"), (ANM_STAGE, enm.as_str()), (crate::bullet::ANM_SLOT, "etama.anm")] {
                anm.insert(slot, name, crate::anm::parse(&ar.read(name).unwrap()).unwrap(), |_| 0);
            }
            let player = crate::player::Player::new_ref();
            let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
            let mut m = EnemyManager::new(ecl::parse(ar.read(&ecl_name).unwrap()).unwrap(), bullets.clone(), 3);
            m.debug_damage = 40;
            let mut lasers = 0;
            for _ in 0..20000 {
                player.borrow_mut().step(&mut anm, &crate::input::Input::default());
                m.step(&mut anm);
                bullets.borrow_mut().step(&mut anm);
                let b = bullets.borrow();
                for l in b.lasers.iter().filter(|l| l.alive) {
                    lasers += 1;
                    assert!(l.head.is_finite() && l.tail.is_finite() && l.tail <= l.head + 1e-3, "{ecl_name}: bad laser");
                }
            }
            eprintln!("{ecl_name}: laser-frames {lasers} hits {}", player.borrow().hits);
            total += lasers;
        }
        assert!(total > 0, "no laser ever spawned");
    }
}
