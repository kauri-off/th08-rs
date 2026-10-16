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
| 1 | opus 5.5 medium | phase1-opus | done: all 5 specs; 317/317 files byte-identical; BGM +0x18, ANM 0x40 hdr, THTX 1/3/5 correct |
| 2 | opus 5.5 medium | phase2-opus | done: 05 ANM VM, 06 STD, 07 ECL (all ops 0..184), tools 0 problems |
| 3 | opus 5.5 medium | phase3-opus | running (08 bullets/items, 09 player, answers to QUESTIONS.md) |
| impl 1 | opus 5.5 medium | impl1-engine-skeleton | done: src/task.rs, game.rs, supervisor.rs, anmmgr.rs, gfx viewport+camera; 4 tests |
| impl 2 | opus 5.5 medium | impl2-anm-vm | done: src/anmvm.rs (all 57 opcodes in data), blend modes, title menu via title01.anm; 9 tests |
| impl 3 | opus 5.5 medium | impl3-std | done: src/stdfile.rs, stage.rs, depth/fog/clip-space gfx; all 9 stage bgs render; 15 tests |
| impl 4 | opus 5.5 medium | impl4-web-parity | done: web pixel-identical to native (WebGL2+WebGPU), ?stage= params via src/env.rs; seams are in texture data |

User (2026-10-17): do not launch subagents for now. Sub-agents disabled in TASK_TEMPLATE.md.

## Observations
- Phase 1 first pass: DeepSeek solved archive + BGM table quickly; GLM got stuck on header field mapping (Hopper).
- opencode may ask for web-search permission (pending request) — answer "allow"; check t3_pending_request_list when a thread looks stalled.

## Haiku 5.5 test (2026-10-17)
- Task `phase1-test-haiku` (cancelled: REA missing) → `phase1-test-haiku-rea`: provider claudeAgent, model claude-haiku-5-5, effort medium, SPEC_DIR specs/haiku (fresh Ghidra project), PHASE1_BRIEF from scratch.
- Grading against known truth: extractor must reproduce 317 files byte-identical to src/archive.rs output (`cargo run --release --bin datdump`);
  BGM loop_start = +0x18 relative bytes; ANM header 0x40, script entry {i32 id, u32 off}; THTX fmt 1/3/5 = BGRA8888/RGB565/ARGB4444.
- 2026-10-17: Haiku web-searched thtk / "TH08 decompilation sources" and tried recalling formats → user called it cheating.
  Rule now in TASK_TEMPLATE.md: no web, no third-party recalled knowledge; binary + data only. (Earlier GLM/DeepSeek runs had web allowed.)

## 2026-10-17: roster change
- Haiku 5.5 test failed; DeepSeek dropped (too expensive). Active dirty-room models: **Opus 5.5** (claudeAgent, claude-opus-5-5) and GLM (rate limited for now).
- Run `phase1-opus`: claude-opus-5-5, effort medium, SPEC_DIR specs/opus (fresh Ghidra project), PHASE1_BRIEF from scratch, no sub-agents, no web.
- 2026-10-17: GLM must NOT be launched without explicit user permission. Default dirty-room agent = Opus 5.5 only.
- 2026-10-17: keep agent tasks small (≤3 related specs per run); big briefs hurt quality. 08–11 are queued as separate runs.
- 2026-10-17: one agent per role: 1 Opus RE + 1 Opus implementer in parallel; coordinator orchestrates only.
