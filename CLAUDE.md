# th08 project rules (for people and AI agents working on this repo)

## Comments in `src/` and `crates/*/src/`

A reader must be able to check spec-bound code against the specs, without the comments drowning the code.
- Each module: exactly one `//!` line naming what it ports and its spec section (`//! ECL enemy VM (07 §5–§10)`).
- On an item or statement, cite a spec section only when it adds something: a different section than the module's,
  a kept quirk, units / fixed-point / angle format, or the original's name or address. Plain fields, constants and
  obvious statements get no citation.
- Quirks of the original kept for bit-exactness (call order, off-by-one, odd clamps, original bugs): what and why,
  1–3 lines.
- Port-only code (launcher, settings, platform, render backend, tools, xtask): no spec citations; one `//!` line
  saying what the module does.
- No process history in code: no IMPL / RE-Q numbers, rewrite steps, run ids, `answers_*`, ARCH_* / plan-doc
  pointers, "was / moved from / until X lands". Git history holds that.
- Do not restate what the code obviously does, no long blocks; CLI usage text belongs in `--help`, not in a comment
  banner. `// SPEC-GAP:` stays a one-line marker.

## Source of truth

- Numbered specs `specs/00–16_*.md` plus `specs/structs.md` are the only source of truth for behaviour.
- `specs/changelog/answers_*.md` are a changelog with evidence; every fact in them must also be in a numbered spec.
- An implementer brief cites a spec section for every deliverable; a deliverable without one goes to RE first.
- Session traces (golden, fullrun_noclip, demos) are used to locate divergences, never to derive behaviour.
  If the spec does not explain a divergence: `// SPEC-GAP:` plus a question for the spec, never a fix fitted to the
  trace.

## Headless runs

Automated runs of the windowed binaries (`th08`, launcher, `launchershot`) go through `tools/offscreen <cmd>`
(`env -u WAYLAND_DISPLAY xvfb-run -a`): winit prefers Wayland, so plain `xvfb-run` still opens a real window there.
Android checks run on an emulator (`adb -s emulator-NNNN`).
Wine (Windows builds, the original exe) runs only through `tools/wine_headless <cmd>` with a throwaway
`WINEPREFIX`: plain `wine` opens windows (prefix setup, Mono / Gecko prompts, the program) on the user's desktop.

## Worktrees and heavy runs (the maintainer's machine)

This section is the setup of the maintainer's machine, which its AI agents follow. An outside contributor runs the
same checks directly, without the queue: [CONTRIBUTING.md](CONTRIBUTING.md#checks); tools: [tools/README.md](tools/README.md).

- `target/` holds only output that a rebuild recreates; `cargo clean` must never lose anything. Local state that tools
  keep (arch_check reference and config, agent task log) lives in the gitignored `devdata/`; the Android debug key in
  `~/.local/share/th08-dev/`.

- Every git worktree builds in its own `target/`; never link or share target dirs between worktrees. Plain cargo
  build / check / clippy / fmt / unit tests need no lock.
- Heavy runs (arch_check, cargo tests with game data, the emulator, replay batches) go through the job queue:
  `tools/q submit --kind check|test|emu|full|batch --label NAME -C DIR -- CMD…`, then `tools/q wait ID` in the
  foreground until it ends (exit 75 = not done yet). From a worktree, use the main checkout's `tools/q` so all jobs
  share one queue. Never take `target/.agents.lock` by hand.
- `tools/check_level` says which check a change needs: `light` (build, clippy, unit tests; no arch_check),
  `full` (arch_check: a game crate, th08-render / -tools, `specs/data/` or the settings changed) or `batch` (plus the
  replay batch: core logic, dependencies, build flags or toolchain); a `pixels:` line for th08-render changes names
  the hirescmp check (arch_check does not compare pixels). Recipes: CONTRIBUTING "Changing game logic / the renderer".

- Agent branches land without the maintainer re-running checks: the agent commits on its `t3/…` branch, runs
  `git rebase master`, then `tools/check NAME` in its worktree (the check_level check as one q job, exit 75 = still
  running → `tools/check --wait ID`; on PASS a stamp `devdata/checks/<tree>.pass` in the main checkout). The
  maintainer runs `tools/land BRANCH` in the main checkout: fast-forward when the stamp covers the branch's tree,
  else rebase + `tools/check` + fast-forward; it removes the worktree and branch and never pushes. A batch-level
  branch is appended to `devdata/batch_due`. `tools/merge_wt` stays for agents without git.
