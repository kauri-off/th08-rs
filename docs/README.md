# Clean-room implementer docs (for me)

Read order after any context loss: this file → STATUS.md → DECISIONS.md → AGENTS.md.

- STATUS.md — what is implemented, what is next.
- DECISIONS.md — architecture/tech choices and why.
- AGENTS.md — agent roster and workflow.
- FORMATS.md — format facts implemented, with source spec.
- briefs/ — templates, RE_WORKFLOW, active briefs; archive/ — finished briefs and history.

Layout: `specs/` clean specs (implementer input) · `re/` dirty-room workspace (Ghidra, dumps, tools, symbols, NOTES) · `src/` Rust.

Clean-room rule: I write Rust only from spec prose in specs/*.md. I do not read Ghidra output,
the agents' Python tools, or the binary's code. Comparing the agents' *outputs* (extracted files, PNGs) is fine.
