# RE-Q67 — worst batch500 draw-log recordings, stage-5 flash, IMPL141 questions (2026-10-31)

Questions: [IMPL141](questions/IMPL141_QUESTIONS.md).

Runs: re/work/runs/q67 (CHECKPOINT.md). Scratch port = q66 portcopy (RE-Q66 fixes)
+ RE-Q67 fixes A–D behind `Q67NO<letter>` env switches (re/work/runs/q67/portcopy, target/re67). Groups by `drawdiff_src`
(drawdiff of a src snapshot at the start of the cycle, same rules as target/drawgroups_144). Before = target/drawgroups_144
(master 3a51a8d); `b0` = q66 portcopy unchanged; `f` = all RE-Q67 fixes (bin_f).

## Changed spec sections
| section | change | evidence |
|---|---|---|
| 08 §6.3 (laser per frame) | startup / fade width and alpha formulas use `Timer_GetF` (cur + frac; f32 per operation, 24-bit x87 precision control, 05 §10); every test uses the integer cur. Under a slowdown the width changes every frame | `BulletMgr_Calc` 0x431240 laser part; ref500 udSr22 present 4837 (speed 0.5: ref 4.8, 4.4, 4.0 …, port 4.8, 4.8, 3.2, 3.2 …) |
| 12 §5.3 (dirty-field writers) | `Spell_End` ORs HUD +0x04 with 0xF only inside its card-active block (spell flag bit 0); `Spell_Start` on every call | `Spell_End` 0x4161B0, `Spell_Start` 0x4152A0 (IMPL141 Q1) |
| 12 §5.3 (full-repaint counter 0x17CE8CC) | `ContinueMenu_Update` writes 8 only once: state 3 (continue accepted), the resume call with counter > 0x59 | 0x40512A, byte scan of every 0x17CE8CC reference (18 sites; one in 0x404890) (IMPL141 Q2) |
| 12 §5.5 (popup timing) | the three popup timers are `Timer`s ticked at game speed; slide x from `Timer_GetF`; `t < 30` and the lifetime tests on cur → a popup lives 250/gs frames under a slowdown | `Hud_Update` 0x435900; ref500 udSr22 " BONUS" drawn 7659–7956 (port 7658–7905) |
| 13 §3 (type 3 flash) | the tick also runs on the repeat-reset call: after the first repeat t = 0 (alpha A) is never drawn; each repeat is dur frames A·(1 − t/dur), t = 1…dur−1, then 0 | 0x45BC90; dl3 udM401 present 54297 (native 10: 3e, 3c, …, 03, 00, 3e …) |
| 13 §4 (popup life) | small / large popup ages are `Timer`s at game speed; `Timer_Greater`(60 / 90) compares the integer cur → kept while cur = 60 / 90 with frac > 0 | `Ascii_Calc` 0x402200, `Timer_Greater` 0x4066C0; ref500 udSr22 present 4837 (5 "+0" copies per frame at speed 0.5, port 4) |

## Causes
Q1 = the biggest non-skipped groups of the 6 worst drawgroups_144 recordings; Q2 = the stage-5 flash.

| cause | groups / recordings / first present | original | spec | port |
|---|---|---|---|---|
| A flash repeat reset | `-` [32,16,416,464] colour 3e/40 + extra 4 frames (12 dl3 recordings; 51890 53888, 53020 51964 in batch500) | `Timer_Tick` after `Timer_Set(0)` on the repeat-reset call; also the 0x18B8A2C ≠ 0 → removed test first | 13 §3 type 3 | CODE BUG |
| B popup life on int cur | ascii [0,208,8,220] / [104,208,112,220] missing (the boss "+0" `Popup_LargeStatic`; 10 recordings in drawgroups_144, udSr22 4837: 2939 frames) | `Timer_Greater` = n < cur (integer) | 13 §4 | CODE BUG |
| C laser width from Timer_GetF | etama#0 [96,48,112,64] pos (udSr22 4837, 2579 frames) | width / alpha formulas with cur + frac | 08 §6.3 | CODE BUG (the width also sets the hit-box size.x, so this is logic too) |
| D HUD popup timers at game speed | ascii " BONUS" glyphs missing (udSr22 7906, 50948 30812) | impl +0x22DA0 / +0x22DC0 / +0x22DE0 are game-speed Timers, x from GetF | 12 §5.5 | CODE BUG |
| — 53406 etama#2 [16,y] strips, udFa11 etama#2 [64,y] colour + 2D z, 51890 / 53020 stg8enm colour | | RE-Q66 A / B / H | 13 §2.3, 13 §2.5, 07 §10.1 | gone with the RE-Q66 fixes (b0) |
| — etama#1 [160,224,176,240] pos / pos+colour (all 6) | | op-128 orbit stars ≈ 0.03 px; two stars at one centre paired crosswise | 13 §2.6c | precision (skipped) |
| — ascii [0,112,16,128] / [80,112,96,128] pos (5 recordings) | | 0.9-scaled message glyph `Draw2DSnap` ties | — | precision (skipped) |
| — etama#1 [0,96,16,112] / [16,96,32,112] pos (50948), stg5bg / stg7bg pos+z | | list-B particle / projection precision | 13 §2.6c | precision (skipped) |
| — `-` missing / `K` th08logo at the replay end | | menu after a game | RE-Q65 H | skipped |

Not analysed: face_st05 / face_st04a / face_st04b portrait colour (50948 76517 25 frames, 54840 59454, udTh03 dl3 55198;
port draws the spell cut-in face VM ffff1f1f for ~25 frames where the reference keeps c0ffffff).

## Q3 — IMPL141 questions
1. `Spell_End` 0x4161B0: `HUD +0x04 |= 0xF` is inside the `if (spell flags & 1)` block (after the bonus / capture
   part, before sfx 0xF and `Stage_EndSpellBg`); with no active card nothing is written. 12 §5.3 updated.
2. `ContinueMenu_Update` 0x404890: the only write (0x40512A) is in state 3 on the call where the counter > 0x59 (resume
   after "continue: yes", next to the lives / bombs / power reset and `*0x164D0BB := 0`). 12 §5.3 updated.

## CODE BUG list (for the next IMPL brief)
| # | src function | change | spec |
|---|---|---|---|
| A | `effect.rs` `ScreenEffect::step` type 3 | first: `load_frames() ≠ 0` → `Ret::Remove`; on the repeat reset (`t ≥ dur`, repeats left) `timer.set(0)` and then fall through to the common `timer.tick(gs)` (no early `return Ret::Next`) | 13 §3 type 3 |
| B | `ascii.rs` `popups_calc` | small and large popups: free when the **integer** age > 60 / 90 (keep the age as a `Timer` ticked at gs, or compare `age.floor()`), not the f32 age | 13 §4 |
| C | `bullet.rs` laser update (state 0 width / alpha, state 2 width / alpha) | formulas in f32 with `t = timer.cur as f32 + timer.frac` (f32 per operation — not f64, 05 §10): width = t·width/startup, width − t·width/fade, alpha = min(255, trunc(t·255/startup)); keep every test on `timer.cur` | 08 §6.3 |
| D | `hud.rs` `ActivePopup::update` (+ its call in the HUD update) | keep a `Timer` ticked with the game speed: x = (GetF·(−312))/30 + 416 in f32 per operation (an f64 version gave 1-px message-glyph ties, udFa11 7222) while cur < 30 else 104; switch off when cur ≥ 250 / 180 / 280; then `tick(gs)` | 12 §5.5 |
| E | `enemy.rs` `EnemyManager::finish_spell` | write HUD +0x04 \|= 0xF only when a card was active (inside the card-active branch); remove the SPEC-GAP | 12 §5.3 |
| F | `ascii.rs` `ContinueMenu::update` | `set_repaint(8)` only in the state-3 resume step (counter > 0x59, with the lives / bombs / power reset), not on every call; remove the SPEC-GAP | 12 §5.3 |

C changes the laser hit-box (size.x := current width / 2) under a slowdown, so the logic batches must be part of the check.

## Full-set compare (orchestrator)
After the IMPL cycle with A–F on master: `target/longcheck.sh <IMPL-tag>` (ref500 + ref_b2 logic batches, drawgroups on
the batch500 draw-log set + golden and on dl3, METRICS row). Expected: `-` flash groups gone in the 12 dl3 stage-5
recordings; ascii [0,208] / [104,208] missing groups gone; udSr22 ≈ 99.6 %.

## Effect (scratch draw-log compare, run f; trace identical in every run)
Identical frames by present (drawdiff `--key present`). 144 = target/drawgroups_144 (master 3a51a8d); b0 = q66 portcopy
(RE-Q66 fixes, no RE-Q67); g = RE-Q67 A–D, final f32 version (q67/g, q67/g_g). The dl3 / golden rows are from run f (A–D with
the first f64 version of C / D; the flash fix A is identical in both); dl3 "q66 f" = RE-Q66 final run. Trace: every row
identical from the first main-menu row (all runs).

| recording | 144 | b0 (RE-Q66) | after (g) | groups after |
|---|---|---|---|---|
| gensokyou_th8_udSr22 | 4916 / 7969 (61.69 %) | 4904 / 7988 | 7955 / 7988 (99.59 %) | 4 (menu after the game + 1 star) |
| silentselene_51890 | 43827 / 54817 (79.95 %) | 44052 / 54835 | 44055 / 54835 (80.34 %) | 11 |
| silentselene_53406 | 33702 / 41941 (80.36 %) | 38702 / 41960 | 38702 / 41960 (92.24 %) | 9 |
| silentselene_53020 | 43356 / 52893 (81.97 %) | 43581 / 52911 | 43584 / 52911 (82.37 %) | 11 |
| gensokyou_th8_udFa11 | 39478 / 46680 (84.57 %) | 39879 / 46698 | 39879 / 46698 (85.40 %) | 10 |
| silentselene_50948 | 107497 / 123713 (86.89 %) | 121284 / 123731 | 121523 / 123731 (98.22 %) | 66 |
| dl3 gensokyou_th8_udM401 (flash) | 82842 / 87485 (IMPL144) | 86192 / 87531 (q66 f) | 86543 / 87531 (run f) | |
| dl3 gensokyou_th8_udSs12 (flash) | 92685 / 99503 (IMPL144) | 98181 / 99532 (q66 f) | 98751 / 99532 (run f) | |
| golden | 25375 / 25633 | — | 25344 / 25646 (run f; q66 scratch base 25333) | |

51890 / 53020 / udFa11 stay low because of the orbit-star precision class (etama#1 [160,224], ≈ 10k frames each), skipped
per brief. The first f64 version of C / D cost 7–9 frames in udFa11 / 53406 (ascii [192,96] message-slide 1-px ties):
f32 per operation is required (05 §10).
