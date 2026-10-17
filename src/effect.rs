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
pub const FIX_DEATHBOMB: usize = 3;
pub const FIX_GAUGE: usize = 8;
pub const FIX_DEATH: usize = 11;

// Effect type → etama.anm script (table 0x4C6D30, 13 §2.6).
const SCRIPTS: [i32; 66] = [
    28, 29, 30, 31, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 45, 45, 0, 32, 33, 51, 56, 52, 54, 104, 104, 35, 53, 34, 57, 58, 59, 60, 48, 49, 50, 88, 88, 92, 71,
    76, 81, 82, 83, 83, 83, 83, 84, 72, 85, 86, 80, 73, 77, 88, 88, 87, 96, 55, 100, 78, 102, 103, 75, 74, 77, 98,
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
    death_ring: bool,
}

#[derive(Clone, Copy, Default)]
pub struct View {
    pub cam: Option<Camera>,
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

pub fn free(idx: usize) {
    get(idx, |x| x.active = false);
}

pub fn clear() {
    with(|e| *e = Effects { player: e.player, ..Effects::default() });
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { a }
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
    fn init(&mut self, rng: &mut crate::rng::Rng, gs: f32, i: usize, kind: u8, pos: [f32; 3], param: [f32; 3], colour: u32, zoff: bool) -> bool {
        let player = self.player;
        let view = self.view;
        let e = &mut self.fx[i];
        *e = Effect { active: true, kind, pos, param, colour, zoff, pending: true, ..Effect::default() };
        e.age.set(0);
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
                e.layer = 0;
                e.centre = pos;
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
            // SPEC-GAP: the 9 RNG draws of the 3D particles are consumed, but their exact meaning is not given;
            // they place the particle in front of the stage camera with a slow drift.
            19 | 51 | 63 => {
                let r: [f32; 10] = std::array::from_fn(|k| if k < 9 || kind == 19 { rng.next_f32() } else { 0.0 });
                let dist = 200.0 + r[0] * 600.0;
                let side = [(r[1] - 0.5) * dist * 1.2, (r[2] - 0.5) * dist];
                let up = [0.0, 0.0, 1.0];
                e.pos = [
                    view.eye[0] + view.facing[0] * dist + view.right[0] * side[0] + up[0] * side[1],
                    view.eye[1] + view.facing[1] * dist + view.right[1] * side[0] + up[1] * side[1],
                    view.eye[2] + view.facing[2] * dist + view.right[2] * side[0] + up[2] * side[1],
                ];
                e.vel = [(r[3] - 0.5) * 2.0, (r[4] - 0.5) * 2.0, (r[5] - 0.5) * 2.0];
                e.acc = [(r[6] - 0.5) * 0.02, (r[7] - 0.5) * 0.02, (r[8] - 0.5) * 0.02];
                e.spin = (r[9] - 0.5) * 0.1;
                e.layer = 1;
            }
            21 | 26 => {
                e.centre = pos;
                let a = if param[0] > 0.0 { param[0] } else { rng.next_f32() * 2.0 * PI };
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
        true
    }

    // Common init's script part (13 §2.2), then the VM fields the type init sets.
    fn start(&mut self, anm: &mut AnmManager, i: usize) {
        let e = &mut self.fx[i];
        if !std::mem::take(&mut e.pending) {
            return;
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
        if e.death_ring {
            e.vm.pos = [16.0, 0.0, 0.0];
            e.vm.interp_pos(60, 4, [192.0, 0.0, 0.0]);
        }
        match e.kind {
            19 | 51 | 63 => e.vm.angular_vel[2] = e.spin,
            60 | 61 => {
                e.vm.rotation[2] = e.spin;
                e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            _ => {}
        }
    }

    fn init_start(&mut self, anm: &mut AnmManager, i: usize, kind: u8, pos: [f32; 3], param: [f32; 3], colour: u32, zoff: bool) -> bool {
        let gs = anm.speed;
        let ok = self.init(&mut anm.rng, gs, i, kind, pos, param, colour, zoff);
        if ok {
            self.start(anm, i);
        }
        ok
    }

    // Spawn from code that only holds the RNG (Player_Die, graze); the script starts at the next calc.
    pub fn spawn_rng(&mut self, rng: &mut crate::rng::Rng, kind: u8, pos: [f32; 3], count: i32, colour: u32) -> usize {
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

    pub fn spawn_fixed_rng(&mut self, rng: &mut crate::rng::Rng, kind: u8, pos: [f32; 3], k: usize, colour: u32) -> usize {
        let i = FIXED_BASE + k.min(FIXED - 1);
        self.init(rng, 1.0, i, kind, pos, [0.0; 3], colour, true);
        i
    }

    pub fn spawn_at(&mut self, anm: &mut AnmManager, kind: u8, pos: [f32; 3], param: [f32; 3], count: i32, colour: u32, zoff: bool) -> usize {
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

    pub fn spawn_high(&mut self, anm: &mut AnmManager, kind: u8, pos: [f32; 3], colour: u32) -> usize {
        let Some(i) = self.slot_high() else { return NONE };
        self.init_start(anm, i, kind, pos, [0.0; 3], colour, true);
        self.fx[i].layer = 0;
        i
    }

    pub fn spawn_fixed(&mut self, anm: &mut AnmManager, kind: u8, pos: [f32; 3], param: [f32; 3], k: usize, colour: u32) -> usize {
        let i = FIXED_BASE + k.min(FIXED - 1);
        self.init_start(anm, i, kind, pos, param, colour, true);
        i
    }

    // Per-type update (13 §2.6); false frees the effect.
    fn update(&mut self, i: usize, gs: f32) -> bool {
        let player = self.player;
        let view = self.view;
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
                e.pos = [e.centre[0] + e.vm.pos_offset[0], e.centre[1] + e.vm.pos_offset[1], 0.0];
                if e.fade {
                    e.fade_n += 1;
                    e.vm.color[anmvm::A] = (255.0 * (1.0 - e.fade_n as f32 / 16.0)).max(0.0) as u8;
                    e.vm.scale = e.vm.scale.map(|s| s * 1.05);
                    e.vm.flags |= anmvm::F_SCALE_DIRTY;
                    if e.fade_n >= 15 {
                        return false;
                    }
                }
            }
            17 | 18 | 27 => {
                let r = 256.0 - 256.0 * t / e.life as f32;
                e.pos = [e.centre[0] + e.dir[0] * r, e.centre[1] + e.dir[1] * r, if e.kind == 18 { e.pos[2] } else { 0.0 }];
            }
            19 | 51 | 63 => {
                for k in 0..3 {
                    e.pos[k] += e.vel[k] * gs;
                    e.vel[k] += e.acc[k] * gs;
                }
                let d = norm(sub(e.pos, view.eye));
                if view.cam.is_some() && dot(d, norm(view.facing)) < 0.94 || e.pos[2] < 0.0 {
                    return false;
                }
            }
            21 | 26 => {
                let f = 1.0 - (1.0 - (t / 90.0).min(1.0)).powi(2);
                e.pos = [e.centre[0] + e.dir[0] * 128.0 * f, e.centre[1] + e.dir[1] * 128.0 * f, 0.0];
            }
            22 | 25 => e.pos = player,
            // Invulnerability ring (13 §2.6): scale 1 → 1/16 over I0 frames (the script fades it in).
            23 | 24 => {
                e.pos = player;
                let n = e.vm.ivars[0].max(1) as f32;
                let s = 1.0 + (1.0 / 16.0 - 1.0) * (t / n).min(1.0);
                e.vm.scale = [s, s];
                e.vm.flags |= anmvm::F_SCALE_DIRTY;
            }
            _ => {}
        }
        if is_ring(e.kind) {
            let r = &mut e.ring;
            r.width = e.vm.scale[0];
            r.radius = e.vm.pos[0];
            r.amp = e.vm.pos[1];
            r.start = e.vm.rotation[2];
            r.tilt = e.vm.rotation[1];
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
                if !self.update(i, gs) {
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
        e.vm.pos = [e.pos[0] + 32.0 + e.vm.pos_offset[0], e.pos[1] + 16.0 + e.vm.pos_offset[1], 0.0];
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

    // List B from Stage_DrawHigh; effect level 1 draws every other element. SPEC-GAP: no fog tint for 0x33/0x3F.
    pub fn draw_background(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, level: u8) {
        if level == 0 {
            return;
        }
        let Some(cam) = self.view.cam else { return };
        let right = self.view.right;
        let step = if level == 1 { 2 } else { 1 };
        for k in (0..self.lists[1].len()).step_by(step) {
            let i = self.lists[1][k];
            let e = &mut self.fx[i];
            if e.layer == 4 {
                self.draw_one(anm, gfx, i, 0.07);
                continue;
            }
            let s = cam.project(e.pos, PLAYFIELD);
            let w = e.vm.sprite_w.max(1.0);
            let s2 = cam.project([e.pos[0] + right[0] * w, e.pos[1] + right[1] * w, e.pos[2] + right[2] * w], PLAYFIELD);
            let sc = ((s2[0] - s[0]).powi(2) + (s2[1] - s[1]).powi(2)).sqrt() / w;
            if !(0.0..=1.0).contains(&s[2]) {
                continue;
            }
            let (pos, scale) = (e.vm.pos, e.vm.scale);
            e.vm.pos = s;
            e.vm.scale = [scale[0] * sc, scale[1] * sc];
            e.vm.flags |= anmvm::F_SCALE_DIRTY;
            anm.draw_mode_z(gfx, &mut e.vm, 2, s[2]);
            e.vm.pos = pos;
            e.vm.scale = scale;
        }
    }
}

// Ring renderer 0x427450 (13 §2.5): strip of 2(n+1) vertices, u along the ring, VM colour.
fn draw_ring(anm: &mut AnmManager, gfx: &mut Gfx, e: &Effect) {
    let vm = &e.vm;
    if vm.flags & (anmvm::F_VISIBLE | anmvm::F_ENABLED) != (anmvm::F_VISIBLE | anmvm::F_ENABLED) || vm.color[anmvm::A] == 0 || !vm.has_sprite {
        return;
    }
    let Some(&spr) = anm.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else { return };
    let r = e.ring;
    let n = r.segments.max(3);
    let step = 2.0 * PI / n as f32;
    let w = r.width / ((PI - step) / 2.0).sin();
    let o = anm.screen_offset;
    let (cx, cy) = (e.centre[0] + 32.0 + o[0] + 0.5, e.centre[1] + 16.0 + o[1] + 0.5);
    let c = vm.color;
    let rgba = anm.tinted([c[anmvm::R], c[anmvm::G], c[anmvm::B], c[anmvm::A]]);
    let point = |k: i32, outer: bool| -> [f32; 2] {
        let th = r.start + k as f32 * step;
        let sgn = if outer { 1.0 } else { -1.0 };
        if r.amp == 0.0 {
            let rr = r.radius + sgn * w;
            [cx + th.cos() * rr, cy + th.sin() * rr]
        } else if r.waves == 0.0 {
            let th = k as f32 * step;
            let (x, y) = (th.cos() * (r.radius + sgn * w), th.sin() * (r.amp + sgn * w));
            let (s, co) = r.tilt.sin_cos();
            [cx + x * co - y * s, cy + x * s + y * co]
        } else {
            let phi = r.tilt + k as f32 * 2.0 * PI * r.waves / n as f32;
            let rr = r.radius + phi.cos() * r.amp + sgn * w;
            [cx + th.cos() * rr, cy + th.sin() * rr]
        }
    };
    let blend = anm.blend_for(vm);
    let (u0, v0, u1, v1) = (spr.uv[0], spr.uv[1], spr.uv[2], spr.uv[3]);
    for k in 0..n {
        let (ua, ub) = (u0 + (u1 - u0) * k as f32 / n as f32, u0 + (u1 - u0) * (k + 1) as f32 / n as f32);
        let p = [point(k, true), point(k, false), point(k + 1, true), point(k + 1, false)];
        let uv = [[ua, v0], [ua, v1], [ub, v0], [ub, v1]];
        let v = std::array::from_fn(|i| Vertex::screen(p[i][0], p[i][1], 0.07, uv[i], rgba));
        gfx.strip(spr.tex, blend, false, v);
    }
}

pub fn spawn(anm: &mut AnmManager, kind: u8, pos: [f32; 3], count: i32, colour: u32) -> usize {
    with(|e| e.spawn_at(anm, kind, pos, [0.0; 3], count, colour, true))
}

pub fn spawn_param(anm: &mut AnmManager, kind: u8, pos: [f32; 3], param: [f32; 3], count: i32, colour: u32) -> usize {
    with(|e| e.spawn_at(anm, kind, pos, param, count, colour, false))
}

pub fn spawn_rng(rng: &mut crate::rng::Rng, kind: u8, pos: [f32; 3], count: i32, colour: u32) -> usize {
    with(|e| e.spawn_rng(rng, kind, pos, count, colour))
}

pub fn spawn_high(anm: &mut AnmManager, kind: u8, pos: [f32; 3], colour: u32) -> usize {
    with(|e| e.spawn_high(anm, kind, pos, colour))
}

pub fn spawn_fixed(anm: &mut AnmManager, kind: u8, pos: [f32; 3], k: usize, colour: u32) -> usize {
    with(|e| e.spawn_fixed(anm, kind, pos, [0.0; 3], k, colour))
}

// Spell-card start ring (13 §2.7): 64 segments, width 15, 6 waves, radius 8 → 256 and amplitude 64 → 0 over 100 frames.
pub fn spell_ring(anm: &mut AnmManager, pos: [f32; 3], special: bool) {
    let i = spawn_fixed(anm, if special { 52 } else { 39 }, pos, FIX_SPELL, 0xFFFFFFFF);
    get(i, |e| {
        e.vm.scale = [15.0, 15.0];
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
    // SPEC-GAP: table 0x4C7480 is not listed; effNN follows the stage order.
    ["eff01.anm", "eff02.anm", "eff03.anm", "eff04a.anm", "eff04b.anm", "eff05.anm", "eff06.anm", "eff07.anm", "eff08.anm"][stage % 9]
}

impl EffectManager {
    pub fn register(ctx: &mut Ctx) -> Result<std::rc::Rc<RefCell<EffectManager>>> {
        let m = std::rc::Rc::new(RefCell::new(EffectManager { level: ctx.game.cfg.effect_level }));
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
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, BG_SLOT, bg_file(ctx.game.stage))?;
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            clear();
            ctx.anm.unload(&mut ctx.gfx, BG_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        with(|e| e.calc(&mut ctx.anm));
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
}

pub const SCREEN_CALC_PRIO: u16 = 3;

// Queues a screen effect; registered after the calc pass (calc prio 3, draw at `prio`).
pub fn screen(kind: u8, p0: i32, p1: i32, p2: i32, p3: i32, prio: u16) {
    with(|e| e.screen.push(ScreenEffect { kind, p: [p0, p1, p2, p3], prio, alpha: 0.0, release: false, timer: Timer::default() }));
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
        with(|e| e.frozen)
            || ctx.game.paused != 0
            || ctx.game.game_over != 0
    }
}

impl Task<Ctx> for ScreenEffect {
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let t = self.timer.cur;
        let gs = ctx.anm.speed;
        let [p0, p1, p2, p3] = self.p;
        let dur = p0.max(1);
        match self.kind {
            0 => {
                self.alpha = (255 - 255 * t / dur).max(0) as f32;
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

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let colour = match self.kind {
            0 | 2 | 4 | 5 | 6 => self.p[1] as u32,
            3 => self.p[2] as u32,
            _ => return Ret::Next,
        };
        let rgba = [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8, self.alpha.clamp(0.0, 255.0) as u8];
        let full = matches!(self.kind, 0 | 4 | 5);
        let r = if full { FULLSCREEN } else { PLAYFIELD };
        ctx.gfx.set_viewport(r);
        ctx.gfx.rect(r.x, r.y, r.w, r.h, rgba);
        Ret::Next
    }
}

// Player death ring (type 59, slot 11, colour 0xFFF0404F). SPEC-GAP: its radius is not given; it grows 16 → 192 over 60 frames.
pub fn death_ring(rng: &mut crate::rng::Rng, pos: [f32; 3]) {
    with(|e| {
        let i = e.spawn_fixed_rng(rng, 59, pos, FIX_DEATH, 0xFFF0404F);
        e.fx[i].death_ring = true;
    });
}

// Invulnerability ring at bomb start (type 23, slot 0): I0 := invulnerability frames.
pub fn invuln_ring(anm: &mut AnmManager, pos: [f32; 3], frames: i32) {
    let i = spawn_fixed(anm, 23, pos, FIX_INVULN, 0xFFFF4040);
    get(i, |e| e.vm.ivars[0] = frames);
}
