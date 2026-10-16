# NOTES.md — glm agent, Phase 1 (th08 reverse engineering)

## Resume checklist
- Re-read this + 01_archive.md after compaction. SPEC_DIR = /home/anon/Projects/th_08/specs/glm (ONLY).
- Game dir: /home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/
- Ghidra helper: specs/ghidra_tools/gh.sh (SERIAL). Python verify tools under tools/.
- Extracted 317 files in extracted/ — sizes all == entry.size, but read on for post-LZ wrapper layer.

## Debug lessons (do not repeat)
- LZSS literal = 8 bits MSB-first read from the bitstream, NOT a raw byte.
- Ring 0x2000 zeroed, write pos starts 1, match src = ring[(p+k)&0x1fff]; end on pos==0 or input limit.
- LZSS input limit = span (next-offset delta), NOT the table size.
- VERIFY claims by running code, not by inventing test runs. (Several phantom memories were caught and fixed.)
- Archive object layouts used by this game when MULTIPLE archives are open — don't assume one global.

## th08.dat container — VERIFIED (extractor at tools/extract_th08.py, un-merges (–) extraction OK)
- header "PBGZ" + 12-byte crypt → count=317, table_offset=0x2CAA0C2 (EOF-4167), tblSize=9063.
- entry table decrypt = FUN_43e1d0 key 0x3e delta 0x9b block 0x80 guard 0x400 → only FIRST 1024 bytes
  change (guard truncated XOR); rest verbatim. Then LZSS → 9063 bytes.
- entry record: asciz name + u32 offset + u32 size + u32 extra (extra=0..3; NOT a crypto key so far).
- span = next.offset - offset. LZSS span → entry.size bytes. NO per-file ZUN-crypt. ✓ 317/317  s izes match.

## LV.2: post-LZ wrapper on EVERY file ("edz" prefix!)
- extracted/ascii.anm starts 65 64 7a XX: 0x65='e' 0x64='d' 0x7a='z' constant; 4th byte varies per family:
  .anm→0x41, else→0x2d, .wav→0x57, .jpg→0x4a, .mid→0x2d, musiccmt.txt→0x54 (only one .txt).
- Format probe WITHOUT assuming a string label:
  - title00.png after un-xor'(s,o,0x7f)(6) +"\xd3\x02\x00"?  (better: on-the-fly checks below)
  - LZ literal values = RAW bytes when pos<=1 for any .anm (i.e. those 6 bytes are not LZ literals: they
    are the XOR key bytes themselves!):  title00.png key = 0x11 0x17 0x1d 0x1d; se_tan00.wav same key.
  - .anm files: header "Wra"+T883? synch via TH88 after removing the 0x65 0x64 0x7a prefix (constant "edZ"
    or "edz"?? probably "edz" lowercase e.d.z = "Wra 201708"? Actually the exact meaning is that the .anm
    portion itself starts with byte 0xA1-cd? no confirmed yet (see anm notes below).
- Wrapper layout (from FUN_43e390 = ReadFileWrapper):
    +0 magic byte2 checks against 0x4c7940 (0x1f), 0x4c7941 (0x43), 0x4c7942 (1f)? — must equal after sub 0x20/0x40/0x60!";
    else: SANITY — probe = first byte == 'e' 0x65 etc. "edz" = 0 codes at [4c7940]=0x45? ("E"?) — CHECK DATA:
    the wrapper is a PER-FILE jump table entry = type byte (=> 12-byte code+key selector?).
    magic /Assembler: pos 0..2 are constants (0x1f/0x1a/0x1a?) → 0x65^? etc.
- Xref: FUN_43e660 (read file by name) → after FUN_474af0 LZ-compress decode:
    push archive_obj, push 0x1c  (0x1c = 28 = "#l"?)   ← wrapper body len 0x1c? then call 0x43e390.
  0x43e390 parses first 3 bytes (0x4c7940 deltas 0x20/0x40/0x60) = type tag; then entry:
    type 'A' (0x41): body BFAB' (wrapper BFAB) OK.'.
- to pin down type ids from 0x4c7940: dump 0x4c7940..0x4c7960 bytes from th08.exe. DO IT NEXT SESSION.

## Status
- [x] archive format & extractor (all 317 files, sizes match): tools/extract_th08.py VERIFIED.
- [ ] post-LZ wrapper ("edz" layer) per file: type byte meanings + wrapper body — IN PROGRESS.
- [ ] THTX/.ANM/.fmt/BGM analysis for 03_images.md, 02_bgm.md.
- [ ] 00_overview.md, 04_file_index.md.

## Todo (resume here)
1. dump bytes at 0x4c7940 (th08.exe file offsets: VA-0x4015c0? check map) to identify the wrapper types.
2. read the post-LZ wrapper's own header (0x1c bytes?) then decide: strip wrapper, THEN treat as THTX/anm/etc.
3. extract first bytes of wrapper to find the actual payload layout; skip/strip edz 3 bytes then treat rest as typical exe-loaded data.
