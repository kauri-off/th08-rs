# IMPL103 questions

Answered in [answers_re_q38](../answers_re_q38.md).

1. 09 §7 (bomb start): the special-mode sentence says the cost block skip means "no unfreeze / ring removal / tint
   off", but the normal-mode list only names "bullets unfrozen, ring effect removed". Does a normal-mode start turn
   the stage tint off (which call, before or after the unfreeze), or is "tint off" a wording slip? The port does not
   turn it off at the start (marker in `src/bomb.rs` `start_bomb`).
