# th_08 project rules

These override the global "no comments / one short line" rule for this repo.

## Comments in `src/`

This is a port: a reader must be able to check the code against the specs. Write comments for:
- each module: a one-line `//!` naming what it ports and the spec section (`//! ECL enemy VM (07 §5–§10)`);
- non-obvious functions, fields and constants: the spec section, units, fixed-point/angle format, the original's name;
- quirks of the original kept for bit-exactness (call order, off-by-one, odd clamps, original bugs): what and why, 1–3 lines.

Do not restate what the code obviously does, and do not write long blocks. `// SPEC-GAP:` stays a one-line marker.

## Source of truth

- Numbered specs `specs/00–16_*.md` plus `specs/structs.md` are the only source of truth for behaviour.
- `specs/answers_*.md` are a changelog with evidence; every fact in them must also be in a numbered spec.
- An implementer brief cites a spec section for every deliverable; a deliverable without one goes to RE first.
- Session traces (golden, fullrun_noclip, demos) are used to locate divergences, never to derive behaviour.
  If the spec does not explain a divergence: `// SPEC-GAP:` plus a question in `docs/briefs/IMPLn_QUESTIONS.md`.
