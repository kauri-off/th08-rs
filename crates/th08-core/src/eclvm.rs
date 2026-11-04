//! ECL interpreter `Ecl_Run` 0x4184B0: contexts, variables, opcodes, natives (07 §5–§9, §11, §12).

mod ops_bullet;
mod ops_enemy;
mod ops_flow;
mod ops_laser;
mod ops_move;
mod ops_native;
mod ops_spell;
mod ops_var;
mod vars;

use crate::anmmgr::AnmManager;
use crate::anmvm;
use crate::enemy::{A_NO_PUSH, Enemy, EnemyManager, Thread, V_ARGI};
use crate::game::Sys;
use th08_formats::ecl::{Arg, Dst, Ins, Op};
use vars::{FltVar, IntVar, read_f, read_i};
pub(crate) use vars::{var_f, var_i};

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
pub fn ecl_run(m: &mut EnemyManager, anm: &mut AnmManager, idx: usize, sys: &mut Sys) -> i32 {
    m.with_taken(idx, |m, e| run_enemy(m, anm, e, idx, sys))
}

fn run_enemy(
    m: &mut EnemyManager,
    anm: &mut AnmManager,
    e: &mut Enemy,
    idx: usize,
    sys: &mut Sys,
) -> i32 {
    let mut main = std::mem::take(&mut e.main);
    let r = Exec {
        m,
        anm,
        e,
        idx,
        sys: sys.re(),
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
            sys: sys.re(),
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
    e.update_move_mode(anm.speed, anm.slowdown_restore);
    let mut main = std::mem::take(&mut e.main);
    Exec {
        m,
        anm,
        e,
        idx,
        sys: sys.re(),
        loaded_ip: 0,
    }
    .autofire_and_dir_anim(&mut main);
    e.main = main;
    0
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

// Script-indexed table entry: a negative index is out of range, as the original's unsigned compare / pointer add.
fn at<T>(s: &[T], k: i32) -> Option<&T> {
    usize::try_from(k).ok().and_then(|k| s.get(k))
}

fn at_mut<T>(s: &mut [T], k: i32) -> Option<&mut T> {
    usize::try_from(k).ok().and_then(|k| s.get_mut(k))
}

// Low byte / low word of a script int, stored into a u8 / u16 field (two's complement truncation).
fn lo8(v: i32) -> u8 {
    v.cast_unsigned() as u8
}

fn lo16(v: i32) -> u16 {
    v.cast_unsigned() as u16
}

pub(crate) struct Exec<'a> {
    m: &'a mut EnemyManager,
    anm: &'a mut AnmManager,
    e: &'a mut Enemy,
    idx: usize,
    sys: Sys<'a>,
    // ctx.ip as stored while the context runs: the ip Ecl_Run last loaded into its local copy (07 §5.4 op 88).
    loaded_ip: usize,
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
                    t.ctx.wait.dec(self.anm.speed, self.anm.slowdown_restore);
                    t.ctx.clock.dec(self.anm.speed, self.anm.slowdown_restore);
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
        let sub = at(&self.e.interrupts, k).copied().unwrap_or(0);
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
    // Argument as int / float: a variable id when its var-mask bit is set (07 §2.1, §6).
    fn ai(&mut self, t: &Thread, a: Arg) -> i32 {
        self.vi(t, a.raw.cast_signed(), a.var)
    }

    fn af(&mut self, t: &Thread, a: Arg) -> f32 {
        self.vf(t, f32::from_bits(a.raw), a.var)
    }

    // Same with an explicit mask bit (bullet/laser blocks, 07 §9.1, §9.4).
    fn vi(&mut self, t: &Thread, id: i32, var: bool) -> i32 {
        if var {
            var_i(
                self.m,
                crate::pl!(self.sys),
                self.sys.bullets.as_deref(),
                self.sys.rng,
                self.e,
                &t.ctx,
                id,
                self.sys.rank.rank,
            )
        } else {
            id
        }
    }

    fn vf(&mut self, t: &Thread, v: f32, var: bool) -> f32 {
        if var {
            var_f(self.m, self.sys.rng, self.e, &t.ctx, v, self.sys.rank.rank)
        } else {
            v
        }
    }

    fn write_i(&mut self, t: &mut Thread, r: IntVar, v: i32) {
        match r {
            IntVar::Ctx(k) => t.ctx.set_int(k, v),
            IntVar::EInt(k) => self.e.ivars[k] = v,
            IntVar::Difficulty => self.m.g.difficulty = v,
            IntVar::Rank => self.sys.rank.rank = v,
            IntVar::F2e1c => self.e.f2e1c = v,
            IntVar::Life => self.e.life = v,
            IntVar::GArg(k) => self.m.g.args[k] = v.cast_unsigned(),
            IntVar::Item => self.e.item = v,
            IntVar::Score => self.e.score = v,
        }
    }

    fn write_f(&mut self, t: &mut Thread, r: FltVar, v: f32) {
        let e = &mut *self.e;
        match r {
            FltVar::Ctx(k) => t.ctx.set_flt(k, v),
            FltVar::EFlt(k) => e.fvars[k] = v,
            FltVar::Pos(k) => e.pos[k] = v,
            FltVar::Player(k) => self.m.g.player[k] = v,
            FltVar::GArg(k) => self.m.g.args[k] = v.to_bits(),
            FltVar::Angle => e.angle = v,
            FltVar::AngVel => e.angvel = v,
            FltVar::Speed => e.speed = v,
            FltVar::Accel => e.accel = v,
            FltVar::Radius => e.radius = v,
            FltVar::Start(k) => e.interp_start[k] = v,
            FltVar::OrbitAngle => e.orbit_angle = v,
            FltVar::OrbitAv => e.orbit_av = v,
            FltVar::Delta(k) => e.interp_delta[k] = v,
        }
    }

    // Destination read of an in-place int op (07 §6 `Ecl_IntVarRef` 0x41FE10): through the reference, never the
    // getter; anything but a writable int variable reads the instruction's own argument slot (e.g. 10042–10044).
    fn ai_dst(&self, t: &Thread, d: Dst) -> i32 {
        let raw = d.arg.raw.cast_signed();
        match IntVar::from_id(raw).filter(|_| d.arg.var) {
            Some(r) => read_i(self.m, self.e, &t.ctx, &r, self.sys.rank.rank),
            None => raw,
        }
    }

    // Destination read of an in-place float op (07 §6 `Ecl_FloatVarRef` 0x420950): 10042–10044 are the pos, not the
    // final pos; anything but a writable float variable reads the argument slot.
    fn af_dst(&self, t: &Thread, d: Dst) -> f32 {
        let lit = f32::from_bits(d.arg.raw);
        match FltVar::from_id(lit as i32).filter(|_| d.arg.var) {
            Some(r) => read_f(self.m, self.e, &t.ctx, &r),
            None => lit,
        }
    }

    // Destination write; anything but a writable variable of the right type writes into the file (07 §6).
    fn set_i(&mut self, t: &mut Thread, ins: &Ins, d: Dst, v: i32) {
        match IntVar::from_id(d.arg.raw.cast_signed()).filter(|_| d.arg.var) {
            Some(r) => self.write_i(t, r, v),
            None => {
                std::sync::Arc::make_mut(&mut self.m.ecl).set_arg(ins, d.k(), v.cast_unsigned());
            }
        }
    }

    fn set_f(&mut self, t: &mut Thread, ins: &Ins, d: Dst, v: f32) {
        match FltVar::from_id(f32::from_bits(d.arg.raw) as i32).filter(|_| d.arg.var) {
            Some(r) => self.write_f(t, r, v),
            None => std::sync::Arc::make_mut(&mut self.m.ecl).set_arg(ins, d.k(), v.to_bits()),
        }
    }

    // Interpolation targets (07 §5.6): float variables, else int variables (truncated).
    fn set_var_any(&mut self, t: &mut Thread, id: i32, v: f32) {
        if let Some(r) = FltVar::from_id(id) {
            self.write_f(t, r, v);
        } else if let Some(r) = IntVar::from_id(id) {
            self.write_i(t, r, v as i32);
        }
    }

    // Jump (07 §7 op 4): clock = t; ip = this instruction + byte offset.
    fn jump(&mut self, t: &mut Thread, ins: &Ins, time: Arg, to: Arg) -> Flow {
        let time = self.ai(t, time);
        let off = self.ai(t, to);
        t.ctx.clock.set(time);
        t.ctx.ip = ins.off.rel(off);
        Flow::Jumped
    }

    // Opcode switch (07 §7 table), one fn per op group; ops outside the table are ignored.
    fn exec(&mut self, t: &mut Thread, ins: &Ins) -> Flow {
        match self.m.ecl.op(ins) {
            Op::Flow(op) => return self.op_flow(t, ins, op),
            Op::Var(op) => self.op_var(t, ins, op),
            Op::Move(op) => self.op_move(t, op),
            Op::Enemy(op) => self.op_enemy(t, op),
            Op::Bullet(op) => self.op_bullet(t, ins, op),
            Op::Laser(op) => self.op_laser(t, ins, op),
            Op::Boss(op) => return self.op_boss(t, ins, op),
            Op::Unknown(_) => {}
        }
        Flow::Next
    }
}

// `ease` table of 05 (shared with ANM interpolation), mode from the script.
fn ease(mode: i32, t: f32) -> f32 {
    anmvm::ease(lo8(mode), t)
}

#[cfg(test)]
mod tests {
    use super::ops_spell::FIXED_BONUS_CAP;
    use super::ops_var::hermite;
    use super::*;
    use crate::bullet::Warp;
    use crate::enemy::*;
    use crate::fmath;
    use crate::gamemgr::{F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE};
    use std::f32::consts::PI;

    #[test]
    fn hermite_eval_order() {
        let v = [12.3f32, -45.6, 7.89, 101.1];
        assert_eq!(hermite(0.0, v), v[0]);
        assert_eq!(hermite(1.0, v), v[1]);
        let mut differs = false;
        for k in 1..1000 {
            let t = k as f32 / 1000.0;
            let h0 = ((t - 1.0) * (t - 1.0)) * (2.0 * t + 1.0);
            let h1 = (t * t) * (3.0 - 2.0 * t);
            let h2 = ((1.0 - t) * (1.0 - t)) * t;
            let h3 = ((t - 1.0) * t) * t;
            let want = ((h0 * v[0] + h1 * v[1]) + h2 * v[2]) + h3 * v[3];
            assert_eq!(hermite(t, v).to_bits(), want.to_bits());
            let old = (2.0 * t + 1.0) * (t - 1.0) * (t - 1.0) * v[0]
                + (3.0 - 2.0 * t) * t * t * v[1]
                + t * (1.0 - t) * (1.0 - t) * v[2]
                + (t - 1.0) * t * t * v[3];
            differs |= old.to_bits() != want.to_bits();
        }
        assert!(differs);
    }

    // Sub instruction (07 §2.1): time, op, size, rank mask 0xFF, then i32 args.
    fn ins(time: i32, op: i16, args: &[i32]) -> Vec<u8> {
        let mut v = time.to_le_bytes().to_vec();
        v.extend(op.to_le_bytes());
        v.extend(i16::try_from(12 + 4 * args.len()).unwrap().to_le_bytes());
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
            for i in *sub {
                data.extend(i);
            }
            data.extend(ins(0, -1, &[]));
        }
        let ecl = th08_formats::ecl::Ecl {
            data,
            subs: offs,
            timelines: Vec::new(),
        };
        let anm = AnmManager::default();
        let mut player = crate::player::Player::default();
        (anm, EnemyManager::new(ecl, &mut player, 1))
    }

    // Test systems with the player of `manager`'s enemy manager.
    fn ts() -> crate::game::TestSys {
        crate::game::TestSys::with_player(1)
    }

    // A fresh player and bullet manager for each `manager` built inside a loop (each had its own before 9a / 9b).
    fn fresh_player(sys: &mut crate::game::Sys) {
        let ts = crate::game::TestSys::with_player(1);
        *crate::pl!(sys) = *ts.player.unwrap();
        *crate::bm!(sys) = *ts.bullets.unwrap();
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
        assert_eq!(ecl_run(&mut m, &mut anm, 1, sys), 0);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let f = |x: f32| x.to_bits().cast_signed();
        let (mut anm, mut m) = manager(&[&[
            with_mask(ins(0, 7, &[f(10016.0), f(10042.0)]), 3),
            with_mask(ins(0, 6, &[10000, 10042]), 3),
            with_mask(ins(0, 15, &[f(10042.0), f(1.0)]), 1),
        ]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].pos = [-5.5, 2.0, 0.0];
        m.enemies[0].pos_offset = [100.0, 20.0, 0.0];
        ecl_run(&mut m, &mut anm, 0, sys);
        let e = &m.enemies[0];
        assert_eq!(e.main.ctx.flt(V_FLT0), 94.5);
        assert_eq!(e.main.ctx.int(V_INT0), 94);
        assert_eq!(e.pos[0], -4.5);
    }

    // 07 §6 var 10099: card active ? spell bit 2 : spell bit 9 (captured).
    #[test]
    fn var_10099_spell_state() {
        let (_, mut m) = manager(&[&[]]);
        spawn(&mut m, 0, 0, -1, 0);
        let mut rng = crate::rng::Rng::default();
        let mut read = |m: &mut EnemyManager| {
            let e = m.enemies[0].clone();
            var_i(
                m,
                &crate::player::Player::default(),
                None,
                &mut rng,
                &e,
                &e.main.ctx,
                10099,
                0,
            )
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 155, &[0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.g.spell = Some(Spell {
            bonus: 33_000_000,
            max: 33_000_000,
            ..Default::default()
        });
        ecl_run(&mut m, &mut anm, 0, sys);
        let s = m.g.spell.as_ref().unwrap();
        assert_eq!((s.bonus, s.max), (33_000_000, FIXED_BONUS_CAP));
    }

    // 07 op 158: word 1 → start, word 2 → end, stored as fractions of the op enemy's max life; op 127 0 → target 1.0.
    #[test]
    fn op158_marker_fractions() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) =
            manager(&[&[ins(0, 158, &[2, 500, 1500, 0xFF_8040]), ins(0, 127, &[0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].max_life = 2000;
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(m.g.hud_markers[2], (0.75, 0.25, 0xFF_8040));
        assert_eq!(m.g.hud_bar_target, 1.0);
    }

    // 07 §5.2: native 18 at time 0 slows this frame's clock tick (live game speed).
    #[test]
    fn native18_slows_same_frame_tick() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[18, 4])]]);
        spawn(&mut m, 0, 0, -1, 0);
        anm.speed = 1.0;
        ecl_run(&mut m, &mut anm, 0, sys);
        let c = &m.enemies[0].main.ctx.clock;
        assert_eq!((c.cur, c.frac), (0, 0.25));
    }

    // 07 §5 (RE-Q54 Y): the step-7 boss timer and the manager clock tick at the speed set by this frame's native 18.
    #[test]
    fn native18_slows_same_frame_boss_timer() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[18, 4])]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].pos = [192.0, 100.0, 0.0];
        m.enemies[0].boss_timer.set(10);
        anm.speed = 1.0;
        m.step(&mut anm, sys);
        let t = m.enemies[0].boss_timer;
        assert_eq!((t.cur, t.frac), (10, 0.25));
        assert_eq!((m.clock.cur, m.clock.frac), (0, 0.25));
    }

    // 07 §5 ops 129/130/133/134 under the spell-practice guard (GameFlags bit 14 + bit 7 or 8).
    #[test]
    fn spell_practice_guard() {
        let mut ts = ts();
        let sys = &mut ts.sys();
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
            fresh_player(sys);
            spawn(&mut m, 0, 0, -1, 0);
            *sys.flags = f;
            ecl_run(&mut m, &mut anm, 0, sys);
            *sys.flags = 0;
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
        ecl_run(&mut m, &mut anm, 0, sys);
        let o = &m.enemies[1];
        assert_eq!((o.main.ctx.sub, o.main.depth, o.work_depth), (2, 0, 4));
        assert_eq!(o.main.stack[3].ip, ip + m.ecl.ins(ip).unwrap().size);
    }

    // 07 §5 op 128: a, b, c, d are raw words whatever the mask (d = f32 bits).
    #[test]
    fn op128_words_are_raw() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let mut i = ins(
            0,
            128,
            &[
                6,
                0.5f32.to_bits().cast_signed(),
                0,
                0,
                96.0f32.to_bits().cast_signed(),
            ],
        );
        i[10] = 0x1F;
        let (mut anm, mut m) = manager(&[&[i, ins(100, 81, &[1])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(m.enemies[0].orbit_target, 96.0);
    }

    // Op 95 in a thread: the main context takes the death sub, the threads are kept (07 §7 op 95).
    #[test]
    fn kill_all_from_thread() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 135, &[0, 2])], &[], &[ins(0, 95, &[])]]);
        spawn(&mut m, 0, 0, 1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        let e = &m.enemies[0];
        assert_eq!((e.life, e.death_sub, e.main.ctx.ip), (0, -1, m.ecl.subs[1]));
        assert!(e.threads[0].is_some());
    }

    // The caller is spared like any other enemy by flags B bit 6.
    #[test]
    fn kill_all_spares_caller_with_b6() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 95, &[])], &[]]);
        spawn(&mut m, 3, 0, 1, B_CANNOT_DIE);
        ecl_run(&mut m, &mut anm, 3, sys);
        let e = &m.enemies[3];
        assert_eq!((e.life, e.death_sub), (100, 1));
        assert_ne!(e.main.ctx.ip, m.ecl.subs[1]);
    }

    // 07 §7 row 109: the stored descriptor fires from base pos + shot offset with no life test, no mode / no-fire
    // filters and no rank add.
    #[test]
    fn op109_fires_stored_descriptor_raw() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 109, &[])]]);
        spawn(&mut m, 0, 0, -1, 0);
        let e = &mut m.enemies[0];
        e.life = 0;
        e.pos = [100.0, 50.0, 0.0];
        e.final_pos = [300.0, 300.0, 0.0];
        e.shot_offset = [4.0, -2.0, 0.0];
        e.nofire_r2 = 1.0e9;
        e.rank_counts = [5, 5, 5, 5];
        e.desc.count1 = 3;
        e.desc.count2 = 1;
        e.desc.speed1 = 2.0;
        e.desc.aim_mode = 1;
        e.desc.flags = 0x8000;
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(m.enemies[0].desc.origin, [104.0, 48.0, 0.0]);
        let b = crate::bm!(sys);
        let live: Vec<_> = b
            .bullets
            .iter()
            .filter(|x| x.state == 1)
            .map(|x| x.pos)
            .collect();
        assert_eq!(live, [[104.0, 48.0, 0.1]; 3]);
    }

    // 07 §12 native 31: item type 3 while the player is bombing, else 5.
    #[test]
    fn native31_item_type() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        for (bombing, kind) in [(false, 5), (true, 3)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[31, 0])]]);
            fresh_player(sys);
            crate::pl!(sys).bombing = bombing;
            spawn(&mut m, 0, 0, -1, 0);
            ecl_run(&mut m, &mut anm, 0, sys);
            let b = crate::bm!(sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[4, 0])]]);
        let cases = [
            ([330.0, 208.0, 0.0], [3.0, 0.0, 0.0]),
            ([250.0, 208.0, 0.0], [-20.0, 0.0, 0.0]),
            ([400.0, 108.0, 0.0], [2.0, -1.0, 0.0]),
        ];
        {
            let b = crate::bm!(sys);
            for (k, (pos, vel)) in cases.into_iter().enumerate() {
                let x = &mut b.bullets[k];
                (x.state, x.pos, x.vel, x.angle) = (1, pos, vel, 0.0);
            }
        }
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        let b = crate::bm!(sys);
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
        let mut bm = crate::bullet::BulletManager::new(
            &mut AnmManager::default(),
            &mut crate::rng::Rng::default(),
        );
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
            fresh_player(sys);
            spawn(&mut m, 0, 0, -1, 0);
            let e = &mut m.enemies[0];
            e.pos = [200.0, 100.0, 0.0];
            e.vm.rotation[2] = rot;
            e.main.ctx.set_flt(V_FLT0, 10.0);
            crate::pl!(sys).pos = [pp[0], pp[1], 0.0];
            ecl_run(&mut m, &mut anm, 0, sys);
            assert_eq!(crate::pl!(sys).state == 2, hit, "rot {rot} at {pp:?}");
        }
    }

    // 07 §12 native 11: small box (192 + 48 per side) grazes only when timer e+0x2E14 changed to a multiple
    // of 12; the big box (240) still misses there.
    #[test]
    fn native11_graze_every_12() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        for (cur, prev, grazed) in [(12, 11, 1), (12, 12, 0), (13, 12, 0)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[11, 0])]]);
            fresh_player(sys);
            spawn(&mut m, 0, 0, -1, 0);
            let e = &mut m.enemies[0];
            e.pos = [100.0, 100.0, 0.0];
            e.boss_timer.cur = cur;
            e.boss_timer.prev = prev;
            crate::pl!(sys).pos = [300.0, 100.0 + 130.0, 0.0];
            ecl_run(&mut m, &mut anm, 0, sys);
            let p = crate::pl!(sys);
            assert_eq!((p.grazes, p.state), (grazed, 0), "timer {cur}/{prev}");
        }
    }

    // 07 §12 native 8: parent 0 → 1 → 2 → 3; caller 3 and enemy 1 share var 10038, enemy 1 leads. Counters in
    // int vars 10005–10007; the caller turns 2 % toward leader + π + leader orbit velocity.
    #[test]
    fn native8_familiar_ring() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[8, 0])], &[]]);
        for (k, tag) in [(0, 0), (1, 5), (2, 6), (3, 5)] {
            spawn(&mut m, k, i32::from(k != 3), -1, 0);
            m.enemies[k].main.ctx.set_int(V_INT1 + 2, tag);
            m.enemies[k].main.ctx.set_int(V_INT1 + 1, -1);
            if k > 0 {
                m.enemies[k].root = Some(0);
                m.enemies[k - 1].chain_next = Some(k);
            }
        }
        m.enemies[1].orbit_av = 0.1;
        ecl_run(&mut m, &mut anm, 3, sys);
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
        let t = anmvm::wrap_add(anmvm::wrap_add(0.0, 6.283_185_5 / 2.0), 0.1);
        assert!((m.enemies[3].orbit_angle - t * 0.02).abs() < 1e-6);
        // A changed count sets the flag 10005 when the old count was not 0.
        m.enemies[3].main.ctx.set_int(V_INT0 + 6, 3);
        m.enemies[3].main.ctx.enter(&m.ecl, 0);
        ecl_run(&mut m, &mut anm, 3, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[8, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0 + 7), 0);
    }

    // 07 §12 native 13: stage tint request 0xFFC0_3030.
    #[test]
    fn native13_tint() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[13, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(anm.stage_tint, Some(0xFFC0_3030));
    }

    // 07 §12 native 16: chained children near a flag-0x100000 bullet with var 10038 == 0 get 60 and var 10019.
    #[test]
    fn native16_marks_children() {
        let mut ts = ts();
        let sys = &mut ts.sys();
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
            let b = crate::bm!(sys);
            (b.bullets[0].state, b.bullets[0].pos, b.bullets[0].flags) =
                (1, [140.0, 100.0, 0.0], 0x10_0000);
            (b.bullets[1].state, b.bullets[1].pos) = (1, [250.0, 100.0, 0.0]);
        }
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 136, &[24, 0])]]);
        spawn(&mut m, 0, 0, -1, 0);
        crate::pl!(sys).stats.spells = 7;
        ecl_run(&mut m, &mut anm, 0, sys);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0), 7);
    }

    // 07 §6: an int read of 10033 draws one Next32 and truncates Rng_Float.
    #[test]
    fn int_read_10033_draws() {
        let (_, mut m) = manager(&[&[]]);
        spawn(&mut m, 0, 0, -1, 0);
        let mut rng = crate::rng::Rng::default();
        let mut r = rng.clone();
        r.next_u32();
        let e = m.enemies[0].clone();
        assert_eq!(
            var_i(
                &mut m,
                &crate::player::Player::default(),
                None,
                &mut rng,
                &e,
                &e.main.ctx,
                10033,
                0
            ),
            0
        );
        assert_eq!(rng.next_u16(), r.next_u16());
    }

    // 07 §4.2 / §5.5: an unregistered index enters sub 0 (table zero-filled); a negative entry still skips the
    // current instruction, pushes and increments the depth, but leaves the main context in its sub.
    #[test]
    fn interrupt_table_default_and_negative() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[ins(0, 0, &[])], &[ins(5, 0, &[]), ins(5, 0, &[])]]);
        spawn(&mut m, 0, 1, -1, 0);
        m.enemies[0].pending_interrupt = 3;
        ecl_run(&mut m, &mut anm, 0, sys);
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
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[
            &[ins(0, 135, &[0, 1]), ins(100, 0, &[])],
            &[ins(0, 125, &[1]), ins(0, 83, &[1]), ins(5, 0, &[])],
            &[ins(50, 0, &[])],
        ]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].interrupts[1] = 2;
        let (s0, s1) = (m.ecl.sub_start(0).unwrap(), m.ecl.sub_start(1).unwrap());
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        for (clock, after, sfx) in [(11, 12, true), (12, 12, false)] {
            let (mut anm, mut m) = manager(&[&[ins(0, 181, &[])]]);
            fresh_player(sys);
            crate::pl!(sys).stats.clock = clock;
            spawn(&mut m, 0, 0, -1, 0);
            sys.sound.drain();
            ecl_run(&mut m, &mut anm, 0, sys);
            assert_eq!(crate::pl!(sys).stats.clock, after);
            let played = sys
                .sound
                .drain()
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let iadd = ins_v(0, 10, 1, &[10042, 5]);
        let inc = ins_v(0, 30, 1, &[10043]);
        let fadd = ins_v(
            0,
            15,
            1,
            &[
                10000.0f32.to_bits().cast_signed(),
                2.0f32.to_bits().cast_signed(),
            ],
        );
        let iadd_ctx = ins_v(0, 10, 1, &[10000, 3]);
        let (mut anm, mut m) = manager(&[&[iadd, inc, fadd, iadd_ctx]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].main.ctx.set_int(V_INT0, 4);
        (m.enemies[0].pos, m.enemies[0].pos_offset) = ([7.0, 8.0, 0.0], [93.0, 92.0, 0.0]);
        ecl_run(&mut m, &mut anm, 0, sys);
        let i = |o: usize| m.ecl.ins(m.ecl.subs[0] + o).unwrap();
        assert_eq!(m.ecl.arg(&i(0), 0).cast_signed(), 10047);
        assert_eq!(m.ecl.arg(&i(20), 0).cast_signed(), 10044);
        assert_eq!(f32::from_bits(m.ecl.arg(&i(36), 0)), 10002.0);
        assert_eq!(m.enemies[0].pos, [7.0, 8.0, 0.0]);
        assert_eq!(m.enemies[0].main.ctx.int(V_INT0), 7);
    }

    // 07 §7 ops 86 / 87: one getter call (one RNG draw for 10082 / 10032), on self or another boss.
    #[test]
    fn boss_var_single_getter() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let op87 = ins_v(0, 87, 1, &[10016.0f32.to_bits().cast_signed(), 10082, 0]);
        let op86 = ins_v(0, 86, 1, &[10000, 10032, 1]);
        let (mut anm, mut m) = manager(&[&[op87, op86]]);
        spawn(&mut m, 0, 0, -1, 0);
        spawn(&mut m, 1, 0, -1, 0);
        m.bosses = [Some(0), Some(1), None, None];
        let mut rng = sys.rng.clone();
        let (f, i) = (
            rng.next_f32() * 2.0 * PI - PI,
            (rng.next_u32() & 0x7FFF_FFFF).cast_signed(),
        );
        ecl_run(&mut m, &mut anm, 0, sys);
        let ctx = &m.enemies[0].main.ctx;
        assert_eq!((ctx.flt(V_FLT0), ctx.vars[V_INT0].cast_signed()), (f, i));
        assert_eq!(sys.rng.state, rng.state);
    }

    // 07 op 93/94 and §10.5: a spawner with life ≤ 0 reads no argument (no RNG draw from var 10032); ops 90–92 set
    // the manager flag, ops 93/94 leave it alone.
    #[test]
    fn spawn_ops_life_gate() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let rx = 10032.0f32.to_bits().cast_signed();
        for op in [90i16, 94] {
            let args: &[i32] = if op == 94 {
                &[1, rx, 0, 0, 10, 0, 100]
            } else {
                &[1, rx, 0, 10, 0, 100]
            };
            for life in [0, 100] {
                let (mut anm, mut m) = manager(&[&[ins_v(0, op, 2, args)], &[]]);
                fresh_player(sys);
                spawn(&mut m, 0, 0, -1, 0);
                m.enemies[0].life = life;
                let mut r = sys.rng.clone();
                ecl_run(&mut m, &mut anm, 0, sys);
                if life == 0 {
                    assert_eq!(sys.rng.next_u16(), r.next_u16());
                    assert!(!m.enemies[1].alive());
                    assert_eq!(m.full, op == 90);
                } else {
                    r.next_u32();
                    assert_eq!(sys.rng.next_u16(), r.next_u16());
                    assert!(m.enemies[1].alive());
                }
            }
        }
    }

    // 07 §12 native 2: edge flips without direction tests; angle only when a branch ran (gravity included).
    #[test]
    fn native2_bounce_no_direction_test() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let mut run = |pos: [f32; 3], vel: [f32; 3], cap: f32| {
            let (mut anm, mut m) = manager(&[&[ins(0, 136, &[2, 0])]]);
            fresh_player(sys);
            spawn(&mut m, 0, 0, -1, 0);
            let e = &mut m.enemies[0];
            (e.pos, e.vel, e.angle) = (pos, vel, 9.0);
            e.main.ctx.vars[V_FLT0 + 6] = 0.5f32.to_bits();
            e.main.ctx.vars[V_FLT0 + 7] = cap.to_bits();
            ecl_run(&mut m, &mut anm, 0, sys);
            (m.enemies[0].vel, m.enemies[0].angle)
        };
        let (v, a) = run([-1.0, 100.0, 0.0], [2.0, 1.0, 0.0], 0.0);
        assert_eq!((v[0], v[1]), (-2.0, 1.0));
        assert_eq!(a, fmath::atan2_f(1.0, -2.0));
        let (v, a) = run([100.0, -65.0, 0.0], [0.0, 3.0, 0.0], 0.0);
        assert_eq!((v[1], a), (-3.0, fmath::atan2_f(-3.0, 0.0)));
        let (v, a) = run([100.0, 100.0, 0.0], [1.0, 1.0, 0.0], 2.0);
        assert_eq!((v[1], a), (1.5, fmath::atan2_f(1.5, 1.0)));
        let (_, a) = run([100.0, 100.0, 0.0], [1.0, 1.0, 0.0], 0.0);
        assert_eq!(a, 9.0);
    }

    // 07 §6 var 10100: the remaining-time timer +0x108, also with no card active.
    #[test]
    fn var_10100_remaining_timer() {
        let (_, mut m) = manager(&[&[]]);
        let mut rng = crate::rng::Rng::default();
        spawn(&mut m, 0, 0, -1, 0);
        assert_eq!(m.g.spell_remaining.cur, 0);
        m.g.spell_remaining.set(764);
        m.g.spell_remaining.dec(1.0, false);
        let e = m.enemies[0].clone();
        assert!(m.g.spell.is_none());
        assert_eq!(
            var_i(
                &mut m,
                &crate::player::Player::default(),
                None,
                &mut rng,
                &e,
                &e.main.ctx,
                10100,
                0
            ),
            763
        );
        for _ in 0..4 {
            m.g.spell_remaining.dec(0.25, false);
        }
        assert_eq!(
            var_i(
                &mut m,
                &crate::player::Player::default(),
                None,
                &mut rng,
                &e,
                &e.main.ctx,
                10100,
                0
            ),
            762
        );
    }

    fn child_op(op: i16, sub: i32) -> Vec<u8> {
        ins(0, op, &[sub, 0, 0, 10, 0, 100])
    }

    // 07 §10.5: ops 90–92 leave the mode bits e+0x3330 at 0, so a youkai-only time-0 instruction runs in human
    // mode; op 92 writes the follow offset, final pos and bit 9 only after the spawn-time run.
    #[test]
    fn op92_follow_and_mode_bits_after_spawn_run() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let mut youkai_only = ins_v(0, 6, 1, &[10000, 7]);
        youkai_only[9] = 0x4F;
        let (x, y) = (
            5.0f32.to_bits().cast_signed(),
            6.0f32.to_bits().cast_signed(),
        );
        let op92 = ins(0, 92, &[1, x, y, 10, 0, 100]);
        let (mut anm, mut m) = manager(&[&[op92], &[youkai_only], &[]]);
        spawn(&mut m, 0, 0, -1, 0);
        m.enemies[0].pos = [100.0, 50.0, 0.0];
        ecl_run(&mut m, &mut anm, 0, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
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
        ecl_run(&mut m, &mut anm, 0, sys);
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
        sys.fx.get(c.fx_aura, |x| {
            assert_eq!(x.vm.pending_interrupt, 1);
            assert_eq!(x.vm.flags & anmvm::F_COLOR2, 0);
        });
    }

    // 07 §10.5: a chained child links the new enemy right after itself and overwrites its own +8.
    #[test]
    fn chained_child_links_after_itself() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[child_op(90, 1)], &[]]);
        for k in 0..3 {
            spawn(&mut m, k, 1, -1, 0);
        }
        (m.enemies[0].chain_next, m.enemies[1].chain_next) = (Some(1), Some(2));
        (m.enemies[1].chain_prev, m.enemies[2].chain_prev) = (Some(0), Some(1));
        (m.enemies[1].root, m.enemies[2].root) = (Some(0), Some(0));
        m.enemies[1].main.ctx.enter(&m.ecl, 0);
        ecl_run(&mut m, &mut anm, 1, sys);
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
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[child_op(90, 1)], &[]]);
        spawn(&mut m, 0, 0, -1, 0);
        ecl_run(&mut m, &mut anm, 0, sys);
        let c = &m.enemies[1];
        assert_eq!(c.slot, 1);
        sys.fx.get(c.fx_aura, |x| {
            assert!(x.vm.angular_vel[2].is_sign_negative());
        });
    }

    // 07 §7 op 174: interrupt (player mode ≠ 0) + 1, odd slot negates the spin, colour2 untouched; ops 80/81 bit 1
    // switch the aura's colour2 flag.
    #[test]
    fn aura_op174_and_contact_flag() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[
            &[ins(0, 174, &[0]), ins(1, 81, &[2]), ins(2, 80, &[2])],
            &[],
        ]);
        spawn(&mut m, 1, 0, -1, 0);
        m.enemies[1].slot = 1;
        m.g.player_mode = 1;
        ecl_run(&mut m, &mut anm, 1, sys);
        let fx = m.enemies[1].fx_aura;
        assert_ne!(fx, crate::effect::NONE);
        let spin = |m: &EnemyManager, fx: &mut crate::effect::Effects| {
            let mut r = (0, 0.0, 0);
            fx.get(m.enemies[1].fx_aura, |x| {
                r = (
                    x.vm.pending_interrupt,
                    x.vm.angular_vel[2],
                    x.vm.flags & anmvm::F_COLOR2,
                );
            });
            r
        };
        let (int, w, c2) = spin(&m, sys.fx);
        assert_eq!((int, c2), (2, 0));
        assert!(w.is_sign_negative());
        ecl_run(&mut m, &mut anm, 1, sys);
        assert_eq!(spin(&m, sys.fx).2, anmvm::F_COLOR2);
        ecl_run(&mut m, &mut anm, 1, sys);
        assert_eq!(spin(&m, sys.fx).2, 0);
    }

    // 07 §4.1 step 3: only pos is written; prev pos stays (0, 0, 0), final pos is recomputed by the run.
    #[test]
    fn spawn_keeps_template_prev_pos() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let (mut anm, mut m) = manager(&[&[]]);
        let i = m.spawn(&mut anm, 0, [100.0, 50.0, 0.0], 10, 0, 100, false, sys);
        let e = &m.enemies[i.unwrap()];
        assert_eq!(e.prev_pos, [0.0; 3]);
        assert_eq!(e.final_pos, [100.0, 50.0, 0.0]);
    }

    // 07 §9.1 step 1: the filters run on the raw instruction before any argument read, so a skipped bullet op
    // with a `$rand` angle draws nothing; the mode filter tests the enemy's flag A bit 11, not the player's mode.
    #[test]
    fn bullet_filters_before_decode() {
        let mut ts = ts();
        let sys = &mut ts.sys();
        let f = |x: f32| x.to_bits().cast_signed();
        let op = |flags: u32| {
            ins_v(
                0,
                96,
                1 << 6,
                &[0, 3, 1, f(2.0), f(1.0), f(10033.0), 0, flags.cast_signed()],
            )
        };
        for (flags, youkai, r2) in [
            (0x8000, false, 0.0),
            (0x10000, true, 0.0),
            (0, false, 1024.0),
        ] {
            let (mut anm, mut m) = manager(&[&[op(flags)]]);
            fresh_player(sys);
            spawn(&mut m, 0, 0, -1, 0);
            m.g.player_mode = 1 - i32::from(youkai);
            m.enemies[0].set_flag(A_YOUKAI, youkai);
            m.enemies[0].nofire_r2 = r2;
            m.enemies[0].pos = [10.0, 20.0, 0.0];
            m.enemies[0].final_pos = [10.0, 20.0, 0.0];
            m.g.player = [30.0, 20.0, 0.0];
            let before = sys.rng.counter;
            ecl_run(&mut m, &mut anm, 0, sys);
            assert_eq!(sys.rng.counter, before, "flags {flags:#x}");
        }
    }
}
