#!/usr/bin/env python3
"""bullettypes.py: per bullet type (table 0x4B4AD8) -> etama.anm scripts, first sprite, size, hitbox class (BulletManager_Load)"""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anm
from pe import PE
D = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'extracted', 'dat', 'etama.anm')
d = open(D, 'rb').read()
ents = anm.parse(d)
sprites, scripts = [], []
for e in ents:
    sprites += [s for s in e['sprites']]
    scripts += [(e, so) for sid, so in e['scripts']]
tab = struct.unpack('<105i', PE().read(0x4B4AD8, 420))

def first_sprite(n):
    e, so = scripts[n]
    for op, sz, t, mask, args in anm.script_ops(d, e['base'] + so, len(d)):
        if op == 3:
            return struct.unpack_from('<i', args)[0]
    return None

def hitbox(sn, h):
    if not h > 8: return 4.0, 5
    if not h > 16: return 24.0 if False else None, None
for t in range(21):
    row = tab[t * 5:t * 5 + 5]
    s = first_sprite(row[0])
    sid, x, y, w, h = sprites[s]
    sn = row[0]
    if h > 8:
        if h > 16:
            if h > 32: hb, cls = 24.0, 0
            elif sn in (8, 0x71, 0x72, 0x73): hb, cls = 5.0, 2
            elif sn in (9, 0x6d, 0x6e): hb, cls = 8.0, 1
            else: hb, cls = 10.0, 1
        else:
            if sn in (2, 0x6f, 0x70, 4, 6, 5, 0x6a, 0x6b, 0x6c): hb, cls = 4.0, 4
            else: hb, cls = 6.0, 3
    else: hb, cls = 4.0, 5
    print('type %2d scripts %s sprite %3d (%gx%g at %g,%g) hitbox %g class %d' % (t, row, s, w, h, x, y, hb, cls))
