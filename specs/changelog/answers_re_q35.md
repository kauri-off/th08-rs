# RE-Q35 — SPEC-GAP batch 1: ECL VM and enemies (2026-10-24)

Brief: docs/briefs/RE_Q35_BRIEF.md; markers: docs/specgaps/batch1_ecl_enemy.txt (17). Runs: re/work/runs/q35/
(CHECKPOINT.md). All answers come from the code (fn.sh / disx.py) and the ECL data (re/extracted/ecl_dis.txt); no
session run was needed, since every marker is either already covered by a spec section or settled by the code. Data
reachability was checked with ecl_dis scans (below).

## Changed spec sections
| spec § | change |
|---|---|
| 07 §4.2 | template zero-fill: the interrupt table e+0x2CF0 is all 0 (sub 0); only pending e+0x2D30 := −1 |
| 07 §5.5 | no registration / range test: an unregistered index enters sub 0; a negative entry leaves the main context untouched, but the skip, push, depth += 1 and pending := −1 still happen |
| 07 §6 (10033) | an int read calls `Rng_NextFloat` (one Next32) and truncates: 0, or 1 for u32 ≥ 0xFFFFFF80; no int read in the data |
| 07 §7 op 86 | global-only variable ids are read normally through the null enemy (only enemy/context ids fault) |
| 07 §10.1 step 6 | homing-target pick (+0xE2AA4 / +0xE2AC0): after the damage block, inside the bit-6 branch, not affected by the first call's spell/parent/bomb gate |
| 07 §12 row 13 | only `Stage_RequestTint`; +0x646C is not written |
| 07 §12 row 16 | exact `EclNative_16`: copies the caller's **int var 10007** (not float 10019); 3D `Vec3_LengthSq` < 4096 strict; member pos e+0x2D34; caller's chain; data use (bullet colour) |
| 07 §12 row 24 | var 10000 := stats +0x1C spell cards captured; the Extra-stage gate (ecldata8 sub84) |
| 11 §5 (+0x1C) | reader native 24, lifetime (per game) |

## Markers
| port marker | answer | spec § | verdict |
|---|---|---|---|
| src/eclvm.rs:239 (int read 10033) | `Ecl_GetIntArg` case 0x2731 (0x41F5E7): `Rng_NextFloat` + `ftol`, so it consumes one Next32. The value is 0, or 1 when u32 ≥ 0xFFFFFF80 | 07 §6 | **CODE BUG** (unreachable: 0 int reads of 10033 in the data) |
| src/eclvm.rs:360 (interrupt only in main) | already in the spec: the pending test is at the LOOP of every context, threads included (0x418510 → 0x41C88A); thread-time behaviour described there | 07 §5.5 "Thread contexts" | **CODE BUG** (unreachable: pending is only ever set by timeline op 8, which runs before the enemies) |
| src/eclvm.rs:414 (unregistered index ignored) | the template's zero-fill gives table entries 0, so the main context enters sub 0. For an entry < 0, `Ecl_SetSub` returns at once, but the ip skip, push, depth+1 and pending −1 still happen | 07 §4.2, §5.5 | **CODE BUG** (port: table default −1 and full ignore; unreachable: all op 126 register index 1, all timeline op 8 send index 1) |
| src/eclvm.rs:724 (op 92 position) | already in the spec: ops 90/92 spawn at (x, y, 0) as given; op 92's follow offset/final pos is written after the spawn run | 07 §10.5 | right |
| src/eclvm.rs:1130 (native 24) | var 10000 := `*(GM+8)` +0x1C = spell cards captured in this game | 07 §12 row 24, 11 §5 | **CODE BUG**, reachable: Extra stage ecldata8 sub84 (`ijge $10000, 7`); the port always reads 0, so that boss always deletes itself |
| src/eclvm.rs:1217 (native 22 face) | already in the spec: `Spell_ShowNameOnly` 0x415D10 passes face −1 explicitly (sprite 1), so there is no portrait and no cut-in | 07 §11.1 bit 10 | right |
| src/eclvm.rs:1239 (native 13 +0x646C) | native 13 = `Stage_RequestTint`(0xFFC03030) only; +0x646C is not written | 07 §12 row 13 | right |
| src/eclvm.rs:1250 (stage VMs of 26/28/29) | already in the spec: g_Stage +0xB38 / +0xDDC (0x4E4B68 / 0x4E4E0C), the native-12 pair | 07 §12 rows 26, 28, 29 | right |
| src/eclvm.rs:1291 (native 16 copy) | copies the caller's **int var 10007** (ctx +0x34) into the member's main context, not float var 10019. Distance: 3D `Vec3_LengthSq` < 4096 strict (z is 0 in the data, so 2D is equivalent) | 07 §12 row 16 | **CODE BUG**, reachable: ecldata6 sub73 / ecldata6sp sub61 → sub77 `bul_fan col=$10007` (wrong bullet colour on marked children) |
| src/eclvm.rs:1344 (+0x2CA0 of a non-running enemy) | already in the spec: outside its own `Ecl_Run` it is always the main context | 07 §4.3 +0x2CA0 row, §12 row 8 | right |
| src/eclvm.rs:1364 (native 8 caller outside the ring) | 10037 is written only for members, so a caller outside the ring keeps its old value. With k = 0 and an old 10037 ≠ 0 the original divides by 0 and reads an uninitialised leader (UB); doing nothing is an acceptable stand-in | 07 §12 row 8 | right (UB case unreachable) |
| src/eclvm.rs:1415 (g_Gui +0xC) | already in the spec: written only by `Spell_Start` := boss e+0x2E0C (slot index) | 07 §11.1 | right |
| src/eclvm.rs:1439 (bit 9 at Spell_Start) | already in the spec: bit 9 := 0 is `Spell_Start`'s first statement | 07 §11.1 | right |
| src/eclvm.rs:1851 (op 86 empty boss) | already in the spec (no existence test, null enemy). Added: global-only ids are read and written normally; enemy/context ids raise an access violation | 07 §7 op 86 | right in effect (unreachable, 0 uses; skipping stands in for the crash) |
| src/enemy.rs:878 (homing pick with the gated call) | the pick runs after both damage calls inside the bit-6 branch, independent of the gate, so a gated enemy is still offered. `Player_DamageAgainst` does not read the target, so the order is immaterial | 07 §10.1 step 6 | right |
| src/enemy.rs:1635 (capture ring alpha) | already in the spec: the colour slot lerps RGB only; 0x20 goes into alpha_end +0x27F, alpha_start := current alpha, and the alpha slot is not started | 07 §11.1, 05 §6 | right |
| src/enemy.rs:2530 (trail coordinate A) | already in the spec: A steps at the end of every walked entry, skipped ones included (VERIFIED RE-Q30) | 07 §10.6 | right (F bit 3 unreachable) |

## Evidence
- `Ecl_GetIntArg` 0x41F420: case 10033 at 0x41F5E7 (`call 0x43ED50; call ftol`); `Rng_NextFloat` 0x43ED50 = fild
  qword / fdiv [0x4B5B10].
- `EnemyMgr_Ctor` 0x429E00: `rep stosd` over the manager, then only +0x2D30 := 0xFFFF (0x42A140) in the template.
- Interrupt handler 0x41C88A–0x41C93C: ctx(+0x2CA0).ip := ip + size; push main (unless bit 26) at e+0x2CA4 +
  0x2CEA·0x228; `Ecl_SetSub`(main, table[k]); 0x2CEA += 1 (< 15); pending := −1. `Ecl_SetSub` 0x418450: sub < 0 → return 0.
- `EclNative_24` 0x425040; stats +0x1C writer `Spell_End` (+1), new-game reset (11 §3). ecl_dis: `native 24` only in
  ecldata8 sub84 / ecldata8sp sub38; ecldata8 timeline0 frame 9678 spawns sub84.
- `EclNative_16` 0x424E50 (0x424F14 `fcomp [0x4B4958]` = 4096.0, `test ah, 5; jp` → strict <; 0x424F43 copies +0x34).
- `EclNative_13` 0x424E00 → `Stage_RequestTint` 0x409160 (+0x6468–0x646B only).
- `Spell_ShowNameOnly` 0x415D10 (`push 1; push str; push -1; call 0x415F00`).
- `EnemyMgr_Calc` target pick 0x42D390–0x42D46A (0x18B899C = player +0xE2AA4, 0x18B89B8 = +0xE2AC0), after the
  damage calls; `Player_DamageAgainst` 0x451670 has no +0xE2AA4 access.
- `Spell_End` 0x4161B0 capture path: +0x278 := VM colour dword, +0x27C..0x27F := A0 80 D0 20, +0xB0 := 60, +0xF9 := 3.

## CODE BUG list
1. **native 24** (eclvm.rs:1130): var 10000 must be stats.spells (captures this game); reachable (Extra stage boss gate).
2. **native 16** (eclvm.rs:1291): copy the caller's int var 10007 into the member's int var 10007 (not float 10019);
   reachable (stage 6A/6A-sp Eirin spell, bullet colour).
3. int read of 10033 (eclvm.rs:239): must consume one `Rng_Next32` and return trunc(Rng_Float) (unreachable).
4. interrupt table default 0 / no ignore (eclvm.rs:414, enemy.rs template `interrupts: [-1; 32]`): unregistered index →
   sub 0; entry < 0 → skip/push/depth/pending without changing the main context (unreachable).
5. interrupts in thread contexts (eclvm.rs:360, 07 §5.5) (unreachable).
