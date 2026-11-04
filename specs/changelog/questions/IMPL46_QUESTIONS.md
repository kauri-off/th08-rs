# IMPL46 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **Spell flag bit 9 "captured" — the clear (07 §11.1, 12 §4).** 07 §11.1 now says `Spell_End` sets bits 8 and 9 on
   the captured path (port matches). The clear is still not stated: the `Spell_Start` paragraph lists bit 5 := GameFlags
   bit 7, bit 6 := 0, bit 11 := 0, and "Both cleared by `Spell_Start`" (after bits 10/11) is ambiguous. Is bit 9
   cleared by `Spell_Start` (the port does that, `eclvm.rs` `SPEC-GAP`), and does it survive a retry (scene 11)?
   (Repeats IMPL40 Q2, narrowed.)
2. **Bomb cooldown +0xE2A6C on a forced start (09 §1.1).** 09 §1.1 decrements it at the top of `Player_UpdateBomb`,
   skipped on a forced-start call. The port decrements it once per `Player_Calc` at step 9, also on a forced
   (deathbomb) start frame, so a forced start within 16 frames of `Spell_End` leaves the cooldown one lower than the
   original. Equivalent otherwise (only the start test reads it). Is the forced-start skip confirmed as described,
   i.e. a code fix (move the decrement into `update_bomb`, skip it when forced) is wanted? Not changed here: IMPL46 is
   comments only.
3. **Audit §C citation for `bullet.rs:1377`.** The audit cites 08 §3, §6, §8.6; the per-bullet draw (z 0.05, RGB
   white, tag rotation, VM by state) is in 08 §1 (`Bullet_Draw` 0x432F20). The comment cites 08 §1. No question for RE.
