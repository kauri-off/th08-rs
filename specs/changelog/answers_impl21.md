# Answers to IMPL21 questions (RE-Q21)

Spec sections changed:
- 07 §6 var 10096: the exact walk, and what a child reads.
- 07 §4.3 field map: trail ring 0x3394, trail settings 0x534C–0x5352.
- 07 §10.1 step 4: trail ring layout and update.
- 07 §10.5: link position, parent pointer = spawner, the post-spawn overrides (contact bit 2).
- 08 §4.2: the +0xDBE mark is sticky; the double drop, with runtime proof.
- 08 §8.4: collection box / attract speed / line always come from the human sht.
- 11 GameFlags bit 2 (recomputed each frame); gameplay frame counter 0x164D2C8 (increment and resets).
- 14 §3.1: Yukari branch order.

Data in `specs/data/q21/`:
- `demo_*`: original escan/bscan/iscan/rngsite for demo 0 f4448–4452 and demo 1 f3866–3871; the command file is
  included.
- `demo0_port_bdump_f4450-4452.txt`: the port's `demosync 0 4448-4452` with `TH08_BDUMP=1`.
- `golden_*`: golden session replay (0 trace differences): scan rows 4465–4468, RNG sites 4466–4467, the items
  nearest the player at 4466.

## Q1. Var 10096 — VERIFIED (code)

`Enemy_ChainChildCount` 0x41FD40, used by the var getter:
- **Chain root** (parent +0x2DA4 == 0 and +8 ≠ 0): count the enemies reached by following **+8** from it. There is
  no alive test.
- **Enemy with a parent:** the same count evaluated on the parent. The parent is the enemy that executed op 90/91/92,
  not necessarily the root. A child of a root therefore reads the root's live child count; a child whose parent is
  itself a child reads 0.
- **Otherwise:** 0.

Every way an enemy leaves the chain unlinks it, so the walk never meets dead members:
- normal death (`Enemy_KillChildren(e, 1)` → `Enemy_ChainUnlink`);
- `Enemy_Free` (including op 1 delete and the off-screen free);
- `EnemyMgr_KillAll`;
- the parent's chain kill, which clears each child's links.

Your unlink of children freed without reward is therefore right. Linking (op 90–92) appends after the chain tail if
the spawner is a root; otherwise directly after the spawner. A child that spawns overwrites its own +8, which cuts the
rest of the chain (07 §10.5; no data case known).

## Q2. Yukari option — VERIFIED (code)

`Option_Yukari` 0x44E3A0, sub-states 0, 1 and 2 each run, in this order:
1. `Option_YukariFollow` 0x44E770:
   - follow step (smoothing, pos += v, |v.x| < 0.05 → 0);
   - **then the attack test:** shot timer ≥ 0 and aimed ≠ 0 and option timer ≥ 10 → `AnmVm_Interrupt(3)`,
     sub := 3, return (the aimed pointer is **kept**);
   - else aimed := 0.
2. Back in the caller, the **animation test of the sub-state the option entered the frame in runs regardless**:
   - sub 0: v.x > 0 → interrupt 2, sub := 2 (mirrored); v.x < 0 → interrupt 2, sub := 1; v.x == 0 → nothing.
   - sub 1: v.x == 0 → interrupt 1, sub := 0; v.x > 0 → interrupt 2, sub := 2; same sign → nothing.
   - sub 2: v.x == 0 → interrupt 1, sub := 0; v.x < 0 → interrupt 2, sub := 1; same sign → nothing.

So an attack started in sub 0 while v.x ≠ 0 is overwritten by sub 1/2 and interrupt 2, but the aimed pointer stays
set. The next frame, the sub-1 test passes again: sub 3. v.x keeps its sign, so the animation test changes nothing
and sub 3 sticks. The first chase step comes on the frame after that.

This matches golden 1462 → 1463 (sub 1, interrupt 2) → 1464 (sub 3). At f1432 the option was in sub 2 with v.x > 0
(same sign), so sub 3 survived directly. Sub 3 itself: chase if aimed ≠ 0; leave (aimed := 0, interrupt 1, sub 0)
when (shot timer < 0 and the shot button is released) or aimed == 0.

## Q3. Pause — VERIFIED (code)

- **GameFlags bit 2** is rewritten on every `GameMgr_Calc`, before the clear/stage-end part and after the pause-key
  test. Its value is 1 iff paused byte GM+0x3DBB2 (0x164D0BA) == 0 and game-over byte GM+0x3DBB3 == 0.
  - ESC sets paused := 1 earlier in the same calc, so the bit drops **on the ESC frame**.
  - It comes back on the first calc after the pause menu sets paused to 0.
  - It also stays clear while the game-over/continue state is set.
  - Separately: stage start sets it, and `GameMgr_OnRemove` clears it.
- **Frame counter 0x164D2C8** (GM+0x3DDC0):
  - Incremented near the end of `GameMgr_Calc`, after the tamper check. It is not incremented on the early returns:
    loading, paused or game over (return 3), and the slow-mode frame skip.
  - Set to 0 only by `GameManager_Register` and `GameMgr_OnRemove`. A pause-quit unregisters the game manager, so
    the counter reads 0 from then on in the menus. The port's 809 should be reset at that teardown.
- Your paused 1 → 2 rule is not contradicted.

## Q4. Bullet → item cancel and collection

Common rules (08 §7, §4):
- `CancelAll(mode)` converts **every** non-free bullet that is not in state 5 (mode 4: state 5 only, no item).
  Bullets in a bomb area give the area's item type; others give type 6 with item mode = `mode`.
- `CancelBonus` converts **every** non-free bullet, including those already in state 5 (type 6 or area type, mode 1),
  and sets state 5.
- There is no per-frame cap and no distance limit. Items with mode 1 make no RNG draws.
- `ItemMgr_Spawn` fails silently (dummy item, no draws) if pos.x is outside the allowed x range; type 7 (time orb) only
  probes the one slot at the item cursor.
- Score: `AddScore(v)` adds v/10 (truncated) to the stored score, per call.
- Cancel-bonus popups are 2000, 2020, … capped at the maximum, so they are multiples of 10. Rounding never loses a
  digit there.

**Demo 0 f4451 — VERIFIED (runtime): 5 time orbs, from three bullets ending their spawn effect.** The 20 draws are 5
time-orb spawns (`ItemMgr_Spawn` +0x23A/+0x258), area item type 7. The 5 new orbs (items 855–859) sit exactly on
bullets 181, 182 and 183, which were in **state 2** at 4450:
1. **Effect end:** each had its bomb-area mark +0xDBE set during the spawn effect. When the effect ended this frame,
   each dropped an orb at its half-step position (y −0.9 → 0.1 for 183, 49.9 → 50.9 for 182, 15.1 → 16.1 for 181).
   The state is set to 5, then overwritten to 1 (08 §4.2 quirk).
2. **Same-frame state-1 update:** 181 and 183 are hit by the bomb area again → state 5 and a **second** orb at the
   full-step position (2.0, 18.0). 182 is no longer inside → stays in state 1, having dropped one orb.

The port's bullet states match the original at 4450 and 4451, so it converts the same bullets. Its missing orb is one
of the five drops:
- most likely the effect-end drop of a bullet that is outside the area at the end of the effect (182);
- the mark is **sticky**: set on any spawn-effect frame inside the area, never cleared.

The draws and call sites are in `demo0_f4449-4452_demo1_f3867-3871_rngsites.txt`.

**Demo 1 f3870 — rule:**
- The original collects 55 time orbs and 3 point items in that frame (110 + 12 checksum draws), after 30 orbs and 2
  points at 3869.
- In list order, each orb calls `AddTime(1)`. That raises max point value (stats +0x24) by 10 on every second orb
  (`((n + parity) / 2)·10` with the updated parity +0x44).
- A point item collected later in the same pass uses the raised max: v = max/2 − trunc(y − line)·(max/1000), then
  minus v % 10.
- So a 10-point (1 stored unit) difference comes from collection order or the time parity, not from cancel rounding.
- Check that items are processed in list (spawn) order with orbs and points interleaved, and that +0x44 carries over
  from earlier frames.

**Golden row 4467 — rule + data:**
- The frame collects 9 attracted time orbs (state 1).
- At the end of 4466, the 9th-nearest orb (slot 719) and the first one not collected (slot 699) differ by < 1 px in
  distance to the player (`golden_4467_items_near_player.txt`). So this is a boundary case of the attract step.
- The attract step for state 1: vel = polar(angle item → player, attract speed sht +0x14), pos += vel, then the
  collection box test.
- The box (C, C), the speed and the collection line always come from the **human** sht (0x18B896C), even though the
  focus byte +3 changed 1 → 0 at 4466. Only the fall multiplier switches with +3.
- If the port takes C or the speed from the youkai record, or rounds the polar/angle in f32 differently, slot 719 is
  the orb it misses.

## Q5. Demo 2 f2261 — see answers_impl19 §Q3 (RE-Q19 addendum)

The original child fires the same 2 bullets, in the same slots, at f2279:
- the thread's random wait is 30;
- the spawn-frame double run makes it 2250 + 29.

Nothing suppresses firing for chained children. The flag difference is unrelated: ops 90–92 clear contact bit 2 (and
set bit 8, bit 11, layer, aura and links) **after** the child's spawn-time `Ecl_Run`. That overrides sub16's own
`flags_on 3` → 0x01001149. Contact plays no part in firing (gates: bullet flags 0x8000/0x10000 vs bit 11, the 32 px
no-fire radius, 07 §9). Apply the post-spawn overrides after the spawn run and fix the 10032/`imod3` evaluation.

## Q6. fullrun_noclip row 6264 — see answers_impl20 §Q5

- Row 6264 is in stage 1, not stage 2.
- The original **does** spawn the single-kill time orb (4 draws; item 1151, type 7, state 3). Its 32 draws are 12 hit
  sparks (3) + 4 (orb) + 16 death sparks.
- The port's extra 4 draws are a 4th hit spark from `Player_DamageAgainst`, not an item.
- The port's score +10 fits the same cause: one extra overlapping shot also adds (d/5)·10 damage score.
- Look at the shot/enemy overlap or hit-spark counter for that frame.

## Q7. Trail ring buffer — VERIFIED (code)

- **Storage:** e+0x3394, **96 entries × 0x1C**: final pos x, y, z (e+0x2D88); velocity x, y, z (e+0x2D4C); angle
  (e+0x2D94).
- **Initialisation:** `EnemyMgr_Ctor` sets every entry's x to −999.0 in the template (other fields 0), and every
  spawn copies the template. Unwritten entries are therefore off screen, and the free test depends on the current
  position until n entries have been written.
- **Settings (op 157):**
  - e+0x534C flags byte: ≠ 0 enables the trail; 8 allocates the draw history; 0x10 hides the main VM;
  - e+0x534E n (length; data uses 15 and 90);
  - e+0x5350 a;
  - e+0x5352 b (draw step: the draw walks entries from n − 1 down in steps of b).
- **Write order:** inside the step-4 loop of `EnemyMgr_Calc`, after movement and before the screen check, on every
  pass while the flags byte ≠ 0:
  1. shift up: for i = n − 1 down to 1, entry[i] = entry[i − 1];
  2. entry[0] := current.

  A phase-change re-run shifts twice in that frame.
- **Readers:**
  - The off-screen free uses entry[n − 1] (both it and the current position must be off screen, same sprite box).
  - `EnemyMgr_KillAll` drops a type-6 item and a score popup at every 6th entry (i = 0, 6, 12, …< n) for flag A bit 7
    enemies.
