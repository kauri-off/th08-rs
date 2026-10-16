# TH08 RE — work log (read me first after compaction)

Phase 1 brief: /home/anon/Projects/th_08/specs/PHASE1_BRIEF.md. SPEC_DIR = /home/anon/Projects/th_08/specs/deepseek.
Deliverables (must exist): 00_overview.md, 01_archive.md, 02_bgm.md, 03_images.md, 04_file_index.md + tools/extract_th08.py unpacking th08.dat into ../deepseek/extracted/.

## Tooling
- Ghidra project: ghidra/th08.gpr (th08.exe). Helper: /home/anon/Projects/th_08/specs/ghidra_tools/gh.sh <SPEC_DIR> <Script.java> [args]
  - Decompile.java / Xrefs.java / Strings.java / ApplySymbols.java. RUN SERIALLY.
- Memory: symbols.csv (ApplySymbols), structs.md, this file.
- Python tools in tools/: pbgz.py (container extract, works: `python3 tools/pbgz.py extract`), edz.py (payload unwrap), anm_dump.py (ANM info/thtx renderer/sprites/hist), pe_util.py.

## STATUS
- 01 (archive) — DONE (verified). §8 basename resolution + §9 full 317-file table added.
- 02 (bgm) — sub-agent B working (sub/bgm/). +0x18 semantics = the open question.
- 03 (images) — sub-agent A working (sub/anm/): ANM sprite semantics, THTX fmt 3/5, opcode table, tools/thtx2png.py.
  Parent-side validation DONE: all 306 embedded textures convert cleanly with current sub/anm/tools/thtx2png.py
  (work/validate_thtx.py; PNGs in work/png_all/); fmt->D3DFMT mapping now code-VERIFIED (see below).
- 00 (overview) — sub-agent C working (sub/overview/): main loop, 60fps, 640x480, playfield, managers.
- 04 (file index) — DONE (04_file_index.md). Uses the in-exe resource tables (§4 of that doc).
- extract_th08.py — DONE + VERIFIED: 317/317, byte-identical to decoded/ (diff -r clean), ~45 s.

## NEW VERIFIED FACTS (parent, this session)
- th08.exe file-name literals live at 0x4B42AA-0x4B967F; engine requests paths like
  "bgm/thbgm.fmt", "title/title00.png", "sprt/musiccmt.txt", "result/music.jpg",
  "demo/demorpy0.rpy" but the archive is flat -> loader takes the basename (strchr-like
  Lib_StrFindChar 0x4A48B0). 16 path-qualified archive requests all resolve.
- Resource name tables in .rdata (all VERIFIED by reading exe bytes):
  0x4C72F8 bg anm (9), 0x4C731C stageN.std (9), 0x4C7340 stageN_s.std (9),
  0x4C7364 stgNenm (9), 0x4C7388 second enm list (11, unexplained),
  0x4C73B4 stgenm_XX (6), 0x4C73CC ecldata1..8 (9), 0x4C73F0 ecldata*sp (9),
  0x4C7418 second sp list (11), 0x4C7444 ecldata_XX (6), 0x4C745C stgNtxt (9),
  0x4C7480 eff01..eff09yk (15), 0x4C74C0 msg table (per stage: 4 ptrs + 8 ptrs,
  stage 4: 4+8+12), 0x4C7678 MIDI table (20-byte records, track number field),
  0x4C7948 D3D format descriptor table {d3dfmt,bytes,maskA,maskR,maskG,maskB} for
  ids 0x16/0x15/0x18/0x17/0x19/0x1a -> A8R8G8B8/X8R8G8B8/X1R5G5B5/R5G6B5/A1R5G5B5/A4R4G4B4.
  Consumers: 0x409850 (stage arrays), 0x4390EE (msg + front/times anm), 0x43ABD7 (midi).
- Image dims: 35 end*.jpg 640x480, staff00(b).jpg 640x720, title00/select00.png 640x480.
- msg*.dat: header {u32 2, u32 12, u32 0x510,...}, records marked by u16 0x003C; text
  encoding still unknown (Phase 2).
- ecl: u32 0x800 header then u32 suboffset table, last entry = filesize.


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
Sprite record 20B = {u32 idx; f32 x; f32 y; f32 w; f32 h} — rect in TEXTURE PIXELS (verified: bbox = texture size for ascii/stg1enm/enemy; parent check work/validate_thtx.py era).
Script region layout (VERIFIED all 1151 entries, work/opcodes_scan.py, 0 fail): optional 20B sprite record; then 1-2 sequences of {u16 op; u16 size(incl 4B hdr); payload size-4} each ended by {u16 0xffff; u16 0} + u32 0 pad (8B). 1003 regions 1 seq, 74 regions 2 seq (intro+loop?), 74 regions sprite-record only. 56 distinct opcodes, ALL with consistent payload sizes (see work/opcode_table.json; goes into 03_images.md §5).
Script id examples: ascii -27..-16, text -39..-6, etama -150..-126, enemy 0..100, eff01 -34.
sub_465860 preloads all chunks (0x44B/sprite internal structs, 4B/script ptrs, 12B/chunk records; FUN_0040b880 = alloc with "global.h" tag); FUN_00465ac0 per-chunk texture preload: version check (chunk+0x28 == 3), loads texture when `+0x34 low byte == 0` and name[0] != '@' (error strings at 0x4b8510/0x4b8554/0x4b8574).

### THTX — VERIFIED header/size/pixel-format mapping (code-level)
Header: "THTX", u16 0, u16 fmt, u16 w, u16 h, u32 size (=w*h*bpp; equals bytes left in chunk − 16 for all 306 textures).
fmt id -> D3DFORMAT table 0x4B8480 (VERIFIED): 0->0, 1->0x15 A8R8G8B8, 2->0x19 A1R5G5B5, 3->0x17 R5G6B5,
4->0x14 R8G8B8, 5->0x1A A4R4G4B4; parallel bpp table 0x4B8498 = {4,4,2,2,3,2}. Shipped histogram: 1 x170, 3 x32, 5 x104.
Code proof: FUN_004655e0 does IDirect3DDevice8::CreateImageSurface(w,h,DAT_004b8480[fmt]) (vtbl+0x6C), Lock (vtbl+0x24,
pitch local_18), row-by-row memcpy of w*DAT_004b8498[fmt] bytes from +0x10, Unlock -> THTX bytes are raw D3D texel memory,
no CPU conversion. FUN_00465510 remaps fmt for devices without 8888 support: if table[fmt]==0x15 or 0 -> fmt 5; if 0x14 -> fmt 3
(capability-gated by FUN_00443a90; does not change file bytes). FUN_0043f2cc = linear search of descriptor table 0x4C7948 (24B records).
- fmt1: u32 B,G,R,A memory order (LE u32 = A<<24|R<<16|G<<8|B) -> PNG RGBA.
- fmt5: 0x0FFF transparent background / 0xFFFF opaque white; A = high nibble (A4R4G4B4).
- fmt3: R5G6B5 (bits 15-11 R, 10-5 G, 4-0 B), NO alpha -> PNG alpha=255, colors decoded by bit replication. Settled by
  table+code evidence; bit15-frequency test over all 32 textures (work/fmt3_bittest.py) shows content correlation, incl. all-black eff02.

## NEXT STEPS
1. Finish ANM semantics via Ghidra (find sprite-record parse + script interpreter + draw code; name functions; confirm fields; confirm THTX pixel fmt conversion in D3D texture upload code).
2. 00_overview: find WinMain/main loop, frame timing (60fps), 640x480, playfield 384x448, subsystem list (enemy/item/player/bullet managers).
3. Write 04_file_index (use work/filelist.txt).
4. Write tools/extract_th08.py (single pass container->decoded).
5. Write all 5 spec .md files.
