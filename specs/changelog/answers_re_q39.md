# RE-Q39 — SPEC-GAP batch 5: music room (2026-10-24)

Brief: docs/briefs/RE_Q39_BRIEF.md; markers: docs/specgaps/batch5_musicroom.txt (13, src/musicroom.rs). Runs:
re/work/runs/q39/ (CHECKPOINT.md, glyph7f.png = ascii.anm sprite 126). Evidence: decompile (fn.sh / fnc.sh) of
`MusicRoom_Register` 0x449254, `_Ctor` 0x449331, `_Calc` 0x449399, `_StateIntro` 0x448A65, `_StateInput` 0x448B16,
`_Draw` 0x449487, `_Init` 0x44970F, `_OnRemove` 0x449C0A, `Text_RenderToVm` 0x4663B0 / `Text_Render` 0x466310 /
`Text_RasterBold` 0x43F9C8, `Music_PlayFile` 0x447F21, `AnmFile_SetScript` 0x4069F0; warning table 0x4C7AAC and
"%2d." 0x4B6E8C read with rd.py; music00.anm / text.anm scripts from anm_dis.txt; ascii.anm sprites (anmspr.py, PNG).
No original run: every answer follows from the code and data.

## Changed spec sections
| spec § | change |
|---|---|
| 16 §5.2 | object zeroed at register (inert VMs), +0x78 := 0 at init; COLORREF colours; mode arg ≠ outline colour; row scripts; full draw order, ASCII cursor 0x7F (▶) and numbers in the row VM colour; title drawn at forced pos/white |
| 16 §5.3 | Calc order (exit returns 0 → node removed, no VM run / draw), sub-frame reset on state change, +2 per frame in state 1, intro → state 1 at sub-frame 8; independent key tests in order, multi-bit edges, Shot resets the sub-frame; reveal timing; comment colour; script shapes; full 7-line warning table |
| 02 §4 | `Music_PlayFile`: MIDI / WAV paths, heard flag only in modes 1–2 (not replay/demo) |
| 10 §6 | `Text_RenderToVm` outline argument is only the ±1/±2 mode flag; outline always black; colour = COLORREF |
| structs.md | MusicRoom object layout |

## Markers
| marker (src/musicroom.rs) | original | spec | port |
|---|---|---|---|
| 28 warning lines | 7 strings at 0x4C7AAC; line 1 and 2–6 start with U+3000; line 4 = one U+3000 | 16 §5.3 table | CODE BUG (lines 2–6 empty, line 1 missing its leading space) |
| 100 colour format | `SetTextColor` → COLORREF 0x00BBGGRR | 16 §5.2, 10 §6 | CODE BUG (read as 0xRRGGBB: R/B swapped) |
| 138 outline mode | 4th arg only compared with −1; values ≠ −1 → ±2 px; outline always black | 10 §6, 16 §5.2 | offset path right; CODE BUG: port draws the outline in the 4th-arg colour |
| 179 initial playing | Init +0x78 := 0 | 16 §5.2 | right |
| 217 flag 0 in mode 0 | `Music_PlayFile` returns −1 in mode 0, marks nothing; modes 1/2 mark unless replay/demo | 02 §4, 16 §5.3 | CODE BUG (port marks in every mode) |
| 230 comment colours | COLORREF 0xFFE0C0 = RGB (192, 224, 255), black ±2 px outline | 16 §5.3 | CODE BUG (white) |
| 247 sub-frame on state 1 | Calc: state changed → 0; intro switches at sub-frame 8 (9th call); state 1 +2 per frame | 16 §5.3 | CODE BUG (keeps counting, switches at 7, +1 per frame → reveal later and every 2nd frame) |
| 262 key order | Up, Down (independent), reveal, Shot\|Enter (sub-frame := 0), Bomb\|ESC exit, else Ctrl, R; multi-bit edge | 16 §5.3 | CODE BUG (else-if chain; Shot no sub-frame reset; `pressed` ≠ multi-bit edge) |
| 296 Ctrl | edge on 0x100 | 16 §5.3 | right |
| 335 comment VMs before reveal | all-zero VM (ip NULL, flags 0): inert, not drawn | 16 §5.2 | right (equivalent: disabled, not drawn) |
| 358 VM run order | after state + sub-frame step: bg, 31 rows, 8 VMs; not on the exit frame (Calc returns 0, node removed) | 16 §5.3 | right; CODE BUG minor: exit frame still runs VMs and draws |
| 373 draw order | surface 0, bg, rows (VM, cursor, number), 7 comments, "Now Playing", title (pos (320,52,0), colour forced 0xFFFFFFFF) | 16 §5.2 | CODE BUG (title before comments; title alpha/colour from its script; ASCII colours) |
| 388 cursor marker | one-byte string 0x7F → ascii.anm sprite 126 (▶), colour = row VM colour | 16 §5.2 | CODE BUG (">" glyph, default colour) |
