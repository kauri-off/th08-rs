#!/usr/bin/env python3
# coverage.py: functions reachable (direct calls + pointer refs) vs. addresses cited in specs/*.md
# usage: coverage.py [--list] ; writes extracted/coverage.tsv (addr, name, size, reachable, cited, cited_by_caller, root_caller)
import sys, os, re, struct, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pe import PE

R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROOT = os.path.dirname(R)

funcs, callers, callees = [], {}, {}
names = {}
for l in open(os.path.join(R, 'extracted', 'callgraph.tsv')):
    if l.startswith('addr'):
        continue
    p = l.rstrip('\n').split('\t')
    a = int(p[0], 16)
    funcs.append(a)
    names[a] = p[1]
    callers[a] = [x for x in p[2].split(',') if x]
    callees[a] = [x for x in p[3].split(',') if x]
funcs.sort()
byname = {}
for a, n in names.items():
    byname.setdefault(n, a)
size = {a: (funcs[i + 1] - a if i + 1 < len(funcs) else 0x100) for i, a in enumerate(funcs)}
fset = set(funcs)

pe = PE()
ptr_refd = set()
d = pe.d
for name, sva, vsz, rptr, rsz in pe.secs:
    blob = d[rptr:rptr + rsz]
    for off in range(0, len(blob) - 3):
        v = struct.unpack_from('<I', blob, off)[0]
        if v in fset:
            ptr_refd.add(v)

cited = collections.defaultdict(set)
for fn in sorted(os.listdir(os.path.join(ROOT, 'specs'))):
    if not fn.endswith('.md'):
        continue
    t = open(os.path.join(ROOT, 'specs', fn), encoding='utf-8').read()
    for m in re.finditer(r'0x([0-9A-Fa-f]{6})\b', t):
        v = int(m.group(1), 16)
        if v in fset:
            cited[v].add(fn.split('_')[0].split('.')[0])
    for n, a in byname.items():
        if not n.startswith('FUN_') and len(n) > 5 and n in t:
            cited[a].add(fn.split('_')[0].split('.')[0])

roots = [byname.get('WinMain', 0)] + sorted(ptr_refd)
reach = {}
dq = collections.deque()
for r_ in roots:
    if r_ in fset and r_ not in reach:
        reach[r_] = names[r_]
        dq.append(r_)
while dq:
    a = dq.popleft()
    for c in callees[a]:
        ca = byname.get(c)
        if ca is not None and ca not in reach:
            reach[ca] = reach[a]
            dq.append(ca)

LIB_LO = 0x4A3000
with open(os.path.join(R, 'extracted', 'coverage.tsv'), 'w') as f:
    f.write('addr\tname\tsize\treach\tcited\tcited_callers\troot\n')
    for a in funcs:
        cc = sorted({s for c in callers[a] for s in cited.get(byname.get(c, -1), ())})
        f.write(f'{a:08x}\t{names[a]}\t{size[a]}\t{int(a in reach)}\t{",".join(sorted(cited.get(a, ())))}\t{",".join(cc)}\t{reach.get(a, "")}\n')

game = [a for a in funcs if a < LIB_LO]
r_ = [a for a in game if a in reach]
c_ = [a for a in r_ if cited.get(a)]
print(f'functions {len(funcs)}, game-range {len(game)}, reachable {len(r_)}, cited {len(c_)}')
print(f'bytes reachable {sum(size[a] for a in r_)}, cited {sum(size[a] for a in c_)}')
