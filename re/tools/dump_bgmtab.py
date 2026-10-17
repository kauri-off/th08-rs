import struct, sys
sys.path.insert(0, __import__('os').path.dirname(__file__))
from pe import PE
p = PE()
va = 0x4c7670
while True:
    thr, a, ptr, b, c = struct.unpack('<iiIii', p.read(va, 20))
    if thr < 0: break
    s = p.read(ptr, 32).split(b'\0')[0].decode()
    print(thr, a, s, b, c)
    va += 20
