# Architecture

How the port is built today. The design reasoning behind it is in [DECISIONS.md](DECISIONS.md) and the historical
plans in [arch/](arch/ARCH_SUMMARY.md); behaviour itself is described only by the specs in [`specs/`](../specs/).

## Crates

```
th08-formats ─► th08-core ─┬─► th08-render ─┐
  (parsers)     (logic)    ├─► th08-audio ──┼─► th08 (app: src/) ─► th08-tools (debug bins)
                           ├─► th08-thcrap ─┘        ▲
                           ├─► th08-repack ──────────┘ (Android import only)
                           └─► th08-replay (headless runner, core only)
xtask (packaging, no game crates)
```

| Crate | Owns | Does not touch |
|---|---|---|
| `th08-formats` | parsers and writers of the original files (archive, ANM, ECL, STD, MSG, replay, score.dat…); bytes in, structs out | IO, `unsafe` |
| `th08-core` | all game logic: `World`, the frame driver `Ctx`, every ported subsystem, the emits that turn state into draw commands, `fmath` | window, GPU, sound device, file system, clocks, threads |
| `th08-render` | `Gfx`: executes the core's `DrawCmd` / `ResCmd` lists on wgpu; text rasterising and translated text (`textdb`) | game state |
| `th08-audio` | `Audio`: executes the core's `SoundCmd`s on cpal; WAV BGM from `thbgm.dat`, MIDI sequencer and synth | game state |
| `th08-thcrap` | thpatch translations → presentation packs (download, key mapping, fonts, build, install) | logic |
| `th08-repack` | `th08.pak` / `thbgm.pak` from the `.dat` files; the Android import job | logic |
| `th08-replay` | sessions and demos played on `th08-core` alone (`MemHost`, no render / audio), per-row trace hashes | — |
| `th08` (`src/`) | winit window and event loop, launcher, settings, `Host` implementation, data sources, pacer, input, port menu, touch overlay; wasm and Android entries | logic |
| `th08-tools` | trace, draw-log and comparison tools (`session`, `drawrun`, `demosync`, viewers, `repack`) | — |
| `xtask` | `cargo xtask dist`: AppImage, Windows exe / installer, APK (through `android/gradlew`), web | — |

The Android launcher UI is Kotlin / Compose in `android/` and talks to Rust through small JNI bridges (settings
schema, packs, game import); the game itself runs in a separate `:game` process on a NativeActivity. The web front
end is `web/` (wasm-bindgen).

## One frame

1. The app's pacer (`src/pacer.rs`) decides when a logic frame is due (60 Hz, or turbo / replay transport).
2. `Ctx::calc` runs the calc chain over `World`: input comes only through `Host::poll_input`, then Sound_Update
   queues `SoundCmd`s, which `AudioOut` (`th08-audio`) executes in order. Nothing is read back from audio.
3. `Ctx::draw` runs the draw chain: each draw node's draw-phase step updates logic state that the original changes
   while drawing, then its emit (`th08-core/src/emit/`) reads a read-only `View` of the `World` and records
   `DrawCmd`s / `ResCmd`s into the `Recorder`.
4. At Present the `Renderer` (`Gfx`) executes the recorded list. Port-only layers (touch overlay, hitboxes, port
   menu, replay transport HUD; `src/platform/`) are egui passes drawn after the game frame, outside the core.

`World` holds all deterministic state and is `Clone`: going to the background and the Android destroy path write
`score.dat` from a clone, and `worldclone.rs` tests that a clone continues like the original.

## Where things plug in

- **Host** (`th08-core/src/host.rs`): input polls, the boot clock, play-time ticks, local time and save storage
  (`read` / `write` / `list` / `mkdir`). The app implements it in `src/platform.rs` (files on desktop and Android,
  site storage on the web); tests and `th08-replay` use `MemHost`.
- **Game data** (`th08-core/src/assets.rs` traits, `src/sources.rs`): a game folder, the Android import folder or
  files picked in the browser; `.dat` or the repacked `.pak` ([arch/ASSETS_FORMAT.md](arch/ASSETS_FORMAT.md)).
- **Settings** (`src/settings.rs`): one typed table, command line > env / URL > `port.cfg` > defaults; the launchers
  (egui and Compose) are generated from it. The core gets only `CoreConfig`.
- **Saves**: `score.dat`, `th08.cfg` and replays go through `Host`; `src/saves.rs` zips and restores them.
- **Translations** (`src/packs.rs`, `th08-render/src/textdb.rs`, `th08-thcrap`): a pack replaces presentation text,
  fonts and images only; logic keys are refused ([arch/I18N.md](arch/I18N.md)).

## How determinism is kept and checked

- Logic floats use only `th08-core/src/fmath.rs` (pinned transcendentals, the AMD `rsqrtss` model).
  `crates/th08-core/clippy.toml` bans the std `sin` / `cos` / `atan2` / … and interior-mutability types, so an emit
  holding `&World` cannot change logic state. `tools/arch_check` fails when `th08-core` depends on wgpu, cpal,
  winit, egui, rfd, clap or liblzma.
- `tools/fmath_xplat/run.sh` (CI) checks `fmath` bit for bit on x86_64, wasm32 and aarch64.
- `tools/xplat_replays` plays recorded replays with `th08-replay` on x86_64, aarch64 (qemu), wasm32-wasip1
  (wasmtime) and windows-gnu (wine) and compares the per-row hashes.
- `tools/arch_check freeze DIR` / `run DIR`: port against port. It replays every recorded session, the four demos
  and the pilot replays (also with emits off) and compares traces and draw hashes with a reference from the commit
  before the change.
- The replay batch (`tools/batch_job`): port against the original. Third-party replays recorded in the original
  under wine must give the same whole trace ([../sessions/README.md](../sessions/README.md)).
- `tools/check_level` says which of these a change needs ([../CONTRIBUTING.md](../CONTRIBUTING.md#checks)).
