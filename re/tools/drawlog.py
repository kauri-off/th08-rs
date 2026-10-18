#!/usr/bin/env python3
"""Draw-log tool (format: specs/data/drawlog/FORMAT.md).
  drawlog.py summary <log[.xz]>                         (label, uv rect) -> first frame, count
  drawlog.py render  <log[.xz]> <present> <out.png> [--from PRESENT] [--fog] [--noclip] [--tex DIR] [--img DIR]
      (3D quads are clipped to 0 <= z <= 1 in homogeneous space; --fog = stage fog as FORMAT.md §2:
       eye depth interpolated perspective-correctly, factor clamped per pixel)
      (--from: draw all frames from PRESENT on into the same buffer; the original never clears outside
       the playfield, see FORMAT.md "persistence")
  drawlog.py diff    <orig[.xz]> <port[.xz]> [--key present|scene|demo] [--tol 0.6] [--max 20] [--playfield]
      (frames matched by present / (scene, n-th visit of that scene, scene_frame) / (demo, demo_frame); --playfield = only primitives
       touching the playfield 32,16..416,464, for ports that repaint the side panel every frame)
  drawlog.py split   <probe_draw.txt> <outdir> [--chunk 1500]   per-demo xz chunks (reference logs)
  drawlog.py frames  <log[.xz]> <present> [<present2>]  print raw records of frames
"""
import lzma, os, sys
from collections import OrderedDict


def open_log(path):
    return lzma.open(path, 'rt') if path.endswith('.xz') else open(path)


def log_lines(path):
    import glob
    paths = sorted(glob.glob(path)) if any(c in path for c in '*?[') else [path]
    for p in paths:
        yield from open_log(p)


def frames(path):
    """yield (header dict, list of record lines); path may be a glob of chunk files"""
    hdr, recs = None, []
    for line in log_lines(path):
        line = line.rstrip('\n')
        if line.startswith('F '):
            if hdr is not None:
                yield hdr, recs
            p = line.split()
            hdr = dict(present=int(p[1]), scene=int(p[2]), sf=int(p[3]), gm=int(p[4]), demo=int(p[5]),
                       df=int(p[6]), input=int(p[7], 16), nquads=int(p[8]))
            recs = []
        elif line and hdr is not None:
            recs.append(line)
    if hdr is not None:
        yield hdr, recs


def parse_vertex(t):
    x, y, z, w = map(float, t[:4])
    u = None if t[4] == '-' else float(t[4])
    v = None if t[5] == '-' else float(t[5])
    return (x, y, z, w, u, v, int(t[6], 16))


def prims(recs):
    """yield (state dict, kind, vertices) with effective T/S/V state; kind in quad/strip/fan/list/..."""
    st = dict(T='-', tw=0, th=0, S={}, V=(0, 0, 640, 480, 0.0, 1.0))
    for r in recs:
        c, rest = r[0], r[2:]
        if c == 'T':
            p = rest.split()
            st['T'] = p[0]
            st['tw'], st['th'] = (int(p[1]), int(p[2])) if len(p) >= 3 else (0, 0)
        elif c == 'S':
            st['S'] = dict(kv.split('=', 1) for kv in rest.split())
        elif c == 'V':
            p = rest.split()
            st['V'] = tuple(int(x) for x in p[:4]) + ((float(p[4]), float(p[5])) if len(p) > 4 else (0.0, 1.0))
        elif c == 'R':
            p = rest.split()
            x0, y0, x1, y1, u0, v0, u1, v1 = map(float, p[:8])
            col = int(p[8], 16)
            z = float(p[9]) if len(p) > 9 else 0.0
            vs = [(x0, y0, z, 1.0, u0, v0, col), (x1, y0, z, 1.0, u1, v0, col),
                  (x0, y1, z, 1.0, u0, v1, col), (x1, y1, z, 1.0, u1, v1, col)]
            yield st, 'quad', vs, r
        elif c == 'Q':
            p = rest.split()
            yield st, 'quad', [parse_vertex(p[i * 7:i * 7 + 7]) for i in range(4)], r
        elif c == 'P':
            p = rest.split()
            kind, n = p[0], int(p[1])
            yield st, kind, [parse_vertex(p[2 + i * 7:9 + i * 7]) for i in range(n)], r
        elif c in 'KC':
            yield st, c, rest.split(), r


def cmd_summary(path):
    seen = OrderedDict()
    for h, recs in frames(path):
        for st, kind, vs, _ in prims(recs):
            if kind in ('K', 'C'):
                if kind == 'K':
                    key = ('K:' + vs[0] + '->' + vs[1], tuple(vs[2:6]))
                else:
                    continue
            else:
                us = [v[4] for v in vs if v[4] is not None]
                ws = [v[5] for v in vs if v[5] is not None]
                if not us:
                    key = (st['T'], ())
                else:
                    tw, th = st['tw'] or 1, st['th'] or 1
                    key = (st['T'], (round(min(us) * tw, 1), round(min(ws) * th, 1),
                                     round(max(us) * tw, 1), round(max(ws) * th, 1)))
            if key in seen:
                seen[key][1] += 1
            else:
                seen[key] = [h['present'], 1]
    print('# label  x0 y0 x1 y1 (texels of the logical texture size)  first_present  count')
    for (lab, rect), (first, n) in seen.items():
        print(lab, ' '.join(str(x) for x in rect) if rect else '-', first, n)


# ---------------------------------------------------------------- render
FOG = False
CLIP = True


def stage_depth(z):
    """eye-space depth from the projected z of the stage projection (near 30, far 1800; 06 §10.1).
    More precise than 1/rhw, which the log rounds to 4 decimals."""
    den = 1800 - 1770 * z
    return 54000 / den if den != 0 else 1e9


def cmd_render(path, present, out, texdir, imgdir, start=None):
    import numpy as np
    from PIL import Image
    W, H = 640, 480
    fb = np.zeros((H, W, 3), np.float32)
    zb = np.ones((H, W), np.float32)
    cache = {}

    def tex(label):
        if label in cache:
            return cache[label]
        a = None
        if '#' in label:
            f, e = label.split('#')
            p = os.path.join(texdir, '%s_%s.png' % (f, e))
            if os.path.exists(p):
                a = np.asarray(Image.open(p).convert('RGBA'), np.float32) / 255
        else:
            for d in (imgdir,):
                p = os.path.join(d, label)
                if os.path.exists(p):
                    a = np.asarray(Image.open(p).convert('RGBA'), np.float32) / 255
        cache[label] = a
        return a

    def sample(t, u, v):
        th, tw = t.shape[:2]
        x = u * tw - 0.5
        y = v * th - 0.5
        x0 = np.floor(x).astype(int)
        y0 = np.floor(y).astype(int)
        fx = (x - x0)[:, None]
        fy = (y - y0)[:, None]
        x0m, x1m = x0 % tw, (x0 + 1) % tw
        y0m, y1m = y0 % th, (y0 + 1) % th
        return (t[y0m, x0m] * (1 - fx) * (1 - fy) + t[y0m, x1m] * fx * (1 - fy) +
                t[y1m, x0m] * (1 - fx) * fy + t[y1m, x1m] * fx * fy)

    def col4(c):
        return np.array([(c >> 16) & 255, (c >> 8) & 255, c & 255, c >> 24], np.float32) / 255

    def fogf(st, v):
        fc, fs, fe = st['S']['fog'].split(',')
        fs, fe = float(fs), float(fe)
        return (fe - stage_depth(v[2])) / (fe - fs)

    def lerpv(p, q, t):
        w0, w1 = 1 / p[3], 1 / q[3]
        X = [(1 - t) * p[i] * w0 + t * q[i] * w1 for i in range(3)]
        w = (1 - t) * w0 + t * w1
        out = [X[0] / w, X[1] / w, X[2] / w, 1 / w]
        for i in (4, 5):
            out.append(None if p[i] is None else (1 - t) * p[i] + t * q[i])
        c = 0
        for sh in (24, 16, 8, 0):
            c |= int(round((1 - t) * ((p[6] >> sh) & 255) + t * ((q[6] >> sh) & 255))) << sh
        out.append(c)
        out.append((1 - t) * p[7] + t * q[7])
        return out

    def tri(st, a, b, c):
        fog = 'fog' in st['S'] and FOG
        vs = [list(v[:7]) + [fogf(st, v) if fog else 1.0] for v in (a, b, c)]
        if CLIP and any(v[3] != 1 and v[3] != 0 for v in vs):
            # clip 0 <= z <= w in homogeneous space; fogged quads are stage quads: w from z
            if fog:
                for v in vs:
                    v[3] = 1 / stage_depth(v[2])
            for side in (0, 1):
                out = []
                for i in range(len(vs)):
                    p, q = vs[i], vs[(i + 1) % len(vs)]
                    dp = (p[2] if side == 0 else 1 - p[2]) / p[3]
                    dq = (q[2] if side == 0 else 1 - q[2]) / q[3]
                    if dp >= 0:
                        out.append(p)
                    if (dp >= 0) != (dq >= 0):
                        out.append(lerpv(p, q, dp / (dp - dq)))
                vs = out
                if len(vs) < 3:
                    return
        for i in range(1, len(vs) - 1):
            raster(st, vs[0], vs[i], vs[i + 1], fog)

    def raster(st, a, b, c, fog):
        vx0, vy0, vw, vh = st['V'][:4]
        xs = [a[0], b[0], c[0]]
        ys = [a[1], b[1], c[1]]
        x0 = max(int(np.ceil(min(xs))), vx0)
        x1 = min(int(np.floor(max(xs))), vx0 + vw - 1)
        y0 = max(int(np.ceil(min(ys))), vy0)
        y1 = min(int(np.floor(max(ys))), vy0 + vh - 1)
        if x1 < x0 or y1 < y0:
            return
        gy, gx = np.mgrid[y0:y1 + 1, x0:x1 + 1]
        px = gx.ravel().astype(np.float64)
        py = gy.ravel().astype(np.float64)
        d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
        if abs(d) < 1e-12:
            return
        l0 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / d
        l1 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / d
        l2 = 1 - l0 - l1
        eps = -1e-7
        m = (l0 >= eps) & (l1 >= eps) & (l2 >= eps)
        if not m.any():
            return
        l0, l1, l2, px, py = l0[m], l1[m], l2[m], px[m].astype(int), py[m].astype(int)
        w0, w1, w2 = a[3], b[3], c[3]
        q = l0 * w0 + l1 * w1 + l2 * w2
        q = np.where(q == 0, 1, q)
        z = l0 * a[2] + l1 * b[2] + l2 * c[2]
        S = st['S']
        zt = S.get('ztest', 'off')
        if zt not in ('off', 'always'):
            cur = zb[py, px]
            ok = {'le': z <= cur, 'lt': z < cur, 'ge': z >= cur, 'gt': z > cur, 'eq': z == cur}.get(zt, np.ones_like(z, bool))
            l0, l1, l2, px, py, z, q = l0[ok], l1[ok], l2[ok], px[ok], py[ok], z[ok], q[ok]
        if not len(px):
            return
        cols = np.stack([col4(a[6]), col4(b[6]), col4(c[6])])
        lam = np.stack([l0, l1, l2], 1)
        col = lam @ cols
        t = tex(st['T']) if st['T'] != '-' else None
        if t is not None and a[4] is not None:
            u = (l0 * a[4] * w0 + l1 * b[4] * w1 + l2 * c[4] * w2) / q
            v = (l0 * a[5] * w0 + l1 * b[5] * w1 + l2 * c[5] * w2) / q
            tx = sample(t, u, v)
        else:
            tx = np.ones((len(px), 4), np.float32)
        cm, am = S.get('c', 'tex*col'), S.get('a', 'tex*col')
        rgb = {'tex': tx[:, :3], 'col': col[:, :3]}.get(cm, tx[:, :3] * col[:, :3])
        al = {'tex': tx[:, 3], 'col': col[:, 3]}.get(am, tx[:, 3] * col[:, 3])
        at = S.get('atest', 'off')
        ok = None
        if at.startswith('ge'):
            ok = al * 255 >= int(at[2:]) - 0.5
            rgb, al, px, py, z = rgb[ok], al[ok], px[ok], py[ok], z[ok]
        if fog:
            fv = [a[7], b[7], c[7]]
            f = np.clip((l0 * fv[0] * w0 + l1 * fv[1] * w1 + l2 * fv[2] * w2) / q, 0, 1)
            if at.startswith('ge'):
                f = f[ok]
            rgb = rgb * f[:, None] + col4(int(S['fog'].split(',')[0], 16))[:3] * (1 - f[:, None])
        bl = S.get('blend', 'alpha')
        dst = fb[py, px]
        if bl == 'alpha':
            fb[py, px] = rgb * al[:, None] + dst * (1 - al[:, None])
        elif bl == 'add':
            fb[py, px] = np.minimum(1, dst + rgb * al[:, None])
        else:
            fb[py, px] = rgb
        if S.get('zwrite') == '1' and zt != 'off':
            zb[py, px] = z

    for h, recs in frames(path):
        if h['present'] > present:
            break
        if h['present'] < (present if start is None else start):
            continue
        for st, kind, vs, raw in prims(recs):
            if kind == 'C':
                c = int(vs[1], 16)
                vx0, vy0, vw, vh = st['V'][:4]
                if 't' in vs[0]:
                    fb[vy0:vy0 + vh, vx0:vx0 + vw] = col4(c)[:3]
                if 'z' in vs[0]:
                    zb[vy0:vy0 + vh, vx0:vx0 + vw] = 1
            elif kind == 'K':
                if vs[0] == 'screen':
                    cache[vs[1]] = np.concatenate([fb.copy(), np.ones((H, W, 1), np.float32)], 2)
                    continue
                if vs[1] != 'screen' or vs[2] == 'full':
                    continue
                t = tex(vs[0])
                if t is None:
                    continue
                sx, sy, w, hh, dx, dy = map(int, vs[2:8])
                fb[dy:dy + hh, dx:dx + w] = t[sy:sy + hh, sx:sx + w, :3]
            elif kind == 'quad':
                tri(st, vs[0], vs[1], vs[2])
                tri(st, vs[1], vs[2], vs[3])
            elif kind == 'strip':
                for i in range(len(vs) - 2):
                    tri(st, vs[i], vs[i + 1], vs[i + 2])
            elif kind == 'fan':
                for i in range(1, len(vs) - 1):
                    tri(st, vs[0], vs[i], vs[i + 1])
            elif kind == 'list':
                for i in range(0, len(vs) - 2, 3):
                    tri(st, vs[i], vs[i + 1], vs[i + 2])
        if h['present'] != present:
            continue
        Image.fromarray((np.clip(fb, 0, 1) * 255 + 0.5).astype(np.uint8)).save(out)
        print('rendered', out)
        return
    print('frame not found')


# ---------------------------------------------------------------- diff
def is_fps(p):
    """the "60.00fps" counter (ascii.anm, bottom right) shows measured real time: never comparable"""
    st, kind, vs, _ = p
    return (kind == 'quad' and st['T'].startswith('ascii.anm') and min(v[0] for v in vs) >= 500 and
            min(v[1] for v in vs) >= 460)


def in_pf(p):
    st, kind, vs, _ = p
    if kind in ('K', 'C'):
        return tuple(st['V'][:4]) == (32, 16, 384, 448)
    xs = [v[0] for v in vs]
    ys = [v[1] for v in vs]
    return max(xs) > 32 and min(xs) < 416 and max(ys) > 16 and min(ys) < 464


def cmd_diff(a, b, key, tol, maxrep, playfield=False):
    def keyed(path):
        visits, prev = {}, None
        for h, recs in frames(path):
            if key == 'demo':
                k = (h['demo'], h['df']) if h['demo'] >= 0 else None
            elif key == 'scene':
                if prev is None or h['scene'] != prev or h['sf'] == 1:
                    visits[h['scene']] = visits.get(h['scene'], 0) + 1
                prev = h['scene']
                k = (h['scene'], visits[h['scene']], h['sf'])
            else:
                k = h['present']
            yield k, h, recs

    B = {}
    for k, h, recs in keyed(b):
        if k is not None and k not in B:
            B[k] = recs
    nfr = nbad = 0
    for k, h, recs in keyed(a):
        if k is None or k not in B:
            continue
        nfr += 1
        pa = list(prims(recs))
        pb = list(prims(B[k]))
        if playfield:
            pa, pb = [x for x in pa if in_pf(x)], [x for x in pb if in_pf(x)]
        pa, pb = [x for x in pa if not is_fps(x)], [x for x in pb if not is_fps(x)]
        bad = None
        if len(pa) != len(pb):
            bad = 'record count %d vs %d' % (len(pa), len(pb))
        for i, (x, y) in enumerate(zip(pa, pb)):
            if bad:
                break
            sx, kx, vx, rx = x
            sy, ky, vy, ry = y
            if kx != ky or sx['T'] != sy['T'] or sx['S'] != sy['S'] or sx['V'] != sy['V']:
                bad = 'record %d: state/kind differs\n  orig: T=%s S=%s V=%s %s\n  port: T=%s S=%s V=%s %s' % (
                    i, sx['T'], sx['S'], sx['V'], rx, sy['T'], sy['S'], sy['V'], ry)
            elif kx not in ('K', 'C'):
                if len(vx) != len(vy):
                    bad = 'record %d: vertex count' % i
                else:
                    for p, q in zip(vx, vy):
                        if (max(abs(p[0] - q[0]), abs(p[1] - q[1])) > tol or
                                (p[4] is None) != (q[4] is None) or
                                (p[4] is not None and max(abs(p[4] - q[4]), abs(p[5] - q[5])) > 1e-3) or
                                any(abs(((p[6] >> s) & 255) - ((q[6] >> s) & 255)) > 2 for s in (0, 8, 16, 24))):
                            bad = 'record %d differs\n  orig: %s\n  port: %s' % (i, rx, ry)
                            break
            elif vx != vy:
                bad = 'record %d differs\n  orig: %s\n  port: %s' % (i, rx, ry)
        if bad:
            nbad += 1
            if nbad <= maxrep:
                print('frame %s: %s' % (k, bad))
    print('compared %d frames, %d differ' % (nfr, nbad))


def cmd_frames(path, p0, p1):
    for h, recs in frames(path):
        if p0 <= h['present'] <= p1:
            print('F', h)
            print('\n'.join(recs))


def cmd_split(path, outdir, chunk):
    """split a probe log into demo<K>_<first>-<last>.drawlog.xz chunks of <chunk> demo frames
    (frames before the first demo_frame > 0 = loading, go into the first chunk)"""
    os.makedirs(outdir, exist_ok=True)
    cur, key, lines, first, last = None, None, [], None, None

    def flush():
        if cur is None or not lines:
            return
        fn = os.path.join(outdir, 'demo%d_%05d-%05d.drawlog.xz' % (cur, first, last))
        with lzma.open(fn, 'wt', preset=9 | lzma.PRESET_EXTREME) as f:
            f.write(''.join(lines))
        print(fn, os.path.getsize(fn))

    for h, recs in frames(path):
        if h['demo'] < 0:
            continue
        k = (h['demo'], max(h['df'] - 1, 0) // chunk)
        if k != key:
            if not (key and key[0] == k[0] and h['df'] == 0):
                flush()
                cur, key, lines, first = h['demo'], k, [], h['df']
        last = h['df']
        lines.append('F %d %d %d %d %d %d %04x %d\n' % (h['present'], h['scene'], h['sf'], h['gm'], h['demo'],
                                                       h['df'], h['input'], h['nquads']))
        lines.extend(r + '\n' for r in recs)
    flush()


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return
    opt = lambda n, d: argv[argv.index(n) + 1] if n in argv else d
    here = os.path.dirname(os.path.abspath(__file__))
    if argv[1] == 'summary':
        cmd_summary(argv[2])
    elif argv[1] == 'render':
        global FOG, CLIP
        FOG = '--fog' in argv
        CLIP = '--noclip' not in argv
        cmd_render(argv[2], int(argv[3]), argv[4], opt('--tex', os.path.join(here, '../work/tex')),
                   opt('--img', os.path.join(here, '../extracted/dat')),
                   int(opt('--from', '0')) if '--from' in argv else None)
    elif argv[1] == 'diff':
        cmd_diff(argv[2], argv[3], opt('--key', 'present'), float(opt('--tol', '0.6')), int(opt('--max', '20')),
                 '--playfield' in argv)
    elif argv[1] == 'split':
        cmd_split(argv[2], argv[3], int(opt('--chunk', '1500')))
    elif argv[1] == 'frames':
        cmd_frames(argv[2], int(argv[3]), int(argv[4]) if len(argv) > 4 else int(argv[3]))


if __name__ == '__main__':
    main(sys.argv)
