# IMPL52 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. 07 §7 op 176: where inside `EnemyMgr_Calc` is the GameFlags-bit-13 `Player_DamageAgainst` box called — before the
   timelines, between the timelines and the slot loop, or after the slot loop? It marks every player shot as hit
   (state 2), so the position decides whether the enemies' own step-6 damage tests still see those shots in that
   frame. The port calls it after the slot loop (`enemy.rs` `last_spell_box`).
2. 07 §10.6 trail strip (flag 8, unreachable with retail data): when an interior entry is skipped (flag 2, k −= 2),
   does texture coordinate A still step down for the skipped entry (step computed from the pre-skip k), or only per
   emitted entry? The port steps per walked entry.
