#!/usr/bin/env python3
"""STD (stage background) parser/dumper. Usage: stddump.py <file.std|dir> [-q]"""
import os, struct, sys, collections

OPS = {0: ('set_origin', 'fff'), 1: ('fog', 'cff'), 2: ('fog_time', 'i__'), 3: ('halt', '___'),
       4: ('jump', 'ii_'), 5: ('cam_pos', 'fff'), 6: ('cam_pos_time', 'ii_'), 7: ('cam_facing', 'fff'),
       8: ('cam_facing_time', 'ii_'), 9: ('cam_up', 'fff'), 10: ('cam_up_time', 'ii_'), 11: ('fov', 'f__'),
       12: ('fov_time', 'ii_'), 13: ('clear_color', 'c__'), 14: ('cam_pos_start', 'fff'), 15: ('cam_pos_end', 'fff'),
       16: ('cam_pos_tan0', 'fff'), 17: ('cam_pos_tan1', 'fff'), 18: ('cam_pos_hermite', 'i__'),
       19: ('cam_facing_start', 'fff'), 20: ('cam_facing_end', 'fff'), 21: ('cam_facing_tan0', 'fff'),
       22: ('cam_facing_tan1', 'fff'), 23: ('cam_facing_hermite', 'i__'), 24: ('cam_up_start', 'fff'),
       25: ('cam_up_end', 'fff'), 26: ('cam_up_tan0', 'fff'), 27: ('cam_up_tan1', 'fff'), 28: ('cam_up_hermite', 'i__'),
       29: ('bg_anm_a', 'i__'), 30: ('bg_anm_b', 'i__'), 31: ('section', 'i__'), 32: ('cam_eye_offset', 'fff'),
       33: ('sway', 'i__'), 34: ('bg_anm_c', 'i__')}


def parse(d):
    nobj, nquad, inst_off, scr_off, z = struct.unpack_from('<hhIII', d, 0)
    h = dict(nobj=nobj, nquad=nquad, inst_off=inst_off, scr_off=scr_off, zero=z,
             texts=[d[0x10 + 0x80 * i:0x90 + 0x80 * i].split(b'\0')[0].decode('cp932', 'replace') for i in range(5)],
             bgm=[d[0x290 + 0x80 * i:0x310 + 0x80 * i].split(b'\0')[0].decode('cp932', 'replace') for i in range(4)])
    objs = []
    for i in range(nobj):
        o = struct.unpack_from('<I', d, 0x490 + 4 * i)[0]
        oid, layer, flags = struct.unpack_from('<hBB', d, o)
        pos = struct.unpack_from('<3f', d, o + 4); size = struct.unpack_from('<3f', d, o + 0x10)
        quads = []; p = o + 0x1c
        while True:
            typ, sz = struct.unpack_from('<hh', d, p)
            if typ < 0: quads.append(dict(type=typ, size=sz, off=p)); break
            script, vmi = struct.unpack_from('<hh', d, p + 4)
            q = dict(type=typ, size=sz, script=script, vmidx=vmi, off=p)
            if typ == 0: q['pos'] = struct.unpack_from('<3f', d, p + 8); q['wh'] = struct.unpack_from('<2f', d, p + 0x14)
            elif typ == 1: q['p1'] = struct.unpack_from('<3f', d, p + 8); q['p2'] = struct.unpack_from('<3f', d, p + 0x14); q['width'] = struct.unpack_from('<f', d, p + 0x20)[0]
            quads.append(q); p += sz
        objs.append(dict(off=o, id=oid, layer=layer, flags=flags, pos=pos, size=size, quads=quads))
    insts = []; p = inst_off
    while True:
        oi, unk = struct.unpack_from('<hh', d, p)
        if oi < 0: insts.append(dict(obj=oi, unk=unk, off=p)); break
        insts.append(dict(obj=oi, unk=unk, pos=struct.unpack_from('<3f', d, p + 4), off=p)); p += 16
    script = []; p = scr_off
    while True:
        t, op, sz = struct.unpack_from('<ihh', d, p)
        raw = d[p + 8:p + 20]
        script.append(dict(time=t, op=op, size=sz, raw=raw, off=p))
        if t == -1: break
        p += 20
    return h, objs, insts, script


def fmt(sig, raw):
    out = []
    for k, c in enumerate(sig):
        b = raw[4 * k:4 * k + 4]
        if c == 'f': out.append(repr(round(struct.unpack('<f', b)[0], 4)))
        elif c == 'i': out.append(str(struct.unpack('<i', b)[0]))
        elif c == 'c': out.append('#%08x' % struct.unpack('<I', b)[0])
    return ', '.join(out)


def main():
    a = sys.argv[1]; quiet = '-q' in sys.argv
    files = [os.path.join(a, f) for f in sorted(os.listdir(a)) if f.endswith('.std')] if os.path.isdir(a) else [a]
    used = collections.Counter(); problems = []; qtypes = collections.Counter(); layers = collections.Counter()
    for fn in files:
        d = open(fn, 'rb').read(); bn = os.path.basename(fn)
        h, objs, insts, script = parse(d)
        nq = sum(len(o['quads']) - 1 for o in objs)
        if nq != h['nquad']: problems.append('%s: quad count %d != header %d' % (bn, nq, h['nquad']))
        if h['zero']: problems.append('%s: header+0xc nonzero' % bn)
        if not quiet:
            print('== %s objs=%d quads=%d inst@%#x script@%#x bgm=%s' % (bn, h['nobj'], h['nquad'], h['inst_off'], h['scr_off'], h['bgm']))
        for i, o in enumerate(objs):
            layers[o['layer']] += 1
            if o['id'] != i: problems.append('%s obj %d id %d' % (bn, i, o['id']))
            if not quiet: print('  obj %d @%#x layer=%d flags=%d pos=%s size=%s' % (i, o['off'], o['layer'], o['flags'], o['pos'], o['size']))
            for q in o['quads'][:-1]:
                qtypes[(q['type'], q['size'])] += 1
                if not quiet: print('    quad type=%d size=%#x script=%d %s' % (q['type'], q['size'], q['script'],
                                    ('pos=%s wh=%s' % (q['pos'], q['wh'])) if q['type'] == 0 else ('p1=%s p2=%s w=%s' % (q.get('p1'), q.get('p2'), q.get('width')))))
            if o['quads'][-1]['size'] != 4: problems.append('%s obj %d terminator size %d' % (bn, i, o['quads'][-1]['size']))
        for it in insts[:-1]:
            if not (0 <= it['obj'] < h['nobj']): problems.append('%s inst bad obj %d' % (bn, it['obj']))
            if not quiet: print('  inst obj=%d unk=%d pos=%s' % (it['obj'], it['unk'], tuple(round(x, 2) for x in it['pos'])))
        for k, s in enumerate(script[:-1]):
            used[s['op']] += 1
            if s['op'] not in OPS: problems.append('%s: unknown op %d' % (bn, s['op'])); continue
            if s['size'] != 12: problems.append('%s: op size %d' % (bn, s['size']))
            name, sig = OPS[s['op']]
            if s['op'] == 4:
                tgt = struct.unpack_from('<i', s['raw'])[0]
                if not 0 <= tgt < len(script) - 1: problems.append('%s: jump target %d' % (bn, tgt))
            if not quiet: print('  [%3d] %6d %-20s %s' % (k, s['time'], name, fmt(sig, s['raw'])))
        tail = d[script[-1]['off'] + 20:]
        if not quiet: print('  tail after end: %d bytes %s' % (len(tail), tail.hex()))
    print('== op coverage:', ' '.join('%d:%d' % kv for kv in sorted(used.items())))
    print('quad (type,size):', dict(qtypes), 'layers:', dict(layers))
    print('unknown ops:', [k for k in used if k not in OPS])
    print('problems:', len(problems))
    for p in problems[:40]: print('  ', p)


if __name__ == '__main__':
    main()
