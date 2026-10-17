# Answers to docs/bugs/VISUAL_BUGS_1.md (RE side)

Sources:
- static reading of th08.exe: `Player_DamageAgainst` 0x451670, `Player_DrawBullets` 0x4512F0, `Player_DrawBulletsHit` 0x451400, `AnmFile_StartScript` 0x45E580, `AnmVm_Init` 0x4068E0, `Player_Die` 0x44AB40, `Player_DyingUpdate` 0x44CBF0, `Player_UpdateBomb` 0x44C650, `EffectMgr_Calc` 0x427BF0, `Effect_SpawnFixed` 0x425870;
- the data files `player00.anm`, `etama.anm` and `ply00as.sht`;
- the wine runtime probe (answers_phase8, `re/TOOLING_REPORT.md` §4).

Summary:
- In both cases the existing specs already stated the rule, but too briefly or in the wrong place. The likely cause is the implementation, but two spec statements were missing and one was misleading; they are fixed in place (§3).
- Reference screenshots are in `specs/data/ref/b1_*` and `b2_*`.

## B1. Hit effect of the Barrier team's focused shot

### What the original does (VERIFIED)

1. **Shot data.** Shot 0's youkai side (Yukari, focused) uses `ply00as.sht`.
   - The main needles are type 0, script 4, angle −π/2 (straight up).
   - The aimed amulets are type 1, script 6. Their angle comes from aim (init 1) or is the entry angle.
2. **Flying script.** `player00.anm` script 4 + 10 = **14**:
   - sprite 35, a **64 × 16** horizontal needle at (0, 160) of player00.png;
   - draw mode 2, `set_tag 1`, alpha 128, z-write off, then waits.
   - Script 16 (amulet) is the same with sprite 36 (64 × 16).
3. **Hit script.** `player00.anm` script 4 + 11 = **15**:
   - sprite 35, draw mode 2, `set_tag 1`, alpha 192, additive, z-write off;
   - `scale_time 10, mode 1, (8.0, 0.0)`: x grows to 8×, y shrinks to 0 over 10 frames;
   - `hide_stop` at 10, which frees the bullet.
   - Script 17 (amulet hit) is the same with sprite 36 and scale (5.0, 0.0).
   - So the hit effect is a streak along the sprite's **local x axis**.
4. **Rotation.** Both draw passes (flying, prio 9; hit, prio 10) do the same thing for each bullet: if VM tag
   (vm+0x1FC) ≠ 0, then VM rotation z := bullet angle (+0x450), with no offset. Then they draw rotated.
   - The angle of the main needles is −π/2, so the local x axis points **up the screen**.
   - The hit streak is therefore **vertical**, along the shot direction.
   - For aimed amulets it follows the aim angle.
5. **Script switch on hit** (`Player_DamageAgainst`, bullet state 1 → 2):
   - save rotation z (vm+0x08);
   - `AnmFile_SetScript(script + 11)` → `AnmFile_StartScript` → `AnmVm_Init`, which zeroes vm+0x000..0x207: rotation, tag, scale, colour, etc.;
   - restore rotation z;
   - spawn effect 5 at the bullet;
   - pos z := 0.1;
   - vel /= 8, unless type 3.

   The bullet angle is untouched, and the hit script sets the tag again, so the hit pass keeps rotating the
   streak by the shot angle every frame.

### Runtime check

Spell practice, Wriggle card (stage 1), shot 0, Shift + Z held, player not moving. Long vertical magenta/white streaks stand at the boss along the needle direction:
- `specs/data/ref/b1_barrier_focus_hit_s270.png`
- `specs/data/ref/b1_barrier_focus_hit_s300.png`

These are full 640×480 frames, `s` = Presents since scene 2 began; the boss is at the top centre of the playfield. No horizontal streaks occur.

### Probable port cause

The hit animation (state 2) is drawn without setting rotation z from the bullet angle, or the rotation is lost
when the VM is reset for the hit script. Unrotated, the x-scale of 8 produces exactly the reported horizontal
streaks.

### Spec status
- 14 §5 already said "hit animation: same rotation/tint rule". That was correct but easy to miss.
- 09 §4.5 did not mention the rotation save/restore, nor that the hit script re-sets the tag. It said only "hit script +11". **Missing → fixed.**
- 05 §2 described tag +0x1FC as "read only by owner code (stage/enemy)". **Misleading → fixed:** player bullets use it as the "rotate by angle" switch.

## B2. Death sprite that never disappears

### Which sprite it is

The red spiky disc in `b2_death_sprite.png` is the **deathbomb-window ring**, effect type 59 in fixed slot 11:
- colour 0xFFF0404F;
- 64 segments, script 78 = etama entry 2 script 2, sprite 226;
- radius 128 → 8, ease mode 4, over the window.

Near the end of the window, at radius ≈ 8, it looks exactly like the red burst in the screenshot. See
`specs/data/ref/b2_demo0_death_f4565.png` and `f4574`.

### Lifecycle (VERIFIED static + runtime)

Measured in attract demo 0: hit at gameplay frame 4548, shot 0, window 27 frames.
- **Spawn** (`Player_Die`): only when bombs ≥ 1 and not in dissolve mode (game flags bits 7–8 = 0).
  - The pointer is kept in player+0xE2B28.
  - The effect's +0x357 = 1, so it updates while bullets are frozen.
  - With 0 bombs (window 2) **no ring is created**.
- **It never ends by itself:**
  - its update 0x427990 always returns 1;
  - script 78 loops (`jump` back after a 10-frame wait);
  - the radius stays at 8 once the interpolation is done.
- **Freed only by game code.** The code sets effect+0x350 (active) := 0 and player+0xE2B28 := 0, in two places:
  1. `Player_DyingUpdate`, on the frame the window counter reaches 0 (real death), **before** spawning the death effects. Runtime: ring alive with age 1 … 27 over gameplay frames 4548 … 4574, gone at 4575.
  2. `Player_UpdateBomb`, when a bomb starts (deathbomb), also clears the freeze flag 0x400.
- **Real death**, at gameplay frame 4575:
  - burst **type 12** in fixed slot 3, colour 0xFF4040FF;
    - script 44 = etama entry 1 script 19: sprite 214 63×63, additive, alpha 255 → 0 over 40 frames, scale 0.5 + 0.3 per frame;
    - `hide_stop` at frame 40, so the effect manager frees it when `AnmVm_Run` returns 1;
    - runtime: alpha 0xF2 at age 1 → 0x00 at age 39, gone at age 40 (frame 4614);
  - 16 × type 6 sparks (30 frames each);
  - sound 0xF.

Reference frames: `specs/data/ref/b2_demo0_death_f4548.png` (hit, ring spawned), `f4565`, `f4574` (ring last frame), `f4575` (ring gone, burst 12 starts), `f4590`, `f4614` (burst gone).
- These are full 640×480 frames from attract demo 0, numbered by gameplay frame (GM+0x3DDC0).
- The player is in the upper right of the playfield, at about (310, 65) in playfield coordinates (screen + (32, 16)).

### Probable port cause

The port does not free slot 11 at the end of the deathbomb window (or on bomb start). Or it treats the ring as
"runs until its script ends", which never happens.

### Spec status
13 §2.6/§2.7 listed ring 59 and burst 12, but:
- **missing:** nothing said who frees ring 59, nor that it is never freed by itself;
- **missing:** the spawn condition (bombs ≥ 1, not in dissolve mode);
- **wrong:** type 12 was described as "deathbomb ring (fixed slot 3)". It is the burst at the **real death**. The fixed-slot list "3 / 11 death rings" was also corrected.

All three are fixed in 13.

## 3. Spec changes

| spec | change |
|---|---|
| 05 §2 (AnmVm +0x1FC) | tag: player bullets rotate by their angle when tag ≠ 0 |
| 09 §4.5 | hit transition: rotation saved/restored around the script restart; hit script re-sets the tag; angle unchanged |
| 13 §2.2 | fixed-slot list: 3 = death burst (type 12), 11 = deathbomb-window ring (type 59) |
| 13 §2.6 | type 12 row: real-death burst, script content, 40-frame life; note on anm_dis per-entry script numbering (global = 25 + entry-1 index, 76 + entry-2 index) |
| 13 §2.7 | player-hit row: ring 59 spawn condition and the two places that free it; real-death effects |
| answers_phase8 §1 | draw-phase RNG statement clarified: main thread only; loader-thread StageStart draws can fall between calc and Present |
