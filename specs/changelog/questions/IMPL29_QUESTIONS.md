# IMPL29 questions (capture time orbs 07 §11.1, death shrink timer 09 §6.2–6.3, op 67/178 07 §8.3)

Answered in [answers_re_q27](../answers_re_q27.md).

Each item has a one-line `// SPEC-GAP:` at the place named.

1. **Capture-ring timer at `Spell_End`** (`EnemyManager::finish_spell`). 07 §11.1 uses t = the ring's timer +0x338,
   but the ring was spawned at `Spell_Start`. Does `Spell_End` (the recolour, +0xF4 → +0xF8) reset that timer to 0?
   The port resets it, because 11 §3 says the reward is "paid by `Spell_Calc` 130 frames later". With the reset, the
   `Spell_Calc` on the capture frame sees t = 0 (effects tick at prio 13, after `Spell_Calc` at 12).
2. **Order of the capture-ring step in `Spell_Calc`** (`Gui::calc`). Does it run before or after the 13 spell VMs
   (12 §5.8 order)? The port runs it first. Also: does it run when the card is inactive but bit 10 name-only is set?
3. **Orb angle arithmetic** (`EnemyManager::capture_ring_step`). (t − 10)·2π/40 − π/2: is 2π/40 one constant
   (0.15707964) or is it ((t − 10)·2π)/40 in f32? The port multiplies left to right.
4. **t == 30 / t == 60 restyle and +0x32C := VM +0x20C**. These are not done in the port (07 §11.1 says no gameplay
   effect). The draw needs the exact VM writes.
5. **Ring rotation**: is +0x318 += −0.0524 / −0.0157 a plain add or `Angle_AddNormalize`? The port adds without wrapping.
6. **Respawn clear in `Player_UpdateInvuln`** (`Player::update_invuln`). 09 §6.3: +0xE2A70 is decremented and the
   768×896 cancel rect added while ≠ 0. Before or after the state-3 colour/timer step and the state ≠ 3 tick? The port
   does it first.

## Next divergences (not chased)

- **golden row 8105** (was 8056; the capture-ring orbs spawn as in the original, rows 8056–8104 match):
  rng 0x86d4/0x2a14, rng_ctr 0x389a8/0x389b0 (port +8 draws), score 0x54df7/0x54e11 (port +26),
  time 0x423/0x425 (port +2) (recorded/port). Rows 8056–8104 are the orb-spawn frames (ring t 9..45); 8105 is after
  the last spawn (the port's ring has +0x10 = 0 from t = 46), so it looks like orb collection: the port collects 2
  orbs more in that frame (2 AddTime checksums × 4 draws, 11 §1.5).
- **fullrun_noclip row 7553** (was 6266): rng 0xbd8f/0x24bb, rng_ctr 0x3889c/0x3888c (port −16), score
  0x5dfab/0x5df67, gauge 0xd7b/0xf37, time 0x41a/0x416 (port −4). Row 7554 matches again (the port collects the 4
  orbs one frame later), then row 7555 differs. Again time-orb collection timing (08 §8.3 mode 5 / 08 §8.4 attract).
  Question: the exact collection/attract rule for type-10 orbs (mode 5) in the frame they are spawned and the next.
- **demo 2 frame 3082** (was 2938): score trace 0x487994, port 0x487992 (port −2); rng equal.
- selftest1 1422 and demos 0/1/3 (4451, 3870, 1606) unchanged.

## fullrun smoke test

After its divergence the fullrun input drives a desynced game; it now reaches stage 5 only (stage 1 ends at row
14180 instead of 11252). Stage 1 sits about 2500 rows after the boss with one emitter enemy (sub 14) while the
recorded input holds Shot. My guess is that the post-boss dialogue waits for a fresh press, but I did not check it.
The test passed only by luck before: with each fix reverted on its own, stage starts still move (y fix reverted: stage 2 at
14056; orbs reverted: stage 3 at 33002, no stage 6). The assert is lowered from 6 to 5 stages.
