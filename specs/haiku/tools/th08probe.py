import struct
import sys


def scramble(data, key, step, block, limit):
    n = len(data)
    l2c = (n % block) if (n % block) < (block >> 2) else 0
    l2c += n & 1
    rem = n - l2c
    out = bytearray(data)
    pos = 0
    idx = 0
    while rem > 0 and limit > 0:
        nb = block if rem >= block else rem
        base = pos
        k = key
        for j in range((nb + 1) // 2):
            out[base + nb - 1 - 2 * j] = data[idx] ^ k
            k = (k + step) & 0xFF
            idx += 1
        for j in range(nb // 2):
            out[base + nb - 2 - 2 * j] = data[idx] ^ k
            k = (k + step) & 0xFF
            idx += 1
        key = k
        rem -= nb
        pos += nb
        limit -= nb
    return bytes(out)


def lzss(data, out_len):
    out = bytearray()
    win = bytearray(0x2000)
    wpos = 1
    bitpos = 0
    nbits = len(data) * 8

    def getbit():
        nonlocal bitpos
        if bitpos >= nbits:
            bitpos += 1
            return 0
        b = (data[bitpos >> 3] >> (7 - (bitpos & 7))) & 1
        bitpos += 1
        return b

    def getbits(n):
        v = 0
        for _ in range(n):
            v = (v << 1) | getbit()
        return v

    while True:
        if getbit():
            v = getbits(8)
            out.append(v)
            win[wpos] = v
            wpos = (wpos + 1) & 0x1FFF
        else:
            off = getbits(13)
            if off == 0:
                break
            ln = getbits(4) + 3
            for i in range(ln):
                v = win[(off + i) & 0x1FFF]
                out.append(v)
                win[wpos] = v
                wpos = (wpos + 1) & 0x1FFF
    return bytes(out)


def main(path):
    raw = open(path, "rb").read()
    assert raw[:4] == b"PBGZ", raw[:4]
    hdr = scramble(raw[4:16], 0x1B, 0x37, 12, 0x400)
    h0, h1, h2 = struct.unpack("<III", hdr)
    count = h0 - 123456
    base = h1 - 345678
    declen = h2 - 567891
    print("hdr raw", raw[4:16].hex(), "dec", hdr.hex())
    print("count", count, "base", base, "declen", declen, "filesize", len(raw))
    tbl = raw[base:]
    tbl = scramble(tbl, 0x3E, 0x9B, 128, 0x400)
    dec = lzss(tbl, declen)
    print("decompressed", len(dec), "expected", declen)
    print(dec[:256])
    return dec


if __name__ == "__main__":
    main(sys.argv[1])
