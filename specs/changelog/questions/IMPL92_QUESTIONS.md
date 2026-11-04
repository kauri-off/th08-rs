# IMPL92 questions

Answered in [answers_re_q34](../answers_re_q34.md).

No new SPEC-GAP markers. One observation from the golden draw diff (locating only, nothing fitted):

1. After the op 174 / ops 80–81 aura changes, golden `etama.anm#1` moved from missing 3091 / extra 396109 to
   missing 3104 / extra 396175 (changed 309048 → 295511). Is any other aura state written by op 174 or by
   `Enemy_ReactPlayerMode` (07 §10.1 step 3) besides interrupt, VM +0x14 and the colour2 flag — e.g. does the old
   aura (effect byte +0x350 := 0) still get drawn in its release frame?
