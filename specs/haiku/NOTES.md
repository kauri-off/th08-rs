# Haiku dirty-room notes (Phase 1)

## Toolchain (decided 2026-10-17)
- REA MCP present; `ghidra` provider verified working: `open_binary(th08.exe, provider_id=ghidra)` -> PE x86, sha256 330fbdbf…d924, 2409 procedures, 1007 strings.
- Primary decompile/xref tool: REA (ephemeral import of th08.exe; annotations session-scoped).
- Persistent names: `symbols.csv` applied to `<SPEC_DIR>/ghidra/th08.gpr` via `specs/ghidra_tools/gh.sh` (ApplySymbols). Serial runs only.
- Hopper not used.

## Target facts
- th08.dat 46,838,025 bytes; thbgm.dat 449,961,024 bytes; th08.exe 840,704 bytes.
- Game dir is read-only for this agent. Never write there.

## Status
- [ ] 00_overview.md
- [ ] 01_archive.md + tools/extract_th08.py
- [ ] 02_bgm.md
- [ ] 03_images.md + tools/thtx2png.py
- [ ] 04_file_index.md
- [ ] symbols.csv / structs.md updated

## Log
- Toolchain check done. Starting archive format analysis.
