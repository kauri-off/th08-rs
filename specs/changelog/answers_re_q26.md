# RE-Q26 — IMPL28 questions (endings, staff roll) and the INFERRED sweep (changelog)

Labels: **SPEC FIXED** (spec was wrong or silent, now corrected), **CODE BUG** (spec right, port differs),
**NEW FACT** (new behaviour added to a numbered spec). Evidence = binary (addresses), the retail `.end` files and a
full replay of `sessions/fullrun_noclip` **with draw log** in the original (2026-10-20: 124 708 trace rows, 0 differ;
output `re/work/runs/q26/fullrun_dl/`, not in git; ending = chunks draw_00067–00069). Scratch: `re/work/runs/q26/`
(`endsim.py` = exact `.end` reader simulation, `dispscan.py` = displacement scan).

## 1. IMPL28

| Q | label | answer | spec § | evidence |
|---|---|---|---|---|
| 1 per-line wait 8 | SPEC FIXED + CODE BUG | only **normal** +0x2A90 := 8 (ctor and every `Ending_LoadScript`); fast/min +0x2A94 starts 0 and keeps the last `@s` value across `@F`. Unreachable with retail data (every file runs `@s` first) | 16 §1.2 (ctor, step 6) | 0x4297B0, 0x4295E0 |
| 2 initial colour | SPEC FIXED + CODE BUG | +0x2AA0 = 0 (black) from the ctor zero-fill; never reset by `@F`. Unreachable (retail sets `@c` first) | 16 §1.2 | 0x4297B0 |
| 3 16 VMs | SPEC FIXED | the ctor builds **16** VMs at +0x14 (array ctor count 16, ends at +0x2A54); VMs 0–14 = text VMs, VM 15 never scripted/run/drawn. `@R`'s +0x21A writes have no observable effect (ending VMs are run by `AnmVm_Run` directly; only range runners read +0x21A). `@a` VMs 0–2 are the text VMs 0–2 | 16 §1.2, §1.3 | 0x4297B0, 0x428B80 case `R`, 0x429860, 0x462310 |
| 4 fade order | SPEC FIXED (port right) | colour from t, then t += 1; fade-in with t ≥ n turns off in that call (nothing drawn); n = 0 → off at once. Runtime: `@2 180` at frame 122 → alpha FF at 122 … 02 at 301, none at 302 | 16 §1.5 | 0x428910; fullrun draw_00067/68 |
| 5 end without `@z` | SPEC FIXED | no end test: reads past the buffer (undefined). All retail files end `@z\n\0\n`; treating the end as `@z` is fine | 16 §1.3 | 0x428B80; endsim.py |
| 6 > 15 lines | SPEC FIXED | no range check: index 15 → invisible VM 15, ≥ 16 overwrites the object's fields. Retail max 4 lines per page. Dropping is fine | 16 §1.3 | endsim.py |
| 7 picture rect | SPEC FIXED (port right) | `CopyRects` with an integer rect, x/y = `ftol` (truncation) of +8/+0xC; no filtering | 16 §1.5 | 0x4298F0, 0x466D70 |
| 8 AnmManager state | SPEC FIXED + CODE BUG | caches +0x24BC := 0, +0x24C8 := 0, +0x24C0 := 3, +0x24C2 := 0xFF, then `Device_ClearPresentTwice(0xFFFFFFFF)`: two extra presents that only clear colour+depth to white | 16 §1.2 step 1 | 0x429980, 0x45B020; fullrun presents 121250/121251 `C tz ffffffff` |
| 9 removal frame | SPEC FIXED (port right) | calc returns 0 → on-remove 0x429D40 unlinks the draw node (object +4, 0x43CF10) inside the calc walk → no ending draw on that frame | 16 §1.6 | 0x429860, 0x429D40, 00 §3.1 |
| 10 text terminator | SPEC FIXED (port equivalent) | after text the reader skips every 0/LF/CR byte; a line starting with 0/LF/CR still sets the line wait and line += 1 (none in retail) | 16 §1.3 | 0x428B80; endsim.py |
| extra | NEW FACT | `@b` / `@F` failure ends the ending (−1); LoadScript resets nothing but the wait-normal and two timers; a screen effect from the last dialogue (MSG op 14) overlaps the first 2 ending frames | 16 §1.2, §1.3, §1.5 | fullrun draw_00067 |
| 11 unlock notices | — | not a question; 16 §4 is already specified | 16 §4 | — |

## 2. INFERRED facts (COVERAGE §A)

Verified or corrected (sync / RNG / draw log / saved files):

| fact | result | spec § | evidence |
|---|---|---|---|
| `ScoreList_Insert` order of equal scores | **CORRECTED**: a new entry goes **before** existing equal scores (strict unsigned `<` walk); equal entries of score.dat come out in reverse file order after a load | 15 §2.3 | 0x45A500 |
| ribbon stack residue "constant" | **CORRECTED**: per-session stale stack values (demo 1 0x910D20/0x676E85, fullrun 0x991C10/0x66B705) | 06 §11.3 | fullrun draw_00030 vs demo1 draw log |
| cfg music default MIDI | VERIFIED + precise: only when defaults are applied and thbgm.dat can't be opened | 02 §1 | 0x4477B1 |
| tex_matrix role, sprite +0x38/+0x3C, load_state | VERIFIED | 05 §2, structs | 0x464470, 0x465CC0, 0x465BA0, 0x45E430 |
| saved_sprite +0x218 | VERIFIED writers/readers | 05 §2 | decompile xrefs |
| Supervisor flag 0x20 reachability | VERIFIED unreachable with retail data (native 29 arg 1) | 05 §12.6 | ecl_dis (ecldata_ym.ecl) |
| tan helper | VERIFIED (`fptan`, CW 0x27F) | 00 §1 (default camera) | 0x4A4110 |
| game-speed users | VERIFIED writers | 00 §2 | globals_xref |
| angle convention | VERIFIED | 00 §5 | 08 §3 |
| viewport MinZ/MaxZ | VERIFIED 0/1 | 06 §10.1 | all `V` records |
| stats word (0x18B8A28)+0xDA, effect bit 0x400 | VERIFIED write-only | 08 §8, 13 §2.2 | dispscan 0xDA, 0x4522A0, 0x452310 |
| orbit-13 geometry | VERIFIED (already 13 §2.6b; table row aligned) | 13 §2 | 0x426030, D3DX 0x4791BC/0x4781CD/0x4792E8 |
| effect type 41 | VERIFIED never spawned | 13 §2 | all spawn sites + ECL data |
| SE priority word, SE-off queue | VERIFIED | 13 §1.1, §1.3 | 0x45D550, 0x45D790 |
| dialogue VM 0x22AB0 | VERIFIED never drawn | 10 §3 | 0x433DB3 refs, msg data ranges |
| op 148 / op 180 HUD meaning | VERIFIED (12 §5.4 / §5.7) | 10, 07 | — |
| spell-plate font bytes 0 | VERIFIED no effect (`Text_Render` maps ≤ 0 → 15) | 12 §5.8 | 0x466310, 0x40B910 |
| trail strip skip test | CORRECTED (θ uses E[i−1]; next midpoint uses fixed E[b]); path unreachable (no flag 8 in data) | 07 §10.6 | 0x42E64E–0x42E72A |
| gauge-fade "HUD side flag" | VERIFIED = 12 §5.9 | 09 §1 | 0x44D420 |
| bomb speed-multiplier order | VERIFIED via 14 §2 | 09 §7 | — |
| retry counters, GameFlags 15–17 | VERIFIED via 15 §2.9 / 16 §4 | 11 | — |
| ANM header +8/+0x20/+0x24/+0x3C, colour key | VERIFIED unused | 03 §2.1 | 0x465CC0, anm.py over 310 entries |
| file roles `_s.std`, `sp.ecl`, `ecldata_xx`, `stgenm_xx`, demorpy, info0N, select00 | VERIFIED; `phantasm.jpg` is unused (no reference in the exe) | 04 | 07 §1, 16 §4, string xrefs |

Left INFERRED (naming / purpose / platform; listed in COVERAGE §A): 00 formats and names, 01 reserved / out_size, 02 MIDI
and slot names, 03 resource priority and reduced-colour name, 04 remaining role names, 07 flag names, 09/14 detail-level
note, 10 GDI pixels, 11 names / slowdown-replay purpose, 12 boot-present cause and VM role names, 13 SE caps bit and unused
SE ids, 15 joystick bits after a failed keyboard read.

## CODE BUG list (next implementer brief)

1. **`ScoreList_Insert`**: new entry before equal scores (`scoredat.rs` `insert_hscr` counts `>=`, must count `>`), also
   when loading score.dat entries in file order (15 §2).
2. **Ending init presents**: two extra presents that only clear colour + depth to 0xFFFFFFFF before the first ending
   frame (16 §1.2 step 1) — affects present numbering / draw-log alignment.
3. Ending ctor/LoadScript: fast/min wait +0x2A94 starts 0 and is not set by LoadScript (port sets 8); initial colour 0
   (port 0xFFFFFF). Unreachable with retail files (16 §1.2).
4. Fade with n = 0: off at once / held at full (port clamps n to 1 and draws one frame). Unreachable (retail n > 0) (16 §1.5).
5. Ribbon residue: not reproducible; keep any constant (06 §11.3) — no change needed.
