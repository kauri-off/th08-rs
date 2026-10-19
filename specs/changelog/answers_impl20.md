# Answers to IMPL20 questions (RE-Q20)

Spec sections changed:
- 07 §6 var 10098: full definition (a, b, c, d; comparison is ≥).
- 07 §7 op 36: the target var id is stored as a float.
- 07 §10.3: the death-mode table ("0–1" row: what "if boss" clears; nothing clears the boss table at death); the
  death-latch paragraph (the single place that clears it, and when that check runs).
- 11 stats table +0x3C (time counter: reset and all writers); 11 GameManager table freeze byte 0x160F534 (writers).

Runtime data in `specs/data/q20/`, all from `fullrun_noclip` replayed in the original with 0 trace differences:
- `fullrun_noclip_boss_9000-25500.txt.xz`: boss log. The new `L` line has time, threshold, pending reward, orb count,
  difficulty and stage. The new `C` line has the item search cursor and whether its slot is busy.
- `fullrun_noclip_lastspell_decision_rows.txt`: rows 9400–9410 and 25478–25490.
- `fullrun_noclip_stage_time_s3C.txt`: row, stage, stats +0x3C.
- `fullrun_noclip_scan_6263-6265.txt`: enemies and items.
- `fullrun_noclip_rngsites_6263-6265.txt`: RNG call sites per row. These come from the new session-mode
  `TH08_RNGSITE="P0 P1"`.

## Q1. Var 10098 — VERIFIED (code + runtime)

Value = **2 if a + b + c ≥ d, else 0** (`Ecl_GetIntArg` case 10098). The old spec had the comparison backwards.

| | what | where | reset / writers |
|---|---|---|---|
| a | stage time | stats +0x3C (`GameMgr_GetStatsTime`) | Set to 0 at every stage start. `GameMgr_AddTime` changes it: +1 per time orb collected (type 7). A card capture adds its time reward, paid 130 frames after `Spell_End` by `Spell_Calc` or at the next `Spell_Start`. −15 per frame of the death-bomb window. At an actual death: −time/10 if time ≤ 5000, else −500. A negative add larger than the current value clamps to 0. |
| b | captured card's time reward not yet paid | spell object g_Gui +0x10 | Set by `Spell_End` on a capture: 700 with flag A bit 27, else from the remaining card time (07 §11.1). Moved into a and zeroed at payout. |
| c | number of time-orb items (type 7) alive in the item list | `ItemMgr` list walk (0x4417E0) | Live count. Each orb is worth +1 time when collected, so c counts uncollected time. |
| d | stage time threshold | stats +0x40 (`GameMgr_GetTimeThreshold`) | Set at stage start to `TIME_THR`[stage][difficulty] (11 §3.1; 0 in spell practice). The same value drives the clock advance (+2 below / +1 at or above) and the death-bomb window length (+7 frames when time ≥ threshold). |

Misses and bombs only enter through a (death penalties, death-bomb drain). There is no direct miss or bomb counter.

Runtime, fullrun_noclip, difficulty 1 (Normal). The test runs when the Last Spell host enemy is spawned. Values are
read after the calc, which is close enough given the margins.

| boss | row | a | b | c | sum | d | result |
|---|---|---|---|---|---|---|---|
| stage 1 | 9408 | 2062 | 622 (spell 2 had just been captured) | 1 | 2685 | 2500 | 2 → Last Spell |
| stage 2 | 25485 (last row before the decision) | 5304 | 0 | 512 | 5816 | 7200 | 0 → host deletes itself |

- Stage 2 is skipped because its threshold is much higher (Normal: 7200) and the run collected too little time.
  Even a maximum capture reward of 1000 at 25486 would leave the sum below 7200.
- The trace's stage-time column (stats +0x3C) at the other decision rows is 9563 (stage 3, threshold 8500), 10745
  (stage 4B, 8500) and 11421 (stage 5, 9999). All are above their thresholds already, consistent with the Last Spells
  that ran.

## Q2. Boss death in modes 1/2 — CONFIRMED, with one correction

- No death mode clears the boss table entry (0xF54CC0[slot]) or flags A bit 1. Only these do:
  - op 127 with −1;
  - `Enemy_Free`: op 1 delete, the off-screen free, and `Ecl_Run` returning −1.
- **Mode 0 also does not unregister.** It clears only the alive bit and does not call `Enemy_Free`, so the boss table
  keeps pointing at the dead slot. The boss trace column stays set. Timeline op 10 (`wait_boss`) still stops waiting,
  because it tests the alive bit too.
- The "if boss" part of the death code:
  - **Modes 0 and 1:** HUD boss display byte Hud +0x2C := 0 (the boss bar hides) and the op 128 effects are released.
    The port should still clear the HUD byte in modes 0 and 1. Runtime: at row 3862 (midboss, mode 1) the HUD flag
    goes 1 → 0 while boss slot 0 stays set.
  - **Modes 0–2:** if no card is active, `BulletMgr_CancelBonus(8000, 1)` + `EnemyMgr_KillAll(8000)` → score +
    `Hud_ShowBonus`.
  - **Mode 2:** keeps the HUD byte. Runtime: row 7494, the stage 1 boss.
  - **Mode 3:** also turns the HUD byte off (07 §10.3).

## Q3. What clears the death latch — VERIFIED (code)

- There is exactly one place, in `EnemyMgr_Calc` right after the damage step (step 6) and just before the death test:
  if flags B bit 3 is set and life > 0, the bit is cleared.
- Any way of making life positive therefore clears it on the next pass of that point:
  - op 131 `set_life`;
  - a write to var 10051 (e.g. stage 1 sub50 `iset $10051, 1`);
  - a threshold or timeout clamp;
  - mode 3's life := 1.
- The check only runs when the enemy's script block runs. It is skipped for:
  - chain-killed enemies (flags A bit 10);
  - enemies paused by flags B bit 7;
  - enemies held by the flags A bit 30 bomb/state condition.
- Clearing in op 131 is equivalent for every case in the data where op 131 sets a positive life. It differs only when
  life becomes positive another way. Clearing on the death-sub jump is wrong: life is still 0 there, so the next frame
  dies again.

## Q4. Enemy screen-check box — CONFIRMED (code; runtime in answers_impl19 §Q2)

- The box is the main VM's current sprite: width = sprite +0x34, height = sprite +0x30 (logical px; 03_images, structs.md), centred on the final position, inclusive tests.
- **The VM scale is not applied**: the call passes the sprite record's fields directly, and rotation is ignored too.
- If the VM has no sprite (e+0x230 = 0), flags A bit 4 is set and the enemy is never checked.

## Q5. Single chain-child kill at row 6264 — the orb IS spawned; the extra draws are a hit spark

Original RNG call sites at row 6264, 32 draws in total (`fullrun_noclip_rngsites_6263-6265.txt`):
1. **3 hit sparks** (`EffInit_Spark` via `Player_DamageAgainst`, 2+2 draws each) = 12;
2. **the single-kill time orb**: `Enemy_KillChildren` → `ItemMgr_Spawn(pos, 7, 1)`, forced mode 3,
   `Rng_FloatRange` + `Rng_SignedRange` = 4. The item appears as slot 1151, type 7, state 3, at the dead child's
   position. The item cursor moves 1151 → 1152;
3. **4 death sparks** (`Effect_Spawn(e+0x3311+4, pos, 4)` from the death path) = 16.

So the port's orb is right. Its 36 must contain **4 hit sparks** instead of 3: one extra `Player_DamageAgainst` spark
this frame. Check:
- the spark gate, per overlapping bullet with +0x3C ≠ 0 and life % period == 0, if +0x3D == 0: u8 counter
  player +0xE2A94 += 1, and spawn when (counter & 3) == 0 (TOOLING_REPORT §2);
- the counter's value carried in from earlier frames;
- the set of bullets overlapping each enemy this frame.

The death path's own "gauge at max → time orb" did not apply. The single-kill path first does
gauge −= gauge/12 (29 → 27), then −200 → −173, so the gauge is not at either end.

## Notes

- **Op 36 target:** correct. The slot keeps the raw dword and compares it as a float, so it is a float-encoded var id.
  07 §7 updated.
- **Dialogue state read live:** the conclusion is right, but the priority is not. The dialogue (`Msg_Run`) runs inside
  `Hud_Calc` (calc prio 15), not in a prio-12 dialogue calc. Prio 12 is `Gui_Calc`, which only runs `Spell_Calc` and
  the face/spell VMs. Dialogue updates therefore happen after the enemies (11), effects (13) and bullets/items (14) of
  the same frame.
- **`wait_boss` (timeline op 10):** correct. It waits while boss[slot] ≠ 0 and its alive bit is set (it decrements its
  clock to hold).
- **Freeze byte 0x160F534:** correct.
  - Writers: only ECL native 26 sets it (= arg). `Player_Die` and `GameManager_StageStart` clear it. The 11 table said
    "set during player death", now fixed.
  - Readers: `EnemyMgr_Calc` (timer tick), `BulletMgr_Calc` (per bullet), `Hud_Calc`, `Player_Calc`, `Spell_Calc` and
    the two screen-effect calcs.
