#!/usr/bin/env python3
# usage: python3 -I extract_th08.py <th08.dat> <outdir>
# Removes every wrapper layer of th08.dat: container scramble+LZSS table, per-entry LZSS, per-file tag and body scramble.
import os
import struct
import sys

ARCHIVE_MAGIC = b'PBGZ'
HEADER_KEY = (0x1B, 0x37, 12, 0x400)
TABLE_KEY = (0x3E, 0x9B, 128, 0x400)
FILE_TAG_PREFIX = b'edz'
FILE_TAG_BYTES = (0x4D, 0x54, 0x41, 0x4A, 0x45, 0x57, 0x2D, 0x2A)
FILE_KEYS = {
    0: (0x1B, 0x37, 64, 10240),
    1: (0x51, 0xE9, 64, 12288),
    2: (0xC1, 0x51, 5120, 8192),
    3: (0x03, 0x19, 5120, 30720),
    4: (0xAB, 0xCD, 512, 4096),
    5: (0x12, 0x34, 1024, 10240),
    6: (0x35, 0x97, 128, 10240),
    7: (0x99, 0x37, 1024, 4096),
}


def descramble(data, key, step, block, limit):
    n = len(data)
    tail = n % block if n % block < (block >> 2) else 0
    tail += n & 1
    rem = n - tail
    out = bytearray(data)
    pos = 0
    idx = 0
    while rem > 0 and limit > 0:
        nb = block if rem >= block else rem
        k = key
        for j in range((nb + 1) // 2):
            out[pos + nb - 1 - 2 * j] = data[idx] ^ k
            k = (k + step) & 0xFF
            idx += 1
        for j in range(nb // 2):
            out[pos + nb - 2 - 2 * j] = data[idx] ^ k
            k = (k + step) & 0xFF
            idx += 1
        key = k
        rem -= nb
        pos += nb
        limit -= nb
    return bytes(out)


def lzss_decode(src, out_len):
    out = bytearray()
    win = bytearray(0x2000)
    wpos = 1
    p = 0
    n = len(src)
    buf = 0
    cnt = 0

    def bits(k):
        nonlocal p, buf, cnt
        v = 0
        for _ in range(k):
            if cnt == 0:
                buf = src[p] if p < n else 0
                p += 1
                cnt = 8
            cnt -= 1
            v = (v << 1) | ((buf >> cnt) & 1)
        return v

    while True:
        if bits(1):
            v = bits(8)
            out.append(v)
            win[wpos] = v
            wpos = (wpos + 1) & 0x1FFF
        else:
            off = bits(13)
            if off == 0:
                break
            for i in range(bits(4) + 3):
                v = win[(off + i) & 0x1FFF]
                out.append(v)
                win[wpos] = v
                wpos = (wpos + 1) & 0x1FFF
    if len(out) != out_len:
        raise ValueError(f'lzss produced {len(out)} bytes, expected {out_len}')
    return bytes(out)


def read_table(raw):
    if raw[:4] != ARCHIVE_MAGIC:
        raise ValueError('not a PBGZ archive')
    h0, h1, h2 = struct.unpack('<III', descramble(raw[4:16], *HEADER_KEY))
    count = h0 - 123456
    base = h1 - 345678
    table_len = h2 - 567891
    if count <= 0 or base <= 16 or base >= len(raw):
        raise ValueError('bad container header')
    table = lzss_decode(descramble(raw[base:], *TABLE_KEY), table_len)
    entries = []
    pos = 0
    for _ in range(count):
        end = table.index(b'\0', pos)
        name = table[pos:end].decode('ascii')
        start, size, flags = struct.unpack('<III', table[end + 1:end + 13])
        entries.append((name, start, size, flags))
        pos = end + 13
    starts = [e[1] for e in entries] + [base]
    for i, (name, start, size, flags) in enumerate(entries):
        if not (16 <= start < starts[i + 1] <= base):
            raise ValueError(f'bad range for {name}')
    return entries, starts


def decode_file(chunk, size):
    data = lzss_decode(chunk, size)
    if data[:3] != FILE_TAG_PREFIX:
        raise ValueError('missing file tag')
    tag = data[3]
    if tag not in FILE_TAG_BYTES:
        raise ValueError(f'unknown file tag {tag:#04x}')
    key, step, block, limit = FILE_KEYS[FILE_TAG_BYTES.index(tag)]
    return descramble(data[4:], key, step, block, limit)


def extract(dat_path, out_dir):
    with open(dat_path, 'rb') as f:
        raw = f.read()
    entries, starts = read_table(raw)
    os.makedirs(out_dir, exist_ok=True)
    for i, (name, start, size, flags) in enumerate(entries):
        if os.path.basename(name) != name or name in ('', '.', '..'):
            raise ValueError(f'unsafe name {name!r}')
        body = decode_file(raw[start:starts[i + 1]], size)
        with open(os.path.join(out_dir, name), 'wb') as f:
            f.write(body)
    return entries


def main(argv):
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    entries = extract(argv[1], argv[2])
    print(f'extracted {len(entries)} files to {argv[2]}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
