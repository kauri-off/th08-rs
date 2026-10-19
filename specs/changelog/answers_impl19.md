# Answers to IMPL19 questions (RE-Q18)

Spec sections changed (RE-Q19 addendum, §Q3 completed below):
- 07 §6 var 10096: number of children linked in the chain (root's count), not the chain depth.

Spec sections changed:
- 08 §8.5: the power clamp goes through `GameMgr_SetPower` (checksummed); type 4 always calls SetPower and its
  cancel is unconditional.
- 07 §10.1 step 4: the screen check uses the main-VM sprite box, inclusive; an enemy with no sprite gets bit 4;
  the trail case.
- 11 §3 "Loading frame count": upload timeline, where the "+6" comes from.

Runtime data in `specs/data/q18/`:
- `demo_enemies.txt` (escan, now with sprite and hitbox sizes);
- `demo_bullets_d2f2256-2262_d3f1496-1502.txt`;
- `demo_rngsites.txt` (demo 3 f1057–1060, f1499–1502, demo 2 f2259–2262);
- `selftest1_loadlog.txt`;
- `demo_probe_cmd.txt` (the command file used).

## Q1. Full-power clamp — VERIFIED (code + runtime)

Yes, the clamp is a checksummed write.
- Types 0 and 2: `GameMgr_AddPower(n)` (checksum, 4 draws). If power ≥ 128 after that, `GameMgr_SetPower(128)`
  follows (checksum, another 4 draws).
- Demo 3 frame 1059 call sites (`demo_rngsites.txt`): AddPower 4 + 4 for two small-power collects, then SetPower 4
  for the second one = 12.
- The full-power item (type 4) calls `GameMgr_SetPower(128)` **on every collect**, even when power is already 128,
  so it always costs 4 draws.
  - Its below-128 branch cancels bullets to items **without** the spell-card test that types 0/2 use.
  - Then HUD message, sfx 0x1F, popup, item conversion; always +1000 score and a "1000" popup.
- Bomb items (type 3) go through `GameMgr_AddBombs` (checksummed) only when bombs < 8, then rank +5.

The port's SPEC-GAP can be closed.

## Q2. Enemy off-screen removal box — VERIFIED (code + runtime)

- **Box:** the main VM's **current sprite** (sprite +0x34 width, +0x30 height, unscaled), centred on the enemy's
  final pos (e+0x2D88). It is not the hitbox (e+0x2D70) and not a fixed margin.
- **Test:** `GameMgr_IsOnScreen` = x + w/2 ≥ 0 && x − w/2 ≤ 384 && y + h/2 ≥ 0 && y − h/2 ≤ 448. All four
  comparisons are inclusive; the enemy is freed only when the test fails.
- **Sequence:**
  - If e+0x230 (sprite pointer) is 0, bit 4 is set first.
  - Unless bit 4: while bit 24 is clear, the first on-screen pass sets bit 24.
  - Afterwards, with bit 28 clear, an off-screen test frees the enemy (`Enemy_Free`, no drops). Enemies with an
    after-image trail need both the current pos and the oldest trail pos off screen.
- **Runtime:** both enemies have a 32×32 sprite and a 24×24 hitbox.

  | enemy | frame | pos | sprite box (half 16) | hitbox (half 12) |
  |---|---|---|---|---|
  | demo 0 e0 | 1720 | (144.87, −12.57) | bottom edge +3.43 → on screen (kept until y < −16) | −0.57 → freed (the port's result) |
  | demo 1 e10 | 1808 | (7.92, 461.51) | top edge 445.51 ≤ 448 → kept | 449.51 → freed |

## Q3. Extra bullets (demo 2 f2261 +2, demo 3 f1501 +12) — PARTIAL (runtime facts, cause open)

- **Demo 3 frame 1501:**
  - The original has 77 → 80: three new bullets in slots 331–333, state 2, colour 6, flags 0x203, 14×16 sprite.
    They form one aimed stack: angle −2.1281, speeds 2.763 / 2.002 / 1.242.
  - Their origins (pos + 4·vel ≈ (191.3–191.7, 106.8–107.5)) are at the **parent** enemy e1 (life 1000, flags A
    0x01111049, t 96, at (192, 108.25), 4 children).
  - Its 4 chained children e2–e5 (life 100, flags A 0x01003349, flags B 2, t 17, ±7 px around e1, parent = e1)
    fire nothing on that frame.
  - The port's +12 = 4 × 3 strongly suggests the port lets the four children fire the same 3-bullet stack.
  - No bullet-spawn RNG is involved (the frame's draws are effect scripts only: `Enemy_ReactPlayerMode` effects
    and effect 51 particles).
  - Next step for the port: dump which enemy spawns each of the 12 extras. If they are the children, compare their
    ECL path. The ECL sub was not identified statically: the stack comes from a thread or autofire, not a timed
    instruction.
- **Demo 2 frame 2261:**
  - The original counter is 138 (0x8a); 137 slots remain after the calc.
  - The only change from frame 2260 is slot 735 (state 1, at (390.64, 13.58) on 2260, speed 1.4, 14×16 sprite),
    which leaves the right edge and is freed. No bullets were spawned on 2256–2262.
  - The port's +2 therefore comes from bullets that exist in the port but not in the original by that frame. Diff
    `demo_bullets_…txt` against `TH08_BDUMP` to find them.
- Known spawn gates (08 §4): the live-count cap 1536, the 10-frame "spawn as state 5" after a cancel
  (mgr+0x6BA53C, unless flag 0x1000), and the spawn-failure flag (manager +0x9DCEF8) for chained children whose
  parent has life < 1. None of them applies at demo 3 f1501 (no cancel, count 77).

## Q4. Stage loading floor — VERIFIED (runtime, selftest1 stage 1)

Measured with `TH08_LDLOG=1` (`selftest1_loadlog.txt`). Loading starts at poll 566 (lf 0) and is released at
poll 614 (lf 48).
- The loader thread preloads (reads and decodes) one ANM file. The main thread then uploads **exactly one entry per
  frame** (`Anm_PostloadStep` from `Supervisor_Calc`). When a file's last entry is uploaded, the loader preloads
  the next file.
- If that preload finishes before the next frame's `Supervisor_Calc`, there is no gap. A slow decode leaves 1–2
  frames with no upload.
  - Files and entries: player00 2, stg1bg 1, etama 6, enemy 1, stg1enm 1, eff01 2, front 2, times 1,
    loading00 1, stg1txt 1, face_cdbg 1, face_rm00 9, face_yk00 9, face_st01 5 = **42 uploads**.
  - Empty frames: lf 2 (before stg1bg), lf 20–21 (face_rm00 decode ≈ 50 ms), lf 31–32 (face_yk00), lf 42
    (face_st01) = **6**.
  - The rest of the stage start (gauge setup, `Loading_Wait`, release) happens in the frame after the last upload.
- So "+6" is host decode time. It is **not** one frame per file: that rule would give 42 + 14 = 56, not the
  measured 48.
- Rule for the port:
  - Keep max(31 | 81, entries + gaps), with gaps = 6 for the fresh Reimu & Yukari stage-1 starts.
  - Use the recorded `release stage poll` for session sync.
  - For other loads the gaps depend on which large images (faces, stage backgrounds) are decoded. A table of
    gaps per file is host-specific and not derivable from the code.

## Notes (no change)
- Replay fps text and ribbon stale colours: nothing contradicts the rules picked; not re-checked.

## Q3 completed (RE-Q19) — VERIFIED (code + runtime)

New runtime data in `specs/data/q19/`: `demo_enemies_bullets_excerpts.txt` (escan/bscan of demo 2 f2249–2251,
2278–2280 and demo 3 f1484–1486, 1501), `demo2_f2250_rngsites.txt`, `demo2_port_vs_orig_bullets_f2256-2262.txt`
(the port's `demosync 2 2255-2262` with `TH08_BDUMP=1`, diffed slot by slot), `demo_probe_cmd.txt`.

### Demo 3 frame 1501 (port 92 vs 80): var 10096 is the chain child count
- Stage 2 (ecldata2). The parent e1 is sub7. At t 80 it creates 4 `child_follow` sub10 children (spawned on frame
  1485). Its thread sub8 fires every 32 frames: `n1 = 5 − var10096` ways × `n2 = 3` (Lunatic) aimed bullets.
- **Var 10096** = `Enemy_ChainChildCount` of the chain: for a root, the number of enemies linked after it (+8 links);
  for a chained child, its root's count; 0 when the enemy is in no chain. The old spec said "depth in the chain",
  which is wrong.
  - With 4 children linked, n1 = 1, so the original fires one 3-bullet stack (the 3 bullets seen at f1501, and again
    at f1533).
  - A port that reads 0 here fires 5 × 3 = 15, i.e. **+12**: exactly the difference.
  - As children die and are unlinked, the parent's fan widens (2, 3, 4, 5 ways).
- The children fire nothing at f1501 because of their own script, not a gate. sub10 orbits until t 100 and only then
  starts thread sub9. They are at t 17. No fire gate applies:
  - the bullet-flag 0x8000 / 0x10000 youkai/human gates (flags 0x203);
  - the 32 px no-fire radius (the player is ≥ 50 px away).

### Demo 2 frame 2261 (port 0x8c vs 0x8a): a child's first shot comes 18 frames early in the port
- Diff (port block = after calc, against the original bscan): the port has two extra state-2 bullets.
  - Slots 156 / 157, at (330.0, 77.2) / (325.8, 77.5), angle 3.06, speeds ≈ 2.86 / 1.67.
  - Origin ≈ (320, 78) = the child e2.
  - Everything else is identical.
- The original fires **the same two bullets in the same slots on frame 2279** (child timer 30), not 2261.
- Script (ecldata3): parent sub14 spawns `child_rel sub16` on frame 2250. sub16 at t 0 starts thread sub15, which
  does:
  1. `imod3 $10000, $10032, 30`;
  2. `iadd $10000, 10`;
  3. `wait $10000`;
  4. then (Lunatic) `bul_fan_aim` n1 1, n2 2, speed 2.9: the 2-bullet aimed stack.
- Original value of the wait (computed from the logged RNG; the state after the frame's 32 draws is edfe, matching the
  trace):
  - The thread's read of var 10032 is draws 9–10 of frame 2250 (`Ecl_GetIntArg` ← `Ecl_Run` ←
    `EnemyMgr_SpawnFromEcl`), starting from state 47eb.
  - Next16 returns 0xE72F, then 0x2F30.
  - `Rng_Next32` = **first draw << 16 | second draw** = 0xE72F2F30.
  - Var 10032 = that & 0x7FFFFFFF = 0x672F2F30 = 1731145520.
  - % 30 = 20, + 10 = **wait 30**.
- Timing rule: the child is in a higher slot than its parent, so on its spawn frame its ECL runs twice. It runs once
  inside `EnemyMgr_SpawnFromEcl` and once in the slot loop (07 §10.1). The wait therefore loses one extra count on
  frame 2250, and the shot comes at 2250 + 30 − 1 = **2279**, as observed.
- The port's shot at 2261 implies a wait of about 12. None of the obvious variants reproduces it:
  - halves swapped: 23;
  - no 0x7FFFFFFF mask, unsigned: 38;
  - no mask, signed C `%`: −8.

  So check the port's evaluation of `imod3` with a random variable as its source. Also check that the spawn-frame
  double run and the thread's first `wait` decrement both happen.
- The RNG stays in sync because the draw count is the same; only the value use differs. This is a gameplay
  divergence: the bullets exist 18 frames early.
