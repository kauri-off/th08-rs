# RE-Q66 — draw-log difference groups on the 36 dl3 recordings (2026-10-31)

Runs: re/work/runs/q66 (CHECKPOINT.md). Scratch port = q65 portcopy (RE-Q65 fixes A–N)
+ RE-Q66 fixes A–D, F, H and one session tool fix, behind `Q66NO<letter>` / `Q66NOTOOL` env switches
(re/work/runs/q66/portcopy, target/re66). Recordings: re/work/runs/dl3 (12 shots × 6A / 6B / Extra, selection.tsv).
Before = master 97bae01 (target/drawgroups_dl3); `b0` = q65 portcopy unchanged (15 recordings); `a` = A–D (udVn17, 54214,
77236); after = run `f` = all fixes (bin_f), groups by a drawdiff built from a src snapshot (`drawdiff_src`, same group
rules as target/drawgroups_dl3).

## Changed spec sections
| section | change | evidence |
|---|---|---|
| 07 §10.1 step 5 | the main VM runs with colour := e+0x2E20, and e+0x2E20 := colour afterwards: the colour reached by the enemy's script survives every later script start (dir animation, ECL anm ops reset it to white) | `EnemyMgr_Calc` 0x42C660; silentselene 77236 present 39707 (Extra boss after the last word, alpha 0x80 kept), probe `TH08_SCANVM` (VM colour = e+0x2E20 = 0x80FFFFFF after the restart) |
| 12 §5.5 (`Hud_ShowBonus`) | both callers show the bonus (and add the score) only when CancelBonus + KillAll ≠ 0 | `EnemyMgr_Calc` death switch / `Spell_End` 0x4161B0; silentselene 54440 present 22953 (port restarted the " BONUS" slide with 0) |
| 12 §5.10 (ASCII frame counter +0x8284) | slow boots: boot calc passes after the ASCII manager's registration on the boot thread do call `Ascii_Calc`; the counter is then ahead of the menu frame by `asciicalc − poll` of the first `loader gauge` event (30 / 31 / 34 / 34 / 2 in 5 dl3 recordings) | events.txt of all 36 recordings vs the marker-blink group (exactly the 5 with offset ≥ 2) |
| 13 §2.3 (`EffectMgr_Calc`) | bit 10 is read live per slot; a spell-card death in the enemy calc (contact test, native laser tests) freezes the effects already in that frame | 0x427BF0; gensokyou udVn17 present 27576 (orbit stars), udM401 20974 (option sparkles) |
| 13 §2.5 (ring vertex z) | plain ring z = centre z = the spawn position z; rings at the player position carry the player z (0.49, 0.2 after a respawn) — the old "centre z is 0 for every spawner" was wrong | `EffDraw_Ring` 0x427450; silentselene 54214 presents 6277 (0.49), 7251 / 17112 (0.2) |
| 13 §3 (screen effects) | every alpha / amplitude formula uses `Timer_GetF` (cur + frac); range tests use cur; type 3 row rewritten (alpha 0 at the end, 0x18B8A2C test) | 0x45B160, 0x45B760, 0x45BC90, 0x45B800, 0x45BDC0, 0x45BF10; silentselene 77236 present 39707 (flash at game speed 0.25) |
| 14 §2.13 (Yukari boundary) | the spawn writes its colour after `AnmFile_SetScript`, so slot 4 (the init's type-0x23 re-spawn, colour 0xFFFFFFFF, no second script start) is drawn white; slots 5–7 get the colours of scripts 0x59–0x5B | `Effect_SpawnFixed(Param)` 0x425870 / 0x4259E0, `EffInit_YukariBoundary` 0x411720, `EffInit_YukariLSBoundary` 0x411A80; 54214 present 6300 |

## Causes (Q1: the biggest groups of target/drawgroups_dl3/all_groups.tsv)
| cause | groups / recordings / first present | original | spec | port |
|---|---|---|---|---|
| A effect freeze one frame late | etama#1 [160,224,176,240] pos / pos+colour (23 + 21 recordings, udM401 20974), etama#1 [0,96,16,112] pos (25, udM401 20974), the etama#2 ring rows of udVn17 / udTh03 | `EffectMgr_Calc` tests GameFlags bit 10 live: a death set in the enemy calc skips that frame's update + VM run; the port's end-of-`Player_Calc` mirror lets one more run through, and every live effect VM stays one frame ahead (rotation, alpha, scale) | 13 §2.3 | CODE BUG |
| — stg4aenm#0 [192,80,256,160] pos + 2D z (12, 54440 39137) | | trail main VM (RE-Q65 B) | 07 §10.6 | gone with the RE-Q65 fixes (b0) |
| — etama#2 [0,y,16,y+12] extra (29, 51602 8926) | | dissolve ring 32 segments (RE-Q65 F) | 13 §2.6 | gone with the RE-Q65 fixes (b0) |
| — etama#1 [128,112,192,176] missing (29, udVn17 7592) | | gauge-max aura (RE-Q65 D) | 09 §9.3 | gone in b0 / a |
| B ring z | etama#2 colour + 2D z (54214 6277, every Yukari / YmYy bomb) | plain ring vertex z = centre z (player z) | 13 §2.5 | CODE BUG |
| C Yukari slot-4 colour | same groups (colour part) | slot 4 white | 14 §2.13 | CODE BUG |
| D screen-effect time | untextured `-` colour (25 recordings, 77236 39709) | alpha from cur + frac (flash at speed 0.25 changes every frame) | 13 §3 | CODE BUG |
| F zero bonus popup | ascii.anm#0 " BONUS" glyph groups extra / missing (28 recordings, 54440 22953) | no `Hud_ShowBonus` when the total is 0 | 12 §5.5 | CODE BUG |
| G marker blink phase | ascii.anm#0 [0,240,48,256] / [48,240,96,256] uv+colour (5 recordings, 78684 3391) | counter +0x8284 includes boot passes on slow boots | 12 §5.10 | TOOL (session sync) |
| H enemy colour copy | stg8enm#1 colour (8 Extra recordings, 77236 39707) | VM colour round-trips through e+0x2E20 | 07 §10.1 step 5 | CODE BUG |

Next biggest groups after these (not investigated, all small or precision): the op-128 orbit stars still differ by
≈ 0.03 px (list-C billboard projection, 13 §2.6c precision class) and, where two stars share a centre with different
alpha, drawdiff pairs them crosswise (54214 26224); the 0.9-scaled HUD message glyphs ([0,112,16,128] etc.) have 1-px
`Draw2DSnap` edge ties (54214 6590, 53191 18326); `-` missing / `K` at the replay end (menu after a game, RE-Q65 H,
skipped); face_st08 / face_st08m (RE-Q65 I, skipped).

## Q2 — the three worst recordings
- gensokyou udVn17 (76.1 %): first difference present 240 = gauge loader race (12 §5.9, skipped); then cause A at 27576
  (orbit stars + rings for the rest of the stage; 27847 → 31838 identical frames with A alone), then star precision.
- silentselene 53191 (78.0 %): 18326 glyph snap tie (precision); the big groups are A (orbit stars), D (flash) and H
  (boss alpha 49794–50033).
- silentselene 54214 (78.9 %): 6277 = B + C (Yukari boundary ring z and colour on every bomb frame); then A and the
  6590 glyph tie.

## Tooling
- `drawdiff_src` (re/work/runs/q66: target/re66/drawdiff_src) = drawdiff of a src snapshot; writes groups.tsv with `--out`.
- TOOL (scratch src/session.rs `parse_events` + src/ascii.rs marker draw): start the port's ASCII frame counter at
  `asciicalc − poll` of the first `loader gauge` event when > 0 (`Q66NOTOOL` disables). Without it the marker blink
  (mode 2–4) is out of phase in slow-boot recordings.
- Probe (re/probe/session.inc): `TH08_SCANVM=1` with `TH08_SCAN="P0 P1"` adds `V` lines to scan.txt.

## Effect (scratch draw-log compare; trace identical in every run)
Identical frames by present (drawdiff `--key present`), per recording. dl3 base = target/drawgroups_dl3 (97bae01, the brief's
input); IMPL144 = target/drawgroups_dl3_144 (master 3a51a8d); after = run `f` (q66/f, q66/f_g, drawdiff_src). The src-snapshot
drawdiff compares a few more frames at the replay end (3007563 vs 3006629 in total); percentages are given per row. "merged" =
distinct (texture, rect, kind, fields) groups over all recordings, as in all_groups.tsv.

| recording | identical: dl3 base (97bae01) | IMPL144 (3a51a8d) | after (f) | groups base / 144 / f | trace |
|---|---|---|---|---|---|
| gensokyou_th8_uda212 | 87024 / 93106 (93.47 %) | 89684 / 93106 (96.32 %) | 91786 / 93125 (98.56 %) | 2450 / 2291 / 41 | T= |
| gensokyou_th8_udDK13 | 99466 / 106399 (93.48 %) | 103866 / 106399 (97.62 %) | 104182 / 106426 (97.89 %) | 242 / 123 / 117 | T= |
| gensokyou_th8_udfl24 | 97666 / 105320 (92.73 %) | 101930 / 105320 (96.78 %) | 103152 / 105336 (97.93 %) | 405 / 191 / 54 | T= |
| gensokyou_th8_udLR08 | 24540 / 25760 (95.26 %) | 25600 / 25760 (99.38 %) | 25735 / 25788 (99.79 %) | 794 / 782 / 9 | T= |
| gensokyou_th8_udM401 | 80655 / 87485 (92.19 %) | 82842 / 87485 (94.69 %) | 86192 / 87531 (98.47 %) | 430 / 231 / 87 | T= |
| gensokyou_th8_udMn38 | 88851 / 95304 (93.23 %) | 93952 / 95304 (98.58 %) | 94172 / 95341 (98.77 %) | 240 / 86 / 59 | T= |
| gensokyou_th8_udRo09 | 103731 / 115971 (89.45 %) | 105922 / 115971 (91.33 %) | 112010 / 116018 (96.55 %) | 437 / 77 / 48 | T= |
| gensokyou_th8_udS728 | 40152 / 46620 (86.13 %) | 41872 / 46620 (89.82 %) | 42200 / 46633 (90.49 %) | 157 / 22 / 14 | T= |
| gensokyou_th8_udSs12 | 89802 / 99503 (90.25 %) | 92685 / 99503 (93.15 %) | 98181 / 99532 (98.64 %) | 551 / 226 / 80 | T= |
| gensokyou_th8_udTh03 | 95173 / 103115 (92.30 %) | 98630 / 103115 (95.65 %) | 102258 / 103133 (99.15 %) | 637 / 361 / 71 | T= |
| gensokyou_th8_udTn06 | 92206 / 101026 (91.27 %) | 95346 / 101026 (94.38 %) | 99688 / 101054 (98.65 %) | 2464 / 2304 / 32 | T= |
| gensokyou_th8_udVn17 | 26761 / 35158 (76.12 %) | 27878 / 35158 (79.29 %) | 31863 / 35183 (90.56 %) | 206 / 120 / 24 | T= |
| gensokyou_th8_udZZ06 | 38560 / 45564 (84.63 %) | 40207 / 45564 (88.24 %) | 40508 / 45577 (88.88 %) | 152 / 18 / 10 | T= |
| silentselene_50559 | 110771 / 123222 (89.90 %) | 116325 / 123222 (94.40 %) | 116702 / 123239 (94.70 %) | 370 / 68 / 46 | T= |
| silentselene_50587 | 39626 / 44109 (89.84 %) | 41322 / 44109 (93.68 %) | 41699 / 44123 (94.51 %) | 160 / 24 / 16 | T= |
| silentselene_51100 | 108589 / 118800 (91.40 %) | 112500 / 118800 (94.70 %) | 116557 / 118816 (98.10 %) | 467 / 236 / 91 | T= |
| silentselene_51602 | 91377 / 97179 (94.03 %) | 94581 / 97179 (97.33 %) | 95874 / 97200 (98.64 %) | 1773 / 1501 / 34 | T= |
| silentselene_51670 | 38680 / 47898 (80.75 %) | 40186 / 47898 (83.90 %) | 40406 / 47911 (84.34 %) | 122 / 12 / 10 | T= |
| silentselene_52101 | 36318 / 42858 (84.74 %) | 37736 / 42858 (88.05 %) | 37737 / 42890 (87.99 %) | 21 / 6 / 9 | T= |
| silentselene_52279 | 112079 / 124372 (90.12 %) | 118422 / 124372 (95.22 %) | 121736 / 124422 (97.84 %) | 359 / 71 / 37 | T= |
| silentselene_52890 | 93107 / 101560 (91.68 %) | 100292 / 101560 (98.75 %) | 100629 / 101576 (99.07 %) | 217 / 56 / 29 | T= |
| silentselene_52939 | 100551 / 107586 (93.46 %) | 106305 / 107586 (98.81 %) | 106482 / 107603 (98.96 %) | 326 / 61 / 35 | T= |
| silentselene_53191 | 39619 / 50800 (77.99 %) | 41087 / 50800 (80.88 %) | 41307 / 50814 (81.29 %) | 125 / 17 / 13 | T= |
| silentselene_53334 | 101812 / 107706 (94.53 %) | 105016 / 107706 (97.50 %) | 105969 / 107724 (98.37 %) | 254 / 116 / 112 | T= |
| silentselene_53678 | 97207 / 104724 (92.82 %) | 100835 / 104724 (96.29 %) | 102823 / 104752 (98.16 %) | 2612 / 2340 / 34 | T= |
| silentselene_53768 | 95762 / 103893 (92.17 %) | 102551 / 103893 (98.71 %) | 102764 / 103926 (98.88 %) | 511 / 74 / 48 | T= |
| silentselene_54214 | 34216 / 43394 (78.85 %) | 35514 / 43394 (81.84 %) | 36956 / 43428 (85.10 %) | 2291 / 2283 / 8 | T= |
| silentselene_54440 | 72728 / 77114 (94.31 %) | 75800 / 77114 (98.30 %) | 76463 / 77134 (99.13 %) | 239 / 56 / 28 | T= |
| silentselene_54584 | 91894 / 98209 (93.57 %) | 96919 / 98209 (98.69 %) | 97202 / 98240 (98.94 %) | 270 / 117 / 110 | T= |
| silentselene_77236 | 36121 / 40616 (88.93 %) | 37796 / 40616 (93.06 %) | 38022 / 40638 (93.56 %) | 128 / 12 / 10 | T= |
| silentselene_78684 | 89709 / 102454 (87.56 %) | 96156 / 102454 (93.85 %) | 98736 / 102503 (96.32 %) | 414 / 91 / 60 | T= |
| silentselene_80415 | 102694 / 111383 (92.20 %) | 106021 / 111383 (95.19 %) | 109093 / 111404 (97.93 %) | 505 / 88 / 52 | T= |
| silentselene_80717 | 90617 / 98479 (92.02 %) | 97101 / 98479 (98.60 %) | 97684 / 98509 (99.16 %) | 549 / 275 / 242 | T= |
| silentselene_83085 | 40902 / 51383 (79.60 %) | 42319 / 51383 (82.36 %) | 42550 / 51407 (82.77 %) | 133 / 27 / 25 | T= |
| silentselene_83608 | 38173 / 42651 (89.50 %) | 39972 / 42651 (93.72 %) | 40370 / 42694 (94.56 %) | 150 / 20 / 12 | T= |
| silentselene_84604 | 98733 / 105908 (93.23 %) | 104312 / 105908 (98.49 %) | 104911 / 105933 (99.04 %) | 384 / 77 / 48 | T= |
| **36 recordings** | 2725872 / 3006629 (90.66 %) | 2849482 / 3006629 (94.77 %) | 2904599 / 3007563 (96.58 %) | 21545 / 14460 / 1755 (merged 3864 / 3159 / 517) | 36 × T= |

Golden (sessions/golden, read-only): IMPL144 25375 / 25633, 62 groups; f 25333 / 25633 (session compare; drawdiff_src 25346 /
25646), 62 groups. The q65 portcopy without any Q66 fix (bin_b0) gives the same 25333, and `Q66NOD` too: the 42 presents
(25473–25514, `-` missing + a K th08logo pair, the menu after the game) come from the scratch base's RE-Q65 H menu-load code, which
IMPL144 implemented differently. On master the Q66 fixes are expected to keep 25375; golden has no in-game group change. Trace T=.

Biggest remaining groups after f (by recordings × frames): etama#1 [160,224,176,240] pos (22 recordings) and [0,96,16,112] pos
(24) = op-128 orbit stars, ≈ 0.03 px (13 §2.6c precision class, skipped); ascii.anm#0 [0,208,8,220] / [104,208,112,220] missing
(22) at the replay end = menu after a game (RE-Q65 H, skipped; the same as in the base); `-` [32,16,416,464] colour, 12 recordings,
blocks of 119 presents every 492 presents in stage 5, alpha 0x3E vs 0x40, and the port draws it 4 presents longer (unchanged
since the base, not investigated → next RE cycle); stg5bg pos+z/rhw (24, small); 0.9-scale HUD glyph snap ties.

## CODE BUG list
Reference implementation of each fix: re/work/runs/q66/portcopy, look for `Q66NO<letter>` / `Q66 TOOL`. Functions are named as in master src.

| # | src function | change | spec |
|---|---|---|---|
| A | `src/effect.rs` `EffectManager::calc` (and the `set_frozen` mirror written at the end of `Player::calc` in `src/player.rs`) | test GameFlags bit 10 live (`gamemgr::flags() & F_DEATH`) when deciding whether a slot is updated, not the end-of-`Player_Calc` copy `self.frozen` | 13 §2.3 |
| B | `src/effect.rs` `draw_ring` | plain ring (amp == 0): vertex z = `e.centre[2]`; ellipse / wavy keep 0 | 13 §2.5 |
| C | `src/bomb.rs` `bomb_yukari` | slot 4 (k = 0, the type-0x23 re-spawn): after starting script 88 set the VM colour to 0xFFFFFFFF (the spawn colour written after `SetScript`); slots 5–7 unchanged | 14 §2.13 / §2.14 |
| D | `src/effect.rs` `ScreenEffect::step` | use t = timer.cur + timer.frac (f64, like x87) in the alpha / amplitude formulas of types 0, 1, 2/4, 3, 5/6, 7; keep the integer cur in the end / range tests; type 3: alpha 0 once cur ≥ dur | 13 §3 |
| F | `src/enemy.rs` `cancel_and_kill_bonus` | push `Popup::Bonus` only when `sum + kills != 0` | 12 §5.5 |
| H | `src/enemy.rs` `run_vms` | before the main VM run set `e.vm.color = e.main_color`, after it `e.main_color = e.vm.color` (main VM only) | 07 §10.1 step 5 |
| T | `src/session.rs` `parse_events` + `src/ascii.rs` marker draw (`frame`) | session tool: offset the ASCII frame counter by `asciicalc − poll` of the first `loader gauge` line when positive | 12 §5.10, data/drawlog/README |
