#!/usr/bin/env python3
"""Parse thbgm.fmt and verify against thbgm.dat. Usage: thbgm.py [fmt] [bgm] [--wav outdir]"""
import os, struct, sys

GAME = "/home/anon/Games/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/"
REC = 0x34


def parse(fmt):
    out = []
    for p in range(0, len(fmt) - REC + 1, REC):
        name = fmt[p:p + 16].split(b'\0')[0].decode()
        if not name:
            break
        off, size, lstart, lend = struct.unpack_from('<IIII', fmt, p + 0x10)
        tag, ch, rate, bps, align, bits, cb = struct.unpack_from('<HHIIHHH', fmt, p + 0x20)
        out.append(dict(name=name, off=off, size=size, loop_start=lstart, loop_end=lend,
                        tag=tag, ch=ch, rate=rate, bps=bps, align=align, bits=bits, cb=cb))
    return out


def main():
    a = [x for x in sys.argv[1:] if not x.startswith('--')]
    fmtp = a[0] if a else os.path.join(os.path.dirname(__file__), '..', 'extracted', 'dat', 'thbgm.fmt')
    bgmp = a[1] if len(a) > 1 else GAME + 'thbgm.dat'
    tracks = parse(open(fmtp, 'rb').read())
    bsize = os.path.getsize(bgmp)
    print('thbgm.dat size=%d, %d tracks' % (bsize, len(tracks)))
    prev_end = 0x10
    for t in tracks:
        sec = lambda b: b / t['bps']
        ok = t['off'] + t['size'] <= bsize and t['loop_end'] <= t['size'] and t['loop_start'] < t['loop_end']
        print('%-12s off=0x%08x size=%9d (%6.2fs) loop=[%9d,%9d) (%6.2fs-%6.2fs) %dHz %dch %dbit align%d gap=%d %s' % (
            t['name'], t['off'], t['size'], sec(t['size']), t['loop_start'], t['loop_end'],
            sec(t['loop_start']), sec(t['loop_end']), t['rate'], t['ch'], t['bits'], t['align'],
            t['off'] - prev_end, 'OK' if ok else 'BAD'))
        prev_end = t['off'] + t['size']
    print('end of last track=0x%x file size=0x%x' % (prev_end, bsize))
    if '--wav' in sys.argv:
        outdir = sys.argv[sys.argv.index('--wav') + 1]
        os.makedirs(outdir, exist_ok=True)
        with open(bgmp, 'rb') as f:
            for t in tracks:
                f.seek(t['off']); pcm = f.read(t['size'])
                fmtck = struct.pack('<HHIIHH', t['tag'], t['ch'], t['rate'], t['bps'], t['align'], t['bits'])
                body = b'WAVE' + b'fmt ' + struct.pack('<I', 16) + fmtck + b'data' + struct.pack('<I', len(pcm)) + pcm
                open(os.path.join(outdir, t['name']), 'wb').write(b'RIFF' + struct.pack('<I', len(body)) + body)


if __name__ == '__main__':
    main()
