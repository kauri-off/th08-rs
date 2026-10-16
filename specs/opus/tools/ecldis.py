#!/usr/bin/env python3
"""ECL disassembler (subs + timelines). Usage: ecldis.py <file.ecl|dir> [-q]
Arg letters: i int, f float, V int-var dest, W float-var dest, T time, O byte offset (relative to instr),
S sub id, X raw dword, b raw byte-packed dword, B bullet block (8 dwords), L laser block (13), C spellcard blob."""
import os, struct, sys, collections

OPS = {
 0: ('nop', ''), 1: ('delete', ''), 2: ('wait', 'i'), 3: ('nop3', 'X'), 4: ('jump', 'TO'), 5: ('loop', 'TOV'),
 6: ('iset', 'Vi'), 7: ('fset', 'Wf'), 8: ('irandsign', 'Vi'), 9: ('frandsign', 'Wf'),
 10: ('iadd', 'Vi'), 11: ('isub', 'Vi'), 12: ('imul', 'Vi'), 13: ('idiv', 'Vi'), 14: ('imod', 'Vi'),
 15: ('fadd', 'Wf'), 16: ('fsub', 'Wf'), 17: ('fmul', 'Wf'), 18: ('fdiv', 'Wf'), 19: ('fmod', 'Wf'),
 20: ('iadd3', 'Vii'), 21: ('isub3', 'Vii'), 22: ('imul3', 'Vii'), 23: ('idiv3', 'Vii'), 24: ('imod3', 'Vii'),
 25: ('fadd3', 'Wff'), 26: ('fsub3', 'Wff'), 27: ('fmul3', 'Wff'), 28: ('fdiv3', 'Wff'), 29: ('fmod3', 'Wff'),
 30: ('inc', 'V'), 31: ('dec', 'V'), 32: ('fsin', 'Wf'), 33: ('fcos', 'Wf'), 34: ('fatan2', 'Wffff'),
 35: ('flerp', 'Wfff'), 36: ('interp', 'Xiiiffff'), 37: ('normangle', 'W'), 38: ('polar', 'WWff'), 39: ('fdist', 'Wffff'),
 40: ('ije', 'iiTO'), 41: ('fje', 'ffTO'), 42: ('ijne', 'iiTO'), 43: ('fjne', 'ffTO'), 44: ('ijl', 'iiTO'), 45: ('fjl', 'ffTO'),
 46: ('ijle', 'iiTO'), 47: ('fjle', 'ffTO'), 48: ('ijg', 'iiTO'), 49: ('fjg', 'ffTO'), 50: ('ijge', 'iiTO'), 51: ('fjge', 'ffTO'),
 52: ('call', 'S'), 53: ('return', ''), 54: ('anm', 'i'), 55: ('anm_dir', 'i'), 56: ('anm_dir6', 'iiiiii'), 57: ('subanm', 'ii'),
 58: ('anm_stg', 'i'), 59: ('anm_dir_stg', 'i'), 60: ('anm_dir6_stg', 'iiiiii'), 61: ('subanm_stg', 'ii'), 62: ('anm_replay', ''),
 63: ('set_pos', 'ff'), 64: ('move_to', 'iiff'), 65: ('set_dir', 'ff'), 66: ('move_dir', 'iiff'), 67: ('move_rand', 'iif'),
 68: ('set_dir_aim', 'ff'), 69: ('move_dir_aim', 'iiff'), 70: ('set_angvel', 'f'), 71: ('set_accel', 'f'),
 72: ('orbit', 'iffffff'), 73: ('orbit_here', 'ifff'), 74: ('orbit_vel', 'iff'), 75: ('clamp_rect', 'ffff'), 76: ('clamp_off', ''),
 77: ('hitbox', 'ff'), 78: ('shotbox', 'ff'), 79: ('flags_set', 'i'), 80: ('flags_off', 'i'), 81: ('flags_on', 'i'),
 82: ('nofire_radius', 'f'), 83: ('flag_3328_1', 'i'), 84: ('nop84', ''), 85: ('nop85', ''), 86: ('iget_boss', 'Vii'), 87: ('fget_boss', 'Wii'),
 88: ('call_boss', 'iS'), 89: ('interrupt_boss', 'ii'), 90: ('child', 'Sffiii'), 91: ('child_rel', 'Sffiii'), 92: ('child_follow', 'Sffiii'),
 93: ('spawn', 'Sfffiii'), 94: ('spawn_rel', 'Sfffiii'), 95: ('kill_all', ''),
 96: ('bul_fan_aim', 'B'), 97: ('bul_fan', 'B'), 98: ('bul_ring_aim', 'B'), 99: ('bul_ring', 'B'), 100: ('bul_ring_off_aim', 'B'),
 101: ('bul_ring_off', 'B'), 102: ('bul_rand_angle', 'B'), 103: ('bul_rand_speed', 'B'), 104: ('bul_rand', 'B'),
 105: ('autofire', 'i'), 106: ('autofire_rand', 'i'), 107: ('autofire_store_on', ''), 108: ('autofire_store_off', ''), 109: ('fire_stored', ''),
 110: ('shot_offset', 'ff'), 111: ('bullet_ex', 'iiiiiff'), 112: ('bullets_412', ''), 113: ('bullet_sfx', 'ii'),
 114: ('laser', 'L'), 115: ('laser_b', 'L'), 116: ('laser_slot', 'i'), 117: ('laser_rotate', 'if'), 118: ('laser_aim', 'if'),
 119: ('laser_offset', 'ifff'), 120: ('laser_alive', 'i'), 121: ('laser_cancel', 'i'), 122: ('spellcard', 'C'), 123: ('spellcard_end', ''),
 124: ('sfx', 'i'), 125: ('interrupt_now', 'i'), 126: ('set_interrupt', 'Si'), 127: ('boss', 'i'), 128: ('effect13', 'Xfffi'),
 129: ('death_mode', 'b'), 130: ('death_sub', 'b'), 131: ('set_life', 'i'), 132: ('set_timer', 'i'), 133: ('life_threshold', 'iiS'),
 134: ('timeout', 'iS'), 135: ('thread', 'iS'), 136: ('native', 'ii'), 137: ('native_cb', 'ii'), 138: ('set_bytes', 'b'),
 139: ('effect', 'iiV'), 140: ('effect_dir', 'iiifff'), 141: ('item', 'i'), 142: ('item_burst', 'i'), 143: ('set_item', 'i'),
 144: ('extra_items', 'ii'), 145: ('flag25', 'b'), 146: ('time_add', 'i'), 147: ('std_section', 'i'), 148: ('hud148', 'i'),
 149: ('anm_interrupt', 'i'), 150: ('subanm_interrupt', 'XX'), 151: ('no_stack', 'b'), 152: ('rank_params', 'ffiiii'),
 153: ('timeout_sub_from_death', ''), 154: ('laser_clear', ''), 155: ('flag27', 'b'), 156: ('flag7', 'b'), 157: ('trail', 'biii'),
 158: ('hp_marker', 'iiii'), 159: ('draw_layer', 'i'), 160: ('dmg_timer', 'i'), 161: ('cancel_radius', 'f'), 162: ('cancel_all', ''),
 163: ('g_f54cec', 'i'), 164: ('spell_mode', 'ifff'), 165: ('anm_rot_z', 'f'), 166: ('polar_ys', 'WWff'), 167: ('laser_angle', 'if'),
 168: ('item_burst1', 'i'), 169: ('rand_away_angle', 'W'), 170: ('laser_599', 'ii'), 171: ('laser_560', 'if'), 172: ('laser_558', 'iff'),
 173: ('flag30', 'i'), 174: ('aura', 'i'), 175: ('spawn_disable', 'i'), 176: ('last_spell', 'X'), 177: ('set_2e04', 'i'),
 178: ('move_rand2', 'iif'), 179: ('hud179', ''), 180: ('hud180', ''), 181: ('extend181', ''), 182: ('flag_3328_8', 'i'),
 183: ('flag31', 'i'), 184: ('spell_ex', 'i'),
}
TL_OPS = {0: ('spawn', 'Sffiii'), 1: ('spawn_mirror', 'Sffiii'), 2: ('spawn_randx', 'Sfffiii'), 3: ('spawn_randx_full', 'Sfiii'),
          4: ('spawn_randx_mirror', 'Sfffiii'), 5: ('spawn_randx_full_mirror', 'Sfiii'), 6: ('msg', 'i'), 7: ('wait_msg', ''),
          8: ('interrupt_boss', 'ii'), 9: ('gm_9', 'i'), 10: ('wait_boss', 'i'), 11: ('spawn_items', 'Sffiiii'),
          12: ('spawn_items_mirror', 'Sffiiii'), 13: ('wait_sync', 'i'), 14: ('post_sync', 'i'), 15: ('spawn_force', 'Sffiii'), 16: ('flag_164d0bb', '')}
SIZES = {'B': 32, 'L': 52, 'C': 232}


def varname(v):
    if 10000 <= v <= 10100: return '$%d' % v
    return None


def fmt(sig, args, mask):
    out = []; k = 0; p = 0
    for c in sig:
        if c in SIZES:
            if c == 'B':
                t, col = struct.unpack_from('<hh', args, 0); w = struct.unpack_from('<iiffffI', args, 4)
                names = ['type', 'col', 'n1', 'n2', 's1', 's2', 'a1', 'a2']
                vals = [t, col] + list(w[:6])
                for j, (nm, v) in enumerate(zip(names, vals)):
                    if mask >> j & 1:
                        out.append('%s=%s' % (nm, varname(int(v)) or '?%r' % v))
                    else:
                        out.append('%s=%s' % (nm, round(v, 5) if isinstance(v, float) else v))
                out.append('flags=%#x' % w[6])
            elif c == 'L':
                t, col = struct.unpack_from('<hh', args, 0); w = struct.unpack_from('<6f3i3I', args, 4)
                out.append('type=%d col=%d %s' % (t, col, ' '.join(str(round(x, 4)) if isinstance(x, float) else str(x) for x in w)))
            elif c == 'C':
                a, b, c2 = struct.unpack_from('<hhi', args, 0)
                txt = []
                for o, n, key in ((8, 0x30, 0xaa), (0x38, 0x30, 0xbb), (0x68, 0x40, 0xdd), (0xa8, 0x40, 0xee)):
                    txt.append(bytes(x ^ key for x in args[o:o + n]).split(b'\0')[0].decode('cp932', 'replace'))
                out.append('face=%d id=%d bonus=%d name=%r owner=%r c1=%r c2=%r' % (a, b, c2, *txt))
            p += SIZES[c]; k += 1
            continue
        raw = args[p:p + 4]; p += 4
        iv = struct.unpack('<i', raw)[0]; fv = struct.unpack('<f', raw)[0]
        isvar = bool(mask >> k & 1) or c in 'VW'
        if c in 'fW':
            vn = varname(int(fv)) if isvar and fv == int(fv) else None
            out.append(vn or (repr(round(fv, 6)) if not (c == 'W') else '?%r' % fv))
        elif c in 'iV':
            vn = varname(iv) if isvar else None
            out.append(vn or (str(iv) if c == 'i' else '?%d' % iv))
        elif c == 'b': out.append('bytes:' + raw.hex())
        elif c == 'X': out.append('%#x' % (iv & 0xffffffff))
        elif c == 'S': out.append('sub%d' % iv)
        elif c == 'T': out.append('t=%d' % iv)
        elif c == 'O': out.append('off=%+d' % iv)
        k += 1
    return ', '.join(out)


def argbytes(sig):
    return sum(SIZES.get(c, 4) for c in sig)


def main():
    a = sys.argv[1]; quiet = '-q' in sys.argv
    files = [os.path.join(a, f) for f in sorted(os.listdir(a)) if f.endswith('.ecl')] if os.path.isdir(a) else [a]
    used = collections.Counter(); tlused = collections.Counter(); problems = []
    for fn in files:
        d = open(fn, 'rb').read(); bn = os.path.basename(fn)
        magic, nsub, ntl = struct.unpack_from('<Ihh', d, 0)
        if magic != 0x800: problems.append('%s: magic %#x' % (bn, magic))
        tls = struct.unpack_from('<16I', d, 8)
        subs = struct.unpack_from('<%dI' % nsub, d, 0x48)
        bounds = sorted(set(list(subs) + [t for t in tls if t] + [len(d)]))
        for si, s in enumerate(subs):
            end = min(b for b in bounds if b > s)
            if not quiet: print('%s sub%d @%#x' % (bn, si, s))
            p = s; offs = set(); ins = []
            while p < end:
                t, op, sz, b8, rank, mask = struct.unpack_from('<ihhBBH', d, p)
                ins.append((p, t, op, sz, b8, rank, mask, d[p + 12:p + sz])); offs.add(p); p += sz
                if sz < 12: problems.append('%s sub%d: bad size' % (bn, si)); break
            if p != end: problems.append('%s sub%d: overrun' % (bn, si))
            if ins and ins[-1][2] != -1: problems.append('%s sub%d: no -1 terminator' % (bn, si))
            for (p, t, op, sz, b8, rank, mask, args) in ins:
                used[op] += 1
                if op == -1:
                    if not quiet: print('  %6d  end' % t)
                    continue
                if op not in OPS: problems.append('%s sub%d +%#x unknown op %d' % (bn, si, p - s, op)); continue
                name, sig = OPS[op]
                if argbytes(sig) != sz - 12: problems.append('%s sub%d +%#x op %d size %d != sig %s' % (bn, si, p - s, op, sz, sig))
                if 'O' in sig:
                    oi = sig.index('O'); rel = struct.unpack_from('<i', args, 4 * oi)[0]
                    if p + rel not in offs: problems.append('%s sub%d +%#x jump target off' % (bn, si, p - s))
                if 'S' in sig and op not in (129, 130):
                    si2 = sig.index('S'); sv = struct.unpack_from('<i', args, 4 * si2)[0]
                    if not (mask >> si2 & 1) and not (0 <= sv < nsub or (op in (133, 134, 135) and sv < 0)):
                        problems.append('%s sub%d +%#x op %d bad sub %d' % (bn, si, p - s, op, sv))
                if not quiet:
                    rk = '' if rank == 0xff else ' [rank %02x]' % rank
                    print('  %6d  +%04x %-18s %s%s%s' % (t, p - s, name, fmt(sig, args, mask), rk, '' if not b8 else ' (b8=%d)' % b8))
        for ti in range(16):
            if not tls[ti]: continue
            p = tls[ti]
            if not quiet: print('%s timeline%d @%#x%s' % (bn, ti, p, '' if ti < ntl else ' (beyond count, never run)'))
            while p + 8 <= len(d):
                t, op, sz, rank = struct.unpack_from('<ihBB', d, p)
                if t < 0:
                    if not quiet: print('  %6d  end' % t)
                    break
                tlused[op] += 1
                if op not in TL_OPS: problems.append('%s tl%d unknown op %d' % (bn, ti, op)); p += sz; continue
                name, sig = TL_OPS[op]
                if argbytes(sig) != sz - 8: problems.append('%s tl%d op %d size %d != %s' % (bn, ti, op, sz, sig))
                if not quiet: print('  %6d  %-22s %s [rank %02x]' % (t, name, fmt(sig, d[p + 8:p + sz], 0), rank))
                if sz == 0: break
                p += sz
    print('== ECL op coverage:', ' '.join('%d:%d' % kv for kv in sorted(used.items())))
    print('== timeline op coverage:', ' '.join('%d:%d' % kv for kv in sorted(tlused.items())))
    print('unknown ops:', [k for k in used if k not in OPS and k != -1], [k for k in tlused if k not in TL_OPS])
    print('problems:', len(problems))
    for p in problems[:60]: print('  ', p)


if __name__ == '__main__':
    main()
