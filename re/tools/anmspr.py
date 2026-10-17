#!/usr/bin/env python3
"""anmspr.py <file.anm> [lo hi] - list sprite rects (global index, entry, id, x, y, w, h)."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from anm import parse
d = open(sys.argv[1], 'rb').read()
lo, hi = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (0, 1 << 30)
for ei, e in enumerate(parse(d)):
    for sid, x, y, w, h in e['sprites']:
        if lo <= sid <= hi:
            print('entry %d  spr %3d  x %6.1f y %6.1f w %6.1f h %6.1f' % (ei, sid, x, y, w, h))
