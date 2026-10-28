//! Effect manager g_EffectMgr 0x4ECE60 and screen effects ScreenEffect_Register 0x45B8B0 (13 §2–§3).

use crate::anmmgr::{self, AnmManager};
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::{cos_f, sin_cos_f, sin_f};
use crate::d3dx;
use crate::game::Ctx;
use crate::gfx::{Camera, FULLSCREEN, Gfx, PLAYFIELD, Vertex};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::f32::consts::PI;

// EffectMgr_Calc prio 13, EffectMgr_Draw 0x427F00 prio 12 (13 §2.3, §2.4).
pub const CALC_PRIO: u16 = 13;
pub const DRAW_PRIO: u16 = 12;
// ANM slot 6 etama.anm (all effect scripts), slot 9 effNN.anm (spell backgrounds, 13 §5).
pub const SCRIPT_SLOT: usize = 6;
pub const BG_SLOT: usize = 9;
// 653 × 0x360: 0–511 ring pool, 512–639 "high" pool, 640–652 fixed slots 0–12 (13 §2.1).
const RING_POOL: usize = 512;
const HIGH_POOL: usize = 128;
const FIXED: usize = 13;
const TOTAL: usize = RING_POOL + HIGH_POOL + FIXED;
// Port stand-in for the dummy effect (index 653) returned when a pool is full.
pub const NONE: usize = usize::MAX;

// Fixed slots (13 §2.2).
pub const FIX_INVULN: usize = 0;
pub const FIXED_BASE: usize = RING_POOL + HIGH_POOL;
pub const FIX_SPELL_INDEX: usize = FIXED_BASE + FIX_SPELL;
pub const FIX_SPELL: usize = 1;
pub const FIX_AURA: usize = 2;
pub const FIX_DEATH_BURST: usize = 3;
pub const FIX_GAUGE: usize = 8;
pub const FIX_DEATH: usize = 11;

// Effect type → etama.anm script (table 0x4C6D30, 13 §2.6).
const SCRIPTS: [i32; 66] = [
    28, 29, 30, 31, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 45, 45, 0, 32, 33, 51, 56, 52, 54, 104,
    104, 35, 53, 34, 57, 58, 59, 60, 48, 49, 50, 88, 88, 92, 71, 76, 81, 82, 83, 83, 83, 83, 84,
    72, 85, 86, 80, 73, 77, 88, 88, 87, 96, 55, 100, 78, 102, 103, 75, 74, 77, 98,
];

// Types drawn by the ring renderer 0x427450 (13 §2.6 table).
fn is_ring(kind: u8) -> bool {
    matches!(kind, 35..=37 | 39..=46 | 48..=50 | 52..=56 | 58 | 59 | 64 | 65)
}

// Ring parameters +0x314 … +0x334 (13 §2.5).
#[derive(Clone, Copy, Default, Debug)]
pub struct Ring {
    // +0x314 radius (VM pos.x), +0x318 start angle (VM rotation z).
    pub radius: f32,
    pub start: f32,
    // +0x324 segments n.
    pub segments: i32,
    // +0x320 width (VM scale.x).
    pub width: f32,
    // +0x32C amplitude / ellipse minor axis (VM pos.y), +0x330 phase / tilt (VM rotation y), +0x334 waves.
    pub amp: f32,
    pub tilt: f32,
    pub waves: f32,
}

// Effect 0x360 (13 §2.1).
#[derive(Clone, Default)]
pub struct Effect {
    pub vm: AnmVm,
    // +0x2A4 position (playfield px; world position for the 3D particles, +0x2D4 in the original).
    pub pos: [f32; 3],
    // +0x2B0 spawn parameter, +0x2BC velocity, +0x2C8 acceleration.
    pub param: [f32; 3],
    pub vel: [f32; 3],
    pub acc: [f32; 3],
    // +0x2E0 centre, +0x2EC direction / axis.
    pub centre: [f32; 3],
    pub dir: [f32; 3],
    pub ring: Ring,
    // +0x338.
    pub age: Timer,
    // +0x350 active, +0x351 type.
    pub active: bool,
    pub kind: u8,
    // +0x352 / +0x353 fade-out request / counter.
    pub fade: bool,
    fade_n: u8,
    // +0x354: 0 2D, 1 3D billboard (background), 2 3D custom matrix, 4 2D in the background pass.
    pub layer: u8,
    // +0x355 draw above bullets.
    pub above: bool,
    // +0x357 keeps running while bullets are frozen.
    pub keep_frozen: bool,
    // Port-only: script life of types 17/18/27 (60 / 240 / 120 frames).
    pub life: i32,
    // Type 19 spin (VM rotation.x in the original) and start rotation z; types 60/61 rotation z.
    spin: f32,
    rot: f32,
    // Types 51/63: pos offset +0x288 (boss pull, −9999 = unset), VM pos_end +0x244 accumulated displacement D,
    // VM +0x238 pull speed (13 §2.6c).
    pull: [f32; 3],
    disp: [f32; 3],
    pull_speed: f32,
}

// Port-only copy of the stage camera (06 §3) for the 3D effects: camera position, eye, facing.
#[derive(Clone, Copy, Default)]
pub struct View {
    pub cam: Option<Camera>,
    pub pos: [f32; 3],
    pub eye: [f32; 3],
    pub facing: [f32; 3],
    // Stage +0x63B8: facing normalised by Stage_Calc (06 §3).
    pub facing_norm: [f32; 3],
}

pub struct Effects {
    pub fx: Vec<Effect>,
    // +0 ring cursor (0..511).
    cursor: usize,
    // +8 live count, recomputed every calc.
    pub live: usize,
    // GameFlags bit 10 (bullets frozen).
    pub frozen: bool,
    // Port-only mirrors: player position, stage camera, boss 0 position (flag A bit 3), stage overlay colour,
    // pause / continue menu.
    pub player: [f32; 3],
    pub view: View,
    pub boss: Option<[f32; 3]>,
    pub overlay: [u8; 4],
    pub paused: bool,
    // Draw lists A, B, C, D, E (13 §2.3).
    lists: [Vec<usize>; 5],
    // Port-only: screen effects queued until `register_pending`.
    screen: Vec<ScreenEffect>,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            fx: vec![Effect::default(); TOTAL],
            cursor: 0,
            live: 0,
            frozen: false,
            player: [192.0, 384.0, 0.0],
            view: View::default(),
            boss: None,
            overlay: [255; 4],
            paused: false,
            lists: Default::default(),
            screen: Vec::new(),
        }
    }
}

thread_local! {
    static EFFECTS: RefCell<Effects> = RefCell::default();
}

pub fn with<R>(f: impl FnOnce(&mut Effects) -> R) -> R {
    EFFECTS.with_borrow_mut(f)
}

pub fn set_player(pos: [f32; 3]) {
    with(|e| e.player = pos);
}

pub fn set_frozen(on: bool) {
    with(|e| e.frozen = on);
}

pub fn set_view(v: View) {
    with(|e| e.view = v);
}

pub fn get(idx: usize, f: impl FnOnce(&mut Effect)) {
    with(|e| {
        if let Some(x) = e.fx.get_mut(idx).filter(|x| x.active) {
            f(x);
        }
    })
}

// Raw +0x2A4 write through a stale pointer, no alive/type check (Sakuya bomb follow, 14 §2.7, RE-Q57 AO).
// The port's pos is the world +0x2D4 for live 19/51/63, whose update recomputes +0x2A4 anyway: skip them.
pub fn write_pos_unchecked(idx: usize, pos: [f32; 3]) {
    with(|e| {
        if let Some(x) = e.fx.get_mut(idx) {
            if !(x.active && matches!(x.kind, 19 | 51 | 63)) {
                x.pos = pos;
            }
        }
    })
}

// Reads a slot even after it was freed (the original reads stale slot data).
pub fn peek<R>(idx: usize, f: impl FnOnce(&Effect) -> R) -> R {
    with(|e| f(&e.fx[idx]))
}

pub fn free(idx: usize) {
    get(idx, |x| x.active = false);
}

// STD op 5 after a jump shifts every kind-51 effect by the camera target change (06 §4).
pub fn shift_kind(kind: u8, d: [f32; 3]) {
    with(|e| {
        for x in e.fx.iter_mut().filter(|x| x.active && x.kind == kind) {
            x.pos = std::array::from_fn(|k| x.pos[k] + d[k]);
        }
    })
}

// Debug dump of every live effect of `kind` (format of specs/data/q15/eff51_decoded.txt).
pub fn dump(kind: u8) -> String {
    let argb = |c: [u8; 4]| u32::from_le_bytes(c);
    with(|m| {
        let v = m.view;
        let mut s = format!(
            "F ring {} cam {:?} fac {:?} ovl {:08x} player {:?}\n",
            m.cursor,
            v.pos,
            v.facing,
            argb(m.overlay),
            m.player
        );
        for (i, e) in m.fx.iter().enumerate() {
            if !e.active || e.kind != kind {
                continue;
            }
            s += &format!(
                " E{i} t{} time{} world{:?} v{:?} a{:?} rotz{:.5} scl{:?} col {:08x} col2 {:08x} fl {:08x} I {} {} D{:?}\n",
                e.kind,
                e.vm.time.cur,
                e.pos,
                e.vel,
                e.acc,
                e.vm.rotation[2],
                e.vm.scale,
                argb(e.vm.color),
                argb(e.vm.color2),
                e.vm.flags,
                e.vm.ivars[0],
                e.vm.ivars[1],
                e.disp
            );
        }
        s
    })
}

pub fn clear() {
    with(|e| {
        *e = Effects {
            player: e.player,
            ..Effects::default()
        }
    });
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// Vec3_Dot 0x40B540: (x·x' + y·y') + z·z' in f32 (13 §2.6a).
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

impl Effect {
    // Enemy_UpdateOrbitEffects 0x42E010 (13 §2.6b): visibility, centre, radius grows 0.3/frame to d, spin.
    pub fn orbit_follow(&mut self, centre: [f32; 3], target: f32, hidden: bool) {
        if hidden {
            self.vm.flags &= !anmvm::F_VISIBLE;
        } else {
            self.vm.flags |= anmvm::F_VISIBLE;
        }
        self.centre = centre;
        if target <= self.ring.radius {
            self.ring.radius = target;
        } else {
            self.ring.radius += 0.3;
        }
        self.ring.start = anmvm::wrap_add(self.ring.start, 0.0314159);
    }
}

impl Effects {
    // Effect_Spawn / Effect_SpawnWithParam loop (13 §2.2): one 512-slot search budget for the whole call, every
    // examined slot advances the cursor; count is decremented per spawn and the loop ends at exactly 0, so
    // count ≤ 0 fills free slots until the budget ends. Returns the last slot taken (even if its init freed it),
    // or the dummy (NONE) when the budget runs out.
    fn spawn_ring(&mut self, count: i32, mut init: impl FnMut(&mut Self, usize) -> bool) -> usize {
        let mut n = count;
        for _ in 0..RING_POOL {
            let i = self.cursor;
            self.cursor = (i + 1) % RING_POOL;
            if self.fx[i].active {
                continue;
            }
            if !init(self, i) {
                self.fx[i].active = false;
            }
            n = n.wrapping_sub(1);
            if n == 0 {
                return i;
            }
        }
        NONE
    }

    // Effect_SpawnHigh 0x425B70: first inactive among 512–639, no cursor.
    fn slot_high(&self) -> Option<usize> {
        (RING_POOL..RING_POOL + HIGH_POOL).find(|&i| !self.fx[i].active)
    }

    fn reset(&mut self, i: usize, kind: u8, pos: [f32; 3], param: [f32; 3]) {
        let e = &mut self.fx[i];
        *e = Effect {
            active: true,
            kind,
            pos,
            param,
            ..Effect::default()
        };
        e.age.set(0);
    }

    // Per-type init of table 0x4C6D30 (13 §2.6); velocities and accelerations are × gs at init.
    fn type_init(&mut self, rng: &mut crate::rng::Rng, gs: f32, i: usize) -> bool {
        let player = self.player;
        let view = self.view;
        let e = &mut self.fx[i];
        let (kind, pos, param) = (e.kind, e.pos, e.param);
        match kind {
            // Sparks: 2 RNG draws (x then y).
            3 => {
                let vx = (rng.next_f32() * 256.0 - 128.0) * 4.0 / 33.0;
                let vy = (rng.next_f32() * 256.0 - 128.0) * 4.0 / 33.0;
                e.vel = [vx * gs, vy * gs, 0.0];
                e.acc = [-vx / 20.0 * gs, -vy / 20.0 * gs, 0.0];
            }
            4..=11 => {
                let vx = (rng.next_f32() * 256.0 - 128.0) / 12.0;
                let vy = (rng.next_f32() * 256.0 - 128.0) / 12.0;
                e.vel = [vx * gs, vy * gs, 0.0];
                e.acc = [-vx / 19.0 * gs, -vy / 19.0 * gs, 0.0];
            }
            // EffInit_Orbit13 0x425FE0 (13 §2.6b); the op-128 spawn then writes the axis.
            13..=15 => {
                e.layer = 2;
                e.centre = pos;
                e.dir = [0.0; 3];
                e.ring.radius = 0.0;
            }
            // EffInit_Converge 0x426B20 (13 §2.6): angle = Rng_FloatRange(2π) − π, one draw.
            17 | 18 | 27 => {
                e.centre = pos;
                let a = rng.next_f32() * (2.0 * PI) - PI;
                e.dir = [cos_f(a), sin_f(a), 0.0];
                e.life = match kind {
                    17 => 60,
                    18 => 240,
                    _ => 120,
                };
            }
            // 3D background particles (13 §2.6a): base = camera + facing/2; 10 draws (19) / 8 draws (51, 63) in
            // table order. Type 51/63 keep the original's expression order for 24-bit bit-exactness.
            19 | 51 | 63 => {
                let mut r = |x: f32| rng.next_f32() * x;
                let base: [f32; 3] =
                    std::array::from_fn(|k| view.pos[k] + view.facing[k] + -view.facing[k] / 2.0);
                let (off, v, a);
                if kind == 19 {
                    let o = [r(120.0) - 60.0, r(200.0) - 100.0, r(100.0) - 100.0];
                    let vv = [
                        r(0.06) - 0.03 + param[0],
                        r(0.06) - 0.03 + param[1],
                        r(0.1) + 0.03 + param[2],
                    ];
                    let aa = [r(0.0002) - 0.0001, r(0.0002) - 0.0001, 0.0];
                    (off, v, a) = (o, vv, aa);
                    e.rot = r(2.0 * PI) - PI;
                    e.spin = r(0.0314159) - 0.015708;
                } else {
                    let h = view.facing.map(|f| -f / 2.0);
                    let ox = h[0] + rng.next_signed() * 60.0;
                    let oy = h[1]
                        + if kind == 51 {
                            rng.next_signed() * 100.0 - 50.0
                        } else {
                            rng.next_signed() * 200.0 - 200.0
                        };
                    let oz = h[2] + (rng.next_f32() * 100.0 - 100.0);
                    let vx = rng.next_signed() * 0.001 + param[0];
                    let vy = rng.next_signed() * 0.03 + if kind == 51 { param[1] } else { 0.4 };
                    let vz = -(rng.next_f32() * 0.1) - 0.3 + param[2];
                    let ax = rng.next_signed() * 0.0001;
                    let ay = rng.next_signed() * 0.0001;
                    (off, v, a) = ([ox, oy, oz], [vx, vy, vz], [ax, ay, -0.0003]);
                    e.pull = [-9999.0, 0.0, 0.0];
                }
                e.pos = if kind == 19 {
                    std::array::from_fn(|k| base[k] + off[k])
                } else {
                    std::array::from_fn(|k| off[k] + (view.facing[k] + view.pos[k]))
                };
                e.vel = v.map(|x| x * gs);
                e.acc = a.map(|x| x * gs);
                e.layer = 1;
            }
            21 | 26 => {
                e.centre = pos;
                // EffInit_Burst 0x4270C0 (13 §2.6): ECL passes −999.0 for "random"; 0 and negative angles are kept.
                let a = if param[0] <= -990.0 {
                    rng.next_f32() * (2.0 * PI) - PI
                } else {
                    anmvm::wrap_add(param[0], 0.0)
                };
                let m = 1.0 + rng.next_f32() * 1.5;
                e.dir = [cos_f(a) * m, sin_f(a) * m, 0.0];
            }
            // 13 §2.6: no init; the spawners pass the player pos +0x2B4.
            22..=25 => e.pos = player,
            // pos += polar(param.x, 256); rotation z = param.x + π/2.
            60 | 61 => {
                let d = crate::bullet::polar(param[0], 256.0);
                e.pos[0] += d[0];
                e.pos[1] += d[1];
                e.spin = param[0] + PI / 2.0;
            }
            _ => {}
        }
        // Ring init 0x4272E0 (13 §2.6): centre := pos, crossed param copy (+0x318 ← x, +0x314 ← y, +0x320 ← z),
        // segments := 24. Types 41 and 50 get their update's per-frame count (48 / 32) here; 56/58/65 are over bullets.
        if is_ring(kind) {
            e.centre = pos;
            (e.ring.start, e.ring.radius, e.ring.width) = (param[0], param[1], param[2]);
            e.ring.segments = match kind {
                41 => 48,
                // `Bomb_Dissolve` writes 64, but the type-50 update writes 32 before the first draw (13 §2.6).
                50 => 32,
                _ => 24,
            };
            e.above = matches!(kind, 56 | 58 | 65);
        }
        let r = &mut e.ring;
        match kind {
            // Boundary inits 0x411720 / 0x411A80: ring with angle/radius/width = param, then 44 / 54 segments.
            36 | 37 => {
                (r.start, r.radius, r.segments, r.width) = (
                    param[0],
                    param[1],
                    if kind == 36 { 44 } else { 54 },
                    if kind == 36 { 4.0 } else { 6.0 },
                )
            }
            // Alice rings 0x2A–0x2D, Sakuya LS 0x2E, Youmu slash 0x30 (14 §2.3, §2.8, §2.11).
            42 => (r.segments, r.waves, r.width) = (64, 5.0, 8.0),
            43 | 44 => {
                (r.segments, r.width, r.tilt) =
                    (48, 8.0, if kind == 43 { PI / 4.0 } else { -PI / 4.0 })
            }
            45 => (r.segments, r.width) = (8, 8.0),
            46 => (r.segments, r.width) = (12, 32.0),
            48 => {
                (r.segments, r.width, r.radius, r.amp, r.tilt) = (48, 3.0, 192.0, 0.0001, PI / 2.0)
            }
            _ => {}
        }
        true
    }

    // The script runs its t = 0 step here, so its RNG draws precede the type init's (13 §2.6).
    fn start_script(&mut self, anm: &mut AnmManager, i: usize, colour: u32, zoff: bool) {
        let e = &mut self.fx[i];
        let script = SCRIPTS.get(e.kind as usize).copied().unwrap_or(-1);
        if script >= 0 {
            anm.set_script(&mut e.vm, SCRIPT_SLOT, script as usize);
        }
        if zoff {
            e.vm.flags |= anmvm::F_ZWRITE_OFF;
        }
        e.vm.color = colour.to_le_bytes();
        e.vm.pos_offset = [0.0; 3];
    }

    // VM fields written by the inits after the script start: type 19 / 60 / 61 rotation z.
    fn start_post(&mut self, i: usize) {
        let e = &mut self.fx[i];
        match e.kind {
            19 => {
                e.vm.rotation[2] = e.rot;
                e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            60 | 61 => {
                e.vm.rotation[2] = e.spin;
                e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            _ => {}
        }
    }

    fn init_start(
        &mut self,
        anm: &mut AnmManager,
        i: usize,
        kind: u8,
        pos: [f32; 3],
        param: [f32; 3],
        colour: u32,
        zoff: bool,
    ) -> bool {
        // 13 §2.2 "Spawn-time game speed": the t = 0 script run and the type init use the speed at the spawn.
        let gs = anm.speed;
        self.reset(i, kind, pos, param);
        self.start_script(anm, i, colour, zoff);
        let ok = self.type_init(&mut anm.rng, gs, i);
        if ok {
            self.start_post(i);
        }
        ok
    }

    // Effect_Spawn 0x425430 / Effect_SpawnWithParam 0x425650 (`zoff` false: no VM flag 0x2000).
    pub fn spawn_at(
        &mut self,
        anm: &mut AnmManager,
        kind: u8,
        pos: [f32; 3],
        param: [f32; 3],
        count: i32,
        colour: u32,
        zoff: bool,
    ) -> usize {
        self.spawn_ring(count, |s, i| {
            s.init_start(anm, i, kind, pos, param, colour, zoff)
        })
    }

    // Effect_SpawnHigh 0x425B70: layer 0.
    pub fn spawn_high(
        &mut self,
        anm: &mut AnmManager,
        kind: u8,
        pos: [f32; 3],
        colour: u32,
    ) -> usize {
        let Some(i) = self.slot_high() else {
            return NONE;
        };
        self.init_start(anm, i, kind, pos, [0.0; 3], colour, true);
        self.fx[i].layer = 0;
        i
    }

    // Effect_SpawnFixed 0x425870 / Effect_SpawnFixedParam 0x4259E0: always slot 640 + k (overwrites).
    pub fn spawn_fixed(
        &mut self,
        anm: &mut AnmManager,
        kind: u8,
        pos: [f32; 3],
        param: [f32; 3],
        k: usize,
        colour: u32,
    ) -> usize {
        let i = FIXED_BASE + k.min(FIXED - 1);
        self.init_start(anm, i, kind, pos, param, colour, true);
        i
    }

    // Per-type update (13 §2.6); false frees the effect.
    fn update(&mut self, i: usize) -> bool {
        let player = self.player;
        let view = self.view;
        let boss = self.boss;
        let overlay = self.overlay;
        let e = &mut self.fx[i];
        let t = e.age.getf();
        match e.kind {
            // Sparks: pos += v, v += a.
            3..=11 => {
                for k in 0..2 {
                    e.pos[k] += e.vel[k];
                    e.vel[k] += e.acc[k];
                }
            }
            // EffUpd_Orbit13 0x426030 (13 §2.6b): the quaternion (n·sin a, cos a) rotates by 2a about n.
            13..=15 => {
                let n = d3dx::vec3_normalize(e.dir);
                let th = 2.0 * e.ring.start;
                let mut p = [n[1], -n[0], 0.0];
                // Quirk kept: below 1e-5 p stays ≈ 0 (the original writes (1, 0, 0) into the wrong variable).
                if dot(p, p) >= 1e-5 {
                    p = d3dx::vec3_normalize(p);
                }
                p = p.map(|x| x * e.ring.radius);
                let (s, c) = sin_cos_f(th);
                let nxp = [
                    n[1] * p[2] - n[2] * p[1],
                    n[2] * p[0] - n[0] * p[2],
                    n[0] * p[1] - n[1] * p[0],
                ];
                let np = dot(n, p);
                let q: [f32; 3] =
                    std::array::from_fn(|k| p[k] * c + nxp[k] * s + n[k] * np * (1.0 - c));
                e.pos = [e.centre[0] + q[0], e.centre[1] + q[1], 0.0];
                if e.fade {
                    e.fade_n += 1;
                    if e.fade_n > 15 {
                        return false;
                    }
                    let f = 1.0 - e.fade_n as f32 / 16.0;
                    e.vm.color[anmvm::A] = (f * 255.0) as u8;
                    e.vm.scale = [2.0 - f, 2.0 - f];
                    e.vm.flags |= anmvm::F_SCALE_DIRTY;
                }
            }
            // Converge: pos = centre + dir·(256 − 256·t/life); 18 keeps z.
            17 | 18 | 27 => {
                let r = 256.0 - 256.0 * t / e.life as f32;
                e.pos = [
                    e.centre[0] + e.dir[0] * r,
                    e.centre[1] + e.dir[1] * r,
                    if e.kind == 18 { e.pos[2] } else { 0.0 },
                ];
            }
            // 13 §2.6a per frame: v += a, world += v, view-cone cull (dot < 0.94 frees).
            19 | 51 | 63 => {
                for k in 0..3 {
                    e.vel[k] += e.acc[k];
                    e.pos[k] += e.vel[k];
                }
                // Stored facing; Vec3_Dot order (13 §2.6a).
                let d = d3dx::vec3_normalize(sub(e.pos, view.pos));
                if dot(d, view.facing_norm) < 0.94 {
                    return false;
                }
                if e.kind == 19 {
                    e.vm.rotation[2] = anmvm::wrap_add(e.vm.rotation[2], e.spin);
                    e.vm.flags |= anmvm::F_ROT_DIRTY;
                    if e.pos[2] >= 0.0 {
                        return false;
                    }
                } else {
                    if let Some(b) = boss {
                        if e.pull[0] == -9999.0 {
                            e.pull = b;
                        } else {
                            e.pull = std::array::from_fn(|k| e.pull[k] + (b[k] - e.pull[k]) * 0.1);
                        }
                    }
                    // Type 51: colour2 = colour × stage overlay colour, (a·b) >> 8, before this frame's VM run.
                    if e.kind == 51 {
                        e.vm.flags |= anmvm::F_COLOR2;
                        e.vm.color2 = std::array::from_fn(|k| {
                            ((e.vm.color[k] as u32 * overlay[k] as u32) >> 8) as u8
                        });
                    }
                }
            }
            // Burst: pos = centre + dir·128·(1 − (1 − t/90)²).
            21 | 26 => {
                let f = 1.0 - (1.0 - (t / 90.0).min(1.0)).powi(2);
                e.pos = [
                    e.centre[0] + e.dir[0] * 128.0 * f,
                    e.centre[1] + e.dir[1] * 128.0 * f,
                    0.0,
                ];
            }
            22 => e.pos = player,
            // Invulnerability ring (13 §2.6): scale → 1/16 over I0 frames (the script fades it in).
            // 13 §2.6: interpolates from the current scale, always (1, 1) (script 104 sets it at time 0).
            23 | 24 => {
                let n = e.vm.ivars[0].max(1) as f32;
                let s = 1.0 + (1.0 / 16.0 - 1.0) * (t / n).min(1.0);
                e.vm.scale = [s, s];
                e.vm.flags |= anmvm::F_SCALE_DIRTY;
            }
            _ => {}
        }
        if bomb_ring(e.kind) {
            let slot = i.wrapping_sub(FIXED_BASE);
            let (screen, keep) = bomb_ring_update(e, slot);
            self.screen.extend(screen);
            return keep;
        }
        // Ring updates 0x427990 (35/39/49/52/59: width, radius only) and 0x4279D0 (also amplitude, angle, phase;
        // 40/55/56/58/65 also segments := I0, waves := I1); 53/54 free at alpha 0 (0x427AE0); 50 ends at 120.
        if is_ring(e.kind) {
            let r = &mut e.ring;
            r.width = e.vm.scale[0];
            r.radius = e.vm.pos[0];
            if !matches!(e.kind, 35 | 39 | 49 | 52 | 59) {
                r.amp = e.vm.pos[1];
                r.start = e.vm.rotation[2];
                r.tilt = e.vm.rotation[1];
            }
            if matches!(e.kind, 40 | 55 | 56 | 58 | 65) {
                r.segments = e.vm.ivars[0].max(3);
                r.waves = e.vm.ivars[1] as f32;
            }
            if matches!(e.kind, 53 | 54) && e.vm.color[anmvm::A] == 0 {
                return false;
            }
            if e.kind == 50 && t >= 120.0 {
                return false;
            }
        }
        true
    }

    // EffectMgr_Calc (13 §2.3): frozen effects (bit 10, +0x357 = 0) skip update/VM/age but are still drawn;
    // survivors are linked in slot order (not spawn order); type 0x40 is never linked.
    pub fn calc(&mut self, anm: &mut AnmManager) {
        let gs = anm.speed;
        self.live = 0;
        for l in &mut self.lists {
            l.clear();
        }
        for i in 0..TOTAL {
            if !self.fx[i].active {
                continue;
            }
            self.live += 1;
            if !self.frozen || self.fx[i].keep_frozen {
                if !self.update(i) {
                    self.fx[i].active = false;
                    continue;
                }
                let e = &mut self.fx[i];
                if e.vm.script.is_some() && e.vm.run(anm) {
                    e.active = false;
                    continue;
                }
                e.age.tick(gs);
            }
            let e = &self.fx[i];
            if e.kind == 64 {
                continue;
            }
            let list = match e.layer {
                1 | 3.. => 1,
                2 => 2,
                _ if e.above => 3,
                _ if e.vm.blend() == 1 => 4,
                _ => 0,
            };
            self.lists[list].push(i);
        }
    }

    // Lists A/D/E (13 §2.4): 2D rotated sprite at pos + playfield origin + pos offset, or the ring renderer.
    fn draw_one(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, i: usize, z: f32) {
        let e = &mut self.fx[i];
        if is_ring(e.kind) {
            draw_ring(anm, gfx, e);
            return;
        }
        let saved = e.vm.pos;
        e.vm.pos = [
            e.pos[0] + 32.0 + e.vm.pos_offset[0],
            e.pos[1] + 16.0 + e.vm.pos_offset[1],
            0.0,
        ];
        let off = e.vm.pos_offset;
        e.vm.pos_offset = [0.0; 3];
        anm.draw_mode_z(gfx, &mut e.vm, 2, z);
        e.vm.pos = saved;
        e.vm.pos_offset = off;
    }

    // EffectMgr_Draw (prio 12): list A, then C, then E; A/E at z 0.07.
    pub fn draw_main(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        gfx.set_viewport(PLAYFIELD);
        for list in [0, 2, 4] {
            for k in 0..self.lists[list].len() {
                let i = self.lists[list][k];
                if list == 2 {
                    self.draw_billboard_c(anm, gfx, i);
                } else {
                    self.draw_one(anm, gfx, i, 0.07);
                }
            }
        }
    }

    // List C (13 §2.4): `AnmManager_DrawBillboard` (05 §9 mode 4) at the effect pos as-is (no playfield origin, no
    // pos offset), projected through `Camera_Set2D` on the playfield viewport left by `Stage_DrawHigh`, whatever
    // the VM's camera bit; sized by the stage camera's right vector, vertex z = projected z.
    fn draw_billboard_c(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, i: usize) {
        let e = &mut self.fx[i];
        let (saved, cam) = (e.vm.pos, e.vm.flags & anmvm::F_STAGE_CAM);
        e.vm.pos = e.pos;
        e.vm.flags &= !anmvm::F_STAGE_CAM;
        anm.draw_mode_z(gfx, &mut e.vm, 4, 0.0);
        e.vm.flags |= cam;
        e.vm.pos = saved;
    }

    // List D, drawn at the end of BulletMgr_Draw (prio 13), z 0.04.
    pub fn draw_over_bullets(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for k in 0..self.lists[3].len() {
            let i = self.lists[3][k];
            self.draw_one(anm, gfx, i, 0.04);
        }
    }

    // List B from Stage_DrawHigh (13 §2.4, §2.6c): only effect level 2 draws; no fog. Layer 4 at raw coordinates.
    // Layer 3 / ≥ 5 would be a 3D quad, unreachable (13 §2.4: no writer sets them); the port billboards them.
    pub fn draw_background(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, level: u8) {
        if level < 2 {
            return;
        }
        let Some(cam) = self.view.cam else { return };
        // 0x4EA3F4 as last written by Stage_SetCamera (this frame's layers 2/3 just before, 13 §2.6c).
        let right = anm.cam_right;
        let player = self.player;
        for k in 0..self.lists[1].len() {
            let i = self.lists[1][k];
            if self.fx[i].layer == 4 {
                let e = &mut self.fx[i];
                let saved = e.vm.pos;
                e.vm.pos = e.pos;
                anm.draw_mode_z(gfx, &mut e.vm, 2, 0.0);
                e.vm.pos = saved;
                continue;
            }
            let e = &mut self.fx[i];
            let mut s = cam.project(e.pos, PLAYFIELD);
            if !(0.0..=1.0).contains(&s[2]) {
                continue;
            }
            let s2 = cam.project(
                [
                    e.pos[0] + right[0],
                    e.pos[1] + right[1],
                    e.pos[2] + right[2],
                ],
                PLAYFIELD,
            );
            let len = anmmgr::length3([s2[0] - s[0], s2[1] - s[1], s2[2] - s[2]]);
            // Callback 0x428310 (13 §2.6c): boss pull within 160 px, player push within 88 px; skipped while paused.
            if matches!(e.kind, 51 | 63) && !self.paused {
                let p = [s[0] + e.disp[0], s[1] + e.disp[1]];
                if e.pull[0] > -9999.0 {
                    let d = [e.pull[0] - p[0] + 32.0, e.pull[1] - p[1] + 16.0];
                    if d[0] * d[0] + d[1] * d[1] < 160.0 * 160.0 {
                        e.pull_speed += 0.0005;
                        e.disp[0] += d[0] * e.pull_speed;
                        e.disp[1] += d[1] * e.pull_speed;
                    }
                }
                let d2 = [p[0] - player[0] - 32.0, p[1] - player[1] - 16.0];
                if d2[0] * d2[0] + d2[1] * d2[1] < 88.0 * 88.0 {
                    e.disp[0] += d2[0] * 0.02;
                    e.disp[1] += d2[1] * 0.02;
                }
                s[0] += e.disp[0];
                s[1] += e.disp[1];
            }
            anm.draw_billboard(gfx, &mut e.vm, s, len);
        }
    }
}

// Ring renderer 0x427450 (13 §2.5): strip of 2(n+1) vertices, v along the ring, u across the band, WRAP sampling.
fn draw_ring(anm: &mut AnmManager, gfx: &mut Gfx, e: &Effect) {
    let vm = &e.vm;
    if vm.flags & (anmvm::F_VISIBLE | anmvm::F_ENABLED) != (anmvm::F_VISIBLE | anmvm::F_ENABLED)
        || vm.color[anmvm::A] == 0
        || !vm.has_sprite
    {
        return;
    }
    let Some(&spr) = anm.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
        return;
    };
    let r = e.ring;
    let n = r.segments.max(3);
    let step = 2.0 * PI / n as f32;
    let w = r.width / ((PI - step) / 2.0).sin();
    // Half width w = width / sin((π − 2π/n)/2); +0.5 is the port's D3D → wgpu pixel-centre offset.
    let (cx, cy) = (e.centre[0] + 32.0 + 0.5, e.centre[1] + 16.0 + 0.5);
    let c = vm.color;
    let rgba = anm.tinted([c[anmvm::R], c[anmvm::G], c[anmvm::B], c[anmvm::A]]);
    let wrap = |a: f32| if a >= PI { a - 2.0 * PI } else { a };
    let point = |k: i32, outer: bool| -> [f32; 2] {
        let th = wrap(r.start + k as f32 * step);
        let sgn = if outer { 1.0 } else { -1.0 };
        // Plain ring / rotated ellipse (θ from 0) / wavy ring (13 §2.5).
        if r.amp == 0.0 {
            let rr = r.radius + sgn * w;
            [cx + th.cos() * rr, cy + th.sin() * rr]
        } else if r.waves == 0.0 {
            let th = wrap(k as f32 * step);
            let (x, y) = (
                th.cos() * (r.radius + sgn * w),
                th.sin() * (r.amp + sgn * w),
            );
            let (s, co) = r.tilt.sin_cos();
            [cx + x * co - y * s, cy + x * s + y * co]
        } else {
            let phi = r.tilt + k as f32 * 2.0 * PI * r.waves / n as f32;
            let rr = r.radius + phi.cos() * r.amp + sgn * w;
            [cx + th.cos() * rr, cy + th.sin() * rr]
        }
    };
    let blend = anm.blend_for(vm);
    let (u0, v0, u1, v1) = (
        spr.uv[0] + vm.uv_off[0],
        spr.uv[1],
        spr.uv[2] + vm.uv_off[0],
        spr.uv[3],
    );
    let vk = |k: i32| v1 + vm.uv_off[1] - k as f32 * (v1 - v0) / n as f32;
    for k in 0..n {
        let p = [
            point(k, true),
            point(k, false),
            point(k + 1, true),
            point(k + 1, false),
        ];
        let uv = [[u0, vk(k)], [u1, vk(k)], [u0, vk(k + 1)], [u1, vk(k + 1)]];
        // Ring vertices are at z 0 (13 §2.5): centre z is 0 for every spawner, ellipse/wavy force 0.
        let v = std::array::from_fn(|i| Vertex::screen(p[i][0], p[i][1], 0.0, uv[i], rgba));
        gfx.strip(spr.tex, blend, false, v);
    }
}

// Ring types whose update functions belong to the bombs (13 §2.6 rows 36/37, 42–46/48).
fn bomb_ring(kind: u8) -> bool {
    matches!(kind, 36 | 37 | 42..=46 | 48)
}

// Bomb-owned ring updates (14 §2): Yukari 0x4114E0 / 0x4117B0, Alice 0x40E040–0x40E2D0, Sakuya LS 0x410BB0,
// Youmu 0x413070. Returns queued screen effects and whether the effect stays.
fn bomb_ring_update(e: &mut Effect, slot: usize) -> (Vec<ScreenEffect>, bool) {
    let t = e.age.cur;
    let tf = t as f32;
    let at = |n: i32| t == n && e.age.prev != n;
    let mut fx = Vec::new();
    let r = &mut e.ring;
    match e.kind {
        // Yukari 0x4114E0 (14 §2.13): shrinking square; sides at t = 40.
        36 => {
            if t < 40 {
                let k = 1.0 - tf / 40.0;
                r.width = 88.0 - 2.0 * tf;
                r.radius = 192.0 - 384.0 * k * k;
                r.segments -= 1;
            } else if at(40) {
                fx.push(screen_fx(3, 8, 1, 0x8FF08080u32 as i32, 0));
                boundary_sides(e.centre, r, 70, 4, 100);
            }
        }
        // Yukari LS 0x4117B0 (14 §2.14): spin ±π/80 by slot parity; sides at t = 50.
        37 => {
            r.start = anmvm::wrap_add(r.start, if slot % 2 == 0 { -PI / 80.0 } else { PI / 80.0 });
            let n = slot as f32 - 4.0;
            if t < 50 {
                let k = 1.0 - tf / 50.0;
                r.width = 88.0 - 80.0 * tf / 50.0;
                r.radius = n * 32.0 + 192.0 - (n * 32.0 + 384.0) * k * k;
                r.segments -= 1;
            } else if at(50) {
                fx.push(screen_fx(1, 16, 8, 0, 0));
                fx.push(screen_fx(3, 8, 1, 0x8F6060F0u32 as i32, 0));
                boundary_sides(e.centre, r, 100, 2, 150);
            }
        }
        // Alice 0x2A–0x2D (14 §2.3): k = 1 − (1 − t/40)², expand 40 frames then collapse; 0x2A amplitude
        // stays 0 (plain ring) until t = 40. Freed by script 83's hide_stop.
        42..=45 => {
            let k = 1.0 - (1.0 - tf / 40.0).powi(2);
            match e.kind {
                42 => {
                    r.radius = 256.0 * k;
                    if t >= 40 {
                        r.amp = 64.0 * k;
                        r.width += 2.0;
                    }
                }
                43 | 44 => {
                    (r.radius, r.amp) = (256.0 * k, 128.0 * k);
                    if t >= 40 {
                        r.width += 1.5;
                    }
                }
                _ => r.radius = 192.0 * k,
            }
        }
        // Sakuya LS 0x2E (14 §2.8): radius +8/frame; freed by script 84.
        46 => r.radius += 8.0,
        // Youmu slash 0x30 (14 §2.11): thin line until t = 30, then f = ((t − 30)/30)⁴; life 60.
        _ => {
            if t >= 30 {
                let f = ((tf - 30.0) / 30.0).powi(4);
                (r.amp, r.width) = (192.0 * f, 80.0 * f + 3.0);
            }
            if t >= 60 {
                return (fx, false);
            }
        }
    }
    (fx, true)
}

// The four side rectangles of a finished boundary (14 §2.13): D rect 8s × 4w, C rect 4s × 4w.
// s uses f32 0x3F3504F2 (1 ulp below √2/2); a wraps at the top of each side and chains from the last rotation.
fn boundary_sides(centre: [f32; 3], r: &Ring, d_life: i32, period: i32, c_life: i32) {
    let s = r.radius * f32::from_bits(0x3F3504F2);
    let mut a = r.start + PI / 4.0;
    for _ in 0..4 {
        if a >= PI {
            a -= 2.0 * PI;
        }
        let d = crate::bullet::polar(a, s);
        let c = [centre[0] + d[0], centre[1] + d[1]];
        let rot = anmvm::wrap_add(a, PI / 2.0);
        let d = crate::bomb::Area {
            pos: c,
            size: [8.0 * s, 4.0 * r.width],
            rot,
            damage: 60,
            life: d_life,
            period,
            item: 6,
            active: true,
            ..Default::default()
        };
        crate::bomb::queue_area(true, d);
        let k = crate::bomb::Area {
            pos: c,
            size: [4.0 * s, 4.0 * r.width],
            rot,
            item: 6,
            life: c_life,
            period: 1,
            active: true,
            ..Default::default()
        };
        crate::bomb::queue_area(false, k);
        a = rot;
    }
}

fn screen_fx(kind: u8, p0: i32, p1: i32, p2: i32, p3: i32) -> ScreenEffect {
    ScreenEffect {
        kind,
        p: [p0, p1, p2, p3],
        // Draw prio 0x15: every code call passes it except the loading darken (1) (13 §3).
        prio: 0x15,
        // ScreenEffect_Register 0x45B8B0 zero-fills the object: a draw before the first calc shows alpha 0 (13 §3
        // "Initial alpha").
        alpha: 0.0,
        release: false,
        loading: false,
        timer: Timer::default(),
        done: false,
    }
}

pub fn spawn(anm: &mut AnmManager, kind: u8, pos: [f32; 3], count: i32, colour: u32) -> usize {
    with(|e| e.spawn_at(anm, kind, pos, [0.0; 3], count, colour, true))
}

pub fn spawn_param(
    anm: &mut AnmManager,
    kind: u8,
    pos: [f32; 3],
    param: [f32; 3],
    count: i32,
    colour: u32,
) -> usize {
    with(|e| e.spawn_at(anm, kind, pos, param, count, colour, false))
}

pub fn spawn_high(anm: &mut AnmManager, kind: u8, pos: [f32; 3], colour: u32) -> usize {
    with(|e| e.spawn_high(anm, kind, pos, colour))
}

pub fn spawn_fixed(anm: &mut AnmManager, kind: u8, pos: [f32; 3], k: usize, colour: u32) -> usize {
    with(|e| e.spawn_fixed(anm, kind, pos, [0.0; 3], k, colour))
}

// Spell-card start ring at Spell_Start (07 §11.1, 13 §2.7): fixed slot 1, VM pos (8, 64) → (256, 0) over 100
// frames (ease 6); Spell_Calc turns pos.y into the wave amplitude while waves ≠ 0.
pub fn spell_ring(anm: &mut AnmManager, pos: [f32; 3], special: bool) {
    let i = spawn_fixed(
        anm,
        if special { 52 } else { 39 },
        pos,
        FIX_SPELL,
        0xFFFFFFFF,
    );
    get(i, |e| {
        e.vm.interp_pos_from(100, 6, [8.0, 64.0, 0.0], [256.0, 0.0, 0.0]);
        e.vm.pos = [8.0, 64.0, 0.0];
        e.pos = pos;
        e.ring = Ring {
            segments: 64,
            start: 0.0,
            radius: 256.0,
            width: 15.0,
            waves: 6.0,
            ..e.ring
        };
    });
}

// Effect manager task; effect level byte (cfg, 0x17CE894) for list B (13 §2.4).
pub struct EffectManager {
    pub level: u8,
}

pub fn bg_file(stage: usize) -> &'static str {
    // Table 0x4C7480 by stage index (13 §5).
    // Not implemented: the spell-practice eff09xx.anm files (table 0x4C7144).
    [
        "eff01.anm",
        "eff02.anm",
        "eff03.anm",
        "eff04a.anm",
        "eff04b.anm",
        "eff05.anm",
        "eff06.anm",
        "eff07.anm",
        "eff08.anm",
    ][stage % 9]
}

impl EffectManager {
    pub fn register(ctx: &mut Ctx) -> Result<std::rc::Rc<RefCell<EffectManager>>> {
        let m = std::rc::Rc::new(RefCell::new(EffectManager {
            level: ctx.game.cfg.effect_level,
        }));
        let t: task::TaskRef<Ctx> = m.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(m)
    }
}

impl Task<Ctx> for EffectManager {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            clear();
            crate::ascii::clear_popups();
            ctx.anm
                .load(&mut ctx.gfx, &ctx.archive, BG_SLOT, bg_file(ctx.game.stage))?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            clear();
            crate::ascii::clear_popups();
            ctx.anm.unload(&mut ctx.gfx, BG_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let paused = ctx.game.paused != 0 || ctx.game.game_over != 0;
        with(|e| {
            e.paused = paused;
            e.calc(&mut ctx.anm)
        });
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        with(|e| e.draw_main(&mut ctx.anm, &mut ctx.gfx));
        Ret::Next
    }
}

// --- screen effects (`ScreenEffect_Register`, 13 §3) ---

// "ScreenInf" 0x34 bytes (13 §3).
#[derive(Clone, Copy, Debug)]
pub struct ScreenEffect {
    // +0 type, +0x14.. p0–p3.
    pub kind: u8,
    pub p: [i32; 4],
    pub prio: u16,
    // +0x10 current alpha.
    alpha: f32,
    // +0x24 release flag.
    release: bool,
    // Port-only: this is the loading overlay.
    loading: bool,
    // +0x28.
    timer: Timer,
    // Port-only: calc returned 0, the draw node removes itself.
    done: bool,
}

thread_local! {
    static LOADING_RELEASE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// 0x4489CA(0) `Loading_StartDarken`: the loading darken overlay (type 5, 60 frames, black, draw prio 1), released when
// loading ends. Called from an on-register callback inside the Supervisor calc (prio 0), so it is linked at once (13 §3).
pub fn loading_overlay(ctx: &mut Ctx) {
    LOADING_RELEASE.set(false);
    link(
        ctx,
        ScreenEffect {
            prio: 1,
            loading: true,
            ..screen_fx(5, 60, 0, 0, 0)
        },
    );
}

// 0x45C160 from Loading_Wait / Loading_End.
pub fn release_loading_overlay() {
    LOADING_RELEASE.set(true);
}

pub const SCREEN_CALC_PRIO: u16 = 3;

// Queues a screen effect; registered after the calc pass (calc prio 3, draw at `prio`). Only valid for callers that run
// after calc prio 3: their effect's first calc is the next frame either way (13 §3).
pub fn screen(kind: u8, p0: i32, p1: i32, p2: i32, p3: i32, prio: u16) {
    with(|e| {
        e.screen.push(ScreenEffect {
            prio,
            ..screen_fx(kind, p0, p1, p2, p3)
        })
    });
}

// Test-only: drain the pending screen effects as (type, p0–p3).
#[cfg(test)]
pub fn take_screen() -> Vec<(u8, [i32; 4])> {
    with(|e| std::mem::take(&mut e.screen))
        .into_iter()
        .map(|s| (s.kind, s.p))
        .collect()
}

// ScreenEffect_Register from a calc node below prio 3 (`GameMgr_Calc`): the nodes are linked at once, so the first
// calc runs in the same frame (13 §3).
pub fn screen_now(ctx: &mut Ctx, kind: u8, p0: i32, p1: i32, p2: i32, p3: i32, prio: u16) {
    link(
        ctx,
        ScreenEffect {
            prio,
            ..screen_fx(kind, p0, p1, p2, p3)
        },
    );
}

pub fn register_pending(ctx: &mut Ctx) {
    let list = with(|e| std::mem::take(&mut e.screen));
    for s in list {
        link(ctx, s);
    }
}

// FuncChain_AddCalc (prio 3) + AddDraw (13 §3).
fn link(ctx: &mut Ctx, mut s: ScreenEffect) {
    s.timer.set(0);
    let t = task::task(s);
    let prio = s.prio;
    if task::add(ctx, Chain::Calc, SCREEN_CALC_PRIO, 0, t.clone()).is_ok() {
        let _ = task::add(ctx, Chain::Draw, prio, 0, t);
    }
}

impl ScreenEffect {
    // Offsets x then y, each one of {0, +a, −a} by Rng_Range(3): 2 RNG draws per frame.
    fn shake(&self, ctx: &mut Ctx, a: f32) {
        let mut pick = || match ctx.anm.rng.range(3) {
            0 => 0.0,
            1 => a,
            _ => -a,
        };
        let x = pick();
        let y = pick();
        ctx.anm.screen_offset = [x, y];
    }

    // Shakes 1 / 7 hold (no tick, no RNG) while GameFlags bit 10 or the freeze byte 0x160F534 is set (13 §3); both read
    // live, not the end-of-Player_Calc mirror `Effects::frozen`.
    fn frozen() -> bool {
        crate::gamemgr::flags() & crate::gamemgr::F_DEATH != 0 || crate::enemy::timer_pause() != 0
    }
}

impl Task<Ctx> for ScreenEffect {
    // Calc returning 0 removes the whole effect, draw node included (13 §3).
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let r = self.step(ctx);
        self.done = r == Ret::Remove;
        r
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if self.done {
            return Ret::Remove;
        }
        self.draw_fx(ctx);
        Ret::Next
    }
}

impl ScreenEffect {
    // Per-type calc (13 §3 table).
    fn step(&mut self, ctx: &mut Ctx) -> Ret {
        let t = self.timer.cur;
        let gs = ctx.anm.speed;
        let [p0, p1, p2, p3] = self.p;
        let dur = p0.max(1);
        match self.kind {
            0 => {
                self.alpha = (255.0 - 255.0 * t as f32 / dur as f32).max(0.0).trunc();
                if t >= dur {
                    return Ret::Remove;
                }
            }
            // 0x45BDC0: frozen → hold; 0x18B8A2C ≠ 0 → removed; else tick first, then end test, then a and 2 draws.
            1 => {
                if Self::frozen() {
                    return Ret::Next;
                }
                if crate::enemy::load_frames() != 0 {
                    return Ret::Remove;
                }
                self.timer.tick(gs);
                let t = self.timer.cur;
                if t >= dur {
                    return Ret::Remove;
                }
                let a = p1 as f32 + t as f32 * (p2 - p1) as f32 / dur as f32;
                self.shake(ctx, a);
                return Ret::Next;
            }
            // 0x45B760 (type 4 "as 2"): 0x18B8A2C ≠ 0 → removed before anything else.
            2 | 4 => {
                if crate::enemy::load_frames() != 0 {
                    return Ret::Remove;
                }
                self.alpha = (255 * t.min(dur) / dur) as f32;
                if t >= dur {
                    return Ret::Remove;
                }
                if ctx.game.paused != 0 || ctx.game.game_over != 0 {
                    return Ret::Next;
                }
            }
            3 => {
                let a = (p2 as u32 >> 24) as i32;
                self.alpha = (a - t * a / dur) as f32;
                if t >= dur {
                    self.p[1] -= 1;
                    if self.p[1] <= 0 {
                        return Ret::Remove;
                    }
                    self.timer.set(0);
                    return Ret::Next;
                }
            }
            5 | 6 => {
                if self.loading && !self.release && LOADING_RELEASE.take() {
                    self.release = true;
                    self.timer.set(0);
                }
                let t = self.timer.cur;
                if self.release {
                    if t > 8 {
                        return Ret::Remove;
                    }
                    self.alpha = (128 - t * 128 / 8) as f32;
                } else if t <= dur {
                    self.alpha = (t * 128 / dur) as f32;
                }
            }
            // 0x45BF10: frozen → hold; gameplay frame counter 0x164D2C8 < 2 → removed (no 0x18B8A2C test); else
            // tick first, then end test, then the 2 draws.
            7 => {
                if Self::frozen() {
                    return Ret::Next;
                }
                if ctx.game.frame_counter < 2 {
                    return Ret::Remove;
                }
                self.timer.tick(gs);
                let t = self.timer.cur;
                let (amp, i, h, o) = (p0 as f32, p1.max(1), p2, p3.max(1));
                if t >= i + h + o {
                    return Ret::Remove;
                }
                let tf = t as f32;
                let env = if t < i {
                    tf / i as f32
                } else if t < i + h {
                    1.0
                } else {
                    ((i + h + o - t) as f32 / o as f32).max(0.0)
                };
                self.shake(ctx, amp * env);
                return Ret::Next;
            }
            _ => return Ret::Remove,
        }
        self.timer.tick(gs);
        Ret::Next
    }

    // Types 0/4/5 full screen, 2/3/6 playfield; colour p1 (p2 for the flash).
    fn draw_fx(&mut self, ctx: &mut Ctx) {
        let colour = match self.kind {
            0 | 2 | 4 | 5 | 6 => self.p[1] as u32,
            3 => self.p[2] as u32,
            _ => return,
        };
        let rgba = [
            (colour >> 16) as u8,
            (colour >> 8) as u8,
            colour as u8,
            self.alpha.clamp(0.0, 255.0) as u8,
        ];
        let full = matches!(self.kind, 0 | 4 | 5);
        let r = if full { FULLSCREEN } else { PLAYFIELD };
        ctx.gfx.set_viewport(r);
        ctx.gfx.rect(r.x, r.y, r.w, r.h, rgba);
    }
}

// Player death ring (type 59, slot 11, colour 0xFFF0404F): radius 128 → 8 over the deathbomb window (ease 4,
// 13 §2.7).
// +0x357 (keeps running while bullets are frozen): set only for this ring (13 §2.1).
pub fn death_ring(anm: &mut AnmManager, pos: [f32; 3], frames: i32) {
    with(|e| {
        let i = e.spawn_fixed(anm, 59, pos, [0.0; 3], FIX_DEATH, 0xFFF0404F);
        let x = &mut e.fx[i];
        // Radius VM pos.x 128 → 8 over the deathbomb window, ease 4 (13 §3 player hit).
        x.vm.pos = [128.0, 32.0, 0.0];
        x.vm.interp_pos(frames.max(1), 4, [8.0, 32.0, 0.0]);
        x.keep_frozen = true;
        x.ring.segments = 64;
        x.ring.waves = 6.0;
    });
}

// Invulnerability ring at bomb start (Player_SpawnInvulnRing 0x40BF00, type 23, slot 0): I0 := invulnerability
// frames (13 §2.6, 14 §1.2).
pub fn invuln_ring(anm: &mut AnmManager, pos: [f32; 3], frames: i32) {
    let i = spawn_fixed(anm, 23, pos, FIX_INVULN, 0xFFFF4040);
    get(i, |e| e.vm.ivars[0] = frames);
}

#[cfg(test)]
mod tests {
    use super::*;

    // 14 §2.7 (RE-Q57 AO): the Sakuya follow write moves 2D/freed slots, never a live 3D particle's world pos.
    #[test]
    fn sakuya_follow_write_skips_3d_particles() {
        let i = 3;
        let old = with(|e| e.fx[i].clone());
        with(|e| {
            e.fx[i] = Effect {
                active: true,
                kind: 51,
                pos: [1.0, 2.0, -3.0],
                ..Effect::default()
            }
        });
        write_pos_unchecked(i, [100.0, 200.0, 0.0]);
        assert_eq!(peek(i, |x| x.pos), [1.0, 2.0, -3.0]);
        with(|e| e.fx[i].kind = 20);
        write_pos_unchecked(i, [100.0, 200.0, 0.0]);
        assert_eq!(peek(i, |x| x.pos), [100.0, 200.0, 0.0]);
        with(|e| (e.fx[i].kind, e.fx[i].active) = (51, false));
        write_pos_unchecked(i, [5.0, 6.0, 0.0]);
        assert_eq!(peek(i, |x| x.pos), [5.0, 6.0, 0.0]);
        with(|e| e.fx[i] = old);
    }

    // 13 §3 type 1/7: the shake freeze reads GameFlags bit 10 live, not the end-of-Player_Calc mirror.
    #[test]
    fn shake_frozen_reads_flags_live() {
        let f = crate::gamemgr::flags();
        set_frozen(false);
        crate::gamemgr::set_flags(f | crate::gamemgr::F_DEATH);
        assert!(ScreenEffect::frozen());
        crate::gamemgr::set_flags(f & !crate::gamemgr::F_DEATH);
        set_frozen(true);
        assert_eq!(ScreenEffect::frozen(), crate::enemy::timer_pause() != 0);
        set_frozen(false);
        crate::gamemgr::set_flags(f);
    }

    // Ring init 0x4272E0 (13 §2.6): 24 segments by default, centre = spawn pos, crossed param copy.
    #[test]
    fn ring_init_defaults() {
        let mut fx = Effects::default();
        let mut anm = AnmManager::default();
        let pos = [10.0, 20.0, 0.0];
        for (kind, n) in [
            (35, 24),
            (39, 24),
            (49, 24),
            (52, 24),
            (53, 24),
            (59, 24),
            (41, 48),
            (42, 64),
        ] {
            let i = fx.spawn_at(&mut anm, kind, pos, [0.0; 3], 1, 0, true);
            assert_eq!(fx.fx[i].ring.segments, n, "type {kind}");
            assert_eq!(fx.fx[i].centre, pos);
        }
        let i = FIXED_BASE;
        assert!(fx.init_start(&mut anm, i, 35, pos, [1.0, 2.0, 3.0], 0, true));
        let r = fx.fx[i].ring;
        assert_eq!((r.start, r.radius, r.width), (1.0, 2.0, 3.0));
    }

    // EffInit_Converge (13 §2.6): angle = R·2π − π, so dir lies on the unit circle at that angle.
    #[test]
    fn converge_angle_minus_pi() {
        for kind in [17, 18, 27] {
            let mut fx = Effects::default();
            let mut anm = AnmManager::default();
            anm.rng = crate::rng::Rng::seeded(0x1234);
            let mut probe = crate::rng::Rng::seeded(0x1234);
            let i = fx.spawn_at(&mut anm, kind, [0.0; 3], [0.0; 3], 1, 0, true);
            let rng = &anm.rng;
            // The type's script runs first and may draw; replay exactly as many draws before the init's one.
            let mut a = 0.0;
            while probe.state != rng.state {
                a = probe.next_f32() * (2.0 * PI) - PI;
            }
            assert_eq!(fx.fx[i].dir, [cos_f(a), sin_f(a), 0.0], "type {kind}");
        }
    }

    // 13 §2.2 "Spawn-time game speed": the spark init scales v and a by the speed at the spawn.
    #[test]
    fn spark_init_uses_spawn_speed() {
        let spark = |gs: f32| {
            let mut fx = Effects::default();
            let mut anm = AnmManager::default();
            anm.rng = crate::rng::Rng::seeded(0x59);
            anm.speed = gs;
            let i = fx.spawn_at(&mut anm, 8, [0.0; 3], [0.0; 3], 1, 0, true);
            (fx.fx[i].vel, fx.fx[i].acc)
        };
        let ((v1, a1), (v, a)) = (spark(1.0), spark(0.25));
        assert_eq!((v, a), (v1.map(|x| x * 0.25), a1.map(|x| x * 0.25)));
    }

    // 13 §2.6a: Vec3_Dot sums (x·x' + y·y') + z·z'; the udOa25 values are freed (< 0.94).
    #[test]
    fn vec3_dot_order() {
        let d = [0x3EC704B9, 0x3F451123, 0x3F019BF1].map(f32::from_bits);
        let f = [0x3D9A5339, 0x3F40E808, 0x3F272F7E].map(f32::from_bits);
        assert_eq!(dot(d, f).to_bits(), 0x3F70A3D6);
        assert!(dot(d, f) < 0.94);
    }

    // 13 §3 "Initial alpha": the registered object is zero-filled, so alpha is 0 before the first calc.
    #[test]
    fn screen_effect_starts_at_alpha_zero() {
        for kind in [0, 3] {
            assert_eq!(screen_fx(kind, 70, 0xFFFFFF, -1, 0).alpha, 0.0);
        }
    }

    // 13 §2.2: one 512-slot budget per call; exhausted → dummy and the cursor is back where it started (+ extra).
    #[test]
    fn spawn_shares_budget() {
        let mut fx = Effects::default();
        let mut anm = AnmManager::default();
        for x in &mut fx.fx[..RING_POOL - 3] {
            x.active = true;
        }
        fx.cursor = 100;
        assert_eq!(
            fx.spawn_at(&mut anm, 4, [0.0; 3], [0.0; 3], 4, 0, true),
            NONE
        );
        assert_eq!(fx.cursor, 100);
        assert_eq!(
            fx.fx[RING_POOL - 3..RING_POOL]
                .iter()
                .filter(|x| x.active)
                .count(),
            3
        );
        // count ≤ 0 fills every free slot until the budget ends.
        let mut fx = Effects::default();
        fx.cursor = 7;
        assert_eq!(
            fx.spawn_at(&mut anm, 4, [0.0; 3], [0.0; 3], 0, 0, true),
            NONE
        );
        assert!(fx.fx[..RING_POOL].iter().all(|x| x.active));
        assert_eq!(fx.cursor, 7);
        // Success returns the last slot taken.
        let mut fx = Effects::default();
        assert_eq!(fx.spawn_at(&mut anm, 4, [0.0; 3], [0.0; 3], 3, 0, true), 2);
        assert_eq!(fx.cursor, 3);
    }

    // 14 §2.13: s = radius · f32 0x3F3504F2; a wraps at ≥ π per side; rotations chain via Angle_AddNormalize.
    #[test]
    fn boundary_sides_wrap_and_chain() {
        let r = Ring {
            radius: 100.0,
            start: 3.0 * PI / 4.0,
            width: 4.0,
            ..Ring::default()
        };
        boundary_sides([192.0, 224.0, 0.0], &r, 70, 4, 100);
        let mut p = crate::player::Player::default();
        p.take_queued_areas();
        let s = 100.0 * f32::from_bits(0x3F3504F2);
        let d: Vec<_> = p.damage_areas.iter().filter(|a| a.active).collect();
        assert_eq!(d.len(), 4);
        let mut a = 3.0 * PI / 4.0 + PI / 4.0;
        for area in d {
            if a >= PI {
                a -= 2.0 * PI;
            }
            let rot = anmvm::wrap_add(a, PI / 2.0);
            assert_eq!(area.rot.to_bits(), rot.to_bits());
            assert!((-PI..=PI).contains(&area.rot));
            assert_eq!(area.size, [8.0 * s, 16.0]);
            a = rot;
        }
    }
}
