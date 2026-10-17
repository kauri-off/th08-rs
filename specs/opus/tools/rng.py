#!/usr/bin/env python3
"""rng.py [seed] [n]: th08 game RNG reference (11 §1). Prints n Next16 values from seed."""
import struct, sys


class Rng:
    def __init__(self, seed=0):
        self.s = seed & 0xFFFF
        self.count = 0

    def next16(self):
        t = ((self.s ^ 0x9630) - 0x6553) & 0xFFFF
        self.s = ((t << 2) | (t >> 14)) & 0xFFFF
        self.count += 1
        return self.s

    def next32(self):
        hi = self.next16()
        return (hi << 16) | self.next16()

    def range(self, n):
        return 0 if n == 0 else self.next32() % n

    def range16(self, n):
        return 0 if n == 0 else self.next16() % n

    def float(self):
        # single-precision: u32 rounded to f32, divided by 2^32 (exact)
        f = struct.unpack('<f', struct.pack('<f', float(self.next32())))[0]
        return f / 4294967296.0


if __name__ == '__main__':
    seed = int(sys.argv[1], 0) if len(sys.argv) > 1 else 0
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 8
    r = Rng(seed)
    print(' '.join('%04x' % r.next16() for _ in range(n)))
