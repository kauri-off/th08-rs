# IMPL81 questions for the dirty room

Answered in [answers_re_q31](../answers_re_q31.md).

No open questions: every deliverable was covered by 07 §7 ops 95 / 176, §10.1 steps 0–2, §10.3a and §10.6.
The two SPEC-GAPs this brief resolves were removed (op-176 box position; caller ip after its own op 95 / 123).

## Note (not a spec question)

`EnemyManager::kill_non_bosses` (threshold / timeout kill of every non-boss, 07 §10.2) still jumps through
`jump_to_death_sub`, i.e. with the full death reset (threads, descriptor, auto-fire, rank ranges, thresholds,
timeout). 07 §10.6 "Death subs" says that jump is the ip/clock/wait-only kind, like `KillAll`'s. Not in this
brief's scope, so left unchanged; it probably wants its own audit row.
