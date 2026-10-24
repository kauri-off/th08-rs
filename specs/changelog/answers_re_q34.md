# RE-Q34 — implementer questions IMPL91–98 (2026-10-24)

Brief: docs/briefs/RE_Q34_BRIEF.md. Runs: re/work/runs/q34/ (portfix = master src copy + `// q34 X` scratch fixes,
q34_fixes.diff; rungolden.sh <tag>; tim.py = per-frame boss-timer / life-bar counts; kl.py = K (capture) lines;
tools/calls.py = byte scan for `call`/`jmp` rel32 and absolute references to a function).
Baseline golden_M0 (master): trace 25634 full, draw same 97.45 % (front.anm extra 3.66 M = the port's every-frame
panel repaint, not in this brief).

## Changed spec sections
| spec § | change |
|---|---|
| 07 §5.4 | exact `Ecl_CallSub` (ECX/EDX args, push target, depth cap); op 88 on another enemy: its 0x2CEA outside `Ecl_Run` = depth of the last context it ran, += 1 lost at its next entry; **op 88 on the caller itself**; thread return → 0x2CEA := 0x2CE8 |
| 07 §7 op 174 | the released aura is not drawn in its release frame; `Effect_SpawnHigh` may reuse the slot; `Enemy_ReactPlayerMode` writes only the interrupt |
| 08 §8.5 | "Full Power Mode!" message is outside the no-spell-card test |
| 11 §4 step 1 | `Hud_StageTransition` behaviour |
| 12 §3 | `Hud_StageTransition` 0x438F58: capture.anm VM script 1 + playfield capture; its three callers; the second capture of a stage advance overwrites the op-9 one |
| 12 §5.1 | +0x24 / +0x28 writers and lifetime (kept across continuing stages / continues); +0x2C writers, plain request (no boss-exists test); +0x1C → state machine |
| 12 §5.1a | tile +0x120 is I4 (var 10008), not I0 |
| 12 §5.1b step 1 | full boss-bar state machine (impl+0x2A40 states 0–3, label VM flag bit 14, alpha := 0 on request, instant hide when the label waits, fill moves only in state ≥ 2) |
| 12 §5.4 | timer: no extra gate, min(+0x24, 99), `%.2d`, colour from the raw value, sound on raw change, +0x28 after the print, alpha-0 glyphs not drawn; guard reads +0x2C live |
| 12 §5.5 | `Hud_ShowMessage` callers (4 sites): types 2, 3, 4 are never shown |
| 12 §5.7 | impl+0x21810 recomputed every frame (= tiles still running); tiles show the latest capture |
| 13 §2.6c | `SetRenderState` reached only via `Draw3DWorld` ← `Stage_DrawLayer`; bit-15 data (stgNbg scripts, etama script 51) |
| 13 §4 | `Ascii_InitVms` sprites: +0x2A4 sprite 0 (8×8), +0x548 sprite 0x88 (8×12), +0x000 sprite 0x20 |

## Questions
- **IMPL91 Q1** (op 88 on the caller itself): `Ecl_CallSub` works on the **running** context (+0x2CA0: a thread while a
  thread runs), its own stack (+0x2CA4) at the working depth 0x2CEA; the pushed ip = the context's stored ip (last
  load by `Ecl_Run`) + that instruction's size; `Ecl_Run` then continues the caller's code with its local ip and
  writes it back when the context stops, so the sub never runs but clock / wait / sub id / args / pushed frame /
  depth + 1 stay. Evidence: disassembly 0x41AE6E–0x41AE96, 0x421BD0, 0x4184B0–0x418520, 0x41E7E4. → 07 §5.4.
  Port: main context, op-52 push, no skip → **CODE BUG** (unreachable in the shipped data).
- **IMPL91 Q2**: yes — `Ecl_Return` 0x421CB0 with depth < 0 sets 0x2CEA := 0x2CE8 and `Ecl_Run` goes to the next
  thread; with none left it returns with the main depth. → 07 §5.4. Port right.
- **IMPL92 Q1**: op 174 writes nothing else; the released aura (+0x350 := 0) is not linked into a draw list by
  `EffectMgr_Calc` (prio 13, after the ECL), so it is not drawn in its release frame; `Enemy_ReactPlayerMode` writes only
  the interrupt. The etama.anm#1 numbers in the question are from an older baseline: golden_M0 has etama.anm#1 missing 62 /
  extra 2020 / changed 25. → 07 op 174. Port right.
- **IMPL93 Q1**: never. `AnmManager_SetRenderState` has one call site (`Draw3DWorld` 0x464470), which is called only
  by `Stage_DrawLayer`; that calls `Stage_SetCamera` itself first. Bit 15 is set in the data only by the stgNbg.anm
  scripts and etama.anm script 51 (effect type 19), and effects are not drawn through `Draw3DWorld`. → 13 §2.6c. Port right.
- **IMPL95 Q1**: only `Hud_Load` and `Hud_Update` write +0x21810; `Hud_Update` runs all 168 tiles while it is ≠ 0 and
  stores 168 − (number of `AnmVm_Run` calls that returned 1) — 1 also for a VM whose script already ended — i.e. the
  number of tiles still running. → 12 §5.7. Port (decrement on script end) right.
- **IMPL95 Q2**: +0x120 (0x5848 − 0x5728) is I4, var 10008. Spec wording fixed (12 §5.1a). Port right.
- **IMPL95 Q3**: `Hud_StageTransition` 0x438F58, called by `GameMgr_Calc` on the clear frame when
  `next_scene_after` < 0 (stage 1–5 advance / replay next stage), and by the pause-menu and continue-menu retry paths.
  It sets VM impl+0x3CC0 to capture.anm script 1 and captures the playfield into the same rect as msg op 9, so the
  later image overwrites the op-9 one and the 168 tiles show it. Golden K lines: 10796 / **10918** and 24986 / **25108**.
  → 12 §3, §5.7, 11 §4. Port has no `Hud_StageTransition` → **CODE BUG** (not scratch-tested: the effect is texture
  content, which the quad diff does not see).
- **IMPL96 Q1**: VM +0x548 gets sprite **0x88** (8×12), not sprite 0 (8×8); the string VM +0x000 gets 0x20. Golden
  3760: large popup glyph `R 36.5 38.5 44.5 50.5` (12 px), port 8 px. → 13 §4. **CODE BUG**, scratch fix A: golden same
  97.45 % → 99.78 %, ascii.anm#0 changed 195 960 → 7 424.
- **IMPL97 Q1**: no. `Hud_ShowMessage` has exactly 4 call sites (byte scan): 0x416823 `Spell_End` (5/6), 0x440B0F /
  0x440D91 power items (1), 0x44120A full-power item (1). Types 2, 3, 4 are unused text. → 12 §5.5. Port right.
- **IMPL97 Q2**: unconditional (after the `if` that cancels the bullets). → 08 §8.5. Port right.
- **IMPL98 Q1**: the timer has no gate of its own; it is part of the guarded block. The frames in the question are
  **game** frames 3697–3759 (presents 4432–4494). There the reference draws nothing of the guarded part because the bar
  state machine (12 §5.1b, new) hides at once: the label VM is waiting (flag bit 14) when +0x2C goes to 0, so state → 0,
  alpha 0, fill 0 in the same frame. The port faded the bar out over 63 frames and gated the timer on the boss timeout.
  Also: the request frame sets alpha := 0 (the port +4), the fill moves only in state ≥ 2 (the port at once), the guard
  reads +0x2C live, and alpha-0 glyphs are not drawn. → 12 §5.1b, §5.4. **CODE BUG**, scratch fixes B (timer
  ungated, colour from the raw value), C (state machine), D (alpha-0 ascii glyph skip), E (live +0x2C guard):
  every frame's timer-glyph and life-bar counts now equal the reference (tim.py, 0 mismatching frames; before: 8 ranges);
  golden same 99.78 % → 99.80 %, logic trace still 25634 full.
- **IMPL98 Q2**: clamped: v = min(+0x24, 99), so ≥ 100 shows "99". → 12 §5.4. Port right.
- **IMPL98 Q3**: +0x24 / +0x28 are cleared only by `Hud_Register`'s fresh-start zero fill; they are kept across
  continuing stages and continues. → 12 §5.1. Port (0 on every stage start) **CODE BUG** (only visible on a first
  guarded draw before `Enemy_CheckTimeout` writes; not seen in golden).

## CODE BUG list (port master after impl98)
1. 13 §4 — large-popup VM +0x548 init sprite 0x88 (glyph height 12), not sprite 0 (scratch A).
2. 12 §5.1b step 1 / §5.1 +0x2C — boss-bar state machine: states 0–3 with the label VM wait flag; alpha := 0 on request;
   instant hide while the label waits; fill moves only in state ≥ 2; request is the +0x2C byte alone (no boss-exists
   test); whole step skipped during dialogue (scratch C).
3. 12 §5.4 — timer drawn on every guarded draw (no boss/timeout gate), colour from unclamped +0x24, sound when raw
   +0x28 ≠ +0x24, +0x28 := +0x24 (scratch B); guard reads +0x2C live at draw (scratch E).
4. 05 §9 / 12 §5.4 — ascii string glyphs with alpha 0 are not drawn (scratch D).
5. 12 §5.1 — HUD +0x24 / +0x28 keep their values across continuing stages and continues (0 only after a fresh start).
6. 12 §3 / §5.7 / 11 §4 — `Hud_StageTransition` missing: clear-frame playfield capture into capture.anm script 1's rect
   (and VM impl+0x3CC0 := script 1), also on pause-menu / continue-menu retry.
7. 07 §5.4 — op 88 on the calling enemy itself (unreachable in the data; low priority).

## Open observations (not in this brief)
- Continue-menu capture: the reference logs only `K screen ? full` at present 25317 (no direct CopyRects into
  capture.anm, i.e. the filtered D3DX path of 12 §3); the port copies at 25316 (`K screen capture.anm#0 … 0 0`).
- front.anm extra 3.66 M: master repaints the side panel every frame instead of the dirty-row rules (12 §5.3).
