#!/usr/bin/env python3
"""rd.py <va> [n] [f|i|h|s]: read exe bytes at VA (f=float32s, i=int32s, s=cstring)"""
import sys, struct, os
sys.path.insert(0, os.path.dirname(__file__))
from pe import PE
p = PE()
va = int(sys.argv[1], 16); n = int(sys.argv[2], 0) if len(sys.argv) > 2 else 16
m = sys.argv[3] if len(sys.argv) > 3 else 'h'
b = p.read(va, n)
if m == 'f': print([round(x, 7) for x in struct.unpack('<%df' % (n // 4), b)])
elif m == 'd': print(struct.unpack('<%dd' % (n // 8), b))
elif m == 'i': print(struct.unpack('<%di' % (n // 4), b))
elif m == 's': print(b.split(b'\0')[0].decode('cp932', 'replace'))
else: print(b.hex(' '))
