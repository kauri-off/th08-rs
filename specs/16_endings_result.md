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

The CLRD+0x20 flag is read and cleared here. No code ever stores a nonzero value in it (struct-relative scan, changelog/answers_phase7.md §4): it is 0 unless score.dat itself contains a nonzero byte, which is then honoured once and cleared. VERIFIED.

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
   - VERIFIED: the caller passes +0x2AA4 in EDX to `Screen_FillRect` (ECX = rect {0, 0, 640, 480}); all four vertices get that diffuse colour, z 0, rhw 1, FVF XYZRHW|DIFFUSE, triangle strip; colour/alpha = SELECTARG1(DIFFUSE); z-write is turned off unless cfg+0x38 (Supervisor+0x150) bit 6 is set (0x438A41); DESTBLEND = INVSRCALPHA (changelog/answers_phase7.md §7).

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
| 0xF | VM 71 (interrupt 18) shows the end banner (stats panel 16 §2.5). `Result_EndBanner` 0x4577E2 itself writes VM 71's pending interrupt := 18 on **every** state-0xF call while the state frame (+4) ≤ 30 (frames 0–30, so the label-18 entry is re-triggered each of those frames); from frame 90 (> 89), a Shot/Enter edge (0x1001) → VM 71 **only** gets interrupt 2, frame := 0, state 0x10. State 0x10: at frame > 29 → frame := 9, state 10 (VERIFIED RE-Q24). |
| 0x10 | after 30 frames → state 10 with frame counter 9 |
| 10–14 | replay save (15 §3.6, 16 §2.6) |
| 2 | at frame 1, begin the loading screen and send interrupt 2 to all VMs. After 20 frames, **scene 1 (title/main menu)**, return 0 (object removed → score.dat saved). |

In slow mode (or Supervisor+0x1A4 bit 3, set only by the anti-tamper fill), state 9 skips the entry: it restores the CATK backup and goes straight to 0xF.

### 2.2 Name entry (`Result_NameEntry` 0x45621E)

**Frame 0** first sends interrupt **difficulty + 3** (difficulty = 0x160F538, stored in Result+0x44; shot index `shot + byte 0x164D0B2` in Result+0x3C) to **all 72** result00 VMs (Result+0x1A0, stride 0x2A4: pending interrupt +0x1FE := d + 3 — no other interrupt is sent at state 9 entry; VM 40's slide-in and the plate/title VMs react to that interrupt in their own scripts), renders the shot title text into the text VM at +0xBFC0 (`Text_RenderCentered`, white, string table 0x4C7F4C[shot index]) and sets byte +0xC1B3 := 0xFF (VERIFIED 0x45621E, RE-Q24). Then it builds the entry from the in-game record (15 §2.3):
- play time = 0x164D09C;
- human rate = human·10000/total;
- score = stats+8, continues = stats+0x29;
- version 4, size 0x168, magic "HSCR", shot = `shot + byte 0x164D0B2`;
- stage = g_StageIdx, or 99 when flags bit 4 is set;
- name = LSNM (default 8 spaces);
- date = "%m/%d" (0x456938: C `time` + `localtime` + `strftime`, i.e. **local** time, VERIFIED RE-Q25);
- slowdown % (15 §2.11).

It then inserts the entry into list [diff][shot] (`ScoreList_Insert`):
- If the rank > 9, it goes straight to state 0xF (no entry).
- Otherwise the write position = 0. The cursor starts on 0x5F (END) if an LSNM chunk was found in the file, else on 0 ('A').

Input is accepted from frame 10. Every key below (directions, Shot/Enter, Bomb/ESC-cancel) accepts an edge press **or** key repeat (0x164D538), so holding Shot types repeatedly; only the ESC (0x08) finish is edge-only. In one frame the order is Up, Down, Left, Right, type, backspace, ESC-finish (typing and backspace can both happen in the same frame).

| key | effect |
|---|---|
| Up / Down | cursor ∓/± 16, wrapping mod 96; repeated while the cell holds a space; SE 12 |
| Left / Right | cursor ∓/± 1, wrapping inside the 16-cell row (Left from column 0 → column 15, Right from 15 → 0); repeated while the cell is a space; SE 12 |
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

Charset: pointer 0x4C7F48 → string 0x4B7340, 96 cells, 6 rows × 16:

| row | cells |
|---|---|
| 0 | `ABCDEFGHIJKLMNOP` |
| 1 | `QRSTUVWXYZ.,:;_@` |
| 2 | `abcdefghijklmnop` |
| 3 | `qrstuvwxyz+-/*=%` |
| 4 | `0123456789#!?'"$` |
| 5 | `(){}[]<>&\|~^` space `-` `-` |

- Only cell 0x5D is a space, so it is the only cell that can never be selected (correction: 0x5C is `^` and is selectable).
- Cells 0x5E and 0x5F hold `-` in the table but are drawn with the ASCII glyphs 0x80 ("SP") and 0x81 ("END"); confirming them writes a space / finishes.

**Grid draw** (`Result_Draw`, states 9 and 13; VERIFIED): origin (160, 356) (constants 0x4B4554 / 0x4B7C40),
cell (c, r) at (160 + 20c, 356 + 18r), each glyph queued as a 1-character ASCII string (12 §2.9, highlight off).
- Normal cells: colour 0xC0C0C0C0, scale 1.
- Selected cell: colour 0xFFFFFFC0; scale s pulses with the result frame counter f (+4): f mod 64 < 32 →
  s = 1.2 + (f mod 32)·0.8/32, else s = 2.0 − (f mod 32)·0.8/32 (1.2 → 2.0 → 1.2 over 64 frames); the glyph is
  moved by −(s − 1)·8 in x and y so it grows around its centre.
- Scale reset to 1 after the grid.

### 2.3 Ranking display (Result_Draw 0x4586B4) — VERIFIED

Ref `menu_ref/mopup1_res_rank.png`. The ranking is drawn in every state except 8 (card list) **while the
x of result00.anm VM 40 is < 640**. VM 40 is an invisible anchor: interrupts 3–8 and 10 move it (after 10
frames at x 640) to (32, 112) over 10 frames (`pos_time_out2`); any other interrupt (1, 2 …) sends it back
to x 640. Let A = VM 40's raw pos (steady state (32, 112)). List = [diff +0x44][shot +0x3C] (linked list
head at Result+0x1144C + 0x90·diff + 0xC·shot).
- Header `No  Name       Score(Stage)   Date   Slow` at (A.x + 24, A.y + 18), colour 0xFFE0E0EF.
- Rows i = 0..9 at y = A.y + 36 + 18i, pen x starts at A.x + 24:
  - colour: state 9 → the new entry (entry byte +9 ≠ 0) 0xFFF0F0FF, others 0xC0FFC0C0; other states 0xFFFFC0C0;
  - `%2d` rank at x; x += 48;
  - state 9, new entry: `"%8s"` of 8 spaces with `_` at min(pos, 7) at x (the cursor underline);
  - `"%8s %9d%1d(%d)"` / `"%8s %9d%1d(C)"` (§2.2 values) at x; x += 320. Arguments: name (+0x17), score (+0x0C), min(continues +0x26, 9), and for the `(%d)` form (stage byte +0x16 < 99) the **shown stage number = table 0x4C7F80[stage byte]** = (1, 2, 3, 4, 4, 5, 6, 6, 1) for bytes 0..8 (4A/4B both 4, 6A/6B both 6, Extra 1), not the raw byte; byte ≥ 99 → `(C)`. So default entries (stage byte 0, 15 §2.3) print `(1)` (VERIFIED `Result_Draw` 0x4586B4, table 0x4C7F80; RE-Q24);
  - `" %5s   %3.2f"` date, slowdown at x;
  - next row: x −= 368.
- Steady state: header (56, 130); rank x 56, name x 104, date x 424; rows y 148 … 310.

### 2.4 Result viewer from the main menu (mode 0) — VERIFIED

Refs `menu_ref/mopup1_res_*.png`, draw log `menu_mopup1.drawlog.xz` (scene 5).

**VM draw** (`Result_Draw` 0x4586B4, VERIFIED RE-Q24): every frame all 72 `result00.anm` VMs are drawn in index order with `AnmManager_Draw2DSnap` at pos **+ pos_offset** (+0x288; pos is saved, offset added, drawn, then pos restored), so the op-6/op-24 offset writes (e.g. the selected item at (−4, −4)) shift the drawn position of every result00 VM.

Result object fields used here: +4 frame counter (zeroed by `Result_SetState`), +8 state, +0x10 sub-state,
+0x18 sub-frame, +0x1C cursor, +0x30 card-list character (0..12, 12 = all; initial 12), +0x34 character the
card list was built for, +0x38 accordion flag, +0x3C ranking shot (initial 0), +0x40 card page, +0x44
ranking difficulty (initial 1), +0x48 card group (initial 5), +0x4C hidden-code counter, +0x54 card-list
leaving flag, +0x64 + 0x34·g + 4·s captured-card counts (below). VMs: 72 `result00.anm` VMs (VM i runs script
i) at +0x1A0; interrupt k to VM i = pending interrupt at VM+0x1FE.

**Exit** (changelog/answers_impl17.md §Q4, VERIFIED with mopup1): X/confirm on item 3 at frame P → state 2.
- P+1: `Loading_Begin(500, 440)`, all VMs interrupt 2.
- P+20: next scene 1, object removed (score.dat saved; surface slot 0 `result.jpg` moved to slot 8 = the loading
  background).
- P+21: first menu frame. The loading VMs are already 20 frames old there; in scene 5 they were covered by the result
  background.

**Menu pattern** (states 1, 3, 4, 6, 7): sub-state 0, sub-frame 0: cursor := remembered value; every VM of the
page gets interrupt 22, is run once, then 20 (selected) or 21 (others). After N sub-frames (N = 20 in state 1,
6 elsewhere) sub-state 1 — exact timing (VERIFIED 0x454CB2, 0x45567D, RE-Q24): the sub-frame +0x18 is incremented at the end of each sub-state-0 call that is still waiting; on the call where it reads N at the test, sub-state := 1, sub-frame := 0 and the sub-state-1 input code below runs **in that same call**, so input is accepted on sub-frame N: Up/Down (edge or repeat, wrap, SE 12, `Result_CursorUpDown` 0x45A0F4) → resend
20/21. Confirm (Shot/Enter edge): selected VM 23, others 1, SE 10. Cancel (Bomb/ESC edge): SE 11.

| state | VMs | count | confirm | cancel |
|---|---|---|---|---|
| 1 top (`Result_TopMenu` 0x454CB2) | 0–3 | 4 | 0 → 3, 1 → 6, 2 → 0x13; 3 → state 2 (exit) with SE 11 | cursor := 3 (SE 11); if already 3 → state 2 (exit, SE 11) |
| 3 difficulty (`Result_CheatUnlock` 0x4550FC) | 4–8 | 5 | +0x44 := cursor, state 4, cursor 0 | +0x44 := cursor, state 1, cursor 0 |
| 4 character (0x45567D) | 15–26 | 12 | VM 40 interrupt 3, +0x3C := −1, state 5 | +0x3C := cursor, all VMs 15–26 interrupt 1, state 3 |
| 6 card group (0x455A33) | 9–14 | 6 | +0x48 := cursor, state 7 | +0x48 := cursor, state 1, cursor 1 |
| 7 card character (0x455CB0) | 27–39 | 13 | +0x30 := cursor, state 8, VM 40 interrupt 3, cursor 0, +0x40 := −1 | +0x30 := cursor, VMs 27–39 interrupt 1, state 6 |

State 1 additionally: on entering (sub-frame 0) all 72 VMs get interrupt 1 first. **D** (edge, input bit
0x2000) in sub-state 1 writes score.txt (§2.7), no sound. State 1 is entered from state 0 (the initial state)
and when the play-data page ends.
State 3: the hidden code (15 §2.12) runs every frame including sub-state 0.
Card groups (table 0x4C67D0 / counts 0x4C67E8): 0 Easy (42 cards), 1 Normal 49, 2 Hard 50, 3 Lunatic 50,
4 Extra 14, 5 all 222 (ids 0–221).

**State 5 — ranking** (0x455925). The first frame after a change of the shot (frame 10) stores +0x3C :=
cursor. From frame 6: Left/Right (edge or repeat, 12 shots, wrap, SE 12, `Result_CursorLeftRight` 0x45A2F2):
frame := 0, VM 40 interrupt 3 + diff (re-slides the table), old shot VM (15 + old) interrupt 24, new 25.
Cancel: +0x3C := cursor, VM 40 interrupt 1, SE 11, state 4. (Shot changes are applied at frame 10, while VM 40
is off-screen, so the list swap is invisible.) Draw: §2.3.

**State 8 — card list** (0x455F6B). n = group count, page p = +0x1C (pages ⌈n/10⌉), character +0x30.
- If +0x54 ≠ 0 and frame > 9 → state 7.
- At frame 10, if (page or character) changed: +0x40 := page, +0x34 := character; for the ≤ 10 cards of the
  page, row text VM k (k = idx mod 10) is rendered with the card name (CATK +0x10) if story attempts[12] ≠ 0,
  else `？？？？？` (5 full-width ?), alpha 255; header VM 10 is rendered `"%3d/%3d（キャラ切り替え↓↑）"` with
  (captured count [group][character], n), alpha 255.
- From frame 6: Left/Right (edge or repeat, wrap over (n + 9) / 10 pages, SE 12) → frame 0, VM 40 interrupt 10.
  Only if no Left/Right this frame: Up/Down (13 characters, `Result_CursorUpDown2` 0x45A1F3 on +0x30: Up (0x10) first, then Down (0x20), each on an edge press or key repeat 0x164D538; wraps mod 13; **SE 12** on a move; VERIFIED RE-Q24) → frame 0, +0x38 := 1, VM
  (27 + old) interrupt 24, VM (27 + new) 25. Cancel: +0x54 := 1, frame 0, SE 11, VM 40 interrupt 1.

Captured counts (Result_OnRegister): for group g and character s = 0..12, count of cards of the group whose
CATK magic is `CATK`, byte +8 = 3, and (story captures[s] ≠ 0 or practice captures[s] ≠ 0).

Text VMs (Result_OnRegister): 28 VMs at Result+0xBFC0 + 0x2A4·k, each set up by `AnmVm_Init` + `AnmVm_SetSprite` (0x4068A0: no script runs, so the VM keeps the `AnmVm_Init` defaults — colour/alpha as initialised there, not hidden), pos := (0, 0, 0), flags |= 0x1800 (anchor top-left), font bytes +0x298/+0x299 := 15 (VERIFIED 0x45964D, RE-Q24); a text VM that `Result_Draw` draws without positioning it first therefore appears at (0, 0): k = 0..13 use
`text.anm` sprites 21..34, k = 14..27 `resulttext.anm` sprites 2..15 (512×17 strips). Card list: k = 0..9
rows, k = 10 header. Play data: k = 0..19. Plus VM +0x1119C = `result00.anm` sprite 32 (256×16 separator bar,
default centred anchor).

Card list draw (Result_Draw, state 8, while VM 40.x < 640; ref `mopup1_res_cards.png`), A = VM 40 pos:
- header VM 10 at (A.x + 320, A.y − 16); pen y = A.y + 16, x = A.x.
- For rows i = 0..9 with idx = p·10 + i < n, id = group[idx], s = +0x34:
  - bar VM at (x + 320, y + 16), scale x 2.375 (centred, so it spans x + 16 … x + 624);
  - row text VM i positioned at (x, y);
  - colour: story attempts[s] = 0 → 0xC0C0C0FF; story captures[s] = 0 → 0xFFC0A0A0; else
    0xFFF0F0FF − i·0x00080800;
  - `"No.%.2d"` (id + 1) at (x, y); row text VM drawn at (x + 78, y);
  - at (x + 446, y): id < 205: story attempts[s] = 0 → `---/---(-)`, else `"%3d/%3d(%s)"` story
    captures[s], story attempts[s], letter of CATK +0x0F (last difficulty: `E N H L -`); id ≥ 205: the same
    with practice attempts / captures;
  - colour 0xFFA08090, scale 0.8: if story captures[s] ≠ 0 `"MaxBonus %8d"` (story max bonus[s]) at
    (x + 424, y − 13); scale 1;
  - row step: +0x38 = 0 → y += 33. Accordion after a character change (+0x38 = 1): frame f < 10 →
    y += ⌊(10 − f)·33/10⌋, else y += ⌊(f − 10)·33/10⌋ (rows collapse to one line at f = 10, when the names are
    swapped, and spread again); +0x38 := 0 once f > 19.
- Steady state: header (352, 96), rows y 128 + 33i, number x 32, name x 110, counts x 478, MaxBonus (456, y − 13).

**States 0x13–0x15 — play data** (`Result_PlayStats` 0x4578AA; ref `mopup1_res_play.png`). Entering state 0x13
(frame 1) renders text VMs k = 0..18 (`Text_RenderToVm`, white, outline 0) and positions them at x 56,
y = 64 (k 0), 81, 98, then +17 each, with one extra +17 gap before k = 16:

| k | y | text |
|---|---|---|
| 0 | 64 | `総起動時間   %.2d:%.2d:%.2d` PLST run time h, m, s (after `Supervisor_UpdateRunTime`) |
| 1 | 81 | `総プレイ時間 %.2d:%.2d:%.2d` PLST play time |
| 2 | 98 | `プレイ回数　　　 　Easy 　Norm 　Hard 　Luna  Extra  Total` |
| 3–14 | 115 + 17(k−3) | `"%s %6d %6d %6d %6d %6d %6d"`: shot name (0x4C7F4C, full-width padded: 霊夢＆紫, 魔理沙＆アリス, 咲夜＆レミリア, 妖夢＆幽々子, 博麗　霊夢, 八雲　紫, 霧雨　魔理沙, アリス・Ｍ, 十六夜　咲夜, レミリア・Ｓ, 魂魄　妖夢, 西行寺　幽々子), plays [1+shot] of blocks E, N, H, L, Ex, block 6 |
| 15 | 319 | `全主人公合計` row: [0] of blocks 0–4 and 6 |
| 16 | 353 | `クリア回数  　　 %6d…` [14] of blocks 0–4, then block 6 [14] (recomputed at register as Σ) |
| 17 | 370 | `コンティニュー   %6d…` [15] of blocks 0–4, then **block 5 [15]** (QUIRK: the unused block, normally 0; the total recomputed into block 6 is not shown — VERIFIED at runtime: shows 0 with 2 + 4 continues) |
| 18 | 387 | `プラクティス　   %6d…` [16] of blocks 0–4, then block 6 [16] |

(PLST layout 15 §2.9.) VM 19 is positioned at y 404 but never rendered. The run-time seconds byte is kept at
Result+0x19C.
- 0x13: frames < 40: alpha of VMs 0..19 = f·255/40; at f = 40 → state 0x14 (frame not reset).
- 0x14: every 60 frames, `Supervisor_UpdateRunTime`; if the seconds changed, re-render line 0. Any of
  Shot/Bomb/ESC/Enter (0x100B, edge) → state 0x15, frame 0.
- 0x15: alpha = 255 − f·255/20 (byte arithmetic, −1 − f·255/20); when f > 19 → state 0, and the same frame
  continues as state 1 (top menu re-entry).
- Draw: VMs k = 0..19 are drawn (after the ranking/end panel) only in states 0x13–0x15.

Confirm SE is 10, cancel SE is 11 throughout.

### 2.5 End-of-game panel (`Result_DrawEndPanel` 0x457E00, states 0xF/0x10) — VERIFIED

Drawn by Result_Draw in states 0xF and 0x10 (after a game). Anchor = VM 71 (`result00.anm` sprite 40,
`result03.png` 256×256, top-left at (48, 96), alpha 0 → 255 over 30 frames on interrupt 18, → 0 over 30 on
the next interrupt). Queue colour = VM 71's current colour (so the text fades with it). Pen P = (VM71.x +
210, VM71.y + 32) = (258, 128); each line y += 22:

| line | y | x | content |
|---|---|---|---|
| 1 | 128 | 258 | `%9d` stats+0x00 (displayed score ÷10); then at x + 9·13 = 375 `%1d` min(continues, 9) |
| 2 | 150 | 258 | difficulty (0x4C7FFC: `      Easy`, `    Normal`, `      Hard`, `   Lunatic`, `     Extra`) |
| 3 | 172 | 271 | `    %3.2f%%` of min(t / T, 0.99)·100, t = 0x164D30C (play-time counter, + 1800 per ECL op 148), T = 195559 (E–L) or 80000 (Extra); `      100%%` if game flags bit 4 (cleared) |
| 4 | 194 | 271 | `%9d` continues (stats+0x29) |
| 5 | 216 | 271 | `%9d` trunc(stats+0x64) misses |
| 6 | 238 | 271 | `%9d` trunc(stats+0x84) bombs used |
| 7 | 260 | 271 | `%9d` stats+0x1C spell cards captured |
| 8 | 282 | 271 | `    %3.2f%%` slowdown (15 §2.11 formula) |

(From line 3 on the pen x is +13.) The labels are part of the result03.png image. The function also computes
an unused "rating" value (score, lives, continues, misses, bombs, cards, point items, graze terms); it is never
displayed (dead code, may be skipped). Queue colour reset to white at the end.

### 2.6 Replay-save dialogue (Result states 10–14, `Result_SaveReplay` 0x45696F) — VERIFIED

Complements 15 §3.6. Uses the frame counter +4 (state 10 is entered with +4 = 9). Cursor +0x1C, slot +0x28,
name +0x58 (9 bytes), char cursor +0x2C.
- **10 Save?** At frame 10: i = 0x13 if slow mode / no-record flag, else 0xE if continues ≠ 0, else 0xB; that
  interrupt goes to 72 VMs starting at **VM 4** (QUIRK: VMs 4–71 and the first 4 text VMs; VMs 0–3 are
  skipped); if i ≠ 0xB → state 11; cursor 0. Every frame: VM 50 RGB := 0xFF6060 if cursor 0 (はい) else
  0x606060, VM 51 the opposite (alpha kept). From frame 12: Left/Right toggles (2 entries, edge or repeat,
  SE 12). Cancel (Bomb/ESC 0x0A edge) or ESC (0x08 edge), or confirm on cursor 1 → SE 11, state 2, all VMs
  interrupt 2, frame 0. Confirm on cursor 0 → SE 10 (sound flushed), state 12, all VMs interrupt 12, frame 0.
- **11 message** (cannot save): from frame 20, confirm or cancel → SE 11, state 2, VMs interrupt 2.
- **12 slot list**: at frame 0: create directory `replay`, read the 15 headers `./replay/th8_%.2d.rpy` into
  Result+0x46470 + 0x134·i. From frame 20: Up/Down over 15 (edge/repeat, wrap, SE 12), +0x28 := cursor.
  Confirm: SE 10, +0x28 := cursor, frame 0, date → +0x476E8, score (stats+8) → +0x4772C; if the slot holds a
  valid replay (magic `T8RP` 0x50523854 and version & 0xFFF = 6) → all VMs 13, VM (56 + slot) 16, state 14;
  else all VMs 17, VM (56 + slot) 16, state 13. Then name pos +0x1C := 0, char cursor := 0x5F if an LSNM
  chunk was loaded (+0x50) else 0. Cancel: SE 11, state 10, all VMs 2, frame 0 (state 10 runs again from
  frame 0).
- **14 overwrite?** VMs 50/51 colours as in state 10. From frame 20: Left/Right. Confirm on cursor 0 → frame
  0, all VMs 17, VM (56 + slot) 16, state 13. Cancel, ESC, or confirm on cursor 1 → SE 10, state 12 (back to
  the list, frame 0).
- **13 name entry**: from frame 30; same grid, cursor movement and edit rules as §2.2 (all keys edge or
  repeat), writing into +0x58. Choosing END: `Replay_Finish("./replay/th8_%.2d.rpy", slot + 1)`, state 2,
  all VMs 2, LSNM := name. ESC (edge) → SE 10, back to state 12. Typing SE 10 is followed by a sound flush.
- **Draw** (states 10–14): VMs 49–54 drawn a second time on top; `No.   Name     Date   Player Score` at
  VM 55; rows i = 0..14 at VM 56 + i, colour 0xFFFF8080 for i = slot, else 0xFF808080:
  - state 13: every row prints the new entry `"No.%.2d %8s %5s  %7s %9d0"` (i + 1, name, date, shot name
    0x4C8010[shot + 0x164D0B2], score), then colour 0xFFF0F0FF `"      %8s"` with `_` at min(pos, 7) (only the
    chosen row VM is on screen: the others were sent away by interrupt 17);
  - else valid slot: `"No.%.2d %8s %5s  %7s %9d0"` with the file's name (+0x72), date (+0x6C), shot (+0x6A),
    score (+0xB0); invalid/empty: `"No.%.2d -------- --/--  -------          0"`.
  The grid (§2.2) is drawn in state 13 too.

### 2.7 score.txt (`Result_DumpScoreTxt` 0x454298, D key in the result top menu) — VERIFIED

Ref `specs/data/drawlog/menu_mopup1.score.txt` (written from `menu_mopup1.score.dat`). Built in a 6.25 MB
buffer, Shift-JIS, every line ends with CR LF, written with length strlen to `score.txt` in the game directory.
Formats below are printf formats (`\r\n` omitted; `　` = full-width space U+3000).

1. Header lines:
   `# 東方永夜抄　～ Imperishable Night. ver 1.00d 記録テキスト版`, `# `,
   `# このファイルは、現在の記録をダンプした物です。`,
   `# このファイルの内容を変更してもゲームには反映されません。`,
   `# このファイルは自由に利用したり転載したり、別のフォーマットにしても構いません。`,
   `# ` + 12 × `　` + `   Time-stamp: <%s>` with local time `strftime("%y/%m/%d %H:%M:%S")`,
   two empty lines, `Version: %.2f` with 0.01 (→ `Version: 0.01`), one empty line.
2. Card names: name[i] = CATK[i] +0x10 if its magic is `CATK`, else `？？？？？`.
3. For difficulty d = 0..4, for shot s = 0..11: walk list [d][s] (≤ 10 nodes, n = node index); for every entry
   whose magic is `HSCR` (default entries have none and are skipped, but still count in n) append to a
   per-shot buffer:
   - `  No.%d ` (n + 1)
   - `    得点  %d%d` score ÷10, continues (signed byte +0x26, not capped)
   - `    名前  %s` (+0x17)
   - stage 99: `    ステージ         All Clear`; else `    ステージ         %s` with 0x4C7FAC[stage] =
     `Stage 1`, `Stage 2`, `Stage 3`, `Stage 4-uncanny`, `Stage 4-powerful`, `Stage 5`, `Stage 6-Eirin`,
     `Stage 6-Kaguya`, `Extra Stage`
   - `    日付               %s` (+0x20)
   - `    処理落ち率        %.2f％` (+0x10)
   - `    プレイ時間      %.2d分%.2d秒` (+0x64 frames: ÷3600, (÷60) mod 60)
   - `    初期プレイヤー数     %d人` (byte +0x44 + 1)
   - `    得点アイテム数   %5d個` (+0x68), `    刻符数          %6d個` (+0x80),
     `    ミス回数           %3d回` (+0x70), `    ボム回数           %3d回` (+0x74),
     `    ラストスペル回数   %3d回` (+0x78), `    ポーズ回数         %3d回` (+0x7C),
     `    コンティニュー回数 %3d回` (+0x26 signed)
   - `    人間率          %3.2f%%` (+0x84 / 100.0)
   - `    取得スペルカード一覧 (総取得回数/総遭遇回数)`
   - for each card i with HSCR +0x88 + i ≠ 0: `      No.%.3d %s (%d/%d)` with i + 1, name[i] padded with
     spaces to exactly 48 bytes, story captures[12], story attempts[12] (all-shot totals from CATK, not from
     the entry).
   If the shot had ≥ 1 entry: if it is the first such shot of difficulty d, write
   `# ======================================== ` (note the trailing space) and `難易度 %s` (0x4C7FD4: `Easy`,
   `Normal`, `Hard`, `Lunatic`, `Extra`); then `# ---------------------------------------- `,
   `使用キャラ %s` (0x4C7F4C full-width name), and `%s` + CR LF with the per-shot buffer (so an empty line
   follows each shot block).
4. `# ======================================== `, `スペルカード一覧 本編　練習`, then for every card with magic
   `CATK` and (story attempts[12] ≠ 0 or practice attempts[12] ≠ 0):
   `No.%.3d %s %3d/%3d %3d/%3d  (%s)` = id + 1, name padded to 48 bytes, story captures[12] / story attempts[12],
   practice captures[12] / practice attempts[12], difficulty name 0x4C7FD4[CATK +0x0F].
5. `Supervisor_UpdateRunTime`; `# ======================================== `;
   `総起動時間   %.2d:%.2d:%.2d`, `総プレイ時間 %.2d:%.2d:%.2d`, the play-count header, 12 shot rows and the
   全主人公合計 row exactly as the play-data page (§2.4, block 6 totals), then
   `クリア回数  　　 %6d…` (total = block 6 [14]), `コンティニュー   %6d…` (total = block 6 [15], correct
   here unlike the page), `プラクティス　   %6d…` (block 6 [16]).

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
| Bomb / ESC | scene 1 with the loading banner (0x44886F: `Loading_Begin` + capture of the current screen into surface slot 8 after the next Present, which the loading screen then shows instead of the logo; 12 §1) |

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

Returning from a spell-practice game (+0x6C ≠ 0) arrives through screen 15 (12 §2.2): screen 16 runs for one frame and
forwards to 17 (cursor 0), returning 2 so screen 17 runs in the same frame. If the last card id is ≥ 205, g_StageIdx := 9.
Bomb/ESC keeps flags bit 14 (only bit 0 is cleared).

Entry interrupts (changelog/answers_impl17.md §Q3, VERIFIED):
- range interrupt 18, then pending **27** written into VMs 138 and 139. 138 has label 27 → fades in (16 frames) and
  slides to (320, 48); 139 has none → hidden.
- portraits/description per 12 §2.4 (entries 1–3, description disabled). The same is redone on every shot change, which
  also zeroes the screen frame counter (pie easing restarts). A shot change writes **only** these per-VM interrupts (VMs 111–130: enabled bit off + interrupt 8; the shot's two portrait VMs from table 0x4B8CD8[shot][1..2]: on + 9; the description VM [shot][3]: on + 23) — no range interrupt, so VMs 0–110 and 131–141 (incl. 141) are untouched (VERIFIED 0x46B174, RE-Q24).
- Input (sub-state 1) starts when the screen frame counter +0x1442C reaches 8 (it counts from 0 at entry, +1 per call).
- cursor (stage row) = g_StageIdx.
- Leaving: confirm sets g_StageIdx = row; Bomb sets g_StageIdx = row, sends range interrupt 6, and returns to the top
  menu (cursor 2) after 20 frames.

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
- range interrupt **26** to all 142 title01 VMs. Portraits 111–118: alpha → 128, (320, 240). Headings 138/139 fade out,
  140 fades in. VM 141 has no label 26 → catch-all → reset to (640, 320) and hidden, so the next screen-16 entry slides
  it in again;
- `LastWord_ComputeUnlocks` runs (15 §2.8);
- the cursor is placed on the previously played id (0x164D0B8);
- the list is shown 15 rows per page.

Row text:
- attempted (story attempts[12] or practice attempts[12] ≠ 0): card name;
- otherwise a Last Word unlocked in FLSP (`Card_IsSelectable` 0x46FD5F): `挑戦者歓迎中！`;
- otherwise `？？？？？？？？`.

The selected row is white, others grey (0x60).

Controls:
- Up/Down move the cursor (`Menu_CursorUpDown(count)`, wraps).
- Only when count > 15 (edge or key repeat, SE 12 + sound flush on Left): **Left** cursor −= 15, if < 0 → count − 1;
  **Right**: if count mod 15 < count − cursor → cursor += 15 (clamped to count − 1), else cursor = cursor mod 15
  (back to the first page).
- Page = cursor ÷ 15. When the page changes, the 15 row VMs are re-created and their names re-rendered (§6.5).
- When the cursor changes: all row VMs grey (RGB 0x60), the cursor row white, the 7 info VMs alpha 0, and the
  info refresh counter (+0xC29C) = 21 (§6.3).

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

Layout (VERIFIED, ref `menu_ref/mopup1_sp17.png`). Seven text VMs at menu+0x11F2C + 0x2A4·k, k = 0..6, each
`text.anm` sprite 21 + k (512×17 strips of the text texture, rendered with `Text_RenderToVm`, white 0xFFFFFF,
outline 0, font 15), anchor top-left, colour 0xFFFFFFFF. Position x 64, y = 344 + 16k for k < 4 and
344 + 16k + 8 for k ≥ 4 → y 344, 360, 376, 392, 416, 432, 448. Lines 5 and 6 (the notes / hint) are moved to
x **96** on every call. Lines: 0 No./name, 1 owner/difficulty, 2 header with the shot name, 3 numbers, 4
`カードの補足`, 5/6 notes or unlock hint (contents below). The 7 VMs are drawn by the screen-17 draw (§6.5).

Refresh timing: on entry (sub-state 0) all 7 lines are rendered at once and shown. After a cursor change the
counter is 21 and the VMs are transparent; `SpellPractice_DrawInfo` runs every frame of sub-state 1 while the
counter ≠ 0 and decrements it at the end. Line 0 is re-rendered when the counter is 11, line 1 at 9, line 3 at
7, line 5 at 5, line 6 and then **all 7 alphas := 255** at 3. Lines 2 and 4 never change after entry. So the
panel is blank for 18 frames after a move, then appears at once.

- `Ｎｏ．%s　　%s`: number = id + 1 as **three full-width digits** (`０`…`９`, leading zeros, e.g. `０１４`), and the name; `？？？？？？？？` (8 full-width ?) if never attempted (story attempts[12] + practice attempts[12] = 0).
- `使用者  %s　　%s %s`: owner, difficulty name (table 0x4C8628), and "Last" when the card is in the last-spell list 0x4C6B90.
- `取得数/挑戦数[最高点]　%s　全主人公合計`, then the values:
  - normal cards: `%3d/%3d(%3d/%3d)[%.8d]` twice (this shot, then total), with practice captures/attempts, story captures/attempts in brackets, and the practice max bonus;
  - Last Words: `%3d/%3d(---/---)[%.8d]`;
  - never attempted: `---/---(---/---)[--------]`.
- `カードの補足` (card notes): the two comment lines (CATK +0x70/+0xB0). They are shown only if practice captures[12] ≠ 0, else `？？？？？？？？？` (9 full-width ?, string 0x4B9330).
- For locked cards with id ≥ 204 that were never attempted, a two-line **unlock hint** comes instead from table 0x4C82C8 (18 records × 0x30: {format ptr, 5 ints} ×2, ints printed +1). Full texts of all tables and format strings: `specs/data/spell_tables.md`.

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

### 6.4 Screen 16 draw (`MainMenu_SpellPracticeCharDraw` 0x46F5CB) — VERIFIED

Ref `menu_ref/mopup1_sp16.png`. Text through the ASCII queue (12 §2.9). Anchor A = current raw pos of title01
VM 141 (screen 16 sends interrupt 18 to all VMs, so A slides from (640, 320) to (256, 320) over 20 frames).

Scratch VMs created on entry (sub-state 0, frame 0):
- menu+0xE7B8: `resulttext.anm` sprite 2 (512×17 strip), anchor top-left, white, rendered text
  `ステージ    取得/挑戦可能/総数（()内は全キャラ合計）`.
- menu+0xEA5C: `resulttext.anm` sprite 3, anchor top-left, white, text `取得率　（キャラ別／総合）`.
- menu+0xED00: `title01.anm` sprite 144 (`select01.png` (0, 400) 96×64, the pie-chart frame). QUIRK: the code
  sets the anchor bits on the **EA5C** VM again instead of ED00, so ED00 keeps the default **centred** anchor.
- The four pie values (+0xC2A0..+0xC2AC) are zeroed.

Per frame:
1. VM E7B8 at (A.x − 32, A.y − 152). Pen y = A.y − 136, **x = A.x − 32** (the code reuses the heading's x as the pen;
   changelog/answers_impl17.md §Q3, matches the mopup1 log). Steady state: rows at x 224, numbers at x 406.
2. Rows r = 0..9 (stage rows 0..8 and Last Word, card lists of §6.2). Over the cards of row r count, for the
   current shot s: pc = practice captures[s] ≠ 0, sc = story captures[s] ≠ 0, seen = story or practice
   attempts[s] ≠ 0 (or, for rows > 8, `Card_IsSelectable`), and the same over all shots ([12]): PC, SC, SEEN.
   Marker = `@` if pc = count, else `*` if PC = count, else ` `.
   - colour: cursor row white + highlight, others 0xFFA0A0A0;
   - `"%s%s"` (marker, name from 0x4C8274: `Stage1   `, `Stage2   `, `Stage3   `, `Stage4A  `, `Stage4B  `,
     `Stage5   `, `Stage6A  `, `Stage6B  `, `Extra    `, `Last Word`) at (A.x − 32, y);
   - scale x 0.75 (y 1.0), `"%3d(%3d)/%3d/%3d"` = pc, PC, SEEN, count at (A.x + 150, y); scale back to 1;
   - y += 16.
   (The per-shot "seen" count and the SC sums are computed but only used below.)
3. y += 5; colour 0xFFD06060, no highlight: `"%sTotal"` with the marker rule against 222 at (A.x − 32, y), then
   scale x 0.75 `"%3d(%3d)/%3d/%3d"` = Σpc, ΣPC, ΣSEEN, 222 at (A.x + 150, y). Reset white, scale 1.
4. VM EA5C at (400, 376); VM ED00 at (552, 426) (centred).
5. Pie charts (`Menu_DrawPie` 0x46FDC2 → `AnmManager_DrawFan` 0x464EC0). Values:
   v1 = Σpc/222, v2 = ΣSC/205 (this shot), v3 = ΣPC/222, v4 = Σ(story captures[12] ≠ 0)/205 (all shots). Each
   value is eased while the screen frame f (+0x1442C) < 50: v := prev + (v − prev)·f/50, where prev is the
   value stored last frame (0 on entry); the result is stored back. From f = 50 on, v is the target.

   | pie | centre | radius | z | colour | value |
   |---|---|---|---|---|---|
   | 1 | (530, 420) | 24 | 0.02 | 0x60C0C0F0 | v1 |
   | 2 | (530, 420) | 12 | 0.01 | 0x80C0C0F0 | v2 |
   | 3 | (562, 438) | 16 | 0.02 | 0x6080C0C0 | v3 |
   | 4 | (562, 438) | 8 | 0.01 | 0x8080C0C0 | v4 |

   Pie geometry: a 64-vertex triangle fan (62 triangles), FVF XYZRHW|DIFFUSE (rhw 1), every vertex the same
   colour. Vertex 0 = centre; vertices 1..63 at centre + R·(cos a, sin a) with a = −π/2 + k·(0.101342·v)
   for k = 0..62 (0.101342 ≈ 2π/62), so the filled sector starts at 12 o'clock and runs clockwise (y down)
   over v·360°. The caller passes 2R (48/24/32/16), halved inside. Render state: flush, FVF 0x44, the 2D
   state of a scratch VM (normal alpha blend, z-write off), texture stage 0 colour and alpha = SELECTARG1
   DIFFUSE (untextured), afterwards back to MODULATE TEXTURE/DIFFUSE. Drawn immediately (not queued).
6. Percent readouts (`Ascii_DrawGaugePercent`, 12 §5.9 step 3, value = trunc(v·10000)): v1 at (514, 404)
   colour 0xFFFFFFFF, v2 at (514, 412) colour 0x80C0C080, v3 at (580, 451) white, v4 at (580, 443)
   colour 0x80C0C080.

### 6.5 Screen 17 draw (`MainMenu_SpellPracticeCardDraw` 0x46FFC0) — VERIFIED

Ref `menu_ref/mopup1_sp17.png`, `…_sp17_d.png`.
- Row VMs (menu+0xE7B8 + 0x2A4·j, j = 0..14, created on entry and on page change): `resulttext.anm` sprite
  j + 2 (512×17 strips), anchor top-left, font 15, text rendered white outline 0: card name (CATK +0x10) if
  attempted (story or practice attempts[12] ≠ 0); else `挑戦者歓迎中！` if `Spell_DifficultyOf` = 5 (Last Word)
  and `Card_IsSelectable`; else `？？？？？？？？`. Colour 0xFF606060, the cursor row 0xFFFFFFFF.
- Header VM menu+0x10F54: `resulttext.anm` sprite 17, top-left, white, text `取得数/挑戦数（()内は本編での集計）`,
  drawn at (334, 78).
- Rows j = 0..14 while page·15 + j < count, y = 94 + 16j, card id = list[page·15 + j]:
  - queue colour = that row VM's colour (white or 0xFF606060), no highlight;
  - marker `@` if practice captures[shot] ≠ 0, else `*` if practice captures[12] ≠ 0, else ` `;
    `"%sNo.%.3d"` (marker, id + 1) at (16, y);
  - scale x 0.8: `"%3d/%3d(%3d/%3d)"` = practice captures / practice attempts (this shot), story captures /
    story attempts (this shot) at (430, y); scale 1;
  - row VM drawn at (118, y).
- Then the 7 info VMs (§6.3) are drawn, and the queue colour/highlight reset.
