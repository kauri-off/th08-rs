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
