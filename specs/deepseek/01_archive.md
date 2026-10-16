# 01 — th08.dat archive format (PBGZ container + edz payload wrapper)

Target: `th08.dat` from Touhou 8: Imperishable Night (v1.00d, Japanese).
File identity (SHA-256):

```
th08.dat  9d7edf43b8ddd347cbb641836f6b5050745dd936f688daebbf9382ca557043bb
th08.exe  330fbdbf58a710829d65277b4f312cfbb38d5448b3df523e79350b879213d924
```

Everything in this document was derived from those two files plus static analysis of
`th08.exe` (Ghidra project `ghidra/th08.gpr`). Each claim is tagged VERIFIED (proven by
code and/or by round-tripping the shipped data) or INFERRED (consistent evidence, not
fully proven).

---

## 1. Container overview (VERIFIED)

`th08.dat` is a **PBGZ archive**. Layout:

```
0x0000  "PBGZ"                       magic
0x0004  12 obfuscated bytes          cipher block (k0=0x1B, k1=0x37, blk=12)
0x0010  (data region begins)         LZSS-compressed member payloads, one per file
 ...    table region                  at table_offset: cipher + LZSS'ed file table
EOF
```

Decoding the 12 header bytes with the cipher gives three little-endian u32 words, each
obfuscated by subtraction of a fixed constant:

| offset | stored | minus   | meaning |
|--------|--------|---------|---------|
| 0x04   | u32    | 123456  | file count = 317 |
| 0x08   | u32    | 345678  | file-table offset = 0x02CAA0C2 |
| 0x0C   | u32    | 567891  | decompressed table size = 9063 bytes |

(All three constants VERIFIED by re-deriving identical values from the shipped file.)

## 2. The block cipher (VERIFIED — `sub_43e1d0`, "Lib_BlockCipherDecode")

One routine is used for every obfuscated region (the 12-byte header, the file table, and
each `edz` payload) with different parameters: `k0`, `k1`, `blk` (block size) and a
`budget` (maximum number of bytes to actually transform).

Algorithm (exact):

```
n      = len(data)
tail   = ((n % blk < blk/4) ? (n % blk) : 0) + (n & 1)
body   = n - tail                    # only `body` bytes are ever transformed
k      = k0 (low byte)
src, dst = 0, 0
while body > 0 and budget > 0:
    b = min(blk, body)
    # pass A: odd 0-based positions, descending: b-1, b-3, b-5, ...
    for p = dst+b-1 down to dst step -2:
        out[p] = data[src] XOR k; k = (k + k1) & 0xFF; src += 1
    # pass B: even 0-based positions, descending: b-2, b-4, ...
    for p = dst+b-2 down to dst step -2:
        out[p] = data[src] XOR k; k = (k + k1) & 0xFF; src += 1
    body -= b; dst += b; budget -= b
out[dst:] = data[dst:]               # tail + anything the budget skipped: raw
```

Notes:
- The XOR key is a rolling byte starting at `k0`, incremented by `k1` after **every**
  byte (not per block); it keeps rolling across blocks.
- **The `budget` (4th key field) is a byte count of *data actually transformed*.**  Because
  it is small (e.g. 0x2000 for ANM), large members are only obfuscated in their first
  few KiB — verified: for `ascii.anm` (405 KB) only the first 0x2800 bytes differ from
  the raw stream; the rest is stored in the clear, and the resulting files parse
  structurally (113/113 ANM chunks tile their file exactly, `tools/anm_check.py`).
  The file-table call passes `0x400`, i.e. only the first 1024 bytes of the region are
  transformed; callers exploit this.

- There is no encryption — decoding is the same operation (XOR), so the same function
  encodes when fed plaintext.

## 3. The LZSS decompressor (VERIFIED — `sub_4740e0`, "Lib_LzssDecode")

Parameters (exact):

- Sliding ring buffer: **8 KiB** (`0x2000`), global at `g_LzssRing = 0x018D5F68`,
  initialised to zero once at startup and **never reset between calls**.
- Stream is a bitstream read **MSB-first** from each byte.
- Read 1 flag bit:
  - `1` → literal: read 8 bits, output the byte, append to ring.
  - `0` → match: read 13 bits = **absolute ring index** (not a distance), then 4 bits =
    match length − 3 (so lengths 3..18), copy that many bytes from the ring (wrapping),
    appending each copied byte to the ring.
- Ring write cursor starts at index **1** (not 0).
- A match with ring index **0** is the end marker: stop.
- Output stops at the requested uncompressed size; if the stream ends first, missing
  bits read as 0 without advancing the input pointer (engine behaviour).
- Although the engine keeps the ring between calls, the shipped data never depends on
  that: extracting all 317 members with the ring reset before every member yields
  byte-identical output (verified for every member).  A reimplementation may reset
  the ring per file.

## 4. File table (VERIFIED)

The region `th08.dat[table_offset : EOF]` is decoded as:

1. block cipher with `k0=0x3E, k1=0x9B, blk=0x80, budget=0x400`;
2. LZSS with expected output size `table_usize` (9063 bytes).

The decompressed table is a flat sequence of 317 records:

```
struct TableRecord {          // little-endian
    char  name[];             // NUL-terminated ASCII; '/' as directory separator
    u32   offset;             // absolute offset of the member's compressed data
    u32   uncompressed_size;  // exact size after LZSS
    u32   flags;              // always 0 in the shipped archive
};
```

- Records appear in `offset` order; each member's compressed extent is
  `next_record.offset - this.offset`. The sentinel "next offset" for the last record is
  `table_offset` itself (so the last member's extent runs up to the table).
- Member data (`th08.dat[offset : offset+extent]`) is **raw LZSS**, no cipher.
- All 317 members decompress to exactly their declared `uncompressed_size` (VERIFIED by
  extracting and checking every file).

## 5. edz payload wrapper (VERIFIED — `sub_43e390`, "Edz_Unwrap")

Each extracted member's bytes begin with a 4-byte ASCII tag:

```
'edz' <type-char>
```

The remaining bytes are **block-cipher-encoded** with per-type parameters; decoding them
yields the real file content (no further header is inserted or dropped):

```
payload = cipher_decode(member[4:], k0, k1, blk, budget)
```

Key table (in `th08.exe` at `g_EdzKeys = 0x004C78E0`, 8 records of 12 bytes each =
`{u8 k0, u8 k1, u16 blk, u32 budget}`), with the observed member extensions per type:

| type char | k0   | k1   | blk    | budget | used for (observed)                |
|-----------|------|------|--------|--------|------------------------------------|
| `M`       | 0x1B | 0x37 | 0x0040 | 0x2800 | (not used by shipped TH08 files)   |
| `T`       | 0x51 | 0xE9 | 0x0040 | 0x3000 | `musiccmt.txt` (only member with this tag) |
| `A`       | 0xC1 | 0x51 | 0x1400 | 0x2000 | `.anm` (113 files)                 |
| `J`       | 0x03 | 0x19 | 0x1400 | 0x7800 | `.jpg` (37 files)                  |
| `E`       | 0xAB | 0xCD | 0x0200 | 0x1000 | `.ecl` (24 files)                  |
| `W`       | 0x12 | 0x34 | 0x0400 | 0x2800 | `.wav` (39 files)                  |
| `-`       | 0x35 | 0x97 | 0x0080 | 0x2800 | `.dat .mid .std .sht .end .rpy .png .fmt .ver` |
| `*`       | 0x99 | 0x37 | 0x0400 | 0x1000 | (not used by shipped TH08 files)   |

Because `blk` is huge (e.g. 0x1400 for ANM) and budget likewise, essentially the whole
payload is transformed for typical file sizes. Files without an `edz` tag pass through
unchanged (none exist in TH08; the code path exists for `.mid` etc. in other games).

## 6. Code map (th08.exe)

| address    | name                   | role |
|------------|------------------------|------|
| 0x00474CE0 | Archive_Open           | PBGZ header/cipher/table parse |
| 0x00474AF0 | Archive_LoadFile       | per-member read + LZSS |
| 0x0043E1D0 | Lib_BlockCipherDecode  | the block cipher above |
| 0x004740E0 | Lib_LzssDecode         | LZSS above |
| 0x0043E390 | Edz_Unwrap             | edz tag + keyed decode |
| 0x0043E660 | File_LoadUniversal     | front-end loader used by everything (archive → unwrap → return buffer) |
| 0x018D5F68 | g_LzssRing             | 8 KiB sliding ring |
| 0x004C78E0 | g_EdzKeys              | edz key table |

## 7. Reference implementation

- `tools/pbgz.py` — container decode/extract/list (`python3 tools/pbgz.py extract th08.dat outdir`).
- `tools/edz.py` — wraps `pbgz.cipher_decode` for the payload layer.
- `tools/extract_th08.py` — **the deliverable single-pass extractor** (self-contained,
  standard library only). It reads `th08.dat`, walks the file table, LZSS-decompresses
  all 317 members and applies the `edz` unwrapping in one pass, writing the final
  plaintext files. Usage:

  ```
  python3 tools/extract_th08.py <th08.dat> <outdir>       # fully decoded files
  python3 tools/extract_th08.py <th08.dat> <outdir> --raw <rawdir>   # also keep raw members
  python3 tools/extract_th08.py <th08.dat> --list         # table only
  ```

  Verified: extracts all 317/317 members at exact sizes; the output is byte-identical
  to the previously validated `decoded/` tree (`diff -r` clean). Runtime ~45 s.
  The edz histogram it prints (`A:113, -:103, W:39, J:37, E:24, T:1`) matches §5.


## 8. Member name resolution (VERIFIED)

The engine asks the loader for names with a *virtual directory prefix*, for example
`bgm/thbgm.fmt`, `title/title00.png`, `sprt/musiccmt.txt`, `result/music.jpg`,
`demo/demorpy0.rpy`, `title/th08logo.jpg` etc.
(these literals live in `th08.exe` around 0x4B6500–0x4B967F).  The archive stores
only **flat basenames** — for every such request the matching member is the
one with everything up to the last `/` removed
(`bgm/thbgm.fmt` → `thbgm.fmt`, `demo/demorpy0.rpy` → `demorpy0.rpy`).
In TH08 no member name contains a `/`; the prefixes `bgm/`, `title/`, `sprt/`,
`result/`, `demo/` occur only in the *request* strings, never in the table.
All 16 path-qualified archive requests found in `th08.exe`
(`bgm/thbgm.fmt`, `bgm/init.mid`, `bgm/th08_01.mid`, `sprt/musiccmt.txt`,
`result/music.jpg`, `result/result.jpg`, `title/title00.png`, `title/select00.png`,
`title/th08logo.jpg`, `title/info00.jpg`, `title/info01.jpg`, `title/info02.jpg`,
`demo/demorpy0.rpy`…`demo/demorpy3.rpy`) resolve to existing flat members by taking
the basename, so the loader strips the directory part (it contains an strchr-like
helper, `Lib_StrFindChar` 0x004A48B0, called with '/' and '\\').
A reimplementation only needs the basename.
(Disk-file paths such as `./replay/th8_%.2d.rpy`, `./log.txt`, `./thbgm.dat`,
`snapshot/th%.3d.bmp` are resolved by the OS, not the archive.)


## 9. Full contained-file list (317 members)

`csize` = compressed extent in `th08.dat`, `usize` = size **after LZSS** (this includes
the 4-byte `edz?` tag, so the final decoded file is `usize - 4` bytes),
`edz` = payload type tag (`-` = generic key; see §5).

| # | name | offset | csize | usize | edz |
|---|------|--------|-------|-------|-----|
| 0 | `ascii.anm` | 0x10 | 129892 | 405424 | A |
| 1 | `capture.anm` | 0x1FB74 | 449 | 1132 | A |
| 2 | `demorpy0.rpy` | 0x1FD35 | 3986 | 3543 | - |
| 3 | `demorpy1.rpy` | 0x20CC7 | 3059 | 2720 | - |
| 4 | `demorpy2.rpy` | 0x218BA | 3471 | 3085 | - |
| 5 | `demorpy3.rpy` | 0x22649 | 3618 | 3215 | - |
| 6 | `ecldata1.ecl` | 0x2346B | 14357 | 45848 | E |
| 7 | `ecldata1sp.ecl` | 0x26C80 | 12912 | 39812 | E |
| 8 | `ecldata2.ecl` | 0x29EF0 | 15197 | 51248 | E |
| 9 | `ecldata2sp.ecl` | 0x2DA4D | 12391 | 37516 | E |
| 10 | `ecldata3.ecl` | 0x30AB4 | 15451 | 50244 | E |
| 11 | `ecldata3sp.ecl` | 0x3470F | 13469 | 41396 | E |
| 12 | `ecldata4a.ecl` | 0x37BAC | 15477 | 49856 | E |
| 13 | `ecldata4asp.ecl` | 0x3B821 | 13894 | 41880 | E |
| 14 | `ecldata4b.ecl` | 0x3EE67 | 16918 | 58692 | E |
| 15 | `ecldata4bsp.ecl` | 0x4307D | 15353 | 50252 | E |
| 16 | `ecldata5.ecl` | 0x46C76 | 14140 | 47228 | E |
| 17 | `ecldata5sp.ecl` | 0x4A3B2 | 11282 | 31188 | E |
| 18 | `ecldata6.ecl` | 0x4CFC4 | 17035 | 51508 | E |
| 19 | `ecldata6sp.ecl` | 0x5124F | 16652 | 50848 | E |
| 20 | `ecldata7.ecl` | 0x5535B | 21097 | 67328 | E |
| 21 | `ecldata7sp.ecl` | 0x5A5C4 | 21530 | 70812 | E |
| 22 | `ecldata8.ecl` | 0x5F9DE | 23688 | 90472 | E |
| 23 | `ecldata8sp.ecl` | 0x65666 | 20447 | 74460 | E |
| 24 | `ecldata_al.ecl` | 0x6A645 | 4353 | 6868 | E |
| 25 | `ecldata_rm.ecl` | 0x6B746 | 3897 | 5928 | E |
| 26 | `ecldata_sk.ecl` | 0x6C67F | 3908 | 6216 | E |
| 27 | `ecldata_yk.ecl` | 0x6D5C3 | 7317 | 24460 | E |
| 28 | `ecldata_ym.ecl` | 0x6F258 | 4039 | 7496 | E |
| 29 | `ecldata_yy.ecl` | 0x7021F | 5184 | 9604 | E |
| 30 | `eff01.anm` | 0x7165F | 61819 | 262644 | A |
| 31 | `eff02.anm` | 0x807DA | 107267 | 262668 | A |
| 32 | `eff03.anm` | 0x9AADD | 123085 | 262660 | A |
| 33 | `eff04a.anm` | 0xB8BAA | 113393 | 262664 | A |
| 34 | `eff04b.anm` | 0xD469B | 130535 | 262648 | A |
| 35 | `eff05.anm` | 0xF4482 | 173927 | 656012 | A |
| 36 | `eff06.anm` | 0x11EBE9 | 286210 | 918656 | A |
| 37 | `eff07.anm` | 0x1649EB | 601151 | 1836144 | A |
| 38 | `eff08.anm` | 0x1F762A | 819893 | 1836084 | A |
| 39 | `eff09al.anm` | 0x2BF8DF | 1362827 | 8520256 | A |
| 40 | `eff09rm.anm` | 0x40C46A | 72664 | 262824 | A |
| 41 | `eff09sk.anm` | 0x41E042 | 1381383 | 8520496 | A |
| 42 | `eff09yk.anm` | 0x56F449 | 144748 | 262864 | A |
| 43 | `eff09ym.anm` | 0x5929B5 | 109513 | 263072 | A |
| 44 | `eff09yy.anm` | 0x5AD57E | 128605 | 262648 | A |
| 45 | `end00.jpg` | 0x5CCBDB | 112192 | 99988 | J |
| 46 | `end00a.end` | 0x5E821B | 830 | 755 | - |
| 47 | `end00b.end` | 0x5E8559 | 2116 | 1951 | - |
| 48 | `end00b.jpg` | 0x5E8D9D | 118807 | 105798 | J |
| 49 | `end00b0.jpg` | 0x605DB4 | 126590 | 112727 | J |
| 50 | `end00b1.jpg` | 0x624C32 | 120098 | 106972 | J |
| 51 | `end00c.end` | 0x642154 | 4823 | 4640 | - |
| 52 | `end00c0.jpg` | 0x64342B | 122406 | 108979 | J |
| 53 | `end00c1.jpg` | 0x661251 | 122425 | 108995 | J |
| 54 | `end00c2.jpg` | 0x67F08A | 123436 | 109927 | J |
| 55 | `end01.jpg` | 0x69D2B6 | 116140 | 103503 | J |
| 56 | `end01a.end` | 0x6B9862 | 808 | 723 | - |
| 57 | `end01b.end` | 0x6B9B8A | 2628 | 2429 | - |
| 58 | `end01b0.jpg` | 0x6BA5CE | 121391 | 108117 | J |
| 59 | `end01b1.jpg` | 0x6D7FFD | 66923 | 59600 | J |
| 60 | `end01c.end` | 0x6E8568 | 4552 | 4394 | - |
| 61 | `end01c.jpg` | 0x6E9730 | 106208 | 94590 | J |
| 62 | `end01c0.jpg` | 0x703610 | 121135 | 107860 | J |
| 63 | `end01c1.jpg` | 0x720F3F | 118726 | 105711 | J |
| 64 | `end01c2.jpg` | 0x73DF05 | 120183 | 107021 | J |
| 65 | `end02.jpg` | 0x75B47C | 111138 | 99074 | J |
| 66 | `end02a.end` | 0x77669E | 846 | 763 | - |
| 67 | `end02b.end` | 0x7769EC | 2682 | 2517 | - |
| 68 | `end02b.jpg` | 0x777466 | 117775 | 104894 | J |
| 69 | `end02b0.jpg` | 0x794075 | 123126 | 109643 | J |
| 70 | `end02b1.jpg` | 0x7B216B | 121295 | 108019 | J |
| 71 | `end02c.end` | 0x7CFB3A | 4768 | 4620 | - |
| 72 | `end02c0.jpg` | 0x7D0DDA | 118097 | 105183 | J |
| 73 | `end02c1.jpg` | 0x7EDB2B | 123685 | 110136 | J |
| 74 | `end02c2.jpg` | 0x80BE50 | 122978 | 109494 | J |
| 75 | `end03.jpg` | 0x829EB2 | 110241 | 98238 | J |
| 76 | `end03a.end` | 0x844D53 | 910 | 821 | - |
| 77 | `end03b.end` | 0x8450E1 | 2443 | 2291 | - |
| 78 | `end03b.jpg` | 0x845A6C | 116372 | 103636 | J |
| 79 | `end03b0.jpg` | 0x862100 | 119929 | 106808 | J |
| 80 | `end03b1.jpg` | 0x87F579 | 120588 | 107375 | J |
| 81 | `end03c.end` | 0x89CC85 | 5346 | 5234 | - |
| 82 | `end03c0.jpg` | 0x89E167 | 119174 | 106139 | J |
| 83 | `end03c1.jpg` | 0x8BB2ED | 118475 | 105487 | J |
| 84 | `end03c2.jpg` | 0x8D81B8 | 118656 | 105673 | J |
| 85 | `enemy.anm` | 0x8F5138 | 143990 | 538292 | A |
| 86 | `etama.anm` | 0x9183AE | 210829 | 613784 | A |
| 87 | `face_al00.anm` | 0x94BB3B | 929863 | 1578292 | A |
| 88 | `face_alsp.anm` | 0xA2EB82 | 227524 | 524572 | A |
| 89 | `face_cdbg.anm` | 0xA66446 | 92653 | 525460 | A |
| 90 | `face_mr00.anm` | 0xA7CE33 | 915953 | 1578420 | A |
| 91 | `face_rm00.anm` | 0xB5C824 | 879284 | 1578292 | A |
| 92 | `face_rs00.anm` | 0xC332D8 | 819988 | 1578420 | A |
| 93 | `face_rssp.anm` | 0xCFB5EC | 218248 | 524572 | A |
| 94 | `face_sk00.anm` | 0xD30A74 | 892277 | 1578420 | A |
| 95 | `face_sksp.anm` | 0xE0A7E9 | 198563 | 524588 | A |
| 96 | `face_st01.anm` | 0xE3AF8C | 477188 | 952876 | A |
| 97 | `face_st01sp.anm` | 0xEAF790 | 222176 | 524572 | A |
| 98 | `face_st02.anm` | 0xEE5B70 | 618157 | 1084560 | A |
| 99 | `face_st02sp.anm` | 0xF7CA1D | 229826 | 524572 | A |
| 100 | `face_st03.anm` | 0xFB4BDF | 789461 | 1347928 | A |
| 101 | `face_st03sp.anm` | 0x10757B4 | 259690 | 524572 | A |
| 102 | `face_st04a.anm` | 0x10B4E1E | 811986 | 1348024 | A |
| 103 | `face_st04asp.anm` | 0x117B1F0 | 310864 | 524572 | A |
| 104 | `face_st04b.anm` | 0x11C7040 | 931535 | 1479724 | A |
| 105 | `face_st04bsp.anm` | 0x12AA70F | 328579 | 524572 | A |
| 106 | `face_st05.anm` | 0x12FAA92 | 724790 | 1871676 | A |
| 107 | `face_st05b.anm` | 0x13AB9C8 | 119021 | 265548 | A |
| 108 | `face_st05msp.anm` | 0x13C8AB5 | 311753 | 1048860 | A |
| 109 | `face_st05sp.anm` | 0x1414C7E | 205897 | 524572 | A |
| 110 | `face_st06.anm` | 0x14470C7 | 899059 | 2395988 | A |
| 111 | `face_st06sp.anm` | 0x15228BA | 312697 | 1048860 | A |
| 112 | `face_st07.anm` | 0x156EE33 | 1138599 | 2133232 | A |
| 113 | `face_st07sp.anm` | 0x1684DDA | 597848 | 1048860 | A |
| 114 | `face_st08.anm` | 0x1716D32 | 917919 | 2658744 | A |
| 115 | `face_st08m.anm` | 0x17F6ED1 | 347035 | 689620 | A |
| 116 | `face_st08msp.anm` | 0x184BA6C | 255365 | 524572 | A |
| 117 | `face_st08sp.anm` | 0x1889FF1 | 282152 | 1048860 | A |
| 118 | `face_yk00.anm` | 0x18CEE19 | 1013966 | 1578420 | A |
| 119 | `face_yksp.anm` | 0x19C66E7 | 308936 | 1048860 | A |
| 120 | `face_ym00.anm` | 0x1A11DAF | 863397 | 1578292 | A |
| 121 | `face_ymsp.anm` | 0x1AE4A54 | 214348 | 524572 | A |
| 122 | `face_yy00.anm` | 0x1B18FA0 | 946390 | 1578420 | A |
| 123 | `face_yysp.anm` | 0x1C00076 | 247662 | 524572 | A |
| 124 | `front.anm` | 0x1C3C7E4 | 118179 | 395984 | A |
| 125 | `info00.jpg` | 0x1C59587 | 188950 | 172231 | J |
| 126 | `info01.jpg` | 0x1C8779D | 185751 | 169379 | J |
| 127 | `info02.jpg` | 0x1CB4D34 | 189042 | 172300 | J |
| 128 | `init.mid` | 0x1CE2FA6 | 807 | 732 | - |
| 129 | `loading00.anm` | 0x1CE32CD | 257913 | 524512 | A |
| 130 | `loading00a.anm` | 0x1D22246 | 258868 | 524512 | A |
| 131 | `loading00h.anm` | 0x1D6157A | 238510 | 524512 | A |
| 132 | `loading01.anm` | 0x1D9B928 | 257812 | 524512 | A |
| 133 | `loading01a.anm` | 0x1DDA83C | 244756 | 524512 | A |
| 134 | `loading01h.anm` | 0x1E16450 | 244915 | 524512 | A |
| 135 | `loading02.anm` | 0x1E52103 | 223966 | 524512 | A |
| 136 | `loading02a.anm` | 0x1E88BE1 | 207943 | 524512 | A |
| 137 | `loading02h.anm` | 0x1EBB828 | 216005 | 524512 | A |
| 138 | `loading03.anm` | 0x1EF03ED | 247016 | 524512 | A |
| 139 | `loading03a.anm` | 0x1F2C8D5 | 256022 | 524512 | A |
| 140 | `loading03h.anm` | 0x1F6B0EB | 259250 | 524512 | A |
| 141 | `msg1a.dat` | 0x1FAA59D | 1850 | 1744 | - |
| 142 | `msg1b.dat` | 0x1FAACD7 | 1753 | 1640 | - |
| 143 | `msg1c.dat` | 0x1FAB3B0 | 1730 | 1604 | - |
| 144 | `msg1d.dat` | 0x1FABA72 | 2249 | 2136 | - |
| 145 | `msg2a.dat` | 0x1FAC33B | 2439 | 2288 | - |
| 146 | `msg2b.dat` | 0x1FACCC2 | 1973 | 1868 | - |
| 147 | `msg2c.dat` | 0x1FAD477 | 1853 | 1744 | - |
| 148 | `msg2d.dat` | 0x1FADBB4 | 1813 | 1708 | - |
| 149 | `msg3a.dat` | 0x1FAE2C9 | 3326 | 3336 | - |
| 150 | `msg3b.dat` | 0x1FAEFC7 | 3296 | 3228 | - |
| 151 | `msg3c.dat` | 0x1FAFCA7 | 3546 | 3512 | - |
| 152 | `msg3d.dat` | 0x1FB0A81 | 3123 | 3072 | - |
| 153 | `msg4ab.dat` | 0x1FB16B4 | 2920 | 2900 | - |
| 154 | `msg4ac.dat` | 0x1FB221C | 3548 | 3536 | - |
| 155 | `msg4ba.dat` | 0x1FB2FF8 | 3109 | 3008 | - |
| 156 | `msg4bd.dat` | 0x1FB3C1D | 2849 | 2768 | - |
| 157 | `msg4dm.dat` | 0x1FB473E | 315 | 284 | - |
| 158 | `msg5a.dat` | 0x1FB4879 | 3748 | 3684 | - |
| 159 | `msg5b.dat` | 0x1FB571D | 4873 | 4788 | - |
| 160 | `msg5c.dat` | 0x1FB6A26 | 4340 | 4272 | - |
| 161 | `msg5d.dat` | 0x1FB7B1A | 4374 | 4276 | - |
| 162 | `msg6a.dat` | 0x1FB8C30 | 3671 | 3676 | - |
| 163 | `msg6b.dat` | 0x1FB9A87 | 3804 | 3720 | - |
| 164 | `msg6c.dat` | 0x1FBA963 | 4507 | 4596 | - |
| 165 | `msg6d.dat` | 0x1FBBAFE | 4621 | 4676 | - |
| 166 | `msg7a.dat` | 0x1FBCD0B | 3677 | 3616 | - |
| 167 | `msg7b.dat` | 0x1FBDB68 | 3979 | 3888 | - |
| 168 | `msg7c.dat` | 0x1FBEAF3 | 4600 | 4536 | - |
| 169 | `msg7d.dat` | 0x1FBFCEB | 4310 | 4368 | - |
| 170 | `msg8a.dat` | 0x1FC0DC1 | 5811 | 5988 | - |
| 171 | `msg8b.dat` | 0x1FC2474 | 6495 | 6616 | - |
| 172 | `msg8c.dat` | 0x1FC3DD3 | 5955 | 6080 | - |
| 173 | `msg8d.dat` | 0x1FC5516 | 6556 | 6820 | - |
| 174 | `music.jpg` | 0x1FC6EB2 | 161943 | 147935 | J |
| 175 | `music00.anm` | 0x1FEE749 | 23371 | 138864 | A |
| 176 | `musiccmt.txt` | 0x1FF4294 | 7826 | 7431 | T |
| 177 | `nowloading.anm` | 0x1FF6126 | 10801 | 33704 | A |
| 178 | `phantasm.jpg` | 0x1FF8B57 | 75601 | 74660 | J |
| 179 | `player00.anm` | 0x200B2A8 | 80189 | 266768 | A |
| 180 | `player01.anm` | 0x201EBE5 | 70667 | 269336 | A |
| 181 | `player02.anm` | 0x202FFF0 | 35736 | 135012 | A |
| 182 | `player03.anm` | 0x2038B88 | 32969 | 134448 | A |
| 183 | `ply00a.sht` | 0x2040C51 | 1121 | 1476 | - |
| 184 | `ply00as.sht` | 0x20410B2 | 1077 | 1420 | - |
| 185 | `ply01a.sht` | 0x20414E7 | 1075 | 1364 | - |
| 186 | `ply01as.sht` | 0x204191A | 789 | 804 | - |
| 187 | `ply02a.sht` | 0x2041C2F | 1204 | 1588 | - |
| 188 | `ply02as.sht` | 0x20420E3 | 2241 | 3572 | - |
| 189 | `ply03a.sht` | 0x20429A4 | 1046 | 1308 | - |
| 190 | `ply03as.sht` | 0x2042DBA | 1689 | 2260 | - |
| 191 | `result.jpg` | 0x2043453 | 346479 | 312714 | J |
| 192 | `result00.anm` | 0x2097DC2 | 198074 | 952984 | A |
| 193 | `resulttext.anm` | 0x20C837C | 351 | 828 | A |
| 194 | `select00.png` | 0x20C84DB | 409419 | 364754 | - |
| 195 | `se_bonus.wav` | 0x212C426 | 50181 | 45340 | W |
| 196 | `se_bonus2.wav` | 0x213882B | 49540 | 45340 | W |
| 197 | `se_border.wav` | 0x21449AF | 147251 | 132432 | W |
| 198 | `se_cancel00.wav` | 0x21688E2 | 13067 | 13656 | W |
| 199 | `se_cardget.wav` | 0x216BBED | 241672 | 222868 | W |
| 200 | `se_cat00.wav` | 0x21A6BF5 | 87191 | 84264 | W |
| 201 | `se_damage00.wav` | 0x21BC08C | 7510 | 6776 | W |
| 202 | `se_damage01.wav` | 0x21BDDE2 | 27854 | 27032 | W |
| 203 | `se_enep00.wav` | 0x21C4AB0 | 128938 | 115588 | W |
| 204 | `se_enep01.wav` | 0x21E425A | 204390 | 230304 | W |
| 205 | `se_extend.wav` | 0x22160C0 | 67103 | 70192 | W |
| 206 | `se_graze.wav` | 0x22266DF | 5103 | 4546 | W |
| 207 | `se_gun00.wav` | 0x2227ACE | 95604 | 134698 | W |
| 208 | `se_invalid.wav` | 0x223F042 | 64741 | 62954 | W |
| 209 | `se_item00.wav` | 0x224ED27 | 5064 | 7382 | W |
| 210 | `se_item01.wav` | 0x22500EF | 11573 | 10856 | W |
| 211 | `se_kira00.wav` | 0x2252E24 | 24251 | 35226 | W |
| 212 | `se_kira01.wav` | 0x2258CDF | 20743 | 21944 | W |
| 213 | `se_kira02.wav` | 0x225DDE6 | 26464 | 25786 | W |
| 214 | `se_lazer00.wav` | 0x2264546 | 24633 | 22186 | W |
| 215 | `se_lazer01.wav` | 0x226A57F | 22073 | 24186 | W |
| 216 | `se_nep00.wav` | 0x226FBB8 | 226187 | 224930 | W |
| 217 | `se_ok00.wav` | 0x22A6F43 | 11921 | 10880 | W |
| 218 | `se_ophide.wav` | 0x22A9DD4 | 17365 | 15456 | W |
| 219 | `se_opshow.wav` | 0x22AE1A9 | 21156 | 18830 | W |
| 220 | `se_option.wav` | 0x22B344D | 29747 | 26508 | W |
| 221 | `se_pause.wav` | 0x22BA880 | 141423 | 133680 | W |
| 222 | `se_pldead00.wav` | 0x22DD0EF | 60207 | 60856 | W |
| 223 | `se_plst00.wav` | 0x22EBC1E | 4010 | 3600 | W |
| 224 | `se_power0.wav` | 0x22ECBC8 | 59174 | 73008 | W |
| 225 | `se_power1.wav` | 0x22FB2EE | 29323 | 30416 | W |
| 226 | `se_powerup.wav` | 0x2302579 | 83512 | 77282 | W |
| 227 | `se_select00.wav` | 0x2316BB1 | 1295 | 1150 | W |
| 228 | `se_slash.wav` | 0x23170C0 | 57480 | 54786 | W |
| 229 | `se_tan00.wav` | 0x2325148 | 14751 | 15104 | W |
| 230 | `se_tan01.wav` | 0x2328AE7 | 12904 | 15104 | W |
| 231 | `se_tan02.wav` | 0x232BD4F | 11669 | 11712 | W |
| 232 | `se_timeout.wav` | 0x232EAE4 | 21669 | 19394 | W |
| 233 | `se_timeout2.wav` | 0x2333F89 | 21748 | 19392 | W |
| 234 | `staff00.end` | 0x233947D | 487 | 432 | - |
| 235 | `staff00.jpg` | 0x2339664 | 45334 | 41842 | J |
| 236 | `staff00b.end` | 0x234477A | 443 | 393 | - |
| 237 | `staff00b.jpg` | 0x2344935 | 36113 | 33563 | J |
| 238 | `staff01.anm` | 0x234D646 | 168140 | 788028 | A |
| 239 | `stage1.std` | 0x2376712 | 3002 | 4432 | - |
| 240 | `stage1_s.std` | 0x23772CC | 2503 | 3852 | - |
| 241 | `stage2.std` | 0x2377C93 | 3310 | 6360 | - |
| 242 | `stage2_s.std` | 0x2378981 | 2927 | 5920 | - |
| 243 | `stage3.std` | 0x23794F0 | 1582 | 2832 | - |
| 244 | `stage3_s.std` | 0x2379B1E | 1343 | 2512 | - |
| 245 | `stage4a.std` | 0x237A05D | 3735 | 5336 | - |
| 246 | `stage4a_s.std` | 0x237AEF4 | 2677 | 4016 | - |
| 247 | `stage4b.std` | 0x237B969 | 3735 | 5336 | - |
| 248 | `stage4b_s.std` | 0x237C800 | 2677 | 4016 | - |
| 249 | `stage5.std` | 0x237D275 | 2987 | 4520 | - |
| 250 | `stage5_s.std` | 0x237DE20 | 2231 | 3640 | - |
| 251 | `stage6.std` | 0x237E6D7 | 2434 | 3808 | - |
| 252 | `stage6_s.std` | 0x237F059 | 1602 | 2828 | - |
| 253 | `stage7.std` | 0x237F69B | 2392 | 3732 | - |
| 254 | `stage7_s.std` | 0x237FFF3 | 1711 | 2932 | - |
| 255 | `stage8.std` | 0x23806A2 | 3734 | 5356 | - |
| 256 | `stage8_s.std` | 0x2381538 | 2680 | 4016 | - |
| 257 | `stg1bg.anm` | 0x2381FB0 | 262854 | 1048940 | A |
| 258 | `stg1enm.anm` | 0x23C2276 | 33009 | 132852 | A |
| 259 | `stg1txt.anm` | 0x23CA367 | 48159 | 262984 | A |
| 260 | `stg2bg.anm` | 0x23D5F86 | 402197 | 1048940 | A |
| 261 | `stg2enm.anm` | 0x243829B | 32631 | 132756 | A |
| 262 | `stg2txt.anm` | 0x2440212 | 48922 | 262984 | A |
| 263 | `stg3bg.anm` | 0x244C12C | 365314 | 788832 | A |
| 264 | `stg3enm.anm` | 0x24A542E | 37495 | 132756 | A |
| 265 | `stg3txt.anm` | 0x24AE6A5 | 48842 | 262984 | A |
| 266 | `stg4abg.anm` | 0x24BA56F | 678764 | 1573856 | A |
| 267 | `stg4aenm.anm` | 0x25600DB | 41279 | 134260 | A |
| 268 | `stg4atxt.anm` | 0x256A21A | 49313 | 262984 | A |
| 269 | `stg4benm.anm` | 0x25762BB | 52665 | 268636 | A |
| 270 | `stg4btxt.anm` | 0x2583074 | 50068 | 262984 | A |
| 271 | `stg5bg.anm` | 0x258F408 | 559181 | 1049508 | A |
| 272 | `stg5enm.anm` | 0x2617C55 | 59433 | 266336 | A |
| 273 | `stg5txt.anm` | 0x262647E | 49434 | 263020 | A |
| 274 | `stg6bg.anm` | 0x2632598 | 1481550 | 2886088 | A |
| 275 | `stg6enm.anm` | 0x279C0E6 | 34099 | 132908 | A |
| 276 | `stg6txt.anm` | 0x27A4619 | 44288 | 263044 | A |
| 277 | `stg7bg.anm` | 0x27AF319 | 578502 | 2361984 | A |
| 278 | `stg7enm.anm` | 0x283C6DF | 61339 | 265604 | A |
| 279 | `stg7txt.anm` | 0x284B67A | 48863 | 263044 | A |
| 280 | `stg8bg.anm` | 0x2857559 | 397080 | 1573836 | A |
| 281 | `stg8enm.anm` | 0x28B8471 | 106370 | 397536 | A |
| 282 | `stg8txt.anm` | 0x28D23F3 | 50510 | 263008 | A |
| 283 | `stgenm_al.anm` | 0x28DE941 | 25902 | 132760 | A |
| 284 | `stgenm_rm.anm` | 0x28E4E6F | 26929 | 131800 | A |
| 285 | `stgenm_sk.anm` | 0x28EB7A0 | 25537 | 132688 | A |
| 286 | `stgenm_yk.anm` | 0x28F1B61 | 132178 | 527792 | A |
| 287 | `stgenm_ym.anm` | 0x2911FB3 | 48469 | 133084 | A |
| 288 | `stgenm_yy.anm` | 0x291DD08 | 496741 | 1050200 | A |
| 289 | `text.anm` | 0x299716D | 4708 | 7052 | A |
| 290 | `th08logo.jpg` | 0x29983D1 | 332214 | 299731 | J |
| 291 | `th08_00.mid` | 0x29E9587 | 12854 | 27035 | - |
| 292 | `th08_01.mid` | 0x29EC7BD | 11945 | 13956 | - |
| 293 | `th08_0100d.ver` | 0x29EF666 | 406 | 360 | - |
| 294 | `th08_03.mid` | 0x29EF7FC | 16651 | 31176 | - |
| 295 | `th08_04.mid` | 0x29F3907 | 30737 | 74729 | - |
| 296 | `th08_05.mid` | 0x29FB118 | 18881 | 37813 | - |
| 297 | `th08_06.mid` | 0x29FFAD9 | 18058 | 31766 | - |
| 298 | `th08_07.mid` | 0x2A04163 | 24230 | 49084 | - |
| 299 | `th08_08.mid` | 0x2A0A009 | 16308 | 36450 | - |
| 300 | `th08_09.mid` | 0x2A0DFBD | 31586 | 93975 | - |
| 301 | `th08_10.mid` | 0x2A15B1F | 19300 | 49417 | - |
| 302 | `th08_11.mid` | 0x2A1A683 | 29321 | 65368 | - |
| 303 | `th08_12.mid` | 0x2A2190C | 20313 | 43948 | - |
| 304 | `th08_13.mid` | 0x2A26865 | 16045 | 32181 | - |
| 305 | `th08_13b.mid` | 0x2A2A712 | 15560 | 32281 | - |
| 306 | `th08_14.mid` | 0x2A2E3DA | 31400 | 75940 | - |
| 307 | `th08_15.mid` | 0x2A35E82 | 26523 | 71003 | - |
| 308 | `th08_16.mid` | 0x2A3C61D | 11270 | 13514 | - |
| 309 | `th08_17.mid` | 0x2A3F223 | 18439 | 40614 | - |
| 310 | `th08_18.mid` | 0x2A43A2A | 22817 | 64843 | - |
| 311 | `th08_19.mid` | 0x2A4934B | 40993 | 97795 | - |
| 312 | `th08_20.mid` | 0x2A5336C | 13402 | 27056 | - |
| 313 | `thbgm.fmt` | 0x2A567C6 | 1198 | 1113 | - |
| 314 | `times.anm` | 0x2A56C74 | 29881 | 132428 | A |
| 315 | `title00.png` | 0x2A5E12D | 713624 | 635002 | - |
| 316 | `title01.anm` | 0x2B0C4C5 | 1694717 | 7386448 | A |
| | **total** | | **46833842** | **112517344** | |
