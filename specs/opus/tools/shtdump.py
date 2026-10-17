#!/usr/bin/env python3
"""shtdump.py <file.sht|dir> [-q]: parse player shot files (layout from ShtLoad 0x44DD70 / fire 0x450F60 / init 0x44FB70)"""
import os, struct, sys
HDR = '<HHfiffffffffffff'  # 0x38 bytes before the level table
NAMES = ['zero', 'nlevels', 'bombs', 'deathbomb', 'hitbox', 'grazebox', 'attract', 'collect', 'poc_line',
         'f20', 'speed_n', 'speed_f', 'diag_n', 'diag_f', 'item_mult']
INIT = {0: 'default', 1: 'aim_enemy', 2: 'nobomb', 3: 'nobomb', 4: 'laser', 5: 'aim_point', 6: 'option_angle',
        7: 'random', 8: 'option2_angle'}
UPD = {0: '-', 1: 'homing', 2: '-', 3: 'gravity', 4: 'laser', 5: 'upd5'}
DRAW = {0: '-', 1: 'trail'}
HIT = {0: '-', 1: 'hit1', 2: 'hit2'}
ENT = '<hhffffffhhhhhhiiii'  # 0x38


def parse(d):
    probs = []
    h = struct.unpack_from('<HH', d, 0)
    nlev = h[1]
    vals = struct.unpack_from('<f i f f f f f i f f f f f'.replace(' ', ''), d, 4)
    hdr = dict(zero=h[0], nlevels=nlev, bombs=vals[0], deathbomb=vals[1], hitbox=vals[2], grazebox=vals[3],
               attract=vals[4], collect=vals[5], poc_line=vals[6], unk20=vals[7], speed_n=vals[8], speed_f=vals[9],
               diag_n=vals[10], diag_f=vals[11], item_mult=vals[12])
    if h[0] != 0: probs.append('hdr+0 != 0')
    levels = []
    end = 0x38 + 8 * nlev
    prev = -1
    for i in range(nlev):
        off, thr = struct.unpack_from('<Ii', d, 0x38 + 8 * i)
        if thr < prev: probs.append('threshold not ascending')
        prev = thr
        ents = []
        p = off
        while True:
            if p + 2 > len(d): probs.append('level %d runs off end' % i); break
            if struct.unpack_from('<h', d, p)[0] < 0: p += 4; break  # terminator: 4 bytes (ffff ffff)
            if p + 0x38 > len(d): probs.append('entry off end'); break
            e = struct.unpack_from(ENT, d, p)
            ent = dict(interval=e[0], phase=e[1], ox=e[2], oy=e[3], w=e[4], h=e[5], angle=e[6], speed=e[7],
                       damage=e[8], gauge=e[9], origin=e[10], type=e[11], script=e[12], sfx=e[13],
                       init=e[14], update=e[15], draw=e[16], hit=e[17])
            if ent['interval'] <= 0: probs.append('interval<=0')
            if ent['init'] not in INIT or ent['update'] not in UPD or ent['draw'] not in DRAW or ent['hit'] not in HIT:
                probs.append('fn index out of range')
            if not 0 <= ent['origin'] <= 4: probs.append('origin out of range')
            ents.append(ent)
            p += 0x38
        end = max(end, p)
        levels.append((off, thr, ents))
    if end != len(d): probs.append('size mismatch: parsed to 0x%x, file 0x%x' % (end, len(d)))
    return hdr, levels, probs


def main():
    a = sys.argv[1]
    files = [os.path.join(a, f) for f in sorted(os.listdir(a)) if f.endswith('.sht')] if os.path.isdir(a) else [a]
    total = 0
    for fn in files:
        d = open(fn, 'rb').read()
        hdr, levels, probs = parse(d)
        total += len(probs)
        print('== %s  %s' % (os.path.basename(fn), ' '.join('%s=%g' % (k, v) for k, v in hdr.items())))
        for p in probs: print('   PROBLEM', p)
        if '-q' in sys.argv: continue
        for i, (off, thr, ents) in enumerate(levels):
            print('  level %d (power < %d) @0x%x: %d entries' % (i, thr, off, len(ents)))
            for e in ents:
                print('    every %2d ph %2d org %d off(%6.1f,%6.1f) box %5.1fx%-5.1f ang %7.4f spd %5.1f dmg %3d g %3d type %d scr %2d sfx %2d %s/%s/%s/%s' % (
                    e['interval'], e['phase'], e['origin'], e['ox'], e['oy'], e['w'], e['h'], e['angle'], e['speed'],
                    e['damage'], e['gauge'], e['type'], e['script'], e['sfx'], INIT[e['init']], UPD[e['update']],
                    DRAW[e['draw']], HIT[e['hit']]))
    print('problems:', total)


if __name__ == '__main__':
    main()
