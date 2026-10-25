# RE-Q49 — replay pilot: the port's two early divergences on third-party .rpy

Code: decompile/disasm of th08.exe (0x46E136, 0x43ABD7, 0x406E50, 0x44F5E0, 0x44F2D0; jump table bytes 0x44F90A).
Traces used only to locate: re/work/runs/replay_pilot (pilot sessions th08_ext_replays/ref/*, port traces at 40cc7d4),
re-run of the port at ebffb92, and one original run at 60 fps: re/work/runs/q49/normal_Nd115 (`run_rpy.sh --normal`).
Scripts: re/work/runs/q49/scripts (cmp.py, rows.py).

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | replay screen: `cursor` 2…8 (recorded) vs 0 (port) from the file-confirm row on (10 non-full-game replays, row 97) | `MainMenu_Replay` keeps **one** cursor, menu +0x00: list row (sub-state 1, copied to +0xC28C), stage row (sub-state 2; the file confirm sets it to the first recorded stage), speed (sub-state 3; := 0 on the stage confirm). Cancels restore +0x00 := +0xC290 (3 → 2) / +0xC28C (2 → 1). Up-direction skip loop wraps −1 → 9 (tests header +0x44); invisible for written files | 15 §3.8 "One cursor field for all three sub-states" (new; the first-stage rule was already there) | CODE BUG (trace state): `menulist.rs` `replay_run` keeps `self.cursor` = file row in sub-states 2/3 and uses `lists.rpy_cur` / `lists.rpy_speed`; the traced menu cursor must be the stage row / speed. Stage and speed choices themselves are right. |
| 2a | full games (and every replay): "loading left one frame early" (recorded loading 1 at the `release stage poll` row, port 0) | The loader thread clears GM+0x38 between calcs; `release stage poll P` = first calc seeing 0 → row of poll P−1 loading 1, row of poll P frame 1. The port clears it at the end of calc P−1: gameplay starts on the same calc. Same 1-row `loading` skew in golden (release 722), hidden there by the loader-column rule. Same at 60 fps (normal_Nd115: release 183, row 183 loading 1/0, row 184 frame 1 in both) → not fast mode | 11 §3.1 after "Loading frame count" (new paragraph) | right (sample timing only, no state difference) |
| 2b | `rng_ctr` −4 from the first stage start of every replay (90 vs 86; state equal) | `GameManager_StageStart` replay branch: `GameMgr_InitRank`, `Replay_Register(1)`, then `s = g_Rng; GameMgr_UpdateChecksum; g_Rng = s` (2 × Rng_Range(100000) = 4 Next16, state restored). Count: 2 jitter + 60 decoys + 4 power + 2 score.dat + 8 deaths/bombs + 8 lives/power (replay load) → seed → 4 checksum + 2 EnemyManager = 90. Same at 60 fps | 11 §3.1 / §1.3 (already stated); 11 §1.4 note that restored draws count in `rng_ctr` (new) | CODE BUG (trace counter only): `gamemgr.rs` `GameManager::on_register` replay branch calls `replay::play_stage_load` but not the save/restore checksum (its comment says it does). |
| 2c | silentselene 51852 (Youmu solo): `rng` differs at frame 1, port +8 draws every frame | `Option_YoumuSolo` 0x44F5E0, unfocused, direction code 0: `switch` case 0 → return (0x44F90A[0] = 0x44F76A → 0x44F8A9 → 0x44F904), skipping the facing step and both `Effect_Spawn(0x2F)` calls. The team half 0x44F2D0 spawns before its switch (every frame) | 14 §3.3 (corrected: "effect every frame" was wrong for the solo on direction 0) | CODE BUG (gameplay RNG): `pshot.rs` `place_option`, `OptKind::YoumuSolo`: no sparkle when unfocused and `dir == 0`. |

## CODE BUG list
1. `pshot.rs` `place_option` (Youmu solo, 0x44F5E0): when unfocused and the direction code is 0, spawn no effect 0x2F
   (14 §3.3). Gameplay RNG: every Youmu-solo game diverges from the first standing frame.
2. `gamemgr.rs` `GameManager::on_register` (0x43ABD7): after `play_stage_load`, one `Stats::checksum` with the RNG state
   saved and restored (11 §3.1). Only the `rng_ctr` trace column changes (+4 per replay stage start).
3. `menulist.rs` `replay_run` (0x46E136): one menu cursor for list row / stage row / speed (15 §3.8): file confirm →
   cursor := first stage, stage confirm → 0, 3 → 2 cancel → +0xC290, 2 → 1 cancel → file row. Trace `cursor` only.

## Not analysed (next divergences once 1–3 are skipped; pilot port traces at 40cc7d4, columns cursor/rng_ctr/loading ignored)
Bu08 / JS06 (spell practice): row 191 frame 2 rng + enemies 1|0. Nd115: row 220 frame 31 enemies/boss/flags (boss
not spawned in the port). Others later in the stage: 52104 row 621 score 1|0; 51842 row 2891 score; 80406 row 2822 rng;
50715 row 3680; 80412 row 4011; 52089 row 5073 (frame 4884|4964); 53373/52220/88234/82309/51249 ~5000–6100 (boss
enemies 1|2); 53084 Extra row 7021 rng; F128 the last row (score, after the end); Ar02/Bi01/Dn14/Ro168 (non-1.00d or TAS).
