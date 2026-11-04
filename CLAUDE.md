# th_08 project rules

These override the global "no comments / one short line" rule for this repo.

## Comments in `src/` and `crates/*/src/`

A reader must be able to check spec-bound code against the specs, without the comments drowning the code.
- Each module: exactly one `//!` line naming what it ports and its spec section (`//! ECL enemy VM (07 §5–§10)`).
  The spec section is cited there once.
- On an item or statement, cite a spec section only when it adds something: a different section than the module's,
  a kept quirk, units / fixed-point / angle format, or the original's name or address. Plain fields, constants and
  obvious statements get no citation.
- Quirks of the original kept for bit-exactness (call order, off-by-one, odd clamps, original bugs): what and why,
  1–3 lines.
- Port-only code (launcher, settings, platform, render backend, tools, xtask): no spec citations; one `//!` line
  saying what the module does.
- No process history in code: no IMPL / RE-Q numbers, rewrite steps, run ids, `answers_*`, ARCH_* / plan-doc
  pointers, "was / moved from / until X lands". Git and docs/ hold that.
- Do not restate what the code obviously does, no long blocks; CLI usage text belongs in `--help`, not in a comment
  banner. `// SPEC-GAP:` stays a one-line marker.

## Source of truth

- Numbered specs `specs/00–16_*.md` plus `specs/structs.md` are the only source of truth for behaviour.
- `specs/answers_*.md` are a changelog with evidence; every fact in them must also be in a numbered spec.
- An implementer brief cites a spec section for every deliverable; a deliverable without one goes to RE first.
- Session traces (golden, fullrun_noclip, demos) are used to locate divergences, never to derive behaviour.
  If the spec does not explain a divergence: `// SPEC-GAP:` plus a question in `docs/briefs/IMPLn_QUESTIONS.md`.

## Off-screen runs

Run the port's windowed binaries (`th08`, launcher, `launchershot`) only via `tools/offscreen <cmd>` — it is
`env -u WAYLAND_DISPLAY xvfb-run -a`. Plain `xvfb-run` is NOT enough on this Wayland desktop: winit prefers Wayland
and opens a real window on the user's screen. wine runs the same way (`WAYLAND_DISPLAY` unset).

## Android devices

Agents use only the headless emulator: every `adb` command takes an explicit emulator serial (`adb -s emulator-NNNN`).
Never install, launch, screencap, read logcat or run any other `adb` command on a physical phone.

## Worktrees and heavy runs

- Every git worktree builds in its own `target/`; never link or share target dirs between worktrees. Plain cargo
  build / check / clippy / fmt / unit tests need no lock.
- Heavy runs (arch_check, cargo tests with game data, the emulator, replay batches) go through the job queue:
  `tools/q submit --kind check|test|emu|full|batch --label NAME -C DIR -- CMD…`, then `tools/q wait ID` in the
  foreground until it ends (exit 75 = not done yet; never run_in_background). In a worktree, call the main checkout's
  `/home/anon/Projects/th_08/tools/q`. Never `flock target/.agents.lock` by hand.

