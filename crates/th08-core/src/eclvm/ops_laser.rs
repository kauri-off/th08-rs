//! ECL laser ops: laser blocks and edits of the enemy's laser slots (07 §7 ops 114–121, 154, 167, 170–172; §9.4).

use super::{Exec, add3, at};
use crate::anmvm::{self, wrap_add};
use crate::bullet::{Laser, LaserDesc};
use crate::enemy::{Thread, V_INT1};
use crate::types::{LaserIdx, angle_from};
use th08_formats::ecl::{Arg, Ins, LaserBlock, LaserOp};

impl Exec<'_> {
    pub(super) fn op_laser(&mut self, t: &mut Thread, ins: &Ins, op: LaserOp) {
        match op {
            LaserOp::Laser { fixed } => self.laser_op(t, ins, fixed),
            LaserOp::LaserSlot(k) => {
                // 0..=31, so the cast is exact.
                self.e.laser_cur = self.ai(t, k).clamp(0, 31).cast_unsigned() as usize;
            }
            LaserOp::LaserRotate { k, a } => {
                self.laser_angle(t, k, a, |l, _, a| wrap_add(l.angle, a));
            }
            LaserOp::LaserAim { k, a } => {
                self.laser_angle(t, k, a, |l, p, a| angle_from(l.origin, p) + a);
            }
            LaserOp::LaserSetAngle { k, a } => self.laser_angle(t, k, a, |_, _, a| a),
            LaserOp::LaserOffset { k, off } => {
                let k = self.ai(t, k);
                let off = off.map(|a| self.af(t, a));
                let p = add3(self.e.final_pos, off);
                self.with_laser(k, |l, _| l.origin = p);
            }
            // Result in var 10038.
            LaserOp::LaserAlive(k) => {
                let k = self.ai(t, k);
                let mut alive = false;
                self.with_laser(k, |_, _| alive = true);
                t.ctx.set_int(V_INT1 + 2, i32::from(alive));
            }
            LaserOp::LaserCancel(k) => {
                let k = self.ai(t, k);
                self.with_laser(k, |l, _| {
                    if l.state < 2 {
                        l.state = 2;
                        l.timer = anmvm::Timer::default();
                    }
                });
            }
            LaserOp::LaserClear => self.e.lasers = [None; 32],
            // Laser +0x599 byte / +0x560 max length / +0x558, +0x55C tail and head.
            // Word 1 is read through the float getter for all three ops (170 then stores its raw low byte).
            LaserOp::LaserHideEmitter { k, a } => {
                let k = self.ai(t, k);
                let _ = self.af(t, a);
                let byte = a.raw as u8;
                self.with_laser(k, |l, _| l.hide_emitter = byte);
            }
            LaserOp::LaserMaxLen { k, a } => {
                let k = self.ai(t, k);
                let a = self.af(t, a);
                self.with_laser(k, |l, _| l.max_len = a);
            }
            LaserOp::LaserLen { k, tail, head } => {
                let k = self.ai(t, k);
                let (a, b) = (self.af(t, tail), self.af(t, head));
                self.with_laser(k, |l, _| (l.tail, l.head) = (a, b));
            }
        }
    }

    // Ops 117 / 118 / 167: new angle of laser k from its old angle, the player pos and `a`.
    fn laser_angle(
        &mut self,
        t: &Thread,
        k: Arg,
        a: Arg,
        f: impl FnOnce(&Laser, [f32; 3], f32) -> f32,
    ) {
        let (k, a) = (self.ai(t, k), self.af(t, a));
        self.with_laser(k, |l, p| l.angle = f(l, p, a));
    }

    // Laser block (64 bytes) → laser slot[current]; +0x34/+0x38/+0x3C raw.
    fn laser_op(&mut self, t: &Thread, ins: &Ins, fixed: bool) {
        let b = LaserBlock::parse(self.m.ecl.bytes(ins));
        let mut d = LaserDesc {
            kind: self.vi(t, i32::from(b.kind), b.var(0)) as i16,
            colour: self.vi(t, i32::from(b.colour), b.var(1)) as i16,
            angle: self.vf(t, b.angle, b.var(2)),
            speed: self.vf(t, b.speed, b.var(3)),
            tail: self.vf(t, b.tail, b.var(4)),
            head: self.vf(t, b.head, b.var(5)),
            max_len: self.vf(t, b.max_len, b.var(6)),
            width: self.vf(t, b.width, b.var(7)),
            startup: self.vi(t, b.startup, b.var(8)),
            duration: self.vi(t, b.duration, b.var(9)),
            fade: self.vi(t, b.fade, b.var(10)),
            hit_start: b.hit_start,
            fade_hit: b.fade_hit,
            flags: b.flags,
            fixed,
            origin: [0.0; 3],
        };
        d.origin = add3(self.e.final_pos, self.e.shot_offset);
        self.e.laser_desc = d.clone();
        let id = crate::bm!(self.sys).spawn_laser(self.anm, self.sys.rng, &d, self.m.g.player);
        let cur = self.e.laser_cur.min(31);
        // Enemy laser slots hold the raw slot number.
        self.e.lasers[cur] = id.map(usize::from);
    }

    pub(super) fn with_laser(&mut self, k: i32, f: impl FnOnce(&mut Laser, [f32; 3])) {
        let Some(&Some(id)) = at(&self.e.lasers, k) else {
            return;
        };
        let p = self.m.g.player;
        if let Some(l) = crate::bm!(self.sys).laser_mut(LaserIdx(id as u16)) {
            f(l, p);
        }
    }
}
