# IMPL35 questions

Answered in [answers_re_q29](../answers_re_q29.md).

1. `Spell_Start` with flag A bit 27 (07 §11.1): is the rate +0x100 computed from the card's own bonus or from
   99 999 990? The port uses the card bonus (it only matters through `Spell_AddBonus`, since decay is skipped
   under bit 27).
2. `Spell_AddBonus` while no card is active (bit 0 clear, e.g. orbs collected after `Spell_End`): does it still
   change +0xFC / +0x100? The port ignores it (no spell object state outside a card).
3. Next divergences after this impl are the ones [RE-Q27](../answers_re_q27.md) left unchased: golden 8298 / fullrun_noclip 7746 (rng −8
   draws, bullets 2/1). Not investigated here.
