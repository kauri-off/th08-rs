# Restructure plan (Opus-only)

Done (2026-10-17): removed specs/glm, specs/haiku, specs/deepseek; old briefs → docs/archive/briefs/.

After the current RE run finishes (it writes into specs/opus):

- `specs/` — clean specs only (implementer reads): 00–16 .md, answers_*.md, structs.md, IMPL*_QUESTIONS.md.
- `re/` — dirty room: tools/, ghidra_scripts/ (+ specs/ghidra_tools merged), ghidra/, extracted/, symbols.csv, NOTES.md, drafts (bomb_draft, ecl_draft).
- `docs/briefs/` — active briefs + templates (TASK_TEMPLATE, IMPL_TEMPLATE, RE_WORKFLOW); finished ones → docs/archive/briefs/.
- Replace hardcoded `specs/opus` paths in tools (gh.sh, fn.sh, fnc.sh, redump.sh) with a path relative to the script; drop SPEC_DIR indirection in templates.
- Update docs/AGENTS.md, STATUS.md, FORMATS.md, templates and memory: single Opus RE agent + single Opus implementer.
- .gitignore: `re/ghidra/`, `re/extracted/`.
- Verify: redump.sh and fn.sh run from new location; cargo build/test unaffected.
