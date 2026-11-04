# IMPL85 questions

Answered in [answers_re_q31](../answers_re_q31.md).

1. 07 §7 op 86 (`iget_boss`): the row says only op 87 tests that boss[slot] exists. What does op 86 do when the
   boss table entry is empty (null)? The port skips the op (`SPEC-GAP` in `eclvm.rs` ops 86 / 87); op 86 has 0 uses
   in the data.
2. 07 §10.1 step 7 lists "effect followers" in the tail after the death test; the port updates the attached effects
   and the aura position in step 5 (after the VMs, before step 6). Is that order observable (e.g. a mode-0 death
   ending the aura in step 6 before the follower update)? Not changed in impl85 (out of the brief's scope).
3. 07 §10.1 step 7 after a step-6 death: the port runs the whole tail (hit flash, boss marker, timer tick,
   damage-timer decrement) for an enemy that died this frame in any mode and only skips the link when alive is
   clear. Please confirm the tail is not skipped for mode 0.
