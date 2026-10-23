#!/usr/bin/env python3
"""th08.dat (PBGZ) extractor. Usage: th08dat.py [list|extract] [dat] [outdir]"""
import os, struct, sys

DAT = "/home/anon/Games/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/th08.dat"

CRYPT = {
    ord('M'): (0x1b, 0x37, 0x40, 0x2800),
    ord('T'): (0x51, 0xe9, 0x40, 0x3000),
    ord('A'): (0xc1, 0x51, 0x1400, 0x2000),
    ord('J'): (0x03, 0x19, 0x1400, 0x7800),
    ord('E'): (0xab, 0xcd, 0x200, 0x1000),
    ord('W'): (0x12, 0x34, 0x400, 0x2800),
    ord('-'): (0x35, 0x97, 0x80, 0x2800),
    ord('*'): (0x99, 0x37, 0x400, 0x1000),
}


def decrypt(src, key, step, block, limit):
    size = len(src)
    rem = size % block
    tail = rem if rem < block // 4 else 0
    tail += size & 1
    n = size - tail
    out = bytearray(size)
    si = 0
    base = 0
    while n > 0 and limit > 0:
        if n < block:
            block = n
        p = base + block - 1
        for _ in range((block + 1) // 2):
            out[p] = src[si] ^ key
            p -= 2; key = (key + step) & 0xff; si += 1
        p = base + block - 2
        for _ in range(block // 2):
            out[p] = src[si] ^ key
            p -= 2; key = (key + step) & 0xff; si += 1
        n -= block
        limit -= block
        base += block
    out[base:] = src[si:]
    return bytes(out)


def lzss(src, outsize):
    dic = bytearray(0x2000)
    dp = 1
    out = bytearray()
    bitpos = 0
    nbits = len(src) * 8

    def bits(k):
        nonlocal bitpos
        v = 0
        for _ in range(k):
            if bitpos < nbits:
                b = (src[bitpos >> 3] >> (7 - (bitpos & 7))) & 1
            else:
                b = 0
            v = (v << 1) | b
            bitpos += 1
        return v

    while len(out) < outsize:
        if bits(1):
            c = bits(8)
            out.append(c); dic[dp] = c; dp = (dp + 1) & 0x1fff
        else:
            off = bits(13)
            if off == 0:
                break
            ln = bits(4) + 3
            for i in range(ln):
                c = dic[(off + i) & 0x1fff]
                out.append(c); dic[dp] = c; dp = (dp + 1) & 0x1fff
    return bytes(out)


def open_archive(path=DAT):
    d = open(path, 'rb').read()
    if d[:4] != b'PBGZ':
        raise ValueError('bad magic')
    h = decrypt(d[4:16], 0x1b, 0x37, 0x0c, 0x400)
    a, b, c = struct.unpack('<III', h)
    count, toff, tsize = a - 123456, b - 345678, c - 567891
    table = lzss(decrypt(d[toff:], 0x3e, 0x9b, 0x80, 0x400), tsize)
    entries = []
    p = 0
    for _ in range(count):
        e = table.index(b'\0', p)
        name = table[p:e].decode('ascii')
        off, size, unk = struct.unpack_from('<III', table, e + 1)
        p = e + 13
        entries.append([name, off, size, unk])
    for i, ent in enumerate(entries):
        nxt = entries[i + 1][1] if i + 1 < count else toff
        ent.append(nxt - ent[1])
    return d, (count, toff, tsize), entries


def read_file(d, ent):
    name, off, size, unk, csize = ent
    raw = lzss(d[off:off + csize], size)
    if len(raw) >= 4 and raw[:3] == b'edz' and raw[3] in CRYPT:
        raw = decrypt(raw[4:], *CRYPT[raw[3]])
    return raw


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else 'list'
    path = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else DAT
    d, hdr, entries = open_archive(path)
    print('count=%d table_off=0x%x table_size=%d' % hdr)
    if cmd == 'list':
        for name, off, size, unk, csize in entries:
            print('%-20s off=0x%08x size=%8d csize=%8d unk=0x%08x' % (name, off, size, csize, unk))
    elif cmd == 'extract':
        outdir = sys.argv[3]
        os.makedirs(outdir, exist_ok=True)
        for ent in entries:
            raw = lzss(d[ent[1]:ent[1] + ent[4]], ent[2])
            tag = chr(raw[3]) if raw[:3] == b'edz' else '?'
            data = read_file(d, ent)
            open(os.path.join(outdir, ent[0]), 'wb').write(data)
            print('%-20s %8d -> %8d  edz=%s head=%s' % (ent[0], ent[2], len(data), tag, data[:8].hex()))


if __name__ == '__main__':
    main()
