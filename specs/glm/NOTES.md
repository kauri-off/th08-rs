# NOTES.md — glm agent, Phase 1 (th08 reverse engineering)

## Resume checklist
- Re-read this file, symbols.csv, structs.md after compaction.
- SPEC_DIR (write ONLY here): /home/anon/Projects/th_08/specs/glm/
- Game dir: /home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/
- Ghidra helper: specs/ghidra_tools/gh.sh — ALWAYS SERIAL Ghidra calls.
- Extracted files: specs/glm/extracted/ ; artifact roots for other playfiles documented in 04_file_index.md

## CRITICAL LESSON (from this session)
- LZSS literals are 8 BITS read from the bitstream (MSB-first), NOT a raw byte.
  Reading a raw byte desyncs immediately (invisible for first ~9 bits: 1 flag + 8 bits ≈ byte-aligned start).
- Hopper decompile of sub_4740e0 was misleading; Ghidra + objdump call-sites are the truth.

## FORMAT (fully VERIFIED by extraction of all 317 files — do not re-derive)
- Container: th08.dat, magic "PBGZ". Header = 16 bytes: 4 magic + 12 encrypted.
- Header decrypt: FUN_43e1d0(ecx=src, edx=len, key=0x1b, delta=0x37, block=0x0c, guard=0x400).
  plaintext = interleave-reverse then xor with running key: dest[block-1-2i]=src[i]^key (ceil part),
  dest[block-2-2i]=src[block/2+i]^key (floor part); key += delta after EACH byte.
- Plain header = 3 LE u32: v0,v1,v2 → entry_count = v0 - 123456; table_offset = v1 - 345678; table_decompressed_size = v2 - 567891.
  th08.dat: count=317, table_offset=0x2CAA0C2 (=filesize-4167, i.e. table at EOF), tblsize=9063.
- Only first guard/block = 1024 bytes of each encrypted stream get the interleave-XOR; the rest is verbatim (guard budget).
  Verify by comparing cipher[1024:] == plain[1024:].
- Entry table: last table compressed bytes at [table_offset:EOF]; decrypt with (key=0x3e, delta=0x9b, block=0x80, guard=0x400) then LZSS-decompress (limit=table decompressed size... actually input limit = span=filesize-table_offset, out=alloc(tablesize)).
- Entry record (uncompressed table): asciz name (Shift-JIS) + u32 offset + u32 size + u32 flag.
  - offset = where compressed data starts in th08.dat
  - size = decompressed size
  - flag = ALL ZERO for all 317 entries in th08.dat (byte maybe reserved)
  - entries SORTED ascending by offset; entry i's COMPRESSED span = offsets[i+1] - offsets[i]
    (entries stored contiguously packed end to end, from 16 to table_offset!)
  - sentinel: entries[count].+4 = table_offset, +8 = 0 (used to clamp last span)
- Per-entry data: NO per-file crypt in th08; just LZSS compress. Decompress with FUN_4740e0(input span, out alloc size = entry.size).
- LZSS (FUN_4740e0): ring=0x2000 zero-init, pos starts 1; flag bit MSB-first per control byte: 1=literal (8 raw bits), 0=match: 13-bit absolute position in ring (0=END), 4-bit length, copy (len+3) bytes ring[(pos+k)&0x1fff] forward, each byte also written to ring.
- Verified: extraction of all 317 files to sizes == entry.size (count 317 = 317 success).
- Sizes sanity: first two illust sizes 405424 = ascii.anm, 1132 = capture.anm. th08.dat contains thbgm.fmt (1113 bytes at 44394438 uncompressed).

## Proven so far (addr/name verified in Ghidra project)
- FUN_43e1d0 = FUN_43e1d0 decrypt (interleave-reverse + XOR, guard-limited) — VERIFIED call site params (0x1b,0x37,0xc,0x400) and (0x3e,0x9b,0x80,0x400).
- FUN_4740e0 = LZSS decompress. FUN_474fa0 = entry-table parser (new-empty-record init FUN_4716e0/4745270).
- FUN_474c70 = find-first-matching-entry by name (case-insensitive __stricmp) over entry array.
- FUN_474af0 = Archive::Read(name, out=0?) → GlobalAlloc span; seek(entry.offset); read; LZSS(out, out.size=entry.size).
  NOTE: no per-entry crypt here.
- Archive object (486 bytes at this=ecx): [0]=entries base, [1]=entry count, [3]=stream interface ptr (vtable at 0x4b98b4).
  sort key: asciz name. table_offset reused as sentinel offset.
- call-site disasm at 0x474e7d..0x474eb4 checked: pushes = (0x400,0x80,0x9b,0x3e) for FUN_43e1d0;
  FUN_4740e0(ecx=decrypted, edx=span, pushed outsize, pushed 0).
- Stream vtable 0x4b98b4 fns: open/close/read(+8)/tell?/size(+0xc? hmm)/seek(+18)… verify by reading vtable refs.

## Status
- [x] th08.dat archive format: header + table + entries + LZSS. FULL extractor work.
- [x] extract_th08.py: single pass; extracts all 317 files (+ table); VERIFIED.
- [x] thbgm.fmt extracted (from archive) and parsed for BGM table (see 02_bgm.md).
- [x] thbgm.dat: raw PCM format (see 02_bgm.md). NOTE размер tracks чудовищный: check why (2ch 16bit 44100 320s=56MB→ but file is 449MB total so большой файл содержит много подряд).
- [x] ANM files inside th08.dat: magic "Form"/ANM_x format + lodging in THTX textures (see 03_images.md).
- [x] 00_overview.md, 01_archive.md, 02_bgm.md, 03_images.md, 04_file_index.md — complete.
- [x] tools/extract_th08.py — single-pass extractor, all 317 files, no per-entry crypt needed.
- [x] tools/thtx2png.py — THTX decode for all formats occurring in th08.dat ANM files.

## Next steps (Phase 1 close)
-주: PHASE1 DONE: all 5 deliverable specs written and extractor verified on real th08.dat.
- Phase 2 (not in scope here): scripts/opcodes, formats (ECL, STD, MSG, SHT) per later briefs.

## Open questions
- (none blocking Phase 1)

## Evidence addresses
- (see above; all previously noted Hopper addresses were CONFIRMED in Ghidra with same semantics —
  except LZSS literal reading subtlety and the per-entry crypt NON-existence in th08.)
- Ghidra project path: /home/anon/Projects/th_08/specs/glm/ghidra/th08.gpr
