# NOTES (opus, Phase 1)

## Status
- PHASE 1 COMPLETE: 00_overview, 01_archive, 02_bgm, 03_images, 04_file_index written; tools th08dat.py, thbgm.py, anm.py, pe.py, dump_bgmtab.py; symbols applied to Ghidra (75).
- extracted/all_decomp.c = full decompile dump (ghidra_scripts/DumpAll.java) for grepping.

## Findings log
- th08.dat begins with magic "PBGZ" (VERIFIED by hexdump); rest looks obfuscated.

## Open questions / next phases
- ANM opcode semantics (AnmVm_Run 0x45EA00), STD/ECL/MSG/SHT/END/RPY formats.
- Frame-skip config semantics (skip 0 and 1 both draw every frame?).
- Exact roles of _s.std / *sp.ecl / *_xx character files (spell practice? last word?).
- Player/enemy coordinate space confirmed only for bullets.
- ARCHIVE (VERIFIED, extractor tools/th08dat.py extracts all 317 files, signatures valid):
  - open: FUN_004748c0 -> FUN_00474ce0 (header) -> FUN_00474fa0 (table parse). Lookup FUN_00474c70 (stricmp linear).
  - XOR "block-reverse" cipher FUN_0043e1d0(buf,size,key,step,block,limit). LZSS FUN_004740e0 (13-bit off, 4-bit len+3, dict 0x2000 start pos 1, offset 0 = end).
  - header: "PBGZ" + 12 bytes enc(0x1b,0x37,0xc,0x400): count+123456, tableoff+345678, tablesize+567891.
  - table: [tableoff..EOF] enc(0x3e,0x9b,0x80,0x400) then LZSS. entries: name\0, off, size, unk(=0 always).
  - per file: LZSS -> "edz"+type char -> decrypt rest with table @004c78e0 (8 entries M T A J E W - *). magic "edz" @004c7940 (stored +0x20/+0x40/+0x60).
  - generic file loader FUN_0043e660(name, &size, from_disk) strips path, arc lookup; disk fallback when flag set.
- BGM (VERIFIED, tools/thbgm.py): thbgm.dat header "ZWAV",1,0x800,0 checked in FUN_004477b1 (config load).
  thbgm.fmt (from archive): 52-byte recs, name[16], off, total_len(+0x14, used only for preload buffer size), loop_start(+0x18), loop_end(+0x1c), WAVEFORMATEX(+0x20, 18B)+2 pad; terminated by empty name.
  Stored data per track = loop_end bytes (tracks contiguous: off[i+1]=off[i]+loop_end[i]; last ends exactly at EOF).
  Stream: first pass seek off, play loop_end bytes; loop: seek off+loop_start, play loop_end-loop_start (FUN_004733e0).
  Play: FUN_00447d56(slot,name) replaces ext with "wav" when cfg music mode==1 (WAV), MIDI when ==2. Lookup FUN_0045c6b0 basename; not found -> index 0.
  STD header has 3 BGM names at +0x290/+0x310/+0x390 (128B each, ' ' = none).
- ANM (VERIFIED, tools/anm.py, 0 problems over 113 files): entry chain, 0x40 header (version 3), sprite offs, script (id ignored, offset) pairs, THTX (fmt/w/h/size). Formats 1=8888,3=565,5=4444. UV normalised by header w/h. Instr header u16 op,u16 size,i16 time,u16 mask. Docs: 03_images.md.
- Files written: 01_archive.md, 02_bgm.md, 03_images.md. Next: 00_overview.md (main loop, managers), 04_file_index.md.
- OVERVIEW (00_overview.md written): Frame_Run 60Hz deadline loop, funcChain calc/draw priorities table, playfield viewport (32,16,384,448), camera fov pi/6.
- 04_file_index.md written: stage index table (0..8, 9..17 = _s/sp), 12-entry character tables, msg team letters, slot map.
