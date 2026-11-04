# impl10 — questions for the dirty room

Answered in [answers_impl10](../answers_impl10.md).

1. Large popups (`Popup_Large` 0x403460 / `Popup_LargeStatic` 0x403600): per-frame motion (rise speed?), alpha,
   glyph advance (8 px × scale like the small popups, or sprite width?) and anchor (centred / left) of the
   0x88 + g glyph strip. Is `Hud_DrawBoss` really issuing "+0" every frame for bosses without chained children,
   or is there a count > 0 test?
2. `Enemy_KillChildren` 0x42ADB0: the boss chain kill's 2·(boss +0x3380) time-orb shower — positions (RNG draws?
   order?), item mode; the single-kill child flag +0x2DA4 — which op sets it?
3. Stage sparkles: the 32-entry point array has no bounds check in the original; can more than 32 tagged quads
   be drawn in a frame in shipped data (overflow into +0x6600…)? The port stops at 32.
4. Native 12's chain part: what is the "e+8 link chain" (enemy list order, chain_next, or a different link)?
5. Spell-practice name plate / effect file choices (0x437F5C sprite exceptions, eff09xx table): needed once
   spell practice exists — confirm the spell id source (0x164D0B8).
6. Msg_Start 6B/Extra entry 0: what do the overlay base write (0x4E4B64) and the 0x437F5C call there do
   (which name-plate sprite)?
7. `Spell_End` with a last spell: message 6 "Last Spell Failed" — is bit 5 set by op 122 for specific card ids or
   by an ECL flag op?
8. Explosion hit filter 0x450C50: is the type-5 spawn (bullet timer % 6 == 0) done before or after the state-1
   enlarge branch, and also on filtered (skipped) frames?
9. `PlayerBullet_DamageAreas` `+0x3D` no-hit-effect byte: 0x450840 (Alice laser update 5) sets it on its one-frame
   rects — confirm (impl11 will create those rects).
