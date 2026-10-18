# Recorded sessions of the original (small files)

Format: `specs/data/drawlog/README.md` "Sessions". Each directory holds `input.txt` (TH08_INPUT, frame 1 = first main-menu frame), `polls.txt`, `events.txt` (boot seed, loader timing), `meta.txt`, `trace.txt.xz` (36-column state trace), `start/` (score.dat, th08.cfg the game read at launch) and `end/` (files after exit).
Draw logs are large and stay outside git in `sessions/<name>/draw_*.drawlog.xz` (chunks; `cat` them into one .xz stream).

- `golden`: human play, 25 634 frames (~7 min): menu → stage 1 → stage 2 → start of stage 3 → back to the menu. Replay in the original is identical (trace and draw log).
- `selftest1`: scripted, 1 922 frames: title → stage 1 → pause → quit → title → quit.
