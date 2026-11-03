# Step 10 — crate cut (plan)

Design: ARCH_A_IDIOMATIC.md §4 step 10 / §5, ARCH_SUMMARY decisions 1–10, REWRITE_PLAN step 10 + Mobile block,
ARCH_D_MOBILE §6.1 item 6 (cdylib, three entries, desktop-only deps), STEP9_WORLD.md (`Ctx { w: World, … }` as built),
target/launcher_review.md Design 2 runs 1–2. Rule: runs 10a/10b make the boundary hold *inside* today's `th08` crate;
the cut (10c) is then a pure move (`git mv`, `crate::` → `th08_core::` paths, Cargo.toml split). Behaviour, call order,
RNG order and every recorded command are frozen; every run passes `tools/arch_check run target/arch_ref` (T + D, wasm,
android), 10c also B.

## 1. Target crates

| crate | contents (from src/) | allowed deps | why |
|---|---|---|---|
| `th08-formats` (exists) | pure parsers / writers | encoding_rs, midly, thiserror | unchanged |
| `th08-core` | `World`, `Ctx` (frame driver: calc / draw / present / restart / saves), all subsystems and VMs, task chain, `DrawCmd`/`ResCmd`/`Recorder`, `SoundCmd` + BGM state machine, `Host`, `Renderer`, new `AudioOut`, `DataSource` trait, `CoreConfig`, trace sampling, draw-log text format; `#![forbid(unsafe_code)]`, `#![deny(clippy::disallowed_methods)]` | formats, anyhow, bytemuck, encoding_rs, libm, log, image (see risk 7) | the compiler proves ARCH decisions 3 / 6: no wgpu / cpal / winit / egui / rfd / clap / std::fs / clock / web-sys |
| `th08-render` | gfx.rs (`Gfx: Renderer`, headless + `attach(window)`), text.rs (ab_glyph native, canvas on wasm) | core, wgpu, winit (surface only), bytemuck, image, ab_glyph (native), web-sys / wasm-bindgen (wasm) | decision 2: pure executor; shared by app, launcher (egui-wgpu on its device) and tools |
| `th08-audio` | audio.rs (cpal mixer, `Audio: AudioOut`), midi.rs (SMF sequencer + rustysynth synth) | core (sound, config, content), cpal, hound, rustysynth (native) | executes `SoundCmd`; midi.rs is playback only (no logic reads it; `Sequencer` users are audio.rs only) |
| `th08-tools` (lib + bins) | session.rs, demosync.rs (runners / parsers / compare), drawlog.rs (reader, diff, merge, `HashSink`, file sinks), drawreport.rs, xz.rs; bins session, demosync, drawrun, drawdiff, datdump, eclview, stdview, anmview, rsqrt_table, launchershot; tests that need a GPU | core, render, audio, `th08` (settings, `ToolHost`, `DirSource`, launcher for launchershot), liblzma, sha2, image, clap; whole lib `cfg(not(wasm32))` | debug tooling leaves the cdylib (wasm / APK no longer compile session / drawreport); bin names unchanged |
| `th08` (app, root package) | app.rs, launcher.rs, settings.rs, platform.rs, sources.rs (new, from assets.rs), joystick.rs, pacer.rs, main.rs; lib.rs = entries (`start` wasm, `android_main`); `tests/port_cfg.rs` | core, render, audio, winit, egui*, gilrs, web-time, directories, chrono; desktop-only: rfd, clap, env_logger, pollster, windows-sys | stays the root package so `th08.wasm`, `--bin th08`, `default-run`, web/build.sh and both workflows keep working |

Not split further: settings.rs + platform.rs (Storage, Clock, ToolHost) are needed by app and tools; tools depend on
the app lib instead of a 7th `th08-platform` crate (tools link render/winit anyway). Revisit only if tool build time
suffers. AppHost (InputMux with winit `KeyCode`, gilrs Joystick) is app-only.

Dependency graph: formats ← core ← {render, audio} ← th08 (app) ← tools. No cycle; core's dev-deps stay empty (a
dev-dep on render would create a second copy of core's types — see §2 tests).

## 2. Per-file table (master 5d5d7c7)

Crossing = a reference that is illegal on the target side today. "—" = moves as is.

| file(s) | target | crossing today (file:line) | change before the move |
|---|---|---|---|
| anm, archive, ecl, sht, stdfile, scoredat (re-export shims) | core | archive.rs:5–11 `test_archive` uses `settings::test_game_dir`, `assets::find_file`, `std::fs` | 10b: `testdata.rs` (cfg(test), TH08_DIR, like formats/src/testdata.rs) |
| anmmgr, anmvm, msg, stage, enemy | core | test-only `archive::test_archive` (anmmgr:808, anmvm:1299, msg:640, stage:1580, enemy:3382); enemy.rs:3385 `settings::get().test_frames` | 10b: `testdata::archive()`; `test_frames` → `TH08_TEST_FRAMES` read in the test |
| ascii | core | test: `platform::ToolHost::default()` (ascii.rs:1978–2045) | 10b: core `MemHost` (in-memory saves, fixed clock) |
| replay | core | test: `platform::Clock::default().local_time()` (replay.rs:745), `test_archive` (868, 908) | 10b: fixed `LocalTime`; `testdata` |
| result | core (tests split) | test: ToolHost / Storage / `settings::temp_dir` / `std::fs` (1840, 2011–2169); `Gfx::headless` (1998); Gfx downcast for screenshots (2117); manifest-relative `specs/data/drawlog` (1928) | 10b: state tests → `NullRenderer` + `MemHost`; the screenshot test → tools; paths via `repo_root()` |
| menu, game | core | tests load a `Ctx` with `Gfx::headless` + `settings::get().core()/tool_host()` (menu.rs:1552–1561, game.rs:567–576) | 10b: `NullRenderer` + `MemHost` + `CoreConfig::default()` + `testdata` (no GPU needed: logic never queries the renderer, decision 2) |
| game (`Ctx`) | core | `audio: Audio` cpal device (game.rs:24), built inside `load_with` (261–264: `Audio::new`, `load_se`, `load_midi`, `midi_ready = midi_available()`); `drawlog: Option<drawlog::Writer>` (34) opened by `Writer::from_spec` (304, fs + xz); `session::on_restart` / `on_extra_present` (232, 445); `start_once` wasm cfg (329) | 10a: `audio: Box<dyn AudioOut>` passed in (trait: `load(PresentView)`, `midi_available`, `exec`, `is_started`, `ensure_started`); writer passed in; 10b: hooks → `trace::` |
| gamemgr | core | `sched: Option<session::Sched>` (gamemgr.rs:191) | 10b: `trace::Sched` |
| world, chain, supervisor | core | `anmdebug` refs (world.rs:64, chain.rs:36, supervisor.rs:582) — fine, anmdebug goes to core (it is a World field + chain node) | — |
| session.rs | split | `Sched`, `Row`, `sample` (202), `on_restart` (24), `on_extra_present` (36) read only `ctx.w` | 10b: those → `trace.rs` (core); runner, parsers, compare, `settings::get()` (487, 693…) stay → tools |
| demosync.rs | split | `sample` (55) reads only `ctx.w`; tests use Gfx / settings (204–333) | 10b: `sample` → `trace.rs`; rest → tools |
| drawlog.rs | split | `Writer` (334) + `format_frame` (151) + `num*` are pure text over `LogRec`; `from_spec` (358) opens fs / xz; `settings::temp_dir` (2109), Gfx tests (2452) | 10a: `drawfmt.rs` (core): `Writer` over `Memory | Box<dyn std::io::Write>`, `format_frame`, `Header`; `open_spec` (fs / xz) stays in drawlog.rs → tools; app + tools pass the writer |
| drawlist | core | `image::load_from_memory` (697) for .jpg size; boundary test reads src/ via `CARGO_MANIFEST_DIR` (786–830) | test becomes the 10b gate (§3), deleted in 10c (the compiler replaces it) |
| assets.rs | split | `DataSource`, `Source`, `MaybeSend`, `BoxFuture` are pure; `DirSource` / `find_file` use std::fs (52–140); `WebFiles` web-sys (89); `spawn` threads / wasm-bindgen-futures (141–150) | 10a: concrete sources + `Source::native/web` + `find_file` → `sources.rs` (app); `spawn` → audio.rs (its only user) |
| fmath | core | `unsafe` `_mm_rsqrt_ss` in tests (fmath.rs:142–146) | 10b: native-comparison tests → `rsqrt_table --verify` (tools); table + model stay |
| rng | core | `#[cfg(not(wasm32))]` backtrace trace (rng.rs:42) | — (std only, debug) |
| camera, menulist | core | `#[allow(clippy::disallowed_methods)]` (camera sin_cos for view matrices) | — (kept, already audited in C0) |
| coreconfig, host, content, config, sound, input, d3dx, task, bomb, bullet, item, player, pshot, eclvm, effect, ending, gui, hud, musicroom, world, anmdebug | core | none outside `#[cfg(test)]` (log::* only) | sound.rs `pub(crate)` Bgm fields used by audio.rs tests (559–716): 10b moves those Bgm tests into sound.rs |
| audio.rs | audio | `settings::get()` soundfont + dirs (266); `assets::{Source, spawn}` (5) | 10a: `AudioConfig { soundfont, soundfont_dirs }` from the app / tools; own `spawn` |
| midi.rs | audio | `archive::` (formats) only | — |
| gfx.rs, text.rs | render | `winit::window` (surface) | — |
| app.rs, launcher.rs, settings.rs, platform.rs, joystick.rs, pacer.rs, main.rs | app | — (own deps) | L1 / L2 (§3); `rfd` (launcher.rs:144–157, 618; app.rs:568, already cfg'd), `clap` (settings.rs:941–1260), `env_logger` (main.rs) desktop-only cfg in 10c |
| lib.rs | app | wasm `start` (73–92) + module list | 10c: entries only (`start`, `android_main` stub calling `app::run` with `PlatformDirs` from `internal_data_path`) |
| drawreport.rs, xz.rs | tools | Gfx, image, liblzma | — |
| worldclone.rs (cfg(test)) | core | `Gfx::headless` + `settings` (653–765) | 10b: `NullRenderer` + `MemHost` (it compares `DrawCmd` / `ResCmd`, not `LogRec`) |
| bin/*.rs | tools | `th08::…` paths | 10c: `th08_core::` / `th08_tools::` / `th08::` |

**`settings`.** No core-bound file reads `settings::get()` outside tests (step 3 done: `Ctx.core: CoreConfig`,
`Settings::core()` settings.rs:1009). The `OnceLock` stays app-side for tools/tests; core never names `settings`.

**`Ctx` vs `World`.** Both in core. `World` = deterministic state (Clone). `Ctx` = `World` + `Recorder` + trait
objects only: `renderer: Box<dyn Renderer>`, `audio: Box<dyn AudioOut>`, `host: Box<dyn Host>`, `drawlog:
Option<drawfmt::Writer>`, `archive: LogicView`, `core: CoreConfig`, `sound_log`, `se_log`, `emit`. Construction:
`Ctx::load_with(source: Source, io: Io { renderer, audio, host, drawlog }, core, persist)`.

**`pub` surface core exports** (everything already `pub mod`; listed = what other crates use): `game::{Ctx, Io, Sys}`,
`world::World`, `host::{Host, RawInput, LocalTime, Words}`, `drawlist::*` (render), `drawfmt::*`, `trace::*`,
`sound::{SoundCmd, MidiCmd, WavCmd, Track, Bgm}` + `config` volume curves (audio), `content::{LogicView, PresentView}`,
`assets::{DataSource, Source}`, `coreconfig`, `input::{PadState, key bits}` (app keymap, joystick), `gamemgr` / `player`
/ `bullet` / `enemy` / `replay` / `menu` fields read by trace and tools, `NullRenderer`, `MemHost`. `pub(crate)` items
read by tools: none outside tests after 10b (checked: sound.rs Bgm fields are the only hit, via audio.rs tests).

## 3. Runs

| run | content (≤ 3 deliverables) | files | parallel | check |
|---|---|---|---|---|
| **10a** IO out of `Ctx` (**done**, below) | (1) `AudioOut` trait + `Io` bundle for `Ctx::load_with`; `Audio` impl; `AudioConfig` instead of `settings::get()` in audio.rs; (2) assets.rs split → `sources.rs` (app-bound), `spawn` into audio.rs; (3) drawlog split → `drawfmt.rs` (core-bound) + `open_spec` (tools-bound); callers pass the writer | game, audio, assets, sources (new), drawlog, drawfmt (new), app, lib, main, session, demosync, drawreport, worldclone, menu/result/game tests (load call sites, 17), bin/* | — (first) | arch_check run (T D W android); `session --sound-log` selftest1 unchanged |
| **10b** core self-contained | (1) `trace.rs` (Sched, Row, sample, demosync sample, restart / extra-present hooks); (2) core test infra: `testdata.rs`, `NullRenderer`, `MemHost`, `CoreConfig::default()` in tests; GPU tests (drawlog, result shots, demosync, session) marked tools-bound; Bgm tests → sound.rs; `repo_root()` for manifest-relative paths; (3) boundary gate: drawlist.rs test generalised to the planned core file set — no `crate::{gfx,text,audio,midi,app,launcher,settings,platform,sources,joystick,pacer,session,demosync,drawlog,drawreport,xz}` and no `wgpu/cpal/winit/egui/rfd/clap/std::fs/web_time/web_sys/pollster` outside `#[cfg(test)]`, no `unsafe` (fmath native tests → `rsqrt_table --verify`) | session, demosync, trace (new), testdata (new), gamemgr, supervisor, game, worldclone, drawlist, fmath, bin/rsqrt_table, audio, sound, tests of ascii/replay/result/menu/enemy/msg/stage/anmmgr/anmvm/archive | **parallel with L1** (worktree; disjoint; lib.rs module list merged by me) | arch_check run; gate test green |
| **L1** (launcher Design 2 run 1) | Shell state machine `enum Shell { Setup, Loading, Running, PortMenu, Error }` replacing `pre/launch/settings/started` (app.rs:63–76); Error + Loading screens with Back; one fullscreen owner; game owns its `HostSettings` by value (OnceLock kept for tools) | app, launcher, main (not lib.rs module list) | parallel with 10b | arch_check run; launchershot images via `tools/offscreen`; manual desktop + wasm start |
| **10c** the cut (pure move) | (1) `git mv` into crates/th08-core/src (incl. rsqrt_amd.bin), crates/th08-render, crates/th08-audio, crates/th08-tools (lib + src/bin); `crate::` → crate paths; `forbid(unsafe_code)`; (2) Cargo.toml per crate, workspace members, deps split, desktop-only `rfd/clap/env_logger/pollster` under `cfg(not(any(wasm32, android)))`, `android_logger` + `android_main` stub in lib.rs; (3) drop the 10b gate test (compiler enforces), update tools/arch_check comment, docs | everything (alone) | — | arch_check run + B (ref500 490/490, ref_b2 498/498) + `cargo tree -p th08-core` shows no wgpu/cpal/winit/egui |
| **L2** (launcher Design 2 run 2) | `Entry` metadata (`label`, `audience: Player | Dev`, `platforms`, `persist`) replacing `Group::developer()` (settings.rs:63) and the quick-win rules of 5d5d7c7; typed `Value`s with dirty state instead of strings + the `Origin::Cli` trick (settings.rs:12–18) | settings, launcher, app (th08 crate only) | parallel with step-11 core runs (other crate) | arch_check run (fast); launchershot |


**10a done (2026-11-03).** Files: game.rs (`AudioOut` trait, `Io { renderer, audio, host, drawlog }`;
`Ctx::load(source, io, core)`, `Ctx::load_with(source, io, core, persist)`; `audio: Box<dyn AudioOut>`, `drawlog:
Option<drawfmt::Writer>`), audio.rs (`AudioConfig { soundfont, soundfont_dirs }`, `Audio::new(source, cfg)`, `impl
AudioOut for Audio` with `load` = `load_se` + `load_midi`, `spawn` moved here; its tests use an in-test `NoFiles`
source instead of a temp dir), assets.rs (pure: `MaybeSend`, `BoxFuture`, `DataSource`, `Source`), sources.rs (new:
`native(dir)`, `web(files)` as free functions instead of `Source::native/web` — inherent impls cannot cross the crate
cut —, `DirSource`, `WebFiles`, `find_file`), drawfmt.rs (new: `num`, `num_g`, `Header`, `format_frame`, `Writer`
over `Memory | Stream(Box<dyn Write>)` with `Writer::stream(w, range)`, `Range`, `parse_range`; the `num_fast` tests
moved with it), drawlog.rs (`open_spec` = the old `Writer::from_spec`; `HashSink` stays), settings.rs (`audio()`,
`tool_io(renderer, &source)` = renderer + real `Audio` probe + `tool_host` + no log, risk 8), coreconfig.rs
(`drawlog` field removed: core no longer reads it), app.rs (builds `Io`; opens `TH08_DRAWLOG` itself), lib.rs, main.rs,
and the 17 load call sites (session, demosync, drawlog, result, menu, game, worldclone tests; bin/session, drawrun,
demosync), archive.rs / launcher.rs / drawreport.rs (`sources::find_file`). Deviations: (a) `Io::drawlog` is used as
passed, also with `persist` false (only the app passes one; before, `from_spec` ran only with `persist`); the app now
opens the log file before the archive is read. (b) **Resolved after 10a:** the app no longer calls
`drawlog::open_spec`: `app::open_drawlog` writes plain-text logs (spec parsed by core `drawfmt::parse_spec`); `.xz`
logs and liblzma stay tools-only. Check: `FAST=1 arch_check run` 0 differing
(emit-off 0; fmt, build, wasm, android, cargo test inside), spec_refs 0, `sound_log_selftest1` / drawlog format and
menu tests green with TH08_DIR.

Order: 10a → (10b ∥ L1) → 10c → L2 (∥ step 11). 4 step-10 runs + 2 launcher runs. Design 2 runs 3–4 fold into
Android runs 1 and 4–5 (ARCH_D §6.3) as planned.

## 4. Risks

1. **Build time.** Core changes rebuild render/audio/app/tools; app-only changes no longer rebuild core (today one
   56 k-line crate) and crates compile in parallel. `[profile.dev.package."*"]` matches non-member packages only, so
   the new crates keep dev opt-level 0 (unchanged for our code). Tool bins pull the app lib (egui etc.), as today.
   Record `cargo build --release --workspace` wall time before / after 10c in docs/METRICS.csv.
2. **wasm / android checks.** `cargo check --workspace --lib --target wasm32` and `tools/android_check` include the tools
   lib: liblzma / sha2 / clap must be `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` of th08-tools and its
   lib.rs `#![cfg(not(target_arch = "wasm32"))]`; android keeps building liblzma static via NDK clang (as today).
   core / render / audio must stay wasm- and android-clean (REWRITE_PLAN mobile W rule).
3. **Feature flags.** `gamepad-linux` stays on `th08` (joystick.rs:8–49); arch_check builds without it. `webgl` stays on
   wgpu in th08-render; cpal `wasm-bindgen` feature moves with cpal to th08-audio; winit `android-native-activity` to
   th08 (and render's winit dep must not disable it: same version, features unify).
4. **`[lib] crate-type`.** Stays `["cdylib", "rlib"]` on root `th08` (ARCH_D's `th08-app` = this package; renaming would
   break web/build.sh `th08.wasm`, release.yml, appimage). th08-tools depends on the rlib. core/render/audio rlib only.
   On Windows bin `th08.exe` + `th08.dll` coexist as today.
5. **Private items in tools.** After 10b no tool uses `pub(crate)`; 10c compile errors of this kind are fixed by
   `pub` on the item (recorded in the run report), never by moving logic into tools.
6. **tests / paths.** `CARGO_MANIFEST_DIR`-relative paths (session.rs:607, 763; drawlog.rs:2117, 2446; result.rs:1928;
   demosync.rs:329; bin/session.rs:58; bin/demosync.rs:26; drawlist.rs:791) break when files move under crates/*:
   10b routes them through `repo_root()`. `include_bytes!("rsqrt_amd.bin")` moves with fmath.rs; rsqrt_table writes
   `crates/th08-core/src/rsqrt_amd.bin`. `tests/port_cfg.rs` stays in the root package. `cargo test --workspace` count
   must equal the pre-cut count (tests moved, none lost).
7. **`image` in core** (drawlist.rs:697: .jpg decode so the texture size is known). Pure and deterministic, kept for a
   pure move; step 11 candidate: size from the JPEG/PNG header, pixels decoded by the renderer (ARCH_B D1).
8. **midi_ready.** Logic reads `AudioOut::midi_available()` once at load (game.rs:264; soundfont present). Tools must
   build the real `th08_audio::Audio` probe (or force false like session.rs:758) so traces stay equal; a `NullAudio`
   with a different answer would change the music path.
9. **Binary names / scripts.** arch_check uses `$BIN/session|demosync|drawrun` from `target/release` (arch_check:13–15):
   workspace bins land there unchanged. `tools/offscreen launchershot` and `cargo run --bin launchershot` keep working
   (bin name unique in the workspace). release.yml / build-appimage.sh `--bin th08` and pages.yml (web/build.sh at the
   root) unchanged.
10. **clippy.toml** is found from crates/* (searched upward); `deny(disallowed_methods)` on core + formats only; the
    per-module `allow` in audio.rs / gfx.rs / midi.rs / drawlog.rs become unnecessary but harmless.
