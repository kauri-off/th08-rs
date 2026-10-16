# Open questions for the dirty room (feed into the next RE briefs)

From impl2 (ANM VM):
- Main menu: which title01.anm scripts it starts, draw order, highlighted/normal sprite switching, interrupts sent. (→ menu/flow RE)
- AnmManager+4 global tint: scalar or D3DCOLOR; applies to alpha?
- Op 59 with n = 0: does it call the RNG? RNG algorithm + seed. (→ 11 RNG)
- Draw-skip alpha check when colour2 (flag bit 17) is active: which colour?
- Device texture filtering and address mode (MAG/MINFILTER, ADDRESSU/V).
- Blend state left by reset_render_state at frame start.
- Where/when th08logo.jpg is shown.
- Supervisor flag 0x20 extra timer decrement (slowdown restore).

From impl3 (STD):
- Draw3DWorld / draw mode 5: anchor half-size offset signed or absolute for negative scale? (stage 3 needs absolute to tile)
- Which ANM file/slot is at 0x17CE8F4 (overlay VM script 33)? Owner of byte 0x17D5EF8+5?
- Texture-matrix "last sprite" cache: shared with 2D paths? where reset?
- Ribbons: vertex z and visibility conditions.
- Billboard scale |s2 − s|: 2D or 3D length?
- Thin seams between ground tiles (stages 1, 3): texture address mode / texel offset for stage textures?

From impl4 (web/seams):
- Texture filter + wrap (MAG/MINFILTER, ADDRESSU/V, mipmaps) for ANM, stage and 2D draws. Seams on stages 1/3 come from texture data rows 0–1 vs 254 (filter at sprite edge).
- Any half-texel UV offset on top of sprite rect UVs?
