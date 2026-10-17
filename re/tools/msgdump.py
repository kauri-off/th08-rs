#!/usr/bin/env python3
"""msgdump.py <msg.dat|dir> [-q]: disassemble th08 msg*.dat dialogue scripts; reports problems."""
import sys, struct, os, glob

# op: (name, arg layout) ; layout chars: h i16, i i32, I u32, B u8, x pad byte, s xor-0x77 string (rest), r raw rest
OPS = {
    0: ('end', ''),
    1: ('face_script', 'hh'),
    2: ('face_sprite', 'hh'),
    3: ('text_line', 'hhs'),
    4: ('wait', 'i'),
    5: ('face_interrupt', 'hBx'),
    6: ('release_timeline', ''),
    7: ('bgm', 'i'),
    8: ('boss_card', 'hhs'),
    9: ('stage_end', 'i'),
    10: ('halt', ''),
    11: ('halt_final', ''),
    12: ('bgm_fade', ''),
    13: ('skippable', 'Bxxx'),
    14: ('flash', ''),
    15: ('speaker4', 'Iiiii'),
    16: ('text', 's'),
    17: ('speaker', 'Ii'),
    18: ('textbox', 'Bxxx'),
    19: ('text0', 's'),
    20: ('text1', 's'),
    21: ('choice', 'i'),
    22: ('choice_jump', ''),
}
FIXED = {'h': 2, 'i': 4, 'I': 4, 'B': 1, 'x': 1}


def xs(b):
    out = bytearray()
    for c in b:
        c ^= 0x77
        if c == 0:
            break
        out.append(c)
    return out.decode('cp932', 'replace'), len(out) + 1


def dump(path, quiet=False):
    d = open(path, 'rb').read()
    probs = []
    lines = []
    n = struct.unpack_from('<I', d)[0]
    offs = struct.unpack_from('<%dI' % n, d, 4)
    hdr_end = 4 + 4 * n
    seen = set()
    for idx, o in enumerate(offs):
        lines.append('entry %d @0x%x' % (idx, o))
        if o < hdr_end or o >= len(d):
            probs.append('entry %d bad offset' % idx)
            continue
        if o in seen:
            lines.append('  (same as previous entry)')
            continue
        seen.add(o)
        p = o
        last_t, last_op = 0, None
        while True:
            if p + 4 > len(d):
                probs.append('entry %d: ran off end' % idx)
                break
            t, op, sz = struct.unpack_from('<HBB', d, p)
            a = d[p + 4:p + 4 + sz]
            if t < last_t and not (op == 0 and last_op in (10, 11)):
                probs.append('entry %d @%x: time decreases' % (idx, p))
            last_t, last_op = t, op
            if op not in OPS:
                probs.append('entry %d @%x: unknown op %d' % (idx, p, op))
                args = a.hex()
                name = 'op%d' % op
            else:
                name, lay = OPS[op]
                vals, q = [], 0
                for ch in lay:
                    if ch in FIXED:
                        k = FIXED[ch]
                        if ch != 'x':
                            vals.append(struct.unpack_from('<' + ch, a, q)[0])
                        elif a[q] != 0:
                            probs.append('entry %d @%x: nonzero pad' % (idx, p))
                        q += k
                    elif ch == 's':
                        s, used = xs(a[q:])
                        if used > len(a) - q:
                            probs.append('entry %d @%x: unterminated string' % (idx, p))
                        if any(c ^ 0x77 for c in a[q + used:]):
                            probs.append('entry %d @%x: non-zero string padding' % (idx, p))
                        if (sz % 4) != 0:
                            probs.append('entry %d @%x: string arg size not multiple of 4' % (idx, p))
                        vals.append(repr(s))
                        q = len(a)
                    elif ch == 'r':
                        s, used = xs(a)
                        vals.append('raw=' + a.hex() + ' xor=' + repr(s))
                        q = len(a)
                if q != len(a):
                    probs.append('entry %d @%x: op %d size %d != layout %d' % (idx, p, op, sz, q))
                args = ', '.join(str(v) for v in vals)
            lines.append('  %5d  %-17s %s' % (t, name, args))
            p += 4 + sz
            if op == 0:
                break
    if not quiet:
        print('\n'.join(lines))
    return probs


def main():
    a = [x for x in sys.argv[1:] if not x.startswith('-')]
    quiet = '-q' in sys.argv
    files = sorted(glob.glob(os.path.join(a[0], 'msg*.dat'))) if os.path.isdir(a[0]) else a
    total = 0
    for f in files:
        if not quiet:
            print('=====', os.path.basename(f))
        pr = dump(f, quiet)
        for x in pr:
            print(os.path.basename(f), 'PROBLEM', x)
        total += len(pr)
    print('%d files, %d problems' % (len(files), total))


if __name__ == '__main__':
    main()
