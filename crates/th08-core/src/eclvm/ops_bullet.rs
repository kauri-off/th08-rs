//! ECL bullet ops: bullet blocks, auto-fire, descriptor edits, cancels (07 §7 ops 96–113, 152, 161, 162; §9.1–§9.3).

use super::{Exec, add3, at_mut};
use crate::bullet::{BulletDesc, ExCmd};
use crate::enemy::{A_MIRROR, A_STORE_FIRE, A_YOUKAI, Thread};
use th08_formats::ecl::{BulletBlock, BulletOp, Ins};

// Bullet block flag mode filters (07 §9.1 step 1): fired only by a youkai- / human-mode enemy (flag A bit 11).
const BLK_YOUKAI_ONLY: u32 = 0x8000;
const BLK_HUMAN_ONLY: u32 = 0x1_0000;
// Descriptor flag 0x200: play the descriptor's sfx (07 §7 op 113, §9.1 flags).
const DESC_SFX: u32 = 0x200;

// Rank lerps (07 §9.2): rank = var 10041 (8–16), integer division truncates toward 0.
pub(super) fn lerp_i(a: i32, b: i32, rank: i32) -> i32 {
    a + ((b - a) * rank) / 32
}

fn lerp_f(a: f32, b: f32, rank: i32) -> f32 {
    a + (b - a) * rank as f32 / 32.0
}

impl Exec<'_> {
    pub(super) fn op_bullet(&mut self, t: &Thread, ins: &Ins, op: BulletOp) {
        match op {
            BulletOp::Bullets => self.bullet_op(t, ins),
            // interval = n + lerp_i(n/5, −n/5) (07 §7, §9.2); 106 starts the timer at Rng_Range(interval).
            BulletOp::Autofire { n, rand } => {
                let n = self.ai(t, n);
                let iv = n + lerp_i(n / 5, -n / 5, self.sys.rank.rank);
                self.e.autofire_interval = iv;
                let start = if rand { self.sys.rng.range(iv) } else { 0 };
                self.e.autofire_timer.set(start);
            }
            BulletOp::AutofireStore(true) => self.e.flags |= A_STORE_FIRE,
            BulletOp::AutofireStore(false) => self.e.flags &= !A_STORE_FIRE,
            BulletOp::FireStored => self.fire_stored(),
            BulletOp::ShotOffset { x, y } => {
                self.e.shot_offset = [self.af(t, x), self.af(t, y), 0.0];
            }
            // Ex slot k = {e, f, c, d, a, b}; ints stored raw, every argument variable-capable (07 §7).
            BulletOp::BulletEx {
                k,
                a,
                b,
                c,
                d,
                e,
                f,
            } => {
                let k = self.ai(t, k);
                let cmd = ExCmd {
                    cmd: self.ai(t, a).cast_unsigned(),
                    chain: self.ai(t, b),
                    c: self.ai(t, c),
                    d: self.ai(t, d),
                    e: self.af(t, e),
                    f: self.af(t, f),
                };
                if let Some(s) = at_mut(&mut self.e.desc.ex, k) {
                    *s = cmd;
                }
            }
            BulletOp::CancelBullets => self.cancel_all(1),
            BulletOp::CancelAll => self.cancel_all(4),
            // id < 0: descriptor flag 0x200 off; desc+0x204 = x either way.
            BulletOp::BulletSfx { id, x } => {
                let (id, x) = (self.ai(t, id), self.ai(t, x));
                if id < 0 {
                    self.e.desc.flags &= !DESC_SFX;
                } else {
                    self.e.desc.sfx = id;
                    self.e.desc.flags |= DESC_SFX;
                }
                self.e.desc.ex_sfx = x;
            }
            BulletOp::RankParams { speed, counts } => {
                self.e.rank_speed = speed.map(|a| self.af(t, a));
                for (k, a) in counts.into_iter().enumerate() {
                    self.e.rank_counts[k] = self.ai(t, a);
                }
            }
            BulletOp::CancelRadius(r) => {
                let r = self.af(t, r);
                let p = crate::pl!(self.sys);
                crate::bm!(self.sys).cancel_radius(self.anm, self.sys.rng, p, self.e.final_pos, r);
            }
        }
    }

    // Bullet block (07 §9.1): fields over the enemy's current descriptor; mask bits 0–7, flags raw.
    // Read order of 07 §9.1 step 3 (each variable read may draw); colour is read last (step 5).
    fn decode_bullet(&mut self, t: &Thread, b: &BulletBlock) -> BulletDesc {
        let mut d = self.e.desc.clone();
        d.kind = self.vi(t, i32::from(b.kind), b.var(0)) as i16;
        d.count1 = self.vi(t, b.count1, b.var(2)) as i16;
        d.count2 = self.vi(t, b.count2, b.var(3)) as i16;
        d.angle1 = self.vf(t, b.angle1, b.var(6));
        d.speed1 = self.vf(t, b.speed1, b.var(4));
        d.angle2 = self.vf(t, b.angle2, b.var(7));
        d.speed2 = self.vf(t, b.speed2, b.var(5));
        d.aim_mode = b.op - 96;
        d.flags = b.flags;
        d.colour = self.vi(t, i32::from(b.colour), b.var(1)) as i16;
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
    fn fire_bullets(&mut self, t: &Thread, bytes: &[u8]) {
        let b = BulletBlock::parse(bytes);
        // Mode filter on the enemy's flag A bit 11, not the player's mode (07 §9.1 step 1).
        let youkai = self.e.flags & A_YOUKAI != 0;
        if (b.flags & BLK_YOUKAI_ONLY != 0 && !youkai) || (b.flags & BLK_HUMAN_ONLY != 0 && youkai)
        {
            return;
        }
        let p = self.m.g.player;
        let e = &*self.e;
        let dd = (e.final_pos[0] - p[0]).powi(2) + (e.final_pos[1] - p[1]).powi(2);
        if e.nofire_r2 > 0.0 && dd < e.nofire_r2 {
            return;
        }
        self.e.desc = self.decode_bullet(t, &b);
        let e = &mut *self.e;
        e.desc.origin = add3(e.final_pos, e.shot_offset);
        // Rank adjustment (07 §9.2) is written in place: op 109 re-fires the adjusted values (§9.1).
        if self.m.g.spell.is_none() {
            let rank = self.sys.rank.rank;
            let [c1a, c1b, c2a, c2b] = e.rank_counts;
            let [s0, s1] = e.rank_speed;
            let d = &mut e.desc;
            d.count1 = (i32::from(d.count1) + lerp_i(c1a, c1b, rank)).max(1) as i16;
            d.count2 = (i32::from(d.count2) + lerp_i(c2a, c2b, rank)).max(1) as i16;
            let s = lerp_f(s0, s1, rank);
            if d.speed1 != 0.0 {
                d.speed1 = (d.speed1 + s).max(0.3);
            }
            // 07 §9.2: speed2's add is unconditional (the speed1 == 0 branch jumps to it).
            d.speed2 = (d.speed2 + s / 2.0).max(0.3);
        }
        self.spawn_desc();
    }

    // Op 109 fire_stored (07 §7 row 109): origin := base pos e+0x2D34 + shot offset, then spawns the descriptor
    // as left by the last Ecl_FireBullets; no life test, no filters, no rank add.
    fn fire_stored(&mut self) {
        let e = &mut *self.e;
        e.desc.origin = add3(e.pos, e.shot_offset);
        self.spawn_desc();
    }

    // BulletMgr_SpawnPattern on the enemy's descriptor e+0x2E24 (07 §9.3).
    fn spawn_desc(&mut self) {
        let p = self.m.g.player;
        let (anm, desc) = (&mut *self.anm, &self.e.desc);
        self.sys
            .with_bullets(|b, sys| b.spawn_pattern(anm, desc, p, sys));
    }

    pub(super) fn cancel_all(&mut self, mode: i32) {
        self.m.cancel_all(self.anm, mode, &mut self.sys);
    }

    // Enemy_AutoFireAndDirAnim 0x423150 (07 §9.3).
    pub(super) fn autofire_and_dir_anim(&mut self, t: &mut Thread) {
        // The whole function (auto-fire and direction animation) runs only while life > 0.
        if self.e.life <= 0 {
            return;
        }
        let gs = self.anm.speed;
        if self.e.autofire_interval > 0 {
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
}
