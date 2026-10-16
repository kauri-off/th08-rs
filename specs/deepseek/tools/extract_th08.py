#!/usr/bin/env python3
"""extract_th08.py — Touhou 8 (Imperishable Night) th08.dat extractor.

Single pass, all wrapper layers removed:

    th08.dat (PBGZ container)
      -> per-member LZSS decompression            (yields the raw member bytes)
      -> "edz<type>" payload unwrapping           (yields the real file)

Usage:
    python3 extract_th08.py <th08.dat> <outdir> [--raw <rawdir>] [--list] [--quiet]

Behaviour:
  * <outdir>  receives the fully decoded files (all wrappers removed), laid out
    with the archive's own directory structure ('/' -> os.sep).
  * --raw     additionally writes the intermediate member bytes (still carrying
    the 4-byte "edz?" tag) for debugging.
  * --list    only prints the file table, writes nothing.
  * exits non-zero if any member fails to reach its declared size.

Format reference: ../01_archive.md (VERIFIED against the shipped th08.dat,
SHA-256 9d7edf43b8ddd347cbb641836f6b5050745dd936f688daebbf9382ca557043bb).

Everything here is the same clean-room algorithm described in 01_archive.md:
  * block cipher  (region obfuscation: PBGZ header, file table, edz payloads)
  * LZSS          (member streams and the file table)
  * edz key table (per-type cipher parameters)
No third-party code; standard library only.
"""
from __future__ import annotations

import os
import struct
import sys

# ----------------------------------------------------------------------------
# block cipher (th08.exe sub_43E1D0) — see 01_archive.md §2
# ----------------------------------------------------------------------------


def cipher_decode(data: bytes, k0: int, k1: int, blk: int, budget: int = 0x7FFFFFFF) -> bytes:
    """Reversible XOR-obfuscation used on every encoded region.

    The first `budget` bytes (rounded down to whole `blk`-byte blocks) are
    transformed; the rest is copied verbatim.  Inside a block, source bytes are
    written to odd 0-based positions descending from blk-1, then to even
    positions descending from blk-2, each XORed with a rolling key that starts
    at k0 and advances by k1 after every byte.
    """
    n = len(data)
    rem = n % blk
    tail = (rem if rem < blk // 4 else 0) + (n & 1)
    body = n - tail
    out = bytearray(n)
    k = k0 & 0xFF
    src = dst = 0
    while body > 0 and budget > 0:
        b = min(blk, body)
        p = dst + b - 1
        for _ in range((b + 1) // 2):       # odd offsets, descending
            out[p] = data[src] ^ k
            p -= 2
            k = (k + k1) & 0xFF
            src += 1
        p = dst + b - 2
        for _ in range(b // 2):             # even offsets, descending
            out[p] = data[src] ^ k
            p -= 2
            k = (k + k1) & 0xFF
            src += 1
        body -= b
        dst += b
        budget -= b
    out[dst:] = data[dst:]
    return bytes(out)


# ----------------------------------------------------------------------------
# LZSS (th08.exe sub_4740E0) — see 01_archive.md §3
# ----------------------------------------------------------------------------

RING_SIZE = 0x2000
# The engine keeps this ring between calls (global at 0x018D5F68); we mirror
# that so multi-file extraction behaves exactly like the running game.
_RING = bytearray(RING_SIZE)


def lzss_decompress(data: bytes, out_size: int, reset_ring: bool = False) -> bytes:
    """MSB-first LZSS: flag 1 = literal byte; flag 0 = 13-bit absolute ring
    index (0 = end marker) followed by 4-bit (length-3).  8 KiB ring."""
    if reset_ring:
        _RING[:] = b"\0" * RING_SIZE
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
                bitbuf = 0          # engine reads 0 past EOF, no pointer advance
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
        if nextbit():
            b = readbits(8)
            out.append(b)
            _RING[ringpos] = b
            ringpos = (ringpos + 1) & (RING_SIZE - 1)
            continue
        idx = readbits(13)
        if idx == 0:
            break
        cnt = readbits(4) + 3
        for i in range(cnt):
            b = _RING[(idx + i) & (RING_SIZE - 1)]
            out.append(b)
            _RING[ringpos] = b
            ringpos = (ringpos + 1) & (RING_SIZE - 1)
    return bytes(out)


# ----------------------------------------------------------------------------
# edz payload wrapper (th08.exe sub_43E390 + key table at 0x004C78E0)
# see 01_archive.md §5
# ----------------------------------------------------------------------------

# type char -> (k0, k1, block, budget)
EDZ_KEYS = {
    "M": (0x1B, 0x37, 0x0040, 0x2800),
    "T": (0x51, 0xE9, 0x0040, 0x3000),
    "A": (0xC1, 0x51, 0x1400, 0x2000),
    "J": (0x03, 0x19, 0x1400, 0x7800),
    "E": (0xAB, 0xCD, 0x0200, 0x1000),
    "W": (0x12, 0x34, 0x0400, 0x2800),
    "-": (0x35, 0x97, 0x0080, 0x2800),
    "*": (0x99, 0x37, 0x0400, 0x1000),
}


def edz_unwrap(member: bytes) -> tuple[bytes, str]:
    """Remove the 4-byte "edz?" tag and decode the payload.
    Returns (plaintext, type_char).  Members without a tag pass through."""
    if len(member) >= 4 and member[:3] == b"edz":
        t = chr(member[3])
        key = EDZ_KEYS.get(t)
        if key:
            k0, k1, blk, budget = key
            return cipher_decode(member[4:], k0, k1, blk, budget), t
        return member, "?"
    return member, ""


# ----------------------------------------------------------------------------
# PBGZ container (th08.exe sub_474CE0 / sub_474AF0) — see 01_archive.md §1/§4
# ----------------------------------------------------------------------------

HEADER_CONST = (123456, 345678, 567891)      # subtracted from the 3 header u32s


class PbgzArchive:
    def __init__(self, path: str):
        with open(path, "rb") as f:
            self.blob = f.read()
        if self.blob[:4] != b"PBGZ":
            raise ValueError("not a PBGZ archive (bad magic %r)" % self.blob[:4])
        hdr = cipher_decode(self.blob[4:16], k0=0x1B, k1=0x37, blk=12)
        c, t, u = struct.unpack_from("<III", hdr, 0)
        self.count = (c - HEADER_CONST[0]) & 0xFFFFFFFF
        self.table_off = (t - HEADER_CONST[1]) & 0xFFFFFFFF
        self.table_usize = (u - HEADER_CONST[2]) & 0xFFFFFFFF
        self.entries: list[tuple[str, int, int, int]] = []
        self._parse_table()

    def _parse_table(self) -> None:
        region = self.blob[self.table_off:]
        dec = cipher_decode(region, k0=0x3E, k1=0x9B, blk=0x80, budget=0x400)
        table = lzss_decompress(dec, self.table_usize)
        if len(table) != self.table_usize:
            raise ValueError("file table short: %d != %d" % (len(table), self.table_usize))
        p = 0
        for _ in range(self.count):
            end = table.index(b"\0", p)
            name = table[p:end].decode("latin1")
            p = end + 1
            off, usize, flags = struct.unpack_from("<III", table, p)
            p += 12
            self.entries.append((name, off, usize, flags))
        self.table_end = p

    def member_raw(self, i: int) -> bytes:
        """LZSS-decompress member i (still carrying its edz tag)."""
        name, off, usize, _flags = self.entries[i]
        nxt = (self.entries[i + 1][1] if i + 1 < len(self.entries)
               else self.table_off)
        csize = nxt - off
        return lzss_decompress(self.blob[off:off + csize], usize)

    def member(self, i: int) -> tuple[bytes, str]:
        """Fully decoded member i: (bytes, edz type char)."""
        return edz_unwrap(self.member_raw(i))


# ----------------------------------------------------------------------------
# CLI
# ----------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    opts = {a for a in argv[1:] if a.startswith("--")}
    if len(args) < 1:
        print(__doc__)
        return 1
    dat = args[0]
    outdir = args[1] if len(args) > 1 else None
    rawdir = None
    if "--raw" in opts:
        i = argv.index("--raw")
        rawdir = argv[i + 1] if i + 1 < len(argv) else None
        if rawdir in args:
            args.remove(rawdir)

    arc = PbgzArchive(dat)
    print(f"{dat}: PBGZ count={arc.count} table_off=0x{arc.table_off:X} "
          f"table_usize={arc.table_usize}")

    if "--list" in opts or (outdir is None and rawdir is None):
        print(f"{'name':44s} {'offset':>10s} {'usize':>9s} type")
        for name, off, usize, _fl in arc.entries:
            print(f"{name:44s} {off:10d} {usize:9d}")
        return 0

    types: dict[str, int] = {}
    bad = 0
    for i, (name, off, usize, _fl) in enumerate(arc.entries):
        try:
            raw = arc.member_raw(i)
        except Exception as e:                      # noqa: BLE001
            print(f"ERROR {name}: LZSS failed: {e}", file=sys.stderr)
            bad += 1
            continue
        if len(raw) != usize:
            print(f"ERROR {name}: LZSS produced {len(raw)} != {usize}", file=sys.stderr)
            bad += 1
            continue
        plain, tchar = edz_unwrap(raw)
        types[tchar or "(none)"] = types.get(tchar or "(none)", 0) + 1
        if outdir:
            dst = os.path.join(outdir, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, "wb") as f:
                f.write(plain)
        if rawdir:
            dst = os.path.join(rawdir, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, "wb") as f:
                f.write(raw)

    ok = arc.count - bad
    if outdir:
        print(f"decoded {ok}/{arc.count} files -> {outdir}")
    if rawdir:
        print(f"raw members {ok}/{arc.count} -> {rawdir}")
    print("edz type histogram: " + ", ".join(
        f"{k}:{v}" for k, v in sorted(types.items(), key=lambda kv: -kv[1])))
    if bad:
        print(f"{bad} file(s) FAILED", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
