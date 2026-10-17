# 16 — Endings, staff roll, result screen, music room, spell practice

Tools:
- `tools/enddump.py`: ending/staff `.end` scripts. 14/14 files, 0 problems.
- `tools/musiccmt.py`: `musiccmt.txt`. 21 entries, 0 problems.

Score-file structures are in 15 §2.

---

## 1. Ending selection — VERIFIED

### 1.1 How the game reaches an ending (scene 9)

| trigger | code | result |
|---|---|---|
| Final stage (6A, 6B) cleared, not practice, not replay, difficulty < 4 | GameMgr_Calc stage-clear path | flags bit 4 set (good ending), next scene 9 |
| Extra cleared | same path, difficulty 4 | CLRD Extra bits (15 §2.4), clears++, flags bit 4, scene 6 (result); **no ending script** |
| Stage 1–5 cleared while the clock (stats+0x28) > 11 | same path | flags bit 4 cleared (bad ending), scene 9 |
| Game over, not spell practice, difficulty < 4, and (clock > 10 or stage is 6B) | `ContinueMenu_Update` 0x404890 | no continue offered: flags bit 4 cleared, scene 9 |
| Game over in practice | ContinueMenu | scene 6 |
| Game over in replay playback | ContinueMenu | scene 7 |

Supervisor scene 9 calls `Ending_Register` 0x4296A0.

### 1.2 Ending object and file choice (`Ending_Init` 0x429980)

`Ending_Register` 0x4296A0 creates the 0x2AB8-byte object:
- calc 0x429860 at prio 5;
- draw 0x4298F0 at prio 4;
- on-remove 0x429D40.

`Ending_Init` does the following:
1. Clears some AnmManager state and sets 0x17CE8D0 = 1 (the HUD stops drawing).
2. Loads `staff01.anm` into ANM slot 0x18 (8 scripts, 13 sprites).
3. Updates the clear records according to the ending type:

   **Bad ending (flags bit 4 = 0):**
   - skip-allowed = CLRD[shot].+0x20, then CLRD[shot].+0x20 = 0;
   - music flag 18 is set.

   **Good ending:**
   - bit = 0x4000 when the final stage was 6B (stage 7), otherwise 0x8000;
   - clears counter +1 (PLST [diff][14] and total);
   - "skip on the next file" (+0x2A5C) = 1 when the ending was already unlocked before this run: 6B and `Clear_ExtraUnlocked`, or 6A and `Clear_AnyFinalA`;
   - no continues used: skip-allowed = this shot's no-continue[E..L] & bit (any difficulty); then no-continue[diff] |= bit for this shot and for entry 12;
   - continues used: skip-allowed = this shot's any-clear[E..L] & bit;
   - in both cases any-clear[diff] |= bit (this shot and entry 12);
   - music flags 18 and 19 are set.

4. Sets up the 15 text VMs, i = 0..14: script 0x12 + i of `text.anm` (0x17CE8F4), position (64, 16·i + 400).

5. Picks the script by `shot` (0..11). Solo shots use their team's file: shots 4,5 → 00, 6,7 → 01, 8,9 → 02, 10,11 → 03.

   | case | table | files |
   |---|---|---|
   | bad (flags bit 4 = 0) | 0x4C7048 | `end00a.end` … `end03a.end` |
   | good, final stage 6B | 0x4C70A8 | `end0Nc.end` |
   | good, 6A | 0x4C7078 | `end0Nb.end` |

6. Loads it with `Ending_LoadScript` 0x4295E0:
   - reads the file from the archive;
   - script pointer = file start;
   - per-line wait = 8;
   - timers +0x2A70 and +0x2A64 reset;
   - the previous file buffer is freed.

The CLRD+0x20 flag is read and cleared here, but no writer was found. INFERRED: always 0 in retail, so it does not matter in practice.

**Skip-allowed** (+0x2A58) enables fast-forward with Ctrl. When it is set and Ctrl is held, the calc runs the interpreter and the 15 VMs up to 9 times per frame. Ctrl also releases waits (§1.4). In effect, an ending already seen can be skipped.

### 1.3 Ending script format (`.end`) — VERIFIED (interpreter 0x428B80)

The file is Shift-JIS text.
- A line beginning with `@` is a command. Its arguments are separated and ended by **NUL bytes** (`@w120\0120\0\n`).
- Any other line is text. It ends with `\0\n` (or just `\n`).
- Integer arguments are read with `atol`; the reader then skips to the next NUL and over any NULs.
- After a command, the reader skips to the end of the line and over all CR/LF.

Text line handling:
- Bytes are copied **two at a time** into a 0x44-byte buffer until a byte 0, LF or CR is reached at an even position.
- The line is drawn with `Text_RenderToVm(text VM[line], colour, −1, text)`, followed by interrupt 1 on that VM. Then line index += 1.
- The wait timer is set to per-line-normal (+0x2A90). If Shot or Enter is held, it is set to per-line-fast (+0x2A94) instead. The min-wait is per-line-fast.

All retail lines have an even byte length; `enddump.py` checks this.

| cmd | args | effect |
|---|---|---|
| `@0` n | frames | fade-in from black: overlay ARGB = (255 − t·255/n) << 24; then off |
| `@1` n | | fade-out to black: alpha t·255/n, then holds 0xFF000000 |
| `@2` n | | fade-in from white (alpha as `@0`, RGB = FFFFFF); then off |
| `@3` n | | fade-out to white; holds 0xFFFFFFFF |
| `@F` file | name | load another script (`Ending_LoadScript`). On success, skip-allowed = +0x2A5C, text buffer reset. **Falls through into `@R`**, and then the normal "skip to end of line" runs on the **new** file, so the new file's first line is always skipped. Both staff files start with a dummy `@R`. |
| `@R` | — | sets i16 VM+0x21A (script index) = 0 on all 16 VMs |
| `@M` n | | `Music_Fade(n)` (seconds as float) |
| `@V` a b | | scroll speed +0x10 = a / b (float) |
| `@a` vm script sprite | | on VM[vm]: set script `script` of `staff01.anm`, then set sprite `sprite` |
| `@b` file | | load the picture into surface slot 0 (`Anm_LoadSurface`) |
| `@c` n | decimal 0xRRGGBB | text colour +0x2AA0 |
| `@m` file | | `Music_Play(0, file)` then `Music_PlaySlot(0, 0)` (the latter also marks music flag 0 heard) |
| `@r` a b | | page wait: page timer +0x2A7C = a, page min-wait +0x2A88 = b. Line wait reset to 0. Ends this frame. |
| `@s` a b | | per-line wait normal +0x2A90 = a, fast/min +0x2A94 = b |
| `@v` n | | picture y offset +0xC = n |
| `@w` a b | | wait: line timer +0x2A70 = a, min-wait +0x2A8C = b. Ends this frame. |
| `@z` | — | end: the interpreter returns −1, the calc returns 0 and the object is removed |
| other `@x` | | ignored (skip line) |

Script names in the files carry a `data/` or `data/end/` prefix. The archive lookup ignores directories (01).

### 1.4 Per-frame interpreter order (0x428B80)

1. **Page timer** (+0x2A7C), if running:
   - decrement it;
   - if the page min-wait == 0: a Shot or Enter press (edge, mask 0x1001), or Ctrl held while skip-allowed, sets the timer to 0;
   - otherwise page min-wait −= 1;
   - if the timer is still > 0, go to step 4;
   - otherwise send interrupt 2 to the 15 text VMs (clears the page) and set line index = 0.
2. If the line timer (+0x2A70) is 0, run commands and text until something waits (text line, `@r`, `@w`) or the script ends.
3. Otherwise decrement the line timer, with the same release rule using +0x2A8C.
4. Tick +0x2A64 (frame counter).
5. Scroll: y offset (+0xC) −= speed (+0x10). If y ≤ 0, both are set to 0.

### 1.5 Drawing (0x4298F0, draw prio 4)

1. Copy surface slot 0 (the `@b` picture) to the back buffer at (0, 0). The source rectangle is (+8, +0xC), size 640×480. +8 is always 0, so the picture scrolls by moving the source y offset.
2. Draw the 15 text VMs.
3. Fade overlay (0x428910), steps once per draw call:
   - modes 1–4 as in §1.3, with counter +0x2AA8 and duration +0x2AAC;
   - when alpha ≠ 0, draw a full-screen 640×480 untextured quad with colour +0x2AA4 (0x45B1E0).
   - INFERRED: the colour is passed via the rectangle struct; it was verified for the info screen in §4.

### 1.6 Remove (0x429D40)

- Releases ANM slot 0x18 and surface 0.
- Frees the script.
- Sets 0x17CE8D0 = 0.
- **Next scene = 6** (result, mode 1 → name entry).

### 1.7 Staff roll — VERIFIED from data

Good endings end with `@Fdata/staff00b.end` (6A, `b` files) or `@Fdata/staff00.end` (6B, `c` files). Bad endings have no staff roll.

The staff scripts:
- load `staff00b.jpg` / `staff00.jpg` (a tall picture);
- set `@v240`, then `@V0 3100` (no scroll yet);
- start `bgm/th08_17.mid`;
- after about 1900 frames, `@V240 1100` scrolls the picture up by 240 px over 1100 frames;
- show the credits with 11 or 13 `@a vm script sprite` commands: VMs 0..2, scripts 0..7, sprites 0..12 of `staff01.anm`, separated by `@w` waits.

Endings use `bgm/th08_16.mid` (music-room entry 18, 月見草). The staff roll uses `th08_17` (entry 19).

Every ending text ends with an ending-number line, for example `ＢＡＤ　ＥＮＤＩＮＧ　Ｎｏ．９` and `ＮＯＲＭＡＬ　ＥＮＤＩＮＧ　Ｎｏ．５`.

---

## 2. Result / score registration — VERIFIED

`Result_Register(mode)` 0x4582A0 (15 §2.2) creates a 0x477B0-byte object:
- calc `Result_Calc` 0x4584B0 at prio 16;
- draw 0x4586B4 at prio 18;
- on-register 0x45964D: loads `result/result.jpg`, `result00.anm` (slot 0x15) and `resulttext.anm` (0x16), builds the ranking lists;
- on-remove 0x459FD2: saves score.dat.

Each frame `Result_Calc` runs the current state, then the 72 VMs, then frame counter (+4) += 1. `Result_SetState(s)` 0x4550B7 sets prev = state, state = s, and zeroes the sub-counters and the frame counter.

### 2.1 After a game (mode 1)

| state | screen |
|---|---|
| 9 | normal game: name entry (§2.2) |
| 0x11 | stage practice: store the practice hiscore (15 §2.6); goes to 10 |
| 0x16 | spell practice: goes to 10 |
| 0xF | VM 71 (interrupt 18) shows the end banner. After 90 frames, Shot or Enter → interrupt 2, state 0x10. |
| 0x10 | after 30 frames → state 10 with frame counter 9 |
| 10–14 | replay save (15 §3.6) |
| 2 | at frame 1, begin the loading screen and send interrupt 2 to all VMs. After 20 frames, **scene 1 (title/main menu)**, return 0 (object removed → score.dat saved). |

In slow mode (or Supervisor+0x1A4 bit 3), state 9 skips the entry: it restores the CATK backup and goes straight to 0xF.

### 2.2 Name entry (`Result_NameEntry` 0x45621E)

**Frame 0** builds the entry from the in-game record (15 §2.3):
- play time = 0x164D09C;
- human rate = human·10000/total;
- score = stats+8, continues = stats+0x29;
- version 4, size 0x168, magic "HSCR", shot = `shot + byte 0x164D0B2`;
- stage = g_StageIdx, or 99 when flags bit 4 is set;
- name = LSNM (default 8 spaces);
- date = "%m/%d" (0x456938);
- slowdown % (15 §2.11).

It then inserts the entry into list [diff][shot] (`ScoreList_Insert`):
- If the rank > 9, it goes straight to state 0xF (no entry).
- Otherwise the write position = 0. The cursor starts on 0x5F (END) if an LSNM chunk was found in the file, else on 0 ('A').

Input is accepted from frame 10. Every direction accepts an edge press or key repeat (0x164D538).

| key | effect |
|---|---|
| Up / Down | cursor ∓/± 16, wrapping mod 96; skips cells that hold a space; SE 12 |
| Left / Right | cursor ∓/± 1, wrapping inside the 16-cell row; skips spaces; SE 12 |
| Shot / Enter, cursor < 0x5E | name[min(pos,7)] = char; if pos < 8, pos += 1; when pos becomes 8 the cursor jumps to END; SE 10 |
| Shot / Enter, cursor 0x5E ("SP") | writes a space, same as above |
| Shot / Enter, cursor 0x5F ("END") | finish (SE 11) |
| Bomb / ESC (0x0A) | if pos > 0: name[min(pos,7)] = ' ', pos −= 1, name[pos] = ' '; SE 11 |
| ESC (edge) | also finishes |

Finishing:
- state 0xF;
- all VMs get interrupt 2;
- the name is copied to Result+0x58 (later used as the replay name) and to LSNM.

The entry is edited in place inside the list, so the ranking already shows it during entry.

Charset table 0x4C7F48 → `"ABCDEFGHIJKLMNOPQRSTUVWXYZ.,:;_@abcdefghijklmnopqrstuvwxyz+-/*=%0123456789#!?'\"$(){}[]<>&\\|~^ --"` (96 cells: 6 rows × 16).
- Cells 0x5C (space) and 0x5D (space) can never be selected.
- Cells 0x5E and 0x5F are drawn with the ASCII glyphs 0x80 ("SP") and 0x81 ("END").
- The grid is drawn at (x0 = 0x4B4554, y0 = 0x4B7C40), 20 px per column and 18 px per row.
- The selected cell pulses: scale oscillates over 64 frames, with a matching offset to keep it centred.

### 2.3 Ranking display (Result_Draw 0x4586B4, states 5 and 9)

Ten rows from the list [diff][shot]. Each row:
- `%2d` rank;
- name (`%8s`). During entry, the editing row also overlays a `%8s` string of spaces with `_` at position min(pos,7);
- `"%8s %9d%1d(%d)"`: name, score ÷10, **last digit = continues (capped at 9)**, and the stage number from table 0x4C7F80 = {1,2,3,4,4,5,6,6,1}[stage];
- or, when stage = 99, `"%8s %9d%1d(C)"`;
- then `" %5s   %3.2f"` with the date and slowdown %.

Rows are 18 px apart. In state 9 the new entry is drawn in a different colour (two colour codes via 0x4398E8).

### 2.4 Result viewer from the main menu (mode 0)

| state | screen |
|---|---|
| 1 (0x454CB2) | 4 items: 0 Score → 3, 1 Spell Card → 6, 2 Play data → 0x13, 3 Exit → 2. Bomb/ESC moves to item 3 (or exits if already there). **D** writes `score.txt` (0x454298, a text dump of rankings, card stats and play stats). |
| 3 (`Result_CheatUnlock` 0x4550FC) | difficulty select (and the hidden code, 15 §2.12) → 4 |
| 4 | character select → 5 |
| 5 | ranking |
| 6 → 7 → 8 | spell card history (character / page / list `No.%.2d`, `%3d/%3d(%s)`, `MaxBonus %8d`) |
| 0x13–0x15 (0x4578AA) | play statistics table `"%s %6d %6d %6d %6d %6d %6d"` (E, N, H, L, Extra, Total) from PLST |

Confirm SE is 10, cancel SE is 11.

---

## 3. "All Clear" and return to title

- The in-game "All Clear!" panel is 12 (yellow text, final stages).
- In the ranking, a cleared game shows `(C)` instead of a stage number (§2.3).
- After the result states, state 2 sends the game back to the title menu (scene 1). Main menu entry may then show an unlock notice (§4).

---

## 4. Unlock notices — VERIFIED (MainMenu_OnRegister 0x470A6C)

At every main-menu registration, before score.dat is reloaded, the game checks the in-memory CLRD. Each condition that is true while its GM flag bit is still clear shows a notice:

| order | condition | flag bit | picture | unlocks |
|---|---|---|---|---|
| 1 | `Clear_ExtraUnlocked` (some team has a no-continue Final B clear) | 15 | `title/info00.jpg` | Extra Start (top menu item greyed 0xFF404040 otherwise) |
| 2 | `Clear_AnyFinalA` (some team reached the Final A ending) | 16 | `title/info02.jpg` | Spell Practice (top menu item greyed otherwise) |
| 3 | `Clear_AllTeamsFinalB` (all four teams no-continue Final B) | 17 | `title/info01.jpg` | solo characters (12 choices instead of 4) |

The flags are then set again from the reloaded score.dat. So each notice appears once, right after the run that unlocked it. At the first menu after boot the in-memory CLRD is still empty, so nothing is shown. `Supervisor_OnRegister` also checks condition 3 at boot, but it is always false there.

`Info_Show(file)` 0x471181 is a **blocking loop** with its own BeginScene/Present and input polling, for up to 6000 frames. Per frame t:
- draw the picture full-screen;
- for t < 20: black quad with alpha byte = trunc((60 − t)·255/20) & 0xFF (QUIRK: starts at 253 and falls to 23, then vanishes);
- for t > 5940: alpha byte = trunc((t − 5980)·255/20) & 0xFF (QUIRK: wraps for t < 5980);
- from t ≥ 180, a press of Shot, Bomb or Enter (edge, 0x1003) jumps to t = 5980 and plays SE 10;
- `Sound_Update` runs each frame.

After the loop, surface 0 is released.

---

## 5. Music room — VERIFIED (`MusicRoom_Register` 0x449254, scene 8)

The object is static at 0x17CF4C8 (0x6A28 bytes):
- calc 0x449399 at prio 4;
- draw 0x449487 at prio 3;
- on-register `MusicRoom_Init` 0x44970F;
- on-remove 0x449C0A.

### 5.1 Data

Init loads `result/music.jpg` (surface 0), `music00.anm` (slot 0x17; VM +0x88 = script 0) and `sprt/musiccmt.txt`.

`musiccmt.txt` parsing:
- skip bytes until `@`;
- `@` + file name up to CR/LF → entry +0x00;
- next line = title → +0x40;
- up to 7 more lines, until the next `@` → comment k at +0x82 + k·0x42.

CR/LF runs are skipped after each line, so blank lines are dropped. Copies are unbounded; each comment slot is zeroed (0x42 bytes) just before it is filled.

The entry array is 32 × 0x292. The retail file has **21 entries**:

| # | file | title |
|---|---|---|
| 0 | th08_01 | 永夜抄　～ Eastern Night. |
| 1 | th08_00 | 幻視の夜　～ Ghostly Eyes |
| 2 | th08_03 | 蠢々秋月　～ Mooned Insect |
| 3 | th08_04 | 夜雀の歌声　～ Night Bird |
| 4 | th08_05 | もう歌しか聞こえない |
| 5 | th08_06 | 懐かしき東方の血　～ Old World |
| 6 | th08_07 | プレインエイジア |
| 7 | th08_08 | 永夜の報い　～ Imperishable Night. |
| 8 | th08_09 | 少女綺想曲　～ Dream Battle |
| 9 | th08_10 | 恋色マスタースパーク |
| 10 | th08_11 | シンデレラケージ　～ Kagome-Kagome |
| 11 | th08_12 | 狂気の瞳　～ Invisible Full Moon |
| 12 | th08_13 | ヴォヤージュ1969 |
| 13 | th08_14 | 千年幻想郷　～ History of the Moon |
| 14 | th08_15 | 竹取飛翔　～ Lunatic Princess |
| 15 | th08_13b | ヴォヤージュ1970 |
| 16 | th08_18 | エクステンドアッシュ　～ 蓬莱人 |
| 17 | th08_19 | 月まで届け、不死の煙 |
| 18 | th08_16 | 月見草 (ending) |
| 19 | th08_17 | Eternal Dream　～ 幽玄の槭樹 (staff roll) |
| 20 | th08_20 | 東方妖怪小町 |

All files are `bgm/…mid`.

The entry index is the "heard" flag index: PLST+0x208[i] (0x164CF14 + i). It is copied to the object (+0xC + 4i) at init.

Comment display copies only **64 bytes** of a line into a zeroed 0x42 buffer. Twenty-three retail lines are 64–68 bytes; the ones over 64 are shown cut. QUIRK; `musiccmt.py` lists them.

### 5.2 Title list

For each entry i, VM (0x32C + i·0x2A4) gets `music00.anm` script i + 1 and flags |= 0x1800. Its text:
- heard: the title, colour 0xC0E0FF, outline 0x302080;
- not heard: `？？？？？？？？`, colour 0x80A0C0, outline 0x100040.

The draw shows 10 rows from top (+0x7C):
- row y = 18·(i − top + 1) + 104 − 20, title x = 93;
- number `%2d.` at x 48;
- the cursor marker is an ASCII string at x 33.

"Now Playing" is printed at (320, 32) and the playing-title VM (+0x6784) at (320, 52).

### 5.3 Calc

| state (+0x70) | behaviour |
|---|---|
| 0 | at sub-frame 0: selected row VM interrupt 1, others 2; the 7 comment VMs interrupt 1. After 8 frames → state 1. |
| 1 | input (below) |

| key | effect |
|---|---|
| Up / Down (edge or repeat) | cursor (+0x74) ∓/± 1, wrapping; the 10-row window follows; rows re-interrupted; comment VMs interrupt 2; sub-frame (+0x6C) = 0 |
| Shot / Enter | playing = cursor. If music mode is WAV (0x449C79), restart the stream from `thbgm.dat`. `Music_PlayFile(entry.file, 0)`; title VM: `text.anm` script 0x11, interrupt 1, text = comment line 0 (colour 0xFFE0C0, outline 0x300000). |
| Ctrl | `Music_Fade(8.0)` |
| R | replay the playing track |
| Bomb / ESC | scene 1 with the loading banner (0x44886F) |

Comment reveal: at sub-frame 10, 12, …, 22, comment line k = 0..6 is drawn on VM 0x5508 + k·0x2A4 (`text.anm` script 10 + k, interrupt 1).
- The text is the real comment if the entry is heard or is the one playing.
- Otherwise it is the warning text from table 0x4C7AAC: "＊＊＊注意＊＊＊ / この曲はまだ、ゲーム中で流れておりません …" (seven lines).
- Empty lines clear VM flag bit 1 (hidden).

`Music_PlayFile(.., 0)` marks flag 0 (title theme) heard, not the chosen track. Playing a track in the music room does **not** unlock it. VERIFIED.

---

## 6. Spell practice and Last Word selection — VERIFIED (replaces the INFERRED parts of 12 §2.6)

### 6.1 Screen 16 (`MainMenu_SpellPracticeChar` 0x46B174)

On entry:
- loads `title/select00.png`;
- flags |= 0x4001 (practice + spell practice);
- cursor shot = current shot, moved forward to the first shot whose team has reached Final A (`Clear_TeamFinalA`; solo shots always pass). There are 12 shots if all teams are cleared (0x447D04), else 4.

Controls:
- Left/Right change the shot, skipping locked ones.
- Up/Down move over 10 stage rows (`Menu_CursorUpDown(10)`): 0..8 = stages 1, 2, 3, 4A, 4B, 5, 6A, 6B, Extra; 9 = Last Word.
- Confirm: g_StageIdx = row, screen 17.
- Bomb/ESC: back to the top menu (cursor 2), flags &= ~1.

Returning from a spell-practice game (+0x6C ≠ 0) jumps straight to screen 17. If the last card id is ≥ 205, the stage row is set to 9.

### 6.2 Screen 17 (`MainMenu_SpellPracticeCard` 0x46BBC0)

The card list for row s comes from table 0x4C6C40[s], counts 0x4C6C68:

| row | count | card ids |
|---|---|---|
| 0 | 13 | 0–12 |
| 1 | 19 | 13–31 |
| 2 | 22 | 32–53 |
| 3 | 23 | 54–76 |
| 4 | 23 | 77–99 |
| 5 | 19 | 100–118 |
| 6 | 28 | 119–146 |
| 7 | 44 | 147–190 |
| 8 | 14 | 191–204 |
| 9 | 17 | 205–221 (Last Words) |

On entry:
- `LastWord_ComputeUnlocks` runs (15 §2.8);
- the cursor is placed on the previously played id (0x164D0B8);
- the list is shown 15 rows per page.

Row text:
- attempted (story attempts[12] or practice attempts[12] ≠ 0): card name;
- otherwise a Last Word unlocked in FLSP (`Card_IsSelectable` 0x46FD5F): `挑戦者歓迎中！`;
- otherwise `？？？？？？？？`.

The selected row is white, others grey (0x60).

Controls:
- Up/Down move the cursor.
- Left/Right jump ±15 (only when there are more than 15 cards).

Confirm is allowed when the card is attempted, or is a Last Word with its FLSP slot set. Otherwise SE 0x29 (buzzer). On a valid confirm:
- flags |= 0x4000;
- 0x164D0B8 = id;
- difficulty = `Spell_DifficultyOf(id)`;
- scene 2, music stops;
- for Last Words (difficulty 5): **difficulty := 1 (Normal)** and the stage is chosen by id:

| id | 205 | 206 | 207 | 208 | 209 | 210 | 211 | 212 | 213 | 214 | 215 | 216–221 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| stage idx | 0 | 1 | 2 | 5 | 6 (6A) | 7 (6B) | 8 (Ex) | 5 | 8 | 3 (4A) | 4 (4B) | 3 (4A) |

Bomb/ESC: remember the id, back to screen 16.

### 6.3 Info panel (0x46D7F9)

- `Ｎｏ．%s　　%s`: number and name. `？？？？？？？？` if never attempted.
- `使用者  %s　　%s %s`: owner, difficulty name (table 0x4C8628), and "Last" when the card is in the last-spell list 0x4C6B90.
- `取得数/挑戦数[最高点]　%s　全主人公合計`, then the values:
  - normal cards: `%3d/%3d(%3d/%3d)[%.8d]` twice (this shot, then total), with practice captures/attempts, story captures/attempts in brackets, and the practice max bonus;
  - Last Words: `%3d/%3d(---/---)[%.8d]`;
  - never attempted: `---/---(---/---)[--------]`.
- `カードの補足` (card notes): the two comment lines (CATK +0x70/+0xB0). They are shown only if practice captures[12] ≠ 0, else `？？？？？？？？`.
- For locked cards with id ≥ 204 that were never attempted, a two-line **unlock hint** comes instead from table 0x4C82C8 (18 records × 0x30: {format ptr, 5 ints} ×2, ints printed +1).

The hints match the conditions in 15 §2.8:

| id | hint |
|---|---|
| 204 | capture ≥ 7 Extra cards and clear Extra |
| 205 / 206 / 208 / 217 | clear Final B with ≥ 2 / 3 / 4 / 6 characters (difficulty irrelevant) |
| 207 / 216 | 50 / 120 cards captured in spell practice (all characters) |
| 209 | capture No.138 |
| 210 / 219 | 15 / 30 last spells captured in practice |
| 211 | capture No.146 and No.196, and see No.205 |
| 212 | see No.209–211 |
| 213 | see No.206, 207, 208, 212 |
| 214 | Marisa (solo) captures every Normal card |
| 215 | Reimu (solo) clears Hard or above, Final B |
| 218 | clear Extra with ≥ 3 characters |
| 220 | clear Lunatic (continues allowed, any character) |
| 221 | see all other Last Words |

Displayed numbers are 1-based (id + 1).

The hints for 205/206/208/217 do not say "no continue", but the code checks the no-continue mask. VERIFIED discrepancy: the code wins.
