# IMPL121 questions

Answered in [answers_re_q55](../answers_re_q55.md).

1. **Op 109 `fire_stored` (07 §7) filters and rank.** 07 §7 only says "fire the descriptor as currently stored,
   origin = final pos + shot offset". Does op 109 go through `Ecl_FireBullets`' filters (mode flags 0x8000/0x10000 —
   against which bit, the enemy's flag A bit 11 or the player's mode — and the no-fire radius) and the §9.2 rank
   adjustment, or straight to `BulletMgr_SpawnPattern`? The port keeps its pre-[RE-Q53](../answers_re_q53.md) behaviour (filters on the stored
   flags with the player's mode, no-fire radius, rank add) — `eclvm.rs` `fire_stored`, SPEC-GAP.
