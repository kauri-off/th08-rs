#!/usr/bin/env python3
# musiccmt.txt parser (see 16_endings_result.md §5)
import sys, os

DAT = os.path.join(os.path.dirname(__file__), '..', 'extracted', 'dat')


def parse(d):
    # mirrors the loader: '@' starts an entry; file, title, then up to 7 non-empty lines until the next '@'
    entries, probs, p, n = [], [], 0, len(d)

    def line(p):
        e = p
        while e < n and d[e] not in b'\r\n':
            e += 1
        s = d[p:e]
        while e < n and d[e] in b'\r\n':
            e += 1
        return s, e
    while p < n:
        if d[p:p + 1] != b'@':
            p += 1
            continue
        fname, p = line(p + 1)
        title, p = line(p)
        cmt = []
        while len(cmt) < 7 and p < n and d[p:p + 1] != b'@':
            s, p = line(p)
            cmt.append(s)
        entries.append((fname, title, cmt))
        # comment slots are 0x42 apart and wiped before each line, so only a line reaching past the next slot breaks
        if len(fname) >= 0x40 or len(title) >= 0x42 or any(len(c) >= 0x84 for c in cmt):
            probs.append('field too long in entry %d' % len(entries))
    if len(entries) > 32:
        probs.append('more than 32 entries')
    return entries, probs


if __name__ == '__main__':
    path = next((a for a in sys.argv[1:] if a != '-q'), os.path.join(DAT, 'musiccmt.txt'))
    entries, probs = parse(open(path, 'rb').read())
    if '-q' not in sys.argv:
        for i, (f, t, c) in enumerate(entries):
            print('%2d %-18s %s (%d lines)' % (i, f.decode(), t.decode('cp932'), len(c)))
            for k, l in enumerate(c):
                if len(l) > 0x40:
                    print('   line %d: %d bytes, shown truncated to 64' % (k, len(l)))
    print('%s: %d entries, %d problems %s' % (os.path.basename(path), len(entries), len(probs), probs))
    sys.exit(1 if probs else 0)
