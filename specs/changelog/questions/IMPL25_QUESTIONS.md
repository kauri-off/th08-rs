# IMPL25 questions (comment pass batch B: player / pshot / sht / bomb / effect / gamemgr / stage / stdfile)

Answered in [answers_re_q24](../answers_re_q24.md).

No behaviour was changed in this pass. These are places where the code and the numbered specs disagree, or where code
has no spec section behind it. Each one has a `// SPEC-GAP:` (or a "Not implemented" line) at the given place.

## Code vs spec (decide: fix the code, or fix the spec)

1. **Stage_Load order** (`stage.rs` `start`). 06 §2 starts the quad scripts (step 3) and then the overlay VM (step 4).
   The port starts the overlay VM first. Does this order matter (ANM RNG ops at t = 0)?
2. **Stage_Calc early return** (`stage.rs` `step`). 06 §5 returns at once while GameFlags bit 10 (death in progress)
   is set. The port never tests it, so the stage script, camera and sparkles keep running during the deathbomb
   window of a spell-card death. Apply it?
3. **Sparkle points** (`stage.rs` `billboard`). 06 §11.1 appends a point only when 0 ≤ z ≤ 1 and dist < fog_near,
   with no bounds check. The port tests `v < near` where v = dist inside near but the fog factor f beyond it (so it
   is true for every f < 1), has no depth test, and stops at 32 points. Fix to 06 §11.1?
4. **Sparkle flag +0x647C** (`stage.rs`). 06 only says `Stage_DrawHigh` clears it. Where is it set? The port sets it
   at the end of every `Stage_Calc`. Also, the port collects points only while +0xB24 ≤ 1. Is that condition real?
5. **Sparkles in spell practice** (`stage.rs` `step`). 06 §5: "frame > 699 **or spell practice**". The port does
   not test spell practice. Confirm that the alternative applies from frame 0 in spell practice.
6. **Player_Calc step 1** (`player.rs` `frame`). The freeze byte 0x160F534 (ECL native 26) should make the player
   calc set VM flag 0x80000 on the two aura effects and return. The port does not do this. Apply it?
7. **Focus switch resets +0xE2A9C for solos** (`player.rs` `move_player`). 09 §3 ties the reset to the team
   sprite-script reset. The port also resets it on a solo's focus change. Is the reset done for solos too?
8. **Game over resets the bombs** (`player.rs` `dying_update`). 09 §6.2 / 11 §4.2 set bombs = trunc(sht +4) only
   when a life is left. The port also does it on game over. Fix the code?
9. **Respawn with GameFlags bit 14** (`player.rs` `respawn_update`, `load`). 09 §6.3: no 240-frame invulnerability
   in spell practice; 09 §6.4: load timer 10 (not 120) with bit 14. The port applies neither. Fix the code?
10. **Yukari option branch order** (`pshot.rs` `place_option`). 14 §3.1 runs the animation test after the attack
    test every time, and it can overwrite sub-state 3. The port lets only a sub-0 start move win, so with sub 1/2
    and v.x = 0 (or a sign change) the port attacks where the original would switch the animation. Fix to 14 §3.1?
11. **Player laser and GameFlags bit 13** (`pshot.rs` init 4 / update 5). 14 §4 refuses a new laser and ends a live
    one while bit 13 is set. The port does not test bit 13 (the player only mirrors bits 7, 8, 14). Apply it?
12. **Shot update 3** (`pshot.rs`). 09 §2.1: vy −= Rng_Float × 0.1 + 0.27 with no condition. The port only does it
    in state 1 (so a bullet in its hit animation draws no RNG). Which is right?
13. **Hit-state entry order** (`pshot.rs` `damage_against`). 09 §4.5: save rotation, restart the VM, restore
    rotation, effect 5, pos z 0.1. The port spawns effect 5 (2 RNG draws) before the restart, and after the
    restart also re-sets the VM pos and flag 0x2000. The order matters only if a hit script draws RNG at t = 0.
    Confirm the order, and whether the restarted VM keeps flag 0x2000.
14. **Reimu LS blast** (`bomb.rs` `bomb_reimu_ls`). 14 §2.2 sets the blast object's VM script 0x14 before the RNG
    point. The port draws the point first. Same question: does script 0x14 draw RNG at t = 0?
15. **Stage-end "return 3"** (`gamemgr.rs` `stage_end`). 11 §4 step 1 returns 3 on the clear frame for stage
    practice (after score_display = score), 6A/6B/Extra and the night-over ending. The port does not return. It goes
    on to the confirm test in the same frame, so for stage ≥ 6 the scene is requested one frame early, and practice
    does not copy the score to the display. Fix the code?
16. **Subsystem registration order** (`gamemgr.rs` `stage_start`). 11 §3.1: Stage, Bullets, Enemies, Effects,
    HUD, Gui. The port registers the HUD before the Effects. Does any on-register callback depend on this order?
17. **BGM slots 1/2** (`gamemgr.rs`). 11 §3.1 "start BGM slots 0..2 from the STD names". 06 §1 says slot 0 plays,
    and slots 1/2 are used "if not ' '". Does "start" mean preload (slots 1/2 are played later by ECL/MSG)?
18. **GameFlags bit 2** (`gamemgr.rs` `calc`). 11 §2.1 recomputes it every GameMgr_Calc (paused == 0 and
    game-over == 0). The port sets it at stage start and clears it on remove. Apply the per-calc recompute?
19. **Screen effects** (`effect.rs`). Shake types 1 and 7 should also freeze on the freeze byte 0x160F534 and
    0x18B8A2C ≠ 0 (13 §3). Type 7 should run only when 0x164D2C8 ≥ 2. Type 2 should end when 0x18B8A2C ≠ 0. The
    port has none of these. Apply them?
20. **Invulnerability ring scale** (`effect.rs` update 23/24). 13 §2.6 interpolates from the *current* scale to
    1/16. The port interpolates from 1. Is the current scale always 1 when the ring is spawned?
21. **Effect list C** (`effect.rs` `draw_main`). 13 §2.4 draws list C (layer 2, e.g. orbit type 13) as a 3D
    billboard. The port draws it in 2D at pos. List B layer 3 should be a 3D quad, but the port billboards it.
    Is any shipped effect on layer 3?

## Uncited code (no spec section)

- `stdfile.rs`: quad records with a type other than 0/1 (and ≥ 0) are skipped without a VM index (06 §1.2 lists
  only 0, 1, −1).
- `player.rs` `START`: the player's position at load. 09 gives only the respawn point (192, 384).
- `player.rs` `real_death`: "all items in state 1 fall again" (0x441530, 09 §6.1 / §6.2) has no call here. The
  port relies on ItemMgr_Calc dropping attracted items while the player is in state 1/2 (08 §8.4). What does
  0x441530 write (state 0, vel.y −0.7?)?
- `player.rs` `dying_update`: last vx/vy := 0 and respawn_clear := 60 at the respawn start. 09 §6.3 says
  "+0xE2A70 = 60" but does not place it.
- `player.rs` `frame`: the bomb cooldown +0xE2A6C is decremented once per Player_Calc. 09 §1.1 does not give the
  place.
- `player.rs` `box_in_cancel_area`: box-vs-cancel-area test with no hit-counter / item update. Which original
  caller does this model?
- `pshot.rs` `origin_pos`: a shot whose origin option is free fires from the player position.
- `pshot.rs` hit filter 2: the counter (the port uses the shared +0xE2A94) and the effect position (bullet x,
  enemy y) are not in 09 §2.1.
- `bomb.rs` `Area::contains`: the geometry of a circle / rotated rect against an enemy *box* (09 §4.5 "overlap";
  09 §8 gives only the point test).
- `bomb.rs` `update_bomb`: no bomb start while respawning (state 1). This is not in 09 §7's start conditions.
- `bomb.rs` `update_bomb`: +0xE2A7C (bomb active) is cleared at the bomb end. 09 §1.1 says "1 from bomb start
  until…".
- `bomb.rs` / `effect.rs`: the draw priority 0x15 for code-spawned shakes and flashes (13 §3 gives it only for the
  demo fade).
- `effect.rs` `type_init`: types 22–25 start at the player position. The generic ring init (0x4272E0) defaults are
  32 segments (48 for 41) and centre = pos.
- `effect.rs` `death_ring`: the type-59 ring sets +0x357 (keeps running while bullets are frozen). 13 does not
  say which types set +0x357.
- `gamemgr.rs` `RANK_STATE`: the boot rank state (16, 32, 0, 0) before the first new game.
- `gamemgr.rs` `calc`: the 0x164D30C increment site. 11 §2 calls it a play-time counter in frames (+1800 per ECL
  op 148), but does not say where the +1 happens.

## Closed by the current specs (comment updated, no SPEC-GAP)

- Gauge-readout setup order vs Player_Load in StageStart: 12 §5.9 (the setup runs after Player_Load).
- The start-move script replays after a focus switch: 09 §3 (+0xE2A9C := 0 at the team sprite reset). Only the
  solo case is still open (Q7).
