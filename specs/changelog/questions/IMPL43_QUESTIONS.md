# IMPL43 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **13 §2.6 RNG draw-count line is stale.** The per-type summary still says "21/26 two (one if param > 0)", but the
   type-21/26 row ([RE-Q28](../answers_re_q28.md)) draws the random angle only when param.x ≤ −990.0, so it is one draw for any param.x > −990
   (0 and negative angles included). The port follows the row. Please fix the summary line.

Notes for RE (no spec change asked):

2. **Golden now stops at 10781** (first diff row 10782: score recorded 0x182AFF, port 0x2DD323; the port is ~1.4M
   ahead). Not investigated here (no deliverable).
3. **Fullrun_noclip stops at 9407** (first diff row 9408: flags recorded 0x10084, port 0x10004), as [RE-Q28](../answers_re_q28.md) predicted
   (~9408).
4. Demo MIN_SYNC unchanged (4628/3879/4035/6664): none of the three fixes reaches the demo divergences.
