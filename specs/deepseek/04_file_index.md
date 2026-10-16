# 04 — File index: what each file in `th08.dat` is for

Scope: the 317 members of `th08.dat` (Touhou 8 v1.00d).  After extraction the final
files live in a flat directory (`decoded/` in this spec dir).  Sizes below are the
**decoded** sizes (raw member size minus the 4-byte `edz?` tag); the byte-exact list
with archive offsets and compressed sizes is in `01_archive.md` §9.

Everything here is tagged VERIFIED (proven by the shipped data, by literals/tables
inside `th08.exe`, or both) or INFERRED (consistent evidence only).

## 0. Overview by extension

| ext | count | total bytes | what it is | confidence |
|-----|------:|------------:|------------|-----------|
| `.anm` | 113 | 102 737 072 | sprite sheets + animation scripts (texture = embedded THTX) | VERIFIED |
| `.jpg` | 37 | 4 343 794 | ending/ending-card/menu artwork (baseline JPEG) | VERIFIED |
| `.wav` | 39 | 2 155 488 | sound effects, RIFF PCM | VERIFIED |
| `.ecl` | 24 | 1 011 064 | enemy behaviour scripts ("ECL") per stage | VERIFIED |
| `.mid` | 22 | 1 010 588 | MIDI arrangements of the BGM (MIDI playback fallback) | VERIFIED |
| `.png` | 2 | 999 748 | title screen + character-select artwork | VERIFIED |
| `.dat` | 33 | 115 844 | `msg*.dat` — stage dialogue/message data | VERIFIED |
| `.std` | 18 | 75 372 | per-stage data (background/timing tables) | INFERRED (loader table VERIFIED) |
| `.end` | 14 | 31 907 | ending scenario text scripts (Shift-JIS with `@` commands) | VERIFIED |
| `.sht` | 8 | 13 760 | player shot/bullet definition per team & shot type | VERIFIED (loader refs) |
| `.rpy` | 4 | 12 547 | bundled demo replays (`T8RP` format) | VERIFIED |
| `.txt` | 1 | 7 427 | `musiccmt.txt` — Music Room titles + comments (Shift-JIS) | VERIFIED |
| `.fmt` | 1 | 1 109 | `thbgm.fmt` — BGM track table (see `02_bgm.md`) | VERIFIED |
| `.ver` | 1 | 356 | `th08_0100d.ver` — game version/hash list checked at startup | VERIFIED |

Naming: all members are flat basenames; the engine's *requests* carry virtual
directories (`bgm/`, `title/`, `sprt/`, `result/`, `demo/`) that the loader strips
(see `01_archive.md` §8).

## 1. `.anm` — animation/texture containers (113)

One `.anm` file = 1..n "chunks"; each chunk has a sprite table, a script table,
a chunk name and usually an **embedded THTX texture**.  Exact layout: `03_images.md`.

Groups (VERIFIED by the 113 file names; usage confirmed by `th08.exe` literals
at 0x4B42AA–0x4B8F54 and the tables in §4):

| pattern | files | purpose |
|---------|-------|---------|
| `ascii.anm`, `text.anm` | 2 | bitmap fonts (ASCII / dialogue), loaded by the text & GUI system |
| `face_XX00.anm`, `face_XXsp.anm` | 37 | character portrait sets for dialogue; `XX` = character code, `sp` = "spell"/alternate set |
| `stg{N}bg.anm` (N=1..8, `stg4abg` covers 4A+4B) | 9 | stage background sprite sheets |
| `stg{N}enm.anm`, `stg{N}txt.anm` | 9+9 | stage-specific enemy sprites / stage banners+text |
| `stgenm_XX.anm` (al,rm,sk,ym,yy,yk) | 6 | per-player-character enemy sprites (player-specific midbosses) |
| `eff01..eff09{yy,sk,ym,al,rm,yk}.anm` | 15 | shared effect sheets (explosions, spell circle, …) |
| `enemy.anm`, `etama.anm` | 2 | shared enemy sprites, "etama" = bullet sprite sheet |
| `player00..03.anm` | 4 | player character sprite sheets (team order 0..3) |
| `loading00..03{h,a}.anm` | 12 | loading screens (stage-specific, `h`/`a` variants) |
| `nowloading.anm`, `front.anm`, `times.anm` | 3 | now-loading text, front end, HUD/timing display |
| `title01.anm`, `music00.anm`, `result00.anm`, `resulttext.anm`, `staff01.anm`, `capture.anm` | 6 | title, music room, results, staff roll, spell-capture screens |

Character codes used across the archive: `rm`=Reimu, `mr`=Marisa, `sk`=Sakuya,
`rs`=Remilia, `yy`=Yuyuko, `ym`=Youmu, `yk`=Yukari, `al`=Alice, `st0N`=stage boss
(`face_st01`…), `cdbg`=music-room debug. (Codes VERIFIED from the file list; the
character mapping is INFERRED from the game, the stage→boss order is well known.)

## 2. `.ecl` — enemy scripts (24)

Loader references are a flat string table in the exe (0x4B545C–0x4B55CC).
Two parallel families, 9 stage slots each (stages 1,2,3,4a,4b,5,6,7,8):

| pattern | count | purpose |
|---------|------:|---------|
| `ecldata{N}.ecl` | 9 | main enemy script for stage slot N (VERIFIED: table 0x4C73CC) |
| `ecldata{N}sp.ecl` | 9 | second script for the same slot — stage-practice / spell variant (INFERRED: table 0x4C73F0) |
| `ecldata_XX.ecl` (al,rm,sk,ym,yy,yk) | 6 | per-player-character script (Last-Word / rival battle scripts) (INFERRED) |

The `.ecl` container itself (VERIFIED for all 24 files): `u32 0x00000800` magic, then
`u16 n_sub` (43..78) and `u16 n_sec`, then `n_sec` increasing u32 section offsets,
then one more u32 equal to the file size. Opcode semantics belong to Phase 2.

## 3. `.std` — per-stage data (18)

`stage{N}.std` + `stage{N}_s.std`, 9+9 files. Both variants share an identical
48-byte header prefix (verified by byte comparison of all pairs); the `_s` body is
smaller (e.g. stage1: 4428 vs 3848). Header of `stage1.std`
(first u32 words): `{u16 3, u16 46, u32 2564, u32 3508, u32 0, u32 28004, …}` — the
first u16 differs per stage (3,5,2,…) and the second is 4 or 46/47. Interpretation
of the fields is Phase 2 (stage/background subsystem); at minimum this is a
stage-specific table loaded together with `stg{N}bg/enm/txt.anm` (VERIFIED: both
`stageN.std` and `stageN_s.std` live in the same loader table, §4).
The `_s` variant is the *stage-practice* data set (INFERRED).

## 4. Resource-name tables inside `th08.exe` (VERIFIED)

The loader keeps parallel arrays of file-name pointers in `.rdata`; the following
were read directly from the shipped exe. They are the authoritative name→purpose
mapping (and tell the implementer the per-stage index order:
**stage index 0..8 = stages 1,2,3,4A,4B,5,6,7,8**).

| address | arrays (4-byte pointers, row length in entries) |
|---------|--------------------------------------------------|
| 0x4C72F8 | `stg1bg` … `stg8bg` (9). Note the 4a and 4b slots both point at `stg4abg.anm` |
| 0x4C731C | `stage1.std` … `stage8.std` (9) |
| 0x4C7340 | `stage1_s.std` … `stage8_s.std` (9) |
| 0x4C7364 | `stg1enm.anm` … `stg8enm.anm` (9) |
| 0x4C7388 | 11 mixed `stg*enm.anm` entries (order not yet explained; used as a second enemy-set list) — INFERRED |
| 0x4C73B4 | `stgenm_al/rm/sk/ym/yy/yk.anm` (6) |
| 0x4C73CC | `ecldata1..8.ecl` (9, incl. 4a/4b) |
| 0x4C73F0 | `ecldata1sp..8sp.ecl` (9) |
| 0x4C7418 | 11 mixed `ecldata*sp` entries (second list, stage-4 routes) |
| 0x4C7444 | `ecldata_al/rm/sk/ym/yy/yk.ecl` (6) |
| 0x4C745C | `stg1txt.anm` … `stg8txt.anm` (9) |
| 0x4C7480 | `eff01..eff09` effect sheets (15) |
| 0x4C74C0 | `msg*.dat` dialogue table, see §5 |
| 0x4C7678 | MIDI table, 20-byte records `{char* name; u32; u32; u32 track; u32}`; 22 records where `track` is the Music-Room track number (15 records shown in the dump + terminator) |
| 0x4C7948 | D3D format descriptor table `{u32 d3dfmt; u32 bytes; u32 maskA; u32 maskR; u32 maskG; u32 maskB}` for ids 0x16 X8R8G8B8, 0x15 A8R8G8B8, 0x18 X1R5G5B5, 0x17 R5G6B5, 0x19 A1R5G5B5, 0x1a A4R4G4B4, terminated by 0xFFFFFFFF — used to convert pixels into the destination surface format (see `03_images.md`) |

Functions: the stage arrays are consumed by `FUN_00409850` (stage resource loader);
the dialogue table by `FUN_004390EE` (game/scene object that also loads
`front.anm`, `times.anm`); the MIDI table by `FUN_0043ABD7` (audio).

## 5. `msg*.dat` — dialogue (33)

Naming: `msg{N}{letters}.dat`, e.g. `msg1a`…`msg1d`; stage 4 has route files
`msg4ab msg4ac msg4ba msg4bd msg4dm`. The exe's selection table (0x4C74C0) has, per
stage, a row of 4 pointers followed by a row of 8 (the 8 always looks like
`a,a,b,b,c,c,d,d`), and stage 4 has 4+8+12 entries (routes A and B). `dm` is a
"default" entry that is reused for several variants (VERIFIED: table contents).
File content: 20-byte header
(`{u32 2, u32 12, u32 0x510, …}` for msg1a), then a sequence of records that begin
with the u16 value `0x003C` ('<') and contain Shift-JIS-like text data.
The exact text encoding/opcodes are **not** decoded yet (Phase 2 — message
subsystem); the header/0x3C record markers are VERIFIED observations only.

## 6. `.sht` — player shot data (8)

`ply00a..ply03a.sht` + `ply00as..ply03as.sht` (VERIFIED string table 0x4B6F74+).
`ply<team><shottype>.sht`; the `s` suffix is the alternate (focused/`S`-type) data
set — INFERRED. Content: header `{u16 0, u16 6, u16 0, u16 0x4040, …}` followed by
mixed int/float tables (speeds, damage, sprite indices). Structure is Phase 2
(player subsystem); the file's role is VERIFIED from the loader strings.

## 7. `.wav`, `.mid`, `.fmt`, `.txt` — audio

| ext | notes |
|-----|-------|
| `.wav` (39) | `se_*.wav` sound effects, plain RIFF PCM; loaded by name literal table 0x4B7EA0–0x4B80CC (VERIFIED) |
| `.mid` (22) | `th08_00.mid`…`th08_20.mid` + `init.mid`: MIDI versions of the BGM used when the game runs in "MIDI" sound mode instead of streaming `thbgm.dat` (VERIFIED: `bgm/init.mid`, `bgm/th08_01.mid` literals; see `02_bgm.md`) |
| `.fmt` (1) | `thbgm.fmt`, the track table for `thbgm.dat` (`02_bgm.md`) |
| `.txt` (1) | `musiccmt.txt`, Music Room title + comment text, Shift-JIS, requested as `sprt/musiccmt.txt` |

## 8. `.jpg`, `.png` — artwork outside ANM

| file(s) | purpose |
|---------|---------|
| `end{NN}{variant}.jpg`, `end{NN}[abc]{0,1,2}.jpg` | ending / ending-card illustrations, 640x480 baseline JPEG |
| `end{NN}.jpg` (no variant) | extra ending art, 640x480 |
| `title00.png`, `select00.png` | title screen and character-select background, 640x480 8-bit PNG |
| `title/th08logo.jpg` → `th08logo.jpg` | game logo used on the title screen, 640x480 |
| `music.jpg`, `result.jpg`, `phantasm.jpg`, `info00..02.jpg` | Music Room, Results, Extra/Phantasm, info-screen backgrounds, 640x480 |
| `staff00.jpg`, `staff00b.jpg` | staff-roll artwork, 640x720 (the only non-640x480 JPEGs) |

Dimensions VERIFIED by parsing SOF/IHDR from all 39 images (35 JPEGs at 640x480,
2 at 640x720, 2 PNGs at 640x480).


## 9. `.end` — ending scenario scripts (14)

`end{NN}{a,b,c}.end`. Plain Shift-JIS text with line-oriented `@` commands, e.g.

```
@mbgm/th08_16.mid
@s70 5
@bdata/end/end00.jpg
@230000
@w120 120
@2180
@c15790320
　　<Shift-JIS text line>
@r1200 4
```

So `@b` = background image, `@mbgm` = music, `@w` = wait, `@r` = "read/advance"
pause; numbers are arguments/millisecond-ish waits (VERIFIED: literal text).
These are *scenario data*, not code.

## 10. `.rpy` — bundled demos (4)

`demorpy0..3.rpy`, magic `T8RP` (also seen as `T8RP` in `FUN_0046E136`, which
references `./replay/th8_%.2d.rpy` and `demo/demorpy0.rpy`) — the four "Demo"
replays shown from the title menu (VERIFIED magic + loader literal).
Playback format itself is a later phase (replay subsystem).

## 11. `.ver` — version file (1)

`th08_0100d.ver` (356 bytes, plain ASCII): lines of `<code> <number> <number>`,
e.g. `0009_ 566784 993132091`, `0009a 596992 489076697`, `0010_ 596992 3478009099`.
The loader requests `th08_%.4x%c.ver` (`FUN_004461A0`) — i.e. this file is
`th08_0100` + `d` + `.ver` — and compares entries, presumably as a
file-integrity/version check (VERIFIED file name pattern; purpose INFERRED).

## 12. Open questions

- `.std` field meanings (Phase 2: background/stage subsystem).
- `msg*.dat` text encoding + the per-stage/per-team variant index math (Phase 2).
- The second `stg*enm` / `ecldata*sp` lists at 0x4C7388 / 0x4C7418 (11 entries each;
  probably "extra"/route-specific selections).
- `.sht` internals (Phase 2: player subsystem).
- `capture.anm` and `times.anm` chunk contents (GUI).
