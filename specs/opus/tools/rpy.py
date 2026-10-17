#!/usr/bin/env python3
# th8 replay decoder/validator (see 15_files_replay_score.md §3)
import sys, struct, os
sys.path.insert(0, os.path.dirname(__file__))
from th08dat import lzss

STAGES = ['1', '2', '3', '4A', '4B', '5', '6A', '6B', 'Ex']


def parse(raw, out=print):
    probs = []
    b = bytearray(raw)
    if b[:4] != b'T8RP' or struct.unpack_from('<H', b, 4)[0] != 6:
        return None, ['magic/version']
    size = struct.unpack_from('<I', b, 0xC)[0]
    key = b[0x15]
    for i in range(0x18, size):
        b[i] = (b[i] - key) & 0xFF
        key = (key + 7) & 0xFF
    if (0x3F000318 + sum(b[0x15:size])) & 0xFFFFFFFF != struct.unpack_from('<I', b, 0x10)[0]:
        probs.append('checksum')
    csz, usz = struct.unpack_from('<II', b, 0x18)
    if 0x68 + csz != size:
        probs.append('compressed size')
    d = bytes(b[:0x68]) + lzss(bytes(b[0x68:0x68 + csz]), usz)
    if len(d) != 0x68 + usz:
        probs.append('decompressed size')
    st = struct.unpack_from('<9I', d, 0x20)
    fp = struct.unpack_from('<9I', d, 0x44)
    shot, diff = d[0x6A], d[0x6B]
    name = d[0x72:0x7B].split(b'\0')[0].decode('cp932', 'replace')
    date = d[0x6C:0x72].split(b'\0')[0].decode()
    spell = struct.unpack_from('<h', d, 0x7C)[0]
    score = struct.unpack_from('<I', d, 0xB0)[0]
    slow = struct.unpack_from('<f', d, 0x118)[0]
    out('name %-8s date %s shot %d diff %d practice %d spell %d score %d0 slow %.3f%% clear %d tag %r' %
        (name, date, shot, diff, d[0x7B], spell, score, slow, d[0x11C], d[0x12C:0x134].split(b'\0')[0]))
    if shot > 11 or diff > 4 or d[0xD9] != 0 or d[0x6]:
        probs.append('info range / slow / debug')
    debug = d[0x12C:0x131] == b'debug'
    if (not debug and struct.unpack_from('<H', d, 0xAE)[0] != 0x100) or struct.unpack_from('<I', d, 0x120)[0] != 30:
        probs.append('info constants')
    used = sorted((o, 's', k) for k, o in enumerate(st) if o) + sorted((o, 'f', k) for k, o in enumerate(fp) if o)
    used.sort()
    if used and used[0][0] != 0x134:
        probs.append('first offset')
    for n, (o, kind, k) in enumerate(used):
        end = used[n + 1][0] if n + 1 < len(used) else len(d)
        if kind == 's':
            r = d[o:o + 0x24]
            sc, pi, gz, ext, nxt, mpv, gauge, seed, pw, lv, bm, rank, f20, sp, clk = struct.unpack_from('<IiIiiihHBBBBBBb', r)
            words = struct.unpack_from('<%dH' % ((end - o - 0x24) // 2), d, o + 0x24)
            if (end - o - 0x24) % 2 or words[0] != 0:
                probs.append('stage %s input stream align/dummy' % STAGES[k])
            # stream: dummy 0, frames..., terminator 0, then 4 uninitialised bytes
            if words[-3] != 0 or any(w & ~0x1FFF for w in words[:-2]):
                probs.append('stage %s input bits' % STAGES[k])
            nz = len(words)
            while nz and words[nz - 1] == 0:
                nz -= 1
            out('stage %-2s @%05x score(end) %d0 seed %04x pw %d lives %d bombs %d rank %d gauge %d frames %d' %
                (STAGES[k], o, sc, seed, pw, lv, bm, rank, gauge, len(words) - 4))
        else:
            fb = d[o:end]
            out('fps   %-2s @%05x %d bytes, min %d, flagged %d' %
                (STAGES[k], o, len(fb), min(x & 0x7F for x in fb) if fb else 0, sum(x >> 7 for x in fb)))
    if size < len(raw):
        tail = bytes(raw[size:])
        if tail[:4] != b'USER' or struct.unpack_from('<I', tail, 4)[0] != len(tail):
            probs.append('USER section')
        else:
            out('USER type %d: %r' % (tail[8], tail[12:].split(b'\0')[0].decode('cp932', 'replace')[:200]))
    return d, probs


if __name__ == '__main__':
    bad = 0
    quiet = '-q' in sys.argv
    for fn in [a for a in sys.argv[1:] if a != '-q']:
        d, probs = parse(open(fn, 'rb').read(), (lambda *a: None) if quiet else print)
        print('%s: %d problems %s' % (os.path.basename(fn), len(probs), probs))
        bad += len(probs)
    sys.exit(1 if bad else 0)
