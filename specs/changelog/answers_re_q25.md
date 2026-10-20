# RE-Q25 — answers to IMPL26 / IMPL27 questions (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Evidence = binary (addresses) and the reference session draw
logs in `sessions/` (golden, gameover_save). Run scratch: `re/work/runs/q25/` (`inputmap.py` = keyboard-map extractor).

## IMPL26

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 mode 4 | NEW FACT | `draw_mode` is written only by ANM op 83 (56 uses, all value 2); `DrawByMode` has one caller (`Player_DrawBulletsHit`), so modes 4/5 never go through the dispatch. Billboards come only from the effect manager's direct calls; the port's "draw nothing" branch is unreachable | 05 §9 | dispscan +0x200; callgraph |
| 2 mode-5 depth | CODE BUG | vertex z = projected z (`EmitQuad` never rewrites it); z ≈ 0.9994 with the 2D camera | 05 §9 | 0x463D60; golden draw_00005 capture.anm `Q … 0.9994` |
| 3 difficulty sprite | CODE BUG | 0x120 only for spell-practice ids ≥ 205; below, 0x11B + difficulty (the card's). Same rule in the pause menu VM +0x1284 (`ascii.rs` has the same bug) | 12 §3, §5.1 | 0x4037B0, `Hud_Load` 0x4390EE |
| 4 replay exclusion | SPEC FIXED + CODE BUG | +0x7B = practice flag of the replay info block; replays never set GM bit 0. Calc adds lives/bombs whenever bit 0 is clear (all replays), night bonus always on 6B, penalty always; draw hides lines for practice and practice replays, penalty line only below Extra outside practice. Full layout, colours, integer percentages and integer multipliers now tabled | 12 §5.6 (rewritten) | 0x43C322, 0x43826B, `Hud_Update` 0x435900 |
| 5 MSG op 14 | CODE BUG | `ScreenEffect_Register(4, p0 = 442 (EDX), 0xFFFFFF, 0, 0, prio 0x15)` + repaint counter 442; type 4 = calc 0x45B760 (as type 2), full-screen draw 0x45BB50 | 10 §5.1 (unchanged), 13 §3 | disasm 0x4351BB |
| 6 Gui face −1 | CODE BUG | only VM 0 is skipped; VMs 2/4 always restart | 12 §5.8 (unchanged) | 0x415D60 |
| 7 continue default | CODE BUG | "Yes" only in spell practice with g_Gui flags bit 9 clear (0x405260) | 12 §4 (unchanged) | 0x404890 |
| 8 menu viewport | CODE BUG | playfield viewport + capture VM | 12 §3/§4 (unchanged) | gameover_save draw_00001 line 364204: `T capture.anm`, `V 32 16 384 448` |
| 9 night clock | NEW FACT + CODE BUG | op 179 = script 2 sprite clock, op 180 / Hud_Load alpha := 0; run/draw order | 12 §5.7, 07 §7 op 179/180 | 0x439007, 0x4390D6 |
| 10 ASCII restarts | NEW FACT + CODE BUG | +0x17C4.. are the four **boss "ENEMY" position markers** (missing in the port; drawn every boss fight); +0x1520 is run but never drawn | 12 §5.9 step 1, new §5.10 | `Ascii_DrawStrings` tail 0x402E9C, `EnemyMgr_Calc` 0x42C660, op 127, `Enemy_Free`; golden draw_00004 `R 199.5 463.5 247.5 479.5 0 0.9375 0.1875 1 60ffffff` |
| 11 stats+0x29 | SPEC FIXED | continue count (incremented by each continue); the route test is "any continue used" | 10 §4 step 3 | 0x404890, 0x43396D |
| uncited 0x80 + adv | NEW FACT | stage-clear VM sprite 0x80 + clock advance; 6A/6B/Ex clear its ip; bonus-panel VMs listed | 10 §9 | 0x433DB3 op 9 |
| uncited "+%d" y | NEW FACT | "Spell Card Bonus!" x 105 y 80; "+%d" y 96, x (384 − len·28)/2 + 32; popups use the clip flag | 12 §5.5 | 0x438A89 |
| uncited (512, 464) | NEW FACT | blank patches list incl. the unconditional (512, 464) one; full repaint also while tile VM 13's script runs (first 200 frames) | 12 §5.3 | 0x43625D; golden 199 full-repaint frames |
| uncited title/clock viewport | NEW FACT | everything drawn between `Stage_DrawLow` and `Hud_DrawPanel` inherits the playfield viewport (popups, gauge readout, Gui, dialogue, boss UI) | 12 §5.3 | golden draw_00004 V sequence |
| uncited popup viewport | NEW FACT | same rule (13 §4 draw at prio 14) | 12 §5.3 | — |
| uncited "(p4)" | NEW FACT | "(p4)" only when p4 > 0; p3 < 1 → "+0" | 13 §4 | 0x403460, 0x403600 |
| uncited face names | NEW FACT + CODE BUG | per-stage table; spell practice loads no player faces and the `…sp` cut-in files by card id | 10 §3 | 0x414590 |
| uncited slow mode / font / filter / div by 0 | — | old gaps, no change | — | — |
| uncited default_2d | — | port test helper, no original counterpart (the original 2D camera is 05 §9 `Camera_Set2D`) | — | — |

## IMPL27

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 key repeat | CODE BUG (spec clarified) | the Supervisor poll uses exactly the 15 §3.3 rule: test before increment → repeat on the 33rd, 41st, … unchanged frame | 11 §7, 12 intro | 0x445453 |
| 2 replay scores | CODE BUG (spec clarified) | only the highest earlier existing record gets +0 := score | 15 §3.2 | 0x452830 |
| 3 SP difficulty | CODE BUG | `Spell_DifficultyOf(id)` (Last Words → 1) | 16 §6.2 (unchanged) | 0x46BBC0 |
| 4 volume steps | CODE BUG | ±4 edge then ±1 (edge or counter > 29); SE 0x1D preview every 50 frames | 15 §1.3 (unchanged, RE-Q22) | options_cfg session |
| 5 back from practice | CODE BUG (spec clarified) | 0 → 8 → 9 → 11 one per frame; 0 → 15 → 16 → (17 same frame) | 12 §2.2, 16 §6.1 | handler return values |
| 6 screen entry | SPEC FIXED | handlers returning 2 run the new screen in the same frame (and RunRange twice): char confirm → 11/16, 11 cancel → 9, 16 → 17, 17 cancel → 16; all others next frame. Delayed-switch / input-start timing tabled | 12 §2 | 0x467399 + every handler |
| 7 replay stage row | SPEC FIXED | cursor := first recorded stage on the file confirm; highlight uses +0xC290 which is updated only in sub-state 2 → one stale frame | 15 §3.8 | 0x46E136, 0x46EF8D |
| 8 SP replay flags | CODE BUG | playback sets bits 3 and 14 only; bit 0 is clear (cleared by the top menu's Replay item / menu load) | 15 §3.8 | 0x4674E0, 0x46E136 |
| 9 screen 16 | CODE BUG | shot change zeroes +0x1442C; Bomb stores the row into g_StageIdx (bit 14 kept) | 16 §6.1 | 0x46B174 |
| 10 title music | CODE BUG | not after scene 5 | 00 §3.2 (unchanged) | 0x470E10 |
| 11 clamp | SPEC FIXED | top menu stores > 3 → 2 but the difficulty entry block overwrites it with > 3 → 1 next frame; port right | 12 §2.2, §2.3 | 0x4674E0, 0x46A071 |
| 12 name entry | CODE BUG | four independent direction tests | 16 §2.2 (unchanged) | 0x45621E |
| 13 resulttext slot | CODE BUG (harmless) | slot 22 | 00 §3.2 (unchanged) | — |
| 14 archive edge | CODE BUG (harmless) | non-edz data used as-is (whole buffer) | 01 §6 (unchanged) | 0x43E390 |
| 15 on-register failure | NEW FACT | node always linked; result only returned | 00 §3 | 0x43C880 |
| 16 mid-chain effects | NEW FACT + CODE BUG | linked at once; registrations from prio < 3: `GameMgr_Calc` demo-end fade (type 2) and `Loading_StartDarken` → first calc same frame | 13 §3 | 0x43C880, callers of 0x45B8B0 |
| spec error | SPEC FIXED | 15 §2.1 now cites 01 §2 (cipher) and 01 §3 (LZSS) | 15 §2.1 | — |
| uncited LZSS encoder | NEW FACT | `Lzss_Compress` 0x473CB0; byte-identical output not required | 01 §3 | callgraph |
| uncited keys | NEW FACT | full VK and DIK keyboard maps (arrows, numpad incl. diagonals, X, Shift, …) | 15 §1.1 | 0x43D970 disasm (`inputmap.py`) |
| uncited Music_Fade | NEW FACT + CODE BUG | linear in dB (0 → −50 dB) over trunc(secs)·60 frames, then stop; BGM volume curve is 1 − (1 − v/100)² (not the SE cube) | 02 §4, 13 §1.3 (corrected) | 0x4480F8, 0x45E3F0, 0x472B50, 0x472840 |
| uncited SE ids | — | callers test ≥ 0; Sound_Play has no range check; port equivalent | 13 §1.2 | `Player_InitShot` 0x44FB70, 0x45D550 |
| uncited date | CODE BUG (minor) | local time | 16 §2.2 | 0x456938 |
| uncited fade music 30 | SPEC FIXED + CODE BUG | Quit does not touch WAV music (MIDI: stop/open 30/play) | 12 §2.2 | 0x4674E0 |
| uncited +21 delay / start frame 8 / option cancel / replay 30 frames + cursor 4 | NEW FACT | tabled (press P → switch call P + 20, new screen P + 21) | 12 §2, 15 §1.3 | handlers |
| uncited FastForward | NEW FACT | one function: dialogue rule first, then fast rule | 15 §3.3 | 0x452490 |
| uncited cfg clamp | SPEC FIXED + CODE BUG | all 60 bytes copied, no clamp | 11 §3.4 | 0x452D60 |
| uncited loading holds / fps phase / gauge limits | — | loader-thread timing (no fixed value); fps phase as 15 §3.3; the original keeps the last limits but the readout is hidden, harmless | — | — |

## CODE BUG list (next implementer brief)

Priority (draw log / sync):
1. **Boss "ENEMY" markers** (12 §5.10): four ASCII VMs, positions from `EnemyMgr_Calc`, interrupts from op 127 / `Enemy_Free` / `Player_Load` / player on-remove, draw at the end of the prio-20 ASCII pass.
2. Mode-5 vertex depth = projected z (05 §9).
3. MSG op 14 flash = ScreenEffect type 4 (442 frames, prio 21) (10 §5.1, 13 §3).
4. Screen effects registered from `GameMgr_Calc` (demo-end fade) get their first calc in the same frame (13 §3).
5. Stage-clear panel per the rewritten 12 §5.6 (calc vs draw conditions, colours, layout, integer maths).
6. HUD viewport: everything before `Hud_DrawPanel` in the playfield viewport; blank patch (512, 464) and the tile-13 full-repaint rule (12 §5.3); spell-bonus line positions (12 §5.5).
7. Pause / continue menu: playfield viewport + capture VM (12 §3/§4); difficulty sprite 0x120 only for ids ≥ 205 (pause menu and HUD); continue "Yes" needs Gui flags bit 9 clear (12 §4).
8. Gui player show with face −1 still restarts VMs 2/4 (12 §5.8).
9. Night-clock ops 179/180 and the player-overlap fade (12 §5.7).
10. Spell-practice face files and no player faces in spell practice (10 §3).
11. Key repeat on the 33rd unchanged frame (11 §7).
12. Menu flow: same-frame switches for return-2 handlers; practice return path one screen per frame; 16 → 17 same frame (12 §2, §2.2); replay row lag (15 §3.8).
13. Replay: only the highest earlier record gets the score (15 §3.2); spell-practice replays without flags bit 0 (15 §3.8); full 60-byte config copy (11 §3.4).
14. Spell-practice difficulty `Spell_DifficultyOf` (16 §6.2); screen 16 shot change / Bomb row (16 §6.1).
15. Option volume ±4/±1 and preview SE (15 §1.3); Quit does not fade WAV music (12 §2.2); `Music_Fade` dB-linear curve, BGM volume square curve (02 §4, 13 §1.3).
16. Title music not after the result viewer (00 §3.2); name entry four directions (16 §2.2); local-time date (16 §2.2).
17. Harmless: resulttext slot 22; archive non-edz data as-is.
