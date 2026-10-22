//! Bombs: Player_UpdateBomb, Bomb_Start, the 16 bombs + dissolve spell, tint (09 §7, 14 §1–§2); areas (09 §8).
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::effect;
use crate::gamemgr::{self, F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE};
use crate::gfx::{Gfx, PLAYFIELD};
use crate::input;
use crate::item::ItemManager;
use crate::player::{ANM_SLOT, DISSOLVE_BEGIN, DISSOLVE_END, Player, Stats};
use crate::sound;
use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

// 192 damage + 192 cancel areas, 0x40 each (09 §8).
pub const AREAS: usize = 192;
// 128 × 0x16F0 bomb objects at player +0x1028 (14 §1.1).
pub const OBJS: usize = 128;
// Playfield origin 0x164D2DC / 0x164D2E0 added to VM positions (14 intro).
const ORIGIN: [f32; 2] = [32.0, 16.0];
// Draw prio of bomb shakes/flashes (13 §3: every code call passes 0x15).
const SCREEN_PRIO: u16 = 0x15;

// Damage / cancel area, 0x40 bytes (09 §8).
#[derive(Clone, Copy, Debug, Default)]
pub struct Area {
    // +0 x, y.
    pub pos: [f32; 2],
    // +8 radius (0 → rectangle), +0xC growth per frame.
    pub r: f32,
    pub dr: f32,
    // +0x10 rectangle w, h; +0x18 growth per frame.
    pub size: [f32; 2],
    pub dsize: [f32; 2],
    // +0x20 rotation (rectangle tested in the rotated frame).
    pub rot: f32,
    // +0x24 frames; decremented while ≥ 0, freed below 1.
    pub life: i32,
    // +0x28 cancel item type (6 default, −1 none).
    pub item: i32,
    // +0x2C damage per hit.
    pub damage: i32,
    // +0x30 hit counter (cancel) / dealt total (damage).
    pub total: i32,
    // +0x34 max total damage (0 = unlimited).
    pub max: i32,
    // +0x38 damages when life % period == 0.
    pub period: i32,
    // +0x3C active, +0x3D no hit effect.
    pub active: bool,
    pub no_spark: bool,
}

impl Area {
    // Damage-area overlap in Player_DamageAgainst (09 §4.5): enemy centre `p`, hitbox size `s` (full w, h).
    // Circle: point test on the centre only, `≤ r²` (the box is ignored); rectangle: inclusive box overlap,
    // the box unrotated in the area's frame.
    pub fn hits_enemy(&self, p: [f32; 2], s: [f32; 2]) -> bool {
        let (w2, h2) = (self.size[0] / 2.0, self.size[1] / 2.0);
        if self.r != 0.0 {
            let (dx, dy) = (self.pos[0] - p[0], self.pos[1] - p[1]);
            return dy * dy + dx * dx <= self.r * self.r;
        }
        if self.rot == 0.0 {
            // Helper 0x451CE0: min = p − s·0.5, max = s·0.5 + p.
            let min = [p[0] - s[0] * 0.5, p[1] - s[1] * 0.5];
            let max = [s[0] * 0.5 + p[0], s[1] * 0.5 + p[1]];
            return self.pos[0] - w2 <= max[0]
                && min[0] <= w2 + self.pos[0]
                && self.pos[1] - h2 <= max[1]
                && min[1] <= h2 + self.pos[1];
        }
        let (dx, dy) = (p[0] - self.pos[0], p[1] - self.pos[1]);
        let (sn, c) = (-self.rot).sin_cos();
        let d = [dx * c - dy * sn, dx * sn + dy * c];
        -w2 <= s[0] / 2.0 + d[0]
            && d[0] - s[0] / 2.0 <= w2
            && -h2 <= s[1] / 2.0 + d[1]
            && d[1] - s[1] / 2.0 <= h2
    }

    // Cancel-area test (Player_InBombArea, 09 §8): circle distance² < r²; rectangle inside the rotated bounds.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let (dx, dy) = (p[0] - self.pos[0], p[1] - self.pos[1]);
        if self.r > 0.0 {
            return dx * dx + dy * dy < self.r * self.r;
        }
        let (s, c) = (-self.rot).sin_cos();
        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
        rx.abs() <= self.size[0] / 2.0 && ry.abs() <= self.size[1] / 2.0
    }
}

thread_local! {
    static QUEUED: RefCell<Vec<(bool, Area)>> = const { RefCell::new(Vec::new()) };
}

// Port-only queue: areas created by effect updates (Yukari's boundaries, 14 §2.13); added at the next Player_Calc.
pub fn queue_area(damage: bool, a: Area) {
    QUEUED.with_borrow_mut(|q| q.push((damage, a)));
}

// Bomb object (14 §1.1, 0x16F0 bytes); fields keep their values between bombs.
#[derive(Clone, Debug, Default)]
pub struct BombObj {
    // +0: 0 free, 1 active, 2 exploding.
    pub state: i32,
    // +4.
    pub counter: i32,
    // +8 speed / orbit radius / angular speed, +0xC speed (per bomb).
    pub f8: f32,
    pub fc: f32,
    // +0x10 radians.
    pub angle: f32,
    // +0x14 position, +0x20 centre / spawn point, +0x2C last displacement (Yuyuko).
    pub pos: [f32; 3],
    pub centre: [f32; 3],
    pub disp: [f32; 3],
    // +0x1A0 velocity / radius + radial speed (per bomb).
    pub v: [f32; 3],
    // +0x1AC spin (Sakuya).
    pub spin: f32,
    // +0x1B8 sprite (player ANM file).
    pub vm: AnmVm,
    // +0x440 draw offset.
    pub offset: [f32; 3],
    // +0x16DC age (Sakuya).
    pub age: Timer,
    // +0x16E8 / +0x16EC attached damage / cancel area.
    pub darea: Option<usize>,
    pub carea: Option<usize>,
}

// Player bomb fields (09 §1.1, 14 §1).
#[derive(Clone, Debug)]
pub struct Bomb {
    // +0xFE0: 0 human bomb, 1 youkai bomb, 2 / 3 last spells, 4 dissolve spell.
    pub index: usize,
    // +0xFE4 frames (999 until Bomb_Start sets it).
    pub duration: i32,
    // +0xFEC bombs used by the last spell.
    pub used: i32,
    // +0xFF4 T, 0 on the first frame.
    pub timer: Timer,
    // +0xB8828 bomb origin.
    pub origin: [f32; 3],
    pub objs: Vec<BombObj>,
    // Object 0's +0x1B8 VM and the next ones' used as plain VM slots (Marisa beams, Yukari LS) (14 §1.1).
    pub vms: [AnmVm; 5],
    // Port-only: spell name and cut-in data for the GUI banner (14 §1.2).
    pub name: &'static str,
    pub last_spell: bool,
    // Port-only: character row of SPECS (bomb_char).
    pub ch: u8,
    // Port-only: "all items attracted" request for the item manager.
    pub attract: bool,
    // +0xFF0 counter (Reimu LS blast index, ring index n).
    pub n: i32,
    // Port-only scratch: start S, dash target P, return start Q (Alice, Youmu); Sakuya effect-20 index.
    pub start: [f32; 3],
    pub dash: [f32; 3],
    pub back: [f32; 3],
    pub fx: usize,
}

impl Default for Bomb {
    fn default() -> Self {
        Self {
            index: 0,
            duration: 0,
            used: 0,
            timer: Timer::default(),
            origin: [0.0; 3],
            objs: vec![BombObj::default(); OBJS],
            vms: Default::default(),
            name: "",
            last_spell: false,
            ch: 0,
            attract: false,
            n: 0,
            start: [0.0; 3],
            dash: [0.0; 3],
            back: [0.0; 3],
            fx: effect::NONE,
        }
    }
}

// (name, duration, invulnerability, isLastSpell) per character (Reimu, Yukari, Marisa, Alice, Sakuya, Remilia,
// Youmu, Yuyuko) × (bomb, last spell), 14 §2.
type BombSpec = (&'static str, i32, i32, bool);
const SPECS: [[BombSpec; 2]; 8] = [
    [
        ("霊符「夢想妙珠」", 200, 260, false),
        ("神霊「夢想封印　瞬」", 200, 260, true),
    ],
    [
        ("境符「四重結界」", 150, 200, false),
        ("境界「永夜四重結界」", 250, 300, true),
    ],
    [
        ("恋符「マスタースパーク」", 300, 350, false),
        ("魔砲「ファイナルスパーク」", 350, 380, true),
    ],
    [
        ("魔符「アーティフルサクリファイス」", 210, 250, false),
        ("魔操「リターンイナニメトネス」", 230, 280, true),
    ],
    [
        ("幻符「殺人ドール」", 250, 290, false),
        ("幻葬「夜霧の幻影殺人鬼」", 320, 350, true),
    ],
    [
        ("紅符「不夜城レッド」", 240, 290, false),
        ("紅魔「スカーレットデビル」", 280, 320, true),
    ],
    [
        ("人符「現世斬」", 220, 270, false),
        ("人鬼「未来永劫斬」", 250, 300, false),
    ],
    [
        ("死符「ギャストリドリーム」", 300, 350, false),
        ("死蝶「華胥の永眠」", 300, 350, true),
    ],
];
// Youmu LS passes isLastSpell = 0 (14 §2.12 quirk); the dissolve spell is 40 frames in spell practice.
const DISSOLVE: BombSpec = ("「ディゾルブスペル」", 120, 200, false);
// Table 0x4C6100 (14 §2.2).
const BLAST_COLOURS: [u32; 7] = [
    0x8FFFFFFF, 0x8F0000FF, 0x8FFF00FF, 0x8FFF0000, 0x8FFFFF00, 0x8F00FF00, 0x8F00FFFF,
];
// Alice effect 0x28 / 0x2D colours (14 §2.3) and the boundary angles of Yukari (14 §2.13).
const DOLL_FLASH: [u32; 5] = [0xFFFFFFFF, 0xFFFFD0D0, 0xFFFFB0B0, 0xFFFF8080, 0xFFFF4040];
const DOLL_OCTAGON: [u32; 4] = [0xFFFFFFFF, 0xFFFFD0D0, 0xFFFF8080, 0xFFFF0000];
const BOUNDARY_ANGLES: [f32; 4] = [PI / 4.0, 3.0 * PI / 8.0, PI / 2.0, 5.0 * PI / 8.0];

fn polar(a: f32, r: f32) -> [f32; 2] {
    [a.cos() * r, a.sin() * r]
}

fn xy(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[1]]
}

fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

// Screen effects (13 §3): shake (1: p0, p1, p2), flash (3: p0, p1, colour). p0 is the duration the
// original passes in EDX (14 header note).
fn shake(dur: i32, a0: i32, a1: i32) {
    effect::screen(1, dur, a0, a1, 0, SCREEN_PRIO);
}

fn flash(dur: i32, repeats: i32, colour: u32) {
    effect::screen(3, dur, repeats, colour as i32, 0, SCREEN_PRIO);
}

// Table 0x4C7AD0 (09 §7): teams take the human member's bomb for even indexes, the youkai member's for odd.
pub fn bomb_char(shot: u8, index: usize) -> u8 {
    if shot >= 4 {
        shot - 4
    } else {
        shot * 2 + (index & 1) as u8
    }
}

// TH08_BOMB=<0–4>: forces the bomb index (4 = dissolve spell) for debugging.
fn forced_index() -> Option<usize> {
    crate::settings::get().bomb
}

impl Player {
    // First inactive slot of the half; index 191 reused when full (09 §8.1).
    fn free_slot(v: &[Area]) -> usize {
        v.iter().position(|a| !a.active).unwrap_or(AREAS - 1)
    }

    // Creators 0x44DF00 / 0x44DE60 / 0x44E040 / 0x44DFA0 (09 §8.1).
    pub fn cancel_circle(&mut self, pos: [f32; 2], r: f32, dr: f32, life: i32, item: i32) -> usize {
        let i = Self::free_slot(&self.cancel_areas);
        self.cancel_areas[i] = Area {
            pos,
            r,
            dr,
            life,
            item,
            period: 1,
            active: true,
            ..Area::default()
        };
        i
    }

    pub fn cancel_rect(&mut self, pos: [f32; 2], size: [f32; 2], item: i32, life: i32) -> usize {
        let i = Self::free_slot(&self.cancel_areas);
        self.cancel_areas[i] = Area {
            pos,
            size,
            life,
            item,
            period: 1,
            active: true,
            ..Area::default()
        };
        i
    }

    pub fn damage_circle(
        &mut self,
        pos: [f32; 2],
        r: f32,
        dr: f32,
        damage: i32,
        life: i32,
    ) -> usize {
        let i = Self::free_slot(&self.damage_areas);
        self.damage_areas[i] = Area {
            pos,
            r,
            dr,
            life,
            damage,
            period: 1,
            item: 6,
            active: true,
            ..Area::default()
        };
        i
    }

    pub fn damage_rect(&mut self, pos: [f32; 2], size: [f32; 2], damage: i32, life: i32) -> usize {
        let i = Self::free_slot(&self.damage_areas);
        self.damage_areas[i] = Area {
            pos,
            size,
            life,
            damage,
            period: 1,
            item: 6,
            active: true,
            ..Area::default()
        };
        i
    }

    // Damage circle / rect with period, max and the +0x3D no-hit-effect flag set by the bomb.
    fn dc(
        &mut self,
        pos: [f32; 2],
        r: f32,
        dr: f32,
        dmg: i32,
        life: i32,
        period: i32,
        max: i32,
        no_spark: bool,
    ) -> usize {
        let i = self.damage_circle(pos, r, dr, dmg, life);
        let a = &mut self.damage_areas[i];
        (a.period, a.max, a.no_spark) = (period, max, no_spark);
        i
    }

    fn dr(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        dmg: i32,
        life: i32,
        period: i32,
        no_spark: bool,
    ) -> usize {
        let i = self.damage_rect(pos, size, dmg, life);
        let a = &mut self.damage_areas[i];
        (a.period, a.no_spark) = (period, no_spark);
        i
    }

    pub fn take_queued_areas(&mut self) {
        for (damage, a) in QUEUED.with_borrow_mut(std::mem::take) {
            let v = if damage {
                &mut self.damage_areas
            } else {
                &mut self.cancel_areas
            };
            let i = Self::free_slot(v);
            v[i] = a;
        }
    }

    // 09 §8.1, start of Player_Calc: an area created with life 0 exists for exactly one pass.
    pub fn update_areas(&mut self) {
        for a in self
            .damage_areas
            .iter_mut()
            .chain(self.cancel_areas.iter_mut())
            .filter(|a| a.active)
        {
            if a.life >= 0 {
                a.life -= 1;
                a.r += a.dr;
                a.size[0] += a.dsize[0];
                a.size[1] += a.dsize[1];
            }
            if a.life < 1 {
                a.active = false;
            }
        }
    }

    // Player_UpdateBomb 0x44C650 (09 §7).
    pub fn update_bomb(
        &mut self,
        anm: &mut AnmManager,
        inp: &input::Input,
        items: &mut Option<&mut ItemManager>,
    ) {
        // +0xE2A6C decrement at the top, skipped on a forced-start call (09 §1.1).
        let forced = self.forced_bomb != 0 && self.deathbomb == 1;
        if !forced && self.bomb_cooldown != 0 {
            self.bomb_cooldown -= 1;
        }
        if !self.bombing {
            let mut pressed = inp.pressed(input::BOMB)
                && !self.env.dialogue
                && self.deathbomb != 0
                && self.stats.bombs >= 1.0
                && self.bomb_cooldown == 0;
            // +0xE2A7C drops on a failed normal start test, so it stays 1 through the end frame (09 §1.1).
            if !pressed {
                self.bomb_active = false;
            }
            // SPEC-GAP: 09 §1.1 does not list the special-mode block among the tests that clear +0xE2A7C; kept.
            if pressed && (self.special() || gamemgr::flags() & F_SPELL_PRACTICE != 0) {
                sound::play(0x29);
                pressed = false;
            }
            // State 1 test is redundant: the counter is 0 throughout state 1 (09 §6.3).
            if !(forced || pressed) || matches!(self.state, 1) {
                return;
            }
            // The bomb calc fn runs immediately on the start frame; that branch returns before the gauge add.
            self.start_bomb(anm, forced);
            self.bomb_calc(anm, items);
            self.bomb.timer.tick(anm.speed);
            return;
        }
        if self.bomb.timer.cur < self.bomb.duration {
            self.bomb_calc(anm, items);
            self.bomb.timer.tick(anm.speed);
        } else {
            // End: speed multipliers 1, bombing 0, tint off (06 §10.2); index 4 per 14 §2.17.
            // +0xE2A7C is not cleared here (09 §1.1).
            self.speed_mult = [1.0; 2];
            self.bombing = false;
            anm.tint_off();
            crate::gui::request(crate::gui::Req::PlayerHide);
            if self.bomb.index == 4 {
                gamemgr::set_flags(gamemgr::flags() & !(F_LAST_SPELL | F_LAST_CAPTURED));
                self.dissolve_req |= DISSOLVE_END;
                flash(30, 1, 0xFFFFFFFF);
            }
        }
        // Quirk kept (09 §7): forced gauge add also on the end frame, with the old index.
        if self.bomb.index < 4 {
            let d = 26000 / self.bomb.duration.max(1);
            self.stats
                .gauge_add(if self.bomb.index & 1 == 1 { d } else { -d });
        }
    }

    // Bomb start (09 §7): index from focus / last-spell request, bomb cost, checksum (0x44E2E0), rank −200,
    // counter += 6 (cap sht +8). Not implemented: statistics bit.
    fn start_bomb(&mut self, anm: &mut AnmManager, forced: bool) {
        let rng = &mut anm.rng;
        self.forced_bomb = 0;
        let special = self.special();
        if !special {
            self.freeze_bullets = false;
            effect::free(effect::FIXED_BASE + effect::FIX_DEATH);
        }
        let mut index = self.focus.min(1) as usize;
        if self.last_spell != 0 {
            index = 1 - index + 2;
            // SPEC-GAP: HSCR +0x78 (Last Spells used) counted here, at the bomb start.
            self.stats.rec.last_spells += 1;
            let b = self.stats.bombs;
            let used = if forced || b < 2.0 { b } else { 2.0 };
            self.bomb.used = used as i32;
            self.stats.add_bombs(-used, rng);
        } else {
            self.stats.add_bombs(-1.0, rng);
        }
        if special {
            index = 4;
        }
        // Port-only debug override.
        if let Some(i) = forced_index() {
            index = i;
        }
        self.stats.bombs_used += 1;
        Stats::checksum(rng);
        self.last_spell = 0;
        self.bombing = true;
        self.bomb_active = true;
        self.bombs_started += 1;
        self.bomb.index = index;
        self.bomb.timer.set(0);
        self.bomb.duration = 999;
        self.bomb.ch = bomb_char(self.shot_type, index);
        self.stats.rank(-200);
        self.deathbomb = (self.deathbomb + 6).min(self.sht.deathbomb);
    }

    // Bomb_Start 0x40BE30 (14 §1.2): banner, duration, state 3 + invulnerability, ring, objects freed (other
    // fields kept), items attracted, origin = p.
    fn bomb_begin(&mut self, anm: &mut AnmManager, spec: BombSpec) {
        self.bomb.name = spec.0;
        self.bomb.last_spell = spec.3;
        let face = if self.bomb.index == 4 {
            -1
        } else {
            (self.bomb.ch & 1) as i32
        };
        crate::gui::request(crate::gui::Req::PlayerShow {
            face,
            name: spec.0,
            last: spec.3,
        });
        self.bomb.duration = spec.1;
        self.state = 3;
        self.state_timer.set(spec.2);
        effect::invuln_ring(anm, self.pos, spec.2);
        self.invuln_ring = true;
        self.bomb.objs.iter_mut().for_each(|o| o.state = 0);
        self.bomb.attract = true;
        self.bomb.origin = self.pos;
    }

    // Bomb calc fn dispatch (table 0x4C7AD0 fns 0–4); Bomb_Start runs on the T = 0 frame of each.
    fn bomb_calc(&mut self, anm: &mut AnmManager, items: &mut Option<&mut ItemManager>) {
        let _ = items;
        let t = self.bomb.timer.cur;
        let adv = self.bomb.timer.cur != self.bomb.timer.prev;
        let ls = self.bomb.index & 2 != 0;
        if self.bomb.index == 4 {
            if t == 0 && adv {
                let practice = gamemgr::flags() & F_SPELL_PRACTICE != 0;
                self.bomb_begin(
                    anm,
                    if practice {
                        (DISSOLVE.0, 40, DISSOLVE.2, DISSOLVE.3)
                    } else {
                        DISSOLVE
                    },
                );
                self.dissolve_start(anm);
            }
            return;
        }
        let ch = self.bomb.ch as usize;
        if t == 0 && adv {
            self.bomb_begin(anm, SPECS[ch][ls as usize]);
        }
        let b = Bf {
            t,
            adv,
            ls,
            gs: anm.speed,
        };
        match ch {
            0 if ls => self.bomb_reimu_ls(anm, b),
            0 => self.bomb_reimu(anm, b),
            1 => self.bomb_yukari(anm, b),
            2 => self.bomb_marisa(anm, b),
            3 => self.bomb_alice(anm, b),
            4 => self.bomb_sakuya(anm, b),
            5 => self.bomb_remilia(anm, b),
            6 => self.bomb_youmu(anm, b),
            _ => self.bomb_yuyuko(anm, b),
        }
    }

    // "At T = n": the timer advanced and equals n, so nothing fires twice in slow motion (14 intro).
    fn at(b: Bf, n: i32) -> bool {
        b.adv && b.t == n
    }

    fn obj_script(anm: &mut AnmManager, o: &mut BombObj, script: usize) {
        anm.set_script(&mut o.vm, ANM_SLOT, script);
    }

    // Attached areas follow the object (no-op once the pointers are cleared).
    fn follow(&mut self, k: usize) {
        let o = &self.bomb.objs[k];
        let p = xy(o.pos);
        if let Some(a) = o.darea {
            self.damage_areas[a].pos = p;
        }
        if let Some(a) = o.carea {
            self.cancel_areas[a].pos = p;
        }
    }

    fn areas_off(&mut self, k: usize) {
        let o = &mut self.bomb.objs[k];
        if let Some(a) = o.darea.take() {
            self.damage_areas[a].active = false;
        }
        if let Some(a) = o.carea.take() {
            self.cancel_areas[a].active = false;
        }
    }

    // Runs an object's VM; true when its script has ended.
    fn run_obj(anm: &mut AnmManager, o: &mut BombObj) -> bool {
        o.vm.script.is_none() || o.vm.run(anm)
    }

    fn homing_target(&self) -> Option<[f32; 2]> {
        let t = self.targets[0];
        (t[0] > -100.0).then_some([t[0], t[1]])
    }

    // Shared orb setup of Reimu's bombs (14 §2.1).
    fn reimu_orbs(&mut self, anm: &mut AnmManager) {
        let p = self.pos;
        for i in 0..16 {
            let c = self.cancel_circle(xy(p), 96.0, 0.0, 200, 6);
            let d = self.dc(xy(p), 64.0, 0.0, 5, 200, 2, 200, true);
            let o = &mut self.bomb.objs[i];
            Self::obj_script(anm, o, 0x13);
            o.angle = -PI + i as f32 * PI / 8.0;
            (o.pos, o.centre, o.f8, o.state) = (p, p, 0.0, 1);
            (o.carea, o.darea) = (Some(c), Some(d));
        }
    }

    // Orb impact (14 §2.1 / §2.2). Quirk kept: the C circle is centred on the player, not the orb.
    fn orb_explode(&mut self, anm: &mut AnmManager, i: usize, ls: bool) {
        self.areas_off(i);
        let p = xy(self.pos);
        let q = self.bomb.objs[i].pos;
        self.cancel_circle(p, 64.0, 4.267, 30, 6);
        if ls {
            self.dc(xy(q), 64.0, 8.533, 25, 15, 5, 50, false);
        } else {
            self.dc(xy(q), 64.0, 12.8, 500, 12, 4, 0, false);
        }
        effect::spawn(anm, 6, q, 8, 0xFFFFFFFF);
        let o = &mut self.bomb.objs[i];
        o.state = 2;
        o.vm.pending_interrupt = 1;
        sound::play_at(0xF, q[0]);
        shake(16, 8, 0);
    }

    // 14 §2.1 霊符「夢想妙珠」 0x40C010. (The original's discarded velocity/8 temporary has no effect.)
    fn bomb_reimu(&mut self, anm: &mut AnmManager, b: Bf) {
        let p = self.pos;
        if Self::at(b, 0) {
            effect::spawn(anm, 12, p, 1, 0xFF4040FF);
            self.reimu_orbs(anm);
            sound::play(0xD);
        }
        // T < 40 spin-out: pos = centre + (sin a, cos a)·radius, radius += 3.2.
        if b.t < 40 {
            for (i, o) in self.bomb.objs[..16]
                .iter_mut()
                .enumerate()
                .filter(|(_, o)| o.state == 1)
            {
                o.angle += if i % 2 == 0 { -PI / 60.0 } else { PI / 60.0 };
                let prev = o.pos;
                o.pos = [
                    o.centre[0] + o.angle.sin() * o.f8,
                    o.centre[1] + o.angle.cos() * o.f8,
                    o.pos[2],
                ];
                o.f8 += 3.2;
                o.v = [o.pos[0] - prev[0], o.pos[1] - prev[1], 0.0];
            }
            for i in 0..16 {
                if self.bomb.objs[i].state == 1 {
                    self.follow(i);
                }
            }
        }
        if Self::at(b, 40) {
            for o in self.bomb.objs[..16].iter_mut() {
                o.fc = len(xy(o.v));
                o.angle = o.v[1].atan2(o.v[0]);
                (o.counter, o.f8) = (0, 8.0);
            }
        }
        // T ≥ 40: homing on advanced frames (1e-6 floor port-only), one-frame C r 128, impact at max or T ≥ dur − 30.
        if b.t >= 40 {
            let target = self.homing_target().unwrap_or(xy(p));
            for i in 0..16 {
                if self.bomb.objs[i].state == 1 && b.adv {
                    let o = &mut self.bomb.objs[i];
                    let d = [target[0] - o.pos[0], target[1] - o.pos[1]];
                    let k = (len(d) / (o.f8 / 8.0)).max(1.0);
                    let v = [d[0] / k + o.v[0], d[1] / k + o.v[1]];
                    let l = len(v).max(1e-6);
                    o.f8 = l.clamp(1.0, 10.0);
                    o.v = [v[0] * o.f8 / l, v[1] * o.f8 / l, 0.0];
                    let q = xy(o.pos);
                    self.cancel_circle(q, 128.0, 0.0, 0, 6);
                    let dealt = self.bomb.objs[i]
                        .darea
                        .is_some_and(|a| self.damage_areas[a].total >= 200);
                    if dealt || b.t >= self.bomb.duration - 30 {
                        self.orb_explode(anm, i, false);
                    }
                }
                let o = &mut self.bomb.objs[i];
                if o.state == 0 {
                    continue;
                }
                o.pos[0] += o.v[0] * b.gs;
                o.pos[1] += o.v[1] * b.gs;
                if o.state == 1 {
                    self.follow(i);
                } else {
                    if b.adv {
                        o.counter += 1;
                    }
                    if o.counter >= 30 {
                        o.state = 0;
                    }
                }
            }
        }
        for o in self.bomb.objs[..16].iter_mut().filter(|o| o.state != 0) {
            Self::run_obj(anm, o);
        }
    }

    // 14 §2.2 神霊「夢想封印　瞬」 0x40C910: orb i explodes at T ≥ dur − 40 − i; blasts every 20 frames from T = 40.
    fn bomb_reimu_ls(&mut self, anm: &mut AnmManager, b: Bf) {
        if Self::at(b, 0) {
            effect::spawn(anm, 12, self.pos, 1, 0xFF4040FF);
            self.reimu_orbs(anm);
            self.bomb.n = 0;
            sound::play(0xD);
        }
        for i in 0..16 {
            let o = &mut self.bomb.objs[i];
            if o.state != 1 {
                continue;
            }
            o.angle += if i % 2 == 0 { -PI / 60.0 } else { PI / 60.0 };
            o.pos = [
                o.centre[0] + o.angle.sin() * o.f8,
                o.centre[1] + o.angle.cos() * o.f8,
                o.pos[2],
            ];
            if b.t < 40 {
                o.f8 += if i % 2 == 0 { 2.4 } else { 1.2 };
            }
            if b.t >= self.bomb.duration - 40 - i as i32 {
                self.orb_explode(anm, i, true);
            } else {
                self.follow(i);
            }
        }
        if b.t >= 40 && b.t % 20 == 0 && b.adv {
            // Quirk kept: the first blast uses object 16, every later one reuses object 17.
            // SPEC-GAP: 14 §2.2 sets the VM script before the RNG point; the port draws the point first.
            let k = 16 + self.bomb.n as usize;
            self.bomb.n = 1;
            let colour = BLAST_COLOURS[((b.t / 20) % 7) as usize];
            let point = match self.homing_target() {
                Some(p) => p,
                None => {
                    let x = anm.rng.next_f32() * 320.0 + 32.0;
                    [x, anm.rng.next_f32() * 384.0 + 32.0]
                }
            };
            let pt = [point[0], point[1], 0.0];
            effect::spawn(anm, 0x31, pt, 1, colour);
            effect::spawn(anm, 0x37, pt, 1, colour);
            let c = self.cancel_circle(point, 64.0, 4.267, 30, 6);
            let d = self.dc(point, 64.0, 8.533, 400, 15, 2, 0, false);
            let o = &mut self.bomb.objs[k];
            Self::obj_script(anm, o, 0x14);
            o.vm.color = colour.to_le_bytes();
            (o.pos, o.state, o.carea, o.darea) = (pt, 1, Some(c), Some(d));
            sound::play(0xF);
            shake(16, 8, 0);
            flash(8, 1, colour);
        }
        for k in 0..OBJS {
            if self.bomb.objs[k].state == 0 {
                continue;
            }
            if k >= 16 {
                self.follow(k);
            }
            if Self::run_obj(anm, &mut self.bomb.objs[k]) && k >= 16 {
                self.bomb.objs[k].state = 0;
            }
        }
    }

    // Draws active objects 0..n at pos + offset + playfield origin, z 0, optional rotation z.
    fn draw_objs(
        &mut self,
        anm: &mut AnmManager,
        gfx: &mut Gfx,
        n: usize,
        mode: i32,
        rot: impl Fn(&BombObj) -> Option<f32>,
    ) {
        for o in self.bomb.objs[..n].iter_mut().filter(|o| o.state != 0) {
            if let Some(a) = rot(o)
                && o.vm.rotation[2] != a
            {
                o.vm.rotation[2] = a;
                o.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            o.vm.pos = [
                o.pos[0] + o.offset[0] + ORIGIN[0],
                o.pos[1] + o.offset[1] + ORIGIN[1],
                0.0,
            ];
            anm.draw_mode_z(gfx, &mut o.vm, mode, 0.0);
        }
    }

    // 14 §2.3 / §2.4 Alice 0x40D430 / 0x40D970: the doll is option 0.
    fn bomb_alice(&mut self, anm: &mut AnmManager, b: Bf) {
        let ls = b.ls;
        let centre = [192.0, 224.0, 0.0];
        if Self::at(b, 0) {
            sound::play(0xD);
            if ls {
                self.bomb.start = self.pos;
            } else {
                self.bomb.start = self.options[0].pos;
                sound::play(6);
            }
        }
        let s = self.bomb.start;
        let doll = &mut self.options[0];
        // T < 60: doll pos = S + (centre − S)·(T/60)², spin −π/10, one-frame C/D r 32.
        if b.t < 60 {
            let f = (b.t as f32 / 60.0).powi(2);
            doll.pos = [
                s[0] + (centre[0] - s[0]) * f,
                s[1] + (centre[1] - s[1]) * f,
                0.0,
            ];
            doll.vm.rotation[2] -= PI / 10.0;
            doll.vm.flags |= anmvm::F_ROT_DIRTY;
            let q = xy(doll.pos);
            self.cancel_circle(q, 32.0, 0.0, 0, 6);
            self.dc(q, 32.0, 0.0, 40, 0, 1, 0, false);
        } else {
            doll.vm.rotation[2] = 0.0;
            doll.vm.flags |= anmvm::F_ROT_DIRTY;
            doll.pos = centre;
        }
        let doll = &mut self.options[0];
        if b.t >= if ls { 128 } else { 150 } {
            doll.vm.color[anmvm::A] = 0;
        }
        let q = doll.pos;
        if b.adv && (60..=76).contains(&b.t) && b.t % 4 == 0 {
            effect::spawn(anm, 0x28, q, 1, DOLL_FLASH[(b.t as usize - 60) / 4]);
        }
        let (blast, oct0, end) = if ls { (120, 130, 180) } else { (90, 100, 150) };
        if Self::at(b, blast) {
            sound::play(0xF);
            for k in [0x2A, 0x2B, 0x2C] {
                effect::spawn(anm, k, q, 1, 0xFFFFFFFF);
            }
            if ls {
                for (p, c) in [
                    ([64.0, 96.0], 0xFF0000F0),
                    ([64.0, 352.0], 0xFFF00000),
                    ([320.0, 352.0], 0xFF00F000),
                    ([320.0, 96.0], 0xFF00F0F0),
                ] {
                    effect::spawn(anm, 0x2D, [p[0], p[1], 0.0], 1, c);
                }
            }
            let c = self.cancel_circle(xy(q), 1.0, 5.0, 110, 6);
            let _ = c;
            self.dc(xy(q), 1.0, 5.0, 70, 110, 5, 0, false);
            if !ls {
                shake(24, 8, 0);
                flash(8, 1, 0x8FFFFFFF);
            }
        }
        if b.adv && (oct0..oct0 + 40).contains(&b.t) && (b.t - oct0) % 10 == 0 {
            effect::spawn(anm, 0x2D, q, 1, DOLL_OCTAGON[((b.t - oct0) / 10) as usize]);
        }
        if Self::at(b, end) {
            flash(8, 1, if ls { 0xFFFFFFFF } else { 0x8FFFFFFF });
            shake(24, 8, 0);
            sound::play(0x19);
        }
        if Self::at(b, self.bomb.duration - 1) {
            self.options[0].state = 1;
            self.options[0].timer.set(0);
        }
    }

    // 14 §2.5 / §2.6 Marisa 0x40E3B0 / 0x40E780: five beam VMs in object 0's memory.
    fn bomb_marisa(&mut self, anm: &mut AnmManager, b: Bf) {
        let p = self.pos;
        if Self::at(b, 0) {
            if b.ls {
                sound::play(0xD);
            }
            sound::play(0x13);
            self.bomb.objs[0].pos = p;
            let base = if b.ls { 0x23 } else { 0x1E };
            for (i, vm) in self.bomb.vms.iter_mut().enumerate() {
                anm.set_script(vm, ANM_SLOT, base + i);
            }
            self.speed_mult = [0.2; 2];
            effect::screen(7, 16, 120, 60, 120, SCREEN_PRIO);
            self.bomb.n = 0;
        }
        // Advanced frames with T % 4 ≠ 0: everything above the player, unrotated, life 0.
        if b.adv && b.t % 4 != 0 {
            let h = p[1];
            self.cancel_rect([192.0, h / 2.0], [384.0, h], 6, 0);
            self.dr([p[0], h / 2.0], [128.0, h], 12, 0, 1, true);
            self.dr(
                [192.0, h / 2.0],
                [384.0, h],
                if b.ls { 7 } else { 6 },
                0,
                1,
                true,
            );
        }
        if b.ls && b.adv && b.t % 10 == 0 {
            let n = self.bomb.n;
            self.spark_ring(anm, n, 0, Some((32.0, 64.0)), [128.0, 0.0, 0.0], n % 2 == 1);
            self.bomb.n += 1;
        }
        for vm in &mut self.bomb.vms {
            if vm.script.is_some() {
                vm.run(anm);
            }
        }
    }

    // Ring effect 0x35 in fixed slot 4 + n % 4 (14 §2.6, §2.9): radius/amplitude, width, fade, white → red;
    // `alt` = Marisa LS odd n → script 0x5C. `width` None = constant 64 (Remilia's 64 → 64 interpolation).
    fn spark_ring(
        &mut self,
        anm: &mut AnmManager,
        n: i32,
        waves: i32,
        width: Option<(f32, f32)>,
        end: [f32; 3],
        alt: bool,
    ) {
        let p = self.pos;
        let i = effect::spawn_fixed(anm, 0x35, p, 4 + n.rem_euclid(4) as usize, 0xFFFFFFFF);
        effect::with(|e| {
            let x = &mut e.fx[i];
            if alt {
                anm.set_script(&mut x.vm, effect::SCRIPT_SLOT, 0x5C);
                x.vm.color = 0xFFFFFFFFu32.to_le_bytes();
            }
            x.ring.segments = 32;
            x.ring.waves = waves as f32;
            x.vm.interp_pos_from(30, 4, [0.0; 3], end);
            match width {
                Some((a, b)) => x.vm.interp_scale_from(30, 1, [a, 0.0], [b, 0.0]),
                None => {
                    x.vm.scale = [64.0, 0.0];
                    x.vm.flags |= anmvm::F_SCALE_DIRTY;
                }
            }
            x.vm.interp_alpha_from(30, 3, 255, 0);
            x.vm.interp_color_from(30, 0, 0xFFFFFF, 0xFF0000);
            x.vm.run(anm);
        });
        sound::play_at(0x11, p[0]);
    }

    // 14 §2.7 / §2.8 Sakuya 0x40FCD0 / 0x4103F0: knives k and k + per appear together every second frame.
    fn bomb_sakuya(&mut self, anm: &mut AnmManager, b: Bf) {
        let ls = b.ls;
        let p = self.pos;
        let (count, per, window) = if ls { (128, 64, 148) } else { (96, 48, 116) };
        if Self::at(b, 0) {
            self.bomb.objs[..count].iter_mut().for_each(|o| o.state = 0);
            self.speed_mult = [0.5; 2];
            self.bomb.fx = effect::spawn(anm, 20, p, 1, 0xFFFFFFFF);
            sound::play(5);
            if ls {
                shake(50, 4, 1);
            }
        }
        if b.t <= 60 {
            effect::get(self.bomb.fx, |e| e.pos = p);
        }
        if (20..window).contains(&b.t) {
            if !ls {
                sound::play(6);
                shake(120, 4, 1);
            }
            for k in 0..count {
                if !Self::at(b, (k % per) as i32 * 2 + 20) {
                    continue;
                }
                // Quirk kept: an already active knife makes the whole function return for this frame.
                if self.bomb.objs[k].state != 0 {
                    return;
                }
                let angle = if ls {
                    anmvm::wrap_add(k as f32 * TAU / 64.0 - PI, 0.0)
                } else {
                    k as f32 * TAU / 96.0 - PI
                };
                let speed = anm.rng.next_f32() + 0.5;
                let accel = anm.rng.next_f32() * 0.1 + 0.03;
                // Quirk kept: Rng_Range16(1) is always 0, so the spin is always −π/20 (one draw; 3 per knife).
                let spin = if anm.rng.range16(1) == 0 {
                    -PI / 20.0
                } else {
                    PI / 20.0
                };
                let d = polar(angle, 24.0);
                let pos = [p[0] + d[0], p[1] + d[1], 0.0];
                let c = self.cancel_circle(xy(pos), 32.0, 0.0, 500, 6);
                let da = self.dc(
                    xy(pos),
                    32.0,
                    0.0,
                    if ls { 30 } else { 20 },
                    500,
                    1,
                    0,
                    false,
                );
                let o = &mut self.bomb.objs[k];
                Self::obj_script(anm, o, if ls { 0x14 } else { 0x16 });
                (o.state, o.angle, o.fc, o.f8, o.spin, o.pos) = (1, angle, speed, accel, spin, pos);
                o.age.set(0);
                (o.carea, o.darea) = (Some(c), Some(da));
            }
            if ls {
                sound::play(6);
            }
        }
        let target = self.homing_target();
        for k in 0..count {
            if self.bomb.objs[k].state == 0 {
                continue;
            }
            let o = &mut self.bomb.objs[k];
            let age = o.age.cur;
            // Ages 30–69 hover and spin; at 70 re-aim at the homing target with speed 14.
            if !(30..=69).contains(&age) {
                if age == 70 && o.age.prev != 70 {
                    if let Some(t) = target {
                        o.angle = (t[1] - o.pos[1]).atan2(t[0] - o.pos[0]);
                    }
                    o.fc = 14.0;
                    if ls {
                        let q = o.pos;
                        effect::spawn(anm, 0x2E, q, 1, 0xFFFFFFFF);
                    }
                }
                let o = &mut self.bomb.objs[k];
                o.fc += o.f8;
                let v = polar(o.angle, o.fc);
                o.v = [v[0], v[1], 0.0];
            } else {
                o.angle += o.spin;
                o.v = [0.0; 3];
            }
            self.follow(k);
            // First damage dealt (age < 120): hit script, effect 0, areas off and pointers cleared (runs once).
            if age < 120 {
                if self.bomb.objs[k]
                    .darea
                    .is_some_and(|a| self.damage_areas[a].total > 0)
                {
                    let o = &mut self.bomb.objs[k];
                    Self::obj_script(anm, o, if ls { 0x15 } else { 0x17 });
                    let q = o.pos;
                    effect::spawn(anm, 0, q, 1, 0xFFFF80FF);
                    self.areas_off(k);
                    sound::play_at(0x2B, q[0]);
                    if ls && k % 3 == 0 {
                        flash(2, 1, 0x208080FF);
                    }
                }
            } else {
                self.areas_off(k);
            }
            // Not scaled by gs (14 §2.7); knives are never freed before the next bomb.
            let o = &mut self.bomb.objs[k];
            o.pos[0] += o.v[0];
            o.pos[1] += o.v[1];
            Self::run_obj(anm, o);
            o.age.tick(b.gs);
        }
    }

    // 14 §2.9 / §2.10 Remilia 0x40EE10 / 0x40F570: the bats are the 4 options (anchors moved).
    // No damage areas: ply02as.sht levels 6/7 do the damage (09 §2.1).
    fn bomb_remilia(&mut self, anm: &mut AnmManager, b: Bf) {
        let p = self.pos;
        let targets = [
            [p[0] - 32.0, p[1]],
            [p[0], p[1] - 32.0],
            [p[0], p[1] + 32.0],
            [p[0] + 32.0, p[1]],
        ];
        if Self::at(b, 0) {
            sound::play(0xD);
            for k in 0..4 {
                self.bomb.objs[k].centre = self.options[k].anchor;
            }
            sound::play(6);
            self.bomb.n = 0;
            self.speed_mult = [0.0; 2];
        }
        if b.t < 60 {
            let f = (b.t as f32 / 60.0).powi(2);
            for k in 0..4 {
                let s = self.bomb.objs[k].centre;
                let a = targets[k];
                self.options[k].anchor = [s[0] + (a[0] - s[0]) * f, s[1] + (a[1] - s[1]) * f, 0.0];
            }
            return;
        }
        if Self::at(b, 60) {
            self.speed_mult = [if b.ls { 3.0 } else { 2.0 }; 2];
            for o in &mut self.options {
                o.vm.pending_interrupt = 2;
            }
        }
        for k in 0..4 {
            self.options[k].anchor = [targets[k][0], targets[k][1], 0.0];
        }
        self.cancel_circle(xy(p), 96.0, 0.0, 0, 6);
        if b.adv && b.t % 10 == 0 {
            let n = self.bomb.n;
            let amp = anm.rng.next_f32() * 128.0;
            let width = Some((64.0, if b.ls { 128.0 } else { 64.0 }));
            self.spark_ring(anm, n, 4, width, [192.0, amp, 0.0], false);
            self.bomb.n += 1;
        }
        // While shooting (shot timer ≥ 5): a cross of two cancel rects, life 0.
        if self.shot_timer.cur >= 5 {
            self.cancel_rect(xy(p), [96.0, 800.0], 6, 0);
            self.cancel_rect(xy(p), [800.0, 96.0], 6, 0);
        }
        if Self::at(b, self.bomb.duration - 1) {
            for o in &mut self.options {
                o.state = 1;
                o.timer.set(0);
            }
        }
    }

    // 14 §2.11 / §2.12 Youmu 0x411B10 / 0x4123D0: dash down, slashes from the top, dash back.
    fn bomb_youmu(&mut self, anm: &mut AnmManager, b: Bf) {
        let ls = b.ls;
        if Self::at(b, 0) {
            sound::play(0xD);
            self.speed_mult = [0.0; 2];
            self.set_script(anm, 0);
            sound::play(6);
            self.bomb.start = self.pos;
            self.bomb.dash = [self.pos[0], 416.0, self.pos[2]];
        }
        let (s, pd) = (self.bomb.start, self.bomb.dash);
        if b.t < 40 {
            let f = 1.0 - (1.0 - b.t as f32 / 40.0).powi(2);
            self.pos[0] = s[0] + (pd[0] - s[0]) * f;
            self.pos[1] = s[1] + (pd[1] - s[1]) * f;
        }
        let colour = if ls { 0xFFFF8080 } else { 0xFF8080FF };
        if Self::at(b, 40) {
            effect::spawn(anm, 0x28, self.pos, 1, colour);
        }
        if Self::at(b, 70) {
            self.pos[1] = 32.0;
            let c = [self.pos[0], 224.0];
            self.cancel_rect(c, [96.0, 448.0], 6, 60);
            if ls {
                self.dr(c, [96.0, 448.0], 500, 0, 1, false);
                self.dr(c, [96.0, 448.0], 100, 60, 5, false);
            } else {
                self.dr(c, [96.0, 448.0], 300, 10, 1, false);
                self.dr(c, [96.0, 448.0], 80, 60, 5, false);
            }
            effect::spawn(anm, 0x30, [c[0], c[1], 0.0], 1, colour);
            flash(8, 1, 0xEFFFFFFF);
            sound::play(0x2A);
        }
        let pairs: &[(i32, f32, i32, u32)] = if ls {
            &[
                (80, 32.0, 100, 0xCF),
                (90, 64.0, 100, 0xAF),
                (100, 96.0, 80, 0x8F),
                (110, 128.0, 60, 0x6F),
                (120, 160.0, 50, 0x5F),
                (130, 192.0, 40, 0x5F),
            ]
        } else {
            &[
                (80, 32.0, 100, 0xCF),
                (90, 64.0, 100, 0xBF),
                (100, 96.0, 80, 0x8F),
            ]
        };
        for &(at, dx, dmg, a) in pairs {
            if !Self::at(b, at) {
                continue;
            }
            for (sgn, period) in [(-1.0, 2), (1.0, 3)] {
                let c = [self.pos[0] + sgn * dx, 224.0];
                self.cancel_rect(c, [96.0, 448.0], 6, 60);
                self.dr(c, [96.0, 448.0], dmg, 40, period, false);
                effect::spawn(anm, 0x30, [c[0], c[1], 0.0], 1, colour);
            }
            flash(8, 1, (a << 24) | 0xFFFFFF);
        }
        let (q_at, back0) = if ls { (130, 150) } else { (100, 120) };
        if Self::at(b, q_at) {
            self.bomb.back = self.pos;
        }
        // Return dash; the original function exits early here.
        if (back0..back0 + 30).contains(&b.t) {
            let q = self.bomb.back;
            let f = 1.0 - (1.0 - (b.t - back0) as f32 / 30.0).powi(2);
            self.pos[0] = q[0] + (s[0] - q[0]) * f;
            self.pos[1] = q[1] + (s[1] - q[1]) * f;
            return;
        }
        if Self::at(b, back0 + 30) {
            self.speed_mult = [1.0; 2];
        }
    }

    // 14 §2.13 / §2.14 Yukari 0x410C40 / 0x410FE0: boundary rings (effects 0x24 / 0x25) and expanding circles.
    fn bomb_yukari(&mut self, anm: &mut AnmManager, b: Bf) {
        let ls = b.ls;
        let p = self.pos;
        if Self::at(b, 0) {
            sound::play(0xD);
            if ls {
                self.bomb.objs[0].pos = p;
                for (i, s) in [(0, 0x15), (1, 0x16)] {
                    anm.set_script(&mut self.bomb.vms[i], ANM_SLOT, s);
                }
            }
        }
        for (k, at) in [0, 10, 20, 30].into_iter().enumerate() {
            if !Self::at(b, at) {
                continue;
            }
            let life = if ls { [100, 40, 100, 100][k] } else { 40 };
            self.cancel_circle(xy(p), 100.0, 1.0, life, 6);
            self.dc(xy(p), 100.0, 1.0, 70, 40, 5, 0, false);
            let at_pos = if ls || k == 0 { self.bomb.origin } else { p };
            let kind = if ls { 0x25 } else { 0x24 };
            let i = effect::with(|e| {
                e.spawn_fixed(
                    anm,
                    kind,
                    at_pos,
                    [BOUNDARY_ANGLES[k], 1.0, 4.0],
                    4 + k,
                    0xFFFFFFFF,
                )
            });
            // Quirk kept (14 §2.14): the init re-spawns the slot as type 0x23, so slot 4 runs script 88 even for 0x25.
            let script = if k == 0 {
                88
            } else if ls {
                0x5C + k
            } else {
                0x58 + k
            };
            effect::with(|e| anm.set_script(&mut e.fx[i].vm, effect::SCRIPT_SLOT, script));
        }
        if ls {
            for vm in self.bomb.vms[..2].iter_mut().filter(|v| v.script.is_some()) {
                vm.run(anm);
            }
        }
    }

    // 14 §2.15 / §2.16 Yuyuko 0x413140 / 0x413990: butterflies.
    fn bomb_yuyuko(&mut self, anm: &mut AnmManager, b: Bf) {
        let p = self.pos;
        if Self::at(b, 0) {
            self.bomb.objs.iter_mut().for_each(|o| o.state = 0);
            const GROUPS: [(usize, f32, f32); 4] = [
                (0x12, PI / 240.0, 2.0),
                (0x13, -PI / 240.0, 2.0),
                (0x12, PI / 200.0, 1.5),
                (0x13, -PI / 200.0, 1.5),
            ];
            for (g, &(script, w, dr)) in GROUPS.iter().enumerate() {
                for i in 0..16 {
                    let d = self.dc(xy(p), 24.0, 0.0, 50, 500, 1, 800, false);
                    let c = self.cancel_circle(xy(p), 24.0, 0.0, 500, 6);
                    let o = &mut self.bomb.objs[g * 16 + i];
                    Self::obj_script(anm, o, script);
                    o.angle = i as f32 * PI / 8.0 - PI;
                    (o.centre, o.pos, o.f8, o.state) = (p, p, w, 1);
                    o.v = [0.0, dr, 0.0];
                    (o.darea, o.carea) = (Some(d), Some(c));
                }
            }
            self.speed_mult = [0.8; 2];
            effect::spawn(anm, 20, p, 1, 0xFFFFFFFF);
            sound::play(5);
            if b.ls {
                shake(60, 16, 0);
            } else {
                shake(120, 12, 0);
            }
        }
        if b.ls && (60..200).contains(&b.t) && b.adv && b.t % 20 == 0 {
            // Rng_SignedRange(π); quirks kept: angle uses the object slot index, the angular speed (f8) is
            // whatever the slot held, SE 0xF is panned at the last examined object.
            let base = anm.rng.next_signed() * PI;
            let (mut taken, mut last) = (0, OBJS - 1);
            for k in 0..OBJS {
                last = k;
                if self.bomb.objs[k].state != 0 {
                    continue;
                }
                let d = self.dc(xy(p), 64.0, 0.0, 100, 500, 1, 1200, false);
                let c = self.cancel_circle(xy(p), 64.0, 0.0, 500, 6);
                let o = &mut self.bomb.objs[k];
                Self::obj_script(anm, o, 0x14);
                o.angle = anmvm::wrap_add(k as f32 * PI / 8.0 + base, 0.0);
                (o.centre, o.pos, o.state) = (p, p, 1);
                o.v = [0.0, 8.0, 0.0];
                (o.darea, o.carea) = (Some(d), Some(c));
                taken += 1;
                if taken == 16 {
                    break;
                }
            }
            sound::play_at(0xF, self.bomb.objs[last].pos[0]);
            shake(30, 8, 0);
            flash(8, 1, 0xE0F0F0F0);
        }
        for k in 0..OBJS {
            let o = &mut self.bomb.objs[k];
            if o.state == 0 {
                continue;
            }
            o.v[0] += o.v[1];
            o.angle += o.f8;
            let prev = o.pos;
            let d = polar(o.angle, o.v[0]);
            o.pos = [o.centre[0] + d[0], o.centre[1] + d[1], 0.0];
            o.disp = [o.pos[0] - prev[0], o.pos[1] - prev[1], 0.0];
            if o.v[0] < 500.0 {
                self.follow(k);
                if Self::run_obj(anm, &mut self.bomb.objs[k]) {
                    self.bomb.objs[k].state = 0;
                    self.areas_off(k);
                }
            } else {
                o.state = 0;
            }
        }
    }

    // 14 §2.17 「ディゾルブスペル」 0x40D100: boss flag clearing is done by the enemy manager on DISSOLVE_BEGIN.
    fn dissolve_start(&mut self, anm: &mut AnmManager) {
        let p = self.pos;
        let practice = gamemgr::flags() & F_SPELL_PRACTICE != 0;
        effect::spawn(anm, 12, p, 1, 0xFF4040FF);
        let i = effect::spawn_fixed(anm, 0x32, p, 4, 0xFF4040FF);
        effect::get(i, |e| {
            e.ring.segments = 64;
            e.ring.waves = 6.0;
            e.vm.scale[0] = 15.0;
            e.vm.pos = [8.0, 64.0, 0.0];
            e.vm.interp_pos_from(
                if practice { 30 } else { 90 },
                5,
                [8.0, 64.0, 0.0],
                [128.0, 0.0, 0.0],
            );
        });
        sound::play(0xD);
        self.speed_mult = [0.0; 2];
        self.dissolve_req |= DISSOLVE_BEGIN;
    }

    // Background tint 0x40BC60 (14 §1.3): ramps over the first/last 60 frames, alpha 0x80; global tint off, then
    // a stage tint request for the next Stage_DrawLow and the +0x646C "bomb drew" flag.
    fn tint(&self, anm: &mut AnmManager, c: u32) {
        let (t, dur) = (self.bomb.timer.cur, self.bomb.duration);
        let ch = |v: u32| -> u32 {
            let v = v as i32;
            let x = if t < 60 {
                128 + (v - 128) * t / 60
            } else if t >= dur - 60 {
                128 + (v - 128) * (dur - t) / 60
            } else {
                v
            };
            (x as u32) & 0xFF
        };
        anm.tint_off();
        anm.request_tint(
            0x8000_0000 | ch((c >> 16) & 0xFF) << 16 | ch((c >> 8) & 0xFF) << 8 | ch(c & 0xFF),
        );
        anm.bomb_drew = true;
    }

    // Alpha-blended playfield rectangle (Alice LS, Youmu draw fns).
    fn playfield_rect(gfx: &mut Gfx, rgb: u32, a: i32) {
        let r = PLAYFIELD;
        gfx.rect(
            r.x,
            r.y,
            r.w,
            r.h,
            [
                (rgb >> 16) as u8,
                (rgb >> 8) as u8,
                rgb as u8,
                a.clamp(0, 255) as u8,
            ],
        );
    }

    // Bomb draw functions (table 0x4C7AD0 fns 5–9), called from the player draw (prio 9, 14 §5).
    // Quirk kept: Sakuya LS draws only knives 0–95 (96–127 are invisible but damage).
    pub fn draw_bomb(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        if !self.bombing {
            return;
        }
        let t = self.bomb.timer.cur;
        let ls = self.bomb.index & 2 != 0;
        if self.bomb.index == 4 {
            let c = if t < 60 {
                0x80404040
            } else {
                let g = ((t - 60) * 176 / 60 + 64) as u32;
                0x8000_0000 | g << 16 | g << 8 | g
            };
            self.tint(anm, c);
            return;
        }
        let p = self.pos;
        match self.bomb.ch {
            0 => {
                self.tint(anm, if ls { 0x802020D0 } else { 0x80404040 });
                self.draw_objs(anm, gfx, if ls { OBJS } else { 16 }, 0, |_| None);
            }
            1 => {
                self.tint(anm, if ls { 0x802020D0 } else { 0x80404040 });
                if ls {
                    let o = self.bomb.objs[0].pos;
                    for (vm, z) in self.bomb.vms[..2].iter_mut().zip([0.01, 0.0]) {
                        vm.pos = [
                            o[0] + ORIGIN[0] + vm.pos_offset[0],
                            o[1] + ORIGIN[1] + vm.pos_offset[1],
                            0.0,
                        ];
                        anm.draw_mode_z(gfx, vm, 2, z);
                    }
                }
            }
            2 => {
                self.tint(anm, 0x80404040);
                for (i, vm) in self.bomb.vms.iter_mut().enumerate() {
                    let a = i as f32 * PI / 15.0 - FRAC_PI_2 - 2.0 * PI / 15.0;
                    let d = polar(a, vm.sprite_w * vm.scale[0] / 2.0);
                    vm.pos = [p[0] + d[0] + ORIGIN[0], p[1] + d[1] + ORIGIN[1], 0.0];
                    if vm.rotation[2] != a {
                        vm.rotation[2] = a;
                        vm.flags |= anmvm::F_ROT_DIRTY;
                    }
                    anm.draw_mode_z(gfx, vm, 2, 0.0);
                }
            }
            3 if !ls => self.tint(anm, 0x80404040),
            3 => {
                if t < 90 || t > 220 {
                    self.tint(anm, 0x802020D0);
                } else if t <= 120 {
                    let c = ((t - 90) * 208 / 30) as u32;
                    let r = (c / 5).wrapping_sub(48) & 0xFF;
                    self.tint(anm, 0x8000_0000 | r << 16 | (c + 32) << 8 | (c + 32));
                    Self::playfield_rect(gfx, 0xFFFFFF, (t - 90) * 255 / 30);
                } else {
                    Self::playfield_rect(gfx, 0xFFFFFF, 0x70);
                }
            }
            4 => {
                self.tint(anm, if ls { 0x80202080 } else { 0x80404040 });
                self.draw_objs(anm, gfx, 96, 2, |o| Some(o.angle));
            }
            5 => self.tint(anm, if ls { 0x80F00000 } else { 0x80D02020 }),
            6 => {
                self.tint(anm, if t >= 70 { 0x80000030 } else { 0x80404040 });
                if (70..160).contains(&t) {
                    let a = if t <= 100 {
                        255
                    } else {
                        255 - (t - 100) * 255 / 60
                    };
                    Self::playfield_rect(gfx, if ls { 0xFF0000 } else { 0xFFFFFF }, a);
                }
            }
            _ => {
                self.tint(anm, if ls { 0x80802020 } else { 0x80404040 });
                let n = if ls { OBJS } else { 96 };
                self.draw_objs(anm, gfx, n, 2, |o| Some(o.disp[1].atan2(o.disp[0])));
            }
        }
    }
}

// Per-frame bomb context: T, timer advanced, last spell, game speed.
#[derive(Clone, Copy)]
struct Bf {
    t: i32,
    adv: bool,
    ls: bool,
    gs: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(bits: u16) -> input::Input {
        let mut i = input::Input {
            raw: bits,
            ..Default::default()
        };
        i.tick();
        i
    }

    fn ready(shot: u8) -> (Player, AnmManager) {
        let mut p = Player {
            state: 0,
            focus: 0,
            shot_type: shot,
            deathbomb: 18,
            ..Player::default()
        };
        p.stats.bombs = 3.0;
        (p, AnmManager::default())
    }

    fn shakes_at(p: &mut Player, anm: &mut AnmManager, ls: bool, t: i32) -> Vec<[i32; 4]> {
        effect::take_screen();
        let b = Bf {
            t,
            adv: true,
            ls,
            gs: 1.0,
        };
        match p.bomb.ch {
            3 => p.bomb_alice(anm, b),
            _ => p.bomb_sakuya(anm, b),
        }
        effect::take_screen()
            .into_iter()
            .filter(|s| s.0 == 1)
            .map(|s| s.1)
            .collect()
    }

    // 14 header note, §2.7 / §2.8: the shake duration is the EDX argument: Sakuya (1: 120, 4, 1) every window frame,
    // Sakuya LS (1: 50, 4, 1) once at T = 0.
    #[test]
    fn sakuya_shakes() {
        let (mut p, mut anm) = ready(8);
        p.bomb.ch = 4;
        assert!(shakes_at(&mut p, &mut anm, false, 0).is_empty());
        assert_eq!(shakes_at(&mut p, &mut anm, false, 21), [[120, 4, 1, 0]]);
        let (mut p, mut anm) = ready(8);
        p.bomb.ch = 4;
        assert_eq!(shakes_at(&mut p, &mut anm, true, 0), [[50, 4, 1, 0]]);
        assert!(shakes_at(&mut p, &mut anm, true, 21).is_empty());
    }

    // 14 §2.3 / §2.4: Alice shakes (1: 24, 8, 0) at T = 90 and 150; Alice LS only at T = 180.
    #[test]
    fn alice_shakes() {
        let (mut p, mut anm) = ready(7);
        p.bomb.ch = 3;
        p.bomb.duration = 210;
        for (t, n) in [(90, 1), (150, 1)] {
            assert_eq!(
                shakes_at(&mut p, &mut anm, false, t),
                vec![[24, 8, 0, 0]; n]
            );
        }
        p.bomb.duration = 230;
        assert!(shakes_at(&mut p, &mut anm, true, 120).is_empty());
        assert_eq!(shakes_at(&mut p, &mut anm, true, 180), [[24, 8, 0, 0]]);
    }

    #[test]
    fn reimu_bomb_lifecycle() {
        let (mut p, mut anm) = ready(4);
        p.step(&mut anm, &press(input::BOMB));
        assert!(p.bombing && p.bomb.duration == 200 && p.state == 3);
        assert_eq!(
            p.bomb.objs[..16].iter().filter(|o| o.state == 1).count(),
            16
        );
        assert_eq!(
            p.damage_areas
                .iter()
                .filter(|a| a.active && a.no_spark && a.max == 200)
                .count(),
            16
        );
        for _ in 0..199 {
            p.step(&mut anm, &press(0));
        }
        assert!(p.bombing && p.bomb_active);
        p.step(&mut anm, &press(0));
        assert!(!p.bombing && p.speed_mult == [1.0; 2]);
        assert!(p.bomb.objs[..16].iter().all(|o| o.state == 0));
        // 09 §1.1: +0xE2A7C holds through the end frame, drops on the next failed start test.
        assert!(p.bomb_active);
        p.step(&mut anm, &press(0));
        assert!(!p.bomb_active);
    }

    // 09 §1.1: +0xE2A6C drops atop Player_UpdateBomb (so 1 → 0 allows a start in the same call), except on a
    // forced-start call.
    #[test]
    fn bomb_cooldown_decrement() {
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 2;
        p.step(&mut anm, &press(input::BOMB));
        assert!(!p.bombing && p.bomb_cooldown == 1);
        p.step(&mut anm, &press(0));
        p.step(&mut anm, &press(input::BOMB));
        assert!(p.bombing && p.bomb_cooldown == 0);
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 5;
        p.forced_bomb = 1;
        p.deathbomb = 1;
        p.step(&mut anm, &press(0));
        assert!(p.bombing && p.bomb_cooldown == 5);
        p.step(&mut anm, &press(0));
        assert_eq!(p.bomb_cooldown, 4);
    }

    // 09 §1.1: the cooldown blocks a normal start, is decremented atop Player_UpdateBomb, but not on a forced start.
    #[test]
    fn bomb_cooldown() {
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 2;
        p.update_bomb(&mut anm, &press(input::BOMB), &mut None);
        assert!(!p.bombing && p.bomb_cooldown == 1);
        p.update_bomb(&mut anm, &press(0), &mut None);
        assert_eq!(p.bomb_cooldown, 0);
        p.update_bomb(&mut anm, &press(input::BOMB), &mut None);
        assert!(p.bombing);
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 16;
        p.forced_bomb = 1;
        p.deathbomb = 1;
        p.update_bomb(&mut anm, &press(0), &mut None);
        assert!(p.bombing && p.bomb_cooldown == 16);
    }

    // 09 §4.5: circle = point test on the enemy centre, `≤ r²`; rectangles overlap the box inclusively.
    #[test]
    fn damage_area_enemy_overlap() {
        let circle = Area {
            pos: [100.0, 100.0],
            r: 10.0,
            ..Area::default()
        };
        assert!(circle.hits_enemy([110.0, 100.0], [64.0, 64.0]));
        assert!(!circle.hits_enemy([111.0, 100.0], [64.0, 64.0]));
        let rect = Area {
            pos: [100.0, 100.0],
            size: [20.0, 40.0],
            ..Area::default()
        };
        assert!(rect.hits_enemy([120.0, 100.0], [20.0, 2.0]));
        assert!(!rect.hits_enemy([120.5, 100.0], [20.0, 2.0]));
        assert!(rect.hits_enemy([100.0, 125.0], [2.0, 10.0]));
        let rot = Area {
            rot: FRAC_PI_2,
            ..rect
        };
        assert!(rot.hits_enemy([120.0, 100.0], [2.0, 2.0]));
        assert!(
            rect.hits_enemy([100.0, 112.0], [2.0, 2.0])
                && !rot.hits_enemy([100.0, 112.0], [2.0, 2.0])
        );
    }

    #[test]
    fn tint_ramp() {
        let (mut p, mut anm) = ready(4);
        p.bomb.duration = 200;
        p.bomb.timer.set(30);
        p.tint(&mut anm, 0x80404040);
        assert_eq!(anm.stage_tint, Some(0x80606060));
        assert!(anm.bomb_drew && !anm.tint_on);
        p.bomb.timer.set(100);
        p.tint(&mut anm, 0x802020D0);
        assert_eq!(anm.stage_tint, Some(0x80404098));
        anm.stage_tint = None;
        p.tint(&mut anm, 0x802020D0);
        assert_eq!(anm.stage_tint, Some(0x802020D0));
        anm.stage_tint = None;
        p.bomb.timer.set(170);
        p.tint(&mut anm, 0x80F00000);
        assert_eq!(anm.stage_tint, Some(0x80B84040));
    }

    #[test]
    fn youmu_dash_and_return() {
        let (mut p, mut anm) = ready(10);
        let start = p.pos;
        p.step(&mut anm, &press(input::BOMB));
        for _ in 0..70 {
            p.step(&mut anm, &press(0));
        }
        assert_eq!(p.pos[1], 32.0);
        for _ in 0..80 {
            p.step(&mut anm, &press(0));
        }
        assert!((p.pos[1] - start[1]).abs() < 1.0 && p.speed_mult == [1.0; 2]);
    }
}
