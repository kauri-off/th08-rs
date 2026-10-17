# Phase 3 — answers to docs/QUESTIONS.md

| # | question | answer (short) | where |
|---|---|---|---|
| impl2-1 | Main menu: title01.anm scripts, draw order, highlight switching, interrupts | **deferred** (menus → spec 11) | — |
| impl2-2 | AnmManager+4 global tint: scalar or D3DCOLOR; alpha? | D3DCOLOR at AnmManager+0 (B,G,R,A); +4 is only the enable flag; all four channels incl. alpha: min(255, c·t >> 7) | 05 §12.1 |
| impl2-3 | Op 59 with n = 0: RNG call? RNG algorithm/seed | n = 0 → result 0, RNG not called (unsigned modulo otherwise). Algorithm/seed **deferred** (11) | 05 §12.3 |
| impl2-4 | Draw-skip alpha check with colour2 (bit 17) | always the primary colour's alpha (vm+0x1F3); alpha2 never tested | 05 §12.2 |
| impl2-5 | Texture filtering / address mode | MIN/MAG LINEAR, MIP NONE, ADDRESSU/V WRAP (+ full device state list) | 05 §12.4 |
| impl2-6 | Blend state left by reset_render_state at frame start | the frame reset only clears the vertex batch; D3D state + caches persist from the previous frame; blend 2/3 set nothing | 05 §12.5 |
| impl2-7 | Where/when th08logo.jpg is shown | **deferred** (title/flow → 11) | — |
| impl2-8 | Supervisor flag 0x20 extra decrement | set by native 29 after a 1/n slowdown with n > 1, never cleared; every Timer_Add/Sub (incl. Decrement) takes one extra step, sub-frame 0, prev −999 | 05 §12.6 |
| impl3-1 | Draw3DWorld/mode 5 anchor offset for negative scale | absolute value (FABS) of half size | 05 §12.7 |
| impl3-2 | ANM at 0x17CE8F4 (overlay script 33); owner of 0x17D5EF8+5 | text.anm (slot 0); script 33 = blue (64,64,255) / white tint switched by the player mode byte; +5 = player's active character (human 0 / youkai 1) | 06 §13, 09 §1.1 |
| impl3-3 | Texture-matrix "last sprite" cache | only Draw3DWorld; reset each frame in Stage_DrawLow (and a few drawers); texture cache +0x24BC shared with 2D | 05 §12.8 |
| impl3-4 | Ribbons: vertex z and visibility | z = projected depth of A (v0,v1) / B (v2,v3); VM enabled+visible+alpha ≠ 0, direction length ≥ 1e-5, both depths in [0,1] | 06 §13 |
| impl3-5 | Billboard scale |s2 − s|: 2D or 3D | 3D length of the projected difference (incl. depth); uses the VM's current scale.x (feedback when quad.w == 0) | 06 §13 |
| impl3-6 | Seams between ground tiles | WRAP + bilinear, no half-texel offset for 3D quads; reproduce the same sampler state | 06 §13, 05 §12.4 |

Correction found while answering: ECL op 111 stores its int arguments raw (07 §7 updated).
