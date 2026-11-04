# IMPL110 questions

Answered in [answers_re_q45](../answers_re_q45.md).

No new SPEC-GAP markers. One wording point for the spec:

1. 15 §3.5 says each キャラ名 is "10 full-width characters", but every listed name is 8 full-width characters
   (16 bytes, e.g. 霊夢＆紫　　　　). demo 0's USER line matches the listed 8-character string (test
   `replay::tests::user_chara_padding`). Please change "10" to "8" (or say "16 bytes") if that is what the table holds.
