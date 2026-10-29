# RE-Q60 — draw-log differences part 1 (replay-menu plate, menu draw on game start, depth-clear colour, replay fps text)

Code: th08.exe decompile/disassembly (addresses per row). Draw logs of the 20 batch500 drawlog recordings
(th08_ext_replays/ref500) and sessions/golden (--to 700), used only to find the differences. Scratch port
`re/work/runs/q60/portcopy` = q59 portcopy (master b93b569 + AT/AU/AV) + the fixes below (CARGO_TARGET_DIR=target/re60; src/
untouched; env Q60NOQ1A / Q60NOQ1B / Q60NOQ2 / Q60NOQ3 / Q60KEEPK turn a fix off). Runs: re/work/runs/q60/{base,fix1,fix2}/
(dc.sh, sum.py), CHECKPOINT.md.

| # | recordings | original | spec | port |
|---|---|---|---|---|
| Q1a | spell replays udg342, udKe02, udSi123, udI811, udGu02, udSr22: menu presents 120–127, port extra `title01.anm#2` [128,112,256,160] | `MainMenu_Replay` 0x46E136 stage confirm (sub-state 2 → 3) and every speed-cursor change: interrupt 21 to VMs 108, 109; VM 110: 21 if header +0x7C < 0 (normal replay), otherwise **no interrupt**, only the alpha byte +0x1F3 := 0; then 20 to VM 108 + cursor. The port sends 21 to VM 110 as well (its `alpha_time` 0 → 96 shows the plate), and its every-frame alpha := 0 is overwritten by the VM run that follows | 12 §2.8 (row "stage chosen → speed" + sub-state 3 note) | CODE BUG Q1a |
| Q1b | next difference: normal replays (14 recordings) menu present 132/133, port draws the whole replay screen (≈190 ascii + title01 + select00 copy), the original only the fps counter; golden present 686 (character confirm) the same | Game-start confirms return 0 from the handler: Character 0x46A625 (screens 5/13), PracticeStage 0x46ADB0, SpellPracticeCard 0x46BBC0, Replay 0x46E136 speed confirm. `MainMenu_Calc` 0x467399 still runs RunRange + description VM and returns 0 → the calc node is removed (00 §3) → on-remove 0x471662 releases anm 0x14/0x16 and surface 0 and removes the draw node. Nothing menu-side is drawn in that frame | 12 §2 "Handlers that return 0" (new); 15 §3.8 Start | CODE BUG Q1b |
| — | next difference after Q1 (all 20 + golden): loading frame (scene 2 frame 1) port logs `K front.anm#1 front.anm#0 …` | Boss name plate `Hud_SetBossNamePlate` copies sprite pixels with 0x467040 = flush + `D3DXLoadSurfaceFromSurface` between two textures: no device call, never in a reference log | specs/data/drawlog/FORMAT.md §3 K (note) | TOOL (port log only) |
| Q2 | udSi123, udKe02, udGu02, udSr22 (gm 1 call 3: `C z 00000000` vs ff000000), udFn15 (gm 2 call 0: `C z ff404060` vs ff000000) | Both z-only clears pass a colour argument that D3D ignores (no pixel effect): `GameMgr_Calc` 0x439BC7 `Clear(0,0,2, g_Stage+0xAF4 = fog colour (global 0x4E4B24), 1.0)` — default 0xFF000000 until the stage's first fog op (udFn15: ff404060 from gm 2); `Stage_DrawLow` 0x409200 step 5: `Clear(Z, +0x830)` in both z-only branches (0 for stages without op 13) | 11 §3.1 "Per-frame depth clear"; 06 §3 Stage_DrawLow step 5 | CODE BUG Q2 (log only, not visible) |
| Q3 | 52233, 50948, 79090, udSe43: port 2 ascii quads at 383.5,447.5 colour ffffffd0 from gm 3, original from gm 13/11/15/14 | Replay-playback fps text "%2d" of fps_now (`Fps_Update` 0x446F53, 15 §3.3). Its buffer 0x17CF2B8 is zero after boot and is written only when the real-time 500 ms meter window closes during replay gameplay (bits 2 and 3, draw-time call). The probe's fast mode virtualises only `Timer_Now`, not `timeGetTime` / QPC, so the first refresh after boot is host-dependent: gm 1 (udg342), 11, 13, 14, 14 (54723), 15 | 15 §3.3 (buffer + first-refresh note) | port right (not reproducible); TOOL: draw-log compare skips the text like the fps counter |

## Effect (scratch draw-log compare, 20 recordings; trace identical in every run)
- base (q59 behaviour: Q60NOQ1A=1 Q60NOQ1B=1): first difference = spell replays VM 110 plate (presents 120–127, 6),
  normal replays speed-confirm frame (132/133, 14). Golden (--to 700): 686.
- fix1 (Q1a + Q1b): every recording moves to its loading frame (127–134, K copy); udSi123 to gameplay 215 (stg4abg
  3D quad positions, gm 1: next group, not analysed). Identical frames +1 (normal) / +7 (spell). Golden 686 → 687 (K copy).
- fix2 (+ K not logged, Q2 colour argument, Q3 tooling skip): all 20 reach gameplay frame 1 (uddo18 gm 2); no `C z`
  and no (384, 448) text difference left. Next groups (not analysed): side-panel ascii glyph at (500.5, 167.5) 8 px wide
  (ref) / 16 px (port), 12 recordings; stg8bg 3D quad positions/colours (6); stg4abg (udSi123); stg7bg.anm#2 strip
  missing (uddo18, gm 2). Golden (--to 1000): 686 → 736 (same side-panel glyph group).
