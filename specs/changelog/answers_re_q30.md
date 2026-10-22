# RE-Q30 — remaining fullrun divergences, IMPL42–54 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Run data `re/work/runs/q30/` (not in git):
- `portfix/`: scratch copy of master 3a3e354 `src/` + RE-Q29 fixes C, D, E (impl75, not merged there) + the fixes below,
  each marked `// q30 X` (also debug dumps: env `Q30_ROWS`, `Q30_EHEX`, `Q30_BUL`, which must not go into `src/`);
  `portfix_src_base/` = master src; `run_all.sh <tag>` runs golden / fullrun_noclip / selftest1 / the 4 demos,
  results `<session>_<tag>.log`, `demo<n>_<tag>.txt`; `answers.txt` = raw notes.
- `fr_scan48/`, `fr_scan51/`: partial replays of the original (`TH08_SCAN`, `TH08_SCANBUL`, `TH08_RNGSITE`).

## 1. Question 1 — fullrun_noclip divergences

Start (scratch tag CDE = master + RE-Q29 C/D/E): golden full (25 634), selftest1 full, demos 0–3 in sync, fullrun 29948.

| divergence | cause | label | spec § | after the fix (scratch, cumulative) |
|---|---|---|---|---|
| fullrun 29948 (bullets 181/180) | `PlaceBullet` 0x42F5F0 passes the **unwrapped** pattern angle to `Vec3_FromPolar` (0x42F97E pushes the local) and stores only the wrapped one; ring bullet 1061 (a ≈ 4π/3): original vel.x 0xBEFFFFFD, port polar(wrapped) 0xBF000001 → off screen one frame early (J) | SPEC FIXED + CODE BUG | 08 §3.2, 07 §9.3 | fullrun 48756 |
| (checked at 48756) | player-shot overlap in `Player_DamageAgainst` uses f32 box edges (helper 0x451CE0), not \|d\| ≤ sum of halves (K); no session change | SPEC FIXED + CODE BUG | 09 §4.5 | — |
| fullrun 48756 (rng +8, score +4) | not the shot test: children e3–e6 of the stage-4B boss (sub21, `native_cb 8`) had different orbit angles (e2, the leader, matched), so shot pb25 hit e3 one frame early. `EclNative_8` 0x4244F0 walks from the **parent**, writes int vars 10037 / 10005 / 10006, counts calls in int var 10007 and adds the leader's orbit velocity when the counters differ; the port followed the old spec wording (root walk, float vars 10017/10018, no velocity terms) (L) | SPEC FIXED + CODE BUG | 07 §12 native 8 | fullrun 51708 |
| (found at 51708) | `Vec3_FromPolar` 0x4286E0: `fcos`/`fsin` results are extended precision (precision control does not apply to them), × r rounded once; the port rounded cos to f32 first (1-ulp velocity differences in ~190 bullets) (M) | SPEC FIXED + CODE BUG | 05 §10 | no change of the first row |
| fullrun 51708 (bullets 405/404) | volley of the 4B familiars (sub23, fires every 10 frames) at poll 51632, the frame rank goes 15 → 16 by time rank: `EnemyMgr_TimeRank` runs at the **start** of `EnemyMgr_Calc` (0x42C77E, before the timelines and the slot loop), so the volley already uses rank 16 (lerp_f(−0.15, 0.15) = 0 → speed 1.1); the port ran it at the end (rank 15 → 1.090625) (N). The op-176 box was moved to its place too (O, no session change) | CODE BUG | 07 §10.1 step 1, 11 §3.2 | fullrun 60112 |

## 2. Question 2 — IMPL42–54 open questions

Skipped as already answered or not a spec question: IMPL42 Q1 (golden 9047 = RE-Q28 CODE BUG C, fixed since),
IMPL42 Q3 (port artefact, no question), IMPL43 Q2–Q4 (status notes), IMPL44 Q1–Q2 (traces / placement note, no
question), IMPL45 Q1–Q4 (notes), IMPL46 Q3 (citation note), IMPL47 Q1 (proposed audit item: re-parse score.dat per
`Result_OnRegister`; 15 §2.3 already states the behaviour), IMPL49 Q2 (note for the A.53 brief).

| question | answer | label | spec § |
|---|---|---|---|
| IMPL42 Q2 ops 90–92 link position | spec right and already explicit (a chained child that spawns links the new enemy directly after itself, overwriting its own +8); the port walks to the tail from the spawner's +8 — differs only for a chained child with a successor (no data case known) | CODE BUG (low priority) | 07 §10.5 |
| IMPL43 Q1 13 §2.6 draw-count summary | fixed: 21/26 one draw for param.x > −990, two for ≤ −990 (port right) | SPEC FIXED | 13 §2.6 |
| IMPL44 Q3 single-kill gauge timers | the three `Timer_Set` calls (0x18B89E0 := 0, 0x18B89C8 := 30, 0x18B89D4 := 50) follow the `GaugeAdd` and are unconditional; `GameMgr_GaugeAdd` 0x43C0BB only adds, clamps and copies (port right) | NEW FACT | 07 §10.3a |
| IMPL45 Q5 window close | `Window_Proc` 0x442390 answers WM_CLOSE with 0x17CE8FC \|= 0x80, return 1 (no `DefWindowProc`); nothing reads bit 7 and the loop flag 0x17CE704 is only written 0 → closing the window does nothing. Every main-loop exit (Frame_Run ≠ 0, failed device reset) takes the same path: save (if PLST tag) → chain teardown 0x43CE30 → `Sound_Update` until 0 | NEW FACT | 15 §2.2 |
| IMPL45 Q6 quitting frame | `Frame_Run` 0x441E70 returns right after the calc chain and `Sound_Update` (JoinLoader; 1 or 2): **no draw and no Present** for the quitting frame. 15 §2.2 ("after that frame's calc, draw and Present") was wrong; 00 §2 made explicit. The port now draws and presents it (impl45) | SPEC FIXED + CODE BUG | 15 §2.2, 00 §2 |
| IMPL46 Q1 spell bit 9 clear | `Spell_Start` clears bit 9 as its first statement (full write order now listed); also cleared by the stage-start zero-fill → does not survive a retry (port right; the `SPEC-GAP` can go) | SPEC FIXED (wording) | 07 §11.1 |
| IMPL46 Q2 cooldown on a forced start | confirmed: the forced test (+6 ≠ 0 and +0xE2A68 == 1) jumps past the decrement, the bombing branch and the start tests. Code fix wanted (decrement inside `update_bomb`, skipped when forced) | CODE BUG | 09 §1.1 |
| IMPL48 Q1 SE off and the BGM queue | the whole `Sound_Update` returns only without a DirectSound object (+0x610); "SE on" = 0 skips only the SE flush, the BGM queue (load / play / fade) is processed (port right) | SPEC FIXED | 13 §1.3 |
| IMPL48 Q2 play during a fade | the play command is staged (one stage per `Sound_Update`); usual path: stage 0 stops the buffers (fade mode := 0), stage 4 plays with fade mode / n / N := 0 and `Bgm_SetVolume(0)`; preload path (Supervisor +0x150 bit 13): no stop, the fade keeps ticking until the play at stage 7. So the fade is cancelled and the attenuation reset — the port's immediate cancel/reset is equivalent except for the staging latency | NEW FACT | 02 §4 |
| IMPL49 Q1 archive entry offsets | not validated; only table offset < file size (open). A read uses next offset − own offset and fails only if seek/read fails (port right) | NEW FACT | 01 §4 |
| IMPL50 Q1 caller's ip after its own death-sub jump | `Ecl_Run` works on a local ip: after op 95 the local ip moves past it in the **old** sub, the next instruction runs only if its time equals the reset clock 0, and the context write-back overwrites the death-sub ip; clock ends at 1. The death sub (consumed) never runs for the caller, which dies at step 6 without it. Port continues at the death-sub start | SPEC FIXED (new quirk) + CODE BUG | 07 §7 op 95 |
| IMPL50 Q2 from a thread | threads are not freed by `KillAll`; the thread finishes its frame, the main context keeps the death sub; the death at step 6 frees the threads as usual (port: thread keeps running this frame, then dropped — same visible result) | NEW FACT | 07 §7 op 95 |
| IMPL50 Q3 `KillAll` jump kind | `Ecl_SetSub` 0x418450 on the main context: ip, clock 0, wait 0, sub id only — threads, bullet descriptor, auto-fire and rank ranges kept. Port applies `reset_for_jump` | NEW FACT + CODE BUG | 07 §10.6 "Death subs" |
| IMPL51 Q1 mode bits of a chained child | ops 90–92 never write e+0x3330 (only writer `Enemy_ReactPlayerMode` 0x42C585); the spawn-time run uses 0 → D = difficulty bit, both human- and youkai-masked time-0 instructions run. Port writes the bits before the run | NEW FACT + CODE BUG | 07 §10.5 |
| IMPL51 Q2 op 92 follow setup | after the spawn-time run: +0x2D40 := parent pos, +0x2D88 := +0x2D40 + pos, aura at +0x2D88, then bit 9, then the link. Port writes them before the run | NEW FACT + CODE BUG | 07 §10.5 |
| IMPL52 Q1 op 176 box position | first inside the bit-10 gate, before `EnemyMgr_TimeRank`, the timelines and the slot loop (call 0x42C773). Port after the slot loop (scratch fix O: no session change) | SPEC FIXED + CODE BUG | 07 §7 op 176, §10.1 step 1 |
| IMPL52 Q2 trail strip coordinate step | k from a separate pre-walk; coordinate A steps at the end of every walked entry, skipped ones included (port right) | NEW FACT | 07 §10.6 |
| IMPL53 Q1 stage VMs of natives 26/28/29 | the two spell-background VMs g_Stage +0xB38 / +0xDDC (0x4E4B68 / 0x4E4E0C), as native 12 (port right); native 26 tests the low byte of arg | SPEC FIXED (detail) | 07 §12 |
| IMPL53 Q2 native 28/29 velocity | 28: game speed := 1/f32(arg) first, then vel (+0xD50, xyz) ×= the new game speed; speed +0xD68 / ex state untouched. 29: vel ×= 1 / current game speed, then flag 0x20 if 1/arg < 1, speed := 1. Every live bullet (port right) | SPEC FIXED (detail) | 07 §12 |
| IMPL53 Q3 native 28/29 sprites | 28: +0x218 := sprite for every live bullet, sprite := 0x6F only for 0x60–0x6F; 29: restore only if the current sprite is in 0x60–0x6F. VM0 only. Port restores every bullet whose sprite differs | SPEC FIXED + CODE BUG | 07 §12 |
| IMPL53 Q4 +0x218 at spawn | template load writes the template's **base** sprite; `AnmVm_Copy` carries it, the colour `SetSprite` does not touch it → a spawned bullet holds the uncoloured template sprite. Port stores the coloured sprite | SPEC FIXED + CODE BUG | 05 §2 +0x218 |
| IMPL54 Q1 +0xE2A7C on a special-mode press | kept: the special-mode branch returns before both writes (port right; `SPEC-GAP` can go) | NEW FACT | 09 §1.1 |
