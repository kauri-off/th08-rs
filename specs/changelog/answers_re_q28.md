# RE-Q28 — next sync divergences after the RE-Q27 fixes (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data: `re/work/runs/q28/` (not in git):
- `golden_orig*/`, `fullrun_noclip_orig*/`, `golden_boss*/`: partial replays of the original (`TH08_SCAN`, `TH08_SCANBUL`,
  `TH08_RNGSITE`, `TH08_BOSS` with the new `V` float-var lines); `demo_orig/`: one attract-demo probe run (iscan, escan,
  bscan, rngsite for all 4 demos, command file `demo_cmd.txt`).
- `portfix/`: scratch copy of `src/` (taken at the start of the run) with the RE-Q27 fixes 1–6 re-applied plus the fixes
  A–K below, each marked `// Q28 fix X` (`q28_fix_sites.txt`). `*_port<X>/` = session results after fix X,
  `demo<n>_<X>.txt` = demosync reports. src/ untouched.

## 1. Root causes (in the order they were hit)

| divergence | cause | label | spec § | after the fix (scratch port) |
|---|---|---|---|---|
| golden 8298, fullrun 7746 (−8 draws, 1 bullet instead of 2) | stage 1 boss children (sub 36, chained to the boss by op 91) start thread sub 35, which fires only on frames chosen by var 10096. The port reads 10096 as "entries after this one" (its own SPEC-GAP); the spec (and 0x41FD40 / `Ecl_GetFloatArg` case 0x2770) evaluates a child on its parent: all children read the root's count | CODE BUG (A) | 07 §6 var 10096 (clarified) | golden 8508, fullrun 8166 |
| golden 8508, fullrun 8166 (+8 draws, +1 bullet) | port artefact: while an enemy runs, `EnemyManager::take` leaves a placeholder without chain links in its slot, so a sibling's walk from the root stops at the running enemy (counts 1/2/3 instead of 3/3/3). The original walks live links, the running enemy included | CODE BUG (B) | 07 §6 (sentence added) | golden 10516, fullrun 9346 |
| golden 8137 (boss life, not in the trace; caused 9047 death after A+B) | chained damage to the root (0x42B370): rd = d/2, then under the **root's** damage timer rd/9 (boss) or 0 — the port used d/9; also no forwarding while the player is bombing; floor = max(0, thresholds), applied only when rd ≠ 0 with ≤ | SPEC FIXED + CODE BUG (C) | 07 §10.1 step 6 | golden 10516 (no change in fullrun) |
| golden 10516, fullrun 9346 (−16 / −20 draws) | death effects: after the death-mode switch the sfx, `Effect_Spawn(e+0x3310)` + `Effect_Spawn(e+0x3311+4, 4)` and the time orb run for **every** death mode, mode 3 included (the port: modes 0–2 only); both effects are gated by e+0x3310 ≥ 0 (the port spawned the sparks unconditionally) | SPEC FIXED + CODE BUG (J) | 07 §10.3 | golden 10533, fullrun 9363 |
| golden 10533, fullrun 9363 (+2 draws) | `EffInit_Burst` 0x4270C0 (types 21/26): the random angle is taken only when param.x ≤ −990.0 (double constant 0x4B49A0; the ECL passes −999.0 for "random"), as `FloatRange(2π) − π`; otherwise `Angle_AddNormalize(param.x)`. Spec said "> 0"; sub 50's sweep reaches −π+ε after 16 steps (runtime: boss float var 10016 = 0xC0490FDA at fullrun 9363, no random draw) | SPEC FIXED + CODE BUG (K) | 13 §2 table row 21/26 | golden 10707 (score), fullrun 9408 (GameFlags bit 7) — not chased |
| demo 0 4629 | `ItemMgr_DropAttracted` 0x441530 is missing in the port (its SPEC-GAP "left to ItemMgr_Calc"): the player died at 4547 above the collection line, so point item 428 (state 1) kept vel.x 5.18 through the `ItemMgr_Calc` dying branch (vel.y = −0.7 only) and drifted to x 300 instead of stopping at 174.3; the original collects it at 4629 | CODE BUG (F) (spec already right, 09 §6.1; 08 §8.4 note added) | 09 §6.1, 08 §8.4 | 5246 (score +1, not chased) |
| demo 1 3880 (gauge) | single-kill block of `Enemy_KillChildren` (chained child killed on its own) changes the gauge through `GameMgr_GaugeAdd(−gauge/12, not forced)` → nothing while bombing (the player was bombing); the port subtracts directly | SPEC FIXED + CODE BUG (G) | 07 §10.3a, 08 §8.4, 13 §4 | 4155 |
| demo 2 4036 (+4 draws) | `Player_OnGraze`: the third time orb is nested inside the "bullet and card active" branch (0x44AACE–0x44AB14); spec read as independent, the port spawned 2 orbs for a no-card bullet graze | SPEC FIXED + CODE BUG (H) | 09 §9.2 | 4641 → with I: 4660 (not chased) |
| demo 3 6665 (+104 draws), demo 2 4641 | GameFlags bit 10 (spell-card death freeze) makes `EnemyMgr_Calc` skip everything but the play-time / human-rate counters (no timelines, ECL, movement, VMs, damage, timers; draw lists kept). The port ran the enemy manager during the freeze (ECL particle effects at 6665) | SPEC FIXED + CODE BUG (I) | 07 §10.1 step 0, 11 §2.1 row 10 | demo 3 6684, demo 2 4660 (not chased) |
| selftest1 1540 | frame counter 0x164D2C8 := 0 as the last statement of `GameMgr_OnRemove` (0x43BE2C); the port never resets it. Only a CODE BUG (11 §2 was right; exact step added) | CODE BUG (D) | 11 §2 | 1922 |
| selftest1 1922 (last row, +2 draws) | exit save: WinMain registers Result(2) **after** the main loop (after the last `Frame_Run`'s calc/draw/Present) and the chain teardown saves; the port saves inside the last frame's Supervisor calc, before that frame's trace sample | SPEC FIXED (placement) + CODE BUG (E) | 15 §2.2 | full match (1922/1922 rows) |

Final scratch-port numbers with A–K: golden 10707, fullrun_noclip 9408, selftest1 no differing row, demo 0 5246, demo 1 4155,
demo 2 4660, demo 3 6684.

## 2. Spec sections changed

| section | change | label |
|---|---|---|
| 07 §6 var 10096 | the walk passes through the enemy currently running (siblings read the same count) | NEW FACT (clarification) |
| 07 §10.1 step 0 | GameFlags bit 10: `EnemyMgr_Calc` runs only its counters | SPEC FIXED |
| 07 §10.1 step 6 | exact `Enemy_ForwardDamageToRoot` 0x42B370 rule | SPEC FIXED |
| 07 §10.3 | death sfx/effects/time orb for every death mode; both effects gated by e+0x3310 ≥ 0 | SPEC FIXED |
| 07 §10.3a single block | gauge via `GaugeAdd(−gauge/12, not forced)` (0x42B223) | SPEC FIXED |
| 08 §8.4 | gauge-lock paragraph: GaugeAdd; dying branch keeps vel.x/z | SPEC FIXED / NEW FACT |
| 09 §6.1 | DropAttracted runtime evidence (demo 0) | (evidence only) |
| 09 §9.2 | third graze orb nested in the card branch | SPEC FIXED |
| 11 §2 | frame counter reset = last statement of `GameMgr_OnRemove` | NEW FACT (placement) |
| 11 §2.1 row 10 | adds the `EnemyMgr_Calc` skip | SPEC FIXED |
| 13 §2 effect table 21/26 | burst angle rule (≤ −990 → random − π, else wrapped param), draw order | SPEC FIXED |
| 13 §4 single-kill popup | GaugeAdd | SPEC FIXED |
| 15 §2.2 | WinMain exit save after the main loop, PLST tag condition | SPEC FIXED |

Symbols: `Enemy_ForwardDamageToRoot` 0x42B370, `g_EffBurstRandomBelow` 0x4B49A0 (re/symbols.csv).

## 3. Other observations (not root causes)
- Before fix C, golden bullets at 9040 differed from the original by 1–4 ulp in pos/vel (same angle/speed) and 1 bullet in count;
  they were downstream of the boss-life divergence at 8137 and were not chased separately.
- The original's per-frame order is unchanged by these fixes; every after-fix number above was measured, not predicted.

## 4. Tools / housekeeping
- Probe: `TH08_SCANBUL` (enemy bullets in scan.txt), `V` lines (boss float vars) in boss.txt — TOOLING_REPORT §15. Old build
  `re/work/hook/d3d8_v10_pre_q28.dll`.
- Game copy: score.dat, th08.cfg, probe_cmd.txt restored (md5 = `re/work/runs/q28/md5_before.txt`); a probe `quit` rewrites
  score.dat through the exit save.
- The user's port.cfg had `invuln = on` from 20:39; port runs after that used `TH08_INVULN=off` (earlier runs predate it).

## CODE BUG list (priority: sync first)

1. **Var 10096** (eclvm.rs `chain_after`): root → own count; child with a parent → the parent's count if the parent is a root,
   else 0; no parent and not a root → 0 (07 §6) — golden 8298, fullrun 7746. Remove the SPEC-GAP.
2. **Chain walks while an enemy runs** (enemy.rs `take`/`put`): the running enemy's links must stay visible to other walks
   (07 §6) — golden 8508, fullrun 8166.
3. **Root damage forward** (enemy.rs `damage`): skip while bombing; rd = d/2, root timer → rd/9 (boss) else 0; floor = max(0,
   thresholds); only if rd ≠ 0; life ≤ floor → floor (07 §10.1 step 6) — golden boss life from 8137.
4. **Death effects for every death mode** (enemy.rs `death`): sfx + effects + time orb also for mode 3 (and 4–7); both effect
   spawns only when e+0x3310 ≥ 0 (07 §10.3) — golden 10516, fullrun 9346.
5. **Burst effect 21/26 angle** (effect.rs): random (FloatRange(2π) − π) only when param.x ≤ −990.0, else
   `Angle_AddNormalize(param.x)` (13 §2) — golden 10533, fullrun 9363.
6. **`ItemMgr_DropAttracted`** at the end of `Player_Die` (both paths, also the dissolve-mode early return) and after the real-death
   drops: state-1 items → state 0, vel (0, −0.9, 0) (09 §6.1) — demo 0 4629. Remove the SPEC-GAP in player.rs.
7. **Single-kill gauge** (enemy.rs `kill_single`): `GaugeAdd(−gauge/12, not forced)` — nothing while bombing, clamped
   (07 §10.3a) — demo 1 3880.
8. **Graze orbs** (player.rs `on_graze`): third orb only inside the bullet-and-card branch (09 §9.2) — demo 2 4036.
9. **GameFlags bit 10** (enemy.rs `step`): while set, only the play-time / human-rate counters run (07 §10.1 step 0) — demo 3
   6665, demo 2 4641.
10. **Frame counter** 0x164D2C8 := 0 at the end of `GameMgr_OnRemove` (11 §2) — selftest1 1540.
11. **Exit save timing** (supervisor.rs): save after the last frame (after the main loop), not inside the requested-scene −1
    calc (15 §2.2) — selftest1 1922.

Next divergences on the scratch port with 1–11 (not chased): golden 10707 (score), fullrun_noclip 9408 (GameFlags bit 7),
demo 0 5246 (score +1), demo 1 4155, demo 2 4660, demo 3 6684.
