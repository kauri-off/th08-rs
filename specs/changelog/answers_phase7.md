# Answers, Phase 7: closing the phase-6 INFERRED items

Method. Phase 6 searched only for absolute addresses, which misses writes made through a struct base
pointer. This phase used a new dirty-room tool, `re/tools/structxref.py`. It resolves every memory operand
whose base register holds a known constant. The constant can come from an immediate, an `ebp` stack slot,
`this` taken from the call sites, a pushed pointer argument, `rep stos`/`rep movs`, or a register index on an
absolute displacement. The tool iterates over the call graph until nothing changes. Each item below was
then confirmed in the disassembly.

One mechanism matters for several items. When `GameMgr_IsTampered` 0x40BB80 detects a checksum mismatch in
`GameMgr_AddBombs`, `AddLives`, `AddPower`, `AddDeathCount` or `AddBombCount`, the game fills the whole
Supervisor (0x17CE758, 0xD9 dwords) with 0xFFFFFFFF. That sets every Supervisor flag. A port that never
tampers can ignore this path. It is the only way some of the bits below ever become 1.

## 1. Replay fps flag u32 0x17CEA90: always 0 — VERIFIED

The address is Supervisor+0x338. The only store is in 0x470DD5, called from `MainMenu_OnRegister`. It
zeroes +0x338, +0x340, +0x344, +0x348 and +0x34C. The only reader is `Replay_RecordCalc`. So the 0x80 bit
of the recorded fps byte is always 0 (15 §3.2 corrected).

## 2. Supervisor+0x1A4 (0x17CE8FC) bit 3: never set — VERIFIED

No code sets this bit. These are all the stores to the word:

| bit | set by | cleared by |
|---|---|---|
| 0x1 | — | `Window_InitD3D` (2 sites) |
| 0x2 | `Window_InitD3D` 0x442634 (screen-capture support, 12 §3) | — |
| 0x4 | `Window_InitD3D` 0x442A1F | `Window_InitD3D` 0x442A2F |
| 0x10 | WinMain 0x441B7D | WinMain 0x441B2E |
| 0x20 | ECL native 29 (05 §12.6) | — |
| 0x40 | Supervisor ctor 0x445B7A; 0x443420 (start-up) | 0x443420 |
| 0x80 | 0x442390 (window proc); `Supervisor_Init` | — |
| 0x100 | Supervisor ctor | `Supervisor_Init` |

The only way bit 3 becomes 1 is the anti-tamper fill. So in a clean game, `Supervisor_NoRecordFlag` is
false. Updated in 15 §2.2 and §3.2, and in 16 §2.

## 3. Byte 0x164D0B2 (GM+0x3DBAA) — VERIFIED

| writer | value |
|---|---|
| `MainMenu_Top` 0x467BD6 (attract demo start) | shot % 2 from the demo header. demorpy1 and demorpy3 have shots 1 and 3, so the value is 1 for them. |
| `MainMenu_Character` 0x46AB76 (character confirmed) | 0 |
| `Replay_PlayStageLoad` 0x452EB6 | 0 |

Note: before 0x467BD6, the demo code first stores shot / 2 in `g_ShotType` and then overwrites it with
shot. Only the final value matters.

Readers:
- `GameManager_StageStart` 0x43AC58 writes GM+0x3DBAB = shot + byte. GM+0x3DBAB has no reader.
- `Result_NameEntry` 0x456288 uses the byte as an index offset.
- `Result_OnRegister` 0x459CEC and 0x459D26 (practice result, PSCR index) use it the same way.

The result screens are reached only after a game started through character select (0, as above). So the byte is 0 whenever it matters
(INFERRED for one edge case: spell practice started right after a shot-1/3 demo skips character select, but
it does not appear to reach these result paths). The port
should still set shot % 2 at demo start, to keep the state identical.

## 4. CLRD[shot]+0x20 (0x164B9B8 + 0x24·shot) — VERIFIED

The only code access is in `Ending_Init`: a read at 0x429C22, then a store of 0 at 0x429C3C. Every other
write to the CLRD array is one of these:
- a mask OR (`GameMgr_Calc`, `Ending_Init`, `Result_CheatUnlock`);
- the file load (`ScoreDat_GetCLRD`, a copy of 9 dwords);
- the bulk copy in `GameMgr_LoadScoreData`.

So a nonzero value can only come from score.dat. The game uses it once (skip allowed) and then clears it.

## 5. cfg+0x26 (Supervisor+0x13E): joystick "shot also focuses" — VERIFIED

Its gameplay reader is 0x43EF50, called only from `Input_PollJoystick` 0x43D120. That function runs on both
paths: winmm `joyGetPosEx`, and DirectInput when a device exists.

**When cfg+0x26 ≠ 0.** Counter u16 0x164D51C, updated once per poll:
- shot button down:
  - counter += 1 if it is < 20;
  - if counter > 9, add Focus (0x4).
- shot button up:
  - if counter < 11, set counter = 0;
  - otherwise counter −= 10 and add Focus.

**Effect.** Holding the pad's shot button for 10 polls also focuses. After release, focus stays for exactly
one more poll (counter 11..20 → 1..10, then 0). Keyboard input is unaffected.

**Rest of the joystick mapping** (the same for both paths):
- Each pad button index from cfg +0x00..+0x10 maps to bits 1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80, 0x100.
  - An index < 0 means "not mapped".
  - winmm: the button is down when `dwButtons` bit n is set.
  - DirectInput: the button is down when `rgbButtons[n] & 0x80`.
- Directions, winmm: thresholds are centre ± range/4 per axis. They use the JOYCAPS read at 0x164D388
  (wXmin/wXmax at 0x164D3AC/0x164D3B0, wYmin/wYmax at 0x164D3B4/0x164D3B8):
  - x > centre + q → Right 0x80; x < centre − q → Left 0x40;
  - y > centre + q → Down 0x20; y < centre − q → Up 0x10.
- Directions, DirectInput (DIJOYSTATE): uses the thresholds cfg +0x18 (X) and +0x1A (Y), default 600:
  - lX > tx → Right; lX < −tx → Left;
  - lY > ty → Down; lY < −ty → Up.

## 6. Frame skip, fps meter and screenshots — VERIFIED

### Frame skip (`Frame_Run` 0x441E70)

An i8 counter at this+0x10 is incremented after every calc step. When the counter is ≥ cfg+0x23, the draw
chain runs and the counter resets to 0. So:
- values 0 and 1 both draw every frame;
- value 2 draws every 2nd frame.

`Present` runs on every executed frame, drawn or not. `Frame_Run` does not clear the screen. The clears are
in `Stage_DrawLow` (target, black), `GameMgr_Calc` (z-buffer + fog colour) and 0x45B020 (target + z). 00 §2
and 15 §1.1 are corrected.

### FPS meter (0x446F53)

It is called each frame (from 0x445BC0). `GameManager_StageStart` calls it with this = 0 to reset it.

1. Frames counted += 1 + cfg+0x23.
2. Every ≥ 500 ms it computes fps = frames / seconds and resets the count.
   - It uses timeGetTime, or QueryPerformanceCounter when a frequency is known.
   - Every 0.5 s of QPC ticks counts as a period.
3. It formats "%.02ffps".
4. During gameplay (flags bit 2), it updates the slowdown accumulators (15 §2.11):
   - B += 60;
   - A += 60·w, with w = 1 if fps > 54, 0.8 if > 42, 0.6 if > 30, else 0.5.
5. When not in a replay, u8 fps_now 0x17CE8F0 = trunc(fps). This value is recorded in replays.
6. In a replay it prints the recorded fps_now with "%2d".

**Drawing**, unless 0x17CE8D0 (ending active) is set, through `Ascii_QueueString`:
- "NN.NNfps" at (512, 464);
- in replay playback during gameplay, the recorded value at (384, 448):
  - colour 0xFFFFFFD0, or 0xFFFF4040 when 0x17CEA94 is set;
  - the colour is reset to white afterwards.

### Screenshots (`Frame_Present` 0x442060)

1. After `Present`: on a new press of raw input bit 0x800 (Home or P), create the directory `snapshot`.
2. Find the first free name `snapshot/th000.bmp` … `th999.bmp`.
3. 0x44748F writes the BMP:
   - 14-byte file header + 40-byte BITMAPINFOHEADER: 640×480, 24 bpp, BI_RGB, bottom-up.
   - Pixel offset 0x36. File size = 0x36 + 0x780·480.
   - Pixels are read from the back buffer: X8R8G8B8 (format 0x16) is copied as B, G, R. Format 0x17
     (R5G6B5) has its own branch, not decoded; the port can simply write 24-bit.

## 7. Ending fade quad colour — VERIFIED

`Ending_DrawFade` 0x428910 calls `Screen_FillRect` 0x45B1E0 with:
- ECX = rect {0, 0, 640, 480};
- EDX = this+0x2AA4.

`Screen_FillRect` copies EDX into the diffuse colour of all 4 vertices: XYZRHW|DIFFUSE, z = 0, rhw = 1,
triangle strip of 2. Render state:
- stage 0 colour/alpha = SELECTARG1(DIFFUSE);
- ZWRITEENABLE = 0 unless cfg+0x38 bit 6;
- DESTBLEND = INVSRCALPHA.

Afterwards it restores MODULATE(TEXTURE, DIFFUSE) and invalidates the ANM state caches. 16 §1.5 is updated.
Note: symbols.csv has two names for 0x45B1E0 (`ScreenEffect_DrawRect`, `Screen_FillRect`); both are
correct.

## 8. Natives 3 / 6 / 20: "intent" — closed

answers_impl9 §6 already lists every draw step and render state as VERIFIED. Those steps fully determine the
picture once the port keeps a depth buffer:
- the black square frame is drawn with z-write at z 0.8;
- the mirrored copies are drawn at z 0.7 / 0.6 with ZFUNC LESSEQUAL.

The pixels follow from the depth test, so no verbal description of the intent is needed. The wording in
answers_impl9 §6 is updated.

## 9. Spec changes in this phase

| spec | change |
|---|---|
| 00 §2 | frame skip VERIFIED (0 and 1 = every frame); no Clear in Frame_Run; screenshot pointer |
| 10 §8 | Replay_FastForward purpose → 15 §3.3 |
| 11 §2 | 0x164D0B2 semantics |
| 12 §2.2 | demo sets 0x164D0B2 = shot % 2 |
| 15 §1.1 | +0x23 frame skip semantics corrected; +0x26 = joystick shot-focus (was "unused") |
| 15 §2.2, §2.6, §3.2, §3.5 | Supervisor+0x1A4 bit 3, 0x164D0B2, 0x17CEA90, info+0x69 now VERIFIED |
| 16 §1.2, §1.5, §2 | CLRD+0x20, fade quad colour, bit 3 |
| answers_impl9 §6 | intent note |

## 10. Tools added

- `re/tools/structxref.py`:
  - `build` takes about 10 s and writes `re/extracted/structxref.tsv`;
  - `q <lo> [hi] [W]` queries it;
  - the rw column suffixes are `a` = absolute, `i` = indexed, `Wrep` = rep store (size −1 = unknown).
- `re/tools/disx.py <va> [before:after]`: capstone disassembly. Do not name it `dis.py`, which shadows the
  stdlib module.
- `re/tools/coverage.py` writes `re/extracted/coverage.tsv`. It lists, per function: whether it is reachable
  from WinMain or a code/data pointer, and which specs cite it or its callers. It feeds COVERAGE.md.
