# IMPL50 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **Caller's ip after its own death-sub jump (07 §5.2, §7 op 95, §11.1 op 123).** When op 95 (or op 123's
   `KillAll`) runs in the main context of a non-boss without flags B bit 6 that has a death sub, `EnemyMgr_KillAll`
   sets that same context to the death sub (ip, clock, wait). 07 §5.2 then does "unless the op transfers control:
   ip += size". Does `Ecl_Run` add op 95's size to the **new** ip (skipping the death sub's first instruction), or
   does it read the ip back / continue at the death sub start? The port continues at the death sub start in the
   same frame (`SPEC-GAP` in `eclvm.rs` `with_self_in_slot`).
2. **Same case from a thread (07 §5.4, §10.3).** The death-sub jump frees all 4 threads, including the one executing
   op 95. Does that thread finish its frame (the port: it keeps running this frame, then is dropped, as op 135 on its
   own slot) or does `Ecl_Run` stop at once?
3. **Death-sub jump reset for `KillAll` (07 §10.6 "Death subs").** That paragraph says the threshold/timeout kills
   reset "main context ip/clock/wait only", while normal death does a full context reset. Is `KillAll`'s jump the
   same "ip/clock/wait only" kind (threads, bullet descriptor, rank ranges kept)? The port (before and after this
   task) applies `reset_for_jump` (threads, descriptor, auto-fire interval, rank ranges) for every death-sub jump.
