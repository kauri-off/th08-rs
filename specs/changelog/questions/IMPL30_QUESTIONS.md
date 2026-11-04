# IMPL30 questions

Answered in [answers_re_q27](../answers_re_q27.md).

1. **ASCII frame counter +0x8284 (12 §5.10 blink).** Start value and when the first `Ascii_Calc` counts are not in
   the spec. The port starts at 0 when the ASCII manager registers at boot and increments at the end of every
   `Ascii_Calc`. Golden: marker positions, colours and the hit flash match on every synced frame, but the mode 2–4
   blink (sprite 158) is one frame early in the port in all three boss fights (presents 4300, 6944, 7872; the port's
   counter is 1 ahead mod 4). Is the counter zeroed somewhere else (stage start, `AsciiManager_Load` after a calc),
   does `Ascii_Calc` return early on some frames (before the increment), or is it the boot calc count?
2. **`Enemy_Free` "of a boss" (12 §5.10).** The port sends interrupt 2 / pos −999 only when flags A bit 1 is set and
   0 ≤ e+0x3313 < 4. Does `Enemy_Free` test the boss flag, or only the slot byte (which op 127 −1 does not reset)?
   What is the template value of e+0x3313?
3. **Death mode 0 vs the boss table (07 §10.4 table row 0–1).** The spec says mode 0 keeps the boss table entry; the
   port's `release()` (mode 0) still unregisters the boss (pre-existing). Not changed here; confirm whether a mode-0
   boss keeps its marker visible at its last position until `Enemy_Free`.
4. **Hit flash sfx (07 §10.1 step 7).** Now applied as written: the flash (colour2 + sfx 0x14/0x25) fires whenever
   d > 0 at step 6, also for enemies without flags A bit 3 (golden: the stage-1 midboss flashes the marker while
   invulnerable at present 3708). The old port also required life damage after reductions for the sfx. Confirm the
   sfx plays when the reduced damage is 0 (damage-reduction timer, spell-card bomb rule).
