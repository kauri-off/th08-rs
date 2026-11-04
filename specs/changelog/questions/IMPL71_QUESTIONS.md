# IMPL71 questions

1. 15 §2.12 hidden code: what does a wrong key do (reset the counter +0x4C to 0, ignored, restart at 1 if it is
   Right)? Does releasing Focus/Skip reset it? Is +0x4C reset on entering state 3? Are the keys edge presses (port:
   yes)? The port resets on a press of another code key (to 1 if it is Right) and ignores frames without Focus/Skip.
2. 15 §2.12 "every card's attempt counter +1": which counter (story attempts [12] only, all 13 story entries,
   practice too)? For all 222 cards or only records with magic CATK (does it create missing records)? Is the CATK
   checksum +0x0E recomputed (if not, the next `Spell_Start` zeroes the counters)? The port: story [12] of present
   records, cap 9999, checksum recomputed.
3. 15 §2.12 / 16 §2.4 state 3: is the code tested before or after the page's cursor/confirm input in the same call?
   Port: before. Same for D in state 1 (16 §2.7): before or after the Up/Down/confirm input? Port: before.
4. 16 §4 Info_Show: where is t incremented relative to the input jump (is t = 5980 drawn, or is the next frame 5981)?
   Port: the jump happens before that frame's draw, so 5980 is drawn. Does the frame start with a clear?
5. 16 §4 says the fade-in alpha "falls to 23", but trunc((60 − t)·255/20) & 0xFF gives 23 at t = 18 and 10 at
   t = 19. Is the condition t < 19, or is the formula different? Port: the formula, t < 20.
6. 16 §4: the original's in-memory CLRD is empty at the first menu after boot; the port loads the whole score.dat at
   boot (game.rs SPEC-GAP), so it skips the notices when the previous scene is the boot scene. Confirm that nothing
   else (e.g. a demo returning to the menu) can show a notice.
