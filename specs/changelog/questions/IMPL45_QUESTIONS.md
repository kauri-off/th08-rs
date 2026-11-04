# IMPL45 questions

Answered in [answers_re_q30](../answers_re_q30.md).

No blocking questions. Notes:

1. **Traces not run (no game data).** golden / fullrun_noclip / selftest1 / demo MIN_SYNC and draw logs need the game data;
   the brief's expected values (demo 3 ~6684, demo 2 ~4660, selftest1 full match) are to be confirmed locally and the
   thresholds raised there.
2. **Play-time counter moved (11 §2).** 0x164D30C (`stats.rec.progress`) was `+= 1` in `GameMgr_Calc` on every
   gameplay frame (its own SPEC-GAP). 11 §2 now names the site: top of `EnemyMgr_Calc`, outside dialogue, before the
   bit-10 test. It moved there, so dialogue frames no longer count; the end-panel / result progress % may change.
3. **Human-rate counters (15 §2.3)** stay in `Player::step` (they already run during the freeze); their existing
   SPEC-GAP (the manager clock > 15 test) is unchanged and out of this brief's scope.
4. **Item 2 (11 §2).** `GameMgr::on_remove` already zeroed 0x164D2C8 as its last statement (impl37); no code change.
   `Ascii_InitVms`, which the spec places before it, has no counterpart in the port's on-remove.
5. **Exit save on window close.** 15 §2.2 says the WinMain exit save runs whenever the main loop leaves (PLST tag
   always set after boot). The port saves only on the Supervisor's quit (requested −1), not when the window is
   closed or on an error exit. Does closing the window (WM_CLOSE / WM_QUIT) also go through 0x441B95, and in which
   order relative to the GM on-remove of a running game?
6. **Quitting frame drawn.** `Frame_Run` in 00 §2 reads "if r == 0 → quit normally" right after the calc chain; 15 §2.2
   says the save follows that frame's calc, draw and Present. The port now draws and presents the quitting frame
   (app.rs) following 15 §2.2; please make 00 §2 explicit.
