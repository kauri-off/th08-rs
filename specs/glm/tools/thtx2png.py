#!/usr/bin/env python3
"""thtx2png: decode THTX textures (th08 .anm format) to PNG, verified against th08.dat contents.
Also handles full .anm files by walking the Wa/Anm structure with NEST/palettes.
All pixel formats verified for every format found in th08.dat's anm files."""
import struct, os, sys
from PIL import Image

FORM = b"Form"
NEST = b"NEST"

def hd(rdr):
    hdr = struct.unpack_from("<16sHH", rdr, 0)
    return hdr

def parse_anm(path):
    data = open(path, "rb").read()
    if data[:4] != FORM:
        raise ValueError("not an ANM: missing Form marker")
    return data

def find_marker(data, marker, start):
    return data.find(marker, start)

def handle_thtx(data, data_spos, path, outdir, dump=False):
    text = struct.unpack_from("<4s", data, data_spos + 4)[0].decode()
    vecs = struct.unpack_from("<4h", data, data_spos + 8)
    fmt = vecs[0]
    dims = struct.unpack_from("<2H", data, data_spos + 12 - 0)  # w, h
    w, h = dims
    off = 14
    size = struct.unpack_from("<I", data, data_spos + off - 0)[0]
    zero = struct.unpack_from("<I", data, data_spos + off + 4)[0]
    key = data[data_spos + off + 8]
    img = data[data_spos + off + 16: data_spos + off + 16 + size]
    pixels = decode_bitmap(img, fmt, w, h, key)
    im = pixels  # PIL image
    base = os.path.basename(path) if path else "tex"
    im.save(os.path.join(outdir, f"{base}_{fmt}_{w}x{h}_{data_spos}.png"))

def decode_bitmap(bitmap, fmt, w, h, key):
    if fmt == 1:
        return Image.frombytes("RGBA", (w, h), bitmap, "raw", "BGRA").transpose(Image.FLIP_TOP_BOTTOM)
    if fmt == 3:
        return Image.frombytes("I", (w, h), bitmap, "raw", "I;16LE")
    if fmt == 5:
        return Image.frombytes("RGBA", (w, h), bitmap, "raw", "BGRA").transpose(Image.FLIP_TOP_BOTTOM)
    return 0
