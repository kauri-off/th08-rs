# RE-Q63 — draw-log differences, part 4 (changelog)

Brief: docs/briefs/RE_Q63_BRIEF.md. Runs: re/work/runs/q63 (CHECKPOINT.md; portcopy = q62 portcopy + scratch fixes,
CARGO_TARGET_DIR target/re63; before = q62/fix4). Every fact below is in the numbered spec section named.

## Answers

| Q | difference | original | spec | verdict |
|---|---|---|---|---|
| 1a | ascii.anm#2 [0,224,160,256] missing (udSi123 / udI811 / udGu02 / udSr22 present 305/306, 54723 later) | `Hud_DrawPanel` 0x43625D step 4: `Draw2DSnap` VM impl+0x5484 (`ascii.anm` script 0x1A: sprite 282, 160 × 32 at (488, 88)) when dirty(lives or bombs) and GameFlags bits 7–8 = 1 and `Gui_IsCardActive`; loaded on a fresh start, run every frame by `Hud_Update` | 12 §5.3 step 4 (sprite, latch rule for full-repaint ports) | CODE BUG Q1a |
| 1b | udg342 213 ascii#0 missing 3, 54723 215 ascii#0 pos 7 (gauge readout one frame late) | loader race: `loader gauge poll B present P` with P = present(B) − 1 means the setup ran inside the frame of poll B − 1 after its input poll, before or after `Ascii_Calc` — not recorded (udSi123 same case, matches B) | 12 §5.9, drawlog README | recording limitation, port right |
| 1c | 51890 216 ascii#0 extra 1 (x 409.5) | replay fps "%2d" of a stored fps ≥ 100 ("120") gives three glyphs | 15 §3.3 (fps text) | TOOL (fps skip box to x 426) |
| 1d | ascii digits uv/pos (79090 3702, 53020 5854, udFa11 6499, udSe43 4213) | `Item_CollectTimeOrb` (ItemMgr_Calc inside BulletMgr_Calc, prio 14) adds the spell bonus +8000 at once, after `Spell_Calc` (Gui_Calc prio 12); `Spell_Draw` (prio 15) reads g_Gui +0xFC → the bonus line shows it the same frame | 08 §8.5 (time orb) | CODE BUG Q1d (logic equal, display one frame late) |
| 2a | 54840 286 stg1bg colour (ref 3f3f6f, port 00005f) | `BombDraw_Youmu` 0x412300 / `BombDraw_YoumuLS` 0x412FA0: `Bomb_DrawTint` 0x80404040 every frame, then a second call 0x80000030 from T ≥ 70; the stage request averages them | 14 §2.11, §2.12 | CODE BUG Q2a |
| 2a′ | 54840 286 (after Q2a): untextured alpha-0 playfield rect, viewport 640 × 480 (ref) vs playfield (port) | `Screen_FillRect` 0x45B1E0 sets no texture and no viewport; pre-transformed vertices use the viewport only for clipping | — | TOOL (viewport difference ignored for a quad inside both viewports) |
| 2b | udVa11 420 stg5bg missing 2 / extra 2 | quads with a vertex behind the near plane (x ≈ 1.2·10⁶): the matcher's centre moves by > 64 px, so the Q62 far-vertex tolerance is never reached | 13 §2.6c (precision) | TOOL (centre from in-depth-range vertices) |
| 2c | uddo18 426 stg7bg missing 1 / extra 1 | same as 2b | — | TOOL |
| 2d | 53406 4659 stg8bg colour 42 | ribbons go through `AnmManager_DrawCustomQuad` 0x464DD0 (SetRenderState2D + PushQuad of the given vertices): the global tint is not applied to their vertex colours | 06 §11.3, 05 §12.1 | CODE BUG Q2d |
| 3a | 50948 2371 etama#1 missing 1 | type-51/63 particle with max y = 16.00 = viewport top: drawn by the original's f32 cull, covers no pixel (centres on integers, top-left rule) | 05 §9 step 3 | TOOL (off_vp: touching near edges = off) |
| 3b | udFn15 4110 etama#1 missing 1 | same as 3a | 05 §9 step 3 | TOOL |
| 3c | udKe02 214 etama#1 pos 1 | three 70–140 px software-projected quads differ ≤ 0.04 px, alternating sign, no drift (D3DXVec3Project precision) | 13 §2.6c | TOOL (ztest=le quads: tolerance 4·10⁻⁴ × quad size) |
| 3d | udLi14 984 player01 colour 1, z | Alice laser trail `ShotDraw_Trail` 0x450AD0: VM pos (x, y, z) := trail[c] (z 0.44), youkai max → RGB (FF, 40, 40), only alpha restored → the red stays in the VM colour | 14 §4 (trail draw) | CODE BUG Q3d |

## Effect (scratch draw-log compare; trace identical in every run)
Before = q62/fix4, after = q63/fix (bin_q3d: Q1a, Q1d, Q2a, Q2d, Q3d + tool rules). Table filled from `sum.py base fix`.

| recording | before (q62 fix4) | after (q63 fix) | next first difference |
|---|---|---|---|
| ne_52233 | 3826 etama#1 pos 1 | 3826 | 13 §2.6c accepted (skip) |
| ne_54723 | 215 ascii#0 pos 7 | 215 | gauge loader race (Q1b) |
| ne_50948 | 2371 etama#1 missing 1 | 3462 | etama#1 pos 1 |
| ne_79090 | 3702 ascii#0 uv 2 | 5021 ¹ | ascii#0 uv/colour 2 |
| ne_54840 | 286 stg1bg colour 33 | 316 | etama#2 missing 36 / extra 36, pos/uv 12 (Youmu bomb) |
| 8_udSe43 | 4213 ascii#0 pos/uv 2 | 4674 ¹ | ascii#0 pos/uv/colour/state 2 |
| ne_51890 | 216 ascii#0 extra 1 | 7835 | ascii#0 extra 3, changed 3 |
| 8_udLi14 | 984 player01 colour 1 | 7299 | ascii#0 uv 2 |
| ne_53406 | 4659 stg8bg colour 42 | 4679 | stg8bg pos 47 |
| ne_53020 | 5854 ascii#0 uv 2 | 6942 | ascii#0 extra 2, changed 2 |
| 8_udVa11 | 420 stg5bg missing/extra 2 | 1286 | etama#1 pos 2 |
| 8_uddo18 | 426 stg7bg missing/extra 1 | 1487 | etama#1 pos 1 |
| 8_udFn15 | 4110 etama#1 missing 1 | 7176 | ascii#0 uv 2 |
| _udSi123 | 306 ascii#2 missing 1 | 306 | eff04a#1 pos 1 (spell practice gm 173) |
| 8_udg342 | 213 ascii#0 missing 3 | 213 | gauge loader race (Q1b) |
| 8_udKe02 | 214 etama#1 pos 1 | 299 | eff06#0 extra 1 (gm 173) |
| 8_udI811 | 305 ascii#2 missing 1 | 305 | eff05#0 pos 1 (gm 173) |
| 8_udGu02 | 306 ascii#2 missing 1 | 306 | eff08#0 extra 1 (gm 173) |
| 8_udFa11 | 6499 ascii#0 uv 2 | 7094 | ascii#0 missing 6, pos 4 |
| 8_udSr22 | 306 ascii#2 missing 1 | 306 | eff06#0 extra 1 (gm 173) |
| golden (--to 8000) | 4439 | 4439 | 13 §2.6c accepted |

Trace identical (T=) in all 20 + golden; identical-frame counts up in 17 recordings.
¹ The batch ran with the first `off_vp` touching rule (no margin) and gave 79090 1900 / udSe43 3567: a particle at max y
16.00 in one log and 16.01 in the other (2-decimal log rounding) fell on different sides. With the 0.05 px margin
(final tool rule) `drawdiff` on the same logs gives 5021 / 4674; the other rows are unaffected (rule only widened).
The spell-practice group (udSi123 / udI811 / udGu02 / udSr22 / udKe02, gm 173) now first differs in the boss
effect files eff04a–eff08 at the card start.

## CODE BUG list (src function → change → spec)
- Q1a `src/hud.rs` `Hud` (load / update / draw_panel, `Kept`): add VM impl+0x5484 = `ascii.anm` script 0x1A, set on a
  fresh start only (kept across a continuing stage like `diff_vm`), run every `Hud_Update` before the difficulty VM;
  in `draw_panel` after the life/bomb icons draw it (`draw_mode` 0 = Draw2DSnap) while the latch is set. Latch: on a
  frame where the original repaints the lives or bombs row (dirty field ≠ 0 or full repaint), latch :=
  (GameFlags bits 7–8 == 1 and a card is active); the port needs the lives/bombs dirty fields for this (writers 12 §5.3
  table) — drawing whenever the condition holds differs only when op 176 sets bit 7 mid-card without a repaint
  (12 §5.3 step 4).
- Q1d `src/item.rs` time-orb collection (`st.spell_bonus_orbs += 1`) + `src/enemy.rs` `EnemyManager::step` + `src/gui.rs`
  `Gui` bonus copy: apply `Spell_AddBonus(8000)` at collection inside the bullet/item calc (not at the next
  `EnemyManager::step`), and let the Gui draw read the live bonus (+0xFC) instead of the copy taken in `Gui::calc`
  (08 §8.5). Logic is unchanged (the next decay uses the raised R either way); only the frame's bonus line changes.
- Q2a `src/bomb.rs` bomb draw, kind 6 (Youmu / Youmu LS): `tint(0x80404040)` every frame, then also `tint(0x80000030)`
  when T ≥ 70 (two requests, averaged by `request_tint`), instead of one call with the colour switched (14 §2.11/§2.12).
- Q2d `src/stage.rs` `Stage::ribbon`: vertex colours without `anm.tinted` (05 §12.1, 06 §11.3).
- Q3d `src/pshot.rs` `draw_trail`: draw at z = trail[c][2] (not 0.4), and after the loop restore only the alpha byte,
  keeping the youkai-max red RGB in the VM colour (14 §4).
- TOOL `src/drawlog.rs`: `is_fps` x range to 426 (3-digit fps); `items()` matching centre from the vertices with
  0 ≤ z ≤ 1 when some but not all are in range; `off_vp`: x1 ≤ vx + 0.05, y1 ≤ vy + 0.05, x0 > vx + vw − 1.05 or
  y0 > vy + vh − 1.05 → off;
  `compare`: viewport mismatch ignored when the quad lies inside both viewports; quads with `ztest=le` get position
  tolerance max(tol, 4·10⁻⁴ × quad size). `off_vp` uses a 0.05 px margin on the touching edges (log rounding). Scratch versions behind `Q63NOTOOL` in re/work/runs/q63/portcopy.

## Open / next
- 12 §5.9 loader race: the probe should log the ASCII calc count at `loader gauge` so a session pins it exactly.
- Next first differences: the last column above. Suggested next group: the gm-173 spell-start effects (eff04a/05/06/08,
  5 recordings), 54840 316 Youmu bomb etama#2, 53406 4679 stg8bg pos, the late ascii#0 group (51890 / udLi14 / 53020 /
  udFn15 / udFa11 / 79090 / udSe43).
