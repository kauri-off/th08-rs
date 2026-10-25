//! ECL interpreter `Ecl_Run` 0x4184B0: contexts, variables, opcodes, natives (07 §5–§9, §11, §12).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::bullet::{BulletDesc, ExCmd, LaserDesc, Warp, angle_from};
use crate::ecl::Ins;
use crate::enemy::*;
use std::f32::consts::PI;

// Card bonus cap g_Gui +0x2638 for flag A bit 27 / op 155 (07 §11.1, op 155).
const FIXED_BONUS_CAP: i32 = 99_999_990;

// What an opcode does to the §5.2 EXEC loop.
enum Flow {
    Next,
    Jumped,
    Post,
    Interrupt,
    Delete,
    ThreadEnd,
}

// How a context's run ended: POST reached, op 1 delete, or a thread returned from its first sub.
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
    let r = Exec {
        m,
        anm,
        e,
        idx,
        loaded_ip: 0,
    }
    .run_thread(&mut main);
    e.main = main;
    if r == End::Delete {
        return -1;
    }
    // 0x2CEA is written back after each context, so it keeps the last one's depth (07 §5.4 depth storage).
    e.work_depth = e.main.depth;
    // Threads in slot order after main; one created this frame by main runs this frame (07 §5.2).
    for k in 0..4 {
        let Some(mut t) = e.threads[k].take() else {
            continue;
        };
        e.running_thread = Some(k);
        e.kill_running_thread = false;
        let r = Exec {
            m,
            anm,
            e,
            idx,
            loaded_ip: 0,
        }
        .run_thread(&mut t);
        e.running_thread = None;
        if r == End::Delete {
            return -1;
        }
        e.work_depth = if r == End::Finished {
            e.main.depth
        } else {
            t.depth
        };
        if r != End::Finished && !e.kill_running_thread && e.threads[k].is_none() {
            e.threads[k] = Some(t);
        }
    }
    // After all contexts: Enemy_UpdateMoveMode, then Enemy_AutoFireAndDirAnim (07 §5.2).
    e.update_move_mode(anm.speed);
    let mut main = std::mem::take(&mut e.main);
    Exec {
        m,
        anm,
        e,
        idx,
        loaded_ip: 0,
    }
    .autofire_and_dir_anim(&mut main);
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
    // ctx.ip as stored while the context runs: the ip Ecl_Run last loaded into its local copy (07 §5.4 op 88).
    loaded_ip: usize,
}

// Writable int variables (07 §6 "writable as int").
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

// Writable float variables (07 §6 "writable as float"); 10045–47 (player pos) are writable (!).
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
    // Float variable ids are float literals (e.g. 10016.0), truncated before lookup (07 §6).
    let id = lit as i32;
    // 07 §6: the getter reads the final pos e+0x2D88; only the destination (FRef::Pos) is the pos e+0x2D34.
    if let 10042..=10044 = id {
        return e.final_pos[(id - 10042) as usize];
    }
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
        10096 => chain_count(m, e) as f32,
        10097 => m.g.player_mode as f32,
        // 07 §6: card active (bit 0) ? bit 2 capture possible : bit 9 last card captured.
        10099 => m.g.spell.as_ref().map_or(m.g.spell_captured, |s| s.capture) as i32 as f32,
        _ => lit,
    }
}

// Int getter 0x41F420 (07 §6): float variables read through the float getter and truncate.
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
        // 07 §6 (0x41F5E7): Rng_Float truncated, one Next32; 1 only when it rounds up to 1.0.
        10033 => anm.rng.next_f32() as i32,
        10032 => (anm.rng.next_u32() & 0x7FFF_FFFF) as i32,
        10034 => anm.rng.next_u32() as i32,
        // 07 §6: 2 if stage time + pending capture time + live time orbs ≥ TIME_THR (11 §3.1).
        10098 => {
            let pending = m.g.payout.map_or(0, |c| c.time);
            let orbs = m.bullets.borrow().items.count(crate::item::TIME_ORB);
            let st = &m.player.borrow().stats;
            if st.time + pending + orbs >= st.time_threshold {
                2
            } else {
                0
            }
        }
        10100 => m.g.spell.as_ref().map_or(0, |s| s.frames),
        // Int read of 10079–10081 gives the literal; 10082 is float only.
        10079..=10082 => id,
        _ if flt_ref(id).is_some()
            || matches!(
                id,
                10035 | 10048 | 10050 | 10052 | 10083..=10091 | 10096 | 10097 | 10099
            ) =>
        {
            var_f(m, anm, e, c, id as f32) as i32
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

// Var 10096, Enemy_ChainChildCount 0x41FD40 (07 §6): a root (no parent, +8 set) counts the enemies reached by +8;
// a child is evaluated on its parent (0 unless the parent is a root); no parent and no +8 → 0. No alive test.
fn chain_count(m: &EnemyManager, e: &Enemy) -> i32 {
    let root = match e.root {
        Some(p) => &*m.enemies[p],
        None => e,
    };
    if root.root.is_some() {
        return 0;
    }
    let mut d = 0;
    let mut p = root.chain_next;
    while let Some(i) = p {
        d += 1;
        if d > MAX_ENEMIES as i32 {
            break;
        }
        p = m.enemies[i].chain_next;
    }
    d
}

// Rank lerps (07 §9.2): rank = var 10041 (8–16), integer division truncates toward 0.
// Spell practice (GameFlags bit 14) with a last spell running or captured (bits 7–8) (07 §5 ops 129–134, 11 §2.1).
fn practice_guard() -> bool {
    use crate::gamemgr::{F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE, flags};
    let f = flags();
    f & F_SPELL_PRACTICE != 0 && f & (F_LAST_SPELL | F_LAST_CAPTURED) != 0
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
    // One context of Ecl_Run (07 §5.2): LOOP / EXEC / POST.
    // Game speed is read live at each Decrement/Tick (07 §5.2): a native 18 earlier in this run already applies.
    fn run_thread(&mut self, t: &mut Thread) -> End {
        self.loaded_ip = t.ctx.ip;
        'outer: loop {
            // 07 §5.2 / §5.5: the pending test sits at the LOOP of every context, threads included.
            if self.e.pending_interrupt >= 0 {
                self.interrupt(t);
                self.loaded_ip = t.ctx.ip;
                continue;
            }
            loop {
                self.e.final_pos = add3(self.e.pos, self.e.pos_offset);
                // Wait: the clock is decremented so POST's Tick nets zero.
                if t.ctx.wait.cur > 0 {
                    t.ctx.wait.dec(self.anm.speed);
                    t.ctx.clock.dec(self.anm.speed);
                    break 'outer;
                }
                let Some(ins) = self.m.ecl.ins(t.ctx.ip) else {
                    break 'outer;
                };
                // Exact time match (07 §5.2): a past instruction is never reached, the sub stalls.
                if ins.op == -1 || t.ctx.clock.cur != ins.time {
                    break 'outer;
                }
                // Rank mask (07 §2.3): skipped in the same frame.
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
        // POST (07 §5.2): native callback and interpolation slots only while life > 0.
        if self.e.life > 0 {
            if t.ctx.callback >= 0 {
                let (n, arg) = (t.ctx.callback, t.ctx.callback_arg);
                self.native(t, n, arg);
            }
            self.run_interp(t);
        }
        t.ctx.clock.tick(self.anm.speed);
        End::Post
    }

    // Interrupt handler 0x41C88A (07 §5.5): the current context's pending (not yet executed) instruction is
    // skipped; the main context is pushed onto the current stack at the working depth and enters table[k].
    fn interrupt(&mut self, t: &mut Thread) {
        let k = self.e.pending_interrupt;
        self.e.pending_interrupt = -1;
        // k ≥ 32 would read the pending word / later enemy fields (07 §5.5); unreachable, the data sends k = 1 only.
        let sub = self.e.interrupts.get(k as usize).copied().unwrap_or(0);
        let size = self.m.ecl.ins(t.ctx.ip).map_or(0, |i| i.size);
        t.ctx.ip += size;
        // Unless flag A bit 26 push; depth += 1 either way (≤ 15).
        let main = if self.e.running_thread.is_some() {
            self.e.main.ctx.clone()
        } else {
            t.ctx.clone()
        };
        if self.e.flags & A_NO_PUSH == 0 {
            t.depth = t.push_copy(t.depth, main);
        } else {
            t.depth = (t.depth + 1).min(15);
        }
        // Ecl_SetSub with sub < 0 returns at once: the main context keeps its ip and clock.
        if self.e.running_thread.is_some() {
            self.e.main.ctx.enter(&self.m.ecl, sub);
        } else {
            t.ctx.enter(&self.m.ecl, sub);
        }
    }

    // Ecl_CallSub 0x421BD0 (07 §5.4): push unless flag A bit 26, then copy the global args 10061–68 into
    // the callee's 10053–60.
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

    // Argument k as int / float: a variable id when var-mask bit k is set (07 §2.1, §6).
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

    // Same with an explicit mask bit (bullet/laser blocks, 07 §9.1, §9.4).
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

    // Destination read of an in-place int op (07 §6 `Ecl_IntVarRef` 0x41FE10): through the reference, never the
    // getter; anything but a writable int variable reads the instruction's own argument slot (e.g. 10042–10044).
    fn ai_dst(&self, t: &Thread, ins: &Ins, k: usize) -> i32 {
        let raw = self.raw(ins, k) as i32;
        match int_ref(raw).filter(|_| ins.mask & (1 << k) != 0) {
            Some(r) => read_i(self.m, self.e, &t.ctx, &r),
            None => raw,
        }
    }

    // Destination read of an in-place float op (07 §6 `Ecl_FloatVarRef` 0x420950): 10042–10044 are the pos, not the
    // final pos; anything but a writable float variable reads the argument slot.
    fn af_dst(&self, t: &Thread, ins: &Ins, k: usize) -> f32 {
        let bits = self.raw(ins, k);
        match flt_ref(f32::from_bits(bits) as i32).filter(|_| ins.mask & (1 << k) != 0) {
            Some(r) => read_f(self.m, self.e, &t.ctx, &r),
            None => f32::from_bits(bits),
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

    // Interpolation targets (07 §5.6): float variables, else int variables (truncated).
    fn set_var_any(&mut self, t: &mut Thread, id: i32, v: f32) {
        if let Some(r) = flt_ref(id) {
            self.write_f(t, r, v);
        } else if let Some(r) = int_ref(id) {
            self.write_i(t, r, v as i32);
        }
    }

    // Jump (07 §7 op 4): clock = t; ip = this instruction + byte offset.
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
    // Mode 1 polar (07 §8): angle wrapped; `dur` 0 = no duration.
    fn mode1(&mut self, ang: f32, spd: f32, dur: i32) {
        let e = &mut *self.e;
        e.angle = anmvm::wrap_add(ang, 0.0);
        e.speed = spd;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.set_move_mode(1);
    }

    // Mode 2 interpolate (07 §8): velocity 0, ease in flag A bits 14–16.
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

    // Ops 66 / 67 / 178 (07 §7, §8.3): dur < 1 → mode 1; else mode 2 from the final pos. `mirror`: op 66
    // negates delta.x for mirrored enemies (`Ecl_OpMoveDir` 0x420D10); ops 67/178 do not.
    fn move_dir(&mut self, dur: i32, ease: i32, ang: f32, spd: f32, mirror: bool) {
        if dur < 1 {
            self.mode1(ang, spd, 0);
        } else {
            // 05 §10 trig sites (0x420DAE / 0x422315): f32(f32(cos a·spd)·dur), cos·spd rounded once.
            let v = crate::bullet::polar(ang, spd);
            let mut delta = [v[0] * dur as f32, v[1] * dur as f32, 0.0];
            if mirror && self.e.flags & A_MIRROR != 0 {
                delta[0] = -delta[0];
            }
            self.mode2(dur, ease, self.e.final_pos, delta);
        }
    }

    fn mode3(&mut self, dur: i32) {
        let e = &mut *self.e;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.set_move_mode(3);
    }

    // Random direction 0x422020 (op 67) / 0x4224A0 (op 178), 07 §8.3; one Rng_Float per line, in order.
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
                // Quirk kept (07 §8.3): uses the enemy's current angle, not a.
                if (0.0..PI / 2.0).contains(&a) {
                    a = PI - e.angle;
                } else if a > -PI / 2.0 && a < 0.0 {
                    a = -PI - a;
                }
            }
        }
        // 07 §8.3: two sequential tests; when the bands overlap both can flip, so the move goes up.
        if y < y0 + 48.0 && a < 0.0 {
            a = -a;
        }
        if y > y1 - 48.0 && a > 0.0 {
            a = -a;
        }
        a
    }

    // --- ANM ---
    // Main VM script from enemy.anm or stgNenm.anm by flags B bit 2 (07 §7 ops 54/58/62, §9.3 direction scripts);
    // started the AnmFile_SetScript way, so VM pos and pos offset are kept.
    fn start_main(&mut self, script: i32) {
        let slot = self.e.anm_slot();
        if script < 0 {
            self.e.vm = AnmVm::default();
            return;
        }
        self.anm
            .file_set_script(&mut self.e.vm, slot, script as usize);
    }

    // Ops 57/61: script < 0 disables the sub VM; else AnmFile_SetScript (pos / pos offset kept, 07 §7).
    fn start_sub(&mut self, k: usize, script: i32) {
        let slot = self.e.anm_slot();
        let Some(vm) = self.e.sub_vms.get_mut(k) else {
            return;
        };
        if script < 0 {
            *vm = AnmVm::default();
        } else {
            self.anm.file_set_script(vm, slot, script as usize);
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
        // 07 §10.5: fails (manager flag) if the parent's life < 1 or it is being chain-killed.
        if chained && (self.e.life < 1 || self.e.flags & A_CHAIN_KILLED != 0) {
            self.m.full = true;
            return;
        }
        // 07 §10.5: ops 90/92 spawn at (x, y, 0) as given; op 92's follow offset is written after the spawn run.
        let pos = match op {
            91 => [self.e.final_pos[0] + x, self.e.final_pos[1] + y, 0.0],
            94 => [self.e.pos[0] + x, self.e.pos[1] + y, self.e.pos[2] + z],
            93 => [x, y, z],
            _ => [x, y, 0.0],
        };
        // 07 §10.5: the tail is looked up before the spawn. A chain root (no parent) links after its last child;
        // a chained child links after itself, overwriting its own +8 (the rest of the chain is cut off).
        let tail = if chained && self.m.enemies[self.idx].root.is_none() {
            let mut last = self.idx;
            let mut guard = 0;
            while let Some(i) = self.m.enemies[last].chain_next {
                guard += 1;
                if guard > MAX_ENEMIES {
                    break;
                }
                last = i;
            }
            last
        } else {
            self.idx
        };
        let Some(ci) = self.m.spawn_prepare(sub, pos, life, false) else {
            return;
        };
        // Sound 0x24 (07 §10.5).
        if chained {
            crate::sound::play_at(crate::sound::OPTION, self.e.final_pos[0]);
        }
        // 07 §4.1 step 3: the 30 variable dwords of the spawning context.
        let vars = t.ctx.vars;
        let youkai = self.m.g.player_mode != 0;
        {
            let c = &mut self.m.enemies[ci];
            // Ops 90–92 never write the mode bits e+0x3330 (07 §10.5): the spawn-time run sees 0, so both human-
            // and youkai-masked time-0 instructions run; Enemy_ReactPlayerMode sets them at the first step 3.
            c.main.ctx.vars = vars;
        }
        if self
            .m
            .spawn_finish(self.anm, ci, life, item, score, true)
            .is_none()
            || !chained
        {
            return;
        }
        // 07 §10.5: applied after the spawn-time Ecl_Run, overriding the child's own time-0 ops.
        let c = &mut self.m.enemies[ci];
        c.flags |= A_MODE_REACT;
        c.set_flag(A_YOUKAI, youkai);
        c.layer = if youkai { 0 } else { 2 };
        // Op 92 (07 §10.5): follow offset := parent pos and final pos, after the run, so time 0 saw neither.
        if op == 92 {
            c.pos_offset = self.e.pos;
            c.final_pos = add3(c.pos_offset, c.pos);
        }
        c.flags &= !A_CONTACT;
        // Aura 0x20 only if it has none yet, at pos (op 92: final pos); interrupt 1 human / 2 youkai.
        if c.fx_aura == crate::effect::NONE {
            let at = if op == 92 { c.final_pos } else { c.pos };
            let contact = c.flags & A_CONTACT != 0;
            let odd = c.slot & 1 != 0;
            let fx = crate::effect::spawn_high(self.anm, 0x20, at, 0xFFFFFFFF);
            crate::effect::get(fx, |x| {
                x.vm.pending_interrupt = if youkai { 2 } else { 1 };
                // VM bit 17 := flags A bit 2 (just cleared); odd slot negates VM +0x14 (z angular velocity).
                x.vm.flags =
                    (x.vm.flags & !anmvm::F_COLOR2) | if contact { anmvm::F_COLOR2 } else { 0 };
                if odd {
                    x.vm.angular_vel[2] = -x.vm.angular_vel[2];
                }
            });
            c.fx_aura = fx;
        }
        // Op 92: bit 9 (follow) after the aura, before the link (07 §10.5).
        if op == 92 {
            c.flags |= A_FOLLOW;
        }
        // The chain link is written last (07 §10.5): during the spawn-time run the child has no parent.
        c.root = Some(self.idx);
        c.chain_prev = Some(tail);
        self.m.enemies[tail].chain_next = Some(ci);
        if tail == self.idx {
            self.e.chain_next = Some(ci);
        }
        self.e.children += 1;
    }

    // --- bullets (07 §9) ---
    // Bullet block (07 §9.1): fields over the enemy's current descriptor; mask bits 0–7, flags raw.
    // Read order of 07 §9.1 step 3 (each variable read may draw); colour is read last (step 5).
    fn decode_bullet(&mut self, t: &Thread, b: &[u8]) -> BulletDesc {
        let mask = u16::from_le_bytes([b[10], b[11]]);
        let mb = |k: u32| mask & (1 << k) != 0;
        let mut d = self.e.desc.clone();
        d.kind = self.vi(t, rd_i16(b, 0x0C) as i32, mb(0)) as i16;
        d.count1 = self.vi(t, rd_u32(b, 0x10) as i32, mb(2)) as i16;
        d.count2 = self.vi(t, rd_u32(b, 0x14) as i32, mb(3)) as i16;
        d.angle1 = self.vf(t, f32::from_bits(rd_u32(b, 0x20)), mb(6));
        d.speed1 = self.vf(t, f32::from_bits(rd_u32(b, 0x18)), mb(4));
        d.angle2 = self.vf(t, f32::from_bits(rd_u32(b, 0x24)), mb(7));
        d.speed2 = self.vf(t, f32::from_bits(rd_u32(b, 0x1C)), mb(5));
        d.aim_mode = rd_i16(b, 4) - 96;
        d.flags = rd_u32(b, 0x28);
        d.colour = self.vi(t, rd_i16(b, 0x0E) as i32, mb(1)) as i16;
        d
    }

    // Ecl_FireBullets 0x422720 (07 §9.1): only while life > 0; flag A bit 17 stores the instruction instead.
    fn bullet_op(&mut self, t: &Thread, ins: &Ins) {
        if self.e.life <= 0 {
            return;
        }
        let bytes = self.m.ecl.bytes(ins).to_vec();
        if self.e.flags & A_STORE_FIRE != 0 {
            self.e.stored_fire = Some(bytes);
            return;
        }
        self.fire_bullets(t, &bytes);
    }

    // Ecl_FireBullets body (07 §9.1 steps 1–5), shared by the op and the auto-fire copy.
    // The filters test the raw instruction before any argument is read: a skipped instruction draws no RNG.
    fn fire_bullets(&mut self, t: &Thread, b: &[u8]) {
        let flags = rd_u32(b, 0x28);
        // Mode filter on the enemy's flag A bit 11, not the player's mode (07 §9.1 step 1).
        let youkai = self.e.flags & A_YOUKAI != 0;
        if (flags & 0x8000 != 0 && !youkai) || (flags & 0x10000 != 0 && youkai) {
            return;
        }
        let p = self.m.g.player;
        let e = &*self.e;
        let dd = (e.final_pos[0] - p[0]).powi(2) + (e.final_pos[1] - p[1]).powi(2);
        if e.nofire_r2 > 0.0 && dd < e.nofire_r2 {
            return;
        }
        self.e.desc = self.decode_bullet(t, b);
        self.fire_desc();
    }

    // Op 109 fire_stored (07 §7): fires the stored descriptor; keeps the pre-RE-Q53 filters on the stored flags.
    // SPEC-GAP: does op 109 run the mode / no-fire filters and the rank add (IMPL121 Q1).
    fn fire_stored(&mut self) {
        let e = &*self.e;
        let flags = e.desc.flags;
        let youkai = self.m.g.player_mode != 0;
        if (flags & 0x8000 != 0 && !youkai) || (flags & 0x10000 != 0 && youkai) {
            return;
        }
        let p = self.m.g.player;
        let dd = (e.final_pos[0] - p[0]).powi(2) + (e.final_pos[1] - p[1]).powi(2);
        if e.nofire_r2 > 0.0 && dd < e.nofire_r2 {
            return;
        }
        self.fire_desc();
    }

    // Origin and rank scaling (07 §9.1–9.2), then BulletMgr_SpawnPattern.
    fn fire_desc(&mut self) {
        let e = &*self.e;
        let mut d = e.desc.clone();
        d.origin = add3(e.final_pos, e.shot_offset);
        if self.m.g.spell.is_none() {
            let rank = crate::gamemgr::rank();
            let [c1a, c1b, c2a, c2b] = e.rank_counts;
            let [s0, s1] = e.rank_speed;
            d.count1 = (d.count1 as i32 + lerp_i(c1a, c1b, rank)).max(1) as i16;
            d.count2 = (d.count2 as i32 + lerp_i(c2a, c2b, rank)).max(1) as i16;
            let s = lerp_f(s0, s1, rank);
            if d.speed1 != 0.0 {
                d.speed1 = (d.speed1 + s).max(0.3);
            }
            // 07 §9.2: speed2's add is unconditional (the speed1 == 0 branch jumps to it).
            d.speed2 = (d.speed2 + s / 2.0).max(0.3);
        }
        let p = self.m.g.player;
        self.m.bullets.borrow_mut().spawn_pattern(self.anm, &d, p);
    }

    // Laser block (07 §9.4, 64 bytes) → laser slot[current]; +0x34/+0x38/+0x3C raw.
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

    // Runs a manager-wide op (KillAll, Spell_End) with the running enemy and its running main context back in
    // its slot, so the loop sees the caller in slot order (07 §7 op 95, §10.2: no self test).
    // Quirk kept (07 §7 op 95): Ecl_Run steps a local copy of the ip, so a death-sub `Ecl_SetSub` on the caller's
    // own main context keeps only clock/wait 0 and the sub id; the old sub continues and the death sub never runs.
    // From a thread the main context keeps the death sub and the thread goes on.
    fn with_self_in_slot(
        &mut self,
        t: &mut Thread,
        f: impl FnOnce(&mut EnemyManager, &mut AnmManager),
    ) -> Flow {
        let in_main = self.e.running_thread.is_none();
        let local_ip = t.ctx.ip;
        if in_main {
            std::mem::swap(t, &mut self.e.main);
        }
        std::mem::swap(self.e, &mut *self.m.enemies[self.idx]);
        f(self.m, self.anm);
        std::mem::swap(self.e, &mut *self.m.enemies[self.idx]);
        // `put` takes the links from the slot: keep any change made while the caller was in it.
        let (e, p) = (&*self.e, &mut self.m.enemies[self.idx]);
        (p.chain_prev, p.chain_next, p.root) = (e.chain_prev, e.chain_next, e.root);
        if in_main {
            std::mem::swap(t, &mut self.e.main);
        }
        t.ctx.ip = local_ip;
        Flow::Next
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

    // Ops 142 / 168 (07 §7): offsets Rng·128 − 64 on x then y; 142 below full power: first type 2, rest 0.
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

    // Enemy_AutoFireAndDirAnim 0x423150 (07 §9.3).
    fn autofire_and_dir_anim(&mut self, t: &mut Thread) {
        let gs = self.anm.speed;
        if self.e.life > 0 && self.e.autofire_interval > 0 {
            self.e.autofire_timer.tick(gs);
            if self.e.autofire_timer.cur >= self.e.autofire_interval {
                if let Some(b) = self.e.stored_fire.clone() {
                    self.fire_bullets(t, &b);
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
        // Swapped when mirrored.
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

    // Variable interpolation slots (07 §5.6); a position target turns into velocity + angle, pos restored.
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
            let v = sl.abcd;
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

    // --- native functions (07 §12): op 136 once, op 137 every POST; `arg` = instr +0x10 ---
    fn native(&mut self, t: &mut Thread, n: i32, arg: i32) {
        let gs = self.anm.speed;
        match n {
            0 => crate::ascii::set_darkness(t.ctx.int(V_INT0), t.ctx.flt(V_FLT0)),
            // Bounce: vars 10022 / 10023 are the gravity step and the fall-speed cap.
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
            // Screen flash / shake parameters (07 §12, 13 §3).
            1 => crate::effect::screen(3, 60, 1, 0xFFFFFFFFu32 as i32, 0, 0x15),
            17 => crate::effect::screen(3, 180, 1, 0xFFFFFFFFu32 as i32, 0, 0x15),
            10 => {
                crate::effect::screen(3, 30, 5, 0x40FFFFFF, 0, 0x15);
                crate::effect::screen(7, 4, 120, 190, 60, 0x15);
            }
            15 => crate::effect::screen(7, 16, 20, 20, 20, 0x15),
            // Two effects (0x38 / 0x3A / 0x41) with an extra script and the stage callback 0x4235A0.
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
            // 07 §12 row 19: the spell-practice card id (i16 0x164D0B8, sign-extended), not the running Spell's id.
            19 => t.ctx.set_int(V_INT0, self.m.g.card as i32),
            // 07 §12 row 24: stats +0x1C, spell cards captured in this game (11 §5).
            24 => {
                let n = self.m.player.borrow().stats.spells;
                t.ctx.set_int(V_INT0, n);
            }
            // Freeze byte 0x160F534 := arg; stage VMs get interrupt 2, or 1 when arg is 0 (07 §12).
            26 => {
                set_timer_pause(arg);
                self.stage_interrupt(if arg != 0 { 2 } else { 1 });
            }
            // Bullets with flag 0x100000 spawn enemy sub = var 10038 (life 800, item −2, score 10, var 10016 = angle).
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
            // Natives 12/14 (08 §7): mask = var 10000, angle/speed = vars 10016/10017; 12 also toggles flags B
            // bit 7 on the chain by var 10001 and interrupts the spell-background VMs.
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
            // Slow-down 0x4251B0 (07 §12): game speed 1/arg first, interrupt 2, then live bullets' velocities ×
            // the new speed, +0x218 := sprite, sprites 0x60–0x6F → 0x6F (VM0 only).
            28 if arg != 0 => {
                self.anm.speed = 1.0 / arg as f32;
                self.stage_interrupt(2);
                let k = self.anm.speed;
                self.m.bullets.borrow_mut().slow_down(self.anm, k);
            }
            // Undo 28 0x425290 (07 §12): velocities × 1/speed, sprites 0x60–0x6F := +0x218; Supervisor flag 0x20
            // when 1/arg < 1; then speed 1 and interrupt 1.
            29 => {
                let f = 1.0 / self.anm.speed;
                self.m.bullets.borrow_mut().slow_down_restore(self.anm, f);
                if 1.0 / (arg as f32) < 1.0 {
                    anmvm::set_slowdown_restore(true);
                }
                self.anm.speed = 1.0;
                self.stage_interrupt(1);
            }
            30 => set_load_frames(arg),
            // Type 3 (bomb) while the player is bombing (player +0xFDC = 0x17D6ED4), else 5 (1up) (07 §12).
            31 => {
                let kind = if self.m.player.borrow().bombing { 3 } else { 5 };
                self.item(self.e.pos, kind);
            }
            22 => {
                // 07 §11.1 bit 10: Spell_ShowNameOnly passes face −1 (sprite 1): no portrait, no cut-in.
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
            // Bullet warp (07 §12).
            4 => self.warp(&Warp::native4()),
            7 => self.warp(&Warp::native7()),
            21 => self.warp(&Warp::native21()),
            // Rotated 590-long player-hit boxes, heights big / small (07 §12); native 11 starts at pos, not final pos.
            9 => self.laser_box(t, self.e.final_pos, 160.0, 128.0),
            11 => self.laser_box(t, self.e.pos, 240.0, 192.0),
            25 => self.laser_box(t, self.e.final_pos, 288.0, 224.0),
            // Stage tint request 0xFFC03030 (07 §12, 06 §10.2 step 3).
            // Only Stage_RequestTint; +0x646C is not written (07 §12 row 13).
            13 => self.anm.request_tint(0xFFC03030),
            16 => self.mark_children(t),
            _ => {
                let _ = gs;
                self.m.g.misc_calls += 1;
            }
        }
    }

    // Natives 26 / 28 / 29 "stage VMs interrupt k" (07 §12).
    // 07 §12 rows 26/28/29: g_Stage +0xB38 / +0xDDC, the spell-background pair of native 12.
    fn stage_interrupt(&mut self, k: u16) {
        if let Some(st) = &self.m.stage {
            st.borrow_mut().spell_interrupt(k);
        }
    }

    fn warp(&mut self, w: &Warp) {
        self.m.bullets.borrow_mut().warp(w);
    }

    // Natives 9 / 11 / 25 0x424730 / 0x424820 / 0x424910 (07 §12): Player_LaserTest from origin
    // o = p − (float vars 10016, 10017) along main VM rotation z; small box with graze when the e+0x2E14 timer
    // changed to a multiple of 12, then the big box without graze on every call.
    fn laser_box(&mut self, t: &Thread, p: [f32; 3], big: f32, small: f32) {
        let o = [p[0] - t.ctx.flt(V_FLT0), p[1] - t.ctx.flt(V_FLT0 + 1)];
        let a = self.e.vm.rotation[2];
        let c = [590.0 / 2.0 + o[0], o[1]];
        let bt = &self.e.boss_timer;
        let graze = bt.cur != bt.prev && bt.cur % 12 == 0;
        let mut pl = self.m.player.borrow_mut();
        let env = pl.env;
        let mut orbs = 0;
        if graze {
            orbs = pl
                .laser_test(c, [590.0, small], o, a, true, &env, &mut self.anm.rng)
                .1;
        }
        pl.laser_test(c, [590.0, big], o, a, false, &env, &mut self.anm.rng);
        let mut b = self.m.bullets.borrow_mut();
        if std::mem::take(&mut pl.drop_attracted) {
            b.items.drop_attracted();
        }
        // Player_OnGraze drops its time orbs (item type 10) at the point it was given: the player pos.
        for _ in 0..orbs {
            b.items.spawn(self.anm, &pl, pl.pos, 10, 0);
        }
    }

    // Native 16 `EclNative_16` 0x424E50 (07 §12 row 16): per flag-0x100000 bullet in slot order, a member of the
    // caller's chain with int var 10038 == 0 and pos within 64 px (3D, strict) gets 10038 := 60 and the caller's
    // int var 10007 (bullet colour of ecldata6 sub77).
    fn mark_children(&mut self, t: &Thread) {
        let col = t.ctx.int(V_INT0 + 7);
        let near: Vec<[f32; 3]> = self.m.bullets.borrow().bullets[..crate::bullet::MAX_BULLETS]
            .iter()
            .filter(|b| !b.free() && b.flags & 0x100000 != 0)
            .map(|b| b.pos)
            .collect();
        for p in near {
            let mut c = self.e.chain_next;
            let mut guard = 0;
            while let Some(i) = c
                && guard <= MAX_ENEMIES
            {
                guard += 1;
                let x = &mut self.m.enemies[i];
                let d = [p[0] - x.pos[0], p[1] - x.pos[1], p[2] - x.pos[2]];
                // Vec3_LengthSq order z·z + y·y + x·x.
                if x.main.ctx.int(V_INT1 + 2) == 0
                    && d[2] * d[2] + d[1] * d[1] + d[0] * d[0] < 4096.0
                {
                    x.main.ctx.set_int(V_INT1 + 2, 60);
                    x.main.ctx.set_int(V_INT0 + 7, col);
                }
                c = x.chain_next;
            }
        }
    }

    // Native 8 `EclNative_8` 0x4244F0 (07 §12): familiar ring. Members are the parent's +8 chain (caller
    // included, no alive test) whose int var 10038 equals the caller's; each gets int var 10037 := its index.
    // Caller's int vars: 10005 count-changed flag, 10006 count, 10007 call counter. Non-leaders turn their orbit
    // angle 2 % toward leader angle + idx·2π/k (+ leader orbit velocity when the 10007 counters differ).
    fn familiar_ring(&mut self, t: &mut Thread) {
        const TWO_PI: f32 = 6.2831855;
        let Some(parent) = self.e.root else {
            return;
        };
        let tag = t.ctx.int(V_INT1 + 2);
        let mut k = 0;
        let mut leader = None;
        let mut n = self.m.enemies[parent].chain_next;
        let mut guard = 0;
        while let Some(i) = n
            && guard <= MAX_ENEMIES
        {
            guard += 1;
            if i == self.idx {
                t.ctx.set_int(V_INT1 + 1, k);
                leader.get_or_insert(i);
                k += 1;
                n = self.e.chain_next;
                continue;
            }
            // e+0x2CA0 outside its own Ecl_Run is the main context (07 §4.3, §12 row 8).
            let c = &mut self.m.enemies[i].main.ctx;
            if c.int(V_INT1 + 2) == tag {
                c.set_int(V_INT1 + 1, k);
                leader.get_or_insert(i);
                k += 1;
            }
            n = self.m.enemies[i].chain_next;
        }
        t.ctx.set_int(V_INT0 + 5, 0);
        let count = t.ctx.int(V_INT0 + 6);
        if count != k {
            if count != 0 {
                t.ctx.set_int(V_INT0 + 5, 1);
            }
            t.ctx.set_int(V_INT0 + 6, k);
        }
        let calls = t.ctx.int(V_INT0 + 7).wrapping_add(1);
        t.ctx.set_int(V_INT0 + 7, calls);
        let idx = t.ctx.int(V_INT1 + 1);
        // 07 §12 row 8: a caller outside the ring keeps its old 10037; k = 0 is UB in the original (unreachable), nothing here.
        let Some(l) = leader.filter(|_| idx != 0) else {
            return;
        };
        let (l_angle, l_av, l_calls) = if l == self.idx {
            (self.e.orbit_angle, self.e.orbit_av, calls)
        } else {
            let e = &self.m.enemies[l];
            (e.orbit_angle, e.orbit_av, e.main.ctx.int(V_INT0 + 7))
        };
        let mut target = anmvm::wrap_add(l_angle, idx as f32 * TWO_PI / k as f32);
        if l_calls != calls {
            target = anmvm::wrap_add(target, l_av);
        }
        let mut d = target - anmvm::wrap_add(self.e.orbit_angle, self.e.orbit_av);
        if d.abs() > PI {
            d += if d > 0.0 { -TWO_PI } else { TWO_PI };
        }
        self.e.orbit_angle = anmvm::wrap_add(self.e.orbit_angle, d * 0.02);
    }

    // Op 122 spell block (07 §11): decoded strings, bonus / max / decay rate, pending payout paid first
    // (07 §11.1), bullets cancelled, spell background, rank ranges reset.
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
        let bonus = rd_u32(b, 0x10);
        // Rate R = (bonus − bonus/7) / (timeout/60), unsigned; op 134 precedes op 122 in all data.
        let secs = (self.e.timeout / 60).max(1) as u32;
        // 07 §11.1: +0xFC := card bonus; cap +0x2638 := card bonus, or 99 999 990 if flag A bit 27 is already set.
        let max = if self.e.flags & A_FIXED_BONUS != 0 {
            FIXED_BONUS_CAP
        } else {
            bonus as i32
        };
        let spell = Spell {
            face: rd_i16(b, 0x0C) as i32,
            id: rd_i16(b, 0x0E) as i32,
            bonus: bonus as i32,
            max,
            decay: ((bonus - bonus / 7) / secs) as i32,
            boss: self.idx,
            // g_Gui +0xC := boss e+0x2E0C, written only by Spell_Start (07 §11.1).
            slot: self.e.slot,
            name: dec(0x14, 48, 0xAA),
            owner: dec(0x44, 48, 0xBB),
            comment: [dec(0x74, 64, 0xDD), dec(0xB4, 64, 0xEE)],
            frames: 0,
            duration: self.e.timeout.max(0),
            deaths0: self.m.player.borrow().deaths,
            bombs0: self.m.player.borrow().bombs_started,
            capture: true,
            timed_out: false,
            bombed: false,
            last: crate::gamemgr::flags() & crate::gamemgr::F_LAST_SPELL != 0,
        };
        log::info!(
            "spell card {} ({} bytes name, bonus {}) at enemy frame {}",
            spell.id,
            spell.name.len(),
            spell.bonus,
            self.m.frame
        );
        self.m.pay_capture(self.anm, false);
        self.m.g.fixed_bonus = self.e.flags & A_FIXED_BONUS != 0;
        (self.m.g.spell_flag6, self.m.g.bonus_frozen) = (false, false);
        // Bit 9 := 0 is Spell_Start's first statement (07 §11.1).
        self.m.g.spell_captured = false;
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
        // Ring at the boss pos e+0x2D34 (not the final pos); type 52 while GameFlags bits 7–8 ≠ 0 (07 §11.1).
        let special = self.m.player.borrow().special();
        crate::effect::spell_ring(self.anm, self.e.pos, special);
        self.cancel_all(1);
        self.e.reset_rank_ranges();
    }

    // Opcode switch (07 §7 table; unknown ops are ignored).
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
                let c = self.ai_dst(t, ins, 2).wrapping_sub(1);
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
            // Random sign: one Rng_Next16, bit 0.
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
                    (self.ai_dst(t, ins, 0), self.ai(t, ins, 1))
                } else {
                    (self.ai(t, ins, 1), self.ai(t, ins, 2))
                };
                let v = match op % 10 {
                    0 => a.wrapping_add(b),
                    1 => a.wrapping_sub(b),
                    2 => a.wrapping_mul(b),
                    // C truncating / and %; the 0 result for b == 0 is a port guard.
                    3 => a.checked_div(b).unwrap_or(0),
                    _ => a.checked_rem(b).unwrap_or(0),
                };
                self.set_i(t, ins, 0, v);
            }
            15..=19 | 25..=29 => {
                let (a, b) = if op < 25 {
                    (self.af_dst(t, ins, 0), self.af(t, ins, 1))
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
                let a = self.ai_dst(t, ins, 0);
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
            // Target id is a float literal; reuse the slot with the same target, else the first free one.
            36 => {
                let target = f32::from_bits(self.raw(ins, 0)) as i32;
                let dur = self.ai(t, ins, 1);
                let func = self.ai(t, ins, 2);
                let mode = self.ai(t, ins, 3);
                // 07 §5.6: a–d are resolved once, at op time.
                let abcd: [f32; 4] = std::array::from_fn(|k| self.af(t, ins, 4 + k));
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
                let a = self.af_dst(t, ins, 0);
                self.set_f(t, ins, 0, anmvm::wrap_add(a, 0.0));
            }
            // 38 wraps the angle and writes x first; 166 does not wrap and writes y first.
            38 | 166 => {
                let (ang, r) = (self.af(t, ins, 2), self.af(t, ins, 3));
                let ang = if op == 38 {
                    anmvm::wrap_add(ang, 0.0)
                } else {
                    ang
                };
                // `call; fmul` (05 §10 trig sites): f32(cos·r), one rounding.
                let v = crate::bullet::polar(ang, r);
                if op == 38 {
                    self.set_f(t, ins, 0, v[0]);
                    self.set_f(t, ins, 1, v[1]);
                } else {
                    self.set_f(t, ins, 1, v[1]);
                    self.set_f(t, ins, 0, v[0]);
                }
            }
            // Even ops int, odd ops float compares (NaN taken as Greater).
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
                    self.loaded_ip = t.ctx.ip;
                    return Flow::Jumped;
                }
            }
            // 0x421CB0 (07 §5.4): restore the whole context from stack[depth].
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
                self.loaded_ip = t.ctx.ip;
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
            // delta = target − final pos, start = pos (07 §7).
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
                self.move_dir(dur, ease, a, s, true);
            }
            67 | 178 => {
                let (dur, ease, s) = (self.ai(t, ins, 0), self.ai(t, ins, 1), self.af(t, ins, 2));
                let a = self.random_dir(op == 178);
                self.move_dir(dur, ease, a, s, false);
            }
            // Movement mode unchanged.
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
                    // Quirk kept (07 §7 op 69): with a duration the movement is not aimed (same as op 66).
                    self.move_dir(dur, ease, off, s, true);
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
                    // The aura VM's colour2 flag (bit 17) follows contact damage (07 §7 ops 80/81).
                    crate::effect::get(e.fx_aura, |x| {
                        x.vm.flags =
                            (x.vm.flags & !anmvm::F_COLOR2) | if on { anmvm::F_COLOR2 } else { 0 };
                    });
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
            // Variable `var` evaluated once, with the op's own getter, on boss[slot]'s main context (07 §7):
            // 87 only if the boss exists; 86 has no such test.
            86 | 87 => {
                let (var, slot) = (self.ai(t, ins, 1), self.ai(t, ins, 2));
                // 07 §7 op 86: an empty entry faults in the original for enemy/context ids (0 uses); skipped as a stand-in.
                if let Some(b) = self.m.boss(slot) {
                    let other = (b != self.idx).then(|| self.m.enemies[b].clone());
                    let (e, ctx) = match &other {
                        Some(o) => (&**o, &o.main.ctx),
                        None => (&*self.e, &t.ctx),
                    };
                    if op == 86 {
                        let v = var_i(self.m, self.anm, e, ctx, var);
                        self.set_i(t, ins, 0, v);
                    } else {
                        let v = var_f(self.m, self.anm, e, ctx, var as f32);
                        self.set_f(t, ins, 0, v);
                    }
                }
            }
            // Call in boss[slot]'s main context the same way (07 §5.4).
            88 => {
                let (slot, sub) = (self.ai(t, ins, 0), self.ai(t, ins, 1));
                if let Some(b) = self.m.boss(slot)
                    && self.m.ecl.sub_start(sub).is_some()
                {
                    let args = self.m.g.args;
                    let ecl = &self.m.ecl;
                    if b == self.idx {
                        // 07 §5.4 on the caller itself: the running context, its own stack and working depth; the
                        // pushed ip is the stored ip + its size. Ecl_Run then writes its local ip (past op 88) back
                        // over the sub start, so the sub never runs; clock, wait, sub id, args and the push stay.
                        let cur = t.ctx.ip;
                        t.ctx.ip = self.loaded_ip + ecl.ins(self.loaded_ip).map_or(0, |i| i.size);
                        if self.e.flags & A_NO_PUSH == 0 {
                            t.push();
                        }
                        t.ctx.enter(ecl, sub);
                        t.ctx.vars[V_ARGI..V_ARGI + 8].copy_from_slice(&args);
                        t.ctx.ip = cur;
                        return Flow::Next;
                    }
                    let o = &mut self.m.enemies[b];
                    // 07 §5.4: Ecl_CallSub gets the target's stored ip, so the instruction it would run next is
                    // skipped on return; the push goes to the working depth 0x2CEA, the main depth 0x2CE8 stays.
                    let ip = o.main.ctx.ip;
                    o.main.ctx.ip = ip + ecl.ins(ip).map_or(0, |i| i.size);
                    if o.flags & A_NO_PUSH == 0 {
                        o.work_depth = o.main.push_at(o.work_depth);
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
                return self.with_self_in_slot(t, |m, anm| {
                    m.kill_all(anm);
                });
            }
            96..=104 => self.bullet_op(t, ins),
            // interval = n + lerp_i(n/5, −n/5) (07 §7, §9.2); 106 starts the timer at Rng_Range(interval).
            105 | 106 => {
                let n = self.ai(t, ins, 0);
                let iv = n + lerp_i(n / 5, -n / 5, crate::gamemgr::rank());
                self.e.autofire_interval = iv;
                let start = if op == 106 { self.anm.rng.range(iv) } else { 0 };
                self.e.autofire_timer.set(start);
            }
            107 => self.e.flags |= A_STORE_FIRE,
            108 => self.e.flags &= !A_STORE_FIRE,
            109 => self.fire_stored(),
            110 => self.e.shot_offset = [self.af(t, ins, 0), self.af(t, ins, 1), 0.0],
            // Ex slot k = {e, f, c, d, a, b}; ints stored raw, every argument variable-capable (07 §7).
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
            // id < 0: descriptor flag 0x200 off; desc+0x204 = x either way.
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
            // Result in var 10038.
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
            123 => return self.with_self_in_slot(t, |m, anm| m.finish_spell(anm)),
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
            // Boss table 0xF54CC0 (07 §7 op 127): no-fire radius 0; slot 0 turns on the HUD boss display.
            127 => {
                let slot = self.ai(t, ins, 0);
                if slot < 0 {
                    self.m.unregister_boss(self.idx);
                    self.e.flags &= !A_BOSS;
                    // ENEMY marker fade-out (12 §5.10).
                    if (0..4).contains(&self.e.boss_slot) {
                        crate::ascii::marker_release(self.e.boss_slot as usize);
                    }
                } else if let Some(b) = self.m.bosses.get_mut(slot as usize) {
                    if slot < 4 {
                        crate::ascii::marker_interrupt(slot as usize, 1);
                    }
                    *b = Some(self.idx);
                    self.e.flags |= A_BOSS;
                    self.e.boss_slot = slot;
                    self.e.nofire_r2 = 0.0;
                    if slot == 0 {
                        self.m.g.hud_boss = true;
                        // HUD +0x30 := 1.0 (12 §5.1).
                        self.m.g.hud_bar_target = 1.0;
                    }
                }
            }
            // Effect 13 (0xFF6060D0) kept in e+0x5360[] and updated every frame (follow, grow to d, spin).
            // 07 §5 op 128, 13 §2.6b: a, b, c, d are copied raw (mask ignored, d = f32 bits into e+0x53C4) and
            // the effect spawns at the base pos e+0x2D34, not the final pos.
            128 => {
                self.e.effects += 1;
                let w = |k| f32::from_bits(self.raw(ins, k));
                let axis = [w(1), w(2), w(3)];
                self.e.orbit_target = w(4);
                let i = crate::effect::spawn(self.anm, 13, self.e.pos, 1, 0xFF6060D0);
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
                // Interrupt (player mode ≠ 0) + 1; odd slot negates VM +0x14; colour2 untouched (07 §7 op 174).
                let interrupt = (self.m.g.player_mode != 0) as u16 + 1;
                let odd = self.e.slot & 1 != 0;
                crate::effect::get(self.e.fx_aura, |x| {
                    x.vm.pending_interrupt = interrupt;
                    if odd {
                        x.vm.angular_vel[2] = -x.vm.angular_vel[2];
                    }
                });
            }
            // Spell-practice guard (0x164D0B4 bit 14 and bits 7–8 ≠ 0): 129/130 ignored, 133/134 keep the sub (07 §5).
            129 if !practice_guard() => {
                let m = self.raw(ins, 0) & 0xFF;
                self.e.flags = (self.e.flags & !(7 << A_DEATH_SHIFT)) | ((m & 7) << A_DEATH_SHIFT);
            }
            130 if !practice_guard() => self.e.death_sub = self.raw(ins, 0) as i16 as i32,
            129 | 130 => {}
            // life = max = HUD life; boss slot 0 resets the 8 HUD markers.
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
                    if !practice_guard() {
                        self.e.threshold_subs[k] = sub;
                    }
                }
            }
            // Timer 0 here, so op 134 on frame F times out on frame F + timeout (07 §10.2).
            134 => {
                self.e.timeout = self.ai(t, ins, 0);
                let sub = self.ai(t, ins, 1);
                if !practice_guard() {
                    self.e.timeout_sub = sub;
                }
                self.e.boss_timer.set(0);
            }
            // Free thread slot k; sub ≥ 0 creates it with the current context's 30 var dwords, clock 0 (07 §5.4).
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
            // Context clock += n.
            146 => {
                let n = self.ai(t, ins, 0);
                t.ctx.clock.cur += n;
            }
            // Stage section request (06 §4.1).
            147 => {
                let n = self.ai(t, ins, 0);
                if let Some(s) = &self.m.stage {
                    s.borrow_mut().section_request = n;
                }
            }
            // HUD 0x423130(n); the 0x164D30C += 1800 happens in the HUD request handler (07 §7 op 148).
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
                // 07 op 155: whatever the byte, 0x4ECCA8 = g_Gui +0x2638 (running card's cap) := 99 999 990.
                if let Some(s) = &mut self.m.g.spell {
                    s.max = FIXED_BONUS_CAP;
                }
            }
            156 => {
                self.e
                    .set_flag(A_CANCEL_BONUS, self.raw(ins, 0) & 0xFF != 0);
                self.e.layer = 2;
            }
            157 => {
                // u8 flags and i16 n / a / b fields (07 §4.3 e+0x534C–0x5352).
                self.e.trail = [
                    (self.raw(ins, 0) & 0xFF) as i32,
                    self.ai(t, ins, 1) as i16 as i32,
                    self.ai(t, ins, 2) as i16 as i32,
                    self.ai(t, ins, 3) as i16 as i32,
                ];
            }
            158 => {
                // 07 op 158: word 1 → start +0x5C, word 2 → end +0x3C, as fractions of this enemy's max life.
                let k = self.ai(t, ins, 0);
                let max = self.e.max_life as f32;
                let (a, b) = (self.ai(t, ins, 1) as f32, self.ai(t, ins, 2) as f32);
                let c = self.ai(t, ins, 3) as u32;
                if let Some(s) = self.m.g.hud_markers.get_mut(k as usize) {
                    *s = (b / max, a / max, c);
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
            // Not implemented: op 164 spell flag bit 6 / 0x41F040 (0 uses in data).
            // Not implemented: the n == 0 call 0x41F040(a, b, c) (op unused in the data).
            164 => {
                self.m.g.spell_flag6 = self.ai(t, ins, 0) & 1 != 0;
                self.m.g.misc_calls += 1;
            }
            165 => {
                self.e.vm.rotation[2] = self.af(t, ins, 0);
                self.e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            // Leftward / rightward cone from one Rng_Float (07 §7 op 169).
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
            // Laser +0x599 byte / +0x560 max length / +0x558, +0x55C tail and head.
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
            // GameFlags 0x164D0B4: clear 7, 8, 13, set 7; bit 13 by stage / practice card (07 §7 op 176).
            176 => {
                use crate::gamemgr::{
                    F_13, F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE, flags,
                };
                let mut f = flags() & !(F_LAST_SPELL | F_LAST_CAPTURED | F_13) | F_LAST_SPELL;
                let card = self.m.g.card;
                let bit13 = if f & F_SPELL_PRACTICE == 0 {
                    matches!(self.m.stage_index, 6 | 7)
                } else {
                    (0x8F..=0x92).contains(&card) || (0xAB..=0xBE).contains(&card)
                };
                // Bit 13's whole-playfield Player_DamageAgainst runs in EnemyMgr_Calc (`last_spell_box`).
                if bit13 {
                    f |= F_13;
                }
                crate::gamemgr::set_flags(f);
                self.e.flags |= A_LAST_SPELL;
            }
            177 => self.e.hud_life = self.ai(t, ins, 0),
            179 | 180 => self.m.g.hud_requests.push(op as u16),
            // Spell_SetBonusFrozen 0x41F0E0: spell flag bit 11 := n & 1 (07 §11.1).
            184 => self.m.g.bonus_frozen = self.ai(t, ins, 0) & 1 != 0,
            // Only below 12: at 12 nothing at all, not even the sfx 0x2D (07 §7).
            181 => {
                let mut p = self.m.player.borrow_mut();
                if p.stats.clock < 12 {
                    crate::sound::play(crate::sound::OK_CLOCK);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bullet::BulletManager;
    use std::cell::RefCell;
    use std::rc::Rc;

    // Sub instruction (07 §2.1): time, op, size, rank mask 0xFF, then i32 args.
    fn ins(time: i32, op: i16, args: &[i32]) -> Vec<u8> {
        let mut v = time.to_le_bytes().to_vec();
        v.extend(op.to_le_bytes());
        v.extend(((12 + 4 * args.len()) as i16).to_le_bytes());
        v.extend([0, 0xFF, 0, 0]);
        for a in args {
            v.extend(a.to_le_bytes());
        }
        v
    }

    // Subs placed after a zeroed 0x48-byte header; each ends with an op −1 marker.
    fn manager(subs: &[&[Vec<u8>]]) -> (AnmManager, EnemyManager) {
        let mut data = vec![0u8; 0x48];
        let mut offs = Vec::new();
        for sub in subs {
            offs.push(data.len());
            for i in sub.iter() {
                data.extend(i);
            }
            data.extend(ins(0, -1, &[]));
        }
        let ecl = crate::ecl::Ecl {
            data,
            subs: offs,
            timelines: Vec::new(),
        };
        let mut anm = AnmManager::default();
        let player = crate::player::Player::new_ref();
        let bullets = Rc::new(RefCell::new(BulletManager::new(&mut anm, player)));
        (anm, EnemyManager::new(ecl, bullets, 1))
    }

    fn spawn(m: &mut EnemyManager, idx: usize, sub: i32, death_sub: i32, flags_b: u32) {
        let e = &mut m.enemies[idx];
        e.flags = A_ALIVE;
        e.flags_b = flags_b;
        e.life = 100;
        e.death_sub = death_sub;
        e.main.ctx.enter(&m.ecl, sub);
    }

    // 07 §7 op 95: no self test. Another enemy gets `Ecl_SetSub` to its death sub (threads, thresholds kept); the
    // caller's own main context keeps its local ip in the old sub (time-0 ops after op 95 still run), only the sub
    // id, clock and wait are taken, and its death sub is consumed without running.
    #[test]
    fn kill_all_includes_caller() {
        let k95 = ins(0, 95, &[]);
        let (mut anm, mut m) = manager(&[
            &[k95.clone(), ins(0, 81, &[1]), ins(1, 81, &[2])],
            &[],
            &[ins(5, 0, &[])],
        ]);
        spawn(&mut m, 0, 2, 1, B_CANNOT_DIE);
        spawn(&mut m, 1, 0, 1, 0);
        spawn(&mut m, 2, 2, 1, 0);
        m.enemies[2].threads[0] = Some(Box::default());
        m.enemies[2].thresholds = [50, -1, -1, -1];
        assert_eq!(ecl_run(&mut m, &mut anm, 1), 0);
        let e = &m.enemies[2];
        assert_eq!(
            (e.life, e.death_sub, e.main.ctx.ip, e.main.ctx.clock.cur),
            (0, -1, m.ecl.subs[1], 0)
        );
        assert!(e.threads[0].is_some());
        assert_eq!(e.thresholds[0], 50);
        let e = &m.enemies[1];
        let after_95 = m.ecl.subs[0] + k95.len() + ins(0, 81, &[1]).len();
        assert_eq!(
            (e.life, e.death_sub, e.main.ctx.ip, e.main.ctx.sub),
            (0, -1, after_95, 1)
        );
        assert_eq!(e.main.ctx.clock.cur, 1);
        assert_eq!(e.flags & (A_SHOT_HIT | A_CONTACT), A_SHOT_HIT);
        // Flags B bit 6 spares an enemy.
        assert_eq!((m.enemies[0].life, m.enemies[0].death_sub), (100, 1));
    }

    fn with_mask(mut v: Vec<u8>, mask: u16) -> Vec<u8> {
        v[10..12].copy_from_slice(&mask.to_le_bytes());
        v
    }

    // 07 §6: a source read of 10042 is the final pos e+0x2D88; the fadd destination read and write use e+0x2D34.
    #[test]
    fn var_10042_source_vs_destination() {
        let f = |x: f32| x.to_bits() as i32;
        let (mut anm, mut m) = manager(&[&[
            with_mask(ins(0, 7, &[f(10016.0), f(10042.0)]), 3),
            with_mask(ins(0, 6, &[10000, 10042]), 3),
            with_mask(ins(0, 15, &[f(10042.0), f(1.0)]), 1),
        ]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].pos = [-5.5, 2.0, 0.0];
        m.enemies[0].pos_offset = [100.0, 20.0, 0.0];
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(e.main.ctx.flt(V_FLT0), 94.5);
        assert_eq!(e.main.ctx.int(V_INT0), 94);
        assert_eq!(e.pos[0], -4.5);
    }

    // 07 §6 var 10099: card active ? spell bit 2 : spell bit 9 (captured).
    #[test]
    fn var_10099_spell_state() {
        let (mut anm, mut m) = manager(&[&[]]);
        spawn(&mut m, 0, 0, -1, 0);
        let mut read = |m: &mut EnemyManager| {
            let e = m.enemies[0].clone();
            var_i(m, &mut anm, &e, &e.main.ctx, 10099)
        };
        m.g.spell_captured = true;
        assert_eq!(read(&mut m), 1);
        m.g.spell = Some(Spell {
            capture: false,
            ..Default::default()
        });
        assert_eq!(read(&mut m), 0);
        m.g.spell.as_mut().unwrap().capture = true;
        m.g.spell_captured = false;
        assert_eq!(read(&mut m), 1);
    }

    // 07 op 155: the running card's cap +0x2638 := 99 999 990 whatever the byte; +0xFC untouched.
    #[test]
    fn op155_raises_card_cap() {
        let (mut anm, mut m) = manager(&[&[ins(0, 155, &[0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.g.spell = Some(Spell {
            bonus: 33_000_000,
            max: 33_000_000,
            ..Default::default()
        });
        ecl_run(&mut m, &mut anm, 0);
        let s = m.g.spell.as_ref().unwrap();
        assert_eq!((s.bonus, s.max), (33_000_000, FIXED_BONUS_CAP));
    }

    // 07 op 158: word 1 → start, word 2 → end, stored as fractions of the op enemy's max life; op 127 0 → target 1.0.
    #[test]
    fn op158_marker_fractions() {
        let (mut anm, mut m) =
            manager(&[&[ins(0, 158, &[2, 500, 1500, 0xFF8040]), ins(0, 127, &[0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].max_life = 2000;
        ecl_run(&mut m, &mut anm, 0);
        assert_eq!(m.g.hud_markers[2], (0.75, 0.25, 0xFF8040));
        assert_eq!(m.g.hud_bar_target, 1.0);
    }

    // 07 §5.2: native 18 at time 0 slows this frame's clock tick (live game speed).
    #[test]
    fn native18_slows_same_frame_tick() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[18, 4])]]);
        spawn(&mut m, 0, 0, -1, 0);
        anm.speed = 1.0;
        ecl_run(&mut m, &mut anm, 0);
        let c = &m.enemies[0].main.ctx.clock;
        assert_eq!((c.cur, c.frac), (0, 0.25));
    }

    // 07 §5 ops 129/130/133/134 under the spell-practice guard (GameFlags bit 14 + bit 7 or 8).
    #[test]
    fn spell_practice_guard() {
        use crate::gamemgr::{F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE, set_flags};
        let sub = [
            ins(0, 129, &[2]),
            ins(0, 130, &[3]),
            ins(0, 133, &[0, 40, 4]),
            ins(0, 134, &[600, 5]),
        ];
        for (f, guarded) in [
            (F_SPELL_PRACTICE, false),
            (F_LAST_SPELL, false),
            (F_SPELL_PRACTICE | F_LAST_SPELL, true),
            (F_SPELL_PRACTICE | F_LAST_CAPTURED, true),
        ] {
            let (mut anm, mut m) = manager(&[&sub]);
            spawn(&mut m, 0, 0, -1, 0);
            set_flags(f);
            ecl_run(&mut m, &mut anm, 0);
            set_flags(0);
            let e = &m.enemies[0];
            assert_eq!((e.thresholds[0], e.timeout), (40, 600));
            let got = (
                (e.flags >> A_DEATH_SHIFT) & 7,
                e.death_sub,
                e.threshold_subs[0],
                e.timeout_sub,
            );
            let want = if guarded {
                (0, -1, -1, -1)
            } else {
                (2, 3, 4, 5)
            };
            assert_eq!(got, want, "flags {f:#x}");
        }
    }

    // 07 §5.4 op 88 on the caller (main context): the frame pushed holds the stored ip (first instruction this frame)
    // + its size; the sub never runs, the caller goes on with clock 0, sub id 1 and depth + 1.
    #[test]
    fn call_boss_self_main_skips_sub() {
        let (mut anm, mut m) = manager(&[
            &[
                ins(0, 0, &[]),
                ins(0, 88, &[0, 1]),
                ins(0, 0, &[]),
                ins(5, 0, &[]),
            ],
            &[ins(0, 83, &[1])],
        ]);
        spawn(&mut m, 0, 0, -1, 0);
        m.bosses[0] = Some(0);
        let s0 = m.ecl.sub_start(0).unwrap();
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(e.flags_b & 2, 0);
        assert_eq!(
            (e.main.ctx.sub, e.main.ctx.ip, e.main.ctx.clock.cur),
            (1, s0 + 44, 1)
        );
        assert_eq!((e.main.depth, e.work_depth), (1, 1));
        assert_eq!((e.main.stack[0].sub, e.main.stack[0].ip), (0, s0 + 12));
    }

    // 07 §5.4 op 88 on the caller from a thread: the thread's own context, stack and depth; main is untouched.
    #[test]
    fn call_boss_self_from_thread_uses_thread() {
        let (mut anm, mut m) = manager(&[
            &[ins(0, 135, &[0, 2]), ins(100, 0, &[])],
            &[ins(0, 83, &[1])],
            &[
                ins(0, 0, &[]),
                ins(0, 88, &[0, 1]),
                ins(0, 0, &[]),
                ins(5, 0, &[]),
            ],
        ]);
        spawn(&mut m, 0, 0, -1, 0);
        m.bosses[0] = Some(0);
        let s2 = m.ecl.sub_start(2).unwrap();
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(e.flags_b & 2, 0);
        assert_eq!((e.main.ctx.sub, e.main.depth), (0, 0));
        let t = e.threads[0].as_ref().unwrap();
        assert_eq!(
            (t.ctx.sub, t.ctx.ip, t.depth, e.work_depth),
            (1, s2 + 44, 1, 1)
        );
        assert_eq!((t.stack[0].sub, t.stack[0].ip), (2, s2 + 12));
    }

    // 07 §5.4 op 88: the target's stored ip is advanced past its next instruction before the push, at the
    // target's working depth; its main depth stays.
    #[test]
    fn call_boss_skips_target_next_ins() {
        let (mut anm, mut m) = manager(&[
            &[ins(0, 88, &[0, 2])],
            &[ins(5, 81, &[1]), ins(6, 81, &[2])],
            &[ins(100, 81, &[1])],
        ]);
        spawn(&mut m, 0, 0, -1, 0);
        spawn(&mut m, 1, 1, -1, 0);
        m.bosses[0] = Some(1);
        let ip = m.enemies[1].main.ctx.ip;
        m.enemies[1].work_depth = 3;
        ecl_run(&mut m, &mut anm, 0);
        let o = &m.enemies[1];
        assert_eq!((o.main.ctx.sub, o.main.depth, o.work_depth), (2, 0, 4));
        assert_eq!(o.main.stack[3].ip, ip + m.ecl.ins(ip).unwrap().size);
    }

    // 07 §5 op 128: a, b, c, d are raw words whatever the mask (d = f32 bits).
    #[test]
    fn op128_words_are_raw() {
        let mut i = ins(
            0,
            128,
            &[6, 0.5f32.to_bits() as i32, 0, 0, 96.0f32.to_bits() as i32],
        );
        i[10] = 0x1F;
        let (mut anm, mut m) = manager(&[&[i, ins(100, 81, &[1])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0);
        assert_eq!(m.enemies[0].orbit_target, 96.0);
    }

    // Op 95 in a thread: the main context takes the death sub, the threads are kept (07 §7 op 95).
    #[test]
    fn kill_all_from_thread() {
        let (mut anm, mut m) = manager(&[&[ins(0, 135, &[0, 2])], &[], &[ins(0, 95, &[])]]);
        spawn(&mut m, 0, 0, 1, 0);
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!((e.life, e.death_sub, e.main.ctx.ip), (0, -1, m.ecl.subs[1]));
        assert!(e.threads[0].is_some());
    }

    // The caller is spared like any other enemy by flags B bit 6.
    #[test]
    fn kill_all_spares_caller_with_b6() {
        let (mut anm, mut m) = manager(&[&[ins(0, 95, &[])], &[]]);
        spawn(&mut m, 3, 0, 1, B_CANNOT_DIE);
        ecl_run(&mut m, &mut anm, 3);
        let e = &m.enemies[3];
        assert_eq!((e.life, e.death_sub), (100, 1));
        assert_ne!(e.main.ctx.ip, m.ecl.subs[1]);
    }

    // 07 §12 native 31: item type 3 while the player is bombing, else 5.
    #[test]
    fn native31_item_type() {
        for (bombing, kind) in [(false, 5), (true, 3)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[31, 0])]]);
            m.player.borrow_mut().bombing = bombing;
            spawn(&mut m, 0, 0, -1, 0);
            ecl_run(&mut m, &mut anm, 0);
            let b = m.bullets.borrow();
            let kinds: Vec<u8> = b
                .items
                .items
                .iter()
                .filter(|i| i.alive)
                .map(|i| i.kind)
                .collect();
            assert_eq!(kinds, [kind], "bombing {bombing}");
        }
    }

    // 07 §12 native 4: zone change between pos and pos − vel reverses the bullet and scales it across the ring
    // (inward on an outer crossing, outward on an inner one); cooldown 2 then skips the next two calls.
    #[test]
    fn native4_warp() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[4, 0])]]);
        let cases = [
            ([330.0, 208.0, 0.0], [3.0, 0.0, 0.0]),
            ([250.0, 208.0, 0.0], [-20.0, 0.0, 0.0]),
            ([400.0, 108.0, 0.0], [2.0, -1.0, 0.0]),
        ];
        {
            let mut b = m.bullets.borrow_mut();
            for (k, (pos, vel)) in cases.into_iter().enumerate() {
                let x = &mut b.bullets[k];
                (x.state, x.pos, x.vel, x.angle) = (1, pos, vel, 0.0);
            }
        }
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0);
        let b = m.bullets.borrow();
        let [b0, b1, b2] = [&b.bullets[0], &b.bullets[1], &b.bullets[2]];
        assert!((b0.pos[0] - 261.0).abs() < 1e-3 && b0.pos[1] == 208.0);
        assert_eq!((b0.vel, b0.warp_cooldown), ([-3.0, -0.0, -0.0], 2));
        assert!((b0.angle - PI).abs() < 1e-6);
        assert!((b1.pos[0] - 308.0).abs() < 1e-3);
        assert_eq!((b1.vel[0], b1.warp_cooldown), (20.0, 2));
        assert_eq!(
            (b2.pos, b2.vel, b2.warp_cooldown),
            (cases[2].0, cases[2].1, 0)
        );
    }

    // 07 §12 native 7: Q's outer x0 is −31.1, so pos −31.0 / prev −31.5 is a zone change (1 vs 2).
    #[test]
    fn native7_prev_bound_quirk() {
        let (_, m) = manager(&[&[]]);
        let mut bm = m.bullets.borrow_mut();
        let x = &mut bm.bullets[0];
        (x.state, x.pos, x.vel) = (1, [-31.0, 224.0, 0.0], [0.5, 0.0, 0.0]);
        bm.warp(&Warp::native7());
        assert_eq!(bm.bullets[0].warp_cooldown, 2);
        assert_eq!(bm.bullets[0].vel[0], -0.5);
        bm.warp(&Warp::native7());
        assert_eq!(bm.bullets[0].warp_cooldown, 1);
    }

    // 07 §12 native 9: box from o = final pos − (vars 10016, 10017), 590 along main VM rotation z, big box
    // height 160 hits on every call (no warning phase).
    #[test]
    fn native9_laser_box() {
        let cases = [
            (0.0, [150.0, 100.0], false),
            (0.0, [200.0, 179.0], true),
            (0.0, [200.0, 181.0], false),
            (0.0, [779.0, 100.0], true),
            (PI / 2.0, [190.0, 300.0], true),
            (PI / 2.0, [300.0, 110.0], false),
        ];
        for (rot, pp, hit) in cases {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[9, 0])]]);
            spawn(&mut m, 0, 0, -1, 0);
            let e = &mut m.enemies[0];
            e.pos = [200.0, 100.0, 0.0];
            e.vm.rotation[2] = rot;
            e.main.ctx.set_flt(V_FLT0, 10.0);
            m.player.borrow_mut().pos = [pp[0], pp[1], 0.0];
            ecl_run(&mut m, &mut anm, 0);
            assert_eq!(m.player.borrow().state == 2, hit, "rot {rot} at {pp:?}");
        }
    }

    // 07 §12 native 11: small box (192 + 48 per side) grazes only when timer e+0x2E14 changed to a multiple
    // of 12; the big box (240) still misses there.
    #[test]
    fn native11_graze_every_12() {
        for (cur, prev, grazed) in [(12, 11, 1), (12, 12, 0), (13, 12, 0)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[11, 0])]]);
            spawn(&mut m, 0, 0, -1, 0);
            let e = &mut m.enemies[0];
            e.pos = [100.0, 100.0, 0.0];
            e.boss_timer.cur = cur;
            e.boss_timer.prev = prev;
            m.player.borrow_mut().pos = [300.0, 100.0 + 130.0, 0.0];
            ecl_run(&mut m, &mut anm, 0);
            let p = m.player.borrow();
            assert_eq!((p.grazes, p.state), (grazed, 0), "timer {cur}/{prev}");
        }
    }

    // 07 §12 native 8: parent 0 → 1 → 2 → 3; caller 3 and enemy 1 share var 10038, enemy 1 leads. Counters in
    // int vars 10005–10007; the caller turns 2 % toward leader + π + leader orbit velocity.
    #[test]
    fn native8_familiar_ring() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[8, 0])], &[]]);
        for (k, tag) in [(0, 0), (1, 5), (2, 6), (3, 5)] {
            spawn(&mut m, k, if k == 3 { 0 } else { 1 }, -1, 0);
            m.enemies[k].main.ctx.set_int(V_INT1 + 2, tag);
            m.enemies[k].main.ctx.set_int(V_INT1 + 1, -1);
            if k > 0 {
                m.enemies[k].root = Some(0);
                m.enemies[k - 1].chain_next = Some(k);
            }
        }
        m.enemies[1].orbit_av = 0.1;
        ecl_run(&mut m, &mut anm, 3);
        let v = |m: &EnemyManager, k: usize, id: usize| m.enemies[k].main.ctx.int(id);
        assert_eq!(
            (
                v(&m, 1, V_INT1 + 1),
                v(&m, 2, V_INT1 + 1),
                v(&m, 3, V_INT1 + 1)
            ),
            (0, -1, 1)
        );
        assert_eq!(
            (
                v(&m, 3, V_INT0 + 5),
                v(&m, 3, V_INT0 + 6),
                v(&m, 3, V_INT0 + 7)
            ),
            (0, 2, 1)
        );
        let t = anmvm::wrap_add(anmvm::wrap_add(0.0, 6.2831855 / 2.0), 0.1);
        assert!((m.enemies[3].orbit_angle - t * 0.02).abs() < 1e-6);
        // A changed count sets the flag 10005 when the old count was not 0.
        m.enemies[3].main.ctx.set_int(V_INT0 + 6, 3);
        m.enemies[3].main.ctx.enter(&m.ecl, 0);
        ecl_run(&mut m, &mut anm, 3);
        assert_eq!(
            (
                v(&m, 3, V_INT0 + 5),
                v(&m, 3, V_INT0 + 6),
                v(&m, 3, V_INT0 + 7)
            ),
            (1, 2, 2)
        );
    }

    // 07 §12 native 8 does nothing without a parent.
    #[test]
    fn native8_needs_parent() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[8, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0 + 7), 0);
    }

    // 07 §12 native 13: stage tint request 0xFFC03030.
    #[test]
    fn native13_tint() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[13, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0);
        assert_eq!(anm.stage_tint, Some(0xFFC03030));
    }

    // 07 §12 native 16: chained children near a flag-0x100000 bullet with var 10038 == 0 get 60 and var 10019.
    #[test]
    fn native16_marks_children() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[16, 0])], &[]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].main.ctx.set_int(V_INT0 + 7, 5);
        m.enemies[0].chain_next = Some(1);
        // 4: 3D distance² = 40² + 50² = 4100 ≥ 4096; 5: exactly 64 px (strict test).
        for (k, x, z, tag) in [
            (1, 100.0, 0.0, 0),
            (2, 100.0, 0.0, 7),
            (3, 250.0, 0.0, 0),
            (4, 100.0, 50.0, 0),
            (5, 76.0, 0.0, 0),
        ] {
            spawn(&mut m, k, 1, -1, 0);
            m.enemies[k].pos = [x, 100.0, z];
            m.enemies[k].main.ctx.set_int(V_INT1 + 2, tag);
            m.enemies[k].chain_next = (k < 5).then_some(k + 1);
        }
        {
            let mut b = m.bullets.borrow_mut();
            (b.bullets[0].state, b.bullets[0].pos, b.bullets[0].flags) =
                (1, [140.0, 100.0, 0.0], 0x100000);
            (b.bullets[1].state, b.bullets[1].pos) = (1, [250.0, 100.0, 0.0]);
        }
        ecl_run(&mut m, &mut anm, 0);
        let got: Vec<_> = (1..6)
            .map(|k| {
                let c = &m.enemies[k].main.ctx;
                (c.int(V_INT1 + 2), c.int(V_INT0 + 7))
            })
            .collect();
        assert_eq!(got, [(60, 5), (7, 0), (0, 0), (0, 0), (0, 0)]);
    }

    // 07 §12 row 24: var 10000 := spell cards captured this game (stats +0x1C).
    #[test]
    fn native24_captured_cards() {
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[24, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.player.borrow_mut().stats.spells = 7;
        ecl_run(&mut m, &mut anm, 0);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0), 7);
    }

    // 07 §6: an int read of 10033 draws one Next32 and truncates Rng_Float.
    #[test]
    fn int_read_10033_draws() {
        let (mut anm, mut m) = manager(&[&[]]);
        spawn(&mut m, 0, 0, -1, 0);
        let mut r = anm.rng.clone();
        r.next_u32();
        let e = m.enemies[0].clone();
        assert_eq!(var_i(&mut m, &mut anm, &e, &e.main.ctx, 10033), 0);
        assert_eq!(anm.rng.next_u16(), r.next_u16());
    }

    // 07 §4.2 / §5.5: an unregistered index enters sub 0 (table zero-filled); a negative entry still skips the
    // current instruction, pushes and increments the depth, but leaves the main context in its sub.
    #[test]
    fn interrupt_table_default_and_negative() {
        let (mut anm, mut m) = manager(&[&[ins(0, 0, &[])], &[ins(5, 0, &[]), ins(5, 0, &[])]]);
        spawn(&mut m, 0, 1, -1, 0);
        m.enemies[0].pending_interrupt = 3;
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(
            (e.main.ctx.sub, e.main.depth, e.pending_interrupt),
            (0, 1, -1)
        );
        let s1 = m.ecl.sub_start(1).unwrap();
        assert_eq!(e.main.stack[0].ip, s1 + 12);

        let (mut anm, mut m) = manager(&[&[ins(5, 0, &[]), ins(5, 0, &[])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].interrupts[2] = -1;
        m.enemies[0].pending_interrupt = 2;
        let s0 = m.ecl.sub_start(0).unwrap();
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(
            (e.main.ctx.sub, e.main.ctx.ip, e.main.depth),
            (0, s0 + 12, 1)
        );
        assert_eq!((e.main.ctx.clock.cur, e.pending_interrupt), (1, -1));
    }

    // 07 §5.5 thread contexts: taken in a thread, the thread's ip skips, main is pushed on the thread's stack at
    // the thread's depth, main enters the sub, the thread's depth += 1 and the main depth stays.
    #[test]
    fn interrupt_in_thread_context() {
        let (mut anm, mut m) = manager(&[
            &[ins(0, 135, &[0, 1]), ins(100, 0, &[])],
            &[ins(0, 125, &[1]), ins(0, 83, &[1]), ins(5, 0, &[])],
            &[ins(50, 0, &[])],
        ]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].interrupts[1] = 2;
        let (s0, s1) = (m.ecl.sub_start(0).unwrap(), m.ecl.sub_start(1).unwrap());
        ecl_run(&mut m, &mut anm, 0);
        let e = &m.enemies[0];
        assert_eq!(
            (e.main.ctx.sub, e.main.depth, e.pending_interrupt),
            (2, 0, -1)
        );
        let t = e.threads[0].as_ref().unwrap();
        // Op 83 (the instruction after 125) still runs in the thread this frame.
        assert_eq!((t.ctx.sub, t.ctx.ip, t.depth), (1, s1 + 16 + 16, 1));
        assert_eq!((t.stack[0].sub, t.stack[0].ip), (0, s0 + 20));
        assert_eq!(e.flags_b & 2, 2);
    }

    // 07 §7 op 181: below 12 sfx 0x2D and clock + 1; at 12 nothing, no sfx.
    #[test]
    fn op181_clock() {
        for (clock, after, sfx) in [(11, 12, true), (12, 12, false)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 181, &[])]]);
            m.player.borrow_mut().stats.clock = clock;
            spawn(&mut m, 0, 0, -1, 0);
            crate::sound::take();
            ecl_run(&mut m, &mut anm, 0);
            assert_eq!(m.player.borrow().stats.clock, after);
            let played = crate::sound::take()
                .iter()
                .any(|&(id, _)| id == crate::sound::OK_CLOCK);
            assert_eq!(played, sfx, "clock {clock}");
            assert_eq!(m.g.hud_requests.contains(&181), sfx);
        }
    }

    // `ins` with a var mask (07 §2.1 +10).
    fn ins_v(time: i32, op: i16, mask: u16, args: &[i32]) -> Vec<u8> {
        let mut v = ins(time, op, args);
        v[10..12].copy_from_slice(&mask.to_le_bytes());
        v
    }

    // 07 §6 in-place ops: the destination is read and written through the reference; a non-int destination
    // (10042–10044) and an int variable under a float op use the instruction's own argument slot.
    #[test]
    fn in_place_ops_use_reference() {
        let iadd = ins_v(0, 10, 1, &[10042, 5]);
        let inc = ins_v(0, 30, 1, &[10043]);
        let fadd = ins_v(
            0,
            15,
            1,
            &[10000.0f32.to_bits() as i32, 2.0f32.to_bits() as i32],
        );
        let iadd_ctx = ins_v(0, 10, 1, &[10000, 3]);
        let (mut anm, mut m) = manager(&[&[iadd, inc, fadd, iadd_ctx]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].main.ctx.set_int(V_INT0, 4);
        (m.enemies[0].pos, m.enemies[0].pos_offset) = ([7.0, 8.0, 0.0], [93.0, 92.0, 0.0]);
        ecl_run(&mut m, &mut anm, 0);
        let i = |o: usize| m.ecl.ins(m.ecl.subs[0] + o).unwrap();
        assert_eq!(m.ecl.arg(&i(0), 0) as i32, 10047);
        assert_eq!(m.ecl.arg(&i(20), 0) as i32, 10044);
        assert_eq!(f32::from_bits(m.ecl.arg(&i(36), 0)), 10002.0);
        assert_eq!(m.enemies[0].pos, [7.0, 8.0, 0.0]);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0), 7);
    }

    // 07 §7 ops 86 / 87: one getter call (one RNG draw for 10082 / 10032), on self or another boss.
    #[test]
    fn boss_var_single_getter() {
        let op87 = ins_v(0, 87, 1, &[10016.0f32.to_bits() as i32, 10082, 0]);
        let op86 = ins_v(0, 86, 1, &[10000, 10032, 1]);
        let (mut anm, mut m) = manager(&[&[op87, op86]]);
        spawn(&mut m, 0, 0, -1, 0);
        spawn(&mut m, 1, 0, -1, 0);
        m.bosses = [Some(0), Some(1), None, None];
        let mut rng = anm.rng.clone();
        let (f, i) = (
            rng.next_f32() * 2.0 * PI - PI,
            (rng.next_u32() & 0x7FFF_FFFF) as i32,
        );
        ecl_run(&mut m, &mut anm, 0);
        let ctx = &m.enemies[0].main.ctx;
        assert_eq!((ctx.flt(V_FLT0), ctx.vars[V_INT0] as i32), (f, i));
        assert_eq!(anm.rng.state, rng.state);
    }

    fn child_op(op: i16, sub: i32) -> Vec<u8> {
        ins(0, op, &[sub, 0, 0, 10, 0, 100])
    }

    // 07 §10.5: ops 90–92 leave the mode bits e+0x3330 at 0, so a youkai-only time-0 instruction runs in human
    // mode; op 92 writes the follow offset, final pos and bit 9 only after the spawn-time run.
    #[test]
    fn op92_follow_and_mode_bits_after_spawn_run() {
        let mut youkai_only = ins_v(0, 6, 1, &[10000, 7]);
        youkai_only[9] = 0x4F;
        let (x, y) = (5.0f32.to_bits() as i32, 6.0f32.to_bits() as i32);
        let op92 = ins(0, 92, &[1, x, y, 10, 0, 100]);
        let (mut anm, mut m) = manager(&[&[op92], &[youkai_only], &[]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].pos = [100.0, 50.0, 0.0];
        ecl_run(&mut m, &mut anm, 0);
        let c = &m.enemies[1];
        assert_eq!(c.main.ctx.vars[V_INT0], 7);
        assert_eq!(c.mode_bits, 0);
        assert_eq!(c.pos_offset, [100.0, 50.0, 0.0]);
        assert_eq!(c.final_pos, [105.0, 56.0, 0.0]);
        assert_eq!(c.flags & (A_FOLLOW | A_CONTACT), A_FOLLOW);
    }

    // 07 §10.5: bit 8, bit 11, layer, contact off and aura override the child's time-0 ops; the link is written
    // after its spawn-time run, so var 10096 reads 0 there although the spawner is a root with a child.
    #[test]
    fn child_flags_after_spawn_run() {
        let child = [
            ins(0, 81, &[2]),
            ins(0, 159, &[5]),
            ins_v(0, 6, 3, &[10000, 10096]),
        ];
        let (mut anm, mut m) = manager(&[&[child_op(90, 1)], &child, &[]]);
        spawn(&mut m, 0, 2, -1, 0);
        spawn(&mut m, 1, 2, -1, 0);
        (m.enemies[0].chain_next, m.enemies[1].chain_prev) = (Some(1), Some(0));
        m.enemies[1].root = Some(0);
        m.enemies[0].main.ctx.enter(&m.ecl, 0);
        m.enemies[0].main.ctx.vars[V_INT0] = 7;
        ecl_run(&mut m, &mut anm, 0);
        let c = &m.enemies[2];
        assert!(c.alive());
        assert_eq!(c.main.ctx.vars[V_INT0], 0);
        assert_eq!(
            c.flags & (A_CONTACT | A_MODE_REACT | A_YOUKAI),
            A_MODE_REACT
        );
        assert_eq!(c.layer, 2);
        assert_eq!(
            (c.root, c.chain_prev, c.chain_next),
            (Some(0), Some(1), None)
        );
        assert_eq!(m.enemies[1].chain_next, Some(2));
        assert_eq!(m.enemies[0].children, 1);
        assert_ne!(c.fx_aura, crate::effect::NONE);
        crate::effect::get(c.fx_aura, |x| {
            assert_eq!(x.vm.pending_interrupt, 1);
            assert_eq!(x.vm.flags & anmvm::F_COLOR2, 0);
        });
    }

    // 07 §10.5: a chained child links the new enemy right after itself and overwrites its own +8.
    #[test]
    fn chained_child_links_after_itself() {
        let (mut anm, mut m) = manager(&[&[child_op(90, 1)], &[]]);
        for k in 0..3 {
            spawn(&mut m, k, 1, -1, 0);
        }
        (m.enemies[0].chain_next, m.enemies[1].chain_next) = (Some(1), Some(2));
        (m.enemies[1].chain_prev, m.enemies[2].chain_prev) = (Some(0), Some(1));
        (m.enemies[1].root, m.enemies[2].root) = (Some(0), Some(0));
        m.enemies[1].main.ctx.enter(&m.ecl, 0);
        ecl_run(&mut m, &mut anm, 1);
        assert_eq!(m.enemies[1].chain_next, Some(3));
        let c = &m.enemies[3];
        assert_eq!(
            (c.root, c.chain_prev, c.chain_next),
            (Some(1), Some(1), None)
        );
        // The cut-off enemy keeps its +4.
        assert_eq!(m.enemies[2].chain_prev, Some(1));
        assert_eq!((m.enemies[1].children, m.enemies[0].children), (1, 0));
    }

    // 07 §10.5: the aura VM +0x14 (z angular velocity) is negated for an odd slot.
    #[test]
    fn child_aura_odd_slot_negated() {
        let (mut anm, mut m) = manager(&[&[child_op(90, 1)], &[]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0);
        let c = &m.enemies[1];
        assert_eq!(c.slot, 1);
        crate::effect::get(c.fx_aura, |x| {
            assert!(x.vm.angular_vel[2].is_sign_negative())
        });
    }

    // 07 §7 op 174: interrupt (player mode ≠ 0) + 1, odd slot negates the spin, colour2 untouched; ops 80/81 bit 1
    // switch the aura's colour2 flag.
    #[test]
    fn aura_op174_and_contact_flag() {
        let (mut anm, mut m) = manager(&[
            &[ins(0, 174, &[0]), ins(1, 81, &[2]), ins(2, 80, &[2])],
            &[],
        ]);
        spawn(&mut m, 1, 0, -1, 0);
        m.enemies[1].slot = 1;
        m.g.player_mode = 1;
        ecl_run(&mut m, &mut anm, 1);
        let fx = m.enemies[1].fx_aura;
        assert_ne!(fx, crate::effect::NONE);
        let spin = |m: &EnemyManager| {
            let mut r = (0, 0.0, 0);
            crate::effect::get(m.enemies[1].fx_aura, |x| {
                r = (
                    x.vm.pending_interrupt,
                    x.vm.angular_vel[2],
                    x.vm.flags & anmvm::F_COLOR2,
                )
            });
            r
        };
        let (int, w, c2) = spin(&m);
        assert_eq!((int, c2), (2, 0));
        assert!(w.is_sign_negative());
        ecl_run(&mut m, &mut anm, 1);
        assert_eq!(spin(&m).2, anmvm::F_COLOR2);
        ecl_run(&mut m, &mut anm, 1);
        assert_eq!(spin(&m).2, 0);
    }

    // 07 §4.1 step 3: only pos is written; prev pos stays (0, 0, 0), final pos is recomputed by the run.
    #[test]
    fn spawn_keeps_template_prev_pos() {
        let (mut anm, mut m) = manager(&[&[]]);
        let i = m.spawn(&mut anm, 0, [100.0, 50.0, 0.0], 10, 0, 100, false);
        let e = &m.enemies[i.unwrap()];
        assert_eq!(e.prev_pos, [0.0; 3]);
        assert_eq!(e.final_pos, [100.0, 50.0, 0.0]);
    }

    // 07 §9.1 step 1: the filters run on the raw instruction before any argument read, so a skipped bullet op
    // with a `$rand` angle draws nothing; the mode filter tests the enemy's flag A bit 11, not the player's mode.
    #[test]
    fn bullet_filters_before_decode() {
        let f = |x: f32| x.to_bits() as i32;
        let op = |flags: u32| {
            ins_v(
                0,
                96,
                1 << 6,
                &[0, 3, 1, f(2.0), f(1.0), f(10033.0), 0, flags as i32],
            )
        };
        for (flags, youkai, r2) in [
            (0x8000, false, 0.0),
            (0x10000, true, 0.0),
            (0, false, 1024.0),
        ] {
            let (mut anm, mut m) = manager(&[&[op(flags)]]);
            spawn(&mut m, 0, 0, -1, 0);
            m.g.player_mode = 1 - youkai as i32;
            m.enemies[0].set_flag(A_YOUKAI, youkai);
            m.enemies[0].nofire_r2 = r2;
            m.enemies[0].pos = [10.0, 20.0, 0.0];
            m.enemies[0].final_pos = [10.0, 20.0, 0.0];
            m.g.player = [30.0, 20.0, 0.0];
            let before = anm.rng.counter;
            ecl_run(&mut m, &mut anm, 0);
            assert_eq!(anm.rng.counter, before, "flags {flags:#x}");
        }
    }
}
