# th08: Touhou 8 rebuilt from scratch in Rust

[![Check](https://github.com/kauri-off/th08-rs/actions/workflows/check.yml/badge.svg?branch=master)](https://github.com/kauri-off/th08-rs/actions/workflows/check.yml)

[Русский](README.ru.md) · [日本語](README.ja.md)

This is an unofficial fan reimplementation of 東方永夜抄 ~ Imperishable Night (Team Shanghai Alice, 2004), written in
Rust. It is not affiliated with Team Shanghai Alice and contains no game data. To play it you need your own copy of
the game: the port reads `th08.dat` and `thbgm.dat` from it.

## Play it

1. **Download** from the [releases page](https://github.com/kauri-off/th08-rs/releases): the `.exe` for Windows, the
   `.AppImage` for Linux (`chmod +x`, then run), the `.apk` for Android. No install needed in the
   [browser](https://kauri-off.github.io/th08-rs/).
2. **Take from your own copy of the game** `th08.dat` and `thbgm.dat` (or `th08.pak` / `thbgm.pak` if you have
   them). The port contains no game data: you must own Touhou 8.
3. **Put them where the port looks:**
   - Windows / Linux: nothing to copy; pick your game folder in the launcher on the first start.
   - Android: in the launcher, tap **Import game files** and pick `th08.dat` and `thbgm.dat`; the app converts them
     into its own storage. This arrives in 0.1.2.
4. **Saves, replays and `port.cfg`** live in `%APPDATA%\th08\` (Windows), `~/.config/th08/` (Linux) or the app's
   private storage (Android: uninstalling deletes them).
5. **Translations:** the **Translations** panel in the launcher downloads one from Touhou Patch Center and builds the
   pack on your device from your game data; then choose it in **Game text language**.

Android builds come in three channels: `release` (`org.th08.port`) is the one on GitHub; `dev` and `debug` are local
builds. Details, exact paths and controls: [docs/PLAYING.md](docs/PLAYING.md).

## What I set out to do

I wanted a port that is not just similar to the original but identical to it. The test is strict: give it the same
inputs, and it must play out the same game, frame by frame and random-number call by random-number call. If it does,
any replay recorded in the original game also plays in the port.

I also wanted the port written cleanly. Nobody writing the Rust code was allowed to look at the original executable.

## How it was done

The work was split into two rooms, the classic clean-room method.

- **The dirty room** studied `th08.exe` with Ghidra and with a runtime probe: a proxy `d3d8.dll` that logs every draw
  call, every RNG call and the game state, frame by frame. Its output was not code. It was 17 written specifications
  (`specs/`, about 10,000 lines) of prose, tables, structure layouts, constants and short pseudocode.
- **The clean room** wrote the Rust code from those specifications alone. Where the spec was not enough, the
  implementer did not guess. It wrote down the question, sent it back to the dirty room, and the answer went into the
  spec first.

Nearly all of the work in both rooms was done by AI agents (Claude). My part was directing: splitting the work into
small tasks, keeping the two rooms apart, deciding what counts as correct, and checking the results. It took about
two and a half weeks (from 2026-10-16), roughly 190 implementation tasks and 70 rounds of reverse-engineering
questions.

## How "identical" was checked

The original was run under Wine, and the port was run with the same input. Their per-frame traces of game state and
their draw logs were then compared:

- the game's own demo plays;
- my own recorded play sessions, from the first stage to the end;
- replays from the community. I collected about 13,000 `.rpy` files and played a sample of 1,010 through
  both programs.

One rule mattered more than any other: **a trace may show where the port differs, but it never says how the game
works.** Only the specification does. When a trace and the port disagreed and the spec could not explain it, that
became a question for the dirty room, never a patch that just made the numbers match.

## What came out

- **Game logic is bit-exact.** All 1,010 community replays and all recorded sessions match the original, including
  every frame, every RNG call, score, items and deaths.
- **Drawing matches visually.** Every remaining draw difference has been tracked down to a known, accepted cause, such
  as sub-pixel floating-point rounding or the frame on which a loader finishes.
- **The whole game is there:** all stages including Extra, all 12 shot types, bombs and Last Spells, spell cards,
  dialogue, the night clock, menus, options, replays, `score.dat`, endings, the music room and BGM.
- **It runs** natively on Windows, Linux and Android, and in the browser through WebAssembly with WebGPU or WebGL2.

## Where it is now

The port is complete. I am now rewriting it into idiomatic Rust:

- the global state becomes one cloneable `World`;
- drawing becomes a command list;
- the core is cut off from the window, sound and file system.

Behaviour is frozen during the rewrite. After every step the port is checked against itself and against the replay
set. The Android build, with a touch overlay you can rearrange, is out; the mobile web build comes next.

## Building it

Prebuilt packages: see [Play it](#play-it). To build it yourself, you need stable Rust:

```sh
cargo run --release -- "/path/to/TH08 - Imperishable Night"
```

The controls are the original's: arrow keys, `Z` shoot, `X` bomb, `Shift` focus, `Esc` pause. Run `th08 --help` to
see every setting. Packages (AppImage, Windows exe or installer, Android APK, web) are built with
`cargo xtask dist --target <linux-appimage|windows-exe|windows-nsis|android-apk|web>`; `cargo xtask dist --help` lists
the options; channels, versions and CI signing are in [CONTRIBUTING.md](CONTRIBUTING.md#packaging-and-versions).

## Repository layout

| Path | Contents |
|---|---|
| `specs/` | the specifications: the only source of truth for behaviour |
| `src/` | the app: game window, launcher, settings, platform (package `th08`, binary `th08`) |
| `crates/` | `th08-formats` (file formats), `th08-core` (game logic), `th08-render` (wgpu), `th08-audio` (cpal, MIDI), `th08-tools` (trace / draw-log tools, bins in `src/bin/`) |
| `re/` | reverse-engineering tools: format dumpers, Ghidra scripts, the runtime probe, the session recorder ([index](re/README.md)) |
| `docs/` | status, decisions, the architecture of the rewrite ([index](docs/README.md)) |

| `web/`, `packaging/`, `xtask/` | the browser front end, packaging files and `cargo xtask dist` |

How to contribute: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

GPL-3.0-or-later (see `LICENSE`). It covers only the code and documents in this repository. Touhou Project and all
game assets belong to Team Shanghai Alice.
