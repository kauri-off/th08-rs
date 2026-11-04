# IMPL37 questions

Answered in [answers_re_q29](../answers_re_q29.md).

1. selftest1 row 1922 (second main-menu visit, menu frame 383, menu cursor column → −1): the port makes 2 RNG draws
   (rng 0x2ccc → 0x6d09, counter +2) that the original does not make on that frame. Looks like the menu idle →
   attract-demo start (heap jitter / demo setup draws, 11 §3.2, §4 step 6). On which frame do these draws happen in
   the original, relative to the menu's idle timeout?
2. 12 §5.10 says a mode-0 boss keeps its boss table entry; the port's table holds the slot index, so a later spawn into
   the freed slot is then seen as the boss (bullet env boss flag, 07 §11.1 `pull` uses boss slot 0). Is that the
   original's behaviour too (the table holds the enemy pointer), or does spawn clear stale entries?
