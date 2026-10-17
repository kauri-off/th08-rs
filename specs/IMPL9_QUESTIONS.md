# IMPL9 — questions for the dirty room

Found while implementing 12 §1–2, 13 §1–5 (menu, SE, effects). Each item: what the spec says, what the port
does now (marked `// SPEC-GAP` in src/), what to verify in the binary.

## Visual differences reported against the real game (coordinator screenshots)

1. **Hit spark (effect type 3, etama script 31, sprite 195 64×64, scale 3 → 0 over 40 frames, additive).**
   13 §2.6/2.7: "player bullet hits enemy → type 3 at the enemy box position". Port: one spark per frame
   whenever shot damage d > 0 (`src/enemy.rs`, shot-hit block). Against a boss this gives a constant stream of
   large spiky wheels flying out (v up to ±15 px/frame), which are not visible in the real game.
   Verify: the exact spawn condition (rate / frame counter / hit counter, boss vs. normal enemy, by-bomb
   exclusion), the colour argument, the count, and which box position is passed. Also confirm the RNG draws
   (spawning consumes 2 replay-relevant RNG calls per spark).

2. **Spell-card ring (type 39 / 52, scripts 76 / 77, sprites 221 / 223 = 16×768 in the 128×128 etama3.png).**
   Real game: a wide band of readable repeated text along the ring. Port: thin concentric lines, because
   `draw_ring` spreads the sprite's u (the 16 px axis) along the ring and puts v (the 768 px axis) across the
   width, so the text is squeezed across the band. 13 §2.5 says "u spread along the ring".
   Verify for 0x427450: per-vertex texture coordinates (which axis runs along the ring, outer vs. inner edge),
   whether the 768 px height means v ∈ [0, 6) with WRAP addressing (text repeated 6×), whether `uv_off`
   (scroll, op 4/26/27) is applied, and the start/end u/v per segment (0..1 over the whole ring, or per segment).
   Also: the width parameter comes from VM scale.x (script: scale 192 → 15 over 120 frames); confirm that
   192 is the initial half-width input to w = width / sin((π − 2π/n)/2).

## Effects

3. Death ring (type 59, slot 11): its radius animation is not given (port: VM pos.x 16 → 192 over 60 frames,
   mode 4). Its visible arc is partial (texture u-spread); probably the same mapping question as item 2.
4. Particle placement for types 19 / 51 / 63: the meaning and order of the 9 RNG draws around the stage camera.
5. ECL op 128 (effect 13): the per-frame "grow param to d, spin" update — rates and which fields.
6. Natives 3 / 6 / 20: the stage callback 0x4235A0 they install; whether the second effect (slot 10) is the same
   type with the extra script (port) or another type.
7. Table 0x4C7480 (stage → effNN.anm): port uses eff01, eff02, eff03, eff04a, eff04b, eff05 … eff08 by stage index.
8. Draw list B (background billboards): fog tint, z, and which camera.
9. Invulnerability ring / gauge aura / youkai aura: do they follow the player every frame, and from where.
10. Stage sparkles (type 0x3E at the sparkle points, 06 §10): not hooked yet; need the sparkle-point source.
11. Screen-shake consumers: which draws get the offset. Port: all 2D ANM quads + ASCII strings; the 3D stage
    (`draw_3d_world`) adds it to the world translation (no visible shake of the background). Verify whether
    Draw3DWorld adds it after projection.

## Popups (13 §4)

12. Popup colours for each caller (power, point / max point, converted point, full power, star, time orb,
    cancel bonus capped / not capped, kill_all). Port guesses: white, max yellow 0xFFFFFF40, PowerUp orange
    0xFFFFA040, time orb cyan 0xFF80FFFF.
13. Glyph 10 ("PowerUp", sprite 10 = 48×8): which fade sprites it uses at age ≥ 52 (sprites 21 / 31 are not
    PowerUp variants), and its centring.
14. Callers of the 3 extra slots (0x403330) and of the large popups (0x403460 / 0x403600).

## Menus (12 §1–2)

15. Title-menu description text: which VM/script draws it and its position (port: text raster at (128, 440)).
16. Option screen value VMs: mapping of the lives value to the digit VM (port: value v → sprite v + 1), the
    lives max (port 6), colour mode 0 = 32 bit?
17. Unlock rules for Extra / Spell Practice / practice stages (bits of the score file used).
18. Boot logo hold time (port 60 frames) and config defaults when th08.cfg is missing.
