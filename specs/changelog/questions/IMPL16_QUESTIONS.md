# impl16 questions for the dirty room

Answered in [answers_impl16](../answers_impl16.md).

Demo sync (`cargo run --release --bin demosync <n> [from-to] [rng-frame]`; frames = gameplay frames of the trace).
Debug dumps in the same tool: `TH08_BDUMP=1` (live enemies + bullets per frame of the range), `TH08_IDUMP=1`
(player + items instead of bullets). Draw counts below are `Rng_Next16` calls. Every first divergence is reached
with RNG state/counter and all other compared fields in sync up to the frame before.

Sync after impl16: demo 0 1128 / demo 1 559 / demo 2 723 / demo 3 825 frames (was 260 / 559 / 449 / 794).

## Rules the port now uses that contradict or go beyond the specs (please confirm or correct)

1. **Shot damage cap (09 §4.5).** "shot damage capped at 50; accumulator += capped value" — the traces need the
   *uncapped* shot sum as the damage (enemy side then caps at 70). Demo 0 from frame 330 (stage 5, shot type 0,
   enemy 0 at (192, 164)): port shot sums 84 / 128 on alternate volleys; the trace's score grows by 140 (d = 70)
   on those frames, by 80 (d = 44) on the others. Port now: damage uncapped, accumulator += min(sum, 50). Is the
   cap only on the accumulator?
2. **Time-orb accumulator.** With a per-enemy accumulator (e+0x2E10) the port misses time orbs on the first hits
   of fresh enemies: demo 3 frame 801 (stage 2, gauge −8099 = human max, fresh enemy slot 9 hit by 26 + 16 → trace
   +4 draws = 1 orb), demo 0 frame 1129 (gauge −8315, fresh enemies hit by 22 and 11 → trace has 2 orbs' worth
   (8 extra draws), port 1). Port now: one accumulator shared by all non-boss enemies, bosses keep their own,
   loop order as in 09 §4.5 → demo 3 syncs to 825, demo 0 still 1 orb short at 1129. Which counter does
   `EnemyMgr_Calc` pass for a normal enemy, and when is it reset?
3. **Demo 3 frame 826** (shared accumulator): enemy 9 killed by a 26-damage hit with accumulator 81 → port spawns
   2 orbs (8 draws), the trace has 1 (total trace 40 = 4 hit spark + 12 drop sparks + 16 death sparks + 4 death
   time orb + 4 one orb). Accumulator history over frames 783–826 is in the `TH08_BDUMP` run; probably answered
   with Q2.
4. **Drop sparks (07 §10.4, [answers_impl14](../answers_impl14.md) §3).** Port now: item ≥ 0 → `Effect_Spawn(e+0x3311 + 4, pos, 3)`;
   item −1 with a table drop → the same effect with count **6** (13 §2.6 "drops (×3 / ×6)"); item −1 without a
   drop / −2 → no sparks. Evidence: demo 2 frame 583 (first item −1 death of the stage, no drop in the trace: 0
   spark draws) and frame 587 (second item −1 death: +24 draws = 6 sparks). Confirm the counts and the condition.
5. **Every-third drop counter start.** Demo 2 (stage 3) needs the *second* item −1 death to drop (counter 2 at
   stage start with "check % 3 == 0, then += 1"). Demos 0/1 have no item −1 deaths before it, so it is not a
   leftover from earlier demos. Port uses start value 2 (SPEC-GAP). What is the counter's initial value / reset
   point and the exact check (pre/post increment)?
6. **Enemy_KillChildren step 5 ([answers_impl10](../answers_impl10.md) §2)** runs for every parent with reward ≠ 0, not only bosses (the port
   had gated it on flag A bit 1). Demo 0 frame 492 (non-boss parent with 13 op-92 children dies): the trace
   needs the 2·13 orb shower (+208 draws). Confirm there is no boss condition.
7. The parent's child counter (+0x3380) is no longer reset after a chain kill (old SPEC-GAP removed; the spec never
   mentions a reset). Confirm.

## Open divergences

8. **Demo 1 frame 560 (stage 4A): off-screen spawn bullets survive in the original.** Enemies e1–e5 (sub 0,
   flags 0x52049 = mirror bit 18 set, spawned at x ≈ 400 moving right to 490 and back) fire 4-bullet patterns
   every 10 frames (sub 2, flags 0x203) and, from t = 30, sub 1 patterns. Port bullet slots (dump, port values):
   - slots 74–77 spawned at frame 549 from (450, 64) (e1, t = 10), angles 2.97 / −0.17 / −0.03 / −0.32,
     off_count 0, active 0; the spawn effect ends on frame 559 at x 447.8 / 451.4 / 451.2 / 451.2 → port frees
     all four by §4.3 that frame;
   - slots 78–81 from (400, 64) (e2, t = 0) end their spawn effect on 560 at x ≈ 397–401 → freed (the answer says
     the original frees 78–81 on 560 as well).
   Trace `bullets`: 559: 15 (port 15), 560–569: 23 (port 19), 570: 41 (29), 580: 57 (30), 590: 82 (40),
   600: 96 (32), 610: 253 (181). The trace never shows the mass frees the port does on 569 (slots 82–89),
   579 (94–111), etc.; it only drops by ~1 per frame. So the original keeps bullets that end their spawn effect
   off the right edge (x 440–505) for a long time, while those ending near x 400 are freed. What keeps them — a
   non-zero off-screen counter +0xDBA at spawn, the on-screen box size used at the state 2 → 1 transition, the
   mirror flag affecting the bullet origin/test, or something else? Slots 74–77 were never used before in the port.
9. **Demo 2 frame 724 (stage 3, shot type 3, full power, gauge 9245, player (245.3, 160.5) moving up, input
   0x10).** The trace has +8 draws, score +14 930 (÷10: 8901 → 10394), gauge −222 (9245 → 9023, then −111 on 725),
   time +2 on that frame; the port has none of it (only the 2 option sparkles, 8 draws each, in both). Two time
   orbs are on screen (state 1 at (261, 134) and (284, −9)), many point items (type 1, state 0) and type-8 items
   above y 230; collection line (sht +0x1C) in the port is 128 for both shts. What happens on that frame —
   which collection / gauge rule is triggered?
