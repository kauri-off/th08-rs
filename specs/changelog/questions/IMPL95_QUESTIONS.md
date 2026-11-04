# IMPL95 questions

Answered in [answers_re_q34](../answers_re_q34.md).

1. 12 §5.7 / §5.1b: impl+0x21810 "counts live" stage-start tiles, but no spec section says who decrements it. The port
   decrements it when a tile's script ends (`AnmVm_Run` returns finished) and stops running/drawing the tiles at 0
   (`// SPEC-GAP:` in `hud.rs` `Task::calc`). Which code writes +0x21810 after `Hud_Load`, and on what condition?
2. 12 §5.1a labels the tile field "+0x120" as I0, 10 §9 labels the ghosts' "+0x120" as I4; 05 §2 says +0x120 = I4
   (var 10008). The port writes I4 for both. Please fix the I0 wording in 12 §5.1a if +0x120 is right.
3. Golden draw log: the reference has a second `K screen capture.anm#0 32 16 384 448 128 0` 122 presents after each
   op-9 capture, at game frame 0 of the next stage (presents 10918 and 25108), i.e. during the continuing stage's
   load. No spec section queues a capture there (12 §3 lists pause / continue / op 9 only). Who queues it, and does it
   overwrite the image the 168 stage-start tiles show (the tiles sample the capture texture)?
