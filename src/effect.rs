use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::game::Ctx;
use crate::gfx::{Camera, FULLSCREEN, Gfx, PLAYFIELD, Vertex};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::f32::consts::PI;

pub const CALC_PRIO: u16 = 13;
pub const DRAW_PRIO: u16 = 12;
pub const SCRIPT_SLOT: usize = 6;
pub const BG_SLOT: usize = 9;
const RING_POOL: usize = 512;
const HIGH_POOL: usize = 128;
const FIXED: usize = 13;
const TOTAL: usize = RING_POOL + HIGH_POOL + FIXED;
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

fn is_ring(kind: u8) -> bool {
    matches!(kind, 35..=37 | 39..=46 | 48..=50 | 52..=56 | 58 | 59 | 64 | 65)
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Ring {
    pub radius: f32,
    pub start: f32,
    pub segments: i32,
    pub width: f32,
    pub amp: f32,
    pub tilt: f32,
    pub waves: f32,
}

#[derive(Clone, Default)]
pub struct Effect {
    pub vm: AnmVm,
    pub pos: [f32; 3],
    pub param: [f32; 3],
    pub vel: [f32; 3],
    pub acc: [f32; 3],
    pub centre: [f32; 3],
    pub dir: [f32; 3],
    pub ring: Ring,
    pub age: Timer,
    pub active: bool,
    pub kind: u8,
    pub fade: bool,
    fade_n: u8,
    pub layer: u8,
    pub above: bool,
    pub keep_frozen: bool,
    pub life: i32,
    colour: u32,
    zoff: bool,
    pending: bool,
    spin: f32,
    rot: f32,
    pull: [f32; 3],
    disp: [f32; 3],
    pull_speed: f32,
    death_ring: i32,
}

#[derive(Clone, Copy, Default)]
pub struct View {
    pub cam: Option<Camera>,
    pub pos: [f32; 3],
    pub eye: [f32; 3],
    pub facing: [f32; 3],
    pub right: [f32; 3],
}

pub struct Effects {
    pub fx: Vec<Effect>,
    cursor: usize,
    pub live: usize,
    pub frozen: bool,
    pub player: [f32; 3],
    pub view: View,
    pub boss: Option<[f32; 3]>,
    pub overlay: [u8; 4],
    pub paused: bool,
    lists: [Vec<usize>; 5],
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

// Reads a slot even after it was freed (the original reads stale slot data).
pub fn peek<R>(idx: usize, f: impl FnOnce(&Effect) -> R) -> R {
    with(|e| f(&e.fx[idx]))
}

pub fn free(idx: usize) {
    get(idx, |x| x.active = false);
}

pub fn shift_kind(kind: u8, d: [f32; 3]) {
    with(|e| {
        for x in e.fx.iter_mut().filter(|x| x.active && x.kind == kind) {
            x.pos = std::array::from_fn(|k| x.pos[k] + d[k]);
        }
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

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        a
    }
}

impl Effect {
    // Enemy_UpdateOrbitEffects 0x42E010 (answers_impl9 §5).
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
    fn slot_ring(&mut self) -> Option<usize> {
        for _ in 0..RING_POOL {
            self.cursor = (self.cursor + 1) % RING_POOL;
            if !self.fx[self.cursor].active {
                return Some(self.cursor);
            }
        }
        None
    }

    fn slot_high(&self) -> Option<usize> {
        (RING_POOL..RING_POOL + HIGH_POOL).find(|&i| !self.fx[i].active)
    }

    // Common init (13 §2.2) and the per-type init; false = init asked to free the slot.
    // The VM script is started by `start` (at once with an AnmManager, else at the next calc).
    fn init(
        &mut self,
        rng: &mut crate::rng::Rng,
        gs: f32,
        i: usize,
        kind: u8,
        pos: [f32; 3],
        param: [f32; 3],
        colour: u32,
        zoff: bool,
    ) -> bool {
        self.reset(i, kind, pos, param, colour, zoff);
        self.type_init(rng, gs, i)
    }

    fn reset(
        &mut self,
        i: usize,
        kind: u8,
        pos: [f32; 3],
        param: [f32; 3],
        colour: u32,
        zoff: bool,
    ) {
        let e = &mut self.fx[i];
        *e = Effect {
            active: true,
            kind,
            pos,
            param,
            colour,
            zoff,
            pending: true,
            ..Effect::default()
        };
        e.age.set(0);
    }

    fn type_init(&mut self, rng: &mut crate::rng::Rng, gs: f32, i: usize) -> bool {
        let player = self.player;
        let view = self.view;
        let e = &mut self.fx[i];
        let (kind, pos, param) = (e.kind, e.pos, e.param);
        match kind {
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
            13..=15 => {
                e.layer = 2;
                e.centre = pos;
                e.dir = [0.0; 3];
                e.ring.radius = 0.0;
            }
            17 | 18 | 27 => {
                e.centre = pos;
                let a = rng.next_f32() * 2.0 * PI;
                e.dir = [a.cos(), a.sin(), 0.0];
                e.life = match kind {
                    17 => 60,
                    18 => 240,
                    _ => 120,
                };
            }
            // 3D background particles (answers_impl9 §4): base = camera + facing/2.
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
                    let ox = rng.next_signed() * 60.0;
                    let oy = if kind == 51 {
                        rng.next_signed() * 100.0 - 50.0
                    } else {
                        rng.next_signed() * 200.0 - 200.0
                    };
                    let oz = rng.next_f32() * 100.0 - 100.0;
                    let vx = rng.next_signed() * 0.001 + param[0];
                    let vy = rng.next_signed() * 0.03 + if kind == 51 { param[1] } else { 0.4 };
                    let vz = -(rng.next_f32() * 0.1) - 0.3 + param[2];
                    let ax = rng.next_signed() * 0.0001;
                    let ay = rng.next_signed() * 0.0001;
                    (off, v, a) = ([ox, oy, oz], [vx, vy, vz], [ax, ay, -0.0003]);
                    e.pull = [-9999.0, 0.0, 0.0];
                }
                e.pos = std::array::from_fn(|k| base[k] + off[k]);
                e.vel = v.map(|x| x * gs);
                e.acc = a.map(|x| x * gs);
                e.layer = 1;
            }
            21 | 26 => {
                e.centre = pos;
                let a = if param[0] > 0.0 {
                    param[0]
                } else {
                    rng.next_f32() * 2.0 * PI
                };
                let m = 1.0 + rng.next_f32() * 1.5;
                e.dir = [a.cos() * m, a.sin() * m, 0.0];
            }
            22..=25 => e.pos = player,
            60 | 61 => {
                e.pos[0] += param[0].cos() * 256.0;
                e.pos[1] += param[0].sin() * 256.0;
                e.spin = param[0] + PI / 2.0;
            }
            _ => {}
        }
        if is_ring(kind) {
            e.centre = pos;
            e.ring.segments = if kind == 41 { 48 } else { 32 };
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

    // Common init's script part (13 §2.2), then the VM fields the type init sets.
    fn start(&mut self, anm: &mut AnmManager, i: usize) {
        if self.start_script(anm, i) {
            self.start_post(i);
        }
    }

    // The script runs its t = 0 step here, so its RNG draws precede the type init's (13 §2.6).
    fn start_script(&mut self, anm: &mut AnmManager, i: usize) -> bool {
        let e = &mut self.fx[i];
        if !std::mem::take(&mut e.pending) {
            return false;
        }
        let script = SCRIPTS.get(e.kind as usize).copied().unwrap_or(-1);
        if script >= 0 {
            anm.set_script(&mut e.vm, SCRIPT_SLOT, script as usize);
        }
        if e.zoff {
            e.vm.flags |= anmvm::F_ZWRITE_OFF;
        }
        e.vm.color = e.colour.to_le_bytes();
        e.vm.pos_offset = [0.0; 3];
        true
    }

    fn start_post(&mut self, i: usize) {
        let e = &mut self.fx[i];
        if e.death_ring > 0 {
            e.vm.pos = [128.0, 32.0, 0.0];
            e.vm.interp_pos(e.death_ring, 4, [8.0, 32.0, 0.0]);
        }
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
        let gs = anm.speed;
        self.reset(i, kind, pos, param, colour, zoff);
        self.start_script(anm, i);
        let ok = self.type_init(&mut anm.rng, gs, i);
        if ok {
            self.start_post(i);
        }
        ok
    }

    // Spawn from code that only holds the RNG (Player_Die, graze); the script starts at the next calc.
    pub fn spawn_rng(
        &mut self,
        rng: &mut crate::rng::Rng,
        kind: u8,
        pos: [f32; 3],
        count: i32,
        colour: u32,
    ) -> usize {
        let mut last = NONE;
        for _ in 0..count.max(1) {
            let Some(i) = self.slot_ring() else { break };
            if self.init(rng, 1.0, i, kind, pos, [0.0; 3], colour, true) {
                last = i;
            } else {
                self.fx[i].active = false;
            }
        }
        last
    }

    pub fn spawn_fixed_rng(
        &mut self,
        rng: &mut crate::rng::Rng,
        kind: u8,
        pos: [f32; 3],
        k: usize,
        colour: u32,
    ) -> usize {
        let i = FIXED_BASE + k.min(FIXED - 1);
        self.init(rng, 1.0, i, kind, pos, [0.0; 3], colour, true);
        i
    }

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
        let mut last = NONE;
        for _ in 0..count.max(1) {
            let Some(i) = self.slot_ring() else { break };
            if self.init_start(anm, i, kind, pos, param, colour, zoff) {
                last = i;
            } else {
                self.fx[i].active = false;
            }
        }
        last
    }

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
            3..=11 => {
                for k in 0..2 {
                    e.pos[k] += e.vel[k];
                    e.vel[k] += e.acc[k];
                }
            }
            13..=15 => {
                let n = norm(e.dir);
                let th = 2.0 * e.ring.start;
                let mut p = [n[1], -n[0], 0.0];
                if dot(p, p) >= 1e-5 {
                    p = norm(p);
                }
                p = p.map(|x| x * e.ring.radius);
                let (s, c) = th.sin_cos();
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
            17 | 18 | 27 => {
                let r = 256.0 - 256.0 * t / e.life as f32;
                e.pos = [
                    e.centre[0] + e.dir[0] * r,
                    e.centre[1] + e.dir[1] * r,
                    if e.kind == 18 { e.pos[2] } else { 0.0 },
                ];
            }
            19 | 51 | 63 => {
                for k in 0..3 {
                    e.vel[k] += e.acc[k];
                    e.pos[k] += e.vel[k];
                }
                let d = norm(sub(e.pos, view.pos));
                if dot(d, norm(view.facing)) < 0.94 {
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
                    if e.kind == 51 {
                        e.vm.flags |= anmvm::F_COLOR2;
                        e.vm.color2 = std::array::from_fn(|k| {
                            ((e.vm.color[k] as u32 * overlay[k] as u32) >> 8) as u8
                        });
                    }
                }
            }
            21 | 26 => {
                let f = 1.0 - (1.0 - (t / 90.0).min(1.0)).powi(2);
                e.pos = [
                    e.centre[0] + e.dir[0] * 128.0 * f,
                    e.centre[1] + e.dir[1] * 128.0 * f,
                    0.0,
                ];
            }
            22 => e.pos = player,
            // Invulnerability ring (13 §2.6): scale 1 → 1/16 over I0 frames (the script fades it in).
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

    // EffectMgr_Calc (13 §2.3).
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
            self.start(anm, i);
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

    // EffectMgr_Draw (prio 12): list A, then C, then E.
    pub fn draw_main(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        gfx.set_viewport(PLAYFIELD);
        for list in [0, 2, 4] {
            for k in 0..self.lists[list].len() {
                let i = self.lists[list][k];
                self.draw_one(anm, gfx, i, 0.07);
            }
        }
    }

    // List D, drawn at the end of BulletMgr_Draw.
    pub fn draw_over_bullets(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for k in 0..self.lists[3].len() {
            let i = self.lists[3][k];
            self.draw_one(anm, gfx, i, 0.04);
        }
    }

    // List B from Stage_DrawHigh (answers_impl9 §8): only effect level 2 draws; no fog.
    pub fn draw_background(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, level: u8) {
        if level < 2 {
            return;
        }
        let Some(cam) = self.view.cam else { return };
        let right = self.view.right;
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
            let sc = ((s2[0] - s[0]).powi(2) + (s2[1] - s[1]).powi(2)).sqrt();
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
            let (pos, scale) = (e.vm.pos, e.vm.scale);
            e.vm.pos = s;
            e.vm.scale = [scale[0] * sc, scale[1] * sc];
            e.vm.flags |= anmvm::F_SCALE_DIRTY;
            anm.draw_mode_z(gfx, &mut e.vm, 2, s[2]);
            e.vm.pos = pos;
            e.vm.scale = scale;
            e.vm.flags |= anmvm::F_SCALE_DIRTY;
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
    let (cx, cy) = (e.centre[0] + 32.0 + 0.5, e.centre[1] + 16.0 + 0.5);
    let c = vm.color;
    let rgba = anm.tinted([c[anmvm::R], c[anmvm::G], c[anmvm::B], c[anmvm::A]]);
    let wrap = |a: f32| if a >= PI { a - 2.0 * PI } else { a };
    let point = |k: i32, outer: bool| -> [f32; 2] {
        let th = wrap(r.start + k as f32 * step);
        let sgn = if outer { 1.0 } else { -1.0 };
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
        let v = std::array::from_fn(|i| Vertex::screen(p[i][0], p[i][1], 0.07, uv[i], rgba));
        gfx.strip(spr.tex, blend, false, v);
    }
}

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
        37 => {
            r.start += if slot % 2 == 0 { -PI / 80.0 } else { PI / 80.0 };
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
        46 => r.radius += 8.0,
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

// The four side rectangles of a finished boundary (D rect 8s × 4w, C rect 4s × 4w, rotated a + π/2).
fn boundary_sides(centre: [f32; 3], r: &Ring, d_life: i32, period: i32, c_life: i32) {
    let s = r.radius * 0.70711;
    let mut a = r.start + PI / 4.0;
    for _ in 0..4 {
        let c = [centre[0] + a.cos() * s, centre[1] + a.sin() * s];
        let rot = a + PI / 2.0;
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
        a += PI / 2.0;
    }
}

fn screen_fx(kind: u8, p0: i32, p1: i32, p2: i32, p3: i32) -> ScreenEffect {
    ScreenEffect {
        kind,
        p: [p0, p1, p2, p3],
        prio: 0x15,
        // A draw before the first calc shows the t = 0 alpha.
        alpha: match kind {
            0 => 255.0,
            3 => (p2 as u32 >> 24) as f32,
            _ => 0.0,
        },
        release: false,
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

pub fn spawn_rng(
    rng: &mut crate::rng::Rng,
    kind: u8,
    pos: [f32; 3],
    count: i32,
    colour: u32,
) -> usize {
    with(|e| e.spawn_rng(rng, kind, pos, count, colour))
}

pub fn spawn_high(anm: &mut AnmManager, kind: u8, pos: [f32; 3], colour: u32) -> usize {
    with(|e| e.spawn_high(anm, kind, pos, colour))
}

pub fn spawn_fixed(anm: &mut AnmManager, kind: u8, pos: [f32; 3], k: usize, colour: u32) -> usize {
    with(|e| e.spawn_fixed(anm, kind, pos, [0.0; 3], k, colour))
}

// Spell-card start ring (answers_impl9 §P2): plain ring, 64 segments, radius 8 → 256 over 100 frames (ease 6).
pub fn spell_ring(anm: &mut AnmManager, pos: [f32; 3], special: bool) {
    let i = spawn_fixed(
        anm,
        if special { 52 } else { 39 },
        pos,
        FIX_SPELL,
        0xFFFFFFFF,
    );
    get(i, |e| {
        e.ring.segments = 64;
        e.ring.waves = 6.0;
        e.vm.pos = [8.0, 64.0, 0.0];
        e.vm.interp_pos(100, 6, [256.0, 0.0, 0.0]);
    });
}

// Effect level byte (cfg +0x24) for list B.
pub struct EffectManager {
    pub level: u8,
}

pub fn bg_file(stage: usize) -> &'static str {
    // Table 0x4C7480 by stage index (answers_impl9 §7).
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

#[derive(Clone, Copy, Debug)]
pub struct ScreenEffect {
    pub kind: u8,
    pub p: [i32; 4],
    pub prio: u16,
    alpha: f32,
    release: bool,
    timer: Timer,
    done: bool,
}

pub const SCREEN_CALC_PRIO: u16 = 3;

// Queues a screen effect; registered after the calc pass (calc prio 3, draw at `prio`).
pub fn screen(kind: u8, p0: i32, p1: i32, p2: i32, p3: i32, prio: u16) {
    with(|e| {
        e.screen.push(ScreenEffect {
            prio,
            ..screen_fx(kind, p0, p1, p2, p3)
        })
    });
}

pub fn register_pending(ctx: &mut Ctx) {
    let list = with(|e| std::mem::take(&mut e.screen));
    for mut s in list {
        s.timer.set(0);
        let t = task::task(s);
        let prio = s.prio;
        if task::add(ctx, Chain::Calc, SCREEN_CALC_PRIO, 0, t.clone()).is_ok() {
            let _ = task::add(ctx, Chain::Draw, prio, 0, t);
        }
    }
}

impl ScreenEffect {
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

    fn frozen(ctx: &Ctx) -> bool {
        with(|e| e.frozen) || ctx.game.paused != 0 || ctx.game.game_over != 0
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
            1 => {
                if Self::frozen(ctx) {
                    return Ret::Next;
                }
                let a = p1 as f32 + t as f32 * (p2 - p1) as f32 / dur as f32;
                self.shake(ctx, a);
                if t >= dur {
                    return Ret::Remove;
                }
            }
            2 | 4 => {
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
                if self.release {
                    self.alpha = (128.0 - 128.0 * t as f32 / 8.0).max(0.0);
                    if t >= 8 {
                        return Ret::Remove;
                    }
                } else {
                    self.alpha = (128 * t.min(dur) / dur) as f32;
                }
            }
            7 => {
                if Self::frozen(ctx) {
                    return Ret::Next;
                }
                let (amp, i, h, o) = (p0 as f32, p1.max(1), p2, p3.max(1));
                let tf = t as f32;
                let env = if t < i {
                    tf / i as f32
                } else if t < i + h {
                    1.0
                } else {
                    ((i + h + o - t) as f32 / o as f32).max(0.0)
                };
                self.shake(ctx, amp * env);
                if t >= i + h + o {
                    return Ret::Remove;
                }
            }
            _ => return Ret::Remove,
        }
        self.timer.tick(gs);
        Ret::Next
    }

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

// Player death ring (type 59, slot 11, colour 0xFFF0404F): radius 128 → 8 over the deathbomb window (ease 4).
pub fn death_ring(rng: &mut crate::rng::Rng, pos: [f32; 3], frames: i32) {
    with(|e| {
        let i = e.spawn_fixed_rng(rng, 59, pos, FIX_DEATH, 0xFFF0404F);
        let x = &mut e.fx[i];
        x.death_ring = frames.max(1);
        x.keep_frozen = true;
        x.ring.segments = 64;
        x.ring.waves = 6.0;
    });
}

// Invulnerability ring at bomb start (type 23, slot 0): I0 := invulnerability frames.
pub fn invuln_ring(anm: &mut AnmManager, pos: [f32; 3], frames: i32) {
    let i = spawn_fixed(anm, 23, pos, FIX_INVULN, 0xFFFF4040);
    get(i, |e| e.vm.ivars[0] = frames);
}
