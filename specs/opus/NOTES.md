# NOTES (opus, Phase 1)

## Status
- PHASE 1 COMPLETE: 00_overview, 01_archive, 02_bgm, 03_images, 04_file_index written; tools th08dat.py, thbgm.py, anm.py, pe.py, dump_bgmtab.py; symbols applied to Ghidra (75).
- extracted/all_decomp.c = full decompile dump (ghidra_scripts/DumpAll.java) for grepping.

## Findings log
- th08.dat begins with magic "PBGZ" (VERIFIED by hexdump); rest looks obfuscated.

## Open questions / next phases
- ANM opcode semantics (AnmVm_Run 0x45EA00), STD/ECL/MSG/SHT/END/RPY formats.
- Frame-skip config semantics (skip 0 and 1 both draw every frame?).
- Exact roles of _s.std / *sp.ecl / *_xx character files (spell practice? last word?).
- Player/enemy coordinate space confirmed only for bullets.
- ARCHIVE (VERIFIED, extractor tools/th08dat.py extracts all 317 files, signatures valid):
  - open: FUN_004748c0 -> FUN_00474ce0 (header) -> FUN_00474fa0 (table parse). Lookup FUN_00474c70 (stricmp linear).
  - XOR "block-reverse" cipher FUN_0043e1d0(buf,size,key,step,block,limit). LZSS FUN_004740e0 (13-bit off, 4-bit len+3, dict 0x2000 start pos 1, offset 0 = end).
  - header: "PBGZ" + 12 bytes enc(0x1b,0x37,0xc,0x400): count+123456, tableoff+345678, tablesize+567891.
  - table: [tableoff..EOF] enc(0x3e,0x9b,0x80,0x400) then LZSS. entries: name\0, off, size, unk(=0 always).
  - per file: LZSS -> "edz"+type char -> decrypt rest with table @004c78e0 (8 entries M T A J E W - *). magic "edz" @004c7940 (stored +0x20/+0x40/+0x60).
  - generic file loader FUN_0043e660(name, &size, from_disk) strips path, arc lookup; disk fallback when flag set.
- BGM (VERIFIED, tools/thbgm.py): thbgm.dat header "ZWAV",1,0x800,0 checked in FUN_004477b1 (config load).
  thbgm.fmt (from archive): 52-byte recs, name[16], off, total_len(+0x14, used only for preload buffer size), loop_start(+0x18), loop_end(+0x1c), WAVEFORMATEX(+0x20, 18B)+2 pad; terminated by empty name.
  Stored data per track = loop_end bytes (tracks contiguous: off[i+1]=off[i]+loop_end[i]; last ends exactly at EOF).
  Stream: first pass seek off, play loop_end bytes; loop: seek off+loop_start, play loop_end-loop_start (FUN_004733e0).
  Play: FUN_00447d56(slot,name) replaces ext with "wav" when cfg music mode==1 (WAV), MIDI when ==2. Lookup FUN_0045c6b0 basename; not found -> index 0.
  STD header has 3 BGM names at +0x290/+0x310/+0x390 (128B each, ' ' = none).
- ANM (VERIFIED, tools/anm.py, 0 problems over 113 files): entry chain, 0x40 header (version 3), sprite offs, script (id ignored, offset) pairs, THTX (fmt/w/h/size). Formats 1=8888,3=565,5=4444. UV normalised by header w/h. Instr header u16 op,u16 size,i16 time,u16 mask. Docs: 03_images.md.
- Files written: 01_archive.md, 02_bgm.md, 03_images.md. Next: 00_overview.md (main loop, managers), 04_file_index.md.
- OVERVIEW (00_overview.md written): Frame_Run 60Hz deadline loop, funcChain calc/draw priorities table, playfield viewport (32,16,384,448), camera fov pi/6.
- 04_file_index.md written: stage index table (0..8, 9..17 = _s/sp), 12-entry character tables, msg team letters, slot map.

# PHASE 2 (started 2026-10-17)
Plan: 05_anm_vm -> 06_std -> 07_ecl -> 08 -> 09 -> 10 -> 11. Read code from extracted/all_decomp.c (re-dump after ApplySymbols).
- Ghidra: ran ghidra_scripts/FixThiscall.java (sets __thiscall where ECX read before write, ignoring PUSH ECX) -> decompile now shows `this` args. Re-dumped extracted/all_decomp.c. tools/fn.sh <addr> prints a function; tools/gh.sh <Script> runs scripts with opus script path; Disasm.java <addr|addr:len>.
- Helpers: Timer struct 0xC {prev(int, -999 on set), subframe float, current int}; Timer_Set 0x4065f0, Timer_Get 0x40d3b0, Timer_Tick 0x406640. Matrix_Identity 0x406940. ftol 0x4a3e70 = truncation.
- RNG object at 0x164d520: Rng_Next16 0x43ecc0, Rng_Next32 0x43ed20 (hi16<<16|lo16, first call = high), Rng_Float 0x43ed50 = u32 / [0x4b5b10], Rng_Range(n) 0x406ef0 = n==0?0:u32%n, Rng_FloatRange 0x40d390 = f*Rng_Float.
- Angle_Normalize 0x43edb0(a, delta): a+=delta, then while > [4b4334] (pi) -= [4b4460](2pi) max 17 iter, while < [4b4464] (-pi) += 2pi.
- math wrappers: 41f090(f,f)->4a431a (fmod?), 409060->4a3fa0, 408d40->4a3ef0, 462250->4a4110, 462210->4a5e20, 462230->4a5f30 (identify).
- SCOPE (coordinator): this run = ONLY 05_anm_vm, 06_std, 07_ecl (+tools, full opcode coverage). 08-11 facts -> "For later phases" section only.

## For later phases
- RNG (11): Rng object 0x164d520; Next16 0x43ecc0; Next32 = (Next16<<16)|Next16 (first call high); Float = u32/[0x4b5b10]; Range(n)=n?u32%n:0.
- ANM VM fully mapped (AnmVm_Run 0x45ea00, symbols added). Tool tools/anmdis.py: all 1151 scripts parse, 0 problems, all ops known. Dump extracted/anm_dis.txt.
  Draw modes 0-5 (vm+0x200, only value 2 set from data). Billboard (mode 4) reference vector 0x4ea3f4 never written (zero) -> degenerate? (unused by scripts).
  CreateDevice flags 0x40/0x20, no FPU_PRESERVE.
## For later phases (cont.)
- Game speed g_GameSpeed 0x17ce8e0 set to 1/N by ScreenEffect funcs 0x424f90/0x4251b0, restored 0x425290 (sets Supervisor flag 0x20 -> Timer_Sub extra decrement). Interrupt 2 sent to VMs 0x4e4b68/0x4e4e0c on slowdown, 1 on restore.
- Screen shake: AnmManager+0x1c/+0x20 offset written ~line 45026 of dump (ScreenEffect).
- 05_anm_vm.md WRITTEN (complete). Next: 06_std (Stage 0x409850 load, 0x40a1b0 draw, std VM).
- STD decoded: tools/stddump.py (0 problems, all ops 0..34 named; data uses 16). Stage_Calc 0x407400, DrawLayer 0x40a1b0. ECL op 0x92 -> stage+0x6260 section request (STD op 31 labels).
- COORDINATOR Qs: (1) STD viewport/projection -> answer in 06; (2) funcchain calc ret 0 vs draw nodes, draw runner ret codes 4/5/6 -> note in 00_overview §3; (3) who starts title BGM.
- Q2, Q3 answered in 00_overview §3.1/3.2. Q1 goes into 06_std.
- 06_std.md WRITTEN (complete; answers Q1). VM A/B/C gating is sprite_index>0 (vm+0x214), op30 quirk disables VM A.
- Next: 07_ecl. Ecl_Run 0x4184b0 (INFERRED). EnemyManager_Load 0x42ebf0.
- ECL draft notes in ecl_draft.md (read it after compaction).
- ECL natives named EclNative_N in symbols.csv; decompile in extracted/ecl_native.c
- tools/ecldis.py: 0 problems all 24 ECL files, all ops named; dump extracted/ecl_dis.txt
- ECL analysis complete enough; writing 07_ecl.md now (draft in ecl_draft.md).
- 07_ecl.md WRITTEN (complete). All three tools pass with 0 problems. Phase-2 scope (05-07) DONE.
- For later phases: bullet struct stride 0x10B8 at 0xF6F710 (1536+1 slots; template table bulletmgr+type*0xD44); laser +0x548 pos, +0x554 angle, +0x584 active, +0x598 state; item mgr 0x1653648 FUN_4400a0(pos,type,flag); effect mgr 0x4ECE60 (0x360 stride, 512); spell mgr 0x4EA670 (bit0 active); HUD 0x160F428; GameManager 0x160F508 (+0x3DE2C rank, +0x30 difficulty); player 0x17D5EF8 (pos 0x17D61AC).
