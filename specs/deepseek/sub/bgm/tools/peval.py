#!/usr/bin/env python3
"""peval.py - read virtual addresses from a PE file (th08.exe) using its section headers.

Usage:
  python3 peval.py <exe> <va>[:count] ...          # dump 'count' bytes as hex + ascii (default 16)
  python3 peval.py <exe> str:<va> ...              # dump a NUL-terminated ASCII string
  python3 peval.py <exe> dwords:<va>:<n>           # dump n little-endian u32
No external deps; pure struct parsing. Read-only.
"""
import struct
import sys


def sections(data):
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[e_lfanew:e_lfanew + 4] == b"PE\0\0", "not a PE"
    coff = e_lfanew + 4
    nsec, = struct.unpack_from("<H", data, coff + 2)
    opt_size, = struct.unpack_from("<H", data, coff + 16)
    opt = coff + 20
    image_base, = struct.unpack_from("<I", data, opt + 28)
    sec = opt + opt_size
    out = []
    for i in range(nsec):
        off = sec + i * 40
        name = data[off:off + 8].rstrip(b"\0").decode("latin1")
        vsize, va, rawsize, rawptr = struct.unpack_from("<IIII", data, off + 8)
        out.append((name, image_base + va, vsize, rawptr, rawsize))
    return image_base, out


def va_to_off(secs, va):
    for name, sva, vsize, rawptr, rawsize in secs:
        if sva <= va < sva + max(vsize, rawsize):
            return rawptr + (va - sva)
    raise ValueError("VA 0x%x not in any section" % va)


def main():
    data = open(sys.argv[1], "rb").read()
    base, secs = sections(data)
    for arg in sys.argv[2:]:
        kind = "bytes"
        if arg.startswith("str:"):
            kind, arg = "str", arg[4:]
        elif arg.startswith("dwords:"):
            kind, arg = "dwords", arg[7:]
        parts = arg.split(":")
        va = int(parts[0], 16)
        n = int(parts[1]) if len(parts) > 1 else 16
        off = va_to_off(secs, va)
        if kind == "str":
            end = data.index(b"\0", off)
            print("0x%08x = %r" % (va, data[off:end].decode("latin1")))
        elif kind == "dwords":
            for i in range(n):
                print("0x%08x = 0x%08x (%d)" % (va + 4 * i, struct.unpack_from("<I", data, off + 4 * i)[0],
                                                struct.unpack_from("<i", data, off + 4 * i)[0]))
        else:
            chunk = data[off:off + n]
            print("0x%08x: %s |%s|" % (va, " ".join("%02x" % b for b in chunk),
                                       "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)))


if __name__ == "__main__":
    main()
