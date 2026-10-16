# Clean-room implementer docs (for me)

Read order after any context loss: this file → STATUS.md → DECISIONS.md → AGENTS.md.

- STATUS.md — what is implemented, what is next.
- DECISIONS.md — architecture/tech choices and why.
- AGENTS.md — dirty-room setup: agents, task IDs, briefs, how to compare results.
- FORMATS.md — format facts I implemented from, with source spec + whether both agents agree.
- COMPARISON.md — per-phase comparison of GLM vs DeepSeek specs.

Clean-room rule: I write Rust only from spec prose in specs/*/ *.md (and NOTES.md). I do not read Ghidra output,
the agents' Python tools, or the binary's code. Comparing the agents' *outputs* (extracted files, PNGs) is fine.
