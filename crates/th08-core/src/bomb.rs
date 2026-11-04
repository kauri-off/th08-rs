//! Bombs: Player_UpdateBomb, Bomb_Start, the 16 bombs + dissolve spell, tint (09 §7, 14 §1–§2); areas (09 §8).
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer, table_index};
use crate::builtin::Builtin;
use crate::drawlist::{PLAYFIELD, Recorder};
use crate::effect::{self, FxEnv, WHITE};
use crate::game::Sys;
use crate::gamemgr::{F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE};
use crate::input;
use crate::item::ItemManager;
use crate::player::{ANM_SLOT, DISSOLVE_BEGIN, DISSOLVE_END, Player, Stats};
use crate::types::{D3dColor, EffectIdx, lo8};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

// 192 damage + 192 cancel areas, 0x40 each.
pub const AREAS: usize = 192;
// 128 × 0x16F0 bomb objects at player +0x1028.
pub const OBJS: usize = 128;
// Playfield origin 0x164D2DC / 0x164D2E0 added to VM positions.
const ORIGIN: [f32; 2] = [32.0, 16.0];
// Draw prio of bomb shakes/flashes (13 §3: every code call passes 0x15).
const SCREEN_PRIO: u16 = 0x15;

// Damage / cancel area, 0x40 bytes.
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
        let (sn, c) = crate::fmath::sin_cos_f(-self.rot);
        let d = [dx * c - dy * sn, dx * sn + dy * c];
        -w2 <= s[0] / 2.0 + d[0]
            && d[0] - s[0] / 2.0 <= w2
            && -h2 <= s[1] / 2.0 + d[1]
            && d[1] - s[1] / 2.0 <= h2
    }

    // Cancel-area test (Player_InBombArea): circle distance² < r²; rectangle inside the rotated bounds.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let (dx, dy) = (p[0] - self.pos[0], p[1] - self.pos[1]);
        if self.r > 0.0 {
            return dx * dx + dy * dy < self.r * self.r;
        }
        let (s, c) = crate::fmath::sin_cos_f(-self.rot);
        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
        rx.abs() <= self.size[0] / 2.0 && ry.abs() <= self.size[1] / 2.0
    }
}

// Port-only queue: areas created by effect updates (Yukari's boundaries, 14 §2.13); added at the next Player_Calc.
// (damage, area) pairs; held in `Ctx::bomb_areas` / `Sys::areas`.
pub type AreaQueue = Vec<(bool, Area)>;

// Bomb object +0 state.
#[repr(i32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjState {
    #[default]
    Free = 0,
    Active = 1,
    Exploding = 2,
}

// Bomb object (0x16F0 bytes); fields keep their values between bombs.
#[derive(Clone, Debug, Default)]
pub struct BombObj {
    // +0 state.
    pub state: ObjState,
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

// Player bomb fields.
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
    // Object 0's +0x1B8 VM and the next ones' used as plain VM slots (Marisa beams, Yukari LS).
    pub vms: [AnmVm; 5],
    // Port-only: spell name and cut-in data for the GUI banner.
    pub name: Option<Builtin>,
    pub last_spell: bool,
    // Port-only: character row of SPECS (bomb_char).
    pub ch: u8,
    // Port-only: "all items attracted" request for the item manager.
    pub attract: bool,
    // +0xFF0 counter (Reimu LS blast index, ring index n).
    pub n: i32,
    // Port-only scratch: start S, dash target P, return start Q (Alice, Youmu); Sakuya effect-20 slot (None = dummy).
    pub start: [f32; 3],
    pub dash: [f32; 3],
    pub back: [f32; 3],
    pub fx: Option<EffectIdx>,
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
            name: None,
            last_spell: false,
            ch: 0,
            attract: false,
            n: 0,
            start: [0.0; 3],
            dash: [0.0; 3],
            back: [0.0; 3],
            fx: None,
        }
    }
}

// (name, duration, invulnerability, isLastSpell) per character (Reimu, Yukari, Marisa, Alice, Sakuya, Remilia,
// Youmu, Yuyuko) × (bomb, last spell).
type BombSpec = (Builtin, i32, i32, bool);
const SPECS: [[BombSpec; 2]; 8] = [
    [
        (
            Builtin::Bomb {
                chara: 0,
                last: false,
            },
            200,
            260,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 0,
                last: true,
            },
            200,
            260,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 1,
                last: false,
            },
            150,
            200,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 1,
                last: true,
            },
            250,
            300,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 2,
                last: false,
            },
            300,
            350,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 2,
                last: true,
            },
            350,
            380,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 3,
                last: false,
            },
            210,
            250,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 3,
                last: true,
            },
            230,
            280,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 4,
                last: false,
            },
            250,
            290,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 4,
                last: true,
            },
            320,
            350,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 5,
                last: false,
            },
            240,
            290,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 5,
                last: true,
            },
            280,
            320,
            true,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 6,
                last: false,
            },
            220,
            270,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 6,
                last: true,
            },
            250,
            300,
            false,
        ),
    ],
    [
        (
            Builtin::Bomb {
                chara: 7,
                last: false,
            },
            300,
            350,
            false,
        ),
        (
            Builtin::Bomb {
                chara: 7,
                last: true,
            },
            300,
            350,
            true,
        ),
    ],
];
// Youmu LS passes isLastSpell = 0 (14 §2.12 quirk); the dissolve spell is 40 frames in spell practice.
const DISSOLVE: BombSpec = (Builtin::Dissolve, 120, 200, false);
// Colours (D3DCOLOR 0xAARRGGBB) by spec row. Table 0x4C6100, the Reimu LS blasts.
const BLAST_COLOURS: [D3dColor; 7] = [
    D3dColor(0x8FFF_FFFF),
    D3dColor(0x8F00_00FF),
    D3dColor(0x8FFF_00FF),
    D3dColor(0x8FFF_0000),
    D3dColor(0x8FFF_FF00),
    D3dColor(0x8F00_FF00),
    D3dColor(0x8F00_FFFF),
];
// Alice effect 0x28 / 0x2D colours.
const DOLL_FLASH: [D3dColor; 5] = [
    WHITE,
    D3dColor(0xFFFF_D0D0),
    D3dColor(0xFFFF_B0B0),
    D3dColor(0xFFFF_8080),
    D3dColor(0xFFFF_4040),
];
const DOLL_OCTAGON: [D3dColor; 4] = [
    WHITE,
    D3dColor(0xFFFF_D0D0),
    D3dColor(0xFFFF_8080),
    D3dColor(0xFFFF_0000),
];
// Alice LS corner rings 0x2D at T = 120.
const DOLL_LS_CORNERS: [([f32; 2], D3dColor); 4] = [
    ([64.0, 96.0], D3dColor(0xFF00_00F0)),
    ([64.0, 352.0], D3dColor(0xFFF0_0000)),
    ([320.0, 352.0], D3dColor(0xFF00_F000)),
    ([320.0, 96.0], D3dColor(0xFF00_F0F0)),
];
// Effect 12 at the bomb start, also the dissolve ring 0x32.
const START_FX: D3dColor = D3dColor(0xFF40_40FF);
// Alice flash (3: 8, 1, ·) at the blast and the end.
const ALICE_FLASH: D3dColor = D3dColor(0x8FFF_FFFF);
// Sakuya knife hit effect 0, Sakuya LS hit flash every third knife.
const KNIFE_HIT: D3dColor = D3dColor(0xFFFF_80FF);
const KNIFE_LS_FLASH: D3dColor = D3dColor(0x2080_80FF);
// Youmu / Youmu LS effects 0x28 and 0x30, the slash flash.
const YOUMU_FX: D3dColor = D3dColor(0xFF80_80FF);
const YOUMU_LS_FX: D3dColor = D3dColor(0xFFFF_8080);
const YOUMU_FLASH: D3dColor = D3dColor(0xEFFF_FFFF);
// Yuyuko LS butterfly-ring flash.
const YUYUKO_LS_FLASH: D3dColor = D3dColor(0xE0F0_F0F0);
// RGB only (alpha byte 0): spark-ring colour interpolation white → red, playfield rectangles.
const RGB_WHITE: D3dColor = D3dColor(0x00FF_FFFF);
const RGB_RED: D3dColor = D3dColor(0x00FF_0000);
// Background tints C; alpha 0x80.
const TINT_ALPHA: u32 = 0x8000_0000;
const TINT: D3dColor = D3dColor(0x8040_4040);
const TINT_LS: D3dColor = D3dColor(0x8020_20D0);
const TINT_SAKUYA_LS: D3dColor = D3dColor(0x8020_2080);
const TINT_REMILIA: D3dColor = D3dColor(0x80D0_2020);
const TINT_REMILIA_LS: D3dColor = D3dColor(0x80F0_0000);
const TINT_YOUMU_2: D3dColor = D3dColor(0x8000_0030);
const TINT_YUYUKO_LS: D3dColor = D3dColor(0x8080_2020);
// Yuyuko (script, angular speed, radial speed) per group of 16 butterflies.
const YUYUKO_GROUPS: [(usize, f32, f32); 4] = [
    (0x12, PI / 240.0, 2.0),
    (0x13, -PI / 240.0, 2.0),
    (0x12, PI / 200.0, 1.5),
    (0x13, -PI / 200.0, 1.5),
];
// Boundary angles of Yukari.
const BOUNDARY_ANGLES: [f32; 4] = [PI / 4.0, 3.0 * PI / 8.0, PI / 2.0, 5.0 * PI / 8.0];

// Vec3_FromPolar (05 §10).
fn polar(a: f32, r: f32) -> [f32; 2] {
    let v = crate::fmath::polar(a, r);
    [v[0], v[1]]
}

// Reimu orbs `call; fmul` (05 §10 trig sites): sin/cos·r rounded once, then + centre.
fn sin_r(a: f32, r: f32) -> f32 {
    crate::fmath::sin_mul(a, f64::from(r))
}

fn cos_r(a: f32, r: f32) -> f32 {
    crate::fmath::cos_mul(a, f64::from(r))
}

fn xy(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[1]]
}

fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

// Screen effects (13 §3): shake (1: p0, p1, p2), flash (3: p0, p1, colour). p0 is the duration the
// original passes in EDX.
fn shake(dur: i32, a0: i32, a1: i32, sys: &mut Sys) {
    sys.fx.screen(1, dur, a0, a1, 0, SCREEN_PRIO);
}

fn flash(dur: i32, repeats: i32, colour: D3dColor, sys: &mut Sys) {
    sys.fx
        .screen(3, dur, repeats, colour.0.cast_signed(), 0, SCREEN_PRIO);
}

// Effect_Spawn (13 §2.2) of `n` effects; None = the dummy.
fn spawn(
    anm: &mut AnmManager,
    sys: &mut Sys,
    kind: u8,
    pos: [f32; 3],
    n: i32,
    c: D3dColor,
) -> Option<EffectIdx> {
    sys.fx.spawn_at(anm, sys.rng, FxEnv::new(kind, pos, c), n)
}

// Object index (< OBJS) as the original's int loop counter.
#[expect(clippy::cast_possible_wrap, reason = "k < 128")]
fn obj_int(k: usize) -> i32 {
    k as i32
}

// Damage-area fields the bombs set after the creator: +0x38 period, +0x34 max total, +0x3D no hit effect.
#[derive(Clone, Copy)]
struct Hit {
    period: i32,
    max: i32,
    no_spark: bool,
}

impl Hit {
    // Period only: max 0 (unlimited), hit effect on.
    const fn every(period: i32) -> Self {
        Self {
            period,
            max: 0,
            no_spark: false,
        }
    }
}

// Ring 0x35 arguments of `spark_ring`.
#[derive(Clone, Copy)]
struct SparkRing {
    // Ring counter n (fixed slot 4 + n % 4).
    n: i32,
    waves: i32,
    // Scale.x start → end over 30 frames; None = constant 64.
    width: Option<(f32, f32)>,
    // VM pos end (radius, amplitude).
    end: [f32; 3],
    // Marisa LS odd n: script 0x5C.
    alt: bool,
}

// Table 0x4C7AD0: teams take the human member's bomb for even indexes, the youkai member's for odd.
pub fn bomb_char(shot: u8, index: usize) -> u8 {
    if shot >= 4 {
        shot - 4
    } else {
        shot * 2 + (index & 1) as u8
    }
}

impl Player {
    // First inactive slot of the half; index 191 reused when full (creators).
    fn free_slot(v: &[Area]) -> usize {
        v.iter().position(|a| !a.active).unwrap_or(AREAS - 1)
    }

    // Creators 0x44DF00 / 0x44DE60 / 0x44E040 / 0x44DFA0.
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
    fn dc(&mut self, pos: [f32; 2], r: f32, dr: f32, dmg: i32, life: i32, hit: Hit) -> usize {
        let i = self.damage_circle(pos, r, dr, dmg, life);
        let a = &mut self.damage_areas[i];
        (a.period, a.max, a.no_spark) = (hit.period, hit.max, hit.no_spark);
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

    pub fn take_queued_areas(&mut self, q: &mut AreaQueue) {
        for (damage, a) in std::mem::take(q) {
            let v = if damage {
                &mut self.damage_areas
            } else {
                &mut self.cancel_areas
            };
            let i = Self::free_slot(v);
            v[i] = a;
        }
    }

    // Area update, start of Player_Calc: an area created with life 0 exists for exactly one pass.
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

    // Player_UpdateBomb 0x44C650.
    pub fn update_bomb(
        &mut self,
        anm: &mut AnmManager,
        inp: &input::Input,
        items: &mut Option<&mut ItemManager>,
        sys: &mut Sys,
    ) {
        // 09 §1.1: a forced-start call (+6 set, +0xE2A68 == 1) jumps straight to the bomb start, past the
        // +0xE2A6C decrement, the bombing branch and the start tests.
        if self.forced_bomb != 0 && self.deathbomb == 1 {
            self.start_bomb(anm, true, sys);
            self.bomb_calc(anm, items, sys);
            self.bomb.timer.tick(anm.speed);
            return;
        }
        if self.bomb_cooldown != 0 {
            self.bomb_cooldown -= 1;
        }
        if !self.bombing {
            // A level test on the held bomb bit (no edge), so holding bomb chains bombs.
            let start = inp.down(input::BOMB)
                && !self.env.dialogue
                && self.deathbomb != 0
                && self.stats.bombs >= 1.0
                && self.bomb_cooldown == 0;
            // +0xE2A7C drops on a failed normal start test, so it stays 1 through the end frame (09 §1.1).
            if !start {
                self.bomb_active = false;
                return;
            }
            // Special mode: SE 0x29 only on a new press (the function's only edge test); +0xE2A7C kept (09 §1.1).
            if self.special(*sys.flags) || *sys.flags & F_SPELL_PRACTICE != 0 {
                if inp.pressed(input::BOMB) {
                    sys.sound.play(0x29);
                }
                return;
            }
            // State 1 test is redundant: the counter is 0 throughout state 1 (09 §6.3).
            if matches!(self.state, 1) {
                return;
            }
            // The bomb calc fn runs immediately on the start frame; that branch returns before the gauge add.
            self.start_bomb(anm, false, sys);
            self.bomb_calc(anm, items, sys);
            self.bomb.timer.tick(anm.speed);
            return;
        }
        if self.bomb.timer.cur < self.bomb.duration {
            self.bomb_calc(anm, items, sys);
            self.bomb.timer.tick(anm.speed);
        } else {
            // End: speed multipliers 1, bombing 0; no tint-off here (06 §10.2); index 4 per 14 §2.17.
            // +0xE2A7C is not cleared here (09 §1.1).
            self.speed_mult = [1.0; 2];
            self.bombing = false;
            sys.gui.push(crate::gui::Req::PlayerHide);
            if self.bomb.index == 4 {
                *sys.flags &= !(F_LAST_SPELL | F_LAST_CAPTURED);
                self.dissolve_req |= DISSOLVE_END;
                flash(30, 1, WHITE, sys);
            }
        }
        // Quirk kept: forced gauge add also on the end frame, with the old index.
        if self.bomb.index < 4 {
            let d = 26000 / self.bomb.duration.max(1);
            self.stats
                .gauge_add(if self.bomb.index & 1 == 1 { d } else { -d });
        }
    }

    // Bomb start: index from focus / last-spell request, bomb cost, checksum (0x44E2E0), rank −200,
    // counter += 6 (cap sht +8). Not implemented: statistics bit.
    fn start_bomb(&mut self, anm: &mut AnmManager, forced: bool, sys: &mut Sys) {
        self.forced_bomb = 0;
        // HUD bombs field := 2 (12 §5.3 writers).
        sys.panel.mark_bombs(2);
        let mut index = 4;
        // In special mode (bits 7–8) the whole cost block is skipped: no unfreeze, bombs, counters or checksums.
        if !self.special(*sys.flags) {
            // Order: silhouette (VM bit 17) off, deathbomb ring ended, bullets unfrozen, AnmMgr_TintOff.
            self.vm.flags &= !anmvm::F_COLOR2;
            sys.fx.free(effect::FIXED_BASE + effect::FIX_DEATH);
            self.set_freeze_bullets(sys.flags, false);
            anm.tint_off();
            let rng = &mut *sys.rng;
            index = self.focus.min(1) as usize;
            if self.last_spell != 0 {
                index = 1 - index + 2;
                let b = self.stats.bombs;
                // Forced or < 2 bombs: GameMgr_SetBombs(0) 0x406F30, no RNG; 2 bombs: AddBombs(−2), 4 draws.
                if forced || b < 2.0 {
                    self.bomb.used = b as i32;
                    self.stats.bombs = 0.0;
                } else {
                    self.bomb.used = 2;
                    self.stats.add_bombs(-2.0, rng);
                }
                // HSCR +0x78, 0x164CFAC (15 §2.3).
                self.stats.rec.last_spells += 1;
            } else {
                self.stats.add_bombs(-1.0, rng);
                // HSCR +0x74, 0x164CFA8 (15 §2.3).
                self.stats.rec.bombs += 1;
            }
            // GameMgr_AddBombCount 0x44E2E0(1): stats +0x84 and a checksum.
            self.stats.bombs_used += 1;
            Stats::checksum(rng);
        }
        // Port-only debug override (TH08_BOMB=<0–4>, 4 = dissolve spell).
        if let Some(i) = self.debug_bomb {
            index = i;
        }
        self.last_spell = 0;
        self.bombing = true;
        self.bomb_active = true;
        self.bombs_started += 1;
        self.bomb.index = index;
        self.bomb.timer.set(0);
        self.bomb.duration = 999;
        self.bomb.ch = bomb_char(self.shot_type, index);
        self.stats.rank(sys.rank, -200);
        self.deathbomb = (self.deathbomb + 6).min(self.sht.deathbomb);
    }

    // Bomb_Start 0x40BE30: banner, duration, state 3 + invulnerability, ring, objects freed (other
    // fields kept), items attracted, origin = p.
    fn bomb_begin(&mut self, anm: &mut AnmManager, spec: BombSpec, sys: &mut Sys) {
        self.bomb.name = Some(spec.0);
        self.bomb.last_spell = spec.3;
        let face = if self.bomb.index == 4 {
            -1
        } else {
            i32::from(self.bomb.ch & 1)
        };
        sys.gui.push(crate::gui::Req::PlayerShow {
            face,
            name: spec.0,
            last: spec.3,
        });
        self.bomb.duration = spec.1;
        self.state = 3;
        self.state_timer.set(spec.2);
        crate::effect::invuln_ring(sys.fx, anm, sys.rng, self.pos, spec.2);
        self.invuln_ring = true;
        self.bomb
            .objs
            .iter_mut()
            .for_each(|o| o.state = ObjState::Free);
        self.bomb.attract = true;
        self.bomb.origin = self.pos;
    }

    // Bomb calc fn dispatch (table 0x4C7AD0 fns 0–4); Bomb_Start runs on the T = 0 frame of each.
    fn bomb_calc(
        &mut self,
        anm: &mut AnmManager,
        items: &mut Option<&mut ItemManager>,
        sys: &mut Sys,
    ) {
        let _ = items;
        let t = self.bomb.timer.cur;
        let adv = self.bomb.timer.cur != self.bomb.timer.prev;
        let ls = self.bomb.index & 2 != 0;
        if self.bomb.index == 4 {
            if t == 0 && adv {
                let practice = *sys.flags & F_SPELL_PRACTICE != 0;
                self.bomb_begin(
                    anm,
                    if practice {
                        (DISSOLVE.0, 40, DISSOLVE.2, DISSOLVE.3)
                    } else {
                        DISSOLVE
                    },
                    sys,
                );
                self.dissolve_start(anm, sys);
            }
            return;
        }
        let ch = self.bomb.ch as usize;
        if t == 0 && adv {
            self.bomb_begin(anm, SPECS[ch][usize::from(ls)], sys);
        }
        let b = Bf {
            t,
            adv,
            ls,
            gs: anm.speed,
        };
        match ch {
            0 if ls => self.bomb_reimu_ls(anm, b, sys),
            0 => self.bomb_reimu(anm, b, sys),
            1 => self.bomb_yukari(anm, b, sys),
            2 => self.bomb_marisa(anm, b, sys),
            3 => self.bomb_alice(anm, b, sys),
            4 => self.bomb_sakuya(anm, b, sys),
            5 => self.bomb_remilia(anm, b, sys),
            6 => self.bomb_youmu(anm, b, sys),
            _ => self.bomb_yuyuko(anm, b, sys),
        }
    }

    // "At T = n": the timer advanced and equals n, so nothing fires twice in slow motion.
    fn at(b: Bf, n: i32) -> bool {
        b.adv && b.t == n
    }

    fn obj_script(anm: &mut AnmManager, rng: &mut crate::rng::Rng, o: &mut BombObj, script: usize) {
        anm.set_script(rng, &mut o.vm, ANM_SLOT, script);
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
    fn run_obj(anm: &mut AnmManager, rng: &mut crate::rng::Rng, o: &mut BombObj) -> bool {
        o.vm.script.is_none() || o.vm.run(anm, rng)
    }

    fn homing_target(&self) -> Option<[f32; 2]> {
        let t = self.targets[0];
        (t[0] > -100.0).then_some([t[0], t[1]])
    }

    // Shared orb setup of Reimu's bombs.
    fn reimu_orbs(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng) {
        let p = self.pos;
        // Start angles are a running f32 sum (0x40C0F0 fld/fadd/fstp), not −π + i·π/8.
        let mut a = -PI;
        for i in 0..16 {
            let c = self.cancel_circle(xy(p), 96.0, 0.0, 200, 6);
            let orb = Hit {
                period: 2,
                max: 200,
                no_spark: true,
            };
            let d = self.dc(xy(p), 64.0, 0.0, 5, 200, orb);
            let o = &mut self.bomb.objs[i];
            Self::obj_script(anm, rng, o, 0x13);
            o.angle = a;
            a += PI / 8.0;
            (o.pos, o.centre, o.f8, o.state) = (p, p, 0.0, ObjState::Active);
            (o.carea, o.darea) = (Some(c), Some(d));
        }
    }

    // Orb impact, rates 64/15 and 128/15 exact. Quirk kept: the C circle is centred on the player, not the orb.
    fn orb_explode(&mut self, anm: &mut AnmManager, i: usize, ls: bool, sys: &mut Sys) {
        self.areas_off(i);
        let p = xy(self.pos);
        let q = self.bomb.objs[i].pos;
        self.cancel_circle(p, 64.0, 64.0 / 15.0, 30, 6);
        if ls {
            let hit = Hit {
                max: 50,
                ..Hit::every(5)
            };
            self.dc(xy(q), 64.0, 128.0 / 15.0, 25, 15, hit);
        } else {
            self.dc(xy(q), 64.0, 12.8, 500, 12, Hit::every(4));
        }
        spawn(anm, sys, 6, q, 8, WHITE);
        let o = &mut self.bomb.objs[i];
        o.state = ObjState::Exploding;
        o.vm.pending_interrupt = 1;
        sys.sound.play_at(0xF, q[0]);
        shake(16, 8, 0, sys);
    }

    // 14 §2.1 霊符「夢想妙珠」 0x40C010. (The original's discarded velocity/8 temporary has no effect.)
    fn bomb_reimu(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let p = self.pos;
        if Self::at(b, 0) {
            spawn(anm, sys, 12, p, 1, START_FX);
            self.reimu_orbs(anm, sys.rng);
            sys.sound.play(0xD);
        }
        // T < 40 spin-out: pos = centre + (sin a, cos a)·radius, radius += 3.2.
        if b.t < 40 {
            for (i, o) in self.bomb.objs[..16]
                .iter_mut()
                .enumerate()
                .filter(|(_, o)| o.state == ObjState::Active)
            {
                o.angle = anmvm::wrap_add(o.angle, if i % 2 == 0 { -PI / 60.0 } else { PI / 60.0 });
                let prev = o.pos;
                o.pos = [
                    o.centre[0] + sin_r(o.angle, o.f8),
                    o.centre[1] + cos_r(o.angle, o.f8),
                    o.pos[2],
                ];
                o.f8 += 3.2;
                o.v = [o.pos[0] - prev[0], o.pos[1] - prev[1], 0.0];
            }
            for i in 0..16 {
                if self.bomb.objs[i].state == ObjState::Active {
                    self.follow(i);
                }
            }
        }
        if Self::at(b, 40) {
            for o in &mut self.bomb.objs[..16] {
                o.fc = len(xy(o.v));
                o.angle = crate::fmath::atan2_f(o.v[1], o.v[0]);
                (o.counter, o.f8) = (0, 8.0);
            }
        }
        // T ≥ 40: homing on advanced frames (1e-6 floor port-only), one-frame C r 128, impact at max or T ≥ dur − 30.
        if b.t >= 40 {
            let target = self.homing_target().unwrap_or(xy(p));
            for i in 0..16 {
                if self.bomb.objs[i].state == ObjState::Active && b.adv {
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
                        self.orb_explode(anm, i, false, sys);
                    }
                }
                let o = &mut self.bomb.objs[i];
                if o.state == ObjState::Free {
                    continue;
                }
                o.pos[0] += o.v[0] * b.gs;
                o.pos[1] += o.v[1] * b.gs;
                if o.state == ObjState::Active {
                    self.follow(i);
                } else {
                    if b.adv {
                        o.counter += 1;
                    }
                    if o.counter >= 30 {
                        o.state = ObjState::Free;
                    }
                }
            }
        }
        for o in self.bomb.objs[..16]
            .iter_mut()
            .filter(|o| o.state != ObjState::Free)
        {
            Self::run_obj(anm, sys.rng, o);
        }
    }

    // 14 §2.2 神霊「夢想封印　瞬」 0x40C910: orb i explodes at T ≥ dur − 40 − i; blasts every 20 frames from T = 40.
    fn bomb_reimu_ls(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        if Self::at(b, 0) {
            spawn(anm, sys, 12, self.pos, 1, START_FX);
            self.reimu_orbs(anm, sys.rng);
            self.bomb.n = 0;
            sys.sound.play(0xD);
        }
        for i in 0..16 {
            let o = &mut self.bomb.objs[i];
            if o.state != ObjState::Active {
                continue;
            }
            o.angle = anmvm::wrap_add(o.angle, if i % 2 == 0 { -PI / 60.0 } else { PI / 60.0 });
            o.pos = [
                o.centre[0] + sin_r(o.angle, o.f8),
                o.centre[1] + cos_r(o.angle, o.f8),
                o.pos[2],
            ];
            if b.t < 40 {
                o.f8 += if i % 2 == 0 { 2.4 } else { 1.2 };
            }
            if b.t >= self.bomb.duration - 40 - obj_int(i) {
                // Quirk kept: the follow write still runs through the old slots, so the impact C circle
                // that reused the just-freed slot moves from p to the orb.
                let (c, d) = (o.carea, o.darea);
                self.orb_explode(anm, i, true, sys);
                let q = xy(self.bomb.objs[i].pos);
                if let Some(a) = d {
                    self.damage_areas[a].pos = q;
                }
                if let Some(a) = c {
                    self.cancel_areas[a].pos = q;
                }
            } else {
                self.follow(i);
            }
        }
        if b.t >= 40 && b.t % 20 == 0 && b.adv {
            // Quirk kept: the first blast uses object 16, every later one reuses object 17. The VM script is set
            // before the RNG point; no player*.anm script draws RNG at time 0, so the order is moot.
            let k = 16 + table_index(self.bomb.n);
            self.bomb.n = 1;
            let colour = BLAST_COLOURS[table_index((b.t / 20) % 7)];
            let point = if let Some(p) = self.homing_target() {
                p
            } else {
                let x = sys.rng.next_f32() * 320.0 + 32.0;
                [x, sys.rng.next_f32() * 384.0 + 32.0]
            };
            let pt = [point[0], point[1], 0.0];
            spawn(anm, sys, 0x31, pt, 1, colour);
            spawn(anm, sys, 0x37, pt, 1, colour);
            let c = self.cancel_circle(point, 64.0, 64.0 / 15.0, 30, 6);
            let d = self.dc(point, 64.0, 128.0 / 15.0, 400, 15, Hit::every(2));
            let o = &mut self.bomb.objs[k];
            Self::obj_script(anm, sys.rng, o, 0x14);
            o.vm.color = colour.to_bgra();
            (o.pos, o.state, o.carea, o.darea) = (pt, ObjState::Active, Some(c), Some(d));
            sys.sound.play(0xF);
            shake(16, 8, 0, sys);
            flash(8, 1, colour, sys);
        }
        for k in 0..OBJS {
            if self.bomb.objs[k].state == ObjState::Free {
                continue;
            }
            if k >= 16 {
                self.follow(k);
            }
            if Self::run_obj(anm, sys.rng, &mut self.bomb.objs[k]) && k >= 16 {
                self.bomb.objs[k].state = ObjState::Free;
            }
        }
    }

    // Active objects 0..n at pos + offset + playfield origin, z 0, optional rotation z. Step: rotation / pos writes.
    fn step_objs(
        &mut self,
        anm: &AnmManager,
        n: usize,
        mode: i32,
        rot: impl Fn(&BombObj) -> Option<f32>,
    ) {
        for o in self.bomb.objs[..n]
            .iter_mut()
            .filter(|o| o.state != ObjState::Free)
        {
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
            anm.prepare(&mut o.vm, mode);
        }
    }

    fn emit_objs(&self, anm: &AnmManager, rec: &mut Recorder, n: usize, mode: i32) {
        for o in self.bomb.objs[..n]
            .iter()
            .filter(|o| o.state != ObjState::Free)
        {
            anm.emit_mode_z(rec, &o.vm, mode, 0.0);
        }
    }

    // 14 §2.3 / §2.4 Alice 0x40D430 / 0x40D970: the doll is option 0.
    fn bomb_alice(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let ls = b.ls;
        let centre = [192.0, 224.0, 0.0];
        if Self::at(b, 0) {
            sys.sound.play(0xD);
            if ls {
                self.bomb.start = self.pos;
            } else {
                self.bomb.start = self.options[0].pos;
                sys.sound.play(6);
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
            self.dc(q, 32.0, 0.0, 40, 0, Hit::every(1));
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
            spawn(anm, sys, 0x28, q, 1, DOLL_FLASH[table_index(b.t - 60) / 4]);
        }
        let (blast, oct0, end) = if ls { (120, 130, 180) } else { (90, 100, 150) };
        if Self::at(b, blast) {
            sys.sound.play(0xF);
            for k in [0x2A, 0x2B, 0x2C] {
                spawn(anm, sys, k, q, 1, WHITE);
            }
            if ls {
                for (p, c) in DOLL_LS_CORNERS {
                    spawn(anm, sys, 0x2D, [p[0], p[1], 0.0], 1, c);
                }
            }
            let c = self.cancel_circle(xy(q), 1.0, 5.0, 110, 6);
            let _ = c;
            self.dc(xy(q), 1.0, 5.0, 70, 110, Hit::every(5));
            if !ls {
                shake(24, 8, 0, sys);
                flash(8, 1, ALICE_FLASH, sys);
            }
        }
        if b.adv && (oct0..oct0 + 40).contains(&b.t) && (b.t - oct0) % 10 == 0 {
            spawn(
                anm,
                sys,
                0x2D,
                q,
                1,
                DOLL_OCTAGON[table_index((b.t - oct0) / 10)],
            );
        }
        if Self::at(b, end) {
            flash(8, 1, if ls { WHITE } else { ALICE_FLASH }, sys);
            shake(24, 8, 0, sys);
            sys.sound.play(0x19);
        }
        if Self::at(b, self.bomb.duration - 1) {
            self.options[0].state = 1;
            self.options[0].timer.set(0);
        }
    }

    // 14 §2.5 / §2.6 Marisa 0x40E3B0 / 0x40E780: five beam VMs in object 0's memory.
    fn bomb_marisa(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let p = self.pos;
        if Self::at(b, 0) {
            if b.ls {
                sys.sound.play(0xD);
            }
            sys.sound.play(0x13);
            self.bomb.objs[0].pos = p;
            let base = if b.ls { 0x23 } else { 0x1E };
            for (i, vm) in self.bomb.vms.iter_mut().enumerate() {
                anm.set_script(sys.rng, vm, ANM_SLOT, base + i);
            }
            self.speed_mult = [0.2; 2];
            sys.fx.screen(7, 16, 120, 60, 120, SCREEN_PRIO);
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
            let ring = SparkRing {
                n,
                waves: 0,
                width: Some((32.0, 64.0)),
                end: [128.0, 0.0, 0.0],
                alt: n % 2 == 1,
            };
            self.spark_ring(anm, ring, sys);
            self.bomb.n += 1;
        }
        for vm in &mut self.bomb.vms {
            if vm.script.is_some() {
                vm.run(anm, sys.rng);
            }
        }
    }

    // Ring effect 0x35 in fixed slot 4 + n % 4: radius/amplitude, width, fade, white → red;
    // `alt` = Marisa LS odd n → script 0x5C. `width` None = constant 64 (Remilia's 64 → 64 interpolation).
    fn spark_ring(&mut self, anm: &mut AnmManager, s: SparkRing, sys: &mut Sys) {
        let p = self.pos;
        let slot = 4 + table_index(s.n.rem_euclid(4));
        let i = sys
            .fx
            .spawn_fixed_param(anm, sys.rng, FxEnv::new(0x35, p, WHITE), slot);
        {
            let x = &mut sys.fx[i];
            if s.alt {
                anm.set_script(sys.rng, &mut x.vm, effect::SCRIPT_SLOT, 0x5C);
                x.vm.color = WHITE.to_bgra();
            }
            x.ring.segments = 32;
            x.ring.waves = s.waves as f32;
            x.vm.interp_pos_from(30, 4, [0.0; 3], s.end);
            if let Some((a, b)) = s.width {
                x.vm.interp_scale_from(30, 1, [a, 0.0], [b, 0.0]);
            } else {
                x.vm.scale = [64.0, 0.0];
                x.vm.flags |= anmvm::F_SCALE_DIRTY;
            }
            x.vm.interp_alpha_from(30, 3, 255, 0);
            x.vm.interp_color_from(30, 0, RGB_WHITE.0, RGB_RED.0);
            x.vm.run(anm, sys.rng);
        }
        sys.sound.play_at(0x11, p[0]);
    }

    // 14 §2.7 / §2.8 Sakuya 0x40FCD0 / 0x4103F0: knives k and k + per appear together every second frame.
    fn bomb_sakuya(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let ls = b.ls;
        let p = self.pos;
        let (count, per, window) = if ls { (128, 64, 148) } else { (96, 48, 116) };
        if Self::at(b, 0) {
            self.bomb.objs[..count]
                .iter_mut()
                .for_each(|o| o.state = ObjState::Free);
            self.speed_mult = [0.5; 2];
            self.bomb.fx = spawn(anm, sys, 20, p, 1, WHITE);
            sys.sound.play(5);
            if ls {
                shake(50, 4, 1, sys);
            }
        }
        if b.t <= 60
            && let Some(i) = self.bomb.fx
        {
            sys.fx.write_pos_unchecked(i, p);
        }
        if (20..window).contains(&b.t) {
            if !ls {
                sys.sound.play(6);
                shake(120, 4, 1, sys);
            }
            for k in 0..count {
                if !Self::at(b, obj_int(k % per) * 2 + 20) {
                    continue;
                }
                // Quirk kept: an already active knife makes the whole function return for this frame.
                if self.bomb.objs[k].state != ObjState::Free {
                    return;
                }
                let angle = if ls {
                    anmvm::wrap_add(k as f32 * TAU / 64.0 - PI, 0.0)
                } else {
                    k as f32 * TAU / 96.0 - PI
                };
                let speed = sys.rng.next_f32() + 0.5;
                let accel = sys.rng.next_f32() * 0.1 + 0.03;
                // Quirk kept: Rng_Range16(1) is always 0, so the spin is always −π/20 (one draw; 3 per knife).
                let spin = if sys.rng.range16(1) == 0 {
                    -PI / 20.0
                } else {
                    PI / 20.0
                };
                let d = polar(angle, 24.0);
                let pos = [p[0] + d[0], p[1] + d[1], 0.0];
                let c = self.cancel_circle(xy(pos), 32.0, 0.0, 500, 6);
                let dmg = if ls { 30 } else { 20 };
                let da = self.dc(xy(pos), 32.0, 0.0, dmg, 500, Hit::every(1));
                let o = &mut self.bomb.objs[k];
                Self::obj_script(anm, sys.rng, o, if ls { 0x14 } else { 0x16 });
                (o.state, o.angle, o.fc, o.f8, o.spin, o.pos) =
                    (ObjState::Active, angle, speed, accel, spin, pos);
                o.age.set(0);
                (o.carea, o.darea) = (Some(c), Some(da));
            }
            if ls {
                sys.sound.play(6);
            }
        }
        let target = self.homing_target();
        for k in 0..count {
            if self.bomb.objs[k].state == ObjState::Free {
                continue;
            }
            let o = &mut self.bomb.objs[k];
            let age = o.age.cur;
            // Ages 30–69 hover and spin; at 70 re-aim at the homing target with speed 14.
            if (30..=69).contains(&age) {
                o.angle += o.spin;
                o.v = [0.0; 3];
            } else {
                if age == 70 && o.age.prev != 70 {
                    if let Some(t) = target {
                        o.angle = crate::fmath::atan2_f(t[1] - o.pos[1], t[0] - o.pos[0]);
                    }
                    o.fc = 14.0;
                    if ls {
                        let q = o.pos;
                        spawn(anm, sys, 0x2E, q, 1, WHITE);
                    }
                }
                let o = &mut self.bomb.objs[k];
                o.fc += o.f8;
                let v = polar(o.angle, o.fc);
                o.v = [v[0], v[1], 0.0];
            }
            self.follow(k);
            // First damage dealt (age < 120): hit script, effect 0, areas off and pointers cleared (runs once).
            if age < 120 {
                if self.bomb.objs[k]
                    .darea
                    .is_some_and(|a| self.damage_areas[a].total > 0)
                {
                    let o = &mut self.bomb.objs[k];
                    Self::obj_script(anm, sys.rng, o, if ls { 0x15 } else { 0x17 });
                    let q = o.pos;
                    spawn(anm, sys, 0, q, 1, KNIFE_HIT);
                    self.areas_off(k);
                    sys.sound.play_at(0x2B, q[0]);
                    if ls && k % 3 == 0 {
                        flash(2, 1, KNIFE_LS_FLASH, sys);
                    }
                }
            } else {
                self.areas_off(k);
            }
            // Not scaled by gs; knives are never freed before the next bomb.
            let o = &mut self.bomb.objs[k];
            o.pos[0] += o.v[0];
            o.pos[1] += o.v[1];
            Self::run_obj(anm, sys.rng, o);
            o.age.tick(b.gs);
        }
    }

    // 14 §2.9 / §2.10 Remilia 0x40EE10 / 0x40F570: the bats are the 4 options (anchors moved).
    // No damage areas: ply02as.sht levels 6/7 do the damage (09 §2.1).
    fn bomb_remilia(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let p = self.pos;
        let targets = [
            [p[0] - 32.0, p[1]],
            [p[0], p[1] - 32.0],
            [p[0], p[1] + 32.0],
            [p[0] + 32.0, p[1]],
        ];
        if Self::at(b, 0) {
            sys.sound.play(0xD);
            for (o, opt) in self.bomb.objs[..4].iter_mut().zip(&self.options) {
                o.centre = opt.anchor;
            }
            sys.sound.play(6);
            self.bomb.n = 0;
            self.speed_mult = [0.0; 2];
        }
        if b.t < 60 {
            let f = (b.t as f32 / 60.0).powi(2);
            for (k, a) in targets.iter().enumerate() {
                let s = self.bomb.objs[k].centre;
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
        for (o, t) in self.options.iter_mut().zip(targets) {
            o.anchor = [t[0], t[1], 0.0];
        }
        self.cancel_circle(xy(p), 96.0, 0.0, 0, 6);
        if b.adv && b.t % 10 == 0 {
            let n = self.bomb.n;
            let amp = sys.rng.next_f32() * 128.0;
            let width = Some((64.0, if b.ls { 128.0 } else { 64.0 }));
            let ring = SparkRing {
                n,
                waves: 4,
                width,
                end: [192.0, amp, 0.0],
                alt: false,
            };
            self.spark_ring(anm, ring, sys);
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
    fn bomb_youmu(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let ls = b.ls;
        if Self::at(b, 0) {
            sys.sound.play(0xD);
            self.speed_mult = [0.0; 2];
            self.set_script(anm, sys.rng, 0);
            sys.sound.play(6);
            self.bomb.start = self.pos;
            self.bomb.dash = [self.pos[0], 416.0, self.pos[2]];
        }
        let (s, pd) = (self.bomb.start, self.bomb.dash);
        if b.t < 40 {
            let f = 1.0 - (1.0 - b.t as f32 / 40.0).powi(2);
            self.pos[0] = s[0] + (pd[0] - s[0]) * f;
            self.pos[1] = s[1] + (pd[1] - s[1]) * f;
        }
        let colour = if ls { YOUMU_LS_FX } else { YOUMU_FX };
        if Self::at(b, 40) {
            spawn(anm, sys, 0x28, self.pos, 1, colour);
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
            spawn(anm, sys, 0x30, [c[0], c[1], 0.0], 1, colour);
            flash(8, 1, YOUMU_FLASH, sys);
            sys.sound.play(0x2A);
        }
        // (T, x offset, damage, flash alpha) per slash pair.
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
                spawn(anm, sys, 0x30, [c[0], c[1], 0.0], 1, colour);
            }
            flash(8, 1, D3dColor((a << 24) | RGB_WHITE.0), sys);
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
    fn bomb_yukari(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let ls = b.ls;
        let p = self.pos;
        if Self::at(b, 0) {
            sys.sound.play(0xD);
            if ls {
                self.bomb.objs[0].pos = p;
                for (i, s) in [(0, 0x15), (1, 0x16)] {
                    anm.set_script(sys.rng, &mut self.bomb.vms[i], ANM_SLOT, s);
                }
            }
        }
        for (k, at) in [0, 10, 20, 30].into_iter().enumerate() {
            if !Self::at(b, at) {
                continue;
            }
            let life = if ls { [100, 40, 100, 100][k] } else { 40 };
            self.cancel_circle(xy(p), 100.0, 1.0, life, 6);
            self.dc(xy(p), 100.0, 1.0, 70, 40, Hit::every(5));
            let at_pos = if ls || k == 0 { self.bomb.origin } else { p };
            let kind = if ls { 0x25 } else { 0x24 };
            let f = FxEnv {
                param: [BOUNDARY_ANGLES[k], 1.0, 4.0],
                ..FxEnv::new(kind, at_pos, WHITE)
            };
            let i = sys.fx.spawn_fixed_param(anm, sys.rng, f, 4 + k);
            // Quirk kept: the init re-spawns the slot as type 0x23, so slot 4 runs script 88 even for 0x25.
            let script = if k == 0 {
                88
            } else if ls {
                0x5C + k
            } else {
                0x58 + k
            };
            anm.set_script(sys.rng, &mut sys.fx[i].vm, effect::SCRIPT_SLOT, script);
            // The re-spawn writes colour 0xFFFFFFFF after its SetScript and slot 4 gets no second script
            // start, so it draws white (script 88's t = 0 `color` is overwritten); slots 5–7 keep their scripts' colour.
            if k == 0 {
                sys.fx[i].vm.color = WHITE.to_bgra();
            }
        }
        if ls {
            for vm in self.bomb.vms[..2].iter_mut().filter(|v| v.script.is_some()) {
                vm.run(anm, sys.rng);
            }
        }
    }

    // 14 §2.15 / §2.16 Yuyuko 0x413140 / 0x413990: butterflies.
    fn bomb_yuyuko(&mut self, anm: &mut AnmManager, b: Bf, sys: &mut Sys) {
        let p = self.pos;
        if Self::at(b, 0) {
            self.bomb
                .objs
                .iter_mut()
                .for_each(|o| o.state = ObjState::Free);
            for (g, &(script, w, dr)) in YUYUKO_GROUPS.iter().enumerate() {
                for i in 0..16 {
                    let hit = Hit {
                        max: 800,
                        ..Hit::every(1)
                    };
                    let d = self.dc(xy(p), 24.0, 0.0, 50, 500, hit);
                    let c = self.cancel_circle(xy(p), 24.0, 0.0, 500, 6);
                    let o = &mut self.bomb.objs[g * 16 + i];
                    Self::obj_script(anm, sys.rng, o, script);
                    o.angle = i as f32 * PI / 8.0 - PI;
                    (o.centre, o.pos, o.f8, o.state) = (p, p, w, ObjState::Active);
                    o.v = [0.0, dr, 0.0];
                    (o.darea, o.carea) = (Some(d), Some(c));
                }
            }
            self.speed_mult = [0.8; 2];
            spawn(anm, sys, 20, p, 1, WHITE);
            sys.sound.play(5);
            if b.ls {
                shake(60, 16, 0, sys);
            } else {
                shake(120, 12, 0, sys);
            }
        }
        if b.ls && (60..200).contains(&b.t) && b.adv && b.t % 20 == 0 {
            // Rng_SignedRange(π); quirks kept: angle uses the object slot index, the angular speed (f8) is
            // whatever the slot held, SE 0xF is panned at the last examined object.
            let base = sys.rng.next_signed() * PI;
            let (mut taken, mut last) = (0, OBJS - 1);
            for k in 0..OBJS {
                last = k;
                if self.bomb.objs[k].state != ObjState::Free {
                    continue;
                }
                let hit = Hit {
                    max: 1200,
                    ..Hit::every(1)
                };
                let d = self.dc(xy(p), 64.0, 0.0, 100, 500, hit);
                let c = self.cancel_circle(xy(p), 64.0, 0.0, 500, 6);
                let o = &mut self.bomb.objs[k];
                Self::obj_script(anm, sys.rng, o, 0x14);
                o.angle = anmvm::wrap_add(k as f32 * PI / 8.0 + base, 0.0);
                (o.centre, o.pos, o.state) = (p, p, ObjState::Active);
                o.v = [0.0, 8.0, 0.0];
                (o.darea, o.carea) = (Some(d), Some(c));
                taken += 1;
                if taken == 16 {
                    break;
                }
            }
            sys.sound.play_at(0xF, self.bomb.objs[last].pos[0]);
            shake(30, 8, 0, sys);
            flash(8, 1, YUYUKO_LS_FLASH, sys);
        }
        for k in 0..OBJS {
            let o = &mut self.bomb.objs[k];
            if o.state == ObjState::Free {
                continue;
            }
            o.v[0] += o.v[1];
            o.angle = anmvm::wrap_add(o.angle, o.f8);
            let prev = o.pos;
            let d = polar(o.angle, o.v[0]);
            o.pos = [o.centre[0] + d[0], o.centre[1] + d[1], 0.0];
            o.disp = [o.pos[0] - prev[0], o.pos[1] - prev[1], 0.0];
            if o.v[0] < 500.0 {
                self.follow(k);
                if Self::run_obj(anm, sys.rng, &mut self.bomb.objs[k]) {
                    self.bomb.objs[k].state = ObjState::Free;
                    self.areas_off(k);
                }
            } else {
                // Freed and both areas switched off.
                o.state = ObjState::Free;
                self.areas_off(k);
            }
        }
    }

    // 14 §2.17 「ディゾルブスペル」 0x40D100: boss flag clearing is done by the enemy manager on DISSOLVE_BEGIN.
    fn dissolve_start(&mut self, anm: &mut AnmManager, sys: &mut Sys) {
        let p = self.pos;
        let practice = *sys.flags & F_SPELL_PRACTICE != 0;
        spawn(anm, sys, 12, p, 1, START_FX);
        let i = sys
            .fx
            .spawn_fixed_param(anm, sys.rng, FxEnv::new(0x32, p, START_FX), 4);
        sys.fx.get(i.idx(), |e| {
            // Ring fields, not the VM scale: the type-50 update copies VM scale.x into the width.
            e.ring.segments = 64;
            e.ring.waves = 6.0;
            (e.ring.width, e.ring.radius, e.ring.start) = (15.0, 8.0, 0.0);
            e.vm.pos = [8.0, 64.0, 0.0];
            e.vm.interp_pos_from(
                if practice { 30 } else { 90 },
                5,
                [8.0, 64.0, 0.0],
                [128.0, 0.0, 0.0],
            );
        });
        sys.sound.play(0xD);
        self.speed_mult = [0.0; 2];
        self.dissolve_req |= DISSOLVE_BEGIN;
    }

    // Background tint 0x40BC60: ramps over the first/last 60 frames, alpha 0x80; global tint off, then
    // a stage tint request for the next Stage_DrawLow and the +0x646C "bomb drew" flag.
    fn tint(&self, anm: &mut AnmManager, c: D3dColor) {
        let (t, dur) = (self.bomb.timer.cur, self.bomb.duration);
        // Low byte of the ramped channel (the original masks with 0xFF).
        let ch = |v: u8| -> u8 {
            let v = i32::from(v);
            let x = if t < 60 {
                128 + (v - 128) * t / 60
            } else if t >= dur - 60 {
                128 + (v - 128) * (dur - t) / 60
            } else {
                v
            };
            lo8(x)
        };
        anm.tint_off();
        let a = D3dColor(TINT_ALPHA).a();
        anm.request_tint(D3dColor::from_bgra([ch(c.b()), ch(c.g()), ch(c.r()), a]).0);
        anm.bomb_drew = true;
    }

    // Alpha-blended playfield rectangle (Alice LS, Youmu draw fns).
    fn playfield_rect(rec: &mut Recorder, rgb: D3dColor, a: i32) {
        let r = PLAYFIELD;
        rec.rect(
            r.x,
            r.y,
            r.w,
            r.h,
            [rgb.r(), rgb.g(), rgb.b(), lo8(a.clamp(0, 255))],
        );
    }

    // Bomb draw functions (table 0x4C7AD0 fns 5–9), called from the player draw (prio 9). Step: tint requests
    // and VM writes; every tint call precedes the draws of its branch, so the emit sees the same tint.
    pub fn step_bomb(&mut self, anm: &mut AnmManager) {
        if !self.bombing {
            return;
        }
        let t = self.bomb.timer.cur;
        let ls = self.bomb.index & 2 != 0;
        if self.bomb.index == 4 {
            let c = if t < 60 {
                TINT
            } else {
                let g = ((t - 60) * 176 / 60 + 64).cast_unsigned();
                D3dColor(TINT_ALPHA | g << 16 | g << 8 | g)
            };
            self.tint(anm, c);
            return;
        }
        let p = self.pos;
        match self.bomb.ch {
            0 => {
                self.tint(anm, if ls { TINT_LS } else { TINT });
                self.step_objs(anm, if ls { OBJS } else { 16 }, 0, |_| None);
            }
            1 => {
                self.tint(anm, if ls { TINT_LS } else { TINT });
                if ls {
                    let o = self.bomb.objs[0].pos;
                    for vm in &mut self.bomb.vms[..2] {
                        vm.pos = [
                            o[0] + ORIGIN[0] + vm.pos_offset[0],
                            o[1] + ORIGIN[1] + vm.pos_offset[1],
                            0.0,
                        ];
                        anm.prepare(vm, 2);
                    }
                }
            }
            2 => {
                self.tint(anm, TINT);
                for (i, vm) in self.bomb.vms.iter_mut().enumerate() {
                    let a = i as f32 * PI / 15.0 - FRAC_PI_2 - 2.0 * PI / 15.0;
                    let d = polar(a, vm.sprite_w * vm.scale[0] / 2.0);
                    vm.pos = [p[0] + d[0] + ORIGIN[0], p[1] + d[1] + ORIGIN[1], 0.0];
                    if vm.rotation[2] != a {
                        vm.rotation[2] = a;
                        vm.flags |= anmvm::F_ROT_DIRTY;
                    }
                    anm.prepare(vm, 2);
                }
            }
            3 if !ls => self.tint(anm, TINT),
            3 => {
                if !(90..=220).contains(&t) {
                    self.tint(anm, TINT_LS);
                } else if t <= 120 {
                    // c = (T − 90)·208/30, R (c/5 − 48) mod 256, G = B = c + 32.
                    let c = ((t - 90) * 208 / 30).cast_unsigned();
                    let r = (c / 5).wrapping_sub(48) & 0xFF;
                    self.tint(
                        anm,
                        D3dColor(TINT_ALPHA | r << 16 | (c + 32) << 8 | (c + 32)),
                    );
                }
            }
            4 => {
                self.tint(anm, if ls { TINT_SAKUYA_LS } else { TINT });
                self.step_objs(anm, 96, 2, |o| Some(o.angle));
            }
            5 => self.tint(anm, if ls { TINT_REMILIA_LS } else { TINT_REMILIA }),
            6 => {
                // Two `Bomb_DrawTint` calls from T 70, averaged by the stage request.
                self.tint(anm, TINT);
                if t >= 70 {
                    self.tint(anm, TINT_YOUMU_2);
                }
            }
            _ => {
                self.tint(anm, if ls { TINT_YUYUKO_LS } else { TINT });
                let n = if ls { OBJS } else { 96 };
                self.step_objs(anm, n, 2, |o| {
                    Some(crate::fmath::atan2_f(o.disp[1], o.disp[0]))
                });
            }
        }
    }

    // Emit of `step_bomb`. Quirk kept: Sakuya LS draws only knives 0–95 (96–127 are invisible but damage).
    pub fn emit_bomb(&self, anm: &AnmManager, rec: &mut Recorder) {
        if !self.bombing || self.bomb.index == 4 {
            return;
        }
        let t = self.bomb.timer.cur;
        let ls = self.bomb.index & 2 != 0;
        match self.bomb.ch {
            0 => self.emit_objs(anm, rec, if ls { OBJS } else { 16 }, 0),
            1 if ls => {
                for (vm, z) in self.bomb.vms[..2].iter().zip([0.01, 0.0]) {
                    anm.emit_mode_z(rec, vm, 2, z);
                }
            }
            2 => self
                .bomb
                .vms
                .iter()
                .for_each(|vm| anm.emit_mode_z(rec, vm, 2, 0.0)),
            3 if ls && (90..=120).contains(&t) => {
                Self::playfield_rect(rec, RGB_WHITE, (t - 90) * 255 / 30);
            }
            3 if ls && (121..=220).contains(&t) => Self::playfield_rect(rec, RGB_WHITE, 0x70),
            4 => self.emit_objs(anm, rec, 96, 2),
            6 if (70..160).contains(&t) => {
                let a = if t <= 100 {
                    255
                } else {
                    255 - (t - 100) * 255 / 60
                };
                Self::playfield_rect(rec, if ls { RGB_RED } else { RGB_WHITE }, a);
            }
            7.. => self.emit_objs(anm, rec, if ls { OBJS } else { 96 }, 2),
            _ => {}
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

    // Text commands carry Shift-JIS; every port literal must round-trip so the raster sees the same characters.
    fn round_trips(s: &str) {
        assert_eq!(th08_formats::sjis::encode(s).decode(), s);
    }

    #[test]
    fn spell_names_are_sjis() {
        SPECS.iter().flatten().for_each(|s| round_trips(s.0.text()));
    }

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

    fn shakes_at(
        p: &mut Player,
        anm: &mut AnmManager,
        ls: bool,
        t: i32,
        sys: &mut Sys,
    ) -> Vec<[i32; 4]> {
        sys.fx.take_screen();
        let b = Bf {
            t,
            adv: true,
            ls,
            gs: 1.0,
        };
        match p.bomb.ch {
            3 => p.bomb_alice(anm, b, sys),
            _ => p.bomb_sakuya(anm, b, sys),
        }
        sys.fx
            .take_screen()
            .into_iter()
            .filter(|s| s.0 == 1)
            .map(|s| s.1)
            .collect()
    }

    // 14 §2.7 / §2.8: the shake duration is the EDX argument: Sakuya (1: 120, 4, 1) every window frame,
    // Sakuya LS (1: 50, 4, 1) once at T = 0.
    #[test]
    fn sakuya_shakes() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(8);
        p.bomb.ch = 4;
        assert!(shakes_at(&mut p, &mut anm, false, 0, sys).is_empty());
        assert_eq!(
            shakes_at(&mut p, &mut anm, false, 21, sys),
            [[120, 4, 1, 0]]
        );
        let (mut p, mut anm) = ready(8);
        p.bomb.ch = 4;
        assert_eq!(shakes_at(&mut p, &mut anm, true, 0, sys), [[50, 4, 1, 0]]);
        assert!(shakes_at(&mut p, &mut anm, true, 21, sys).is_empty());
    }

    // 14 §2.1: start angles are a running f32 sum; the ±π/60 spin wraps into [−π, π].
    #[test]
    fn reimu_orb_angles() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(0);
        p.bomb.duration = 200;
        let b = Bf {
            t: 0,
            adv: true,
            ls: false,
            gs: 1.0,
        };
        p.bomb_reimu(&mut anm, b, sys);
        let mut a = -PI;
        let mut differs = 0;
        for i in 0..16 {
            let d = if i % 2 == 0 { -PI / 60.0 } else { PI / 60.0 };
            assert_eq!(p.bomb.objs[i].angle, anmvm::wrap_add(a, d), "orb {i}");
            differs += u32::from(a != -PI + i as f32 * PI / 8.0);
            a += PI / 8.0;
        }
        assert!(differs > 0);
        assert!(p.bomb.objs[0].angle > 0.0);
    }

    // 14 §2.3 / §2.4: Alice shakes (1: 24, 8, 0) at T = 90 and 150; Alice LS only at T = 180.
    #[test]
    fn alice_shakes() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(7);
        p.bomb.ch = 3;
        p.bomb.duration = 210;
        for (t, n) in [(90, 1), (150, 1)] {
            assert_eq!(
                shakes_at(&mut p, &mut anm, false, t, sys),
                vec![[24, 8, 0, 0]; n]
            );
        }
        p.bomb.duration = 230;
        assert!(shakes_at(&mut p, &mut anm, true, 120, sys).is_empty());
        assert_eq!(shakes_at(&mut p, &mut anm, true, 180, sys), [[24, 8, 0, 0]]);
    }

    // 09 §7: a normal-mode start clears the silhouette (VM bit 17) and the global tint; the end does not tint off.
    #[test]
    fn bomb_start_tint_off() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        p.vm.flags |= anmvm::F_COLOR2;
        anm.tint_on = true;
        p.step(&mut anm, &press(input::BOMB), sys);
        assert!(p.bombing && !anm.tint_on && p.vm.flags & anmvm::F_COLOR2 == 0);
        anm.tint_on = true;
        for _ in 0..200 {
            p.step(&mut anm, &press(0), sys);
        }
        assert!(!p.bombing && anm.tint_on);
    }

    // 14 §2 / §2.2: exact 64/15 and 128/15; the LS impact C circle reuses the freed slot and is moved to the orb.
    #[test]
    fn reimu_ls_impact_follow_write() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        assert_eq!(
            ((64.0f32 / 15.0).to_bits(), (128.0f32 / 15.0).to_bits()),
            (0x4088_8889, 0x4108_8889)
        );
        let (mut p, mut anm) = ready(0);
        p.bomb.ch = 0;
        p.bomb.duration = 200;
        let bf = |t| Bf {
            t,
            adv: true,
            ls: true,
            gs: 1.0,
        };
        p.bomb_reimu_ls(&mut anm, bf(0), sys);
        let old_c = p.bomb.objs[15].carea.unwrap();
        p.bomb.objs[15].f8 = 50.0;
        p.bomb_reimu_ls(&mut anm, bf(145), sys);
        let o = &p.bomb.objs[15];
        assert_eq!((o.state, o.carea), (ObjState::Exploding, None));
        let c = &p.cancel_areas[old_c];
        assert!(c.active && c.dr == 64.0 / 15.0);
        assert_eq!(c.pos, xy(o.pos));
        assert_ne!(c.pos, xy(p.pos));
    }

    #[test]
    fn reimu_bomb_lifecycle() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        p.step(&mut anm, &press(input::BOMB), sys);
        assert!(p.bombing && p.bomb.duration == 200 && p.state == 3);
        assert_eq!(
            p.bomb.objs[..16]
                .iter()
                .filter(|o| o.state == ObjState::Active)
                .count(),
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
            p.step(&mut anm, &press(0), sys);
        }
        assert!(p.bombing && p.bomb_active);
        p.step(&mut anm, &press(0), sys);
        assert!(!p.bombing && p.speed_mult == [1.0; 2]);
        assert!(p.bomb.objs[..16].iter().all(|o| o.state == ObjState::Free));
        // 09 §1.1: +0xE2A7C holds through the end frame, drops on the next failed start test.
        assert!(p.bomb_active);
        p.step(&mut anm, &press(0), sys);
        assert!(!p.bomb_active);
    }

    // 09 §1.1: +0xE2A6C drops atop Player_UpdateBomb (so 1 → 0 allows a start in the same call), except on a
    // forced-start call.
    #[test]
    fn bomb_cooldown_decrement() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 2;
        p.step(&mut anm, &press(input::BOMB), sys);
        assert!(!p.bombing && p.bomb_cooldown == 1);
        p.step(&mut anm, &press(0), sys);
        p.step(&mut anm, &press(input::BOMB), sys);
        assert!(p.bombing && p.bomb_cooldown == 0);
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 5;
        p.forced_bomb = 1;
        p.deathbomb = 1;
        p.step(&mut anm, &press(0), sys);
        assert!(p.bombing && p.bomb_cooldown == 5);
        p.step(&mut anm, &press(0), sys);
        assert_eq!(p.bomb_cooldown, 4);
    }

    // 09 §1.1: the cooldown blocks a normal start, is decremented atop Player_UpdateBomb, but not on a forced start.
    #[test]
    fn bomb_cooldown() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 2;
        p.update_bomb(&mut anm, &press(input::BOMB), &mut None, sys);
        assert!(!p.bombing && p.bomb_cooldown == 1);
        p.update_bomb(&mut anm, &press(0), &mut None, sys);
        assert_eq!(p.bomb_cooldown, 0);
        p.update_bomb(&mut anm, &press(input::BOMB), &mut None, sys);
        assert!(p.bombing);
        let (mut p, mut anm) = ready(4);
        p.bomb_cooldown = 16;
        p.forced_bomb = 1;
        p.deathbomb = 1;
        p.update_bomb(&mut anm, &press(0), &mut None, sys);
        assert!(p.bombing && p.bomb_cooldown == 16);
        // Forced start also bypasses the bombing branch: the running bomb restarts at timer 0.
        p.update_bomb(&mut anm, &press(0), &mut None, sys);
        assert!(p.bomb.timer.cur > 0 && p.bomb_cooldown == 15);
        (p.forced_bomb, p.deathbomb) = (1, 1);
        p.update_bomb(&mut anm, &press(0), &mut None, sys);
        assert!(p.bombing && p.forced_bomb == 0 && p.bomb_cooldown == 15);
        assert_eq!(p.bomb.timer.cur, 1);
    }

    // 09 §7: the start test reads the held bit, so a bomb held since before the call still starts one.
    #[test]
    fn bomb_start_on_held() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        let mut held = press(input::BOMB);
        held.tick();
        assert!(!held.pressed(input::BOMB));
        p.update_bomb(&mut anm, &held, &mut None, sys);
        assert!(p.bombing && p.stats.bombs == 2.0);
        assert_eq!((p.stats.rec.bombs, p.stats.bombs_used), (1, 1));
    }

    // 09 §7: special mode (bits 7–8) skips the cost block: index 4, no bombs, counters or RNG draws.
    #[test]
    fn special_mode_bomb_is_free() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(4);
        (p.forced_bomb, p.deathbomb, p.last_spell) = (1, 1, 1);
        *sys.flags = F_LAST_SPELL;
        let rng = sys.rng.clone();
        p.start_bomb(&mut anm, true, sys);
        *sys.flags = 0;
        assert_eq!(p.bomb.index, 4);
        assert_eq!(p.stats.bombs, 3.0);
        assert_eq!(
            (
                p.stats.rec.bombs,
                p.stats.rec.last_spells,
                p.stats.bombs_used
            ),
            (0, 0, 0)
        );
        assert_eq!(format!("{:?}", sys.rng), format!("{rng:?}"));
        assert!(p.bombing && p.last_spell == 0 && p.forced_bomb == 0);
    }

    // 09 §7: last-spell bomb with < 2 bombs (or forced) → SetBombs(0), no RNG; 2 bombs → AddBombs(−2), 4 draws.
    // GameMgr_AddBombCount adds 4 draws either way.
    #[test]
    fn last_spell_bomb_cost() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        for (bombs, forced, left, used, draws) in [
            (1.0, false, 0.0, 1, 4),
            (2.0, false, 0.0, 2, 8),
            (3.0, false, 1.0, 2, 8),
            (3.0, true, 0.0, 3, 4),
        ] {
            let (mut p, mut anm) = ready(4);
            (p.stats.bombs, p.last_spell) = (bombs, 1);
            let t = sys.rng.total;
            p.start_bomb(&mut anm, forced, sys);
            assert_eq!((p.stats.bombs, p.bomb.used), (left, used));
            assert_eq!(sys.rng.total - t, draws);
        }
    }

    // 14 §2.15: a butterfly reaching radius 500 is freed with both areas off; its angle is wrapped.
    #[test]
    fn yuyuko_butterfly_end_areas_off() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(6);
        let bf = |t| Bf {
            t,
            adv: true,
            ls: false,
            gs: 1.0,
        };
        let hit = Hit {
            max: 800,
            ..Hit::every(1)
        };
        let d = p.dc([0.0; 2], 24.0, 0.0, 50, 500, hit);
        let c = p.cancel_circle([0.0; 2], 24.0, 0.0, 500, 6);
        let o = &mut p.bomb.objs[0];
        (o.state, o.v, o.angle, o.f8) =
            (ObjState::Active, [499.0, 2.0, 0.0], PI - 0.001, PI / 240.0);
        (o.darea, o.carea) = (Some(d), Some(c));
        p.bomb_yuyuko(&mut anm, bf(1), sys);
        let o = &p.bomb.objs[0];
        assert_eq!(o.state, ObjState::Free);
        assert!(!p.damage_areas[d].active && !p.cancel_areas[c].active);
        assert_eq!(o.angle, anmvm::wrap_add(PI - 0.001, PI / 240.0));
        assert!(o.angle < 0.0);
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
        p.tint(&mut anm, TINT);
        assert_eq!(anm.stage_tint, Some(0x8060_6060));
        assert!(anm.bomb_drew && !anm.tint_on);
        p.bomb.timer.set(100);
        p.tint(&mut anm, TINT_LS);
        assert_eq!(anm.stage_tint, Some(0x8040_4098));
        anm.stage_tint = None;
        p.tint(&mut anm, TINT_LS);
        assert_eq!(anm.stage_tint, Some(0x8020_20D0));
        anm.stage_tint = None;
        p.bomb.timer.set(170);
        p.tint(&mut anm, TINT_REMILIA_LS);
        assert_eq!(anm.stage_tint, Some(0x80B8_4040));
    }

    #[test]
    fn youmu_dash_and_return() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let (mut p, mut anm) = ready(10);
        let start = p.pos;
        p.step(&mut anm, &press(input::BOMB), sys);
        for _ in 0..70 {
            p.step(&mut anm, &press(0), sys);
        }
        assert_eq!(p.pos[1], 32.0);
        for _ in 0..80 {
            p.step(&mut anm, &press(0), sys);
        }
        assert!((p.pos[1] - start[1]).abs() < 1.0 && p.speed_mult == [1.0; 2]);
    }
}
