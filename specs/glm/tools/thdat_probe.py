#!/usr/bin/env python3
"""
th08.dat extractor, reverse-engineered from th08.exe (verified claims:
- magic "PBGZ" + 12-byte triple-encrypted header via per-block interleave-reverse XOR
- entry table stored LZ-compressed at end of file, records {name\0, u32 offset,
  u32 size, u32 flag}
- LZSS variant: 8192-byte zero ring, absolute 13-bit position, 4-bit length+3,
  end marker position==0, input limited to compressed size
Usage:
  python3 thdat_extract.py <th08.dat> [outdir]
Writes all files and prints the index as CSV on stdout.
"""
import struct, sys, os

def blockdecode(src, key, delta, blocklen, guard=0x400):
    """sub_43e1d0: per-block interleave-reverse XOR decode.
    Exact reference is sub_43e1d0 at VA 0x43e1d0 in th08.exe (x86)."""
    dest = bytearray(len(src))
    key &= 0xff
    s = 0
    d = 0
    remaining = len(src)
    while remaining > 0 and guard > 0:
        block = min(blocklen, remaining)
        ca = (block + 1) // 2
        cb = block // 2
        for i in range(ca):
            dest[d + block - 1 - 2 * i] = src[s + i] ^ key
            key = (key + delta) & 0xff
        for i in range(cb):
            dest[d + block - 2 - 2 * i] = src[s + i] ^ key
            key = (key + delta) & 0xff
        s += block
        d += blocklen
        remaining -= block
        guard -= blocklen
    if remaining > 0 and s < len(src):
        dest[d:d + remaining] = src[s:s + remaining]
    return bytes(dest)

RING = 0x2000

def lzss(src, limit, ctx=None):
    """sub_4740e0 at VA 0x4740e0. Ring zero-initialised per process; pos starts 1."""
    ring = bytearray(RING)
    pos = 1
    out = bytearray()
    i = 0
    consumed = 0

    def rd():
        nonlocal i, consumed
        if consumed >= limit:
            return 0, False
        b = src[i] if i < len(src) else 0
        i += 1
        consumed += 1
        return b, True

    mask = 0x80
    flagbyte = 0

    def bit():
        nonlocal mask, flagbyte
        if mask == 0x80:
            b, ok = rd()
            flagbyte = b
        v = 1 if (flagbyte & mask) else 0
        mask >>= 1
        if mask == 0:
            mask = 0x80
        return v

    while True:
        if bit():
            v, ok = rd()
            out.append(v)
            ring[pos] = v
            pos = (pos + 1) & (RING - 1)
        else:
            p = 0
            for k in range(13):
                p = (p << 1) | bit()
            if p == 0:
                break
            l4 = 0
            for k in range(4):
                l4 = (l4 << 1) | bit()
            n = l4 + 3
            for k in range(n):
                b = ring[(p + k) & (RING - 1)]
                out.append(b)
                ring[pos] = b
                pos = (pos + 1) & (RING - 1)
    return out

def parse_table(tbl, count, table_offset):
    """sub_474fa0: records {NUL-terminated name, u32 offset, u32 size, u32 flag}."""
    entries = []
    cur = 0
    for i in range(count):
        end = tbl.index(0, cur)
        name = tbl[cur:end].decode("shift-jis", "replace")
        cur = end + 1
        off, size, flag = struct.unpack_from("<III", tbl, cur)
        cur += 12
        entries.append((name, off, size, flag))
    return entries

def main():
    path = sys.argv[1]
    outdir = sys.argv[2] if len(sys.argv) > 2 else "."
    data = open(path, "rb").read()
    assert data[:4] == b"PBGZ", data[:4]
    hdr = blockdecode(data[4:16], 0x1b, 0x37, 0xc)
    f1, f2, f3 = struct.unpack("<III", hdr)
    count = f1 - 123456
    table_offset = f2 - 345678
    tbl_comp_limit = f3 - 567891
    print(f"# count={count} table_offset={table_offset} tbl_comp_limit={tbl_comp_limit}",
          file=sys.stderr)

    raw = data[table_offset:]
    dec = blockdecode(raw, 0x3e, 0x9b, 0x80)
    tbl = lzss(dec, tbl_comp_limit)
    entries = parse_table(tbl, count, table_offset)

    os.makedirs(outdir, exist_ok=True)
    for (name, off, size, flag) in entries:
        compressed = data[off:off + (table_offset - off)] if off < table_offset else b""
        if off >= table_offset:
            print(f"{off},{0},{size},{name}!!outofrange", file=sys.stderr)
            continue
        span = min(table_offset - off, 0x10000000)
        raw2 = data[off:off + span]
        if size == len(raw2) and b"PBGZ" not in raw2[:4] or True:
            out = lzss(raw2, size)
        if len(out) != size:
            print(f"{name}: out {len(out)} != size {size}", file=sys.stderr)
        safe = name.replace("/", "_")
        with open(os.path.join(outdir, safe), "wb") as fp:
            fp.write(out)
        print(f"{off},{size},{len(out)},{flag},{name}")

if __name__ == "__main__":
    main()
