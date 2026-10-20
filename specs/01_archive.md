# 01 — th08.dat archive format ("PBGZ")

All facts derived from th08.exe (functions listed in `symbols.csv`) and verified by
`tools/th08dat.py`, which unpacks all 317 entries of the shipped th08.dat into
`extracted/dat/`; every JPEG/PNG/RIFF/MIDI file decodes with a valid signature,
and every entry's decoded size equals the table size minus 4. **VERIFIED** unless marked otherwise.

## 1. Processing pipeline

```
th08.dat ─┬─ header (16 B)      ─ XOR-cipher ──► count / table offset / table size
          ├─ file table         ─ XOR-cipher ─► LZSS ──► entry list
          └─ entry data (each)  ─ LZSS ──► "edz?"-wrapped blob ─ XOR-cipher (per type) ──► file
```

Two primitives are used everywhere: an XOR **block cipher** (§2) and **LZSS** (§3).

## 2. XOR block cipher — `decrypt(src, key, step, block, limit)`

Engine function: `Crypt_XorBlockReverse` (0x43E1D0). Parameters are byte `key`, byte `step`,
int `block`, int `limit`. Output has the same length as the input.

1. `size = len(src)`; `rem = size mod block`.
   `tail = rem` if `rem < block/4` (integer division) else `0`; then `tail += size & 1`.
2. `n = size - tail` (number of bytes that get deciphered, subject to `limit`).
3. While `n > 0` and `limit > 0`:
   - if `n < block` then `block = n` (the shrunk value persists for following iterations).
   - Within the current output block (starting at output index `base`) the source bytes are consumed
     **sequentially**, but written to the block from the end backwards, every second byte:
     first to positions `base+block-1, base+block-3, …` (`ceil(block/2)` bytes),
     then to `base+block-2, base+block-4, …` (`floor(block/2)` bytes).
   - every written byte is `src_byte XOR key`, then `key = (key + step) mod 256`.
   - `base += block; n -= block; limit -= block`.
4. All remaining source bytes (from the current source position to the end) are copied unchanged
   to the output from `base` onward.

Note the key keeps evolving across blocks; it is not reset per block.

## 3. LZSS — `lzss(src, out_size)`

Engine function: `Lzss_Decompress` (0x4740E0).

- Bit reader: MSB-first within each byte. Reading past the end of the input yields 0 bits.
- Dictionary: 0x2000 (8192) bytes, initially zero, write cursor starts at **1**,
  wraps with mask 0x1FFF. Every output byte is also stored into the dictionary at the cursor.
- Loop:
  - read 1 bit. **1** → read 8 bits = literal byte; output it.
  - **0** → read 13 bits `offset`. If `offset == 0` → **end of stream**.
    Otherwise read 4 bits `len`, copy `len + 3` bytes (3..18) from dictionary positions
    `offset, offset+1, …` (masked 0x1FFF) to the output, one at a time (overlapping copies allowed;
    each copied byte is written back into the dictionary immediately).
- The engine allocates `out_size` bytes for the output; the stream is expected to end with the
  0-offset terminator exactly when `out_size` bytes are produced.
- (Engine detail) The dictionary is a single global buffer that is not re-cleared between calls.
  Zero-initialising it per call reproduces all 317 files byte-exactly (VERIFIED via signatures and
  sizes), so the implementer should simply clear it per call.
- (RE-Q25) The game's own encoder is `Lzss_Compress` 0x473CB0 (score.dat and replays). Byte-identical output is not
  required by any reader: score.dat and replay checksums are computed by the writer over the bytes it wrote, and the
  decoder above accepts any valid stream, so any correct LZSS encoder in this format is compatible.

## 4. Archive header (file offset 0, 16 bytes)

| off | size | meaning |
|---|---|---|
| 0x00 | 4 | magic `"PBGZ"` (u32 LE 0x5A474250), read raw |
| 0x04 | 12 | three u32 LE, enciphered with `decrypt(key=0x1B, step=0x37, block=0x0C, limit=0x400)` |

After decryption the three dwords `a, b, c` give:

| value | meaning | th08.dat |
|---|---|---|
| `a - 123456` (0x1E240) | number of entries | 317 |
| `b - 345678` (0x5464E) | absolute file offset of the file table | 0x2CAA0C2 |
| `c - 567891` (0x8AA53) | decompressed size of the file table | 9063 |

Validation done by the engine: magic must match, entry count must be > 0, and table offset must be
< file size; otherwise the archive open fails.

## 5. File table

- Location: from the table offset to the **end of file** (`file_size - table_offset` bytes).
- Decode: `lzss(decrypt(bytes, key=0x3E, step=0x9B, block=0x80, limit=0x400), table_size)`.
- Content: `count` packed entries, no alignment:

| field | type | meaning |
|---|---|---|
| name | NUL-terminated ASCII | file name, no directory part (e.g. `stage1.std`) |
| offset | u32 | absolute offset of the compressed data in th08.dat |
| size | u32 | decompressed size (includes the 4-byte `edz?` prefix, see §6) |
| reserved | u32 | always 0 in th08.dat; not read by the loader (INFERRED from loader code) |

- Compressed size of entry *i* = `offset[i+1] - offset[i]`; for the last entry the
  "next offset" is the table offset (the engine appends a sentinel entry with offset = table offset).
- Lookup (`PbgArchive_FindEntry`, 0x474C70) is a linear, **case-insensitive** name comparison.
  Callers strip any directory part (`/` or `\`) before lookup, so e.g. `bgm/thbgm.fmt` resolves to
  entry `thbgm.fmt`.

## 6. Per-file data and the "edz" type wrapper

1. Read `compressed size` bytes at `offset`; LZSS-decompress to `size` bytes (the data itself is
   **not** XOR-enciphered before LZSS).
2. The result starts with a 4-byte wrapper: bytes `'e' 'd' 'z'` followed by a **type character**.
   (In the exe the magic is stored obfuscated at 0x4C7940 as `0x85 0xA4 0xDA`, i.e. +0x20/+0x40/+0x60.)
3. The type character selects cipher parameters from an 8-entry table (exe 0x4C78E0, 12 bytes per
   entry; the stored type byte is `char + 16*i + 16`). The remaining `size - 4` bytes are passed to
   `decrypt()`; its output is the final file content (the wrapper is dropped).
4. If the first three bytes are not `edz` or the type char is not in the table, the data is used as-is.

| type char | key | step | block | limit | used by (th08.dat) |
|---|---|---|---|---|---|
| `M` | 0x1B | 0x37 | 0x40 | 0x2800 | (unused) |
| `T` | 0x51 | 0xE9 | 0x40 | 0x3000 | `.txt` (1) |
| `A` | 0xC1 | 0x51 | 0x1400 | 0x2000 | `.anm` (113) |
| `J` | 0x03 | 0x19 | 0x1400 | 0x7800 | `.jpg` (37) |
| `E` | 0xAB | 0xCD | 0x200 | 0x1000 | `.ecl` (24) |
| `W` | 0x12 | 0x34 | 0x400 | 0x2800 | `.wav` (39) |
| `-` | 0x35 | 0x97 | 0x80 | 0x2800 | `.msg .dat .end .std .sht .rpy .mid .png .fmt .ver` (103) |
| `*` | 0x99 | 0x37 | 0x400 | 0x1000 | (unused) |

All 317 files in th08.dat carry an `edz` wrapper (VERIFIED by extractor log).

## 7. Engine loader behaviour (for the reimplementation)

`FileSystem_OpenFile(path, out_size*, from_disk)` (0x43E660):
- `from_disk == 0`: strip directory, look up in archive; if missing → logs
  `error : %s is not found in arcfile.` and returns null. Otherwise returns the fully decoded buffer
  (LZSS + edz decrypt) and writes the **pre-unwrap** size (table `size`) into `out_size` (INFERRED:
  the size reported to callers includes the 4 wrapper bytes; harmless since callers parse headers).
- `from_disk != 0`: read the file from the real file system (used for `score.dat`, `th08.cfg`
  and replays); the result also goes through the `edz` check.
- The archive file name is `th08.dat` in the working directory (string at 0x4B66C4).

## 8. Extractor

`tools/th08dat.py list` prints the table; `tools/th08dat.py extract "" <outdir>` writes all files
(≈95 s in pure Python). Uses only the standard library.

## 9. Full file list (name, decompressed size incl. 4-byte wrapper, compressed size, type)

| # | name | offset | size | stored | edz | final size |
|---|---|---|---|---|---|---|
| 0 | ascii.anm | 0x00000010 | 405424 | 129892 | A | 405420 |
| 1 | capture.anm | 0x0001fb74 | 1132 | 449 | A | 1128 |
| 2 | demorpy0.rpy | 0x0001fd35 | 3543 | 3986 | - | 3539 |
| 3 | demorpy1.rpy | 0x00020cc7 | 2720 | 3059 | - | 2716 |
| 4 | demorpy2.rpy | 0x000218ba | 3085 | 3471 | - | 3081 |
| 5 | demorpy3.rpy | 0x00022649 | 3215 | 3618 | - | 3211 |
| 6 | ecldata1.ecl | 0x0002346b | 45848 | 14357 | E | 45844 |
| 7 | ecldata1sp.ecl | 0x00026c80 | 39812 | 12912 | E | 39808 |
| 8 | ecldata2.ecl | 0x00029ef0 | 51248 | 15197 | E | 51244 |
| 9 | ecldata2sp.ecl | 0x0002da4d | 37516 | 12391 | E | 37512 |
| 10 | ecldata3.ecl | 0x00030ab4 | 50244 | 15451 | E | 50240 |
| 11 | ecldata3sp.ecl | 0x0003470f | 41396 | 13469 | E | 41392 |
| 12 | ecldata4a.ecl | 0x00037bac | 49856 | 15477 | E | 49852 |
| 13 | ecldata4asp.ecl | 0x0003b821 | 41880 | 13894 | E | 41876 |
| 14 | ecldata4b.ecl | 0x0003ee67 | 58692 | 16918 | E | 58688 |
| 15 | ecldata4bsp.ecl | 0x0004307d | 50252 | 15353 | E | 50248 |
| 16 | ecldata5.ecl | 0x00046c76 | 47228 | 14140 | E | 47224 |
| 17 | ecldata5sp.ecl | 0x0004a3b2 | 31188 | 11282 | E | 31184 |
| 18 | ecldata6.ecl | 0x0004cfc4 | 51508 | 17035 | E | 51504 |
| 19 | ecldata6sp.ecl | 0x0005124f | 50848 | 16652 | E | 50844 |
| 20 | ecldata7.ecl | 0x0005535b | 67328 | 21097 | E | 67324 |
| 21 | ecldata7sp.ecl | 0x0005a5c4 | 70812 | 21530 | E | 70808 |
| 22 | ecldata8.ecl | 0x0005f9de | 90472 | 23688 | E | 90468 |
| 23 | ecldata8sp.ecl | 0x00065666 | 74460 | 20447 | E | 74456 |
| 24 | ecldata_al.ecl | 0x0006a645 | 6868 | 4353 | E | 6864 |
| 25 | ecldata_rm.ecl | 0x0006b746 | 5928 | 3897 | E | 5924 |
| 26 | ecldata_sk.ecl | 0x0006c67f | 6216 | 3908 | E | 6212 |
| 27 | ecldata_yk.ecl | 0x0006d5c3 | 24460 | 7317 | E | 24456 |
| 28 | ecldata_ym.ecl | 0x0006f258 | 7496 | 4039 | E | 7492 |
| 29 | ecldata_yy.ecl | 0x0007021f | 9604 | 5184 | E | 9600 |
| 30 | eff01.anm | 0x0007165f | 262644 | 61819 | A | 262640 |
| 31 | eff02.anm | 0x000807da | 262668 | 107267 | A | 262664 |
| 32 | eff03.anm | 0x0009aadd | 262660 | 123085 | A | 262656 |
| 33 | eff04a.anm | 0x000b8baa | 262664 | 113393 | A | 262660 |
| 34 | eff04b.anm | 0x000d469b | 262648 | 130535 | A | 262644 |
| 35 | eff05.anm | 0x000f4482 | 656012 | 173927 | A | 656008 |
| 36 | eff06.anm | 0x0011ebe9 | 918656 | 286210 | A | 918652 |
| 37 | eff07.anm | 0x001649eb | 1836144 | 601151 | A | 1836140 |
| 38 | eff08.anm | 0x001f762a | 1836084 | 819893 | A | 1836080 |
| 39 | eff09al.anm | 0x002bf8df | 8520256 | 1362827 | A | 8520252 |
| 40 | eff09rm.anm | 0x0040c46a | 262824 | 72664 | A | 262820 |
| 41 | eff09sk.anm | 0x0041e042 | 8520496 | 1381383 | A | 8520492 |
| 42 | eff09yk.anm | 0x0056f449 | 262864 | 144748 | A | 262860 |
| 43 | eff09ym.anm | 0x005929b5 | 263072 | 109513 | A | 263068 |
| 44 | eff09yy.anm | 0x005ad57e | 262648 | 128605 | A | 262644 |
| 45 | end00.jpg | 0x005ccbdb | 99988 | 112192 | J | 99984 |
| 46 | end00a.end | 0x005e821b | 755 | 830 | - | 751 |
| 47 | end00b.end | 0x005e8559 | 1951 | 2116 | - | 1947 |
| 48 | end00b.jpg | 0x005e8d9d | 105798 | 118807 | J | 105794 |
| 49 | end00b0.jpg | 0x00605db4 | 112727 | 126590 | J | 112723 |
| 50 | end00b1.jpg | 0x00624c32 | 106972 | 120098 | J | 106968 |
| 51 | end00c.end | 0x00642154 | 4640 | 4823 | - | 4636 |
| 52 | end00c0.jpg | 0x0064342b | 108979 | 122406 | J | 108975 |
| 53 | end00c1.jpg | 0x00661251 | 108995 | 122425 | J | 108991 |
| 54 | end00c2.jpg | 0x0067f08a | 109927 | 123436 | J | 109923 |
| 55 | end01.jpg | 0x0069d2b6 | 103503 | 116140 | J | 103499 |
| 56 | end01a.end | 0x006b9862 | 723 | 808 | - | 719 |
| 57 | end01b.end | 0x006b9b8a | 2429 | 2628 | - | 2425 |
| 58 | end01b0.jpg | 0x006ba5ce | 108117 | 121391 | J | 108113 |
| 59 | end01b1.jpg | 0x006d7ffd | 59600 | 66923 | J | 59596 |
| 60 | end01c.end | 0x006e8568 | 4394 | 4552 | - | 4390 |
| 61 | end01c.jpg | 0x006e9730 | 94590 | 106208 | J | 94586 |
| 62 | end01c0.jpg | 0x00703610 | 107860 | 121135 | J | 107856 |
| 63 | end01c1.jpg | 0x00720f3f | 105711 | 118726 | J | 105707 |
| 64 | end01c2.jpg | 0x0073df05 | 107021 | 120183 | J | 107017 |
| 65 | end02.jpg | 0x0075b47c | 99074 | 111138 | J | 99070 |
| 66 | end02a.end | 0x0077669e | 763 | 846 | - | 759 |
| 67 | end02b.end | 0x007769ec | 2517 | 2682 | - | 2513 |
| 68 | end02b.jpg | 0x00777466 | 104894 | 117775 | J | 104890 |
| 69 | end02b0.jpg | 0x00794075 | 109643 | 123126 | J | 109639 |
| 70 | end02b1.jpg | 0x007b216b | 108019 | 121295 | J | 108015 |
| 71 | end02c.end | 0x007cfb3a | 4620 | 4768 | - | 4616 |
| 72 | end02c0.jpg | 0x007d0dda | 105183 | 118097 | J | 105179 |
| 73 | end02c1.jpg | 0x007edb2b | 110136 | 123685 | J | 110132 |
| 74 | end02c2.jpg | 0x0080be50 | 109494 | 122978 | J | 109490 |
| 75 | end03.jpg | 0x00829eb2 | 98238 | 110241 | J | 98234 |
| 76 | end03a.end | 0x00844d53 | 821 | 910 | - | 817 |
| 77 | end03b.end | 0x008450e1 | 2291 | 2443 | - | 2287 |
| 78 | end03b.jpg | 0x00845a6c | 103636 | 116372 | J | 103632 |
| 79 | end03b0.jpg | 0x00862100 | 106808 | 119929 | J | 106804 |
| 80 | end03b1.jpg | 0x0087f579 | 107375 | 120588 | J | 107371 |
| 81 | end03c.end | 0x0089cc85 | 5234 | 5346 | - | 5230 |
| 82 | end03c0.jpg | 0x0089e167 | 106139 | 119174 | J | 106135 |
| 83 | end03c1.jpg | 0x008bb2ed | 105487 | 118475 | J | 105483 |
| 84 | end03c2.jpg | 0x008d81b8 | 105673 | 118656 | J | 105669 |
| 85 | enemy.anm | 0x008f5138 | 538292 | 143990 | A | 538288 |
| 86 | etama.anm | 0x009183ae | 613784 | 210829 | A | 613780 |
| 87 | face_al00.anm | 0x0094bb3b | 1578292 | 929863 | A | 1578288 |
| 88 | face_alsp.anm | 0x00a2eb82 | 524572 | 227524 | A | 524568 |
| 89 | face_cdbg.anm | 0x00a66446 | 525460 | 92653 | A | 525456 |
| 90 | face_mr00.anm | 0x00a7ce33 | 1578420 | 915953 | A | 1578416 |
| 91 | face_rm00.anm | 0x00b5c824 | 1578292 | 879284 | A | 1578288 |
| 92 | face_rs00.anm | 0x00c332d8 | 1578420 | 819988 | A | 1578416 |
| 93 | face_rssp.anm | 0x00cfb5ec | 524572 | 218248 | A | 524568 |
| 94 | face_sk00.anm | 0x00d30a74 | 1578420 | 892277 | A | 1578416 |
| 95 | face_sksp.anm | 0x00e0a7e9 | 524588 | 198563 | A | 524584 |
| 96 | face_st01.anm | 0x00e3af8c | 952876 | 477188 | A | 952872 |
| 97 | face_st01sp.anm | 0x00eaf790 | 524572 | 222176 | A | 524568 |
| 98 | face_st02.anm | 0x00ee5b70 | 1084560 | 618157 | A | 1084556 |
| 99 | face_st02sp.anm | 0x00f7ca1d | 524572 | 229826 | A | 524568 |
| 100 | face_st03.anm | 0x00fb4bdf | 1347928 | 789461 | A | 1347924 |
| 101 | face_st03sp.anm | 0x010757b4 | 524572 | 259690 | A | 524568 |
| 102 | face_st04a.anm | 0x010b4e1e | 1348024 | 811986 | A | 1348020 |
| 103 | face_st04asp.anm | 0x0117b1f0 | 524572 | 310864 | A | 524568 |
| 104 | face_st04b.anm | 0x011c7040 | 1479724 | 931535 | A | 1479720 |
| 105 | face_st04bsp.anm | 0x012aa70f | 524572 | 328579 | A | 524568 |
| 106 | face_st05.anm | 0x012faa92 | 1871676 | 724790 | A | 1871672 |
| 107 | face_st05b.anm | 0x013ab9c8 | 265548 | 119021 | A | 265544 |
| 108 | face_st05msp.anm | 0x013c8ab5 | 1048860 | 311753 | A | 1048856 |
| 109 | face_st05sp.anm | 0x01414c7e | 524572 | 205897 | A | 524568 |
| 110 | face_st06.anm | 0x014470c7 | 2395988 | 899059 | A | 2395984 |
| 111 | face_st06sp.anm | 0x015228ba | 1048860 | 312697 | A | 1048856 |
| 112 | face_st07.anm | 0x0156ee33 | 2133232 | 1138599 | A | 2133228 |
| 113 | face_st07sp.anm | 0x01684dda | 1048860 | 597848 | A | 1048856 |
| 114 | face_st08.anm | 0x01716d32 | 2658744 | 917919 | A | 2658740 |
| 115 | face_st08m.anm | 0x017f6ed1 | 689620 | 347035 | A | 689616 |
| 116 | face_st08msp.anm | 0x0184ba6c | 524572 | 255365 | A | 524568 |
| 117 | face_st08sp.anm | 0x01889ff1 | 1048860 | 282152 | A | 1048856 |
| 118 | face_yk00.anm | 0x018cee19 | 1578420 | 1013966 | A | 1578416 |
| 119 | face_yksp.anm | 0x019c66e7 | 1048860 | 308936 | A | 1048856 |
| 120 | face_ym00.anm | 0x01a11daf | 1578292 | 863397 | A | 1578288 |
| 121 | face_ymsp.anm | 0x01ae4a54 | 524572 | 214348 | A | 524568 |
| 122 | face_yy00.anm | 0x01b18fa0 | 1578420 | 946390 | A | 1578416 |
| 123 | face_yysp.anm | 0x01c00076 | 524572 | 247662 | A | 524568 |
| 124 | front.anm | 0x01c3c7e4 | 395984 | 118179 | A | 395980 |
| 125 | info00.jpg | 0x01c59587 | 172231 | 188950 | J | 172227 |
| 126 | info01.jpg | 0x01c8779d | 169379 | 185751 | J | 169375 |
| 127 | info02.jpg | 0x01cb4d34 | 172300 | 189042 | J | 172296 |
| 128 | init.mid | 0x01ce2fa6 | 732 | 807 | - | 728 |
| 129 | loading00.anm | 0x01ce32cd | 524512 | 257913 | A | 524508 |
| 130 | loading00a.anm | 0x01d22246 | 524512 | 258868 | A | 524508 |
| 131 | loading00h.anm | 0x01d6157a | 524512 | 238510 | A | 524508 |
| 132 | loading01.anm | 0x01d9b928 | 524512 | 257812 | A | 524508 |
| 133 | loading01a.anm | 0x01dda83c | 524512 | 244756 | A | 524508 |
| 134 | loading01h.anm | 0x01e16450 | 524512 | 244915 | A | 524508 |
| 135 | loading02.anm | 0x01e52103 | 524512 | 223966 | A | 524508 |
| 136 | loading02a.anm | 0x01e88be1 | 524512 | 207943 | A | 524508 |
| 137 | loading02h.anm | 0x01ebb828 | 524512 | 216005 | A | 524508 |
| 138 | loading03.anm | 0x01ef03ed | 524512 | 247016 | A | 524508 |
| 139 | loading03a.anm | 0x01f2c8d5 | 524512 | 256022 | A | 524508 |
| 140 | loading03h.anm | 0x01f6b0eb | 524512 | 259250 | A | 524508 |
| 141 | msg1a.dat | 0x01faa59d | 1744 | 1850 | - | 1740 |
| 142 | msg1b.dat | 0x01faacd7 | 1640 | 1753 | - | 1636 |
| 143 | msg1c.dat | 0x01fab3b0 | 1604 | 1730 | - | 1600 |
| 144 | msg1d.dat | 0x01faba72 | 2136 | 2249 | - | 2132 |
| 145 | msg2a.dat | 0x01fac33b | 2288 | 2439 | - | 2284 |
| 146 | msg2b.dat | 0x01faccc2 | 1868 | 1973 | - | 1864 |
| 147 | msg2c.dat | 0x01fad477 | 1744 | 1853 | - | 1740 |
| 148 | msg2d.dat | 0x01fadbb4 | 1708 | 1813 | - | 1704 |
| 149 | msg3a.dat | 0x01fae2c9 | 3336 | 3326 | - | 3332 |
| 150 | msg3b.dat | 0x01faefc7 | 3228 | 3296 | - | 3224 |
| 151 | msg3c.dat | 0x01fafca7 | 3512 | 3546 | - | 3508 |
| 152 | msg3d.dat | 0x01fb0a81 | 3072 | 3123 | - | 3068 |
| 153 | msg4ab.dat | 0x01fb16b4 | 2900 | 2920 | - | 2896 |
| 154 | msg4ac.dat | 0x01fb221c | 3536 | 3548 | - | 3532 |
| 155 | msg4ba.dat | 0x01fb2ff8 | 3008 | 3109 | - | 3004 |
| 156 | msg4bd.dat | 0x01fb3c1d | 2768 | 2849 | - | 2764 |
| 157 | msg4dm.dat | 0x01fb473e | 284 | 315 | - | 280 |
| 158 | msg5a.dat | 0x01fb4879 | 3684 | 3748 | - | 3680 |
| 159 | msg5b.dat | 0x01fb571d | 4788 | 4873 | - | 4784 |
| 160 | msg5c.dat | 0x01fb6a26 | 4272 | 4340 | - | 4268 |
| 161 | msg5d.dat | 0x01fb7b1a | 4276 | 4374 | - | 4272 |
| 162 | msg6a.dat | 0x01fb8c30 | 3676 | 3671 | - | 3672 |
| 163 | msg6b.dat | 0x01fb9a87 | 3720 | 3804 | - | 3716 |
| 164 | msg6c.dat | 0x01fba963 | 4596 | 4507 | - | 4592 |
| 165 | msg6d.dat | 0x01fbbafe | 4676 | 4621 | - | 4672 |
| 166 | msg7a.dat | 0x01fbcd0b | 3616 | 3677 | - | 3612 |
| 167 | msg7b.dat | 0x01fbdb68 | 3888 | 3979 | - | 3884 |
| 168 | msg7c.dat | 0x01fbeaf3 | 4536 | 4600 | - | 4532 |
| 169 | msg7d.dat | 0x01fbfceb | 4368 | 4310 | - | 4364 |
| 170 | msg8a.dat | 0x01fc0dc1 | 5988 | 5811 | - | 5984 |
| 171 | msg8b.dat | 0x01fc2474 | 6616 | 6495 | - | 6612 |
| 172 | msg8c.dat | 0x01fc3dd3 | 6080 | 5955 | - | 6076 |
| 173 | msg8d.dat | 0x01fc5516 | 6820 | 6556 | - | 6816 |
| 174 | music.jpg | 0x01fc6eb2 | 147935 | 161943 | J | 147931 |
| 175 | music00.anm | 0x01fee749 | 138864 | 23371 | A | 138860 |
| 176 | musiccmt.txt | 0x01ff4294 | 7431 | 7826 | T | 7427 |
| 177 | nowloading.anm | 0x01ff6126 | 33704 | 10801 | A | 33700 |
| 178 | phantasm.jpg | 0x01ff8b57 | 74660 | 75601 | J | 74656 |
| 179 | player00.anm | 0x0200b2a8 | 266768 | 80189 | A | 266764 |
| 180 | player01.anm | 0x0201ebe5 | 269336 | 70667 | A | 269332 |
| 181 | player02.anm | 0x0202fff0 | 135012 | 35736 | A | 135008 |
| 182 | player03.anm | 0x02038b88 | 134448 | 32969 | A | 134444 |
| 183 | ply00a.sht | 0x02040c51 | 1476 | 1121 | - | 1472 |
| 184 | ply00as.sht | 0x020410b2 | 1420 | 1077 | - | 1416 |
| 185 | ply01a.sht | 0x020414e7 | 1364 | 1075 | - | 1360 |
| 186 | ply01as.sht | 0x0204191a | 804 | 789 | - | 800 |
| 187 | ply02a.sht | 0x02041c2f | 1588 | 1204 | - | 1584 |
| 188 | ply02as.sht | 0x020420e3 | 3572 | 2241 | - | 3568 |
| 189 | ply03a.sht | 0x020429a4 | 1308 | 1046 | - | 1304 |
| 190 | ply03as.sht | 0x02042dba | 2260 | 1689 | - | 2256 |
| 191 | result.jpg | 0x02043453 | 312714 | 346479 | J | 312710 |
| 192 | result00.anm | 0x02097dc2 | 952984 | 198074 | A | 952980 |
| 193 | resulttext.anm | 0x020c837c | 828 | 351 | A | 824 |
| 194 | select00.png | 0x020c84db | 364754 | 409419 | - | 364750 |
| 195 | se_bonus.wav | 0x0212c426 | 45340 | 50181 | W | 45336 |
| 196 | se_bonus2.wav | 0x0213882b | 45340 | 49540 | W | 45336 |
| 197 | se_border.wav | 0x021449af | 132432 | 147251 | W | 132428 |
| 198 | se_cancel00.wav | 0x021688e2 | 13656 | 13067 | W | 13652 |
| 199 | se_cardget.wav | 0x0216bbed | 222868 | 241672 | W | 222864 |
| 200 | se_cat00.wav | 0x021a6bf5 | 84264 | 87191 | W | 84260 |
| 201 | se_damage00.wav | 0x021bc08c | 6776 | 7510 | W | 6772 |
| 202 | se_damage01.wav | 0x021bdde2 | 27032 | 27854 | W | 27028 |
| 203 | se_enep00.wav | 0x021c4ab0 | 115588 | 128938 | W | 115584 |
| 204 | se_enep01.wav | 0x021e425a | 230304 | 204390 | W | 230300 |
| 205 | se_extend.wav | 0x022160c0 | 70192 | 67103 | W | 70188 |
| 206 | se_graze.wav | 0x022266df | 4546 | 5103 | W | 4542 |
| 207 | se_gun00.wav | 0x02227ace | 134698 | 95604 | W | 134694 |
| 208 | se_invalid.wav | 0x0223f042 | 62954 | 64741 | W | 62950 |
| 209 | se_item00.wav | 0x0224ed27 | 7382 | 5064 | W | 7378 |
| 210 | se_item01.wav | 0x022500ef | 10856 | 11573 | W | 10852 |
| 211 | se_kira00.wav | 0x02252e24 | 35226 | 24251 | W | 35222 |
| 212 | se_kira01.wav | 0x02258cdf | 21944 | 20743 | W | 21940 |
| 213 | se_kira02.wav | 0x0225dde6 | 25786 | 26464 | W | 25782 |
| 214 | se_lazer00.wav | 0x02264546 | 22186 | 24633 | W | 22182 |
| 215 | se_lazer01.wav | 0x0226a57f | 24186 | 22073 | W | 24182 |
| 216 | se_nep00.wav | 0x0226fbb8 | 224930 | 226187 | W | 224926 |
| 217 | se_ok00.wav | 0x022a6f43 | 10880 | 11921 | W | 10876 |
| 218 | se_ophide.wav | 0x022a9dd4 | 15456 | 17365 | W | 15452 |
| 219 | se_opshow.wav | 0x022ae1a9 | 18830 | 21156 | W | 18826 |
| 220 | se_option.wav | 0x022b344d | 26508 | 29747 | W | 26504 |
| 221 | se_pause.wav | 0x022ba880 | 133680 | 141423 | W | 133676 |
| 222 | se_pldead00.wav | 0x022dd0ef | 60856 | 60207 | W | 60852 |
| 223 | se_plst00.wav | 0x022ebc1e | 3600 | 4010 | W | 3596 |
| 224 | se_power0.wav | 0x022ecbc8 | 73008 | 59174 | W | 73004 |
| 225 | se_power1.wav | 0x022fb2ee | 30416 | 29323 | W | 30412 |
| 226 | se_powerup.wav | 0x02302579 | 77282 | 83512 | W | 77278 |
| 227 | se_select00.wav | 0x02316bb1 | 1150 | 1295 | W | 1146 |
| 228 | se_slash.wav | 0x023170c0 | 54786 | 57480 | W | 54782 |
| 229 | se_tan00.wav | 0x02325148 | 15104 | 14751 | W | 15100 |
| 230 | se_tan01.wav | 0x02328ae7 | 15104 | 12904 | W | 15100 |
| 231 | se_tan02.wav | 0x0232bd4f | 11712 | 11669 | W | 11708 |
| 232 | se_timeout.wav | 0x0232eae4 | 19394 | 21669 | W | 19390 |
| 233 | se_timeout2.wav | 0x02333f89 | 19392 | 21748 | W | 19388 |
| 234 | staff00.end | 0x0233947d | 432 | 487 | - | 428 |
| 235 | staff00.jpg | 0x02339664 | 41842 | 45334 | J | 41838 |
| 236 | staff00b.end | 0x0234477a | 393 | 443 | - | 389 |
| 237 | staff00b.jpg | 0x02344935 | 33563 | 36113 | J | 33559 |
| 238 | staff01.anm | 0x0234d646 | 788028 | 168140 | A | 788024 |
| 239 | stage1.std | 0x02376712 | 4432 | 3002 | - | 4428 |
| 240 | stage1_s.std | 0x023772cc | 3852 | 2503 | - | 3848 |
| 241 | stage2.std | 0x02377c93 | 6360 | 3310 | - | 6356 |
| 242 | stage2_s.std | 0x02378981 | 5920 | 2927 | - | 5916 |
| 243 | stage3.std | 0x023794f0 | 2832 | 1582 | - | 2828 |
| 244 | stage3_s.std | 0x02379b1e | 2512 | 1343 | - | 2508 |
| 245 | stage4a.std | 0x0237a05d | 5336 | 3735 | - | 5332 |
| 246 | stage4a_s.std | 0x0237aef4 | 4016 | 2677 | - | 4012 |
| 247 | stage4b.std | 0x0237b969 | 5336 | 3735 | - | 5332 |
| 248 | stage4b_s.std | 0x0237c800 | 4016 | 2677 | - | 4012 |
| 249 | stage5.std | 0x0237d275 | 4520 | 2987 | - | 4516 |
| 250 | stage5_s.std | 0x0237de20 | 3640 | 2231 | - | 3636 |
| 251 | stage6.std | 0x0237e6d7 | 3808 | 2434 | - | 3804 |
| 252 | stage6_s.std | 0x0237f059 | 2828 | 1602 | - | 2824 |
| 253 | stage7.std | 0x0237f69b | 3732 | 2392 | - | 3728 |
| 254 | stage7_s.std | 0x0237fff3 | 2932 | 1711 | - | 2928 |
| 255 | stage8.std | 0x023806a2 | 5356 | 3734 | - | 5352 |
| 256 | stage8_s.std | 0x02381538 | 4016 | 2680 | - | 4012 |
| 257 | stg1bg.anm | 0x02381fb0 | 1048940 | 262854 | A | 1048936 |
| 258 | stg1enm.anm | 0x023c2276 | 132852 | 33009 | A | 132848 |
| 259 | stg1txt.anm | 0x023ca367 | 262984 | 48159 | A | 262980 |
| 260 | stg2bg.anm | 0x023d5f86 | 1048940 | 402197 | A | 1048936 |
| 261 | stg2enm.anm | 0x0243829b | 132756 | 32631 | A | 132752 |
| 262 | stg2txt.anm | 0x02440212 | 262984 | 48922 | A | 262980 |
| 263 | stg3bg.anm | 0x0244c12c | 788832 | 365314 | A | 788828 |
| 264 | stg3enm.anm | 0x024a542e | 132756 | 37495 | A | 132752 |
| 265 | stg3txt.anm | 0x024ae6a5 | 262984 | 48842 | A | 262980 |
| 266 | stg4abg.anm | 0x024ba56f | 1573856 | 678764 | A | 1573852 |
| 267 | stg4aenm.anm | 0x025600db | 134260 | 41279 | A | 134256 |
| 268 | stg4atxt.anm | 0x0256a21a | 262984 | 49313 | A | 262980 |
| 269 | stg4benm.anm | 0x025762bb | 268636 | 52665 | A | 268632 |
| 270 | stg4btxt.anm | 0x02583074 | 262984 | 50068 | A | 262980 |
| 271 | stg5bg.anm | 0x0258f408 | 1049508 | 559181 | A | 1049504 |
| 272 | stg5enm.anm | 0x02617c55 | 266336 | 59433 | A | 266332 |
| 273 | stg5txt.anm | 0x0262647e | 263020 | 49434 | A | 263016 |
| 274 | stg6bg.anm | 0x02632598 | 2886088 | 1481550 | A | 2886084 |
| 275 | stg6enm.anm | 0x0279c0e6 | 132908 | 34099 | A | 132904 |
| 276 | stg6txt.anm | 0x027a4619 | 263044 | 44288 | A | 263040 |
| 277 | stg7bg.anm | 0x027af319 | 2361984 | 578502 | A | 2361980 |
| 278 | stg7enm.anm | 0x0283c6df | 265604 | 61339 | A | 265600 |
| 279 | stg7txt.anm | 0x0284b67a | 263044 | 48863 | A | 263040 |
| 280 | stg8bg.anm | 0x02857559 | 1573836 | 397080 | A | 1573832 |
| 281 | stg8enm.anm | 0x028b8471 | 397536 | 106370 | A | 397532 |
| 282 | stg8txt.anm | 0x028d23f3 | 263008 | 50510 | A | 263004 |
| 283 | stgenm_al.anm | 0x028de941 | 132760 | 25902 | A | 132756 |
| 284 | stgenm_rm.anm | 0x028e4e6f | 131800 | 26929 | A | 131796 |
| 285 | stgenm_sk.anm | 0x028eb7a0 | 132688 | 25537 | A | 132684 |
| 286 | stgenm_yk.anm | 0x028f1b61 | 527792 | 132178 | A | 527788 |
| 287 | stgenm_ym.anm | 0x02911fb3 | 133084 | 48469 | A | 133080 |
| 288 | stgenm_yy.anm | 0x0291dd08 | 1050200 | 496741 | A | 1050196 |
| 289 | text.anm | 0x0299716d | 7052 | 4708 | A | 7048 |
| 290 | th08logo.jpg | 0x029983d1 | 299731 | 332214 | J | 299727 |
| 291 | th08_00.mid | 0x029e9587 | 27035 | 12854 | - | 27031 |
| 292 | th08_01.mid | 0x029ec7bd | 13956 | 11945 | - | 13952 |
| 293 | th08_0100d.ver | 0x029ef666 | 360 | 406 | - | 356 |
| 294 | th08_03.mid | 0x029ef7fc | 31176 | 16651 | - | 31172 |
| 295 | th08_04.mid | 0x029f3907 | 74729 | 30737 | - | 74725 |
| 296 | th08_05.mid | 0x029fb118 | 37813 | 18881 | - | 37809 |
| 297 | th08_06.mid | 0x029ffad9 | 31766 | 18058 | - | 31762 |
| 298 | th08_07.mid | 0x02a04163 | 49084 | 24230 | - | 49080 |
| 299 | th08_08.mid | 0x02a0a009 | 36450 | 16308 | - | 36446 |
| 300 | th08_09.mid | 0x02a0dfbd | 93975 | 31586 | - | 93971 |
| 301 | th08_10.mid | 0x02a15b1f | 49417 | 19300 | - | 49413 |
| 302 | th08_11.mid | 0x02a1a683 | 65368 | 29321 | - | 65364 |
| 303 | th08_12.mid | 0x02a2190c | 43948 | 20313 | - | 43944 |
| 304 | th08_13.mid | 0x02a26865 | 32181 | 16045 | - | 32177 |
| 305 | th08_13b.mid | 0x02a2a712 | 32281 | 15560 | - | 32277 |
| 306 | th08_14.mid | 0x02a2e3da | 75940 | 31400 | - | 75936 |
| 307 | th08_15.mid | 0x02a35e82 | 71003 | 26523 | - | 70999 |
| 308 | th08_16.mid | 0x02a3c61d | 13514 | 11270 | - | 13510 |
| 309 | th08_17.mid | 0x02a3f223 | 40614 | 18439 | - | 40610 |
| 310 | th08_18.mid | 0x02a43a2a | 64843 | 22817 | - | 64839 |
| 311 | th08_19.mid | 0x02a4934b | 97795 | 40993 | - | 97791 |
| 312 | th08_20.mid | 0x02a5336c | 27056 | 13402 | - | 27052 |
| 313 | thbgm.fmt | 0x02a567c6 | 1113 | 1198 | - | 1109 |
| 314 | times.anm | 0x02a56c74 | 132428 | 29881 | A | 132424 |
| 315 | title00.png | 0x02a5e12d | 635002 | 713624 | - | 634998 |
| 316 | title01.anm | 0x02b0c4c5 | 7386448 | 1694717 | A | 7386444 |
