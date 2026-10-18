#!/usr/bin/env python3
"""Session recorder helper (format: specs/data/drawlog/README.md "Sessions").
  session.py finalize <sessdir>            polls.txt -> input.txt (TH08_INPUT) + meta.txt; xz trace.txt
  session.py compare  <sessdir> <playdir>  trace rows (aligned at the first main-menu frame) + draw logs
"""
import glob, lzma, os, subprocess, sys

TRACE_COLS = ("frame demo_frame input rng rng_ctr px py pstate score graze points lives bombs power gauge time "
              "bullets enemies rank spell boss flags ff present calc poll scene scene_frame menu cursor result "
              "pause rawinput stage loading demo").split()
SKIP = {"present", "calc", "poll"}
# written by the stage loader thread while the main thread keeps running loading frames (timing-dependent)
LOADER_COLS = {"rng", "rng_ctr", "px", "py", "pstate", "rank", "flags", "lives", "bombs", "power"}


def opentext(path):
    if os.path.exists(path):
        return open(path)
    if os.path.exists(path + ".xz"):
        return lzma.open(path + ".xz", "rt")
    raise FileNotFoundError(path)


def read_polls(d):
    out = []
    for line in opentext(os.path.join(d, "polls.txt")):
        if line.startswith("#"):
            continue
        i, pr, sc, src, m = line.split()
        out.append((int(i), int(pr), int(sc), src, int(m, 16)))
    return out


def menu_base(d):
    for line in open(os.path.join(d, "probe_log.txt"), errors="replace"):
        if line.startswith("# scene ") and " -> 1 at present " in line:
            return int(line.split()[-1])
    return None


def rle(words):
    runs = []
    for w in words:
        if runs and runs[-1][1] == w:
            runs[-1][0] += 1
        else:
            runs.append([1, w])
    return runs


def finalize(d):
    polls = read_polls(d)
    base = menu_base(d)
    meta = {}
    mp = os.path.join(d, "meta.txt")
    if os.path.exists(mp):
        for line in open(mp):
            if ":" in line:
                k, v = line.split(":", 1)
                meta[k.strip()] = v.strip()
    if base is None or not polls:
        print("no main-menu frame / no polls: session incomplete")
        return
    last_present = polls[-1][1]
    by_present = {}
    multi = 0
    for i, pr, sc, src, m in polls:
        if pr in by_present:
            multi += 1
        by_present[pr] = m
    words, cur, nopoll = [], 0, 0
    for pr in range(base, last_present + 1):
        if pr in by_present:
            cur = by_present[pr]
        else:
            nopoll += 1
        words.append(cur)
    boot = [p for p in polls if p[1] < base]
    with open(os.path.join(d, "input.txt"), "w") as f:
        f.write("# TH08_INPUT for this session: frames:mask, frame 1 = first presented main-menu frame "
                f"(present {base}); {len(words)} frames\n")
        f.write(f"# frames without an Input_Poll call (value repeated): {nopoll}; presents with >1 poll "
                f"(last one written): {multi}; exact per-poll list: polls.txt\n")
        runs = rle(words)
        for j in range(0, len(runs), 16):
            f.write(",".join(f"{n}:0x{w:x}" for n, w in runs[j:j + 16]) + "\n")
    seed = None
    for line in open(os.path.join(d, "events.txt")):
        if line.startswith("seed "):
            seed = line.split()[1]
            break
    first_menu_poll = next((p[0] for p in polls if p[1] >= base), None)
    meta.update({
        "menu_base_present": base,
        "boot_presents": base - 1,
        "boot_polls": len(boot),
        "first_menu_poll": first_menu_poll,
        "polls": len(polls),
        "last_present": last_present,
        "frames_without_poll": nopoll,
        "presents_with_multiple_polls": multi,
        "boot_seed": seed,
        "rng_state_after_seed": "%04x" % (int(seed, 16) & 0xFFFF) if seed else None,
    })
    with open(mp, "w") as f:
        for k, v in meta.items():
            f.write(f"{k}: {v}\n")
    tp = os.path.join(d, "trace.txt")
    if os.path.exists(tp):
        subprocess.run(["xz", "-T0", "-6", "-f", tp], check=True)
    for p in sorted(glob.glob(os.path.join(d, "draw_*.txt"))):
        if os.path.getsize(p) == 0:
            os.remove(p)
            continue
        subprocess.run(["xz", "-T0", "-6", "-f", p], check=True)
        os.rename(p + ".xz", p[:-4] + ".drawlog.xz")
    print(f"{d}: {len(polls)} polls, menu base present {base}, {len(words)} input frames, seed {seed}")


def trace_rows(d):
    rows = []
    for line in opentext(os.path.join(d, "trace.txt")):
        if line.startswith("#"):
            continue
        rows.append(dict(zip(TRACE_COLS, line.split())))
    i0 = next(i for i, r in enumerate(rows) if r["scene"] == "1")
    return rows[i0:]


def compare(a, b, maxrep=20):
    ra, rb = trace_rows(a), trace_rows(b)
    n = min(len(ra), len(rb))
    bad = 0
    for i in range(n):
        skip = SKIP | (LOADER_COLS if ra[i].get("loading") != "0" or rb[i].get("loading") != "0" else set())
        diff = [c for c in TRACE_COLS if c not in skip and ra[i].get(c) != rb[i].get(c)]
        if diff:
            bad += 1
            if bad <= maxrep:
                print(f"row {i} (rec present {ra[i]['present']} / play {rb[i]['present']}): " +
                      ", ".join(f"{c} {ra[i].get(c)}|{rb[i].get(c)}" for c in diff))
    print(f"trace: {n} rows compared from the first main-menu row (rec {len(ra)}, play {len(rb)}), {bad} differ")
    da = sorted(glob.glob(os.path.join(a, "draw_*.drawlog.xz")))
    db = sorted(glob.glob(os.path.join(b, "draw_*.drawlog.xz")))
    if da and db:
        tool = os.path.join(os.path.dirname(os.path.abspath(__file__)), "drawlog.py")
        subprocess.run([sys.executable, "-I", tool, "diff", os.path.join(a, "draw_*.drawlog.xz"),
                        os.path.join(b, "draw_*.drawlog.xz"), "--key", "scene"])
    return bad


def main(argv):
    if len(argv) >= 2 and argv[0] == "finalize":
        finalize(argv[1])
    elif len(argv) >= 3 and argv[0] == "compare":
        sys.exit(1 if compare(argv[1], argv[2]) else 0)
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main(sys.argv[1:])
