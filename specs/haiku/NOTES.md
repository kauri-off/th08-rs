# Haiku dirty-room notes (Phase 1)

## RULE (coordinator, effective 2026-10-17)
- NO web search / web fetch. NO reliance on remembered third-party knowledge (thtk, thpatch, touhou-doc, community decompilations).
- Every fact must come from th08.exe (REA/Ghidra) or the game data files, and be verified against them.
- Two early WebSearch calls returned nothing usable (PBG3 docs only, no PBGZ); nothing in specs is based on them.

## Toolchain (decided 2026-10-17)
- REA MCP present; `ghidra` provider verified working: `open_binary(th08.exe, provider_id=ghidra)` -> PE x86, sha256 330fbdbf…d924, 2409 procedures, 1007 strings.
- Primary decompile/xref tool: REA (ephemeral import of th08.exe; annotations session-scoped).
- Persistent names: `symbols.csv` applied to `<SPEC_DIR>/ghidra/th08.gpr` via `specs/ghidra_tools/gh.sh` (ApplySymbols). Serial runs only.
- Hopper not used.

## Target facts
- th08.dat 46,838,025 bytes; starts with ASCII `PBGZ`. thbgm.dat 449,961,024 bytes; starts with ASCII `ZWAV`.
- th08.exe 840,704 bytes. Game dir is read-only for this agent. Never write there.

## Binary facts so far (from decompile of th08.exe; NOT yet verified against th08.dat)
- Magic `PBGZ` (LE dword 0x5a474250) is checked in `FUN_00474ce0` (archive open). Its hit for the bytes is at 0x474d4b.
- Header: read 12 bytes, then de-scrambled by `FUN_0043e1d0(key=0x1b, step=0x37, block=12, limit=0x400)`.
  Three LE dwords (h0,h1,h2) then give: count' = h0-123456, dataBase = h1-345678, decLen = h2-567891.
- `FUN_0043e1d0(key,step,block,limit)` = keyed block permutation + XOR: per block of `block` bytes, input bytes
  go (XOR key, key += step per byte) to output positions block-1, block-3, ... (first ceil(n/2)), then block-2, block-4, ... (floor(n/2)).
  Only first `limit` bytes (in units of block) are processed; the remainder is copied verbatim. (derived from pseudocode; to verify)
- Table section: data from dataBase to EOF is read, de-scrambled with `FUN_0043e1d0(0x3e, 0x9b, 128, 0x400)`, then decompressed.
- Decompressor `FUN_004740e0` (LZSS, derived): MSB-first bit reader; bit 1 = literal (8 bits); bit 0 = match:
  12-bit offset (0 = end of stream), 8-bit length, copy length+3 bytes from window of 8192 (index & 0x1fff). Window starts at pos 1.
- Table parser `FUN_00474fa0(buf, count, base)`: builds count entries of 4 dwords each; entry[i][1..3] read via FUN_00475200 after FUN_00475220 (encoding still to read).


## VERIFIED archive chain (2026-10-17, tools/extract_th08.py, 317/317 files, all signatures OK)
1. Container header: bytes 4..15 (after magic PBGZ) -> descramble(key=0x1B, step=0x37, block=12, limit=0x400) -> dwords h0,h1,h2:
   count=h0-123456, tableBase=h1-345678, tableLen=h2-567891.
2. Table: bytes [tableBase..EOF] -> descramble(0x3E,0x9B,128,0x400) -> LZSS -> tableLen bytes (rest of table = zero padding).
   Records: name\0 + u32 start + u32 usz + u32 flags(always 0). Entry i covers [start_i, start_{i+1}) with start_{n}=tableBase.
3. Entry data: LZSS(raw chunk) -> usz bytes = 4-byte tag 'edz'+X, then body.
4. Tag X selects one of 8 param sets: X in {M,T,A,J,E,W,-,*} (index 0..7). th08.dat only uses A,T,J,E,W,- (indices 2,1,3,4,5,6).
5. Body = descramble(body, key, step, block, limit) with the set's params (body = bytes after the 4-byte tag).
- descramble(FUN_0043e1d0): per block of `block` bytes, input bytes (xor key, key+=step per byte) go to out pos (n-1,n-3,...) then (n-2,n-4,...);
  tail (len%block if < block/4, plus len&1) copied verbatim; only `limit` bytes processed.
- LZSS (FUN_004740e0): MSB-first bits; 1=literal(8 bits); 0=match: 13-bit offset (0=EOS), 4-bit len+3; 8 KiB window, start pos 1.
- Set params (key,step,block,limit): M:(1B,37,64,10240) T:(51,E9,64,12288) A:(C1,51,5120,8192) J:(03,19,5120,30720) E:(AB,CD,512,4096) W:(12,34,1024,10240) -:(35,97,128,10240) *:(99,37,1024,4096)
- Tag check constants: first 3 bytes must be 'e','d','z'. Expected tag per set i = (byte0_i - 16*i - 16)&0xFF, where byte0 are 0x5d,74,71,8a,95,b7,9d,aa -> tags M,T,A,J,E,W,-,*.
- Validated: PNG/JFIF/RIFF(size=body-8)/MThd/Shift-JIS text all decode; sum of usz = 112,517,344 = extracted total + 317*4.
- Extracted output: specs/haiku/extracted/ (109 MB, not for git).


## VERIFIED BGM facts (2026-10-17)
- thbgm.dat: 16-byte header dwords = 'ZWAV'(0x5641575a), 1, 0x800 (validated by FUN_004477b1, which also reads th08.cfg). Track data is raw PCM: 16-bit stereo 44100 Hz (WAVEFORMATEX in fmt).
- bgm/thbgm.fmt (inside th08.dat, 1109 bytes) = 21 records x 52 bytes: name[16] (NUL-padded) | dw0 | dw1 | dw2 | dw3 | WAVEFORMATEX[16] | pad[4]=0.
  Streamer (FUN_00473250/4733e0/473530/472d30, verified by decompile):
  * dw0 = data start offset in thbgm.dat (struct +0x10). Tracks are contiguous: dw0[i+1] = dw0[i]+dw3[i]; last end = file size (VERIFIED).
  * dw3 = data length in bytes (struct +0x1c), the remaining-byte counter; read raw with ReadFile.
  * dw2 = loop start in bytes from data start (struct +0x18). On wrap: seek to dw0+dw2 and continue (loop only if dw2 > 0; dw2==0 would restart at data start).
  * Loop end = data end (dw0+dw3). Frame = 4 bytes (blockAlign), all dw2/dw3/dw0 are multiples of 4 (VERIFIED for all 21).
  * dw1 (struct +0x14): never read on the streaming path; its value is larger than dw3 for every track (dw1 != dw3 in 21/21). Meaning UNKNOWN (INFERRED unused).
  * Streaming: DirectSound ring buffer, notify events per block, BGM thread calls HandleWaveStreamNotification(1) => loop is always on for BGM.
- Track names th08_00..th08_20 incl. th08_13b. Mapping track -> stage/music ID: TODO.

## THTX / ANM (in progress)
- THTX chunks: 110 ANM files, each chunk magic 'THTX' at chunk start. The exe has no 'THTX' constant (search 54 48 54 58 = 0 hits), so the loader locates chunks by offset.
- Header fields after magic (LE): u32 A (0x10000/0x30000/0x50000), u16 w, u16 h, u16 B (0x0000/0x8000), u16 C (0,2,4,8,16,128). Pixel data starts +16 (data size meaning TBD).
- Format of pixel data NOT YET DERIVED. Plan: find the texture creation path in th08.exe (not yet found), else infer from data with a smoothness test.

## Status
- [ ] 00_overview.md
- [ ] 01_archive.md + tools/extract_th08.py
- [ ] 02_bgm.md
- [ ] 03_images.md + tools/thtx2png.py
- [ ] 04_file_index.md
- [ ] symbols.csv / structs.md updated

## Log
- Toolchain check done.
- Decoded reader flow of FUN_00474ce0 (header, table, LZSS) from pseudocode.
- Next: read FUN_00475150/00475200/00475220 (entry field encoding), confirm register inputs of the FUN_0043e1d0 calls via listing, then implement and check against th08.dat.
