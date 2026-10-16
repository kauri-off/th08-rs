#!/usr/bin/env python3
"""TH08 "edz" payload unwrapper (clean-room, from th08.exe sub_43e390 and the
key table at 0x4c78e0 / magic globals at 0x4c7940).

Every file stored in th08.dat is first LZSS-decompressed by PBGZ, and then
its payload carries a 4-byte ASCII header "edz" + <type>; the bytes after the
header are the same block cipher as the PBGZ header/table, with per-type keys:

  type  k0    k1    block   limit(loop budget)
  M     0x1b  0x37  0x40    0x2800     (not used by shipped TH08 files)
  T     0x51  0xe9  0x40    0x3000     text.anm / musiccmt.txt
  A     0xc1  0x51  0x1400  0x2000     .anm
  J     0x03  0x19  0x1400  0x7800     .jpg
  E     0xab  0xcd  0x200   0x1000     .ecl
  W     0x12  0x34  0x400   0x2800     .wav
  -     0x35  0x97  0x80    0x2800     .mid .std .sht .msg .png .rpy .end .fmt .ver
  *     0x99  0x37  0x400   0x1000     (not used by shipped TH08 files)

unwrapped payload = cipher_decode(file[4:], k0, k1, blk, limit)
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pbgz import cipher_decode  # noqa: E402

KEY_TABLE = {
    "M": (0x1B, 0x37, 0x40, 0x2800),
    "T": (0x51, 0xE9, 0x40, 0x3000),
    "A": (0xC1, 0x51, 0x1400, 0x2000),
    "J": (0x03, 0x19, 0x1400, 0x7800),
    "E": (0xAB, 0xCD, 0x200, 0x1000),
    "W": (0x12, 0x34, 0x400, 0x2800),
    "-": (0x35, 0x97, 0x80, 0x2800),
    "*": (0x99, 0x37, 0x400, 0x1000),
}


def edz_unwrap(data: bytes) -> bytes:
    """Returns the plaintext payload.  Non-edz files are returned unchanged."""
    if len(data) < 4 or data[:3] != b"edz":
        return data
    t = chr(data[3])
    if t not in KEY_TABLE:
        return data
    k0, k1, blk, limit = KEY_TABLE[t]
    return cipher_decode(data[4:], k0=k0, k1=k1, blk=blk, limit=limit)


if __name__ == "__main__":
    src, dst = sys.argv[1], sys.argv[2]
    os.makedirs(dst, exist_ok=True)
    for root, _dirs, files in os.walk(src):
        for fn in sorted(files):
            p = os.path.join(root, fn)
            rel = os.path.relpath(p, src)
            with open(p, "rb") as f:
                data = f.read()
            out = edz_unwrap(data)
            q = os.path.join(dst, rel)
            os.makedirs(os.path.dirname(q), exist_ok=True)
            with open(q, "wb") as f:
                f.write(out)
    print("done ->", dst)
