# IMPL78 questions for the dirty room

Answered in [answers_re_q31](../answers_re_q31.md).

## Q1 — ECL op 128 argument d: int or float?

07 §5 op 128 lists the signature `raw, f a, f b, f c, i d`, and 13 §2.6b says the op stores d in enemy +0x53C4 as
the target radius, which `Enemy_UpdateOrbitEffects` compares against the float radius (+0x314).
Every op 128 in ecldata1–7 has d = 0x42C00000 or 0x43000000, the f32 bit patterns of 96.0 and 128.0. If d were
converted as an integer, the target would be ≈ 1.1e9 and the radius would grow 0.3/frame forever.
The port now reads d as the raw 32-bit word and treats it as f32 (`src/eclvm.rs` op 128, `// SPEC-GAP:`).

Please confirm: is d copied raw into +0x53C4 (an f32 field), or is it read as a float argument (`f`)? And if
its mask bit is set, which variable kind resolves it (int var or float var)?
