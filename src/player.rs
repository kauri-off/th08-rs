//! Player 0x17D5EF8 and the stats block (09 §1–§3, §6, §9; 11 §5); shots in pshot.rs, bombs in bomb.rs.
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::BulletManager;
use crate::game::Ctx;
use crate::gamemgr::{self, F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE};
use crate::gfx::PLAYFIELD;
use crate::input;
use crate::item::ItemManager;
use crate::rng::Rng;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub use crate::bomb::{Area, Bomb};
pub use crate::pshot::{Opt, PBullet};
pub use crate::sht::Sht;

// Player_Calc 0x44C390 (09 §1).
pub const CALC_PRIO: u16 = 9;
// Requests for the enemy manager: dissolve spell started / ended.
pub const DISSOLVE_BEGIN: u8 = 1;
pub const DISSOLVE_END: u8 = 2;
// Draw 0x44D530 / 0x44D630 (09 §1, 14 §5).
pub const DRAW_PRIO: u16 = 9;
pub const DRAW_PRIO_HIGH: u16 = 10;
// playerNN.anm slot (09 §1.1 +0xC).
pub const ANM_SLOT: usize = 5;
// (192, 448 − 64, 0.2), playfield px (09 §6.2, z per §6.4).
pub const RESPAWN: [f32; 3] = [192.0, 384.0, 0.2];
// Player_Load: (playfield w / 2, playfield h − 64, 0.49) (09 §6.4).
pub const START: [f32; 3] = [192.0, 384.0, 0.49];
// Clamp rect min x, min y, max x, max y (GM +0x3DDE4.., 09 §3).
pub const CLAMP: [f32; 4] = [8.0, 16.0, 376.0, 432.0];

pub type PlayerRef = Rc<RefCell<Player>>;

// Gauge limits (game manager +0x3DDF8.., 09 §9.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaugeLimits {
    pub min: i32,
    pub max: i32,
    pub human_max: i32,
    pub youkai_max: i32,
    pub human: i32,
    pub youkai: i32,
}

impl Default for GaugeLimits {
    fn default() -> Self {
        Self {
            min: -10000,
            max: 10000,
            human_max: -8000,
            youkai_max: 8000,
            human: -2000,
            youkai: 2000,
        }
    }
}

impl GaugeLimits {
    // Table of 09 §9.3 by shot type (solo human = even ≥ 4, solo youkai = odd ≥ 4).
    pub fn for_shot(shot: u8) -> Self {
        let d = Self::default();
        match shot {
            3 => Self {
                min: -5000,
                human_max: -3000,
                ..d
            },
            10 => Self {
                min: -5000,
                max: 5000,
                human_max: -3000,
                youkai_max: 3000,
                ..d
            },
            s if s >= 4 && s % 2 == 0 => Self {
                max: 2000,
                youkai: 2001,
                ..d
            },
            s if s >= 4 => Self {
                min: -2000,
                human: -2001,
                ..d
            },
            _ => d,
        }
    }
}

// Game-manager stats block (09 §9.1, 11 §5).
#[derive(Clone, Debug)]
pub struct Stats {
    // GM +0x30, 0 Easy .. 4 Extra.
    pub difficulty: i32,
    // +0x08 score ÷ 10, cap 999 999 999.
    pub score10: i64,
    // +0x04 graze this stage (cap 99 999), +0x0C total (cap 999 999).
    pub graze: i32,
    pub graze_total: i32,
    // +0x22 human/youkai gauge.
    pub gauge: i16,
    // GM +0x3DDF8.. (09 §9.3).
    pub limits: GaugeLimits,
    // +0x24 max point value.
    pub max_point: i32,
    // +0x2C point items this stage, +0x30 total (extend counter).
    pub point_items: i32,
    pub point_items2: i32,
    // +0x34 extends (−1 disabled), +0x38 next extend threshold.
    pub extends: i32,
    pub next_extend: i32,
    // +0x3C time this stage, +0x40 clock-advance threshold, +0x44 time added (parity for AddTime).
    pub time: i32,
    pub time_threshold: i32,
    pub time_added: i32,
    // +0x74 / +0x80 / +0x98 floats; getters truncate.
    pub lives: f32,
    pub bombs: f32,
    pub power: f32,
    // +0x64 deaths, +0x84 bombs used.
    pub deaths: i32,
    pub bombs_used: i32,
    // Port-only: sum of this frame's rank deltas.
    pub rank_delta: i32,
    // 0x164D31C stage frames, 0x164D32C human-max / 0x164D328 youkai-max frames (09 §1 step 10).
    pub stage_frames: u32,
    pub human_frames: u32,
    pub youkai_frames: u32,
    // Port-only: pending Spell_AddBonus 0x416B10 calls (+8000 each, one per time orb, 07 §11), paid by the enemy manager.
    pub spell_bonus_orbs: i32,
    // Port-only: point items collected (statistics).
    pub point_counter: i32,
    // +0x00 displayed score, +0x10 roll step, +0x14 hiscore, +0x18 continues at hiscore (11 §4 step 9).
    pub score_disp: i64,
    pub score_step: i64,
    pub hiscore: i64,
    pub hiscore_cont: u8,
    // +0x28 night clock 0–12, +0x29 continues used.
    pub clock: i32,
    pub continues: u8,
    // GM byte 0x164D0B0: := 0 at a fresh start, the real death, a small power item < 128; record +0x20 (11 §2).
    pub d0b0: u8,
    // +0x1C spell cards captured.
    pub spells: i32,
    // +0x20 gauge at stage start.
    pub gauge_start: i16,
    // Port-only HUD popup queue.
    pub popups: Vec<Popup>,
    pub rec: Record,
}

// In-game HSCR record counters (15 §2.3; GM+0x3DA2C), zeroed with the stats at a new game. human/total: counted by
// EnemyMgr_Calc (human rate = human·10000/total).
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub frames: u32,
    // 0x164D30C play-time counter (11 §2): EnemyMgr_Calc outside dialogue, + 1800 per ECL op 148; zeroed at a fresh start (11 §3.2).
    pub progress: i32,
    pub points: u32,
    pub bombs: u32,
    pub last_spells: u32,
    pub pauses: u32,
    pub orbs: i32,
    pub human: u32,
    pub total: u32,
}

// HUD popup requests (12 §5.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Popup {
    Bonus(i32),
    Message(u8, i32),
    SpellBonus(i32),
}

impl Default for Stats {
    // Starting lives come from the config at game start (gamemgr); 2 is the config default (15 §1.1).
    fn default() -> Self {
        Self {
            difficulty: 1,
            score10: 0,
            graze: 0,
            graze_total: 0,
            gauge: 0,
            limits: GaugeLimits::default(),
            max_point: 100000,
            point_items: 0,
            point_items2: 0,
            extends: 0,
            next_extend: 100,
            time: 0,
            time_threshold: 0,
            time_added: 0,
            lives: 2.0,
            bombs: 3.0,
            power: 0.0,
            deaths: 0,
            bombs_used: 0,
            rank_delta: 0,
            stage_frames: 0,
            human_frames: 0,
            youkai_frames: 0,
            spell_bonus_orbs: 0,
            point_counter: 0,
            score_disp: 0,
            score_step: 0,
            hiscore: 100000,
            hiscore_cont: 0,
            clock: 0,
            continues: 0,
            d0b0: 0,
            spells: 0,
            gauge_start: 0,
            popups: Vec::new(),
            rec: Record::default(),
        }
    }
}

// Max point value at a fresh start by difficulty (11 §3.2).
pub const MAX_POINT: [i32; 5] = [60000, 100000, 200000, 300000, 300000];
// Time threshold per stage index 0–8 and difficulty E/N/H/L (11 §3.1); Extra reads 10.
pub const TIME_THR: [[i32; 4]; 8] = [
    [2000, 2500, 2700, 3000],
    [6500, 7200, 7200, 7200],
    [7500, 8500, 8800, 8800],
    [9999; 4],
    [7500, 8500, 8500, 8500],
    [9999; 4],
    [0; 4],
    [0; 4],
];

impl Stats {
    // AddScore 0x4181F0: score += n/10 (11 §5).
    pub fn add_score(&mut self, v: i32) {
        self.score10 = (self.score10 + (v / 10) as i64).min(999_999_999);
    }

    pub fn score(&self) -> i64 {
        self.score10 * 10
    }

    // Rank deltas go straight through the GM sub-counter (11 §3.2); `rank_delta` only sums them per frame.
    pub fn rank(&mut self, d: i32) {
        self.rank_delta += d;
        crate::gamemgr::rank_add(d);
    }

    // GameMgr_UpdateChecksum 0x406E50: only the 2 × Rng_Range(100000) draws are kept (11 §1.5, §5).
    pub fn checksum(rng: &mut Rng) {
        rng.range(100000);
        rng.range(100000);
    }

    // `s = g_Rng; GameMgr_UpdateChecksum; g_Rng = s`: state kept, the 4 draws still count in `rng_ctr` (11 §1.4).
    pub fn checksum_restored(rng: &mut Rng) {
        let s = rng.state;
        Self::checksum(rng);
        rng.state = s;
    }

    // GameMgr_ScrambleDecoys 0x44E160: 5 × Range + 3 × Float, draws only (11 §1.5).
    pub fn scramble(rng: &mut Rng) {
        for _ in 0..5 {
            rng.range(100000);
        }
        for _ in 0..3 {
            rng.next_f32();
        }
    }

    // Gauge queries (09 §9.3).
    pub fn youkai_max(&self) -> bool {
        self.gauge as i32 >= self.limits.youkai_max
    }

    pub fn youkai(&self) -> bool {
        self.gauge as i32 >= self.limits.youkai
    }

    pub fn human_max(&self) -> bool {
        self.gauge as i32 <= self.limits.human_max
    }

    pub fn human(&self) -> bool {
        self.gauge as i32 <= self.limits.human
    }

    // GaugeAdd clamp; callers apply the "ignored while bombing unless forced" rule (09 §9.3).
    pub fn gauge_add(&mut self, n: i32) {
        self.gauge = (self.gauge as i32 + n).clamp(self.limits.min, self.limits.max) as i16;
    }

    // 08 §8.5 1up rule (Extend 0x439B29, 11 §5): life, else bomb; sfx 0x1C, rank +200.
    pub fn extend(&mut self, rng: &mut Rng) {
        if self.lives < 8.0 {
            self.add_lives(1.0, rng);
            self.rank(200);
            crate::sound::play(crate::sound::EXTEND);
        } else if self.bombs < 8.0 {
            self.add_bombs(1.0, rng);
            self.rank(200);
            crate::sound::play(crate::sound::EXTEND);
        }
    }

    // AddLives 0x43C641 / AddBombs 0x439883 / SetPower 0x406FA0: each runs a checksum (09 §9.4).
    pub fn add_lives(&mut self, n: f32, rng: &mut Rng) {
        self.lives += n;
        Self::checksum(rng);
    }

    pub fn add_bombs(&mut self, n: f32, rng: &mut Rng) {
        self.bombs += n;
        Self::checksum(rng);
    }

    pub fn set_power(&mut self, p: f32, rng: &mut Rng) {
        self.power = p;
        Self::checksum(rng);
    }

    // AddTime 0x418220 (11 §1.5): clamps at 0 (no checksum when clamped); n > 0 also raises the max point value
    // with the updated parity.
    pub fn add_time(&mut self, n: i32, rng: &mut Rng) {
        if self.time + n < 0 {
            self.time = 0;
            return;
        }
        self.time += n;
        self.rec.orbs += n;
        // Parity +0x44 takes negative n too (death-bomb drain, death penalty) (11 §1.5 setters).
        self.time_added += n;
        if n > 0 {
            self.max_point += (n + (self.time_added & 1)) / 2 * 10;
        }
        Self::checksum(rng);
    }
}

// Centred boxes, inclusive overlap (08 §8.4).
// Box test of HitTest / HitTestNoBomb / GrazeTest (08 §4.1 step 6): object min = (p − s/2) − e,
// max = (s/2 + p) + e, each edge an f32, against a stored player box [min x, min y, max x, max y]; touching overlaps.
fn box_overlap(p: [f32; 3], size: [f32; 2], e: f32, b: [f32; 4]) -> bool {
    let h = [size[0] / 2.0, size[1] / 2.0];
    let min = [(p[0] - h[0]) - e, (p[1] - h[1]) - e];
    let max = [(h[0] + p[0]) + e, (h[1] + p[1]) + e];
    !(max[0] < b[0] || b[2] < min[0] || max[1] < b[1] || b[3] < min[1])
}

// Facts the player needs from other managers (bosses, spell card, dialogue).
#[derive(Clone, Copy, Debug, Default)]
pub struct GrazeEnv {
    pub boss: bool,
    pub spell: bool,
    pub dialogue: bool,
}

// Player object 0x17D5EF8, 0xE2B30 bytes (09 §1.1).
pub struct Player {
    // +0: 0 normal, 1 respawning, 2 dying (deathbomb window), 3 invulnerable.
    pub state: u8,
    // +3: 0 unfocused / human, 1 focused / youkai, 2 right after load.
    pub focus: u8,
    // +4 last-spell request (deathbomb window).
    pub last_spell: u8,
    // +5 active character: focus held > 6 frames; solos: shot-type parity.
    pub mode: u8,
    // +6 forced bomb (dissolve spell, 09 §6.1).
    pub forced_bomb: u8,
    pub dissolve_req: u8,
    // +8 frames since the last focus change.
    pub focus_age: i32,
    // Gauge-max aura effect 25 alive (09 §9.3).
    pub gauge_aura: bool,
    // +0x10 sprite VM.
    pub vm: AnmVm,
    // +0x2B4 playfield px.
    pub pos: [f32; 3],
    // +0x2CC previous positions, shifted only on frames with movement.
    pub trail: [[f32; 3]; 16],
    // +0x3F8 velocity × gs.
    pub last_disp: [f32; 2],
    // +0x404 / +0x408 x / y speed multipliers (bombs change them).
    pub speed_mult: [f32; 2],
    // +0x40C 4 × 0x2F4 (09 §5, 14 §3).
    pub options: [Opt; 4],
    // +0xFDC.. bomb state and objects (14 §1).
    pub bomb: Bomb,
    // +0xFDC bombing.
    pub bombing: bool,
    // +0xE2A7C bomb active flag (items/graze).
    pub bomb_active: bool,
    // +0xB8834 / +0xBB834 192 × 0x40 each (09 §8).
    pub damage_areas: Vec<Area>,
    pub cancel_areas: Vec<Area>,
    // +0xBE838 128 × 0x484 (09 §4.1).
    pub bullets: Vec<PBullet>,
    // +0xE2A38 3 slots (14 §6).
    pub laser_slots: [crate::pshot::LaserSlot; crate::pshot::LASER_SLOTS],
    // +0xE2A68 deathbomb counter / bomb-allowed counter (09 §6).
    pub deathbomb: i32,
    // +0xE2A6C := 16 by Spell_End (07 §11.1); decremented atop Player_UpdateBomb; a normal bomb start needs 0.
    pub bomb_cooldown: i32,
    // +0xE2A70 respawn clear frames (09 §6.3).
    pub respawn_clear: i32,
    // +0xE2A74 / +0xE2A78 human / youkai sht (which fields come from which: 09 §2.1).
    pub sht: Sht,
    pub sht_youkai: Sht,
    // +0xE2A90 cancel item type, reset to 6 by every test.
    pub cancel_item: i32,
    // Port-only: `Player_Die` ended with `ItemMgr_DropAttracted` (09 §6.1); the caller that owns the item
    // manager runs it right after the hit test returns.
    pub drop_attracted: bool,
    // +0x3BC..+0x3D0 item box min x, min y, max x, max y = pos ∓ collect/2, stored by `Player_Move` (09 §1.1, §3).
    pub item_box: [f32; 4],
    // +0x38C / +0x3A4 hit and graze boxes, same layout, also stored only by `Player_Move` (09 §1.1).
    pub hit_box: [f32; 4],
    pub graze_box: [f32; 4],
    // +0xE2A98 direction code (09 §3).
    pub direction: u8,
    // +0xE2A9C / +0xE2AA0 last vx / vy before gs.
    pub last_v: [f32; 2],
    // +0xE2AA4 / +0xE2AB0 homing targets (−999 = none; the second is never written).
    pub targets: [[f32; 3]; 2],
    // +0xE2ABC aimed enemy (index) and its position.
    pub aimed: Option<usize>,
    pub aimed_pos: [f32; 3],
    // Port-only: target 0 holds a boss.
    pub boss_target: bool,
    // +0xE2AC4: −1 idle, 0..19 firing cycle (09 §4.2).
    pub shot_timer: Timer,
    // +0xE2AD0 / +0xE2AE8 gauge timers B / C (09 §9.3).
    pub gauge_b: Timer,
    pub gauge_c: Timer,
    // +0xE2AF4 death / respawn / invulnerability timer.
    pub state_timer: Timer,
    // +0xE2B0C option base angle, −π/2 at load.
    pub option_angle: f32,
    // +0xE2B2C time-orb damage step: 40, solo human 27.
    pub orb_step: i32,
    // GM +0x3DBA9 shot type 0–11 (09 §2).
    pub shot_type: u8,
    pub stats: Stats,
    pub env: GrazeEnv,
    // 0x164D2C8 gameplay frame counter copy.
    pub frames: i32,
    // Pending write of GM +0x3DBB3 (game over), stored into `Game::game_over` by the player / enemy calc that set it.
    pub game_over: bool,
    // Port-only counters (hits, grazes, deaths, bombs started; spell failure reads deaths / bombs_started).
    pub hits: u32,
    // +0xE2A94 effect counter (13 §2.6 type 3).
    pub spark_counter: u8,
    // Invulnerability ring effect alive (fixed slot 0).
    pub invuln_ring: bool,
    pub grazes: u32,
    pub deaths: u32,
    pub bombs_started: u32,
    // Port-only debug switches (TH08_INVULN = noclip probe, 09 §6.1).
    pub debug_move: bool,
    pub debug_invuln: bool,
    pub world: Weak<RefCell<BulletManager>>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            state: 0,
            focus: 2,
            last_spell: 0,
            mode: 0,
            forced_bomb: 0,
            dissolve_req: 0,
            focus_age: 0,
            gauge_aura: false,
            vm: AnmVm::default(),
            pos: START,
            trail: [START; 16],
            last_disp: [0.0; 2],
            speed_mult: [1.0; 2],
            options: Default::default(),
            bomb: Bomb::default(),
            bombing: false,
            bomb_active: false,
            damage_areas: vec![Area::default(); crate::bomb::AREAS],
            cancel_areas: vec![Area::default(); crate::bomb::AREAS],
            bullets: vec![PBullet::default(); crate::pshot::MAX_BULLETS],
            laser_slots: Default::default(),
            deathbomb: 0,
            bomb_cooldown: 0,
            respawn_clear: 0,
            sht: Sht::default(),
            sht_youkai: Sht::default(),
            cancel_item: 6,
            drop_attracted: false,
            item_box: [0.0; 4],
            hit_box: [0.0; 4],
            graze_box: [0.0; 4],
            direction: 0,
            last_v: [0.0; 2],
            targets: [[-999.0, -999.0, 0.0]; 2],
            aimed: None,
            aimed_pos: [0.0; 3],
            boss_target: false,
            shot_timer: Timer {
                cur: -1,
                ..Timer::default()
            },
            gauge_b: Timer::default(),
            gauge_c: Timer::default(),
            state_timer: Timer::default(),
            option_angle: -std::f32::consts::FRAC_PI_2,
            orb_step: 40,
            shot_type: 0,
            stats: Stats::default(),
            env: GrazeEnv::default(),
            frames: 0,
            game_over: false,
            hits: 0,
            spark_counter: 0,
            invuln_ring: false,
            grazes: 0,
            deaths: 0,
            bombs_started: 0,
            debug_move: false,
            debug_invuln: crate::settings::get().invuln,
            world: Weak::new(),
        }
    }
}

// TH08_PLAYER: shot type 0–11 or a name (09 §2).
pub fn parse_shot(s: &str) -> Option<u8> {
    const NAMES: [&str; 12] = [
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
    ];
    let s = s.trim().to_ascii_lowercase();
    if let Ok(n) = s.parse::<u8>() {
        return (n < 12).then_some(n);
    }
    let alias = match s.as_str() {
        "barrier" | "ry" => "reimu&yukari",
        "magic" | "ma" => "marisa&alice",
        "scarlet" | "sr" => "sakuya&remilia",
        "ghost" | "yy" => "youmu&yuyuko",
        x => x,
    };
    NAMES.iter().position(|&n| n == alias).map(|i| i as u8)
}

impl Player {
    pub fn new_ref() -> PlayerRef {
        Rc::new(RefCell::new(Player::default()))
    }

    pub fn solo_human(&self) -> bool {
        self.shot_type >= 4 && self.shot_type.is_multiple_of(2)
    }

    pub fn solo_youkai(&self) -> bool {
        self.shot_type >= 4 && self.shot_type % 2 == 1
    }

    pub fn solo(&self) -> bool {
        self.shot_type >= 4
    }

    // Player_Load 0x44D650 (09 §2, §6.4): sht files, gauge limits, initial script 0 / 5, solo options, state 1
    // with timer 120, or 10 in spell practice (GM +0x3DBAC bit 14, 11 §2.1).
    pub fn load(&mut self, anm: &mut AnmManager, human: Sht, youkai: Sht, shot: u8) {
        self.shot_type = shot;
        self.sht = human;
        self.sht_youkai = youkai;
        self.stats.limits = GaugeLimits::for_shot(shot);
        self.orb_step = if self.solo_human() { 27 } else { 40 };
        let script = if self.solo_youkai() { 5 } else { 0 };
        anm.set_script(&mut self.vm, ANM_SLOT, script);
        self.pos = START;
        self.state = 1;
        let t = if gamemgr::flags() & F_SPELL_PRACTICE != 0 {
            10
        } else {
            120
        };
        self.state_timer.set(t);
        self.focus = 2;
        if self.solo() {
            self.spawn_options(anm, true);
        }
    }

    pub fn register(ctx: &mut Ctx, p: PlayerRef, shot: u8) -> Result<()> {
        let (anm, a, s) = crate::sht::files(shot);
        let human = crate::sht::parse(&ctx.archive.read(&a)?)?;
        let youkai = crate::sht::parse(&ctx.archive.read(&s)?)?;
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, &anm)?;
        {
            let mut p = p.borrow_mut();
            p.load(&mut ctx.anm, human, youkai, shot);
        }
        let t: task::TaskRef<Ctx> = p;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO_HIGH, 1, t)?;
        Ok(())
    }

    // Box half sizes, all from the human sht (09 §2.1, §3).
    pub fn hit_half(&self) -> [f32; 2] {
        [self.sht.hitbox / 2.0; 2]
    }

    pub fn graze_half(&self) -> [f32; 2] {
        [self.sht.graze / 2.0; 2]
    }

    pub fn item_half(&self) -> [f32; 2] {
        [self.sht.collect / 2.0; 2]
    }

    pub fn xy(&self) -> [f32; 2] {
        [self.pos[0], self.pos[1]]
    }

    // Item fall multiplier source: youkai sht when +3 ≠ 0 (09 §2.1, 08 §8.4).
    pub fn item_sht(&self) -> &Sht {
        if self.focus == 1 {
            &self.sht_youkai
        } else {
            &self.sht
        }
    }

    pub fn angle_from(&self, o: [f32; 3]) -> f32 {
        crate::bullet::angle_from(o, self.pos)
    }

    // Player_InBombArea 0x449FF0 (09 §8): pure point test of the active cancel areas; +0xE2A90 is written only on a
    // hit, so a miss keeps the item of the last hit (08 §4.1).
    pub fn in_bomb_area(&mut self, p: [f32; 3]) -> bool {
        for a in self.cancel_areas.iter_mut().filter(|a| a.active) {
            if a.contains([p[0], p[1]]) {
                self.cancel_item = a.item;
                a.total += 1;
                return true;
            }
        }
        false
    }

    // Player_GrazeTest 0x44A470 (08 §4.1): 0 none, 1 grazed (handler ran), 2 inside a bomb area; plus time orbs.
    // Bomb-area test first; bullet box enlarged by 20 px per side; no graze in states 1/2.
    pub fn graze_test(
        &mut self,
        p: [f32; 3],
        size: [f32; 2],
        env: &GrazeEnv,
        rng: &mut Rng,
    ) -> (u8, u8) {
        self.cancel_item = 6;
        if self.in_bomb_area(p) {
            return (2, 0);
        }
        if matches!(self.state, 1 | 2) || !box_overlap(p, size, 20.0, self.graze_box) {
            return (0, 0);
        }
        (1, self.on_graze(p, false, env, rng))
    }

    // Player_HitTest 0x44A230 (09 §6.1 steps 1–6): 0 miss, 1 hit, 2 inside a bomb area.
    pub fn hit_test(&mut self, p: [f32; 3], size: [f32; 2], rng: &mut Rng) -> u8 {
        self.cancel_item = 6;
        if self.in_bomb_area(p) {
            return 2;
        }
        if !box_overlap(p, size, 0.0, self.hit_box) {
            return 0;
        }
        self.die(rng);
        1
    }

    // Hit path of bullets/lasers/enemies (09 §6.1 steps 5–6): state check, ScrambleDecoys, then Player_Die.
    // Quirk kept: the state ≠ 0 return comes before the scramble, so invulnerable hits draw no RNG.
    pub fn die(&mut self, rng: &mut Rng) {
        if self.state != 0 {
            return;
        }
        Stats::scramble(rng);
        self.hits += 1;
        self.player_die(rng);
    }

    // Player_Die 0x44AB40 (09 §6.1); `debug_invuln` returns at its start like the noclip probe.
    // Both paths end with ItemMgr_DropAttracted, requested through `drop_attracted`.
    fn player_die(&mut self, rng: &mut Rng) {
        if self.debug_invuln {
            return;
        }
        // Freeze byte 0x160F534 := 0.
        crate::enemy::set_timer_pause(0);
        Stats::checksum(rng);
        crate::sound::play_at(crate::sound::PLDEAD, self.pos[0]);
        crate::effect::spawn_rng(rng, 6, self.pos, 16, 0xFFFFFFFF);
        self.state = 2;
        self.state_timer.set(0);
        if self.special() {
            self.deathbomb = 2;
            self.forced_bomb = 1;
            self.drop_attracted = true;
            return;
        }
        let st = &mut self.stats;
        st.gauge = 0;
        let n = if st.bombs < 1.0 {
            2
        } else {
            let mut n = st.bombs as i32 * 6;
            if st.time >= st.time_threshold {
                n += 7;
            }
            n = n.min(15);
            if self.env.spell {
                n = (n * 2).min(30);
            }
            if matches!(self.shot_type, 0 | 4 | 5) {
                n = n * 9 / 5;
            }
            n
        };
        // Silhouette and ring 59 (fixed slot 11) only with a bomb in stock (09 §6.1 "else" branch): colour2 :=
        // white with the VM colour's alpha, drawn via VM flag bit 17 (05 §2.1).
        if st.bombs >= 1.0 {
            let a = self.vm.color[anmvm::A];
            self.vm.color2 = [0xFF, 0xFF, 0xFF, a];
            self.vm.flags |= anmvm::F_COLOR2;
            crate::effect::death_ring(rng, self.pos, n);
            // GameFlags bit 10 only in the bombs ≥ 1 branch: a 0-bomb "Miss" during a card freezes nothing (09 §6.1).
            if self.env.spell {
                self.set_freeze_bullets(true);
            }
        }
        self.deathbomb = n;
        self.drop_attracted = true;
    }

    // GameFlags bit 10, the spell-card death freeze (09 §6.1, 11 §2.1).
    pub fn freeze_bullets(&self) -> bool {
        gamemgr::flags() & gamemgr::F_DEATH != 0
    }

    pub fn set_freeze_bullets(&mut self, on: bool) {
        let f = gamemgr::flags() & !gamemgr::F_DEATH;
        gamemgr::set_flags(if on { f | gamemgr::F_DEATH } else { f });
    }

    // GameFlags bits 7–8: a boss last spell runs or was captured (the dissolve-spell mode, 11 §2.1).
    pub fn special(&self) -> bool {
        gamemgr::flags() & (F_LAST_SPELL | F_LAST_CAPTURED) != 0
    }

    // Player_OnGraze 0x44A930 (09 §9.2); returns how many time orbs (item type 10, mode 5) to drop at `p`.
    // Not implemented: HUD flag and statistics bit 0x2000.
    pub fn on_graze(&mut self, p: [f32; 3], laser: bool, env: &GrazeEnv, _rng: &mut Rng) -> u8 {
        self.grazes += 1;
        crate::sound::play_at(crate::sound::GRAZE, p[0]);
        crate::effect::spawn_rng(
            _rng,
            8,
            [(p[0] + self.pos[0]) / 2.0, (p[1] + self.pos[1]) / 2.0, 0.0],
            1,
            0xFFFFFFFF,
        );
        let (solo_human, solo_youkai) = (self.solo_human(), self.solo_youkai());
        let s = &mut self.stats;
        if !self.bombing {
            let g = if s.human_max() {
                3
            } else if s.human() {
                2
            } else {
                1
            };
            s.graze = (s.graze + g).min(99_999);
            s.graze_total = (s.graze_total + g).min(999_999);
        }
        s.rank(6);
        s.add_score(if s.youkai() { 4000 } else { 2000 });
        if self.mode != 0 && !self.bombing {
            s.gauge_add(100);
        }
        let mut orbs = 0;
        if (!solo_human || self.shot_type == 10) && env.boss && s.youkai_max() {
            orbs = 1;
            if !laser && env.spell {
                orbs += 1;
                // Nested in the bullet-and-card branch (09 §9.2, RE-Q28).
                if !solo_youkai {
                    orbs += 1;
                }
            }
        }
        orbs
    }

    // Hit / graze / item boxes = pos ∓ half size (09 §3), as `Player_Move` stores them after the clamp.
    pub fn update_boxes(&mut self) {
        let pos = self.pos;
        let b = |h: [f32; 2]| [pos[0] - h[0], pos[1] - h[1], pos[0] + h[0], pos[1] + h[1]];
        self.hit_box = b(self.hit_half());
        self.graze_box = b(self.graze_half());
        self.item_box = b(self.item_half());
    }

    // Player_ItemCollectTest 0x44A5A0 (08 §8.4): player states 0/3/4; item box (c, c) vs the stored collect box.
    // Each edge is rounded to f32 before the inclusive compare, which differs from |d| ≤ sum of halves.
    pub fn collect_test(&self, p: [f32; 3], c: f32) -> bool {
        let h = c / 2.0;
        let [x0, y0, x1, y1] = self.item_box;
        matches!(self.state, 0 | 3 | 4)
            && !(p[0] + h < x0 || x1 < p[0] - h || p[1] + h < y0 || y1 < p[1] - h)
    }

    // EnemyMgr_Calc target pick (09 §1.1 +0xE2AA4): target 1 = boss (horizontally nearest) else the lowest enemy;
    // Target 2 is never written; the aimed enemy is picked by the enemy manager (14 §3.1).
    pub fn offer_target(&mut self, e: [f32; 3], boss: bool) {
        let t = &mut self.targets[0];
        if boss {
            if !self.boss_target || (e[0] - self.pos[0]).abs() < (t[0] - self.pos[0]).abs() {
                *t = e;
            }
            self.boss_target = true;
        } else if !self.boss_target && e[1] > t[1] {
            *t = e;
        }
    }

    // Player_HitTestNoBomb 0x44A360 (09 §6.1): no bomb-area test; true on overlap in every state; only state 0 dies.
    pub fn contact_test(&mut self, p: [f32; 3], size: [f32; 2], rng: &mut Rng) -> bool {
        self.cancel_item = 6;
        if !box_overlap(p, size, 0.0, self.hit_box) {
            return false;
        }
        self.die(rng);
        true
    }

    // Player_LaserTest 0x44A6A0 (09 §6.1): box centre `c`, size `s` in the frame rotated by `a` about origin `o`.
    // Returns (0 miss, 1 hit, 2 graze; time orbs to drop at the player). No bomb-area test, no +0xE2A90 write;
    // f32 box edges, touching counts as overlap.
    #[allow(clippy::too_many_arguments)]
    pub fn laser_test(
        &mut self,
        c: [f32; 2],
        s: [f32; 2],
        o: [f32; 2],
        a: f32,
        graze: bool,
        env: &GrazeEnv,
        rng: &mut Rng,
    ) -> (u8, u8) {
        // Vec2_Rotate by −a: sin/cos rounded to f32.
        let (sn, cs) = (-a).sin_cos();
        let (dx, dy) = (self.pos[0] - o[0], self.pos[1] - o[1]);
        let q = [o[0] + (cs * dx - sn * dy), o[1] + (sn * dx + cs * dy)];
        let ph = self.hit_half();
        let (pmin, pmax) = ([q[0] - ph[0], q[1] - ph[1]], [q[0] + ph[0], q[1] + ph[1]]);
        let hit = |g: f32| {
            let lmin = [c[0] - s[0] / 2.0 - g, c[1] - s[1] / 2.0 - g];
            let lmax = [c[0] + s[0] / 2.0 + g, c[1] + s[1] / 2.0 + g];
            !(lmax[0] < pmin[0] || pmax[0] < lmin[0] || lmax[1] < pmin[1] || pmax[1] < lmin[1])
        };
        if hit(0.0) {
            // Step 5: state ≠ 0 returns 0 here, before the scramble.
            let alive = self.state == 0;
            self.die(rng);
            return (alive as u8, 0);
        }
        if graze && hit(48.0) && !matches!(self.state, 1 | 2) {
            let p = self.pos;
            return (2, self.on_graze(p, true, env, rng));
        }
        (0, 0)
    }

    // Restarts the sprite VM keeping its position (the player sprite is placed at draw time).
    pub(crate) fn set_script(&mut self, anm: &mut AnmManager, s: usize) {
        let pos = self.vm.pos;
        anm.set_script(&mut self.vm, ANM_SLOT, s);
        self.vm.pos = pos;
    }

    // Sprite script base: 0 human / 5 youkai; teams by focus, solos by parity (09 §3).
    fn sprite_base(&self, focus: u8) -> usize {
        let youkai = if self.solo() {
            self.shot_type % 2 == 1
        } else {
            focus == 1
        };
        if youkai { 5 } else { 0 }
    }

    // Player_Move (09 §3).
    fn move_player(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        let gs = anm.speed;
        let bits = inp.held;
        let (u, d, l, r) = (
            bits & input::UP != 0,
            bits & input::DOWN != 0,
            bits & input::LEFT != 0,
            bits & input::RIGHT != 0,
        );
        // Direction code +0xE2A98 in the priority order of 09 §3.
        self.direction = match (u, d, l, r) {
            (true, _, true, _) => 5,
            (_, true, true, _) => 7,
            (true, _, _, true) => 6,
            (_, true, _, true) => 8,
            (_, true, _, _) => 2,
            (true, ..) => 1,
            (_, _, true, _) => 3,
            (_, _, _, true) => 4,
            _ => 0,
        };
        let f = if self.bombing {
            (self.bomb.index & 1) as u8
        } else {
            (bits & input::FOCUS != 0) as u8
        };
        if f != self.focus {
            self.focus = f;
            if !self.solo() {
                self.swap_options(anm, f);
                self.set_script(anm, self.sprite_base(f));
                // Switch flash only when the previous state lasted > 3 frames (09 §3).
                if self.focus_age > 3 {
                    let (kind, colour) = if f == 1 {
                        (29, 0x80FF8080)
                    } else {
                        (28, 0x808080FF)
                    };
                    crate::effect::spawn(anm, kind, self.pos, 1, colour);
                }
                // 09 §3: the team sprite reset also sets +0xE2A9C := 0, so moving on the switch frame replays the
                // start-move script; solos keep it.
                self.last_v[0] = 0.0;
            }
            // Hitbox marker: effect 22 in fixed slot 2 on every switch to focused, all shot types (14 §5).
            if f == 1 {
                crate::effect::spawn_fixed(anm, 22, self.pos, crate::effect::FIX_AURA, 0xFFFFFFFF);
            } else {
                crate::effect::get(crate::effect::FIXED_BASE + crate::effect::FIX_AURA, |e| {
                    e.vm.pending_interrupt = 1
                });
            }
            self.focus_age = 0;
            self.gauge_c.set(0);
        } else {
            self.focus_age += 1;
        }
        if self.focus_age > 6 {
            self.mode = f;
        }
        if self.solo() {
            self.mode = self.shot_type % 2;
        }
        // Unfocused: human sht +0x24 / +0x2C; focused: youkai sht +0x28 / +0x30 (09 §2.1).
        let (ortho, diag) = if f == 1 {
            (self.sht_youkai.speed_focus, self.sht_youkai.diag_focus)
        } else {
            (self.sht.speed, self.sht.diag)
        };
        let (mut vx, mut vy) = match self.direction {
            1 => (0.0, -ortho),
            2 => (0.0, ortho),
            3 => (-ortho, 0.0),
            4 => (ortho, 0.0),
            5 => (-diag, -diag),
            6 => (diag, -diag),
            7 => (-diag, diag),
            8 => (diag, diag),
            _ => (0.0, 0.0),
        };
        vx *= self.speed_mult[0];
        vy *= self.speed_mult[1];
        let base = self.sprite_base(f);
        let pvx = self.last_v[0];
        // Start/stop scripts 1–4 (+5 youkai) against the previous frame's vx (09 §3).
        let script = if vx < 0.0 && pvx >= 0.0 {
            Some(1)
        } else if vx > 0.0 && pvx <= 0.0 {
            Some(3)
        } else if vx == 0.0 && pvx < 0.0 {
            Some(2)
        } else if vx == 0.0 && pvx > 0.0 {
            Some(4)
        } else {
            None
        };
        if let Some(s) = script {
            self.set_script(anm, base + s);
        }
        self.last_v = [vx, vy];
        self.last_disp = [vx * gs, vy * gs];
        self.pos[0] = (self.pos[0] + vx * gs).clamp(CLAMP[0], CLAMP[2]);
        self.pos[1] = (self.pos[1] + vy * gs).clamp(CLAMP[1], CLAMP[3]);
        self.update_boxes();
        self.update_options(anm, inp.down(input::SHOT));
        if inp.down(input::SHOT) && self.shot_timer.cur < 0 && !self.env.dialogue {
            self.shot_timer.set(0);
        }
        self.gauge_drift(gs);
        if vx != 0.0 || vy != 0.0 {
            self.trail.copy_within(0..15, 1);
            self.trail[0] = self.pos;
        }
    }

    // Gauge per frame (09 §9.3): shooting → timer C ramp (sign by focus); idle → drift toward 0 after B ≥ 30.
    fn gauge_drift(&mut self, gs: f32) {
        if self.focus_age < 30 || self.bombing || self.env.dialogue {
            return;
        }
        let st = &mut self.stats;
        if self.shot_timer.cur >= 0 {
            if self.gauge_b.cur > 0 {
                self.gauge_b.cur -= 1;
            } else {
                let c = self.gauge_c.cur;
                let mut n = if c > 300 { 21 } else { (c / 15).min(21) };
                if self.focus != 1 {
                    n = -n;
                }
                st.gauge_add((n as f32 * gs) as i32);
                self.gauge_c.tick(gs);
            }
        } else {
            if self.gauge_b.cur >= 4 {
                self.gauge_c.set(0);
            }
            if self.gauge_b.cur >= 30 {
                let g = st.gauge as i32;
                if g.abs() <= 9 {
                    st.gauge = 0;
                } else {
                    let step = if st.youkai_max() || st.human_max() {
                        5
                    } else if st.youkai() || st.human() {
                        3
                    } else {
                        2
                    };
                    let d = (step as f32 * gs) as i32;
                    st.gauge = (g - g.signum() * d) as i16;
                }
            } else {
                self.gauge_b.tick(gs);
            }
        }
    }

    // Player_DyingUpdate 0x44CBF0 (09 §6.2); true when the respawn starts.
    fn dying_update(&mut self, anm: &mut AnmManager, items: &mut Option<&mut ItemManager>) -> bool {
        if self.deathbomb > 0 {
            self.stats.add_time(-15, &mut anm.rng);
            self.deathbomb -= 1;
            self.last_spell = 1;
            if self.deathbomb == 0 {
                self.real_death(anm, items);
            }
            return false;
        }
        // 09 §6.2: t counts from the hit (not reset at the real death); last vx/vy := 0 every frame here.
        self.last_v = [0.0; 2];
        let t = self.state_timer.cur;
        if t <= 29 {
            self.vm.scale = [1.0 - t as f32 / 30.0, 1.0 + 3.0 * t as f32 / 30.0];
            self.vm.flags |= anmvm::F_SCALE_DIRTY;
            return false;
        }
        self.state = 1;
        self.pos = RESPAWN;
        self.state_timer.set(0);
        self.vm.scale = [3.0, 3.0];
        self.vm.flags |= anmvm::F_SCALE_DIRTY;
        let base = self.sprite_base(self.focus.min(1));
        self.set_script(anm, base);
        self.vm.scale = [3.0, 3.0];
        if self.stats.lives >= 1.0 {
            self.stats.add_lives(-1.0, &mut anm.rng);
            self.stats.bombs = self.sht.bombs.trunc();
        } else {
            // Game over: the ASCII manager opens the continue menu (12 §4).
            // Lives and bombs are not changed on game over (09 §6.2).
            self.game_over = true;
        }
        true
    }

    // The real death when the deathbomb window runs out (09 §6.2): effects, sound 0xF, unfreeze, miss counter
    // (checksum), time −= min(time/10, 500), drops (mode 2), rank −1600. The spell manager detects the failure
    // through `deaths`.
    fn real_death(&mut self, anm: &mut AnmManager, items: &mut Option<&mut ItemManager>) {
        self.deaths += 1;
        log::info!("player death at frame {}", self.frames);
        crate::sound::play_at(crate::sound::TAN00_LOUD, self.pos[0]);
        crate::effect::free(crate::effect::FIXED_BASE + crate::effect::FIX_DEATH);
        crate::effect::spawn(anm, 6, self.pos, 16, 0xFFFFFFFF);
        crate::effect::spawn_fixed(
            anm,
            12,
            self.pos,
            crate::effect::FIX_DEATH_BURST,
            0xFF4040FF,
        );
        // Unfreeze, then the only tint-off of Player_DyingUpdate (06 §10.2), then the silhouette off (09 §6.2).
        self.set_freeze_bullets(false);
        anm.tint_off();
        self.vm.flags &= !anmvm::F_COLOR2;
        // 0x164D0B0 := 0 here, not in Player_Die: a deathbomb leaves it unchanged (09 §6.2, 11 §2).
        self.stats.d0b0 = 0;
        self.last_spell = 0;
        let rng = &mut anm.rng;
        self.stats.deaths += 1;
        Stats::checksum(rng);
        let t = self.stats.time;
        self.stats.add_time(-(t / 10).min(500), rng);
        let mut drops: Vec<i32> = Vec::new();
        if self.stats.lives < 1.0 {
            self.stats.set_power(0.0, rng);
            drops.extend([crate::item::FULL_POWER as i32; 5]);
        } else {
            let p = self.stats.power;
            self.stats
                .set_power(if p <= 16.0 { 0.0 } else { p - 16.0 }, rng);
            drops.push(crate::item::BIG_POWER as i32);
            drops.extend([crate::item::POWER as i32; 5]);
            if matches!(self.shot_type, 2 | 8 | 9) && self.stats.bombs > 0.0 {
                drops.push(crate::item::BOMB as i32);
            }
        }
        if let Some(it) = items.as_deref_mut() {
            for k in drops {
                it.spawn(anm, self, self.pos, k, 2);
            }
            // 09 §6.2 "attracted items fall": ItemMgr_DropAttracted after the death drops.
            it.drop_attracted();
        }
        self.stats.rank(-1600);
    }

    // Player_RespawnUpdate 0x44D180 (09 §6.3): clear timer := 60 on every call, 30-frame grow from scale 3.
    // Spell practice (GameFlags bit 14): the invulnerability timer is not set and keeps its value 30.
    fn respawn_update(&mut self) {
        self.respawn_clear = 60;
        let t = self.state_timer.cur;
        self.speed_mult = [1.0; 2];
        self.deathbomb = 0;
        if t <= 29 {
            let s = 3.0 - 2.0 * t as f32 / 30.0;
            self.vm.scale = [s, s];
            self.vm.flags |= anmvm::F_SCALE_DIRTY;
            self.vm.color[anmvm::A] = (t * 255 / 30) as u8;
            return;
        }
        self.state = 3;
        self.vm.scale = [1.0, 1.0];
        self.vm.flags |= anmvm::F_SCALE_DIRTY;
        self.vm.color = [255; 4];
        if gamemgr::flags() & F_SPELL_PRACTICE == 0 {
            self.state_timer.set(240);
        }
        self.deathbomb = self.sht.deathbomb;
    }

    // Player_UpdateInvuln 0x44D2C0 (09 §6.4): colour 0xFFF02020 when t mod 8 < 2; the ring follows the player.
    fn update_invuln(&mut self, gs: f32) {
        // 09 §6.3: +0xE2A70 counts down here (60 more frames after the respawn), full-screen clear, item −1.
        // 09 §6.4: the respawn clear runs first, before the state-3 step.
        if self.respawn_clear != 0 {
            self.respawn_clear -= 1;
            self.cancel_rect([self.pos[0], self.pos[1]], [768.0, 896.0], -1, 0);
        }
        if self.state == 3 {
            self.state_timer.dec(gs);
            let t = self.state_timer.cur;
            self.vm.color = if t.rem_euclid(8) < 2 {
                [0x20, 0x20, 0xF0, 0xFF]
            } else {
                [0xFF; 4]
            };
            let ring = crate::effect::FIXED_BASE + crate::effect::FIX_INVULN;
            if self.invuln_ring {
                let p = self.pos;
                crate::effect::get(ring, |e| e.pos = p);
            }
            if t < 1 {
                self.state = 0;
                self.vm.color = [0xFF; 4];
                if std::mem::take(&mut self.invuln_ring) {
                    crate::effect::free(ring);
                }
            }
        } else {
            self.state_timer.tick(gs);
        }
    }

    // Player_Calc 0x44C390 (09 §1); `items` is the item manager when the bullet manager exists.
    pub fn frame(
        &mut self,
        anm: &mut AnmManager,
        inp: &input::Input,
        mut items: Option<&mut ItemManager>,
    ) {
        // Step 1: freeze byte 0x160F534 (ECL native 26) pauses the youkai aura (+0xBE834, fixed slot 2) and the
        // gauge-max aura (+0xE2B24, slot 8) with VM flag 0x80000 and skips the rest of the calc.
        let freeze = crate::enemy::timer_pause() != 0;
        for k in [crate::effect::FIX_AURA, crate::effect::FIX_GAUGE] {
            crate::effect::get(crate::effect::FIXED_BASE + k, |e| {
                if freeze {
                    e.vm.flags |= anmvm::F_PAUSED;
                } else {
                    e.vm.flags &= !anmvm::F_PAUSED;
                }
            });
        }
        if freeze {
            return;
        }
        let gs = anm.speed;
        // Port-only: areas queued by effects (bomb-owned rings) during the last frame.
        self.take_queued_areas();
        self.update_areas();
        self.update_bomb(anm, inp, &mut items);
        match self.state {
            2 => {
                if self.dying_update(anm, &mut items) {
                    self.respawn_update();
                }
            }
            1 => self.respawn_update(),
            _ => {}
        }
        self.update_invuln(gs);
        if !matches!(self.state, 1 | 2) {
            self.move_player(anm, inp);
        }
        if self.vm.script.is_some() {
            self.vm.run(anm);
        }
        // 09 §4.4: skipped while GameFlags bit 10.
        if !self.freeze_bullets() {
            self.update_bullets(anm);
        }
        self.update_shot_timer(anm, inp);
        // Step 9 (0x44D420): HUD panel-side switch, then the target reset.
        crate::ascii::gauge_fade(self.pos);
        self.targets = [[-999.0, -999.0, 0.0]; 2];
        self.boss_target = false;
        if !self.env.dialogue {
            let st = &mut self.stats;
            st.stage_frames += 1;
            if st.human_max() {
                st.human_frames += 1;
                st.add_score(100);
            } else if st.youkai_max() {
                st.youkai_frames += 1;
                st.add_score(100);
            }
        }
        // Gauge-max aura: effect 25 in fixed slot 8 follows the player (09 §9.3, 13 §2.6).
        let at_max = self.stats.youkai_max() || self.stats.human_max();
        if at_max != self.gauge_aura {
            self.gauge_aura = at_max;
            if at_max {
                crate::effect::spawn_fixed(anm, 25, self.pos, crate::effect::FIX_GAUGE, 0xFFFFFFFF);
            } else {
                crate::effect::free(crate::effect::FIXED_BASE + crate::effect::FIX_GAUGE);
            }
        }
        if self.gauge_aura {
            let p = self.pos;
            crate::effect::get(crate::effect::FIXED_BASE + crate::effect::FIX_GAUGE, |e| {
                e.pos = p
            });
        }
        self.frames += 1;
    }

    // Standalone step without a bullet manager (tests).
    pub fn step(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        self.frame(anm, inp, None);
    }

    // 0x164D0BB := 1 is written in the same calc (prio 9 death, prio 11 timeline op 16), so later tasks of this frame
    // see it and the ASCII calc (prio 1) opens the continue menu on the next frame (12 §4).
    pub fn store_game_over(&mut self, g: &mut crate::gamemgr::Game) {
        if std::mem::take(&mut self.game_over) {
            g.game_over = 1;
        }
    }

    // 14 §5 (3): sprite at pos + playfield origin, z 0.1, Draw2DSnap; hidden while the continue menu is up.
    fn draw_sprite(&mut self, anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx, game_over: bool) {
        if self.vm.script.is_none() || game_over {
            return;
        }
        self.vm.pos = [self.pos[0] + 32.0, self.pos[1] + 16.0, 0.0];
        anm.draw_mode_z(gfx, &mut self.vm, 0, 0.1);
    }
}

impl Task<Ctx> for Player {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            // Full exit hides the gauge readout (11 §7, 12 §5.9).
            if !matches!(ctx.game.requested, 3 | 11 | 12) {
                crate::ascii::gauge_interrupt(99);
                // Boss markers M_0..M_2 hidden at once (12 §5.10).
                (0..3).for_each(|k| crate::ascii::marker_interrupt(k, 99));
            }
            ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let inp = ctx.input.game_view();
        // 0x164D2C8 gameplay frame counter (11 §2), read after GameMgr_Calc (prio 2) incremented it.
        self.frames = ctx.game.frame_counter as i32;
        // The dialogue state is read live: the previous frame's dialogue calc (prio 12) ran after the enemies.
        if let Some(gm) = crate::gamemgr::current()
            && let Ok(gm) = gm.try_borrow()
            && let Some(e) = &gm.enemies
            && let Ok(e) = e.try_borrow()
        {
            self.env.dialogue = e.dialogue_active();
        }
        match self.world.upgrade() {
            Some(b) => {
                let mut b = b.borrow_mut();
                self.frame(&mut ctx.anm, &inp, Some(&mut b.items));
                if std::mem::take(&mut self.bomb.attract) {
                    b.items.attract_all();
                }
            }
            None => self.frame(&mut ctx.anm, &inp, None),
        }
        crate::effect::set_player(self.pos);
        crate::effect::set_frozen(self.freeze_bullets());
        self.store_game_over(&mut ctx.game);
        Ret::Next
    }

    // Draw 9 (0x44D530): flying bullets, bomb, sprite, options; draw 10 (0x44D630): bullets in their hit animation
    // (14 §5).
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        if tag == 0 {
            self.draw_bullets(anm, gfx, 1);
            self.draw_bomb(anm, gfx);
            self.draw_sprite(anm, gfx, ctx.game.game_over != 0);
            self.draw_options(anm, gfx);
        } else {
            self.draw_bullets(anm, gfx, 2);
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 11 §3.1 / §1.4: replay stage start checksum keeps the RNG state but counts 4 Next16 draws.
    #[test]
    fn checksum_restored_counts_draws() {
        let mut rng = Rng::seeded(0x1234);
        let t = rng.total;
        Stats::checksum_restored(&mut rng);
        assert_eq!((rng.state, rng.total - t), (0x1234, 4));
    }

    fn ready() -> (Player, AnmManager) {
        let mut p = Player {
            state: 0,
            focus: 0,
            ..Player::default()
        };
        p.stats.limits = GaugeLimits::default();
        (p, AnmManager::default())
    }

    fn input(bits: u16) -> input::Input {
        let mut i = input::Input {
            raw: bits,
            ..Default::default()
        };
        i.tick();
        i
    }

    #[test]
    fn collect_test_rounds_f32_edges() {
        let (mut p, _) = ready();
        p.item_box = [140.0, 0.0, 200.0, 100.0];
        // 127.99999 + 12 rounds to 140.0 in f32 although the exact gap is 12 + 2^-17 (08 §8.4).
        assert!(p.collect_test([127.99999, 50.0, 0.0], 24.0));
        assert!(!p.collect_test([127.9999, 50.0, 0.0], 24.0));
        assert!(p.collect_test([212.0, 112.0, 0.0], 24.0));
        p.state = 1;
        assert!(!p.collect_test([150.0, 50.0, 0.0], 24.0));
    }

    #[test]
    fn hit_and_graze_use_stored_f32_boxes() {
        let (mut p, _) = ready();
        let mut rng = Rng::default();
        p.state = 1;
        p.pos = [170.0, 50.0, 0.0];
        p.hit_box = [140.0, 0.0, 200.0, 100.0];
        // 12 + 127.99999 rounds to 140.0 in f32: touching edges overlap (08 §4.1 step 6).
        assert_eq!(
            p.hit_test([127.99999, 50.0, 0.0], [24.0, 24.0], &mut rng),
            1
        );
        assert_eq!(p.hit_test([127.9999, 50.0, 0.0], [24.0, 24.0], &mut rng), 0);
        assert!(p.contact_test([212.0, 112.0, 0.0], [24.0, 24.0], &mut rng));
        // The boxes are only refreshed by Player_Move: a moved position is not seen until then.
        p.pos = [0.0, 0.0, 0.0];
        assert_eq!(p.hit_test([170.0, 50.0, 0.0], [2.0, 2.0], &mut rng), 1);
        p.sht.graze = 10.0;
        p.update_boxes();
        assert_eq!(p.graze_box, [-5.0, -5.0, 5.0, 5.0]);
        assert!(box_overlap(
            [-37.0, 0.0, 0.0],
            [24.0, 24.0],
            20.0,
            p.graze_box
        ));
        assert!(!box_overlap(
            [-37.001, 0.0, 0.0],
            [24.0, 24.0],
            20.0,
            p.graze_box
        ));
    }

    #[test]
    fn movement_speeds_and_bounds() {
        let (mut p, mut anm) = ready();
        p.step(&mut anm, &input(input::LEFT | input::UP));
        assert!(
            (p.pos[0] - (192.0 - p.sht.diag)).abs() < 1e-5
                && (p.pos[1] - (384.0 - p.sht.diag)).abs() < 1e-5
        );
        let x = p.pos[0];
        p.step(&mut anm, &input(input::RIGHT));
        assert!((p.pos[0] - (x + p.sht.speed)).abs() < 1e-5);
        let x = p.pos[0];
        p.step(&mut anm, &input(input::RIGHT | input::FOCUS));
        assert!((p.pos[0] - (x + p.sht_youkai.speed_focus)).abs() < 1e-5);
        for _ in 0..300 {
            p.step(&mut anm, &input(input::FOCUS | input::DOWN | input::RIGHT));
        }
        assert_eq!((p.pos[0], p.pos[1], p.focus, p.mode), (376.0, 432.0, 1, 1));
        for _ in 0..300 {
            p.step(&mut anm, &input(input::UP | input::LEFT));
        }
        assert_eq!((p.pos[0], p.pos[1], p.mode), (8.0, 16.0, 0));
        p.speed_mult = [0.5, 0.5];
        p.step(&mut anm, &input(input::DOWN));
        assert!((p.pos[1] - (16.0 + p.sht.speed * 0.5)).abs() < 1e-5);
    }

    #[test]
    fn mode_after_six_frames() {
        let (mut p, mut anm) = ready();
        for k in 1..=8 {
            p.step(&mut anm, &input(input::FOCUS));
            assert_eq!(p.mode, (k > 7) as u8, "frame {k}");
        }
    }

    #[test]
    fn collisions() {
        let (mut p, mut anm) = ready();
        p.step(&mut anm, &input(0));
        let mut rng = Rng::default();
        let env = GrazeEnv::default();
        let q = p.pos;
        assert_eq!(
            p.graze_test([q[0] + 25.0, q[1], 0.0], [4.0, 4.0], &env, &mut rng)
                .0,
            0
        );
        assert_eq!(
            p.graze_test([q[0] + 20.0, q[1], 0.0], [4.0, 4.0], &env, &mut rng)
                .0,
            1
        );
        assert_eq!(p.hit_test([q[0] + 2.0, q[1], 0.0], [4.0, 4.0], &mut rng), 1);
        assert_eq!((p.state, p.hits), (2, 1));
        p.cancel_rect([q[0], q[1]], [100.0, 100.0], 9, 10);
        assert_eq!(p.hit_test([q[0] + 2.0, q[1], 0.0], [4.0, 4.0], &mut rng), 2);
        assert_eq!(p.cancel_item, 9);
        // A bomb-area miss keeps the last item; the hit test resets it to 6 first (08 §4.1).
        let far = [q[0] + 200.0, q[1], 0.0];
        assert!(!p.in_bomb_area(far));
        assert_eq!(p.cancel_item, 9);
        assert_eq!(p.hit_test(far, [4.0, 4.0], &mut rng), 0);
        assert_eq!(p.cancel_item, 6);
    }

    // 09 §6.4: Player_Load start (192, 384, 0.49); the respawn point uses z 0.2.
    #[test]
    fn start_and_respawn_z() {
        let p = Player::default();
        assert_eq!(p.pos, [192.0, 384.0, 0.49]);
        assert_eq!(RESPAWN, [192.0, 384.0, 0.2]);
    }

    #[test]
    fn die_requests_drop_attracted() {
        let (mut p, _) = ready();
        let mut rng = Rng::default();
        p.die(&mut rng);
        assert!(p.drop_attracted);
        let (mut p, _) = ready();
        gamemgr::set_flags(F_LAST_SPELL);
        p.die(&mut rng);
        assert!(p.drop_attracted);
    }

    // 09 §6.1/§6.2: with a bomb in stock the hit sets the white colour2 silhouette; the dying frames keep the tint,
    // the real death turns both off.
    #[test]
    fn die_silhouette() {
        let (mut p, mut anm) = ready();
        let mut rng = Rng::default();
        p.stats.bombs = 1.0;
        p.vm.color = [0x20, 0x40, 0x60, 0x80];
        p.die(&mut rng);
        assert!(p.vm.flags & anmvm::F_COLOR2 != 0);
        assert_eq!(p.vm.color2, [0xFF, 0xFF, 0xFF, 0x80]);
        anm.tint_on = true;
        p.deathbomb = 2;
        p.dying_update(&mut anm, &mut None);
        assert!(anm.tint_on && p.vm.flags & anmvm::F_COLOR2 != 0);
        p.dying_update(&mut anm, &mut None);
        assert!(!anm.tint_on && p.vm.flags & anmvm::F_COLOR2 == 0);
        let (mut p, _) = ready();
        p.stats.bombs = 0.0;
        p.die(&mut rng);
        assert!(p.vm.flags & anmvm::F_COLOR2 == 0);
    }

    // 09 §9.2: the third time orb only inside the bullet-and-card branch.
    #[test]
    fn graze_orbs() {
        let (mut p, _) = ready();
        let mut rng = Rng::default();
        let env = |spell| GrazeEnv {
            boss: true,
            spell,
            dialogue: false,
        };
        let mut orbs = |p: &mut Player, laser, spell| {
            p.stats.gauge = 9000;
            p.on_graze([0.0; 3], laser, &env(spell), &mut rng)
        };
        assert_eq!(orbs(&mut p, false, false), 1);
        assert_eq!(orbs(&mut p, true, true), 1);
        assert_eq!(orbs(&mut p, false, true), 3);
        p.shot_type = 5;
        assert_eq!(orbs(&mut p, false, true), 2);
    }

    #[test]
    fn deathbomb_window_and_death() {
        let (mut p, mut anm) = ready();
        p.stats.bombs = 3.0;
        p.stats.lives = 2.0;
        p.stats.power = 40.0;
        p.stats.time = 1000;
        p.stats.time_threshold = 5000;
        p.stats.d0b0 = 7;
        let mut rng = Rng::default();
        p.die(&mut rng);
        assert_eq!((p.state, p.deathbomb), (2, 15 * 9 / 5));
        p.shot_type = 2;
        p.state = 0;
        p.die(&mut rng);
        assert_eq!(p.deathbomb, 15);
        for k in 0..15 {
            // 09 §6.2: 0x164D0B0 is kept through the window and reset at the real death.
            assert_eq!(p.stats.d0b0, 7, "frame {k}");
            p.step(&mut anm, &input(0));
            assert_eq!(p.state, 2, "frame {k}");
        }
        assert_eq!(p.stats.d0b0, 0);
        assert_eq!(
            (p.deaths, p.stats.power, p.stats.rank_delta),
            (1, 24.0, -1600)
        );
        assert_eq!(p.stats.time, 1000 - 15 * 15 - (1000 - 225) / 10);
        for _ in 0..31 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!(
            (p.state, p.stats.lives, p.stats.bombs, p.pos),
            (1, 1.0, 3.0, RESPAWN)
        );
        for _ in 0..31 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!((p.state, p.deathbomb), (3, p.sht.deathbomb));
        for _ in 0..240 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!(p.state, 0);
    }

    // 09 §6.1: card doubling without a boss test; GameFlags bit 10 only with bombs ≥ 1.
    #[test]
    fn spell_death_freeze_flag() {
        let mut rng = Rng::default();
        let (mut p, _) = ready();
        p.shot_type = 2;
        p.env.spell = true;
        p.stats.time_threshold = 5000;
        gamemgr::set_flags(0);
        p.stats.bombs = 0.0;
        p.die(&mut rng);
        assert_eq!((p.deathbomb, gamemgr::flags()), (2, 0));
        p.state = 0;
        p.stats.bombs = 1.0;
        p.die(&mut rng);
        assert_eq!((p.deathbomb, gamemgr::flags()), (12, gamemgr::F_DEATH));
        assert!(p.freeze_bullets());
        gamemgr::set_flags(0);
    }

    #[test]
    fn freeze_byte_pauses_auras_and_skips_calc() {
        crate::effect::clear();
        let (mut p, mut anm) = ready();
        let aura = crate::effect::FIXED_BASE + crate::effect::FIX_GAUGE;
        crate::effect::spawn_fixed(&mut anm, 25, p.pos, crate::effect::FIX_GAUGE, 0xFFFFFFFF);
        let paused = || crate::effect::peek(aura, |e| e.vm.flags & anmvm::F_PAUSED != 0);
        crate::enemy::set_timer_pause(1);
        let (pos, t) = (p.pos, p.state_timer.cur);
        p.step(&mut anm, &input(input::LEFT));
        assert!(paused());
        assert_eq!((p.pos, p.state_timer.cur, p.frames), (pos, t, 0));
        crate::enemy::set_timer_pause(0);
        p.step(&mut anm, &input(input::LEFT));
        assert!(!paused() && p.pos[0] < pos[0] && p.frames == 1);
        crate::effect::clear();
    }

    #[test]
    fn spell_practice_timers() {
        let (mut p, mut anm) = ready();
        gamemgr::set_flags(F_SPELL_PRACTICE);
        p.load(&mut anm, Sht::default(), Sht::default(), 0);
        assert_eq!((p.state, p.state_timer.cur), (1, 10));
        let (mut q, _) = ready();
        gamemgr::set_flags(0);
        q.load(&mut anm, Sht::default(), Sht::default(), 0);
        assert_eq!(q.state_timer.cur, 120);
        gamemgr::set_flags(F_SPELL_PRACTICE);
        // Respawn: state 3 at t = 30; the timer keeps 30 instead of 240.
        p.state_timer.set(0);
        for _ in 0..31 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!((p.state, p.state_timer.cur), (3, 29));
        for _ in 0..29 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!(p.state, 0);
    }

    #[test]
    fn death_ring_lifecycle() {
        let ring = crate::effect::FIXED_BASE + crate::effect::FIX_DEATH;
        let active = || crate::effect::peek(ring, |e| e.active);
        crate::effect::clear();
        let (mut p, mut anm) = ready();
        p.stats.bombs = 1.0;
        let mut rng = Rng::default();
        p.die(&mut rng);
        let n = p.deathbomb;
        for _ in 0..n - 1 {
            p.step(&mut anm, &input(0));
            assert!(active());
        }
        p.step(&mut anm, &input(0));
        assert!(!active() && p.deaths == 1);
        crate::effect::clear();
        let (mut p, _) = ready();
        p.stats.bombs = 0.0;
        p.die(&mut rng);
        assert!(!active());
    }

    #[test]
    fn deathbomb_saves() {
        let (mut p, mut anm) = ready();
        p.shot_type = 2;
        p.deathbomb = 10;
        p.stats.bombs = 2.0;
        let mut rng = Rng::default();
        p.die(&mut rng);
        p.step(&mut anm, &input(0));
        p.step(&mut anm, &input(input::BOMB));
        assert_eq!(
            (p.state, p.bombing, p.bomb.index, p.stats.bombs, p.bomb.used),
            (3, true, 3, 0.0, 2)
        );
        assert_eq!(p.deaths, 0);
    }

    #[test]
    fn power_and_lives_rules() {
        let (mut p, mut anm) = ready();
        p.stats.lives = 0.0;
        p.stats.power = 100.0;
        let mut rng = Rng::default();
        p.stats.bombs = 0.0;
        p.die(&mut rng);
        assert_eq!(p.deathbomb, 2);
        for _ in 0..40 {
            p.step(&mut anm, &input(0));
        }
        // 09 §6.2: lives and bombs unchanged on game over.
        assert_eq!(
            (p.stats.power, p.game_over, p.stats.lives, p.stats.bombs),
            (0.0, true, 0.0, 0.0)
        );
        let (mut p, mut anm) = ready();
        p.stats.power = 10.0;
        p.die(&mut rng);
        for _ in 0..40 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!(p.stats.power, 0.0);
        let mut s = Stats {
            lives: 8.0,
            bombs: 7.0,
            ..Stats::default()
        };
        s.extend(&mut rng);
        assert_eq!((s.lives, s.bombs), (8.0, 8.0));
        let before = rng.counter;
        s.add_time(-5, &mut rng);
        assert_eq!((s.time, rng.counter), (0, before));
    }

    #[test]
    fn gauge_limits() {
        let mut s = Stats {
            limits: GaugeLimits::for_shot(4),
            ..Stats::default()
        };
        s.gauge_add(5000);
        assert_eq!(s.gauge, 2000);
        assert!(!s.youkai());
        s.limits = GaugeLimits::for_shot(10);
        s.gauge_add(-9000);
        assert_eq!(s.gauge, -5000);
        assert!(s.human_max());
        assert_eq!(parse_shot("yuyuko"), Some(11));
        assert_eq!(parse_shot("ma"), Some(1));
    }
}
