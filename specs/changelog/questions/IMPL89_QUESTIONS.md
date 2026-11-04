# IMPL89 questions

Answered in [answers_re_q32](../answers_re_q32.md).

1. 07 §6 / op table: the in-place **int** ops (5 loop counter, 10–14, 30/31) with a variable destination 10042–10044
   (a float variable, not a valid int destination): does the destination read go through the int reference
   (0x41FE10, i.e. the instruction's own argument slot in the file) or through the int getter (final pos, truncated)?
   The port reads it through the getter and writes into the file slot. Probably 0 uses; spec silent.
2. 07 op 155 with no card active (spell flag bit 0 clear): +0x2638 is still written; the port keeps the cap only inside
   the running card, so a later `Spell_AddBonus` without a card (07 §11.1) cannot see it. Confirm unreachable in the
   data (op 155 always follows op 122).
