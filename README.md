# th08 — a clean-room Rust reimplementation of Touhou 8: Imperishable Night

An engine reimplementation of 東方永夜抄 ~ Imperishable Night (Team Shanghai Alice, 2004) in Rust. It runs natively
(winit + wgpu + cpal) and in the browser (WebAssembly, WebGPU/WebGL2). The goal is a frame-exact port: the same
input must produce the same game, frame by frame and RNG call by RNG call, as the original `th08.exe`.

**No game data is included.** You need your own copy of the game: the port reads `th08.dat` and `thbgm.dat` from
the original install at runtime.

This is an unofficial fan project, not affiliated with Team Shanghai Alice.

## Status

Work in progress. Working today:
- boot and title, the main menu, options, and character, difficulty and practice selection;
- all stages (1–6A/6B and Extra) with backgrounds, enemies, ECL scripts, bullets, lasers and items;
- all 12 shot types, all bombs and Last Spells, and the human/youkai gauge and time orbs;
- dialogue, the HUD, boss life bars and spell cards with capture bonuses, and the night clock;
- the pause and continue menus, game over, and the result screen with name entry and `score.dat` saving;
- BGM (`thbgm.dat`) and sound effects;
- replays of the original's demo plays and of recorded sessions.

Not done yet:
- the real endings and staff roll (currently a stub);
- `th08.cfg` and key config, replay save/load from the menus, the music room, spell practice, and MIDI mode.

Synchronisation with the original is measured against recorded sessions; see `docs/STATUS.md` for numbers.

## Building and running

Requires a recent stable Rust (edition 2024).

```sh
cargo run --release -- "/path/to/TH08 - Imperishable Night"
# or
TH08_DIR="/path/to/TH08 - Imperishable Night" cargo run --release
# or with no folder: a folder dialog asks once and remembers it
cargo run --release
```

The game folder is taken from the first argument, else `TH08_DIR`, else `game_dir` in the port config file, else a
folder dialog (the folder must contain `th08.dat`; file names are matched case-insensitively). A folder picked in
the dialog is saved. Cancelling the dialog exits.

Settings (`th08 --help` lists all of them, grouped, with one line each) come from, highest first:
1. command-line flags: `--stage 4a`, `--player=3`, `--invuln` (booleans take `1/0`, `true/false`, `on/off`; the bare
   flag means on);
2. environment variables: `TH08_STAGE=4a` (on the web: URL parameters without the prefix, `?stage=4a`);
3. the port config file `port.cfg`: one `key = value` per line, `#` comments, any setting by its flag name
   (`stage_frame = 600`);
4. defaults.

A bad value is logged and ignored. `port.cfg` lives in `$XDG_CONFIG_HOME/th08/` (`~/.config/th08/`) on Linux,
`%APPDATA%\th08\` on Windows and `~/Library/Application Support/th08/` on macOS, next to the port's `score.dat`
and replays. It is written only when you pick a folder or toggle fullscreen. The game's own `th08.cfg` is separate.

Display settings (port only, they never affect the game logic):
- `scale = 1..4`: window size 640×480 × scale (default 1);
- `fullscreen = on|off`: Alt+Enter toggles it and saves the choice;
- `vsync = on|off` (default on).
The web build ignores `scale` and `fullscreen`.

Controls follow the original:
- arrow keys move; `Z` shoots; `X` bombs; `Shift` focuses;
- `Esc` pauses; `Ctrl` skips dialogue;
- `Enter` confirms in menus.

Web build (needs `wasm-bindgen-cli` and the `wasm32-unknown-unknown` target):

```sh
web/build.sh
python3 -m http.server -d web/dist 8087
```

Live build: https://kauri-off.github.io/th_08/ (deployed on every push to master). Open the page and select the game files; they are cached in the browser (OPFS).

Tests that need game data read `TH08_DIR` (or `game_dir` from `port.cfg`):

```sh
TH08_DIR="/path/to/TH08 - Imperishable Night" cargo test --release
```

Debug switches (all in `th08 --help`), e.g.:
- `--stage <1..6b|ex>` / `TH08_STAGE`: start a game directly at a stage;
- `--player <shot type>` / `TH08_PLAYER`: pick the shot type;
- `--invuln` / `TH08_INVULN=1`: the player can't die;
- `--shot <frame>` / `TH08_SHOT`: save a screenshot at that frame and exit.

## How it is made: clean room

The port is written with a clean-room process, split between two roles.

| Role | Works in | Inputs | Output |
|---|---|---|---|
| Dirty room (reverse engineering) | `re/` | `th08.exe` and the data files, studied with Ghidra and runtime tracing (a `d3d8.dll` proxy that logs draws, RNG calls and game state; `re/probe/`) | written specifications in `specs/`: prose, tables, structure layouts, constants and short pseudocode, never decompiled code |
| Clean room (implementation) | `src/` | only `specs/` | the Rust code |

The clean room never looks at the executable or at `re/`.

Correctness is checked by replaying the same inputs in both programs and comparing per-frame traces and draw logs:
- the original's demo plays;
- sessions recorded from human play (`re/tools/record_session.sh`), in `specs/data/sessions/`.

Most of the reverse engineering, specification and implementation was done by AI agents (Claude) under human
direction.

## Repository layout

| Path | Contents |
|---|---|
| `src/` | the port (library + `th08` binary); `src/bin/` holds tools: `datdump`, `anmview`, `stdview`, `eclview`, `demosync`, `session`, `drawdiff` |
| `specs/` | the specifications (`00_overview.md` … `16_endings_result.md`, `structs.md`), the coverage map, and reference data |
| `specs/changelog/` | answered questions and the evidence behind spec changes (the numbered specs already contain every fact) |
| `re/` | reverse-engineering tools: format dumpers, Ghidra scripts, the runtime probe, the session record/replay harness |
| `docs/` | project status, decisions, the agent workflow and task briefs |
| `web/` | the browser front end |

## License

GPL-3.0-or-later, see `LICENSE`. This covers the code and documents in this repository only. Touhou Project and
all game assets belong to Team Shanghai Alice.
