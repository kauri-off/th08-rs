# IMPL141 questions

Answered in [answers_re_q67](../answers_re_q67.md).

1. 12 §5.3 dirty-field writers: does `Spell_End` 0x4161B0 OR HUD +0x04 |= 0xF on every call, or only inside its
   card-active block? The port writes it on every call (`EnemyManager::finish_spell`). Only the last-spell label
   latch depends on it.
2. 12 §5.3 counter writers: does `ContinueMenu_Update` set 0x17CE8CC := 8 on every call or only on some phase? The
   port writes it on every call.
