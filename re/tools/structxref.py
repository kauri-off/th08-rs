#!/usr/bin/env python3
# structxref.py: resolve memory accesses through constant base pointers (regs, ebp slots, this, stack args)
# build: structxref.py build  -> extracted/structxref.tsv (ea, insn, func, R/W, size, how)
# query: structxref.py q <lo> [hi] [W]
import sys, os, struct, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pe import PE
import capstone
from capstone import x86

R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(R, 'extracted', 'structxref.tsv')
LO, HI = 0x400000, 0x1C00000
MAXCTX = 96

def load_funcs():
    fs = []
    for l in open(os.path.join(R, 'extracted', 'callgraph.tsv')):
        if l.startswith('addr'):
            continue
        a, n = l.split('\t')[:2]
        fs.append((int(a, 16), n))
    fs.sort()
    return fs

def is_ptr(v):
    return v is not None and LO <= v < HI

class Analyzer:
    def __init__(self):
        self.pe = PE()
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
        self.md.detail = True
        self.funcs = load_funcs()
        self.names = dict(self.funcs)
        self.code = {}
        for i, (a, n) in enumerate(self.funcs):
            end = self.funcs[i + 1][0] if i + 1 < len(self.funcs) else a + 0x2000
            end = min(end, a + 0x8000)
            try:
                b = self.pe.read(a, end - a)
            except ValueError:
                continue
            self.code[a] = list(self.md.disasm(b, a))
        self.ctx = collections.defaultdict(set)

    def run(self, rounds=10):
        for r in range(rounds):
            newctx = collections.defaultdict(set)
            acc = []
            for a in self.code:
                ctxs = self.ctx.get(a) or {(None, ())}
                ctxs = list(ctxs)[:MAXCTX]
                for c in ctxs:
                    self.walk(a, c, acc, newctx)
            changed = any(newctx[k] - self.ctx.get(k, set()) for k in newctx)
            for k, v in newctx.items():
                self.ctx[k] |= v
            print(f'round {r}: {len(acc)} accesses, ctx funcs {len(self.ctx)}', file=sys.stderr)
            if not changed:
                break
        return acc

    def walk(self, fa, c, acc, newctx):
        ecx0, args = c
        regs = {}
        if ecx0 is not None:
            regs[x86.X86_REG_ECX] = ecx0
        slots = {}
        for k, v in enumerate(args):
            if v is not None:
                slots[8 + 4 * k] = v
        pushes = []
        how = 'this' if ecx0 is not None else ''
        for ins in self.code[fa]:
            m = ins.mnemonic
            ops = ins.operands
            # memory operand resolution
            for o in ops:
                if o.type == x86.X86_OP_MEM:
                    base, idx, disp = o.mem.base, o.mem.index, o.mem.disp & 0xffffffff
                    if disp >= 0x80000000:
                        disp -= 0x100000000
                    if m == 'lea':
                        continue
                    if base == 0:
                        if LO <= (disp & 0xffffffff) < HI:
                            rw = 'W' if (o.access & capstone.CS_AC_WRITE) else 'R'
                            acc.append((disp & 0xffffffff, ins.address, fa, rw + ('i' if idx else '') + 'a', o.size, m))
                        continue
                    if base in regs and is_ptr(regs[base]):
                        ea = (regs[base] + disp) & 0xffffffff
                        rw = 'W' if (o.access & capstone.CS_AC_WRITE) else 'R'
                        if idx:
                            rw += 'i'
                        acc.append((ea, ins.address, fa, rw, o.size, m))
            # rep stos/movs
            if m.startswith('rep stos') or m.startswith('rep movs'):
                edi = regs.get(x86.X86_REG_EDI)
                if is_ptr(edi):
                    cnt = regs.get(x86.X86_REG_ECX)
                    n = cnt * (4 if m.endswith('d') else 1) if cnt is not None else -1
                    acc.append((edi, ins.address, fa, 'Wrep', n, m))
            # dataflow
            if m == 'call':
                tgt = ops[0].imm if ops and ops[0].type == x86.X86_OP_IMM else None
                if tgt in self.code:
                    e = regs.get(x86.X86_REG_ECX)
                    a = tuple(reversed(pushes[-6:])) if pushes else ()
                    e = e if is_ptr(e) else None
                    a = tuple(v if is_ptr(v) else None for v in a)
                    while a and a[-1] is None:
                        a = a[:-1]
                    if e is not None or a:
                        newctx[tgt].add((e, a))
                for r_ in (x86.X86_REG_EAX, x86.X86_REG_ECX, x86.X86_REG_EDX):
                    regs.pop(r_, None)
                pushes = []
                continue
            if m == 'push':
                o = ops[0]
                if o.type == x86.X86_OP_IMM:
                    pushes.append(o.imm & 0xffffffff)
                elif o.type == x86.X86_OP_REG:
                    pushes.append(regs.get(o.reg))
                elif o.type == x86.X86_OP_MEM and o.mem.base == x86.X86_REG_EBP and o.mem.index == 0:
                    pushes.append(slots.get(o.mem.disp))
                else:
                    pushes.append(None)
                continue
            if m == 'mov' and len(ops) == 2:
                d, s = ops
                val = 'unk'
                if s.type == x86.X86_OP_IMM:
                    val = s.imm & 0xffffffff
                elif s.type == x86.X86_OP_REG:
                    val = regs.get(s.reg)
                elif s.type == x86.X86_OP_MEM and s.mem.base == x86.X86_REG_EBP and s.mem.index == 0:
                    val = slots.get(s.mem.disp)
                elif s.type == x86.X86_OP_MEM:
                    val = None
                if d.type == x86.X86_OP_REG:
                    if d.size == 4:
                        if val is None or val == 'unk':
                            regs.pop(d.reg, None)
                        else:
                            regs[d.reg] = val
                    else:
                        self.clobber(ins, regs)
                    continue
                if d.type == x86.X86_OP_MEM and d.mem.base == x86.X86_REG_EBP and d.mem.index == 0:
                    if d.size == 4 and val not in (None, 'unk'):
                        slots[d.mem.disp] = val
                    else:
                        slots.pop(d.mem.disp, None)
                    continue
                continue
            if m == 'lea' and len(ops) == 2:
                d, s = ops
                if s.mem.base in regs and s.mem.index == 0 and is_ptr(regs[s.mem.base]):
                    regs[d.reg] = (regs[s.mem.base] + s.mem.disp) & 0xffffffff
                elif s.mem.base == 0 and s.mem.index == 0:
                    regs[d.reg] = s.mem.disp & 0xffffffff
                else:
                    regs.pop(d.reg, None)
                continue
            if m == 'add' and len(ops) == 2 and ops[0].type == x86.X86_OP_REG and ops[1].type == x86.X86_OP_IMM:
                r_ = ops[0].reg
                if r_ in regs:
                    regs[r_] = (regs[r_] + ops[1].imm) & 0xffffffff
                continue
            # memory store to ebp slot by other insns
            for o in ops:
                if o.type == x86.X86_OP_MEM and o.mem.base == x86.X86_REG_EBP and (o.access & capstone.CS_AC_WRITE):
                    slots.pop(o.mem.disp, None)
            self.clobber(ins, regs)

    def clobber(self, ins, regs):
        try:
            _, w = ins.regs_access()
        except Exception:
            return
        for r_ in w:
            for full in self.full(r_):
                regs.pop(full, None)

    FULL = {}
    def full(self, r):
        n = self.md.reg_name(r)
        m = {'al': 'eax', 'ah': 'eax', 'ax': 'eax', 'bl': 'ebx', 'bh': 'ebx', 'bx': 'ebx', 'cl': 'ecx', 'ch': 'ecx', 'cx': 'ecx',
             'dl': 'edx', 'dh': 'edx', 'dx': 'edx', 'si': 'esi', 'di': 'edi', 'bp': 'ebp'}
        n = m.get(n, n)
        ids = {'eax': x86.X86_REG_EAX, 'ebx': x86.X86_REG_EBX, 'ecx': x86.X86_REG_ECX, 'edx': x86.X86_REG_EDX,
               'esi': x86.X86_REG_ESI, 'edi': x86.X86_REG_EDI, 'ebp': x86.X86_REG_EBP}
        return [ids[n]] if n in ids else []

def build():
    an = Analyzer()
    acc = an.run()
    seen = set()
    with open(OUT, 'w') as f:
        f.write('ea\tinsn\tfunc\trw\tsize\tmnem\n')
        for ea, ia, fa, rw, sz, m in sorted(acc):
            k = (ea, ia, rw)
            if k in seen:
                continue
            seen.add(k)
            f.write(f'{ea:08x}\t{ia:08x}\t{an.names.get(fa, hex(fa))}\t{rw}\t{sz}\t{m}\n')
    print(OUT, len(seen))

def query(lo, hi, wonly):
    for l in open(OUT):
        if l.startswith('ea'):
            continue
        p = l.rstrip('\n').split('\t')
        ea = int(p[0], 16)
        if p[3] == 'Wrep':
            n = int(p[4])
            hit = ea <= hi and (n < 0 or ea + n > lo)
        else:
            hit = lo <= ea <= hi
        if hit and (not wonly or p[3].startswith('W')):
            print(l, end='')

if __name__ == '__main__':
    if sys.argv[1] == 'build':
        build()
    else:
        lo = int(sys.argv[2], 16)
        hi = int(sys.argv[3], 16) if len(sys.argv) > 3 and sys.argv[3] != 'W' else lo
        query(lo, hi, 'W' in sys.argv[3:])
