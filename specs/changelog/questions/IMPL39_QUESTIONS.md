# IMPL39 questions (screen-effect linking, stage-clear panel, HUD viewport)

Answered in [answers_re_q29](../answers_re_q29.md).

1. **Clear bonus timing (12 §5.6 "once, on the first frame after op 9").** Is it the same `Hud_Calc` that ran op 9
   (computation after `Msg_Run`) or the next frame's? The port computes it in the next `Hud_Calc`, so the op-9 frame's
   draw shows the previously stored total. Is impl+0x22D88 reset by `Hud_Load` (port: 0 per stage)?
2. **Title y for stage idx > 6 (12 §5.6).** Only the title is drawn at y 80 and the pen continues from 96 ("Clear" at
   128), or does the whole panel start at 80 ("Clear" at 112)? The port moves only the title.
3. **Lives / bombs / clock in the panel (12 §5.6).** Op 9 stores clear bonus, points, graze, time, power (10 §9). Are
   lives, bombs (calc step 2, "Player"/"Bomb" lines) and the clock of the night bonus (step 3, "        %8d0") read
   live from the stats each frame? The port reads them live.
4. **Stage-clear VM impl+0x212C8 draw site.** 10 §9 restarts it, but no section says which function draws it, with
   which draw mode, and in which order. The port draws it after the stage-title / clock VMs in `Hud_Draw`, playfield
   viewport (`// SPEC-GAP` in `src/hud.rs`).
5. **Percentage lines with f = 0 (12 §5.6).** `(h·100)/f` with 0x164D31C = 0 (op 9 with no counted frame) is an
   integer division by zero; what does the original do? The port divides by 1.
6. **`Hud_DrawBoss` guard vs. title VMs.** 12 §5.4 says `Hud_DrawBoss` runs only when no dialogue runs and the bar is
   shown or fading, but 12 §5.1 / §5.7 say the stage-title VMs, the clock impl+0x34D4 and the night clock are drawn
   every frame by `Hud_DrawBoss`. Which parts does the guard cover? The port draws the title / clock VMs every frame
   and only the bar, markers, gauge, "Enemy" label and timer under the guard.
7. **Dirty-row counters (12 §5.1 +0x04, §5.3).** Writers and the per-draw decrement of the six 2-bit counters are not
   specified (needed only for a port that keeps the back buffer, as the original does). Low priority.
