# IMPL108 questions

Answered in [answers_re_q44](../answers_re_q44.md).

No new SPEC-GAPs. Observations for the dirty room:

1. 10 §6 says the outline is always black ([RE-Q39](../answers_re_q39.md)), but `musicroom.rs` still passes an `edge` colour to
   `Raster::render_coloured`; not changed here (outside the brief).
2. Several callers (`gui.rs`, `menu.rs`, `menulist.rs`, `musicroom.rs`) rasterise with w = sprite width and an x offset
   into the sprite's rect, while 10 §6 says the copy always uses W = the texture-row width (512) and replaces the
   whole row strip. The gradient uses the 1024-pixel DIB row stride so a narrow strip matches the left part of a
   512-wide one, but the right-edge wrap of the TRIANGLE filter differs from the original for those callers.
3. 06 §10.2 still says "the calc-time tint-off of the bomb end happens before the draw"; 09 §7 / [RE-Q38](../answers_re_q38.md) say the bomb
   end has no tint-off. Implemented per 09 §7; the 06 sentence could be reworded.
