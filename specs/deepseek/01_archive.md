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
- The `budget` is a byte count. The header call passes a large budget so all 12 bytes
  transform. The file-table call passes `0x400`, i.e. only the first 1024 bytes of the
  region are transformed; the rest is copied verbatim (callers exploit this).
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
| `T`       | 0x51 | 0xE9 | 0x0040 | 0x3000 | `.txt` (musiccmt.txt), `text.anm`  |
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
- `tools/extract_th08.py` — **the deliverable single-pass extractor**: reads `th08.dat`,
  LZSS-decompresses all 317 members into `extracted/`, then applies `edz` unwrapping and
  writes plaintext files into `decoded/`. Usage:

  ```
  python3 tools/extract_th08.py            # defaults: game th08.dat → ./extracted + ./decoded
  python3 tools/extract_th08.py <th08.dat> <raw_outdir> [<decoded_outdir>]
  ```

  Verified: reproduces all 317 files at exact sizes; `decoded/` byte-identical to the
  incremental `pbgz.py`+`edz.py` pipeline output.

## 8. Full contained-file list (317 members)

`csize` = compressed extent in th08.dat, `usize` = decompressed size, `type` = edz tag
char (`-` = generic key; see §5).

<!--FILETABLE-->
