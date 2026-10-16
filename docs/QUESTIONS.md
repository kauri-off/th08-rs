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
