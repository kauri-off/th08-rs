# NOTES.md —glm agent, Phase 1 (th08 reverse engineering)

## Resume checklist
- Re-read this file, symbols.csv, structs.md after compaction.
- All spec files go in /home/anon/Projects/th_08/specs/glm/. Do NOT write elsewhere.
- Game dir: /home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/

## Status
- [x] th08.exe opened in REA (Hopper). Tool catalog: search in `execute` code.
- [x] LZSS decompressor found: sub_4740e0 (VA). See below.
- [x] Archive ctor: sub_474ce0. Lookup: sub_474c70 (16-byte entries at obj[0], count at obj[1]).
- [x] Read-file-from-archive: sub_43e660. Log: sub_43eb60. Entry decode: sub_474af0.
- [x] Table parser: sub_474fa0 (strdup 0x475150, skip name 0x475220, advance u32 0x475200, read u32 0x4751e0).
- [x] Archive header decryptor: sub_43e1d0(key, delta, blocklen, guard) with (ecx=src, edx=len).
- [ ] **BLOCKED on exact header semantics + verify extractor** — see Open questions.
- Wrote prototype extractor specs/glm/tools/thdat_probe.py — needs verification against real th08.dat.

## What is verified (from assembly, Hopper + raw objdump)
- th08.dat starts with "PBGZ" magic (checked real file).
- ctor sub_474ce0 flow: open stream by filename; read 4 bytes => must equal 0x5a474250 ("PBGZ" LE);
  then read 12 bytes into var_24; sub_43e1d0(0x1b,0x37,block=0xc,guard=0x400) with ecx=&var_24, edx=0xc
  = de-obfuscate 3 u32s; then: obj[1] = val0 - 0x1e240 (=123456);
  val1 = var + 0x5464e skipped
  val1 = var - 0x5464e (=345678)      ; var_14
  val2 = var - 0x8aa53 (=567891)      ; var_8
  - file size from vtable+0x14; seek to var_14 (vtable+0x18); read (filesize-var_14) bytes;
    decrypt with (0x3e,0x9b,block=0x80,guard=0x400), ecx=cheap buffer, edx=remaining length;
    LZSS-decompress (input limit = val2); parse table.
- sub_43e1d0 (disassembled with objdump, fully understood):
  - args: key at ebp+8, delta ebp+0xc, blocklen ebp+0x10, guard ebp+0x14; ecx=src ptr, edx=length
  - allocates dest (len bytes, via sub_40b880 with global buffer tag 0x164f598 and name str 0x4b5a18 "d:\cygwin\home\zun\prog\th08\global.h")
  - destination per block: dest[d+blocklen-1-2i] = src[s+i]^key for i<ceil(blocklen/2),  (pass A)
                            dest[d+blocklen-2-2i] = src[s+ceil+i]^key for i<floor(blocklen/2) (pass B)
    key = (key + delta) & 0xff after EACH written byte; src consumed in order.
  - if guard budget exhausted: remaining bytes memcpy VERBATIM (no xor).
  - returns allocated dest.
- Entry table (sub_474fa0, ASM confirmed):
  - alloc (count+1)*16 + 4 bytes; [0]=count+1; elements inited (ctor sub_4716e0) via 0x4a6035 loop.
  - per record i<count: name=strdup(cursor); name_skip(cursor); entry[i].+4=u32(cursor); advance;
    entry[i].+8=u32(cursor); advance; entry[i].+12=u32(cursor); advance.
  - sentinel entry at index count: .+4 = arg_8 (table offset...), .+8=0.
  - obj[0] = entries array base (returned = alloc+4); obj[1] = count.
- Lookup sub_474c70: iterates obj[0] with stride 0x10, count obj[1], strcmp-style compare sub_4b1f5f (case-insensitive compare via CRT style helper).
- Read-entry sub_474af0: obj[0x14]-obj[4] span check; opens stream (obj[0xc]) mode 'r'; seek(entry+4, 0 span...);
  actual: entry+0x14 minus entry+4 found as size;
  raw = GlobalAlloc(0, span); stream read → decrypt via vtable; then sub_4740e0(out, limit) = LZSS decompress.
- LZSS sub_4740e0 (fully disassembled, HIGH confidence):
  - this=ecx=input, edx=limit (input bytes), arg0(out buffer)=ebp+8, arg4(size, for alloc if out==0)=ebp+0xc
  - ring buffer at EA 0x18d5f68, size 0x2000 (8192), zero-initialized (BSS), write pos starts at 1, mask 0x1fff
  - bit stream: mask starts 0x80; when mask==0x80 fetch new control byte from input (respects limit, treats beyond-limit as 0 and stops advancing);
    bit = flag & mask; mask >>= 1; if mask becomes 0 -> mask = 0x80.
  - flag bit set => literal: read 8 bits MSB-first, append to output; ring[pos]=byte; pos++, &0x1fff
  - flag bit clear => match: read 13 bits MSB-first => position P; if P==0 -> end of stream;
    read 4 bits MSB-first => length L; copy (L+3) bytes from ring[(P+k)&0x1fff] forward (k=0..L+2), each copied byte appended to output and stored to ring[pos++&0x1fff].
  - INPUT limit (edx): stops consuming past limit (bytes read as 0).
- Stream class: vtable at 0x4b98b4 = [open(+0) 0x473790, close_?(+4) 0x4738e0, read(+8) 0x473920, (+c) 0x473970, (+10) 0x4739c0, size/tell(+14) 0x473a20, seek(+18) 0x473a00, close(+1c) 0x473a60].
  read = plain ReadFile (no crypto on raw data). 'r' mode 0x80000000/OPEN_EXISTING/FILE_ATTRIBUTE_... etc.

## Open questions (BLOCKING extractor)
1. Header: val0 - 123456 should be the entry count but we computed 11862389 (not sane). Are the 12 plaintext bytes fmt "III"? Could be (val0,val1,val2) = (count*, table offset*, compressed size*) with DIFFERENT obfuscation additive constants. The three downward subtractions in asm are 0x1e240, 0x5464e, 0x8aa53 (123456, 345678, 567891) and those are the stored VA constants. Suspect the raw u32s themselves are obfuscated further (XOR with a key from version?). RECHECK sub_4751e0? OR the order in which var_24..var_1C map to (count, offset, size) might be different — verify with real file once exact math appears sane.
2. My extractor produced sparse/garbage table ⇒ plaintext decode may be wrong, OR table_offset candidate wrong, OR LZSS "out" buffer handling wrong.
3. Need to double check sub_4751e0 ("plain read 4") vs the u32 items in table parse — actually 474fa0 uses sub_475200-style advance + direct deref of cursor; seems right.

## Next steps
1. Switch to Ghidra (pre-analyzed project exists in <SPEC_DIR>/ghidra/th08.gpr). Use helper script /home/anon/Projects/th_08/specs/ghidra_tools/gh.sh. SERIAL calls only, ~5s each.
2. Cross-check sub_474ce0/sub_474fa0/sub_4740e0/sub_43e1d0/sub_4751e0 semantics in Ghidra.
3. Look for WAL: read file "ver" (th08_0100m.ver) — "bgm/thbgm.fmt" load path. That file likely supplies KEY for archive decrypt. Find code path that sets *dword_4c8704 (mode str) or a global crypto key derived from th08_0100h.ver.
4. Persist every confirmed struct/kernel to symbols.csv + structs.md.

## Evidence addresses (Hopper, VA space, imagebase 0x400000)
- ArchiveReader ctor:        0x4748c0 / entry-table init + parse: 0x474ce0 (open), 0x474fa0 (parse)
- LZSS decompress:           0x4740e0
- Archive header decrypt:    0x43e1d0
- Read file (by name):       0x43e660
- Entry lookup:              0x474c70 (thiscall, obj = archive)
- stran name compare:        0x4b1f5f (case-insensitive)
- Entry read + LZSS wrapper: 0x474af0
- read-from-disk fallback:   direct CreateFileA in sub_43e660 (paths with \ or / skipped)
- Stream vtable:             0x4b98b4 (8 fns, listed above)
- Ring buffer:               0x18d5f68 (8192 bytes, .data sect virtual size 0x1413840, BSS region → zero-filled)
- thbgm loading (per ctor strings): "thbgm.dat", "bgm/thbgm.fmt", "./thbgm.dat"
- game data log strings: "info : %s open arcfile\r\n" etc.
