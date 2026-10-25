# RE-Q51 — replay pilot: "frame / ff / calc one behind" group, replay-end flags, 52104 r690

Code: decompile of th08.exe (`FuncChain_RunCalc` 0x43CA50, `Replay_FastForward` 0x452490, `ContinueMenu_Update`
0x404890, `GameMgr_Calc` 0x439BC7, `MainMenu_OnRegister` 0x470A6C, `MainMenu_Top` 0x4674E0, `ShotUpd_AliceLaser`
0x450840); probe `re/probe/session.inc` (row per pass at 0x43CA65, `ff` = pass). Byte/decompile search: only
`Replay_FastForward` returns 6. Traces used only to locate: pilot sessions th08_ext_replays/ref/*.
Runs (re/work/runs/q51/): `normal_Tt16` (run_rpy.sh --normal, 60 fps re-record of gensokyou udTt16);
`orig_52104` (TH08_SCAN polls 680–700) and `orig_52104b` (672–681, stopped after the window); scratch port copy
`portcopy` (snapshot of src incl. IMPL117 WIP A–C, built with target/re51; src/ untouched) → `base`, `fix1`…`fix5`.

| # | question | original | spec | port |
|---|---|---|---|---|
| 1 | 10 sessions first differ with `frame`, `ff`, `calc` one behind | **Not a fast-mode artifact**: the 60 fps re-record of Tt16 has the same 5830 gameplay rows and the same 2004 `ff` rows as the fast one. `ff` is the calc-chain pass. The first diff of every session is the first skippable-dialogue `Replay_FastForward` restart (return 6): the runner restarts the chain from the head inside the same Frame_Run, so `Supervisor_Calc` (prio 0) runs and polls input once per pass, and several gameplay frames share one present (udAr02: presents 5170–5172 run 2 / 3 / 2 frames). The probe writes one trace row and one `polls.txt` entry per pass; `input.txt` keeps only the last poll per present. Port gameplay is identical (compared per present: scripts/bypresent.py). The port session runner instead feeds `input.txt` per present, uses present − boot as the poll index (also for the loader sync points) and writes one row per present, so all later rows are shifted | 00 §3 (restart = full extra calc pass, new), 15 §3.3 (pass note + example), specs/data/drawlog/README.md (polls per pass, `ff`/`calc`/`poll` on pass rows) | CODE BUG D (tool) |
| 2 | spell practice replays end ~32 rows early (`frame` one less, bit 2 cleared a frame early) | (a) `ContinueMenu_Update`'s early exits (stage practice → 6, replay → 7, no-continue ending → 9/6) each set game_over 0x164D0BB := 0 and displayed score := score before returning, with no sound / VMs / capture / play-time update. `GameMgr_Calc` runs later in the same pass, sees game_over 0, sets bit 2 and runs one more gameplay frame (udJS06: frame 725 of 725). (b) After the replay the menu rows differ: flags bit 3 is **kept** by the 2→7 switch; `MainMenu_OnRegister` clears bit 3 only when bit 1 (demo) is set, clears bits 0 and 14, sets screen 0 and the previous-scene cursor (no practice override for a spell replay); the Top handler's entry block (release frame, after loading) then switches to screen 7, zeroes +0x10/+0x1442C/+0x0C/+0xC294, interrupt 13, description VM interrupt 2, bit 3 := 0 | 11 §4.2 (early exits clear game_over, consequence), 12 §4 (opening timing wording), 12 §2.2 (back from a replay, new), 15 §2.9 (play time only when the prompt opens) | CODE BUG E, F |
| 3 | 52104 `score` 0x7739/0x7738 at r690 | Frame 501: new enemy (life 20) takes 9 = old laser B2 (7, timer 16 even) + two 1-damage trail rects. B2 was ended by the frame-489 focus change (RE-Q50 rule; new laser B1 created in frame 490 in both). `ShotUpd_AliceLaser`'s end branch (interrupt 1, slot cleared, update := 0) **does not return**: the follow body still runs on that frame, so B2 moved to option 0 (x 82.6) in frame 490. The port returns at once, B2 stays at x 79.79 and 12.8 px higher, and its rect misses the enemy at frame 501 | 14 §4 (update 5 fall-through quirk) | CODE BUG G |

Confirmation (scratch port with D–G on top of the IMPL117 WIP A–C; all 22 pilot sessions, re/work/runs/q51/fix5):
- 0 differing rows over the whole session: Bu08, JS06, Nd115, Tt16 (6050 rows incl. the menu return), F128.
- First differing rows of the rest (not analysed): GameFlags bit 10 (0x400) set in the original only: Ar02 6634,
  Dn14 5048, Ro168 6186, 53373 47393; rng/score: 50715 3680, 51249 8330, 51842 2891, 51852 4717 (RE-Q49 bug,
  pshot.rs sparkles, not in the scratch copy), 52089 18346, 52104 22171, 52220 16139, 53084 7021, 80406 2822,
  80412 4011, 82309 6870, 88234 55210, Bi01 7416.

## CODE BUG list
D. Session tool (`session.rs` `Session::run`/`load`, `task.rs` restart, `supervisor.rs` calc): take the raw input per
   `Supervisor_Calc` call from `polls.txt` (`S` rows in order; `input.txt` is per present and loses restart polls),
   advance the poll index on every chain pass (also for the loader sync points, which are poll indices), and emit one
   trace row per pass (pass rows ff = 1…n before the final row ff = n; `calc` only on the final row; README "Trace").
   Gameplay itself is right. Scratch version: re/work/runs/q51/portcopy (`Ctx::q51_restart`, `Sched::pass_rows`).
E. `ascii.rs` `ContinueMenu::update`: the three early exits (practice → SCENE_RESULT, replay → SCENE_REPLAY_END,
   no-continue → SCENE_ENDING) must set `game_over = 0` and displayed score := score, and must not add play time
   (that happens only when the prompt opens; 11 §4.2, 15 §2.9). Today game_over stays 1, so `GameManager` skips the
   last gameplay frame of every replay / stage practice / no-continue ending.
F. `replay.rs` `end_playback` + `menu.rs` `MainMenu::on_register` / `top`: keep flags bit 3 after a watched replay
   (clear it at the scene switch only for demos), clear bits 0 and 14 (and the port's practice bookkeeping), stay on
   screen 0 with the previous-scene cursor during menu loading, and do the replay-screen switch in the Top entry
   block (12 §2.2 "Back from a replay"): +0x68 := 0, screen 7, counters 0, range interrupt 13, description VM
   interrupt 2, bit 3 := 0. Trace-visible in every replay's menu-loading rows.
G. `pshot.rs` `laser_update` (update 5): the end branch must not return; after interrupt 1 / slot clear /
   `update = 0` run the option-0 test and the rest of the body (rects, trail shift, pos, colour) that frame (14 §4).
