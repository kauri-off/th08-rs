# IMPL148 questions

Answered in [answers_re_q70](../answers_re_q70.md).

Q1 (12 §1 point 3, 12 §5.3 step 6; `src/hud.rs` panel time row). The prompt alpha leaks into the side-panel strings
on the first gameplay frame, and golden 10982 shows the time row's "/" leaked as well, so the time row takes the
current queue colour when time < threshold. 12 §5.3 says "colour reset to white after" the time row. Is that reset
unconditional (also when time < threshold)? The port resets after the time row every frame. If the reset only
happens when time ≥ threshold, which caller resets the queue colour before the next frame's score row? Golden
10983 shows it white again.

Q2 (CODE BUG 6, 06 §10.2 / 14 §1.3, check only, no code change). The port makes the stage tint request and sets
`bomb_drew` only in `Bomb::tint`. That is called only from `draw_bomb` in the player draw (prio 9), which runs once
per presented frame and is gated by `bombing`, the same flag the calc clears at the end. Nothing in a calc pass
requests the tint, and the request is consumed by the next `Stage_DrawLow`. Code reading finds no path that
requests the tint on a later present. Please check after the C1/C2 fixes with udSs12 74263 / 74264 and udSe43
32812 / 32813. If the one-present shift is still there, we need the original's gate for the bomb draw fn call in
`Player_Draw` 0x44D530 (bomb timer vs duration, or +0xE2A7C) in a numbered spec.
