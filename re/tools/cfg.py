#!/usr/bin/env python3
# th08.cfg validator (see 15_files_replay_score.md §1)
import sys, struct

FIELDS = [(0x1C, 'lives', 7), (0x1D, 'unused1D', 4), (0x1E, 'color16', 2), (0x1F, 'music', 3), (0x20, 'se', 2),
          (0x21, 'difficulty', 6), (0x22, 'window', 2), (0x23, 'frameskip', 3), (0x24, 'effects', 3),
          (0x25, 'slow', 2), (0x26, 'shotfocus', 2)]
b = open(sys.argv[1], 'rb').read()
probs = []
if len(b) != 0x3C:
    probs.append('size')
if struct.unpack_from('<I', b, 0x14)[0] != 0x80001:
    probs.append('version')
print('pad', struct.unpack_from('<9h', b, 0), 'stick', struct.unpack_from('<2h', b, 0x18))
for off, name, lim in FIELDS:
    print(name, b[off])
    if b[off] >= lim:
        probs.append(name)
print('bgmvol', b[0x27], 'sevol', b[0x28], 'debug %#x' % struct.unpack_from('<I', b, 0x38)[0])
print('%d problems %s' % (len(probs), probs))
sys.exit(1 if probs else 0)
