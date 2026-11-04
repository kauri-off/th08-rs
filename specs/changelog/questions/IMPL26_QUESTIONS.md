# IMPL26 questions (comment pass batch C: anm / anmvm / anmmgr / gfx / msg / hud / gui / ascii / text)

Answered in [answers_re_q25](../answers_re_q25.md).

No behaviour was changed in this pass. These are places where the code and the numbered specs disagree, or where code
has no spec section behind it. Each one has a `// SPEC-GAP:` (or a "Not implemented" line) at the given place.

## Code vs spec (decide: fix the code, or fix the spec)

1. **Draw mode 4** (`anmmgr.rs` draw dispatch). 05 §9 draws mode 4 as a billboard sized with the camera-right vector
   that Stage_SetCamera writes each frame. The port draws nothing for mode 4. Which shipped scripts use mode 4?
2. **Mode-5 vertex depth** (`anmmgr.rs`). 05 §9 uses the projected z as the vertex depth. The port keeps pos z.
   This changes depth-test results for 3D-drawn VMs against the stage geometry. Fix to 05 §9?
3. **Difficulty sprite in spell practice** (`hud.rs`). 12 §5.1 uses sprite 0x120 only for spell-practice ids ≥ 205.
   The port uses it for every spell practice. Fix the code?
4. **Spell-bonus replay exclusion** (`hud.rs`). 12 §5.6 excludes only "a replay with header +0x7B". The port
   excludes every replay. What does +0x7B hold, and which replays show the bonus?
5. **MSG op 14 flash** (`hud.rs`). 10 §5.1 makes it a ScreenEffect type 4 at draw prio 21 (with the effect's RNG
   and list placement). The port draws the flash inside the HUD at prio 17. Move it to the effect manager?
6. **Gui face −1** (`gui.rs`). 12 §5.8 skips only VM 0 for face −1 (dissolve spell). The port also leaves VMs 2/4
   untouched. Fix the code?
7. **Continue "Yes" default** (`ascii.rs`). 12 §4 also needs the card not captured (spell flag bit 9 clear). The
   port does not test it. Apply it?
8. **Menu viewport** (`ascii.rs`). 12 §3 / §4 draw the pause and continue menus in the playfield viewport over the
   capture VM. The port draws them full-screen with no capture. Fix the code?
9. **Night clock** (`hud.rs`, not implemented). The player-overlap fade and ECL op 180 of 12 §5.7 are missing.
10. **ASCII VM restarts** (`ascii.rs`, not implemented). 12 §5.9 step 1 also restarts +0x1520 (script 9) and
    +0x17C4.. (script 10). Only the gauge readout VM is restarted.
11. **stats+0x29** (`msg.rs`). 10 §4 step 3 calls it the practice flag; 16 §1 calls it the continue count. Which
    name is right? (The behaviour is the same either way.)

## Uncited code (no spec section)

- `hud.rs`: the stage-clear VM's sprite 0x80 + clock advance.
- `hud.rs`: the y of the 2× "+%d" spell-bonus line (12 §5.5 gives none).
- `hud.rs`: the erase patch at (512, 464), not in the 12 §5.3 repaint list.
- `hud.rs`: the playfield viewport for the title / clock VMs.
- `ascii.rs`: the playfield viewport for popups and the gauge readout (13 §4 draw 0x405420).
- `ascii.rs`: the "(p4)" part of a large popup, added for p4 > 0 (13 §4 does not say when).
- `ascii.rs`: slow mode is not configurable, so script 21 never runs (old gap).
- `msg.rs`: the per-stage face_stNN names (10 §3 names them only by range).
- `gfx.rs`: FOV π/6 / near 100 of `default_2d` (only the gfx tests use it).
- `text.rs`: D3DX filter TRIANGLE is approximated by an alpha-weighted 2×2 box; `BASELINE` is a port font metric;
  the GDI font itself is not reproducible (old gap).
- `anmvm.rs`: integer division/remainder by zero yields 0 (old gap).
