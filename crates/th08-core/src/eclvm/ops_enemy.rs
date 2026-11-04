//! ECL enemy-state ops: ANM scripts, flags, child spawns, items, effects, HUD requests (07 §7, §9.3, §10.5).

use super::{Exec, add3, lo8, lo16};
use crate::anmvm::{self, AnmVm};
use crate::enemy::{
    A_CANCEL_BONUS, A_CHAIN_KILLED, A_CONTACT, A_FOLLOW, A_HIDDEN, A_KEEP_OFFSCREEN, A_LOSES_LIFE,
    A_MODE_REACT, A_NO_PUSH, A_SHOT_HIT, A_YOUKAI, B_CANNOT_DIE, B_STAGE_ANM, MAX_ENEMIES, Thread,
};
use th08_formats::ecl::{Arg, EnemyOp, SpawnKind};

// Effect VM colour2 flag (bit 17) := `on` (07 §7 ops 80/81, §10.5 aura).
fn set_color2(vm: &mut AnmVm, on: bool) {
    vm.flags = (vm.flags & !anmvm::F_COLOR2) | if on { anmvm::F_COLOR2 } else { 0 };
}

impl Exec<'_> {
    pub(super) fn op_enemy(&mut self, t: &mut Thread, op: EnemyOp) {
        match op {
            EnemyOp::Anm { script, stg } => {
                self.e.set_flag_b(B_STAGE_ANM, stg);
                let s = self.ai(t, script);
                self.start_main(s);
            }
            EnemyOp::AnmDir { base, stg } => {
                self.e.set_flag_b(B_STAGE_ANM, stg);
                let base = self.ai(t, base);
                self.e.dir_scripts = [0, 1, 2, 3, 4, 5].map(|k| base + k);
                // Stored through 0x421DE0, which also sets the current direction e+0x332E := -1 (07 §9.3).
                self.e.dir = -1;
            }
            EnemyOp::AnmDir6 { scripts, stg } => {
                self.e.set_flag_b(B_STAGE_ANM, stg);
                for (k, a) in scripts.into_iter().enumerate() {
                    self.e.dir_scripts[k] = self.ai(t, a);
                }
                self.e.dir = -1;
            }
            EnemyOp::SubAnm { vm, script, stg } => {
                self.e.set_flag_b(B_STAGE_ANM, stg);
                let (k, s) = (self.ai(t, vm), self.ai(t, script));
                // VM index clamped to 0..=1.
                self.start_sub(usize::from(k > 0), s);
            }
            EnemyOp::AnmReplay => {
                let s = self.e.dir_scripts[5];
                self.start_main(s);
            }
            EnemyOp::FlagsSet(m) => self.flags_set(t, m),
            EnemyOp::FlagsOff(m) => self.flags_on_off(t, m, false),
            EnemyOp::FlagsOn(m) => self.flags_on_off(t, m, true),
            EnemyOp::FlagB1(b) => {
                let b = self.ai(t, b);
                self.e.set_flag_b(1 << 1, b & 1 != 0);
            }
            EnemyOp::Spawn { .. } => self.spawn_child(t, op),
            EnemyOp::Sfx(id) => {
                let id = self.ai(t, id);
                self.sys.sound.play_at(id, self.e.final_pos[0]);
            }
            EnemyOp::Effect13 { axis, radius } => self.effect13(axis, radius),
            EnemyOp::SetBytes(w) => {
                let b = w.to_le_bytes();
                self.e.death_bytes = [b[0], b[1], b[2]];
            }
            EnemyOp::Effect { .. } | EnemyOp::EffectDir { .. } | EnemyOp::Aura(_) => {
                self.effect(t, op);
            }
            EnemyOp::Item(k) => {
                let k = self.ai(t, k);
                self.item(self.e.pos, k);
            }
            EnemyOp::ItemBurst(n) => {
                let n = self.ai(t, n);
                self.item_burst(n, false);
            }
            EnemyOp::ItemBurst1(n) => {
                let n = self.ai(t, n);
                self.item_burst(n, true);
            }
            EnemyOp::SetItem(n) => self.e.item = self.ai(t, n),
            EnemyOp::ExtraItems { point, power } => {
                self.e.extra_point = self.ai(t, point);
                self.e.extra_power = self.ai(t, power);
            }
            EnemyOp::FlagA25(w) => self.e.set_flag(1 << 25, w & 0xFF != 0),
            EnemyOp::NoStack(w) => self.e.set_flag(A_NO_PUSH, w & 0xFF != 0),
            EnemyOp::FlagA7(w) => {
                self.e.set_flag(A_CANCEL_BONUS, w & 0xFF != 0);
                self.e.layer = 2;
            }
            _ => self.op_enemy_misc(t, op),
        }
    }

    // Ops 147–183 that write one field or post one request (07 §7).
    fn op_enemy_misc(&mut self, t: &Thread, op: EnemyOp) {
        match op {
            // Stage section request (06 §4.1).
            EnemyOp::StdSection(n) => {
                let n = self.ai(t, n);
                if let Some(s) = self.sys.stage.as_deref_mut() {
                    s.section_request = n;
                }
            }
            // HUD 0x423130(n); the 0x164D30C += 1800 happens in the HUD request handler (07 §7 op 148).
            EnemyOp::Hud148(n) => {
                self.m.g.hud_bars = self.ai(t, n);
                self.m.g.hud_requests.push(148);
            }
            EnemyOp::AnmInterrupt(n) => {
                let n = self.ai(t, n);
                self.e.vm.pending_interrupt = lo16(n);
            }
            EnemyOp::SubAnmInterrupt { vm, n } => {
                if let Some(vm) = self.e.sub_vms.get_mut(vm as usize) {
                    vm.pending_interrupt = n as u16;
                }
            }
            // u8 flags and i16 n / a / b fields (07 §4.3 e+0x534C–0x5352).
            EnemyOp::Trail { flags, n, a, b } => {
                self.e.trail = [
                    (flags & 0xFF).cast_signed(),
                    i32::from(self.ai(t, n) as i16),
                    i32::from(self.ai(t, a) as i16),
                    i32::from(self.ai(t, b) as i16),
                ];
            }
            EnemyOp::DrawLayer(n) => self.e.layer = lo8(self.ai(t, n)),
            EnemyOp::DmgTimer(n) => {
                let n = self.ai(t, n);
                self.e.dmg_timer.set(n);
            }
            EnemyOp::GF54cec(n) => self.m.g.g_f54cec = self.ai(t, n),
            EnemyOp::AnmRotZ(a) => {
                self.e.vm.rotation[2] = self.af(t, a);
                self.e.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            EnemyOp::FlagA30(b) => {
                let v = self.ai(t, b) != 0;
                self.e.set_flag(1 << 30, v);
            }
            EnemyOp::FlagB8(b) => {
                let v = self.ai(t, b) != 0;
                self.e.set_flag_b(1 << 8, v);
            }
            EnemyOp::FlagA31(b) => {
                let v = self.ai(t, b) != 0;
                self.e.set_flag(1 << 31, v);
            }
            EnemyOp::SpawnDisable(n) => self.m.g.spawn_disabled = self.ai(t, n),
            EnemyOp::SetHudLife(n) => self.e.hud_life = self.ai(t, n),
            EnemyOp::NightClockShow => self.m.g.hud_requests.push(179),
            EnemyOp::NightClockHide => self.m.g.hud_requests.push(180),
            // Only below 12: at 12 nothing at all, not even the sfx 0x2D (07 §7).
            EnemyOp::Extend181 => {
                let p = crate::pl!(self.sys);
                if p.stats.clock < 12 {
                    self.sys.sound.play(crate::sound::OK_CLOCK);
                    p.stats.clock += 1;
                    self.m.g.hud_requests.push(181);
                }
            }
            _ => {}
        }
    }

    // Op 79: absolute flag set from the mask bits (07 §7).
    fn flags_set(&mut self, t: &Thread, m: Arg) {
        let m = self.ai(t, m);
        let e = &mut *self.e;
        e.set_flag(A_SHOT_HIT, m & 1 == 0);
        e.set_flag(A_CONTACT, m & 2 == 0);
        e.set_flag(A_LOSES_LIFE, m & 4 == 0);
        e.set_flag(A_HIDDEN, m & 8 != 0);
        e.set_flag(A_KEEP_OFFSCREEN, m & 16 != 0);
        e.set_flag_b(B_CANNOT_DIE, m & 32 != 0);
    }

    // Ops 80 / 81: only the flags selected by the mask (07 §7).
    fn flags_on_off(&mut self, t: &Thread, m: Arg, on: bool) {
        let m = self.ai(t, m);
        let e = &mut *self.e;
        if m & 1 != 0 {
            e.set_flag(A_SHOT_HIT, on);
        }
        if m & 2 != 0 {
            e.set_flag(A_CONTACT, on);
            // The aura VM's colour2 flag (bit 17) follows contact damage (07 §7 ops 80/81).
            self.sys.fx.get(e.fx_aura, |x| set_color2(&mut x.vm, on));
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

    // Op 128: effect 13 (0xFF6060D0) kept in e+0x5360[] and updated every frame (follow, grow to d, spin).
    // 07 §5 op 128, 13 §2.6b: a, b, c, d are copied raw (mask ignored, d = f32 bits into e+0x53C4) and
    // the effect spawns at the base pos e+0x2D34, not the final pos.
    fn effect13(&mut self, axis: [f32; 3], radius: f32) {
        self.e.effects += 1;
        self.e.orbit_target = radius;
        let i = self
            .sys
            .fx
            .spawn(self.anm, self.sys.rng, 13, self.e.pos, 1, 0xFF60_60D0);
        if i != crate::effect::NONE {
            self.sys.fx.get(i, |x| x.dir = axis);
            self.e.fx_attached.push(i);
        }
    }

    // Ops 139 / 140 / 174 (07 §7).
    fn effect(&mut self, t: &Thread, op: EnemyOp) {
        self.e.effects += 1;
        match op {
            EnemyOp::Effect { id, n, colour } => {
                let (id, n, c) = (self.ai(t, id), self.ai(t, n), self.ai(t, colour));
                self.sys.fx.spawn(
                    self.anm,
                    self.sys.rng,
                    lo8(id),
                    self.e.final_pos,
                    n,
                    c.cast_unsigned(),
                );
            }
            EnemyOp::EffectDir {
                id,
                n,
                colour,
                param,
            } => {
                let (id, n, c) = (self.ai(t, id), self.ai(t, n), self.ai(t, colour));
                let param = param.map(|a| self.af(t, a));
                self.sys.fx.spawn_param(
                    self.anm,
                    self.sys.rng,
                    lo8(id),
                    self.e.final_pos,
                    param,
                    n,
                    c.cast_unsigned(),
                );
            }
            EnemyOp::Aura(n) => {
                let n = self.ai(t, n);
                self.sys.fx.free(self.e.fx_aura);
                self.e.fx_aura = self.sys.fx.spawn_high(
                    self.anm,
                    self.sys.rng,
                    lo8(0x20 + n),
                    self.e.final_pos,
                    0xFFFF_FFFF,
                );
                // Interrupt (player mode ≠ 0) + 1; odd slot negates VM +0x14; colour2 untouched (07 §7 op 174).
                let interrupt = u16::from(self.m.g.player_mode != 0) + 1;
                let odd = self.e.slot & 1 != 0;
                self.sys.fx.get(self.e.fx_aura, |x| {
                    x.vm.pending_interrupt = interrupt;
                    if odd {
                        x.vm.angular_vel[2] = -x.vm.angular_vel[2];
                    }
                });
            }
            _ => {}
        }
    }

    // --- ANM ---
    // Main VM script from enemy.anm or stgNenm.anm by flags B bit 2 (07 §7 ops 54/58/62, §9.3 direction scripts);
    // started the AnmFile_SetScript way, so VM pos and pos offset are kept.
    pub(super) fn start_main(&mut self, script: i32) {
        let slot = self.e.anm_slot();
        let Ok(script) = usize::try_from(script) else {
            self.e.vm = AnmVm::default();
            return;
        };
        self.anm
            .file_set_script(self.sys.rng, &mut self.e.vm, slot, script);
    }

    // Ops 57/61: script < 0 disables the sub VM; else AnmFile_SetScript (pos / pos offset kept, 07 §7).
    fn start_sub(&mut self, k: usize, script: i32) {
        let slot = self.e.anm_slot();
        let Some(vm) = self.e.sub_vms.get_mut(k) else {
            return;
        };
        match usize::try_from(script) {
            Ok(script) => self.anm.file_set_script(self.sys.rng, vm, slot, script),
            Err(_) => *vm = AnmVm::default(),
        }
    }

    // --- spawning (ops 90–94, 07 §4.1 / §10.5) ---
    fn spawn_child(&mut self, t: &Thread, op: EnemyOp) {
        let EnemyOp::Spawn { kind, .. } = op else {
            return;
        };
        if kind.chained() {
            // 07 §10.5: fails (manager flag) if the parent's life < 1 or it is being chain-killed, tested before
            // any argument is read; SE 0x24 at the spawner pos plays after the spawn block either way.
            if self.e.life < 1 || self.e.flags & A_CHAIN_KILLED != 0 {
                self.m.full = true;
            } else {
                self.spawn_block(t, op);
            }
            self.sys.sound.play_at(crate::sound::OPTION, self.e.pos[0]);
        } else if self.e.life > 0 {
            // Ops 93/94 (07 op table): nothing at all while the spawner's life ≤ 0, before any argument read.
            self.spawn_block(t, op);
        }
    }

    fn spawn_block(&mut self, t: &Thread, op: EnemyOp) {
        let EnemyOp::Spawn {
            kind,
            sub,
            x,
            y,
            z,
            life,
            item,
            score,
        } = op
        else {
            return;
        };
        let sub = self.ai(t, sub);
        let (x, y) = (self.af(t, x), self.af(t, y));
        let z = match z {
            Some(z) => self.af(t, z),
            None => 0.0,
        };
        let life = self.ai(t, life);
        let item = self.ai(t, item);
        let score = self.ai(t, score);
        let chained = kind.chained();
        let follow = kind == SpawnKind::ChildFollow;
        // 07 §10.5: ops 90/92 spawn at (x, y, 0) as given; op 92's follow offset is written after the spawn run.
        let pos = match kind {
            SpawnKind::ChildRel => [self.e.final_pos[0] + x, self.e.final_pos[1] + y, 0.0],
            SpawnKind::SpawnRel => [self.e.pos[0] + x, self.e.pos[1] + y, self.e.pos[2] + z],
            SpawnKind::Spawn => [x, y, z],
            SpawnKind::Child | SpawnKind::ChildFollow => [x, y, 0.0],
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
        // 07 §4.1 step 3: the 30 variable dwords of the spawning context.
        // Ops 90–92 never write the mode bits e+0x3330 (07 §10.5): the spawn-time run sees 0, so both human-
        // and youkai-masked time-0 instructions run; Enemy_ReactPlayerMode sets them at the first step 3.
        self.m.enemies[ci].main.ctx.vars = t.ctx.vars;
        let youkai = self.m.g.player_mode != 0;
        if self
            .m
            .spawn_finish(self.anm, ci, life, item, score, true, &mut self.sys)
            .is_none()
            || !chained
        {
            return;
        }
        self.link_child(ci, tail, youkai, follow);
    }

    // 07 §10.5: applied after the spawn-time Ecl_Run, overriding the child's own time-0 ops.
    fn link_child(&mut self, ci: usize, tail: usize, youkai: bool, follow: bool) {
        let c = &mut self.m.enemies[ci];
        c.flags |= A_MODE_REACT;
        c.set_flag(A_YOUKAI, youkai);
        c.layer = if youkai { 0 } else { 2 };
        // Op 92 (07 §10.5): follow offset := parent pos and final pos, after the run, so time 0 saw neither.
        if follow {
            c.pos_offset = self.e.pos;
            c.final_pos = add3(c.pos_offset, c.pos);
        }
        c.flags &= !A_CONTACT;
        // Aura 0x20 only if it has none yet, at pos (op 92: final pos); interrupt 1 human / 2 youkai.
        if c.fx_aura == crate::effect::NONE {
            let at = if follow { c.final_pos } else { c.pos };
            let contact = c.flags & A_CONTACT != 0;
            let odd = c.slot & 1 != 0;
            let fx = self
                .sys
                .fx
                .spawn_high(self.anm, self.sys.rng, 0x20, at, 0xFFFF_FFFF);
            self.sys.fx.get(fx, |x| {
                x.vm.pending_interrupt = if youkai { 2 } else { 1 };
                // VM bit 17 := flags A bit 2 (just cleared); odd slot negates VM +0x14 (z angular velocity).
                set_color2(&mut x.vm, contact);
                if odd {
                    x.vm.angular_vel[2] = -x.vm.angular_vel[2];
                }
            });
            c.fx_aura = fx;
        }
        // Op 92: bit 9 (follow) after the aura, before the link (07 §10.5).
        if follow {
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

    // --- items ---
    pub(super) fn item(&mut self, pos: [f32; 3], kind: i32) {
        let p = crate::pl!(self.sys);
        crate::bm!(self.sys)
            .items
            .spawn(self.anm, self.sys.rng, p, pos, kind, 0);
    }

    // Ops 142 / 168 (07 §7): offsets Rng·128 − 64 on x then y; 142 below full power: first type 2, rest 0.
    fn item_burst(&mut self, n: i32, all_one: bool) {
        let p = self.e.pos;
        let low = crate::pl!(self.sys).stats.power < 128.0;
        for i in 0..n.max(0) {
            let dx = self.sys.rng.next_f32() * 128.0 - 64.0;
            let dy = self.sys.rng.next_f32() * 128.0 - 64.0;
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
}
