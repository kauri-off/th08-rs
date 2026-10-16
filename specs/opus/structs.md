# Struct layouts (opus)

## AnmLoadedFile (ANM manager + 0x24 + slot*0x1C), 25 slots — VERIFIED (Anm_Preload 0x465860)
| off | size | type | name | meaning | conf |
|---|---|---|---|---|---|
| 0x00 | 4 | int | slot | slot index | VERIFIED |
| 0x04 | 4 | ptr | data | file buffer | VERIFIED |
| 0x08 | 4 | int | num_entries | | VERIFIED |
| 0x0C | 4 | ptr | sprites | AnmSprite[total] (0x44 each) | VERIFIED |
| 0x10 | 4 | ptr | scripts | ptr[total scripts] | VERIFIED |
| 0x14 | 4 | ptr | textures | {tex, ext_data, ext_size}[num_entries] | VERIFIED |
| 0x18 | 4 | int | load_state | | INFERRED |

## AnmSprite runtime (0x44) — from Anm_LoadEntryTexture / FUN_004661b0
| off | type | meaning | conf |
|---|---|---|---|
| 0x00 | int | anm slot | VERIFIED |
| 0x04 | ptr | texture | VERIFIED |
| 0x08..0x14 | f32×4 | u0,v0,u1,v1 in actual-texture pixels | VERIFIED |
| 0x18 | f32 | actual texture height | VERIFIED |
| 0x1C | f32 | actual texture width | VERIFIED |
| 0x20..0x2C | f32×4 | normalised u0,v0,u1,v1 | VERIFIED |
| 0x30 | f32 | sprite height (logical px) | VERIFIED |
| 0x34 | f32 | sprite width (logical px) | VERIFIED |
| 0x38,0x3C | f32 | scale factors actual/logical (x,y) | INFERRED |

## thbgm.fmt record (0x34) — see 02_bgm.md (VERIFIED)
## PBGZ entry (runtime 0x10): name*, offset, size, reserved — see 01_archive.md (VERIFIED)

## Phase 2 structures (full tables in the spec files)
- AnmVm (0x2A4) — 05_anm_vm.md §2 (VERIFIED). Timer (0xC) — 05 §2.
- Stage object g_Stage 0x4E4030 (partial) — 06_std.md §3. STD file records — 06 §1.
- ECL context (0x228), thread block (0x24B0), enemy object (0x53D0, main fields) — 07_ecl.md §4.3/§5.1.
- Bullet spawn descriptor (enemy +0x2E24, 0x210 bytes): +0 i16 type, +2 i16 colour, +4 f32[3] origin, +0x10 angle1, +0x14 angle2, +0x18 speed1, +0x1C speed2, +0x20 extra commands 18x24 B, +0x1F4 i16 count1, +0x1F6 count2, +0x1F8 aim mode, +0x1FA 0, +0x1FC u32 flags, +0x200 sfx, +0x204 i32, +0x20C template ptr — VERIFIED (07 §9).
