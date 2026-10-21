# RE-Q29 — replay frame hook vs traces, next divergences, IMPL35–40 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data: `re/work/runs/q29/` (not in git):
- `portfix/`: scratch copy of `src/` (master 896927b = f50761b src) with PR #29 applied (supervisor hunk merged by hand) and
  the fixes below, each marked `// q29`. `portfix_src_base/` = master src, `q29_fixes_I.diff` = all fixes, `run_all.sh <tag>` runs
  golden / fullrun_noclip / selftest1 / the 4 demos; `<session>_<tag>.log`, `demo<n>_<tag>.txt` = results per fix tag.
- `golden_scan/`, `fr_scan*/`: partial replays of the original (`TH08_SCAN`, `TH08_SCANBUL`, `TH08_RNGSITE`);
  `demo3_orig/`: attract demo 3 probe run (bscan, iscan, rngsite 6678–6683). Work game files restored (md5 checked).

## 1. Question 1 — `Replay_FrameHook` vs the traces (blocks PR #29)

| question | answer | label | spec § |
|---|---|---|---|
| when is the hook linked / run | `Replay_Register` (from `StageStart`, after `Gui_Register`) creates the node only together with the replay object (first stage of a game); recording: always; playback: only with header +6 ≠ 0 (never in recorder-written files: +6 := 0). Also called once directly at linking. No gate of its own: runs every frame the chain reaches prio 7 | SPEC FIXED (detail) | 11 §1.4, 15 §3.1 |
| what it writes | +0xDA := 0; +0xD8 := RNG state; g_Rng+4 := 0; if GM+0x3DB98 (0x164D0A0, set only by the pause entry 0x43A153): +0xDA \|= 0x100, then GM+0x3DB98 := 0. **No reader** of g_Rng+4 (only `Rng_Next16` ++ and `Rng_ResetCounter`), +0xD8 or +0xDA: the hook is behaviourally inert | NEW FACT | 11 §1.1 row +4, §1.4; 15 §3.1 object table |
| why the traces never reset | the trace column `rng_ctr` is **not** g_Rng+4: it is the probe's own counter of all `Rng_Next16` calls since process start (`hook.c` `cb_rng`, `g_rng_calls`; already documented in specs/data/drawlog/README.md). The port's `demosync::sample` and `bin/demosync` read `Rng::counter`, which PR #29 now zeroes every frame → 722 / 240 / 614 | CODE BUG (PR #29) | 11 §1.4 |
| IMPL73 Q1 debug playback | `Replay_PlayCalcDebug` 0x4526C0 = PlayCalc except: cursor +0x78 (:= +0x50 = record+0x24 at every stage start, also used by the recorder for the dummy frame) and stride 6. Flag word and RNG word are **never read**; the RNG is not forced. Same gates, repeat emulation, fps read, frame += 1 | SPEC FIXED (15 said "RNG forced") + CODE BUG (PR #29 forces it) | 15 §3.1 |
| IMPL73 Q2 hook gate / +0xD8 reader | none / none | NEW FACT | 11 §1.4 |
| IMPL73 Q3 slow mode | counter GM+0x3DE08, shared with the step-5 playback-speed skip, never reset except n < 128 → 0 (only other writer: GM ctor). Step 10 runs after the score roll; gm+0x2D := 0 at the top of step 10 every frame that reaches it, by `StageStart` and `GameMgr_OnRemove`; skip → gm+0x2D := 1, return 3 (no `IsTampered`, no frame_counter++). Step 4 reads gm+0x2D through `this` (ESC blocked on the frame after a skip); `Fps_Update` does not count flagged frames. Step 5 detailed (dialogue exception, exact pattern, return 3) | SPEC FIXED + NEW FACT; CODE BUG (step 10 not implemented) | 11 §4 steps 5, 10 |
| IMPL73 Q4 OR-accumulation | only the raw word \|= Input_Poll(); previous word, repeat flag and repeat counter untouched (the port is right) | NEW FACT | 11 §7 |

Scratch confirmation: PR #29 alone → golden 722, fullrun 240, selftest1 614 (rng_ctr 0x52/0). PR #29 + a never-reset
`Rng::calls` for the trace column + no RNG forcing → golden 10782, fullrun 9408, selftest1 full, demos unchanged (= master).

## 2. Question 2 — next divergences

| divergence | cause | label | spec § | after the fix (scratch, cumulative) |
|---|---|---|---|---|
| golden 10782 (score) | `Hud_Calc` = `Hud_Update` (bonus, trigger, clock animation, VMs, fade, popup timers) **then** `Msg_Run`: the op-9 bonus is added on the next frame; the port ran the dialogue first (A) | SPEC FIXED (order was unstated) + CODE BUG | 12 §5.1b (new), §5.6 | golden 10902 |
| golden 10902 (flags bit 6) | clear trigger is tested **before** the clock step and has no 60-frame gate (fires the frame after `shown` reaches `target`; immediately if the clock did not move) (B) | SPEC FIXED + CODE BUG | 12 §5.6 | golden 11911 |
| golden 11911 (rank 16/15) | rank sub-counter GM+0x3DE38 (= 0x164D340) := 0 at **every** stage start; `GameMgr_InitRank` sets only rank/min/max. The port zeroes it only on fresh starts (C) | SPEC FIXED (named) + CODE BUG | 11 §3, §3.1 | golden 23298 |
| fullrun 9408 (flags bit 7) | the port keeps GameFlags bits 7/8/13 in `Player::gflags`, never in the flags word, so the word (trace column, any reader of `game.flags`) lacks them (D) | CODE BUG | 11 §2.1 | fullrun 11251 |
| fullrun 11251 (flags bit 6) | msg op 11 sets the trigger only for stage idx 6/7/8; stage 1–5 scripts (msg1* use op 11 after op 9) only stall (E) | SPEC FIXED (clarified) + CODE BUG | 10 §5 op 11 | fullrun 16164 |
| (none; found while chasing 23298) | `Timer_Sub` slow path (speed ≤ 0.99) sets previous := current; the fast path leaves previous alone (F) | SPEC FIXED + CODE BUG | 05 §2 | no change in the sessions |
| golden 23298 (+16 draws), demo 2 4660 (+40 draws) | `EnemyMgr_Calc` step 6: (a) whole step skipped when flags A bit 31 is set and the player is bombing; (b) the first `Player_DamageAgainst` call is not made while a card is active, the enemy has a parent (e+0x2DA4) and the player is bombing (G). (b) alone produces the sync; (a) is verified in code only, the sessions do not exercise it | SPEC FIXED + CODE BUG | 07 §10.1 step 6 | golden **full match** (25 634 rows), demo 2 **in sync** |
| fullrun 16164 (score) | item collection compares **f32 box edges** (item ± C/2 vs the player box stored by `Player_Move`), not \|d\| ≤ sum of halves: x 127.99999 + 12 rounds to 140.0 (H) | SPEC FIXED + CODE BUG | 08 §8.4 | fullrun 29948 |
| demo 3 6682 (−4 draws) | `ScreenEffect_Register` takes the duration in **EDX** (decompiler hides it): Sakuya shake (1: 120, 4, 1), Sakuya LS (1: 50, 4, 1), Alice / Alice LS shakes (1: 24, 8, 0); spec and port had (4, 1, 0) / 16 (I) | SPEC FIXED + CODE BUG | 14 header note, §2.3, §2.4, §2.7, §2.8 | demo 3 **in sync** |

## 3. Question 3 — IMPL35–40 open questions

Skipped as already answered by RE-Q27/Q28 or this run: IMPL35 Q3, IMPL36, IMPL37 Q1, IMPL39 Q1 (bonus timing, §2 A).

| question | answer | label | spec § |
|---|---|---|---|
| IMPL35 Q1 rate +0x100 with bit 27 | from the card's own bonus (bit 27 only raises the cap +0x2638 to 99 999 990) — the spec already said so; port right | — | 07 §11.1 |
| IMPL35 Q2 `Spell_AddBonus` without a card | tests only bit 11: +0xFC / +0x100 change also with bit 0 clear | NEW FACT | 07 §11.1 |
| IMPL37 Q2 boss table | holds enemy pointers; no spawn path clears it, so a new enemy in the freed slot is seen as the boss (slot-index model is exact; port right) | NEW FACT | 07 §10.3 death-mode row |
| IMPL38 Q1 colour slot alpha | colour slots lerp R, G, B only; alpha only by alpha slots. `Spell_End`'s 0x20 lands in the alpha slot's end byte, unused (port right) | NEW FACT | 05 §6, 07 §11.1 |
| IMPL38 Q2 g_Gui +0xC | written only by `Spell_Start` := boss e+0x2E0C (port right) | NEW FACT | 07 §11.1 |
| IMPL38 Q3 steps 1.2–1.7 after 1.1 | yes, same call (port right) | NEW FACT | 07 §11.1 |
| IMPL39 Q1b +0x22D88 reset | only on fresh starts (impl block reallocated + zero-filled); continuing stages keep the previous total | NEW FACT | 12 §5.6 |
| IMPL39 Q2 title y for idx > 6 | the pen moves up 16 for the whole panel ("Clear" at 112) | SPEC FIXED + CODE BUG (port moves only the title) | 12 §5.6 |
| IMPL39 Q3 lives / bombs / clock | live every draw (port right); "Last Time" stored | NEW FACT | 12 §5.6 |
| IMPL39 Q4 stage-clear VM draw | last in `Hud_DrawBoss`, unguarded, `Draw2DSnap` | NEW FACT | 12 §5.4 |
| IMPL39 Q5 f = 0 | `idiv` fault, unreachable in data | NEW FACT | 12 §5.6 |
| IMPL39 Q6 `Hud_DrawBoss` guard | covers bar, markers, label, gauge, timer, chain popup; titles / clocks / spell-name / transition VMs and the stage-clear VM are outside (port right) | SPEC FIXED (§5.4 title wording) | 12 §5.4 |
| IMPL39 Q7 dirty counters | not answered (low priority, back-buffer only) | — | — |
| IMPL40 Q1 capture VMs | `capture.anm` script 0, pos := (32, 16, 0) after `SetScript`, never interrupted (port right) | NEW FACT | 12 §3 |
| IMPL40 Q2 spell bit 9 | set by `Spell_End` (captured), cleared by `Spell_Start` and by the `Gui_LoadFaces` zero-fill of the whole spell object at every stage start → does not survive a retry | NEW FACT | 07 §11.1 |
| IMPL40 Q3 night-clock fade | after the VM run, playfield coordinates | NEW FACT | 12 §5.7 |
| IMPL40 Q4 capture frame | copied after `Present` of the same `Frame_Run`; COPY swap effects (windowed, every recorded session) → frame N; FLIP → N−1 (INFERRED). 12 §3 "frame presented just before the request" corrected | SPEC FIXED | 12 §3 |
| IMPL40 Q5 0x17CE8FC bit 1 | set unconditionally by `Window_InitD3D`, never cleared (port right) | NEW FACT | 12 §3 |
| IMPL40 Q6 continue menu first frame | game_over set at prio 9/11 → ASCII sees it the next frame; first `ContinueMenu_Update` does state 0 and falls into state 2/1 in the same call; capture VM at script time 2 on the first drawn frame. The port opens one calc later | NEW FACT + CODE BUG | 12 §4 |
