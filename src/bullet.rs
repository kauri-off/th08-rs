use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::game::Ctx;
use crate::gfx::{Gfx, PLAYFIELD};
use crate::item::ItemManager;
use crate::player::{GrazeEnv, Player, PlayerRef};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;
use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

pub const ANM_SLOT: usize = 6;
pub const CALC_PRIO: u16 = 14;
pub const DRAW_PRIO: u16 = 13;
pub const MAX_BULLETS: usize = 1536;
pub const MAX_LASERS: usize = 256;
pub const TYPES: usize = 21;
const MAIN_SCRIPTS: [usize; TYPES] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 25, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
];
const COLOR_MID: [i32; 8] = [0, 1, 1, 2, 2, 3, 4, 0];
const COLOR_SMALL: [i32; 16] = [0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 0];
const CANCEL_ITEM: i32 = 6;
pub const F_NO_CANCEL: u32 = 0x1000;
const REDIRECT_OR_BOUNCE: u32 = 0x40 | 0x80 | 0x100 | 0x400 | 0x800;

// Spawn-effect (VM1..VM3) and cancel (VM4) scripts per type (08 §2.1).
fn effect_scripts(t: usize) -> [usize; 4] {
    match t {
        0 => [18, 19, 20, 15],
        7..=9 | 14 | 15 | 18..=20 => [24, 24, 24, 17],
        10 => [27, 27, 27, 26],
        _ => [21, 22, 23, 16],
    }
}

// Hitbox and size class from the main sprite height and script (08 §2.1).
pub fn hitbox_class(h: f32, s: usize) -> (f32, u8) {
    if h > 32.0 {
        (24.0, 0)
    } else if h > 16.0 {
        match s {
            8 | 113 | 114 | 115 => (5.0, 2),
            9 | 109 | 110 => (8.0, 1),
            _ => (10.0, 1),
        }
    } else if h > 8.0 {
        match s {
            2 | 4 | 5 | 6 | 106 | 107 | 108 => (4.0, 4),
            _ => (6.0, 3),
        }
    } else {
        (4.0, 5)
    }
}

// Reduced colour index for the cancel/effect VMs (08 §3.2).
fn effect_color(h: u8, colour: i16) -> i32 {
    let c = colour as i32;
    if h > 32 {
        c
    } else if h > 16 {
        COLOR_MID.get(c as usize).copied().unwrap_or(0)
    } else {
        COLOR_SMALL.get(c as usize).copied().unwrap_or(0)
    }
}

fn polar(a: f32, r: f32) -> [f32; 3] {
    [a.cos() * r, a.sin() * r, 0.0]
}

pub fn angle_from(o: [f32; 3], p: [f32; 3]) -> f32 {
    let (dx, dy) = (p[0] - o[0], p[1] - o[1]);
    if dx == 0.0 && dy == 0.0 {
        PI / 2.0
    } else {
        dy.atan2(dx)
    }
}

// One extra-command slot (08 §5.1), filled by ECL op 111.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExCmd {
    pub e: f32,
    pub f: f32,
    pub c: i32,
    pub d: i32,
    pub cmd: u32,
    pub chain: i32,
}

// ECL bullet descriptor (enemy +0x2E24) with rank scaling already applied.
#[derive(Clone, Debug, PartialEq)]
pub struct BulletDesc {
    pub kind: i16,
    pub colour: i16,
    pub origin: [f32; 3],
    pub angle1: f32,
    pub angle2: f32,
    pub speed1: f32,
    pub speed2: f32,
    pub ex: [ExCmd; 18],
    pub count1: i16,
    pub count2: i16,
    pub aim_mode: i16,
    pub flags: u32,
    pub sfx: i32,
    pub ex_sfx: i32,
    pub ex_start: i32,
}

impl Default for BulletDesc {
    fn default() -> Self {
        Self {
            kind: 0,
            colour: 0,
            origin: [0.0; 3],
            angle1: 0.0,
            angle2: 0.0,
            speed1: 0.0,
            speed2: 0.0,
            ex: [ExCmd::default(); 18],
            count1: 0,
            count2: 0,
            aim_mode: 0,
            flags: 0,
            sfx: 7,
            ex_sfx: 25,
            ex_start: 0,
        }
    }
}

// ECL laser descriptor (enemy +0x3070, 07 §9.4); `fixed` = op 114 (desc +0x1F8 = 1, no aiming).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaserDesc {
    pub kind: i16,
    pub colour: i16,
    pub origin: [f32; 3],
    pub angle: f32,
    pub speed: f32,
    pub tail: f32,
    pub head: f32,
    pub max_len: f32,
    pub width: f32,
    pub startup: i32,
    pub duration: i32,
    pub fade: i32,
    pub hit_start: i32,
    pub fade_hit: i32,
    pub flags: u32,
    pub fixed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Template {
    pub vms: [AnmVm; 5],
    pub hitbox: f32,
    pub class: u8,
    pub height: u8,
}

// Extra-command state blocks (08 §5.2).
#[derive(Clone, Copy, Debug, Default)]
pub struct ExState {
    pub speedup: Timer,
    pub accel_timer: Timer,
    pub accel: f32,
    pub accel_angle: f32,
    pub avec: [f32; 3],
    pub accel_dur: i32,
    pub ang_timer: Timer,
    pub dspeed: f32,
    pub dangle: f32,
    pub ang_dur: i32,
    pub redir_timer: Timer,
    pub new_speed: f32,
    pub redir_a: f32,
    pub period: i32,
    pub count: i32,
    pub redirs: i32,
    pub bspeed: f32,
    pub bounces: i32,
    pub max_bounces: i32,
    pub wait: Timer,
    pub wrap: Timer,
}

#[derive(Clone, Debug, Default)]
pub struct Bullet {
    pub vms: [AnmVm; 5],
    pub hitbox: f32,
    pub class: u8,
    pub height: u8,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub speed: f32,
    pub angle: f32,
    pub state_timer: Timer,
    pub age: Timer,
    pub grace: i32,
    pub active: u32,
    pub flags: u32,
    pub colour: i16,
    pub state: i16,
    pub off_count: i16,
    pub grazed: bool,
    pub cancel_after_spawn: bool,
    pub ex_sfx: i32,
    pub ex_index: i32,
    pub ex: [ExCmd; 18],
    pub st: ExState,
    pub intangible: bool,
    effect: usize,
}

impl Bullet {
    pub fn free(&self) -> bool {
        self.state == 0
    }

    // GameMgr_IsOnScreen with the current main sprite's size (08 §4.3).
    fn on_screen(&self) -> bool {
        let (hw, hh) = (self.vms[0].sprite_w / 2.0, self.vms[0].sprite_h / 2.0);
        let [x, y, _] = self.pos;
        x + hw >= 0.0 && x - hw <= 384.0 && y + hh >= 0.0 && y - hh <= 448.0
    }

    fn size(&self) -> [f32; 2] {
        [self.hitbox; 2]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Laser {
    pub body: AnmVm,
    pub emitter: AnmVm,
    pub origin: [f32; 3],
    pub angle: f32,
    pub tail: f32,
    pub head: f32,
    pub max_len: f32,
    pub width: f32,
    pub cur_width: f32,
    pub speed: f32,
    pub startup: i32,
    pub hit_start: i32,
    pub duration: i32,
    pub fade: i32,
    pub fade_hit: i32,
    pub alive: bool,
    pub timer: Timer,
    pub flags: u16,
    pub colour: i16,
    pub state: u8,
    pub hide_emitter: u8,
}

impl Laser {
    // Rotated-rectangle collision (08 §6.3 step 2 with the per-state size).
    fn test(
        &self,
        p: &mut Player,
        size: [f32; 2],
        graze: bool,
        env: &GrazeEnv,
        rng: &mut crate::rng::Rng,
    ) -> u8 {
        let len = self.head - self.tail;
        let centre = [self.origin[0] + self.tail + len / 2.0, self.origin[1]];
        let (dx, dy) = (p.pos[0] - self.origin[0], p.pos[1] - self.origin[1]);
        let (s, c) = (-self.angle).sin_cos();
        let q = [
            dx * c - dy * s + self.origin[0],
            dx * s + dy * c + self.origin[1],
        ];
        let half = [size[0] / 2.0, size[1] / 2.0];
        let ph = p.hit_half();
        let over = |h: [f32; 2]| {
            (q[0] - centre[0]).abs() <= h[0] + ph[0] && (q[1] - centre[1]).abs() <= h[1] + ph[1]
        };
        if over(half) {
            if p.state == 0 {
                p.die(rng);
            }
            return 0;
        }
        if graze && over([half[0] + 48.0, half[1] + 48.0]) && !matches!(p.state, 1 | 2) {
            return p.on_graze(p.pos, true, env, rng);
        }
        0
    }
}

pub struct BulletManager {
    pub templates: Vec<Template>,
    pub bullets: Vec<Bullet>,
    pub lasers: Vec<Laser>,
    pub items: ItemManager,
    pub player: PlayerRef,
    pub live: usize,
    pub cancel_guard: i32,
    pub timer: Timer,
    pub frame: i32,
    lists: [Vec<u16>; 6],
    cursor: usize,
    pub env: GrazeEnv,
    pub freeze: bool,
    pub frozen: bool,
    pub patterns: u64,
}

impl BulletManager {
    // BulletManager_Load: one template per type with its 5 VMs (etama.anm must be in ANM_SLOT).
    pub fn new(anm: &mut AnmManager, player: PlayerRef) -> Self {
        let templates = (0..TYPES)
            .map(|t| {
                let s = MAIN_SCRIPTS[t];
                let e = effect_scripts(t);
                let vms = [s, e[0], e[1], e[2], e[3]].map(|sc| {
                    let mut vm = AnmVm::default();
                    anm.set_script(&mut vm, ANM_SLOT, sc);
                    vm.flags |= anmvm::F_ZWRITE_OFF;
                    vm
                });
                let h = vms[0].sprite_h;
                let (hitbox, class) = hitbox_class(h, s);
                Template {
                    vms,
                    hitbox,
                    class,
                    height: h as u8,
                }
            })
            .collect();
        let mut bullets = vec![Bullet::default(); MAX_BULLETS + 1];
        bullets[MAX_BULLETS].state = 6;
        Self {
            templates,
            bullets,
            lasers: vec![Laser::default(); MAX_LASERS],
            items: ItemManager::default(),
            player,
            live: 0,
            cancel_guard: 0,
            timer: Timer::default(),
            frame: 0,
            lists: Default::default(),
            cursor: 0,
            env: GrazeEnv::default(),
            freeze: false,
            frozen: false,
            patterns: 0,
        }
    }

    pub fn register(ctx: &mut Ctx, player: PlayerRef) -> Result<Rc<RefCell<BulletManager>>> {
        ctx.anm
            .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "etama.anm")?;
        let b = Rc::new(RefCell::new(BulletManager::new(
            &mut ctx.anm,
            player.clone(),
        )));
        player.borrow_mut().world = Rc::downgrade(&b);
        let t: task::TaskRef<Ctx> = b.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(b)
    }

    pub fn live_bullets(&self) -> impl Iterator<Item = &Bullet> {
        self.bullets[..MAX_BULLETS].iter().filter(|b| !b.free())
    }

    // BulletMgr_SpawnPattern (08 §3.1).
    pub fn spawn_pattern(&mut self, anm: &mut AnmManager, d: &BulletDesc, ppos: [f32; 3]) {
        self.patterns += 1;
        if self.live >= MAX_BULLETS {
            return;
        }
        let aim = angle_from(d.origin, ppos);
        for j in 0..d.count2.max(0) as i32 {
            for i in 0..d.count1.max(0) as i32 {
                if !self.place(anm, d, i, j, aim, ppos) {
                    return;
                }
            }
        }
        if d.flags & 0x200 != 0 {
            crate::sound::play_at(d.sfx, d.origin[0]);
        }
    }

    // Speed and angle of bullet (i, j) (07 §9.3).
    fn pattern_angle(anm: &mut AnmManager, d: &BulletDesc, i: i32, j: i32, aim: f32) -> (f32, f32) {
        let (c1, c2) = (d.count1 as i32, d.count2 as i32);
        let (a1, a2) = (d.angle1, d.angle2);
        let tau = 2.0 * PI;
        let mut speed = if c2 < 2 {
            d.speed1
        } else {
            d.speed1 - (d.speed1 - d.speed2) * j as f32 / c2 as f32
        };
        let ring = j as f32 * a2 + a1 + i as f32 * tau / c1 as f32;
        let angle = match d.aim_mode {
            0 | 1 => {
                let mut k = if c1 % 2 == 0 {
                    a2 / 2.0 + (i / 2) as f32 * a2
                } else {
                    ((i + 1) / 2) as f32 * a2
                };
                if i % 2 == 1 {
                    k = -k;
                }
                k + a1 + if d.aim_mode == 0 { aim } else { 0.0 }
            }
            2 => aim + ring,
            3 => ring,
            4 => aim + i as f32 * tau / c1 as f32 + PI / c1 as f32 + a1,
            5 => i as f32 * tau / c1 as f32 + PI / c1 as f32 + a1,
            6 => anm.rng.next_f32() * (a1 - a2) + a2,
            7 => {
                speed = anm.rng.next_f32() * (d.speed1 - d.speed2) + d.speed2;
                ring
            }
            _ => {
                let a = anm.rng.next_f32() * (a1 - a2) + a2;
                speed = anm.rng.next_f32() * (d.speed1 - d.speed2) + d.speed2;
                a
            }
        };
        (speed, angle)
    }

    fn recolor(anm: &AnmManager, vm: &mut AnmVm, base: &AnmVm, k: i32) {
        *vm = base.clone();
        vm.set_sprite(anm, base.sprite_index as i32 + k);
    }

    // BulletMgr_PlaceBullet (08 §3.2).
    fn place(
        &mut self,
        anm: &mut AnmManager,
        d: &BulletDesc,
        i: i32,
        j: i32,
        aim: f32,
        ppos: [f32; 3],
    ) -> bool {
        let mut idx = self.cursor;
        let mut found = None;
        for _ in 0..MAX_BULLETS {
            if idx >= MAX_BULLETS {
                idx = 0;
            }
            if self.bullets[idx].free() {
                found = Some(idx);
                break;
            }
            idx += 1;
        }
        let Some(idx) = found else { return false };
        let (speed, angle) = Self::pattern_angle(anm, d, i, j, aim);
        let gs = anm.speed;
        let t = &self.templates[(d.kind.max(0) as usize).min(TYPES - 1)];
        let b = &mut self.bullets[idx];
        b.state = 1;
        b.grazed = false;
        b.cancel_after_spawn = false;
        b.state_timer = Timer::default();
        b.age = Timer::default();
        b.intangible = false;
        b.speed = speed;
        b.angle = anmvm::wrap_add(angle, 0.0);
        b.pos = [d.origin[0], d.origin[1], 0.1];
        b.vel = polar(b.angle, speed * gs);
        b.vms[0] = t.vms[0].clone();
        b.hitbox = t.hitbox;
        b.class = t.class;
        b.height = t.height;
        b.ex_sfx = d.ex_sfx;
        b.grace = 0;
        b.colour = d.colour;
        if b.vms[0].sprite_index != t.vms[0].sprite_index + d.colour {
            b.vms[0].set_sprite(anm, t.vms[0].sprite_index as i32 + d.colour as i32);
        }
        let k = effect_color(t.height, d.colour);
        Self::recolor(anm, &mut b.vms[4], &t.vms[4], k);
        let eff = if d.flags & 2 != 0 {
            Some((2, 1))
        } else if d.flags & 4 != 0 {
            Some((3, 2))
        } else if d.flags & 8 != 0 {
            Some((4, 3))
        } else {
            None
        };
        if let Some((state, v)) = eff {
            b.state = state;
            b.effect = v;
            Self::recolor(anm, &mut b.vms[v], &t.vms[v], k);
            for a in 0..3 {
                b.pos[a] -= 4.0 * b.vel[a];
            }
        }
        b.ex = d.ex;
        b.flags = d.flags;
        b.active = 0;
        b.ex_index = d.ex_start;
        b.st = ExState::default();
        self.apply_ex(anm, idx, ppos);
        let b = &mut self.bullets[idx];
        if self.cancel_guard > 0 && b.flags & F_NO_CANCEL == 0 {
            b.state = 5;
        }
        self.cursor = if idx + 1 >= MAX_BULLETS { 0 } else { idx + 1 };
        true
    }

    // Bullet_ApplyExCmds (08 §5.1).
    fn apply_ex(&mut self, anm: &mut AnmManager, idx: usize, ppos: [f32; 3]) {
        let gs = anm.speed;
        loop {
            let b = &mut self.bullets[idx];
            let k = b.ex_index;
            if !(0..=17).contains(&k) {
                return;
            }
            let s = b.ex[k as usize];
            if s.cmd == 0 || (s.chain == 0 && b.active != 0) {
                return;
            }
            if b.flags & s.cmd == 0 {
                b.ex_index += 1;
                continue;
            }
            b.ex_index += 1;
            let cont = match s.cmd {
                0x1 => {
                    b.active |= 1;
                    b.st.speedup = Timer::default();
                    false
                }
                0x10 => {
                    b.active |= 0x10;
                    b.st.accel = s.e;
                    b.st.accel_angle = if s.f > -990.0 { s.f } else { b.angle };
                    b.st.accel_dur = s.c;
                    b.st.avec = polar(b.st.accel_angle, s.e * gs);
                    b.st.accel_timer = Timer::default();
                    if k != 0 {
                        ex_sound(b.ex_sfx);
                    }
                    false
                }
                0x20 => {
                    b.active |= 0x20;
                    b.st.dspeed = s.e;
                    b.st.dangle = s.f;
                    b.st.ang_dur = s.c;
                    b.st.ang_timer = Timer::default();
                    if k != 0 {
                        ex_sound(b.ex_sfx);
                    }
                    false
                }
                0x40 | 0x80 | 0x100 => {
                    b.active |= s.cmd;
                    b.st.redir_a = s.e;
                    b.st.new_speed = if s.f > -999.0 { s.f } else { b.speed };
                    b.st.period = s.c;
                    b.st.count = s.d;
                    b.st.redirs = 0;
                    b.st.redir_timer = Timer::default();
                    false
                }
                0x400 | 0x800 => {
                    b.active |= s.cmd;
                    b.st.bspeed = if s.e >= 0.0 { s.e } else { b.speed };
                    b.st.max_bounces = s.c;
                    b.st.bounces = 0;
                    false
                }
                0x2000 => {
                    b.grace = s.c;
                    true
                }
                0x4000 => {
                    let t = &self.templates[(s.c.max(0) as usize).min(TYPES - 1)];
                    b.vms = t.vms.clone();
                    b.hitbox = t.hitbox;
                    b.class = t.class;
                    b.height = t.height;
                    let sp = b.vms[0].sprite_index as i32 + s.d;
                    b.vms[0].set_sprite(anm, sp);
                    true
                }
                0x20000 => {
                    b.active |= s.cmd;
                    b.st.wait.set(s.c);
                    false
                }
                0x40000 => {
                    b.state = 5;
                    false
                }
                0x80000 => {
                    crate::sound::play_at(s.c, b.pos[0]);
                    true
                }
                0x400000 | 0x800000 => {
                    b.active |= s.cmd;
                    b.st.wrap.set(s.c);
                    false
                }
                0x1000000 => {
                    let nb = b.ex.get(k as usize + 1).copied().unwrap_or_default();
                    let mut d = BulletDesc {
                        kind: ((s.c >> 16) & 0xFF) as i16,
                        colour: ((s.c >> 8) & 0xFF) as i16,
                        aim_mode: ((s.c >> 24) & 0x7F) as i16,
                        ex_start: s.c & 0xFF,
                        origin: b.pos,
                        speed1: s.e,
                        speed2: s.f,
                        count1: s.d as i16,
                        angle1: nb.e,
                        angle2: nb.f,
                        count2: nb.c as i16,
                        flags: nb.d as u32,
                        ex: b.ex,
                        sfx: 0,
                        ex_sfx: -1,
                    };
                    d.origin[2] = 0.0;
                    b.ex_index += 1;
                    let kill = s.c < 0;
                    self.spawn_pattern(anm, &d, ppos);
                    if kill {
                        self.bullets[idx].state = 5;
                        return;
                    }
                    true
                }
                _ => false,
            };
            if !cont {
                return;
            }
        }
    }

    // Motion handlers for every running extra command (08 §5.3), in the original order.
    fn motion(b: &mut Bullet, gs: f32, ppos: [f32; 3]) {
        let st = &mut b.st;
        if b.active & 0x1 != 0 {
            if st.speedup.cur <= 16 {
                b.vel = polar(
                    b.angle,
                    (5.0 - 5.0 * st.speedup.getf() / 16.0 + b.speed) * gs,
                );
            } else {
                b.active &= !0x1;
            }
            st.speedup.tick(gs);
        }
        if b.active & 0x10 != 0 {
            if st.accel_timer.cur >= st.accel_dur {
                b.active &= !0x10;
            } else {
                for a in 0..3 {
                    b.vel[a] += st.avec[a] * gs;
                }
                if b.vel[0].abs() > 0.0001 || b.vel[1].abs() > 0.0001 {
                    b.angle = b.vel[1].atan2(b.vel[0]);
                }
            }
            st.accel_timer.tick(gs);
        }
        if b.active & 0x20 != 0 {
            if st.ang_timer.cur >= st.ang_dur {
                b.active &= !0x20;
            } else {
                b.angle = anmvm::wrap_add(b.angle, gs * st.dangle);
                b.speed += gs * st.dspeed;
                b.vel = polar(b.angle, gs * b.speed);
            }
            st.ang_timer.tick(gs);
        }
        for bit in [0x40, 0x100, 0x80] {
            if b.active & bit == 0 {
                continue;
            }
            let t = st.redir_timer.getf();
            let s = if st.redir_timer.cur < st.period {
                b.speed - t * b.speed / st.period as f32
            } else {
                ex_sound(b.ex_sfx);
                st.redirs += 1;
                if st.redirs >= st.count {
                    b.active &= !bit;
                }
                b.angle = match bit {
                    0x40 => b.angle + st.redir_a,
                    0x100 => st.redir_a,
                    _ => anmvm::wrap_add(angle_from(b.pos, ppos) + st.redir_a, 0.0),
                };
                b.speed = st.new_speed;
                st.redir_timer = Timer::default();
                b.speed
            };
            b.vel = polar(b.angle, s * gs);
            st.redir_timer.tick(gs);
        }
        if b.active & 0xC00 != 0 && !b.on_screen() {
            ex_sound(b.ex_sfx);
            let [x, y, _] = b.pos;
            if x < 0.0 || x >= 384.0 {
                b.angle = anmvm::wrap_add(-b.angle - PI, 0.0);
            }
            if y < 0.0 || (y >= 448.0 && b.active & 0x400 != 0) {
                b.angle = -b.angle;
            }
            let st = &mut b.st;
            b.speed = st.bspeed;
            b.vel = polar(b.angle, b.speed * gs);
            st.bounces += 1;
            if st.bounces >= st.max_bounces {
                b.active &= !0xC00;
            }
        }
        let st = &mut b.st;
        for (bit, axis, len) in [(0x400000, 0, 384.0), (0x800000, 1, 448.0)] {
            if b.active & bit == 0 {
                continue;
            }
            if b.pos[axis] < 0.0 {
                b.pos[axis] += len;
            } else if b.pos[axis] > len {
                b.pos[axis] -= len;
            }
            if st.wrap.cur <= 0 {
                b.active &= !bit;
            } else {
                st.wrap.dec(gs);
            }
        }
        if b.active & 0x20000 != 0 {
            if st.wait.cur <= 0 {
                b.active &= !0x20000;
            } else {
                st.wait.dec(gs);
            }
        }
    }

    // Items dropped by a bullet cancelled inside a bomb area (08 §4.1).
    fn bomb_drop(items: &mut ItemManager, anm: &mut AnmManager, p: &Player, pos: [f32; 3]) {
        match p.cancel_item {
            9 => {
                items.spawn(anm, p, pos, 7, 1);
                items.spawn(anm, p, pos, 7, 1);
            }
            k if k >= 0 => {
                items.spawn(anm, p, pos, k, 1);
            }
            _ => {}
        }
    }

    fn push(&mut self, idx: usize) {
        let c = (self.bullets[idx].class as usize).min(5);
        self.lists[c].push(idx as u16);
    }

    // State 1 (08 §4.1); returns false when the bullet was freed.
    fn update_normal(&mut self, anm: &mut AnmManager, idx: usize, p: &mut Player) -> bool {
        let gs = anm.speed;
        self.apply_ex(anm, idx, p.pos);
        let freeze = self.freeze;
        let b = &mut self.bullets[idx];
        if b.active != 0 {
            Self::motion(b, gs, p.pos);
        }
        if b.grace > 0 {
            b.grace -= 1;
        }
        if !freeze {
            for a in 0..3 {
                b.pos[a] += b.vel[a];
            }
        }
        if b.grace == 0 {
            if b.on_screen() {
                b.off_count = 0;
            } else if b.active & REDIRECT_OR_BOUNCE == 0 {
                if b.off_count > 0 {
                    b.off_count -= 1;
                } else {
                    b.state = 0;
                    return false;
                }
            } else {
                b.off_count += 1;
                if b.off_count >= 128 {
                    b.state = 0;
                    return false;
                }
            }
        }
        if !b.intangible {
            let (pos, size) = (b.pos, b.size());
            let mut hit_test = true;
            if !b.grazed && b.age.cur > 15 {
                let (g, orbs) = p.graze_test(pos, size, &self.env, &mut anm.rng);
                match g {
                    1 => {
                        self.bullets[idx].grazed = true;
                        for _ in 0..orbs {
                            self.items.spawn(anm, p, pos, 10, 0);
                        }
                    }
                    2 => {
                        hit_test = false;
                        if self.bullets[idx].flags & F_NO_CANCEL == 0 {
                            self.bullets[idx].state = 5;
                            Self::bomb_drop(&mut self.items, anm, p, pos);
                        }
                    }
                    _ => hit_test = false,
                }
            }
            if hit_test {
                match p.hit_test(pos, size, &mut anm.rng) {
                    1 => self.bullets[idx].state = 5,
                    2 if self.bullets[idx].flags & F_NO_CANCEL == 0 => {
                        self.bullets[idx].state = 5;
                        Self::bomb_drop(&mut self.items, anm, p, pos);
                    }
                    _ => {}
                }
            }
        }
        let b = &mut self.bullets[idx];
        if b.vms[0].script.is_some() {
            b.vms[0].run(anm);
        }
        b.state_timer.tick(gs);
        b.age.tick(gs);
        self.push(idx);
        true
    }

    fn update_bullet(&mut self, anm: &mut AnmManager, idx: usize, p: &mut Player) {
        let gs = anm.speed;
        match self.bullets[idx].state {
            1 => {
                self.update_normal(anm, idx, p);
            }
            2..=4 => {
                let b = &mut self.bullets[idx];
                b.age.dec(gs);
                let div = [2.0, 2.5, 3.0][(b.state - 2) as usize];
                for a in 0..3 {
                    b.pos[a] += b.vel[a] / div;
                }
                if b.flags & F_NO_CANCEL == 0 && p.in_bomb_area(b.pos) {
                    b.cancel_after_spawn = true;
                }
                let e = b.effect;
                if b.vms[e].run(anm) {
                    if b.cancel_after_spawn {
                        let pos = b.pos;
                        Self::bomb_drop(&mut self.items, anm, p, pos);
                    }
                    let b = &mut self.bullets[idx];
                    b.state = 1;
                    b.state_timer = Timer::default();
                    self.update_normal(anm, idx, p);
                } else {
                    b.state_timer.tick(gs);
                    b.age.tick(gs);
                    self.push(idx);
                }
            }
            5 => {
                let b = &mut self.bullets[idx];
                for a in 0..3 {
                    b.pos[a] += b.vel[a] / 2.0;
                }
                if b.vms[4].run(anm) {
                    b.state = 0;
                } else {
                    self.push(idx);
                }
            }
            _ => {}
        }
    }

    // Laser spawn (08 §6.2).
    pub fn spawn_laser(
        &mut self,
        anm: &mut AnmManager,
        d: &LaserDesc,
        ppos: [f32; 3],
    ) -> Option<usize> {
        if self.cancel_guard != 0 && d.flags & 4 == 0 {
            return None;
        }
        let slot = self.lasers.iter().position(|l| !l.alive)?;
        let mut l = Laser {
            origin: d.origin,
            angle: if d.fixed {
                d.angle
            } else {
                d.angle + angle_from(d.origin, ppos)
            },
            speed: d.speed,
            tail: d.tail,
            head: d.head,
            max_len: d.max_len,
            width: d.width,
            startup: d.startup,
            duration: d.duration,
            fade: d.fade,
            hit_start: d.hit_start,
            fade_hit: d.fade_hit,
            flags: d.flags as u16,
            colour: d.colour,
            state: if d.startup == 0 { 1 } else { 0 },
            alive: true,
            ..Laser::default()
        };
        anm.set_script(&mut l.body, ANM_SLOT, (d.kind as i32 + 10).max(0) as usize);
        let sp = l.body.sprite_index as i32 + d.colour as i32;
        l.body.set_sprite(anm, sp);
        anm.set_script(
            &mut l.emitter,
            ANM_SLOT,
            0x92 + COLOR_SMALL.get(d.colour as usize).copied().unwrap_or(0) as usize,
        );
        l.emitter.flags =
            (l.emitter.flags & !(3 << anmvm::F_BLEND_SHIFT)) | (1 << anmvm::F_BLEND_SHIFT);
        if l.state == 1 {
            l.cur_width = l.width;
        }
        self.lasers[slot] = l;
        Some(slot)
    }

    pub fn laser_mut(&mut self, id: usize) -> Option<&mut Laser> {
        self.lasers.get_mut(id).filter(|l| l.alive)
    }

    // Per-frame laser update (08 §6.3).
    fn update_laser(&mut self, anm: &mut AnmManager, k: usize, p: &mut Player) {
        let gs = anm.speed;
        let env = self.env;
        let mut orbs = 0;
        let l = &mut self.lasers[k];
        l.head += gs * l.speed;
        if l.head - l.tail > l.max_len {
            l.tail = l.head - l.max_len;
        }
        if l.tail < 0.0 {
            l.tail = 0.0;
        }
        let len = l.head - l.tail;
        let mut size = [if l.tail > 0.0 { len * 0.7 } else { len }, l.width / 2.0];
        if l.body.sprite_w != 0.0 && l.body.sprite_h != 0.0 {
            l.body.scale = [l.width / l.body.sprite_w, len / l.body.sprite_h];
        }
        l.body.rotation[2] = anmvm::wrap_add(l.angle, PI / 2.0);
        l.body.flags |= anmvm::F_ROT_DIRTY | anmvm::F_SCALE_DIRTY;
        if l.state == 0 {
            let t = l.timer.cur;
            if l.flags & 1 != 0 {
                l.body.color[anmvm::A] = if l.startup > 0 {
                    (t * 255 / l.startup).min(255) as u8
                } else {
                    255
                };
            } else {
                l.cur_width = if t > l.startup - l.startup.min(30) {
                    t as f32 * l.width / l.startup as f32
                } else {
                    1.2
                };
                l.body.scale[0] = l.cur_width / 16.0;
                size[0] = l.cur_width / 2.0;
            }
            if t >= l.hit_start {
                orbs += l.test(p, size, false, &env, &mut anm.rng);
            }
            if t >= l.startup {
                l.timer = Timer::default();
                l.state = 1;
                l.cur_width = l.width;
            }
        }
        if l.state == 1 {
            let t = l.timer.cur;
            orbs += l.test(p, size, t % 20 == 0, &env, &mut anm.rng);
            if t >= l.duration {
                l.timer = Timer::default();
                l.state = 2;
                if l.fade == 0 {
                    l.alive = false;
                }
            }
        }
        if l.alive && l.state == 2 {
            let t = l.timer.cur;
            if l.flags & 1 != 0 {
                l.body.color[anmvm::A] = if l.startup > 0 {
                    (t * 255 / l.startup).min(255) as u8
                } else {
                    255
                };
            } else if l.fade > 0 {
                l.cur_width = l.width - t as f32 * l.width / l.fade as f32;
                l.body.scale[0] = l.cur_width / 16.0;
                size[0] = l.cur_width / 2.0;
            }
            if t < l.fade_hit {
                orbs += l.test(p, size, false, &env, &mut anm.rng);
            }
            if t >= l.fade {
                l.alive = false;
            }
        }
        if l.alive {
            if l.tail >= 640.0 {
                l.alive = false;
            } else {
                l.timer.tick(gs);
                l.body.run(anm);
            }
        }
        for _ in 0..orbs {
            self.items.spawn(anm, p, p.pos, 10, 0);
        }
    }

    // Laser part of the cancels (08 §7); `items` false skips the item line.
    fn cancel_lasers(
        &mut self,
        anm: &mut AnmManager,
        p: &Player,
        mode: i32,
        items: bool,
        at_origin: bool,
    ) {
        for k in 0..MAX_LASERS {
            let l = &mut self.lasers[k];
            if !l.alive || (l.flags & 4 != 0 && mode != 4) {
                continue;
            }
            if l.state < 2 {
                l.state = 2;
                l.timer = Timer::default();
                l.cur_width = l.width;
                if items {
                    let (o, a, tail, head) = (l.origin, l.angle, l.tail, l.head);
                    let (s, c) = a.sin_cos();
                    if at_origin {
                        self.items.spawn(anm, p, o, CANCEL_ITEM, 1);
                    }
                    let mut d = tail;
                    while d < head {
                        self.items
                            .spawn(anm, p, [o[0] + c * d, o[1] + s * d, 0.0], CANCEL_ITEM, 1);
                        d += 32.0;
                    }
                }
            }
            self.lasers[k].fade_hit = 0;
        }
    }

    // BulletMgr_CancelAll (08 §7).
    pub fn cancel_all(&mut self, anm: &mut AnmManager, p: &mut Player, mode: i32) {
        for i in 0..MAX_BULLETS {
            let b = &mut self.bullets[i];
            if b.free() || b.state == 5 {
                continue;
            }
            let pos = b.pos;
            if p.in_bomb_area(pos) {
                b.state = 0;
                Self::bomb_drop(&mut self.items, anm, p, pos);
            } else if mode == 4 {
                b.state = 5;
            } else {
                b.state = 0;
                self.items.spawn(anm, p, pos, CANCEL_ITEM, mode as u8);
            }
        }
        self.cancel_lasers(anm, p, mode, mode != 4, false);
        self.cancel_guard = 10;
    }

    // BulletMgr_CancelBonus (08 §7): returns the sum of the popup values.
    pub fn cancel_bonus(
        &mut self,
        anm: &mut AnmManager,
        p: &mut Player,
        max_value: i32,
        lasers_too: bool,
    ) -> i32 {
        let (mut v, mut sum) = (2000, 0);
        for i in 0..MAX_BULLETS {
            let b = &mut self.bullets[i];
            if b.free() {
                continue;
            }
            b.state = 5;
            let pos = b.pos;
            if p.in_bomb_area(pos) {
                Self::bomb_drop(&mut self.items, anm, p, pos);
            } else {
                self.items.spawn(anm, p, pos, CANCEL_ITEM, 1);
            }
            crate::ascii::popup(
                pos,
                v,
                if v >= max_value {
                    crate::item::POPUP_MAX
                } else {
                    crate::item::POPUP_NORMAL
                },
            );
            sum += v;
            v = (v + 20).min(max_value);
        }
        self.cancel_lasers(anm, p, 1, lasers_too, true);
        sum
    }

    // BulletMgr_CancelRadius (08 §7).
    pub fn cancel_radius(&mut self, anm: &mut AnmManager, p: &Player, c: [f32; 3], r: f32) {
        for i in 0..MAX_BULLETS {
            let b = &mut self.bullets[i];
            if b.free() || b.state == 5 {
                continue;
            }
            let (dx, dy) = (b.pos[0] - c[0], b.pos[1] - c[1]);
            if dx * dx + dy * dy <= r * r {
                b.state = 0;
                let pos = b.pos;
                self.items.spawn(anm, p, pos, CANCEL_ITEM, 1);
            }
        }
    }

    // ECL natives 12/14 (08 §7): toggle the "ghost" state of bullets whose flags match `mask`.
    // ECL natives 12 / 14 (08 §7): ghost toggle by VM0 tag; native 14 cycles 1 → 0 → 2 (fade-in) → 1.
    pub fn toggle_ghost(
        &mut self,
        anm: &mut AnmManager,
        mask: u32,
        angle: f32,
        speed: f32,
        native14: bool,
    ) {
        let gs = anm.speed;
        let set_blend = |vm: &mut AnmVm, b: u32| {
            vm.flags = (vm.flags & !(3 << anmvm::F_BLEND_SHIFT)) | (b << anmvm::F_BLEND_SHIFT)
        };
        for b in self.bullets[..MAX_BULLETS].iter_mut() {
            if b.free() || b.flags & mask == 0 {
                continue;
            }
            let vm = &mut b.vms[0];
            match vm.tag {
                1 => {
                    let sp = vm.sprite_index as i32 + 16;
                    vm.set_sprite(anm, sp);
                    vm.tag = 0;
                    set_blend(vm, 1);
                    b.intangible = true;
                    if native14 {
                        vm.color[anmvm::A] = 0;
                        b.vel = polar(b.angle, speed * gs);
                    } else {
                        b.vel = polar(angle, speed * gs);
                    }
                }
                0 if native14 => {
                    vm.tag = 2;
                    vm.color[anmvm::A] = 0;
                    vm.interp_alpha(15, 0, 255);
                }
                _ => {
                    let sp = vm.sprite_index as i32 - 16;
                    vm.set_sprite(anm, sp);
                    vm.tag = 1;
                    set_blend(vm, 0);
                    b.intangible = false;
                    b.vel = polar(b.angle, b.speed * gs);
                }
            }
        }
    }

    // BulletMgr_Calc (08 §1).
    pub fn step(&mut self, anm: &mut AnmManager) {
        self.player.borrow_mut().take_queued_areas();
        if self.frozen || self.player.borrow().freeze_bullets {
            return;
        }
        let pl = self.player.clone();
        let mut p = pl.borrow_mut();
        let mut items = std::mem::take(&mut self.items);
        let spell = self.env.spell;
        items.calc(anm, &mut p, spell, |it, anm, p| {
            std::mem::swap(&mut self.items, it);
            self.cancel_all(anm, p, 1);
            std::mem::swap(&mut self.items, it);
        });
        self.items = items;
        self.live = 0;
        self.lists.iter_mut().for_each(Vec::clear);
        for idx in std::iter::once(0).chain((1..MAX_BULLETS).rev()) {
            if self.bullets[idx].free() {
                continue;
            }
            self.live += 1;
            self.update_bullet(anm, idx, &mut p);
        }
        for k in 0..MAX_LASERS {
            if self.lasers[k].alive {
                self.update_laser(anm, k, &mut p);
            }
        }
        if self.cancel_guard > 0 {
            self.cancel_guard -= 1;
        }
        self.timer.tick(anm.speed);
        self.frame += 1;
    }

    fn draw_lasers(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for l in self.lasers.iter_mut().filter(|l| l.alive) {
            let (s, c) = l.angle.sin_cos();
            let len = l.head - l.tail;
            let mid = l.tail + len / 2.0;
            l.body.pos = [
                l.origin[0] + c * mid + 32.0,
                l.origin[1] + s * mid + 16.0,
                0.0,
            ];
            anm.draw_mode_z(gfx, &mut l.body, 2, 0.06);
            if (l.tail < 16.0 || l.speed == 0.0) && (l.hide_emitter == 0 || l.state != 0) {
                let mut sc = (16.0 - l.tail) / 16.0 * l.width / 10.0;
                if sc <= 0.0 {
                    sc = l.width / 10.0;
                }
                l.emitter.scale = [sc, sc];
                l.emitter.color = l.body.color;
                l.emitter.color[anmvm::A] = 255;
                l.emitter.pos = [
                    l.origin[0] + c * l.tail + 32.0,
                    l.origin[1] + s * l.tail + 16.0,
                    0.0,
                ];
                anm.draw_mode_z(gfx, &mut l.emitter, 2, 0.05);
            }
        }
    }

    // BulletMgr_Draw: items, lasers, then bullets by size class (each list in reverse visiting order).
    pub fn draw(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        gfx.set_viewport(PLAYFIELD);
        if self.player.borrow().freeze_bullets {
            (anm.tint, anm.tint_on) = (0xFFF01010, true);
        }
        self.items.draw(anm, gfx);
        self.draw_lasers(anm, gfx);
        for c in 0..6 {
            for k in (0..self.lists[c].len()).rev() {
                let b = &mut self.bullets[self.lists[c][k] as usize];
                let v = match b.state {
                    1 => 0,
                    2..=4 => b.effect,
                    5 => 4,
                    _ => continue,
                };
                let vm = &mut b.vms[v];
                // Bullet_Draw (08 §3): z 0.05, RGB white, rotation only for tagged VMs.
                vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.05];
                if vm.tag != 0 {
                    let r = anmvm::wrap_add(b.angle, PI / 2.0);
                    if vm.rotation[2] != r {
                        vm.rotation[2] = r;
                        vm.flags |= anmvm::F_ROT_DIRTY;
                    }
                }
                vm.color[..3].fill(255);
                anm.draw_mode_z(gfx, vm, 2, 0.05);
            }
        }
        anm.tint_off();
        crate::effect::with(|e| e.draw_over_bullets(anm, gfx));
    }
}

impl Task<Ctx> for BulletManager {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            ctx.anm.unload(&mut ctx.gfx, ANM_SLOT);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.step(&mut ctx.anm);
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        BulletManager::draw(self, &mut ctx.anm, &mut ctx.gfx);
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item;
    use crate::player::Player;

    fn mgr() -> (AnmManager, BulletManager) {
        let mut anm = AnmManager::default();
        let p = Player::new_ref();
        p.borrow_mut().pos = [-1000.0, -1000.0, 0.0];
        let b = BulletManager::new(&mut anm, p);
        (anm, b)
    }

    fn desc(angle: f32, speed: f32, flags: u32, ex: &[ExCmd]) -> BulletDesc {
        let mut d = BulletDesc {
            aim_mode: 3,
            count1: 1,
            count2: 1,
            angle1: angle,
            speed1: speed,
            flags,
            origin: [192.0, 224.0, 0.0],
            ..BulletDesc::default()
        };
        d.ex[..ex.len()].copy_from_slice(ex);
        d
    }

    fn cmd(cmd: u32, e: f32, f: f32, c: i32, d: i32) -> ExCmd {
        ExCmd {
            e,
            f,
            c,
            d,
            cmd,
            chain: 0,
        }
    }

    fn spawn(anm: &mut AnmManager, b: &mut BulletManager, d: &BulletDesc) -> usize {
        let before: Vec<bool> = b.bullets.iter().map(Bullet::free).collect();
        b.spawn_pattern(anm, d, [-1000.0, -1000.0, 0.0]);
        (0..MAX_BULLETS)
            .find(|&i| before[i] && !b.bullets[i].free())
            .expect("bullet placed")
    }

    fn speed(b: &Bullet) -> f32 {
        (b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt()
    }

    #[test]
    fn ring_pattern_and_visit_order() {
        let (mut anm, mut b) = mgr();
        let d = BulletDesc {
            aim_mode: 3,
            count1: 4,
            count2: 1,
            speed1: 2.0,
            origin: [100.0, 100.0, 0.0],
            ..BulletDesc::default()
        };
        b.spawn_pattern(&mut anm, &d, [0.0; 3]);
        let a: Vec<f32> = b.live_bullets().map(|x| x.angle).collect();
        assert_eq!(a.len(), 4);
        for (got, want) in a.iter().zip([0.0, PI / 2.0, PI, -PI / 2.0]) {
            assert!(
                (anmvm::wrap_add(got - want, 0.0)).abs() < 1e-5,
                "{got} vs {want}"
            );
        }
        assert!(
            b.live_bullets()
                .all(|x| (speed(x) - 2.0).abs() < 1e-5 && x.pos[2] == 0.1)
        );
        b.step(&mut anm);
        assert_eq!(b.live, 4);
        assert_eq!(b.lists[5].len(), 4);
    }

    #[test]
    fn speed_up_command() {
        let (mut anm, mut b) = mgr();
        let i = spawn(
            &mut anm,
            &mut b,
            &desc(0.0, 1.0, 1, &[cmd(1, 0.0, 0.0, 0, 0)]),
        );
        assert_eq!(b.bullets[i].active, 1);
        b.step(&mut anm);
        assert!((speed(&b.bullets[i]) - 6.0).abs() < 1e-5);
        for _ in 0..8 {
            b.step(&mut anm);
        }
        assert!((speed(&b.bullets[i]) - (5.0 - 5.0 * 8.0 / 16.0 + 1.0)).abs() < 1e-5);
        for _ in 0..9 {
            b.step(&mut anm);
        }
        assert_eq!(b.bullets[i].active, 0);
        assert!((speed(&b.bullets[i]) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn accel_command() {
        let (mut anm, mut b) = mgr();
        let i = spawn(
            &mut anm,
            &mut b,
            &desc(0.0, 1.0, 0x10, &[cmd(0x10, 0.1, PI / 2.0, 10, 0)]),
        );
        for _ in 0..12 {
            b.step(&mut anm);
        }
        let x = &b.bullets[i];
        assert_eq!(x.active, 0);
        assert!(
            (x.vel[0] - 1.0).abs() < 1e-4 && (x.vel[1] - 1.0).abs() < 1e-4,
            "{:?}",
            x.vel
        );
        assert!((x.angle - PI / 4.0).abs() < 1e-4);
        assert_eq!(x.speed, 1.0);
        let j = spawn(
            &mut anm,
            &mut b,
            &desc(0.5, 1.0, 0x10, &[cmd(0x10, 0.1, -999.0, 1, 0)]),
        );
        assert!((b.bullets[j].st.accel_angle - 0.5).abs() < 1e-6);
    }

    #[test]
    fn angular_command() {
        let (mut anm, mut b) = mgr();
        let i = spawn(
            &mut anm,
            &mut b,
            &desc(0.0, 1.0, 0x20, &[cmd(0x20, 0.5, 0.1, 4, 0)]),
        );
        for _ in 0..6 {
            b.step(&mut anm);
        }
        let x = &b.bullets[i];
        assert!((x.angle - 0.4).abs() < 1e-5 && (x.speed - 3.0).abs() < 1e-5);
        assert!((speed(x) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn redirect_commands() {
        let (mut anm, mut b) = mgr();
        let i = spawn(
            &mut anm,
            &mut b,
            &desc(0.0, 2.0, 0x100, &[cmd(0x100, 1.0, 3.0, 4, 1)]),
        );
        b.step(&mut anm);
        assert!((speed(&b.bullets[i]) - 2.0).abs() < 1e-5);
        b.step(&mut anm);
        assert!((speed(&b.bullets[i]) - 1.5).abs() < 1e-5);
        for _ in 0..3 {
            b.step(&mut anm);
        }
        let x = &b.bullets[i];
        assert_eq!(x.active, 0);
        assert_eq!((x.angle, x.speed), (1.0, 3.0));
        let j = spawn(
            &mut anm,
            &mut b,
            &desc(0.5, 2.0, 0x40, &[cmd(0x40, 1.0, -1000.0, 0, 2)]),
        );
        b.step(&mut anm);
        b.step(&mut anm);
        assert!((b.bullets[j].angle - 2.5).abs() < 1e-5 && b.bullets[j].speed == 2.0);
        assert_eq!(b.bullets[j].active, 0);
    }

    #[test]
    fn bounce_and_wrap() {
        let (mut anm, mut b) = mgr();
        let mut d = desc(PI, 4.0, 0x400, &[cmd(0x400, 1.5, 0.0, 1, 0)]);
        d.origin = [2.0, 200.0, 0.0];
        let i = spawn(&mut anm, &mut b, &d);
        b.step(&mut anm);
        assert!(b.bullets[i].pos[0] < 0.0);
        b.step(&mut anm);
        let x = &b.bullets[i];
        assert!(
            x.angle.abs() < 1e-5 && x.speed == 1.5 && x.active == 0,
            "{} {}",
            x.angle,
            x.speed
        );
        let mut d = desc(
            PI,
            4.0,
            0x2000 | 0x400000,
            &[cmd(0x2000, 0.0, 0.0, 5, 0), cmd(0x400000, 0.0, 0.0, 3, 0)],
        );
        d.origin = [2.0, 200.0, 0.0];
        let j = spawn(&mut anm, &mut b, &d);
        b.step(&mut anm);
        b.step(&mut anm);
        assert!(
            (b.bullets[j].pos[0] - (2.0 - 8.0 + 384.0)).abs() < 1e-4,
            "{}",
            b.bullets[j].pos[0]
        );
    }

    #[test]
    fn chaining_and_flag_gating() {
        let (mut anm, mut b) = mgr();
        let ex = [
            cmd(0x10, 0.0, 0.0, 1, 0),
            cmd(0x20000, 0.0, 0.0, 3, 0),
            cmd(0x100, 2.0, 1.0, 0, 1),
        ];
        let i = spawn(&mut anm, &mut b, &desc(0.0, 1.0, 0x20000 | 0x100, &ex));
        assert_eq!((b.bullets[i].active, b.bullets[i].ex_index), (0x20000, 2));
        for _ in 0..4 {
            b.step(&mut anm);
            assert_eq!(b.bullets[i].active & 0x100, 0);
        }
        b.step(&mut anm);
        assert_eq!(b.bullets[i].ex_index, 3);
        b.step(&mut anm);
        assert_eq!(b.bullets[i].angle, 2.0);
        let ex = [cmd(0x2000, 0.0, 0.0, 7, 0), cmd(0x40000, 0.0, 0.0, 0, 0)];
        let j = spawn(&mut anm, &mut b, &desc(0.0, 1.0, 0x2000 | 0x40000, &ex));
        assert_eq!((b.bullets[j].grace, b.bullets[j].state), (7, 5));
    }

    #[test]
    fn sub_pattern_command() {
        let (mut anm, mut b) = mgr();
        let a = (3 << 24) | (2 << 16) | (5 << 8);
        let ex = [
            cmd(0x1000000, 3.0, 1.0, a | (1 << 31), 4),
            ExCmd {
                e: 0.25,
                f: 0.0,
                c: 1,
                d: 0,
                cmd: 0,
                chain: 0,
            },
        ];
        let i = spawn(&mut anm, &mut b, &desc(0.0, 1.0, 0x1000000, &ex));
        assert_eq!(b.bullets[i].state, 5);
        let subs: Vec<&Bullet> = b.live_bullets().filter(|x| x.state == 1).collect();
        assert_eq!(subs.len(), 4);
        assert!(
            subs.iter()
                .all(|x| x.colour == 5 && x.speed == 3.0 && x.pos[0] == 192.0)
        );
        assert!((subs[1].angle - (0.25 + PI / 2.0)).abs() < 1e-5);
    }

    #[test]
    fn offscreen_rules() {
        let (mut anm, mut b) = mgr();
        let mut d = desc(PI, 5.0, 0, &[]);
        d.origin = [1.0, 200.0, 0.0];
        let i = spawn(&mut anm, &mut b, &d);
        b.step(&mut anm);
        assert!(b.bullets[i].free());
        let mut d = desc(PI, 5.0, 0x100, &[cmd(0x100, 0.0, 0.0, 1000, 1)]);
        d.origin = [1.0, 200.0, 0.0];
        let j = spawn(&mut anm, &mut b, &d);
        for _ in 0..127 {
            b.step(&mut anm);
        }
        assert!(!b.bullets[j].free());
        assert_eq!(b.bullets[j].off_count, 127);
        b.step(&mut anm);
        assert!(b.bullets[j].free());
    }

    #[test]
    fn cancel_to_items() {
        let (mut anm, mut b) = mgr();
        let d = BulletDesc {
            aim_mode: 3,
            count1: 5,
            count2: 1,
            speed1: 1.0,
            origin: [100.0, 100.0, 0.0],
            ..BulletDesc::default()
        };
        b.spawn_pattern(&mut anm, &d, [0.0; 3]);
        let pl = b.player.clone();
        b.cancel_all(&mut anm, &mut pl.borrow_mut(), 1);
        assert_eq!(b.live_bullets().count(), 0);
        let items: Vec<_> = b.items.live().map(|i| (i.kind, i.state)).collect();
        assert_eq!(items, vec![(6, 1); 5]);
        assert_eq!(b.cancel_guard, 10);
        let i = spawn(&mut anm, &mut b, &desc(0.0, 1.0, 0, &[]));
        assert_eq!(b.bullets[i].state, 5);
        let j = spawn(&mut anm, &mut b, &desc(0.0, 1.0, F_NO_CANCEL, &[]));
        assert_eq!(b.bullets[j].state, 1);
        b.cancel_all(&mut anm, &mut pl.borrow_mut(), 4);
        assert_eq!(b.bullets[j].state, 5);
        assert_eq!(b.items.live().count(), 5);

        let (mut anm, mut b) = mgr();
        b.spawn_pattern(
            &mut anm,
            &BulletDesc {
                count1: 3,
                ..d.clone()
            },
            [0.0; 3],
        );
        let pl = b.player.clone();
        let sum = b.cancel_bonus(&mut anm, &mut pl.borrow_mut(), 2030, false);
        assert_eq!(sum, 2000 + 2020 + 2030);
        assert!(b.live_bullets().all(|x| x.state == 5));

        let (mut anm, mut b) = mgr();
        let l = LaserDesc {
            fixed: true,
            head: 100.0,
            max_len: 200.0,
            width: 16.0,
            startup: 0,
            duration: 100,
            fade: 10,
            origin: [50.0, 50.0, 0.0],
            ..LaserDesc::default()
        };
        let k = b.spawn_laser(&mut anm, &l, [0.0; 3]).unwrap();
        b.cancel_all(&mut anm, &mut pl.borrow_mut(), 1);
        assert_eq!(b.lasers[k].state, 2);
        let xs: Vec<f32> = b.items.live().map(|i| i.pos[0]).collect();
        assert_eq!(xs, vec![50.0, 82.0, 114.0, 146.0]);
        assert!(b.spawn_laser(&mut anm, &l, [0.0; 3]).is_none());
    }

    #[test]
    fn laser_lifecycle() {
        let (mut anm, mut b) = mgr();
        let l = LaserDesc {
            fixed: true,
            speed: 4.0,
            head: 0.0,
            max_len: 64.0,
            width: 16.0,
            startup: 10,
            duration: 20,
            fade: 5,
            origin: [50.0, 50.0, 0.0],
            ..LaserDesc::default()
        };
        let k = b.spawn_laser(&mut anm, &l, [0.0; 3]).unwrap();
        assert_eq!(b.lasers[k].state, 0);
        for _ in 0..10 {
            b.step(&mut anm);
        }
        assert_eq!(b.lasers[k].state, 0);
        b.step(&mut anm);
        assert_eq!(b.lasers[k].state, 1);
        assert_eq!((b.lasers[k].tail, b.lasers[k].head), (0.0, 44.0));
        for _ in 0..20 {
            b.step(&mut anm);
        }
        assert_eq!(b.lasers[k].state, 2);
        assert_eq!(b.lasers[k].tail, b.lasers[k].head - 64.0);
        for _ in 0..5 {
            b.step(&mut anm);
        }
        assert!(!b.lasers[k].alive);
    }

    #[test]
    fn items_spawn_and_collect() {
        let (mut anm, mut b) = mgr();
        let pl = b.player.clone();
        pl.borrow_mut().stats.power = 128.0;
        let k = b
            .items
            .spawn(&mut anm, &pl.borrow(), [100.0, 100.0, 0.0], 0, 0)
            .unwrap();
        assert_eq!(b.items.items[k].kind, item::CONVERTED);
        let k = b
            .items
            .spawn(&mut anm, &pl.borrow(), [100.0, 100.0, 0.0], 10, 0)
            .unwrap();
        assert_eq!(
            (b.items.items[k].kind, b.items.items[k].state),
            (item::TIME_ORB, 5)
        );
        assert!(
            b.items
                .spawn(&mut anm, &pl.borrow(), [500.0, 100.0, 0.0], 1, 0)
                .is_none()
        );

        let (mut anm, mut b) = mgr();
        let pl = b.player.clone();
        pl.borrow_mut().pos = [100.0, 300.0, 0.0];
        b.items.spawn(
            &mut anm,
            &pl.borrow(),
            [100.0, 100.0, 0.0],
            item::POINT as i32,
            1,
        );
        for _ in 0..40 {
            b.step(&mut anm);
        }
        let p = pl.borrow();
        assert_eq!(b.items.collected, 1);
        let line = p.sht.poc_line;
        assert!(
            p.stats.score() > 0 && p.stats.point_items == 1,
            "line {line} score {}",
            p.stats.score()
        );
    }
}

// Ex-command "sfx" (08 §5): Sound_Play(ex sfx) when ≥ 0.
fn ex_sound(id: i32) {
    if id >= 0 {
        crate::sound::play(id);
    }
}
