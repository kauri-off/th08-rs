#!/usr/bin/env python3
"""dump_fmt.py - parse a Touhou 8 (TH08) thbgm.fmt track table.

Usage:
  python3 tools/dump_fmt.py [--csv out.csv] [--dat thbgm.dat] [fmtfile]

Record layout (52 bytes = 0x34 stride, little-endian):
  +0x00  char name[16]            NUL-padded ASCII, e.g. "th08_00.wav"
  +0x10  u32  start               byte offset of track data inside thbgm.dat
  +0x14  u32  preload_bytes       bytes Bgm_PreloadTrack() mallocs+reads (>= length)
  +0x18  u32  loop_start          loop point offset, relative to +0x10 (0 = none)
  +0x1C  u32  length              total track byte length; loop end offset
  +0x20  WAVEFORMATEX (18 bytes)  wFormatTag,wChannels,nSamplesPerSec,nAvgBytesPerSec,
                                  nBlockAlign,wBitsPerSample,cbSize
  +0x32  u16  zero
The list is terminated by a record whose name[0] == 0; a 17-byte zero trailer
after the last record guarantees this.  There is no file header and no count.
All values are VERIFIED against th08.exe (see 02_bgm.md).
"""
import csv
import struct
import sys

REC = 0x34


def parse(data):
    recs = []
    off = 0
    while off + REC <= len(data):
        name = data[off:off + 16].split(b"\0")[0].decode("latin1")
        if not name:
            break
        start, preload, loop, length = struct.unpack_from("<IIII", data, off + 0x10)
        wf = struct.unpack_from("<HHIIHHH", data, off + 0x20)
        tail = struct.unpack_from("<H", data, off + 0x32)[0]
        recs.append(dict(idx=len(recs), name=name, start=start, preload=preload,
                         loop=loop, length=length, wf=wf, tail=tail))
        off += REC
    return recs


def main():
    args = sys.argv[1:]
    csv_path = dat_path = None
    fmt_path = "thbgm.fmt"
    i = 0
    while i < len(args):
        if args[i] == "--csv":
            csv_path = args[i + 1]; i += 2
        elif args[i] == "--dat":
            dat_path = args[i + 1]; i += 2
        else:
            fmt_path = args[i]; i += 1

    data = open(fmt_path, "rb").read()
    recs = parse(data)
    print("# %s: %d bytes, %d records of 52, trailer %d bytes"
          % (fmt_path, len(data), len(recs), len(data) - len(recs) * REC))
    hdr = "idx name             start      preload   loop@+18     length       end   seconds  loop_s"
    print(hdr)
    for r in recs:
        end = r["start"] + r["length"]
        print("%3d %-16s 0x%08x %9d 0x%08x %10d 0x%08x %8.3f %6.3f"
              % (r["idx"], r["name"], r["start"], r["preload"], r["loop"], r["length"],
                 end, r["length"] / 176400.0, r["loop"] / 176400.0))

    wfs = set(r["wf"] for r in recs)
    print("\n## checks")
    print("  first track offset == 0x10      :", recs[0]["start"] == 0x10)
    print("  all offsets/lengths 4-aligned   :", all(
        r["start"] % 4 == 0 and r["length"] % 4 == 0 and r["loop"] % 4 == 0 for r in recs))
    print("  offsets contiguous (no gaps)    :", all(
        recs[i]["start"] + recs[i]["length"] == recs[i + 1]["start"] for i in range(len(recs) - 1)))
    print("  all loop < length (loop>0)      :", all(0 < r["loop"] < r["length"] for r in recs))
    print("  all preload >= length           :", all(r["preload"] >= r["length"] for r in recs))
    print("  single WAVEFORMATEX tuple       :", len(wfs) == 1, wfs)
    print("  trailing u16 zero after wf      :", all(r["tail"] == 0 for r in recs))
    if dat_path:
        import os
        size = os.path.getsize(dat_path)
        print("  last end == thbgm.dat size      :", recs[-1]["start"] + recs[-1]["length"] == size,
              "(%d bytes)" % size)

    if csv_path:
        with open(csv_path, "w", newline="") as fh:
            w = csv.writer(fh)
            w.writerow(["fmt_index", "name", "start_byte", "preload_bytes", "field_0x18",
                        "length_bytes", "end_byte", "seconds", "field_0x18_seconds"])
            for r in recs:
                w.writerow([r["idx"], r["name"], r["start"], r["preload"], r["loop"], r["length"],
                            r["start"] + r["length"], "%.3f" % (r["length"] / 176400.0),
                            "%.3f" % (r["loop"] / 176400.0)])
        print("\nwrote %s" % csv_path)


if __name__ == "__main__":
    main()
