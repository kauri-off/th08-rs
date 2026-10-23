# RE-Q31 — fullrun from 90010, IMPL78–85 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data `re/work/runs/q31/` (not in git):
- `portfix/`: scratch copy of master src at 1a2cbe5 (+ impl83 WIP) with fix A (`// q31 A`); `portfix2/`: master src after
  impl84/85 (+ impl86 WIP) with fix A and the debug bullet dump (env `Q30_BUL`, not for `src/`).
  `PF=<dir> ./run_all.sh <tag>` runs golden / fullrun_noclip / selftest1 / demos 0–3 → `<session>_<tag>.log`, `demo<n>_<tag>.txt`.
- `fr_scan98/` (polls 98547–98553), `fr_scan988/` (98803–98809), `fr_scan1138/` (113828–113833): partial replays of the original (`TH08_SCAN`, `TH08_SCANBUL`,
  `TH08_RNGSITE`); `fr_p98*`, `fr_p988*`: port state dumps (`TH08_SESSION_DUMP`).

## 1. Question 1 — fullrun_noclip divergences

Start (tag `base`, master 1a2cbe5): golden full, selftest1 full, demos 0–3 in sync, fullrun 90009. RE-Q30 M (`Vec3_FromPolar`)
and T (f32 box edges) applied on scratch: no change.

| divergence | cause | label | spec § | after the fix (scratch, cumulative) |
|---|---|---|---|---|
| fullrun 90010 (graze +1, score +200) | slot 163 (ecldata7 sub6 ring) grazed one frame early: its familiar's angle e+0x2D94 was 2 ulp off because enemy move mode 3 (orbit) computed the offset as f32(cos)·r; the original calls `Vec3_FromPolar` (single rounding) (A) | SPEC FIXED + CODE BUG | 07 §8 movement table mode 3, 05 §10 | fullrun 98550 |
| fullrun 98551 (enemies 0x15/0x13) | stage-6B familiars (ecldata7 sub33, op 92 children of sub31) at final pos = boss pos + boss pos + offset in the old scratch (op 92 spawn position); the original spawns op 92 at (x, y) as given (helper 0x41F110, same as op 90) and e+0x2D34 stays the offset. Already right on master since impl84 (`portfix2`, tag A2) | (fixed on master) | 07 §10.5 | fullrun 98806 |
| (checked at 98806) | trig rounding sites (05 §10 table): mode 1, time orbs, ops 38/166/66/67, Reimu orbs single-rounding (B); no session change | SPEC FIXED + CODE BUG | 05 §10 | fullrun 98806 |
| fullrun 98807 (bullets 237/238) | bullet 706 (ecldata7 sub32 grid shot) 16 px right of the original: sub32 computes `$10042 + 16 + rand·32`, `iset` + `idiv 16` + `imul 16`; var 10042–10044 as a **source** reads the final pos e+0x2D88 (`Ecl_GetFloatArg`/`GetIntArg`), as a destination the pos e+0x2D34 (`Ecl_FloatVarRef`); the familiars (sub33) are op-92 followers, so the port read their follow offset (negative x → truncation toward zero lands one grid cell off) (C) | SPEC FIXED + CODE BUG | 07 §6 var table | fullrun 113830 |
| fullrun 113831 (boss 0x0/0x1) | the 6B Eiya-gaeshi card (sub77) times out into sub45, whose `spellcard_end` is followed by `ijne $10099, 0, t=80` → `boss −1`; var 10099 is *card active ? spell flag bit 2 (capture possible) : bit 9 (last card captured)*, so after `Spell_End` of a captured card it reads 1 and the jump unregisters the boss in the same frame; the port read "a card is active" (0) (D) | SPEC FIXED + CODE BUG | 07 §6 var 10099 | fullrun 113960 |
| fullrun 113961 (score) | capture payout 34 904 000 vs 33 000 000: op 155's "global 0x4ECCA8" is g_Gui +0x2638, the card bonus cap; op 155 follows op 122 in sub77, so it raises the cap of the running card to 99 999 990 and time orbs push the bonus past the declared value. The port kept the cap at the card bonus (it also initialised +0xFC := cap instead of the card bonus) (E) | SPEC FIXED + CODE BUG | 07 op 155, §11.1 Spell_Start | fullrun 120057 |
| fullrun 120058 (rng +8) | sub44 (timeout sub of sub87) runs `native 18, 4` at time 0; the ECL clock tick / wait decrement read the **live** game speed (`Timer_Tick` → g_GameSpeed), so the clock is 0.25 after that frame and the time-1 `effect_dir` (2 + 6 RNG draws) runs 4 frames later; the port cached the speed at `run_thread` entry and ticked by 1.0 (F) | SPEC FIXED + CODE BUG | 07 §5.2 | **fullrun FULL (124 707)** |

Tag A2 (`portfix2` = current master + A): golden full, selftest1 full, demos 0–3 in sync, fullrun 98806. Tag B (+B): same. Tag C (+C): fullrun 113830. Tag D: 113960. Tag E: 120057. **Tag F (A–F): golden full (25 634), fullrun_noclip FULL (124 707), selftest1 full (1 922), demos 0–3 in sync.** Scratch diff B–F: `re/work/runs/q31/q31_fixes.diff` (against `portfix2_src_A`, which already contains A: `// q31 A` in `enemy.rs`).

## 2. Questions 2 — IMPL78 / IMPL80 / IMPL83 / IMPL84 / IMPL85

| question | answer | label | spec § |
|---|---|---|---|
| IMPL78 Q1 op 128 `d` | all four words (a, b, c, d) are copied raw — no `Ecl_GetIntArg`/`GetFloatArg`, mask bits ignored; d is stored as its bit pattern in the f32 e+0x53C4; the effect is spawned at pos e+0x2D34, not the final pos. The port resolves a, b, c via the mask and spawns/centres at the final pos | SPEC FIXED + CODE BUG | 07 op 128, §4 0x53C4, 13 §2.6b |
| IMPL80 Q1 native 8 member context | `Ecl_Run` resets e+0x2CA0 to the main context before returning, so outside its own run a member's current context is its main one | NEW FACT | 07 §4 +0x2CA0, §12 native 8 |
| IMPL80 Q2 native 8 no leader | not tested by the code (division by f32(0), uninitialised leader pointer: UB); unreachable in the shipped data | NEW FACT | 07 §12 native 8 |
| RE-Q30 open: `Enemy_ChainUnlink` of a root | the first child keeps its +4 (already in the spec); master matches | — | 07 §10.3a |
| IMPL83 Q1 which sites are `Vec3_FromPolar` | byte scan of all trig sites: `Math_Cos`/`Math_Sin` return the unrounded x87 value, so `call; fmul` sites round once like `Vec3_FromPolar`; sites that `fstp` the result first round cos/sin to f32. Ops 38 / 166 / 66 / 67, mode 1 / 3, kill-children orbs, Reimu/Sakuya bombs = single rounding; laser draw / laser cancel (`Math_SinCosF32` 0x433880), `Vec2_Rotate`, effect burst / converge directions, op-128 effect update = f32 first. Full table in 05 §10 | SPEC FIXED + CODE BUG (ops 38/166/66/67, time orbs, Reimu bomb, if the port rounds cos first) | 05 §10, 07 ops 38/66/166, §10.3a, 08 §6.3, 13 §2 |
| IMPL83 Q2 other writers of +0x38C / +0x3A4 | none: only `Player_Move` (and the zeroing at registration); in states 1/2 the boxes keep the last moved frame's values | NEW FACT (port right) | 09 §3 |
| IMPL84 Q1 op 92 spawn position | (x, y) as given (op 90 helper 0x41F110); only op 91 adds the parent's final pos (0x41F280). Runtime: fullrun row 98551, e+0x2D34 = (5.85, 0) | SPEC FIXED (port right) | 07 §10.5, ops 90–92 row |
| IMPL84 Q2 op 92 final pos z | `Vec3_Add` of all three components: z = parent pos z + own z (port right) | NEW FACT | 07 §10.5 |
| IMPL85 Q1 op 86 with an empty boss entry | no existence test: a variable operand is read through a null enemy pointer (access violation); a literal operand is copied without reading the slot. 0 uses | NEW FACT | 07 ops 86/87 row |
| IMPL85 Q2 follower order | the aura VM pos is written inside the step-4 loop right after the final pos (also on a screen-free frame and on each phase-change pass); the step-7 "effect followers" are only the op-128 effects (`Enemy_UpdateOrbitEffects`), after the boss HUD/marker and before the timer tick; a boss mode 0/1 death has released them (count 0), so they get no update that frame. Observable in drawing only (draw log), not in gameplay | SPEC FIXED + CODE BUG (draw-only) | 07 §10.1 steps 4, 7 |
| IMPL85 Q3 tail after a mode-0 death | not skipped: the mode-0 case falls through to the drops and leaves the switch normally; the whole tail runs, only the link tests alive (port right) | NEW FACT | 07 §10.1 step 7 |

## 3. CODE BUG list (for the implementer)

1. **A** enemy move mode 3: orbit offset = `Vec3_FromPolar`(orbit angle, r), i.e. f32(f64 cos·r) (07 §8, 05 §10). Scratch-confirmed: fullrun 90009 → 98550.
2. **op 128** raw words: a, b, c, d copied raw (no var resolution, d as f32 bits), effect spawned at / centred on e+0x2D34 (07 op 128, 13 §2.6b). Being implemented as impl86.
3. **C** var 10042–10044 read (source operand, float or int) = final pos e+0x2D88 (z 0); destination / in-place = pos e+0x2D34 (07 §6). Scratch-confirmed: fullrun 98806 → 113830 (`eclvm.rs` `var_f` read `e.pos`).
4. **D** var 10099 = card active ? spell flag bit 2 : spell flag bit 9 (captured) (07 §6). Scratch: 113830 → 113960 (`eclvm.rs` `var_f` 10099).
5. **E** op 155 always sets the running card's cap +0x2638 := 99 999 990 (07 op 155); `Spell_Start` sets +0xFC := card bonus, cap := card bonus or 99 999 990 if bit 27 is already set (07 §11.1) — the port sets +0xFC := cap. Scratch: 113960 → 120057 (`eclvm.rs` op 122 / op 155).
6. **F** ECL clock tick / wait decrement use the live game speed (07 §5.2); the port caches `gs` at `run_thread` entry. Scratch: 120057 → full (`eclvm.rs` `run_thread`). The same caching exists for the autofire timer and the interpolation slots (`eclvm.rs` POST helpers) — differs only if native 18 / 28 / 29 runs earlier in the same `Ecl_Run`; whether those original timers also read the speed live is INFERRED (not checked; no session case).
7. **B** Trig rounding (05 §10 trig-site table), all still f32(f32(cos)·r) on master: enemy move mode 1 velocity (`enemy.rs` mode 1 — `Vec3_FromPolar`, not covered by impl83), kill-children time orbs (`enemy.rs` chain kill — `Vec3_FromPolar`), ops 38 / 166 (`eclvm.rs` — f32(cos·r)), op 66 / 67 delta (`eclvm.rs` `move_dir` — f32(f32(cos·spd)·dur); the port also multiplies spd·dur first), Reimu bomb orbs (`bomb.rs` — f32(f32(sin·r) + cx)). Laser draw / cancel, `Vec2_Rotate` boxes and effect burst/converge directions must keep f32 cos/sin. Spec-only (no session change seen).
8. Aura VM position written in step 4 (each pass, before the screen check); op-128 effect update moved to the step-7 tail after the boss marker (none after a boss mode-0/1 release) (07 §10.1). Draw-only.
