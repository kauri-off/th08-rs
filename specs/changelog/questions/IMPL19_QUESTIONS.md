# IMPL19 questions

Answered in [answers_impl19](../answers_impl19.md).

Tools: `demosync <n> [a-b] [rng-frame]` (`TH08_BDUMP=1` enemies + bullets), `session specs/data/sessions/<name>`.
Frame = trace frame (demo*_trace.txt column 1).

## Q1. Full-power clamp: is `power = 128` a checksummed write? (demo 3 frame 1059)
- Frame 1059: power 126 → 128, all bullets cancelled (49 → 0), score identical. Trace rng_ctr +12, port +8 (two
  `Stats::checksum` calls from `ItemManager::collect`, i.e. two power collects).
- Writing the clamp through the checksummed setter (+4 draws) makes demo 3 sync 1058 → 1500 and changes nothing
  else (demos 0–2, sessions). The port now does this under a SPEC-GAP (`src/item.rs`). Please confirm (08 §8.5:
  "power += n; if power ≥ 128: power = 128") and say whether the full-power item (type 4) does the same.

## Q2. Enemy screen check frees too early (demo 0 frame 1720, demo 1 frame 1808)
- Demo 0: e0 (sub 29, flags A 0x01001049, t 200) leaves the top. Port final pos (142.0, −9.7) at frame 1718,
  (143.4, −11.1) at 1719; the port frees it on 1720, the original keeps it (enemies 6 vs 5).
- Demo 1: e10 (sub 3, flags A 0x01011049, t 198) at (8.6, 459.1) on frame 1807, freed by the port on 1808; the
  original keeps it.
- The port's test (07 §10.1 step 4): overlap of final pos ± hitbox/2 with [0,384]×[0,448]. Which box (hitbox
  e+0x2D70, shot box, or a fixed margin) and which position are used, and is the comparison strict?

## Q3. Demo 2 frame 2261: port has 2 more bullets (0x8c vs 0x8a); demo 3 frame 1501: port 92 vs 80
- Not analysed further. Demo 3 frame 1501: RNG equal, 12 more bullets in the port after a frame of ECL effect
  spawns (enemy step). Any known bullet-spawn condition (e.g. a cap or a skip while the spell/boss starts)?

## Q4. Stage loading floor
- The port uses max(31 | 81, entries + 6), with entries = ANM texture entries loaded by the stage start (42 for
  stage 1 Reimu & Yukari → 48 frames, matching the sessions). Is the per-file decode time another frame per file
  (so a better formula is entries + files), or is +6 just host time?

## Notes (rules picked, no answer needed unless wrong)
- Replay fps text: refreshed on every 30th gameplay calc of a stage at phase 3 (demo 0 / demo 2 phase; demos 1
  and 3 refresh 2 frames later / earlier in the original, host timing). The buffer starts empty and survives
  between games.
- Ribbon stale colours: slots reset per `Stage_DrawLayer` call to the residue constants; manager tint applied when
  the vertices are emitted, not stored in the slots.
