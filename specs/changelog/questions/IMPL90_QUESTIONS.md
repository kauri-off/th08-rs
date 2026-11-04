# IMPL90 questions

Answered in [answers_re_q32](../answers_re_q32.md).

1. 07 §10.1 step 2 vs step 7: a held enemy (flags B bit 7, or A bit 30 while bombing) "skips effect follow", but jumps
   to the common tail 0x42DB0B, which now contains the op-128 update `Enemy_UpdateOrbitEffects` 0x42E010 (after the
   boss marker). Does a held enemy's op-128 effects get the 0x42E010 update (i.e. is the call inside the shared tail),
   or is it skipped? The port skips it (step-2 wording). Same question for the boss HUD life bar / marker, which the
   port runs for held enemies.
2. 07 op 178 (`move_rand2`, dur ≥ 1): the 05 §10 trig-site table lists op 67 (`Ecl_SetMode2Polar` 0x422315) but not
   op 178. The port uses the same f32(f32(cos·spd)·dur) form for 178 (07 §8.3 "Both: … mode 2 (0x4222B0)"). Confirm
   op 178 goes through the same helper.
