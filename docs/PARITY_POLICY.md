# Parity policy (2026-10-24)

- **Logic is bit-exact, no exceptions**: physics, RNG, ECL/boss scripts, collisions, graze, score, dialogue events.
  Every recorded session (golden, fullrun_noclip, selftest1, demos) must stay full.
- **Drawing must be visually indistinguishable, not 100 % in the draw log.** The draw diff is a bug finder.
  Port visible differences and spec-backed fixes that take a few lines (z values, 0.5 px offsets).
  Skip changes that only matter to the log and need architecture work: HUD dirty-row repaint with a kept back buffer
  (RE-Q32 CODE BUG 1), probe batch truncation (FORMAT.md §3).
- **No trace-fitting hacks in `src/`.** Original quirks documented in the specs are ported. Unexplained differences
  get a `// SPEC-GAP:` or stay accepted.
- **Final idiomatic refactor:** logic keeps its arithmetic and call order; quirks are isolated in small named helpers.
  The renderer may be rewritten freely while it stays visually identical. Regression checks then compare the port
  with itself: traces exact, draw log or images.
- **New code** (new subsystems while porting CODE BUGs) is written cleanly from the start. Existing code is not
  restyled before the refactor phase.
