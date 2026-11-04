# Contributing

Thanks for your interest. This port has one goal that shapes every rule below: given the same inputs, it must play
out exactly the same game as the original `th08.exe` 1.00d, frame by frame and RNG call by RNG call.

## The clean-room rule

- The numbered specifications `specs/00–16_*.md` and `specs/structs.md` are the **only** source of truth for game
  behaviour. Code in `src/` and `crates/` is written from them alone, never from decompiler output, disassembly or the
  executable.
- Reverse engineering happens in `re/` (see [re/README.md](re/README.md)). Its output is spec prose, tables and
  short pseudocode, never code for the port.
- Traces and draw logs of the original show *where* the port differs, never *how* the game works. A fix that only
  makes a trace match, without a spec behind it, is not accepted.

## How a spec question flows

1. While porting, the spec does not say enough. Do not guess: leave a one-line `// SPEC-GAP: …` marker at the site
   and write the question down (an issue, with the spec section it is about).
2. The reverse-engineering side answers by fixing the numbered spec **in place**, with the original's addresses as
   evidence, and records the change and its evidence in `specs/changelog/answers_*.md`. Every fact in a changelog
   must also be in a numbered spec.
3. The port is then changed from the updated spec, and the `SPEC-GAP` marker goes away.

Port-only code (launcher, settings, platform, rendering backend, tools, `xtask`) is not spec-bound.

## Code and comments

The comment rules (one `//!` line per module with its spec section, citations only where they add something, kept
quirks explained in 1–3 lines, no process history in code) are in [CLAUDE.md](CLAUDE.md). They apply to people and AI
agents alike. Parity rules for logic and drawing: [docs/PARITY_POLICY.md](docs/PARITY_POLICY.md).

## Checks

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` for ordinary changes.
- `python3 tools/spec_refs.py`: every `NN §x.y` citation in the code must point at an existing spec heading.
- `tools/arch_check run DIR`: the full regression check. It builds, runs clippy, the wasm and Android checks and the
  tests, then replays every recorded session, demo and pilot replay and compares traces and draw hashes with a
  reference made by `tools/arch_check freeze DIR` on the commit before your change. Any difference fails it.
- Heavy runs (arch_check, tests with game data, the Android emulator, replay batches) go through the job queue
  `tools/q submit --kind check|test|emu|full|batch --label NAME -C DIR -- CMD…`, then `tools/q wait ID` until it ends.
- Windowed binaries in automated runs go through `tools/offscreen <cmd>`.

## Game data and replays

The repository contains no game data. Everything that runs the game, including the tests with game data and the
regression checks, needs your own copy of Touhou 8 (`th08.dat`, `thbgm.dat`; point `TH08_DIR` at the folder).
Reference sessions are recorded in the original game (see [sessions/README.md](sessions/README.md)); third-party
replays come from public Touhou replay sites. Neither is committed.

## Packaging and versions

`cargo xtask dist --target <linux-appimage|windows-exe|windows-nsis|android-apk|web>` builds a package into
`target/dist/`; `cargo xtask dist --help` lists the options.

Versions come from git; nothing is edited by hand:

- The version name is `git describe --tags --dirty --always` without a leading `v` (`0.1.0-42-g3409632-dirty`), or
  `<Cargo version>-g<sha>` when no tag is reachable. A published release is named `X.Y.Z`, the workspace
  `version` in `Cargo.toml`, and tagged `vX.Y.Z` by the release workflow.
- The Android `versionCode` is the number of minutes from 2026-01-01T00:00Z to HEAD's commit time, or to the build
  time when the tree is dirty, so a fresh build always installs over an older one. `VERSION_CODE=N` overrides it.
- The launchers and the port menu show "version (channel)" (desktop: the version alone).

Android has three channels, installable side by side:

| `--channel` | applicationId | label | signing | use |
|---|---|---|---|---|
| `release` | `org.th08.port` | th08 | `--keystore PROPS` only | GitHub Releases; `--assets none` only, no `--packs` |
| `dev` (default) | `org.th08.port.dev` | th08 dev | `--keystore PROPS` or `TH08_KEYSTORE_PROPS` | personal builds, may bundle game data and packs |
| `debug` | `org.th08.port.debug` | th08 debug | `--debug-key` (throwaway `target/android/debug.keystore`) | agents and emulators |

`PROPS` is a `keystore.properties` file with `storeFile` (absolute or relative to the file), `storePassword`,
`keyAlias` and `keyPassword`. Only its path is passed on; Gradle reads the passwords. Never commit a keystore.
The release channel never falls back to `TH08_KEYSTORE_PROPS`: an APK signed with another key cannot update the
published one, and switching back means an uninstall, which deletes the saves.

Android build env: `TH08_ANDROID_MIN_SDK` (minSdk and NDK clang API, default 26) and `TH08_ANDROID_PROFILE` (cargo
profile of `libth08.so`, default `release`).

The release workflow (`.github/workflows/release.yml`) runs by hand from the Actions tab on `master`
(`gh workflow run release.yml -f publish=true`). With `publish` it tags `v<Cargo.toml version>` at that commit and
creates the release; without it only builds the artifacts. To release, bump `version` in `Cargo.toml`, commit,
push, then run the workflow; it refuses a version whose tag already points at another commit. Tags are not pushed
by hand: a tag-triggered run could not reuse the build cache, which is kept per branch. It signs the APK with these
repository secrets:

- `TH08_ANDROID_KEYSTORE_B64`: the `.jks`, base64-encoded (`base64 -w0 release.jks`);
- `TH08_ANDROID_STORE_PASSWORD`;
- `TH08_ANDROID_KEY_ALIAS`;
- `TH08_ANDROID_KEY_PASSWORD`.

The job decodes the keystore into `$RUNNER_TEMP`, writes a properties file next to it and deletes both at the end.
Without the secrets (forks) it builds the debug channel with the throwaway key instead, as artifact
`th08-android-debug` and file `th08-<version>-android-apk-debug.apk`. With `publish` the APK is attached to the
release.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `feat(core): …`, `fix(render): …`, `docs: …`,
`chore(re): …`. One verified step per commit.
