//! ECL native functions, op 136 once / op 137 every POST; `arg` = instr +0x10 (07 §12).

use super::Exec;
use crate::anmvm::wrap_add;
use crate::bullet::{MAX_BULLETS, Warp};
use crate::enemy::{A_KEEP_OFFSCREEN, B_PAUSED, MAX_ENEMIES, Thread, V_FLT0, V_INT0, V_INT1};
use crate::fmath;
use crate::types::TWO_PI;
use std::f32::consts::PI;

// Bullet flag +0xDB0 0x100000: marks bullets that natives 16 and 27 act on.
const BULLET_MARKED: u32 = 0x10_0000;
// Screen-effect colour 0xFFFFFFFF passed as the i32 parameter (natives 1 / 17, 13 §3).
const WHITE: i32 = 0xFFFF_FFFF_u32.cast_signed();

impl Exec<'_> {
    pub(super) fn native(&mut self, t: &mut Thread, n: i32, arg: i32) {
        match n {
            0 => self
                .sys
                .ov
                .set_darkness(t.ctx.int(V_INT0), t.ctx.flt(V_FLT0)),
            // Screen flash / shake parameters (13 §3).
            1 => self.sys.fx.screen(3, 60, 1, WHITE, 0, 0x15),
            17 => self.sys.fx.screen(3, 180, 1, WHITE, 0, 0x15),
            10 => {
                self.sys.fx.screen(3, 30, 5, 0x40FF_FFFF, 0, 0x15);
                self.sys.fx.screen(7, 4, 120, 190, 60, 0x15);
            }
            15 => self.sys.fx.screen(7, 16, 20, 20, 20, 0x15),
            2 => self.bounce(t),
            3 | 6 | 20 => self.spell_mask_fx(n),
            5 => {
                self.sys.fx.free(crate::effect::FIXED_BASE + 9);
                self.sys.fx.free(crate::effect::FIXED_BASE + 10);
                self.m.g.g_4e4b60 = 2;
            }
            8 => self.familiar_ring(t),
            18 if arg != 0 => self.anm.speed = 1.0 / arg as f32,
            // Native 19: the spell-practice card id (i16 0x164D0B8, sign-extended), not the running Spell's id.
            19 => t.ctx.set_int(V_INT0, i32::from(self.m.g.card)),
            // Native 24: stats +0x1C, spell cards captured in this game (11 §5).
            24 => {
                let n = crate::pl!(self.sys).stats.spells;
                t.ctx.set_int(V_INT0, n);
            }
            // Freeze byte 0x160F534 := low byte of arg; stage VMs get interrupt 2, or 1 when that byte is 0.
            26 => {
                let b = i32::from(arg.to_le_bytes()[0]);
                *self.sys.timer_pause = b;
                self.stage_interrupt(if b != 0 { 2 } else { 1 });
            }
            27 => self.spawn_from_marked(t),
            12 | 14 => self.toggle_ghost(t, n == 14),
            // Slow-down 0x4251B0: game speed 1/arg first, interrupt 2, then live bullets' velocities ×
            // the new speed, +0x218 := sprite, sprites 0x60–0x6F → 0x6F (VM0 only).
            28 if arg != 0 => {
                self.anm.speed = 1.0 / arg as f32;
                self.stage_interrupt(2);
                let k = self.anm.speed;
                crate::bm!(self.sys).slow_down(self.anm, k);
            }
            // Undo 28 0x425290: velocities × 1/speed, sprites 0x60–0x6F := +0x218; Supervisor flag 0x20
            // when 1/arg < 1; then speed 1 and interrupt 1.
            29 => {
                let f = 1.0 / self.anm.speed;
                crate::bm!(self.sys).slow_down_restore(self.anm, f);
                if 1.0 / (arg as f32) < 1.0 {
                    self.anm.slowdown_restore = true;
                }
                self.anm.speed = 1.0;
                self.stage_interrupt(1);
            }
            30 => *self.sys.load_frames = arg,
            // Type 3 (bomb) while the player is bombing (player +0xFDC = 0x17D6ED4), else 5 (1up).
            31 => {
                let kind = if crate::pl!(self.sys).bombing { 3 } else { 5 };
                self.item(self.e.pos, kind);
            }
            22 => {
                // 07 §11.1 bit 10: Spell_ShowNameOnly passes face −1 (sprite 1): no portrait, no cut-in.
                self.sys.gui.push(crate::gui::Req::BossShow {
                    face: -1,
                    name: crate::builtin::Builtin::Resurrection.txt(),
                    card: -1,
                    name_only: true,
                });
            }
            23 => self.sys.gui.push(crate::gui::Req::BossHide),
            // Bullet warp.
            4 => self.warp(&Warp::native4()),
            7 => self.warp(&Warp::native7()),
            21 => self.warp(&Warp::native21()),
            // Rotated 590-long player-hit boxes, heights big / small; native 11 starts at pos, not final pos.
            9 => self.laser_box(t, self.e.final_pos, 160.0, 128.0),
            11 => self.laser_box(t, self.e.pos, 240.0, 192.0),
            25 => self.laser_box(t, self.e.final_pos, 288.0, 224.0),
            // Stage tint request 0xFFC03030 (06 §10.2 step 3).
            // Only Stage_RequestTint; +0x646C is not written.
            13 => self.anm.request_tint(0xFFC0_3030),
            16 => self.mark_children(t),
            _ => self.m.g.misc_calls += 1,
        }
    }

    // Bounce: vars 10022 / 10023 are the gravity step and the fall-speed cap.
    // EclNative_2: no direction tests, so an enemy still past an edge flips back next
    // frame; the angle is recomputed when any branch ran, the gravity add included.
    fn bounce(&mut self, t: &Thread) {
        let e = &mut *self.e;
        let mut ran = false;
        if e.pos[0] <= 0.0 || e.pos[0] >= 384.0 {
            e.vel[0] = -e.vel[0];
            ran = true;
        }
        if e.vel[1] < t.ctx.flt(V_FLT0 + 7) {
            e.vel[1] += t.ctx.flt(V_FLT0 + 6);
            ran = true;
        }
        if e.pos[1] < -64.0 {
            e.vel[1] = -e.vel[1];
            ran = true;
        } else if e.pos[1] >= 480.0 {
            e.flags &= !A_KEEP_OFFSCREEN;
        }
        if ran {
            e.angle = fmath::atan2_f(e.vel[1], e.vel[0]);
        }
    }

    // Natives 3 / 6 / 20 (07 §12.1): two effects (0x38 / 0x3A / 0x41) with an extra script and the stage
    // callback 0x4235A0.
    fn spell_mask_fx(&mut self, n: i32) {
        if let Some(st) = self.sys.stage.as_deref_mut() {
            st.spell_mask = true;
        }
        let (kind, extra) = match n {
            3 => (0x38, 0x61),
            6 => (0x3A, 0x65),
            _ => (0x41, 99),
        };
        let pos = self.e.final_pos;
        self.sys
            .fx
            .spawn_fixed(self.anm, self.sys.rng, kind, pos, 9, 0xFFFF_FFFF);
        let i = self
            .sys
            .fx
            .spawn_fixed(self.anm, self.sys.rng, kind, pos, 10, 0xFFFF_FFFF);
        let anm = &mut *self.anm;
        self.sys.fx.get(i, |e| {
            anm.set_script(self.sys.rng, &mut e.vm, crate::effect::SCRIPT_SLOT, extra);
        });
    }

    // Native 27: bullets with flag 0x100000 spawn enemy sub = var 10038 (life 800, item −2,
    // score 10, var 10016 = angle); the flag is cleared.
    fn spawn_from_marked(&mut self, t: &Thread) {
        let sub = t.ctx.int(V_INT1 + 2);
        let marked: Vec<_> = {
            let b = crate::bm!(self.sys);
            b.bullets[..MAX_BULLETS]
                .iter_mut()
                .filter(|x| !x.free() && x.flags & BULLET_MARKED != 0)
                .map(|x| {
                    x.flags &= !BULLET_MARKED;
                    (x.pos, x.angle)
                })
                .collect()
        };
        for (p, a) in marked {
            if let Some(ci) = self.m.spawn_prepare(sub, [p[0], p[1], 0.0], 800, false) {
                self.m.enemies[ci].main.ctx.set_flt(V_FLT0, a);
                self.m
                    .spawn_finish(self.anm, ci, 800, -2, 10, false, &mut self.sys);
            }
        }
    }

    // Natives 12/14 (08 §7): mask = var 10000, angle/speed = vars 10016/10017; 12 also toggles flags B
    // bit 7 on the chain by var 10001 and interrupts the spell-background VMs.
    fn toggle_ghost(&mut self, t: &Thread, n14: bool) {
        let (mask, a, s) = (
            t.ctx.int(V_INT0).cast_unsigned(),
            t.ctx.flt(V_FLT0),
            t.ctx.flt(V_FLT0 + 1),
        );
        crate::bm!(self.sys).toggle_ghost(self.anm, mask, a, s, n14);
        if n14 {
            return;
        }
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
        if let Some(st) = self.sys.stage.as_deref_mut() {
            st.spell_interrupt(if pause { 2 } else { 1 });
        }
    }

    // Natives 26 / 28 / 29 "stage VMs interrupt k": g_Stage +0xB38 / +0xDDC, the spell-background pair of native 12.
    fn stage_interrupt(&mut self, k: u16) {
        if let Some(st) = self.sys.stage.as_deref_mut() {
            st.spell_interrupt(k);
        }
    }

    fn warp(&mut self, w: &Warp) {
        crate::bm!(self.sys).warp(w);
    }

    // Natives 9 / 11 / 25 0x424730 / 0x424820 / 0x424910: Player_LaserTest from origin
    // o = p − (float vars 10016, 10017) along main VM rotation z; small box with graze when the e+0x2E14 timer
    // changed to a multiple of 12, then the big box without graze on every call.
    fn laser_box(&mut self, t: &Thread, p: [f32; 3], big: f32, small: f32) {
        let o = [p[0] - t.ctx.flt(V_FLT0), p[1] - t.ctx.flt(V_FLT0 + 1)];
        let a = self.e.vm.rotation[2];
        let c = [590.0 / 2.0 + o[0], o[1]];
        let bt = &self.e.boss_timer;
        let graze = bt.cur != bt.prev && bt.cur % 12 == 0;
        let anm = &mut *self.anm;
        let sys = &mut self.sys;
        let orbs = sys.with_player(|pl, sys| {
            let env = pl.env;
            let mut orbs = 0;
            if graze {
                orbs = pl
                    .laser_test(c, [590.0, small], o, a, true, &env, anm, sys)
                    .1;
            }
            pl.laser_test(c, [590.0, big], o, a, false, &env, anm, sys);
            orbs
        });
        let b = crate::bm!(self.sys);
        let pl = crate::pl!(self.sys);
        if std::mem::take(&mut pl.drop_attracted) {
            b.items.drop_attracted();
        }
        // Player_OnGraze drops its time orbs (item type 10) at the point it was given: the player pos.
        for _ in 0..orbs {
            b.items.spawn(self.anm, self.sys.rng, pl, pl.pos, 10, 0);
        }
    }

    // Native 16 `EclNative_16` 0x424E50: per flag-0x100000 bullet in slot order, a member of the
    // caller's chain with int var 10038 == 0 and pos within 64 px (3D, strict) gets 10038 := 60 and the caller's
    // int var 10007 (bullet colour of ecldata6 sub77).
    fn mark_children(&mut self, t: &Thread) {
        let col = t.ctx.int(V_INT0 + 7);
        let near: Vec<[f32; 3]> = crate::bm!(self.sys).bullets[..MAX_BULLETS]
            .iter()
            .filter(|b| !b.free() && b.flags & BULLET_MARKED != 0)
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

    // `EclNative_8` 0x4244F0 familiar ring: members are the parent's +8 chain (caller included, no alive test) with the
    // caller's int var 10038; each gets 10037 := its index. Caller's 10005 / 10006 / 10007: changed, count, calls.
    // Non-leaders turn their orbit angle 2 % toward leader + idx·2π/k (+ leader orbit velocity if the 10007s differ).
    fn familiar_ring(&mut self, t: &mut Thread) {
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
            if i.idx() == self.idx {
                t.ctx.set_int(V_INT1 + 1, k);
                leader.get_or_insert(i);
                k += 1;
                n = self.e.chain_next;
                continue;
            }
            // e+0x2CA0 outside its own Ecl_Run is the main context (07 §4.3).
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
        // A caller outside the ring keeps its old 10037; k = 0 is UB in the original (unreachable), nothing here.
        let Some(l) = leader.filter(|_| idx != 0) else {
            return;
        };
        let (l_angle, l_av, l_calls) = if l.idx() == self.idx {
            (self.e.orbit_angle, self.e.orbit_av, calls)
        } else {
            let e = &self.m.enemies[l];
            (e.orbit_angle, e.orbit_av, e.main.ctx.int(V_INT0 + 7))
        };
        let mut target = wrap_add(l_angle, idx as f32 * TWO_PI / k as f32);
        if l_calls != calls {
            target = wrap_add(target, l_av);
        }
        let mut d = target - wrap_add(self.e.orbit_angle, self.e.orbit_av);
        if d.abs() > PI {
            d += if d > 0.0 { -TWO_PI } else { TWO_PI };
        }
        self.e.orbit_angle = wrap_add(self.e.orbit_angle, d * 0.02);
    }
}
