import struct, sys
EXE = "/home/anon/Games/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/th08.exe"

class PE:
    def __init__(self, path=EXE):
        self.d = open(path, 'rb').read()
        pe = struct.unpack_from('<I', self.d, 0x3c)[0]
        nsec = struct.unpack_from('<H', self.d, pe + 6)[0]
        optsz = struct.unpack_from('<H', self.d, pe + 20)[0]
        self.base = struct.unpack_from('<I', self.d, pe + 24 + 28)[0]
        self.secs = []
        o = pe + 24 + optsz
        for i in range(nsec):
            name = self.d[o:o + 8].rstrip(b'\0').decode()
            vsz, va, rsz, rptr = struct.unpack_from('<IIII', self.d, o + 8)
            self.secs.append((name, self.base + va, vsz, rptr, rsz))
            o += 40

    def read(self, va, n):
        for name, sva, vsz, rptr, rsz in self.secs:
            if sva <= va < sva + max(vsz, rsz):
                off = va - sva
                b = self.d[rptr + off: rptr + min(off + n, rsz)]
                return b + b'\0' * (n - len(b))
        raise ValueError(hex(va))

if __name__ == '__main__':
    p = PE()
    for s in p.secs: print(s[0], hex(s[1]), hex(s[2]), hex(s[3]), hex(s[4]))
    if len(sys.argv) > 2:
        print(p.read(int(sys.argv[1], 16), int(sys.argv[2], 0)).hex(' '))
