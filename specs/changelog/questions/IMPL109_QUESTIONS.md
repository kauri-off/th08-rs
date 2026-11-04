# IMPL109 questions

Answered in [answers_re_q45](../answers_re_q45.md).

No new SPEC-GAPs. Observations for the dirty room:

1. Out of brief: `menu.rs` `render_description` now follows 12 §2.2 ([RE-Q44](../answers_re_q44.md)): x = trunc(256 − len·15/4) with len = the
   Shift-JIS byte length, computed in f32 before the trunc (12 §2.2 writes "trunc", so a non-integer intermediate was
   assumed; integer `len·15/4` would differ by 1 px for odd len·15 mod 4). Confirm the arithmetic.
2. The text.anm / music00.anm strips are 17 rows tall but 10 §6 copies only trunc(fs2·sy) = 15 rows, so rows 15–16 of
   each strip are never written by `Text_Render`. The port now leaves them as they are; other callers that still write
   the sprite height (`menulist.rs` `render_strip`, zero-padded to 17 rows) are harmless but not per spec.
3. 16 §5.3 step 3 says the reveal runs "if sub-frame < 30" at sub-frames 10, 12, …, 22; the port tests the even values
   only (the sub-frame is always even there). Which test the original uses for odd values is not observable.
