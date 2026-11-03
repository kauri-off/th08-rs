# th_08 project rules

These override the global "no comments / one short line" rule for this repo.

## Comments in `src/` and `crates/*/src/`

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

## Off-screen runs

Run the port's windowed binaries (`th08`, launcher, `launchershot`) only via `tools/offscreen <cmd>` — it is
`env -u WAYLAND_DISPLAY xvfb-run -a`. Plain `xvfb-run` is NOT enough on this Wayland desktop: winit prefers Wayland
and opens a real window on the user's screen. wine runs the same way (`WAYLAND_DISPLAY` unset).

## Android devices

Agents use only the headless emulator: every `adb` command takes an explicit emulator serial (`adb -s emulator-NNNN`).
Never install, launch, screencap, read logcat or run any other `adb` command on a physical phone.
