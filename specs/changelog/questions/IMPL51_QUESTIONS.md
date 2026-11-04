# IMPL51 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **Mode bits of a chained child (07 §10.5, §2.3).** §10.5 lists what ops 90–92 write after the spawn-time
   `Ecl_Run` (bit 8, bit 11, layer, contact off, aura) but not the rank-mask mode bits e+0x3330 (0x20 human /
   0x40 youkai, otherwise written only by `Enemy_ReactPlayerMode`, §10.1 step 3). Do ops 90–92 write e+0x3330, and
   if so before or after the spawn-time run (it decides which mode-masked time-0 instructions of the child run)?
   The port keeps writing it before the run (`eclvm.rs` `spawn_child`, SPEC-GAP).
2. **Op 92 follow setup timing (07 §10.5).** When are the follow offset e+0x2D40 := parent pos and flags A bit 9
   written: before the spawn-time `Ecl_Run` or after it (with the other §10.5 writes)? It decides the final pos seen
   by the child's time-0 instructions and the aura position e+0x2D88. The port writes them before the run.
