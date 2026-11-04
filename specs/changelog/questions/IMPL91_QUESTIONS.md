# IMPL91 questions

Answered in [answers_re_q34](../answers_re_q34.md).

1. 07 §5.4 op 88 when boss[slot] is the caller itself (unused in data): during its own `Ecl_Run` e+0x2CA0 is the
   running context (a thread while a thread runs) and `Ecl_Run` holds the ip in a local copy (07 §7 op 95 quirk).
   Which ip is advanced and pushed, into which stack (main or thread), at which depth (0x2CEA = the running
   context's working depth?), and is the caller's running ip overwritten when the context stops? The port keeps the
   audit-D.4 behaviour (main context, op-52 style push at the main depth, no skip).
2. 07 §5.4: after the last thread of `Ecl_Run`, 0x2CEA holds that thread's depth. For a thread that returned from its
   first sub (slot freed, "depth := e+0x2CE8"), is the value written back to 0x2CEA the main depth? The port assumes
   so (`Enemy::work_depth`).
