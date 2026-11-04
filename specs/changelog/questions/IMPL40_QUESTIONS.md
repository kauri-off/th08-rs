# IMPL40 questions

Answered in [answers_re_q29](../answers_re_q29.md).

1. **Capture VM script and position (12 §3, §4).** The pause menu (+0x1A70) and continue menu (+0xFE0) capture VMs:
   which `capture.anm` script do they run, and who sets their position? The port uses script 0 (sprite 0 = 128×128,
   scale 3 × 3.5, colour (128, 48, 16), alpha 0 → 255 over 30; interrupt 1 fades out) and pos (32, 16), because that is
   the only placement that makes the 384×448 quad cover the playfield. Is the VM ever interrupted (fade out) when the
   menu closes? The port never interrupts it.
2. **Spell flag bit 9 "captured" (07 §11.1, 12 §4).** Where is it set and where cleared? The port sets it on the
   captured path of `Spell_End` and clears it at `Spell_Start`. Does it survive a retry (scene 11)?
3. **Night-clock overlap fade order (12 §5.7).** In `Hud_Calc`, does the player-overlap fade of the alpha byte
   run before or after `Hud_Update` runs the night-clock VM (whose script 2 interpolates alpha 0 → 255 over 30 at
   start)? The port fades first. Is the player position compared in playfield coordinates (like 12 §5.8 step 1.2)?
4. **Capture timing (12 §3).** "After the next Present … copies the back-buffer" and "the capture shows the frame
   presented just before the request": the port copies the last presented frame at once when the request is made
   (no pending state, so the "close at once" failure path never happens). Is the copied frame N−1 (presented before
   the request's calc) or N (the frame whose draw follows the request)?
5. **Capture option bit 0x17CE8FC bit 1.** `Window_InitD3D` sets it on "screen-capture support": under which
   condition is it clear? The port treats it as always set.
6. **Continue menu first frame (11 §4.2, 12 §4).** In gameover_save (trace in sync until row 4214) the reference
   first draws the continue menu at present 3258 with the capture VM already at script time 2 (alpha 0x11); the port
   opens the menu one calc later (first draw at 3259, all menu VMs one frame late, not only the capture). Which state
   does the first `ContinueMenu_Update` frame leave (state 0 for one frame, then 1/2?), and how long does state 0 last
   given the draw rule "state ≠ 0 or the state frame > 2"? Is `game_over` seen by the prio-1 ASCII calc in the same
   frame it is set (e.g. by timeline op 16 / 0x164D0BB)?
