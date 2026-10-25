# 15 — Files: th08.cfg, score.dat, replays

Tools in `tools/`: `cfg.py` (th08.cfg check), `scoredat.py` (score.dat decode and validation), `rpy.py` (replay decode and validation).

Current results:
- the real `th08.cfg`: 0 problems
- the real `score.dat`: 0 problems
- `demorpy0..3.rpy`: 0 problems each

Addresses are VAs in th08.exe. GM = game manager 0x160F508.

---

## 1. th08.cfg — VERIFIED (`Config_Load` 0x4477B1)

### 1.1 Layout

The file is 60 bytes (0x3C). In memory it lives at 0x17CE870 (= Supervisor+0x118). `*(GM+4)` points to the copy used by gameplay.

| off | type | field | valid | default |
|---|---|---|---|---|
| 0x00 | i16 | pad button → Shot | — | 0 |
| 0x02 | i16 | pad button → Bomb | — | 1 |
| 0x04 | i16 | pad button → Focus | — | 2 |
| 0x06 | i16 | pad button → Pause (ESC) | — | 4 |
| 0x08 | i16 | pad button → Up | — | −1 |
| 0x0A | i16 | pad button → Down | — | −1 |
| 0x0C | i16 | pad button → Left | — | −1 |
| 0x0E | i16 | pad button → Right | — | −1 |
| 0x10 | i16 | pad button → Skip (Ctrl) | — | 3 |
| 0x12 | 2 B | padding | | |
| 0x14 | u32 | version | must be 0x00080001 | 0x80001 |
| 0x18 | i16 | analog stick X threshold | — | 600 |
| 0x1A | i16 | analog stick Y threshold | — | 600 |
| 0x1C | u8 | starting lives option | < 7 | 2 |
| 0x1D | u8 | unused (reset to 3 by the option "Reset") | < 4 | 3 |
| 0x1E | u8 | force 16-bit colour | < 2 | 0; forced to 1 if the desktop is 16-bit |
| 0x1F | u8 | music: 0 off, 1 WAV (thbgm.dat), 2 MIDI | < 3 | 1 if `thbgm.dat` has a valid header ("ZWAV", version 1, 0x800), else 2 |
| 0x20 | u8 | sound effects on | < 2 | 1 |
| 0x21 | u8 | last chosen difficulty | < 6 | 1 |
| 0x22 | u8 | windowed | < 2 | 0 |
| 0x23 | u8 | frame skip: the draw chain runs when an i8 counter (incremented every frame) reaches this value, then the counter is reset. So 0 **and** 1 = draw every frame, 2 = every 2nd frame (VERIFIED `Frame_Run`, changelog/answers_phase7.md §6). Not in the option menu. | < 3 | 0 |
| 0x24 | u8 | effect level (0 none, 1 half, 2 all; see 13 §2) | < 3 | 2 |
| 0x25 | u8 | slow mode | < 2 | 0 |
| 0x26 | u8 | joystick "shot also focuses" toggle; only read by `Input_PollJoystick` (changelog/answers_phase7.md §5) — VERIFIED | < 2 | 0 at load, 1 after a key-config reset |
| 0x27 | u8 | BGM volume 0..100 | — | 100 |
| 0x28 | u8 | SE volume 0..100 | — | 80 |
| 0x29..0x37 | | unused | | 0 |
| 0x38 | u32 | debug/driver flags | — | bit 0 is always forced to 1 |

Pad buttons are DirectInput button indices; −1 means no button.

**Keyboard mapping** (`Input_Poll` 0x43D970, VERIFIED disassembly, RE-Q25). The raw word starts at 0; each mapped key
whose state byte has bit 7 set ORs its bits in; then the joystick bits are ORed (below). With the DirectInput keyboard
(device 0x17CE768 ≠ 0) the state comes from `GetDeviceState` (DIK scan codes); when the device pointer is null,
from `GetKeyboardState` (VK codes). The DirectInput keyboard is created at boot (`FUN_00446A37`) unless config +0x38
bit 11 (Supervisor +0x150, "DirectInput off") is set or creation fails, so the **DIK path is the normal one**. A failed
`GetDeviceState` (any non-zero result, `DIERR_INPUTLOST` included) calls `Acquire` and returns the joystick bits only:
the raw word is zeroed after the call and passed to `Input_PollJoystick` with no key bits (VERIFIED 0x43DD3C–0x43DD99,
RE-Q41). No other keys are mapped; in particular the DIK path maps only the main Enter (0x1C), not keypad Enter (0x9C),
while the VK path's `VK_RETURN` 0x0D covers both, and VK Shift/Ctrl (0x10/0x11) cover both sides
(full table re-verified from the disassembly, RE-Q41).
Device set-up (`FUN_00446A37`, VERIFIED RE-Q46): `DirectInput8Create`, `CreateDevice(GUID_SysKeyboard)`,
`SetDataFormat(c_dfDIKeyboard)`, `SetCooperativeLevel(hwnd, 0x16 = DISCL_NONEXCLUSIVE | DISCL_FOREGROUND |
DISCL_NOWINKEY)`, one `Acquire`; the joystick (if enumerated) gets `SetCooperativeLevel(hwnd, 0x0A = NONEXCLUSIVE |
BACKGROUND)`. With FOREGROUND the keyboard is unacquired while the window is not the foreground window, so
`GetDeviceState` fails (and the poll's `Acquire` fails) every poll until the window is active again: keys read as
none, pad bits still count.

| bits | VK path | DIK path |
|---|---|---|
| 0x10 Up / 0x20 Down / 0x40 Left / 0x80 Right | arrows 0x26/0x28/0x25/0x27; numpad 8/2/4/6 (0x68/0x62/0x64/0x66) | arrows 0xC8/0xD0/0xCB/0xCD; numpad 0x48/0x50/0x4B/0x4D |
| diagonals 0x50 / 0x90 / 0x60 / 0xA0 | numpad 7/9/1/3 (0x67/0x69/0x61/0x63) | numpad 0x47/0x49/0x4F/0x51 |
| 0x1 Shot | Z 0x5A | Z 0x2C |
| 0x2 Bomb | X 0x58 | X 0x2D |
| 0x4 Focus | Shift 0x10 | LShift 0x2A, RShift 0x36 |
| 0x8 ESC | Esc 0x1B | Esc 0x01 |
| 0x100 Skip | Ctrl 0x11 | LCtrl 0x1D, RCtrl 0x9D |
| 0x200 / 0x400 / 0x4000 | Q 0x51 / S 0x53 / R 0x52 | Q 0x10 / S 0x1F / R 0x13 |
| 0x800 | Home 0x24, P 0x50 | Home 0xC7, P 0x19 |
| 0x1000 Enter | Enter 0x0D | Enter 0x1C (main Enter only) |
| 0x2000 | D 0x44 | D 0x20 |

**Joystick mapping** (`Input_PollJoystick` 0x43D120, both the winmm `joyGetPosEx` path and the DirectInput path;
VERIFIED, changelog/answers_phase7.md §5):
- Button fields +0x00…+0x10 in file order map to input bits 0x1 (Shot), 0x2 (Bomb), 0x4 (Focus), 0x8 (Pause/ESC),
  0x10 (Up), 0x20 (Down), 0x40 (Left), 0x80 (Right), 0x100 (Skip). Index < 0 = not mapped. A button is down when
  winmm `dwButtons` bit n is set, or DirectInput `rgbButtons[n] & 0x80`.
- Directions, winmm: q = range/4 around the axis centre (JOYCAPS read at 0x164D388: wXmin/wXmax 0x164D3AC/0x164D3B0,
  wYmin/wYmax 0x164D3B4/0x164D3B8); x > centre + q → Right, x < centre − q → Left, y > centre + q → Down,
  y < centre − q → Up. DirectInput (DIJOYSTATE): lX > +0x18 → Right, lX < −(+0x18) → Left, lY > +0x1A → Down,
  lY < −(+0x1A) → Up (defaults 600). Axis range: at device creation (`FUN_00446A37`: `SetDataFormat(c_dfDIJoystick)`,
  `SetCooperativeLevel(hwnd, 10)`, `GetCapabilities`, `EnumObjects(0x445E3D, 0, DIDFT_ALL)`) the callback sets
  `DIPROP_RANGE` {lMin −1000, lMax +1000} (`DIPH_BYID`, dwObj = the object's dwType) on **every** object whose type has
  bit 0 or 1 set (relative or absolute axis); a failing `SetProperty` stops the enumeration. So the default threshold
  600 = 60 % deflection (VERIFIED 0x445E3D, RE-Q41).
- **+0x26 ≠ 0 "shot also focuses"** (0x43EF50): u16 counter 0x164D51C, once per poll. Shot button down: counter += 1
  if < 20; if counter > 9 add Focus. Shot button up: counter < 11 → counter := 0, else counter −= 10 and add Focus.
  So holding the pad's shot button for 10 polls also focuses, and focus stays for one poll after release. Keyboard
  input is unaffected.

Debug flags at +0x38 (VERIFIED from the log strings printed at load):

| bit | effect |
|---|---|
| 1 | no vertex buffer |
| 2 | force 16-bit textures |
| 3, 4 | clear the back buffer |
| 5 | Gouraud shading off |
| 6 | depth test off |
| 8 | colour composite off |
| 9 | (logged) |
| 10 | fog off: FOGENABLE 0 at device init, `Supervisor_FogOn/Off` never called (06 §8; VERIFIED from code) |
| 11–15 | DirectInput off, BGM read into memory, bit 14 = no vsync, … |

The real file has 0x4001.

### 1.2 Load and save

At boot the game reads `th08.cfg` from disk. The whole file is reset to defaults when any of these hold:
- the file is missing;
- the size is not 0x3C;
- the version is wrong;
- any byte field is out of its valid range.

When that happens it logs "コンフィグデータが異常でしたので再初期化しました" (a missing file logs
"コンフィグデータが見つからないので初期化しました" instead). After the music-mode probe it writes the file back straight away. It writes the file again when WinMain exits.

In the defaults path a missing `./thbgm.dat` sets music mode 2 (MIDI) and passes the text "wave データが無いので、midi
にします" to 0x437CDD, which is an **empty function** (a compiled-out debug print; the scene-switch trace "scene %d -> %d"
goes there too): nothing is shown and nothing is logged (VERIFIED 0x4477B1 / 0x437CDD, RE-Q40). A thbgm.dat that exists
with a bad header ("ZWAV", 1, 0x800) logs "BGM データのバージョンが違います" and fails the start-up, in both paths.

### 1.3 Option menu (`MainMenu_Option` 0x46837B) — VERIFIED

| item | effect |
|---|---|
| 0 Player (lives) | Cycles +0x1C. The maximum depends on the total play count (PLST total block [0], 0x164CED0): < 30 → 0..4, < 60 → 0..5, otherwise 0..6. |
| 1 Color | Toggles +0x1E. |
| 2 BGM | Cycles +0x1F (VERIFIED 0x46837B, RE-Q46). Left: `Music_Stop` (in the **old** mode, 02 §4), then if the old mode is 2 `Midi_Stop`, `Midi_Open(30)`, `Midi_Play` (init.mid), then mode − 1 (0 → 2). Right: `Music_Stop` (old mode), mode + 1 (2 → 0), no init.mid. Both then `Music_Play(8, "bgm/th08_01.mid")` + `Music_PlaySlot(8, 0)` in the new mode (title music; nothing in mode 0). So the old mode's music is always stopped (WAV: queued command 3/4; MIDI: at once). |
| 3 BGM volume, 4 SE volume | Two steps run in this order every frame (VERIFIED code + `options_cfg` session). (a) Left/Right **edge**: value ∓/± 4, clamped to 0..100. (b) Left/Right edge, **or** that direction held while the unchanged-input counter 0x164D53C > 29 (not the every-8th-frame repeat flag): value ∓/± 1 if it is still above 0 / below 100. So one press steps ±5 (e.g. 80 → 75 → 70 → 65; 98 → 100), and a held key adds ±1 on every frame from the 30th unchanged frame on. Every step sends the set-volume command. While the cursor is on item 3 or 4, SE 0x1D plays whenever the menu frame counter (+0x1442C) is a multiple of 50 (volume preview). |
| 5 Mode | Toggles window mode +0x22. |
| 6 Slow | Toggles +0x25. |
| 7 Reset | Sets lives 2, +0x1D 3, music 1, SE 1, slow 0; no music call (VERIFIED RE-Q46): a MIDI song playing in mode 2 keeps playing (the WAV-mode paths never call a MIDI function, so it keeps looping under any WAV music until a mode-2 `Midi_Stop`, e.g. the next BGM change back to MIDI, or exit). |
| 8 KeyConfig | Opens key config (`MainMenu_KeyConfig` 0x469636, screen 3; details below). |
| 9 Quit | Leaves the option menu (confirm). Bomb/ESC on any other item moves the cursor to Quit (SE 11); Bomb/ESC on Quit leaves. Leaving: SE 11, top-menu cursor 7, screen 0 at once (return 1, or 5 when colour depth or window mode changed). The cursor moves from the first call; Left/Right/confirm/cancel are accepted from screen frame 4 (VERIFIED 0x46837B, RE-Q25). |

**Key config (screen 3)** — VERIFIED code; on-screen labels from the `options_cfg` session:
- 12 rows, top to bottom: Shot (+0x00), Bomb (+0x02), Slow (+0x04), Skip (+0x10), Pause (+0x06), Up (+0x08),
  Down (+0x0A), Left (+0x0C), Right (+0x0E), ShotSlow (+0x26 Off/On), Reset, Quit. Note the order differs from the
  file order: Skip comes 4th. Each button row shows its pad button number as two digits; no number for −1.
- On entry the 9 button fields are copied to a working copy (menu +0x14440…+0x14451), and the **live** Up/Down
  fields (+0x08/+0x0A) are set to −1 while the screen is open.
- Cursor: `Menu_CursorUpDown(12)`. Cursor 0 is set by the option menu's item-8 confirm (cursor := 0, then
  `Menu_SetScreen(3)`, SE 10, return 1); the key-config entry block does not write the cursor (VERIFIED 0x46837B /
  0x469636, RE-Q42). Likewise the option screen's cursor 0 from top item 7 is written by the top handler (at the
  confirm, before and after its in-place `MainMenu_Option` call, and again in the switch call P + 29, 12 §2.2); the
  option entry block does not write it.
- **Call order** (every call, VERIFIED 0x469636, RE-Q42). There is no +0x1442C input gate: input works from the entry
  call. 1) Sub-state 0 only: entry block (call 0: range interrupt 4, row sprites, working copy, live Up/Down := −1,
  digit pairs once, +0x08 := −1), sub-state := 1, render the 12 descriptions; then in the same call:
  2) `Menu_CursorUpDown(12)`, row highlight if it moved; 3) change test (description VM); 4) the 9 digit pairs
  (0x469FA9) from the working copy; 5) ShotSlow VMs 75/76 from cfg +0x26; 6) joystick scan + assign (SE 10);
  7) 0x4C868C := scan result; 8) Left / Right toggles of +0x26 on row 9; 9) input resets the idle counter +0xC294;
  10) +0xC294 ≥ 3600 → exit; 11) confirm: Reset (working copy := defaults, +0x26 := 1) or Quit → exit;
  12) counter increments, return 1. So the digits / ShotSlow plates show an assign, a toggle or a Reset **one call
  later** (the next call's steps 4–5), and steps 1–5 also run in the Quit / idle-exit call (the exit itself is after
  them, at steps 10/11).
- **Assigning** works only from the joystick: each frame the raw DirectInput button array (`Input_PollJoystickRaw`,
  32 bytes) is scanned for the first button with bit 7 set (index 0–31, or 32 when none is down). If one is found,
  differs from the button found on the previous frame (global i16 0x4C868C, i.e. an edge), and the cursor is on rows
  0–8: every row of the working copy
  that already holds that button gets the row's old button (swap), the row gets the new button, SE 10.
  Keyboard keys cannot change a binding. 0x4C868C := the scan result (0–31, or **32** when no button is down) on every
  key-config call, whatever the cursor; it is a static initialised to 32 and never reset, so it carries over between
  key-config visits and main-menu registrations (VERIFIED 0x469636, RE-Q37).
- ShotSlow row: Left or Right edge toggles +0x26 (live, not the working copy).
- Confirm (Shot/Enter edge, mask 0x1001) on Reset: SE 10, working copy := table 0x4C78C8 (0, 1, 2, 4, −1, −1, −1, −1,
  3), +0x26 := 1. On Quit: SE 11, screen 2 with cursor 8, working copy written back to +0x00…+0x11 (this also
  restores Up/Down). Bomb/ESC do nothing here.
- After 3600 frames without input the screen quits the same way (idle rule below).

**Key config VMs** (title01 VMs 44–76, all with label 4 = range interrupt 4 sent by the entry block; VERIFIED 0x469636 +
anm dump, RE-Q37). Positions come from the scripts; the handler only sets sprites and the enabled bit (flag bit 1):
- VM 44 heading (sprite 33).
- VMs 45–56 = rows 0–11 (Shot … Quit, the order above): selected = the script's sprite, others sprite + 1; set in the
  entry block (all + 1, cursor row base) and again on every cursor move.
- VMs 57–74 = 9 digit pairs (tens VM 57 + 2r, ones VM 58 + 2r) for rows r = 0–8, from the **working copy** fields
  Shot, Bomb, Slow, Skip, Pause, Up, Down, Left, Right (menu +0x14440, +0x14442, +0x14444, +0x14450, +0x14446, +0x14448,
  +0x1444A, +0x1444C, +0x1444E). Every call, per pair (0x469FA9): value < 0 → both VMs disabled; else tens sprite =
  script sprite + 2·(v / 10), ones sprite = script sprite + 2·(v mod 10), both enabled. The number shown is the
  0-based button index.
- VMs 75 / 76 = ShotSlow Off / On: every call both get sprite + 1, then VM 75 + cfg +0x26 its base sprite.
- Description VMs: 12 §2.2 (12 strings, pointer to the cursor row's VM on every cursor change).

**Idle exit** (option and key config; VERIFIED 0x46837B / 0x469636, RE-Q37). Counter menu +0xC294: zeroed by
`Menu_SetScreen` (and by the top menu's manual switch to screen 2), += 1 at the end of every call. Each call, before
the confirm/cancel tests (in the option menu after the value changes and the SE 0x1D preview): g_InputRawCur ≠ 0 →
+0xC294 := 0 (only the polled input word counts, not raw pad buttons that no binding maps); then +0xC294 ≥ 3600 →
exit. So the exit comes in call 3600 after the last call with input (call 3600 after the entry when there was none).
The option menu skips the reset and the test while +0x1442C < 4 (the counter still counts). The exit is the Quit
path: option = SE 11, cursor 7, screen 0, return 1 / 5; key config = SE 11, write-back, screen 2, cursor 8.

When leaving the menu, if +0x1E or +0x22 changed, the function returns 5: the whole program restarts with the new
mode (00 §2 "Restart").

---

## 2. score.dat — VERIFIED

### 2.1 Container

Load: `ScoreDat_Load` 0x45A5E0. Save: `ScoreDat_Save` 0x453D0D (`this` = Result object).

The file is encrypted on disk with two layers, outermost first:
1. Archive block cipher `Crypt(key 0x59, step 0x79, block 0x100, limit 0xC00)` over the whole file (01 §2). Decrypt with 0x43E1D0, encrypt with 0x43E4A0.
2. A rolling XOR that starts at byte 1:
   - Decode: `acc = 0`; for i = 1 .. size−2: `acc = rotl8(acc + b[i], 3)`, then `b[i+1] ^= acc`. Because the loop runs forward, `b[i]` is already decoded (plain) when it is used, and b[1] is never changed.
   - Encode (save): the same running `acc`, computed from the plain bytes; each byte from index 2 on is XOR-ed with it. The saver processes the bytes so that the decoder above recovers them (VERIFIED by round-trip with the real file).

Header after decoding (0x1C bytes):

| off | type | value |
|---|---|---|
| 0x00 | u8 | 0 |
| 0x01 | u8 | random (Rng_Range16(256)); because of the XOR it acts as the key |
| 0x02 | u16 | checksum = sum of bytes 4..end of the decoded **compressed** file, mod 65536 |
| 0x04 | u16 | version 1 |
| 0x06 | u8 | random (Rng_Range16(256)) |
| 0x08 | u32 | header size 0x1C |
| 0x0C | u32 | runtime pointer, garbage |
| 0x10 | u32 | decompressed size including the header |
| 0x14 | u32 | decompressed body size (= +0x10 − 0x1C) |
| 0x18 | u32 | compressed size (= file size − 0x1C) |

The body from +0x1C is LZSS (01 §3). It decompresses to a list of chunks.

**Saving consumes 2 RNG draws** (two Rng_Range16(256)). Saves happen only outside gameplay, but the RNG state carries over into the next game (11 §1.5).

Each chunk starts with this header:

| off | type | field |
|---|---|---|
| 0 | char[4] | magic |
| 4 | u16 | size |
| 6 | u16 | size (again) |
| 8 | u8 | version |
| 9 | 3 B | padding |

Load rejects the file, and the game then starts from defaults, when any of these hold:
- size < 0x1C;
- bad checksum;
- header size ≠ 0x1C;
- version ≠ 1;
- a chunk with size 0;
- the VRSM exe check fails (§2.10);
- no TH8K chunk with version 1.

Each `ScoreDat_Get*` walks the chunk list and only accepts a chunk whose magic and version match.

Write order (VERIFIED from the saver):
1. TH8K
2. HSCR × n
3. CLRD × 13
4. CATK × (cards whose magic is set)
5. PSCR × (shots whose used flag +0x175 is set)
6. LSNM
7. FLSP
8. PLST (the run-time clock is updated just before)
9. VRSM

| chunk | size | ver | count |
|---|---|---|---|
| TH8K | 0x0C | 1 | 1 (header only) |
| HSCR | 0x168 | 4 | top 10 per (difficulty 0..4, shot 0..11), default entries skipped |
| CLRD | 0x24 | 4 | 13 |
| CATK | 0x22C | 3 | up to 222 |
| PSCR | 0x178 | 2 | up to 12 |
| LSNM | 0x18 | 1 | 1 |
| FLSP | 0x20 | 1 | 1 |
| PLST | 0x228 | 2 | 1 |
| VRSM | 0x1C | 1 | 1 |

### 2.2 When the file is loaded and saved

| caller | what is taken from the file |
|---|---|
| Supervisor_OnRegister 0x445FA3 (boot) | PLST only. The block is zeroed first. |
| MainMenu_OnRegister 0x470A6C | CLRD, PSCR, CATK, FLSP, PLST. Then sets GM flags bit 15/16/17 (16 §4). These two functions are the only writers of bits 15–17 (all `g_GameFlags` stores checked); nothing clears them in game, replay or demo, so playback keeps them (changelog/answers_impl14.md §11). |
| GameMgr_LoadScoreData 0x43BBE1 (new game) | After the mode-2 flush (save), the whole in-memory CATK block (0x1E228 bytes) is **zeroed**, and all 222 records are stamped: magic `CATK`, size 0x22C (+4 and +6), version 3, id +0x0C = index (counters, name, owner, comments, checksum +0x0E, difficulty +0x0F all 0); only then the file is loaded (VERIFIED 0x43BBE1, RE-Q43). So after any game start all 222 records have the magic (and the next save writes all 222): a record that had no magic in memory (not in the file) loses whatever was added to it in memory (e.g. the hidden code's +1, §2.12), and if the load fails the card history is all zero. Then: hiscore for (shot, difficulty) → stats+0x14; CATK, CLRD, PSCR. In practice the hiscore is PSCR[shot].hiscore[stage][diff] and that play count is incremented. Then backs up CATK, clears the in-game HSCR record (§2.3) and copies the config into it. |
| Result_OnRegister 0x45964D | Builds the 5×12 ranking lists and LSNM. In viewer mode (Result mode 0) also loads CATK, CLRD, PSCR. |

Equivalence note (INFERRED from this table and the save paths below, RE-Q36): the blocks in memory change only through
gameplay / menu code, and every path from such a change to the next re-take of the same blocks (main menu, new game,
result) passes a save (game start flush, pause quit, after-game result, exit) — the only exception is the slow-mode
CATK reload of mode 2 (below) — so an implementation that loads every block once at boot and keeps them in memory
reaches the same state, provided it applies that slow-mode CATK restore.

`Result_Register(mode)` 0x4582A0 is the only path to a save. Its on-remove handler 0x459FD2 calls `ScoreDat_Save`. The call sites are:
- mode 2 (flush only: load, then immediately save) from:
  - the pause menu "quit" (0x4045AC);
  - GameMgr_LoadScoreData (0x43BBF3, at game start);
  - WinMain exit (0x441B95): after the main loop has left (`Frame_Run` returned non-zero: the quitting frame runs
    its calc chain and `Sound_Update`, then `Supervisor_JoinLoader` and returns — **no draw and no Present** for that
    frame, 00 §2; corrected RE-Q30), and only if the PLST block tag 0x164CD0C ≠ 0 (always after boot); then the chains
    are torn down (0x43CE30: `Supervisor_JoinLoader`, then both chain lists, head +0 first, then +0x20), which removes the Result and
    saves; then `Sound_Update` is called until it returns 0. So the 2 save draws (11 §1.5) come after the last frame's
    post-calc state, never inside a frame (VERIFIED WinMain, RE-Q28). The loop also ends (same exit path) when the
    device reset after a lost device fails. Closing the window does **not** end it: the window procedure 0x442390
    answers WM_CLOSE by setting 0x17CE8FC bit 7 and returning 1 (no `DefWindowProc`, the window stays), and no code
    reads bit 7; the loop flag 0x17CE704 is only ever written 0 (VERIFIED RE-Q30).
- mode 1 from Supervisor scene 6 (after a game).
- mode 0 from scene 5 (result viewer from the menu).

In mode 2 with slow mode on (or Supervisor+0x1A4 bit 3, which retail code only sets through the anti-tamper fill, changelog/answers_phase7.md §2), CATK is first reloaded from the old file, which throws away card credit from this run. VERIFIED.

### 2.3 HSCR (0x168) — high-score entry

| off | type | field |
|---|---|---|
| 0x0C | u32 | score ÷ 10 |
| 0x10 | f32 | slowdown % (§2.11) |
| 0x14 | u8 | shot 0..11 |
| 0x15 | u8 | difficulty 0..4 |
| 0x16 | u8 | stage reached: g_StageIdx (0..8), or **99** if the game was cleared (flags bit 4) |
| 0x17 | char[9] | name (8 + NUL) |
| 0x20 | char[6] | date "%m/%d" |
| 0x26 | u8 | continues used |
| 0x28 | 60 B | th08.cfg copy at game start (+0x44 = lives option) |
| 0x64 | u32 | play time in frames (0x164D09C) |
| 0x68 | u32 | point items collected (incremented in Item_CollectPoint 0x440E40) |
| 0x6C | u32 | never written (0) |
| 0x70 | u32 | misses (GM+0x3DA9C++) |
| 0x74 | u32 | bombs used (0x164CFA8 += 1 in `Player_UpdateBomb` at a normal-mode bomb start without the last-spell flag; VERIFIED RE-Q24) |
| 0x78 | u32 | Last Spells used (0x164CFAC += 1 in `Player_UpdateBomb` at a normal-mode start with the last-spell flag +4 set, 09 §7; never for the dissolve spell; VERIFIED RE-Q24) |
| 0x7C | u32 | pauses (GM+0x3DAA8++) |
| 0x80 | u32 | time-orb total (GM+0x3DAAC += n) |
| 0x84 | u32 | human rate × 100 (see below) |
| 0x88 | u8[222] | per card: captured during this game |
| 0x166 | u8 | 1 = default (placeholder) entry |

The in-game record is at 0x164CF34 (= GM+0x3DA2C). It is zeroed at new-game start (GameMgr_LoadScoreData). Shot and difficulty are filled at that point; the date, name and slowdown at name entry.

Human rate: EnemyMgr_Calc counts only outside dialogue and while its timer is > 15 — the timer is the enemy-manager clock (Timer at EnemyMgr +0x9DCED0, the same clock as the time rank 11 §3.2; ticked once at the end of every `EnemyMgr_Calc`), so frames while that clock reads 0–15 are not counted; the test runs at the top of `EnemyMgr_Calc`, next to the play-time counter 0x164D30C (11 §2), and EnemyMgr_Calc does not run on paused, game-over-menu, loading or post-clear frames (`GameMgr_Calc` returns 3 earlier in the chain) (VERIFIED 0x42C6A0, RE-Q24). Each frame it increments `total` (0x164D0AC), and also `human` (0x164D0A8) when the player is in human mode (player+3 == 0). Then rate = human·10000 / total. VERIFIED.

**Default table** (Result_OnRegister). For every difficulty d, shot s and rank i = 0..9:
- score (÷10) = 100000 − 10000·i;
- magic +0 = "DMYS" (0x4B7D58), not "HSCR";
- name "--------", date "--/--";
- stage byte 0 (printed as stage 1 by the ranking's stage map, 16 §2.3), continues 0, version 4, difficulty byte +0x15 = d, slowdown 0, sizes +4/+6 = 0x168, byte +9 = 0; the shot byte +0x14 is **not** written by this loop (VERIFIED 0x45964D, RE-Q24);
- +0x166 = 1.

The file's HSCR entries are then inserted into these lists in order. `ScoreList_Insert` 0x45A500 inserts into a singly linked list sorted by descending score and returns its 0-based rank. The walk advances past a node while that node has no entry or the new score is **strictly less** (unsigned u32 compare of +0xC) than the node's score, so a new entry goes **before** every existing entry with an equal score (VERIFIED RE-Q26). Consequences: a new run that ties a stored score takes the better rank (and ties with a DMYS default rank above the default); when score.dat is loaded, equal-score HSCR entries of one list end up in reverse file order, so each load/save cycle flips their order. Only the first 10 are kept or saved; default entries are never written to the file — `ScoreDat_Save` 0x453D0D skips every list node whose magic is not "HSCR" (the defaults carry "DMYS"). For each saved HSCR node it first rewrites, **in the in-memory entry itself**, shot +0x14 := list shot, difficulty +0x15 := list difficulty, sizes +4/+6 := 0x168, version +8 := 4 and the new-entry byte **+9 := 0**, then copies the 0x168 bytes; so +9 is always 0 in the file and the highlight of a new entry (16 §2.3, state 9) ends at the next save (VERIFIED, RE-Q24).

`ScoreDat_GetHiscore` 0x45A950 returns max(top entry of (shot, difficulty), 100000), in ÷10 units. That value is the HUD "HiScore" at game start. It builds a fresh list from the file's HSCR entries of (shot, difficulty) only (no defaults; `ScoreList_Insert` order, so of equal scores the **later** file entry is on top), and when its 4th argument is non-NULL writes the top entry's continues byte (+0x26) there (0 when the list is empty): `GameMgr_LoadScoreData` passes stats+0x18, the HUD's hiscore-continues digit (11 §5, 12 §5.3) (VERIFIED RE-Q32).

Save (VERIFIED `ScoreDat_Save` 0x453D0D, RE-Q40): for each written entry the saver first rewrites, **in the in-memory
entry itself**, +0x14 := shot, +0x15 := difficulty (from the list it walks), +4 / +6 := 0x168, +8 := 4 and **+9 := 0**,
then copies the 0x168 bytes. Byte +9 is the Result screen's "new entry" mark (16 §2.3); it is therefore 0 in every saved
file and also 0 in memory after any save.

### 2.4 CLRD (0x24 × 13) — clear flags

The in-memory copy is at GM+0x3C490 (0x164B998). Entries 0..11 are per shot; entry 12 is the union over all shots.

| off | type | field |
|---|---|---|
| 0x0C | u16[5] | no-continue clear mask per difficulty (E, N, H, L, Ex) |
| 0x16 | u16[5] | clear mask (continues allowed) |
| 0x20 | u8 | "bad ending seen, good not yet" flag (16 §1) |
| 0x21 | u8 | entry index |

Every mask defaults to 1 (stage 1 bit).

Bits:
- `1 << stage_idx` is set when a stage is cleared, by GameMgr_Calc at the stage-clear transition. The no-continue mask is set only when continues == 0. This also happens in practice, but not in replays.
- 0x4000: Final B ending (6B / stage 7) reached.
- 0x8000: Final A ending (6A) reached. Both bits are set by the ending code 16 §1.
- Extra clear: no-continue[4] |= 0x8000 and union any[4] |= 0x8000.

Unlock checks (VERIFIED):

| fn | true when |
|---|---|
| 0x43C15F(team t) | for t < 4: any no-continue[E..L] & 0x4000. Returns true for t ≥ 4. |
| 0x43C23B(team t) | for t < 4: any any-clear[E..L] & 0x8000. Returns true for t ≥ 4. |
| 0x43C1E9 | some team 0..3 passes 0x43C15F → **Extra unlocked** (GM flags bit 15) |
| 0x43C2C5 | some team passes 0x43C23B → flags bit 16 |
| 0x447D04 | all teams 0..3 pass 0x43C15F → flags bit 17 (solo characters unlocked) |

The menu shows `title/info00.jpg`, `info02.jpg` or `info01.jpg` once when the matching condition newly becomes true (16 §4).

### 2.5 CATK (0x22C × 222) — card history

The in-memory copy is GM+0x40 (0x160F548). There is a backup at 0x162D770 (0x1E228 bytes) taken at game start; the backup is copied back when slow mode is on (Result modes 0x11/0x16/0x12).

| off | type | field |
|---|---|---|
| 0x0C | u16 | card id |
| 0x0E | u8 | checksum = (Σ signed name bytes up to the NUL + Σ all 78 u32 counters) & 0xFF (bytes after the NUL are not summed) |
| 0x0F | u8 | difficulty of the last attempt |
| 0x10 | char[48] | card name (decoded from ECL with XOR 0xAA) |
| 0x40 | char[48] | owner (XOR 0xBB) |
| 0x70 | char[64] | comment line 1 (XOR 0xDD); written only on a spell-practice capture |
| 0xB0 | char[64] | comment line 2 (XOR 0xEE) |
| 0xF0 | u32[13] | story: max bonus per shot (index 12 = all shots) |
| 0x124 | u32[13] | story: attempts |
| 0x158 | u32[13] | story: captures |
| 0x18C | u32[13] | spell practice: max bonus |
| 0x1C0 | u32[13] | spell practice: attempts |
| 0x1F4 | u32[13] | spell practice: captures |

Counters are capped at 9999.

- Attempts are counted in `Spell_Start` 0x4152A0 (ECL op 122 → 0x421280). If the stored checksum is wrong at that moment, all counters of that card are zeroed first.
  Exact order (VERIFIED 0x4152A0, RE-Q43), all skipped in replay playback:
  1. card name → +0x10 (`strcpy` from the Spell object; bytes after the NUL keep their old value);
  2. owner: 48 ECL bytes XOR 0xBB into a zeroed 0x80 buffer, `strcpy` → +0x40;
  3. (spell practice, flags bit 14: the two comment lines go into the Spell object +0x74/+0xB4, not CATK);
  4. n = Σ signed name bytes from +0x10 **up to the NUL** (of the name just written); test (n + Σ all 78 u32
     counters) & 0xFF against +0x0E; if different, zero all 78 counters. There is **no magic / version test**:
     the test runs on whatever is at CATK[id]. In practice every record has the magic here, because the game
     start stamps all 222 (§2.2, `GameMgr_LoadScoreData`);
  5. story (bit 14 clear): attempts[shot] and attempts[12] += 1, each capped at 9999 (`< 9999` test); spell
     practice: the practice attempt row (+0x1C0) instead;
  6. +0x0F := difficulty (0x160F538); +0x0E := (n + Σ 78 counters) & 0xFF.
- Captures and max bonus are counted in `Spell_End` 0x4161B0.
- Neither happens in replay playback.

### 2.6 PSCR (0x178 × 12) — practice scores

In memory at 0x164BB6C + shot·0x178.

| off | type | field |
|---|---|---|
| 0x0C | u32[9][5] | practice plays [stage][difficulty] |
| 0xC0 | u32[9][5] | practice hiscore ÷10 |
| 0x174 | u8 | shot |
| 0x175 | u8 | used (the chunk is only saved if ≠ 0) |

- A practice start increments plays and sets +0x175 (GameMgr_LoadScoreData).
- The practice result (Result mode 0x11) stores max(hiscore, score) at index `[shot + byte 0x164D0B2]`. The byte 0x164D0B2 is 0 on every path that reaches a result screen (VERIFIED, changelog/answers_phase7.md §3; it is nonzero only during attract demos).

### 2.7 LSNM (0x18)

+0x0C char[12] = last name entered. The default is 8 spaces. It is held in Result+0x46450 and pre-fills both name entries.

### 2.8 FLSP (0x20) — Last Word unlocks

In memory at GM+0xC (0x160F514), +0x0C..+0x1C u8[17]. Slot k holds 0xCD + k when Last Word 205 + k is unlocked, else 0.

The slots are computed by `LastWord_ComputeUnlocks` 0x46CBBB in `MainMenu_OnRegister` (every menu load, after the
score.dat chunks are read) and on every entry to screen 17.
- It only **sets** slots (0xCD + k), never clears them.
- The block (with its 12-byte header) is read whole from a "FLSP" v1 chunk (`ScoreDat_GetFLSP`); memory is untouched when
  there is no chunk.
- It is written by every `ScoreDat_Save` with a fresh header.
- `Card_IsSelectable(id)` compares byte GM+(id − 181) with id (changelog/answers_impl17.md §Q1). Tables: `specs/data/spell_tables.md`. Card ids count from 0. "Prac" = practice capture counters (CATK +0x1F4[12]); "story or prac" = either capture counter.

| card | owner | unlock condition (VERIFIED) |
|---|---|---|
| 205 | Wriggle | ≥ 2 shots have a no-continue 0x4000 (Final B) clear on any of E/N/H/L |
| 206 | Mystia | ≥ 3 such shots |
| 207 | Keine | ≥ 50 distinct cards captured in practice |
| 208 | Reisen | ≥ 4 such shots (as 205) |
| 209 | Eirin | card 137 captured (story or prac) |
| 210 | Kaguya | ≥ 15 cards of the 43-card "boss last spell" list (0x4C6B90, test 0x414540) captured in practice |
| 211 | Mokou | cards 195 and 145 captured (story or prac), and card 204 attempted |
| 212 | Tewi | Last Words 208, 209, 210 all attempted |
| 213 | Keine (Hakutaku) | Last Words 205, 206, 207, 211 all attempted |
| 214 | Reimu | all 49 cards of the Normal list (0x4C61C8) captured with shot 6 (Marisa solo; matches the in-game hint) |
| 215 | Marisa | CLRD[4] (shot 4) no-continue Hard or Lunatic has 0x4000 |
| 216 | Sakuya | ≥ 120 distinct cards captured in practice |
| 217 | Youmu | ≥ 6 shots as 205 |
| 218 | Alice | ≥ 3 shots with Extra no-continue mask & 0x100 (Extra stage cleared) |
| 219 | Remilia | ≥ 30 last-spell-list cards captured in practice |
| 220 | Yuyuko | union entry (12) any-clear Lunatic & 0xC000 ≠ 0 |
| 221 | Yukari | all of 205..220 attempted |

"Attempted" means any attempt counter ≠ 0 (story or practice).

Card lists:

| list | address | count |
|---|---|---|
| Easy | 0x4C6120 | 42 |
| Normal | 0x4C61C8 | 49 |
| Hard | 0x4C6290 | 50 |
| Lunatic | 0x4C6358 | 50 |
| Extra | 0x4C6420 | 14 |
| all | 0x4C6458 | 222 |

Pointer table 0x4C67D0, counts at 0x4C67E8. `Spell_DifficultyOf` 0x4144D0 returns the list index, and 5 for ids ≥ 205.

### 2.9 PLST (0x228) — play statistics

In memory at 0x164CD0C (GM+0x3D804).

| off | type | field |
|---|---|---|
| 0x0C | u32[4] | total run time h, m, s, ms (`Supervisor_UpdateRunTime` 0x4482A1, from timeGetTime) |
| 0x1C | u32[4] | total play time h, m, s, ms (`Supervisor_UpdatePlayTime` 0x448418) |
| 0x2C | u32[7][17] | counters: block 0..4 = difficulty E, N, H, L, Ex; block 5 unused (always 0); block 6 = total |
| 0x208 | u8[32] | music-room "heard" flags |

Inside each 17-word block:

| index | counter | updated in |
|---|---|---|
| [0] | plays | StageStart on a fresh start (**including stage practice and retries**), and on each continue (continue menu, 0x4037B0) |
| [1..12] | plays per shot | same |
| [13] | retries | StageStart when the requested scene 0x17CE8B4 == 10 — in addition to [0]/[1+shot] |
| [14] | clears | ending 0x429980 / Extra clear in GameMgr_Calc |
| [15] | continues | continue menu |
| [16] | practice plays | StageStart in practice |

- StageStart counts only in the fresh-start branch, when not replay (GameFlags bit 3), not spell practice (bit 14)
  and the cfg copy slow-mode byte (+0x25) is 0. The cfg copy's +0x25 is forced to 0 just before for Extra
  (diff ≥ 4), stage practice (bit 0) and spell practice, so practice always counts. Order: [0] of the
  difficulty block, [0] of block 6, [1+shot] diff, [1+shot] block 6, then if retry [13] diff + block 6, then if
  stage practice [16] diff + block 6 (changelog/answers_impl14.md §9, VERIFIED 0x43B05C–0x43B11A). Every counter is capped at 999999
  (increment only while < 999999).
- Play time (`Supervisor_UpdatePlayTime` 0x448418) adds `timeGetTime() − mark` (mark = Supervisor+0x1AC;
  mark := 0 if the clock went backwards) split into h/m/s/ms with carries, then mark := now. Mark set: top of
  every `GameManager_StageStart`, on leaving the pause menu (resume/retry/quit paths) and the continue menu.
  Added: on pause entry (GameMgr_Calc), when the continue prompt opens (not on its early exits, 11 §4.2), and in `GameMgr_OnRemove` unless replay
  (then mark := 0). So play time = wall time with a GM in a stage, excluding pause and continue menus,
  replays/demos not counted at removal.
- Music flags are set by `Music_PlaySlot`/`Music_PlayFile` unless in replay or demo. The ending sets flag 18, and 19 on a good ending.
- The parser checks block 6 == Σ blocks 0..5.

### 2.10 VRSM (0x1C) and the exe check — VERIFIED (RE-Q40)

| off | type | field |
|---|---|---|
| 0x09 | u8 | 0 (written by the saver); +0x0A/+0x0B are not initialised (stack garbage) |
| 0x0C | char[8] | version tag "0100d" (strcpy of 0x4B660C; bytes after the NUL are garbage) |
| 0x14 | u32 | exe size |
| 0x18 | u32 | exe sum = Σ u32 words of th08.exe, count = size/4 − 1 (0x443730) |

**The version list.** At boot (`Supervisor_OnRegister`, 0x4461A0, after opening th08.dat) the game loads
`th08_%.4x%c.ver` formatted with (0x100, 'd') = **`th08_0100d.ver` from the archive only** (`FileSystem_OpenFile`
flag 0, no disk fallback) into Supervisor+0x360 (buffer) / +0x35C (size). If it is missing, start-up fails with the log
"error : データのバージョンが違います" (0x4B668C), so in retail the list is always present. The shipped file is 15 lines
`tag size sum\n` (LF only, the last line ends with LF):

```
0009_ 566784 993132091      0009a 596992 489076697      0010_ 596992 3478009099
0011_ 596992 3545283257     0001_ 800256 1644916963     0001a 807424 256801575
0002_ 809984 1745855920     0003_ 825344 922048924      0003a 825344 2460321175
0100_ 840192 3552390786     0100a 841216 3000360128     0100b 841216 4189886502
0100c 840704 1605176696     0100c 840704 1588396223     0100d 840704 2724749753
```

**`ExeCheck(tag, size, sum)` 0x44858D** returns 0 (pass) or −1 (fail):
1. list buffer null → pass (unreachable in retail, see above);
2. `strncmp(tag, "debug", 5) == 0` → pass;
3. the build tag "0100d" (0x4B660C) equal to "debug" (0x4B6D08) → pass (never: a debug-build switch);
4. walk the lines with `remaining` = file size: if the first 5 bytes of the line equal the first 5 of `tag`, parse the
   rest from line start + 6 with `sscanf("%d %d")` and pass if both equal `size` and `sum` (32-bit compare; the
   CRT's `%d` wraps, so "2724749753" gives 0xA26861B9 — VERIFIED: demo 0 (tag "0100a", sum 3000360128) is accepted and
   the periodic check below never quits); then advance to after the next LF and subtract the bytes advanced from
   `remaining`; stop when `remaining` == 0 → fail.
   QUIRK: on a line whose tag matches but whose numbers do not, the 6 skipped bytes are not subtracted, so `remaining`
   never reaches 0 at the end of the buffer and the walk reads past it (undefined behaviour: a crash or a fail). A
   matching later line (the two "0100c" lines) still passes first. A port may treat the over-run as a fail.

Callers:
- `ScoreDat_Load` (§2.1): in the chunk walk, for every chunk with magic VRSM **and version 1**, before that chunk's
  size-0 test: fail → log "warning : score.dat exesumcheck error", file rejected (defaults).
- `Replay_Load` (§3.7 step 5): tag info+0x12C, size +0x124, sum +0x128; fail → load fails (empty / invalid slot). The
  demos pass: demo 0 has tag "0100a" with the 1.00a size/sum (listed), demos 1–3 have tag "debug".
- `Supervisor_Calc` (11 §7): every 4000th counted frame, with the running exe's tag/size/sum; fail → return 4 (quit).

Values written (`ScoreDat_Save` and `Replay_RecordStageStart`): tag = string 0x4B660C "0100d", size/sum =
0x17CEAB0/0x17CEAAC computed once at boot by 0x443730 over the running exe (`GetModuleFileNameA`): sum =
Σ (u32 LE words 0 .. size/4 − 2), wrapping. For the shipped th08.exe: **size 840704 (0xCD400), sum
0xA26861B9** (computed from the exe and matching an original-written score.dat, changelog/answers_impl14.md §10). A port
writes these constants, uses them as the "running exe" values (so the periodic check always passes), and applies the
check to loaded score.dat and replay files with the list read from th08.dat.

### 2.11 Slowdown % (stored in HSCR and replays) — VERIFIED

Accumulators: A (0x17CE8E8) and B (0x17CE8EC). Each FPS measurement (window ≥ 500 ms) during gameplay adds:
- B += 60;
- A += 60·w, where w = 1.0 if fps > 54, 0.8 if > 42, 0.6 if > 30, else 0.5.

At the result: x = clamp((A/B − 0.5)·2, 0, 1), and slowdown % = (1 − x)·100.

### 2.12 Hidden unlock code — VERIFIED (0x4550FC, result-viewer difficulty page)

While holding Focus or Skip, enter Right ×4, Left ×1, D ×2, Q ×3.

Exact rule (VERIFIED 0x4550FC, RE-Q38). Counter = Result+0x4C (0 at registration; `Result_SetState` does not clear
it). It is stepped once per state-3 call, after the page input (16 §2.4), in this order:
1. Neither Focus (0x4) nor Skip (0x100) held in g_InputRawCur → counter := 0, done.
2. Counter < 10: the wanted key is Right (0x80) for counter 0–3, Left (0x40) for 4, D (0x2000) for 5–6, Q (0x200) for
   7–9. A press of it (edge: `cur & k ≠ 0` and `≠ prev & k`) → counter += 1. Otherwise an edge on the mask 0x160B
   (Shot, Bomb, Pause/ESC, Q, S, Enter; `(cur & m) ≠ 0` and `(cur & m) ≠ (prev & m)`) → counter := 0. Other keys
   (wrong arrows, D in the Right phase, Up/Down …) change nothing.
3. Counter ≥ 10 (so the effect comes on the call **after** the tenth key, and only if Focus/Skip is still held):
   the effect below, counter := 0.

Effect:
- every CLRD mask becomes 0xFFFF: no-continue (+0x0C) and any-clear (+0x16) masks, all 5 difficulties, all 13
  entries (0..11 and the union entry 12);
- all 222 in-memory CATK records (whether or not the record has the `CATK` magic): the 13 story attempt counters
  (+0x124, shots 0..11 and the all-shot total 12) += 1 each — **no 9999 cap and no checksum update** (QUIRK: the
  stored byte +0x0E no longer matches, so the next `Spell_Start` of that card zeroes its counters first, §2.5);
  records without the magic (not loaded from the file) are not saved, and the next game start zeroes and re-stamps
  them (§2.2), so their +1 never reaches `Spell_Start` (RE-Q43);
- plays SE 0x1C.

---

## 3. Replays (`replay/th8_NN.rpy`) — VERIFIED

### 3.1 Runtime object

The object is 0xDC bytes, pointer at 0x18B8A28. `Replay_Register(mode)` 0x451F90 sets g_Input and g_InputPrev to 0.

| mode | calc entries |
|---|---|
| 0 record | `Replay_RecordCalc` 0x452310, prio 17. On-register `Replay_RecordStageStart` 0x452830; on-remove 0x453080. Also `Replay_FrameHook` 0x4522A0, prio 7 (11 §1.4). |
| 1 play | `Replay_PlayCalc` 0x452550, prio 6. On-register `Replay_PlayStageLoad` 0x452D60. Also `Replay_FastForward` 0x452490, prio 18. If header+6 (debug flag) ≠ 0: `Replay_PlayCalcDebug` 0x4526C0 replaces PlayCalc, and `Replay_FrameHook` is linked at prio 7 (11 §1.4). The recorder always writes header+6 = 0, so retail and self-recorded files never take this path. |

`Replay_PlayCalcDebug` (VERIFIED 0x4526C0, RE-Q29) is PlayCalc (§3.3) with exactly one difference: the input word is
read from a second cursor +0x78, which then steps by **6** bytes (PlayCalc: cursor +0x50, step 2). `Replay_PlayStageLoad`
sets +0x78 := +0x50 = record+0x24 at every stage, so the debug calc reads the words at record+0x24, +0x2A, +0x30, … of
the normal stream. The other two words of each 6-byte step are **never read**: the RNG state is **not** forced and
there is no flag word with a meaning (the {input, 0, RNG} layout survives only as the recorder's dummy frame, §3.2).
Everything else is identical and in the same order: the flags-bit-2 / slow-mode / bit-9 gates, g_InputPrev,
key-repeat emulation, the fps byte every 30 frames, frame += 1.

Object fields:

| off | field |
|---|---|
| +0 | frame counter |
| +4 | clear-bonus frame counter |
| +8 | info pointer (0x134-byte header block) |
| +0x50 | input cursor |
| +0x54[9] | input end per stage |
| +0x78 | debug-format cursor: := +0x50 (record+0x24) at every stage start of both modes; the recorder writes its dummy {0, 0, RNG} frame through it (§3.2); in playback only `Replay_PlayCalcDebug` reads/advances it |
| +0xA0 | fps cursor |
| +0xA4[9] | fps end per stage |
| +0x10 / +0x14 | mode (0 record, 1 play) / file path (playback) passed to `Replay_Register` |
| +0xC8 | main calc node (RecordCalc / PlayCalc / PlayCalcDebug) |
| +0xD0 | `Replay_FastForward` node (playback only; 0 when recording) |
| +0xD4 | `Replay_FrameHook` node (recording always; playback only with header+6 ≠ 0; else 0); +0xD0/+0xD4 are unlinked by `Replay_OnRemove` 0x453080 |
| +0xD8 | u16 RNG state snapshot written by the hook (11 §1.4); never read |
| +0xDA | u16 statistics word (08 §9); never read |

### 3.2 Recording, per frame (calc prio 17, after the game objects)

Only while flags bit 2 (gameplay running):
1. `g_InputPrev = g_Input; g_Input = raw input`.
2. If slow mode (byte +0x25 of the **gameplay config copy** `*(GM+4)`, VERIFIED 0x452310 RE-Q40) or Supervisor+0x1A4 bit 3 is set, nothing is recorded. VERIFIED: no retail code sets bit 3, except the anti-tamper fill that writes 0xFFFFFFFF over the whole Supervisor (changelog/answers_phase7.md §2).
3. While flags bit 9 (stage-clear bonus): if counter+4 > 2, return; else counter+4 += 1. So at most 3 frames are recorded during the bonus.
4. `p += 2; end[stage] = p + 2; *(u16*)p = raw`.
5. If `frame % 30 == 0`:
   - `fps[q] = fps_now | (flag ? 0x80 : 0)`, `fps[q+1] = fps_now`;
   - `fpsEnd[stage] = q + 2`; `q += 1`.

   fps_now is u8 0x17CE8F0. The flag is the u32 0x17CEA90, = Supervisor+0x338; its only writer stores 0 (MainMenu_OnRegister), so it is always 0. VERIFIED (changelog/answers_phase7.md §1).

   fps_now writers (VERIFIED RE-Q40): 0 at boot; `Fps_Update` (§3.3) := trunc(measured fps) when a meter window closes
   while flags bit 2 is set and not in playback; `Replay_PlayCalc` := the byte it reads (below). Nothing resets it, so
   it carries across scenes and games. The record calc of a stage's frame 0 runs before any window of that stage can
   close (the meter runs at draw prio 16, after the calc chain), so **sample 0 = the carried value**: 0 in the first
   game after boot, the last byte read by a preceding playback (e.g. an attract demo), else the last measured value.
   At a steady 60 fps the meter gives 60 (an original-recorded replay, game over in stage 1 under wine: sample 0 = 0,
   all 49 others 60). A port without a meter writes 60 for samples ≥ 1 and the carried value for sample 0. Sample 0
   is never read back by playback (QUIRK below), so it only shows in the file bytes.
6. `frame += 1`.

The game objects ran earlier in the same frame using `g_Input` from the previous frame. So the input of frame n is the raw input sampled at the end of frame n−1.

Stage start (`Replay_RecordStageStart`):
- Allocates two 0xD2F00-byte buffers (record, fps).
- Fills the 0x24-byte record header (§3.4).
- Sets the cursor to record+0x24 and writes a 0 word there.
- Also writes 0 at +0x26 and the RNG state at +0x28. These are debug-format leftovers; they are overwritten by frames 1 and 2.

So the stream is `[dummy 0][frame 1][frame 2]…`.

Before allocating (only when the info block already exists, i.e. not at the first recorded stage), exactly **one**
record gets +0 := current score (stats+8): the record of the **highest stage index below the current stage that has a
record** (scan k = 0 … stage − 1, keep the last non-null pointer; VERIFIED 0x452830, RE-Q25). Earlier records are not
touched. Record +0 is therefore the **score at the end of that stage**. For the last stage it is set when the replay is finished.

`Replay_AppendEnd` 0x4531A0 runs on game end, before saving: `p += 2; *p = 0; end = p + 6`. The stream ends with a 0 terminator plus 4 uninitialised bytes. The demo files contain 0xCDCD there.

### 3.3 Playback, per frame (calc prio 6, before the game objects)

Only while flags bit 2, not slow mode (gameplay config copy `*(GM+4)` +0x25, VERIFIED 0x452550 RE-Q40), and **not** flags bit 9:
- `g_InputPrev = g_Input; g_Input = *p; p += 2`.
  - The first read is the dummy 0 word. This matches the one-frame latency of recording, so frame n sees the same value it saw when recorded.
- Key repeat is emulated (normally done by the input poller):
  - repeat flag 0x164D538 = 0;
  - if input == previous: when counter 0x164D53C > 29, repeat = ((counter & 7) == 0), and if counter > 37 it is reset to 30; then counter += 1;
  - else counter = 0.
- If `frame % 30 == 0`:
  - b = fps[q+1]; displayed fps = b & 0x7F; red flag 0x17CEA94 = b >> 7; q += 1.
  - QUIRK: playback reads byte q+1, so it shows the sample one step ahead (and the first byte is never read). The record's overlapping two-byte writes make the stream one byte longer than the sample count. Reproduce it exactly if bit-exact display matters.
- `frame += 1`.

Frames during the stage-clear bonus (bit 9) are not read. The ≤ 3 frames recorded there sit after the frames used and are harmless.

Playback speed byte 0x164D0BD (chosen in the replay menu, §3.8):

| value | behaviour |
|---|---|
| 0 | normal |
| 1 | "slowdown reproduction". GameMgr_Calc runs a frame only on a pattern that depends on the stored fps: fps < 20 → 1 of 3 frames; 20–29 → 1 of 2; 30–39 → 2 of 3; 40–49 → 5 of 6; ≥ 50 → every frame. |
| 2 | fast. `Replay_FastForward` (prio 18) returns 6 ("restart the calc chain") on every frame except `frame % 5 == 4`, only when no boss is active (0x42F1F0). |

In any playback mode, while skippable dialogue is shown (HUD impl+0x22D7C), the chain is restarted on 2 of every 3 frames (= 3× speed).
Both rules are one function (`Replay_FastForward` 0x452490, calc prio 18, registered only for playback; VERIFIED RE-Q25, re-checked RE-Q40):
only while GM flags bit 2 is set and bit 9 clear: (1) dialogue active and skippable and `frame % 3 ≠ 2` → return 6;
(2) otherwise, speed byte 2, no boss (0x42F1F0) and `frame % 5 ≠ 4` → return 6; else return 1. `frame` = the replay
object's frame counter (+0) in **both** tests — there is no separate dialogue counter; +0 advances once per PlayCalc
run, and a restart re-runs PlayCalc, so the dialogue test sees consecutive values. The dialogue test comes first: with
fast playback during skippable dialogue, `frame % 3 ≠ 2` restarts, and only on `frame % 3 == 2` is the fast test made.
Outside bit 2, or with bit 9 set, neither test is made (return 1).
Each restart is a full extra calc pass (00 §3, VERIFIED RE-Q51): `Supervisor_Calc` runs again and polls input again,
`Replay_PlayCalc` reads the next input word and the GM runs the next gameplay frame. Nothing is drawn between passes.
Example (gensokyou udAr02, skippable dialogue): presents 5170–5172 run gameplay frames 4963–4964, 4965–4967,
4968–4969 (2, 3, 2 passes).

The HUD shows the playback fps in yellow (0xFFFFFFD0), or red (0xFFFF4040) when the flag is set.
Exact behaviour (`Fps_Update` 0x446F53, VERIFIED changelog/answers_impl17.md §5.3; this is the attract demos' "2-digit text at the
bottom right of the playfield"):
- Drawn every frame while replay playback (GameFlags bit 3) **and** gameplay running (bit 2), unless 0x17CE8D0 ≠ 0
  (ending): `Ascii_QueueString` at (384, 448) (ASCII queue, 12 §2.9: 16 × 16 `ascii.anm#0` glyphs, advance 13 →
  quads (383.5, 447.5)–(399.5, 463.5) and (396.5, 447.5)–(412.5, 463.5)), colour as above, then the queue colour
  is reset to white. The normal fps string ("%.02ffps" at (512, 464), white) is queued just before it.
- The text is a separate buffer (0x17CF2B8) formatted with **"%2d" of fps_now (0x17CE8F0)** only when the fps meter
  closes a measuring window (≥ 500 ms of host time, `timeGetTime`, or the performance counter when present) — i.e.
  about every 30 frames at 60 fps, with a host-dependent phase. Between refreshes the old text stays; the buffer
  survives between games (a demo starts by showing the previous demo's last value).
- fps_now itself changes every 30 replay frames (the `frame % 30 == 0` read above). Demo 1's fps bytes are
  60, 16, 60, 60, … → frame 0 reads 16 (QUIRK, byte q+1); the original showed "60" for gameplay frames 1–4 (old
  text), "16" for frames 5–34 and "60" afterwards. A port without a real-time meter should refresh the text every
  30 frames; the exact refresh frame cannot be reproduced bit-exactly.
- The meter (VERIFIED 0x446F53, RE-Q40): runs once per drawn frame from Supervisor draw prio 16 (0x445BC0), also once
  (with no draw and no slowdown accounting) from `GameManager_StageStart` on the loader thread, and does nothing on
  slow-mode skipped frames (gm+0x2D). Each run adds 1 + cfg frame-skip byte (+0x23) to a frame count; when ≥ 500 ms
  (`timeGetTime`; with a performance counter: ≥ freq/2 ticks) have passed since the window start, fps = count / elapsed
  seconds, the "%.02ffps" text is formatted, the count is reset and the window restarts at now. The window start is
  taken at the first run after boot and is **never reset by scenes, stage starts or demos** (only when the clock goes
  backwards), so the refresh phase inside a demo depends on everything since boot (boot and menu load times are host
  thread times, 12 §1). Model for a port at 60 Hz: one window per 30 drawn frames counted continuously from boot.

### 3.4 Stage record (0x24 header + streams)

See 11 §6 for the table. Corrected meaning of +0: score at the **end** of that stage (÷10). Playback (`Replay_PlayStageLoad`) restores lives, bombs, power, rank, seed and so on from the record of the stage being started; the **score** comes from +0 of the last existing record of an earlier stage (0 / untouched for the first recorded stage) (11 §3.4, changelog/answers_impl14.md §6).

### 3.5 File layout

```
0x000  file header (0x68), partly encrypted
0x068  LZSS-compressed, encrypted body
size   "USER" section (plain text; not covered by checksum or encryption)
```

File header:

| off | type | field |
|---|---|---|
| 0x00 | char[4] | "T8RP" |
| 0x04 | u16 | version 6 |
| 0x06 | u8 | debug-frame flag (0) |
| 0x07 | u8 | 0 |
| 0x0C | u32 | file size without the USER section (= 0x68 + compressed size) |
| 0x10 | u32 | checksum = 0x3F000318 + Σ bytes [0x15 .. 0x68) + Σ compressed bytes, taken **before** encryption |
| 0x14 | u8 | random byte |
| 0x15 | u8 | encryption key = random + 0x40 |
| 0x18 | u32 | compressed size (encrypted from here on) |
| 0x1C | u32 | decompressed size (= image size − 0x68) |
| 0x20 | u32[9] | file offset of each stage's record (0 = stage not played), in decompressed-image coordinates |
| 0x44 | u32[9] | file offset of each stage's fps stream |

Encryption runs over bytes 0x18..0x67 of the header, then over the compressed body: `b += key; key += 7` (u8 arithmetic). Decrypt by subtracting the same sequence.

Saving uses **3 RNG draws** (Rng_Range16(256) each): the key byte, info +0x68, and header +0x14. (In the decompiled save these are three separate draws; 11 §1.5 is corrected.)

Decompressed image = header (0x68) + info block (0x68..0x134) + for each recorded stage, in order: its record (0x24 header + input words up to `end`), then its fps bytes up to `fpsEnd`.

Info block (offsets in the image):

| off | type | field |
|---|---|---|
| 0x68 | u8 | random |
| 0x69 | u8 | 100 (set at record start; VERIFIED never read) |
| 0x6A | u8 | shot |
| 0x6B | u8 | difficulty |
| 0x6C | char[6] | date "%m/%d" |
| 0x72 | char[9] | name (pre-set "NO NAME"; the entered name on save) |
| 0x7B | u8 | practice flag (GameMgr_IsPractice) |
| 0x7C | i16 | spell id in spell practice, else −1 |
| 0x7E | char[48] | card name (spell practice) |
| 0xAE | u16 | 0x100 (demo files 1–3, tag "debug", have 0) |
| 0xB0 | u32 | final score ÷10 |
| 0xB4 | 60 B | th08.cfg copy at record start (+0xD9 = slow mode; must be 0) |
| 0x114 | f32 | slowdown % + 1.12 (constant 0x4B7088) |
| 0x118 | f32 | slowdown % |
| 0x11C | u8 | clear status at record time: 2 if any no-continue Final B clear on E–L for this shot or shot ≥ 4; else 1 if any Final A clear; else 0 |
| 0x120 | u32 | 30 (fps sample interval) |
| 0x124 | u32 | exe size |
| 0x128 | u32 | exe sum |
| 0x12C | char[8] | version tag "0100d" |

USER section:
- "USER", u32 total length (including this 12-byte head), u8 type 0, 3 garbage bytes.
- Then Shift-JIS lines `key\tvalue\r\n`:
  - プレイヤー名 (name)
  - プレイ時刻 (`%Y/%m/%d %H:%M:%S`)
  - キャラ名 = `Shot_Name(g_ShotType)`, table 0x4C7F4C[shot] (VERIFIED RE-Q40), printed whole **including the
    trailing U+3000 padding** (each name is 8 full-width characters = 16 Shift-JIS bytes, VERIFIED RE-Q45 all 12 entries:
    霊夢＆紫　　　　 / 魔理沙＆アリス　 /
    咲夜＆レミリア　 / 妖夢＆幽々子　　 / 博麗　霊夢　　　 / 八雲　紫　　　　 / 霧雨　魔理沙　　 / アリス・Ｍ　　　 /
    十六夜　咲夜　　 / レミリア・Ｓ　　 / 魂魄　妖夢　　　 / 西行寺　幽々子　 (one trailing U+3000); demo 0's USER text shows the padding)
  - スコア (`%d0`)
  - 難易度 = table 0x4C7F30[GM difficulty]: Easy / Normal / Hard / Lunatic / Extra (index 5 "LastWord" unused here)
  - 最終ステージ: "Clear" when GM flags bit 4 is set, else `0x453CD1(g_StageIdx)` = table 0x4C7FAC[stage] for
    stage < 9 (Stage 1 / Stage 2 / Stage 3 / Stage 4-uncanny / Stage 4-powerful / Stage 5 / Stage 6-Eirin /
    Stage 6-Kaguya / Extra Stage), "Clear" for ≥ 9; or in spell practice (info +0x7C ≥ 0) カード名 `No.%3d %s` with
    id + 1 and the info card name +0x7E
  - ミス回数
  - ボム回数
  - 処理落ち率 (`%f%%`)
  - 人間率 (`%3.2f％`, from HSCR+0x84 / 100)
  - ゲームのバージョン (`%d.%.2d%c`, 1,0,'d')
- NUL-terminated, padded to an even length.

### 3.6 Save flow (`Replay_Finish` 0x4531F0, ECX = path, EDX = name)

The save is skipped (nothing written) when any of these hold:
- playing back;
- a normal game (not practice) on difficulty < 4 whose current config differs from the recorded one;
- the recorded slow mode ≠ 0.

Otherwise:
1. Fill date, name, score, slowdown and card name. Card name (spell practice, id = info +0x7C ≥ 0): 48 bytes copied
   from the in-memory CATK entry of that card, `0x160F558 + id·0x22C` = CATK[id] +0x10 (§2.5; GM+0x40 = 0x160F548 is
   the array) (VERIFIED 0x4531F0, RE-Q40).
2. Draw 3 RNG values.
3. Build the image and compress it.
4. Compute the checksum, then encrypt.
5. Write the header, body and USER section.

Save UI, Result states 10–14 (0x45696F); full input, interrupt and draw details in 16 §2.6:

| state | behaviour |
|---|---|
| 10 | "Save replay?" Yes/No. At frame 10: slow mode → message (interrupt 0x13), state 11; continues ≠ 0 → "cannot save" (interrupt 14), state 11. |
| 11 | Message; leaves after 20 frames. |
| 12 | Slot list `./replay/th8_%.2d.rpy` 01..15 (creates the directory `replay`). The cursor moves from frame 20. |
| 13 | Name entry (16 §2.2: same 96-char grid, 8 chars max), pre-filled from LSNM. |
| 14 | "Overwrite?" Yes/No. |

Choosing an existing valid file goes to 14, otherwise to 13. Finishing the name: write the file, store the name in LSNM, leave.

File names: the game writes only `th8_01.rpy`..`th8_15.rpy`. It also lists `th8_ud????.rpy` (user-renamed files).

### 3.7 Load (`Replay_Load` 0x451D90)

Steps:
1. Check the magic "T8RP" and version 6.
2. Decrypt from 0x18.
3. Verify the checksum.
4. Decompress.
5. Reject if info+0xD9 (recorded slow mode) ≠ 0, or if the exe check (§2.10, using +0x124/+0x128/+0x12C) fails.
6. Convert stage offsets to pointers.

Error message: "リプレイデータが異常".

### 3.8 Replay menu (`MainMenu_Replay` 0x46E136, draw `MainMenu_ReplayDraw` 0x46EF8D; layout 12 §2.8)

Listing:
- Loads slots 01..15, then up to 45 `th8_ud????.rpy` files (shown as "User ").
- 15 rows per page; Left/Right (edge or repeat flag) moves ±15 rows **only when there are more than 15 files**, and
  wraps: −15 < 0 → + count, +15 ≥ count → − count; SE 12 on every such press (VERIFIED 0x46E136, RE-Q37). Input
  start: 12 §2 (cursor from call 9, confirm/cancel from call 18; the +0x10 > 9 gate is armed only once per visit
  and sub-states 2/3 have none, RE-Q42). Entry block (only in sub-state 0 with +0x1442C == 0; the cancels 3 → 2 / 2 → 1
  zero +0x1442C but do not re-run it, 12 §2 intro, RE-Q47): surface `select00.png` (unless the
  previous screen was 7), range interrupt 14, cursor 0, +0xC2BC := 0, file scan. Confirm with 0 files does nothing.
- Row format: `"%s %8s  %6s %7s  %8s"` = No., name (+0x72), date (+0x6C), character (table 0x4C86C0[shot]), difficulty (0x4C86F0[diff]). For spell practice the last column is `Spell No.%3d`.

Detail view after choosing a file:
- slowdown `%2.3f%%`;
- a "Stage    LastScore" table: `%s %9d0` per stage that has a record, `----------` otherwise. For spell practice: `No.%.3d %9d0`.

The user then:
- picks one of the recorded stages (sub-state 2). On the file confirm (sub-state 1) the stage cursor (+0) is set to
  the **first stage with a record** (scan from 0 over the listing copy +0x7A50 + 0x134·row + 0x20 + 4k). None
  (k > 8) → error log line `リプレイデータが異常` (0x43EB60: appended to the log buffer 0x164D540 and its error flag
  +0x2004 := 1, so at program exit `log.txt` is written and the log is shown in an error MessageBox, 00 §2), scene −1
  (program exit), the handler returns 0 (VERIFIED 0x46E136, RE-Q37). Files written by the game always hold a stage.
  The same confirm sets up the card-name scratch VM of 12 §2.8 (for every file; drawn only for spell replays). Sub-state 2: `Menu_CursorUpDown(9)`,
  then while the cursor's stage has no record it keeps moving in the same direction (wrapping 0 ↔ 8), then the
  displayed stage row +0xC290 := cursor. +0xC290 is written **only** there, and the draw highlights +0xC290, so on the
  confirm frame the table shows the previous +0xC290 (0 after the menu was entered, else the last chosen row) and the
  first recorded stage from the next frame on (VERIFIED, RE-Q25). Confirm → sub-state 3 (speed cursor 0); Bomb → free
  the replay, back to the list (cursor = file row);
- picks the speed (sub-state 3): 3 options (normal / slowdown reproduction / fast), or 2 in spell practice.

**One cursor field for all three sub-states** (menu +0x00, the session trace's `cursor` column; VERIFIED 0x46E136,
RE-Q49). There is no separate stage or speed cursor: +0x00 is the list row in sub-state 1 (copied into +0xC28C
every sub-state-1 frame, after the paging), the stage row in sub-state 2 and the speed in sub-state 3. Writes:
file confirm → +0x00 := first recorded stage (above); stage confirm → +0x00 := 0 (speed 0); speed cancel (3 → 2) →
+0x00 := +0xC290; stage cancel (2 → 1) → +0x00 := +0xC28C (the file row); speed confirm → playback speed byte
0x164D0BD := +0x00. So after a file confirm the trace's `cursor` shows the stage row, not the file row, and after the
stage confirm 0. Stage-skip quirk: in the **up** direction the skip loop wraps −1 → **9** (the down loop and
`Menu_CursorUpDown(9)` wrap to 8 / 0) and then tests "record 9" = listing copy +0x20 + 9·4 = header +0x44, the stage-0
fps-stream offset. That loop only passes below 0 when stage 0 has no record, and then +0x44 is 0 in any written file,
so it continues 9 → 8 → … and the result equals a plain 0 ↔ 8 wrap. (A crafted file with record 0 = 0 but +0x44 ≠ 0
would stop on stage row 9, which becomes g_StageIdx.)

Start:
- flags |= 8 (replay);
- the path is copied to 0x164D0C4;
- **GM flag bit 0 (practice) is not set** — it was cleared when the Replay item was chosen in the top menu (and by
  `MainMenu_OnRegister`), so a spell-practice replay plays with bit 14 set and bit 0 clear, a stage-practice replay with
  both clear (its practice nature is only in info +0x7B, 12 §5.6) (VERIFIED 0x4674E0 / 0x46E136, RE-Q25);
- difficulty, shot and spell practice (spell id ≥ 0 → flags bit 14, id → 0x164D0B8) come from the header;
- stage = the chosen stage;
- scene 2.

Demo play uses the same loader with `demo/demorpy%d.rpy` (12 §2.4).

### 3.9 Parser results (VERIFIED)

`tools/rpy.py` checks:
- magic and version;
- checksum;
- sizes;
- stage and fps offsets (the first one is 0x134);
- the dummy word;
- the terminator;
- input bits ⊂ 0x1FFF;
- info constants;
- the USER length.

All 4 demos pass. They contain one stage each:
- demo0: Lunatic, Reimu & Yukari, stage 5, 11083 frames, tag "0100a";
- demo1–3: stages 4A / 3 / 2, tag "debug".

The fps bytes match 30-frame sampling: 371 bytes ≈ 11083/30 + 2.
