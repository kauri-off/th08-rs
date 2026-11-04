# RE-Q70 — IMPL148 questions and the last open draw items (2026-11-01)

Questions: [IMPL148](questions/IMPL148_QUESTIONS.md).

Inputs: target/drawgroups_148 (batch500 + golden), target/drawgroups_dl3_148 (36 dl3). Runs: re/work/runs/q70
(CHECKPOINT.md; portcopy = master ebe04ae src + scratch fixes, scratch.diff; `Q70=1` instrumentation is env-gated debug
only). Port cuts (target/re70/out/<tag>): a1/a2 golden ≤ 11000; a3/a4 udSe43 instrumented; a5 udSe43 + udSs12 ≤ 74266;
a6 udSe43 ≤ 56606, udM401 ≤ 26146, 51100 ≤ 32277; a7/a8 golden full; a9 80415 / udTh03 (C2 spot check).

## Changed spec sections
- 11 §7 — new: the AnmManager reset at the top of every `Supervisor_Calc` pass (exact fields, incl. tint off and
  shake offset := 0).
- 06 §10.2 — per-calc tint reset; why the frame after a bomb end is clean; fast-forward case; `Player_Draw` gate.
- 05 §12.1, §12.5 — tint off and cache invalidation per calc pass.
- 13 §3 type 1 — shake offset zeroed per calc pass; fast-forward consequence.
- 12 §1 — point 5 (prompt timing against `loader lwait poll B`); replay `StageStart` wait; 78684 explained.
- 12 §5.3 — step 6 time-row colour reset (when it runs); step 7 power text order; global tint on the panel sprites.
- 12 §3 — exact pause interrupts (state 0, confirm branches, shortcuts, capture VM); capture timing on the first frame.
- 14 §2.11 — `Screen_FillRect` colour path; udfl24 rect colour = log artefact.
- 15 §3.3 — fps text refresh in fast-forward presents (accepted host timing).
- data/drawlog/FORMAT.md — port K record for size-mismatched captures (tooling).

## Q1 (IMPL148 Q1, 12 §5.3)
`Hud_DrawPanel` 0x43625D: `Ascii_SetColor(0xFFFFF0C0)` only if time ≥ threshold, and `Ascii_SetColor(0xFFFFFFFF)` as the
last statement **inside** the time-row block. So the reset runs whenever the time row is drawn (dirty or cfg4),
independent of the threshold, and not on frames where the row is not drawn. Nothing else resets the colour before the
next frame's score row. On golden 10982 the row is drawn (fields = 2 after `Hud_Load`), so the colour is white
afterwards, and on 10983 the prompt is gone. The port's unconditional reset every frame is equivalent because the port
draws every row every frame. The real difference at 10982 was the **power text**. The original queues it in step 7,
after the time row (and after the flush), so it is white. The port queues it before the graze row, so it takes the
leaked alpha (y 135.5 "60": 0x7F in the port, 0xFF in the reference). → CODE BUG 2.

## Q2 (IMPL148 Q2, 06 §10.2 / 14 §1.3)
The gate in `Player_Draw` 0x44D530 is Player +0xFDC ≠ 0, the same flag as the port's `bombing`. Instrumented port
(a3/a4, udSe43): the bomb timer, bomb end (in the last pass of present 32812) and bomb draws match the original. Last
bomb draw at present 32811, timer 198, requests 0x807D7D82. The shift has two causes:
1. `Supervisor_Calc` 0x445453 switches the tint off inline at the top of every calc pass (0x44548A…: +4 := 0,
   +0 := 0x80808080). The port has no such reset. At present 32812 its tint (on, drew = 1) survives into present
   32813's Stage_DrawLow, so the stage is tinted one present too long. At normal speed the last bomb draw requests the
   neutral 0x80808080, so this never shows; it shows only in fast-forward. → CODE BUG 3.
2. At present 32812 the reference tints the whole frame, HUD panel included (front.anm 0xFFF9F9FF). The port's panel
   blank patches, tiles and strips are raw strips with colour [255; 4] and no `anm.tinted()`. The original draws them
   with `Draw2DSnap` → `EmitQuad` (05 §12.1). → CODE BUG 4.
Scratch fix (supervisor calc `tint_off`, tinted panel sprites): udSe43 32812/32813, udSs12 56241/56242/74264, udM401
16850/16851, 51100 32275/32276 and udTh03 47037/47038 groups are all gone.

## Re-run of the q69 classifier on the 148 compares (cls.py → totals_148.tsv)
- **B** (game-over respawn): gone.
- **A** (prompt), **C** (FF bomb tint), **D** (pause): partly gone. What remains has new causes:
  - A remains as A2: the prompt appears one draw early in golden (extra at 10950, alpha one step ahead), plus the power
    text in 10982 / 25196.
  - C remains: the tint is not reset per calc pass, and the panel is untinted.
  - D remains as D2: pause interrupts and the capture.
- **New:** nothing else. U = 5 groups (10 rec×frames) of single-present items (udMn38 78314 an additive particle colour,
  udSs12 12282 one missing particle, 50587/52101 one etama sprite 8 px, 53334 one enemy 1 px, 78684 39702 one text quad);
  they are not investigated further.

## Q3 sub-pixel camera/position on single fast-forward presents (class C2)
Cause: the same `Supervisor_Calc` reset sets the shake offset AnmManager +0x1C/+0x20 := 0 on every pass. When the
shake effect ends in the last pass of a fast-forward present (removed, no write), the original draws with offset 0.
The port keeps the offset written by the previous pass and shifts the whole playfield (udSe43 32811: +0.5, +0.5 on
every 2D/3D quad, with ±1 px snap on glyphs). → CODE BUG 5. Scratch fix: udSe43 32811 / 56577 / 56584 / 56604, udM401
26145, 51100 32267 / 32274, 80415 49155 and udTh03 57923 are gone (a6, a9). The rest of the old C2 rows were mislabelled:
- the y 167.5 0xFFE0B0FF glyph rows are P3 ties (they also occur on non-FF presents, e.g. golden 23339);
- the fps digit `uv` rows are F, the host-time fps window (15 §3.3).

## Q3 extras
- **silentselene 78684 9771–9781 (O1):** the probe's `lwait` hook sits on the entry of `Loading_Wait`, before its body.
  Replay `StageStart` waits ≥ 80 loading frames, calls `Loading_Wait`, then sleeps while GameFlags bits 5–6 ≠ 0
  (0x43B84A–0x43B8A8). In the reference there is no interrupt-2 hide and no prompt until the release at poll 9829,
  so the body cannot have run before that frame: the loader thread was delayed after logging. INFERRED artefact →
  class L (accepted).
- **udfl24 87251–87322 (O2):** `Screen_FillRect` stores EDX into all four vertex colours (0x45B2E5–0x45B2FD).
  `BombDraw_Youmu` passes 0xAAFFFFFF and `BombDraw_YoumuLS` 0xAAFF0000, so 0xFF06060A cannot come from the game. It
  equals the stage-quad colour of that present: INFERRED log artefact, the port is right (accepted).

## Causes → spec → port
| # | cause (original) | spec | port |
|---|---|---|---|
| 1 | `Loading_Wait` acts between frames: the first draw with state 3 is the draw of the frame with poll B | 12 §1 pt 1, pt 5 | **CODE BUG 1** |
| 2 | power text queued after the time row (step 7); time-row reset only when that row is drawn | 12 §5.3 steps 6–7 | **CODE BUG 2** (order); reset OK |
| 3 | tint off at the top of every calc pass | 11 §7, 06 §10.2, 05 §12.1 | **CODE BUG 3** |
| 4 | panel sprites tinted by the global tint | 12 §5.3, 05 §12.1 | **CODE BUG 4** |
| 5 | shake offset := 0 at the top of every calc pass | 11 §7, 13 §3 | **CODE BUG 5** |
| 6 | pause state 0 interrupts VMs 0–3 only; close interrupts in the transition branch; capture VM interrupt 1 on resume/ESC | 12 §3 | **CODE BUG 6** |
| 7 | pause/continue capture queued, copied after the Present of that frame | 12 §3, §4 | **CODE BUG 7** |
| — | 78684 lwait gap (probe/loader thread delay) | 12 §1 | accepted (L) |
| — | udfl24 Youmu rect colour (log artefact) | 14 §2.11 | port right |
| — | fps digits on FF presents (host time) | 15 §3.3 | accepted (F) |

## CODE BUG list (for IMPL149)
1. `gamemgr.rs` loader wait (session schedule): `load_prompt := 2` is set in the calc of poll B − 1 (`next = s.poll + 1
   ≥ wait`), before the prio-0 draw of frame B − 1, so the prompt starts one draw early. The state must change only
   after that draw (scratch: set it on the second wait calc, `loading_wait == 1`; the nowloading hide timing stays
   as is). 12 §1 pt 5. Golden 10950/25140.
2. `hud.rs` `draw_panel`: queue the power text ("MAX" / `%d` at (488, 136)) after the time row and its colour reset.
   12 §5.3 step 7. Golden 10982/25196.
3. `supervisor.rs` calc: at its top, on every calc pass, switch the tint off (`anm.tint_off()`). 11 §7, 06 §10.2.
   FF bomb ends (udSe43 32813, udSs12 56242/74264, udM401 16851, udTh03 47038/57941, 51100 32276, 50948 63724…).
4. `hud.rs` panel `spr` closure: vertex colour `anm.tinted([255; 4])` instead of `[255; 4]` (blank patches, tiles 13,
   strips 14); the gauge quad stays untinted. 12 §5.3, 05 §12.1. FF bomb-end presents (front.anm rows).
5. `supervisor.rs` calc: also `anm.screen_offset = [0.0; 2]` at the top of every pass (and the other cache resets of
   11 §7 if the port models them). 11 §7, 13 §3. C2 rows (udSe43 32811, 51100 32267, udM401 26145, …).
6. `ascii.rs` PauseMenu: state 0 sends interrupt 1 to VMs 0–3 only (the port also interrupts the difficulty VM 7 and
   the practice VM 8; mark them shown without an interrupt). Send the close interrupts in the transition branch, in
   the same call: resume confirm → VMs 0–3 interrupt 2 + capture VM interrupt 1; "yes" → VMs 4–6 interrupt 2;
   ESC → all visible VMs interrupt 2 + capture VM interrupt 1; Q/R → all visible VMs interrupt 2. Today they sit in a
   `frame == 0` branch of states 4/9/10 that never runs after a confirm (`frame` is already 1 on the next call).
   12 §3. Golden 25316–25334 (label 24/36/48 → 25/38/51), 25452–25471.
7. `ascii.rs` pause and continue menus: `queue_capture` (applied after the Present by `apply_pending_captures`)
   instead of the immediate `apply_capture` in the calc. 12 §3 / §4. Golden 25316/25317 (K record). Drawlog tooling:
   do not log a `K` for a size-mismatched capture (FORMAT.md).
Scratch verification of all seven: golden full (a8) leaves only P3 ties, H 25646 and the K record of item 7's tooling
note (`K screen capture.anm#0 … 0 0` at 25317, the reference has only `K screen ? full`).

## Class totals (148 compares, before the fixes; re/work/runs/q70/totals_148.tsv, rec×frames)
| class | batch500+golden | dl3 |
|---|---|---|
| P1 orbit stars precision | 319 536 | 1 519 182 |
| P2 list-B particle precision | 57 867 | 442 779 |
| P3 glyph snap ties | 4 388 | 36 498 |
| P5 particle snap ties | 558 | 4 299 |
| P4 ASCII colour ±2 | 11 | 564 |
| P6 enemy ≤ 0.1 px | — | 12 |
| L loader race (incl. 78684) | 487 | 5 870 |
| F fps digits | 9 | — |
| H menu after a game | 2 | — |
| O2 udfl24 log artefact | — | 72 |
| A2 CODE BUG 1+2 | 992 | — |
| C CODE BUG 3+4 | 53 | 138 |
| C2 CODE BUG 5 | 76 | 197 |
| D2 CODE BUG 6+7 | 79 | — |
| U single-present | — | 10 |
After CODE BUGs 1–7, in both sets only the accepted classes (precision, races, host timing, log artefacts) remain,
plus 10 rec×frames of single-present U.

## Full-set compare
Not run here: the draw-group comparison over both recording sets is to be repeated after the fixes above.

## Draw parity
Not done yet: seven small CODE BUGs remain (above). Once IMPL149 fixes them and the full-set compare confirms it, every
remaining group belongs to an accepted class (precision 13 §2.6c / 05 §10, loader races 12 §5.9, host fps timing
15 §3.3, two log artefacts), and draw parity is done.
