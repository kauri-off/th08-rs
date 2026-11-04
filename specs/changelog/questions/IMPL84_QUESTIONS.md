# IMPL84 questions

Answered in [answers_re_q31](../answers_re_q31.md).

1. 07 §10.5 op 92 spawn position: "Op 91 position relative to the parent's final pos; op 92 additionally makes the
   child follow". Is op 92's (x, y) written to e+0x2D34 as given (the port's reading, kept as `SPEC-GAP` in
   `eclvm.rs` `spawn_child`), or as parent final pos + (x, y) like op 91? With the follow offset := parent pos the
   second reading would add the parent position twice in the final pos.
2. 07 §10.5 op 92 final pos e+0x2D88 := e+0x2D40 + e+0x2D34: is z also summed (the port does), or is z written 0
   like the other final-pos updates in §10.1?
