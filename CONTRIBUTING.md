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
- Heavy runs (arch_check, tests with game data, the emulator, replay batches) can be run directly. The job queue
  `tools/q submit --kind check|test|emu|full|batch --label NAME -C DIR -- CMD…` (then `tools/q wait ID`) is optional:
  it shares one machine between parallel runs and needs a systemd user session. Plain cargo works everywhere.
- Windowed binaries in automated runs go through `tools/offscreen <cmd>`.
- Replay batch (port vs the original on third-party replays): `tools/batch_job NAME` queues a `batch` job that builds
  `session` (release) once and runs `tools/batch_run` on the replay sets `ref500` and `ref_b2` under
  `$TH08_EXT_REPLAYS` (from `packaging/local/local.env`, like the game dir `TH08_GAME_SRC`). A replay passes when its
  whole trace matches the recording; a port trace that ends early fails. The job prints the replays that left
  `tools/batch/<set>.identical` and fails on any of them, or when fewer replays ran than expected.
  `TH08_BATCH_LIMIT=N` runs only the first N replays of each set; outputs go to `target/batch_NAME/`.

CI (`.github/workflows/check.yml`, on every push to `master` and every pull request) runs `cargo fmt --check`,
clippy with `-D warnings`, `cargo test --workspace`, the wasm and aarch64 Android `cargo check` and
`tools/spec_refs.py`. It has no game data, so tests that need `TH08_DIR` skip there, and arch_check, the replay
batches and the emulator stay local: `tools/check_level` says which of them a change needs.

## Game data and replays

The repository contains no game data. Everything that runs the game, including the tests with game data and the
regression checks, needs your own copy of Touhou 8 (`th08.dat`, `thbgm.dat`; point `TH08_DIR` at the folder).
Reference sessions are recorded in the original game (see [sessions/README.md](sessions/README.md)); third-party
replays come from public Touhou replay sites. Neither is committed.

## Prerequisites per target

The Rust toolchain is pinned in `rust-toolchain.toml` (1.98.0, with clippy, rustfmt and the wasm, Android and
windows-gnu targets); rustup installs it on the first `cargo` call. Package names are Debian / Ubuntu (`apt`) and
Arch (`pacman`).

| Target / tool | Needs |
|---|---|
| Linux desktop (`cargo run`, tests, clippy) | apt `pkg-config libasound2-dev libudev-dev`; pacman `pkgconf alsa-lib systemd-libs` |
| `windows-exe` / `windows-nsis` from Linux | apt `gcc-mingw-w64-x86-64 binutils-mingw-w64-x86-64`, pacman `mingw-w64-gcc`; NSIS also `makensis` (apt `nsis`, AUR `nsis`) or docker |
| `windows-exe` on Windows | the MSVC build tools (`x86_64-pc-windows-msvc`, as in the release workflow) |
| `linux-appimage` | `curl` (xtask downloads appimagetool 1.9.1, sha256-pinned) |
| `web` | `cargo install wasm-bindgen-cli --version 0.2.129` (must equal `wasm-bindgen` in `Cargo.lock`) |
| `android-apk`, `tools/android_check` | JDK 17 (apt `openjdk-17-jdk`, pacman `jdk17-openjdk`); Android SDK with `platforms;android-36`, `build-tools;36.0.0`, `ndk;27.1.12297006` (`ANDROID_SDK`, else `ANDROID_HOME`, else `~/.local/share/android-sdk/Sdk`). Gradle comes from `android/gradlew` |
| Android emulator checks | the SDK `emulator` and an x86_64 system image (`--abi x86_64`), `adb` |
| `tools/xplat_replays` | `rustup target add wasm32-wasip1`, `wasmtime`, apt `qemu-user` / pacman `qemu-user`, the NDK above, wine for the Windows run |
| `tools/fmath_xplat/run.sh` | `node` (apt / pacman `nodejs`), `qemu-user`, the NDK |
| Windowed binaries in scripts (`tools/offscreen`) | apt `xvfb` / pacman `xorg-server-xvfb` (`xvfb-run`) |
| Wine runs (`tools/wine_headless`) | `wine` and xvfb; always a throwaway `WINEPREFIX` |

Gradle is pinned by the wrapper in `android/` (Gradle 9.8.2, distribution sha256 in
`android/gradle/wrapper/gradle-wrapper.properties`; `gradle-wrapper.jar` sha256
`3b8a25775a69158b5ad2b1d17a80a88dc7b40a352a73eb27d06c612a7ce68e98`, Gradle's published value, also checked by the
release workflow's `setup-gradle`). No system Gradle is needed; xtask calls `android/gradlew`.

An Android build for contributors needs no key: without `--keystore` and `TH08_KEYSTORE_PROPS`,
`cargo xtask dist --target android-apk` builds the `debug` channel, signed with a throwaway key.

## Packaging and versions

`cargo xtask dist --target <linux-appimage|windows-exe|windows-nsis|android-apk|web>` builds a package into
`target/dist/`; `cargo xtask dist --help` lists the options; what each target needs is in
[Prerequisites per target](#prerequisites-per-target).
With `TH08_RELEASE_TAG` set (the release workflow) and no game data, the AppImage embeds the update information
`gh-releases-zsync|kauri-off|th08-rs|latest|th08-*-linux-appimage.AppImage.zsync` and gets a `.zsync` beside it,
so AppImageUpdate and AppImage managers can update it from the latest release; local builds carry none.

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
| `dev` | `org.th08.port.dev` | th08 dev | `--keystore PROPS` or `TH08_KEYSTORE_PROPS` | personal builds, may bundle game data and packs; the default when a key is configured |
| `debug` | `org.th08.port.debug` | th08 debug | throwaway `target/android/debug.keystore` | contributors, emulators; the default without a key |

`PROPS` is a `keystore.properties` file with `storeFile` (absolute or relative to the file), `storePassword`,
`keyAlias` and `keyPassword`. Only its path is passed on; Gradle reads the passwords. Never commit a keystore.
The release channel never falls back to `TH08_KEYSTORE_PROPS`: an APK signed with another key cannot update the
published one, and switching back means an uninstall, which deletes the saves.

Android build env: `TH08_ANDROID_MIN_SDK` (minSdk and NDK clang API, default 26) and `TH08_ANDROID_PROFILE` (cargo
profile of `libth08.so`, default `release`).

The release workflow (`.github/workflows/release.yml`) runs by hand from the Actions tab on `master`
(`gh workflow run release.yml -f publish=true`). With `publish` it tags `v<Cargo.toml version>` at that commit and
creates the release; without it only builds the artifacts. To release, bump `version` in `Cargo.toml`, commit,
push, then run the workflow; it refuses a version whose tag already points at another commit, and a version that
already has a release (published assets are never replaced: bump the version, or delete the release by hand). Tags
are not pushed by hand: a tag-triggered run could not reuse the build cache, which is kept per branch.

The APK is signed with the secrets of the GitHub environment `release` (Settings → Environments), not repository
secrets. The environment must contain:

- secret `TH08_ANDROID_KEYSTORE_B64`: the `.jks`, base64-encoded (`base64 -w0 release.jks`);
- secret `TH08_ANDROID_STORE_PASSWORD`;
- secret `TH08_ANDROID_KEY_ALIAS`;
- secret `TH08_ANDROID_KEY_PASSWORD`;
- deployment branches: `master` only (optionally a required reviewer).

Only the `android-sign` job uses that environment. The `android` job builds the release channel with a key made for
that run, so Gradle and the third-party actions never see the release key; `android-sign` downloads that APK,
re-signs it with `apksigner` and uploads it as artifact `th08-android`. With `publish`, a missing secret fails the
run. Without `publish` and without the secrets (forks), only the artifact `presign-android` is left: an APK signed
with a throwaway key, which can never update the published app.

With `publish`, the release gets five files: the Windows installer (`windows-nsis`: per-user, no admin; its
th08.exe is cross-built with MinGW on Linux and packed by makensis from apt) and the portable exe (`windows-exe`,
MSVC), the AppImage and its `.zsync`, and the APK; plus a `SHA256SUMS` file over them and a build provenance
attestation for each (`gh attestation verify th08-<version>-android-apk.apk -R kauri-off/th08-rs`). All actions in the workflows
are pinned by commit SHA, the version in a trailing comment; `pages.yml` checks the downloaded wasm-bindgen against
a pinned sha256, so a wasm-bindgen bump in `Cargo.lock` also updates `PIN_VERSION` and `PIN_SHA256` there.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `feat(core): …`, `fix(render): …`, `docs: …`,
`chore(re): …`. One verified step per commit.
