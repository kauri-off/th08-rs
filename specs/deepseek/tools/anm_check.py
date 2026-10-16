#!/usr/bin/env python3
"""anm_check.py — independent structural validator for TH08 .anm files.

Walks every chunk of every decoded .anm, checks the documented layout
(see 03_images.md / structs.md) and reports statistics + anomalies:

  * chunk chain deltas stay inside the file, chunks tile without overlap
  * sprite offsets: within chunk, 20-byte records
  * script table offsets inside the chunk
  * embedded THTX: magic + size == 16 + w*h*2 (+ format histogram)

Usage: python3 anm_check.py <decoded_dir> [--verbose]
"""
import os
import struct
import sys
from collections import Counter


def u32(d, o):
    return struct.unpack_from("<I", d, o)[0]


def u16(d, o):
    return struct.unpack_from("<H", d, o)[0]


def i32(d, o):
    return struct.unpack_from("<i", d, o)[0]


def check_file(path, verbose=False):
    d = open(path, "rb").read()
    n = len(d)
    errs = []
    stats = {"chunks": 0, "sprites": 0, "scripts": 0, "thtx": 0, "fmt": Counter(),
             "sprite_step": Counter(), "name0": 0, "flags": Counter(), "flag0": []}
    off = 0
    ci = 0
    while off + 0x3C <= n:
        n_spr = u32(d, off + 0x00)
        n_scr = u32(d, off + 0x04)
        name_off = u32(d, off + 0x1C)
        ver = u32(d, off + 0x28)
        thtx_off = u32(d, off + 0x30)
        flags = u32(d, off + 0x34)
        nxt = u32(d, off + 0x38)
        cend = off + nxt if nxt else n
        if cend > n:
            errs.append(f"chunk{ci} @{off:#x}: next delta {nxt:#x} runs past EOF")
            break
        if ver != 3:
            errs.append(f"chunk{ci} @{off:#x}: version {ver} != 3")
        stats["chunks"] += 1
        stats["flags"][flags & 0xFF] += 1

        # sprite offset table
        straddr = off + 0x3C
        spo = [u32(d, straddr + 4 * k) for k in range(n_spr)]
        for k, v in enumerate(spo):
            if v == 0:
                stats["name0"] += 1
                continue
            if not (off <= off + v <= cend - 20):
                errs.append(f"chunk{ci} spr[{k}] off {v:#x} outside chunk")
                continue
            stats["sprites"] += 1
        nz = [v for v in spo if v]
        if len(nz) > 1:
            for a, b in zip(nz, nz[1:]):
                stats["sprite_step"][b - a] += 1

        # script table
        soff = straddr + 4 * n_spr
        for k in range(n_scr):
            if soff + 8 * k + 8 > cend:
                errs.append(f"chunk{ci} script table runs past chunk end")
                break
            sp = u32(d, soff + 8 * k)
            _sid = i32(d, soff + 8 * k + 4)
            if not (off <= off + sp < cend):
                errs.append(f"chunk{ci} scr[{k}] data off {sp:#x} outside chunk")
                continue
            stats["scripts"] += 1

        # name sanity
        if not (off < off + name_off <= cend):
            errs.append(f"chunk{ci} name_off {name_off:#x} outside chunk")
        if (flags & 0xFF) == 0:
            nm = d[off + name_off:off + name_off + 0x40].split(b"\0")[0]
            stats.setdefault("flag0", []).append((os.path.basename(path), ci, nm))

        # THTX
        if thtx_off:
            t = off + thtx_off
            if d[t:t + 4] != b"THTX":
                errs.append(f"chunk{ci} thtx @{t:#x}: bad magic {d[t:t+4]!r}")
            else:
                fmt = u16(d, t + 6)
                w = u16(d, t + 8)
                h = u16(d, t + 10)
                size = u32(d, t + 12)      # stored size of the pixel block
                bpp = {1: 4, 3: 2, 5: 2}.get(fmt)
                avail = cend - t - 16
                stats["thtx"] += 1
                stats["fmt"][fmt] += 1
                if bpp is None:
                    errs.append(f"chunk{ci} thtx fmt {fmt} unknown")
                elif size != w * h * bpp or size != avail:
                    errs.append(f"chunk{ci} thtx {w}x{h} fmt{fmt}: "
                                f"size={size} w*h*bpp={w*h*bpp} avail={avail}")
        if verbose:
            print(f"  {os.path.basename(path)} chunk{ci} @{off:#x} "
                  f"n_spr={n_spr} n_scr={n_scr} thtx={thtx_off:#x} next={nxt:#x}")
        ci += 1
        if nxt == 0:
            if cend != n:
                stats["trailing"] = n - cend
            break
        off = cend
    else:
        errs.append("chunk chain did not terminate (ran past EOF)")
    return stats, errs


def main(argv):
    root = argv[1] if len(argv) > 1 else "decoded"
    verbose = "--verbose" in argv
    files = sorted(f for f in os.listdir(root) if f.endswith(".anm"))
    tot = dict(chunks=0, sprites=0, scripts=0, thtx=0, name0=0)
    fmt = Counter()
    step = Counter()
    flags = Counter()
    bad = 0
    for f in files:
        st, errs = check_file(os.path.join(root, f), verbose=verbose)
        for k in tot:
            tot[k] += st.get(k, 0)
        fmt += st["fmt"]
        step += st["sprite_step"]
        flags += st["flags"]
        if errs:
            bad += 1
            print(f"!! {f}: {len(errs)} error(s)")
            for e in errs[:6]:
                print("   -", e)
    print(f"\n{len(files)} .anm files, {bad} with errors")
    print("chunks={chunks} sprites={sprites} scripts={scripts} thtx={thtx} "
          "null-sprite-entries={name0}".format(**tot))
    print("THTX formats:", dict(fmt))
    print("chunk flag low byte:", dict(flags))
    top = [x for x in step.most_common(8)]
    print("sprite record steps (top):", top)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
