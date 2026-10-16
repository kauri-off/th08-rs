#!/usr/bin/env python3
"""Header + entry-table decode probe for th08.dat (PBGZ). VERIFIED bits logged to NOTES.md."""
import struct, sys, os

DAT = "/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/th08.dat"

def blockdecode(src, key, delta, blocklen, guard=0x400):
    """sub_43e1d0: per-block interleave-reverse XOR.
    pass A: dest[d+block-1-2i] = src[s+i]^key  for i < ceil(block/2)
    pass B: dest[d+block-2-2i] = src[s+ceil+i]^key for i < floor(block/2)
    key advances +delta after each written byte."""
    dest = bytearray(len(src))
    dest = bytearray(len(src))
    key &= 0xff
    s = 0; d = 0; remaining = len(src); g = guard
    while remaining > 0 and g > 0:
        block = min(blocklen, remaining)
        ca = (block + 1) // 2
        cb = block // 2
        for i in range(ca):
            dest[d + block - 1 - 2*i] = src[s + i] ^ key
            key = (key + delta) & 0xff
        for i in range(cb):
            dest[d + block - 2 - 2*i] = src[s + ca + i] ^ key
            key = (key + delta) & 0xff
        s += block; d += blocklen; remaining -= block; g -= blocklen
    if remaining > 0:
        dest[d:d+remaining] = src[s:s+remaining]
    return bytes(dest)

RING = 0x2000
def lzss(src, limit):
    """sub_4740e0: ring zero-init, pos starts 1; flag bit per control bit MSB-first;
    1 => literal byte; 0 => 13-bit abs position P, 4-bit len+3, copy (P+k).. ; P==0 = end."""
    ring = bytearray(RING); pos = 1; out = bytearray(); i = 0; consumed = 0
    mask = 0x80; flag = 0
    def bit():
        nonlocal mask, flag, i, consumed
        if mask == 0x80:
            flag = src[i] if consumed < limit and i < len(src) else 0
            if consumed < limit: i += 1
            consumed += 1
        v = 1 if flag & mask else 0
        mask >>= 1
        if mask == 0: mask = 0x80
        return v
    while True:
        if bit():
            v = 0
            for _ in range(8): v = (v << 1) | bit()
            out.append(v); ring[pos] = v; pos = (pos+1) & 0x1fff
        else:
            p = 0
            for _ in range(13): p = (p << 1) | bit()
            if p == 0: break
            n = 0
            for _ in range(4): n = (n << 1) | bit()
            for k in range(n + 3):
                b = ring[(p + k) & 0x1fff]
                out.append(b); ring[pos] = b; pos = (pos+1) & 0x1fff
    return bytes(out)

def main():
    data = open(DAT, "rb").read()
    fs = len(data)
    assert data[:4] == b"PBGZ"
    hdr = blockdecode(data[4:16], 0x1b, 0x37, 0xc)
    v0, v1, v2 = struct.unpack("<III", hdr)
    count = v0 - 0x1e240      # 123456
    toff  = v1 - 0x5464e      # 345678
    tsize = v2 - 0x8aa53      # 567891
    print(f"count={count} table_off={toff} (0x{toff:x}) val3={tsize} "
          f"span={fs-toff} filesize={fs}")
    tbl_raw = data[toff:]
    dec = blockdecode(tbl_raw, 0x3e, 0x9b, 0x80)
    tbl = lzss(dec, fs - toff)
    print(f"decompressed table: {len(tbl)} bytes (val3 was {tsize})")
    open("/tmp/opencode/table.bin", "wb").write(tbl)
    # parse
    cur = 0; ents = []
    for i in range(count):
        e = tbl.index(0, cur)
        name = tbl[cur:e].decode("shift-jis", "replace")
        cur = e + 1
        off, size, flag = struct.unpack_from("<III", tbl, cur); cur += 12
        ents.append((name, off, size, flag))
    for n, o, s, f in ents[:15]:
        print(f"{o:9d} {s:9d} flag={f:2d} {n}")
    print("...")
    for n, o, s, f in ents[-5:]:
        print(f"{o:9d} {s:9d} flag={f:2d} {n}")
    print(f"max offset+size = {max(o+s for _,o,s,_ in ents)} vs table_off {toff}")
    print(f"names sorted ok: {ents == sorted(ents, key=lambda e: e[1])}")

if __name__ == "__main__":
    main()
