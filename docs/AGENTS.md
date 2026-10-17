# Agents

- Roster: exactly 1 RE agent + 1 implementer, both Opus 5.5 effort medium (`delegate_task`, provider `claudeAgent`, model `claude-opus-5-5`). No sub-agents. Other models only with explicit user permission.
- Tasks are small: ≤3 deliverables per run.
- RE task prompt: follow `docs/briefs/TASK_TEMPLATE.md` + the brief. Works in `re/`, writes specs to `specs/`.
- Implementer prompt: follow `docs/briefs/IMPL_TEMPLATE.md` + the brief. Reads `specs/` only, writes `src/`, never touches the exe, `re/` or git.
- Questions flow: implementer → `docs/briefs/IMPLn_QUESTIONS.md` → RE answers in `specs/answers_*.md` and fixes specs in place.
- Coordinator (me): writes briefs, reviews, verifies (build, tests, wasm check, screenshots), commits. Finished briefs → `docs/archive/briefs/`.
- History of earlier runs and models: `docs/archive/AGENTS_history.md`.
