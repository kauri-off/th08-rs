# IMPL17 questions for the dirty room

Answered in [answers_impl17](../answers_impl17.md).

1. Spell-card data: the per-difficulty card lists and the "Last" list (0x4C6B90), the difficulty/shot name table (0x4C8628), the unlock-hint texts (0x4C82C8). What does LastWord_ComputeUnlocks do with FLSP?
2. Stage loading frame count: the port uses 81 frames (replay) / 30 (otherwise); the original waits for its loader thread (see specs/data/drawlog/README.md "Loader threads"). Is the full-screen depth clear at the start of every gameplay frame unconditional?
3. Menu interrupts copied from the log without a rule: portraits ignoring the range interrupt on screen 11 (practice), heading VMs 138 (fades in at once) / 139 (hidden), interrupt 26 on the chosen team's portraits, VM 141 restarting its slide from (640, 320) on each entry. Why do the screen-16 rows sit 32 px left of VM 141? Practice screen: why does the description hide at once and the cursor start on the last stage?
4. After returning from the result viewer or music room: the menu reloads with different loading-animation timing and the top-menu description shows item 0 while the cursor stays on that item. Exit timing of the result viewer (port: 20 frames after the second X). What is the rule?
5. Demos:
   - Demo 1: vertex colours and positions of the `stg4abg` background.
   - Demo 1: the night clock (times.anm) appears ~129 frames early in the port. Why?
   - All demos: the unknown 2-digit ASCII text at alpha 0xD0 at the bottom right of the playfield.
6. Text rasterizer row layout (tab / column spacing): the screen-17 info panel rows 3–4 are ~20 px off; the 取得率 heading on screen 16 has a different weight.
7. Spell practice: does the fading description stay drawn? Is the card-name VM under the spell row drawn?
