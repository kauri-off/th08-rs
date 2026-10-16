#!/usr/bin/env python3
"""PBGZ (th08.dat) archive decoder / extractor — clean-room implementation
derived from static analysis of th08.exe (see ../01_archive.md).

Usage:
  python3 pbgz.py info    <th08.dat>
  python3 pbgz.py extract <th08.dat> <outdir>
  python3 pbgz.py list    <th08.dat>

Container layout (all verified against the shipped th08.dat):
  offset 0x00: magic "PBGZ"
  offset 0x04: 12 obfuscated bytes (cipher block, k0=0x1b k1=0x37 blk=12)
               -> u32 @0x04 = file_count      + 123456
               -> u32 @0x08 = table_offset    + 345678
               -> u32 @0x0c = table_raw_size  + 567891   (LZSS output size)
  table region: file[table_offset : EOF], decoded as:
      cipher (k0=0x3e k1=0x9b blk=0x80, first 0x400 bytes only)
      then LZSS with 8KB ring -> records:  name\\0, u32 offset, u32 usize, u32 fC
  file data:   file[offset : next.offset] -> LZSS (8KB ring) -> usize bytes
"""
import os
import struct
import sys

RING_SIZE = 0x2000
RING = bytearray(RING_SIZE)  # mirrors the engine's global ring at 0x18d5f68


def cipher_decode(data: bytes, k0: int, k1: int, blk: int, limit: int | None = None) -> bytes:
    """ZUN PBGZ block cipher (sub_43e1d0).

    Decodes in `blk`-byte blocks; inside a block the source bytes are written
    to odd 0-based positions descending from blk-1, then to even positions
    descending from blk-2.  Each byte is XORed with a rolling key that starts
    at k0 (low byte) and increments by k1 (low byte) after every byte.
    The final tail = ((len%blk < blk//4) ? len%blk : 0) + (len&1) bytes are
    copied verbatim.  `limit` mirrors the engine's arg_C loop bound; the
    engine passes 0x400 there, i.e. only the first 0x400 bytes of a region
    are actually XOR-processed (callers rely on this for large regions).
    """
    n = len(data)
    if limit is None:
        limit = 0x7FFFFFFF
    rem = n % blk
    tail = (rem if rem < blk // 4 else 0) + (n & 1)
    body = n - tail
    out = bytearray(n)
    k = k0 & 0xFF
    src = 0
    dst = 0
    budget = limit
    while body > 0 and budget > 0:
        b = min(blk, body)
        # pass 1: odd 0-based positions, descending from b-1
        p = dst + b - 1
        for _ in range((b + 1) // 2):
            out[p] = data[src] ^ k
            p -= 2
            k = (k + k1) & 0xFF
            src += 1
        # pass 2: even 0-based positions, descending from b-2
        p = dst + b - 2
        for _ in range(b // 2):
            out[p] = data[src] ^ k
            p -= 2
            k = (k + k1) & 0xFF
            src += 1
        body -= b
        dst += b
        budget -= b
    # verbatim tail (and any region the engine's budget left undecoded)
    out[dst:] = data[dst:]
    return bytes(out)


def _lzss_decompress(data: bytes, out_size: int, reset_ring: bool = False) -> bytes:
    if reset_ring:
        RING[:] = b"\0" * RING_SIZE
    out = bytearray()
    pos = 0
    ringpos = 1
    bitbuf = 0
    mask = 0

    def nextbit() -> int:
        nonlocal pos, bitbuf, mask
        if mask == 0:
            if pos < len(data):
                bitbuf = data[pos]
                pos += 1
            else:
                bitbuf = 0  # engine: reads 0 and does not advance past end
            mask = 0x80
        v = 1 if (bitbuf & mask) else 0
        mask >>= 1
        return v

    def readbits(nbits: int) -> int:
        v = 0
        for _ in range(nbits):
            v = (v << 1) | nextbit()
        return v

    while len(out) < out_size:
        if nextbit():  # 1 = literal
            b = readbits(8)
            out.append(b)
            RING[ringpos] = b
            ringpos = (ringpos + 1) & (RING_SIZE - 1)
            continue
        idx = readbits(13)  # absolute ring index; 0 = end marker
        if idx == 0:
            break
        cnt = readbits(4) + 3
        for i in range(cnt):
            b = RING[(idx + i) & (RING_SIZE - 1)]
            out.append(b)
            RING[ringpos] = b
            ringpos = (ringpos + 1) & (RING_SIZE - 1)
    return bytes(out)


def lzss_decompress(data: bytes, out_size: int, reset_ring: bool = False) -> bytes:
    """Public entry: engine keeps the 8KB ring between calls (global at
    0x18d5f68, initialised once at startup)."""
    return _lzss_decompress(data, out_size, reset_ring=reset_ring)


class PBGZ:
    def __init__(self, path: str):
        self.path = path
        with open(path, "rb") as f:
            self.blob = f.read()
        if self.blob[:4] != b"PBGZ":
            raise ValueError("not a PBGZ archive: bad magic %r" % self.blob[:4])
        hdr = cipher_decode(self.blob[4:16], k0=0x1B, k1=0x37, blk=12)
        self.count = (struct.unpack_from("<I", hdr, 0)[0] - 123456) & 0xFFFFFFFF
        self.table_off = (struct.unpack_from("<I", hdr, 4)[0] - 345678) & 0xFFFFFFFF
        self.table_usize = (struct.unpack_from("<I", hdr, 8)[0] - 567891) & 0xFFFFFFFF
        self.header_raw = hdr
        self.entries = []  # (name, offset, usize, fC)
        self._parse_table()

    def _parse_table(self):
        enc = self.blob[self.table_off:]
        dec = cipher_decode(enc, k0=0x3E, k1=0x9B, blk=0x80, limit=0x400)
        table = lzss_decompress(dec, self.table_usize)
        if len(table) != self.table_usize:
            print(f"warning: table short: {len(table)} != {self.table_usize}", file=sys.stderr)
        p = 0
        for _ in range(self.count):
            end = table.index(b"\0", p)
            name = table[p:end].decode("latin1")
            p = end + 1
            off, usize, fc = struct.unpack_from("<III", table, p)
            p += 12
            self.entries.append((name, off, usize, fc))
        self.table_end = p
        # sentinel: next offset for the last entry = table_off
        self.extents = []
        for i, (name, off, usize, fc) in enumerate(self.entries):
            nxt = self.entries[i + 1][1] if i + 1 < len(self.entries) else self.table_off
            self.extents.append(nxt - off)

    def data(self, i: int) -> bytes:
        name, off, usize, fc = self.entries[i]
        csize = self.extents[i]
        raw = self.blob[off:off + csize]
        return lzss_decompress(raw, usize)


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    cmd, path = argv[1], argv[2]
    arc = PBGZ(path)
    print(f"PBGZ {path}: count={arc.count} table_off=0x{arc.table_off:x} "
          f"table_usize={arc.table_usize}")
    if cmd == "info" or cmd == "list":
        total_c = sum(arc.extents)
        total_u = sum(e[2] for e in arc.entries)
        print(f"{'name':44s} {'offset':>10s} {'csize':>9s} {'usize':>9s} {'fC':>10s}")
        for (name, off, usize, fc), csize in zip(arc.entries, arc.extents):
            print(f"{name:44s} {off:10d} {csize:9d} {usize:9d} {fc:10d}")
        print(f"files={arc.count} compressed={total_c} uncompressed={total_u} "
              f"blob={len(arc.blob)}")
    elif cmd == "extract":
        outdir = argv[3]
        bad = 0
        for i, (name, off, usize, fc) in enumerate(arc.entries):
            try:
                data = arc.data(i)
            except Exception as e:  # noqa: BLE001
                print(f"ERROR {name}: {e}")
                bad += 1
                continue
            if len(data) != usize:
                print(f"WARN {name}: decompressed {len(data)} != usize {usize}")
                bad += 1
            dst = os.path.join(outdir, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, "wb") as f:
                f.write(data)
        print(f"extracted {arc.count - bad}/{arc.count} files to {outdir}")
    else:
        print("unknown command", cmd)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
