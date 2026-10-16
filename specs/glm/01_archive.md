# Touhou 8 (IN) — th08 dat archive format (`th08.dat`, magic `PBGZ`)
**VERIFIED** (extraction of the real 46,838,025-byte th08.dat in this spec dir: all 317 entries decompressed and sizes match, see `tools/extract_th08.py`).

Summary of what the archive looks like, from binary analysis (Ghidra headless on th08.exe).

## Top-level container

One file `th08.dat` = 16-byte header + contiguous LZSS-compressed file blobs + one small encrypted entry table at end-of-file.

| field | size | notes |
|--|--|--|
| magic | 4 | always "PBGZ" (0x50 42 47 5a) || magic | 4 | ASCII "PBGZ" |
| hdr buf | 12 | three LE u32 obfuscated (see header): (val0, val1, val2) |
| payloads | ... | one contiguous LZ-compressed blob per file, lengths = size[i+1]-size[i]; no padding |
| entry table | rest | encrypted entry table (see table section), stored at table_offset (= file end - 4167B for th08.dat) |

- Offsets are absolute in the .dat file, SIZE is the DECOMPRESSED size of each entry (verified: all 317 sizes = the actual lzss output).
- The first payload begins immediately at byte 0x10 (=16), no gap.
- File blobs are byte-contiguous; the i-th compressed blob size = offset[i+1] − offset[i]; there is no separate compressed-size field. This is VERIFIED from FUN_474af0 (`next.offset - this.offset` = GlobalAlloc'ed span).
- Entries are SORTED in the table by ascending offset and their compressed spans tile from 0x10 to `table_offset` (VERIFIED: max(offset)+span == table_offset, no gaps).
- With N entries: compressed payloads ≡ offset[0…N-1], table_offset = offsets[N] (i.e. end).
- th08.dat only: no per-file obfuscation step (the older Touhou 6 shipped XOR precautions for entries; th08 has NONE — verified `FUN_474af0` contains no `FUN_43e1d0` call).

## Structure in memory

```
struct Archive {              // 0x14 or more
    Entry*  entries;          // +0
    uint    entry_count;      // +4
    ...
    Stream* stream;           // +12 (vtable at 0x4b98b4)
}
struct Entry {                // 16 bytes
    char*  name;              // +0  (asciz, shift-jis)
    uint32 offset;            // +4  (file offset of compressed blob)
    uint32 size;              // +8  (uncompressed size)
    uint32 flag;              // +0xc (unused/single 0; stored values are still read)
}
```
- The entry table is kept as `count+1` `Entry` values appended with a sentinel: `entries[count].offset = table_offset`, `size = 0`.

## Header (VERIFIED)

First 16 bytes:
- bytes 0..3  literal `"PBGZ"` (checked directly, byte order 0x50 0x42 0x47 0x5a → LE dwords represented as `0x5a474250`).
- bytes 4..15 three LE u32 obfuscated.

Header decrypt (`FUN_43e1d0(ecx=buf, edx=12, key=0x1b, delta=0x37, block=0x0c, guard=0x400)`):

Per block (here: exactly one 12-byte block):
```
for i in 0 .. ceil(block/2)-1   : dest[block - 1 - 2i] = src[i] ^ key            (key += delta per byte)
for i in 0 .. floor(block/2)-1  : dest[block - 2 - 2i] = src[ceil + i] ^ key    (key += delta per byte)
```
i.e. the plain bytes are distributed in REVERSE interleaved slots with the running-XOR applied per byte, so after the whole file is parsed the running key has advanced once per plaintext byte. For a 0-length tail: after the block loop, a residual tail (`len % block`, only if len%block < block/4, plus `len & 1` byte) is copied verbatim from the source (this matters for the big table decrypt below, not for this 12-byte header).

After decrypting the 3 u32s:
```
entry_count           = u32[0] - 123456
table_offset          = u32[1] - 345678
table_decomp_size     = u32[2] - 567891
```
For th08.dat this yields `317`, `0x2CAA0C2` (= filesize − 4167), `9063`.

## Entry table (VERIFIED)

Located at the very end of `th08.dat` (`table_offset` = 0x2CAA0C2).
1. Read `span = filesize - table_offset` bytes (= 4167 bytes) into a buffer.
2. Decrypt in place with `FUN_43e1d0(ecx=buf, edx=span, key=0x3e, delta=0x9b, block=0x80, guard=0x400)`:
   only first `guard / block` blocks fully covered = `0x400 / 0x80` = **8 blocks = 1024 bytes** get the interleave-reverse + running-XOR; the remaining `3143` bytes of the span are **verbatim plaintext** (verified: the decrypted data and the raw data match byte-for-byte after 1024). The main block loop keeps the same last-block reduction and `len & 1` tail rules as FUN_43e1d0 — the tail byte of 4167B (`4167 & 1`) is verbatim-copied as well.
3. LZSS-decompress the buffer (`FUN_4740e0(ecx=decrypted, edx=span, [out=0, outsize=table_decomp_size])`):
   output is `table_decomp_size` = 9063 bytes = the *uncompressed* entry table.
4. The uncompressed entry table is then parse (`FUN_474fa0(buf, count, table_offset)`):
   `for each of entry_count records: read name (asciz), then three consecutive LE u32 (offset, size, flag)`.

Entry-table record layout (uncompressed):
```
char    name[];    // asciz, length within table
uint32  offset;    // where compressed file data lives in th08.dat
uint32  size;      // DECOMPRESSED size of the file
uint32  flag;      // observed = 0 for all 317 entries in th08.dat (last byte reserved/unknown use)
```
NOTES:
- All 317 `flag` bytes are 0 in th08.dat. For th06 (*incl.*) the values differ; do not assume.
- No filled "crypt key byte" for th08 (checked; entry body is raw LZSS).
- `entries` array ends with a dummy record whose `offset` = table_offset and `size` = 0; used only so the
  "span = next.offset − this.offset" rule still works for the last entry (which is that entry, so its NEXT
  offset is the sentinel). **Note:** in-memory the entries array is an asciz char* followed by 12 bytes of ints.

## Per-entry payload

No additional obfuscation. A record's blob = raw LZSS stream read from `offset` to `offset + span`.
FUN_474af0 (`Archive::Read(name, out_hint)`):
1. `FUN_474c70` finds the entry with matching name (case-insensitive ASCII compare).
2. `span = entries[i+1].offset - entries[i].offset`
3. `GlobalAlloc(span)`, seek stream to `offset`, read `span` bytes.
4. `FUN_4740e0(ecx=raw, edx=span, [out = caller provided or 0, outsize = entry.size])` produces the final file.
Returns that buffer.

## LZSS decompressor (FUN_4740e0) — VERIFIED via disassembly + successful round-trip on all 317 files

- ring buffer `0x2000` bytes (8 KB), zero-filled (static / BSS), write position starts at **1** (not 0).
- bit reader maintains a per-bit mask starting at `0x80` for the *control* byte; each control byte feeds 8 flag bits MSB→LSB. When exhausted, a fresh control byte is fetched from input at the current input offset (beyond the limit, the flag byte is treated as 0 — see below).
- `flag bit == 1` → **literal**: read the next 8 bitstream bits (MSB-first) as a byte; append to output; also write to ring at the running `ring_pos` (starting at 1, `pos = (pos+1) & 0x1fff`).
- `flag bit == 0` → **match**:
  - read 13 bitstream bits MSB-first = `position` (absolute ring index).
  - `position == 0` → end of stream.
  - read 4 bitstream bits MSB-first = `length`; copy exactly `length + 3` bytes.
  - For k = 0..length+2: output `ring[(position + k) & 0x1fff]`, and also write each byte to the ring at `ring_pos` advancing it.
- input `limit` = number of bytes the decompressor may consume from the input buffer. Reads at or past the limit return 0 and do not advance; therefore a stream can end by either hitting the 13-bit `position == 0` marker or simply running out of permitted input (in the real game the `position == 0` marker occurs inside the span).
- The output buffer is pre-allocated with the caller-provided `decomp_size` (entry.size or table size). Neither the game nor our reimplementation should have to grow it. ZUN's games expect `decomp_size` to be the EXACT uncompressed size.

## Call-graph params as seen in the binary

Call site of the header decrypt (`FUN_474ce0` @ 0x474d89):
`FUN_43e1d0(ecx=&hdr12, edx=0x0c, [PUSH order 0x1b,0x37,0xc,0x400])` → these four stack args are (key, delta, block, guard) as shown by the callee's disassembly.
The push order corresponds to reading the args forward (first pushed = last read); the effective meanings are:
```
const uint8_t key = 0x1b, delta = 0x37;  const uint32_t block = 0x0c, guard = 0x400;   // header
const uint8_t key = 0x3e, delta = 0x9b;  const uint32_t block = 0x80, guard = 0x400;   // entry table
```
Call site of entry-table LZSS (`FUN_474ce0` @ 0x474eae):
`FUN_4740e0(ecx=decrypted_table, edx=span, [push decomp_size, push 0])`
Call site of per-entry LZSS (`FUN_474af0`):
`FUN_4740e0(ecx=raw_blob, edx=span, [push entry.size, push caller_out_or_0])`

## OEIS of the extracted table (spot checks)

- `ascii.anm`       offset 0x10 (16),       size 405,424
- `capture.anm`     offset 129,908,         size 1,132
- `demorpy0.rpy`    offset 130,357,         size 3,543
- last entries: `title01.anm` offset 45,139,141, size 7,386,448; `title00.png` 635,002;
  `thbgm.fmt` 1,113; `times.anm` 132,428; `th08_20.mid` 27,056.
</think><tool_call>write<arg_key>content</arg_key><arg_value># Touhou 8 (IN) — th08 dat archive format (`th08.dat`, magic `PBGZ`)
**VERIFIED** (extraction of the real 46,838,025-byte th08.dat in this spec dir: all 317 entries decompressed and sizes match, see `tools/extract_th08.py`).

Summary of what the archive looks like, from binary analysis (Ghidra headless on th08.exe).

## Top-level container

One file `th08.dat` = 16-byte header + contiguous LZSS-compressed file blobs + one small encrypted entry table at end-of-file.

| field | size | notes |
|--|--|--|
| magic | 4 | ASCII "PBGZ" |
| hdr buf | 12 | three LE u32 obfuscated (see below): (val0, val1, val2) |
| payloads | ... | one contiguous LZSS-compressed blob per file, lengths = size[i+1]-size[i]; no padding |
| entry table | rest | encrypted entry table (see table section), stored at table_offset (= file end - 4167B for th08.dat) |

- Offsets are absolute in the .dat file, SIZE is the DECOMPRESSED size of each entry (verified: all 317 sizes = actual lzss output).
- The first payload begins immediately at byte 0x10 (=16), no gap.
- File blobs are byte-contiguous; entry i's compressed span = offset[i+1] − offset[i]; there is no separate compressed-size field. VERIFIED in FUN_474af0 (`next.offset - this.offset` = GlobalAlloc'ed span).
- Entries are SORTED in the table by ascending offset and their compressed spans tile from 0x10 to `table_offset` (VERIFIED: max(offset)+span == table_offset, no gaps).
- th08.dat only: no per-file obfuscation. (Older Touhou 6 shipped an extra XOR layer gone here — verified `FUN_474af0` contains no `FUN_43e1d0` call.)

## Structure in memory

```
struct Archive {              // 0x14 or more
    Entry*  entries;          // +0
    uint    entry_count;      // +4
    ...
    Stream* stream;           // +12 (vtable at 0x4b98b4)
}
struct Entry {                // 16 bytes, FOUR consecutive u32/char* per record
    char*  name;              // +0  (asciz, shift-jis)
    uint32 offset;            // +4  (file offset of compressed blob)
    uint32 size;              // +8  (uncompressed size)
    uint32 flag;              // +0xc (unused/0 in th08.dat)
}
```
- The entry table is kept as `count+1` `Entry` values appended with a sentinel: `entries[count].offset = table_offset`, `size = 0`.

## Header (VERIFIED)

First 16 bytes:
- bytes 0..3  literal `"PBGZ"` (checked: 0x50 42 47 5a = ASCII, LE dword 0x5a474250).
- bytes 4..15 three LE u32 obfuscated.

Header decrypt (`FUN_43e1d0(ecx=buf, edx=12, key=0x1b, delta=0x37, block=0x0c, guard=0x400)`):

Per block (here: exactly one 12-byte block):
```
for i in 0 .. ceil(block/2)-1   : dest[block - 1 - 2i] = src[i] ^ key            (key += delta per byte)
for i in 0 .. floor(block/2)-1  : dest[block - 2 - 2i] = src[ceil + i] ^ key    (key += delta per byte)
```
i.e. the plain bytes are distributed in REVERSE interleaved slots with the running-XOR applied per byte.
After the block loop, a small residual tail (`len % block`, only when `len % block < block / 4`, plus a `len & 1` extra byte) is copied verbatim from the source. For the 12-byte header this tail is 0.

After decrypting the 3 u32s:
```
entry_count           = u32[0] - 123456
table_offset          = u32[1] - 345678
table_decomp_size     = u32[2] - 567891
```
For th08.dat this yields `317`, `0x2CAA0C2` (= filesize − 4167), `9063`.

## Entry table (VERIFIED)

Located at the very end of `th08.dat` (`table_offset` = 0x2CAA0C2).
1. Read `span = filesize - table_offset` bytes (= 4167 bytes) into a buffer.
2. Decrypt in place with `FUN_43e1d0(ecx=buf, edx=span, key=0x3e, delta=0x9b, block=0x80, guard=0x400)`:
   only first `guard / block` blocks fully covered = `0x400 / 0x80` = **8 blocks = 1024 bytes** get the interleave-reverse + running-XOR; the remaining `3143` bytes of the span are **verbatim plaintext** (verified: the decrypted data and the raw data match byte-for-byte after 1024). The main block loop keeps the same last-block reduction and `len & 1` tail rules as FUN_43e1d0 — the tail byte of 4167B (`4167 & 1`) is verbatim-copied as well.
3. LZSS-decompress the buffer (`FUN_4740e0(ecx=decrypted, edx=span, [out=0, outsize=table_decomp_size])`):
   output is `table_decomp_size` = 9063 bytes = the *uncompressed* entry table.
4. The uncompressed entry table is then parse (`FUN_474fa0(buf, count, table_offset)`):
   `for each of entry_count records: read name (asciz), then three consecutive LE u32 (offset, size, flag)`.

Entry-table record layout (uncompressed):
```
char    name[];    // asciz
uint32  offset;    // where compressed file data lives in th08.dat
uint32  size;      // DECOMPRESSED size of the file
uint32  flag;      // observed = 0 for all 317 entries in th08.dat
```
NOTES:
- All 317 `flag` values are 0 in th08.dat.
- No filled "crypt key byte" for th08 (checked; entry body is raw LZSS).
- `entries` array ends with a dummy record whose `offset` = table_offset and `size` = 0; used only so the "span = next.offset − this.offset" rule still works for the last entry. (In memory this is one asciz char* followed by 12 bytes of ints, one record per file.)

## Per-entry payload

No additional obfuscation. A record's blob = raw LZSS stream read from `offset` to `offset + span`.
FUN_474af0 (`Archive::Read(name, out_hint)`):
1. `FUN_474c70` finds the entry with matching name (case-insensitive compare).
2. `span = entries[i+1].offset - entries[i].offset`
3. `GlobalAlloc(span)`, seek stream to `offset`, read `span` bytes.
4. `FUN_4740e0(ecx=raw, edx=span, [out = caller provided or 0, outsize = entry.size])` produces the final file.
Returns that buffer.

## LZSS decompressor (FUN_4740e0) — VERIFIED via disassembly + successful round-trip on all 317 files

- ring buffer `0x2000` bytes (8 KB), zero-filled (static / BSS), write position starts at **1** (not 0).
- bit reader maintains a per-bit mask starting at `0x80` for the *control* byte; each control byte feeds 8 flag bits MSB→LSB. When exhausted, a fresh control byte is fetched from input at the current input offset (beyond the limit, the flag byte is treated as 0 — see below).
- `flag bit == 1` → **literal**: read the next 8 bitstream bits (MSB-first) as a byte; append to output; also write to ring at the running `ring_pos` (starting at 1, `pos = (pos+1) & 0x1fff`).
- `flag bit == 0` → **match**:
  - read 13 bitstream bits MSB-first = `position` (absolute ring index).
  - `position == 0` → end of stream.
  - read 4 bitstream bits MSB-first = `length`; copy exactly `length + 3` bytes.
  - For k = 0..length+2: output `ring[(position + k) & 0x1fff]`, and also write each byte to the ring at `ring_pos` advancing it.
- input `limit` = number of bytes the decompressor may consume from the input buffer. Reads at or past the limit return 0 and do not advance; therefore a stream can end by either hitting the 13-bit `position == 0` marker or simply running out of permitted input (in the real game the `position == 0` marker occurs inside the span).
- The output buffer is pre-allocated with the caller-provided `decomp_size` (entry.size or table size). The game expects `decomp_size` to be the EXACT uncompressed size.

## Call-graph params as seen in the binary

Call site of the header decrypt (`FUN_474ce0` @ 0x474d89):
`FUN_43e1d0(ecx=&hdr12, edx=0x0c, [PUSH order 0x1b,0x37,0xc,0x400])` → these four stack args are (key, delta, block, guard) as shown by the callee's disassembly.
Call site of entry-table LZSS (`FUN_474ce0` @ 0x474eae):
`FUN_4740e0(ecx=decrypted_table, edx=span, [push decomp_size, push 0])`
Call site of per-entry LZSS (`FUN_474af0`):
`FUN_4740e0(ecx=raw_blob, edx=span, [push entry.size, push caller_out_or_0])`

## Extracted table (spot checks, VERIFIED)

- `ascii.anm`       offset 0x10 (16),       size 405,424
- `capture.anm`     offset 129,908,         size 1,132
- `demorpy0.rpy`    offset 130,357,         size 3,543
- Last several entries: `th08_20.mid` 27,056; `thbgm.fmt` 1,113; `times.anm` 132,428;
  `title00.png` 635,002; `title01.anm` 7,386,448 (offset 45,139,141).
