# Answers to IMPL14_QUESTIONS (dirty room, 2026-10-18)

Sources: th08.exe (`re/extracted/all_decomp.c`, disassembly, byte/displacement scans) and a new wine probe run
over all 4 attract demos. The probe now logs every main-thread `Rng_Next16` call with its return-address chain
inside chosen demo-frame windows (`rngsite K F0 F1` → `probe_rng.txt`), and dumps all live bullets per frame
(`bscan K F0 F1` → `probe_bullets.txt`). Both outputs line up with the rows of `specs/data/demo*_trace.txt`
(a draw belongs to the next `R` line, which is the trace row of that frame). Draw counts below are
`Rng_Next16` calls: Next32 / NextFloat / Range / FloatRange / SignedRange each use 2.

Spec sections fixed in place: 07 §10.1 step 3, §10.3, §10.5; 08 §4.3a (new); 09 §1.1 (+5 note);
11 §3.4, §4 item 6; 13 §2.6 (type 38 row, script t=0 RNG list); 14 §3; 15 §2.9, §2.10, §3.4, load table.

## 1. Op-83 children: 10 draws every 2nd frame — VERIFIED (runtime + code)

The draws come from `Enemy_ReactPlayerMode` 0x42C420 (07 §10.1 step 3). It runs for enemies with flag A bit 8,
which ops 90–92 give every child, at the start of each enemy's update, before `Ecl_Run`.

Runtime call chain, demo 0 frame 261 (130 draws): `Enemy_ReactPlayerMode+0xBF → Effect_Spawn → AnmFile_SetScript
→ AnmFile_StartScript → AnmVm_Run → Rng_*`. That is 13 effects × (1 FloatRange + 4 Range). Draws happen on
frames 261, 263, 265 … and none on the even frames. Demo 2 frames 450/452/454 show 1 effect each, 10 draws.

Rule:
- Condition: the enemy's stored mode bit 11 is clear at entry (human state). Flags B bit 1 (op 83) is set. The
  enemy timer e+0x2E14 has "changed and is even": current ≠ previous and current % 2 == 0 (`FUN_0040ebc0(t,2)`).
  - A fresh enemy's timer is 0 with previous = −999 (template made by `EnemyMgr_Ctor` with Timer_Set), so the
    first spawn is on the enemy's first update.
  - The timer ticks at the end of the update, so the effect then repeats every 2nd frame.
  - An ECL `Timer_Set` (ops that set the time) also makes it fire if the new value is even.
- Action: `Effect_Spawn(38, e+0x2D88 final pos, 1, 0xFFFFFFFF)`. Effect 38 has no init fn. Its script (etama
  71 = entry 1 script 46) runs at spawn and draws in this order:
  1. `frand %F0, 1.0` (2 draws);
  2. four `irand n, 64` (2 draws each): alpha, then the R, G, B colour components.
  Total **10 draws**.
- Order inside the function: first the human→youkai switch check (effect 0x1F, aura interrupt 2, sfx 0x28,
  layer 0). Then the effect-38 check, which still runs on the switching frame. In youkai state there is only
  the youkai→human check (effect 0x1E, interrupt 1, sfx 0x27, layer 2) and never effect 38. At the end:
  bit 11 := player +5 & 1, e+0x3330 := 0x40 / 0x20.

Related general rule (13 §2.6): every effect type whose script uses `frand`/`irand` at t = 0 draws RNG at spawn,
**before** its init fn's draws. Types with t=0 script draws: 17/18/27: 4, 21: 4, 22: 2, 26: 6, 38: 10, 47: 8,
51/63: 10.

## 2. Bullets outside the right edge (demo 1 frame 560) — VERIFIED (runtime + code)

There is no exception: the original frees these bullets exactly as the port does. The difference is the
**counter semantics**.
- Bullet dump: 4 bullets spawned from (400, 64) (enemy off the right edge, flags 0x203) in state 2. They are
  freed by the off-screen rule on the frame their spawn effect ends: slots 70–73 on frame 550, slots 78–81 on
  frame 560.
- Not mirrored: the angles are the raw pattern angles (−0.31 … 2.96).
- `BulletMgr_Calc` 0x431240 zeroes mgr+0x6BA538 (0x160F3C8, the trace's `bullets` column). It then adds 1 for
  every slot whose state ≠ 0 **before** that bullet's update. So bullets freed during the frame are counted on
  that frame (frame 560: 19 survivors + 4 freed = 23).
- The same counter feeds the spawn limit (≥ 1536) and slow mode, so the port should reproduce it exactly (08
  §4.3a). It is not reset while bullets are frozen (GameFlags bit 10, the whole calc is skipped). Lasers are
  not counted.

## 3. Demo 3 frame 795: 4 extra draws on a kill — VERIFIED (runtime + code)

The 4 draws are the **time orb**: `ItemMgr_Spawn(final pos, 7, 1)` at EnemyMgr_Calc+0x13F9. They are spent only
when `Gauge_IsHumanMax` (gauge ≤ GM+0x3DDFC) or `Gauge_IsYoukaiMax` (gauge ≥ GM+0x3DDFE). Type 7 forces mode 3:
`Rng_FloatRange(0.2)`, then `Rng_SignedRange(0.6)` = 4 draws (08 §8.3).

Exact 40-draw order on that frame:

| # | draws | source |
|---|---|---|
| 1 | 8 | 2 hit sparks from `Player_DamageAgainst` (2 × EffInit_Spark, 4 each) |
| 2 | 12 | `Enemy_DropItems`: `Effect_Spawn(e+0x3311+4, pos, 3)` = 3 sparks (the item itself is mode 0/1: no RNG) |
| 3 | 16 | death sparks `Effect_Spawn(e+0x3311+4, pos, 4)` (EnemyMgr_Calc+0x13C5); the preceding `Effect_Spawn(e+0x3310, pos, 1)` spawns a type without RNG |
| 4 | 4 | time orb (above) |

The port's "3 extra items × 2 floats" (12) matches step 2 only by count. In this kill the 12 draws are the 3 drop
sparks, and e+0x330C/+0x3308 are 0. Check that the port draws the drop sparks inside the drop step, before the
death sparks.

## 4. Enemy-death gauge change — VERIFIED (0x42D65C)

`GameMgr_GaugeAdd(d, forced = 0)`:
- d = **−200 when player byte +3 (0x17D5EFB, the immediate focus byte) == 0**, else +200. This is not the +5
  mode byte. It differs from +5 for the first 6 frames after a focus change, and +3 = 2 right after load
  counts as youkai.
- Not forced, so it is skipped completely while bombing (0x17D6ED4 ≠ 0).
- Clamped to [GM+0x3DDF8, GM+0x3DDFA]; then stats+0x20 := stats+0x22.

Nothing else happens in that step. It comes after the chain-kill of the children and before the death-mode switch.

## 5. Leaving options (state 3) — VERIFIED (all 7 calc fns)

The port's fit is right: in state 3 the calc fn does no movement and no effect 0x2F.
- It only sends interrupt 5 at t == 0, and frees the option when **t > 16** (`FUN_004066c0` = "greater than";
  14 §3 said ≥ 16).
- Freeing sets state 0 and clears the calc and draw fns.
- Exception: **Alice** (0x44EA40) still sets pos = player − (0, 32) every leaving frame.
- The generic loop in `Player_Move` still runs the option VM (`AnmVm_Run`) and ticks the option timer after the
  calc fn, also on the freeing frame.
- The state is entered from `Player_Move` (state ≠ 0 and ≠ 3 → 3, timer Set(0)).

## 6. Replay stage load: score — VERIFIED (0x452D60)

Score (stats+8) and displayed score (stats+0) := +0 of the **last existing record with a lower stage index**.
The loop k = 0 … stage−1 keeps the last non-null record pointer.
- If there is none (first recorded stage), the score is not written and keeps the fresh-start 0 (the StageStart
  fresh branch zeroes stats+0/+8).
- The port's "previous recorded stage" is right as long as it takes the latest present lower record (gaps
  allowed).
- Side note: the replay header's config copy (+0xB4, 60 bytes) is copied over the cfg copy at load. So the
  slow-mode byte during playback is the recorded one; StageStart's own zeroing of +0x25 runs before it.

## 7. Child aura interrupt — VERIFIED (Ecl_Run ops 0x5A/0x5B/0x5C)

- **Interrupt 1 when player +5 == 0 (human), 2 when youkai.**
- Layer 2 (human) / 0 (youkai).
- The aura is created only if e+0x53C8 == 0. It sits at e+0x2D34 (op 92: e+0x2D88).
- VM flag bit 17 := flags A bit 2 (0).
- VM +0x14 is negated if e+0x2E0C bit 0.
- `Enemy_ReactPlayerMode` later sends 2 on human→youkai and 1 on youkai→human.

## 8. Attract demo end — VERIFIED (0x43A3FA, 0x43C5E1, MainMenu_Top 0x467AED)

- At 6000/4800/4920/6900 frames: `ScreenEffect_Register(2, p0 = 120 frames, colour 0 (black), 0, 0, prio 0x15)`,
  then `Music_Fade(3.0)`.
  - The fade is **3 s**, not 2. In WAV mode it is divided by the game speed when 0 < speed ≤ 1.
  - The port's 120 frames to black is right.
- GM+0x3DBB4 at boot = **3** (GameManager static constructor). MainMenu_Top does n = (n+1) % 4 before loading
  `demorpy<n>`, so the first demo is demorpy0 as the port does.

## 9. PLST counters and play time — VERIFIED (StageStart 0x43B05C…, 0x448418)

The counters change only in the fresh-start branch. They are skipped in replay (bit 3), spell practice
(bit 14) and with the cfg-copy slow mode (+0x25) on. +0x25 is forced to 0 for Extra, stage practice and
spell practice.
- Every counted start, **including stage practice and retries**: [0] and [1+shot] in the difficulty block and
  in block 6.
- Retry (requested scene == 10): additionally [13] (diff + block 6). So a retry counts as a play too.
- Stage practice (bit 0, not bit 14): additionally [16].
- All counters cap at 999 999.
- **The port is wrong for practice**: it must also count [0]/[1+shot] there.

Play time = wall-clock ms (`timeGetTime`) accumulated into PLST +0x1C:
- The mark (Supervisor+0x1AC) is set at the top of every StageStart, and when the pause menu or the continue
  menu is left.
- Time is added on pause entry, on continue-menu entry, and at `GameMgr_OnRemove` unless replay. The mark is
  then zeroed.
- Pause and continue menus are excluded; replay/demo time is not added at removal.

## 10. VRSM — VERIFIED

Keeping the loaded values is fine: on load the check passes whenever no `th08_*.ver` file exists.

For a fresh score.dat (and the replay VRSM chunk), the original writes:
- the tag "0100d" (0x4B660C);
- size **840704** and sum **0xA26861B9**, computed at boot by 0x443730 over the running th08.exe (Σ of u32
  words 0 … size/4−2).

These values match both the shipped exe and the VRSM in an original-written score.dat. A port can use them as
constants (15 §2.10).

## 11. GM flags bits 15–17 during playback — VERIFIED

Only `Supervisor_OnRegister` (boot, from the loaded score.dat) and `MainMenu_OnRegister` write bits 15–17 (all
`g_GameFlags` stores checked). No game, replay or demo path clears them. The `flags` column 0x1000E (bit 16)
in the traces confirms this. Keep them.

## Uncertain / not checked
- Q6: that a fresh playback start leaves the score at exactly 0 rests on StageStart's fresh-branch zeroing
  (read in code, not at runtime).
- Q1: the effect-38 visuals (sprite 219 etc.) come from the script dump only and were not compared on screen.
