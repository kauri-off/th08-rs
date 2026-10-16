#!/usr/bin/env python3
"""Minimal PE helpers for th08.exe (no external deps).

Usage:
  python3 pe_util.py foff2va th08.exe 0x74b4b      # file offset -> VA
  python3 pe_util.py va2foff th08.exe 0x447b4b     # VA -> file offset
  python3 pe_util.py sections th08.exe             # list sections
"""
import struct
import sys


def sections(path):
    data = open(path, "rb").read()
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[e_lfanew:e_lfanew + 4] == b"PE\0\0", "not a PE"
    coff = e_lfanew + 4
    nsections = struct.unpack_from("<H", data, coff + 2)[0]
    opt_size = struct.unpack_from("<H", data, coff + 16)[0]
    opt_off = coff + 20
    magic = struct.unpack_from("<H", data, opt_off)[0]
    image_base = struct.unpack_from("<I", data, opt_off + 28)[0] if magic == 0x10B else struct.unpack_from("<Q", data, opt_off + 24)[0]
    secs = []
    off = opt_off + opt_size
    for i in range(nsections):
        name = data[off:off + 8].rstrip(b"\0").decode("latin1")
        vsize, vaddr, rawsize, rawptr = struct.unpack_from("<IIII", data, off + 8)
        secs.append(dict(name=name, vaddr=vaddr, vsize=vsize, rawptr=rawptr, rawsize=rawsize))
        off += 40
    return image_base, secs


def foff2va(path, foff):
    base, secs = sections(path)
    for s in secs:
        if s["rawptr"] <= foff < s["rawptr"] + s["rawsize"]:
            return base + s["vaddr"] + (foff - s["rawptr"])
    raise ValueError("offset not in any section")


def va2foff(path, va):
    base, secs = sections(path)
    rva = va - base
    for s in secs:
        if s["vaddr"] <= rva < s["vaddr"] + max(s["vsize"], s["rawsize"]):
            return s["rawptr"] + (rva - s["vaddr"])
    raise ValueError("va not in any section")


if __name__ == "__main__":
    cmd, path = sys.argv[1], sys.argv[2]
    if cmd == "sections":
        base, secs = sections(path)
        print(f"image_base=0x{base:x}")
        for s in secs:
            print(f"  {s['name']:8s} VA=0x{base+s['vaddr']:08x} vsize=0x{s['vsize']:x} raw=0x{s['rawptr']:x}+0x{s['rawsize']:x}")
    elif cmd == "foff2va":
        print(hex(foff2va(path, int(sys.argv[3], 0))))
    elif cmd == "va2foff":
        print(hex(va2foff(path, int(sys.argv[3], 0))))
