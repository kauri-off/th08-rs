# impl12 — questions for the dirty room

Answered in [answers_impl12](../answers_impl12.md).

1. **`Enemy_KillChildren` path choice ([answers_impl10](../answers_impl10.md) §2).** A chained child's +8 is its *next sibling*. When a child
   with later siblings is freed / times out (reward 0), does the chain-kill path run on those siblings (e+8 ≠ 0), or is
   the chain path gated on "no parent"? Exact order of the two tests (single-kill: parent ≠ 0 && reward ≠ 0)?
   The port only chain-kills from enemies without a parent.
2. **Parent child counter +0x3380 after a chain kill.** Is it reset (or decremented per child) by `Enemy_KillChildren`,
   or does it keep counting? The HUD "+p3(p4)" shows it as p4 (e.g. "+0(3)" after the children died).
3. **Boss part of `Enemy_KillChildren`.** Is step 5 (2× scale popup, 2·count orbs, cancel circle, gauge-lock 0) gated
   on the parent being a boss (flag A bit 1), on reward only, or on something else?
4. **Orb count `c` vs the bombing ÷3 (§2 step 3).** Is `c` computed from n before or after the n := n/3 of the same
   child? (Port: before; popup shows the divided n.)
5. **GameFlags bit 13 damage box** (`EnemyMgr_Calc`, boss present): position, size and damage of the large
   `Player_DamageAgainst` box.
6. **Native 22 `Spell_ShowNameOnly`**: the face argument passed to `Gui_ShowBossSpellName` (−1?), and does it touch
   the active card (bits 0/4 cleared while a card runs)?
7. **Spell history digits (12 §5.8).** Without score.dat the port counts captures/attempts per session. When is the
   CATK attempts counter incremented (at `Spell_Start`, also for timeouts / in practice), and the captures counter
   (at `Spell_End` capture)? Are they per difficulty too?
8. **Spell practice pieces not yet used**: effect file table 0x4C7144[id] contents (eff file names for ids ≥ 0xD8).
9. **`Gui_ShowPlayerSpellName` global 0x17CE8CC := 2** — what reads it?
