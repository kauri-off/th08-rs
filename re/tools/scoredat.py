#!/usr/bin/env python3
# score.dat decoder/validator (see 15_files_replay_score.md §2)
import sys, struct, os
sys.path.insert(0, os.path.dirname(__file__))
from th08dat import decrypt, lzss

DEFAULT = "/home/anon/Games/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/score.dat"
CHUNKS = {b'TH8K': (0xC, 1), b'HSCR': (0x168, 4), b'CLRD': (0x24, 4), b'CATK': (0x22C, 3),
          b'PSCR': (0x178, 2), b'LSNM': (0x18, 1), b'FLSP': (0x20, 1), b'PLST': (0x228, 2), b'VRSM': (0x1C, 1)}


def decode(raw):
    b = bytearray(decrypt(raw, 0x59, 0x79, 0x100, 0xC00))
    acc = 0
    for i in range(1, len(b) - 1):
        acc = (acc + b[i]) & 0xFF
        acc = ((acc << 3) | (acc >> 5)) & 0xFF
        b[i + 1] ^= acc
    return b


def parse(raw, out=print):
    probs = []
    b = decode(raw)
    csum = sum(b[4:]) & 0xFFFF
    if csum != struct.unpack_from('<H', b, 2)[0]:
        probs.append('checksum')
    ver, hsz, _ptr, tot, body, csz = struct.unpack_from('<HxxIIIII', b, 4)
    if ver != 1 or hsz != 0x1C:
        probs.append('header ver/size')
    if csz != len(b) - 0x1C or body != tot - 0x1C:
        probs.append('sizes')
    d = bytes(b[:0x1C]) + lzss(bytes(b[0x1C:]), tot - 0x1C)
    out('header key=%02x sum=%04x rnd=%02x total=%d compressed=%d' % (b[1], csum, b[6], tot, csz))
    p, counts, ids = 0x1C, {}, set()
    while p < tot:
        m, s1, s2, v = struct.unpack_from('<4sHHB', d, p)
        if s1 == 0:
            probs.append('zero-size chunk @%x' % p)
            break
        exp = CHUNKS.get(m)
        if exp is None or exp != (s1, v) or s1 != s2:
            probs.append('bad chunk %r size %x/%x v%d @%x' % (m, s1, s2, v, p))
        c = d[p:p + s1]
        counts[m] = counts.get(m, 0) + 1
        if m == b'HSCR':
            sc, slow, shot, diff, stg = struct.unpack_from('<IfBBB', c, 0xC)
            name = c[0x17:0x20].split(b'\0')[0].decode('cp932', 'replace')
            date = c[0x20:0x26].split(b'\0')[0].decode()
            if shot > 11 or diff > 4 or not (stg <= 8 or stg == 99):
                probs.append('HSCR range')
            out('HSCR d%d s%2d %10d0 stage %2d %-8s %s slow %.2f%% cont %d' % (diff, shot, sc, stg, name, date, slow, c[0x26]))
        elif m == b'CATK':
            cid, ck = struct.unpack_from('<HB', c, 0xC)
            arr = struct.unpack_from('<78I', c, 0xF0)
            nm = bytes(x for x in c[0x10:0x40] if x).split(b'\0')[0]
            s = (sum((x - 256 if x > 127 else x) for x in c[0x10:0x40]) + sum(arr)) & 0xFF
            if s != ck or cid > 221 or cid in ids:
                probs.append('CATK %d checksum/id' % cid)
            ids.add(cid)
            if any(v > 9999 for v in arr[13:]):
                probs.append('CATK %d cap' % cid)
        elif m == b'CLRD':
            if c[0x21] > 12:
                probs.append('CLRD index')
            out('CLRD %2d nc=%s any=%s bad=%d' % (c[0x21], [hex(x) for x in struct.unpack_from('<5H', c, 0xC)],
                                                   [hex(x) for x in struct.unpack_from('<5H', c, 0x16)], c[0x20]))
        elif m == b'FLSP':
            out('FLSP %s' % c[0xC:0x1D].hex(' '))
            for k, x in enumerate(c[0xC:0x1D]):
                if x not in (0, 0xCD + k):
                    probs.append('FLSP slot %d' % k)
        elif m == b'LSNM':
            out('LSNM %r' % c[0xC:0x18].split(b'\0')[0])
        elif m == b'PLST':
            out('PLST run %d:%d:%d.%d play %d:%d:%d.%d' % (struct.unpack_from('<4I', c, 0xC) + struct.unpack_from('<4I', c, 0x1C)))
            for k in range(7):
                out('PLST blk%d %s' % (k, struct.unpack_from('<17I', c, 0x2C + k * 0x44)))
            blk = [struct.unpack_from('<17I', c, 0x2C + k * 0x44) for k in range(7)]
            if any(blk[6][i] != sum(blk[k][i] for k in range(6)) for i in range(17)):
                probs.append('PLST total block')
            out('PLST bgm %s' % c[0x208:0x228].hex())
        elif m == b'VRSM':
            out('VRSM %r size %d sum %08x' % ((c[0xC:0x14].split(b'\0')[0],) + struct.unpack_from('<II', c, 0x14)))
        elif m == b'PSCR':
            if c[0x174] > 11:
                probs.append('PSCR shot')
        p += s1
    if p != tot:
        probs.append('chunk walk end %x != %x' % (p, tot))
    if counts.get(b'TH8K') != 1 or counts.get(b'CLRD') != 13:
        probs.append('chunk counts %r' % counts)
    out('chunks %r' % {k.decode(): v for k, v in counts.items()})
    return d, probs


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if a != '-q']
    path = args[0] if args else DEFAULT
    quiet = '-q' in sys.argv
    d, probs = parse(open(path, 'rb').read(), (lambda *a: None) if quiet else print)
    print('%s: %d problems %s' % (os.path.basename(path), len(probs), probs))
    sys.exit(1 if probs else 0)
