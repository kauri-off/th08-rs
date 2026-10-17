// Bombs (09 §7) and damage/cancel areas (09 §8).
use crate::anmmgr::AnmManager;
use crate::anmvm::{AnmVm, Timer};
use crate::gfx::{Blend, Gfx};
use crate::input;
use crate::item::ItemManager;
use crate::player::{ANM_SLOT, Player, Stats};
use std::f32::consts::{PI, TAU};

pub const AREAS: usize = 192;

#[derive(Clone, Copy, Debug, Default)]
pub struct Area {
    pub pos: [f32; 2],
    pub r: f32,
    pub dr: f32,
    pub size: [f32; 2],
    pub dsize: [f32; 2],
    pub rot: f32,
    pub life: i32,
    pub item: i32,
    pub damage: i32,
    pub total: i32,
    pub max: i32,
    pub period: i32,
    pub active: bool,
}

impl Area {
    // Circle: distance² < r² (to the nearest point of the box); rectangle: inside the rotated bounds.
    pub fn contains(&self, p: [f32; 2], half: [f32; 2]) -> bool {
        let (dx, dy) = (p[0] - self.pos[0], p[1] - self.pos[1]);
        if self.r > 0.0 {
            let ex = (dx.abs() - half[0]).max(0.0);
            let ey = (dy.abs() - half[1]).max(0.0);
            return ex * ex + ey * ey < self.r * self.r;
        }
        let (s, c) = (-self.rot).sin_cos();
        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
        rx.abs() <= self.size[0] / 2.0 + half[0] && ry.abs() <= self.size[1] / 2.0 + half[1]
    }
}

#[derive(Clone, Debug, Default)]
pub struct BombObj {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub vm: Option<AnmVm>,
    pub t: i32,
    pub k: usize,
    pub angle: f32,
    pub r: f32,
    pub target: [f32; 2],
    pub area: Option<usize>,
    pub carea: Option<usize>,
    pub phase: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Bomb {
    pub index: usize,
    pub duration: i32,
    pub used: i32,
    pub timer: Timer,
    pub origin: [f32; 3],
    pub objs: Vec<BombObj>,
    pub name: &'static str,
    pub ch: u8,
    pub attract: bool,
    pub bg: Option<AnmVm>,
}

// (name, duration, invulnerability, speed multiplier) per character (Reimu, Yukari, Marisa, Alice, Sakuya,
// Remilia, Youmu, Yuyuko) × (bomb, last spell), 09 §7.
type BombSpec = (&'static str, i32, i32);
const SPECS: [[BombSpec; 2]; 8] = [
    [("霊符「夢想妙珠」", 200, 260), ("神霊「夢想封印　瞬」", 200, 260)],
    [("境符「四重結界」", 150, 200), ("境界「永夜四重結界」", 250, 300)],
    [("恋符「マスタースパーク」", 300, 350), ("魔砲「ファイナルスパーク」", 350, 380)],
    [("魔符「アーティフルサクリファイス」", 210, 250), ("魔操「リターンイナニメトネス」", 230, 280)],
    [("幻符「殺人ドール」", 250, 290), ("幻葬「夜霧の幻影殺人鬼」", 320, 350)],
    [("紅符「不夜城レッド」", 240, 290), ("紅魔「スカーレットデビル」", 280, 320)],
    [("人符「現世斬」", 220, 270), ("人鬼「未来永劫斬」", 250, 300)],
    [("死符「ギャストリドリーム」", 300, 350), ("死蝶「華胥の永眠」", 300, 350)],
];
const COLOURS: [[u8; 4]; 9] = [
    [255, 80, 80, 70],
    [200, 80, 255, 60],
    [255, 255, 160, 70],
    [120, 160, 255, 70],
    [200, 200, 255, 70],
    [255, 40, 40, 70],
    [160, 255, 200, 70],
    [255, 140, 220, 70],
    [255, 255, 255, 60],
];

fn polar(a: f32, r: f32) -> [f32; 2] {
    [a.cos() * r, a.sin() * r]
}

enum OrbEv {
    Move(usize, [f32; 2]),
    Fly([f32; 2]),
    Impact([f32; 2], Option<usize>),
}

pub fn bomb_char(shot: u8, index: usize) -> u8 {
    if shot >= 4 { shot - 4 } else { shot * 2 + (index & 1) as u8 }
}

impl Player {
    fn free_slot(v: &[Area]) -> usize {
        v.iter().position(|a| !a.active).unwrap_or(AREAS - 1)
    }

    pub fn cancel_circle(&mut self, pos: [f32; 2], r: f32, dr: f32, life: i32, item: i32) -> usize {
        let i = Self::free_slot(&self.cancel_areas);
        self.cancel_areas[i] = Area { pos, r, dr, life, item, period: 1, active: true, ..Area::default() };
        i
    }

    pub fn cancel_rect(&mut self, pos: [f32; 2], size: [f32; 2], item: i32, life: i32) -> usize {
        let i = Self::free_slot(&self.cancel_areas);
        self.cancel_areas[i] = Area { pos, size, life, item, period: 1, active: true, ..Area::default() };
        i
    }

    pub fn damage_circle(&mut self, pos: [f32; 2], r: f32, dr: f32, damage: i32, life: i32) -> usize {
        let i = Self::free_slot(&self.damage_areas);
        self.damage_areas[i] = Area { pos, r, dr, life, damage, period: 1, item: 6, active: true, ..Area::default() };
        i
    }

    pub fn damage_rect(&mut self, pos: [f32; 2], size: [f32; 2], damage: i32, life: i32) -> usize {
        let i = Self::free_slot(&self.damage_areas);
        self.damage_areas[i] = Area { pos, size, life, damage, period: 1, item: 6, active: true, ..Area::default() };
        i
    }

    // 09 §8.1, start of Player_Calc.
    pub fn update_areas(&mut self) {
        for a in self.damage_areas.iter_mut().chain(self.cancel_areas.iter_mut()).filter(|a| a.active) {
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

    // Player_UpdateBomb (09 §7).
    pub fn update_bomb(&mut self, anm: &mut AnmManager, inp: &input::Input, items: &mut Option<&mut ItemManager>) {
        if !self.bombing {
            let forced = self.forced_bomb != 0 && self.deathbomb == 1;
            let pressed = inp.pressed(input::BOMB) && !self.env.dialogue && self.deathbomb != 0 && self.stats.bombs >= 1.0 && self.bomb_cooldown == 0;
            if !(forced || pressed) || matches!(self.state, 1) {
                return;
            }
            self.start_bomb(anm, forced);
            self.bomb_calc(anm, items);
            self.bomb.timer.tick(anm.speed);
            return;
        }
        if self.bomb.timer.cur < self.bomb.duration {
            self.bomb_calc(anm, items);
            self.bomb.timer.tick(anm.speed);
        } else {
            self.speed_mult = [1.0; 2];
            self.bombing = false;
            self.bomb_active = false;
            self.bomb.objs.clear();
            self.bomb.bg = None;
        }
    }

    fn start_bomb(&mut self, anm: &mut AnmManager, forced: bool) {
        let rng = &mut anm.rng;
        self.forced_bomb = 0;
        self.freeze_bullets = false;
        let mut index = self.focus.min(1) as usize;
        if self.last_spell != 0 {
            index = 1 - index + 2;
            let b = self.stats.bombs;
            let used = if forced || b < 2.0 { b } else { 2.0 };
            self.bomb.used = used as i32;
            self.stats.add_bombs(-used, rng);
        } else {
            self.stats.add_bombs(-1.0, rng);
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

    // Bomb start helper 0x40BE30.
    fn bomb_begin(&mut self, anm: &mut AnmManager, spec: BombSpec) {
        self.bomb.name = spec.0;
        crate::sound::play(crate::sound::CAT);
        self.bomb.duration = spec.1;
        self.state = 3;
        self.state_timer.set(spec.2);
        crate::effect::invuln_ring(anm, self.pos, spec.2);
        self.bomb.objs.clear();
        self.bomb.bg = None;
        self.bomb.attract = true;
        self.bomb.origin = self.pos;
    }

    fn bomb_calc(&mut self, anm: &mut AnmManager, items: &mut Option<&mut ItemManager>) {
        let t = self.bomb.timer.cur;
        let ls = self.bomb.index & 2 != 0;
        if self.bomb.index == 4 {
            if t == 0 {
                self.bomb_begin(anm, ("「ディゾルブスペル」", 120, 200));
            }
            self.speed_mult = [0.0; 2];
            return;
        }
        let ch = self.bomb.ch as usize;
        if t == 0 {
            self.bomb_begin(anm, SPECS[ch][ls as usize]);
        }
        match ch {
            0 => self.bomb_reimu(anm, t, ls),
            1 => self.bomb_yukari(anm, t, ls),
            2 => self.bomb_marisa(t, ls),
            3 => self.bomb_alice(t),
            4 => self.bomb_sakuya(anm, t, ls),
            5 => self.bomb_remilia(t, ls),
            6 => self.bomb_youmu(t, ls),
            _ => self.bomb_yuyuko(anm, t, ls),
        }
        let d = 26000 / self.bomb.duration.max(1);
        self.stats.gauge_add(if self.bomb.index & 1 == 1 { d } else { -d });
        for o in &mut self.bomb.objs {
            if let Some(vm) = &mut o.vm {
                vm.run(anm);
            }
        }
        if let Some(vm) = &mut self.bomb.bg {
            vm.run(anm);
        }
        let _ = items;
    }

    fn obj_vm(anm: &mut AnmManager, script: usize) -> Option<AnmVm> {
        let mut vm = AnmVm::default();
        anm.set_script(&mut vm, ANM_SLOT, script);
        vm.script.is_some().then_some(vm)
    }

    fn free_obj_areas(&mut self, o: &BombObj) {
        if let Some(i) = o.area {
            self.damage_areas[i].active = false;
        }
        if let Some(i) = o.carea {
            self.cancel_areas[i].active = false;
        }
    }

    // SPEC-GAP: bomb choreography is summarised in 09 (INFERRED); objects below follow the listed numbers with
    // simple motion. Reimu: 16 orbs circle the player, launch from frame 40 (4 apart) and burst on impact.
    fn bomb_reimu(&mut self, anm: &mut AnmManager, t: i32, ls: bool) {
        let me = [self.pos[0], self.pos[1]];
        if t == 0 {
            self.cancel_circle(me, 96.0, 0.0, 200, 6);
            for k in 0..16 {
                let a = self.damage_circle(me, 64.0, 0.0, 5, 200);
                self.damage_areas[a].period = 2;
                self.damage_areas[a].max = 200;
                let vm = Self::obj_vm(anm, 19);
                self.bomb.objs.push(BombObj { pos: me, k, angle: k as f32 * TAU / 16.0, area: Some(a), vm, ..BombObj::default() });
            }
        }
        let aim = self.aimed.map(|p| [p[0], p[1]]);
        let mut events = Vec::new();
        let mut objs = std::mem::take(&mut self.bomb.objs);
        objs.retain_mut(|o| {
            o.t += 1;
            let launch = 40 + 4 * o.k as i32;
            if o.t < launch {
                o.angle += 0.08;
                o.r = (o.t as f32 * 2.0).min(64.0);
                let d = polar(o.angle, o.r);
                o.pos = [me[0] + d[0], me[1] + d[1]];
            } else {
                if o.phase == 0 {
                    o.phase = 1;
                    o.target = aim.unwrap_or([o.pos[0], -32.0]);
                    o.vel = polar(o.angle, 2.0);
                }
                let d = [o.target[0] - o.pos[0], o.target[1] - o.pos[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-3);
                let sp = ((o.t - launch) as f32 * 0.3 + 2.0).min(10.0);
                o.vel = [o.vel[0] * 0.85 + d[0] / len * sp * 0.15, o.vel[1] * 0.85 + d[1] / len * sp * 0.15];
                o.pos = [o.pos[0] + o.vel[0], o.pos[1] + o.vel[1]];
                if len < 16.0 || o.pos[1] < -16.0 || o.t > launch + 120 || t >= 190 {
                    events.push(OrbEv::Impact(o.pos, o.area));
                    return false;
                }
                events.push(OrbEv::Fly(o.pos));
            }
            if let Some(a) = o.area {
                events.push(OrbEv::Move(a, o.pos));
            }
            true
        });
        self.bomb.objs = objs;
        for ev in events {
            match ev {
                OrbEv::Move(a, p) => self.damage_areas[a].pos = p,
                OrbEv::Fly(p) => {
                    self.cancel_circle(p, 128.0, 0.0, 0, 6);
                }
                OrbEv::Impact(p, area) => {
                    if let Some(a) = area {
                        self.damage_areas[a].active = false;
                    }
                    self.cancel_circle(p, 64.0, 4.27, 30, 6);
                    if ls {
                        let a = self.damage_circle(p, 64.0, 8.53, 25, 15);
                        self.damage_areas[a].period = 5;
                        self.damage_areas[a].max = 50;
                    } else {
                        let a = self.damage_circle(p, 64.0, 12.8, 500, 12);
                        self.damage_areas[a].period = 4;
                    }
                }
            }
        }
        if ls && t >= 40 && (t - 40) % 20 == 0 {
            let p = match self.aimed {
                Some(a) => [a[0], a[1]],
                None => [anm.rng.next_f32() * 384.0, anm.rng.next_f32() * 224.0],
            };
            self.cancel_circle(p, 64.0, 4.27, 30, 6);
            let a = self.damage_circle(p, 64.0, 8.53, 400, 15);
            self.damage_areas[a].period = 2;
        }
    }

    // Yukari: four barriers 30 frames apart at the player.
    fn bomb_yukari(&mut self, anm: &mut AnmManager, t: i32, ls: bool) {
        if t == 0 {
            self.bomb.bg = Self::obj_vm(anm, if ls { 22 } else { 21 });
        }
        let gap = if ls { 50 } else { 30 };
        if t % gap == 0 && t / gap < 4 {
            let me = [self.pos[0], self.pos[1]];
            let life = if ls && t / gap >= 2 { 100 } else { 40 };
            let c = self.cancel_circle(me, 100.0, 0.0, life, 6);
            self.cancel_areas[c].dr = 1.0;
            let d = self.damage_circle(me, 100.0, 1.0, 70, 40);
            self.damage_areas[d].period = 5;
        }
    }

    // Marisa: beam from the player to the top, refreshed every frame.
    fn bomb_marisa(&mut self, t: i32, ls: bool) {
        self.speed_mult = [0.2; 2];
        let _ = t;
        let len = self.pos[1].max(1.0);
        let c = [self.pos[0], self.pos[1] / 2.0];
        self.cancel_rect(c, [384.0, len], 6, 0);
        self.damage_rect(c, [128.0, len], 12, 0);
        self.damage_rect(c, [384.0, len], if ls { 7 } else { 6 }, 0);
    }

    // Alice: growing blast at the doll; small blasts every 10 frames after the growth.
    fn bomb_alice(&mut self, t: i32) {
        let p = if self.options[0].state != 0 { self.options[0].pos } else { self.pos };
        let p = [p[0], p[1]];
        if t == 0 {
            self.cancel_circle(p, 1.0, 5.0, 110, 6);
            let d = self.damage_circle(p, 1.0, 5.0, 70, 110);
            self.damage_areas[d].period = 5;
        }
        if t >= 110 && t % 10 == 0 {
            self.cancel_circle(p, 32.0, 0.0, 0, 6);
            self.damage_circle(p, 32.0, 0.0, 40, 0);
        }
    }

    // Sakuya: knives thrown in random directions (RNG) every 8 frames for 160 frames.
    fn bomb_sakuya(&mut self, anm: &mut AnmManager, t: i32, ls: bool) {
        self.speed_mult = [0.5; 2];
        let me = [self.pos[0], self.pos[1]];
        if t < 160 && t % 8 == 0 {
            for _ in 0..4 {
                let a = anm.rng.next_f32() * TAU;
                let c = self.cancel_circle(me, 32.0, 0.0, 500, 6);
                let d = self.damage_circle(me, 32.0, 0.0, if ls { 30 } else { 20 }, 500);
                let mut vm = Self::obj_vm(anm, 12);
                if let Some(v) = &mut vm {
                    v.rotation[2] = a;
                    v.flags |= crate::anmvm::F_ROT_DIRTY;
                }
                self.bomb.objs.push(BombObj { pos: me, vel: polar(a, 6.0), angle: a, area: Some(d), carea: Some(c), vm, ..BombObj::default() });
            }
        }
        self.move_objs(|o| {
            o.pos = [o.pos[0] + o.vel[0], o.pos[1] + o.vel[1]];
            !(o.pos[0] < -32.0 || o.pos[0] > 416.0 || o.pos[1] < -32.0 || o.pos[1] > 480.0)
        });
    }

    fn move_objs(&mut self, mut f: impl FnMut(&mut BombObj) -> bool) {
        let mut objs = std::mem::take(&mut self.bomb.objs);
        let mut dead = Vec::new();
        objs.retain_mut(|o| {
            o.t += 1;
            let keep = f(o);
            if !keep {
                dead.push(o.clone());
            }
            keep
        });
        for o in &dead {
            self.free_obj_areas(o);
        }
        for o in &objs {
            if let Some(i) = o.area {
                self.damage_areas[i].pos = o.pos;
            }
            if let Some(i) = o.carea {
                self.cancel_areas[i].pos = o.pos;
            }
        }
        self.bomb.objs = objs;
    }

    // Remilia: cancel cross at the player; the damage comes from shot levels 6/7 (09 §2.1).
    fn bomb_remilia(&mut self, t: i32, ls: bool) {
        let m = if t < 60 { 0.0 } else if ls { 3.0 } else { 2.0 };
        self.speed_mult = [m; 2];
        let me = [self.pos[0], self.pos[1]];
        self.cancel_rect(me, [96.0, 800.0], 6, 0);
        self.cancel_rect(me, [800.0, 96.0], 6, 0);
        self.cancel_circle(me, 96.0, 0.0, 0, 6);
    }

    // Youmu: three vertical slashes 30 frames apart; last spell: seven rows and a final screen slash.
    fn bomb_youmu(&mut self, t: i32, ls: bool) {
        self.speed_mult = [if t < 60 { 0.0 } else { 1.0 }; 2];
        if !ls {
            if t % 30 == 0 && t / 30 < 3 {
                let k = (t / 30) as usize;
                let c = [self.pos[0], 224.0];
                self.cancel_rect(c, [96.0, 448.0], 6, 60);
                let d = self.damage_rect(c, [96.0, 448.0], [80, 100, 300][k], 60);
                self.damage_areas[d].period = [2, 3, 5][k];
            }
            return;
        }
        if t % 20 == 0 && t / 20 < 7 {
            let k = t / 20;
            let c = [192.0, 416.0 - 64.0 * k as f32];
            self.cancel_rect(c, [384.0, 64.0], 6, 20);
            let d = self.damage_rect(c, [384.0, 64.0], 100 - 10 * k, 20);
            self.damage_areas[d].period = 2;
        }
        if t == 200 {
            self.cancel_rect([192.0, 224.0], [384.0, 448.0], 6, 0);
            self.damage_rect([192.0, 224.0], [384.0, 448.0], 500, 0);
        }
    }

    // Yuyuko: four butterflies spiralling out from the player.
    fn bomb_yuyuko(&mut self, anm: &mut AnmManager, t: i32, ls: bool) {
        self.speed_mult = [0.8; 2];
        let me = [self.pos[0], self.pos[1]];
        if t == 0 {
            for k in 0..4 {
                let c = self.cancel_circle(me, 24.0, 0.0, 500, 6);
                let d = self.damage_circle(me, 24.0, 0.0, 50, 500);
                self.damage_areas[d].max = if ls && k == 0 { 1200 } else { 800 };
                let vm = Self::obj_vm(anm, 11);
                self.bomb.objs.push(BombObj { pos: me, k, angle: k as f32 * PI / 2.0, area: Some(d), carea: Some(c), vm, ..BombObj::default() });
            }
            if ls {
                self.cancel_circle(me, 64.0, 0.0, 300, 6);
            }
        }
        self.move_objs(|o| {
            o.angle += 0.04;
            o.r = (o.t as f32 * 1.2).min(150.0);
            let d = polar(o.angle, o.r);
            o.pos = [me[0] + d[0], me[1] + d[1]];
            true
        });
    }

    fn disc(gfx: &mut Gfx, c: [f32; 2], r: f32, col: [u8; 4]) {
        let n = 16;
        for i in 0..n {
            let a0 = i as f32 * TAU / n as f32;
            let a1 = (i + 1) as f32 * TAU / n as f32;
            let p0 = [c[0] + a0.cos() * r, c[1] + a0.sin() * r];
            let p1 = [c[0] + a1.cos() * r, c[1] + a1.sin() * r];
            gfx.solid_quad(Blend::Add, [c, p0, p1, p1], col);
        }
    }

    fn area_shape(gfx: &mut Gfx, a: &Area, col: [u8; 4]) {
        let c = [a.pos[0] + 32.0, a.pos[1] + 16.0];
        if a.r > 0.0 {
            Self::disc(gfx, c, a.r, col);
            return;
        }
        let (s, co) = a.rot.sin_cos();
        let (hw, hh) = (a.size[0] / 2.0, a.size[1] / 2.0);
        let pts = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map(|[x, y]| [c[0] + x * co - y * s, c[1] + x * s + y * co]);
        gfx.solid_quad(Blend::Add, pts, col);
    }

    // SPEC-GAP: bomb graphics are not specified; areas are drawn as additive shapes plus the obvious sprites.
    pub fn draw_bomb(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        if self.bombing {
            let col = COLOURS[if self.bomb.index == 4 { 8 } else { self.bomb.ch as usize }];
            if let Some(vm) = &mut self.bomb.bg {
                vm.pos = [self.pos[0] + 32.0, self.pos[1] + 16.0, 0.0];
                anm.draw(gfx, vm);
            }
            for a in self.damage_areas.iter().filter(|a| a.active) {
                Self::area_shape(gfx, a, col);
            }
            let faint = [col[0], col[1], col[2], col[3] / 3];
            for a in self.cancel_areas.iter().filter(|a| a.active && a.item >= 0) {
                Self::area_shape(gfx, a, faint);
            }
            for o in &mut self.bomb.objs {
                if let Some(vm) = &mut o.vm {
                    vm.pos = [o.pos[0] + 32.0, o.pos[1] + 16.0, 0.0];
                    anm.draw(gfx, vm);
                }
            }
        }
    }
}
