//! Enemy manager 0x577F20: timelines, enemy objects, update, damage, death, drawing (07 §3, §4, §8, §10, §11).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::{BulletDesc, BulletManager, LaserDesc};
use crate::ecl::{self, Ecl};
use crate::game::Ctx;
use crate::gfx::PLAYFIELD;
use crate::player::PlayerRef;
use crate::stage::Stage;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

pub const CALC_PRIO: u16 = 11;
// 07 §10.6: draw prio 8 → layers 0–1, prio 11 → layers 2–3.
pub const DRAW_LOW_PRIO: u16 = 8;
pub const DRAW_HIGH_PRIO: u16 = 11;
// ANM slots of enemy.anm and stgNenm.anm (flags B bit 2 picks the latter).
pub const ANM_ENEMY: usize = 7;
pub const ANM_STAGE: usize = 8;
// 07 §4: 480 slots of 0x53D0 after the template.
pub const MAX_ENEMIES: usize = 480;

// Flags A (e+0x3324, 07 §4.4).
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
// Bits 12–13 movement mode (07 §8), 14–16 easing (07 §5.6 table).
pub const A_MOVE_SHIFT: u32 = 12;
pub const A_EASE_SHIFT: u32 = 14;
pub const A_STORE_FIRE: u32 = 1 << 17;
pub const A_MIRROR: u32 = 1 << 18;
pub const A_CLAMP: u32 = 1 << 19;
// Bits 20–22 death mode (op 129, 07 §10.3).
pub const A_DEATH_SHIFT: u32 = 20;
pub const A_DEAD_CONT: u32 = 1 << 23;
pub const A_ON_SCREEN: u32 = 1 << 24;
pub const A_NO_PUSH: u32 = 1 << 26;
pub const A_FIXED_BONUS: u32 = 1 << 27;
pub const A_KEEP_OFFSCREEN: u32 = 1 << 28;
pub const A_NO_MOVE: u32 = 1 << 29;
// Set by ops 173 / 176 (07 §7); pauses the script while the player bombs (07 §4.4 flags B bit 7).
pub const A_LAST_SPELL: u32 = 1 << 30;
// Flags B (e+0x3328, 07 §4.4).
pub const B_STAGE_ANM: u32 = 1 << 2;
pub const B_DEATH_LATCH: u32 = 1 << 3;
pub const B_CANNOT_DIE: u32 = 1 << 6;
pub const B_PAUSED: u32 = 1 << 7;
// Bits 4–5 low-life warning level (07 §10.2).
pub const B_WARN_SHIFT: u32 = 4;
pub const B_WARN_MASK: u32 = 3 << B_WARN_SHIFT;

// Item table 0x4C70D8 for item −1 drops (07 §10.4).
const DROP_TABLE: [i32; 32] = [
    0, 0, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1, 0, 0,
];

const ECL_NAMES: [&str; 9] = ["1", "2", "3", "4a", "4b", "5", "6", "7", "8"];

// Card tables 0x4C70E4 / 0x4C7054 for cards 205–221 (07 §1): 205–215 reuse a stage's `sp` ECL and `stgNenm`,
// 216–221 use the per-character `_xx` files.
const LAST_WORD_STAGE: [usize; 11] = [0, 1, 2, 5, 6, 7, 8, 5, 8, 3, 4];
const LAST_WORD_CHAR: [&str; 6] = ["sk", "ym", "al", "rm", "yy", "yk"];

// ECL and slot-8 stage-enemy ANM file names (07 §1 table): `spell` = card id when GameFlags bit 14 is set.
pub fn files(index: usize, spell: Option<i16>) -> (String, String) {
    let k = ECL_NAMES[index];
    let Some(c) = spell else {
        return (format!("ecldata{k}.ecl"), format!("stg{k}enm.anm"));
    };
    match c - 205 {
        n @ 0..=10 => {
            let k = ECL_NAMES[LAST_WORD_STAGE[n as usize]];
            (format!("ecldata{k}sp.ecl"), format!("stg{k}enm.anm"))
        }
        n @ 11..=16 => {
            let x = LAST_WORD_CHAR[n as usize - 11];
            (format!("ecldata_{x}.ecl"), format!("stgenm_{x}.anm"))
        }
        _ => (format!("ecldata{k}sp.ecl"), format!("stg{k}enm.anm")),
    }
}

// Variable interpolation slot, ctx+0x9C + 0x30·k (07 §5.6, op 36).
#[derive(Clone, Copy, Debug, Default)]
pub struct Interp {
    pub active: bool,
    pub elapsed: Timer,
    pub dur: i32,
    // fn index 0–6 linear, 7 Hermite.
    pub func: i32,
    // Easing mode (07 §5.6 table).
    pub mode: i32,
    // Raw a–d, re-read through the float getter at every evaluation.
    pub abcd: [f32; 4],
    // Target var id, the raw dword of a float (e.g. 10016.0).
    pub target: i32,
}

// Script context (0x228 bytes). `vars` = the 30 dwords copied to spawned children and threads:
// 0..8 ints 10000–07, 8..16 floats 10016–23, 16..20 ints 10036–39, 20..22 floats 10094–95,
// 22..26 call-argument ints 10053–56, 26..30 call-argument floats 10057–60.
#[derive(Clone, Debug)]
pub struct Context {
    // +0x00 byte offset into the ECL file.
    pub ip: usize,
    // +0x04 (07 §5.3).
    pub clock: Timer,
    // +0x10 / +0x14 per-frame native (op 137), −1 none; its argument.
    pub callback: i32,
    pub callback_arg: i32,
    // +0x18–0x8F, raw dwords (ints or f32 bits).
    pub vars: [u32; 30],
    // +0x90 op 2 wait.
    pub wait: Timer,
    pub interp: [Interp; 8],
    // +0x224 current sub id.
    pub sub: i32,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            ip: 0,
            clock: Timer::default(),
            callback: -1,
            callback_arg: 0,
            vars: [0; 30],
            wait: Timer::default(),
            interp: [Interp::default(); 8],
            sub: -1,
        }
    }
}

// Offsets into `Context::vars` of each variable group (see above).
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

    // Ecl_SetSub: ip = sub start, clock 0, wait 0, sub id; variables and slots kept (07 §10.2).
    pub fn enter(&mut self, ecl: &Ecl, sub: i32) -> bool {
        let Some(ip) = ecl.sub_start(sub) else {
            return false;
        };
        self.ip = ip;
        self.clock.set(0);
        self.wait.set(0);
        self.sub = sub;
        true
    }
}

// The main context or a thread (op 135, 0x24B0 block, 07 §5.1): a context plus its own 16-entry call stack.
#[derive(Clone, Debug, Default)]
pub struct Thread {
    pub ctx: Context,
    pub stack: Vec<Context>,
    // Main: e+0x2CE8; thread: block +6 (07 §5.4 depth storage).
    pub depth: i32,
}

impl Thread {
    // Push a copy of the whole context at stack[depth]; depth max 15 (07 §5.4).
    pub fn push(&mut self) {
        self.depth = self.push_at(self.depth);
    }

    // Push at stack[d] and return the new depth (max 15).
    pub fn push_at(&mut self, d: i32) -> i32 {
        self.push_copy(d, self.ctx.clone())
    }

    // Push `c` at stack[d] and return the new depth (max 15); an interrupt in a thread pushes main (07 §5.5).
    pub fn push_copy(&mut self, d: i32, c: Context) -> i32 {
        if self.stack.len() < 16 {
            self.stack.resize(16, Context::default());
        }
        self.stack[d.clamp(0, 15) as usize] = c;
        (d + 1).min(15)
    }
}

// Enemy object 0x53D0 (07 §4.3); positions in playfield px, angles in radians, timers in frames.
#[derive(Clone, Debug)]
pub struct Enemy {
    // e+0xC main VM, e+0x2B0 / e+0x554 sub VMs (ops 57/61).
    pub vm: AnmVm,
    pub sub_vms: [AnmVm; 2],
    // e+0x7F8 main context + e+0xA20 call stack.
    pub main: Thread,
    // e+0x2CEA working depth as left by the last `Ecl_Run` (the last context run) or a phase-change reset;
    // only op 88 from another enemy reads it outside `Ecl_Run` (07 §5.4).
    pub work_depth: i32,
    // e+0x3384 op 135 threads.
    pub threads: [Option<Box<Thread>>; 4],
    // e+0x2CA8 vars 10008–15, e+0x2CC8 vars 10024–31.
    pub ivars: [i32; 8],
    pub fvars: [f32; 8],
    // e+0x2CEE op 130, −1 none.
    pub death_sub: i32,
    // e+0x2CF0 sub per interrupt index (op 126).
    pub interrupts: [i32; 32],
    // e+0x2D30 (07 §5.5).
    pub pending_interrupt: i32,
    pub pos: [f32; 3],
    // e+0x2D40 parent position for followers (flags A bit 9).
    pub pos_offset: [f32; 3],
    // e+0x2D4C per frame at game speed 1.
    pub vel: [f32; 3],
    // e+0x2D58 / e+0x2D64 (vars 10085–87).
    pub prev_pos: [f32; 3],
    pub displacement: [f32; 3],
    // e+0x2D70 (op 77) / e+0x2D7C (op 78, used if x > 0).
    pub hitbox: [f32; 3],
    pub shotbox: [f32; 3],
    // e+0x2D88 pos + offset, z forced 0 after movement.
    pub final_pos: [f32; 3],
    pub angle: f32,
    pub angvel: f32,
    pub orbit_angle: f32,
    pub orbit_av: f32,
    pub speed: f32,
    pub accel: f32,
    pub radius: f32,
    pub radius_vel: f32,
    // e+0x2DB8 (op 110).
    pub shot_offset: [f32; 3],
    // e+0x2DC4 / e+0x2DD0: mode-2 delta / start, or the mode-3 orbit centre.
    pub interp_delta: [f32; 3],
    pub interp_start: [f32; 3],
    // e+0x2DDC / e+0x2DE8.
    pub move_timer: Timer,
    pub move_dur: i32,
    // e+0x2DEC–0x2DFA bullet rank ranges (op 152, 07 §9.2): speed s0/s1, counts c1a c1b c2a c2b.
    pub rank_speed: [f32; 2],
    pub rank_counts: [i32; 4],
    // e+0x2DFC / e+0x2E00 / e+0x2E04 (HUD).
    pub life: i32,
    pub max_life: i32,
    pub hud_life: i32,
    pub score: i32,
    // e+0x2E0C slot index.
    pub slot: i32,
    // e+0x2E1C (var 10049).
    pub f2e1c: i32,
    // e+0x2E14 boss/spell timer (op 132); held by the freeze byte 0x160F534.
    pub boss_timer: Timer,
    // e+0x2E20 main VM colour copied after the spawn run (07 §4.1).
    pub main_color: [u8; 4],
    // e+0x2E24 (07 §9).
    pub desc: BulletDesc,
    // e+0x3034 stored 44-byte bullet instruction for auto-fire.
    pub stored_fire: Option<Vec<u8>>,
    // e+0x3060 / e+0x3064.
    pub autofire_interval: i32,
    pub autofire_timer: Timer,
    // e+0x3070 (07 §9.4).
    pub laser_desc: LaserDesc,
    // e+0x3280 laser slots (bullet-manager indexes), e+0x3300 current.
    pub lasers: [Option<usize>; 32],
    pub laser_cur: usize,
    // e+0x3304 item drop (var 10092); e+0x3308 / e+0x330C extra point / power items.
    pub item: i32,
    pub extra_point: i32,
    pub extra_power: i32,
    // e+0x3310–0x3312 death-effect parameters (op 138).
    pub death_bytes: [u8; 3],
    // e+0x3313 (var 10084).
    pub boss_slot: i32,
    pub flags: u32,
    pub flags_b: u32,
    // e+0x332E current direction anim / e+0x332F draw layer.
    pub dir: i8,
    pub layer: u8,
    // e+0x3330 rank-mask mode bits (07 §2.3): 0x20 human, 0x40 youkai.
    pub mode_bits: u8,
    // e+0x3332–0x333C: idle, left, right, left→idle, right→idle, replay.
    pub dir_scripts: [i32; 6],
    // e+0x3340 xmin, ymin, xmax, ymax.
    pub clamp: [f32; 4],
    // e+0x3350 no-fire radius², px².
    pub nofire_r2: f32,
    // e+0x3354 (var 10083).
    pub last_damage: i32,
    // e+0x3358 / e+0x3368 (op 133).
    pub thresholds: [i32; 4],
    pub threshold_subs: [i32; 4],
    // e+0x3378 / e+0x337C (op 134).
    pub timeout: i32,
    pub timeout_sub: i32,
    // e+0x3380 chained children created minus killed by damage (07 §4.3).
    pub children: i32,
    // e+0x4 / e+0x8 chain links, e+0x2DA4 parent (the spawner, 07 §10.5).
    pub chain_prev: Option<usize>,
    pub chain_next: Option<usize>,
    pub root: Option<usize>,
    // e+0x5354 damage-reduction timer (op 160).
    pub dmg_timer: Timer,
    // e+0x2E10 time-orb damage accumulator (07 §4.3).
    pub orb_acc: i32,
    // e+0x53CC spell-capture bonus seconds.
    pub bonus_seconds: i32,
    // Port: damage taken this frame / hitByBomb / hit-flash flag (07 §10.1 steps 6–7).
    pub hit: bool,
    pub bomb_hit: bool,
    pub flash: bool,
    // Port: a thread returned from its first sub while running (07 §5.4 return).
    pub kill_running_thread: bool,
    pub running_thread: Option<usize>,
    // e+0x534C–0x5352: trail flags, length n, op 157 arg a, draw step b.
    pub trail: [i32; 4],
    // e+0x3394 after-image ring (07 §4.3, §10.1 step 4): entry 0 newest; template x = −999.
    pub trail_ring: [TrailEntry; TRAIL_LEN],
    // e+0x53C0 effect count (op 128 and friends); handles in `fx_attached`.
    pub effects: u32,
    pub fx_attached: Vec<usize>,
    // e+0x53C4 effect parameter target (op 128 d).
    pub orbit_target: f32,
    // e+0x53C8 aura effect.
    pub fx_aura: usize,
}

// Trail strip angle midpoint (07 §10.6): lo + d/2 with d = min(|a − c|, lo + 2π − hi).
// QUIRK: the wrap distance is taken from lo, not the true shortest-arc midpoint.
fn trail_mid(a: f32, c: f32) -> f32 {
    let (lo, hi) = (a.min(c), a.max(c));
    lo + 0.5 * (a - c).abs().min(lo + 2.0 * std::f32::consts::PI - hi)
}

// e+0x3394 ring length: 96 entries of 0x1C (07 §4.3).
pub const TRAIL_LEN: usize = 96;

// One 0x1C ring entry (07 §10.1 step 4): final pos, velocity e+0x2D4C, angle e+0x2D94.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailEntry {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub angle: f32,
}

impl Default for TrailEntry {
    // EnemyMgr_Ctor writes only x = −999, so an unwritten entry is off screen (07 §10.1 step 4).
    fn default() -> Self {
        Self {
            pos: [-999.0, 0.0, 0.0],
            vel: [0.0; 3],
            angle: 0.0,
        }
    }
}

impl Default for Enemy {
    // EnemyMgr_Ctor template defaults (07 §4.2).
    fn default() -> Self {
        Self {
            vm: AnmVm::default(),
            sub_vms: [AnmVm::default(), AnmVm::default()],
            main: Thread::default(),
            work_depth: 0,
            threads: Default::default(),
            ivars: [0; 8],
            fvars: [0.0; 8],
            death_sub: -1,
            // 07 §4.2: zero-filled by the constructor, so an unregistered index enters sub 0.
            interrupts: [0; 32],
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
            bomb_hit: false,
            flash: false,
            kill_running_thread: false,
            running_thread: None,
            trail: [0; 4],
            trail_ring: [TrailEntry::default(); TRAIL_LEN],
            effects: 0,
            fx_attached: Vec::new(),
            orbit_target: 0.0,
            fx_aura: crate::effect::NONE,
        }
    }
}

impl Enemy {
    // e+0x534E trail length n; the clamp is a port-only bound for the 96-entry ring (data: 15 and 90).
    fn trail_n(&self) -> usize {
        self.trail[1].clamp(0, TRAIL_LEN as i32) as usize
    }

    // Ring shift (07 §10.1 step 4): entry[i] = entry[i − 1] for i = n − 1..1, then entry[0] = now.
    fn push_trail(&mut self) {
        let n = self.trail_n();
        if n > 1 {
            self.trail_ring.copy_within(0..n - 1, 1);
        }
        self.trail_ring[0] = TrailEntry {
            pos: self.final_pos,
            vel: self.vel,
            angle: self.angle,
        };
    }

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

    // Sprite file by flags B bit 2 (07 §7 ops 54/58).
    pub fn anm_slot(&self) -> usize {
        if self.flags_b & B_STAGE_ANM != 0 {
            ANM_STAGE
        } else {
            ANM_ENEMY
        }
    }

    // Enemy_ClampPos 0x42C180 (07 §8): only with flag A bit 19.
    pub fn clamp_pos(&mut self) {
        if self.flags & A_CLAMP != 0 {
            let [x0, y0, x1, y1] = self.clamp;
            self.pos[0] = self.pos[0].max(x0).min(x1);
            self.pos[1] = self.pos[1].max(y0).min(y1);
        }
    }

    // Phase-change / death reset (07 §10.2): speed −0.5…+0.5, not the template's ±0.15.
    pub fn reset_rank_ranges(&mut self) {
        self.rank_speed = [-0.5, 0.5];
        self.rank_counts = [0; 4];
    }

    // Enemy_UpdateMoveMode 0x422C40 (07 §8 table).
    pub fn update_move_mode(&mut self, gs: f32) {
        let mirror = self.flags & A_MIRROR != 0;
        match self.move_mode() {
            1 => {
                self.angle = anmvm::wrap_add(self.angle, gs * self.angvel);
                self.speed += gs * self.accel;
                // Vec3_FromPolar 0x422CED (05 §10 trig sites): one f32 rounding of cos·speed.
                self.vel = crate::bullet::polar(self.angle, self.speed);
                self.tick_duration(gs);
            }
            2 => {
                self.move_timer.dec(gs);
                let dur = self.move_dur.max(1) as f32;
                let t = (1.0 - self.move_timer.getf() / dur).max(0.0);
                let t = anmvm::ease(((self.flags >> A_EASE_SHIFT) & 7) as u8, t);
                let target: [f32; 3] =
                    std::array::from_fn(|k| self.interp_start[k] + self.interp_delta[k] * t);
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
                // Vec3_FromPolar (05 §10): cos/sin·r in double, one f32 rounding.
                let o = crate::bullet::polar(self.orbit_angle, self.radius);
                self.vel[0] = c[0] + o[0] - self.pos[0];
                self.vel[1] = c[1] + o[1] - self.pos[1];
                self.angle = self.vel[1].atan2(self.vel[0]);
                self.tick_duration(gs);
            }
            _ => {}
        }
    }

    // Modes 1/3 with a duration: timer ≤ 0 → mode 0, velocity kept (07 §8).
    fn tick_duration(&mut self, gs: f32) {
        if self.move_dur > 0 {
            self.move_timer.dec(gs);
            if self.move_timer.cur <= 0 {
                self.set_move_mode(0);
            }
        }
    }

    // Phase-change cleanup; the depth := 0 only hits the working depth, so the main depth stays (07 §5.4).
    pub fn reset_for_jump(&mut self) {
        self.work_depth = 0;
        self.threads = Default::default();
        self.desc = BulletDesc::default();
        self.autofire_interval = 0;
        self.reset_rank_ranges();
    }
}

// Running spell card (op 122 block, 07 §11; spell object g_Gui 0x4EA670, §11.1).
#[derive(Clone, Debug, Default)]
pub struct Spell {
    pub id: i32,
    // +0x0C portrait/face parameter.
    pub face: i32,
    // Decoded name / owner / comment lines (XOR 0xAA / 0xBB / 0xDD / 0xEE, Shift-JIS).
    pub name: Vec<u8>,
    pub owner: Vec<u8>,
    pub comment: [Vec<u8>; 2],
    // Bonus +0xFC, max +0x2638, per-second decay rate R +0x100 = (bonus − bonus/7) / (timeout/60) (07 §11.1).
    pub bonus: i32,
    pub max: i32,
    pub decay: i32,
    // g_Gui +4: the boss that started the card (enemy slot); g_Gui +0xC: its slot index e+0x2E0C (07 §11.1 step 1.1).
    pub boss: usize,
    pub slot: i32,
    // g_Gui +0x114 card duration (07 §11.1); the remaining time +0x108 is `Globals::spell_remaining`.
    pub duration: i32,
    // Port: death / bomb counters at card start.
    pub deaths0: u32,
    pub bombs0: u32,
    // Spell flag bit 2 capture possible, bit 3 timed out, bit 7 bomb started during the card, bit 5 last spell.
    pub capture: bool,
    pub timed_out: bool,
    pub bombed: bool,
    pub last: bool,
}

thread_local! {
    static TIMER_PAUSE: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    static LOAD_FRAMES: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

// Global freeze byte 0x160F534: native 26 sets it, Player_Die clears it, enemy timers and bullets read it live.
pub fn timer_pause() -> i32 {
    TIMER_PAUSE.get()
}

pub fn set_timer_pause(v: i32) {
    TIMER_PAUSE.set(v);
}

// Global 0x18B8A2C "frames since load" (13 §3): ECL native 30 (= arg), 2 at the end of StageStart, 1 by
// GameMgr_OnRemove, 0 by Result / MainMenu register; Supervisor_Calc decrements it once per frame while ≠ 0.
// Read by screen effects 1 / 2 / 4, which are removed while it is ≠ 0.
pub fn load_frames() -> i32 {
    LOAD_FRAMES.get()
}

pub fn set_load_frames(v: i32) {
    LOAD_FRAMES.set(v);
}

// Globals touched by ECL (difficulty, rank, argument registers, spell state, …).
#[derive(Clone, Debug, Default)]
pub struct Globals {
    // 0x160F538 (var 10040): 0 Easy … 3 Lunatic, 4 Extra.
    pub difficulty: i32,
    // GM+0x3DE2C (var 10041, 07 §9.2).
    pub rank: i32,
    // 0x4ECE20–0x4ECE3C vars 10061–10068, copied into the callee on a call (07 §5.4).
    pub args: [u32; 8],
    // 0x17D61AC (vars 10045–47).
    pub player: [f32; 3],
    // Byte 0x164D0B1 (var 10052).
    pub player_char: i32,
    // Player byte +5 (var 10097).
    pub player_mode: i32,
    pub spell: Option<Spell>,
    // g_Gui +0x108 remaining card time (var 10100, 07 §6): set at Spell_Start, decremented at game speed by every
    // Spell_Calc, also after the card; zero-filled at stage start (07 §11.1).
    pub spell_remaining: Timer,
    // 0xF54E2C (op 175).
    pub spawn_disabled: i32,
    // HUD +0x2C boss display byte (07 §3 timeline op 0).
    pub hud_boss: bool,
    pub dialogue: bool,
    // mgr+0x9DCDC0 / +0x9DCDC2 (07 §10.4).
    pub drop_counter: u16,
    pub drop_index: u16,
    // Byte 0x164D0BB (timeline op 16).
    pub flag_164d0bb: bool,
    // 0xF54CEC (op 163), 0x4E4B60 (native 5).
    pub g_f54cec: i32,
    pub g_4e4b60: i32,
    // HUD +0x3C / +0x5C / +0x7C life-bar markers [end, start, colour]: fractions stored by op 158 (12 §5.1).
    pub hud_markers: [(f32, f32, u32); 8],
    // HUD +0x30 boss bar target life / max life, no clamp (12 §5.1); written in the step-7 tail and by op 127.
    pub hud_bar_target: f32,
    // HUD +0x24 boss timer seconds, written only by Enemy_CheckTimeout (12 §5.1, 07 §10.2).
    pub hud_timer: i32,
    // 0x4ECCA8 = 99 999 990 (op 155).
    pub fixed_bonus: bool,
    // Spell flag bit 6 (op 164: the start ring stops following the boss) and bit 11 (op 184: bonus frozen), 07 §11.1.
    pub spell_flag6: bool,
    pub bonus_frozen: bool,
    // g_Gui +4 boss pointer (enemy slot): set by Spell_Start, kept after a silent end (step 1.1), cleared by the
    // Spell_End tail (07 §11.1).
    pub spell_boss: Option<usize>,
    // Spell flag bit 9 "captured" (07 §11.1); read by the continue menu default (12 §4).
    pub spell_captured: bool,
    // Port: op 148 argument, HUD requests (ops 148/179–181) handled by the HUD, unimplemented-op counter.
    pub hud_bars: i32,
    pub hud_requests: Vec<u16>,
    pub misc_calls: u64,
    // Spell-practice card id.
    pub card: i16,
    pub payout: Option<Payout>,
}

// Capture payout pending in the spell object (07 §11.1, bit 8): paid at ring timer 130 or at the next card start.
#[derive(Clone, Copy, Debug)]
pub struct Payout {
    // +0x104 bonus, +0x10 time reward still to be spawned as time orbs.
    pub bonus: i32,
    pub time: i32,
}

// Enemy manager 0x577F20 (07 §4).
pub struct EnemyManager {
    pub enemies: Vec<Box<Enemy>>,
    spare: Vec<Box<Enemy>>,
    // mgr+0 template enemy (07 §4.2), copied into every spawned slot.
    template: Box<Enemy>,
    pub ecl: Ecl,
    // 0xF54CC0[4] boss table.
    pub bosses: [Option<usize>; 4],
    // mgr+0x9DCDD0 + 16·i: {clock, ip} per timeline (07 §3).
    timelines: Vec<(Timer, usize)>,
    // mgr+0x9DCEFC sync slots (timeline ops 13/14), −1 empty.
    pub sync: [i32; 4],
    pub g: Globals,
    pub bullets: Rc<RefCell<BulletManager>>,
    pub player: PlayerRef,
    pub stage: Option<Rc<RefCell<Stage>>>,
    // Manager "full" flag: the last spawn found no slot (07 §4.1).
    pub full: bool,
    // Draw lists by layer, linked at the end of each enemy's calc (07 §10.1 step 7).
    draw_lists: [Vec<usize>; 4],
    // mgr+0x9DCED0 manager clock (11 §3.2): a Timer ticked at game speed, zero-filled at construction.
    pub clock: Timer,
    // Debug: `TH08_ENEMY_DMG`, `TH08_STAGE_FRAME`.
    pub debug_damage: i32,
    pub fast_forward: u32,
    pub msg: Option<Rc<RefCell<crate::msg::Dialogue>>>,
    pub stage_index: usize,
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
        let timelines = ecl
            .timelines
            .iter()
            .map(|&o| (Timer::default(), o))
            .collect();
        // Rank is GM+0x3DE2C, synced from the game manager every frame (11 §3.2).
        let player = bullets.borrow().player.clone();
        player.borrow_mut().stats.difficulty = difficulty;
        let step = player.borrow().orb_step;
        set_timer_pause(0);
        let g = Globals {
            difficulty,
            rank: 16,
            player: player.borrow().pos,
            spell_remaining: Timer {
                prev: 0,
                frac: 0.0,
                cur: 0,
            },
            ..Default::default()
        };
        Self {
            enemies: (0..MAX_ENEMIES)
                .map(|_| {
                    Box::new(Enemy {
                        flags: 0,
                        ..Enemy::default()
                    })
                })
                .collect(),
            spare: Vec::new(),
            // EnemyMgr_Ctor: the template's time-orb accumulator starts at the player's step (07 e+0x2E10).
            template: Box::new(Enemy {
                orb_acc: step,
                ..Enemy::default()
            }),
            ecl,
            bosses: [None; 4],
            timelines,
            sync: [-1; 4],
            g,
            bullets,
            player,
            stage: None,
            full: false,
            draw_lists: Default::default(),
            clock: Timer {
                prev: 0,
                frac: 0.0,
                cur: 0,
            },
            debug_damage: 0,
            fast_forward: 0,
            msg: None,
            stage_index: 0,
        }
    }

    pub fn register(
        ctx: &mut Ctx,
        index: usize,
        stage: Option<Rc<RefCell<Stage>>>,
        bullets: Rc<RefCell<BulletManager>>,
        diff: i32,
    ) -> Result<Rc<RefCell<EnemyManager>>> {
        let spell = ctx.game.spell_practice().then_some(ctx.game.card);
        let (ecl_name, enm) = files(index, spell);
        let ecl = ecl::parse(ctx.archive.read(&ecl_name)?)?;
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_ENEMY, "enemy.anm")?;
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_STAGE, &enm)?;
        let mut m = EnemyManager::new(ecl, bullets, diff);
        m.stage = stage;
        m.stage_index = index;
        m.g.card = ctx.game.card;
        m.fast_forward = ctx.settings.stage_frame;
        m.debug_damage = ctx.settings.enemy_dmg;
        m.stage_start(&mut ctx.anm, index);
        let m = Rc::new(RefCell::new(m));
        let t: task::TaskRef<Ctx> = m.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        Ok(m)
    }

    // Stage start values owned by the game manager (11 §3.1) and EnemyManager_Load's two draws (11 §1.5, 07 §10.4).
    pub fn stage_start(&mut self, anm: &mut AnmManager, index: usize) {
        let d = self.g.difficulty.clamp(0, 4) as usize;
        let mut p = self.player.borrow_mut();
        p.stats.time_threshold = crate::player::TIME_THR
            .get(index % 9)
            .map_or(10, |r| r[d.min(3)]);
        p.stats.time = 0;
        p.stats.time_added = 0;
        (
            p.stats.stage_frames,
            p.stats.human_frames,
            p.stats.youkai_frames,
        ) = (0, 0, 0);
        drop(p);
        self.g.drop_counter = anm.rng.range16(3);
        self.g.drop_index = anm.rng.range16(8);
    }

    // Player collision (07 §10.1 step 6): graze box hitbox/0.7, contact box hitbox/1.5, shot/bomb damage.
    fn player_collision(&mut self, idx: usize, anm: &mut AnmManager) {
        // Graze, contact death and the explosion hit filter read the card flag live (09 §2.1 hit 1, §9.2): a card
        // ended earlier in this calc (step 4) already counts as none.
        self.player.borrow_mut().env.spell = self.g.spell.is_some();
        let e = &self.enemies[idx];
        let (flags, pos, hb, sb) = (e.flags, e.final_pos, e.hitbox, e.shotbox);
        let boss = flags & A_BOSS != 0;
        // (a) graze every 6 frames of the enemy timer for flag A bit 7, (b) contact unless a chained child
        // under shot type 0/4; contact costs a non-boss without bit 7 10 life per frame.
        if flags & A_CONTACT != 0 {
            let e = &self.enemies[idx];
            let graze = flags & A_CANCEL_BONUS != 0
                && e.boss_timer.cur != e.boss_timer.prev
                && e.boss_timer.cur % 6 == 0;
            let child = e.root.is_some();
            let mut p = self.player.borrow_mut();
            if graze {
                let env = p.env;
                p.graze_test(pos, [hb[0] / 0.7, hb[1] / 0.7], &env, &mut anm.rng);
            }
            let touched = !(child && matches!(p.shot_type, 0 | 4))
                && p.contact_test(pos, [hb[0] / 1.5, hb[1] / 1.5], &mut anm.rng);
            let drop_attracted = std::mem::take(&mut p.drop_attracted);
            drop(p);
            if drop_attracted {
                self.bullets.borrow_mut().items.drop_attracted();
            }
            if touched && !boss && flags & A_CANCEL_BONUS == 0 {
                self.enemies[idx].life -= 10;
            }
        }
        if flags & A_SHOT_HIT != 0 {
            let mut acc = self.enemies[idx].orb_acc;
            // hitByBomb starts as "player is bombing" but Player_DamageAgainst overwrites it (07 §10.1 step 6).
            let mut by_bomb = self.player.borrow().bombing;
            let d = {
                let mut b = self.bullets.borrow_mut();
                let mut p = self.player.borrow_mut();
                // Homing-target pick, not gated (07 §10.1 step 6); the original runs it after the damage calls, which do not read it.
                p.offer_target(pos, boss);
                // The first call is not made for a parented enemy during a card while bombing (07 §10.1 step 6).
                let skip = self.g.spell.is_some() && self.enemies[idx].root.is_some() && p.bombing;
                let mut d = if skip {
                    0
                } else {
                    p.damage_against(
                        anm,
                        Some(&mut b.items),
                        pos,
                        [hb[0], hb[1]],
                        &mut acc,
                        &mut by_bomb,
                    )
                };
                if sb[0] > 0.0 {
                    let d2 = p.damage_against(
                        anm,
                        Some(&mut b.items),
                        pos,
                        [sb[0], sb[1]],
                        &mut acc,
                        &mut by_bomb,
                    );
                    // Extra shot box d2 is added only when the second call left hitByBomb 0.
                    if !by_bomb {
                        let k = if matches!(p.shot_type, 3 | 11) {
                            6.5
                        } else {
                            1.7
                        };
                        d = (d as f32 + d2 as f32 / k) as i32;
                    }
                }
                d
            };
            // Aimed-enemy offer only for shootable enemies, after both damage calls (14 §3.1, 0x42D46A, RE-Q54).
            self.offer_aimed(idx);
            self.enemies[idx].orb_acc = acc;
            self.enemies[idx].bomb_hit = by_bomb;
            self.damage(idx, d, by_bomb);
        }
    }

    // The running enemy is swapped out for a placeholder whose alive bit keeps its slot reserved. The placeholder
    // keeps the chain links (07 §6 var 10096: the original's walks pass through the running enemy).
    pub(crate) fn take(&mut self, idx: usize) -> Box<Enemy> {
        let mut p = self.spare.pop().unwrap_or_default();
        p.flags = A_ALIVE;
        let e = &self.enemies[idx];
        (p.chain_prev, p.chain_next, p.root) = (e.chain_prev, e.chain_next, e.root);
        std::mem::replace(&mut self.enemies[idx], p)
    }

    // Links changed in the slot during the run (a neighbour unlinked, the running enemy unlinked) win.
    pub(crate) fn put(&mut self, idx: usize, mut e: Box<Enemy>) {
        let p = &self.enemies[idx];
        (e.chain_prev, e.chain_next, e.root) = (p.chain_prev, p.chain_next, p.root);
        let p = std::mem::replace(&mut self.enemies[idx], e);
        self.spare.push(p);
    }

    // Enemy_Free: also clears the aimed pointer (14 §3.1, 07 §10.1); a boss fades its ENEMY marker out (12 §5.10).
    pub fn free(&mut self, idx: usize) {
        let e = &self.enemies[idx];
        if e.flags & A_BOSS != 0 && (0..4).contains(&e.boss_slot) {
            crate::ascii::marker_release(e.boss_slot as usize);
        }
        if self.player.borrow().aimed == Some(idx) {
            self.player.borrow_mut().aimed = None;
        }
        self.release(idx);
        self.unregister_boss(idx);
    }

    // Death mode 0: like Enemy_Free but the aimed pointer survives until the next frame's slot test, and the boss
    // table entry and flags A bit 1 stay, so the ENEMY marker keeps its last position (07 §10.3 rows 0–1, 12 §5.10).
    fn release(&mut self, idx: usize) {
        self.kill_chain(idx, None);
        self.release_effects(idx);
        self.enemies[idx].flags &= !A_ALIVE;
    }

    // HUD +0x2C (boss display) is cleared when any boss slot unregisters (answers_phase5 impl8).
    pub(crate) fn unregister_boss(&mut self, idx: usize) {
        for b in self.bosses.iter_mut() {
            if *b == Some(idx) {
                *b = None;
                self.g.hud_boss = false;
            }
        }
    }

    pub fn boss(&self, slot: i32) -> Option<usize> {
        self.bosses
            .get(usize::try_from(slot).ok()?)
            .copied()
            .flatten()
    }

    pub fn alive_count(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive()).count()
    }

    // Rank mask D (07 §2.3).
    pub fn rank_mask(&self, mode_bits: u8) -> u8 {
        let d = self.g.difficulty;
        (if d < 4 { 1u8 << d } else { 0x0F }) | mode_bits
    }

    // EnemyMgr_SpawnFromTimeline 0x42A4E0 / _FromEcl 0x42A680 steps 1–3 (07 §4.1); the caller finishes with
    // `spawn_finish`. Only pos is written: prev pos and final pos keep the template's (0, 0, 0), so the first
    // move's displacement (vars 10085–87) equals the spawn pos (07 §4.1 step 3 QUIRK).
    pub(crate) fn spawn_prepare(
        &mut self,
        sub: i32,
        pos: [f32; 3],
        life: i32,
        mirror: bool,
    ) -> Option<usize> {
        let Some(idx) = self.enemies.iter().position(|e| !e.alive()) else {
            self.full = true;
            return None;
        };
        let mut e = self.template.clone();
        e.slot = idx as i32;
        // Mirror bit 18: all x motion negated (07 §3 timeline op 1).
        e.set_flag(A_MIRROR, mirror);
        if life >= 0 {
            e.life = life;
        }
        e.pos = pos;
        e.main.ctx.enter(&self.ecl, sub);
        self.enemies[idx] = e;
        Some(idx)
    }

    pub(crate) fn spawn_finish(
        &mut self,
        anm: &mut AnmManager,
        idx: usize,
        life: i32,
        item: i32,
        score: i32,
        from_ecl: bool,
    ) -> Option<usize> {
        // 07 §4.1 steps 4–5: time-0 instructions run at once; op 1 frees the slot.
        if crate::eclvm::ecl_run(self, anm, idx) == -1 {
            self.free(idx);
            return None;
        }
        let e = &mut self.enemies[idx];
        e.main_color = e.vm.color;
        // Item is taken as a signed char (07 §4.1 step 5).
        e.item = item as i8 as i32;
        if score >= 0 {
            e.score = score;
        }
        // ECL spawns re-apply life ≥ 0 after the run.
        if from_ecl && life >= 0 {
            e.life = life;
        }
        e.max_life = e.life;
        e.hud_life = e.life;
        Some(idx)
    }

    pub fn spawn(
        &mut self,
        anm: &mut AnmManager,
        sub: i32,
        pos: [f32; 3],
        life: i32,
        item: i32,
        score: i32,
        mirror: bool,
    ) -> Option<usize> {
        let idx = self.spawn_prepare(sub, pos, life, mirror)?;
        self.spawn_finish(anm, idx, life, item, score, false)
    }

    fn spawn_blocked(&self) -> bool {
        // 0x42F320 = HUD boss display byte: spawns are blocked while a slot-0 boss is registered.
        self.g.hud_boss || self.g.spawn_disabled != 0
    }

    // EclTimeline_Run 0x42A8A0 (07 §3): late instructions are skipped, not stalled.
    fn run_timelines(&mut self, anm: &mut AnmManager) {
        let gs = anm.speed;
        let dmask = self.rank_mask(0);
        for i in 0..self.timelines.len() {
            loop {
                let (clock, ptr) = self.timelines[i];
                let Some(ins) = self.ecl.tl_ins(ptr) else {
                    break;
                };
                if ins.time < 0 {
                    self.timelines[i].0.tick(gs);
                    break;
                }
                if clock.cur == ins.time {
                    if ins.rank & dmask != 0 && !self.timeline_op(anm, &ins) {
                        // Stall (07 §3): Decrement then the normal Tick, net zero even at game speed < 1.
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

    // Timeline ops (07 §3 table). Returns false to stall.
    fn timeline_op(&mut self, anm: &mut AnmManager, ins: &ecl::TlIns) -> bool {
        let args: [u32; 7] = std::array::from_fn(|k| self.ecl.tl_arg(ins, k));
        let (ai, af) = (
            |k: usize| args[k] as i32,
            |k: usize| f32::from_bits(args[k]),
        );
        match ins.op {
            0 | 1 | 15 => {
                if ins.op == 15 || !self.spawn_blocked() {
                    self.spawn(
                        anm,
                        ai(0),
                        [af(1), af(2), 0.0],
                        ai(3),
                        ai(4),
                        ai(5),
                        ins.op == 1,
                    );
                }
            }
            2 | 4 => {
                if !self.spawn_blocked() {
                    let x = anm.rng.next_f32() * (af(2) - af(1)) + af(1);
                    self.spawn(
                        anm,
                        ai(0),
                        [x, af(3), 0.0],
                        ai(4),
                        ai(5),
                        ai(6),
                        ins.op == 4,
                    );
                }
            }
            3 | 5 => {
                if !self.spawn_blocked() {
                    let x = anm.rng.next_f32() * 384.0;
                    self.spawn(
                        anm,
                        ai(0),
                        [x, af(1), 0.0],
                        ai(2),
                        ai(3),
                        ai(4),
                        ins.op == 5,
                    );
                }
            }
            6 => self.start_msg(anm, ai(0)),
            7 => return !self.msg.as_ref().is_some_and(|m| m.borrow().blocking()),
            8 => {
                if let Some(b) = self.boss(ai(0)) {
                    self.enemies[b].pending_interrupt = ai(1);
                }
            }
            // GameMgr_SetPower 0x406FA0: power := n, checksum (4 RNG draws) (07 timeline op 9, 09 §9.4).
            9 => self
                .player
                .borrow_mut()
                .stats
                .set_power(ai(0) as f32, &mut anm.rng),
            // A mode-0 boss stays in the table, so the alive bit is tested too (07 §10.3).
            10 => {
                return self.boss(ai(0)).is_none_or(|b| !self.enemies[b].alive());
            }
            11 | 12 => {
                if !self.spawn_blocked()
                    && let Some(idx) = self.spawn(
                        anm,
                        ai(0),
                        [af(1), af(2), 0.0],
                        ai(3),
                        -1,
                        ai(6),
                        ins.op == 12,
                    )
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
                self.sync
                    .iter_mut()
                    .filter(|s| **s < 0)
                    .for_each(|s| *s = v);
            }
            // 0x164D0BB = 1: game-over / continue byte (11 §2).
            16 => self.player.borrow_mut().game_over = true,
            _ => {}
        }
        true
    }

    // Enemy_Move 0x42DEB0 (07 §8): clamp, step by velocity (x mirrored), clamp, follow parent, final pos z 0.
    // Flag A bit 29 skips only the clamp/step part.
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

    // Screen check (07 §10.1 step 4); returns true when the enemy left the screen and must be freed.
    // No free test on the pass that first sets bit 24.
    fn screen_check(&mut self, idx: usize) -> bool {
        let e = &mut self.enemies[idx];
        if !e.vm.has_sprite {
            e.flags |= A_HIDDEN;
        }
        if e.flags & A_HIDDEN != 0 {
            return false;
        }
        // Main VM sprite box, inclusive (07 §10.1 step 4).
        let (hw, hh) = (e.vm.sprite_w / 2.0, e.vm.sprite_h / 2.0);
        let on_screen = |[x, y, _]: [f32; 3]| {
            x + hw >= 0.0 && x - hw <= 384.0 && y + hh >= 0.0 && y - hh <= 448.0
        };
        let inside = on_screen(e.final_pos);
        if e.flags & A_ON_SCREEN == 0 {
            if inside {
                e.flags |= A_ON_SCREEN;
            }
            false
        } else {
            // With a trail the oldest entry n − 1 must be off screen too (port bound: n = 0 reads entry 0).
            let oldest = e.trail[0] != 0 && on_screen(e.trail_ring[e.trail_n().max(1) - 1].pos);
            !inside && !oldest && e.flags & A_KEEP_OFFSCREEN == 0
        }
    }

    pub(crate) fn jump_main(&mut self, idx: usize, sub: i32) {
        let ecl = &self.ecl;
        self.enemies[idx].main.ctx.enter(ecl, sub);
    }

    // Jump to the death sub with the full context reset; the death sub is consumed (07 §10.3, §10.6).
    pub(crate) fn jump_to_death_sub(&mut self, idx: usize) {
        let sub = self.enemies[idx].death_sub;
        if sub >= 0 {
            let e = &mut self.enemies[idx];
            e.reset_for_jump();
            e.thresholds = [-1; 4];
            e.timeout = -1;
            self.jump_main(idx, sub);
            self.enemies[idx].death_sub = -1;
        }
    }

    // EnemyMgr_KillAll(8000, 0) 0x42EFB0 (07 §7 op 95): per alive non-boss enemy without flags B bit 6, in slot
    // order: life 0; flag A bit 7 → item 6 + popup at the recomputed final pos and at every 6th trail entry
    // (v = 2000 first, then min(v + 30, 8000)); `Enemy_ChainUnlink`; `Ecl_SetSub` to the death sub (consumed).
    // No self test: an ECL caller must be back in its slot (`Exec::with_self_in_slot`) to be included.
    pub(crate) fn kill_all(&mut self, anm: &mut AnmManager) -> i32 {
        const MAX: i32 = 8000;
        let (mut v, mut sum) = (2000, 0);
        for j in 0..self.enemies.len() {
            let e = &mut self.enemies[j];
            if !e.alive() || e.flags & A_BOSS != 0 || e.flags_b & B_CANNOT_DIE != 0 {
                continue;
            }
            e.life = 0;
            if e.flags & A_CANCEL_BONUS != 0 {
                e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
                let mut at = vec![e.final_pos];
                // u8 e+0x534C trail on: entries k = 0, 6, 12, … < n.
                if e.trail[0] as u8 != 0 {
                    at.extend((0..e.trail_n()).step_by(6).map(|k| e.trail_ring[k].pos));
                }
                for p in at {
                    self.spawn_item(anm, p, 6, 1);
                    let colour = if v >= MAX {
                        crate::item::POPUP_MAX
                    } else {
                        crate::item::POPUP_NORMAL
                    };
                    crate::ascii::popup(p, v, colour);
                    sum += v;
                    v = (v + 30).min(MAX);
                }
            }
            self.unlink(j);
            let e = &mut self.enemies[j];
            if e.death_sub >= 0 {
                let sub = std::mem::replace(&mut e.death_sub, -1);
                self.jump_main(j, sub);
            }
        }
        self.add_score(sum);
        sum
    }

    // CancelBonus(8000, lasers) + KillAll(8000): score and Hud_ShowBonus (08 §7, 07 §11.1).
    fn cancel_and_kill_bonus(&mut self, anm: &mut AnmManager) {
        let sum = {
            let mut p = self.player.borrow_mut();
            self.bullets
                .borrow_mut()
                .cancel_bonus(anm, &mut p, 8000, true)
        };
        let kills = self.kill_all(anm);
        let mut p = self.player.borrow_mut();
        p.stats.add_score(sum);
        p.stats
            .popups
            .push(crate::player::Popup::Bonus(sum + kills));
    }

    // Enemy_ChainChildCount 0x41FD40 (07 §6 var 10096), walked from `idx` via +8. The original has no alive
    // test (dead members are always unlinked); this one counts alive members only.
    pub fn chain_count(&self, idx: usize) -> i32 {
        let (mut next, mut n) = (self.enemies[idx].chain_next, 0);
        while let Some(c) = next {
            if n as usize > MAX_ENEMIES {
                break;
            }
            next = self.enemies[c].chain_next;
            if self.enemies[c].alive() {
                n += 1;
            }
        }
        n
    }

    // Enemy_KillChildren 0x42ADB0 (07 §10.5, 13 §4 large popups, 08 §8.4 gauge lock); `anm` None = no drops.
    pub(crate) fn kill_chain(&mut self, idx: usize, mut anm: Option<&mut AnmManager>) {
        if let Some(anm) = anm.as_deref_mut()
            && self.enemies[idx].root.is_some()
        {
            self.kill_single(idx, anm);
            return;
        }
        // A chained child never chain-kills its later siblings; without reward it only unlinks (answers_impl12 §1).
        if self.enemies[idx].root.is_some() {
            self.unlink(idx);
            return;
        }
        if self.enemies[idx].chain_next.is_none() {
            return;
        }
        let mut n = self.chain_count(idx);
        // Popup colour chosen once from the original n (13 §4).
        let colour = match n {
            ..2 => 0xFFFFFFFF,
            2..=5 => 0xFFFFFFD0,
            6..=9 => 0xFFFFFFB0,
            _ => 0xFFFFFF80,
        };
        let parent_pos = self.enemies[idx].pos;
        let mut kids = Vec::new();
        let mut next = std::mem::take(&mut self.enemies[idx].chain_next);
        while let Some(c) = next {
            if kids.len() > MAX_ENEMIES {
                break;
            }
            kids.push(c);
            let e = &mut self.enemies[c];
            next = e.chain_next.take();
            if e.flags & A_FOLLOW != 0 {
                e.pos_offset = parent_pos;
            }
            e.flags |= A_CHAIN_KILLED;
            e.root = None;
            e.chain_prev = None;
        }
        if let Some(anm) = anm.as_deref_mut() {
            let cancel = if self.enemies[idx].flags & A_BOSS != 0 {
                7
            } else {
                9
            };
            let shot = self.player.borrow().shot_type;
            let boss_or_spell = self.bosses.iter().any(Option::is_some) && self.g.spell.is_none();
            for (k, &c) in kids.iter().enumerate() {
                // Orb count c and cancel circle per child (07 §10.3a step 3).
                let orbs = match shot {
                    0..4 => {
                        if n < 8 {
                            2 * n + 10
                        } else {
                            26
                        }
                    }
                    _ if shot % 2 == 0 => {
                        if n < 4 {
                            6 * n + 16
                        } else {
                            40
                        }
                    }
                    _ => {
                        if n < 10 {
                            2 * n + 6
                        } else {
                            26
                        }
                    }
                };
                // Quirk kept (13 §4): n /= 3 inside the loop while bombing, so it compounds per child.
                if self.player.borrow().bombing {
                    n /= 3;
                }
                let e = &mut self.enemies[c];
                crate::ascii::popup_large(e.final_pos, n, 0, colour, 1.0);
                e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
                let pos = e.final_pos;
                self.player
                    .borrow_mut()
                    .cancel_circle([pos[0], pos[1]], 32.0, 2.0, 8, cancel);
                for _ in 0..orbs {
                    let r = anm.rng.next_f32() * (2 * orbs) as f32;
                    let a = anm.rng.next_signed() * std::f32::consts::PI;
                    // Vec3_FromPolar 0x42B01D (05 §10 trig sites).
                    let d = crate::bullet::polar(a, r);
                    self.spawn_item(
                        anm,
                        [pos[0] + d[0], pos[1] + d[1], 0.0],
                        crate::item::TIME_ORB as i32,
                        3,
                    );
                }
                if !boss_or_spell {
                    self.enemies[c].item = 8;
                    self.drop_items(c, anm, Some(0));
                }
                // SE 2 + (k & 1) by the child's index in the chain walk (07 §10.3a, 13 §1.4).
                crate::sound::play_at(crate::sound::ENEP0 + (k & 1) as i32, pos[0]);
            }
        }
        for &c in &kids {
            let e = &mut self.enemies[c];
            e.extra_point = 0;
            e.extra_power = 0;
            e.item = -2;
        }
        let e = &self.enemies[idx];
        // Boss chain kill (13 §4): popup of e+0x3380 at scale 2, 2·count time orbs within 128 px.
        if let Some(anm) = anm {
            let (pos, count) = (e.final_pos, e.children);
            crate::ascii::popup_large(pos, count, 0, 0xFFF0F00F, 2.0);
            for _ in 0..2 * count {
                let r = anm.rng.next_f32() * 128.0;
                let a = anm.rng.next_signed() * std::f32::consts::PI;
                let d = crate::bullet::polar(a, r);
                self.spawn_item(
                    anm,
                    [pos[0] + d[0], pos[1] + d[1], 0.0],
                    crate::item::TIME_ORB as i32,
                    1,
                );
            }
            self.player
                .borrow_mut()
                .cancel_circle([pos[0], pos[1]], 32.0, 1.0, 16, 7);
            self.bullets.borrow_mut().items.collect_timer = 0;
        }
    }

    // Single-kill path of 0x42ADB0 (13 §4, 08 §8.4): gauge −gauge/12, gauge lock 50, one time orb.
    fn kill_single(&mut self, idx: usize, anm: &mut AnmManager) {
        {
            let mut p = self.player.borrow_mut();
            // 07 §10.3a: GaugeAdd(−gauge/12, not forced), so nothing while bombing; clamped.
            if !p.bombing {
                let g = p.stats.gauge as i32;
                p.stats.gauge_add(-g / 12);
            }
            p.gauge_c.set(0);
            p.gauge_b.set(30);
        }
        self.bullets.borrow_mut().items.collect_timer = 50;
        let e = &mut self.enemies[idx];
        e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
        let pos = e.final_pos;
        crate::ascii::popup_large(pos, 1, 0, 0xFFFFFFFF, 1.0);
        self.spawn_item(anm, pos, crate::item::TIME_ORB as i32, 1);
        let e = &mut self.enemies[idx];
        e.item = -2;
        e.extra_point = 0;
        e.extra_power = 0;
        self.unlink(idx);
    }

    // Enemy_ChainUnlink 0x42B2F0 (07 §10.3a). Quirk kept: without a parent only e+4/e+8 are zeroed, so the
    // first child keeps prev = e and its own unlink later re-attaches the rest of the chain to e.
    fn unlink(&mut self, idx: usize) {
        let e = &mut self.enemies[idx];
        let (prev, next) = (e.chain_prev.take(), e.chain_next.take());
        if e.root.take().is_none() {
            return;
        }
        if let Some(p) = prev {
            self.enemies[p].chain_next = next;
        }
        if let Some(n) = next {
            self.enemies[n].chain_prev = prev;
        }
    }

    // Yukari's aimed enemy (14 §3.1): a persistent pick, replaced by a higher parentless enemy in the 64-px column.
    fn offer_aimed(&mut self, idx: usize) {
        let mut p = self.player.borrow_mut();
        let e = &self.enemies[idx];
        if e.root.is_some() || (e.final_pos[0] - p.pos[0]).abs() >= 64.0 {
            return;
        }
        if p.aimed
            .is_none_or(|a| e.final_pos[1] < self.enemies[a].pos[1])
        {
            p.aimed = Some(idx);
        }
    }

    // Dissolve spell (14 §2.17): start clears flag A bit 6 of the bosses; the end kills them.
    fn dissolve_requests(&mut self) {
        let req = std::mem::take(&mut self.player.borrow_mut().dissolve_req);
        for b in self.bosses.into_iter().flatten() {
            if req & crate::player::DISSOLVE_BEGIN != 0 {
                self.enemies[b].flags &= !A_SHOT_HIT;
            }
            if req & crate::player::DISSOLVE_END != 0 {
                self.kill_chain(b, None);
                self.enemies[b].life = 0;
                self.enemies[b].flags &= !A_LAST_SPELL;
            }
        }
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

    // Msg_Start side effects (10 §4.6): cancel bullets, KillAll, attract items.
    pub fn msg_side_effects(&mut self, anm: &mut AnmManager) {
        self.cancel_all(anm, 1);
        self.kill_all(anm);
        self.bullets.borrow_mut().items.attract_all();
    }

    // Spell_End 0x4161B0 (07 §11.1); the payout is queued and made 130 frames later in `step`.
    pub fn finish_spell(&mut self, anm: &mut AnmManager) {
        // 0x4E3D28 (darkness mask alpha) := 0 first, every call; then, with the card active, Spell_HideName.
        crate::ascii::clear_darkness();
        self.end_active_spell(anm);
        // Tail of every op 123 (07 §11.1, RE-Q56): after the time-reward test, the boss loses flag A bit 27,
        // g_Gui +4 := 0, spell flag bit 11 := 0.
        if let Some(b) = self.g.spell_boss.take() {
            self.enemies[b].flags &= !A_FIXED_BONUS;
            self.g.fixed_bonus = false;
        }
        self.g.bonus_frozen = false;
    }

    // Card-active block of Spell_End 0x4161B0 (07 §11.1).
    fn end_active_spell(&mut self, anm: &mut AnmManager) {
        let Some(s) = self.g.spell.take() else { return };
        crate::gui::request(crate::gui::Req::BossHide);
        // Time reward from +0x114 and the remaining-time timer +0x108 (07 §11.1).
        let (total, remaining) = (s.duration, self.g.spell_remaining.cur);
        crate::sound::play(crate::sound::TAN00_LOUD);
        if let Some(st) = &self.stage {
            st.borrow_mut().end_spell_bg();
        }
        if !s.timed_out {
            self.cancel_and_kill_bonus(anm);
        }
        let mut p = self.player.borrow_mut();
        // Always: bomb cooldown +0xE2A6C := 16, its only writer (07 §11.1, 09 §1.1).
        p.bomb_cooldown = 16;
        if p.state == 0 {
            p.state = 3;
            p.state_timer.set(70);
        }
        use crate::gamemgr::{F_LAST_CAPTURED, F_LAST_SPELL, flags, set_flags};
        if !s.capture || p.deaths != s.deaths0 || p.bombs_started != s.bombs0 {
            set_flags(flags() & !(F_LAST_SPELL | F_LAST_CAPTURED));
            // Not captured: the start ring is freed, +0xF4 := 0 (07 §11.1).
            crate::effect::free(crate::effect::FIX_SPELL_INDEX);
            p.stats
                .popups
                .push(crate::player::Popup::Message(5 + s.last as u8, 0));
            return;
        }
        if flags() & (F_LAST_SPELL | F_LAST_CAPTURED) != 0 {
            set_flags(flags() & !F_LAST_SPELL | F_LAST_CAPTURED);
        }
        // Spell flag bit 9 "captured" is set by `Spell_End` (07 §11.1).
        self.g.spell_captured = true;
        crate::gui::request(crate::gui::Req::Captured {
            card: s.id,
            bonus: s.bonus,
            comment: s.comment.clone(),
        });
        // Time reward 700 (flag A bit 27) or from the remaining card time (07 §11.1 → answers_phase5 impl8).
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
        crate::sound::play(crate::sound::CARDGET);
        p.stats.spells += 1;
        // Spell capture checksum (11 §1.5, 4 draws).
        crate::player::Stats::checksum(&mut anm.rng);
        // Capture ring restyle (07 §11.1): radius → 256 over 30 (mode 6), colour slot → 0x20D080A0 over 60 (mode 3),
        // waves 6, age timer +0x338 := 0; the start ring becomes the capture ring (+0xF4 → +0xF8).
        crate::effect::get(crate::effect::FIX_SPELL_INDEX, |e| {
            let p = [e.ring.radius, e.ring.amp, 0.0];
            e.vm.interp_pos_from(30, 6, p, [256.0, 0.0, 0.0]);
            e.vm.pos = p;
            let c = u32::from_le_bytes(e.vm.color);
            e.vm.interp_color_from(60, 3, c, 0x20D0_80A0);
            // The colour slot lerps RGB only; 0x20 goes into alpha_end, alpha_start := current alpha (07 §11.1, 05 §6).
            e.vm.color_start[crate::anmvm::A] = e.vm.color[crate::anmvm::A];
            e.vm.color_end[crate::anmvm::A] = 0x20;
            e.ring.waves = 6.0;
            e.age.set(0);
        });
        self.g.payout = Some(Payout {
            bonus: s.bonus,
            time,
        });
    }

    // Payout (07 §11.1): at ring timer 130 (`ring`: ring freed, max point += left·10) or at the next Spell_Start.
    pub fn pay_capture(&mut self, anm: &mut AnmManager, ring: bool) {
        let Some(c) = self.g.payout.take() else {
            return;
        };
        if ring {
            crate::effect::free(crate::effect::FIX_SPELL_INDEX);
        }
        let st = &mut self.player.borrow_mut().stats;
        st.popups.push(crate::player::Popup::SpellBonus(c.bonus));
        st.add_score(c.bonus);
        if c.time > 0 {
            st.add_time(c.time, &mut anm.rng);
            if ring {
                st.max_point += c.time * 10;
            }
        }
    }

    // Spell_AddBonus 0x416B10 (07 §11): ignored while bit 11; below the max (unsigned) R += n/120, else bonus := max.
    pub fn spell_add_bonus(&mut self, n: i32) {
        if self.g.bonus_frozen {
            return;
        }
        if let Some(s) = &mut self.g.spell {
            s.bonus = s.bonus.wrapping_add(n);
            if (s.bonus as u32) < s.max as u32 {
                s.decay += n / 120;
            } else {
                s.bonus = s.max;
            }
        }
    }

    // Card-active part of Spell_Calc 0x416B90 (07 §11.1 steps 1.1 and 1.3–1.7); the caller skips it while bit 10 /
    // the freeze byte is set and does step 1.2 (bonus-line tint). Returns true when step 1.1 ended the card.
    pub fn spell_step(&mut self, anm: &AnmManager) -> bool {
        let Some(mut s) = self.g.spell.take() else {
            return false;
        };
        let boss = &self.enemies[s.boss];
        // Step 1.1: the card ends silently (no Spell_End, no capture); steps 1.2–1.7 still run with bit 0 clear.
        let active = boss.alive() && boss.slot == s.slot;
        if !active {
            crate::gui::request(crate::gui::Req::BossHide);
        }
        let ring = crate::effect::FIX_SPELL_INDEX;
        if !s.capture {
            // Failed-ring look: sprite 0xDD → 0xDE, scale_end.x and scale.x := 4.
            crate::effect::get(ring, |e| {
                if e.vm.sprite_index == 0xDD {
                    e.vm.set_sprite(anm, 0xDE);
                    e.vm.scale_end[0] = 4.0;
                    e.vm.scale[0] = 4.0;
                }
            });
        } else if !self.g.bonus_frozen && boss.flags & A_FIXED_BONUS == 0 {
            let d = ((s.decay as u32 / 60) as f32 * anm.speed) as u32;
            let b = (s.bonus as u32).wrapping_sub(d);
            s.bonus = (b - b % 10) as i32;
        }
        let duration = s.duration;
        let follow = (!self.g.spell_flag6)
            .then(|| -> [f32; 3] { std::array::from_fn(|k| boss.pos[k] + boss.pos_offset[k]) });
        let d = if active && s.last {
            -0.031415928
        } else {
            0.015707964
        };
        crate::effect::get(ring, |e| {
            // Step 1.4: wavy start ring, amplitude +0x32C := VM pos.y until it reaches 0, then plain (+0x334 := 0).
            if e.ring.waves != 0.0 {
                e.ring.amp = e.vm.pos[1];
                if e.ring.amp == 0.0 {
                    e.ring.waves = 0.0;
                }
            }
            // Step 1.5: after the 100-frame grow-in, shrink 256 → 8 (linear) over the rest of the card time.
            if e.vm.interp_duration[0].cur == 0 {
                e.vm.interp_pos_from(duration - 100, 0, [256.0, 0.0, 0.0], [8.0, 0.0, 0.0]);
            }
            // Step 1.6: the ring follows the boss unless spell bit 6.
            if let Some(target) = follow {
                for k in 0..2 {
                    e.centre[k] += (target[k] - e.centre[k]) / 16.0;
                }
                e.centre[2] = 0.0;
            }
            // Step 1.7: −π/100 per frame for a last spell (bit 0 and bit 5), else +π/200.
            e.ring.start = anmvm::wrap_add(e.ring.start, d);
        });
        if active {
            self.g.spell = Some(s);
        }
        !active
    }

    // Capture-ring part of Spell_Calc 0x416B90 (07 §11.1); the caller skips it while bit 10 / the freeze byte is set.
    pub fn capture_ring_step(&mut self, anm: &mut AnmManager) {
        if self.g.spell.is_some() || self.g.payout.is_none() {
            return;
        }
        let ring = crate::effect::FIX_SPELL_INDEX;
        let t = crate::effect::peek(ring, |e| e.age.cur);
        // Only one of t == 30 / 60 / 130 runs per frame; at 130 the ring is freed and the rest is skipped.
        match t {
            30 => crate::effect::get(ring, |e| {
                let p = [e.ring.radius, e.ring.amp, 0.0];
                e.vm.interp_pos_from(100, 4, p, [0.0, 60.0, 0.0]);
                e.vm.pos = p;
                let (s, end) = (e.vm.scale_start, e.vm.scale_end);
                e.vm.interp_scale_from(20, 1, [e.ring.width, s[1]], [64.0, end[1]]);
            }),
            60 => crate::effect::get(ring, |e| {
                let (s, end) = (e.vm.scale_start, e.vm.scale_end);
                e.vm.interp_scale_from(70, 1, [e.ring.width, s[1]], [0.0, end[1]]);
            }),
            130 => {
                self.pay_capture(anm, true);
                return;
            }
            _ => {}
        }
        let centre = crate::effect::peek(ring, |e| e.centre);
        let pp = self.player.borrow().pos;
        let div = if t > 80 { 4.0 } else { 16.0 };
        let centre = [
            centre[0] + (pp[0] - centre[0]) / div,
            centre[1] + (pp[1] - centre[1]) / div,
            0.0,
        ];
        crate::effect::get(ring, |e| {
            e.centre = centre;
            let d = if t > 80 { -0.05235988 } else { -0.015707964 };
            e.ring.start = anmvm::wrap_add(e.ring.start, d);
            // +0x32C := VM pos.y every frame (no +0x334 test here).
            e.ring.amp = e.vm.pos[1];
        });
        let left = self.g.payout.map_or(0, |c| c.time);
        if t > 8 && left > 0 {
            // t = Timer_GetF; f32 per operation, left to right (07 §11.1, main-thread CW 0x007F).
            use std::f32::consts::PI;
            let tf = crate::effect::peek(ring, |e| e.age.getf());
            let a = anmvm::wrap_add((tf - 10.0) * (2.0 * PI) / 40.0 - PI / 2.0, 0.0);
            let mut left = left;
            for (ang, n) in [(a, None), (anmvm::wrap_add(a + PI, 0.0), Some(6))] {
                let k = left.min(7);
                // Quirk kept (07 §11.1): the second burst always spawns 6, whatever k is.
                let spawns = n.unwrap_or(k);
                let d = crate::bullet::polar(ang, 128.0);
                let pos = [centre[0] + d[0], centre[1] + d[1], centre[2]];
                for _ in 0..spawns {
                    self.spawn_item(anm, pos, 10, 0);
                }
                left -= k;
            }
            if let Some(c) = &mut self.g.payout {
                c.time = left.max(0);
            }
        }
    }

    pub fn dialogue_active(&self) -> bool {
        self.msg.as_ref().is_some_and(|m| m.borrow().active())
    }

    pub fn spawn_item(&mut self, anm: &mut AnmManager, pos: [f32; 3], kind: i32, mode: u8) {
        let p = self.player.borrow();
        self.bullets
            .borrow_mut()
            .items
            .spawn(anm, &p, pos, kind, mode);
    }

    pub fn add_score(&mut self, v: i32) {
        self.player.borrow_mut().stats.add_score(v);
    }

    pub fn cancel_all(&mut self, anm: &mut AnmManager, mode: i32) {
        let mut p = self.player.borrow_mut();
        self.bullets.borrow_mut().cancel_all(anm, &mut p, mode);
    }

    // Enemy_DropItems 0x42BEA0 (07 §10.4): mode 1 (auto-collected) when hitByBomb; extras always mode 0.
    // RNG order: sparks (effect), then extras x then y per item, then the items themselves.
    fn drop_items(&mut self, idx: usize, anm: &mut AnmManager, mode: Option<u8>) {
        let e = &mut self.enemies[idx];
        let p = e.final_pos;
        let mut items = Vec::new();
        let mode = mode.unwrap_or(if e.bomb_hit { 1 } else { 0 });
        let mut sparks = 0;
        if e.item >= 0 {
            items.push((p, e.item, mode));
            sparks = 3;
        } else if e.item == -1 {
            // Check before the increment; the counter advances on every item −1 death.
            if self.g.drop_counter.is_multiple_of(3) {
                items.push((p, DROP_TABLE[self.g.drop_index as usize % 32], mode));
                self.g.drop_index = (self.g.drop_index + 1) % 32;
                sparks = 6;
            }
            self.g.drop_counter = self.g.drop_counter.wrapping_add(1);
        }
        if sparks > 0 {
            let spark = self.enemies[idx].death_bytes[1].wrapping_add(4);
            crate::effect::spawn(anm, spark, p, sparks, 0xFFFFFFFF);
        }
        let e = &mut self.enemies[idx];
        let power_kind = if self.player.borrow().stats.power < 128.0 {
            0
        } else {
            1
        };
        for (n, kind) in [(e.extra_power, power_kind), (e.extra_point, 1)] {
            for _ in 0..n.max(0) {
                let dx = anm.rng.next_f32() * 128.0 - 64.0;
                let dy = anm.rng.next_f32() * 128.0 - 64.0;
                items.push(([p[0] + dx, p[1] + dy, p[2]], kind, 0));
            }
        }
        e.extra_point = 0;
        e.extra_power = 0;
        for (pos, kind, mode) in items {
            self.spawn_item(anm, pos, kind, mode);
        }
    }

    // Threshold / timeout switch cleanup (07 §10.2).
    fn cleanup_after_jump(&mut self, anm: &mut AnmManager, idx: usize, reward: bool) {
        let e = &mut self.enemies[idx];
        e.reset_for_jump();
        e.flags_b &= !B_WARN_MASK;
        self.kill_chain(idx, if reward { Some(&mut *anm) } else { None });
        self.kill_non_bosses();
        if self.enemies[idx].flags & A_BOSS != 0 {
            self.protect_player(70);
        }
    }

    // Every alive non-boss enemy: life 0 and an `Ecl_SetSub` jump to its death sub, consumed (07 §10.2, §10.6:
    // ip/clock/wait only). No self test: a non-boss switching enemy also dies, its new sub replaced by the death sub.
    fn kill_non_bosses(&mut self) {
        for j in 0..self.enemies.len() {
            let e = &mut self.enemies[j];
            if !e.alive() || e.flags & A_BOSS != 0 {
                continue;
            }
            e.life = 0;
            if e.death_sub >= 0 {
                let sub = std::mem::replace(&mut e.death_sub, -1);
                self.jump_main(j, sub);
            }
        }
    }

    // Player state 0 → 3 (invulnerable) for `t` frames (07 §10.2, §10.3, §11.1).
    fn protect_player(&mut self, t: i32) {
        let mut p = self.player.borrow_mut();
        if p.state == 0 {
            p.state = 3;
            p.state_timer.set(t);
        }
    }

    // Low-life warning level 0–3 (07 §10.2); `diff` = life − threshold, or life without thresholds.
    fn warn_level(&self, e: &Enemy, diff: i32, with_thr: bool) -> u32 {
        let card = self.g.spell.is_some();
        let t: [i32; 3] = match (with_thr, card, e.flags & A_BOSS != 0) {
            (true, false, _) => [500, 1500, 2200],
            (true, true, _) => [120, 200, 300],
            (false, false, true) => [600, 1600, 2400],
            (false, true, true) => [120, 300, 400],
            (false, false, false) => [50, i32::MIN, i32::MIN],
            (false, true, false) => [10, i32::MIN, i32::MIN],
        };
        t.iter().position(|&v| diff < v).map_or(0, |k| 3 - k as u32)
    }

    // Enemy_CheckLifeThresholds 0x42B490 (07 §10.2): first k with life < threshold[k] wins; max life unchanged.
    fn check_thresholds(&mut self, anm: &mut AnmManager, idx: usize) -> bool {
        self.enemies[idx].flags_b &= !B_WARN_MASK;
        let e = &self.enemies[idx];
        let mut level = 0;
        let mut hit = None;
        for k in 0..4 {
            let t = e.thresholds[k];
            if t < 0 {
                continue;
            }
            if e.life < t {
                hit = Some(k);
                break;
            }
            level = level.max(self.warn_level(e, e.life - t, true));
        }
        let Some(k) = hit else {
            if e.thresholds.iter().all(|&t| t < 0) {
                level = level.max(self.warn_level(e, e.life, false));
            }
            self.enemies[idx].flags_b |= level << B_WARN_SHIFT;
            return false;
        };
        let sub = e.threshold_subs[k];
        let e = &mut self.enemies[idx];
        e.life = e.thresholds[k];
        e.hud_life = e.life;
        e.thresholds[k] = -1;
        e.bonus_seconds = (e.timeout - e.boss_timer.cur) / 60;
        e.timeout = -1;
        self.jump_main(idx, sub);
        self.cleanup_after_jump(anm, idx, true);
        true
    }

    // Enemy_CheckTimeout 0x42B930 (07 §10.2): the largest active threshold becomes life; timeout sub := death sub.
    fn check_timeout(&mut self, anm: &mut AnmManager, idx: usize) -> bool {
        let e = &mut self.enemies[idx];
        // Every call on a slot-0 boss, before the fire test and before this frame's step-7 tick (07 §10.2).
        if e.flags & A_BOSS != 0 && e.boss_slot == 0 {
            self.g.hud_timer = (e.timeout - e.boss_timer.cur) / 60;
        }
        if e.timeout < 0 || e.boss_timer.cur < e.timeout {
            return false;
        }
        e.bonus_seconds = 0;
        if let Some(k) = (0..4)
            .filter(|&k| e.thresholds[k] > 0)
            .max_by_key(|&k| e.thresholds[k])
        {
            e.life = e.thresholds[k];
            e.hud_life = e.life;
            e.thresholds[k] = -1;
        }
        let sub = e.timeout_sub;
        e.timeout = -1;
        e.timeout_sub = e.death_sub;
        e.boss_timer.set(0);
        let fixed = e.flags & A_FIXED_BONUS != 0;
        self.jump_main(idx, sub);
        // 0x42BC50: the card stays active, its own op 123 ends it.
        if !fixed {
            if let Some(sp) = &mut self.g.spell {
                sp.timed_out = true;
                sp.capture = false;
                sp.bonus = 0;
            }
            self.cancel_all(anm, 4);
        }
        self.cleanup_after_jump(anm, idx, false);
        true
    }

    // Shot damage (07 §10.1 step 6); `d` comes from the player side. Score is added before the bit-3 test.
    pub fn damage(&mut self, idx: usize, d: i32, bomb: bool) {
        if d <= 0 {
            return;
        }
        let d = d.min(70);
        self.add_score((d / 5) * 10);
        let spell = self.g.spell.is_some();
        let bombed = self.g.spell.as_ref().is_some_and(|s| s.bombed);
        let e = &mut self.enemies[idx];
        // The hit flash needs only d > 0 at step 6, also without bit 3 (07 §10.1 step 7).
        e.hit = true;
        if e.flags & A_LOSES_LIFE == 0 {
            return;
        }
        let child = e.root.is_some();
        // Bomb damage during a card only after a bomb started in it (spell flag bit 7), never to chained children.
        let mut d = match (spell, bomb) {
            (true, true) if child || !bombed => 0,
            (true, true) => {
                if d < 3 {
                    d.min(1)
                } else {
                    (d as f32 / 2.5) as i32
                }
            }
            (true, false) => {
                if d < 8 {
                    d.min(1)
                } else {
                    d / 7
                }
            }
            _ => d,
        };
        if e.dmg_timer.cur > 0 {
            d = if e.flags & A_BOSS != 0 { d / 9 } else { 0 };
        }
        e.life -= d;
        e.last_damage = d;
        // 0x42B370: chained damage to the root; skipped while bombing; the root's damage timer cuts trunc(d/2)/9.
        if let Some(r) = e.root {
            if self.player.borrow().bombing {
                return;
            }
            let root = &mut self.enemies[r];
            let mut rd = d / 2;
            if root.dmg_timer.cur > 0 {
                rd = if root.flags & A_BOSS != 0 { rd / 9 } else { 0 };
            }
            // Unused −1 thresholds count as 0.
            let floor = root.thresholds.iter().fold(0, |m, &t| m.max(t));
            if rd != 0 {
                root.life -= rd;
                if root.life <= floor {
                    root.life = floor;
                }
            }
        }
    }

    // Enemy-owned effects: op 128 effects fade out; the op 174 aura gets interrupt 3 and plays its own end, it is
    // not freed (07 §10.3b).
    fn release_effects(&mut self, idx: usize) {
        for i in self.enemies[idx].fx_attached.drain(..) {
            crate::effect::get(i, |x| x.fade = true);
        }
        self.end_aura(idx);
    }

    // Death processing (07 §10.3): runs after this frame's Ecl_Run, so the death sub starts next frame.
    fn death(&mut self, idx: usize, anm: &mut AnmManager) {
        let e = &self.enemies[idx];
        if e.life >= 1 || e.flags_b & (B_DEATH_LATCH | B_CANNOT_DIE) != 0 || !e.alive() {
            return;
        }
        let e = &mut self.enemies[idx];
        e.flags_b |= B_DEATH_LATCH;
        e.bonus_seconds = (e.timeout - e.boss_timer.cur) / 60;
        e.timeout = -1;
        e.thresholds = [-1; 4];
        e.threads = Default::default();
        if let Some(r) = self.enemies[idx].root {
            self.enemies[r].children -= 1;
        }
        self.kill_chain(idx, Some(&mut *anm));
        {
            let mut p = self.player.borrow_mut();
            // Non-forced GaugeAdd, skipped while bombing; sign by player byte +3, not the +5 mode byte.
            if !p.bombing {
                let n = if p.focus == 0 { -200 } else { 200 };
                p.stats.gauge_add(n);
            }
        }
        let e = &mut self.enemies[idx];
        let mode = (e.flags >> A_DEATH_SHIFT) & 7;
        let boss = e.flags & A_BOSS != 0;
        match mode {
            0 => {
                let sc = e.score;
                e.flags &= !A_ALIVE;
                self.add_score(sc);
                self.end_aura(idx);
            }
            1 => {
                let sc = e.score;
                e.flags |= A_DEAD_CONT;
                e.flags &= !(A_CONTACT | A_LOSES_LIFE | A_SHOT_HIT);
                self.add_score(sc);
            }
            // Latch B bit 3 stays until life > 0 again; HUD boss display off for every mode-3 enemy (07 §10.3).
            3 => {
                e.life = 1;
                e.flags &= !(A_LOSES_LIFE | 7 << A_DEATH_SHIFT | 3 << 30);
                self.g.hud_boss = false;
                let db = e.death_bytes[0];
                let pos = e.final_pos;
                if db < 0x80 {
                    crate::effect::spawn(anm, db, pos, 3, 0xFFFFFFFF);
                }
                self.end_aura(idx);
                self.protect_player(90);
            }
            _ => {}
        }
        if mode <= 1 && boss {
            self.g.hud_boss = false;
            for i in self.enemies[idx].fx_attached.drain(..) {
                crate::effect::get(i, |x| x.fade = true);
            }
        }
        let (pos, db) = (self.enemies[idx].final_pos, self.enemies[idx].death_bytes);
        if mode <= 2 {
            self.drop_items(idx, anm, None);
            if boss && self.g.spell.is_none() {
                self.cancel_and_kill_bonus(anm);
            }
            self.enemies[idx].life = 0;
        }
        // Every death mode, also 3 and 4–7 (07 §10.3). RNG order: drops → death sparks ×4 → time orb.
        if self.enemies[idx].flags & A_CHAIN_KILLED == 0 {
            crate::sound::play_at(
                if idx % 2 == 0 {
                    crate::sound::ENEP0
                } else {
                    crate::sound::ENEP1
                },
                pos[0],
            );
            // Both spawns only when the signed byte e+0x3310 ≥ 0.
            if db[0] < 0x80 {
                crate::effect::spawn(anm, db[0], pos, 1, 0xFFFFFFFF);
                crate::effect::spawn(anm, db[1].wrapping_add(4), pos, 4, 0xFFFFFFFF);
            }
            let max = {
                let p = self.player.borrow();
                p.stats.human_max() || p.stats.youkai_max()
            };
            if max {
                self.spawn_item(anm, pos, crate::item::TIME_ORB as i32, 1);
            }
        }
        if mode == 0 {
            self.release(idx);
            return;
        }
        self.jump_to_death_sub(idx);
    }

    // Aura told to end (interrupt 3).
    fn end_aura(&mut self, idx: usize) {
        let h = std::mem::replace(&mut self.enemies[idx].fx_aura, crate::effect::NONE);
        crate::effect::get(h, |x| x.vm.pending_interrupt = 3);
    }

    // 07 §10.1 step 5: a finished sub VM is disabled.
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

    // Enemy_ReactPlayerMode 0x42C420 (07 §10.1 step 3), using the stored bit 11 at entry.
    fn react_player_mode(&mut self, idx: usize, anm: &mut AnmManager) {
        let youkai = self.g.player_mode != 0;
        let e = &mut self.enemies[idx];
        let p = e.final_pos;
        if e.flags & A_YOUKAI == 0 {
            if youkai {
                crate::effect::spawn(anm, 0x1F, p, 1, 0x80303080);
                crate::effect::get(e.fx_aura, |x| x.vm.pending_interrupt = 2);
                crate::sound::play_at(crate::sound::OPHIDE, p[0]);
                e.layer = 0;
            }
            // Effect 38 (10 RNG draws) when the timer "changed and even", also on the switching frame.
            let t = e.boss_timer;
            if e.flags_b & 1 << 1 != 0 && t.cur != t.prev && t.cur % 2 == 0 {
                crate::effect::spawn(anm, 38, p, 1, 0xFFFFFFFF);
            }
        } else if !youkai {
            crate::effect::spawn(anm, 0x1E, p, 1, 0x80803030);
            crate::effect::get(e.fx_aura, |x| x.vm.pending_interrupt = 1);
            crate::sound::play_at(crate::sound::OPSHOW, p[0]);
            e.layer = 2;
        }
        e.mode_bits = if youkai { 0x40 } else { 0x20 };
        e.set_flag(A_YOUKAI, youkai);
    }

    // EnemyMgr_Calc per enemy (07 §10.1).
    fn update(&mut self, idx: usize, anm: &mut AnmManager) {
        if self.enemies[idx].flags & A_CHAIN_KILLED != 0 {
            let e = &mut self.enemies[idx];
            e.life = 0;
            e.final_pos = [e.pos[0] + e.pos_offset[0], e.pos[1] + e.pos_offset[1], 0.0];
            self.death(idx, anm);
            return;
        }
        // Held (07 §10.1 step 2): flags B bit 7, or flags A bit 30 while bombing / player state ≠ 0. Steps 3–6 and
        // the death test are skipped (no movement, never freed off screen); the e+0x2E14 decrement nets the tick.
        let held = {
            let p = self.player.borrow();
            let e = &self.enemies[idx];
            e.flags_b & B_PAUSED != 0
                || (e.flags & A_LAST_SPELL != 0 && (p.bombing || p.state != 0))
        };
        if held {
            self.enemies[idx].boss_timer.dec(anm.speed);
        } else if !self.update_body(idx, anm) {
            return;
        }
        // Death latch cleared when life > 0, only on frames the script block ran (07 §10.3).
        if !held {
            let e = &mut self.enemies[idx];
            if e.life > 0 {
                e.flags_b &= !B_DEATH_LATCH;
            }
            self.death(idx, anm);
        }
        // Step 7 after step 6's death: the death bonus seconds read the un-ticked timer (07 §10.1, §10.3).
        // Game speed read live (07 §5): a native 18 run by this frame's Ecl_Run already slows these ticks.
        self.update_tail(idx, anm.speed);
    }

    // 07 §10.1 steps 3–6; false when the enemy was freed (op 1 delete, off screen).
    fn update_body(&mut self, idx: usize, anm: &mut AnmManager) -> bool {
        if self.enemies[idx].flags & A_MODE_REACT != 0 {
            self.react_player_mode(idx, anm);
        }
        // 07 §10.1 step 4: a threshold or timeout switch re-runs the whole body in the same frame.
        loop {
            if crate::eclvm::ecl_run(self, anm, idx) == -1 {
                self.free(idx);
                return false;
            }
            // Live game speed (07 §5): set by this pass's Ecl_Run.
            self.move_enemy(idx, anm.speed);
            // Aura VM pos (07 §10.1 step 4, 0x42CAC9): every pass, before the trail shift and the screen check.
            let e = &self.enemies[idx];
            let p = e.final_pos;
            crate::effect::get(e.fx_aura, |x| x.pos = p);
            if self.enemies[idx].trail[0] != 0 {
                self.enemies[idx].push_trail();
            }
            if self.screen_check(idx) {
                self.free(idx);
                return false;
            }
            if !(self.check_thresholds(anm, idx) || self.check_timeout(anm, idx)) {
                break;
            }
        }
        self.run_vms(idx, anm);
        // Step 6 is skipped for flag A bits 4/5/11, and for bit 31 while the player is bombing (07 §10.1 step 6).
        let flags = self.enemies[idx].flags;
        if flags & (A_HIDDEN | 1 << 5 | A_YOUKAI) == 0
            && !(flags & 1 << 31 != 0 && self.player.borrow().bombing)
        {
            self.player_collision(idx, anm);
        }
        // Debug: `TH08_ENEMY_DMG` damage per frame.
        if self.debug_damage > 0
            && self.enemies[idx].flags & (A_SHOT_HIT | A_HIDDEN) == A_SHOT_HIT
            && self.enemies[idx].flags & A_ON_SCREEN != 0
        {
            self.damage(idx, self.debug_damage, false);
        }
        true
    }

    // Common tail of 07 §10.1 step 7: hit flash, boss marker, timers, draw-list link.
    fn update_tail(&mut self, idx: usize, gs: f32) {
        let pause = timer_pause() != 0;
        let dialogue = self.dialogue_active();
        let e = &mut self.enemies[idx];
        // Counter e+0x3314 (07 §10.1 step 7): a flashed frame is followed by one without.
        let hit = std::mem::take(&mut e.hit);
        if e.flags & A_YOUKAI != 0 {
            // Youkai state skips the hit flash entirely (no counter change, no sfx); BGRA (0x20, 0x20, 0xC0, a / 2).
            e.vm.flags |= anmvm::F_COLOR2;
            e.vm.color2 = [0xC0, 0x20, 0x20, e.vm.color[anmvm::A] / 2];
        } else if hit && !e.flash {
            let low = (e.flags_b >> 4) & 3 >= 2;
            crate::sound::play_at(
                if low {
                    crate::sound::DAMAGE1
                } else {
                    crate::sound::DAMAGE0
                },
                e.final_pos[0],
            );
            e.vm.flags |= anmvm::F_COLOR2;
            // BGRA: R 0xFF, G 0x60, B 0x80, current alpha.
            e.vm.color2 = [0x80, 0x60, 0xFF, e.vm.color[anmvm::A]];
            e.flash = true;
        } else {
            // Both the counter-decrement and the idle branch turn the flag off.
            e.flash = false;
            e.vm.flags &= !anmvm::F_COLOR2;
        }
        // Boss HUD target +0x30 (12 §5.1): slot-0 boss, no dialogue; the enemy's own max life e+0x2E00.
        if e.flags & A_BOSS != 0 && e.boss_slot == 0 && !dialogue {
            self.g.hud_bar_target = e.life as f32 / e.max_life as f32;
        }
        // ENEMY marker (12 §5.10): x + 32 (−999 while not drawn); mode = warning level + 1, else 1 on a hit flash.
        if e.flags & A_BOSS != 0 && (0..4).contains(&e.boss_slot) {
            let x = if e.flags & A_HIDDEN != 0 {
                -999.0
            } else {
                e.final_pos[0] + 32.0
            };
            let level = ((e.flags_b >> 4) & 3) as u8;
            let mode = if level != 0 {
                level + 1
            } else {
                (e.vm.flags & anmvm::F_COLOR2 != 0) as u8
            };
            crate::ascii::marker_track(e.boss_slot as usize, x, mode);
        }
        // Enemy_UpdateOrbitEffects 0x42E010 (07 §10.1 step 7): after the marker; a boss mode-0/1 death has
        // already drained them. 13 §2.6b: centred on the base pos e+0x2D34, not the final pos.
        // A held enemy runs it too (unconditional call in the shared tail, 07 §10.1 step 2).
        let (c, d, hidden) = (e.pos, e.orbit_target, e.flags & A_HIDDEN != 0);
        for &i in &e.fx_attached {
            crate::effect::get(i, |x| x.orbit_follow(c, d, hidden));
        }
        if !pause {
            e.boss_timer.tick(gs);
        }
        if e.dmg_timer.cur > 0 {
            e.dmg_timer.dec(gs);
        }
        // Linked only if still alive: a mode-0 death this frame skips the link (07 §10.1 step 7).
        if e.alive() && e.flags & A_HIDDEN == 0 {
            self.draw_lists[e.layer.min(3) as usize].push(idx);
        }
    }

    // EnemyMgr_Calc 0x42C660 (07 §10.1).
    pub fn step(&mut self, anm: &mut AnmManager) {
        {
            let p = self.player.borrow();
            self.g.player = p.pos;
            self.g.player_mode = p.mode as i32;
        }
        self.dissolve_requests();
        let orbs = std::mem::take(&mut self.player.borrow_mut().stats.spell_bonus_orbs);
        let (deaths, bombs) = {
            let p = self.player.borrow();
            (p.deaths, p.bombs_started)
        };
        // Port: spell flag bits 2/7 from the player's bomb/death counters (07 §11.1: 0x44CBA0, 0x44D150).
        // Port: the orbs' Spell_AddBonus calls are applied here, before anything reads the bonus.
        for _ in 0..orbs {
            self.spell_add_bonus(8000);
        }
        if let Some(s) = &mut self.g.spell {
            if bombs != s.bombs0 && !s.bombed {
                s.bombed = true;
                s.capture = false;
                s.bonus = 0;
            }
            if deaths != s.deaths0 {
                s.capture = false;
            }
        }
        // 0x164D30C play-time counter: top of EnemyMgr_Calc outside dialogue, before the bit-10 test (11 §2).
        // Human-rate counters (15 §2.3) in the same block, only while the manager clock (+0x9DCED0) is > 15;
        // human = player byte +3 == 0.
        if !self.dialogue_active() {
            let mut p = self.player.borrow_mut();
            p.stats.rec.progress += 1;
            if self.clock.cur > 15 {
                let human = (p.focus == 0) as u32;
                p.stats.rec.total += 1;
                p.stats.rec.human += human;
            }
        }
        // Step 0: GameFlags bit 10 (spell-card death freeze) returns here; the draw lists keep last frame's
        // enemies, the manager clock and Spell_Calc's card timer hold (07 §10.1 step 0, §11.1).
        if self.player.borrow().freeze_bullets() {
            return;
        }
        // Step 1: the op-176 box, then EnemyMgr_TimeRank, then the resets and the timelines (07 §10.1 step 1).
        self.last_spell_box(anm);
        self.time_rank();
        self.full = false;
        self.draw_lists.iter_mut().for_each(Vec::clear);
        self.run_timelines(anm);
        // Slot order; an inactive slot clears the aimed pointer (07 §10.1 step 7, 14 §3.1).
        for idx in 0..self.enemies.len() {
            if self.enemies[idx].alive() {
                self.update(idx, anm);
            } else if self.player.borrow().aimed == Some(idx) {
                self.player.borrow_mut().aimed = None;
            }
        }
        {
            let mut p = self.player.borrow_mut();
            if let Some(a) = p.aimed {
                p.aimed_pos = self.enemies[a].final_pos;
                p.aimed_base = self.enemies[a].pos;
            }
        }
        let pull = self.bosses[0]
            .map(|i| &self.enemies[i])
            .filter(|e| e.flags & A_LOSES_LIFE != 0)
            .map(|e| e.final_pos);
        crate::effect::with(|fx| fx.boss = pull);
        let mut b = self.bullets.borrow_mut();
        b.env.boss = self.bosses.iter().any(Option::is_some);
        b.env.spell = self.g.spell.is_some();
        b.env.dialogue = self.dialogue_active();
        let env = b.env;
        drop(b);
        self.player.borrow_mut().env = env;
        // Timer_Tick at the end of EnemyMgr_Calc with the live game speed (11 §3.2).
        self.clock.tick(anm.speed);
    }

    // EnemyMgr_TimeRank 0x42C3B0 (11 §3.2): rank +100 outside dialogue when the manager clock (before this frame's
    // tick) advanced and cur % (2400 − 240·lives) == 0, signed.
    fn time_rank(&mut self) {
        if self.dialogue_active() {
            return;
        }
        let lives = self.player.borrow().stats.lives as i32;
        // Port-only guard: lives ≤ 8 keeps the period positive.
        let period = (2400 - 240 * lives).max(1);
        let c = self.clock;
        if c.cur != c.prev && c.cur % period == 0 {
            self.player.borrow_mut().stats.rank(100);
        }
    }

    // After-image trail (07 §10.6, op 157), drawn before the main VM; the main VM's scale and colour are restored.
    fn draw_trail(anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, e: &mut Enemy) {
        let (f, n) = (e.trail[0], e.trail_n() as i32);
        // Port-only bound: a step b ≤ 0 would never end the walk (data: b ≥ 1).
        let b = e.trail[3].max(1);
        let ring = e.trail_ring;
        let used = |i: i32| ring[i as usize].pos[0] >= -990.0;
        let (scale, color) = (e.vm.scale, e.vm.color);
        let a = color[anmvm::A] as i32;
        if f & 8 == 0 {
            // Sprite copies, oldest first; entry 0 is not drawn here.
            let mut i = n - 1;
            while i > 0 {
                if used(i) {
                    let t = ring[i as usize];
                    let vm = &mut e.vm;
                    if e.flags & (1 << 25) != 0 && vm.rotation[2] != t.angle {
                        vm.rotation[2] = t.angle;
                        vm.flags |= anmvm::F_ROT_DIRTY;
                    }
                    if f & 2 != 0 {
                        vm.scale[0] = scale[0] - i as f32 * scale[0] / n as f32;
                    }
                    if f & 4 != 0 {
                        vm.color[anmvm::A] = (a - a * i / n) as u8;
                    }
                    let saved_off = vm.pos_offset;
                    vm.pos = [
                        t.pos[0] + saved_off[0] + 32.0,
                        t.pos[1] + saved_off[1] + 16.0,
                        0.3,
                    ];
                    vm.pos_offset = [0.0; 3];
                    anm.draw_mode_z(gfx, vm, 2, 0.3);
                    vm.pos_offset = saved_off;
                }
                i -= b;
            }
        } else if let Some(&spr) = anm.sprite(e.vm.anm_slot as usize, e.vm.sprite_index as usize) {
            // One triangle strip (Anm_DrawRingStrip); unreachable with retail data (07 §10.6).
            let mut idx = Vec::new();
            let mut i = 0;
            while i < n && used(i) {
                idx.push(i);
                i += b;
            }
            let vm = &e.vm;
            let [u0, _, u1, _] = spr.uv;
            let step = (u1 - u0) / (idx.len() as f32 - 1.0).max(1.0);
            let (va, vb) = (spr.uv[1] + vm.uv_off[1], spr.uv[3] + vm.uv_off[1]);
            let theta = |i: i32| {
                let at = |k: i32| ring[k as usize].angle;
                if i == 0 {
                    at(0)
                } else {
                    trail_mid(at(i - 1), at(i))
                }
            };
            let mut v = Vec::new();
            let mut prev = None;
            for (j, &i) in idx.iter().enumerate() {
                let th = theta(i);
                // Flag 2: an interior entry on a straight run is skipped (QUIRK: m uses the fixed index b).
                if f & 2 != 0 && i > 0 && i + b < n {
                    let m = trail_mid(ring[(i + b - 1) as usize].angle, ring[b as usize].angle);
                    if prev.is_some_and(|p: f32| (p - th).abs() < 1e-5) && (th - m).abs() < 1e-5 {
                        continue;
                    }
                }
                prev = Some(th);
                // Coordinate A steps per walked entry, skipped ones included (07 §10.6).
                let ua = u1 + vm.uv_off[0] - step * j as f32;
                let mut h = vm.scale[1] * vm.sprite_h / 2.0;
                if f & 2 != 0 {
                    h *= 1.0 - i as f32 / n as f32;
                }
                let al = if f & 4 != 0 { a - a * i / n } else { a } as u8;
                let p = ring[i as usize].pos;
                let (sn, cs) = th.sin_cos();
                let (x, y) = (p[0] + 32.0, p[1] + 16.0);
                v.push(([x - sn * h, y + cs * h], [ua, va], al));
                v.push(([x + sn * h, y - cs * h], [ua, vb], al));
            }
            if v.len() > 2 {
                anm.draw_ring_strip(gfx, vm, &v);
            }
        }
        (e.vm.scale, e.vm.color) = (scale, color);
    }

    // GameFlags bit 13 with boss slot 0 set (07 §7 op 176): Player_DamageAgainst over the whole playfield with the
    // boss's time-orb accumulator; the damage and hitByBomb are discarded, so every shot only counts as a hit.
    fn last_spell_box(&mut self, anm: &mut AnmManager) {
        let Some(boss) = self.bosses[0] else {
            return;
        };
        if crate::gamemgr::flags() & crate::gamemgr::F_13 == 0 {
            return;
        }
        let mut acc = self.enemies[boss].orb_acc;
        let mut scratch = false;
        {
            let mut b = self.bullets.borrow_mut();
            let mut p = self.player.borrow_mut();
            p.env.spell = self.g.spell.is_some();
            p.damage_against(
                anm,
                Some(&mut b.items),
                [192.0, 224.0, 0.0],
                [384.0, 448.0],
                &mut acc,
                &mut scratch,
            );
        }
        self.enemies[boss].orb_acc = acc;
    }

    // Enemy drawing (07 §10.6): sub VM 0, main VM, sub VM 1, all Draw2DRot.
    fn draw_layer(&mut self, ctx_anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, high: bool) {
        gfx.set_viewport(PLAYFIELD);
        // Prio 11 draws with tint 0xFFF01010 while bullets are frozen (07 §10.6).
        let frozen = self.player.borrow().freeze_bullets();
        if high && frozen {
            (ctx_anm.tint, ctx_anm.tint_on) = (0xFFF01010, true);
        }
        // Lists are prepended (07 §10.1 step 7), so they are drawn newest-linked first.
        let first = if high { 2 } else { 0 };
        let order: Vec<usize> = (first..first + 2)
            .flat_map(|l| self.draw_lists[l].iter().rev().copied())
            .collect();
        for i in order {
            let e = &mut self.enemies[i];
            if !e.alive() || e.flags & A_HIDDEN != 0 {
                continue;
            }
            let p = [e.final_pos[0] + 32.0, e.final_pos[1] + 16.0];
            let (angle, main_off) = (e.angle, e.vm.pos_offset);
            // Flags B bit 8: sub VMs use the main VM's position offset.
            let shared = e.flags_b & (1 << 8) != 0;
            for k in [0usize, 2, 1] {
                let main = k == 2;
                if main && e.trail[0] != 0 {
                    Self::draw_trail(ctx_anm, gfx, e);
                }
                let vm = if main { &mut e.vm } else { &mut e.sub_vms[k] };
                // Main VM skipped when flag A bit 5 (invisible) or trail flag 0x10.
                if vm.script.is_none()
                    || main && (e.flags & (1 << 5) != 0 || e.trail[0] & 0x10 != 0)
                {
                    continue;
                }
                let off = if main || shared {
                    main_off
                } else {
                    vm.pos_offset
                };
                let z = if main { 0.25 } else { 0.3 };
                let rot = if main {
                    (e.flags & (1 << 25) != 0).then_some(angle)
                } else {
                    (vm.tag != 0).then_some(if k == 0 { angle } else { -angle })
                };
                if let Some(r) = rot
                    && vm.rotation[2] != r
                {
                    vm.rotation[2] = r;
                    vm.flags |= anmvm::F_ROT_DIRTY;
                }
                let saved_off = vm.pos_offset;
                vm.pos = [p[0] + off[0], p[1] + off[1], z];
                vm.pos_offset = [0.0; 3];
                // Draw2DRot carries the VM z (0.25 main / 0.3 sub) as the vertex depth (07 §10.6, 05 §9).
                ctx_anm.draw_mode_z(gfx, vm, 2, z);
                vm.pos_offset = saved_off;
            }
        }
        if high {
            ctx_anm.tint_off();
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
        // Timeline op 16 sets the game-over byte during this calc (11 §4.2).
        self.player.borrow_mut().store_game_over(&mut ctx.game);
        Ret::Next
    }

    // Draw prio 8 → layers 0–1, prio 11 → layers 2–3 (07 §10.6).
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        self.draw_layer(&mut ctx.anm, &mut ctx.gfx, tag == 1);
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GameFlags bit 10 (07 §10.1 step 0): only the play-time counter runs; lists and the manager clock hold.
    #[test]
    fn death_freeze_skips_manager() {
        let mut anm = AnmManager::default();
        let player = crate::player::Player::new_ref();
        let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
        let ecl = ecl::Ecl {
            data: Vec::new(),
            subs: Vec::new(),
            timelines: Vec::new(),
        };
        let mut m = EnemyManager::new(ecl, bullets, 1);
        m.draw_lists[0].push(5);
        player.borrow_mut().set_freeze_bullets(true);
        m.step(&mut anm);
        assert_eq!((m.clock.cur, m.draw_lists[0].len()), (0, 1));
        assert_eq!(player.borrow().stats.rec.progress, 1);
        player.borrow_mut().set_freeze_bullets(false);
        m.step(&mut anm);
        assert_eq!((m.clock.cur, m.draw_lists[0].len()), (1, 0));
        assert_eq!(player.borrow().stats.rec.progress, 2);
    }

    fn empty_manager(anm: &mut AnmManager) -> (EnemyManager, Rc<RefCell<crate::player::Player>>) {
        let player = crate::player::Player::new_ref();
        let bullets = Rc::new(RefCell::new(BulletManager::new(anm, player.clone())));
        let ecl = ecl::Ecl {
            data: Vec::new(),
            subs: Vec::new(),
            timelines: Vec::new(),
        };
        (EnemyManager::new(ecl, bullets, 1), player)
    }

    // 07 §10.1 step 7: youkai state sets colour2 (0x20, 0x20, 0xC0, a / 2) and skips the flash; otherwise the flag
    // is turned off on every frame without a new flash.
    #[test]
    fn tail_youkai_colour2_and_flag_off() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        let e = &mut m.enemies[0];
        e.flags = A_ALIVE | A_YOUKAI;
        e.vm.color = [0xFF, 0xFF, 0xFF, 0x81];
        e.hit = true;
        m.update_tail(0, 1.0);
        let e = &mut m.enemies[0];
        assert_eq!(e.vm.color2, [0xC0, 0x20, 0x20, 0x40]);
        assert_ne!(e.vm.flags & anmvm::F_COLOR2, 0);
        assert!(!e.flash && !e.hit);
        e.flags = A_ALIVE;
        m.update_tail(0, 1.0);
        assert_eq!(m.enemies[0].vm.flags & anmvm::F_COLOR2, 0);
    }

    // 07 §10.1 step 2: a held enemy does not move, is not freed off screen, does not die; its timer nets zero.
    #[test]
    fn held_enemy_skips_body() {
        let mut anm = AnmManager::default();
        let (mut m, player) = empty_manager(&mut anm);
        for (b7, bombing) in [(true, false), (false, true)] {
            player.borrow_mut().bombing = bombing;
            let e = &mut m.enemies[0];
            **e = Enemy::default();
            e.flags = A_ALIVE | A_ON_SCREEN | if b7 { 0 } else { A_LAST_SPELL };
            e.flags_b = if b7 { B_PAUSED } else { 0 };
            (e.vm.has_sprite, e.vm.sprite_w, e.vm.sprite_h) = (true, 32.0, 32.0);
            (e.pos, e.vel, e.life) = ([-100.0, 100.0, 0.0], [3.0, 0.0, 0.0], 0);
            e.boss_timer.set(10);
            m.update(0, &mut anm);
            let e = &m.enemies[0];
            assert!(e.alive());
            assert_eq!((e.pos[0], e.boss_timer.cur), (-100.0, 10));
            assert_eq!(e.flags_b & B_DEATH_LATCH, 0);
            assert_eq!(m.draw_lists[0].pop(), Some(0));
        }
    }

    // 07 §10.1 step 2: the shared tail runs Enemy_UpdateOrbitEffects for a held enemy too (13 §2.6b).
    #[test]
    fn held_enemy_updates_orbit_effects() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        crate::effect::clear();
        let mut rng = crate::rng::Rng::seeded(0);
        let i = crate::effect::spawn_rng(&mut rng, 4, [0.0; 3], 1, 0);
        let e = &mut m.enemies[0];
        **e = Enemy::default();
        (e.flags, e.flags_b, e.life) = (A_ALIVE, B_PAUSED, 10);
        (e.pos, e.orbit_target) = ([5.0, 6.0, 0.0], 96.0);
        e.fx_attached.push(i);
        m.update(0, &mut anm);
        assert_eq!(crate::effect::peek(i, |x| x.centre), [5.0, 6.0, 0.0]);
    }

    // 07 §10.3b: Enemy_Free sends the aura interrupt 3 and drops the pointer; the aura stays live.
    #[test]
    fn free_ends_aura_without_freeing() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        crate::effect::clear();
        let mut rng = crate::rng::Rng::seeded(0);
        let i = crate::effect::spawn_rng(&mut rng, 4, [0.0; 3], 1, 0);
        let e = &mut m.enemies[0];
        **e = Enemy::default();
        (e.flags, e.fx_aura) = (A_ALIVE, i);
        m.free(0);
        assert_eq!(m.enemies[0].fx_aura, crate::effect::NONE);
        assert!(crate::effect::peek(i, |x| x.active));
        assert_eq!(crate::effect::peek(i, |x| x.vm.pending_interrupt), 3);
    }

    // 07 §10.2 / §10.6: threshold-kill victims jump to their death sub with `Ecl_SetSub` only; threads, thresholds
    // and timeout are kept, the death sub is consumed.
    #[test]
    fn kill_non_bosses_sets_sub_only() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        m.ecl.subs = vec![0, 40];
        for k in 0..2 {
            let e = &mut m.enemies[k];
            (e.flags, e.life, e.death_sub) = (A_ALIVE, 100, 1);
            (e.thresholds, e.timeout) = ([50, -1, -1, -1], 300);
            e.threads[0] = Some(Box::default());
            e.main.ctx.clock.set(7);
        }
        m.enemies[1].flags |= A_BOSS;
        m.kill_non_bosses();
        let e = &m.enemies[0];
        assert_eq!(
            (e.life, e.death_sub, e.main.ctx.ip, e.main.ctx.sub),
            (0, -1, 40, 1)
        );
        assert_eq!(e.main.ctx.clock.cur, 0);
        assert!(e.threads[0].is_some());
        assert_eq!((e.thresholds[0], e.timeout), (50, 300));
        assert_eq!((m.enemies[1].life, m.enemies[1].death_sub), (100, 1));
    }

    // 07 §10.1 / §10.3: death (step 6) reads the timer before step 7 ticks it; a mode-0 death skips the link.
    #[test]
    fn death_before_timer_tick() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        let e = &mut m.enemies[0];
        (e.flags, e.life, e.timeout) = (A_ALIVE | A_ON_SCREEN, 0, 600);
        (e.vm.has_sprite, e.vm.sprite_w, e.vm.sprite_h) = (true, 32.0, 32.0);
        e.pos = [100.0, 100.0, 0.0];
        e.boss_timer.set(60);
        m.update(0, &mut anm);
        let e = &m.enemies[0];
        assert!(!e.alive());
        assert_eq!((e.bonus_seconds, e.boss_timer.cur), (9, 61));
        assert!(m.draw_lists[0].is_empty());
    }

    // 12 §5.1: HUD +0x24 from Enemy_CheckTimeout before the tick; +0x30 = life / max life e+0x2E00, no clamp.
    #[test]
    fn boss_hud_timer_and_target() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        let e = &mut m.enemies[0];
        (e.flags, e.boss_slot, e.timeout) = (A_ALIVE | A_BOSS, 0, 600);
        (e.life, e.max_life, e.hud_life) = (500, 400, 100);
        e.boss_timer.set(125);
        assert!(!m.check_timeout(&mut anm, 0));
        m.update_tail(0, 1.0);
        assert_eq!((m.g.hud_timer, m.g.hud_bar_target), (7, 1.25));
        m.enemies[0].boss_slot = 1;
        m.enemies[0].boss_timer.set(0);
        m.check_timeout(&mut anm, 0);
        assert_eq!(m.g.hud_timer, 7);
    }

    // 07 §7 op 95: popups 2000, then +30 each, at the final pos and every 6th trail entry; every victim unlinked.
    #[test]
    fn kill_all_popups_and_unlink() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        for k in 0..4 {
            m.enemies[k].flags = A_ALIVE;
            m.enemies[k].life = 100;
        }
        let e = &mut m.enemies[0];
        e.flags |= A_CANCEL_BONUS;
        (e.pos, e.pos_offset, e.trail) = ([10.0, 20.0, 0.0], [1.0, 2.0, 0.0], [1, 15, 0, 1]);
        m.enemies[1].flags |= A_CANCEL_BONUS;
        m.enemies[3].flags |= A_BOSS | A_CANCEL_BONUS;
        (m.enemies[0].chain_next, m.enemies[1].chain_next) = (Some(1), Some(2));
        (m.enemies[1].chain_prev, m.enemies[2].chain_prev) = (Some(0), Some(1));
        (m.enemies[1].root, m.enemies[2].root) = (Some(0), Some(0));
        // Slot 0: final pos + entries 0, 6, 12; slot 1: one more popup.
        assert_eq!(m.kill_all(&mut anm), 2000 + 2030 + 2060 + 2090 + 2120);
        assert_eq!(m.enemies[0].final_pos, [11.0, 22.0, 0.0]);
        for k in 0..3 {
            let e = &m.enemies[k];
            assert_eq!(e.life, 0);
            assert_eq!(
                (e.root, e.chain_prev, e.chain_next),
                (None, None, None),
                "slot {k}"
            );
        }
        assert_eq!(m.enemies[3].life, 100);
    }

    // 07 §10.3a Enemy_ChainUnlink: a root keeps its first child's +4, whose unlink re-attaches the rest to the root.
    #[test]
    fn chain_unlink_root_quirk() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        (m.enemies[0].chain_next, m.enemies[1].chain_next) = (Some(1), Some(2));
        (m.enemies[1].chain_prev, m.enemies[2].chain_prev) = (Some(0), Some(1));
        (m.enemies[1].root, m.enemies[2].root) = (Some(0), Some(0));
        m.unlink(0);
        assert_eq!(m.enemies[0].chain_next, None);
        assert_eq!(
            (m.enemies[1].chain_prev, m.enemies[1].root),
            (Some(0), Some(0))
        );
        m.unlink(1);
        assert_eq!(m.enemies[0].chain_next, Some(2));
        assert_eq!(m.enemies[2].chain_prev, Some(0));
    }

    // 07 §10.1 step 4: the ring shifts n − 1 entries, entry 0 = final pos, velocity, angle; the rest untouched.
    #[test]
    fn trail_ring_shift() {
        let mut e = Enemy::default();
        e.trail = [5, 3, 0, 1];
        for k in 0..4 {
            e.final_pos = [k as f32, 0.0, 0.0];
            e.vel = [0.0, k as f32, 0.0];
            e.angle = k as f32 / 10.0;
            e.push_trail();
        }
        let xs: Vec<f32> = e.trail_ring[..4].iter().map(|t| t.pos[0]).collect();
        assert_eq!(xs, [3.0, 2.0, 1.0, -999.0]);
        assert_eq!((e.trail_ring[1].vel[1], e.trail_ring[1].angle), (2.0, 0.2));
    }

    // 07 §10.1 step 4: with a trail, an off-screen enemy is freed only once the oldest entry n − 1 is off screen too.
    #[test]
    fn trail_oldest_entry_keeps_enemy() {
        let mut anm = AnmManager::default();
        let (mut m, _) = empty_manager(&mut anm);
        let e = &mut m.enemies[0];
        e.flags = A_ALIVE | A_ON_SCREEN;
        (e.vm.has_sprite, e.vm.sprite_w, e.vm.sprite_h) = (true, 32.0, 32.0);
        e.final_pos = [-100.0, 100.0, 0.0];
        e.trail = [1, 15, 0, 1];
        e.trail_ring[14].pos = [10.0, 100.0, 0.0];
        assert!(!m.screen_check(0));
        m.enemies[0].trail_ring[14].pos = [-17.0, 100.0, 0.0];
        assert!(m.screen_check(0));
        // Without a trail the ring is ignored.
        m.enemies[0].trail = [0, 15, 0, 1];
        m.enemies[0].trail_ring[14].pos = [10.0, 100.0, 0.0];
        assert!(m.screen_check(0));
    }

    // 07 §10.6 mid(): lo + d/2 with the wrap distance taken from lo.
    #[test]
    fn trail_mid_quirk() {
        assert!((trail_mid(1.0, 2.0) - 1.5).abs() < 1e-6);
        assert!(
            (trail_mid(3.0, -3.0) - (-3.0 + 0.5 * (2.0 * std::f32::consts::PI - 6.0))).abs() < 1e-6
        );
    }

    // 07 §7 op 176: with bit 13 and boss 0, every shot in the playfield is a hit, feeding the boss accumulator only.
    #[test]
    fn last_spell_box_hits_all_shots() {
        for bit13 in [false, true] {
            let mut anm = AnmManager::default();
            let (mut m, player) = empty_manager(&mut anm);
            m.enemies[0].flags = A_ALIVE | A_BOSS;
            m.enemies[0].life = 1000;
            m.bosses[0] = Some(0);
            let acc0 = m.enemies[0].orb_acc;
            {
                let mut p = player.borrow_mut();
                crate::gamemgr::set_flags(if bit13 { crate::gamemgr::F_13 } else { 0 });
                p.state_timer.set(5);
                let b = &mut p.bullets[0];
                (b.state, b.kind, b.damage) = (1, 1, 10);
                (b.pos, b.size) = ([20.0, 30.0, 0.0], [8.0, 8.0]);
            }
            m.last_spell_box(&mut anm);
            let state = player.borrow().bullets[0].state;
            let e = &m.enemies[0];
            assert_eq!(state, if bit13 { 2 } else { 1 });
            assert_eq!(e.orb_acc - acc0, if bit13 { 10 } else { 0 });
            assert_eq!(e.life, 1000);
        }
    }

    // 07 §11.1 tail of every op 123 (RE-Q56): the card's boss loses flag A bit 27, g_Gui +4 and bit 11 := 0,
    // also after a silent end (no card active).
    #[test]
    fn spell_end_tail_clears_fixed_bonus() {
        let mut anm = AnmManager::default();
        let (mut m, _player) = empty_manager(&mut anm);
        m.enemies[2].flags |= A_FIXED_BONUS;
        m.g.spell_boss = Some(2);
        m.g.fixed_bonus = true;
        m.g.bonus_frozen = true;
        m.finish_spell(&mut anm);
        assert_eq!(m.enemies[2].flags & A_FIXED_BONUS, 0);
        assert_eq!(m.g.spell_boss, None);
        assert!(!m.g.fixed_bonus && !m.g.bonus_frozen);
    }

    // Spell_End (07 §11.1): bomb cooldown +0xE2A6C := 16 whether or not the card was captured; 0x4E3D28 := 0.
    #[test]
    fn spell_end_sets_bomb_cooldown() {
        let mut anm = AnmManager::default();
        let (mut m, player) = empty_manager(&mut anm);
        crate::ascii::set_darkness(200, 288.0);
        m.finish_spell(&mut anm);
        assert_eq!(player.borrow().bomb_cooldown, 0);
        // 0x4E3D28 := 0 even with no card active.
        assert_eq!(crate::ascii::darkness_alpha(), 0);
        m.g.spell = Some(Spell {
            timed_out: true,
            duration: 600,
            ..Spell::default()
        });
        m.finish_spell(&mut anm);
        assert_eq!(player.borrow().bomb_cooldown, 16);
    }

    // 14 §3.1 (RE-Q54 U): only an enemy with flag A bit 6 is offered as the aimed enemy.
    #[test]
    fn aimed_offer_needs_shot_flag() {
        let mut anm = AnmManager::default();
        let (mut m, player) = empty_manager(&mut anm);
        player.borrow_mut().pos = [192.0, 400.0, 0.0];
        for (f, want) in [(A_ALIVE, None), (A_ALIVE | A_SHOT_HIT, Some(0))] {
            player.borrow_mut().aimed = None;
            let e = &mut m.enemies[0];
            (e.flags, e.life, e.final_pos) = (f, 100, [200.0, 100.0, 0.0]);
            m.player_collision(0, &mut anm);
            assert_eq!(player.borrow().aimed, want);
        }
    }

    // 11 §3.2 (RE-Q54 V): the time-rank tick follows the game-speed clock and fires once per clock value.
    #[test]
    fn time_rank_on_slowed_clock() {
        let mut anm = AnmManager::default();
        let (mut m, player) = empty_manager(&mut anm);
        player.borrow_mut().stats.lives = 8.0;
        anm.speed = 0.5;
        let delta = |n: usize, m: &mut EnemyManager, anm: &mut AnmManager| {
            for _ in 0..n {
                m.step(anm);
            }
            player.borrow().stats.rank_delta
        };
        let r0 = player.borrow().stats.rank_delta;
        assert_eq!(delta(960, &mut m, &mut anm), r0);
        assert_eq!(m.clock.cur, 480);
        assert_eq!(delta(1, &mut m, &mut anm), r0 + 100);
        assert_eq!(delta(1, &mut m, &mut anm), r0 + 100);
    }

    // Human rate (15 §2.3): counted by EnemyMgr_Calc only while its clock reads > 15; human = player +3 == 0.
    #[test]
    fn human_rate_after_clock_15() {
        let mut anm = AnmManager::default();
        let player = crate::player::Player::new_ref();
        let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
        let ecl = ecl::Ecl {
            data: Vec::new(),
            subs: Vec::new(),
            timelines: Vec::new(),
        };
        let mut m = EnemyManager::new(ecl, bullets, 1);
        player.borrow_mut().focus = 0;
        for _ in 0..16 {
            m.step(&mut anm);
        }
        let r = player.borrow().stats.rec.clone();
        assert_eq!((m.clock.cur, r.progress, r.total, r.human), (16, 16, 0, 0));
        m.step(&mut anm);
        player.borrow_mut().focus = 1;
        m.step(&mut anm);
        let r = player.borrow().stats.rec.clone();
        assert_eq!((r.progress, r.total, r.human), (18, 2, 1));
    }

    // Scripted input per shot type through a stage: shooting, moving, focusing, bombing.
    #[test]
    fn scripted_player_runs() {
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        let frames = crate::settings::get().test_frames;
        for shot in 0..12u8 {
            let index = [0, 1, 2, 4][shot as usize % 4];
            let (ecl_name, enm) = files(index, None);
            let ecl = ecl::parse(ar.read(&ecl_name).unwrap()).unwrap();
            let mut anm = AnmManager::default();
            anm.insert(
                ANM_ENEMY,
                "enemy.anm",
                crate::anm::parse(&ar.read("enemy.anm").unwrap()).unwrap(),
                |_| 0,
            );
            anm.insert(
                ANM_STAGE,
                &enm,
                crate::anm::parse(&ar.read(&enm).unwrap()).unwrap(),
                |_| 0,
            );
            anm.insert(
                crate::bullet::ANM_SLOT,
                "etama.anm",
                crate::anm::parse(&ar.read("etama.anm").unwrap()).unwrap(),
                |_| 0,
            );
            let (pa, a, sa) = crate::sht::files(shot);
            anm.insert(
                crate::player::ANM_SLOT,
                &pa,
                crate::anm::parse(&ar.read(&pa).unwrap()).unwrap(),
                |_| 0,
            );
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
                assert!(
                    p.pos.iter().all(|v| v.is_finite()),
                    "shot {shot}: non-finite player"
                );
                for b in p.bullets.iter().filter(|b| b.state != 0) {
                    assert!(
                        b.pos.iter().all(|v| v.is_finite()),
                        "shot {shot}: non-finite bullet"
                    );
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
            assert!(
                shots > 0 && dmg_frames > 0,
                "shot {shot}: no shooting/damage"
            );
            assert!(p.bombs_started > 0, "shot {shot}: never bombed");
        }
    }

    #[test]
    fn all_stage_scripts_run() {
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        let all = (0..9).flat_map(|i| [(i, None), (i, Some(0))]);
        for (index, spell) in all.chain((205..=221).map(|c| (0, Some(c)))) {
            let mut fired = 0;
            for diff in [0, 3] {
                let (ecl_name, enm) = files(index, spell);
                let ecl = ecl::parse(ar.read(&ecl_name).unwrap()).unwrap();
                let mut anm = AnmManager::default();
                anm.insert(
                    ANM_ENEMY,
                    "enemy.anm",
                    crate::anm::parse(&ar.read("enemy.anm").unwrap()).unwrap(),
                    |_| 0,
                );
                anm.insert(
                    ANM_STAGE,
                    &enm,
                    crate::anm::parse(&ar.read(&enm).unwrap()).unwrap(),
                    |_| 0,
                );
                anm.insert(
                    crate::bullet::ANM_SLOT,
                    "etama.anm",
                    crate::anm::parse(&ar.read("etama.anm").unwrap()).unwrap(),
                    |_| 0,
                );
                let player = crate::player::Player::new_ref();
                let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
                let diff = if index % 9 == 8 { 4 } else { diff };
                let mut m = EnemyManager::new(ecl, bullets.clone(), diff);
                m.debug_damage = 8;
                let (mut spawned, mut max_alive, mut max_live, mut lasers, mut items) =
                    (0, 0, 0, 0, 0);
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
                        assert!(
                            x.pos.iter().all(|v| v.is_finite())
                                && x.vel.iter().all(|v| v.is_finite()),
                            "{ecl_name}: non-finite bullet"
                        );
                    }
                    drop(b);
                    let n = m.alive_count();
                    max_alive = max_alive.max(n);
                    spawned += (m
                        .enemies
                        .iter()
                        .filter(|e| e.alive() && e.main.ctx.clock.cur == 1)
                        .count()
                        > 0) as i32;
                    for e in m.enemies.iter().filter(|e| e.alive()) {
                        assert!(
                            e.pos.iter().all(|v| v.is_finite()),
                            "{ecl_name}: non-finite position"
                        );
                    }
                }
                assert!(
                    max_alive > 0,
                    "{ecl_name} diff {diff}: no enemy ever spawned"
                );
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
            assert!(
                spell.is_some() || fired > 0,
                "{}: nothing fired",
                files(index, spell).0
            );
        }
    }
}

#[cfg(test)]
mod file_tests {
    use super::files;

    // 07 §1 selection table and card tables 0x4C70E4 / 0x4C7054.
    #[test]
    fn spell_practice_files() {
        let f = |i, c| {
            let (e, a) = files(i, c);
            format!("{e} {a}")
        };
        assert_eq!(f(7, None), "ecldata7.ecl stg7enm.anm");
        assert_eq!(f(7, Some(171)), "ecldata7sp.ecl stg7enm.anm");
        assert_eq!(f(2, Some(54)), "ecldata3sp.ecl stg3enm.anm");
        assert_eq!(f(8, Some(205)), "ecldata1sp.ecl stg1enm.anm");
        assert_eq!(f(8, Some(208)), "ecldata5sp.ecl stg5enm.anm");
        assert_eq!(f(8, Some(215)), "ecldata4bsp.ecl stg4benm.anm");
        assert_eq!(f(8, Some(216)), "ecldata_sk.ecl stgenm_sk.anm");
        assert_eq!(f(0, Some(221)), "ecldata_yk.ecl stgenm_yk.anm");
    }
}

#[cfg(test)]
mod laser_tests {
    use super::*;

    // Lasers only appear in later boss phases: run the laser stages longer with more debug damage.
    #[test]
    fn laser_stages_run() {
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        let mut total = 0;
        for index in [2, 4, 6, 7, 8] {
            let (ecl_name, enm) = files(index, None);
            let mut anm = AnmManager::default();
            for (slot, name) in [
                (ANM_ENEMY, "enemy.anm"),
                (ANM_STAGE, enm.as_str()),
                (crate::bullet::ANM_SLOT, "etama.anm"),
            ] {
                anm.insert(
                    slot,
                    name,
                    crate::anm::parse(&ar.read(name).unwrap()).unwrap(),
                    |_| 0,
                );
            }
            let player = crate::player::Player::new_ref();
            let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player.clone())));
            let mut m = EnemyManager::new(
                ecl::parse(ar.read(&ecl_name).unwrap()).unwrap(),
                bullets.clone(),
                3,
            );
            m.debug_damage = 40;
            let mut lasers = 0;
            for _ in 0..20000 {
                player
                    .borrow_mut()
                    .step(&mut anm, &crate::input::Input::default());
                m.step(&mut anm);
                bullets.borrow_mut().step(&mut anm);
                let b = bullets.borrow();
                for l in b.lasers.iter().filter(|l| l.alive) {
                    lasers += 1;
                    assert!(
                        l.head.is_finite() && l.tail.is_finite() && l.tail <= l.head + 1e-3,
                        "{ecl_name}: bad laser"
                    );
                }
            }
            eprintln!(
                "{ecl_name}: laser-frames {lasers} hits {}",
                player.borrow().hits
            );
            total += lasers;
        }
        assert!(total > 0, "no laser ever spawned");
    }
}
