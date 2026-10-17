// Player (09): movement, boxes, states, death/deathbomb/respawn, gauge, stats; shots in pshot.rs, bombs in bomb.rs.
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::BulletManager;
use crate::game::Ctx;
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

pub const CALC_PRIO: u16 = 9;
pub const DRAW_PRIO: u16 = 9;
pub const DRAW_PRIO_HIGH: u16 = 10;
pub const ANM_SLOT: usize = 5;
pub const RESPAWN: [f32; 3] = [192.0, 384.0, 0.0];
pub const START: [f32; 3] = RESPAWN;
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
        Self { min: -10000, max: 10000, human_max: -8000, youkai_max: 8000, human: -2000, youkai: 2000 }
    }
}

impl GaugeLimits {
    pub fn for_shot(shot: u8) -> Self {
        let d = Self::default();
        match shot {
            3 => Self { min: -5000, human_max: -3000, ..d },
            10 => Self { min: -5000, max: 5000, human_max: -3000, youkai_max: 3000, ..d },
            s if s >= 4 && s % 2 == 0 => Self { max: 2000, youkai: 2001, ..d },
            s if s >= 4 => Self { min: -2000, human: -2001, ..d },
            _ => d,
        }
    }
}

// Game-manager stats block (09 §9.1, 11 §5).
#[derive(Clone, Debug)]
pub struct Stats {
    pub difficulty: i32,
    pub score10: i64,
    pub graze: i32,
    pub graze_total: i32,
    pub gauge: i16,
    pub limits: GaugeLimits,
    pub max_point: i32,
    pub point_items: i32,
    pub point_items2: i32,
    pub extends: i32,
    pub next_extend: i32,
    pub time: i32,
    pub time_threshold: i32,
    pub time_added: i32,
    pub lives: f32,
    pub bombs: f32,
    pub power: f32,
    pub deaths: i32,
    pub bombs_used: i32,
    pub rank_delta: i32,
    pub stage_frames: u32,
    pub human_frames: u32,
    pub youkai_frames: u32,
    pub spell_bonus_add: i32,
    pub point_counter: i32,
    pub score_disp: i64,
    pub score_step: i64,
    pub hiscore: i64,
    pub hiscore_cont: u8,
    pub clock: i32,
    pub continues: u8,
    pub spells: i32,
    pub gauge_start: i16,
    pub popups: Vec<Popup>,
}

// HUD popup requests (12 §5.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Popup {
    Bonus(i32),
    Message(u8, i32),
    SpellBonus(i32),
}

impl Default for Stats {
    // Starting lives come from the config at game start (gamemgr); 2 is the config default.
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
            spell_bonus_add: 0,
            point_counter: 0,
            score_disp: 0,
            score_step: 0,
            hiscore: 100000,
            hiscore_cont: 0,
            clock: 0,
            continues: 0,
            spells: 0,
            gauge_start: 0,
            popups: Vec::new(),
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
    pub fn add_score(&mut self, v: i32) {
        self.score10 = (self.score10 + (v / 10) as i64).min(999_999_999);
    }

    pub fn score(&self) -> i64 {
        self.score10 * 10
    }

    // Rank deltas, applied by the game manager through its sub-counter (11 §3.2).
    pub fn rank(&mut self, d: i32) {
        self.rank_delta += d;
    }

    // GameMgr_UpdateChecksum: 2 × Rng_Range(100000) (11 §1.5).
    pub fn checksum(rng: &mut Rng) {
        rng.range(100000);
        rng.range(100000);
    }

    // GameMgr_ScrambleDecoys: 5 × Range + 3 × Float.
    pub fn scramble(rng: &mut Rng) {
        for _ in 0..5 {
            rng.range(100000);
        }
        for _ in 0..3 {
            rng.next_f32();
        }
    }

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

    pub fn gauge_add(&mut self, n: i32) {
        self.gauge = (self.gauge as i32 + n).clamp(self.limits.min, self.limits.max) as i16;
    }

    // 08 §8.5 1up rule (Extend 0x439B29).
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

    // AddTime 0x418220: clamps at 0 (no checksum when clamped); n > 0 also raises the max point value.
    pub fn add_time(&mut self, n: i32, rng: &mut Rng) {
        if self.time + n < 0 {
            self.time = 0;
            return;
        }
        self.time += n;
        if n > 0 {
            self.time_added += n;
            self.max_point += (n + (self.time_added & 1)) / 2 * 10;
        }
        Self::checksum(rng);
    }
}

fn overlap(a: [f32; 2], ah: [f32; 2], b: [f32; 2], bh: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() <= ah[0] + bh[0] && (a[1] - b[1]).abs() <= ah[1] + bh[1]
}

// Facts the player needs from other managers (bosses, spell card, dialogue).
#[derive(Clone, Copy, Debug, Default)]
pub struct GrazeEnv {
    pub boss: bool,
    pub spell: bool,
    pub dialogue: bool,
}

pub struct Player {
    pub state: u8,
    pub focus: u8,
    pub last_spell: u8,
    pub mode: u8,
    pub forced_bomb: u8,
    pub focus_age: i32,
    pub gauge_aura: bool,
    pub vm: AnmVm,
    pub pos: [f32; 3],
    pub trail: [[f32; 3]; 16],
    pub last_disp: [f32; 2],
    pub speed_mult: [f32; 2],
    pub options: [Opt; 4],
    pub bomb: Bomb,
    pub bombing: bool,
    pub bomb_active: bool,
    pub damage_areas: Vec<Area>,
    pub cancel_areas: Vec<Area>,
    pub bullets: Vec<PBullet>,
    pub laser_slots: [crate::pshot::LaserSlot; crate::pshot::LASER_SLOTS],
    pub deathbomb: i32,
    pub bomb_cooldown: i32,
    pub respawn_clear: i32,
    pub sht: Sht,
    pub sht_youkai: Sht,
    pub cancel_item: i32,
    pub direction: u8,
    pub last_v: [f32; 2],
    pub targets: [[f32; 3]; 2],
    pub aimed: Option<[f32; 3]>,
    pub boss_target: bool,
    pub shot_timer: Timer,
    pub gauge_b: Timer,
    pub gauge_c: Timer,
    pub state_timer: Timer,
    pub option_angle: f32,
    pub orb_step: i32,
    pub shot_type: u8,
    pub stats: Stats,
    pub env: GrazeEnv,
    pub frames: i32,
    pub game_over: bool,
    pub freeze_bullets: bool,
    pub hits: u32,
    pub spark_counter: u8,
    pub invuln_ring: bool,
    pub grazes: u32,
    pub deaths: u32,
    pub bombs_started: u32,
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
            direction: 0,
            last_v: [0.0; 2],
            targets: [[-999.0, -999.0, 0.0]; 2],
            aimed: None,
            boss_target: false,
            shot_timer: Timer { cur: -1, ..Timer::default() },
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
            freeze_bullets: false,
            hits: 0,
            spark_counter: 0,
            invuln_ring: false,
            grazes: 0,
            deaths: 0,
            bombs_started: 0,
            debug_move: false,
            debug_invuln: crate::env::var("TH08_INVULN").is_some_and(|v| v != "0"),
            world: Weak::new(),
        }
    }
}

// TH08_PLAYER: shot type 0–11 or a name (09 §2).
pub fn parse_shot(s: &str) -> Option<u8> {
    const NAMES: [&str; 12] = ["reimu&yukari", "marisa&alice", "sakuya&remilia", "youmu&yuyuko", "reimu", "yukari", "marisa", "alice", "sakuya", "remilia", "youmu", "yuyuko"];
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

    // Player_Load: sht files, gauge limits, initial script, options for solos, state 1 with timer 120.
    pub fn load(&mut self, anm: &mut AnmManager, human: Sht, youkai: Sht, shot: u8) {
        self.shot_type = shot;
        self.sht = human;
        self.sht_youkai = youkai;
        self.stats.limits = GaugeLimits::for_shot(shot);
        self.orb_step = if self.solo_human() { 27 } else { 40 };
        let script = if self.solo_youkai() { 5 } else { 0 };
        anm.set_script(&mut self.vm, ANM_SLOT, script);
        self.state = 1;
        self.state_timer.set(120);
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
        p.borrow_mut().load(&mut ctx.anm, human, youkai, shot);
        let t: task::TaskRef<Ctx> = p;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO_HIGH, 1, t)?;
        Ok(())
    }

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

    // Item fall multiplier source (08 §8.4).
    pub fn item_sht(&self) -> &Sht {
        if self.focus == 1 { &self.sht_youkai } else { &self.sht }
    }

    pub fn angle_from(&self, o: [f32; 3]) -> f32 {
        crate::bullet::angle_from(o, self.pos)
    }

    // Player_InBombArea (09 §8): resets the cancel item type, then tests every active cancel area.
    pub fn in_bomb_area(&mut self, p: [f32; 3]) -> bool {
        self.cancel_item = 6;
        for a in self.cancel_areas.iter_mut().filter(|a| a.active) {
            if a.contains([p[0], p[1]], [0.0, 0.0]) {
                self.cancel_item = a.item;
                a.total += 1;
                return true;
            }
        }
        false
    }

    // Player_GrazeTest: 0 none, 1 grazed (handler ran), 2 inside a bomb area; plus time orbs to drop.
    pub fn graze_test(&mut self, p: [f32; 3], size: [f32; 2], env: &GrazeEnv, rng: &mut Rng) -> (u8, u8) {
        if self.in_bomb_area(p) {
            return (2, 0);
        }
        let half = [size[0] / 2.0 + 20.0, size[1] / 2.0 + 20.0];
        if matches!(self.state, 1 | 2) || !overlap([p[0], p[1]], half, self.xy(), self.graze_half()) {
            return (0, 0);
        }
        (1, self.on_graze(p, false, env, rng))
    }

    // Player_HitTest: 0 miss, 1 hit, 2 inside a bomb area.
    pub fn hit_test(&mut self, p: [f32; 3], size: [f32; 2], rng: &mut Rng) -> u8 {
        if self.in_bomb_area(p) {
            return 2;
        }
        if !overlap([p[0], p[1]], [size[0] / 2.0, size[1] / 2.0], self.xy(), self.hit_half()) {
            return 0;
        }
        if self.state == 0 {
            self.die(rng);
        }
        1
    }

    // Hit path of bullets/lasers/enemies: ScrambleDecoys, then Player_Die (09 §6.1).
    pub fn die(&mut self, rng: &mut Rng) {
        Stats::scramble(rng);
        self.hits += 1;
        if self.state != 0 || self.debug_invuln {
            return;
        }
        Stats::checksum(rng);
        crate::sound::play_at(crate::sound::PLDEAD, self.pos[0]);
        crate::effect::spawn_rng(rng, 6, self.pos, 16, 0xFFFFFFFF);
        self.state = 2;
        self.state_timer.set(0);
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
            if self.env.spell && self.env.boss {
                n = (n * 2).min(30);
            }
            if matches!(self.shot_type, 0 | 4 | 5) {
                n = n * 9 / 5;
            }
            n
        };
        // Ring 59 only with a bomb in stock (and outside the dissolve mode, not in the port yet).
        if st.bombs >= 1.0 {
            crate::effect::death_ring(rng, self.pos, n);
        }
        if self.env.spell {
            self.freeze_bullets = true;
        }
        self.deathbomb = n;
    }

    // Player_OnGraze (09 §9.2); returns how many time orbs (item type 10) to drop at `p`.
    pub fn on_graze(&mut self, p: [f32; 3], laser: bool, env: &GrazeEnv, _rng: &mut Rng) -> u8 {
        self.grazes += 1;
        crate::sound::play_at(crate::sound::GRAZE, p[0]);
        crate::effect::spawn_rng(_rng, 8, [(p[0] + self.pos[0]) / 2.0, (p[1] + self.pos[1]) / 2.0, 0.0], 1, 0xFFFFFFFF);
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
        s.add_score(if s.youkai_max() { 4000 } else { 2000 });
        if self.mode != 0 && !self.bombing {
            s.gauge_add(100);
        }
        let mut orbs = 0;
        if (!solo_human || self.shot_type == 10) && env.boss && s.youkai_max() {
            orbs = 1;
            if !laser && env.spell {
                orbs += 1;
            }
            if !solo_youkai {
                orbs += 1;
            }
        }
        orbs
    }

    // Player_ItemCollectTest: item box (c, c) vs the collect box.
    pub fn collect_test(&self, p: [f32; 3], c: f32) -> bool {
        matches!(self.state, 0 | 3 | 4) && overlap([p[0], p[1]], [c / 2.0; 2], self.xy(), self.item_half())
    }

    pub fn box_in_cancel_area(&self, p: [f32; 3], half: [f32; 2]) -> bool {
        self.cancel_areas.iter().any(|a| a.active && a.contains([p[0], p[1]], half))
    }

    // EnemyMgr_Calc target pick (09 §2.1): target 1 = boss (horizontally nearest) else the lowest enemy;
    // aimed = smallest y within 64 px horizontally, not a chained child; target 2 is never written.
    pub fn offer_target(&mut self, e: [f32; 3], boss: bool, child: bool) {
        let t = &mut self.targets[0];
        if boss {
            if !self.boss_target || (e[0] - self.pos[0]).abs() < (t[0] - self.pos[0]).abs() {
                *t = e;
            }
            self.boss_target = true;
        } else if !self.boss_target && e[1] > t[1] {
            *t = e;
        }
        if !child && (e[0] - self.pos[0]).abs() < 64.0 && self.aimed.is_none_or(|a| e[1] < a[1]) {
            self.aimed = Some(e);
        }
    }

    // Player_HitTestNoBomb 0x44A360: true on overlap in every player state; only state 0 dies.
    pub fn contact_test(&mut self, p: [f32; 3], size: [f32; 2], rng: &mut Rng) -> bool {
        if !overlap([p[0], p[1]], [size[0] / 2.0, size[1] / 2.0], self.xy(), self.hit_half()) {
            return false;
        }
        if self.state == 0 {
            self.die(rng);
        }
        true
    }

    pub(crate) fn set_script(&mut self, anm: &mut AnmManager, s: usize) {
        let pos = self.vm.pos;
        anm.set_script(&mut self.vm, ANM_SLOT, s);
        self.vm.pos = pos;
    }

    fn sprite_base(&self, focus: u8) -> usize {
        let youkai = if self.solo() { self.shot_type % 2 == 1 } else { focus == 1 };
        if youkai { 5 } else { 0 }
    }

    // Player_Move (09 §3).
    fn move_player(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        let gs = anm.speed;
        let bits = inp.held;
        let (u, d, l, r) = (bits & input::UP != 0, bits & input::DOWN != 0, bits & input::LEFT != 0, bits & input::RIGHT != 0);
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
        let f = if self.bombing { (self.bomb.index & 1) as u8 } else { (bits & input::FOCUS != 0) as u8 };
        if f != self.focus {
            self.focus = f;
            if !self.solo() {
                self.swap_options(anm, f);
                self.set_script(anm, self.sprite_base(f));
            }
            self.focus_age = 0;
            self.gauge_c.set(0);
            if f == 0 {
                crate::effect::get(crate::effect::FIXED_BASE + crate::effect::FIX_AURA, |e| e.vm.pending_interrupt = 1);
            }
        } else {
            self.focus_age += 1;
        }
        if self.focus_age == 4 {
            if !self.solo() {
                let (kind, colour) = if f == 1 { (29, 0x80FF8080) } else { (28, 0x808080FF) };
                crate::effect::spawn(anm, kind, self.pos, 1, colour);
            }
            // Hitbox marker: effect 22 (etama script 54, sprite 218) in fixed slot 2 (answers_phase5 impl7).
            if f == 1 {
                crate::effect::spawn_fixed(anm, 22, self.pos, crate::effect::FIX_AURA, 0xFFFFFFFF);
            }
        }
        if self.focus_age > 6 {
            self.mode = f;
        }
        if self.solo() {
            self.mode = self.shot_type % 2;
        }
        let (ortho, diag) = if f == 1 { (self.sht_youkai.speed_focus, self.sht_youkai.diag_focus) } else { (self.sht.speed, self.sht.diag) };
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

    // Gauge per frame (09 §9.3).
    fn gauge_drift(&mut self, gs: f32) {
        if self.focus_age < 30 || self.bombing {
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

    // Player_DyingUpdate (09 §6.2); true when the respawn starts.
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
        self.last_v = [0.0; 2];
        self.respawn_clear = 60;
        if self.stats.lives >= 1.0 {
            self.stats.add_lives(-1.0, &mut anm.rng);
            self.stats.bombs = self.sht.bombs.trunc();
        } else {
            // Game over: the ASCII manager opens the continue menu (12 §4).
            self.game_over = true;
            self.stats.bombs = self.sht.bombs.trunc();
        }
        true
    }

    // The death completes when the deathbomb window runs out.
    fn real_death(&mut self, anm: &mut AnmManager, items: &mut Option<&mut ItemManager>) {
        self.deaths += 1;
        log::info!("player death at frame {}", self.frames);
        crate::sound::play_at(crate::sound::TAN00_LOUD, self.pos[0]);
        crate::effect::free(crate::effect::FIXED_BASE + crate::effect::FIX_DEATH);
        crate::effect::spawn(anm, 6, self.pos, 16, 0xFFFFFFFF);
        crate::effect::spawn_fixed(anm, 12, self.pos, crate::effect::FIX_DEATH_BURST, 0xFF4040FF);
        self.freeze_bullets = false;
        self.last_spell = 0;
        self.state_timer.set(0);
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
            self.stats.set_power(if p <= 16.0 { 0.0 } else { p - 16.0 }, rng);
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
        }
        self.stats.rank(-1600);
    }

    // Player_RespawnUpdate (09 §6.3).
    fn respawn_update(&mut self) {
        if self.respawn_clear > 0 {
            self.respawn_clear -= 1;
            self.cancel_rect([self.pos[0], self.pos[1]], [768.0, 896.0], -1, 0);
        }
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
        self.state_timer.set(240);
        self.deathbomb = self.sht.deathbomb;
    }

    // Player_UpdateInvuln (09 §6.4).
    fn update_invuln(&mut self, gs: f32) {
        if self.state == 3 {
            self.state_timer.dec(gs);
            let t = self.state_timer.cur;
            self.vm.color = if t.rem_euclid(8) < 2 { [0x20, 0x20, 0xF0, 0xFF] } else { [0xFF; 4] };
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

    // Player_Calc (09 §1); `items` is the item manager when the bullet manager exists.
    pub fn frame(&mut self, anm: &mut AnmManager, inp: &input::Input, mut items: Option<&mut ItemManager>) {
        let gs = anm.speed;
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
        if !self.freeze_bullets {
            self.update_bullets(anm);
        }
        self.update_shot_timer(anm, inp);
        self.targets = [[-999.0, -999.0, 0.0]; 2];
        self.aimed = None;
        self.boss_target = false;
        if self.bomb_cooldown > 0 {
            self.bomb_cooldown -= 1;
        }
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
            crate::effect::get(crate::effect::FIXED_BASE + crate::effect::FIX_GAUGE, |e| e.pos = p);
        }
        self.frames += 1;
    }

    // Standalone step without a bullet manager (tests).
    pub fn step(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        self.frame(anm, inp, None);
    }

    fn draw_sprite(&mut self, anm: &mut AnmManager, gfx: &mut crate::gfx::Gfx) {
        if self.vm.script.is_none() {
            return;
        }
        if self.game_over {
            return;
        }
        self.vm.pos = [self.pos[0] + 32.0, self.pos[1] + 16.0, 0.0];
        anm.draw_mode_z(gfx, &mut self.vm, 0, 0.1);
    }
}

impl Task<Ctx> for Player {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let inp = ctx.input;
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
        crate::effect::set_frozen(self.freeze_bullets);
        Ret::Next
    }

    // Draw 9 (0x44D530): flying bullets, bomb, sprite, options; draw 10 (0x44D630): bullets in their hit animation.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        if tag == 0 {
            self.draw_bullets(anm, gfx, 1);
            self.draw_bomb(anm, gfx);
            self.draw_sprite(anm, gfx);
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

    fn ready() -> (Player, AnmManager) {
        let mut p = Player { state: 0, focus: 0, ..Player::default() };
        p.stats.limits = GaugeLimits::default();
        (p, AnmManager::default())
    }

    fn input(bits: u16) -> input::Input {
        let mut i = input::Input { raw: bits, ..Default::default() };
        i.tick();
        i
    }

    #[test]
    fn movement_speeds_and_bounds() {
        let (mut p, mut anm) = ready();
        p.step(&mut anm, &input(input::LEFT | input::UP));
        assert!((p.pos[0] - (192.0 - p.sht.diag)).abs() < 1e-5 && (p.pos[1] - (384.0 - p.sht.diag)).abs() < 1e-5);
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
        assert_eq!(p.graze_test([q[0] + 25.0, q[1], 0.0], [4.0, 4.0], &env, &mut rng).0, 0);
        assert_eq!(p.graze_test([q[0] + 20.0, q[1], 0.0], [4.0, 4.0], &env, &mut rng).0, 1);
        assert_eq!(p.hit_test([q[0] + 2.0, q[1], 0.0], [4.0, 4.0], &mut rng), 1);
        assert_eq!((p.state, p.hits), (2, 1));
        p.cancel_rect([q[0], q[1]], [100.0, 100.0], 9, 10);
        assert_eq!(p.hit_test([q[0] + 2.0, q[1], 0.0], [4.0, 4.0], &mut rng), 2);
        assert_eq!(p.cancel_item, 9);
    }

    #[test]
    fn deathbomb_window_and_death() {
        let (mut p, mut anm) = ready();
        p.stats.bombs = 3.0;
        p.stats.lives = 2.0;
        p.stats.power = 40.0;
        p.stats.time = 1000;
        p.stats.time_threshold = 5000;
        let mut rng = Rng::default();
        p.die(&mut rng);
        assert_eq!((p.state, p.deathbomb), (2, 15 * 9 / 5));
        p.shot_type = 2;
        p.state = 0;
        p.die(&mut rng);
        assert_eq!(p.deathbomb, 15);
        for k in 0..15 {
            p.step(&mut anm, &input(0));
            assert_eq!(p.state, 2, "frame {k}");
        }
        assert_eq!((p.deaths, p.stats.power, p.stats.rank_delta), (1, 24.0, -1600));
        assert_eq!(p.stats.time, 1000 - 15 * 15 - (1000 - 225) / 10);
        for _ in 0..31 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!((p.state, p.stats.lives, p.stats.bombs, p.pos), (1, 1.0, 3.0, RESPAWN));
        for _ in 0..31 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!((p.state, p.deathbomb), (3, p.sht.deathbomb));
        for _ in 0..240 {
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
        assert_eq!((p.state, p.bombing, p.bomb.index, p.stats.bombs, p.bomb.used), (3, true, 3, 0.0, 2));
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
        assert_eq!((p.stats.power, p.game_over), (0.0, true));
        let (mut p, mut anm) = ready();
        p.stats.power = 10.0;
        p.die(&mut rng);
        for _ in 0..40 {
            p.step(&mut anm, &input(0));
        }
        assert_eq!(p.stats.power, 0.0);
        let mut s = Stats { lives: 8.0, bombs: 7.0, ..Stats::default() };
        s.extend(&mut rng);
        assert_eq!((s.lives, s.bombs), (8.0, 8.0));
        let before = rng.counter;
        s.add_time(-5, &mut rng);
        assert_eq!((s.time, rng.counter), (0, before));
    }

    #[test]
    fn gauge_limits() {
        let mut s = Stats { limits: GaugeLimits::for_shot(4), ..Stats::default() };
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
