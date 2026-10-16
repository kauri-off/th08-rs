#!/usr/bin/env python3
"""th08.dat (PBGZ) extractor for Touhou 8 — Imperishable Night.

Single pass: container header -> entry table -> all 317 files, fully decoded
(all wrapper layers removed: header decrypt, table decrypt, LZSS).

Usage: python3 extract_th08.py <th08.dat> <outdir>
       python3 extract_th08.py <th08.dat> <outdir> --list

Format reference: specs/glm/01_archive.md (all claims VERIFIED by this tool).
Entry table is stored encrypted+LZSS at the end of the archive; no per-entry crypt.
"""
import os, struct, sys

MAGIC_COUNT = 123456      # subtracted from header u32[0]
MAGIC_OFFS  = 345678      # subtracted from header u32[1]
MAGIC_SIZE  = 567891      # subtracted from header u32[2]

def crypt(src, key, delta, block, guard=0x400):
    """FUN_43e1d0 (interleave-reverse + running XOR, guard-limited). VERIFIED."""
    dst = bytearray(len(src))
    s = d = 0
    rem = len(src)
    # tail rule: last partial block (if small) + odd-length extra byte copied verbatim
    tail = (rem % block) if (rem % block) < (block // 4) else 0
    tail += rem & 1
    body = rem - tail
    while body > 0 and guard > 0:
        n = min(block, body)
        ca = (n + 1) // 2
        for i in range(ca):
            dst[d + n - 1 - 2 * i] = src[s + i] ^ (key & 0xFF)
            key = (key + delta) & 0xFF
        for i in range(block // 2):
            if i < (n - ca):
                dst[d + n - 2 - 2 * i] = src[s + ca + i] ^ (key & 0xFF)
                key = (key + delta) & 0xFF
        s += n; d += block; body -= n; guard -= block
    if rem - (s) > 0 or tail > 0:
        start = s
        dst[start:start + (len(src) - start)] = src[start:]
    return bytes(dst)

def lzss(src, limit):
    """FUN_4740e0. VERIFIED. Literal = 8 bits MSB-first; match = 13-bit abs pos + 4-bit len+3; pos 0 = end."""
    ring = bytearray(0x2000)
    pos = 1
    out = bytearray()
    i = 0
    mask = 0x80
    flag = 0

    def bit():
        nonlocal i, mask, flag
        if mask == 0x80:
            if i < limit and i < len(src):
                flag = src[i]; i += 1
            else:
                flag = 0
        v = 1 if (flag & mask) else 0
        mask >>= 1
        if mask == 0:
            mask = 0x80
        return v

    while True:
        if bit():
            v = 0
            for _ in range(8):
                v = (v << 1) | bit()
            out.append(v)
            ring[pos] = v
            pos = (pos + 1) & 0x1FFF
        else:
            p = 0
            for _ in range(13):
                p = (p << 1) | bit()
            if p == 0:
                break
            n = 0
            for _ in range(4):
                n = (n << 1) | bit()
            for _ in range(n + 3):
                b = ring[(p) & 0x1FFF]
                out.append(b)
                ring[pos] = b
                pos = (pos + 1) & 0x1FFF
                p = (p + 1) & 0x1FFF
    return bytes(out)

def parse_table(tbl, count, table_offset):
    entries = []
    cur = 0
    for _ in range(count):
        end = tbl.index(0, cur)
        name = tbl[cur:end].decode("shift-jis", "replace")
        cur = end + 1
        off, size, extra = struct.unpack_from("<III", tbl, cur)
        cur += 12
        entries.append((name, off, size, extra))
    return entries

def main():
    all_args = sys.argv[1:]
    list_only = "--list" in all_args
    all_args = [a for a in all_args if a != "--list"]
    if len(all_args) < 2:
        print(__doc__)
        return 1
    src, outdir = all_args[0], all_args[1]
    data = open(src, "rb").read()
    if data[:4] != b"PBGZ":
        print("PBGZ magic not found", file=sys.stderr)
        return 1
    # header
    hdr = crypt(data[4:16], 0x1B, 0x37, 0x0C, 0x400)
    v0, v1, v2 = struct.unpack("<III", hdr)
    count = v0 - MAGIC_COUNT
    tbl_off = v1 - MAGIC_OFFS
    tbl_size = v2 - MAGIC_SIZE
    if not (0 < count < 1_000_000 and 0 < tbl_off < len(data) and 0 < tbl_size < 10_000_000):
        print(f"insane header (count={count} tbl_off={tbl_off:#x} tbl_size={tbl_size})", file=sys.stderr)
        return 1
    # entry table
    span = len(data) - tbl_off
    tbl = lzss(crypt(data[tbl_off:], 0x3E, 0x9B, 0x80, 0x400), span)
    if len(tbl) != tbl_size:
        print(f"table decompressed to {len(tbl)} bytes, header said {tbl_size}", file=sys.stderr)
    entries = parse_table(tbl, count, tbl_off)
    # extracts
    if not list_only:
        os.makedirs(outdir, exist_ok=True)
    ok = 0
    for idx, (name, off, size, extra) in enumerate(entries):
        safe = name.replace("\\", "_").replace("/", "_")
        if list_only:
            print(f"{off:10d} {size:10d} {name}")
            continue
        if idx + 1 < count:
            span_i = entries[idx + 1][1] - off
        else:
            span_i = tbl_off - off
        if span_i <= 0:
            span_i = tbl_off - off
        raw = data[off:off + span_i]
        out = lzss(raw, len(raw))
        if len(out) != size:
            print(f"WARN {name}: lzss {len(out)} bytes != header size {size}", file=sys.stderr)
        else:
            ok += 1
        with open(os.path.join(outdir, safe), "wb") as fp:
            fp.write(out)
    if not list_only:
        print(f"extracted {ok}/{count} files to {outdir}")
    else:
        print(f"{count} entries")
    return 0

if __name__ == "__main__":
    sys.exit(main())
