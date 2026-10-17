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
// SPEC-GAP: option sprite scripts are not given; the last script of each playerNN.anm texture 0 (they carry label 5).
pub const OPTION_SCRIPT: [usize; 4] = [18, 29, 24, 21];

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
}

#[derive(Clone, Debug, Default)]
pub struct Opt {
    pub vm: AnmVm,
    pub pos: [f32; 3],
    pub anchor: [f32; 3],
    pub state: u8,
    pub index: usize,
    pub angle: f32,
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
}

const REMILIA_ANCHOR: [[f32; 2]; 4] = [[-30.0, -16.0], [-10.0, -32.0], [10.0, -32.0], [30.0, -16.0]];
const REMILIA_SPIN: [f32; 4] = [PI / 120.0, -PI / 90.0, PI / 90.0, -PI / 120.0];

impl Player {
    fn youkai_set(&self) -> (OptKind, usize) {
        match crate::sht::team(self.shot_type) {
            0 => (OptKind::Yukari, 1),
            1 => (OptKind::Alice, 1),
            2 => (OptKind::Remilia, 4),
            _ => (OptKind::Yuyuko, 2),
        }
    }

    fn add_option(&mut self, anm: &mut AnmManager, k: usize, kind: OptKind) {
        let mut o = Opt { kind, index: k, state: 2, pos: self.pos, anchor: self.pos, ..Opt::default() };
        anm.set_script(&mut o.vm, ANM_SLOT, self.option_script);
        match kind {
            OptKind::Remilia => o.anchor = [REMILIA_ANCHOR[k][0], REMILIA_ANCHOR[k][1], 0.0],
            OptKind::Yuyuko => o.angle = if k == 0 { 0.0 } else { -PI },
            OptKind::YoumuHalf => o.angle = -FRAC_PI_2,
            _ => {}
        }
        self.options[k] = o;
        self.place_option(k);
    }

    fn remove_option(&mut self, k: usize) {
        let o = &mut self.options[k];
        if o.state != 0 && o.state != 3 {
            o.state = 3;
            o.timer.set(0);
        }
    }

    // Solos get their set at load (09 §5).
    pub fn spawn_options(&mut self, anm: &mut AnmManager, _solo: bool) {
        match self.shot_type {
            5 => self.add_option(anm, 0, OptKind::Yukari),
            7 => self.add_option(anm, 0, OptKind::Alice),
            // SPEC-GAP: Remilia's solo options (0x44EE70) are not detailed; the team behaviour is used.
            9 => (0..4).for_each(|k| self.add_option(anm, k, OptKind::Remilia)),
            11 => (0..2).for_each(|k| self.add_option(anm, k, OptKind::Yuyuko)),
            10 => self.add_option(anm, 2, OptKind::YoumuHalf),
            _ => {}
        }
    }

    // Teams: youkai set while focused, team 3 gets Youmu's half (option 2) while unfocused.
    pub fn swap_options(&mut self, anm: &mut AnmManager, f: u8) {
        let (kind, n) = self.youkai_set();
        let team3 = self.shot_type == 3;
        if f == 1 {
            if team3 {
                self.remove_option(2);
            }
            (0..n).for_each(|k| self.add_option(anm, k, kind));
        } else {
            (0..n).for_each(|k| self.remove_option(k));
            if team3 {
                self.add_option(anm, 2, OptKind::YoumuHalf);
            }
        }
    }

    // Option calc functions (09 §5); positions in playfield space.
    fn place_option(&mut self, k: usize) {
        let (px, py) = (self.pos[0], self.pos[1]);
        let bombing = self.bombing;
        let target = self.aimed;
        let o = &mut self.options[k];
        let t = o.timer.cur;
        match o.kind {
            // SPEC-GAP: the height above the player (C) and the attack helpers are not given; 48 px, no attack.
            OptKind::Yukari => {
                let want = [px, (py - 48.0).max(32.0)];
                let _ = target;
                o.pos = [o.pos[0] + (want[0] - o.pos[0]) * 0.3, o.pos[1] + (want[1] - o.pos[1]) * 0.3, 0.0];
            }
            OptKind::Alice => {
                if !bombing {
                    o.pos = [px, py - 32.0, 0.0];
                }
            }
            // SPEC-GAP: the anchors are kept relative to the player (09 says "fixed at spawn").
            OptKind::Remilia => {
                if t > 12 {
                    o.angle += REMILIA_SPIN[k];
                }
                let d = polar(o.angle, 8.0);
                o.pos = [px + o.anchor[0] + d[0], py + o.anchor[1] + d[1], 0.0];
            }
            OptKind::Yuyuko => {
                let tx = if k == 0 { px - 32.0 } else { px + 32.0 };
                o.anchor[0] += (tx - o.anchor[0]) * 0.09;
                o.anchor[1] += (py - o.anchor[1]) * 0.09;
                o.angle += if k == 0 { PI / 60.0 } else { -PI / 60.0 };
                let d = polar(o.angle, 6.0);
                o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
            }
            // SPEC-GAP: Youmu's half (0x44F2D0 / 0x44F5E0) is not detailed; it trails 24 px below the player.
            OptKind::YoumuHalf => {
                o.anchor[0] += (px - o.anchor[0]) * 0.15;
                o.anchor[1] += (py + 24.0 - o.anchor[1]) * 0.15;
                o.pos = o.anchor;
            }
            OptKind::None => {}
        }
    }

    pub fn update_options(&mut self, anm: &mut AnmManager) {
        let gs = anm.speed;
        for k in 0..4 {
            match self.options[k].state {
                0 => continue,
                3 => {
                    if self.options[k].timer.cur == 0 {
                        self.options[k].vm.pending_interrupt = 5;
                    }
                    if self.options[k].timer.cur > 16 {
                        self.options[k] = Opt::default();
                        continue;
                    }
                }
                _ => {}
            }
            self.place_option(k);
            let o = &mut self.options[k];
            if o.vm.script.is_some() {
                o.vm.run(anm);
            }
            o.timer.tick(gs);
        }
    }

    pub fn draw_options(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for o in self.options.iter_mut().filter(|o| o.state != 0 && o.vm.script.is_some()) {
            o.vm.pos = [o.pos[0] + 32.0, o.pos[1] + 16.0, 0.0];
            anm.draw(gfx, &mut o.vm);
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
            4 => {
                let s = (e.phase as usize).min(3);
                if self.bombing || self.laser_slots[s].is_some_and(|i| self.bullets[i].state != 0) {
                    return false;
                }
                self.spawn_bullet(anm, slot, e);
                self.laser_slots[s] = Some(slot);
                self.place_laser(slot);
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
                self.set_dir(slot, self.options[2].angle + e.angle, e.speed);
                true
            }
            _ => self.default_init(anm, slot, t, e),
        }
    }

    // SPEC-GAP: laser body (update 4/5) details are not given: the beam hitbox spans from its origin to the top.
    fn place_laser(&mut self, i: usize) {
        let o = self.origin_pos(self.bullets[i].origin);
        let b = &mut self.bullets[i];
        let len = o[1].max(1.0);
        b.pos = [o[0], o[1] / 2.0, 0.495];
        b.size[1] = len;
        b.vm.pos_offset = [0.0, 0.0, 0.0];
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
            4 | 5 => {
                let ending = self.shot_timer.cur < 0 || self.bombing || matches!(self.state, 1 | 2);
                if b.state == 1 && ending {
                    b.state = 2;
                    b.vm.pending_interrupt = 1;
                }
                self.place_laser(i);
                false
            }
            _ => false,
        }
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
            let laser = matches!(b.kind, 4 | 5);
            if !laser {
                b.pos[0] += b.vel[0] * gs;
                b.pos[1] += b.vel[1] * gs;
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
        for s in &mut self.laser_slots {
            if s.is_some_and(|i| self.bullets[i].state == 0) {
                *s = None;
            }
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
                anm.set_script(&mut b.vm, ANM_SLOT, (b.script + 11).max(0) as usize);
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

    pub fn draw_bullets(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for b in self.bullets.iter_mut().filter(|b| b.state != 0 && b.vm.script.is_some()) {
            b.vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.0];
            // SPEC-GAP: owner use of the VM tag (op 25) is inferred: tag ≠ 0 shot sprites face their direction.
            if b.state == 1 && b.vm.tag != 0 && b.vm.rotation[2] != b.angle {
                b.vm.rotation[2] = b.angle;
                b.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            // SPEC-GAP: the "red tint" colour of youkai-side shots at youkai max is not given.
            if b.red {
                b.vm.color[anmvm::G] = 0x60;
                b.vm.color[anmvm::B] = 0x60;
            }
            if matches!(b.kind, 4 | 5) {
                b.vm.pos[1] = b.pos[1] * 2.0 + 16.0 - 224.0;
            }
            anm.draw(gfx, &mut b.vm);
        }
    }
}
