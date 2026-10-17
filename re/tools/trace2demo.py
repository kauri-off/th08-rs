#!/usr/bin/env python3
# trace2demo.py <probe_trace.txt> <outdir> : split probe trace into per-demo ground-truth files
import sys, struct, os

COLS = ("calc present demo stage loading frame demo_frame input rng rng_ctr rng_total px py pstate "
        "score graze points lives bombs power gauge time bullets enemies rank spell boss pause flags pass").split()
STAGES = ["1", "2", "3", "4A", "4B", "5", "6A", "6B", "Ex"]
DEMO_FILE = {0: "demorpy0", 1: "demorpy1", 2: "demorpy2", 3: "demorpy3"}


def f32(h):
    return struct.unpack("<f", struct.pack("<I", int(h, 16)))[0]


def main(src, outdir):
    segs, prev = [], None
    for line in open(src):
        r = dict(zip(COLS, line.split()))
        if prev is None or r["demo"] != prev["demo"] or int(r["calc"]) > int(prev["calc"]) + 1:
            segs.append([])
        segs[-1].append(r)
        prev = r
    runs = {}
    for seg in segs:
        d = int(seg[0]["demo"])
        if d in runs:
            print(f"demo {d}: extra occurrence ({len(seg)} rows) ignored")
            continue
        runs[d] = seg
    for d, rows in sorted(runs.items()):
        load = [r for r in rows if r["loading"] != "0"]
        play = [r for r in rows if r["loading"] == "0" and int(r["demo_frame"]) > 0]
        out, last = [], None
        for r in play:
            if last is not None and r["demo_frame"] == last["demo_frame"]:
                continue
            if last is not None and int(r["demo_frame"]) != int(last["demo_frame"]) + 1:
                print(f"demo {d}: gap {last['demo_frame']} -> {r['demo_frame']}")
            last = r
            out.append(r)
        base_ctr = int(load[-1]["rng_ctr"]) if load else 0
        path = os.path.join(outdir, f"demo{d}_trace.txt")
        with open(path, "w") as f:
            f.write(f"# th08 ground truth, attract demo {d} = {DEMO_FILE[d]}.rpy, stage {STAGES[int(rows[0]['stage'])]}\n")
            f.write(f"# before frame 1: rng state {load[-1]['rng']} (after {len(load)} loading frames); "
                    f"rng_ctr = Next16 calls since then (cumulative)\n")
            f.write("# frame demo_frame input rng rng_ctr px py pstate score graze points lives bombs power gauge time bullets enemies rank spell boss flags ff\n")
            for r in out:
                f.write(" ".join([r["frame"], r["demo_frame"], r["input"], r["rng"],
                                  str(int(r["rng_ctr"]) - base_ctr), r["px"], r["py"], r["pstate"],
                                  r["score"], r["graze"], r["points"], r["lives"], r["bombs"], r["power"],
                                  r["gauge"], r["time"], r["bullets"], r["enemies"], r["rank"], r["spell"],
                                  r["boss"], "%x" % int(r["flags"]), r["pass"]]) + "\n")
        print(path, len(out), "frames", "first px,py=%.3f,%.3f" % (f32(out[0]["px"]), f32(out[0]["py"])))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
