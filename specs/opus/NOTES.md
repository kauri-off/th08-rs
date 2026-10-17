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

# PHASE 3 (started 2026-10-17) — 08 bullets/items, 09 player, answers_phase3
- FIX: op 111 slot stores ints RAW ({f e,f f,i c,i d,u32 a=command bit,i b}); 07 corrected.
- BulletMgr 0xF54E90 (ctor 0x42F360 zeroes 0x1AE95E dwords). Templates: 21 types x 0xD44 at mgr+0 (5 AnmVm: main, eff flag2, eff4, eff8, +0xA90 cancel/aux), script table 0x4B4AD8 (5 ints/type). Hitbox mgr+t*0xD44+0xD34/0xD38, class byte +0xD42 by sprite height (>32:24/0; 16..32: 5/2 [scr 8,0x71-73], 8/1 [9,0x6d,0x6e], 10/1; 8..16: 4/4 [2,4,5,6,0x6a-0x70 incl fallthrough], 6/3; <=8: 4/5). tools/bullettypes.py prints table.
- Bullets mgr+0x1A880 (=0xF6F710) stride 0x10B8, 1536 + sentinel (state 6 at mgr+0x660638). cursor mgr+0x6BA56C (round robin), live count mgr+0x6BA538 (=0x160F3C8), anm file mgr+0x6BA574.
- PlaceBullet 0x42F5F0 field init: +0xD44 pos(z=0.1) +0xD50 vel +0xD68 speed +0xD74 angle +0xD80/+0xD8C timers +0xDAC active ex flags +0xDB0 desc flags +0xDB4 colour +0xDB8 state(1 normal,2/3/4 spawn effect,5 ?,6 sentinel) +0xDC8 desc+0x204 +0xDCC ex index (desc+0x208) +0xDD0 ex cmds copy (0x1B0).
  Effect VM sprite = tmpl sprite + colour remap: main h>32: +colour; 16<h<=32: tbl 0x4C7138; h<=16: tbl 0x4C70F8. Spawn effect: pos -= 4*vel.
- Ex-command setup Bullet_ApplyExCmds 0x42FFC0 (in progress).
- Ex cmds (Bullet_ApplyExCmds 0x42FFC0) DECODED: loop idx<=17; stop if a==0; stop if b==0 && active!=0; skip (idx++) if (descflags & a)==0. bits: 1 speedup(timer F80) ;0x10 accel(e mag,f angle>-990 else cur, c dur) ;0x20 angular(e dspeed,f dangle,c dur); 0x40/0x80/0x100 redirect (0x40 rel angle, 0x80 aim player+e, 0x100 abs angle; f speed>-999 else cur; c period, d count); 0x400/0x800 bounce (e speed>=0 else cur; c count; 0x400 also bottom wall); 0x2000 offscreen grace (c frames, continues); 0x4000 change type c/colour d (copies template, continues); 0x20000 timer c; 0x40000 cancel(state5); 0x80000 sfx c (continues); 0x400000/0x800000 wrap X/Y (timer c); 0x1000000 spawn sub-pattern (consumes 2 slots).
- Calc 0x431240: item mgr calc first (0x440500), bullets iterated order 0,1535..1; state machine 1..5; lasers mgr+0x660938 x256 stride 0x59C. Size-class draw lists heads mgr+0x6BA554[6], link +0xDC0.
- Player collision fns: 0x44A470 graze test (bullet box +20 margin vs player graze box +0x3A4..0x3B4), 0x44A230 hit (vs +0x38C..0x39C), 0x449FF0 bomb-area test (192 areas at player+0xBB834, 0x40 each; returns 2), 0x44A6A0 laser test (graze margin 48), 0x44A5A0 item collect box (+0x3BC..0x3CC), 0x44C1B0 angle to player (pi/2 if coincident), 0x44A930 graze handler.
- On player hit 0x44E160 consumes RNG: 5x Rng_Range(100000) + 3x Rng_FloatRange(100000) (decoy stats). IMPORTANT for RNG sync.
- GameManager 0x160F508: +8 -> stats block (+4 graze cap 99999, +8 score/10, +0xC cap 999999, +0x22 i16 gauge (+0x20 copy), +0x80 f32 ?lives, +0x98 f32 ?power). rank +0x3DE2C (max +0x3DE30, min +0x3DE34, sub +0x3DE38 per 100). gauge min/max +0x3DDF8/+0x3DDFA.
- Bullet ex state blocks 7x0x2C at +0xF80 (speedup F80, accel FAC, angular FD8, redirect 1004, bounce 1030, timer 105C, wrap 1088); +0x10B4 u8 no-collide.
- Offscreen: +0xDA8 grace countdown skips check; else visible box (sprite w/h) vs [0,384]x[0,448]; offscreen w/o 0xDC0 flags: +0xDBA countdown (NOT reset on spawn!) then free; with 0xDC0 flags: +0xDBA++ free at 128.
- Cancels: 0x430830(mode) all bullets->item (mgr+0x6BA570, init 6) or state5 if mode 4; bomb-area -> item g_CancelItem 0x18B8988 (9 => two type 7). 0x430AA0(max,lasersToo) bonus cancel w/ score popup 2000+20n. 0x430D30(pos,r) radius cancel -> item 6. All set mgr+0x6BA53C=10 (spawn->state5 for 10 frames unless flag 0x1000).
- LASERS: spawn 0x430F20 (only if mgr+0x6BA53C==0 or flags&4); struct 0x59C: VM0 body (script type+10, sprite+colour), VM1 +0x2A4 emitter (script tbl70f8[colour]+0x92), +0x548 pos,+0x554 angle(+aim if desc+0x1F8==0),+0x558 tail,+0x55C head,+0x560 maxlen,+0x564 width,+0x568 curwidth,+0x56C speed,+0x570 startup,+0x574 hitstart,+0x578 duration,+0x57C fade,+0x580 hit-during-fade,+0x584 active,+0x588 timer,+0x594 flags,+0x596 colour,+0x598 state.
  Calc per laser: head+=gs*speed; tail=max(tail,head-maxlen); tail<0->0. collision box local frame: centre x=pos.x+tail+len/2 (y=pos.y), size (len*(tail>0?0.7:1), width/2), halved in test. State0: width 1.2 until last min(startup,30) frames then t*width/startup; QUIRK size.x=cur/2; test if t>=hitstart; t>=startup->state1. State1 test w/ graze each 20 frames; t>=duration->state2 (fade 0 -> dead). State2 width shrink, test if t<+0x580, dead at t>=fade. flag1: alpha=min(255,t*255/startup) instead of width. tail>=640 -> dead.
- ITEMS decoded: mgr 2096x0x2E4, dummy +0x17AAC0, list; spawn modes 0/1/2/3/5; types 0..8 (+10 alias); collection handlers; point formula; extend tables 0x4C7A84/0x4C7A9C; power thresholds 0x4C7A6C.
- sht-derived cfg ptrs: g_ShtHuman 0x18B896C, g_ShtYoukai 0x18B8970 (+0x14 attract speed, +0x18 collect box, +0x1C POC line, +0x34 item speed mult). player+3 = youkai flag. player+0xE2A90 bomb-area item (default 6). player+0xE2A7C flag (item value 100 when set; INFERRED bomb/death?).
- stats block (gm+8): +0x74 lives f32, +0x80 bombs f32, +0x98 power f32, +0x24 max point value, +0x2C/+0x30 point counters, +0x34 extends, +0x38 next extend.
- 08_bullets_items.md WRITTEN. 07 op 112 updated. Next: 09_player (Player_Register 0x44C230, calc 0x44C390, struct 0xE2B30 bytes).
- PLAYER: struct 0xE2B30 at 0x17D5EF8. Player_Load 0x44D650: sht ptrs player+0xE2A74 (=0x18B896C human/a) +0xE2A78 (=0x18B8970 youkai/as) via ShtLoad 0x44DD70 (fixes per-level offsets +0x38+8i; entries 0x38 bytes terminated by i16<0; +0x28..+0x34 indexes into fn tables 0x4C7EE0/0x4C7F04/0x4C7F1C/0x4C7F24).
  sht hdr guess: +2 u16 nlevels, +4 f32 (3.0), +8 i32 (18 -> player+0xE2A68 'bomb gauge'?), +0xC hitbox, +0x10 graze size, +0x14 attract speed, +0x18 collect box, +0x1C POC line, +0x20 i32 8000?, +0x24..+0x30 speeds (4,2,2.83,1.41), +0x34 item mult 0.9, +0x38 (off,powerthreshold)[n].
  Player_Load: pos (192, 448-64, 0.49); hitbox half player+0x3D4/3D8 = sht+0xC/2 (+0x3DC=5); graze +0x3E0/3E4 = sht+0x10/2; collect +0x3EC/3F0 = sht+0x18/2. state byte +0 = 1 (spawning), +2=1, +3=2 initially. Bomb fns: player+0x1000[5] = tbl 0x4C7AD0[shot*10..], +0x1014[5] second set.
  Player_UpdateBomb 0x44C650 decoded (bomb start conditions, last spell (player+4) uses 2 bombs, idx = player+3 / 1-.. +2, idx4 special; gauge +-26000/dur per frame; +0xE2A7C=1 while bombing; rank -200; cooldown +0xE2A6C; +0xE2A68 +6 capped sht+8).
  Shot type byte g_ShotType 0x164D0B1 (0-3 teams, 4-11 solo; >=4 even human solo, odd youkai solo: gm+0x3DBA9 copy, 0x42F230 / 0x42F270).
  Input: 0x164D52C current buttons (bit1 bomb), 0x164D534 previous.
- Input bits 0x164D52C: 1 shot, 2 bomb, 4 focus, 0x10 up, 0x20 down, 0x40 left, 0x80 right. dir code +0xE2A98 (1U 2D 3L 4R 5UL 6UR 7DL 8DR). Movement 0x44AEC0: +3 focus(0 human/1 youkai), +8 frames since switch, +5 = mode byte (=+3 after >6 frames; solos fixed by parity). speeds: human sht +0x24/+0x2C(diag), youkai sht +0x28/+0x30; x mult +0x404,+0x408. bounds x[8,376] y[16,432] (gm+0x3DDE4..). boxes hit +0x38C..0x3A0 (pos±0x3D4), graze +0x3A4.. (±0x3E0), collect +0x3BC.. (±0x3EC). trail 16 pos +0x2CC. options 4 x 0x2F4 at +0x40C (fn +0x2EC/+0x2F0, state +0x2C8, timer +0x2E0).
- Gauge thresholds gm+0x3DDF8.. = 0x164D300.. (min,max,humanMax(-8000),youkaiMax(8000),human(-2000),youkai(2000)); variants for shot 3/10 and solos (Player_Load). Gauge drift when shot timer idle: after 30 frames toward 0 by 5/3/2 steps; |g|<=9 -> 0. While shooting (timerA>=0) after delay B: gauge +-min(t/15,21) per frame (sign by focus).
- Shot timer A +0xE2AC4: -1 idle, 0..19 cycle, fire fn 0x450F60 on change. Player bullets 128 x 0x484 at +0xBE838 (calc 0x451150).
- Death 0x44AB40 (state 2, window +0xE2A68 = bombs*6 (+7 cond) cap15, x2 cap30 in spell, x9/5 for shots 0/4/5; 2 if no bombs); state2 0x44CBF0 counts window (time -15/frame, +4 last-spell flag); death at 0: power -16 (or 0), drops, rank -1600; anim 30 frames; respawn 0x44D180 state1 30 frames + 60 frames clear area 768x896; state3 invuln 240 blink.
- Bomb areas: 384 x 0x40 at +0xB8834 (second half +0xBB834 tested by InBombArea, created by 0x44DE60); fields x,y,r,dr,w,h,dw,dh,angle,life,item,..,hits(+0x30),+0x38=1,+0x3C active.
- SHT entry 0x38 decoded: +0 i16 interval,+2 i16 phase (fire if A%interval==phase; laser: slot), +4/+8 offset, +0xC/+0x10 hitbox, +0x14 angle, +0x18 speed, +0x1C i16 dmg, +0x1E i16 (>0: youkai-max flag; <0: time orb on human max), +0x20 i16 origin (0 player, k option k-1), +0x22 i16 type, +0x24 i16 anm script (+10; hit anim +11), +0x26 i16 sfx, +0x28 init fn idx tbl 0x4C7EE0 (0 default 0x44FD80,1 aim enemy 0x450240,2/3 no-bomb 0x44FDD0,4 laser 0x44FE20,5 aim point 0x44FFA0,6 option angle 0x450080,7 random 0x4501B0,8 0x450110), +0x2C update tbl 0x4C7F04 (1 homing 0x450320, 3 gravity 0x450580, 4 laser 0x4505D0, 5 0x450840), +0x30 draw tbl 0x4C7F1C (1 trail 0x450AD0), +0x34 hit tbl 0x4C7F24 (1 0x450C50, 2 0x450EE0). Level = first (thr > power); Sakuya&Remilia team youkai bomb / Remilia solo after 60 bomb frames: level 6/7.
- sht files: human tbl 0x4C7CE0[shot], youkai tbl 0x4C7D10[shot] (solos same file twice). anm 0x4C7CB0.
- Player_DamageAgainst 0x451670 decoded (shots cap 50, bombing dmg/5 min1, bomb areas first 192 at +0xB8834 w/ damage +0x2C period +0x38 max +0x34; youkai max x106/100; time orb accumulator thr player+0xE2B2C (40 / 27 solo human)).
- Bomb fn table 0x4C7AD0: per shot 5 calc (+0x1000) + 5 draw (+0x1014); idx 0 human bomb,1 youkai bomb,2 human LS,3 youkai LS,4 special(0x40D100). LS idx = 2 + (1-focus).
- tools/shtdump.py: 0 problems over 8 sht (terminator = 4 bytes ffffffff; levels 6/7 of ply02as thr 999 = bomb-only). Dump extracted/sht_dump.txt. Header +4 f32 bombs/life(3), +8 i32 deathbomb base (18/10/10/12), +0x20 i32 unk (8000/3000).
- GameMgr_UpdateChecksum 0x406E50 consumes 2x Rng_Range(100000) on every lives/bombs/power/time change! (0x439883 bombs+, 0x43C641 lives+, 0x441850 power+, 0x406FA0 power=, 0x418220 time+, die). Time counter stats+0x3C; 0x418220(n): n>0 also max point value += ((n + (+0x44&1))/2)*10.
- Bombs: 0x40BE30(name,dur,invuln,isLS) sets +0xFE4 dur, state 3 w/ invuln timer; per-bomb params extracted (see 09).
- 09_player.md WRITTEN (options 0x44EE70/0x44F2D0/0x44F5E0/0x44E770 not detailed = open). Symbols applied.
- Next: answers_phase3 (QUESTIONS.md). Supervisor+0x19C = 0x17CE8F4 overlay VM question.
- Phase-3 deliverable 3 DONE: answers in 05 §12, 06 §13, answers_phase3.md (menus/logo/RNG seed deferred).

# PHASE 4 (started 2026-10-17) — 10_msg, 11_rng_and_flow, 12_menus_hud (in that order)
- Dialogue register 0x417F60 (calc 12, draw 15, anm slots 15-19). Timeline op6 -> 0x439810 (HUD?), ECL ops 148/158/179-181.
- MSG (VERIFIED): lives in HUD impl object (g_Hud 0x160F428 -> +8 ptr "FRScreen impl"): file ptr +0x21814, cur instr +0x21818, msg idx +0x2181C (-1 none), Timer +0x21820, wait counter +0x2182C, min-wait +0x21830, face VMs 4x0x2A4 at +0x21834, text line VMs 2 at +0x222C4, boss card VM +0x2280C, +0x22AB0 extra VM; colour tbl ptrs +0x22D54[4] (tbl 0x4C7180 shot*4+k), +0x22D64[4]=0 outline; font size +0x22D74=15; skippable +0x22D7C; speaker +0x22D7D; newbox +0x22D7E; line +0x22D7F; active +0x22D80; textbox +0x22D81; choice +0x22D82; release counter +0x22D78.
  Msg_Start 0x43396D, Msg_Run 0x433DB3, Msg_Draw 0x43542B, Msg_IsBlocking 0x43587E (used by timeline op7), Msg_Load 0x439710 (offsets -> ptrs), Hud_StartMsg 0x439810. Text render 0x4663B0 (vsprintf, GDI "ＭＳ ゴシック" 2x supersample, A1R5G5B5 1024x64 surface 0x1653620, D3DXLoadSurface TRIANGLE). XOR 0x77 strings.
  tools/msgdump.py: 0 problems over 33 msg files; dump extracted/msg_dump.txt. Ops 0..22 (see 10_msg.md).
- INPUT keyboard map (0x43D970, GetKeyboardState VK codes; VERIFIED): Z->1 shot, X->2 bomb, Shift->4 focus, Esc->8, arrows/numpad 8246 -> 0x10/0x20/0x40/0x80 (numpad 7/9/1/3 diagonals), Ctrl->0x100 (skip), Q->0x200, S->0x400, Home/P->0x800 (snapshot), Enter->0x1000, D->0x2000, R->0x4000. Joystick buttons from cfg words 0x17CE870..880 -> bits 1..0x100.
  Input words: 0x164D528 raw cur, 0x164D530 raw prev; 0x164D52C game (replay-substituted) cur, 0x164D534 prev.
- GM flags word gm+0x3DBAC = 0x164D0B4: bit3 replay playback, bit14 spell practice, bit9 stage-end bonus shown, bits 11-12 final route choice, bit6 (0x40) game-clear trigger (op 11).
- Stats +0x28 i8 'clock' counter (max 12, sprite index of HUD VM +0x2156C); stage end += 2 if stats+0x3C(time) < stats+0x40 else 1 (st1-5), +0 6A/6B, +4 extra.
- Clear flags: gm+0x3C49C + shot*0x24 + diff*2 (u16 bitmask by stage idx), second array +0x3C4A6.
- Pause (gm calc ~0x439bc7 area): saves g_Rng, UpdateChecksum, restores g_Rng (so pause doesn't perturb RNG).
- DAT_017CE8CC = HUD full-repaint frame counter (Supervisor decrements).
- 10_msg.md WRITTEN (complete). Hud object: +0xC front.anm, +0x10 stgNtxt.anm, +0x14 times.anm, +0x18 loadingNN.anm, +8 impl (0x8C2E dwords).
- NEXT: 11_rng_and_flow (RNG, GameManager new game/stage flow, Supervisor scenes, stats block).
- RNG: u16 state @0x164D520 + u32 call counter @+4. Next16: t=(s^0x9630)-0x6553; s=rotl16(t,2). Seed at boot = timeGetTime() low16 (0x4464C8). Replay: per-stage record +0x1A u16 seed; playback Rng_SetState 0x453BE0. StageStart stores seed 0x164D2C4 = g_Rng after setup. Replay calc prio 7 (0x4522A0) stores state each frame & zeroes counter.
- StageStart 0x43ABD7 runs in loader thread (0x4486BA) from GM on-register 0x43AAF4. GM calc 0x439BC7: loading flag gm+0x38 (0x160F540): returns 3 (stop calc chain) + gm+0x3C counter++; thread waits counter>=30 (80 replay) then until flags bits5-6 ==0.
- Stage clear: HUD 0x435900 sets flags bits5-6 =2 when clock anim done (stage<6); msg op 11 sets 2 (st 6-8). GM calc on 2: set 3, record clear masks, next scene 0x164D0A4/0x17CE8B4. Stage advance 0x43C4B3: 0->1->2->(4A=3 for teams1,2 / 4B=4 for teams 0,3)->5->(6A if route bits 0 else 6B).
- Supervisor +0x158 cur scene, +0x15C next (0x17CE8B4), +0x160 prev. Scenes: 0 boot,1 menu,2 game,3 reload GM (next stage),5 result,6 result(after game),7 game->menu(replay end),8 music room,9 ending,10 retry stage1,11/12 special reload, -1 quit, 4 -> exit code 5.
- GM calc demo mode (flags bit1): key -> menu; time limit gm+0x3DBB8 vs demo idx gm+0x3DBB4: fade at 6000/4800/4920/6900, menu at 6120/4920/5040/7020.
- Pause: ESC edge (raw input) when not paused; gm+0x3DBB2 pause state; returns 3 (stop calc) while paused.
- 11_rng_and_flow.md WRITTEN (RNG alg/seeding/consumers, GM fields+flags, StageStart, GM calc flow, routing, death/continue, stats block, replay stage record, Supervisor scene table, time system).
- NEXT: 12_menus_hud (MainMenu 0x47146D/0x4674E0 state machine, title01.anm scripts, pause 0x4037B0 / continue 0x404890 menus (ascii.anm scripts 0xC..0x18), HUD draw 0x433927 & update 0x435900, front.anm, th08logo.jpg).
- 12_menus_hud.md WRITTEN (boot logo/loading states, main menu screens + title01 VM map, difficulty/char/practice/option/replay, pause & continue menus, HUD object, front.anm layout, side panel, boss UI, popups, clear panel + bonus formula).
- tools/rng.py added (reference RNG). PHASE 4 DELIVERABLES DONE. Symbols applied.

# PHASE 5 (started 2026-10-17) — 13_sfx_effects, 14_bombs_options, answers_phase5 (in that order)
- Re-dumped extracted/all_decomp.c with all symbols (old: all_decomp_phase2.c). fn.sh takes addr without 0x (e.g. 0045d550).
- SFX (VERIFIED): Sound_LoadSE 0x45d3f0: 36 wavs (ptr tbl 0x4c81b0), 46 SE ids tbl 0x4c8040 {i32 wav, i16 vol(1/100dB, applied at load), i16 prio (stored mgr+0x408+id*4, unused)}; DuplicateSoundBuffer per SE id at mgr+0x208+id*4.
  Sound_Play(id,pan) / Sound_PlayAt(id,x): pan=trunc((x-192)*1000/192). Queue: 12 slots mgr+0x620 (id, -1 end), per-slot pan list mgr+0x680+slot*0x200 (max 128), count mgr+0x650. Same id in a frame merges (pans averaged). Full (12 distinct) -> dropped.
  Sound_Update 0x45d790 (Frame_Run after calc chain): BGM cmd queue mgr+0x1f88; if cfg SE on (0x17ce890): per slot Stop, pos 0, pan avg, vol = cfg SE vol (0x17ce898, 0..100) v: k=1-(1-v/100)^3, vol=trunc((tblVol+5000)*k)-5000 (v=0 -> -10000), Play.
- EFFECTS: mgr 0x4ECE60, entries 0x360 at +0x1C: 653 (0..511 ring cursor +0; 512..639 Effect_SpawnHigh; 640+n fixed slots). type tbl 0x4c6d30 {script(etama.anm slot6), update fn, init fn} 66 types. Lists (calc): +0x354 layer, +0x355 over-bullets, blend. Draw: prio12 lists 0/2/4; list1 in Stage_DrawHigh (cfg 0x17ce894 0 none/1 half/2 all); list3 end of BulletMgr_Draw. Ring renderer 0x427450 (VM-param driven). Calls list: /tmp/effcalls (re-run awk if lost).
- 13_sfx_effects.md WRITTEN (SE table/queue/flush/call sites; effect mgr layout/spawn/calc/draw/ring renderer/66 types/spawn sites; ScreenEffect types 0-7 (ECX type, EDX p0 — 07 lists lacked p0, corrected in 13 §3); popups; spell bg). /tmp/etama_scripts.txt = script lifetimes (regen from anm_dis).
- NEXT: 14_bombs_options (bomb fns tbl 0x4C7AD0; options 0x44E3A0/0x44EE70/0x44F2D0/0x44F5E0; laser upd 0x4505D0/0x450840).
- bomb_draft.md has all 16 bombs + dissolve decoded (read after compaction). NEXT: options 0x44E3A0/0x44EE70/0x44F2D0/0x44F5E0 + draws, lasers 0x4505D0/0x450840, shot rotation, red tint, hitbox marker.
- 14_bombs_options.md WRITTEN (all 16 bombs + dissolve, options 0x44E3A0/0x44EA40/0x44EE70/0x44F2D0/0x44F5E0, Alice laser, shot draw/red tint FF4040/hitbox marker = effect 22 script 54 sprite 218). 13 §2.5 corrected (ellipse mode).
- NEXT: answers_phase5.md (QUESTIONS.md items).
- answers_phase5.md started (impl2 done). Findings so far: mirroring (op64 delta mirrored, op66 not), enemy draw (layers 0-1 prio8, 2-3 prio11; e+0x332F layer)
- answers impl5 written. NEXT impl6 questions.
- NOTE: 0x164D2DC/E0 = playfield origin (32,16), NOT shake (fixed in 13/14/07). Shake = AnmMgr+0x1C/0x20.
- found: rank time increase 0x42C3B0; contact test 0x42C290; spell bit7 = bombed during card; homing targets filled in EnemyMgr_Calc (player+0xE2AA4 boss/lowest, +0xE2ABC aimed within 64px highest). Hit test returns 1 for invulnerable -> bullet cancelled.
- user added impl8 section (7 items) to QUESTIONS — answer too.
- impl6 answers written
- impl7 answers written. NEXT impl8 (7 items).
- impl8 answers written. NEXT: corrections list + final verification.
- PHASE 5 DONE: answers_phase5.md complete (impl2/5/6/7/8), corrections listed, symbols applied (541).

# PHASE 6 (started 2026-10-17) — 15_files_replay_score, 16_endings_result, answers_phase6
- CFG (VERIFIED Config_Load 0x4477B1): 60 B at 0x17CE870 (=Supervisor+0x118). +0x00..+0x11 i16 pad buttons (shot,bomb,focus,pause,up,down,left,right,skip@+0x10; dflt tbl 0x4C78C8: 0,1,2,4,-1,-1,-1,-1,3), +0x14 u32 version 0x80001, +0x18/+0x1A i16 stick threshold 600, +0x1C lives opt(<7,d2), +0x1D (<4,d3, unused), +0x1E 16bit color(<2,d0), +0x1F music 0off/1wav/2midi(<3; d1 if thbgm.dat ok else 2), +0x20 SE on(<2,d1), +0x21 last diff(<6,d1), +0x22 window(<2,d0), +0x23 frameskip(<3,d0), +0x24 effects(<3,d2), +0x25 slow(<2,d0), +0x26 shot=focus keyconf(<2,d0), +0x27 bgm vol 100, +0x28 se vol 80, +0x38 u32 debug flags (bit0 forced 1). invalid -> all reset. size must be 0x3C. Written at load & WinMain exit.
- SCORE.DAT: load 0x45A5E0 (ScoreDat_Load), save 0x453D0D (ScoreDat_Save, this=Result obj). Crypt(0x59,0x79,0x100,0xC00) + rolling xor from byte1 (acc=rotl3(acc+prev)) + u16 sum bytes4.. at +2; hdr 0x1C: +1 key,+2 sum,+4 ver1,+6 rnd,+8 0x1C,+0xC runtime ptr,+0x10 total size,+0x14 body size,+0x18 compressed size. LZSS body. chunks: magic,u16 size,u16 size,u8 ver. TH8K(12,v1) HSCR(0x168,v4) CLRD(0x24 x13,v4) CATK(0x22c x222,v3) PSCR(0x178 x12,v2; only if +0x175) LSNM(0x18,v1) FLSP(0x20,v1) PLST(0x228,v2) VRSM(0x1C,v1). 2 Rng_Range16 draws on save!
- HSCR: +0xC score,+0x10 f32 slowdown%,+0x14 shot,+0x15 diff,+0x16 stage(99=all clear),+0x17 name[9],+0x20 date[6],+0x26 continues,+0x28 cfg copy(60),+0x64 playtime frames,+0x68 point items,+0x70 miss,+0x74 bombs,+0x78 last spells,+0x7C pauses,+0x80 time orbs,+0x84 human rate*100,+0x88 u8[222] captured.
- CATK: +0xC u16 id,+0xE u8 chksum,+0xF u8 last diff,+0x10 name[48],+0x40 boss[48],+0x70/+0xB0 comments(64, practice capture only),+0xF0 u32[13] max bonus,+0x124 attempts[13],+0x158 captures[13],+0x18C prac max bonus[13],+0x1C0 prac attempts[13],+0x1F4 prac captures[13]; idx12=total. Spell_Start 0x4152A0 (attempts), Spell_End 0x4161B0 (captures).
