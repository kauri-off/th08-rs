# TH08 RE — work log (read me first after compaction)

Phase 1 brief: /home/anon/Projects/th_08/specs/PHASE1_BRIEF.md. SPEC_DIR = /home/anon/Projects/th_08/specs/deepseek.
Deliverables (must exist): 00_overview.md, 01_archive.md, 02_bgm.md, 03_images.md, 04_file_index.md + tools/extract_th08.py unpacking th08.dat into ../deepseek/extracted/.

## Tooling
- Ghidra project: ghidra/th08.gpr (th08.exe). Helper: /home/anon/Projects/th_08/specs/ghidra_tools/gh.sh <SPEC_DIR> <Script.java> [args]
  - Decompile.java / Xrefs.java / Strings.java / ApplySymbols.java. RUN SERIALLY.
- Memory: symbols.csv (ApplySymbols), structs.md, this file.
- Python tools in tools/: pbgz.py (container extract, works: `python3 tools/pbgz.py extract`), edz.py (payload unwrap), anm_dump.py (ANM info/thtx renderer/sprites/hist), pe_util.py.

## STATUS
- 01 (archive) — DONE (verified). 02 (bgm) — 95% (field +0x18 semantics open, documented as INFERRED).
- 03 (images) — 80%: THTX header+size VERIFIED; pixel format candidates rendered (ascii/stg1enm/eff01 PNGs in work/) — fmt 5 = A4R4G4B4-family (LE, A=high nibble) strongly indicated visually; fmt 3 = unknown (eff01). Sprite 20B record: {u32 id; f32 x; f32 y; f32 w; f32 h}? Fields verified structurally but x/y semantics unclear for font-like sheets (ascii x=8k grows past texture width -> maybe logical strip coords or cell-index*size). Script table = {u32 data_off; i32 script_id}. Script data = sequence of ops {u16 op; u16 size; payload(size-4)} but some scripts start with a 20B record {u32 sprite_id; f32 a,b,c,d} (e.g. ascii script -27 = sprite 159 + 4 floats) — NEEDS Ghidra confirmation (find ANM draw/parse code).
- 00 (overview) — TODO (need main loop, 60fps timing, 640x480, playfield 384x448 confirmation via Ghidra).
- 04 (file index) — TODO (work/filelist.txt has all 317 names+sizes; classify by pattern).
- extract_th08.py — TODO (compose pbgz+edz single pass).

## VERIFIED FACTS (highlights)
Hashes: th08.exe 330fbdbf58a710829d65277b4f312cfbb38d5448b3df523e79350b879213d924; th08.dat 9d7edf43b8ddd347cbb641836f6b5050745dd936f688daebbf9382ca557043bb; thbgm.dat 2c2ef05f9ff6f43f752dae5da519a2477372c3f8875b7b44996a8f69fdad4d89.

### PBGZ (th08.dat) — VERIFIED, extractor works (317 files)
Header cipher k0=0x1b k1=0x37 blk=12 -> 12 bytes -> u32s minus 123456/345678/567891 = count 317, table_off 0x2CAA0C2, table_usize 9063. Table region cipher k0=0x3e k1=0x9b blk=0x80 limit=0x400 (only first 1024 bytes; rest raw) -> LZSS -> records {NUL name, u32 off, u32 usize, u32 fC(=0)}; sentinel offset=table_off. File data = raw LZSS (no cipher). Extent = next.off - off.
Cipher: blocks of blk; odd 0-based indices descending b-1..1, then even b-2..0; each byte XOR rolling key (k0 start, +=k1 mod256); tail = ((len%blk < blk/4) ? len%blk : 0) + (len&1) raw; loop while remaining>0 && budget>0, budget-=block.
LZSS: MSB-first; flag 1=literal 8 bits; 0=match: 13-bit ring index (0=end), 4-bit len+3; 8KB ring at g_LzssRing 0x18d5f68, index starts at 1, persists, no reset.

### edz payload wrapper — VERIFIED
file = 4-byte tag + cipher(payload); type from tag char; key table at 0x4c78e0 (8 x 12B): M(1b,37,40,2800) T(51,e9,40,3000) A(c1,51,1400,2000) J(03,19,1400,7800) E(ab,cd,200,1000) W(12,34,400,2800) -(35,97,80,2800) *(99,37,400,1000). Decode = cipher-decode, drop first 4 output bytes. Type char observed: anm=A(113) wav=W(39) jpg=J(37) dat/ecl/etc '-' (E for ecl, T for txt).

### thbgm.dat / thbgm.fmt — VERIFIED (except notes)
thbgm.dat: "ZWAV", u32 1, u32 0x800, u32 0; PCM at 0x10; size 449,961,024 = end of last track.
fmt: 21 entries x 52B + 17 zero bytes. Entry: name[16] "th08_XX.wav"; +0x10 u32 offset (VERIFIED via SetFilePointer); +0x14 u32 preload read size (VERIFIED: malloc+ReadFile size; always > +0x1C); +0x18 u32 4-aligned, < +0x1C (loop start? INFERRED); +0x1C u32 length (VERIFIED cumulative; sums to file size); +0x20 WAVEFORMATEX (PCM 2ch 44100 16bit, ba=4), +0x30 u32 0.
fmt order: 01,00,03,04,05,06,07,08,09,10,11,12,13,14,13b,15,18,19,16,17,20 (note 13b/15 swapped vs music room order).
musiccmt.txt (decoded, Shift-JIS) = music room order + titles + comments: No.1 永夜抄～Eastern Night (title), 2 幻視の夜～Ghostly Eyes (st1), 3 蠢々秋月 (Riggle), 4 夜雀の歌声 (st2), 5 もう歌しか聞こえない (Mystia), 6 懐かしき東方の血 (st3), 7 プレインエイジア (Keine), 8 永夜の報い (st4), 9 少女綺想曲～Dream Battle (Reimu), 10 恋色マスタースパーク (Marisa), 11 シンデレラケージ (st5), 12 狂気の瞳 (Reisen), 13 ヴォヤージュ1969 (st6), 14 千年幻想郷 (Eirin), 15 竹取飛翔 (Kaguya), 13b ヴォヤージュ1970 (Last Spell), 18 エクステンドアッシュ (Extra), 19 月まで届け不死の煙 (Mokou), 16 月見草 (ending), 17 Eternal Dream (staff), 20 東方妖怪小町 (Last Word).
BGM object g_Bgm 0x18b8a68: +0x1e80 desc ptr table; +0x1ec0 buffer ptr; +0x1f00 buffer; +0x1f40 size; +0x1f84 raw fmt blob; +0x4108+0x100*i per-track state (name string); +0x5108 filename; +0x5208 player obj. Code: 0x4477b1 ZWAV check, 0x446590 init, 0x45cc30 fmt load, 0x45ce70 preload, 0x45d0f0 load, 0x45e370 free, 0x471c40 graph build, 0x45c6b0 name->index.

### ANM — VERIFIED structure (5-6 files cross-checked)
Chunk header 0x3C: +0x00 n_spr, +0x04 n_scr, +0x08 0, +0x0C u32 (256=1.0 fixed), +0x10 u32 (256=1.0), +0x14 u32 5, +0x18 color (0/0xFF000000), +0x1C u32 name_off, +0x20 0, +0x24 0, +0x28 u32 3 (version), +0x2C u32 (10/11), +0x30 u32 thtx_off, +0x34 u32 low byte flag (1=skip external texture load), +0x38 u32 next chunk delta (0=end).
Then: n_spr x u32 sprite offs (0=null, dense 20B records after name), n_scr x {u32 data_off; i32 id}, name string (NUL). Sprite region starts right after name; script data after sprites; THTX at +0x30.
Sprite record 20B = {u32 idx; f32 x; f32 y; f32 w; f32 h}?? (ascii: (k, 8k, 0, 8, 8); stg1enm: (k, 48k, 0, 48, 64); text: (k, 0, 18k, 320/256, 17); enemy: (k, 32k, 0, 32, 32)).
Script id examples: ascii -27..-16, text -39..-6, etama -150..-126, enemy 0..100, eff01 -34. Script data ops: {u16 op; u16 size; payload}; observed ops 3,6,8,0x15,0x16,0x17,0x18,0x1a?,0x1e,0x20; but first script of a chunk may be a 20B {u32 sprite_id; f32,f32,f32,f32} record referencing last sprite of chunk.
sub_465860 loads chunks (alloc 0x44B/sprite internal structs, 4B/script ptrs, 12B/chunk records); sub_465ac0 per-chunk texture: version check (`+0x28 == 3`; `+0x34 & 0xff`), name at +0x1C, '@' = skip load; error strings "animation version mismatch" etc.

### THTX — VERIFIED header/size, pixel fmt partially
Header: "THTX", u16 0, u16 fmt (3 or 5), u16 w, u16 h, u16 0, u16 2 (bytes/px). Pixels follow; blob size = 16 + w*h*2. fmt=5: dominant patterns 0x0FFF transparent background / 0xFFFF opaque white -> A = HIGH nibble (A4R4G4B4-style, LE). fmt=3 seen only in eff01; needs decision (check work/eff01_m*.png).

## NEXT STEPS
1. Finish ANM semantics via Ghidra (find sprite-record parse + script interpreter + draw code; name functions; confirm fields; confirm THTX pixel fmt conversion in D3D texture upload code).
2. 00_overview: find WinMain/main loop, frame timing (60fps), 640x480, playfield 384x448, subsystem list (enemy/item/player/bullet managers).
3. Write 04_file_index (use work/filelist.txt).
4. Write tools/extract_th08.py (single pass container->decoded).
5. Write all 5 spec .md files.
