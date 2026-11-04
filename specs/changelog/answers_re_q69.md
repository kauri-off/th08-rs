# RE-Q69 — close-out audit of the remaining draw groups

Input: port master a29585a (IMPL147) compares `target/drawgroups_147` (20 batch500 draw-log recordings + golden, 157 merged
groups) and `target/drawgroups_dl3_147` (36 dl3, 409 merged groups). Work dir `re/work/runs/q69` (CHECKPOINT.md;
scripts dump.py / ex.py / cls.py / fd.py / qs.py / sel.py / rc.py / rn.sh; scratch build target/re69 = master src, no
changes). Port cuts: a0 golden to 11100 rows; a1 udM401 / 51100 / udSe43 / udLR08; a2 udfl24 to 90000 rows, golden full.

## Changed spec sections
| section | change |
|---|---|
| 12 §1 (loading-screen states, new "Press Shot Button" paragraph) | `Supervisor_DrawLoading` 0x445BD4: the state +0x2FC (0x17CEA54) is the prompt counter. Each Supervisor draw does state += 1 when it is > 1, and draws only when it is > 4, so the prompt starts on the **third** draw after `Loading_Wait`. Alpha by c < 35 / ≥ 35, period 60 (c > 64 → 5). The alpha is written into the ASCII queue colour byte 0x4D508B and **not restored** (it leaks into the first HUD strings). `Loading_End` acts on state 1 only. The state is cleared by `GameMgr_Calc` step 3, so the prompt is still drawn on the confirm frame. `Loading_Wait` acts on state 1 only (interrupt 2 = hide_stop). |
| 09 §6.2 (new paragraph "Return value and the same-frame respawn") | `Player_DyingUpdate` returns 1 only in the lives > 0 branch. On the game-over frame no `Player_RespawnUpdate` runs, so there is no respawn clear rectangle and the bullets are untouched (caller 0x44C48F). |
| 12 §2.9 (ASCII queue drawing) | Glyph quads are `EmitQuad` quads, so the global ANM tint applies to every queued string (whole-frame tint, 06 §10.2). |
| 12 §3 (pause colours) | The colour writes are whole dwords with alpha 0xFF, at the top of each state case before the input test. Items are therefore opaque from the first frame (the `alpha_time` slot starts from 0xFF), and a cursor move shows one frame later. The (−4, −4) store goes to `pos_offset`, which nothing draws, so no offset is visible. |
| 12 §4 (continue menu colours) | Same whole-dword writes before the input test; same invisible `pos_offset` store. |
| 15 §3.3 (playback fps text) | Both fps strings are queued only from the draw-time `Fps_Update` call (Supervisor draw prio 16), once per presented frame (one copy per fast-forward present). The "%.02ffps" string uses the current queue colour. |

## Causes (original → spec → port)
| # | groups | original | spec | port |
|---|---|---|---|---|
| A | golden 10951–10982 and 25141–25196: ascii prompt glyphs missing on the first and last frame, alpha one step behind (fb vs ff), side-panel digits 0x7F / 0xE6 vs 0xFF on the first gameplay frame | prompt from the 3rd draw after `Loading_Wait`, counter in the draw, period 60, alpha leaked into the queue colour, cleared by GM step 3 (prompt shown on the confirm frame) | 12 §1 | **CODE BUG**: `gamemgr.rs` `loading_wait` counts calcs and shows from the 5th, uses a 59-frame loop with a different rising formula, stops on the release calc, and draws with an explicit colour (no leak) |
| B | last gameplay present of the 6 game-over dl3 replays (udLR08 25756, udMn38 95309, udVn17 35151, uda212 93093, udfl24 105304, udM401 87499): etama#5 bullets missing, etama#0 cancel sprites extra (564 quads in udLR08) | game over → `Player_DyingUpdate` returns 0 → no respawn update / clear that frame | 09 §6.2 | **CODE BUG**: `player.rs` `dying_update` returns `true` on game over, so `respawn_update` sets `respawn_clear` = 60 and `update_invuln` cancels every bullet that frame (logic: the cancel is not in the trace columns, but the bullets are in their cancel state from then on) |
| C | replay fast-forward bomb-end presents (dl3 udM401 16850, udSs12 56241 / 74263, udTh03 47037, 51100 32275; batch udSe43 32812 / 56605, 50948 63723, 52233 32206): at present P the original draws the whole frame with the bomb's last tint (HUD front.anm, panel digits, popup text, fps text); at P+1 it draws the stage untinted, while the port tints the stage at P+1 | 06 §10.2 whole-frame tint; queued strings are tinted (12 §2.9); fps text once per present (15 §3.3) | 12 §2.9, 15 §3.3 | **CODE BUG C1**: `ascii.rs` `Queue::draw` never applies `anm.tinted()` (strings never get the global tint). **CODE BUG C2**: `supervisor.rs` `replay_fps` queues the fps text in the Supervisor **calc**, i.e. once per calc pass (3 copies in a fast-forward present); it must be queued from the draw. **C3, INFERRED, implementer to check**: at P the port's front.anm is untinted and at P+1 its stage is still tinted. That fits the port running a bomb draw / `Stage_RequestTint` on the present where the bomb ended in an earlier fast-forward pass; the original requests the tint only from the bomb draw of a presented frame (06 §10.2). No logic difference (traces identical). |
| D | golden 25316–25471, pause menu (ascii.anm#2 rows colour / pos / pos+colour, capture extra 25316) | items written 0xFFFFFFFF / 0xFF505050 incl. alpha before the VM run (opaque from frame 1); colours before the input test (highlight one frame later); (−4, −4) only in the undrawn `pos_offset` | 12 §3, §4 | **CODE BUG**: `ascii.rs` pause / continue `set_rgb` writes only RGB (items fade in from alpha 0), writes the colours after the state change, and the pause draw shifts the selected VM by (−4, −4). Capture at 25316 vs `K screen ? full` at 25317 = the known capture-path note (RE-Q34), not scored. |

Open items (not explained by any numbered spec; SPEC-GAP, small):
- **O1** silentselene 78684 presents 9771–9781 (14 groups, 110 rec×frames). After `loader lwait` (poll 9818, present 9771) the
  loader released only 11 frames later, and the original drew neither the prompt nor the interrupt-2 hide (the nowloading
  VMs keep their loop). The code read in 12 §1 predicts both. The port shows the prompt from 9774. Needs a runtime probe of
  0x17CEA54 / nowloading interrupts on that recording. This is the only `lwait` ≠ `release` gap longer than one present
  in the 36 dl3 + 500 ref500 recordings.
- **O2** gensokyou udfl24 presents 87251–87322 (Extra, Youmu bomb, 72 frames). The original log draws the Youmu bomb
  playfield rectangle as 0xFF06060A, constant, equal to the last stage quad's colour. `BombDraw_Youmu` 0x412300 passes
  0xAAFFFFFF (A = 255, then 255 − (T − 100)·255/60), and ref500 silentselene 54840 presents 285–374 log exactly that
  white fade. The port draws that white fade in udfl24 too. Unexplained (probe or host artefact suspected, unproven).
- **C2-pos** single fast-forward presents with sub-pixel position differences on several textures at once: stage 3D quads,
  player options, etama#3 (0.2–0.7 px, growing with screen distance, so the camera differs slightly). 30 batch / 43 dl3
  groups, 1 248 rec×frames, e.g. udM401 26145, udSe43 32811, 51100 32267. Traces are identical. The screen-shake offset
  handling matches (port zeroes it only in the prio-14 popup draw, as the original does). Not explained.
- **U** 7 single-frame dl3 groups (12 rec×frames): udMn38 78313 boss-timer digit colour 0xFFFFFFFF vs 0xFFA0D0FF and 78314
  stg5bg tint (0x4C vs 0x93); 50587 32210 / 52101 32822 etama#1 [193,113] pos > 1 px; udSs12 12282 etama#1 [65,241]
  missing; 53334 87016 etama#0 [97,209] pos; 78684 39702 text.anm missing.

## CODE BUG list (for the next IMPL brief)
1. 12 §1: "Press Shot Button". Draw-time counter (start 2 at `Loading_Wait`, +1 per Supervisor draw before the test,
   prompt when > 4). Alpha formula and 60-frame period as specified. Write the alpha into the ASCII queue colour and
   leave it there (it leaks into the next queued strings). Keep the prompt until `GameMgr_Calc` step 3 of the first
   gameplay calc (drawn on the confirm frame). `Loading_End` / `Loading_Wait` act on state 1 only.
2. 09 §6.2: `Player_DyingUpdate` game-over branch returns 0. No `Player_RespawnUpdate` (no `respawn_clear`, no cancel
   rectangle, no respawn sprite write) on the game-over frame.
3. 12 §2.9 / 05 §12.1: queued ASCII strings must get the global ANM tint (`Ascii_DrawStrings` glyphs are `EmitQuad`
   quads). Applies to every string drawn while the tint is on.
4. 15 §3.3: queue the playback fps text (and the "%.02ffps" string) from the draw (once per presented frame), not per
   calc pass.
5. 12 §3 / §4: pause and continue menu colour writes are whole ARGB (alpha 0xFF) at the top of each state, before the
   input test and before the VM run. No (−4, −4) shift of the selected item at draw time.
6. (verify, INFERRED) 06 §10.2 in replay fast-forward: the stage tint request and the "+0x646C bomb drew" flag come only
   from a bomb draw of a presented frame. When the bomb ends in an earlier pass of the same present, the original makes
   no bomb draw (the frame gets the whole-frame tint, the next frame no stage tint). Compare udSs12 74263 / 74264 and
   udSe43 32812 / 32813 after fixes 3–4.

## Class totals (recordings × frames, computed per class as in all_groups.tsv; `re/work/runs/q69/totals.tsv`, per-group table `classes.tsv`, per-row `rowcls.tsv`)
| class | batch500+golden groups | rec×frames | dl3 groups | rec×frames | status |
|---|---|---|---|---|---|
| P1 orbit stars (13 §2.6c) | 2 | 319 536 | 2 | 1 519 182 | accepted precision |
| P2 list-B particles (13 §2.6c) | 7 | 57 867 | 9 | 442 779 | accepted precision |
| P3 glyph `Draw2DSnap` ties | 9 | 4 035 | 15 | 32 717 | accepted precision |
| P4 ASCII colour ±2 | 4 | 11 | 13 | 564 | accepted precision |
| P5 particle snap ties | 11 | 558 | 13 | 4 299 | accepted precision |
| P6 enemy / etama#3 sprite ≤ 0.1–0.5 px | — | — | 1 | 12 | accepted precision |
| L loader race (12 §5.9; stage starts and ±1-present `lwait`/release) | 10 | 487 | 231 | 5 691 | accepted |
| F FPS digits | — | — | 2 | 2 | accepted |
| H menu after a game (RE-Q65 H) | 2 | 2 | — | — | accepted |
| A prompt | 33 | 967 | — | — | CODE BUG 1 |
| B game-over frame | — | — | 20 | 36 | CODE BUG 2 |
| C fast-forward bomb-end tint | 53 | 263 | 63 | 528 | CODE BUG 3, 4 (+6 to verify) |
| D pause menu | 21 | 276 | — | — | CODE BUG 5 |
| C2-pos fast-forward sub-pixel | 30 | 126 | 43 | 1 122 | open |
| O1 78684 replay loader wait | — | — | 14 | 110 | open (SPEC-GAP) |
| O2 udfl24 Youmu rect | — | — | 1 | 72 | open (SPEC-GAP) |
| U unexplained single frames | — | — | 7 | 12 | open |
| **total** | | 384 128 | | 2 007 126 | |

## Is draw parity done?
Not yet, but almost. After CODE BUGs 1–5 (and the check in 6), what is left outside the accepted classes is about
1 440 rec×frames: about 1 250 of them in single fast-forward presents with sub-pixel camera / position differences
(C2-pos), plus O1 (one recording, 11 frames), O2 (one recording, 72 frames, suspected log artefact) and 7 single-frame
groups. Everything else is accepted precision or race. By rec×frames, 99.9 % of both sets is accepted. The open items
are all one-frame or one-recording effects; C2-pos is the only one that recurs (57 presents in 29 recordings).

## Full-set compare
Not run here: the draw-group comparison over both recording sets is to be repeated after the fixes above.
