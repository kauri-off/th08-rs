#!/usr/bin/env python3
"""ANM/THTX parser. Usage: anm.py <file.anm|dir> [--png outdir] [--scripts]"""
import os, struct, sys, zlib

FMT = {1: ('A8R8G8B8', 4), 3: ('R5G6B5', 2), 5: ('A4R4G4B4', 2), 2: ('A1R5G5B5', 2), 4: ('R8G8B8', 3)}


def parse(d):
    entries = []
    base = 0
    while True:
        h = struct.unpack_from('<16I', d, base)
        e = dict(base=base, nsprites=h[0], nscripts=h[1], zero8=h[2], w=h[3], h=h[4], fmt=h[5],
                 colorkey=h[6], name_off=h[7], x=h[8], y=h[9], version=h[10], priority=h[11],
                 thtx_off=h[12], has_thtx=h[13] & 0xff, next=h[14], unk3c=h[15])
        e['name'] = d[base + h[7]:].split(b'\0')[0].decode('ascii', 'replace')
        p = base + 0x40
        e['sprites'] = []
        for i in range(h[0]):
            so = struct.unpack_from('<I', d, p + 4 * i)[0]
            e['sprites'].append(struct.unpack_from('<Iffff', d, base + so))
        p += 4 * h[0]
        e['scripts'] = [struct.unpack_from('<iI', d, p + 8 * i) for i in range(h[1])]
        if e['has_thtx']:
            t = base + h[12]
            magic, z, f, tw, th, sz = struct.unpack_from('<4sHHHHI', d, t)
            e['thtx'] = dict(magic=magic, zero=z, fmt=f, w=tw, h=th, size=sz, data_off=t + 16)
        entries.append(e)
        if h[14] == 0:
            break
        base += h[14]
    return entries


def script_ops(d, start, limit):
    ops = []
    p = start
    while p + 8 <= limit:
        op, sz, t, mask = struct.unpack_from('<HHhH', d, p)
        ops.append((op if op != 0xffff else -1, sz, t, mask, d[p + 8:p + sz] if sz >= 8 else b''))
        if op == 0xffff or sz == 0:
            break
        p += sz
    return ops


def to_png(t, d, path):
    name, bpp = FMT[t['fmt']]
    w, h = t['w'], t['h']
    raw = d[t['data_off']:t['data_off'] + t['size']]
    rows = []
    for y in range(h):
        row = bytearray(b'\0')
        for x in range(w):
            o = (y * w + x) * bpp
            if t['fmt'] == 1:
                b, g, r, a = raw[o:o + 4]
            elif t['fmt'] == 4:
                b, g, r = raw[o:o + 3]; a = 255
            else:
                v = raw[o] | raw[o + 1] << 8
                if t['fmt'] == 5:
                    a, r, g, b = ((v >> 12) & 15) * 17, ((v >> 8) & 15) * 17, ((v >> 4) & 15) * 17, (v & 15) * 17
                elif t['fmt'] == 3:
                    r, g, b, a = (v >> 11) * 255 // 31, ((v >> 5) & 63) * 255 // 63, (v & 31) * 255 // 31, 255
                else:
                    a, r, g, b = (v >> 15) * 255, ((v >> 10) & 31) * 255 // 31, ((v >> 5) & 31) * 255 // 31, (v & 31) * 255 // 31
            row += bytes((r, g, b, a))
        rows.append(bytes(row))
    def chunk(tag, data):
        c = tag + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)) + \
        chunk(b'IDAT', zlib.compress(b''.join(rows), 6)) + chunk(b'IEND', b'')
    open(path, 'wb').write(png)


def main():
    a = sys.argv[1]
    files = [os.path.join(a, f) for f in sorted(os.listdir(a)) if f.endswith('.anm')] if os.path.isdir(a) else [a]
    pngdir = sys.argv[sys.argv.index('--png') + 1] if '--png' in sys.argv else None
    problems = 0
    for fn in files:
        d = open(fn, 'rb').read()
        for i, e in enumerate(parse(d)):
            t = e.get('thtx')
            msg = ''
            if e['version'] != 3: msg += ' BADVER'
            if t:
                if t['magic'] != b'THTX': msg += ' BADMAGIC'
                if t['size'] != t['w'] * t['h'] * FMT[t['fmt']][1]: msg += ' BADSIZE'
                if t['fmt'] != e['fmt']: msg += ' FMTDIFF'
            problems += bool(msg)
            print('%-18s #%d name=%-32s hdr %4dx%-4d fmt=%d ck=%08x pos=(%d,%d) prio=%d spr=%3d scr=%3d thtx=%s%s' % (
                os.path.basename(fn), i, e['name'], e['w'], e['h'], e['fmt'], e['colorkey'], e['x'], e['y'],
                e['priority'], e['nsprites'], e['nscripts'],
                ('%dx%d f%d' % (t['w'], t['h'], t['fmt'])) if t else '-', msg))
            if pngdir and t:
                os.makedirs(pngdir, exist_ok=True)
                to_png(t, d, os.path.join(pngdir, '%s_%d.png' % (os.path.basename(fn), i)))
            if '--scripts' in sys.argv:
                for sid, so in e['scripts']:
                    print('   script id=%d off=0x%x' % (sid, so))
                    for op in script_ops(d, e['base'] + so, len(d)):
                        print('     op=%d size=%d time=%d mask=%x args=%s' % (op[0], op[1], op[2], op[3], op[4].hex()))
    print('problems:', problems)


if __name__ == '__main__':
    main()
