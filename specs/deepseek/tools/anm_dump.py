#!/usr/bin/env python3
"""ANM/THTX structure dumper + THTX texture renderer for Touhou 8 (IN).

Usage:
  python3 anm_dump.py info  <file.anm> [more.anm ...]
  python3 anm_dump.py thtx  <chunk.anm> <chunk_index> <out.png> [--mode N]
  python3 anm_dump.py sprites <file.anm> [count]     (dump raw sprite records of chunk 0)

Pure stdlib. THTX is assumed 2 bytes/pixel RGBA4444-family; --mode selects
nibble interpretation so the correct one can be identified visually.
"""
import struct, sys, zlib

def u32(b,o): return struct.unpack_from('<I',b,o)[0]
def i32(b,o): return struct.unpack_from('<i',b,o)[0]
def f32(b,o): return struct.unpack_from('<f',b,o)[0]
def u16(b,o): return struct.unpack_from('<H',b,o)[0]

def chunks(d):
    off = 0
    while off + 0x3c <= len(d):
        nxt = u32(d, off+0x38)
        yield off
        if nxt == 0:
            break
        off += nxt

def chunk_info(d, off, ci):
    ne  = u32(d,off+0x00); ns = u32(d,off+0x04)
    c08 = u32(d,off+0x08)
    c0c = u32(d,off+0x0c); c10 = u32(d,off+0x10)
    c14 = u32(d,off+0x14); c18 = u32(d,off+0x18)
    nameoff = u32(d,off+0x1c)
    c20 = u32(d,off+0x20); c24 = u32(d,off+0x24)
    ver = u32(d,off+0x28); c2c = u32(d,off+0x2c)
    thtx = u32(d,off+0x30); c34 = u32(d,off+0x34)
    nxt = u32(d,off+0x38)
    print(f"chunk{ci} @0x{off:x}: n_spr={ne} n_scr={ns} +08={c08} +0c={c0c} +10={c10} "
          f"+14={c14} +18=0x{c18:08x} name_off=0x{nameoff:x} +20={c20} +24={c24} "
          f"ver={ver} +2c={c2c} thtx_off=0x{thtx:x} +34=0x{c34:08x} next=0x{nxt:x}")
    if nameoff and off+nameoff < len(d):
        s = d[off+nameoff:off+nameoff+0x40].split(b'\0')[0]
        print(f"   texture name: {s!r}")
    # hypothesised sprite offset table
    toff = off + 0x3c
    so = [u32(d,toff+4*k) for k in range(ne)]
    nz = [(k,v) for k,v in enumerate(so) if v]
    mono = all(nz[i][1] < nz[i+1][1] for i in range(len(nz)-1))
    print(f"   spr_offsets: n={ne} first4={so[:4]} last2={so[-2:]} nonzero={len(nz)} monotonic={mono} null0={so[0]==0 if so else '?'}")
    if nz:
        steps = sorted({nz[i+1][1]-nz[i][1] for i in range(len(nz)-1)})
        print(f"   first spr @0x{nz[0][1]:x}, steps(nonzero)={steps[:5]}")
    # script table right after
    soff = toff + 4*ne
    print(f"   script table @0x{soff:x}:")
    for k in range(min(ns, 8)):
        sp = u32(d, soff+8*k); sid = i32(d, soff+8*k+4)
        print(f"      scr[{k}] off=0x{sp:x} id={sid}")
    if ns > 8:
        sp = u32(d, soff+8*(ns-1)); sid = i32(d, soff+8*(ns-1)+4)
        print(f"      ... last scr[{ns-1}] off=0x{sp:x} id={sid}")
    # THTX
    if thtx:
        t = off + thtx
        print(f"   thtx @0x{t:x}: raw24={d[t:t+24].hex()}")
        if d[t:t+4] == b'THTX':
            hh = [u16(d,t+4+2*k) for k in range(8)]
            print(f"     u16 hdr: {hh}")
            w0,h0 = hh[2], hh[3]
            ce = (off+nxt) if nxt else len(d)
            rem = ce - t - 16
            print(f"     w={w0} h={h0} hdr=16 chunk_bytes_left={rem} w*h*2={w0*h0*2} match2={rem==w0*h0*2}")
    return ne, ns, nz

def info(path):
    d = open(path,'rb').read()
    print(f"== {path}  ({len(d)} bytes)")
    for ci, off in enumerate(chunks(d)):
        chunk_info(d, off, ci)

def thtx_pixels(d, off, ci, mode):
    off_ci = list(chunks(d))[ci]
    t = off_ci + u32(d, off_ci+0x30)
    assert d[t:t+4] == b'THTX', "no THTX"
    w = u16(d, t+8); h = u16(d, t+10)
    px = d[t+16 : t+16+w*h*2]
    assert len(px) == w*h*2, "short pixel data"
    out = bytearray(w*h*4)
    for i in range(w*h):
        v = px[2*i] | (px[2*i+1] << 8)
        if mode == 0:    # RGBA4444, R=high nibble, A=low nibble
            rr=((v>>12)&15)*17; gg=((v>>8)&15)*17; bb=((v>>4)&15)*17; aa=(v&15)*17
        elif mode == 1:  # ARGB4444, A=high nibble
            aa=((v>>12)&15)*17; rr=((v>>8)&15)*17; gg=((v>>4)&15)*17; bb=(v&15)*17
        elif mode == 2:  # RGB4444 (alpha forced opaque)
            rr=((v>>12)&15)*17; gg=((v>>8)&15)*17; bb=((v>>4)&15)*17; aa=255
        elif mode == 3:  # A1R5G5B5 (alpha = bit15)
            aa = 255 if (v & 0x8000) else 0
            rr = ((v>>10)&31)*255//31; gg = ((v>>5)&31)*255//31; bb = (v&31)*255//31
        else:            # X1R5G5B5 (alpha = bit0)
            aa = 255 if (v & 1) else 0
            rr = ((v>>11)&31)*255//31; gg = ((v>>6)&31)*255//31; bb = ((v>>1)&31)*255//31
        out[4*i:4*i+4] = bytes((rr,gg,bb,aa))
    return w, h, bytes(out)

def write_png(path, w, h, rgba):
    def chunk(t, data):
        return struct.pack('>I', len(data)) + t + data + struct.pack('>I', zlib.crc32(t+data) & 0xffffffff)
    raw = b''.join(b'\x00' + rgba[y*w*4:(y+1)*w*4] for y in range(h))
    png = b'\x89PNG\r\n\x1a\n'
    png += chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(raw, 6))
    png += chunk(b'IEND', b'')
    open(path,'wb').write(png)

def sprites(path, count=6):
    d = open(path,'rb').read()
    off = 0
    ne = u32(d,off); 
    toff = off+0x3c
    print(f"sprite table of {path}: n={ne}")
    seen = 0
    for k in range(ne):
        so = u32(d,toff+4*k)
        if so and seen < count:
            base = off+so
            print(f" spr[{k}] @0x{base:x}: u32x5={[i32(d,base+4*i) for i in range(5)]}")
            print(f"           f32x5={[round(f32(d,base+4*i),3) for i in range(5)]}")
            seen += 1

if __name__ == '__main__':
    cmd = sys.argv[1]
    if cmd == 'info':
        for p in sys.argv[2:]: info(p)
    elif cmd == 'thtx':
        p, ci, out = sys.argv[2], int(sys.argv[3]), sys.argv[4]
        mode = int(sys.argv[sys.argv.index('--mode')+1]) if '--mode' in sys.argv else 0
        d = open(p,'rb').read()
        w,h,px = thtx_pixels(d, 0, ci, mode)
        write_png(out, w, h, px)
        print(f"wrote {out} {w}x{h} mode={mode}")
    elif cmd == 'sprites':
        cnt = int(sys.argv[3]) if len(sys.argv) > 3 else 6
        sprites(sys.argv[2], cnt)
    elif cmd == 'hist':
        p, ci = sys.argv[2], int(sys.argv[3])
        d = open(p,'rb').read()
        off_ci = list(chunks(d))[ci]
        t = off_ci + u32(d, off_ci+0x30)
        w = u16(d, t+8); h = u16(d, t+10)
        from collections import Counter
        c = Counter()
        for i in range(w*h):
            c[d[t+16+2*i] | (d[t+17+2*i] << 8)] += 1
        print(f"{p} chunk{ci} THTX fmt={u16(d,t+6)} {w}x{h}; top 12 pixel values (of {w*h}):")
        for v, n in c.most_common(12):
            print(f"   0x{v:04x}: {n}")
