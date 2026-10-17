#!/usr/bin/env python3
# dis.py <va> [count|-before:after] : capstone disassembly around an address
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pe import PE
import capstone
p = PE(); md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
va = int(sys.argv[1], 16)
before, after = 0x30, 0x30
if len(sys.argv) > 2:
    a = sys.argv[2]
    if ':' in a:
        before, after = (int(x, 0) for x in a.split(':'))
    else:
        before, after = 0, int(a, 0)
start = va - before
for i in md.disasm(p.read(start, before + after), start):
    print(('>' if i.address == va else ' '), hex(i.address), i.mnemonic, i.op_str)
