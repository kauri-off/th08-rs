# 04 — File index: what each archive file is for

317 files in th08.dat (full list with sizes in 01_archive.md §9). Purposes come from the code that
loads each name (loader functions in `symbols.csv`) and from file-name tables in the exe
(addresses below). **VERIFIED** = name ↔ loader/table link observed in the exe or file content;
**INFERRED** = role deduced from naming or context. Detailed formats of ECL/STD/MSG/SHT/END/RPY are
for later phases; only identification and headers are given here.

## Global indices used by the file tables

- **Stage index** (global 0x164D2CC): 0 = Stage 1, 1 = 2, 2 = 3, 3 = 4A ("uncanny"),
  4 = 4B ("powerful"), 5 = 5, 6 = 6A (Eirin, files `*6*`), 7 = 6B (Kaguya, files `*7*`),
  8 = Extra (files `*8*`) (VERIFIED from tables 0x4C72F8/0x4C731C/0x4C7364/0x4C73CC/0x4C745C;
  names of 4A/4B/6A/6B from replay stage strings `Stage 4-uncanny`, `Stage 4-powerful`, `Stage 6-Eirin`,
  `Stage 6-Kaguya`). The `_s.std` / `*sp.ecl` variants are the table entries 9–17 (table base + 9), indexed by the
  same stage index 0–8 when GameFlags bit 14 (spell practice) is set; the stage index itself never exceeds 8 (07 §1, VERIFIED RE-Q50).
- **Shot type / character index** (12 values): 0–3 teams (`Rm & Yk`, `Ms & Al`, `Sk & Rr`,
  `Ym & Yy`), 4–11 solo characters in pair order (Reimu, Yukari, Marisa, Alice, Sakuya, Remilia, Youmu,
  Yuyuko) (VERIFIED table layout 0x4C7CB0/0x4C7CE0 and 12-entry msg tables; name order INFERRED from the
  string arrays at 0x4B7B68–0x4B7BC0).

## By name pattern

| pattern | count | crypt | purpose | loaded by | conf. |
|---|---|---|---|---|---|
| `ascii.anm` | 1 | A | font / small UI sprites (ANM slot 1) | `AsciiManager_Load` | VERIFIED |
| `capture.anm` | 1 | A | `@` render-target used for screen capture (slot 3) | `AsciiManager_Load` | VERIFIED |
| `text.anm` | 1 | A | `@` 512×256 render target for dynamically drawn text (slot 0) | startup (0x4464C8) | VERIFIED |
| `nowloading.anm` | 1 | A | "now loading" indicator (slot 2) | Supervisor title init 0x445FA3 | VERIFIED |
| `title01.anm`, `title00.png`, `th08logo.jpg` | 3 | A/-/J | title / main menu graphics (slot 20) | MainMenu 0x470E10, Supervisor | VERIFIED |
| `select00.png` | 1 | - | menu selection background, loaded as `title/select00.png` by the menu screens 0x46A071 / 0x46A625 / 0x46B174 / 0x46E136 (12 §2) | those functions | VERIFIED (string xrefs) |
| `front.anm` | 1 | A | HUD frame around the playfield, score panel labels (slot 10) | `Hud_Load` | VERIFIED |
| `times.anm` | 1 | A | HUD numbers/timer sprites (slot 14) | `Hud_Load` | VERIFIED |
| `loading0N.anm`, `loading0Nh.anm`, `loading0Na.anm` (N=0..3) | 12 | A | stage-start loading picture per team (N = team); `h`/`a` variants for solo human / youkai (slot 12) | `Hud_Load` via table 0x4C72C4 | VERIFIED table, variant meaning INFERRED |
| `stgNtxt.anm` (1,2,3,4a,4b,5,6,7,8) | 9 | A | stage title / name cards (slot 13) | `Hud_Load` via 0x4C745C | VERIFIED |
| `stgNbg.anm` (4B reuses `stg4abg`) | 8 | A | 3D background textures (slot 4) | `Stage_Load` via 0x4C72F8 | VERIFIED |
| `stageN.std` | 9 | - | stage background script: 3D geometry, camera script, 3 BGM names at 0x290/0x310/0x390 | `Stage_Load` via 0x4C731C | VERIFIED |
| `stageN_s.std` | 9 | - | spell-practice stage scripts: used when GameFlags bit 14 is set, indexed by the stage index 0–8 (07 §1, 16 §6) | `Stage_Load` via 0x4C7340 (= 0x4C731C + 9 entries) | VERIFIED |
| `enemy.anm` | 1 | A | common enemy sprites (fairies etc.) (slot 7) | `EnemyManager_Load` | VERIFIED |
| `stgNenm.anm` | 9 | A | stage-specific enemy/boss sprites (slot 8) | `EnemyManager_Load` via 0x4C7364 | VERIFIED |
| `stgenm_{sk,ym,al,rm,yy,yk}.anm` | 6 | A | per-character boss sprites for the six character Last Words (card ids 216–221) in spell practice (slot 8; 07 §1) | dword at 0x4C7054 + 4·card (card ≥ 205; 205–215 give `stgNenm.anm`) | VERIFIED |
| `ecldataN.ecl` | 9 | E | enemy/boss behaviour scripts (ECL) per stage | `EnemyManager_Load` via 0x4C73CC | VERIFIED |
| `ecldataNsp.ecl` | 9 | E | spell-practice scripts: GameFlags bit 14 and card id < 205, indexed by the stage index 0–8; also Last Words 205–215 via the card table (07 §1) | 0x4C73F0 (= 0x4C73CC + 9 entries) | VERIFIED |
| `ecldata_{al,rm,sk,yk,ym,yy}.ecl` | 6 | E | Last Word scripts of the six character Last Words (card ids 216–221; card ids ≥ 205 read the dword at 0x4C70E4 + 4·card, 205–215 give an `ecldataNsp.ecl`; pair with `stgenm_xx.anm`) | names at 0x4B545C | VERIFIED (07 §1, RE-Q50) |
| `etama.anm` | 1 | A | bullets (shared bullet sheet, slot 6) | `BulletManager_Load` | VERIFIED |
| `effNN.anm` (01–08, 04a/04b) | 9 | A | per-stage special effects (slot 9) | `EffectManager` via 0x4C7480 | VERIFIED |
| `eff09{sk,ym,al,rm,yy,yk}.anm` | 6 | A | per-character effects (index 9–14 of the same table / per-spell table 0x4C7144) | `EffectManager` | VERIFIED table, role INFERRED |
| `playerNN.anm` (00–03) | 4 | A | player sprites per team (slot 5); solos use their team's file | `Player` load 0x44D650 via 0x4C7CB0 | VERIFIED |
| `plyNNa.sht`, `plyNNas.sht` | 8 | - | player shot definitions; `a` = human-side shot, `as` = youkai-side shot (teams use `a` unfocused / `as` focused) | tables 0x4C7CE0/0x4C7D10/0x4C7D20 | VERIFIED tables, a/as meaning INFERRED |
| `face_{rm,yk,mr,al,sk,rs,ym,yy}00.anm` | 8 | A | player-character dialogue portraits (slots 16/17) | `Gui_LoadFaces` | VERIFIED |
| `face_stNN*.anm`, `face_*sp.anm` | 30 | A | boss / spell-card portraits (slots 18/19); `sp` = spell-card cut-in | `Gui_LoadFaces` | VERIFIED load, `sp` meaning INFERRED |
| `face_cdbg.anm` | 1 | A | spell-card background (slot 15) | `Gui_LoadFaces` | VERIFIED load, role INFERRED |
| `msgN{a,b,c,d}.dat` | 28 | - | dialogue scripts: stage N, letter = team (a = Rm&Yk, b = Ms&Al, c = Sk&Rr, d = Ym&Yy; solos use their team's file) | tables 0x4C74C0… (12 entries per stage) | VERIFIED tables |
| `msg4ab/4ac/4ba/4bd.dat`, `msg4dm.dat` | 5 | - | Stage 4A dialogue for teams b/c, Stage 4B for teams a/d; `4dm` = placeholder for team/route combinations that are not played | same tables | VERIFIED tables, route interpretation INFERRED |
| `end0{0..3}{a,b,c}.end` | 12 | - | ending scripts per team (0–3) and ending type a/b/c; text with `@` commands (`@m` music, `@b` picture, `@w` wait, `@s` …) | Ending 0x4296A0 via 0x4C7054 | VERIFIED |
| `staff00.end`, `staff00b.end` | 2 | - | staff-roll scripts | Ending | VERIFIED names |
| `staff01.anm` | 1 | A | staff-roll graphics (slot 24) | Ending | VERIFIED |
| `end*.jpg`, `staff00*.jpg` | 30 | J | ending / staff-roll pictures referenced from `.end` files (`@bdata/end/end00.jpg`) | `.end` scripts | VERIFIED |
| `info00-02.jpg`, `phantasm.jpg` | 4 | J | `info00-02`: unlock notices shown after the run that unlocked Extra / spell practice / solo characters (16 §4); `phantasm.jpg`: **unused** — its name does not occur in th08.exe (string scan) | 16 §4 | VERIFIED |
| `result00.anm`, `resulttext.anm`, `result.jpg` | 3 | A/J | result / high-score screen (slots 21, 22) | Result 0x45964D | VERIFIED |
| `music00.anm`, `music.jpg`, `musiccmt.txt` | 3 | A/J/T | music room: graphics, background, track titles & comments (Shift-JIS text, `@bgm/…mid` entries) | MusicRoom 0x44970F | VERIFIED |
| `thbgm.fmt` | 1 | - | BGM track table for thbgm.dat | `Bgm_LoadFmt` | VERIFIED |
| `th08_NN.mid`, `init.mid` | 22 | - | MIDI versions of the BGM (used in MIDI music mode); `init.mid` = MIDI reset sequence, loaded into slot 30 at boot (not played there) and played at the 4 sites of 02 §5 | `Music_Play`, startup | VERIFIED |
| `se_*.wav` | 39 | W | sound effects: RIFF PCM (e.g. 22050 Hz mono 8-bit). 36 are named in the exe SFX table (0x4B7EA0…); `se_bonus`, `se_bonus2`, `se_border` are not in that table | sound init | VERIFIED |
| `demorpyN.rpy` (0–3) | 4 | - | attract-mode demo replays; magic `T8RP` | replay system, demo start (11 §4) | VERIFIED |
| `th08_0100d.ver` | 1 | - | text table of known exe versions: `<version> <exe size> <checksum>`; last line `0100d 840704 …` matches th08.exe (840 704 bytes) | 0x4461A0 (`th08_%.4x%c.ver`) | VERIFIED name/size, role INFERRED |

## Files the engine reads outside th08.dat

| file | purpose |
|---|---|
| `thbgm.dat` | streamed PCM music (02_bgm.md) |
| `th08.cfg` | 60-byte config (written on exit) |
| `score.dat` | scores / progress (backups `backup/score_N.*.bak`); format in a later phase |
| `replay/th8_NN.rpy` | saved replays |
| `log.txt` | debug log |
| `snapshot/thNNN.bmp` | screenshots (written) |
