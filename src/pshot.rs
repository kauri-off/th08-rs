// Player shots and options (09 §4, §5).
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::gfx::Gfx;
use crate::input;
use crate::item::ItemManager;
use crate::player::{ANM_SLOT, Player};
use crate::sht::ShotEntry;
use std::f32::consts::{FRAC_PI_2, PI};

pub const MAX_BULLETS: usize = 128;

fn polar(a: f32, r: f32) -> [f32; 2] {
    [a.cos() * r, a.sin() * r]
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OptKind {
    #[default]
    None,
    Yukari,
    Alice,
    Remilia,
    Yuyuko,
    YoumuHalf,
    RemiliaSolo,
    YoumuSolo,
}

#[derive(Clone, Debug, Default)]
pub struct Opt {
    pub vm: AnmVm,
    pub pos: [f32; 3],
    pub anchor: [f32; 3],
    pub vel: [f32; 2],
    pub state: u8,
    pub sub: u8,
    pub index: usize,
    pub angle: f32,
    pub facing: f32,
    pub timer: Timer,
    pub kind: OptKind,
}

#[derive(Clone, Debug, Default)]
pub struct PBullet {
    pub vm: AnmVm,
    pub pos: [f32; 3],
    pub size: [f32; 2],
    pub vel: [f32; 2],
    pub speed: f32,
    pub angle: f32,
    pub timer: Timer,
    pub damage: i16,
    pub state: u8,
    pub kind: i16,
    pub slot: i16,
    pub focus: u8,
    pub script: i16,
    pub side: i16,
    pub origin: i16,
    pub red: bool,
    pub update: i32,
    pub draw: i32,
    pub hit: i32,
    pub hits: u32,
    pub trail: [[f32; 3]; 32],
    pub trail_n: i32,
}

pub const LASER_SLOTS: usize = 3;

// Player laser slot (+0xE2A38 Timer + bullet, entry at +0xE2A80; 14 §4).
#[derive(Clone, Copy, Debug, Default)]
pub struct LaserSlot {
    pub owner: Option<usize>,
    pub timer: i32,
    pub entry: Option<ShotEntry>,
}

const REMILIA_ANCHOR: [[f32; 2]; 4] = [[-30.0, -16.0], [-10.0, -32.0], [10.0, -32.0], [30.0, -16.0]];
const REMILIA_SPIN: [f32; 4] = [PI / 120.0, -PI / 90.0, PI / 90.0, -PI / 120.0];
const REMILIA_ORBIT: [f32; 4] = [0.0, PI, 0.0, PI];
// Youmu facing target by direction code 1–8 (away from the motion).
const FACING: [f32; 8] = [FRAC_PI_2, -FRAC_PI_2, 0.0, PI, PI / 4.0, 3.0 * PI / 4.0, -PI / 4.0, -3.0 * PI / 4.0];

impl Player {
    fn youkai_set(&self) -> (OptKind, usize) {
        match crate::sht::team(self.shot_type) {
            0 => (OptKind::Yukari, 1),
            1 => (OptKind::Alice, 1),
            2 => (OptKind::Remilia, 4),
            _ => (OptKind::Yuyuko, 2),
        }
    }

    // A new option starts in state 1; its calc fn initialises it on the next update.
    fn set_option(&mut self, k: usize, kind: OptKind) {
        let state = if kind == OptKind::None { 0 } else { 1 };
        self.options[k] = Opt { kind, index: k, state, ..Opt::default() };
    }

    fn remove_option(&mut self, k: usize) {
        let o = &mut self.options[k];
        if o.state != 0 && o.state != 3 {
            o.state = 3;
            o.timer.set(0);
        }
    }

    // Solos get their set at load (14 §3).
    pub fn spawn_options(&mut self, _anm: &mut AnmManager, _solo: bool) {
        match self.shot_type {
            5 => self.set_option(0, OptKind::Yukari),
            7 => self.set_option(0, OptKind::Alice),
            9 => (0..4).for_each(|k| self.set_option(k, OptKind::RemiliaSolo)),
            11 => (0..2).for_each(|k| self.set_option(k, OptKind::Yuyuko)),
            10 => self.set_option(2, OptKind::YoumuSolo),
            _ => {}
        }
    }

    // Teams: focus rewrites all 4 slots from the youkai table; unfocus sends the youkai options away and
    // gives team 3 Youmu's half in slot 2 (trail reset to the player).
    pub fn swap_options(&mut self, _anm: &mut AnmManager, f: u8) {
        let (kind, n) = self.youkai_set();
        let team3 = crate::sht::team(self.shot_type) == 3;
        if f == 1 {
            (0..4).for_each(|k| self.set_option(k, if k < n { kind } else { OptKind::None }));
        } else {
            (0..if team3 { 2 } else { 4 }).for_each(|k| self.remove_option(k));
            if team3 {
                self.trail = [self.pos; 16];
                self.set_option(2, OptKind::YoumuHalf);
            }
        }
    }

    fn init_option(&mut self, anm: &mut AnmManager, k: usize) {
        let p = self.pos;
        let trail = self.trail[15];
        let o = &mut self.options[k];
        let script = match o.kind {
            OptKind::Yukari => 0x12,
            OptKind::Alice => 0x1D,
            OptKind::Remilia | OptKind::RemiliaSolo => 0x18,
            OptKind::Yuyuko | OptKind::YoumuHalf | OptKind::YoumuSolo => 0x15,
            OptKind::None => return,
        };
        anm.set_script(&mut o.vm, ANM_SLOT, script);
        o.timer.set(0);
        o.sub = 0;
        o.vel = [0.0; 2];
        match o.kind {
            OptKind::Yukari => {
                o.pos = [p[0], (p[1] - 96.0).max(32.0), 0.0];
                self.aimed = None;
            }
            OptKind::Alice => o.pos = [p[0], p[1] - 32.0, 0.0],
            // SPEC-GAP: the team bats' start orbit angle is not given; the solo values are used.
            OptKind::Remilia | OptKind::RemiliaSolo => {
                o.anchor = [p[0] + REMILIA_ANCHOR[k][0], p[1] + REMILIA_ANCHOR[k][1], 0.0];
                o.angle = REMILIA_ORBIT[k];
            }
            OptKind::Yuyuko => {
                o.anchor = p;
                o.angle = if k == 0 { 0.0 } else { -PI };
            }
            OptKind::YoumuHalf | OptKind::YoumuSolo => {
                o.anchor = trail;
                o.angle = 0.0;
                o.facing = -FRAC_PI_2;
            }
            OptKind::None => {}
        }
        o.state = 2;
    }

    // Option calc functions (14 §3, 09 §5); positions in playfield space.
    fn place_option(&mut self, anm: &mut AnmManager, k: usize, shot_held: bool) {
        let (px, py) = (self.pos[0], self.pos[1]);
        let bombing = self.bombing;
        let leaving = self.options[k].state == 3;
        let shooting = self.shot_timer.cur >= 0;
        let unfocused = self.focus == 0;
        let dir = self.direction;
        let trail = self.trail[15];
        let mut aimed = self.aimed;
        let o = &mut self.options[k];
        let t = o.timer.cur;
        let mut sparkle = None;
        match o.kind {
            OptKind::Yukari => {
                let smooth = |o: &mut Opt, target: [f32; 2]| {
                    let want = [(target[0] - o.pos[0]) / 16.0, (target[1] - o.pos[1]) / 16.0];
                    o.vel = [o.vel[0] + (want[0] - o.vel[0]) * 0.2, o.vel[1] + (want[1] - o.vel[1]) * 0.2];
                    o.pos[0] += o.vel[0];
                    o.pos[1] += o.vel[1];
                    if o.vel[0].abs() < 0.05 {
                        o.vel[0] = 0.0;
                    }
                };
                if o.sub < 3 {
                    smooth(o, [px, (py - 96.0).max(32.0)]);
                    if shooting && aimed.is_some() && t >= 10 {
                        o.vm.pending_interrupt = 3;
                        o.sub = 3;
                    } else {
                        aimed = None;
                        let vx = o.vel[0];
                        let mirror = |o: &mut Opt, neg: bool| o.vm.scale[0] = if neg { -o.vm.scale[0].abs() } else { o.vm.scale[0].abs() };
                        match o.sub {
                            0 if vx != 0.0 => {
                                o.vm.pending_interrupt = 2;
                                o.sub = if vx > 0.0 { 2 } else { 1 };
                                mirror(o, vx > 0.0);
                            }
                            1 | 2 if vx == 0.0 => {
                                o.vm.pending_interrupt = 1;
                                o.sub = 0;
                                mirror(o, false);
                            }
                            1 if vx > 0.0 => {
                                o.vm.pending_interrupt = 2;
                                o.sub = 2;
                                mirror(o, true);
                            }
                            2 if vx < 0.0 => {
                                o.vm.pending_interrupt = 2;
                                o.sub = 1;
                                mirror(o, false);
                            }
                            _ => {}
                        }
                        o.vm.flags |= anmvm::F_SCALE_DIRTY;
                    }
                } else {
                    match aimed {
                        Some(e) if shooting || shot_held => smooth(o, [e[0], (e[1] + 32.0).max(32.0)]),
                        _ => {
                            o.vm.pending_interrupt = 1;
                            o.sub = 0;
                            aimed = None;
                        }
                    }
                }
            }
            OptKind::Alice => {
                if !bombing || leaving {
                    o.pos = [px, py - 32.0, 0.0];
                }
            }
            OptKind::Remilia | OptKind::RemiliaSolo => {
                if t >= 12 {
                    o.angle += REMILIA_SPIN[k];
                }
                if o.kind == OptKind::RemiliaSolo {
                    let c = if unfocused { 0xFF80FFFFu32 } else { 0xFFFF8080 };
                    o.vm.color = c.to_le_bytes();
                    if unfocused {
                        let a = [px + REMILIA_ANCHOR[k][0], py + REMILIA_ANCHOR[k][1]];
                        o.anchor[0] += (a[0] - o.anchor[0]) * 0.2;
                        o.anchor[1] += (a[1] - o.anchor[1]) * 0.2;
                    }
                    sparkle = Some(0x80602050);
                }
                let d = polar(o.angle, 8.0);
                o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
            }
            OptKind::Yuyuko => {
                let tx = if k == 0 { px - 32.0 } else { px + 32.0 };
                o.anchor[0] += (tx - o.anchor[0]) * 0.09;
                o.anchor[1] += (py - o.anchor[1]) * 0.09;
                o.angle += if k == 0 { PI / 60.0 } else { -PI / 60.0 };
                let d = polar(o.angle, 6.0);
                o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
                // SPEC-GAP: the colour of Yuyuko's option sparkle is not given; Youmu's is used.
                sparkle = Some(0x80405080);
            }
            OptKind::YoumuHalf | OptKind::YoumuSolo => {
                o.angle += PI / 60.0;
                o.anchor[0] += (trail[0] - o.anchor[0]) * 0.05;
                o.anchor[1] += (trail[1] - o.anchor[1]) * 0.05;
                let d = polar(o.angle, 8.0);
                o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
                let tracks = o.kind == OptKind::YoumuHalf || unfocused;
                if o.kind == OptKind::YoumuSolo {
                    o.vm.color = if unfocused { 0xFFFFFFFFu32 } else { 0xFFFF8080 }.to_le_bytes();
                }
                if tracks && (1..=8).contains(&dir) {
                    let mut target = FACING[dir as usize - 1];
                    if (o.facing - target).abs() > PI {
                        target += if target < o.facing { 2.0 * PI } else { -2.0 * PI };
                    }
                    o.facing = if (target - o.facing).abs() <= FRAC_PI_2 { anmvm::wrap_add(o.facing + (target - o.facing) * 0.07, 0.0) } else { target };
                }
                sparkle = Some(if tracks { 0x80405080 } else { 0xFFF05080 });
            }
            OptKind::None => {}
        }
        let pos = o.pos;
        self.aimed = aimed;
        if let Some(c) = sparkle {
            crate::effect::spawn(anm, 0x2F, pos, 1, c);
        }
    }

    pub fn update_options(&mut self, anm: &mut AnmManager, shot_held: bool) {
        let gs = anm.speed;
        for k in 0..4 {
            match self.options[k].state {
                0 => continue,
                1 => self.init_option(anm, k),
                3 => {
                    if self.options[k].timer.cur == 0 {
                        self.options[k].vm.pending_interrupt = 5;
                    }
                    if self.options[k].timer.cur >= 16 {
                        self.options[k] = Opt::default();
                        continue;
                    }
                }
                _ => {}
            }
            self.place_option(anm, k, shot_held);
            let o = &mut self.options[k];
            if o.vm.script.is_some() {
                o.vm.run(anm);
            }
            o.timer.tick(gs);
        }
    }

    // Option draw fn 0x44E9E0: VM at pos + playfield origin, z 0.49, Draw2DRot.
    pub fn draw_options(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for o in self.options.iter_mut().filter(|o| o.state != 0 && o.vm.script.is_some()) {
            o.vm.pos = [o.pos[0] + 32.0, o.pos[1] + 16.0, 0.0];
            anm.draw_mode_z(gfx, &mut o.vm, 2, 0.49);
        }
    }

    fn origin_pos(&self, origin: i16) -> [f32; 3] {
        match origin {
            1..=4 => {
                let o = &self.options[origin as usize - 1];
                if o.state != 0 { o.pos } else { self.pos }
            }
            _ => self.pos,
        }
    }

    // Shot timer (0x451500, 09 §4.2).
    pub fn update_shot_timer(&mut self, anm: &mut AnmManager, inp: &input::Input) {
        let special = self.bombing && self.bomb.index == 4;
        if self.frames >= 20 && self.shot_timer.cur >= 0 && !special {
            let t = self.shot_timer.cur;
            if t != self.shot_timer.prev && !(self.bombing && matches!(self.shot_type, 1 | 6 | 7)) {
                self.fire(anm, t);
            }
            self.shot_timer.tick(anm.speed);
            if self.shot_timer.cur > 19 {
                self.shot_timer.set(-1);
            }
        }
        if inp.down(input::SHOT) && self.shot_timer.cur < 0 && !self.env.dialogue {
            self.shot_timer.set(0);
        }
        if matches!(self.state, 1 | 2) {
            self.shot_timer.set(-1);
        }
    }

    fn forced_level(&self) -> Option<usize> {
        let typed = (self.shot_type == 2 && self.bomb.index & 1 == 1) || self.shot_type == 9;
        (typed && self.bombing && self.bomb.timer.cur >= 60).then_some(if self.bomb.index & 2 != 0 { 7 } else { 6 })
    }

    // Firing (0x450F60, 09 §4.3).
    pub fn fire(&mut self, anm: &mut AnmManager, t: i32) {
        let sht = if self.focus == 1 { &self.sht_youkai } else { &self.sht };
        let Some(level) = sht.level(self.stats.power, self.forced_level()) else { return };
        let entries = level.entries.clone();
        let mut slot = 0;
        for e in &entries {
            while slot < MAX_BULLETS && self.bullets[slot].state != 0 {
                slot += 1;
            }
            if slot >= MAX_BULLETS {
                break;
            }
            if self.init_bullet(anm, slot, t, e) {
                slot += 1;
            }
        }
    }

    fn spawn_bullet(&mut self, anm: &mut AnmManager, slot: usize, e: &ShotEntry) {
        if e.sound >= 0 {
            crate::sound::play_at(e.sound as i32, self.pos[0]);
        }
        let o = self.origin_pos(e.origin);
        let mut b = PBullet {
            pos: [o[0] + e.offset[0], o[1] + e.offset[1], 0.495],
            size: e.size,
            angle: e.angle,
            speed: e.speed,
            vel: polar(e.angle, e.speed),
            damage: e.damage,
            state: 1,
            kind: e.kind,
            slot: e.phase,
            focus: self.focus,
            script: e.script,
            side: e.side,
            origin: e.origin,
            red: e.side > 0 && self.stats.youkai_max(),
            update: e.update,
            draw: e.draw,
            hit: e.hit,
            ..PBullet::default()
        };
        b.timer.set(0);
        anm.set_script(&mut b.vm, ANM_SLOT, (e.script + 10).max(0) as usize);
        b.vm.flags |= anmvm::F_ZWRITE_OFF;
        self.bullets[slot] = b;
    }

    fn default_init(&mut self, anm: &mut AnmManager, slot: usize, t: i32, e: &ShotEntry) -> bool {
        if e.interval <= 0 || t % e.interval as i32 != e.phase as i32 {
            return false;
        }
        self.spawn_bullet(anm, slot, e);
        true
    }

    fn set_dir(&mut self, slot: usize, angle: f32, speed: f32) {
        let b = &mut self.bullets[slot];
        b.angle = anmvm::wrap_add(angle, 0.0);
        b.vel = polar(b.angle, speed);
    }

    // Init functions (table 0x4C7EE0); true when the slot was used.
    fn init_bullet(&mut self, anm: &mut AnmManager, slot: usize, t: i32, e: &ShotEntry) -> bool {
        match e.init {
            1 => {
                if !self.default_init(anm, slot, t, e) {
                    return false;
                }
                if let Some(a) = self.aimed {
                    let p = self.bullets[slot].pos;
                    let ang = (a[1] - p[1]).atan2(a[0] - p[0]) + e.angle + FRAC_PI_2;
                    self.set_dir(slot, ang, e.speed * 1.5);
                }
                true
            }
            2 | 3 => !self.bombing && self.default_init(anm, slot, t, e),
            // Laser 0x44FE20: one live laser per slot; a different entry (level change) ends the old one.
            4 => {
                let s = (e.phase.max(0) as usize).min(LASER_SLOTS - 1);
                if self.bombing {
                    return false;
                }
                let ls = self.laser_slots[s];
                if let Some(old) = ls.owner {
                    if ls.entry != Some(*e) {
                        self.bullets[old].vm.pending_interrupt = 1;
                        self.laser_slots[s] = LaserSlot::default();
                    }
                    return false;
                }
                self.spawn_bullet(anm, slot, e);
                let b = &mut self.bullets[slot];
                b.slot = s as i16;
                b.trail_n = e.interval as i32;
                b.trail = [[-999.0, 0.0, 0.0]; 32];
                b.pos[0] = -999.0;
                self.laser_slots[s] = LaserSlot { owner: Some(slot), timer: 999, entry: Some(*e) };
                true
            }
            5 => {
                if !self.default_init(anm, slot, t, e) {
                    return false;
                }
                let tg = self.targets[1];
                if tg[0] > -100.0 {
                    let p = self.bullets[slot].pos;
                    self.set_dir(slot, (tg[1] - p[1]).atan2(tg[0] - p[0]), e.speed * 1.5);
                }
                true
            }
            6 => {
                if !self.default_init(anm, slot, t, e) {
                    return false;
                }
                self.set_dir(slot, self.option_angle + e.angle + FRAC_PI_2, e.speed);
                true
            }
            // SPEC-GAP: the random angle is drawn only when the entry actually fires.
            7 => {
                if !self.default_init(anm, slot, t, e) {
                    return false;
                }
                let a = anm.rng.next_signed() * PI / 48.0 - FRAC_PI_2;
                self.set_dir(slot, a, e.speed);
                true
            }
            8 => {
                if self.bombing || !self.default_init(anm, slot, t, e) {
                    return false;
                }
                self.set_dir(slot, self.options[2].facing + e.angle, e.speed);
                true
            }
            _ => self.default_init(anm, slot, t, e),
        }
    }

    // Update functions (table 0x4C7F04); true frees the bullet.
    fn bullet_update_fn(&mut self, anm: &mut AnmManager, i: usize) -> bool {
        let tg = self.targets[0];
        let b = &mut self.bullets[i];
        match b.update {
            1 => {
                if b.state == 1 && tg[0] > -100.0 && b.timer.cur <= 39 && b.timer.cur != b.timer.prev {
                    let d = [tg[0] - b.pos[0], tg[1] - b.pos[1]];
                    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    let k = (len / (b.speed / 4.0)).max(1.0);
                    let v = [b.vel[0] + d[0] / k, b.vel[1] + d[1] / k];
                    let vl = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-6);
                    b.speed = vl.clamp(1.0, 10.0);
                    b.vel = [v[0] * b.speed / vl, v[1] * b.speed / vl];
                } else if b.speed < 10.0 {
                    let old = b.speed.max(1e-6);
                    b.speed += 1.0 / 3.0;
                    b.vel = [b.vel[0] * b.speed / old, b.vel[1] * b.speed / old];
                }
                b.angle = b.vel[1].atan2(b.vel[0]);
                false
            }
            3 => {
                if b.state == 1 {
                    b.vel[1] -= anm.rng.next_f32() * 0.1 + 0.27;
                }
                false
            }
            5 => self.laser_update(i),
            // SPEC-GAP: update 4 (type-4 laser) is not used by any shipped entry; not implemented.
            _ => false,
        }
    }

    // Laser update 5 (0x450840): follows option 0, leaves a 32-point trail of damage rectangles.
    fn laser_update(&mut self, i: usize) -> bool {
        let s = self.bullets[i].slot.clamp(0, LASER_SLOTS as i16 - 1) as usize;
        let owned = self.laser_slots[s].owner == Some(i);
        if !owned || self.env.dialogue || self.shot_timer.cur < 0 || self.state == 2 || self.bombing {
            let b = &mut self.bullets[i];
            b.vm.pending_interrupt = 1;
            b.update = 0;
            if owned {
                self.laser_slots[s] = LaserSlot::default();
            }
            return false;
        }
        if self.options[0].state == 0 {
            self.laser_slots[s] = LaserSlot::default();
            return true;
        }
        let n = self.bullets[i].trail_n.clamp(0, 16) as usize;
        for k in 0..n {
            let p = self.bullets[i].trail[2 * k];
            if p[0] >= -900.0 {
                let a = self.damage_rect([p[0], p[1]], [16.0, 448.0], 1, 0);
                self.damage_areas[a].no_spark = true;
            }
        }
        let opt = self.options[0].pos;
        let white = !self.stats.youkai_max();
        let b = &mut self.bullets[i];
        for k in (1..32).rev() {
            b.trail[k] = b.trail[k - 1];
            b.trail[k][1] -= 1.0;
        }
        b.trail[0] = b.pos;
        b.pos = [opt[0], opt[1] - 208.0, 0.44];
        b.size[1] = 448.0;
        let c = if white { [0xFF; 3] } else { [0xFF, 0xD0, 0xB0] };
        (b.vm.color[anmvm::R], b.vm.color[anmvm::G], b.vm.color[anmvm::B]) = (c[0], c[1], c[2]);
        false
    }

    // Player bullets update (0x451150, 09 §4.4).
    pub fn update_bullets(&mut self, anm: &mut AnmManager) {
        let gs = anm.speed;
        for i in 0..MAX_BULLETS {
            if self.bullets[i].state == 0 {
                continue;
            }
            if self.bullet_update_fn(anm, i) {
                self.bullets[i].state = 0;
                continue;
            }
            let b = &mut self.bullets[i];
            b.pos[0] += b.vel[0] * gs;
            b.pos[1] += b.vel[1] * gs;
            if !matches!(b.kind, 4 | 5) {
                let (hw, hh) = (b.vm.sprite_w / 2.0, b.vm.sprite_h / 2.0);
                let on = b.pos[0] + hw >= 0.0 && b.pos[0] - hw <= 384.0 && b.pos[1] + hh >= 0.0 && b.pos[1] - hh <= 448.0;
                if !on {
                    b.state = 0;
                    continue;
                }
            }
            if b.vm.script.is_some() && b.vm.run(anm) {
                b.state = 0;
                continue;
            }
            if b.vm.script.is_none() {
                b.state = 0;
                continue;
            }
            b.timer.tick(gs);
        }
    }

    // Hit filters (table 0x4C7F24); true skips this hit.
    fn hit_filter(&mut self, anm: &mut AnmManager, i: usize, enemy: [f32; 3]) -> bool {
        let spell = self.env.spell;
        let b = &mut self.bullets[i];
        match b.hit {
            1 => {
                if b.timer.cur % 6 == 0 {
                    crate::effect::spawn(anm, 5, enemy, 1, 0xFFFFFFFF);
                }
                let b = &mut self.bullets[i];
                if b.state == 1 {
                    // SPEC-GAP: "enlarged by sprite (48–128 px)": the hit sprite's larger side clamped to 48..128.
                    let s = b.vm.sprite_w.max(b.vm.sprite_h).clamp(48.0, 128.0);
                    b.size = [s, s];
                    let a = anm.rng.next_f32() * FRAC_PI_2 - 3.0 * PI / 4.0;
                    b.vel = polar(a, 6.0);
                    return false;
                }
                let period = if spell { 4 } else { 2 };
                if b.timer.cur % period != 0 {
                    return true;
                }
                b.damage = (b.damage / 3).max(1);
                b.vel = [b.vel[0] * 0.88, b.vel[1] * 0.88];
                false
            }
            2 => {
                b.hits += 1;
                let x = b.pos[0];
                self.spark_counter = self.spark_counter.wrapping_add(1);
                if self.spark_counter & 7 == 0 {
                    crate::effect::spawn(anm, 5, [x, enemy[1], enemy[2]], 1, 0xFFFFFFFF);
                }
                false
            }
            _ => false,
        }
    }

    // Player_DamageAgainst (0x451670, 09 §4.5); `acc` is the enemy's time-orb accumulator.
    pub fn damage_against(&mut self, anm: &mut AnmManager, items: Option<&mut ItemManager>, pos: [f32; 3], size: [f32; 2], acc: &mut i32, by_bomb: &mut bool) -> i32 {
        if self.state_timer.cur == self.state_timer.prev {
            return 0;
        }
        let half = [size[0] / 2.0, size[1] / 2.0];
        let mut orbs = Vec::new();
        let mut shot = 0;
        for i in 0..MAX_BULLETS {
            let b = &self.bullets[i];
            if b.state == 0 || !(b.state == 1 || b.kind == 3) {
                continue;
            }
            if (b.pos[0] - pos[0]).abs() > half[0] + b.size[0] / 2.0 || (b.pos[1] - pos[1]).abs() > half[1] + b.size[1] / 2.0 {
                continue;
            }
            if matches!(b.kind, 4 | 5) && b.timer.cur % 2 != 0 {
                continue;
            }
            if self.hit_filter(anm, i, pos) {
                continue;
            }
            let d = self.bullets[i].damage as i32;
            shot += if self.bombing { (d / 5).max(1) } else { d };
            while *acc >= self.orb_step {
                *acc -= self.orb_step;
                if self.stats.human_max() && self.bullets[i].side < 0 {
                    orbs.push(self.bullets[i].pos);
                }
            }
            let b = &mut self.bullets[i];
            if !(4..=6).contains(&b.kind) && b.state == 1 {
                b.state = 2;
                let pos = b.pos;
                crate::effect::spawn(anm, 5, pos, 1, 0xFFFFFFFF);
                let b = &mut self.bullets[i];
                let rot = b.vm.rotation[2];
                anm.set_script(&mut b.vm, ANM_SLOT, (b.script + 11).max(0) as usize);
                b.vm.rotation[2] = rot;
                b.vm.flags |= anmvm::F_ROT_DIRTY;
                b.vm.pos = pos;
                b.vm.flags |= anmvm::F_ZWRITE_OFF;
                b.pos[2] = 0.1;
                if b.kind != 3 {
                    b.vel = [b.vel[0] / 8.0, b.vel[1] / 8.0];
                }
            }
        }
        let shot = shot.min(50);
        *acc += shot;
        let mut dmg = shot;
        for a in self.damage_areas.iter_mut().filter(|a| a.active) {
            if a.period > 0 && a.life % a.period != 0 || !a.contains([pos[0], pos[1]], half) {
                continue;
            }
            dmg += a.damage;
            a.total += a.damage;
            if a.max > 0 && a.total >= a.max {
                dmg -= a.total - a.max;
                a.total = a.max;
                a.damage = 0;
            }
            if self.bombing {
                *by_bomb = true;
            }
            if !a.no_spark {
                self.spark_counter = self.spark_counter.wrapping_add(1);
                if self.spark_counter & 3 == 0 {
                    crate::effect::spawn(anm, 3, pos, 1, 0xFFFFFFFF);
                }
            }
        }
        if self.stats.youkai_max() {
            dmg = dmg * 106 / 100;
        }
        if let Some(items) = items {
            for p in orbs {
                items.spawn(anm, self, p, crate::item::TIME_ORB as i32, 0);
            }
        }
        dmg
    }

    // Player draw (14 §5): flying bullets (state 1) at z 0.4 with their extra draw fn, or hit bullets (state 2) at z 0.2.
    pub fn draw_bullets(&mut self, anm: &mut AnmManager, gfx: &mut Gfx, state: u8) {
        let youkai_max = self.stats.youkai_max();
        for b in self.bullets.iter_mut().filter(|b| b.state == state && b.vm.script.is_some()) {
            if b.vm.tag != 0 && b.vm.rotation[2] != b.angle {
                b.vm.rotation[2] = b.angle;
                b.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            if b.red {
                (b.vm.color[anmvm::R], b.vm.color[anmvm::G], b.vm.color[anmvm::B]) = (0xFF, 0x40, 0x40);
            }
            b.vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.0];
            if state == 2 {
                let mode = b.vm.draw_mode;
                anm.draw_mode_z(gfx, &mut b.vm, mode, 0.2);
                continue;
            }
            anm.draw_mode_z(gfx, &mut b.vm, 2, 0.4);
            if b.draw == 1 {
                Self::draw_trail(anm, gfx, b, youkai_max);
            }
        }
    }

    // Trail draw 0x450AD0: every second trail point, alpha fading from 3/4 of the VM alpha.
    fn draw_trail(anm: &mut AnmManager, gfx: &mut Gfx, b: &mut PBullet, youkai_max: bool) {
        let n = b.trail_n.max(1);
        let (alpha, colour) = (b.vm.color[anmvm::A], b.vm.color);
        let a0 = alpha as i32 * 3 / 4;
        let mut c = 0;
        while c < 2 * n && c < 32 && b.trail[c as usize][0] != -999.0 {
            let p = b.trail[c as usize];
            b.vm.color[anmvm::A] = if c == 0 { alpha } else { (a0 - (a0 / 2) * c / n).clamp(0, 255) as u8 };
            if youkai_max {
                (b.vm.color[anmvm::R], b.vm.color[anmvm::G], b.vm.color[anmvm::B]) = (0xFF, 0x40, 0x40);
            }
            b.vm.pos = [p[0] + 32.0, p[1] + 16.0, 0.0];
            anm.draw_mode_z(gfx, &mut b.vm, 2, 0.4);
            c += 2;
        }
        b.vm.color = colour;
    }
}
