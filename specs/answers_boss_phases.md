# Boss phase transitions (RE-Q19 item 1)

Spec sections changed:
- 07 §4.3 (0x2CE8 / 0x2CEA meaning), §5.4 (new "Depth storage" rule: phase changes do not reset the main stack depth).
- 07 §6: var 10096 (chain child count, not depth), var 10098 (comparison direction was inverted).
- 07 §10.1 step 4 (what the re-run repeats).
- 07 §10.2 rewritten (check order vs damage, strict `<`, first index wins, what the switch resets and keeps, warning
  levels, timeout = "marked timed out" + cancel, not a card end; player state 3 for 70 frames).
- 07 §10.3 (common pre-steps, mode table corrected: mode 2 does not turn the HUD off, mode 3 details, latch clear,
  death sub runs on the next frame).

Runtime data: `specs/data/q19/` (README below). Source: `fullrun_noclip` replayed in the original up to poll 11000
(stage 1 only) with the new `TH08_BOSS` probe log (0 trace differences against the recording).

## 1. Stage 1 boss script (ecldata1, data facts)

| sub | role | entered by | key ops at entry |
|---|---|---|---|
| 25 | boss entry / dialogue idle | timeline spawn (life 60000) | boss 0, flags_off 20/3, timeout 180000 → 38, move_to 60, interrupt[1] = 26, idles (jump to itself at t 100) |
| 26 | non-spell 1 | interrupt 1 from the timeline (dialogue end) | flags_on 4/3, dmg_timer 120, spellcard_end (no card: nothing), set_life 13000, death_mode 2, death_sub 33, timeout 1800 → 38, threshold 0 = 1500 → 38 |
| 38 | spell 1 | threshold 1500 (or timeout) | t 0: autofire 0, kill_all, flags_off 4 (no life loss), timeout 1920 → 33, op 153 (timeout sub := death sub 33), set_timer 0, move_to 90, spellcard; t 90: flags_on 4, dmg_timer 180, death_mode 2, death_sub 33 |
| 33 | non-spell 2 | death of spell 1 (death sub) or its timeout | flags_on 4, spellcard_end, dmg_timer 150, set_life 13000, death_mode 3, death_sub 50, timeout 1920 → 44, threshold 1800 → 44 |
| 44 | spell 2 | threshold 1800 | same pattern as 38 (timeout 2220 → 50, death sub 50) |
| 50 | spell 2 end | death (mode 3) | spell_ex 1, life var := 1, death mode 0, flags_off 3, …; t 10: spellcard_end, boss −1, … final-spell enemy sub37 |
| 37 | final spell host | spawned | deletes itself unless var 10098 ≥ 2; boss 0, set_life 1600, last_spell; t 132: death_mode 3, death_sub 51, call 48 |

## 2. Exact sequence (fullrun_noclip; poll = session poll, gm = gameplay frame = poll − 240 here)

All values are read after the frame's calc. "clk" = main-context clock after the frame's tick.

| poll (gm) | what happens |
|---|---|
| 5335 (5095) | sub25 boss registered (boss 0, HUD boss display on). |
| 5581 (5341) | timeline `interrupt_boss 0, 1` → pending 1 → sub26 pushed (depth 1); its t 0 runs this frame: life = max = 13000, threshold 1500, timeout 1800 (timer := 0 → 1 after the tick), death sub 33. |
| 6564 (6324) | sub26 frame runs; shot damage (step 6) takes life 1502 → **1464** (< 1500). No switch yet. |
| 6565 (6325) | **Threshold switch** in step 4 after sub26's first run of this frame: life := 1500, HUD life 1500 (max stays 13000), bonus seconds e+0x53CC = (1800 − 984)/60 = 13, timeout −1, player state 0 → 3 (timer 70). Same frame re-run of sub38 t 0: `kill_all`, `flags_off 4`, timeout 1920 (timer 0 → 1), timeout sub := 33, `move_to`, `spellcard` → `Spell_Start`: bullets 97 → 0 (cancelled to items in this frame), spell flags 0x5, card timer 1920 → 1919. Depth stays **1** (§3). |
| 6565–6654 | boss takes no damage (flags A bit 3 clear); the move_to runs 90 frames. |
| 6655 (6415) | sub38 t 90: `flags_on 4` (damage on), dmg_timer 180 (bosses take d/9 while > 0), death mode 2, death sub 33. |
| 7494 (7254) | damage takes life 1 → 0 → **death** (mode 2): latch, bonus seconds 16, thresholds/timeout −1, chained children killed with drops (enemies 10 → 2), drops; card active → no bonus cancel; life := 0; main context → sub33 at clock 0, **not executed this frame**. Card still active (flags 0x5) and its bonus still decays this frame. |
| 7495 (7255) | sub33 t 0: `spellcard_end` → captured (flags 0x304), `CancelBonus` + `KillAll` (the 140 bullets turn to bonus over the next frames), player state 3 (timer 70); set_life 13000, threshold 1800, timeout 1920; depth reloads as **2** (the death happened inside the call to sub41). |
| 7625 | capture payout (spell flag bit 8 cleared) 130 frames after the end. |
| 8368 (8128) | threshold 1800 → sub44 (spell 2), same pattern as 6565 (bullets 115 → 0, player state 3 / 70). |
| 9346 (9106) | death with **mode 3**: life := 1, HUD boss display off, player state 3 with timer **90**, no drops, no cancel (293 bullets stay); death sub 50 next frame. |
| 9347 | sub50 t 0: spell_ex 1 (spell flags 0x805). |
| 9407 / 9408 | sub50 ends the card and unregisters (boss 0 empty for one frame); sub37 registers the final spell host. |
| 9567 → 10844 | final spell (sub48) via sub37's call; death mode 3 at 10844 → sub51. |

The midboss (subs 15/16/18, polls 3176–3953) follows the same rules (death → sub18 next frame, HUD off at death).

## 3. Rules the transition depends on (VERIFIED code; runtime where shown)

1. **Order inside a frame** (07 §10.1): `Ecl_Run` → move → screen check → threshold / timeout check → (switch →
   re-run all of these) → ANM → contact/damage (step 6) → death check → death sub reset → hit flash → timer tick.
   - A threshold switch therefore happens one frame **after** the damage that crossed it, and the new sub's t 0 runs in
     that same frame (runtime: damage 6564, sub38 t 0 executed in 6565, clk 1 after the frame).
   - A death-sub switch happens in the damage frame, but the death sub's t 0 runs on the **next** frame (runtime: life
     0 at 7494 with sub33 clk 0; `spellcard_end` effects at 7495).
2. **Threshold test** is `life < threshold` (strict), indices 0..3 in order, first hit wins; life is clamped up to
   the threshold. HUD life follows; max life does not.
3. **Stack depth**: `Ecl_Run` reloads the working depth from the saved main depth at entry, so the "depth := 0" of
   the threshold/timeout/death handlers is lost. Runtime: depth 1 in sub38 (entered from sub26, which was pushed by
   the interrupt), depth 2 in sub33 and sub44 (entered while sub41 / sub33 code ran at depth 2). A port that resets
   depth to 0 behaves the same for this data until a phase sub calls deep enough to hit the 15 cap earlier/later; no
   phase target sub `return`s.
4. **What a threshold switch does not do**: it does not cancel bullets, start/end a card, reset the enemy timer, the
   death sub, the variables, the interpolation slots or the interrupt table. The new sub's own ops do the visible
   work (in stage 1: `kill_all`, `flags_off 4`, `timeout` (timer := 0), op 153, `move_to`, `spellcard`).
5. **Player protection**: threshold and timeout switches on a boss set player state 3 / 70 frames if the player is in
   state 0; death mode 3 uses 90 frames; `Spell_End` (op 123) uses 70.
6. **Damage between phases**: no life loss while flags A bit 3 is clear (op 80 bit 4, 90 frames in stage 1 spells);
   afterwards op 160's timer makes boss damage d/9 for its duration (07 §10.1 step 6).
7. **Timeout** (not exercised in this run): marks the card timed out and cancels bullets (`CancelAll(4)`), it does not
   end the card; life becomes the largest active threshold; the timeout sub's own op 123 ends the card.

## 4. Data (`specs/data/q19/`)

- `fullrun_noclip_stage1_boss.txt.xz`: probe `TH08_BOSS` log, one block per calc while any boss exists, polls 3100–11000.
  `G` line: poll, pass, gm frame, rng, spell flags (0x4EA670), card bonus (+0xFC), card id, card timers (+0x108/+0x114
  current), bullet counter, live enemies, player state byte and state timer, bombing flag (0x17D6ED4), HUD boss flag
  (Hud +0x2C), boss slot 0 pointer, GameFlags. `B` line per boss enemy: slot, main sub id, ip offset in the sub, clock,
  wait, working depth (0x2CEA — after threads have run it holds the last thread's depth), life / max / HUD life, 4
  thresholds and subs, timeout and timeout sub, death sub, timer e+0x2E14, op-160 timer, flags A/B, final pos (hex),
  bonus seconds e+0x53CC, boss slot byte, pending interrupt. `T` lines: threads (slot, sub, ip, clock, wait).
- `fullrun_noclip_stage1_boss_changes.txt`: only the rows where sub/thresholds/timeout/death sub/flags or the global
  spell/HUD/player state change.
- `fullrun_noclip_stage1_transitions.txt`: full rows around each transition (5579–5582, 6562–6567, 6653–6656,
  7492–7497, 7563–7566, 8366–8369, 9344–9348, 9405–9409).

Probe: `re/probe/session.inc` `TH08_BOSS="P0 P1"`; runner `re/work/runs/q19/bossrun.sh <session> <out> <p0> <p1>`.
