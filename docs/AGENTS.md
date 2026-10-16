# Dirty-room agents

- Briefs: specs/TASK_TEMPLATE.md (common preamble, sub-agent rules), specs/RE_WORKFLOW.md (Ghidra + memory files),
  specs/PHASE1_BRIEF.md, specs/PHASE1_RESUME.md.
- Spec dirs: specs/glm, specs/deepseek (each has own ghidra/ project copy, NOTES.md, symbols.csv).
- Always send the SAME brief to both; only SPEC_DIR differs. Target options: `{"variant":"max"}`, provider `opencode`,
  models `opencode/glm-5.3-flash-free`, `opencode/deepseek-v4.1-flash-free`.
- Sub-agents may spawn 1 level of same-model sub-agents (workdir SPEC_DIR/sub/<topic>, own Ghidra copy).
- Old top-level threads (stopped, kept for history): GLM 6f189115-ca00-45f4-ba49-f7dec69ddabb, DeepSeek 7676c80f-1340-4cdf-909b-cc1ff1ebc0c5.

## Task log
| phase | agent | clientRequestId | state |
|---|---|---|---|
| 1 resume | glm | phase1-resume-glm | failed: rate limit |
| 1 resume | deepseek | phase1-resume-deepseek | failed: rate limit (3 sub-agents) |
| 1 resume2 | deepseek | phase1-resume2-deepseek | failed: rate limit |

User (2026-10-17): do not launch subagents for now. Sub-agents disabled in TASK_TEMPLATE.md.

## Observations
- Phase 1 first pass: DeepSeek solved archive + BGM table quickly; GLM got stuck on header field mapping (Hopper).
- opencode may ask for web-search permission (pending request) — answer "allow"; check t3_pending_request_list when a thread looks stalled.
