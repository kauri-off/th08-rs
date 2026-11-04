# IMPL22 questions

Answered in [answers_re_q24](../answers_re_q24.md).

Tools: `session <dir> [--to N]` (`TH08_SESSION_DUMP=a-b`, `TH08_SESSION_RNG=<row>`, `TH08_SESSION_PB=1` for player
bullets), `demosync <n> [a-b] [rng-frame]`. Row = menu frame (trace column 26).

## Q1. Op 36 (interpolation): when are a–d read?
- The spec says the four operands are re-read each frame. In fullrun stage 1, the familiars' y slot runs op 27/op 9, and
  these overwrite vars 10018/10019, which the x slot uses. A per-frame re-read gives wrong paths and the run stops after 5
  stages.
- The port evaluates a–d once, at op 36 time (SPEC-GAP). The recorded positions then match H(t) exactly. Please confirm.

## Q2. Capture payout: time and max point
- The port stores the payout at Spell_End and pays it 130 frames later (or at the next card start): AddScore(bonus),
  max_point += time × 10, and the SpellBonus popup.
- The traces show no stats.time jump at the payout, so AddTime is not applied (SPEC-GAP). What does the payout do with
  the card's time reward, and is the max-point change right?

## Q3. Threshold switch: who is killed?
- The port kills all alive non-boss enemies (life 0 + death sub) but skips the switching enemy itself (SPEC-GAP). Is the
  enemy that switches excluded by its boss flag, or does the loop really skip it?

## Q4. Timeout without a card
- check_timeout calls cancel_all(4) whenever the fixed-bonus flag is clear, even when no spell is active. Is the cancel
  done only for an active card?

## Q5. HUD bar after a switch
- The port sets life and hud_life to the threshold value; max life is left unchanged. Should the boss bar's max change too?

## Q6. Chained child contact flag
- The port clears A_CONTACT on a chained child after its first ECL run (in spawn_finish), not before it. This is needed
  for demo sync. Please confirm the order.

## Q7. fullrun_noclip row 6266
- The player bullet pb22 (kind 1, homing) hits e4 one frame earlier than the original. The homing bullet's position or
  hit test is off by one frame (see [answers_impl20](../answers_impl20.md) Q5). Dump: `TH08_SESSION_DUMP=6260-6268 TH08_SESSION_PB=1`.

## Q8. golden row 8056, demo 2 frame 2938
- golden: first RNG difference at row 8056 (stage 1 boss area). demo 2: first difference at 2938. The causes are not
  found yet; RNG call-site logs of the original at these frames would help.

## Q9. Menus
- VM 141: its catch-all should reset it to (640, 320) (Q3), but the port's interrupt path misses a case. Without the
  position hack, mopup1 drops to 83.31 %. Which interrupt reaches VM 141 on screen 16 → 17 → 16?
- Shot change on screen 16: is a range interrupt sent (like 18/27 on entry), or only the char-VM enable path
  (`char_vms_list`)? A range interrupt made mopup1 worse.
- Fading description (Q7): if it is drawn on every screen, all three menu logs gain extra quads. Which interrupt fades
  the current description VM on screens reached from the top menu other than Spell Practice (difficulty, replay,
  options)?
