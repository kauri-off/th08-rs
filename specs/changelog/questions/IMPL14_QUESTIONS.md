# impl14 questions for the dirty room

Answered in [answers_impl14](../answers_impl14.md).

Demo sync (`cargo run --release --bin demosync <n> [from-to] [rng-frame]`, frames = gameplay frames of the trace).
Each first divergence below is reached with the RNG state, counter and all other compared fields in sync up to the
frame before.

1. **Demo 0, frame 261 (stage 5) / demo 2, frame 450 (stage 3): RNG draws per op-83 child.** Stage 5 sub 2 spawns
   13 children with op 92 (subs 3/5/7: op 54 54, op 77, op 80 16, op 81 3, op 83 1, op 73/74, op 135 thread at t20).
   The trace makes 130 draws on the spawn frame and again every 2nd frame (261, 263, …) = 10 draws per child every
   other frame. Demo 2 frame 450: one such child (stage 3 sub 3) → 10 extra draws on its spawn frame. The port makes
   none. What consumes them (flags B bit 1 = op 83? the enemy.anm script 54 sprite cycle of 2 frames? an effect with
   10 draws)? Exact call site, period and order inside the enemy update, please.
2. **Demo 1, frame 560 (stage 4A): bullets of enemies outside the playfield.** Enemies at x ≈ 400–480 (right of the
   playfield, flags 0x203 patterns, counts 1/1/2) place bullets whose velocity points away from the playfield
   (vel.x ≈ +2.4). In the port they are freed by the off-screen rule (08 §4.3) the frame their 10-frame spawn effect
   ends (counter 0, no redirect/bounce); the trace's bullet count never drops for them. Are bullet angles mirrored
   for mirrored enemies (flag A bit 18), are these enemies mirrored copies of left-side ones, or is there another
   off-screen exception (spawn-effect bullets, inherited counter)?
3. **Demo 3, frame 795 (stage 2): 4 extra draws at an enemy kill.** Killed by player shots, gauge −8064 (≤ human max)
   after the −200. Port draws: 8 (hit spark) + 16 (death effects) + 12 (3 extra items × 2 floats) = 36; trace 40.
   Which call adds 4 draws (a checksum? an item/time orb at human max?).
4. Enemy death gauge change (07 §10.3, "±200, later phase"): the port uses +200 when the player mode byte (+5) is
   youkai, −200 when human, skipped while bombing (`GaugeAdd` not forced). It matches the traces so far — please
   confirm the rule (mode byte vs focus byte, forced or not, anything else in that step).
5. Leaving options (state 3): the traces only fit when their calc fn (and its 0x2F sparkle) does not run in state 3.
   Confirm, and say whether the option VM/position still update while leaving.
6. Replay stage load: record +0 is the score at the **end** of the stage, but the demos start at score 0. The port
   starts a stage at the previous recorded stage's +0 (0 for the first). Which value does 0x452D60 actually load
   into score / displayed score?
7. Child aura 0x20 for ops 90–92 (07 §10.5): which interrupt (1 or 2) goes with human vs youkai mode?
8. Attract demo end (11 §4): ScreenEffect type 2 parameters (duration, colour) and the music fade duration. The port
   uses 120 frames to black and a 2 s fade. First demo index: the port starts with demorpy0 (n starts at 3) as the
   trace order suggests — what is GM+0x3DBB4 at boot?
9. PLST counters at StageStart (15 §2.9): in stage practice, are [0] plays / [1+shot] counted as well as [16]? Are
   [13] retries also counted as plays? The port counts only [16] in practice and both [0]/[1+shot] and [13] on retry.
   `Supervisor_UpdatePlayTime`: what interval counts as play time (port: wall time while a GM exists)?
10. VRSM: the port cannot compute the exe size/sum; it keeps the loaded values (tag "0100d"). Fine?
11. The trace's GM flags column has bit 16 set (score.dat unlocks). The port sets bits 15–17 at every menu
    registration from the in-memory CLRD (no notices yet). Confirm that playback/demos keep these bits.
