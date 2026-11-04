# IMPL54 questions

Answered in [answers_re_q30](../answers_re_q30.md).

1. **+0xE2A7C and the special-mode press (09 §1.1, §7).** 09 §1.1 lists the failed start tests that clear +0xE2A7C
   (no press, tampered, dialogue, counter 0, bombs < 1, cooldown ≠ 0). A press in the special mode (0x164D0B4 bits
   7–8 or bit 14) passes those tests and only plays sound 0x29 (§7). Is +0xE2A7C cleared on that call too? The port
   keeps it (`bomb.rs` SPEC-GAP in `update_bomb`).
