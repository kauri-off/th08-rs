# IMPL80 questions for the dirty room

Answered in [answers_re_q31](../answers_re_q31.md).

## Q1 — native 8: which context is e+0x2CA0 of a ring member that is not running?

07 §12 native 8 writes int var 10037 into each member's **current** context (e+0x2CA0) and reads the leader's
int var 10007 there; 07 §4.3 only names +0x2CA0 "current context". For the caller it is the running context.
For the other members (not running at that moment) the port uses their main context (`src/eclvm.rs`
`familiar_ring`, `// SPEC-GAP:`). After `Ecl_Run` has run an enemy's op-135 threads, does +0x2CA0 still point at
the last thread's context, or is it set back to the main context (e+0x7F8)? Is int var 10038 of a member also read
through +0x2CA0?

## Q2 — native 8 with a stale index and no leader

When the caller is not reached by the walk (for example it was unlinked) and no member matches, k = 0 and there is
no leader, but the caller's own 10037 keeps its old value. If that value ≠ 0 the code in 07 §12 would read the
leader's orbit angle. What does the original do here (null leader pointer, or is the step skipped)? The port skips
it (`// SPEC-GAP:`). Probably unreachable in the data.
