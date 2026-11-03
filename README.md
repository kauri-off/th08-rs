# th08: Touhou 8 rebuilt from scratch in Rust

[Русский](README.ru.md) · [日本語](README.ja.md)

This is an unofficial fan reimplementation of 東方永夜抄 ~ Imperishable Night (Team Shanghai Alice, 2004), written in
Rust. It is not affiliated with Team Shanghai Alice and contains no game data. To play it you need your own copy of
the game: the port reads `th08.dat` and `thbgm.dat` from it.

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
- **It runs** natively on Windows and Linux, and in the browser through WebAssembly with WebGPU or WebGL2.

## Where it is now

The port is complete. I am now rewriting it into idiomatic Rust:

- the global state becomes one cloneable `World`;
- drawing becomes a command list;
- the core is cut off from the window, sound and file system.

Behaviour is frozen during the rewrite. After every step the port is checked against itself and against the replay
set. Next comes an Android build, then a mobile web build, with a touch overlay you can rearrange.

## Trying it

Prebuilt Windows and Linux builds are on the [releases page](https://github.com/kauri-off/th08-rs/releases). A browser
build is at https://kauri-off.github.io/th08-rs/. On first start the port asks for your Touhou 8 folder, the one that
contains `th08.dat`.

To build it yourself, you need stable Rust:

```sh
cargo run --release -- "/path/to/TH08 - Imperishable Night"
```

The controls are the original's: arrow keys, `Z` shoot, `X` bomb, `Shift` focus, `Esc` pause. Run `th08 --help` to
see every setting. For the web build, see `web/build.sh`.

## Repository layout

| Path | Contents |
|---|---|
| `specs/` | the specifications: the only source of truth for behaviour |
| `src/` | the port, plus tools in `src/bin/` |
| `re/` | reverse-engineering tools: format dumpers, Ghidra scripts, the runtime probe, the session recorder |
| `docs/` | status, decisions, task briefs, the architecture of the rewrite |
| `web/`, `packaging/` | the browser front end and release packaging |

## License

GPL-3.0-or-later (see `LICENSE`). It covers only the code and documents in this repository. Touhou Project and all
game assets belong to Team Shanghai Alice.
