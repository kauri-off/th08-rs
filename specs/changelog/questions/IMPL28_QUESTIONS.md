# IMPL28 questions (endings and staff roll, 16 §1 / §3)

Answered in [answers_re_q26](../answers_re_q26.md).

Each item has a one-line `// SPEC-GAP:` in `src/ending.rs` (or the file named).

1. **Default per-line wait** (`Ending::load_script`). 16 §1.2 step 6 says `Ending_LoadScript` sets "per-line wait
   = 8". Which field: +0x2A90 (normal), +0x2A94 (fast / min-wait), or both? The port sets both. Retail files
   always run `@s` before their first text line, so this only matters for a script that does not.
2. **Initial text colour +0x2AA0** (`Ending::new`). What is it before the first `@c`? (Retail files always set
   `@c` first.) The port uses 0xFFFFFF.
3. **`@R` on 16 VMs** (`Ending::reset_scripts`). 16 §1.3 says `@R` writes script index 0 on all 16 VMs, but the
   object has 15 text VMs (§1.2 step 4). Is there a 16th VM (for example a separate staff VM, or the field after the
   array), and what does the 16th write hit? Are the `@a` VMs 0..2 the same as text VMs 0..2? The port assumes yes,
   because §1.5 draws only the 15 text VMs and the staff sprites have to be drawn somewhere.
4. **Fade counter order** (`Ending::fade_step`). In 0x428910, is the alpha computed from the counter before or
   after it is incremented in the same draw call? And when a fade-in reaches t = n, is the overlay already off in
   that call (alpha 0), or is it drawn once more? The port computes the alpha first, then increments; a fade-in
   switches off on the first call with t ≥ n.
5. **Script end without `@z`** (`Ending::run_commands`). What happens when the reader reaches the end of the
   buffer? The port treats it as `@z`. Retail files all end with `@z`.
6. **More than 15 text lines in a page** (`Ending::text_line`). Line index ≥ 15 would write past the VM array.
   The port drops those lines. Retail pages have at most 5 lines.
7. **Picture source rectangle** (`Ending::draw`). +8 / +0xC are floats while scrolling (`@V240 1100`). Is the
   copy rectangle truncated, rounded, or is the copy a textured quad with sub-pixel UVs? The port truncates.
8. **"Clears some AnmManager state"** (`Ending::init`, §1.2 step 1). Which fields? Not modelled.
9. **Draw on the removal frame**. On `@z` the calc returns 0 and the object is removed (on-remove 0x429D40). Does
   the draw node go with it in the same frame (so nothing is drawn by the ending on that frame), and which callback
   of the 0x2AB8-byte object unregisters the draw node? The port draws nothing from the removal frame on.
10. **Text line terminator**. After a text line, does the reader also "skip to the end of the line and over all
    CR/LF" like after a command (§1.3 says this only for commands)? The port does; with `\0\n` endings both readings
    land on the next line.
11. **Unlock notices (16 §4)** are still not implemented (the info pictures after the run that unlocked them);
    this brief covered §1 and §3 only.
