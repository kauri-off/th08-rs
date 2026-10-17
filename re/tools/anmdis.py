#!/usr/bin/env python3
"""ANM script disassembler. Usage: anmdis.py <file.anm|dir> [-q]   (-q: only coverage summary)
Arg type letters: i=int32 f=float32 V=int var-ref(dest) W=float var-ref(dest) O=byte offset (from script start) T=time"""
import os, struct, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anm

OPS = {
    -1: ('end', ''), 0: ('nop', ''), 1: ('hide_stop', ''), 2: ('stop', ''),
    3: ('sprite', 'i'), 4: ('jump', 'OT'), 5: ('loop_dec', 'VOT'),
    6: ('pos', 'fff'), 7: ('scale', 'ff'), 8: ('alpha', 'i'), 9: ('color', 'iii'),
    10: ('flip_x', ''), 11: ('flip_y', ''), 12: ('rotation', 'fff'), 13: ('angular_vel', 'fff'),
    14: ('scale_growth', 'ff'), 15: ('alpha_fade', 'ii'), 16: ('blend_additive', 'i'),
    17: ('pos_time_linear', 'fffi'), 18: ('pos_time_out2', 'fffi'), 19: ('pos_time_out4', 'fffi'),
    20: ('wait_interrupt', ''), 21: ('interrupt_label', 'i'), 22: ('anchor_top_left', ''),
    23: ('hide_wait_interrupt', ''), 24: ('pos_target_offset', 'i'), 25: ('set_tag', 'i'),
    26: ('uv_add_u', 'f'), 27: ('uv_add_v', 'f'), 28: ('visible', 'i'),
    29: ('scale_time_linear', 'ffi'), 30: ('zwrite_off', 'i'), 31: ('stage_camera', 'i'),
    32: ('pos_time', 'iifff'), 33: ('color_time', 'iiiii'), 34: ('alpha_time', 'iii'),
    35: ('rotation_time', 'iifff'), 36: ('scale_time', 'iiff'),
    37: ('iset', 'Vi'), 38: ('fset', 'Wf'), 39: ('iadd', 'Vi'), 40: ('fadd', 'Wf'), 41: ('isub', 'Vi'),
    42: ('fsub', 'Wf'), 43: ('imul', 'Vi'), 44: ('fmul', 'Wf'), 45: ('idiv', 'Vi'), 46: ('fdiv', 'Wf'),
    47: ('imod', 'Vi'), 48: ('fmod', 'Wf'),
    49: ('iadd3', 'Vii'), 50: ('fadd3', 'Wff'), 51: ('isub3', 'Vii'), 52: ('fsub3', 'Wff'),
    53: ('imul3', 'Vii'), 54: ('fmul3', 'Wff'), 55: ('idiv3', 'Vii'), 56: ('fdiv3', 'Wff'),
    57: ('imod3', 'Vii'), 58: ('fmod3', 'Wff'), 59: ('irand', 'Vi'), 60: ('frand', 'Wf'),
    61: ('fsin', 'Wf'), 62: ('fcos', 'Wf'), 63: ('ftan', 'Wf'), 64: ('facos', 'Wf'), 65: ('fatan', 'Wf'),
    66: ('norm_angle', 'W'),
    67: ('ije', 'iiOT'), 68: ('fje', 'ffOT'), 69: ('ijne', 'iiOT'), 70: ('fjne', 'ffOT'),
    71: ('ijl', 'iiOT'), 72: ('fjl', 'ffOT'), 73: ('ijle', 'iiOT'), 74: ('fjle', 'ffOT'),
    75: ('ijg', 'iiOT'), 76: ('fjg', 'ffOT'), 77: ('ijge', 'iiOT'), 78: ('fjge', 'ffOT'),
    79: ('wait', 'i'), 80: ('uv_scroll_u', 'f'), 81: ('uv_scroll_v', 'f'), 82: ('blend_mode', 'i'),
    83: ('draw_mode', 'i'), 84: ('color2', 'iii'), 85: ('alpha2', 'i'), 86: ('color2_time', 'iiiii'),
    87: ('alpha2_time', 'iii'), 88: ('use_color2', 'i'), 89: ('return_interrupt', ''),
}
VARS = {10000: '$I0', 10001: '$I1', 10002: '$I2', 10003: '$I3', 10004: '%F0', 10005: '%F1', 10006: '%F2',
        10007: '%F3', 10008: '$I4', 10009: '$I5'}


def fmt_arg(t, raw, isvar):
    iv = struct.unpack('<i', raw)[0]; fv = struct.unpack('<f', raw)[0]
    if isvar:
        k = iv if t in 'iVOT' else int(fv)
        return VARS.get(k, '?var(%r)' % (iv if t in 'iVOT' else fv))
    if t in 'fW': return repr(round(fv, 6))
    return str(iv)


def script_instrs(d, start):
    p = start; out = []
    while True:
        op, sz, t, mask = struct.unpack_from('<hHhH', d, p)
        out.append((p - start, op, sz, t, mask, d[p + 8:p + sz]))
        if op == -1: break
        if sz < 8: raise ValueError('bad size at %x' % p)
        p += sz
    return out


def main():
    a = sys.argv[1]; quiet = '-q' in sys.argv
    files = [os.path.join(a, f) for f in sorted(os.listdir(a)) if f.endswith('.anm')] if os.path.isdir(a) else [a]
    used = collections.Counter(); problems = []
    for fn in files:
        d = open(fn, 'rb').read(); bn = os.path.basename(fn)
        for ei, e in enumerate(anm.parse(d)):
            offs = sorted(so for _, so in e['scripts'])
            for si, (sid, so) in enumerate(e['scripts']):
                start = e['base'] + so
                ins = script_instrs(d, start)
                if not quiet: print('%s entry %d script %d (id %d) @0x%x' % (bn, ei, si, sid, start))
                bounds = {i[0] for i in ins}
                end = start + ins[-1][0] + 8
                nxt = [o for o in offs if e['base'] + o > start]
                if nxt and e['base'] + nxt[0] != end:
                    problems.append('%s e%d s%d: gap/overlap to next script (%x vs %x)' % (bn, ei, si, end, e['base'] + nxt[0]))
                for off, op, sz, t, mask, args in ins:
                    used[op] += 1
                    if op not in OPS:
                        problems.append('%s e%d s%d +%x: unknown op %d' % (bn, ei, si, off, op)); continue
                    name, sig = OPS[op]
                    if len(args) != 4 * len(sig):
                        problems.append('%s e%d s%d +%x: op %d size %d != sig %s' % (bn, ei, si, off, op, sz, sig))
                    vals = []
                    for k, ty in enumerate(sig):
                        raw = args[4 * k:4 * k + 4]
                        if len(raw) < 4: break
                        vals.append(fmt_arg(ty, raw, bool(mask >> k & 1)))
                        if ty == 'O' and not (mask >> k & 1):
                            tgt = struct.unpack('<i', raw)[0]
                            if tgt not in bounds: problems.append('%s e%d s%d +%x: jump target %d not instr' % (bn, ei, si, off, tgt))
                    if not quiet:
                        print('  %5d +%04x %-20s %s%s' % (t, off, name, ', '.join(vals), '' if not mask else '   ;mask=%x' % mask))
    print('== opcode coverage (op: count) ==')
    print(' '.join('%d:%d' % (k, used[k]) for k in sorted(used)))
    unk = [k for k in used if k not in OPS]
    print('unknown opcodes:', unk)
    print('problems:', len(problems))
    for p in problems[:50]: print('  ', p)


if __name__ == '__main__':
    main()
