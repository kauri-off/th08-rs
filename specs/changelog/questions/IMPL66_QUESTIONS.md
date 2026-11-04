# IMPL66 questions

1. **Same-stage face reload (10 §3).** "Slots 16–19 are loaded only on a fresh start; a same-stage reload (0x17CE8C4)
   re-uses the loaded slots." The port's `Hud::load` still unloads and reloads the four face slots on every HUD load.
   Not changed here (outside A.62); proposed audit item.
2. **Score.dat re-read (15 §2.2–§2.3).** The port rebuilds the ranking lists from its in-memory HSCR entries, which
   equal what the last save wrote. When saving is off (`persist` false) or a write fails, the original would re-read
   the older file instead. Not a spec question; noted for the maintainers.
