# IMPL70 questions (music room 16 §5, music off 02 §4)

1. **Warning table 0x4C7AAC (16 §5.3).** Only lines 0–1 are given ("＊＊＊注意＊＊＊", "この曲はまだ、ゲーム中で流れておりません …").
   Please give all seven lines byte-exact (and whether "…" is part of line 1).
2. **Colour byte order (16 §5.2–§5.3).** Are 0xC0E0FF / 0x302080, 0x80A0C0 / 0x100040 and 0xFFE0C0 / 0x300000 0xRRGGBB or GDI
   COLORREF (0x00BBGGRR)? The port assumes 0xRRGGBB.
3. **Text call (16 §5.2).** Which text function draws the titles with a coloured outline (Text_RenderToVm's outline
   argument is −1 / 0, 10 §6)? Outline offsets ±1 or ±2 px? Font size? The comment lines' colours (16 §5.3 gives none)?
   The port uses ±2 px, size 15, comments white / black.
4. **Sprites of the text VMs.** The port renders into the sprite each VM's script sets on frame 0 (music00.anm scripts 1–21,
   text.anm scripts 10–16 and 0x11). Confirm, or give the sprite / strip per VM.
5. **Draw (16 §5.2).** Order of: surface 0, VM +0x88, title rows, numbers, cursor marker, "Now Playing", playing-title VM,
   comment VMs. The cursor marker string at x 33 and its colour; colours of `%2d.` and "Now Playing". Are the comment VMs
   drawn at their script positions? Is the playing-title VM centred at (320, 52)?
6. **Calc (16 §5.3).**
   a. Initial `playing` (+0x?): 0 or −1? It decides what R does before a selection and whose comment shows unheard.
   b. Does the sub-frame restart when state 0 → 1 (8 frames), or keep counting into the reveal at 10?
   c. Order of the key tests and whether more than one can act in a frame; Ctrl edge or held; SE on cursor move / select / exit?
   d. R: `Music_PlayFile(entry.file, 0)` again (also marks flag 0)?
   e. VM run order within the calc (before / after the state code); which VMs are run.
   f. Comment VMs before the first reveal (state 0 interrupts them with 1): what text do they hold?
7. **Music mode 0 and heard flags (02 §4).** Do `Music_PlaySlot` / `Music_PlayFile` still mark the heard flag when music
   mode is 0? The port marks flag 0 in the music room in any mode.
8. **In-game heard flags (10 §5.1 op 7, 02 §4).** Table 0x4C7240 (`stage*3 + n` → music-room index) and the `track` argument
   of the deferred `Music_PlaySlot(0, track)` at stage start are not in a numbered spec, so the port never marks stage
   tracks heard; only the ending (18, 19) and title (0) flags are set. Please add them (separate audit item).
