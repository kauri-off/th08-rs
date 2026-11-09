# Tools

Every script here prints its usage from its first comment lines (`sed -n 2,6p tools/NAME`, or run it without
arguments); every `th08-tools` bin has a one-line `//!` header and `--help` / a usage error. Which check a change
needs: `tools/check_level`, explained in [CONTRIBUTING.md](../CONTRIBUTING.md#checks).

**Who:** *all* = any contributor; *data* = needs your own game files (`TH08_DIR`); *maintainer* = the maintainer's
machine only (systemd user session, local replay sets in `packaging/local/local.env`, worktree workflow).

## Scripts (`tools/`)

| Tool | Who | What |
|---|---|---|
| `check_level [BASE]` | all | which check a change needs: `light`, `full` or `batch`, plus a `pixels:` line for th08-render changes and a `batch-due:` line for dependency changes outside the core tree |
| `spec_refs.py` | all (CI) | every `NN §x.y` citation in the code points at an existing spec heading |
| `core_deps` | all (CI) | th08-core / th08-out have no window, GPU, audio or CLI crates among their dependencies |
| `emit_rule` | all (CI) | th08-core logic outside `src/emit/` never names the emit side |
| `doc_pointers` | all (CI) | no pointers to plan documents (`ARCH_*`, `docs/arch/…`) in code and build files |
| `android_check` | all (CI) | `cargo check` of the workspace libs for aarch64-linux-android (skips without the target / NDK) |
| `fmath_xplat/run.sh` | all (CI) | `fmath` bit for bit on wasm32 (node) and aarch64 (qemu) against x86_64 |
| `lint_subset` | all | per-file counts of the `[workspace.lints.clippy]` subset |
| `offscreen CMD…` | all | runs a windowed binary under xvfb with Wayland hidden, with a throwaway config dir |
| `wine_headless CMD…` | all | wine with no window on your display; use a throwaway `WINEPREFIX` |
| `i18n_font_subset` | all | rebuilds the web launcher's CJK font subset after editing `i18n/ja.toml`; `--merge` is the git merge driver for it |
| `arch_check freeze\|run DIR` | data | port-vs-port regression check: builds, lints, tests, replays every session / demo / pilot replay and compares traces and draw hashes with a reference |
| `xplat_replays [OUT]` | data | plays `xplat_replays.list` with `th08-replay` on x86_64, aarch64, wasm32-wasip1 and windows-gnu and compares the per-row hashes |
| `batch_run SET OUT` | data + replay sets | one replay set (`$TH08_EXT_REPLAYS/SET`) against `batch/SET.identical` |
| `android/cargo_android.sh` | all | `libth08.so` per ABI with the NDK (used by the Gradle build) |
| `android/run_debug.sh` | all | debug APK on an emulator: build, install, run, screenshot |
| `android/ui_matrix.py` | all | Android launcher on an emulator with real gestures across orientation, language, theme, font scale and cut-out; reachability, value-guard and layout checks, contact sheets (`--port`, default `$QEMU_PORT`) |
| `android/build_apk.sh` | all | convenience wrapper; `cargo xtask dist --target android-apk` is the supported way |
| `q` | maintainer | job queue for heavy runs on one machine (systemd user units, weighted slots); stall watchdog (`wait` exit 76, `kill ID`), two emulator slots (`QEMU_PORT`, own AVD per slot), emulator snapshot around emu jobs, scratch quota warning; `q selftest` |
| `private_sessions [NAME]` | maintainer | prints where the maintainer's two human sessions are (`TH08_PRIVATE_SESSIONS`, else `devdata/sessions` of the main checkout); exit 1 when absent |
| `arch_job NAME` | maintainer | `arch_check run devdata/arch_ref` as a `q` job |
| `batch_job NAME` | maintainer | the replay batch (sets `ref500`, `ref_b2`) as a `q` job; `due` / `train [--now]`: the batch train for batch-due commits (bisects on FAIL) |
| `check NAME` | maintainer | runs the `check_level` check of a worktree's committed HEAD as a `q` job; on PASS writes a stamp in `devdata/checks/` |
| `land BRANCH` | maintainer | lands an agent branch on master: fast-forward with a pass stamp, else rebase, `check`, fast-forward; removes the worktree; `--setup` registers the merge drivers of `.gitattributes`, `--selftest` |
| `release_gate` | maintainer | refuses a release while batch-due commits wait or no passing batch covers HEAD |
| `merge_wt WT NAME` | maintainer | applies an agent worktree's changes to this checkout and removes the worktree |

`batch/*.identical` lists the replays of each set that match the original; `xplat_replays.list` the replays the
cross-target check plays.

## Bins (`crates/th08-tools/src/bin/`, `cargo run --release -p th08-tools --bin NAME -- …`)

| Bin | Who | What |
|---|---|---|
| `session` | data | replays a recorded session of the original headless and compares the trace (and the draw log, if `sessions/<name>/` has one) |
| `demosync` | data | attract demo 0–3 against `specs/data/demo<N>_trace.txt` |
| `drawrun` | data | headless run that writes a draw log or draw hashes of a menu script or demo |
| `drawdiff` | all | per-frame diff of two draw logs, HTML with renders of both |
| `hirescmp` | data + GPU | renders a session or demo at scale 1 and k, or `--save` / `--ref` frames of two builds, and compares the pixels |
| `replayvideo` | data + GPU | a replay or demo to video (ffmpeg) or PNG + WAV |
| `launchershot` | all | the launcher screen to a PNG, off-screen |
| `portmenushot` | data + GPU | the in-game port menu over a game frame to a PNG |
| `uishots` | all (+ GPU for PNGs) | every launcher view and port-menu page × size × OS scale × language over a fixture save / game folder; the layout check (clipped / overlapping text, mid-word breaks, glued words, stock egui visuals, old colours) exits 1 on any issue; with `OUT_DIR` also the PNGs, `index.html` and `sheet_*.png` contact sheets (`tools/offscreen`) |
| `uitextsheet` | GPU | blind comparison sheets of the same UI strings from the Scene renderer, Slint and egui (`OUT_DIR`: `sheet_*.png`, `zoom_*.png`, `KEY.txt`) |
| `anmview` | data | ANM textures to PNG with sprite rects; `--dis` prints the scripts |
| `eclview` | data | disassembles an `.ecl` file |
| `stdview` | data | prints an `.std` file |
| `datdump` | data | extracts every `th08.dat` entry |
| `repack` | data | builds `th08.pak` / `thbgm.pak`; `--table` writes core's vanilla sha256 table |
| `thcrap-import` | data | a Touhou Patch Center language → a local presentation pack |
| `pack-check` | all | missing glyphs, overflow and unknown keys of a presentation pack |
| `settings_schema` | all | the settings table as JSON for the Android launcher (run by Gradle) |
| `rsqrt_table` | maintainer | regenerates or verifies `rsqrt_amd.bin` (needs the reference CPU) |
