#!/usr/bin/env python3
"""Check `NN §x.y` spec citations in src/ and crates/ against the numbered spec headings; exit 1 on broken ones."""
import glob, os, re, sys

R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CITE = re.compile(r"\b(0\d|1[0-6]) §(\d+(?:\.\d+)*[a-z]?)")
HEAD = re.compile(r"^#+\s+§?(\d+(?:\.\d+)*[a-z]?)[.\s)]")
secs = {}
for p in glob.glob(f"{R}/specs/[01][0-9]_*.md"):
    n = os.path.basename(p)[:2]
    s = secs.setdefault(n, set())
    for l in open(p, encoding="utf-8"):
        m = HEAD.match(l)
        if m:
            s.add(m.group(1))
bad = []
for p in sorted(glob.glob(f"{R}/src/**/*.rs", recursive=True) + glob.glob(f"{R}/crates/**/*.rs", recursive=True)):
    for i, l in enumerate(open(p, encoding="utf-8"), 1):
        for m in CITE.finditer(l):
            n, sec = m.groups()
            # `2.6c` may cite a lettered paragraph inside heading 2.6
            if not {sec, sec.rstrip("abcdefghijklmnopqrstuvwxyz")} & secs.get(n, set()):
                bad.append(f"{os.path.relpath(p, R)}:{i}: {n} §{sec}")
print("\n".join(bad))
print(f"spec_refs: {len(bad)} broken citation(s)", file=sys.stderr)
sys.exit(1 if bad else 0)
