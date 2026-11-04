# IMPL76 questions

1. 07 §10.1 step 6: the homing-target pick (player +0xE2AA4, 09 §1.1 "filled by `EnemyMgr_Calc`") — where in step 6
   is it made, and is it also skipped when the first `Player_DamageAgainst` call is gated (card active, enemy with a
   parent, player bombing)? The port picks it before that call (inside the flag A bit 6 branch), gated or not.
2. 08 §8.4 / 09 §1.1: the item box +0x3BC..+0x3D0 is written only by `Player_Move`. What does it hold before the first
   `Player_Move` (player init) — zero, pos ∓ half, or untouched? The port starts it at 0 (only read in states 0/3/4,
   after a move has run that frame).
