# IMPL113 questions

Answered in [answers_re_q47](../answers_re_q47.md).

1. `src/menulist.rs` replay_run (12 §2 intro, 15 §3.8): the cancels 2 → 1 and 3 → 2 zero +0x1442C. The screen's
   entry block is guarded by +0x1442C == 0 (12 §2 "Screen entry rule"). Does that guard also test the sub-state, or
   does the entry block (surface, range interrupt 14, cursor 0, +0xC2BC := 0, file scan) run again in the next call?
   And does anything else in sub-states 1–3 read +0x1442C (the port keeps it counting)?
2. `src/result.rs` back-to-list path (16 §2.6): the port runs the `Sound_Update` flush after the state code of the
   call (no SE follows it in that call). Confirm nothing between the flush and the end of the call plays an SE.
3. `src/game.rs` restart (00 §2 step 1): `Result_Register(2)` runs only if the PLST tag 0x164CD0C ≠ 0. When is
   that tag 0 at the restart (option menu exit)? The port always flushes, as at the normal exit.
