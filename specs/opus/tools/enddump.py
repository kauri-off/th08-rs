#!/usr/bin/env python3
# ending/staff-roll script (.end) parser (see 16_endings_result.md §1)
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
import anm

NARGS = {'0': 1, '1': 1, '2': 1, '3': 1, 'M': 1, 'V': 2, 'a': 3, 'c': 1, 'r': 2, 's': 2, 'v': 1, 'w': 2}
STR = {'F', 'b', 'm'}
DAT = os.path.join(os.path.dirname(__file__), '..', 'extracted', 'dat')


def parse(d, staff=None, out=print):
    probs, ops, p, n, lines = [], {}, 0, len(d), 0
    ended = False
    while p < n:
        e = d.find(b'\n', p)
        e = n if e < 0 else e
        line = d[p:e].rstrip(b'\r')
        p = e + 1
        if not line.strip(b'\0 '):
            continue
        if ended:
            probs.append('data after @z')
        if line[:1] == b'@':
            op = chr(line[1]) if len(line) > 1 else '?'
            ops[op] = ops.get(op, 0) + 1
            args = [a for a in line[2:].split(b'\0') if a.strip()]
            if op in NARGS:
                if len(args) != NARGS[op] or not all(a.strip().lstrip(b'-').isdigit() for a in args):
                    probs.append('bad args @%s %r' % (op, line))
                v = [int(a) for a in args]
                if op == 'a' and staff is not None:
                    vm, sc, sp = v
                    if vm > 15 or sc >= staff[0] or sp >= staff[1]:
                        probs.append('@a out of range %r' % v)
                out('@%s %s' % (op, ' '.join(map(str, v))))
            elif op in STR:
                name = args[0].decode('ascii') if args else ''
                if not name:
                    probs.append('missing name @%s' % op)
                elif not os.path.exists(os.path.join(DAT, os.path.basename(name))):
                    probs.append('missing file %s' % name)
                out('@%s %s' % (op, name))
            elif op in ('R', 'z'):
                ended = op == 'z'
                out('@' + op)
            else:
                probs.append('unknown op %r' % line)
        else:
            line = line.rstrip(b'\0')
            if b'\0' in line:
                probs.append('NUL in text')
            if len(line) > 0x44 - 2:
                probs.append('text too long (%d)' % len(line))
            # text is copied two bytes at a time; an odd length would swallow the terminator
            if len(line) % 2:
                probs.append('odd text length %r' % line)
            lines += 1
            out('T ' + line.decode('cp932', 'replace'))
    if not ended:
        probs.append('no @z')
    return ops, lines, probs


if __name__ == '__main__':
    quiet = '-q' in sys.argv
    files = [a for a in sys.argv[1:] if a != '-q']
    sa = anm.parse(open(os.path.join(DAT, 'staff01.anm'), 'rb').read())
    staff = (sum(len(e['scripts']) for e in sa), sum(len(e['sprites']) for e in sa))
    if not quiet:
        print('staff01.anm scripts %d sprites %d' % staff)
    bad = 0
    for f in files:
        ops, lines, probs = parse(open(f, 'rb').read(), staff, (lambda *a: None) if quiet else print)
        print('%s: %d text lines, ops %s, %d problems %s' % (os.path.basename(f), lines, dict(sorted(ops.items())), len(probs), probs))
        bad += len(probs)
    sys.exit(1 if bad else 0)
