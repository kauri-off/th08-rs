# RE-Q61 — draw-log differences part 2 (side-panel "/", stage-layer projection vector, stage 6B ring)

Code: th08.exe decompile/disassembly (addresses per row). Draw logs of the 20 batch500 drawlog recordings
(th08_ext_replays/ref500) and sessions/golden, used only to find the differences. Scratch port `re/work/runs/q61/portcopy`
= q60 portcopy (all RE-Q60 fixes) + the fixes below (CARGO_TARGET_DIR=target/re61; src/ untouched; env Q61NOQ1 / Q61NOQ2 /
Q61NOQ3 / Q61NOQ3B / Q61NOTOOL turn a fix off). Runs: re/work/runs/q61/{fix,fix2}/ (dc.sh, sum.py, qd.py, pc.py), CHECKPOINT.md.
Before = re/work/runs/q60/fix2.

| # | recordings | original | spec | port |
|---|---|---|---|---|
| Q1 | 12 recordings + golden present 736: ascii glyph at (500.5, 167.5) / (500.5, 183.5) 8 px wide in the original, 16 in the port | `Hud_DrawPanel` 0x436D37 (point row) / 0x436E2C (time row): after the value (`Ascii_Printf` 0x402A80 returns the digit count, x += 13·digits) `Ascii_SetScale(0.5, 1.0)` (0x4B847C = 0.5), queue "/", `Ascii_SetScale(1, 1)`, x += 6.0 (0x4B497C), queue the second value. The queued string keeps its scale (+0x50), so the "/" sprite (16 px) is drawn 8 px wide | 12 §5.3 step 6 + ASCII table rows point / time | CODE BUG Q1 |
| Q2 | stg8bg (6 recordings, Extra) layer-1 ribbons and billboards: up to 7 px off, error grows with depth, first ribbon of the frame exact | `Stage_DrawLayer` 0x40A1B0: every D3DXVec3Project (thunk 0x47A355) gets input vector = local ebp−0x9C (`pV`, pushed at 0x40A59B …) with world = translation(point). The local is zeroed once per call (0x40A201) and overwritten by every ribbon with (sB − sA)·(1/len2d) (0x40B16B, `Vec3_ScaleInv` 0x40B1D1 only when len ≥ 1e-5) before the depth/visibility checks, so all later billboards/ribbons of the call are projected at point + previous ribbon direction. Scratch: udGu02 present 215 all 57 changed positions → equal | 06 §11.1, §11.3, new §11.4 | CODE BUG Q2 |
| Q3a | udSi123 stg4abg (stage 4A ribbons) | same as Q2 | 06 §11.4 | CODE BUG Q2 |
| Q3b | uddo18 (stage 6B): 120 stg7bg.anm#2 (later #3) quads missing, the original's `P strip 242` before the stage clear | `Stage_Calc` head for g_StageIdx 7: spawn fixed effect slot 12 type 0x40 + `AnmFile_SetScript` stgNbg script 11, requests 1–4 (script 11 again / save VM, script 12, interrupt 2 and restore pos interpolation + colour / interrupt 3, 4); type-64 update 0x427B50 (radius = F1, centre = VM pos); `Stage_DrawLow` step 4 calls the effect's custom draw (ring renderer 0x427450) at 0x40945D. Ring vertex colour = VM colour without the global ANM tint (`Anm_RingSetUV` 0x464B00) and z-write from the VM flags (`AnmManager_SetRenderState2D` in 0x464C60); visible here because the bomb tint is on at prio 6 | 06 §3 (+0xAE8), §4.1, §5, new §5.1, §10.2 step 4; 13 §2.5, §2.6 row 64 | CODE BUG Q3b, Q3c |
| — | remaining stg8bg / stg4abg first differences (colour only) | alpha-0 ribbon ends keep stack-residue RGB (06 §11.3, accepted approximation) | — | TOOL: compare treats two alpha-0 colours as equal (scratch drawlog.rs `col_close`) |

## Effect (scratch draw-log compare; trace identical in every run, 20/20 + golden)
- before (q60 fix2): all 20 first differences at gameplay frame 1 (gm 1; uddo18 gm 2); golden 736.
- fix (Q1 + Q2 + Q3b/c): golden 736 → 3677 (scene 2 frame 2991, etama.anm#1 one extra quad).
  52233 3826 (etama#1 pos 1), 54723 215 (ascii pos 7), 50948 226 (etama#3 pos 1), 79090 3699 (etama#2 64 pos),
  54840 216 (etama#3), udSe43 2362 (etama#1 1 extra), udVa11 216 (etama#3), uddo18 216 → 386 (stg7bg#0 1 missing / 1 extra),
  udFn15 6228 (etama#2 64 pos), udg342 / udI811 208/214 (ascii uv+colour, port-only digits), udKe02 / udSr22 208/215
  (player00 blend/size, spell replays), stg8bg/stg4abg recordings: residue colour only (fix2 below).
- fix2 (fix + alpha-0 colour tool), 7 recordings: Extra 51890 216 (ascii 1 extra), udLi14 / 53406 / 53020 / udFa11 582–583
  (times.anm#0 uv 1: the "times.anm#0[0,192,128,224]" vs [0,0,128,32] sprite, gm 450), udSi123 / udGu02 215 (ascii uv+colour,
  spell replays as udg342 / udI811).

## Next first differences (not analysed)
1. Spell replays (udg342, udI811, udSi123, udGu02; udKe02, udSr22): gameplay frame 1 ascii uv/colour + port-only digit
   sprites, and one player0N.anm quad (original additive 10 × 108, port alpha 70 × 108).
2. Extra stage gm 450: times.anm#0 sprite [0,192,128,224] (original) vs [0,0,128,32] (port) — 4 recordings.
3. etama.anm#3 single quad pos at gm 12–13 (50948, 54840, udVa11); etama.anm#2 64-quad pos (79090 gm 3567, udFn15 gm 6096);
   etama.anm#1 single extra / pos (udSe43 gm 2230, 52233 gm 3693, golden gm 2991).
4. uddo18 gm 253: stg7bg.anm#0 one 3D quad missing / extra; 54723 gm 1 ascii pos ×7; 51890 gm 2 ascii 1 extra.
