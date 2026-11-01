# RE-Q71 — the last single-present draw groups (2026-11-01)

Inputs: target/drawgroups_149 + target/drawgroups_dl3_149, classifier output target/cls149. Runs: re/work/runs/q71
(CHECKPOINT.md; portcopy = master c8913b0 src + scratch fixes, scratch.diff; `Q71=1` = env-gated popup debug print;
fr.py = per-present quad dump of a reference dir or a port draw.drawlog.xz). Port cuts in target/re71/out/{b,c,d}/.
Note: `session --to N` counts menu-relative rows, not presents (row ≈ present + 600…2650 in long dl3 runs).

## Changed spec sections
- 13 §4 — popup cursors (+0x8290 / +0x8294 / +0x8298), slot-order draw, `Ascii_InitVms` resets small/special cursors
  (not large) and clears the slots; callers incl. every `GameManager_StageStart`.
- 11 §3.1 — `Ascii_InitVms` + `Ascii_SetupGaugeVms` placed in the stage-start sequence (after `flags &= ~0x10`, before
  the loading wait).
- 10 §3 (boss_card row) and 10 §5.1 op 16 — no clamp on `next_line`: a third line in one box lands on boss_card
  (text.anm script 2).

## Cause 1 — class "C FF bomb-end tint" (and the old "P4 ASCII colour ±2"): popup cursor not reset → CODE BUG 1
The rows are not about tint: all are small score-popup glyphs (ascii.anm#0 8-px digit rects) whose alpha differs by
±2. They only show up where several popups spawned in the same frame overlap (same snapped glyph rect; alpha by player
distance, 13 §4). Fast-forward presents just happen to be where that occurs. Ref vs port at dl3 silentselene_52279
present 45984 (stage 3): the same three popups (alpha 9d, 9e, 9f, identical rects), ref order 9d, 9e, 9f, port order
9e, 9f, 9d. The `Q71` print shows the port's popup positions, the player position and d are right; only the slot order
differs. `Popup_Number` 0x403200 = the port's allocator, but `Ascii_InitVms` 0x402450 (called by
`GameManager_StageStart` at every stage start, `GameMgr_OnRemove`, `MainMenu_OnRegister`, `AsciiManager_Load`)
zeroes +0x8290 and +0x8294 (not +0x8298). The port's `clear_popups` never resets the cursors, so from the second stage
or game on the slots are rotated. Scratch fix (cursor reset in `clear_popups`, re/work/runs/q71/scratch.diff) →
ref order at 52279 45984, ref500 52233 42259, udMn38 27183–27187, 54584 22250, 78684 16254 (cuts c/, d/). The old
"P4 ASCII ±2" rows are the same cause: udTh03 cut (e/, presents 86692–86716, 6 sampled) → popup glyphs identical,
same order. cls.py now puts every ascii 8-px glyph colour diff (rows 0/8/16) in class Q.

## Cause 2 — 78684 39702 text.anm#0 [0,54,256,71] missing: MSG op 16 third line → CODE BUG 2
Op 16 (`Msg_Run` 0x433DB3) addresses line VM +0x222C4 + next_line·0x2A4 with **no clamp**. The data has exactly one box
with three op-16 lines: msg4ab entry 0 (stage 4A), time 121, '魔理沙の仕業ね。' (awk check over msg_dump.txt; op 3 lines
are only 0/1). next_line = 2 → VM +0x2280C = boss_card, which starts text.anm script 2 (sprite 2, strip (0,54,256,17),
centred at (272,352), alpha 0 → 240 over 60 frames) and gets the text rendered into that strip. Replay skip runs the
whole box in one frame, so the ref shows it for one present (alpha 0x08); at present 39703 op 8 (time 123) replaces it
with face_st04a's name card (ref quad face_st04a.anm#1 at 143.5,351.5). The port clamps (`next_line.min(1)`) and
rewrites line[1]; later texts in the same frame overwrite line[1] anyway, so the boss_card strip is the only visible
difference. Scratch fix (index 2 → boss_card in `render` / `flush_texts`) → 78684 cut to present 39949: no text.anm
diff.

## Accepted
- **50587 32210, 52101 32822 / 34071 etama#1 [193,113,255,175] "changed pos" — compare artefact (X).** Sprite 197
  = etama entry 1 script 23 (effect script 48, enemy aura type 32, 13 §2): additive, zwrite off, spins −0.05236/frame,
  2 quads per VM. Ten aura VMs share almost the same centre. In port cuts (b/) every sprite-197 quad line at those
  presents is **byte-identical to the reference, in the same order**. The diff comes from `drawlog::diff_frame` pass 1:
  a port quad takes the first ref quad in the grid cell that is within tolerance (a neighbouring VM), and the leftover
  pair with equal centres and different rotation then gets paired in pass 2 as "changed pos". Tooling only (optional
  fix: exact-match pass before the tolerance pass); no game change.
- **udSs12 12282 etama#1 [65,241,79,255] missing — P5 snap tie + edge cull.** A list-B particle in its last frame
  (2×1 px, alpha 0x0C). It matches exactly at 12278/12279. At 12282 the ref quad is x 30.5–32.5 against viewport
  x 32: it only touches the edge (05 §9 step 3). The port's quad lands 1 px left after a snap tie (same sprite's earlier
  rows are P5 "116.5 vs 117.5"), so it is culled. The port's cull test = spec. One column at 5 % alpha.
- **53334 87016 etama#0 [97,209,127,239] — P6 snap tie.** Sprite 148 (30×30 bullet): only the top edge differs
  (ref y 111.5, port 110.5), bottom 141.5 the same, the other 11 quads identical (cut d/). That is a .5 tie of
  py − 15 rounded by `Draw2DSnap` (sub-ULP difference), one present.

## CODE BUG list (IMPL150)
1. Popup cursors: `Ascii_InitVms` at every `GameManager_StageStart` (and GM remove / main-menu register) clears the
   popup slots and sets the small (+0x8290) and special (+0x8294) cursors to 0; the large cursor (+0x8298) is kept
   (13 §4, 11 §3.1). Port: `ascii::clear_popups` must reset `next` and `special` (not `large_next`), and run at the
   points of 13 §4.
2. MSG op 16 without clamp: next_line = 2 → boss_card VM (text.anm script 2, text into sprite 2's strip) (10 §5.1,
   10 §3). Port: `msg.rs` op 16 `next_line.min(1)` and `hud.rs::flush_texts` indexing `lines[line]`.

## Full-set compare (orchestrator)
After IMPL150: the usual drawgroups for ref500 + golden and dl3 (target/drawgroups_150, target/drawgroups_dl3_150), then
`python3 re/work/runs/q71/cls.py` with paths switched to _150. Expected: classes Q and M gone; only P1–P6, L, F, H,
O2 and X are left.

## Draw parity
After CODE BUGs 1–2, every remaining group is in an accepted class: precision (P1–P6), loader race (L), host-time fps
(F), menu after a game (H), log/compare artefacts (O2, X). **Draw parity is done** once IMPL150 is confirmed by the
full-set compare.
