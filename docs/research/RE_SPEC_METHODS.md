# How other projects document reverse-engineering results — comparison with th_08

Research date: 2026-10-31. Web only; sources are linked per section. Where a page could not be fetched
(Cloudflare / 403), that is said and the description relies on long-standing public knowledge.

Our method, for reference: a dirty-room RE agent reads th08.exe 1.00d (Ghidra dump, wine runs) and writes
numbered **behaviour** specs (`specs/00–16`, `specs/structs.md`, original addresses as evidence, VERIFIED /
INFERRED / QUIRK markers) plus per-cycle changelogs (`specs/changelog/answers_*.md`, evidence only). An
implementer that never sees the binary writes Rust from the specs, leaves `// SPEC-GAP:` markers and question
files. Parity: deterministic trace + draw-log comparison of original (wine) vs port on ~1000 third-party
replays; logic bit-exact, drawing visually identical (`docs/PARITY_POLICY.md`).

## 1. Comparison table

| Project | Kind | What is documented | Format | How correctness is verified | Clean-room / legal separation | Knowledge ↔ code sync |
|---|---|---|---|---|---|---|
| **prosper** (PS5→PC, AI-agent driven) | HLE compat layer | Architecture rules, per-title status, falsified hypotheses ("Ruled out"), "instrument traps", verification strategy; *not* Sony internals | Markdown in repo with YAML front-matter (`kind`, `status`, `owner`, `last-verified: date sha`), rule IDs (`LAY-1`), ADRs, `AGENTS.md` per folder, GitHub issues as trackers | Exact-value unit tests vs independent oracles (`llvm-mc`, `spirv-val`), llvmpipe pixel/CRC goldens, execution-differential shader tests, 6-rung per-title ladder ending in a reviewed snapshot guard | Reimplements *published* NID/symbol interfaces "the way Wine does"; no keys/decryption; refuses third-party bypass modules. No separate dirty team | CI checker keeps rule IDs/enforcers live; generated table blocks; ratchets; "a correction only in an issue is not a correction" |
| shadPS4 | PS4 HLE emulator | Build guides, debugging notes, shader patching, changelog; per-game status lives in a separate compatibility-list repo (GitHub issues) | Markdown in `documents/`, `CONTRIBUTING.md` style guide, issues | Games boot/play; community reports in the compat tracker; CI builds | Disclaimer only; contributor rule that AI use must be disclosed and comments/descriptions written by humans | None formal; docs are user/developer how-tos, not behaviour records |
| sm64 / zeldaret oot / papermario | Matching decompilation | The code itself: named functions/structs, Doxygen comments, `//! @bug` markers for original bugs | C source + headers; per-version ROM SHA1/MD5 tables in README; Doxygen style guide | Rebuilt ROM must hash-match the original (`md5sum -c`, `OK`); per-function asm diff (asm-differ / objdiff); progress % per version published as a badge | None (they reproduce the original's code; ROM not distributed, user supplies it) | The docs *are* the code; a non-matching change fails the checksum, so docs cannot drift from behaviour |
| decomp.me / asm-differ / objdiff / splat | Decomp tooling | Per-function "scratches", splitting configs (`splat.yaml`), progress reports | Web scratches, YAML configs, JSON progress reports (objdiff powers decomp.dev) | Instruction-level diff score per function; % matched code/data per unit | — | Progress is computed from the build, never written by hand |
| OpenRCT2 | Reimplementation (from a decompiled base) | Little behaviour documentation; knowledge lives in code, issues and the wiki | C++ source, GitHub wiki, issues | Recorded game replays re-run as tests (from public knowledge; the test-data path was not found at fetch time) | None claimed; started as a decompilation that was progressively rewritten | Replay tests flag simulation drift; otherwise manual |
| OpenMW | Clean reimplementation | Game mechanics as formulas ("Research" wiki), one page per area, each mechanic with *Implementation status* and *Analysis status* (Verified / …) | MediaWiki pages; pseudocode with explicit conventions (rounding, `roll(n)`, float vs int) | In-game black-box tests against the original; the wiki asks for "primary research, or sourced research you independently verified" | Black-box observation and community research; no code from the original | Per-mechanic "Implementation status" field; manual |
| Wine | Clean-room API reimplementation | Windows API behaviour, recorded as **conformance tests** rather than prose | C test programs in the tree (`dlls/*/tests`), run on real Windows and on Wine (TestBot) | The same test must pass on Windows and Wine | Strictest: no disassembly, no Microsoft source, no debug symbols, no `+relay` on native DLLs, no ReactOS code, no LLM-generated code | Tests live next to the code and run in CI; a behaviour claim without a test is not accepted |
| ReactOS | Clean reimplementation (Windows NT) | Wiki + code; historically audited its tree for improperly RE'd code | MediaWiki (the clean-room page was empty when fetched) | Wine conformance tests reused, own rostests | Documented two-team policy (one team writes a spec from RE, another implements) — from public knowledge, page not available | Manual |
| psdevwiki / GBATEK / Pan Docs | Hardware/format reference | Registers, file formats, memory maps, per-revision differences | Wiki (psdevwiki), single HTML/text file (GBATEK), mdBook in git with PR review (Pan Docs) | Test ROMs and community hardware tests; cited per fact in Pan Docs PRs | None (documentation only; implementers read it) | None: emulators cite it; Pan Docs uses issue triage + PRs |
| ExpHP thpages / truth, thtk, thpatch | Touhou format & VM docs, script tools | Per-instruction reference for ANM/STD/MSG/ECL across all games, per-game differences, field offsets and function names | Static site generated from data tables (instruction signature + prose per game), map files (`.anmm`, eclmap), tool READMEs | Round-trip: decompile → compile must reproduce the original file; prose marks "UNTESTED", "only looked at assembly", "unknown" | None (modding docs, written from disassembly) | Instruction tables are data shared by site and tools (mapfiles); thtk keeps a per-game × per-action support matrix |
| **th_08 (us)** | Clean-room port (two roles) | Behaviour specs 00–16 + structs, evidence changelogs, coverage map | Markdown, VERIFIED / INFERRED / QUIRK markers, original VAs as evidence | Deterministic logic traces + draw logs, original under wine vs port, ~1000 third-party replays + golden sessions | Dirty RE agent / clean implementer agent; implementer never sees binary or decompiler output | `// SPEC-GAP:` + question files, briefs cite spec sections, `COVERAGE.md`; all manual |

## 2. Per project

### prosper (PS5 → PC compatibility layer, agent-driven)
Source: <https://github.com/mattias800/prosper> — `AGENTS.md`, `CLAUDE.md`, `prosper/docs/AGENTS.md`,
`prosper/docs/spec/AGENTS.md`, `prosper/docs/adr/AGENTS.md` + `0000-template.md`, `prosper/docs/process/VERIFICATION.md`,
`prosper/tools/docs/*.py`. (Only the documentation process was read; nothing about console internals.)

The closest analogue to us: most work is done by AI agents, and the repository is organised so that agents
cannot quietly drift.

- **Document kinds with front-matter.** Every doc opens with `kind:` (`design`, `reference`, `investigation`,
  `status`, `process`, `plan`, `archive`) and `status:` (`current` / `historical` / `superseded` +
  `superseded-by:`); `check_doc_meta.py` enforces it. The kind says how to read a doc: a `reference` is stable and
  verified against captures, an `investigation` is "true when measured; re-measure before quoting", an `archive`
  is "never start work from one".
- **Rule IDs.** Binding rules live in `docs/spec/`, one `### PREFIX-N -- title` per rule, IDs never reused, each with
  `Status:` and an `Enforcement:` line built from tokens (`ratchet:…`, `ci:…`, `ctest:…`, `runtime:…`, `adr:NNNN`,
  `review: (why no tool can check this)`). Spec files carry `last-verified: <date> <short sha>`. No measurements in
  rule text — a number that changes lives in a baseline or status doc and is cited.
- **ADRs.** One file per decision (Context with measurements + commit, Decision, Consequences incl. what enforces it,
  Alternatives considered with the evidence that rejected them, Approval). Only the owner moves an ADR to
  `accepted`; accepted ADRs are frozen by CI, changes are new superseding ADRs; rejected ones are kept "so the
  question is not re-opened at full cost".
- **Falsified hypotheses.** Every per-title status doc has a `## Ruled out` section: one line per dead hypothesis,
  the evidence that killed it, and the issue link — "the most expensive knowledge in the repository". A numbered
  "instrument trap" table records ways a measurement misled someone; `check_trap_citations.py` verifies that every
  `trap NNN` cited in code, tests or docs resolves to an existing row, and `check_numbered_table.py --baseline`
  forbids deleting rows.
- **Unverified-claims ledger.** Code carries `CONFIDENCE: HIGH|MED|LOW` markers; `confidence_ledger.py` lists them
  (a report, deliberately not a gate, "failing a build for one would teach people to stop writing them").
- **Verification ladder.** Cheapest first: exact-value unit tests against independent oracles (an external
  assembler/validator rather than the project's own assumptions), deterministic software-rasterizer pixel checks and
  a CRC golden, execution-differential tests of the shader recompiler, a coverage metric over real inputs that
  turns "what next" into data. Per title a rung scale 1–6 (any graphics → title → gameplay → human check → oracle
  comparison → automatic snapshot guard); progress claims must name the rung. The gates are also checked for
  their own failure modes ("a missing validator is a failure, not a pass").
- **Docs ↔ code sync rules.** A session-start script compares the checkout with `origin/main`; nested `AGENTS.md`
  per folder; "a correction that lives only in an issue thread is not a correction" — if a measurement changes,
  updating the charter is part of the fix.
- **Cost:** the root `CLAUDE.md` is ~115 KB with long narrative corrections inline; the rules are good, the single
  charter is too big to be read in full every session.
- Clean-room stance: reimplements published interfaces "the way Wine does"; no separate dirty/clean roles.

### shadPS4 (PS4 emulator)
Source: <https://github.com/shadps4-emu/shadPS4> (`documents/`, `CONTRIBUTING.md`). Docs are build guides, a
debugging guide, a shader-patching note and a changelog; game status is tracked as GitHub issues in a separate
compatibility repo. No behaviour spec, no recorded evidence format. Notable rule: AI use must be disclosed and
"descriptions and comments should never be written by AI". Other PS5 emulators (KytyPS5, SharpEmu, AnyPS5) document
even less (README, compat lists, Discord) and are not useful as models.

### Matching decompilations (sm64, zeldaret/oot, papermario)
Sources: <https://github.com/n64decomp/sm64>, <https://github.com/zeldaret/oot> (`README.md`,
`docs/Documenting.md`), <https://github.com/pmret/papermario>.
- **Verification is binary and total**: the build must reproduce the original ROM byte-for-byte (README lists
  SHA1/MD5 per version; `make` ends with `md5sum -c` → `OK`). Functions not yet matched stay as asm
  (`non_matchings/`) or under `NON_MATCHING` with an equivalent C version.
- **Knowledge lives in the code**: names, Doxygen comments (OoT restricts the allowed commands so raw text stays
  readable), original bugs marked with `//! @bug` on the line where they begin — the same idea as our QUIRK
  comments.
- **Progress metric**: per version, % of code/data matched, published as a badge and a site
  (zelda.deco.mp, papermar.io), generated from the build.

### decomp.me, asm-differ, objdiff, splat
Sources: <https://github.com/decompme/decomp.me>, <https://github.com/simonlindholm/asm-differ>,
<https://github.com/encounter/objdiff>, <https://github.com/ethteck/splat> (decomp.me site itself is behind
Cloudflare). splat splits a binary into segments from a YAML config; asm-differ/objdiff diff one function at a time
with a score and auto-rebuild; objdiff emits progress reports that power decomp.dev. The method: a per-unit
**distance** that can only go down, and progress computed by a tool, never typed in.

### OpenRCT2
Source: <https://github.com/OpenRCT2/OpenRCT2> (`readme.md`, `CONTRIBUTING.md`). Reimplementation of RCT2 that grew
out of a decompilation and was rewritten function by function. Behaviour documentation is thin; regressions are
reportedly caught by recorded replays re-run as tests (public knowledge; not confirmed at fetch time) — the same idea as our replay suite at a smaller scale. Requires the
user's original data files, as we do.

### OpenMW
Source: <https://wiki.openmw.org/index.php?title=Research>, example
<https://wiki.openmw.org/index.php?title=Research:Combat>. Mechanics are documented as pseudocode with a stated
convention block (all numbers float unless said, `int(x)` truncates, `round` = FPU mode, `roll(n)` range). Each
mechanic has **Actions affected**, **Description**, **Implementation status** (Implemented / …) and **Analysis status**
(Verified / "Verified, but there may be further interactions"). Contribution rule: "only primary research, or
sourced research that you have independently verified". This is the nearest public equivalent of our numbered
specs, but kept as a wiki without links to code.

### Wine, ReactOS
Sources: <https://gitlab.winehq.org/wine/wine/-/wikis/Clean-Room-Guidelines>,
<https://gitlab.winehq.org/wine/wine/-/wikis/Developer-FAQ>; <https://reactos.org/wiki/Clean_Room_Reverse_Engineering>
(empty page at fetch time).
- Wine records behaviour as **executable conformance tests** that must pass on real Windows and on Wine; a test
  program is preferred even over reading documentation. The don't-list is explicit: no disassembly, no Microsoft
  source, no debug symbols, no relay tracing of native DLLs, no ReactOS code, no LLM-generated code, and no test that
  "prints out the values of an internal table". People who have seen Windows source cannot contribute.
- ReactOS is known for the classic two-team method (one side writes a description from RE, the other implements
  from it) and for having audited its tree; its wiki page on the subject was empty, so no detail is claimed here.
- Relevance: our split is the ReactOS-style two-role model, not Wine's single black-box role; Wine's "a test that
  must pass on the original" is the analogue of our wine-vs-port traces.

### Hardware/format wikis (psdevwiki, GBATEK, Pan Docs)
Sources: <https://www.psdevwiki.com/> and <https://problemkaputt.de/gbatek.htm> (both blocked: Cloudflare / 403 at
fetch time; practice from public knowledge), <https://github.com/gbdev/pandocs> (`CONTRIBUTING.md`).
psdevwiki is an open MediaWiki of tables per register/file/syscall with per-firmware columns. GBATEK is one
author-maintained text file, dense tables, no citations. Pan Docs moved to mdBook in git: every change is a reviewed
PR with "what, how and **why**" in the message, issues go through a Triage → To Do board, and boxes
(`:::tip/warning/danger`) flag caveats. Lesson: git + PR review gives the history and evidence trail a wiki lacks.

### Touhou RE (ExpHP thpages / truth, thtk, thpatch)
Sources: <https://exphp.github.io/thpages/> (ANM/STD/MSG/END instruction reference), <https://github.com/ExpHP/truth>,
<https://github.com/thpatch/thtk> (`README`); <https://www.thpatch.net/> was behind Cloudflare.
- thpages documents each instruction once with per-game notes ("EoSD: …", "PCB–: …", "StB and earlier / MoF and
  later") and is explicit about evidence quality: "This has not been tested", "These are all untested, I only looked
  at assembly", "UNTESTED:", "some unknown vector". A stats page tracks the documented-instruction count toward 100 %.
- truth and thtk verify format knowledge by round-trip (decompile → compile → identical file) and share instruction
  signatures as map files; thtk's README keeps a game × action support matrix. Our dumpers' "problems: 0" is the
  same check.
- These are behaviour docs for modders, not a clean-room process; they were written from disassembly. Useful to us
  only as cross-checks, and our specs stay the only source of truth.

## 3. What we could adopt / what we do differently

What we already do as well as or better than these projects: a written two-role split with briefs that cite spec
sections (only ReactOS has something comparable, and it is undocumented online); verification against the
original *running* (wine traces + draw logs on ~1000 replays — stronger than everyone except matching decomps);
explicit VERIFIED / INFERRED / QUIRK evidence levels (OpenMW and thpages do this informally). Our weak spot is the
same one prosper fixed with tooling: all the docs↔code links are kept by hand.

Ranked suggestions (cost → value):

1. **CI check: SPEC-GAP ↔ question ↔ answer.** Format `// SPEC-GAP(Q-IMPL144-3): …`; a script fails if a SPEC-GAP has
   no question entry, if a question is marked answered while its SPEC-GAP is still in `src/`, or if a question has
   no SPEC-GAP and no spec section. Same for spec-section citations in `//!` comments and briefs: every `07 §5.2`
   must resolve to a heading that exists (prosper's `check_trap_citations.py`). Cheap, catches the drift we cannot see
   today.
2. **Spec front-matter with last-verified sha.** Per spec: `status`, `last-verified: <date> <sha of symbols.csv /
   re-dump>`, `owner: RE`, and per-section `Verified-against:` where useful. `COVERAGE.md` gets generated from it
   instead of being edited by hand. Tells the implementer which sections predate the latest RE corrections.
3. **Stable claim IDs for spec facts that code depends on.** Give numbered IDs to quirks and non-obvious constants
   (`ECL-Q12`, `BULLET-7`), never reused. `src/` comments cite the ID instead of only "07 §10.3a", so a renumbered
   section does not break references and a grep shows every place a quirk is ported. Start with the 47 QUIRK
   entries.
4. **Falsified-hypothesis log ("Ruled out").** One line per hypothesis that RE or IMPL disproved, with the evidence
   (run dir, VA, replay) — per spec file as a `## Ruled out` section, plus a global file for cross-cutting ones.
   Today they sit in `re/NOTES.md` (1000+ lines) and in old briefs, so new agents re-derive them.
5. **Decomp-style progress metrics, generated.** One script producing a table per cycle: replays logic-identical
   (`ref500 490/490`), golden draw-identical frames, draw-log groups, open SPEC-GAPs, open questions, PARTIAL rows in
   coverage — appended to a CSV, so a regression in any number is visible across cycles (we already quote these in
   commit messages; make it a file, like objdiff/decomp.dev).
6. **Evidence-quality vocabulary per claim**, closer to OpenMW/thpages: keep VERIFIED / INFERRED, add `OBSERVED`
   (seen in a wine run, not read in code) and `UNTESTED` (read in code, no data exercises it). Helps decide when a
   divergence is a spec error versus a port error.
7. **Executable spec checks where cheap (Wine-style).** For formats and pure functions (RNG, angle math, ECL
   decoders), record small input → output vectors produced from the original (wine probe) in `specs/data/`, and run
   them as port unit tests. The implementer still never sees code; the vectors are behaviour, like Wine's
   conformance tests. Must be produced by RE from runs, not by trace-fitting in `src/`.
8. **Decision records for policy** (ADR-lite): `docs/decisions/NNNN-*.md` for things like the parity policy draw
   exceptions, frozen once accepted, superseded rather than edited. Only the user accepts; agents propose.
9. **Session-start check for agents** (prosper `session_start.py`): verifies the dump is newer than `symbols.csv`,
   the agent's checkout is at master, and lists the relevant nested instructions. Mostly folds existing RE_WORKFLOW
   rules into one command.

What we deliberately do differently: no LLM ban or human-only comments (Wine, shadPS4) — our separation is between
roles, not between humans and agents; no matching-decomp goal — Rust cannot be byte-matched, so our oracle is
observable behaviour (traces/draw logs); and no single huge charter (prosper's 115 KB `CLAUDE.md`) — keep the
numbered specs as the only truth and keep instruction files short.
