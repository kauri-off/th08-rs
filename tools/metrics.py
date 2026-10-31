#!/usr/bin/env python3
"""usage: metrics.py TAG [COMMIT] — append one row of parity metrics for TAG (from target/longcheck_TAG.txt + drawgroups dirs) to docs/METRICS.csv."""
import csv, glob, os, re, subprocess, sys, time

R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
T = os.path.join(R, "target")
COLS = ["date", "commit", "tag", "pilot", "ref500", "ref_b2", "golden_draw_identical", "golden_draw_frames",
        "drawset_identical_pct", "drawset_groups", "dl3_identical_pct", "dl3_groups", "spec_gaps", "broken_spec_refs"]


def ident(path):
    rows = [l.rstrip("\n").split("\t") for l in open(path)] if os.path.exists(path) else []
    rows = [r for r in rows if len(r) >= 3 and r[1].isdigit()]
    return rows


def pct(rows):
    c = sum(int(r[1]) for r in rows if r[0] != "golden")
    i = sum(int(r[2]) for r in rows if r[0] != "golden")
    return f"{100 * i / c:.2f}" if c else ""


def groups(d):
    p = os.path.join(d, "all_groups.tsv")
    return str(sum(1 for _ in open(p)) - 1) if os.path.exists(p) else ""


def batch(tag, prefix):
    p = os.path.join(T, f"{prefix}_{tag}", "summary.txt")
    if not os.path.exists(p):
        return ""
    lines = open(p).read().splitlines()
    return f"{sum(l.endswith(' identical') for l in lines)}/{len(lines)}"


def main():
    tag = sys.argv[1]
    row = dict.fromkeys(COLS, "")
    row.update(date=time.strftime("%Y-%m-%d"), tag=tag,
               commit=sys.argv[2] if len(sys.argv) > 2 else subprocess.run(["git", "-C", R, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip())
    pilot = glob.glob(os.path.join(T, f"pilot_{tag}", "*.txt"))
    if pilot:
        row["pilot"] = f"{sum('no differing row' in open(p).read() for p in pilot)}/{len(pilot)}"
    row["ref500"], row["ref_b2"] = batch(tag, "batch"), batch(tag, "batchb2")
    dg = os.path.join(T, f"drawgroups_{tag}")
    rows = ident(os.path.join(dg, "identical.tsv"))
    g = [r for r in rows if r[0] == "golden"]
    if g:
        row["golden_draw_identical"], row["golden_draw_frames"] = g[0][2], g[0][1]
    row["drawset_identical_pct"], row["drawset_groups"] = pct(rows), groups(dg)
    dl3 = os.path.join(T, f"drawgroups_dl3_{tag}")
    row["dl3_identical_pct"], row["dl3_groups"] = pct(ident(os.path.join(dl3, "identical.tsv"))), groups(dl3)
    row["spec_gaps"] = str(sum(open(p, encoding="utf-8").read().count("SPEC-GAP:") for p in glob.glob(f"{R}/src/**/*.rs", recursive=True)))
    row["broken_spec_refs"] = str(len([l for l in subprocess.run([sys.executable, os.path.join(R, "tools", "spec_refs.py")], capture_output=True, text=True).stdout.split("\n") if l.strip()]))
    out = os.path.join(R, "docs", "METRICS.csv")
    new = not os.path.exists(out)
    with open(out, "a", newline="") as f:
        w = csv.DictWriter(f, COLS)
        if new:
            w.writeheader()
        w.writerow(row)
    print(",".join(row[c] for c in COLS))


if __name__ == "__main__":
    main()
