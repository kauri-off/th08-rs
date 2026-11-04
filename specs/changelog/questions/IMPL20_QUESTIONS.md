# IMPL20 questions

Answered in [answers_impl20](../answers_impl20.md).

Tools: `session specs/data/sessions/fullrun_noclip [--to N]` (`TH08_SESSION_DUMP=a-b` now also prints enemy threads,
`TH08_SESSION_RNG=<row>`). Row = menu frame (trace column 26).

## Q1. Var 10098: what are a, b, c, d? (blocks every boss Last Spell)
- 07 §6: "(int only) 2 if a+b+c < d else 0 (game counters; later phase)". The port reads 0 (SPEC-GAP).
- ecldata1–5 each test it once (`op50 $10098, 2, …` at the start of the Last Spell sub, e.g. ecldata1 sub 37:
  ≠ 2 → `delete`). In fullrun_noclip the original ran the Last Spell after the stage 1, 3, 4B and 5 bosses
  (boss slot empty for 1 frame, then a new boss + spell: rows 9407/9408/9567, 41814/41815/41974, 65347/65348/65506,
  87319/87320/87479) but **not** after the stage 2 boss (25486). The player never bombed and never lost a life
  (noclip). Which counters (misses, bombs, spells failed/timed out, …), when are they reset, and what is d?

## Q2. Boss death in mode 1/2 does not unregister the boss (07 §10.3 table)
- 07 §10.3: "0–2 | if boss: HUD off and boss unregistered". fullrun_noclip row 3862: the stage 1 midboss
  (ecldata1 sub 16, death mode 1, death sub 18) dies; the recorded boss column stays 1 and timeline spawns stay
  blocked until its death sub runs `op127 −1` 90 frames later (row 3953; next spawn 3954). The port now leaves the
  boss slot and the HUD byte alone at death (mode 0 still unregisters through the free). Please confirm, and say
  what the "if boss" branch actually clears (only the bullet-to-bonus / kill-others part?).

## Q3. What clears the death latch (flags B bit 3)?
- After a mode-2 death (stage 1 boss, sub 38 → death sub 33) the boss must be able to die again (spell 2 ends by
  death at row 9407). The port now clears the latch in op 131 (`set_life`); this reproduces the boss phase timing
  of every stage in the run. Clearing it in op 129 instead breaks stage 3 (a 10 000-frame phase); clearing it on
  the death-sub jump loops forever (death every frame). Which instruction/function really clears it?

## Q4. Enemy screen check box (IMPL19 Q2)
- Using the main VM sprite size (like bullets, 08 §4.3) instead of the hitbox fixes demo 0 frame 1720, demo 1
  frame 1808 and the stage 1 midboss leaving the top (row 3939: hitbox frees it 14 frames before its `op1`).
  Sync: demo 0 1719 → 1940, demo 1 1807 → 3869. Please confirm the box (and whether the VM scale applies).

## Q5. Single chain-child kill: time orb not spawned in the original (row 6264)
- A stage 1 boss familiar (sub 32 child, life 120) dies from shots. Port: `Enemy_KillChildren` single-kill path →
  `ItemMgr_Spawn(pos, 7, 1)` (4 draws), 36 draws in the frame; the original makes 32 (the shot effects 16 + death
  effects 16 match). Is the single-kill orb skipped in some case (item cursor slot busy, gauge lock, killed by a
  shot vs. contact), or is the orb spawned without the forced-mode-3 draws?

## Notes (rules picked, no answer needed unless wrong)
- Op 36 target: all 37 uses store the target as a float (10016.0, 10017.0, 10042.0, 10043.0, 10078.0); the port
  read the raw bits as an int, so the interpolation never wrote anything. Fixed (familiar paths in the stage 1 boss
  fight now match the bullet counts). 07 §7 says "raw var"; worth stating "float-encoded var id".
- Gauge per-frame update and the player's dialogue checks read the dialogue state live (the dialogue calc,
  prio 12, runs after the enemies; the port used a snapshot taken during the enemy calc, one frame late).
- `wait_boss` (timeline op 10) waits while the boss slot is set and its alive bit is set (life ≤ 0 no longer
  counts as gone).
- Freeze byte 0x160F534 is a global: `Player_Die` clears it, bullets read it per bullet.
