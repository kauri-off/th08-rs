# IMPL18 questions (session replay blockers)

Answered in [answers_impl18](../answers_impl18.md).

Tool: `cargo run --release --bin session -- specs/data/sessions/<name> [--no-drawlog]`. "Row" = trace row counted from
the first main-menu row (= poll index + 1 = input.txt frame). Game frame = trace column 1. Values recorded / port.

## Q1. golden row 2173 (stage 1, game frame 1451): enemy killed only in the original — Yukari option attacks at frame 1432
- Row 2173: rng 0x20d4/0x675a, rng_ctr 0xa418/0xa3f8, score 30413/30359 (+54 = one kill), gauge −7621/−7821 (+200
  kill bonus), enemies 8/9. Everything (incl. the 6 drawn enemies' quads) is identical up to row 2172.
- Root cause seen in the draw log: from present 2167 (game frame 1432) the original's Yukari option (player00.anm,
  z 0.49) switches to the rotated 46×46 sprite uv (0.316, 0.566)–(0.496, 0.746) and flies off toward y ≈ 200
  (presents 2174–2186), i.e. it enters sub-state 3 (14 §3.1, attack). Its shots tilt with it. In the port the option
  stays in sub-state 1 (following, moving left) because there is no aimed enemy.
- Port state at frame 1431/1432: player (285.75 / 283.75, 400.25), shot + focus + left held, option t = 191, sub 1.
  Alive enemies (slot, final pos, flags A): e0 (30, −16) 0x1000005d (bit 4 set), e1 (204.5, 203.8), e2 (199.4, 180.1),
  e3 (193.1, 150.9), e4 (118.2, 116.1), e5 (101.9, 73.7), e6 (84.3, 26.5), all 0x0101204d. None is within 64 px of
  the player's x, so the port picks no aimed enemy. The next enemies the port spawns are e7 at (273.1, 12.6) at
  frame 1448 and e8 at (263.7, −21.7) at frame 1451.
- Question: which enemy is the aimed enemy (+0xE2ABC) in the original at frames 1431–1432 (slot, final pos, base
  pos, flags A, when it was spawned and by whom)? Does an enemy exist that the draw log cannot show (above the
  screen, no main VM script), e.g. the e7/e8 spawn happening earlier than in the port? Or is the 64-px column test
  against something other than the player's x?

## Q2. selftest1 row 1326 (stage 1, game frame 712): item drop mode while bombing
- Row 1326: power 0x40a00000/0x40c00000 (5.0 / 6.0), score 3544/3545, rng_ctr 0x5198/0x519c. Identical before.
- A Yukari bomb (index 1, duration 150) started at frame 684; at frame 712 the player is still in state 3, bomb timer 28.
- An enemy dies at frame 712 and drops a power item (type 0) at about (234.4, 212.5). In the original's draw log the
  item is not attracted: it rises like a mode-0 drop (presents 1326–1330: y 210.5, 208.5, 207.5, 205.5, 203.5).
  The port spawns it in mode 1 (auto-collected) because `hitByBomb` starts as "player is bombing" ([answers_phase5](../answers_phase5.md):
  "item mode m = 1 when the player was bombing on the killing frame (the hit-by-bomb flag starts as player +0xFDC)"),
  so it flies to the player at speed 10 and is collected one frame later.
- Question: what is the exact condition for drop mode 1? Which field is +0xFDC and when is it set/cleared during a
  bomb (is it the bomb object's "damage active" flag rather than the whole bomb duration)? Is the mode taken from the
  killing call's `hitByBomb` output after the damage areas were tested?

## Q3. Smaller items (no trace effect)
- Enemy draw order (07 §10.6 / §10.1 step 7): the session logs show each layer list drawn in **reverse** link order
  (newest-linked first) and enemies spawned during a calc not drawn in that frame. The port now links at the end of
  each enemy's calc and draws each list backwards. Please confirm the list insertion (prepend) and which enemies a
  frame's calc links (only those processed by that frame's loop?).
- Hit flash (07 §10.1 step 7): hits on three consecutive frames flash only on the 1st and 3rd (golden presents
  1936–1938, 1948–1949). The port now skips the flash (and SE) when the previous frame flashed. Exact rule?
- Bomb gauge (09 §7, "Gauge while bombing"): the trace shows no gauge change on the bomb's start frame
  (selftest1 frame 684: −1975 → −1975, then +173 per frame). The port adds it only in the per-frame branch of
  `Player_UpdateBomb`. Correct?
- Focus switch while moving (09 §3): after the sprite-script reset the original replays the start-move script
  (golden present 932: focus released while moving right → script 3 sprite). The port now treats the previous vx as
  0 after a switch. What does the original store/compare?
- Character select cursor (12 §2.4): the cursor is already the shot type at the start of the screen's first calc
  (trace column cursor, selftest1 row 440: 0 while the difficulty cursor was 1). Where is it set (difficulty confirm)?
- Main-menu description text (text.anm script 9) at the loader release frame: the original's fade-in is one tick
  ahead (selftest1 present 47: alpha 0x19 vs port 0x0C). When does the description VM get its interrupt 1 relative to
  `Loading_End`?
- Title surface order: on every menu frame the original logs `K title00.png` before the nowloading VM quads; the
  port draws the VMs (Supervisor prio 2) before the menu surface (MainMenu prio 3). Which node draws surface 0?
