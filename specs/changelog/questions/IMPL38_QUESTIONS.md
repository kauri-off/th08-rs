# IMPL38 questions (spell ring visuals, silent card end, bonus-line fade)

Answered in [answers_re_q29](../answers_re_q29.md).

1. **Colour slot 1 and alpha** (07 §11.1 capture ring, 05 §6): `Spell_End` writes colour_end := D3DCOLOR 0x20D080A0
   and starts slot 1 (duration 60, mode 3). 05 §6 says colour / alpha slots lerp "per byte", while op 33 (05 §5) only
   writes R, G, B into the colour slot's start / end. Does slot 1 interpolate the alpha byte too (ring alpha → 0x20
   over 60 frames), or only B, G, R (alpha stays as the ring script leaves it)? The port lerps B, G, R only
   (`// SPEC-GAP` in `EnemyManager::finish_spell`); impl35 had set alpha := 0x20 at once, without a spec basis.
2. **g_Gui +0xC** (07 §11.1 step 1.1): where is it written? The port stores the boss's slot index e+0x2E0C at
   `Spell_Start` (`// SPEC-GAP` in `eclvm.rs` `spell_card`).
3. **Step 1.1 then 1.2–1.7** (07 §11.1): after step 1.1 clears bit 0, do steps 1.2–1.7 still run in the same frame?
   The port assumes yes (step 1.7 tests "bit 0 and bit 5", which only makes sense if bit 0 can be clear there).
   Please confirm in 07 §11.1.
