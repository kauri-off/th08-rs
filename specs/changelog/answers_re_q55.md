# RE-Q55 — op 109 filters; gameover_save and musicroom_practice divergences

Code: decompile/disassembly of th08.exe (functions per row). Traces only to locate; no original runs were needed.
Runs in re/work/runs/q55/ (CHECKPOINT.md). Scratch port `portcopy` = master src snapshot 2026-10-26 08:43 + fixes
A–H (built with target/re55; src/ untouched); the fixes alone are in `q55_scratch.diff` (5 files: ascii, gamemgr,
menu, menulist, result) and were re-applied on master of 2026-10-26 ~09:30 (`fix2src`) with the same results.

| # | question / row | original | spec | port |
|---|---|---|---|---|
| 1 | IMPL121 Q1, op 109 | `Ecl_Run` case 0x6D (0x41B8A0–0x41B8E7): desc origin := **pos e+0x2D34** + shot offset, then `BulletMgr_SpawnPattern`(e+0x2E24) directly — no life test, no store-flag test, no `Ecl_FireBullets` filters, no rank add. `Ecl_FireBullets` writes its decoded, rank-adjusted values **in place** into e+0x2E24, so op 109 re-fires them as left (plus op 111/113 in-place edits). 0 uses in the data | 07 §7 row 109, §9.1 note | CODE BUG 1 (unreachable) |
| A | musicroom 3506 (menu) | Trace samples screen/+0x00 at every `MainMenu_Calc` entry (probe hook 0x467399); 16 → 17 returns 2 → second call → 17 on the press frame | drawlog README trace cols 29/30 | session-tool bug A |
| B | musicroom 4062 (cursor) | +0x00 is the stage row on screen 16 (shot only transiently for Left/Right), the card row on screen 17 (0x46B174 / 0x46BBC0) | 16 §6.1, §6.2; README col 30 | session-tool bug B |
| C | musicroom 4226 (power) | Spell practice start power 30 / 80 / 128 by card id (0x43ABD7) | 11 §3.1 (already right) | CODE BUG C |
| D | musicroom 5077 (frame, flags bit 2) | Continue menu "Yes" in spell practice / Extra acts in the press call; game_over := 0 on both music branches → one more gameplay frame (0x404890 case 1) | 12 §4 "Confirm timing", 11 §4.2 | CODE BUG D |
| E | gameover_save 4215 (frame, flags bit 2) | "No": at P+20 game_over := 0, scene 6, VMs hidden, displayed score := score → one more gameplay frame (case 4) | 12 §4, 11 §4.2 | CODE BUG E |
| F | gameover_save 7700, musicroom 7854 (rng_ctr +2) | `Result_Calc` state 2 frame > 19 returns 0 → calc node removed in that pass → `Result_OnRemove` → `ScoreDat_Save` (2 draws) on the last result frame | 16 §2 Exit | CODE BUG F |
| G | musicroom 7887 (flags bit 14) | `MainMenu_OnRegister` sets +0x6C from flags bit 0 for **any** previous scene (here the result screen) and clears bits 0/14; the Top entry block (call 0, after the replay test) re-sets bit 14 for +0x6C == 2 and switches to 8/15 (return 1) | 12 §2.2 | CODE BUG G |
| H | musicroom 9169 (flags bit 0) | Screen 16 Bomb: flags &= ~1 only with the switch to the top menu at P+20 (sub-state 3) | 16 §6.1 | CODE BUG H |

Confirmation (scratch A–H): gameover_save 12743/12743 and musicroom_practice 9363/9363 rows identical; golden,
fullrun_noclip, noclip_test1, options_cfg, selftest1 still 0 diffs; the 22 pilot replays give the same first rows as
master without A–H.
