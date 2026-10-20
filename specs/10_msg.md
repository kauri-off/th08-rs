# 10 — Dialogue (msg*.dat): format, interpreter, portraits, text rendering

Sources: th08.exe via Ghidra (names in `symbols.csv`), all 33 `msg*.dat` files.
Tool: `tools/msgdump.py <dir|file> [-q]` — disassembles every file; **0 problems over 33 files**
(full dump `extracted/msg_dump.txt`). VERIFIED = read from code/data; INFERRED = interpretation.

## 1. Where the dialogue system lives — VERIFIED

There is no separate "MSG manager". The dialogue interpreter is part of the HUD object
(`g_Hud` 0x160F428, `FRScreenImplInf`, calc prio 15, draw prio 17, 00_overview §4). `g_Hud+8` points to a
large heap block ("HudImpl" below); all dialogue state is at offsets inside it.

| function | addr | role |
|---|---|---|
| `Msg_Load` | 0x439710 | called from `Hud_Load`: loads the stage's msg file; converts the entry offset table to pointers |
| `Hud_StartMsg` | 0x439810 | `Msg_Start(HudImpl, n)` (called by ECL timeline op 6 and msg op 22) |
| `Msg_Start` | 0x43396D | starts entry `n` (§4) |
| `Msg_Run` | 0x433DB3 | per-frame interpreter, called from HUD calc (0x4338CA) every frame unless the game is paused |
| `Msg_Draw` | 0x43542B | draws portraits, text box, text lines; called from HUD draw (0x433927) |
| `Msg_IsActive` | 0x4358BB | `HudImpl != 0 && entry index ≥ 0` (a −2 test exists but −2 is never stored) |
| `Msg_IsBlocking` | 0x43587E | `Msg_IsActive && release_counter == 0` (used only by timeline op 7) |
| `Text_RenderToVm` | 0x4663B0 | printf-style text → the VM's sprite area in a render-target texture (§6) |

File choice: table 0x4C74C0, `name = tbl[stage*12 + shot]` (stage index 0..8, shot 0..11; solos use their
team's file). Loaded only when **not** in spell-practice mode (GM flags bit 14).

| stage idx | 0 | 1 | 2 | 3 (4A) | 4 (4B) | 5 | 6 (6A) | 7 (6B) | 8 (Ex) |
|---|---|---|---|---|---|---|---|---|---|
| team 0 Rm&Yk (+Reimu, Yukari) | msg1a | msg2a | msg3a | msg4dm | msg4ba | msg5a | msg6a | msg7a | msg8a |
| team 1 Ms&Al (+Marisa, Alice) | msg1b | msg2b | msg3b | msg4ab | msg4dm | msg5b | msg6b | msg7b | msg8b |
| team 2 Sk&Rr (+Sakuya, Remilia) | msg1c | msg2c | msg3c | msg4ac | msg4dm | msg5c | msg6c | msg7c | msg8c |
| team 3 Ym&Yy (+Youmu, Yuyuko) | msg1d | msg2d | msg3d | msg4dm | msg4bd | msg5d | msg6d | msg7d | msg8d |

(`msg4dm` is a 4-entry placeholder for the route/team combinations that cannot occur.)

## 2. File layout — VERIFIED

Little-endian.

```
u32 count
u32 offset[count]          ; file offsets of each entry's first instruction (entries may share an offset)
instructions...
```

Instruction: `u16 time, u8 opcode, u8 arg_size, arg bytes[arg_size]`; next = this + 4 + arg_size.
An entry runs until opcode 0. Times are non-decreasing within an entry (the only exception is an
unreachable `end` with time 0 placed after `halt`).

Strings: each byte XOR 0x77; terminated by a byte that decodes to 0; the argument is padded to a multiple of 4
with bytes that decode to 0 (raw 0x77). Decoded text is Shift-JIS (cp932).

Entry counts per file: stages 1–2: 2; stages 3–5 (incl. 4A/4B): 4; 6A/6B files (`msg6*`, `msg7*`): 6 / 10;
extra (`msg8*`): 3.

## 3. Interpreter state (HudImpl offsets) — VERIFIED

| off | type | name | meaning |
|---|---|---|---|
| 0x21814 | ptr | file | loaded msg file (entry table converted to absolute pointers) |
| 0x21818 | ptr | ip | current instruction |
| 0x2181C | i32 | entry | running entry index; −1 = no dialogue |
| 0x21820 | Timer | clock | script clock (05 §2 Timer; ticks with game speed) |
| 0x2182C | i32 | wait_count | frames spent in the current wait / text-line counter reset |
| 0x21830 | i32 | min_wait | frames a wait must last before Shot can advance it (6 at start, then 8 or 30) |
| 0x21834 + k·0x2A4 | AnmVm[4] | face[k] | portrait VMs, k = 0..3 |
| 0x222C4 + l·0x2A4 | AnmVm[2] | line[l] | text line VMs (text.anm) |
| 0x2280C | AnmVm | boss_card | boss name card VM (op 8) |
| 0x22AB0 | AnmVm | — | run and drawn every frame; not started by any msg op (INFERRED unused here) |
| 0x22D54 + k·4 | u32[4] | text_colour[k] | GDI COLORREF per speaker (from table 0x4C7180, §6) |
| 0x22D64 + k·4 | u32[4] | outline[k] | always 0 (selects the 2-px outline style, §6) |
| 0x22D74 | i32 | font_size | 15 |
| 0x22D78 | i32 | release_counter | op 6 increments; decremented by 1 at the start of each `Msg_Run` |
| 0x22D7C | u8 | skippable | Ctrl fast-forward allowed (op 13) |
| 0x22D7D | u8 | text_speaker | speaker whose colour op 16 uses |
| 0x22D7E | u8 | new_box | next op 16 clears the box first |
| 0x22D7F | u8 | next_line | line index op 16 writes to |
| 0x22D80 | u8 | active_speaker | portrait currently highlighted (0xFF = none) |
| 0x22D81 | u8 | textbox | draw the dark text-box quad (op 18) |
| 0x22D82 | u8 | choice | 0/1, final-route choice (op 21) |
| 0x22D84 | i32 | stage_end | set by op 9; HUD draws the stage-clear bonus panel (12) |

Portrait slots k ↔ face files (Gui object 0x4EA670, `Gui_LoadFaces` 0x414590):

| k | file pointer | ANM slot | file |
|---|---|---|---|
| 0 | 0x4ECC94 | 16 | player-team character A (`face_rm00`/`mr00`/`sk00`/`ym00` by shot: team/solos 0,4,5 → rm; 1,6,7 → mr; 2,8,9 → sk; 3,10,11 → ym) |
| 1 | 0x4ECC98 | 17 | partner (`face_yk00`/`al00`/`rs00`/`yy00`) |
| 2 | 0x4ECC9C | 18 | stage face file (table below) |
| 3 | 0x4ECCA0 | 19 | second stage face file (table below; else none) |

Stage face files by stage index (VERIFIED `Gui_LoadFaces`, RE-Q25). Slots 16–19 are loaded only on a fresh start; a
same-stage reload (0x17CE8C4) re-uses the loaded slots. In **spell practice** slots 16/17 are not loaded at all and slot
18 gets the `…sp.anm` cut-in file instead (slot 19 none); `face_cdbg.anm` (slot 15) is always loaded.

| idx | slot 18 / slot 19 (normal) | slot 18 in spell practice (card id c) |
|---|---|---|
| 0 | face_st01 / — | face_st01sp |
| 1 | face_st02 / — | face_st02sp |
| 2 | face_st03 / — | face_st03sp |
| 3 (4A) | face_st04a / — | c = 0xD8 sksp, 0xD9 ymsp, 0xDA alsp, 0xDB rssp, 0xDC yysp, 0xDD yksp (`face_<x>.anm`); else (incl. 0xD6) face_st04asp |
| 4 (4B) | face_st04b / — | face_st04bsp |
| 5 | face_st05 / face_st05b | c = 0xD4 face_st05msp, else face_st05sp |
| 6 (6A) | face_st06 / — | face_st06sp |
| 7 (6B) | face_st06 / face_st07 | c in 0x93–0x96 face_st06sp, else face_st07sp |
| 8 (Ex) | face_st08m / face_st08 | c in 0xBF–0xC1 or 0xD5 face_st08msp; c in 0xC2–0xCC or 0xD3 face_st08sp; else nothing loaded |
| other | face_st03 / — | face_st03 |

Speakers 0/1 are the player side (left), 2/3 the opponent side (right).

## 4. Starting a dialogue (`Msg_Start(n)`) — VERIFIED

1. Clear all dialogue state except the file pointer (0x55C dwords from +0x21814).
2. Stage-specific hooks when `n == 0`:
   - stage 5 (idx 5): `Hud_SetBossNamePlate(0x16)` 0x437F5C — copies `front.anm` sprite 0x16's rect into
     sprite 10 (the boss name plate, 12 §5.x), i.e. the plate switches from Tewi (0x15) to **Reisen** (0x16);
   - 6A (idx 6): spell-background script base `g_Stage+0xB34 (0x4E4B64) = 2` — the two spell-background VMs
     (0x4E4B68 / 0x4E4E0C) then use slot-9 scripts 2, 3 instead of 0, 1 (`Spell_Start` sets script +0xB34 + i);
   - 6B (idx 7) and Extra (idx 8): swap face files of slots 2 and 3 (0x4ECC9C ↔ 0x4ECCA0), spell-background base
     = 2, and plate 0x18 (**Kaguya**) / 0x19 (**Mokou**).
   (changelog/answers_impl10.md §5–6, VERIFIED. Other writers of +0xB34: `EffectMgr_OnRegister` = 0, `StageStart` = 2 in
   spell practice, 06, changelog/answers_impl10.md §5.)
3. Stage 5, `n == 10` → final-route resolution (the ECL asks for entry 10 after the stage-5 boss):
   - if the continue count `stats+0x29 != 0` (any continue used; +0x29 is incremented by each continue, 16 §2.2 —
     corrected name, RE-Q25): n = 1, choice = 0;
   - else if replay playback (GM flag bit 3): route byte from the replay header (`replay+8 → +0x11C`):
     2 → n = 3, 1 → n = 2, else n = 1 (choice = 0 for n = 1, 1 otherwise);
   - else: if the player has never cleared 6B (clear-mask bit 7 in `gm+0x3C49C + shot*0x24 + diff*2`, all 4
     difficulties) **and** `shot < 4` (team): if 6A was never cleared either (bit 6 in the second array
     `gm+0x3C4A6 + …`) → n = 1, choice 0 (Final A dialogue); else → n = 2, choice 1 (Final B dialogue);
     otherwise (6B cleared at least once, or a solo shot type) → n = 3 (the choice menu, op 21).
   - The choice is copied into GM flags bits 11–12 (`flags = flags & ~0x1800 | (choice & 3) << 11`).
4. Stage 6B (idx 7), `n > 5` and clock counter `stats+0x28 > 11` → n = 5 (stage-end entry instead of the
   mid-fight lines 6–9).
5. `entry = n; ip = offset[n]`; line VMs' script index = −1 (so the first clear is skipped);
   `textbox = 1; font_size = 15; text_colour[k] = tbl0x4C7180[shot*4 + k]; outline[k] = 0; skippable = 1;
   min_wait = 6; text_speaker = 0; new_box = 1; next_line = 0; active_speaker = 0xFF`.
6. Side effects on the playfield: cancel all bullets into items (`BulletMgr_CancelAll(mode 1)`, 08 §cancels),
   destroy every killable enemy (0x42EFB0 with bonus 0: enemies with flag bit0 set, bit1 clear, flag2 bit6
   clear die; enemies with flag bit7 drop item type 6 and a score popup; their death sub is run), and set every
   item to auto-collect (`item+0x2D7 = 1`, velocity (0, −0.5, 0)).

## 5. Per-frame interpreter (`Msg_Run`) — VERIFIED

```
if entry < 0: return
if release_counter > 0: release_counter -= 1
if skippable and (input & SKIP): clock.set(ip.time)          # fast-forward
if player.state != 2 (not dying): set all items to auto-collect   (same as §4.6)
loop:
    if clock.current < ip.time: break
    execute ip (table below); ops that "stall" leave the function WITHOUT ticking the clock
    ip = next instruction
clock.tick()
stalled/normal end: run face[0..3], line[0..1], boss_card, VM 0x22AB0
if clock < 60 and skippable and (input & SKIP): clock.set(60)
```

Input bits (word 0x164D52C current / 0x164D534 previous frame; keyboard map VERIFIED in `Input_Poll`
0x43D970): `SHOT 0x1` (Z), `UP 0x10`, `DOWN 0x20`, `SKIP 0x100` (Ctrl). "pressed" = bit set now and not set in
the previous frame.

### 5.1 Opcode table (all 23 opcodes; all appear in the data)

Arguments are given in file order. `str` = XOR-0x77 string (§2). Count = occurrences in all files.

| op | name | args | behaviour | count |
|---|---|---|---|---|
| 0 | end | — | `entry = −1` (dialogue over); returns immediately | 136 |
| 1 | face_script | i16 k, i16 script | start ANM `script` of face file k on face[k]; then pos_offset.x = −112 if the VM's sprite width > 128 else 0 (see note) | 256 |
| 2 | face_sprite | i16 k, i16 sprite | set sprite of face[k]; pos_offset = (−208, −50) if sprite width > 256, x = −80 if > 128, else x = 0 | 1 |
| 3 | text_line | i16 speaker, i16 line, str | if line == 0 and line[1] has a script, clear line[1] (render `" "`); start text.anm script `line` on line[line]; render `str` with speaker's colour; wait_count = 0 | 18 |
| 4 | wait | i32 n | **stall** handling, see §5.2 | 1565 |
| 5 | face_interrupt | i16 k, u8 i, pad | `face[k].pending_interrupt = i` | 177 |
| 6 | release_timeline | — | `release_counter += 1` → `Msg_IsBlocking` is false for this frame, so a timeline op 7 waiting on the dialogue passes (used to spawn a boss mid-dialogue) | 40 |
| 7 | bgm | i32 n | n < 0: stop music (`Music_Stop` 0x448081). Else: HUD VM (HudImpl+0x3230) runs script 3 of `stgNtxt.anm` with sprite n+3 (song-title card); play preloaded BGM slot n (the 3 STD names, 02 §4); mark music-room entry `tbl0x4C7240[stage*3+n]` as heard (`0x164CF14[idx] = 1`) unless replay/flag bit 1 | 49 |
| 8 | boss_card | i16, i16, str | starts script 1 of face file 2 on `boss_card` (the boss name plate, `face_stNN_name`), wait_count = 0. **The arguments are ignored** (the string is the boss name) | 35 |
| 9 | stage_end | i32 (always 0, ignored) | stage-clear: computes and shows the clear bonus, advances the clock counter, sets GM flag 0x200 (§7, 11/12) | 37 |
| 10 | halt | — | **stall forever** (ip is not advanced; the game manager ends the stage) | 37 |
| 11 | halt_final | — | stages 6A/6B/Extra: GM flags = flags & ~0x60 \| 0x40 (game-clear trigger); then stall like op 10 | 36 |
| 12 | bgm_fade | — | `Music_Fade(4.0 s)` (0x4480F8; WAV mode: 4 s divided by game speed when slowed) | 37 |
| 13 | skippable | u8 b, pad×3 | `skippable = b` | 204 |
| 14 | flash | — | full-screen ScreenEffect type 4: colour 0xFFFFFF, duration 442 frames, alpha = 255·t/442 (fade to white), draw prio 21; HUD full-repaint counter (0x17CE8CC) = 442 | 12 |
| 15 | speaker4 | u32 who, i32 s0, s1, s2, s3 | highlight speaker `who` (§5.3); for each k with s_k ≥ 0 set face[k]'s sprite to s_k; text_speaker = who; new_box = 1 | 216 |
| 16 | text | str | if new_box: clear line[1] (if it has a script) and next_line = 0. Start text.anm script `next_line` on line[next_line], render `str` with `text_colour[text_speaker]`; wait_count = 0; new_box = 0; next_line += 1 | 2214 |
| 17 | speaker | u32 who, i32 s | like op 15 but only face[who]'s sprite is set (if s ≥ 0) | 1336 |
| 18 | textbox | u8 b, pad×3 | `textbox = b` | 4 |
| 19 | text0 | str | start script 0 on line[0], render with text_colour[0]; wait_count = 0 | 4 |
| 20 | text1 | str | start script 1 on line[1], render with text_colour[0]; wait_count = 0 | 4 |
| 21 | choice | i32 n | final-route menu, **stall** handling §5.4 | 4 |
| 22 | choice_jump | — | GM flags bits 11–12 = choice; `Msg_Start(choice + 1)`; continue interpreting the new entry in the same frame | 4 |

Note on ops 1/2: they write `pos_offset` (AnmVm+0x288). The 2D draw path does not read pos_offset and the face
scripts never set flag bit 8, so this has **no visible effect** (VERIFIED for `AnmManager_Draw2DSnap`).

Negative sprite arguments in ops 15/17 (data uses −1 and −2) mean "keep".

### 5.2 wait (op 4) — VERIFIED

```
if skippable and (input & SKIP): advance                        # instant
elif SHOT pressed and wait_count >= min_wait: new_box = 1; min_wait = 8; advance
elif wait_count < n: wait_count += 1; STALL (clock not ticked)
else: new_box = 1; min_wait = 30; advance                       # timeout
```

`wait_count` is reset to 0 by every text op (3, 8, 16, 19, 20), so it counts frames since the last text.
Data uses n = 500 for normal boxes (≈8.3 s auto-advance) and 800 before stage end.

### 5.3 Speaker highlight (ops 15, 17) — VERIFIED

If `who != active_speaker`: for each k in 0..3, face[k].pending_interrupt = 4, except the previously active
speaker, which gets 4 if it is on the same side as `who` (`prev/2 == who/2`) else 6. Then (always)
face[who].pending_interrupt = 3; active_speaker = who.

Portrait script interrupts (player face scripts, e.g. `face_rm00` script 1, and stage face script 2; VERIFIED
from anm dump):

| interrupt | effect |
|---|---|
| (start) | hidden, alpha 0, parked off-screen (player side x = −224, opponent x = 416, y = 128) |
| 1 | slide in over 30 frames (player to x 48, opponent to x 272), alpha → 255 |
| 3 | highlight: colour → (255,255,255) and alpha → 255 in 15 frames, move to the home position (48,128) / (272,128) |
| 4 | dim: colour → (128,128,128), alpha → 192, move 24 px outward and 8 px down (z 0.01) over 15 frames |
| 6 | dim colour only (128,128,128) over 15 frames (opponent script also moves like 4) |
| 5 | exit: alpha → 0 and slide back off-screen over 30 frames, then hide |

### 5.4 choice (op 21) — VERIFIED

Each frame while on op 21: UP pressed → choice = 0 (sfx 12 `se_select00` if it changed); DOWN pressed →
choice = 1 (sfx 12 if changed). line[choice].colour = 0xFFFFFFFF, the other line's colour = 0xE0606060.
Then: if SHOT pressed and wait_count ≥ 60 → sfx 10 (`se_ok00`), advance; elif wait_count < n → wait_count += 1,
STALL; else (timeout, n = 1200 in data) → new_box = 1, min_wait = 30, advance with the current choice.
The two lines are rendered beforehand by ops 19/20 ("目に見えるものを追う（FinalA）" / "勘を信じる（FinalB）").
Op 21 ignores SKIP.

## 6. Text rendering — VERIFIED (font/GDI) / INFERRED (exact pixels)

Dialogue text is drawn into the dynamic texture `text.anm` (ANM slot 0, entry name `@`, 512×256 render
target, 03/04). Each line VM uses a sprite that is a 320×17 strip of that texture:

| VM | text.anm script | sprite | strip (x, y, w, h) | screen pos (top-left anchor) | fade-in |
|---|---|---|---|---|---|
| line[0] | 0 | 0 | (0, 18, 320, 17) | (75, 390) | alpha 0 → 255 in 12 frames |
| line[1] | 1 | 1 | (0, 36, 320, 17) | (75, 410) | same |

So a new line fades in over 12 frames each time it is (re)started; the lines are not typed out character by
character.

`Text_RenderToVm(vm, colour, outline, fmt, …)`: `vsprintf` into a 128-byte buffer, then calls the
rasteriser with the sprite's texture, its rect (x = u0, y = v0 in texture pixels, w = texture-row width), the
font size bytes `vm+0x298/0x299` (both = font_size = 15) and the sprite's scale factors; finally sets VM flag bit 0.

Rasteriser (font size > 8 path, always taken for dialogue, 0x43F9C8):
- Work surface: 1024×64 A1R5G5B5 off-screen surface (`0x1653620`), cleared to transparent over
  (0, 0, 2w, 2·size+6). Text is drawn at **2× resolution** and then downsampled.
- Font: GDI `CreateFont(height = 2·size − 2 = 28, weight 600, charset SHIFTJIS, quality ANTIALIASED,
  pitch FIXED|FF_ROMAN, face "ＭＳ ゴシック")` → ~14 px glyphs at 1×.
- Background mode transparent. Outline: text drawn in black at four offsets, then in `colour` at the centre
  `(2·x + 2, 2)`. With `outline == −1` the offsets are ±1 px (2× space); otherwise (dialogue, outline = 0) ±2 px:
  `(2x+4,2) (2x,2) (2x+2,0) (2x+2,4)`.
- Copy: `D3DXLoadSurfaceFromSurface` from src rect (0, 0, min(2w,1024), 2·size) to the texture's dest rect
  (0, y, w, y + size·scale_y) with filter TRIANGLE (4) → 2:1 box-like downsample. The whole row strip is
  replaced (anything left of x is cleared too).
- (Font size ≤ 8 path 0x43FCE9: weight 400, height 16 at 2×; used by other callers.)

Text colours (`tbl 0x4C7180`, identical for all 12 shot types), COLORREF 0x00BBGGRR:

| speaker | value | RGB |
|---|---|---|
| 0 | 0x00E8F0FF | (255, 240, 232) warm white |
| 1 | 0x00F0E8FF | (255, 232, 240) pinkish white |
| 2, 3 | 0x00FFE8F0 | (240, 232, 255) bluish white |

Reference runs (wine) do **not** use MS Gothic (changelog/answers_impl17.md §Q6, VERIFIED with WINEDEBUG=+font):
- the Shift-JIS face name is mis-converted (process code page ≠ 932);
- wine falls back to Droid Sans Fallback (proportional) at ppem 21 with synthetic bold;
- all text strips in `menu_ref` screenshots and recorded sessions have that layout (full-width ≈ 11 px, digits 6.5 px at
  1×).
On Windows, MS Gothic is monospaced: 7 / 14 px. Strings contain no tabs; columns come only from spaces in the format
strings.

A reimplementation may rasterise with any Japanese font at ~14 px with a 1 px black outline and the colours
above (INFERRED equivalence).

## 7. Drawing (`Msg_Draw`, inside HUD draw prio 17) — VERIFIED

Only when `entry ≥ 0`:
1. Portraits: pair (face[0], face[1]) drawn back-to-front by `pos.z` (larger z first), then pair
   (face[2], face[3]) the same way, with `AnmManager_Draw2DSnap`; flush.
2. If `textbox`: one untextured quad (triangle strip, XYZRHW|DIFFUSE, z-write off) from
   (x0+16, 384) – (x0+368, 384+h) where x0 = playfield screen x (32): top vertices colour 0xD0000000,
   bottom 0x90000000 (black, alpha 208 → 144). `h = 48` once the clock is ≥ 60, else `h = clock·48/60`
   (box grows down during the first second).
3. line[0], line[1], boss_card, VM 0x22AB0 (Draw2DSnap).

Boss name card (op 8, stage face file script 1, e.g. `face_st01_name`): top-left at (144, 352), fades in
over 30 frames, out at frame 350–380.

## 8. Interaction with the rest of the game

### 8.1 ECL timeline — VERIFIED
- Timeline op 6 `msg n` → `Hud_StartMsg(n)`.
- Timeline op 7 `wait_msg`: while `Msg_IsBlocking()` → `clock.Decrement()` and stop for this frame; i.e. the
  timeline stalls until the dialogue ends or the dialogue executes op 6 (release for one frame).
- Timeline op 0 (spawn) is suppressed while a boss/dialogue state is active (07 §timeline).
- Ordering: the timeline runs in the enemy calc (prio 11), the dialogue in the HUD calc (prio 15) of the same
  frame, so a dialogue started by the timeline executes its first instructions in the same frame.

### 8.2 What `Msg_IsActive` blocks — VERIFIED (callers)
While a dialogue runs: the player cannot start shooting (shot timer not started, 0x451500/0x44AEC0) and
cannot bomb (0x44C650); certain play counters (0x164D318/0x164D31C, per-frame stats) are not advanced
(0x44C390); boss life-bar refresh and the bonus/"spell" timer bookkeeping in the enemy manager
(0x42C3B0/0x42C660) are skipped; option/laser shot updates hold (0x4505D0, 0x450840); the replay manager may
skip frames when fast-forwarding during dialogue (0x452490; VERIFIED, see 15 §3.3 playback speed).

### 8.3 ECL ops touching the HUD (from 07 §7) — VERIFIED
| ECL op | effect |
|---|---|
| 148 | `g_Hud+0x20 = n` (INFERRED: boss/spell display mode); global 0x164D30C += 1800 |
| 158 | boss life-bar marker k at a/max, b/max, style c (12) |
| 179 | HUD clock VM (HudImpl+0x2156C) runs `times.anm` script 2 with sprite = clock counter (stats+0x28) |
| 180 | `HudImpl+0x2175F = 0` (INFERRED: hide a HUD element; detail in 12) |
| 181 | if clock counter < 12: sfx 0x2D (`se_ok00`), counter += 1, clock VM sprite = counter, interrupt 2 if it reached 12 else 1 |

### 8.4 Music
- op 7 plays BGM slot n of the stage (preloaded from the STD header, 02 §4); n = −1 stops music.
- op 12 fades the music out over 4 s.
- The song-title card is sprite n+3 of `stgNtxt.anm` script 3.

## 9. Stage end (op 9) — summary (details in 11/12)

VERIFIED actions: store for the bonus panel the current power, `stats+0x2C` (point items), time
(`stats+0x3C`), graze (`stats+0x04`), the per-stage clear bonus `tbl 0x4C7158[stage]`
(1 000 000, 1 500 000, 2 000 000, 2 500 000, 2 500 000, 3 000 000, 4 000 000, 6 000 000, 6 660 000),
`clock*30 + 660` before and after advancing the clock counter by `ClockAdvance()` (stages 1–5: +2 if
time < `stats+0x40` else +1; 6A/6B +0; Extra +4); `stage_end = 1`; GM flag 0x200.
Stage-clear VM impl+0x212C8 (VERIFIED 0x433DB3, RE-Q25): stages 1–5 restart it with `ascii.anm` script 3 and sprite
**0x80 + clock advance** (129 / 130 = the 48×16 "+1" / "+2" clock plates at (0, 16) / (48, 16)); 6A/6B/Ex only clear
its ip (+0x220 := NULL, so it never runs); then interrupt 1 in every case.
Stages 1–5 also start the bonus panel VMs and capture the playfield (0x406A30 with rect 32,16,384,448): VM
impl+0x3778 `loadingNN.anm` script 0, VM impl+0x3CC0 `capture.anm` script 1, and the 8 VMs impl+0x3F64 + k·0x2A4
`capture.anm` script 2 with I4 (+0x120) := 4k + 3 and alpha byte := 64 − 2k (k = 0..7); these are drawn as mode-5 quads
by `Hud_DrawBoss` (12 §5.4).
Stages 6A/6B/Ex set `stats+0x34 = −1` instead. Team Youmu&Yuyuko and solo Youmu/Yuyuko (shot 3, 10, 11) get
+1 bomb (sfx 0x23 `se_cardget`) if they have fewer than 3 bombs (stages 1–5 only).

## 10. Sound ids used here (table 0x4C8040: i32 wav index, i16 volume (1/100 dB), i16 priority) — VERIFIED
10 `se_ok00`, 12 `se_select00`, 0x23 `se_cardget`, 0x2D `se_ok00` (−1 dB). (Full table: 46 ids, 0..45.)
