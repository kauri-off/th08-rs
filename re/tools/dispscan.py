#!/usr/bin/env python3
# dispscan.py <disp> [disp...] : every game-code instruction whose memory operand has this displacement, with function name
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pe import PE
import capstone
from capstone import x86
R = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
p = PE(); md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32); md.detail = True
fns = []
for l in open(os.path.join(R, 'extracted/callgraph.tsv')):
    f = l.rstrip('\n').split('\t')
    try: fns.append((int(f[0], 16), f[1]))
    except ValueError: pass
fns.sort()
want = {int(a, 0) for a in sys.argv[1:]}
for k, (a, n) in enumerate(fns):
    if a >= 0x476CE0: break
    end = fns[k + 1][0] if k + 1 < len(fns) else a + 0x1000
    for i in md.disasm(p.read(a, end - a), a):
        for op in i.operands:
            if op.type == x86.X86_OP_MEM and op.mem.disp in want:
                print(hex(i.address), n, i.mnemonic, i.op_str)
