# 00 — Engine overview (th08.exe, "Imperishable Night. ver 1.00d")

Facts come from th08.exe via Ghidra (function names in `symbols.csv`). Build-time strings show the
source tree `d:\cygwin\home\zun\prog\th08\` and Direct3D 8 / D3DX 8 (Intel compiler, statically linked
D3DX). Marking: **VERIFIED** = read directly from code/data; **INFERRED** = interpretation.

## 1. Platform layer

| item | value | conf. |
|---|---|---|
| Window class / title | class `BASE`; title = "東方永夜抄　～ Imperishable Night. ver 1.00d" (Shift-JIS, 0x4B5C30) | VERIFIED |
| Back-buffer / logical resolution | **640 × 480**; windowed: client area 640×480 (window size adjusted by frame metrics); fullscreen: 640×480 | VERIFIED (CreateWindowExA args, viewport 0,0,640,480 every frame) |
| Graphics API | Direct3D 8 HAL device (falls back to REF on failure); 32-bit or 16-bit back buffer by config | VERIFIED (CreateDevice calls), formats INFERRED |
| Timer | `QueryPerformanceCounter/Frequency` (seconds); fallback `timeGetTime` | VERIFIED |
| Input | DirectInput (keyboard + joystick; `DIERR_INPUTLOST` handling) | VERIFIED strings |
| Audio | DirectSound: 39 `se_*.wav` effects + streamed BGM (02_bgm.md); MIDI fallback | VERIFIED |
| Config | `th08.cfg` (60 bytes) read from disk at start; `log.txt` written at exit | VERIFIED |
| Data | everything loaded from `th08.dat` (01_archive.md); `thbgm.dat` streamed from disk | VERIFIED |

### Default 3D camera (set once after device creation, `Window_InitD3D` 0x4424C0) — VERIFIED constants
- View: left-handed look-at, eye `(320, −240, −d)`, target `(320, −240, 0)`, up `(0, 1, 0)`
  where `d = 240 / tan(fov/2)` ≈ 895.7 (VERIFIED RE-Q26: 0x4A4110 is the CRT `tan` — x87 `fptan`, evaluated with the control word temporarily at 0x27F, i.e. 64-bit precision).
- Projection: left-handed perspective, FOV **π/6 (30°)**, aspect **4/3**, near **100**, far **10000**.
- Effect: a point at z = 0 with world (x, −y) maps 1:1 to screen pixel (x, y). 2D sprites are drawn
  in screen space; stage backgrounds (`.std`) use this camera with their own view changes.

## 2. Main loop (WinMain 0x4418C0, frame function `Frame_Run` 0x441E70)

Start-up order: save/disable accessibility shortcuts → `log` init → register window class →
load `th08.cfg` (also validates `thbgm.dat` header) → create window → create D3D device →
DirectInput → sound → allocate the global sprite/ANM manager (`SprtCtrlInf`, 0x2A2570 bytes) →
register the **Supervisor** task (§4) → message loop.

Message loop: `PeekMessage`; when idle: `TestCooperativeLevel`; if OK run `Frame_Run`, if the device is
lost-but-resettable, `Reset` and re-init render states.

`Frame_Run` (per call) — VERIFIED:

```
now = timer()
if now < last_now: deadline = now           # timer wrap/backwards protection
last_now = now
if now <= deadline: Sleep(0); return         # not time for a new frame yet
while deadline < now: deadline += 1/60       # 0.016666… s; late frames are dropped, not caught up
anm_manager.begin_frame()
SetViewport(0, 0, 640, 480)
r = run(calc_chain)                          # all update callbacks
sound.update()
if r == 0  -> JoinLoader; return 1 (quit) ; if r == -1 -> JoinLoader; return 2 (error)   # no draw, no Present (RE-Q30)
skip_counter += 1
if skip_counter >= config.frame_skip:        # VERIFIED cfg+0x23: 0 and 1 both draw every frame, 2 every 2nd
    BeginScene; anm_manager.reset_render_state;   # no Clear here (Stage_DrawLow / GameMgr_Calc / 0x45B020 clear)
    run(draw_chain)
    anm_manager.flush; EndScene; skip_counter = 0
Present()                                    # every executed frame; on failure Reset device
screenshot key -> snapshot/thNNN.bmp         # changelog/answers_phase7.md §6
```

- Logic rate: fixed **60 Hz**, one calc step per frame; there is no fixed-step catch-up.
- A global float game-speed multiplier (0x17CE8E0) scales per-frame motion (VERIFIED as multiplier in
  bullet/ANM movement code). Writers (VERIFIED RE-Q26, globals xref): ECL natives 18/28 (1/arg) and 29 (1.0), and
  resets to 1.0 in `GameManager_StageStart`, GM unregister, `MainMenu_OnRegister`, `Supervisor_OnRegister`. Replay
  speed does not use it (fast-forward runs extra calc passes, 15 §3).

## 3. Task system ("funcChain", `FuncChain_*` 0x43C760…0x43CF50) — VERIFIED

Two singly-linked lists of 0x20-byte nodes, sorted by ascending 16-bit priority (a new node is
inserted after existing nodes with the same priority):

| node off | type | meaning |
|---|---|---|
| 0x00 | u16 | priority |
| 0x02 | u16 | flags (bit 0 = heap-allocated node) |
| 0x04 | fn | per-frame callback(arg) |
| 0x08 | fn | on-register callback (called once when added; non-zero result is returned) |
| 0x0C | fn | on-remove callback |
| 0x10 | ptr | previous |
| 0x14 | ptr | next |
| 0x1C | ptr | argument (the subsystem object) |

- **calc chain** head at chain+0x00 (`FuncChain_AddCalc` 0x43C880), **draw chain** head at chain+0x20
  (`FuncChain_AddDraw` 0x43C960).
- Runner (`FuncChain_RunCalc` 0x43CA50 / `FuncChain_RunDraw` 0x43CB60) walks the list and interprets
  the callback's return value:

| ret | meaning |
|---|---|
| 0 | remove this node (calls on-remove), continue |
| 1 | continue with next node |
| 2 | call the same node again immediately |
| 3 | stop this frame, runner returns 1 |
| 4 | stop, runner returns 0 → program exits |
| 5 | stop, runner returns −1 → exit with error |
| 6 | (calc only) restart the chain from the head |

**Adding a node** (`FuncChain_AddCalc` 0x43C880 / `AddDraw` 0x43C960, VERIFIED RE-Q25): if the node has an
on-register callback it is called first and then cleared; **whatever it returns, the node is linked** (under the
chain lock) after the last node whose priority is ≤ the new one (so equal priorities keep insertion order); the
callback's result is returned to the caller, which decides what to do (e.g. a failed `GameManager_Register` makes the
Supervisor return 4). A node added during a chain walk is visited in the same walk if it lands after the current node.

### 3.1 Node removal and teardown (answers implementer Q2) — VERIFIED

- Removal (`FuncChain_Remove` 0x43CF50, locked wrapper 0x43CF10) unlinks exactly one node. If the node is
  static (flag bit 0 clear) its on-remove callback is called once (the pointer is cleared first). If it is
  heap-allocated (bit 0 set) the node is freed and its destructor (0x43C7D0) calls the on-remove callback.
- A calc callback returning 0 removes **only that calc node**. Draw nodes are *not* linked to it; each
  subsystem has its own unregister function that removes its calc node and all its draw nodes explicitly,
  e.g. Stage 0x409CA0 (calc prio 8 + draw 6 + draw 7). The in-game subsystems are torn down by the
  GameManager on-remove callback (0x43BE2C), which calls the unregister functions of Dialogue, Stage, Bullets,
  Player, Enemies, Effects, HUD, …; the Supervisor removes the GameManager calc+draw nodes (0x43BF6E) on
  scene changes.
- Runner return value: number of callbacks counted while walking (non-zero) on normal completion, or 1 / 0 /
  −1 for codes 3 / 4 / 5.
- **Draw runner** handles codes 0 (remove node), 2 (call again), 3, 4, 5 exactly like the calc runner;
  code 6 is *not* special in the draw runner (treated like 1: continue with next node). Only the calc runner
  restarts from the head on 6.

### 3.2 Title-screen BGM (answers implementer Q3) — VERIFIED

`MainMenu_Register` (0x47146D) adds the menu's calc node; its on-register callback (0x470A6C, runs inside
`FuncChain_AddCalc`, i.e. during the Supervisor's scene switch to the menu scene) sets up menu state and
starts a **loader thread** (`CreateThread` via 0x4486BA) running 0x470E10. That thread loads `title01.anm`
(slot 20) and `resulttext.anm` (slot 22); then, if the global at 0x17CE9E4 is 0, loads `title/title00.png`
and, if that succeeded, calls `Music_Play(slot 8, "bgm/th08_01.mid")` **unless the previous scene
(Supervisor+0x160) was 5 (result screen)**. So title music starts asynchronously as soon as the title
graphics have loaded, not on a fixed frame. (`Music_Play` maps `.mid` → `.wav`/thbgm track in WAV mode,
02_bgm.md.) The options menu (0x46837B) also re-issues the same call after the music-mode setting changes.

## 4. Subsystems and their calc / draw priorities (VERIFIED from registration calls)

Lower number runs first. Names are INFERRED from the files each one loads / its strings.

| subsystem (register fn) | evidence | calc prio | draw prio(s) | ANM slot(s) |
|---|---|---|---|---|
| Supervisor / scene manager (0x445EA7) | `scene %d -> %d`, input polling, title logo | 0 | 0, 2, 16 | 0 `text.anm`, 2 `nowloading.anm` |
| ASCII / text overlay (0x402780) | `ascii.anm`, `capture.anm` | 1 | 14, 20 | 1, 3 |
| GameManager (0x43AA5C) | per-stage setup, BGM start, `etama.anm` | 2 | 5 | — |
| ScreenEffect (0x45B8B0) | `ScreenInf` (fades/shakes; created on demand) | 3 | caller-given | — |
| MainMenu / title (0x47146D) | `title01.anm`, `bgm/th08_01.mid` | 4 | 3 | 20, 22 |
| MusicRoom (0x449254) | `musiccmt.txt`, `music00.anm` | 4 | 3 | 23 |
| Ending / staff roll (0x4296A0) | `EndingInf`, `staff01.anm`, `*.end` | 5 | 4 | 24 |
| Replay manager (0x451F90) | `ReplayInf` | 6/7 (play) or 17/18 (record) | — | — |
| Stage background (0x409B20) | `stgNbg.anm`, `stageN.std` | 8 | 6, 7 | 4 |
| Player (0x44C230) | `playerNN.anm`, `plyNNa.sht`, `player DEAD` | 9 | 9, 10 | 5 |
| EnemyManager + ECL (0x42C590) | `enemy.anm`, `stgNenm.anm`, `ecldataN.ecl` | 11 | 8, 11 | 7, 8 |
| Dialogue / face GUI (0x417F60) | `face_*.anm`, `msgN.dat` | 12 | 15 | 15–19 |
| EffectManager (0x428620) | `effNN.anm`, 653 effect slots | 13 | 12 | 9 |
| BulletManager (0x4311A0) | `etama.anm` | 14 | 13 | 6 |
| HUD / front screen (0x437AD0) | `FRScreenImplInf`, `front.anm`, `times.anm`, `stgNtxt.anm`, `loadingNN.anm`, score/bonus strings | 15 | 17 | 10, 12, 13, 14 |
| Result / score screen (0x4582A0) | `ResultSysInf`, `result00.anm` | 16 | 18 | 21, 22 |

Resulting in-game order — **calc**: Supervisor → ASCII → GameManager → Stage → Player → Enemies → Dialogue
→ Effects → Bullets → HUD (+ replay hooks at 6/7). **draw**: Supervisor(0) → Supervisor(2) → GameManager
(5) → Stage (6, 7) → Enemies (8) → Player (9, 10) → Enemies (11) → Effects (12) → Bullets (13) →
ASCII (14) → Dialogue (15) → Supervisor (16) → HUD (17) → ASCII (20).

Scene ids handled by the Supervisor (INFERRED from which subsystem each case registers): 1 = main menu,
2 = in-game, 5 = result screen, 8 = music room, 9 = ending; values 3, 6, 7, 10–12 are transitions
(e.g. replay start, next stage, retry) that re-run GameManager / Result / MainMenu registration.

ANM system: 25 slots (0–24), each holding one loaded `.anm` file (03_images.md §2.4).

## 5. Coordinate systems

| space | definition | conf. |
|---|---|---|
| Screen | 640×480 pixels, origin top-left, +y down | VERIFIED |
| Playfield viewport | screen rect **x = 32, y = 16, w = 384, h = 448** (the stage draw sets this viewport and clears it) | VERIFIED |
| Game (playfield) coords | origin = playfield top-left, x ∈ [0, 384), y ∈ [0, 448), +y down; screen = game + (32, 16) | VERIFIED for bullets (wall-bounce tests against 0/384 and 0/448; centre constants 192/224 used for mirroring) / INFERRED for all other objects |
| Angles | radians, 0 = +x, increasing clockwise on screen (because +y is down) | VERIFIED (velocity = (cos a, sin a)·speed everywhere, 08 §3; aim = atan2(dy, dx) in screen y-down coordinates) |

## 6. Per-frame data-loading model (relevant for a reimplementation)

- ANM files may be loaded asynchronously: a loader thread loads one texture entry per step while
  the main thread waits (`Anm_LoadAndWait`), so a reimplementation can load synchronously.
- Stage start loads: `stgNbg.anm` + `stageN.std` (Stage), `stgNenm.anm` + `ecldataN.ecl` (Enemies),
  `effNN.anm` (Effects), `msgN?.dat` (Dialogue), `stgNtxt.anm` (HUD), then starts BGM slot 0 from the
  `.std` header (02_bgm.md §4).
