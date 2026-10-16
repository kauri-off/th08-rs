# Formats as implemented (source → specs/<agent>/...)

## th08.dat (PBGZ) — source: deepseek NOTES.md; verified by byte-identical extraction
- "PBGZ", then 12 bytes ciphered (key 0x1b, step 0x37, block 12, limit 0x400) → 3 u32 minus 123456 / 345678 / 567891
  = count / table offset / table decompressed size.
- Table: from table offset to EOF, cipher (0x3e,0x9b,0x80,0x400) → LZSS → records {cstr name, u32 off, u32 size, u32 0}.
  Entry extent = next offset (last → table offset). Entry data = LZSS (no cipher) → "edz"+type char + ciphered payload.
- Cipher: per block (size b = min(block, left)): first ceil(b/2) src bytes go to dest b-1, b-3, ...; next floor(b/2) to b-2, b-4, ...;
  XOR with rolling key (+= step per byte). Raw tail = (len%block < block/4 ? len%block : 0) + (len&1). Stops when limit budget used.
- LZSS: MSB-first bits, flag 1 = literal byte, 0 = 13-bit ring pos (0 = end) + 4-bit len (+3); 8 KiB ring, write pos starts at 1.
- edz keys: M 1b/37/40/2800, T 51/e9/40/3000, A c1/51/1400/2000, J 03/19/1400/7800, E ab/cd/200/1000, W 12/34/400/2800, - 35/97/80/2800, * 99/37/400/1000.

## thbgm.dat / thbgm.fmt — source: deepseek NOTES.md
- fmt: 52-byte entries {name[16], +0x10 offset, +0x14 preload size, +0x18 loop start? (bytes), +0x1C length (bytes), +0x20 WAVEFORMATEX}.
- PCM 44.1 kHz stereo s16. Track for title = th08_01.wav.
