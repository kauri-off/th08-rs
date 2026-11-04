//! ECL variable ops and interpolation slots (07 §5.6, §6, §7 ops 6–39, 86/87, 166, 169).

use super::{Exec, ease, var_f, var_i};
use crate::anmvm::{self, wrap_add};
use crate::enemy::{Interp, Thread};
use crate::fmath;
use std::f32::consts::PI;
use th08_formats::ecl::{Arg, Arith, Dst, Ins, VarOp};

// Ops 10–14 / 20–24: C truncating / and %, two's complement wrap; the 0 result for b == 0 is a port guard.
fn int_arith(kind: Arith, a: i32, b: i32) -> i32 {
    match kind {
        Arith::Add => a.wrapping_add(b),
        Arith::Sub => a.wrapping_sub(b),
        Arith::Mul => a.wrapping_mul(b),
        Arith::Div => a.checked_div(b).unwrap_or(0),
        Arith::Mod => a.checked_rem(b).unwrap_or(0),
    }
}

// Ops 15–19 / 25–29 (f32, fmod = C `%`).
fn flt_arith(kind: Arith, a: f32, b: f32) -> f32 {
    match kind {
        Arith::Add => a + b,
        Arith::Sub => a - b,
        Arith::Mul => a * b,
        Arith::Div => a / b,
        Arith::Mod => a % b,
    }
}

// Ops 8 / 9: one Rng_Next16, bit 0 set keeps the sign.
fn rand_sign<T: std::ops::Neg<Output = T>>(r: u16, b: T) -> T {
    if r & 1 != 0 { b } else { -b }
}

// EclInterp_Fn7 0x421180: f32 product/sum order as in the disasm; any other order is 1 ulp off.
pub(super) fn hermite(t: f32, v: [f32; 4]) -> f32 {
    let h0 = ((t - 1.0) * (t - 1.0)) * (2.0 * t + 1.0);
    let h1 = (t * t) * (3.0 - 2.0 * t);
    let h2 = ((1.0 - t) * (1.0 - t)) * t;
    let h3 = ((t - 1.0) * t) * t;
    ((h0 * v[0] + h1 * v[1]) + h2 * v[2]) + h3 * v[3]
}

impl Exec<'_> {
    pub(super) fn op_var(&mut self, t: &mut Thread, ins: &Ins, op: VarOp) {
        match op {
            VarOp::ISet { dst, v } => {
                let v = self.ai(t, v);
                self.set_i(t, ins, dst, v);
            }
            VarOp::FSet { dst, v } => {
                let v = self.af(t, v);
                self.set_f(t, ins, dst, v);
            }
            VarOp::IRandSign { dst, v } => {
                let b = self.ai(t, v);
                let v = rand_sign(self.sys.rng.next_u16(), b);
                self.set_i(t, ins, dst, v);
            }
            VarOp::FRandSign { dst, v } => {
                let b = self.af(t, v);
                let v = rand_sign(self.sys.rng.next_u16(), b);
                self.set_f(t, ins, dst, v);
            }
            VarOp::IArith { kind, dst, rhs } => {
                let (a, b) = (self.ai_dst(t, dst), self.ai(t, rhs));
                self.set_i(t, ins, dst, int_arith(kind, a, b));
            }
            VarOp::IArith3 { kind, dst, a, b } => {
                let (a, b) = (self.ai(t, a), self.ai(t, b));
                self.set_i(t, ins, dst, int_arith(kind, a, b));
            }
            VarOp::FArith { kind, dst, rhs } => {
                let (a, b) = (self.af_dst(t, dst), self.af(t, rhs));
                self.set_f(t, ins, dst, flt_arith(kind, a, b));
            }
            VarOp::FArith3 { kind, dst, a, b } => {
                let (a, b) = (self.af(t, a), self.af(t, b));
                self.set_f(t, ins, dst, flt_arith(kind, a, b));
            }
            VarOp::Inc(dst) => {
                let a = self.ai_dst(t, dst);
                self.set_i(t, ins, dst, a.wrapping_add(1));
            }
            VarOp::Dec(dst) => {
                let a = self.ai_dst(t, dst);
                self.set_i(t, ins, dst, a.wrapping_sub(1));
            }
            VarOp::FSin { dst, a } => {
                let b = self.af(t, a);
                self.set_f(t, ins, dst, fmath::sin_f(b));
            }
            VarOp::FCos { dst, a } => {
                let b = self.af(t, a);
                self.set_f(t, ins, dst, fmath::cos_f(b));
            }
            VarOp::FAtan2 {
                dst,
                x1,
                y1,
                x2,
                y2,
            } => {
                let (x1, y1, x2, y2) = (
                    self.af(t, x1),
                    self.af(t, y1),
                    self.af(t, x2),
                    self.af(t, y2),
                );
                self.set_f(t, ins, dst, fmath::atan2_f(y2 - y1, x2 - x1));
            }
            VarOp::FDist {
                dst,
                x1,
                y1,
                x2,
                y2,
            } => {
                let (x1, y1, x2, y2) = (
                    self.af(t, x1),
                    self.af(t, y1),
                    self.af(t, x2),
                    self.af(t, y2),
                );
                self.set_f(t, ins, dst, ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt());
            }
            VarOp::FLerp { dst, b, c, d } => {
                let (b, c, d) = (self.af(t, b), self.af(t, c), self.af(t, d));
                self.set_f(t, ins, dst, (b - c) * d + c);
            }
            VarOp::Interp {
                target_bits,
                dur,
                func,
                mode,
                abcd,
            } => self.interp_op(t, target_bits, dur, func, mode, abcd),
            VarOp::NormAngle(dst) => {
                let a = self.af_dst(t, dst);
                self.set_f(t, ins, dst, wrap_add(a, 0.0));
            }
            // 38 wraps the angle and writes x first; 166 does not wrap and writes y first.
            VarOp::Polar { x, y, ang, r } => {
                let (ang, r) = (self.af(t, ang), self.af(t, r));
                // `call; fmul` (05 §10 trig sites): f32(cos·r), one rounding.
                let v = fmath::polar(wrap_add(ang, 0.0), r);
                self.set_f(t, ins, x, v[0]);
                self.set_f(t, ins, y, v[1]);
            }
            VarOp::PolarYs { x, y, ang, r } => {
                let (ang, r) = (self.af(t, ang), self.af(t, r));
                let v = fmath::polar(ang, r);
                self.set_f(t, ins, y, v[1]);
                self.set_f(t, ins, x, v[0]);
            }
            VarOp::IGetBoss { dst, var, slot } => self.get_boss(t, ins, dst, var, slot, false),
            VarOp::FGetBoss { dst, var, slot } => self.get_boss(t, ins, dst, var, slot, true),
            // Op 169: leftward / rightward cone from one Rng_Float.
            VarOp::RandAwayAngle(dst) => {
                let (x, px) = (self.e.pos[0], self.m.g.player[0]);
                let r = self.sys.rng.next_f32() * (PI / 2.0);
                let a = if x > px && x > 96.0 || x > 288.0 {
                    wrap_add(r + 3.0 * PI / 4.0, 0.0)
                } else {
                    r - PI / 4.0
                };
                self.set_f(t, ins, dst, a);
            }
        }
    }

    // Op 36: target id is a float literal; reuse the slot with the same target, else the first free one.
    fn interp_op(
        &mut self,
        t: &mut Thread,
        target_bits: u32,
        dur: Arg,
        func: Arg,
        mode: Arg,
        abcd: [Arg; 4],
    ) {
        let target = f32::from_bits(target_bits) as i32;
        let dur = self.ai(t, dur);
        let func = self.ai(t, func);
        let mode = self.ai(t, mode);
        // a–d are resolved once, at op time.
        let abcd: [f32; 4] = abcd.map(|a| self.af(t, a));
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

    // Ops 86 / 87: variable `var` evaluated once, with the op's own getter, on boss[slot]'s main context:
    // 87 only if the boss exists; 86 has no such test.
    fn get_boss(&mut self, t: &mut Thread, ins: &Ins, dst: Dst, var: Arg, slot: Arg, float: bool) {
        let (var, slot) = (self.ai(t, var), self.ai(t, slot));
        // Op 86: an empty entry faults in the original for enemy/context ids (0 uses); skipped as a stand-in.
        let Some(b) = self.m.boss(slot) else {
            return;
        };
        let other = (b != self.idx).then(|| self.m.enemies[b].clone());
        let (e, ctx) = match &other {
            Some(o) => (&**o, &o.main.ctx),
            None => (&*self.e, &t.ctx),
        };
        if float {
            let v = var_f(self.m, self.sys.rng, e, ctx, var as f32, self.sys.rank.rank);
            self.set_f(t, ins, dst, v);
        } else {
            let pl = crate::pl!(self.sys);
            let v = var_i(
                self.m,
                pl,
                self.sys.bullets.as_deref(),
                self.sys.rng,
                e,
                ctx,
                var,
                self.sys.rank.rank,
            );
            self.set_i(t, ins, dst, v);
        }
    }

    // Variable interpolation slots; a position target turns into velocity + angle, pos restored.
    pub(super) fn run_interp(&mut self, t: &mut Thread) {
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
                hermite(tt, v)
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
            e.angle = fmath::atan2_f(e.vel[1], e.vel[0]);
            e.pos = old;
        }
    }
}
