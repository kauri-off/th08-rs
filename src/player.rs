// SPEC-GAP: stub player until spec 09 is implemented: debug movement (arrows, shift = focus), no shots/bombs/death
// sequence. It provides the narrow interface bullets/lasers/items call (graze, hit, bomb areas, collect, stats).
use crate::anmmgr::AnmManager;
use crate::anmvm::{AnmVm, Timer};
use crate::game::Ctx;
use crate::gfx::PLAYFIELD;
use crate::input;
use crate::rng::Rng;
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

pub const CALC_PRIO: u16 = 9;
pub const DRAW_PRIO: u16 = 9;
pub const ANM_SLOT: usize = 5;
pub const START: [f32; 3] = [192.0, 384.0, 0.0];

pub type PlayerRef = Rc<RefCell<Player>>;

// .sht header fields used outside the player (09 §2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sht {
    pub hitbox: f32,
    pub graze: f32,
    pub attract_speed: f32,
    pub collect: f32,
    pub poc_line: f32,
    pub speed: f32,
    pub speed_focus: f32,
    pub diag: f32,
    pub diag_focus: f32,
    pub fall: f32,
}

impl Default for Sht {
    fn default() -> Self {
        Self { hitbox: 1.65, graze: 2.8, attract_speed: 10.0, collect: 24.0, poc_line: 128.0, speed: 4.0, speed_focus: 2.0, diag: 4.0 / 2f32.sqrt(), diag_focus: 2.0 / 2f32.sqrt(), fall: 0.9 }
    }
}

pub fn parse_sht(b: &[u8]) -> Option<Sht> {
    let f = |o: usize| Some(f32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
    Some(Sht {
        hitbox: f(0xC)?,
        graze: f(0x10)?,
        attract_speed: f(0x14)?,
        collect: f(0x18)?,
        poc_line: f(0x1C)?,
        speed: f(0x24)?,
        speed_focus: f(0x28)?,
        diag: f(0x2C)?,
        diag_focus: f(0x30)?,
        fall: f(0x34)?,
    })
}

// Bomb cancel area (09 §8); only the bullet-side test is needed here.
#[derive(Clone, Copy, Debug, Default)]
pub struct Area {
    pub pos: [f32; 2],
    pub r: f32,
    pub size: [f32; 2],
    pub rot: f32,
    pub item: i32,
    pub hits: i32,
}

// Game-manager stats block (09 §9.1; owner moves to the game manager with spec 11).
#[derive(Clone, Debug)]
pub struct Stats {
    pub difficulty: i32,
    pub score10: i64,
    pub graze: i32,
    pub graze_total: i32,
    pub gauge: i16,
    pub max_point: i32,
    pub point_items: i32,
    pub point_items2: i32,
    pub extends: i32,
    pub next_extend: i32,
    pub time: i32,
    pub lives: f32,
    pub bombs: f32,
    pub power: f32,
    pub rank_delta: i32,
    pub spell_bonus_add: i32,
    pub point_counter: i32,
}

impl Default for Stats {
    // SPEC-GAP: starting lives/bombs/max point value belong to spec 11; 2 / 3 / 10000.
    fn default() -> Self {
        Self {
            difficulty: 1,
            score10: 0,
            graze: 0,
            graze_total: 0,
            gauge: 0,
            max_point: 10000,
            point_items: 0,
            point_items2: 0,
            extends: 0,
            next_extend: 100,
            time: 0,
            lives: 2.0,
            bombs: 3.0,
            power: 0.0,
            rank_delta: 0,
            spell_bonus_add: 0,
            point_counter: 0,
        }
    }
}

impl Stats {
    pub fn add_score(&mut self, v: i32) {
        self.score10 += (v / 10) as i64;
    }

    pub fn score(&self) -> i64 {
        self.score10 * 10
    }

    // SPEC-GAP: rank bookkeeping (+1/+3/−3/+200/−1600) is collected here; mapping to ECL rank 0–32 is spec 11.
    pub fn rank(&mut self, d: i32) {
        self.rank_delta += d;
    }

    // GameMgr_UpdateChecksum: two decoy draws on every lives/bombs/power/time change (09 §9.4).
    pub fn checksum(rng: &mut Rng) {
        rng.range(100000);
        rng.range(100000);
    }

    // Gauge thresholds, default column of 09 §9.3.
    pub fn youkai_max(&self) -> bool {
        self.gauge >= 8000
    }

    pub fn human_max(&self) -> bool {
        self.gauge <= -8000
    }

    pub fn human(&self) -> bool {
        self.gauge <= -2000
    }

    pub fn gauge_add(&mut self, n: i32) {
        self.gauge = (self.gauge as i32 + n).clamp(-10000, 10000) as i16;
    }

    // 08 §8.5 1up rule.
    pub fn extend(&mut self, rng: &mut Rng) {
        if self.lives < 8.0 {
            self.lives += 1.0;
            Self::checksum(rng);
            self.rank(200);
        } else if self.bombs < 8.0 {
            self.bombs += 1.0;
            Self::checksum(rng);
            self.rank(200);
        }
    }

    pub fn add_time(&mut self, n: i32, rng: &mut Rng) {
        self.time += n;
        Self::checksum(rng);
    }
}

fn overlap(a: [f32; 2], ah: [f32; 2], b: [f32; 2], bh: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() <= ah[0] + bh[0] && (a[1] - b[1]).abs() <= ah[1] + bh[1]
}

pub struct Player {
    pub pos: [f32; 3],
    pub state: u8,
    pub focus: u8,
    pub mode: u8,
    pub focus_age: i32,
    pub shot_type: u8,
    pub sht: Sht,
    pub sht_youkai: Sht,
    pub bombing: bool,
    pub bomb_active: bool,
    pub cancel_item: i32,
    pub cancel_areas: Vec<Area>,
    pub state_timer: Timer,
    pub stats: Stats,
    pub hits: u32,
    pub grazes: u32,
    pub debug_move: bool,
    vm: AnmVm,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            pos: START,
            state: 0,
            focus: 0,
            mode: 0,
            focus_age: 0,
            shot_type: 0,
            sht: Sht::default(),
            sht_youkai: Sht::default(),
            bombing: false,
            bomb_active: false,
            cancel_item: 6,
            cancel_areas: Vec::new(),
            state_timer: Timer::default(),
            stats: Stats::default(),
            hits: 0,
            grazes: 0,
            debug_move: false,
            vm: AnmVm::default(),
        }
    }
}

impl Player {
    pub fn new_ref() -> PlayerRef {
        Rc::new(RefCell::new(Player::default()))
    }

    pub fn register(ctx: &mut Ctx, p: PlayerRef) -> Result<()> {
        {
            let mut pl = p.borrow_mut();
            for (name, youkai) in [("ply00a.sht", false), ("ply00as.sht", true)] {
                if let Some(s) = ctx.archive.read(name).ok().as_deref().and_then(parse_sht) {
                    if youkai { pl.sht_youkai = s } else { pl.sht = s }
                }
            }
            pl.debug_move = true;
            if ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "player00.anm").is_ok() {
                let mut vm = AnmVm::default();
                ctx.anm.set_script(&mut vm, ANM_SLOT, 0);
                pl.vm = vm;
            }
        }
        let t: task::TaskRef<Ctx> = p;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
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

    fn xy(&self) -> [f32; 2] {
        [self.pos[0], self.pos[1]]
    }

    // Item fall multiplier source (08 §8.4).
    pub fn item_sht(&self) -> Sht {
        if self.focus != 0 { self.sht_youkai } else { self.sht }
    }

    pub fn angle_from(&self, o: [f32; 3]) -> f32 {
        crate::bullet::angle_from(o, self.pos)
    }

    // Player_InBombArea (09 §8): resets the cancel item type, then tests every cancel area.
    pub fn in_bomb_area(&mut self, p: [f32; 3]) -> bool {
        self.cancel_item = 6;
        for a in &mut self.cancel_areas {
            let (dx, dy) = (p[0] - a.pos[0], p[1] - a.pos[1]);
            let inside = if a.r > 0.0 {
                dx * dx + dy * dy < a.r * a.r
            } else {
                let (s, c) = (-a.rot).sin_cos();
                let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
                rx.abs() <= a.size[0] / 2.0 && ry.abs() <= a.size[1] / 2.0
            };
            if inside {
                self.cancel_item = a.item;
                a.hits += 1;
                return true;
            }
        }
        false
    }

    // Player_GrazeTest: 0 none, 1 grazed (handler ran), 2 inside a bomb area.
    // SPEC-GAP: no graze while respawning/dying (states 1/2), as for lasers.
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
    // SPEC-GAP: only state 0 counts as a hit; the stub then turns invulnerable for 120 frames instead of dying.
    pub fn hit_test(&mut self, p: [f32; 3], size: [f32; 2], rng: &mut Rng) -> u8 {
        if self.in_bomb_area(p) {
            return 2;
        }
        if self.state != 0 || !overlap([p[0], p[1]], [size[0] / 2.0, size[1] / 2.0], self.xy(), self.hit_half()) {
            return 0;
        }
        self.die(rng);
        1
    }

    // GameMgr_ScrambleDecoys + Player_Die (stub).
    pub fn die(&mut self, rng: &mut Rng) {
        for _ in 0..5 {
            rng.range(100000);
        }
        for _ in 0..3 {
            rng.next_f32();
        }
        self.hits += 1;
        self.state = 3;
        self.state_timer.set(120);
    }

    // Player_OnGraze (09 §9.2); returns how many time orbs (item type 10) to drop at `p`.
    pub fn on_graze(&mut self, _p: [f32; 3], laser: bool, env: &GrazeEnv, _rng: &mut Rng) -> u8 {
        self.grazes += 1;
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
        let solo_human = self.shot_type >= 4 && self.shot_type.is_multiple_of(2);
        let solo_youkai = self.shot_type >= 4 && self.shot_type % 2 == 1;
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

    // Debug movement with the sht speeds and the 09 §3 clamp.
    fn debug_update(&mut self, inp: &input::Input, gs: f32) {
        let f = inp.down(input::FOCUS) as u8;
        if f != self.focus {
            self.focus = f;
            self.focus_age = 0;
        } else {
            self.focus_age += 1;
        }
        if self.focus_age > 6 {
            self.mode = self.focus;
        }
        let (h, v) = (
            inp.down(input::RIGHT) as i32 - inp.down(input::LEFT) as i32,
            inp.down(input::DOWN) as i32 - inp.down(input::UP) as i32,
        );
        let (ortho, diag) = if f != 0 { (self.sht_youkai.speed_focus, self.sht_youkai.diag_focus) } else { (self.sht.speed, self.sht.diag) };
        let s = if h != 0 && v != 0 { diag } else { ortho };
        self.pos[0] = (self.pos[0] + h as f32 * s * gs).clamp(8.0, 376.0);
        self.pos[1] = (self.pos[1] + v as f32 * s * gs).clamp(16.0, 432.0);
    }

    pub fn step(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        let gs = anm.speed;
        if self.debug_move {
            self.debug_update(inp, gs);
        }
        if self.state == 3 {
            self.state_timer.dec(gs);
            if self.state_timer.cur < 1 {
                self.state = 0;
            }
        }
        if self.vm.script.is_some() {
            self.vm.run(anm);
        }
    }
}

// Facts the graze handler needs from other managers.
#[derive(Clone, Copy, Debug, Default)]
pub struct GrazeEnv {
    pub boss: bool,
    pub spell: bool,
}

impl Task<Ctx> for Player {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let inp = ctx.input;
        self.step(&mut ctx.anm, &inp);
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        let (x, y) = (self.pos[0] + 32.0, self.pos[1] + 16.0);
        if self.vm.script.is_some() {
            self.vm.pos = [x, y, 0.0];
            let blink = self.state == 3 && self.state_timer.cur % 8 < 2;
            self.vm.color = if blink { [0x20, 0x20, 0xF0, 0xFF] } else { [0xFF; 4] };
            anm.draw_mode(gfx, &mut self.vm, 0);
        }
        let h = self.sht.hitbox.max(2.0);
        gfx.rect(x - h, y - h, 2.0 * h, 2.0 * h, [255, 255, 255, 255]);
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_movement_and_boxes() {
        let mut p = Player { debug_move: true, ..Player::default() };
        let mut anm = AnmManager::default();
        let mut inp = input::Input { raw: input::LEFT | input::UP, ..Default::default() };
        inp.tick();
        p.step(&mut anm, &inp);
        assert!((p.pos[0] - (192.0 - p.sht.diag)).abs() < 1e-5 && (p.pos[1] - (384.0 - p.sht.diag)).abs() < 1e-5);
        inp.raw = input::FOCUS | input::DOWN;
        inp.tick();
        for _ in 0..100 {
            p.step(&mut anm, &inp);
        }
        assert_eq!((p.pos[1], p.focus, p.mode), (432.0, 1, 1));
        let mut rng = Rng::default();
        let env = GrazeEnv::default();
        assert_eq!(p.graze_test([p.pos[0] + 25.0, p.pos[1], 0.0], [4.0, 4.0], &env, &mut rng).0, 0);
        assert_eq!(p.graze_test([p.pos[0] + 20.0, p.pos[1], 0.0], [4.0, 4.0], &env, &mut rng).0, 1);
        assert_eq!(p.hit_test([p.pos[0] + 2.0, p.pos[1], 0.0], [4.0, 4.0], &mut rng), 1);
        assert_eq!((p.state, p.hits), (3, 1));
        p.cancel_areas.push(Area { pos: [p.pos[0], p.pos[1]], r: 50.0, item: 9, ..Area::default() });
        assert_eq!(p.hit_test([p.pos[0] + 2.0, p.pos[1], 0.0], [4.0, 4.0], &mut rng), 2);
        assert_eq!(p.cancel_item, 9);
    }
}
