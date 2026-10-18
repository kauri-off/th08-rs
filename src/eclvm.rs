use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::bullet::{BulletDesc, ExCmd, LaserDesc, angle_from};
use crate::ecl::Ins;
use crate::enemy::*;
use std::f32::consts::PI;

enum Flow {
    Next,
    Jumped,
    Post,
    Interrupt,
    Delete,
    ThreadEnd,
}

#[derive(PartialEq)]
enum End {
    Post,
    Delete,
    Finished,
}

// Ecl_Run (07 §5.2). The enemy is taken out of the manager while it runs; returns −1 when op 1 executed.
pub fn ecl_run(m: &mut EnemyManager, anm: &mut AnmManager, idx: usize) -> i32 {
    let mut e = m.take(idx);
    let r = run_enemy(m, anm, &mut e, idx);
    m.put(idx, e);
    r
}

fn run_enemy(m: &mut EnemyManager, anm: &mut AnmManager, e: &mut Enemy, idx: usize) -> i32 {
    let mut main = std::mem::take(&mut e.main);
    let r = Exec { m, anm, e, idx }.run_thread(&mut main);
    e.main = main;
    if r == End::Delete {
        return -1;
    }
    for k in 0..4 {
        let Some(mut t) = e.threads[k].take() else {
            continue;
        };
        e.running_thread = Some(k);
        e.kill_running_thread = false;
        let r = Exec { m, anm, e, idx }.run_thread(&mut t);
        e.running_thread = None;
        if r == End::Delete {
            return -1;
        }
        if r != End::Finished && !e.kill_running_thread && e.threads[k].is_none() {
            e.threads[k] = Some(t);
        }
    }
    e.update_move_mode(anm.speed);
    let mut main = std::mem::take(&mut e.main);
    Exec { m, anm, e, idx }.autofire_and_dir_anim(&mut main);
    e.main = main;
    0
}

fn ease(mode: i32, t: f32) -> f32 {
    anmvm::ease(mode as u8, t)
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) struct Exec<'a> {
    m: &'a mut EnemyManager,
    anm: &'a mut AnmManager,
    e: &'a mut Enemy,
    idx: usize,
}

enum IRef {
    Ctx(usize),
    EInt(usize),
    Difficulty,
    Rank,
    F2e1c,
    Life,
    GArg(usize),
    Item,
    Score,
}

enum FRef {
    Ctx(usize),
    EFlt(usize),
    Pos(usize),
    Player(usize),
    GArg(usize),
    Angle,
    AngVel,
    Speed,
    Accel,
    Radius,
    Start(usize),
    OrbitAngle,
    OrbitAv,
    Delta(usize),
}

fn int_ref(id: i32) -> Option<IRef> {
    Some(match id {
        10000..=10007 => IRef::Ctx(V_INT0 + (id - 10000) as usize),
        10008..=10015 => IRef::EInt((id - 10008) as usize),
        10036..=10039 => IRef::Ctx(V_INT1 + (id - 10036) as usize),
        10040 => IRef::Difficulty,
        10041 => IRef::Rank,
        10049 => IRef::F2e1c,
        10051 => IRef::Life,
        10053..=10056 => IRef::Ctx(V_ARGI + (id - 10053) as usize),
        10061..=10064 => IRef::GArg((id - 10061) as usize),
        10092 => IRef::Item,
        10093 => IRef::Score,
        _ => return None,
    })
}

fn flt_ref(id: i32) -> Option<FRef> {
    Some(match id {
        10016..=10023 => FRef::Ctx(V_FLT0 + (id - 10016) as usize),
        10024..=10031 => FRef::EFlt((id - 10024) as usize),
        10042..=10044 => FRef::Pos((id - 10042) as usize),
        10045..=10047 => FRef::Player((id - 10045) as usize),
        10057..=10060 => FRef::Ctx(V_ARGF + (id - 10057) as usize),
        10065..=10068 => FRef::GArg(4 + (id - 10065) as usize),
        10069 => FRef::Angle,
        10070 => FRef::AngVel,
        10071 => FRef::Speed,
        10072 => FRef::Accel,
        10073 => FRef::Radius,
        10074..=10076 => FRef::Start((id - 10074) as usize),
        10077 => FRef::OrbitAngle,
        10078 => FRef::OrbitAv,
        10079..=10081 => FRef::Delta((id - 10079) as usize),
        10094 | 10095 => FRef::Ctx(V_FLT1 + (id - 10094) as usize),
        _ => return None,
    })
}

// Variable getters (07 §6) evaluated on any enemy/context (also used for boss queries, ops 86/87).
pub(crate) fn var_f(
    m: &mut EnemyManager,
    anm: &mut AnmManager,
    e: &Enemy,
    c: &Context,
    lit: f32,
) -> f32 {
    let id = lit as i32;
    if let Some(r) = flt_ref(id) {
        return read_f(m, e, c, &r);
    }
    if let Some(r) = int_ref(id) {
        return read_i(m, e, c, &r) as f32;
    }
    let rng = &mut anm.rng;
    match id {
        10032 => (rng.next_u32() & 0x7FFF_FFFF) as f32,
        10033 => rng.next_f32(),
        10034 => rng.next_u32() as i32 as f32,
        10035 => rng.next_signed(),
        10048 => angle_from(e.final_pos, m.g.player),
        10050 => dist(e.final_pos, m.g.player),
        10052 => m.g.player_char as f32,
        10082 => rng.next_f32() * 2.0 * PI - PI,
        10083 => e.last_damage as f32,
        10084 => e.boss_slot as f32,
        10085..=10087 => e.displacement[(id - 10085) as usize],
        10088..=10091 => e.thresholds[(id - 10088) as usize] as f32,
        10096 => chain_depth(m, e) as f32,
        10097 => m.g.player_mode as f32,
        10099 => m.g.spell.is_some() as i32 as f32,
        _ => lit,
    }
}

pub(crate) fn var_i(
    m: &mut EnemyManager,
    anm: &mut AnmManager,
    e: &Enemy,
    c: &Context,
    id: i32,
) -> i32 {
    if let Some(r) = int_ref(id) {
        return read_i(m, e, c, &r);
    }
    match id {
        // SPEC-GAP: whether an int read of 10033 consumes a random number is not stated (assumed not).
        10033 => 0,
        // SPEC-GAP: 10098 compares game counters of a later phase; reads 0.
        10098 => 0,
        10100 => m.g.spell.as_ref().map_or(0, |s| s.frames),
        10079..=10082 => id,
        _ if flt_ref(id).is_some()
            || matches!(
                id,
                10032 | 10034 | 10035 | 10048 | 10050 | 10052 | 10083
                    ..=10091 | 10096 | 10097 | 10099
            ) =>
        {
            let v = var_f(m, anm, e, c, id as f32);
            match id {
                10032 | 10034 => v as i64 as i32,
                _ => v as i32,
            }
        }
        _ => id,
    }
}

fn read_i(m: &EnemyManager, e: &Enemy, c: &Context, r: &IRef) -> i32 {
    match *r {
        IRef::Ctx(k) => c.int(k),
        IRef::EInt(k) => e.ivars[k],
        IRef::Difficulty => m.g.difficulty,
        IRef::Rank => crate::gamemgr::rank(),
        IRef::F2e1c => e.f2e1c,
        IRef::Life => e.life,
        IRef::GArg(k) => m.g.args[k] as i32,
        IRef::Item => e.item,
        IRef::Score => e.score,
    }
}

fn read_f(m: &EnemyManager, e: &Enemy, c: &Context, r: &FRef) -> f32 {
    match *r {
        FRef::Ctx(k) => c.flt(k),
        FRef::EFlt(k) => e.fvars[k],
        FRef::Pos(k) => e.pos[k],
        FRef::Player(k) => m.g.player[k],
        FRef::GArg(k) => f32::from_bits(m.g.args[k]),
        FRef::Angle => e.angle,
        FRef::AngVel => e.angvel,
        FRef::Speed => e.speed,
        FRef::Accel => e.accel,
        FRef::Radius => e.radius,
        FRef::Start(k) => e.interp_start[k],
        FRef::OrbitAngle => e.orbit_angle,
        FRef::OrbitAv => e.orbit_av,
        FRef::Delta(k) => e.interp_delta[k],
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn chain_depth(m: &EnemyManager, e: &Enemy) -> i32 {
    let mut d = 0;
    let mut p = e.chain_prev;
    while let Some(i) = p {
        d += 1;
        if d > MAX_ENEMIES as i32 {
            break;
        }
        p = m.enemies[i].chain_prev;
    }
    d
}

fn lerp_i(a: i32, b: i32, rank: i32) -> i32 {
    a + ((b - a) * rank) / 32
}

fn lerp_f(a: f32, b: f32, rank: i32) -> f32 {
    a + (b - a) * rank as f32 / 32.0
}

fn rd_u32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn rd_i16(b: &[u8], o: usize) -> i16 {
    b.get(o..o + 2)
        .map_or(0, |s| i16::from_le_bytes(s.try_into().unwrap()))
}

impl Exec<'_> {
    fn run_thread(&mut self, t: &mut Thread) -> End {
        let gs = self.anm.speed;
        'outer: loop {
            if self.e.pending_interrupt >= 0 && self.e.running_thread.is_none() {
                self.interrupt(t);
                continue;
            }
            loop {
                self.e.final_pos = add3(self.e.pos, self.e.pos_offset);
                if t.ctx.wait.cur > 0 {
                    t.ctx.wait.dec(gs);
                    t.ctx.clock.dec(gs);
                    break 'outer;
                }
                let Some(ins) = self.m.ecl.ins(t.ctx.ip) else {
                    break 'outer;
                };
                if ins.op == -1 || t.ctx.clock.cur != ins.time {
                    break 'outer;
                }
                let d = self.m.rank_mask(self.e.mode_bits);
                if ins.rank & d != d {
                    t.ctx.ip += ins.size;
                    continue;
                }
                match self.exec(t, &ins) {
                    Flow::Next => t.ctx.ip += ins.size,
                    Flow::Jumped => {}
                    Flow::Post => break 'outer,
                    Flow::Interrupt => continue 'outer,
                    Flow::Delete => return End::Delete,
                    Flow::ThreadEnd => return End::Finished,
                }
            }
        }
        if self.e.life > 0 {
            if t.ctx.callback >= 0 {
                let (n, arg) = (t.ctx.callback, t.ctx.callback_arg);
                self.native(t, n, arg);
            }
            self.run_interp(t);
        }
        t.ctx.clock.tick(gs);
        End::Post
    }

    // 07 §5.5: the pending (not yet executed) instruction is skipped when the interrupt returns.
    fn interrupt(&mut self, t: &mut Thread) {
        let k = self.e.pending_interrupt;
        self.e.pending_interrupt = -1;
        let sub = self.e.interrupts.get(k as usize).copied().unwrap_or(-1);
        // SPEC-GAP: an interrupt index without a registered sub is ignored.
        if self.m.ecl.sub_start(sub).is_none() {
            return;
        }
        let size = self.m.ecl.ins(t.ctx.ip).map_or(0, |i| i.size);
        t.ctx.ip += size;
        if self.e.flags & A_NO_PUSH == 0 {
            t.push();
        } else {
            t.depth = (t.depth + 1).min(15);
        }
        t.ctx.enter(&self.m.ecl, sub);
    }

    fn call(&mut self, t: &mut Thread, next_ip: usize, sub: i32) {
        t.ctx.ip = next_ip;
        if self.e.flags & A_NO_PUSH == 0 {
            t.push();
        }
        t.ctx.enter(&self.m.ecl, sub);
        for k in 0..8 {
            t.ctx.vars[V_ARGI + k] = self.m.g.args[k];
        }
    }

    // --- arguments ---
    fn raw(&self, ins: &Ins, k: usize) -> u32 {
        self.m.ecl.arg(ins, k)
    }

    fn ai(&mut self, t: &Thread, ins: &Ins, k: usize) -> i32 {
        let raw = self.raw(ins, k) as i32;
        if ins.mask & (1 << k) != 0 {
            var_i(self.m, self.anm, self.e, &t.ctx, raw)
        } else {
            raw
        }
    }

    fn af(&mut self, t: &Thread, ins: &Ins, k: usize) -> f32 {
        let raw = f32::from_bits(self.raw(ins, k));
        if ins.mask & (1 << k) != 0 {
            var_f(self.m, self.anm, self.e, &t.ctx, raw)
        } else {
            raw
        }
    }

    fn vi(&mut self, t: &Thread, id: i32, var: bool) -> i32 {
        if var {
            var_i(self.m, self.anm, self.e, &t.ctx, id)
        } else {
            id
        }
    }

    fn vf(&mut self, t: &Thread, v: f32, var: bool) -> f32 {
        if var {
            var_f(self.m, self.anm, self.e, &t.ctx, v)
        } else {
            v
        }
    }

    fn write_i(&mut self, t: &mut Thread, r: IRef, v: i32) {
        match r {
            IRef::Ctx(k) => t.ctx.set_int(k, v),
            IRef::EInt(k) => self.e.ivars[k] = v,
            IRef::Difficulty => self.m.g.difficulty = v,
            IRef::Rank => crate::gamemgr::set_rank(v),
            IRef::F2e1c => self.e.f2e1c = v,
            IRef::Life => self.e.life = v,
            IRef::GArg(k) => self.m.g.args[k] = v as u32,
            IRef::Item => self.e.item = v,
            IRef::Score => self.e.score = v,
        }
    }

    fn write_f(&mut self, t: &mut Thread, r: FRef, v: f32) {
        let e = &mut *self.e;
        match r {
            FRef::Ctx(k) => t.ctx.set_flt(k, v),
            FRef::EFlt(k) => e.fvars[k] = v,
            FRef::Pos(k) => e.pos[k] = v,
            FRef::Player(k) => self.m.g.player[k] = v,
            FRef::GArg(k) => self.m.g.args[k] = v.to_bits(),
            FRef::Angle => e.angle = v,
            FRef::AngVel => e.angvel = v,
            FRef::Speed => e.speed = v,
            FRef::Accel => e.accel = v,
            FRef::Radius => e.radius = v,
            FRef::Start(k) => e.interp_start[k] = v,
            FRef::OrbitAngle => e.orbit_angle = v,
            FRef::OrbitAv => e.orbit_av = v,
            FRef::Delta(k) => e.interp_delta[k] = v,
        }
    }

    // Destination write; anything but a writable variable of the right type writes into the file (07 §6).
    fn set_i(&mut self, t: &mut Thread, ins: &Ins, k: usize, v: i32) {
        let raw = self.raw(ins, k) as i32;
        match int_ref(raw).filter(|_| ins.mask & (1 << k) != 0) {
            Some(r) => self.write_i(t, r, v),
            None => self.m.ecl.set_arg(ins, k, v as u32),
        }
    }

    fn set_f(&mut self, t: &mut Thread, ins: &Ins, k: usize, v: f32) {
        let raw = f32::from_bits(self.raw(ins, k)) as i32;
        match flt_ref(raw).filter(|_| ins.mask & (1 << k) != 0) {
            Some(r) => self.write_f(t, r, v),
            None => self.m.ecl.set_arg(ins, k, v.to_bits()),
        }
    }

    // Interpolation targets: float variables, else int variables (truncated).
    fn set_var_any(&mut self, t: &mut Thread, id: i32, v: f32) {
        if let Some(r) = flt_ref(id) {
            self.write_f(t, r, v);
        } else if let Some(r) = int_ref(id) {
            self.write_i(t, r, v as i32);
        }
    }

    fn jump(&mut self, t: &mut Thread, ins: &Ins, tk: usize) -> Flow {
        let time = self.ai(t, ins, tk);
        let off = self.ai(t, ins, tk + 1);
        t.ctx.clock.set(time);
        t.ctx.ip = (ins.off as i64 + off as i64) as usize;
        Flow::Jumped
    }

    fn player_angle(&self, p: [f32; 3]) -> f32 {
        angle_from(p, self.m.g.player)
    }

    // --- movement helpers (07 §8) ---
    fn mode1(&mut self, ang: f32, spd: f32, dur: i32) {
        let e = &mut *self.e;
        e.angle = anmvm::wrap_add(ang, 0.0);
        e.speed = spd;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.set_move_mode(1);
    }

    fn mode2(&mut self, dur: i32, ease: i32, start: [f32; 3], delta: [f32; 3]) {
        let e = &mut *self.e;
        e.interp_start = start;
        e.interp_delta = delta;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.vel = [0.0; 3];
        e.set_ease(ease);
        e.set_move_mode(2);
    }

    fn move_dir(&mut self, dur: i32, ease: i32, ang: f32, spd: f32) {
        if dur < 1 {
            self.mode1(ang, spd, 0);
        } else {
            let d = spd * dur as f32;
            let delta = [ang.cos() * d, ang.sin() * d, 0.0];
            self.mode2(dur, ease, self.e.final_pos, delta);
        }
    }

    fn mode3(&mut self, dur: i32) {
        let e = &mut *self.e;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.set_move_mode(3);
    }

    // Random direction 0x422020 (op 67) / 0x4224A0 (op 178), 07 §8.3.
    fn random_dir(&mut self, op178: bool) -> f32 {
        let rng = &mut self.anm.rng;
        let e = &*self.e;
        let (x, y) = (e.pos[0], e.pos[1]);
        let px = self.m.g.player[0];
        let [x0, y0, x1, y1] = e.clamp;
        let wrap = |a: f32| anmvm::wrap_add(a, 0.0);
        let mut a;
        if op178 {
            if rng.range(4) == 0 {
                a = rng.next_signed() * PI;
            } else if x <= px {
                a = if x - (px - 384.0) <= px - x {
                    wrap(rng.next_f32() * (PI / 2.0) + 3.0 * PI / 4.0)
                } else {
                    rng.next_f32() * (PI / 2.0) - PI / 4.0
                };
            } else {
                a = if (px + 384.0) - x <= x - px {
                    wrap(rng.next_f32() * (PI / 2.0) - PI / 4.0)
                } else {
                    wrap(rng.next_f32() * (PI / 2.0) + 3.0 * PI / 4.0)
                };
            }
        } else {
            a = if x <= px {
                rng.next_f32() * (PI / 2.0) - PI / 4.0
            } else {
                wrap(rng.next_f32() * (PI / 2.0) + 3.0 * PI / 4.0)
            };
            if x < x0 + 96.0 {
                if a > PI / 2.0 {
                    a = PI - a;
                } else if a < -PI / 2.0 {
                    a = -PI - a;
                }
            }
            if x > x1 - 96.0 {
                if (0.0..PI / 2.0).contains(&a) {
                    a = PI - e.angle;
                } else if a > -PI / 2.0 && a < 0.0 {
                    a = -PI - a;
                }
            }
        }
        if y < y0 + 48.0 && a < 0.0 || y > y1 - 48.0 && a > 0.0 {
            a = -a;
        }
        a
    }

    // --- ANM ---
    fn start_main(&mut self, script: i32) {
        let slot = self.e.anm_slot();
        if script < 0 {
            self.e.vm = AnmVm::default();
            return;
        }
        self.anm.set_script(&mut self.e.vm, slot, script as usize);
    }

    fn start_sub(&mut self, k: usize, script: i32) {
        let slot = self.e.anm_slot();
        let Some(vm) = self.e.sub_vms.get_mut(k) else {
            return;
        };
        if script < 0 {
            *vm = AnmVm::default();
        } else {
            self.anm.set_script(vm, slot, script as usize);
        }
    }

    // --- spawning (ops 90–94, 07 §4.1 / §10.5) ---
    fn spawn_child(&mut self, t: &mut Thread, ins: &Ins) {
        let op = ins.op;
        let sub = self.ai(t, ins, 0);
        let (x, y) = (self.af(t, ins, 1), self.af(t, ins, 2));
        let (z, base) = if op >= 93 {
            (self.af(t, ins, 3), 4)
        } else {
            (0.0, 3)
        };
        let life = self.ai(t, ins, base);
        let item = self.ai(t, ins, base + 1);
        let score = self.ai(t, ins, base + 2);
        let chained = op <= 92;
        if chained && (self.e.life < 1 || self.e.flags & A_CHAIN_KILLED != 0) {
            self.m.full = true;
            return;
        }
        // SPEC-GAP: op 92 places the child at (x, y) relative to its follow offset (parent pos), not added twice.
        let pos = match op {
            91 => [self.e.final_pos[0] + x, self.e.final_pos[1] + y, 0.0],
            94 => [self.e.pos[0] + x, self.e.pos[1] + y, self.e.pos[2] + z],
            93 => [x, y, z],
            _ => [x, y, 0.0],
        };
        let Some(ci) = self.m.spawn_prepare(sub, pos, life, false) else {
            return;
        };
        if chained {
            crate::sound::play_at(crate::sound::OPTION, self.e.final_pos[0]);
        }
        let vars = t.ctx.vars;
        let youkai = self.m.g.player_mode != 0;
        {
            let c = &mut self.m.enemies[ci];
            c.main.ctx.vars = vars;
            if chained {
                c.root = Some(self.idx);
                c.flags |= A_MODE_REACT;
                c.set_flag(A_YOUKAI, youkai);
                c.mode_bits = if youkai { 0x40 } else { 0x20 };
                c.layer = if youkai { 2 } else { 0 };
                c.flags &= !A_CONTACT;
                if op == 92 {
                    c.pos_offset = self.e.pos;
                    c.flags |= A_FOLLOW;
                    c.final_pos = add3(c.pos, c.pos_offset);
                }
            }
        }
        if chained {
            // Child aura 0x20 (07 §10.5). SPEC-GAP: interrupt 1 = youkai, 2 = human is a guess.
            let fp = self.m.enemies[ci].final_pos;
            let fx = crate::effect::spawn_high(self.anm, 0x20, fp, 0xFFFFFFFF);
            crate::effect::get(fx, |x| x.vm.pending_interrupt = if youkai { 1 } else { 2 });
            self.m.enemies[ci].fx_aura = fx;
        }
        if chained {
            let mut last: Option<usize> = None;
            let mut n = self.e.chain_next;
            let mut guard = 0;
            while let Some(i) = n {
                guard += 1;
                if guard > MAX_ENEMIES || i == ci {
                    break;
                }
                last = Some(i);
                n = self.m.enemies[i].chain_next;
            }
            match last {
                Some(l) => {
                    self.m.enemies[l].chain_next = Some(ci);
                    self.m.enemies[ci].chain_prev = Some(l);
                }
                None => {
                    self.e.chain_next = Some(ci);
                    self.m.enemies[ci].chain_prev = Some(self.idx);
                }
            }
            self.e.children += 1;
        }
        self.m.spawn_finish(self.anm, ci, life, item, score, true);
    }

    // --- bullets (07 §9) ---
    fn decode_bullet(&mut self, t: &Thread, b: &[u8]) -> BulletDesc {
        let mask = u16::from_le_bytes([b[10], b[11]]);
        let mb = |k: u32| mask & (1 << k) != 0;
        let mut d = self.e.desc.clone();
        d.kind = self.vi(t, rd_i16(b, 0x0C) as i32, mb(0)) as i16;
        d.colour = self.vi(t, rd_i16(b, 0x0E) as i32, mb(1)) as i16;
        d.count1 = self.vi(t, rd_u32(b, 0x10) as i32, mb(2)) as i16;
        d.count2 = self.vi(t, rd_u32(b, 0x14) as i32, mb(3)) as i16;
        d.speed1 = self.vf(t, f32::from_bits(rd_u32(b, 0x18)), mb(4));
        d.speed2 = self.vf(t, f32::from_bits(rd_u32(b, 0x1C)), mb(5));
        d.angle1 = self.vf(t, f32::from_bits(rd_u32(b, 0x20)), mb(6));
        d.angle2 = self.vf(t, f32::from_bits(rd_u32(b, 0x24)), mb(7));
        d.flags = rd_u32(b, 0x28);
        d.aim_mode = rd_i16(b, 4) - 96;
        d
    }

    fn bullet_op(&mut self, t: &Thread, ins: &Ins) {
        if self.e.life <= 0 {
            return;
        }
        let bytes = self.m.ecl.bytes(ins).to_vec();
        if self.e.flags & A_STORE_FIRE != 0 {
            self.e.stored_fire = Some(bytes);
            return;
        }
        self.e.desc = self.decode_bullet(t, &bytes);
        self.fire_desc();
    }

    // Filters, origin and rank scaling (07 §9.1–9.2), then BulletMgr_SpawnPattern.
    fn fire_desc(&mut self) {
        let e = &*self.e;
        let mut d = e.desc.clone();
        let youkai = self.m.g.player_mode != 0;
        if (d.flags & 0x8000 != 0 && !youkai) || (d.flags & 0x10000 != 0 && youkai) {
            return;
        }
        let p = self.m.g.player;
        let dd = (e.final_pos[0] - p[0]).powi(2) + (e.final_pos[1] - p[1]).powi(2);
        if e.nofire_r2 > 0.0 && dd < e.nofire_r2 {
            return;
        }
        d.origin = add3(e.final_pos, e.shot_offset);
        if self.m.g.spell.is_none() {
            let rank = crate::gamemgr::rank();
            let [c1a, c1b, c2a, c2b] = e.rank_counts;
            let [s0, s1] = e.rank_speed;
            d.count1 = (d.count1 as i32 + lerp_i(c1a, c1b, rank)).max(1) as i16;
            d.count2 = (d.count2 as i32 + lerp_i(c2a, c2b, rank)).max(1) as i16;
            if d.speed1 != 0.0 {
                let s = lerp_f(s0, s1, rank);
                d.speed1 = (d.speed1 + s).max(0.3);
                d.speed2 = (d.speed2 + s / 2.0).max(0.3);
            }
        }
        let p = self.m.g.player;
        self.m.bullets.borrow_mut().spawn_pattern(self.anm, &d, p);
    }

    fn laser_op(&mut self, t: &Thread, ins: &Ins) {
        let b = self.m.ecl.bytes(ins).to_vec();
        let mask = u16::from_le_bytes([b[10], b[11]]);
        let mb = |k: u32| mask & (1 << k) != 0;
        let fl = |o: usize| f32::from_bits(rd_u32(&b, o));
        let mut d = LaserDesc {
            kind: self.vi(t, rd_i16(&b, 0x0C) as i32, mb(0)) as i16,
            colour: self.vi(t, rd_i16(&b, 0x0E) as i32, mb(1)) as i16,
            angle: self.vf(t, fl(0x10), mb(2)),
            speed: self.vf(t, fl(0x14), mb(3)),
            tail: self.vf(t, fl(0x18), mb(4)),
            head: self.vf(t, fl(0x1C), mb(5)),
            max_len: self.vf(t, fl(0x20), mb(6)),
            width: self.vf(t, fl(0x24), mb(7)),
            startup: self.vi(t, rd_u32(&b, 0x28) as i32, mb(8)),
            duration: self.vi(t, rd_u32(&b, 0x2C) as i32, mb(9)),
            fade: self.vi(t, rd_u32(&b, 0x30) as i32, mb(10)),
            hit_start: rd_u32(&b, 0x34) as i32,
            fade_hit: rd_u32(&b, 0x38) as i32,
            flags: rd_u32(&b, 0x3C),
            fixed: ins.op == 114,
            origin: [0.0; 3],
        };
        d.origin = add3(self.e.final_pos, self.e.shot_offset);
        self.e.laser_desc = d.clone();
        let id = self
            .m
            .bullets
            .borrow_mut()
            .spawn_laser(self.anm, &d, self.m.g.player);
        let cur = self.e.laser_cur.min(31);
        self.e.lasers[cur] = id;
    }

    fn with_laser(&mut self, k: i32, f: impl FnOnce(&mut crate::bullet::Laser, [f32; 3])) {
        let Some(&Some(id)) = self.e.lasers.get(k as usize) else {
            return;
        };
        let p = self.m.g.player;
        if let Some(l) = self.m.bullets.borrow_mut().laser_mut(id) {
            f(l, p);
        }
    }

    fn item(&mut self, pos: [f32; 3], kind: i32) {
        let p = self.m.player.borrow();
        self.m
            .bullets
            .borrow_mut()
            .items
            .spawn(self.anm, &p, pos, kind, 0);
    }

    fn cancel_all(&mut self, mode: i32) {
        let mut p = self.m.player.borrow_mut();
        self.m
            .bullets
            .borrow_mut()
            .cancel_all(self.anm, &mut p, mode);
    }

    fn item_burst(&mut self, n: i32, all_one: bool) {
        let p = self.e.pos;
        let low = self.m.player.borrow().stats.power < 128.0;
        for i in 0..n.max(0) {
            let dx = self.anm.rng.next_f32() * 128.0 - 64.0;
            let dy = self.anm.rng.next_f32() * 128.0 - 64.0;
            let kind = if all_one || !low {
                1
            } else if i == 0 {
                2
            } else {
                0
            };
            self.item([p[0] + dx, p[1] + dy, p[2]], kind);
        }
    }

    // Enemy_AutoFireAndDirAnim (07 §9.3).
    fn autofire_and_dir_anim(&mut self, t: &mut Thread) {
        let gs = self.anm.speed;
        if self.e.life > 0 && self.e.autofire_interval > 0 {
            self.e.autofire_timer.tick(gs);
            if self.e.autofire_timer.cur >= self.e.autofire_interval {
                if let Some(b) = self.e.stored_fire.clone() {
                    self.e.desc = self.decode_bullet(t, &b);
                    self.fire_desc();
                }
                self.e.autofire_timer.set(0);
            }
        }
        if self.e.dir_scripts[1] < 0 {
            return;
        }
        let vx = self.e.vel[0];
        let mut dir = if vx < -0.01 {
            1
        } else if vx > 0.01 {
            2
        } else {
            0
        };
        if self.e.flags & A_MIRROR != 0 && dir != 0 {
            dir = 3 - dir;
        }
        let prev = self.e.dir;
        if dir == prev {
            return;
        }
        let script = match (dir, prev) {
            (1, _) => self.e.dir_scripts[1],
            (2, _) => self.e.dir_scripts[2],
            (_, 1) => self.e.dir_scripts[3],
            (_, 2) => self.e.dir_scripts[4],
            _ => self.e.dir_scripts[0],
        };
        self.e.dir = dir;
        self.start_main(script);
    }

    // Variable interpolation slots (07 §5.6).
    fn run_interp(&mut self, t: &mut Thread) {
        let gs = self.anm.speed;
        let old = self.e.pos;
        let mut moved = false;
        for s in 0..8 {
            if !t.ctx.interp[s].active {
                continue;
            }
            let mut sl = t.ctx.interp[s];
            sl.elapsed.tick(gs);
            if sl.elapsed.cur >= sl.dur {
                sl.elapsed.set(sl.dur);
            }
            let tt = if sl.dur > 0 {
                sl.elapsed.getf() / sl.dur as f32
            } else {
                1.0
            };
            let tt = ease(sl.mode, tt);
            let v: [f32; 4] =
                std::array::from_fn(|k| var_f(self.m, self.anm, self.e, &t.ctx, sl.abcd[k]));
            let val = if sl.func == 7 {
                (2.0 * tt + 1.0) * (tt - 1.0) * (tt - 1.0) * v[0]
                    + (3.0 - 2.0 * tt) * tt * tt * v[1]
                    + tt * (1.0 - tt) * (1.0 - tt) * v[2]
                    + (tt - 1.0) * tt * tt * v[3]
            } else {
                v[0] + (v[1] - v[0]) * tt
            };
            self.set_var_any(t, sl.target, val);
            moved |= (10042..=10044).contains(&sl.target);
            sl.active = sl.elapsed.cur < sl.dur;
            t.ctx.interp[s] = sl;
        }
        if moved {
            let e = &mut *self.e;
            e.vel = [e.pos[0] - old[0], e.pos[1] - old[1], e.pos[2] - old[2]];
            e.angle = e.vel[1].atan2(e.vel[0]);
            e.pos = old;
        }
    }

    // --- native functions (07 §12) ---
    fn native(&mut self, t: &mut Thread, n: i32, arg: i32) {
        let gs = self.anm.speed;
        match n {
            0 => self.m.g.native_globals = [t.ctx.int(V_INT0), t.ctx.flt(V_FLT0).to_bits() as i32],
            2 => {
                let e = &mut *self.e;
                let before = e.vel;
                if (e.pos[0] <= 0.0 && e.vel[0] < 0.0) || (e.pos[0] >= 384.0 && e.vel[0] > 0.0) {
                    e.vel[0] = -e.vel[0];
                }
                if e.vel[1] < t.ctx.flt(V_FLT0 + 7) {
                    e.vel[1] += t.ctx.flt(V_FLT0 + 6);
                }
                if e.pos[1] < -64.0 && e.vel[1] < 0.0 {
                    e.vel[1] = -e.vel[1];
                }
                if e.pos[1] >= 480.0 {
                    e.flags &= !A_KEEP_OFFSCREEN;
                }
                if e.vel != before {
                    e.angle = e.vel[1].atan2(e.vel[0]);
                }
            }
            1 => crate::effect::screen(3, 60, 1, 0xFFFFFFFFu32 as i32, 0, 0x15),
            17 => crate::effect::screen(3, 180, 1, 0xFFFFFFFFu32 as i32, 0, 0x15),
            10 => {
                crate::effect::screen(3, 30, 5, 0x40FFFFFF, 0, 0x15);
                crate::effect::screen(7, 4, 120, 190, 60, 0x15);
            }
            15 => crate::effect::screen(7, 16, 20, 20, 20, 0x15),
            3 | 6 | 20 => {
                if let Some(st) = &self.m.stage {
                    st.borrow_mut().spell_mask = true;
                }
                let (kind, extra) = match n {
                    3 => (0x38, 0x61),
                    6 => (0x3A, 0x65),
                    _ => (0x41, 99),
                };
                let pos = self.e.final_pos;
                crate::effect::spawn_fixed(self.anm, kind, pos, 9, 0xFFFFFFFF);
                let i = crate::effect::spawn_fixed(self.anm, kind, pos, 10, 0xFFFFFFFF);
                let anm = &mut *self.anm;
                crate::effect::get(i, |e| {
                    anm.set_script(&mut e.vm, crate::effect::SCRIPT_SLOT, extra)
                });
            }
            5 => {
                crate::effect::free(crate::effect::FIXED_BASE + 9);
                crate::effect::free(crate::effect::FIXED_BASE + 10);
                self.m.g.g_4e4b60 = 2;
            }
            8 => self.familiar_ring(t),
            18 if arg != 0 => self.anm.speed = 1.0 / arg as f32,
            19 => t
                .ctx
                .set_int(V_INT0, self.m.g.spell.as_ref().map_or(0, |s| s.id)),
            // SPEC-GAP: game value at (0x160F510)+0x1C belongs to a later phase; reads 0.
            24 => t.ctx.set_int(V_INT0, 0),
            26 => self.m.g.timer_pause = arg,
            27 => {
                let sub = t.ctx.int(V_INT1 + 2);
                let marked: Vec<_> = {
                    let mut b = self.m.bullets.borrow_mut();
                    b.bullets[..crate::bullet::MAX_BULLETS]
                        .iter_mut()
                        .filter(|x| !x.free() && x.flags & 0x100000 != 0)
                        .map(|x| {
                            x.flags &= !0x100000;
                            (x.pos, x.angle)
                        })
                        .collect()
                };
                for (p, a) in marked {
                    if let Some(ci) = self.m.spawn_prepare(sub, [p[0], p[1], 0.0], 800, false) {
                        self.m.enemies[ci].main.ctx.set_flt(V_FLT0, a);
                        self.m.spawn_finish(self.anm, ci, 800, -2, 10, false);
                    }
                }
            }
            12 | 14 => {
                let (mask, a, s) = (
                    t.ctx.int(V_INT0) as u32,
                    t.ctx.flt(V_FLT0),
                    t.ctx.flt(V_FLT0 + 1),
                );
                self.m
                    .bullets
                    .borrow_mut()
                    .toggle_ghost(self.anm, mask, a, s, n == 14);
                if n == 12 {
                    let pause = t.ctx.int(V_INT0 + 1) == 0;
                    let mut c = self.e.chain_next;
                    let mut guard = 0;
                    while let Some(i) = c
                        && guard <= MAX_ENEMIES
                    {
                        guard += 1;
                        let x = &mut self.m.enemies[i];
                        x.flags_b = if pause {
                            x.flags_b | B_PAUSED
                        } else {
                            x.flags_b & !B_PAUSED
                        };
                        c = x.chain_next;
                    }
                    if let Some(st) = &self.m.stage {
                        st.borrow_mut().spell_interrupt(if pause { 2 } else { 1 });
                    }
                }
            }
            28 if arg != 0 => self.anm.speed = 1.0 / arg as f32,
            29 => {
                self.anm.speed = 1.0;
                if arg > 1 {
                    anmvm::set_slowdown_restore(true);
                }
            }
            30 => self.m.g.g_18b8a2c = arg,
            31 => self.item(self.e.pos, 5),
            22 => {
                // SPEC-GAP: native 22's face argument is not given; no boss cut-in is started.
                let name = encoding_rs::SHIFT_JIS
                    .encode("「リザレクション」")
                    .0
                    .into_owned();
                crate::gui::request(crate::gui::Req::BossShow {
                    face: -1,
                    name,
                    card: -1,
                    name_only: true,
                });
            }
            23 => crate::gui::request(crate::gui::Req::BossHide),
            // SPEC-GAP: natives 4, 7, 9, 11, 13, 16, 21, 25 (effect objects, bullet warps, hit rectangles) are not implemented.
            _ => {
                let _ = gs;
                self.m.g.misc_calls += 1;
            }
        }
    }

    // Native 8 (INFERRED reading of 07 §12): chain members sharing var 10038 spread their orbit angles.
    fn familiar_ring(&mut self, t: &mut Thread) {
        let tag = t.ctx.int(V_INT1 + 2);
        let root = self.e.root;
        let mut members = Vec::new();
        let mut n = root.and_then(|r| {
            if r == self.idx {
                self.e.chain_next
            } else {
                self.m.enemies[r].chain_next
            }
        });
        let mut guard = 0;
        while let Some(i) = n {
            guard += 1;
            if guard > MAX_ENEMIES {
                break;
            }
            if i == self.idx {
                members.push((i, self.e.orbit_angle));
                n = self.e.chain_next;
                continue;
            }
            let c = &self.m.enemies[i];
            if c.alive() && c.main.ctx.int(V_INT1 + 2) == tag {
                members.push((i, c.orbit_angle));
            }
            n = c.chain_next;
        }
        let Some(index) = members.iter().position(|&(i, _)| i == self.idx) else {
            return;
        };
        let count = members.len() as f32;
        let changed = t.ctx.flt(V_FLT0 + 2) != count;
        t.ctx.set_int(V_INT1 + 1, index as i32);
        t.ctx.set_flt(V_FLT0 + 2, count);
        t.ctx.set_flt(V_FLT0 + 1, if changed { 1.0 } else { 0.0 });
        let target = members[0].1 + index as f32 * 2.0 * PI / count;
        let diff = anmvm::wrap_add(target - self.e.orbit_angle, 0.0);
        self.e.orbit_angle = anmvm::wrap_add(self.e.orbit_angle, diff * 0.02);
    }

    fn spell_card(&mut self, ins: &Ins) {
        let b = self.m.ecl.bytes(ins);
        let dec = |o: usize, n: usize, key: u8| -> Vec<u8> {
            let s: Vec<u8> = b
                .get(o..o + n)
                .unwrap_or(&[])
                .iter()
                .map(|c| c ^ key)
                .collect();
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            s[..end].to_vec()
        };
        let bonus = rd_u32(b, 0x10) as i32;
        let secs = (self.e.timeout / 60).max(1);
        let spell = Spell {
            face: rd_i16(b, 0x0C) as i32,
            id: rd_i16(b, 0x0E) as i32,
            bonus: if self.e.flags & A_FIXED_BONUS != 0 {
                99_999_990
            } else {
                bonus
            },
            decay: (bonus - bonus / 7) / secs,
            name: dec(0x14, 48, 0xAA),
            owner: dec(0x44, 48, 0xBB),
            comment: [dec(0x74, 64, 0xDD), dec(0xB4, 64, 0xEE)],
            frames: 0,
            deaths0: self.m.player.borrow().deaths,
            bombs0: self.m.player.borrow().bombs_started,
            capture: true,
            timed_out: false,
            bombed: false,
            last: self.m.player.borrow().gflags & crate::player::GF_LAST_SPELL != 0,
        };
        log::info!(
            "spell card {} ({} bytes name, bonus {}) at enemy frame {}",
            spell.id,
            spell.name.len(),
            spell.bonus,
            self.m.frame
        );
        self.m.g.fixed_bonus = self.e.flags & A_FIXED_BONUS != 0;
        crate::gui::request(crate::gui::Req::BossShow {
            face: spell.face,
            name: spell.name.clone(),
            card: spell.id,
            name_only: false,
        });
        crate::gui::request(crate::gui::Req::Attempt {
            card: spell.id,
            name: spell.name.clone(),
            owner: spell.owner.clone(),
        });
        self.m.g.spell = Some(spell);
        if let Some(st) = &self.m.stage {
            st.borrow_mut().start_spell_bg(self.anm);
        }
        crate::effect::spell_ring(self.anm, self.e.final_pos, false);
        self.cancel_all(1);
        self.e.reset_rank_ranges();
    }

    fn exec(&mut self, t: &mut Thread, ins: &Ins) -> Flow {
        let op = ins.op;
        let next = ins.off + ins.size;
        match op {
            0 | 3 | 84 | 85 => {}
            1 => return Flow::Delete,
            2 => {
                let n = self.ai(t, ins, 0);
                t.ctx.wait.set(n);
            }
            4 => return self.jump(t, ins, 0),
            5 => {
                let c = self.ai(t, ins, 2) - 1;
                self.set_i(t, ins, 2, c);
                if c > 0 {
                    return self.jump(t, ins, 0);
                }
            }
            6 => {
                let v = self.ai(t, ins, 1);
                self.set_i(t, ins, 0, v);
            }
            7 => {
                let v = self.af(t, ins, 1);
                self.set_f(t, ins, 0, v);
            }
            8 => {
                let b = self.ai(t, ins, 1);
                let v = if self.anm.rng.next_u16() & 1 != 0 {
                    b
                } else {
                    -b
                };
                self.set_i(t, ins, 0, v);
            }
            9 => {
                let b = self.af(t, ins, 1);
                let v = if self.anm.rng.next_u16() & 1 != 0 {
                    b
                } else {
                    -b
                };
                self.set_f(t, ins, 0, v);
            }
            10..=14 | 20..=24 => {
                let (a, b) = if op < 20 {
                    (self.ai(t, ins, 0), self.ai(t, ins, 1))
                } else {
                    (self.ai(t, ins, 1), self.ai(t, ins, 2))
                };
                let v = match op % 10 {
                    0 => a.wrapping_add(b),
                    1 => a.wrapping_sub(b),
                    2 => a.wrapping_mul(b),
                    3 => a.checked_div(b).unwrap_or(0),
                    _ => a.checked_rem(b).unwrap_or(0),
                };
                self.set_i(t, ins, 0, v);
            }
            15..=19 | 25..=29 => {
                let (a, b) = if op < 25 {
                    (self.af(t, ins, 0), self.af(t, ins, 1))
                } else {
                    (self.af(t, ins, 1), self.af(t, ins, 2))
                };
                let v = match (op - 15) % 10 {
                    0 => a + b,
                    1 => a - b,
                    2 => a * b,
                    3 => a / b,
                    _ => a % b,
                };
                self.set_f(t, ins, 0, v);
            }
            30 | 31 => {
                let a = self.ai(t, ins, 0);
                self.set_i(
                    t,
                    ins,
                    0,
                    if op == 30 {
                        a.wrapping_add(1)
                    } else {
                        a.wrapping_sub(1)
                    },
                );
            }
            32 | 33 => {
                let b = self.af(t, ins, 1);
                self.set_f(t, ins, 0, if op == 32 { b.sin() } else { b.cos() });
            }
            34 | 39 => {
                let (x1, y1, x2, y2) = (
                    self.af(t, ins, 1),
                    self.af(t, ins, 2),
                    self.af(t, ins, 3),
                    self.af(t, ins, 4),
                );
                let v = if op == 34 {
                    (y2 - y1).atan2(x2 - x1)
                } else {
                    ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt()
                };
                self.set_f(t, ins, 0, v);
            }
            35 => {
                let (b, c, d) = (self.af(t, ins, 1), self.af(t, ins, 2), self.af(t, ins, 3));
                self.set_f(t, ins, 0, (b - c) * d + c);
            }
            36 => {
                let target = self.raw(ins, 0) as i32;
                let dur = self.ai(t, ins, 1);
                let func = self.ai(t, ins, 2);
                let mode = self.ai(t, ins, 3);
                let abcd: [f32; 4] = std::array::from_fn(|k| f32::from_bits(self.raw(ins, 4 + k)));
                let slots = &mut t.ctx.interp;
                let s = slots
                    .iter()
                    .position(|s| s.active && s.target == target)
                    .or_else(|| slots.iter().position(|s| !s.active));
                if let Some(s) = s {
                    let mut elapsed = anmvm::Timer::default();
                    elapsed.set(0);
                    slots[s] = Interp {
                        active: true,
                        elapsed,
                        dur,
                        func,
                        mode,
                        abcd,
                        target,
                    };
                }
            }
            37 => {
                let a = self.af(t, ins, 0);
                self.set_f(t, ins, 0, anmvm::wrap_add(a, 0.0));
            }
            38 | 166 => {
                let (ang, r) = (self.af(t, ins, 2), self.af(t, ins, 3));
                let ang = if op == 38 {
                    anmvm::wrap_add(ang, 0.0)
                } else {
                    ang
                };
                if op == 38 {
                    self.set_f(t, ins, 0, ang.cos() * r);
                    self.set_f(t, ins, 1, ang.sin() * r);
                } else {
                    self.set_f(t, ins, 1, ang.sin() * r);
                    self.set_f(t, ins, 0, ang.cos() * r);
                }
            }
            40..=51 => {
                use std::cmp::Ordering::*;
                let ord = if op % 2 == 0 {
                    self.ai(t, ins, 0).cmp(&self.ai(t, ins, 1))
                } else {
                    self.af(t, ins, 0)
                        .partial_cmp(&self.af(t, ins, 1))
                        .unwrap_or(Greater)
                };
                let take = match (op - 40) / 2 {
                    0 => ord == Equal,
                    1 => ord != Equal,
                    2 => ord == Less,
                    3 => ord != Greater,
                    4 => ord == Greater,
                    _ => ord != Less,
                };
                if take {
                    return self.jump(t, ins, 2);
                }
            }
            52 => {
                let sub = self.ai(t, ins, 0);
                if self.m.ecl.sub_start(sub).is_some() {
                    self.call(t, next, sub);
                    return Flow::Jumped;
                }
            }
            53 => {
                t.depth -= 1;
                if t.depth < 0 {
                    if self.e.running_thread.is_some() {
                        return Flow::ThreadEnd;
                    }
                    // Malformed main-context return at depth 0 (never in shipped data, answers_phase5): halt this frame.
                    t.depth = 0;
                    return Flow::Post;
                }
                match t.stack.get(t.depth as usize) {
                    Some(c) => t.ctx = c.clone(),
                    None => return Flow::Post,
                }
                return Flow::Jumped;
            }
            54 | 58 => {
                self.e.set_flag_b(B_STAGE_ANM, op == 58);
                let s = self.ai(t, ins, 0);
                self.start_main(s);
            }
            55 | 59 => {
                self.e.set_flag_b(B_STAGE_ANM, op == 59);
                let base = self.ai(t, ins, 0);
                self.e.dir_scripts = std::array::from_fn(|k| base + k as i32);
            }
            56 | 60 => {
                self.e.set_flag_b(B_STAGE_ANM, op == 60);
                for k in 0..6 {
                    self.e.dir_scripts[k] = self.ai(t, ins, k);
                }
            }
            57 | 61 => {
                self.e.set_flag_b(B_STAGE_ANM, op == 61);
                let (k, s) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                self.start_sub(k.clamp(0, 1) as usize, s);
            }
            62 => {
                let s = self.e.dir_scripts[5];
                self.start_main(s);
            }
            63 => {
                self.e.pos = [self.af(t, ins, 0), self.af(t, ins, 1), 0.0];
                self.e.clamp_pos();
            }
            64 => {
                let (dur, ease) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                let (x, y) = (self.af(t, ins, 2), self.af(t, ins, 3));
                let f = self.e.final_pos;
                let mut delta = [x - f[0], y - f[1], 0.0 - f[2]];
                if self.e.flags & A_MIRROR != 0 {
                    delta[0] = -delta[0];
                }
                self.mode2(dur, ease, self.e.pos, delta);
            }
            65 => {
                let (a, s) = (self.af(t, ins, 0), self.af(t, ins, 1));
                self.mode1(a, s, 0);
            }
            66 => {
                let (dur, ease, a, s) = (
                    self.ai(t, ins, 0),
                    self.ai(t, ins, 1),
                    self.af(t, ins, 2),
                    self.af(t, ins, 3),
                );
                self.move_dir(dur, ease, a, s);
            }
            67 | 178 => {
                let (dur, ease, s) = (self.ai(t, ins, 0), self.ai(t, ins, 1), self.af(t, ins, 2));
                let a = self.random_dir(op == 178);
                self.move_dir(dur, ease, a, s);
            }
            68 => {
                let (off, s) = (self.af(t, ins, 0), self.af(t, ins, 1));
                self.e.angle = anmvm::wrap_add(self.player_angle(self.e.pos) + off, 0.0);
                self.e.speed = s;
            }
            69 => {
                let (dur, ease, off, s) = (
                    self.ai(t, ins, 0),
                    self.ai(t, ins, 1),
                    self.af(t, ins, 2),
                    self.af(t, ins, 3),
                );
                if dur < 1 {
                    let a = self.player_angle(self.e.pos) + off;
                    self.mode1(a, s, dur);
                } else {
                    // Quirk: with a duration the movement is not aimed (same as op 66).
                    self.move_dir(dur, ease, off, s);
                }
            }
            70 => {
                self.e.angvel = self.af(t, ins, 0);
                self.e.set_move_mode(1);
            }
            71 => {
                self.e.accel = self.af(t, ins, 0);
                self.e.set_move_mode(1);
            }
            72 => {
                let dur = self.ai(t, ins, 0);
                self.e.interp_start = [self.af(t, ins, 1), self.af(t, ins, 2), 0.0];
                self.e.orbit_angle = self.af(t, ins, 3);
                self.e.orbit_av = self.af(t, ins, 4);
                self.e.radius = self.af(t, ins, 5);
                self.e.radius_vel = self.af(t, ins, 6);
                self.mode3(dur);
            }
            73 => {
                let dur = self.ai(t, ins, 0);
                self.e.interp_start = self.e.pos;
                self.e.orbit_angle = self.af(t, ins, 1);
                self.e.orbit_av = self.af(t, ins, 2);
                self.e.radius = 0.0;
                self.e.radius_vel = self.af(t, ins, 3);
                self.mode3(dur);
            }
            74 => {
                let dur = self.ai(t, ins, 0);
                self.e.orbit_av = self.af(t, ins, 1);
                self.e.radius_vel = self.af(t, ins, 2);
                self.mode3(dur);
            }
            75 => {
                self.e.clamp = [
                    self.af(t, ins, 0),
                    self.af(t, ins, 1),
                    self.af(t, ins, 2),
                    self.af(t, ins, 3),
                ];
                self.e.flags |= A_CLAMP;
            }
            76 => self.e.flags &= !A_CLAMP,
            77 => {
                self.e.hitbox[0] = self.af(t, ins, 0);
                self.e.hitbox[1] = self.af(t, ins, 1);
            }
            78 => {
                self.e.shotbox[0] = self.af(t, ins, 0);
                self.e.shotbox[1] = self.af(t, ins, 1);
            }
            79 => {
                let m = self.ai(t, ins, 0);
                let e = &mut *self.e;
                e.set_flag(A_SHOT_HIT, m & 1 == 0);
                e.set_flag(A_CONTACT, m & 2 == 0);
                e.set_flag(A_LOSES_LIFE, m & 4 == 0);
                e.set_flag(A_HIDDEN, m & 8 != 0);
                e.set_flag(A_KEEP_OFFSCREEN, m & 16 != 0);
                e.set_flag_b(B_CANNOT_DIE, m & 32 != 0);
            }
            80 | 81 => {
                let m = self.ai(t, ins, 0);
                let on = op == 81;
                let e = &mut *self.e;
                if m & 1 != 0 {
                    e.set_flag(A_SHOT_HIT, on);
                }
                if m & 2 != 0 {
                    e.set_flag(A_CONTACT, on);
                }
                if m & 4 != 0 {
                    e.set_flag(A_LOSES_LIFE, on);
                }
                if m & 8 != 0 {
                    e.set_flag(A_HIDDEN, !on);
                }
                if m & 16 != 0 {
                    e.set_flag(A_KEEP_OFFSCREEN, !on);
                }
                if m & 32 != 0 {
                    e.set_flag_b(B_CANNOT_DIE, !on);
                }
            }
            82 => {
                let r = self.af(t, ins, 0);
                self.e.nofire_r2 = r * r;
            }
            83 => {
                let b = self.ai(t, ins, 0);
                self.e.set_flag_b(1 << 1, b & 1 != 0);
            }
            86 | 87 => {
                let (var, slot) = (self.ai(t, ins, 1), self.ai(t, ins, 2));
                if let Some(b) = self.m.boss(slot) {
                    let (v_i, v_f) = if b == self.idx {
                        (
                            var_i(self.m, self.anm, self.e, &t.ctx, var),
                            var_f(self.m, self.anm, self.e, &t.ctx, var as f32),
                        )
                    } else {
                        let other = self.m.enemies[b].clone();
                        (
                            var_i(self.m, self.anm, &other, &other.main.ctx, var),
                            var_f(self.m, self.anm, &other, &other.main.ctx, var as f32),
                        )
                    };
                    if op == 86 {
                        self.set_i(t, ins, 0, v_i);
                    } else {
                        self.set_f(t, ins, 0, v_f);
                    }
                }
            }
            88 => {
                let (slot, sub) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                if let Some(b) = self.m.boss(slot)
                    && self.m.ecl.sub_start(sub).is_some()
                {
                    if b == self.idx {
                        self.call(t, next, sub);
                        return Flow::Jumped;
                    }
                    let args = self.m.g.args;
                    let ecl = &self.m.ecl;
                    let o = &mut self.m.enemies[b];
                    if o.flags & A_NO_PUSH == 0 {
                        o.main.push();
                    }
                    o.main.ctx.enter(ecl, sub);
                    o.main.ctx.vars[V_ARGI..V_ARGI + 8].copy_from_slice(&args);
                }
            }
            89 => {
                let (slot, k) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                match self.m.boss(slot) {
                    Some(b) if b == self.idx => self.e.pending_interrupt = k,
                    Some(b) => self.m.enemies[b].pending_interrupt = k,
                    None => {}
                }
            }
            90..=94 => self.spawn_child(t, ins),
            95 => {
                self.m.kill_all(self.anm, Some(self.idx));
            }
            96..=104 => self.bullet_op(t, ins),
            105 | 106 => {
                let n = self.ai(t, ins, 0);
                let iv = n + lerp_i(n / 5, -n / 5, crate::gamemgr::rank());
                self.e.autofire_interval = iv;
                let start = if op == 106 { self.anm.rng.range(iv) } else { 0 };
                self.e.autofire_timer.set(start);
            }
            107 => self.e.flags |= A_STORE_FIRE,
            108 => self.e.flags &= !A_STORE_FIRE,
            109 => self.fire_desc(),
            110 => self.e.shot_offset = [self.af(t, ins, 0), self.af(t, ins, 1), 0.0],
            111 => {
                let k = self.ai(t, ins, 0);
                let cmd = ExCmd {
                    cmd: self.ai(t, ins, 1) as u32,
                    chain: self.ai(t, ins, 2),
                    c: self.ai(t, ins, 3),
                    d: self.ai(t, ins, 4),
                    e: self.af(t, ins, 5),
                    f: self.af(t, ins, 6),
                };
                if let Some(s) = self.e.desc.ex.get_mut(k as usize) {
                    *s = cmd;
                }
            }
            112 => self.cancel_all(1),
            162 => self.cancel_all(4),
            113 => {
                let (id, x) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                if id < 0 {
                    self.e.desc.flags &= !0x200;
                } else {
                    self.e.desc.sfx = id;
                    self.e.desc.flags |= 0x200;
                }
                self.e.desc.ex_sfx = x;
            }
            114 | 115 => self.laser_op(t, ins),
            116 => self.e.laser_cur = self.ai(t, ins, 0).clamp(0, 31) as usize,
            117 | 118 | 167 => {
                let (k, a) = (self.ai(t, ins, 0), self.af(t, ins, 1));
                self.with_laser(k, |l, p| {
                    l.angle = match op {
                        117 => anmvm::wrap_add(l.angle, a),
                        118 => angle_from(l.origin, p) + a,
                        _ => a,
                    }
                });
            }
            119 => {
                let k = self.ai(t, ins, 0);
                let off = [self.af(t, ins, 1), self.af(t, ins, 2), self.af(t, ins, 3)];
                let p = add3(self.e.final_pos, off);
                self.with_laser(k, |l, _| l.origin = p);
            }
            120 => {
                let k = self.ai(t, ins, 0);
                let mut alive = false;
                self.with_laser(k, |_, _| alive = true);
                t.ctx.set_int(V_INT1 + 2, alive as i32);
            }
            121 => {
                let k = self.ai(t, ins, 0);
                self.with_laser(k, |l, _| {
                    if l.state < 2 {
                        l.state = 2;
                        l.timer = anmvm::Timer::default();
                    }
                });
            }
            122 => self.spell_card(ins),
            123 => {
                let (total, remaining) = (
                    self.e.timeout.max(0),
                    (self.e.timeout - self.e.boss_timer.cur).max(0),
                );
                self.m
                    .finish_spell(total, remaining, self.anm, Some(self.idx));
            }
            124 => crate::sound::play_at(self.ai(t, ins, 0), self.e.final_pos[0]),
            125 => {
                self.e.pending_interrupt = self.ai(t, ins, 0);
                return Flow::Interrupt;
            }
            126 => {
                let (sub, k) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                if let Some(s) = self.e.interrupts.get_mut(k as usize) {
                    *s = sub;
                }
            }
            127 => {
                let slot = self.ai(t, ins, 0);
                if slot < 0 {
                    self.m.unregister_boss(self.idx);
                    self.e.flags &= !A_BOSS;
                } else if let Some(b) = self.m.bosses.get_mut(slot as usize) {
                    *b = Some(self.idx);
                    self.e.flags |= A_BOSS;
                    self.e.boss_slot = slot;
                    self.e.nofire_r2 = 0.0;
                    if slot == 0 {
                        self.m.g.hud_boss = true;
                    }
                }
            }
            128 => {
                self.e.effects += 1;
                let axis = [self.af(t, ins, 1), self.af(t, ins, 2), self.af(t, ins, 3)];
                self.e.orbit_target = self.ai(t, ins, 4) as f32;
                let i = crate::effect::spawn(self.anm, 13, self.e.final_pos, 1, 0xFF6060D0);
                if i != crate::effect::NONE {
                    crate::effect::get(i, |x| x.dir = axis);
                    self.e.fx_attached.push(i);
                }
            }
            139 => {
                self.e.effects += 1;
                let (id, n, c) = (self.ai(t, ins, 0), self.ai(t, ins, 1), self.ai(t, ins, 2));
                crate::effect::spawn(self.anm, id as u8, self.e.final_pos, n, c as u32);
            }
            140 => {
                self.e.effects += 1;
                let (id, n, c) = (self.ai(t, ins, 0), self.ai(t, ins, 1), self.ai(t, ins, 2));
                let param = [self.af(t, ins, 3), self.af(t, ins, 4), self.af(t, ins, 5)];
                crate::effect::spawn_param(
                    self.anm,
                    id as u8,
                    self.e.final_pos,
                    param,
                    n,
                    c as u32,
                );
            }
            174 => {
                self.e.effects += 1;
                let n = self.ai(t, ins, 0);
                crate::effect::free(self.e.fx_aura);
                self.e.fx_aura = crate::effect::spawn_high(
                    self.anm,
                    (0x20 + n) as u8,
                    self.e.final_pos,
                    0xFFFFFFFF,
                );
            }
            129 => {
                let m = self.raw(ins, 0) & 0xFF;
                self.e.flags = (self.e.flags & !(7 << A_DEATH_SHIFT)) | ((m & 7) << A_DEATH_SHIFT);
            }
            130 => self.e.death_sub = self.raw(ins, 0) as i16 as i32,
            131 => {
                let n = self.ai(t, ins, 0);
                (self.e.life, self.e.max_life, self.e.hud_life) = (n, n, n);
                if self.e.boss_slot == 0 && self.e.flags & A_BOSS != 0 {
                    self.m.g.hud_markers = Default::default();
                }
            }
            132 => {
                let n = self.ai(t, ins, 0);
                self.e.boss_timer.set(n);
            }
            133 => {
                let (k, life, sub) = (self.ai(t, ins, 0), self.ai(t, ins, 1), self.ai(t, ins, 2));
                if let Some(k) = usize::try_from(k).ok().filter(|&k| k < 4) {
                    self.e.thresholds[k] = life;
                    self.e.threshold_subs[k] = sub;
                }
            }
            134 => {
                self.e.timeout = self.ai(t, ins, 0);
                self.e.timeout_sub = self.ai(t, ins, 1);
                self.e.boss_timer.set(0);
            }
            135 => {
                let (k, sub) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                if let Some(k) = usize::try_from(k).ok().filter(|&k| k < 4) {
                    if self.e.running_thread == Some(k) {
                        self.e.kill_running_thread = true;
                    }
                    self.e.threads[k] = None;
                    if sub >= 0 {
                        let mut th = Thread::default();
                        th.ctx.vars = t.ctx.vars;
                        if th.ctx.enter(&self.m.ecl, sub) {
                            self.e.threads[k] = Some(Box::new(th));
                            if self.e.running_thread == Some(k) {
                                self.e.kill_running_thread = false;
                            }
                        }
                    }
                }
            }
            136 => {
                let (n, arg) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                self.native(t, n, arg);
            }
            137 => {
                let (n, arg) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                t.ctx.callback = if n < 0 { -1 } else { n };
                t.ctx.callback_arg = arg;
            }
            138 => {
                let b = self.raw(ins, 0).to_le_bytes();
                self.e.death_bytes = [b[0], b[1], b[2]];
            }
            141 => {
                let k = self.ai(t, ins, 0);
                self.item(self.e.pos, k);
            }
            142 => {
                let n = self.ai(t, ins, 0);
                self.item_burst(n, false);
            }
            168 => {
                let n = self.ai(t, ins, 0);
                self.item_burst(n, true);
            }
            143 => self.e.item = self.ai(t, ins, 0),
            144 => {
                self.e.extra_point = self.ai(t, ins, 0);
                self.e.extra_power = self.ai(t, ins, 1);
            }
            145 => self.e.set_flag(1 << 25, self.raw(ins, 0) & 0xFF != 0),
            146 => {
                let n = self.ai(t, ins, 0);
                t.ctx.clock.cur += n;
            }
            147 => {
                let n = self.ai(t, ins, 0);
                if let Some(s) = &self.m.stage {
                    s.borrow_mut().section_request = n;
                }
            }
            148 => {
                self.m.g.hud_bars = self.ai(t, ins, 0);
                self.m.g.hud_requests.push(148);
            }
            149 => {
                let n = self.ai(t, ins, 0);
                self.e.vm.pending_interrupt = n as u16;
            }
            150 => {
                let (k, n) = (self.raw(ins, 0) as usize, self.raw(ins, 1));
                if let Some(vm) = self.e.sub_vms.get_mut(k) {
                    vm.pending_interrupt = n as u16;
                }
            }
            151 => self.e.set_flag(A_NO_PUSH, self.raw(ins, 0) & 0xFF != 0),
            152 => {
                self.e.rank_speed = [self.af(t, ins, 0), self.af(t, ins, 1)];
                for k in 0..4 {
                    self.e.rank_counts[k] = self.ai(t, ins, 2 + k);
                }
            }
            153 => {
                self.e.timeout_sub = self.e.death_sub;
                self.e.boss_timer.set(0);
            }
            154 => self.e.lasers = [None; 32],
            155 => {
                let on = self.raw(ins, 0) & 0xFF != 0;
                self.e.set_flag(A_FIXED_BONUS, on);
                self.m.g.fixed_bonus = on;
            }
            156 => {
                self.e
                    .set_flag(A_CANCEL_BONUS, self.raw(ins, 0) & 0xFF != 0);
                self.e.layer = 2;
            }
            157 => {
                self.e.trail = [
                    self.raw(ins, 0) as i32,
                    self.ai(t, ins, 1),
                    self.ai(t, ins, 2),
                    self.ai(t, ins, 3),
                ];
            }
            158 => {
                let k = self.ai(t, ins, 0);
                let v = [self.ai(t, ins, 1), self.ai(t, ins, 2), self.ai(t, ins, 3)];
                if let Some(s) = self.m.g.hud_markers.get_mut(k as usize) {
                    *s = v;
                }
            }
            159 => self.e.layer = self.ai(t, ins, 0) as u8,
            160 => {
                let n = self.ai(t, ins, 0);
                self.e.dmg_timer.set(n);
            }
            161 => {
                let r = self.af(t, ins, 0);
                let p = self.m.player.borrow();
                self.m
                    .bullets
                    .borrow_mut()
                    .cancel_radius(self.anm, &p, self.e.final_pos, r);
            }
            163 => self.m.g.g_f54cec = self.ai(t, ins, 0),
            164 => self.m.g.misc_calls += 1,
            165 => {
                self.e.vm.rotation[2] = self.af(t, ins, 0);
                self.e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            169 => {
                let (x, px) = (self.e.pos[0], self.m.g.player[0]);
                let r = self.anm.rng.next_f32() * (PI / 2.0);
                let a = if x > px && x > 96.0 || x > 288.0 {
                    anmvm::wrap_add(r + 3.0 * PI / 4.0, 0.0)
                } else {
                    r - PI / 4.0
                };
                self.set_f(t, ins, 0, a);
            }
            170..=172 => {
                let k = self.ai(t, ins, 0);
                let (a, b) = (
                    self.af(t, ins, 1),
                    if op == 172 { self.af(t, ins, 2) } else { 0.0 },
                );
                let byte = self.raw(ins, 1) as u8;
                self.with_laser(k, |l, _| match op {
                    170 => l.hide_emitter = byte,
                    171 => l.max_len = a,
                    _ => (l.tail, l.head) = (a, b),
                });
            }
            173 => {
                let v = self.ai(t, ins, 0) != 0;
                self.e.set_flag(1 << 30, v);
            }
            175 => self.m.g.spawn_disabled = self.ai(t, ins, 0),
            176 => {
                use crate::player::{GF_13, GF_LAST_CAPTURED, GF_LAST_SPELL, GF_SPELL_PRACTICE};
                let mut p = self.m.player.borrow_mut();
                p.gflags &= !(GF_LAST_SPELL | GF_LAST_CAPTURED | GF_13);
                p.gflags |= GF_LAST_SPELL;
                let card = self.m.g.card;
                let bit13 = if p.gflags & GF_SPELL_PRACTICE == 0 {
                    matches!(self.m.stage_index, 6 | 7)
                } else {
                    (0x8F..=0x92).contains(&card) || (0xAB..=0xBE).contains(&card)
                };
                // SPEC-GAP: bit 13's large player-damage box (EnemyMgr_Calc, boss present) has no size; not modelled.
                if bit13 {
                    p.gflags |= GF_13;
                }
                self.e.flags |= A_LAST_SPELL;
            }
            177 => self.e.hud_life = self.ai(t, ins, 0),
            179 | 180 => self.m.g.hud_requests.push(op as u16),
            184 => self.m.g.misc_calls += 1,
            181 => {
                crate::sound::play(crate::sound::OK_CLOCK);
                let mut p = self.m.player.borrow_mut();
                if p.stats.clock < 12 {
                    p.stats.clock += 1;
                    drop(p);
                    self.m.g.hud_requests.push(181);
                }
            }
            182 => {
                let v = self.ai(t, ins, 0) != 0;
                self.e.set_flag_b(1 << 8, v);
            }
            183 => {
                let v = self.ai(t, ins, 0) != 0;
                self.e.set_flag(1 << 31, v);
            }
            _ => {}
        }
        Flow::Next
    }
}
