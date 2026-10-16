# TH08 struct layouts (all offsets little-endian unless stated)

Confidence: VERIFIED = proven by shipped data and/or code; INFERRED = best explanation
so far. On-disk structs are packed.

## 1. PBGZ container (th08.dat) — VERIFIED

### 1.1 Header (file offset 0x00, 16 bytes)

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | char[4] | magic | `"PBGZ"` |
| 0x04 | 12 | obfuscated | hdr | cipher(k0=0x1B,k1=0x37,blk=12) → 3 u32: `{count+123456, table_off+345678, table_usize+567891}` |

### 1.2 File-table record (inside the LZSS'ed table) — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | var | char[] | name | NUL-terminated ASCII basename (no `/`) |
| —    | 4 | u32 | offset | absolute offset of the member's LZSS stream in th08.dat |
| —    | 4 | u32 | usize | size after LZSS (= decoded size + 4 for the edz tag) |
| —    | 4 | u32 | flags | always 0 in the shipped archive |

Extent = next.offset − offset; the last member ends at `table_off`.

### 1.3 edz payload — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 3 | char[3] | magic | `"edz"` |
| 0x03 | 1 | char | type | key selector: `M T A J E W - *` |
| 0x04 | — | obfuscated | payload | cipher(per-type keys from g_EdzKeys 0x4C78E0) |

### 1.4 edz key record (g_EdzKeys 0x004C78E0, 8 × 12 bytes on disk) — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 1 | u8 | k0 | initial XOR key (see 01_archive.md §2) |
| 0x01 | 1 | u8 | k1 | key increment |
| 0x02 | 2 | u16 | blk | block size |
| 0x04 | 4 | u32 | budget | bytes to transform |
| 0x08 | 4 | u32 | — | unused/padding (12-byte stride) |

Values: M(1B,37,0x40,0x2800) T(51,E9,0x40,0x3000) A(C1,51,0x1400,0x2000)
J(03,19,0x1400,0x7800) E(AB,CD,0x200,0x1000) W(12,34,0x400,0x2800)
-(35,97,0x80,0x2800) *(99,37,0x400,0x1000)

## 2. thbgm.dat — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | char[4] | magic | `"ZWAV"` |
| 0x04 | 4 | u32 | 1 | constant 1 |
| 0x08 | 4 | u32 | block | 0x800 (junk/alignment block) |
| 0x0C | 4 | u32 | 0 | zero |
| 0x10 | — | PCM | stream | 16-bit stereo 44100 Hz raw PCM for all tracks |

## 3. thbgm.fmt record (52 bytes × 21) — VERIFIED except +0x18

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 16 | char[16] | name | `"th08_XX.wav"` |
| 0x10 | 4 | u32 | offset | byte offset of the track in thbgm.dat |
| 0x14 | 4 | u32 | preload | bytes read up-front by Bgm_PreloadTrack |
| 0x18 | 4 | u32 | loop? | 4-aligned, < +0x1C — suspected loop start (see 02_bgm.md) |
| 0x1C | 4 | u32 | length | total track length in bytes (offsets+lengths sum to file size) |
| 0x20 | 16 | WAVEFORMATEX | fmt | PCM 2ch 44100 16-bit block-align 4 |
| 0x30 | 4 | u32 | 0 | zero |

## 4. BGM object g_Bgm 0x18B8A68 — VERIFIED (addresses from code)

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x1E80 | 4·21 | ptr[] | desc | fmt record pointers |
| 0x1EC0 | 4 | ptr | buffer | preloaded data buffer |
| 0x1F00 | 0x40 | byte[0x40] | buffer2 | second buffer |
| 0x1F40 | 4 | u32 | size | buffer size |
| 0x1F84 | 1109 | byte[] | fmt_raw | raw thbgm.fmt blob |
| 0x4108 | 0x100·i | str[0x100] | state[i] | per-track state (name string) |
| 0x5108 | 0x100 | char[0x100] | filename | current bgm file name |
| 0x5208 | 4 | ptr | player | DirectShow graph/player object |

## 5. ANM chunk — VERIFIED (structure), semantics in 03_images.md

### 5.1 Chunk header (0x3C bytes) at chunk offset 0

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | u32 | n_sprites | number of sprite records |
| 0x04 | 4 | u32 | n_scripts | number of script entries |
| 0x08 | 4 | u32 | 0 | zero |
| 0x0C | 4 | u32 | 256 | fixed-point scale (256 = 1.0) |
| 0x10 | 4 | u32 | 256 | fixed-point scale (256 = 1.0) |
| 0x14 | 4 | u32 | 5 | constant 5 |
| 0x18 | 4 | u32 | color | 0 or 0xFF000000 (chunk tint?) |
| 0x1C | 4 | u32 | name_off | offset of chunk name string (from chunk start) |
| 0x20 | 4 | u32 | 0 | zero |
| 0x24 | 4 | u32 | 0 | zero |
| 0x28 | 4 | u32 | version | always 3 in TH08 |
| 0x2C | 4 | u32 | 10 or 11 | sprite-format variant? |
| 0x30 | 4 | u32 | thtx_off | offset of embedded THTX (0 = none) |
| 0x34 | 4 | u32 | flags | low byte: texture-name flag ('@' = skip load) |
| 0x38 | 4 | u32 | next | delta to next chunk (0 = last) |

### 5.2 Sprite record (20 bytes, at each table offset)

Offsets are relative to the chunk start; the sprite table has `n_sprites` entries and
**index 0 is always NULL** (verified in all 310 chunks) — sprite ids are 1-based.
Records are laid out densely right after the chunk name string
(successive non-null offsets differ by exactly 20 in 1507/1507 cases).
Field semantics (u32 id + 4 floats) are documented in `03_images.md`.

### 5.4 Chunk statistics of the shipped archive — VERIFIED (tools/anm_check.py)

113 files, 310 chunks, 1607 sprite records, 1151 script-table entries,
306 embedded THTX textures (the 4 without a texture are the chunks named `"@"`,
whose flag byte +0x34 is 0; all texture chunks have flag low byte 1).
Chunks tile their file exactly: sum of `+0x38` deltas == file size.

### 5.3 Script table entry (8 bytes) — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | u32 | data_off | offset of the script data (from chunk start) |
| 0x04 | 4 | i32 | id | script id (may be negative; e.g. enemy 0..100, ascii -27..-16) |

### 5.5 Script region layout — VERIFIED (all 1151 script entries)

A script region (from its `data_off` to the next script's offset, or to the chunk's
THTX offset for the last script) consists of:

1. optional 20-byte sprite record `{u32 sprite_number; f32 x,y,w,h}` (same layout as
   §5.2). Present in 305 of 1151 regions; in 74 of them it is the whole region
   (no op stream). Its u32 is always < 65536, so it can never be mistaken for an op
   header (an op header's `size` would be 0 → invalid).
2. one or more op sequences, concatenated. Sequence = op records
   `{u16 op; u16 size}` where **size includes the 4-byte header** (payload = size−4
   bytes), terminated by `{u16 0xffff; u16 0x0000}` **followed by a u32 0 pad**
   (8 bytes total).
   Sequence counts: 1003 regions have 1 sequence, 74 have 2 (plausibly intro+loop,
   Phase 2), 74 have 0 (sprite-record only).

Byte-exact for all 1151 regions / 113 files (work/opcodes_scan.py, 0 failures).
Opcode list with *consistent* payload sizes: `03_images.md` §5;
machine-readable table `work/opcode_table.json`.

## 6. THTX texture — VERIFIED (all 306 embedded textures)

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | char[4] | magic | `"THTX"` |
| 0x04 | 2 | u16 | 0 | zero |
| 0x06 | 2 | u16 | fmt | pixel format: 1 = 32bpp (4 B/px), 3 = 16bpp (2 B/px), 5 = 16bpp (2 B/px) |
| 0x08 | 2 | u16 | w | width |
| 0x0A | 2 | u16 | h | height |
| 0x0C | 4 | u32 | size | size of the pixel block = w·h·bpp; equals the bytes left in the chunk minus 16 for all 306 textures |
| 0x10 | size | u8[] | pixels | pixel data (2 or 4 bytes per pixel) |

Format histogram over the shipped archive: fmt 1 ×170, fmt 3 ×32, fmt 5 ×104.
Bit layouts per fmt: see `03_images.md` (fmt 1 = 8888, fmt 5 = 4444-family).


## 7. D3D format descriptor (0x4C7948, 24-byte records) — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | u32 | d3dfmt | 0x16 X8R8G8B8, 0x15 A8R8G8B8, 0x18 X1R5G5B5, 0x17 R5G6B5, 0x19 A1R5G5B5, 0x1A A4R4G4B4 |
| 0x04 | 4 | u32 | bytes | bytes per pixel (0x20 / 0x10) |
| 0x08 | 4 | u32 | mask_a | alpha mask |
| 0x0C | 4 | u32 | mask_r | red mask |
| 0x10 | 4 | u32 | mask_g | green mask |
| 0x14 | 4 | u32 | mask_b | blue mask |

Terminated by `0xFFFFFFFF`.

## 8. msg*.dat — header only (Phase 2 for internals)

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | u32 | 2 | constant |
| 0x04 | 4 | u32 | 12 | constant |
| 0x08 | 4 | u32 | 0x510 | constant (offset?) |
| 0x0C | 4 | u32 | 0x4010000 | unknown |
| 0x10 | 2 | u16 | 0 | unknown |
| 0x12 | 2 | u16 | 0x100 | unknown |
| 0x14 | … | records | records begin with u16 0x003C | text encoding TBD |

## 9. .ecl header — VERIFIED (all 24 files)

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | u32 | magic | `0x00000800` in all 24 shipped files |
| 0x04 | 2 | u16 | n_sub | 43..78, plausibly the number of subroutines |
| 0x06 | 2 | u16 | n_sec | number of section offsets that follow |
| 0x08 | 4·n_sec | u32[] | sec_offsets | increasing file offsets of the following sections |
| 0x08+4·n_sec | 4 | u32 | end | equals the file size, i.e. the end of the last section |

Verified programmatically for all 24 files: magic, `n_sec`, strictly increasing
`sec_offsets`, and the extra u32 after the array equal to the file size.
Where the `n_sub` subroutine table lives is still open (Phase 2).


## 10. .std header (48 bytes visible) — INFERRED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 2 | u16 | a | per-stage (3 for stage1, 5 stage2, 2 stage3) |
| 0x02 | 2 | u16 | b | 46/47 or 4 |
| 0x04 | 4 | u32 | c | 2564 (st1) / 1352 (st3) |
| 0x08 | 4 | u32 | d | 3508 (st1) / 2248 (st3) |
| 0x0C | 4 | u32 | 0 | zero |
| 0x10 | 4 | u32 | 28004 | constant 0x6D64 in stage1/2/3… |
| 0x14… | | | zeros | |
| tail | | | 0xFFFFFFFF run | |

## 11. MIDI name table (0x4C7678, 20-byte records) — VERIFIED

| off | size | type | name | meaning |
|-----|------|------|------|---------|
| 0x00 | 4 | char* | name | e.g. `"th08_00.mid"` |
| 0x04 | 4 | u32 | ? | 0/1/2 |
| 0x08 | 4 | u32 | ? | 0/1 |
| 0x0C | 4 | u32 | ? | increasing small values (musiccmt line?) |
| 0x10 | 4 | u32 | track | Music Room track number |
