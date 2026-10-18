# Answers to IMPL16_QUESTIONS (dirty room, 2026-10-18)

Sources: th08.exe (`re/extracted/all_decomp.c`, disassembly, displacement scans) and two wine-probe runs of the
attract demos. The probe has two new commands: `escan K F0 F1` dumps every live enemy slot per frame (flags, final
pos, life, time-orb accumulator e+0x2E10, item, timer, parent, child count). `iscan K F0 F1` dumps the item list
plus player position, shot timer, sht collection values, gauge, orb lock and point counters. Both write after the
frame's calc (TOOLING_REPORT §10). Run data: `re/work/runs/q16/` (commands `re/work/cmds/q16.txt`, `q16b.txt`).
Frame numbers are trace frames (GM+0x3DDC0), as in `specs/data/demo*_trace.txt`.

Spec sections fixed in place:
- 07 §4.3: new row +0x2E10.
- 07 §7 op 66: delta.x is negated when mirrored.
- 07 §10.4: drop sparks, counter and index initial values, check-then-increment.
- 08 §8.4: state 3 → 1 example; collection-box geometry.
- 09 §4.5: damage is uncapped; only the accumulator add is capped; the orb loop's timing.

## 1. Shot damage cap — VERIFIED (`Player_DamageAgainst` 0x451670)

The port's new rule is right, and the spec was wrong:
- `Player_DamageAgainst` returns the **uncapped** sum of the shot damage, plus the area damage, × 106/100 at youkai
  max gauge.
- Only the accumulator increment is capped: after the bullet loop, `accumulator += min(shot sum, 50)`. The area
  damage is not added to the accumulator.
- The enemy side then applies its own caps (70 for the score, 07 §10.1).

The time-orb `while (accumulator ≥ step)` loop sits **inside** the bullet loop. It runs once per hitting bullet and
sees the accumulator value from earlier frames: this frame's add comes after the loop. So an accumulator ≥ 2·step
gives 2 orbs on the first hitting bullet, and the later bullets of the frame find it below the step.

Orbs need two things: human max gauge, and a human-side shot entry (sht entry +0x1E < 0). They spawn at the bullet
position with `ItemMgr_Spawn(pos, 7, 3)`.

## 2. Which accumulator; when it is reset — VERIFIED (code + runtime)

There is **one accumulator per enemy, e+0x2E10**, for normal enemies and bosses alike. The port's shared counter is
wrong. The thing it was compensating for is the **initial value**:
- `Player_Load` writes the time-orb step (40, or 27 for solo human; player+0xE2B2C) into the enemy template's +0x2E10
  (global 0x57AD30 = EnemyMgr 0x577F20 + 0x2E10).
- `EnemyMgr_Ctor` (every `EnemyManager_Register` at stage start; it runs after `Player_Register`) also copies
  player+0xE2B2C into the template's +0x2E10.
- `EnemyMgr_SpawnFromTimeline` / `SpawnFromEcl` copy the whole template (0x53D0 bytes) into the slot.

So every fresh enemy starts with **accumulator = step**. Its first hitting bullet spawns an orb at human max gauge
(40 ≥ 40), and the accumulator drops to 0 before the frame's add. Nothing else writes +0x2E10 (displacement scan:
the only store is in `EnemyMgr_Ctor`; the only other user is `Player_DamageAgainst` through the pointer). It is never
reset while the enemy lives.

Runtime (probe template value `tmpl_acc 40 step 40` on every frame):
- **Demo 0 frame 1129:** two fresh enemies. Slot 0 is hit for 22 and ends the frame at acc 22. Slot 1 is hit for
  11 and ends at acc 11. Each gave 40 → orb → 0 + damage, so 2 orbs (8 draws), as the trace needs.
- **Demo 3 frame 801:** fresh slot 9 is hit by 26 + 16 = 42. The first bullet gives the orb; the frame ends at
  acc 42. This is the trace's +4.

## 3. Demo 3 frame 826 — VERIFIED (follows from §2)

Slot 9 at this point is a re-spawned enemy. It went from acc 40 at spawn (frame 813) to 42 after its first hits
(frame 821). The killing hit (26) finds acc 42 ≥ 40, which gives **one** orb (42 → 2), then 2 + 26 = 28. With
per-enemy accumulators the port's draw count is 4 hit spark + 12 drop sparks + 16 death sparks + 4 death orb + 4
damage orb = 40, matching the trace.

## 4. Drop sparks — VERIFIED (`Enemy_DropItems` 0x42BEA0)

The port is right:

| item | action |
|---|---|
| ≥ 0 | `Effect_Spawn(e+0x3311 + 4, pos, 3)`, then `ItemMgr_Spawn(pos, item, mode)` |
| −1, counter % 3 == 0 | `Effect_Spawn(e+0x3311 + 4, pos, **6**)`, then the table item (index += 1, wraps at 32) |
| −1, otherwise | nothing (no sparks, no item) |
| ≤ −2 | nothing (counter unchanged) |

For item −1 the counter is incremented **after** the check, in both cases. The extra point/power items
(e+0x330C / +0x3308, 2 `Rng_NextFloat` each) follow in all cases.

## 5. Every-third counter start — VERIFIED (`EnemyManager_Load` 0x42EBF0)

The counter is the u16 at mgr+0x9DCDC0 (0xF54CE0) and the table index is the u16 at mgr+0x9DCDC2 (0xF54CE2). Both
are set at **every stage load** (loader thread, after the ECL file is loaded):
- counter = `Rng_Range16(3)` = `Next16() % 3`;
- index = `Rng_Range16(8)`.

These are the "2 draws" already listed for `EnemyManager_Load` in 11 §3. Nothing else writes them (byte scan:
`EnemyManager_Load` and `Enemy_DropItems` only; `EnemyMgr_Ctor` zeroes them before the load). Check: `counter % 3 == 0`,
then `counter += 1` (pre-check, post-increment).

So the port's "start value 2" was the RNG result for demo 2. Use the two `Range16` results. The table index also
comes from the second draw, so the dropped item type depends on it too.

## 6. KillChildren step 5 — VERIFIED

There is no boss condition. The orb shower and the 2× parent-count popup run inside `if (chain child count ≠ 0)`
(that count is non-zero only for a chain root, `Enemy_IsChainRoot`) and `if (reward)`. `EnemyMgr_Calc` always
passes reward = 1 on death. The shower size is 2 × parent +0x3380.

## 7. Parent child counter +0x3380 — VERIFIED

There is no reset after a chain kill. Displacement scan writers:
- ops 90/91/92 (`Ecl_Run`): += 1 on the parent;
- `EnemyMgr_Calc` death path: −= 1 on the parent when the dying enemy still has a parent pointer.

`Enemy_KillChildren` and `Hud_DrawBoss` only read it. Spawns copy the template (0).

## 8. Demo 1 frame 560 bullets — VERIFIED (code + runtime): the enemy moves the wrong way in the port

The bullets are not special. The original spawns them **on screen**, because the mirrored enemies move **left**.

Stage 4A `ecldata4a.ecl` sub 0 starts with `move_dir 30, 4, 0.0, 3.0` (op 66, angle 0, speed 3, 30 frames, ease 4).
For dur ≥ 1, `Ecl_OpMoveDir` 0x420D10 builds delta = (cos a·spd·dur, sin a·spd·dur, 0) and then **negates delta.x
when the enemy's mirror bit 18 is set**. The 07 table did not say this. Op 69 with dur ≥ 1 calls the same function.
Op 64 already had it. Ops 67/178 (`Ecl_SetMode2Polar`) do not mirror.

Then mode 2 negates the velocity x again, and `Enemy_Move` subtracts it. The net motion is toward start + delta,
i.e. to the left.

Runtime (`escan`, demo 1; flags 0x01052049, bit 18 set):

| frame | slot 1 (e1) | slot 2 (e2) |
|---|---|---|
| 545 | (367.6, 64) t 5 | — |
| 549 | (350.0, 64) t 9 | — |
| 555 | (329.6, 64) t 15 | (367.6, 64) t 5 |
| 559 | (320.0, 64) t 19 | (350.0, 64) t 9 |
| 565 | (311.6, 64) t 25 | (329.6, 64) t 15 |

So bullets 74–77 come from (350, 64), not from (450, 64). They are on screen when the spawn effect ends, and
§4.3 keeps them. The later sub-1/sub-2 volleys also come from x ≈ 310–390, which explains the trace's bullet
counts. The original still frees slots 78–81 (spawned from x = 400) on frame 560, as answers_impl14 §2 said.

The off-screen rule and the counters are unchanged:
- the grace +0xDA8 is 0 for these bullets;
- off-screen counter 0;
- the box comes from the VM's current sprite.

## 9. Demo 2 frame 724 — VERIFIED (runtime): two time orbs collected; the port kept one in state 3

What happens on frame 724 (probe `iscan`, `rngsite`; player calc runs before the item calc):
- Three time orbs are homing in state 1 at speed 10 (sht +0x14):
  - item 358 (first dumped on frame 682, t = 1);
  - item 359 (first dumped on 694);
  - item 361 (first dumped on 710).
- After frame 723 they are at (262.3, 134.1), (255.0, 134.9) and (268.5, 121.2). The player is at (245.3, 170.5),
  moving up 5 px/frame, and is at (245.3, 165.5) when the items are tested on frame 724.
- Collection box: |dx| ≤ 24 and |dy| ≤ 24 (item ±12 vs player ±12, sht +0x18 = 24 for both; 08 §8.4).
  - Items 358 and 359 move to ≈ (258, 143) and (252, 145) and are collected.
  - The converted point item 142 (type 8, state 0, at (233.9, 143.9)) is collected too.
  - Item 361 is collected on frame 725.
- Each time orb (`Item_CollectTimeOrb`):
  - score +100 (bomb flag clear, stats +0x2C < 2000, (stats +0x30 / 2)·10 = 0 → minimum 100);
  - `GameMgr_AddTime(1)` → `GameMgr_UpdateChecksum` = 2 `Rng_Range` = **4 draws**;
  - spell bonus +8000;
  - gauge −111 (player +3 = 0, orb lock timer 0).

  Two orbs = the trace's +8 draws, time +2, gauge −222. Frame 725: the third orb (−111, 4 draws).
- Score: type 8 at y 143.9 > line 128 → v = 301660/2 − 15·301 = 146315 → /10 → 14631 → 14630 (no ×2: gauge
  is not at human max). Plus 2 × 100 = 14 830. The other 100 of the trace's +14 930 is shot-damage score that
  frame (INFERRED: enemies are being hit on 723/724).

**Why the port has none of it:** its orbs are in the wrong place. In the port the second orb is at (284, −9). That
is where item 359 sat while it was still rising in **state 3**.

In the original, every state-3 orb switches to state 1 on **frame 707**. That is the first frame the player's shot
timer (player +0xE2AC4, 0x18B89BC) is −1. Values per frame: 13 … 19 on frames 700–706, then −1 on 707. The input
has no shot bit, so the timer runs out its 20-frame cycle and goes to −1 (09 §4.2).

The switch happens **with vel.y still negative**: item 359 at frame 707 had pos (284.1, −22.4), vel.y −1.06, state 3
→ 1. It homed from frame 708 on. Item 361, spawned when the player was no longer shooting, is already in state 1 on its first dumped frame (710).

Rule (08 §8.4, already in the spec):

`state 3: vel.y += 0.05·gs; if vel.y > 0 or shot_timer < 0 → state 1`

The shot timer is the player's Timer current value, after this frame's player calc. The port is probably testing
the shot input or a stale timer.

Per-frame data for frames 640–726: `re/work/runs/q16/run2/probe_items.txt` (decode as in the table above; shot timer
= `shot` field).
