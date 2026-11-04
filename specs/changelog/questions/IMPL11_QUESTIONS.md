# impl11 questions for the dirty room (bombs, options, player laser)

Answered in [answers_impl11](../answers_impl11.md).

1. **Bomb tint lifetime (14 §1.3, 06 §10.2 step 3).** `Stage_DrawLow` applies the requested tint (stage +0x646B) to
   the global ANM tint. When is the global tint switched off again (AnmManager +4 := 0)? Does it stay on for the
   enemies/bullets/player drawn later in the frame, or only for the stage layers? The port switches it off at the
   end of `Stage_DrawHigh` (only the background is darkened). What is the stage flag 0x4EA49C used for?
2. **Spark flag per area ([answers_impl9](../answers_impl9.md) §1 vs 14 §2).** [answers_impl9](../answers_impl9.md) says Reimu / Reimu LS / Marisa / Marisa LS set
   +0x3D ("no hit effect"). 14 marks it only on the orb damage circles (2.1) and Marisa's rectangles (2.5). Do
   Reimu's impact circles (dmg 500 / 25) and the Reimu LS blasts (dmg 400) also have +0x3D set?
3. **Reimu orb areas during the spin-out (14 §2.1).** "State 1: areas follow the orb" is listed under T ≥ 40. Do the
   attached C r 96 / D r 64 circles stay at the player's start position for T < 40, or do they follow the orbs then too?
4. **Reimu LS start effect.** Does 0x40C910 spawn effect 12 at the player (0xFF4040FF) at T = 0 like 0x40C010?
5. **Sakuya knife hit test (14 §2.7).** "If its D area has dealt damage" — after "areas off" the area pointer is
   still set and its total stays > 0. Is the hit branch guarded (pointer cleared / area active) or does it re-run
   (script 0x17 restarted) every frame until age 120?
6. **Alice ring colours.** Colours of effects 0x2A–0x2C at T = 90 (and LS T = 120)? The port uses 0xFFFFFFFF.
7. **Alice ring 0x2A before t = 40.** Amplitude (VM pos.y equivalent) for t < 40: 0 (plain ring) or 64k as after?
8. **Remilia rings (14 §2.9).** Start of the pos interpolation (0, 0, 0 like Marisa LS?), scale mode for the LS
   64 → 128 width interpolation, and do odd n also switch to script 0x5C as in Marisa LS?
9. **Team Remilia bats (0x44EB70).** Start orbit angle (solo: 0, π, 0, π) and do they spawn effect 0x2F?
   Yuyuko options (0x44F930): colour of their effect 0x2F?
10. **Sakuya LS window SE / effect 20.** In 2.8, is SE 6 still played every frame of the window (only the shake moved to
    T = 0)? Is effect 20 at the player kept and moved while T ≤ 60 as in 2.7?
11. **Effect lifetimes.** When are ring effects 0x2E (Sakuya LS, radius +8/frame) and 0x24/0x25 (after their side
    rectangles) freed — only when their VM script ends?
12. **Yukari option attack (14 §3.1).** The aimed enemy (+0xE2ABC) is a pointer kept across frames; in sub-state 3 is
    it re-picked each frame by `EnemyMgr_Calc` (smallest y within 64 px of the player) or does Yukari keep chasing
    the same enemy even when it leaves the 64-px column?
13. **Spell-name plate.** `Gui_ShowPlayerSpellName` ([answers_phase5](../answers_phase5.md) row "Gui spell-name plate") is not drawn yet
    (neither is the boss one): is there a spec section with the plate's position/animation (face_cdbg scripts 0/2,
    text.anm script 4) and the cut-in (player face slot 16/17 script 0/1)?
14. **Dissolve spell.** The special mode (0x164D0B4 bits 7–8 / 14) that triggers it is not specified: which game modes
    set those bits (spell practice?), and the colour of the end flash (3: 30, 1, colour)?

## Visual bugs ([docs/bugs/VISUAL_BUGS_1.md](../../../docs/bugs/VISUAL_BUGS_1.md))

15. **(Answered in specs/[answers_bugs1](../answers_bugs1.md), implemented.)** **B2 — death ring 59 (fixed slot 11) after a real death.** [answers_impl9](../answers_impl9.md) §P2 "Death ring": "Script 78 (sprite 226):
    … scale.x 20 → 8 over 120 frames; **loops**" and "Radius: VM pos.x 128 → 8 (shrinking) over D frames". 09 §7
    ("normal mode: bullets unfrozen, ring effect removed") removes it when a bomb saves the player (now done in the
    port), but 09 §6.2 (real death: "effects + sound 0xF; bullets unfrozen; …") and 13 §2.7 ("at death time type 6
    ×16 again and ring 12 (slot 3)") never free slot 11, and script 78 never ends (op 0 at t=10 then a jump back).
    So after a real death the ring stays forever at radius 8 at the death spot (the red burst in
    `b2_death_sprite.png`). Who frees / interrupts slot 11 on the real death (or on respawn), and when?
