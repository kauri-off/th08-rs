# Answers to IMPL19 questions (RE-Q18)

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
