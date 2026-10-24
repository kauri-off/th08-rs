# RE-Q33 — draw parity remainder (golden draw log)

Runs: `re/work/runs/q33/` (scratch port `portfix/` = q32 portfix (fixes A–U) + fixes tagged `// q33 X`; diff vs q32
`q33_fixes.diff`, vs master base `q33_fixes_full.diff`; `rungolden.sh <tag>` = golden with draw log). Logic trace full
(25634 rows) for every tag. Final tag AH: fullrun_noclip 124707 full, selftest1 1922 full, demos 0–3 in sync.

Tooling (scratch, `re/work/runs/q33/`): `probe/` = copy of `re/probe` with `TH08_SCANEFF` (+`TH08_SCANEFF_ONLY`) in the
session scan: per calc row an `R` line (effect ring cursor 0x4ECE60 + live slots `slot:type:layer`) and a `C` line
(stage facing/up/normalised facing/+0x63C4 right/eye offset); `play_session.sh` / `scanrun.sh` use that DLL.
Port debug envs `Q33EFF` (Q33R ring dump per trace row, Q33S spawns), `Q33CAM`; `ringcmp2.py` (port poll + 1 = probe
poll); drawdiff `Q33TEX=<texture>` prints per-frame `Q33T` counts; `tally2.py`.

## 1. Draw divergences (golden), in the order fixed

| tag | draw group | cause | spec | after (same quads / identical frames) |
|---|---|---|---|---|
| U0 | — | q32 tag U | — | 98.06 % / 7426 |
| V | etama.anm#1 pos 107.6 k (list-B/C billboard size ±1 %) | camera-right +0x63C4 = normalise(cross(raw facing +0x63A0, up +0x63AC)); the port used view-matrix row 0 | 13 §2.6c, 06 §10.1 — SPEC CLARIFIED (operands; ≠ view row 0) | 98.96 % |
| W | — (sub-print) | D3DX8 `D3DXVec3Project` SSE path: (T·V)·P sums, `rcpps` + Newton 1/w, x87 viewport map; half = ((len·0.5)·w)·scale, 3D length | 13 §2.6c — NEW FACT | no count change |
| X | etama.anm#1 order / reordered 368 k | `Effect_Spawn` 512-slot budget is shared by the whole call (near-full pool: cursor wraps back); count ≤ 0; dummy return | 13 §2.2 — SPEC FIXED | 98.96 % (reordered 368 k → 47 k) |
| Y | type-32 auras | `Enemy_Free` sends the aura interrupt 3, does not free it | 07 §10.3b — NEW FACT | missing −400 |
| Z | etama.anm#1 list-C size | +0x63C4 only written by `Stage_SetCamera` (stage layers drawn / stage-camera render-state switch): stale during spell backgrounds | 13 §2.6c, 06 §10.1 — NEW FACT | 99.23 % |
| AA | etama.anm#1 type 17 point-reflected | `EffInit_Converge` angle = FloatRange(2π) − π | 13 §2.6 row 17 — SPEC FIXED | 99.41 % (etama#1 changed 21) |
| AB | ascii boss-name block / bonus line after the card, text.anm#0 extra 5 k | `Spell_End` calls `Spell_HideName`; also zeroes 0x4E3D28 first | 07 §11.1, 12 §5.8 — SPEC FIXED | 99.42 % (extra 69.8 k → 12.7 k) |
| AC | etama.anm#4 never drawn 2329, `-` missing 4.4 k | darkness mask of `Ascii_DrawPopups` (native-0 globals 0x4E3D28 alpha / 0x4E3D24 radius): 4 black rects + etama script 105 hole sprite, between small and large popups | 13 §4.1, 07 §12 native 0 — NEW FACT | 99.50 % |
| AD | ascii bonus-line attempts digit x 392.5 vs 399.5 | captures/attempts fields advance between digits only (no advance after the units) | 12 §5.8 — SPEC FIXED | 99.59 % |
| AE | ascii messages y 168 / bonus y 48 | message colours 0xFFE0B0FF / 0xFFC0B0FF, scale (0.9, 1.0), advance read at draw time (stays 13); slide x from the timer before its tick, lifetime test before the tick | 12 §5.5, §2.9 — SPEC FIXED | 99.68 % |
| AF | stg2enm/stg1enm pos (bob offset) | ops 54/58/62, 57/61 and the direction animation start scripts with `AnmFile_SetScript` (pos / pos offset kept) | 07 §9.3, op rows 54/57/62 — NEW FACT | 99.74 % |
| AG | enemy.anm#0 colour 3.6 k | step-7 colour2 flag is turned off every frame without a new flash (youkai tint did not clear) | 07 §10.1 step 7 (already right) — CODE BUG | 99.78 % |
| AH | front.anm#0 missing repaints, ascii missing 5 k | "Full Power Mode!" (`Hud_ShowMessage(0, 1)`, repaint counter 2) on reaching power 128 and on the full-power item | 08 §8.5 — SPEC FIXED | **99.85 % / 19204** |

## 2. Brief questions

1. **etama.anm#1 billboard positions** — not rounding: centres matched, sizes differed (wrong right vector, V; stale
   right vector, Z); the rest was slot order (X), type-17 reflection (AA). The D3DX SSE arithmetic (W) is documented
   but changes nothing at the log's 0.01 px resolution.
2. **ascii.anm#0 leftover** — the "unexplained glyph row" (present 19480, x 392.5 vs 399.5) is the bonus-line attempts
   digit (AD). Probe 4096-vertex truncation accounts for ~8 k of the remaining 9.7 k extra (108 frames with `P list 4`).
   Colour 7.7 k was mostly the message rows (AE) and the post-card plate (AB).
3. **etama.anm#4** = darkness mask hole sprite (AC). **text.anm#0** extra = boss card name left after the card (AB).
   **front.anm#0** = "Full Power Mode!" repaints (AH); left: 1-frame edge of the 60-frame StageStart counter at
   11041 / 25255 (285 quads, not chased). **`-` missing** = darkness rects (AC). **enemy.anm colour** = AG;
   **stg1enm/stg2enm pos** = AF.

## 3. CODE BUG list (port vs spec; all confirmed on scratch except 8b)

1. List-B/C billboards: right vector = normalise(cross(facing +0x63A0, up +0x63AC)), not view row 0 (13 §2.6c).
2. +0x63C4 is written only when `Stage_SetCamera` runs (stage layers drawn, or a stage-camera VM switch): keep the old
   value while the layers are skipped (13 §2.6c, 06 §10.1).
3. Billboard size: half = ((len·0.5)·sprite_w)·scale.x with len the 3D `Vec3_Length` (13 §2.6c); optional D3DX SSE
   projection arithmetic (no measurable effect).
4. `Effect_Spawn` / `Effect_SpawnWithParam`: one 512-slot budget per call, count ≤ 0 fills until the budget ends,
   dummy return when exhausted (13 §2.2).
5. `Enemy_Free`: aura gets interrupt 3, not freed (07 §10.3b).
6. `EffInit_Converge`: angle − π (13 §2.6, types 17/18/27).
7. `Spell_End`: 0x4E3D28 := 0, then `Spell_HideName` when the card was active (07 §11.1).
8. Darkness mask not implemented (13 §4.1; native 0 stores alpha/radius, 07 §12). 8b: `GameManager_StageStart` and
   `GameMgr_OnRemove` also zero the alpha (not scratch-fixed).
9. Bonus line: no advance after the captures' units digit (12 §5.8).
10. HUD messages: colours 0xFFC0B0FF (1, 3) / 0xFFE0B0FF (2, 4–6), scale (0.9, 1.0), advance 13·0.9 (the advance is
    read at draw time, 12 §2.9); types 2 "Supernatural Border!!" and 3 "CherryPoint Max!" missing; popup slide uses the
    timer before the tick, lifetime test before the tick (12 §5.5).
11. ECL ops 54/58/62, 57/61 and direction animations must use `AnmFile_SetScript` (keep VM pos / pos offset) (07 §9.3,
    op rows).
12. Enemy step 7: colour2 flag off on every non-flash frame (07 §10.1 step 7).
13. "Full Power Mode!" message missing at power 128 / full-power item (08 §8.5).

Remaining golden draw differences after AH (99.85 %): ascii.anm#0 extra 9.7 k (≈8 k probe truncation) and changed
7.4 k (scattered), etama.anm#1 extra 2.1 k (truncation frames), `-` 1.8 k, player00 pos 847, etama.anm#3 pos 777,
etama.anm#2 384, stg2enm 302, ascii.anm#2 272, times.anm uv 179, front.anm#0 missing 285 (StageStart counter edge).
