//! ECL movement ops: position, polar / interpolated / orbit modes, clamp, boxes (07 §7 ops 63–78, 82, 178; §8).

use super::Exec;
use crate::anmvm::wrap_add;
use crate::enemy::{A_CLAMP, A_MIRROR, Thread};
use crate::types::angle_from;
use std::f32::consts::PI;
use th08_formats::ecl::MoveOp;

impl Exec<'_> {
    pub(super) fn op_move(&mut self, t: &Thread, op: MoveOp) {
        match op {
            MoveOp::SetPos { x, y } => {
                self.e.pos = [self.af(t, x), self.af(t, y), 0.0];
                self.e.clamp_pos();
            }
            // Op 64: delta = target − final pos, start = pos.
            MoveOp::MoveTo { dur, ease, x, y } => {
                let (dur, ease) = (self.ai(t, dur), self.ai(t, ease));
                let (x, y) = (self.af(t, x), self.af(t, y));
                let f = self.e.final_pos;
                let mut delta = [x - f[0], y - f[1], 0.0 - f[2]];
                if self.e.flags & A_MIRROR != 0 {
                    delta[0] = -delta[0];
                }
                self.mode2(dur, ease, self.e.pos, delta);
            }
            MoveOp::SetDir { ang, spd } => {
                let (a, s) = (self.af(t, ang), self.af(t, spd));
                self.mode1(a, s, 0);
            }
            MoveOp::MoveDir {
                dur,
                ease,
                ang,
                spd,
            } => {
                let (dur, ease, a, s) = (
                    self.ai(t, dur),
                    self.ai(t, ease),
                    self.af(t, ang),
                    self.af(t, spd),
                );
                self.move_dir(dur, ease, a, s, true);
            }
            MoveOp::MoveRand { dur, ease, spd } | MoveOp::MoveRand2 { dur, ease, spd } => {
                let (dur, ease, s) = (self.ai(t, dur), self.ai(t, ease), self.af(t, spd));
                let a = self.random_dir(matches!(op, MoveOp::MoveRand2 { .. }));
                self.move_dir(dur, ease, a, s, false);
            }
            // Op 68: movement mode unchanged.
            MoveOp::SetDirAim { off, spd } => {
                let (off, s) = (self.af(t, off), self.af(t, spd));
                self.e.angle = wrap_add(self.player_angle(self.e.pos) + off, 0.0);
                self.e.speed = s;
            }
            MoveOp::MoveDirAim {
                dur,
                ease,
                off,
                spd,
            } => {
                let (dur, ease, off, s) = (
                    self.ai(t, dur),
                    self.ai(t, ease),
                    self.af(t, off),
                    self.af(t, spd),
                );
                if dur < 1 {
                    let a = self.player_angle(self.e.pos) + off;
                    self.mode1(a, s, dur);
                } else {
                    // Quirk kept (07 §7 op 69): with a duration the movement is not aimed (same as op 66).
                    self.move_dir(dur, ease, off, s, true);
                }
            }
            MoveOp::SetAngVel(v) => {
                self.e.angvel = self.af(t, v);
                self.e.set_move_mode(1);
            }
            MoveOp::SetAccel(v) => {
                self.e.accel = self.af(t, v);
                self.e.set_move_mode(1);
            }
            MoveOp::Orbit {
                dur,
                cx,
                cy,
                a,
                av,
                r,
                rv,
            } => {
                let dur = self.ai(t, dur);
                self.e.interp_start = [self.af(t, cx), self.af(t, cy), 0.0];
                self.e.orbit_angle = self.af(t, a);
                self.e.orbit_av = self.af(t, av);
                self.e.radius = self.af(t, r);
                self.e.radius_vel = self.af(t, rv);
                self.mode3(dur);
            }
            MoveOp::OrbitHere { dur, a, av, rv } => {
                let dur = self.ai(t, dur);
                self.e.interp_start = self.e.pos;
                self.e.orbit_angle = self.af(t, a);
                self.e.orbit_av = self.af(t, av);
                self.e.radius = 0.0;
                self.e.radius_vel = self.af(t, rv);
                self.mode3(dur);
            }
            MoveOp::OrbitVel { dur, av, rv } => {
                let dur = self.ai(t, dur);
                self.e.orbit_av = self.af(t, av);
                self.e.radius_vel = self.af(t, rv);
                self.mode3(dur);
            }
            MoveOp::ClampRect(r) => {
                self.e.clamp = r.map(|a| self.af(t, a));
                self.e.flags |= A_CLAMP;
            }
            MoveOp::ClampOff => self.e.flags &= !A_CLAMP,
            MoveOp::Hitbox { w, h } => {
                self.e.hitbox[0] = self.af(t, w);
                self.e.hitbox[1] = self.af(t, h);
            }
            MoveOp::Shotbox { w, h } => {
                self.e.shotbox[0] = self.af(t, w);
                self.e.shotbox[1] = self.af(t, h);
            }
            MoveOp::NofireRadius(r) => {
                let r = self.af(t, r);
                self.e.nofire_r2 = r * r;
            }
        }
    }

    fn player_angle(&self, p: [f32; 3]) -> f32 {
        angle_from(p, self.m.g.player)
    }

    // Mode 1 polar: angle wrapped; `dur` 0 = no duration.
    fn mode1(&mut self, ang: f32, spd: f32, dur: i32) {
        let e = &mut *self.e;
        e.angle = wrap_add(ang, 0.0);
        e.speed = spd;
        e.move_dur = dur;
        e.move_timer.set(dur);
        e.set_move_mode(1);
    }

    // Mode 2 interpolate: velocity 0, ease in flag A bits 14–16.
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

    // Ops 66 / 67 / 178: dur < 1 → mode 1; else mode 2 from the final pos. `mirror`: op 66
    // negates delta.x for mirrored enemies (`Ecl_OpMoveDir` 0x420D10); ops 67/178 do not.
    fn move_dir(&mut self, dur: i32, ease: i32, ang: f32, spd: f32, mirror: bool) {
        if dur < 1 {
            self.mode1(ang, spd, 0);
        } else {
            // 05 §10 trig sites (0x420DAE / 0x422315): f32(f32(cos a·spd)·dur), cos·spd rounded once.
            let v = crate::fmath::polar(ang, spd);
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

    // Random direction 0x422020 (op 67) / 0x4224A0 (op 178); one Rng_Float per line, in order.
    fn random_dir(&mut self, op178: bool) -> f32 {
        let rng = &mut self.sys.rng;
        let e = &*self.e;
        let (x, y) = (e.pos[0], e.pos[1]);
        let px = self.m.g.player[0];
        let [x0, y0, x1, y1] = e.clamp;
        let wrap = |a: f32| wrap_add(a, 0.0);
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
        // Two sequential tests; when the bands overlap both can flip, so the move goes up.
        if y < y0 + 48.0 && a < 0.0 {
            a = -a;
        }
        if y > y1 - 48.0 && a > 0.0 {
            a = -a;
        }
        a
    }
}
