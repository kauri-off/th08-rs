//! ECL boss and spell-card ops: boss slots, life thresholds, timeouts, death, spell cards (07 §7, §10.2, §11).

use super::{Exec, Flow, at_mut};
use crate::anmmgr::AnmManager;
use crate::enemy::{
    A_BOSS, A_DEATH_SHIFT, A_FIXED_BONUS, A_LAST_SPELL, EnemyManager, Spell, Thread,
};
use crate::game::Sys;
use crate::gamemgr::{F_13, F_LAST_CAPTURED, F_LAST_SPELL, F_SPELL_PRACTICE};
use crate::types::EnemyIdx;
use th08_formats::ecl::{Arg, BossOp, Ins, SpellBlock};

// Card bonus cap g_Gui +0x2638 for flag A bit 27 / op 155 (07 §11.1, op 155).
pub(super) const FIXED_BONUS_CAP: i32 = 99_999_990;

// Spell practice (GameFlags bit 14) with a last spell running or captured (bits 7–8) (07 §5 ops 129–134, 11 §2.1).
fn practice_guard(f: u32) -> bool {
    f & F_SPELL_PRACTICE != 0 && f & (F_LAST_SPELL | F_LAST_CAPTURED) != 0
}

impl Exec<'_> {
    pub(super) fn op_boss(&mut self, t: &mut Thread, ins: &Ins, op: BossOp) -> Flow {
        match op {
            BossOp::KillAll => {
                return self.with_self_in_slot(t, |m, anm, sys| {
                    m.kill_all(anm, sys);
                });
            }
            BossOp::Spellcard => self.spell_card(ins),
            BossOp::SpellcardEnd => {
                return self.with_self_in_slot(t, EnemyManager::finish_spell);
            }
            BossOp::Boss(slot) => {
                let slot = self.ai(t, slot);
                self.boss(slot);
            }
            // Spell-practice guard (0x164D0B4 bit 14 and bits 7–8 ≠ 0): 129/130 ignored, 133/134 keep the sub (07 §5).
            BossOp::DeathMode(w) if !practice_guard(*self.sys.flags) => {
                let m = w & 0xFF;
                self.e.flags = (self.e.flags & !(7 << A_DEATH_SHIFT)) | ((m & 7) << A_DEATH_SHIFT);
            }
            BossOp::DeathSub(w) if !practice_guard(*self.sys.flags) => {
                self.e.death_sub = i32::from(w as i16);
            }
            BossOp::DeathMode(_) | BossOp::DeathSub(_) => {}
            // life = max = HUD life; boss slot 0 resets the 8 HUD markers.
            BossOp::SetLife(n) => {
                let n = self.ai(t, n);
                (self.e.life, self.e.max_life, self.e.hud_life) = (n, n, n);
                if self.e.boss_slot == 0 && self.e.flags & A_BOSS != 0 {
                    self.m.g.hud_markers = Default::default();
                }
            }
            BossOp::SetTimer(n) => {
                let n = self.ai(t, n);
                self.e.boss_timer.set(n);
            }
            BossOp::LifeThreshold { k, life, sub } => {
                let (k, life, sub) = (self.ai(t, k), self.ai(t, life), self.ai(t, sub));
                if let Some(k) = usize::try_from(k).ok().filter(|&k| k < 4) {
                    self.e.thresholds[k] = life;
                    if !practice_guard(*self.sys.flags) {
                        self.e.threshold_subs[k] = sub;
                    }
                }
            }
            // Timer 0 here, so op 134 on frame F times out on frame F + timeout (07 §10.2).
            BossOp::Timeout { frames, sub } => {
                self.e.timeout = self.ai(t, frames);
                let sub = self.ai(t, sub);
                if !practice_guard(*self.sys.flags) {
                    self.e.timeout_sub = sub;
                }
                self.e.boss_timer.set(0);
            }
            BossOp::TimeoutSubFromDeath => {
                self.e.timeout_sub = self.e.death_sub;
                self.e.boss_timer.set(0);
            }
            BossOp::FixedBonus(w) => {
                let on = w & 0xFF != 0;
                self.e.set_flag(A_FIXED_BONUS, on);
                self.m.g.fixed_bonus = on;
                // 07 op 155: whatever the byte, 0x4ECCA8 = g_Gui +0x2638 (running card's cap) := 99 999 990.
                if let Some(s) = &mut self.m.g.spell {
                    s.max = FIXED_BONUS_CAP;
                }
            }
            BossOp::HpMarker { k, a, b, c } => self.hp_marker(t, k, [a, b, c]),
            // Not implemented: op 164 spell flag bit 6 / 0x41F040 (0 uses in data).
            // Not implemented: the n == 0 call 0x41F040(a, b, c) (op unused in the data).
            BossOp::SpellMode(n) => {
                self.m.g.spell_flag6 = self.ai(t, n) & 1 != 0;
                self.m.g.misc_calls += 1;
            }
            BossOp::LastSpell => self.last_spell(),
            // Spell_SetBonusFrozen 0x41F0E0: spell flag bit 11 := n & 1 (07 §11.1).
            BossOp::SpellEx(n) => self.m.g.bonus_frozen = self.ai(t, n) & 1 != 0,
        }
        Flow::Next
    }

    // Op 127, boss table 0xF54CC0 (07 §7 op 127): no-fire radius 0; slot 0 turns on the HUD boss display.
    fn boss(&mut self, slot: i32) {
        if slot < 0 {
            self.m.unregister_boss(self.idx);
            self.e.flags &= !A_BOSS;
            // ENEMY marker fade-out (12 §5.10).
            if let Some(k) = usize::try_from(self.e.boss_slot).ok().filter(|&k| k < 4) {
                self.sys.ov.marker_release(k);
            }
            // `Enemy_ReleaseOrbitEffects` 0x42A820 (13 §2.6b, RE-Q56 AL): op-128 effects fade and free.
            for i in self.e.fx_attached.drain(..) {
                self.sys.fx.get(i, |x| x.fade = true);
            }
        } else if let Some(b) = at_mut(&mut self.m.bosses, slot) {
            if slot < 4 {
                self.sys
                    .ov
                    .marker_interrupt(slot.cast_unsigned() as usize, 1);
            }
            *b = Some(EnemyIdx::from_slot(self.idx));
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

    // Op 158: word 1 → start +0x5C, word 2 → end +0x3C, as fractions of this enemy's max life (07 op 158).
    fn hp_marker(&mut self, t: &Thread, k: Arg, abc: [Arg; 3]) {
        let k = self.ai(t, k);
        let max = self.e.max_life as f32;
        let (a, b) = (self.ai(t, abc[0]) as f32, self.ai(t, abc[1]) as f32);
        let c = self.ai(t, abc[2]).cast_unsigned();
        if let Some(s) = at_mut(&mut self.m.g.hud_markers, k) {
            *s = (b / max, a / max, c);
        }
    }

    // Op 176, GameFlags 0x164D0B4: clear 7, 8, 13, set 7; bit 13 by stage / practice card (07 §7 op 176).
    fn last_spell(&mut self) {
        let mut f = *self.sys.flags & !(F_LAST_SPELL | F_LAST_CAPTURED | F_13) | F_LAST_SPELL;
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
        *self.sys.flags = f;
        self.e.flags |= A_LAST_SPELL;
    }

    // Runs a manager-wide op (KillAll, Spell_End) with the running enemy and its running main context back in
    // its slot, so the loop sees the caller in slot order (07 §7 op 95, §10.2: no self test).
    // Quirk kept (07 §7 op 95): Ecl_Run steps a local copy of the ip, so a death-sub `Ecl_SetSub` on the caller's
    // own main context keeps only clock/wait 0 and the sub id; the old sub continues and the death sub never runs.
    // From a thread the main context keeps the death sub and the thread goes on.
    fn with_self_in_slot(
        &mut self,
        t: &mut Thread,
        f: impl FnOnce(&mut EnemyManager, &mut AnmManager, &mut Sys),
    ) -> Flow {
        let in_main = self.e.running_thread.is_none();
        let local_ip = t.ctx.ip;
        if in_main {
            std::mem::swap(t, &mut self.e.main);
        }
        std::mem::swap(self.e, &mut *self.m.enemies[self.idx]);
        f(self.m, self.anm, &mut self.sys);
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

    // Op 122 spell block (07 §11): decoded strings, bonus / max / decay rate, pending payout paid first
    // (07 §11.1), bullets cancelled, spell background, rank ranges reset.
    fn spell_card(&mut self, ins: &Ins) {
        let b = SpellBlock::parse(self.m.ecl.bytes(ins));
        let bonus = b.bonus;
        // Rate R = (bonus − bonus/7) / (timeout/60), unsigned; op 134 precedes op 122 in all data.
        let secs = (self.e.timeout / 60).max(1).cast_unsigned();
        // 07 §11.1: +0xFC := card bonus; cap +0x2638 := card bonus, or 99 999 990 if flag A bit 27 is already set.
        let max = if self.e.flags & A_FIXED_BONUS != 0 {
            FIXED_BONUS_CAP
        } else {
            bonus.cast_signed()
        };
        let spell = Spell {
            face: i32::from(b.face),
            id: i32::from(b.id),
            bonus: bonus.cast_signed(),
            max,
            decay: ((bonus - bonus / 7) / secs).cast_signed(),
            boss: EnemyIdx::from_slot(self.idx),
            // g_Gui +0xC := boss e+0x2E0C, written only by Spell_Start (07 §11.1).
            slot: self.e.slot,
            name: b.name,
            owner: b.owner,
            comment: b.comment,
            duration: self.e.timeout.max(0),
            deaths0: crate::pl!(self.sys).deaths,
            bombs0: crate::pl!(self.sys).bombs_started,
            capture: true,
            timed_out: false,
            bombed: false,
            last: *self.sys.flags & F_LAST_SPELL != 0,
        };
        log::info!(
            "spell card {} ({} bytes name, bonus {}) at enemy frame {}",
            spell.id,
            spell.name.len(),
            spell.bonus,
            self.m.clock.cur
        );
        self.m.pay_capture(false, &mut self.sys);
        self.m.g.fixed_bonus = self.e.flags & A_FIXED_BONUS != 0;
        self.m.g.spell_boss = Some(EnemyIdx::from_slot(self.idx));
        (self.m.g.spell_flag6, self.m.g.bonus_frozen) = (false, false);
        // Bit 9 := 0 is Spell_Start's first statement (07 §11.1).
        self.m.g.spell_captured = false;
        // HUD +0x04 |= 0xF: lives and bombs fields := 3 (12 §5.3).
        self.sys.panel.mark_lives(3);
        self.sys.panel.mark_bombs(3);
        self.sys.gui.push(crate::gui::Req::BossShow {
            face: spell.face,
            name: spell.name.clone(),
            card: spell.id,
            name_only: false,
        });
        self.sys.gui.push(crate::gui::Req::Attempt {
            card: spell.id,
            name: spell.name.clone(),
            owner: spell.owner.clone(),
        });
        self.m.g.spell = Some(spell);
        self.m.g.spell_left = None;
        // Timer +0x108 (remaining) := e+0x3378 (07 §11.1).
        self.m.g.spell_remaining.set(self.e.timeout);
        if let Some(st) = self.sys.stage.as_deref_mut() {
            st.start_spell_bg(self.anm, self.sys.rng);
        }
        // Ring at the boss pos e+0x2D34 (not the final pos); type 52 while GameFlags bits 7–8 ≠ 0 (07 §11.1).
        let special = crate::pl!(self.sys).special(*self.sys.flags);
        self.sys
            .fx
            .spell_ring(self.anm, self.sys.rng, self.e.pos, special);
        self.cancel_all(1);
        self.e.reset_rank_ranges();
    }
}
