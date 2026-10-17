// TEMPORARY stub BulletManager: holds the fully-parameterised spawn requests produced by ECL (07 §9) and
// draws bullets as plain etama.anm sprites moving in straight lines. Real behaviour comes with the 08 spec.
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::game::Ctx;
use crate::gfx::{Blend, Gfx, PLAYFIELD};
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
const MAIN_SCRIPTS: [usize; 21] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 25, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExCmd {
    pub e: f32,
    pub f: f32,
    pub c: i32,
    pub d: i32,
    pub a: u32,
    pub b: i32,
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
        }
    }
}

// ECL laser descriptor (enemy +0x3070, 07 §9.4); `aimed` = op 115.
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
    pub aimed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemReq {
    pub pos: [f32; 3],
    pub kind: i32,
}

#[derive(Clone, Debug)]
pub struct Bullet {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub angle: f32,
    pub speed: f32,
    pub kind: i16,
    pub colour: i16,
    pub flags: u32,
    vm: AnmVm,
}

#[derive(Clone, Debug)]
pub struct Laser {
    pub desc: LaserDesc,
    pub pos: [f32; 3],
    pub angle: f32,
    pub tail: f32,
    pub head: f32,
    pub state: i32,
    pub timer: Timer,
    pub fields: [f32; 3],
}

#[derive(Default)]
pub struct BulletManager {
    pub requests: Vec<BulletDesc>,
    pub items: Vec<ItemReq>,
    pub bullets: Vec<Bullet>,
    pub lasers: Vec<Option<Laser>>,
    pub patterns: u64,
    pub cancels: u64,
}

pub fn angle_from(o: [f32; 3], p: [f32; 3]) -> f32 {
    let (dx, dy) = (p[0] - o[0], p[1] - o[1]);
    if dx == 0.0 && dy == 0.0 { PI / 2.0 } else { dy.atan2(dx) }
}

impl BulletManager {
    pub fn register(ctx: &mut Ctx) -> Result<Rc<RefCell<BulletManager>>> {
        ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "etama.anm")?;
        let b = Rc::new(RefCell::new(BulletManager { lasers: vec![None; MAX_LASERS], ..Default::default() }));
        let t: task::TaskRef<Ctx> = b.clone();
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_PRIO, 0, t)?;
        Ok(b)
    }

    pub fn spawn_laser(&mut self, d: LaserDesc, player: [f32; 3]) -> Option<usize> {
        let slot = self.lasers.iter().position(Option::is_none)?;
        let angle = if d.aimed { d.angle } else { d.angle + angle_from(d.origin, player) };
        let state = if d.startup == 0 { 1 } else { 0 };
        self.lasers[slot] = Some(Laser { pos: d.origin, angle, tail: d.tail, head: d.head, state, timer: Timer::default(), fields: [0.0; 3], desc: d });
        Some(slot)
    }

    pub fn laser_mut(&mut self, id: usize) -> Option<&mut Laser> {
        self.lasers.get_mut(id)?.as_mut()
    }

    pub fn cancel_all(&mut self) {
        self.bullets.clear();
        self.cancels += 1;
    }

    pub fn cancel_radius(&mut self, c: [f32; 3], r: f32) {
        self.bullets.retain(|b| (b.pos[0] - c[0]).powi(2) + (b.pos[1] - c[1]).powi(2) >= r * r);
    }

    // BulletMgr_SpawnPattern placement (07 §9.3).
    fn spawn_pattern(&mut self, anm: &mut AnmManager, d: &BulletDesc, player: [f32; 3]) {
        self.patterns += 1;
        if self.bullets.len() >= MAX_BULLETS {
            return;
        }
        let aim = angle_from(d.origin, player);
        let (c1, c2) = (d.count1.max(0) as i32, d.count2.max(0) as i32);
        let (a1, a2) = (d.angle1, d.angle2);
        let tau = 2.0 * PI;
        for j in 0..c2 {
            for i in 0..c1 {
                if self.bullets.len() >= MAX_BULLETS {
                    return;
                }
                let mut speed = if c2 < 2 { d.speed1 } else { d.speed1 - (d.speed1 - d.speed2) * j as f32 / c2 as f32 };
                let ring = j as f32 * a2 + a1 + i as f32 * tau / c1 as f32;
                let angle = match d.aim_mode {
                    0 | 1 => {
                        let mut k = if c1 % 2 == 0 { a2 / 2.0 + (i / 2) as f32 * a2 } else { ((i + 1) / 2) as f32 * a2 };
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
                let angle = anmvm::wrap_add(angle, 0.0);
                let mut vm = AnmVm::default();
                let script = MAIN_SCRIPTS[(d.kind.max(0) as usize).min(20)];
                anm.set_script(&mut vm, ANM_SLOT, script);
                vm.set_sprite(anm, vm.sprite_index as i32 + d.colour as i32);
                let gs = anm.speed;
                let vel = [angle.cos() * speed * gs, angle.sin() * speed * gs, 0.0];
                self.bullets.push(Bullet { pos: [d.origin[0], d.origin[1], 0.1], vel, angle, speed, kind: d.kind, colour: d.colour, flags: d.flags, vm });
            }
        }
    }

    pub fn step(&mut self, anm: &mut AnmManager) {
        let player = crate::player::position();
        for d in std::mem::take(&mut self.requests) {
            self.spawn_pattern(anm, &d, player);
        }
        for b in &mut self.bullets {
            b.pos[0] += b.vel[0];
            b.pos[1] += b.vel[1];
            b.vm.run(anm);
        }
        self.bullets.retain(|b| (-64.0..=448.0).contains(&b.pos[0]) && (-64.0..=512.0).contains(&b.pos[1]));
        let gs = anm.speed;
        for slot in &mut self.lasers {
            let Some(l) = slot else { continue };
            l.head += gs * l.desc.speed;
            if l.head - l.tail > l.desc.max_len {
                l.tail = l.head - l.desc.max_len;
            }
            l.tail = l.tail.max(0.0);
            let t = l.timer.cur;
            let dead = match l.state {
                0 if t >= l.desc.startup => {
                    (l.state, l.timer) = (1, Timer::default());
                    false
                }
                1 if t >= l.desc.duration => {
                    (l.state, l.timer) = (2, Timer::default());
                    l.desc.fade == 0
                }
                2 => t >= l.desc.fade,
                _ => false,
            };
            if dead || l.tail >= 640.0 {
                *slot = None;
            } else {
                l.timer.tick(gs);
            }
        }
        self.items.truncate(4096);
    }

    fn draw_lasers(&self, gfx: &mut Gfx) {
        for l in self.lasers.iter().flatten() {
            let (s, c) = l.angle.sin_cos();
            let w = if l.state == 1 { l.desc.width / 2.0 } else { 1.0 };
            let (ox, oy) = (l.pos[0] + 32.0, l.pos[1] + 16.0);
            let p = |d: f32, n: f32| [ox + c * d - s * n, oy + s * d + c * n];
            let q = [p(l.tail, -w), p(l.head, -w), p(l.head, w), p(l.tail, w)];
            gfx.quad(gfx.white(), Blend::Add, q, [[0.0; 2]; 4], [255, 120, 160, 160]);
        }
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
        let (anm, gfx) = (&mut ctx.anm, &mut ctx.gfx);
        gfx.set_viewport(PLAYFIELD);
        self.draw_lasers(gfx);
        for b in &mut self.bullets {
            b.vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.0];
            b.vm.rotation[2] = b.angle + PI / 2.0;
            anm.draw_mode(gfx, &mut b.vm, 2);
        }
        Ret::Next
    }
}
