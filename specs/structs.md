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

## Phase 3 structures (full tables in 08 / 09)
- Bullet manager 0xF54E90 (templates 21×0xD44, bullets 1536×0x10B8 at +0x1A880, lasers 256×0x59C at +0x660938) — 08 §2, §6.
- Bullet ex-command slot (24 B: f32 e, f32 f, i32 c, i32 d, u32 cmd, i32 chain) — 08 §5.
- Item manager 0x1653648 (2096×0x2E4, list) — 08 §8.
- Player 0x17D5EF8 (0xE2B30), player bullet 0x484, option 0x2F4, area 0x40, .sht header/entry — 09.

## Supervisor g_Supervisor 0x17CE758 (0x364 bytes = 0xD9 dwords) — fields confirmed in phase 7
| off | abs | type | meaning | conf |
|---|---|---|---|---|
| 0x008 | 0x17CE760 | ptr | IDirect3DDevice8 | VERIFIED |
| 0x118 | 0x17CE870 | 60 B | th08.cfg copy (15 §1.1); +0x13E = cfg+0x26 joystick shot-focus, +0x150 = cfg+0x38 debug flags | VERIFIED |
| 0x158/0x15C/0x160 | 0x17CE8B0.. | i32 | current / next / previous scene (11 §7) | VERIFIED |
| 0x188 | 0x17CE8E0 | f32 | game speed | VERIFIED |
| 0x190/0x194 | 0x17CE8E8/EC | f32 | slowdown accumulators A / B (15 §2.11) | VERIFIED |
| 0x198 | 0x17CE8F0 | u8 | fps_now (answers_phase7 §6) | VERIFIED |
| 0x1A4 | 0x17CE8FC | u32 | flags; bit table in answers_phase7 §2 (bit 3 never set) | VERIFIED |
| 0x338..0x34C | 0x17CEA90.. | u32×5 | zeroed by MainMenu_OnRegister; +0x338 = replay fps flag (always 0), +0x33C (0x17CEA94) = playback fps-red flag | VERIFIED |
| 0x350 | 0x17CEAA8 | u32 | fog state cache | VERIFIED |
Whole object is overwritten with 0xFFFFFFFF on anti-tamper detection (answers_phase7).
