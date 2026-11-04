# Reverse engineering (dirty room)

This directory holds the tools used to study the original `th08.exe` (version 1.00d). Its results are **not** here:
every finding is written into the numbered specifications in [`../specs/`](../specs/), which are the only source of
truth for the port. Port code never reads anything in `re/`.

## Where the findings are

| Topic | Spec |
|---|---|
| main loop, task chains, scenes | `specs/00_overview.md`, `11_rng_and_flow.md` |
| `th08.dat`, `thbgm.dat`, images, file index | `01_archive.md`, `02_bgm.md`, `03_images.md`, `04_file_index.md` |
| ANM, STD, ECL virtual machines | `05_anm_vm.md`, `06_std.md`, `07_ecl.md` |
| bullets, items, player, bombs and options | `08_bullets_items.md`, `09_player.md`, `14_bombs_options.md` |
| dialogue, menus and HUD, sound and effects | `10_msg.md`, `12_menus_hud.md`, `13_sfx_effects.md` |
| config, replays, `score.dat`, endings, result, music room | `15_files_replay_score.md`, `16_endings_result.md` |
| structure layouts | `structs.md` |
| which functions are covered | `COVERAGE.md` |
| draw-log and session formats, reference traces | `data/drawlog/FORMAT.md`, `data/README.md` |
| evidence for each spec change | `changelog/answers_*.md` |

Function and global names of the original, with signatures and confidence, are in [`symbols.csv`](symbols.csv)
(`addr,kind,name,signature,confidence,note`).

## Static analysis (Ghidra)

The Ghidra project lives in `re/ghidra/` and its exports in `re/extracted/` (both git-ignored, rebuilt locally).
`tools/redump.sh` applies `symbols.csv` and re-exports the full decompile, call graph and global cross-references;
`tools/fn.sh <addr|name>` then prints a function from that dump, `tools/gh.sh <Script.java>` runs one headless script.

Scripts in [`ghidra_scripts/`](ghidra_scripts/):

| Script | Does |
|---|---|
| `FixThiscall.java` | sets `__thiscall` on functions that read ECX before writing it (run once after import) |
| `ApplySymbolsOpus.java` | applies `symbols.csv` names and signatures (used by `redump.sh`) |
| `DumpAll2.java` | decompiles every function, writes the call graph and global xrefs (used by `redump.sh`) |
| `Decompile.java`, `Disasm.java`, `Xrefs.java`, `Strings.java` | one function, a disassembly range, references, strings |

## Runtime (wine)

The original runs under system wine on a scratch copy of the game in `re/work/` (git-ignored). The runtime probe in
[`probe/`](probe/) is a proxy `d3d8.dll` (`hook.c`) that logs every draw call, every RNG call and the game state per
frame, drives input from a script and takes screenshots. On top of it:

- `tools/record_session.sh`, `tools/play_session.sh`: record a play session in the original and replay it
  (see [`../sessions/README.md`](../sessions/README.md));
- `tools/run_rpy.sh`, `tools/run_rpy_batch.sh`: play third-party `.rpy` replays in the original, one or a batch;
- `tools/drawlog.py`, `tools/session.py`, `tools/trace2demo.py`, `tools/rngsites.py`: draw-log, session and RNG trace
  helpers.

## Format tools

Parsers and dumpers for the game's files, used to check spec claims against all the data: `th08dat.py` (archive),
`thbgm.py`, `anm.py` / `anmdis.py` / `anmspr.py`, `stddump.py`, `ecldis.py`, `msgdump.py`, `shtdump.py`,
`enddump.py`, `musiccmt.py`, `cfg.py`, `rpy.py`, `scoredat.py`, `rng.py` (reference RNG), `bullettypes.py`. Binary
helpers: `pe.py`, `rd.py`, `disx.py`, `dispscan.py`, `structxref.py`, `coverage.py`. Each prints its usage.

## Local paths

The tools find the game copy and other local files through environment variables, or `packaging/local/local.env`
(git-ignored `KEY=VALUE` lines, e.g. `TH08_GAME_SRC=<your game folder>`); see `tools/localenv.py`.
