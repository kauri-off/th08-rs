# IMPL102 questions

1. `src/eclvm.rs` `Exec::interrupt` (07 §5.5): the handler has no range test on k, and the table e+0x2CF0 is
   i16[32]. What does an interrupt index k ≥ 32 (or a k that reaches past the table into e+0x2D30…) read? The port
   uses sub 0. Unreachable in the shipped data (every timeline op 8 / op 125 / op 89 sends index 1).
