#!/usr/bin/env python3
# rngsites.py <probe_rng.txt> [demo] [-o] : per-frame RNG call chains (symbolised); -o keeps call order
import bisect, collections, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
rows = [l.rstrip('\n').split('\t') for l in open(os.path.join(ROOT, 'extracted', 'callgraph.tsv'))][1:]
fs = sorted((int(r[0], 16), r[1]) for r in rows if r[0].strip())
ad = [f[0] for f in fs]


def nm(a):
    a = int(a, 16)
    i = bisect.bisect_right(ad, a) - 1
    return f"{fs[i][1]}+{a - fs[i][0]:x}" if i >= 0 else hex(a)


args = [a for a in sys.argv[1:] if a != '-o']
order = '-o' in sys.argv
demo = args[1] if len(args) > 1 else None
cur = []
for l in open(args[0]):
    p = l.split()
    if p[0] != 'R':
        cur.append(' < '.join(nm(x) for x in p[2:8]))
        continue
    if cur and (demo is None or p[2] == demo):
        print(l.strip(), f"({len(cur)} draws)")
        if order:
            prev, n = None, 0
            for k in cur + [None]:
                if k == prev:
                    n += 1
                    continue
                if prev:
                    print('   ', n, prev)
                prev, n = k, 1
        else:
            for k, v in sorted(collections.Counter(cur).items(), key=lambda x: -x[1]):
                print('   ', v, k)
    cur = []
